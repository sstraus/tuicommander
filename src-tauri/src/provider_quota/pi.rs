//! Quota of pi-owned subscriptions (plan step 8, spike S6).
//!
//! pi signs in to Claude Pro/Max, ChatGPT Codex and GitHub Copilot with its own
//! tokens in `<agent dir>/auth.json`. Two sources, like every owner:
//!
//! - **passive**: the managed pi extension forwards each provider response on
//!   its generic `pi-event` channel (kind `provider_response`) with the status
//!   and only the [`QUOTA_HEADERS`]. [`receive_event`] stores the windows and
//!   [`QuotaOwner::observed`] returns them without a request.
//! - **active**: the direct endpoints of the plan (section 2.1) with the pi
//!   credential of the provider.
//!
//! Spike evidence: `docs/backend/quota-source-spikes.md` (S6).
use super::adapters::{get_json, read_json};
use super::*;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The only response headers the extension forwards and TUIC accepts. An
/// allow-list, so a credential (`authorization`, `set-cookie`, `x-api-key`,
/// the opaque `x-codex-turn-state`) can never cross the channel. Names come
/// from recorded responses (`fixtures/README.md`); pi passes them lowercase.
pub(crate) const QUOTA_HEADERS: &[&str] = &[
    "anthropic-ratelimit-unified-status",
    "anthropic-ratelimit-unified-representative-claim",
    "anthropic-ratelimit-unified-5h-status",
    "anthropic-ratelimit-unified-5h-utilization",
    "anthropic-ratelimit-unified-5h-reset",
    "anthropic-ratelimit-unified-7d-status",
    "anthropic-ratelimit-unified-7d-utilization",
    "anthropic-ratelimit-unified-7d-reset",
    "anthropic-ratelimit-unified-overage-status",
    "anthropic-ratelimit-unified-overage-utilization",
    "anthropic-ratelimit-unified-overage-reset",
    "x-codex-plan-type",
    "x-codex-active-limit",
    "x-codex-primary-used-percent",
    "x-codex-primary-window-minutes",
    "x-codex-primary-reset-at",
    "x-codex-primary-reset-after-seconds",
    "x-codex-primary-over-secondary-limit-percent",
    "x-codex-secondary-used-percent",
    "x-codex-secondary-window-minutes",
    "x-codex-secondary-reset-at",
    "x-codex-secondary-reset-after-seconds",
    "x-codex-credits-has-credits",
    "x-codex-credits-balance",
    "x-codex-credits-unlimited",
];

/// Longest header value the extension forwards; quota values are short.
pub(crate) const MAX_HEADER_VALUE: usize = 64;
/// Bound on one encoded frame, as for `agent-tool` frames.
const MAX_FRAME: usize = 8192;

/// One frame of the generic channel. Unknown kinds are ignored, so the
/// extension can add a kind before TUIC knows it.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum PiEvent {
    ProviderResponse {
        provider: String,
        status: u16,
        #[serde(default)]
        headers: BTreeMap<String, String>,
        /// `PI_CODING_AGENT_DIR` and `HOME` of the pi process: they name the
        /// credential, as the Claude status line relay headers do.
        #[serde(default)]
        agent_dir: String,
        #[serde(default)]
        home: String,
    },
}

/// Decode `<TUIC_SESSION>:<base64 JSON>`. A frame of another session, an
/// oversized or malformed frame, or an unknown kind gives `None`. Headers
/// outside [`QUOTA_HEADERS`] are dropped here as well.
pub(crate) fn decode(payload: &str, owner: Option<&str>) -> Option<PiEvent> {
    use base64::Engine;
    let (sender, encoded) = payload.split_once(':')?;
    if owner != Some(sender) || encoded.len() > MAX_FRAME {
        return None;
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let mut event: PiEvent = serde_json::from_slice(&bytes).ok()?;
    let PiEvent::ProviderResponse { headers, .. } = &mut event;
    headers.retain(|name, value| {
        QUOTA_HEADERS.contains(&name.as_str()) && value.len() <= MAX_HEADER_VALUE
    });
    Some(event)
}

/// Windows from forwarded quota headers; `None` when there are none.
pub(crate) fn snapshot_from_headers(
    headers: &BTreeMap<String, String>,
    observed_at_ms: u64,
) -> Option<QuotaSnapshot> {
    let number = |name: &str| {
        headers
            .get(name)?
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
    };
    let reset = |name: &str| {
        let secs = headers.get(name)?.trim().parse::<i64>().ok()?;
        chrono::DateTime::from_timestamp(secs, 0).map(|d| d.to_rfc3339())
    };
    let mut windows = Vec::new();
    // Anthropic reports utilization as a fraction.
    for (part, secs) in [("5h", 18_000), ("7d", 604_800)] {
        let prefix = format!("anthropic-ratelimit-unified-{part}");
        if let Some(window) = number(&format!("{prefix}-utilization")).and_then(|u| {
            QuotaWindow::from_duration(
                u * 100.0,
                Some(secs),
                reset(&format!("{prefix}-reset")),
                None,
            )
        }) {
            windows.push(window);
        }
    }
    // A zero window means the slot is not in use.
    for part in ["primary", "secondary"] {
        let prefix = format!("x-codex-{part}");
        let Some(minutes) = number(&format!("{prefix}-window-minutes")).filter(|m| *m > 0.0) else {
            continue;
        };
        if let Some(window) = number(&format!("{prefix}-used-percent")).and_then(|p| {
            QuotaWindow::from_duration(
                p,
                Some((minutes * 60.0) as i64),
                reset(&format!("{prefix}-reset-at")),
                None,
            )
        }) {
            windows.push(window);
        }
    }
    (!windows.is_empty()).then(|| QuotaSnapshot {
        windows,
        detail: None,
        plan: headers.get("x-codex-plan-type").cloned(),
        credits: headers.get("x-codex-credits-balance").cloned(),
        account: None,
        observed_at_ms,
        source: QuotaSource::Passive,
    })
}

/// Newest windows forwarded per credential, and the provider each pi session
/// last used (the binding needs it; pi can switch model mid-session).
#[derive(Default)]
struct Forwarded {
    snapshots: HashMap<SubscriptionKey, QuotaSnapshot>,
    providers: HashMap<String, String>,
}
static FORWARDED: std::sync::LazyLock<parking_lot::Mutex<Forwarded>> =
    std::sync::LazyLock::new(Default::default);

/// Provider of the newest response the session's pi forwarded.
pub(super) fn session_provider(session_id: &str) -> Option<String> {
    FORWARDED.lock().providers.get(session_id).cloned()
}

/// Forget closed sessions.
pub(super) fn retain_sessions(live: &std::collections::HashSet<&str>) {
    FORWARDED
        .lock()
        .providers
        .retain(|id, _| live.contains(id.as_str()));
}

/// Handle one decoded frame for a session: bind it to the pi credential of the
/// provider and store the windows. No request and no file read.
pub(crate) fn receive(registry: &QuotaRegistry, session_id: &str, event: PiEvent, now_ms: u64) {
    let PiEvent::ProviderResponse {
        provider,
        headers,
        agent_dir,
        home,
        ..
    } = event;
    let env = [
        ("PI_CODING_AGENT_DIR", agent_dir.as_str()),
        ("HOME", home.as_str()),
    ];
    let Some(key) = super::binding::resolve("pi", &env, Some(&provider), None) else {
        return;
    };
    let snapshot = snapshot_from_headers(&headers, now_ms);
    {
        let mut forwarded = FORWARDED.lock();
        forwarded
            .providers
            .insert(session_id.to_owned(), provider.clone());
        if let Some(snapshot) = &snapshot {
            forwarded.snapshots.insert(key.clone(), snapshot.clone());
        }
    }
    if registry.binding(session_id).as_ref() != Some(&key) {
        registry.bind(session_id, key.clone());
    }
    if let Some(snapshot) = snapshot {
        registry.observe(&key, snapshot);
    }
}

/// The `pi-event` OSC verb: decode and receive. Called from the PTY reader.
pub(crate) fn receive_event(
    registry: &QuotaRegistry,
    session_id: &str,
    payload: &str,
    owner: Option<&str>,
) {
    if let Some(event) = decode(payload, owner) {
        receive(registry, session_id, event, SystemClock.now_ms());
    }
}

pub(super) struct Pi;

fn auth_file(key: &SubscriptionKey) -> PathBuf {
    Path::new(&key.credential).join("auth.json")
}

impl QuotaOwner for Pi {
    fn owner(&self) -> &'static str {
        "pi"
    }

    fn observed(&self, key: &SubscriptionKey) -> Option<QuotaSnapshot> {
        FORWARDED.lock().snapshots.get(key).cloned()
    }

    /// Metadata only: the file holds tokens, and any change may end an auth block.
    fn credential_fingerprint(&self, key: &SubscriptionKey) -> Option<Fingerprint> {
        let meta = std::fs::metadata(auth_file(key)).ok()?;
        let modified = meta
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?;
        Some(format!("{}:{}", meta.len(), modified.as_nanos()))
    }

    fn read<'a>(
        &'a self,
        key: &'a SubscriptionKey,
    ) -> BoxFuture<'a, Result<QuotaSnapshot, QuotaError>> {
        Box::pin(async move {
            let auth = read_json(&auth_file(key))?;
            let request = endpoint_request(&key.provider, &auth[key.provider.as_str()], now())?;
            let headers: Vec<(&str, &str)> = request
                .headers
                .iter()
                .map(|(name, value)| (*name, value.as_str()))
                .collect();
            let body = get_json(request.url, &request.token, &headers).await?;
            match key.provider.as_str() {
                "anthropic" => super::adapters::parse_claude_usage(&body, now()),
                _ => super::adapters::parse_codex_usage(&body, now()),
            }
        })
    }
}

fn now() -> u64 {
    SystemClock.now_ms()
}

/// One direct non-generating read for a pi credential entry.
#[derive(Debug, PartialEq)]
pub(super) struct EndpointRequest {
    pub url: &'static str,
    pub token: String,
    pub headers: Vec<(&'static str, String)>,
}

/// Select the endpoint for one `auth.json` entry (`{type, access, expires,
/// accountId?}`), with the expiry pre-check. Pure, so tests never touch the
/// network or a real login file.
pub(super) fn endpoint_request(
    provider: &str,
    entry: &Value,
    now_ms: u64,
) -> Result<EndpointRequest, QuotaError> {
    if entry.is_null() {
        return Err(QuotaError::Unavailable("login not found".into()));
    }
    if entry["type"] != "oauth" {
        return Err(QuotaError::Unavailable("no subscription login".into()));
    }
    if entry["expires"]
        .as_i64()
        .is_some_and(|ms| ms > 0 && ms as u64 <= now_ms)
    {
        return Err(QuotaError::Auth("pi login expired".into()));
    }
    let token = entry["access"]
        .as_str()
        .filter(|t| !t.is_empty())
        .ok_or_else(|| QuotaError::Auth("pi token missing".into()))?
        .to_owned();
    match provider {
        "anthropic" => Ok(EndpointRequest {
            url: "https://api.anthropic.com/api/oauth/usage",
            headers: vec![("anthropic-beta", "oauth-2025-04-20".into())],
            token,
        }),
        "openai-codex" => {
            let account = entry["accountId"]
                .as_str()
                .filter(|a| !a.is_empty())
                .ok_or_else(|| QuotaError::Auth("pi Codex account missing".into()))?
                .to_owned();
            Ok(EndpointRequest {
                url: "https://chatgpt.com/backend-api/wham/usage",
                headers: vec![("ChatGPT-Account-Id", account)],
                token,
            })
        }
        // DEFERRED (2026-11-14) — GitHub Copilot quota. S6 found no recorded
        // non-generating endpoint: pi itself reads none, and no pi Copilot
        // login exists here to record one. Add it with a recorded response.
        "github-copilot" => Err(QuotaError::Unavailable(
            "no Copilot quota endpoint recorded".into(),
        )),
        _ => Err(QuotaError::Unavailable(
            "provider has no quota source".into(),
        )),
    }
}

#[cfg(test)]
#[path = "pi_tests.rs"]
mod tests;
