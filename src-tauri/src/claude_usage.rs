//! Claude Usage Dashboard — native Rust data layer.
//!
//! Provides three Tauri commands:
//! - `get_claude_usage_api`: Reads OAuth credentials and calls the Anthropic usage API
//! - `get_claude_session_stats`: Scans `~/.claude/projects/*/` JSONL transcripts
//! - `get_claude_project_list`: Lists available project slugs
//!
//! Session stats use a file-size-based incremental cache: only new bytes in
//! append-only JSONL files are parsed. The cache is persisted to disk as JSON
//! in the app config directory so restarts don't require a full rescan.

use serde::{Deserialize, Serialize};
#[cfg(target_os = "macos")]
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
#[cfg(feature = "desktop")]
use tauri::State;

// ---------------------------------------------------------------------------
// API types (from Anthropic OAuth usage endpoint)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RateBucket {
    pub utilization: f64,
    /// Reset timestamp — nullable when no active rate limit window.
    pub resets_at: Option<String>,
}

impl Default for RateBucket {
    fn default() -> Self {
        Self {
            utilization: 0.0,
            resets_at: None,
        }
    }
}

/// "Extra usage" (overage) bucket — consumption beyond the plan quota.
/// The Anthropic `/api/oauth/usage` endpoint returns the *credits* fields; the
/// `/v1/messages` response headers provide `resets_at` and `in_use`. Both paths
/// feed the same struct so the frontend sees a unified view.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ExtraUsage {
    pub is_enabled: bool,
    /// Monthly credit allowance (e.g. 17000).
    pub monthly_limit: Option<u64>,
    /// Credits consumed this month (float — the API returns e.g. 665.0).
    pub used_credits: Option<f64>,
    /// Direct percentage 0-100 of `used_credits / monthly_limit`.
    pub utilization: Option<f64>,
    /// ISO8601 reset time (from overage-reset header, not from API body).
    #[serde(skip_deserializing)]
    pub resets_at: Option<String>,
    /// True when overage is actively being consumed (from overage-in-use header).
    #[serde(skip_deserializing)]
    pub in_use: bool,
}

/// Metadata about the overall rate-limit state — populated from
/// `anthropic-ratelimit-unified-*` response headers on `/v1/messages`.
/// The primary `/api/oauth/usage` endpoint does not return these fields.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RateLimitMeta {
    /// Overall request status: "allowed" / "allowed_warning" / "rejected".
    pub unified_status: Option<String>,
    /// Which bucket is the active constraint ("five_hour", "seven_day", …).
    pub representative_claim: Option<String>,
}

/// Plan / subscription info extracted from the local OAuth credentials file.
/// NOT fetched from any remote endpoint — lives in the macOS Keychain or
/// `~/.claude/.credentials.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlanInfo {
    pub subscription_type: Option<String>,
    pub rate_limit_tier: Option<String>,
    pub scopes: Vec<String>,
}

/// API response from Anthropic OAuth usage endpoint.
/// Uses `deny_unknown_fields` = false (default) to tolerate new API fields.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct UsageApiResponse {
    pub five_hour: Option<RateBucket>,
    pub seven_day: Option<RateBucket>,
    pub seven_day_oauth_apps: Option<RateBucket>,
    pub seven_day_opus: Option<RateBucket>,
    pub seven_day_sonnet: Option<RateBucket>,
    pub seven_day_cowork: Option<RateBucket>,
    pub extra_usage: Option<ExtraUsage>,
    /// Injected by the backend from local credentials — never deserialized from the API body.
    #[serde(skip_deserializing)]
    pub plan: Option<PlanInfo>,
    /// Injected by the backend from response headers — never deserialized from the API body.
    #[serde(skip_deserializing)]
    pub meta: Option<RateLimitMeta>,
}

// ---------------------------------------------------------------------------
// Usage timeline types (reconstructed from session transcripts)
// ---------------------------------------------------------------------------

/// Hourly token usage bucket, stored per-file in the cache.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HourlyTokens {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub message_count: u32,
}

/// Aggregated timeline point returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelinePoint {
    /// Hour key in "YYYY-MM-DDTHH" format (e.g. "2026-02-04T10")
    pub hour: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// Info extracted from a single JSONL line for cross-line correlation.
struct LineInfo {
    /// Timestamp from `system/turn_duration` lines
    timestamp: Option<String>,
    /// `(input_tokens, output_tokens)` from `assistant` lines
    assistant_tokens: Option<(u64, u64)>,
}

// ---------------------------------------------------------------------------
// Session stats types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelTokens {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    pub message_count: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DayStats {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub message_count: u32,
    pub session_count: u32,
}

/// Per-file cached stats — stored in the persistent cache.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CachedFileStats {
    pub file_size: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_creation_tokens: u64,
    pub total_cache_read_tokens: u64,
    pub assistant_message_count: u32,
    pub user_message_count: u32,
    pub model_usage: HashMap<String, ModelTokens>,
    pub daily_activity: HashMap<String, DayStats>,
    /// Unique session IDs seen in this file
    pub session_ids: HashSet<String>,
    pub first_timestamp: Option<String>,
    pub last_timestamp: Option<String>,
    /// Token usage bucketed by hour ("YYYY-MM-DDTHH" → HourlyTokens)
    #[serde(default)]
    pub hourly_tokens: HashMap<String, HourlyTokens>,
}

/// Aggregated stats returned to the frontend.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionStats {
    pub total_sessions: u32,
    pub total_assistant_messages: u32,
    pub total_user_messages: u32,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_creation_tokens: u64,
    pub total_cache_read_tokens: u64,
    pub model_usage: HashMap<String, ModelTokens>,
    pub daily_activity: HashMap<String, DayStats>,
    pub per_project: HashMap<String, ProjectStats>,
    /// Per-project daily breakdown: project_slug → date → DayStats.
    /// Used by the heatmap tooltip to show top projects per day.
    pub per_project_daily: HashMap<String, HashMap<String, DayStats>>,
    /// Number of distinct hours with activity (for tokens-per-hour calculation).
    #[serde(default)]
    pub active_hours: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectStats {
    pub session_count: u32,
    pub assistant_message_count: u32,
    pub user_message_count: u32,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// Project entry for the dropdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectEntry {
    pub slug: String,
    pub session_count: usize,
    /// Resolved filesystem path (if available), for display purposes.
    pub display_path: Option<String>,
}

// ---------------------------------------------------------------------------
// Cache
// ---------------------------------------------------------------------------

/// In-memory cache: project_slug → (file_name → CachedFileStats)
pub type SessionStatsCache = HashMap<String, HashMap<String, CachedFileStats>>;

/// The scan gate serializes refreshes while the independent callback retains
/// events arriving during I/O for the next pass. Only top-level JSONL files
/// contribute usage; subagent transcripts are deliberately outside this cache.
#[derive(Default)]
pub(crate) struct UsageScan {
    root: Option<PathBuf>,
    watcher: Option<notify::RecommendedWatcher>,
    events: Arc<parking_lot::Mutex<UsageChanges>>,
    seen: HashSet<PathBuf>,
    pending: HashSet<PathBuf>,
    reconciled_at: Option<Instant>,
}

#[derive(Default)]
struct UsageChanges {
    paths: HashSet<PathBuf>,
    rescan: bool,
}

impl UsageScan {
    fn prepare(&mut self, root: &Path) {
        use notify::Watcher;
        if self.root.as_deref() != Some(root) {
            *self = Self::default();
            self.root = Some(root.to_path_buf());
        }
        if self.watcher.is_none() {
            let events = Arc::clone(&self.events);
            let watched_root = root.to_path_buf();
            self.watcher =
                notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                    let mut changes = events.lock();
                    let Ok(event) = event else {
                        changes.rescan = true;
                        return;
                    };
                    if event.kind.is_access() {
                        return;
                    }
                    if event.need_rescan() || event.paths.is_empty() {
                        changes.rescan = true;
                    }
                    for path in event.paths {
                        let Ok(relative) = path.strip_prefix(&watched_root) else {
                            continue;
                        };
                        let depth = relative.components().count();
                        if depth < 2 {
                            changes.rescan = true;
                        } else if depth == 2 && path.extension().is_some_and(|e| e == "jsonl") {
                            changes.paths.insert(path);
                        }
                    }
                    if changes.paths.len() > 16_384 {
                        changes.paths.clear();
                        changes.rescan = true;
                    }
                })
                .ok()
                .and_then(|mut watcher| {
                    watcher.watch(root, notify::RecursiveMode::Recursive).ok()?;
                    Some(watcher)
                });
        }
        let mut changes = self.events.lock();
        self.pending.extend(changes.paths.drain());
        if self.pending.len() > 16_384
            || std::mem::take(&mut changes.rescan)
            || self.watcher.is_none()
            || self
                .reconciled_at
                .is_none_or(|at| at.elapsed() >= Duration::from_secs(30 * 60))
        {
            self.seen.clear();
            self.pending.clear();
            self.reconciled_at = Some(Instant::now());
        }
    }
}

const CACHE_FILENAME: &str = "claude-usage-cache.json";

/// Load the cache from disk, or return an empty map on any error.
pub(crate) fn load_cache_from_disk() -> SessionStatsCache {
    let path = crate::config::config_dir().join(CACHE_FILENAME);
    match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => HashMap::new(),
    }
}

/// Persist the cache to disk. Best-effort — errors are logged but not fatal.
fn save_cache_to_disk(cache: &SessionStatsCache) {
    let path = crate::config::config_dir().join(CACHE_FILENAME);
    match serde_json::to_string(cache) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                tracing::warn!(source = "claude_usage", "Failed to write cache: {e}");
            }
        }
        Err(e) => tracing::warn!(source = "claude_usage", "Failed to serialize cache: {e}"),
    }
}

/// Base directory for Claude session transcripts.
fn claude_projects_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude").join("projects"))
}

/// macOS TCC-protected directories under $HOME that we must never probe.
/// Calling `.exists()` on these triggers the "would like to access" system dialog.
const TCC_PROTECTED_DIRS: &[&str] = &[
    "Desktop",
    "Documents",
    "Downloads",
    "Movies",
    "Music",
    "Pictures",
    "Library",
    "Photos Library.photoslibrary",
];

/// Returns true if `path` is a TCC-protected directory (or inside one).
fn is_tcc_protected(path: &std::path::Path) -> bool {
    let Some(home) = dirs::home_dir() else {
        return false;
    };
    if !path.starts_with(&home) {
        return false;
    }
    if let Ok(rel) = path.strip_prefix(&home)
        && let Some(first) = rel.components().next()
    {
        let name = first.as_os_str().to_string_lossy();
        return TCC_PROTECTED_DIRS
            .iter()
            .any(|d| d.eq_ignore_ascii_case(&name));
    }
    false
}

/// Resolve a Claude Code project slug back to a filesystem path.
///
/// Claude Code creates project slugs by replacing `/`, `.`, `_`, and other
/// special characters with `-`. This is ambiguous, so we greedily match
/// atoms against real directories on disk.
///
/// SAFETY: Never probes macOS TCC-protected directories (Desktop, Documents,
/// Photos, Music, etc.) to avoid triggering system permission dialogs.
///
/// Example: `-Users-stefano-straus-Gits-CC-Playground-tui-commander`
///       → `/Users/stefano.straus/Gits/CC_Playground/tui-commander`
fn resolve_slug_to_path(slug: &str) -> Option<String> {
    // Remove leading dash (represents root `/`)
    let raw = slug.strip_prefix('-').unwrap_or(slug);
    let atoms: Vec<&str> = raw.split('-').collect();
    if atoms.is_empty() {
        return None;
    }

    let mut path = PathBuf::from("/");
    let mut i = 0;

    while i < atoms.len() {
        // Try progressively longer combinations of atoms joined by
        // different separators, checking if the directory exists
        let mut found = false;

        // Try from longest possible segment to shortest (greedy)
        let max_len = (atoms.len() - i).min(8); // cap to avoid pathological cases
        for len in (1..=max_len).rev() {
            let segment_atoms = &atoms[i..i + len];

            // Try different join strategies
            for separator in &["-", ".", "_"] {
                let candidate = segment_atoms.join(separator);
                let candidate_path = path.join(&candidate);
                // Skip TCC-protected directories to avoid macOS permission dialogs
                if is_tcc_protected(&candidate_path) {
                    continue;
                }
                if candidate_path.exists() {
                    path = candidate_path;
                    i += len;
                    found = true;
                    break;
                }
            }
            if found {
                break;
            }

            // Also try plain dash-join (it's a valid char in names)
            if len == 1 {
                // Single atom — just use it directly
                let candidate_path = path.join(atoms[i]);
                // Accept even if it doesn't exist (might be the last segment
                // or the directory was deleted)
                path = candidate_path;
                i += 1;
                found = true;
                break;
            }
        }

        if !found {
            // Shouldn't happen due to len==1 fallback, but just in case
            path.push(atoms[i]);
            i += 1;
        }
    }

    Some(path.to_string_lossy().to_string())
}

// ---------------------------------------------------------------------------
// JSONL parsing
// ---------------------------------------------------------------------------

/// Parse a single JSONL line and accumulate stats.
///
/// Returns a `LineInfo` so the caller can correlate assistant tokens
/// with the timestamp from the next `turn_duration` line.
fn parse_jsonl_line(line: &str, stats: &mut CachedFileStats) -> LineInfo {
    let empty = LineInfo {
        timestamp: None,
        assistant_tokens: None,
    };

    // Fast pre-filter: skip lines that can't contain useful data
    if line.len() < 10 {
        return empty;
    }

    let Some(v) = serde_json::from_str::<serde_json::Value>(line).ok() else {
        return empty;
    };
    let Some(obj) = v.as_object() else {
        return empty;
    };
    let Some(line_type) = obj.get("type").and_then(|t| t.as_str()) else {
        return empty;
    };

    match line_type {
        "assistant" => {
            let Some(message) = obj.get("message").and_then(|m| m.as_object()) else {
                return empty;
            };
            let model = message
                .get("model")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown")
                .to_string();
            let Some(usage) = message.get("usage").and_then(|u| u.as_object()) else {
                return empty;
            };

            let input = usage
                .get("input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let output = usage
                .get("output_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let cache_creation = usage
                .get("cache_creation_input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let cache_read = usage
                .get("cache_read_input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            stats.total_input_tokens += input;
            stats.total_output_tokens += output;
            stats.total_cache_creation_tokens += cache_creation;
            stats.total_cache_read_tokens += cache_read;
            stats.assistant_message_count += 1;

            let model_entry = stats.model_usage.entry(model).or_default();
            model_entry.input_tokens += input;
            model_entry.output_tokens += output;
            model_entry.cache_creation_tokens += cache_creation;
            model_entry.cache_read_tokens += cache_read;
            model_entry.message_count += 1;

            LineInfo {
                timestamp: None,
                assistant_tokens: Some((input, output)),
            }
        }
        "user" => {
            stats.user_message_count += 1;
            empty
        }
        "system" => {
            let subtype = obj.get("subtype").and_then(|s| s.as_str());
            let timestamp = obj
                .get("timestamp")
                .and_then(|t| t.as_str())
                .map(|s| s.to_string());

            if subtype == Some("turn_duration")
                && let Some(ref ts) = timestamp
            {
                // Extract date part "2026-02-04" from ISO timestamp
                let date = &ts[..10.min(ts.len())];
                if date.len() == 10 {
                    let day = stats.daily_activity.entry(date.to_string()).or_default();
                    day.message_count += 1;
                }

                // Track session ID for counting unique sessions
                if obj
                    .get("sessionId")
                    .and_then(|s| s.as_str())
                    .filter(|sid| stats.session_ids.insert(sid.to_string()))
                    .is_some()
                {
                    // Bump session count for the day
                    let date = &ts[..10.min(ts.len())];
                    if date.len() == 10 {
                        let day = stats.daily_activity.entry(date.to_string()).or_default();
                        day.session_count += 1;
                    }
                }
            }

            // Update first/last timestamps for any system line with a timestamp
            if let Some(ref ts) = timestamp {
                if stats.first_timestamp.is_none()
                    || stats.first_timestamp.as_deref() > Some(ts.as_str())
                {
                    stats.first_timestamp = Some(ts.clone());
                }
                if stats.last_timestamp.is_none()
                    || stats.last_timestamp.as_deref() < Some(ts.as_str())
                {
                    stats.last_timestamp = Some(ts.clone());
                }
            }

            // Return timestamp from any system subtype that has one,
            // so the caller can flush pending assistant tokens
            if timestamp.is_some() {
                return LineInfo {
                    timestamp,
                    assistant_tokens: None,
                };
            }
            empty
        }
        _ => empty, // skip "progress", "file-history-snapshot", etc.
    }
}

/// Parse a JSONL file from a given byte offset, appending stats to `existing`.
/// Returns the final file position after parsing.
fn parse_jsonl_file_from_offset(
    path: &Path,
    offset: u64,
    stats: &mut CachedFileStats,
) -> std::io::Result<u64> {
    let file = std::fs::File::open(path)?;
    let metadata = file.metadata()?;
    let file_size = metadata.len();

    if offset >= file_size {
        return Ok(file_size);
    }

    let mut reader = std::io::BufReader::new(file);

    // If resuming mid-file, check whether we're at a line boundary.
    // If the byte before the offset is not a newline, we landed in the
    // middle of a line — skip it. If it IS a newline (or offset is 0),
    // we're at a clean boundary and can parse immediately.
    if offset > 0 {
        reader.seek(SeekFrom::Start(offset - 1))?;
        let mut one_byte = [0u8; 1];
        use std::io::Read;
        reader.read_exact(&mut one_byte)?;
        if one_byte[0] != b'\n' {
            // Mid-line — skip to end of this partial line
            let mut discard = String::new();
            reader.read_line(&mut discard)?;
        }
        // else: already at offset, right after the newline
    } else {
        reader.seek(SeekFrom::Start(0))?;
    }

    // Track pending assistant tokens for correlation with the next turn_duration
    let mut pending_tokens: Option<(u64, u64)> = None;

    let mut line_buf = String::new();
    loop {
        line_buf.clear();
        let bytes_read = reader.read_line(&mut line_buf)?;
        if bytes_read == 0 {
            break;
        }
        let trimmed = line_buf.trim();
        if trimmed.is_empty() {
            continue;
        }

        let info = parse_jsonl_line(trimmed, stats);

        // If this was an assistant line, stash its tokens
        if let Some(tokens) = info.assistant_tokens {
            pending_tokens = Some(tokens);
        }

        // If this line has a timestamp, assign pending tokens
        // to the hourly bucket and daily activity
        if let Some(ref ts) = info.timestamp
            && let Some((input, output)) = pending_tokens.take()
        {
            // Hour key: "2026-02-04T10" (first 13 chars of ISO timestamp)
            if ts.len() >= 13 {
                let hour_key = &ts[..13];
                let hourly = stats.hourly_tokens.entry(hour_key.to_string()).or_default();
                hourly.input_tokens += input;
                hourly.output_tokens += output;
                hourly.message_count += 1;
            }

            // Also populate daily_activity token counts
            let date = &ts[..10.min(ts.len())];
            if date.len() == 10 {
                let day = stats.daily_activity.entry(date.to_string()).or_default();
                day.input_tokens += input;
                day.output_tokens += output;
            }
        }
    }

    // Flush orphan pending tokens using the last known timestamp.
    // This happens when a session is still active: the final assistant
    // message(s) have no following turn_duration to provide a timestamp.
    if let Some((input, output)) = pending_tokens
        && let Some(ref ts) = stats.last_timestamp
    {
        if ts.len() >= 13 {
            let hour_key = &ts[..13];
            let hourly = stats.hourly_tokens.entry(hour_key.to_string()).or_default();
            hourly.input_tokens += input;
            hourly.output_tokens += output;
            hourly.message_count += 1;
        }
        let date = &ts[..10.min(ts.len())];
        if date.len() == 10 {
            let day = stats.daily_activity.entry(date.to_string()).or_default();
            day.input_tokens += input;
            day.output_tokens += output;
        }
    }

    stats.file_size = file_size;
    Ok(file_size)
}

// ---------------------------------------------------------------------------
// Credential reading (with macOS keychain → JSON fallback)
// ---------------------------------------------------------------------------

/// Read the Claude OAuth credentials (access token + plan info). On macOS, tries
/// Read the selected Claude profile's Keychain entry first, then its
/// `.credentials.json` file. On other platforms, read the file directly.
/// Both the token and plan may be absent; callers decide how to react.
fn read_claude_credentials(
    config_dir: Option<&str>,
) -> Result<(Option<String>, Option<PlanInfo>), String> {
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    let raw_json = read_credential_json(config_dir, &home, |service| {
        crate::plugin_credentials::cached_read(service)
    });

    let Some(json_str) = raw_json else {
        return Ok((None, None));
    };

    parse_credentials_json(&json_str)
}

/// Parse the `claudeAiOauth` object out of raw credentials JSON into a token + `PlanInfo`.
/// Pure function (no I/O) — split out of `read_claude_credentials` so it's unit-testable
/// without touching the filesystem or macOS Keychain.
fn parse_credentials_json(json_str: &str) -> Result<(Option<String>, Option<PlanInfo>), String> {
    let parsed: serde_json::Value =
        serde_json::from_str(json_str).map_err(|e| format!("Failed to parse credentials: {e}"))?;

    let oauth = parsed.get("claudeAiOauth");

    let token = oauth
        .and_then(|o| o.get("accessToken"))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string());

    let plan = oauth.map(|o| PlanInfo {
        subscription_type: o
            .get("subscriptionType")
            .and_then(|v| v.as_str())
            .map(String::from),
        rate_limit_tier: o
            .get("rateLimitTier")
            .and_then(|v| v.as_str())
            .map(String::from),
        scopes: o
            .get("scopes")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|s| s.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
    });

    Ok((token, plan))
}

fn read_credential_json(
    config_dir: Option<&str>,
    home: &Path,
    read_keychain: impl FnOnce(&str) -> Result<Option<String>, String>,
) -> Option<String> {
    let credentials_path = config_dir
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".claude"))
        .join(".credentials.json");
    {
        #[cfg(target_os = "macos")]
        {
            let service = match config_dir.filter(|dir| !dir.is_empty()) {
                Some(dir) => {
                    let hash = Sha256::digest(dir.as_bytes());
                    format!("Claude Code-credentials-{}", &hex::encode(hash)[..8])
                }
                None => "Claude Code-credentials".to_string(),
            };
            let keychain_result = read_keychain(&service);
            match keychain_result {
                Ok(Some(json)) => Some(json),
                _ => std::fs::read_to_string(&credentials_path).ok(),
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = read_keychain;
            std::fs::read_to_string(&credentials_path).ok()
        }
    }
}

// ---------------------------------------------------------------------------
// API cache + retry
// ---------------------------------------------------------------------------

/// Cached API response with timestamp for TTL checks.
struct ApiCache {
    data: UsageApiResponse,
    fetched_at: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum CredentialProfile {
    Default,
    ConfigDir(String),
}

impl CredentialProfile {
    fn config_dir(&self) -> Option<&str> {
        match self {
            Self::Default => None,
            Self::ConfigDir(dir) => Some(dir),
        }
    }
}

fn profile_for_session(
    state: &crate::AppState,
    session_id: Option<&str>,
) -> Result<CredentialProfile, String> {
    let Some(session_id) = session_id else {
        return Ok(CredentialProfile::Default);
    };
    let pid = crate::pty::session_leaf_pid(state, session_id)
        .ok_or("Cannot resolve Claude session profile")?;
    let environment = crate::process_env::read_process_environment(pid)
        .map_err(|_| "Cannot read Claude session profile")?;
    Ok(profile_from_environment(&environment))
}

fn profile_from_environment(environment: &[String]) -> CredentialProfile {
    let config_dir = environment.iter().find_map(|entry| {
        let (key, value) = entry.split_once('=')?;
        let matches = if cfg!(windows) {
            key.eq_ignore_ascii_case("CLAUDE_CONFIG_DIR")
        } else {
            key == "CLAUDE_CONFIG_DIR"
        };
        (matches && !value.is_empty()).then_some(value)
    });
    match config_dir {
        Some(dir) => CredentialProfile::ConfigDir(dir.to_string()),
        None => CredentialProfile::Default,
    }
}

/// In-memory cache for the usage API response.
static API_CACHE: std::sync::LazyLock<parking_lot::Mutex<HashMap<CredentialProfile, ApiCache>>> =
    std::sync::LazyLock::new(|| parking_lot::Mutex::new(HashMap::new()));

/// When set, we're rate-limited and should not hit the API until this instant.
static RATE_LIMITED_UNTIL: std::sync::LazyLock<
    parking_lot::Mutex<HashMap<CredentialProfile, Instant>>,
> = std::sync::LazyLock::new(|| parking_lot::Mutex::new(HashMap::new()));

/// Cache TTL: return cached data without hitting the API.
const API_CACHE_TTL: Duration = Duration::from_secs(300); // 5 minutes

/// Maximum retry attempts for 429 responses.
const MAX_429_RETRIES: u32 = 1;

/// Initial backoff delay for 429 retry.
const RETRY_INITIAL_DELAY: Duration = Duration::from_secs(10);

/// Minimum backoff after exhausting 429 retries (prevents hammering on next poll).
const RATE_LIMIT_BACKOFF: Duration = Duration::from_secs(120);

/// Error from a raw API fetch, carrying status and optional Retry-After.
struct FetchError {
    status: u16,
    message: String,
    retry_after_secs: Option<u64>,
}

/// Raw HTTP fetch — no caching, no retry.
async fn fetch_usage_from_api(token: &str) -> Result<UsageApiResponse, FetchError> {
    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.anthropic.com/api/oauth/usage")
        .header("Authorization", format!("Bearer {token}"))
        .header("anthropic-beta", "oauth-2025-04-20")
        .send()
        .await
        .map_err(|e| FetchError {
            status: 0,
            message: format!("API request failed: {e}"),
            retry_after_secs: None,
        })?;

    let status = resp.status().as_u16();
    if !resp.status().is_success() {
        let retry_after_secs = resp
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok());
        let body = resp.text().await.unwrap_or_default();
        return Err(FetchError {
            status,
            message: format!("API returned {status}: {body}"),
            retry_after_secs,
        });
    }

    let body = resp.text().await.map_err(|e| FetchError {
        status: 0,
        message: format!("Failed to read API response: {e}"),
        retry_after_secs: None,
    })?;

    serde_json::from_str(&body).map_err(|e| {
        tracing::error!(source = "claude_usage", "Parse error: {e}\nBody: {body}");
        FetchError {
            status: 0,
            message: format!("Failed to parse API response: {e}"),
            retry_after_secs: None,
        }
    })
}

// ---------------------------------------------------------------------------
// Fallback: parse rate limits from /v1/messages response headers
// ---------------------------------------------------------------------------

/// Extract a header value as a string slice.
fn header_str<'a>(headers: &'a reqwest::header::HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

/// Parse a single claim (e.g. "5h" or "7d") into a `RateBucket`.
/// Utilization in headers is 0.0–1.0; we convert to 0–100 percentage.
/// Reset is a unix epoch seconds timestamp; we convert to ISO8601.
fn parse_claim_bucket(headers: &reqwest::header::HeaderMap, abbrev: &str) -> Option<RateBucket> {
    let util_str = header_str(
        headers,
        &format!("anthropic-ratelimit-unified-{abbrev}-utilization"),
    )?;
    let utilization_frac: f64 = util_str.parse().ok()?;
    let reset_epoch = header_str(
        headers,
        &format!("anthropic-ratelimit-unified-{abbrev}-reset"),
    )
    .and_then(|v| v.parse::<i64>().ok());
    let resets_at = reset_epoch.map(|epoch| {
        chrono::DateTime::from_timestamp(epoch, 0)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_else(|| epoch.to_string())
    });
    Some(RateBucket {
        utilization: utilization_frac * 100.0,
        resets_at,
    })
}

/// Convert a unix epoch header value (seconds since 1970) to an ISO8601 string.
fn header_epoch_to_iso(headers: &reqwest::header::HeaderMap, name: &str) -> Option<String> {
    header_str(headers, name)
        .and_then(|v| v.parse::<i64>().ok())
        .and_then(|epoch| chrono::DateTime::from_timestamp(epoch, 0))
        .map(|dt| dt.to_rfc3339())
}

/// Parse `anthropic-ratelimit-unified-*` headers into `UsageApiResponse`.
/// Pure function, no I/O — used by fallback and tests.
fn parse_unified_rate_limit_headers(headers: &reqwest::header::HeaderMap) -> UsageApiResponse {
    let five_hour = parse_claim_bucket(headers, "5h");
    let seven_day = parse_claim_bucket(headers, "7d");

    // Overage → ExtraUsage (partial — monthly_limit / used_credits only available via /api/oauth/usage)
    let overage_status = header_str(headers, "anthropic-ratelimit-unified-overage-status");
    let overage_in_use = header_str(headers, "anthropic-ratelimit-unified-overage-in-use")
        .map(|v| v == "true")
        .unwrap_or(false);
    let overage_utilization =
        header_str(headers, "anthropic-ratelimit-unified-overage-utilization")
            .and_then(|v| v.parse::<f64>().ok())
            .map(|f| f * 100.0);
    let overage_resets_at =
        header_epoch_to_iso(headers, "anthropic-ratelimit-unified-overage-reset");
    let extra_usage = if overage_status.is_some() || overage_in_use {
        Some(ExtraUsage {
            is_enabled: true,
            monthly_limit: None,
            used_credits: None,
            utilization: overage_utilization,
            resets_at: overage_resets_at,
            in_use: overage_in_use,
        })
    } else {
        None
    };

    // Global rate-limit meta — status + representative claim (which bucket is the bottleneck).
    let unified_status =
        header_str(headers, "anthropic-ratelimit-unified-status").map(String::from);
    let representative_claim =
        header_str(headers, "anthropic-ratelimit-unified-representative-claim").map(String::from);
    let meta = if unified_status.is_some() || representative_claim.is_some() {
        Some(RateLimitMeta {
            unified_status,
            representative_claim,
        })
    } else {
        None
    };

    UsageApiResponse {
        five_hour,
        seven_day,
        seven_day_oauth_apps: None,
        seven_day_opus: None,
        seven_day_sonnet: None,
        seven_day_cowork: None,
        extra_usage,
        plan: None,
        meta,
    }
}

/// Fallback: make a minimal Haiku call and extract rate limits from response headers.
/// Cost: ~9 tokens per call (~$0.00001).
async fn fetch_usage_from_headers(token: &str) -> Result<UsageApiResponse, FetchError> {
    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("Authorization", format!("Bearer {token}"))
        .header("anthropic-beta", "oauth-2025-04-20")
        .header("anthropic-version", "2023-06-01")
        .header("Content-Type", "application/json")
        .body(r#"{"model":"claude-haiku-4-5-20251001","max_tokens":1,"messages":[{"role":"user","content":"h"}]}"#)
        .send()
        .await
        .map_err(|e| FetchError {
            status: 0,
            message: format!("Headers fallback request failed: {e}"),
            retry_after_secs: None,
        })?;

    let headers = resp.headers().clone();
    let status = resp.status().as_u16();

    // Rate limit headers are present on both 200 and 429 from /v1/messages
    let data = parse_unified_rate_limit_headers(&headers);

    // If no claim data was extracted, treat as failure
    if data.five_hour.is_none() && data.seven_day.is_none() {
        let body = resp.text().await.unwrap_or_default();
        return Err(FetchError {
            status,
            message: format!(
                "Headers fallback: no rate limit headers in {status} response: {body}"
            ),
            retry_after_secs: None,
        });
    }

    Ok(data)
}

/// Store a successful response in the cache.
fn cache_response(profile: &CredentialProfile, data: &UsageApiResponse) {
    API_CACHE.lock().insert(
        profile.clone(),
        ApiCache {
            data: data.clone(),
            fetched_at: Instant::now(),
        },
    );
}

/// Try to get a cached response. Returns Some if cache exists and is within TTL.
fn try_get_fresh_cache(profile: &CredentialProfile) -> Option<UsageApiResponse> {
    let guard = API_CACHE.lock();
    guard.get(profile).and_then(|c| {
        if c.fetched_at.elapsed() < API_CACHE_TTL {
            Some(c.data.clone())
        } else {
            None
        }
    })
}

/// Get stale cached data (any age) as fallback on errors.
fn get_stale_cache(profile: &CredentialProfile) -> Option<UsageApiResponse> {
    API_CACHE.lock().get(profile).map(|c| c.data.clone())
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Fetch rate-limit usage from the Anthropic OAuth API.
/// Uses an in-memory cache (5 min TTL) and retries 429s with exponential backoff.
#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn get_claude_usage_api(
    state: State<'_, Arc<crate::AppState>>,
    session_id: Option<String>,
) -> Result<UsageApiResponse, String> {
    get_claude_usage_api_impl(&state, session_id.as_deref()).await
}

pub(crate) async fn get_claude_usage_api_impl(
    state: &crate::AppState,
    session_id: Option<&str>,
) -> Result<UsageApiResponse, String> {
    let profile = profile_for_session(state, session_id)?;
    let (token_opt, plan) = read_claude_credentials(profile.config_dir())?;
    let token = token_opt.ok_or_else(|| "No Claude OAuth token found".to_string())?;
    // Return fresh cache if available
    if let Some(cached) = try_get_fresh_cache(&profile) {
        return Ok(cached);
    }

    // If we're in a rate-limit backoff window, return stale cache without hitting the API
    if let Some(until) = RATE_LIMITED_UNTIL.lock().get(&profile).copied()
        && Instant::now() < until
    {
        if let Some(stale) = get_stale_cache(&profile) {
            return Ok(stale);
        }
        return Err("Rate limited — waiting for backoff to expire".to_string());
    }

    // Attempt fetch with 429 retry
    let mut last_err_msg = String::new();
    let mut was_rate_limited = false;
    for attempt in 0..=MAX_429_RETRIES {
        match fetch_usage_from_api(&token).await {
            Ok(mut data) => {
                // Clear any rate-limit backoff on success
                RATE_LIMITED_UNTIL.lock().remove(&profile);
                data.plan = plan.clone();
                cache_response(&profile, &data);
                return Ok(data);
            }
            Err(e) => {
                last_err_msg = e.message.clone();
                if e.status == 429 {
                    was_rate_limited = true;
                    if attempt < MAX_429_RETRIES {
                        // Use server's retry-after if > 0, otherwise our own exponential backoff.
                        // retry-after: 0 is treated as "no hint" since immediate retry is pointless.
                        let delay = match e.retry_after_secs.filter(|&s| s > 0) {
                            Some(secs) => Duration::from_secs(secs),
                            None => RETRY_INITIAL_DELAY * 2u32.pow(attempt),
                        };
                        tracing::warn!(
                            source = "claude_usage",
                            attempt = attempt + 1,
                            max_retries = MAX_429_RETRIES,
                            delay_secs = delay.as_secs(),
                            "429 rate limited, retrying"
                        );
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                }
                // Non-429 error or exhausted retries — fall through
                break;
            }
        }
    }

    // Set backoff so next poll doesn't hammer a rate-limited endpoint
    if was_rate_limited {
        RATE_LIMITED_UNTIL
            .lock()
            .insert(profile.clone(), Instant::now() + RATE_LIMIT_BACKOFF);
        tracing::warn!(
            source = "claude_usage",
            backoff_secs = RATE_LIMIT_BACKOFF.as_secs(),
            "Rate limited — backing off"
        );
    }

    // Fallback: extract rate limits from /v1/messages response headers (Haiku, ~9 tokens)
    match fetch_usage_from_headers(&token).await {
        Ok(mut data) => {
            tracing::info!(
                source = "claude_usage",
                "Primary API failed, using headers fallback"
            );
            data.plan = plan.clone();
            cache_response(&profile, &data);
            return Ok(data);
        }
        Err(e) => {
            tracing::warn!(
                source = "claude_usage",
                "Headers fallback also failed: {}",
                e.message
            );
        }
    }

    // On error, return stale cache if available
    if let Some(stale) = get_stale_cache(&profile) {
        tracing::info!(
            source = "claude_usage",
            "Returning stale cache after error: {last_err_msg}"
        );
        return Ok(stale);
    }

    Err(last_err_msg)
}

/// Get token usage timeline reconstructed from session transcripts.
///
/// Returns hourly token usage points aggregated from the session stats cache.
/// The `scope` parameter filters which projects to include ("all" or a slug).
/// The `days` parameter limits the time window (default 7).
#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn get_claude_usage_timeline(
    state: State<'_, Arc<crate::AppState>>,
    scope: String,
    days: Option<u32>,
) -> Result<Vec<TimelinePoint>, String> {
    get_claude_usage_timeline_impl(&state, scope, days).await
}

/// HTTP/remote-safe body shared with the Tauri command above (no Tauri `State`).
pub async fn get_claude_usage_timeline_impl(
    state: &Arc<crate::AppState>,
    scope: String,
    days: Option<u32>,
) -> Result<Vec<TimelinePoint>, String> {
    let days = days.unwrap_or(7);
    let cache = state.claude_usage_cache.lock().clone();

    // Determine cutoff hour key
    let cutoff = chrono::Utc::now() - chrono::Duration::days(days as i64);
    let cutoff_key = cutoff.format("%Y-%m-%dT%H").to_string();

    // Aggregate hourly tokens across matching projects
    let mut hourly: HashMap<String, (u64, u64)> = HashMap::new();

    let slugs: Vec<&String> = match scope.as_str() {
        "all" => cache.keys().collect(),
        _ => cache.keys().filter(|k| k.as_str() == scope).collect(),
    };

    for slug in slugs {
        if let Some(files) = cache.get(slug) {
            for file_stats in files.values() {
                for (hour, tokens) in &file_stats.hourly_tokens {
                    if hour.as_str() >= cutoff_key.as_str() {
                        let entry = hourly.entry(hour.clone()).or_default();
                        entry.0 += tokens.input_tokens;
                        entry.1 += tokens.output_tokens;
                    }
                }
            }
        }
    }

    let mut points: Vec<TimelinePoint> = hourly
        .into_iter()
        .map(|(hour, (input, output))| TimelinePoint {
            hour,
            input_tokens: input,
            output_tokens: output,
        })
        .collect();

    points.sort_by(|a, b| a.hour.cmp(&b.hour));

    Ok(points)
}

/// Scan session transcripts and return aggregated stats.
///
/// `scope` values:
/// - `"all"` — all projects
/// - `"current"` — current project (determined from config / active repo)
/// - Any other string — treated as a specific project slug
#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn get_claude_session_stats(
    state: State<'_, Arc<crate::AppState>>,
    scope: String,
) -> Result<SessionStats, String> {
    get_claude_session_stats_impl(&state, scope).await
}

/// HTTP/remote-safe body shared with the Tauri command above (no Tauri `State`).
pub async fn get_claude_session_stats_impl(
    state: &Arc<crate::AppState>,
    scope: String,
) -> Result<SessionStats, String> {
    let projects_dir =
        claude_projects_dir().ok_or_else(|| "Cannot determine home directory".to_string())?;
    get_claude_session_stats_from_dir(state, scope, &projects_dir).await
}

#[cfg(test)]
thread_local! {
    static SESSION_FILE_STATS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

async fn get_claude_session_stats_from_dir(
    state: &Arc<crate::AppState>,
    scope: String,
    projects_dir: &std::path::Path,
) -> Result<SessionStats, String> {
    let projects_root = match projects_dir.canonicalize() {
        Ok(root) => root,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(SessionStats::default());
        }
        Err(error) => return Err(format!("Failed to resolve projects dir: {error}")),
    };
    let projects_dir = projects_root.as_path();
    // Single-flight across scopes: concurrent refreshes must not clone the same
    // old parse cache and overwrite each other's newly ingested transcripts.
    let mut scan = state.claude_usage_scan.lock();
    scan.prepare(projects_dir);
    let cache_mutex = state.claude_usage_cache.lock();
    // Clone the cache so we can release the lock during I/O
    let mut cache = cache_mutex.clone();
    drop(cache_mutex);

    // A scoped request resolves that one slug directly. Directory entry types
    // avoid a metadata lookup for every historical project in all-project scope.
    let mut symlink_projects = HashSet::new();
    let project_dirs: Vec<(String, PathBuf)> = if scope == "all" {
        std::fs::read_dir(projects_dir)
            .map_err(|e| format!("Failed to read projects dir: {e}"))?
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let file_type = entry.file_type().ok()?;
                if file_type.is_symlink() {
                    symlink_projects.insert(entry.file_name().to_string_lossy().into_owned());
                }
                (file_type.is_dir() || (file_type.is_symlink() && entry.path().is_dir())).then(
                    || {
                        (
                            entry.file_name().to_string_lossy().into_owned(),
                            entry.path(),
                        )
                    },
                )
            })
            .collect()
    } else {
        let mut components = Path::new(&scope).components();
        if !matches!(components.next(), Some(std::path::Component::Normal(_)))
            || components.next().is_some()
        {
            return Ok(SessionStats::default());
        }
        let path = projects_dir.join(&scope);
        if path
            .symlink_metadata()
            .is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            symlink_projects.insert(scope.clone());
        }
        if path.is_dir() {
            vec![(scope.clone(), path)]
        } else {
            Vec::new()
        }
    };
    let filtered: Vec<&(String, PathBuf)> = project_dirs.iter().collect();

    let mut cache_dirty = false;

    // Prune deleted projects from cache
    let existing_slugs: std::collections::HashSet<&str> =
        project_dirs.iter().map(|(s, _)| s.as_str()).collect();
    let stale_slugs: Vec<String> = cache
        .keys()
        .filter(|k| (scope == "all" || *k == &scope) && !existing_slugs.contains(k.as_str()))
        .cloned()
        .collect();
    for slug in stale_slugs {
        cache.remove(&slug);
        cache_dirty = true;
    }

    // Scan each project
    for (slug, dir) in &filtered {
        let project_cache = cache.entry(slug.clone()).or_default();

        // List JSONL files in this project dir
        let jsonl_files: Vec<(String, PathBuf)> = match std::fs::read_dir(dir) {
            Ok(entries) => entries
                .filter_map(|e| {
                    let e = e.ok()?;
                    let p = e.path();
                    if p.extension().and_then(|s| s.to_str()) == Some("jsonl") {
                        let name = e.file_name().to_string_lossy().to_string();
                        Some((name, p))
                    } else {
                        None
                    }
                })
                .collect(),
            Err(_) => continue,
        };

        // Prune deleted files from cache
        let existing_files: std::collections::HashSet<&str> =
            jsonl_files.iter().map(|(name, _)| name.as_str()).collect();
        let stale_files: Vec<String> = project_cache
            .keys()
            .filter(|k| !existing_files.contains(k.as_str()))
            .cloned()
            .collect();
        for f in stale_files {
            scan.seen.remove(&dir.join(&f));
            scan.pending.remove(&dir.join(&f));
            project_cache.remove(&f);
            cache_dirty = true;
        }

        // Incremental parse each file
        for (name, path) in &jsonl_files {
            // Recursive watchers need not follow a project directory symlink.
            // Keep validation for those targets rather than trust absent events.
            if !symlink_projects.contains(slug)
                && scan.seen.contains(path)
                && !scan.pending.contains(path)
            {
                continue;
            }
            #[cfg(test)]
            SESSION_FILE_STATS.with(|stats| stats.set(stats.get() + 1));
            let current_size = match std::fs::metadata(path) {
                Ok(m) => m.len(),
                Err(_) => continue,
            };

            let cached = project_cache.get(name);

            // Cache migration: if the file has messages but no hourly data,
            // force a full reparse to populate hourly_tokens.
            let needs_migration = cached
                .map(|c| c.assistant_message_count > 0 && c.hourly_tokens.is_empty())
                .unwrap_or(false);

            match cached {
                Some(c) if c.file_size == current_size && !needs_migration => {
                    // Unchanged and has hourly data — skip future stats until
                    // a watcher event or reconciliation invalidates this file.
                    scan.seen.insert(path.clone());
                    scan.pending.remove(path);
                    continue;
                }
                Some(c) if current_size > c.file_size && !needs_migration => {
                    // File grew (no migration needed) — parse only new bytes
                    let offset = c.file_size;
                    let mut stats = c.clone();
                    match parse_jsonl_file_from_offset(path, offset, &mut stats) {
                        Ok(_) => {
                            project_cache.insert(name.clone(), stats);
                            cache_dirty = true;
                        }
                        Err(e) => {
                            tracing::warn!(source = "claude_usage", path = %path.display(), "Error parsing: {e}");
                        }
                    }
                }
                Some(_) => {
                    // File shrank (truncated/rewritten) — full reparse
                    let mut stats = CachedFileStats::default();
                    match parse_jsonl_file_from_offset(path, 0, &mut stats) {
                        Ok(_) => {
                            project_cache.insert(name.clone(), stats);
                            cache_dirty = true;
                        }
                        Err(e) => {
                            tracing::warn!(source = "claude_usage", path = %path.display(), "Error reparsing: {e}");
                        }
                    }
                }
                None => {
                    // New file — full parse
                    let mut stats = CachedFileStats::default();
                    match parse_jsonl_file_from_offset(path, 0, &mut stats) {
                        Ok(_) => {
                            project_cache.insert(name.clone(), stats);
                            cache_dirty = true;
                        }
                        Err(e) => {
                            tracing::warn!(source = "claude_usage", path = %path.display(), "Error parsing new file: {e}");
                        }
                    }
                }
            }
            if project_cache
                .get(name)
                .is_some_and(|c| c.file_size == current_size)
            {
                scan.seen.insert(path.clone());
                scan.pending.remove(path);
            }
        }
    }

    // Persist cache if anything changed
    if cache_dirty {
        save_cache_to_disk(&cache);
        // Update in-memory cache
        *state.claude_usage_cache.lock() = cache.clone();
    }

    // Aggregate stats from cache
    let mut result = SessionStats::default();
    let mut all_active_hours: HashSet<String> = HashSet::new();

    for (slug, dir_path) in &filtered {
        if let Some(project_files) = cache.get(slug.as_str()) {
            let mut proj = ProjectStats::default();
            let mut project_sessions: std::collections::HashSet<String> =
                std::collections::HashSet::new();

            for file_stats in project_files.values() {
                result.total_input_tokens += file_stats.total_input_tokens;
                result.total_output_tokens += file_stats.total_output_tokens;
                result.total_cache_creation_tokens += file_stats.total_cache_creation_tokens;
                result.total_cache_read_tokens += file_stats.total_cache_read_tokens;
                result.total_assistant_messages += file_stats.assistant_message_count;
                result.total_user_messages += file_stats.user_message_count;

                proj.input_tokens += file_stats.total_input_tokens;
                proj.output_tokens += file_stats.total_output_tokens;
                proj.assistant_message_count += file_stats.assistant_message_count;
                proj.user_message_count += file_stats.user_message_count;

                // Merge model usage
                for (model, tokens) in &file_stats.model_usage {
                    let entry = result.model_usage.entry(model.clone()).or_default();
                    entry.input_tokens += tokens.input_tokens;
                    entry.output_tokens += tokens.output_tokens;
                    entry.cache_creation_tokens += tokens.cache_creation_tokens;
                    entry.cache_read_tokens += tokens.cache_read_tokens;
                    entry.message_count += tokens.message_count;
                }

                // Merge daily activity
                for (date, day) in &file_stats.daily_activity {
                    let entry = result.daily_activity.entry(date.clone()).or_default();
                    entry.input_tokens += day.input_tokens;
                    entry.output_tokens += day.output_tokens;
                    entry.message_count += day.message_count;
                    // Don't double-count sessions — we'll compute from unique session IDs

                    // Also accumulate per-project daily breakdown
                    let proj_daily = result
                        .per_project_daily
                        .entry(slug.clone())
                        .or_default()
                        .entry(date.clone())
                        .or_default();
                    proj_daily.input_tokens += day.input_tokens;
                    proj_daily.output_tokens += day.output_tokens;
                    proj_daily.message_count += day.message_count;
                }

                // Collect unique hourly buckets with activity
                for hour_key in file_stats.hourly_tokens.keys() {
                    all_active_hours.insert(hour_key.clone());
                }

                // Collect unique sessions
                for sid in &file_stats.session_ids {
                    project_sessions.insert(sid.clone());
                }
            }

            proj.session_count = project_sessions.len() as u32;
            result.per_project.insert(slug.clone(), proj);

            // Don't accumulate sessions here — we'll deduplicate at the end
            let _ = dir_path; // suppress unused warning
        }
    }

    // Total sessions = sum of per-project unique sessions
    result.total_sessions = result.per_project.values().map(|p| p.session_count).sum();

    result.active_hours = all_active_hours.len() as u32;

    Ok(result)
}

/// List available Claude project slugs for the scope dropdown.
#[cfg_attr(feature = "desktop", tauri::command)]
pub async fn get_claude_project_list() -> Result<Vec<ProjectEntry>, String> {
    let projects_dir =
        claude_projects_dir().ok_or_else(|| "Cannot determine home directory".to_string())?;

    if !projects_dir.exists() {
        return Ok(Vec::new());
    }

    let mut entries: Vec<ProjectEntry> = std::fs::read_dir(&projects_dir)
        .map_err(|e| format!("Failed to read projects dir: {e}"))?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.is_dir() {
                let slug = entry.file_name().to_string_lossy().to_string();
                // Count JSONL files as sessions
                let session_count = std::fs::read_dir(&path)
                    .ok()
                    .map(|entries| {
                        entries
                            .filter_map(|e| e.ok())
                            .filter(|e| {
                                e.path().extension().and_then(|s| s.to_str()) == Some("jsonl")
                            })
                            .count()
                    })
                    .unwrap_or(0);
                if session_count > 0 {
                    let display_path = resolve_slug_to_path(&slug);
                    Some(ProjectEntry {
                        slug,
                        session_count,
                        display_path,
                    })
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    // Sort by session count descending
    entries.sort_by_key(|a| std::cmp::Reverse(a.session_count));

    Ok(entries)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_environment_selects_credential_profile() {
        assert_eq!(profile_from_environment(&[]), CredentialProfile::Default);
        assert_eq!(
            profile_from_environment(&["CLAUDE_CONFIG_DIR=/fixture/private".into()]),
            CredentialProfile::ConfigDir("/fixture/private".into())
        );
        assert_eq!(
            profile_from_environment(&["CLAUDE_CONFIG_DIR=".into()]),
            CredentialProfile::Default
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn default_profile_keeps_the_bare_keychain_service() {
        let home = tempfile::tempdir().unwrap();
        let mut requested = String::new();
        let credentials = read_credential_json(None, home.path(), |service| {
            requested = service.to_string();
            Ok(Some("default-account".into()))
        });
        assert_eq!(requested, "Claude Code-credentials");
        assert_eq!(credentials.as_deref(), Some("default-account"));
    }

    #[test]
    fn custom_profile_reads_its_own_credentials_file() {
        let home = tempfile::tempdir().unwrap();
        let default_dir = home.path().join(".claude");
        let profile = home.path().join(".claude-private");
        std::fs::create_dir_all(&default_dir).unwrap();
        std::fs::create_dir_all(&profile).unwrap();
        std::fs::write(default_dir.join(".credentials.json"), "default-account").unwrap();
        std::fs::write(profile.join(".credentials.json"), "private-account").unwrap();

        let credentials = read_credential_json(profile.to_str(), home.path(), |_| Ok(None));
        assert_eq!(credentials.as_deref(), Some("private-account"));
    }

    #[test]
    fn missing_custom_profile_never_reads_default_credentials_file() {
        let home = tempfile::tempdir().unwrap();
        let default_dir = home.path().join(".claude");
        std::fs::create_dir_all(&default_dir).unwrap();
        std::fs::write(default_dir.join(".credentials.json"), "default-account").unwrap();

        let missing_profile = home.path().join(".claude-missing");
        let credentials = read_credential_json(missing_profile.to_str(), home.path(), |_| Ok(None));
        assert!(credentials.is_none());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn custom_profile_reads_its_namespaced_keychain_service() {
        let home = tempfile::tempdir().unwrap();
        let mut requested = String::new();
        let credentials =
            read_credential_json(Some("/fixture/claude-private"), home.path(), |service| {
                requested = service.to_string();
                Ok(Some("private-account".into()))
            });
        assert_eq!(requested, "Claude Code-credentials-db8e0363");
        assert_eq!(credentials.as_deref(), Some("private-account"));
    }

    #[test]
    fn parse_assistant_line() {
        let line = r#"{"type":"assistant","message":{"model":"claude-opus-4-6","usage":{"input_tokens":100,"output_tokens":50,"cache_creation_input_tokens":200,"cache_read_input_tokens":300}}}"#;
        let mut stats = CachedFileStats::default();
        let info = parse_jsonl_line(line, &mut stats);

        // LineInfo should carry the assistant tokens for correlation
        assert_eq!(info.assistant_tokens, Some((100, 50)));
        assert!(info.timestamp.is_none());

        assert_eq!(stats.total_input_tokens, 100);
        assert_eq!(stats.total_output_tokens, 50);
        assert_eq!(stats.total_cache_creation_tokens, 200);
        assert_eq!(stats.total_cache_read_tokens, 300);
        assert_eq!(stats.assistant_message_count, 1);
        assert_eq!(stats.model_usage.len(), 1);
        let model = stats.model_usage.get("claude-opus-4-6").unwrap();
        assert_eq!(model.input_tokens, 100);
        assert_eq!(model.message_count, 1);
    }

    #[test]
    fn parse_user_line() {
        let line = r#"{"type":"user","message":"hello"}"#;
        let mut stats = CachedFileStats::default();
        parse_jsonl_line(line, &mut stats);
        assert_eq!(stats.user_message_count, 1);
        assert_eq!(stats.assistant_message_count, 0);
    }

    #[test]
    fn parse_turn_duration_line() {
        let line = r#"{"type":"system","subtype":"turn_duration","timestamp":"2026-02-04T23:22:42.546Z","sessionId":"abc-123","durationMs":5000}"#;
        let mut stats = CachedFileStats::default();
        let info = parse_jsonl_line(line, &mut stats);
        assert_eq!(info.timestamp, Some("2026-02-04T23:22:42.546Z".to_string()));
        assert!(info.assistant_tokens.is_none());
        assert!(stats.session_ids.contains("abc-123"));
        assert_eq!(stats.session_ids.len(), 1);
        assert!(stats.daily_activity.contains_key("2026-02-04"));
        let day = stats.daily_activity.get("2026-02-04").unwrap();
        assert_eq!(day.session_count, 1);
    }

    #[test]
    fn parse_progress_line_is_skipped() {
        let line = r#"{"type":"progress","content":"tool_use"}"#;
        let mut stats = CachedFileStats::default();
        parse_jsonl_line(line, &mut stats);
        assert_eq!(stats.assistant_message_count, 0);
        assert_eq!(stats.user_message_count, 0);
    }

    #[test]
    fn parse_invalid_json_is_skipped() {
        let line = "not valid json {{{";
        let mut stats = CachedFileStats::default();
        parse_jsonl_line(line, &mut stats);
        assert_eq!(stats.assistant_message_count, 0);
    }

    #[test]
    fn parse_multiple_assistant_lines_accumulate() {
        let mut stats = CachedFileStats::default();

        let line1 = r#"{"type":"assistant","message":{"model":"claude-opus-4-6","usage":{"input_tokens":100,"output_tokens":50,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}"#;
        let line2 = r#"{"type":"assistant","message":{"model":"claude-sonnet-4-6","usage":{"input_tokens":200,"output_tokens":100,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}"#;
        let line3 = r#"{"type":"assistant","message":{"model":"claude-opus-4-6","usage":{"input_tokens":50,"output_tokens":25,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}"#;

        parse_jsonl_line(line1, &mut stats);
        parse_jsonl_line(line2, &mut stats);
        parse_jsonl_line(line3, &mut stats);

        assert_eq!(stats.total_input_tokens, 350);
        assert_eq!(stats.total_output_tokens, 175);
        assert_eq!(stats.assistant_message_count, 3);
        assert_eq!(stats.model_usage.len(), 2);

        let opus = stats.model_usage.get("claude-opus-4-6").unwrap();
        assert_eq!(opus.input_tokens, 150);
        assert_eq!(opus.message_count, 2);

        let sonnet = stats.model_usage.get("claude-sonnet-4-6").unwrap();
        assert_eq!(sonnet.input_tokens, 200);
        assert_eq!(sonnet.message_count, 1);
    }

    #[test]
    fn duplicate_session_id_not_double_counted() {
        let mut stats = CachedFileStats::default();

        let line1 = r#"{"type":"system","subtype":"turn_duration","timestamp":"2026-02-04T10:00:00Z","sessionId":"sess-1","durationMs":1000}"#;
        let line2 = r#"{"type":"system","subtype":"turn_duration","timestamp":"2026-02-04T10:05:00Z","sessionId":"sess-1","durationMs":2000}"#;

        parse_jsonl_line(line1, &mut stats);
        parse_jsonl_line(line2, &mut stats);

        assert_eq!(stats.session_ids.len(), 1);
        let day = stats.daily_activity.get("2026-02-04").unwrap();
        assert_eq!(day.session_count, 1);
        assert_eq!(day.message_count, 2);
    }

    #[test]
    fn first_last_timestamp_tracking() {
        let mut stats = CachedFileStats::default();

        let line1 = r#"{"type":"system","subtype":"turn_duration","timestamp":"2026-02-04T23:22:42.546Z","sessionId":"s1","durationMs":1000}"#;
        let line2 = r#"{"type":"system","subtype":"turn_duration","timestamp":"2026-02-01T10:00:00.000Z","sessionId":"s2","durationMs":1000}"#;
        let line3 = r#"{"type":"system","subtype":"turn_duration","timestamp":"2026-02-10T08:00:00.000Z","sessionId":"s3","durationMs":1000}"#;

        parse_jsonl_line(line1, &mut stats);
        parse_jsonl_line(line2, &mut stats);
        parse_jsonl_line(line3, &mut stats);

        assert_eq!(
            stats.first_timestamp,
            Some("2026-02-01T10:00:00.000Z".to_string())
        );
        assert_eq!(
            stats.last_timestamp,
            Some("2026-02-10T08:00:00.000Z".to_string())
        );
    }

    #[test]
    fn cache_serialization_roundtrip() {
        let mut cache: SessionStatsCache = HashMap::new();
        let mut file_stats = CachedFileStats {
            file_size: 12345,
            total_input_tokens: 1000,
            ..CachedFileStats::default()
        };
        file_stats.session_ids.insert("s1".to_string());

        let mut project = HashMap::new();
        project.insert("session1.jsonl".to_string(), file_stats);
        cache.insert("my-project".to_string(), project);

        let json = serde_json::to_string(&cache).unwrap();
        let restored: SessionStatsCache = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.len(), 1);
        let proj = restored.get("my-project").unwrap();
        let f = proj.get("session1.jsonl").unwrap();
        assert_eq!(f.file_size, 12345);
        assert_eq!(f.total_input_tokens, 1000);
    }

    #[test]
    fn parse_jsonl_file_from_offset_works() {
        // Create a temp file with JSONL content
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let path = dir.path().join("test.jsonl");

        let content = r#"{"type":"user","message":"hello"}
{"type":"assistant","message":{"model":"claude-opus-4-6","usage":{"input_tokens":100,"output_tokens":50,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}
{"type":"progress","content":"working"}
{"type":"system","subtype":"turn_duration","timestamp":"2026-02-04T10:00:00Z","sessionId":"s1","durationMs":5000}
"#;
        std::fs::write(&path, content).unwrap();

        let mut stats = CachedFileStats::default();
        let final_size = parse_jsonl_file_from_offset(&path, 0, &mut stats).unwrap();

        assert_eq!(final_size, content.len() as u64);
        assert_eq!(stats.user_message_count, 1);
        assert_eq!(stats.assistant_message_count, 1);
        assert_eq!(stats.total_input_tokens, 100);
        assert!(stats.session_ids.contains("s1"));
        assert_eq!(stats.session_ids.len(), 1);

        // Verify hourly token correlation: assistant tokens should be bucketed
        // into the hour of the following turn_duration timestamp
        assert_eq!(stats.hourly_tokens.len(), 1);
        let hourly = stats.hourly_tokens.get("2026-02-04T10").unwrap();
        assert_eq!(hourly.input_tokens, 100);
        assert_eq!(hourly.output_tokens, 50);
        assert_eq!(hourly.message_count, 1);

        // Daily activity should also have token counts
        let day = stats.daily_activity.get("2026-02-04").unwrap();
        assert_eq!(day.input_tokens, 100);
        assert_eq!(day.output_tokens, 50);

        // Now append more data and parse incrementally
        let append = r#"{"type":"assistant","message":{"model":"claude-sonnet-4-6","usage":{"input_tokens":200,"output_tokens":100,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}
{"type":"system","subtype":"turn_duration","timestamp":"2026-02-04T14:00:00Z","sessionId":"s1","durationMs":3000}
"#;
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(append.as_bytes())
            .unwrap();

        use std::io::Write;
        let new_size = parse_jsonl_file_from_offset(&path, final_size, &mut stats).unwrap();

        assert_eq!(stats.assistant_message_count, 2);
        assert_eq!(stats.total_input_tokens, 300);
        assert!(new_size > final_size);

        // Verify new hourly bucket was created for 14:00
        assert_eq!(stats.hourly_tokens.len(), 2);
        let hourly14 = stats.hourly_tokens.get("2026-02-04T14").unwrap();
        assert_eq!(hourly14.input_tokens, 200);
        assert_eq!(hourly14.output_tokens, 100);
    }

    fn transcript(input: u64, output: u64, session: &str, hour: &str) -> String {
        format!(
            "{{\"type\":\"assistant\",\"message\":{{\"model\":\"claude-test\",\"usage\":{{\"input_tokens\":{input},\"output_tokens\":{output},\"cache_creation_input_tokens\":0,\"cache_read_input_tokens\":0}}}}}}\n{{\"type\":\"system\",\"subtype\":\"turn_duration\",\"timestamp\":\"{hour}:00:00Z\",\"sessionId\":\"{session}\",\"durationMs\":1}}\n"
        )
    }

    async fn wait_for_usage_event(state: &crate::AppState, path: &Path) {
        // Setup only: the next assertion is about incremental parsing, not the
        // operating system's watcher delivery latency under machine load.
        tokio::time::timeout(Duration::from_secs(60), async {
            loop {
                let ready = {
                    let scan = state.claude_usage_scan.lock();
                    let changes = scan.events.lock();
                    changes.rescan || changes.paths.contains(path)
                };
                if ready {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("usage watcher must deliver the filesystem change before refresh");
    }

    #[tokio::test]
    async fn session_stats_cache_tracks_growth_truncation_and_deletion() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().join("config"));
        let projects = dir.path().join("projects");
        let project = projects.join("project-a");
        std::fs::create_dir_all(&project).unwrap();
        let session = project.join("session.jsonl");
        std::fs::write(&session, transcript(100, 50, "s1", "2026-07-30T10")).unwrap();
        let state = Arc::new(crate::state::tests_support::make_test_app_state());

        let first = get_claude_session_stats_from_dir(&state, "all".into(), &projects)
            .await
            .unwrap();
        assert_eq!(first.total_input_tokens, 100);
        assert_eq!(first.total_sessions, 1);

        use std::io::Write;
        std::fs::OpenOptions::new()
            .append(true)
            .open(&session)
            .unwrap()
            .write_all(transcript(200, 80, "s1", "2026-07-30T11").as_bytes())
            .unwrap();
        wait_for_usage_event(&state, &session).await;
        let grown = get_claude_session_stats_from_dir(&state, "all".into(), &projects)
            .await
            .unwrap();
        assert_eq!(grown.total_input_tokens, 300);
        assert_eq!(grown.active_hours, 2);

        std::fs::write(&session, transcript(7, 3, "s2", "2026-07-30T12")).unwrap();
        wait_for_usage_event(&state, &session).await;
        let truncated = get_claude_session_stats_from_dir(&state, "all".into(), &projects)
            .await
            .unwrap();
        assert_eq!(truncated.total_input_tokens, 7);
        assert_eq!(truncated.total_sessions, 1);

        std::fs::remove_file(&session).unwrap();
        let deleted_file = get_claude_session_stats_from_dir(&state, "all".into(), &projects)
            .await
            .unwrap();
        assert_eq!(deleted_file.total_input_tokens, 0);
        assert_eq!(deleted_file.total_sessions, 0);

        std::fs::remove_dir(&project).unwrap();
        let deleted_project = get_claude_session_stats_from_dir(&state, "all".into(), &projects)
            .await
            .unwrap();
        assert!(deleted_project.per_project.is_empty());
        assert!(!state.claude_usage_cache.lock().contains_key("project-a"));
    }

    /// Catches a warm Usage refresh still statting every historical JSONL
    /// before its already-populated parse cache can reject unchanged files.
    #[tokio::test]
    async fn session_stats_warm_refresh_does_not_stat_unchanged_history() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().join("config"));
        let projects = dir.path().join("projects");
        let project = projects.join("project-a");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(
            project.join("session.jsonl"),
            transcript(10, 1, "s1", "2026-07-30T10"),
        )
        .unwrap();
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        SESSION_FILE_STATS.with(|stats| stats.set(0));
        let first = get_claude_session_stats_from_dir(&state, "all".into(), &projects)
            .await
            .unwrap();
        assert_eq!(first.total_input_tokens, 10);
        SESSION_FILE_STATS.with(|stats| assert_eq!(stats.get(), 1));
        let second = get_claude_session_stats_from_dir(&state, "all".into(), &projects)
            .await
            .unwrap();
        assert_eq!(second.total_input_tokens, 10);
        SESSION_FILE_STATS.with(|stats| {
            assert_eq!(
                stats.get(),
                1,
                "warm refresh must not stat unchanged history"
            )
        });
    }

    /// Catches a single active transcript invalidating every historical file,
    /// or a missed watcher event never being repaired by reconciliation.
    #[tokio::test]
    async fn session_stats_only_stats_the_changed_transcript_then_reconciles() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().join("config"));
        let projects = dir.path().join("projects");
        let project = projects.join("project-a");
        std::fs::create_dir_all(&project).unwrap();
        for name in ["active.jsonl", "history.jsonl"] {
            std::fs::write(project.join(name), transcript(10, 1, name, "2026-07-30T10")).unwrap();
        }
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        get_claude_session_stats_from_dir(&state, "all".into(), &projects)
            .await
            .unwrap();
        let active = project.join("active.jsonl");
        use std::io::Write;
        std::fs::OpenOptions::new()
            .append(true)
            .open(&active)
            .unwrap()
            .write_all(transcript(7, 1, "active.jsonl", "2026-07-30T11").as_bytes())
            .unwrap();
        wait_for_usage_event(&state, &active).await;
        SESSION_FILE_STATS.with(|stats| stats.set(0));
        let updated = get_claude_session_stats_from_dir(&state, "all".into(), &projects)
            .await
            .unwrap();
        assert_eq!(updated.total_input_tokens, 27);
        SESSION_FILE_STATS
            .with(|stats| assert_eq!(stats.get(), 1, "only the changed transcript needs metadata"));
        state.claude_usage_scan.lock().reconciled_at =
            Some(Instant::now() - Duration::from_secs(30 * 60));
        SESSION_FILE_STATS.with(|stats| stats.set(0));
        get_claude_session_stats_from_dir(&state, "all".into(), &projects)
            .await
            .unwrap();
        SESSION_FILE_STATS
            .with(|stats| assert_eq!(stats.get(), 2, "reconciliation validates all history"));
    }

    #[tokio::test]
    async fn session_stats_scope_excludes_other_projects() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().join("config"));
        let projects = dir.path().join("projects");
        for (slug, tokens) in [("project-a", 10), ("project-b", 90)] {
            let project = projects.join(slug);
            std::fs::create_dir_all(&project).unwrap();
            std::fs::write(
                project.join("session.jsonl"),
                transcript(tokens, 1, slug, "2026-07-30T10"),
            )
            .unwrap();
        }
        let state = Arc::new(crate::state::tests_support::make_test_app_state());

        let scoped = get_claude_session_stats_from_dir(&state, "project-a".into(), &projects)
            .await
            .unwrap();

        assert_eq!(scoped.total_input_tokens, 10);
        assert_eq!(scoped.per_project.len(), 1);
        assert!(scoped.per_project.contains_key("project-a"));
    }

    #[tokio::test]
    async fn session_stats_migrates_cache_without_hourly_buckets() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().join("config"));
        let projects = dir.path().join("projects");
        let project = projects.join("project-a");
        std::fs::create_dir_all(&project).unwrap();
        let session = project.join("session.jsonl");
        let content = transcript(25, 5, "s1", "2026-07-30T10");
        std::fs::write(&session, &content).unwrap();
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        state.claude_usage_cache.lock().insert(
            "project-a".into(),
            HashMap::from([(
                "session.jsonl".into(),
                CachedFileStats {
                    file_size: content.len() as u64,
                    assistant_message_count: 1,
                    hourly_tokens: HashMap::new(),
                    ..CachedFileStats::default()
                },
            )]),
        );

        let migrated = get_claude_session_stats_from_dir(&state, "all".into(), &projects)
            .await
            .unwrap();

        assert_eq!(migrated.total_input_tokens, 25);
        assert_eq!(migrated.active_hours, 1);
        assert!(
            !state.claude_usage_cache.lock()["project-a"]["session.jsonl"]
                .hourly_tokens
                .is_empty()
        );
    }

    #[test]
    fn orphan_pending_tokens_flushed_to_last_known_hour() {
        // Simulates an active session: assistant message followed by
        // stop_hook_summary (which has a timestamp but is NOT turn_duration),
        // then the file ends with no turn_duration to flush pending tokens.
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let path = dir.path().join("orphan.jsonl");

        let content = r#"{"type":"user","message":"hello"}
{"type":"assistant","message":{"model":"claude-opus-4-6","usage":{"input_tokens":100,"output_tokens":50,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}
{"type":"system","subtype":"turn_duration","timestamp":"2026-02-25T10:00:00Z","sessionId":"s1","durationMs":5000}
{"type":"user","message":"do more"}
{"type":"assistant","message":{"model":"claude-opus-4-6","usage":{"input_tokens":200,"output_tokens":80,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}
{"type":"system","subtype":"stop_hook_summary","timestamp":"2026-02-25T10:05:00Z","sessionId":"s1"}
{"type":"user","message":"and more"}
{"type":"assistant","message":{"model":"claude-opus-4-6","usage":{"input_tokens":150,"output_tokens":60,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}
"#;
        std::fs::write(&path, content).unwrap();

        let mut stats = CachedFileStats::default();
        parse_jsonl_file_from_offset(&path, 0, &mut stats).unwrap();

        // First assistant (100+50) should be bucketed at 10:00 via turn_duration
        let h10 = stats.hourly_tokens.get("2026-02-25T10").unwrap();
        // Second assistant (200+80) should be flushed by stop_hook_summary at 10:05 (same hour)
        // Third assistant (150+60) has NO following timestamp — should be flushed
        // using last_timestamp fallback (10:05, same hour bucket)
        assert_eq!(
            h10.input_tokens,
            100 + 200 + 150,
            "all tokens should be bucketed in hour 10"
        );
        assert_eq!(h10.output_tokens, 50 + 80 + 60);
    }

    #[test]
    fn orphan_tokens_at_eof_use_last_timestamp() {
        // File ends with assistant message, no system line after it.
        // There IS a prior turn_duration so last_timestamp is known.
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let path = dir.path().join("eof.jsonl");

        let content = r#"{"type":"assistant","message":{"model":"claude-opus-4-6","usage":{"input_tokens":100,"output_tokens":50,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}
{"type":"system","subtype":"turn_duration","timestamp":"2026-02-25T14:00:00Z","sessionId":"s1","durationMs":5000}
{"type":"assistant","message":{"model":"claude-opus-4-6","usage":{"input_tokens":300,"output_tokens":120,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}
"#;
        std::fs::write(&path, content).unwrap();

        let mut stats = CachedFileStats::default();
        parse_jsonl_file_from_offset(&path, 0, &mut stats).unwrap();

        // Both should end up in hour 14
        let h14 = stats.hourly_tokens.get("2026-02-25T14").unwrap();
        assert_eq!(h14.input_tokens, 100 + 300);
        assert_eq!(h14.output_tokens, 50 + 120);
    }

    #[test]
    fn stop_hook_summary_flushes_pending_tokens() {
        // stop_hook_summary with a timestamp should flush pending tokens
        // just like turn_duration does.
        let mut stats = CachedFileStats::default();

        let line1 = r#"{"type":"assistant","message":{"model":"claude-opus-4-6","usage":{"input_tokens":100,"output_tokens":50,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}"#;
        let line2 = r#"{"type":"system","subtype":"stop_hook_summary","timestamp":"2026-02-25T15:30:00Z","sessionId":"s1"}"#;

        let info1 = parse_jsonl_line(line1, &mut stats);
        assert!(info1.assistant_tokens.is_some());

        let info2 = parse_jsonl_line(line2, &mut stats);
        // stop_hook_summary should return a timestamp so caller can flush
        assert!(info2.timestamp.is_some());
        assert_eq!(info2.timestamp.unwrap(), "2026-02-25T15:30:00Z");
    }

    #[test]
    fn resolve_slug_to_path_real_dirs() {
        // This test runs on the dev machine where these paths exist.
        // On CI, the resolution will still produce a path (just may not match).
        let result = resolve_slug_to_path("-Users-stefano-straus-Gits-CC-Playground-tui-commander");
        let resolved = result.unwrap();
        // If the real directory exists, it should resolve correctly
        if std::path::Path::new("/Users/stefano.straus").exists() {
            assert!(
                resolved.contains("stefano.straus") || resolved.contains("stefano-straus"),
                "Expected path to contain user dir, got: {resolved}"
            );
        }
        // The path should at least start with /Users
        assert!(
            resolved.starts_with("/Users"),
            "Expected /Users prefix, got: {resolved}"
        );
    }

    #[test]
    fn resolve_slug_handles_empty() {
        // Empty slug should not resolve to a meaningful path
        let result = resolve_slug_to_path("");
        assert!(result.is_none() || result.as_deref() == Some("/"));
    }

    #[test]
    fn api_cache_isolated_by_profile() {
        let first = CredentialProfile::ConfigDir("/fixture/first".into());
        let second = CredentialProfile::ConfigDir("/fixture/second".into());
        assert!(try_get_fresh_cache(&first).is_none());
        assert!(get_stale_cache(&second).is_none());
        let data = UsageApiResponse {
            five_hour: Some(RateBucket {
                utilization: 42.0,
                resets_at: Some("2026-03-05T12:00:00Z".into()),
            }),
            ..Default::default()
        };
        cache_response(&first, &data);
        assert_eq!(
            try_get_fresh_cache(&first)
                .unwrap()
                .five_hour
                .unwrap()
                .utilization
                .to_bits(),
            42.0_f64.to_bits()
        );
        assert!(try_get_fresh_cache(&second).is_none());
        assert!(get_stale_cache(&second).is_none());

        API_CACHE.lock().insert(
            first.clone(),
            ApiCache {
                data,
                fetched_at: Instant::now() - API_CACHE_TTL - Duration::from_secs(1),
            },
        );
        assert!(try_get_fresh_cache(&first).is_none());
        assert!(get_stale_cache(&first).is_some());
        API_CACHE.lock().remove(&first);
    }

    #[test]
    fn rate_limit_backoff_isolated_by_profile() {
        let first = CredentialProfile::ConfigDir("/fixture/first".into());
        let second = CredentialProfile::ConfigDir("/fixture/second".into());
        let mut guard = RATE_LIMITED_UNTIL.lock();
        guard.insert(first.clone(), Instant::now() + Duration::from_secs(60));
        assert!(
            guard
                .get(&first)
                .is_some_and(|until| Instant::now() < *until)
        );
        assert!(!guard.contains_key(&second));
        guard.remove(&first);
    }

    #[test]
    fn usage_api_response_tolerates_extra_fields() {
        let json = r#"{"five_hour":{"utilization":0.5,"resets_at":"2026-02-23T12:00:00Z"},"new_field":"ignored","seven_day":null}"#;
        let parsed: UsageApiResponse = serde_json::from_str(json).unwrap();
        assert!(parsed.five_hour.is_some());
        assert!((parsed.five_hour.unwrap().utilization - 0.5).abs() < 0.001);
    }

    #[test]
    fn parse_credentials_json_normal_plan() {
        let json = r#"{
            "claudeAiOauth": {
                "accessToken": "sk-ant-oat-abc123",
                "refreshToken": "sk-ant-ort-def456",
                "expiresAt": 1787809644161,
                "scopes": ["user:inference", "user:profile"],
                "subscriptionType": "max",
                "rateLimitTier": "default"
            }
        }"#;
        let (token, plan) = parse_credentials_json(json).unwrap();
        assert_eq!(token.as_deref(), Some("sk-ant-oat-abc123"));
        let plan = plan.expect("plan should be present");
        assert_eq!(plan.subscription_type.as_deref(), Some("max"));
        assert_eq!(plan.rate_limit_tier.as_deref(), Some("default"));
        assert_eq!(plan.scopes, vec!["user:inference", "user:profile"]);
    }

    #[test]
    fn parse_credentials_json_enterprise_plan() {
        // Ground-truth shape captured from a real enterprise-plan credentials file
        // on 2026-08-26 (token values replaced with placeholders).
        let json = r#"{
            "claudeAiOauth": {
                "accessToken": "sk-ant-oat-enterprise-example",
                "refreshToken": "sk-ant-ort-enterprise-example",
                "expiresAt": 1787809644161,
                "scopes": [
                    "user:file_upload",
                    "user:inference",
                    "user:mcp_servers",
                    "user:profile",
                    "user:sessions:claude_code"
                ],
                "subscriptionType": "enterprise",
                "rateLimitTier": "default_claude_zero"
            }
        }"#;
        let (token, plan) = parse_credentials_json(json).unwrap();
        assert!(token.is_some());
        let plan = plan.expect("plan should be present");
        assert_eq!(plan.subscription_type.as_deref(), Some("enterprise"));
        assert_eq!(plan.rate_limit_tier.as_deref(), Some("default_claude_zero"));
        assert_eq!(plan.scopes.len(), 5);
    }

    #[test]
    fn parse_credentials_json_missing_oauth_key_returns_none() {
        let json = r#"{"someOtherKey": true}"#;
        let (token, plan) = parse_credentials_json(json).unwrap();
        assert!(token.is_none());
        assert!(plan.is_none());
    }

    #[test]
    fn parse_credentials_json_malformed_returns_err() {
        let result = parse_credentials_json("not valid json {{{");
        assert!(result.is_err());
    }

    #[test]
    fn parse_credentials_json_missing_scopes_defaults_to_empty() {
        let json = r#"{"claudeAiOauth": {"accessToken": "tok", "subscriptionType": "pro"}}"#;
        let (token, plan) = parse_credentials_json(json).unwrap();
        assert_eq!(token.as_deref(), Some("tok"));
        let plan = plan.expect("plan should be present even without scopes");
        assert_eq!(plan.rate_limit_tier, None);
        assert!(plan.scopes.is_empty());
    }

    #[test]
    fn parse_credentials_json_missing_access_token_still_yields_plan() {
        let json = r#"{"claudeAiOauth": {"subscriptionType": "enterprise"}}"#;
        let (token, plan) = parse_credentials_json(json).unwrap();
        assert!(token.is_none());
        assert_eq!(
            plan.expect("plan present").subscription_type.as_deref(),
            Some("enterprise")
        );
    }

    #[test]
    fn usage_api_response_handles_null_resets_at() {
        let json = r#"{"five_hour":{"utilization":0.42,"resets_at":null}}"#;
        let parsed: UsageApiResponse = serde_json::from_str(json).unwrap();
        let bucket = parsed.five_hour.unwrap();
        assert!((bucket.utilization - 0.42).abs() < 0.001);
        assert!(bucket.resets_at.is_none());
    }

    #[test]
    fn parse_unified_headers_full() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "anthropic-ratelimit-unified-status",
            "allowed_warning".parse().unwrap(),
        );
        headers.insert(
            "anthropic-ratelimit-unified-reset",
            "1773200000".parse().unwrap(),
        );
        headers.insert(
            "anthropic-ratelimit-unified-5h-utilization",
            "0.73".parse().unwrap(),
        );
        headers.insert(
            "anthropic-ratelimit-unified-5h-reset",
            "1773180000".parse().unwrap(),
        );
        headers.insert(
            "anthropic-ratelimit-unified-7d-utilization",
            "0.42".parse().unwrap(),
        );
        headers.insert(
            "anthropic-ratelimit-unified-7d-reset",
            "1773200000".parse().unwrap(),
        );

        let resp = parse_unified_rate_limit_headers(&headers);
        let five = resp.five_hour.unwrap();
        assert!((five.utilization - 73.0).abs() < 0.01); // converted to percentage
        assert!(five.resets_at.is_some());

        let seven = resp.seven_day.unwrap();
        assert!((seven.utilization - 42.0).abs() < 0.01);
    }

    #[test]
    fn parse_unified_headers_empty() {
        let headers = reqwest::header::HeaderMap::new();
        let resp = parse_unified_rate_limit_headers(&headers);
        assert!(resp.five_hour.is_none());
        assert!(resp.seven_day.is_none());
    }

    #[test]
    fn parse_unified_headers_rejected_with_representative_claim() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "anthropic-ratelimit-unified-status",
            "rejected".parse().unwrap(),
        );
        headers.insert(
            "anthropic-ratelimit-unified-representative-claim",
            "five_hour".parse().unwrap(),
        );
        headers.insert(
            "anthropic-ratelimit-unified-reset",
            "1773180000".parse().unwrap(),
        );
        headers.insert(
            "anthropic-ratelimit-unified-5h-utilization",
            "1.0".parse().unwrap(),
        );
        headers.insert(
            "anthropic-ratelimit-unified-5h-reset",
            "1773180000".parse().unwrap(),
        );

        let resp = parse_unified_rate_limit_headers(&headers);
        let five = resp.five_hour.unwrap();
        assert!((five.utilization - 100.0).abs() < 0.01);
        let meta = resp
            .meta
            .expect("meta should be populated from status headers");
        assert_eq!(meta.unified_status.as_deref(), Some("rejected"));
        assert_eq!(meta.representative_claim.as_deref(), Some("five_hour"));
    }

    #[test]
    fn parse_unified_headers_overage() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "anthropic-ratelimit-unified-status",
            "allowed".parse().unwrap(),
        );
        headers.insert(
            "anthropic-ratelimit-unified-overage-status",
            "allowed_warning".parse().unwrap(),
        );
        headers.insert(
            "anthropic-ratelimit-unified-overage-reset",
            "1773200000".parse().unwrap(),
        );
        headers.insert(
            "anthropic-ratelimit-unified-overage-utilization",
            "0.039".parse().unwrap(),
        );
        headers.insert(
            "anthropic-ratelimit-unified-overage-in-use",
            "true".parse().unwrap(),
        );

        let resp = parse_unified_rate_limit_headers(&headers);
        let extra = resp.extra_usage.unwrap();
        assert!(extra.is_enabled);
        assert!(extra.in_use);
        assert!(extra.resets_at.is_some());
        let util = extra.utilization.expect("utilization should be populated");
        assert!((util - 3.9).abs() < 0.01); // 0.039 → 3.9%
    }

    #[test]
    fn extra_usage_deserializes_real_api_schema() {
        // Ground-truth payload captured from /api/oauth/usage on 2026-04-10.
        let json = r#"{
            "five_hour": {"utilization": 3.0, "resets_at": "2026-04-10T11:00:00.580406+00:00"},
            "seven_day": {"utilization": 100.0, "resets_at": "2026-04-11T08:00:00.580427+00:00"},
            "seven_day_oauth_apps": null,
            "seven_day_opus": null,
            "seven_day_sonnet": {"utilization": 16.0, "resets_at": "2026-04-11T17:59:59.580435+00:00"},
            "seven_day_cowork": null,
            "extra_usage": {
                "is_enabled": true,
                "monthly_limit": 17000,
                "used_credits": 665.0,
                "utilization": 3.911764705882353
            }
        }"#;
        let parsed: UsageApiResponse = serde_json::from_str(json).unwrap();
        let extra = parsed.extra_usage.expect("extra_usage must deserialize");
        assert!(extra.is_enabled);
        assert_eq!(extra.monthly_limit, Some(17000));
        assert!((extra.used_credits.unwrap() - 665.0).abs() < 0.001);
        assert!((extra.utilization.unwrap() - 3.911764705882353).abs() < 0.001);
        // Headers-only fields should be default on the API-body path.
        assert!(extra.resets_at.is_none());
        assert!(!extra.in_use);
    }

    #[test]
    fn usage_api_response_deserializes_enterprise_schema_with_unknown_fields() {
        // Ground-truth payload captured from /api/oauth/usage for an enterprise
        // (subscriptionType "enterprise", rateLimitTier "default_claude_zero") account
        // on 2026-08-26. The named rate buckets are all null for this plan — usage is
        // only reported via `extra_usage` — alongside several experimental/unstable
        // field names this struct intentionally does not model (and must tolerate).
        let json = r#"{"five_hour":null,"seven_day":null,"seven_day_oauth_apps":null,"seven_day_opus":null,"seven_day_sonnet":null,"seven_day_cowork":null,"seven_day_omelette":null,"tangelo":null,"iguana_necktie":null,"omelette_promotional":null,"nimbus_quill":{"utilization":0.0,"resets_at":null,"limit_dollars":null,"used_dollars":null,"remaining_dollars":null},"cinder_cove":{"utilization":100.0,"resets_at":"2026-09-28T16:09:21.790107+00:00","limit_dollars":1000,"used_dollars":1000.0,"remaining_dollars":0.0},"amber_ladder":null,"extra_usage":{"is_enabled":true,"monthly_limit":500000,"used_credits":162709.0,"utilization":32.541799999999995,"currency":"USD","decimal_places":2,"disabled_reason":null,"user_disabled":false,"spend_limit_reached":false,"credits_ever_enabled":true,"daily":null,"weekly":null},"limits":[],"spend":{"used":{"amount_minor":162709,"currency":"USD","exponent":2},"limit":{"amount_minor":500000,"currency":"USD","exponent":2},"percent":33,"severity":"normal","enabled":true,"disabled_reason":null,"cap":{"money":null,"credits":{"amount_minor":500000,"exponent":2}},"balance":null,"auto_reload":null,"disclaimer":"Usage credits cover you when you hit your plan limits. [Learn more](https://support.claude.com/articles/12429409)","can_purchase_credits":false,"can_toggle":false},"member_dashboard_available":true}"#;

        let parsed: UsageApiResponse = serde_json::from_str(json).unwrap();
        assert!(parsed.five_hour.is_none());
        assert!(parsed.seven_day.is_none());
        assert!(parsed.seven_day_opus.is_none());
        assert!(parsed.seven_day_sonnet.is_none());
        assert!(parsed.seven_day_cowork.is_none());

        let extra = parsed.extra_usage.expect("extra_usage must deserialize");
        assert!(extra.is_enabled);
        assert_eq!(extra.monthly_limit, Some(500000));
        assert!((extra.used_credits.unwrap() - 162709.0).abs() < 0.001);
        assert!((extra.utilization.unwrap() - 32.5418).abs() < 0.001);

        // plan/meta are never deserialized from the API body — only injected by the backend.
        assert!(parsed.plan.is_none());
        assert!(parsed.meta.is_none());
    }

    /// Call the real Anthropic usage API and verify the response deserializes.
    /// Requires a valid OAuth token in the macOS keychain or ~/.claude/.credentials.json.
    /// Skipped automatically if no token is available.
    #[tokio::test]
    async fn live_usage_api_deserializes() {
        let token = match read_claude_credentials(None) {
            Ok((Some(t), _)) => t,
            _ => {
                eprintln!("Skipping live API test: no OAuth token available");
                return;
            }
        };
        match fetch_usage_from_api(&token).await {
            Ok(data) => {
                // At minimum the response should parse — print it for manual inspection
                eprintln!("Live API response: {data:?}");
            }
            Err(e) if e.status == 429 => {
                eprintln!("Skipping live API test: rate limited (429)");
            }
            Err(e) if e.status == 401 => {
                // Token present but rejected (expired/rotated) — same outcome as
                // "no token available", skip rather than fail the suite.
                eprintln!("Skipping live API test: token rejected (401)");
            }
            Err(e) if e.status == 0 => {
                eprintln!("Skipping live API test: network error ({})", e.message);
            }
            Err(e) => {
                panic!(
                    "Live API call failed: status={} msg={}",
                    e.status, e.message
                );
            }
        }
    }
}
