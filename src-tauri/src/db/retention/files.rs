//! Retention for the per-session and per-run files in the data directory.
//!
//! Every deletion goes through [`remove`], which refuses any path that is not
//! strictly inside the data directory.

use std::collections::HashSet;
use std::path::{Component, Path};
use std::time::UNIX_EPOCH;

pub(super) const AGENT_CAPABILITIES: &str = "agent-capabilities";
pub(super) const BG_WAKES: &str = "bg-wakes";
pub(super) const CAPTURES: &str = "captures";
pub(super) const MCP_BRIDGE: &str = "mcp-bridge";

/// Delete `path` (a file, a symlink or a whole directory) only when it lies
/// strictly inside `root`, also after resolving symlinks in its parents.
pub(in crate::db) fn remove(root: &Path, path: &Path) -> Result<(), String> {
    let lexical = path.starts_with(root)
        && path != root
        && !path
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir));
    let resolved = || -> Option<bool> {
        let root = root.canonicalize().ok()?;
        let parent = path.parent()?.canonicalize().ok()?;
        Some(parent.starts_with(&root))
    };
    if !lexical || resolved() != Some(true) {
        return Err(format!(
            "refusing to delete {} outside {}",
            path.display(),
            root.display()
        ));
    }
    let meta = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    let result = if meta.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    result.map_err(|e| format!("delete {}: {e}", path.display()))
}

/// Delete through [`remove`], logging a refusal or failure. Returns success.
fn remove_logged(root: &Path, path: &Path) -> bool {
    match remove(root, path) {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!(source = "db", %error, "Cleanup kept a file");
            false
        }
    }
}

pub(super) fn modified_ms(path: &Path) -> i64 {
    std::fs::symlink_metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(i64::MAX, |age| age.as_millis().min(i64::MAX as u128) as i64)
}

/// The newest change to `dir` or anything directly inside it.
pub(super) fn last_used_ms(dir: &Path) -> i64 {
    let entries = std::fs::read_dir(dir).into_iter().flatten().flatten();
    entries.fold(modified_ms(dir), |newest, entry| {
        newest.max(modified_ms(&entry.path()))
    })
}

/// Delete the entries of `root/sub` that `keep` does not protect and that were
/// last used before `cutoff_ms`. Returns how many were removed.
fn sweep(root: &Path, sub: &str, cutoff_ms: i64, keep: impl Fn(&str) -> bool) -> usize {
    let Ok(entries) = std::fs::read_dir(root.join(sub)) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|entry| {
            let name = entry.file_name();
            let path = entry.path();
            !keep(&name.to_string_lossy())
                && last_used_ms(&path) < cutoff_ms
                && remove_logged(root, &path)
        })
        .count()
}

/// `<session>.json` files of `sub` whose session is not live.
pub(super) fn session_files(
    root: &Path,
    sub: &str,
    cutoff_ms: i64,
    live: &HashSet<String>,
) -> usize {
    sweep(root, sub, cutoff_ms, |name| {
        name.strip_suffix(".json")
            .is_some_and(|session| live.contains(session))
    })
}

pub(super) fn captures(root: &Path, cutoff_ms: i64) -> usize {
    sweep(root, CAPTURES, cutoff_ms, |_| false)
}

/// Every bridge revision except `current`, once untouched since the cutoff.
pub(super) fn bridge_revisions(root: &Path, cutoff_ms: i64, current: Option<&str>) -> usize {
    sweep(root, MCP_BRIDGE, cutoff_ms, |name| Some(name) == current)
}
