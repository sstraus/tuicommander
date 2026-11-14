use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug)]
pub struct RunStore {
    db_path: PathBuf,
}

/// Layout version recorded in `PRAGMA user_version`.
// 20: neutral run identities and immutable versioned event bodies; no legacy conversion.
// 21: neutral execution decisions; older owners must not ignore progressed runs.
// 22: correlated neutral effect lifecycle and transactional resource ownership.
// 23: the legacy run engine's `workflow_*` tables are backed up once, then dropped.
const RUN_STORE_SCHEMA_VERSION: i64 = 23;

impl RunStore {
    pub(in crate::workflows) fn database_path(&self) -> &Path {
        &self.db_path
    }

    pub fn open() -> Result<Self, String> {
        Self::open_at(&crate::config::config_dir().join("workflow_runs.sqlite3"))
    }

    pub(crate) fn open_at(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create workflow run directory: {e}"))?;
        }
        let store = Self {
            db_path: path.to_path_buf(),
        };
        store.connect()?;
        Ok(store)
    }

    pub(super) fn connect(&self) -> Result<Connection, String> {
        let conn =
            Connection::open(&self.db_path).map_err(|e| format!("open workflow run store: {e}"))?;
        conn.busy_timeout(Duration::from_secs(5))
            .map_err(|e| format!("workflow run busy timeout: {e}"))?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| format!("workflow run WAL: {e}"))?;
        crate::sqlite_once::hold_open(&self.db_path);
        let stored_version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(|e| format!("read workflow run schema version: {e}"))?;
        if stored_version > RUN_STORE_SCHEMA_VERSION {
            return Err(format!(
                "workflow run store schema version {stored_version} is newer than this build supports ({RUN_STORE_SCHEMA_VERSION}); update TUICommander"
            ));
        }
        if stored_version < RUN_STORE_SCHEMA_VERSION {
            drop_legacy_tables(&conn, &self.db_path)?;
        }
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| format!("workflow run foreign keys: {e}"))?;
        if stored_version == RUN_STORE_SCHEMA_VERSION
            && crate::sqlite_once::is_prepared(&self.db_path)
        {
            return Ok(conn);
        }
        super::neutral::prepare_db(&conn)?;
        if stored_version < RUN_STORE_SCHEMA_VERSION {
            conn.pragma_update(None, "user_version", RUN_STORE_SCHEMA_VERSION)
                .map_err(|e| format!("record workflow run schema version: {e}"))?;
        }
        crate::sqlite_once::mark_prepared(&self.db_path);
        Ok(conn)
    }
}

/// The removed legacy run engine kept every table under the `workflow_` prefix,
/// plus its command journal; the neutral engine uses `neutral_*` and `native_*`.
/// Write one backup next to the store, then drop those tables in one transaction.
/// Parent and child tables drop in any order.
fn drop_legacy_tables(conn: &Connection, path: &Path) -> Result<(), String> {
    let legacy = legacy_tables(conn)?;
    if legacy.is_empty() {
        return Ok(());
    }
    crate::sqlite_once::backup_before_legacy_removal(conn, path)?;
    // A dropped parent table must not cascade into its legacy children:
    // foreign keys are off for this transaction and turned on by the caller.
    conn.pragma_update(None, "foreign_keys", "OFF")
        .map_err(|e| format!("disable foreign keys for legacy run removal: {e}"))?;
    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| format!("begin legacy run removal: {e}"))?;
    let removed = (|| {
        for table in legacy_tables(conn)? {
            conn.execute_batch(&format!("DROP TABLE IF EXISTS \"{table}\""))
                .map_err(|e| format!("drop legacy run table {table}: {e}"))?;
        }
        conn.pragma_update(None, "user_version", RUN_STORE_SCHEMA_VERSION)
            .map_err(|e| format!("record workflow run schema version: {e}"))
    })();
    match removed {
        Ok(()) => conn
            .execute_batch("COMMIT")
            .map_err(|e| format!("commit legacy run removal: {e}")),
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

fn legacy_tables(conn: &Connection) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND (name LIKE 'workflow\\_%' ESCAPE '\\' OR name IN ('commands','command_observations','command_resources'))")
        .map_err(|e| format!("find legacy run tables: {e}"))?;
    stmt.query_map([], |row| row.get(0))
        .map_err(|e| format!("find legacy run tables: {e}"))?
        .collect::<Result<Vec<String>, _>>()
        .map_err(|e| format!("read legacy run tables: {e}"))
}

pub(super) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

#[cfg(test)]
mod legacy_removal_tests {
    use super::*;

    // Catches: legacy run tables surviving the upgrade, a removal without a
    // backup, or the neutral tables being dropped together with the legacy ones.
    #[test]
    fn legacy_run_tables_are_backed_up_then_dropped_and_neutral_tables_stay() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workflow_runs.sqlite3");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE workflow_runs (id TEXT PRIMARY KEY, snapshot_json TEXT);
                 CREATE TABLE workflow_events (run_id TEXT REFERENCES workflow_runs(id), sequence INTEGER);
                 INSERT INTO workflow_runs VALUES ('legacy', '{}');
                 INSERT INTO workflow_events VALUES ('legacy', 1);
                 CREATE TABLE neutral_workflow_runs (
                     id TEXT PRIMARY KEY, project TEXT NOT NULL, request_id TEXT NOT NULL,
                     request_hash TEXT NOT NULL, contract_version INTEGER NOT NULL,
                     UNIQUE(project,request_id));
                 INSERT INTO neutral_workflow_runs VALUES ('n1', '/p', 'r', 'h', 1);
                 PRAGMA user_version = 22;",
            )
            .unwrap();
        }
        let store = RunStore::open_at(&path).unwrap();
        let conn = store.connect().unwrap();
        assert_eq!(legacy_tables(&conn).unwrap(), Vec::<String>::new());
        let neutral: i64 = conn
            .query_row("SELECT count(*) FROM neutral_workflow_runs", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(neutral, 1);
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, RUN_STORE_SCHEMA_VERSION);
        let backup = Connection::open(dir.path().join(format!(
            "workflow_runs.sqlite3.{}",
            crate::sqlite_once::LEGACY_BACKUP_SUFFIX
        )))
        .unwrap();
        let retained: String = backup
            .query_row("SELECT id FROM workflow_runs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(retained, "legacy");
    }

    // Catches: a fresh store writing a pointless backup file.
    #[test]
    fn a_fresh_store_writes_no_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workflow_runs.sqlite3");
        RunStore::open_at(&path).unwrap();
        assert!(
            !dir.path()
                .join(format!(
                    "workflow_runs.sqlite3.{}",
                    crate::sqlite_once::LEGACY_BACKUP_SUFFIX
                ))
                .exists()
        );
    }
}
