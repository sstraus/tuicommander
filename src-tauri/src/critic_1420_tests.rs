//! Critic tests for story 1420-f3de (foreground agent discovery).

use crate::pty::refresh_session_agent;
#[cfg(unix)]
use crate::pty::{AgentSubmissionWrite, agent_submission_rejection_detail};
#[cfg(unix)]
use crate::test_support::ForegroundIdentityProbe;
use std::sync::Arc;

fn state() -> Arc<crate::state::AppState> {
    Arc::new(crate::state::tests_support::make_test_app_state())
}

/// Catches: an unknown or already-closed session id panics or invents an
/// identity when the timer / IPC / HTTP all route through the shared classifier.
#[test]
fn refresh_of_an_unknown_session_is_none_and_leaves_no_state() {
    let state = state();
    assert_eq!(refresh_session_agent(&state, "no-such-session"), None);
    assert!(
        state
            .session_maps
            .session_states
            .get("no-such-session")
            .is_none()
    );
}

/// Catches: lock-order inversion or lost update when IPC, HTTP and the backend
/// timer classify the same session at once (sessions -> session_states ->
/// vt -> silence taken from three call sites).
#[cfg(unix)]
#[test]
fn concurrent_refreshes_agree_and_do_not_deadlock() {
    let state = state();
    let sid = "critic-1420-concurrent";
    let _probe = ForegroundIdentityProbe::new(state.clone(), sid, "claude");
    let results: Vec<Option<String>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    (0..50)
                        .map(|_| refresh_session_agent(&state, sid))
                        .collect::<Vec<_>>()
                        .pop()
                        .unwrap()
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    assert!(
        results.iter().all(|r| r.as_deref() == Some("claude")),
        "{results:?}"
    );
    assert_eq!(
        state
            .session_maps
            .session_states
            .get(sid)
            .unwrap()
            .agent_type
            .as_deref(),
        Some("claude")
    );
}

/// Catches: the rejection detail leaks a filesystem path (the foreground
/// executable's location) to a remote caller, or panics once the PTY is gone.
#[cfg(unix)]
#[test]
fn not_managed_agent_detail_names_the_process_but_never_a_path() {
    let state = state();
    let sid = "critic-1420-detail";
    let probe = ForegroundIdentityProbe::new(state.clone(), sid, "bash");
    let detail = agent_submission_rejection_detail(&state, sid, "not_managed_agent");
    assert!(detail.contains("foreground process: bash"), "{detail}");
    assert!(!detail.contains('/') && !detail.contains('\\'), "{detail}");
    assert!(matches!(
        crate::pty::write_agent_submission_to_pty(&state, sid, "x"),
        AgentSubmissionWrite::Rejected {
            reason: "not_managed_agent",
            ..
        }
    ));
    drop(probe);
    let gone = agent_submission_rejection_detail(&state, sid, "not_managed_agent");
    assert!(gone.contains("unknown"), "{gone}");
}

/// Catches: classification by ANY path component. A non-agent executable that
/// merely lives under a directory named after an agent (`.../pi/tool`) is
/// promoted to an agent by the macOS path rule, and the backend timer now
/// applies that to every session without a frontend in the loop.
#[cfg(unix)]
#[test]
fn executable_under_an_agent_named_directory_is_not_an_agent() {
    use std::process::{Command, Stdio};
    let scratch = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
    let dir = scratch.path().join("pi");
    std::fs::create_dir(&dir).unwrap();
    let exe = tuic_test_support::install_identity_probe(&dir, "tool");
    let mut child = Command::new(&exe).stdin(Stdio::piped()).spawn().unwrap();
    let mut name = None;
    for _ in 0..200 {
        name = crate::pty::process_name_from_pid(child.id());
        if name.as_deref() == Some("tool") || name.as_deref() == Some("pi") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    child.kill().unwrap();
    child.wait().unwrap();
    let name = name.expect("process name");
    assert_eq!(crate::pty::classify_agent(&name), None, "name={name}");
}

/// Catches: foreground identity read from anything but the executable's own
/// name. An installed probe named exactly like an agent is that agent; one
/// whose name merely contains an agent's (`claudex`, `my-codex`) or a shell
/// is not, and the name `process_name_from_pid` reports is the file name.
#[cfg(unix)]
#[test]
fn process_name_from_pid_classifies_an_agent_named_executable_and_not_a_lookalike() {
    use std::process::{Command, Stdio};
    let scratch = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
    let observe = |name: &str| -> String {
        let exe = tuic_test_support::install_identity_probe(scratch.path(), name);
        let mut child = Command::new(&exe).stdin(Stdio::piped()).spawn().unwrap();
        let observed = crate::pty::process_name_from_pid(child.id());
        child.kill().unwrap();
        child.wait().unwrap();
        observed.unwrap_or_else(|| panic!("no process name for {name}"))
    };
    for agent in ["claude", "codex"] {
        let name = observe(agent);
        assert_eq!(name, agent);
        assert_eq!(crate::pty::classify_agent(&name), Some(agent));
    }
    for (file, expected) in [
        ("claudex", "claudex"),
        ("my-codex", "my-codex"),
        ("bash", "bash"),
        ("mywrapper", "mywrapper"),
    ] {
        let name = observe(file);
        assert_eq!(name, expected);
        assert_eq!(crate::pty::classify_agent(&name), None, "name={name}");
    }
}
