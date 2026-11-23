use serde_json::{Value, json};

use super::*;

fn user(id: &str, text: &str) -> Value {
    json!({"sessionUpdate": "user_message_chunk", "messageId": id, "content": {"type": "text", "text": text}})
}

fn agent(id: &str, text: &str) -> Value {
    json!({"sessionUpdate": "agent_message_chunk", "messageId": id, "content": {"type": "text", "text": text}})
}

fn thought(id: &str) -> Value {
    json!({"sessionUpdate": "agent_thought_chunk", "messageId": id, "content": {"type": "text", "text": "hm"}})
}

fn tool() -> Value {
    json!({"sessionUpdate": "tool_call", "toolCallId": "t1", "title": "Bash", "status": "pending"})
}

// Catches: a question lost behind tool calls, or the answer built from the
// interim narration instead of the final message.
#[test]
fn finds_the_question_and_the_final_reply_across_tool_calls() {
    let updates = vec![
        user("u1", "Why is the build slow?"),
        agent("m1", "Let me check."),
        tool(),
        thought("m2"),
        agent("m2", "The cache was cold."),
    ];
    assert_eq!(
        last_exchange(&updates),
        Some(Exchange {
            message_id: "u1".into(),
            question: "Why is the build slow?".into(),
            answer: "The cache was cold.".into()
        })
    );
}

// Catches: the idle that fires between Claude's thinking record and its text
// record filing nothing forever — the read must see "no answer yet", so the
// caller retries, and must not file the thought as the answer.
#[test]
fn a_turn_with_only_its_thinking_written_has_no_exchange_yet() {
    let updates = vec![user("u1", "Is it done?"), thought("m1")];
    assert_eq!(last_exchange(&updates), None);
}

// Catches: filing a half-finished turn on an idle that fired during a long tool.
#[test]
fn a_turn_still_running_files_nothing() {
    let updates = vec![user("u1", "Is it done?"), agent("m1", "Checking."), tool()];
    assert_eq!(last_exchange(&updates), None);
}

// Catches: every instruction filed as a question, and harness text read as
// the user's words.
#[test]
fn only_human_questions_count() {
    assert_eq!(
        last_exchange(&[user("u1", "Fix the parser."), agent("m1", "Done.")]),
        None
    );
    assert_eq!(
        last_exchange(&[
            user("u1", "<command-name>/model</command-name> which?"),
            agent("m1", "Opus.")
        ]),
        None
    );
}

// Catches: the journal answer keeping the terminal protocol markers.
#[test]
fn the_answer_is_filed_without_protocol_markers() {
    let updates = vec![
        user("u1", "Only with Claude?"),
        agent(
            "m1",
            "intent: Checking (Gate)\n💬 No.\n\nsuggest: [ A | B | C ]",
        ),
    ];
    assert_eq!(
        last_exchange(&updates).map(|e| e.answer),
        Some("No.".into())
    );
}

// Catches: a model-change card glued to the answer, or read as "turn still running".
#[test]
fn a_notice_card_is_neither_answer_nor_a_running_turn() {
    let card = json!({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "Model changed"},
        "_meta": {"ego": {"salience": "card"}}});
    let updates = vec![user("u1", "Which model?"), agent("m1", "Opus."), card];
    assert_eq!(
        last_exchange(&updates).map(|e| e.answer),
        Some("Opus.".into())
    );
}

// Catches: a user message without an id (no dedup key) filed on every idle.
#[test]
fn a_question_without_a_message_id_is_not_filed() {
    let updates = vec![
        json!({"sessionUpdate": "user_message_chunk", "content": {"type": "text", "text": "Why?"}}),
        agent("m1", "Because."),
    ];
    assert_eq!(last_exchange(&updates), None);
}

// Catches: a terminal with no exact binding filing a guessed transcript's
// exchange, or failing loudly on every idle.
#[test]
fn a_terminal_without_a_binding_files_nothing() {
    let dir = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    assert_eq!(capture(&state, "no-such-terminal"), Ok(false));
    let journal = ProgressStore::open().unwrap();
    assert_eq!(
        journal.recent(&Default::default()).unwrap(),
        Vec::<crate::progress::ProgressRecentEntry>::new()
    );
}

// Catches: a second session's question dropped because its raw message id
// (Grok's `grok-prompt-0`) equals one already filed by another session.
#[test]
fn sessions_sharing_a_raw_message_id_each_file_their_question() {
    let dir = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
    let journal = ProgressStore::open().unwrap();
    let file = |session: &str| {
        let q = NewQuestion {
            project: "/p",
            pty_id: session,
            agent_name: None,
            question: "Why?",
        };
        store::record_direct(
            &journal,
            &q,
            "Because.",
            &source_id(session, "grok-prompt-0"),
        )
        .unwrap()
    };
    assert!(file("session-a").is_some());
    assert!(file("session-b").is_some(), "second session must file");
    assert!(file("session-a").is_none(), "same session still dedups");
}
