//! Instance-scoped, owner-guarded SQLite reservations. Opening a reader never recovers runs.
use super::model::AutomationDefinition;
use super::run::*;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Arc;

mod admission;
mod history;
mod mutations;
#[cfg(test)]
mod tests;

const SCHEMA: crate::db::Schema = crate::db::Schema {
    name: "automation runs",
    migrations: &[&["CREATE TABLE automation_runs (
        id TEXT PRIMARY KEY, automation_id TEXT NOT NULL, occurrence_ms INTEGER,
        status TEXT NOT NULL, created_ms INTEGER NOT NULL, finished_ms INTEGER,
        snapshot_json TEXT NOT NULL);
        CREATE UNIQUE INDEX automation_occurrence ON automation_runs(automation_id,occurrence_ms) WHERE occurrence_ms IS NOT NULL;
        CREATE INDEX automation_history ON automation_runs(created_ms DESC,id DESC);
        CREATE INDEX automation_history_by_id ON automation_runs(automation_id,created_ms DESC,id DESC);
        CREATE INDEX automation_status ON automation_runs(status);
        CREATE INDEX automation_retention ON automation_runs(finished_ms);
        CREATE TABLE automation_cursors (
            automation_id TEXT PRIMARY KEY, occurrence_ms INTEGER NOT NULL);
        CREATE TABLE automation_deliveries (
            run_id TEXT NOT NULL REFERENCES automation_runs(id) ON DELETE CASCADE,
            transition TEXT NOT NULL, channel TEXT NOT NULL, document_json TEXT NOT NULL,
            PRIMARY KEY(run_id,transition,channel));"]],
};
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
        Self::acquire_at(&crate::db::path(crate::db::AUTOMATION_RUNS_DB), now_ms)
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
    /// Open for inspection only. Readers cannot reserve, mutate or recover runs; the
    /// database cleanup prunes finished runs through one.
    pub fn open() -> Result<Self, String> {
        Self::open_at(&crate::db::path(crate::db::AUTOMATION_RUNS_DB))
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
        crate::db::open_with(&self.path, &SCHEMA).map_err(error)
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
        started_ms: None,
        deadline_ms: Some(now_ms.saturating_add(
            i64::try_from(definition.max_duration_secs.saturating_mul(1000)).unwrap_or(i64::MAX),
        )),
        task_id: None,
        session_id: None,
        workspace: None,
        workspace_id: None,
        stdout: SavedOutput::default(),
        stderr: SavedOutput::default(),
        precheck: None,
        precheck_outcome: None,
        precheck_context: None,
        reason: None,
    };
    let inserted = conn.execute("INSERT INTO automation_runs(id,automation_id,occurrence_ms,status,created_ms,snapshot_json,finished_ms) VALUES(?1,?2,?3,?6,?4,?5,?7) ON CONFLICT(automation_id,occurrence_ms) WHERE occurrence_ms IS NOT NULL DO NOTHING", params![run.id, definition.id, occurrence, now_ms, encode(&run)?, status(initial_status)?, run.finished_ms]).map_err(error)?;
    Ok((inserted == 1).then_some(run))
}
