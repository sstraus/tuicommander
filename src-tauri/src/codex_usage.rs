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

async fn session_quota_from_binary(
    binary: &Path,
    home: &str,
    codex_home: &str,
    profile: Option<&str>,
) -> Result<CodexUsageApiResponse, String> {
    let mut args = Vec::new();
    if let Some(profile) = profile {
        args.extend(["--profile", profile]);
    }
    args.push("app-server");
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

    /// Catches: passing root --profile after app-server, which the real CLI rejects.
    #[cfg(unix)]
    #[tokio::test]
    async fn codex_profile_quota_uses_root_arguments_at_process_boundary() {
        // Syntax mirrors the installed CLI help captured by critic-2150r1.
        let script = crate::test_support::fake_ssh_script(
            "2150-codex-root-profile",
            r#"
case "$*" in
  'app-server'|'--profile work app-server') ;;
  *) exit 2 ;;
esac
while IFS= read -r request; do
  case "$request" in
    *account/rateLimits/read*)
      printf '%s\n' '{"id":1,"result":{"rateLimits":{"primary":{"usedPercent":23.5,"windowDurationMins":300}}}}'
      ;;
  esac
done
"#,
            "exit /b 2",
        );
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let binary = dir.path().join("codex");
        std::fs::copy(script, &binary).unwrap();
        for profile in [Some("work"), None] {
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
                23.5
            );
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
