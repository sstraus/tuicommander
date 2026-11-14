//! Agent hook side channel (#2749-bee7).
//!
//! Generated agent hooks post their OSC 7770 frames to the local TUIC socket
//! instead of the tty the agent TUI redraws, where a split write printed them
//! over the screen (#2747-636f). The frames go through the reader's own
//! `ChunkProcessor`, between two PTY reads, so every receiver stays unchanged.
//!
//! Order (#2632-97ef): hooks are separate processes, and one the agent gave up
//! on can land after the turn settled. A post stamped with the hook's
//! process-start reading of [`tuic_ipc::hook_clock`] is refused when it started
//! before this PTY generation or before the last settlement it would undo, or
//! when the same `(pid, stamp)` was already applied. Unstamped posts (the
//! POSIX hooks) keep their arrival order.

use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;

use super::{ChunkProcessor, SilenceState};
use crate::state::AppState;

/// The verbs a generated hook sends. Anything else is not a hook frame.
const HOOK_VERBS: [&str; 5] = [
    "state",
    "claude-hook",
    "grok-hook",
    "codex-session",
    "codex-tool",
];

/// The PTY reader's processor, shared with the local API.
pub(crate) struct HookInbox {
    processor: Arc<Mutex<ChunkProcessor>>,
    silence: Arc<Mutex<SilenceState>>,
    order: Mutex<HookOrder>,
}

/// A stamped post: the hook's process-start clock reading and its pid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HookStamp {
    pub(crate) at: u64,
    pub(crate) pid: u32,
}

/// Stamped deliveries remembered to refuse a repeat: well above the hooks one
/// agent event fans out to.
const RECENT_STAMPS: usize = 64;

/// The ordering facts of one PTY generation.
struct HookOrder {
    /// Clock reading when this PTY reader started.
    generation: u64,
    /// Stamp of the last applied post that settled the turn (`state=idle`).
    settled: Option<u64>,
    recent: VecDeque<HookStamp>,
}

impl HookOrder {
    fn new(generation: u64) -> Self {
        Self {
            generation,
            settled: None,
            recent: VecDeque::with_capacity(RECENT_STAMPS),
        }
    }

    /// Whether a post with `stamp` may still change this session.
    fn admits(&self, stamp: HookStamp) -> bool {
        stamp.at >= self.generation
            && self.settled.is_none_or(|settled| stamp.at > settled)
            && !self.recent.contains(&stamp)
    }

    fn record(&mut self, stamp: HookStamp, settles: bool) {
        if settles {
            self.settled = Some(self.settled.map_or(stamp.at, |at| at.max(stamp.at)));
        }
        if self.recent.len() == RECENT_STAMPS {
            self.recent.pop_front();
        }
        self.recent.push_back(stamp);
    }
}

/// Whether `frames` settle the turn.
fn settles(frames: &str) -> bool {
    ["\x1b]7770;state=idle\x1b\\", "\x1b]7770;state=idle\x07"]
        .iter()
        .any(|frame| frames.contains(frame))
}

impl HookInbox {
    /// Share `processor` as the hook inbox of `session_id` and return it for
    /// the reader, which locks it for each chunk it processes.
    pub(super) fn register(
        state: &AppState,
        session_id: &str,
        processor: ChunkProcessor,
        silence: Arc<Mutex<SilenceState>>,
    ) -> Arc<Mutex<ChunkProcessor>> {
        let processor = Arc::new(Mutex::new(processor));
        // Bound before the child started; a reader without a binding starts
        // its generation now.
        let generation = state
            .session_maps
            .hook_generations
            .remove(session_id)
            .map_or_else(tuic_ipc::hook_clock::now, |(_, at)| at);
        state.session_maps.hook_inboxes.insert(
            session_id.to_owned(),
            Arc::new(Self {
                processor: processor.clone(),
                silence,
                order: Mutex::new(HookOrder::new(generation)),
            }),
        );
        processor
    }
}

/// What happened to a posted hook body.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum HookDelivery {
    Delivered,
    /// No live PTY reader is bound to that `TUIC_SESSION`.
    UnknownSession,
    /// The body is not only complete OSC 7770 hook frames.
    Rejected,
    /// The stamped hook started before this PTY generation or the settlement
    /// it would undo, or was already applied. Nothing was processed.
    Stale,
}

/// Accept only complete `ESC ] 7770 ; <hook verb>=<printable ASCII>` frames,
/// ended by ST or BEL, so a post can never put text or another escape sequence
/// on the terminal grid.
fn hook_frames_only(body: &str) -> bool {
    let mut rest = body;
    while !rest.is_empty() {
        let Some(frame) = rest.strip_prefix("\x1b]7770;") else {
            return false;
        };
        let Some(end) = frame.find(|c: char| !(' '..='~').contains(&c)) else {
            return false;
        };
        let (content, tail) = frame.split_at(end);
        let Some(tail) = tail
            .strip_prefix("\x1b\\")
            .or_else(|| tail.strip_prefix('\x07'))
        else {
            return false;
        };
        if !content
            .split_once('=')
            .is_some_and(|(verb, _)| HOOK_VERBS.contains(&verb))
        {
            return false;
        }
        rest = tail;
    }
    !body.is_empty()
}

/// Process the hook frames that the agent of `tuic_session` posted, as if its
/// reader had read them, then return. The hook waits for this answer, so the
/// next hook of the same agent is processed after this one.
pub(crate) fn deliver_hook_frames(
    state: &Arc<AppState>,
    tuic_session: &str,
    body: &str,
    stamp: Option<HookStamp>,
) -> HookDelivery {
    if !hook_frames_only(body) {
        return HookDelivery::Rejected;
    }
    let Some(session_id) = state
        .session_maps
        .live_pty_by_tuic_session
        .get(tuic_session)
        .map(|entry| entry.value().clone())
    else {
        return HookDelivery::UnknownSession;
    };
    let Some(inbox) = state
        .session_maps
        .hook_inboxes
        .get(&session_id)
        .map(|entry| entry.value().clone())
    else {
        return HookDelivery::UnknownSession;
    };
    let mut processor = inbox.processor.lock();
    // Checked and recorded under the processor lock, so two posts of one
    // session are ordered against each other and against the frames applied.
    if let Some(stamp) = stamp {
        let mut order = inbox.order.lock();
        if !order.admits(stamp) {
            return HookDelivery::Stale;
        }
        order.record(stamp, settles(body));
    }
    processor.process_hook_frames(body, &inbox.silence, &session_id, state);
    let flush = std::mem::take(&mut processor.queued_idle_flush);
    drop(processor);
    // A hook event is agent activity even when the terminal stays silent.
    let ended_stall = state.agent_reports.health.lock().restart(&session_id);
    if ended_stall {
        state.republish_session_state(&session_id);
    }
    if flush {
        super::flush_pending_injections(state, &session_id);
    }
    HookDelivery::Delivered
}

#[cfg(test)]
mod tests {
    use super::hook_frames_only;

    // Catches: the route feeds arbitrary bytes to the grid, so any local
    // process could print text or send escape sequences into an agent tab.
    #[test]
    fn only_complete_hook_frames_are_accepted() {
        for accepted in [
            "\x1b]7770;state=idle\x1b\\",
            "\x1b]7770;claude-hook=tab:42:eyJ9\x1b\\\x1b]7770;claude-hook=tab:42:\x1b\\\x1b]7770;state=busy\x1b\\",
            "\x1b]7770;grok-hook=tab:eyJ9\x07",
        ] {
            assert!(hook_frames_only(accepted), "{accepted:?}");
        }
        for rejected in [
            "",
            "hello",
            "\x1b]7770;state=idle\x1b\\visible text",
            "\x1b]7770;state=idle",
            "\x1b]7770;intent=x\x1b\\",
            "\x1b]0;title\x1b\\",
            "\x1b]7770;state=idle\x1b[2J\x1b\\",
            "\x1b]7770;state=id\nle\x1b\\",
        ] {
            assert!(!hook_frames_only(rejected), "{rejected:?}");
        }
    }
}
