use anyhow::{Result, bail};
use std::path::PathBuf;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::Mutex;

use crate::mdkb_client::{MdkbClient, MdkbPing};
use crate::plugin_exec::resolve_binary;

const DAEMON_SPAWN_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// A liveness probe runs on a much shorter leash than a query: it gates the
/// shared `MdkbDaemon` lock, and `wait_for_compatible_daemon` budgets its whole
/// poll loop at `DAEMON_SPAWN_TIMEOUT` — a probe allowed to run to the query
/// deadline would blow that budget on its first iteration.
const PING_TIMEOUT: Duration = Duration::from_secs(2);

/// Ask a candidate daemon whether it is alive and what it is.
///
/// Every caller drops the client when this fails, so a probe cut short by its
/// own deadline can never leave a half-read stream in the cache.
async fn probe(client: &mut MdkbClient) -> Result<MdkbPing> {
    match tokio::time::timeout(PING_TIMEOUT, client.ping_info()).await {
        Ok(ping) => ping,
        Err(_) => bail!("mdkb: ping timed out after {}s", PING_TIMEOUT.as_secs()),
    }
}

pub struct MdkbDaemon {
    client: Option<MdkbClient>,
    binary_path: Option<PathBuf>,
    cached_version: Option<String>,
}

impl MdkbDaemon {
    pub fn new() -> Self {
        let binary_path = resolve_binary("mdkb").map(PathBuf::from);
        let cached_version = binary_path.as_ref().and_then(|bin| {
            let output = std::process::Command::new(bin)
                .arg("--version")
                .output()
                .ok()?;
            let stdout = String::from_utf8_lossy(&output.stdout);
            let version = stdout.trim().strip_prefix("mdkb ").unwrap_or(stdout.trim());
            Some(version.to_string())
        });
        Self {
            client: None,
            binary_path,
            cached_version,
        }
    }

    pub fn is_available(&self) -> bool {
        self.binary_path.as_ref().is_some_and(|p| p.exists())
    }

    pub fn is_connected(&self) -> bool {
        self.client.is_some()
    }

    pub fn binary_path(&self) -> Option<&std::path::Path> {
        self.binary_path.as_deref()
    }

    pub fn version(&self) -> Option<String> {
        self.cached_version.clone()
    }

    /// Take a live, compatible client *out* of the cache.
    ///
    /// Owned, not `&mut` borrowed out of the shared lock's guard: the borrow is
    /// what used to keep the mutex alive for the whole RPC. The caller runs its
    /// query with the lock dropped and hands the client back with
    /// `release_client`.
    pub async fn acquire_client(&mut self) -> Result<MdkbClient> {
        let mut incompatible_daemon_found = false;

        if let Some(mut client) = self.client.take()
            && let Ok(ping) = probe(&mut client).await
        {
            if self.is_compatible(&ping) {
                return Ok(client);
            }
            incompatible_daemon_found = ping.pong;
        }

        if let Ok(mut client) = MdkbClient::connect().await
            && let Ok(ping) = probe(&mut client).await
        {
            if self.is_compatible(&ping) {
                return Ok(client);
            }
            incompatible_daemon_found |= ping.pong;
        }

        if incompatible_daemon_found {
            self.restart_daemon().await?;
        } else {
            self.spawn_daemon()?;
        }

        self.wait_for_compatible_daemon().await
    }

    /// Put a client back so the next query reuses the connection.
    ///
    /// Unconditional, including after a failed query. A client abandoned
    /// mid-exchange refuses every later call outright, so the next
    /// `acquire_client` probe discards it and reconnects — caching it costs one
    /// cheap, non-blocking probe, whereas dropping it here would turn every
    /// later query into a fresh connect.
    ///
    /// Concurrent callers each get their own connection — the second one finds
    /// an empty cache and dials again — so the last release wins and the other
    /// connection closes. Queries no longer queue behind each other, and the
    /// cache stays a one-slot reuse hint, not a pool.
    pub fn release_client(&mut self, client: MdkbClient) {
        self.client = Some(client);
    }

    pub async fn ensure_running(&mut self) -> Result<&mut MdkbClient> {
        let client = self.acquire_client().await?;
        Ok(self.client.insert(client))
    }

    fn is_compatible(&self, ping: &MdkbPing) -> bool {
        ping.pong
            && self
                .cached_version
                .as_deref()
                .is_none_or(|expected| ping.version.as_deref() == Some(expected))
    }

    /// Not `async`: `--detach` means we spawn and walk away, and
    /// `tokio::process::Command::spawn` is itself synchronous.
    fn spawn_daemon(&self) -> Result<()> {
        let bin = self
            .binary_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("mdkb binary not found in trusted directories"))?;

        Command::new(bin)
            .args(["serve", "--daemon", "--detach"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;

        Ok(())
    }

    async fn restart_daemon(&self) -> Result<()> {
        let bin = self
            .binary_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("mdkb binary not found in trusted directories"))?;

        let status = Command::new(bin)
            .args(["daemon", "restart"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await?;
        if !status.success() {
            bail!("mdkb daemon restart failed with status {status}");
        }
        Ok(())
    }

    async fn wait_for_compatible_daemon(&self) -> Result<MdkbClient> {
        let deadline = tokio::time::Instant::now() + DAEMON_SPAWN_TIMEOUT;

        while tokio::time::Instant::now() < deadline {
            if let Ok(mut c) = MdkbClient::connect().await
                && let Ok(ping) = probe(&mut c).await
                && self.is_compatible(&ping)
            {
                return Ok(c);
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }

        bail!(
            "mdkb daemon did not start within {}s",
            DAEMON_SPAWN_TIMEOUT.as_secs()
        );
    }
}

pub type SharedMdkbDaemon = Mutex<MdkbDaemon>;

pub fn create_shared_daemon() -> SharedMdkbDaemon {
    Mutex::new(MdkbDaemon::new())
}

/// Run one query against the shared daemon, holding the lock only either side
/// of it.
///
/// The whole point is the gap in the middle: the lock is dropped before `f`
/// runs, so a query stuck on a silent daemon cannot make unrelated callers wait
/// out its deadline. Daemon startup stays exclusive on purpose — two callers
/// must not race two spawns — so a caller can still wait on a cold spawn. What
/// it can no longer wait on is somebody else's RPC.
///
/// `f` takes the client and gives it back, whatever the query did, so the
/// connection returns to the cache on the failure path as well as the happy
/// one.
///
/// `what` names the command for the log line. An absent daemon is routine and
/// logs at debug; a daemon that answered badly is not, and logs at warn.
pub async fn with_client<T, F, Fut>(shared: &SharedMdkbDaemon, what: &str, f: F) -> Option<T>
where
    F: FnOnce(MdkbClient) -> Fut,
    Fut: std::future::Future<Output = (MdkbClient, Result<T>)>,
{
    // Scoped so the guard is gone before `f` runs. Do not flatten this into a
    // single statement: the point of the function is the lock NOT being held
    // across the await below.
    let client = {
        let mut daemon = shared.lock().await;
        match daemon.acquire_client().await {
            Ok(client) => client,
            Err(e) => {
                tracing::debug!("mdkb unavailable: {e}");
                return None;
            }
        }
    };

    let (client, result) = f(client).await;
    shared.lock().await.release_client(client);

    match result {
        Ok(value) => Some(value),
        Err(e) => {
            tracing::warn!("{what} failed: {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_without_binary_is_not_available() {
        // In test env, mdkb may or may not be installed
        let daemon = MdkbDaemon {
            client: None,
            binary_path: None,
            cached_version: None,
        };
        assert!(!daemon.is_available());
    }

    #[test]
    fn new_with_binary_is_available() {
        // Use a path guaranteed to exist
        let daemon = MdkbDaemon {
            client: None,
            binary_path: Some(PathBuf::from(env!("CARGO_MANIFEST_DIR"))),
            cached_version: None,
        };
        assert!(daemon.is_available());
    }

    #[test]
    fn stale_cached_path_not_available() {
        let daemon = MdkbDaemon {
            client: None,
            binary_path: Some(PathBuf::from("/nonexistent/mdkb")),
            cached_version: None,
        };
        assert!(!daemon.is_available());
    }

    #[tokio::test]
    async fn ensure_running_without_binary_uses_existing_daemon() {
        let mut daemon = MdkbDaemon {
            client: None,
            binary_path: None,
            cached_version: None,
        };
        let result = daemon.ensure_running().await;
        if MdkbClient::socket_path().is_some_and(|path| path.exists()) {
            assert!(result.is_ok(), "should connect to running daemon");
        } else {
            assert!(result.unwrap_err().to_string().contains("not found"));
        }
    }

    #[tokio::test]
    async fn spawn_daemon_fails_when_no_binary() {
        let daemon = MdkbDaemon {
            client: None,
            binary_path: None,
            cached_version: None,
        };
        let err = daemon.spawn_daemon().unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn installed_version_rejects_old_or_unidentified_daemon() {
        let daemon = MdkbDaemon {
            client: None,
            binary_path: Some(PathBuf::from("/tmp/mdkb")),
            cached_version: Some("3.7.11".to_string()),
        };

        assert!(daemon.is_compatible(&MdkbPing {
            pong: true,
            version: Some("3.7.11".to_string()),
        }));
        assert!(!daemon.is_compatible(&MdkbPing {
            pong: true,
            version: Some("3.7.10".to_string()),
        }));
        assert!(!daemon.is_compatible(&MdkbPing {
            pong: true,
            version: None,
        }));
    }

    /// A daemon that passes the liveness probe and then stalls on the query.
    /// This is the shape the story is about: alive enough to be cached, silent
    /// once it matters. `stall` never answers; anything else is an RPC error.
    #[cfg(unix)]
    async fn spawn_probeable_server() -> (PathBuf, tokio::task::JoinHandle<()>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let dir = crate::test_support::short_socket_tempdir();
        let sock_path = dir.path().join("probeable.sock");
        let listener = tokio::net::UnixListener::bind(&sock_path).unwrap();
        let path = sock_path.clone();

        let handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            loop {
                let mut hdr = [0u8; 4];
                if stream.read_exact(&mut hdr).await.is_err() {
                    break;
                }
                let mut body = vec![0u8; u32::from_le_bytes(hdr) as usize];
                if stream.read_exact(&mut body).await.is_err() {
                    break;
                }
                let req: serde_json::Value = serde_json::from_slice(&body).unwrap();
                let id = req.get("id").cloned().unwrap_or(serde_json::Value::Null);
                let response = match req.get("method").and_then(serde_json::Value::as_str) {
                    Some("ping") => serde_json::json!({
                        "jsonrpc": "2.0", "id": id,
                        "result": {"pong": true, "version": "9.8.7"}
                    }),
                    // Read the request, answer nothing, ever.
                    Some("stall") => {
                        std::future::pending::<()>().await;
                        unreachable!()
                    }
                    other => serde_json::json!({
                        "jsonrpc": "2.0", "id": id,
                        "error": {"code": -32601, "message": format!("unknown tool: {other:?}")}
                    }),
                };
                let bytes = serde_json::to_vec(&response).unwrap();
                stream
                    .write_all(&(bytes.len() as u32).to_le_bytes())
                    .await
                    .unwrap();
                stream.write_all(&bytes).await.unwrap();
            }
            drop(dir);
        });

        tokio::time::sleep(Duration::from_millis(10)).await;
        (path, handle)
    }

    /// A shared daemon with no local binary — so nothing can be spawned — whose
    /// cache already holds a client pointed at `path`.
    #[cfg(unix)]
    async fn shared_daemon_on(path: &std::path::Path, deadline: Duration) -> SharedMdkbDaemon {
        let stream = tokio::net::UnixStream::connect(path).await.unwrap();
        Mutex::new(MdkbDaemon {
            client: Some(MdkbClient::from_stream(stream, deadline)),
            binary_path: None,
            cached_version: None,
        })
    }

    /// A caller stuck in an RPC must not make unrelated callers wait out its
    /// deadline.
    ///
    /// Note the boundary: this is NOT "nothing ever serialises". Daemon startup
    /// is legitimately exclusive — two callers must not race two spawns — so a
    /// second caller can still wait on `wait_for_compatible_daemon`. A cold
    /// spawn wait is not a regression against this test.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_stuck_query_does_not_hold_the_shared_lock() {
        const QUERY_DEADLINE: Duration = Duration::from_secs(2);

        let (path, _server) = spawn_probeable_server().await;
        let shared = shared_daemon_on(&path, QUERY_DEADLINE).await;

        let stuck = with_client(&shared, "stuck", |mut client| async move {
            let result = client.call("stall", serde_json::json!({})).await;
            (client, result)
        });

        let unrelated = async {
            // Long enough for the stuck caller to clear `acquire_client` and be
            // sitting in its RPC, short enough to stay well inside its deadline.
            tokio::time::sleep(Duration::from_millis(150)).await;
            let started = tokio::time::Instant::now();
            let _guard = shared.lock().await;
            let waited = started.elapsed();
            assert!(
                waited < Duration::from_millis(300),
                "unrelated caller waited {waited:?} on someone else's RPC"
            );
        };

        let (stuck_result, ()) = tokio::join!(stuck, unrelated);
        assert!(stuck_result.is_none(), "the stuck query must give up");
    }

    /// A query that fails must still hand the connection back, or every later
    /// query silently degrades into a reconnect — one regression traded for
    /// another.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_failed_query_hands_the_connection_back() {
        let (path, _server) = spawn_probeable_server().await;
        let shared = shared_daemon_on(&path, Duration::from_secs(5)).await;

        let out = with_client(&shared, "failing", |mut client| async move {
            let result = client.call("no_such_method", serde_json::json!({})).await;
            (client, result)
        })
        .await;

        assert!(out.is_none(), "an RPC error must not read as a result");
        assert!(
            shared.lock().await.is_connected(),
            "the client must go back in the cache after a failed query"
        );
    }

    /// A silent daemon must not pin the shared lock for a query deadline.
    /// The client is handed a deliberately long query deadline: only the
    /// probe's own leash can make this return.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_liveness_probe_gives_up_long_before_a_query_would() {
        let (path, _server) = crate::mdkb_client::tests::spawn_silent_server().await;
        let stream = tokio::net::UnixStream::connect(&path).await.unwrap();
        let mut client = MdkbClient::from_stream(stream, Duration::from_secs(30));

        let outcome = tokio::time::timeout(PING_TIMEOUT * 2, probe(&mut client)).await;

        let err = outcome
            .expect("a probe gates the shared daemon lock; it must never run to a query deadline")
            .unwrap_err();
        assert!(err.to_string().contains("timed out"), "err: {err}");
    }

    #[test]
    fn daemon_without_local_binary_accepts_any_live_version() {
        let daemon = MdkbDaemon {
            client: None,
            binary_path: None,
            cached_version: None,
        };
        assert!(daemon.is_compatible(&MdkbPing {
            pong: true,
            version: None,
        }));
    }
}
