//! What one AI Chat session is doing, decided in one place.
//!
//! A host renders "working", "waiting for you" or "idle" for a chat, and every
//! host — the desktop panel, a browser, a phone — has to render the same one.
//! So the meaning is derived here, inside the connection actor that owns the
//! facts, and published on the attachment snapshot and the event stream rather
//! than re-derived by each frontend from the frames it happened to see.
//!
//! ```text
//! effective = human wait ? awaiting : running turn || live tool || live child ? busy : idle
//! ```
//!
//! The facts are the attachment's turn, the seats the agent is waiting on, the
//! tool calls of the running turn and, when ego negotiated it, the child
//! roster carried in `session_info_update._meta.ego.childActivity` (ego story
//! 772-d55a, `docs/integrations/acp-extensions.md` in that repository). Child
//! activity is metadata: it never opens or answers a human wait, and it never
//! decides whether a prompt is sent or queued — only the turn does that.
//!
//! Every published activity carries a revision that only grows, per
//! attachment. A host that applies a snapshot it fetched before a newer stream
//! frame keeps the newer one, so a reconnect cannot bring back a question that
//! was already answered.

use std::collections::VecDeque;

use agent_client_protocol::schema::v1;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::AcpHostRequestId;

/// The child activity extension version this client speaks, and only this one.
pub(super) const CHILD_ACTIVITY_VERSION: u64 = 1;

/// Cleared foreground turns remembered, so a late lifecycle frame for one of
/// them cannot latch a child that its turn already retracted.
const CLEARED_TURNS_RETAINED: usize = 64;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AcpChatActivityState {
    #[default]
    Idle,
    Busy,
    Awaiting,
}

/// Which of the two human seats a wait is on.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AcpHumanWaitKind {
    Permission,
    Question,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AcpHumanWait {
    pub request_id: AcpHostRequestId,
    pub kind: AcpHumanWaitKind,
}

/// One tool call of the running turn that has not finished.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AcpActiveTool {
    pub tool_call_id: String,
    pub title: String,
}

/// One live child, by the exact identity ego published.
///
/// `turn_id` and `item_id` are ego's, not this client's `AcpTurnId`: equal
/// meanings on the two sides do not make equal ids.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AcpChildActivity {
    pub turn_id: String,
    pub item_id: String,
    pub child: Value,
}

impl AcpChildActivity {
    fn foreground(&self) -> bool {
        self.child.get("kind").and_then(Value::as_str) == Some("foreground")
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AcpChatActivity {
    pub state: AcpChatActivityState,
    /// Open seats in the order the agent asked.
    pub human_waits: Vec<AcpHumanWait>,
    pub tools: Vec<AcpActiveTool>,
    pub children: Vec<AcpChildActivity>,
    /// The last turn ended because it was cancelled.
    pub interrupted: bool,
    /// Grows by one on every published change of this attachment's activity.
    pub revision: u64,
}

impl AcpChatActivity {
    /// The same activity as `other`, ignoring the revision.
    fn same_meaning(&self, other: &Self) -> bool {
        self.state == other.state
            && self.human_waits == other.human_waits
            && self.tools == other.tools
            && self.children == other.children
            && self.interrupted == other.interrupted
    }

    /// What a connection that ended leaves behind: nothing running, nobody
    /// asked, and the turn that was running did not finish.
    #[must_use]
    pub(super) fn retired(&self, turn_running: bool) -> Self {
        let next = Self {
            interrupted: self.interrupted || turn_running,
            revision: self.revision,
            ..Self::default()
        };
        if next.same_meaning(self) {
            next
        } else {
            Self {
                revision: self.revision + 1,
                ..next
            }
        }
    }
}

/// The facts one attachment's activity is derived from that are not already
/// on the attachment itself.
#[derive(Debug, Default)]
pub(super) struct ActivityTracker {
    tools: Vec<AcpActiveTool>,
    children: Vec<AcpChildActivity>,
    cleared_turns: VecDeque<String>,
    interrupted: bool,
}

impl ActivityTracker {
    /// A new turn starts with no tools and without the last turn's outcome.
    pub(super) fn turn_started(&mut self) {
        self.tools.clear();
        self.interrupted = false;
    }

    /// The prompt answered. Tools cannot outlive it, and neither can a
    /// foreground child whose clear ego omitted; a durable child can.
    pub(super) fn turn_ended(&mut self, interrupted: bool) {
        self.tools.clear();
        self.interrupted = interrupted;
        let retired: Vec<String> = self
            .children
            .iter()
            .filter(|child| child.foreground())
            .map(|child| child.turn_id.clone())
            .collect();
        self.children.retain(|child| !child.foreground());
        for turn in retired {
            self.remember_cleared(turn);
        }
    }

    /// Take in one update. Returns whether anything this tracker holds changed.
    ///
    /// Tool calls count only inside a running turn: a `session/load` replays
    /// calls that never finished, and a replayed `in_progress` with no turn
    /// behind it would mark the chat busy forever.
    pub(super) fn observe(
        &mut self,
        update: &v1::SessionUpdate,
        turn_running: bool,
        children_negotiated: bool,
    ) -> bool {
        match update {
            v1::SessionUpdate::ToolCall(call) if turn_running => self.tool(
                call.tool_call_id.0.as_ref(),
                Some(&call.title),
                Some(call.status),
            ),
            v1::SessionUpdate::ToolCallUpdate(update) if turn_running => self.tool(
                update.tool_call_id.0.as_ref(),
                update.fields.title.as_deref(),
                update.fields.status,
            ),
            v1::SessionUpdate::SessionInfoUpdate(info) if children_negotiated => info
                .meta
                .as_ref()
                .and_then(|meta| meta.get("ego"))
                .and_then(|ego| ego.get("childActivity"))
                .is_some_and(|frame| self.child_frame(frame)),
            _ => false,
        }
    }

    pub(super) fn derive(
        &self,
        turn_running: bool,
        human_waits: Vec<AcpHumanWait>,
        previous: &AcpChatActivity,
    ) -> AcpChatActivity {
        let state = if !human_waits.is_empty() {
            AcpChatActivityState::Awaiting
        } else if turn_running || !self.tools.is_empty() || !self.children.is_empty() {
            AcpChatActivityState::Busy
        } else {
            AcpChatActivityState::Idle
        };
        let next = AcpChatActivity {
            state,
            human_waits,
            tools: self.tools.clone(),
            children: self.children.clone(),
            interrupted: self.interrupted,
            revision: previous.revision,
        };
        if next.same_meaning(previous) {
            next
        } else {
            AcpChatActivity {
                revision: previous.revision + 1,
                ..next
            }
        }
    }

    fn tool(&mut self, id: &str, title: Option<&str>, status: Option<v1::ToolCallStatus>) -> bool {
        let index = self.tools.iter().position(|tool| tool.tool_call_id == id);
        let open = match status {
            Some(v1::ToolCallStatus::Pending | v1::ToolCallStatus::InProgress) => true,
            None => index.is_some(),
            // Completed, failed, and any status a later protocol adds: a call
            // this client cannot read as running is not shown as running.
            Some(_) => false,
        };
        match (index, open) {
            (Some(index), false) => {
                self.tools.remove(index);
                true
            }
            (Some(index), true) => {
                let Some(title) = title.map(str::to_owned) else {
                    return false;
                };
                let changed = self.tools[index].title != title;
                self.tools[index].title = title;
                changed
            }
            (None, true) => {
                self.tools.push(AcpActiveTool {
                    tool_call_id: id.to_owned(),
                    title: title.map_or_else(|| id.to_owned(), str::to_owned),
                });
                true
            }
            (None, _) => false,
        }
    }

    /// Apply one negotiated child frame. A frame this client cannot read
    /// changes nothing: guessing would latch or drop a child.
    fn child_frame(&mut self, frame: &Value) -> bool {
        if frame.get("version").and_then(Value::as_u64) != Some(CHILD_ACTIVITY_VERSION) {
            return false;
        }
        match frame.get("kind").and_then(Value::as_str) {
            Some("lifecycle") => {
                let Some(child) = child_entry(frame) else {
                    return false;
                };
                let index = self.children.iter().position(|held| *held == child);
                match frame
                    .get("phase")
                    .and_then(|phase| phase.get("phase"))
                    .and_then(Value::as_str)
                {
                    Some("started") => {
                        if index.is_some()
                            || (child.foreground() && self.cleared_turns.contains(&child.turn_id))
                        {
                            return false;
                        }
                        self.children.push(child);
                        true
                    }
                    Some("settled") => index.is_some_and(|index| {
                        self.children.remove(index);
                        true
                    }),
                    _ => false,
                }
            }
            Some("clear") => match frame.get("turnId") {
                Some(Value::Null) => {
                    let changed = !self.children.is_empty();
                    self.children.clear();
                    changed
                }
                Some(Value::String(turn))
                    if frame.get("scope").and_then(Value::as_str) == Some("foreground") =>
                {
                    let before = self.children.len();
                    self.children
                        .retain(|child| !(child.foreground() && &child.turn_id == turn));
                    self.remember_cleared(turn.clone());
                    before != self.children.len()
                }
                _ => false,
            },
            Some("snapshot") => {
                let Some(entries) = frame.get("children").and_then(Value::as_array) else {
                    return false;
                };
                let mut children: Vec<AcpChildActivity> = Vec::new();
                for child in entries.iter().filter_map(child_entry) {
                    if !children.contains(&child) {
                        children.push(child);
                    }
                }
                let changed = children != self.children;
                self.children = children;
                changed
            }
            _ => false,
        }
    }

    fn remember_cleared(&mut self, turn: String) {
        if self.cleared_turns.contains(&turn) {
            return;
        }
        if self.cleared_turns.len() == CLEARED_TURNS_RETAINED {
            self.cleared_turns.pop_front();
        }
        self.cleared_turns.push_back(turn);
    }
}

/// The identity of one child entry, or `None` when any part is missing.
fn child_entry(value: &Value) -> Option<AcpChildActivity> {
    let turn_id = value.get("turnId")?.as_str()?.to_owned();
    let item_id = value.get("itemId")?.as_str()?.to_owned();
    let child = value.get("child")?;
    match child.get("kind")?.as_str()? {
        "durable" | "foreground" => {}
        _ => return None,
    }
    Some(AcpChildActivity {
        turn_id,
        item_id,
        child: child.clone(),
    })
}

#[cfg(test)]
#[path = "activity_tests.rs"]
mod tests;
