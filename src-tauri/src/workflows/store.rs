use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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

impl WorkflowStore {
    pub fn open() -> Result<Self, String> {
        Self::open_at(&crate::db::path(crate::db::TUIC_DB))
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
        crate::db::open_with(&self.db_path, &crate::db::TUIC_SCHEMA)
            .map_err(|e| format!("open workflow store: {e}"))
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
