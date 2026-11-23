//! Direct questions: the user asked the agent in its terminal. When the turn
//! ends, TUIC reads the question and the final reply from the terminal's chat
//! view and files them in the journal — no agent cooperation and no extra
//! tokens. The chat view adapters turn every agent's transcript (Claude, Codex,
//! Grok, ego) into the same ACP updates, so one reader serves them all.

use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use super::store::{self, NewQuestion};
use crate::progress::ProgressStore;
use crate::state::{AppEvent, AppState};

/// Longest user message still read as a question, in chars.
const MAX_QUESTION_CHARS: usize = 2_000;
/// Agents with a chat view adapter.
const AGENTS: [&str; 4] = ["claude", "codex", "grok", "ego"];
/// Waits before each read after an idle. The idle can precede the final
/// transcript record (Claude writes the thinking block and the text in two
/// records), so an empty read is retried instead of losing the question.
const READ_DELAYS: [Duration; 4] = [
    Duration::from_millis(500),
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(4),
];

/// The last finished exchange of a chat view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Exchange {
    /// The user message id: one filing per message, across restarts.
    pub message_id: String,
    pub question: String,
    pub answer: String,
}

fn kind(update: &Value) -> Option<&str> {
    update["sessionUpdate"].as_str()
}

fn text(update: &Value) -> &str {
    update["content"]["text"].as_str().unwrap_or_default()
}

/// A notice the projection draws as a card ("Model changed"), not prose.
fn is_notice(update: &Value) -> bool {
    !update["_meta"]["ego"]["salience"].is_null()
}

fn is_question(text: &str) -> bool {
    text.ends_with('?') && text.chars().count() <= MAX_QUESTION_CHARS
}

/// The last user message and the agent's final message after it. `None` while
/// the turn still runs (a tool call follows the last agent text), after a
/// non-question, or when the turn produced no text.
pub(crate) fn last_exchange(updates: &[Value]) -> Option<Exchange> {
    let user = updates
        .iter()
        .rposition(|u| kind(u) == Some("user_message_chunk"))?;
    let message_id = updates[user]["messageId"].as_str()?;
    // A long prompt can arrive in several chunks of one message.
    let first = updates[..user]
        .iter()
        .rposition(|u| {
            kind(u) != Some("user_message_chunk") || u["messageId"].as_str() != Some(message_id)
        })
        .map_or(0, |i| i + 1);
    let question: String = updates[first..=user].iter().map(text).collect();
    let question = question.trim();
    if question.starts_with('<') || !is_question(question) {
        return None;
    }
    let turn = &updates[user + 1..];
    let last = turn
        .iter()
        .rposition(|u| kind(u) != Some("agent_thought_chunk") && !is_notice(u))?;
    if kind(&turn[last]) != Some("agent_message_chunk") {
        return None;
    }
    let final_id = &turn[last]["messageId"];
    let answer: String = turn
        .iter()
        .filter(|u| {
            kind(u) == Some("agent_message_chunk") && !is_notice(u) && &u["messageId"] == final_id
        })
        .map(text)
        .collect();
    let answer = super::markers::strip(&answer);
    (!answer.is_empty()).then(|| Exchange {
        message_id: message_id.to_owned(),
        question: question.to_owned(),
        answer,
    })
}

/// The dedup key of an exchange. A transcript's own message ids are only
/// unique within one session (Grok numbers its prompts from 0), while the
/// store dedups globally, so the key carries the TUIC session.
fn source_id(session_id: &str, message_id: &str) -> String {
    format!("{session_id}:{message_id}")
}

/// File the last exchange of an agent terminal. `Ok(true)` when the exchange
/// was found (filed now or before), `Ok(false)` when there is none yet.
fn capture(state: &Arc<AppState>, session_id: &str) -> Result<bool, String> {
    let snapshot = match crate::chat_view::chat_view_snapshot(state, session_id, None, 0) {
        Ok(snapshot) => snapshot,
        // No exact binding: nothing to read, and no guessing.
        Err(error) if error.starts_with("not_bound") => return Ok(false),
        Err(error) => return Err(error),
    };
    let Some(exchange) = last_exchange(&snapshot.updates) else {
        return Ok(false);
    };
    let Some(project) = crate::progress::project_for_session(state, session_id) else {
        return Ok(true);
    };
    let project = crate::progress::project_of(&project)?;
    let agent_name = state
        .session_maps
        .sessions
        .get(session_id)
        .and_then(|s| s.lock().display_name.clone());
    let q = NewQuestion {
        project: &project,
        pty_id: session_id,
        agent_name: agent_name.as_deref(),
        question: &exchange.question,
    };
    if let Some((question, answer)) = store::record_direct(
        &ProgressStore::open()?,
        &q,
        &exchange.answer,
        &source_id(session_id, &exchange.message_id),
    )? {
        crate::mcp_http::mcp_transport::emit_progress_entry(state, question);
        crate::mcp_http::mcp_transport::emit_progress_entry(state, answer);
    }
    Ok(true)
}

fn agent_type(state: &AppState, session_id: &str) -> Option<String> {
    state
        .session_maps
        .session_states
        .get(session_id)?
        .agent_type
        .clone()
}

fn is_idle(state: &AppState, session_id: &str) -> bool {
    state
        .session_maps
        .session_states
        .get(session_id)
        .is_some_and(|s| s.shell_state.as_deref() == Some("idle"))
}

/// The session of an idle transition, when the terminal runs an agent with a
/// chat view. The agent type is read from the session state: the event
/// carries it only sometimes.
fn idle_agent_session<'a>(state: &AppState, event: &'a AppEvent) -> Option<&'a str> {
    let AppEvent::PtyParsed { session_id, parsed } = event else {
        return None;
    };
    if parsed["type"] != "shell-state" || parsed["state"] != "idle" {
        return None;
    }
    let agent = agent_type(state, session_id)?;
    AGENTS
        .contains(&agent.as_str())
        .then_some(session_id.as_str())
}

/// Read until the exchange shows up or the agent starts a new turn.
async fn capture_after_idle(state: Arc<AppState>, session_id: String) {
    for delay in READ_DELAYS {
        tokio::time::sleep(delay).await;
        if !is_idle(&state, &session_id) {
            return; // a new turn: its own idle captures it
        }
        let (st, sid) = (state.clone(), session_id.clone());
        match tokio::task::spawn_blocking(move || capture(&st, &sid)).await {
            Ok(Ok(true)) => return,
            Ok(Ok(false)) => {}
            Ok(Err(error)) => {
                tracing::warn!(target: "discussion", session_id, %error, "Cannot capture a direct question");
                return;
            }
            Err(error) => {
                tracing::warn!(target: "discussion", session_id, %error, "Direct question capture task failed");
                return;
            }
        }
    }
}

/// Follow the bus and capture after every idle transition of an agent terminal.
pub(crate) fn start(state: Arc<AppState>) {
    let Ok(runtime) = tokio::runtime::Handle::try_current() else {
        return;
    };
    runtime.spawn(async move {
        use tokio::sync::broadcast::error::RecvError;
        let mut events = state.event_bus.subscribe();
        loop {
            match events.recv().await {
                Ok(event) => {
                    let Some(session_id) = idle_agent_session(&state, &event).map(str::to_owned)
                    else {
                        continue;
                    };
                    tokio::spawn(capture_after_idle(state.clone(), session_id));
                }
                // Missed idles are recovered by the next one: the exchange is still last.
                Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => return,
            }
        }
    });
}

#[cfg(test)]
#[path = "capture_tests.rs"]
mod tests;
