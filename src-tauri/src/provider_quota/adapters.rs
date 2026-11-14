//! Quota readers for CLI-owned subscriptions.
use super::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

pub(crate) fn register(registry: &QuotaRegistry) {
    registry.register_owner(Arc::new(ClaudeCode));
    registry.register_owner(Arc::new(CodexCli));
    registry.register_owner(Arc::new(super::ego::Ego::default()));
    registry.register_owner(Arc::new(super::pi::Pi));
}

fn now_ms() -> u64 {
    SystemClock.now_ms()
}
fn snapshot(
    windows: Vec<QuotaWindow>,
    plan: Option<String>,
    credits: Option<String>,
    source: QuotaSource,
    observed_at_ms: u64,
) -> QuotaSnapshot {
    QuotaSnapshot {
        windows,
        detail: None,
        plan,
        credits,
        account: None,
        observed_at_ms,
        source,
    }
}
fn string(v: &Value, key: &str) -> Option<String> {
    v.get(key)?.as_str().map(str::to_owned)
}
fn epoch(value: &Value) -> Option<String> {
    chrono::DateTime::from_timestamp(value.as_i64()?, 0).map(|d| d.to_rfc3339())
}
fn credential_file(key: &SubscriptionKey, name: &str) -> PathBuf {
    Path::new(&key.credential).join(name)
}
fn fingerprint(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    Some(hex::encode(Sha256::digest(bytes)))
}
pub(super) fn read_json(path: &Path) -> Result<Value, QuotaError> {
    let bytes = std::fs::read(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            QuotaError::Unavailable("login not found".into())
        } else {
            QuotaError::Auth("Login unreadable".into())
        }
    })?;
    serde_json::from_slice(&bytes).map_err(|_| QuotaError::Contract("Invalid login file".into()))
}
fn expired(ms: Option<i64>) -> bool {
    ms.is_some_and(|v| v > 0 && v as u64 <= now_ms())
}
pub(super) async fn get_json(
    url: &str,
    token: &str,
    headers: &[(&str, &str)],
) -> Result<Value, QuotaError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| QuotaError::Network(e.to_string()))?;
    let mut req = client.get(url).bearer_auth(token);
    for (key, value) in headers {
        req = req.header(*key, *value);
    }
    let response = req
        .send()
        .await
        .map_err(|e| QuotaError::Network(e.to_string()))?;
    let status = response.status();
    if status.as_u16() == 429 {
        let retry_after_secs = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok());
        return Err(QuotaError::RateLimited { retry_after_secs });
    }
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(QuotaError::Auth(format!("HTTP {status}")));
    }
    if !status.is_success() {
        return Err(QuotaError::Network(format!("HTTP {status}")));
    }
    response
        .json()
        .await
        .map_err(|e| QuotaError::Contract(e.to_string()))
}
fn push_window(
    windows: &mut Vec<QuotaWindow>,
    pct: Option<f64>,
    secs: Option<i64>,
    reset: Option<String>,
    model: Option<String>,
) {
    if let Some(window) = pct.and_then(|p| QuotaWindow::from_duration(p, secs, reset, model)) {
        windows.push(window);
    }
}

/// Newest windows relayed by the Claude status line, per credential.
static CLAUDE_STATUS_LINE: std::sync::LazyLock<
    parking_lot::Mutex<HashMap<SubscriptionKey, QuotaSnapshot>>,
> = std::sync::LazyLock::new(Default::default);

/// Store relayed windows; true when they differ from the stored ones. The
/// status line repeats one payload on every refresh, so an identical payload
/// keeps its first arrival time and ages like any other observation. Within one
/// window the percentage only rises until `resets_at` changes, so a lower
/// percentage for the same window is an idle tab's older reading and is ignored.
pub(super) fn record_claude_status_line(key: &SubscriptionKey, snapshot: QuotaSnapshot) -> bool {
    let mut relayed = CLAUDE_STATUS_LINE.lock();
    if relayed.get(key).is_some_and(|stored| {
        stored.windows == snapshot.windows
            || snapshot.windows.iter().any(|incoming| {
                stored.windows.iter().any(|kept| {
                    kept.kind == incoming.kind
                        && kept.model == incoming.model
                        && kept.resets_at == incoming.resets_at
                        && kept.used_percent > incoming.used_percent
                })
            })
    }) {
        return false;
    }
    relayed.insert(key.clone(), snapshot);
    true
}

#[cfg(test)]
pub(crate) fn claude_code() -> Arc<dyn QuotaOwner> {
    Arc::new(ClaudeCode)
}

struct ClaudeCode;
impl QuotaOwner for ClaudeCode {
    fn owner(&self) -> &'static str {
        "claude-code"
    }
    fn unavailable_reason(&self, key: &SubscriptionKey) -> Option<&'static str> {
        key.profile.as_ref().map(|_| "profile not supported")
    }
    fn observed(&self, key: &SubscriptionKey) -> Option<QuotaSnapshot> {
        CLAUDE_STATUS_LINE.lock().get(key).cloned()
    }
    fn credential_fingerprint(&self, key: &SubscriptionKey) -> Option<Fingerprint> {
        claude_credentials(key)
            .ok()
            .map(|s| hex::encode(Sha256::digest(s.as_bytes())))
    }
    fn read<'a>(
        &'a self,
        key: &'a SubscriptionKey,
    ) -> BoxFuture<'a, Result<QuotaSnapshot, QuotaError>> {
        Box::pin(async move {
            if key.profile.is_some() {
                return Err(QuotaError::Unavailable("profile not supported".into()));
            }
            let credentials = claude_credentials(key)?;
            claude_with_credentials(&credentials, |token| async move {
                let body = get_json(
                    "https://api.anthropic.com/api/oauth/usage",
                    &token,
                    &[("anthropic-beta", "oauth-2025-04-20")],
                )
                .await?;
                parse_claude_usage(&body, now_ms())
            })
            .await
        })
    }
}
pub(crate) fn parse_claude_usage(
    body: &Value,
    observed_at_ms: u64,
) -> Result<QuotaSnapshot, QuotaError> {
    let parsed = crate::claude_usage::parse_primary_usage(&body.to_string())
        .map_err(|e| QuotaError::Contract(e.to_string()))?;
    let detail = parsed.clone();
    let mut windows = Vec::new();
    for (bucket, secs, model) in [
        (parsed.five_hour, 18_000, None),
        (parsed.seven_day, 604_800, None),
        (parsed.seven_day_oauth_apps, 604_800, Some("oauth apps")),
        (parsed.seven_day_opus, 604_800, Some("opus")),
        (parsed.seven_day_sonnet, 604_800, Some("sonnet")),
        (parsed.seven_day_cowork, 604_800, Some("cowork")),
    ] {
        if let Some(b) = bucket {
            push_window(
                &mut windows,
                Some(b.utilization),
                Some(secs),
                b.resets_at,
                model.map(str::to_owned),
            );
        }
    }
    if windows.is_empty() {
        return Err(QuotaError::Contract("No quota windows".into()));
    }
    let mut value = snapshot(windows, None, None, QuotaSource::Active, observed_at_ms);
    value.detail = Some(QuotaDetail::Claude {
        observed_at_ms,
        response: detail,
    });
    Ok(value)
}
fn claude_credentials(key: &SubscriptionKey) -> Result<String, QuotaError> {
    let home = dirs::home_dir().ok_or_else(|| QuotaError::Auth("Home not found".into()))?;
    let default = home.join(".claude");
    let dir = if Path::new(&key.credential) == default {
        None
    } else {
        Some(key.credential.as_str())
    };
    crate::claude_usage::read_credential_json(dir, &home, crate::plugin_credentials::cached_read)
        .ok_or_else(|| QuotaError::Unavailable("login not found".into()))
}
async fn claude_with_credentials<F, Fut>(
    credentials: &str,
    fetch: F,
) -> Result<QuotaSnapshot, QuotaError>
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = Result<QuotaSnapshot, QuotaError>>,
{
    let value: Value = serde_json::from_str(credentials)
        .map_err(|_| QuotaError::Contract("Invalid Claude login".into()))?;
    let oauth = &value["claudeAiOauth"];
    if expired(oauth.get("expiresAt").and_then(Value::as_i64)) {
        return Err(QuotaError::Auth("Claude login expired".into()));
    }
    let token = string(oauth, "accessToken")
        .ok_or_else(|| QuotaError::Auth("Claude token missing".into()))?;
    let mut result = fetch(token).await?;
    result.plan = string(oauth, "subscriptionType");
    if let Some(QuotaDetail::Claude { response, .. }) = &mut result.detail {
        response.plan = Some(crate::claude_usage::PlanInfo {
            subscription_type: result.plan.clone(),
            rate_limit_tier: string(oauth, "rateLimitTier"),
            scopes: oauth["scopes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect(),
        });
    }
    Ok(result)
}

struct CodexCli;
impl QuotaOwner for CodexCli {
    fn owner(&self) -> &'static str {
        "codex-cli"
    }
    fn unavailable_reason(&self, key: &SubscriptionKey) -> Option<&'static str> {
        if key.profile.is_some() {
            Some("profile not supported")
        } else if !credential_file(key, "auth.json").is_file() {
            Some("login not found")
        } else {
            None
        }
    }
    fn credential_fingerprint(&self, key: &SubscriptionKey) -> Option<Fingerprint> {
        fingerprint(&credential_file(key, "auth.json"))
    }
    fn observed(&self, key: &SubscriptionKey) -> Option<QuotaSnapshot> {
        if key.profile.is_some() || !credential_file(key, "auth.json").is_file() {
            return None;
        }
        newest_rollout(&credential_file(key, "sessions"))
    }
    fn read<'a>(
        &'a self,
        key: &'a SubscriptionKey,
    ) -> BoxFuture<'a, Result<QuotaSnapshot, QuotaError>> {
        Box::pin(async move {
            if key.profile.is_some() {
                return Err(QuotaError::Unavailable("profile not supported".into()));
            }
            let auth = read_json(&credential_file(key, "auth.json"))?;
            let tokens = &auth["tokens"];
            let token = string(tokens, "access_token")
                .ok_or_else(|| QuotaError::Auth("Codex token missing".into()))?;
            let account = string(tokens, "account_id")
                .ok_or_else(|| QuotaError::Auth("Codex account missing".into()))?;
            let body = get_json(
                "https://chatgpt.com/backend-api/wham/usage",
                &token,
                &[("ChatGPT-Account-Id", &account)],
            )
            .await?;
            parse_codex_usage(&body, now_ms())
        })
    }
}
pub(super) fn parse_codex_usage(
    body: &Value,
    observed_at_ms: u64,
) -> Result<QuotaSnapshot, QuotaError> {
    let mut windows = Vec::new();
    codex_pair(&mut windows, &body["rate_limit"], None, false);
    if let Some(extra) = body["additional_rate_limits"].as_array() {
        for item in extra {
            codex_pair(
                &mut windows,
                &item["rate_limit"],
                string(item, "limit_name"),
                false,
            );
        }
    }
    codex_pair(
        &mut windows,
        &body["code_review_rate_limit"],
        Some("code review".into()),
        false,
    );
    if windows.is_empty() {
        return Err(QuotaError::Contract("No Codex quota windows".into()));
    }
    let mut normalized = body.clone();
    if normalized["additional_rate_limits"].is_null() {
        normalized["additional_rate_limits"] = serde_json::json!([]);
    }
    let response: crate::codex_usage::CodexUsageApiResponse =
        serde_json::from_value(normalized).map_err(|e| QuotaError::Contract(e.to_string()))?;
    let mut value = snapshot(
        windows,
        string(body, "plan_type"),
        string(&body["credits"], "balance"),
        QuotaSource::Active,
        observed_at_ms,
    );
    value.detail = Some(QuotaDetail::Codex {
        observed_at_ms,
        response,
    });
    Ok(value)
}
fn codex_pair(windows: &mut Vec<QuotaWindow>, pair: &Value, model: Option<String>, passive: bool) {
    for name in if passive {
        ["primary", "secondary"]
    } else {
        ["primary_window", "secondary_window"]
    } {
        let v = &pair[name];
        let secs = if passive {
            v.get("window_minutes")
                .and_then(Value::as_i64)
                .map(|v| v * 60)
        } else {
            v.get("limit_window_seconds").and_then(Value::as_i64)
        };
        let reset = epoch(&v[if passive { "resets_at" } else { "reset_at" }]);
        push_window(
            windows,
            v.get("used_percent").and_then(Value::as_f64),
            secs,
            reset,
            model.clone(),
        );
    }
}
fn newest_rollout(root: &Path) -> Option<QuotaSnapshot> {
    let mut dirs = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_type().ok().is_some_and(|v| v.is_dir()) {
                dirs.push(path);
                continue;
            }
            if path.extension().is_none_or(|v| v != "jsonl") {
                continue;
            }
            let Some(modified) = entry.metadata().ok().and_then(|m| m.modified().ok()) else {
                continue;
            };
            files.push((modified, path));
        }
    }
    // Bound the amount of rollout content read in one scheduler pass.
    files.sort_unstable_by_key(|entry| std::cmp::Reverse(entry.0));
    for (_, path) in files.into_iter().take(32) {
        if let Some(snapshot) = rollout_snapshot(&path) {
            return Some(snapshot);
        }
    }
    None
}

fn rollout_snapshot(path: &Path) -> Option<QuotaSnapshot> {
    let file = std::fs::File::open(path).ok()?;
    let mut newest: Option<QuotaSnapshot> = None;
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if value["type"] != "event_msg" || value["payload"]["type"] != "token_count" {
            continue;
        }
        let Some(ts) = value["timestamp"]
            .as_str()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .and_then(|d| u64::try_from(d.timestamp_millis()).ok())
        else {
            continue;
        };
        if newest.as_ref().is_some_and(|s| s.observed_at_ms >= ts) {
            continue;
        }
        let limits = &value["payload"]["rate_limits"];
        let mut windows = Vec::new();
        codex_pair(&mut windows, limits, None, true);
        if windows.is_empty() {
            continue;
        }
        newest = Some(snapshot(
            windows,
            string(limits, "plan_type"),
            string(&limits["credits"], "balance"),
            QuotaSource::Passive,
            ts,
        ));
    }
    newest
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // Catches the direct Claude response losing its weekly utilization.
    // Catches: a missing CLI hides a recorded Codex quota or a missing login
    // selects another account. The subprocess gets a real empty PATH.
    #[cfg(unix)]
    #[test]
    fn absent_cli_still_reads_recorded_quota_and_missing_login_is_unavailable() {
        const CHILD: &str = "TUIC_QUOTA_NO_CLI_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let dir = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "provider_quota::adapters::tests::absent_cli_still_reads_recorded_quota_and_missing_login_is_unavailable", "--nocapture"])
                .env("PATH", dir.path()).env(CHILD, "1").output().unwrap();
            assert!(
                output.status.success(),
                "child failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let error = std::process::Command::new("codex").output().unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        let home = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
        std::fs::create_dir_all(home.path().join("sessions/2026/11/03")).unwrap();
        std::fs::write(home.path().join("auth.json"), "{}").unwrap();
        std::fs::write(
            home.path().join("sessions/2026/11/03/rollout.jsonl"),
            include_str!("fixtures/codex-token-count.jsonl").replace(
                "2026-11-03T13:08:43.254Z",
                &chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            ),
        )
        .unwrap();
        let key = SubscriptionKey::new("codex-cli", "openai-codex", home.path().to_string_lossy());
        assert!(
            CodexCli.credential_fingerprint(&key).is_some(),
            "login file was not read"
        );
        assert!(
            CodexCli.observed(&key).is_some(),
            "recorded quota requires no CLI"
        );
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let registry = QuotaRegistry::default();
        registry.register_owner(Arc::new(CodexCli));
        assert!(runtime.block_on(registry.ensure(&key)).snapshot.is_some());
        std::fs::remove_file(home.path().join("auth.json")).unwrap();
        let view = runtime.block_on(registry.ensure(&key));
        assert!(
            matches!(view.state, QuotaState::Unavailable { .. }),
            "{view:?}"
        );
    }

    // Catches: a named profile reads the default profile's passive or active account.
    #[tokio::test]
    async fn named_profile_is_unavailable_without_reading_default_account() {
        let dir = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
        std::fs::create_dir_all(dir.path().join("sessions/2026/11/03")).unwrap();
        std::fs::write(
            dir.path().join("sessions/2026/11/03/rollout.jsonl"),
            include_str!("fixtures/codex-token-count.jsonl"),
        )
        .unwrap();
        let mut key =
            SubscriptionKey::new("codex-cli", "openai-codex", dir.path().to_string_lossy());
        key.profile = Some("work".into());
        let registry = QuotaRegistry::default();
        register(&registry);
        let view = registry.ensure(&key).await;
        assert!(view.snapshot.is_none());
        assert!(
            matches!(view.state, QuotaState::Unavailable { ref reason } if reason == "profile not supported"),
            "{view:?}"
        );
    }

    // Catches: unsupported Claude profiles falling back to the default login.
    #[tokio::test]
    async fn claude_named_profile_does_not_read_default_account() {
        let dir = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
        let mut key =
            SubscriptionKey::new("claude-code", "anthropic", dir.path().to_string_lossy());
        key.profile = Some("work".into());
        let registry = QuotaRegistry::default();
        register(&registry);
        let view = registry.ensure(&key).await;
        assert!(view.snapshot.is_none());
        assert!(
            matches!(view.state, QuotaState::Unavailable { ref reason } if reason == "profile not supported"),
            "{view:?}"
        );
    }

    #[test]
    fn recorded_claude_usage_preserves_weekly_window() {
        let body: Value =
            serde_json::from_str(include_str!("fixtures/anthropic-usage.json")).unwrap();
        let value = parse_claude_usage(&body, 1).unwrap();
        assert_eq!(value.windows.len(), 1);
        assert_eq!(value.windows[0].used_percent, 82.0);
        assert_eq!(value.windows[0].window_seconds, Some(604800));
    }

    // Catches a direct response being parsed as the App Server wire shape.
    #[test]
    fn recorded_codex_usage_preserves_weekly_window() {
        let body: Value =
            serde_json::from_str(include_str!("fixtures/openai-codex-usage.json")).unwrap();
        let value = parse_codex_usage(&body, 1).unwrap();
        assert_eq!(value.plan.as_deref(), Some("pro"));
        assert_eq!(value.windows.len(), 1);
        assert_eq!(value.windows[0].used_percent, 14.0);
        assert_eq!(value.windows[0].window_seconds, Some(604800));
    }

    #[test]
    fn codex_rollout_newest_token_count_wins() {
        let root = tuic_test_support::test_temp_root();
        let temp = tempfile::tempdir_in(root).unwrap();
        let home = temp.path().join("selected");
        let other = temp.path().join("other");
        std::fs::create_dir_all(home.join("sessions/2026/11/03")).unwrap();
        std::fs::create_dir_all(other.join("sessions/2026/11/03")).unwrap();
        std::fs::write(home.join("auth.json"), "{}").unwrap();
        let captured = include_str!("fixtures/codex-token-count.jsonl");
        let newer = captured
            .replace("2026-11-03T13:08:43.254Z", "2026-11-03T13:09:43.254Z")
            .replace("59.0", "42.0");
        let newest_other = captured
            .replace("2026-11-03T13:08:43.254Z", "2026-11-03T13:10:43.254Z")
            .replace("59.0", "99.0");
        std::fs::write(home.join("sessions/2026/11/03/older.jsonl"), captured).unwrap();
        std::fs::write(home.join("sessions/2026/11/03/newer.jsonl"), newer).unwrap();
        let older = std::fs::File::options()
            .write(true)
            .open(home.join("sessions/2026/11/03/older.jsonl"))
            .unwrap();
        older
            .set_times(
                std::fs::FileTimes::new()
                    .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1)),
            )
            .unwrap();
        std::fs::write(other.join("sessions/2026/11/03/other.jsonl"), newest_other).unwrap();
        let key = SubscriptionKey::new("codex-cli", "openai-codex", home.to_string_lossy());
        let snapshot = CodexCli.observed(&key).expect("latest recorded event");
        assert_eq!(snapshot.windows[0].used_percent, 42.0);
    }

    // Catches a newer empty session hiding a valid older quota event.
    #[test]
    fn codex_newer_rollout_without_quota_keeps_older_observation() {
        let temp = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
        // Passive observations still require this credential scope to exist.
        std::fs::write(temp.path().join("auth.json"), "{}").unwrap();
        let sessions = temp.path().join("sessions");
        std::fs::create_dir_all(&sessions).unwrap();
        let older = sessions.join("older.jsonl");
        std::fs::write(&older, include_str!("fixtures/codex-token-count.jsonl")).unwrap();
        std::fs::File::options()
            .write(true)
            .open(&older)
            .unwrap()
            .set_times(
                std::fs::FileTimes::new()
                    .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1)),
            )
            .unwrap();
        std::fs::write(sessions.join("newer.jsonl"), "{}\n").unwrap();
        let key = SubscriptionKey::new("codex-cli", "openai-codex", temp.path().to_string_lossy());
        let observed = CodexCli
            .observed(&key)
            .expect("older recorded quota remains available");
        assert_eq!(observed.windows[0].used_percent, 59.0);
    }

    #[tokio::test]
    async fn expired_token_is_reported_without_a_request() {
        let calls = AtomicUsize::new(0);
        let credentials = r#"{"claudeAiOauth":{"accessToken":"fixture-token","expiresAt":1}}"#;
        let result = claude_with_credentials(credentials, |_| async {
            calls.fetch_add(1, Ordering::SeqCst);
            Err(QuotaError::Network("request made".into()))
        })
        .await;
        assert!(matches!(result, Err(QuotaError::Auth(_))));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}
