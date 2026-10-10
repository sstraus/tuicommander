use std::collections::{BTreeSet, HashMap};
use std::io::BufReader;
use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde::Serialize;

use super::discovery::{self, ConfigHost, DiscoveredHost, DiscoveredHosts};
use super::profile::TunnelProfile;
use super::storage::ProfileStore;
use crate::AppState;

/// JSON error helper.
fn err_json(status: StatusCode, msg: &str) -> Response {
    (status, Json(serde_json::json!({"error": msg}))).into_response()
}

// ── Profile CRUD ────────────────────────────────────────────

/// GET /tunnels/profiles — list all saved profiles.
pub(crate) async fn list_tunnel_profiles(State(state): State<Arc<AppState>>) -> Response {
    match ProfileStore::load_all(&state.data_dir, None) {
        Ok(profiles) => (StatusCode::OK, Json(serde_json::json!(profiles))).into_response(),
        Err(e) => err_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

/// POST /tunnels/profiles — create or update a profile.
pub(crate) async fn save_tunnel_profile(
    State(state): State<Arc<AppState>>,
    Json(mut profile): Json<TunnelProfile>,
) -> Response {
    if let Err(e) = profile.validate() {
        return err_json(StatusCode::BAD_REQUEST, &e);
    }
    match ProfileStore::save(&state.data_dir, &profile) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"id": profile.id}))).into_response(),
        Err(e) => err_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

/// DELETE /tunnels/profiles/:id — delete a profile, stopping its tunnel if active.
pub(crate) async fn delete_tunnel_profile(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Response {
    state.tunnel_manager.stop_if_running(&id);

    match ProfileStore::delete(&state.data_dir, None, &id) {
        Ok(true) => (StatusCode::OK, Json(serde_json::json!({"deleted": true}))).into_response(),
        Ok(false) => err_json(StatusCode::NOT_FOUND, "profile not found"),
        Err(e) => err_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

// ── Tunnel lifecycle ────────────────────────────────────────

/// POST /tunnels/start/:id — load profile from storage and start its tunnel.
pub(crate) async fn start_tunnel(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Response {
    let profiles = match ProfileStore::load_all(&state.data_dir, None) {
        Ok(p) => p,
        Err(e) => return err_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    let profile = match profiles.into_iter().find(|p| p.id == id) {
        Some(p) => p,
        None => return err_json(StatusCode::NOT_FOUND, "profile not found"),
    };

    let result = state.tunnel_manager.start(profile).await;

    match result {
        Ok(tunnel_id) => {
            (StatusCode::OK, Json(serde_json::json!({"id": tunnel_id}))).into_response()
        }
        Err(e) => err_json(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

/// POST /tunnels/stop/:id — stop an active tunnel.
pub(crate) async fn stop_tunnel(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Response {
    match state.tunnel_manager.stop(&id) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"stopped": true}))).into_response(),
        Err(e) => err_json(StatusCode::NOT_FOUND, &e),
    }
}

// ── Status queries ──────────────────────────────────────────

/// GET /tunnels/active — list all running tunnels with status.
pub(crate) async fn list_active_tunnels(State(state): State<Arc<AppState>>) -> Response {
    let list = state.tunnel_manager.list();
    let entries: Vec<serde_json::Value> = list
        .into_iter()
        .map(|(id, status)| serde_json::json!({"id": id, "status": status}))
        .collect();
    (StatusCode::OK, Json(serde_json::json!(entries))).into_response()
}

/// GET /tunnels/status/:id — single tunnel status.
pub(crate) async fn get_tunnel_status(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Response {
    match state.tunnel_manager.get_status(&id) {
        Some(status) => (
            StatusCode::OK,
            Json(serde_json::json!({"id": id, "status": status})),
        )
            .into_response(),
        None => err_json(StatusCode::NOT_FOUND, "tunnel not found"),
    }
}

// ── Audit log ───────────────────────────────────────────────

#[derive(Deserialize)]
pub(crate) struct AuditQuery {
    limit: Option<usize>,
}

/// GET /tunnels/audit/:id — audit log for a tunnel.
pub(crate) async fn get_tunnel_audit(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<AuditQuery>,
) -> Response {
    let limit = query.limit.unwrap_or(20);
    match state.tunnel_audit.lock().query_by_tunnel(&id, limit) {
        Ok(events) => (StatusCode::OK, Json(serde_json::json!(events))).into_response(),
        Err(e) => err_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

// ── SSH config hosts ────────────────────────────────────────

const SSH_PROBE_CACHE_TTL: Duration = Duration::from_secs(60);
const SSH_PROBE_TIMEOUT: Duration = Duration::from_secs(7);
const SSH_PROBE_CONCURRENCY: usize = 4;
/// Most hosts one bulk probe contacts; the rest of a long config stay unprobed.
const SSH_PROBE_MAX_HOSTS: usize = 64;
/// known_hosts is read up to this size; a longer file is cut at its last full line.
const KNOWN_HOSTS_MAX_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HostAuth {
    Shell,
    NoShell,
    AuthFailed,
    Unreachable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SshHostStatus {
    pub(crate) host: String,
    /// With `port`, the identity of the discovered entry this result belongs to.
    pub(crate) target: String,
    pub(crate) port: Option<u16>,
    pub(crate) auth: HostAuth,
}

struct ProbeCacheEntry {
    stored_at: Instant,
    hosts: Vec<DiscoveredHost>,
    statuses: Vec<SshHostStatus>,
}

static SSH_PROBE_CACHE: std::sync::OnceLock<tokio::sync::Mutex<Option<ProbeCacheEntry>>> =
    std::sync::OnceLock::new();

/// What every ssh probe, bulk or single, goes through: one permit pool bounds
/// the processes running at once, and one slot per host lets concurrent
/// probes of that host share a single process.
struct ProbeGate {
    permits: tokio::sync::Semaphore,
    hosts: std::sync::Mutex<HashMap<(String, u16), HostSlot>>,
}

type HostSlot = Arc<tokio::sync::Mutex<Option<(Instant, HostAuth)>>>;

impl ProbeGate {
    fn new(max_running: usize) -> Self {
        Self {
            permits: tokio::sync::Semaphore::new(max_running),
            hosts: Default::default(),
        }
    }

    fn slot(&self, identity: (String, u16)) -> HostSlot {
        self.hosts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .entry(identity)
            .or_default()
            .clone()
    }
}

static SSH_PROBE_GATE: std::sync::LazyLock<ProbeGate> =
    std::sync::LazyLock::new(|| ProbeGate::new(SSH_PROBE_CONCURRENCY));

fn ssh_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".ssh").join("config"))
}

pub(crate) fn load_ssh_config_hosts() -> Result<Vec<String>, String> {
    let Some(path) = ssh_config_path() else {
        return Ok(Vec::new());
    };
    parse_ssh_config_hosts(&path)
}

fn parse_ssh_config_hosts(path: &FsPath) -> Result<Vec<String>, String> {
    Ok(parse_ssh_config_entries(path)?
        .into_iter()
        .map(|entry| entry.alias)
        .collect())
}

fn parse_ssh_config_entries(path: &FsPath) -> Result<Vec<ConfigHost>, String> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("failed to read SSH config: {error}")),
    };
    let mut reader = BufReader::new(file);
    let config = ssh2_config::SshConfig::default()
        .parse(&mut reader, ssh2_config::ParseRule::ALLOW_UNKNOWN_FIELDS)
        .map_err(|error| format!("failed to parse SSH config: {error}"))?;
    let aliases: BTreeSet<&str> = config
        .get_hosts()
        .iter()
        .flat_map(|host| &host.pattern)
        .filter(|clause| !clause.negated && !discovery::is_wildcard(&clause.pattern))
        .map(|clause| clause.pattern.as_str())
        .collect();
    Ok(aliases
        .into_iter()
        .map(|alias| {
            let params = config.query(alias);
            ConfigHost {
                alias: alias.to_string(),
                hostname: params.host_name,
                user: params.user,
                port: params.port,
            }
        })
        .collect())
}

fn known_hosts_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".ssh").join("known_hosts"))
}

fn read_known_hosts(path: &FsPath) -> Result<discovery::KnownHosts, String> {
    use std::io::Read;
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Default::default()),
        Err(error) => return Err(format!("failed to read known_hosts: {error}")),
    };
    let mut bytes = Vec::new();
    file.take(KNOWN_HOSTS_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("failed to read known_hosts: {error}"))?;
    Ok(discovery::parse_known_hosts(&bounded_text(bytes)))
}

/// Lossy text of at most `KNOWN_HOSTS_MAX_BYTES`, cut at the last full line.
/// Empty when the cut has no newline: the only line is incomplete.
fn bounded_text(mut bytes: Vec<u8>) -> String {
    if bytes.len() as u64 > KNOWN_HOSTS_MAX_BYTES {
        bytes.truncate(KNOWN_HOSTS_MAX_BYTES as usize);
        match bytes.iter().rposition(|b| *b == b'\n') {
            Some(last_newline) => bytes.truncate(last_newline + 1),
            None => bytes.clear(),
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

pub(crate) fn load_discovered_hosts() -> Result<DiscoveredHosts, String> {
    let config = match ssh_config_path() {
        Some(path) => parse_ssh_config_entries(&path)?,
        None => Vec::new(),
    };
    let known = match known_hosts_path() {
        Some(path) => read_known_hosts(&path)?,
        None => Default::default(),
    };
    Ok(discovery::merge_discovered(config, known))
}

/// GET /tunnels/ssh-hosts/discovered — config aliases plus known_hosts names.
pub(crate) async fn list_discovered_ssh_hosts_http() -> Response {
    match load_discovered_hosts() {
        Ok(discovered) => (StatusCode::OK, Json(serde_json::json!(discovered))).into_response(),
        Err(error) => err_json(StatusCode::INTERNAL_SERVER_ERROR, &error),
    }
}

/// GET /tunnels/ssh-hosts — parse ~/.ssh/config and return host aliases.
pub(crate) async fn list_ssh_config_hosts() -> Response {
    match load_ssh_config_hosts() {
        Ok(hosts) => (StatusCode::OK, Json(serde_json::json!(hosts))).into_response(),
        Err(error) => err_json(StatusCode::INTERNAL_SERVER_ERROR, &error),
    }
}

pub(crate) async fn probe_ssh_config_hosts_http() -> Response {
    match probe_ssh_config_hosts().await {
        Ok(statuses) => (StatusCode::OK, Json(serde_json::json!(statuses))).into_response(),
        Err(error) => err_json(StatusCode::INTERNAL_SERVER_ERROR, &error),
    }
}

/// Bulk probe: the `~/.ssh/config` aliases only. known_hosts names are probed
/// one at a time, on request (`probe_discovered_host`).
pub(crate) async fn probe_ssh_config_hosts() -> Result<Vec<SshHostStatus>, String> {
    let config = match ssh_config_path() {
        Some(path) => parse_ssh_config_entries(&path)?,
        None => Vec::new(),
    };
    let mut hosts = discovery::merge_discovered(config, Default::default()).hosts;
    hosts.truncate(SSH_PROBE_MAX_HOSTS);
    let cache = SSH_PROBE_CACHE.get_or_init(|| tokio::sync::Mutex::new(None));
    probe_cached(cache, hosts, FsPath::new("ssh"), SSH_PROBE_TIMEOUT).await
}

#[derive(Deserialize)]
pub(crate) struct ProbeHostRequest {
    pub(crate) target: String,
    pub(crate) port: Option<u16>,
}

/// Probe one discovered entry, identified by (target, port). Only entries the
/// discovery list itself yields are contacted, never a caller-supplied name.
/// A manual probe always asks ssh again; it only joins a probe of the same
/// host that is already running.
pub(crate) async fn probe_discovered_host(
    target: &str,
    port: Option<u16>,
) -> Result<SshHostStatus, String> {
    let discovered = load_discovered_hosts()?;
    probe_listed_host(
        &SSH_PROBE_GATE,
        discovered.hosts,
        target,
        port,
        FsPath::new("ssh"),
        SSH_PROBE_TIMEOUT,
    )
    .await
}

async fn probe_listed_host(
    gate: &ProbeGate,
    listed: Vec<DiscoveredHost>,
    target: &str,
    port: Option<u16>,
    binary: &FsPath,
    timeout: Duration,
) -> Result<SshHostStatus, String> {
    let wanted = (target.to_ascii_lowercase(), port.unwrap_or(22));
    let host = listed
        .into_iter()
        .find(|host| host.identity() == wanted)
        .ok_or_else(|| "host is not in the discovered list".to_string())?;
    let auth = probe_host_gated(gate, &host, binary, timeout).await;
    Ok(status_of(host, auth))
}

/// POST /tunnels/ssh-hosts/probe — probe one discovered host.
pub(crate) async fn probe_discovered_host_http(Json(request): Json<ProbeHostRequest>) -> Response {
    match probe_discovered_host(&request.target, request.port).await {
        Ok(status) => (StatusCode::OK, Json(serde_json::json!(status))).into_response(),
        Err(error) => err_json(StatusCode::NOT_FOUND, &error),
    }
}

fn status_of(host: DiscoveredHost, auth: HostAuth) -> SshHostStatus {
    SshHostStatus {
        host: host.host,
        target: host.target,
        port: host.port,
        auth,
    }
}

/// The cache lock is held only to read and to store, never across `ssh`.
async fn probe_cached(
    cache: &tokio::sync::Mutex<Option<ProbeCacheEntry>>,
    hosts: Vec<DiscoveredHost>,
    binary: &FsPath,
    timeout: Duration,
) -> Result<Vec<SshHostStatus>, String> {
    probe_cached_with(&SSH_PROBE_GATE, cache, hosts, binary, timeout).await
}

async fn probe_cached_with(
    gate: &ProbeGate,
    cache: &tokio::sync::Mutex<Option<ProbeCacheEntry>>,
    hosts: Vec<DiscoveredHost>,
    binary: &FsPath,
    timeout: Duration,
) -> Result<Vec<SshHostStatus>, String> {
    if let Some(entry) = cache.lock().await.as_ref()
        && entry.hosts == hosts
        && entry.stored_at.elapsed() < SSH_PROBE_CACHE_TTL
    {
        return Ok(entry.statuses.clone());
    }
    let statuses = probe_hosts_with_gate(gate, hosts.clone(), binary, timeout).await;
    *cache.lock().await = Some(ProbeCacheEntry {
        stored_at: Instant::now(),
        hosts,
        statuses: statuses.clone(),
    });
    Ok(statuses)
}

async fn probe_hosts_with_gate(
    gate: &ProbeGate,
    hosts: Vec<DiscoveredHost>,
    binary: &FsPath,
    timeout: Duration,
) -> Vec<SshHostStatus> {
    use futures_util::StreamExt;
    futures_util::stream::iter(hosts.into_iter().map(|host| async move {
        let auth = probe_host_gated(gate, &host, binary, timeout).await;
        status_of(host, auth)
    }))
    .buffer_unordered(SSH_PROBE_CONCURRENCY)
    .collect()
    .await
}

/// `host_key_policy` is the `StrictHostKeyChecking` value. The host follows
/// `--` so a name that starts with `-` can never be read as an option.
fn probe_args(host: &str, port: Option<u16>, host_key_policy: &str) -> Vec<String> {
    let mut args = vec![
        "-o".into(),
        "BatchMode=yes".into(),
        "-o".into(),
        "ConnectTimeout=5".into(),
        "-o".into(),
        format!("StrictHostKeyChecking={host_key_policy}"),
    ];
    if let Some(port) = port {
        args.push("-p".into());
        args.push(port.to_string());
    }
    args.push("--".into());
    args.push(host.to_string());
    args.push("true".into());
    args
}

/// Probes that overlap on one host share the result of the first; a probe
/// that starts after it finished runs ssh again.
async fn probe_host_gated(
    gate: &ProbeGate,
    host: &DiscoveredHost,
    binary: &FsPath,
    timeout: Duration,
) -> HostAuth {
    let requested = Instant::now();
    let slot = gate.slot(host.identity());
    let mut last = slot.lock().await;
    if let Some((finished, auth)) = *last
        && finished >= requested
    {
        return auth;
    }
    let auth = run_probe(gate, host, binary, timeout).await;
    *last = Some((Instant::now(), auth));
    auth
}

/// A known_hosts entry is already trusted, so a changed key must fail rather
/// than be accepted; a config alias keeps the original `accept-new`.
async fn run_probe(
    gate: &ProbeGate,
    host: &DiscoveredHost,
    binary: &FsPath,
    timeout: Duration,
) -> HostAuth {
    let policy = match host.source {
        discovery::HostSource::KnownHosts => "yes",
        discovery::HostSource::Config => "accept-new",
    };
    let Ok(_permit) = gate.permits.acquire().await else {
        return HostAuth::Unreachable;
    };
    let mut command = tokio::process::Command::new(binary);
    command
        .args(probe_args(&host.host, host.probe_port(), policy))
        .kill_on_drop(true)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let output = match tokio::time::timeout(timeout, command.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(_)) | Err(_) => return HostAuth::Unreachable,
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    classify_probe(output.status.success(), &stdout, &stderr)
}

fn classify_probe(success: bool, stdout: &str, stderr: &str) -> HostAuth {
    if success {
        return HostAuth::Shell;
    }
    let message = format!("{stdout}\n{stderr}").to_ascii_lowercase();
    if message.contains("does not provide shell access")
        || message.contains("shell access is disabled")
    {
        HostAuth::NoShell
    } else if matches!(
        super::classifier::classify_exit(stderr, Some(255)),
        super::classifier::ExitReason::AuthFailed
    ) {
        HostAuth::AuthFailed
    } else {
        HostAuth::Unreachable
    }
}

// ── SSH agent keys ──────────────────────────────────────────

/// GET /tunnels/agent-keys — list loaded SSH agent key fingerprints.
pub(crate) async fn list_agent_keys() -> Response {
    let output = match tokio::process::Command::new("ssh-add")
        .arg("-l")
        .output()
        .await
    {
        Ok(o) => o,
        Err(e) => {
            return err_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("failed to run ssh-add: {e}"),
            );
        }
    };

    // Exit code 1 means "no identities" — return empty list, not an error.
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("no identities") || output.status.code() == Some(1) {
            return (StatusCode::OK, Json(serde_json::json!([]))).into_response();
        }
        return err_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("ssh-add failed: {}", stderr.trim()),
        );
    }

    // Each line: "256 SHA256:xxxxx user@host (ED25519)"
    let stdout = String::from_utf8_lossy(&output.stdout);
    let keys: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            let parts: Vec<&str> = line.splitn(4, ' ').collect();
            if parts.len() >= 3 {
                serde_json::json!({
                    "bits": parts[0],
                    "fingerprint": parts[1],
                    "comment": parts.get(2).unwrap_or(&""),
                    "type": parts.get(3).map(|s| s.trim_matches(|c| c == '(' || c == ')')),
                })
            } else {
                serde_json::json!({"raw": line})
            }
        })
        .collect();

    (StatusCode::OK, Json(serde_json::json!(keys))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_host(name: &str) -> DiscoveredHost {
        DiscoveredHost {
            host: name.to_string(),
            target: name.to_string(),
            user: None,
            port: None,
            source: discovery::HostSource::Config,
        }
    }

    async fn probe_host_with_binary(
        host: &DiscoveredHost,
        binary: &FsPath,
        timeout: Duration,
    ) -> HostAuth {
        probe_host_gated(
            &ProbeGate::new(SSH_PROBE_CONCURRENCY),
            host,
            binary,
            timeout,
        )
        .await
    }

    fn spawn_count(script: &FsPath) -> usize {
        std::fs::read_to_string(format!("{}.log", script.display()))
            .map(|log| log.lines().count())
            .unwrap_or(0)
    }

    /// Catches: concurrent probes of one host (two clicks, or a bulk probe and
    /// a click) each spawning their own ssh.
    #[tokio::test]
    async fn overlapping_probes_of_one_host_spawn_one_ssh() {
        let counter = crate::test_support::fake_ssh_script(
            "ssh-hosts-single-flight",
            "echo x >> \"$0.log\"; sleep 1; exit 0",
            "echo x>> \"%~f0.log\"\r\nping -n 3 127.0.0.1 >nul\r\nexit /b 0",
        );
        let _ = std::fs::remove_file(format!("{}.log", counter.display()));
        let gate = ProbeGate::new(SSH_PROBE_CONCURRENCY);
        let cache = tokio::sync::Mutex::new(None);
        let listed = vec![config_host("one")];
        let timeout = Duration::from_secs(5);
        let (a, b, bulk) = tokio::join!(
            probe_listed_host(&gate, listed.clone(), "one", None, &counter, timeout),
            probe_listed_host(&gate, listed.clone(), "ONE", None, &counter, timeout),
            probe_cached_with(&gate, &cache, listed.clone(), &counter, timeout),
        );
        assert_eq!(a.unwrap().auth, HostAuth::Shell);
        assert_eq!(b.unwrap().auth, HostAuth::Shell);
        assert_eq!(bulk.unwrap()[0].auth, HostAuth::Shell);
        assert_eq!(spawn_count(&counter), 1);
    }

    /// Catches: no ceiling on running ssh processes; the gate's permit count
    /// must hold across bulk and single probes of different hosts.
    #[tokio::test]
    async fn no_more_probes_run_at_once_than_the_gate_allows() {
        // cmd append redirections are not a concurrent event log. Lock only
        // each write, leaving the wait outside the lock so probes can overlap.
        let tracker = crate::test_support::fake_ssh_script(
            "ssh-hosts-permits",
            "echo + >> \"$0.log\"; sleep 1; echo - >> \"$0.log\"; exit 0",
            &format!(
                r#"call :record +
if errorlevel 1 exit /b 1
{ping} -n 3 127.0.0.1 >nul
if errorlevel 1 exit /b 1
call :record -
exit /b %errorlevel%
:record
mkdir "%~f0.lock" 2>nul
if errorlevel 1 (
    {ping} -n 2 127.0.0.1 >nul
    if errorlevel 1 exit /b 1
    goto record
)
>> "%~f0.log" echo %1
set "record_status=%errorlevel%"
rmdir "%~f0.lock"
if errorlevel 1 exit /b 1
exit /b %record_status%"#,
                ping = crate::test_support::system32_exe("ping.exe"),
            ),
        );
        let _ = std::fs::remove_file(format!("{}.log", tracker.display()));
        let _ = std::fs::remove_dir(format!("{}.lock", tracker.display()));
        let gate = ProbeGate::new(2);
        let listed: Vec<DiscoveredHost> = (0..4).map(|i| config_host(&format!("h{i}"))).collect();
        let timeout = Duration::from_secs(10);
        let bulk = probe_hosts_with_gate(&gate, listed[..3].to_vec(), &tracker, timeout);
        let single = probe_listed_host(&gate, listed.clone(), "h3", None, &tracker, timeout);
        let (statuses, single) = tokio::join!(bulk, single);
        assert!(statuses.iter().all(|s| s.auth == HostAuth::Shell));
        assert_eq!(single.unwrap().auth, HostAuth::Shell);
        let (mut running, mut peak) = (0i32, 0i32);
        for line in std::fs::read_to_string(format!("{}.log", tracker.display()))
            .unwrap()
            .lines()
        {
            running += match line.trim() {
                "+" => 1,
                "-" => -1,
                other => panic!("unexpected probe event: {other:?}"),
            };
            assert!(running >= 0, "a probe ended without a recorded start");
            peak = peak.max(running);
        }
        assert_eq!(
            spawn_count(&tracker),
            8,
            "4 hosts, one start and one end each"
        );
        assert_eq!(running, 0, "every started probe must end");
        assert_eq!(peak, 2, "peak concurrent ssh processes");
    }

    /// Catches: a failed probe being replayed from memory, so after the
    /// network is fixed a manual probe still shows the stale failure.
    #[tokio::test]
    async fn a_manual_probe_asks_ssh_again_after_a_failure() {
        let down = crate::test_support::fake_ssh_script(
            "ssh-hosts-manual-down",
            "echo 'ssh: connect to host x port 22: Connection refused' >&2; exit 255",
            "echo ssh: connect to host x port 22: Connection refused 1>&2& exit /b 255",
        );
        let up = crate::test_support::fake_ssh_script("ssh-hosts-manual-up", "exit 0", "exit /b 0");
        let gate = ProbeGate::new(SSH_PROBE_CONCURRENCY);
        let listed = vec![config_host("flaky")];
        let timeout = Duration::from_secs(5);
        let first = probe_listed_host(&gate, listed.clone(), "flaky", None, &down, timeout)
            .await
            .unwrap();
        let second = probe_listed_host(&gate, listed, "flaky", None, &up, timeout)
            .await
            .unwrap();
        assert_eq!(first.auth, HostAuth::Unreachable);
        assert_eq!(second.auth, HostAuth::Shell);
    }

    /// Catches: the single probe contacting a caller-supplied name that the
    /// discovery list does not contain.
    #[tokio::test]
    async fn a_probe_for_an_unlisted_host_spawns_nothing() {
        let counter = crate::test_support::fake_ssh_script(
            "ssh-hosts-unlisted",
            "echo x >> \"$0.log\"; exit 0",
            "echo x>> \"%~f0.log\"\r\nexit /b 0",
        );
        let _ = std::fs::remove_file(format!("{}.log", counter.display()));
        let gate = ProbeGate::new(SSH_PROBE_CONCURRENCY);
        let result = probe_listed_host(
            &gate,
            vec![config_host("known")],
            "other",
            None,
            &counter,
            Duration::from_secs(2),
        )
        .await;
        assert!(result.is_err());
        assert_eq!(spawn_count(&counter), 0);
    }
    #[test]
    fn ssh_hosts_are_deduplicated_and_wildcards_are_omitted() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config");
        std::fs::write(
            &config,
            "Host alpha beta\n  HostName example.test\nHost alpha\n  User boss\nHost * !internal\n  ServerAliveInterval 10\n",
        )
        .unwrap();

        assert_eq!(
            parse_ssh_config_hosts(&config).unwrap(),
            vec!["alpha".to_string(), "beta".to_string()]
        );
    }

    #[test]
    fn ssh_hosts_probe_uses_the_noninteractive_bounded_command() {
        assert_eq!(
            probe_args("vps", None, "accept-new"),
            vec![
                "-o",
                "BatchMode=yes",
                "-o",
                "ConnectTimeout=5",
                "-o",
                "StrictHostKeyChecking=accept-new",
                "--",
                "vps",
                "true",
            ]
        );
    }

    #[tokio::test]
    async fn ssh_hosts_probe_classifies_shell_no_shell_auth_and_timeout() {
        let shell = crate::test_support::fake_ssh_script("ssh-hosts-shell", "exit 0", "exit /b 0");
        let no_shell = crate::test_support::fake_ssh_script(
            "ssh-hosts-no-shell",
            "echo 'This service does not provide shell access.' >&2; exit 1",
            "echo This service does not provide shell access. 1>&2& exit /b 1",
        );
        let auth = crate::test_support::fake_ssh_script(
            "ssh-hosts-auth",
            "echo 'Permission denied (publickey).' >&2; exit 255",
            "echo Permission denied (publickey). 1>&2& exit /b 255",
        );
        let timeout = crate::test_support::fake_ssh_script(
            "ssh-hosts-timeout",
            "sleep 2",
            "ping -n 3 127.0.0.1 >nul",
        );

        assert_eq!(
            probe_host_with_binary(&config_host("host"), &shell, Duration::from_secs(1)).await,
            HostAuth::Shell
        );
        assert_eq!(
            probe_host_with_binary(&config_host("host"), &no_shell, Duration::from_secs(1)).await,
            HostAuth::NoShell
        );
        assert_eq!(
            probe_host_with_binary(&config_host("host"), &auth, Duration::from_secs(1)).await,
            HostAuth::AuthFailed
        );
        assert_eq!(
            probe_host_with_binary(&config_host("host"), &timeout, Duration::from_millis(50)).await,
            HostAuth::Unreachable
        );
    }

    #[tokio::test]
    async fn ssh_hosts_probe_cache_reuses_results_for_sixty_seconds() {
        let cache = tokio::sync::Mutex::new(None);
        let shell =
            crate::test_support::fake_ssh_script("ssh-hosts-cache-shell", "exit 0", "exit /b 0");
        let auth = crate::test_support::fake_ssh_script(
            "ssh-hosts-cache-auth",
            "echo 'Permission denied' >&2; exit 255",
            "echo Permission denied 1>&2& exit /b 255",
        );
        let hosts = vec![config_host("cached")];

        let first = probe_cached(&cache, hosts.clone(), &shell, Duration::from_secs(1))
            .await
            .unwrap();
        let second = probe_cached(&cache, hosts, &auth, Duration::from_secs(1))
            .await
            .unwrap();

        assert_eq!(first[0].auth, HostAuth::Shell);
        assert_eq!(second, first, "fresh cache must skip the second process");
    }

    /// Catches: a known_hosts entry probed with `accept-new` (a changed key
    /// accepted for a host that is already trusted) or without its `-p` port.
    #[tokio::test]
    async fn a_known_hosts_probe_is_strict_and_carries_its_port() {
        let recorder = crate::test_support::fake_ssh_script(
            "ssh-hosts-known-strict",
            "printf '%s\\n' \"$*\" > \"$0.log\"; exit 0",
            "echo %* > \"%~f0.log\"\r\nexit /b 0",
        );
        let _ = std::fs::remove_file(format!("{}.log", recorder.display()));
        let host = DiscoveredHost {
            host: "10.0.0.5".into(),
            target: "10.0.0.5".into(),
            user: None,
            port: Some(2222),
            source: discovery::HostSource::KnownHosts,
        };
        probe_host_with_binary(&host, &recorder, Duration::from_secs(2)).await;
        let log = std::fs::read_to_string(format!("{}.log", recorder.display())).unwrap();
        assert!(log.contains("StrictHostKeyChecking=yes"), "{log}");
        assert!(!log.contains("accept-new"), "{log}");
        assert!(log.contains("-p 2222 -- 10.0.0.5"), "{log}");
    }

    /// Catches: the probe cache mutex held across the ssh processes, so a
    /// second request waits for every host of the first.
    #[tokio::test]
    async fn the_probe_cache_lock_is_free_while_ssh_runs() {
        let cache = std::sync::Arc::new(tokio::sync::Mutex::new(None));
        let slow = crate::test_support::fake_ssh_script(
            "ssh-hosts-lock-slow",
            "sleep 1; exit 0",
            "ping -n 3 127.0.0.1 >nul",
        );
        let running = tokio::spawn({
            let cache = cache.clone();
            async move {
                probe_cached(
                    &cache,
                    vec![config_host("slow")],
                    &slow,
                    Duration::from_secs(5),
                )
                .await
            }
        });
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(
            cache.try_lock().is_ok(),
            "cache locked while ssh is running"
        );
        running.await.unwrap().unwrap();
    }

    /// Catches: an unbounded known_hosts read; a file over the cap must be cut
    /// at its last full line, never mid-name.
    #[test]
    fn an_oversized_known_hosts_is_cut_at_a_full_line() {
        let line = "h.example ssh-rsa AAAA\n";
        let count = (KNOWN_HOSTS_MAX_BYTES as usize / line.len()) + 10;
        let bytes = line.repeat(count).into_bytes();
        let text = bounded_text(bytes);
        assert!(text.len() as u64 <= KNOWN_HOSTS_MAX_BYTES);
        assert!(text.ends_with('\n'));
    }
}

#[cfg(test)]
mod hostile_probe_tests {
    use super::*;

    /// Catches: a host name starting with `-` (from an ssh config alias) being
    /// passed to `ssh` as an option because no `--` precedes it.
    #[test]
    fn probe_args_cannot_let_a_host_become_an_option() {
        let args = probe_args("-oProxyCommand=evil", None, "yes");
        let host_at = args
            .iter()
            .position(|a| a == "-oProxyCommand=evil")
            .unwrap();
        assert_eq!(args[host_at - 1], "--", "host must follow `--`: {args:?}");
    }
}

#[cfg(test)]
mod critic_r3_tests {
    use super::*;

    /// Catches: a file over the cap with no newline in its first
    /// `KNOWN_HOSTS_MAX_BYTES` handed to the parser cut mid-name.
    #[test]
    fn an_oversized_known_hosts_without_a_newline_yields_no_partial_line() {
        let bytes = vec![b'a'; KNOWN_HOSTS_MAX_BYTES as usize + 10];
        assert_eq!(bounded_text(bytes), "");
    }

    /// Catches: the cache lock released across ssh with no single-flight, so two
    /// concurrent bulk probes of the same hosts each spawn every ssh process.
    #[tokio::test]
    async fn concurrent_bulk_probes_spawn_each_host_once() {
        let counter = crate::test_support::fake_ssh_script(
            "ssh-hosts-critic-r3-count",
            "echo x >> \"$0.log\"; sleep 1; exit 0",
            "echo x>> \"%~f0.log\"\r\nping -n 3 127.0.0.1 >nul\r\nexit /b 0",
        );
        let _ = std::fs::remove_file(format!("{}.log", counter.display()));
        let cache = tokio::sync::Mutex::new(None);
        let hosts = vec![DiscoveredHost {
            host: "one".to_string(),
            target: "one".to_string(),
            user: None,
            port: None,
            source: discovery::HostSource::Config,
        }];
        let (a, b) = tokio::join!(
            probe_cached(&cache, hosts.clone(), &counter, Duration::from_secs(5)),
            probe_cached(&cache, hosts.clone(), &counter, Duration::from_secs(5)),
        );
        a.unwrap();
        b.unwrap();
        let runs = std::fs::read_to_string(format!("{}.log", counter.display()))
            .unwrap()
            .lines()
            .count();
        assert_eq!(runs, 1, "ssh spawned {runs} times for one host");
    }
}

#[cfg(test)]
mod critic_round6_tests {
    use super::*;

    fn host(name: &str, port: Option<u16>, source: discovery::HostSource) -> DiscoveredHost {
        DiscoveredHost {
            host: name.to_string(),
            target: name.to_string(),
            user: None,
            port,
            source,
        }
    }

    fn config_host(name: &str) -> DiscoveredHost {
        host(name, None, discovery::HostSource::Config)
    }

    fn counting_script(name: &str, sleep_secs: u32) -> PathBuf {
        // cmd append redirections can lose a record when both ports spawn at once.
        // Serialize only the append, so the SSH processes still overlap.
        let script = crate::test_support::fake_ssh_script(
            name,
            &format!("echo x >> \"$0.log\"; sleep {sleep_secs}; exit 0"),
            &format!(
                r#"call :record
if errorlevel 1 exit /b 1
{ping} -n {pings} 127.0.0.1 >nul
exit /b %errorlevel%
:record
mkdir "%~f0.lock" 2>nul
if errorlevel 1 (
    {ping} -n 2 127.0.0.1 >nul
    if errorlevel 1 exit /b 1
    goto record
)
>> "%~f0.log" echo x
set "record_status=%errorlevel%"
rmdir "%~f0.lock"
if errorlevel 1 exit /b 1
exit /b %record_status%"#,
                ping = crate::test_support::system32_exe("ping.exe"),
                pings = sleep_secs + 1,
            ),
        );
        let _ = std::fs::remove_file(format!("{}.log", script.display()));
        let _ = std::fs::remove_dir(format!("{}.lock", script.display()));
        script
    }

    fn spawns(script: &FsPath) -> usize {
        std::fs::read_to_string(format!("{}.log", script.display()))
            .map(|log| log.lines().count())
            .unwrap_or(0)
    }

    /// Catches: the manual probe reading the bulk cache again, so a click on
    /// "Probe" replays a failure the bulk list cached up to a minute ago.
    #[tokio::test]
    async fn a_manual_probe_ignores_a_fresh_bulk_cache_entry() {
        let down = crate::test_support::fake_ssh_script(
            "ssh-r6-cache-down",
            "echo 'ssh: connect to host x port 22: Connection refused' >&2; exit 255",
            "echo ssh: connect to host x port 22: Connection refused 1>&2& exit /b 255",
        );
        let up = counting_script("ssh-r6-cache-up", 0);
        let gate = ProbeGate::new(SSH_PROBE_CONCURRENCY);
        let cache = tokio::sync::Mutex::new(None);
        let listed = vec![config_host("cached")];
        let timeout = Duration::from_secs(5);
        let bulk = probe_cached_with(&gate, &cache, listed.clone(), &down, timeout)
            .await
            .unwrap();
        assert_eq!(bulk[0].auth, HostAuth::Unreachable);
        let manual = probe_listed_host(&gate, listed, "cached", None, &up, timeout)
            .await
            .unwrap();
        assert_eq!(manual.auth, HostAuth::Shell);
        assert_eq!(spawns(&up), 1);
    }

    /// Catches: the per-host slot replaying a finished probe to a bulk probe
    /// that started later, so the list shows a result older than its request.
    #[tokio::test]
    async fn a_bulk_probe_started_after_a_manual_one_finished_asks_ssh_again() {
        let up = crate::test_support::fake_ssh_script("ssh-r6-after-up", "exit 0", "exit /b 0");
        let down = crate::test_support::fake_ssh_script(
            "ssh-r6-after-down",
            "echo 'ssh: connect to host x port 22: Connection refused' >&2; exit 255",
            "echo ssh: connect to host x port 22: Connection refused 1>&2& exit /b 255",
        );
        let gate = ProbeGate::new(SSH_PROBE_CONCURRENCY);
        let listed = vec![config_host("later")];
        let timeout = Duration::from_secs(5);
        let manual = probe_listed_host(&gate, listed.clone(), "later", None, &up, timeout)
            .await
            .unwrap();
        assert_eq!(manual.auth, HostAuth::Shell);
        let bulk = probe_hosts_with_gate(&gate, listed, &down, timeout).await;
        assert_eq!(bulk[0].auth, HostAuth::Unreachable);
    }

    /// Catches: the single-flight slot keyed by target alone, so two ports of
    /// one machine share one ssh and one of them reports the other's result.
    #[tokio::test]
    async fn probes_of_one_target_on_different_ports_each_spawn_ssh() {
        let counter = counting_script("ssh-r6-ports", 1);
        let gate = ProbeGate::new(SSH_PROBE_CONCURRENCY);
        let listed = vec![
            host("box", Some(22), discovery::HostSource::KnownHosts),
            host("box", Some(2222), discovery::HostSource::KnownHosts),
        ];
        let timeout = Duration::from_secs(5);
        let (a, b) = tokio::join!(
            probe_listed_host(&gate, listed.clone(), "box", Some(22), &counter, timeout),
            probe_listed_host(&gate, listed.clone(), "box", Some(2222), &counter, timeout),
        );
        assert_eq!(a.unwrap().auth, HostAuth::Shell);
        assert_eq!(b.unwrap().auth, HostAuth::Shell);
        assert_eq!(spawns(&counter), 2);
    }

    /// Catches: a request name compared to a mixed-case listed target without
    /// lowercasing both sides, so "Prod.Example.com" is rejected as unlisted.
    #[tokio::test]
    async fn a_mixed_case_listed_target_is_found_by_any_case_variant() {
        let up = crate::test_support::fake_ssh_script("ssh-r6-case", "exit 0", "exit /b 0");
        let gate = ProbeGate::new(SSH_PROBE_CONCURRENCY);
        let listed = vec![host(
            "Prod.Example.com",
            None,
            discovery::HostSource::KnownHosts,
        )];
        for variant in ["Prod.Example.com", "prod.example.com", "PROD.EXAMPLE.COM"] {
            let status = probe_listed_host(
                &gate,
                listed.clone(),
                variant,
                Some(22),
                &up,
                Duration::from_secs(5),
            )
            .await
            .unwrap_or_else(|error| panic!("{variant} not found: {error}"));
            assert_eq!(status.auth, HostAuth::Shell);
        }
    }

    /// Catches: a probe cancelled mid-flight (client disconnect) leaving the
    /// gate's only permit or the host slot held, so every later probe hangs.
    #[tokio::test]
    async fn a_cancelled_probe_leaves_the_gate_and_the_host_slot_usable() {
        let counter = counting_script("ssh-r6-cancel", 1);
        let gate = ProbeGate::new(1);
        let listed = vec![config_host("dropped")];
        let timeout = Duration::from_secs(5);
        let cancelled = tokio::time::timeout(
            Duration::from_millis(200),
            probe_listed_host(&gate, listed.clone(), "dropped", None, &counter, timeout),
        )
        .await;
        assert!(cancelled.is_err(), "the probe was meant to still run");
        let next = tokio::time::timeout(
            Duration::from_secs(4),
            probe_listed_host(&gate, listed, "dropped", None, &counter, timeout),
        )
        .await
        .expect("a cancelled probe left a permit or the host slot held")
        .unwrap();
        assert_eq!(next.auth, HostAuth::Shell);
    }

    /// Catches: the permit surviving a timed-out probe, so with every permit
    /// taken by hung hosts no other host is ever probed again.
    #[tokio::test]
    async fn a_timed_out_probe_releases_its_permit_to_the_next_host() {
        let script = crate::test_support::fake_ssh_script(
            "ssh-r6-timeout-permit",
            "case \"$*\" in *slow*) sleep 15;; esac; exit 0",
            &format!(
                "echo %* | {} slow >nul\r\nif not errorlevel 1 {} -n 16 127.0.0.1 >nul\r\nexit /b 0",
                crate::test_support::system32_exe("findstr.exe"),
                crate::test_support::system32_exe("ping.exe"),
            ),
        );
        let gate = ProbeGate::new(1);
        let listed = vec![config_host("slow"), config_host("fast")];
        // The slow fixture exceeds this deadline; the fast one has time to
        // start even when process creation is delayed on a loaded runner.
        let timeout = Duration::from_secs(5);
        let statuses = tokio::time::timeout(
            Duration::from_secs(15),
            probe_hosts_with_gate(&gate, listed, &script, timeout),
        )
        .await
        .expect("the permit of a timed-out probe was never released");
        let auth = |name: &str| statuses.iter().find(|s| s.host == name).unwrap().auth;
        assert_eq!(auth("slow"), HostAuth::Unreachable);
        assert_eq!(auth("fast"), HostAuth::Shell);
    }

    /// Catches: single-flight applying only between a bulk and a single probe,
    /// so two overlapping bulk probes (two panels, a retry) spawn every host twice.
    #[tokio::test]
    async fn two_overlapping_bulk_probes_spawn_each_host_once() {
        let counter = counting_script("ssh-r6-two-bulks", 1);
        let gate = ProbeGate::new(2);
        let listed: Vec<DiscoveredHost> = (0..6).map(|i| config_host(&format!("b{i}"))).collect();
        let (cache_a, cache_b) = (tokio::sync::Mutex::new(None), tokio::sync::Mutex::new(None));
        let timeout = Duration::from_secs(10);
        let (a, b) = tokio::join!(
            probe_cached_with(&gate, &cache_a, listed.clone(), &counter, timeout),
            probe_cached_with(&gate, &cache_b, listed.clone(), &counter, timeout),
        );
        assert!(a.unwrap().iter().all(|s| s.auth == HostAuth::Shell));
        assert!(b.unwrap().iter().all(|s| s.auth == HostAuth::Shell));
        assert_eq!(spawns(&counter), 6);
    }
}
