use crate::git_cli::{FETCH_TIMEOUT, git_cmd};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

/// Represents a git worktree
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorktreeInfo {
    pub name: String,
    pub path: PathBuf,
    pub branch: Option<String>,
    pub base_repo: PathBuf,
}

type WarmEntry = (u64, serde_json::Value, Arc<std::sync::Mutex<()>>);
static WARM_STATES: LazyLock<dashmap::DashMap<String, WarmEntry>> =
    LazyLock::new(dashmap::DashMap::new);
static NEXT_WARM_TOKEN: AtomicU64 = AtomicU64::new(1);

fn warm_key(path: &Path) -> String {
    let parent = path.parent().unwrap_or(path);
    std::fs::canonicalize(parent)
        .unwrap_or_else(|_| parent.to_path_buf())
        .join(path.file_name().unwrap_or_default())
        .to_string_lossy()
        .into_owned()
}

pub fn begin_warm(path: &Path) -> u64 {
    let token = NEXT_WARM_TOKEN.fetch_add(1, Ordering::Relaxed);
    WARM_STATES.insert(
        warm_key(path),
        (
            token,
            serde_json::json!({"status": "pending"}),
            Arc::new(std::sync::Mutex::new(())),
        ),
    );
    token
}

pub fn finish_warm(path: &Path, token: u64, status: serde_json::Value) {
    if let Some(mut current) = WARM_STATES.get_mut(&warm_key(path))
        && current.0 == token
    {
        current.1 = status;
    }
}

pub fn clear_warm(path: &Path) {
    WARM_STATES.remove(&warm_key(path));
}

pub fn warm_lock(path: &Path) -> Option<Arc<std::sync::Mutex<()>>> {
    WARM_STATES
        .get(&warm_key(path))
        .map(|state| Arc::clone(&state.2))
}

pub fn warm_token_is_current(path: &Path, token: u64) -> bool {
    WARM_STATES
        .get(&warm_key(path))
        .is_some_and(|state| state.0 == token)
}

#[cfg(test)]
pub fn spawn_background_warm(
    source: PathBuf,
    destination: PathBuf,
    token: u64,
    warm: impl FnOnce(&Path, &Path) -> crate::cow::WarmingReport + Send + 'static,
) -> tokio::task::JoinHandle<()> {
    tokio::task::spawn_blocking(move || {
        finish_background_warm_blocking(source, destination, token, warm)
    })
}

/// Complete a warm task after the app adapter schedules it on a blocking pool.
pub fn finish_background_warm_blocking(
    source: PathBuf,
    destination: PathBuf,
    token: u64,
    warm: impl FnOnce(&Path, &Path) -> crate::cow::WarmingReport + Send + 'static,
) {
    let Some(lock) = warm_lock(&destination) else {
        return;
    };
    let _guard = lock
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if !warm_token_is_current(&destination, token) {
        return;
    }
    let report = warm(&source, &destination);
    let status = if report.warnings.is_empty() {
        serde_json::json!({"status": "done"})
    } else {
        serde_json::json!({"status": "failed", "reason": report.warnings.join("; ")})
    };
    finish_warm(&destination, token, status);
    for warning in report.warnings {
        tracing::warn!(source = "worktree", worktree = %destination.display(), "background warm failed: {warning}");
    }
}

pub fn warm_status(path: &Path) -> serde_json::Value {
    WARM_STATES
        .get(&warm_key(path))
        .map(|state| state.1.clone())
        .unwrap_or_else(|| serde_json::json!({"status": "done"}))
}

/// Classification of a failed `git worktree add` based on its stderr, used to
/// decide how `create_worktree_internal` should recover.
///
/// Git emits several distinct "already exists" failures from `worktree add` and
/// they require different handling — a single `contains("already exists")` guard
/// conflates them and can swallow a hard failure as success.
#[derive(Debug, PartialEq, Eq)]
enum WorktreeAddFailure {
    /// The destination PATH (or registered worktree) already exists / is already
    /// checked out / already used by another worktree. A real worktree directory
    /// may genuinely exist here → caller may treat it as idempotent, but MUST
    /// verify the directory is present before returning Ok.
    PathExists,
    /// A branch with the requested name already exists, so `-b <branch>` failed.
    /// No worktree directory was created → caller must recover (retry without
    /// `-b` to check the existing branch out into a new worktree).
    BranchExists,
    /// Any other failure → propagate as an error.
    Other,
}

/// Classify a `git worktree add` failure from its stderr. Pure function so the
/// branching logic can be unit-tested without invoking real git.
fn classify_worktree_add_failure(stderr: &str) -> WorktreeAddFailure {
    // Branch collision: git says e.g. "fatal: a branch named 'X' already exists".
    // Check this FIRST — it also contains "already exists", so the broader
    // path-exists check below would otherwise misclassify it.
    if stderr.contains("a branch named") && stderr.contains("already exists") {
        return WorktreeAddFailure::BranchExists;
    }
    // Path / worktree already present: "'<path>' already exists",
    // "is already checked out", "already used by worktree".
    if stderr.contains("already exists")
        || stderr.contains("already checked out")
        || stderr.contains("already used by worktree")
    {
        return WorktreeAddFailure::PathExists;
    }
    WorktreeAddFailure::Other
}

// `find_worktree_path_for_branch` was deleted with #726-5ac7. It returned the
// FIRST porcelain block carrying a branch, which is the whole bug: with two
// workspaces on one branch every caller silently got the wrong directory. Its
// replacement is `resolve_workspace`, keyed by workspace id. Do not reintroduce
// a branch-keyed path lookup — resolve an id and read `.branch` off the record.

#[derive(Clone, Serialize, Deserialize)]
pub struct WorktreeConfig {
    pub task_name: String,
    pub base_repo: String,
    pub branch: Option<String>,
    pub create_branch: bool,
}

#[derive(Clone, Serialize)]
pub struct WorktreeResult {
    pub session_id: String,
    pub worktree_path: String,
    pub branch: Option<String>,
}

/// Sanitize task name for use as directory name
pub fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .to_lowercase()
}

/// Create a git worktree for a task.
///
/// `base_ref` optionally specifies the starting commit/branch for new worktrees
/// (e.g., "main" or "origin/develop"). When `None`, git uses HEAD.
pub fn create_worktree_internal(
    worktrees_dir: &Path,
    config: &WorktreeConfig,
    base_ref: Option<&str>,
) -> Result<WorktreeInfo, String> {
    let worktree_name = sanitize_name(&config.task_name);
    let worktree_path = worktrees_dir.join(&worktree_name);

    // Check if worktree already exists (idempotent return — stale cleanup is caller's responsibility).
    // Detached HEAD (actual_branch == None) is NOT treated as stale: it's a transient state during
    // rebase/bisect/`git checkout <sha>` on a worktree we created. Forcing cleanup there would
    // destroy an agent's in-progress work.
    if worktree_path.exists() {
        if !worktree_path.join(".git").exists() {
            return Err(format!(
                "{STALE_DIR_PREFIX} directory '{}' is not a Git checkout",
                worktree_path.display()
            ));
        }
        let actual_branch = crate::git::read_branch_from_head(&worktree_path);
        if let Some(ref expected) = config.branch
            && let Some(ref actual) = actual_branch
            && actual.as_str() != expected.as_str()
        {
            return Err(format!(
                "{STALE_DIR_PREFIX} directory '{}' is checked out on branch '{}', not '{}'",
                worktree_path.display(),
                actual,
                expected,
            ));
        }
        // Fall back to config.branch when actual_branch is None (detached HEAD):
        // the worktree was created for `config.branch`, the detach is transient, and
        // the JS layer's `BranchState` keys on `result.branch: string` — returning
        // `null` would corrupt the store. The branch field reflects logical
        // ownership, not the live HEAD state.
        return Ok(WorktreeInfo {
            name: worktree_name,
            path: worktree_path,
            branch: actual_branch.or_else(|| config.branch.clone()),
            base_repo: PathBuf::from(&config.base_repo),
        });
    }

    // Ensure worktrees directory exists
    std::fs::create_dir_all(worktrees_dir)
        .map_err(|e| format!("Failed to create worktrees directory: {e}"))?;

    // Build git worktree add command
    let base_repo_path = PathBuf::from(&config.base_repo);
    let wt_path_str = worktree_path.to_string_lossy().to_string();
    // --quiet suppresses git's own checkout progress lines ("Updating files: X% (N/M)")
    // that would otherwise appear in the controlling terminal. Hooks still run normally.
    let mut args: Vec<String> = vec!["worktree".into(), "add".into(), "--quiet".into()];

    if config.create_branch
        && let Some(ref branch) = config.branch
    {
        args.push("-b".into());
        args.push(branch.clone());
    }

    // End-of-options guard: the branch/start-point below is attacker-influenced
    // (e.g. a PR head_ref), so `--` forces git to treat it as a ref, not an option.
    args.push("--".into());
    args.push(wt_path_str);

    if let Some(ref branch) = config.branch
        && !config.create_branch
    {
        args.push(branch.clone());
    }

    // Append base_ref as start-point when creating a new branch
    if config.create_branch
        && let Some(start_point) = base_ref
    {
        // Auto-fetch if the base ref is a remote tracking branch
        fetch_if_remote(&config.base_repo, start_point)?;
        args.push(start_point.to_string());
    }

    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    match git_cmd(&base_repo_path).args(&args_str).run() {
        Ok(_) => {}
        Err(crate::git_cli::GitError::NonZeroExit { ref stderr, .. }) => {
            match classify_worktree_add_failure(stderr) {
                WorktreeAddFailure::BranchExists => {
                    // `-b <branch>` failed because the branch already exists, but
                    // NO worktree was created. The user intent ("give me a worktree
                    // for this branch") is still satisfiable: retry without `-b` to
                    // check the existing branch out into a fresh worktree. Only
                    // reachable when create_branch && branch is Some (that's the
                    // only way `-b` was passed), so `branch` is guaranteed present.
                    let branch = config
                        .branch
                        .as_ref()
                        .expect("BranchExists implies -b was passed, so branch is Some");
                    let retry_args = [
                        "worktree",
                        "add",
                        "--quiet",
                        "--",
                        &worktree_path.to_string_lossy(),
                        branch.as_str(),
                    ];
                    if let Err(e) = git_cmd(&base_repo_path).args(retry_args).run() {
                        return Err(format!(
                            "Git worktree failed: branch '{branch}' already exists and could not be checked out into a new worktree: {e}"
                        ));
                    }
                    // Retry creates the dir at config.branch — fall through to the
                    // success return below.
                }
                WorktreeAddFailure::PathExists => {
                    // A path/worktree already exists. Defensive fail-loud: only treat
                    // this as idempotent if the directory is genuinely present on disk.
                    if !worktree_path.exists() {
                        return Err(format!(
                            "Git worktree failed: git reported '{}' already exists but no worktree directory is present: {stderr}",
                            worktree_path.display(),
                        ));
                    }
                    let actual_branch = crate::git::read_branch_from_head(&worktree_path);
                    // Mirror the earlier idempotent-path STALE_DIR check: only treat a
                    // KNOWN-mismatched branch as stale. Detached HEAD (None) is preserved
                    // as transient state. Use the same STALE_DIR_PREFIX so the recovery
                    // path in `create_worktree` (background cleanup + retry) handles it.
                    if let Some(ref expected) = config.branch
                        && let Some(ref actual) = actual_branch
                        && actual.as_str() != expected.as_str()
                    {
                        return Err(format!(
                            "{STALE_DIR_PREFIX} directory '{}' already exists and is checked out on branch '{}', not '{}'",
                            worktree_path.display(),
                            actual,
                            expected,
                        ));
                    }
                    return Ok(WorktreeInfo {
                        name: worktree_name,
                        path: worktree_path,
                        branch: actual_branch.or_else(|| config.branch.clone()),
                        base_repo: PathBuf::from(&config.base_repo),
                    });
                }
                WorktreeAddFailure::Other => {
                    return Err(crate::git_locks::describe_stale_lock(&base_repo_path)
                        .unwrap_or_else(|| {
                            format!("Git worktree failed: git exited with: {stderr}")
                        }));
                }
            }
        }
        Err(e) => {
            return Err(crate::git_locks::describe_stale_lock(&base_repo_path)
                .unwrap_or_else(|| format!("Git worktree failed: {e}")));
        }
    }

    // Persist the base ref in git config for "Update from base" support
    if let Some(ref branch) = config.branch
        && let Some(start_point) = base_ref
    {
        let _ = set_branch_base(&config.base_repo, branch, start_point);
    }

    Ok(WorktreeInfo {
        name: worktree_name,
        path: worktree_path,
        branch: config.branch.clone(),
        base_repo: PathBuf::from(&config.base_repo),
    })
}

/// Error prefix returned when a worktree is git-locked and `force` is false.
/// The JS layer checks for this prefix to show a confirmation dialog before retrying.
pub const LOCKED_WORKTREE_PREFIX: &str = "worktree_locked:";

/// Error prefix returned when trying to `git worktree remove` the main working tree.
/// The JS layer treats this as a non-fatal condition and does NOT remove the branch
/// from the store (to avoid resurrection on the next refresh).
pub const MAIN_WORKTREE_PREFIX: &str = "worktree_is_main:";

/// Error prefix returned when a worktree directory exists but is checked out on a
/// different branch than requested. The Tauri command's stale-recovery path matches
/// this prefix to trigger background cleanup + recreate. Centralised here so callers
/// don't drift on the literal string.
pub const STALE_DIR_PREFIX: &str = "STALE_DIR:";

/// Remove only an unregistered stale directory without a Git checkout.
/// Registered worktrees and orphan Git repositories are preserved, even when clean.
pub fn cleanup_stale_worktree_dir(base_repo: &str, stale_path: &Path) -> Result<(), String> {
    let listed = git_cmd(Path::new(base_repo))
        .args(["worktree", "list", "--porcelain"])
        .run()
        .map_err(|error| format!("Cannot verify stale directory registration: {error}"))?;
    if parse_worktree_entries(&listed.stdout)
        .iter()
        .any(|entry| warm_key(Path::new(&entry.path)) == warm_key(stale_path))
        || stale_path.join(".git").exists()
    {
        return Err(format!(
            "Cannot clean stale directory '{}': registered worktree or Git checkout",
            stale_path.display()
        ));
    }

    if stale_path.exists()
        && let Err(e) = std::fs::remove_dir_all(stale_path)
    {
        return Err(format!(
            "stale dir cleanup failed for '{}': {e}",
            stale_path.display()
        ));
    }

    if stale_path.exists() {
        return Err(format!(
            "stale dir '{}' still present after cleanup",
            stale_path.display()
        ));
    }
    Ok(())
}

/// Synchronous create-with-STALE_DIR-recovery for non-Tauri callers (MCP HTTP routes,
/// `create_session_with_worktree`, etc.). Tries `create_worktree_internal`; on a
/// STALE_DIR error, runs `cleanup_stale_worktree_dir` and retries once. The retry's
/// result is returned as-is — a second STALE_DIR (e.g. TOCTOU with another caller)
/// surfaces to the caller rather than looping.
pub fn create_worktree_with_stale_recovery(
    worktrees_dir: &Path,
    config: &WorktreeConfig,
    base_ref: Option<&str>,
) -> Result<WorktreeInfo, String> {
    match create_worktree_internal(worktrees_dir, config, base_ref) {
        Ok(wt) => Ok(wt),
        Err(ref e) if e.starts_with(STALE_DIR_PREFIX) => {
            let stale_path = worktrees_dir.join(sanitize_name(&config.task_name));
            tracing::warn!(
                source = "worktree",
                stale = %stale_path.display(),
                "create_worktree_with_stale_recovery: STALE_DIR detected, cleaning up + retrying"
            );
            cleanup_stale_worktree_dir(&config.base_repo, &stale_path)?;
            create_worktree_internal(worktrees_dir, config, base_ref)
        }
        Err(e) => Err(e),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceCommitStatus {
    Unmerged,
    /// Not merged, but every commit exists on the branch's own remote-tracking
    /// ref, so deleting the local branch loses nothing.
    PushedUnmerged,
    /// HEAD is the default branch's tip: this workspace has no commits of its
    /// own, so it was never merged. Kept apart from `Merged` because both
    /// satisfy `merge-base --is-ancestor` and only one of them describes a
    /// history that happened.
    InSync,
    Merged,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceRemovalSafety {
    Safe,
    RequiresForce,
    Unknown,
}

/// One backend-authored answer for every UI that explains or removes a
/// workspace. Optional fields mean inspection failed; unknown is never zero.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkspaceLifecycleStatus {
    /// Count of staged, unstaged and untracked changes. Ignored files are not
    /// counted; removal also deletes those files, including warmed caches.
    pub dirty_files: Option<usize>,
    /// The registration exists but its checkout directory has disappeared.
    pub missing_checkout: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dirty_fingerprint: Option<String>,
    pub submodule_unpushed_commits: Vec<SubmoduleUnpushedCommits>,
    pub commit_status: WorkspaceCommitStatus,
    #[serde(skip)]
    merge_proof: Option<&'static str>,
    pub removal_safety: WorkspaceRemovalSafety,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SubmoduleUnpushedCommits {
    pub path: String,
    pub count: usize,
}

fn rev_at(path: &Path, spec: &str) -> Result<String, String> {
    git_cmd(path)
        .args(["rev-parse", spec])
        .run()
        .map(|out| out.stdout.trim().to_string())
        .map_err(|e| format!("could not resolve {spec}: {e}"))
}

fn dirty_files_at(path: &Path) -> Result<usize, String> {
    git_cmd(path)
        .args([
            "status",
            "--porcelain",
            "--untracked-files=all",
            "--ignore-submodules=none",
        ])
        .run()
        .map(|out| out.stdout.lines().filter(|l| !l.trim().is_empty()).count())
        .map_err(|e| format!("could not check the workspace for uncommitted changes: {e}"))
}

pub fn dirty_fingerprint_at(
    path: &Path,
) -> Result<(String, Vec<SubmoduleUnpushedCommits>), String> {
    let status = git_cmd(path)
        .args([
            "status",
            "--porcelain",
            "--untracked-files=all",
            "--ignore-submodules=none",
        ])
        .run()
        .map_err(|e| format!("could not fingerprint worktree status: {e}"))?;
    let submodules = git_cmd(path)
        .args(["submodule", "status", "--recursive"])
        .run()
        .map_err(|e| format!("could not fingerprint submodules: {e}"))?;
    let mut digest = Sha256::new();
    digest.update(status.stdout.as_bytes());
    digest.update([0]);
    digest.update(rev_at(path, "HEAD")?.as_bytes());
    digest.update([0]);
    digest.update(submodules.stdout.as_bytes());
    let mut unpushed = Vec::new();
    for line in submodules.stdout.lines() {
        if !matches!(line.chars().next(), Some(' ' | '+')) {
            continue;
        }
        let (_, description) = line[1..]
            .split_once(' ')
            .ok_or_else(|| format!("Cannot parse submodule status: {line}"))?;
        let submodule_path = description
            .rsplit_once(" (")
            .map_or(description, |(path, _)| path);
        let module = path.join(submodule_path);
        let refs = git_cmd(&module)
            .args(["for-each-ref", "--format=%(refname) %(objectname)"])
            .run()
            .map_err(|e| format!("Cannot inspect submodule {submodule_path}: {e}"))?;
        digest.update(refs.stdout.as_bytes());
        let count = git_cmd(&module)
            .args(["rev-list", "--count", "HEAD", "--all", "--not", "--remotes"])
            .run()
            .map_err(|e| format!("Cannot count submodule commits in {submodule_path}: {e}"))?
            .stdout
            .trim()
            .parse()
            .map_err(|e| format!("Cannot parse submodule commit count in {submodule_path}: {e}"))?;
        if count > 0 {
            unpushed.push(SubmoduleUnpushedCommits {
                path: submodule_path.into(),
                count,
            });
        }
    }
    Ok((hex::encode(digest.finalize()), unpushed))
}

fn initialized_main_submodule(base_repo: &Path, destination: &Path) -> bool {
    if !destination.join(".git").exists() {
        return false;
    }
    let (Ok(module_root), Ok(superproject_root), Ok(module_gitdir), Ok(base_gitdir)) = (
        rev_at(destination, "--show-toplevel"),
        rev_at(destination, "--show-superproject-working-tree"),
        rev_at(destination, "--absolute-git-dir"),
        rev_at(base_repo, "--absolute-git-dir"),
    ) else {
        return false;
    };
    let (
        Ok(module_root),
        Ok(destination),
        Ok(superproject_root),
        Ok(base_repo),
        Ok(module_gitdir),
        Ok(base_gitdir),
    ) = (
        Path::new(&module_root).canonicalize(),
        destination.canonicalize(),
        Path::new(&superproject_root).canonicalize(),
        base_repo.canonicalize(),
        Path::new(&module_gitdir).canonicalize(),
        Path::new(&base_gitdir).canonicalize(),
    )
    else {
        return false;
    };
    // Git can report the expected worktree and superproject even when this
    // submodule's .git file points back to the superproject object store.
    module_root == destination && superproject_root == base_repo && module_gitdir != base_gitdir
}

/// One path component of a preserved ref. A SHA-256 digest keeps it at 64 bytes
/// however long the checkout path, submodule path or ref name is (a git ref
/// component is capped at 255 bytes by the filesystem). Nothing decodes these
/// components, so refs written under the old hex-of-input scheme stay reachable
/// through the `refs/tuic/preserved` prefix unchanged.
fn preserved_ref_component(input: &[u8]) -> String {
    hex::encode(Sha256::digest(input))
}

fn preserve_submodule_refs(
    base_repo: &Path,
    worktree: &Path,
    submodule_path: &str,
) -> Result<(), String> {
    let source = worktree.join(submodule_path);
    let destination = base_repo.join(submodule_path);
    let source_gitdir = rev_at(&source, "--absolute-git-dir")?;
    // Without an initialized module repository in the main checkout there is
    // nowhere durable to move the objects. Refuse rather than delete their only copy.
    if !initialized_main_submodule(base_repo, &destination) {
        return Err(format!(
            "Cannot remove worktree: main checkout has no repository for submodule {submodule_path}"
        ));
    }
    let refs = git_cmd(&source)
        .args(["for-each-ref", "--format=%(refname) %(objectname)"])
        .run()
        .map_err(|e| format!("Cannot inspect submodule refs in {submodule_path}: {e}"))?;
    let mut tips = vec![("HEAD".to_string(), rev_at(&source, "HEAD")?)];
    for line in refs.stdout.lines() {
        let (name, oid) = line
            .split_once(' ')
            .ok_or_else(|| format!("Cannot parse submodule ref in {submodule_path}: {line}"))?;
        tips.push((name.to_string(), oid.to_string()));
    }
    let reflog = git_cmd(&source)
        .args(["reflog", "show", "--all", "--format=%H"])
        .run()
        .map_err(|e| format!("Cannot inspect submodule reflog in {submodule_path}: {e}"))?;
    for oid in reflog.stdout.lines() {
        tips.push((format!("reflog/{oid}"), oid.to_string()));
    }
    let namespace = format!(
        "refs/tuic/preserved/{}/{}/{}/",
        preserved_ref_component(worktree.to_string_lossy().as_bytes()),
        preserved_ref_component(submodule_path.as_bytes()),
        uuid::Uuid::new_v4().simple()
    );
    let mut args = vec![
        "fetch".to_string(),
        "--no-tags".to_string(),
        "--no-recurse-submodules".to_string(),
        "--no-write-fetch-head".to_string(),
        source_gitdir,
    ];
    for (name, oid) in tips {
        args.push(format!(
            "{oid}:{}{}",
            namespace,
            preserved_ref_component(name.as_bytes())
        ));
    }
    git_cmd(&destination)
        .args(&args)
        .timeout(Duration::from_secs(60))
        .run()
        .map_err(|e| format!("Cannot preserve submodule refs in {submodule_path}: {e}"))?;
    Ok(())
}

fn submodule_admin_dir_at(
    worktree: &Path,
    worktree_gitdir: &Path,
    relative_path: &Path,
) -> Result<PathBuf, String> {
    let mut owner = worktree.to_path_buf();
    let mut admin = worktree_gitdir.to_path_buf();
    let mut remaining = relative_path;
    loop {
        let declarations = git_cmd(&owner)
            .args([
                "config",
                "--file",
                ".gitmodules",
                "--get-regexp",
                r"^submodule\..*\.path$",
            ])
            .run()
            .map_err(|e| format!("Cannot resolve submodule {relative_path:?} name: {e}"))?;
        let matched = declarations
            .stdout
            .lines()
            .filter_map(|line| {
                let (key, declared_path) = line.split_once(char::is_whitespace)?;
                let name = key.strip_prefix("submodule.")?.strip_suffix(".path")?;
                let declared_path = Path::new(declared_path.trim());
                if name.is_empty()
                    || !Path::new(name)
                        .components()
                        .all(|component| matches!(component, Component::Normal(_)))
                    || !declared_path
                        .components()
                        .all(|component| matches!(component, Component::Normal(_)))
                {
                    return None;
                }
                let rest = remaining.strip_prefix(declared_path).ok()?;
                Some((name, declared_path, rest))
            })
            .max_by_key(|(_, declared_path, _)| declared_path.components().count())
            .ok_or_else(|| format!("Cannot resolve submodule {relative_path:?} name"))?;
        admin = admin.join("modules").join(matched.0);
        if matched.2.as_os_str().is_empty() {
            return Ok(admin);
        }
        owner = owner.join(matched.1);
        remaining = matched.2;
    }
}

fn verify_submodules_at(path: &Path, base_repo: &Path, force: bool) -> Result<(), String> {
    let output = git_cmd(path)
        .args(["submodule", "status", "--recursive"])
        .run()
        .map_err(|e| format!("Cannot verify submodules before removal: {e}"))?;
    let admin_gitdir = rev_at(path, "--absolute-git-dir")?;
    for line in output.stdout.lines() {
        let state = line.chars().next().ok_or("Empty submodule status line")?;
        let (_, description) = line[1..]
            .split_once(' ')
            .ok_or_else(|| format!("Cannot parse submodule status before removal: {line}"))?;
        let submodule_path = description
            .rsplit_once(" (")
            .map_or(description, |(path, _)| path);
        if state == '-' {
            if path.join(submodule_path).join(".git").exists()
                || submodule_admin_dir_at(
                    path,
                    Path::new(&admin_gitdir),
                    Path::new(submodule_path),
                )?
                .exists()
            {
                return Err(format!(
                    "Cannot remove worktree: uninitialized submodule {submodule_path} still has Git state"
                ));
            }
            continue;
        }
        if state == 'U' || (state == '+' && !force) || (state != ' ' && state != '+') {
            return Err(format!(
                "Cannot remove worktree: submodule {submodule_path} is conflicted or has a different commit"
            ));
        }
        preserve_submodule_refs(base_repo, path, submodule_path)?;
    }
    Ok(())
}

pub fn inspect_workspace_lifecycle(
    base_repo: &Path,
    workspace_id: &str,
) -> WorkspaceLifecycleStatus {
    inspect_workspace_lifecycle_with_pr(base_repo, workspace_id, |_, _, _| false)
}

fn is_ancestor(repo: &Path, ancestor: &str, descendant: &str) -> Result<bool, String> {
    let result = git_cmd(repo)
        .args(["merge-base", "--is-ancestor", ancestor, descendant])
        .run_raw()
        .map_err(|error| format!("could not compare commits: {error}"))?;
    match result.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        code => Err(format!(
            "could not compare commits (exit {code:?}): {}",
            String::from_utf8_lossy(&result.stderr).trim()
        )),
    }
}

/// GitHub records the PR head even when squash merging did not preserve its
/// commit IDs on the default branch. A local tip is safe only if that recorded
/// head contains it; matching a branch name alone is never enough.
pub fn merged_pr_proof_from_pages(
    repo: &Path,
    branch: &str,
    tip: &str,
    pages: &serde_json::Value,
) -> bool {
    let Some(pages) = pages.as_array() else {
        return false;
    };
    if pages
        .iter()
        .any(|page| page.get("errors").is_some_and(|errors| !errors.is_null()))
    {
        return false;
    }
    for page in pages {
        let Some(nodes) = page
            .pointer("/data/repository/pullRequests/nodes")
            .and_then(serde_json::Value::as_array)
        else {
            continue;
        };
        for pr in nodes {
            if pr["state"].as_str() != Some("MERGED") || pr["headRefName"].as_str() != Some(branch)
            {
                continue;
            }
            let Some(sha) = pr["headRefOid"].as_str() else {
                continue;
            };
            if !matches!(sha.len(), 40 | 64) || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                continue;
            }
            let Some(number) = pr["number"].as_u64().filter(|number| *number > 0) else {
                continue;
            };
            let commit = format!("{sha}^{{commit}}");
            if git_cmd(repo)
                .args(["cat-file", "-e", &commit])
                .run_silent()
                .is_none()
            {
                let refspec = format!("refs/pull/{number}/head");
                if git_cmd(repo)
                    .timeout(Duration::from_secs(30))
                    .args(["fetch", "--no-tags", "origin", &refspec])
                    .run_silent()
                    .is_none()
                    || git_cmd(repo)
                        .args(["cat-file", "-e", &commit])
                        .run_silent()
                        .is_none()
                {
                    continue;
                }
            }
            if is_ancestor(repo, tip, sha) == Ok(true) {
                return true;
            }
        }
    }
    false
}

/// Refs a branch is compared against: the default branch's upstream, which the
/// local default branch routinely trails, and the local default branch itself
/// (it may hold merges not yet pushed). Missing refs are skipped.
fn integration_bases(repo: &Path, default_branch: &str) -> Vec<String> {
    [
        format!("refs/remotes/origin/{default_branch}"),
        format!("refs/heads/{default_branch}"),
    ]
    .into_iter()
    .filter(|base| rev_at(repo, &format!("{base}^{{commit}}")).is_ok())
    .collect()
}

/// Human-readable form of `integration_bases`, for messages.
fn describe_bases(bases: &[String]) -> String {
    bases
        .iter()
        .map(|base| {
            base.strip_prefix("refs/heads/")
                .or_else(|| base.strip_prefix("refs/remotes/"))
                .unwrap_or(base)
        })
        .collect::<Vec<_>>()
        .join(" and ")
}

fn is_ancestor_of_any(repo: &Path, tip: &str, bases: &[String]) -> Result<bool, String> {
    for base in bases {
        if is_ancestor(repo, tip, base)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Every commit up to `tip` exists on the branch's own remote-tracking ref.
fn pushed_to_own_remote(repo: &Path, branch: &str, tip: &str) -> Result<bool, String> {
    let remote = format!("refs/remotes/origin/{branch}");
    if rev_at(repo, &format!("{remote}^{{commit}}")).is_err() {
        return Ok(false);
    }
    is_ancestor(repo, tip, &remote)
}

fn classify_branch_merge(
    repo: &Path,
    branch: &str,
    tip: &str,
    default_branch: &str,
    pr_proves_tip: impl Fn(&Path, &str, &str) -> bool,
) -> Result<(WorkspaceCommitStatus, Option<&'static str>), String> {
    let bases = integration_bases(repo, default_branch);
    if bases.is_empty() {
        return Err(format!("default branch '{default_branch}' was not found"));
    }
    let merged = is_ancestor_of_any(repo, tip, &bases)?;
    // Ancestry alone cannot distinguish own commits from a branch that merely
    // followed the default branch. The branch reflog records both its source
    // and how its ref moved after creation.
    let reflog = git_cmd(repo)
        .args([
            "reflog",
            "show",
            "--format=%H%x09%gs",
            &format!("refs/heads/{branch}"),
        ])
        .run()
        .map_err(|error| format!("could not inspect branch history: {error}"))?
        .stdout;
    let mut entries = reflog.lines().collect::<Vec<_>>();
    let creation = entries
        .pop()
        .ok_or("branch creation is absent from reflog")?;
    let (_, creation_message) = creation
        .split_once('\t')
        .ok_or("branch creation has no reflog message")?;
    let source = creation_message.strip_prefix("branch: Created from ");
    let from_default = source.is_some_and(|source| {
        source == "HEAD"
            || source == default_branch
            || source == format!("refs/heads/{default_branch}")
    });
    let pull_upstream_is_default = entries
        .iter()
        .any(|entry| entry.ends_with("\tpull: Fast-forward"))
        && git_cmd(repo)
            .args(["config", "--get", &format!("branch.{branch}.merge")])
            .run()
            .ok()
            .is_some_and(|output| output.stdout.trim() == format!("refs/heads/{default_branch}"));
    let follows_default = entries.iter().all(|entry| {
        let message = entry.split_once('\t').map(|(_, message)| message);
        message.is_some_and(|message| {
            message == format!("merge {default_branch}: Fast-forward")
                || message == format!("reset: moving to {default_branch}")
                || (message.starts_with("pull ")
                    && message.ends_with(&format!(" {default_branch}: Fast-forward")))
                || (message == "pull: Fast-forward" && pull_upstream_is_default)
                || (message.starts_with("rebase (finish): refs/heads/")
                    && message.contains(&format!("refs/heads/{branch} onto ")))
        })
    });
    if merged && from_default && follows_default {
        Ok((WorkspaceCommitStatus::InSync, None))
    } else if merged {
        Ok((WorkspaceCommitStatus::Merged, Some("ancestry")))
    } else if is_ancestor(repo, tip, &rev_at(repo, "HEAD")?)? {
        Ok((WorkspaceCommitStatus::Merged, Some("integration_ancestry")))
    } else if let Some(proof) = integrated_in_defaults(repo, &bases, tip)? {
        let proof = if proof == "content_superset" && pr_proves_tip(repo, branch, tip) {
            "github_pr"
        } else {
            proof
        };
        Ok((WorkspaceCommitStatus::Merged, Some(proof)))
    } else if pr_proves_tip(repo, branch, tip) {
        Ok((WorkspaceCommitStatus::Merged, Some("github_pr")))
    } else if pushed_to_own_remote(repo, branch, tip)? {
        Ok((WorkspaceCommitStatus::PushedUnmerged, None))
    } else {
        Ok((WorkspaceCommitStatus::Unmerged, None))
    }
}

/// Subjects alone never prove integration. Squash messages corroborate a
/// structural no-op; context-free twins and content require a recovery ref.
fn integrated_in_defaults(
    repo: &Path,
    bases: &[String],
    tip: &str,
) -> Result<Option<&'static str>, String> {
    for base in bases {
        let target = rev_at(repo, base)?;
        if patches_integrated_in(repo, &target, tip)? {
            return Ok(Some("patch_equivalence"));
        }
        let merged = git_cmd(repo)
            .args(["merge-tree", "--write-tree", &target, tip])
            .run();
        let target_tree = rev_at(repo, &format!("{target}^{{tree}}"))?;
        if merged.is_ok_and(|output| output.stdout.trim() == target_tree) {
            let subjects = git_cmd(repo)
                .args(["log", "--format=%s", &format!("{target}..{tip}")])
                .run()
                .map_err(|error| format!("could not inspect branch subjects: {error}"))?
                .stdout;
            let messages = git_cmd(repo)
                .args(["log", "--format=%B%x00", &format!("{tip}..{target}")])
                .run()
                .map_err(|error| format!("could not inspect squash messages: {error}"))?
                .stdout;
            let squash = !subjects.trim().is_empty()
                && messages.split('\0').any(|message| {
                    subjects.lines().all(|subject| {
                        message.lines().any(|line| {
                            line.trim().strip_prefix("* ").unwrap_or(line.trim()) == subject
                        })
                    })
                });
            return Ok(Some(if squash {
                "squash_message"
            } else {
                "noop_merge"
            }));
        }
    }
    for base in bases {
        let target = rev_at(repo, base)?;
        if same_subject_twins(repo, &target, tip)? || content_superset(repo, &target, tip)? {
            return Ok(Some("content_superset"));
        }
    }
    Ok(None)
}

/// The audit's "twin 0" rule: every unique non-merge commit must have a
/// same-subject twin with exactly the same path, mode, added and removed lines.
/// Only hunk coordinates and unchanged context are ignored. This is still a
/// content heuristic, and must never authorize deletion without an archive.
fn same_subject_twins(repo: &Path, target: &str, tip: &str) -> Result<bool, String> {
    let merges = git_cmd(repo)
        .args(["rev-list", "--merges", &format!("{target}..{tip}")])
        .run()
        .map_err(|error| format!("could not inspect merge commits: {error}"))?;
    if !merges.stdout.trim().is_empty() {
        return Ok(false);
    }
    let cherry = git_cmd(repo)
        .args(["cherry", target, tip])
        .run()
        .map_err(|error| format!("could not compare patches: {error}"))?;
    let twins = git_cmd(repo)
        .args([
            "log",
            "--no-merges",
            "--format=%H%x09%s",
            &format!("{tip}..{target}"),
        ])
        .run()
        .map_err(|error| format!("could not inspect twin subjects: {error}"))?
        .stdout;
    let mut found = false;
    for line in cherry.stdout.lines().filter(|line| line.starts_with("+ ")) {
        let sha = &line[2..];
        let subject = git_cmd(repo)
            .args(["show", "--no-patch", "--format=%s", sha])
            .run()
            .map_err(|error| format!("could not inspect commit subject: {error}"))?
            .stdout;
        let Some(patch) = context_free_patch(repo, sha)? else {
            return Ok(false);
        };
        let mut matched = false;
        for (twin, twin_subject) in twins.lines().filter_map(|line| line.split_once('\t')) {
            if twin_subject == subject.trim()
                && context_free_patch(repo, twin)?.as_ref() == Some(&patch)
            {
                matched = true;
                break;
            }
        }
        if !matched {
            return Ok(false);
        }
        found = true;
    }
    Ok(found)
}

fn context_free_patch(repo: &Path, sha: &str) -> Result<Option<Vec<String>>, String> {
    let output = git_cmd(repo)
        .args([
            "show",
            "--format=",
            "--no-ext-diff",
            "--no-textconv",
            "--no-renames",
            "--unified=0",
            sha,
        ])
        .run()
        .map_err(|error| format!("could not inspect twin patch: {error}"))?;
    let mut patch = Vec::new();
    let mut in_hunk = false;
    for line in output.stdout.lines() {
        if line.starts_with("Binary files ") {
            return Ok(None);
        }
        if line.starts_with("diff --git ") {
            in_hunk = false;
            patch.push(line.to_owned());
        } else if line.starts_with("@@ ") {
            in_hunk = true;
        } else if (in_hunk && (line.starts_with(['+', '-', '\\'])))
            || line.starts_with("old mode ")
            || line.starts_with("new mode ")
            || line.starts_with("new file mode ")
            || line.starts_with("deleted file mode ")
        {
            patch.push(line.to_owned());
        }
    }
    Ok(Some(patch))
}

/// A conservative heuristic for revised twins from the branch audit. Require
/// 100% of added lines, with multiplicity, in the same regular file on target.
/// Deletions, mode changes, binary files and unrelated histories are not guessed.
fn content_superset(repo: &Path, target: &str, tip: &str) -> Result<bool, String> {
    let Ok(base) = git_cmd(repo).args(["merge-base", target, tip]).run() else {
        return Ok(false);
    };
    let base = base.stdout.trim();
    let changes = git_cmd(repo)
        .args([
            "diff",
            "--name-status",
            "--no-renames",
            "-z",
            base,
            tip,
            "--",
        ])
        .run()
        .map_err(|error| format!("could not inspect changed paths: {error}"))?
        .stdout;
    let mut fields = changes.split('\0').filter(|field| !field.is_empty());
    let mut found = false;
    while let Some(status) = fields.next() {
        let Some(path) = fields.next() else {
            return Ok(false);
        };
        if !matches!(status, "A" | "M") {
            return Ok(false);
        }
        let mode = |revision: &str| -> Option<String> {
            let output = git_cmd(repo)
                .args(["--literal-pathspecs", "ls-tree", revision, "--", path])
                .run()
                .ok()?;
            output.stdout.split_whitespace().next().map(str::to_owned)
        };
        let tip_mode = mode(tip);
        if !matches!(tip_mode.as_deref(), Some("100644" | "100755"))
            || mode(target) != tip_mode
            || (status == "M" && mode(base) != tip_mode)
        {
            return Ok(false);
        }
        let diff = git_cmd(repo)
            .args([
                "--literal-pathspecs",
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--no-renames",
                "--unified=0",
                base,
                tip,
                "--",
                path,
            ])
            .run()
            .map_err(|error| format!("could not compare file changes: {error}"))?
            .stdout;
        let mut in_hunk = false;
        let mut additions = Vec::new();
        for line in diff.lines() {
            if line.starts_with("Binary files ") || line.starts_with("\\ No newline") {
                return Ok(false);
            }
            if line.starts_with("@@ ") {
                in_hunk = true;
                continue;
            }
            if !in_hunk {
                continue;
            }
            if line.starts_with('-') {
                return Ok(false);
            }
            if let Some(added) = line.strip_prefix('+') {
                additions.push(added);
            }
        }
        let content = git_cmd(repo)
            .args(["show", &format!("{target}:{path}")])
            .run()
            .map_err(|error| format!("could not read integrated content: {error}"))?
            .stdout;
        let mut remaining = HashMap::<&str, usize>::new();
        for line in content.lines() {
            *remaining.entry(line).or_default() += 1;
        }
        for added in additions {
            let count = remaining.entry(added).or_default();
            if *count == 0 {
                return Ok(false);
            }
            *count -= 1;
            found |= !added.trim().is_empty();
        }
    }
    Ok(found)
}

fn patches_integrated_in(repo: &Path, target: &str, tip: &str) -> Result<bool, String> {
    // `git cherry` omits merge commits and their resolution changes.
    let merges = git_cmd(repo)
        .args(["rev-list", "--merges", &format!("{target}..{tip}")])
        .run()
        .map_err(|error| format!("could not inspect merge commits: {error}"))?;
    if !merges.stdout.trim().is_empty() {
        return Ok(false);
    }
    let cherry = git_cmd(repo)
        .args(["cherry", target, tip])
        .run()
        .map_err(|error| format!("could not compare patches: {error}"))?;
    Ok(cherry.stdout.lines().all(|line| line.starts_with("- ")))
}

pub fn inspect_workspace_lifecycle_with_pr(
    base_repo: &Path,
    workspace_id: &str,
    pr_proves_tip: impl Fn(&Path, &str, &str) -> bool,
) -> WorkspaceLifecycleStatus {
    let inspected = (|| -> Result<WorkspaceLifecycleStatus, String> {
        let workspace = resolve_any_workspace(base_repo, workspace_id)
            .or_else(|_| resolve_missing_registered_workspace(base_repo, workspace_id))?;
        if !Path::new(&workspace.path).exists() {
            let default_branch = get_remote_default_branch(&base_repo.to_string_lossy())?;
            let tip = rev_at(base_repo, &format!("refs/heads/{}", workspace.branch))?;
            let (commit_status, merge_proof) = classify_branch_merge(
                base_repo,
                &workspace.branch,
                &tip,
                &default_branch,
                pr_proves_tip,
            )?;
            return Ok(WorkspaceLifecycleStatus {
                dirty_files: None,
                missing_checkout: true,
                dirty_fingerprint: None,
                submodule_unpushed_commits: Vec::new(),
                commit_status,
                merge_proof,
                removal_safety: WorkspaceRemovalSafety::RequiresForce,
                error: None,
            });
        }
        let dirty_files = dirty_files_at(Path::new(&workspace.path))?;
        let (dirty_fingerprint, submodule_unpushed_commits) =
            dirty_fingerprint_at(Path::new(&workspace.path))?;
        let dirty = dirty_files > 0;
        let default_branch = get_remote_default_branch(&base_repo.to_string_lossy())?;
        let workspace_path = Path::new(&workspace.path);
        let tip = rev_at(workspace_path, "HEAD")?;
        let (commit_status, merge_proof) = classify_branch_merge(
            base_repo,
            &workspace.branch,
            &tip,
            &default_branch,
            pr_proves_tip,
        )?;
        Ok(WorkspaceLifecycleStatus {
            dirty_files: Some(dirty_files),
            missing_checkout: false,
            dirty_fingerprint: Some(dirty_fingerprint),
            submodule_unpushed_commits,
            commit_status,
            merge_proof,
            removal_safety: if dirty {
                WorkspaceRemovalSafety::RequiresForce
            } else {
                WorkspaceRemovalSafety::Safe
            },
            error: None,
        })
    })();

    inspected.unwrap_or_else(|error| WorkspaceLifecycleStatus {
        dirty_files: None,
        missing_checkout: false,
        dirty_fingerprint: None,
        submodule_unpushed_commits: Vec::new(),
        commit_status: WorkspaceCommitStatus::Unknown,
        merge_proof: None,
        removal_safety: WorkspaceRemovalSafety::Unknown,
        error: Some(error),
    })
}

/// Resolve a workspace id through git's linked-worktree list.
pub fn resolve_any_workspace(
    base_repo: &Path,
    workspace_id: &str,
) -> Result<WorkspaceWorktree, String> {
    let listed = git_cmd(base_repo)
        .args(["worktree", "list", "--porcelain"])
        .run()
        .map_err(|error| format!("git worktree list failed: {error}"))?;
    map_worktree_workspace_paths(&listed.stdout)
        .remove(workspace_id)
        .ok_or_else(|| {
            format!(
                "No workspace found for id '{workspace_id}' in '{}'",
                base_repo.display()
            )
        })
}

/// Removal alone may resolve a registered checkout whose directory disappeared.
/// The normal workspace listing deliberately hides these stale entries.
fn resolve_missing_registered_workspace(
    base_repo: &Path,
    workspace_id: &str,
) -> Result<WorkspaceWorktree, String> {
    let listed = git_cmd(base_repo)
        .args(["worktree", "list", "--porcelain"])
        .run()
        .map_err(|error| format!("git worktree list failed: {error}"))?;
    for entry in parse_worktree_entries(&listed.stdout) {
        let Some(branch) = entry.branch else { continue };
        let path = PathBuf::from(&entry.path);
        if workspace_id_of_worktree(&branch) == workspace_id
            && !path.exists()
            && registered_worktree_admin_dir(base_repo, &path)?.is_some()
        {
            return Ok(WorkspaceWorktree {
                branch,
                path: entry.path,
                kind: WorkspaceKind::Worktree,
                warm_artifacts: None,
                lifecycle_status: None,
            });
        }
    }
    Err(format!(
        "No workspace found for id '{workspace_id}' in '{}'",
        base_repo.display()
    ))
}

/// A workspace, however it was built.
///
/// One type for both mechanisms on purpose: the caller asked for a workspace,
/// and everything downstream — the sidebar row, publish, remove — needs to know
/// which one it got rather than infer it from the directory's shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceKind {
    Worktree,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedWorkspace {
    /// The linked worktree id. It currently equals the branch name.
    pub workspace_id: String,
    pub path: PathBuf,
    pub branch: String,
    pub kind: WorkspaceKind,
    /// Failures encountered while warming ignored build directories.
    pub warnings: Vec<String>,
    /// Git-ignored build directories clonefiled in from the parent so a linked
    /// worktree starts warm.
    pub warmed_directories: usize,
}

impl CreatedWorkspace {
    pub fn instruction_payload_pending(&self) -> serde_json::Value {
        let mut payload = self.instruction_payload();
        payload["warm_artifacts"]["status"] = serde_json::json!("pending");
        payload["warm_artifacts"]["note"] = serde_json::json!(
            "Build inputs are still being copied. Check this workspace's warm_artifacts.status with get_worktree_paths(repo_path) or GET /worktrees/paths?path=<repo_path>. Wait for done or failed before installing dependencies or building here."
        );
        payload
    }

    /// What the caller needs to know to USE this workspace, at the moment it
    /// can act on it.
    ///
    /// Boss ruled out every enforcement layer — no deny hooks, no shell
    /// override, no PATH shim — so this response is the only instruction
    /// channel there is. It states the things a model would otherwise get
    /// wrong by reflex, with their consequences, rather than in a preamble read
    /// 200k tokens ago:
    ///
    /// - inherited work in progress is not the model's own bug,
    /// - the warm artifacts are already there, so setting up is not a build,
    /// - and, for a clone, that the parent cannot see this branch — the failure
    ///   is silent, because `git merge` in the parent finds a same-named ref
    ///   and merges the WRONG one.
    pub fn instruction_payload(&self) -> serde_json::Value {
        let warm = crate::cow::warm_artifacts(&self.path);
        let warm_state = if WARM_STATES.contains_key(&warm_key(&self.path)) {
            warm_status(&self.path)
        } else if self.warnings.is_empty() {
            serde_json::json!({"status": "done"})
        } else {
            serde_json::json!({"status": "failed", "reason": self.warnings.join("; ")})
        };
        let isolation = "This is a linked worktree: refs and objects are shared with the parent \
            repository, so your commits are visible there immediately."
            .to_string();

        let setup = if warm.is_empty() {
            "No build output came with this workspace.".to_string()
        } else {
            "These came with the workspace at near-zero cost. Do NOT run an install or a full build \
             to \"set up\" — they are already warm. Run one only if a lockfile or a dependency \
             actually changed."
                .to_string()
        };

        serde_json::json!({
            "workspace_id": self.workspace_id,
            "path": self.path.to_string_lossy(),
            "branch": self.branch,
            "kind": self.kind,
            "warnings": self.warnings,
            "state": {
                "carried_over": 0,
                "note": "Tracked changes are not carried over: this workspace starts from a clean checkout.",
            },
            "warm_artifacts": {
                "status": warm_state["status"],
                "reason": warm_state.get("reason"),
                "present": warm,
                "warmed_directories": self.warmed_directories,
                "note": setup,
            },
            "isolation": isolation,
        })
    }
}

/// Create the registered worktree and its required local inputs, but defer the
/// expensive cache warm to the caller.
pub fn create_workspace_unwarmed(
    worktrees_dir: &Path,
    config: &WorktreeConfig,
    base_ref: Option<&str>,
) -> Result<CreatedWorkspace, String> {
    let src = PathBuf::from(&config.base_repo);
    let branch = config
        .branch
        .clone()
        .unwrap_or_else(|| sanitize_name(&config.task_name));
    ensure_branch_has_no_workspace(&src, &branch)?;
    let worktree = create_worktree_with_stale_recovery(worktrees_dir, config, base_ref)?;
    let branch = worktree.branch.unwrap_or(branch);
    let mut warnings = link_shared_stores(&src, &worktree.path);
    warnings.extend(initialize_submodules(&src, &worktree.path));
    Ok(CreatedWorkspace {
        workspace_id: workspace_id_of_worktree(&branch),
        path: worktree.path,
        branch,
        kind: WorkspaceKind::Worktree,
        warnings,
        warmed_directories: 0,
    })
}

/// `create_workspace` with warming injected for deterministic tests.
#[cfg(test)]
pub fn create_workspace_with(
    worktrees_dir: &Path,
    config: &WorktreeConfig,
    base_ref: Option<&str>,
    warm: impl Fn(&Path, &Path) -> crate::cow::WarmingReport,
) -> Result<CreatedWorkspace, String> {
    let mut workspace = create_workspace_unwarmed(worktrees_dir, config, base_ref)?;
    let src = PathBuf::from(&config.base_repo);

    // DEFERRED (2026-09-13): carrying the parent's tracked changes was dropped
    // with independent COW workspace creation. If reinstated, pipe
    // `git diff HEAD` in the parent to `git apply` in this linked worktree.
    let warming = warm(&src, &workspace.path);
    workspace.warnings.extend(warming.warnings);
    workspace.warmed_directories = warming.warmed;
    Ok(workspace)
}

const SHARED_WORKTREE_STORES: [&str; 3] = ["stories", "plans", "ideas"];

fn link_shared_stores(src: &Path, dest: &Path) -> Vec<String> {
    let mut warnings = Vec::new();
    for store in SHARED_WORKTREE_STORES {
        let source = src.join(store);
        if !source.is_dir() {
            continue;
        }
        let target = dest.join(store);
        if std::fs::symlink_metadata(&target).is_ok() {
            warnings.push(format!(
                "could not link shared store '{store}': '{}' already exists",
                target.display()
            ));
            continue;
        }
        #[cfg(unix)]
        let result = std::os::unix::fs::symlink(&source, &target);
        #[cfg(windows)]
        let result = std::os::windows::fs::symlink_dir(&source, &target);
        #[cfg(not(any(unix, windows)))]
        let result: std::io::Result<()> = Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "directory symlinks are unsupported",
        ));
        if let Err(error) = result {
            warnings.push(format!("could not link shared store '{store}': {error}"));
        }
    }
    warnings
}

fn initialize_submodules(src: &Path, dest: &Path) -> Vec<String> {
    initialize_submodules_with_allowed_protocol(src, dest, None)
}

/// `allowed_protocol`, when set, is granted only to the remote-fallback `git
/// submodule update` subprocess via [`GitCmd::env`] rather than the process
/// environment, so a caller (a test, say) can unlock a single transport
/// (e.g. `ext`) without leaking `GIT_ALLOW_PROTOCOL` into anything else
/// running concurrently in the same process.
fn initialize_submodules_with_allowed_protocol(
    src: &Path,
    dest: &Path,
    allowed_protocol: Option<&str>,
) -> Vec<String> {
    if !src.join(".gitmodules").is_file() {
        return Vec::new();
    }
    let Ok(listed) = git_cmd(src)
        .args([
            "config",
            "--file",
            ".gitmodules",
            "--get-regexp",
            r"^submodule\..*\.path$",
        ])
        .run()
    else {
        return vec!["could not read submodule declarations".into()];
    };
    listed.stdout.lines().filter_map(|line| {
        let (key, path) = line.split_once(char::is_whitespace)?;
        let name = key.strip_prefix("submodule.")?.strip_suffix(".path")?;
        let url_key = format!("submodule.{name}.url");
        let local = git_cmd(dest).args(["config", &url_key, &src.join(path).to_string_lossy()]).run().and_then(|_| git_cmd(dest).args(["-c", "protocol.file.allow=always", "submodule", "update", "--init", "--", path]).run());
        if local.is_ok() { return None; }
        let local_error = local.expect_err("failed above");
        let _ = git_cmd(dest).args(["config", "--unset", &url_key]).run();
        let mut update = git_cmd(dest).args(["submodule", "update", "--init", "--", path]).timeout(FETCH_TIMEOUT);
        if let Some(protocol) = allowed_protocol {
            update = update.env("GIT_ALLOW_PROTOCOL", protocol);
        }
        match git_cmd(dest).args(["submodule", "sync", "--", path]).run().and_then(|_| update.run()) {
            Ok(_) => Some(format!("submodule '{path}' could not use the parent checkout ({local_error}); initialized from its configured remote instead")),
            Err(remote_error) => Some(format!("could not initialize submodule '{path}' from the parent checkout ({local_error}) or its configured remote ({remote_error})")),
        }
    }).collect()
}

/// A branch can belong to only one linked worktree.
fn ensure_branch_has_no_workspace(base_repo: &Path, branch: &str) -> Result<(), String> {
    let listed = git_cmd(base_repo)
        .args(["worktree", "list", "--porcelain"])
        .run()
        .map_err(|error| format!("git worktree list failed: {error}"))?;
    match map_worktree_workspace_paths(&listed.stdout)
        .into_iter()
        .find(|(_, workspace)| workspace.branch == branch)
    {
        Some((workspace_id, workspace)) => Err(format!(
            "branch '{branch}' already belongs to linked worktree '{workspace_id}' at '{}'; reuse that worktree",
            workspace.path
        )),
        None => Ok(()),
    }
}
/// Remove a detached orphan. A checkout whose directory is already gone holds
/// nothing to lose, so dropping its registration is confirmed by the safety
/// assessment instead of a `force` flag.
pub fn remove_orphan_worktree_internal(worktree: &WorktreeInfo) -> Result<(), String> {
    remove_worktree_internal(worktree, !path_entry_exists(&worktree.path)?)
}

pub fn remove_worktree_internal(worktree: &WorktreeInfo, force: bool) -> Result<(), String> {
    remove_worktree_internal_with_lock(worktree, force, false, None, None)
}

fn registered_worktree_admin_dir(base_repo: &Path, path: &Path) -> Result<Option<PathBuf>, String> {
    let admin_root = PathBuf::from(rev_at(base_repo, "--absolute-git-dir")?).join("worktrees");
    let entries = match std::fs::read_dir(&admin_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Cannot inspect worktree registrations: {error}")),
    };
    for entry in entries {
        let entry =
            entry.map_err(|error| format!("Cannot inspect worktree registration: {error}"))?;
        let gitdir = match std::fs::read_to_string(entry.path().join("gitdir")) {
            Ok(gitdir) => gitdir,
            Err(_) => continue,
        };
        if Path::new(gitdir.trim()).parent().map(warm_key).as_deref() == Some(&warm_key(path)) {
            return Ok(Some(entry.path()));
        }
    }
    Ok(None)
}

fn preserve_missing_worktree_modules(
    base_repo: &Path,
    worktree: &Path,
    admin: &Path,
) -> Result<(), String> {
    fn visit(
        base_repo: &Path,
        worktree: &Path,
        path: &Path,
        names: &mut Vec<String>,
        inside_repo: bool,
    ) -> Result<(), String> {
        if !path.is_dir() {
            return Ok(());
        }
        let is_repo = path.join("HEAD").is_file() && path.join("objects").is_dir();
        if is_repo {
            let relative = names.join("/");
            let destination = base_repo.join(&relative);
            if !initialized_main_submodule(base_repo, &destination) {
                return Err(format!(
                    "Cannot preserve missing worktree submodule {relative}: main module is not initialized"
                ));
            }
            let refs = git_cmd(base_repo)
                .args([
                    "--git-dir",
                    &path.to_string_lossy(),
                    "--work-tree",
                    &base_repo.to_string_lossy(),
                    "for-each-ref",
                    "--format=%(refname) %(objectname)",
                ])
                .run()
                .map_err(|e| format!("Cannot inspect missing submodule {relative}: {e}"))?;
            let head = git_cmd(base_repo)
                .args([
                    "--git-dir",
                    &path.to_string_lossy(),
                    "--work-tree",
                    &base_repo.to_string_lossy(),
                    "rev-parse",
                    "HEAD",
                ])
                .run()
                .map_err(|e| format!("Cannot inspect missing submodule HEAD {relative}: {e}"))?;
            let reflog = git_cmd(base_repo)
                .args([
                    "--git-dir",
                    &path.to_string_lossy(),
                    "--work-tree",
                    &base_repo.to_string_lossy(),
                    "reflog",
                    "show",
                    "--all",
                    "--format=%H",
                ])
                .run()
                .map_err(|e| format!("Cannot inspect missing submodule reflog {relative}: {e}"))?;
            // A deleted checkout leaves core.worktree pointing at a path that no
            // longer exists. Bundle from its gitdir with an explicit live worktree
            // so upload-pack never tries to chdir into the vanished checkout.
            let bundle = tempfile::Builder::new()
                .prefix("tuic-module-")
                .suffix(".bundle")
                .tempfile_in(rev_at(base_repo, "--absolute-git-dir")?)
                .map_err(|e| format!("Cannot stage missing submodule refs: {e}"))?;
            git_cmd(base_repo)
                .args([
                    "--git-dir",
                    &path.to_string_lossy(),
                    "--work-tree",
                    &base_repo.to_string_lossy(),
                    "bundle",
                    "create",
                    &bundle.path().to_string_lossy(),
                    "HEAD",
                    "--all",
                    "--reflog",
                ])
                .run()
                .map_err(|e| format!("Cannot bundle missing submodule {relative}: {e}"))?;
            let namespace = format!(
                "refs/tuic/preserved/{}/{}/{}/",
                preserved_ref_component(worktree.to_string_lossy().as_bytes()),
                preserved_ref_component(relative.as_bytes()),
                uuid::Uuid::new_v4().simple()
            );
            let mut args = vec![
                "fetch".to_string(),
                "--no-tags".to_string(),
                "--no-recurse-submodules".to_string(),
                "--no-write-fetch-head".to_string(),
                bundle.path().to_string_lossy().into_owned(),
                format!(
                    "{}:{}{}",
                    head.stdout.trim(),
                    namespace,
                    preserved_ref_component(b"HEAD")
                ),
            ];
            for line in refs.stdout.lines() {
                let (name, oid) = line
                    .split_once(' ')
                    .ok_or_else(|| format!("Cannot parse submodule ref: {line}"))?;
                args.push(format!(
                    "{oid}:{}{}",
                    namespace,
                    preserved_ref_component(name.as_bytes())
                ));
            }
            for oid in reflog.stdout.lines() {
                args.push(format!(
                    "{oid}:{}{}",
                    namespace,
                    preserved_ref_component(format!("reflog/{oid}").as_bytes())
                ));
            }
            git_cmd(&destination)
                .args(&args)
                .timeout(Duration::from_secs(60))
                .run()
                .map_err(|e| format!("Cannot preserve missing submodule {relative}: {e}"))?;
        }
        for entry in std::fs::read_dir(path)
            .map_err(|e| format!("Cannot inspect module admin directory: {e}"))?
        {
            let entry = entry.map_err(|e| format!("Cannot inspect module admin entry: {e}"))?;
            if !entry.path().is_dir()
                || entry.file_name() == "objects"
                || entry.file_name() == "refs"
                || entry.file_name() == "logs"
            {
                continue;
            }
            let component = entry.file_name().to_string_lossy().into_owned();
            let structural_modules = component == "modules" && (inside_repo || is_repo);
            if !structural_modules {
                names.push(component.clone());
            }
            visit(
                base_repo,
                worktree,
                &entry.path(),
                names,
                inside_repo || is_repo,
            )?;
            if !structural_modules {
                names.pop();
            }
        }
        Ok(())
    }
    visit(
        base_repo,
        worktree,
        &admin.join("modules"),
        &mut Vec::new(),
        false,
    )
}

fn path_entry_exists(path: &Path) -> Result<bool, String> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("Cannot inspect {}: {error}", path.display())),
    }
}

fn finish_unregistered_worktree_removal(worktree: &WorktreeInfo) -> Result<(), String> {
    if registered_worktree_admin_dir(&worktree.base_repo, &worktree.path)?.is_some()
        || path_entry_exists(&worktree.path.join(".git"))?
    {
        return Err("Cannot finish removal: worktree is still registered".into());
    }
    // Once Git has removed its registration, no later warm may recreate the
    // checkout. A failed cleanup reports the actual remaining path to callers.
    clear_warm(&worktree.path);
    if path_entry_exists(&worktree.path)? {
        std::fs::remove_dir_all(&worktree.path).map_err(|error| {
            format!(
                "Worktree has no Git registration but its directory remains at {}: {error}",
                worktree.path.display()
            )
        })?;
    }
    Ok(())
}

fn remove_worktree_internal_with_lock(
    worktree: &WorktreeInfo,
    force: bool,
    override_lock: bool,
    expected_fingerprint: Option<&str>,
    expected_missing_checkout: Option<bool>,
) -> Result<(), String> {
    // A copy already in flight must finish before Git can remove its destination.
    // If removal wins, the queued copy sees the cleared token and never starts.
    let warm_lock = warm_lock(&worktree.path);
    let _warm_guard = warm_lock.as_ref().map(|lock| {
        lock.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    });
    let wt_path_str = worktree.path.to_string_lossy().to_string();
    tracing::info!(
        source = "worktree",
        branch = %worktree.name,
        path = %wt_path_str,
        force = %force,
        "remove_worktree_internal: start"
    );

    if warm_key(&worktree.path) == warm_key(&worktree.base_repo) {
        return Err(format!(
            "{MAIN_WORKTREE_PREFIX}cannot remove the main worktree"
        ));
    }
    if let Some(expected_missing) = expected_missing_checkout
        && expected_missing == path_entry_exists(&worktree.path)?
    {
        return Err(
            "Worktree presence changed since confirmation; review it before removal".into(),
        );
    }
    let admin = registered_worktree_admin_dir(&worktree.base_repo, &worktree.path)?;
    if !path_entry_exists(&worktree.path)? {
        if let Some(admin) = admin {
            if admin.join("locked").exists() && !override_lock {
                return Err(format!(
                    "{LOCKED_WORKTREE_PREFIX}missing worktree is locked"
                ));
            }
            if !force {
                return Err(
                    "Cannot remove missing worktree registration without force confirmation".into(),
                );
            }
            preserve_missing_worktree_modules(&worktree.base_repo, &worktree.path, &admin)?;
            let force_args: &[&str] = if override_lock {
                &["worktree", "remove", "--force", "--force"]
            } else {
                &["worktree", "remove", "--force"]
            };
            git_cmd(&worktree.base_repo)
                .args(
                    force_args
                        .iter()
                        .copied()
                        .chain(std::iter::once(wt_path_str.as_str())),
                )
                .run()
                .map_err(|e| format!("Cannot remove missing worktree registration: {e}"))?;
            if admin.exists() {
                return Err("Cannot prune missing worktree registration".into());
            }
        }
        clear_warm(&worktree.path);
        return Ok(());
    }
    if admin.is_none() {
        // Only a warm token proves that TUIC previously created this checkout.
        // Without one, this could be an unrelated directory at the same path.
        if warm_lock.is_none() {
            return Err(format!(
                "Worktree directory remains without Git registration at {}; refusing unverified cleanup",
                worktree.path.display()
            ));
        }
        return finish_unregistered_worktree_removal(worktree);
    }
    if has_operation_in_progress(&wt_path_str) {
        return Err("Cannot remove worktree: a Git operation is in progress".into());
    }
    let head_ref = git_cmd(&worktree.path)
        .args(["symbolic-ref", "-q", "HEAD"])
        .run_raw()
        .map_err(|error| format!("Cannot inspect worktree HEAD: {error}"))?;
    match head_ref.status.code() {
        Some(0) => {}
        Some(1) => {
            let head = rev_at(&worktree.path, "HEAD")?;
            let containing = git_cmd(&worktree.base_repo)
                .args([
                    "for-each-ref",
                    &format!("--contains={head}"),
                    "--count=1",
                    "--format=%(refname)",
                ])
                .run()
                .map_err(|error| format!("Cannot check detached HEAD reachability: {error}"))?;
            if containing.stdout.trim().is_empty() {
                return Err(
                    "Cannot remove worktree: detached HEAD commit has no durable ref".into(),
                );
            }
        }
        code => {
            return Err(format!(
                "Cannot inspect worktree HEAD (git exit {code:?}): {}",
                String::from_utf8_lossy(&head_ref.stderr).trim()
            ));
        }
    }

    let before = if admin.is_some() {
        Some(dirty_fingerprint_at(&worktree.path)?.0)
    } else {
        None
    };

    if !force && dirty_files_at(&worktree.path)? != 0 {
        return Err("Cannot remove worktree: the worktree has uncommitted changes".into());
    }
    if admin.is_some() {
        verify_submodules_at(&worktree.path, &worktree.base_repo, force)?;
    }
    if let Some(before) = before {
        let after = dirty_fingerprint_at(&worktree.path)?.0;
        if before != after {
            return Err(
                "Worktree state changed while preserving submodules; review it before removal"
                    .into(),
            );
        }
        if !force && dirty_files_at(&worktree.path)? != 0 {
            return Err("Cannot remove worktree: the worktree gained uncommitted changes".into());
        }
    }
    if let Some(expected) = expected_fingerprint {
        let (current, _) = dirty_fingerprint_at(&worktree.path)?;
        if current != expected {
            return Err(
                "Worktree state changed since confirmation; review it before removal".into(),
            );
        }
    }

    // Git requires one --force even for clean populated submodules. The caller
    // proves cleanliness first; a second --force would also override a lock.
    let force_args: &[&str] = if override_lock {
        &["worktree", "remove", "--force", "--force"]
    } else {
        &["worktree", "remove", "--force"]
    };

    // Git may delete tracked files before a sealed ignored directory stops it.
    // Make only this checkout removable while its registration is still intact.
    crate::cow::restore_owner_write(&worktree.path).map_err(|error| {
        format!(
            "Cannot prepare worktree {} for removal: {error}",
            worktree.path.display()
        )
    })?;

    match git_cmd(&worktree.base_repo)
        .args(
            force_args
                .iter()
                .chain(std::iter::once(&wt_path_str.as_str())),
        )
        .run()
    {
        Ok(_) => {
            tracing::info!(source = "worktree", branch = %worktree.name, force = %force, "git worktree remove: OK");
        }
        Err(crate::git_cli::GitError::NonZeroExit { ref stderr, .. })
            if stderr.contains("not a working tree") || stderr.contains("No such file") =>
        {
            tracing::info!(
                source = "worktree",
                branch = %worktree.name,
                "git worktree remove: worktree already gone (treating as success)"
            );
        }
        Err(crate::git_cli::GitError::NonZeroExit { ref stderr, .. })
            if stderr.contains("locked working tree")
                || stderr.contains("cannot remove a locked") =>
        {
            // A lock is independent of permission to discard dirty files.
            tracing::warn!(
                source = "worktree",
                branch = %worktree.name,
                stderr = %stderr,
                "git worktree remove: locked — returning error for JS confirmation prompt"
            );
            return Err(format!("{LOCKED_WORKTREE_PREFIX}{stderr}"));
        }
        Err(crate::git_cli::GitError::NonZeroExit { ref stderr, .. })
            if stderr.contains("is a main working tree") =>
        {
            // The branch is checked out in the main repo directory, not a linked
            // worktree. `git worktree remove` is not the right tool here.
            // Return a distinctive prefix so the JS layer can show a clear message
            // and NOT remove the branch from the store (avoiding resurrection).
            tracing::warn!(
                source = "worktree",
                branch = %worktree.name,
                "git worktree remove: branch is in main worktree, cannot remove"
            );
            return Err(format!("{MAIN_WORKTREE_PREFIX}{stderr}"));
        }
        Err(e) => {
            tracing::error!(source = "worktree", branch = %worktree.name, "git worktree remove FAILED: {e}");
            if registered_worktree_admin_dir(&worktree.base_repo, &worktree.path)?.is_none()
                && !path_entry_exists(&worktree.path.join(".git"))?
            {
                tracing::warn!(
                    source = "worktree",
                    branch = %worktree.name,
                    "Git removed the registration before failing; finishing directory cleanup"
                );
                return finish_unregistered_worktree_removal(worktree);
            }
            return Err(crate::git_locks::describe_stale_lock(&worktree.base_repo)
                .unwrap_or_else(|| format!("Git worktree remove failed: {e}")));
        }
    }

    if path_entry_exists(&worktree.path)? {
        tracing::warn!(
            source = "worktree",
            branch = %worktree.name,
            path = %wt_path_str,
            "directory still exists after git worktree remove — finishing cleanup"
        );
    }
    finish_unregistered_worktree_removal(worktree)?;

    tracing::info!(source = "worktree", branch = %worktree.name, "remove_worktree_internal: done");
    Ok(())
}

/// Adjective + sci-fi character worktree name generator
pub fn generate_worktree_name(existing: &[String]) -> String {
    let adjectives = [
        "brave", "calm", "dark", "eager", "fair", "glad", "happy", "keen", "lush", "mild", "neat",
        "proud", "quick", "rare", "safe", "tall", "vast", "warm", "wise", "bold", "cool", "deep",
        "fast", "gold", "huge", "iron", "jade", "kind", "lean", "mint", "nova", "open", "pale",
        "red", "slim", "tidy", "ultra", "vivid", "wild", "zen",
    ];

    let names = [
        "neo",
        "ripley",
        "deckard",
        "morpheus",
        "trinity",
        "cypher",
        "nexus",
        "cortex",
        "tron",
        "hal",
        "skynet",
        "muad",
        "atreides",
        "harkonnen",
        "seldon",
        "daneel",
        "solaris",
        "neuro",
        "winter",
        "armitage",
        "molly",
        "case",
        "hiro",
        "kovacs",
        "takeshi",
        "quell",
        "pris",
        "batty",
        "zhora",
        "gaff",
        "tyrell",
        "gibson",
        "asimov",
        "vance",
        "rama",
        "ender",
        "bean",
        "valentine",
        "petra",
        "revan",
    ];

    // Simple PRNG using current time
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    for attempt in 0..100u128 {
        let adj_idx =
            ((seed.wrapping_add(attempt.wrapping_mul(7))) % adjectives.len() as u128) as usize;
        let name_idx = ((seed.wrapping_add(attempt.wrapping_mul(13)).wrapping_add(3))
            % names.len() as u128) as usize;
        let num = ((seed.wrapping_add(attempt.wrapping_mul(31))) % 1000) as u16;
        let name = format!("{}-{}-{:03}", adjectives[adj_idx], names[name_idx], num);
        if !existing.contains(&name) {
            return name;
        }
    }

    // Fallback
    format!("worktree-{}", seed % 10000)
}

/// Generate a hybrid branch name for the quick-clone flow.
///
/// Format: `{source_branch}--{random_name}` (e.g., `feat-auth--brave-neo-042`).
/// The double-dash separator makes it easy to parse the source branch later.
/// Checks collision against `existing` list and regenerates random part if needed.
pub fn generate_clone_branch_name(source_branch: &str, existing: &[String]) -> String {
    let sanitized = sanitize_name(source_branch);
    for _ in 0..100 {
        let random_part = generate_worktree_name(existing);
        let name = format!("{sanitized}--{random_part}");
        if !existing.contains(&name) {
            return name;
        }
    }
    // Fallback with timestamp
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("{sanitized}--wt-{}", ts % 100000)
}

pub fn ipc_worktree_response(workspace: &CreatedWorkspace, base_repo: &str) -> serde_json::Value {
    serde_json::json!({
        "status": "ok",
        "name": workspace.path.file_name().map(|name| name.to_string_lossy().to_string()),
        "path": workspace.path.to_string_lossy(),
        "workspace_id": workspace.workspace_id,
        "branch": workspace.branch,
        "base_repo": base_repo,
        "kind": workspace.kind,
        "instructions": workspace.instruction_payload_pending(),
    })
}
/// Core logic for removing one workspace's checkout, addressed by workspace id.
///
/// When `delete_branch` is true, also deletes the local branch after removing
/// the worktree directory. When false, the branch is preserved.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RemoveWorktreeOutcome {
    pub branch_delete_warning: Option<String>,
    pub removal_rule: String,
    /// Branch the removed workspace was on, read off the record before removal.
    /// Callers need it for branch-keyed follow-up work (config labels, logs) and
    /// cannot re-resolve it: the id stops resolving the moment the worktree is
    /// gone, and it is not the branch to begin with once ids are minted.
    pub branch: String,
}

pub fn remove_worktree_by_workspace_id(
    repo_path: &str,
    workspace_id: &str,
    delete_branch: bool,
    archive_script: Option<&str>,
    force: bool,
) -> Result<RemoveWorktreeOutcome, String> {
    remove_worktree_by_workspace_id_with_lock(
        repo_path,
        workspace_id,
        delete_branch,
        archive_script,
        force,
        false,
    )
}

pub fn remove_worktree_by_workspace_id_with_lock(
    repo_path: &str,
    workspace_id: &str,
    delete_branch: bool,
    archive_script: Option<&str>,
    force: bool,
    override_lock: bool,
) -> Result<RemoveWorktreeOutcome, String> {
    remove_worktree_by_workspace_id_with_confirmation(
        repo_path,
        workspace_id,
        delete_branch,
        archive_script,
        force,
        override_lock,
        None,
    )
}

pub fn remove_worktree_by_workspace_id_with_confirmation(
    repo_path: &str,
    workspace_id: &str,
    delete_branch: bool,
    archive_script: Option<&str>,
    force: bool,
    override_lock: bool,
    expected_fingerprint: Option<&str>,
) -> Result<RemoveWorktreeOutcome, String> {
    remove_worktree_by_workspace_id_with_confirmation_and_pr(
        repo_path,
        workspace_id,
        delete_branch,
        archive_script,
        force,
        override_lock,
        expected_fingerprint,
        |_, _, _| false,
    )
}

// The flat removal boundary keeps each independently confirmed safety input explicit.
// A parameter-count warning here does not justify changing the caller contract.
#[expect(
    clippy::too_many_arguments,
    reason = "explicit removal confirmation boundary"
)]
pub fn remove_worktree_by_workspace_id_with_confirmation_and_pr(
    repo_path: &str,
    workspace_id: &str,
    delete_branch: bool,
    archive_script: Option<&str>,
    force: bool,
    override_lock: bool,
    expected_fingerprint: Option<&str>,
    pr_proves_tip: impl Fn(&Path, &str, &str) -> bool,
) -> Result<RemoveWorktreeOutcome, String> {
    remove_worktree_by_workspace_id_with_missing_confirmation_and_pr(
        repo_path,
        workspace_id,
        delete_branch,
        archive_script,
        force,
        override_lock,
        expected_fingerprint,
        None,
        pr_proves_tip,
    )
}

/// Transport-facing variant: `Some(true)` confirms a missing checkout, while
/// `Some(false)` requires a live checkout and a fingerprint for force removal.
// The flat removal boundary keeps each independently confirmed safety input explicit.
// A parameter-count warning here does not justify changing the caller contract.
#[expect(
    clippy::too_many_arguments,
    reason = "explicit removal confirmation boundary"
)]
pub fn remove_worktree_by_workspace_id_with_missing_confirmation_and_pr(
    repo_path: &str,
    workspace_id: &str,
    delete_branch: bool,
    archive_script: Option<&str>,
    force: bool,
    override_lock: bool,
    expected_fingerprint: Option<&str>,
    expected_missing_checkout: Option<bool>,
    pr_proves_tip: impl Fn(&Path, &str, &str) -> bool,
) -> Result<RemoveWorktreeOutcome, String> {
    let base_repo = PathBuf::from(repo_path);
    let mut branch_delete_warning = None;
    let mut removal_rule = if force { "force" } else { "kept_branch" };

    tracing::info!(
        source = "worktree",
        workspace_id = %workspace_id,
        delete_branch = %delete_branch,
        "remove_worktree_by_workspace_id: start"
    );

    let workspace = resolve_any_workspace(&base_repo, workspace_id)
        .or_else(|_| resolve_missing_registered_workspace(&base_repo, workspace_id))
        .inspect_err(|_| {
            tracing::error!(
                source = "worktree",
                workspace_id = %workspace_id,
                "remove_worktree_by_workspace_id: no workspace found for id"
            );
        })?;
    // The branch to delete comes off the resolved record instead of duplicating
    // the workspace-id representation at the call site.
    let branch_name = workspace.branch.as_str();
    let worktree_path = PathBuf::from(&workspace.path);
    let missing_checkout = !path_entry_exists(&worktree_path)?;
    if let Some(expected_missing) = expected_missing_checkout {
        if expected_missing != missing_checkout {
            return Err(
                "Worktree presence changed since confirmation; review it before removal".into(),
            );
        }
        if force && !missing_checkout && expected_fingerprint.is_none() {
            return Err(
                "force requires expected_fingerprint from the confirmed lifecycle status".into(),
            );
        }
    }

    // Force permits discarding dirty files, but never detached commits or a
    // lock. Branch deletion still needs its own proof in either mode.
    let branch_ref = format!("refs/heads/{branch_name}");
    let expected_branch_oid = rev_at(&base_repo, &branch_ref)?;
    let lifecycle = inspect_workspace_lifecycle_with_pr(&base_repo, workspace_id, pr_proves_tip);
    if force
        && let Some(expected) = expected_fingerprint
        && lifecycle.dirty_fingerprint.as_deref() != Some(expected)
    {
        return Err("Worktree state changed since confirmation; review it before removal".into());
    }
    if !force && lifecycle.dirty_files != Some(0) {
        return Err(lifecycle.error.clone().unwrap_or_else(|| {
            format!("Cannot remove {branch_name}: the worktree has uncommitted changes")
        }));
    }
    if has_operation_in_progress(&workspace.path) {
        return Err(format!(
            "Cannot remove {branch_name}: a Git operation is in progress"
        ));
    }
    if !missing_checkout && rev_at(&worktree_path, "HEAD")? != expected_branch_oid {
        return Err(format!(
            "Cannot remove {branch_name}: worktree HEAD differs from its branch tip"
        ));
    }
    if delete_branch {
        let branch_proof = (|| -> Result<&'static str, String> {
            match lifecycle.commit_status {
                WorkspaceCommitStatus::Unmerged => {
                    let default_branch = get_remote_default_branch(repo_path)?;
                    let bases = integration_bases(&base_repo, &default_branch);
                    Err(format!(
                        "Cannot remove {branch_name}: branch has unmerged commits (compared against {}). Merge it first, or remove the worktree while keeping the branch.",
                        describe_bases(&bases)
                    ))
                }
                WorkspaceCommitStatus::PushedUnmerged => Ok("remote_tracking"),
                WorkspaceCommitStatus::Unknown => {
                    Err(lifecycle.error.clone().unwrap_or_else(|| {
                        format!("Cannot verify whether {branch_name} can be safely removed")
                    }))
                }
                WorkspaceCommitStatus::InSync => Ok("in_sync"),
                WorkspaceCommitStatus::Merged => {
                    let proof = lifecycle
                        .merge_proof
                        .ok_or_else(|| format!("Cannot verify merged commits for {branch_name}"))?;
                    require_integration_archive(
                        &base_repo,
                        branch_name,
                        &expected_branch_oid,
                        proof,
                    )?;
                    Ok(proof)
                }
            }
        })();
        match branch_proof {
            Ok(rule) => removal_rule = rule,
            Err(error) if force => branch_delete_warning = Some(error),
            Err(error) => return Err(error),
        }
    }

    tracing::info!(
        source = "worktree",
        workspace_id = %workspace_id,
        branch = %branch_name,
        path = %worktree_path.display(),
        "remove_worktree_by_workspace_id: worktree path resolved"
    );

    // Run archive/cleanup script before deletion (if configured)
    if let Some(script) = archive_script
        && !missing_checkout
        && !script.is_empty()
    {
        run_script_in_dir(script, &worktree_path)
            .map_err(|e| format!("Archive script failed: {e}"))?;
    }

    // Remove the worktree
    let worktree = WorktreeInfo {
        name: workspace_id.to_string(),
        path: worktree_path,
        branch: Some(branch_name.to_string()),
        base_repo,
    };

    remove_worktree_internal_with_lock(
        &worktree,
        force,
        override_lock,
        expected_fingerprint,
        expected_missing_checkout,
    )?;

    // Compare-and-delete prevents an archive hook or another process from
    // advancing the branch after the safety proof.
    if delete_branch && branch_delete_warning.is_none() {
        let deleted = require_integration_archive(
            &worktree.base_repo,
            branch_name,
            &expected_branch_oid,
            removal_rule,
        )
        .and_then(|()| {
            git_cmd(&worktree.base_repo)
                .args([
                    "update-ref",
                    "-d",
                    &format!("refs/heads/{branch_name}"),
                    &expected_branch_oid,
                ])
                .run()
                .map_err(|error| error.to_string())
        });
        match deleted {
            Ok(_) => tracing::info!(
                source = "worktree",
                branch = %branch_name,
                "git branch delete: OK"
            ),
            Err(e) => {
                let warning = format!("Branch {branch_name} changed or could not be deleted: {e}");
                tracing::warn!(
                    source = "worktree",
                    branch = %branch_name,
                    "git branch delete failed (branch ref preserved): {e}"
                );
                branch_delete_warning = Some(warning);
            }
        }
    }

    tracing::info!(
        source = "worktree",
        workspace_id = %workspace_id,
        branch = %branch_name,
        "remove_worktree_by_workspace_id: done"
    );
    Ok(RemoveWorktreeOutcome {
        branch_delete_warning,
        removal_rule: removal_rule.to_string(),
        branch: branch_name.to_string(),
    })
}

/// Check whether a workspace's working directory has uncommitted changes.
///
/// Resolves the workspace by id and runs `git status --porcelain` in its
/// directory. If the id resolves to no checkout (bare local ref), returns
/// `false` — there's nothing to be dirty.
pub fn check_worktree_dirty(repo_path: String, workspace_id: String) -> Result<bool, String> {
    match worktree_dirtiness(Path::new(&repo_path), &workspace_id) {
        WorktreeDirtiness::Clean => Ok(false),
        WorktreeDirtiness::Dirty => Ok(true),
        // An unanswered question is an error here, never a "no". Callers that
        // gate a destructive action on this must see the failure.
        WorktreeDirtiness::Unknown(reason) => Err(reason),
    }
}

/// Return the same workspace lifecycle verdict used by repository refresh and
/// removal confirmation. This is intentionally a fresh read: a cached sidebar
/// badge is explanation, not authorization for a destructive action.
#[cfg(test)]
pub async fn get_workspace_lifecycle(
    repo_path: String,
    workspace_id: String,
) -> Result<WorkspaceLifecycleStatus, String> {
    tokio::task::spawn_blocking(move || {
        Ok(inspect_workspace_lifecycle(
            Path::new(&repo_path),
            &workspace_id,
        ))
    })
    .await
    .map_err(|e| format!("workspace lifecycle task failed: {e}"))?
}

/// Delete a local branch, disposing of the workspace `workspace_id` names.
///
/// Two different objects, two parameters: `branch_name` is the ref to delete,
/// `workspace_id` is the checkout holding it. They are the same string only
/// under the identity migration, and the id must never be parsed back into a
/// branch — so the branch always comes from the caller or the resolved record,
/// never from the id.
///
/// When the id resolves to a checkout, behaviour depends on `keep_worktree`:
/// - `false` (default): remove the worktree directory together with the branch
///   ref via `remove_worktree_by_workspace_id`.
/// - `true`: detach the worktree HEAD (so the branch ref is no longer checked
///   out anywhere), then delete the branch ref with `git branch -d`. The
///   worktree directory and its files are preserved.
///
/// When it resolves to nothing the branch is a bare ref, and only the ref goes.
///
/// Safety: refuses to delete the repository's default branch, and refuses when
/// the resolved workspace is on a different branch than the one asked for —
/// that mismatch means the caller's id and branch disagree, and guessing which
/// one it meant is how the wrong ref gets deleted.
/// Uses `git branch -d` (safe delete) which fails if the branch has unmerged commits.
pub fn delete_local_branch_impl(
    repo_path: &str,
    branch_name: &str,
    workspace_id: &str,
    keep_worktree: bool,
) -> Result<(), String> {
    // Refuse to delete the default branch
    let default_branch =
        get_remote_default_branch(repo_path).unwrap_or_else(|_| "main".to_string());
    if branch_name == default_branch {
        return Err(format!("Refusing to delete default branch '{branch_name}'"));
    }

    let base_repo = PathBuf::from(repo_path);

    // Resolve the checkout by id. `None` is a bare branch, not an error.
    let workspace = git_cmd(&base_repo)
        .args(["worktree", "list", "--porcelain"])
        .run_silent()
        .and_then(|o| map_worktree_workspace_paths(&o.stdout).remove(workspace_id));

    if let Some(ref ws) = workspace
        && ws.branch != branch_name
    {
        return Err(format!(
            "Workspace '{workspace_id}' is on branch '{}', not '{branch_name}' — refusing to delete",
            ws.branch
        ));
    }
    let worktree_path = workspace.map(|ws| PathBuf::from(ws.path));

    match (worktree_path, keep_worktree) {
        (Some(wt_path), true) => {
            // Detach the worktree HEAD so `git branch -d` will accept the
            // branch as deletable while leaving the worktree files on disk.
            git_cmd(&wt_path)
                .args(["checkout", "--detach"])
                .run()
                .map_err(|e| {
                    format!(
                        "git checkout --detach in worktree {} failed: {e}",
                        wt_path.display()
                    )
                })?;
            git_cmd(&base_repo)
                .args(["branch", "-d", "--", branch_name])
                .run()
                .map_err(|e| format!("git branch -d {branch_name} failed: {e}"))?;
        }
        (Some(_), false) => {
            // Remove worktree + branch in one go
            remove_worktree_by_workspace_id(repo_path, workspace_id, true, None, false)?;
        }
        (None, _) => {
            // Bare branch — no worktree to consider
            git_cmd(&base_repo)
                .args(["branch", "-d", "--", branch_name])
                .run()
                .map_err(|e| format!("git branch -d {branch_name} failed: {e}"))?;
        }
    }

    Ok(())
}

/// Ref under which the coordinator preserves a retired branch tip.
pub fn archive_ref_name(branch_name: &str) -> String {
    format!("refs/archive/{branch_name}")
}

/// Archive refs that may hold `tip` of `branch`: the primary ref, then the one
/// a reused branch name falls back to, named by the tip's short SHA.
fn archive_refs_for_tip(branch: &str, tip: &str) -> [String; 2] {
    let primary = archive_ref_name(branch);
    let suffixed = format!("{primary}-{}", tip.get(..7).unwrap_or(tip));
    [primary, suffixed]
}

/// Preserve a force-deleted branch tip without replacing an earlier archive.
pub(crate) fn delete_local_branch_with_archive(repo: &Path, branch: &str) -> Result<(), String> {
    git_cmd(repo)
        .args(["check-ref-format", "--branch", branch])
        .run()
        .map_err(|error| format!("Invalid local branch name '{branch}': {error}"))?;
    let listed = git_cmd(repo)
        .args(["worktree", "list", "--porcelain"])
        .run()
        .map_err(|error| format!("Cannot verify branch checkouts: {error}"))?;
    if parse_worktree_entries(&listed.stdout)
        .iter()
        .any(|entry| entry.branch.as_deref() == Some(branch))
    {
        return Err(format!(
            "Cannot delete '{branch}': checked out in a worktree"
        ));
    }
    let branch_ref = format!("refs/heads/{branch}");
    let tip = rev_at(repo, &branch_ref)?;
    // A reused branch name collides with the earlier archive. The tip's short
    // SHA names a second archive, so earlier work is never replaced and the new
    // tip is still preserved.
    let archives = archive_refs_for_tip(branch, &tip);
    if archive_ref_at_tip(repo, branch, &tip).is_none() {
        let mut failure = String::new();
        let created = archives.iter().any(|archive| {
            // An empty old value means the archive must not exist.
            match git_cmd(repo).args(["update-ref", archive, &tip, ""]).run() {
                Ok(_) => true,
                Err(error) => {
                    failure = error.to_string();
                    false
                }
            }
        });
        if !created {
            return Err(format!(
                "Cannot preserve '{branch}' at {} or {}: {failure}",
                archives[0], archives[1]
            ));
        }
    }
    git_cmd(repo)
        .args(["update-ref", "-d", &branch_ref, &tip])
        .run()
        .map_err(|error| {
            format!("Cannot delete '{branch}': local ref moved or deletion failed: {error}")
        })?;
    Ok(())
}

/// The archive ref that points at exactly `tip`: the branch's work is
/// preserved. An archive taken before later commits does not qualify.
fn archive_ref_at_tip(repo: &Path, branch_name: &str, tip: &str) -> Option<String> {
    archive_refs_for_tip(branch_name, tip)
        .into_iter()
        .find(|archive| rev_at(repo, archive).is_ok_and(|archived| archived == tip))
}

fn archived_at_tip(repo: &Path, branch_name: &str, tip: &str) -> bool {
    archive_ref_at_tip(repo, branch_name, tip).is_some()
}

fn require_integration_archive(
    repo: &Path,
    branch: &str,
    tip: &str,
    proof: &str,
) -> Result<(), String> {
    if proof == "content_superset" && !archived_at_tip(repo, branch, tip) {
        return Err(format!(
            "Cannot delete '{branch}': content_superset requires {} at the current tip",
            archive_ref_name(branch)
        ));
    }
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct BranchIntegration {
    pub branch: String,
    pub tip: String,
    pub default_branch: String,
    pub commit_status: WorkspaceCommitStatus,
    pub integrated: bool,
    pub proof: Option<&'static str>,
    pub archive_required: bool,
    pub archived: bool,
    pub archive_ref: String,
    pub worktree_paths: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// The query and lifecycle/delete consumers share classify_branch_merge.
pub fn branch_integration_with_pr(
    repo: &Path,
    branch: &str,
    pr_proves_tip: impl Fn(&Path, &str, &str) -> bool,
) -> Result<BranchIntegration, String> {
    if branch.is_empty()
        || git_cmd(repo)
            .args(["check-ref-format", "--branch", branch])
            .run_silent()
            .is_none()
    {
        return Err(format!("Invalid local branch name '{branch}'"));
    }
    let tip = rev_at(repo, &format!("refs/heads/{branch}"))?;
    let default_branch = get_remote_default_branch(&repo.to_string_lossy())?;
    let (commit_status, proof, error) =
        match classify_branch_merge(repo, branch, &tip, &default_branch, pr_proves_tip) {
            Ok((status, proof)) => (status, proof, None),
            Err(error) => (WorkspaceCommitStatus::Unknown, None, Some(error)),
        };
    let listed = git_cmd(repo)
        .args(["worktree", "list", "--porcelain"])
        .run()
        .map_err(|error| format!("Cannot list branch checkouts: {error}"))?;
    let archive_ref = archive_ref_at_tip(repo, branch, &tip);
    Ok(BranchIntegration {
        branch: branch.into(),
        archived: archive_ref.is_some(),
        archive_ref: archive_ref.unwrap_or_else(|| archive_ref_name(branch)),
        tip,
        default_branch,
        integrated: matches!(
            commit_status,
            WorkspaceCommitStatus::Merged | WorkspaceCommitStatus::InSync
        ),
        commit_status,
        proof: proof.or((commit_status == WorkspaceCommitStatus::InSync).then_some("in_sync")),
        archive_required: proof == Some("content_superset"),
        worktree_paths: parse_worktree_entries(&listed.stdout)
            .into_iter()
            .filter(|entry| entry.branch.as_deref() == Some(branch))
            .map(|entry| entry.path)
            .collect(),
        error,
    })
}

/// Branches classified at once. Each one may wait on a GitHub lookup, so a
/// sequential listing of dozens of unmerged branches outlasts the MCP timeout.
const BRANCH_INTEGRATION_WORKERS: usize = 6;

pub fn branch_integrations_with_pr(
    repo: &Path,
    pr_proves_tip: impl Fn(&Path, &str, &str) -> bool + Sync,
) -> Result<Vec<BranchIntegration>, String> {
    let listed = git_cmd(repo)
        .args(["for-each-ref", "--format=%(refname:strip=2)", "refs/heads/"])
        .run()
        .map_err(|error| format!("Cannot list branches: {error}"))?;
    let branches: Vec<&str> = listed.stdout.lines().collect();
    let next = AtomicUsize::new(0);
    let results = std::sync::Mutex::new(Vec::with_capacity(branches.len()));
    std::thread::scope(|scope| {
        for _ in 0..BRANCH_INTEGRATION_WORKERS.min(branches.len()) {
            scope.spawn(|| {
                while let Some(branch) = branches.get(next.fetch_add(1, Ordering::Relaxed)) {
                    let result = branch_integration_with_pr(repo, branch, &pr_proves_tip);
                    results
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .push((branch, result));
                }
            });
        }
    });
    let mut results = results
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    results.sort_by_key(|(branch, _)| branches.iter().position(|listed| listed == *branch));
    results.into_iter().map(|(_, result)| result).collect()
}

/// A linked checkout is never detached or removed by this operation.
pub fn delete_integrated_local_branch(
    repo_path: &str,
    branch_name: &str,
) -> Result<&'static str, String> {
    delete_integrated_local_branch_with_pr(repo_path, branch_name, |_, _, _| false)
}

pub fn delete_integrated_local_branch_with_pr(
    repo_path: &str,
    branch_name: &str,
    pr_proves_tip: impl Fn(&Path, &str, &str) -> bool,
) -> Result<&'static str, String> {
    let repo = Path::new(repo_path);
    if branch_name.is_empty()
        || git_cmd(repo)
            .args(["check-ref-format", "--branch", branch_name])
            .run_silent()
            .is_none()
    {
        return Err(format!("Invalid local branch name '{branch_name}'"));
    }
    let default_branch = get_remote_default_branch(repo_path)?;
    let current_branch = git_cmd(repo)
        .args(["symbolic-ref", "--quiet", "--short", "HEAD"])
        .run()
        .map_err(|error| format!("Cannot determine the current integration branch: {error}"))?
        .stdout
        .trim()
        .to_string();
    if branch_name == current_branch {
        return Err(format!(
            "Cannot delete current integration branch '{branch_name}'"
        ));
    }
    if branch_name == default_branch {
        return Err(format!("Cannot delete default branch '{branch_name}'"));
    }
    let listed = git_cmd(repo)
        .args(["worktree", "list", "--porcelain"])
        .run()
        .map_err(|error| format!("Cannot verify branch checkouts: {error}"))?;
    if parse_worktree_entries(&listed.stdout)
        .iter()
        .any(|entry| entry.branch.as_deref() == Some(branch_name))
    {
        return Err(format!(
            "Cannot delete '{branch_name}': checked out in a worktree"
        ));
    }
    let branch_ref = format!("refs/heads/{branch_name}");
    let tip = rev_at(repo, &branch_ref)
        .map_err(|_| format!("Local branch '{branch_name}' does not exist"))?;
    let (status, merge_proof) =
        classify_branch_merge(repo, branch_name, &tip, &default_branch, pr_proves_tip)?;
    let proof = match status {
        WorkspaceCommitStatus::InSync => "in_sync",
        WorkspaceCommitStatus::Merged => merge_proof
            .ok_or_else(|| format!("Cannot verify merged commits for '{branch_name}'"))?,
        WorkspaceCommitStatus::Unmerged => {
            if archived_at_tip(repo, branch_name, &tip) {
                "archived"
            } else {
                return Err(format!(
                    "Cannot delete '{branch_name}': unmerged commits are not in the default branch (compared against {})",
                    describe_bases(&integration_bases(repo, &default_branch))
                ));
            }
        }
        WorkspaceCommitStatus::PushedUnmerged if archived_at_tip(repo, branch_name, &tip) => {
            "archived"
        }
        WorkspaceCommitStatus::PushedUnmerged => {
            return Err(format!(
                "Cannot delete '{branch_name}': it is pushed but not merged into the default branch"
            ));
        }
        WorkspaceCommitStatus::Unknown => {
            return Err(format!("Cannot verify merged commits for '{branch_name}'"));
        }
    };
    require_integration_archive(repo, branch_name, &tip, proof)?;
    git_cmd(repo)
        .args(["update-ref", "-d", &branch_ref, &tip])
        .run()
        .map_err(|error| {
            format!("Cannot delete '{branch_name}': local ref moved or deletion failed: {error}")
        })?;
    Ok(proof)
}

/// One block of `git worktree list --porcelain` output.
struct WorktreeEntry {
    path: String,
    /// Branch from the `branch refs/heads/...` line — absent while HEAD is detached.
    branch: Option<String>,
    detached: bool,
}

fn parse_worktree_entries(porcelain: &str) -> Vec<WorktreeEntry> {
    let mut entries = Vec::new();

    for block in porcelain.split("\n\n") {
        let block = block.trim();
        if block.is_empty() {
            continue;
        }

        let mut path: Option<String> = None;
        let mut branch: Option<String> = None;
        let mut detached = false;

        for line in block.lines() {
            if let Some(p) = line.strip_prefix("worktree ") {
                path = Some(p.to_string());
            } else if let Some(b) = line.strip_prefix("branch refs/heads/") {
                branch = Some(b.to_string());
            } else if line == "detached" {
                detached = true;
            }
        }

        if let Some(path) = path {
            entries.push(WorktreeEntry {
                path,
                branch,
                detached,
            });
        }
    }

    entries
}

/// Marker files git writes into a worktree's admin dir while a multi-step operation is in
/// flight. Rebase and bisect detach HEAD, so `git worktree list --porcelain` emits no branch
/// line and the worktree reads as dead to anything keyed on that line (GH #112).
const IN_PROGRESS_MARKERS: [&str; 6] = [
    "rebase-merge",
    "rebase-apply",
    "MERGE_HEAD",
    "CHERRY_PICK_HEAD",
    "REVERT_HEAD",
    "BISECT_LOG",
];

/// Admin dir of a linked worktree: its `.git` is a *file* holding
/// `gitdir: <repo>/.git/worktrees/<name>`. Returns `None` for the main worktree (where `.git`
/// is a directory) and for paths that no longer exist.
fn worktree_admin_dir(worktree_path: &str) -> Option<PathBuf> {
    let content = std::fs::read_to_string(Path::new(worktree_path).join(".git")).ok()?;
    let gitdir = content.trim().strip_prefix("gitdir:")?.trim();
    Some(PathBuf::from(gitdir))
}

/// True when the worktree is in the middle of a rebase / merge / cherry-pick / revert / bisect.
fn has_operation_in_progress(worktree_path: &str) -> bool {
    let Some(admin) = worktree_admin_dir(worktree_path) else {
        return false;
    };
    IN_PROGRESS_MARKERS
        .iter()
        .any(|marker| admin.join(marker).exists())
}

/// Branch a detached worktree was on before the in-flight operation started. Git records it in
/// `head-name` for both rebase backends; merge/cherry-pick/revert never detach, so they have no
/// equivalent (and need none). Bisect records only a raw name in `BISECT_START`, which we do not
/// trust as a branch — such a worktree stays alive as an in-progress op, just without a row.
///
/// **Stays path-addressed on purpose (#726-5ac7).** Story 726 asked for this to
/// take a `workspace_id` alongside `worktree_dirtiness` and `check_worktree_dirty`,
/// and that is not implementable: this function is an *input* to id resolution,
/// not a consumer of it. `map_worktree_workspace_paths` calls it to recover the
/// branch of a detached worktree while it is building the id-keyed map, so
/// resolving an id here would need the map that this call is helping construct.
/// It reads git's state files at a directory, which is what it is addressed by.
/// The gix backend's other call site passes a path for the same reason.
pub fn operation_head_branch(worktree_path: &str) -> Option<String> {
    let admin = worktree_admin_dir(worktree_path)?;
    for backend in ["rebase-merge", "rebase-apply"] {
        let head_name = std::fs::read_to_string(admin.join(backend).join("head-name")).ok();
        if let Some(branch) = head_name
            .as_deref()
            .and_then(|s| s.trim().strip_prefix("refs/heads/"))
        {
            return Some(branch.to_string());
        }
    }
    None
}

/// One workspace's checkout, resolved by opaque workspace id.
///
/// `branch` remains ordinary data even though linked worktree ids currently
/// use the branch name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceWorktree {
    /// What is checked out here. Ordinary data: never a key, never parsed out of
    /// the id.
    pub branch: String,
    pub path: String,
    pub kind: WorkspaceKind,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub warm_artifacts: Option<serde_json::Value>,
    /// Merge/dirty/removal verdict, attached by callers that already paid for
    /// `inspect_workspace_lifecycle` per worktree (e.g. the MCP worktree_list
    /// action). Absent, not merely `None`, for callers that never compute it.
    #[serde(skip_serializing_if = "Option::is_none", default, skip_deserializing)]
    pub lifecycle_status: Option<WorkspaceLifecycleStatus>,
}

/// The workspace id a freshly created **git worktree** gets.
///
/// The one place allowed to produce an id from a branch, and only because the
/// identity migration defines it that way: a linked worktree keeps
/// `workspace_id == branch` so nothing persisted moves. Reading it in the other
/// direction is the forbidden move — `resolve_workspace` looks an id up, it
/// never parses one.
///
pub fn workspace_id_of_worktree(branch: &str) -> String {
    branch.to_string()
}

/// Map workspace id -> its checkout. A worktree detached by an in-progress rebase keeps its
/// row: its pre-rebase branch is recovered from git's own state files, so the sidebar entry
/// survives and its terminals are not closed mid-conflict-resolution.
///
/// For a **git worktree** the id is the branch name, because the plan's identity
/// migration is exactly that: existing entries keep `workspace_id = branch`, so
/// no persisted key moves and no id is invented for data that already works.
/// This is not a placeholder — it is the migration.
fn map_worktree_workspace_paths(porcelain: &str) -> HashMap<String, WorkspaceWorktree> {
    let mut result = HashMap::new();

    for entry in parse_worktree_entries(porcelain) {
        let branch = match entry.branch {
            Some(branch) => Some(branch),
            None => operation_head_branch(&entry.path),
        };
        // Skip entries whose directory no longer exists (double safety after prune)
        if let Some(branch) = branch
            && Path::new(&entry.path).exists()
            && Path::new(&entry.path).parent().and_then(Path::file_name)
                != Some(std::ffi::OsStr::new("__archived"))
        {
            result.insert(
                workspace_id_of_worktree(&branch),
                WorkspaceWorktree {
                    branch,
                    path: entry.path.clone(),
                    kind: WorkspaceKind::Worktree,
                    warm_artifacts: None,
                    lifecycle_status: None,
                },
            );
        }
    }

    result
}

/// Get every linked workspace of a repository.
pub fn get_worktree_paths(repo_path: String) -> Result<HashMap<String, WorkspaceWorktree>, String> {
    let mut paths = get_worktree_paths_raw(&repo_path)?;
    attach_warm_statuses(&mut paths);
    Ok(paths)
}

pub fn get_worktree_paths_raw(
    repo_path: &str,
) -> Result<HashMap<String, WorkspaceWorktree>, String> {
    let base_repo = PathBuf::from(&repo_path);
    let output = git_cmd(&base_repo)
        .args(["worktree", "list", "--porcelain"])
        .run()
        .map_err(|error| format!("git worktree list failed: {error}"))?;
    Ok(map_worktree_workspace_paths(&output.stdout))
}

pub fn attach_warm_statuses<S: std::hash::BuildHasher>(
    paths: &mut HashMap<String, WorkspaceWorktree, S>,
) {
    for workspace in paths.values_mut() {
        workspace.warm_artifacts = Some(warm_status(Path::new(&workspace.path)));
    }
}
/// Resolve one workspace by its opaque id.
///
/// This is the single lookup every id-taking operation goes through — removal,
/// dirtiness, branch deletion. Resolving by *branch* instead is the #726-5ac7
/// bug: a branch label and a workspace id answer different questions, and an
/// id-taking operation must not silently select a checkout by its branch.
///
/// Returns the record, so callers that need the branch (deleting the ref,
/// logging) read it off the value rather than assuming it equals the id.
pub fn resolve_workspace(
    base_repo: &Path,
    workspace_id: &str,
) -> Result<WorkspaceWorktree, String> {
    let out = git_cmd(base_repo)
        .args(["worktree", "list", "--porcelain"])
        .run()
        .map_err(|e| format!("git worktree list failed: {e}"))?;

    map_worktree_workspace_paths(&out.stdout)
        .remove(workspace_id)
        .ok_or_else(|| format!("No workspace found for id '{workspace_id}'"))
}

/// Parse `git worktree list --porcelain` output and return paths of linked worktrees that are in
/// detached HEAD state (i.e. their branch has been deleted). The main worktree (first entry) is
/// always skipped — it can't be removed without removing the repo itself. A worktree detached by
/// an in-progress operation is not an orphan: its branch is coming back when the rebase ends.
fn parse_orphan_worktrees(porcelain: &str) -> Vec<String> {
    parse_worktree_entries(porcelain)
        .into_iter()
        .skip(1)
        .filter(|e| e.detached && e.branch.is_none() && !has_operation_in_progress(&e.path))
        .map(|e| e.path)
        .collect()
}

/// Detect orphan worktrees: linked worktrees present on the filesystem but in detached HEAD
/// state (i.e. their branch has been deleted). Returns a list of worktree directory paths.
#[cfg(test)]
pub async fn detect_orphan_worktrees(repo_path: String) -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(move || detect_orphan_worktrees_blocking(repo_path))
        .await
        .map_err(|e| format!("orphan worktree detection task failed: {e}"))?
}

/// Validate that `worktree_path` is a detached orphan of the given repo, not a
/// main or branch checkout or one with a Git operation in progress.
pub fn validate_worktree_path(repo_path: &str, worktree_path: &str) -> Result<(), String> {
    let path = PathBuf::from(worktree_path);
    if !path.is_absolute() {
        return Err("worktree_path must be an absolute path".to_string());
    }

    let out = git_cmd(Path::new(repo_path))
        .args(["worktree", "list", "--porcelain"])
        .run()
        .map_err(|e| format!("git worktree list failed: {e}"))?;

    let entry = parse_worktree_entries(&out.stdout)
        .into_iter()
        .find(|entry| {
            tuic_core::path_spelling::portable_spelling(&entry.path)
                == tuic_core::path_spelling::portable_spelling(worktree_path)
        })
        .ok_or_else(|| {
            format!(
                "Refused: '{}' is not a known worktree of '{}'",
                worktree_path, repo_path
            )
        })?;
    if has_operation_in_progress(worktree_path) {
        return Err("Cannot remove orphan worktree: a Git operation is in progress".into());
    }
    if !entry.detached || entry.branch.is_some() {
        return Err("Cannot remove orphan worktree: checkout is not detached".into());
    }

    Ok(())
}

/// Generate a worktree name (Story 063)
pub fn generate_worktree_name_cmd(existing_names: Vec<String>) -> String {
    generate_worktree_name(&existing_names)
}

/// Generate a hybrid clone branch name: `{sanitized_source}--{random_name}`
pub fn generate_clone_branch_name_cmd(
    source_branch: String,
    existing_names: Vec<String>,
) -> String {
    generate_clone_branch_name(&source_branch, &existing_names)
}

/// List local branch names for a repository (excludes HEAD and remote-only refs)
pub fn list_local_branches(repo_path: String) -> Result<Vec<String>, String> {
    let out = git_cmd(Path::new(&repo_path))
        .args(["branch", "--format=%(refname:short)"])
        .run()
        .map_err(|e| format!("git branch failed: {e}"))?;

    let branches: Vec<String> = out
        .stdout
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    Ok(branches)
}

/// Get the remote default branch for a repo.
///
/// Tries `git symbolic-ref refs/remotes/origin/HEAD` first, then falls back
/// to checking if `main` or `master` exist as local branches.
pub fn get_remote_default_branch(repo_path: &str) -> Result<String, String> {
    // Try symbolic-ref first (cheapest, no network)
    if let Some(out) = git_cmd(Path::new(repo_path))
        .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
        .run_silent()
    {
        let trimmed = out.stdout.trim().to_string();
        // Output is like "refs/remotes/origin/main"
        if let Some(branch) = trimmed.strip_prefix("refs/remotes/origin/")
            && !branch.is_empty()
        {
            return Ok(branch.to_string());
        }
    }

    // Fallback: check if main or master branches exist locally
    let branches = list_local_branches(repo_path.to_string()).unwrap_or_default();
    if branches.iter().any(|b| b == "main") {
        return Ok("main".to_string());
    }
    if branches.iter().any(|b| b == "master") {
        return Ok("master".to_string());
    }

    // Last resort: return "main"
    Ok("main".to_string())
}

/// Fetch a remote ref if the ref name is a remote tracking branch (e.g. "origin/main").
/// Local refs are a no-op. Returns Ok(()) on success or if the ref is local.
pub fn fetch_if_remote(repo_path: &str, ref_name: &str) -> Result<(), String> {
    // A "/" alone does NOT mean remote — local branches routinely contain slashes
    // (e.g. "POC-0001/merge-radar", "feature/foo"). Only fetch when the ref actually
    // resolves as a remote-tracking ref under refs/remotes/.
    let is_remote_ref = git_cmd(Path::new(repo_path))
        .args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/remotes/{ref_name}"),
        ])
        .run_silent()
        .is_some();
    if !is_remote_ref {
        return Ok(());
    }
    if let Some(slash_pos) = ref_name.find('/') {
        let remote = &ref_name[..slash_pos];
        let branch = &ref_name[slash_pos + 1..];
        if !remote.is_empty() && !branch.is_empty() {
            git_cmd(Path::new(repo_path))
                .timeout(FETCH_TIMEOUT)
                .args(["fetch", remote, branch])
                .run()
                .map_err(|e| format!("Failed to fetch {ref_name}: {e}"))?;
        }
    }
    Ok(())
}

/// Persist the base ref for a branch in git config.
/// Stored as `branch.<name>.tuicommander-base` in `.git/config`.
pub fn set_branch_base(repo_path: &str, branch_name: &str, base_ref: &str) -> Result<(), String> {
    let key = format!("branch.{branch_name}.tuicommander-base");
    git_cmd(Path::new(repo_path))
        .args(["config", &key, base_ref])
        .run()
        .map_err(|e| format!("Failed to set branch base: {e}"))?;
    Ok(())
}

/// Read the stored base ref for a branch from git config.
/// Returns None if not set.
pub fn get_branch_base(repo_path: &str, branch_name: &str) -> Option<String> {
    let key = format!("branch.{branch_name}.tuicommander-base");
    git_cmd(Path::new(repo_path))
        .args(["config", &key])
        .run_silent()
        .map(|out| out.stdout.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Read every branch's stored base ref in a single git subprocess.
///
/// Returns a map of branch name -> base ref, parsed from the
/// `branch.<name>.tuicommander-base` config entries. Empty when none are set
/// or the lookup fails. Replaces N sequential per-branch `git config` calls in
/// `apply_base_ahead_behind_and_sort`.
pub fn get_branch_bases(repo_path: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let Some(out) = git_cmd(Path::new(repo_path))
        .args(["config", "--get-regexp", r"^branch\..*\.tuicommander-base$"])
        .run_silent()
    else {
        return map;
    };
    for line in out.stdout.lines() {
        // Each line: `branch.<name>.tuicommander-base <base-ref>`. The key and
        // value are whitespace-separated; the branch name is the middle of the
        // key (may contain dots, so anchor on both prefix and suffix).
        let Some((key, value)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        if let Some(name) = key
            .strip_prefix("branch.")
            .and_then(|k| k.strip_suffix(".tuicommander-base"))
        {
            map.insert(name.to_string(), value.to_string());
        }
    }
    map
}

/// A base ref option with metadata for grouped dropdown display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseRefOption {
    pub name: String,
    /// "local" or "remote"
    pub kind: String,
    /// Whether this is the default branch (e.g. main/master)
    pub is_default: bool,
}

/// List available base ref options for branch/worktree creation.
///
/// Returns structured refs: default branch first (flagged), then local branches,
/// then remote tracking branches. Filters out origin/HEAD and deduplicates
/// where a local branch has the same name as its remote tracking branch.
pub fn list_base_ref_options(repo_path: String) -> Result<Vec<BaseRefOption>, String> {
    let default_branch = get_remote_default_branch(&repo_path)?;
    let repo = Path::new(&repo_path);

    // Get all refs (local + remote) in one git call
    let out = git_cmd(repo)
        .args([
            "for-each-ref",
            "--format=%(refname:short)\t%(refname)",
            "refs/heads/",
            "refs/remotes/",
        ])
        .run()
        .map_err(|e| format!("git for-each-ref failed: {e}"))?;

    let mut local_refs: Vec<BaseRefOption> = Vec::new();
    let mut remote_refs: Vec<BaseRefOption> = Vec::new();
    let mut local_names: std::collections::HashSet<String> = std::collections::HashSet::new();

    for line in out.stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.splitn(2, '\t').collect();
        if parts.len() != 2 {
            continue;
        }

        let short_name = parts[0].to_string();
        let full_ref = parts[1];

        if full_ref.starts_with("refs/heads/") {
            local_names.insert(short_name.clone());
            if short_name != default_branch {
                local_refs.push(BaseRefOption {
                    name: short_name,
                    kind: "local".to_string(),
                    is_default: false,
                });
            }
        } else if full_ref.starts_with("refs/remotes/") {
            // Skip origin/HEAD (synthetic ref)
            if short_name.ends_with("/HEAD") {
                continue;
            }
            remote_refs.push(BaseRefOption {
                name: short_name,
                kind: "remote".to_string(),
                is_default: false,
            });
        }
    }

    // Sort alphabetically within each group
    local_refs.sort_by(|a, b| a.name.cmp(&b.name));
    remote_refs.sort_by(|a, b| a.name.cmp(&b.name));

    // Build result: default first, then local, then remote
    let mut result = Vec::with_capacity(1 + local_refs.len() + remote_refs.len());
    result.push(BaseRefOption {
        name: default_branch,
        kind: "local".to_string(),
        is_default: true,
    });
    result.extend(local_refs);
    result.extend(remote_refs);

    Ok(result)
}

/// Result of a merge-and-archive operation
#[derive(Clone, Serialize)]
pub struct MergeArchiveResult {
    /// Whether the merge succeeded
    pub merged: bool,
    /// What happened to the worktree (archived / deleted / pending user choice /
    /// needs_confirmation — nothing was touched, the caller must confirm)
    pub action: String,
    /// Path to archived directory (if archived)
    pub archive_path: Option<String>,
    /// Commits the branch had that the target did not, measured BEFORE the merge.
    /// 0 means the merge was a no-op ("Already up to date") — the worktree is about
    /// to disappear from the sidebar without contributing anything.
    pub commits_ahead: usize,
    /// Whether the worktree had uncommitted changes at pre-flight time.
    pub worktree_dirty: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch_delete_warning: Option<String>,
}

/// Whether a branch's worktree holds uncommitted work.
///
/// Tri-state on purpose. "We could not tell" is not the same answer as "clean",
/// and when the answer gates an irreversible delete it must not collapse into it.
pub enum WorktreeDirtiness {
    /// The worktree exists and `git status` reported nothing, or the branch has
    /// no worktree at all — either way there is no uncommitted work to lose.
    Clean,
    /// `git status` reported uncommitted work.
    Dirty,
    /// A git command failed, so the question is unanswered. Carries the reason.
    Unknown(String),
}

impl WorktreeDirtiness {
    /// True only when git actually reported uncommitted work.
    pub fn is_dirty(&self) -> bool {
        matches!(self, WorktreeDirtiness::Dirty)
    }

    /// True unless the worktree is known to be clean. An irreversible cleanup
    /// needs a positive answer, and silence is not one.
    fn blocks_cleanup(&self) -> bool {
        !matches!(self, WorktreeDirtiness::Clean)
    }
}

/// Ask git whether the workspace `workspace_id` names has uncommitted work.
///
/// Addressed by id, not branch: this answer gates an irreversible cleanup, so
/// it must inspect the exact checkout the caller intends to remove (#726-5ac7).
///
/// The three outcomes are kept apart deliberately: an id with no checkout has
/// nothing to lose (Clean), while a git command that failed tells us nothing
/// (Unknown). Folding the second into the first is what let a dirty worktree be
/// force-removed on a transient git error.
pub fn worktree_dirtiness(base_repo: &Path, workspace_id: &str) -> WorktreeDirtiness {
    let path = match resolve_any_workspace(base_repo, workspace_id) {
        Ok(workspace) => PathBuf::from(workspace.path),
        Err(error) if error.starts_with("No workspace found") => return WorktreeDirtiness::Clean,
        Err(error) => return WorktreeDirtiness::Unknown(error),
    };

    match dirty_files_at(&path) {
        Ok(0) => WorktreeDirtiness::Clean,
        Ok(_) => WorktreeDirtiness::Dirty,
        Err(e) => WorktreeDirtiness::Unknown(e),
    }
}

/// Shared dirty-state gate for destructive worktree cleanup.
///
/// Both entry points — `merge_and_archive_worktree_impl` and
/// `finalize_merged_worktree_impl` — call this, so the two cleanup paths cannot
/// drift apart on dirtiness. The app layer also checks lifecycle and live
/// sessions before automatic cleanup. `force` records the user's confirmation.
pub fn cleanup_needs_confirmation(action: &str, force: bool, dirt: &WorktreeDirtiness) -> bool {
    let cleans_up = action == "archive" || action == "delete";
    if !cleans_up || force {
        return false;
    }
    if let WorktreeDirtiness::Unknown(reason) = dirt {
        tracing::warn!(
            source = "worktree",
            "Cleanup blocked: could not confirm the worktree is clean ({reason})"
        );
    }
    dirt.blocks_cleanup()
}

/// What the pre-flight learned about a worktree branch before we merge it.
pub struct MergePreflight {
    pub commits_ahead: usize,
    pub worktree_dirty: WorktreeDirtiness,
}

/// Count commits on `branch` that `target` does not have, and check whether the
/// workspace `workspace_id` names has uncommitted changes.
///
/// Two keys because there are two questions: the commit count is about a *branch*
/// and the dirty check is about one exact *checkout*. Conflating them can inspect
/// a different path than the caller intends to clean up.
///
/// This is what tells a real merge apart from an "Already up to date" no-op. Both
/// succeed as far as `git merge` is concerned, but only one of them justifies
/// making the worktree row disappear.
///
/// A failing rev-list is not fatal — the merge was asked for and deserves to run,
/// so the count falls back to 0. The dirty check is different: it gates a delete,
/// so its failure is reported as Unknown rather than swallowed. See
/// `cleanup_needs_confirmation`.
pub fn merge_preflight(
    repo_path: &str,
    branch_name: &str,
    workspace_id: &str,
    target_branch: &str,
) -> MergePreflight {
    let base_repo = Path::new(repo_path);
    let commits_ahead = git_cmd(base_repo)
        .args([
            "rev-list",
            "--count",
            &format!("{target_branch}..{branch_name}"),
        ])
        .run()
        .ok()
        .and_then(|out| out.stdout.trim().parse::<usize>().ok())
        .unwrap_or(0);

    MergePreflight {
        commits_ahead,
        worktree_dirty: worktree_dirtiness(base_repo, workspace_id),
    }
}

/// Archive a worktree: move its directory to `{worktrees_dir}/__archived/{branch_name}/`
/// and repair its Git registration so the archived checkout stays usable.
///
/// If `archive_script` is provided (non-empty), it runs in the worktree directory
/// before archiving. A non-zero exit code aborts the operation.
/// Pick a non-colliding archive destination under `archive_dir` for `sanitized`.
///
/// Returns `archive_dir/sanitized` when free, otherwise the first free
/// `sanitized-2`, `sanitized-3`, … so archiving the same branch twice never
/// clobbers a prior archive. Single-user local tool: a find-first-free-suffix
/// loop is fine, no TOCTOU hardening needed.
fn free_archive_dest(archive_dir: &Path, sanitized: &str) -> PathBuf {
    let base = archive_dir.join(sanitized);
    if !base.exists() {
        return base;
    }
    let mut counter = 2;
    loop {
        let candidate = archive_dir.join(format!("{sanitized}-{counter}"));
        if !candidate.exists() {
            return candidate;
        }
        counter += 1;
    }
}

fn repair_archived_submodules(
    base_repo: &Path,
    worktree: &Path,
    modules: &[(String, String)],
) -> Result<(), String> {
    for (relative, gitdir) in modules {
        let module = worktree.join(relative);
        std::fs::write(module.join(".git"), format!("gitdir: {gitdir}\n"))
            .map_err(|error| format!("Cannot repair submodule {relative} gitfile: {error}"))?;
        git_cmd(base_repo)
            .args([
                "config",
                "--file",
                &Path::new(gitdir).join("config").to_string_lossy(),
                "core.worktree",
                &module.to_string_lossy(),
            ])
            .run()
            .map_err(|error| format!("Cannot repair submodule {relative} worktree: {error}"))?;
    }
    Ok(())
}

pub fn archive_worktree(
    base_repo: &Path,
    workspace_id: &str,
    archive_script: Option<&str>,
) -> Result<String, String> {
    // Resolve the checkout by id — the archive directory name is derived from the
    // record's branch, so a same-branch sibling can never be the one moved away.
    let workspace = resolve_workspace(base_repo, workspace_id)?;
    let wt_path = PathBuf::from(&workspace.path);
    let lock = warm_lock(&wt_path);
    let _warm_guard = lock.as_ref().map(|lock| {
        lock.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    });
    let admin = registered_worktree_admin_dir(base_repo, &wt_path)?
        .ok_or("Cannot archive worktree: its Git registration is missing")?;
    if admin.join("locked").exists() {
        return Err(format!("{LOCKED_WORKTREE_PREFIX}worktree is locked"));
    }
    if !wt_path.exists() {
        return Err("Cannot archive worktree: checkout directory is missing".into());
    }

    // Run archive script before archiving (if configured)
    if let Some(script) = archive_script
        && !script.is_empty()
    {
        run_script_in_dir(script, &wt_path).map_err(|e| format!("Archive script failed: {e}"))?;
    }
    let submodules = git_cmd(&wt_path)
        .args(["submodule", "status", "--recursive"])
        .run()
        .map_err(|error| format!("Cannot inspect submodules before archive: {error}"))?;
    let mut module_gitdirs = Vec::new();
    for line in submodules.stdout.lines() {
        if line.starts_with('-') {
            continue;
        }
        let (_, description) = line[1..]
            .split_once(' ')
            .ok_or_else(|| format!("Cannot parse submodule status before archive: {line}"))?;
        let relative = description
            .rsplit_once(" (")
            .map_or(description, |(path, _)| path);
        let module = wt_path.join(relative);
        module_gitdirs.push((relative.to_string(), rev_at(&module, "--absolute-git-dir")?));
    }
    let parent_dir = wt_path.parent().ok_or("Worktree has no parent directory")?;
    let archive_dir = parent_dir.join("__archived");
    let sanitized = sanitize_name(&workspace.branch);
    std::fs::create_dir_all(&archive_dir)
        .map_err(|e| format!("Failed to create archive directory: {e}"))?;
    // Keep the linked checkout and its Git administration intact. Never clobber
    // an earlier archive of the same branch.
    let archive_dest = free_archive_dest(&archive_dir, &sanitized);
    std::fs::rename(&wt_path, &archive_dest)
        .map_err(|e| format!("Failed to move worktree to archive: {e}"))?;
    let repaired = (|| -> Result<(), String> {
        git_cmd(base_repo)
            .args(["worktree", "repair", &archive_dest.to_string_lossy()])
            .run()
            .map_err(|error| format!("Cannot repair archived worktree: {error}"))?;
        repair_archived_submodules(base_repo, &archive_dest, &module_gitdirs)
    })();
    if let Err(error) = repaired {
        let rollback = (|| -> Result<(), String> {
            std::fs::rename(&archive_dest, &wt_path)
                .map_err(|error| format!("Cannot move checkout back: {error}"))?;
            git_cmd(base_repo)
                .args(["worktree", "repair", &wt_path.to_string_lossy()])
                .run()
                .map_err(|error| format!("Cannot restore worktree registration: {error}"))?;
            repair_archived_submodules(base_repo, &wt_path, &module_gitdirs)
        })();
        return Err(format!(
            "Failed to repair archived worktree: {error}; rollback: {rollback:?}"
        ));
    }
    clear_warm(&wt_path);

    Ok(archive_dest.to_string_lossy().to_string())
}

/// Deadline for a user-supplied worktree script.
///
/// These are the user's own scripts — `pnpm install`, `cargo build`, a cleanup
/// hook — so the deadline is not there to bound how long a build may take. It is
/// there for the script that will never finish: one reading a stdin it can never
/// be given (`apply_no_window` leaves no window to type into, which is what
/// issue #7 reported), or waiting on a lock nobody will release. Fifteen minutes
/// sits above any plausible cold-cache install-and-build, so no real setup dies
/// on it.
const SCRIPT_TIMEOUT: Duration = Duration::from_secs(900);

/// Run `script` through the platform shell in `cwd`, killing it at `timeout`.
///
/// Both callers pass [`SCRIPT_TIMEOUT`]; the parameter is what lets a test drive
/// the kill path without waiting a quarter of an hour for it.
fn run_shell_script(
    script: &str,
    cwd: &Path,
    timeout: Duration,
) -> Result<std::process::Output, String> {
    let (shell, flag) = if cfg!(target_os = "windows") {
        ("cmd", "/C")
    } else {
        ("sh", "-c")
    };

    let mut cmd = std::process::Command::new(shell);
    cmd.arg(flag).arg(script).current_dir(cwd);
    let path = tuic_core::cli::enriched_path();
    #[cfg(windows)]
    let path = tuic_core::cli::which_cli("git")
        .or_else(|| {
            // Windows shell PATH lookup can miss the inherited Git installation.
            // The filesystem fallback must include the executable extension.
            let git = tuic_core::cli::resolve_cli("git.exe");
            Path::new(&git).is_absolute().then_some(git)
        })
        .and_then(|git| {
            Path::new(&git)
                .parent()
                .map(|dir| format!("{};{path}", dir.display()))
        })
        .unwrap_or(path);
    #[cfg(windows)]
    let path = path.replace('/', "\\");
    cmd.env("PATH", path);
    tuic_core::cli::apply_no_window(&mut cmd);
    crate::git_cli::output_with_deadline(&mut cmd, timeout).map_err(|e| match e {
        crate::git_cli::GitError::TimedOut { after } => format!(
            "Script timed out after {:.0}s and was killed",
            after.as_secs_f64()
        ),
        e => format!("Failed to execute script: {e}"),
    })
}

/// Run a shell script in a directory and return an error if it exits non-zero.
///
/// Used by archive/delete operations to run cleanup scripts before the operation.
fn run_script_in_dir(script: &str, cwd: &Path) -> Result<(), String> {
    let output = run_shell_script(script, cwd, SCRIPT_TIMEOUT)?;

    let exit_code = output.status.code().unwrap_or(-1);
    if exit_code != 0 {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "Script failed with exit code {exit_code}: {stderr}"
        ));
    }
    Ok(())
}

/// Run a shell script in a given directory and return exit code + output.
///
/// Used to execute setup/run scripts after worktree creation.
/// The script is passed to `sh -c` (Unix) or `cmd /C` (Windows).
pub fn run_setup_script(script: String, cwd: String) -> Result<serde_json::Value, String> {
    let cwd = tuic_core::cli::expand_tilde(&cwd);
    let cwd_path = Path::new(&cwd);
    if !cwd_path.exists() {
        return Err(format!("Working directory does not exist: {cwd}"));
    }

    let output = run_shell_script(&script, cwd_path, SCRIPT_TIMEOUT)?;

    Ok(serde_json::json!({
        "exit_code": output.status.code().unwrap_or(-1),
        "stdout": String::from_utf8_lossy(&output.stdout),
        "stderr": String::from_utf8_lossy(&output.stderr),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::{
        base_branch_of, dirty_worktree_with, setup_test_repo, worktree_with,
    };
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;
    use tuic_test_support::{fail_with_stderr_script, print_file_script, touch_script};

    // Catches: lossy-name recovery deleting another branch's dirty or clean registered checkout.
    #[test]
    fn stale_recovery_preserves_registered_name_collisions_and_detached_head() {
        for (first, second, dirty) in [("feat/x", "feat-x", true), ("Feat.X", "feat-x", false)] {
            let repo = setup_test_repo();
            let dir = repo.path().join("worktrees");
            let mut config = WorktreeConfig {
                task_name: first.to_owned(),
                base_repo: repo.path().to_string_lossy().into_owned(),
                branch: Some(first.to_owned()),
                create_branch: true,
            };
            let original =
                create_worktree_with_stale_recovery(&dir, &config, None).expect("first worktree");
            if dirty {
                fs::write(original.path.join("uncommitted.txt"), "keep me").expect("write");
            }
            config.task_name = second.to_owned();
            config.branch = Some(second.to_owned());
            assert!(
                create_worktree_with_stale_recovery(&dir, &config, None).is_err(),
                "{first} collides with {second}"
            );
            assert_eq!(
                crate::git::read_branch_from_head(&original.path).as_deref(),
                Some(first)
            );
            if dirty {
                assert_eq!(
                    fs::read_to_string(original.path.join("uncommitted.txt")).expect("preserved"),
                    "keep me"
                );
            }
            git_cmd(&original.path)
                .args(["checkout", "--detach"])
                .run()
                .expect("detach");
            create_worktree_with_stale_recovery(&dir, &config, None)
                .expect("detached checkout preserved");
            assert!(original.path.join(".git").is_file());
            assert!(crate::git::read_branch_from_head(&original.path).is_none());
            assert!(cleanup_stale_worktree_dir(&config.base_repo, &original.path).is_err());
        }
    }

    // Catches: mistaking a non-Git orphan directory for an existing detached checkout.
    #[test]
    fn stale_recovery_recreates_plain_orphan_directory() {
        let repo = setup_test_repo();
        let dir = repo.path().join("worktrees");
        let orphan = dir.join("orphan");
        fs::create_dir_all(&orphan).expect("orphan dir");
        let config = WorktreeConfig {
            task_name: "orphan".to_owned(),
            base_repo: repo.path().to_string_lossy().into_owned(),
            branch: Some("orphan".to_owned()),
            create_branch: true,
        };
        let recovered =
            create_worktree_with_stale_recovery(&dir, &config, None).expect("recover orphan");
        assert!(recovered.path.join(".git").is_file());
        assert_eq!(
            crate::git::read_branch_from_head(&recovered.path).as_deref(),
            Some("orphan")
        );
    }

    #[test]
    fn test_sanitize_name() {
        assert_eq!(sanitize_name("my-task"), "my-task");
        assert_eq!(sanitize_name("My Task Name"), "my-task-name");
        assert_eq!(sanitize_name("task/with/slashes"), "task-with-slashes");
        assert_eq!(
            sanitize_name("task_with_underscores"),
            "task_with_underscores"
        );
        assert_eq!(sanitize_name("UPPERCASE"), "uppercase");
        assert_eq!(sanitize_name("special!@#chars"), "special---chars");
    }

    #[test]
    fn test_create_worktree() {
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");

        let config = WorktreeConfig {
            task_name: "test-task".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: None,
            create_branch: false,
        };

        let result = create_worktree_internal(&worktrees_dir, &config, None);
        assert!(result.is_ok(), "Failed to create worktree: {:?}", result);

        let worktree = result.unwrap();
        assert_eq!(worktree.name, "test-task");
        assert!(worktree.path.exists(), "Worktree path should exist");
    }

    #[test]
    fn test_create_worktree_with_new_branch() {
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");

        let config = WorktreeConfig {
            task_name: "feature-branch-task".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: Some("feature/new-feature".to_string()),
            create_branch: true,
        };

        let result = create_worktree_internal(&worktrees_dir, &config, None);
        assert!(
            result.is_ok(),
            "Failed to create worktree with branch: {:?}",
            result
        );

        let worktree = result.unwrap();
        assert_eq!(worktree.branch, Some("feature/new-feature".to_string()));
    }

    /// Regression (story 120-797d): a branch/ref beginning with `-` — e.g. an
    /// attacker-chosen PR head_ref like `--upload-pack=...` — must reach
    /// `git worktree add` as DATA, never be parsed as a git OPTION. The `--`
    /// end-of-options guard turns an injection attempt into a plain
    /// "invalid reference" failure instead of "unknown option".
    #[test]
    fn test_create_worktree_dash_ref_treated_as_data() {
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");

        let config = WorktreeConfig {
            task_name: "dash-ref-task".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            // Checkout of an existing ref (create_branch = false) whose name looks
            // like a git option — the classic argument-injection payload.
            branch: Some("--upload-pack=touch /tmp/pwned".to_string()),
            create_branch: false,
        };

        let result = create_worktree_internal(&worktrees_dir, &config, None);
        let err = result.expect_err("worktree add on a nonexistent dash-ref must fail");

        // Without `--`, git parses it as an option ("unknown option"/"unknown switch").
        // With the guard, git resolves it as a ref and fails "invalid reference".
        assert!(
            !err.contains("unknown option") && !err.contains("unknown switch"),
            "dash-prefixed ref was parsed as a git OPTION, not data: {err}"
        );
        assert!(
            err.contains("invalid reference"),
            "expected git to reject the ref as data (invalid reference), got: {err}"
        );
    }

    #[test]
    fn test_create_worktree_idempotent() {
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");

        let config = WorktreeConfig {
            task_name: "idempotent-task".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: None,
            create_branch: false,
        };

        // Create twice - should not fail
        let result1 = create_worktree_internal(&worktrees_dir, &config, None);
        assert!(result1.is_ok());

        let result2 = create_worktree_internal(&worktrees_dir, &config, None);
        assert!(result2.is_ok(), "Second create should succeed (idempotent)");

        // Both should return same path
        assert_eq!(result1.unwrap().path, result2.unwrap().path);
    }

    #[test]
    fn test_create_worktree_stale_dir_returns_stale_error() {
        // Scenario: directory exists but is checked out on a DIFFERENT branch
        // than the one requested → create_worktree_internal must return STALE_DIR error.
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");

        // Create branch-a worktree first
        let config_a = WorktreeConfig {
            task_name: "shared-name".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: Some("branch-a".to_string()),
            create_branch: true,
        };
        create_worktree_internal(&worktrees_dir, &config_a, None)
            .expect("Failed to create branch-a worktree");

        // Now attempt to create at same path but with branch-b → should be STALE_DIR
        let config_b = WorktreeConfig {
            task_name: "shared-name".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: Some("branch-b".to_string()),
            create_branch: true,
        };
        let result = create_worktree_internal(&worktrees_dir, &config_b, None);

        assert!(result.is_err(), "expected STALE_DIR error, got Ok");
        let err = result.unwrap_err();
        assert!(
            err.starts_with("STALE_DIR:"),
            "expected STALE_DIR prefix, got: {err}"
        );
        assert!(
            err.contains("branch-a"),
            "expected actual branch 'branch-a' in error: {err}"
        );
        assert!(
            err.contains("branch-b"),
            "expected expected branch 'branch-b' in error: {err}"
        );
    }

    #[test]
    fn test_create_worktree_same_branch_is_idempotent() {
        // Scenario: directory exists with the SAME branch → should succeed (idempotent)
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");

        let config = WorktreeConfig {
            task_name: "same-task".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: Some("feature/x".to_string()),
            create_branch: true,
        };
        let first = create_worktree_internal(&worktrees_dir, &config, None)
            .expect("First create should succeed");

        // Second call with same branch should succeed and return same path with actual branch
        let second = create_worktree_internal(&worktrees_dir, &config, None)
            .expect("Second create should succeed (idempotent same-branch)");

        assert_eq!(first.path, second.path);
        assert_eq!(second.branch, Some("feature/x".to_string()));
    }

    #[test]
    fn test_classify_worktree_add_failure() {
        // Branch collision must win over the broad "already exists" substring.
        assert_eq!(
            classify_worktree_add_failure("fatal: a branch named 'feature/x' already exists"),
            WorktreeAddFailure::BranchExists
        );
        // Path already exists.
        assert_eq!(
            classify_worktree_add_failure("fatal: '/tmp/wt/foo' already exists"),
            WorktreeAddFailure::PathExists
        );
        // Already checked out by another worktree.
        assert_eq!(
            classify_worktree_add_failure(
                "fatal: 'feature/x' is already checked out at '/tmp/wt/foo'"
            ),
            WorktreeAddFailure::PathExists
        );
        // Already used by worktree.
        assert_eq!(
            classify_worktree_add_failure(
                "fatal: '/tmp/wt/foo' is already used by worktree at '/tmp/wt/bar'"
            ),
            WorktreeAddFailure::PathExists
        );
        // Unrelated failure.
        assert_eq!(
            classify_worktree_add_failure("fatal: invalid reference: nope"),
            WorktreeAddFailure::Other
        );
    }

    #[test]
    fn test_create_worktree_orphan_branch_no_worktree_recovers() {
        // The confirmed bug: a branch exists but has NO linked worktree (left over
        // after worktree_remove with delete_branch=false). A fresh create for that
        // branch hits "a branch named 'X' already exists". The fix must NOT swallow
        // this as a phantom Ok — it must either produce a REAL worktree or Err,
        // but never return Ok with a non-existent path.
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");

        // 1. Create branch B with a worktree.
        let config = WorktreeConfig {
            task_name: "orphan-task".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: Some("orphan-branch".to_string()),
            create_branch: true,
        };
        let wt = create_worktree_internal(&worktrees_dir, &config, None)
            .expect("First create should succeed");

        // 2. Remove the worktree but PRESERVE the branch (delete_branch=false path).
        remove_worktree_internal(&wt, true).expect("remove should succeed");
        assert!(
            !wt.path.exists(),
            "worktree dir should be gone after removal"
        );
        // Branch still exists (we never deleted it).

        // 3. Create again for the same branch → `-b` fails "branch already exists".
        let result = create_worktree_internal(&worktrees_dir, &config, None);

        // Invariant: never Ok with a missing path.
        match result {
            Ok(info) => {
                assert!(
                    info.path.exists(),
                    "create returned Ok but worktree path does not exist: {}",
                    info.path.display()
                );
                // It must be a REAL linked worktree checked out on the branch.
                assert_eq!(
                    crate::git::read_branch_from_head(&info.path).as_deref(),
                    Some("orphan-branch"),
                    "recovered worktree should be on the existing branch"
                );
            }
            Err(e) => {
                // Failing loud is acceptable; silently-Ok-with-no-dir is not.
                assert!(!e.is_empty(), "error must carry git stderr context");
            }
        }
    }

    #[test]
    fn test_create_worktree_detached_head_is_not_stale() {
        // Scenario: worktree exists for `feature/x` but its HEAD is detached
        // (mid-rebase, bisect, or `git checkout <sha>`). A subsequent
        // create_worktree_internal call with the same branch must NOT return
        // STALE_DIR and destroy the in-progress work.
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");

        let config = WorktreeConfig {
            task_name: "agent-task".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: Some("feature/x".to_string()),
            create_branch: true,
        };
        let wt = create_worktree_internal(&worktrees_dir, &config, None)
            .expect("Failed to create worktree");

        // Detach HEAD inside the worktree (simulates rebase/bisect)
        git_cmd(&wt.path)
            .args(["checkout", "--detach"])
            .run()
            .expect("detach failed");

        let result = create_worktree_internal(&worktrees_dir, &config, None);
        assert!(
            result.is_ok(),
            "Detached HEAD must not trigger STALE_DIR: {result:?}"
        );
        let returned = result.unwrap();
        assert_eq!(returned.path, wt.path);
        // Detached HEAD is NOT stale; branch field falls back to the logical
        // owner (config.branch) so the JS layer's `string`-typed contract holds
        // even when the worktree's HEAD is transiently detached.
        assert_eq!(
            returned.branch,
            Some("feature/x".to_string()),
            "branch should fall back to config.branch on detached HEAD, got {:?}",
            returned.branch
        );
    }

    #[test]
    fn test_remove_worktree() {
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");

        let config = WorktreeConfig {
            task_name: "to-be-removed".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: None,
            create_branch: false,
        };

        let worktree = create_worktree_internal(&worktrees_dir, &config, None)
            .expect("Failed to create worktree");

        assert!(
            worktree.path.exists(),
            "Worktree should exist before removal"
        );

        let result = remove_worktree_internal(&worktree, false);
        assert!(result.is_ok(), "Failed to remove worktree: {:?}", result);

        assert!(
            !worktree.path.exists(),
            "Worktree path should not exist after removal"
        );
    }

    #[test]
    fn interrupted_git_removal_clears_the_owned_checkout_without_deleting_its_branch() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let path = add_worktree(&repo, "interrupted-removal");
        let worktree = WorktreeInfo {
            name: "interrupted-removal".into(),
            path: path.clone(),
            branch: Some("interrupted-removal".into()),
            base_repo: repo.clone(),
        };
        begin_warm(&path);

        // Reproduce the persisted state observed after Git reported
        // "Directory not empty": registration is gone, but checkout files remain.
        git_cmd(&repo)
            .args(["worktree", "remove", &path.to_string_lossy()])
            .run()
            .unwrap();
        fs::create_dir(&path).unwrap();
        fs::write(path.join("remaining-tracked-file"), "old checkout\n").unwrap();

        remove_worktree_internal(&worktree, false).unwrap();
        assert!(
            !path.exists(),
            "an interrupted removal must not leave an orphan"
        );
        assert!(
            git_cmd(&repo)
                .args(["show-ref", "--verify", "refs/heads/interrupted-removal"])
                .run()
                .is_ok(),
            "directory cleanup alone must keep the branch ref"
        );
    }

    #[test]
    fn interrupted_removal_does_not_delete_a_new_checkout_at_the_same_path() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let path = add_worktree(&repo, "replacement-checkout");
        let worktree = WorktreeInfo {
            name: "replacement-checkout".into(),
            path: path.clone(),
            branch: Some("replacement-checkout".into()),
            base_repo: repo.clone(),
        };
        begin_warm(&path);
        git_cmd(&repo)
            .args(["worktree", "remove", &path.to_string_lossy()])
            .run()
            .unwrap();
        fs::create_dir(&path).unwrap();
        fs::write(path.join(".git"), "replacement marker\n").unwrap();
        fs::write(path.join("keep.txt"), "new checkout\n").unwrap();

        assert!(remove_worktree_internal(&worktree, false).is_err());
        assert_eq!(
            fs::read_to_string(path.join("keep.txt")).unwrap(),
            "new checkout\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn interrupted_removal_reports_a_broken_link_at_the_checkout_path() {
        use std::os::unix::fs::symlink;

        let (_temp, repo, _workspaces) = workspace_fixture();
        let path = add_worktree(&repo, "broken-link-removal");
        let worktree = WorktreeInfo {
            name: "broken-link-removal".into(),
            path: path.clone(),
            branch: Some("broken-link-removal".into()),
            base_repo: repo.clone(),
        };
        begin_warm(&path);
        git_cmd(&repo)
            .args(["worktree", "remove", &path.to_string_lossy()])
            .run()
            .unwrap();
        symlink(repo.join("target-does-not-exist"), &path).unwrap();

        let result = remove_worktree_internal(&worktree, false);
        assert!(
            result.is_err() || fs::symlink_metadata(&path).is_err(),
            "a successful removal must not leave a broken link at the checkout path"
        );
    }

    #[cfg(unix)]
    #[test]
    fn removing_a_worktree_with_a_sealed_ignored_tree_never_leaves_a_partial_checkout() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let (temp, repo, _workspaces) = workspace_fixture();
        let path = add_worktree(&repo, "sealed-build-removal");
        commit_file(&path, ".gitignore", "target/\n");
        let sealed = path.join("target/build/evidence");
        fs::create_dir_all(&sealed).unwrap();
        fs::write(sealed.join("result.txt"), "sealed evidence\n").unwrap();
        fs::set_permissions(&sealed, fs::Permissions::from_mode(0o555)).unwrap();
        let outside = temp.path().join("outside-sealed.txt");
        fs::write(&outside, "outside evidence\n").unwrap();
        fs::set_permissions(&outside, fs::Permissions::from_mode(0o444)).unwrap();
        symlink(&outside, path.join("target/build/outside-link")).unwrap();
        let worktree = WorktreeInfo {
            name: "sealed-build-removal".into(),
            path: path.clone(),
            branch: Some("sealed-build-removal".into()),
            base_repo: repo.clone(),
        };

        let result = remove_worktree_internal(&worktree, false);
        let registered = registered_worktree_admin_dir(&repo, &path)
            .unwrap()
            .is_some();
        if sealed.exists() {
            fs::set_permissions(&sealed, fs::Permissions::from_mode(0o755)).unwrap();
        }

        assert!(result.is_ok(), "sealed worktree removal failed: {result:?}");
        assert!(!path.exists(), "successful removal left a worktree remnant");
        assert!(!registered, "successful removal left Git registration");
        assert_eq!(fs::read_to_string(&outside).unwrap(), "outside evidence\n");
        assert_eq!(
            fs::metadata(&outside).unwrap().permissions().mode() & 0o777,
            0o444
        );
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_ignored_tree_refuses_removal_before_git_unregisters_it() {
        use std::os::unix::fs::PermissionsExt;

        let (_temp, repo, _workspaces) = workspace_fixture();
        let path = add_worktree(&repo, "unreadable-build-removal");
        commit_file(&path, ".gitignore", "target/\n");
        let unreadable = path.join("target/build/evidence");
        fs::create_dir_all(&unreadable).unwrap();
        fs::write(unreadable.join("result.txt"), "keep me\n").unwrap();
        fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000)).unwrap();
        let worktree = WorktreeInfo {
            name: "unreadable-build-removal".into(),
            path: path.clone(),
            branch: Some("unreadable-build-removal".into()),
            base_repo: repo.clone(),
        };

        let result = remove_worktree_internal(&worktree, false);
        fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o700)).unwrap();

        let error = result.expect_err("unreadable build output must refuse removal");
        assert!(
            error.contains(&path.to_string_lossy().to_string()),
            "{error}"
        );
        assert!(path.exists(), "refusal must retain the checkout");
        assert!(
            registered_worktree_admin_dir(&repo, &path)
                .unwrap()
                .is_some()
        );
        assert_eq!(
            fs::read_to_string(unreadable.join("result.txt")).unwrap(),
            "keep me\n"
        );
    }

    #[test]
    fn test_remove_nonexistent_worktree() {
        let repo = setup_test_repo();

        let worktree = WorktreeInfo {
            name: "nonexistent".to_string(),
            path: repo.path().join("worktrees").join("nonexistent"),
            branch: None,
            base_repo: repo.path().to_path_buf(),
        };

        // Should not error when removing non-existent worktree
        let result = remove_worktree_internal(&worktree, false);
        assert!(
            result.is_ok(),
            "Removing nonexistent worktree should succeed"
        );
    }

    #[test]
    fn removing_one_worktree_keeps_another_missing_worktree_module_registration() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        add_populated_submodule(&repo);
        let missing = add_worktree(&repo, "missing-neighbor");
        let removed = add_worktree(&repo, "removed-neighbor");
        git_cmd(&missing)
            .args([
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "update",
                "--init",
            ])
            .run()
            .unwrap();
        let module = missing.join("modules/local");
        git_cmd(&module)
            .args(["config", "user.email", "test@test.com"])
            .run()
            .unwrap();
        git_cmd(&module)
            .args(["config", "user.name", "Test"])
            .run()
            .unwrap();
        commit_file(&module, "neighbor-only.txt", "neighbor commit\n");
        let oid = rev_at(&module, "HEAD").unwrap();
        let module_gitdir = PathBuf::from(rev_at(&module, "--absolute-git-dir").unwrap());
        assert!(module_gitdir.exists());
        let admin = registered_worktree_admin_dir(&repo, &missing)
            .unwrap()
            .unwrap();
        fs::remove_dir_all(&missing).unwrap();
        let worktree = WorktreeInfo {
            name: "removed-neighbor".into(),
            path: removed,
            branch: Some("removed-neighbor".into()),
            base_repo: repo,
        };
        remove_worktree_internal(&worktree, false).unwrap();
        assert!(
            admin.exists(),
            "unrelated missing worktree registration was pruned"
        );
        assert!(
            module_gitdir.exists(),
            "unpreserved module gitdir was pruned"
        );
        assert!(
            git_cmd(&worktree.base_repo)
                .args([
                    "--git-dir",
                    &module_gitdir.to_string_lossy(),
                    "--work-tree",
                    &worktree.base_repo.to_string_lossy(),
                    "cat-file",
                    "-e",
                    &oid,
                ])
                .run()
                .is_ok(),
            "module-only commit was lost"
        );
    }

    #[test]
    fn refused_dirty_removal_preserves_pending_warm_token() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let path = add_worktree(&repo, "dirty-pending-warm");
        let token = begin_warm(&path);
        fs::write(path.join("dirty.txt"), "uncommitted\n").unwrap();
        let worktree = WorktreeInfo {
            name: "dirty-pending-warm".into(),
            path: path.clone(),
            branch: Some("dirty-pending-warm".into()),
            base_repo: repo,
        };
        let error = remove_worktree_internal(&worktree, false).unwrap_err();
        assert!(error.contains("uncommitted changes"), "{error}");
        assert!(path.exists());
        assert!(warm_token_is_current(&path, token));
        clear_warm(&path);
    }

    #[test]
    fn test_remove_locked_worktree_without_force_returns_locked_error() {
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");

        let config = WorktreeConfig {
            task_name: "locked-branch".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: None,
            create_branch: false,
        };

        let worktree = create_worktree_internal(&worktrees_dir, &config, None)
            .expect("Failed to create worktree");

        // Lock the worktree simulating an active agent
        Command::new("git")
            .current_dir(repo.path())
            .args([
                "worktree",
                "lock",
                "--reason",
                "claude agent test-lock",
                worktree.path.to_str().unwrap(),
            ])
            .output()
            .expect("git worktree lock failed");

        let result = remove_worktree_internal(&worktree, false);
        assert!(
            result.is_err(),
            "Should fail on locked worktree without force"
        );
        let err = result.unwrap_err();
        assert!(
            err.starts_with(LOCKED_WORKTREE_PREFIX),
            "Error should start with LOCKED_WORKTREE_PREFIX, got: {err}"
        );
        assert!(
            worktree.path.exists(),
            "Worktree directory should still exist after failed removal"
        );
    }

    #[test]
    fn test_remove_locked_worktree_with_force_still_refuses_lock() {
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");

        let config = WorktreeConfig {
            task_name: "locked-branch-force".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: None,
            create_branch: false,
        };

        let worktree = create_worktree_internal(&worktrees_dir, &config, None)
            .expect("Failed to create worktree");

        // Lock the worktree
        Command::new("git")
            .current_dir(repo.path())
            .args([
                "worktree",
                "lock",
                "--reason",
                "claude agent force-test",
                worktree.path.to_str().unwrap(),
            ])
            .output()
            .expect("git worktree lock failed");

        let error = remove_worktree_internal(&worktree, true).unwrap_err();
        assert!(error.starts_with(LOCKED_WORKTREE_PREFIX), "{error}");
        assert!(worktree.path.exists());
    }

    #[test]
    fn explicit_lock_override_removes_a_locked_clean_worktree() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let path = add_worktree(&repo, "locked-override");
        git_cmd(&repo)
            .args(["worktree", "lock", &path.to_string_lossy()])
            .run()
            .unwrap();

        let outcome = remove_worktree_by_workspace_id_with_lock(
            &repo.to_string_lossy(),
            "locked-override",
            true,
            None,
            false,
            true,
        )
        .unwrap();

        assert!(!path.exists());
        assert!(outcome.branch_delete_warning.is_none());
    }

    #[test]
    fn test_remove_main_worktree_returns_main_prefix_error() {
        let repo = setup_test_repo();

        // The main worktree IS the repo path itself — git refuses to remove it
        let main_worktree = WorktreeInfo {
            name: "main".to_string(),
            path: repo.path().to_path_buf(),
            branch: Some("main".to_string()),
            base_repo: repo.path().to_path_buf(),
        };

        let result = remove_worktree_internal(&main_worktree, false);
        assert!(result.is_err(), "Removing main worktree should fail");
        let err = result.unwrap_err();
        assert!(
            err.starts_with(MAIN_WORKTREE_PREFIX),
            "Error should start with MAIN_WORKTREE_PREFIX, got: {err}"
        );
    }

    #[test]
    fn test_remove_worktree_by_workspace_id_safe_delete_preserves_unmerged_worktree() {
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");

        let config = WorktreeConfig {
            task_name: "feat-unmerged".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: Some("feat-unmerged".to_string()),
            create_branch: true,
        };
        let wt = create_worktree_internal(&worktrees_dir, &config, None)
            .expect("Failed to create worktree");

        // Add an unmerged commit on the branch
        std::fs::write(wt.path.join("new.txt"), "unmerged work").unwrap();
        git_cmd(&wt.path).args(["add", "."]).run().unwrap();
        git_cmd(&wt.path)
            .args(["commit", "-m", "unmerged change"])
            .run()
            .unwrap();

        let error = remove_worktree_by_workspace_id(
            repo.path().to_str().unwrap(),
            "feat-unmerged",
            true,
            None,
            false,
        )
        .expect_err("unmerged branch must block removal before touching the worktree");
        assert!(error.contains("unmerged"), "{error}");
        assert!(wt.path.exists(), "worktree dir must be preserved");

        let branches = git_cmd(repo.path())
            .args(["branch", "--list", "feat-unmerged"])
            .run()
            .unwrap();
        assert!(
            branches.stdout.contains("feat-unmerged"),
            "branch ref should survive safe delete on unmerged branch (got: {})",
            branches.stdout
        );
    }

    #[test]
    fn force_removal_keeps_an_unmerged_branch_with_a_warning() {
        // Force may discard checkout changes, but it does not grant permission
        // to delete commits that have not reached the default branch.
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");

        let config = WorktreeConfig {
            task_name: "feat-force".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: Some("feat-force".to_string()),
            create_branch: true,
        };
        let wt = create_worktree_internal(&worktrees_dir, &config, None)
            .expect("Failed to create worktree");

        std::fs::write(wt.path.join("new.txt"), "unmerged work").unwrap();
        git_cmd(&wt.path).args(["add", "."]).run().unwrap();
        git_cmd(&wt.path)
            .args(["commit", "-m", "unmerged change"])
            .run()
            .unwrap();

        let res = remove_worktree_by_workspace_id(
            repo.path().to_str().unwrap(),
            "feat-force",
            true,
            None,
            true,
        );
        let outcome = res.expect("force remove should succeed");
        assert!(
            outcome
                .branch_delete_warning
                .as_deref()
                .is_some_and(|w| w.contains("unmerged")),
            "{outcome:?}"
        );

        let branches = git_cmd(repo.path())
            .args(["branch", "--list", "feat-force"])
            .run()
            .unwrap();
        assert!(
            branches.stdout.contains("feat-force"),
            "unmerged branch ref must survive (got: {})",
            branches.stdout
        );
    }

    #[test]
    fn test_worktree_name_with_special_characters() {
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");

        let config = WorktreeConfig {
            task_name: "Fix bug #123: Add feature!".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: None,
            create_branch: false,
        };

        let result = create_worktree_internal(&worktrees_dir, &config, None);
        assert!(result.is_ok());

        let worktree = result.unwrap();
        assert_eq!(worktree.name, "fix-bug--123--add-feature-");
        assert!(worktree.path.exists());
    }

    #[test]
    fn generate_clone_branch_name_includes_source() {
        let existing: Vec<String> = vec![];
        let name = generate_clone_branch_name("feat/auth-flow", &existing);
        assert!(
            name.starts_with("feat-auth-flow--"),
            "Name should start with sanitized source branch: {name}"
        );
        // Should contain a random part after the double-dash
        let parts: Vec<&str> = name.splitn(2, "--").collect();
        assert_eq!(parts.len(), 2, "Should have source--random format: {name}");
        assert!(!parts[1].is_empty(), "Random part should not be empty");
    }

    #[test]
    fn generate_clone_branch_name_avoids_collisions() {
        // Pre-populate with one name and verify a different one is generated
        let first = generate_clone_branch_name("main", &[]);
        let second = generate_clone_branch_name("main", std::slice::from_ref(&first));
        assert_ne!(first, second, "Should generate unique names");
    }

    #[test]
    fn get_remote_default_branch_from_test_repo() {
        let repo = setup_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();
        // Test repo has no remote, so should fall back to checking local branches.
        // git init creates "master" or "main" depending on config.
        let result = get_remote_default_branch(&repo_path);
        assert!(result.is_ok());
        let branch = result.unwrap();
        // Should be "main" or "master" (depends on git version default)
        assert!(
            branch == "main" || branch == "master",
            "Expected main or master, got: {branch}"
        );
    }

    #[test]
    fn list_base_ref_options_returns_default_first() {
        let repo = setup_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();

        // Create a second branch
        git_cmd(repo.path())
            .args(["branch", "feature-x"])
            .run()
            .expect("Failed to create branch");

        let refs = list_base_ref_options(repo_path).unwrap();
        assert!(refs.len() >= 2, "Expected at least 2 refs, got: {refs:?}");
        // First entry should be the default branch (main or master), flagged is_default
        assert!(
            refs[0].name == "main" || refs[0].name == "master",
            "First ref should be default branch, got: {}",
            refs[0].name
        );
        assert!(refs[0].is_default, "First ref should have is_default=true");
        assert_eq!(refs[0].kind, "local");
        // feature-x should be in the list
        let names: Vec<&str> = refs.iter().map(|r| r.name.as_str()).collect();
        assert!(
            names.contains(&"feature-x"),
            "feature-x not found in {names:?}"
        );
        // No duplicate names
        let unique: std::collections::HashSet<&str> = names.iter().copied().collect();
        assert_eq!(unique.len(), refs.len(), "Duplicate refs found: {names:?}");
    }

    // --- merge pre-flight ---
    //
    // The whole point: `git merge` succeeds identically for a branch carrying 7
    // commits and for one carrying none ("Already up to date"), and in both cases
    // the worktree row then disappears from the sidebar. Only the pre-flight can
    // tell the user which of the two just happened.

    #[test]
    fn merge_preflight_counts_commits_the_target_is_missing() {
        let repo = setup_test_repo();
        let base = base_branch_of(repo.path());
        worktree_with(repo.path(), "feat-ahead", true);

        let pf = merge_preflight(
            &repo.path().to_string_lossy(),
            "feat-ahead",
            "feat-ahead",
            &base,
        );
        assert_eq!(pf.commits_ahead, 1, "one commit the base branch lacks");
        assert!(
            matches!(pf.worktree_dirty, WorktreeDirtiness::Clean),
            "everything was committed"
        );
    }

    #[test]
    fn merge_preflight_reports_zero_for_a_branch_with_nothing_to_merge() {
        let repo = setup_test_repo();
        let base = base_branch_of(repo.path());
        worktree_with(repo.path(), "feat-empty", false);

        let pf = merge_preflight(
            &repo.path().to_string_lossy(),
            "feat-empty",
            "feat-empty",
            &base,
        );
        assert_eq!(
            pf.commits_ahead, 0,
            "branch was cut from base and never committed — merging it is a no-op"
        );
    }

    #[test]
    fn merge_preflight_sees_uncommitted_work_in_the_worktree() {
        let repo = setup_test_repo();
        let base = base_branch_of(repo.path());
        let wt = worktree_with(repo.path(), "feat-dirty", false);
        fs::write(wt.join("scratch.txt"), "not committed yet").expect("write");

        let pf = merge_preflight(
            &repo.path().to_string_lossy(),
            "feat-dirty",
            "feat-dirty",
            &base,
        );
        assert_eq!(pf.commits_ahead, 0);
        assert!(
            matches!(pf.worktree_dirty, WorktreeDirtiness::Dirty),
            "an untracked file still counts as work that archiving would sweep away"
        );
    }

    #[test]
    fn merge_preflight_is_clean_for_an_already_merged_branch() {
        let repo = setup_test_repo();
        let base = base_branch_of(repo.path());
        worktree_with(repo.path(), "feat-merged", true);
        git_cmd(repo.path())
            .args(["merge", "feat-merged", "--no-edit"])
            .run()
            .expect("merge");

        let pf = merge_preflight(
            &repo.path().to_string_lossy(),
            "feat-merged",
            "feat-merged",
            &base,
        );
        assert_eq!(
            pf.commits_ahead, 0,
            "already merged — the target has everything"
        );
    }

    #[test]
    fn merge_preflight_falls_back_to_unknown_for_a_branch_that_does_not_exist() {
        let repo = setup_test_repo();
        let base = base_branch_of(repo.path());
        // rev-list fails on an unknown ref; the pre-flight must not panic or block
        // the merge — it is a guard rail, not a gate.
        let pf = merge_preflight(
            &repo.path().to_string_lossy(),
            "no-such-branch",
            "no-such-branch",
            &base,
        );
        assert_eq!(pf.commits_ahead, 0);
        assert!(
            matches!(pf.worktree_dirty, WorktreeDirtiness::Clean),
            "no worktree at all means there is no uncommitted work to lose"
        );
    }

    #[test]
    fn an_unanswered_dirty_check_blocks_the_cleanup() {
        // Failing open here is what let a transient git error wipe a worktree.
        let unknown = WorktreeDirtiness::Unknown("git exploded".to_string());
        assert!(cleanup_needs_confirmation("archive", false, &unknown));
        assert!(cleanup_needs_confirmation("delete", false, &unknown));
        assert!(
            !unknown.is_dirty(),
            "reported as not-known-dirty: the field must not claim more than git said"
        );
    }

    #[test]
    fn the_gate_only_guards_the_destructive_actions() {
        let dirty = WorktreeDirtiness::Dirty;
        assert!(cleanup_needs_confirmation("archive", false, &dirty));
        assert!(cleanup_needs_confirmation("delete", false, &dirty));
        // "ask" and anything else remove nothing, so there is nothing to confirm.
        assert!(!cleanup_needs_confirmation("ask", false, &dirty));
        assert!(!cleanup_needs_confirmation("keep", false, &dirty));
        // force is the confirmation itself.
        assert!(!cleanup_needs_confirmation("delete", true, &dirty));
        assert!(!cleanup_needs_confirmation(
            "archive",
            false,
            &WorktreeDirtiness::Clean
        ));
    }

    #[test]
    fn archive_worktree_moves_directory() {
        let repo = setup_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();
        let worktrees_dir = repo.path().join("worktrees");

        // Create a worktree with a branch
        let config = WorktreeConfig {
            task_name: "feat-archive-test".to_string(),
            base_repo: repo_path.clone(),
            branch: Some("feat-archive-test".to_string()),
            create_branch: true,
        };
        let wt = create_worktree_internal(&worktrees_dir, &config, None)
            .expect("Failed to create worktree");
        assert!(wt.path.exists(), "Worktree should exist");

        // Make a commit on the feature branch so merge has something to do
        fs::write(wt.path.join("feature.txt"), "feature work").expect("write feature");
        git_cmd(&wt.path).args(["add", "."]).run().expect("git add");
        git_cmd(&wt.path)
            .args(["commit", "-m", "feat: add feature"])
            .run()
            .expect("git commit");
        let head = rev_at(&wt.path, "HEAD").unwrap();
        let reflog = git_cmd(&wt.path)
            .args(["reflog", "show", "--format=%H", "-1", "HEAD"])
            .run()
            .unwrap()
            .stdout;

        // Archive the worktree
        let result = archive_worktree(repo.path(), "feat-archive-test", None);
        assert!(result.is_ok(), "Archive should succeed: {:?}", result);

        let archive_path = PathBuf::from(result.unwrap());
        // The worktree should no longer exist at original location
        assert!(!wt.path.exists(), "Original worktree path should be gone");
        assert!(archive_path.exists());
        assert_eq!(rev_at(&archive_path, "HEAD").unwrap(), head);
        assert_eq!(
            git_cmd(&archive_path)
                .args(["reflog", "show", "--format=%H", "-1", "HEAD"])
                .run()
                .unwrap()
                .stdout,
            reflog
        );
        assert!(
            !get_worktree_paths_raw(&repo_path)
                .unwrap()
                .contains_key("feat-archive-test")
        );
        git_cmd(repo.path())
            .args(["worktree", "prune", "--expire", "now"])
            .run()
            .unwrap();
        assert_eq!(rev_at(&archive_path, "HEAD").unwrap(), head);
    }

    #[test]
    fn archive_refuses_a_locked_worktree_without_touching_it() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let path = add_worktree(&repo, "archive-locked");
        git_cmd(&repo)
            .args(["worktree", "lock", &path.to_string_lossy()])
            .run()
            .unwrap();
        let admin = registered_worktree_admin_dir(&repo, &path)
            .unwrap()
            .expect("worktree registration");

        let error = archive_worktree(&repo, "archive-locked", None).unwrap_err();
        assert!(error.starts_with(LOCKED_WORKTREE_PREFIX), "{error}");
        assert!(path.exists());
        assert!(admin.join("locked").exists());
        assert!(!path.parent().unwrap().join("__archived").exists());
    }

    #[test]
    fn archiving_a_worktree_preserves_a_missing_neighbors_module_only_commit() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        add_populated_submodule(&repo);
        let missing = add_worktree(&repo, "missing-archive-neighbor");
        let archived = add_worktree(&repo, "archived-neighbor");
        git_cmd(&missing)
            .args([
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "update",
                "--init",
            ])
            .run()
            .unwrap();
        let module = missing.join("modules/local");
        git_cmd(&module)
            .args(["config", "user.email", "test@test.com"])
            .run()
            .unwrap();
        git_cmd(&module)
            .args(["config", "user.name", "Test"])
            .run()
            .unwrap();
        commit_file(&module, "neighbor-only.txt", "neighbor commit\n");
        let oid = rev_at(&module, "HEAD").unwrap();
        let module_gitdir = PathBuf::from(rev_at(&module, "--absolute-git-dir").unwrap());
        let admin = registered_worktree_admin_dir(&repo, &missing)
            .unwrap()
            .expect("missing neighbor registration");
        assert!(module_gitdir.exists());
        assert!(
            git_cmd(&repo.join("modules/local"))
                .args(["cat-file", "-e", &oid])
                .run()
                .is_err(),
            "commit must exist only in the missing neighbor's module"
        );
        fs::remove_dir_all(&missing).unwrap();

        let destination = archive_worktree(&repo, "archived-neighbor", None).unwrap();
        assert!(Path::new(&destination).exists());
        assert!(!archived.exists());
        assert!(admin.exists(), "archive pruned the unrelated registration");
        assert!(
            module_gitdir.exists(),
            "archive pruned the module repository"
        );
        git_cmd(&repo)
            .args([
                "--git-dir",
                &module_gitdir.to_string_lossy(),
                "--work-tree",
                &repo.to_string_lossy(),
                "cat-file",
                "-e",
                &oid,
            ])
            .run()
            .expect("module-only commit remains reachable after archive");
    }

    #[test]
    fn archived_submodule_keeps_its_local_commit_and_ref() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        add_populated_submodule(&repo);
        let path = add_worktree(&repo, "archive-module");
        git_cmd(&path)
            .args([
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "update",
                "--init",
            ])
            .run()
            .unwrap();
        let module = path.join("modules/local");
        git_cmd(&module)
            .args(["config", "user.email", "test@test.com"])
            .run()
            .unwrap();
        git_cmd(&module)
            .args(["config", "user.name", "Test"])
            .run()
            .unwrap();
        commit_file(&module, "local-only.txt", "local commit\n");
        let local_oid = rev_at(&module, "HEAD").unwrap();
        let module_reflog = git_cmd(&module)
            .args(["reflog", "show", "--format=%H", "-1", "HEAD"])
            .run()
            .unwrap()
            .stdout;
        git_cmd(&module)
            .args(["branch", "local-archive-ref", &local_oid])
            .run()
            .unwrap();

        let archived = PathBuf::from(archive_worktree(&repo, "archive-module", None).unwrap());
        let archived_module = archived.join("modules/local");
        assert_eq!(rev_at(&archived_module, "HEAD").unwrap(), local_oid);
        assert_eq!(
            git_cmd(&archived_module)
                .args(["reflog", "show", "--format=%H", "-1", "HEAD"])
                .run()
                .unwrap()
                .stdout,
            module_reflog
        );
        assert!(
            git_cmd(&archived_module)
                .args(["status", "--porcelain"])
                .run()
                .is_ok()
        );
        assert!(
            git_cmd(&archived_module)
                .args(["cat-file", "-e", &local_oid])
                .run()
                .is_ok()
        );
        assert!(
            git_cmd(&archived_module)
                .args(["for-each-ref", "--points-at", &local_oid])
                .run()
                .unwrap()
                .stdout
                .lines()
                .next()
                .is_some()
        );
    }

    #[test]
    fn remove_worktree_by_workspace_id_deletes_branch_when_true() {
        let repo = setup_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();
        let worktrees_dir = repo.path().join("worktrees");

        // Create a worktree with a new branch
        let config = WorktreeConfig {
            task_name: "feat-delete-branch".to_string(),
            base_repo: repo_path.clone(),
            branch: Some("feat-delete-branch".to_string()),
            create_branch: true,
        };
        create_worktree_internal(&worktrees_dir, &config, None).expect("Failed to create worktree");

        // Remove with delete_branch=true
        let outcome =
            remove_worktree_by_workspace_id(&repo_path, "feat-delete-branch", true, None, false)
                .expect("Failed to remove worktree");
        assert_eq!(outcome.removal_rule, "in_sync");

        // Branch should be gone
        let out = git_cmd(repo.path())
            .args(["branch", "--list", "feat-delete-branch"])
            .run()
            .expect("Failed to list branches");
        assert!(
            out.stdout.trim().is_empty(),
            "Branch should be deleted when delete_branch=true, but found: {}",
            out.stdout
        );
    }

    #[test]
    fn remove_worktree_by_workspace_id_keeps_branch_when_false() {
        let repo = setup_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();
        let worktrees_dir = repo.path().join("worktrees");

        // Create a worktree with a new branch
        let config = WorktreeConfig {
            task_name: "feat-keep-branch".to_string(),
            base_repo: repo_path.clone(),
            branch: Some("feat-keep-branch".to_string()),
            create_branch: true,
        };
        create_worktree_internal(&worktrees_dir, &config, None).expect("Failed to create worktree");

        // Remove with delete_branch=false
        remove_worktree_by_workspace_id(&repo_path, "feat-keep-branch", false, None, false)
            .expect("Failed to remove worktree");

        // Branch should still exist
        let out = git_cmd(repo.path())
            .args(["branch", "--list", "feat-keep-branch"])
            .run()
            .expect("Failed to list branches");
        assert!(
            !out.stdout.trim().is_empty(),
            "Branch should be preserved when delete_branch=false"
        );
    }

    #[test]
    fn parse_orphan_worktrees_detects_detached_linked_worktrees() {
        let porcelain = "\
worktree /repo/main
HEAD abc123
branch refs/heads/main

worktree /wt/feat-auth
HEAD def456
branch refs/heads/feat-auth

worktree /wt/orphan
HEAD deadbeef
detached

";
        let orphans = super::parse_orphan_worktrees(porcelain);
        assert_eq!(orphans, vec!["/wt/orphan"]);
    }

    #[test]
    fn parse_orphan_worktrees_ignores_main_worktree_even_if_detached() {
        let porcelain = "\
worktree /repo/main
HEAD abc123
detached

worktree /wt/also-detached
HEAD deadbeef
detached

";
        // Main worktree (first) is always skipped; only the second shows up
        let orphans = super::parse_orphan_worktrees(porcelain);
        assert_eq!(orphans, vec!["/wt/also-detached"]);
    }

    #[test]
    fn parse_orphan_worktrees_returns_empty_when_all_have_branches() {
        let porcelain = "\
worktree /repo/main
HEAD abc123
branch refs/heads/main

worktree /wt/feat
HEAD def456
branch refs/heads/feat

";
        let orphans = super::parse_orphan_worktrees(porcelain);
        assert!(orphans.is_empty());
    }

    #[tokio::test]
    async fn detect_orphan_worktrees_runs_as_async_command() {
        let repo = TempDir::new().expect("temp repo");
        let status = Command::new("git")
            .args(["init", "--quiet"])
            .arg(repo.path())
            .status()
            .expect("run git init");
        assert!(status.success());

        let orphans = super::detect_orphan_worktrees(repo.path().display().to_string())
            .await
            .expect("detect orphan worktrees");

        assert!(orphans.is_empty());
    }

    /// Build a linked-worktree fixture: `<root>/wt` with a `.git` file pointing at
    /// `<root>/admin`, plus whichever in-progress marker files the test needs.
    fn linked_worktree_fixture(root: &Path, markers: &[(&str, &str)]) -> String {
        let wt = root.join("wt");
        let admin = root.join("admin");
        std::fs::create_dir_all(&wt).expect("worktree dir");
        std::fs::create_dir_all(&admin).expect("admin dir");
        std::fs::write(wt.join(".git"), format!("gitdir: {}\n", admin.display()))
            .expect("gitdir file");
        for (rel, contents) in markers {
            let target = admin.join(rel);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).expect("marker parent");
            }
            std::fs::write(target, contents).expect("marker file");
        }
        wt.to_string_lossy().into_owned()
    }

    fn detached_porcelain(wt_path: &str) -> String {
        format!(
            "worktree /repo/main\nHEAD abc123\nbranch refs/heads/main\n\nworktree {wt_path}\nHEAD deadbeef\ndetached\n\n"
        )
    }

    /// Two workspaces may share a branch (#726-5ac7), so a lookup keyed on the
    /// branch cannot say WHICH one you asked for. Resolution is by opaque
    /// workspace id, and the branch travels as a field on the value.
    ///
    /// For a git worktree the id is the branch. Keeping the id explicit avoids
    /// coupling callers to that representation.
    #[test]
    fn workspace_paths_are_keyed_by_id_and_carry_the_branch() {
        // A real directory: the mapper drops entries whose path no longer exists,
        // which is the post-prune safety guard, not something to work around.
        let dir = TempDir::new().expect("temp dir");
        let wt = dir.path().to_string_lossy().into_owned();
        let porcelain = format!("worktree {wt}\nHEAD abc123\nbranch refs/heads/main\n\n");

        let map = super::map_worktree_workspace_paths(&porcelain);

        let entry = map.get("main").expect("resolvable by workspace id");
        assert_eq!(entry.path, wt);
        assert_eq!(
            entry.branch, "main",
            "branch survives as data, not as the key"
        );
        assert_eq!(entry.kind, crate::worktree::WorkspaceKind::Worktree);
    }

    #[test]
    fn branch_already_owned_by_a_linked_checkout_is_refused() {
        let repo = setup_test_repo();
        let branch = git_cmd(repo.path())
            .args(["branch", "--show-current"])
            .run()
            .expect("current branch")
            .stdout;

        let error = super::ensure_branch_has_no_workspace(repo.path(), branch.trim())
            .expect_err("main checkout already owns its branch");
        assert!(
            error.contains("already belongs to linked worktree"),
            "{error}"
        );
        assert!(error.contains("reuse that worktree"), "{error}");
    }

    /// Resolution remains keyed by workspace id while the branch travels as
    /// data on the record. The load-bearing case is the detached worktree:
    /// mid-rebase git emits no `branch refs/heads/…` line at
    /// all, so a scan for that line could not find it by branch under any
    /// argument. The id-keyed mapper recovers the branch from git's own
    /// `head-name` and keys on it, so the workspace stays resolvable — which is
    /// what stops the sidebar row vanishing and its terminals being closed
    /// mid-conflict-resolution.
    #[test]
    fn resolution_by_workspace_id_returns_that_workspace_not_a_branch_match() {
        let repo = setup_test_repo();
        let alpha = worktree_with(repo.path(), "feat-alpha", false);
        let beta = worktree_with(repo.path(), "feat-beta", false);

        let resolved_alpha =
            super::resolve_workspace(repo.path(), "feat-alpha").expect("alpha resolves");
        let resolved_beta =
            super::resolve_workspace(repo.path(), "feat-beta").expect("beta resolves");
        assert_eq!(
            std::fs::canonicalize(&resolved_alpha.path).expect("canonical alpha"),
            std::fs::canonicalize(&alpha).expect("canonical alpha dir"),
        );
        assert_eq!(
            std::fs::canonicalize(&resolved_beta.path).expect("canonical beta"),
            std::fs::canonicalize(&beta).expect("canonical beta dir"),
        );
        assert_ne!(
            resolved_alpha.path, resolved_beta.path,
            "each id must land on its own directory"
        );

        // An id is opaque: a directory path is not one, and must not resolve.
        assert!(
            super::resolve_workspace(repo.path(), &alpha.to_string_lossy()).is_err(),
            "a path is not a workspace id"
        );
        assert!(
            super::resolve_workspace(repo.path(), "no-such-workspace").is_err(),
            "an unknown id is an error, not a silent first-match"
        );
    }

    /// Criterion: removing one workspace leaves the other resolvable and on disk.
    #[test]
    fn removing_one_workspace_leaves_its_sibling_resolvable_and_on_disk() {
        let repo = setup_test_repo();
        let doomed = worktree_with(repo.path(), "feat-doomed", false);
        let survivor = worktree_with(repo.path(), "feat-survivor", false);

        let outcome = super::remove_worktree_by_workspace_id(
            &repo.path().to_string_lossy(),
            "feat-doomed",
            true,
            None,
            false,
        )
        .expect("removal by id");
        assert_eq!(
            outcome.branch, "feat-doomed",
            "the outcome reports the branch it read off the record"
        );

        assert!(!doomed.exists(), "the targeted worktree is gone");
        assert!(survivor.exists(), "the sibling is untouched on disk");
        let still_there =
            super::resolve_workspace(repo.path(), "feat-survivor").expect("sibling resolves");
        assert_eq!(
            std::fs::canonicalize(&still_there.path).expect("canonical"),
            std::fs::canonicalize(&survivor).expect("canonical"),
        );
        assert!(
            super::resolve_workspace(repo.path(), "feat-doomed").is_err(),
            "the removed id stops resolving"
        );
    }

    /// Criterion: `worktree_dirtiness` and `check_worktree_dirty` answer about
    /// the workspace they were asked about.
    ///
    /// This is the one that gates an irreversible cleanup, so it must never
    /// report one workspace clean because another workspace is clean.
    #[test]
    fn dirtiness_answers_about_the_workspace_it_was_asked_about() {
        let repo = setup_test_repo();
        let dirty = dirty_worktree_with(repo.path(), "feat-dirty-one", false);
        worktree_with(repo.path(), "feat-clean-one", false);
        assert!(dirty.join("scratch.txt").exists(), "fixture is dirty");

        assert!(
            super::worktree_dirtiness(repo.path(), "feat-dirty-one").is_dirty(),
            "the dirty workspace reports dirty"
        );
        assert!(
            !super::worktree_dirtiness(repo.path(), "feat-clean-one").is_dirty(),
            "the clean sibling is not tainted by it"
        );

        let repo_str = repo.path().to_string_lossy().into_owned();
        assert_eq!(
            check_worktree_dirty(repo_str.clone(), "feat-dirty-one".to_string()),
            Ok(true)
        );
        assert_eq!(
            check_worktree_dirty(repo_str.clone(), "feat-clean-one".to_string()),
            Ok(false)
        );
        // An id with no checkout has nothing to lose — Clean, not an error.
        assert_eq!(
            check_worktree_dirty(repo_str, "no-such-workspace".to_string()),
            Ok(false)
        );
    }

    /// Criterion: `delete_local_branch_impl` refuses when its id and branch name
    /// disagree, instead of guessing which one the caller meant.
    #[test]
    fn delete_local_branch_refuses_an_id_branch_mismatch() {
        let repo = setup_test_repo();
        let keeper = worktree_with(repo.path(), "feat-keeper", false);
        worktree_with(repo.path(), "feat-other", false);
        let repo_str = repo.path().to_string_lossy().into_owned();

        let err = delete_local_branch_impl(&repo_str, "feat-other", "feat-keeper", false)
            .expect_err("mismatched id and branch must be refused");
        assert!(
            err.contains("feat-keeper") && err.contains("feat-other"),
            "the refusal must name both, got: {err}"
        );
        assert!(
            keeper.exists(),
            "nothing was destroyed while the request was ambiguous"
        );
        let branches = git_cmd(repo.path())
            .args(["branch", "--list", "feat-other"])
            .run()
            .expect("branch list");
        assert!(
            branches.stdout.contains("feat-other"),
            "the branch the caller named still exists"
        );
    }

    #[test]
    fn worktree_mid_rebase_is_not_orphan_and_keeps_its_branch() {
        let dir = TempDir::new().expect("temp dir");
        let wt = linked_worktree_fixture(
            dir.path(),
            &[("rebase-merge/head-name", "refs/heads/feat-auth\n")],
        );
        let porcelain = detached_porcelain(&wt);

        assert!(super::parse_orphan_worktrees(&porcelain).is_empty());
        assert_eq!(
            super::map_worktree_workspace_paths(&porcelain)
                .get("feat-auth")
                .map(|w| &w.path),
            Some(&wt)
        );
    }

    #[test]
    fn worktree_mid_rebase_apply_is_not_orphan_and_keeps_its_branch() {
        let dir = TempDir::new().expect("temp dir");
        let wt = linked_worktree_fixture(
            dir.path(),
            &[("rebase-apply/head-name", "refs/heads/feat-am\n")],
        );
        let porcelain = detached_porcelain(&wt);

        assert!(super::parse_orphan_worktrees(&porcelain).is_empty());
        assert_eq!(
            super::map_worktree_workspace_paths(&porcelain)
                .get("feat-am")
                .map(|w| &w.path),
            Some(&wt)
        );
    }

    #[test]
    fn interrupted_merge_and_cherry_pick_are_not_orphans() {
        // Merge and cherry-pick never detach HEAD, but a worktree that is BOTH detached and
        // mid-operation (e.g. a cherry-pick started from a detached HEAD) must not be archived.
        for marker in [
            "MERGE_HEAD",
            "CHERRY_PICK_HEAD",
            "REVERT_HEAD",
            "BISECT_LOG",
        ] {
            let dir = TempDir::new().expect("temp dir");
            let wt = linked_worktree_fixture(dir.path(), &[(marker, "deadbeef\n")]);
            assert!(
                super::parse_orphan_worktrees(&detached_porcelain(&wt)).is_empty(),
                "{marker} should suppress the orphan verdict"
            );
        }
    }

    #[test]
    fn genuinely_orphaned_worktree_is_still_reported() {
        let dir = TempDir::new().expect("temp dir");
        // Same fixture, no in-progress marker: the branch really is gone.
        let wt = linked_worktree_fixture(dir.path(), &[]);
        let porcelain = detached_porcelain(&wt);

        assert_eq!(super::parse_orphan_worktrees(&porcelain), vec![wt.clone()]);
        assert!(
            !super::map_worktree_workspace_paths(&porcelain)
                .values()
                .any(|w| w.path == wt)
        );
    }

    #[test]
    fn delete_local_branch_removes_bare_branch() {
        let repo = setup_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();

        // Create a branch from current HEAD
        git_cmd(repo.path())
            .args(["branch", "feat-to-delete"])
            .run()
            .expect("Failed to create branch");

        // Verify it exists
        let branches = list_local_branches(repo_path.clone()).unwrap();
        assert!(branches.contains(&"feat-to-delete".to_string()));

        // Delete it
        let result =
            delete_local_branch_impl(&repo_path, "feat-to-delete", "feat-to-delete", false);
        assert!(
            result.is_ok(),
            "delete_local_branch_impl failed: {:?}",
            result
        );

        // Verify it's gone
        let branches = list_local_branches(repo_path).unwrap();
        assert!(!branches.contains(&"feat-to-delete".to_string()));
    }

    #[test]
    fn delete_local_branch_refuses_default_branch() {
        let repo = setup_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();

        let default_branch = get_remote_default_branch(&repo_path).unwrap();
        let result = delete_local_branch_impl(&repo_path, &default_branch, &default_branch, false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("Refusing to delete default branch")
        );
    }

    #[test]
    fn delete_local_branch_with_worktree() {
        let repo = setup_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();
        let worktrees_dir = repo.path().join("worktrees");

        // Create a worktree with a new branch
        let config = WorktreeConfig {
            task_name: "wt-to-delete".to_string(),
            base_repo: repo_path.clone(),
            branch: None,
            create_branch: false,
        };
        let wt = create_worktree_internal(&worktrees_dir, &config, None)
            .expect("Failed to create worktree");

        // Verify worktree exists
        assert!(wt.path.exists());

        // Delete via delete_local_branch_impl (default cascade: keep_worktree = false)
        let result = delete_local_branch_impl(&repo_path, &wt.name, &wt.name, false);
        assert!(
            result.is_ok(),
            "delete_local_branch_impl failed: {:?}",
            result
        );

        // Worktree directory should be removed
        assert!(!wt.path.exists(), "Worktree directory should be gone");
    }

    /// Regression test for the "Keep worktree" bug.
    ///
    /// PostMergeCleanupDialog lets the user uncheck the "Archive/Delete worktree"
    /// step (intent: keep the worktree on disk) while leaving the "Delete local
    /// branch" step checked. With `keep_worktree = true`, `delete_local_branch_impl`
    /// must detach the worktree HEAD and remove only the branch ref, leaving the
    /// worktree directory and its files intact.
    #[test]
    fn delete_local_branch_should_preserve_worktree_when_user_keeps_it() {
        let repo = setup_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();
        let worktrees_dir = repo.path().join("worktrees");

        let config = WorktreeConfig {
            task_name: "wt-keep".to_string(),
            base_repo: repo_path.clone(),
            branch: None,
            create_branch: false,
        };
        let wt = create_worktree_internal(&worktrees_dir, &config, None)
            .expect("Failed to create worktree");
        assert!(wt.path.exists(), "precondition: worktree should exist");

        // Simulates PostMergeCleanupDialog flow with the worktree step
        // unchecked but delete-local checked. `keep_worktree = true` must
        // detach the worktree HEAD and remove only the branch ref.
        let result = delete_local_branch_impl(&repo_path, &wt.name, &wt.name, true);
        assert!(
            result.is_ok(),
            "delete_local_branch_impl with keep_worktree=true failed: {:?}",
            result
        );

        assert!(
            wt.path.exists(),
            "Worktree directory was deleted despite keep_worktree=true"
        );

        // Branch ref must be gone
        let branches = list_local_branches(repo_path).unwrap();
        assert!(
            !branches.contains(&wt.name),
            "Branch ref should have been deleted"
        );
    }

    #[test]
    fn check_worktree_dirty_clean_worktree() {
        let repo = setup_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();
        let worktrees_dir = repo.path().join("worktrees");

        let config = WorktreeConfig {
            task_name: "clean-wt".to_string(),
            base_repo: repo_path.clone(),
            branch: None,
            create_branch: false,
        };
        let wt = create_worktree_internal(&worktrees_dir, &config, None)
            .expect("Failed to create worktree");

        let dirty = check_worktree_dirty(repo_path, wt.name);
        assert!(dirty.is_ok());
        assert!(!dirty.unwrap(), "Clean worktree should not be dirty");
    }

    #[test]
    fn check_worktree_dirty_with_uncommitted_changes() {
        let repo = setup_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();
        let worktrees_dir = repo.path().join("worktrees");

        let config = WorktreeConfig {
            task_name: "dirty-wt".to_string(),
            base_repo: repo_path.clone(),
            branch: None,
            create_branch: false,
        };
        let wt = create_worktree_internal(&worktrees_dir, &config, None)
            .expect("Failed to create worktree");

        // Add an uncommitted file in the worktree
        fs::write(wt.path.join("dirty.txt"), "uncommitted").expect("Failed to write dirty file");

        let dirty = check_worktree_dirty(repo_path, wt.name);
        assert!(dirty.is_ok());
        assert!(
            dirty.unwrap(),
            "Worktree with uncommitted changes should be dirty"
        );
    }

    #[test]
    fn check_worktree_dirty_no_worktree() {
        let repo = setup_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();

        // Create a branch without a worktree
        git_cmd(repo.path())
            .args(["branch", "bare-branch"])
            .run()
            .expect("Failed to create branch");

        let dirty = check_worktree_dirty(repo_path, "bare-branch".to_string());
        assert!(dirty.is_ok());
        assert!(
            !dirty.unwrap(),
            "Branch without worktree should not be dirty"
        );
    }

    #[test]
    fn run_setup_script_success() {
        let dir = TempDir::new().expect("temp dir");
        let cwd = dir.path().to_string_lossy().to_string();

        let result = run_setup_script("echo hello".to_string(), cwd).expect("should succeed");
        assert_eq!(result["exit_code"], 0);
        assert_eq!(result["stdout"].as_str().unwrap().trim(), "hello");
        assert_eq!(result["stderr"].as_str().unwrap(), "");
    }

    /// A setup script that never finishes must be killed at its deadline, and
    /// the caller must be told so rather than getting a plausible-looking
    /// exit code. `sleep` stands in for the real cases: a script blocked on a
    /// stdin it can never be given, or on a lock nobody releases.
    #[cfg(unix)]
    #[test]
    fn run_shell_script_gives_up_at_the_deadline() {
        let dir = TempDir::new().expect("temp dir");

        let started = std::time::Instant::now();
        let err = run_shell_script("sleep 30", dir.path(), Duration::from_millis(300))
            .expect_err("a script that never finishes must fail");
        let waited = started.elapsed();

        assert!(
            err.contains("timed out"),
            "expected a timeout error, got: {err}"
        );
        assert!(
            waited < Duration::from_secs(5),
            "must not wait for the script; waited {waited:?}"
        );
    }

    /// The control: a script that finishes inside its deadline is not truncated.
    #[test]
    fn run_shell_script_keeps_output_of_a_script_that_finishes_in_time() {
        let dir = TempDir::new().expect("temp dir");

        let out = run_shell_script("echo alive", dir.path(), Duration::from_secs(30))
            .expect("a fast script must succeed");
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "alive");
    }

    #[test]
    fn run_setup_script_failure() {
        let dir = TempDir::new().expect("temp dir");
        let cwd = dir.path().to_string_lossy().to_string();

        let result = run_setup_script("exit 42".to_string(), cwd)
            .expect("should return result even on non-zero exit");
        assert_eq!(result["exit_code"], 42);
    }

    #[test]
    fn run_setup_script_captures_stderr() {
        let dir = TempDir::new().expect("temp dir");
        let cwd = dir.path().to_string_lossy().to_string();

        let result = run_setup_script(fail_with_stderr_script("oops", 1), cwd)
            .expect("should return result");
        assert_eq!(result["exit_code"], 1);
        assert_eq!(result["stderr"].as_str().unwrap().trim(), "oops");
    }

    #[test]
    fn run_setup_script_invalid_cwd() {
        let result = run_setup_script("echo hi".to_string(), "/nonexistent/path/xyz".to_string());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not exist"));
    }

    #[test]
    fn run_setup_script_runs_in_cwd() {
        let dir = TempDir::new().expect("temp dir");
        fs::write(dir.path().join("marker.txt"), "found").expect("write marker");
        let cwd = dir.path().to_string_lossy().to_string();

        let result =
            run_setup_script(print_file_script("marker.txt"), cwd).expect("should succeed");
        assert_eq!(result["exit_code"], 0);
        assert_eq!(result["stdout"].as_str().unwrap().trim(), "found");
    }

    #[test]
    fn run_script_in_dir_succeeds_with_zero_exit() {
        let dir = TempDir::new().expect("temp dir");
        let result = run_script_in_dir("echo hello", dir.path());
        assert!(result.is_ok());
    }

    #[test]
    fn run_script_in_dir_fails_with_nonzero_exit() {
        let dir = TempDir::new().expect("temp dir");
        let result = run_script_in_dir("exit 1", dir.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("exit code 1"));
    }

    #[test]
    fn run_script_in_dir_runs_in_correct_directory() {
        let dir = TempDir::new().expect("temp dir");
        fs::write(dir.path().join("test-file.txt"), "content").expect("write");
        let result = run_script_in_dir(&print_file_script("test-file.txt"), dir.path());
        assert!(result.is_ok());
    }

    #[test]
    fn archive_worktree_runs_archive_script_before_archiving() {
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");
        let config = WorktreeConfig {
            task_name: "archive-script-test".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: None,
            create_branch: false,
        };
        let _wt = create_worktree_internal(&worktrees_dir, &config, None).expect("create worktree");
        // Script creates a marker file inside the worktree dir; archive should still succeed
        let marker = worktrees_dir.join("archive-marker.txt");
        let script = touch_script(&marker.display().to_string());
        let result = archive_worktree(repo.path(), "archive-script-test", Some(&script));
        assert!(
            result.is_ok(),
            "archive with script should succeed: {:?}",
            result
        );
    }

    #[test]
    fn archive_worktree_blocks_on_failed_script() {
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");
        let config = WorktreeConfig {
            task_name: "archive-block-test".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: None,
            create_branch: false,
        };
        let wt = create_worktree_internal(&worktrees_dir, &config, None).expect("create worktree");
        // Script exits non-zero — archive should be blocked
        let result = archive_worktree(repo.path(), "archive-block-test", Some("exit 1"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Archive script failed"));
        // Worktree should still exist (not archived)
        assert!(
            wt.path.exists(),
            "worktree should still exist after failed script"
        );
    }

    #[test]
    fn archive_worktree_skips_empty_script() {
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");
        let config = WorktreeConfig {
            task_name: "archive-noscript-test".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: None,
            create_branch: false,
        };
        create_worktree_internal(&worktrees_dir, &config, None).expect("create worktree");
        // None script — should proceed normally
        let result = archive_worktree(repo.path(), "archive-noscript-test", None);
        assert!(
            result.is_ok(),
            "archive without script should succeed: {:?}",
            result
        );
    }

    #[test]
    fn free_archive_dest_avoids_clobbering_prior_archives() {
        // The anti-clobber guarantee for archive_worktree: archiving the same
        // branch name again must land on a fresh suffix, never overwrite.
        let tmp = tempfile::tempdir().expect("tempdir");
        let archive_dir = tmp.path().join("__archived");
        fs::create_dir_all(&archive_dir).expect("mkdir archive");

        // No prior archive → base name.
        assert_eq!(
            free_archive_dest(&archive_dir, "dup-branch"),
            archive_dir.join("dup-branch")
        );

        // Prior archive at base name → first free suffix, base left untouched.
        let base = archive_dir.join("dup-branch");
        fs::create_dir_all(&base).expect("mkdir base");
        fs::write(base.join("marker.txt"), "first").expect("write marker");
        assert_eq!(
            free_archive_dest(&archive_dir, "dup-branch"),
            archive_dir.join("dup-branch-2")
        );

        // Base and -2 taken → skips to -3.
        fs::create_dir_all(archive_dir.join("dup-branch-2")).expect("mkdir -2");
        assert_eq!(
            free_archive_dest(&archive_dir, "dup-branch"),
            archive_dir.join("dup-branch-3")
        );

        // Original archive contents are never removed by the picker.
        assert_eq!(
            fs::read_to_string(base.join("marker.txt")).expect("read marker"),
            "first"
        );
    }

    #[test]
    fn test_set_and_get_branch_base() {
        let repo = setup_test_repo();
        let path = repo.path().to_string_lossy().to_string();

        // Initially no base set
        let base = get_branch_base(&path, "main");
        assert!(
            base.is_none(),
            "expected no base initially, got: {:?}",
            base
        );

        // Set a base
        set_branch_base(&path, "main", "develop").unwrap();
        let base = get_branch_base(&path, "main");
        assert_eq!(base, Some("develop".to_string()));

        // Overwrite
        set_branch_base(&path, "main", "origin/main").unwrap();
        let base = get_branch_base(&path, "main");
        assert_eq!(base, Some("origin/main".to_string()));
    }

    #[test]
    fn test_get_branch_bases_batches_all_entries() {
        let repo = setup_test_repo();
        let path = repo.path().to_string_lossy().to_string();

        // No bases set yet -> empty map.
        assert!(get_branch_bases(&path).is_empty());

        // Set bases for several branches (names include a dot and a slash to
        // exercise the prefix/suffix anchoring of the key parser).
        set_branch_base(&path, "main", "develop").unwrap();
        set_branch_base(&path, "feature.x", "main").unwrap();
        set_branch_base(&path, "team/feat", "origin/main").unwrap();

        let bases = get_branch_bases(&path);
        assert_eq!(bases.len(), 3, "expected all 3 bases, got: {bases:?}");
        assert_eq!(bases.get("main").map(String::as_str), Some("develop"));
        assert_eq!(bases.get("feature.x").map(String::as_str), Some("main"));
        assert_eq!(
            bases.get("team/feat").map(String::as_str),
            Some("origin/main")
        );

        // Batch result matches the per-branch reader byte-for-byte.
        for (name, base) in &bases {
            assert_eq!(get_branch_base(&path, name).as_ref(), Some(base));
        }
    }

    #[test]
    fn test_fetch_remote_ref_for_local_is_noop() {
        let repo = setup_test_repo();
        let path = repo.path().to_string_lossy().to_string();

        // A local ref like "main" should not trigger a fetch (no-op)
        let result = fetch_if_remote(&path, "main");
        assert!(result.is_ok(), "local ref should not fail: {:?}", result);
    }

    #[test]
    fn test_fetch_local_branch_with_slash_is_noop() {
        let repo = setup_test_repo();
        let path = repo.path().to_string_lossy().to_string();

        // A local branch name can legitimately contain a slash (e.g. Jira-style
        // "POC-0001/merge-radar"). It must NOT be mistaken for a remote ref and
        // fetched — otherwise git treats "POC-0001" as a remote and fails.
        git_cmd(repo.path())
            .args(["branch", "POC-0001/merge-radar"])
            .run()
            .expect("Failed to create slashed local branch");

        let result = fetch_if_remote(&path, "POC-0001/merge-radar");
        assert!(
            result.is_ok(),
            "slashed local branch should be a no-op, not a failed fetch: {:?}",
            result
        );
    }

    /// A fetch that never answers must be killed at its deadline, not waited on.
    ///
    /// The remote is an `ext::` transport helper that only sleeps, so the hang
    /// is deterministic and needs no network — and, like a real credential
    /// helper, the sleeper is a grandchild holding git's pipes open, which is
    /// the case `output_with_deadline` refuses to join on.
    ///
    /// Unix only: the helper is `sleep`.
    #[cfg(unix)]
    #[test]
    fn fetch_if_remote_gives_up_at_the_deadline() {
        let repo = setup_test_repo();
        let path = repo.path().to_string_lossy().to_string();

        git_cmd(repo.path())
            .args(["remote", "add", "origin", "ext::sleep 45"])
            .run()
            .expect("add the hanging remote");
        // git refuses the ext transport unless the repo opts in.
        git_cmd(repo.path())
            .args(["config", "protocol.ext.allow", "always"])
            .run()
            .expect("allow the ext transport");
        // fetch_if_remote only fetches a ref that resolves under refs/remotes/.
        git_cmd(repo.path())
            .args(["update-ref", "refs/remotes/origin/main", "HEAD"])
            .run()
            .expect("create the remote-tracking ref");

        // Zombies owned by this process BEFORE the fetch. Under `cargo nextest`
        // this is always empty — one process per test — but under `cargo test`
        // every test in the binary is a thread of the SAME process, so other
        // tests' unreaped children are counted too. Comparing against a
        // baseline instead of against zero is what makes the assertion below
        // mean "this fetch leaked a zombie" under either runner.
        let zombies_before = own_zombie_pids();

        // Off-thread behind a hard receive deadline: an unwired timeout means
        // the call never returns, and this must report that rather than hang
        // the suite on it.
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(fetch_if_remote(&path, "origin/main"));
        });
        let result = rx
            .recv_timeout(FETCH_TIMEOUT + std::time::Duration::from_secs(30))
            .expect("fetch_if_remote must return — its deadline is not wired");

        let err = result.expect_err("a fetch that never answers must fail");
        assert!(
            err.contains("timed out"),
            "expected a timeout error, got: {err}"
        );

        // The killed git must be reaped, not left as a zombie of this process.
        let leaked: Vec<u32> = own_zombie_pids()
            .into_iter()
            .filter(|pid| !zombies_before.contains(pid))
            .collect();
        assert!(
            leaked.is_empty(),
            "the timed-out git was left as a zombie child: {leaked:?}"
        );
    }

    /// PIDs of this process's children currently in the zombie state.
    ///
    /// Returns pids rather than a count so a caller can diff two samples: a
    /// count would report "2 before, 2 after" as unchanged even if one child
    /// had been reaped and a different one leaked in the same window.
    ///
    /// Unix-only, like its caller: `ps` and the zombie state are both POSIX,
    /// and without the gate this is dead code the Windows job warns about.
    #[cfg(unix)]
    fn own_zombie_pids() -> Vec<u32> {
        let ps = Command::new("ps")
            .args(["-o", "pid=,ppid=,stat=", "-ax"])
            .output()
            .expect("ps");
        let table = String::from_utf8_lossy(&ps.stdout);
        let mine = std::process::id().to_string();
        table
            .lines()
            .filter_map(|line| {
                let mut fields = line.split_whitespace();
                let pid = fields.next()?;
                if fields.next()? != mine {
                    return None;
                }
                fields.next()?.starts_with('Z').then(|| pid.parse().ok())?
            })
            .collect()
    }

    /// End-to-end companion to `test_fetch_local_branch_with_slash_is_noop`:
    /// the user-facing "Create Branch from <slashed local branch>" flow reaches
    /// `fetch_if_remote` through `create_worktree_internal`'s start-point, so the
    /// no-op must hold at the caller too — not just in the helper.
    #[test]
    fn test_create_worktree_from_slashed_local_start_point_succeeds() {
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");

        git_cmd(repo.path())
            .args(["branch", "POC-0001/merge-radar"])
            .run()
            .expect("Failed to create slashed local branch");

        let config = WorktreeConfig {
            task_name: "from-slashed-base".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: Some("POC-0002/derived".to_string()),
            create_branch: true,
        };

        let result =
            create_worktree_internal(&worktrees_dir, &config, Some("POC-0001/merge-radar"));
        assert!(
            result.is_ok(),
            "slashed local start-point must not be fetched as a remote: {:?}",
            result
        );
        assert_eq!(
            result.unwrap().branch,
            Some("POC-0002/derived".to_string()),
            "new branch should be created from the slashed local base"
        );
    }

    /// Get the current branch name in a test repo (could be main or master)
    fn current_branch(repo: &TempDir) -> String {
        let out = git_cmd(repo.path())
            .args(["branch", "--show-current"])
            .run()
            .expect("Failed to get current branch");
        out.stdout.trim().to_string()
    }

    #[test]
    fn test_create_branch_persists_base_ref() {
        let repo = setup_test_repo();
        let path = repo.path().to_string_lossy().to_string();
        let default_branch = current_branch(&repo);

        // Create branch with a start_point
        crate::git::create_branch_impl(&path, "feature-x", Some(&default_branch), false).unwrap();

        // Base ref should be persisted
        let base = get_branch_base(&path, "feature-x");
        assert_eq!(base, Some(default_branch));
    }

    #[test]
    fn test_create_worktree_persists_base_ref() {
        let repo = setup_test_repo();
        let default_branch = current_branch(&repo);
        let worktrees_dir = repo.path().join("worktrees");
        let config = WorktreeConfig {
            task_name: "persist-base-test".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: Some("persist-base-test".to_string()),
            create_branch: true,
        };
        create_worktree_internal(&worktrees_dir, &config, Some(&default_branch)).unwrap();

        let base = get_branch_base(&repo.path().to_string_lossy(), "persist-base-test");
        assert_eq!(base, Some(default_branch));
    }

    /// A `post-checkout` hook script kept in the build directory and rewritten
    /// only when its content is wrong, so every run execs the *same* inode.
    ///
    /// macOS scans each never-before-seen executable inode on its first exec.
    /// Two independent effects stack, and measuring only one of them misleads
    /// (both of us did, from opposite directions, before pairing the samples).
    ///
    /// 1. A **persistent** penalty on `/var/folders/…/T` — `$TMPDIR`, which is
    ///    exactly where `TempDir` lands. Paired alternating samples, fresh inode
    ///    each time, 2026-09-06: `$TMPDIR` 0.685s / 0.359s against `/tmp` 0.250s
    ///    / 0.233s and `~/Gits/.tmp` 0.263s / 0.238s in the same seconds.
    /// 2. An **episodic** scanner backlog that lifts the floor everywhere for
    ///    minutes at a time, and amplifies (1) enormously while it lasts: the
    ///    same pairing during a backlog gave `$TMPDIR` 191-393s against `/tmp`
    ///    1.9-4.5s. A 120s+ outlier is what first surfaced this test.
    ///
    /// How the two combine is NOT settled: a plain multiplicative model predicts
    /// ~9s for `$TMPDIR` under the backlog above and 393s was measured, so the
    /// interaction looks superlinear — which would mean the penalty is worst
    /// exactly when the suite is busiest. Pinning that down costs 400-second
    /// samples and changes no remedy, so it is left open on purpose.
    ///
    /// So a single timing sample proves nothing, and neither variable is worth
    /// chasing. What is stable is the caching: a re-exec of an already-scanned
    /// inode is ~0.01s, and the cache is keyed on the *inode*, so a symlink to a
    /// warm script is free while a byte-identical copy is not. Minting a hook
    /// into a fresh `TempDir` per run re-rolls both dice every run; pointing at
    /// one stable file removes the exec from the lottery under either model.
    #[cfg(unix)]
    fn shared_post_checkout_hook() -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        const BODY: &[u8] = b"#!/bin/sh\ntouch .hook-ran\n";

        // with-test-tmp.sh removes each run's temp root. Keep the executable
        // in its stable parent so later runs reuse the scanned inode.
        let dir = tuic_test_support::test_temp_root()
            .parent()
            .expect("test temp root has a parent")
            .join("tuic-git/test-hooks");
        fs::create_dir_all(&dir).expect("create shared hook dir");
        let hook = dir.join("post-checkout");

        if fs::read(&hook).ok().as_deref() != Some(BODY) {
            // Stage and rename so a concurrent run can never exec a partial file.
            let staged = dir.join(format!("post-checkout.{}.tmp", std::process::id()));
            fs::write(&staged, BODY).expect("write shared hook");
            fs::set_permissions(&staged, fs::Permissions::from_mode(0o755))
                .expect("chmod shared hook");
            fs::rename(&staged, &hook).expect("install shared hook");
        }
        hook
    }

    // Verify that post-checkout hooks still run after `git worktree add --quiet`.
    // --quiet only suppresses git's own checkout progress lines, not hooks.
    #[test]
    #[cfg(unix)]
    fn test_create_worktree_runs_post_checkout_hook() {
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");

        // Symlink rather than copy: git resolves it and execs the shared inode,
        // so the hook still installs at the canonical path real users use, with
        // no per-run exec scan. See `shared_post_checkout_hook`.
        let hooks_dir = repo.path().join(".git/hooks");
        fs::create_dir_all(&hooks_dir).unwrap();
        std::os::unix::fs::symlink(shared_post_checkout_hook(), hooks_dir.join("post-checkout"))
            .unwrap();

        let config = WorktreeConfig {
            task_name: "hook-run-test".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: Some("hook-run-test".to_string()),
            create_branch: true,
        };
        create_worktree_internal(&worktrees_dir, &config, None).unwrap();

        let worktree_path = worktrees_dir.join("hook-run-test");
        assert!(
            worktree_path.join(".hook-ran").exists(),
            "post-checkout hook did not run — --quiet must not suppress hooks"
        );
    }

    #[test]
    fn test_list_base_ref_options_returns_structured_refs() {
        let repo = setup_test_repo();
        let path = repo.path().to_string_lossy().to_string();

        // Create extra local branches
        git_cmd(repo.path())
            .args(["branch", "feature-a"])
            .run()
            .unwrap();
        git_cmd(repo.path())
            .args(["branch", "feature-b"])
            .run()
            .unwrap();

        let refs = list_base_ref_options(path).unwrap();

        // Should have at least the default + 2 feature branches
        assert!(
            refs.len() >= 3,
            "expected at least 3 refs, got {}",
            refs.len()
        );

        // First ref should be the default branch, flagged is_default
        let default_ref = &refs[0];
        assert!(
            default_ref.is_default,
            "first ref should be the default branch"
        );
        assert_eq!(default_ref.kind, "local");

        // All refs should have non-empty names
        for r in &refs {
            assert!(!r.name.is_empty(), "ref name should not be empty");
            assert!(
                r.kind == "local" || r.kind == "remote",
                "kind should be local or remote"
            );
        }

        // feature-a and feature-b should be present as local
        let names: Vec<&str> = refs.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"feature-a"), "feature-a should be in refs");
        assert!(names.contains(&"feature-b"), "feature-b should be in refs");

        // No origin/HEAD should appear
        assert!(
            !names.contains(&"origin/HEAD"),
            "origin/HEAD should be filtered out"
        );
    }

    #[test]
    fn test_list_base_ref_options_includes_remote_refs() {
        let repo = setup_test_repo();
        let path_str = repo.path().to_string_lossy().to_string();

        // Create a bare remote and push to it to get remote tracking refs
        let remote_dir = TempDir::new().unwrap();
        git_cmd(remote_dir.path())
            .args(["init", "--bare"])
            .run()
            .unwrap();
        git_cmd(repo.path())
            .args([
                "remote",
                "add",
                "origin",
                &remote_dir.path().to_string_lossy(),
            ])
            .run()
            .unwrap();
        git_cmd(repo.path())
            .args(["push", "-u", "origin", "main"])
            .run()
            .or_else(|_| {
                git_cmd(repo.path())
                    .args(["push", "-u", "origin", "master"])
                    .run()
            })
            .unwrap();

        // Create a remote-only branch
        git_cmd(repo.path())
            .args(["branch", "remote-only"])
            .run()
            .unwrap();
        git_cmd(repo.path())
            .args(["push", "origin", "remote-only"])
            .run()
            .unwrap();
        git_cmd(repo.path())
            .args(["branch", "-D", "remote-only"])
            .run()
            .unwrap();

        // Fetch so we have remote tracking refs
        git_cmd(repo.path())
            .args(["fetch", "origin"])
            .run()
            .unwrap();

        let refs = list_base_ref_options(path_str).unwrap();

        // Should include remote refs
        let remote_refs: Vec<&BaseRefOption> = refs.iter().filter(|r| r.kind == "remote").collect();
        assert!(
            !remote_refs.is_empty(),
            "should include remote refs, got: {:?}",
            refs
        );

        // origin/remote-only should appear as remote
        let names: Vec<&str> = refs.iter().map(|r| r.name.as_str()).collect();
        assert!(
            names.contains(&"origin/remote-only"),
            "origin/remote-only should be in refs, got: {:?}",
            names
        );
    }

    // ── linked workspace creation and warming ─────────────────────────────

    fn workspace_fixture() -> (TempDir, PathBuf, PathBuf) {
        let temp = TempDir::new().expect("temp dir");
        let repo = temp.path().join("repo");
        let workspaces = temp.path().join("repo__wt");
        fs::create_dir_all(&repo).expect("repo dir");
        git_cmd(&repo).args(["init"]).run().expect("git init");
        git_cmd(&repo)
            .args(["config", "user.email", "test@test.com"])
            .run()
            .expect("git email");
        git_cmd(&repo)
            .args(["config", "user.name", "Test"])
            .run()
            .expect("git name");
        fs::write(repo.join("README.md"), "base\n").expect("tracked file");
        git_cmd(&repo).args(["add", "."]).run().expect("git add");
        git_cmd(&repo)
            .args(["commit", "-m", "initial"])
            .run()
            .expect("git commit");
        (temp, repo, workspaces)
    }

    #[test]
    fn create_workspace_always_returns_a_linked_worktree() {
        let (_temp, repo, workspaces) = workspace_fixture();
        let config = WorktreeConfig {
            task_name: "feature".into(),
            base_repo: repo.to_string_lossy().into_owned(),
            branch: Some("feature".into()),
            create_branch: true,
        };
        let created = create_workspace_with(&workspaces, &config, None, |_, _| {
            crate::cow::WarmingReport::default()
        })
        .expect("linked workspace");

        assert_eq!(created.kind, WorkspaceKind::Worktree);
        assert_eq!(created.workspace_id, "feature");
        assert!(created.path.join(".git").is_file());
    }

    #[test]
    fn create_workspace_reports_best_effort_warming() {
        let (_temp, repo, workspaces) = workspace_fixture();
        let config = WorktreeConfig {
            task_name: "warm".into(),
            base_repo: repo.to_string_lossy().into_owned(),
            branch: Some("warm".into()),
            create_branch: true,
        };
        let created = create_workspace_with(&workspaces, &config, None, |_, _| {
            crate::cow::WarmingReport {
                warmed: 2,
                warnings: vec!["one cache stayed cold".into()],
            }
        })
        .expect("linked workspace");

        assert_eq!(created.warmed_directories, 2);
        assert_eq!(created.warnings, vec!["one cache stayed cold"]);
    }

    #[cfg(unix)]
    #[test]
    fn removing_a_workspace_unlinks_shared_stores_without_touching_the_parent() {
        let (_temp, repo, workspaces) = workspace_fixture();
        fs::write(repo.join(".gitignore"), "stories\nplans\nideas\n").unwrap();
        git_cmd(&repo).args(["add", ".gitignore"]).run().unwrap();
        git_cmd(&repo)
            .args(["commit", "-m", "ignore stores"])
            .run()
            .unwrap();
        fs::create_dir(repo.join("stories")).unwrap();
        fs::write(repo.join("stories").join("keep.md"), "parent story").unwrap();
        let config = WorktreeConfig {
            task_name: "shared-remove".into(),
            base_repo: repo.to_string_lossy().into_owned(),
            branch: Some("shared-remove".into()),
            create_branch: true,
        };
        let created = create_workspace_with(&workspaces, &config, None, |_, _| {
            crate::cow::WarmingReport::default()
        })
        .expect("linked workspace");
        assert!(
            fs::symlink_metadata(created.path.join("stories"))
                .unwrap()
                .file_type()
                .is_symlink()
        );

        remove_worktree_by_workspace_id(
            &repo.to_string_lossy(),
            "shared-remove",
            false,
            None,
            false,
        )
        .expect("an ignored store link must not make the workspace dirty");

        assert!(!created.path.exists(), "workspace directory removed");
        assert_eq!(
            fs::read_to_string(repo.join("stories").join("keep.md")).unwrap(),
            "parent story"
        );
    }

    #[cfg(unix)]
    #[test]
    fn create_workspace_links_existing_shared_stores_to_the_parent() {
        let (_temp, repo, workspaces) = workspace_fixture();
        fs::create_dir(repo.join("stories")).unwrap();
        fs::write(repo.join("stories/active.md"), "shared").unwrap();
        let config = WorktreeConfig {
            task_name: "shared-stores".into(),
            base_repo: repo.to_string_lossy().into_owned(),
            branch: Some("shared-stores".into()),
            create_branch: true,
        };

        let created = create_workspace_with(&workspaces, &config, None, |_, _| {
            crate::cow::WarmingReport::default()
        })
        .expect("linked workspace");

        assert!(
            fs::symlink_metadata(created.path.join("stories"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            fs::canonicalize(created.path.join("stories/active.md")).unwrap(),
            fs::canonicalize(repo.join("stories/active.md")).unwrap()
        );
    }

    #[test]
    fn create_workspace_initializes_submodules_from_the_parent_checkout() {
        let (_temp, repo, workspaces) = workspace_fixture();
        let module = repo.parent().unwrap().join("module");
        fs::create_dir(&module).unwrap();
        git_cmd(&module).args(["init"]).run().unwrap();
        git_cmd(&module)
            .args(["config", "user.email", "test@test.com"])
            .run()
            .unwrap();
        git_cmd(&module)
            .args(["config", "user.name", "Test"])
            .run()
            .unwrap();
        fs::write(module.join("sidecar.txt"), "local object").unwrap();
        git_cmd(&module).args(["add", "."]).run().unwrap();
        git_cmd(&module)
            .args(["commit", "-m", "module"])
            .run()
            .unwrap();
        git_cmd(&repo)
            .args([
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                &module.to_string_lossy(),
                "modules/local",
            ])
            .run()
            .unwrap();
        git_cmd(&repo).args(["add", "."]).run().unwrap();
        git_cmd(&repo)
            .args(["commit", "-m", "submodule"])
            .run()
            .unwrap();
        git_cmd(&repo)
            .args([
                "config",
                "--file",
                ".gitmodules",
                "submodule.modules/local.url",
                "/missing/remote",
            ])
            .run()
            .unwrap();
        git_cmd(&repo).args(["add", ".gitmodules"]).run().unwrap();
        git_cmd(&repo)
            .args(["commit", "-m", "unavailable remote"])
            .run()
            .unwrap();

        let config = WorktreeConfig {
            task_name: "local-submodule".into(),
            base_repo: repo.to_string_lossy().into_owned(),
            branch: Some("local-submodule".into()),
            create_branch: true,
        };
        let created = create_workspace_with(&workspaces, &config, None, |_, _| {
            crate::cow::WarmingReport::default()
        })
        .unwrap();

        assert!(created.warnings.is_empty(), "{:#?}", created.warnings);
        assert_eq!(
            fs::read_to_string(created.path.join("modules/local/sidecar.txt")).unwrap(),
            "local object"
        );
    }

    #[test]
    fn create_workspace_reports_an_unavailable_submodule_as_a_warning() {
        let (_temp, repo, workspaces) = workspace_fixture();
        fs::write(
            repo.join(".gitmodules"),
            "[submodule \"missing\"]\n\tpath = modules/missing\n\turl = /missing/remote\n",
        )
        .unwrap();
        git_cmd(&repo)
            .args([
                "update-index",
                "--add",
                "--cacheinfo",
                "160000,1111111111111111111111111111111111111111,modules/missing",
            ])
            .run()
            .unwrap();
        git_cmd(&repo).args(["add", ".gitmodules"]).run().unwrap();
        git_cmd(&repo)
            .args(["commit", "-m", "missing submodule"])
            .run()
            .unwrap();
        let config = WorktreeConfig {
            task_name: "missing-submodule".into(),
            base_repo: repo.to_string_lossy().into_owned(),
            branch: Some("missing-submodule".into()),
            create_branch: true,
        };

        let created = create_workspace_with(&workspaces, &config, None, |_, _| {
            crate::cow::WarmingReport::default()
        })
        .unwrap();

        assert!(
            created.warnings.iter().any(|warning| {
                warning.contains("modules/missing") && warning.contains("could not initialize")
            }),
            "warnings: {:?}",
            created.warnings
        );
    }

    #[cfg(unix)]
    #[test]
    fn submodule_remote_fallback_gives_up_at_its_deadline() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        fs::write(
            repo.join(".gitmodules"),
            "[submodule \"hanging\"]\n\tpath = modules/hanging\n\turl = ext::sleep 12\n",
        )
        .unwrap();
        git_cmd(&repo)
            .args([
                "update-index",
                "--add",
                "--cacheinfo",
                "160000,1111111111111111111111111111111111111111,modules/hanging",
            ])
            .run()
            .unwrap();
        git_cmd(&repo).args(["add", ".gitmodules"]).run().unwrap();
        git_cmd(&repo)
            .args(["commit", "-m", "hanging submodule"])
            .run()
            .unwrap();
        let worktree = add_worktree(&repo, "submodule-timeout");
        let (tx, rx) = std::sync::mpsc::channel();
        let source = repo.clone();
        std::thread::spawn(move || {
            // Git's submodule child refuses `ext` even when the repository
            // config allows it, so the opt-in is granted on this call's own
            // subprocess (see `initialize_submodules_with_allowed_protocol`)
            // rather than the process environment.
            let warnings =
                initialize_submodules_with_allowed_protocol(&source, &worktree, Some("ext"));
            let _ = tx.send(warnings);
        });
        let warnings = rx
            .recv_timeout(FETCH_TIMEOUT + Duration::from_secs(30))
            .expect("submodule fallback must return within its deadline");
        assert!(
            warnings.iter().any(|warning| warning.contains("timed out")),
            "{warnings:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn submodule_remote_fallback_does_not_block_concurrent_file_transport() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        fs::write(
            repo.join(".gitmodules"),
            "[submodule \"hanging\"]\n\tpath = modules/hanging\n\turl = ext::sleep 12\n",
        )
        .unwrap();
        git_cmd(&repo)
            .args([
                "update-index",
                "--add",
                "--cacheinfo",
                "160000,1111111111111111111111111111111111111111,modules/hanging",
            ])
            .run()
            .unwrap();
        git_cmd(&repo).args(["add", ".gitmodules"]).run().unwrap();
        git_cmd(&repo)
            .args(["commit", "-m", "hanging submodule"])
            .run()
            .unwrap();
        let worktree = add_worktree(&repo, "submodule-isolation");
        let source = repo.clone();
        let done = Arc::new(AtomicU64::new(0));
        let done_writer = Arc::clone(&done);
        let fallback = std::thread::spawn(move || {
            let warnings =
                initialize_submodules_with_allowed_protocol(&source, &worktree, Some("ext"));
            done_writer.store(1, Ordering::SeqCst);
            warnings
        });

        // While the fallback subprocess is granted `ext` for its own submodule
        // update, hammer an unrelated file-protocol git call from this thread.
        // It must never observe the override: it is scoped to the fallback's
        // own subprocess, not leaked into the process environment.
        let mut failures = Vec::new();
        while done.load(Ordering::SeqCst) == 0 {
            if let Err(error) = git_cmd(&repo)
                .args([
                    "-c",
                    "protocol.file.allow=always",
                    "ls-remote",
                    &repo.to_string_lossy(),
                ])
                .run()
            {
                failures.push(error.to_string());
            }
        }

        fallback
            .join()
            .expect("submodule fallback thread must not panic");

        assert!(
            failures.is_empty(),
            "an ext-only override for one submodule fallback must not block concurrent file-protocol git calls: {failures:?}"
        );
    }

    #[test]
    fn pending_warm_instructions_do_not_claim_the_workspace_is_cold() {
        let (_temp, repo, workspaces) = workspace_fixture();
        let config = WorktreeConfig {
            task_name: "pending-warm".into(),
            base_repo: repo.to_string_lossy().into_owned(),
            branch: Some("pending-warm".into()),
            create_branch: true,
        };
        let workspace = create_workspace_unwarmed(&workspaces, &config, None).unwrap();

        let instructions = workspace.instruction_payload_pending();

        assert_eq!(instructions["warm_artifacts"]["status"], "pending");
        let note = instructions["warm_artifacts"]["note"].as_str().unwrap();
        assert!(note.contains("GET /worktrees/paths"), "{note}");
        assert!(!note.contains("No build output"), "{note}");
        assert!(!note.contains("build to set up"), "{note}");
    }

    #[test]
    fn ipc_create_response_reports_pending_warm_state() {
        let (_temp, repo, workspaces) = workspace_fixture();
        let config = WorktreeConfig {
            task_name: "ipc-pending".into(),
            base_repo: repo.to_string_lossy().into_owned(),
            branch: Some("ipc-pending".into()),
            create_branch: true,
        };
        let workspace = create_workspace_unwarmed(&workspaces, &config, None).unwrap();
        let response = ipc_worktree_response(&workspace, &repo.to_string_lossy());
        assert_eq!(
            response["instructions"]["warm_artifacts"]["status"],
            "pending"
        );
    }

    #[test]
    fn instructions_report_failed_warming() {
        let (_temp, repo, workspaces) = workspace_fixture();
        let config = WorktreeConfig {
            task_name: "failed-warm".into(),
            base_repo: repo.to_string_lossy().into_owned(),
            branch: Some("failed-warm".into()),
            create_branch: true,
        };
        let mut workspace = create_workspace_unwarmed(&workspaces, &config, None).unwrap();
        workspace.warnings.push("copy failed".into());
        let payload = workspace.instruction_payload();
        assert_eq!(payload["warm_artifacts"]["status"], "failed");
        assert!(
            payload["warm_artifacts"]["reason"]
                .as_str()
                .unwrap()
                .contains("copy failed")
        );
    }

    #[test]
    fn workspace_payload_states_linked_isolation_and_clean_tracked_state() {
        let (_temp, repo, workspaces) = workspace_fixture();
        let config = WorktreeConfig {
            task_name: "payload".into(),
            base_repo: repo.to_string_lossy().into_owned(),
            branch: Some("payload".into()),
            create_branch: true,
        };
        let created = create_workspace_with(&workspaces, &config, None, |_, _| {
            crate::cow::WarmingReport::default()
        })
        .expect("linked workspace");
        let payload = created.instruction_payload();

        assert_eq!(payload["kind"], "worktree");
        assert_eq!(payload["state"]["carried_over"], 0);
        assert!(
            payload["isolation"]
                .as_str()
                .unwrap()
                .contains("linked worktree")
        );
        assert_eq!(payload["warm_artifacts"]["warmed_directories"], 0);
    }

    /// Add a linked worktree on a new branch and return its path.
    fn add_worktree(repo: &Path, branch: &str) -> PathBuf {
        let path = repo.parent().expect("parent").join(branch);
        git_cmd(repo)
            .args([
                "worktree",
                "add",
                "-b",
                branch,
                &path.to_string_lossy(),
                "HEAD",
            ])
            .run()
            .expect("git worktree add");
        path
    }

    fn add_populated_submodule(repo: &Path) {
        let module = repo.parent().unwrap().join("module-source");
        fs::create_dir(&module).unwrap();
        git_cmd(&module).args(["init"]).run().unwrap();
        git_cmd(&module)
            .args(["config", "user.email", "test@test.com"])
            .run()
            .unwrap();
        git_cmd(&module)
            .args(["config", "user.name", "Test"])
            .run()
            .unwrap();
        commit_file(&module, "module.txt", "committed module content\n");
        git_cmd(repo)
            .args([
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                &module.to_string_lossy(),
                "modules/local",
            ])
            .run()
            .unwrap();
        git_cmd(repo).args(["add", "."]).run().unwrap();
        git_cmd(repo)
            .args(["commit", "-m", "add submodule"])
            .run()
            .unwrap();
    }

    #[test]
    fn clean_populated_submodule_allows_non_force_removal() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        add_populated_submodule(&repo);
        let worktree = add_worktree(&repo, "clean-module");
        git_cmd(&worktree)
            .args([
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "update",
                "--init",
            ])
            .run()
            .unwrap();

        remove_worktree_by_workspace_id(&repo.to_string_lossy(), "clean-module", true, None, false)
            .expect("a clean populated submodule can be safely removed");
        assert!(!worktree.exists());
    }

    #[test]
    fn merged_submodule_commit_survives_non_force_worktree_removal() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        add_populated_submodule(&repo);
        let worktree = add_worktree(&repo, "module-commit");
        git_cmd(&worktree)
            .args([
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "update",
                "--init",
            ])
            .run()
            .unwrap();
        let module = worktree.join("modules/local");
        git_cmd(&module)
            .args(["config", "user.email", "test@test.com"])
            .run()
            .unwrap();
        git_cmd(&module)
            .args(["config", "user.name", "Test"])
            .run()
            .unwrap();
        commit_file(&module, "local-only.txt", "only in this worktree\n");
        let submodule_oid = rev_at(&module, "HEAD").unwrap();
        git_cmd(&worktree)
            .args(["add", "modules/local"])
            .run()
            .unwrap();
        git_cmd(&worktree)
            .args(["commit", "-m", "advance gitlink"])
            .run()
            .unwrap();
        git_cmd(&repo)
            .args([
                "merge",
                "--no-ff",
                "module-commit",
                "-m",
                "merge module commit",
            ])
            .run()
            .unwrap();
        assert_eq!(dirty_files_at(&worktree).unwrap(), 0);
        assert!(
            git_cmd(&worktree)
                .args(["submodule", "status"])
                .run()
                .unwrap()
                .stdout
                .starts_with(' ')
        );
        assert!(
            git_cmd(&repo.join("modules/local"))
                .args(["cat-file", "-e", &submodule_oid])
                .run()
                .is_err()
        );

        remove_worktree_by_workspace_id(
            &repo.to_string_lossy(),
            "module-commit",
            true,
            None,
            false,
        )
        .unwrap();
        assert!(
            git_cmd(&repo.join("modules/local"))
                .args(["cat-file", "-e", &submodule_oid])
                .run()
                .is_ok(),
            "removal must preserve the submodule object in the main module repository"
        );
    }

    #[test]
    fn clean_submodule_local_branch_and_stash_survive_removal() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        add_populated_submodule(&repo);
        let worktree = add_worktree(&repo, "module-refs");
        git_cmd(&worktree)
            .args([
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "update",
                "--init",
            ])
            .run()
            .unwrap();
        let module = worktree.join("modules/local");
        git_cmd(&module)
            .args(["config", "user.email", "test@test.com"])
            .run()
            .unwrap();
        git_cmd(&module)
            .args(["config", "user.name", "Test"])
            .run()
            .unwrap();
        let original = rev_at(&module, "HEAD").unwrap();
        git_cmd(&module)
            .args(["switch", "-c", "local-only"])
            .run()
            .unwrap();
        commit_file(&module, "local-ref.txt", "local ref\n");
        let local_oid = rev_at(&module, "HEAD").unwrap();
        git_cmd(&module)
            .args(["checkout", "--detach", &original])
            .run()
            .unwrap();
        fs::write(module.join("module.txt"), "stash-only\n").unwrap();
        git_cmd(&module)
            .args(["stash", "push", "-m", "test stash"])
            .run()
            .unwrap();
        let stash_oid = rev_at(&module, "refs/stash").unwrap();
        assert_eq!(dirty_files_at(&worktree).unwrap(), 0);

        remove_worktree_by_workspace_id(&repo.to_string_lossy(), "module-refs", true, None, false)
            .unwrap();
        let main_module = repo.join("modules/local");
        for oid in [local_oid, stash_oid] {
            assert!(
                git_cmd(&main_module)
                    .args(["cat-file", "-e", &oid])
                    .run()
                    .is_ok(),
                "lost submodule ref object {oid}"
            );
        }
    }

    #[test]
    fn uninitialized_submodule_does_not_block_non_force_removal() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        add_populated_submodule(&repo);
        let worktree = add_worktree(&repo, "uninitialized-module");
        assert!(
            git_cmd(&worktree)
                .args(["submodule", "status"])
                .run()
                .unwrap()
                .stdout
                .starts_with('-')
        );
        remove_worktree_by_workspace_id(
            &repo.to_string_lossy(),
            "uninitialized-module",
            true,
            None,
            false,
        )
        .unwrap();
        assert!(!worktree.exists());
    }

    #[test]
    fn preservation_succeeds_when_the_worktree_path_is_long() {
        // Catches a ref component that grows with the checkout path: the old
        // hex-of-path scheme doubled a 130-byte dir name past the 255-byte limit.
        let (_temp, repo, _workspaces) = workspace_fixture();
        add_populated_submodule(&repo);
        let short_worktree = add_worktree(&repo, "long-path-module");
        let worktree = repo.parent().unwrap().join("w".repeat(130));
        git_cmd(&short_worktree)
            .args([
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "update",
                "--init",
            ])
            .run()
            .unwrap();
        // Initialize before moving: Git for Windows cannot clone a submodule
        // through its fixed-size $GIT_DIR buffer at this depth.
        let gitdir = rev_at(&short_worktree.join("modules/local"), "--absolute-git-dir").unwrap();
        fs::rename(&short_worktree, &worktree).unwrap();
        git_cmd(&repo)
            .args(["worktree", "repair", &worktree.to_string_lossy()])
            .run()
            .unwrap();
        repair_archived_submodules(&repo, &worktree, &[("modules/local".into(), gitdir)]).unwrap();
        let head = rev_at(&worktree.join("modules/local"), "HEAD").unwrap();
        preserve_submodule_refs(&repo, &worktree, "modules/local").unwrap();
        let refs = git_cmd(&repo.join("modules/local"))
            .args([
                "for-each-ref",
                "--format=%(refname)",
                "--points-at",
                &head,
                "refs/tuic/preserved",
            ])
            .run()
            .unwrap();
        assert!(!refs.stdout.trim().is_empty());
        assert!(
            refs.stdout
                .lines()
                .all(|r| r.split('/').all(|c| c.len() <= 255))
        );
    }

    #[test]
    fn repeated_preservation_keeps_refs_for_both_submodule_heads() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        add_populated_submodule(&repo);
        let worktree = add_worktree(&repo, "repeat-module");
        git_cmd(&worktree)
            .args([
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "update",
                "--init",
            ])
            .run()
            .unwrap();
        let module = worktree.join("modules/local");
        git_cmd(&module)
            .args(["config", "user.email", "test@test.com"])
            .run()
            .unwrap();
        git_cmd(&module)
            .args(["config", "user.name", "Test"])
            .run()
            .unwrap();
        commit_file(&module, "first.txt", "first\n");
        let first = rev_at(&module, "HEAD").unwrap();
        preserve_submodule_refs(&repo, &worktree, "modules/local").unwrap();
        commit_file(&module, "second.txt", "second\n");
        let second = rev_at(&module, "HEAD").unwrap();
        preserve_submodule_refs(&repo, &worktree, "modules/local").unwrap();
        for oid in [&first, &second] {
            let refs = git_cmd(&repo.join("modules/local"))
                .args([
                    "for-each-ref",
                    "--format=%(refname)",
                    "--points-at",
                    oid,
                    "refs/tuic/preserved",
                ])
                .run()
                .unwrap();
            assert!(
                !refs.stdout.trim().is_empty(),
                "{oid} lost its preservation ref"
            );
        }
    }

    #[test]
    fn removal_preserves_submodule_stash_history_and_reflog_only_commit() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        add_populated_submodule(&repo);
        let path = add_worktree(&repo, "module-reflog-history");
        git_cmd(&path)
            .args([
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "update",
                "--init",
            ])
            .run()
            .unwrap();
        let module = path.join("modules/local");
        git_cmd(&module)
            .args(["config", "user.email", "test@test.com"])
            .run()
            .unwrap();
        git_cmd(&module)
            .args(["config", "user.name", "Test"])
            .run()
            .unwrap();
        for index in 1..=2 {
            fs::write(module.join("module.txt"), format!("stash {index}\n")).unwrap();
            git_cmd(&module)
                .args(["stash", "push", "-m", &format!("stash {index}")])
                .run()
                .unwrap();
        }
        let stashes = git_cmd(&module)
            .args(["stash", "list", "--format=%H"])
            .run()
            .unwrap()
            .stdout
            .lines()
            .map(str::to_string)
            .collect::<Vec<_>>();
        assert_eq!(stashes.len(), 2);
        commit_file(&module, "reflog-only.txt", "detached local commit\n");
        let reflog_only = rev_at(&module, "HEAD").unwrap();
        git_cmd(&module)
            .args(["reset", "--hard", "HEAD~1"])
            .run()
            .unwrap();
        assert_ne!(rev_at(&module, "HEAD").unwrap(), reflog_only);

        let worktree = WorktreeInfo {
            name: "module-reflog-history".into(),
            path: path.clone(),
            branch: Some("module-reflog-history".into()),
            base_repo: repo.clone(),
        };
        remove_worktree_internal(&worktree, false).unwrap();
        assert!(!path.exists());
        let destination = repo.join("modules/local");
        for oid in stashes.iter().chain(std::iter::once(&reflog_only)) {
            git_cmd(&destination)
                .args(["cat-file", "-e", oid])
                .run()
                .expect("removed module commit remains available");
            let refs = git_cmd(&destination)
                .args([
                    "for-each-ref",
                    "--format=%(refname)",
                    "--points-at",
                    oid,
                    "refs/tuic/preserved",
                ])
                .run()
                .unwrap();
            assert!(!refs.stdout.trim().is_empty(), "{oid} has no durable ref");
        }
    }

    #[test]
    fn preservation_refuses_uninitialized_main_module_without_writing_superproject_refs() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        add_populated_submodule(&repo);
        let path = add_worktree(&repo, "main-module-deinitialized");
        git_cmd(&path)
            .args([
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "update",
                "--init",
            ])
            .run()
            .unwrap();
        git_cmd(&repo)
            .args(["submodule", "deinit", "--force", "modules/local"])
            .run()
            .unwrap();
        assert!(!repo.join("modules/local/.git").exists());
        let error = preserve_submodule_refs(&repo, &path, "modules/local").unwrap_err();
        assert!(error.contains("main checkout has no repository"), "{error}");
        assert!(path.exists());
        assert!(
            git_cmd(&repo)
                .args(["for-each-ref", "--format=%(refname)", "refs/tuic/preserved"])
                .run()
                .unwrap()
                .stdout
                .trim()
                .is_empty()
        );
    }

    #[test]
    fn removal_refuses_deinitialized_named_submodule_with_local_commit() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let source = repo.parent().unwrap().join("named-module-source");
        fs::create_dir(&source).unwrap();
        git_cmd(&source).args(["init"]).run().unwrap();
        git_cmd(&source)
            .args(["config", "user.email", "test@test.com"])
            .run()
            .unwrap();
        git_cmd(&source)
            .args(["config", "user.name", "Test"])
            .run()
            .unwrap();
        commit_file(&source, "module.txt", "base commit\n");
        git_cmd(&repo)
            .args([
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                "--name",
                "git-module-name",
                &source.to_string_lossy(),
                "modules/checkout-path",
            ])
            .run()
            .unwrap();
        git_cmd(&repo).args(["add", "."]).run().unwrap();
        git_cmd(&repo)
            .args(["commit", "-m", "add named submodule"])
            .run()
            .unwrap();

        let path = add_worktree(&repo, "deinitialized-named-module");
        git_cmd(&path)
            .args([
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "update",
                "--init",
            ])
            .run()
            .unwrap();
        let module = path.join("modules/checkout-path");
        git_cmd(&module)
            .args(["config", "user.email", "test@test.com"])
            .run()
            .unwrap();
        git_cmd(&module)
            .args(["config", "user.name", "Test"])
            .run()
            .unwrap();
        commit_file(&module, "local-only.txt", "local commit\n");
        let oid = rev_at(&module, "HEAD").unwrap();
        let module_gitdir = PathBuf::from(rev_at(&module, "--absolute-git-dir").unwrap());
        git_cmd(&path)
            .args(["submodule", "deinit", "--force", "modules/checkout-path"])
            .run()
            .unwrap();
        assert!(!module.join(".git").exists());
        assert!(module_gitdir.exists(), "deinit retained module Git admin");

        let worktree = WorktreeInfo {
            name: "deinitialized-named-module".into(),
            path: path.clone(),
            branch: Some("deinitialized-named-module".into()),
            base_repo: repo,
        };
        let error = remove_worktree_internal(&worktree, false).unwrap_err();
        assert!(error.contains("uninitialized submodule"), "{error}");
        assert!(path.exists(), "refusal keeps the source worktree");
        assert!(module_gitdir.exists(), "refusal keeps the module Git store");
        git_cmd(&worktree.base_repo)
            .args([
                "--git-dir",
                &module_gitdir.to_string_lossy(),
                "cat-file",
                "-e",
                &oid,
            ])
            .run()
            .expect("local-only commit remains recoverable");
    }

    #[test]
    fn removal_refuses_deinitialized_nested_named_submodule() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let leaf = repo.parent().unwrap().join("nested-leaf-source");
        fs::create_dir(&leaf).unwrap();
        git_cmd(&leaf).args(["init"]).run().unwrap();
        git_cmd(&leaf)
            .args(["config", "user.email", "test@test.com"])
            .run()
            .unwrap();
        git_cmd(&leaf)
            .args(["config", "user.name", "Test"])
            .run()
            .unwrap();
        commit_file(&leaf, "leaf.txt", "base leaf\n");
        let outer = repo.parent().unwrap().join("nested-outer-source");
        fs::create_dir(&outer).unwrap();
        git_cmd(&outer).args(["init"]).run().unwrap();
        git_cmd(&outer)
            .args(["config", "user.email", "test@test.com"])
            .run()
            .unwrap();
        git_cmd(&outer)
            .args(["config", "user.name", "Test"])
            .run()
            .unwrap();
        commit_file(&outer, "outer.txt", "base outer\n");
        git_cmd(&outer)
            .args([
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                "--name",
                "nested-git-name",
                &leaf.to_string_lossy(),
                "nested/checkout-path",
            ])
            .run()
            .unwrap();
        git_cmd(&outer).args(["add", "."]).run().unwrap();
        git_cmd(&outer)
            .args(["commit", "-m", "add leaf"])
            .run()
            .unwrap();
        git_cmd(&repo)
            .args([
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                "--name",
                "outer-git-name",
                &outer.to_string_lossy(),
                "modules/outer-path",
            ])
            .run()
            .unwrap();
        git_cmd(&repo).args(["add", "."]).run().unwrap();
        git_cmd(&repo)
            .args(["commit", "-m", "add outer"])
            .run()
            .unwrap();

        let path = add_worktree(&repo, "deinitialized-nested-module");
        git_cmd(&path)
            .args([
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "update",
                "--init",
                "--recursive",
            ])
            .run()
            .unwrap();
        let outer_module = path.join("modules/outer-path");
        let nested = outer_module.join("nested/checkout-path");
        git_cmd(&nested)
            .args(["config", "user.email", "test@test.com"])
            .run()
            .unwrap();
        git_cmd(&nested)
            .args(["config", "user.name", "Test"])
            .run()
            .unwrap();
        commit_file(&nested, "local-only.txt", "nested local commit\n");
        let oid = rev_at(&nested, "HEAD").unwrap();
        let nested_gitdir = PathBuf::from(rev_at(&nested, "--absolute-git-dir").unwrap());
        git_cmd(&outer_module)
            .args(["submodule", "deinit", "--force", "nested/checkout-path"])
            .run()
            .unwrap();
        assert!(nested_gitdir.exists());
        let worktree = WorktreeInfo {
            name: "deinitialized-nested-module".into(),
            path: path.clone(),
            branch: Some("deinitialized-nested-module".into()),
            base_repo: repo,
        };
        let error = remove_worktree_internal(&worktree, false).unwrap_err();
        assert!(error.contains("uninitialized submodule"), "{error}");
        assert!(path.exists());
        git_cmd(&worktree.base_repo)
            .args([
                "--git-dir",
                &nested_gitdir.to_string_lossy(),
                "cat-file",
                "-e",
                &oid,
            ])
            .run()
            .expect("nested local-only commit remains recoverable");
    }

    #[test]
    fn preservation_refuses_a_main_module_gitfile_pointing_at_the_superproject() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        add_populated_submodule(&repo);
        let path = add_worktree(&repo, "wrong-main-module-gitdir");
        git_cmd(&path)
            .args([
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "update",
                "--init",
            ])
            .run()
            .unwrap();
        let module = path.join("modules/local");
        git_cmd(&module)
            .args(["config", "user.email", "test@test.com"])
            .run()
            .unwrap();
        git_cmd(&module)
            .args(["config", "user.name", "Test"])
            .run()
            .unwrap();
        commit_file(&module, "module-only.txt", "module-only commit\n");
        let oid = rev_at(&module, "HEAD").unwrap();
        git_cmd(&repo)
            .args(["submodule", "deinit", "--force", "modules/local"])
            .run()
            .unwrap();
        let destination = repo.join("modules/local");
        fs::write(
            destination.join(".git"),
            format!("gitdir: {}\n", repo.join(".git").display()),
        )
        .unwrap();
        let reported = rev_at(&destination, "--absolute-git-dir").unwrap();
        #[cfg(unix)]
        let expected = format!("{}/.git", repo.canonicalize().unwrap().display());
        #[cfg(windows)]
        let expected = format!("{}/.git", repo.display().to_string().replace('\\', "/"));
        assert_eq!(reported, expected);
        assert_eq!(
            Path::new(&reported).canonicalize().unwrap(),
            repo.join(".git").canonicalize().unwrap()
        );

        let error = preserve_submodule_refs(&repo, &path, "modules/local").unwrap_err();
        assert!(error.contains("main checkout has no repository"), "{error}");
        let worktree = WorktreeInfo {
            name: "wrong-main-module-gitdir".into(),
            path: path.clone(),
            branch: Some("wrong-main-module-gitdir".into()),
            base_repo: repo.clone(),
        };
        let removal_error = remove_worktree_internal(&worktree, true).unwrap_err();
        assert!(
            removal_error.contains("main checkout has no repository")
                || removal_error.contains("uninitialized submodule"),
            "{removal_error}"
        );
        assert!(path.exists(), "the source worktree must remain available");
        assert!(
            git_cmd(&repo).args(["cat-file", "-e", &oid]).run().is_err(),
            "the module-only commit must not be fetched into the superproject"
        );
    }

    #[test]
    fn missing_worktree_directory_preserves_its_submodule_refs_before_pruning() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        add_populated_submodule(&repo);
        let worktree_path = add_worktree(&repo, "missing-module");
        git_cmd(&worktree_path)
            .args([
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "update",
                "--init",
            ])
            .run()
            .unwrap();
        let module = worktree_path.join("modules/local");
        let module_gitdir = rev_at(&module, "--absolute-git-dir").unwrap();
        assert!(Path::new(&module_gitdir).exists());
        git_cmd(&module)
            .args(["config", "user.email", "test@test.com"])
            .run()
            .unwrap();
        git_cmd(&module)
            .args(["config", "user.name", "Test"])
            .run()
            .unwrap();
        commit_file(&module, "local-only.txt", "local commit\n");
        let local_oid = rev_at(&module, "HEAD").unwrap();
        for index in 1..=2 {
            fs::write(module.join("local-only.txt"), format!("stash {index}\n")).unwrap();
            git_cmd(&module)
                .args(["stash", "push", "-m", &format!("stash {index}")])
                .run()
                .unwrap();
        }
        let stashes = git_cmd(&module)
            .args(["stash", "list", "--format=%H"])
            .run()
            .unwrap()
            .stdout
            .lines()
            .map(str::to_string)
            .collect::<Vec<_>>();
        assert_eq!(stashes.len(), 2);
        commit_file(&module, "reflog-only.txt", "later commit\n");
        let reflog_only = rev_at(&module, "HEAD").unwrap();
        git_cmd(&module)
            .args(["reset", "--hard", "HEAD~1"])
            .run()
            .unwrap();
        assert!(
            git_cmd(&repo.join("modules/local"))
                .args(["cat-file", "-e", &local_oid])
                .run()
                .is_err()
        );
        fs::remove_dir_all(&worktree_path).unwrap();
        let worktree = WorktreeInfo {
            name: "missing-module".into(),
            path: worktree_path,
            branch: Some("missing-module".into()),
            base_repo: repo.clone(),
        };
        let admin = registered_worktree_admin_dir(&repo, &worktree.path)
            .unwrap()
            .expect("missing checkout remains registered");
        let error = remove_worktree_internal(&worktree, false).unwrap_err();
        assert!(error.contains("force"), "{error}");
        assert!(admin.exists(), "refusal must retain the registration");
        assert!(
            Path::new(&module_gitdir).exists(),
            "refusal must retain module objects"
        );
        remove_worktree_internal(&worktree, true).unwrap();
        assert!(!admin.exists());
        assert!(!Path::new(&module_gitdir).exists());
        assert!(
            git_cmd(&repo.join("modules/local"))
                .args(["cat-file", "-e", &local_oid])
                .run()
                .is_ok()
        );
        let refs = git_cmd(&repo.join("modules/local"))
            .args([
                "for-each-ref",
                "--format=%(refname)",
                "--points-at",
                &local_oid,
                "refs/tuic/preserved",
            ])
            .run()
            .unwrap();
        assert!(
            !refs.stdout.trim().is_empty(),
            "local commit needs a durable ref"
        );
        for oid in stashes.iter().chain(std::iter::once(&reflog_only)) {
            git_cmd(&repo.join("modules/local"))
                .args(["cat-file", "-e", oid])
                .run()
                .expect("missing worktree's reflog object remains available");
            let refs = git_cmd(&repo.join("modules/local"))
                .args([
                    "for-each-ref",
                    "--format=%(refname)",
                    "--points-at",
                    oid,
                    "refs/tuic/preserved",
                ])
                .run()
                .unwrap();
            assert!(!refs.stdout.trim().is_empty(), "{oid} has no durable ref");
        }
    }

    #[test]
    fn missing_registered_checkout_requires_force_through_workspace_removal() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let path = add_worktree(&repo, "missing-force");
        fs::remove_dir_all(&path).unwrap();
        let admin = registered_worktree_admin_dir(&repo, &path)
            .unwrap()
            .expect("missing checkout remains registered");

        let error = remove_worktree_by_workspace_id(
            &repo.to_string_lossy(),
            "missing-force",
            false,
            None,
            false,
        )
        .unwrap_err();
        assert!(
            admin.exists(),
            "non-force removal must retain the registration: {error}"
        );

        remove_worktree_by_workspace_id(
            &repo.to_string_lossy(),
            "missing-force",
            false,
            None,
            true,
        )
        .unwrap();
        assert!(
            !admin.exists(),
            "confirmed removal must prune the registration"
        );
    }

    #[test]
    fn missing_registered_checkout_has_a_confirmable_lifecycle() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let path = add_worktree(&repo, "missing-preflight");
        fs::remove_dir_all(&path).unwrap();

        let status = inspect_workspace_lifecycle(&repo, "missing-preflight");
        assert_eq!(status.removal_safety, WorkspaceRemovalSafety::RequiresForce);
        assert_eq!(status.dirty_files, None);
        assert_eq!(status.dirty_fingerprint, None);
        assert_eq!(status.commit_status, WorkspaceCommitStatus::InSync);
        assert!(resolve_any_workspace(&repo, "missing-preflight").is_err());
        assert!(!inspect_workspace_lifecycle(&repo, "not-registered").missing_checkout);
    }

    #[test]
    fn missing_checkout_confirmation_prunes_only_the_confirmed_registration() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let missing = add_worktree(&repo, "missing-confirmed");
        let live = add_worktree(&repo, "live-neighbor");
        fs::remove_dir_all(&missing).unwrap();
        let repo_name = repo.to_string_lossy();

        let wrong_presence = remove_worktree_by_workspace_id_with_missing_confirmation_and_pr(
            &repo_name,
            "live-neighbor",
            false,
            None,
            true,
            false,
            None,
            Some(true),
            |_, _, _| false,
        )
        .unwrap_err();
        assert!(
            wrong_presence.contains("presence changed"),
            "{wrong_presence}"
        );
        assert!(live.exists());

        let outcome = remove_worktree_by_workspace_id_with_missing_confirmation_and_pr(
            &repo_name,
            "missing-confirmed",
            false,
            None,
            true,
            false,
            None,
            Some(true),
            |_, _, _| false,
        )
        .unwrap();
        assert_eq!(outcome.branch, "missing-confirmed");
        assert!(
            registered_worktree_admin_dir(&repo, &missing)
                .unwrap()
                .is_none()
        );
        assert!(live.exists());
    }

    #[test]
    fn live_checkout_force_still_requires_its_fingerprint() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let live = add_worktree(&repo, "live-fingerprint");
        fs::write(live.join("uncommitted.txt"), "keep me\n").unwrap();

        let error = remove_worktree_by_workspace_id_with_missing_confirmation_and_pr(
            &repo.to_string_lossy(),
            "live-fingerprint",
            false,
            None,
            true,
            false,
            None,
            Some(false),
            |_, _, _| false,
        )
        .unwrap_err();
        assert!(error.contains("expected_fingerprint"), "{error}");
        assert_eq!(
            fs::read_to_string(live.join("uncommitted.txt")).unwrap(),
            "keep me\n"
        );
    }

    #[test]
    fn missing_checkout_cleanup_does_not_require_its_deleted_archive_script_directory() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let missing = add_worktree(&repo, "missing-with-script");
        fs::remove_dir_all(&missing).unwrap();

        remove_worktree_by_workspace_id_with_missing_confirmation_and_pr(
            &repo.to_string_lossy(),
            "missing-with-script",
            false,
            Some("printf 'would run only in a live checkout'"),
            true,
            false,
            None,
            Some(true),
            |_, _, _| false,
        )
        .unwrap();
        assert!(
            registered_worktree_admin_dir(&repo, &missing)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn missing_locked_checkout_requires_a_separate_lock_override() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let path = add_worktree(&repo, "missing-locked");
        git_cmd(&repo)
            .args(["worktree", "lock", &path.to_string_lossy()])
            .run()
            .unwrap();
        fs::remove_dir_all(&path).unwrap();
        let admin = registered_worktree_admin_dir(&repo, &path)
            .unwrap()
            .expect("locked checkout remains registered");

        let error = remove_worktree_by_workspace_id_with_lock(
            &repo.to_string_lossy(),
            "missing-locked",
            false,
            None,
            true,
            false,
        )
        .unwrap_err();
        assert!(error.starts_with(LOCKED_WORKTREE_PREFIX), "{error}");
        assert!(admin.exists(), "lock refusal must retain the registration");

        remove_worktree_by_workspace_id_with_lock(
            &repo.to_string_lossy(),
            "missing-locked",
            false,
            None,
            true,
            true,
        )
        .unwrap();
        assert!(!admin.exists(), "lock override must prune the registration");
    }

    #[test]
    fn force_refuses_when_the_confirmed_worktree_state_changes() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let worktree = add_worktree(&repo, "stale-confirmation");
        fs::write(worktree.join("first.txt"), "first\n").unwrap();
        let confirmed = inspect_workspace_lifecycle(&repo, "stale-confirmation")
            .dirty_fingerprint
            .unwrap();
        fs::write(worktree.join("second.txt"), "second\n").unwrap();

        let error = remove_worktree_by_workspace_id_with_confirmation(
            &repo.to_string_lossy(),
            "stale-confirmation",
            false,
            None,
            true,
            false,
            Some(&confirmed),
        )
        .unwrap_err();
        assert!(error.contains("changed since confirmation"), "{error}");
        assert!(worktree.join("first.txt").exists());
        assert!(worktree.join("second.txt").exists());
    }

    #[test]
    fn force_refuses_new_submodule_commit_even_when_dirty_file_count_is_unchanged() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        add_populated_submodule(&repo);
        let worktree = add_worktree(&repo, "stale-module");
        git_cmd(&worktree)
            .args([
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "update",
                "--init",
            ])
            .run()
            .unwrap();
        let module = worktree.join("modules/local");
        git_cmd(&module)
            .args(["config", "user.email", "test@test.com"])
            .run()
            .unwrap();
        git_cmd(&module)
            .args(["config", "user.name", "Test"])
            .run()
            .unwrap();
        commit_file(&module, "first.txt", "first\n");
        let confirmed = inspect_workspace_lifecycle(&repo, "stale-module");
        assert_eq!(confirmed.dirty_files, Some(1));
        assert!(
            confirmed
                .submodule_unpushed_commits
                .iter()
                .any(|entry| entry.path == "modules/local" && entry.count > 0)
        );
        commit_file(&module, "second.txt", "second\n");
        assert_eq!(dirty_files_at(&worktree).unwrap(), 1);

        let error = remove_worktree_by_workspace_id_with_confirmation(
            &repo.to_string_lossy(),
            "stale-module",
            false,
            None,
            true,
            false,
            confirmed.dirty_fingerprint.as_deref(),
        )
        .unwrap_err();
        assert!(error.contains("changed since confirmation"), "{error}");
        assert!(worktree.exists());
    }

    #[test]
    fn local_submodule_commit_blocks_non_force_removal_and_keeps_gitdir() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        add_populated_submodule(&repo);
        let worktree = add_worktree(&repo, "modified-module");
        git_cmd(&worktree)
            .args([
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "update",
                "--init",
            ])
            .run()
            .unwrap();
        let module = worktree.join("modules/local");
        git_cmd(&module)
            .args(["config", "user.email", "test@test.com"])
            .run()
            .unwrap();
        git_cmd(&module)
            .args(["config", "user.name", "Test"])
            .run()
            .unwrap();
        commit_file(&module, "local.txt", "only in this checkout\n");
        let gitdir = fs::read_to_string(module.join(".git")).unwrap();
        let gitdir = gitdir.trim().strip_prefix("gitdir: ").unwrap();
        let gitdir = module.join(gitdir);
        assert!(gitdir.exists());

        let error = remove_worktree_by_workspace_id(
            &repo.to_string_lossy(),
            "modified-module",
            true,
            None,
            false,
        )
        .unwrap_err();
        assert!(
            error.contains("uncommitted") || error.contains("submodule"),
            "{error}"
        );
        assert!(worktree.exists());
        assert!(gitdir.exists(), "local-only commit must stay reachable");
    }

    fn commit_file(dir: &Path, name: &str, body: &str) {
        fs::write(dir.join(name), body).expect("write file");
        git_cmd(dir).args(["add", "."]).run().expect("git add");
        git_cmd(dir)
            .args(["commit", "-m", name])
            .run()
            .expect("git commit");
    }

    /// A branch sitting exactly on the default branch's tip never merged
    /// anything: it has no commits of its own. `--is-ancestor` is satisfied
    /// here and by a genuinely merged branch alike, and reporting both as
    /// merged told Boss that a worktree holding 429 uncommitted lines had
    /// nothing to lose.
    #[test]
    fn a_workspace_on_the_default_tip_is_in_sync_not_merged() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        add_worktree(&repo, "untouched");

        let status = inspect_workspace_lifecycle(&repo, "untouched");

        assert_eq!(status.commit_status, WorkspaceCommitStatus::InSync);
        assert_eq!(status.dirty_files, Some(0));
        assert_eq!(status.removal_safety, WorkspaceRemovalSafety::Safe);
    }

    /// A branch created at an older main tip has no work of its own even after
    /// main advances. Its removal can still be confirmed explicitly.
    #[test]
    fn a_workspace_behind_the_default_tip_without_own_commits_is_in_sync() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let worktree = add_worktree(&repo, "trails");
        commit_file(&repo, "moved-on.txt", "default branch advanced\n");

        let status = inspect_workspace_lifecycle(&repo, "trails");

        assert_eq!(status.commit_status, WorkspaceCommitStatus::InSync);
        assert_eq!(status.dirty_files, Some(0));
        assert!(worktree.exists());
        let outcome =
            remove_worktree_by_workspace_id(&repo.to_string_lossy(), "trails", true, None, false)
                .unwrap();
        assert_eq!(outcome.removal_rule, "in_sync");
    }

    #[test]
    fn a_branch_with_own_commits_merged_into_main_remains_eligible_for_cleanup() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let worktree = add_worktree(&repo, "completed-feature");
        commit_file(&worktree, "feature.txt", "completed work\n");
        git_cmd(&repo)
            .args(["merge", "completed-feature", "--no-edit"])
            .run()
            .unwrap();
        commit_file(&repo, "later.txt", "main advanced\n");

        let status = inspect_workspace_lifecycle(&repo, "completed-feature");

        assert_eq!(status.commit_status, WorkspaceCommitStatus::Merged);
        assert_eq!(status.merge_proof, Some("ancestry"));
        assert!(worktree.exists());
    }

    #[test]
    fn fast_forward_merge_of_own_commits_is_merged_even_at_the_default_tip() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let worktree = add_worktree(&repo, "fast-forwarded");
        commit_file(&worktree, "feature.txt", "completed work\n");
        git_cmd(&repo)
            .args(["merge", "--ff-only", "fast-forwarded"])
            .run()
            .unwrap();

        let status = inspect_workspace_lifecycle(&repo, "fast-forwarded");

        assert_eq!(status.commit_status, WorkspaceCommitStatus::Merged);
    }

    #[test]
    fn branch_fast_forwarded_to_new_main_without_own_commits_stays_in_sync() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let worktree = add_worktree(&repo, "following-main");
        commit_file(&repo, "new.txt", "main moved\n");
        git_cmd(&worktree)
            .args(["merge", "--ff-only", &base_branch_of(&repo)])
            .run()
            .unwrap();

        let status = inspect_workspace_lifecycle(&repo, "following-main");

        assert_eq!(status.commit_status, WorkspaceCommitStatus::InSync);
    }

    #[test]
    fn branch_pulled_to_new_main_without_own_commits_stays_in_sync() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let worktree = add_worktree(&repo, "pulling-main");
        commit_file(&repo, "new.txt", "main moved\n");
        git_cmd(&worktree)
            .args([
                "pull",
                "--ff-only",
                &repo.to_string_lossy(),
                &base_branch_of(&repo),
            ])
            .run()
            .unwrap();

        let status = inspect_workspace_lifecycle(&repo, "pulling-main");

        assert_eq!(status.commit_status, WorkspaceCommitStatus::InSync);
    }

    #[test]
    fn branch_tracking_main_and_pulled_without_own_commits_stays_in_sync() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let worktree = add_worktree(&repo, "tracking-main");
        git_cmd(&repo)
            .args(["remote", "add", "origin", &repo.to_string_lossy()])
            .run()
            .unwrap();
        git_cmd(&repo)
            .args(["fetch", "origin", &base_branch_of(&repo)])
            .run()
            .unwrap();
        git_cmd(&worktree)
            .args([
                "branch",
                "--set-upstream-to",
                &format!("origin/{}", base_branch_of(&repo)),
            ])
            .run()
            .unwrap();
        commit_file(&repo, "new.txt", "main moved\n");
        git_cmd(&worktree).args(["pull"]).run().unwrap();

        let status = inspect_workspace_lifecycle(&repo, "tracking-main");

        assert_eq!(status.commit_status, WorkspaceCommitStatus::InSync);
    }

    #[test]
    fn branch_pulled_from_non_default_upstream_is_merged_after_integration() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let source = add_worktree(&repo, "source-work");
        commit_file(&source, "feature.txt", "source work\n");
        let tracking = add_worktree(&repo, "tracking-source");
        git_cmd(&repo)
            .args(["remote", "add", "origin", &repo.to_string_lossy()])
            .run()
            .unwrap();
        git_cmd(&repo)
            .args(["fetch", "origin", "source-work"])
            .run()
            .unwrap();
        git_cmd(&tracking)
            .args(["branch", "--set-upstream-to", "origin/source-work"])
            .run()
            .unwrap();
        git_cmd(&tracking).args(["pull"]).run().unwrap();
        git_cmd(&repo)
            .args(["merge", "--ff-only", "source-work"])
            .run()
            .unwrap();

        let status = inspect_workspace_lifecycle(&repo, "tracking-source");

        assert_eq!(status.commit_status, WorkspaceCommitStatus::Merged);
    }

    #[test]
    fn branch_rebased_to_new_main_without_own_commits_stays_in_sync() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let worktree = add_worktree(&repo, "rebasing-main");
        commit_file(&repo, "new.txt", "main moved\n");
        git_cmd(&worktree)
            .args(["rebase", &base_branch_of(&repo)])
            .run()
            .unwrap();

        let status = inspect_workspace_lifecycle(&repo, "rebasing-main");

        assert_eq!(status.commit_status, WorkspaceCommitStatus::InSync);
    }

    #[test]
    fn branch_created_from_non_default_commits_is_merged_even_without_later_edits() {
        let (_temp, repo, workspaces) = workspace_fixture();
        let source = add_worktree(&repo, "source-branch");
        commit_file(&source, "source.txt", "source work\n");
        let derived = workspaces.join("derived");
        fs::create_dir_all(&workspaces).unwrap();
        git_cmd(&repo)
            .args([
                "worktree",
                "add",
                "-b",
                "derived",
                &derived.to_string_lossy(),
                "source-branch",
            ])
            .run()
            .unwrap();
        git_cmd(&repo)
            .args(["merge", "--ff-only", "source-branch"])
            .run()
            .unwrap();

        let status = inspect_workspace_lifecycle(&repo, "derived");

        assert_eq!(status.commit_status, WorkspaceCommitStatus::Merged);
    }

    #[test]
    fn a_workspace_with_its_own_commit_is_unmerged() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let worktree = add_worktree(&repo, "diverged");
        commit_file(&worktree, "own.txt", "only here\n");

        let status = inspect_workspace_lifecycle(&repo, "diverged");

        assert_eq!(status.commit_status, WorkspaceCommitStatus::Unmerged);
        assert_eq!(status.removal_safety, WorkspaceRemovalSafety::Safe);
    }

    #[test]
    fn a_squashed_pr_with_a_merge_commit_needs_github_proof() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        git_cmd(&repo).args(["branch", "-M", "main"]).run().unwrap();
        let worktree = add_worktree(&repo, "pr-squashed");
        commit_file(&worktree, "feature.txt", "feature\n");
        commit_file(&repo, "base.txt", "base changed\n");
        git_cmd(&worktree)
            .args(["merge", "main", "--no-edit"])
            .run()
            .unwrap();
        let local_tip = rev_at(&worktree, "HEAD").unwrap();
        commit_file(&worktree, "pr-extra.txt", "included in PR\n");
        let pr_head = rev_at(&worktree, "HEAD").unwrap();
        git_cmd(&worktree)
            .args(["reset", "--hard", &local_tip])
            .run()
            .unwrap();
        // The squash on the default branch need not preserve the PR's patches.
        commit_file(&repo, "squashed.txt", "squash result\n");
        let api = serde_json::json!([{"data": {"repository": {"pullRequests": {"nodes": [{
            "number": 42,
            "state": "MERGED",
            "headRefName": "pr-squashed",
            "headRefOid": pr_head
        }]}}}}]);

        let status = inspect_workspace_lifecycle_with_pr_fixture(&repo, "pr-squashed", &api);

        assert_eq!(status.commit_status, WorkspaceCommitStatus::Merged);
        let outcome = remove_worktree_by_workspace_id_with_confirmation_and_pr(
            &repo.to_string_lossy(),
            "pr-squashed",
            true,
            None,
            false,
            false,
            None,
            |repo, branch, tip| merged_pr_proof_from_pages(repo, branch, tip, &api),
        )
        .unwrap();
        assert_eq!(outcome.removal_rule, "github_pr");
        assert!(!worktree.exists());
    }

    #[test]
    fn a_branch_contained_in_the_checked_out_integration_branch_is_merged() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        git_cmd(&repo).args(["branch", "-M", "main"]).run().unwrap();
        git_cmd(&repo)
            .args([
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/main",
            ])
            .run()
            .unwrap();
        git_cmd(&repo)
            .args(["checkout", "-b", "integration"])
            .run()
            .unwrap();
        let worktree = add_worktree(&repo, "integrated-feature");
        commit_file(&worktree, "feature.txt", "feature\n");
        git_cmd(&repo)
            .args(["merge", "integrated-feature", "--no-edit"])
            .run()
            .unwrap();
        commit_file(&repo, "later.txt", "integration advanced\n");

        let status = inspect_workspace_lifecycle(&repo, "integrated-feature");

        assert_eq!(status.commit_status, WorkspaceCommitStatus::Merged);
        let outcome = remove_worktree_by_workspace_id(
            &repo.to_string_lossy(),
            "integrated-feature",
            true,
            None,
            false,
        )
        .unwrap();
        assert_eq!(outcome.removal_rule, "integration_ancestry");
    }

    fn inspect_workspace_lifecycle_with_pr_fixture(
        repo: &Path,
        workspace_id: &str,
        api: &serde_json::Value,
    ) -> WorkspaceLifecycleStatus {
        inspect_workspace_lifecycle_with_pr(repo, workspace_id, |repo, branch, tip| {
            merged_pr_proof_from_pages(repo, branch, tip, api)
        })
    }

    #[test]
    fn github_pr_proof_rejects_open_closed_and_stale_heads() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let worktree = add_worktree(&repo, "pr-safety");
        let base = rev_at(&repo, "HEAD").unwrap();
        commit_file(&worktree, "feature.txt", "feature\n");
        let tip = rev_at(&worktree, "HEAD").unwrap();
        for (name, state, head_branch, head_sha) in [
            ("open", "OPEN", "pr-safety", tip.as_str()),
            ("closed", "CLOSED", "pr-safety", tip.as_str()),
            ("wrong branch", "MERGED", "other-branch", tip.as_str()),
            ("tip ahead", "MERGED", "pr-safety", base.as_str()),
        ] {
            let api = serde_json::json!([{"data": {"repository": {"pullRequests": {"nodes": [{
                "number": 42,
                "state": state,
                "headRefName": head_branch,
                "headRefOid": head_sha
            }]}}}}]);
            let status = inspect_workspace_lifecycle_with_pr_fixture(&repo, "pr-safety", &api);
            assert_eq!(
                status.commit_status,
                WorkspaceCommitStatus::Unmerged,
                "{name}"
            );
        }
        assert!(worktree.exists());
    }

    #[test]
    fn github_pr_proof_rejects_a_partial_graphql_error() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let worktree = add_worktree(&repo, "pr-error");
        commit_file(&worktree, "feature.txt", "feature\n");
        let tip = rev_at(&worktree, "HEAD").unwrap();
        let api = serde_json::json!([{
            "errors": [{"message": "partial response"}],
            "data": {"repository": {"pullRequests": {"nodes": [{
                "number": 42,
                "state": "MERGED",
                "headRefName": "pr-error",
                "headRefOid": tip
            }]}}}
        }]);

        let status = inspect_workspace_lifecycle_with_pr_fixture(&repo, "pr-error", &api);
        assert_eq!(status.commit_status, WorkspaceCommitStatus::Unmerged);
    }

    #[test]
    fn github_pr_proof_fetches_the_head_and_checks_the_api_sha() {
        let (temp, repo, _workspaces) = workspace_fixture();
        let remote = temp.path().join("remote.git");
        git_cmd(temp.path())
            .args(["init", "--bare", remote.to_str().unwrap()])
            .run()
            .unwrap();
        git_cmd(&repo)
            .args(["remote", "add", "origin", remote.to_str().unwrap()])
            .run()
            .unwrap();
        let worktree = add_worktree(&repo, "pr-fetch");
        commit_file(&worktree, "feature.txt", "feature\n");
        git_cmd(&worktree)
            .args(["push", "origin", "HEAD:refs/heads/pr-source"])
            .run()
            .unwrap();
        let upstream = temp.path().join("upstream");
        git_cmd(temp.path())
            .args([
                "clone",
                remote.to_str().unwrap(),
                upstream.to_str().unwrap(),
            ])
            .run()
            .unwrap();
        git_cmd(&upstream)
            .args(["config", "user.email", "test@test.com"])
            .run()
            .unwrap();
        git_cmd(&upstream)
            .args(["config", "user.name", "Test"])
            .run()
            .unwrap();
        git_cmd(&upstream)
            .args(["checkout", "-b", "pr-head", "origin/pr-source"])
            .run()
            .unwrap();
        commit_file(&upstream, "extra.txt", "PR advanced\n");
        let head = rev_at(&upstream, "HEAD").unwrap();
        git_cmd(&upstream)
            .args(["push", "origin", "HEAD:refs/pull/42/head"])
            .run()
            .unwrap();
        assert!(
            git_cmd(&repo)
                .args(["cat-file", "-e", &format!("{head}^{{commit}}")])
                .run_silent()
                .is_none()
        );
        let api = |sha: &str| {
            serde_json::json!([{"data": {"repository": {"pullRequests": {"nodes": [{
                "number": 42,
                "state": "MERGED",
                "headRefName": "pr-fetch",
                "headRefOid": sha
            }]}}}}])
        };

        let status = inspect_workspace_lifecycle_with_pr_fixture(&repo, "pr-fetch", &api(&head));
        assert_eq!(status.commit_status, WorkspaceCommitStatus::Merged);
        commit_file(&upstream, "not-in-pr.txt", "newer\n");
        let stale_api_head = rev_at(&upstream, "HEAD").unwrap();
        let status =
            inspect_workspace_lifecycle_with_pr_fixture(&repo, "pr-fetch", &api(&stale_api_head));
        assert_eq!(status.commit_status, WorkspaceCommitStatus::Unmerged);
    }

    // Audit cases: wiz POC-00168/170 landed together in squash 4a190ec8;
    // tuic backup/worktree-v4 has revised twins; parked WIP has unique files.
    fn integration_1295_fixture(message: bool, unique: bool) -> (TempDir, PathBuf, PathBuf) {
        let (temp, repo, _) = workspace_fixture();
        git_cmd(&repo).args(["branch", "-M", "main"]).run().unwrap();
        let wt = add_worktree(&repo, "audit-1295");
        commit_file(&wt, "first.txt", "first landed change\n");
        commit_file(&wt, "second.txt", "second landed change\n");
        fs::write(repo.join("first.txt"), "first landed change\n").unwrap();
        fs::write(repo.join("second.txt"), "second landed change\n").unwrap();
        git_cmd(&repo).args(["add", "."]).run().unwrap();
        git_cmd(&repo)
            .args([
                "commit",
                "-m",
                if message {
                    "release squash\n\nfirst.txt\nsecond.txt"
                } else {
                    "revised implementation"
                },
            ])
            .run()
            .unwrap();
        if unique {
            commit_file(&wt, "unique.txt", "unlanded work\n");
        }
        (temp, repo, wt)
    }

    #[test]
    fn content_heuristic_rejects_partial_binary_and_deleted_changes_1295() {
        let mut failures = Vec::new();
        for (name, branch, target) in [
            ("partial", "rule A\nrule B\nunique\n", "rule B\nrule A\n"),
            ("duplicate", "rule A\nrule A\nrule B\n", "rule B\nrule A\n"),
            ("binary", "rule A\0rule B\n", "rule B\0rule A\n"),
            ("no-newline", "rule A\nrule B", "rule B\nrule A\n"),
        ] {
            let (_temp, repo, _) = workspace_fixture();
            git_cmd(&repo).args(["branch", "-M", "main"]).run().unwrap();
            let wt = add_worktree(&repo, name);
            commit_file(&wt, "rules.txt", branch);
            commit_file(&repo, "rules.txt", target);
            if branch_integration_with_pr(&repo, name, |_, _, _| false)
                .unwrap()
                .integrated
            {
                failures.push(name);
            }
        }
        // Audit WIP: removing a base file is unique work even when added lines exist.
        let (_temp, repo, _) = workspace_fixture();
        git_cmd(&repo).args(["branch", "-M", "main"]).run().unwrap();
        let wt = add_worktree(&repo, "deleted-1295");
        fs::remove_file(wt.join("README.md")).unwrap();
        commit_file(&wt, "rules.txt", "rule A\nrule B\n");
        commit_file(&repo, "rules.txt", "rule B\nrule A\n");
        if branch_integration_with_pr(&repo, "deleted-1295", |_, _, _| false)
            .unwrap()
            .integrated
        {
            failures.push("deleted");
        }
        assert!(
            failures.is_empty(),
            "false integration proofs: {failures:?}"
        );
    }

    #[test]
    fn literal_path_content_proof_preserves_tip_after_worktree_removal_1295() {
        let (_temp, repo, _) = workspace_fixture();
        git_cmd(&repo).args(["branch", "-M", "main"]).run().unwrap();
        let wt = add_worktree(&repo, "literal-1295");
        // Brackets are legal across platforms. Do not interpret them as a glob.
        let name = "[literal].txt";
        commit_file(&wt, name, "rule A\n++literal\nrule B\n");
        commit_file(&repo, name, "rule B\nrule A\n++literal\nextra\n");
        let query = branch_integration_with_pr(&repo, "literal-1295", |_, _, _| false).unwrap();
        assert_eq!(query.proof, Some("content_superset"));
        #[cfg(unix)]
        let expected = format!(
            "{}/literal-1295",
            _temp.path().canonicalize().unwrap().display()
        );
        #[cfg(windows)]
        let expected = format!(
            "{}/literal-1295",
            _temp.path().display().to_string().replace('\\', "/")
        );
        assert_eq!(query.worktree_paths, vec![expected]);
        assert_eq!(
            Path::new(&query.worktree_paths[0]).canonicalize().unwrap(),
            wt.canonicalize().unwrap()
        );
        git_cmd(&repo)
            .args(["update-ref", &query.archive_ref, &query.tip])
            .run()
            .unwrap();
        let outcome = remove_worktree_by_workspace_id(
            &repo.to_string_lossy(),
            "literal-1295",
            true,
            None,
            false,
        )
        .unwrap();
        assert_eq!(outcome.removal_rule, "content_superset");
        assert!(!wt.exists());
        assert!(rev_at(&repo, "refs/heads/literal-1295").is_err());
        assert_eq!(rev_at(&repo, &query.archive_ref).unwrap(), query.tip);
    }

    /// Catches: a tip archived at the tip-suffixed ref (reused branch name) reading
    /// "not archived", which blocks the content-proof deletion of preserved work.
    #[test]
    fn tip_archived_at_the_suffixed_ref_counts_as_archived_1462() {
        let (_temp, repo, _) = workspace_fixture();
        git_cmd(&repo).args(["branch", "-M", "main"]).run().unwrap();
        let wt = add_worktree(&repo, "suffixed-1462");
        let name = "[literal].txt";
        commit_file(&wt, name, "rule A\n++literal\nrule B\n");
        commit_file(&repo, name, "rule B\nrule A\n++literal\nextra\n");
        let query = branch_integration_with_pr(&repo, "suffixed-1462", |_, _, _| false).unwrap();
        assert_eq!(query.proof, Some("content_superset"));
        assert!(!query.archived);
        let suffixed = format!("{}-{}", query.archive_ref, &query.tip[..7]);
        git_cmd(&repo)
            .args(["update-ref", &suffixed, &query.tip])
            .run()
            .unwrap();
        git_cmd(&repo)
            .args(["worktree", "remove", "--force", &wt.to_string_lossy()])
            .run()
            .unwrap();
        let after = branch_integration_with_pr(&repo, "suffixed-1462", |_, _, _| false).unwrap();
        assert!(after.archived);
        assert_eq!(after.archive_ref, suffixed);
        assert_eq!(
            delete_integrated_local_branch(&repo.to_string_lossy(), "suffixed-1462").unwrap(),
            "content_superset"
        );
        assert!(rev_at(&repo, "refs/heads/suffixed-1462").is_err());
    }

    /// Catches a branch filter that lists checkouts of other branches: those
    /// paths feed removal and archive decisions.
    #[test]
    fn worktree_paths_list_only_checkouts_of_the_queried_branch_1327() {
        let (_temp, repo, _) = workspace_fixture();
        git_cmd(&repo).args(["branch", "-M", "main"]).run().unwrap();
        let wt_a = add_worktree(&repo, "alpha-1327");
        let wt_b = add_worktree(&repo, "beta-1327");
        for (branch, own) in [("alpha-1327", &wt_a), ("beta-1327", &wt_b)] {
            let query = branch_integration_with_pr(&repo, branch, |_, _, _| false).unwrap();
            #[cfg(unix)]
            let expected = format!(
                "{}/{branch}",
                _temp.path().canonicalize().unwrap().display()
            );
            #[cfg(windows)]
            let expected = format!(
                "{}/{branch}",
                _temp.path().display().to_string().replace('\\', "/")
            );
            assert_eq!(query.worktree_paths, vec![expected]);
            assert_eq!(
                Path::new(&query.worktree_paths[0]).canonicalize().unwrap(),
                own.canonicalize().unwrap()
            );
        }
    }

    #[test]
    fn branch_panel_and_safe_ui_delete_use_squash_proof_1295() {
        let (_temp, repo, wt) = integration_1295_fixture(true, false);
        let merged = crate::git::get_merged_branches_impl(&repo).unwrap();
        assert!(merged.contains(&"audit-1295".into()), "{merged:?}");
        git_cmd(&repo)
            .args(["worktree", "remove", &wt.to_string_lossy()])
            .run()
            .unwrap();
        assert!(
            crate::git::delete_branch_impl(&repo.to_string_lossy(), "audit-1295", false)
                .unwrap()
                .deleted
        );
        assert!(rev_at(&repo, "refs/heads/audit-1295").is_err());
    }

    #[test]
    fn diff_header_like_unique_lines_are_not_ignored_1295() {
        let (_temp, repo, _) = workspace_fixture();
        git_cmd(&repo).args(["branch", "-M", "main"]).run().unwrap();
        let wt = add_worktree(&repo, "header-lines-1295");
        commit_file(&wt, "rules.txt", "rule A\nrule B\n++unique unlanded line\n");
        commit_file(&repo, "rules.txt", "rule B\nrule A\nextra rule\n");
        let query =
            branch_integration_with_pr(&repo, "header-lines-1295", |_, _, _| false).unwrap();
        assert!(!query.integrated, "{query:?}");
        assert!(
            remove_worktree_by_workspace_id(
                &repo.to_string_lossy(),
                "header-lines-1295",
                true,
                None,
                false
            )
            .is_err()
        );
        assert!(wt.exists());
    }

    #[test]
    fn archive_hook_cannot_remove_content_proof_recovery_ref_1295() {
        let (_temp, repo, _) = workspace_fixture();
        git_cmd(&repo).args(["branch", "-M", "main"]).run().unwrap();
        let wt = add_worktree(&repo, "hook-1295");
        commit_file(&wt, "rules.txt", "rule A\nrule B\n");
        commit_file(&repo, "rules.txt", "rule B\nrule A\nextra rule\n");
        let tip = rev_at(&repo, "refs/heads/hook-1295").unwrap();
        git_cmd(&repo)
            .args(["update-ref", "refs/archive/hook-1295", &tip])
            .run()
            .unwrap();
        let outcome = remove_worktree_by_workspace_id(
            &repo.to_string_lossy(),
            "hook-1295",
            true,
            Some("git update-ref -d refs/archive/hook-1295"),
            false,
        )
        .unwrap();
        assert!(outcome.branch_delete_warning.is_some(), "{outcome:?}");
        assert_eq!(rev_at(&repo, "refs/heads/hook-1295").unwrap(), tip);
    }

    #[test]
    fn audited_same_subject_twin_survives_later_refactoring_1295() {
        // Reduced c6585c43e -> main twin from backup/worktree-v4-466eb71a.
        // The audit compares added/deleted lines of same-subject commits,
        // not just whether those lines still exist after main refactors.
        let (_temp, repo, _) = workspace_fixture();
        git_cmd(&repo).args(["branch", "-M", "main"]).run().unwrap();
        commit_file(&repo, "warm.rs", "old context\nold warmth\nlast context\n");
        let wt = add_worktree(&repo, "twin-1295");
        let subject = "fix(worktree): report pending warmth and serialize setup (closes #920-9947)";
        commit_file(
            &wt,
            "warm.rs",
            "old context\nuse std::sync::LazyLock;\nstatic NEXT_WARM_TOKEN: AtomicU64 = AtomicU64::new(1);\nlast context\n",
        );
        git_cmd(&wt)
            .args(["commit", "--amend", "-m", subject])
            .run()
            .unwrap();
        commit_file(
            &repo,
            "warm.rs",
            "new context\nold warmth\nchanged context\n",
        );
        commit_file(
            &repo,
            "warm.rs",
            "new context\nuse std::sync::LazyLock;\nstatic NEXT_WARM_TOKEN: AtomicU64 = AtomicU64::new(1);\nchanged context\n",
        );
        git_cmd(&repo)
            .args(["commit", "--amend", "-m", subject])
            .run()
            .unwrap();
        commit_file(
            &repo,
            "warm.rs",
            "new context\nstatic NEXT_WARM_TOKEN: AtomicU64 = AtomicU64::new(1);\nuse std::sync::LazyLock;\nchanged context\n",
        );
        let query = branch_integration_with_pr(&repo, "twin-1295", |_, _, _| false).unwrap();
        assert_eq!(query.proof, Some("content_superset"), "{query:?}");
        assert!(query.archive_required);
        assert!(
            remove_worktree_by_workspace_id(
                &repo.to_string_lossy(),
                "twin-1295",
                true,
                None,
                false
            )
            .is_err()
        );
        assert!(wt.exists());
    }

    #[test]
    fn content_superset_requires_current_archive_before_deletion_1295() {
        // Audit: revised worktree twins have the same additions in new context.
        let (_temp, repo, _) = workspace_fixture();
        git_cmd(&repo).args(["branch", "-M", "main"]).run().unwrap();
        let wt = add_worktree(&repo, "content-1295");
        commit_file(&wt, "rules.txt", "rule A\nrule B\n");
        commit_file(&repo, "rules.txt", "rule B\nrule A\nextra rule\n");
        let tip = rev_at(&repo, "refs/heads/content-1295").unwrap();
        let status = inspect_workspace_lifecycle(&repo, "content-1295");
        assert_eq!(status.commit_status, WorkspaceCommitStatus::Merged);
        assert_eq!(status.merge_proof, Some("content_superset"));
        assert!(
            remove_worktree_by_workspace_id(
                &repo.to_string_lossy(),
                "content-1295",
                true,
                None,
                false
            )
            .is_err()
        );
        assert!(wt.exists());
        git_cmd(&repo)
            .args(["worktree", "remove", &wt.to_string_lossy()])
            .run()
            .unwrap();
        assert!(delete_integrated_local_branch(&repo.to_string_lossy(), "content-1295").is_err());
        assert_eq!(rev_at(&repo, "refs/heads/content-1295").unwrap(), tip);
        git_cmd(&repo)
            .args(["update-ref", "refs/archive/content-1295", "main"])
            .run()
            .unwrap();
        assert!(delete_integrated_local_branch(&repo.to_string_lossy(), "content-1295").is_err());
        assert_eq!(rev_at(&repo, "refs/heads/content-1295").unwrap(), tip);
        git_cmd(&repo)
            .args(["update-ref", "refs/archive/content-1295", &tip])
            .run()
            .unwrap();
        assert_eq!(
            delete_integrated_local_branch(&repo.to_string_lossy(), "content-1295").unwrap(),
            "content_superset"
        );
        assert_eq!(rev_at(&repo, "refs/archive/content-1295").unwrap(), tip);
    }

    #[test]
    fn github_squash_message_bullets_report_the_real_audit_proof_1295() {
        // Verbatim excerpt of wiz 4a190ec8, PR #189 from the audit.
        let (_temp, repo, wt) = integration_1295_fixture(false, false);
        git_cmd(&wt)
            .args([
                "commit",
                "--amend",
                "-m",
                "fix(hud): add Opus 4.7 to pricing table (#343-d76a)",
            ])
            .run()
            .unwrap();
        // A one-subject release still corroborates the structural no-op proof.
        git_cmd(&repo).args(["commit", "--amend", "-m", "Poc 00170/wiz 5.0 pre (#189)\n\n* first.txt\n\n* fix(hud): add Opus 4.7 to pricing table (#343-d76a)"]).run().unwrap();
        let query = branch_integration_with_pr(&repo, "audit-1295", |_, _, _| false).unwrap();
        assert_eq!(query.proof, Some("squash_message"));
    }

    #[test]
    fn branch_integrations_look_up_pull_requests_concurrently_in_listing_order_1295() {
        // Catches: a sequential listing pays every GitHub lookup in turn, so a
        // repo with dozens of unmerged branches outlasts the MCP timeout.
        let (_temp, repo, _wt) = integration_1295_fixture(true, true);
        for n in 0..8 {
            git_cmd(&repo)
                .args(["branch", &format!("extra-1295-{n}"), "audit-1295"])
                .run()
                .unwrap();
        }
        let in_flight = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let listed = branch_integrations_with_pr(&repo, |_, _, _| {
            let now = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(now, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(100));
            in_flight.fetch_sub(1, Ordering::SeqCst);
            false
        })
        .unwrap();
        assert!(
            peak.load(Ordering::SeqCst) > 1,
            "pull request lookups ran one at a time"
        );
        let expected = git_cmd(&repo)
            .args(["for-each-ref", "--format=%(refname:strip=2)", "refs/heads/"])
            .run()
            .unwrap()
            .stdout;
        assert_eq!(
            listed
                .iter()
                .map(|entry| entry.branch.as_str())
                .collect::<Vec<_>>(),
            expected.lines().collect::<Vec<_>>()
        );
    }

    fn add_extra_branches_1295(repo: &Path, count: usize) {
        for n in 0..count {
            git_cmd(repo)
                .args(["branch", &format!("extra-1295-{n:02}"), "audit-1295"])
                .run()
                .unwrap();
        }
    }

    fn listed_branches_1295(repo: &Path) -> Vec<String> {
        git_cmd(repo)
            .args(["for-each-ref", "--format=%(refname:strip=2)", "refs/heads/"])
            .run()
            .unwrap()
            .stdout
            .lines()
            .map(String::from)
            .collect()
    }

    #[test]
    fn branch_integrations_never_exceed_the_worker_bound_and_look_up_each_branch_once_1295() {
        // Catches: one thread per branch (unbounded fan-out hammers `gh` and trips
        // its rate limit), and a worker index race that skips or repeats a branch.
        let (_temp, repo, _wt) = integration_1295_fixture(true, true);
        add_extra_branches_1295(&repo, 18);
        let in_flight = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let calls = std::sync::Mutex::new(Vec::<String>::new());
        let listed = branch_integrations_with_pr(&repo, |_, branch, _| {
            let now = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(now, Ordering::SeqCst);
            calls.lock().unwrap().push(branch.to_string());
            std::thread::sleep(Duration::from_millis(60));
            in_flight.fetch_sub(1, Ordering::SeqCst);
            false
        })
        .unwrap();
        assert!(
            peak.load(Ordering::SeqCst) <= BRANCH_INTEGRATION_WORKERS,
            "more than {BRANCH_INTEGRATION_WORKERS} lookups in flight: {}",
            peak.load(Ordering::SeqCst)
        );
        let mut calls = calls.into_inner().unwrap();
        calls.sort();
        let mut deduped = calls.clone();
        deduped.dedup();
        assert_eq!(calls, deduped, "a branch was looked up more than once");
        for n in 0..18 {
            assert!(
                calls.contains(&format!("extra-1295-{n:02}")),
                "extra-1295-{n:02} was never looked up"
            );
        }
        assert_eq!(listed.len(), listed_branches_1295(&repo).len());
    }

    #[test]
    fn branch_integrations_keep_listing_order_when_later_branches_finish_first_1295() {
        // Catches: results returned in completion order instead of listing order
        // (earlier-listed branches are made the slowest here).
        let (_temp, repo, _wt) = integration_1295_fixture(true, true);
        add_extra_branches_1295(&repo, 12);
        let expected = listed_branches_1295(&repo);
        let total = expected.len() as u64;
        let listed = branch_integrations_with_pr(&repo, |_, branch, _| {
            let index = expected.iter().position(|b| b == branch).unwrap() as u64;
            std::thread::sleep(Duration::from_millis((total - index) * 25));
            false
        })
        .unwrap();
        assert_eq!(
            listed.iter().map(|e| e.branch.clone()).collect::<Vec<_>>(),
            expected
        );
    }

    #[test]
    fn branch_integrations_attach_each_lookup_result_to_its_own_branch_1295() {
        // Catches: a result paired with a neighbouring branch after the concurrent
        // collect-and-sort (only extra-1295-05 is proven by a pull request).
        let (_temp, repo, _wt) = integration_1295_fixture(true, true);
        add_extra_branches_1295(&repo, 10);
        let listed = branch_integrations_with_pr(&repo, |_, branch, _| {
            std::thread::sleep(Duration::from_millis(20));
            branch == "extra-1295-05"
        })
        .unwrap();
        for entry in &listed {
            assert_eq!(
                entry.proof == Some("github_pr"),
                entry.branch == "extra-1295-05",
                "wrong pull request proof on {}",
                entry.branch
            );
            assert_eq!(
                entry.tip,
                rev_at(&repo, &format!("refs/heads/{}", entry.branch)).unwrap()
            );
        }
    }

    #[test]
    fn squash_merged_branch_is_integrated_1295() {
        let (_temp, repo, wt) = integration_1295_fixture(true, false);
        let status = inspect_workspace_lifecycle(&repo, "audit-1295");
        assert_eq!(status.commit_status, WorkspaceCommitStatus::Merged);
        assert_eq!(status.merge_proof, Some("squash_message"));
        git_cmd(&repo)
            .args(["worktree", "remove", &wt.to_string_lossy()])
            .run()
            .unwrap();
        assert_eq!(
            delete_integrated_local_branch(&repo.to_string_lossy(), "audit-1295").unwrap(),
            "squash_message"
        );
        assert!(rev_at(&repo, "refs/heads/audit-1295").is_err());
    }

    #[test]
    fn noop_merge_branch_is_integrated_1295() {
        let (_temp, repo, wt) = integration_1295_fixture(false, false);
        let status = inspect_workspace_lifecycle(&repo, "audit-1295");
        assert_eq!(status.commit_status, WorkspaceCommitStatus::Merged);
        assert_eq!(status.merge_proof, Some("noop_merge"));
        git_cmd(&repo)
            .args(["worktree", "remove", &wt.to_string_lossy()])
            .run()
            .unwrap();
        assert_eq!(
            delete_integrated_local_branch(&repo.to_string_lossy(), "audit-1295").unwrap(),
            "noop_merge"
        );
        assert!(rev_at(&repo, "refs/heads/audit-1295").is_err());
    }

    #[test]
    fn branch_with_unique_lines_is_not_integrated_1295() {
        let (_temp, repo, wt) = integration_1295_fixture(true, true);
        let tip = rev_at(&repo, "refs/heads/audit-1295").unwrap();
        assert_eq!(
            inspect_workspace_lifecycle(&repo, "audit-1295").commit_status,
            WorkspaceCommitStatus::Unmerged
        );
        git_cmd(&repo)
            .args(["worktree", "remove", &wt.to_string_lossy()])
            .run()
            .unwrap();
        assert!(delete_integrated_local_branch(&repo.to_string_lossy(), "audit-1295").is_err());
        assert_eq!(rev_at(&repo, "refs/heads/audit-1295").unwrap(), tip);
    }

    #[test]
    fn squash_merged_clean_workspace_is_removed_by_patch_equivalence() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        git_cmd(&repo).args(["branch", "-M", "main"]).run().unwrap();
        let worktree = add_worktree(&repo, "squashed");
        git_cmd(&repo)
            .args(["update-ref", "refs/remotes/origin/main", "HEAD"])
            .run()
            .unwrap();
        git_cmd(&repo)
            .args([
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/main",
            ])
            .run()
            .unwrap();
        commit_file(&worktree, "same.txt", "same patch\n");
        fs::write(repo.join("same.txt"), "same patch\n").unwrap();
        git_cmd(&repo).args(["add", "same.txt"]).run().unwrap();
        git_cmd(&repo)
            .args(["commit", "-m", "squash equivalent"])
            .run()
            .unwrap();
        assert_ne!(
            git_cmd(&repo)
                .args(["rev-parse", "main"])
                .run()
                .unwrap()
                .stdout,
            git_cmd(&repo)
                .args(["rev-parse", "origin/main"])
                .run()
                .unwrap()
                .stdout,
            "the squash commit must exist only on local main"
        );

        let outcome =
            remove_worktree_by_workspace_id(&repo.to_string_lossy(), "squashed", true, None, false)
                .unwrap();

        assert_eq!(outcome.removal_rule, "patch_equivalence");
        assert!(!worktree.exists());
        assert!(
            git_cmd(&repo)
                .args(["show-ref", "--verify", "refs/heads/squashed"])
                .run_silent()
                .is_none()
        );
    }

    #[test]
    fn rebase_merged_clean_workspace_is_removed_by_patch_equivalence() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let worktree = add_worktree(&repo, "rebased");
        commit_file(&worktree, "same.txt", "same patch\n");
        commit_file(&repo, "advance.txt", "advance\n");
        git_cmd(&repo)
            .args(["cherry-pick", "rebased"])
            .run()
            .unwrap();

        let outcome =
            remove_worktree_by_workspace_id(&repo.to_string_lossy(), "rebased", true, None, false)
                .unwrap();

        assert_eq!(outcome.removal_rule, "patch_equivalence");
        assert!(!worktree.exists());
    }

    #[test]
    fn branch_delete_and_worktree_remove_reject_a_patch_only_on_current_nondefault_branch() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        git_cmd(&repo).args(["branch", "-M", "main"]).run().unwrap();
        git_cmd(&repo)
            .args(["update-ref", "refs/remotes/origin/main", "HEAD"])
            .run()
            .unwrap();
        git_cmd(&repo)
            .args([
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/main",
            ])
            .run()
            .unwrap();
        let worktree = add_worktree(&repo, "feature");
        commit_file(&worktree, "feature.txt", "unique patch\n");
        git_cmd(&repo)
            .args(["checkout", "-b", "integration"])
            .run()
            .unwrap();
        commit_file(&repo, "integration.txt", "integration-only advance\n");
        git_cmd(&repo)
            .args(["cherry-pick", "feature"])
            .run()
            .unwrap();

        let remove_error =
            remove_worktree_by_workspace_id(&repo.to_string_lossy(), "feature", true, None, false)
                .unwrap_err();
        assert!(remove_error.contains("unmerged commits"), "{remove_error}");

        remove_worktree_by_workspace_id(&repo.to_string_lossy(), "feature", false, None, false)
            .unwrap();
        let delete_error =
            delete_integrated_local_branch(&repo.to_string_lossy(), "feature").unwrap_err();
        assert!(delete_error.contains("unmerged commits"), "{delete_error}");
        assert!(
            git_cmd(&repo)
                .args(["show-ref", "--verify", "refs/heads/feature"])
                .run()
                .is_ok(),
            "branch must remain after both safety refusals"
        );
    }

    #[test]
    fn unique_patch_still_refuses_non_force_removal() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let worktree = add_worktree(&repo, "unique-patch");
        commit_file(&worktree, "unique.txt", "only on the worktree\n");

        let error = remove_worktree_by_workspace_id(
            &repo.to_string_lossy(),
            "unique-patch",
            true,
            None,
            false,
        )
        .unwrap_err();

        assert!(error.contains("unmerged commits"), "{error}");
        assert!(worktree.exists());
    }

    #[test]
    fn non_force_removal_preserves_dirty_work_in_every_commit_state() {
        for (name, advance_main, delete_branch) in [
            ("dirty-in-sync", false, true),
            ("dirty-merged", true, true),
            ("dirty-keep-branch", false, false),
        ] {
            let (_temp, repo, _workspaces) = workspace_fixture();
            let worktree = add_worktree(&repo, name);
            if advance_main {
                commit_file(&repo, "main-only.txt", "advanced\n");
            }
            fs::write(worktree.join("untracked.txt"), "keep me\n").unwrap();
            let error = remove_worktree_by_workspace_id(
                &repo.to_string_lossy(),
                name,
                delete_branch,
                None,
                false,
            )
            .unwrap_err();
            assert!(error.contains("uncommitted changes"), "{name}: {error}");
            assert_eq!(
                fs::read_to_string(worktree.join("untracked.txt")).unwrap(),
                "keep me\n"
            );
        }
    }

    #[test]
    fn non_force_patch_equivalence_preserves_dirty_work() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let worktree = add_worktree(&repo, "dirty-squash");
        commit_file(&worktree, "same.txt", "same patch\n");
        commit_file(&repo, "same.txt", "same patch\n");
        git_cmd(&repo)
            .args(["commit", "--amend", "-m", "squash equivalent"])
            .run()
            .unwrap();
        fs::write(worktree.join("untracked.txt"), "keep me\n").unwrap();
        let error = remove_worktree_by_workspace_id(
            &repo.to_string_lossy(),
            "dirty-squash",
            true,
            None,
            false,
        )
        .unwrap_err();
        assert!(error.contains("uncommitted changes"), "{error}");
        assert!(worktree.join("untracked.txt").exists());
    }

    #[test]
    fn archive_that_advances_branch_keeps_its_new_commit() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let worktree = add_worktree(&repo, "late-commit");
        commit_file(&worktree, "same.txt", "same patch\n");
        commit_file(&repo, "same.txt", "same patch\n");
        git_cmd(&repo)
            .args(["commit", "--amend", "-m", "squash equivalent"])
            .run()
            .unwrap();
        let outcome = remove_worktree_by_workspace_id(
            &repo.to_string_lossy(),
            "late-commit",
            true,
            Some("git commit --allow-empty -m late-commit"),
            false,
        )
        .unwrap();
        assert!(
            outcome
                .branch_delete_warning
                .as_deref()
                .is_some_and(|warning| warning.contains("changed")),
            "{:?}",
            outcome.branch_delete_warning
        );
        assert!(
            git_cmd(&repo)
                .args(["show-ref", "--verify", "refs/heads/late-commit"])
                .run()
                .is_ok()
        );
    }

    #[test]
    fn forced_removal_still_compares_the_branch_tip_before_deleting() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let worktree = add_worktree(&repo, "force-late-commit");
        let outcome = remove_worktree_by_workspace_id(
            &repo.to_string_lossy(),
            "force-late-commit",
            true,
            Some("git commit --allow-empty -m late-commit"),
            true,
        )
        .unwrap();
        assert!(!worktree.exists());
        assert!(
            outcome
                .branch_delete_warning
                .as_deref()
                .is_some_and(|warning| warning.contains("changed")),
            "{outcome:?}"
        );
        assert!(
            git_cmd(&repo)
                .args(["show-ref", "--verify", "refs/heads/force-late-commit"])
                .run()
                .is_ok()
        );
    }

    #[test]
    fn non_force_removal_refuses_an_in_progress_git_operation() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let worktree = add_worktree(&repo, "mid-merge");
        let admin = worktree_admin_dir(&worktree.to_string_lossy()).unwrap();
        fs::write(admin.join("MERGE_HEAD"), rev_at(&repo, "HEAD").unwrap()).unwrap();
        let error = remove_worktree_by_workspace_id(
            &repo.to_string_lossy(),
            "mid-merge",
            true,
            None,
            false,
        )
        .unwrap_err();
        assert!(error.contains("operation is in progress"), "{error}");
        assert!(worktree.exists());
    }

    #[test]
    fn force_cannot_discard_a_detached_head_commit() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let worktree = add_worktree(&repo, "detached-work");
        git_cmd(&worktree)
            .args(["checkout", "--detach"])
            .run()
            .unwrap();
        commit_file(&worktree, "detached.txt", "only at detached HEAD\n");
        let detached_oid = rev_at(&worktree, "HEAD").unwrap();

        let error = remove_worktree_by_workspace_id(
            &repo.to_string_lossy(),
            "detached-work",
            true,
            None,
            true,
        )
        .unwrap_err();
        // A plain detached checkout has no branch-keyed workspace id, so the
        // request fails before the branch-tip comparison can run.
        assert!(error.contains("No workspace found"), "{error}");
        assert!(worktree.exists());
        assert_eq!(rev_at(&worktree, "HEAD").unwrap(), detached_oid);
    }

    // Catches: applying Windows separator rewriting on Unix authorizes a
    // different checkout when a registered directory contains a literal backslash.
    #[cfg(unix)]
    #[test]
    fn orphan_validation_does_not_alias_literal_backslash_to_a_separator() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let parent = repo.parent().unwrap();
        let registered = parent.join(r"literal\checkout");
        git_cmd(&repo)
            .args([
                "worktree",
                "add",
                "--detach",
                &registered.to_string_lossy(),
                "HEAD",
            ])
            .run()
            .unwrap();
        let different = parent.join("literal").join("checkout");
        fs::create_dir_all(&different).unwrap();
        fs::write(different.join("keep.txt"), "unregistered user work\n").unwrap();

        validate_worktree_path(&repo.to_string_lossy(), &registered.to_string_lossy()).unwrap();
        let error = validate_worktree_path(&repo.to_string_lossy(), &different.to_string_lossy())
            .unwrap_err();
        assert!(error.contains("not a known worktree"), "{error}");
        assert_eq!(
            fs::read_to_string(different.join("keep.txt")).unwrap(),
            "unregistered user work\n"
        );
    }

    #[test]
    fn orphan_removal_refuses_a_detached_commit_without_a_durable_ref() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let path = add_worktree(&repo, "orphan-only-head");
        git_cmd(&path).args(["checkout", "--detach"]).run().unwrap();
        commit_file(&path, "detached.txt", "only at detached HEAD\n");
        let oid = rev_at(&path, "HEAD").unwrap();
        let worktree = WorktreeInfo {
            name: "orphan-only-head".into(),
            path: path.clone(),
            branch: None,
            base_repo: repo.clone(),
        };

        validate_worktree_path(&repo.to_string_lossy(), &path.to_string_lossy()).unwrap();
        let error = remove_worktree_internal(&worktree, false).unwrap_err();
        assert!(error.contains("detached"), "{error}");
        assert!(path.exists());
        assert_eq!(rev_at(&path, "HEAD").unwrap(), oid);
    }

    #[test]
    fn orphan_removal_accepts_a_detached_head_reachable_from_a_ref() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let path = add_worktree(&repo, "orphan-reachable");
        git_cmd(&path).args(["checkout", "--detach"]).run().unwrap();
        let detached_oid = rev_at(&path, "HEAD").unwrap();
        commit_file(&repo, "later.txt", "main moved ahead\n");
        assert_ne!(rev_at(&repo, "HEAD").unwrap(), detached_oid);
        let worktree = WorktreeInfo {
            name: "orphan-reachable".into(),
            path: path.clone(),
            branch: None,
            base_repo: repo.clone(),
        };

        validate_worktree_path(&repo.to_string_lossy(), &path.to_string_lossy()).unwrap();
        remove_worktree_internal(&worktree, false).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn orphan_removal_refuses_an_in_progress_operation() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let path = add_worktree(&repo, "orphan-mid-operation");
        git_cmd(&path).args(["checkout", "--detach"]).run().unwrap();
        let admin = worktree_admin_dir(&path.to_string_lossy()).unwrap();
        fs::write(admin.join("MERGE_HEAD"), rev_at(&repo, "HEAD").unwrap()).unwrap();

        let error =
            validate_worktree_path(&repo.to_string_lossy(), &path.to_string_lossy()).unwrap_err();
        assert!(error.contains("operation"), "{error}");
        let worktree = WorktreeInfo {
            name: "orphan-mid-operation".into(),
            path: path.clone(),
            branch: None,
            base_repo: repo.clone(),
        };
        let error = remove_worktree_internal(&worktree, false).unwrap_err();
        assert!(error.contains("operation"), "{error}");
        assert!(path.exists());
    }

    #[test]
    fn orphan_removal_refuses_an_attached_worktree_path() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let path = add_worktree(&repo, "not-an-orphan");

        let error =
            validate_worktree_path(&repo.to_string_lossy(), &path.to_string_lossy()).unwrap_err();
        assert!(error.contains("not detached"), "{error}");
        assert!(path.exists());
    }

    // Catches: a Keep that cannot tell "still the same edits" from "edited again
    // or cleaned", because the assessment carries no fingerprint (orphan-dialog-repeats).
    #[test]
    fn orphan_assessment_fingerprint_follows_the_checkout_state() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let path = add_worktree(&repo, "orphan-fingerprint");
        git_cmd(&path).args(["checkout", "--detach"]).run().unwrap();
        let fingerprint = |repo: &Path| {
            assess_orphan_worktrees(&repo.to_string_lossy())
                .unwrap()
                .into_iter()
                .find(|entry| entry.path.ends_with("orphan-fingerprint"))
                .expect("the orphan is listed")
        };

        let clean = fingerprint(&repo);
        assert!(clean.safe, "{clean:?}");
        fs::write(path.join("note.txt"), "one\n").unwrap();
        let dirty = fingerprint(&repo);
        assert!(!dirty.safe, "{dirty:?}");
        assert_eq!(
            dirty,
            fingerprint(&repo),
            "unchanged edits keep the fingerprint"
        );
        fs::write(path.join("second.txt"), "x\n").unwrap();
        let grown = fingerprint(&repo);

        assert!(clean.dirty_fingerprint.is_some());
        assert_ne!(clean.dirty_fingerprint, dirty.dirty_fingerprint);
        assert_ne!(dirty.dirty_fingerprint, grown.dirty_fingerprint);
    }

    /// The directory was moved away but git still lists the worktree. Spawning
    /// git in the missing cwd used to surface as "Failed to spawn git".
    #[test]
    fn orphan_whose_directory_is_gone_is_safe_to_forget_and_removal_drops_the_entry() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let path = add_worktree(&repo, "orphan-moved-away");
        git_cmd(&path).args(["checkout", "--detach"]).run().unwrap();
        fs::remove_dir_all(&path).unwrap();

        let assessments = assess_orphan_worktrees(&repo.to_string_lossy()).unwrap();

        let entry = assessments
            .iter()
            .find(|entry| entry.path.ends_with("orphan-moved-away"))
            .expect("the tracked orphan is still listed");
        assert!(entry.safe, "{entry:?}");
        let reason = entry.reason.as_deref().unwrap_or_default();
        assert!(reason.contains("already gone"), "{reason}");
        assert!(!reason.contains("spawn"), "{reason}");

        let worktree = WorktreeInfo {
            name: "orphan-moved-away".into(),
            path: path.clone(),
            branch: None,
            base_repo: repo.clone(),
        };
        remove_orphan_worktree_internal(&worktree).unwrap();
        let listed = git_cmd(&repo)
            .args(["worktree", "list", "--porcelain"])
            .run()
            .unwrap()
            .stdout;
        assert!(!listed.contains("orphan-moved-away"), "{listed}");
    }

    #[tokio::test]
    async fn queued_warm_cannot_recreate_a_removed_worktree() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let path = add_worktree(&repo, "cancelled-warm");
        let token = begin_warm(&path);
        let worktree = WorktreeInfo {
            name: "cancelled-warm".into(),
            path: path.clone(),
            branch: Some("cancelled-warm".into()),
            base_repo: repo.clone(),
        };
        remove_worktree_internal(&worktree, false).unwrap();
        let task = spawn_background_warm(repo, path.clone(), token, |_, destination| {
            fs::create_dir_all(destination).unwrap();
            crate::cow::WarmingReport::default()
        });
        task.await.unwrap();
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn archive_waits_for_active_warm_and_cancels_queued_warm() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let path = add_worktree(&repo, "archive-warm");
        let token = begin_warm(&path);
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let active = spawn_background_warm(repo.clone(), path.clone(), token, move |_, dest| {
            started_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            fs::create_dir_all(dest.join("warm-cache")).unwrap();
            crate::cow::WarmingReport::default()
        });
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();

        let repo_for_archive = repo.clone();
        let archive = tokio::task::spawn_blocking(move || {
            archive_worktree(&repo_for_archive, "archive-warm", None)
        });
        // Before the guard, archive completes while the copy is held and the
        // copy later recreates the old checkout path. With the guard it waits.
        for _ in 0..100 {
            if archive.is_finished() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        release_tx.send(()).unwrap();
        active.await.unwrap();
        let archived = archive.await.unwrap().unwrap();
        assert!(!path.exists(), "warm recreated {}", path.display());
        assert!(Path::new(&archived).join("warm-cache").exists());
        assert!(!warm_token_is_current(&path, token));
    }

    #[test]
    fn a_unique_merge_resolution_is_not_hidden_by_git_cherry() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        commit_file(&repo, "base.txt", "base\n");
        let worktree = add_worktree(&repo, "merge-resolution");
        commit_file(&repo, "advance.txt", "advance\n");
        let default_branch = get_remote_default_branch(&repo.to_string_lossy()).unwrap();
        git_cmd(&worktree)
            .args(["merge", "--no-ff", &default_branch, "-m", "merge upstream"])
            .run()
            .unwrap();
        fs::write(worktree.join("resolution.txt"), "only in the merge\n").unwrap();
        git_cmd(&worktree)
            .args(["add", "resolution.txt"])
            .run()
            .unwrap();
        git_cmd(&worktree)
            .args(["commit", "--amend", "--no-edit"])
            .run()
            .unwrap();
        let cherry = git_cmd(&repo)
            .args(["cherry", &default_branch, "merge-resolution"])
            .run()
            .unwrap();
        assert!(cherry.stdout.trim().is_empty(), "{}", cherry.stdout);

        let error = remove_worktree_by_workspace_id(
            &repo.to_string_lossy(),
            "merge-resolution",
            true,
            None,
            false,
        )
        .unwrap_err();

        assert!(error.contains("unmerged commits"), "{error}");
        assert!(worktree.exists());
    }

    #[test]
    fn removing_a_worktree_clears_its_warm_state() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let worktree = add_worktree(&repo, "warm-removal");
        let old_token = begin_warm(&worktree);
        assert_eq!(warm_status(&worktree)["status"], "pending");
        let listed = get_worktree_paths(repo.to_string_lossy().into_owned()).unwrap();
        assert_eq!(
            listed["warm-removal"].warm_artifacts.as_ref().unwrap()["status"],
            "pending"
        );

        remove_worktree_by_workspace_id(&repo.to_string_lossy(), "warm-removal", true, None, false)
            .unwrap();

        assert!(!WARM_STATES.contains_key(&warm_key(&worktree)));
        assert_eq!(warm_status(&worktree)["status"], "done");
        let new_token = begin_warm(&worktree);
        finish_warm(
            &worktree,
            old_token,
            serde_json::json!({"status": "failed"}),
        );
        assert_eq!(warm_status(&worktree)["status"], "pending");
        finish_warm(&worktree, new_token, serde_json::json!({"status": "done"}));
        clear_warm(&worktree);
    }

    #[test]
    fn warm_states_remain_isolated_between_workspace_paths() {
        let temp = TempDir::new().unwrap();
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        let first_token = begin_warm(&first);
        let second_token = begin_warm(&second);

        finish_warm(&first, first_token, serde_json::json!({"status": "done"}));
        assert_eq!(warm_status(&first)["status"], "done");
        assert_eq!(warm_status(&second)["status"], "pending");

        finish_warm(
            &second,
            second_token,
            serde_json::json!({"status": "failed", "reason": "copy failed"}),
        );
        assert_eq!(warm_status(&first)["status"], "done");
        assert_eq!(warm_status(&second)["status"], "failed");
        clear_warm(&first);
        clear_warm(&second);
    }

    #[cfg(unix)]
    #[test]
    fn removal_clears_pending_warm_even_when_leftover_path_cleanup_fails() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let path = add_worktree(&repo, "warm-cleanup-failure");
        let token = begin_warm(&path);
        assert_eq!(warm_status(&path)["status"], "pending");

        // Git has already unregistered the checkout. A protected directory at
        // the old path makes recursive cleanup fail, but the pending state
        // must still be removed before a later checkout can inherit it.
        git_cmd(&repo)
            .args(["worktree", "remove", &path.to_string_lossy()])
            .run()
            .unwrap();
        fs::create_dir(&path).unwrap();
        fs::write(path.join("protected"), "x").unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o500)).unwrap();
        let worktree = WorktreeInfo {
            name: "warm-cleanup-failure".into(),
            path: path.clone(),
            branch: Some("warm-cleanup-failure".into()),
            base_repo: repo,
        };
        let error = remove_worktree_internal(&worktree, true).unwrap_err();
        assert!(!warm_token_is_current(&path, token), "{error}");
        assert!(
            error.contains(&path.to_string_lossy().to_string()),
            "{error}"
        );
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        fs::remove_dir_all(&path).unwrap();
    }

    #[test]
    fn unknown_directory_is_not_removed_without_a_warm_token() {
        let (_temp, repo, workspaces) = workspace_fixture();
        let path = workspaces.join("unrelated-directory");
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("keep.txt"), "unrelated\n").unwrap();
        let worktree = WorktreeInfo {
            name: "unrelated-directory".into(),
            path: path.clone(),
            branch: None,
            base_repo: repo,
        };

        assert!(remove_worktree_internal(&worktree, true).is_err());
        assert_eq!(
            fs::read_to_string(path.join("keep.txt")).unwrap(),
            "unrelated\n"
        );
    }

    /// Dirtiness is orthogonal to the commit verdict, and that is exactly why
    /// the verdict must never be read as "nothing would be lost": the removal
    /// discards these files, and no commit check can see them.
    #[test]
    fn uncommitted_files_make_removal_unsafe_whatever_the_commit_verdict_says() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let worktree = add_worktree(&repo, "dirty-and-in-sync");
        fs::write(worktree.join("untracked.txt"), "not committed\n").expect("untracked file");
        fs::write(worktree.join("README.md"), "edited\n").expect("modified file");

        let status = inspect_workspace_lifecycle(&repo, "dirty-and-in-sync");

        assert_eq!(status.commit_status, WorkspaceCommitStatus::InSync);
        // The count, not a flag: both files are lost by a removal, and one
        // tracked edit plus one untracked file must read as two.
        assert_eq!(status.dirty_files, Some(2));
        assert_eq!(status.removal_safety, WorkspaceRemovalSafety::RequiresForce);
    }

    #[test]
    fn an_unresolvable_workspace_reports_unknown_with_the_reason() {
        let (_temp, repo, _workspaces) = workspace_fixture();

        let status = inspect_workspace_lifecycle(&repo, "never-created");

        assert_eq!(status.commit_status, WorkspaceCommitStatus::Unknown);
        assert_eq!(status.dirty_files, None);
        assert_eq!(status.removal_safety, WorkspaceRemovalSafety::Unknown);
        assert!(
            status
                .error
                .as_deref()
                .is_some_and(|e| e.contains("never-created")),
            "the reason must name the workspace: {:?}",
            status.error
        );
    }

    #[test]
    fn resolving_a_workspace_preserves_git_failure() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join(".git"), "gitdir: missing-git-dir\n").unwrap();
        let error = resolve_any_workspace(temp.path(), "missing").unwrap_err();
        assert!(error.contains("git worktree list failed"), "{error}");
        assert!(!error.contains("No workspace found"), "{error}");
    }

    /// A bare origin holding the repo's default branch, fetched so
    /// `origin/<default>` exists locally.
    fn add_origin(repo: &Path) -> PathBuf {
        let origin = repo.parent().unwrap().join("origin.git");
        git_cmd(repo.parent().unwrap())
            .args(["init", "--bare", &origin.to_string_lossy()])
            .run()
            .unwrap();
        git_cmd(repo)
            .args(["remote", "add", "origin", &origin.to_string_lossy()])
            .run()
            .unwrap();
        git_cmd(repo)
            .args(["push", "origin", &base_branch_of(repo)])
            .run()
            .unwrap();
        git_cmd(repo).args(["fetch", "origin"]).run().unwrap();
        origin
    }

    /// Local main is stale (10 behind origin/main in the field); the branch
    /// was merged upstream. Comparing only the local default branch called it
    /// unmerged and refused removal.
    #[test]
    fn branch_merged_into_upstream_default_while_local_default_is_stale_is_merged() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        add_origin(&repo);
        let worktree = add_worktree(&repo, "landed-upstream");
        commit_file(&worktree, "feature.txt", "landed work\n");
        let main = base_branch_of(&repo);
        git_cmd(&worktree)
            .args(["push", "origin", &format!("landed-upstream:{main}")])
            .run()
            .unwrap();
        git_cmd(&repo).args(["fetch", "origin"]).run().unwrap();

        let status = inspect_workspace_lifecycle(&repo, "landed-upstream");

        assert_eq!(status.commit_status, WorkspaceCommitStatus::Merged);
        let outcome = remove_worktree_by_workspace_id(
            &repo.to_string_lossy(),
            "landed-upstream",
            true,
            None,
            false,
        )
        .unwrap();
        assert_eq!(outcome.removal_rule, "ancestry");
        assert!(!worktree.exists());
    }

    /// Every commit is on origin/<branch>: deleting the local ref loses
    /// nothing, so removal must not be refused as data loss.
    #[test]
    fn branch_fully_pushed_to_its_own_remote_is_pushed_unmerged_and_removable() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        add_origin(&repo);
        let worktree = add_worktree(&repo, "pushed-only");
        commit_file(&worktree, "feature.txt", "pushed work\n");
        git_cmd(&worktree)
            .args(["push", "origin", "pushed-only"])
            .run()
            .unwrap();
        git_cmd(&repo).args(["fetch", "origin"]).run().unwrap();

        let status = inspect_workspace_lifecycle(&repo, "pushed-only");

        assert_eq!(status.commit_status, WorkspaceCommitStatus::PushedUnmerged);
        let outcome = remove_worktree_by_workspace_id(
            &repo.to_string_lossy(),
            "pushed-only",
            true,
            None,
            false,
        )
        .unwrap();
        assert_eq!(outcome.removal_rule, "remote_tracking");
        assert!(!worktree.exists());
        let remaining = git_cmd(&repo)
            .args(["branch", "--list", "pushed-only"])
            .run()
            .unwrap()
            .stdout;
        assert!(remaining.trim().is_empty(), "{remaining}");
        git_cmd(&repo)
            .args(["cat-file", "-e", "refs/remotes/origin/pushed-only"])
            .run()
            .expect("the pushed commits stay reachable");
    }

    #[test]
    fn branch_with_a_commit_missing_from_its_remote_stays_unmerged_and_refused() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        add_origin(&repo);
        let worktree = add_worktree(&repo, "half-pushed");
        commit_file(&worktree, "one.txt", "pushed\n");
        git_cmd(&worktree)
            .args(["push", "origin", "half-pushed"])
            .run()
            .unwrap();
        git_cmd(&repo).args(["fetch", "origin"]).run().unwrap();
        commit_file(&worktree, "two.txt", "local only\n");

        let status = inspect_workspace_lifecycle(&repo, "half-pushed");
        assert_eq!(status.commit_status, WorkspaceCommitStatus::Unmerged);

        let error = remove_worktree_by_workspace_id(
            &repo.to_string_lossy(),
            "half-pushed",
            true,
            None,
            false,
        )
        .unwrap_err();
        assert!(error.contains("unmerged"), "{error}");
        assert!(worktree.exists());
    }

    #[test]
    fn unmerged_refusal_names_every_base_it_compared() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        add_origin(&repo);
        let worktree = add_worktree(&repo, "unshared");
        commit_file(&worktree, "feature.txt", "local only\n");
        let main = base_branch_of(&repo);

        let error =
            remove_worktree_by_workspace_id(&repo.to_string_lossy(), "unshared", true, None, false)
                .unwrap_err();

        assert!(error.contains(&format!("origin/{main}")), "{error}");
        assert!(error.contains(&format!("and {main}")), "{error}");
    }

    /// The serialized spellings are the contract the sidebar's label table
    /// reads; renaming a variant silently turns a badge into dead code.
    #[test]
    fn commit_status_serializes_as_the_frontend_spells_it() {
        let spellings = [
            (WorkspaceCommitStatus::Unmerged, "unmerged"),
            (WorkspaceCommitStatus::PushedUnmerged, "pushed_unmerged"),
            (WorkspaceCommitStatus::InSync, "in_sync"),
            (WorkspaceCommitStatus::Merged, "merged"),
            (WorkspaceCommitStatus::Unknown, "unknown"),
        ];
        for (status, expected) in spellings {
            assert_eq!(serde_json::to_value(status).expect("serialize"), expected);
        }
    }

    fn orphan_entry(repo: &Path, name: &str) -> OrphanCleanupAssessment {
        assess_orphan_worktrees(&repo.to_string_lossy())
            .unwrap()
            .into_iter()
            .find(|entry| entry.path.ends_with(name))
            .unwrap_or_else(|| panic!("{name} is listed as an orphan"))
    }

    fn detached_orphan(repo: &Path, name: &str) -> PathBuf {
        let path = add_worktree(repo, name);
        git_cmd(&path).args(["checkout", "--detach"]).run().unwrap();
        path
    }

    // Catches (critic-1367): any form of unsaved work that `git status` reports being
    // missed by the safety verdict, so ask mode now removes it without a dialog.
    #[test]
    fn orphan_assessment_is_unsafe_for_every_kind_of_unsaved_work() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let staged = detached_orphan(&repo, "orphan-staged");
        fs::write(staged.join("new.txt"), "x\n").unwrap();
        git_cmd(&staged).args(["add", "new.txt"]).run().unwrap();
        let edited = detached_orphan(&repo, "orphan-edited");
        fs::write(edited.join("README.md"), "changed\n").unwrap();
        let deleted = detached_orphan(&repo, "orphan-deleted");
        fs::remove_file(deleted.join("README.md")).unwrap();
        let untracked = detached_orphan(&repo, "orphan-untracked");
        fs::write(untracked.join("scratch.txt"), "x\n").unwrap();
        detached_orphan(&repo, "orphan-clean");

        for name in [
            "orphan-staged",
            "orphan-edited",
            "orphan-deleted",
            "orphan-untracked",
        ] {
            let entry = orphan_entry(&repo, name);
            assert!(!entry.safe, "{name} holds unsaved work: {entry:?}");
            assert!(entry.dirty_fingerprint.is_some(), "{name}: {entry:?}");
        }
        assert!(orphan_entry(&repo, "orphan-clean").safe);
    }

    // Catches (critic-1367): a fingerprint built from `status` alone, so a Keep on a
    // clean checkout survives it being moved to another commit.
    #[test]
    fn orphan_assessment_fingerprint_changes_when_a_clean_checkout_moves_head() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        commit_file(&repo, "second.txt", "two\n");
        let path = detached_orphan(&repo, "orphan-moves");

        let before = orphan_entry(&repo, "orphan-moves");
        assert_eq!(
            before.dirty_fingerprint,
            orphan_entry(&repo, "orphan-moves").dirty_fingerprint,
            "an untouched checkout keeps its fingerprint"
        );
        git_cmd(&path)
            .args(["checkout", "--detach", "HEAD~1"])
            .run()
            .unwrap();
        let after = orphan_entry(&repo, "orphan-moves");

        assert!(before.safe && after.safe, "{before:?} {after:?}");
        assert_ne!(before.dirty_fingerprint, after.dirty_fingerprint);
    }

    // Catches (critic-1367): a vanished checkout reported with a fingerprint, so a Keep
    // would hold for a directory nobody can inspect any more.
    #[test]
    fn orphan_whose_directory_is_gone_has_no_fingerprint() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let path = detached_orphan(&repo, "orphan-vanished");
        fs::remove_dir_all(&path).unwrap();

        let entry = orphan_entry(&repo, "orphan-vanished");

        assert!(entry.safe, "{entry:?}");
        assert_eq!(entry.dirty_fingerprint, None);
    }

    // Catches (critic-1367): the fingerprint being skipped when the verdict is "unsafe"
    // for a reason other than dirt, so such an orphan can never be remembered as kept;
    // and a verdict that does not follow the branch that later protects its HEAD.
    #[test]
    fn clean_orphan_with_an_unreachable_head_is_unsafe_with_a_fingerprint_until_a_branch_holds_it()
    {
        let (_temp, repo, _workspaces) = workspace_fixture();
        let path = detached_orphan(&repo, "orphan-unreachable");
        commit_file(&path, "only-here.txt", "x\n");

        let before = orphan_entry(&repo, "orphan-unreachable");
        assert!(!before.safe, "{before:?}");
        assert!(
            before
                .reason
                .as_deref()
                .unwrap_or_default()
                .contains("unreachable"),
            "{before:?}"
        );
        assert!(before.dirty_fingerprint.is_some(), "{before:?}");

        let head = rev_at(&path, "HEAD").unwrap();
        git_cmd(&repo)
            .args(["branch", "rescue", &head])
            .run()
            .unwrap();
        assert!(orphan_entry(&repo, "orphan-unreachable").safe);
    }

    // Catches (critic-1367): uncommitted edits inside a submodule being invisible to the
    // verdict and the fingerprint, so a populated submodule's work is removed unasked.
    #[test]
    fn orphan_assessment_sees_edits_inside_a_populated_submodule() {
        let (_temp, repo, _workspaces) = workspace_fixture();
        add_populated_submodule(&repo);
        let path = detached_orphan(&repo, "orphan-module");
        git_cmd(&path)
            .args([
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "update",
                "--init",
            ])
            .run()
            .unwrap();
        let clean = orphan_entry(&repo, "orphan-module");
        assert!(clean.safe, "{clean:?}");

        fs::write(path.join("modules/local/module.txt"), "edited in module\n").unwrap();
        let edited = orphan_entry(&repo, "orphan-module");

        assert!(!edited.safe, "{edited:?}");
        assert_ne!(clean.dirty_fingerprint, edited.dirty_fingerprint);
    }
}

/// Blocking orphan scan; scheduling belongs to the app adapter.
pub fn detect_orphan_worktrees_blocking(repo_path: String) -> Result<Vec<String>, String> {
    let base_repo = PathBuf::from(&repo_path);
    let out = git_cmd(&base_repo)
        .args(["worktree", "list", "--porcelain"])
        .run()
        .map_err(|e| format!("git worktree list failed: {e}"))?;
    Ok(parse_orphan_worktrees(&out.stdout))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OrphanCleanupAssessment {
    pub path: String,
    pub safe: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Same fingerprint `worktree_lifecycle` reports (status + HEAD + submodules).
    /// A remembered Keep holds only while it is unchanged. None when the
    /// checkout is gone or cannot be inspected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dirty_fingerprint: Option<String>,
}

/// Assess each current detached worktree. Only a clean checkout whose HEAD is
/// reachable from a local or remote branch may be removed automatically.
pub fn assess_orphan_worktrees(repo_path: &str) -> Result<Vec<OrphanCleanupAssessment>, String> {
    detect_orphan_worktrees_blocking(repo_path.to_string()).map(|paths| {
        paths
            .into_iter()
            .map(|path| {
                let dirty_fingerprint = dirty_fingerprint_at(Path::new(&path))
                    .ok()
                    .map(|(fingerprint, _)| fingerprint);
                match orphan_cleanup_safety(repo_path, &path) {
                    Ok(()) => OrphanCleanupAssessment {
                        reason: (!Path::new(&path).exists()).then(|| DIRECTORY_GONE.to_string()),
                        path,
                        safe: true,
                        dirty_fingerprint,
                    },
                    Err(reason) => OrphanCleanupAssessment {
                        path,
                        safe: false,
                        reason: Some(reason),
                        dirty_fingerprint,
                    },
                }
            })
            .collect()
    })
}

const DIRECTORY_GONE: &str = "directory is already gone; removal only drops the tracking entry";

pub fn orphan_cleanup_safety(repo_path: &str, worktree_path: &str) -> Result<(), String> {
    validate_worktree_path(repo_path, worktree_path)?;
    let worktree = Path::new(worktree_path);
    // Nothing to inspect: git cannot run in a missing cwd, and there is no
    // work left to lose.
    if !worktree.exists() {
        return Ok(());
    }
    let status = git_cmd(worktree)
        .args([
            "status",
            "--porcelain",
            "--untracked-files=all",
            "--ignore-submodules=none",
        ])
        .run()
        .map_err(|error| format!("Cannot inspect worktree changes: {error}"))?;
    if status.stdout.lines().any(|line| line.starts_with("??")) {
        return Err("untracked files".into());
    }
    if !status.stdout.trim().is_empty() {
        return Err("uncommitted changes".into());
    }

    let head = rev_at(worktree, "HEAD")?;
    let containing = git_cmd(Path::new(repo_path))
        .args([
            "for-each-ref",
            &format!("--contains={head}"),
            "--count=1",
            "--format=%(refname)",
            "refs/heads",
            "refs/remotes",
        ])
        .run()
        .map_err(|error| format!("Cannot inspect branch reachability: {error}"))?;
    if containing.stdout.trim().is_empty() {
        return Err("HEAD has commits unreachable from any branch".into());
    }
    Ok(())
}
