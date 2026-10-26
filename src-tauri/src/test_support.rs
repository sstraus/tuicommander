//! App-specific fixtures built on the shared platform-neutral test helpers.

// Tests construct reqwest clients without running the desktop/headless startup,
// which installs this provider before any production client is built.
#[ctor::ctor]
fn install_test_tls_provider() {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("install the production TLS provider before libtest");
}

pub(crate) use crate::fs::system32_exe;
#[cfg(unix)]
pub(crate) use tuic_test_support::short_socket_test_temp_root;
pub(crate) use tuic_test_support::{
    dir_outside_home, fail_with_stderr_script, fake_ssh_script, host_shell, normalize_newlines,
    print_var_script, replay_file_command, slashed, sleep_script, test_temp_root,
};

/// A listening owner that resets every connection instead of serving a protocol.
pub(crate) struct ResettingTcpPeer {
    listener: std::sync::Arc<tokio::net::TcpListener>,
    task: tokio::task::JoinHandle<()>,
}

impl ResettingTcpPeer {
    pub(crate) fn local_addr(&self) -> std::io::Result<std::net::SocketAddr> {
        self.listener.local_addr()
    }
}

impl Drop for ResettingTcpPeer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Hold the listening port throughout each probe, including on macOS, where a
/// bound-only TCP socket times out rather than refusing. This fixture produces
/// ConnectionReset, NOT ConnectionRefused, after the TCP handshake succeeds.
/// Tokio's zero linger sets SO_LINGER to {on: 1, seconds: 0}; close sends RST on
/// macOS/Linux and Winsock (WSAECONNRESET). std/Mio do not set SO_REUSEADDR on
/// Windows, so ordinary parallel listeners cannot hijack this listening port.
pub(crate) fn resetting_tcp_peer() -> ResettingTcpPeer {
    let socket = tokio::net::TcpSocket::new_v4().expect("TCP socket");
    socket
        .bind(([127, 0, 0, 1], 0).into())
        .expect("reserve resetting TCP port");
    let listener = std::sync::Arc::new(socket.listen(128).expect("listen on resetting TCP port"));
    let acceptor = listener.clone();
    let task = tokio::spawn(async move {
        loop {
            let (stream, _) = acceptor
                .accept()
                .await
                .expect("accept resetting connection");
            stream.set_zero_linger().expect("abortive TCP close");
            drop(stream);
        }
    });
    ResettingTcpPeer { listener, task }
}

// Catches: a dead-peer fixture releasing its port or serving a live connection.
#[tokio::test]
async fn resetting_tcp_peer_reserves_its_port_and_resets_connections() {
    use tokio::io::AsyncReadExt;

    let peer = resetting_tcp_peer();
    let address = peer.local_addr().unwrap();
    // Separate probes protect consumers that reuse the fixture (seed then SSE).
    for _ in 0..2 {
        assert!(
            std::net::TcpListener::bind(address).is_err(),
            "a parallel listener must not take the resetting port"
        );
        let error = match tokio::net::TcpStream::connect(address).await {
            Ok(mut stream) => stream.read(&mut [0]).await.unwrap_err(),
            Err(error) => error,
        };
        assert_eq!(error.kind(), std::io::ErrorKind::ConnectionReset);
    }
}

/// Scratch dir short enough to bind a Unix socket whatever the checkout path.
#[cfg(unix)]
pub(crate) fn short_socket_tempdir() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("s")
        .tempdir_in(short_socket_test_temp_root())
        .expect("short socket test dir")
}

/// Snapshot the fake SSH PID and any direct children before tunnel teardown.
#[cfg(unix)]
pub(crate) fn fake_ssh_processes(marker: &std::path::Path) -> Vec<i32> {
    let parent: i32 = std::fs::read_to_string(marker)
        .expect("fake SSH wrote its PID")
        .trim()
        .parse()
        .unwrap();
    let output = std::process::Command::new("pgrep")
        .args(["-P", &parent.to_string()])
        .output()
        .expect("list fake SSH child PIDs");
    let mut pids = vec![parent];
    pids.extend(
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|line| line.parse::<i32>().unwrap()),
    );
    pids
}

/// Assert the OS no longer runs any process owned by this fake SSH instance.
#[cfg(unix)]
pub(crate) fn assert_fake_ssh_stopped(pids: Vec<i32>) {
    let surviving: Vec<_> = pids
        .into_iter()
        .filter(|pid| unsafe { libc::kill(*pid, 0) == 0 })
        .collect();
    // A failed test must still clean up the processes it deliberately started.
    for pid in &surviving {
        unsafe { libc::kill(*pid, libc::SIGKILL) };
    }
    assert!(
        surviving.is_empty(),
        "fake SSH processes survived shutdown: {surviving:?}"
    );
}

/// Feed everything a PTY master produces to `sink`, in the small chunks the
/// production reader thread sees, and return once the master goes quiet.
///
/// Quiet, not EOF: a unix master reports EOF once the slave is closed and the
/// child has exited, but a Windows ConPTY master stays readable for as long as
/// the console host lives, so `read` never answers `Ok(0)` there. The three
/// tests that drove a real PTY with an EOF loop did not fail on Windows — they
/// hung, and nextest killed them at 120s.
///
/// The drain must also answer the terminal's side of the conversation, which is
/// why it takes the master's writer. A ConPTY's console host opens by asking
/// the terminal where the cursor is — DSR, `ESC[6n` — and writes nothing at all
/// until it gets a reply. With no reply these tests saw exactly those four
/// bytes and then silence, so the child's output never existed to assert on.
/// Production answers the same query through alacritty's `Event::PtyWrite`
/// (see `pty.rs`); a buffer under test has no terminal, so the drain is the
/// terminal.
///
/// Three named budgets, per the rule that one deadline may not serve two roles.
/// `STARTUP` waits for the first byte the *child* wrote — the handshake above
/// does not count, or the drain would start its quiet countdown against a
/// console host that has not spoken yet. `QUIET` decides the child has finished
/// writing, and `DEADLINE` is the outer bound that says the read itself is
/// stuck.
pub(crate) fn drain_pty(
    mut reader: Box<dyn std::io::Read + Send>,
    mut terminal: Box<dyn std::io::Write + Send>,
    mut sink: impl FnMut(&[u8]),
) {
    const STARTUP: std::time::Duration = std::time::Duration::from_secs(15);
    const QUIET: std::time::Duration = std::time::Duration::from_millis(500);
    const DEADLINE: std::time::Duration = std::time::Duration::from_secs(60);

    let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
    // Detached: on Windows this thread stays parked in `read` after the child
    // exits, and the test process is what ends it.
    std::thread::spawn(move || {
        // 64 bytes, so the parser under test sees a chunk boundary in the
        // middle of the escape sequences rather than one tidy buffer.
        let mut raw = [0u8; 64];
        loop {
            match reader.read(&mut raw) {
                Ok(0) => break,
                Ok(n) => {
                    if tx.send(raw[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                Err(_) => break,
            }
        }
    });

    let deadline = std::time::Instant::now() + DEADLINE;
    let mut budget = STARTUP;
    while std::time::Instant::now() < deadline {
        // A timeout means the child stopped writing; a disconnect means the
        // reader reached EOF. Both end the drain.
        match rx.recv_timeout(budget) {
            Ok(chunk) => {
                let payload = answer_cursor_queries(&chunk, &mut terminal);
                // Nothing but the handshake: the child still has not spoken,
                // so the startup budget keeps running.
                if payload.is_empty() {
                    continue;
                }
                sink(&payload);
                budget = QUIET;
            }
            Err(_) => return,
        }
    }
}

/// Reply to every cursor-position query in `chunk` and return what is left.
///
/// The reply is always `1;1`: these tests open a fresh PTY and replay a file
/// into it, so the console host is asking about a cursor that has not moved.
/// The query is matched inside one chunk because the console host writes it as
/// a single write of its own, which the drain's read returns on its own.
fn answer_cursor_queries(chunk: &[u8], terminal: &mut (impl std::io::Write + ?Sized)) -> Vec<u8> {
    const QUERY: &[u8] = b"\x1b[6n";
    if !chunk.windows(QUERY.len()).any(|w| w == QUERY) {
        return chunk.to_vec();
    }
    let mut rest = Vec::with_capacity(chunk.len());
    let mut at = 0;
    while at < chunk.len() {
        if chunk[at..].starts_with(QUERY) {
            let _ = terminal.write_all(b"\x1b[1;1R");
            let _ = terminal.flush();
            at += QUERY.len();
        } else {
            rest.push(chunk[at]);
            at += 1;
        }
    }
    rest
}

// ---------------------------------------------------------------------------
// Sessions a test can deliver to
// ---------------------------------------------------------------------------
//
// These used to live in `pty::tests`, where only that module could reach them.
// Anything else that needed a session an agent could be delivered to — the
// hands-free dictation surface, for one — had no way to build one, and the
// alternative was making a test module public. They are the same helpers,
// moved; `pty::tests` imports them from here rather than keeping copies.

/// A session `session_is_agent` accepts, with its shell state and silence
/// tracker pre-seeded. `shell` is one of `pty`'s `SHELL_*` constants; an idle
/// one is confirmed idle, so the injection gate can claim it.
pub(crate) fn agent_session(state: &crate::state::AppState, sid: &str, shell: u8) {
    use std::sync::atomic::AtomicU8;
    state
        .session_maps
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(shell));
    state.session_maps.session_states.insert(
        sid.to_string(),
        crate::state::SessionState {
            agent_type: Some("claude".to_string()),
            spawn_root_role: crate::state::SpawnRootRole::DirectProgram,
            ..Default::default()
        },
    );
    let mut silence = crate::pty::SilenceState::new();
    if shell == crate::pty::SHELL_IDLE {
        silence.confirm_idle();
    }
    state.session_maps.silence_states.insert(
        sid.to_string(),
        std::sync::Arc::new(parking_lot::Mutex::new(silence)),
    );
}

/// Everything written to a PTY, kept so a test can assert both what reached the
/// composer and what deliberately did not.
#[cfg(unix)]
pub(crate) struct RecordingWriter {
    pub(crate) bytes: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
}

#[cfg(unix)]
impl std::io::Write for RecordingWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.bytes.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The three line-discipline states `write_terminal_reply` must tell apart.
/// `Cbreak` is not a curiosity: it is the one where `ICANON` and `ECHO`
/// disagree, so it is the only case that can prove the gate keys on the right
/// flag. A fresh `openpty` is `Cooked`.
#[cfg(unix)]
#[derive(Clone, Copy)]
pub(crate) enum TtyMode {
    /// `ICANON` + `ECHO` — a reply is painted on screen and never delivered.
    Cooked,
    /// `ICANON` off, `ECHO` on — ugly, but the reply IS read immediately.
    Cbreak,
    /// Both off — what an agent sets before it queries.
    Raw,
}

/// A live PTY whose every byte is recorded.
#[cfg(unix)]
pub(crate) fn insert_recording_session(
    state: &crate::state::AppState,
    session_id: &str,
) -> std::sync::Arc<std::sync::Mutex<Vec<u8>>> {
    let bytes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    insert_session_with_writer(
        state,
        session_id,
        Box::new(RecordingWriter {
            bytes: std::sync::Arc::clone(&bytes),
        }),
        TtyMode::Raw,
    );
    bytes
}

/// Register a real `openpty` session in `session_maps.sessions`, with its line
/// discipline set to `mode` and its writes going to `writer`.
#[cfg(unix)]
pub(crate) fn insert_session_with_writer(
    state: &crate::state::AppState,
    session_id: &str,
    writer: Box<dyn std::io::Write + Send>,
    mode: TtyMode,
) {
    use portable_pty::{CommandBuilder, PtySize, native_pty_system};
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("openpty");
    if !matches!(mode, TtyMode::Cooked) {
        let fd = pair.master.as_raw_fd().expect("master fd");
        let mut termios = std::mem::MaybeUninit::<libc::termios>::uninit();
        assert_eq!(unsafe { libc::tcgetattr(fd, termios.as_mut_ptr()) }, 0);
        let mut termios = unsafe { termios.assume_init() };
        termios.c_lflag &= !libc::ICANON;
        if matches!(mode, TtyMode::Raw) {
            termios.c_lflag &= !libc::ECHO;
        }
        assert_eq!(
            unsafe { libc::tcsetattr(fd, libc::TCSANOW, &termios) },
            0,
            "setting the line discipline must succeed"
        );
    }
    let mut command = CommandBuilder::new("/bin/sh");
    command.args(["-c", "sleep 30"]);
    let child = pair.slave.spawn_command(command).expect("spawn shell");
    state.session_maps.sessions.insert(
        session_id.to_string(),
        parking_lot::Mutex::new(crate::state::PtySession {
            launch_receipt: None,
            writer: Arc::new(parking_lot::Mutex::new(writer)),
            master: pair.master,
            _child: child,
            paused: Arc::new(AtomicBool::new(false)),
            worktree: None,
            initial_cwd: None,
            cwd: None,
            display_name: None,
            display_name_is_custom: false,
            display_name_from_spawn: false,
            is_remote: false,
            shell: "/bin/sh".to_string(),
        }),
    );
}

/// A real foreground OS executable with a chosen basename, plus recorded PTY
/// writes. This probes process identity, never a third-party CLI's behavior.
#[cfg(unix)]
pub(crate) struct ForegroundIdentityProbe {
    pub(crate) bytes: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
    state: std::sync::Arc<crate::state::AppState>,
    session_id: String,
    _scratch: tempfile::TempDir,
    foreground_child: Option<u32>,
}

#[cfg(unix)]
impl ForegroundIdentityProbe {
    pub(crate) fn new(
        state: std::sync::Arc<crate::state::AppState>,
        sid: &str,
        name: &str,
    ) -> Self {
        Self::spawn(
            state,
            sid,
            name,
            crate::state::SpawnRootRole::DirectProgram,
            false,
            false,
        )
    }

    pub(crate) fn shell_root(
        state: std::sync::Arc<crate::state::AppState>,
        sid: &str,
        name: &str,
    ) -> Self {
        Self::spawn(
            state,
            sid,
            name,
            crate::state::SpawnRootRole::Shell,
            false,
            false,
        )
    }

    pub(crate) fn shell_parent(
        state: std::sync::Arc<crate::state::AppState>,
        sid: &str,
        name: &str,
    ) -> Self {
        Self::spawn(
            state,
            sid,
            name,
            crate::state::SpawnRootRole::Shell,
            true,
            false,
        )
    }

    pub(crate) fn bash_wrapper(
        state: std::sync::Arc<crate::state::AppState>,
        sid: &str,
        role: crate::state::SpawnRootRole,
    ) -> Self {
        Self::spawn(state, sid, "bash", role, true, true)
    }

    /// Attach the production reader so normal EOF, rather than a manually
    /// injected lifecycle event, removes the probe's live process and identity.
    pub(crate) fn start_reader(&self) {
        let (reader, paused) = {
            let entry = self
                .state
                .session_maps
                .sessions
                .get(&self.session_id)
                .unwrap();
            let session = entry.lock();
            (
                session.master.try_clone_reader().unwrap(),
                session.paused.clone(),
            )
        };
        // Match spawn bookkeeping: the reader decrements this on native exit.
        self.state
            .metrics
            .active_sessions
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        crate::pty::spawn_reader_thread(
            reader,
            paused,
            self.session_id.clone(),
            self.state.clone(),
            None,
        );
    }

    pub(crate) fn return_to_root(&mut self) {
        let pid = self
            .foreground_child
            .take()
            .expect("probe owns a foreground child");
        // SAFETY: this PID belongs to the child started by this probe.
        assert_eq!(unsafe { libc::kill(pid as i32, libc::SIGKILL) }, 0);
        loop {
            let session = self
                .state
                .session_maps
                .sessions
                .get(&self.session_id)
                .unwrap();
            let session = session.lock();
            if session.master.process_group_leader()
                == session._child.process_id().map(|pid| pid as i32)
            {
                break;
            }
            drop(session);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    fn spawn(
        state: std::sync::Arc<crate::state::AppState>,
        sid: &str,
        name: &str,
        role: crate::state::SpawnRootRole,
        shell_parent: bool,
        bash_script: bool,
    ) -> Self {
        use parking_lot::Mutex;
        use portable_pty::{CommandBuilder, PtySize, native_pty_system};
        use std::sync::{Arc, atomic::AtomicBool};

        let scratch = tempfile::tempdir_in(test_temp_root()).unwrap();
        let executable = scratch.path().join(name);
        std::fs::copy("/bin/cat", &executable).unwrap();
        // macOS kills a relocated Apple platform binary (verified exit -9).
        // Ad-hoc signing the scratch copy makes it a local identity probe.
        #[cfg(target_os = "macos")]
        {
            let signed = std::process::Command::new("/usr/bin/codesign")
                .args(["--force", "--sign", "-"])
                .arg(&executable)
                .output()
                .expect("sign scratch identity probe");
            assert!(
                signed.status.success(),
                "codesign: {}",
                String::from_utf8_lossy(&signed.stderr)
            );
        }
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let command = if shell_parent {
            let mut command = CommandBuilder::new("/bin/bash");
            // Real job control puts the launched child in its own foreground group.
            // Keep another builtin after it so bash cannot exec away the shell root.
            command.args(["-c", "set -m; \"$@\"; read -r completion", "probe-root"]);
            if bash_script {
                let script = scratch.path().join("wrapper.sh");
                std::fs::write(&script, "read -r input\n").unwrap();
                command.arg("/bin/bash");
                command.arg(script);
            } else {
                command.arg(&executable);
            }
            command
        } else if role == crate::state::SpawnRootRole::Shell {
            // A real shell exec preserves its root PID while replacing its image.
            // This models exec into either an agent-name or a renamed shell root.
            let mut command = CommandBuilder::new("/bin/sh");
            if name == "bash" {
                // Exercise a real shell-to-shell image replacement as well.
                command.args(["-c", "exec /bin/bash -c 'read -r input'", "probe-root"]);
            } else {
                command.args(["-c", "exec \"$1\"", "probe-root"]);
                command.arg(&executable);
            }
            command
        } else {
            CommandBuilder::new(&executable)
        };
        let mut child = pair.slave.spawn_command(command).unwrap();
        let child_pid = child.process_id().unwrap();
        // fork precedes setsid/exec. This is setup, not a latency assertion;
        // nextest bounds a genuine hang without charging startup to behavior.
        let foreground_pid = loop {
            if let Some(fg) = pair.master.process_group_leader().map(|pid| pid as u32) {
                let group_ready = if shell_parent {
                    fg != child_pid
                } else {
                    fg == child_pid
                };
                if group_ready && crate::pty::process_name_from_pid(fg).as_deref() == Some(name) {
                    break fg;
                }
            }
            if let Some(status) = child.try_wait().unwrap() {
                panic!("identity probe exited before setup: {status:?}");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        let bytes = Arc::new(std::sync::Mutex::new(Vec::new()));
        state.session_maps.sessions.insert(
            sid.into(),
            Mutex::new(crate::state::PtySession {
                launch_receipt: None,
                writer: Arc::new(Mutex::new(Box::new(RecordingWriter {
                    bytes: bytes.clone(),
                }))),
                master: pair.master,
                _child: child,
                paused: Arc::new(AtomicBool::new(false)),
                worktree: None,
                initial_cwd: None,
                cwd: None,
                display_name: None,
                display_name_is_custom: false,
                display_name_from_spawn: false,
                is_remote: false,
                shell: "/bin/sh".into(),
            }),
        );
        agent_session(&state, sid, crate::pty::SHELL_IDLE);
        {
            let mut session = state.session_maps.session_states.get_mut(sid).unwrap();
            session.agent_type = None;
            session.spawn_root_role = role;
        }
        Self {
            bytes,
            state,
            session_id: sid.into(),
            _scratch: scratch,
            foreground_child: (foreground_pid != child_pid).then_some(foreground_pid),
        }
    }
}

#[cfg(unix)]
impl Drop for ForegroundIdentityProbe {
    fn drop(&mut self) {
        if let Some((_, session)) = self.state.session_maps.sessions.remove(&self.session_id) {
            let mut session = session.into_inner();
            // macOS waits for slave output to drain during process exit. These
            // identity-only probes have no production reader, so an unread
            // bash job-control message can keep child.wait() blocked forever.
            let mut reader = session
                .master
                .try_clone_reader()
                .expect("probe drain reader");
            let drain = std::thread::spawn(move || {
                let mut bytes = [0u8; 4096];
                loop {
                    match reader.read(&mut bytes) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {}
                    }
                }
            });
            if let Some(pid) = self.foreground_child {
                // This is the child created by our probe, never an ancestor.
                // SAFETY: kill takes a numeric PID and a valid signal.
                unsafe {
                    libc::kill(pid as i32, libc::SIGKILL);
                }
            }
            session._child.kill().expect("kill our identity probe");
            session._child.wait().expect("reap our identity probe");
            drop(session);
            drain.join().expect("join probe output drain");
        }
    }
}

/// Assert the public pre-dispatch repair contract and return its named constraint.
pub(crate) fn mcp_argument_constraint<'a>(
    result: &'a serde_json::Value,
    path: &str,
    keyword: &str,
) -> &'a serde_json::Value {
    assert_eq!(result["code"], "invalid_arguments", "{result}");
    assert_eq!(result["effect"], "not_started", "{result}");
    assert_eq!(result["outcome"], "rejected", "{result}");
    assert_eq!(result["retryable"], false, "{result}");
    assert_eq!(result["repair"]["retry_after_correction"], true, "{result}");
    &result["violations"]
        .as_array()
        .expect("repair violations")
        .iter()
        .find(|violation| {
            violation["path"] == path
                && violation["schema_path"]
                    .as_str()
                    .is_some_and(|schema_path| schema_path.rsplit('/').next() == Some(keyword))
        })
        .unwrap_or_else(|| panic!("missing {path} {keyword} violation: {result}"))["constraint"]
}

#[cfg(test)]
mod temp_root_tests {
    #[test]
    fn bare_test_binary_keeps_tempfile_dirs_inside_test_root() {
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap();
        let inside = repo.join(".tmp/tuic-tests/980-inside");
        let default_root = repo.join(".tmp/tuic-tests");
        let outside = repo.join(".tmp/980-outside");
        std::fs::create_dir_all(&inside).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        for (configured_root, expected_root) in [
            (Some(inside.as_path()), inside.as_path()),
            (None, default_root.as_path()),
        ] {
            let mut child = std::process::Command::new(std::env::current_exe().unwrap());
            child
                .args([
                    "--exact",
                    "test_support::temp_root_tests::bare_test_binary_child",
                ])
                .env("TUIC_TEST_TMP_EXPECTED_ROOT", expected_root)
                .env("TMPDIR", &outside)
                .env("TMP", &outside)
                .env("TEMP", &outside);
            if let Some(root) = configured_root {
                child.env("TUIC_TEST_TMP_ROOT", root);
            } else {
                child.env_remove("TUIC_TEST_TMP_ROOT");
            }
            let output = child.output().unwrap();
            assert!(
                output.status.success(),
                "child used ambient temp: {}",
                String::from_utf8_lossy(&output.stdout)
            );
        }
    }

    #[test]
    fn bare_test_binary_child() {
        let Some(expected) = std::env::var_os("TUIC_TEST_TMP_EXPECTED_ROOT") else {
            return;
        };
        let expected = std::path::PathBuf::from(expected).canonicalize().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let actual = dir.path().canonicalize().unwrap();
        assert!(
            actual.starts_with(&expected),
            "{} escaped the test root",
            actual.display()
        );
        let std_temp = std::env::temp_dir().canonicalize().unwrap();
        assert!(
            std_temp.starts_with(&expected),
            "{} escaped the test root",
            std_temp.display()
        );
    }
}
