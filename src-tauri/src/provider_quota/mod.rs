//! Provider quota registry: one entry per subscription, one policy for every owner.
//!
//! A quota belongs to a credential, and the program that owns the credential
//! (`claude-code`, `codex-cli`, `grok-cli`, `gemini-cli`, `ego`, `pi`) is its
//! reader. Each owner is one [`QuotaOwner`] adapter with up to two sources:
//!
//! - **passive** ([`QuotaOwner::observed`] or [`QuotaRegistry::observe`]): a
//!   value the harness already received with a model response. No request.
//! - **active** ([`QuotaOwner::read`]): one non-generating network read, used
//!   only when the newest value is older than the cadence.
//!
//! The registry keeps the newest snapshot per [`SubscriptionKey`], whatever its
//! source. Sessions bind to keys; only bound keys are scheduled. Reads are
//! single-flight per key. [`policy`] owns the failure rules (429, 401/403,
//! network, contract). [`QuotaRegistry::get`] and [`QuotaRegistry::all`]
//! never cause network traffic, so a tab switch costs nothing.
//!
//! Active provider details are additive and non-secret. Claude retains its
//! extra-usage and local plan metadata; Codex retains allowed/reached flags,
//! credits flags and model availability. Detail has its own observation time:
//! newer passive windows do not erase the last active detail. Dashboard reads
//! use `ensure_detail`, sharing the same owner, flight and failure policy as
//! scheduler reads. Existing usage endpoints project the typed detail without
//! changing their response shapes. Codex token statistics use a separate RPC.
//!
//! Plan: `plans/provider-quota-registry.md` (steps 1-10).

// DEFERRED (2026-11-12) — the adapters (2988-b7bf), bindings (2989-9fa6) and
// transport (2990-4345) are the first non-test callers of this API. Remove
// this allow when they land; until then only the scheduler and tests use it.
#![allow(dead_code)]

pub(crate) mod adapters;
mod binding;
mod ego;
pub(crate) use ego::observe_usage_report as observe_ego_usage_report;
pub(crate) mod pi;
pub(crate) mod policy;
pub(crate) mod status_line;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use policy::{Entry, ReadDecision};

pub(crate) type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Opaque value that changes when a credential changes (file mtime, token hash).
pub(crate) type Fingerprint = String;

/// One subscription: who holds the credential, for which provider, where.
/// Two sessions on the same credential share one key and one read.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) struct SubscriptionKey {
    /// Credential owner: `claude-code`, `codex-cli`, `grok-cli`, `gemini-cli`, `ego`, `pi`.
    pub owner: String,
    /// Subscription provider: `anthropic`, `openai-codex`, `xai`, `google`, `github-copilot`.
    pub provider: String,
    /// Credential location that identifies the account for this owner
    /// (config dir, `CODEX_HOME`, `EGO_HOME`, auth file). Never a secret.
    pub credential: String,
    /// Named CLI profile when it changes the live configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
}

impl SubscriptionKey {
    pub(crate) fn new(
        owner: impl Into<String>,
        provider: impl Into<String>,
        credential: impl Into<String>,
    ) -> Self {
        Self {
            owner: owner.into(),
            provider: provider.into(),
            credential: credential.into(),
            profile: None,
        }
    }
}

impl fmt::Display for SubscriptionKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.owner, self.provider, self.credential)?;
        if let Some(profile) = &self.profile {
            write!(f, ":profile={profile}")?;
        }
        Ok(())
    }
}

/// Window slot, decided by duration and never by a provider's field name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WindowKind {
    FiveHour,
    Weekly,
    Monthly,
    Other,
}

const HOUR: i64 = 3600;
const DAY: i64 = 86_400;

/// 3-8 h five-hour, 5-9 d weekly, 27-32 d monthly, anything else other.
pub(crate) fn classify(window_seconds: Option<i64>) -> WindowKind {
    match window_seconds {
        Some(s) if (3 * HOUR..=8 * HOUR).contains(&s) => WindowKind::FiveHour,
        Some(s) if (5 * DAY..=9 * DAY).contains(&s) => WindowKind::Weekly,
        Some(s) if (27 * DAY..=32 * DAY).contains(&s) => WindowKind::Monthly,
        _ => WindowKind::Other,
    }
}

fn label(kind: WindowKind, window_seconds: Option<i64>) -> String {
    match (kind, window_seconds) {
        (WindowKind::FiveHour, _) => "5h".into(),
        (WindowKind::Weekly, _) => "7d".into(),
        (WindowKind::Monthly, _) => "30d".into(),
        (WindowKind::Other, Some(s)) if s >= DAY => format!("{}d", (s + DAY / 2) / DAY),
        (WindowKind::Other, Some(s)) if s >= HOUR => format!("{}h", (s + HOUR / 2) / HOUR),
        (WindowKind::Other, Some(s)) => format!("{}m", ((s + 30) / 60).max(1)),
        (WindowKind::Other, None) => "credits".into(),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct QuotaWindow {
    pub kind: WindowKind,
    /// Short display label derived from the duration (`5h`, `7d`, `2d`, `credits`).
    pub label: String,
    /// Percent used. May exceed 100 for spend limits.
    pub used_percent: f64,
    /// RFC 3339 reset time.
    pub resets_at: Option<String>,
    pub window_seconds: Option<i64>,
    /// Model or product the window applies to; `None` for account-wide windows.
    pub model: Option<String>,
}

impl QuotaWindow {
    /// The only constructor: adapters pass the duration, the registry names it.
    /// Returns `None` for a non-finite or negative percentage.
    pub(crate) fn from_duration(
        used_percent: f64,
        window_seconds: Option<i64>,
        resets_at: Option<String>,
        model: Option<String>,
    ) -> Option<Self> {
        if !used_percent.is_finite() || used_percent < 0.0 {
            return None;
        }
        let window_seconds = window_seconds.filter(|s| *s > 0);
        let kind = classify(window_seconds);
        Some(Self {
            kind,
            label: label(kind, window_seconds),
            used_percent,
            resets_at,
            window_seconds,
            model,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum QuotaSource {
    Passive,
    Active,
}

/// Provider-specific active data kept independently of passive quota windows.
/// It contains no credentials; its timestamp describes the active read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub(crate) enum QuotaDetail {
    Claude {
        observed_at_ms: u64,
        response: crate::claude_usage::UsageApiResponse,
    },
    Codex {
        observed_at_ms: u64,
        response: crate::codex_usage::CodexUsageApiResponse,
    },
}
impl QuotaDetail {
    pub(crate) fn observed_at_ms(&self) -> u64 {
        match self {
            Self::Claude { observed_at_ms, .. } | Self::Codex { observed_at_ms, .. } => {
                *observed_at_ms
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct QuotaSnapshot {
    pub windows: Vec<QuotaWindow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<QuotaDetail>,
    pub plan: Option<String>,
    /// Remaining credits or balance as reported, unparsed.
    pub credits: Option<String>,
    /// Non-secret account id when the source reports one.
    pub account: Option<String>,
    /// When the provider produced the value (ms since the epoch), not when it was read.
    pub observed_at_ms: u64,
    pub source: QuotaSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(crate) enum QuotaState {
    /// Newest value is younger than the cadence.
    Fresh,
    /// A value exists but is older than the cadence; show it with its age.
    Stale,
    /// No value and no way to get one now.
    Unavailable { reason: String },
    /// 401/403: sign in again. No read until the credential changes.
    AuthInvalid,
    /// Rate-limited or failing; the snapshot, if any, is the last good value.
    Backoff { until_ms: u64 },
    /// The response no longer matches the parser; no stale value is shown.
    ContractError { message: String },
}

/// What every consumer (status bar, mobile, dashboards, transport) renders.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct QuotaView {
    pub key: SubscriptionKey,
    /// Initial client binding inventory; subsequent changes have their own event.
    pub session_ids: Vec<String>,
    pub snapshot: Option<QuotaSnapshot>,
    #[serde(flatten)]
    pub state: QuotaState,
}

/// Outcome classes of an active read. Adapters map provider errors onto these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum QuotaError {
    RateLimited {
        retry_after_secs: Option<u64>,
    },
    Auth(String),
    Network(String),
    Contract(String),
    /// This owner has no active source for the key.
    Unsupported,
    /// The configured source cannot be read without changing identity.
    Unavailable(String),
}

/// Port implemented once per credential owner.
pub(crate) trait QuotaOwner: Send + Sync {
    /// Owner id; must equal [`SubscriptionKey::owner`] of the keys it serves.
    fn owner(&self) -> &'static str;

    /// Newest value the harness already recorded, or `None`. No network.
    /// Called on every scheduler pass for a bound key, so it must be cheap.
    fn observed(&self, _key: &SubscriptionKey) -> Option<QuotaSnapshot> {
        None
    }

    /// A cheap identity precondition that must override a cached value.
    fn unavailable_reason(&self, _key: &SubscriptionKey) -> Option<&'static str> {
        None
    }

    /// One non-generating network read. Must not retry internally: the
    /// registry owns backoff.
    fn read<'a>(
        &'a self,
        key: &'a SubscriptionKey,
    ) -> BoxFuture<'a, Result<QuotaSnapshot, QuotaError>>;

    /// Changes when the credential changes; ends an auth block. Must be cheap.
    fn credential_fingerprint(&self, _key: &SubscriptionKey) -> Option<Fingerprint> {
        None
    }
}

/// Wall clock in ms since the epoch. Injected so tests control time.
pub(crate) trait Clock: Send + Sync {
    fn now_ms(&self) -> u64;
}

pub(crate) struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct QuotaConfig {
    /// A value younger than this is fresh; an older one allows an active read.
    pub cadence: Duration,
    /// Scheduler pass interval.
    pub tick: Duration,
    /// Minimum interval between two forced refreshes of one key.
    pub refresh_min_interval: Duration,
}

impl Default for QuotaConfig {
    fn default() -> Self {
        Self {
            cadence: Duration::from_secs(300),
            tick: Duration::from_secs(60),
            refresh_min_interval: Duration::from_secs(60),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RefreshError {
    /// A forced refresh of this key ran less than `refresh_min_interval` ago.
    TooSoon { retry_at_ms: u64 },
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct BindingChange {
    pub session_id: String,
    pub key: Option<SubscriptionKey>,
}

pub(crate) struct QuotaRegistry {
    clock: Arc<dyn Clock>,
    config: QuotaConfig,
    owners: parking_lot::RwLock<HashMap<&'static str, Arc<dyn QuotaOwner>>>,
    entries: parking_lot::Mutex<HashMap<SubscriptionKey, Entry>>,
    flights: parking_lot::Mutex<HashMap<SubscriptionKey, Arc<tokio::sync::Mutex<()>>>>,
    bindings: parking_lot::Mutex<HashMap<String, SubscriptionKey>>,
    changes: broadcast::Sender<QuotaView>,
    binding_changes: broadcast::Sender<BindingChange>,
}

impl QuotaRegistry {
    pub(crate) fn new(clock: Arc<dyn Clock>, config: QuotaConfig) -> Self {
        Self {
            clock,
            config,
            owners: Default::default(),
            entries: Default::default(),
            flights: Default::default(),
            bindings: Default::default(),
            changes: broadcast::channel(64).0,
            binding_changes: broadcast::channel(64).0,
        }
    }

    pub(crate) fn config(&self) -> QuotaConfig {
        self.config
    }

    fn cadence_ms(&self) -> u64 {
        self.config.cadence.as_millis() as u64
    }

    /// Register (or replace) the adapter for one owner id.
    pub(crate) fn register_owner(&self, owner: Arc<dyn QuotaOwner>) {
        self.owners.write().insert(owner.owner(), owner);
    }

    /// Bind a session to the key of the credential it uses. Rebinding replaces
    /// the old key. Returns true when no other session had this key, so the
    /// caller can `ensure` it once instead of waiting for the next pass.
    pub(crate) fn bind(&self, session_id: &str, key: SubscriptionKey) -> bool {
        let mut bindings = self.bindings.lock();
        let newly_bound = !bindings
            .iter()
            .any(|(session, bound)| *bound == key && session != session_id);
        let previous = bindings.insert(session_id.to_owned(), key.clone());
        if previous.as_ref() != Some(&key) {
            let _ = self.binding_changes.send(BindingChange {
                session_id: session_id.to_owned(),
                key: Some(key.clone()),
            });
        }
        drop(bindings);
        self.entries.lock().entry(key).or_default();
        newly_bound
    }

    pub(crate) fn unbind(&self, session_id: &str) -> Option<SubscriptionKey> {
        let mut bindings = self.bindings.lock();
        let previous = bindings.remove(session_id);
        if previous.is_some() {
            let _ = self.binding_changes.send(BindingChange {
                session_id: session_id.to_owned(),
                key: None,
            });
        }
        previous
    }

    pub(crate) fn subscribe_bindings(&self) -> broadcast::Receiver<BindingChange> {
        self.binding_changes.subscribe()
    }

    pub(crate) fn binding(&self, session_id: &str) -> Option<SubscriptionKey> {
        self.bindings.lock().get(session_id).cloned()
    }

    /// Distinct keys bound to at least one session, sorted.
    pub(crate) fn bound_keys(&self) -> Vec<SubscriptionKey> {
        let mut keys: Vec<_> = self.bindings.lock().values().cloned().collect();
        keys.sort();
        keys.dedup();
        keys
    }

    /// Current view. Never touches the network.
    pub(crate) fn get(&self, key: &SubscriptionKey) -> QuotaView {
        let now = self.clock.now_ms();
        let entries = self.entries.lock();
        let entry = entries.get(key).cloned().unwrap_or_default();
        self.view_of(key, &entry, now)
    }

    /// Views of every known key, sorted by key. Never touches the network.
    pub(crate) fn all(&self) -> Vec<QuotaView> {
        let now = self.clock.now_ms();
        let entries = self.entries.lock();
        let mut views: Vec<_> = entries
            .iter()
            .map(|(key, entry)| self.view_of(key, entry, now))
            .collect();
        views.sort_by(|a, b| a.key.cmp(&b.key));
        views
    }

    /// Every view change, for transport (Tauri emit and SSE).
    pub(crate) fn subscribe(&self) -> broadcast::Receiver<QuotaView> {
        self.changes.subscribe()
    }

    /// Push a passive value (status line relay, pi channel). Newest wins.
    /// Returns true when the value was stored.
    pub(crate) fn observe(&self, key: &SubscriptionKey, snapshot: QuotaSnapshot) -> bool {
        self.mutate(key, |entry| policy::accept(entry, snapshot))
    }

    /// Restore an existing on-disk owner cooldown once, before its first read.
    /// Storage adapters must not use this to override live policy.
    pub(crate) fn restore_backoff(&self, key: &SubscriptionKey, until_ms: u64) {
        self.mutate(key, |entry| {
            if entry.backoff_until_ms.is_none() && until_ms > self.clock.now_ms() {
                entry.backoff_until_ms = Some(until_ms);
                true
            } else {
                false
            }
        });
    }

    /// Bring one key up to date: pull the passive value, then read actively
    /// only if the policy allows. Single-flight per key.
    pub(crate) async fn ensure(&self, key: &SubscriptionKey) -> QuotaView {
        self.ensure_inner(key, false, false).await
    }

    /// Ensure provider-specific active detail exists and is fresh. Passive
    /// windows do not satisfy this request, but share the same flight and policy.
    pub(crate) async fn ensure_detail(&self, key: &SubscriptionKey) -> QuotaView {
        self.ensure_inner(key, false, true).await
    }

    fn detail_is_stale(&self, key: &SubscriptionKey) -> bool {
        let now = self.clock.now_ms();
        self.entries
            .lock()
            .get(key)
            .and_then(|entry| entry.snapshot.as_ref())
            .and_then(|snapshot| snapshot.detail.as_ref())
            .is_none_or(|detail| now.saturating_sub(detail.observed_at_ms()) >= self.cadence_ms())
    }

    /// Forced read that ignores freshness (not backoff or auth), at most once
    /// per `refresh_min_interval` per key.
    pub(crate) async fn refresh(&self, key: &SubscriptionKey) -> Result<QuotaView, RefreshError> {
        let now = self.clock.now_ms();
        let min = self.config.refresh_min_interval.as_millis() as u64;
        {
            let mut entries = self.entries.lock();
            let entry = entries.entry(key.clone()).or_default();
            if let Some(last) = entry.last_forced_ms
                && now.saturating_sub(last) < min
            {
                return Err(RefreshError::TooSoon {
                    retry_at_ms: last + min,
                });
            }
            entry.last_forced_ms = Some(now);
        }
        Ok(self.ensure_inner(key, true, false).await)
    }

    /// One scheduler pass over the bound keys.
    pub(crate) async fn tick(&self) {
        let keys = self.bound_keys();
        futures_util::future::join_all(keys.iter().map(|key| self.ensure(key))).await;
    }

    async fn ensure_inner(&self, key: &SubscriptionKey, force: bool, detail: bool) -> QuotaView {
        let owner = self.owners.read().get(key.owner.as_str()).cloned();
        let Some(owner) = owner else {
            let reason = Some(format!("no quota owner `{}`", key.owner));
            self.mutate(key, |entry| {
                let changed = entry.unavailable != reason;
                entry.unavailable = reason;
                changed
            });
            return self.get(key);
        };
        if let Some(reason) = owner.unavailable_reason(key) {
            self.mutate(key, |entry| {
                policy::apply(
                    entry,
                    Err(QuotaError::Unavailable(reason.into())),
                    self.clock.now_ms(),
                    self.cadence_ms(),
                    None,
                );
                true
            });
            return self.get(key);
        }
        if let Some(snapshot) = owner.observed(key) {
            self.observe(key, snapshot);
        }
        if detail && !self.detail_is_stale(key) {
            return self.get(key);
        }
        if !self.may_read(
            owner.as_ref(),
            key,
            force || (detail && self.detail_is_stale(key)),
        ) {
            return self.get(key);
        }
        let flight = self.flights.lock().entry(key.clone()).or_default().clone();
        let _guard = flight.lock().await;
        // Whoever held the flight may have refreshed the key meanwhile; a
        // waiting forced caller accepts a value produced within the interval.
        let force = force && !self.produced_within_refresh_interval(key);
        if detail && !self.detail_is_stale(key) {
            return self.get(key);
        }
        if !self.may_read(
            owner.as_ref(),
            key,
            force || (detail && self.detail_is_stale(key)),
        ) {
            return self.get(key);
        }
        let result = owner.read(key).await;
        let fingerprint = matches!(result, Err(QuotaError::Auth(_)))
            .then(|| owner.credential_fingerprint(key))
            .flatten();
        let now = self.clock.now_ms();
        let cadence = self.cadence_ms();
        self.mutate(key, |entry| {
            policy::apply(entry, result, now, cadence, fingerprint);
            true
        });
        self.get(key)
    }

    fn produced_within_refresh_interval(&self, key: &SubscriptionKey) -> bool {
        let now = self.clock.now_ms();
        let min = self.config.refresh_min_interval.as_millis() as u64;
        self.entries.lock().get(key).is_some_and(|entry| {
            entry
                .snapshot
                .as_ref()
                .is_some_and(|s| now.saturating_sub(s.observed_at_ms) < min)
        })
    }

    fn may_read(&self, owner: &dyn QuotaOwner, key: &SubscriptionKey, force: bool) -> bool {
        let now = self.clock.now_ms();
        let cadence = self.cadence_ms();
        let (decision, blocked_at) = {
            let entries = self.entries.lock();
            let entry = entries.get(key).cloned().unwrap_or_default();
            (
                policy::decide(&entry, now, cadence, force),
                entry.auth.clone(),
            )
        };
        match decision {
            ReadDecision::Skip => false,
            ReadDecision::Read => true,
            ReadDecision::CheckFingerprint => {
                let changed = match (&blocked_at, owner.credential_fingerprint(key)) {
                    (policy::AuthBlock::BlockedAt(old), Some(current)) => current != *old,
                    // A credential that appears after the failure is a change.
                    (policy::AuthBlock::Blocked, Some(_)) => true,
                    // No fingerprint now: only a newer passive value unblocks.
                    _ => false,
                };
                if !changed {
                    return false;
                }
                self.mutate(key, |entry| {
                    entry.auth = policy::AuthBlock::Clear;
                    true
                });
                true
            }
        }
    }

    /// Apply `change` to the entry; publish the view when it changed.
    fn mutate(&self, key: &SubscriptionKey, change: impl FnOnce(&mut Entry) -> bool) -> bool {
        let now = self.clock.now_ms();
        let mut entries = self.entries.lock();
        let entry = entries.entry(key.clone()).or_default();
        let before = self.view_of(key, entry, now);
        let changed = change(entry);
        let after = self.view_of(key, entry, now);
        drop(entries);
        if after != before {
            let _ = self.changes.send(after);
        }
        changed
    }

    fn view_of(&self, key: &SubscriptionKey, entry: &Entry, now_ms: u64) -> QuotaView {
        QuotaView {
            key: key.clone(),
            session_ids: {
                let mut ids: Vec<_> = self
                    .bindings
                    .lock()
                    .iter()
                    .filter_map(|(id, bound)| (bound == key).then_some(id.clone()))
                    .collect();
                ids.sort();
                ids
            },
            snapshot: entry.snapshot.clone(),
            state: policy::state(entry, now_ms, self.cadence_ms()),
        }
    }
}

impl Default for QuotaRegistry {
    fn default() -> Self {
        Self::new(Arc::new(SystemClock), QuotaConfig::default())
    }
}

/// Run the scheduler for the app's registry: one pass per `tick`.
pub(crate) fn spawn_scheduler(state: Arc<crate::AppState>) {
    adapters::register(&state.provider_quota);
    let tick = state.provider_quota.config().tick;
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tick);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            binding::reconcile(state.clone()).await;
            state.provider_quota.tick().await;
        }
    });
}

#[cfg(test)]
mod binding_tests;

#[cfg(test)]
mod binding_process_tests;
