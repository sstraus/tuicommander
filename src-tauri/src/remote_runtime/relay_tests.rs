//! A desktop reaching a headless daemon through a real relay process.
//!
//! The pieces are the production ones: the relay binary, `daemon::run` serving
//! the remote router of an in-process `AppState`, and `connect()` of the remote
//! runtime on the desktop side. Only the `AppState`s are test fixtures.
//!
//! The relay cannot be a dev-dependency (its `rusqlite` links a different
//! `libsqlite3-sys` than this crate's), so the test builds and spawns the binary.
//! `TUIC_RELAY_BIN` skips the build.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use tokio::process::{Child, Command};

use super::*;
use crate::relay_tunnel::bundle::PairingBundle;
use crate::relay_tunnel::daemon::{DaemonAgent, run as run_daemon};
use crate::relay_tunnel::mux::MuxConfig;
use crate::relay_tunnel::service::register_bearer;

const DAEMON_TOKEN: &str = "daemon-api-token";
const WAIT: Duration = Duration::from_secs(60);

fn relay_binary() -> PathBuf {
    if let Some(path) = std::env::var_os("TUIC_RELAY_BIN") {
        return PathBuf::from(path);
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tools/relay/Cargo.toml");
    let output = std::process::Command::new(std::env::var("CARGO").unwrap_or("cargo".into()))
        .args(["build", "--message-format=json-render-diagnostics"])
        .arg("--manifest-path")
        .arg(manifest)
        .stderr(Stdio::inherit())
        .output()
        .expect("run cargo to build the relay");
    assert!(output.status.success(), "building the relay failed");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|message| message["reason"] == "compiler-artifact")
        .filter(|message| message["target"]["name"] == "tuic-relay")
        .find_map(|message| message["executable"].as_str().map(PathBuf::from))
        .expect("the relay build reports its executable")
}

/// A relay process on a fixed loopback port, so a restart keeps the address.
struct Relay {
    child: Option<Child>,
    port: u16,
    db: tempfile::TempDir,
}

impl Relay {
    async fn start() -> Self {
        let port = {
            let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            probe.local_addr().unwrap().port()
        };
        let mut relay = Self {
            child: None,
            port,
            db: tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap(),
        };
        relay.spawn().await;
        relay
    }

    async fn spawn(&mut self) {
        let child = Command::new(relay_binary())
            .args(["--bind", &format!("127.0.0.1:{}", self.port)])
            .arg("--db-path")
            .arg(self.db.path().join("relay.db"))
            .args(["--rate-limit-per-hour", "0"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .expect("spawn the relay");
        self.child = Some(child);
        let health = format!("http://127.0.0.1:{}/health", self.port);
        let up = async {
            while reqwest::get(&health).await.is_err() {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        };
        tokio::time::timeout(WAIT, up).await.expect("relay answers");
    }

    fn url(&self) -> String {
        format!("ws://127.0.0.1:{}", self.port)
    }

    async fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            child.kill().await.unwrap();
        }
    }
}

/// The headless daemon: the remote router on a loopback port, and the tunnel
/// agent that serves it. Restarting the agent leaves the router and its PTYs.
struct Daemon {
    state: Arc<AppState>,
    agent: Arc<DaemonAgent>,
    run: Option<tokio::task::JoinHandle<()>>,
    _http: tokio::task::JoinHandle<()>,
}

impl Daemon {
    async fn start(bundle: &PairingBundle) -> Self {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        *state.session_token.write() = DAEMON_TOKEN.to_string();
        state.config.write().services.auth.lan_auth_bypass = false;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream: SocketAddr = listener.local_addr().unwrap();
        let app = crate::mcp_http::build_remote_router(state.clone());
        let http = tokio::spawn(async move {
            let _ = axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await;
        });
        let agent = Arc::new(DaemonAgent {
            bundle: bundle.clone(),
            upstream,
            daemon_token: zeroize::Zeroizing::new(DAEMON_TOKEN.to_string()),
            mux: MuxConfig::default(),
        });
        let mut daemon = Self {
            state,
            agent,
            run: None,
            _http: http,
        };
        daemon.attach();
        daemon
    }

    fn attach(&mut self) {
        let agent = self.agent.clone();
        self.run = Some(tokio::spawn(async move {
            run_daemon(agent, std::future::pending()).await
        }));
    }

    fn detach(&mut self) {
        if let Some(run) = self.run.take() {
            run.abort();
        }
    }

    fn pty_count(&self) -> usize {
        self.state.session_maps.sessions.len()
    }
}

fn desktop(bundle: &PairingBundle) -> (Arc<AppState>, String) {
    // The daemon runs in this process, so the desktop must not take it for
    // itself; a real daemon is another process with its own identity.
    let mut state = crate::state::tests_support::make_test_app_state();
    state.remote.set_identity_for_test("desktop-under-test");
    let state = Arc::new(state);
    let connection = RemoteConnection::new_relay("through the relay");
    let id = connection.id.clone();
    RemoteConnectionStore::save(&state.data_dir, &[connection]).unwrap();
    crate::remote_connection::set_relay_bundle(&id, &bundle.to_text()).unwrap();
    (state, id)
}

async fn bundle_for(relay: &Relay) -> PairingBundle {
    let bearer = register_bearer(&relay.url()).await.unwrap();
    PairingBundle::generate(&relay.url(), bearer).unwrap()
}

async fn until<F: Fn() -> bool>(what: &str, within: Duration, condition: F) {
    let poll = async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    };
    tokio::time::timeout(within, poll)
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for {what}"));
}

fn snap(state: &Arc<AppState>, id: &str) -> RemoteConnectionStatus {
    state
        .remote
        .entries
        .get(id)
        .map(|entry| entry.snapshot(id))
        .expect("the connection has an entry")
}

fn authed(state: &Arc<AppState>, id: &str) -> (reqwest::Client, String, String) {
    let status = state.remote.status_of(id);
    assert_eq!(status, RemoteStatus::Connected);
    let base = state.remote.base_url(id).expect("connected has a base url");
    let token = state.remote.probe_token(id).expect("daemon token");
    (state.remote.http_client(), base, token)
}

async fn json_get(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    path: &str,
) -> serde_json::Value {
    client
        .get(format!("{base}{path}"))
        .header("cookie", format!("tui-session={token}"))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap()
}

// Catches: an attach that skips the end-to-end check (a wrong pairing secret
// reaching the daemon's API), a typed state that is not surfaced, and a relay
// path whose handler contracts differ from a direct connection (repos, sessions,
// a PTY and its output).
#[tokio::test]
async fn desktop_attaches_through_the_relay_and_runs_a_command() {
    let relay = Relay::start().await;
    let bundle = bundle_for(&relay).await;
    let daemon = Daemon::start(&bundle).await;

    // Same room and relay bearer, different pairing secret: the daemon must
    // refuse before any command can run.
    let mut impostor = bundle.clone();
    impostor.secret = crate::relay_tunnel::secure::PairingSecret::generate().unwrap();
    let (impostor_state, impostor_id) = desktop(&impostor);
    connect(&impostor_state, &impostor_id).await.unwrap_err();
    let rejected = snap(&impostor_state, &impostor_id);
    assert_eq!(rejected.status, RemoteStatus::Unauthenticated);
    assert_eq!(rejected.relay_failure, Some("pairing_rejected"));
    assert_eq!(daemon.pty_count(), 0);
    teardown(&impostor_state, &impostor_id);

    let (state, id) = desktop(&bundle);
    connect(&state, &id).await.expect("attach");
    let (client, base, token) = authed(&state, &id);
    assert!(base.starts_with("http://127.0.0.1:"));
    assert_eq!(
        json_get(&client, &base, &token, "/sessions").await,
        serde_json::json!([])
    );
    // A repository handler answers through the relay exactly as it does on a
    // direct connection: the daemon reads a repo on its own disk.
    let repo = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let init = std::process::Command::new("git")
        .args(["init", "-q", "-b", "trunk"])
        .arg(repo.path())
        .status()
        .unwrap();
    assert!(init.success());
    let branches = client
        .get(format!("{base}/repo/branches"))
        .query(&[("path", repo.path().to_str().unwrap())])
        .header("cookie", format!("tui-session={token}"))
        .send()
        .await
        .unwrap();
    assert!(branches.status().is_success(), "{}", branches.status());

    let created: serde_json::Value = client
        .post(format!("{base}/sessions"))
        .header("cookie", format!("tui-session={token}"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let session = created["session_id"]
        .as_str()
        .expect("session id")
        .to_owned();
    client
        .post(format!("{base}/sessions/{session}/write"))
        .header("cookie", format!("tui-session={token}"))
        .json(&serde_json::json!({ "data": "echo $((6*7))relay\n" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    // The typed-in line contains `$((6*7))`, never `42relay`: only the shell's
    // answer does.
    let seen = async {
        loop {
            let output = client
                .get(format!("{base}/sessions/{session}/output"))
                .header("cookie", format!("tui-session={token}"))
                .send()
                .await
                .unwrap()
                .text()
                .await
                .unwrap();
            if output.contains("42relay") {
                return;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    };
    tokio::time::timeout(WAIT, seen)
        .await
        .expect("command output");
    teardown(&state, &id);
}

// Catches: a lost relay that reads as a generic error, a reconnect that
// replays the session creation (a second PTY), and a daemon restart that the
// desktop never recovers from.
#[tokio::test]
async fn relay_loss_and_daemon_restart_recover_without_duplicate_sessions() {
    let mut relay = Relay::start().await;
    let bundle = bundle_for(&relay).await;
    let mut daemon = Daemon::start(&bundle).await;
    let (state, id) = desktop(&bundle);
    connect(&state, &id).await.expect("attach");
    let (client, base, token) = authed(&state, &id);
    client
        .post(format!("{base}/sessions"))
        .header("cookie", format!("tui-session={token}"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    assert_eq!(daemon.pty_count(), 1);

    // Observe the transition, not a later mutable snapshot: a supervisor retry
    // can legitimately replace link_lost with relay_unreachable before a poll.
    let mut events = state.event_bus.subscribe();
    relay.stop().await;
    let loss = tokio::time::timeout(WAIT, async {
        loop {
            if let AppEvent::RemoteConnectionStatusChanged { payload } =
                events.recv().await.expect("remote status event")
                && payload["id"] == id
                && payload["status"] == "error"
                && payload["relay_failure"].is_string()
            {
                break payload;
            }
        }
    })
    .await
    .expect("typed link loss");
    assert_eq!(loss["relay_failure"], "link_lost", "{loss}");

    relay.spawn().await;
    until("reconnect after relay loss", WAIT, || {
        state.remote.status_of(&id) == RemoteStatus::Connected
    })
    .await;
    assert_eq!(daemon.pty_count(), 1);

    daemon.detach();
    until("typed loss after daemon stop", WAIT, || {
        state.remote.status_of(&id) == RemoteStatus::Error
    })
    .await;
    daemon.attach();
    until("reconnect after daemon restart", WAIT, || {
        state.remote.status_of(&id) == RemoteStatus::Connected
    })
    .await;
    let (client, base, token) = authed(&state, &id);
    let listed = json_get(&client, &base, &token, "/sessions").await;
    assert_eq!(listed.as_array().map(Vec::len), Some(1));
    assert_eq!(daemon.pty_count(), 1);
    teardown(&state, &id);
}
