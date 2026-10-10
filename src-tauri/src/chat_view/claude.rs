//! Claude Code transcript rows -> ACP `SessionUpdate` JSON.
//!
//! Pure: text in, updates out. Claude writes one JSONL row per content block;
//! rows of one API message share `message.id`. The output is the shape
//! `acpTranscript` already folds, so `Transcript.tsx` renders it unchanged.
//!
//! Dropped on purpose: non-human attachments / snapshot / mode rows (harness plumbing,
//! the largest rows in the file), empty `thinking` blocks (Claude stores only a
//! signature) and anything that is not a human prompt, an assistant block or a
//! tool result. A row type this adapter has never seen is counted, never fatal:
//! a new CLI version must not blank the view.

use std::borrow::Cow;
use std::collections::HashSet;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::redaction::redact_secrets;

/// Characters kept of one prompt or reply.
const MAX_TEXT_CHARS: usize = 20_000;
/// Characters kept of one tool output.
const MAX_TOOL_OUTPUT_CHARS: usize = 4_000;
/// Characters kept of one string inside a tool input.
const MAX_INPUT_STRING_CHARS: usize = 1_000;
/// Characters of a tool's argument shown in its title.
const MAX_TITLE_ARG_CHARS: usize = 120;

/// Row types that carry no conversation. Skipped without decoding the body.
const PLUMBING_ROWS: &[&str] = &[
    "file-history-snapshot",
    "file-history-delta",
    "custom-title",
    "ai-title",
    "agent-name",
    "last-prompt",
    "mode",
    "permission-mode",
    "atis-latch",
    "queue-operation",
    "summary",
    "system",
    "progress",
    "cost-state",
    "continued-in",
    "pr-link",
    "fork-context-ref",
    "started",
    "launched",
];

#[derive(Deserialize)]
struct Head<'a> {
    #[serde(rename = "type", borrow)]
    kind: Option<Cow<'a, str>>,
    #[serde(borrow)]
    attachment: Option<AttachmentHead<'a>>,
}

/// Inspect attachment discriminants without allocating their often-large payloads.
#[derive(Deserialize)]
struct AttachmentHead<'a> {
    #[serde(rename = "type", borrow)]
    kind: Option<Cow<'a, str>>,
    #[serde(borrow)]
    origin: Option<OriginHead<'a>>,
}

#[derive(Deserialize)]
struct OriginHead<'a> {
    #[serde(borrow)]
    kind: Option<Cow<'a, str>>,
}

/// What the adapter did with the rows it was given.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct AdapterStats {
    /// Rows whose `type` this adapter does not know.
    pub unknown_rows: u64,
    /// Lines that were not JSON.
    pub malformed_rows: u64,
}

#[derive(Default)]
pub(crate) struct ClaudeAdapter {
    pub stats: AdapterStats,
    known_tools: HashSet<String>,
    /// Message id of the previous update when it was agent text, so a second
    /// text block of the same message is separated from the first.
    last_agent_text_of: Option<String>,
}

impl ClaudeAdapter {
    pub(crate) fn absorb(&mut self, line: &str) -> Vec<Value> {
        let line = line.trim();
        if line.is_empty() {
            return Vec::new();
        }
        let Ok(head) = serde_json::from_str::<Head>(line) else {
            self.stats.malformed_rows += 1;
            return Vec::new();
        };
        let kind = head.kind.as_deref().unwrap_or("");
        if PLUMBING_ROWS.contains(&kind) {
            return Vec::new();
        }
        if kind == "attachment" {
            let human_queued = head.attachment.as_ref().is_some_and(|a| {
                a.kind.as_deref() == Some("queued_command")
                    && a.origin.as_ref().and_then(|o| o.kind.as_deref()) == Some("human")
            });
            if !human_queued {
                return Vec::new();
            }
        } else if kind != "user" && kind != "assistant" {
            self.stats.unknown_rows += 1;
            return Vec::new();
        }
        let Ok(row) = serde_json::from_str::<Value>(line) else {
            self.stats.malformed_rows += 1;
            return Vec::new();
        };
        if row.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            return Vec::new();
        }
        let updates = if kind == "attachment" {
            row.pointer("/attachment/prompt")
                .and_then(|prompt| match prompt {
                    Value::String(text) => Some(Cow::Borrowed(text.as_str())),
                    Value::Array(blocks) => Some(Cow::Owned(blocks_text(blocks))),
                    _ => None,
                })
                .filter(|text| !text.trim().is_empty())
                .map(|text| prompt_update(&row, &text))
                .into_iter()
                .collect()
        } else if kind == "user" {
            self.user_row(&row)
        } else {
            self.assistant_row(&row)
        };
        // Anything but agent text breaks the "same message, next block" run.
        if !updates.iter().any(is_agent_text) {
            self.last_agent_text_of = None;
        }
        updates
    }

    fn user_row(&mut self, row: &Value) -> Vec<Value> {
        let Some(content) = row.pointer("/message/content") else {
            return Vec::new();
        };
        match content {
            Value::String(text) => {
                if row.get("isCompactSummary").and_then(Value::as_bool) == Some(true) {
                    return vec![notice("Conversation compacted")];
                }
                human_prompt(row, text).into_iter().collect()
            }
            Value::Array(blocks) => {
                let mut out: Vec<Value> =
                    blocks.iter().filter_map(|b| self.tool_result(b)).collect();
                out.extend(human_prompt(row, &blocks_text(blocks)));
                out
            }
            _ => Vec::new(),
        }
    }

    fn tool_result(&mut self, block: &Value) -> Option<Value> {
        if block.get("type").and_then(Value::as_str) != Some("tool_result") {
            return None;
        }
        let id = block.get("tool_use_id").and_then(Value::as_str)?;
        // The call opened before the window: a card for it would be an empty
        // one titled with the raw id.
        if !self.known_tools.remove(id) {
            return None;
        }
        let failed = block.get("is_error").and_then(Value::as_bool) == Some(true);
        let text = clean(
            &tool_result_text(block.get("content")),
            MAX_TOOL_OUTPUT_CHARS,
        );
        let mut update = json!({
            "sessionUpdate": "tool_call_update",
            "toolCallId": id,
            "status": if failed { "failed" } else { "completed" },
        });
        if !text.is_empty() {
            update["content"] =
                json!([{ "type": "content", "content": { "type": "text", "text": text } }]);
        }
        Some(update)
    }

    fn assistant_row(&mut self, row: &Value) -> Vec<Value> {
        let Some(message) = row.get("message") else {
            return Vec::new();
        };
        let message_id = message
            .get("id")
            .and_then(Value::as_str)
            .or_else(|| row.get("uuid").and_then(Value::as_str))
            .map(str::to_owned);
        let Some(blocks) = message.get("content").and_then(Value::as_array) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for block in blocks {
            match block.get("type").and_then(Value::as_str) {
                Some("text") => {
                    let raw = block
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    if raw.trim().is_empty() {
                        continue;
                    }
                    let mut text = clean(raw, MAX_TEXT_CHARS);
                    if self.last_agent_text_of.is_some() && self.last_agent_text_of == message_id {
                        text.insert_str(0, "\n\n");
                    }
                    self.last_agent_text_of = message_id.clone();
                    out.push(json!({
                        "sessionUpdate": "agent_message_chunk",
                        "messageId": message_id,
                        "content": { "type": "text", "text": text },
                    }));
                }
                Some("thinking") => {
                    let raw = block
                        .get("thinking")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    if raw.trim().is_empty() {
                        continue;
                    }
                    out.push(json!({
                        "sessionUpdate": "agent_thought_chunk",
                        "messageId": message_id,
                        "content": { "type": "text", "text": clean(raw, MAX_TEXT_CHARS) },
                    }));
                }
                Some("tool_use") => out.extend(self.tool_use(block)),
                Some("fallback") => out.push(notice("Model changed")),
                _ => {}
            }
        }
        out
    }

    fn tool_use(&mut self, block: &Value) -> Option<Value> {
        let id = block.get("id").and_then(Value::as_str)?;
        let name = block.get("name").and_then(Value::as_str).unwrap_or("tool");
        let input = block.get("input").cloned().unwrap_or(Value::Null);
        self.known_tools.insert(id.to_owned());
        Some(json!({
            "sessionUpdate": "tool_call",
            "toolCallId": id,
            "title": tool_title(name, &input),
            "kind": tool_kind(name),
            "status": "in_progress",
            "rawInput": cap_strings(&input),
        }))
    }
}

/// A prompt the user typed, as a user entry. `None` for harness text.
fn human_prompt(row: &Value, text: &str) -> Option<Value> {
    let origin = row.pointer("/origin/kind").and_then(Value::as_str);
    // Recorded older CLI rows have promptId but no origin. Harness echoes
    // also have promptId: exclude their markers, meta rows and tool results.
    let legacy_prompt = origin.is_none()
        && row.get("promptId").and_then(Value::as_str).is_some()
        && row.get("isMeta").and_then(Value::as_bool) != Some(true)
        && row.get("sourceToolAssistantUUID").is_none()
        && !row
            .pointer("/message/content")
            .and_then(Value::as_array)
            .is_some_and(|blocks| {
                blocks
                    .iter()
                    .any(|b| b.get("type").and_then(Value::as_str) == Some("tool_result"))
            })
        && ![
            "<command-name>",
            "<command-message>",
            "<local-command-stdout>",
            "<local-command-caveat>",
            "<bash-input>",
            "<bash-stdout>",
            "<task-notification>",
            "<system-reminder>",
        ]
        .iter()
        .any(|prefix| text.trim_start().starts_with(prefix));
    let human = origin == Some("human") || legacy_prompt;
    if !human || text.trim().is_empty() {
        return None;
    }
    Some(prompt_update(row, text))
}

fn prompt_update(row: &Value, text: &str) -> Value {
    json!({
        "sessionUpdate": "user_message_chunk",
        "messageId": row.get("uuid").and_then(Value::as_str),
        "content": { "type": "text", "text": clean(text, MAX_TEXT_CHARS) },
    })
}

/// The text of a prompt made of blocks: text joined, each image a short
/// placeholder. Tool results are not part of the prompt.
fn blocks_text(blocks: &[Value]) -> String {
    blocks
        .iter()
        .filter_map(|b| match b.get("type").and_then(Value::as_str) {
            Some("text") => b.get("text").and_then(Value::as_str).map(str::to_owned),
            Some("image") => Some("[image]".to_owned()),
            Some("document") => Some("[document]".to_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_agent_text(update: &Value) -> bool {
    update.get("sessionUpdate").and_then(Value::as_str) == Some("agent_message_chunk")
}

/// A card the transcript projection draws apart from the agent's prose.
fn notice(text: &str) -> Value {
    json!({
        "sessionUpdate": "agent_message_chunk",
        "content": { "type": "text", "text": text },
        "_meta": { "ego": { "salience": "card" } },
    })
}

/// Redacted first, then cut: a secret split by the cut would no longer match
/// its pattern.
fn clean(text: &str, max: usize) -> String {
    let redacted = redact_secrets(text);
    if redacted.chars().count() <= max {
        return redacted;
    }
    redacted.chars().take(max - 1).chain(['…']).collect()
}

fn tool_result_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(blocks)) => blocks_text(blocks),
        _ => String::new(),
    }
}

fn tool_kind(name: &str) -> &'static str {
    match name {
        "Read" => "read",
        "Edit" | "MultiEdit" | "Write" | "NotebookEdit" => "edit",
        "Bash" => "execute",
        "Grep" | "Glob" => "search",
        "WebFetch" | "WebSearch" => "fetch",
        _ => "other",
    }
}

/// `Bash: cargo test`, `Read: src/a.rs`: the tool and the one argument that
/// says what it did.
fn tool_title(name: &str, input: &Value) -> String {
    let key = match name {
        "Bash" => "command",
        "Read" | "Edit" | "MultiEdit" | "Write" => "file_path",
        "NotebookEdit" => "notebook_path",
        "Grep" | "Glob" => "pattern",
        "WebFetch" => "url",
        "WebSearch" => "query",
        "Agent" | "Task" => "description",
        _ => "",
    };
    let arg = input.get(key).and_then(Value::as_str).unwrap_or_default();
    let first_line = arg.lines().next().unwrap_or_default();
    if first_line.is_empty() {
        return name.to_owned();
    }
    let arg = clean(first_line, MAX_TITLE_ARG_CHARS);
    format!("{name}: {arg}")
}

/// A tool input with every string redacted and cut, so one `Write` of a whole
/// file does not ride the log.
fn cap_strings(value: &Value) -> Value {
    match value {
        Value::String(s) => Value::String(clean(s, MAX_INPUT_STRING_CHARS)),
        Value::Array(items) => Value::Array(items.iter().map(cap_strings).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), cap_strings(v)))
                .collect(),
        ),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod recorded_tests;
