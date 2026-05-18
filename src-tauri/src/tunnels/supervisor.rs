use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;

use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use super::agent::discover_agent_socket;
use super::backoff::BackoffCalculator;
use super::classifier::{ExitReason, classify_exit};
use super::command::{build_ssh_args, build_ssh_env};
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

pub struct TunnelSupervisor {
    profile: TunnelProfile,
    status: Arc<Mutex<TunnelStatus>>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    ssh_binary: PathBuf,
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
                    };
                }
            }
        }

        status_callback(TunnelStatus::Starting);

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let task_status = Arc::clone(&status);
        let task_profile = profile.clone();
        let task_binary = ssh_binary.clone();

        tokio::spawn(async move {
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
        }
    }

    /// Request graceful shutdown of the supervised tunnel.
    pub fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
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

        let spawn_result = cmd.spawn();
        let mut child = match spawn_result {
            Ok(c) => c,
            Err(e) => {
                set_status(
                    &status,
                    TunnelStatus::Error {
                        message: format!("failed to spawn ssh: {e}"),
                    },
                    &callback,
                );
                return;
            }
        };

        // Brief health check — if the process dies within 500ms it never connected.
        let health_check = tokio::time::sleep(Duration::from_millis(500));
        tokio::pin!(health_check);

        let died_early = tokio::select! {
            result = child.wait() => Some(result),
            () = &mut health_check => None,
            _ = &mut shutdown_rx => {
                graceful_kill(&mut child).await;
                set_status(&status, TunnelStatus::Stopped { reason: "shutdown requested".to_string() }, &callback);
                return;
            }
        };

        if let Some(wait_result) = died_early {
            // Process died during health check.
            let stderr = read_stderr(&mut child).await;
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

        // Process survived 500ms — consider it connected.
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

        let stderr = read_stderr(&mut child).await;
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

/// Read whatever stderr the child has buffered.
async fn read_stderr(child: &mut tokio::process::Child) -> String {
    let Some(mut stderr) = child.stderr.take() else {
        return String::new();
    };
    let mut buf = String::new();
    // Read with a size limit to avoid unbounded allocation.
    let mut raw = vec![0u8; 8192];
    match stderr.read(&mut raw).await {
        Ok(n) => buf.push_str(&String::from_utf8_lossy(&raw[..n])),
        Err(e) => {
            tracing::warn!(source = "tunnel_supervisor", error = %e, "Failed to read ssh stderr");
        }
    }
    buf
}

/// Send SIGTERM, wait 5s, escalate to SIGKILL.
async fn graceful_kill(child: &mut tokio::process::Child) {
    #[cfg(unix)]
    if let Some(id) = child.id() {
        match i32::try_from(id) {
            Ok(pid) => {
                // SAFETY: `pid` is from `child.id()` which returns the OS PID of
                // a child process we spawned and have not yet waited on.
                // SIGTERM has no preconditions beyond a valid PID.
                unsafe {
                    libc::kill(pid, libc::SIGTERM);
                }
            }
            Err(_) => {
                tracing::warn!(
                    source = "tunnel_supervisor",
                    raw_pid = id,
                    "PID overflows i32, escalating to SIGKILL"
                );
                let _ = child.kill().await;
                return;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill().await;
        return;
    }

    // Wait up to 5s for clean exit after SIGTERM, then escalate.
    #[cfg(unix)]
    tokio::select! {
        _ = child.wait() => {}
        () = tokio::time::sleep(Duration::from_secs(5)) => {
            let _ = child.kill().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::SocketAddr;
    use std::sync::atomic::AtomicU32;
    use tempfile::NamedTempFile;
    use tokio::net::TcpListener;

    fn fake_ssh_script(behavior: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "#!/bin/sh").unwrap();
        writeln!(f, "{behavior}").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(f.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        f
    }

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

    #[tokio::test]
    async fn spawn_clean_exit() {
        let script = fake_ssh_script("sleep 0.2; exit 0");
        let (cb, statuses) = status_collector();

        let mut sup =
            TunnelSupervisor::start_with_binary(test_profile(), script.path().to_path_buf(), cb)
                .await;

        // Wait for the process to exit and supervisor to settle.
        tokio::time::sleep(Duration::from_secs(2)).await;

        let history = statuses.lock().clone();
        // Should see Starting, then either Connected or Stopped (process exits
        // quickly — if it exits before 500ms health check, it goes straight to
        // Stopped; if after, Connected then Stopped).
        assert!(!history.is_empty(), "should have status updates");

        // Final status should be Stopped with a non-error reason.
        let final_status = sup.status();
        match &final_status {
            TunnelStatus::Stopped { .. } => {} // expected
            other => panic!("expected Stopped, got {other:?}"),
        }

        sup.stop(); // idempotent
    }

    #[tokio::test]
    async fn auth_failure_no_retry() {
        let script = fake_ssh_script(r#"echo "Permission denied (publickey)." >&2; exit 255"#);
        let (cb, statuses) = status_collector();

        let mut sup =
            TunnelSupervisor::start_with_binary(test_profile(), script.path().to_path_buf(), cb)
                .await;

        tokio::time::sleep(Duration::from_secs(2)).await;

        let history = statuses.lock().clone();

        // Must NOT contain Reconnecting — auth failures are not retryable.
        let has_reconnecting = history
            .iter()
            .any(|s| matches!(s, TunnelStatus::Reconnecting { .. }));
        assert!(
            !has_reconnecting,
            "auth failure should not trigger reconnect, history: {history:?}"
        );

        // Final status should be Stopped with AuthFailed reason.
        let final_status = sup.status();
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
        let attempt_counter = Arc::new(AtomicU32::new(0));
        let counter = Arc::clone(&attempt_counter);

        let script = fake_ssh_script(
            r#"echo "ssh: connect to host example.com port 22: Connection refused" >&2; exit 255"#,
        );
        let (cb, statuses) = status_collector();

        let mut sup =
            TunnelSupervisor::start_with_binary(test_profile(), script.path().to_path_buf(), cb)
                .await;

        // Wait long enough for at least 2 retry attempts (first backoff ~1s, second ~2s).
        tokio::time::sleep(Duration::from_secs(5)).await;

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
        let _ = counter;
    }

    #[tokio::test]
    async fn graceful_shutdown() {
        // Script that sleeps forever.
        let script = fake_ssh_script("sleep 3600");
        let (cb, _statuses) = status_collector();

        let mut sup =
            TunnelSupervisor::start_with_binary(test_profile(), script.path().to_path_buf(), cb)
                .await;

        // Wait for health check to pass.
        tokio::time::sleep(Duration::from_millis(800)).await;

        // Should be Connected.
        assert_eq!(sup.status(), TunnelStatus::Connected);

        // Request shutdown.
        sup.stop();

        // Should terminate within 6s (SIGTERM + 5s grace).
        tokio::time::sleep(Duration::from_secs(6)).await;

        let final_status = sup.status();
        match &final_status {
            TunnelStatus::Stopped { reason } => {
                assert!(
                    reason.contains("shutdown"),
                    "reason should mention shutdown, got: {reason}"
                );
            }
            other => panic!("expected Stopped after shutdown, got {other:?}"),
        }
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

        let script = fake_ssh_script("exit 0");
        let (cb, _statuses) = status_collector();

        let sup =
            TunnelSupervisor::start_with_binary(profile, script.path().to_path_buf(), cb).await;

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
}
