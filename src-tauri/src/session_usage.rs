//! Session-owned, display-ready usage. Never infer account ownership from a workspace.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::AsyncReadExt;

use crate::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct UsageLine {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SessionUsage {
    pub agent: Option<String>,
    pub model: Option<String>,
    pub scope: Option<String>,
    pub source: Option<String>,
    pub availability: String,
    pub reason: Option<String>,
    pub observed_at_ms: Option<u64>,
    pub stale: bool,
    pub lines: Vec<UsageLine>,
}

impl SessionUsage {
    fn unavailable(reason: &str) -> Self {
        Self {
            agent: None,
            model: None,
            scope: None,
            source: None,
            availability: "unavailable".into(),
            reason: Some(reason.into()),
            observed_at_ms: None,
            stale: false,
            lines: Vec::new(),
        }
    }

    fn available(agent: &str, scope: &str, source: &str) -> Self {
        Self {
            agent: Some(agent.into()),
            scope: Some(scope.into()),
            source: Some(source.into()),
            availability: "available".into(),
            reason: None,
            ..Self::unavailable("")
        }
    }

    fn line(&mut self, label: &str, value: impl Into<String>) {
        self.lines.push(UsageLine {
            label: label.into(),
            value: value.into(),
        });
    }

    fn finish(self) -> Self {
        if self.lines.is_empty() {
            Self::unavailable("no reported usage")
        } else {
            self
        }
    }
}

type Cached = Arc<tokio::sync::Mutex<Option<(SessionUsage, Instant)>>>;
static CACHE: LazyLock<parking_lot::Mutex<HashMap<String, Cached>>> =
    LazyLock::new(|| parking_lot::Mutex::new(HashMap::new()));
static READ_SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

/// At most 32 retained snapshots, one fetch per identity, and two CLI readers at once.
/// The public request deadline also bounds time spent queued behind another reader.
async fn cached(
    key: String,
    ttl: Duration,
    fetch: impl std::future::Future<Output = Result<SessionUsage, String>>,
) -> Result<SessionUsage, String> {
    let entry = {
        let mut cache = CACHE.lock();
        if !cache.contains_key(&key) && cache.len() >= 32 {
            cache.retain(|_, value| Arc::strong_count(value) > 1);
            if cache.len() >= 32 {
                return Err("usage readers busy".into());
            }
        }
        cache
            .entry(key)
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(None)))
            .clone()
    };
    let mut guard = entry.lock().await;
    if let Some((snapshot, at)) = &*guard
        && at.elapsed() < ttl
    {
        return Ok(snapshot.clone());
    }
    let _permit = READ_SLOTS
        .acquire()
        .await
        .map_err(|_| "usage readers stopped")?;
    match fetch.await {
        Ok(snapshot) => {
            *guard = Some((snapshot.clone(), Instant::now()));
            Ok(snapshot)
        }
        Err(error) => {
            if !crate::cli_usage_rpc::looks_like_auth_failure(&error)
                && !crate::cli_usage_rpc::looks_like_contract_failure(&error)
                && let Some((snapshot, at)) = &*guard
                && at.elapsed() < Duration::from_secs(1800)
            {
                let mut snapshot = snapshot.clone();
                snapshot.stale = true;
                return Ok(snapshot);
            }
            Err(error)
        }
    }
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn get_session_usage(
    state: tauri::State<'_, Arc<AppState>>,
    session_id: String,
) -> Result<SessionUsage, String> {
    Ok(get_session_usage_impl(&state, &session_id).await)
}

pub(crate) async fn get_session_usage_impl(state: &AppState, session_id: &str) -> SessionUsage {
    // The owner check precedes every local lookup, cache read and provider operation.
    if let Some(owner) = crate::remote_mirror::owner_connection(state, session_id) {
        return remote_usage(state, &owner, session_id)
            .await
            .unwrap_or_else(|| SessionUsage::unavailable("remote session"));
    }
    match tokio::time::timeout(Duration::from_secs(30), local_usage(state, session_id)).await {
        Ok(Ok(snapshot)) => snapshot,
        // Do not expose provider stderr, profile paths or account metadata as an error reason.
        _ => SessionUsage::unavailable("session usage source unavailable"),
    }
}

async fn remote_usage(state: &AppState, owner: &str, session_id: &str) -> Option<SessionUsage> {
    let base = state.remote.base_url(owner)?;
    let mut url = reqwest::Url::parse(&base).ok()?;
    url.path_segments_mut()
        .ok()?
        .extend(["sessions", session_id, "usage"]);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .ok()?;
    let mut request = client.get(url);
    if let Some(token) = state.remote.token(owner) {
        request = request.bearer_auth(token);
    }
    request
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .await
        .ok()
}

async fn local_usage(state: &AppState, session_id: &str) -> Result<SessionUsage, String> {
    let agent = state
        .session_maps
        .session_states
        .get(session_id)
        .and_then(|session| session.agent_type.clone());
    let Some(agent) = agent else {
        return Ok(SessionUsage::unavailable("no active agent"));
    };
    let Some(pid) = crate::pty::session_leaf_pid(state, session_id) else {
        return Ok(SessionUsage::unavailable("session binding unavailable"));
    };
    match agent.as_str() {
        "claude" => {
            let (quota, stale) = crate::claude_usage::session_quota(state, session_id).await?;
            let mut snapshot = claude_quota(quota);
            snapshot.stale = stale;
            Ok(snapshot)
        }
        "codex" => {
            let environment = crate::process_env::read_process_environment(pid)
                .map_err(|_| "Codex profile unavailable")?;
            let argv =
                crate::process_env::read_process_argv(pid).ok_or("Codex profile unavailable")?;
            let (home, codex_home, profile) = codex_profile(&environment, &argv)?;
            let key = format!("codex:{home:?}:{codex_home:?}:{profile:?}");
            cached(key, Duration::from_secs(300), async {
                crate::codex_usage::session_quota(&home, &codex_home, profile.as_deref())
                    .await
                    .map(codex_quota)
            })
            .await
        }
        "ego" => {
            let argv =
                crate::process_env::read_process_argv(pid).ok_or("ego binding unavailable")?;
            // The existing exact resolver knows EGO_HOME, not an explicit CLI store override.
            if argv
                .iter()
                .any(|arg| arg == "--session-dir" || arg.starts_with("--session-dir="))
            {
                return Ok(SessionUsage::unavailable("explicit ego store is not bound"));
            }
            let (home, ego_session) = crate::agent_session::ego_chat_session_for_pid(pid)
                .ok_or("ego binding unavailable")?;
            let manifest: Value = serde_json::from_slice(
                &std::fs::read(
                    home.join("sessions")
                        .join(&ego_session)
                        .join("manifest.json"),
                )
                .map_err(|_| "ego workspace unavailable")?,
            )
            .map_err(|_| "ego workspace unavailable")?;
            let workspace = manifest
                .get("workspace_root")
                .and_then(Value::as_str)
                .ok_or("ego workspace unavailable")?;
            // PTY usage follows the terminal executable, not AI Chat's ACP setting.
            let program = match argv.first().map(PathBuf::from) {
                Some(path) if path.is_absolute() && path.is_file() => path,
                _ => crate::agent::detect_agent_binary("ego".into())
                    .await
                    .path
                    .map(PathBuf::from)
                    .ok_or("ego executable unavailable")?,
            };
            if !home.is_absolute() || !program.is_absolute() || !Path::new(workspace).is_absolute()
            {
                return Ok(SessionUsage::unavailable(
                    "ego executable or workspace unavailable",
                ));
            }
            let key = format!("ego:{program:?}:{home:?}:{workspace:?}:{ego_session}");
            cached(key, Duration::from_secs(30), async {
                let report = ego_report(&program, &home, workspace).await?;
                ego_snapshot(&report, &ego_session)
            })
            .await
        }
        _ => Ok(SessionUsage::unavailable("unsupported agent")),
    }
}

fn env_value<'a>(environment: &'a [String], name: &str) -> Option<&'a str> {
    environment.iter().find_map(|entry| {
        let (key, value) = entry.split_once('=')?;
        ((if cfg!(windows) {
            key.eq_ignore_ascii_case(name)
        } else {
            key == name
        }) && !value.is_empty())
        .then_some(value)
    })
}

fn codex_profile(
    environment: &[String],
    argv: &[String],
) -> Result<(String, String, Option<String>), String> {
    let home = env_value(environment, "HOME")
        .or_else(|| env_value(environment, "USERPROFILE"))
        .ok_or("Codex home unavailable")?;
    let codex_home = env_value(environment, "CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(home).join(".codex"));
    if !Path::new(home).is_absolute() || !codex_home.is_absolute() {
        return Err("Codex home unavailable".into());
    }
    let mut profile = None;
    let mut args = argv.iter().skip(1);
    while let Some(arg) = args.next() {
        // Configuration overrides can select a different auth/provider context. Do not guess.
        if arg == "-c"
            || arg == "--config"
            || arg.starts_with("--config=")
            || arg.starts_with("-c")
            || arg == "--oss"
            || arg.starts_with("--local-provider")
        {
            return Err("Codex configuration override is not bound".into());
        }
        if arg == "-p" || arg == "--profile" {
            profile = Some(
                args.next()
                    .filter(|p| !p.starts_with('-'))
                    .ok_or("Codex profile unavailable")?
                    .clone(),
            );
        } else if let Some(value) = arg
            .strip_prefix("--profile=")
            .or_else(|| arg.strip_prefix("-p").filter(|value| !value.is_empty()))
        {
            if value.is_empty() {
                return Err("Codex profile unavailable".into());
            }
            profile = Some(value.into());
        }
    }
    Ok((
        home.into(),
        codex_home.to_string_lossy().into_owned(),
        profile,
    ))
}

fn quota_line(snapshot: &mut SessionUsage, label: &str, percent: f64, reset: Option<String>) {
    if !percent.is_finite() || !(0.0..=100.0).contains(&percent) {
        return;
    }
    snapshot.line("Account quota", format!("{label} {percent:.1}%"));
    if let Some(reset) = reset {
        snapshot.line("Resets", format!("{label}: {reset}"));
    }
}

fn claude_quota(quota: crate::claude_usage::UsageApiResponse) -> SessionUsage {
    let mut snapshot = SessionUsage::available("claude", "account_quota", "claude_oauth_usage");
    for (label, bucket) in [("5h", quota.five_hour), ("7d", quota.seven_day)] {
        if let Some(bucket) = bucket {
            quota_line(&mut snapshot, label, bucket.utilization, bucket.resets_at);
        }
    }
    snapshot.finish()
}

fn codex_quota(quota: crate::codex_usage::CodexUsageApiResponse) -> SessionUsage {
    let mut snapshot = SessionUsage::available("codex", "account_quota", "codex_app_server");
    if let Some(rate) = quota.rate_limit {
        for (label, window) in [
            ("primary", rate.primary_window),
            ("secondary", rate.secondary_window),
        ] {
            if let Some(window) = window {
                let label = window
                    .limit_window_seconds
                    .map(|s| format!("{}m", s / 60))
                    .unwrap_or_else(|| label.into());
                let reset = window
                    .reset_at
                    .and_then(|at| chrono::DateTime::from_timestamp(at, 0))
                    .map(|at| at.to_rfc3339());
                quota_line(&mut snapshot, &label, window.used_percent, reset);
            }
        }
    }
    snapshot.finish()
}

/// Bound stdout before decoding: a large workspace is not an unbounded memory allocation.
async fn ego_report(program: &Path, home: &Path, workspace: &str) -> Result<Value, String> {
    const MAX_REPORT: u64 = 4 * 1024 * 1024;
    let mut command = std::process::Command::new(program);
    command
        .args(["usage", "--json", "-C", workspace, "--session-dir"])
        .arg(home.join("sessions"))
        .env("EGO_HOME", home)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    crate::cli::apply_no_window(&mut command);
    let mut command: tokio::process::Command = command.into();
    command.kill_on_drop(true);
    let mut child = command.spawn().map_err(|_| "ego usage unavailable")?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or("ego usage unavailable")?
        .take(MAX_REPORT + 1);
    let mut bytes = Vec::new();
    let read = async {
        stdout
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| "ego usage unavailable")?;
        if bytes.len() as u64 > MAX_REPORT {
            return Err("ego usage report too large");
        }
        if !child
            .wait()
            .await
            .map_err(|_| "ego usage unavailable")?
            .success()
        {
            return Err("ego usage unavailable");
        }
        Ok(())
    };
    let result = tokio::time::timeout(Duration::from_secs(20), read).await;
    let _ = child.kill().await;
    let _ = child.wait().await;
    result.map_err(|_| "ego usage timed out")??;
    serde_json::from_slice(&bytes).map_err(|_| "Failed to parse ego usage".into())
}

fn ego_snapshot(report: &Value, session_id: &str) -> Result<SessionUsage, String> {
    if report.get("v").and_then(Value::as_u64) != Some(3) {
        return Err("Failed to parse ego usage version".into());
    }
    let sessions = report
        .get("sessions")
        .and_then(Value::as_array)
        .ok_or("Failed to parse ego usage sessions")?;
    let mut matching = sessions
        .iter()
        .filter(|row| row.get("session").and_then(Value::as_str) == Some(session_id));
    let row = matching.next().ok_or("ego session usage unavailable")?;
    if matching.next().is_some() {
        return Err("Failed to parse duplicate ego session".into());
    }
    let total = &row["lines"]["total"];
    let usage = &total["usage"];
    let cost = &total["cost"];
    let mut snapshot = SessionUsage::available("ego", "session_usage", "ego_usage_v3");
    snapshot.model = row.get("model").and_then(Value::as_str).map(str::to_owned);
    snapshot.observed_at_ms = report.get("as_of").and_then(Value::as_u64);
    let categories = [
        ("uncached_input_tokens", "input"),
        ("cache_read_tokens", "cache read"),
        ("cache_write_tokens", "cache write"),
        ("cache_write_extended_tokens", "extended cache write"),
        ("output_tokens", "output"),
    ];
    let mut tokens = Vec::new();
    for (key, label) in categories {
        let count = usage
            .get(key)
            .and_then(Value::as_u64)
            .ok_or("Failed to parse ego usage tokens")?;
        tokens.push(format!("{count} {label}"));
    }
    if let Some(count) = usage.get("reasoning_tokens").and_then(Value::as_u64) {
        tokens.push(format!("{count} reasoning (included in output)"));
    }
    if let Some(count) = usage.get("web_search_requests").and_then(Value::as_u64) {
        tokens.push(format!("{count} web searches"));
    }
    snapshot.line("Session usage", tokens.join(" · "));
    if let Some(at) = snapshot
        .observed_at_ms
        .and_then(|at| i64::try_from(at).ok())
        .and_then(chrono::DateTime::from_timestamp_millis)
    {
        snapshot.line("Report as of", at.to_rfc3339());
    }
    for (key, label) in [
        ("known", "Cost"),
        ("subscription_equivalent", "API-equivalent estimate"),
    ] {
        let amounts = cost
            .get(key)
            .and_then(Value::as_array)
            .ok_or("Failed to parse ego usage cost")?;
        for money in amounts {
            let amount = money
                .get("amount")
                .and_then(Value::as_str)
                .ok_or("Failed to parse ego amount")?;
            let currency = money
                .get("currency")
                .and_then(Value::as_str)
                .ok_or("Failed to parse ego currency")?;
            let prefix = if key == "subscription_equivalent" {
                "≈"
            } else {
                ""
            };
            snapshot.line(label, format!("{prefix}{amount} {currency}"));
        }
    }
    for (key, label) in [
        ("unknown", "Cost unknown"),
        ("subscription_unknown", "Estimate unknown"),
    ] {
        let reasons = cost
            .get(key)
            .and_then(Value::as_array)
            .ok_or("Failed to parse ego cost reasons")?;
        for reason in reasons {
            snapshot.line(label, reason.to_string());
        }
    }
    if cost["known"].as_array().is_some_and(Vec::is_empty)
        && cost["unknown"].as_array().is_some_and(Vec::is_empty)
    {
        snapshot.line("Cost", "not reported");
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::tests_support::make_test_app_state;

    /// Catches: a live terminal ego ledger being hidden by empty ACP configuration.
    #[cfg(unix)]
    #[tokio::test]
    async fn terminal_ego_usage_does_not_require_acp_executable() {
        use portable_pty::{CommandBuilder, PtySize, native_pty_system};
        use std::sync::atomic::AtomicBool;

        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let script = crate::test_support::fake_ssh_script(
            "2150-terminal-ego-usage",
            r#"[ "$1" = usage ] || exit 2
cat "$EGO_HOME/report.json""#,
            "exit /b 2",
        );
        let binary = dir.path().join("ego");
        std::fs::copy(script, &binary).unwrap();
        let id = "01a02491-6a90-7ae2-9089-fd6a8532eb82";
        let session_dir = dir.path().join("sessions").join(id);
        std::fs::create_dir_all(&session_dir).unwrap();
        let mut report = report_with_sessions();
        report["sessions"][1]["session"] = serde_json::json!(id);
        std::fs::write(dir.path().join("report.json"), report.to_string()).unwrap();
        std::fs::write(
            session_dir.join("manifest.json"),
            serde_json::json!({
                "workspace_root": dir.path().to_str().unwrap()
            })
            .to_string(),
        )
        .unwrap();

        let pair = native_pty_system().openpty(PtySize::default()).unwrap();
        // A real foreground process with an ego argv0 and isolated initial env.
        // exec -a avoids writing or compiling a second native test executable.
        let mut command = CommandBuilder::new("/bin/bash");
        command.args(["-c", "exec -a \"$1\" /bin/sh -c 'read -r input'", "probe"]);
        command.arg(&binary);
        command.env("EGO_HOME", dir.path());
        let mut child = pair.slave.spawn_command(command).unwrap();
        let pid = child.process_id().unwrap();
        loop {
            if pair.master.process_group_leader() == Some(pid as i32)
                && crate::process_env::read_process_argv0(pid).as_deref() == binary.to_str()
            {
                break;
            }
            assert!(
                child.try_wait().unwrap().is_none(),
                "terminal probe exited during setup"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let lease_path = session_dir.join("lease");
        std::fs::write(
            &lease_path,
            serde_json::json!({"pid":pid,"heartbeat":1}).to_string(),
        )
        .unwrap();
        let lease = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&lease_path)
            .unwrap();
        lease.lock().unwrap();
        let state = make_test_app_state();
        state.config.write().ego_executable.clear();
        state.session_maps.sessions.insert(
            "terminal-ego".into(),
            parking_lot::Mutex::new(crate::state::PtySession {
                launch_receipt: None,
                writer: Arc::new(parking_lot::Mutex::new(pair.master.take_writer().unwrap())),
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
                shell: "/bin/bash".into(),
            }),
        );
        state.session_maps.session_states.insert(
            "terminal-ego".into(),
            crate::state::SessionState {
                agent_type: Some("ego".into()),
                ..Default::default()
            },
        );
        let snapshot = get_session_usage_impl(&state, "terminal-ego").await;
        state.config.write().ego_executable = "/unrelated/acp/ego".into();
        let with_other_acp = get_session_usage_impl(&state, "terminal-ego").await;
        let (_, session) = state.session_maps.sessions.remove("terminal-ego").unwrap();
        let mut session = session.into_inner();
        session._child.kill().unwrap();
        session._child.wait().unwrap();
        for snapshot in [snapshot, with_other_acp] {
            assert_eq!(snapshot.availability, "available", "{:?}", snapshot.reason);
            assert_eq!(snapshot.model.as_deref(), Some("current/model"));
            assert!(
                snapshot
                    .lines
                    .iter()
                    .any(|line| line.value.contains("24 output"))
            );
            assert!(!snapshot.lines.iter().any(|line| line.value.contains("999")));
        }
    }

    fn report_with_sessions() -> Value {
        let mut report: Value =
            serde_json::from_str(include_str!("fixtures/session_usage/ego-v3.json")).unwrap();
        let lines = report["totals"].clone();
        report["sessions"] = serde_json::json!([
            {"session":"sibling", "model":"sibling/model", "lines":lines},
            {"session":"exact", "model":"current/model", "lines":lines}
        ]);
        report["sessions"][0]["lines"]["total"]["usage"]["output_tokens"] = serde_json::json!(999);
        report["sessions"][1]["lines"]["total"]["usage"]["output_tokens"] = serde_json::json!(24);
        report
    }

    /// Catches: selecting the first/newest sibling row or workspace totals.
    #[test]
    fn ego_usage_selects_exact_session_not_sibling_or_totals() {
        let snapshot = ego_snapshot(&report_with_sessions(), "exact").unwrap();
        assert_eq!(snapshot.model.as_deref(), Some("current/model"));
        assert!(
            snapshot
                .lines
                .iter()
                .any(|line| line.value.contains("24 output"))
        );
        assert!(!snapshot.lines.iter().any(|line| line.value.contains("999")));
    }

    /// Catches: missing, duplicated or future-version rows falling back to account totals.
    #[test]
    fn ego_usage_rejects_missing_duplicate_and_unsupported_rows() {
        let mut report = report_with_sessions();
        assert!(ego_snapshot(&report, "absent").is_err());
        report["sessions"][0] = report["sessions"][1].clone();
        assert!(ego_snapshot(&report, "exact").is_err());
        report["v"] = serde_json::json!(4);
        assert!(ego_snapshot(&report, "exact").is_err());
    }

    /// Catches: estimates labelled as spend, currencies merged, or unknowns displayed as zero.
    #[test]
    fn ego_usage_keeps_estimates_currencies_and_partial_cost_separate() {
        let mut report = report_with_sessions();
        report["sessions"][1]["lines"]["total"]["cost"] = serde_json::json!({
            "known":[{"amount":"1.25","currency":"USD"},{"amount":"2.5","currency":"EUR"}],
            "unknown":[{"reason":"unpriced_category","category":"output"}],
            "subscription_equivalent":[{"amount":"3.75","currency":"USD"}],
            "subscription_unknown":[]
        });
        report["sessions"][1]["lines"]["total"]["usage"]
            .as_object_mut()
            .unwrap()
            .remove("reasoning_tokens");
        let snapshot = ego_snapshot(&report, "exact").unwrap();
        assert!(
            snapshot
                .lines
                .iter()
                .any(|line| line.label == "Cost" && line.value == "1.25 USD")
        );
        assert!(
            snapshot
                .lines
                .iter()
                .any(|line| line.label == "Cost" && line.value == "2.5 EUR")
        );
        assert!(
            snapshot
                .lines
                .iter()
                .any(|line| line.label == "API-equivalent estimate" && line.value == "≈3.75 USD")
        );
        assert!(
            snapshot.lines.iter().any(
                |line| line.label == "Cost unknown" && line.value.contains("unpriced_category")
            )
        );
        assert!(
            !snapshot
                .lines
                .iter()
                .any(|line| line.value.contains("0 reasoning"))
        );
    }

    /// Catches: a remote PTY without an owner route falling through to this machine's account.
    #[tokio::test]
    async fn remote_usage_never_falls_through_to_local_account() {
        let state = make_test_app_state();
        crate::remote_mirror::store_seed_for_test(
            &state,
            "offline-owner",
            vec![crate::mcp_http::types::SessionInfo {
                session_id: "remote-pty".into(),
                ..Default::default()
            }],
        );
        state.session_maps.session_states.insert(
            "remote-pty".into(),
            crate::state::SessionState {
                agent_type: Some("claude".into()),
                ..Default::default()
            },
        );
        let snapshot = get_session_usage_impl(&state, "remote-pty").await;
        assert_eq!(snapshot.availability, "unavailable");
        assert_eq!(snapshot.reason.as_deref(), Some("remote session"));
        assert!(snapshot.lines.is_empty());
    }

    /// Catches: launching Codex with this host's home or dropping the real --profile.
    #[test]
    fn codex_usage_binds_process_home_and_profile_and_rejects_unbound_overrides() {
        let home = crate::test_support::test_temp_root();
        let private = home.join("codex-private");
        let env = vec![
            format!("HOME={}", home.display()),
            format!("CODEX_HOME={}", private.display()),
        ];
        let args = vec!["codex".into(), "--profile".into(), "work".into()];
        let (actual_home, actual_codex_home, profile) = codex_profile(&env, &args).unwrap();
        assert_eq!(actual_home, home.to_string_lossy());
        assert_eq!(actual_codex_home, private.to_string_lossy());
        assert_eq!(profile.as_deref(), Some("work"));
        assert!(codex_profile(&[], &args).is_err());
        assert!(
            codex_profile(
                &env,
                &["codex".into(), "-c".into(), "model_provider=other".into()]
            )
            .is_err()
        );
    }

    /// Catches: an empty provider DTO turning into fabricated 0% quota.
    #[test]
    fn empty_quota_is_unavailable_not_zero() {
        let snapshot = claude_quota(crate::claude_usage::UsageApiResponse::default());
        assert_eq!(snapshot.availability, "unavailable");
        assert!(snapshot.lines.is_empty());
    }
}
