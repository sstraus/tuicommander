use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;

use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio::process::Command;

use super::agent::discover_agent_socket;
use super::backoff::BackoffCalculator;
use super::classifier::{ExitReason, classify_exit};
use super::command::{build_ssh_args, build_ssh_env, ensure_ssh_control_dir};
use super::port::check_local_port;
#[cfg(unix)]
use super::port::kill_ssh_on_port;
use super::profile::{ForwardSpec, TunnelProfile};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TunnelStatus {
    Starting,
    Connected,
    Reconnecting { attempt: u32, reason: String },
    Stopped { reason: String },
    Error { message: String },
}

/// Shared budget for a forwarded socket to become usable and for its caller to wait.
pub(crate) const TUNNEL_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const FORWARD_POLL: Duration = Duration::from_millis(250);
const MIN_SSH_SURVIVAL: Duration = Duration::from_millis(500);

/// Bound for `stop_and_wait`/`stop_and_take_task`-based confirmation that a
/// tunnel's underlying OS process has actually exited. `graceful_kill`'s own
/// SIGTERM grace period is 5s; this adds a buffer for signal delivery,
/// task-join scheduling, and (on Windows) the `taskkill /T` tree-kill — a
/// caller (production app-exit, or a test) can rely on this as an absolute
/// ceiling and never hang indefinitely on a stuck supervisor.
pub(crate) const GRACEFUL_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(7);

pub struct TunnelSupervisor {
    profile: TunnelProfile,
    status: Arc<Mutex<TunnelStatus>>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    ssh_binary: PathBuf,
    /// The supervision loop's task handle. Resolves once the loop returns,
    /// which only happens after any live child process has been confirmed
    /// dead (see `supervision_loop`'s shutdown branches, which `.await`
    /// `graceful_kill` before returning) — awaiting this is therefore a real
    /// confirmation that the OS process is gone, not just that in-memory
    /// status/bookkeeping updated. `None` when nothing was ever spawned
    /// (validation/port-check failed first) or after a previous
    /// `stop_and_take_task` call already consumed it.
    task: Option<tokio::task::JoinHandle<()>>,
}

impl TunnelSupervisor {
    /// Start supervising an SSH tunnel for the given profile.
    ///
    /// `status_callback` is invoked on every status transition from a spawned
    /// tokio task — it must be `Send + 'static`.
    pub async fn start(
        profile: TunnelProfile,
        status_callback: impl Fn(TunnelStatus) + Send + 'static,
    ) -> Self {
        Self::start_with_binary(profile, PathBuf::from("ssh"), status_callback).await
    }

    /// Like `start`, but allows overriding the ssh binary path (for tests).
    pub(crate) async fn start_with_binary(
        mut profile: TunnelProfile,
        ssh_binary: PathBuf,
        status_callback: impl Fn(TunnelStatus) + Send + 'static,
    ) -> Self {
        let status = Arc::new(Mutex::new(TunnelStatus::Starting));

        // Validate profile.
        if let Err(e) = profile.validate() {
            let error_status = TunnelStatus::Error { message: e };
            *status.lock() = error_status.clone();
            status_callback(error_status);
            return Self {
                profile,
                status,
                shutdown_tx: None,
                ssh_binary,
                task: None,
            };
        }

        // Check port availability for all Local forwards.
        // If a port is in use, try to kill orphaned SSH processes holding it.
        for forward in &profile.forwards {
            if let ForwardSpec::Local { bind_port, .. } = forward
                && check_local_port(*bind_port).await.is_err()
            {
                #[cfg(unix)]
                {
                    kill_ssh_on_port(*bind_port).await;
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
                if let Err(msg) = check_local_port(*bind_port).await {
                    let error_status = TunnelStatus::Error { message: msg };
                    *status.lock() = error_status.clone();
                    status_callback(error_status);
                    return Self {
                        profile,
                        status,
                        shutdown_tx: None,
                        ssh_binary,
                        task: None,
                    };
                }
            }
        }

        status_callback(TunnelStatus::Starting);

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let task_status = Arc::clone(&status);
        let task_profile = profile.clone();
        let task_binary = ssh_binary.clone();

        let task = tokio::spawn(async move {
            supervision_loop(
                task_profile,
                task_binary,
                task_status,
                shutdown_rx,
                status_callback,
            )
            .await;
        });

        Self {
            profile,
            status,
            shutdown_tx: Some(shutdown_tx),
            ssh_binary,
            task: Some(task),
        }
    }

    /// Request graceful shutdown of the supervised tunnel.
    pub fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }

    /// Request shutdown and return the supervision loop's task handle, if
    /// one was ever spawned. Awaiting the returned handle confirms the
    /// underlying OS process is actually gone (see `task`'s doc comment) —
    /// deliberately non-async and non-blocking itself, so a caller already
    /// holding a lock on this supervisor (e.g. `TunnelManager`, behind a
    /// `parking_lot::Mutex` whose guard isn't `Send`) can drop that lock
    /// before awaiting what's returned here.
    pub(crate) fn stop_and_take_task(&mut self) -> Option<tokio::task::JoinHandle<()>> {
        self.stop();
        self.task.take()
    }

    /// Convenience wrapper for direct (non-`TunnelManager`) callers — this
    /// module's own tests — that request shutdown and wait, bounded by
    /// `GRACEFUL_SHUTDOWN_TIMEOUT`, for confirmation that the underlying OS
    /// process actually exited. Returns `true` if confirmed within the
    /// timeout; `false` on timeout, in which case the supervision task keeps
    /// running in the background regardless (each spawned `Command` also has
    /// `kill_on_drop(true)` set as a last-resort safety net for whenever that
    /// task is eventually dropped). Safe to call more than once, and safe to
    /// call after the tunnel already stopped on its own (a non-retryable
    /// exit, a spawn failure, etc.) — the stored task handle resolves
    /// immediately in that case.
    #[cfg(test)]
    pub(crate) async fn stop_and_wait(&mut self) -> bool {
        let Some(task) = self.stop_and_take_task() else {
            return true;
        };
        tokio::time::timeout(GRACEFUL_SHUTDOWN_TIMEOUT, task)
            .await
            .is_ok()
    }

    /// Return the current tunnel status.
    pub fn status(&self) -> TunnelStatus {
        self.status.lock().clone()
    }
}

fn set_status(status: &Mutex<TunnelStatus>, new: TunnelStatus, callback: &impl Fn(TunnelStatus)) {
    *status.lock() = new.clone();
    callback(new);
}

async fn supervision_loop(
    profile: TunnelProfile,
    ssh_binary: PathBuf,
    status: Arc<Mutex<TunnelStatus>>,
    mut shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    callback: impl Fn(TunnelStatus) + Send + 'static,
) {
    let agent_socket = discover_agent_socket();
    let mut backoff = BackoffCalculator::new();

    loop {
        if let Err(error) = ensure_ssh_control_dir() {
            set_status(
                &status,
                TunnelStatus::Error {
                    message: format!("failed to create SSH control directory: {error}"),
                },
                &callback,
            );
            return;
        }
        let args = build_ssh_args(&profile);
        let env = build_ssh_env(agent_socket.as_deref());

        // Build command — skip argv[0] ("ssh") from args since we set the binary separately.
        let mut cmd = Command::new(&ssh_binary);
        cmd.args(&args[1..])
            .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        // A fresh process group (id == the child's own PID), so
        // `graceful_kill` can signal the whole tree via `kill(-pid, ...)`
        // instead of just this one process — see that function's doc
        // comment for why a single-PID signal isn't enough (a shell-script
        // `ssh` stand-in, or a real `ssh` with a `ProxyCommand`, forks a
        // child that isn't automatically killed when its parent dies from
        // an unhandled signal).
        #[cfg(unix)]
        cmd.process_group(0);

        // Retry spawn briefly on transient OS errors (Linux ETXTBSY: race
        // between closing a write fd and execve on the same temp script).
        let mut child = 'spawn: {
            let mut last_err = None;
            for attempt in 0..3u8 {
                match cmd.spawn() {
                    Ok(c) => break 'spawn c,
                    Err(e) if is_retryable_spawn_error(&e) && attempt < 2 => {
                        last_err = Some(e);
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                    Err(e) => {
                        last_err = Some(e);
                        break;
                    }
                }
            }
            set_status(
                &status,
                TunnelStatus::Error {
                    message: format!("failed to spawn ssh: {}", last_err.unwrap()),
                },
                &callback,
            );
            return;
        };

        // Drain stderr concurrently on a background task, retaining only a
        // bounded tail for diagnostics. Without this, a chatty ssh fills the
        // OS pipe buffer (64KB on Linux, 16KB on macOS) and blocks on write()
        // forever — the child never exits and the status stays stuck at
        // Connected even though ssh is effectively hung.
        let stderr_tail = child.stderr.take().map(spawn_stderr_drainer);
        // `child.id()` is `None` once the child has been waited on, so keep the
        // group id (== the leader's PID, see `process_group(0)`) for the
        // exit-on-its-own paths below.
        let pgid = child.id();

        // A running SSH process needs a brief survival window, and its -L
        // sockets may take longer to listen across a WAN. Require both.
        let ready = tokio::select! {
            biased;
            result = child.wait() => Some(result),
            result = tokio::time::timeout(TUNNEL_CONNECT_TIMEOUT, wait_until_ready(&profile)) => {
                if result.is_err() {
                    graceful_kill(&mut child).await;
                    set_status(&status, TunnelStatus::Error { message: "SSH local forward did not listen in time".to_string() }, &callback);
                    return;
                }
                None
            },
            _ = &mut shutdown_rx => {
                graceful_kill(&mut child).await;
                set_status(&status, TunnelStatus::Stopped { reason: "shutdown requested".to_string() }, &callback);
                return;
            }
        };

        if let Some(wait_result) = ready {
            // Process died before its forwards were ready.
            reap_process_group(pgid).await;
            let stderr = stderr_tail_snapshot(&stderr_tail);
            let code = wait_result.ok().and_then(|s| s.code());
            let reason = classify_exit(&stderr, code);
            if handle_exit(&reason, &mut backoff, &status, &callback) {
                // Retryable — wait backoff then loop.
                if let Some(delay) = backoff_delay(&mut backoff) {
                    tokio::select! {
                        () = tokio::time::sleep(delay) => {}
                        _ = &mut shutdown_rx => {
                            set_status(&status, TunnelStatus::Stopped { reason: "shutdown requested".to_string() }, &callback);
                            return;
                        }
                    }
                } else {
                    set_status(
                        &status,
                        TunnelStatus::Stopped {
                            reason: "max retries exceeded".to_string(),
                        },
                        &callback,
                    );
                    return;
                }
                continue;
            }
            return;
        }

        // SSH survived startup and all local forwards accept TCP connections.
        backoff.reset();
        set_status(&status, TunnelStatus::Connected, &callback);

        // Wait for process exit or shutdown signal.
        let wait_result = tokio::select! {
            result = child.wait() => result,
            _ = &mut shutdown_rx => {
                graceful_kill(&mut child).await;
                set_status(&status, TunnelStatus::Stopped { reason: "shutdown requested".to_string() }, &callback);
                return;
            }
        };

        // ssh exited on its own (network drop, auth failure): its leader is
        // gone but a `ProxyCommand` descendant may not be.
        reap_process_group(pgid).await;
        let stderr = stderr_tail_snapshot(&stderr_tail);
        let code = wait_result.ok().and_then(|s| s.code());
        let reason = classify_exit(&stderr, code);

        if handle_exit(&reason, &mut backoff, &status, &callback) {
            // Retryable — wait backoff then loop.
            if let Some(delay) = backoff_delay(&mut backoff) {
                tokio::select! {
                    () = tokio::time::sleep(delay) => {}
                    _ = &mut shutdown_rx => {
                        set_status(&status, TunnelStatus::Stopped { reason: "shutdown requested".to_string() }, &callback);
                        return;
                    }
                }
            } else {
                set_status(
                    &status,
                    TunnelStatus::Stopped {
                        reason: "max retries exceeded".to_string(),
                    },
                    &callback,
                );
                return;
            }
            continue;
        }

        // Non-retryable — already set by handle_exit.
        return;
    }
}

async fn wait_until_ready(profile: &TunnelProfile) {
    tokio::time::sleep(MIN_SSH_SURVIVAL).await;
    loop {
        let mut all_ready = true;
        for forward in &profile.forwards {
            if let ForwardSpec::Local { bind_port, .. } = forward {
                let addr = (std::net::Ipv4Addr::LOCALHOST, *bind_port);
                if !matches!(
                    tokio::time::timeout(FORWARD_POLL, TcpStream::connect(addr)).await,
                    Ok(Ok(_))
                ) {
                    all_ready = false;
                    break;
                }
            }
        }
        if all_ready {
            return;
        }
        tokio::time::sleep(FORWARD_POLL).await;
    }
}

/// Returns `true` if the exit is retryable (caller should loop), `false` if
/// the supervisor should stop. Updates status accordingly.
fn handle_exit(
    reason: &ExitReason,
    backoff: &mut BackoffCalculator,
    status: &Mutex<TunnelStatus>,
    callback: &impl Fn(TunnelStatus),
) -> bool {
    if reason.is_retryable() {
        let attempt = backoff.attempts() + 1;
        let reason_str = format!("{reason:?}");
        set_status(
            status,
            TunnelStatus::Reconnecting {
                attempt,
                reason: reason_str,
            },
            callback,
        );
        true
    } else {
        let reason_str = format!("{reason:?}");
        set_status(
            status,
            TunnelStatus::Stopped { reason: reason_str },
            callback,
        );
        false
    }
}

/// Get the next backoff delay, or `None` if retries are exhausted.
fn backoff_delay(backoff: &mut BackoffCalculator) -> Option<Duration> {
    backoff.next_delay()
}

/// Cap on the retained stderr tail, in bytes. Diagnostics only need the most
/// recent output (e.g. the auth-failure or connection-refused message), not
/// the full chatty stream.
const STDERR_TAIL_LIMIT: usize = 8192;

/// Spawn a background task that continuously drains the child's stderr pipe
/// for as long as the process runs, keeping only a bounded tail.
///
/// This must run concurrently with the process, not after it exits: ssh's
/// stderr is a pipe with a small OS buffer (64KB on Linux, 16KB on macOS).
/// A chatty process fills it and blocks on write() until someone reads —
/// reading only after `child.wait()` returns means nobody ever reads while
/// the process is alive, so it can block forever and never exit.
fn spawn_stderr_drainer(mut stderr: tokio::process::ChildStderr) -> Arc<Mutex<Vec<u8>>> {
    let tail = Arc::new(Mutex::new(Vec::new()));
    let task_tail = Arc::clone(&tail);
    tokio::spawn(async move {
        let mut buf = [0u8; 4096];
        loop {
            match stderr.read(&mut buf).await {
                Ok(0) => break, // EOF — pipe closed (process exited).
                Ok(n) => {
                    let mut guard = task_tail.lock();
                    guard.extend_from_slice(&buf[..n]);
                    if guard.len() > STDERR_TAIL_LIMIT {
                        let excess = guard.len() - STDERR_TAIL_LIMIT;
                        guard.drain(0..excess);
                    }
                }
                Err(e) => {
                    tracing::warn!(source = "tunnel_supervisor", error = %e, "Failed to read ssh stderr");
                    break;
                }
            }
        }
    });
    tail
}

/// Snapshot whatever stderr tail has been drained so far, for diagnostics.
fn stderr_tail_snapshot(tail: &Option<Arc<Mutex<Vec<u8>>>>) -> String {
    match tail {
        Some(tail) => String::from_utf8_lossy(&tail.lock()).into_owned(),
        None => String::new(),
    }
}

/// After the group leader (ssh) has exited on its own, make sure no member of
/// its dedicated process group outlives it.
///
/// A process group persists until its last member exits, and `kill(-pgid, ..)`
/// keeps working for as long as any member lives, so a `ProxyCommand`
/// descendant that survived its parent is still reachable here. The common
/// case (no survivors) costs one `kill` that returns ESRCH. Otherwise SIGTERM,
/// poll up to 5s (same grace as `graceful_kill`), then SIGKILL. Must run
/// before the exit is acted on (status, retry/backoff) so a retry never
/// overlaps stale descendants and `Stopped` implies the group is gone.
async fn reap_process_group(pgid: Option<u32>) {
    #[cfg(unix)]
    {
        let Some(pgid) = pgid.and_then(|id| i32::try_from(id).ok()) else {
            return;
        };
        // SAFETY: `pgid` was the PID of a child we spawned into its own group
        // via `process_group(0)`; signalling `-pgid` only reaches that group,
        // and ESRCH (no members left) is the expected no-op.
        if unsafe { libc::kill(-pgid, libc::SIGTERM) } != 0 {
            return;
        }
        for _ in 0..100 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            // SAFETY: signal 0 only probes for existence.
            if unsafe { libc::kill(-pgid, 0) } != 0 {
                return;
            }
        }
        // SAFETY: as above.
        unsafe {
            libc::kill(-pgid, libc::SIGKILL);
        }
    }
    #[cfg(not(unix))]
    let _ = pgid;
}

/// Send SIGTERM, wait 5s, escalate to SIGKILL.
///
/// Signals the whole process **group**, not just the direct child, on Unix
/// (the child is spawned into its own group via `process_group(0)` above
/// specifically so this is safe/scoped) — mirroring the `taskkill /T`
/// tree-kill already done for Windows below. A single-PID signal isn't
/// enough: found via a real leaked-process bug (a `sleep 3600` fake-ssh test
/// fixture — a multi-line shell script, not a single exec'd binary — outlived
/// its own test by a full hour). The script's `/bin/sh` interpreter forked
/// `sleep` as its own child rather than exec-replacing itself with it; SIGTERM
/// killed the shell but never reached the orphaned `sleep` underneath it. A
/// real `ssh` with a `ProxyCommand` has the identical shape.
///
/// Confirming the reap (an explicit `child.wait()` after every kill, not just
/// firing the signal and returning) matters for the same reason on both
/// platforms: `stop_and_wait`/`stop_and_take_task` treat "this function
/// returned" as "the OS process is gone," and only awaiting `wait()` actually
/// establishes that.
async fn graceful_kill(child: &mut tokio::process::Child) {
    #[cfg(unix)]
    if let Some(id) = child.id() {
        match i32::try_from(id) {
            Ok(pid) => {
                // SAFETY: `pid` is from `child.id()` which returns the OS PID
                // of a child process we spawned (into its own process group,
                // via `process_group(0)`) and have not yet waited on.
                // SIGTERM has no preconditions beyond a valid PID/group id.
                // Negating it targets the whole group instead of just `pid`.
                unsafe {
                    libc::kill(-pid, libc::SIGTERM);
                }
            }
            Err(_) => {
                tracing::warn!(
                    source = "tunnel_supervisor",
                    raw_pid = id,
                    "PID overflows i32, escalating to SIGKILL"
                );
                // No usable signed PID to negate for a group-kill here — fall
                // back to killing just the direct child via tokio's own
                // PID-agnostic `Child::kill()`. This overflow case is
                // exceedingly unlikely on any real system (PIDs stay far
                // below `i32::MAX`), so the (rare, theoretical) grandchild
                // gap this leaves is accepted rather than engineered around.
                let _ = child.kill().await;
                let _ = child.wait().await;
                return;
            }
        }
    }
    #[cfg(not(unix))]
    {
        // The tree, not just the child. `Child::kill` calls `TerminateProcess`,
        // which leaves grandchildren running — and a grandchild keeps the
        // stderr write handle open, so the drainer's read never sees EOF. On
        // Windows that read holds a blocking thread the tokio runtime waits for
        // at shutdown, which is a hang rather than a leak. An `ssh` with a
        // `ProxyCommand` has exactly that shape.
        if let Some(id) = child.id() {
            let _ = tokio::process::Command::new(crate::fs::system32_exe("taskkill.exe"))
                .args(["/PID", &id.to_string(), "/T", "/F"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .await;
        }
        let _ = child.kill().await;
        let _ = child.wait().await;
    }

    // Wait up to 5s for clean exit after SIGTERM, then escalate to SIGKILL.
    #[cfg(unix)]
    tokio::select! {
        _ = child.wait() => {}
        () = tokio::time::sleep(Duration::from_secs(5)) => {
            if let Some(id) = child.id()
                && let Ok(pid) = i32::try_from(id)
            {
                // SAFETY: see above — same PID/group, still not yet waited on.
                unsafe {
                    libc::kill(-pid, libc::SIGKILL);
                }
            } else {
                let _ = child.kill().await;
            }
            let _ = child.wait().await;
        }
    }
}

/// ETXTBSY (26 on Linux) — exec on a file still open for writing.
fn is_retryable_spawn_error(e: &std::io::Error) -> bool {
    #[cfg(unix)]
    {
        e.raw_os_error() == Some(libc::ETXTBSY)
    }
    #[cfg(not(unix))]
    {
        let _ = e;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use crate::test_support::{assert_fake_ssh_stopped, fake_ssh_processes};
    use crate::test_support::{fake_ssh_script, system32_exe};
    use std::net::SocketAddr;
    use tokio::net::TcpListener;

    fn test_profile() -> TunnelProfile {
        TunnelProfile {
            id: uuid::Uuid::new_v4().to_string(),
            name: "test-tunnel".to_string(),
            host: "example.com".to_string(),
            port: 22,
            user: "alice".to_string(),
            identity_file: None,
            forwards: Vec::new(),
            options: super::super::profile::ProfileOptions::default(),
            auto_connect: false,
        }
    }

    /// Collect statuses via a shared vec behind Arc<Mutex<_>>.
    fn status_collector() -> (
        impl Fn(TunnelStatus) + Send + 'static,
        Arc<Mutex<Vec<TunnelStatus>>>,
    ) {
        let statuses: Arc<Mutex<Vec<TunnelStatus>>> = Arc::new(Mutex::new(Vec::new()));
        let s = Arc::clone(&statuses);
        let cb = move |st: TunnelStatus| {
            s.lock().push(st);
        };
        (cb, statuses)
    }

    /// Poll until the supervisor settles on `Stopped`.
    ///
    /// The bound covers the supervisor's own state machine and nothing else:
    /// the 500ms survival check, child's exit and at worst the first two backoffs (~1s and ~2s).
    /// It is not sized for process startup — that cost
    /// is paid up front by [`fake_ssh_script`], deliberately, because it is the
    /// one term here that the OS can stretch without limit. Keep it that way:
    /// if this bound ever needs raising, the cause is a supervisor change or a
    /// new unwarmed executable, not a busy machine.
    async fn wait_for_stopped(supervisor: &TunnelSupervisor) -> TunnelStatus {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        let mut status = supervisor.status();
        while !matches!(status, TunnelStatus::Stopped { .. })
            && tokio::time::Instant::now() < deadline
        {
            tokio::time::sleep(Duration::from_millis(50)).await;
            status = supervisor.status();
        }
        status
    }

    /// Catches: a child exiting before readiness being reported Connected.
    #[tokio::test]
    async fn spawn_clean_exit() {
        let script = fake_ssh_script("spawn_clean_exit", "exit 0", "exit /b 0");
        let (cb, statuses) = status_collector();

        let mut sup =
            TunnelSupervisor::start_with_binary(test_profile(), script.to_path_buf(), cb).await;

        // Poll for the terminal state rather than sampling once after a fixed
        // sleep. This child exits inside the minimum survival window.
        let final_status = wait_for_stopped(&sup).await;

        let history = statuses.lock().clone();
        // An early exit must never be reported as a connection.
        assert!(!history.is_empty(), "should have status updates");
        assert!(!history.contains(&TunnelStatus::Connected));

        // Final status should be Stopped with a non-error reason.
        match &final_status {
            TunnelStatus::Stopped { .. } => {} // expected
            other => panic!("expected Stopped, got {other:?}"),
        }

        sup.stop(); // idempotent
    }

    #[tokio::test]
    async fn remote_only_forward_exiting_during_startup_never_connects() {
        let script = fake_ssh_script(
            "remote_only_forward_exiting_during_startup_never_connects",
            "sleep 0.2; exit 255",
            &format!(
                "{} -n 1 127.0.0.1 >nul & exit /b 255",
                system32_exe("ping.exe")
            ),
        );
        let mut profile = test_profile();
        profile.forwards.push(ForwardSpec::Remote {
            bind_port: 9877,
            local_host: "127.0.0.1".to_string(),
            local_port: 9877,
        });
        let (cb, statuses) = status_collector();
        let mut sup = TunnelSupervisor::start_with_binary(profile, script, cb).await;

        assert!(matches!(
            wait_for_stopped(&sup).await,
            TunnelStatus::Stopped { .. }
        ));
        let history = statuses.lock().clone();
        assert!(
            !history.contains(&TunnelStatus::Connected),
            "an SSH process that dies during startup cannot be connected: {history:?}"
        );
        sup.stop();
    }

    #[tokio::test]
    async fn auth_failure_no_retry() {
        let script = fake_ssh_script(
            "auth_failure_no_retry",
            r#"echo "Permission denied (publickey)." >&2; exit 255"#,
            "echo Permission denied ^(publickey^). 1>&2 & exit /b 255",
        );
        let (cb, statuses) = status_collector();

        let mut profile = test_profile();
        profile.forwards.push(ForwardSpec::Local {
            bind_port: super::super::port::find_free_port().await.unwrap(),
            remote_host: "127.0.0.1".to_string(),
            remote_port: 9877,
        });
        let mut sup = TunnelSupervisor::start_with_binary(profile, script.to_path_buf(), cb).await;

        // The script exits immediately, but the stderr drainer has to deliver
        // "Permission denied" before classify_exit can call it AuthFailed. Wait
        // for the terminal state instead of sampling at a fixed offset.
        let final_status = wait_for_stopped(&sup).await;

        let history = statuses.lock().clone();

        // Must NOT contain Reconnecting — auth failures are not retryable.
        let has_reconnecting = history
            .iter()
            .any(|s| matches!(s, TunnelStatus::Reconnecting { .. }));
        assert!(
            !has_reconnecting,
            "auth failure should not trigger reconnect, history: {history:?}"
        );
        assert!(
            !history.contains(&TunnelStatus::Connected),
            "a child that exits before its forward listens was never connected: {history:?}"
        );

        // Final status should be Stopped with AuthFailed reason.
        match &final_status {
            TunnelStatus::Stopped { reason } => {
                assert!(
                    reason.contains("AuthFailed"),
                    "reason should mention AuthFailed, got: {reason}"
                );
            }
            other => panic!("expected Stopped, got {other:?}"),
        }

        sup.stop();
    }

    #[tokio::test]
    async fn network_error_retries() {
        // Script that prints "Connection refused" and exits — supervisor should retry.
        let script = fake_ssh_script(
            "network_error_retries",
            r#"echo "ssh: connect to host example.com port 22: Connection refused" >&2; exit 255"#,
            "echo ssh: connect to host example.com port 22: Connection refused 1>&2 & exit /b 255",
        );
        let (cb, statuses) = status_collector();

        let mut sup =
            TunnelSupervisor::start_with_binary(test_profile(), script.to_path_buf(), cb).await;

        // The first two backoffs are roughly one and two seconds, and each
        // retry re-execs the script, so poll the observed transitions rather
        // than sleeping for a total nobody can predict exactly.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        while statuses
            .lock()
            .iter()
            .filter(|status| matches!(status, TunnelStatus::Reconnecting { .. }))
            .count()
            < 2
            && tokio::time::Instant::now() < deadline
        {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        let history = statuses.lock().clone();

        // Should contain at least one Reconnecting status.
        let reconnect_count = history
            .iter()
            .filter(|s| matches!(s, TunnelStatus::Reconnecting { .. }))
            .count();
        assert!(
            reconnect_count >= 2,
            "expected at least 2 reconnect attempts, got {reconnect_count}, history: {history:?}"
        );

        // Verify attempt numbers increase.
        let attempts: Vec<u32> = history
            .iter()
            .filter_map(|s| {
                if let TunnelStatus::Reconnecting { attempt, .. } = s {
                    Some(*attempt)
                } else {
                    None
                }
            })
            .collect();
        for window in attempts.windows(2) {
            assert!(
                window[1] > window[0],
                "attempt numbers should increase: {attempts:?}"
            );
        }

        sup.stop();
    }

    #[tokio::test]
    async fn graceful_shutdown() {
        // Script that sleeps forever.
        #[cfg(unix)]
        let marker = crate::test_support::test_temp_root().join("graceful_shutdown_ssh.pid");
        #[cfg(unix)]
        let _ = std::fs::remove_file(&marker);
        #[cfg(unix)]
        let posix = format!("echo $$ > '{}'; exec sleep 3600", marker.display());
        #[cfg(not(unix))]
        let posix = "sleep 3600".to_string();
        let script = fake_ssh_script(
            "graceful_shutdown",
            &posix,
            &format!("{} -n 3601 127.0.0.1 >nul", system32_exe("ping.exe")),
        );
        let (cb, _statuses) = status_collector();

        let mut sup =
            TunnelSupervisor::start_with_binary(test_profile(), script.to_path_buf(), cb).await;

        // Allow the no-forward tunnel to survive startup.
        tokio::time::sleep(Duration::from_millis(800)).await;

        // Should be Connected.
        assert_eq!(sup.status(), TunnelStatus::Connected);

        #[cfg(unix)]
        let pids = fake_ssh_processes(&marker);

        // Request shutdown.
        sup.stop();

        // The implementation has a five-second grace period before it escalates
        // to SIGKILL; poll across that boundary rather than guessing which side
        // of it the child exits on.
        let final_status = wait_for_stopped(&sup).await;
        match &final_status {
            TunnelStatus::Stopped { reason } => {
                assert!(
                    reason.contains("shutdown"),
                    "reason should mention shutdown, got: {reason}"
                );
            }
            other => panic!("expected Stopped after shutdown, got {other:?}"),
        }

        #[cfg(unix)]
        assert_fake_ssh_stopped(pids);
    }

    /// Catches: `graceful_kill` signalling only the direct child. The fake ssh
    /// forks `sleep` (a grandchild that SIGTERM to the shell alone never
    /// reaches); the whole process group must die before `stop_and_wait`
    /// returns.
    #[cfg(unix)]
    #[tokio::test]
    async fn shutdown_kills_forked_grandchild_before_returning() {
        let root = crate::test_support::test_temp_root();
        let child_marker = root.join("grandchild_shutdown_ssh_child.pid");
        let _ = std::fs::remove_file(&child_marker);
        let posix = format!("sleep 3600 &\necho $! > '{}'\nwait", child_marker.display());
        let script = fake_ssh_script("grandchild_shutdown", &posix, "exit /b 0");
        let (cb, _statuses) = status_collector();

        let mut sup =
            TunnelSupervisor::start_with_binary(test_profile(), script.to_path_buf(), cb).await;
        let grandchild: i32 = tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                if let Ok(text) = std::fs::read_to_string(&child_marker)
                    && let Ok(pid) = text.trim().parse::<i32>()
                {
                    return pid;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("fake SSH forked its grandchild");
        assert!(
            unsafe { libc::kill(grandchild, 0) } == 0,
            "grandchild should be alive before shutdown"
        );

        assert!(sup.stop_and_wait().await, "shutdown must be confirmed");

        // Allow only for init reaping the orphan; the group signal itself has
        // already been delivered and the shell reaped by `stop_and_wait`.
        let mut alive = true;
        for _ in 0..40 {
            alive = unsafe { libc::kill(grandchild, 0) } == 0;
            if !alive {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        if alive {
            unsafe { libc::kill(grandchild, libc::SIGKILL) };
        }
        assert!(!alive, "forked grandchild {grandchild} survived shutdown");
    }

    /// Catches: ssh exiting on its own (network drop, auth failure) leaving a
    /// forked descendant (a `ProxyCommand`) alive in the dedicated process
    /// group. The fake ssh forks `sleep` and exits; once the supervisor has
    /// settled on `Stopped` the sleeper must already be gone.
    #[cfg(unix)]
    #[tokio::test]
    async fn ssh_exiting_on_its_own_does_not_leave_a_group_descendant() {
        let root = crate::test_support::test_temp_root();
        let child_marker = root.join("self_exit_ssh_child.pid");
        let _ = std::fs::remove_file(&child_marker);
        let posix = format!(
            "sleep 3600 &\necho $! > '{}'\nexit 0",
            child_marker.display()
        );
        let script = fake_ssh_script("self_exit_group", &posix, "exit /b 0");
        let (cb, _statuses) = status_collector();

        let mut sup =
            TunnelSupervisor::start_with_binary(test_profile(), script.to_path_buf(), cb).await;
        assert!(matches!(
            wait_for_stopped(&sup).await,
            TunnelStatus::Stopped { .. }
        ));
        let sleeper: i32 = std::fs::read_to_string(&child_marker)
            .expect("fake SSH forked its descendant")
            .trim()
            .parse()
            .unwrap();

        // Allow only for init reaping the orphan; the group signal has
        // already been delivered before `Stopped` was published.
        let mut alive = true;
        for _ in 0..40 {
            alive = unsafe { libc::kill(sleeper, 0) } == 0;
            if !alive {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        if alive {
            unsafe { libc::kill(sleeper, libc::SIGKILL) };
        }
        sup.stop();
        assert!(!alive, "descendant {sleeper} survived ssh's own exit");
    }

    #[tokio::test]
    async fn local_forward_stays_starting_until_its_port_accepts_connections() {
        let first = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let second = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = first.local_addr().unwrap().port();
        let second_port = second.local_addr().unwrap().port();
        drop(first);
        drop(second);
        let mut profile = test_profile();
        profile.forwards.push(ForwardSpec::Local {
            bind_port: port,
            remote_host: "127.0.0.1".to_string(),
            remote_port: 9877,
        });
        profile.forwards.push(ForwardSpec::Local {
            bind_port: second_port,
            remote_host: "127.0.0.1".to_string(),
            remote_port: 9878,
        });
        // The long-lived SSH stub and the delayed listener together model an
        // SSH process that is alive before its -L forwarding socket is ready.
        #[cfg(unix)]
        let marker = crate::test_support::test_temp_root().join("local_forward_ssh.pid");
        #[cfg(unix)]
        let _ = std::fs::remove_file(&marker);
        #[cfg(unix)]
        let posix = format!("echo $$ > '{}'; exec sleep 3600", marker.display());
        #[cfg(not(unix))]
        let posix = "sleep 3600".to_string();
        let script = fake_ssh_script(
            "local_forward_stays_starting_until_its_port_accepts_connections",
            &posix,
            &format!("{} -n 3601 127.0.0.1 >nul", system32_exe("ping.exe")),
        );
        let (cb, _) = status_collector();
        let mut sup = TunnelSupervisor::start_with_binary(profile, script, cb).await;

        tokio::time::sleep(Duration::from_millis(1500)).await;
        assert_eq!(sup.status(), TunnelStatus::Starting);

        let addr: SocketAddr = ([127, 0, 0, 1], port).into();
        let listener = TcpListener::bind(addr).await.unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert_eq!(sup.status(), TunnelStatus::Starting);

        let second_addr: SocketAddr = ([127, 0, 0, 1], second_port).into();
        let second_listener = TcpListener::bind(second_addr).await.unwrap();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        while sup.status() == TunnelStatus::Starting && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert_eq!(sup.status(), TunnelStatus::Connected);
        assert!(tokio::net::TcpStream::connect(addr).await.is_ok());
        assert!(tokio::net::TcpStream::connect(second_addr).await.is_ok());
        #[cfg(unix)]
        let pids = fake_ssh_processes(&marker);
        sup.stop();
        assert!(matches!(
            wait_for_stopped(&sup).await,
            TunnelStatus::Stopped { .. }
        ));
        #[cfg(unix)]
        assert_fake_ssh_stopped(pids);
        drop(listener);
        drop(second_listener);
    }

    #[tokio::test]
    async fn port_in_use_error_before_spawn() {
        // Bind a port so it's occupied.
        let addr: SocketAddr = ([127, 0, 0, 1], 0).into();
        let listener = TcpListener::bind(addr).await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let mut profile = test_profile();
        profile.forwards = vec![ForwardSpec::Local {
            bind_port: port,
            remote_host: "remote.example.com".to_string(),
            remote_port: 80,
        }];

        let script = fake_ssh_script("port_in_use_error_before_spawn", "exit 0", "exit /b 0");
        let (cb, _statuses) = status_collector();

        let sup = TunnelSupervisor::start_with_binary(profile, script.to_path_buf(), cb).await;

        // Should immediately be in Error state — no spawn.
        let status = sup.status();
        match &status {
            TunnelStatus::Error { message } => {
                assert!(
                    message.contains("already in use"),
                    "message should mention port in use, got: {message}"
                );
            }
            other => panic!("expected Error for port in use, got {other:?}"),
        }

        drop(listener); // release the port
    }

    #[tokio::test]
    async fn chatty_stderr_does_not_stall() {
        // Emit >64KB of stderr, then exit cleanly. If stderr isn't drained
        // concurrently while the process runs, the OS pipe buffer (64KB on
        // Linux, 16KB on macOS) fills, the child blocks forever on write(),
        // and the tunnel never reaches Stopped — it stalls at Connected.
        let script = fake_ssh_script(
            "chatty_stderr_does_not_stall",
            "yes x | head -c 100000 1>&2; exit 0",
            // 1000 lines of 100 characters: past the 64KB pipe buffer, and a
            // loop `cmd` gets through in well under a second.
            "for /L %%i in (1,1,1000) do @echo xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx 1>&2\r\nexit /b 0",
        );
        let (cb, _statuses) = status_collector();

        let mut sup =
            TunnelSupervisor::start_with_binary(test_profile(), script.to_path_buf(), cb).await;

        let final_status = wait_for_stopped(&sup).await;

        match &final_status {
            TunnelStatus::Stopped { .. } => {} // expected: exited promptly, no stall
            other => {
                panic!("expected Stopped (chatty stderr must not stall the tunnel), got {other:?}")
            }
        }

        sup.stop();
    }
}
