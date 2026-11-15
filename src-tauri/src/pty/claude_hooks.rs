//! Correlate native Claude permission receipts without clearing unrelated dialogs.

use base64::Engine;
use serde::Deserialize;
use std::collections::{HashMap, VecDeque};

#[derive(Deserialize)]
struct Receipt {
    session_id: String,
    prompt_id: Option<String>,
    hook_event_name: String,
    tool_use_id: Option<String>,
    tool_name: Option<String>,
    tool_input: Option<serde_json::Value>,
    notification_type: Option<String>,
    agent_id: Option<String>,
}

struct Tool {
    id: String,
    name: String,
    input: serde_json::Value,
}

#[derive(Default)]
pub(super) struct ClaudeHooks {
    session: Option<String>,
    prompt: Option<String>,
    pending: VecDeque<Tool>,
    waiting: Option<String>,
    retired: VecDeque<String>,
    elicitation: bool,
    /// The normalized payload of the open question wait (`waiting`).
    card: Option<crate::agent_hook_payload::QuestionCard>,
    /// Base64 pieces of receipts still being written, by hook pid.
    pieces: HashMap<String, String>,
    /// Live tool calls of the current prompt, for the tool readout.
    pub(super) tools: crate::agent_tool_readout::ToolReadout,
}

impl ClaudeHooks {
    pub(super) fn awaiting(&self) -> bool {
        self.waiting.is_some() || self.elicitation
    }

    /// The native tool call id of the open wait, if any.
    pub(super) fn wait_request(&self) -> Option<&str> {
        self.waiting.as_deref()
    }

    /// The payload card of the open wait. A card of a settled request is
    /// never returned, even before `card` itself is replaced.
    pub(super) fn wait_card(&self) -> Option<&crate::agent_hook_payload::QuestionCard> {
        self.card
            .as_ref()
            .filter(|card| Some(&card.request_id) == self.waiting.as_ref())
    }

    fn retire(&mut self, id: String) {
        if self.retired.len() == 64 {
            self.retired.pop_front();
        }
        self.retired.push_back(id);
    }

    pub(super) fn clear(&mut self) {
        self.pending.clear();
        self.waiting = None;
        self.card = None;
        self.prompt = None;
        self.retired.clear();
        self.elicitation = false;
        self.tools.clear();
    }

    pub(super) fn resolve_permission_input(&mut self) -> bool {
        let Some(index) = self.pending.iter().position(|tool| {
            Some(&tool.id) == self.waiting.as_ref()
                && !matches!(tool.name.as_str(), "AskUserQuestion" | "ExitPlanMode")
        }) else {
            return false;
        };
        let tool = self.pending.remove(index).unwrap();
        self.waiting = None;
        self.retire(tool.id);
        true
    }

    /// Collect one `<tab>:<hook pid>:<piece>` frame; an empty piece ends a receipt.
    pub(super) fn receive(&mut self, payload: &str, owner: Option<&str>) -> Option<&'static str> {
        let mut fields = payload.splitn(3, ':');
        let (sender, writer, piece) = (fields.next()?, fields.next()?, fields.next()?);
        if owner != Some(sender) || writer.is_empty() || writer.len() > 32 {
            return None;
        }
        if !piece.is_empty() {
            if self.pieces.len() == 16 && !self.pieces.contains_key(writer) {
                self.pieces.clear(); // Hooks that died mid-receipt.
            }
            let encoded = self.pieces.entry(writer.to_owned()).or_default();
            encoded.push_str(piece);
            if encoded.len() > 1024 * 1024 {
                self.pieces.remove(writer);
            }
            return None;
        }
        let encoded = self.pieces.remove(writer)?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .ok()?;
        let receipt: Receipt = serde_json::from_slice(&bytes).ok()?;
        if receipt.agent_id.is_some()
            || receipt.session_id.is_empty()
            || receipt.session_id.len() > 256
        {
            return None;
        }
        if receipt.hook_event_name == "UserPromptSubmit" {
            self.clear();
            self.session = Some(receipt.session_id);
            self.prompt = receipt.prompt_id;
            return None; // The same command carries the existing state=prompt marker.
        }
        if self.session.as_deref() != Some(&receipt.session_id)
            || self.prompt.is_none()
            || (receipt.prompt_id != self.prompt
                && !matches!(
                    receipt.hook_event_name.as_str(),
                    "Elicitation" | "ElicitationResult"
                ))
        {
            return None;
        }
        match receipt.hook_event_name.as_str() {
            "PreToolUse" => {
                let id = receipt.tool_use_id?;
                if id.is_empty()
                    || id.len() > 256
                    || self.retired.contains(&id)
                    || self.pending.iter().any(|tool| tool.id == id)
                {
                    return None;
                }
                if let Some(name) = receipt.tool_name.as_deref() {
                    self.tools.start(&id, name);
                }
                let input = receipt.tool_input?;
                if serde_json::to_vec(&input).ok()?.len() > 16 * 1024 {
                    return None;
                }
                if self.pending.len() == 64 {
                    let index = self
                        .pending
                        .iter()
                        .position(|tool| Some(&tool.id) != self.waiting.as_ref())?;
                    self.pending.remove(index);
                }
                let name = receipt.tool_name?;
                let question = matches!(name.as_str(), "AskUserQuestion" | "ExitPlanMode");
                let card = question
                    .then(|| crate::agent_hook_payload::claude_card(&name, &id, &input))
                    .flatten();
                self.pending.push_back(Tool {
                    id: id.clone(),
                    name,
                    input,
                });
                if question && !self.awaiting() {
                    self.waiting = Some(id);
                    self.card = card;
                    Some("awaiting")
                } else {
                    None
                }
            }
            "Elicitation" => {
                self.elicitation = true;
                Some("awaiting")
            }
            "ElicitationResult" => {
                self.elicitation = false;
                (!self.awaiting()).then_some("busy")
            }
            "PermissionRequest" if !self.awaiting() => {
                // Native 2.1.314 omits tool_use_id on PermissionRequest. Match its
                // exact input to an unresolved PreToolUse in the same prompt.
                let tool = self.pending.iter().rev().find(|tool| {
                    Some(&tool.name) == receipt.tool_name.as_ref()
                        && Some(&tool.input) == receipt.tool_input.as_ref()
                })?;
                self.waiting = Some(tool.id.clone());
                Some("awaiting")
            }
            "Notification"
                if receipt.notification_type.as_deref() == Some("permission_prompt")
                    && !self.awaiting() =>
            {
                self.waiting = Some(self.pending.back()?.id.clone());
                Some("awaiting")
            }
            "PostToolUse" | "PostToolUseFailure" => {
                let id = receipt.tool_use_id?;
                self.tools.end(&id);
                let index = self.pending.iter().position(|tool| tool.id == id)?;
                self.pending.remove(index);
                self.retire(id.clone());
                if self.waiting.as_ref() == Some(&id) {
                    self.waiting = None;
                    (!self.awaiting()).then_some("busy")
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

/// Claude's permission row can wrap the always-allow path. Preserve those rows
/// as one label so production input bookkeeping has the native refusal key.
pub(super) fn permission_choices(screen: &[String]) -> Option<crate::output_parser::ParsedEvent> {
    let title = screen
        .iter()
        .rposition(|row| row.trim() == "Do you want to proceed?")?;
    let footer = screen
        .iter()
        .enumerate()
        .skip(title + 1)
        .find(|(_, row)| row.trim().starts_with("Esc to cancel") && row.contains("Tab to amend"))?
        .0;
    let mut rows = screen[..=title].to_vec();
    let mut has_option = false;
    for row in &screen[title + 1..footer] {
        let text = row.trim();
        if text.is_empty() {
            continue;
        }
        let option = text.trim_start_matches(['❯', '›', '>']).trim_start();
        let numbered = option.split_once('.').is_some_and(|(key, label)| {
            !key.is_empty() && key.chars().all(|ch| ch.is_ascii_digit()) && label.starts_with(' ')
        });
        if numbered {
            rows.push(row.clone());
            has_option = true;
        } else if has_option && row.starts_with("      ") {
            rows.last_mut()?.push(' ');
            rows.last_mut()?.push_str(text);
        } else {
            return None;
        }
    }
    rows.extend_from_slice(&screen[footer..]);
    crate::output_parser::parse_choice_prompt(&rows)
}
