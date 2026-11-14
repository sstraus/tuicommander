use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Clone, Debug, Deserialize, Serialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CheckDefinition {
    pub id: String,
    pub argv: Vec<String>,
    pub timeout_secs: u32,
}

pub(super) fn validate_checks(checks: &[CheckDefinition]) -> Result<(), String> {
    if checks.len() > 16 {
        return Err("too many workflow checks".into());
    }
    let mut ids = std::collections::HashSet::new();
    for check in checks {
        if check.id.is_empty()
            || check.id.len() > 80
            || !check
                .id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
            || !ids.insert(&check.id)
        {
            return Err("invalid or duplicate workflow check id".into());
        }
        if check.argv.is_empty()
            || check.argv.len() > 32
            || !(1..=600).contains(&check.timeout_secs)
            || check
                .argv
                .iter()
                .any(|arg| arg.is_empty() || arg.len() > 4096 || arg.contains('\0'))
        {
            return Err("invalid workflow check command".into());
        }
        let executable = Path::new(&check.argv[0])
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if matches!(
            executable.as_str(),
            "sh" | "bash"
                | "zsh"
                | "fish"
                | "cmd"
                | "cmd.exe"
                | "powershell"
                | "powershell.exe"
                | "pwsh"
                | "pwsh.exe"
        ) {
            return Err("workflow checks must invoke an executable without a shell".into());
        }
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct WorkflowStore {
    db_path: PathBuf,
}

/// Layout version recorded in `PRAGMA user_version`. Version 1 drops the legacy
/// role/kind definitions, publications, host routing policy, kind bindings, PR
/// subjects, issue admissions and migration conversions after one backup.
const WORKFLOW_STORE_SCHEMA_VERSION: i64 = 1;

impl WorkflowStore {
    pub fn open() -> Result<Self, String> {
        Self::open_at(&crate::config::config_dir().join("workflows.sqlite3"))
    }

    pub(crate) fn open_at(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create workflow store directory: {e}"))?;
        }
        let store = Self {
            db_path: path.to_path_buf(),
        };
        store.connect()?;
        Ok(store)
    }

    pub(super) fn connect(&self) -> Result<Connection, String> {
        let conn =
            Connection::open(&self.db_path).map_err(|e| format!("open workflow store: {e}"))?;
        conn.busy_timeout(Duration::from_secs(5))
            .map_err(|e| format!("workflow busy timeout: {e}"))?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| format!("workflow WAL: {e}"))?;
        crate::sqlite_once::hold_open(&self.db_path);
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(|e| format!("read workflow schema version: {e}"))?;
        if version > WORKFLOW_STORE_SCHEMA_VERSION {
            return Err(format!(
                "workflow store schema version {version} is newer than this build supports ({WORKFLOW_STORE_SCHEMA_VERSION}); update TUICommander"
            ));
        }
        if version < WORKFLOW_STORE_SCHEMA_VERSION {
            drop_legacy_tables(&conn, &self.db_path)?;
        }
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| format!("workflow foreign keys: {e}"))?;
        conn.execute_batch(super::activity_preset::SCHEMA)
            .map_err(|e| format!("prepare activity presets: {e}"))?;
        conn.execute_batch(super::reusable_operation::SCHEMA)
            .map_err(|e| format!("create reusable operation schema: {e}"))?;
        conn.execute_batch(super::neutral_api::drafts::SCHEMA)
            .map_err(|e| format!("prepare neutral workflow drafts: {e}"))?;
        conn.execute_batch(super::neutral_publication::SCHEMA)
            .map_err(|e| format!("prepare neutral workflow publications: {e}"))?;
        Ok(conn)
    }

    /// Projects with a neutral draft or publication.
    pub(crate) fn projects_with_user_definitions(&self) -> Result<Vec<String>, String> {
        let conn = self.connect()?;
        let mut stmt = conn
            .prepare("SELECT project FROM neutral_workflow_drafts UNION SELECT project FROM neutral_workflow_publications")
            .map_err(|e| format!("prepare workflow projects: {e}"))?;
        stmt.query_map([], |row| row.get(0))
            .map_err(|e| format!("read workflow projects: {e}"))?
            .map(|row| row.map_err(|e| format!("read workflow project: {e}")))
            .collect()
    }
}

/// Every legacy table of this store: the `workflow_` prefix plus the migration
/// conversion journal. Neutral drafts, publications, presets, reusable
/// operations and trigger bindings stay.
fn legacy_tables(conn: &Connection) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND (name LIKE 'workflow\\_%' ESCAPE '\\' OR name='neutral_migration_conversions')")
        .map_err(|e| format!("find legacy workflow tables: {e}"))?;
    stmt.query_map([], |row| row.get(0))
        .map_err(|e| format!("find legacy workflow tables: {e}"))?
        .collect::<Result<Vec<String>, _>>()
        .map_err(|e| format!("read legacy workflow tables: {e}"))
}

/// Back up once, then drop the legacy tables and record the version in one
/// transaction, in any table order.
fn drop_legacy_tables(conn: &Connection, path: &Path) -> Result<(), String> {
    if legacy_tables(conn)?.is_empty() {
        return conn
            .pragma_update(None, "user_version", WORKFLOW_STORE_SCHEMA_VERSION)
            .map_err(|e| format!("record workflow schema version: {e}"));
    }
    crate::sqlite_once::backup_before_legacy_removal(conn, path)?;
    // A dropped parent table must not cascade into its legacy children:
    // foreign keys are off for this transaction and turned on by the caller.
    conn.pragma_update(None, "foreign_keys", "OFF")
        .map_err(|e| format!("disable foreign keys for legacy workflow removal: {e}"))?;
    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| format!("begin legacy workflow removal: {e}"))?;
    let removed = (|| {
        for table in legacy_tables(conn)? {
            conn.execute_batch(&format!("DROP TABLE IF EXISTS \"{table}\""))
                .map_err(|e| format!("drop legacy workflow table {table}: {e}"))?;
        }
        conn.pragma_update(None, "user_version", WORKFLOW_STORE_SCHEMA_VERSION)
            .map_err(|e| format!("record workflow schema version: {e}"))
    })();
    match removed {
        Ok(()) => conn
            .execute_batch("COMMIT")
            .map_err(|e| format!("commit legacy workflow removal: {e}")),
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

#[cfg(test)]
mod legacy_removal_tests {
    use super::*;

    // Catches: legacy definition tables surviving the upgrade, a removal without
    // a backup, or neutral drafts being dropped with them.
    #[test]
    fn legacy_definition_tables_are_backed_up_then_dropped_and_neutral_drafts_stay() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workflows.sqlite3");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE workflow_definitions (id TEXT PRIMARY KEY, project TEXT);
                 CREATE TABLE workflow_published (id TEXT REFERENCES workflow_definitions(id));
                 CREATE TABLE neutral_migration_conversions (id TEXT PRIMARY KEY);
                 INSERT INTO workflow_definitions VALUES ('legacy', '/p');
                 INSERT INTO workflow_published VALUES ('legacy');",
            )
            .unwrap();
            conn.execute_batch(super::super::neutral_api::drafts::SCHEMA)
                .unwrap();
        }
        let store = WorkflowStore::open_at(&path).unwrap();
        let conn = store.connect().unwrap();
        assert_eq!(legacy_tables(&conn).unwrap(), Vec::<String>::new());
        let drafts: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name='neutral_workflow_drafts'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(drafts, 1);
        let backup = Connection::open(dir.path().join(format!(
            "workflows.sqlite3.{}",
            crate::sqlite_once::LEGACY_BACKUP_SUFFIX
        )))
        .unwrap();
        let retained: String = backup
            .query_row("SELECT id FROM workflow_definitions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(retained, "legacy");
    }
}
