use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug)]
pub struct RunStore {
    db_path: PathBuf,
}

const SCHEMA: crate::db::Schema = crate::db::Schema {
    name: "workflow runs",
    migrations: &[&[super::neutral::SCHEMA]],
};

impl RunStore {
    pub(in crate::workflows) fn database_path(&self) -> &Path {
        &self.db_path
    }

    pub fn open() -> Result<Self, String> {
        Self::open_at(&crate::db::path(crate::db::WORKFLOW_RUNS_DB))
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
        crate::db::open_with(&self.db_path, &SCHEMA)
            .map_err(|e| format!("open workflow run store: {e}"))
    }
}

pub(super) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}
