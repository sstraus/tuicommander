//! App-side half of the worktree module: config-driven directory and archive
//! script resolution, the removal paths that notify `AppState`, and the Tauri
//! wrappers over [`tuic_git::worktree`]. The domain functions are re-exported
//! so `crate::worktree::*` paths are unchanged.

use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
#[cfg(feature = "desktop")]
use tauri::State;

pub(crate) use tuic_git::worktree::*;

/// Call only on a blocking thread; the domain callback stays synchronous.
pub(crate) fn pr_proof(
    state: Arc<AppState>,
    rt: tokio::runtime::Handle,
) -> impl Fn(&Path, &str, &str) -> bool {
    move |repo, branch, tip| {
        rt.block_on(query_merged_github_pr(&state, repo, branch, tip))
            .unwrap_or(false)
    }
}

/// Capture the runtime before a domain wrapper fans out to scoped OS threads.
fn merged_github_pr_proves_tip(state: &Arc<AppState>) -> impl Fn(&Path, &str, &str) -> bool {
    let proof = tokio::runtime::Handle::try_current()
        .ok()
        .map(|rt| pr_proof(Arc::clone(state), rt));
    move |repo, branch, tip| proof.as_ref().is_some_and(|proof| proof(repo, branch, tip))
}

/// How long the sidebar refresh trusts a GitHub answer for one branch tip.
const PR_PROOF_TTL: Duration = Duration::from_secs(300);

static PR_PROOFS: std::sync::LazyLock<moka::sync::Cache<(PathBuf, String, String), bool>> =
    std::sync::LazyLock::new(|| {
        moka::sync::Cache::builder()
            .max_capacity(1024)
            .time_to_live(PR_PROOF_TTL)
            .build()
    });

/// Listing must not wait for GitHub behind the native CLI response deadline.
/// A miss is unproven; normal monitoring may populate this same definitive cache.
pub(crate) fn merged_github_pr_proves_tip_cached(repo: &Path, branch: &str, tip: &str) -> bool {
    PR_PROOFS
        .get(&(repo.to_path_buf(), branch.to_owned(), tip.to_owned()))
        .unwrap_or(false)
}

/// Sidebar monitoring variant: every repository refresh asked GitHub about every
/// unmerged branch (one paginated GraphQL lookup each), many times a minute under agent
/// writes. A definitive answer is reused for `PR_PROOF_TTL`; a failed lookup is
/// not remembered. Removal preflights keep the uncached function above.
pub(crate) fn merged_github_pr_proves_tip_monitoring(
    state: &Arc<AppState>,
    repo: &Path,
    branch: &str,
    tip: &str,
) -> bool {
    cached_pr_proof(&PR_PROOFS, repo, branch, tip, || {
        let rt = tokio::runtime::Handle::try_current().ok()?;
        rt.block_on(query_merged_github_pr(state, repo, branch, tip))
    })
}

fn cached_pr_proof(
    cache: &moka::sync::Cache<(PathBuf, String, String), bool>,
    repo: &Path,
    branch: &str,
    tip: &str,
    lookup: impl FnOnce() -> Option<bool>,
) -> bool {
    let key = (repo.to_path_buf(), branch.to_owned(), tip.to_owned());
    if let Some(answer) = cache.get(&key) {
        return answer;
    }
    match lookup() {
        Some(answer) => {
            cache.insert(key, answer);
            answer
        }
        None => false,
    }
}

/// `None` when GitHub could not be asked or did not answer.
async fn query_merged_github_pr(
    state: &AppState,
    repo: &Path,
    branch: &str,
    tip: &str,
) -> Option<bool> {
    let api = crate::github_pr_api::RepoApi::resolve(state, repo.to_str()?)
        .await
        .ok()?;
    let variables = serde_json::json!({"owner":api.owner, "name":api.repo, "branch":branch});
    query_merged_github_pr_with(&api, variables, repo, branch, tip).await
}

async fn query_merged_github_pr_with(
    api: &impl crate::github_pr_api::PrApi,
    variables: serde_json::Value,
    repo: &Path,
    branch: &str,
    tip: &str,
) -> Option<bool> {
    let query = r#"query($owner: String!, $name: String!, $branch: String!, $endCursor: String) {
      repository(owner: $owner, name: $name) {
        pullRequests(first: 100, after: $endCursor, headRefName: $branch, states: [MERGED]) {
          nodes { number state headRefName headRefOid }
          pageInfo { hasNextPage endCursor }
        }
      }
    }"#;
    const MAX_PAGES: usize = 100;
    let pages = crate::github_pr_api::graphql_pages(
        api,
        query,
        variables,
        "/data/repository/pullRequests",
        MAX_PAGES,
    )
    .await
    .ok()?;
    let pages = serde_json::Value::Array(pages);
    Some(tuic_git::worktree::merged_pr_proof_from_pages(
        repo, branch, tip, &pages,
    ))
}

pub(crate) fn inspect_workspace_lifecycle(
    state: &Arc<AppState>,
    base_repo: &Path,
    workspace_id: &str,
) -> WorkspaceLifecycleStatus {
    tuic_git::worktree::inspect_workspace_lifecycle_with_pr(
        base_repo,
        workspace_id,
        merged_github_pr_proves_tip(state),
    )
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct WorktreeRemovalPreview {
    #[serde(flatten)]
    pub lifecycle: WorkspaceLifecycleStatus,
    pub untracked_files: Option<usize>,
    pub live_sessions: Vec<WorktreeLiveSession>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct WorktreeLiveSession {
    pub session_id: String,
    pub name: String,
}

/// Durable tabs still own their checkout even though their PTY has ended.
fn suspended_users_in(checkout: &Path) -> Result<Vec<WorktreeLiveSession>, String> {
    let root = checkout
        .canonicalize()
        .unwrap_or_else(|_| checkout.to_path_buf());
    Ok(crate::suspended_sessions::list()?
        .into_values()
        .filter(|record| {
            std::iter::once(Path::new(&record.cwd))
                .chain(
                    record
                        .worktree
                        .as_ref()
                        .map(|worktree| worktree.path.as_path()),
                )
                .any(|path| {
                    path.canonicalize()
                        .unwrap_or_else(|_| path.to_path_buf())
                        .starts_with(&root)
                })
        })
        .map(|record| WorktreeLiveSession {
            session_id: record.tuic_session.clone(),
            name: format!(
                "Suspended tab: {}",
                record.name.as_deref().unwrap_or(&record.tuic_session)
            ),
        })
        .collect())
}

/// Suspended tabs cannot be terminated by a live-session confirmation.
fn guard_suspended_users(checkout: &Path) -> Result<(), String> {
    let users = suspended_users_in(checkout)?;
    if users.is_empty() {
        Ok(())
    } else {
        Err(live_session_reason(&users))
    }
}

pub(crate) fn guard_suspended_workspace(repo: &Path, workspace_id: &str) -> Result<(), String> {
    if let Ok(workspace) = resolve_any_workspace(repo, workspace_id) {
        guard_suspended_users(Path::new(&workspace.path))?;
    }
    Ok(())
}

/// Live sessions and durable suspended tabs inside `checkout`. The one source of
/// "who is working in this checkout" for every removal guard.
pub(crate) fn live_sessions_in(state: &AppState, checkout: &Path) -> Vec<WorktreeLiveSession> {
    let root = checkout
        .canonicalize()
        .unwrap_or_else(|_| checkout.to_path_buf());
    let mut live_sessions = Vec::new();
    for entry in &state.session_maps.sessions {
        let session = entry.value().lock();
        let cwd = session.cwd.as_ref().map(PathBuf::from).or_else(|| {
            session
                .worktree
                .as_ref()
                .map(|worktree| worktree.path.clone())
        });
        if cwd.is_some_and(|cwd| cwd.canonicalize().unwrap_or(cwd).starts_with(&root)) {
            live_sessions.push(WorktreeLiveSession {
                session_id: entry.key().clone(),
                name: session
                    .display_name
                    .clone()
                    .unwrap_or_else(|| entry.key().clone()),
            });
        }
    }
    match suspended_users_in(checkout) {
        Ok(users) => live_sessions.extend(users),
        Err(error) => {
            // Unknown durable ownership must veto allocation and automatic cleanup.
            tracing::error!(%error, "Cannot inspect suspended checkout users");
            live_sessions.push(WorktreeLiveSession {
                session_id: "suspended-storage-unavailable".into(),
                name: format!("Suspended tabs unavailable: {error}"),
            });
        }
    }
    live_sessions.sort_by(|a, b| a.session_id.cmp(&b.session_id));
    live_sessions
}

pub(crate) fn inspect_worktree_removal(
    state: &Arc<AppState>,
    repo_path: &Path,
    workspace_id: &str,
) -> WorktreeRemovalPreview {
    let lifecycle = inspect_workspace_lifecycle(state, repo_path, workspace_id);
    let worktree_path = tuic_git::worktree::resolve_removable_workspace(repo_path, workspace_id)
        .ok()
        .map(|workspace| PathBuf::from(workspace.path));
    let untracked_files = worktree_path.as_ref().and_then(|path| {
        git_cmd(path)
            .args([
                "status",
                "--porcelain",
                "--untracked-files=all",
                "--ignore-submodules=none",
            ])
            .run()
            .ok()
            .map(|output| {
                output
                    .stdout
                    .lines()
                    .filter(|line| line.starts_with("??"))
                    .count()
            })
    });
    let live_sessions = worktree_path
        .as_deref()
        .map(|path| live_sessions_in(state, path))
        .unwrap_or_default();
    let mut warnings = Vec::new();
    match lifecycle.commit_status {
        WorkspaceCommitStatus::InSync => {
            warnings.push("This branch has nothing of its own, not merged work".to_string())
        }
        WorkspaceCommitStatus::Merged => {
            warnings.push("This branch's commits are in the default branch".to_string())
        }
        WorkspaceCommitStatus::Unmerged => {
            warnings.push("This branch has unmerged commits".to_string())
        }
        WorkspaceCommitStatus::PushedUnmerged => {
            warnings.push("This branch is not merged, but all its commits are pushed".to_string())
        }
        WorkspaceCommitStatus::Unknown => {
            warnings.push("Branch history could not be verified".to_string())
        }
    }
    if let Some(total) = lifecycle.dirty_files.filter(|total| *total > 0) {
        let untracked = untracked_files.unwrap_or(0);
        warnings.push(format!(
            "{total} uncommitted files, including {untracked} untracked files"
        ));
    }
    warnings.extend(
        live_sessions
            .iter()
            .map(|session| format!("Live session: {}", session.name)),
    );
    WorktreeRemovalPreview {
        lifecycle,
        untracked_files,
        live_sessions,
        warnings,
    }
}

fn retain_finished_checkout(
    state: &Arc<AppState>,
    repo_path: &str,
    workspace_id: &str,
    delete_branch: bool,
    archive_script: Option<&str>,
    force: bool,
) -> Result<Option<RemoveWorktreeOutcome>, String> {
    if force || !delete_branch {
        return Ok(None);
    }
    let repo = Path::new(repo_path);
    let status = inspect_workspace_lifecycle(state, repo, workspace_id);
    if status.dirty_files != Some(0)
        || !status.submodule_unpushed_commits.is_empty()
        || !matches!(
            status.commit_status,
            WorkspaceCommitStatus::Merged | WorkspaceCommitStatus::InSync
        )
    {
        return Ok(None);
    }
    // A pooled checkout is already recycled; removal takes it from here.
    let Ok(workspace) = resolve_any_workspace(repo, workspace_id) else {
        return Ok(None);
    };
    let users = live_sessions_in(state, Path::new(&workspace.path));
    if !users.is_empty() {
        return Err(live_session_reason(&users));
    }
    if operation_head_branch(&workspace.path).is_some() {
        return Err("Cannot recycle a checkout during a Git operation".into());
    }
    archive_worktree(repo, workspace_id, archive_script)?;
    Ok(Some(RemoveWorktreeOutcome {
        branch_delete_warning: None,
        removal_rule: "recyclable".into(),
        branch: workspace.branch,
    }))
}

#[expect(
    clippy::too_many_arguments,
    reason = "flat safety-confirmation contract plus account state"
)]
pub(crate) fn remove_worktree_by_workspace_id_with_confirmation(
    state: &Arc<AppState>,
    repo_path: &str,
    workspace_id: &str,
    delete_branch: bool,
    archive_script: Option<&str>,
    force: bool,
    override_lock: bool,
    expected_fingerprint: Option<&str>,
) -> Result<RemoveWorktreeOutcome, String> {
    guard_suspended_workspace(Path::new(repo_path), workspace_id)?;
    if expected_fingerprint.is_none()
        && let Some(outcome) = retain_finished_checkout(
            state,
            repo_path,
            workspace_id,
            delete_branch,
            archive_script,
            force,
        )?
    {
        return Ok(outcome);
    }
    tuic_git::worktree::remove_worktree_by_workspace_id_with_confirmation_and_pr(
        repo_path,
        workspace_id,
        delete_branch,
        archive_script,
        force,
        override_lock,
        expected_fingerprint,
        merged_github_pr_proves_tip(state),
    )
}

// Keep the independently supplied boundary fields explicit; grouping changes this contract.
#[expect(
    clippy::too_many_arguments,
    reason = "flat IPC safety-confirmation contract"
)]
pub(crate) fn remove_worktree_with_presence_confirmation(
    state: &Arc<AppState>,
    repo_path: &str,
    workspace_id: &str,
    delete_branch: bool,
    archive_script: Option<&str>,
    force: bool,
    override_lock: bool,
    expected_fingerprint: Option<&str>,
    confirm_missing_checkout: bool,
) -> Result<RemoveWorktreeOutcome, String> {
    guard_suspended_workspace(Path::new(repo_path), workspace_id)?;
    if expected_fingerprint.is_none()
        && !confirm_missing_checkout
        && let Some(outcome) = retain_finished_checkout(
            state,
            repo_path,
            workspace_id,
            delete_branch,
            archive_script,
            force,
        )?
    {
        return Ok(outcome);
    }
    tuic_git::worktree::remove_worktree_by_workspace_id_with_missing_confirmation_and_pr(
        repo_path,
        workspace_id,
        delete_branch,
        archive_script,
        force,
        override_lock,
        expected_fingerprint,
        Some(confirm_missing_checkout),
        merged_github_pr_proves_tip(state),
    )
}

pub(crate) fn branch_integration(
    state: &Arc<AppState>,
    repo: &Path,
    branch: &str,
) -> Result<BranchIntegration, String> {
    tuic_git::worktree::branch_integration_with_pr(repo, branch, merged_github_pr_proves_tip(state))
}

pub(crate) fn branch_integrations(
    state: &Arc<AppState>,
    repo: &Path,
) -> Result<Vec<BranchIntegration>, String> {
    tuic_git::worktree::branch_integrations_with_pr(repo, merged_github_pr_proves_tip(state))
}

pub(crate) fn delete_integrated_local_branch(
    state: &Arc<AppState>,
    repo_path: &str,
    branch_name: &str,
) -> Result<tuic_git::worktree::DeletedBranch, String> {
    tuic_git::worktree::delete_integrated_local_branch_with_pr(
        repo_path,
        branch_name,
        merged_github_pr_proves_tip(state),
    )
}

pub fn spawn_background_warm(
    source: PathBuf,
    destination: PathBuf,
    token: u64,
    warm: impl FnOnce(&Path, &Path) -> tuic_git::cow::WarmingReport + Send + 'static,
) -> tokio::task::JoinHandle<()> {
    tokio::task::spawn_blocking(move || {
        tuic_git::worktree::finish_background_warm_blocking(source, destination, token, warm)
    })
}

use crate::git_cli::{finish_failed_git_operation_after_abort, git_cmd};
use crate::state::AppState;

/// Resolve the effective archive_script for a repo from the three-tier config:
/// per-repo settings → repo-local .tuic.json → global defaults.
/// Returns None if no script is configured at any level.
pub(crate) fn resolve_archive_script(repo_path: &str) -> Option<String> {
    // 1. Per-repo app settings (highest priority)
    let repo_settings = crate::config::load_repo_settings();
    if let Some(entry) = repo_settings.repos.get(repo_path)
        && let Some(ref script) = entry.archive_script
        && !script.is_empty()
    {
        return Some(script.clone());
    }
    // .tuic.json scripts intentionally skipped — executing repo-committed
    // scripts without TOFU prompt is unsafe. Re-add when trust-on-first-use
    // confirmation is implemented.
    // 2. Global repo defaults (lowest priority)
    let defaults = crate::config::load_repo_defaults();
    if !defaults.archive_script.is_empty() {
        return Some(defaults.archive_script);
    }
    None
}

/// Resolve the worktree base directory for a given repo + storage strategy.
///
/// - `Sibling`: `{repo_parent}/{repo_name}__wt/`
/// - `AppDir`: `{app_config_dir}/worktrees/{repo_name}/`
/// - `InsideRepo`: `{repo_path}/.worktrees/`
/// - `ClaudeCodeDefault`: `{repo_path}/.claude/worktrees/`
pub(crate) fn resolve_worktree_dir(
    repo_path: &Path,
    strategy: &crate::config::WorktreeStorage,
    app_worktrees_dir: &Path,
) -> PathBuf {
    use crate::config::WorktreeStorage;
    match strategy {
        WorktreeStorage::Sibling => {
            let repo_name = repo_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "repo".to_string());
            let parent = repo_path.parent().unwrap_or(repo_path);
            parent.join(format!("{repo_name}__wt"))
        }
        WorktreeStorage::AppDir => {
            let repo_name = repo_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "repo".to_string());
            app_worktrees_dir.join(repo_name)
        }
        WorktreeStorage::InsideRepo => repo_path.join(".worktrees"),
        WorktreeStorage::ClaudeCodeDefault => repo_path.join(".claude").join("worktrees"),
    }
}

/// Resolve the effective worktree directory for a repo by loading config from disk.
/// Per-repo `worktree_storage` overrides the global default from repo-defaults.
pub(crate) fn resolve_worktree_dir_for_repo(repo_path: &Path, app_worktrees_dir: &Path) -> PathBuf {
    let repo_path_str = repo_path.to_string_lossy();
    let repo_settings = crate::config::load_repo_settings();
    let strategy = repo_settings
        .repos
        .get(repo_path_str.as_ref())
        .and_then(|entry| entry.worktree_storage.clone())
        .unwrap_or_else(|| crate::config::load_repo_defaults().worktree_storage);
    resolve_worktree_dir(repo_path, &strategy, app_worktrees_dir)
}

/// Shared allocation adapter for IPC and HTTP/MCP. Finished checkouts never
/// bypass the live-session guard, including descendants of the checkout root.
pub(crate) fn allocate_workspace(
    state: &Arc<AppState>,
    directory: &Path,
    config: &WorktreeConfig,
    base_ref: Option<&str>,
) -> Result<(CreatedWorkspace, Option<serde_json::Value>), String> {
    let (workspace, recycled) = create_workspace_recycling(directory, config, base_ref, |path| {
        live_sessions_in(state, path).is_empty()
    })?;
    let rename = recycled.as_ref().map(|old| {
        serde_json::json!({
            "old_path": old.path, "new_path": workspace.path.to_string_lossy()
        })
    });
    if let Some(old) = recycled {
        state.notify_worktree_removed(crate::state::WorktreeRemovedPayload {
            repo_path: config.base_repo.clone(),
            workspace_id: workspace_id_of_worktree(&old.branch),
            branch: old.branch,
        });
    }
    Ok((workspace, rename))
}

/// Create a linked worktree without a PTY session.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn create_worktree(
    state: State<'_, Arc<AppState>>,
    base_repo: String,
    branch_name: String,
    create_branch: Option<bool>,
    base_ref: Option<String>,
) -> Result<serde_json::Value, String> {
    let config = WorktreeConfig {
        task_name: branch_name.clone(),
        base_repo: base_repo.clone(),
        branch: Some(branch_name),
        create_branch: create_branch.unwrap_or(true),
    };
    let worktrees_dir =
        resolve_worktree_dir_for_repo(Path::new(&config.base_repo), &state.worktrees_dir);
    let allocation_state = Arc::clone(&state);
    let (workspace, recycled) = tokio::task::spawn_blocking(move || {
        allocate_workspace(
            &allocation_state,
            &worktrees_dir,
            &config,
            base_ref.as_deref(),
        )
    })
    .await
    .map_err(|error| format!("Task panic: {error}"))??;

    let token = begin_warm(&workspace.path);
    if recycled.is_some() {
        finish_warm(
            &workspace.path,
            token,
            serde_json::json!({"status": "done"}),
        );
    } else {
        spawn_background_warm(
            PathBuf::from(&base_repo),
            workspace.path.clone(),
            token,
            crate::cow::warm_worktree,
        );
    }
    state.invalidate_repo_caches(&base_repo);
    let mut response = ipc_worktree_response(&workspace, &base_repo);
    response["worktree_rename"] = serde_json::json!(recycled);
    Ok(response)
}

/// Get worktrees directory path.
/// When `repo_path` is provided, resolves the effective storage strategy for the repo.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn get_worktrees_dir(
    state: State<'_, Arc<AppState>>,
    repo_path: Option<String>,
) -> String {
    match repo_path {
        Some(rp) => resolve_worktree_dir_for_repo(Path::new(&rp), &state.worktrees_dir)
            .to_string_lossy()
            .to_string(),
        None => state.worktrees_dir.to_string_lossy().to_string(),
    }
}

/// Remove one workspace's checkout by workspace id (Tauri command with cache invalidation)
///
/// `delete_branch` defaults to `false` with force and `true` otherwise.
#[cfg(feature = "desktop")]
#[tauri::command]
// Keep the independently supplied boundary fields explicit; grouping changes this contract.
#[expect(
    clippy::too_many_arguments,
    reason = "flat IPC safety-confirmation contract"
)]
pub(crate) async fn remove_worktree(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
    workspace_id: String,
    delete_branch: Option<bool>,
    force: Option<bool>,
    override_lock: Option<bool>,
    expected_fingerprint: Option<String>,
    confirm_missing_checkout: Option<bool>,
) -> Result<RemoveWorktreeOutcome, String> {
    let force = force.unwrap_or(false);
    let confirm_missing_checkout = confirm_missing_checkout.unwrap_or(false);
    if force && expected_fingerprint.is_none() && !confirm_missing_checkout {
        return Err(
            "force requires expected_fingerprint from the confirmed lifecycle status".into(),
        );
    }
    let delete_branch = delete_branch.unwrap_or(!force);
    let override_lock = override_lock.unwrap_or(false);
    tracing::info!(
        source = "worktree",
        workspace_id = %workspace_id,
        repo = %repo_path,
        delete_branch = %delete_branch,
        force = %force,
        "remove_worktree command: invoked"
    );
    let script = resolve_archive_script(&repo_path);
    let repo_path_clone = repo_path.clone();
    let workspace_id_clone = workspace_id.clone();
    let proof_state = Arc::clone(&state);
    let result = tokio::task::spawn_blocking(move || {
        remove_worktree_with_presence_confirmation(
            &proof_state,
            &repo_path_clone,
            &workspace_id_clone,
            delete_branch,
            script.as_deref(),
            force,
            override_lock,
            expected_fingerprint.as_deref(),
            confirm_missing_checkout,
        )
    })
    .await
    .map_err(|e| format!("Task panic: {e}"))?;

    match result {
        Ok(outcome) => {
            tracing::info!(source = "worktree", workspace_id = %workspace_id, "remove_worktree command: SUCCESS — invalidating caches");
            if outcome.branch_delete_warning.is_none() {
                // Branch labels are branch-keyed, so a removed worktree drops the
                // label only after the branch itself was deleted successfully.
                crate::config::remove_branch_label(&repo_path, &outcome.branch);
            }
            state.notify_worktree_removed(crate::state::WorktreeRemovedPayload {
                repo_path: repo_path.clone(),
                workspace_id: workspace_id.clone(),
                branch: outcome.branch.clone(),
            });
            Ok(outcome)
        }
        Err(e) => {
            tracing::error!(source = "worktree", workspace_id = %workspace_id, "remove_worktree command: FAILED — {e}");
            Err(e)
        }
    }
}

/// Tauri command: delete a local branch.
///
/// `keep_worktree` (optional, default `false`): when `true`, preserves the
/// linked worktree directory by detaching its HEAD before removing the branch
/// ref. Used by the post-merge cleanup dialog when the user unchecks the
/// "Archive/Delete worktree" step.
///
/// Async + `spawn_blocking` because the body runs `git branch -d` and may
/// remove a whole worktree directory. A plain `fn` command runs inline on the
/// IPC thread — the macOS main thread — so it froze the WebView for the length
/// of the delete. Its HTTP twin already offloaded; see
/// `docs/backend/command-threading.md`.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn delete_local_branch(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
    branch_name: String,
    workspace_id: String,
    keep_worktree: Option<bool>,
) -> Result<(), String> {
    let keep_worktree = keep_worktree.unwrap_or(false);
    {
        let repo_path = repo_path.clone();
        let branch_name = branch_name.clone();
        let workspace_id = workspace_id.clone();
        tokio::task::spawn_blocking(move || {
            guard_suspended_workspace(Path::new(&repo_path), &workspace_id)?;
            delete_local_branch_impl(&repo_path, &branch_name, &workspace_id, keep_worktree)
        })
        .await
        .map_err(|e| format!("Task panic: {e}"))??;
    }
    if keep_worktree {
        state.invalidate_repo_caches(&repo_path);
    } else {
        // The workspace's checkout went with the branch — the sidebar row must go too.
        // `delete_local_branch_impl` refuses when the branch and the id disagree,
        // so by here `branch_name` is this workspace's own branch.
        state.notify_worktree_removed(crate::state::WorktreeRemovedPayload {
            repo_path: repo_path.clone(),
            workspace_id: workspace_id.clone(),
            branch: branch_name.clone(),
        });
    }
    Ok(())
}

/// Cached workspaces (id -> checkout) for synchronous callers (MCP handlers, etc.).
pub(crate) fn get_worktree_paths_cached(
    state: &crate::state::AppState,
    repo_path: &str,
) -> HashMap<String, WorkspaceWorktree> {
    let p = repo_path.to_string();
    (*state
        .git_cache
        .worktree_paths
        .get_with(repo_path.to_string(), || {
            std::sync::Arc::new(
                crate::git_reads::git_reads()
                    .worktree_paths(std::path::Path::new(&p))
                    .unwrap_or_default(),
            )
        }))
    .clone()
}

/// Remove an orphan checkout by path after the shared guard. Both the desktop
/// command and the MCP `worktree_remove` call this.
pub(crate) fn remove_orphan_checkout(
    state: &AppState,
    repo_path: &str,
    worktree_path: &str,
    safe_only: bool,
    confirmed_sessions: &[String],
) -> Result<(), String> {
    validate_worktree_path(repo_path, worktree_path)?;
    orphan_removal_guard(
        state,
        repo_path,
        worktree_path,
        safe_only,
        confirmed_sessions,
    )?;

    let path = PathBuf::from(worktree_path);
    let worktree = WorktreeInfo {
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| worktree_path.to_string()),
        path,
        branch: None,
        base_repo: PathBuf::from(repo_path),
    };
    tuic_git::worktree::remove_orphan_worktree_internal(&worktree)?;
    state.invalidate_repo_caches(repo_path);
    Ok(())
}

/// Remove an orphan worktree by its filesystem path (detached HEAD — no branch to look up).
///
/// Safety: `worktree_path` is validated against the repo's actual worktree list to prevent
/// arbitrary directory deletion via a crafted path. Safety checks, git subprocesses,
/// and recursive deletion run on the blocking pool, not the desktop IPC thread.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn remove_orphan_worktree(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
    worktree_path: String,
    safe_only: Option<bool>,
    confirmed_sessions: Option<Vec<String>>,
) -> Result<(), String> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        remove_orphan_checkout(
            &state,
            &repo_path,
            &worktree_path,
            safe_only.unwrap_or(false),
            &confirmed_sessions.unwrap_or_default(),
        )
    })
    .await
    .map_err(|e| format!("Task panic: {e}"))?
}

/// Result of switching the main worktree to a different branch.
#[derive(Clone, Serialize)]
pub(crate) struct SwitchBranchResult {
    pub(crate) success: bool,
    /// True if changes were auto-stashed before checkout
    pub(crate) stashed: bool,
    pub(crate) previous_branch: String,
    pub(crate) new_branch: String,
}

/// Switch the main worktree to a different branch.
///
/// Runs `git checkout` directly (no PTY involvement) so it's safe even when
/// terminals have editors or processes running — the caller is responsible
/// for checking terminal busy-state before invoking this.
///
/// When `stash` is true, performs `git stash push` before checkout and
/// does NOT auto-pop (the user can pop manually).
/// When `force` is true, passes `--force` to discard uncommitted changes.
/// Core logic for switching the checked-out branch. Blocking — callers wrap in
/// `spawn_blocking` when on an async runtime.
pub(crate) fn switch_branch_impl(
    state: &Arc<AppState>,
    repo_path: String,
    branch_name: String,
    force: bool,
    stash: bool,
) -> Result<SwitchBranchResult, String> {
    let base_repo = PathBuf::from(&repo_path);

    // `git checkout <ref>` has no end-of-options `--` form that keeps <ref> a
    // branch (post-`--` args become pathspecs), so validate instead — a name
    // beginning with `-` would otherwise be parsed as an option.
    crate::git::validate_branch_name(&branch_name)?;

    // Read current branch before switching
    let previous_branch = crate::git::read_branch_from_head(&base_repo).unwrap_or_default();

    if previous_branch == branch_name {
        return Ok(SwitchBranchResult {
            success: true,
            stashed: false,
            previous_branch: previous_branch.clone(),
            new_branch: previous_branch,
        });
    }

    // Check for uncommitted changes (unless force or stash)
    if !force && !stash {
        let status_out = git_cmd(&base_repo)
            .args(["status", "--porcelain"])
            .run()
            .map_err(|e| format!("Failed to check working tree status: {e}"))?;
        if !status_out.stdout.trim().is_empty() {
            return Err("dirty".to_string());
        }
    }

    // Stash if requested
    let did_stash = if stash {
        let stash_msg = format!("auto-stash before switching to {branch_name}");
        let stash_out = git_cmd(&base_repo)
            .args(["stash", "push", "-m", &stash_msg])
            .run()
            .map_err(|e| {
                crate::git_locks::describe_stale_lock(&base_repo)
                    .unwrap_or_else(|| format!("Stash failed: {e}"))
            })?;

        // "No local changes to save" means nothing was stashed
        !stash_out.stdout.contains("No local changes to save")
    } else {
        false
    };

    // Checkout
    let mut args = vec!["checkout"];
    if force {
        args.push("--force");
    }
    args.push(&branch_name);

    git_cmd(&base_repo).args(&args).run().map_err(|e| {
        crate::git_locks::describe_stale_lock(&base_repo)
            .unwrap_or_else(|| format!("Checkout failed: {e}"))
    })?;

    state.invalidate_repo_caches(&repo_path);

    Ok(SwitchBranchResult {
        success: true,
        stashed: did_stash,
        previous_branch,
        new_branch: branch_name,
    })
}

/// Switch the checked-out branch (Tauri command).
///
/// Async + `spawn_blocking`: a checkout stashes, rewrites the working tree and
/// can wait on an index lock. Inline on the IPC thread that is a frozen WebView
/// for the whole operation — see `docs/backend/command-threading.md`.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn switch_branch(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
    branch_name: String,
    force: bool,
    stash: bool,
) -> Result<SwitchBranchResult, String> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        switch_branch_impl(&state, repo_path, branch_name, force, stash)
    })
    .await
    .map_err(|e| format!("Task panic: {e}"))?
}

/// Create a local branch tracking a remote branch and switch to it.
/// Equivalent to `git checkout -b <branch> origin/<branch>`.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn checkout_remote_branch(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
    branch_name: String,
) -> Result<(), String> {
    let base_repo = PathBuf::from(&repo_path);
    let remote_ref = format!("origin/{branch_name}");

    git_cmd(&base_repo)
        .args(["checkout", "-b", &branch_name, &remote_ref])
        .run()
        .map_err(|e| format!("Checkout failed: {e}"))?;

    state.invalidate_repo_caches(&repo_path);
    Ok(())
}

/// Complete a pending merge by archiving or deleting the worktree.
///
/// Called after `merge_and_archive_worktree` returns `action: "pending"` (ask mode),
/// and by the auto-archive-merged sweep, which has no user in the loop at all. The
/// merge has already succeeded; this only handles the worktree cleanup.
///
/// `force` is the user's confirmation that a dirty worktree may be destroyed.
/// Without it, a worktree that is not known to be clean is left untouched and the
/// caller gets `needs_confirmation` — the same contract
/// `merge_and_archive_worktree_impl` uses, through the same gate.
///
/// Blocking — callers wrap in `spawn_blocking` when on an async runtime.
#[cfg(test)]
pub(crate) fn finalize_merged_worktree_impl(
    state: &Arc<AppState>,
    repo_path: String,
    workspace_id: String,
    action: String,
    force: bool,
) -> Result<MergeArchiveResult, String> {
    finalize_merged_worktree_impl_with_confirmation(
        state,
        repo_path,
        workspace_id,
        action,
        force,
        None,
    )
}

/// Both one-click merge cleanup and post-merge finalization use this review
/// gate. Before the merge, an unmerged commit is expected; afterwards, only
/// merged work may be cleaned up without an explicit confirmation.
fn cleanup_needs_lifecycle_confirmation(
    state: &Arc<AppState>,
    repo_path: &Path,
    workspace_id: &str,
    action: &str,
    force: bool,
    dirt: &WorktreeDirtiness,
    require_merged: bool,
) -> bool {
    if cleanup_needs_confirmation(action, force, dirt) {
        return true;
    }
    if force || (action != "archive" && action != "delete") {
        return false;
    }
    let preview = inspect_worktree_removal(state, repo_path, workspace_id);
    preview.lifecycle.removal_safety != WorkspaceRemovalSafety::Safe
        || !preview.live_sessions.is_empty()
        || preview.lifecycle.commit_status == WorkspaceCommitStatus::Unknown
        || (require_merged && preview.lifecycle.commit_status != WorkspaceCommitStatus::Merged)
}

pub(crate) fn finalize_merged_worktree_impl_with_confirmation(
    state: &Arc<AppState>,
    repo_path: String,
    workspace_id: String,
    action: String,
    force: bool,
    expected_fingerprint: Option<&str>,
) -> Result<MergeArchiveResult, String> {
    let script = resolve_archive_script(&repo_path);
    let base_repo = std::path::PathBuf::from(&repo_path);
    guard_suspended_workspace(&base_repo, &workspace_id)?;

    if let Some(expected) = expected_fingerprint {
        let workspace = resolve_any_workspace(&base_repo, &workspace_id)?;
        if dirty_fingerprint_at(Path::new(&workspace.path))?.0 != expected {
            return Err(
                "Worktree state changed since confirmation; review it before cleanup".into(),
            );
        }
    }

    let dirt = worktree_dirtiness(&base_repo, &workspace_id);
    if cleanup_needs_lifecycle_confirmation(
        state,
        &base_repo,
        &workspace_id,
        &action,
        force,
        &dirt,
        true,
    ) {
        return Ok(MergeArchiveResult {
            merged: true, // The merge itself already happened; only cleanup stopped.
            action: "needs_confirmation".to_string(),
            archive_path: None,
            commits_ahead: 0,
            worktree_dirty: dirt.is_dirty(),
            branch_delete_warning: None,
        });
    }

    match action.as_str() {
        "archive" => {
            // Read the branch off the record BEFORE the directory moves: archiving
            // takes the worktree out of `git worktree list`, after which the id
            // resolves to nothing and the removal event would have no branch to
            // show. Resolution here is also still safe to fail — nothing has been
            // mutated yet.
            let branch = resolve_workspace(&base_repo, &workspace_id)?.branch;
            let archive_path = archive_worktree(&base_repo, &workspace_id, script.as_deref())?;
            // Archiving moves the worktree out of the repo — as far as the sidebar
            // is concerned the row is gone, same as a delete.
            state.notify_worktree_removed(crate::state::WorktreeRemovedPayload {
                repo_path: repo_path.clone(),
                workspace_id: workspace_id.clone(),
                branch,
            });
            Ok(MergeArchiveResult {
                merged: true,
                action: "archived".to_string(),
                archive_path: Some(archive_path),
                // The merge already happened in the "pending" call that preceded
                // this one; its pre-flight numbers were reported there.
                commits_ahead: 0,
                worktree_dirty: dirt.is_dirty(),
                branch_delete_warning: None,
            })
        }
        "delete" => {
            let outcome = remove_worktree_by_workspace_id_with_confirmation(
                state,
                &repo_path,
                &workspace_id,
                true,
                script.as_deref(),
                force,
                false,
                expected_fingerprint,
            )?;
            state.notify_worktree_removed(crate::state::WorktreeRemovedPayload {
                repo_path: repo_path.clone(),
                workspace_id: workspace_id.clone(),
                branch: outcome.branch,
            });
            Ok(MergeArchiveResult {
                merged: true,
                action: "deleted".to_string(),
                archive_path: None,
                commits_ahead: 0,
                worktree_dirty: dirt.is_dirty(),
                branch_delete_warning: outcome.branch_delete_warning,
            })
        }
        _ => Err(format!(
            "Unknown action '{action}': expected 'archive' or 'delete'"
        )),
    }
}

/// Finalize a pending merge by archiving/deleting the worktree (Tauri command).
///
/// Async + `spawn_blocking`: archiving moves a directory and deleting removes
/// one, both unbounded. `finalize_merged_worktree_http` already offloaded, so
/// the same work froze the WebView over IPC and not over HTTP — the drift
/// `docs/backend/command-threading.md` exists to prevent.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn finalize_merged_worktree(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
    workspace_id: String,
    action: String,
    force: Option<bool>,
    expected_fingerprint: Option<String>,
) -> Result<MergeArchiveResult, String> {
    if force.unwrap_or(false) && expected_fingerprint.is_none() {
        return Err(
            "force requires expected_fingerprint from the confirmed lifecycle status".into(),
        );
    }
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        finalize_merged_worktree_impl_with_confirmation(
            &state,
            repo_path,
            workspace_id,
            action,
            force.unwrap_or(false),
            expected_fingerprint.as_deref(),
        )
    })
    .await
    .map_err(|e| format!("Task panic: {e}"))?
}

/// Merge a worktree branch into a target branch, then archive or delete the worktree.
///
/// Steps:
/// 1. `git checkout <target_branch>` (in the base repo)
/// 2. `git merge <source_branch>` (in the base repo)
/// 3. Based on `after_merge`: archive (move dir) or delete (remove worktree + branch)
///
/// Blocking — callers wrap in `spawn_blocking` when on an async runtime.
#[cfg(test)]
pub(crate) fn merge_and_archive_worktree_impl(
    state: &Arc<AppState>,
    repo_path: String,
    branch_name: String,
    workspace_id: String,
    target_branch: String,
    after_merge: String,
    force: bool,
) -> Result<MergeArchiveResult, String> {
    merge_and_archive_worktree_impl_with_confirmation(
        state,
        repo_path,
        branch_name,
        workspace_id,
        target_branch,
        after_merge,
        force,
        None,
    )
}

// Keep the independently supplied boundary fields explicit; grouping changes this contract.
#[expect(
    clippy::too_many_arguments,
    reason = "flat IPC safety-confirmation contract"
)]
pub(crate) fn merge_and_archive_worktree_impl_with_confirmation(
    state: &Arc<AppState>,
    repo_path: String,
    branch_name: String,
    workspace_id: String,
    target_branch: String,
    after_merge: String,
    force: bool,
    expected_fingerprint: Option<&str>,
) -> Result<MergeArchiveResult, String> {
    let script = resolve_archive_script(&repo_path);
    let base_repo = PathBuf::from(&repo_path);
    if after_merge == "archive" || after_merge == "delete" {
        guard_suspended_workspace(&base_repo, &workspace_id)?;
    }
    if let Some(expected) = expected_fingerprint {
        let workspace = resolve_any_workspace(&base_repo, &workspace_id)?;
        if dirty_fingerprint_at(Path::new(&workspace.path))?.0 != expected {
            return Err(
                "Worktree state changed since confirmation; review it before cleanup".into(),
            );
        }
    }

    // 0. Pre-flight: would the cleanup take uncommitted work with it? Both
    //    "archive" and "delete" end in `git worktree remove --force`, so any
    //    worktree not known to be clean must be confirmed first — whether or not
    //    the branch carries commits. `commits_ahead` is reported alongside so the
    //    dialog can also say that an empty branch's merge would be a no-op.
    let preflight = merge_preflight(&repo_path, &branch_name, &workspace_id, &target_branch);
    if cleanup_needs_lifecycle_confirmation(
        state,
        &base_repo,
        &workspace_id,
        &after_merge,
        force,
        &preflight.worktree_dirty,
        false,
    ) {
        return Ok(MergeArchiveResult {
            merged: false,
            action: "needs_confirmation".to_string(),
            archive_path: None,
            commits_ahead: preflight.commits_ahead,
            worktree_dirty: preflight.worktree_dirty.is_dirty(),
            branch_delete_warning: None,
        });
    }

    // 1. Ensure we're on the target branch in the base repo
    git_cmd(&base_repo)
        .args(["checkout", &target_branch])
        .run()
        .map_err(|e| {
            crate::git_locks::describe_stale_lock(&base_repo)
                .unwrap_or_else(|| format!("Failed to checkout {target_branch}: {e}"))
        })?;

    // 2. Merge the source branch
    //    `--ff` only when the user's config says `merge.ff=only` (see
    //    `merge_ff_override`): that policy would otherwise fail a diverged merge.
    if let Err(e) = git_cmd(&base_repo)
        .args(["merge", &branch_name, "--no-edit"])
        .args(crate::git_cli::merge_ff_override(
            &base_repo,
            Some(&target_branch),
        ))
        .run()
    {
        return Err(finish_failed_git_operation_after_abort(
            &base_repo,
            "merge",
            "Merge failed",
            e,
        ));
    }

    // 3. Handle the worktree based on after_merge setting
    let MergePreflight {
        commits_ahead,
        worktree_dirty,
    } = preflight;
    let worktree_dirty = worktree_dirty.is_dirty();
    match after_merge.as_str() {
        "archive" => {
            // Before the move, for the same reason as in `finalize_merged_worktree_impl`:
            // an archived worktree no longer resolves by id. `branch_name` is the
            // merge subject the caller named, which is not necessarily what this
            // workspace has checked out — the record is.
            let branch = resolve_workspace(&base_repo, &workspace_id)?.branch;
            let archive_path = archive_worktree(&base_repo, &workspace_id, script.as_deref())?;
            // Archiving moves the worktree out of the repo — as far as the sidebar
            // is concerned the row is gone, same as a delete.
            state.notify_worktree_removed(crate::state::WorktreeRemovedPayload {
                repo_path: repo_path.clone(),
                workspace_id: workspace_id.clone(),
                branch,
            });
            Ok(MergeArchiveResult {
                merged: true,
                action: "archived".to_string(),
                archive_path: Some(archive_path),
                commits_ahead,
                worktree_dirty,
                branch_delete_warning: None,
            })
        }
        "delete" => {
            let outcome = remove_worktree_by_workspace_id_with_confirmation(
                state,
                &repo_path,
                &workspace_id,
                true,
                script.as_deref(),
                force,
                false,
                expected_fingerprint,
            )?;
            state.notify_worktree_removed(crate::state::WorktreeRemovedPayload {
                repo_path: repo_path.clone(),
                workspace_id: workspace_id.clone(),
                branch: outcome.branch,
            });
            Ok(MergeArchiveResult {
                merged: true,
                action: "deleted".to_string(),
                archive_path: None,
                commits_ahead,
                worktree_dirty,
                branch_delete_warning: outcome.branch_delete_warning,
            })
        }
        _ => {
            // "ask" — merge succeeded, let frontend decide what to do next
            state.invalidate_repo_caches(&repo_path);
            Ok(MergeArchiveResult {
                merged: true,
                action: "pending".to_string(),
                archive_path: None,
                commits_ahead,
                worktree_dirty,
                branch_delete_warning: None,
            })
        }
    }
}

/// Merge a worktree branch into a target branch, then archive/delete (Tauri command).
///
/// `force` skips the pre-flight guard that refuses to clean up a branch carrying no
/// commits while its worktree is dirty. The frontend sets it after the user confirms.
#[cfg(feature = "desktop")]
#[tauri::command]
// Keep the independently supplied boundary fields explicit; grouping changes this contract.
#[expect(
    clippy::too_many_arguments,
    reason = "flat IPC safety-confirmation contract"
)]
pub(crate) async fn merge_and_archive_worktree(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
    branch_name: String,
    workspace_id: String,
    target_branch: String,
    after_merge: String,
    force: Option<bool>,
    expected_fingerprint: Option<String>,
) -> Result<MergeArchiveResult, String> {
    if force.unwrap_or(false) && expected_fingerprint.is_none() {
        return Err(
            "force requires expected_fingerprint from the confirmed lifecycle status".into(),
        );
    }
    let state = Arc::clone(&state);
    tokio::task::spawn_blocking(move || {
        merge_and_archive_worktree_impl_with_confirmation(
            &state,
            repo_path,
            branch_name,
            workspace_id,
            target_branch,
            after_merge,
            force.unwrap_or(false),
            expected_fingerprint.as_deref(),
        )
    })
    .await
    .map_err(|e| format!("Task panic: {e}"))?
}

// --- Tauri wrappers over tuic_git::worktree ---

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn check_worktree_dirty(
    repo_path: String,
    workspace_id: String,
) -> Result<bool, String> {
    tuic_git::worktree::check_worktree_dirty(repo_path, workspace_id)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn get_workspace_lifecycle(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
    workspace_id: String,
) -> Result<WorktreeRemovalPreview, String> {
    let state = Arc::clone(&state);
    tokio::task::spawn_blocking(move || {
        Ok(inspect_worktree_removal(
            &state,
            Path::new(&repo_path),
            &workspace_id,
        ))
    })
    .await
    .map_err(|e| format!("workspace lifecycle task failed: {e}"))?
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn get_worktree_paths(
    repo_path: String,
) -> Result<HashMap<String, WorkspaceWorktree>, String> {
    tuic_git::worktree::get_worktree_paths(repo_path)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn detect_orphan_worktrees(repo_path: String) -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(move || {
        tuic_git::worktree::detect_orphan_worktrees_blocking(repo_path)
    })
    .await
    .map_err(|e| format!("orphan worktree detection task failed: {e}"))?
}

/// A detached checkout's removal verdict, plus the sessions working in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct OrphanCleanupReview {
    #[serde(flatten)]
    pub assessment: tuic_git::worktree::OrphanCleanupAssessment,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub live_sessions: Vec<WorktreeLiveSession>,
}

fn live_session_reason(sessions: &[WorktreeLiveSession]) -> String {
    let names: Vec<&str> = sessions.iter().map(|s| s.name.as_str()).collect();
    format!("live session: {}", names.join(", "))
}

/// Git safety plus the session registry: an orphan checkout a session still
/// works in is never safe to remove without a human review. A checkout that
/// is already gone holds no work, so its stale sessions do not block.
pub(crate) fn orphan_cleanup_safety_with_sessions(
    state: &AppState,
    repo_path: &str,
    worktree_path: &str,
) -> Result<(), String> {
    tuic_git::worktree::orphan_cleanup_safety(repo_path, worktree_path)?;
    let path = Path::new(worktree_path);
    if !path.exists() {
        return Ok(());
    }
    let live = live_sessions_in(state, path);
    if live.is_empty() {
        Ok(())
    } else {
        Err(live_session_reason(&live))
    }
}

/// The removal guard both transports share. A safe-only removal needs the full
/// verdict. A review-confirmed one (`safe_only` false) needs no sessions beyond
/// those the user saw in the dialog: one that started since was never reviewed.
pub(crate) fn orphan_removal_guard(
    state: &AppState,
    repo_path: &str,
    worktree_path: &str,
    safe_only: bool,
    confirmed_sessions: &[String],
) -> Result<(), String> {
    guard_suspended_users(Path::new(worktree_path))?;
    if safe_only {
        return orphan_cleanup_safety_with_sessions(state, repo_path, worktree_path);
    }
    let path = Path::new(worktree_path);
    if !path.exists() {
        return Ok(());
    }
    let unseen: Vec<WorktreeLiveSession> = live_sessions_in(state, path)
        .into_iter()
        .filter(|session| !confirmed_sessions.contains(&session.session_id))
        .collect();
    if unseen.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{}; not part of the confirmed removal",
            live_session_reason(&unseen)
        ))
    }
}

pub(crate) fn assess_orphan_cleanup_with_sessions(
    state: &AppState,
    repo_path: &str,
) -> Result<Vec<OrphanCleanupReview>, String> {
    Ok(tuic_git::worktree::assess_orphan_worktrees(repo_path)?
        .into_iter()
        .map(|mut assessment| {
            let live_sessions = if Path::new(&assessment.path).exists() {
                live_sessions_in(state, Path::new(&assessment.path))
            } else {
                Vec::new()
            };
            if !live_sessions.is_empty() {
                assessment.safe = false;
                let live = live_session_reason(&live_sessions);
                assessment.reason = Some(match assessment.reason.take() {
                    Some(reason) => format!("{reason}; {live}"),
                    None => live,
                });
            }
            OrphanCleanupReview {
                assessment,
                live_sessions,
            }
        })
        .collect())
}

pub(crate) async fn assess_orphan_cleanup_internal(
    state: Arc<AppState>,
    repo_path: String,
) -> Result<Vec<OrphanCleanupReview>, String> {
    tokio::task::spawn_blocking(move || assess_orphan_cleanup_with_sessions(&state, &repo_path))
        .await
        .map_err(|error| format!("orphan cleanup assessment task failed: {error}"))?
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn assess_orphan_cleanup(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
) -> Result<Vec<OrphanCleanupReview>, String> {
    assess_orphan_cleanup_internal(state.inner().clone(), repo_path).await
}

#[derive(Clone)]
pub(crate) struct PendingOrphanCleanup {
    pub(crate) paths: Vec<String>,
    pub(crate) answer: Option<bool>,
    /// The dialog has ended. A Keep stays readable for other clients that still
    /// show it, but nothing is left to answer.
    pub(crate) settled: bool,
}

pub(crate) fn begin_orphan_cleanup_internal(
    state: &AppState,
    repo_path: &str,
    paths: Vec<String>,
) -> Result<(), String> {
    let current = tuic_git::worktree::assess_orphan_worktrees(repo_path)?;
    if paths.is_empty()
        || paths.iter().any(|path| {
            !current.iter().any(|entry| {
                tuic_core::path_spelling::portable_spelling(&entry.path)
                    == tuic_core::path_spelling::portable_spelling(path)
            })
        })
    {
        return Err("Pending cleanup must list current orphan worktrees".into());
    }
    state.pending_orphan_cleanup.insert(
        repo_path.to_string(),
        PendingOrphanCleanup {
            paths,
            answer: None,
            settled: false,
        },
    );
    Ok(())
}

pub(crate) fn answer_orphan_cleanup_internal(
    state: &AppState,
    repo_path: &str,
    remove: bool,
) -> Result<(), String> {
    let pending = state
        .pending_orphan_cleanup
        .get(repo_path)
        .filter(|entry| !entry.settled)
        .ok_or("No pending orphan cleanup for this repository")?
        .paths
        .clone();
    if remove {
        for path in &pending {
            orphan_cleanup_safety_with_sessions(state, repo_path, path)?;
        }
    }
    let mut current = state
        .pending_orphan_cleanup
        .get_mut(repo_path)
        .ok_or("Orphan cleanup was already dismissed")?;
    if current.paths != pending || current.answer.is_some() {
        return Err("Orphan cleanup changed while it was being answered".into());
    }
    current.answer = Some(remove);
    Ok(())
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn begin_orphan_cleanup(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
    paths: Vec<String>,
) -> Result<(), String> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || begin_orphan_cleanup_internal(&state, &repo_path, paths))
        .await
        .map_err(|error| format!("orphan cleanup registration task failed: {error}"))?
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn pending_orphan_cleanup_answer(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
) -> Option<bool> {
    state
        .pending_orphan_cleanup
        .get(&repo_path)
        .and_then(|entry| entry.answer)
}

/// End a client's cleanup dialog. A Keep stays on the shared entry as `answer =
/// Some(false)` so other clients polling it close their own dialog instead of
/// counting down to a removal; anything else drops the entry.
pub(crate) fn clear_orphan_cleanup_internal(state: &AppState, repo_path: &str, kept: bool) {
    if kept && let Some(mut entry) = state.pending_orphan_cleanup.get_mut(repo_path) {
        entry.answer = Some(false);
        entry.settled = true;
        return;
    }
    state.pending_orphan_cleanup.remove(repo_path);
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn clear_orphan_cleanup(state: State<'_, Arc<AppState>>, repo_path: String, kept: bool) {
    clear_orphan_cleanup_internal(&state, &repo_path, kept);
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn generate_worktree_name_cmd(existing_names: Vec<String>) -> String {
    tuic_git::worktree::generate_worktree_name_cmd(existing_names)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn generate_clone_branch_name_cmd(
    source_branch: String,
    existing_names: Vec<String>,
) -> String {
    tuic_git::worktree::generate_clone_branch_name_cmd(source_branch, existing_names)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn list_local_branches(repo_path: String) -> Result<Vec<String>, String> {
    tuic_git::worktree::list_local_branches(repo_path)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn list_base_ref_options(repo_path: String) -> Result<Vec<BaseRefOption>, String> {
    tuic_git::worktree::list_base_ref_options(repo_path)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn run_setup_script(script: String, cwd: String) -> Result<serde_json::Value, String> {
    tuic_git::worktree::run_setup_script(script, cwd)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WorktreeStorage;
    use std::fs;
    use tempfile::TempDir;
    use tuic_git::test_fixtures::{
        base_branch_of, dirty_worktree_with, setup_test_repo, worktree_with,
    };

    // Catches: a cached proof for another branch/tip (or a miss) invents merged status.
    #[test]
    fn listing_pr_proof_requires_matching_definitive_cached_answer() {
        let repo = tempfile::tempdir().unwrap();
        let path = repo.path();
        assert!(!merged_github_pr_proves_tip_cached(path, "branch", "tip"));
        PR_PROOFS.insert((path.to_path_buf(), "branch".into(), "tip".into()), true);
        assert!(merged_github_pr_proves_tip_cached(path, "branch", "tip"));
        assert!(!merged_github_pr_proves_tip_cached(path, "other", "tip"));
        assert!(!merged_github_pr_proves_tip_cached(
            path, "branch", "new-tip"
        ));
        PR_PROOFS.insert((path.to_path_buf(), "branch".into(), "tip".into()), false);
        assert!(!merged_github_pr_proves_tip_cached(path, "branch", "tip"));
    }

    // Catches: every sidebar refresh re-running `gh api graphql` per unmerged branch
    // (answered definitively before), while a failed lookup must stay retryable.
    #[test]
    fn pr_proof_monitoring_reuses_definitive_answers_and_retries_failures_1491() {
        let cache = moka::sync::Cache::builder().max_capacity(8).build();
        let repo = Path::new("/repo");
        let calls = std::cell::Cell::new(0);
        let ask = |answer: Option<bool>| {
            cached_pr_proof(&cache, repo, "branch", "tip", || {
                calls.set(calls.get() + 1);
                answer
            })
        };
        assert!(!ask(None));
        assert!(ask(Some(true)));
        assert!(ask(None));
        assert_eq!(calls.get(), 2, "the failure was retried, the answer reused");
        assert!(!cached_pr_proof(
            &cache,
            repo,
            "branch",
            "newer-tip",
            || Some(false)
        ));
    }

    // Catches accepting an empty or unregistered orphan list after weakening
    // the validation OR: neither request may create an actionable dialog.
    #[test]
    fn cleanup_rejects_empty_and_unknown_paths_1488() {
        let repo = setup_test_repo();
        assert!(
            repo.path()
                .canonicalize()
                .unwrap()
                .starts_with(tuic_test_support::test_temp_root().canonicalize().unwrap())
        );
        let state = crate::state::tests_support::make_test_app_state();
        let path = repo.path().to_string_lossy();
        for paths in [
            vec![],
            vec![repo.path().join("unknown").to_string_lossy().into_owned()],
        ] {
            assert!(begin_orphan_cleanup_internal(&state, &path, paths).is_err());
            assert!(state.pending_orphan_cleanup.get(path.as_ref()).is_none());
        }
    }

    // Catches replacing a client's answer because the paths are unchanged:
    // answer.is_some alone must forbid a second answer.
    #[test]
    fn cleanup_cannot_overwrite_an_existing_answer_1488() {
        let state = crate::state::tests_support::make_test_app_state();
        pending_cleanup(&state, "/repo");
        answer_orphan_cleanup_internal(&state, "/repo", false).unwrap();
        assert!(answer_orphan_cleanup_internal(&state, "/repo", false).is_err());
        assert_eq!(pending_answer(&state, "/repo"), Some(false));
    }

    fn pending_cleanup(state: &AppState, repo: &str) {
        state.pending_orphan_cleanup.insert(
            repo.to_string(),
            PendingOrphanCleanup {
                paths: vec!["/wt/a".into()],
                answer: None,
                settled: false,
            },
        );
    }

    fn pending_answer(state: &AppState, repo: &str) -> Option<bool> {
        state
            .pending_orphan_cleanup
            .get(repo)
            .and_then(|entry| entry.answer)
    }

    // Catches: clear deleting the entry after a Keep, so a second client that shows the
    // same dialog polls null, keeps counting down and removes the worktree (1289-27f8).
    #[test]
    fn clearing_a_kept_cleanup_leaves_the_keep_visible_to_other_clients() {
        let state = crate::state::tests_support::make_test_app_state();
        pending_cleanup(&state, "/repo");

        clear_orphan_cleanup_internal(&state, "/repo", true);

        assert_eq!(pending_answer(&state, "/repo"), Some(false));
    }

    // Catches: a Keep left on the shared entry turning every later answer into
    // "changed while it was being answered", so an agent could never remove the
    // orphan that had since become clean (orphan-dialog-repeats).
    #[test]
    fn answering_a_dialog_that_was_already_kept_reports_nothing_pending() {
        let state = crate::state::tests_support::make_test_app_state();
        pending_cleanup(&state, "/repo");
        clear_orphan_cleanup_internal(&state, "/repo", true);

        let error = answer_orphan_cleanup_internal(&state, "/repo", true).unwrap_err();

        assert_eq!(error, "No pending orphan cleanup for this repository");
        assert_eq!(pending_answer(&state, "/repo"), Some(false));
    }

    // Catches: a non-Keep clear leaving a stale entry behind that answers later dialogs.
    #[test]
    fn clearing_a_finished_cleanup_removes_the_entry() {
        let state = crate::state::tests_support::make_test_app_state();
        pending_cleanup(&state, "/repo");

        clear_orphan_cleanup_internal(&state, "/repo", false);

        assert!(state.pending_orphan_cleanup.get("/repo").is_none());
    }

    // Attack: a Keep for a dialog nobody registered must not invent a pending entry.
    #[test]
    fn keeping_without_a_pending_cleanup_creates_nothing() {
        let state = crate::state::tests_support::make_test_app_state();

        clear_orphan_cleanup_internal(&state, "/repo", true);

        assert!(state.pending_orphan_cleanup.get("/repo").is_none());
    }

    // Attack: two clients both end in Keep; the second must not flip the answer back.
    #[test]
    fn keeping_twice_stays_kept() {
        let state = crate::state::tests_support::make_test_app_state();
        pending_cleanup(&state, "/repo");

        clear_orphan_cleanup_internal(&state, "/repo", true);
        clear_orphan_cleanup_internal(&state, "/repo", true);

        assert_eq!(pending_answer(&state, "/repo"), Some(false));
    }

    // Attack: a kept entry is per repo and a new dialog starts unanswered again.
    #[test]
    fn keep_does_not_leak_to_another_repo_or_to_the_next_dialog() {
        let state = crate::state::tests_support::make_test_app_state();
        pending_cleanup(&state, "/repo");
        pending_cleanup(&state, "/other");
        clear_orphan_cleanup_internal(&state, "/repo", true);
        assert_eq!(pending_answer(&state, "/other"), None);

        pending_cleanup(&state, "/repo"); // what begin_orphan_cleanup does for a new dialog

        assert_eq!(pending_answer(&state, "/repo"), None);
    }

    #[cfg(unix)]
    #[test]
    fn removal_preview_names_live_nested_session_and_counts_untracked_work() {
        let repo = setup_test_repo();
        assert!(
            repo.path()
                .canonicalize()
                .unwrap()
                .starts_with(tuic_test_support::test_temp_root().canonicalize().unwrap())
        );
        let worktree = worktree_with(repo.path(), "active-work", false);
        fs::create_dir_all(worktree.join("nested")).expect("nested cwd");
        fs::write(worktree.join("README.md"), "changed").expect("modified file");
        fs::write(worktree.join("new-a.txt"), "a").expect("first untracked file");
        fs::write(worktree.join("new-b.txt"), "b").expect("second untracked file");
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        crate::state::tests_support::insert_dummy_session(&state, "pty-active");
        crate::state::tests_support::set_session_cwd(
            &state,
            "pty-active",
            &worktree.join("nested").to_string_lossy(),
        );
        state
            .session_maps
            .sessions
            .get("pty-active")
            .unwrap()
            .lock()
            .display_name = Some("Codex: gate work".to_string());
        crate::state::tests_support::insert_dummy_session(&state, "pty-other");
        crate::state::tests_support::set_session_cwd(
            &state,
            "pty-other",
            &repo.path().to_string_lossy(),
        );

        let preview = inspect_worktree_removal(&state, repo.path(), "active-work");

        assert_eq!(preview.lifecycle.dirty_files, Some(3));
        assert_eq!(preview.untracked_files, Some(2));
        assert_eq!(
            preview.live_sessions,
            vec![WorktreeLiveSession {
                session_id: "pty-active".to_string(),
                name: "Codex: gate work".to_string(),
            }]
        );
        assert!(
            preview
                .warnings
                .iter()
                .any(|warning| warning.contains("Codex: gate work"))
        );
        assert!(
            preview
                .warnings
                .iter()
                .any(|warning| warning.contains("2 untracked"))
        );
    }

    /// The post-merge cleanup dialog runs these one after another, and each was
    /// a plain `fn` command: a checkout, a branch delete that can take a whole
    /// worktree with it, and an archive that moves a directory. A plain `fn`
    /// runs inline on the IPC thread — the macOS main thread — so pressing
    /// Execute froze the WebView for the length of every step. Their HTTP twins
    /// in `mcp_http/worktree_routes.rs` were already on the blocking pool, which
    /// is exactly the transport drift `docs/backend/command-threading.md` names.
    /// Orphan removal has the same boundary: its safety checks and recursive
    /// deletion must not block heartbeats while the desktop handles cleanup.
    #[test]
    fn post_merge_cleanup_commands_never_run_on_the_ipc_thread() {
        let source = include_str!("worktree.rs");
        for command in [
            "switch_branch",
            "delete_local_branch",
            "finalize_merged_worktree",
            "remove_orphan_worktree",
        ] {
            let signature = format!("pub(crate) async fn {command}(");
            let at = source.find(&signature).unwrap_or_else(|| {
                panic!("{command} must be async: a plain fn command runs on the macOS main thread")
            });
            // Async alone only moves the work to a Tokio worker; the body still
            // runs git subprocesses and recursive deletes, so it needs the
            // blocking pool as well.
            // By chars, not bytes: this file has em dashes in its comments, and
            // a byte window can end inside one. Where it lands depends on the
            // line endings, so on Windows the same slice panicked.
            let body: String = source[at..].chars().take(800).collect();
            assert!(
                body.contains("spawn_blocking"),
                "{command} awaits blocking git work and must hand it to spawn_blocking"
            );
        }
    }

    #[test]
    fn resolve_worktree_dir_sibling() {
        let repo = Path::new("/home/user/dev/myrepo");
        let app_dir = Path::new("/app/worktrees");
        let result = resolve_worktree_dir(repo, &WorktreeStorage::Sibling, app_dir);
        assert_eq!(result, PathBuf::from("/home/user/dev/myrepo__wt"));
    }

    #[test]
    fn resolve_worktree_dir_app_dir() {
        let repo = Path::new("/home/user/dev/myrepo");
        let app_dir = Path::new("/app/worktrees");
        let result = resolve_worktree_dir(repo, &WorktreeStorage::AppDir, app_dir);
        assert_eq!(result, PathBuf::from("/app/worktrees/myrepo"));
    }

    #[test]
    fn resolve_worktree_dir_inside_repo() {
        let repo = Path::new("/home/user/dev/myrepo");
        let app_dir = Path::new("/app/worktrees");
        let result = resolve_worktree_dir(repo, &WorktreeStorage::InsideRepo, app_dir);
        assert_eq!(result, PathBuf::from("/home/user/dev/myrepo/.worktrees"));
    }

    #[test]
    fn resolve_worktree_dir_sibling_with_dots_in_name() {
        let repo = Path::new("/home/user/dev/my.project.name");
        let app_dir = Path::new("/app/worktrees");
        let result = resolve_worktree_dir(repo, &WorktreeStorage::Sibling, app_dir);
        assert_eq!(result, PathBuf::from("/home/user/dev/my.project.name__wt"));
    }

    // --- the destructive-cleanup gate ---
    //
    // Both "archive" and "delete" end in `git worktree remove --force`, which
    // deletes uncommitted work without a word. Every path to that call must pass
    // `cleanup_needs_confirmation` first. These tests exercise the two entry
    // points end to end on a real repo, because the bug they cover was not in
    // the gate — it was in a caller that never reached it.

    /// An isolated config dir, so `resolve_archive_script` reads an empty config
    /// instead of the developer's real one and cannot run a live archive script.
    fn isolated_config() -> (TempDir, impl Drop) {
        let dir = TempDir::new().expect("config dir");
        let guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        (dir, guard)
    }

    // Catches: durable suspended cwd/placement bypassing destructive checkout consumers.
    fn suspended_checkout_consumer(consumer: &str) {
        let (_cfg, _guard) = isolated_config();
        let repo = setup_test_repo();
        let checkout = worktree_with(repo.path(), "suspended-work", true);
        fs::create_dir(checkout.join("nested")).unwrap();
        git_cmd(repo.path())
            .args(["merge", "suspended-work", "--no-edit"])
            .run()
            .unwrap();
        let record = crate::suspended_sessions::SuspendedSession {
            tuic_session: "sleeping-tab".into(),
            session_id: "old-pty".into(),
            cwd: checkout.join("nested").to_string_lossy().into_owned(),
            name: Some("Sleeping agent".into()),
            ..Default::default()
        };
        crate::config::ConfigFile::new("suspended-sessions.json")
            .save(&std::collections::BTreeMap::from([(
                "sleeping-tab".to_string(),
                record,
            )]))
            .unwrap();
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let repo_path = repo.path().to_string_lossy().into_owned();
        match consumer {
            "preview" => {
                let preview = inspect_worktree_removal(&state, repo.path(), "suspended-work");
                assert!(
                    preview
                        .live_sessions
                        .iter()
                        .any(|user| user.name.contains("Sleeping agent"))
                );
            }
            "finish" | "forced-remove" => {
                let result = remove_worktree_by_workspace_id_with_confirmation(
                    &state,
                    &repo_path,
                    "suspended-work",
                    true,
                    None,
                    consumer == "forced-remove",
                    false,
                    None,
                );
                assert!(
                    result
                        .expect_err("suspended user must veto cleanup")
                        .contains("Sleeping agent")
                );
            }
            "allocate" => {
                tuic_git::worktree::finish_worktree(repo.path(), &checkout).unwrap();
                let config = WorktreeConfig {
                    task_name: "replacement".into(),
                    base_repo: repo_path.clone(),
                    branch: Some("replacement".into()),
                    create_branch: true,
                };
                let (_, renamed) =
                    allocate_workspace(&state, checkout.parent().unwrap(), &config, None).unwrap();
                assert!(renamed.is_none(), "suspended checkout was recycled");
            }
            "finalize" => {
                let result = finalize_merged_worktree_impl(
                    &state,
                    repo_path,
                    "suspended-work".into(),
                    "archive".into(),
                    true,
                );
                assert!(
                    result
                        .err()
                        .expect("suspended user must veto cleanup")
                        .contains("Sleeping agent")
                );
            }
            "merge" => {
                let result = merge_and_archive_worktree_impl(
                    &state,
                    repo_path,
                    "suspended-work".into(),
                    "suspended-work".into(),
                    base_branch_of(repo.path()),
                    "archive".into(),
                    true,
                );
                assert!(
                    result
                        .err()
                        .expect("suspended user must veto cleanup")
                        .contains("Sleeping agent")
                );
            }
            "orphan" => {
                let error = orphan_removal_guard(
                    &state,
                    &repo_path,
                    &checkout.to_string_lossy(),
                    false,
                    &["sleeping-tab".into()],
                )
                .unwrap_err();
                assert!(error.contains("Sleeping agent"));
            }
            _ => unreachable!(),
        }
        assert!(
            checkout.exists(),
            "{consumer} removed the suspended checkout"
        );
        assert!(
            crate::suspended_sessions::list()
                .unwrap()
                .contains_key("sleeping-tab")
        );
    }

    // Catches: placement-only suspended ownership missed when the shell cwd is elsewhere,
    // prefix siblings overmatched, or unreadable durable state treated as no users.
    #[test]
    fn suspended_checkout_placement_boundaries_and_unreadable_state() {
        let (cfg, _guard) = isolated_config();
        let repo = setup_test_repo();
        let checkout = worktree_with(repo.path(), "placement", false);
        let sibling = worktree_with(repo.path(), "placement-sibling", false);
        let record = crate::suspended_sessions::SuspendedSession {
            tuic_session: "placed-tab".into(),
            cwd: repo.path().to_string_lossy().into_owned(),
            worktree: Some(WorktreeInfo {
                name: "placement".into(),
                path: checkout.clone(),
                branch: Some("placement".into()),
                base_repo: repo.path().into(),
            }),
            ..Default::default()
        };
        crate::config::ConfigFile::new("suspended-sessions.json")
            .save(&std::collections::BTreeMap::from([(
                "placed-tab".to_string(),
                record,
            )]))
            .unwrap();
        let state = crate::state::tests_support::make_test_app_state();
        assert_eq!(
            live_sessions_in(&state, &checkout)[0].session_id,
            "placed-tab"
        );
        assert!(live_sessions_in(&state, &sibling).is_empty());
        assert!(guard_suspended_workspace(repo.path(), "placement-sibling").is_ok());
        fs::write(cfg.path().join("suspended-sessions.json"), "not json").unwrap();
        assert!(guard_suspended_workspace(repo.path(), "placement").is_err());
        assert!(!live_sessions_in(&state, &checkout).is_empty());
        assert_eq!(
            fs::read_to_string(cfg.path().join("suspended-sessions.json")).unwrap(),
            "not json"
        );
    }

    macro_rules! suspended_checkout_test {
        ($name:ident, $consumer:literal) => {
            #[test]
            fn $name() {
                suspended_checkout_consumer($consumer);
            }
        };
    }
    suspended_checkout_test!(suspended_checkout_preview_names_the_tab, "preview");
    suspended_checkout_test!(suspended_checkout_finish_refuses_the_tab, "finish");
    suspended_checkout_test!(
        suspended_checkout_forced_removal_refuses_the_tab,
        "forced-remove"
    );
    suspended_checkout_test!(suspended_checkout_allocation_skips_the_tab, "allocate");
    suspended_checkout_test!(suspended_checkout_finalize_refuses_the_tab, "finalize");
    suspended_checkout_test!(suspended_checkout_merge_refuses_the_tab, "merge");
    suspended_checkout_test!(
        suspended_checkout_orphan_confirmation_cannot_forget_the_tab,
        "orphan"
    );

    #[test]
    fn merge_and_archive_asks_before_destroying_a_dirty_worktree_with_commits() {
        let (_cfg, _guard) = isolated_config();
        let repo = setup_test_repo();
        let base = base_branch_of(repo.path());
        let wt = dirty_worktree_with(repo.path(), "feat-dirty-ahead", true);
        let state = Arc::new(crate::state::tests_support::make_test_app_state());

        let res = merge_and_archive_worktree_impl(
            &state,
            repo.path().to_string_lossy().to_string(),
            "feat-dirty-ahead".to_string(),
            "feat-dirty-ahead".to_string(),
            base,
            "archive".to_string(),
            false,
        )
        .expect("pre-flight returns a result, not an error");

        assert_eq!(res.action, "needs_confirmation");
        assert!(!res.merged, "nothing ran — not even the merge");
        assert_eq!(res.commits_ahead, 1, "the dialog says what would be merged");
        assert!(res.worktree_dirty);
        assert!(
            wt.join("scratch.txt").exists(),
            "the uncommitted file is still there"
        );
    }

    #[test]
    fn merge_and_archive_proceeds_once_the_user_confirms() {
        let (_cfg, _guard) = isolated_config();
        let repo = setup_test_repo();
        let base = base_branch_of(repo.path());
        let wt = dirty_worktree_with(repo.path(), "feat-confirmed", true);
        let state = Arc::new(crate::state::tests_support::make_test_app_state());

        let res = merge_and_archive_worktree_impl(
            &state,
            repo.path().to_string_lossy().to_string(),
            "feat-confirmed".to_string(),
            "feat-confirmed".to_string(),
            base,
            "archive".to_string(),
            true,
        )
        .expect("archive");

        assert_eq!(res.action, "archived");
        assert!(res.merged);
        assert!(!wt.exists(), "the worktree left its old place");
        // Archiving is the non-destructive choice, so the work must survive the
        // move. Removing the worktree before the rename used to delete it.
        let archived = PathBuf::from(res.archive_path.expect("archive path"));
        assert!(
            archived.join("scratch.txt").exists(),
            "uncommitted work moved to the archive instead of being deleted"
        );
    }

    #[test]
    fn merge_and_archive_does_not_ask_about_a_clean_worktree() {
        let (_cfg, _guard) = isolated_config();
        let repo = setup_test_repo();
        let base = base_branch_of(repo.path());
        worktree_with(repo.path(), "feat-clean", true);
        let state = Arc::new(crate::state::tests_support::make_test_app_state());

        let res = merge_and_archive_worktree_impl(
            &state,
            repo.path().to_string_lossy().to_string(),
            "feat-clean".to_string(),
            "feat-clean".to_string(),
            base,
            "archive".to_string(),
            false,
        )
        .expect("archive");

        assert_eq!(res.action, "archived", "nothing to lose, nothing to ask");
        assert!(!res.worktree_dirty);
    }

    // --- merge-and-archive vs. a hostile `merge.ff` config ---

    /// Merge `branch` into the base repo through merge-and-archive, with
    /// `merge.ff` set repo-locally to `ff` and (optionally) the base branch
    /// advanced past the worktree branch's fork point. Returns the repo, the
    /// worktree branch tip, and the parent count of the base branch's HEAD.
    fn merge_archive_with_ff_config(ff: &str, diverge: bool) -> (TempDir, String, usize) {
        let (_cfg, _guard) = isolated_config();
        let repo = setup_test_repo();
        let base = base_branch_of(repo.path());
        worktree_with(repo.path(), "feat-ffcfg", true);
        if diverge {
            fs::write(repo.path().join("base-only.txt"), "moved on").unwrap();
            git_cmd(repo.path()).args(["add", "."]).run().unwrap();
            git_cmd(repo.path())
                .args(["commit", "-m", "base moves on"])
                .run()
                .unwrap();
        }
        git_cmd(repo.path())
            .args(["config", "merge.ff", ff])
            .run()
            .unwrap();
        let tip = git_cmd(repo.path())
            .args(["rev-parse", "feat-ffcfg"])
            .run()
            .unwrap()
            .stdout
            .trim()
            .to_string();
        let state = Arc::new(crate::state::tests_support::make_test_app_state());

        let res = merge_and_archive_worktree_impl(
            &state,
            repo.path().to_string_lossy().to_string(),
            "feat-ffcfg".to_string(),
            "feat-ffcfg".to_string(),
            base,
            "archive".to_string(),
            false,
        )
        .unwrap_or_else(|e| panic!("merge.ff={ff} diverge={diverge}: {e}"));
        assert!(res.merged, "merge.ff={ff} diverge={diverge}");

        let parents = git_cmd(repo.path())
            .args(["rev-list", "--parents", "-n", "1", "HEAD"])
            .run()
            .unwrap()
            .stdout
            .split_whitespace()
            .count()
            - 1;
        (repo, tip, parents)
    }

    #[test]
    fn merge_and_archive_merges_a_diverged_target_under_merge_ff_only() {
        let (repo, _tip, parents) = merge_archive_with_ff_config("only", true);
        assert_eq!(parents, 2, "a diverged target gets a merge commit");
        assert!(repo.path().join("work.txt").exists());
        assert!(repo.path().join("base-only.txt").exists());
    }

    #[test]
    fn merge_and_archive_still_fast_forwards_under_merge_ff_only() {
        let (repo, tip, parents) = merge_archive_with_ff_config("only", false);
        assert_eq!(parents, 1, "ff-able merge stays a fast-forward");
        let head = git_cmd(repo.path())
            .args(["rev-parse", "HEAD"])
            .run()
            .unwrap()
            .stdout;
        assert_eq!(head.trim(), tip);
    }

    #[test]
    fn merge_and_archive_keeps_a_merge_commit_under_merge_ff_false() {
        let (_repo, _tip, parents) = merge_archive_with_ff_config("false", false);
        assert_eq!(parents, 2, "merge.ff=false users still get a merge commit");
    }

    #[test]
    fn merge_and_archive_fast_forwards_by_default() {
        let (_repo, tip, parents) = merge_archive_with_ff_config("true", false);
        assert_eq!(parents, 1);
        assert!(!tip.is_empty());
    }

    #[test]
    #[cfg(unix)]
    fn one_click_cleanup_keeps_a_clean_worktree_with_a_live_agent() {
        let (_cfg, _guard) = isolated_config();
        for action in ["archive", "delete"] {
            let repo = setup_test_repo();
            let worktree = worktree_with(repo.path(), "active-feature", true);
            let original_head = git_cmd(repo.path())
                .args(["rev-parse", "HEAD"])
                .run()
                .unwrap()
                .stdout;
            let state = Arc::new(crate::state::tests_support::make_test_app_state());
            crate::state::tests_support::insert_dummy_session(&state, "active-agent");
            crate::state::tests_support::set_session_cwd(
                &state,
                "active-agent",
                &worktree.to_string_lossy(),
            );

            let result = merge_and_archive_worktree_impl(
                &state,
                repo.path().to_string_lossy().into_owned(),
                "active-feature".into(),
                "active-feature".into(),
                base_branch_of(repo.path()),
                action.into(),
                false,
            )
            .unwrap();

            assert_eq!(result.action, "needs_confirmation", "{action}");
            assert!(
                !result.merged,
                "{action} must wait for approval before merging"
            );
            assert_eq!(
                git_cmd(repo.path())
                    .args(["rev-parse", "HEAD"])
                    .run()
                    .unwrap()
                    .stdout,
                original_head
            );
            assert!(worktree.exists(), "{action} must leave the checkout intact");

            let fingerprint = inspect_worktree_removal(&state, repo.path(), "active-feature")
                .lifecycle
                .dirty_fingerprint
                .expect("confirmed checkout fingerprint");
            let confirmed = merge_and_archive_worktree_impl_with_confirmation(
                &state,
                repo.path().to_string_lossy().into_owned(),
                "active-feature".into(),
                "active-feature".into(),
                base_branch_of(repo.path()),
                action.into(),
                true,
                Some(&fingerprint),
            )
            .unwrap();
            assert_eq!(
                confirmed.action,
                if action == "archive" {
                    "archived"
                } else {
                    "deleted"
                }
            );
            assert!(!worktree.exists(), "{action} proceeds after confirmation");
        }
    }

    #[test]
    fn merge_and_archive_in_ask_mode_never_blocks() {
        let (_cfg, _guard) = isolated_config();
        let repo = setup_test_repo();
        let base = base_branch_of(repo.path());
        let wt = dirty_worktree_with(repo.path(), "feat-ask", true);
        let state = Arc::new(crate::state::tests_support::make_test_app_state());

        let res = merge_and_archive_worktree_impl(
            &state,
            repo.path().to_string_lossy().to_string(),
            "feat-ask".to_string(),
            "feat-ask".to_string(),
            base,
            "ask".to_string(),
            false,
        )
        .expect("merge");

        // "ask" destroys nothing — it merges and hands the cleanup decision to the
        // dialog, which asks for itself. Blocking here would deadlock the flow.
        assert_eq!(res.action, "pending");
        assert!(res.merged);
        assert!(res.worktree_dirty, "the dialog needs to know");
        assert!(wt.join("scratch.txt").exists());
    }

    #[test]
    fn finalize_refuses_to_destroy_a_dirty_worktree() {
        let (_cfg, _guard) = isolated_config();
        let repo = setup_test_repo();
        let wt = dirty_worktree_with(repo.path(), "feat-finalize-dirty", true);
        let state = Arc::new(crate::state::tests_support::make_test_app_state());

        // This is the path the auto-archive sweep takes, with nobody watching.
        let res = finalize_merged_worktree_impl(
            &state,
            repo.path().to_string_lossy().to_string(),
            "feat-finalize-dirty".to_string(),
            "delete".to_string(),
            false,
        )
        .expect("guard returns a result");

        assert_eq!(res.action, "needs_confirmation");
        assert!(res.worktree_dirty);
        assert!(wt.join("scratch.txt").exists(), "still on disk, untouched");
    }

    #[test]
    fn finalize_deletes_once_the_user_confirms() {
        let (_cfg, _guard) = isolated_config();
        let repo = setup_test_repo();
        let base = base_branch_of(repo.path());
        let wt = dirty_worktree_with(repo.path(), "feat-finalize-forced", true);
        let state = Arc::new(crate::state::tests_support::make_test_app_state());

        // Finalize always follows the "ask" merge, as in the dialog flow. Without
        // it the branch keeps unmerged commits, and removal rightly refuses to
        // strand them before the confirmation is ever consulted.
        let pending = merge_and_archive_worktree_impl(
            &state,
            repo.path().to_string_lossy().to_string(),
            "feat-finalize-forced".to_string(),
            "feat-finalize-forced".to_string(),
            base,
            "ask".to_string(),
            false,
        )
        .expect("merge");
        assert_eq!(pending.action, "pending");

        let res = finalize_merged_worktree_impl(
            &state,
            repo.path().to_string_lossy().to_string(),
            "feat-finalize-forced".to_string(),
            "delete".to_string(),
            true,
        )
        .expect("delete");

        assert_eq!(res.action, "deleted");
        assert!(!wt.exists(), "the user asked for it");
    }

    #[test]
    fn forced_finalize_reports_that_an_unmerged_branch_was_kept() {
        let (_cfg, _guard) = isolated_config();
        let repo = setup_test_repo();
        let worktree = worktree_with(repo.path(), "feat-finalize-unmerged", true);
        let state = Arc::new(crate::state::tests_support::make_test_app_state());

        let result = finalize_merged_worktree_impl(
            &state,
            repo.path().to_string_lossy().into_owned(),
            "feat-finalize-unmerged".into(),
            "delete".into(),
            true,
        )
        .unwrap();

        assert_eq!(result.action, "deleted");
        assert!(!worktree.exists());
        let payload = serde_json::to_value(result).unwrap();
        assert!(
            payload["branch_delete_warning"]
                .as_str()
                .is_some_and(|w| w.contains("unmerged")),
            "{payload}"
        );
        assert!(
            git_cmd(repo.path())
                .args(["show-ref", "--verify", "refs/heads/feat-finalize-unmerged"])
                .run()
                .is_ok()
        );
    }

    #[test]
    fn finalize_leaves_a_clean_worktree_to_the_sweep() {
        let (_cfg, _guard) = isolated_config();
        let repo = setup_test_repo();
        worktree_with(repo.path(), "feat-finalize-clean", true);
        git_cmd(repo.path())
            .args(["merge", "feat-finalize-clean", "--no-edit"])
            .run()
            .expect("merge before finalizing cleanup");
        let state = Arc::new(crate::state::tests_support::make_test_app_state());

        let res = finalize_merged_worktree_impl(
            &state,
            repo.path().to_string_lossy().to_string(),
            "feat-finalize-clean".to_string(),
            "archive".to_string(),
            false,
        )
        .expect("archive");

        assert_eq!(res.action, "archived", "no confirmation needed");
    }

    #[test]
    fn automatic_archive_keeps_a_clean_untouched_worktree() {
        let (_cfg, _guard) = isolated_config();
        let repo = setup_test_repo();
        let worktree = worktree_with(repo.path(), "untouched-archive", false);
        let state = Arc::new(crate::state::tests_support::make_test_app_state());

        let result = finalize_merged_worktree_impl(
            &state,
            repo.path().to_string_lossy().into_owned(),
            "untouched-archive".into(),
            "archive".into(),
            false,
        )
        .expect("unsafe automatic cleanup returns a review result");

        assert_eq!(result.action, "needs_confirmation");
        assert!(worktree.exists());
    }

    #[test]
    #[cfg(unix)]
    fn automatic_archive_keeps_a_merged_worktree_with_a_live_session() {
        let (_cfg, _guard) = isolated_config();
        let repo = setup_test_repo();
        let worktree = worktree_with(repo.path(), "active-archive", true);
        git_cmd(repo.path())
            .args(["merge", "active-archive", "--no-edit"])
            .run()
            .expect("merge before finalizing cleanup");
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        crate::state::tests_support::insert_dummy_session(&state, "active-agent");
        crate::state::tests_support::set_session_cwd(
            &state,
            "active-agent",
            &worktree.to_string_lossy(),
        );

        let result = finalize_merged_worktree_impl(
            &state,
            repo.path().to_string_lossy().into_owned(),
            "active-archive".into(),
            "archive".into(),
            false,
        )
        .expect("unsafe automatic cleanup returns a review result");

        assert_eq!(result.action, "needs_confirmation");
        assert!(worktree.exists());
    }

    #[test]
    fn automatic_archive_leaves_a_locked_worktree_untouched() {
        let (_cfg, _guard) = isolated_config();
        let repo = setup_test_repo();
        let worktree = worktree_with(repo.path(), "feat-archive-locked", true);
        git_cmd(repo.path())
            .args(["merge", "feat-archive-locked", "--no-edit"])
            .run()
            .expect("merge before finalizing cleanup");
        git_cmd(repo.path())
            .args(["worktree", "lock", &worktree.to_string_lossy()])
            .run()
            .unwrap();
        let state = Arc::new(crate::state::tests_support::make_test_app_state());

        let error = finalize_merged_worktree_impl(
            &state,
            repo.path().to_string_lossy().into_owned(),
            "feat-archive-locked".into(),
            "archive".into(),
            false,
        )
        .err()
        .expect("locked checkout must not be archived");

        assert!(error.starts_with(tuic_git::worktree::LOCKED_WORKTREE_PREFIX));
        assert!(worktree.exists());
        let listed = git_cmd(repo.path())
            .args(["worktree", "list", "--porcelain"])
            .run()
            .unwrap()
            .stdout;
        #[cfg(unix)]
        let expected = format!(
            "{}/worktrees/feat-archive-locked",
            repo.path().canonicalize().unwrap().display()
        );
        #[cfg(windows)]
        let expected = format!(
            "{}/worktrees/feat-archive-locked",
            repo.path().display().to_string().replace('\\', "/")
        );
        assert!(listed.contains(&expected));
        assert!(
            listed
                .lines()
                .filter_map(|line| line.strip_prefix("worktree "))
                .any(|path| {
                    Path::new(path).canonicalize().unwrap() == worktree.canonicalize().unwrap()
                })
        );
    }

    #[test]
    fn resolve_worktree_dir_sibling_strategy() {
        use crate::config::WorktreeStorage;
        let repo = PathBuf::from("/home/user/dev/myrepo");
        let app_dir = PathBuf::from("/home/user/.config/tuic/worktrees");
        assert_eq!(
            resolve_worktree_dir(&repo, &WorktreeStorage::Sibling, &app_dir),
            PathBuf::from("/home/user/dev/myrepo__wt")
        );
    }

    #[test]
    fn resolve_worktree_dir_appdir_strategy() {
        use crate::config::WorktreeStorage;
        let repo = PathBuf::from("/home/user/dev/myrepo");
        let app_dir = PathBuf::from("/home/user/.config/tuic/worktrees");
        assert_eq!(
            resolve_worktree_dir(&repo, &WorktreeStorage::AppDir, &app_dir),
            PathBuf::from("/home/user/.config/tuic/worktrees/myrepo")
        );
    }

    #[test]
    fn resolve_worktree_dir_inside_repo_strategy() {
        use crate::config::WorktreeStorage;
        let repo = PathBuf::from("/home/user/dev/myrepo");
        let app_dir = PathBuf::from("/home/user/.config/tuic/worktrees");
        assert_eq!(
            resolve_worktree_dir(&repo, &WorktreeStorage::InsideRepo, &app_dir),
            PathBuf::from("/home/user/dev/myrepo/.worktrees")
        );
    }

    #[test]
    fn resolve_worktree_dir_claude_code_default_strategy() {
        use crate::config::WorktreeStorage;
        let repo = PathBuf::from("/home/user/dev/myrepo");
        let app_dir = PathBuf::from("/home/user/.config/tuic/worktrees");
        assert_eq!(
            resolve_worktree_dir(&repo, &WorktreeStorage::ClaudeCodeDefault, &app_dir),
            PathBuf::from("/home/user/dev/myrepo/.claude/worktrees")
        );
    }

    #[test]
    fn merge_cleanup_entry_points_refuse_stale_force_confirmation() {
        let (_cfg, _guard) = isolated_config();
        let repo = setup_test_repo();
        let base = base_branch_of(repo.path());
        let worktree = dirty_worktree_with(repo.path(), "stale-merge-cleanup", false);
        let confirmed = dirty_fingerprint_at(&worktree).unwrap().0;
        fs::write(worktree.join("after-confirmation.txt"), "new work\n").unwrap();
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let repo_path = repo.path().to_string_lossy().to_string();

        let finalize_error = finalize_merged_worktree_impl_with_confirmation(
            &state,
            repo_path.clone(),
            "stale-merge-cleanup".into(),
            "delete".into(),
            true,
            Some(&confirmed),
        )
        .err()
        .expect("stale confirmation must be rejected");
        assert!(
            finalize_error.contains("changed since confirmation"),
            "{finalize_error}"
        );

        let merge_error = merge_and_archive_worktree_impl_with_confirmation(
            &state,
            repo_path,
            "stale-merge-cleanup".into(),
            "stale-merge-cleanup".into(),
            base,
            "archive".into(),
            true,
            Some(&confirmed),
        )
        .err()
        .expect("stale confirmation must be rejected");
        assert!(
            merge_error.contains("changed since confirmation"),
            "{merge_error}"
        );
        assert!(worktree.join("after-confirmation.txt").exists());
    }
    #[cfg(unix)]
    mod orphan_removal_guard_critic {
        use super::*;
        use crate::state::tests_support::{insert_dummy_session, set_session_cwd};

        fn detached(repo: &Path, name: &str) -> PathBuf {
            let path = repo.join(name);
            let out = std::process::Command::new("git")
                .current_dir(repo)
                .args(["worktree", "add", "--detach"])
                .arg(&path)
                .arg("HEAD")
                .output()
                .expect("run git");
            assert!(out.status.success(), "{out:?}");
            path
        }

        fn guard(
            state: &AppState,
            repo: &Path,
            checkout: &Path,
            confirmed: &[&str],
        ) -> Result<(), String> {
            let confirmed: Vec<String> = confirmed.iter().map(|s| s.to_string()).collect();
            orphan_removal_guard(
                state,
                &repo.to_string_lossy(),
                &checkout.to_string_lossy(),
                false,
                &confirmed,
            )
        }

        // Catches: a session in a subdirectory of the checkout escaping the unreviewed-session
        // check on the confirmed path (exact cwd match instead of prefix).
        #[test]
        fn refuses_an_unreviewed_session_in_a_subdirectory() {
            let repo = setup_test_repo();
            let linked = detached(repo.path(), "linked");
            fs::create_dir_all(linked.join("sub/deeper")).unwrap();
            let state = crate::state::tests_support::make_test_app_state();
            insert_dummy_session(&state, "agent");
            set_session_cwd(
                &state,
                "agent",
                &linked.join("sub/deeper").to_string_lossy(),
            );

            assert!(guard(&state, repo.path(), &linked, &[]).is_err());
            guard(&state, repo.path(), &linked, &["agent"]).expect("reviewed");
        }

        // Catches: a session in a sibling checkout whose name shares a string prefix
        // ("linked" vs "linked-2") being counted as inside this checkout.
        #[test]
        fn a_sibling_with_a_shared_name_prefix_is_not_unreviewed() {
            let repo = setup_test_repo();
            let linked = detached(repo.path(), "linked");
            let sibling = detached(repo.path(), "linked-2");
            let state = crate::state::tests_support::make_test_app_state();
            insert_dummy_session(&state, "agent");
            set_session_cwd(&state, "agent", &sibling.to_string_lossy());

            guard(&state, repo.path(), &linked, &[]).expect("sibling is outside");
        }

        // Catches: the refusal text hiding which session was not reviewed, so the caller cannot
        // act on a 400.
        #[test]
        fn the_refusal_names_the_unreviewed_session_and_not_the_reviewed_one() {
            let repo = setup_test_repo();
            let linked = detached(repo.path(), "linked");
            let state = crate::state::tests_support::make_test_app_state();
            for (id, name) in [("seen", "Seen Agent"), ("late", "Late Agent")] {
                insert_dummy_session(&state, id);
                set_session_cwd(&state, id, &linked.to_string_lossy());
                state
                    .session_maps
                    .sessions
                    .get(id)
                    .unwrap()
                    .lock()
                    .display_name = Some(name.to_string());
            }

            let error = guard(&state, repo.path(), &linked, &["seen"]).unwrap_err();

            assert!(error.contains("Late Agent"), "{error}");
            assert!(!error.contains("Seen Agent"), "{error}");
        }

        // Catches: an already-removed checkout with stale sessions still refusing the
        // confirmed removal (idempotent retry must succeed).
        #[test]
        fn a_missing_checkout_never_blocks() {
            let repo = setup_test_repo();
            let gone = repo.path().join("gone");
            let state = crate::state::tests_support::make_test_app_state();
            insert_dummy_session(&state, "agent");
            set_session_cwd(&state, "agent", &gone.to_string_lossy());

            guard(&state, repo.path(), &gone, &[]).expect("nothing left to protect");
        }
    }
    #[cfg(unix)]
    mod orphan_session_guard {
        use super::*;
        use crate::state::tests_support::{insert_dummy_session, set_session_cwd};

        fn detached(repo: &Path, name: &str) -> PathBuf {
            let path = repo.join(name);
            let out = std::process::Command::new("git")
                .current_dir(repo)
                .args(["worktree", "add", "--detach"])
                .arg(&path)
                .arg("HEAD")
                .output()
                .expect("run git");
            assert!(out.status.success(), "{out:?}");
            path
        }

        fn guard(state: &AppState, repo: &Path, checkout: &Path) -> Result<(), String> {
            orphan_cleanup_safety_with_sessions(
                state,
                &repo.to_string_lossy(),
                &checkout.to_string_lossy(),
            )
        }

        // Catches: automatic cleanup deleting a checkout rescued by a branch after discovery.
        #[test]
        fn automatic_orphan_removal_rechecks_a_branch_restored_after_discovery() {
            let repo = setup_test_repo();
            let linked = detached(repo.path(), "rescued-checkout");
            let state = crate::state::tests_support::make_test_app_state();
            let before =
                assess_orphan_cleanup_with_sessions(&state, &repo.path().to_string_lossy())
                    .unwrap();
            assert!(
                before.iter().any(|row| {
                    Path::new(&row.assessment.path) == linked && row.assessment.safe
                })
            );
            let rescue = std::process::Command::new("git")
                .current_dir(&linked)
                .args(["branch", "rescued-active", "HEAD"])
                .output()
                .unwrap();
            assert!(rescue.status.success(), "{rescue:?}");
            assert!(
                assess_orphan_cleanup_with_sessions(&state, &repo.path().to_string_lossy(),)
                    .unwrap()
                    .is_empty()
            );

            let result = remove_orphan_checkout(
                &state,
                &repo.path().to_string_lossy(),
                &linked.to_string_lossy(),
                true,
                &[],
            );
            assert!(
                result.is_err(),
                "rescued active checkout was removed: {result:?}"
            );
            assert!(linked.exists(), "rescued active checkout must survive");
        }

        // Catches: an equality or exact-match cwd test, so an agent whose shell sits
        // in a subdirectory of the checkout is not seen and its checkout is removed.
        #[test]
        fn refuses_a_session_working_in_a_subdirectory_of_the_checkout() {
            let repo = setup_test_repo();
            let linked = detached(repo.path(), "linked");
            fs::create_dir(linked.join("src")).unwrap();
            let state = crate::state::tests_support::make_test_app_state();
            insert_dummy_session(&state, "agent");
            set_session_cwd(&state, "agent", &linked.join("src").to_string_lossy());

            let error = guard(&state, repo.path(), &linked).expect_err("live session");
            assert!(error.contains("live session"), "{error}");
        }

        // Catches: a string-prefix cwd test, so a session in `wt-2` blocks the
        // cleanup of `wt` for ever (or, inverted, `wt` hides a session of `wt-2`).
        #[test]
        fn ignores_a_sibling_directory_whose_name_extends_the_checkout_name() {
            let repo = setup_test_repo();
            let linked = detached(repo.path(), "wt");
            let sibling = repo.path().join("wt-2");
            fs::create_dir(&sibling).unwrap();
            let state = crate::state::tests_support::make_test_app_state();
            insert_dummy_session(&state, "agent");
            set_session_cwd(&state, "agent", &sibling.to_string_lossy());

            guard(&state, repo.path(), &linked)
                .expect("sibling session is not inside the checkout");
        }

        // Catches: comparing raw cwd text, so a session that entered the checkout
        // through a symlink is invisible to the guard.
        #[test]
        fn refuses_a_session_whose_cwd_reaches_the_checkout_through_a_symlink() {
            let repo = setup_test_repo();
            let linked = detached(repo.path(), "linked");
            let alias = repo.path().join("alias");
            std::os::unix::fs::symlink(&linked, &alias).unwrap();
            let state = crate::state::tests_support::make_test_app_state();
            insert_dummy_session(&state, "agent");
            set_session_cwd(&state, "agent", &alias.to_string_lossy());

            assert!(guard(&state, repo.path(), &linked).is_err());
        }

        // Catches: reading only `cwd`, so a session created for a worktree but with
        // no OSC 7 cwd report yet does not protect that worktree.
        #[test]
        fn falls_back_to_the_session_worktree_when_the_cwd_is_unknown() {
            let repo = setup_test_repo();
            let linked = detached(repo.path(), "linked");
            let state = crate::state::tests_support::make_test_app_state();
            insert_dummy_session(&state, "agent");
            state
                .session_maps
                .sessions
                .get("agent")
                .unwrap()
                .lock()
                .worktree = Some(WorktreeInfo {
                name: "linked".into(),
                path: linked.clone(),
                branch: None,
                base_repo: repo.path().to_path_buf(),
            });

            assert!(guard(&state, repo.path(), &linked).is_err());
        }

        // Catches: treating a session with no location as "inside everything", which
        // would block every orphan cleanup while any such session exists.
        #[test]
        fn a_session_with_no_cwd_and_no_worktree_blocks_nothing() {
            let repo = setup_test_repo();
            let linked = detached(repo.path(), "linked");
            let state = crate::state::tests_support::make_test_app_state();
            insert_dummy_session(&state, "agent");

            guard(&state, repo.path(), &linked).expect("no location, no claim on the checkout");
        }

        // Catches: a dead session left in the registry blocking the cleanup for
        // ever — the exit path must drop it from what the guard reads.
        #[test]
        fn a_session_that_exited_no_longer_blocks_the_cleanup() {
            let repo = setup_test_repo();
            let linked = detached(repo.path(), "linked");
            let state = Arc::new(crate::state::tests_support::make_test_app_state());
            insert_dummy_session(&state, "agent");
            set_session_cwd(&state, "agent", &linked.to_string_lossy());
            assert!(guard(&state, repo.path(), &linked).is_err());

            crate::pty::mark_session_exited("agent", &state);

            guard(&state, repo.path(), &linked).expect("exited session is gone");
        }

        // Catches: one live session flipping the verdict of every orphan in the
        // repo (a shared flag instead of a per-checkout check).
        #[test]
        fn assessment_marks_only_the_orphan_a_session_works_in() {
            let repo = setup_test_repo();
            let idle = detached(repo.path(), "idle");
            let busy = detached(repo.path(), "busy");
            let state = crate::state::tests_support::make_test_app_state();
            insert_dummy_session(&state, "agent");
            set_session_cwd(&state, "agent", &busy.to_string_lossy());

            let rows = assess_orphan_cleanup_with_sessions(&state, &repo.path().to_string_lossy())
                .unwrap();

            let row_of = |dir: &Path| {
                rows.iter()
                    .find(|row| Path::new(&row.assessment.path).ends_with(dir.file_name().unwrap()))
                    .expect("orphan listed")
            };
            assert!(row_of(&idle).assessment.safe);
            assert!(row_of(&idle).live_sessions.is_empty());
            assert!(!row_of(&busy).assessment.safe);
            assert_eq!(row_of(&busy).live_sessions.len(), 1);
        }

        // Catches: the live-session reason overwriting the git reason, so a dirty
        // checkout with a session in it hides the uncommitted work from the dialog.
        #[test]
        fn assessment_keeps_the_git_reason_next_to_the_live_session() {
            let repo = setup_test_repo();
            let linked = detached(repo.path(), "linked");
            fs::write(linked.join("scratch.txt"), "unsaved").unwrap();
            let git_reason =
                tuic_git::worktree::assess_orphan_worktrees(&repo.path().to_string_lossy())
                    .unwrap()
                    .remove(0)
                    .reason
                    .expect("untracked file is unsafe");
            let state = crate::state::tests_support::make_test_app_state();
            insert_dummy_session(&state, "agent");
            set_session_cwd(&state, "agent", &linked.to_string_lossy());

            let rows = assess_orphan_cleanup_with_sessions(&state, &repo.path().to_string_lossy())
                .unwrap();

            let reason = rows[0].assessment.reason.clone().unwrap();
            assert!(reason.contains(&git_reason), "{reason}");
            assert!(reason.contains("live session"), "{reason}");
        }
    }

    mod orphan_keep_settled_critic {
        use super::*;

        fn detached(repo: &Path, name: &str) -> PathBuf {
            let path = repo.join(name);
            let out = std::process::Command::new("git")
                .current_dir(repo)
                .args(["worktree", "add", "--detach"])
                .arg(&path)
                .arg("HEAD")
                .output()
                .expect("run git");
            assert!(out.status.success(), "{out:?}");
            path
        }

        // Catches (critic-1367): the settled filter applied only to a "remove" answer, so a
        // "keep" answer after a Keep still reports "changed while it was being answered".
        #[test]
        fn a_keep_answer_after_a_keep_reports_nothing_pending() {
            let state = crate::state::tests_support::make_test_app_state();
            pending_cleanup(&state, "/repo");
            clear_orphan_cleanup_internal(&state, "/repo", true);

            let error = answer_orphan_cleanup_internal(&state, "/repo", false).unwrap_err();

            assert_eq!(error, "No pending orphan cleanup for this repository");
        }

        // Catches (critic-1367): a new dialog inheriting the settled flag (or the old
        // Keep) from the entry it replaces, so after one Keep every later dialog is
        // unanswerable by agents and closes at once on the stale Keep.
        #[test]
        fn a_dialog_begun_after_a_keep_is_pending_and_answerable_again() {
            let repo = setup_test_repo();
            let linked = detached(repo.path(), "linked");
            let repo_path = repo.path().to_string_lossy().to_string();
            let paths = vec![linked.to_string_lossy().to_string()];
            let state = crate::state::tests_support::make_test_app_state();
            begin_orphan_cleanup_internal(&state, &repo_path, paths.clone()).unwrap();
            clear_orphan_cleanup_internal(&state, &repo_path, true);

            begin_orphan_cleanup_internal(&state, &repo_path, paths).unwrap();

            assert_eq!(pending_answer(&state, &repo_path), None);
            answer_orphan_cleanup_internal(&state, &repo_path, false)
                .expect("a fresh dialog is answerable");
            assert_eq!(pending_answer(&state, &repo_path), Some(false));
        }

        // Catches (critic-1367): a confirmed removal that settles the entry like a Keep, or
        // a Keep on a repo with no dialog creating a phantom entry that a later "remove"
        // answer would then trip over.
        #[test]
        fn clearing_without_a_dialog_leaves_nothing_to_answer() {
            let state = crate::state::tests_support::make_test_app_state();

            clear_orphan_cleanup_internal(&state, "/repo", true);
            clear_orphan_cleanup_internal(&state, "/repo", false);

            let error = answer_orphan_cleanup_internal(&state, "/repo", true).unwrap_err();
            assert_eq!(error, "No pending orphan cleanup for this repository");
            assert_eq!(pending_answer(&state, "/repo"), None);
        }
    }
}

#[cfg(test)]
#[path = "worktree_pr_proof_tests.rs"]
mod pr_proof_tests;
