use super::CodexAdapter;
use serde_json::Value;

const RECORDED: &str = include_str!("../../fixtures/chat_view/codex/completed.jsonl");

// Catches: losing completed Codex messages, or emitting response_item duplicates.
#[test]
fn codex_recorded_completed_items_emit_each_message_once() {
    let mut adapter = CodexAdapter::default();
    let updates: Vec<Value> = RECORDED
        .lines()
        .flat_map(|line| adapter.absorb(line))
        .collect();
    let messages: Vec<_> = updates
        .iter()
        .filter(|u| {
            matches!(
                u["sessionUpdate"].as_str(),
                Some("user_message_chunk" | "agent_message_chunk")
            )
        })
        .map(|u| {
            (
                u["sessionUpdate"].as_str().unwrap(),
                u["content"]["text"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        messages,
        [
            ("user_message_chunk", "Recorded prompt"),
            ("agent_message_chunk", "Recorded reply")
        ]
    );
}

fn recorded_item(kind: &str) -> Value {
    RECORDED
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|row| row["payload"]["item"]["type"] == kind)
        .unwrap()
}

// Catches: terminal tool cards losing their arguments, output, or final status.
#[test]
fn codex_recorded_tools_keep_titles_raw_input_and_output() {
    let mut adapter = CodexAdapter::default();
    let command = adapter.absorb(&recorded_item("CommandExecution").to_string());
    assert_eq!(command.len(), 1);
    assert_eq!(command[0]["title"], "command: printf recorded");
    assert_eq!(command[0]["status"], "completed");
    assert_eq!(command[0]["rawInput"]["command"], "printf recorded");
    assert_eq!(
        command[0]["content"][0]["content"]["text"],
        "recorded command output"
    );
    let tool = adapter.absorb(&recorded_item("McpToolCall").to_string());
    assert_eq!(tool.len(), 1);
    assert_eq!(tool[0]["title"], "read: src/example.rs");
    assert_eq!(tool[0]["rawInput"]["path"], "src/example.rs");
    assert_eq!(
        tool[0]["content"][0]["content"]["text"],
        "recorded tool output"
    );
    let thought = adapter.absorb(&recorded_item("Reasoning").to_string());
    assert_eq!(thought[0]["sessionUpdate"], "agent_thought_chunk");
    assert_eq!(thought[0]["content"]["text"], "Recorded reasoning");
}

// Catches: running tools appearing as completed or failed tools appearing successful.
#[test]
fn codex_incomplete_tools_are_hidden_and_failures_are_preserved() {
    for kind in ["CommandExecution", "McpToolCall"] {
        let mut adapter = CodexAdapter::default();
        let mut row = recorded_item(kind);
        row["payload"]["item"]["status"] = "in_progress".into();
        assert_eq!(
            adapter.absorb(&row.to_string()),
            [] as [serde_json::Value; 0]
        );
        row["payload"]["item"]["status"] = "failed".into();
        assert_eq!(adapter.absorb(&row.to_string())[0]["status"], "failed");
        row["payload"]["item"]["status"] = "completed".into();
        if kind == "CommandExecution" {
            row["payload"]["item"]["exit_code"] = 1.into();
        } else {
            row["payload"]["item"]["result"]["isError"] = true.into();
        }
        assert_eq!(adapter.absorb(&row.to_string())[0]["status"], "failed");
        row["payload"]["type"] = "item_started".into();
        assert_eq!(
            adapter.absorb(&row.to_string()),
            [] as [serde_json::Value; 0]
        );
    }
}

// Catches: corrupted rows blanking later output, or harness context leaking as chat.
#[test]
fn codex_bad_rows_do_not_stop_later_messages_or_expose_hook_context() {
    let mut adapter = CodexAdapter::default();
    assert_eq!(adapter.absorb("{broken"), [] as [serde_json::Value; 0]);
    assert_eq!(
        adapter.absorb(r#"{"type":"future_row"}"#),
        [] as [serde_json::Value; 0]
    );
    assert_eq!(
        adapter.absorb(&recorded_item("HookPrompt").to_string()),
        [] as [serde_json::Value; 0]
    );
    let update = adapter.absorb(&recorded_item("AgentMessage").to_string());
    assert_eq!(update[0]["content"]["text"], "Recorded reply");
    assert_eq!(adapter.stats.malformed_rows, 1);
    assert_eq!(adapter.stats.unknown_rows, 1);
}

// Catches: Codex output bypassing the shared redaction or per-field size bounds.
#[test]
fn codex_text_and_tool_fields_redact_secrets_before_truncation() {
    let mut adapter = CodexAdapter::default();
    let secret = "AKIAIOSFODNN7EXAMPLE";
    let mut row = recorded_item("McpToolCall");
    let text = format!("{secret} {}", "é".repeat(25_000));
    row["payload"]["item"]["arguments"]["path"] = text.clone().into();
    row["payload"]["item"]["result"]["content"][0]["text"] = text.clone().into();
    let update = adapter.absorb(&row.to_string());
    assert!(!update[0].to_string().contains(secret));
    assert_eq!(
        update[0]["rawInput"]["path"]
            .as_str()
            .unwrap()
            .chars()
            .count(),
        1_000
    );
    assert_eq!(
        update[0]["content"][0]["content"]["text"]
            .as_str()
            .unwrap()
            .chars()
            .count(),
        4_000
    );
    assert!(update[0]["title"].as_str().unwrap().chars().count() <= 126);
    let mut row = recorded_item("AgentMessage");
    row["payload"]["item"]["content"][0]["text"] = text.into();
    let update = adapter.absorb(&row.to_string());
    assert!(!update[0].to_string().contains(secret));
    assert_eq!(
        update[0]["content"]["text"]
            .as_str()
            .unwrap()
            .chars()
            .count(),
        20_000
    );
}
