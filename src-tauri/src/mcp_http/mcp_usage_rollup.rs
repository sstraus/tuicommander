//! Persistent daily MCP usage rollup (#2808-5998).
//!
//! One SQLite row per UTC day x client kind x caller role x tool x action. A row
//! holds counts and a response-size histogram only. No argument, response body,
//! error text, session id or peer name is ever stored: every key part comes from
//! a fixed set (client kind, role) or from the native catalogue (tool, action).
//! The file lives beside `progress.sqlite3` in the app config directory.
//!
//! Calls are summed in memory and written in batches, so the hot path does no
//! I/O. A crash loses at most one flush interval of counts.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{Json, extract::State};
use rusqlite::{Connection, params};
use serde_json::{Value, json};

use crate::AppState;

const STORE_FILE: &str = "mcp_usage.sqlite3";
const RETENTION_DAYS: i64 = 90;
const READ_DAYS: i64 = 7;
const FLUSH_INTERVAL: Duration = Duration::from_secs(30);
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);
/// Bucket `i` holds sizes in `[2^(i-1), 2^i)` bytes; bucket 0 holds zero. The last
/// bucket is open-ended (>= 8 MiB), which no MCP reply approaches.
const BUCKETS: usize = 24;
const MS_PER_DAY: u64 = 86_400_000;

/// One writer or reader at a time inside the process. Two first opens at once
/// raced on the WAL switch and the schema creation, which SQLite answers with an
/// immediate BUSY that the busy timeout does not retry.
static STORE: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

const CLIENTS: [&str; 4] = ["claude", "codex", "ego", "grok"];

/// Client kind from the MCP client name or the user agent. Whole alphanumeric
/// tokens only, so a name such as "category-tool" never reads as `ego`.
pub(super) fn client_kind(name: Option<&str>, user_agent: Option<&str>) -> &'static str {
    for text in [name, user_agent].into_iter().flatten() {
        let lower = text.to_ascii_lowercase();
        for token in lower.split(|c: char| !c.is_ascii_alphanumeric()) {
            if let Some(kind) = CLIENTS.iter().find(|kind| **kind == token) {
                return kind;
            }
        }
    }
    "other"
}

/// Split a catalogue operation name (`tool.action`, `tool`, `opaque:<digest>`)
/// into the stored key. An unrecognized tool keeps no digest: the rollup has no
/// use for it and a digest is a grouping key for a caller-chosen string.
pub(super) fn split_operation(operation: &str) -> (String, String) {
    if operation.starts_with("opaque:") {
        return ("opaque".to_owned(), String::new());
    }
    match operation.split_once('.') {
        Some((tool, action)) => (tool.to_owned(), action.to_owned()),
        None => (operation.to_owned(), String::new()),
    }
}

fn bucket(bytes: u64) -> usize {
    if bytes == 0 {
        0
    } else {
        (bytes.bit_width() as usize).min(BUCKETS - 1)
    }
}

/// Largest size the bucket can hold: the quantile estimate before the max clamp.
fn bucket_upper(index: usize) -> u64 {
    if index == 0 { 0 } else { (1u64 << index) - 1 }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
struct Key {
    day: String,
    client: String,
    role: String,
    tool: String,
    action: String,
}

#[derive(Clone, Debug)]
struct Agg {
    calls: u64,
    app_errors: u64,
    invalid_arguments: u64,
    hist: [u64; BUCKETS],
    bytes_max: u64,
    catalogue_bytes: u64,
}

impl Default for Agg {
    fn default() -> Self {
        Self {
            calls: 0,
            app_errors: 0,
            invalid_arguments: 0,
            hist: [0; BUCKETS],
            bytes_max: 0,
            catalogue_bytes: 0,
        }
    }
}

impl Agg {
    fn merge(&mut self, other: &Agg) {
        self.calls += other.calls;
        self.app_errors += other.app_errors;
        self.invalid_arguments += other.invalid_arguments;
        for (mine, theirs) in self.hist.iter_mut().zip(other.hist) {
            *mine += theirs;
        }
        self.bytes_max = self.bytes_max.max(other.bytes_max);
        self.catalogue_bytes += other.catalogue_bytes;
    }

    /// Estimate of the `q`-th quantile: the upper bound of the bucket that holds
    /// the rank, never above the true maximum.
    fn quantile(&self, q: f64) -> u64 {
        let total: u64 = self.hist.iter().sum();
        if total == 0 {
            return 0;
        }
        let rank = ((total as f64) * q).ceil().max(1.0) as u64;
        let mut seen = 0;
        for (index, count) in self.hist.iter().enumerate() {
            seen += count;
            if seen >= rank {
                return bucket_upper(index).min(self.bytes_max);
            }
        }
        self.bytes_max
    }
}

/// What one tools/call contributed. Built from the reply, never from arguments.
pub(super) struct Observation {
    pub client: &'static str,
    pub role: &'static str,
    pub operation: String,
    pub response_bytes: u64,
    pub application_error: bool,
    pub invalid_arguments: bool,
}

#[derive(Default)]
pub(crate) struct Rollup {
    pending: parking_lot::Mutex<HashMap<Key, Agg>>,
    last_flush: parking_lot::Mutex<Option<Instant>>,
}

fn today_ms() -> u64 {
    super::mcp_transport_peer::now_unix_ms()
}

/// UTC civil day (`YYYY-MM-DD`) of a unix millisecond timestamp.
fn day_of(ms: u64) -> String {
    // Howard Hinnant's civil-from-days, on whole days since 1970-01-01.
    let days = (ms / MS_PER_DAY) as i64 + 719_468;
    let era = days.div_euclid(146_097);
    let doe = days.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

fn db_path() -> PathBuf {
    crate::config::config_dir().join(STORE_FILE)
}

fn connect() -> Result<Connection, String> {
    let path = db_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("mcp usage store: create dir: {e}"))?;
    }
    let conn = Connection::open(&path).map_err(|e| format!("mcp usage store: open: {e}"))?;
    conn.busy_timeout(BUSY_TIMEOUT)
        .map_err(|e| format!("mcp usage store: busy timeout: {e}"))?;
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(|e| format!("mcp usage store: WAL: {e}"))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS mcp_usage_daily (
           day               TEXT NOT NULL,
           client            TEXT NOT NULL,
           role              TEXT NOT NULL,
           tool              TEXT NOT NULL,
           action            TEXT NOT NULL,
           calls             INTEGER NOT NULL,
           app_errors        INTEGER NOT NULL,
           invalid_arguments INTEGER NOT NULL,
           bytes_hist        TEXT NOT NULL,
           bytes_max         INTEGER NOT NULL,
           catalogue_bytes   INTEGER NOT NULL,
           PRIMARY KEY (day, client, role, tool, action)
         ) WITHOUT ROWID;",
    )
    .map_err(|e| format!("mcp usage store: schema: {e}"))?;
    Ok(conn)
}

fn encode_hist(hist: &[u64; BUCKETS]) -> String {
    hist.iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn decode_hist(text: &str) -> [u64; BUCKETS] {
    let mut hist = [0; BUCKETS];
    for (slot, part) in hist.iter_mut().zip(text.split(',')) {
        *slot = part.parse().unwrap_or(0);
    }
    hist
}

impl Rollup {
    fn add(&self, key: Key, update: impl FnOnce(&mut Agg)) {
        update(self.pending.lock().entry(key).or_default());
    }

    fn key(client: &str, role: &str, operation: &str) -> Key {
        let (tool, action) = split_operation(operation);
        Key {
            day: day_of(today_ms()),
            client: client.to_owned(),
            role: role.to_owned(),
            tool,
            action,
        }
    }

    pub(super) fn record_call(&self, obs: &Observation) {
        self.add(Self::key(obs.client, obs.role, &obs.operation), |agg| {
            agg.calls += 1;
            if obs.invalid_arguments {
                agg.invalid_arguments += 1;
            } else if obs.application_error {
                agg.app_errors += 1;
            }
            agg.hist[bucket(obs.response_bytes)] += 1;
            agg.bytes_max = agg.bytes_max.max(obs.response_bytes);
            // The schema lookups are catalogue reads: their reply is input cost.
            if matches!(obs.operation.as_str(), "search_tools" | "get_tool_schema") {
                agg.catalogue_bytes += obs.response_bytes;
            }
        });
    }

    /// A `tools/list` reply: catalogue bytes served, not a tool call.
    pub(super) fn record_catalogue(&self, client: &str, role: &str, bytes: u64) {
        self.add(Self::key(client, role, "tools/list"), |agg| {
            agg.catalogue_bytes += bytes;
        });
    }

    /// Write the pending counts and prune past retention. A failed write puts the
    /// counts back so a later flush retries them.
    pub(crate) fn flush(&self) -> Result<(), String> {
        let _store = STORE.lock();
        *self.last_flush.lock() = Some(Instant::now());
        let drained = std::mem::take(&mut *self.pending.lock());
        let result = Self::write(&drained);
        if result.is_err() {
            let mut pending = self.pending.lock();
            for (key, agg) in drained {
                pending.entry(key).or_default().merge(&agg);
            }
        }
        result
    }

    fn write(rows: &HashMap<Key, Agg>) -> Result<(), String> {
        let mut conn = connect()?;
        let tx = conn
            .transaction()
            .map_err(|e| format!("mcp usage store: begin: {e}"))?;
        for (key, agg) in rows {
            let existing: Option<(i64, i64, i64, String, i64, i64)> = tx
                .query_row(
                    "SELECT calls, app_errors, invalid_arguments, bytes_hist, bytes_max, catalogue_bytes
                     FROM mcp_usage_daily WHERE day=?1 AND client=?2 AND role=?3 AND tool=?4 AND action=?5",
                    params![key.day, key.client, key.role, key.tool, key.action],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
                )
                .ok();
            let mut merged = agg.clone();
            if let Some((calls, errors, invalid, hist, max, catalogue)) = existing {
                let stored = Agg {
                    calls: calls as u64,
                    app_errors: errors as u64,
                    invalid_arguments: invalid as u64,
                    hist: decode_hist(&hist),
                    bytes_max: max as u64,
                    catalogue_bytes: catalogue as u64,
                };
                merged.merge(&stored);
            }
            tx.execute(
                "INSERT OR REPLACE INTO mcp_usage_daily
                 (day, client, role, tool, action, calls, app_errors, invalid_arguments, bytes_hist, bytes_max, catalogue_bytes)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
                params![
                    key.day,
                    key.client,
                    key.role,
                    key.tool,
                    key.action,
                    merged.calls as i64,
                    merged.app_errors as i64,
                    merged.invalid_arguments as i64,
                    encode_hist(&merged.hist),
                    merged.bytes_max as i64,
                    merged.catalogue_bytes as i64
                ],
            )
            .map_err(|e| format!("mcp usage store: upsert: {e}"))?;
        }
        tx.execute(
            "DELETE FROM mcp_usage_daily WHERE day < ?1",
            params![day_of(
                today_ms().saturating_sub(RETENTION_DAYS as u64 * MS_PER_DAY)
            )],
        )
        .map_err(|e| format!("mcp usage store: prune: {e}"))?;
        tx.commit()
            .map_err(|e| format!("mcp usage store: commit: {e}"))
    }

    /// Flush on a worker thread when the interval has passed. Never blocks the call.
    pub(super) fn flush_if_due(self: &Arc<Self>) {
        let due = self
            .last_flush
            .lock()
            .is_none_or(|at| at.elapsed() >= FLUSH_INTERVAL);
        if !due {
            return;
        }
        *self.last_flush.lock() = Some(Instant::now());
        let this = Arc::clone(self);
        // A flush failure is retried by the next one; nothing waits on this result.
        tokio::task::spawn_blocking(move || {
            if let Err(error) = this.flush() {
                tracing::warn!("{error}");
            }
        });
    }

    /// Last `READ_DAYS` days grouped per role, then per tool action. Clients and
    /// days are merged; a quantile is estimated from the merged histogram.
    pub(crate) fn report(&self) -> Result<Value, String> {
        self.flush()?;
        let _store = STORE.lock();
        let conn = connect()?;
        let since = day_of(today_ms().saturating_sub((READ_DAYS - 1) as u64 * MS_PER_DAY));
        let mut stmt = conn
            .prepare(
                "SELECT role, tool, action, calls, app_errors, invalid_arguments, bytes_hist, bytes_max, catalogue_bytes
                 FROM mcp_usage_daily WHERE day >= ?1",
            )
            .map_err(|e| format!("mcp usage store: read: {e}"))?;
        let rows = stmt
            .query_map(params![since], |r| {
                Ok((
                    (
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ),
                    Agg {
                        calls: r.get::<_, i64>(3)? as u64,
                        app_errors: r.get::<_, i64>(4)? as u64,
                        invalid_arguments: r.get::<_, i64>(5)? as u64,
                        hist: decode_hist(&r.get::<_, String>(6)?),
                        bytes_max: r.get::<_, i64>(7)? as u64,
                        catalogue_bytes: r.get::<_, i64>(8)? as u64,
                    },
                ))
            })
            .map_err(|e| format!("mcp usage store: read: {e}"))?;
        let mut grouped: std::collections::BTreeMap<
            String,
            std::collections::BTreeMap<(String, String), Agg>,
        > = Default::default();
        for row in rows {
            let ((role, tool, action), agg) =
                row.map_err(|e| format!("mcp usage store: row: {e}"))?;
            grouped
                .entry(role)
                .or_default()
                .entry((tool, action))
                .or_default()
                .merge(&agg);
        }
        let roles: Vec<Value> = grouped
            .into_iter()
            .map(|(role, actions)| {
                let mut total = Agg::default();
                let mut list: Vec<Value> = Vec::new();
                for ((tool, action), agg) in &actions {
                    total.merge(agg);
                    list.push(json!({
                        "tool": tool, "action": action,
                        "calls": agg.calls, "application_errors": agg.app_errors,
                        "invalid_arguments": agg.invalid_arguments,
                        "response_bytes_p50": agg.quantile(0.5),
                        "response_bytes_p95": agg.quantile(0.95),
                        "response_bytes_max": agg.bytes_max,
                        "catalogue_bytes": agg.catalogue_bytes,
                    }));
                }
                list.sort_by(|a, b| b["calls"].as_u64().cmp(&a["calls"].as_u64()));
                json!({
                    "role": role, "calls": total.calls,
                    "application_errors": total.app_errors,
                    "invalid_arguments": total.invalid_arguments,
                    "catalogue_bytes": total.catalogue_bytes,
                    "actions": list,
                })
            })
            .collect();
        Ok(json!({"enabled": true, "days": READ_DAYS, "roles": roles}))
    }
}

/// One projection for the HTTP route and the Tauri command, so the transports cannot drift.
pub(crate) fn snapshot(state: &AppState) -> Value {
    if !state.config.read().mcp_usage_rollup_enabled {
        return json!({"enabled": false, "days": READ_DAYS, "roles": []});
    }
    state.mcp.usage_rollup.report().unwrap_or_else(|error| {
        tracing::warn!("{error}");
        json!({"enabled": true, "days": READ_DAYS, "roles": [], "error": "store_unavailable"})
    })
}

pub(super) async fn endpoint(State(state): State<Arc<AppState>>) -> Json<Value> {
    // The read opens SQLite, so it stays off the async workers.
    let value = tokio::task::spawn_blocking(move || snapshot(&state))
        .await
        .unwrap_or_else(|_| json!({"enabled": true, "days": READ_DAYS, "roles": []}));
    Json(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Catches: a client name that merely contains "ego" is classed as ego.
    #[test]
    fn client_kind_matches_whole_tokens_only() {
        assert_eq!(client_kind(Some("claude-code"), None), "claude");
        assert_eq!(client_kind(None, Some("codex_cli_rs/0.1")), "codex");
        assert_eq!(client_kind(Some("category-tool"), None), "other");
        assert_eq!(client_kind(None, None), "other");
    }

    // Catches: day arithmetic off by one at month, leap-year or epoch edges.
    #[test]
    fn day_of_is_utc_civil_date() {
        assert_eq!(day_of(0), "1970-01-01");
        assert_eq!(day_of(951_782_400_000), "2000-02-29");
        assert_eq!(day_of(1_793_750_400_000 + MS_PER_DAY - 1), "2026-11-04");
    }

    // Catches: a quantile above the true maximum, or one that ignores the tail.
    #[test]
    fn quantiles_come_from_the_histogram_and_never_exceed_max() {
        let mut agg = Agg::default();
        for bytes in [100u64; 95].into_iter().chain([5000u64; 5]) {
            agg.hist[bucket(bytes)] += 1;
            agg.bytes_max = agg.bytes_max.max(bytes);
        }
        assert_eq!(agg.quantile(0.5), 127);
        assert_eq!(agg.quantile(0.95), 127);
        assert_eq!(agg.quantile(0.99), 5000);
    }

    fn obs(operation: &str, bytes: u64) -> Observation {
        Observation {
            client: "claude",
            role: "managed_child",
            operation: operation.to_owned(),
            response_bytes: bytes,
            application_error: false,
            invalid_arguments: false,
        }
    }

    // Catches: a second flush replaces the stored day instead of adding to it, and
    // rows older than the retention window survive a write.
    #[test]
    fn flushes_add_to_the_stored_day_and_prune_past_retention() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let rollup = Rollup::default();
        rollup.record_call(&obs("agent.send", 100));
        rollup.flush().unwrap();
        rollup.record_call(&obs("agent.send", 5000));
        rollup.record_call(&obs("agent.send", 100));
        {
            let conn = connect().unwrap();
            let old = day_of(today_ms() - (RETENTION_DAYS as u64 + 1) * MS_PER_DAY);
            conn.execute(
                "INSERT INTO mcp_usage_daily VALUES (?1,'claude','managed_child','agent','old',9,0,0,?2,1,0)",
                params![old, encode_hist(&[0; BUCKETS])],
            )
            .unwrap();
        }
        let report = rollup.report().unwrap();
        let action = &report["roles"][0]["actions"][0];
        assert_eq!(action["calls"], 3, "{report}");
        assert_eq!(action["response_bytes_max"], 5000, "{report}");
        assert_eq!(report["roles"][0]["actions"].as_array().unwrap().len(), 1);
        let rows: i64 = connect()
            .unwrap()
            .query_row("SELECT count(*) FROM mcp_usage_daily", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1, "the 91-day-old row was not pruned");
    }
}
