//! Correlate native Grok receipts before publishing the shared PTY state events.

use base64::Engine;
use serde::Deserialize;
use std::collections::VecDeque;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Receipt {
    hook_event_name: String,
    session_id: String,
    prompt_id: Option<String>,
    tool_use_id: Option<String>,
    tool_name: Option<String>,
    notification_type: Option<String>,
    subagent_type: Option<String>,
}

#[derive(Default)]
pub(super) struct GrokHooks {
    session: Option<String>,
    prompt: Option<String>,
    ended: bool,
    closed: bool,
    pending: VecDeque<String>,
    waiting: Option<String>,
    /// Live tool calls of the current prompt, for the tool readout.
    pub(super) tools: crate::agent_tool_readout::ToolReadout,
}

impl GrokHooks {
    pub(super) fn active(&self) -> bool {
        self.session.is_some()
    }

    pub(super) fn awaiting(&self) -> bool {
        self.waiting.is_some()
    }

    pub(super) fn receive(&mut self, payload: &str, owner: Option<&str>) -> Option<&'static str> {
        let (sender, encoded) = payload.split_once(':')?;
        if owner != Some(sender) || encoded.len() > 1024 * 1024 {
            return None;
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .ok()?;
        let receipt: Receipt = serde_json::from_slice(&bytes).ok()?;
        if receipt.subagent_type.is_some()
            || receipt.session_id.is_empty()
            || receipt.session_id.len() > 256
            || [&receipt.prompt_id, &receipt.tool_use_id]
                .iter()
                .any(|id| id.as_ref().is_some_and(|id| id.len() > 256))
        {
            return None;
        }
        let event = receipt.hook_event_name.as_str();
        if !matches!(
            event,
            "session_start"
                | "user_prompt_submit"
                | "pre_tool_use"
                | "post_tool_use"
                | "post_tool_use_failure"
                | "permission_denied"
                | "notification"
                | "stop"
                | "stop_cancelled"
                | "stop_failure"
                | "session_end"
        ) {
            return None;
        }
        if self.session.as_deref() != Some(&receipt.session_id) {
            if self.session.is_some() && event != "user_prompt_submit" {
                return None;
            }
            *self = Self {
                session: Some(receipt.session_id),
                ..Self::default()
            };
        }
        if self.closed && (event != "user_prompt_submit" || receipt.prompt_id == self.prompt) {
            return None;
        }
        match event {
            "session_start" => None, // Can arrive after UserPromptSubmit; never reset its turn.
            "user_prompt_submit" => {
                self.prompt = Some(receipt.prompt_id.filter(|id| !id.is_empty())?);
                self.ended = false;
                self.closed = false;
                self.pending.clear();
                self.waiting = None;
                self.tools.clear();
                Some("prompt")
            }
            "session_end" => {
                self.closed = true;
                self.finish()
            }
            "stop" | "stop_cancelled" | "stop_failure" => {
                // Cancellation can be dispatched after the next prompt.
                if receipt.prompt_id.is_some() && receipt.prompt_id != self.prompt {
                    return None;
                }
                self.finish()
            }
            _ if self.ended || self.prompt.is_none() => None,
            "pre_tool_use" => {
                let id = receipt.tool_use_id.filter(|id| !id.is_empty())?;
                if let Some(name) = receipt.tool_name.as_deref() {
                    self.tools.start(&id, name);
                }
                if !self.pending.contains(&id) {
                    // Bound bookkeeping without evicting the request owning the modal.
                    if self.pending.len() == 64 {
                        let index = self
                            .pending
                            .iter()
                            .position(|id| Some(id) != self.waiting.as_ref())?;
                        self.pending.remove(index);
                    }
                    self.pending.push_back(id);
                }
                (!self.awaiting()).then_some("busy")
            }
            "notification" if receipt.notification_type.as_deref() == Some("permission_prompt") => {
                // The real notification has neither turn nor tool id. Use the newest
                // unanswered PreToolUse in this session's current turn.
                self.waiting = Some(self.pending.back()?.clone());
                Some("awaiting")
            }
            "post_tool_use" | "post_tool_use_failure" | "permission_denied" => {
                let id = receipt.tool_use_id?;
                self.tools.end(&id);
                let index = self.pending.iter().position(|pending| *pending == id)?;
                self.pending.remove(index);
                if self.waiting.as_ref() == Some(&id) {
                    self.waiting = None;
                    Some("busy")
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn finish(&mut self) -> Option<&'static str> {
        self.ended = true;
        self.pending.clear();
        self.tools.clear();
        self.waiting = None;
        Some("idle")
    }
}
