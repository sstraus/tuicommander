//! The one way TUIC opens a SQLite database it owns.
//!
//! Durable, low-write data lives in [`TUIC_DB`]; each high-volume append log
//! keeps its own `<name>.sqlite3`. Every file goes through [`open`], which sets
//! WAL, the busy timeout and foreign keys, and keeps the WAL alive between
//! per-call connections. Inventory and retention rules:
//! `docs/backend/persistence.md`.

use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub(crate) mod legacy;
pub(crate) mod retention;
mod tuic;

pub(crate) use tuic::TUIC_SCHEMA;

/// Durable data: stories, workflow definitions, managed launches, MCP usage.
pub(crate) const TUIC_DB: &str = "tuic.sqlite3";
/// High-volume append logs, one file each.
pub(crate) const PROGRESS_DB: &str = "progress.sqlite3";
pub(crate) const AUTOMATION_RUNS_DB: &str = "automation_runs.sqlite3";
pub(crate) const WORKFLOW_RUNS_DB: &str = "workflow_runs.sqlite3";
pub(crate) const MAIL_DB: &str = "mail.sqlite3";
pub(crate) const TUNNEL_AUDIT_DB: &str = "tunnel_audit.sqlite3";

/// How long a writer waits for another connection's lock.
pub(crate) const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// A database layout: migration `i` takes `user_version` from `i` to `i + 1`.
/// Each migration is a list of SQL batches run in one transaction.
pub(crate) struct Schema {
    pub name: &'static str,
    pub migrations: &'static [&'static [&'static str]],
}

/// The path of `file` in the application's data directory.
pub(crate) fn path(file: &str) -> PathBuf {
    crate::config::config_dir().join(file)
}

/// Open `path` with WAL, [`BUSY_TIMEOUT`] and foreign keys, creating its directory.
pub(crate) fn open(path: &Path) -> Result<Connection, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create database directory {}: {e}", parent.display()))?;
    }
    let new = !path.exists();
    let conn = Connection::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    conn.busy_timeout(BUSY_TIMEOUT)
        .map_err(|e| format!("busy timeout {}: {e}", path.display()))?;
    if new {
        // Only an empty file accepts it; the cleanup then returns freed pages.
        conn.pragma_update(None, "auto_vacuum", "INCREMENTAL")
            .map_err(|e| format!("enable incremental vacuum on {}: {e}", path.display()))?;
    }
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(|e| format!("enable WAL on {}: {e}", path.display()))?;
    crate::sqlite_once::hold_open(path);
    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(|e| format!("enable foreign keys on {}: {e}", path.display()))?;
    Ok(conn)
}

/// Open an existing database for reading only; never creates or migrates it.
pub(crate) fn open_read_only(path: &Path) -> Result<Connection, String> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| format!("open {} read-only: {e}", path.display()))?;
    conn.busy_timeout(BUSY_TIMEOUT)
        .map_err(|e| format!("busy timeout {}: {e}", path.display()))?;
    Ok(conn)
}

/// [`open`], then bring the file to `schema`'s latest version once per process.
pub(crate) fn open_with(path: &Path, schema: &Schema) -> Result<Connection, String> {
    let mut conn = open(path)?;
    if !crate::sqlite_once::is_prepared(path) {
        migrate(&mut conn, schema)?;
        crate::sqlite_once::mark_prepared(path);
    }
    Ok(conn)
}

fn user_version(conn: &Connection) -> Result<i64, String> {
    conn.pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|e| format!("read schema version: {e}"))
}

fn migrate(conn: &mut Connection, schema: &Schema) -> Result<(), String> {
    let latest = schema.migrations.len() as i64;
    let newer = |version: i64| {
        format!(
            "{} schema version {version} is not one this build supports (0..={latest}); update TUICommander",
            schema.name
        )
    };
    let version = user_version(conn)?;
    if !(0..=latest).contains(&version) {
        return Err(newer(version));
    }
    if version == latest {
        return Ok(());
    }
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| format!("begin {} migration: {e}", schema.name))?;
    // Another process may have migrated while this one waited for the lock.
    let version = user_version(&tx)?;
    if !(0..=latest).contains(&version) {
        return Err(newer(version));
    }
    for (index, batches) in schema.migrations.iter().enumerate().skip(version as usize) {
        for sql in *batches {
            tx.execute_batch(sql)
                .map_err(|e| format!("{} migration {}: {e}", schema.name, index + 1))?;
        }
    }
    tx.pragma_update(None, "user_version", latest)
        .map_err(|e| format!("record {} schema version: {e}", schema.name))?;
    tx.commit()
        .map_err(|e| format!("commit {} migration: {e}", schema.name))
}

#[cfg(test)]
mod tests;
