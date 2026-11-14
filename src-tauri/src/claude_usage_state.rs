//! Persistence of the Claude usage cache and 429 cooldown across restarts.

use super::*;

// The 5-minute cache and the 429 cooldown live in memory, so every restart of the
// app polled the shared OAuth quota at once. This file keeps both across restarts,
// as wall-clock times that are turned back into `Instant`s on load.
const USAGE_STATE_FILENAME: &str = "claude-usage-state.json";

pub(super) fn usage_state_path() -> PathBuf {
    crate::config::config_dir().join(USAGE_STATE_FILENAME)
}

#[derive(Serialize, Deserialize)]
pub(super) struct PersistedProfile {
    fetched_at_ms: Option<u64>,
    data: Option<UsageApiResponse>,
    plan: Option<PlanInfo>,
    meta: Option<RateLimitMeta>,
    pub(super) backoff_until_ms: Option<u64>,
}

/// Serialises read-modify-write of the state file between threads.
static USAGE_STATE_IO: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

/// Profiles whose state was already loaded from disk in this process.
static USAGE_STATE_HYDRATED: std::sync::LazyLock<parking_lot::Mutex<HashSet<CredentialProfile>>> =
    std::sync::LazyLock::new(|| parking_lot::Mutex::new(HashSet::new()));

pub(super) fn profile_state_key(profile: &CredentialProfile) -> String {
    match profile {
        CredentialProfile::ApiKey => "api-key".into(),
        CredentialProfile::Default => "default".into(),
        CredentialProfile::ConfigDir(dir) => format!("dir:{dir}"),
    }
}

pub(super) fn unix_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

pub(super) fn read_usage_state(path: &Path) -> HashMap<String, PersistedProfile> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

/// Best-effort: a failed write only costs one extra request after the next restart.
pub(super) fn persist_profile(path: &Path, profile: &CredentialProfile) {
    let now_ms = unix_now_ms();
    let entry = {
        let cache = API_CACHE.lock();
        let cached = cache.get(profile);
        let until = RATE_LIMITED_UNTIL.lock().get(profile).copied();
        PersistedProfile {
            fetched_at_ms: cached
                .map(|c| now_ms.saturating_sub(c.fetched_at.elapsed().as_millis() as u64)),
            plan: cached.and_then(|c| c.data.plan.clone()),
            meta: cached.and_then(|c| c.data.meta.clone()),
            data: cached.map(|c| c.data.clone()),
            backoff_until_ms: until
                .map(|u| now_ms + u.saturating_duration_since(Instant::now()).as_millis() as u64),
        }
    };
    let _io = USAGE_STATE_IO.lock();
    let mut state = read_usage_state(path);
    state.insert(profile_state_key(profile), entry);
    match serde_json::to_string(&state) {
        Ok(json) => {
            if let Err(e) = std::fs::write(path, json) {
                tracing::warn!(
                    source = "claude_usage",
                    "Failed to persist usage state: {e}"
                );
            }
        }
        Err(e) => tracing::warn!(
            source = "claude_usage",
            "Failed to serialize usage state: {e}"
        ),
    }
}

/// Fill memory from disk for a profile this process has not fetched yet.
pub(super) fn hydrate_profile(path: &Path, profile: &CredentialProfile) {
    if API_CACHE.lock().contains_key(profile) || RATE_LIMITED_UNTIL.lock().contains_key(profile) {
        return;
    }
    let _io = USAGE_STATE_IO.lock();
    let Some(entry) = read_usage_state(path).remove(&profile_state_key(profile)) else {
        return;
    };
    let now_ms = unix_now_ms();
    if let (Some(fetched_ms), Some(mut data)) = (entry.fetched_at_ms, entry.data) {
        // `plan` and `meta` are never deserialized from a body; restore them here.
        data.plan = entry.plan;
        data.meta = entry.meta;
        let age = Duration::from_millis(now_ms.saturating_sub(fetched_ms));
        let fetched_at = Instant::now().checked_sub(age).unwrap_or_else(Instant::now);
        API_CACHE
            .lock()
            .insert(profile.clone(), ApiCache { data, fetched_at });
    }
    if let Some(until_ms) = entry.backoff_until_ms
        && until_ms > now_ms
    {
        RATE_LIMITED_UNTIL.lock().insert(
            profile.clone(),
            Instant::now() + Duration::from_millis(until_ms - now_ms),
        );
    }
}

pub(super) fn hydrate_profile_once(profile: &CredentialProfile) {
    if USAGE_STATE_HYDRATED.lock().insert(profile.clone()) {
        hydrate_profile(&usage_state_path(), profile);
    }
}
