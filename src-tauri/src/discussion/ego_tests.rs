use std::path::PathBuf;
use std::time::Duration;

use tokio::sync::oneshot;

use super::ego::{Ego, manifest_model_and_cwd, parse_events, parse_fork};
use super::{Adapter, ForkOutcome, ForkTarget, Turn, Usage, run};

// Captured 2026-11-21 from an ego build of 8782c15de, model
// openai-codex/gpt-6.2-sol, main session of 25.8k context tokens.
/// `ego fork ID --output json` of a session whose turn was still running.
const FORK: &str = include_str!("fixtures/ego_fork.json");
/// First question in that fork: `ego resume FORK Q --mode plan --no-hooks --output jsonl`.
const FIRST_QUESTION: &str = include_str!("fixtures/ego_fork_mid_turn.jsonl");
/// Fourth turn of another fork, in plan mode with hooks on: plan mode refuses
/// each hook and reports it as an `error` event.
const FOLLOW_UP: &str = include_str!("fixtures/ego_follow_up.jsonl");
/// A fork told to run `touch x.txt` in plan mode.
const PLAN_DENIED: &str = include_str!("fixtures/ego_plan_denied.jsonl");

const FORK_ID: &str = "01a1fe64-3e6b-703f-b609-9610382e097e";

fn target(model: Option<&str>) -> ForkTarget {
    ForkTarget {
        program: PathBuf::from("ego"),
        cwd: PathBuf::from("."),
        env: Vec::new(),
        main_session_id: "main-id".to_owned(),
        model: model.map(str::to_owned),
    }
}

fn follow_up(fork: &str) -> Turn {
    Turn::FollowUp {
        fork_session_id: fork.to_owned(),
        prior: Vec::new(),
    }
}

// --- parser ---------------------------------------------------------------

#[test]
fn first_question_reads_the_main_session_from_the_cache() {
    let out = parse_events(FIRST_QUESTION).unwrap();
    assert_eq!(out.fork_session_id, FORK_ID);
    assert!(
        out.text.starts_with("[answer] Waiting for the command\n"),
        "{}",
        out.text
    );
    let usage = Usage {
        input_tokens: 731,
        output_tokens: 47,
        cache_read_tokens: 25_472,
        ..Usage::default()
    };
    assert_eq!(out.usage, usage);
}

#[test]
fn follow_up_usage_is_the_run_and_a_refused_hook_is_not_a_failure() {
    let out = parse_events(FOLLOW_UP).unwrap();
    // `--output json` sums every turn of the fork: it reported 77,440 cached
    // for the three turns before this one.
    assert_eq!(out.usage.cache_read_tokens, 26_240);
    assert_eq!(out.usage.input_tokens, 216);
    assert_eq!(out.usage.output_tokens, 98);
    assert!(
        FOLLOW_UP.contains("\"type\":\"error\""),
        "the fixture holds the hook errors"
    );
    assert!(
        out.text
            .starts_with("This session is in read-only plan mode"),
        "{}",
        out.text
    );
}

#[test]
fn a_turn_stopped_by_plan_mode_or_cut_short_is_an_error() {
    assert!(
        parse_events(PLAN_DENIED)
            .unwrap_err()
            .contains("PermissionRequired")
    );
    // A missing session prints nothing on stdout (the reason is on stderr).
    assert!(parse_events("").unwrap_err().contains("turn_completed"));
    let cut = FIRST_QUESTION
        .lines()
        .filter(|l| !l.contains("turn_completed"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(parse_events(&cut).unwrap_err().contains("turn_completed"));
}

#[test]
fn fork_output_gives_the_new_session_id() {
    assert_eq!(parse_fork(FORK).unwrap(), FORK_ID);
    assert!(parse_fork("").is_err());
    let failed = r#"{"v":21,"session":"x","ok":false,"exit_reason":"error"}"#;
    assert!(parse_fork(failed).unwrap_err().contains("error"));
}

// --- command --------------------------------------------------------------

#[test]
fn fork_command_forks_the_main_session_without_a_prompt() {
    assert_eq!(
        Ego.fork_args(&target(Some("p/m"))).unwrap(),
        ["fork", "main-id", "--output", "json"]
    );
}

#[test]
fn question_resumes_the_fork_in_plan_mode_with_the_main_model() {
    let args = Ego.args(
        &target(Some("openai-codex/gpt-6.2-sol")),
        &follow_up("fork-id"),
        "<q>",
    );
    assert_eq!(&args[..3], ["resume", "fork-id", "<q>"]);
    let joined = args.join(" ");
    assert!(joined.contains("-m openai-codex/gpt-6.2-sol"), "{joined}");
    assert!(
        joined.contains("--mode plan") && joined.contains("--no-hooks"),
        "{joined}"
    );
    assert!(joined.contains("--output jsonl"), "{joined}");
    assert!(
        !joined.contains("main-id"),
        "a question never runs in the main session: {joined}"
    );
    let args = Ego.args(&target(None), &follow_up("fork-id"), "<q>");
    assert!(
        !args.contains(&"-m".to_owned()),
        "no model known: CLI default"
    );
}

#[test]
fn no_command_resumes_the_main_session_or_changes_the_cached_prefix() {
    for turn in [Turn::NewFork, follow_up("fork-id")] {
        let args = Ego.args(&target(Some("p/m")), &turn, "<q>");
        assert!(!(args[0] == "resume" && args[1] == "main-id"), "{args:?}");
        for flag in [
            "--system",
            "--append-system",
            "--tools",
            "--no-tools",
            "--bare",
            "--yolo",
            "--instructions",
        ] {
            assert!(!args.iter().any(|a| a == flag), "{flag} in {args:?}");
        }
    }
}

// --- manifest -------------------------------------------------------------

#[test]
fn manifest_gives_the_model_and_the_workspace() {
    let dir = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
    let path = dir.path().join("manifest.json");
    std::fs::write(
        &path,
        r#"{"id":"s","workspace_root":"/w/repo","model":"openai-codex/gpt-6.2-sol"}"#,
    )
    .unwrap();
    let (model, cwd) = manifest_model_and_cwd(&path).unwrap();
    assert_eq!(model.as_deref(), Some("openai-codex/gpt-6.2-sol"));
    assert_eq!(cwd, PathBuf::from("/w/repo"));
    std::fs::write(&path, r#"{"id":"s","model":"p/m"}"#).unwrap();
    assert!(
        manifest_model_and_cwd(&path)
            .unwrap_err()
            .contains("workspace")
    );
}

// --- run ------------------------------------------------------------------

/// Stands in for a CLI that forks in one command and asks in another: each
/// command is a host-shell `echo`.
struct TwoStep {
    fork_prints: &'static str,
}

impl Adapter for TwoStep {
    fn args(&self, _: &ForkTarget, turn: &Turn, _: &str) -> Vec<String> {
        let session = match turn {
            Turn::NewFork => "asked-in-main",
            Turn::FollowUp {
                fork_session_id, ..
            } => fork_session_id,
        };
        vec![
            tuic_test_support::host_shell().1.to_owned(),
            format!("echo {session}"),
        ]
    }
    fn parse(&self, stdout: &str) -> Result<ForkOutcome, String> {
        Ok(ForkOutcome {
            fork_session_id: stdout.trim().to_owned(),
            text: String::new(),
            usage: Usage::default(),
        })
    }
    fn fork_args(&self, _: &ForkTarget) -> Option<Vec<String>> {
        Some(vec![
            tuic_test_support::host_shell().1.to_owned(),
            format!("echo {}", self.fork_prints),
        ])
    }
    fn parse_fork(&self, stdout: &str) -> Result<String, String> {
        Some(stdout.trim())
            .filter(|id| id.starts_with("fork-"))
            .map(str::to_owned)
            .ok_or_else(|| "no fork".to_owned())
    }
}

async fn run_two_step(fork_prints: &'static str, turn: Turn) -> Result<String, String> {
    let target = ForkTarget {
        program: PathBuf::from(tuic_test_support::host_shell().0),
        ..target(None)
    };
    let (_keep, cancel) = oneshot::channel();
    run(
        &TwoStep { fork_prints },
        &target,
        &turn,
        "Q",
        cancel,
        Duration::from_secs(60),
    )
    .await
    .map(|answer| answer.outcome.fork_session_id)
}

#[tokio::test]
async fn a_new_thread_forks_first_and_asks_in_the_fork() {
    assert_eq!(
        run_two_step("fork-1", Turn::NewFork).await.unwrap(),
        "fork-1"
    );
}

#[tokio::test]
async fn a_follow_up_does_not_fork_again() {
    assert_eq!(
        run_two_step("fork-2", follow_up("fork-1")).await.unwrap(),
        "fork-1"
    );
}

#[tokio::test]
async fn a_failed_fork_asks_nothing() {
    let error = run_two_step("refused", Turn::NewFork).await.unwrap_err();
    assert!(error.starts_with("no fork"), "{error}");
}
