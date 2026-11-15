//! Grok account usage through Grok Build's documented ACP billing extension.
//!
//! The short-lived ACP process is telemetry-only. Normal Grok tabs remain PTY
//! sessions; this module does not introduce ACP session routing or fallback.
//! The quota registry owns caching, single-flight and failure cooldown
//! (`provider_quota::grok`); the dashboard reads through it.

use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GrokMoney {
    pub val: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GrokUsagePeriod {
    #[serde(alias = "type")]
    pub period_type: Option<String>,
    pub start: Option<String>,
    pub end: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GrokBillingConfig {
    #[serde(alias = "creditUsagePercent")]
    pub credit_usage_percent: Option<f64>,
    #[serde(alias = "currentPeriod")]
    pub current_period: Option<GrokUsagePeriod>,
    #[serde(alias = "onDemandCap")]
    pub on_demand_cap: Option<GrokMoney>,
    #[serde(alias = "onDemandUsed")]
    pub on_demand_used: Option<GrokMoney>,
    #[serde(alias = "prepaidBalance")]
    pub prepaid_balance: Option<GrokMoney>,
    #[serde(alias = "isUnifiedBillingUser")]
    pub is_unified_billing_user: Option<bool>,
    #[serde(alias = "billingPeriodStart")]
    pub billing_period_start: Option<String>,
    #[serde(alias = "billingPeriodEnd")]
    pub billing_period_end: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GrokUsageApiResponse {
    pub config: GrokBillingConfig,
    #[serde(alias = "onDemandEnabled")]
    pub on_demand_enabled: Option<bool>,
    #[serde(alias = "subscriptionTier")]
    pub subscription_tier: Option<String>,
}

/// One `_x.ai/billing` request through `grok agent stdio`, bound to `grok_home`.
pub(crate) async fn billing(
    program: &Path,
    grok_home: &Path,
) -> Result<GrokUsageApiResponse, String> {
    let messages = [
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": "initialize",
            "params": {
                "protocolVersion": 1,
                "clientCapabilities": {"fs": {"readTextFile": false, "writeTextFile": false}, "terminal": false},
                "clientInfo": {"name": "tuicommander", "version": env!("CARGO_PKG_VERSION")}
            }
        }),
        serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "_x.ai/billing", "params": {}}),
    ];
    let responses = crate::cli_usage_rpc::request_jsonl_with_env(
        program,
        &["agent", "stdio"],
        &messages,
        &[1],
        Duration::from_secs(20),
        &[("GROK_HOME".into(), grok_home.to_string_lossy().into_owned())],
    )
    .await?;
    crate::cli_usage_rpc::decode_result(&responses, 1, "Grok billing")
}

/// The GROK_HOME inherited by the global quota reader.
fn host_identity() -> Option<String> {
    let home = dirs::home_dir()?;
    let grok_home = std::env::var_os("GROK_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| home.join(".grok"));
    Some(grok_home.to_string_lossy().into_owned())
}

/// The dashboard read: the registry's last good billing detail, or the
/// registry state as an error.
async fn registry_usage(
    registry: &crate::provider_quota::QuotaRegistry,
    key: &crate::provider_quota::SubscriptionKey,
) -> Result<GrokUsageApiResponse, String> {
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
                "Grok",
            )
        }
        QuotaState::AuthInvalid => "Grok login expired: run `grok login`".into(),
        QuotaState::Unavailable { reason } => format!("Grok not logged in: {reason}"),
        QuotaState::ContractError { message } => message.clone(),
        _ => format!("Grok usage unavailable: {:?}", view.state),
    };
    if matches!(
        view.state,
        QuotaState::AuthInvalid | QuotaState::ContractError { .. }
    ) {
        return Err(error());
    }
    match view.snapshot.and_then(|snapshot| snapshot.detail) {
        Some(QuotaDetail::Grok { response, .. }) => Ok(response),
        _ => Err(error()),
    }
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn get_grok_usage_api(
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
) -> Result<GrokUsageApiResponse, String> {
    get_grok_usage_api_impl(&state).await
}

pub(crate) async fn get_grok_usage_api_impl(
    state: &crate::AppState,
) -> Result<GrokUsageApiResponse, String> {
    let grok_home = host_identity().ok_or("Home not found")?;
    let key = crate::provider_quota::SubscriptionKey::new("grok-cli", "xai", grok_home);
    registry_usage(&state.provider_quota, &key).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_current_camel_case_billing_shape() {
        let parsed: GrokUsageApiResponse = serde_json::from_value(serde_json::json!({
            "config": {
                "creditUsagePercent": 26,
                "currentPeriod": {"type": "USAGE_PERIOD_TYPE_WEEKLY", "start": "a", "end": "b"},
                "onDemandCap": {"val": 10},
                "onDemandUsed": {"val": 2.5},
                "prepaidBalance": {"val": 4},
                "isUnifiedBillingUser": true,
                "billingPeriodStart": "a",
                "billingPeriodEnd": "b"
            },
            "on_demand_enabled": true,
            "subscription_tier": "X Premium+"
        }))
        .unwrap();

        assert_eq!(parsed.config.credit_usage_percent, Some(26.0));
        assert_eq!(
            parsed
                .config
                .current_period
                .as_ref()
                .unwrap()
                .period_type
                .as_deref(),
            Some("USAGE_PERIOD_TYPE_WEEKLY")
        );
        let on_demand = parsed.config.on_demand_used.as_ref().unwrap().val;
        assert!(
            (on_demand - 2.5).abs() < f64::EPSILON,
            "on_demand_used.val was {on_demand}, expected 2.5"
        );
        assert_eq!(parsed.subscription_tier.as_deref(), Some("X Premium+"));
        let public = serde_json::to_value(&parsed).unwrap();
        assert_eq!(
            public["config"]["current_period"]["period_type"],
            "USAGE_PERIOD_TYPE_WEEKLY"
        );
        assert!(public["config"]["current_period"].get("type").is_none());
    }

    #[test]
    fn missing_usage_percent_stays_unknown() {
        let parsed: GrokUsageApiResponse = serde_json::from_value(serde_json::json!({
            "config": {}, "on_demand_enabled": null, "subscription_tier": null
        }))
        .unwrap();
        assert!(parsed.config.credit_usage_percent.is_none());
        assert!(parsed.on_demand_enabled.is_none());
    }
}
