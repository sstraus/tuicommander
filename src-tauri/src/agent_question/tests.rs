//! Ledger and priority rules over the recorded transcripts in
//! `fixtures/agent_question` (see its README for provenance).

use super::*;
use crate::chat_view::{claude, codex};

const CLAUDE_ANSWER: &str =
    include_str!("../fixtures/agent_question/claude-answer-then-reopen-2.1.316.jsonl");
const CLAUDE_ESCAPE: &str = include_str!("../fixtures/agent_question/claude-escape-2.1.316.jsonl");
const CODEX_ANSWER: &str = include_str!("../fixtures/agent_question/codex-answered-0.166.0.jsonl");
const CODEX_ABORT: &str =
    include_str!("../fixtures/agent_question/codex-interrupted-0.166.0.jsonl");

const CLAUDE_SESSION: &str = "c73f5d6b-3f0a-4394-bd61-5e13f445d3bd";
const ESCAPE_SESSION: &str = "3460adb4-584b-4837-9c25-2651270be1d2";
const FIRST_CALL: &str = "toolu_01Bb9j3EnCkaRy6YnKADfFYh";
const SECOND_CALL: &str = "toolu_01BzfoG3Fo2GZqVG7Xv6j6Rn";
const PROBE_QUESTIONS: &str =
    "Which color should the probe use?\n\nWhich size should the probe use?";

/// The first `n` rows of a recording: a real earlier state of an append-only file.
fn prefix(file: &str, n: usize) -> Vec<&str> {
    file.lines().take(n).collect()
}

fn claude_ledger(lines: &[&str], session: &str) -> QuestionLedger {
    let mut ledger = QuestionLedger::default();
    for line in lines {
        for row in claude::question_rows(line, session) {
            ledger.absorb(row);
        }
    }
    ledger
}

fn codex_ledger(lines: &[&str]) -> QuestionLedger {
    let mut ledger = QuestionLedger::default();
    for line in lines {
        for row in codex::question_rows(line) {
            ledger.absorb(row);
        }
    }
    ledger
}

// Catches: an AskUserQuestion call parsed with the wrong id or text, so a live
// hook wait can never be named from the transcript.
#[test]
fn claude_incomplete_call_names_the_matching_request() {
    let ledger = claude_ledger(&prefix(CLAUDE_ANSWER, 2), CLAUDE_SESSION);
    assert_eq!(ledger.question(Some(FIRST_CALL)), Some(PROBE_QUESTIONS));
    assert_eq!(ledger.question(None), Some(PROBE_QUESTIONS));
    assert_eq!(
        ledger.question(Some(SECOND_CALL)),
        None,
        "a request mismatch"
    );
}

// Catches: Claude's request replay row (`api-request-blob`) counted as a new call
// after the answer.
#[test]
fn claude_answer_and_its_replay_keep_the_call_settled() {
    let ledger = claude_ledger(&prefix(CLAUDE_ANSWER, 5), CLAUDE_SESSION);
    assert_eq!(ledger.question(Some(FIRST_CALL)), None);
    assert_eq!(ledger.question(None), None);
    let replay_only = claude_ledger(&prefix(CLAUDE_ANSWER, 4)[3..], CLAUDE_SESSION);
    assert_eq!(replay_only.question(None), None, "a replay is not a call");
}

// Catches: a settled call of an earlier turn surviving next to the new one, which
// makes the sole-question rule ambiguous or names the old call.
#[test]
fn claude_new_turn_opens_only_the_new_call() {
    let ledger = claude_ledger(&prefix(CLAUDE_ANSWER, 7), CLAUDE_SESSION);
    assert_eq!(ledger.question(Some(SECOND_CALL)), Some(PROBE_QUESTIONS));
    assert_eq!(ledger.question(Some(FIRST_CALL)), None);
    assert_eq!(ledger.question(None), Some(PROBE_QUESTIONS));
}

// Catches: an `is_error` tool result (Escape) left open because only a successful
// answer settles a call.
#[test]
fn claude_cancelled_call_is_settled() {
    assert!(
        claude_ledger(&prefix(CLAUDE_ESCAPE, 2), ESCAPE_SESSION)
            .question(None)
            .is_some()
    );
    assert_eq!(
        claude_ledger(&prefix(CLAUDE_ESCAPE, 4), ESCAPE_SESSION).question(None),
        None
    );
}

// Catches: rows of another session in the same file read as this terminal's.
#[test]
fn claude_rows_of_another_session_never_count() {
    let ledger = claude_ledger(&prefix(CLAUDE_ANSWER, 2), ESCAPE_SESSION);
    assert_eq!(ledger.question(None), None);
}

// Catches: a row cut by a concurrent append parsed as a call, or a panic on it.
#[test]
fn torn_row_opens_nothing_until_it_is_complete() {
    let call = prefix(CLAUDE_ANSWER, 2)[1];
    let torn = &call[..call.len() / 2];
    assert_eq!(claude::question_rows(torn, CLAUDE_SESSION).len(), 0);
    let codex_call = prefix(CODEX_ANSWER, 15)[14];
    assert_eq!(
        codex::question_rows(&codex_call[..codex_call.len() / 2]).len(),
        0
    );
    let ledger = claude_ledger(&[torn, call], CLAUDE_SESSION);
    assert_eq!(ledger.question(Some(FIRST_CALL)), Some(PROBE_QUESTIONS));
}

// Catches: the Codex `request_user_input` arguments (a JSON string) not decoded,
// or its `function_call_output` not settling the call.
#[test]
fn codex_question_is_open_until_its_output() {
    let open = codex_ledger(&prefix(CODEX_ANSWER, 15));
    assert_eq!(open.question(Some("question-probe")), Some("Pick a color"));
    assert_eq!(codex_ledger(&prefix(CODEX_ANSWER, 20)).question(None), None);
    assert!(
        codex_ledger(&prefix(CODEX_ABORT, 5))
            .question(None)
            .is_some()
    );
    assert_eq!(codex_ledger(&prefix(CODEX_ABORT, 8)).question(None), None);
}

// Catches: a call left open by a lost output row surviving into the next turn.
#[test]
fn codex_next_turn_settles_a_stale_call() {
    let mut lines = prefix(CODEX_ABORT, 5);
    lines.push(prefix(CODEX_ANSWER, 2)[1]);
    assert_eq!(codex_ledger(&lines).question(None), None);
}

// Catches: question parsing leaking into the completed-only Codex chat projection
// as an invented tool card.
#[test]
fn codex_chat_projection_gains_no_question_row() {
    let mut adapter = codex::CodexAdapter::default();
    let updates: Vec<_> = prefix(CODEX_ANSWER, 15)
        .into_iter()
        .flat_map(|line| adapter.absorb(line))
        .collect();
    assert!(
        !updates.iter().any(
            |u| u["toolCallId"] == "question-probe" || u["sessionUpdate"] == "tool_call_update"
        ),
        "{updates:#?}"
    );
}

// Catches: two open calls without a request resolved to one of them by order.
#[test]
fn two_open_calls_without_a_request_are_ambiguous() {
    let mut ledger = QuestionLedger::default();
    for id in ["a", "b"] {
        ledger.absorb(Row::Open {
            id: id.into(),
            text: format!("question {id}"),
        });
    }
    assert_eq!(ledger.question(None), None);
    assert_eq!(ledger.question(Some("b")), Some("question b"));
}

// Catches: a settled id reopened by a later row that repeats the call.
#[test]
fn a_settled_call_never_reopens() {
    let mut ledger = QuestionLedger::default();
    let open = || Row::Open {
        id: "a".into(),
        text: "question".into(),
    };
    ledger.absorb(open());
    ledger.absorb(Row::Settled("a".into()));
    ledger.absorb(open());
    assert_eq!(ledger.question(None), None);
}

// Catches: the transcript overriding the text a structured hook event carried, or
// being read when the hook text is already final.
#[test]
fn structured_text_wins_and_skips_the_transcript() {
    let read = || -> Option<String> { panic!("the transcript must not be read") };
    assert_eq!(recovered_text(Some("Pick a color"), Some("id"), read), None);
    let transcript = || Some("from transcript".to_owned());
    assert_eq!(
        recovered_text(Some(""), Some("id"), transcript).as_deref(),
        Some("from transcript"),
        "an empty hook question is named from the transcript"
    );
    assert_eq!(
        recovered_text(Some("screen text"), None, transcript).as_deref(),
        Some("from transcript"),
        "the transcript outranks the screen fallback"
    );
    assert_eq!(recovered_text(Some("screen text"), None, || None), None);
}
