//! Delete the database files of the layout before `tuic.sqlite3`.
//!
//! Nothing is migrated (Boss, 2026-11-21): the old stores, their WAL files and
//! the one-time `.legacy-workflows.bak` copies are removed at startup, with
//! the directories nothing uses any more.

use super::{AUTOMATION_RUNS_DB, PROGRESS_DB, TUIC_DB, WORKFLOW_RUNS_DB};
use std::path::Path;

/// Suffix of the copies the removed legacy workflow engine cleanup wrote.
pub(crate) const LEGACY_BACKUP_SUFFIX: &str = "legacy-workflows.bak";

/// Files whose data moved into `tuic.sqlite3` or that changed name.
const RETIRED: [&str; 6] = [
    "stories.sqlite3",
    "workflows.sqlite3",
    "managed-launches.sqlite",
    "mcp_usage.sqlite3",
    "mail-subscriptions.db",
    "tunnel_audit.db",
];

/// Logs that kept their name but lost their migrations: an old file has an
/// old layout. Removed only while `tuic.sqlite3` does not exist yet.
const RESET_WITH_LAYOUT: [&str; 3] = [PROGRESS_DB, AUTOMATION_RUNS_DB, WORKFLOW_RUNS_DB];

/// Directories no code writes or reads any more: `ai-sessions/` lost its
/// writer in a961a079f5.
const RETIRED_DIRS: [&str; 1] = ["ai-sessions"];

const COMPANIONS: [&str; 4] = ["", "-wal", "-shm", "-journal"];

/// Remove retired files from `dir`, then create `tuic.sqlite3`, whose presence
/// marks the directory as converted. Returns the names removed.
pub(crate) fn remove_retired_files(dir: &Path) -> Vec<String> {
    let converted = dir.join(TUIC_DB).exists();
    let reset: &[&str] = if converted { &[] } else { &RESET_WITH_LAYOUT };
    let mut removed = Vec::new();
    for name in RETIRED.iter().chain(reset) {
        for suffix in COMPANIONS {
            let file = format!("{name}{suffix}");
            let path = dir.join(&file);
            if path.exists() && super::retention::files::remove(dir, &path).is_ok() {
                removed.push(file);
            }
        }
    }
    for name in RETIRED_DIRS {
        let path = dir.join(name);
        if path.exists() && super::retention::files::remove(dir, &path).is_ok() {
            removed.push(format!("{name}/"));
        }
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.ends_with(LEGACY_BACKUP_SUFFIX)
                && super::retention::files::remove(dir, &entry.path()).is_ok()
            {
                removed.push(name);
            }
        }
    }
    if !removed.is_empty() {
        tracing::info!(
            source = "db",
            "Removed retired database files: {}",
            removed.join(", ")
        );
    }
    if let Err(error) = super::open_with(&dir.join(TUIC_DB), &super::TUIC_SCHEMA) {
        tracing::error!(source = "db", %error, "Cannot create tuic.sqlite3");
    }
    removed
}
