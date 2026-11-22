//! Grok `chat_history.jsonl` rows -> ACP `SessionUpdate` JSON.
//!
//! Pure: text in, updates out. `chat_history.jsonl` is Grok's complete,
//! append-only conversation: one row per user turn, reasoning summary,
//! assistant message (with its tool calls) and tool result. `updates.jsonl`
//! streams the same events but omits user turns, so it is not read.
//!
//! Dropped on purpose: the system prompt and every user row that is not a
//! prompt the human typed (`<user_info>` context, reminders, hook feedback,
//! task-completion notices). A human prompt is the row carrying `prompt_index`
//! and no `synthetic_reason`. A row type this adapter has never seen is
//! counted, never fatal.

use std::collections::HashSet;

use serde_json::{Value, json};

use super::claude::{
    AdapterStats, MAX_TEXT_CHARS, MAX_TITLE_ARG_CHARS, MAX_TOOL_OUTPUT_CHARS, cap_strings, clean,
    notice,
};

#[derive(Default)]
pub(crate) struct GrokAdapter {
    pub stats: AdapterStats,
    known_tools: HashSet<String>,
    /// Rows carry no message id; the projection joins chunks without one, so
    /// each assistant row gets its own.
    messages: u64,
    backend_calls: u64,
}

impl GrokAdapter {
    pub(crate) fn absorb(&mut self, line: &str) -> Vec<Value> {
        let line = line.trim();
        if line.is_empty() {
            return Vec::new();
        }
        let Ok(row) = serde_json::from_str::<Value>(line) else {
            self.stats.malformed_rows += 1;
            return Vec::new();
        };
        match row.get("type").and_then(Value::as_str).unwrap_or("") {
            "system" => Vec::new(),
            "user" => user_row(&row),
            "reasoning" => reasoning_row(&row),
            "assistant" => self.assistant_row(&row),
            "tool_result" => self.tool_result(&row).into_iter().collect(),
            "backend_tool_call" => self.backend_call(&row).into_iter().collect(),
            _ => {
                self.stats.unknown_rows += 1;
                Vec::new()
            }
        }
    }

    fn assistant_row(&mut self, row: &Value) -> Vec<Value> {
        self.messages += 1;
        let message_id = format!("grok-{}", self.messages);
        let mut out = Vec::new();
        let text = row
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !text.trim().is_empty() {
            out.push(json!({
                "sessionUpdate": "agent_message_chunk",
                "messageId": message_id,
                "content": { "type": "text", "text": clean(text, MAX_TEXT_CHARS) },
            }));
        }
        for call in row
            .get("tool_calls")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(id) = call.get("id").and_then(Value::as_str) else {
                continue;
            };
            let name = call.get("name").and_then(Value::as_str).unwrap_or("tool");
            let input = call
                .get("arguments")
                .and_then(Value::as_str)
                .and_then(|a| serde_json::from_str::<Value>(a).ok())
                .unwrap_or(Value::Null);
            self.known_tools.insert(id.to_owned());
            out.push(json!({
                "sessionUpdate": "tool_call",
                "toolCallId": id,
                "title": tool_title(name, &input),
                "kind": tool_kind(name),
                "status": "in_progress",
                "rawInput": cap_strings(&input),
            }));
        }
        out
    }

    fn tool_result(&mut self, row: &Value) -> Option<Value> {
        let id = row.get("tool_call_id").and_then(Value::as_str)?;
        // The call opened before the window: a card for it would be an empty
        // one titled with the raw id.
        if !self.known_tools.remove(id) {
            return None;
        }
        let text = clean(
            row.get("content")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            MAX_TOOL_OUTPUT_CHARS,
        );
        let mut update = json!({
            "sessionUpdate": "tool_call_update",
            "toolCallId": id,
            "status": "completed",
        });
        if !text.is_empty() {
            update["content"] =
                json!([{ "type": "content", "content": { "type": "text", "text": text } }]);
        }
        Some(update)
    }

    /// A tool Grok ran on the server (web search): one finished card.
    fn backend_call(&mut self, row: &Value) -> Option<Value> {
        let kind = row.get("kind")?;
        let tool = kind.get("tool_type").and_then(Value::as_str)?;
        let query = kind
            .pointer("/action/query")
            .and_then(Value::as_str)
            .unwrap_or_default();
        self.backend_calls += 1;
        Some(json!({
            "sessionUpdate": "tool_call",
            "toolCallId": format!("grok-backend-{}", self.backend_calls),
            "title": titled(tool, query),
            "kind": "fetch",
            "status": "completed",
            "rawInput": cap_strings(&json!({ "query": query })),
        }))
    }
}

fn user_row(row: &Value) -> Vec<Value> {
    let text = row
        .pointer("/content/0/text")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match row.get("synthetic_reason").and_then(Value::as_str) {
        Some("compaction_meta") => return vec![notice("Conversation compacted")],
        Some(_) => return Vec::new(),
        None => {}
    }
    if row.get("prompt_index").is_none() {
        return Vec::new();
    }
    let prompt = user_query(text);
    if prompt.trim().is_empty() {
        return Vec::new();
    }
    // The prompt index is the message id: a direct-question capture files one
    // entry per message and needs a stable key across reads.
    vec![json!({
        "sessionUpdate": "user_message_chunk",
        "messageId": row.get("prompt_index").map(|i| format!("grok-prompt-{i}")),
        "content": { "type": "text", "text": clean(prompt, MAX_TEXT_CHARS) },
    })]
}

/// What the human typed: Grok wraps it in `<user_query>`, and an interrupted
/// turn adds text before and after the wrapper.
fn user_query(text: &str) -> &str {
    const OPEN: &str = "<user_query>";
    const CLOSE: &str = "</user_query>";
    match (text.find(OPEN), text.rfind(CLOSE)) {
        (Some(start), Some(end)) if start + OPEN.len() <= end => {
            text[start + OPEN.len()..end].trim()
        }
        _ => text,
    }
}

fn reasoning_row(row: &Value) -> Vec<Value> {
    let text = row
        .get("summary")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n\n");
    if text.trim().is_empty() {
        return Vec::new();
    }
    vec![json!({
        "sessionUpdate": "agent_thought_chunk",
        "content": { "type": "text", "text": clean(&text, MAX_TEXT_CHARS) },
    })]
}

fn tool_kind(name: &str) -> &'static str {
    match name {
        "read_file" | "list_dir" => "read",
        "search_replace" | "write" => "edit",
        "run_terminal_command" => "execute",
        "grep" | "search_tool" => "search",
        "web_fetch" => "fetch",
        _ => "other",
    }
}

/// `run_terminal_command: cargo test`, `read_file: src/a.rs`: the tool and the
/// one argument that says what it did.
fn tool_title(name: &str, input: &Value) -> String {
    let key = match name {
        "run_terminal_command" => "command",
        "read_file" => "target_file",
        "search_replace" | "write" => "file_path",
        "list_dir" => "target_directory",
        "grep" => "pattern",
        "search_tool" => "query",
        "web_fetch" => "url",
        "use_tool" => "tool_name",
        "spawn_subagent" => "description",
        _ => "",
    };
    titled(
        name,
        input.get(key).and_then(Value::as_str).unwrap_or_default(),
    )
}

fn titled(name: &str, arg: &str) -> String {
    let first_line = arg.lines().next().unwrap_or_default();
    if first_line.is_empty() {
        return name.to_owned();
    }
    format!("{name}: {}", clean(first_line, MAX_TITLE_ARG_CHARS))
}

#[cfg(test)]
mod tests;
