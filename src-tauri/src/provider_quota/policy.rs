//! Pure freshness and failure policy, shared by every quota owner.
//!
//! No clock, lock or IO here: the registry passes `now_ms` in, so every rule
//! below is testable with plain values.

use super::{Fingerprint, QuotaError, QuotaSnapshot, QuotaState};

/// Minimum and maximum cooldown after a 429, in seconds.
pub(crate) const RATE_LIMIT_MIN_SECS: u64 = 120;
pub(crate) const RATE_LIMIT_MAX_SECS: u64 = 1800;
/// First and maximum cooldown after a network failure, in seconds.
pub(crate) const NETWORK_FIRST_SECS: u64 = 60;
pub(crate) const NETWORK_MAX_SECS: u64 = 600;

/// Auth state of a key after a 401/403.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) enum AuthBlock {
    /// No auth failure recorded.
    #[default]
    Clear,
    /// Blocked; the owner had no fingerprint at the failure.
    Blocked,
    /// Blocked at this credential fingerprint; a different one unblocks.
    BlockedAt(Fingerprint),
}

impl AuthBlock {
    pub(crate) fn is_blocked(&self) -> bool {
        !matches!(self, Self::Clear)
    }
}

/// Everything the registry knows about one subscription key.
#[derive(Debug, Clone, Default)]
pub(crate) struct Entry {
    pub snapshot: Option<QuotaSnapshot>,
    /// No active read before this wall-clock instant (ms since the epoch).
    pub backoff_until_ms: Option<u64>,
    pub network_failures: u32,
    pub auth: AuthBlock,
    pub contract_error: Option<String>,
    pub unavailable: Option<String>,
    pub last_forced_ms: Option<u64>,
}

/// What an active read is allowed to do now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReadDecision {
    Skip,
    Read,
    /// Blocked by an auth failure: read only if the credential changed.
    CheckFingerprint,
}

pub(crate) fn is_fresh(entry: &Entry, now_ms: u64, cadence_ms: u64) -> bool {
    entry
        .snapshot
        .as_ref()
        .is_some_and(|s| now_ms.saturating_sub(s.observed_at_ms) < cadence_ms)
}

fn in_backoff(entry: &Entry, now_ms: u64) -> bool {
    entry.backoff_until_ms.is_some_and(|until| now_ms < until)
}

pub(crate) fn decide(entry: &Entry, now_ms: u64, cadence_ms: u64, force: bool) -> ReadDecision {
    if in_backoff(entry, now_ms) {
        return ReadDecision::Skip;
    }
    if entry.auth.is_blocked() {
        return ReadDecision::CheckFingerprint;
    }
    if !force && is_fresh(entry, now_ms, cadence_ms) {
        return ReadDecision::Skip;
    }
    ReadDecision::Read
}

/// Newest value wins, whatever its source. Returns true when stored.
/// A newer value proves the credential works, so it clears an auth block
/// and a contract error; a running rate-limit cooldown still applies.
pub(crate) fn accept(entry: &mut Entry, mut snapshot: QuotaSnapshot) -> bool {
    if let Some(current) = entry.snapshot.as_ref() {
        let keep_current_detail = match (&current.detail, &snapshot.detail) {
            (Some(old), Some(new)) => old.observed_at_ms() > new.observed_at_ms(),
            (Some(_), None) => true,
            _ => false,
        };
        if current.observed_at_ms >= snapshot.observed_at_ms {
            if !keep_current_detail && let Some(detail) = snapshot.detail {
                let current = entry.snapshot.as_mut().expect("snapshot present");
                current.detail = Some(detail);
                entry.auth = AuthBlock::Clear;
                entry.contract_error = None;
                entry.unavailable = None;
                return true;
            }
            return false;
        }
        if keep_current_detail {
            snapshot.detail = current.detail.clone();
        }
    }
    entry.snapshot = Some(snapshot);
    entry.auth = AuthBlock::Clear;
    entry.contract_error = None;
    entry.unavailable = None;
    true
}

/// Record the outcome of one active read.
pub(crate) fn apply(
    entry: &mut Entry,
    result: Result<QuotaSnapshot, QuotaError>,
    now_ms: u64,
    cadence_ms: u64,
    fingerprint: Option<Fingerprint>,
) {
    match result {
        Ok(snapshot) => {
            entry.backoff_until_ms = None;
            entry.network_failures = 0;
            entry.auth = AuthBlock::Clear;
            entry.contract_error = None;
            entry.unavailable = None;
            accept(entry, snapshot);
        }
        Err(QuotaError::RateLimited { retry_after_secs }) => {
            let secs = retry_after_secs
                .unwrap_or(RATE_LIMIT_MIN_SECS)
                .clamp(RATE_LIMIT_MIN_SECS, RATE_LIMIT_MAX_SECS);
            entry.backoff_until_ms = Some(now_ms + secs * 1000);
        }
        Err(QuotaError::Auth(_)) => {
            entry.auth = fingerprint.map_or(AuthBlock::Blocked, AuthBlock::BlockedAt);
        }
        Err(QuotaError::Network(_)) => {
            entry.network_failures = entry.network_failures.saturating_add(1);
            let shift = (entry.network_failures - 1).min(4);
            let secs = (NETWORK_FIRST_SECS << shift).min(NETWORK_MAX_SECS);
            entry.backoff_until_ms = Some(now_ms + secs * 1000);
        }
        Err(QuotaError::Contract(message)) => {
            // A changed schema must not keep showing an old number as current.
            entry.snapshot = None;
            entry.contract_error = Some(message);
            entry.backoff_until_ms = Some(now_ms + cadence_ms);
        }
        Err(QuotaError::Unavailable(reason)) => {
            entry.snapshot = None;
            entry.auth = AuthBlock::Clear;
            entry.contract_error = None;
            entry.unavailable = Some(reason);
            entry.backoff_until_ms = Some(now_ms + cadence_ms);
        }
        Err(QuotaError::Unsupported) => {
            entry.unavailable = Some("no active source".into());
            entry.backoff_until_ms = Some(now_ms + cadence_ms);
        }
    }
}

pub(crate) fn state(entry: &Entry, now_ms: u64, cadence_ms: u64) -> QuotaState {
    if entry.auth.is_blocked() {
        return QuotaState::AuthInvalid;
    }
    if let Some(message) = &entry.contract_error {
        return QuotaState::ContractError {
            message: message.clone(),
        };
    }
    if is_fresh(entry, now_ms, cadence_ms) {
        return QuotaState::Fresh;
    }
    if let Some(until_ms) = entry.backoff_until_ms.filter(|until| now_ms < *until)
        && entry.unavailable.is_none()
    {
        return QuotaState::Backoff { until_ms };
    }
    if entry.snapshot.is_some() {
        return QuotaState::Stale;
    }
    QuotaState::Unavailable {
        reason: entry
            .unavailable
            .clone()
            .unwrap_or_else(|| "no value yet".into()),
    }
}
