//! Session-owned usage. Quota fields are formatted by the shared client presenter. Never infer account ownership from a workspace.
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
pub(crate) struct QuotaWindow {
    pub provider: String,
    pub plan: Option<String>,
    pub window_seconds: Option<i64>,
    pub window_name: String,
    pub used_percent: f64,
    pub resets_at: Option<String>,
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
    #[serde(default)]
    pub quota_windows: Vec<QuotaWindow>,
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
            quota_windows: Vec::new(),
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
        if self.lines.is_empty() && self.quota_windows.is_empty() {
            Self::unavailable("no reported usage")
        } else {
            self
        }
    }
}

#[derive(Clone)]
enum UsageRead {
    Ego { report: Arc<Value>, stale: bool },
}

impl UsageRead {
    fn mark_stale(&mut self) {
        match self {
            Self::Ego { stale, .. } => *stale = true,
        }
    }
}

type Cached = Arc<tokio::sync::Mutex<Option<(UsageRead, Instant)>>>;
static CACHE: LazyLock<parking_lot::Mutex<HashMap<String, Cached>>> =
    LazyLock::new(|| parking_lot::Mutex::new(HashMap::new()));
static READ_SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

/// At most 32 retained snapshots, one fetch per identity, and two CLI readers at once.
/// The public request deadline also bounds time spent queued behind another reader.
async fn cached_read(
    key: String,
    ttl: Duration,
    fetch: impl std::future::Future<Output = Result<UsageRead, String>>,
) -> Result<UsageRead, String> {
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
                snapshot.mark_stale();
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
    Ok(Box::pin(get_session_usage_impl(&state, &session_id)).await)
}

pub(crate) async fn get_session_usage_impl(state: &AppState, session_id: &str) -> SessionUsage {
    // The owner check precedes every local lookup, cache read and provider operation.
    if let Some(owner) = crate::remote_mirror::owner_connection(state, session_id) {
        return remote_usage(state, &owner, session_id)
            .await
            .unwrap_or_else(|| SessionUsage::unavailable("remote session"));
    }
    match Box::pin(tokio::time::timeout(
        Duration::from_secs(30),
        local_usage(state, session_id),
    ))
    .await
    {
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
    if agent == "claude" || agent == "codex" {
        let Some(key) = state.provider_quota.binding(session_id) else {
            return Ok(SessionUsage::unavailable("quota binding unavailable"));
        };
        let view = state.provider_quota.get(&key);
        let mut usage = SessionUsage::available(&agent, "account_quota", "provider_quota_registry");
        usage.stale = view.state != crate::provider_quota::QuotaState::Fresh;
        let Some(snapshot) = view.snapshot else {
            return Ok(SessionUsage::unavailable("quota source unavailable"));
        };
        usage.observed_at_ms = Some(snapshot.observed_at_ms);
        usage.quota_windows = snapshot
            .windows
            .into_iter()
            .map(|window| QuotaWindow {
                provider: key.provider.clone(),
                plan: snapshot.plan.clone(),
                window_seconds: window.window_seconds,
                window_name: window
                    .model
                    .map(|model| format!("{model} {}", window.label))
                    .unwrap_or(window.label),
                used_percent: window.used_percent,
                resets_at: window.resets_at,
            })
            .collect();
        return Ok(usage.finish());
    }
    let Some(_pid) = crate::pty::session_leaf_pid(state, session_id) else {
        return Ok(SessionUsage::unavailable("session binding unavailable"));
    };
    match agent.as_str() {
        "ego" => {
            let Some(source) = ego_terminal_source(state, session_id).await? else {
                return Ok(SessionUsage::unavailable("ego binding unavailable"));
            };
            let (report, stale) = source.read(state, session_id).await?;
            let mut snapshot = ego_snapshot(&report, &source.ego_session)?;
            snapshot.stale = stale;
            Ok(snapshot)
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

pub(crate) fn codex_profile(
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

#[cfg(test)]
fn quota_window(
    snapshot: &mut SessionUsage,
    provider: &str,
    plan: Option<&str>,
    name: &str,
    seconds: Option<i64>,
    percent: f64,
    reset: Option<String>,
) {
    if !percent.is_finite() || !(0.0..=100.0).contains(&percent) {
        return;
    }
    snapshot.quota_windows.push(QuotaWindow {
        provider: provider.into(),
        plan: plan.map(str::to_owned),
        window_name: name.into(),
        window_seconds: seconds,
        used_percent: percent,
        resets_at: reset,
    });
}

#[cfg(test)]
fn claude_quota(quota: crate::claude_usage::UsageApiResponse) -> SessionUsage {
    let mut snapshot = SessionUsage::available("claude", "account_quota", "claude_oauth_usage");
    let plan = quota
        .plan
        .as_ref()
        .and_then(|plan| plan.subscription_type.as_deref());
    for (name, seconds, bucket) in [
        ("primary", 18000, quota.five_hour),
        ("secondary", 604800, quota.seven_day),
    ] {
        if let Some(bucket) = bucket {
            quota_window(
                &mut snapshot,
                "Claude",
                plan,
                name,
                Some(seconds),
                bucket.utilization,
                bucket.resets_at,
            );
        }
    }
    snapshot.finish()
}

/// Shared terminal identity and acquisition for session details and the badge.
#[derive(PartialEq, Eq)]
pub(crate) struct EgoTerminalSource {
    pid: u32,
    home: PathBuf,
    pub(crate) ego_session: String,
    workspace: String,
    program: PathBuf,
}

pub(crate) async fn ego_terminal_source(
    state: &AppState,
    terminal: &str,
) -> Result<Option<EgoTerminalSource>, String> {
    if crate::remote_mirror::owner_connection(state, terminal).is_some()
        || state
            .session_maps
            .session_states
            .get(terminal)
            .and_then(|session| session.agent_type.clone())
            .as_deref()
            != Some("ego")
    {
        return Ok(None);
    }
    let Some(pid) = crate::pty::session_leaf_pid(state, terminal) else {
        return Ok(None);
    };
    let argv = crate::process_env::read_process_argv(pid).ok_or("ego binding unavailable")?;
    // The existing exact resolver knows EGO_HOME, not an explicit CLI store override.
    if argv
        .iter()
        .any(|arg| arg == "--session-dir" || arg.starts_with("--session-dir="))
    {
        return Ok(None);
    }
    let (home, ego_session) =
        crate::agent_session::ego_chat_session_for_pid(pid).ok_or("ego binding unavailable")?;
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
        .ok_or("ego workspace unavailable")?
        .to_owned();
    // PTY usage follows the terminal executable, not AI Chat's ACP setting.
    let program = match argv.first().map(PathBuf::from) {
        Some(path) if path.is_absolute() && path.is_file() => path,
        _ => crate::agent::detect_agent_binary("ego".into())
            .await
            .path
            .map(PathBuf::from)
            .ok_or("ego executable unavailable")?,
    };
    if !home.is_absolute() || !program.is_absolute() || !Path::new(&workspace).is_absolute() {
        return Ok(None);
    }
    Ok(Some(EgoTerminalSource {
        pid,
        home,
        ego_session,
        workspace,
        program,
    }))
}

impl EgoTerminalSource {
    pub(crate) async fn read(
        &self,
        state: &AppState,
        terminal: &str,
    ) -> Result<(Arc<Value>, bool), String> {
        // Both consumers share the report, not either consumer's projection.
        // The deadline includes the identity lock and the global reader queue.
        let read = async {
            let key = format!(
                "ego:{:?}:{:?}:{:?}:{}",
                self.program, self.home, self.workspace, self.ego_session
            );
            let snapshot = cached_read(key, Duration::from_secs(30), async {
                let report = ego_report(
                    &self.program,
                    &self.home,
                    &self.workspace,
                    &self.ego_session,
                )
                .await?;
                crate::provider_quota::observe_ego_usage_report(
                    &state.provider_quota,
                    &self.home,
                    &report,
                );
                Ok(UsageRead::Ego {
                    report: Arc::new(report),
                    stale: false,
                })
            })
            .await?;
            // Even a cache hit or stale fallback must still belong to this live terminal.
            if ego_terminal_source(state, terminal).await?.as_ref() != Some(self) {
                return Err("ego binding changed".into());
            }
            match snapshot {
                UsageRead::Ego { report, stale } => Ok((report, stale)),
            }
        };
        tokio::time::timeout(Duration::from_secs(30), read)
            .await
            .map_err(|_| "ego usage timed out")?
    }
}

/// Bound stdout before decoding: a large workspace is not an unbounded memory allocation.
async fn ego_report(
    program: &Path,
    home: &Path,
    workspace: &str,
    session: &str,
) -> Result<Value, String> {
    const MAX_REPORT: u64 = 4 * 1024 * 1024;
    let mut command = std::process::Command::new(program);
    command
        .args([
            "usage",
            "--json",
            "--session",
            session,
            "-C",
            workspace,
            "--session-dir",
        ])
        .arg(home.join("sessions"))
        .env("EGO_HOME", home)
        .env_remove("EGO_PROFILE")
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
    // Leave headroom for binding checks inside the public thirty-second request deadline.
    let result = tokio::time::timeout(Duration::from_secs(25), read).await;
    let _ = child.kill().await;
    let _ = child.wait().await;
    result.map_err(|_| "ego usage timed out")??;
    serde_json::from_slice(&bytes).map_err(|_| "Failed to parse ego usage".into())
}

fn ego_snapshot(report: &Value, session_id: &str) -> Result<SessionUsage, String> {
    crate::ego_cli::usage::validate_report(report)?;
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
    let source = format!("ego_usage_v{}", report["v"]);
    let mut snapshot = SessionUsage::available("ego", "session_usage", &source);
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

    /// Catches mobile details issuing a provider read instead of showing the registry.
    #[tokio::test]
    async fn session_quota_uses_registry_without_a_live_cli() {
        let state = make_test_app_state();
        let key = crate::provider_quota::SubscriptionKey::new(
            "codex-cli",
            "openai-codex",
            "/not-installed",
        );
        state.session_maps.session_states.insert(
            "quota-only".into(),
            crate::state::SessionState {
                agent_type: Some("codex".into()),
                ..Default::default()
            },
        );
        state.provider_quota.bind("quota-only", key.clone());
        state.provider_quota.observe(
            &key,
            crate::provider_quota::QuotaSnapshot {
                windows: vec![
                    crate::provider_quota::QuotaWindow::from_duration(
                        37.0,
                        Some(18000),
                        None,
                        None,
                    )
                    .unwrap(),
                ],
                detail: None,
                plan: Some("pro".into()),
                credits: None,
                account: None,
                observed_at_ms: 1,
                source: crate::provider_quota::QuotaSource::Passive,
            },
        );
        let result = get_session_usage_impl(&state, "quota-only").await;
        assert_eq!(result.availability, "available");
        assert_eq!(result.source.as_deref(), Some("provider_quota_registry"));
        assert_eq!(result.quota_windows[0].used_percent, 37.0);
    }

    /// Catches: a live terminal ego ledger being hidden by empty ACP configuration.
    #[cfg(unix)]
    #[tokio::test]
    async fn terminal_ego_usage_does_not_require_acp_executable() {
        Box::pin(terminal_ego_usage_probe(false)).await;
    }

    /// Catches: badge polls bypassing the shared cache and reader limit.
    #[cfg(unix)]
    #[tokio::test]
    async fn ego_badge_shares_cached_bounded_usage_reader() {
        Box::pin(terminal_ego_usage_probe(true)).await;
    }

    #[cfg(unix)]
    async fn terminal_ego_usage_probe(check_cache: bool) {
        use portable_pty::{CommandBuilder, PtySize, native_pty_system};
        use std::sync::atomic::AtomicBool;

        // The same native binary serves as a foreground fixture. System shells
        // on macOS can expose argv without their initial environment, so a shell
        // blocked on read cannot prove the ego store binding.
        if std::env::var_os("TUIC_EGO_USAGE_PROBE").is_some() {
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).unwrap();
            return;
        }

        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let script = crate::test_support::fake_ssh_script(
            "2150-terminal-ego-usage",
            r#"[ "$1" = usage ] && [ "$2" = --json ] && [ "$3" = --session ] && [ "$4" = 01a02491-6a90-7ae2-9089-fd6a8532eb82 ] || exit 2
printf "read\n" >> "$EGO_HOME/reads"
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
        // Reuse the compiled test binary: no new executable or scan-time budget.
        let mut command = CommandBuilder::new("/bin/bash");
        command.args([
            "-c",
            "exec -a \"$1\" \"$2\" --exact session_usage::tests::terminal_ego_usage_does_not_require_acp_executable --nocapture",
            "probe",
        ]);
        command.arg(&binary);
        command.arg(std::env::current_exe().unwrap());
        command.env("TUIC_EGO_USAGE_PROBE", "1");
        command.env("EGO_HOME", dir.path());
        let mut child = pair.slave.spawn_command(command).unwrap();
        // The parent must not retain the slave: macOS can keep an exiting
        // controlling-terminal process in teardown until that handle closes.
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().unwrap();
        let output = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Err(error) = std::io::Read::read_to_end(&mut reader, &mut bytes) {
                assert_eq!(error.raw_os_error(), Some(libc::EIO), "{error}");
            }
            String::from_utf8(bytes).unwrap()
        });
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
        let reader_limit_held = if check_cache {
            let permits = READ_SLOTS.acquire_many(2).await.unwrap();
            let result = tokio::time::timeout(
                Duration::from_millis(100),
                crate::ego_cli::usage::read(&state, "terminal-ego"),
            )
            .await;
            drop(permits);
            result.is_err() && !dir.path().join("reads").exists()
        } else {
            true
        };
        let (concurrent_badge, snapshot) = tokio::join!(
            crate::ego_cli::usage::read(&state, "terminal-ego"),
            get_session_usage_impl(&state, "terminal-ego"),
        );
        let badge = crate::ego_cli::usage::read(&state, "terminal-ego").await;
        state.config.write().ego_executable = "/unrelated/acp/ego".into();
        let with_other_acp = Box::pin(get_session_usage_impl(&state, "terminal-ego")).await;
        let badge_with_other_acp = crate::ego_cli::usage::read(&state, "terminal-ego").await;
        let reads = std::fs::read_to_string(dir.path().join("reads")).unwrap();
        // The badge now reads model/context only; details retain accounting totals.
        let recorded: Value =
            serde_json::from_str(include_str!("fixtures/ego-usage-v3.json")).unwrap();
        let mut mixed = recorded["sessions"][1].clone();
        mixed["session"] = serde_json::json!(id);
        mixed["context"] = Value::Null;
        let mixed_report = serde_json::json!({"v": 3, "sessions": [mixed]});
        std::fs::write(dir.path().join("report.json"), mixed_report.to_string()).unwrap();
        expire_terminal_ego_cache(&state).await;
        let mixed_badge = crate::ego_cli::usage::read(&state, "terminal-ego").await;
        // Catches: provider limits in v4 JSON ignored, so the badge has no remaining limit.
        let mut v4: Value =
            serde_json::from_str(include_str!("fixtures/ego-usage-v4.json")).unwrap();
        v4["sessions"][0]["session"] = serde_json::json!(id);
        std::fs::write(dir.path().join("report.json"), v4.to_string()).unwrap();
        expire_terminal_ego_cache(&state).await;
        let v4_badge = crate::ego_cli::usage::read(&state, "terminal-ego").await;
        let v4_snapshot = ego_snapshot(&v4, id);
        let reads_after_expiration = std::fs::read_to_string(dir.path().join("reads")).unwrap();
        let mut failure_checks = None;
        if check_cache {
            // Catches: transient failure discarding the last good report, or
            // malformed reports silently reusing a previously priced result.
            expire_terminal_ego_cache(&state).await;
            std::fs::remove_file(dir.path().join("report.json")).unwrap();
            let stale = Box::pin(get_session_usage_impl(&state, "terminal-ego")).await;
            let stale_badge = crate::ego_cli::usage::read(&state, "terminal-ego").await;
            std::fs::write(dir.path().join("report.json"), "not json").unwrap();
            let malformed = crate::ego_cli::usage::read(&state, "terminal-ego").await;
            // Catches: distinct source workspaces sharing a cached report.
            let other_workspace = dir.path().join("other");
            std::fs::create_dir(&other_workspace).unwrap();
            std::fs::write(
                session_dir.join("manifest.json"),
                serde_json::json!({"workspace_root": other_workspace}).to_string(),
            )
            .unwrap();
            std::fs::write(dir.path().join("report.json"), mixed_report.to_string()).unwrap();
            let other = crate::ego_cli::usage::read(&state, "terminal-ego").await;
            // Catches: cached usage surviving withdrawal of the live ego identity.
            state
                .session_maps
                .session_states
                .get_mut("terminal-ego")
                .unwrap()
                .agent_type = None;
            let withdrawn = crate::ego_cli::usage::read(&state, "terminal-ego").await;
            failure_checks = Some((stale, stale_badge, malformed, other, withdrawn));
        }
        let source_error = if snapshot.availability != "available" {
            Some(format!(
                "local={:?}; pid={:?}/{pid}; argv={:?}; home={:?}; binding={:?}",
                Box::pin(local_usage(&state, "terminal-ego")).await.err(),
                crate::pty::session_leaf_pid(&state, "terminal-ego"),
                crate::process_env::read_process_argv(pid),
                crate::process_env::read_process_env_var(pid, "EGO_HOME"),
                crate::agent_session::ego_chat_session_for_pid(pid),
            ))
        } else {
            None
        };
        let (_, session) = state.session_maps.sessions.remove("terminal-ego").unwrap();
        let mut session = session.into_inner();
        // Let the native helper finish its read and exit normally. A PTY hangup
        // is not the helper protocol and can leave native runtime teardown waiting.
        std::io::Write::write_all(&mut *session.writer.lock(), b"done\n").unwrap();
        session._child.wait().unwrap();
        drop(session);
        let output = output.join().unwrap();
        assert!(output.contains("test result: ok"), "{output}");
        if check_cache {
            assert_eq!(
                reads.lines().count(),
                1,
                "badge and details must share one CLI read"
            );
            assert!(reader_limit_held, "badge spawned a CLI outside READ_SLOTS");
            assert_eq!(
                reads_after_expiration.lines().count(),
                3,
                "expired reports must refresh once"
            );
            let (stale, stale_badge, malformed, other, withdrawn) = failure_checks.unwrap();
            assert!(stale.stale);
            assert_eq!(stale.availability, "available");
            assert_eq!(stale_badge.unwrap().text, v4_badge.as_ref().unwrap().text);
            assert!(
                malformed.is_err(),
                "contract failures must not serve cached prices"
            );
            assert_eq!(other.unwrap().text.as_deref(), Some("gpt-6.1-sol"));
            assert!(withdrawn.unwrap().text.is_none());
            assert_eq!(concurrent_badge.unwrap().text.as_deref(), Some("model"));
        }
        // Catches: the badge taking its executable from unrelated ACP settings.
        for badge in [badge, badge_with_other_acp] {
            assert_eq!(badge.unwrap().text.as_deref(), Some("model"));
        }
        assert_eq!(mixed_badge.unwrap().text.as_deref(), Some("gpt-6.1-sol"));
        let v4_badge = v4_badge.unwrap();
        assert!(
            v4_badge
                .tooltip
                .as_deref()
                .unwrap()
                .contains("openrouter/openai/gpt-4o")
        );
        assert_eq!(v4_snapshot.unwrap().source.as_deref(), Some("ego_usage_v4"));
        assert_eq!(
            v4_badge.text.as_deref(),
            Some("openai/gpt-4o · ctx: 14/128000")
        );
        for snapshot in [snapshot, with_other_acp] {
            assert_eq!(
                snapshot.availability, "available",
                "{:?}; source error: {source_error:?}",
                snapshot.reason
            );
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

    #[cfg(unix)]
    async fn expire_terminal_ego_cache(state: &AppState) {
        let source = ego_terminal_source(state, "terminal-ego")
            .await
            .unwrap()
            .unwrap();
        let key = format!(
            "ego:{:?}:{:?}:{:?}:{}",
            source.program, source.home, source.workspace, source.ego_session
        );
        let entry = CACHE.lock().get(&key).cloned();
        if let Some(entry) = entry {
            let mut guard = entry.lock().await;
            if let Some((_, at)) = &mut *guard {
                *at -= Duration::from_secs(31);
            }
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

    // Catches: accepting v5 in the badge but rejecting the shared session-details consumer.
    #[test]
    fn ego_usage_v5_session_details_keep_model_and_accounting() {
        let report: Value =
            serde_json::from_str(include_str!("fixtures/ego-usage-v5.json")).unwrap();
        let snapshot = ego_snapshot(&report, "01a1c787-d41f-7067-9cb5-db90deb9ca8d").unwrap();
        assert_eq!(snapshot.model.as_deref(), Some("openai-codex/gpt-6.1-sol"));
        assert_eq!(snapshot.source.as_deref(), Some("ego_usage_v5"));
        assert!(
            snapshot
                .lines
                .iter()
                .any(|line| line.value.contains("4491 output"))
        );
    }

    /// Catches: missing, duplicated or future-version rows falling back to account totals.
    #[test]
    fn ego_usage_rejects_missing_duplicate_and_unsupported_rows() {
        let mut report = report_with_sessions();
        assert!(ego_snapshot(&report, "absent").is_err());
        report["sessions"][0] = report["sessions"][1].clone();
        assert!(ego_snapshot(&report, "exact").is_err());
        report["v"] = serde_json::json!(999);
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
        let snapshot = Box::pin(get_session_usage_impl(&state, "remote-pty")).await;
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

    /// Catches: dropping plan/window identity or mixing timestamps into raw display lines.
    #[test]
    fn quota_snapshot_preserves_provider_plan_window_and_reset_instant() {
        let quota = crate::claude_usage::UsageApiResponse {
            five_hour: Some(crate::claude_usage::RateBucket {
                utilization: 2.0,
                resets_at: Some("2026-10-26T11:20:00.093350+00:00".into()),
            }),
            plan: Some(crate::claude_usage::PlanInfo {
                subscription_type: Some("max".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let value = serde_json::to_value(claude_quota(quota)).unwrap();
        assert_eq!(
            value["quota_windows"][0],
            serde_json::json!({
                "provider":"Claude", "plan":"max", "window_seconds":18000,
                "window_name":"primary", "used_percent":2.0,
                "resets_at":"2026-10-26T11:20:00.093350+00:00"
            })
        );
        assert_eq!(value["lines"], serde_json::json!([]));
    }

    /// Catches: an empty provider DTO turning into fabricated 0% quota.
    #[test]
    fn empty_quota_is_unavailable_not_zero() {
        let snapshot = claude_quota(crate::claude_usage::UsageApiResponse::default());
        assert_eq!(snapshot.availability, "unavailable");
        assert!(snapshot.lines.is_empty());
    }
}
