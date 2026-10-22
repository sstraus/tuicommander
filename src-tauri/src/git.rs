//! App-side half of the git module: the `AppState` git caches, the commands
//! that invalidate them, and the Tauri wrappers over [`tuic_git::git`]. The
//! domain functions are re-exported so `crate::git::*` paths are unchanged.

use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
#[cfg(feature = "desktop")]
use tauri::State;

pub(crate) use tuic_git::git::*;

use crate::git_cli::{finish_failed_git_operation_after_abort, git_cmd};
use crate::git_graph::GraphNode;
use crate::git_reads::git_reads;
use crate::state::{AppState, GitCache};

// --- Coalesced git-cache load helpers (Step 1) ---
//
// `moka` collapses concurrent identical loads for one key to a single
// computation (the headline fix for the `repo-changed` fan-out). Loaders are
// blocking git work, so they run on the blocking pool; the cache TTL/bound is
// configured on the cache itself. Values are `Arc`-wrapped in the cache and
// cloned out at the boundary to preserve the existing by-value return shapes.

/// Coalesced + cached blocking load for an infallible compute.
async fn cached_get<T, F>(cache: GitCache<T>, key: String, f: F) -> Result<T, String>
where
    T: Clone + Send + Sync + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let v = tokio::task::spawn_blocking(move || cache.get_with(key, || Arc::new(f())))
        .await
        .map_err(|e| format!("spawn_blocking error: {e}"))?;
    Ok((*v).clone())
}

/// Coalesced + cached blocking load for a fallible compute. Only `Ok` is cached.
async fn cached_try<T, F>(cache: GitCache<T>, key: String, f: F) -> Result<T, String>
where
    T: Clone + Send + Sync + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    let v = tokio::task::spawn_blocking(move || cache.try_get_with(key, || f().map(Arc::new)))
        .await
        .map_err(|e| format!("spawn_blocking error: {e}"))?
        .map_err(|e: Arc<String>| (*e).clone())?;
    Ok((*v).clone())
}

/// Cached repo info for synchronous callers (MCP handlers, etc.).
pub(crate) fn get_repo_info_cached(state: &AppState, path: &str) -> RepoInfo {
    let p = path.to_string();
    (*state
        .git_cache
        .repo_info
        .get_with(path.to_string(), || Arc::new(get_repo_info_impl(&p))))
    .clone()
}

/// Get git repository info for a path (cached, 5s TTL)
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn get_repo_info(
    state: State<'_, Arc<AppState>>,
    path: String,
) -> Result<RepoInfo, String> {
    let p = path.clone();
    cached_get(state.git_cache.repo_info.clone(), path, move || {
        get_repo_info_impl(&p)
    })
    .await
}

/// Rename a git branch (Tauri command with cache invalidation)
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn rename_branch(
    state: State<'_, Arc<AppState>>,
    path: String,
    old_name: String,
    new_name: String,
) -> Result<(), String> {
    let state_arc = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        rename_branch_impl(&path, &old_name, &new_name)?;
        state_arc.invalidate_repo_caches(&path);
        Ok(())
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// Create a git branch (Tauri command with cache invalidation)
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn create_branch(
    state: State<'_, Arc<AppState>>,
    path: String,
    name: String,
    start_point: Option<String>,
    checkout: bool,
) -> Result<(), String> {
    let state_arc = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        create_branch_impl(&path, &name, start_point.as_deref(), checkout)?;
        state_arc.invalidate_repo_caches(&path);
        Ok(())
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// Core logic for updating a branch from its stored base ref (rebase or merge).
///
/// Reads `branch.<name>.tuicommander-base` from git config, fetches if remote,
/// then applies the chosen strategy. On conflict, aborts and returns error.
/// Blocking — callers wrap in `spawn_blocking`.
pub(crate) fn update_from_base_impl(
    state: &Arc<AppState>,
    path: &str,
    branch_name: &str,
    strategy: Option<&str>,
) -> Result<String, String> {
    let repo_path = PathBuf::from(path);
    let strategy = strategy.unwrap_or("rebase");

    // Read stored base, fall back to default branch
    let base = crate::worktree::get_branch_base(path, branch_name).unwrap_or_else(|| {
        crate::worktree::get_remote_default_branch(path).unwrap_or_else(|_| "main".to_string())
    });

    // Fetch if remote
    crate::worktree::fetch_if_remote(path, &base)?;

    // Apply strategy
    match strategy {
        "rebase" => match git_cmd(&repo_path).args(["rebase", &base]).run() {
            Ok(_) => Ok(format!("Rebased {branch_name} onto {base}")),
            Err(crate::git_cli::GitError::NonZeroExit { stderr, .. }) => {
                let message = finish_failed_git_operation_after_abort(
                    &repo_path,
                    "rebase",
                    "Rebase failed",
                    stderr,
                );
                state.invalidate_repo_caches(path);
                Err(message)
            }
            Err(e) => Err(format!("Rebase error: {e}")),
        },
        "merge" => {
            match git_cmd(&repo_path)
                .args(["merge", &base, "--no-edit"])
                // A `merge.ff=only` config must not fail this explicitly requested merge.
                // `git merge` applies the mergeOptions of the branch checked out in
                // `repo_path`, which is not necessarily `branch_name`.
                .args(crate::git_cli::merge_ff_override_for_head(&repo_path))
                .run()
            {
                Ok(_) => Ok(format!("Merged {base} into {branch_name}")),
                Err(crate::git_cli::GitError::NonZeroExit { stderr, .. }) => {
                    let message = finish_failed_git_operation_after_abort(
                        &repo_path,
                        "merge",
                        "Merge failed",
                        stderr,
                    );
                    state.invalidate_repo_caches(path);
                    Err(message)
                }
                Err(e) => Err(format!("Merge error: {e}")),
            }
        }
        _ => Err(format!(
            "Unknown strategy: {strategy}. Use 'rebase' or 'merge'."
        )),
    }
}

/// Update a branch from its stored base ref (Tauri command).
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn update_from_base(
    state: State<'_, Arc<AppState>>,
    path: String,
    branch_name: String,
    strategy: Option<String>,
) -> Result<String, String> {
    let state_arc = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        update_from_base_impl(&state_arc, &path, &branch_name, strategy.as_deref())
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// Delete a git branch (Tauri command with cache invalidation)
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn delete_branch(
    state: State<'_, Arc<AppState>>,
    path: String,
    name: String,
    force: bool,
) -> Result<DeleteBranchResult, String> {
    let state_arc = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let result = delete_branch_impl(&path, &name, force)?;
        state_arc.invalidate_repo_caches(&path);
        Ok(result)
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// Tauri command: get branches merged into the main branch (cached, 5s TTL)
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn get_merged_branches(
    state: State<'_, Arc<AppState>>,
    path: String,
) -> Result<Vec<String>, String> {
    let p = path.clone();
    cached_try(state.git_cache.merged_branches.clone(), path, move || {
        get_merged_branches_impl(Path::new(&p))
    })
    .await
}

/// Lightweight structural snapshot: worktree paths + merged branches.
/// Returns fast (two git subprocesses, no per-worktree diff stats).
#[derive(Serialize)]
pub(crate) struct RepoStructure {
    /// Workspace id -> its checkout. Never keyed by branch: two workspaces may
    /// share one (#726-5ac7).
    worktree_paths: HashMap<String, crate::worktree::WorkspaceWorktree>,
    merged_branches: Vec<String>,
}

/// Per-worktree diff stats + last-commit timestamps.
/// Working-tree monitoring plus one batched last-commit read.
#[derive(Clone, Serialize)]
pub(crate) struct RepoDiffStats {
    diff_stats: HashMap<String, DiffStats>,
    last_commit_ts: HashMap<String, Option<i64>>,
    /// Authoritative lifecycle state keyed by opaque workspace id. Path-keyed
    /// diff_stats remains for existing clients; a path cannot identify one row.
    workspace_statuses: HashMap<String, crate::worktree::WorkspaceLifecycleStatus>,
}

/// Aggregate repo snapshot returned by `get_repo_summary`.
/// Collapses the N+2 IPC storm (get_worktree_paths + get_merged_branches + N×get_diff_stats)
/// into a single round-trip.
#[derive(Serialize)]
pub(crate) struct RepoSummary {
    /// Workspace id -> its checkout. Never keyed by branch: two workspaces may
    /// share one (#726-5ac7).
    worktree_paths: HashMap<String, crate::worktree::WorkspaceWorktree>,
    merged_branches: Vec<String>,
    /// Per-worktree diff stats, keyed by worktree path (matches the `path` field
    /// of worktree_paths values).
    diff_stats: HashMap<String, DiffStats>,
    /// Unix timestamp of the last commit on each branch, keyed by branch name.
    /// Branch-keyed on purpose: the answer is a property of the ref, so two
    /// workspaces on one branch share the entry.
    last_commit_ts: HashMap<String, Option<i64>>,
    workspace_statuses: HashMap<String, crate::worktree::WorkspaceLifecycleStatus>,
}

/// The repo's workspace-id→checkout map, shared across every phase of a refresh.
///
/// Progressive loading calls `get_repo_structure` then `get_repo_diff_stats` for
/// one bump and both need this map, so reading it per call forked
/// `git worktree list` twice for the same answer. `git_cache.worktree_paths` is
/// invalidated by `invalidate_repo_caches` — which the repo watcher runs before
/// every `repo-changed` — and by `notify_worktree_removed`, so a cache hit can
/// only ever be from the current refresh.
async fn cached_worktree_paths(
    state: &AppState,
    repo_path: String,
) -> Result<HashMap<String, crate::worktree::WorkspaceWorktree>, String> {
    let p = repo_path.clone();
    cached_try(
        state.git_cache.worktree_paths.clone(),
        repo_path,
        move || git_reads().worktree_paths(Path::new(&p)),
    )
    .await
    .map_err(|e| format!("get_worktree_paths failed: {e}"))
}

/// Core implementation of get_repo_summary, callable from both Tauri command and HTTP route.
/// Shares the progressive loading caches, including their single-flight loaders.
pub(crate) async fn get_repo_summary_impl(
    state: &Arc<AppState>,
    repo_path: String,
) -> Result<RepoSummary, String> {
    let structure = get_repo_structure_impl(state, repo_path.clone()).await?;
    let stats = get_repo_diff_stats_impl(state, repo_path).await?;
    Ok(RepoSummary {
        worktree_paths: structure.worktree_paths,
        merged_branches: structure.merged_branches,
        diff_stats: stats.diff_stats,
        last_commit_ts: stats.last_commit_ts,
        workspace_statuses: stats.workspace_statuses,
    })
}

/// Single IPC replacement for the N+2 calls in refreshAllBranchStats.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn get_repo_summary(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
) -> Result<RepoSummary, String> {
    get_repo_summary_impl(&state, repo_path).await
}

/// Fast structural snapshot: worktree paths + merged branches only.
/// Used by progressive loading Phase 1 — returns before expensive diff stats.
pub(crate) async fn get_repo_structure_impl(
    state: &AppState,
    repo_path: String,
) -> Result<RepoStructure, String> {
    // Bound monitoring across repositories; the cache coalesces concurrent reads.
    let _permit = state.monitoring_git_permit().await;
    let mb_path = repo_path.clone();
    let (worktree_paths, merged_branches) = tokio::join!(
        cached_worktree_paths(state, repo_path.clone()),
        cached_try(
            state.git_cache.merged_branches.clone(),
            repo_path.clone(),
            move || get_merged_branches_impl(Path::new(&mb_path)),
        ),
    );

    Ok(RepoStructure {
        worktree_paths: worktree_paths?,
        merged_branches: merged_branches?,
    })
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn get_repo_structure(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
) -> Result<RepoStructure, String> {
    get_repo_structure_impl(&state, repo_path).await
}

/// Per-worktree diff stats + last-commit timestamps.
/// Used by progressive loading Phase 2 — runs after structure is already displayed.
pub(crate) async fn get_repo_diff_stats_impl(
    state: &Arc<AppState>,
    repo_path: String,
) -> Result<RepoDiffStats, String> {
    // Bound monitoring across repositories; the cache coalesces concurrent reads.
    let _permit = state.monitoring_git_permit().await;
    // Need worktree paths to know which directories to diff. Phase 1
    // (`get_repo_structure`) of this same refresh already read them.
    let worktree_paths = cached_worktree_paths(state, repo_path.clone()).await?;
    let path = repo_path.clone();
    let proof_state = Arc::clone(state);
    cached_try(
        state.git_cache.repo_diff_stats.clone(),
        repo_path,
        move || {
            let mut diff_stats = HashMap::new();
            let mut workspace_statuses = HashMap::new();
            let branches: Vec<_> = worktree_paths.values().map(|w| w.branch.clone()).collect();
            let ref_keys = tuic_git::worktree::monitoring_ref_keys(Path::new(&path), &branches);
            for (id, workspace) in &worktree_paths {
                let (diff, dirty) =
                    crate::monitoring_status::CACHE.read(Path::new(&workspace.path));
                diff_stats.insert(workspace.path.clone(), diff);
                workspace_statuses.insert(
                    id.clone(),
                    tuic_git::worktree::inspect_workspace_monitoring_with_status(
                        Path::new(&path),
                        workspace,
                        ref_keys.as_ref().map_err(String::as_str).and_then(|keys| {
                            keys.get(&workspace.branch)
                                .map(String::as_str)
                                .ok_or("missing monitoring ref key")
                        }),
                        dirty,
                        |repo, branch, tip| {
                            crate::worktree::merged_github_pr_proves_tip_monitoring(
                                &proof_state,
                                repo,
                                branch,
                                tip,
                            )
                        },
                    ),
                );
            }
            let branches: Vec<_> = worktree_paths.values().map(|w| w.branch.clone()).collect();
            Ok(RepoDiffStats {
                diff_stats,
                last_commit_ts: get_last_commit_timestamps(Path::new(&path), &branches),
                workspace_statuses,
            })
        },
    )
    .await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn get_repo_diff_stats(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
) -> Result<RepoDiffStats, String> {
    get_repo_diff_stats_impl(&state, repo_path).await
}

/// Get rich branch details for a repository (cached, 5s TTL).
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn get_branches_detail(
    state: State<'_, Arc<AppState>>,
    path: String,
) -> Result<Vec<BranchDetail>, String> {
    branches_detail_cached(&state, path).await
}

/// HTTP/remote-safe cached read shared with the Tauri command above (no Tauri `State`).
pub(crate) async fn branches_detail_cached(
    state: &Arc<AppState>,
    path: String,
) -> Result<Vec<BranchDetail>, String> {
    let p = path.clone();
    cached_try(state.git_cache.branches_detail.clone(), path, move || {
        git_reads().branches_detail(Path::new(&p))
    })
    .await
}

/// Result of a background git command execution
#[derive(Clone, Serialize)]
pub(crate) struct GitCommandResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Ensure the SSH askpass helper script exists in the config directory.
/// Returns the path to the script. The script shows a native GUI dialog
/// so SSH can prompt for passphrases without a TTY.
pub(crate) fn ensure_askpass_script() -> Option<PathBuf> {
    let dir = crate::config::config_dir();
    let script_path = dir.join("ssh-askpass");

    if script_path.exists() {
        return Some(script_path);
    }

    #[cfg(target_os = "macos")]
    let content = r#"#!/bin/bash
# TUICommander SSH askpass helper — shows a native macOS dialog
exec osascript -e "display dialog \"$1\" default answer \"\" with hidden answer with title \"SSH Authentication\"" -e 'text returned of result'
"#;

    #[cfg(target_os = "linux")]
    let content = r#"#!/bin/bash
# TUICommander SSH askpass helper — tries zenity, then kdialog
if command -v zenity >/dev/null 2>&1; then
    exec zenity --password --title="SSH Authentication" --text="$1"
elif command -v kdialog >/dev/null 2>&1; then
    exec kdialog --password "$1" --title "SSH Authentication"
else
    exit 1
fi
"#;

    #[cfg(target_os = "windows")]
    let content = r#"@echo off
REM TUICommander SSH askpass — not supported on Windows without a helper
exit /b 1
"#;

    if let Err(e) = fs::write(&script_path, content) {
        tracing::error!(source = "git", "Failed to write askpass script: {e}");
        return None;
    }

    // Make executable on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755));
    }

    Some(script_path)
}

/// Git subcommands that can block on something off this machine, so every one
/// of them runs under [`crate::git_cli::FETCH_TIMEOUT`].
///
/// Audit of the shared allowlist (`ALLOWED_GIT_SUBCOMMANDS` in this module), which is what a browser or remote client can
/// reach: `fetch`, `pull` and `push` always contact a remote. `remote` does for
/// `update` and `prune`, and is bounded whole because its local forms
/// (`remote -v`, `remote add`) return in milliseconds, so a deadline can only
/// ever fire on the network ones. The other eleven — `stash`, `log`, `diff`,
/// `show`, `branch`, `tag`, `merge`, `rebase`, `cherry-pick`, `status`,
/// `rev-parse` — read and write only the local object store. A slow one is slow
/// because the repo is big, not because a host stopped answering, and killing
/// it would abort work that was going to finish.
const NETWORK_GIT_SUBCOMMANDS: &[&str] = &["fetch", "pull", "push", "remote"];

/// Whether `args` names a subcommand that talks to a remote.
pub(crate) fn is_network_git_subcommand(args: &[String]) -> bool {
    args.first()
        .is_some_and(|sub| NETWORK_GIT_SUBCOMMANDS.contains(&sub.as_str()))
}

/// Allowed git subcommands for the HTTP endpoint.
/// GitPanel and sidebar operations supported by both transports.
pub(crate) const ALLOWED_GIT_SUBCOMMANDS: &[&str] = &[
    "fetch",
    "pull",
    "push",
    "stash",
    "log",
    "diff",
    "show",
    "branch",
    "tag",
    "merge",
    "rebase",
    "cherry-pick",
    "remote",
    "status",
    "rev-parse",
];

/// Reject caller-controlled Git options outside the flags used by the UI.
pub(crate) fn validate_git_command_args(args: &[String]) -> Result<(), String> {
    let subcommand = args.first().map(String::as_str).unwrap_or("");
    if !ALLOWED_GIT_SUBCOMMANDS.contains(&subcommand) {
        return Err(format!("Git subcommand \"{subcommand}\" is not allowed"));
    }
    let flags: &[&str] = match subcommand {
        "fetch" => &["--all"],
        "pull" => &["--ff-only"],
        "push" => &["-u", "--delete"],
        "diff" => &["--name-status"],
        "status" => &["--porcelain"],
        _ => &[],
    };
    for arg in &args[1..] {
        if arg.starts_with('-') && arg != "--" && !flags.contains(&arg.as_str()) {
            return Err(format!(
                "Git option \"{arg}\" is not allowed for {subcommand}"
            ));
        }
    }
    Ok(())
}

/// Run an allowlisted UI git command to completion, blocking the calling thread.
///
/// Shared by the Tauri command below and the `/repo/run-git` HTTP handler so the
/// two transports cannot drift: same deadline, same askpass wiring, same result
/// shape. A git-level failure — including a killed network command — is a
/// `GitCommandResult` with `success: false`, never an `Err`; callers inspect
/// `success` and `stderr`.
pub(crate) fn run_git_command_blocking(
    state: &Arc<AppState>,
    path: &str,
    args: &[String],
) -> GitCommandResult {
    if let Err(stderr) = validate_git_command_args(args) {
        return GitCommandResult {
            success: false,
            stdout: String::new(),
            stderr,
            exit_code: -1,
        };
    }
    let repo_path = PathBuf::from(path);
    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let mut builder = git_cmd(&repo_path).args(&args_str);

    // A network subcommand is the only one that can park this thread forever —
    // a credential helper on a prompt, a half-open connection, a wedged mount.
    if is_network_git_subcommand(args) {
        builder = builder.timeout(crate::git_cli::FETCH_TIMEOUT);
    }

    // Enable GUI-based SSH authentication so passphrase-protected keys work
    // without a TTY. SSH_ASKPASS_REQUIRE=prefer tells SSH to use the askpass
    // program even when stdin looks like it could be a terminal.
    if let Some(ref askpass_path) = ensure_askpass_script() {
        let askpass_str = askpass_path.to_string_lossy();
        builder = builder
            .env("SSH_ASKPASS", &askpass_str)
            .env("SSH_ASKPASS_REQUIRE", "prefer")
            .env("DISPLAY", ":0"); // Required on Linux for SSH_ASKPASS
    }

    match builder.run_raw() {
        Ok(o) => {
            let success = o.status.success();
            if success {
                state.invalidate_repo_caches(path);
            }
            GitCommandResult {
                success,
                stdout: String::from_utf8_lossy(&o.stdout).to_string(),
                stderr: String::from_utf8_lossy(&o.stderr).to_string(),
                exit_code: o.status.code().unwrap_or(-1),
            }
        }
        Err(e) => GitCommandResult {
            success: false,
            stdout: String::new(),
            stderr: format!("Failed to execute git: {e}"),
            exit_code: -1,
        },
    }
}

/// Run an allowlisted UI git command in the background (no PTY, no terminal).
/// Used by the sidebar Git Quick Actions (pull, push, fetch, stash).
/// Async so network operations (pull/push/fetch) don't block the IPC thread.
/// Sets SSH_ASKPASS so passphrase prompts show a native GUI dialog.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn run_git_command(
    state: State<'_, Arc<AppState>>,
    path: String,
    args: Vec<String>,
) -> Result<GitCommandResult, String> {
    let state_arc = state.inner().clone();

    tokio::task::spawn_blocking(move || run_git_command_blocking(&state_arc, &path, &args))
        .await
        .map_err(|e| format!("Git command task failed: {e}"))
}

// --- Tauri wrappers over tuic_git::git ---

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn get_remote_url(path: String) -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(move || tuic_git::git::get_remote_url_blocking(path))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn get_branch_base(
    path: String,
    branch_name: String,
) -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(move || tuic_git::git::get_branch_base_blocking(path, branch_name))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn get_recent_commits(
    path: String,
    count: Option<u32>,
) -> Result<Vec<RecentCommit>, String> {
    tokio::task::spawn_blocking(move || tuic_git::git::get_recent_commits_blocking(path, count))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn get_git_diff(path: String, scope: Option<String>) -> Result<String, String> {
    tokio::task::spawn_blocking(move || tuic_git::git::get_git_diff_blocking(path, scope))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn get_diff_stats(
    path: String,
    scope: Option<String>,
) -> Result<DiffStats, String> {
    tokio::task::spawn_blocking(move || tuic_git::git::get_diff_stats_blocking(path, scope))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn get_changed_files(
    path: String,
    scope: Option<String>,
) -> Result<Vec<ChangedFile>, String> {
    tokio::task::spawn_blocking(move || tuic_git::git::get_changed_files_blocking(path, scope))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn get_file_diff(
    path: String,
    file: String,
    scope: Option<String>,
    untracked: Option<bool>,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        tuic_git::git::get_file_diff_blocking(path, file, scope, untracked)
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn get_gutter_changes(
    path: String,
    file: String,
    scope: Option<String>,
) -> Result<Vec<GutterChange>, String> {
    tokio::task::spawn_blocking(move || {
        tuic_git::git::get_gutter_changes_blocking(path, file, scope)
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn get_initials(name: String) -> String {
    tuic_git::git::get_initials(name)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn check_is_main_branch(branch: String) -> bool {
    tuic_git::git::check_is_main_branch(branch)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn get_git_branches(path: String) -> Result<Vec<serde_json::Value>, String> {
    tokio::task::spawn_blocking(move || tuic_git::git::get_git_branches_blocking(path))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn get_recent_branches(
    path: String,
    limit: Option<usize>,
) -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(move || tuic_git::git::get_recent_branches_blocking(path, limit))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

async fn compute_working_tree_status(path: String) -> Result<WorkingTreeStatus, String> {
    tokio::task::spawn_blocking(move || tuic_git::git::compute_working_tree_status_blocking(path))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}
/// In-flight `get_working_tree_status` computations, keyed by repo path.
///
/// Deliberately NOT a TTL cache. ChangesTab stages a file and refetches
/// immediately; any retained value would answer that with pre-mutation state.
/// Single-flight collapses only callers that genuinely overlap, so it can never
/// return anything staler than a read started right now.
/// Keyed by repo path *and* working-tree generation: joining is only safe
/// between reads of the same generation. A read requested after a stage or a
/// discard must not be answered by a read that started before it.
/// Repo path plus the working-tree generation it was read at.
type WtStatusKey = (String, u64);
type WtStatusPublisher = tokio::sync::broadcast::Sender<Result<WorkingTreeStatus, String>>;

static WT_STATUS_IN_FLIGHT: std::sync::LazyLock<dashmap::DashMap<WtStatusKey, WtStatusPublisher>> =
    std::sync::LazyLock::new(dashmap::DashMap::new);

/// Either this call leads the read for its generation, or it waits for the one
/// already running.
enum Flight {
    Lead(WtStatusLeader),
    Follow(tokio::sync::broadcast::Receiver<Result<WorkingTreeStatus, String>>),
}

/// Join the read in flight for the current generation of `path`, or become its
/// leader.
fn enter_flight(path: &str) -> Flight {
    use dashmap::mapref::entry::Entry;

    let key: WtStatusKey = (path.to_string(), tuic_git::git::working_tree_epoch(path));
    // Subscribing happens under the entry lock and the leader publishes before
    // its guard removes the entry, so a follower cannot miss the value.
    match WT_STATUS_IN_FLIGHT.entry(key.clone()) {
        Entry::Occupied(e) => Flight::Follow(e.get().subscribe()),
        Entry::Vacant(e) => {
            e.insert(tokio::sync::broadcast::channel(1).0);
            Flight::Lead(WtStatusLeader(key))
        }
    }
}

/// Retires the in-flight entry when the leader finishes **or is cancelled**.
///
/// Cancellation is real: the HTTP route awaits this inside an axum handler, and
/// a client disconnect drops that future. Without this guard the entry would
/// outlive the leader and every later caller for the repo would wait on a
/// computation that no longer exists. Dropping the sender instead wakes
/// followers with `Closed`, which they answer by computing themselves.
struct WtStatusLeader(WtStatusKey);

impl Drop for WtStatusLeader {
    fn drop(&mut self) {
        WT_STATUS_IN_FLIGHT.remove(&self.0);
    }
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn get_working_tree_status(path: String) -> Result<WorkingTreeStatus, String> {
    let guard = match enter_flight(&path) {
        Flight::Lead(guard) => guard,
        Flight::Follow(mut rx) => {
            return match rx.recv().await {
                Ok(result) => result,
                // Leader cancelled before publishing — do the work ourselves
                // rather than surfacing an error the caller cannot act on.
                Err(_) => compute_working_tree_status(path).await,
            };
        }
    };

    let result = compute_working_tree_status(path.clone()).await;
    if let Some(tx) = WT_STATUS_IN_FLIGHT.get(&guard.0) {
        // Errs only when nobody is listening, which is the common case.
        let _ = tx.send(result.clone());
    }
    drop(guard);
    result
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn git_stage_files(path: String, files: Vec<String>) -> Result<(), String> {
    tokio::task::spawn_blocking(move || tuic_git::git::git_stage_files_blocking(path, files))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn git_unstage_files(path: String, files: Vec<String>) -> Result<(), String> {
    tokio::task::spawn_blocking(move || tuic_git::git::git_unstage_files_blocking(path, files))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn git_discard_files(path: String, files: Vec<String>) -> Result<(), String> {
    tokio::task::spawn_blocking(move || tuic_git::git::git_discard_files_blocking(path, files))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn git_apply_reverse_patch(
    path: String,
    patch: String,
    scope: Option<String>,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        tuic_git::git::git_apply_reverse_patch_blocking(path, patch, scope)
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn git_commit(
    path: String,
    message: String,
    amend: Option<bool>,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || tuic_git::git::git_commit_blocking(path, message, amend))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn get_commit_log(
    path: String,
    count: Option<u32>,
    after: Option<String>,
) -> Result<Vec<CommitLogEntry>, String> {
    tokio::task::spawn_blocking(move || tuic_git::git::get_commit_log_blocking(path, count, after))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn get_stash_list(path: String) -> Result<Vec<StashEntry>, String> {
    tokio::task::spawn_blocking(move || tuic_git::git::get_stash_list_blocking(path))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn git_stash_apply(path: String, stash_ref: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || tuic_git::git::git_stash_apply_blocking(path, stash_ref))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn git_stash_pop(path: String, stash_ref: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || tuic_git::git::git_stash_pop_blocking(path, stash_ref))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn git_stash_drop(path: String, stash_ref: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || tuic_git::git::git_stash_drop_blocking(path, stash_ref))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn git_stash_show(path: String, stash_ref: String) -> Result<String, String> {
    tokio::task::spawn_blocking(move || tuic_git::git::git_stash_show_blocking(path, stash_ref))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn get_file_history(
    path: String,
    file: String,
    count: Option<u32>,
    after: Option<String>,
) -> Result<Vec<CommitLogEntry>, String> {
    tokio::task::spawn_blocking(move || {
        tuic_git::git::get_file_history_blocking(path, file, count, after)
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn get_file_blame(path: String, file: String) -> Result<Vec<BlameLine>, String> {
    tokio::task::spawn_blocking(move || tuic_git::git::get_file_blame_blocking(path, file))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn get_commit_graph(
    path: String,
    count: Option<u32>,
) -> Result<Vec<GraphNode>, String> {
    tokio::task::spawn_blocking(move || tuic_git::git_graph::get_commit_graph_blocking(path, count))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use tuic_git::test_fixtures::setup_test_repo_with_commit;

    /// Progressive loading splits one refresh into `get_repo_structure` (phase 1)
    /// then `get_repo_diff_stats` (phase 2), and both need the branch→path map.
    /// Each was forking `git worktree list` for itself, so every repo-changed
    /// bump paid for the same read twice — while `git_cache.worktree_paths`,
    /// invalidated by exactly that event, sat unused by both.
    #[tokio::test]
    async fn repo_structure_and_diff_stats_share_one_worktree_paths_read() {
        let (_dir, path) = setup_test_repo_with_commit();
        let repo = path.to_string_lossy().to_string();
        let state = Arc::new(crate::state::tests_support::make_test_app_state());

        let structure = get_repo_structure_impl(&state, repo.clone())
            .await
            .expect("structure");
        assert!(
            !structure.worktree_paths.is_empty(),
            "phase 1 must read the worktree paths"
        );

        // Poisoning the shared entry makes reuse observable: a phase 2 that forks
        // git again would report the repo's real branch, never this sentinel.
        state.git_cache.worktree_paths.insert(
            repo.clone(),
            Arc::new(HashMap::from([(
                "sentinel-id".to_string(),
                crate::worktree::WorkspaceWorktree {
                    branch: "sentinel-branch".to_string(),
                    path: repo.clone(),
                    kind: crate::worktree::WorkspaceKind::Worktree,
                    warm_artifacts: None,
                    lifecycle_status: None,
                },
            )])),
        );

        let stats = get_repo_diff_stats_impl(&state, repo.clone())
            .await
            .expect("diff stats");
        assert!(
            stats.last_commit_ts.contains_key("sentinel-branch"),
            "phase 2 must reuse phase 1's worktree paths, got {:?}",
            stats.last_commit_ts.keys().collect::<Vec<_>>()
        );
    }
    /// `update_from_base` with the merge strategy must merge a diverged base even
    /// when `merge.ff=only` and the checked-out branch's `mergeOptions` say
    /// `--ff-only`: the explicit `--ff` it forwards is the last word.
    #[test]
    fn update_from_base_merge_succeeds_on_diverged_history_under_ff_only() {
        let (_dir, path) = setup_test_repo_with_commit();
        let repo = path.to_string_lossy().to_string();
        let git = |args: &[&str]| {
            git_cmd(&path).args(args).run().unwrap();
        };
        git(&["branch", "-M", "main"]);
        git(&["checkout", "-b", "feat"]);
        std::fs::write(path.join("feat.txt"), "feat").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "feat work"]);
        git(&["checkout", "main"]);
        std::fs::write(path.join("main.txt"), "main").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "main moves on"]);
        git(&["checkout", "feat"]);
        git(&["config", "branch.feat.tuicommander-base", "main"]);
        git(&["config", "merge.ff", "only"]);
        git(&["config", "branch.feat.mergeOptions", "--ff-only"]);
        let state = Arc::new(crate::state::tests_support::make_test_app_state());

        let msg = update_from_base_impl(&state, &repo, "feat", Some("merge"))
            .unwrap_or_else(|e| panic!("diverged merge under --ff-only: {e}"));
        assert_eq!(msg, "Merged main into feat");
        assert!(path.join("main.txt").exists());
        assert!(path.join("feat.txt").exists());
        let parents = git_cmd(&path)
            .args(["rev-list", "--parents", "-n", "1", "HEAD"])
            .run()
            .unwrap()
            .stdout
            .split_whitespace()
            .count()
            - 1;
        assert_eq!(parents, 2, "a diverged merge makes a merge commit");
    }

    // Catches a cached sidebar dirty badge surviving watcher invalidation.
    #[tokio::test]
    async fn monitoring_gitpoll_diff_cache_refreshes_dirty_badges_after_invalidation() {
        let (_dir, path) = setup_test_repo_with_commit();
        let repo = path.to_string_lossy().to_string();
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let first = get_repo_diff_stats_impl(&state, repo.clone())
            .await
            .unwrap();
        assert_eq!(
            first.workspace_statuses.len(),
            1,
            "fixture has one workspace"
        );
        assert!(
            first
                .workspace_statuses
                .values()
                .all(|status| status.dirty_files == Some(0))
        );
        std::fs::write(path.join("new-untracked.txt"), "dirty").unwrap();
        state.invalidate_repo_caches(&repo);
        let (a, b) = tokio::join!(
            get_repo_diff_stats_impl(&state, repo.clone()),
            get_repo_diff_stats_impl(&state, repo.clone()),
        );
        for stats in [a.unwrap(), b.unwrap()] {
            assert_eq!(
                stats.workspace_statuses.keys().collect::<Vec<_>>(),
                first.workspace_statuses.keys().collect::<Vec<_>>(),
                "refresh must preserve the fixture workspace"
            );
            assert!(
                stats
                    .workspace_statuses
                    .values()
                    .all(|status| status.dirty_files == Some(1))
            );
        }
    }

    // Catches: the retired private auto-fetch marker being accepted again and reaching git as an
    // unknown option.
    #[test]
    fn the_retired_auto_fetch_marker_is_rejected() {
        let args = ["fetch", "--all", "--tuic-auto-fetch"]
            .map(str::to_owned)
            .to_vec();
        assert!(validate_git_command_args(&args).is_err());
    }

    // Catches: a flag-table edit that rejects an argument vector the frontend still sends
    // (every `run_git_command` caller in src/, inventoried in story 1460-9ed8), or that
    // admits an option-shaped argument in a position the first-arg check never saw.
    #[test]
    fn git_option_policy_accepts_every_frontend_argument_vector_and_rejects_option_injection() {
        let vec_of = |args: &[&str]| args.iter().map(|a| (*a).to_owned()).collect::<Vec<_>>();
        for accepted in [
            vec!["pull"],
            vec!["pull", "--ff-only"],
            vec!["push"],
            vec!["push", "-u", "origin", "feature/x"],
            vec!["push", "origin", "--delete", "feature/x"],
            vec!["fetch", "--all"],
            vec!["fetch", "origin", "feature/x"],
            vec!["stash"],
            vec!["stash", "push"],
            vec!["stash", "pop"],
            vec!["merge", "origin/feature"],
            vec!["rebase", "origin/feature"],
            vec!["status", "--porcelain"],
            vec!["diff", "--name-status", "main...feature"],
        ] {
            assert_eq!(
                validate_git_command_args(&vec_of(&accepted)),
                Ok(()),
                "{accepted:?}"
            );
        }
        for rejected in [
            vec!["push", "--force"],
            vec!["pull", "--rebase"],
            vec!["fetch", "--all", "--upload-pack=x"],
            vec!["merge", "-s", "ours"],
            vec!["status", "--porcelain", "--ignore-submodules"],
            vec!["diff", "--name-status", "--ext-diff"],
            vec!["stash", "push", "-m", "x"],
            vec!["show", "--output=x", "HEAD"],
            vec!["branch", "-D", "main"],
            vec!["-c", "core.pager=x", "status"],
            vec!["--exec-path=x", "status"],
            vec![],
        ] {
            assert!(
                validate_git_command_args(&vec_of(&rejected)).is_err(),
                "{rejected:?}"
            );
        }
    }
}
