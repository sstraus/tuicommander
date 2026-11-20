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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requirements_digest: Option<String>,
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

/// The artifact a native step may act on: HEAD plus the exact index tree. The
/// worktree must equal the index and hold no untracked file, so staged work is an
/// artifact and foreign dirt is not. With a clean worktree this is `clean_artifact`.
pub(in crate::workflows) fn staged_artifact(path: &Path) -> Result<(String, String), String> {
    if !git_output(path, &["diff", "--name-only"])?.is_empty()
        || !git_output(path, &["ls-files", "--others", "--exclude-standard"])?.is_empty()
    {
        return Err(
            "native artifact requires a clean worktree apart from staged work: unstaged or untracked changes"
                .into(),
        );
    }
    let commit = git_output(path, &["rev-parse", "HEAD"])?;
    let tree = git_output(path, &["write-tree"])?;
    if (commit.len() != 40 && commit.len() != 64) || tree.len() != commit.len() {
        return Err("invalid Git artifact digest".into());
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
    stop_error: Option<String>,
}

impl CheckProcess {
    fn stop(&mut self, cancelled: bool) {
        self.cancelled |= cancelled;
        if let Some(tree) = self.tree.take() {
            #[cfg(unix)]
            if let Err(error) = stop_owned_descendants(self.child.id()) {
                self.stop_error = Some(error);
            }
            tree.terminate(self.child.id());
            // The owned tree was signalled first. Reap the direct child even
            // when it exited just before the termination request.
            if let Err(error) = self.child.wait() {
                tracing::warn!(source = "workflows", %error, "Failed to reap workflow check");
            }
        }
    }
}

/// A wrapper such as build-slot --gate owns child groups outside its own PGID.
/// Freeze the owned tree before walking it, so stopping its root cannot orphan a
/// still-forking child group. Only descendants of our recorded child are signalled.
#[cfg(unix)]
fn stop_owned_descendants(root: u32) -> Result<(), String> {
    let mut owned = HashSet::from([root]);
    unsafe {
        libc::kill(-(root as libc::pid_t), libc::SIGSTOP);
    }
    loop {
        let map = crate::pty::process_parent_map()
            .ok_or("cannot inspect owned process tree; stop uncertain")?;
        let mut pending = vec![root];
        let mut added = false;
        let mut visited = HashSet::new();
        while let Some(pid) = pending.pop() {
            if !visited.insert(pid) {
                continue;
            }
            if let Some(children) = map.get(&pid) {
                for &child in children {
                    pending.push(child);
                    if owned.insert(child) {
                        unsafe {
                            libc::kill(child as libc::pid_t, libc::SIGSTOP);
                        }
                        added = true;
                    }
                }
            }
        }
        if !added {
            break;
        }
    }
    for pid in owned.into_iter().filter(|pid| *pid != root) {
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGKILL);
        }
    }
    Ok(())
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

/// Durable record of an owned process group. A SIGKILL of TUIC leaves the group
/// running (it has its own group), and this file is the only trace of it.
#[cfg(unix)]
fn group_record(owner: &CheckOwner) -> PathBuf {
    let name: String = owner
        .1
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    owner.0.with_extension("processes").join(name)
}

#[cfg(unix)]
fn write_group_record(owner: &CheckOwner, pid: u32) {
    let path = group_record(owner);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(error) = std::fs::write(&path, pid.to_string()) {
        tracing::warn!(source = "workflows", %error, "Cannot record the owned process group");
    }
}

/// Before an original attempt runs again: stop its group while this process
/// still owns it, then look for the recorded group. A group that survived a TUIC
/// kill is never signalled (its id may belong to another program now): the error
/// is evidence for the operator, never a claim that the group is ours.
pub(in crate::workflows) fn original_process_gone(
    db_path: &Path,
    run_id: &str,
) -> Result<(), String> {
    let owner = (db_path.to_owned(), run_id.to_owned());
    {
        let mut registry = CHECKS.lock().unwrap_or_else(|error| error.into_inner());
        let live: Vec<_> = registry
            .active
            .get(&owner)
            .into_iter()
            .flatten()
            .filter_map(|process| process.upgrade())
            .collect();
        registry.active.remove(&owner);
        drop(registry);
        for process in live {
            process
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .stop(true);
        }
    }
    #[cfg(unix)]
    {
        let record = group_record(&owner);
        let Ok(text) = std::fs::read_to_string(&record) else {
            return Ok(());
        };
        let pgid: libc::pid_t = text
            .trim()
            .parse()
            .map_err(|_| "recorded process group is unreadable; not run again".to_owned())?;
        // SAFETY: signal 0 only probes for existence.
        let alive = unsafe { libc::kill(-pgid, 0) } == 0
            || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH);
        if alive {
            return Err(format!(
                "the original script process group {pgid} may still be running (the id may also belong to another program now)"
            ));
        }
        let _ = std::fs::remove_file(record);
    }
    Ok(())
}

/// The effect settled or its run ended: its record names nothing to wait for.
pub(in crate::workflows) fn forget_process_group(db_path: &Path, run_id: &str) {
    #[cfg(unix)]
    let _ = std::fs::remove_file(group_record(&(db_path.to_owned(), run_id.to_owned())));
    #[cfg(not(unix))]
    let _ = (db_path, run_id);
}

pub(in crate::workflows) fn cancel_checks(db_path: &Path, run_id: &str) {
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
    let mut command = Command::new(executable);
    command
        .args(check.argv.iter().skip(1))
        .current_dir(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let output = execute_process(
        command,
        Duration::from_secs(u64::from(check.timeout_secs)),
        owner,
        allow_cancelled,
    )?;
    if output.cancelled {
        return Err("workflow check cancelled".into());
    }
    let (exit_code, duration_ms) = (output.exit_code, output.duration_ms);
    if clean_artifact(path)? != (commit.clone(), tree.clone())
        || git_output(path, &["symbolic-ref", "HEAD"])? != ref_name
    {
        return Err("workflow check changed the branch, commit, or tree".into());
    }
    Ok(CheckReceipt {
        requirements_digest: None,
        check_id: check.id.clone(),
        argv: check.argv.clone(),
        exit_code,
        ref_name,
        commit,
        tree,
        duration_ms,
    })
}

pub(in crate::workflows) struct ProcessOutput {
    pub exit_code: i32,
    pub duration_ms: u64,
    pub cancelled: bool,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

fn drain_output(
    reader: Option<impl std::io::Read + Send + 'static>,
) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let Some(mut reader) = reader else {
            return vec![];
        };
        let mut captured = Vec::new();
        let mut bytes = [0; 8192];
        loop {
            match reader.read(&mut bytes) {
                Ok(0) | Err(_) => break,
                Ok(count) => {
                    let remaining = (1024 * 1024_usize).saturating_sub(captured.len());
                    captured.extend_from_slice(&bytes[..count.min(remaining)]);
                }
            }
        }
        captured
    })
}

#[derive(Debug)]
pub(in crate::workflows) enum ProcessError {
    NotStarted(String),
    Uncertain(String),
}

impl From<ProcessError> for String {
    fn from(error: ProcessError) -> Self {
        match error {
            ProcessError::NotStarted(reason) | ProcessError::Uncertain(reason) => reason,
        }
    }
}

/// One cancellation registry for legacy checks and positioned command nodes.
pub(in crate::workflows) fn execute_process(
    command: Command,
    timeout: Duration,
    owner: Option<(PathBuf, String)>,
    allow_cancelled: bool,
) -> Result<ProcessOutput, ProcessError> {
    execute_process_recording(command, timeout, owner, allow_cancelled, false)
}

/// `record_group` leaves a durable trace of the process group, for an attempt
/// that an operator may approve to run again.
pub(in crate::workflows) fn execute_process_recording(
    mut command: Command,
    timeout: Duration,
    owner: Option<(PathBuf, String)>,
    allow_cancelled: bool,
    record_group: bool,
) -> Result<ProcessOutput, ProcessError> {
    let start = Instant::now();
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
        return Err(ProcessError::NotStarted("workflow check cancelled".into()));
    }
    let tree_owner = crate::agent::ScreenProbeTree::prepare(&mut command).map_err(|error| {
        ProcessError::NotStarted(format!("prepare workflow check tree: {error}"))
    })?;
    let mut child = command
        .spawn()
        .map_err(|error| ProcessError::NotStarted(format!("start workflow command: {error}")))?;
    if let Err(error) = tree_owner.assign(&child) {
        tree_owner.terminate(child.id());
        let _ = child.kill();
        let _ = child.wait();
        return Err(ProcessError::Uncertain(format!(
            "assign workflow check tree: {error}"
        )));
    }
    #[cfg(unix)]
    if record_group && let Some(owner) = &owner {
        write_group_record(owner, child.id());
    }
    // A Windows job object ends the group with its owner: nothing to record.
    #[cfg(not(unix))]
    let _ = record_group;
    let stdout = drain_output(child.stdout.take());
    let stderr = drain_output(child.stderr.take());
    let process = Arc::new(Mutex::new(CheckProcess {
        child,
        tree: Some(tree_owner),
        cancelled: false,
        stop_error: None,
    }));
    let key = owner.unwrap_or_else(|| (PathBuf::new(), uuid::Uuid::new_v4().to_string()));
    registry
        .active
        .entry(key.clone())
        .or_default()
        .push(Arc::downgrade(&process));
    drop(registry);
    let result: Result<i32, String> = (|| {
        loop {
            {
                let mut process = process.lock().unwrap_or_else(|error| error.into_inner());
                if process.cancelled {
                    return Ok(-2);
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
    let stop_error = process
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .stop_error
        .clone();
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
    if let Some(error) = stop_error {
        return Err(ProcessError::Uncertain(error));
    }
    // The tree was stopped and reaped: the record no longer names a live group.
    #[cfg(unix)]
    if result.is_ok() && record_group {
        let _ = std::fs::remove_file(group_record(&key));
    }
    let exit_code = result.map_err(ProcessError::Uncertain)?;
    let duration_ms = start.elapsed().as_millis().min(u64::MAX as u128) as u64;
    Ok(ProcessOutput {
        exit_code,
        duration_ms,
        cancelled: exit_code == -2,
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    })
}
