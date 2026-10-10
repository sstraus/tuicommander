use super::*;

const SESSION: &str = include_str!("../../fixtures/chat_view/claude_session.jsonl");

fn run(jsonl: &str) -> (Vec<Value>, ClaudeAdapter) {
    let mut adapter = ClaudeAdapter::default();
    let updates = jsonl.lines().flat_map(|l| adapter.absorb(l)).collect();
    (updates, adapter)
}

fn kinds(updates: &[Value]) -> Vec<&str> {
    updates
        .iter()
        .map(|u| u["sessionUpdate"].as_str().unwrap())
        .collect()
}

/// Rows of one API message share `message.id`; the transcript projection joins
/// consecutive chunks only when that id matches.
#[test]
fn claude_blocks_of_one_message_merge_into_one_bubble() {
    let (updates, _) = run(SESSION);
    let last_two: Vec<&Value> = updates
        .iter()
        .filter(|u| u["messageId"] == "msg_3")
        .collect();
    assert_eq!(last_two.len(), 2, "two text blocks of msg_3");
    assert_eq!(last_two[0]["messageId"], last_two[1]["messageId"]);
    assert!(
        last_two[1]["content"]["text"]
            .as_str()
            .unwrap()
            .starts_with("\n\n"),
        "the second block is separated from the first inside the joined bubble"
    );
    assert!(
        !last_two[0]["content"]["text"]
            .as_str()
            .unwrap()
            .starts_with('\n'),
        "the first block of a message has no leading gap"
    );
    let msg1: Vec<&Value> = updates
        .iter()
        .filter(|u| u["messageId"] == "msg_1")
        .collect();
    assert_eq!(msg1.len(), 1, "msg_1: empty thinking dropped, text kept");
}

#[test]
fn claude_empty_thinking_block_produces_no_thought() {
    let (updates, _) = run(SESSION);
    assert!(!kinds(&updates).contains(&"agent_thought_chunk"));
}

#[test]
fn claude_non_empty_thinking_is_a_thought() {
    let line = r#"{"type":"assistant","uuid":"x","message":{"id":"m","content":[{"type":"thinking","thinking":"PLACEHOLDER"}]}}"#;
    let (updates, _) = run(line);
    assert_eq!(kinds(&updates), ["agent_thought_chunk"]);
}

#[test]
fn claude_tool_result_is_error_marks_card_failed() {
    let (updates, _) = run(SESSION);
    let status = |id: &str| {
        updates
            .iter()
            .find(|u| u["sessionUpdate"] == "tool_call_update" && u["toolCallId"] == id)
            .map(|u| u["status"].as_str().unwrap().to_owned())
    };
    assert_eq!(status("toolu_1").as_deref(), Some("completed"));
    assert_eq!(status("toolu_2").as_deref(), Some("failed"));
}

#[test]
fn claude_tool_card_is_titled_by_tool_and_first_argument_line() {
    let (updates, _) = run(SESSION);
    let calls: Vec<&Value> = updates
        .iter()
        .filter(|u| u["sessionUpdate"] == "tool_call")
        .collect();
    assert_eq!(calls[0]["title"], "Bash: echo PLACEHOLDER");
    assert_eq!(calls[0]["kind"], "execute");
    assert_eq!(calls[1]["title"], "Read: /PLACEHOLDER/file.rs");
    assert_eq!(calls[1]["kind"], "read");
}

#[test]
fn claude_attachment_and_snapshot_rows_are_skipped() {
    let (updates, adapter) = run(SESSION);
    let text: String = updates.iter().map(|u| u.to_string()).collect();
    assert!(!text.contains("hook_success"));
    assert!(!text.contains("trackedFileBackups"));
    assert_eq!(adapter.stats.unknown_rows, 1, "only the future row type");
}

#[test]
fn claude_unknown_row_type_is_counted_not_fatal() {
    let (updates, adapter) = run(SESSION);
    assert_eq!(adapter.stats.unknown_rows, 1);
    assert!(
        kinds(&updates).contains(&"agent_message_chunk"),
        "rows after the unknown one still parse"
    );
    let mut adapter = ClaudeAdapter::default();
    assert!(adapter.absorb("{not json").is_empty());
    assert_eq!(adapter.stats.malformed_rows, 1);
}

#[test]
fn claude_human_prompt_row_becomes_user_entry() {
    let (updates, _) = run(SESSION);
    let users: Vec<&Value> = updates
        .iter()
        .filter(|u| u["sessionUpdate"] == "user_message_chunk")
        .collect();
    assert_eq!(users.len(), 1, "tool_result rows are not prompts");
    assert_eq!(users[0]["content"]["text"], "PLACEHOLDER prompt one");
}

/// A string-content user row without the human origin is harness text (a
/// slash-command echo, an injected reminder), not something the user typed.
#[test]
fn claude_string_user_row_without_human_origin_is_not_a_prompt() {
    let line = r#"{"type":"user","uuid":"x","message":{"role":"user","content":"<command-name>/clear</command-name>"}}"#;
    let (updates, _) = run(line);
    assert!(updates.is_empty());
}

/// A result whose call opened before the attach window would render as an
/// empty card titled with the raw tool id.
#[test]
fn claude_result_for_a_call_outside_the_window_is_dropped() {
    let line = r#"{"type":"user","uuid":"x","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_old","content":"PLACEHOLDER"}]}}"#;
    let (updates, _) = run(line);
    assert!(updates.is_empty());
}

/// A prompt with a pasted image is an ARRAY of blocks. Only tool results were
/// read from arrays, so such a prompt vanished from the chat.
#[test]
fn claude_prompt_with_an_image_is_not_dropped_from_the_chat() {
    let line = r#"{"type":"user","uuid":"u1","origin":{"kind":"human"},"message":{"role":"user","content":[{"type":"text","text":"what is this"},{"type":"image","source":{"type":"base64","media_type":"image/png","data":"AAAA"}}]}}"#;
    let (updates, _) = run(line);
    assert_eq!(kinds(&updates), ["user_message_chunk"]);
    assert_eq!(updates[0]["content"]["text"], "what is this\n[image]");
}

/// An array row that is only tool results (no human origin) stays no prompt.
#[test]
fn claude_array_row_without_human_origin_is_not_a_prompt() {
    let line = r#"{"type":"user","uuid":"u1","message":{"role":"user","content":[{"type":"text","text":"<system-reminder>x</system-reminder>"}]}}"#;
    let (updates, _) = run(line);
    assert!(updates.is_empty());
}

#[test]
fn claude_large_tool_output_is_cut() {
    let big = "x".repeat(12_000);
    let call = r#"{"type":"assistant","uuid":"a","message":{"id":"m","content":[{"type":"tool_use","id":"t","name":"Bash","input":{"command":"ls"}}]}}"#;
    let result = format!(
        r#"{{"type":"user","uuid":"u","message":{{"content":[{{"type":"tool_result","tool_use_id":"t","content":"{big}"}}]}}}}"#
    );
    let (updates, _) = run(&format!("{call}\n{result}"));
    let update = &updates[1];
    let text = update["content"][0]["content"]["text"].as_str().unwrap();
    assert_eq!(text.chars().count(), 4_000);
}

#[test]
fn claude_secrets_are_redacted_in_prompts() {
    let line = r#"{"type":"user","uuid":"x","origin":{"kind":"human"},"message":{"role":"user","content":"key AKIAIOSFODNN7EXAMPLE end"}}"#;
    let (updates, _) = run(line);
    assert!(
        !updates[0]["content"]["text"]
            .as_str()
            .unwrap()
            .contains("AKIAIOSFODNN7EXAMPLE")
    );
}

#[test]
fn claude_compaction_summary_becomes_a_card_without_the_summary_body() {
    let line = r#"{"type":"user","uuid":"x","isCompactSummary":true,"message":{"role":"user","content":"PLACEHOLDER long summary"}}"#;
    let (updates, _) = run(line);
    assert_eq!(updates[0]["_meta"]["ego"]["salience"], "card");
    assert!(!updates[0].to_string().contains("long summary"));
}

/// Recorded Claude queued image prompts must not disappear merely because
/// `attachment.prompt` is block content rather than a string.
#[test]
fn queued_human_image_prompt_is_not_silently_dropped() {
    let (updates, _) = run(include_str!(
        "../../fixtures/chat_view/recorded/queued-human-image.jsonl"
    ));
    assert_eq!(updates.len(), 1, "queued human image prompt disappeared");
    assert_eq!(updates[0]["sessionUpdate"], "user_message_chunk");
    assert_eq!(updates[0]["content"]["text"], "xxxxxxxxxx\n[image]");
}
