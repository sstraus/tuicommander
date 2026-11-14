//! The ordered record of what happened on one connection.
//!
//! A host that renders a turn needs two things a snapshot cannot give it: the
//! order the agent said things in, and the ability to join late without losing
//! what it missed. So every accepted callback and local settlement is stamped
//! with a sequence number here, appended, and only then fanned out.
//!
//! The journal is bounded, which means a subscriber can fall far enough behind
//! that what it asked for is gone. That is answered with [`stream_gap`] rather
//! than by resuming past the hole: the missing chunks exist nowhere in this
//! client, and a stream that quietly skipped them would render as a turn where
//! part of what the model said simply never happened. Recovery is a fresh
//! connection and `session/load`, which replays from ego, the only place that
//! actually knows.
//!
//! Nothing here ever blocks the SDK reader. Appending takes an uncontended
//! lock and a broadcast send that drops the slowest subscriber's view rather
//! than waiting for it, so one slow host cannot stall the protocol.
//!
//! [`stream_gap`]: AcpClientError::stream_gap

use std::collections::VecDeque;
use std::sync::{Arc, Weak};

use parking_lot::Mutex;
use tokio::sync::broadcast;

use super::{
    AcpClientError, AcpClientEvent, AcpConnectionId, AcpEventEnvelope, AcpEventPage, AcpNotice,
    AcpStreamFrame, AcpTurnId,
};
use agent_client_protocol::schema::v1;

/// How many events one connection keeps for subscribers that join late.
///
/// Large enough that a host reconnecting its stream within a turn keeps its
/// place, small enough that an unattended connection cannot grow without
/// bound. Falling off the end is a reported gap, not a silent loss.
const JOURNAL_CAPACITY: usize = 1024;

/// How far behind a live subscriber may fall before it is told it has a gap.
const LIVE_CAPACITY: usize = 256;

/// Events in one page when the caller does not say.
const PAGE_DEFAULT_EVENTS: usize = 256;

/// Serialized bytes in one page when the caller does not say.
const PAGE_DEFAULT_BYTES: usize = 1024 * 1024;

/// The most serialized bytes one page may carry, whatever the caller asks for.
///
/// A single event larger than this is still returned alone, so a cursor can
/// never stall behind it.
const PAGE_MAX_BYTES: usize = 4 * 1024 * 1024;

/// The first sequence number an event can carry.
///
/// Sequences start at one so that zero means "from the beginning" without
/// ambiguity with the first real event.
const FIRST_SEQUENCE: u64 = 1;

pub struct AcpEventJournal {
    connection_id: AcpConnectionId,
    generation: u64,
    /// Shared by every connection this client holds, unlike `live`.
    ///
    /// A notice is a wake signal for the whole app, so it goes on one bus that
    /// an SSE consumer subscribes to once instead of one per connection.
    notices: broadcast::Sender<AcpNotice>,
    state: Arc<Mutex<JournalState>>,
}

struct JournalState {
    /// Lives with the retained events so a lagging stream can re-subscribe and
    /// read what it missed in one step under the lock.
    live: broadcast::Sender<AcpEventEnvelope>,
    next_sequence: u64,
    retained: VecDeque<AcpEventEnvelope>,
}

impl JournalState {
    /// The oldest sequence still held, or the next one when nothing is.
    fn earliest(&self) -> u64 {
        self.retained
            .front()
            .map_or(self.next_sequence, |event| event.sequence)
    }
}

/// Events from a chosen point, backlog first and then live.
///
/// The two halves are handed over together, and the live half is subscribed
/// while the backlog is taken, so no event can slip between them.
pub struct AcpEventStream {
    backlog: std::vec::IntoIter<AcpEventEnvelope>,
    live: broadcast::Receiver<AcpEventEnvelope>,
    connection_id: AcpConnectionId,
    /// Weak, so a dropped journal still ends the stream.
    journal: Weak<Mutex<JournalState>>,
    /// The sequence after the last one handed out.
    next: u64,
}

impl AcpEventJournal {
    #[must_use]
    pub fn new(
        connection_id: AcpConnectionId,
        generation: u64,
        notices: broadcast::Sender<AcpNotice>,
    ) -> Self {
        let (live, _) = broadcast::channel(LIVE_CAPACITY);
        Self {
            connection_id,
            generation,
            notices,
            state: Arc::new(Mutex::new(JournalState {
                live,
                next_sequence: FIRST_SEQUENCE,
                retained: VecDeque::with_capacity(JOURNAL_CAPACITY),
            })),
        }
    }

    /// Stamp one event, retain it, and hand it to whoever is listening.
    ///
    /// Returns the envelope so a caller that also has to act on the event
    /// works from the same stamped copy every subscriber saw.
    pub fn append(
        &self,
        session_id: Option<v1::SessionId>,
        turn_id: Option<AcpTurnId>,
        event: AcpClientEvent,
    ) -> AcpEventEnvelope {
        let envelope = {
            let mut state = self.state.lock();
            let envelope = AcpEventEnvelope {
                connection_id: self.connection_id,
                generation: self.generation,
                sequence: state.next_sequence,
                session_id,
                turn_id,
                event,
            };
            state.next_sequence += 1;
            if state.retained.len() == JOURNAL_CAPACITY {
                state.retained.pop_front();
            }
            state.retained.push_back(envelope.clone());
            // An error here means nobody is subscribed, which is not a failure.
            // Sent under the lock so live order is sequence order, and a
            // re-subscribing stream cannot miss or repeat an event.
            let _ = state.live.send(envelope.clone());
            envelope
        };
        if let Some(notice) = AcpNotice::from_envelope(&envelope) {
            let _ = self.notices.send(notice);
        }
        envelope
    }

    /// The range a subscriber can still ask for, as the snapshot reports it.
    #[must_use]
    pub fn bounds(&self) -> (u64, u64) {
        let state = self.state.lock();
        (state.earliest(), state.next_sequence.saturating_sub(1))
    }

    /// Every event from `from` onwards, then everything that follows.
    ///
    /// `from` is the first sequence the caller wants, so a subscriber resumes
    /// with the sequence after the last one it handled. Zero means everything
    /// still held.
    pub fn subscribe(&self, from: u64) -> Result<AcpEventStream, AcpClientError> {
        let state = self.state.lock();
        // Subscribing under the lock is what makes the two halves seamless: an
        // append cannot land between the backlog being copied and the live
        // receiver existing.
        let live = state.live.subscribe();
        self.check_cursor(&state, from)?;

        let backlog: Vec<_> = state
            .retained
            .iter()
            .filter(|event| event.sequence >= from)
            .cloned()
            .collect();
        let next = backlog
            .last()
            .map_or(from.max(state.next_sequence), |event| event.sequence + 1);
        Ok(AcpEventStream {
            backlog: backlog.into_iter(),
            live,
            connection_id: self.connection_id,
            journal: Arc::downgrade(&self.state),
            next,
        })
    }

    /// The earliest sequence held, or the gap a cursor before it has fallen into.
    fn check_cursor(&self, state: &JournalState, from: u64) -> Result<u64, AcpClientError> {
        let earliest = state.earliest();
        if from != 0 && from < earliest {
            return Err(AcpClientError::stream_gap(
                self.connection_id,
                from,
                earliest,
            ));
        }
        Ok(earliest)
    }

    /// One bounded read of what the journal holds from `from` onwards.
    ///
    /// `from` has the stream's meaning: the first sequence wanted, zero for
    /// everything still held, and a dropped cursor is the same gap. `limit` and
    /// `max_bytes` default and are capped, so no request can make one response
    /// carry the whole journal. The first event is always included, however
    /// large, so `next` always moves while events remain.
    pub fn page(
        &self,
        from: u64,
        limit: Option<usize>,
        max_bytes: Option<usize>,
    ) -> Result<AcpEventPage, AcpClientError> {
        let limit = limit
            .unwrap_or(PAGE_DEFAULT_EVENTS)
            .clamp(1, JOURNAL_CAPACITY);
        let max_bytes = max_bytes
            .unwrap_or(PAGE_DEFAULT_BYTES)
            .clamp(1, PAGE_MAX_BYTES);
        // Cloned under the lock and measured outside it: serializing here
        // would hold up `append`, which the SDK reader calls.
        let (candidates, earliest, latest) = {
            let state = self.state.lock();
            let earliest = self.check_cursor(&state, from)?;
            let candidates: Vec<_> = state
                .retained
                .iter()
                .filter(|event| event.sequence >= from)
                .take(limit)
                .cloned()
                .collect();
            (candidates, earliest, state.next_sequence.saturating_sub(1))
        };

        let mut events = Vec::with_capacity(candidates.len());
        let mut bytes = 0usize;
        for event in candidates {
            let size = serde_json::to_vec(&event).map_or(0, |json| json.len());
            if !events.is_empty() && bytes.saturating_add(size) > max_bytes {
                break;
            }
            bytes = bytes.saturating_add(size);
            events.push(event);
        }
        let next = events
            .last()
            .map_or(from.max(earliest), |event: &AcpEventEnvelope| {
                event.sequence + 1
            });
        Ok(AcpEventPage {
            events,
            next,
            latest,
        })
    }
}

impl AcpEventStream {
    /// The next event, or `None` once the connection can produce no more.
    ///
    /// A subscriber that fell behind the live buffer resumes from the journal,
    /// which still holds what it missed. Only a subscriber that fell behind the
    /// journal itself is told it has a gap, for the same reason the journal
    /// refuses a cursor it no longer holds. Cancel-safe: the resume has no
    /// await point.
    pub async fn recv(&mut self) -> Option<Result<AcpEventEnvelope, AcpClientError>> {
        loop {
            if let Some(event) = self.backlog.next() {
                self.next = event.sequence + 1;
                return Some(Ok(event));
            }
            match self.live.recv().await {
                Ok(event) => {
                    self.next = event.sequence + 1;
                    return Some(Ok(event));
                }
                Err(broadcast::error::RecvError::Closed) => return None,
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    if let Err(gap) = self.resume_from_journal() {
                        return Some(Err(gap));
                    }
                }
            }
        }
    }

    /// Replace the lagged receiver with a fresh one plus the journal's copy of
    /// everything from `next`, taken together under the lock.
    fn resume_from_journal(&mut self) -> Result<(), AcpClientError> {
        let Some(journal) = self.journal.upgrade() else {
            // The journal is gone, so the missed events cannot be replayed.
            // Reading the lagged receiver on would skip them silently.
            return Err(AcpClientError::stream_gap(
                self.connection_id,
                self.next,
                self.next,
            ));
        };
        let state = journal.lock();
        let earliest = state.earliest();
        if self.next < earliest {
            return Err(AcpClientError::stream_gap(
                self.connection_id,
                self.next,
                earliest,
            ));
        }
        self.live = state.live.subscribe();
        let backlog: Vec<_> = state
            .retained
            .iter()
            .filter(|event| event.sequence >= self.next)
            .cloned()
            .collect();
        self.backlog = backlog.into_iter();
        Ok(())
    }

    /// The next frame a subscriber is sent. A gap or an end is the last one.
    ///
    /// Cancel-safe, like [`Self::recv`], so a transport can race it against
    /// its own close.
    pub async fn next_frame(&mut self) -> AcpStreamFrame {
        match self.recv().await {
            Some(Ok(envelope)) => AcpStreamFrame::Event(Box::new(envelope)),
            Some(Err(gap)) => AcpStreamFrame::Gap(gap),
            None => AcpStreamFrame::End,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn journal() -> AcpEventJournal {
        let (notices, _) = broadcast::channel(4);
        AcpEventJournal::new(AcpConnectionId::new(), 1, notices)
    }

    fn append_many(journal: &AcpEventJournal, count: usize) {
        for _ in 0..count {
            journal.append(None, None, AcpClientEvent::TurnStarted);
        }
    }

    /// Catches: a reader that lags the 256-event live broadcast being cut off
    /// with a gap while the 1024-event journal still holds what it missed.
    #[tokio::test]
    async fn a_reader_that_lags_the_live_buffer_resumes_from_the_journal() {
        let journal = journal();
        let mut stream = journal.subscribe(0).expect("subscribe");
        let total = LIVE_CAPACITY * 3;
        assert!(total < JOURNAL_CAPACITY);
        append_many(&journal, total);

        for expected in 1..=total as u64 {
            let event = stream
                .recv()
                .await
                .expect("the stream stays open")
                .unwrap_or_else(|gap| panic!("event {expected}: unexpected gap {gap:?}"));
            assert_eq!(
                event.sequence, expected,
                "events arrive in order, none lost"
            );
        }
    }

    /// Catches: a reader that fell behind the journal itself being resumed
    /// past the hole instead of told about it.
    #[tokio::test]
    async fn a_reader_that_falls_behind_the_journal_gets_the_gap() {
        let journal = journal();
        let mut stream = journal.subscribe(0).expect("subscribe");
        append_many(&journal, JOURNAL_CAPACITY + LIVE_CAPACITY);

        let gap = stream
            .recv()
            .await
            .expect("the stream reports the gap")
            .expect_err("events 1.. are gone");
        assert!(matches!(
            gap.code,
            super::super::AcpClientErrorCode::StreamGap
        ));
    }

    /// Catches: resuming after a lag duplicating or skipping the event that
    /// was delivered live just before the lag was noticed.
    #[tokio::test]
    async fn resuming_after_a_lag_keeps_following_live_events() {
        let journal = journal();
        let mut stream = journal.subscribe(0).expect("subscribe");
        append_many(&journal, LIVE_CAPACITY * 2);
        for expected in 1..=(LIVE_CAPACITY * 2) as u64 {
            assert_eq!(stream.recv().await.unwrap().unwrap().sequence, expected);
        }
        let live = journal.append(None, None, AcpClientEvent::TurnStarted);
        assert_eq!(
            stream.recv().await.unwrap().unwrap().sequence,
            live.sequence
        );
    }

    /// Catches: a reader that lags while its connection closes (the journal is
    /// dropped) being handed the tail of the closed live buffer past the hole,
    /// so it ends cleanly having silently lost the events in between.
    #[tokio::test]
    async fn a_lagging_reader_whose_journal_is_dropped_never_skips_silently() {
        let journal = journal();
        let mut stream = journal.subscribe(0).expect("subscribe");
        let total = LIVE_CAPACITY * 2;
        append_many(&journal, total);
        drop(journal);

        let mut expected = 1u64;
        loop {
            match stream.recv().await {
                Some(Ok(event)) => {
                    assert_eq!(event.sequence, expected, "an event after a hole, no gap");
                    expected += 1;
                }
                Some(Err(_gap)) => break,
                None => {
                    assert_eq!(
                        expected,
                        total as u64 + 1,
                        "ended cleanly after losing events"
                    );
                    break;
                }
            }
        }
    }
}
