//! Instance-scoped, owner-guarded SQLite reservations. Opening a reader never recovers runs.
use super::model::AutomationDefinition;
use super::run::*;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

mod admission;
mod history;
mod mutations;
#[cfg(test)]
mod tests;

/// Initial ledger layout; unknown versions must never be overwritten.
const SCHEMA_VERSION: i64 = 1;
const OPEN: &str = "('reserved','prechecking','running','needs_you')";

#[derive(Clone, Debug)]
pub struct RunStore {
    path: PathBuf,
    // A cloned writer keeps ownership alive until all dispatch work releases it.
    owner: Option<Arc<File>>,
}

pub struct RunOwner {
    store: RunStore,
}

impl RunOwner {
    /// Acquire exclusive runtime ownership before schema setup or restart recovery.
    pub fn acquire(now_ms: i64) -> Result<Self, String> {
        Self::acquire_at(
            &crate::config::config_dir().join("automation_runs.sqlite3"),
            now_ms,
        )
    }

    pub fn acquire_at(path: &Path, now_ms: i64) -> Result<Self, String> {
        create_parent(path)?;
        // Never unlink: all contenders must lock the same inode.
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.with_extension("owner.lock"))
            .map_err(error)?;
        lock.try_lock()
            .map_err(|e| format!("automation executor unavailable: {e}"))?;
        let store = RunStore {
            path: path.into(),
            owner: Some(Arc::new(lock)),
        };
        store.connect()?;
        store.interrupt_open(now_ms)?;
        Ok(Self { store })
    }

    pub fn store(&self) -> &RunStore {
        &self.store
    }
}

impl RunStore {
    /// Open for inspection only. Readers cannot reserve, mutate, prune or recover runs.
    pub fn open() -> Result<Self, String> {
        Self::open_at(&crate::config::config_dir().join("automation_runs.sqlite3"))
    }

    pub fn open_at(path: &Path) -> Result<Self, String> {
        create_parent(path)?;
        let store = Self {
            path: path.into(),
            owner: None,
        };
        store.connect()?;
        Ok(store)
    }

    fn require_owner(&self) -> Result<(), String> {
        self.owner
            .as_ref()
            .map(|_| ())
            .ok_or_else(|| "automation executor ownership required".into())
    }

    fn connect(&self) -> Result<Connection, String> {
        let mut conn = Connection::open(&self.path).map_err(error)?;
        conn.busy_timeout(Duration::from_secs(5)).map_err(error)?;
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .map_err(error)?;
        if version != 0 && version != SCHEMA_VERSION {
            return Err(format!(
                "unsupported automation run schema version {version}"
            ));
        }
        // Refuse unversioned populated files, rather than guessing their contract.
        if version == 0 {
            let count: i64 = conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'", [], |r| r.get(0)).map_err(error)?;
            if count != 0 {
                return Err("unsupported unversioned automation run schema".into());
            }
        }
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(error)?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(error)?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS automation_runs (
            id TEXT PRIMARY KEY, automation_id TEXT NOT NULL, occurrence_ms INTEGER,
            status TEXT NOT NULL, created_ms INTEGER NOT NULL, finished_ms INTEGER,
            snapshot_json TEXT NOT NULL);
            CREATE UNIQUE INDEX IF NOT EXISTS automation_occurrence ON automation_runs(automation_id,occurrence_ms) WHERE occurrence_ms IS NOT NULL;
            CREATE INDEX IF NOT EXISTS automation_history ON automation_runs(created_ms DESC,id DESC);
            CREATE INDEX IF NOT EXISTS automation_history_by_id ON automation_runs(automation_id,created_ms DESC,id DESC);
            CREATE INDEX IF NOT EXISTS automation_status ON automation_runs(status);
            CREATE INDEX IF NOT EXISTS automation_retention ON automation_runs(finished_ms);
            CREATE TABLE IF NOT EXISTS automation_cursors (
                automation_id TEXT PRIMARY KEY, occurrence_ms INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS automation_deliveries (
                run_id TEXT NOT NULL REFERENCES automation_runs(id) ON DELETE CASCADE,
                transition TEXT NOT NULL, channel TEXT NOT NULL, document_json TEXT NOT NULL,
                PRIMARY KEY(run_id,transition,channel));").map_err(error)?;
        tx.pragma_update(None, "user_version", SCHEMA_VERSION)
            .map_err(error)?;
        tx.commit().map_err(error)?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(error)?;
        Ok(conn)
    }

    /// Commit a run before the caller performs any external effect. Duplicate occurrences return None.
    pub fn reserve(
        &self,
        definition: &AutomationDefinition,
        trigger: RunTrigger,
        now_ms: i64,
    ) -> Result<Option<AutomationRun>, String> {
        self.require_owner()?;
        definition.validate()?;
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(error)?;
        let run = reserve_in(&tx, definition, trigger, RunStatus::Reserved, now_ms)?;
        tx.commit().map_err(error)?;
        Ok(run)
    }

    pub fn get(&self, id: &str) -> Result<AutomationRun, String> {
        read(&self.connect()?, id)
    }
}

fn create_parent(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(error)?;
    }
    Ok(())
}
fn error(e: impl std::fmt::Display) -> String {
    format!("automation run store: {e}")
}
fn encode<T: serde::Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(error)
}
fn decode<T: serde::de::DeserializeOwned>(value: String) -> Result<T, String> {
    serde_json::from_str(&value).map_err(error)
}
fn status(value: RunStatus) -> Result<String, String> {
    serde_json::to_value(value)
        .map_err(error)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "invalid run status".into())
}
fn read(conn: &Connection, id: &str) -> Result<AutomationRun, String> {
    let json = conn
        .query_row(
            "SELECT snapshot_json FROM automation_runs WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()
        .map_err(error)?
        .ok_or("automation run not found")?;
    decode(json)
}
fn write(conn: &Connection, run: &AutomationRun) -> Result<(), String> {
    conn.execute(
        "UPDATE automation_runs SET status=?2,finished_ms=?3,snapshot_json=?4 WHERE id=?1",
        params![run.id, status(run.status)?, run.finished_ms, encode(run)?],
    )
    .map_err(error)?;
    Ok(())
}

fn reserve_in(
    conn: &Connection,
    definition: &AutomationDefinition,
    trigger: RunTrigger,
    initial_status: RunStatus,
    now_ms: i64,
) -> Result<Option<AutomationRun>, String> {
    let occurrence = match trigger {
        RunTrigger::Scheduled { occurrence_ms } => Some(occurrence_ms),
        RunTrigger::Manual => None,
    };
    let run = AutomationRun {
        id: uuid::Uuid::now_v7().to_string(),
        definition: definition.clone(),
        trigger,
        status: initial_status,
        created_ms: now_ms,
        updated_ms: now_ms,
        finished_ms: (!initial_status.is_open()).then_some(now_ms),
        task_id: None,
        session_id: None,
        workspace: None,
        workspace_id: None,
        stdout: SavedOutput::default(),
        stderr: SavedOutput::default(),
        precheck: None,
        precheck_outcome: None,
        reason: None,
    };
    let inserted = conn.execute("INSERT INTO automation_runs(id,automation_id,occurrence_ms,status,created_ms,snapshot_json,finished_ms) VALUES(?1,?2,?3,?6,?4,?5,?7) ON CONFLICT(automation_id,occurrence_ms) WHERE occurrence_ms IS NOT NULL DO NOTHING", params![run.id, definition.id, occurrence, now_ms, encode(&run)?, status(initial_status)?, run.finished_ms]).map_err(error)?;
    Ok((inserted == 1).then_some(run))
}
