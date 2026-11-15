//! Native tool activity and user-input waits, scoped to the exact Codex turn.

use base64::Engine;
use serde::Deserialize;
use std::collections::VecDeque;

#[derive(Deserialize)]
struct Receipt {
    session_id: String,
    turn_id: Option<String>,
    hook_event_name: String,
    tool_name: Option<String>,
    tool_use_id: Option<String>,
    agent_id: Option<String>,
    agent_type: Option<String>,
    tool_input: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct Input {
    questions: Option<Vec<Question>>,
}

#[derive(Deserialize)]
struct Question {
    question: String,
}

pub(super) enum Activity {
    Busy,
    Question(String),
    Answered,
    Ended,
    /// The last live child settled after the lead's own Stop: the turn ends now.
    Settled,
}

/// Live children one lead keeps: far above a real fan-out, so a flood of
/// starts cannot grow the roster without bound.
const MAX_CHILDREN: usize = 16;

#[derive(Default)]
pub(super) struct CodexTools {
    session: Option<String>,
    turn: Option<String>,
    waiting: Option<String>,
    /// The normalized payload of the open wait (`waiting`).
    card: Option<crate::agent_hook_payload::QuestionCard>,
    retired: VecDeque<String>,
    /// Live tool calls of the bound turn, for the tool readout.
    pub(super) tools: crate::agent_tool_readout::ToolReadout,
    /// `(agent_id, agent_type)` of the native children in flight (#2635-8052).
    children: Vec<(String, String)>,
    /// Children already settled, so a repeated or late stop or start is inert.
    settled_children: VecDeque<String>,
    /// The lead sent its Stop while children were live.
    lead_settled: bool,
    /// A child's own UserPromptSubmit was seen; its paired `state=prompt` follows.
    child_prompt: bool,
}

fn decode(payload: &str, owner: Option<&str>) -> Option<Receipt> {
    let (sender, encoded) = payload.split_once(':')?;
    if owner != Some(sender) {
        return None;
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}

impl CodexTools {
    pub(super) fn observe_identity(&mut self, payload: &str, owner: Option<&str>) {
        let Some(receipt) = decode(payload, owner) else {
            return;
        };
        // A child's own prompt carries the lead's session id: it is not a lead turn.
        if receipt.agent_id.is_some() {
            self.child_prompt |= receipt.hook_event_name == "UserPromptSubmit";
            return;
        }
        match receipt.hook_event_name.as_str() {
            "SessionStart" => *self = Self::default(),
            "UserPromptSubmit" => {
                self.session = Some(receipt.session_id);
                self.turn = receipt.turn_id;
                self.waiting = None;
                self.retired.clear();
                self.tools.clear();
                // A new lead turn never inherits an old child's busy latch.
                self.forget_children();
            }
            _ => {}
        }
    }

    /// Whether an identity receipt is a child's: it must not rebind the lead.
    pub(super) fn is_child_identity(payload: &str, owner: Option<&str>) -> bool {
        decode(payload, owner).is_some_and(|receipt| receipt.agent_id.is_some())
    }

    /// Consume the mark of a child prompt, so its `state=prompt` is dropped.
    pub(super) fn take_child_prompt(&mut self) -> bool {
        std::mem::take(&mut self.child_prompt)
    }

    /// The lead stopped: hold its turn open while children are live.
    pub(super) fn hold_for_children(&mut self) -> bool {
        self.lead_settled = !self.children.is_empty();
        self.lead_settled
    }

    /// Labels of the live children, in start order.
    pub(super) fn children(&self) -> impl Iterator<Item = &str> {
        self.children.iter().map(|(_, label)| label.as_str())
    }

    fn forget_children(&mut self) {
        self.children.clear();
        self.lead_settled = false;
    }

    fn child(&mut self, receipt: Receipt) -> Option<Activity> {
        let id = receipt.agent_id.filter(|id| !id.is_empty())?;
        if self.settled_children.contains(&id) {
            return None;
        }
        let held = self.children.iter().position(|(held, _)| *held == id);
        match (receipt.hook_event_name.as_str(), held) {
            ("SubagentStart", None) if self.children.len() < MAX_CHILDREN => {
                let label = receipt.agent_type.unwrap_or_else(|| "child".into());
                self.children.push((id, label));
                None
            }
            ("SubagentStop", held) => {
                // Record every stop, also one that arrives before its start
                // (two hook processes, out of order): the late start is inert.
                if self.settled_children.len() == 64 {
                    self.settled_children.pop_front();
                }
                self.settled_children.push_back(id);
                let index = held?;
                self.children.remove(index);
                (self.children.is_empty() && std::mem::take(&mut self.lead_settled))
                    .then_some(Activity::Settled)
            }
            _ => None,
        }
    }

    pub(super) fn awaiting(&self) -> bool {
        self.waiting.is_some()
    }

    /// The native tool call id of the open wait, if any.
    pub(super) fn wait_request(&self) -> Option<&str> {
        self.waiting.as_deref()
    }

    /// The payload card of the open wait; never one of a settled request.
    pub(super) fn wait_card(&self) -> Option<&crate::agent_hook_payload::QuestionCard> {
        self.card
            .as_ref()
            .filter(|card| Some(&card.request_id) == self.waiting.as_ref())
    }

    pub(super) fn clear(&mut self) {
        self.card = None;
        if let Some(id) = self.waiting.take() {
            if self.retired.len() == 64 {
                self.retired.pop_front();
            }
            self.retired.push_back(id);
        }
    }

    pub(super) fn receive(
        &mut self,
        payload: &str,
        owner: Option<&str>,
        native_session: Option<&str>,
    ) -> Option<Activity> {
        let receipt = decode(payload, owner)?;
        if native_session != Some(receipt.session_id.as_str())
            || self.session.as_deref() != Some(receipt.session_id.as_str())
        {
            return None;
        }
        if matches!(
            receipt.hook_event_name.as_str(),
            "SubagentStart" | "SubagentStop"
        ) {
            return self.child(receipt);
        }
        if receipt.agent_id.is_some() {
            return None;
        }
        if receipt.hook_event_name == "SessionEnd" {
            self.forget_children();
            self.clear();
            self.tools.clear();
            self.session = None;
            self.turn = None;
            return Some(Activity::Ended);
        }
        if receipt.turn_id.is_none() || receipt.turn_id != self.turn {
            return None;
        }
        if receipt.hook_event_name == "Interrupt" {
            self.forget_children();
            self.clear();
            self.tools.clear();
            self.turn = None;
            return Some(Activity::Ended);
        }
        let id = receipt.tool_use_id.filter(|id| !id.is_empty())?;
        match receipt.hook_event_name.as_str() {
            "PreToolUse" => {
                if let Some(name) = receipt.tool_name.as_deref() {
                    self.tools.start(&id, name);
                }
            }
            "PostToolUse" => self.tools.end(&id),
            _ => {}
        }
        match receipt.hook_event_name.as_str() {
            "PreToolUse" if receipt.tool_name.as_deref() == Some("request_user_input") => {
                if self.retired.contains(&id) {
                    return None;
                }
                let input = receipt.tool_input?;
                let card = crate::agent_hook_payload::codex_card(&id, &input);
                let questions = serde_json::from_value::<Input>(input).ok()?.questions?;
                let text = questions
                    .iter()
                    .map(|q| q.question.as_str())
                    .collect::<Vec<_>>()
                    .join("\n\n");
                if text.is_empty() {
                    return None;
                }
                self.waiting = Some(id);
                self.card = card;
                Some(Activity::Question(text))
            }
            "PostToolUse" if self.waiting.as_ref() == Some(&id) => {
                self.clear();
                Some(Activity::Answered)
            }
            "PreToolUse" | "PostToolUse" => Some(Activity::Busy),
            _ => None, // PermissionRequest is deliberately not consumed.
        }
    }
}
