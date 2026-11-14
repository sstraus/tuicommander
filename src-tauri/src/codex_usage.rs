//! Codex quota comes from the shared owner registry. Token statistics use
//! the separate Codex App Server account/usage/read JSON-RPC method.

use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::cli_usage_rpc::{
    FailureBackoff, UsageCache, looks_like_auth_failure, looks_like_contract_failure,
};

// ---------------------------------------------------------------------------
// Stable TUIC transport types (mapped from the App Server response)
// ---------------------------------------------------------------------------

/// One rate-limit window (session or weekly).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodexAdditionalRateLimit {
    pub limit_name: Option<String>,
    pub metered_feature: Option<String>,
    pub rate_limit: Option<CodexRateLimit>,
}

/// Credit balance — Codex's equivalent of Claude's extra-usage bucket.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodexCredits {
    #[serde(default)]
    pub has_credits: bool,
    #[serde(default)]
    pub unlimited: bool,
    /// Decimal string, not a number — the backend sends `"0"`.
    pub balance: Option<String>,
}

/// Availability of a model that the current plan/limit state gates.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodexModelUsage {
    #[serde(default)]
    pub available: bool,
    pub available_at: Option<String>,
    #[serde(default)]
    pub credits_would_enable: bool,
}

/// Provider-neutral response exposed through Tauri and HTTP.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

/// One reader per account identity: cache, failure cooldown and single-flight lock.
struct Reader {
    cache: UsageCache<CodexStatsResponse>,
    backoff: FailureBackoff,
    lock: tokio::sync::Mutex<()>,
}

impl Reader {
    const fn new() -> Self {
        Self {
            cache: UsageCache::new(),
            backoff: FailureBackoff::new(),
            lock: tokio::sync::Mutex::const_new(()),
        }
    }

    async fn read(
        &self,
        fetch: impl std::future::Future<Output = Result<CodexStatsResponse, String>>,
    ) -> Result<CodexStatsResponse, String> {
        if let Some(snapshot) = self.cache.fresh() {
            return Ok(snapshot);
        }
        let _guard = self.lock.lock().await;
        if let Some(snapshot) = self.cache.fresh() {
            return Ok(snapshot);
        }
        // Inside a failure cooldown: serve the last reading or the cached error, no spawn.
        if let Some(message) = self.backoff.active() {
            return self.cache.stale().ok_or(message);
        }
        match fetch.await {
            Ok(snapshot) => {
                self.backoff.clear();
                self.cache.put(&snapshot);
                Ok(snapshot)
            }
            Err(message) => {
                self.backoff.record(&message);
                if !looks_like_auth_failure(&message)
                    && !looks_like_contract_failure(&message)
                    && let Some(snapshot) = self.cache.stale()
                {
                    tracing::info!(
                        source = "codex_usage",
                        "Returning stale cache after error: {message}"
                    );
                    return Ok(snapshot);
                }
                Err(message)
            }
        }
    }
}

/// Serves the global ticker, the dashboard and every session on the host's own account.
static HOST_READER: Reader = Reader::new();

fn map_token_usage(tokens: AppTokenUsageResponse) -> CodexStatsResponse {
    let summary = tokens.summary;
    CodexStatsResponse {
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
    }
}

async fn fetch_snapshot() -> Result<CodexStatsResponse, String> {
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
        serde_json::json!({"method": "account/usage/read", "id": 2, "params": null}),
    ];
    let responses = crate::cli_usage_rpc::request_jsonl(
        Path::new(&binary),
        &["app-server"],
        &messages,
        &[2],
        Duration::from_secs(20),
    )
    .await?;
    let tokens = crate::cli_usage_rpc::decode_result(&responses, 2, "Codex usage")?;
    Ok(map_token_usage(tokens))
}

async fn cached_snapshot() -> Result<CodexStatsResponse, String> {
    HOST_READER.read(fetch_snapshot()).await
}

/// The CODEX_HOME inherited by the global quota reader.
fn host_identity() -> Option<String> {
    let home = dirs::home_dir()?;
    let codex_home = std::env::var_os("CODEX_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| home.join(".codex"));
    Some(codex_home.to_string_lossy().into_owned())
}

/// Read quota from the registry with explicit credential home/profile.
pub(crate) async fn session_quota(
    registry: &crate::provider_quota::QuotaRegistry,
    codex_home: &str,
    profile: Option<&str>,
) -> Result<CodexUsageApiResponse, String> {
    // Preserve the live profile identity; unsupported profiles fail closed.
    let mut key =
        crate::provider_quota::SubscriptionKey::new("codex-cli", "openai-codex", codex_home);
    key.profile = profile.map(str::to_owned);
    registry_usage(registry, &key).await
}

async fn registry_usage(
    registry: &crate::provider_quota::QuotaRegistry,
    key: &crate::provider_quota::SubscriptionKey,
) -> Result<CodexUsageApiResponse, String> {
    use crate::provider_quota::{QuotaDetail, QuotaState};
    let view = registry.ensure_detail(key).await;
    let error = || match &view.state {
        QuotaState::Backoff { until_ms } => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            crate::claude_usage::usage_retry_message(
                std::time::Instant::now() + Duration::from_millis(until_ms.saturating_sub(now)),
                "Codex",
            )
        }
        QuotaState::AuthInvalid => "Codex usage API returned 401: sign in again".into(),
        QuotaState::Unavailable { reason } => format!("Codex not logged in: {reason}"),
        _ => format!("Codex usage unavailable: {:?}", view.state),
    };
    if matches!(
        view.state,
        QuotaState::AuthInvalid | QuotaState::ContractError { .. }
    ) {
        return Err(error());
    }
    match view.snapshot.and_then(|snapshot| snapshot.detail) {
        Some(QuotaDetail::Codex { response, .. }) => Ok(response),
        _ => Err(error()),
    }
}

/// Fetch Codex rate-limit usage (powers the status bar ticker).
#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn get_codex_usage_api(
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
) -> Result<CodexUsageApiResponse, String> {
    get_codex_usage_api_impl(&state).await
}

pub(crate) async fn get_codex_usage_api_impl(
    state: &crate::AppState,
) -> Result<CodexUsageApiResponse, String> {
    let codex_home = host_identity().ok_or("Home not found")?;
    session_quota(&state.provider_quota, &codex_home, None).await
}

/// Fetch Codex token history and lifetime stats (powers the dashboard).
#[cfg_attr(feature = "desktop", tauri::command)]
pub async fn get_codex_usage_stats() -> Result<CodexStatsResponse, String> {
    cached_snapshot().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // Catches: registry errors hiding a missing login, expiry or retry time as offline.
    #[tokio::test]
    async fn registry_usage_errors_keep_the_ticker_classification() {
        use crate::provider_quota::{
            BoxFuture, QuotaError, QuotaOwner, QuotaRegistry, QuotaSnapshot, SubscriptionKey,
        };
        struct Failing(fn() -> QuotaError);
        impl QuotaOwner for Failing {
            fn owner(&self) -> &'static str {
                "codex-cli"
            }
            fn read<'a>(
                &'a self,
                _: &'a SubscriptionKey,
            ) -> BoxFuture<'a, Result<QuotaSnapshot, QuotaError>> {
                let error = (self.0)();
                Box::pin(async move { Err(error) })
            }
        }
        for (error, expected) in [
            (
                (|| QuotaError::RateLimited {
                    retry_after_secs: Some(3600),
                }) as fn() -> QuotaError,
                "Codex usage rate-limited, retry ",
            ),
            (
                || QuotaError::Unavailable("login not found".into()),
                "not logged in",
            ),
            (|| QuotaError::Auth("expired".into()), "401"),
        ] {
            let registry = QuotaRegistry::default();
            registry.register_owner(std::sync::Arc::new(Failing(error)));
            let key = SubscriptionKey::new("codex-cli", "openai-codex", "/fixture/codex");
            let message = registry_usage(&registry, &key).await.unwrap_err();
            assert!(message.contains(expected), "{message}");
        }
    }

    fn snapshot() -> CodexStatsResponse {
        map_token_usage(AppTokenUsageResponse {
            summary: AppTokenSummary::default(),
            daily_usage_buckets: None,
        })
    }

    /// Catches: every ticker poll, window or session re-spawning app-server after a 429.
    #[tokio::test]
    async fn codex_failed_read_is_not_retried_inside_backoff() {
        let reader = Reader::new();
        let spawns = AtomicUsize::new(0);
        for _ in 0..3 {
            let error = reader
                .read(async {
                    spawns.fetch_add(1, Ordering::SeqCst);
                    Err::<CodexStatsResponse, _>("429 Too Many Requests".to_string())
                })
                .await
                .err()
                .unwrap();
            assert_eq!(error, "429 Too Many Requests");
        }
        assert_eq!(spawns.load(Ordering::SeqCst), 1);
    }

    /// Catches: repeated statistics requests spawning App Server in one cache window.
    #[tokio::test]
    async fn codex_stats_reads_share_one_fetch() {
        let reader = Reader::new();
        let spawns = AtomicUsize::new(0);
        for _ in 0..2 {
            reader
                .read(async {
                    spawns.fetch_add(1, Ordering::SeqCst);
                    Ok(snapshot())
                })
                .await
                .unwrap();
        }
        assert_eq!(spawns.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn maps_official_app_server_snapshot() {
        let tokens: AppTokenUsageResponse = serde_json::from_value(serde_json::json!({
            "summary": {"lifetimeTokens": 42, "currentStreakDays": 3},
            "dailyUsageBuckets": [{"startDate": "2026-09-19", "tokens": 9}]
        }))
        .unwrap();

        let stats = map_token_usage(tokens);
        assert_eq!(stats.stats.lifetime_tokens, Some(42));
        assert_eq!(stats.stats.daily_usage_buckets[0].tokens, 9);
    }

    #[test]
    fn null_daily_buckets_are_empty_and_unknown_stats_stay_unknown() {
        let tokens: AppTokenUsageResponse = serde_json::from_value(serde_json::json!({
            "summary": {}, "dailyUsageBuckets": null
        }))
        .unwrap();

        let stats = map_token_usage(tokens);
        assert!(stats.stats.daily_usage_buckets.is_empty());
        assert!(stats.stats.total_threads.is_none());
    }

    #[test]
    fn schema_failures_are_not_hidden_by_stale_data() {
        assert!(looks_like_contract_failure(
            "Failed to parse Codex rate limits response: invalid type: null"
        ));
        assert!(!looks_like_contract_failure("network connection reset"));
    }
}
