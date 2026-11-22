use std::path::PathBuf;

use super::codex::{Codex, Totals, parse_events, rollout_state};
use super::wrap::{wrap_follow_up, wrap_question};
use super::{Adapter, Exchange, ForkTarget, Turn};

/// `codex exec fork --ephemeral --json` of a session in the middle of a tool
/// call (codex-cli 0.169.0, 2026-11-21). The main session's totals were
/// 214,573 input and 172,160 cached when the fork started.
const MID_TOOL_CALL: &str = include_str!("fixtures/codex_fork_mid_tool_call.jsonl");
/// `turn_context` and two `token_count` records of that main session's rollout.
const ROLLOUT: &str = include_str!("fixtures/codex_rollout_excerpt.jsonl");

const INHERITED: Totals = Totals {
    input: 214_573,
    cached_input: 172_160,
    output: 390,
};

fn target(model: Option<&str>) -> ForkTarget {
    ForkTarget {
        program: PathBuf::from("codex"),
        cwd: PathBuf::from("."),
        env: Vec::new(),
        main_session_id: "main-id".to_owned(),
        model: model.map(str::to_owned),
    }
}

fn exchange(question: &str, answer: &str) -> Exchange {
    Exchange {
        question: question.to_owned(),
        answer: answer.to_owned(),
    }
}

// --- parser ---------------------------------------------------------------

#[test]
fn usage_is_the_fork_turn_only_not_the_inherited_thread_totals() {
    let out = parse_events(MID_TOOL_CALL, INHERITED).unwrap();
    // turn.completed reports 459,035 input and 403,328 cached for the thread.
    assert_eq!(out.usage.cache_read_tokens, 231_168);
    assert_eq!(
        out.usage.input_tokens, 13_294,
        "uncached input: 244,462 less the cached part"
    );
    assert_eq!(out.usage.output_tokens, 388);
    assert_eq!(out.usage.cache_creation_tokens, 0);
    assert_eq!(out.usage.cost_usd, None);
}

#[test]
fn answer_is_the_last_agent_message_and_the_id_is_the_fork_thread() {
    let out = parse_events(MID_TOOL_CALL, INHERITED).unwrap();
    assert_eq!(out.fork_session_id, "01a1fe4b-f0d2-7542-8a4e-1eb81b6cd781");
    assert!(
        out.text
            .starts_with("💬 La parola segreta resta riservata."),
        "{}",
        out.text
    );
    assert!(
        !out.text.contains("intent:"),
        "first message is commentary: {}",
        out.text
    );
}

#[test]
fn a_run_without_a_completed_turn_is_an_error() {
    // A missing session prints nothing on stdout (the reason is on stderr).
    assert!(
        parse_events("", INHERITED)
            .unwrap_err()
            .contains("turn.completed")
    );
    let failed = "{\"type\":\"thread.started\",\"thread_id\":\"t\"}\n\
{\"type\":\"turn.failed\",\"error\":{\"message\":\"usage limit reached\"}}\n";
    assert!(
        parse_events(failed, INHERITED)
            .unwrap_err()
            .contains("usage limit reached")
    );
}

// --- command --------------------------------------------------------------

#[test]
fn every_turn_is_an_ephemeral_read_only_fork_of_the_main_session() {
    let follow_up = Turn::FollowUp {
        fork_session_id: "fork-id".to_owned(),
        prior: Vec::new(),
    };
    for turn in [Turn::NewFork, follow_up] {
        let args = Codex {
            inherited: INHERITED,
        }
        .args(&target(Some("gpt-6.2-sol")), &turn, "<q>");
        assert_eq!(&args[..3], ["exec", "fork", "--ephemeral"]);
        assert_eq!(
            &args[args.len() - 2..],
            ["main-id", "<q>"],
            "session id then prompt, last"
        );
        let joined = args.join(" ");
        assert!(joined.contains("-c sandbox_mode=\"read-only\""), "{joined}");
        assert!(joined.contains("--disable hooks"), "{joined}");
        assert!(
            joined.contains("-m gpt-6.2-sol") && joined.contains("--json"),
            "{joined}"
        );
        // An ephemeral fork leaves nothing to resume.
        assert!(
            !joined.contains("fork-id") && !joined.contains("resume"),
            "{joined}"
        );
    }
    let args = Codex {
        inherited: INHERITED,
    }
    .args(&target(None), &Turn::NewFork, "<q>");
    assert!(
        !args.contains(&"-m".to_owned()),
        "no model known: CLI default"
    );
}

// --- follow-up prompt -----------------------------------------------------

#[test]
fn follow_up_prompt_replays_the_thread_before_the_new_question() {
    let prior = [
        exchange("Why does it wait?", "For the lock."),
        exchange("Which lock?", "The store lock."),
    ];
    let prompt = wrap_follow_up(&prior, "Who holds it?");
    let at = |needle: &str| {
        prompt
            .find(needle)
            .unwrap_or_else(|| panic!("{needle} missing in {prompt}"))
    };
    assert!(
        at("Q: Why does it wait?\nA: For the lock.") < at("Q: Which lock?\nA: The store lock.")
    );
    assert!(at("The store lock.") < at("Question:\nWho holds it?"));
    assert!(prompt.starts_with("<side-question>") && prompt.ends_with("</side-question>"));
}

#[test]
fn follow_up_prompt_without_answered_turns_is_the_plain_question() {
    assert_eq!(wrap_follow_up(&[], "And?"), wrap_question("And?"));
}

#[test]
fn follow_up_prompt_drops_the_oldest_exchanges_past_the_budget() {
    let long = "x".repeat(6_000);
    let prior = [
        exchange("first", &long),
        exchange("second", &long),
        exchange("third", "short"),
    ];
    let prompt = wrap_follow_up(&prior, "And?");
    assert!(
        prompt.contains("Q: third") && prompt.contains("Q: second"),
        "newest kept"
    );
    assert!(!prompt.contains("Q: first"), "oldest dropped");
    assert!(
        prompt.chars().count() < 10_000,
        "{}",
        prompt.chars().count()
    );
}

// --- rollout --------------------------------------------------------------

#[test]
fn rollout_gives_the_model_cwd_and_newest_totals() {
    let dir = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
    let path = dir.path().join("rollout.jsonl");
    // A tool output that quotes both record names must not be read as one.
    let decoy = "{\"type\":\"response_item\",\"payload\":{\"type\":\"custom_tool_call_output\",\
\"output\":\"turn_context token_count\",\"model\":\"decoy\",\"info\":{\"total_token_usage\":{\"input_tokens\":1}}}}\n";
    std::fs::write(&path, format!("{ROLLOUT}{decoy}")).unwrap();
    let state = rollout_state(&path).unwrap();
    assert_eq!(state.model.as_deref(), Some("gpt-6.2-sol"));
    assert_eq!(
        state.cwd,
        Some(PathBuf::from("/Users/me/Gits/.tmp/disc-codex-live"))
    );
    assert_eq!(state.totals, INHERITED);
}
