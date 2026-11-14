//! Unresolved question tool calls in the exact transcript of a terminal's agent.
//!
//! This is enrichment only. The PTY reader or a native hook must already hold a
//! live wait (`awaiting_input`); transcript history never sets one. While that
//! wait is live, the text of the question comes from, in order:
//!
//! 1. the current structured event (a native hook that carried the question);
//! 2. the one unresolved question tool call in the exact Claude or Codex
//!    transcript that matches the hook request, or the sole unresolved one when
//!    no hook named a request;
//! 3. the screen fallback already in `question_text`.
//!
//! An answer, a cancellation or any other tool result settles a call for good:
//! a later replay of the same call cannot reopen it. A new turn settles every
//! call that is still open. A different session file, a lost exact binding or
//! the exit of the process drops the ledger. ego is not served here: its native
//! question owner holds its questions.

use std::collections::VecDeque;

use serde_json::Value;

use crate::state::{AppState, SessionState};

/// Open calls kept at most. A turn normally holds one.
const MAX_OPEN: usize = 8;
/// Settled call ids kept, so a replayed call never reopens.
const MAX_SETTLED: usize = 256;

/// What one transcript row means for the question ledger.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Row {
    /// A question tool call was issued.
    Open { id: String, text: String },
    /// A tool call received its result (an answer, a refusal or an abort).
    Settled(String),
    /// A turn started or ended: nothing issued before it can still wait.
    TurnEnded,
}

/// Unresolved question calls of one transcript.
#[derive(Debug, Default)]
pub(crate) struct QuestionLedger {
    open: Vec<(String, String)>,
    settled: VecDeque<String>,
}

impl QuestionLedger {
    pub(crate) fn absorb(&mut self, row: Row) {
        match row {
            Row::Open { id, text } => {
                if self.settled.contains(&id) || self.open.iter().any(|(open, _)| *open == id) {
                    return;
                }
                if self.open.len() == MAX_OPEN {
                    let (oldest, _) = self.open.remove(0);
                    self.settle(oldest);
                }
                self.open.push((id, text));
            }
            Row::Settled(id) => {
                self.open.retain(|(open, _)| *open != id);
                self.settle(id);
            }
            Row::TurnEnded => {
                for (id, _) in std::mem::take(&mut self.open) {
                    self.settle(id);
                }
            }
        }
    }

    fn settle(&mut self, id: String) {
        if self.settled.contains(&id) {
            return;
        }
        if self.settled.len() == MAX_SETTLED {
            self.settled.pop_front();
        }
        self.settled.push_back(id);
    }

    /// The open question for `request`. Without a request only a sole open
    /// question qualifies: two candidates are ambiguous and give nothing.
    pub(crate) fn question(&self, request: Option<&str>) -> Option<&str> {
        match request {
            Some(request) => self
                .open
                .iter()
                .find(|(id, _)| id == request)
                .map(|(_, text)| text.as_str()),
            None => match self.open.as_slice() {
                [(_, text)] => Some(text),
                _ => None,
            },
        }
    }
}

/// The text of the `questions` array that Claude `AskUserQuestion` and Codex
/// `request_user_input` share, joined like the Codex hook joins it.
pub(crate) fn questions_text(questions: &Value) -> String {
    let text = questions
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|q| q.get("question").and_then(Value::as_str))
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    crate::chat_view::clean_question(&text)
}

/// The priority rule. `transcript` is called only when it can change the
/// answer: a hook request that already carried its text keeps it.
pub(crate) fn recovered_text(
    current: Option<&str>,
    request: Option<&str>,
    transcript: impl FnOnce() -> Option<String>,
) -> Option<String> {
    if request.is_some() && current.is_some_and(|text| !text.trim().is_empty()) {
        return None;
    }
    transcript()
}

/// Name a confirmed live Claude or Codex wait from its exact transcript.
pub(crate) fn enrich(state: &AppState, sid: &str, snapshot: &mut SessionState) {
    if !snapshot.awaiting_input
        || !matches!(snapshot.agent_type.as_deref(), Some("claude" | "codex"))
    {
        return;
    }
    let request = snapshot.wait_request.clone();
    if let Some(text) = recovered_text(
        snapshot.question_text.as_deref(),
        request.as_deref(),
        || crate::chat_view::transcript_question(state, sid, request.as_deref()),
    ) {
        snapshot.question_text = Some(text);
    }
}

#[cfg(test)]
mod tests;
