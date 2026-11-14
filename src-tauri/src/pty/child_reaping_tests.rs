use super::*;
#[cfg(all(target_os = "linux", target_env = "gnu"))]
use crate::state::VtLogBuffer;
use portable_pty::{Child, ChildKiller, ExitStatus};
use std::io::{self, Read};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

/// Observes a real OS wait; neither exit readiness nor the result is simulated.
#[derive(Debug)]
struct ObservedChild {
    child: std::process::Child,
    waited: mpsc::Sender<u32>,
}

impl ChildKiller for ObservedChild {
    fn kill(&mut self) -> io::Result<()> {
        self.child.kill()
    }

    fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
        self.child.clone_killer()
    }
}

impl Child for ObservedChild {
    fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        Child::try_wait(&mut self.child)
    }

    fn wait(&mut self) -> io::Result<ExitStatus> {
        let status = Child::wait(&mut self.child)?;
        let _ = self.waited.send(status.exit_code());
        Ok(status)
    }

    fn process_id(&self) -> Option<u32> {
        Some(self.child.id())
    }
}

fn insert_gated_child(
    state: &AppState,
    sid: &str,
) -> (std::process::ChildStdin, mpsc::Receiver<u32>, libc::pid_t) {
    let (input, receiver, pid, _) = insert_gated_child_with_diagnostic(state, sid);
    (input, receiver, pid)
}

fn insert_gated_child_with_diagnostic(
    state: &AppState,
    sid: &str,
) -> (
    std::process::ChildStdin,
    mpsc::Receiver<u32>,
    libc::pid_t,
    Vec<u8>,
) {
    let mut child = Command::new("/bin/sh")
        .args([
            "-c",
            "printf 'startup diagnostic\\n'; exec 1>&-; read release; exit 7",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn gated shell");
    let input = child.stdin.take().expect("child stdin");
    let mut diagnostic = Vec::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_end(&mut diagnostic)
        .unwrap();
    assert!(child.try_wait().expect("poll child").is_none());
    let pid = child.id() as libc::pid_t;
    let (waited, receiver) = mpsc::channel();
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("openpty");
    let writer = pair.master.take_writer().expect("writer");
    state
        .metrics
        .active_sessions
        .fetch_add(1, Ordering::Relaxed);
    state.session_maps.sessions.insert(
        sid.to_string(),
        Mutex::new(PtySession {
            writer: Arc::new(Mutex::new(writer)),
            master: pair.master,
            _child: Box::new(ObservedChild { child, waited }),
            paused: Arc::new(AtomicBool::new(false)),
            worktree: None,
            initial_cwd: None,
            launch_receipt: None,
            cwd: None,
            display_name: None,
            display_name_is_custom: false,
            display_name_from_spawn: false,
            is_remote: true,
            shell: "/bin/sh".to_string(),
        }),
    );
    (input, receiver, pid, diagnostic)
}

/// Catches: EOF before wait readiness publishes completed/null, discards the
/// eventual nonzero exit status, and expires unread managed-child diagnostics.
#[test]
fn agent_startup_exit_reports_failure_and_keeps_output() {
    use crate::tasks::{TaskKind, TaskStatus};
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "startup-exit";
    let (mut input, waited, pid, diagnostic) = insert_gated_child_with_diagnostic(&state, sid);
    assert_eq!(diagnostic, b"startup diagnostic\n");
    let mut output = crate::OutputRingBuffer::new(4096);
    output.write(&diagnostic);
    state
        .session_maps
        .output_buffers
        .insert(sid.into(), Mutex::new(output));
    let task = state
        .tasks
        .create(TaskKind::AgentSpawn, "parent", Some(sid));
    mark_session_exited(sid, &state);
    mark_session_exited(sid, &state); // duplicate EOF must not steal the wait
    let before_release = state.tasks.get(&task).unwrap().status;
    // Always release our own child, even if the pre-fix assertion will fail.
    input.write_all(b"release\n").unwrap();
    drop(input);
    waited
        .recv_timeout(Duration::from_secs(30))
        .expect("child wait");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let record = loop {
        let record = state.tasks.get(&task).unwrap();
        if record.status.is_terminal() {
            break record;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "wait status never reached task"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_os_child_reaped(pid);
    assert_eq!(
        before_release,
        TaskStatus::Working,
        "EOF is not process completion"
    );
    assert_eq!(record.status, TaskStatus::Failed);
    assert_eq!(record.result.as_ref().unwrap()["exit_code"], 7);
    assert_eq!(*state.session_maps.exit_codes.get(sid).unwrap(), 7);
    let stamp = state
        .session_maps
        .last_output_ms
        .get(sid)
        .unwrap()
        .load(Ordering::Relaxed);
    let expired = aged_out_tombstones(&state, stamp + TOMBSTONE_TTL_MS + 1);
    reap_tombstones(&state, &expired);
    assert!(
        state.session_maps.output_buffers.contains_key(sid),
        "task diagnostics expired before collection"
    );
}

fn assert_os_child_reaped(pid: libc::pid_t) {
    let mut status = 0;
    // SAFETY: only observes this test's own child and writes to a live integer.
    let result = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
    assert_eq!(result, -1, "the session must reap its own child");
    assert_eq!(
        io::Error::last_os_error().raw_os_error(),
        Some(libc::ECHILD)
    );
}

fn assert_teardown_returns_after_os_reaped_child(kill: bool) {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "externally-reaped-child";
    let (mut input, _waited, pid) = insert_gated_child(&state, sid);
    input.write_all(b"release\n").expect("release child");
    drop(input);
    let mut status = 0;
    // SAFETY: reap only this test's own shell, leaving its Child handle without
    // a cached status. This records a real persistent OS polling error.
    assert_eq!(unsafe { libc::waitpid(pid, &mut status, 0) }, pid);
    let error = state
        .session_maps
        .sessions
        .get(sid)
        .unwrap()
        .lock()
        ._child
        .try_wait()
        .expect_err("externally reaped child must report an OS error");
    assert_eq!(error.raw_os_error(), Some(libc::ECHILD));

    let (done, returned) = mpsc::channel();
    let closing_state = Arc::clone(&state);
    let worker = std::thread::spawn(move || {
        if kill {
            assert!(kill_pty_core(&closing_state, sid));
        } else {
            close_pty_core(&closing_state, sid, false);
        }
        let _ = done.send(());
    });
    // Arm the harness after setup: teardown's 100ms polling windows must not
    // become an infinite retry just because the OS cannot supply exit status.
    returned
        .recv_timeout(Duration::from_secs(2))
        .expect("PTY teardown retried ECHILD forever instead of returning");
    worker.join().expect("teardown worker");
    assert!(!state.session_maps.sessions.contains_key(sid));
    assert_eq!(state.metrics.active_sessions.load(Ordering::Relaxed), 0);
    assert_os_child_reaped(pid);
}

/// Catches: a persistent child-status error bypasses the close grace deadline,
/// preventing the desktop quit loop from ever reaching app.exit.
#[test]
fn close_teardown_returns_after_os_reaped_child() {
    assert_teardown_returns_after_os_reaped_child(false);
}

/// Catches: the immediate-kill path retries ECHILD past its reap deadline.
#[test]
fn kill_teardown_returns_after_os_reaped_child() {
    assert_teardown_returns_after_os_reaped_child(true);
}

/// Catches: a TUIC kill recorded as exit code 1 (portable-pty maps a signal
/// death to 1), indistinguishable from an agent failure.
#[test]
fn tuic_kill_and_close_record_the_sigkill_exit_code() {
    for kill in [true, false] {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let sid = if kill { "killed-code" } else { "closed-code" };
        let (_input, _waited, pid) = insert_gated_child(&state, sid);
        if kill {
            assert!(kill_pty_core(&state, sid));
        } else {
            close_pty_core_with_reason(&state, sid, false, "idle_close");
        }
        assert_eq!(
            *state.session_maps.exit_codes.get(sid).unwrap(),
            128 + libc::SIGKILL,
            "kill={kill}"
        );
        let expected_reason = if kill { "kill_requested" } else { "idle_close" };
        assert_eq!(
            state
                .session_maps
                .exit_reasons
                .get(sid)
                .as_deref()
                .map(String::as_str),
            Some(expected_reason)
        );
        assert_os_child_reaped(pid);
    }
}

/// Catches: the reader thread publishes the `exited` state change before
/// kill records the reason, so the parent reads a null `exit_reason`.
#[cfg(unix)]
#[test]
fn tuic_kill_exited_event_carries_the_reason() {
    // The reader spawns its silence timer on the ambient Tokio runtime.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("test Tokio runtime");
    let _runtime_guard = runtime.enter();
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let (sid, parent) = ("killed-event", "killed-event-parent");
    let probe =
        crate::test_support::ForegroundIdentityProbe::shell_root(state.clone(), sid, "claude");
    state
        .session_maps
        .session_parent
        .insert(sid.to_string(), parent.to_string());
    state.agent_inbox.entry(parent.to_string()).or_default();
    probe.start_reader();
    assert!(kill_pty_core(&state, sid));
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let exited = loop {
        let found = state.agent_inbox.get(parent).and_then(|inbox| {
            inbox
                .iter()
                .filter_map(|m| serde_json::from_str::<serde_json::Value>(&m.content).ok())
                .find(|v| v["state"] == "exited")
        });
        if let Some(found) = found {
            break found;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "parent never received the exited state change"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(exited["exit_reason"], "kill_requested");
}

/// Catches: kill records its exit code only after reaping, so the reader's
/// `exited` state change can reach the parent with a null `exit_code`.
#[cfg(unix)]
#[test]
fn tuic_kill_exited_event_carries_the_sigkill_code() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("test Tokio runtime");
    let _runtime_guard = runtime.enter();
    for round in 0..5 {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let sid = format!("killed-code-event-{round}");
        let parent = format!("killed-code-event-parent-{round}");
        let probe =
            crate::test_support::ForegroundIdentityProbe::shell_root(state.clone(), &sid, "claude");
        state
            .session_maps
            .session_parent
            .insert(sid.clone(), parent.clone());
        state.agent_inbox.entry(parent.clone()).or_default();
        probe.start_reader();
        assert!(kill_pty_core(&state, &sid));
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let exited = loop {
            let found = state.agent_inbox.get(&parent).and_then(|inbox| {
                inbox
                    .iter()
                    .filter_map(|m| serde_json::from_str::<serde_json::Value>(&m.content).ok())
                    .find(|v| v["state"] == "exited")
            });
            if let Some(found) = found {
                break found;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "round {round}: parent never received the exited state change"
            );
            std::thread::sleep(Duration::from_millis(20));
        };
        assert_eq!(exited["exit_code"], 128 + libc::SIGKILL, "round {round}");
    }
}

/// Catches: EOF/close drops the child after try_wait(None), leaving a zombie
/// when the real process exits later. Also prevents waiting under session locks.
#[test]
fn session_removal_reaps_child_that_exits_after_first_poll() {
    for explicit_close in [false, true] {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let sid = if explicit_close {
            "reap-close"
        } else {
            "reap-eof"
        };
        let (mut input, waited, pid) = insert_gated_child(&state, sid);
        if explicit_close {
            cleanup_session(sid, &state);
        } else {
            mark_session_exited(sid, &state);
        }
        assert!(!state.session_maps.sessions.contains_key(sid));
        assert_eq!(state.metrics.active_sessions.load(Ordering::Relaxed), 0);
        // The shell cannot exit until teardown has returned and this releases it.
        input.write_all(b"release\n").expect("release child");
        drop(input);
        assert_eq!(
            waited
                .recv_timeout(Duration::from_secs(30))
                .expect("removed child must still have an OS wait owner"),
            7
        );
        assert_os_child_reaped(pid);
    }
}

/// Catches: reaping already-completed children loses the captured exit status
/// or decrements active-session accounting twice on a repeated EOF/close.
#[test]
fn session_removal_preserves_already_reaped_exit_status() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "reap-already-exited";
    let (mut input, _waited, pid) = insert_gated_child(&state, sid);
    input.write_all(b"release\n").expect("release child");
    drop(input);
    state
        .session_maps
        .sessions
        .get(sid)
        .unwrap()
        .lock()
        ._child
        .wait()
        .expect("wait child");
    mark_session_exited(sid, &state);
    assert_eq!(*state.session_maps.exit_codes.get(sid).unwrap(), 7);
    mark_session_exited(sid, &state);
    assert_eq!(state.metrics.active_sessions.load(Ordering::Relaxed), 0);
    assert_os_child_reaped(pid);
}

/// Catches: removing scrollback owners leaves hundreds of MiB of freed glibc
/// arena pages resident, on explicit close or expiry of post-mortem buffers.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[test]
fn linux_glibc_session_payload_release_returns_scrollback_pages() {
    fn resident_bytes() -> usize {
        let status = std::fs::read_to_string("/proc/self/status").expect("process memory status");
        status
            .lines()
            .find_map(|line| line.strip_prefix("VmRSS:"))
            .expect("resident memory")
            .split_whitespace()
            .next()
            .unwrap()
            .parse::<usize>()
            .unwrap()
            * 1024
    }

    for tombstone in [false, true] {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mut live_grid = VtLogBuffer::new(24, 80, 1);
        live_grid.process(b"still-live");
        state
            .grid
            .vt_log_buffers
            .insert("live-session".into(), Mutex::new(live_grid));
        let ids: Vec<_> = (0..6).map(|n| format!("rss-owner-{n}")).collect();
        // Production grids are allocated by separate PTY reader threads. Use
        // real threads and real grids so this exercises glibc's thread arenas.
        let workers: Vec<_> = ids
            .iter()
            .map(|id| {
                let state = Arc::clone(&state);
                let id = id.clone();
                std::thread::spawn(move || {
                    let mut grid = VtLogBuffer::new(24, 220, 10000);
                    grid.process(&b"row\r\n".repeat(10024));
                    state.grid.vt_log_buffers.insert(id, Mutex::new(grid));
                })
            })
            .collect();
        for worker in workers {
            worker.join().expect("scrollback allocator thread");
        }
        let before = resident_bytes();
        if tombstone {
            reap_tombstones(&state, &ids);
        } else {
            for id in &ids {
                cleanup_session(id, &state);
            }
        }
        assert_eq!(state.grid.vt_log_buffers.len(), 1);
        assert_eq!(
            state
                .grid
                .vt_log_buffers
                .get("live-session")
                .unwrap()
                .lock()
                .grid_get_row_text(0),
            "still-live"
        );
        let after = resident_bytes();
        // Six 10000-row, 220-column grids hold over 300 MiB. A deliberately
        // loose OS-level bound tolerates metadata/caches without accepting the
        // measured failure where all freed grid pages stay resident.
        assert!(
            before.saturating_sub(after) > 64 * 1024 * 1024,
            "freed scrollback must return resident pages: tombstone={tombstone}, before={before}, after={after}"
        );
    }
}
