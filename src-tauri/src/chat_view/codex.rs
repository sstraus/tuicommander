//! Completed Codex rollout items -> the existing ACP transcript updates.
//!
//! `response_item` repeats the same conversation and is deliberately ignored.
//! This parser does not establish ownership of a rollout: callers must resolve
//! an exact terminal binding before reading it.

use serde_json::{Value, json};

use super::claude::{AdapterStats, MAX_TEXT_CHARS, cap_strings, clean};

#[derive(Default)]
pub(crate) struct CodexAdapter {
    pub stats: AdapterStats,
}

impl CodexAdapter {
    pub(crate) fn absorb(&mut self, line: &str) -> Vec<Value> {
        if line.trim().is_empty() {
            return Vec::new();
        }
        let Ok(row) = serde_json::from_str::<Value>(line) else {
            self.stats.malformed_rows += 1;
            return Vec::new();
        };
        match row["type"].as_str() {
            Some("event_msg") => {}
            Some(
                "response_item" | "session_meta" | "turn_context" | "token_usage_record"
                | "world_state" | "compacted",
            ) => return Vec::new(),
            _ => {
                self.stats.unknown_rows += 1;
                return Vec::new();
            }
        }
        let payload = &row["payload"];
        if payload["type"] != "item_completed" {
            return Vec::new();
        }
        let item = &payload["item"];
        let Some(id) = item["id"].as_str().filter(|id| !id.is_empty()) else {
            self.stats.malformed_rows += 1;
            return Vec::new();
        };
        match item["type"].as_str() {
            Some("UserMessage" | "AgentMessage") => {
                let text = item["content"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|block| match block["type"].as_str() {
                        Some("text" | "Text") => block["text"].as_str(),
                        Some("image" | "Image" | "local_image" | "LocalImage") => Some("[image]"),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                text_update(
                    if item["type"] == "UserMessage" {
                        "user_message_chunk"
                    } else {
                        "agent_message_chunk"
                    },
                    id,
                    &text,
                )
            }
            Some("Reasoning") => {
                let text = item["summary_text"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join("\n");
                text_update("agent_thought_chunk", id, &text)
            }
            Some("CommandExecution") => command(item, id).into_iter().collect(),
            Some("McpToolCall") => mcp_tool(item, id).into_iter().collect(),
            // Hook prompts are harness instructions, not human conversation.
            Some("HookPrompt") => Vec::new(),
            _ => {
                self.stats.unknown_rows += 1;
                Vec::new()
            }
        }
    }
}

fn text_update(kind: &str, id: &str, text: &str) -> Vec<Value> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    vec![json!({"sessionUpdate": kind, "messageId": id,
        "content": {"type": "text", "text": clean(text, MAX_TEXT_CHARS)}})]
}

fn terminal_status(item: &Value) -> Option<&'static str> {
    match item["status"].as_str()? {
        "completed" => Some("completed"),
        "failed" | "declined" => Some("failed"),
        // An item_completed envelope may still contain a running command.
        _ => None,
    }
}

fn command(item: &Value, id: &str) -> Option<Value> {
    let mut status = terminal_status(item)?;
    if item["exit_code"].as_i64().is_some_and(|code| code != 0) {
        status = "failed";
    }
    let argv: Vec<_> = item["command"]
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let command = match argv.as_slice() {
        [_, "-lc" | "-c", script] => (*script).to_owned(),
        _ => argv.join(" "),
    };
    Some(tool_update(
        id,
        "command",
        &command,
        "execute",
        status,
        json!({"command": command, "cwd": item["cwd"]}),
        item["aggregated_output"].as_str().unwrap_or_default(),
    ))
}

fn mcp_tool(item: &Value, id: &str) -> Option<Value> {
    let mut status = terminal_status(item)?;
    if item.pointer("/result/isError").and_then(Value::as_bool) == Some(true) {
        status = "failed";
    }
    let name = item["tool"].as_str()?;
    let input = &item["arguments"];
    let arg = [
        "command",
        "cmd",
        "file_path",
        "path",
        "pattern",
        "query",
        "url",
        "description",
        "action",
    ]
    .into_iter()
    .find_map(|key| input[key].as_str())
    .unwrap_or_default();
    let output = item
        .pointer("/result/content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|block| block["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let output = if output.is_empty() {
        item["error"].as_str().unwrap_or_default()
    } else {
        &output
    };
    Some(tool_update(
        id,
        name,
        arg,
        "other",
        status,
        input.clone(),
        output,
    ))
}

fn tool_update(
    id: &str,
    name: &str,
    arg: &str,
    kind: &str,
    status: &str,
    input: Value,
    output: &str,
) -> Value {
    let arg = clean(arg.lines().next().unwrap_or_default(), 120);
    let name = clean(name, 120);
    let title = if arg.is_empty() {
        name
    } else {
        format!("{name}: {arg}")
    };
    let mut update = json!({"sessionUpdate": "tool_call", "toolCallId": id,
        "title": title, "kind": kind, "status": status, "rawInput": cap_strings(&input)});
    if !output.is_empty() {
        update["content"] =
            json!([{"type": "content", "content": {"type": "text", "text": clean(output, 4_000)}}]);
    }
    update
}

#[cfg(test)]
mod tests;
