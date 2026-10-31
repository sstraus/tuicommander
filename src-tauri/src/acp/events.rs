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

use parking_lot::Mutex;
use tokio::sync::broadcast;

use super::{
    AcpClientError, AcpClientEvent, AcpConnectionId, AcpEventEnvelope, AcpNotice, AcpTurnId,
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

/// The first sequence number an event can carry.
///
/// Sequences start at one so that zero means "from the beginning" without
/// ambiguity with the first real event.
const FIRST_SEQUENCE: u64 = 1;

pub struct AcpEventJournal {
    connection_id: AcpConnectionId,
    generation: u64,
    live: broadcast::Sender<AcpEventEnvelope>,
    /// Shared by every connection this client holds, unlike `live`.
    ///
    /// A notice is a wake signal for the whole app, so it goes on one bus that
    /// an SSE consumer subscribes to once instead of one per connection.
    notices: broadcast::Sender<AcpNotice>,
    state: Mutex<JournalState>,
}

struct JournalState {
    next_sequence: u64,
    retained: VecDeque<AcpEventEnvelope>,
}

/// Events from a chosen point, backlog first and then live.
///
/// The two halves are handed over together, and the live half is subscribed
/// while the backlog is taken, so no event can slip between them.
pub struct AcpEventStream {
    backlog: std::vec::IntoIter<AcpEventEnvelope>,
    live: broadcast::Receiver<AcpEventEnvelope>,
    connection_id: AcpConnectionId,
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
            live,
            notices,
            state: Mutex::new(JournalState {
                next_sequence: FIRST_SEQUENCE,
                retained: VecDeque::with_capacity(JOURNAL_CAPACITY),
            }),
        }
    }

    /// Positive protocol handoff proof only. A retained journal miss is unknown,
    /// not proof that a prompt was never sent.
    pub(super) fn has_sent_prompt(&self, turn: super::AcpTurnId, expected: &str) -> bool {
        self.state.lock().retained.iter().any(|event| {
            event.turn_id == Some(turn)
                && matches!(&event.event,
                super::AcpClientEvent::PromptSent { text } if text == expected)
        })
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
            envelope
        };
        // An error here means nobody is subscribed, which is not a failure.
        let _ = self.live.send(envelope.clone());
        if let Some(notice) = AcpNotice::from_envelope(&envelope) {
            let _ = self.notices.send(notice);
        }
        envelope
    }

    /// The range a subscriber can still ask for, as the snapshot reports it.
    #[must_use]
    pub fn bounds(&self) -> (u64, u64) {
        let state = self.state.lock();
        let earliest = state
            .retained
            .front()
            .map_or(state.next_sequence, |event| event.sequence);
        (earliest, state.next_sequence.saturating_sub(1))
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
        let live = self.live.subscribe();
        let earliest = state
            .retained
            .front()
            .map_or(state.next_sequence, |event| event.sequence);
        if from != 0 && from < earliest {
            return Err(AcpClientError::stream_gap(
                self.connection_id,
                from,
                earliest,
            ));
        }

        let backlog: Vec<_> = state
            .retained
            .iter()
            .filter(|event| event.sequence >= from)
            .cloned()
            .collect();
        Ok(AcpEventStream {
            backlog: backlog.into_iter(),
            live,
            connection_id: self.connection_id,
        })
    }
}

impl AcpEventStream {
    /// The next event, or `None` once the connection can produce no more.
    ///
    /// A subscriber that fell behind the live buffer is told it has a gap
    /// instead of being handed the events that survived, for the same reason
    /// the journal refuses a cursor it no longer holds.
    pub async fn recv(&mut self) -> Option<Result<AcpEventEnvelope, AcpClientError>> {
        if let Some(event) = self.backlog.next() {
            return Some(Ok(event));
        }
        match self.live.recv().await {
            Ok(event) => Some(Ok(event)),
            Err(broadcast::error::RecvError::Closed) => None,
            Err(broadcast::error::RecvError::Lagged(_)) => {
                Some(Err(AcpClientError::stream_lagged(self.connection_id)))
            }
        }
    }
}
