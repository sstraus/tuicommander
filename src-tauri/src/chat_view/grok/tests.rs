use super::*;

/// Real rows of a Grok session, payloads replaced by placeholders.
const SESSION: &str = include_str!("../../fixtures/chat_view/grok_chat_history.jsonl");

fn run(jsonl: &str) -> (Vec<Value>, GrokAdapter) {
    let mut adapter = GrokAdapter::default();
    let updates = jsonl.lines().flat_map(|l| adapter.absorb(l)).collect();
    (updates, adapter)
}

fn of_kind<'a>(updates: &'a [Value], kind: &str) -> Vec<&'a Value> {
    updates
        .iter()
        .filter(|u| u["sessionUpdate"] == kind)
        .collect()
}

// Catches: the harness context rows Grok stores as user turns (`<user_info>`, system
// reminders) shown as things the human typed, or the human prompt missing.
#[test]
fn grok_only_the_human_prompt_is_a_user_turn() {
    let (updates, _) = run(SESSION);
    let users = of_kind(&updates, "user_message_chunk");
    assert_eq!(users.len(), 1, "one prompt, three harness rows dropped");
    assert_eq!(users[0]["content"]["text"], "PLACEHOLDER human prompt");
}

// Catches: tool calls missing from the chat, or a call that never gets its result.
#[test]
fn grok_every_tool_call_becomes_a_card_with_its_result() {
    let (updates, _) = run(SESSION);
    let calls = of_kind(&updates, "tool_call");
    assert_eq!(calls.len(), 7, "five calls in one row, two in the next");
    assert_eq!(calls[0]["title"], "read_file: PLACEHOLDER_target_file");
    assert_eq!(calls[0]["kind"], "read");
    assert_eq!(calls[3]["title"], "grep: PLACEHOLDER_pattern");
    assert_eq!(calls[3]["kind"], "search");
    let results = of_kind(&updates, "tool_call_update");
    assert_eq!(results.len(), 7);
    // Grok writes results in completion order, not call order.
    for call in &calls {
        let result = results
            .iter()
            .find(|r| r["toolCallId"] == call["toolCallId"])
            .expect("a result for every call");
        assert_eq!(result["status"], "completed");
        assert_eq!(
            result["content"][0]["content"]["text"],
            "PLACEHOLDER tool output"
        );
    }
}

// Catches: successive assistant rows glued into one bubble because they carry no
// message id (the projection joins chunks whose id is absent).
#[test]
fn grok_each_assistant_row_is_its_own_message() {
    let (updates, _) = run(SESSION);
    let texts = of_kind(&updates, "agent_message_chunk");
    assert_eq!(
        texts.len(),
        2,
        "the row with tools has text, the row without is silent"
    );
    assert!(texts.iter().all(|u| u["messageId"].is_string()));
    assert_ne!(texts[0]["messageId"], texts[1]["messageId"]);
}

#[test]
fn grok_reasoning_summary_is_a_thought_and_comes_before_the_reply() {
    let (updates, _) = run(SESSION);
    let first_thought = updates
        .iter()
        .position(|u| u["sessionUpdate"] == "agent_thought_chunk")
        .expect("a thought");
    let first_reply = updates
        .iter()
        .position(|u| u["sessionUpdate"] == "agent_message_chunk")
        .expect("a reply");
    assert!(first_thought < first_reply);
    assert_eq!(
        updates[first_thought]["content"]["text"],
        "PLACEHOLDER reasoning summary"
    );
}

// Catches: a result whose call is outside the window rendered as an empty card
// titled with the raw id.
#[test]
fn grok_orphan_tool_result_is_dropped() {
    let line = r#"{"type":"tool_result","tool_call_id":"call-gone","content":"x"}"#;
    let (updates, adapter) = run(line);
    assert_eq!(updates, [] as [serde_json::Value; 0]);
    assert_eq!(adapter.stats.unknown_rows, 0);
}

// Catches: the interruption wrapper shown to the human, or a task-completion
// notice (which carries `prompt_index` too) shown as a typed prompt.
#[test]
fn grok_interrupt_prompt_is_unwrapped_and_synthetic_turns_are_dropped() {
    let rows = [
        r#"{"type":"user","content":[{"type":"text","text":"The user interrupted the previous turn:\n<user_query>\nstop that\n</user_query>\nMake sure to complete any unfinished tasks from previous turns."}],"prior_turn_interrupt":"mid_turn_abort","prompt_index":2}"#,
        r#"{"type":"user","content":[{"type":"text","text":"<system-reminder>\nBackground task done\n</system-reminder>"}],"synthetic_reason":"task_completed","prompt_index":3}"#,
        r#"{"type":"user","content":[{"type":"text","text":"Reply with exactly OK."}],"prompt_index":4}"#,
    ];
    let (updates, _) = run(&rows.join("\n"));
    let texts: Vec<&str> = updates
        .iter()
        .map(|u| u["content"]["text"].as_str().unwrap())
        .collect();
    assert_eq!(texts, ["stop that", "Reply with exactly OK."]);
}

#[test]
fn grok_compaction_shows_a_notice_card() {
    let line = r#"{"type":"user","content":[{"type":"text","text":"<user_info>x</user_info>"}],"synthetic_reason":"compaction_meta"}"#;
    let (updates, _) = run(line);
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0]["content"]["text"], "Conversation compacted");
    assert_eq!(updates[0]["_meta"]["ego"]["salience"], "card");
}

// Catches: Grok's server-side web search silently missing from the tool list.
#[test]
fn grok_backend_web_search_is_a_completed_card() {
    let line = r#"{"type":"backend_tool_call","kind":{"tool_type":"web_search","action":{"type":"search","query":"rust jsonl","sources":[]}}}"#;
    let (updates, _) = run(line);
    let calls = of_kind(&updates, "tool_call");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0]["title"], "web_search: rust jsonl");
    assert_eq!(calls[0]["status"], "completed");
}

// Catches: dropping secret-bearing tool cards/results, or leaking the token in
// a title, raw input or result while still returning the other redacted fields.
#[test]
fn grok_secret_bearing_tool_card_and_result_survive_with_redacted_content() {
    let secret = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";
    let call = json!({
        "type": "assistant", "content": "", "tool_calls": [{
            "id": "c1", "name": "run_terminal_command",
            "arguments": json!({ "command": format!("echo {secret} safe") }).to_string(),
        }],
    });
    let result = json!({
        "type": "tool_result", "tool_call_id": "c1",
        "content": format!("output: {secret} safe"),
    });
    let (updates, _) = run(&format!("{call}\n{result}"));
    assert_eq!(updates.len(), 2, "the call and its result must survive");
    assert_eq!(updates[0]["sessionUpdate"], "tool_call");
    assert_eq!(
        updates[0]["title"],
        "run_terminal_command: echo [REDACTED] safe"
    );
    assert_eq!(updates[0]["rawInput"]["command"], "echo [REDACTED] safe");
    assert_eq!(updates[1]["sessionUpdate"], "tool_call_update");
    assert_eq!(updates[1]["toolCallId"], "c1");
    assert_eq!(
        updates[1]["content"][0]["content"]["text"],
        "output: [REDACTED] safe"
    );
    assert!(!serde_json::to_string(&updates).unwrap().contains(secret));
}

#[test]
fn grok_unknown_and_malformed_rows_are_counted_not_fatal() {
    let rows = [
        r#"{"type":"future_row","x":1}"#,
        "{ not json",
        r#"{"type":"system","content":"s"}"#,
    ];
    let (updates, adapter) = run(&rows.join("\n"));
    assert_eq!(updates, [] as [serde_json::Value; 0]);
    assert_eq!(adapter.stats.unknown_rows, 1, "system is plumbing");
    assert_eq!(adapter.stats.malformed_rows, 1);
}
