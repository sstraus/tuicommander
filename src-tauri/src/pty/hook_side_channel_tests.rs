// Agent hook side channel (#2749-bee7): end to end through the production PTY
// reader, the local IPC router and the generated hook commands.

/// A PTY whose output is whatever the test sends, in the chunks it sends.
#[cfg(unix)]
struct ChannelPty {
    chunks: std::sync::mpsc::Receiver<Vec<u8>>,
    pending: Vec<u8>,
}

#[cfg(unix)]
impl std::io::Read for ChannelPty {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.pending.is_empty() {
            match self.chunks.recv() {
                Ok(chunk) => self.pending = chunk,
                Err(_) => return Ok(0),
            }
        }
        let n = buf.len().min(self.pending.len());
        buf[..n].copy_from_slice(&self.pending[..n]);
        self.pending.drain(..n);
        Ok(n)
    }
}

/// A hook-instrumented agent session: the production reader fed by a TUI
/// that redraws its spinner every millisecond, and the local IPC router on a
/// Unix socket, as `TUIC_SOCKET` names it in the agent environment.
#[cfg(unix)]
struct HookChannelSession {
    state: Arc<AppState>,
    owner: &'static str,
    socket: std::path::PathBuf,
    tty: std::path::PathBuf,
    redrawing: Arc<AtomicBool>,
    /// Keeps the reader's input open after the spinner stops, so the session
    /// lives until the test drops it.
    output: std::sync::mpsc::Sender<Vec<u8>>,
    _dir: tempfile::TempDir,
}

#[cfg(unix)]
const SPINNER: &str = "\r\x1b[2K✻ Working";

#[cfg(unix)]
impl HookChannelSession {
    fn start(sid: &str, agent: &str, owner: &'static str) -> Self {
        let state = accumulating_state(sid);
        state.config.write().progress_tracking = false;
        agent_session(&state, sid, SHELL_IDLE);
        {
            let mut session = state.session_maps.session_states.get_mut(sid).unwrap();
            session.agent_type = Some(agent.into());
            session.hook_instrumented = true;
        }
        state
            .grid
            .vt_log_buffers
            .insert(sid.into(), Mutex::new(VtLogBuffer::new(30, 100, 1000)));
        state.bind_live_pty(owner, sid);
        let (output, chunks) = std::sync::mpsc::channel();
        spawn_reader_thread(
            Box::new(ChannelPty {
                chunks,
                pending: Vec::new(),
            }),
            Arc::new(AtomicBool::new(false)),
            sid.into(),
            state.clone(),
            Some(owner.into()),
        );
        let redrawing = Arc::new(AtomicBool::new(true));
        let running = redrawing.clone();
        let held_output = output.clone();
        std::thread::spawn(move || {
            while running.load(Ordering::Relaxed) {
                if output.send(SPINNER.as_bytes().to_vec()).is_err() {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        });
        let dir = crate::test_support::short_socket_tempdir();
        let socket = dir.path().join("h.sock");
        let listener = tokio::net::UnixListener::bind(&socket).unwrap();
        let router = crate::mcp_http::build_router(state.clone(), false, false);
        tokio::spawn(async move { axum::serve(listener, router).await });
        let tty = dir.path().join("tty");
        Self {
            state,
            owner,
            socket,
            tty,
            redrawing,
            output: held_output,
            _dir: dir,
        }
    }

    /// Run `command` as the agent runs a hook, with `event` on stdin, and fail
    /// if it wrote anything to the tty the TUI is redrawing.
    async fn hook(&self, command: &str, event: &serde_json::Value) {
        use std::io::Write;
        use std::process::{Command, Stdio};
        let command = command.replace(
            crate::agent_hook::tty_resolve(),
            &format!("__t='{}'", self.tty.display()),
        );
        let (owner, socket, input) = (self.owner, self.socket.clone(), event.to_string());
        tokio::task::spawn_blocking(move || {
            let mut child = Command::new("sh")
                .args(["-c", &command])
                .env("TUIC_SESSION", owner)
                .env("TUIC_SOCKET", socket)
                .stdin(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(input.as_bytes())
                .unwrap();
            assert!(child.wait().unwrap().success());
        })
        .await
        .unwrap();
        assert!(
            !self.tty.exists(),
            "the hook wrote to the agent tty: {:?}",
            std::fs::read(&self.tty).unwrap_or_default()
        );
    }

    /// Stop the spinner and clear the screen, so only hooks move the state.
    fn quiet(&self) {
        self.redrawing.store(false, Ordering::Relaxed);
        std::thread::sleep(std::time::Duration::from_millis(5));
        self.output.send(b"\x1b[2J\x1b[H".to_vec()).unwrap();
    }

    fn screen_rows(&self, sid: &str) -> Vec<String> {
        self.state
            .grid
            .vt_log_buffers
            .get(sid)
            .unwrap()
            .lock()
            .screen_rows()
    }
}

#[cfg(unix)]
impl Drop for HookChannelSession {
    fn drop(&mut self) {
        self.redrawing.store(false, Ordering::Relaxed);
    }
}

#[cfg(unix)]
fn claude_hook_for(event: &serde_json::Value) -> String {
    crate::agent_hook::claude_hook_map()
        .into_iter()
        .find(|(e, m, _)| *e == event["hook_event_name"].as_str().unwrap() && m.is_empty())
        .unwrap()
        .2
}

// Catches: a Claude hook receipt still travels through the agent tty, where a
// TUI redraw splits it and its base64 prints over the screen, or a large
// PostToolUse receipt is dropped and its permission wait never clears
// (#2749-bee7).
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn claude_hook_receipts_reach_the_session_while_the_tui_redraws() {
    let sid = "claude-hook-side-channel";
    let session = HookChannelSession::start(sid, "claude", "claude-test-owner");
    let events = recorded_claude_permissions("accept");
    for event in &events[1..4] {
        session.hook(&claude_hook_for(event), event).await;
    }
    assert!(await_session(&session.state, sid, |s| s.awaiting_input).await);
    let mut post = events[4].clone();
    post["tool_name"] = "Read".into();
    post["tool_response"] = serde_json::json!({"file": {"content": "x".repeat(200 * 1024)}});
    session.hook(&claude_hook_for(&post), &post).await;
    assert!(await_session(&session.state, sid, |s| !s.awaiting_input).await);
    let rows = session.screen_rows(sid);
    assert!(
        rows.iter()
            .map(|row| row.trim())
            .all(|row| row.is_empty() || row == "✻ Working"),
        "hook payload reached the screen: {rows:#?}"
    );
}

// Catches: Grok hooks still use the tty, so their receipts can print over the
// Grok screen, or the side channel loses the permission wait and its
// settlement (#2749-bee7).
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn grok_hook_receipts_reach_the_session_while_the_tui_redraws() {
    let sid = "grok-hook-side-channel";
    let session = HookChannelSession::start(sid, "grok", "grok-test-owner");
    let command = crate::agent_hook::grok_hook_map().remove(0).2;
    for event in recorded_grok_hooks("allow") {
        session.hook(&command, &event).await;
        if event["hookEventName"] == "notification" {
            assert!(await_session(&session.state, sid, |s| s.awaiting_input).await);
        }
        if event["hookEventName"] == "post_tool_use" {
            assert!(await_session(&session.state, sid, |s| !s.awaiting_input).await);
        }
    }
}

/// The system-wide monotonic clock a hook reads at process start, read here
/// independently of the production helper so the test is its own oracle.
#[cfg(unix)]
fn hook_clock_now() -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `ts` is a valid, writable timespec.
    assert_eq!(
        unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) },
        0
    );
    ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64
}

/// POST `frames` to the hook route as `tuic agent-state` does: stamped with
/// the hook's process-start clock reading and its pid.
#[cfg(unix)]
async fn post_stamped(
    socket: &std::path::Path,
    owner: &str,
    frames: &str,
    stamp: u64,
    pid: u32,
) -> u16 {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::UnixStream::connect(socket).await.unwrap();
    let (stamp, pid) = (stamp.to_string(), pid.to_string());
    let request = tuic_ipc::http::request(
        "POST",
        &format!("/sessions/{owner}/hook"),
        Some(frames),
        &[("x-tuic-hook-stamp", &stamp), ("x-tuic-hook-pid", &pid)],
    );
    stream.write_all(&request).await.unwrap();
    let mut decoder = tuic_ipc::http::ResponseDecoder::default();
    let mut buf = [0u8; 1024];
    loop {
        let n = stream.read(&mut buf).await.unwrap();
        decoder.push(&buf[..n]);
        if let Some(response) = decoder.response(n == 0).unwrap() {
            return response.status;
        }
    }
}

#[cfg(unix)]
fn hook_shell_state(state: &AppState, sid: &str) -> u8 {
    state
        .session_maps
        .shell_states
        .get(sid)
        .map(|atom| atom.load(Ordering::Relaxed))
        .unwrap()
}

#[cfg(unix)]
async fn await_shell_state(state: &AppState, sid: &str, want: u8) -> bool {
    for _ in 0..200 {
        if hook_shell_state(state, sid) == want {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    false
}

// Catches: an out-of-order pipe hook (a PreToolUse busy that the agent gave up
// on and that lands after Stop) revives a settled turn, a hook of the previous
// PTY generation reaches its replacement, or a repeated delivery is applied
// twice (#2632-97ef). Order comes from the hook's process-start monotonic
// stamp, never from arrival time or the wall clock.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn late_pipe_hook_cannot_revive_settled_turn() {
    let sid = "late-pipe-hook";
    let before_generation = hook_clock_now();
    let session = HookChannelSession::start(sid, "claude", "late-pipe-owner");
    session.quiet();
    let (socket, owner) = (session.socket.clone(), session.owner);
    // The reader thread registers the inbox; before that every post is a 404.
    for _ in 0..200 {
        if session.state.session_maps.hook_inboxes.contains_key(sid) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    const BUSY: &str = "\x1b]7770;state=busy\x1b\\";
    const IDLE: &str = "\x1b]7770;state=idle\x1b\\";
    const PROMPT: &str = "\x1b]7770;state=prompt\x1b\\";

    assert_eq!(
        post_stamped(&socket, owner, BUSY, before_generation, 10).await,
        409,
        "a hook started before this PTY generation must be refused"
    );

    let busy_at = hook_clock_now();
    let late_busy_at = hook_clock_now();
    assert_eq!(post_stamped(&socket, owner, BUSY, busy_at, 11).await, 204);
    assert!(await_shell_state(&session.state, sid, SHELL_BUSY).await);
    let idle_at = hook_clock_now();
    assert_eq!(post_stamped(&socket, owner, IDLE, idle_at, 12).await, 204);
    assert!(await_shell_state(&session.state, sid, SHELL_IDLE).await);

    assert_eq!(
        post_stamped(&socket, owner, BUSY, late_busy_at, 13).await,
        409,
        "a hook started before the settlement must not reach the reducer"
    );
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(
        hook_shell_state(&session.state, sid),
        SHELL_IDLE,
        "the late busy hook revived the settled turn"
    );

    let prompt_at = hook_clock_now();
    assert_eq!(
        post_stamped(&socket, owner, PROMPT, prompt_at, 14).await,
        204
    );
    assert!(await_shell_state(&session.state, sid, SHELL_BUSY).await);
    assert_eq!(
        post_stamped(&socket, owner, PROMPT, prompt_at, 14).await,
        409,
        "a repeated delivery of one hook must be applied once"
    );
}
