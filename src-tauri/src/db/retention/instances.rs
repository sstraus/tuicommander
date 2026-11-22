//! Remove the config directories that stopped instances leave under
//! `instances/` (`make agent-test`, `make test`, isolated checks).
//!
//! A running instance holds an exclusive lock on `instance.lock` in its data
//! directory for its whole life, so a directory whose lock can be taken has no
//! process. The daily instance is the data directory itself, never a child of
//! `instances/`.

use super::files;
use std::fs::{File, OpenOptions};
use std::path::Path;
use std::sync::OnceLock;

const LOCK_FILE: &str = "instance.lock";

static INSTANCE_LOCK: OnceLock<File> = OnceLock::new();

fn open_lock(dir: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join(LOCK_FILE))
}

/// Mark this process as the live owner of `dir` until it exits.
pub(super) fn hold_instance_lock(dir: &Path) {
    let held = open_lock(dir).and_then(|file| file.try_lock().map(|()| file).map_err(Into::into));
    match held {
        Ok(file) => {
            let _ = INSTANCE_LOCK.set(file);
        }
        Err(error) => {
            tracing::warn!(source = "db", %error, "Cannot hold the instance liveness lock")
        }
    }
}

/// Whether a live process holds the lock of `dir`. A directory without the
/// lock file predates it and has no owner either.
fn is_live(dir: &Path) -> bool {
    if !dir.join(LOCK_FILE).exists() {
        return false;
    }
    match open_lock(dir) {
        Ok(file) => file.try_lock().is_err(),
        Err(_) => true,
    }
}

/// Delete `dir/instances/*` directories unused since `cutoff_ms` and owned by
/// no process. Returns how many were removed.
pub(super) fn remove_stale_instances(dir: &Path, cutoff_ms: i64) -> usize {
    let Ok(entries) = std::fs::read_dir(dir.join("instances")) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let stale = path.is_dir() && files::last_used_ms(&path) < cutoff_ms && !is_live(&path);
        if stale {
            match files::remove(dir, &path) {
                Ok(()) => removed += 1,
                Err(error) => tracing::warn!(source = "db", %error, "Cannot remove stale instance"),
            }
        }
    }
    removed
}
