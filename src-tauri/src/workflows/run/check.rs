use crate::workflows::CheckDefinition;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, LazyLock, Mutex, Weak};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CheckReceipt {
    pub check_id: String,
    pub argv: Vec<String>,
    pub exit_code: i32,
    #[serde(default)]
    pub ref_name: String,
    pub commit: String,
    pub tree: String,
    pub duration_ms: u64,
}

#[cfg(test)]
thread_local! {
    // Pause the real Git boundary without replacing its output with a fixture.
    pub(super) static BEFORE_GIT: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
}

pub(super) fn git_output(path: &Path, args: &[&str]) -> Result<String, String> {
    #[cfg(test)]
    BEFORE_GIT.with(|slot| {
        let hook = slot.borrow_mut().take();
        if let Some(hook) = hook {
            hook();
        }
    });
    let output = Command::new("git")
        .args(args)
        .current_dir(path)
        .output()
        .map_err(|error| format!("run git: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout)
        .map(|value| value.trim().to_owned())
        .map_err(|error| format!("decode git output: {error}"))
}

pub(super) fn require_merge_tree_support(path: &Path) -> Result<(), String> {
    let version = git_output(path, &["--version"])?;
    let mut components = version
        .strip_prefix("git version ")
        .unwrap_or("")
        .split('.');
    let major = components
        .next()
        .and_then(|value| value.parse::<u32>().ok());
    let minor = components
        .next()
        .and_then(|value| value.parse::<u32>().ok());
    match (major, minor) {
        (Some(major), Some(minor)) => require_merge_tree_git_version(major, minor),
        _ => Err(format!(
            "git >= 2.38 required; cannot verify installed version: {version}"
        )),
    }
}

fn require_merge_tree_git_version(major: u32, minor: u32) -> Result<(), String> {
    if (major, minor) < (2, 38) {
        Err(format!(
            "git >= 2.38 required for workflow merge-tree verification (detected {major}.{minor})"
        ))
    } else {
        Ok(())
    }
}

pub(in crate::workflows) fn clean_artifact(path: &Path) -> Result<(String, String), String> {
    if !git_output(path, &["status", "--porcelain", "--untracked-files=all"])?.is_empty() {
        return Err("workflow check requires a clean worktree".into());
    }
    let commit = git_output(path, &["rev-parse", "HEAD"])?;
    let tree = git_output(path, &["rev-parse", "HEAD^{tree}"])?;
    if commit.len() != 40 && commit.len() != 64 {
        return Err("invalid Git commit digest".into());
    }
    if tree.len() != commit.len() {
        return Err("invalid Git tree digest".into());
    }
    Ok((commit, tree))
}

type CheckOwner = (PathBuf, String);

#[derive(Default)]
struct CheckRegistry {
    stopped: bool,
    cancelled: HashSet<CheckOwner>,
    active: HashMap<CheckOwner, Vec<Weak<Mutex<CheckProcess>>>>,
}

static CHECKS: LazyLock<Mutex<CheckRegistry>> = LazyLock::new(Mutex::default);

struct CheckProcess {
    child: std::process::Child,
    tree: Option<crate::agent::ScreenProbeTree>,
    cancelled: bool,
}

impl CheckProcess {
    fn stop(&mut self, cancelled: bool) {
        self.cancelled |= cancelled;
        if let Some(tree) = self.tree.take() {
            tree.terminate(self.child.id());
            // The owned tree was signalled first. Reap the direct child even
            // when it exited just before the termination request.
            if let Err(error) = self.child.wait() {
                tracing::warn!(source = "workflows", %error, "Failed to reap workflow check");
            }
        }
    }
}

impl Drop for CheckProcess {
    fn drop(&mut self) {
        self.stop(false);
    }
}

fn owner_matches(owner: &CheckOwner, cancelled: &CheckOwner) -> bool {
    owner.0 == cancelled.0
        && (owner.1 == cancelled.1 || owner.1.starts_with(&format!("{}:", cancelled.1)))
}

pub(super) fn cancel_checks(db_path: &Path, run_id: &str) {
    let owner = (db_path.to_owned(), run_id.to_owned());
    let mut registry = CHECKS.lock().unwrap_or_else(|error| error.into_inner());
    registry.cancelled.insert(owner.clone());
    let keys: Vec<_> = registry
        .active
        .keys()
        .filter(|key| owner_matches(key, &owner))
        .cloned()
        .collect();
    for key in keys {
        if let Some(checks) = registry.active.remove(&key) {
            for process in checks.into_iter().filter_map(|process| process.upgrade()) {
                process
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .stop(true);
            }
        }
    }
}

/// Synchronous teardown: desktop exit skips destructors and cannot wait for a
/// check worker's next polling tick.
pub(crate) fn shutdown_checks() {
    let mut registry = CHECKS.lock().unwrap_or_else(|error| error.into_inner());
    registry.stopped = true;
    for checks in registry.active.drain().map(|(_, checks)| checks) {
        for process in checks.into_iter().filter_map(|process| process.upgrade()) {
            process
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .stop(true);
        }
    }
}

pub(super) fn execute_run_check(
    check: &CheckDefinition,
    path: &Path,
    db_path: &Path,
    run_id: &str,
    allow_cancelled: bool,
) -> Result<CheckReceipt, String> {
    execute_check_owned(
        check,
        path,
        Some((db_path.to_owned(), run_id.to_owned())),
        allow_cancelled,
    )
}

fn execute_check_owned(
    check: &CheckDefinition,
    path: &Path,
    owner: Option<CheckOwner>,
    allow_cancelled: bool,
) -> Result<CheckReceipt, String> {
    let (commit, tree) = clean_artifact(path)?;
    let ref_name = git_output(path, &["symbolic-ref", "HEAD"])?;
    let executable = check
        .argv
        .first()
        .ok_or("workflow check has no executable")?;
    let start = Instant::now();
    let mut command = Command::new(executable);
    command
        .args(check.argv.iter().skip(1))
        .current_dir(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // Registration and spawn share the cancellation lock: cancellation also
    // fences workers which were reading Git when the run was cancelled.
    let mut registry = CHECKS.lock().unwrap_or_else(|error| error.into_inner());
    if registry.stopped
        || (!allow_cancelled
            && owner.as_ref().is_some_and(|owner| {
                registry
                    .cancelled
                    .iter()
                    .any(|cancelled| owner_matches(owner, cancelled))
            }))
    {
        return Err("workflow check cancelled".into());
    }
    let tree_owner = crate::agent::ScreenProbeTree::prepare(&mut command)
        .map_err(|error| format!("prepare workflow check tree: {error}"))?;
    let mut child = command
        .spawn()
        .map_err(|error| format!("start workflow check {}: {error}", check.id))?;
    if let Err(error) = tree_owner.assign(&child) {
        tree_owner.terminate(child.id());
        let _ = child.kill();
        let _ = child.wait();
        return Err(format!("assign workflow check tree: {error}"));
    }
    let process = Arc::new(Mutex::new(CheckProcess {
        child,
        tree: Some(tree_owner),
        cancelled: false,
    }));
    let key = owner.unwrap_or_else(|| (path.to_owned(), uuid::Uuid::new_v4().to_string()));
    registry
        .active
        .entry(key.clone())
        .or_default()
        .push(Arc::downgrade(&process));
    drop(registry);
    let timeout = Duration::from_secs(u64::from(check.timeout_secs));
    let result = (|| {
        loop {
            {
                let mut process = process.lock().unwrap_or_else(|error| error.into_inner());
                if process.cancelled {
                    return Err("workflow check cancelled".to_string());
                }
                if let Some(status) = process
                    .child
                    .try_wait()
                    .map_err(|error| format!("wait for workflow check: {error}"))?
                {
                    process.stop(false);
                    return Ok(status.code().unwrap_or(-1));
                }
                if start.elapsed() >= timeout {
                    process.stop(false);
                    return Ok(-1);
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    })();
    // Never take the registry lock while holding the process lock.
    drop(process);
    let mut registry = CHECKS.lock().unwrap_or_else(|error| error.into_inner());
    if let Some(active) = registry.active.get_mut(&key) {
        active.retain(|process| process.strong_count() != 0);
        if active.is_empty() {
            registry.active.remove(&key);
        }
    }
    drop(registry);
    let exit_code = result?;
    let duration_ms = start.elapsed().as_millis().min(u64::MAX as u128) as u64;
    if clean_artifact(path)? != (commit.clone(), tree.clone())
        || git_output(path, &["symbolic-ref", "HEAD"])? != ref_name
    {
        return Err("workflow check changed the branch, commit, or tree".into());
    }
    Ok(CheckReceipt {
        check_id: check.id.clone(),
        argv: check.argv.clone(),
        exit_code,
        ref_name,
        commit,
        tree,
        duration_ms,
    })
}

#[cfg(test)]
mod git_version_tests {
    use super::*;

    #[test]
    fn old_git_is_refused_with_the_required_version_not_conflict_review() {
        // catches: an unsupported merge-tree command being blamed on conflict resolution.
        let error = require_merge_tree_git_version(2, 37).unwrap_err();
        assert!(error.contains("git >= 2.38 required"), "{error}");
        assert!(!error.contains("human review"), "{error}");
        assert!(require_merge_tree_git_version(1, 99).is_err());
        assert!(require_merge_tree_git_version(2, 38).is_ok());
        assert!(require_merge_tree_git_version(2, 55).is_ok());
        assert!(require_merge_tree_git_version(3, 0).is_ok());
    }
}
