use super::*;
use crate::test_support::{
    fail_with_stderr_script, normalize_newlines, sleep_script, test_temp_root,
};
use tuic_test_support::{print_file_script, touch_script};

fn check(command: String) -> Precheck {
    Precheck {
        command,
        timeout_secs: 30,
        inject_output: false,
    }
}
fn executed(outcome: PrecheckOutcome) -> PrecheckResult {
    let PrecheckOutcome::Executed(result) = outcome else {
        panic!("expected executed check: {outcome:?}")
    };
    result
}

// Catches: a successful real shell check being refused instead of admitting dispatch.
#[tokio::test]
async fn zero_exit_admits_dispatch() {
    let outcome = run_precheck(&test_temp_root(), Some(&check("echo ready".into())), false).await;
    assert!(
        outcome.proceeds(),
        "successful check must proceed: {outcome:?}"
    );
    let result = executed(outcome);
    assert_eq!(normalize_newlines(&result.stdout), "ready\n");
    assert_eq!(result.stderr, "");
    assert!(!result.stdout_truncated);
}

// Catches: a precheck reading from the repository instead of its resolved worktree.
#[tokio::test]
async fn relative_file_reads_use_resolved_workspace() {
    let workspace = tempfile::tempdir_in(test_temp_root()).unwrap();
    std::fs::write(workspace.path().join("input.txt"), "workspace-specific").unwrap();
    let result = executed(
        run_precheck(
            workspace.path(),
            Some(&check(print_file_script("input.txt"))),
            false,
        )
        .await,
    );
    assert_eq!(result.termination, Termination::Exited(Some(0)));
    assert_eq!(result.stdout, "workspace-specific");
}

// Catches: nonzero exit being mistaken for success or losing stderr diagnostics.
#[tokio::test]
async fn nonzero_exit_preserves_both_streams_and_refuses_dispatch() {
    let separator = if cfg!(windows) { "&" } else { ";" };
    let script = format!(
        "echo before {separator} {}",
        fail_with_stderr_script("refused", 7)
    );
    let outcome = run_precheck(&test_temp_root(), Some(&check(script)), false).await;
    assert!(!outcome.proceeds());
    let result = executed(outcome);
    assert_eq!(result.termination, Termination::Exited(Some(7)));
    assert_eq!(normalize_newlines(&result.stdout).trim(), "before");
    assert_eq!(normalize_newlines(&result.stderr).trim(), "refused");
}

// Catches: spawn failure being flattened to nonzero exit (or allowing launch).
#[tokio::test]
async fn missing_workspace_preserves_spawn_error() {
    let workspace = tempfile::tempdir_in(test_temp_root()).unwrap();
    let outcome = run_precheck(
        &workspace.path().join("absent"),
        Some(&check("echo ready".into())),
        false,
    )
    .await;
    assert!(!outcome.proceeds());
    let result = executed(outcome);
    assert!(matches!(result.termination, Termination::SpawnError(_)));
    assert_eq!(result.stdout, "");
}

// Catches: manually triggered runs executing prechecks or recording them as absent.
#[tokio::test]
async fn manual_bypass_is_explicit_and_never_launches_shell() {
    let workspace = tempfile::tempdir_in(test_temp_root()).unwrap();
    let precheck = check(touch_script("must-not-exist"));
    assert_eq!(
        run_precheck(workspace.path(), Some(&precheck), true).await,
        PrecheckOutcome::Bypassed
    );
    assert!(!workspace.path().join("must-not-exist").exists());
    assert_eq!(
        run_precheck(workspace.path(), None, true).await,
        PrecheckOutcome::Bypassed
    );
    assert_eq!(
        run_precheck(workspace.path(), None, false).await,
        PrecheckOutcome::NotConfigured
    );
}

// Catches: deadlocking full pipes, merging streams, or unbounded final capture.
#[tokio::test]
async fn large_streams_are_drained_and_individually_truncated() {
    let workspace = tempfile::tempdir_in(test_temp_root()).unwrap();
    let bytes = vec![b'x'; MAX_STREAM_BYTES + 32768];
    std::fs::write(workspace.path().join("large.txt"), bytes).unwrap();
    let read = print_file_script("large.txt");
    let separator = if cfg!(windows) { "&" } else { ";" };
    let script = format!("{read} {separator} {read} 1>&2");
    let outcome = run_precheck(workspace.path(), Some(&check(script)), false).await;
    assert!(outcome.proceeds(), "{outcome:?}");
    let result = executed(outcome);
    assert_eq!(result.stdout.len(), MAX_STREAM_BYTES);
    assert_eq!(result.stderr.len(), MAX_STREAM_BYTES);
    assert!(result.stdout_truncated && result.stderr_truncated);
}

// Catches: invalid UTF-8 expanding the stored string past the byte cap.
#[test]
fn lossy_unicode_stays_within_stored_byte_limit() {
    let mut capture = StreamCapture::default();
    capture.append(&vec![0xff; MAX_STREAM_BYTES]);
    let (text, truncated) = capture.text();
    assert!(text.len() <= MAX_STREAM_BYTES && truncated);
    assert_ne!(text, "");
    let mut exact = StreamCapture::default();
    exact.append(&vec![b'x'; MAX_STREAM_BYTES]);
    assert!(!exact.text().1);
}

// Catches: timeout being returned as success or losing output already captured.
#[tokio::test]
async fn timeout_preserves_partial_output_and_refuses_dispatch() {
    let separator = if cfg!(windows) { "&" } else { ";" };
    let precheck = Precheck {
        command: format!("echo before {separator} {}", sleep_script()),
        timeout_secs: 1,
        inject_output: false,
    };
    let outcome = run_precheck(&test_temp_root(), Some(&precheck), false).await;
    assert!(!outcome.proceeds());
    let result = executed(outcome);
    assert_eq!(result.termination, Termination::TimedOut);
    assert_eq!(normalize_newlines(&result.stdout).trim(), "before");
    assert!(result.duration_ms >= 1000);
}

// Catches: killing only the shell and leaving its owned subprocess alive.
#[cfg(unix)]
#[tokio::test]
async fn timeout_terminates_owned_grandchild() {
    let precheck = Precheck {
        command: format!("{} & echo $!; wait", sleep_script()),
        timeout_secs: 1,
        inject_output: false,
    };
    let result = executed(run_precheck(&test_temp_root(), Some(&precheck), false).await);
    assert_eq!(result.termination, Termination::TimedOut);
    let pid: i32 = result
        .stdout
        .trim()
        .parse()
        .expect("shell must publish its child pid");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        // SAFETY: signal 0 probes only; this test never signals another process.
        if unsafe { libc::kill(pid, 0) } == -1 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "owned child {pid} survived timeout teardown"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

// Catches: invalid direct-call bounds launching work instead of failing closed.
#[tokio::test]
async fn invalid_execution_bounds_refuse_dispatch() {
    for precheck in [
        Precheck {
            command: "echo ready".into(),
            timeout_secs: 0,
            inject_output: false,
        },
        check(" ".into()),
    ] {
        let outcome = run_precheck(&test_temp_root(), Some(&precheck), false).await;
        assert!(!outcome.proceeds());
        assert!(matches!(
            executed(outcome).termination,
            Termination::ProcessError(_)
        ));
    }
}

// Catches: persistence losing the manual bypass versus no configured precheck.
#[test]
fn outcomes_round_trip_for_run_ledger() {
    for outcome in [
        PrecheckOutcome::Bypassed,
        PrecheckOutcome::NotConfigured,
        PrecheckOutcome::Executed(empty_result(Termination::TimedOut, Instant::now())),
    ] {
        let json = serde_json::to_string(&outcome).unwrap();
        assert_eq!(
            serde_json::from_str::<PrecheckOutcome>(&json).unwrap(),
            outcome
        );
    }
}
