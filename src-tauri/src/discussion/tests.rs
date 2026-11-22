use std::path::PathBuf;
use std::time::{Duration, Instant};

use tokio::sync::oneshot;

use super::claude::{Claude, parse_stream, transcript_model_and_cwd};
use super::header::{Category, classify};
use super::{Adapter, ForkOutcome, ForkTarget, Turn, run};

const FOLLOW_UP: &str = include_str!("fixtures/claude_follow_up.jsonl");
const MID_TOOL_CALL: &str = include_str!("fixtures/claude_fork_mid_tool_call.jsonl");

fn target(model: Option<&str>) -> ForkTarget {
    ForkTarget {
        program: PathBuf::from("claude"),
        cwd: PathBuf::from("."),
        env: Vec::new(),
        main_session_id: "main-id".to_owned(),
        model: model.map(str::to_owned),
    }
}

// --- header ---------------------------------------------------------------

#[test]
fn header_sets_category_and_title_and_leaves_the_body_without_it() {
    let c = classify("[action] Stop the migration\n\nThe agent should revert step 2.");
    assert_eq!(c.category, Category::Action);
    assert_eq!(c.title, "Stop the migration");
    assert_eq!(c.body, "The agent should revert step 2.");
    assert!(c.header_found);
}

#[test]
fn header_in_bold_or_code_span_is_still_recognised() {
    assert_eq!(
        classify("**[unknown] Not in context**\nbody").category,
        Category::Unknown
    );
    assert_eq!(
        classify("`[Answer] Why it waits`\nbody").title,
        "Why it waits"
    );
}

#[test]
fn missing_or_unknown_header_falls_back_to_answer_with_first_line_title() {
    for text in [
        "It is waiting for the build.\nMore.",
        "[question] What\nMore.",
        "[answer]\nbody",
    ] {
        let c = classify(text);
        assert_eq!(c.category, Category::Answer, "{text}");
        assert!(!c.header_found, "{text}");
        assert_eq!(c.body, text.trim(), "fallback keeps the whole text: {text}");
    }
    assert_eq!(classify("It is waiting.\nMore.").title, "It is waiting.");
}

#[test]
fn long_multibyte_title_is_cut_on_a_char_boundary() {
    let c = classify(&format!("[answer] {}\nbody", "è".repeat(200)));
    assert_eq!(c.title.chars().count(), 81, "80 chars plus the ellipsis");
}

// --- stream-json ----------------------------------------------------------

#[test]
fn follow_up_cost_is_the_run_not_the_cumulative_chain() {
    let out = parse_stream(FOLLOW_UP).unwrap();
    assert_eq!(out.fork_session_id, "dabf3e9d-30c8-4ae4-b8d9-0ee8003738cc");
    assert_eq!(out.text, "Line 200 of `data.txt` is `line 200`.");
    // total_cost_usd in the fixture is 0.204: the parent's cost carried over.
    let cost = out.usage.cost_usd.unwrap();
    assert!((cost - 0.007356).abs() < 1e-9, "{cost}");
    assert_eq!(out.usage.cache_read_tokens, 46060);
    assert_eq!(out.usage.cache_creation_tokens, 633);
}

#[test]
fn without_run_usage_each_call_counts_once_not_once_per_block() {
    // The fixture's one call arrives as two assistant events (thinking, text).
    let stripped: String = MID_TOOL_CALL
        .lines()
        .map(|line| {
            let mut v: serde_json::Value = serde_json::from_str(line).unwrap();
            if let Some(o) = v.as_object_mut() {
                o.remove("model_usage_since_last_result");
            }
            v.to_string() + "\n"
        })
        .collect();
    let out = parse_stream(&stripped).unwrap();
    assert_eq!(out.usage.cache_read_tokens, 40326);
    assert_eq!(out.usage.cost_usd, None, "per-call usage carries no cost");
}

#[test]
fn error_result_and_missing_result_are_errors() {
    let error = r#"{"type":"result","subtype":"error_during_execution","is_error":true,"result":"","session_id":"x"}"#;
    assert!(
        parse_stream(error)
            .unwrap_err()
            .contains("error_during_execution")
    );
    let no_result = r#"{"type":"system","subtype":"init","session_id":"x"}"#;
    assert!(parse_stream(no_result).is_err());
    assert!(parse_stream("Warning: no stdin data received\n").is_err());
}

// --- command --------------------------------------------------------------

#[test]
fn new_fork_resumes_main_with_fork_session_plan_mode_and_the_main_model() {
    let args = Claude.args(&target(Some("claude-opus-5-5")), &Turn::NewFork, "Q");
    let joined = args.join(" ");
    assert!(
        joined.contains("--resume main-id --fork-session"),
        "{joined}"
    );
    assert!(joined.contains("--model claude-opus-5-5"), "{joined}");
    assert!(joined.contains("--permission-mode plan"), "{joined}");
    assert_eq!(&args[..2], ["-p", "Q"]);
}

#[test]
fn follow_up_resumes_the_fork_without_forking_again() {
    let turn = Turn::FollowUp {
        fork_session_id: "fork-id".to_owned(),
        prior: Vec::new(),
    };
    let args = Claude.args(&target(None), &turn, "Q");
    let joined = args.join(" ");
    assert!(joined.contains("--resume fork-id"), "{joined}");
    assert!(
        !joined.contains("--fork-session") && !joined.contains("main-id"),
        "{joined}"
    );
    assert!(!joined.contains("--model"), "no model known: CLI default");
}

#[test]
fn no_flag_that_changes_the_cached_prefix_or_skips_permissions() {
    for turn in [
        Turn::NewFork,
        Turn::FollowUp {
            fork_session_id: "f".into(),
            prior: Vec::new(),
        },
    ] {
        let joined = Claude.args(&target(Some("m")), &turn, "Q").join(" ");
        for flag in [
            "--tools",
            "--allowedTools",
            "--disallowedTools",
            "--append-system-prompt",
            "--system-prompt",
            "--json-schema",
            "--dangerously-skip-permissions",
            "--settings",
        ] {
            assert!(!joined.contains(flag), "{flag} in {joined}");
        }
    }
}

// --- transcript -----------------------------------------------------------

#[test]
fn transcript_gives_the_latest_real_model_and_cwd() {
    let dir = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
    let path = dir.path().join("s.jsonl");
    let lines = [
        r#"{"type":"assistant","cwd":"/a","message":{"model":"claude-sonnet-5-5"}}"#,
        r#"{"type":"assistant","cwd":"/b","message":{"model":"claude-opus-5-5"}}"#,
        r#"{"type":"assistant","cwd":"/b","message":{"model":"<synthetic>"}}"#,
        r#"{"type":"user","cwd":"/b","message":{}}"#,
    ];
    std::fs::write(&path, lines.join("\n")).unwrap();
    let (model, cwd) = transcript_model_and_cwd(&path).unwrap();
    assert_eq!(model.as_deref(), Some("claude-opus-5-5"));
    assert_eq!(cwd, Some(PathBuf::from("/b")));
}

// --- run ------------------------------------------------------------------

/// Runs a host-shell script in place of the agent CLI.
struct Shell(String);

impl Adapter for Shell {
    fn args(&self, _: &ForkTarget, _: &Turn, _: &str) -> Vec<String> {
        vec![tuic_test_support::host_shell().1.to_owned(), self.0.clone()]
    }
    fn parse(&self, stdout: &str) -> Result<ForkOutcome, String> {
        parse_stream(stdout)
    }
}

fn shell_target() -> ForkTarget {
    ForkTarget {
        program: PathBuf::from(tuic_test_support::host_shell().0),
        ..target(None)
    }
}

#[tokio::test]
async fn dropped_cancel_sender_does_not_cancel_the_run() {
    let dir = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
    let fixture = dir.path().join("out.jsonl");
    std::fs::write(&fixture, FOLLOW_UP).unwrap();
    let adapter = Shell(tuic_test_support::print_file_script(
        fixture.to_str().unwrap(),
    ));
    let (tx, rx) = oneshot::channel::<()>();
    drop(tx);
    let answer = run(
        &adapter,
        &shell_target(),
        &Turn::NewFork,
        "Q",
        rx,
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    assert_eq!(
        answer.outcome.fork_session_id,
        "dabf3e9d-30c8-4ae4-b8d9-0ee8003738cc"
    );
}

#[tokio::test]
async fn cancel_and_timeout_stop_a_running_fork() {
    let adapter = Shell(tuic_test_support::sleep_script());
    let (tx, rx) = oneshot::channel();
    let started = Instant::now();
    let target = shell_target();
    let pending = run(
        &adapter,
        &target,
        &Turn::NewFork,
        "Q",
        rx,
        Duration::from_secs(60),
    );
    tx.send(()).unwrap();
    assert_eq!(pending.await.unwrap_err(), "cancelled");

    let (_tx, rx) = oneshot::channel::<()>();
    let error = run(
        &adapter,
        &shell_target(),
        &Turn::NewFork,
        "Q",
        rx,
        Duration::from_millis(300),
    )
    .await
    .unwrap_err();
    assert!(error.contains("did not answer"), "{error}");
    // Outer hang bound only: the sleep lasts 60 s.
    assert!(started.elapsed() < Duration::from_secs(30));
}
