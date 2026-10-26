//! Flow control for grid-frame delivery, on both transports.
//!
//! A grid frame is usually a DELTA — only the rows alacritty marked dirty — so
//! the client's row map is a stateful reassembly of everything it has received.
//! Dropping one frame silently strands whatever rows it carried. Both halves of
//! this module exist because a drop mechanism was applied to that stream anyway:
//! the desktop gate could not tell a fresh ack from a late one, and the WS
//! channel skipped intermediate frames on purpose.
//!
//! The frame ticker may only send while the frontend has caught up. That was a
//! bare `AtomicBool` cleared by every ack, and it had no way to tell one frame's
//! ack from another's: after the ticker gave a frame up for lost (500 ms) and
//! sent the next one, the late ack for the abandoned frame reopened the gate and
//! a third frame went out at once — a burst delivered at exactly the moment the
//! frontend was proven to be behind.
//!
//! Counting fixes that without touching the wire format. Rust counts frames it
//! sends; the frontend counts frames it receives and echoes that total on each
//! ack. The gate is open only when the echoed total has caught up with what was
//! sent, so an ack for an abandoned frame is a number that is already in the
//! past and changes nothing.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use parking_lot::Mutex;

/// Stop retrying a desktop that has not acknowledged three abandoned frames.
pub const MAX_STUCK_BEFORE_PAUSE: u32 = 3;

#[derive(Debug, Default)]
struct Recovery {
    received: u64,
    abandoned: u32,
}

/// Hands out a fresh id to every subscription, process-wide.
static NEXT_EPOCH: AtomicU64 = AtomicU64::new(1);

/// Hands out the order a frame was cut from a grid in, process-wide.
///
/// Global rather than per session so that a session whose watch channel is
/// replaced — every spawn path installs a fresh one — cannot start behind the
/// order that channel already recorded and have every frame rejected as stale.
static NEXT_FRAME_ORDER: AtomicU64 = AtomicU64::new(1);

/// The bytes of one grid frame plus the order it was cut from the grid in.
///
/// Five producers serialize under the vt lock and hand the bytes to
/// `send_grid_frame` *after* releasing it, so the order frames reach a transport
/// is not the order they were cut: a resize can serialize a full frame, lose the
/// race to a delta cut later, and land last — repainting the client with the
/// older screen, which is the "blank after zoom" the resize flush exists to
/// prevent. The order is stamped inside the critical section that produced the
/// bytes, because that is the only place it exists.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GridFrame {
    pub order: u64,
    pub bytes: Vec<u8>,
}

impl GridFrame {
    /// Wrap freshly serialized bytes, claiming the next order for them. Call
    /// while the vt lock that produced `bytes` is still held.
    pub fn cut(bytes: Vec<u8>) -> Self {
        Self {
            order: NEXT_FRAME_ORDER.fetch_add(1, Ordering::Relaxed),
            bytes,
        }
    }

    /// Nothing changed, so there is nothing to send.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

/// Where a frame stands against the newest one already handed to a transport.
#[derive(Debug, PartialEq, Eq)]
pub enum FrameOrder {
    /// Nothing newer has been delivered; this frame may go out.
    Newest,
    /// A frame cut *after* this one was already delivered. This one carries rows
    /// that newer frame does not — the damage behind them was consumed when it
    /// was serialized — so it can neither be sent nor silently dropped.
    Stale,
}

/// Frame-delivery gate for one session with real-receipt recovery accounting.
#[derive(Debug)]
pub struct GridGate {
    /// Identifies the subscription this gate belongs to. Acks name it, so a
    /// message from the previous subscription cannot be mistaken for a fresh one.
    epoch: u64,
    /// Frames handed to the channel since this gate was installed.
    sent: AtomicU64,
    /// Frames the frontend has reported receiving, clamped to `sent`.
    acked: AtomicU64,
    /// Set when a frame was withheld from this channel because the gate was
    /// closed while the WebSocket transport kept receiving. See
    /// [`Self::note_missed`].
    missed: AtomicBool,
    /// Real receipt progress must not be advanced by synthetic abandonment.
    recovery: Mutex<Recovery>,
}

impl GridGate {
    pub fn new() -> Self {
        Self {
            epoch: NEXT_EPOCH.fetch_add(1, Ordering::Relaxed),
            sent: AtomicU64::new(0),
            acked: AtomicU64::new(0),
            missed: AtomicBool::new(false),
            recovery: Mutex::new(Recovery::default()),
        }
    }

    /// The id of the subscription this gate serves.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// A frame just went on the wire.
    pub fn mark_sent(&self) {
        self.sent.fetch_add(1, Ordering::Relaxed);
    }

    /// The frontend reports `received` frames delivered in total, for the
    /// subscription `epoch` names.
    ///
    /// An ack from a previous subscription is dropped outright: the frontend
    /// restarts its count at zero on resubscribe, so an old ack still in flight
    /// carries a number that means nothing here — and clamping it to what this
    /// gate has sent would credit exactly the frame still waiting to be painted.
    /// Within the right epoch the value is monotonic and clamped, so a reordered
    /// or duplicated ack can neither rewind the gate nor open it for frames that
    /// were never sent.
    pub fn ack(&self, epoch: u64, received: u64) -> bool {
        if epoch != self.epoch {
            return false;
        }
        let mut recovery = self.recovery.lock();
        let sent = self.sent.load(Ordering::Relaxed);
        let bounded = received.min(sent);
        self.acked.fetch_max(bounded, Ordering::Relaxed);
        let progressed = bounded > recovery.received;
        if progressed {
            recovery.received = bounded;
            recovery.abandoned = 0;
        }
        progressed
    }

    /// True when no frame is outstanding, i.e. the ticker may send.
    pub fn is_open(&self) -> bool {
        !self.is_paused() && self.acked.load(Ordering::Relaxed) >= self.sent.load(Ordering::Relaxed)
    }

    /// Give up on every outstanding frame (the frontend missed its deadline).
    ///
    /// This is what makes a late ack harmless: the gate moves forward to what was
    /// actually sent, so the ack that arrives afterwards is stale by definition.
    pub fn abandon(&self) {
        let mut recovery = self.recovery.lock();
        let sent = self.sent.load(Ordering::Relaxed);
        if self.acked.fetch_max(sent, Ordering::Relaxed) < sent {
            recovery.abandoned = recovery.abandoned.saturating_add(1);
        }
    }

    pub fn abandoned_count(&self) -> u32 {
        self.recovery.lock().abandoned
    }

    pub fn is_paused(&self) -> bool {
        self.abandoned_count() >= MAX_STUCK_BEFORE_PAUSE
    }

    /// A visible viewport explicitly requests authoritative state. Allow a new
    /// delivery attempt without pretending that the frontend acknowledged one.
    pub fn resume_for_full_frame(&self) {
        let mut recovery = self.recovery.lock();
        recovery.abandoned = 0;
        self.acked
            .fetch_max(self.sent.load(Ordering::Relaxed), Ordering::Relaxed);
    }

    /// A frame was serialized and delivered to the WebSocket subscribers while
    /// this gate was closed, so it never reached this channel.
    ///
    /// The gate used to stop the frame ticker outright, which held the damage on
    /// the vt until the WebView caught up — lossless for the desktop, and it
    /// starved every browser subscriber on the other transport. They keep
    /// receiving now, which means the rows in those frames left the shared damage
    /// without reaching this channel. A delta on top of a row map missing them
    /// would leave stale rows with no error, so the debt is recorded here and
    /// paid with a full frame once the gate reopens.
    pub fn note_missed(&self) {
        self.missed.store(true, Ordering::Relaxed);
    }

    /// Take the debt [`Self::note_missed`] recorded, if any. True exactly once
    /// per stall: the caller owes this subscription one full frame.
    pub fn take_missed(&self) -> bool {
        self.missed.swap(false, Ordering::Relaxed)
    }

    /// Frames sent and not yet reported as received — for diagnostics.
    pub fn outstanding(&self) -> u64 {
        self.sent
            .load(Ordering::Relaxed)
            .saturating_sub(self.acked.load(Ordering::Relaxed))
    }
}

impl Default for GridGate {
    fn default() -> Self {
        Self::new()
    }
}

/// Latest frame published to WebSocket clients, tagged with a sequence number.
///
/// The app's watch adapter keeps only the newest value. This sequence lets a
/// reader detect skipped deltas and request a full frame.
#[derive(Clone, Debug, Default)]
pub struct GridWatchFrame {
    pub seq: u64,
    /// Highest [`GridFrame::order`] handed to any of this session's transports.
    /// Distinct from `seq`, which counts what this channel published: a frame
    /// can be delivered to the desktop and to nobody here, and a reader that
    /// treated the two as one number would see a gap where none exists.
    pub order: u64,
    pub frame: Vec<u8>,
}

/// Claim `order` and update the latest frame when bytes are present.
///
/// One critical section on purpose. Claiming the order and publishing the bytes
/// separately reopens exactly the window this closes: two producers could claim
/// in the order they serialized and then publish in the other one.
///
/// `frame` is `None` when nobody is watching. The order is still claimed,
/// because the desktop channel and this watch are two transports for one grid
/// and one damage set, so they share one ordering.
pub fn claim_grid_frame(
    slot: &mut GridWatchFrame,
    order: u64,
    frame: Option<Vec<u8>>,
) -> FrameOrder {
    if order <= slot.order {
        return FrameOrder::Stale;
    }
    slot.order = order;
    if let Some(bytes) = frame {
        slot.seq += 1;
        slot.frame = bytes;
    }
    FrameOrder::Newest
}

/// Free the retained frame after the last grid client leaves.
///
/// A `watch` keeps its last value for the life of the channel, and the channel
/// lives as long as the session — so a browser that watched a 110 KB grid once
/// pinned those bytes until the PTY closed. Nothing needs them: a client that
/// reconnects is answered with a fresh full frame.
///
/// The sequence is deliberately left alone. It is what a reconnecting reader
/// compares against to tell a delta from a gap, and rewinding it would make the
/// next real frame look like a replay.
pub fn release_grid_frame(slot: &mut GridWatchFrame) {
    slot.frame.clear();
}

/// Did the watch channel drop frames between `last_seq` and `seq`?
///
/// Consecutive means nothing was lost. The same sequence twice means the reader
/// was woken without a new value, which loses nothing either. Anything else — a
/// skip forward, or a sequence that went backwards because the channel was
/// replaced under the reader — means at least one delta went missing and the
/// client's row map can no longer be trusted, so the reader must send a full
/// frame rather than the delta it just picked up.
pub fn watch_dropped_frames(last_seq: u64, seq: u64) -> bool {
    seq != last_seq && seq != last_seq + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Catches synthetic abandonment being mistaken for real frontend progress:
    /// the ticker's next open-gate tick used to reset its stall counter to zero.
    #[test]
    fn repeated_unacknowledged_frames_back_off() {
        let gate = GridGate::new();
        for _ in 0..3 {
            assert!(gate.is_open());
            gate.mark_sent();
            gate.abandon();
        }
        assert!(
            !gate.is_open(),
            "three abandoned frames without a real ack must pause desktop emission"
        );
    }

    #[test]
    fn a_fresh_gate_is_open() {
        let gate = GridGate::new();
        assert!(gate.is_open());
        assert_eq!(gate.outstanding(), 0);
    }

    #[test]
    fn sending_closes_the_gate_and_an_ack_for_that_frame_opens_it() {
        let gate = GridGate::new();
        gate.mark_sent();
        assert!(!gate.is_open());
        assert_eq!(gate.outstanding(), 1);

        gate.ack(gate.epoch(), 1);
        assert!(gate.is_open());
        assert_eq!(gate.outstanding(), 0);
    }

    /// The regression this type exists for. Old behaviour: force-reset → send
    /// frame 2 → late ack for frame 1 clears the flag → frame 3 goes out at once.
    #[test]
    fn a_late_ack_for_an_abandoned_frame_does_not_open_the_gate() {
        let gate = GridGate::new();
        gate.mark_sent(); // frame 1
        gate.abandon(); // ticker waited 500 ms and gave up
        assert!(gate.is_open(), "the ticker must be able to send frame 2");

        gate.mark_sent(); // frame 2
        assert!(!gate.is_open());

        gate.ack(gate.epoch(), 1); // the frontend finally reports frame 1
        assert!(
            !gate.is_open(),
            "frame 2 is still outstanding; opening here is the burst bug"
        );

        gate.ack(gate.epoch(), 2);
        assert!(gate.is_open());
    }

    /// A frontend that painted several frames in one turn reports the total, not
    /// one ack per frame.
    #[test]
    fn one_ack_can_clear_several_frames() {
        let gate = GridGate::new();
        for _ in 0..4 {
            gate.mark_sent();
        }
        assert_eq!(gate.outstanding(), 4);
        gate.ack(gate.epoch(), 4);
        assert!(gate.is_open());
    }

    #[test]
    fn an_ack_never_rewinds_the_gate() {
        let gate = GridGate::new();
        gate.mark_sent();
        gate.mark_sent();
        gate.ack(gate.epoch(), 2);
        gate.ack(gate.epoch(), 1); // reordered/duplicated ack
        assert!(gate.is_open());
    }

    /// The frontend resets its counter when it (re)subscribes and Rust installs a
    /// fresh gate; an ack already in flight from the old channel carries a huge
    /// number that must not open the new gate.
    #[test]
    fn an_ack_cannot_run_ahead_of_what_was_sent() {
        let gate = GridGate::new();
        gate.ack(gate.epoch(), 57);
        gate.mark_sent();
        assert!(
            !gate.is_open(),
            "a stale ack from a previous subscription must not credit a new frame"
        );
        gate.ack(gate.epoch(), 1);
        assert!(gate.is_open());
    }

    /// Clamping alone is not enough once the NEW gate has sent something: an ack
    /// from the previous subscription (already in flight when the frontend
    /// resubscribed) would clamp to that frame's number and credit it, releasing
    /// the next frame while the first is still unpainted. The epoch is what tells
    /// the two subscriptions apart.
    #[test]
    fn an_ack_from_a_previous_subscription_is_ignored() {
        let old = GridGate::new();
        let new = GridGate::new();
        assert_ne!(
            old.epoch(),
            new.epoch(),
            "each subscription gets its own id"
        );

        new.mark_sent();
        new.ack(old.epoch(), 57);
        assert!(
            !new.is_open(),
            "an ack addressed to the previous subscription credited this one"
        );

        new.ack(new.epoch(), 1);
        assert!(new.is_open());
    }

    #[test]
    fn epochs_keep_increasing() {
        let first = GridGate::new().epoch();
        let second = GridGate::new().epoch();
        let third = GridGate::new().epoch();
        assert!(first < second && second < third);
    }

    #[test]
    fn consecutive_watch_frames_are_not_a_drop() {
        assert!(!watch_dropped_frames(0, 1));
        assert!(!watch_dropped_frames(41, 42));
    }

    #[test]
    fn a_skipped_watch_frame_is_a_drop() {
        // The client saw 41 and the channel handed it 43: frame 42's dirty rows
        // exist nowhere else, so the row map is now missing them.
        assert!(watch_dropped_frames(41, 43));
        assert!(watch_dropped_frames(0, 7));
    }

    /// A `watch` receiver can be woken without a new value (a spurious
    /// `changed()`); re-reading the same sequence is not a drop and must not cost
    /// a full frame.
    #[test]
    fn the_same_watch_frame_twice_is_not_a_drop() {
        assert!(!watch_dropped_frames(42, 42));
    }

    /// The sequence can only go backwards if the reader is looking at a channel
    /// that was replaced under it. Whatever it holds is then unrelated to what is
    /// arriving, which is exactly the case a full frame repairs.
    #[test]
    fn a_sequence_that_went_backwards_is_a_drop() {
        assert!(watch_dropped_frames(42, 3));
        assert!(watch_dropped_frames(42, 0));
    }

    // --- Frame ordering (670-b9a2) ---
    //
    // Five producers serialize under the vt lock and publish after releasing it,
    // so the order frames reach a transport is not the order they were cut. The
    // resize path is the one that hurts: it serializes a FULL frame, and a full
    // frame that lands after a delta repaints the client with the older screen —
    // the "blank after zoom" the resize flush exists to prevent. Nothing about
    // the arrival order is observable at the transport, which is why the order is
    // stamped at the only place it exists: inside the critical section that cut
    // the bytes.

    #[test]
    fn a_frame_cut_earlier_cannot_overwrite_one_cut_later() {
        let mut tx = GridWatchFrame::default();
        // Two producers, both still holding their own vt critical section.
        let resize = GridFrame::cut(vec![0xAA; 4]);
        let delta = GridFrame::cut(vec![0xBB; 2]);
        assert!(resize.order < delta.order, "cut order must be recorded");

        // Both released the lock; the one cut LAST wins the race to the channel,
        // then the older one arrives.
        assert_eq!(
            claim_grid_frame(&mut tx, delta.order, Some(delta.bytes.clone())),
            FrameOrder::Newest
        );
        assert_eq!(
            claim_grid_frame(&mut tx, resize.order, Some(resize.bytes)),
            FrameOrder::Stale
        );

        assert_eq!(
            tx.frame, delta.bytes,
            "a frame cut before the one already published repainted the newer screen"
        );
    }

    #[test]
    fn a_rejected_frame_does_not_move_the_sequence() {
        let mut tx = GridWatchFrame::default();
        let older = GridFrame::cut(vec![1]);
        let newer = GridFrame::cut(vec![2]);
        claim_grid_frame(&mut tx, newer.order, Some(newer.bytes));
        let seq_after_newer = tx.seq;

        claim_grid_frame(&mut tx, older.order, Some(older.bytes));

        // The sequence counts what this channel published. Moving it for a frame
        // that was never published would show every reader a gap that did not
        // happen, and cost each of them a full-frame resync to close it.
        assert_eq!(tx.seq, seq_after_newer);
    }

    #[test]
    fn claiming_without_publishing_still_orders_the_next_frame() {
        let mut tx = GridWatchFrame::default();
        let older = GridFrame::cut(vec![1]);
        let newer = GridFrame::cut(vec![2]);

        // Nobody was watching when the newer frame went out, so no bytes went
        // into the slot — but the desktop channel and this watch share one grid
        // and one damage set, so they share one ordering. A browser that connects
        // in between must not be handed the older frame as if it were current.
        assert_eq!(
            claim_grid_frame(&mut tx, newer.order, None),
            FrameOrder::Newest
        );
        assert_eq!(tx.seq, 0, "nothing was published");

        assert_eq!(
            claim_grid_frame(&mut tx, older.order, Some(older.bytes)),
            FrameOrder::Stale,
            "an order claimed by the other transport must still reject an older frame"
        );
        assert_eq!(tx.frame, [] as [u8; 0]);
    }

    #[test]
    fn a_frame_cut_later_is_admitted_after_one_cut_earlier() {
        let mut tx = GridWatchFrame::default();
        let first = GridFrame::cut(vec![1]);
        let second = GridFrame::cut(vec![2]);

        // The ordinary case, in the ordinary order: nothing is rejected.
        assert_eq!(
            claim_grid_frame(&mut tx, first.order, Some(first.bytes)),
            FrameOrder::Newest
        );
        assert_eq!(
            claim_grid_frame(&mut tx, second.order, Some(second.bytes.clone())),
            FrameOrder::Newest
        );
        assert_eq!(tx.frame, second.bytes);
        assert_eq!(tx.seq, 2);
    }

    // --- Frames withheld from a stalled WebView (670-b9a2) ---

    #[test]
    fn a_fresh_gate_owes_nothing() {
        let gate = GridGate::new();
        assert!(!gate.take_missed());
    }

    #[test]
    fn a_missed_frame_is_owed_exactly_once() {
        let gate = GridGate::new();
        // Several frames went to the WebSocket subscribers while the WebView was
        // behind. The debt is one full frame, not one per frame skipped.
        gate.note_missed();
        gate.note_missed();

        assert!(gate.take_missed(), "the debt must be reported");
        assert!(
            !gate.take_missed(),
            "paying it twice sends a full frame nobody needs"
        );
    }

    /// Catches a late real receipt being ignored after synthetic abandonment.
    #[test]
    fn real_ack_resumes_paused_delivery_but_duplicates_do_not() {
        let gate = GridGate::new();
        for _ in 0..3 {
            gate.mark_sent();
            gate.abandon();
        }
        assert!(gate.is_paused());
        assert!(!gate.ack(gate.epoch() + 1, 3));
        assert!(gate.is_paused());
        assert!(gate.ack(gate.epoch(), 1));
        assert!(gate.is_open());
        for _ in 0..3 {
            gate.mark_sent();
            gate.abandon();
        }
        assert!(!gate.ack(gate.epoch(), 1));
        assert!(gate.is_paused());
    }

    /// Catches a show/full-frame request leaving a recovered viewport paused.
    #[test]
    fn full_frame_request_resumes_without_faking_real_receipt() {
        let gate = GridGate::new();
        for _ in 0..3 {
            gate.mark_sent();
            gate.abandon();
        }
        gate.resume_for_full_frame();
        assert!(gate.is_open());
        assert!(gate.ack(gate.epoch(), 1));
        assert!(!gate.ack(gate.epoch(), 1));
    }
}
