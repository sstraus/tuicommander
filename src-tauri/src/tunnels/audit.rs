use chrono::{DateTime, Utc};
use rusqlite::{Connection, Result, params};
use serde::{Deserialize, Serialize};
use std::cell::OnceCell;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelEvent {
    pub id: i64,
    pub timestamp: DateTime<Utc>,
    pub tunnel_id: String,
    pub kind: EventKind,
    pub detail: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Started,
    Connected,
    Disconnected,
    Error,
    Retry,
    Stopped,
}

const SCHEMA: crate::db::Schema = crate::db::Schema {
    name: "tunnel audit",
    migrations: &[&["CREATE TABLE tunnel_events (
            id        INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            tunnel_id TEXT    NOT NULL,
            kind      TEXT    NOT NULL,
            detail    TEXT    NOT NULL DEFAULT '{}'
        );
        CREATE INDEX idx_tunnel_events_tunnel_id ON tunnel_events(tunnel_id);
        CREATE INDEX idx_tunnel_events_timestamp ON tunnel_events(timestamp);"]],
};

pub struct AuditLog {
    db_path: PathBuf,
    /// Opened on first use — see [`AuditLog::open`].
    conn: OnceCell<Connection>,
}

impl AuditLog {
    /// Prepare a handle to the audit database at `db_path` without touching the disk.
    ///
    /// The connection is deferred to the first insert or query. `AppState::new`
    /// builds this on every launch and most launches never start a tunnel, so
    /// connecting here put `Connection::open`, a WAL-mode pragma, three
    /// `CREATE ... IF NOT EXISTS` statements, retention and WAL recovery on the
    /// startup path for a database nothing would go on to read.
    pub fn open(db_path: &Path) -> Result<Self> {
        Ok(Self {
            db_path: db_path.to_path_buf(),
            conn: OnceCell::new(),
        })
    }

    /// The live connection, connecting and migrating on first use.
    fn conn(&self) -> Result<&Connection> {
        if let Some(conn) = self.conn.get() {
            return Ok(conn);
        }
        let conn = Self::connect(&self.db_path)?;
        // `set` can only fail if the cell is already full. Callers reach us
        // through the `Mutex` in `AppState`, so nothing else can have filled it —
        // and if it somehow did, that connection is equally good.
        let _ = self.conn.set(conn);
        Ok(self.conn.get().expect("connection was just stored"))
    }

    /// Open the database and ensure the schema. Retention: `crate::db::retention`.
    fn connect(db_path: &Path) -> Result<Connection> {
        crate::db::open_with(db_path, &SCHEMA).map_err(|e| {
            rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CANTOPEN),
                Some(e),
            )
        })
    }

    /// Insert a new event and return the `rowid` of the inserted row.
    pub fn insert(
        &self,
        tunnel_id: &str,
        kind: EventKind,
        detail: serde_json::Value,
    ) -> Result<i64> {
        let kind_str = serde_json::to_string(&kind)
            .map(|s| s.trim_matches('"').to_owned())
            .unwrap_or_else(|_| "unknown".to_owned());
        let detail_str = detail.to_string();

        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO tunnel_events (tunnel_id, kind, detail) VALUES (?1, ?2, ?3)",
            params![tunnel_id, kind_str, detail_str],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Return the most recent `limit` events for `tunnel_id`, newest first.
    pub fn query_by_tunnel(&self, tunnel_id: &str, limit: usize) -> Result<Vec<TunnelEvent>> {
        let mut stmt = self.conn()?.prepare(
            "SELECT id, timestamp, tunnel_id, kind, detail
             FROM tunnel_events
             WHERE tunnel_id = ?1
             ORDER BY id DESC
             LIMIT ?2",
        )?;

        let rows = stmt.query_map(params![tunnel_id, limit as i64], row_to_event)?;
        rows.collect()
    }
}

/// Map a SQLite row to a [`TunnelEvent`].
fn row_to_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<TunnelEvent> {
    let id: i64 = row.get(0)?;
    let timestamp_str: String = row.get(1)?;
    let tunnel_id: String = row.get(2)?;
    let kind_str: String = row.get(3)?;
    let detail_str: String = row.get(4)?;

    let timestamp = DateTime::parse_from_rfc3339(&timestamp_str)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|e| {
            tracing::warn!(source = "audit", row_id = id, raw = %timestamp_str, error = %e, "Corrupt audit timestamp, substituting now()");
            Utc::now()
        });

    let kind: EventKind =
        serde_json::from_value(serde_json::Value::String(kind_str.clone())).unwrap_or_else(|e| {
            tracing::warn!(source = "audit", row_id = id, raw = %kind_str, error = %e, "Unknown audit event kind, substituting Error");
            EventKind::Error
        });

    let detail: serde_json::Value =
        serde_json::from_str(&detail_str).unwrap_or_else(|e| {
            tracing::warn!(source = "audit", row_id = id, raw = %detail_str, error = %e, "Corrupt audit detail JSON, substituting null");
            serde_json::Value::Null
        });

    Ok(TunnelEvent {
        id,
        timestamp,
        tunnel_id,
        kind,
        detail,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn temp_db() -> (AuditLog, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = AuditLog::open(&dir.path().join("audit.db")).expect("open");
        (db, dir)
    }

    // Fallback when tempfile crate is somehow unavailable — unused in practice
    // because Cargo.toml includes tempfile = "3".
    #[allow(dead_code)]
    fn temp_db_fallback() -> AuditLog {
        let path = env::temp_dir().join(format!("tuic_audit_test_{}.db", std::process::id()));
        AuditLog::open(&path).expect("open")
    }

    #[test]
    fn insert_and_query_by_tunnel() {
        let (log, _dir) = temp_db();
        let id = log
            .insert("t1", EventKind::Started, serde_json::json!({"port": 22}))
            .expect("insert");
        assert!(id > 0);

        let events = log.query_by_tunnel("t1", 10).expect("query");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].tunnel_id, "t1");
        assert_eq!(events[0].kind, EventKind::Started);
        assert_eq!(events[0].detail["port"], 22);
    }

    #[test]
    fn query_limit_returns_most_recent() {
        let (log, _dir) = temp_db();

        for i in 0..10_i64 {
            log.insert("t3", EventKind::Retry, serde_json::json!({"seq": i}))
                .expect("insert");
        }

        let events = log.query_by_tunnel("t3", 5).expect("query");
        assert_eq!(events.len(), 5, "should return exactly 5 events");

        // Newest first — seq 9 should be first.
        assert_eq!(events[0].detail["seq"], 9);
    }

    /// Performance test: 10 000 inserts must complete in under 500 ms.
    /// Marked #[ignore] to avoid flakiness in slow CI environments.
    #[test]
    #[ignore = "performance test — run explicitly with `cargo test -- --ignored`"]
    fn bulk_insert_performance() {
        let (log, _dir) = temp_db();
        let start = std::time::Instant::now();

        for i in 0..10_000_i64 {
            log.insert("perf", EventKind::Connected, serde_json::json!({"i": i}))
                .expect("insert");
        }

        let elapsed = start.elapsed();
        assert!(
            elapsed.as_millis() < 500,
            "10 000 inserts took {elapsed:?}, expected < 500 ms"
        );
    }

    /// The WAL needs no manual checkpoint: SQLite's default
    /// `wal_autocheckpoint` (1000 pages) bounds it while the connection stays
    /// open. This test pins it, so setting `wal_autocheckpoint=0` — the one change that
    /// would let the sidecar outgrow the database — cannot pass unnoticed.
    #[test]
    fn the_write_ahead_log_stays_bounded_without_a_manual_checkpoint() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("audit.db");
        let wal = dir.path().join("audit.db-wal");

        let pages: i64 = {
            let log = AuditLog::open(&path).expect("open");
            // Every autocommit insert otherwise fsyncs the WAL: 5000 of them cost
            // 13-58 s on a loaded box. The checkpoint threshold counts pages, not
            // syncs, so dropping the fsync leaves what this test pins untouched.
            log.conn()
                .expect("connect")
                .execute_batch("PRAGMA synchronous=OFF;")
                .expect("pragma");
            for i in 0..5_000_i64 {
                log.insert("bulk", EventKind::Connected, serde_json::json!({"i": i}))
                    .expect("insert");
            }
            let auto: i64 = log
                .conn()
                .expect("connect")
                .query_row("PRAGMA wal_autocheckpoint", [], |r| r.get(0))
                .expect("pragma");
            assert!(auto > 0, "autocheckpoint must stay enabled, got {auto}");

            let page_size: i64 = log
                .conn()
                .expect("connect")
                .query_row("PRAGMA page_size", [], |r| r.get(0))
                .expect("pragma");
            let wal_len = std::fs::metadata(&wal).map(|m| m.len() as i64).unwrap_or(0);
            // Allow a generous multiple: the checkpoint runs *after* the threshold
            // is crossed, and a reader can defer it briefly.
            assert!(
                wal_len < auto * page_size * 4,
                "WAL grew to {wal_len} bytes with autocheckpoint at {auto} pages of {page_size}"
            );
            auto
        };
        assert!(pages > 0);
    }

    /// `AppState::new` builds this on every launch; most launches never start a
    /// tunnel. Connecting eagerly put `Connection::open`, a WAL pragma, three
    /// `CREATE ... IF NOT EXISTS` statements and WAL recovery on the startup path
    /// for a database nothing would read.
    #[test]
    fn preparing_the_log_does_not_touch_the_disk() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("audit.db");

        let _log = AuditLog::open(&path).expect("prepare");

        assert!(
            !path.exists(),
            "the tunnel audit DB must not be created before a tunnel writes to it"
        );
    }

    /// The other half of laziness: deferring must not lose the schema.
    #[test]
    fn the_first_write_opens_and_migrates_the_database() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("audit.db");
        let log = AuditLog::open(&path).expect("prepare");

        log.insert("t1", EventKind::Started, serde_json::json!({"port": 22}))
            .expect("first insert must create and migrate the DB");

        assert!(path.exists(), "the first write must create the DB");
        let events = log.query_by_tunnel("t1", 10).expect("query");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].detail["port"], 22);
    }

    /// A read is a first use too — the audit HTTP/IPC endpoints must not see a
    /// missing-table error just because no tunnel has run yet.
    #[test]
    fn a_query_before_any_write_returns_empty_rather_than_failing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let log = AuditLog::open(&dir.path().join("audit.db")).expect("prepare");

        let events = log
            .query_by_tunnel("never-existed", 10)
            .expect("a read on an unused audit log must succeed");
        assert!(events.is_empty());
    }

    #[test]
    fn wal_mode_is_active() {
        let (log, _dir) = temp_db();
        let mode: String = log
            .conn()
            .expect("connect")
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .expect("pragma");
        assert_eq!(mode, "wal");
    }

    #[test]
    fn empty_query_returns_empty_vec() {
        let (log, _dir) = temp_db();
        let events = log
            .query_by_tunnel("nonexistent", 10)
            .expect("query should not panic");
        assert!(events.is_empty());
    }
}
