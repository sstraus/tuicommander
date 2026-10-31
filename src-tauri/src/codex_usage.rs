//! Codex account usage through the documented Codex App Server JSON-RPC API.
//!
//! The CLI owns authentication, token refresh and upstream schema translation.
//! TUICommander never reads `~/.codex/auth.json` and never calls ChatGPT's
//! private backend routes directly.

use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::cli_usage_rpc::{UsageCache, looks_like_auth_failure, looks_like_contract_failure};

// ---------------------------------------------------------------------------
// Stable TUIC transport types (mapped from the App Server response)
// ---------------------------------------------------------------------------

/// One rate-limit window (session or weekly).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexRateWindow {
    /// Direct percentage, 0-100.
    pub used_percent: f64,
    /// Window length in seconds (18000 = 5h, 604800 = 7d).
    pub limit_window_seconds: Option<i64>,
    /// Seconds until the window resets — avoids trusting the local clock.
    pub reset_after_seconds: Option<i64>,
    /// Unix epoch seconds of the reset.
    pub reset_at: Option<i64>,
}

/// Primary/secondary window pair plus the reached flags.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexRateLimit {
    pub allowed: Option<bool>,
    #[serde(default)]
    pub limit_reached: bool,
    pub primary_window: Option<CodexRateWindow>,
    pub secondary_window: Option<CodexRateWindow>,
}

// ---------------------------------------------------------------------------
// Codex App Server wire types (camelCase, deliberately private)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppRateWindow {
    used_percent: f64,
    window_duration_mins: Option<i64>,
    resets_at: Option<i64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppCredits {
    #[serde(default)]
    has_credits: bool,
    #[serde(default)]
    unlimited: bool,
    balance: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppRateLimitSnapshot {
    limit_id: Option<String>,
    limit_name: Option<String>,
    plan_type: Option<String>,
    primary: Option<AppRateWindow>,
    secondary: Option<AppRateWindow>,
    rate_limit_reached_type: Option<String>,
    spend_control_reached: Option<bool>,
    credits: Option<AppCredits>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppRateLimitsResponse {
    rate_limits: AppRateLimitSnapshot,
    rate_limits_by_limit_id: Option<HashMap<String, AppRateLimitSnapshot>>,
    ordinary_usage_allowed: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppTokenSummary {
    lifetime_tokens: Option<i64>,
    peak_daily_tokens: Option<i64>,
    current_streak_days: Option<i64>,
    longest_streak_days: Option<i64>,
    longest_running_turn_sec: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppDailyBucket {
    start_date: String,
    tokens: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppTokenUsageResponse {
    summary: AppTokenSummary,
    daily_usage_buckets: Option<Vec<AppDailyBucket>>,
}

/// A per-model limit (e.g. "GPT-5.3-Codex-Spark"), alongside the account limit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexAdditionalRateLimit {
    pub limit_name: Option<String>,
    pub metered_feature: Option<String>,
    pub rate_limit: Option<CodexRateLimit>,
}

/// Credit balance — Codex's equivalent of Claude's extra-usage bucket.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexCredits {
    #[serde(default)]
    pub has_credits: bool,
    #[serde(default)]
    pub unlimited: bool,
    /// Decimal string, not a number — the backend sends `"0"`.
    pub balance: Option<String>,
}

/// Availability of a model that the current plan/limit state gates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexModelUsage {
    #[serde(default)]
    pub available: bool,
    pub available_at: Option<String>,
    #[serde(default)]
    pub credits_would_enable: bool,
}

/// Provider-neutral response exposed through Tauri and HTTP.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexUsageApiResponse {
    pub plan_type: Option<String>,
    pub rate_limit: Option<CodexRateLimit>,
    #[serde(default)]
    pub additional_rate_limits: Vec<CodexAdditionalRateLimit>,
    pub credits: Option<CodexCredits>,
    #[serde(default)]
    pub model_usage: std::collections::HashMap<String, CodexModelUsage>,
}

/// One day of token consumption.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexDailyBucket {
    /// `YYYY-MM-DD`, already bucketed by the backend.
    pub start_date: String,
    pub tokens: i64,
}

/// Lifetime and rolling stats from `account/usage/read`.
///
/// Legacy optional fields stay in the transport shape for frontend
/// compatibility. The official App Server does not expose them, so they remain
/// `None` and the dashboard omits them.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CodexStats {
    pub lifetime_tokens: Option<i64>,
    pub peak_daily_tokens: Option<i64>,
    pub current_streak_days: Option<i64>,
    pub longest_streak_days: Option<i64>,
    pub total_threads: Option<i64>,
    pub longest_running_turn_sec: Option<i64>,
    pub fast_mode_usage_percentage: Option<f64>,
    pub total_skills_used: Option<i64>,
    pub unique_skills_used: Option<i64>,
    pub most_used_reasoning_effort: Option<String>,
    pub most_used_reasoning_effort_percentage: Option<f64>,
    #[serde(default)]
    pub daily_usage_buckets: Vec<CodexDailyBucket>,
}

/// Token history response exposed through Tauri and HTTP.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CodexStatsResponse {
    #[serde(default)]
    pub stats: CodexStats,
}

// ---------------------------------------------------------------------------
// Cache + fetch
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct CodexSnapshot {
    usage: CodexUsageApiResponse,
    stats: CodexStatsResponse,
}

static SNAPSHOT_CACHE: UsageCache<CodexSnapshot> = UsageCache::new();
static FETCH_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn reset_after(resets_at: Option<i64>, now: i64) -> Option<i64> {
    resets_at.map(|value| value.saturating_sub(now).max(0))
}

fn map_window(window: Option<AppRateWindow>, now: i64) -> Option<CodexRateWindow> {
    window.map(|value| CodexRateWindow {
        used_percent: value.used_percent,
        limit_window_seconds: value.window_duration_mins.map(|minutes| minutes * 60),
        reset_after_seconds: reset_after(value.resets_at, now),
        reset_at: value.resets_at,
    })
}

fn map_rate_limit(
    snapshot: AppRateLimitSnapshot,
    allowed: Option<bool>,
    now: i64,
) -> CodexRateLimit {
    CodexRateLimit {
        allowed,
        limit_reached: snapshot.rate_limit_reached_type.is_some()
            || snapshot.spend_control_reached == Some(true),
        primary_window: map_window(snapshot.primary, now),
        secondary_window: map_window(snapshot.secondary, now),
    }
}

fn map_app_server_snapshot(
    limits: AppRateLimitsResponse,
    tokens: AppTokenUsageResponse,
    now: i64,
) -> CodexSnapshot {
    let primary_id = limits.rate_limits.limit_id.clone();
    let plan_type = limits.rate_limits.plan_type.clone();
    let credits = limits
        .rate_limits
        .credits
        .clone()
        .map(|value| CodexCredits {
            has_credits: value.has_credits,
            unlimited: value.unlimited,
            balance: value.balance,
        });
    let rate_limit = Some(map_rate_limit(
        limits.rate_limits,
        limits.ordinary_usage_allowed,
        now,
    ));
    let mut additional_rate_limits = limits
        .rate_limits_by_limit_id
        .unwrap_or_default()
        .into_iter()
        .filter(|(key, value)| {
            Some(key) != primary_id.as_ref()
                && key.as_str() != "codex"
                && value.limit_id != primary_id
        })
        .map(|(key, value)| CodexAdditionalRateLimit {
            limit_name: value.limit_name.clone(),
            metered_feature: value.limit_id.clone().or(Some(key)),
            rate_limit: Some(map_rate_limit(value, limits.ordinary_usage_allowed, now)),
        })
        .collect::<Vec<_>>();
    additional_rate_limits.sort_by(|a, b| a.limit_name.cmp(&b.limit_name));

    let summary = tokens.summary;
    CodexSnapshot {
        usage: CodexUsageApiResponse {
            plan_type,
            rate_limit,
            additional_rate_limits,
            credits,
            model_usage: HashMap::new(),
        },
        stats: CodexStatsResponse {
            stats: CodexStats {
                lifetime_tokens: summary.lifetime_tokens,
                peak_daily_tokens: summary.peak_daily_tokens,
                current_streak_days: summary.current_streak_days,
                longest_streak_days: summary.longest_streak_days,
                longest_running_turn_sec: summary.longest_running_turn_sec,
                daily_usage_buckets: tokens
                    .daily_usage_buckets
                    .unwrap_or_default()
                    .into_iter()
                    .map(|bucket| CodexDailyBucket {
                        start_date: bucket.start_date,
                        tokens: bucket.tokens,
                    })
                    .collect(),
                ..CodexStats::default()
            },
        },
    }
}

async fn fetch_snapshot() -> Result<CodexSnapshot, String> {
    let detection = crate::agent::detect_agent_binary("codex".to_string()).await;
    let binary = detection
        .path
        .ok_or_else(|| "Codex CLI not found".to_string())?;
    let messages = [
        serde_json::json!({
            "method": "initialize",
            "id": 0,
            "params": {"clientInfo": {"name": "tuicommander", "title": "TUICommander", "version": env!("CARGO_PKG_VERSION")}}
        }),
        serde_json::json!({"method": "initialized", "params": {}}),
        serde_json::json!({"method": "account/rateLimits/read", "id": 1, "params": null}),
        serde_json::json!({"method": "account/usage/read", "id": 2, "params": null}),
    ];
    let responses = crate::cli_usage_rpc::request_jsonl(
        Path::new(&binary),
        &["app-server"],
        &messages,
        &[1, 2],
        Duration::from_secs(20),
    )
    .await?;
    let limits = crate::cli_usage_rpc::decode_result(&responses, 1, "Codex rate limits")?;
    let tokens = crate::cli_usage_rpc::decode_result(&responses, 2, "Codex usage")?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    Ok(map_app_server_snapshot(limits, tokens, now))
}

async fn cached_snapshot() -> Result<CodexSnapshot, String> {
    if let Some(snapshot) = SNAPSHOT_CACHE.fresh() {
        return Ok(snapshot);
    }
    let _guard = FETCH_LOCK.lock().await;
    if let Some(snapshot) = SNAPSHOT_CACHE.fresh() {
        return Ok(snapshot);
    }
    match fetch_snapshot().await {
        Ok(snapshot) => {
            SNAPSHOT_CACHE.put(&snapshot);
            Ok(snapshot)
        }
        Err(message)
            if !looks_like_auth_failure(&message) && !looks_like_contract_failure(&message) =>
        {
            if let Some(snapshot) = SNAPSHOT_CACHE.stale() {
                tracing::info!(
                    source = "codex_usage",
                    "Returning stale cache after error: {message}"
                );
                Ok(snapshot)
            } else {
                Err(message)
            }
        }
        Err(message) => Err(message),
    }
}

/// Query only quota through the existing App Server contract, with explicit process home/profile.
pub(crate) async fn session_quota(
    home: &str,
    codex_home: &str,
    profile: Option<&str>,
) -> Result<CodexUsageApiResponse, String> {
    let detection = crate::agent::detect_agent_binary("codex".into()).await;
    let binary = detection.path.ok_or("Codex CLI unavailable")?;
    session_quota_from_binary(Path::new(&binary), home, codex_home, profile).await
}

// AbsolutePathBuf fields from Codex 0.166's codex-rs/core/config.schema.json:
// https://github.com/openai/codex/blob/rust-v0.166.0/codex-rs/core/config.schema.json
// Its loader
// resolves exactly these fields against each layer's directory before merging:
// https://github.com/openai/codex/blob/rust-v0.166.0/codex-rs/config/src/loader/mod.rs
// Keep full paths: similarly named MCP commands, args and unknown keys are not paths.
const CODEX_PROFILE_PATHS: &[&str] = &[
    "agents.*.config_file",
    "experimental_compact_prompt_file",
    "log_dir",
    "model_catalog_json",
    "model_instructions_file",
    "model_providers.*.auth.cwd",
    "otel.exporter.otlp-http.tls.ca-certificate",
    "otel.exporter.otlp-http.tls.client-certificate",
    "otel.exporter.otlp-http.tls.client-private-key",
    "otel.exporter.otlp-grpc.tls.ca-certificate",
    "otel.exporter.otlp-grpc.tls.client-certificate",
    "otel.exporter.otlp-grpc.tls.client-private-key",
    "otel.metrics_exporter.otlp-http.tls.ca-certificate",
    "otel.metrics_exporter.otlp-http.tls.client-certificate",
    "otel.metrics_exporter.otlp-http.tls.client-private-key",
    "otel.metrics_exporter.otlp-grpc.tls.ca-certificate",
    "otel.metrics_exporter.otlp-grpc.tls.client-certificate",
    "otel.metrics_exporter.otlp-grpc.tls.client-private-key",
    "otel.trace_exporter.otlp-http.tls.ca-certificate",
    "otel.trace_exporter.otlp-http.tls.client-certificate",
    "otel.trace_exporter.otlp-http.tls.client-private-key",
    "otel.trace_exporter.otlp-grpc.tls.ca-certificate",
    "otel.trace_exporter.otlp-grpc.tls.client-certificate",
    "otel.trace_exporter.otlp-grpc.tls.client-private-key",
    "profiles.*.experimental_compact_prompt_file",
    "profiles.*.model_catalog_json",
    "profiles.*.model_instructions_file",
    "sandbox_workspace_write.writable_roots.*",
    "skills.config.*.path",
    "sqlite_home",
];

fn resolve_profile_path(value: &mut toml::Value, keys: &[&str], base: &Path) {
    let Some((key, rest)) = keys.split_first() else {
        if let toml::Value::String(path) = value {
            // Leave home expansion to Codex under the session-bound HOME.
            let home_relative =
                path == "~" || path.starts_with("~/") || (cfg!(windows) && path.starts_with("~\\"));
            if !home_relative && !Path::new(path.as_str()).is_absolute() {
                *path = base.join(&*path).to_string_lossy().into_owned();
            }
        }
        return;
    };
    match value {
        toml::Value::Table(table) if *key == "*" => {
            for (_, value) in table.iter_mut() {
                resolve_profile_path(value, rest, base);
            }
        }
        toml::Value::Array(array) if *key == "*" => {
            for value in array {
                resolve_profile_path(value, rest, base);
            }
        }
        toml::Value::Table(table) => {
            if let Some(value) = table.get_mut(*key) {
                resolve_profile_path(value, rest, base);
            }
        }
        _ => {}
    }
}

async fn session_quota_from_binary(
    binary: &Path,
    home: &str,
    codex_home: &str,
    profile: Option<&str>,
) -> Result<CodexUsageApiResponse, String> {
    // App Server 0.166 rejects --profile (on either side of app-server), and
    // -c profile=... is a rejected legacy selector. Apply the profile's top-level
    // TOML values as supported CLI overrides; Codex still owns the base config,
    // authentication and the merge of nested tables.
    let mut args = vec!["app-server".to_string()];
    if let Some(profile) = profile {
        let path = Path::new(codex_home).join(format!("{profile}.config.toml"));
        let contents = tokio::fs::read_to_string(&path)
            .await
            .map_err(|_| "Codex profile configuration unavailable".to_string())?;
        let mut layer: toml::Value = toml::from_str(&contents)
            .map_err(|_| "Codex profile configuration invalid".to_string())?;
        let base = std::path::absolute(path.parent().ok_or("Codex profile directory unavailable")?)
            .map_err(|_| "Codex profile directory unavailable".to_string())?;
        for key in CODEX_PROFILE_PATHS {
            resolve_profile_path(&mut layer, &key.split('.').collect::<Vec<_>>(), &base);
        }
        for (key, value) in layer
            .as_table()
            .ok_or("Codex profile configuration invalid")?
        {
            args.push("-c".to_string());
            args.push(format!("{key}={value}"));
        }
    }
    let args = args.iter().map(String::as_str).collect::<Vec<_>>();
    let messages = [
        serde_json::json!({"method":"initialize","id":0,"params":{"clientInfo":{"name":"tuicommander","version":env!("CARGO_PKG_VERSION")}}}),
        serde_json::json!({"method":"initialized","params":{}}),
        serde_json::json!({"method":"account/rateLimits/read","id":1,"params":null}),
    ];
    let responses = crate::cli_usage_rpc::request_jsonl_with_env(
        binary,
        &args,
        &messages,
        &[1],
        Duration::from_secs(20),
        &[
            ("HOME".into(), home.into()),
            ("CODEX_HOME".into(), codex_home.into()),
        ],
    )
    .await?;
    let limits = crate::cli_usage_rpc::decode_result(&responses, 1, "Codex rate limits")?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    Ok(map_app_server_snapshot(
        limits,
        AppTokenUsageResponse {
            summary: AppTokenSummary::default(),
            daily_usage_buckets: None,
        },
        now,
    )
    .usage)
}

/// Fetch Codex rate-limit usage (powers the status bar ticker).
#[cfg_attr(feature = "desktop", tauri::command)]
pub async fn get_codex_usage_api() -> Result<CodexUsageApiResponse, String> {
    Ok(cached_snapshot().await?.usage)
}

/// Fetch Codex token history and lifetime stats (powers the dashboard).
#[cfg_attr(feature = "desktop", tauri::command)]
pub async fn get_codex_usage_stats() -> Result<CodexStatsResponse, String> {
    Ok(cached_snapshot().await?.stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Catches: CLI overrides resolving profile-relative instructions from the process cwd.
    #[cfg(unix)]
    #[tokio::test]
    async fn codex_profile_quota_preserves_relative_instruction_path_at_process_boundary() {
        // Codex 0.166 live config/read evidence: explicit -c model selects the layer.
        let script = crate::test_support::fake_ssh_script(
            "2541-codex-profile-layer",
            r#"
case "$*" in
  'app-server') used=5 ;;
  'app-server -c model="profile-marker" -c model_instructions_file='*)
    instructions=${5#model_instructions_file=}
    instructions=${instructions#\"}
    instructions=${instructions%\"}
    # Recorded 0.166 behavior: an unreadable instruction file drops the whole layer.
    if [ -r "$instructions" ]; then used=23.5; else used=5; fi
    ;;
  *) exit 2 ;;
esac
while IFS= read -r request; do
  case "$request" in
    *account/rateLimits/read*)
      printf '{"id":1,"result":{"rateLimits":{"primary":{"usedPercent":%s,"windowDurationMins":300}}}}\n' "$used"
      ;;
  esac
done
"#,
            "exit /b 2",
        );
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let binary = dir.path().join("codex");
        std::fs::copy(script, &binary).unwrap();
        std::fs::write(dir.path().join("instructions.md"), "Profile instructions").unwrap();
        for (profile, instructions) in [
            (Some("work"), "instructions.md".to_string()),
            (
                Some("work"),
                dir.path().join("instructions.md").display().to_string(),
            ),
            (None, "instructions.md".to_string()),
        ] {
            std::fs::write(
                dir.path().join("work.config.toml"),
                format!(
                    "model = \"profile-marker\"\nmodel_instructions_file = {}\n",
                    toml::Value::String(instructions)
                ),
            )
            .unwrap();
            let quota = session_quota_from_binary(
                &binary,
                dir.path().to_str().unwrap(),
                dir.path().to_str().unwrap(),
                profile,
            )
            .await
            .expect("profiled CLI must return quota, not an argument error");
            assert_eq!(
                quota
                    .rate_limit
                    .unwrap()
                    .primary_window
                    .unwrap()
                    .used_percent,
                if profile.is_some() { 23.5 } else { 5.0 }
            );
        }
    }

    /// Catches: a missing or malformed selected profile silently querying the default account.
    #[tokio::test]
    async fn codex_profile_quota_rejects_unavailable_and_invalid_layers() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let binary = dir.path().join("must-not-start");
        let home = dir.path().to_str().unwrap();
        let missing = session_quota_from_binary(&binary, home, home, Some("missing"))
            .await
            .unwrap_err();
        assert_eq!(missing, "Codex profile configuration unavailable");
        std::fs::write(dir.path().join("broken.config.toml"), "model = [").unwrap();
        let invalid = session_quota_from_binary(&binary, home, home, Some("broken"))
            .await
            .unwrap_err();
        assert_eq!(invalid, "Codex profile configuration invalid");
    }

    /// Catches: publishing default-account usage after Codex rejects a selected layer.
    #[cfg(unix)]
    #[tokio::test]
    async fn codex_profile_quota_rejects_provider_defaults_fallback() {
        let script = crate::test_support::fake_ssh_script(
            "2541-codex-profile-warning",
            r#"
while IFS= read -r request; do
  case "$request" in
    *account/rateLimits/read*)
      cat "$CODEX_HOME/warning.json"
      printf '{"id":1,"result":{"rateLimits":{"primary":{"usedPercent":5}}}}\n'
      ;;
  esac
done
"#,
            "exit /b 2",
        );
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        std::fs::write(dir.path().join("work.config.toml"), "model = \"marker\"").unwrap();
        std::fs::write(
            dir.path().join("warning.json"),
            include_str!("fixtures/codex_profile_invalid_configuration_0166.json"),
        )
        .unwrap();
        let error = session_quota_from_binary(
            &script,
            dir.path().to_str().unwrap(),
            dir.path().to_str().unwrap(),
            Some("work"),
        )
        .await
        .unwrap_err();
        assert_eq!(
            error,
            "Provider usage configuration invalid; defaults rejected"
        );
        assert!(
            looks_like_contract_failure(&error),
            "must not enable stale-cache fallback"
        );
    }

    /// Catches: rebasing only top-level paths, or rebasing unrelated strings and home paths.
    #[test]
    fn codex_profile_paths_preserve_nested_origins_and_non_path_values() {
        let base = crate::test_support::test_temp_root();
        let mut layer: toml::Value = toml::from_str(
            r#"
model_instructions_file = "instructions.md"
experimental_compact_prompt_file = "compact.md"
model_catalog_json = "catalog.json"
log_dir = "logs"
sqlite_home = "db"
notify = ["relative-command"]
[agents."role.with.dot"]
config_file = "agents/reviewer.toml"
[model_providers.custom.auth]
cwd = "provider"
[profiles.nested]
model_instructions_file = "nested.md"
[skills]
config = [{ path = "skills/SKILL.md", enabled = false }]
[sandbox_workspace_write]
writable_roots = ["workspace", "~/shared"]
[mcp_servers.custom]
command = "relative-command"
cwd = "relative-mcp-cwd"
[unknown]
model_instructions_file = "not-a-known-path"
"#,
        )
        .unwrap();
        // Both exporter variants under all three OTEL settings use AbsolutePathBuf TLS fields.
        let mut otel = String::new();
        for exporter in ["exporter", "metrics_exporter", "trace_exporter"] {
            for protocol in ["otlp-http", "otlp-grpc"] {
                otel.push_str(&format!("[otel.{exporter}.{protocol}.tls]\n"));
                for key in ["ca-certificate", "client-certificate", "client-private-key"] {
                    otel.push_str(&format!("{key} = \"tls.pem\"\n"));
                }
            }
        }
        layer
            .as_table_mut()
            .unwrap()
            .extend(toml::from_str::<toml::Table>(&otel).unwrap());
        for key in CODEX_PROFILE_PATHS {
            resolve_profile_path(&mut layer, &key.split('.').collect::<Vec<_>>(), &base);
        }
        for (key, file) in [
            ("model_instructions_file", "instructions.md"),
            ("experimental_compact_prompt_file", "compact.md"),
            ("model_catalog_json", "catalog.json"),
            ("log_dir", "logs"),
            ("sqlite_home", "db"),
        ] {
            assert_eq!(layer[key].as_str(), base.join(file).to_str(), "{key}");
        }
        assert_eq!(
            layer["agents"]["role.with.dot"]["config_file"].as_str(),
            base.join("agents/reviewer.toml").to_str()
        );
        assert_eq!(
            layer["model_providers"]["custom"]["auth"]["cwd"].as_str(),
            base.join("provider").to_str()
        );
        assert_eq!(
            layer["profiles"]["nested"]["model_instructions_file"].as_str(),
            base.join("nested.md").to_str()
        );
        assert_eq!(
            layer["skills"]["config"][0]["path"].as_str(),
            base.join("skills/SKILL.md").to_str()
        );
        assert_eq!(
            layer["sandbox_workspace_write"]["writable_roots"][0].as_str(),
            base.join("workspace").to_str()
        );
        assert_eq!(
            layer["sandbox_workspace_write"]["writable_roots"][1].as_str(),
            Some("~/shared")
        );
        assert_eq!(layer["notify"][0].as_str(), Some("relative-command"));
        assert_eq!(
            layer["mcp_servers"]["custom"]["command"].as_str(),
            Some("relative-command")
        );
        assert_eq!(
            layer["mcp_servers"]["custom"]["cwd"].as_str(),
            Some("relative-mcp-cwd")
        );
        assert_eq!(
            layer["unknown"]["model_instructions_file"].as_str(),
            Some("not-a-known-path")
        );
        for exporter in ["exporter", "metrics_exporter", "trace_exporter"] {
            for protocol in ["otlp-http", "otlp-grpc"] {
                for key in ["ca-certificate", "client-certificate", "client-private-key"] {
                    assert_eq!(
                        layer["otel"][exporter][protocol]["tls"][key].as_str(),
                        base.join("tls.pem").to_str()
                    );
                }
            }
        }
    }

    #[test]
    fn maps_official_app_server_snapshot() {
        let limits: AppRateLimitsResponse = serde_json::from_value(serde_json::json!({
            "ordinaryUsageAllowed": true,
            "rateLimits": {
                "limitId": "codex",
                "planType": "pro",
                "primary": {"usedPercent": 23.5, "windowDurationMins": 300, "resetsAt": 1600},
                "credits": {"hasCredits": true, "unlimited": false, "balance": "12.5"}
            },
            "rateLimitsByLimitId": {
                "codex": {"limitId": "codex", "planType": "pro"},
                "spark": {
                    "limitId": "spark",
                    "limitName": "Spark",
                    "secondary": {"usedPercent": 80, "windowDurationMins": 10080, "resetsAt": 2200},
                    "rateLimitReachedType": "secondary"
                }
            }
        }))
        .unwrap();
        let tokens: AppTokenUsageResponse = serde_json::from_value(serde_json::json!({
            "summary": {"lifetimeTokens": 42, "currentStreakDays": 3},
            "dailyUsageBuckets": [{"startDate": "2026-09-19", "tokens": 9}]
        }))
        .unwrap();

        let snapshot = map_app_server_snapshot(limits, tokens, 1000);
        assert_eq!(snapshot.usage.plan_type.as_deref(), Some("pro"));
        let primary = snapshot.usage.rate_limit.unwrap();
        assert_eq!(primary.allowed, Some(true));
        assert_eq!(
            primary.primary_window.unwrap().limit_window_seconds,
            Some(18_000)
        );
        assert_eq!(snapshot.usage.additional_rate_limits.len(), 1);
        assert!(
            snapshot.usage.additional_rate_limits[0]
                .rate_limit
                .as_ref()
                .unwrap()
                .limit_reached
        );
        assert_eq!(snapshot.stats.stats.lifetime_tokens, Some(42));
        assert_eq!(snapshot.stats.stats.daily_usage_buckets[0].tokens, 9);
        assert!(snapshot.usage.model_usage.is_empty());
    }

    #[test]
    fn null_daily_buckets_are_empty_and_unknown_stats_stay_unknown() {
        let limits: AppRateLimitsResponse = serde_json::from_value(serde_json::json!({
            "ordinaryUsageAllowed": null,
            "rateLimits": {"primary": null, "secondary": null},
            "rateLimitsByLimitId": null
        }))
        .unwrap();
        let tokens: AppTokenUsageResponse = serde_json::from_value(serde_json::json!({
            "summary": {}, "dailyUsageBuckets": null
        }))
        .unwrap();

        let snapshot = map_app_server_snapshot(limits, tokens, 0);
        assert_eq!(snapshot.usage.rate_limit.unwrap().allowed, None);
        assert!(snapshot.usage.additional_rate_limits.is_empty());
        assert!(snapshot.stats.stats.daily_usage_buckets.is_empty());
        assert!(snapshot.stats.stats.total_threads.is_none());
    }

    #[test]
    fn reset_countdown_never_goes_negative() {
        assert_eq!(reset_after(Some(99), 100), Some(0));
        assert_eq!(reset_after(Some(160), 100), Some(60));
        assert_eq!(reset_after(None, 100), None);
    }

    #[test]
    fn schema_failures_are_not_hidden_by_stale_data() {
        assert!(looks_like_contract_failure(
            "Failed to parse Codex rate limits response: invalid type: null"
        ));
        assert!(!looks_like_contract_failure("network connection reset"));
    }
}
