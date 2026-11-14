//! Run a store's schema preparation once per database file per process.
//!
//! The run, automation and progress stores open a connection per call, and each
//! open used to repeat `CREATE ... IF NOT EXISTS`, schema introspection and, for
//! automations, an IMMEDIATE transaction. The automation reconcile and the
//! attention snapshot open these stores on every session state change. A new
//! process prepares each file once, so a build that adds a table still creates it.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, PoisonError};

/// The path plus the file's identity, so a database deleted and created again
/// at the same path is prepared again.
type FileKey = (PathBuf, u64, Option<std::time::SystemTime>);

static PREPARED: LazyLock<Mutex<HashSet<FileKey>>> = LazyLock::new(Mutex::default);

fn file_key(path: &Path) -> Option<FileKey> {
    let metadata = std::fs::metadata(path).ok()?;
    #[cfg(unix)]
    let id = std::os::unix::fs::MetadataExt::ino(&metadata);
    #[cfg(not(unix))]
    let id = 0;
    let created = metadata.created().ok();
    // Without an inode or a creation time the file has no identity: prepare every time.
    if id == 0 && created.is_none() {
        return None;
    }
    Some((path.to_path_buf(), id, created))
}

/// Whether this process already prepared the schema of the file at `path`.
pub(crate) fn is_prepared(path: &Path) -> bool {
    file_key(path).is_some_and(|key| {
        PREPARED
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(&key)
    })
}

/// Record that the schema of the file at `path` is prepared.
pub(crate) fn mark_prepared(path: &Path) {
    if let Some(key) = file_key(path) {
        PREPARED
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key);
    }
}

static HOLDERS: LazyLock<
    Mutex<std::collections::HashMap<PathBuf, (FileKey, rusqlite::Connection)>>,
> = LazyLock::new(Mutex::default);

/// Keep one idle connection to a WAL database for the life of the process.
///
/// SQLite deletes the `-wal` file, after a checkpoint, when the last connection
/// closes. These stores open a connection per call, so every call created the
/// WAL again, wrote it and deleted it: measured 32 KB written per open/close
/// cycle, about 4 cycles a second from the attention snapshot alone. An idle
/// holder keeps the WAL alive; per-call opens then write nothing.
/// Call after the store set `journal_mode=WAL`. A database replaced at the same
/// path gets a new holder.
pub(crate) fn hold_open(path: &Path) {
    let Some(key) = file_key(path) else { return };
    let mut holders = HOLDERS.lock().unwrap_or_else(PoisonError::into_inner);
    if holders.get(path).is_some_and(|(held, _)| *held == key) {
        return;
    }
    if let Ok(conn) = rusqlite::Connection::open(path) {
        // An untouched connection has not attached to the WAL, so SQLite would still delete it
        // when the per-call connection closes. Entering WAL mode attaches this one.
        if conn.pragma_update(None, "journal_mode", "WAL").is_err() {
            return;
        }
        holders.insert(path.to_path_buf(), (key, conn));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Catches: every per-call open/close recreating and deleting the WAL file
    // (the 32 KB-per-cycle disk write churn): with a holder the `-wal` file's
    // inode is the same after several open/close cycles.
    #[cfg(unix)]
    #[test]
    fn a_held_database_keeps_its_wal_across_open_close_cycles() {
        use std::os::unix::fs::MetadataExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("held.sqlite3");
        let wal = dir.path().join("held.sqlite3-wal");
        let open = || {
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.pragma_update(None, "journal_mode", "WAL").unwrap();
            conn.execute_batch("CREATE TABLE IF NOT EXISTS t(x); INSERT INTO t VALUES (1);")
                .unwrap();
            hold_open(&path);
            conn
        };
        drop(open());
        let first = std::fs::metadata(&wal).unwrap().ino();
        for _ in 0..3 {
            drop(open());
        }
        assert_eq!(std::fs::metadata(&wal).unwrap().ino(), first);
    }

    // Catches: a database replaced at the same path being taken as prepared, so
    // its tables are never created.
    #[test]
    fn a_recreated_file_at_the_same_path_is_not_prepared() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("store.sqlite3");
        std::fs::write(&path, b"a").unwrap();
        assert!(!is_prepared(&path));
        mark_prepared(&path);
        assert!(is_prepared(&path));

        // Keep the old inode alive so the new file cannot reuse its number.
        let kept = dir.path().join("kept");
        std::fs::rename(&path, &kept).unwrap();
        std::fs::write(&path, b"b").unwrap();
        assert!(!is_prepared(&path));
    }
}

/// Suffix of the one-time copy a store writes before it drops the removed
/// legacy workflow engine's tables and rows.
pub(crate) const LEGACY_BACKUP_SUFFIX: &str = "legacy-workflows.bak";

/// Write a consistent copy of the database next to it, once, before the
/// legacy workflow data is removed. An existing copy is kept: it holds the
/// state before the first removal. Concurrent first connections each write a
/// private copy and publish it with a hard link, which only the first wins.
pub(crate) fn backup_before_legacy_removal(
    conn: &rusqlite::Connection,
    path: &Path,
) -> Result<PathBuf, String> {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".");
    name.push(LEGACY_BACKUP_SUFFIX);
    let backup = path.with_file_name(&name);
    if backup.exists() {
        return Ok(backup);
    }
    name.push(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let staging = path.with_file_name(name);
    let written = conn
        .execute("VACUUM INTO ?1", [staging.to_string_lossy()])
        .map_err(|e| format!("back up {} before legacy removal: {e}", path.display()))
        .and_then(|_| match std::fs::hard_link(&staging, &backup) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
            Err(error) => Err(format!("publish backup {}: {error}", backup.display())),
        });
    let _ = std::fs::remove_file(&staging);
    written.map(|()| backup)
}

#[cfg(test)]
mod legacy_backup_tests {
    use super::*;

    // Catches: two first connections racing on one backup path, so the loser
    // fails its open (seen as "table ... already exists" from VACUUM INTO) or
    // overwrites the copy taken before the first removal.
    #[test]
    fn concurrent_backups_publish_one_copy_and_never_fail() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let path = dir.path().join("store.sqlite3");
        rusqlite::Connection::open(&path)
            .unwrap()
            .execute_batch(
                "CREATE TABLE workflow_runs(id TEXT); INSERT INTO workflow_runs VALUES ('first');",
            )
            .unwrap();
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let path = path.clone();
                std::thread::spawn(move || {
                    let conn = rusqlite::Connection::open(&path).unwrap();
                    backup_before_legacy_removal(&conn, &path)
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap().unwrap();
        }
        let backup = dir
            .path()
            .join(format!("store.sqlite3.{LEGACY_BACKUP_SUFFIX}"));
        let kept: String = rusqlite::Connection::open(&backup)
            .unwrap()
            .query_row("SELECT id FROM workflow_runs", [], |row| row.get(0))
            .unwrap();
        assert_eq!(kept, "first");
        let leftovers = std::fs::read_dir(dir.path())
            .unwrap()
            .filter(|entry| {
                entry
                    .as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".tmp")
            })
            .count();
        assert_eq!(leftovers, 0);
    }
}
