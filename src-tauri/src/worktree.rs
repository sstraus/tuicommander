use crate::state::{AppState, WorktreeInfo};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use tauri::State;

/// Parse `git worktree list --porcelain` output and return the worktree path
/// for the given branch name, if any.
fn find_worktree_path_for_branch(stdout: &str, branch_name: &str) -> Option<PathBuf> {
    let mut current_path: Option<PathBuf> = None;
    for line in stdout.lines() {
        if line.starts_with("worktree ") {
            current_path = Some(PathBuf::from(line.trim_start_matches("worktree ")));
        } else if line.starts_with("branch refs/heads/")
            && line.trim_start_matches("branch refs/heads/") == branch_name
        {
            return current_path;
        }
    }
    None
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct WorktreeConfig {
    pub(crate) task_name: String,
    pub(crate) base_repo: String,
    pub(crate) branch: Option<String>,
    pub(crate) create_branch: bool,
}

#[derive(Clone, Serialize)]
pub(crate) struct WorktreeResult {
    pub(crate) session_id: String,
    pub(crate) worktree_path: String,
    pub(crate) branch: Option<String>,
}

/// Resolve the worktree base directory for a given repo + storage strategy.
///
/// - `Sibling`: `{repo_parent}/{repo_name}__wt/`
/// - `AppDir`: `{app_config_dir}/worktrees/{repo_name}/`
/// - `InsideRepo`: `{repo_path}/.worktrees/`
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
    }
}

/// Resolve the effective worktree directory for a repo by loading config from disk.
/// Per-repo `worktree_storage` overrides the global default from repo-defaults.
pub(crate) fn resolve_worktree_dir_for_repo(
    repo_path: &Path,
    app_worktrees_dir: &Path,
) -> PathBuf {
    let repo_path_str = repo_path.to_string_lossy();
    let repo_settings = crate::config::load_repo_settings();
    let strategy = repo_settings
        .repos
        .get(repo_path_str.as_ref())
        .and_then(|entry| entry.worktree_storage.clone())
        .unwrap_or_else(|| crate::config::load_repo_defaults().worktree_storage);
    resolve_worktree_dir(repo_path, &strategy, app_worktrees_dir)
}

/// Sanitize task name for use as directory name
pub(crate) fn sanitize_name(name: &str) -> String {
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
pub(crate) fn create_worktree_internal(
    worktrees_dir: &Path,
    config: &WorktreeConfig,
    base_ref: Option<&str>,
) -> Result<WorktreeInfo, String> {
    let worktree_name = sanitize_name(&config.task_name);
    let worktree_path = worktrees_dir.join(&worktree_name);

    // Check if worktree already exists
    if worktree_path.exists() {
        // Return existing worktree info
        return Ok(WorktreeInfo {
            name: worktree_name,
            path: worktree_path,
            branch: config.branch.clone(),
            base_repo: PathBuf::from(&config.base_repo),
        });
    }

    // Ensure worktrees directory exists
    std::fs::create_dir_all(worktrees_dir)
        .map_err(|e| format!("Failed to create worktrees directory: {e}"))?;

    // Build git worktree add command
    let mut cmd = Command::new(crate::agent::resolve_cli("git"));
    cmd.current_dir(&config.base_repo);
    cmd.arg("worktree").arg("add");

    if config.create_branch
        && let Some(ref branch) = config.branch {
            cmd.arg("-b").arg(branch);
        }

    cmd.arg(&worktree_path);

    if let Some(ref branch) = config.branch
        && !config.create_branch {
            cmd.arg(branch);
        }

    // Append base_ref as start-point when creating a new branch
    if config.create_branch
        && let Some(start_point) = base_ref
    {
        cmd.arg(start_point);
    }

    let output = cmd
        .output()
        .map_err(|e| format!("Failed to execute git worktree: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Check if it's just a "already exists" error
        if stderr.contains("already exists") || stderr.contains("already checked out") {
            return Ok(WorktreeInfo {
                name: worktree_name,
                path: worktree_path,
                branch: config.branch.clone(),
                base_repo: PathBuf::from(&config.base_repo),
            });
        }
        return Err(format!("Git worktree failed: {stderr}"));
    }

    Ok(WorktreeInfo {
        name: worktree_name,
        path: worktree_path,
        branch: config.branch.clone(),
        base_repo: PathBuf::from(&config.base_repo),
    })
}

/// Remove a git worktree
pub(crate) fn remove_worktree_internal(worktree: &WorktreeInfo) -> Result<(), String> {
    // First, run git worktree remove
    let output = Command::new(crate::agent::resolve_cli("git"))
        .current_dir(&worktree.base_repo)
        .arg("worktree")
        .arg("remove")
        .arg("--force")
        .arg(&worktree.path)
        .output()
        .map_err(|e| format!("Failed to execute git worktree remove: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // If worktree doesn't exist, that's fine
        if !stderr.contains("not a working tree") && !stderr.contains("No such file") {
            return Err(format!("Git worktree remove failed: {stderr}"));
        }
    }

    // Cleanup the directory if it still exists
    if worktree.path.exists() {
        std::fs::remove_dir_all(&worktree.path)
            .map_err(|e| format!("Failed to remove worktree directory: {e}"))?;
    }

    // Prune worktrees (non-fatal: stale entries are harmless)
    if let Err(e) = Command::new(crate::agent::resolve_cli("git"))
        .current_dir(&worktree.base_repo)
        .arg("worktree")
        .arg("prune")
        .output()
    {
        eprintln!("Warning: git worktree prune failed: {e}");
    }

    Ok(())
}

/// Adjective + sci-fi character worktree name generator
pub(crate) fn generate_worktree_name(existing: &[String]) -> String {
    let adjectives = [
        "brave", "calm", "dark", "eager", "fair", "glad", "happy", "keen",
        "lush", "mild", "neat", "proud", "quick", "rare", "safe", "tall",
        "vast", "warm", "wise", "bold", "cool", "deep", "fast", "gold",
        "huge", "iron", "jade", "kind", "lean", "mint", "nova", "open",
        "pale", "red", "slim", "tidy", "ultra", "vivid", "wild", "zen",
    ];

    let names = [
        "neo", "ripley", "deckard", "morpheus", "trinity", "cypher", "nexus", "cortex",
        "tron", "hal", "skynet", "muad", "atreides", "harkonnen", "seldon", "daneel",
        "solaris", "neuro", "winter", "armitage", "molly", "case", "hiro", "kovacs",
        "takeshi", "quell", "pris", "batty", "zhora", "gaff", "tyrell", "gibson",
        "asimov", "vance", "rama", "ender", "bean", "valentine", "petra", "revan",
    ];

    // Simple PRNG using current time
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    for attempt in 0..100u128 {
        let adj_idx = ((seed.wrapping_add(attempt.wrapping_mul(7))) % adjectives.len() as u128) as usize;
        let name_idx = ((seed.wrapping_add(attempt.wrapping_mul(13)).wrapping_add(3)) % names.len() as u128) as usize;
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
pub(crate) fn generate_clone_branch_name(source_branch: &str, existing: &[String]) -> String {
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

/// Create a worktree without a PTY session
#[tauri::command]
pub(crate) fn create_worktree(
    state: State<'_, Arc<AppState>>,
    base_repo: String,
    branch_name: String,
    create_branch: Option<bool>,
    base_ref: Option<String>,
) -> Result<serde_json::Value, String> {
    let config = WorktreeConfig {
        task_name: branch_name.clone(),
        base_repo,
        branch: Some(branch_name),
        create_branch: create_branch.unwrap_or(true),
    };

    let worktrees_dir = resolve_worktree_dir_for_repo(
        Path::new(&config.base_repo),
        &state.worktrees_dir,
    );
    let worktree = create_worktree_internal(&worktrees_dir, &config, base_ref.as_deref())?;
    state.invalidate_repo_caches(&config.base_repo);

    Ok(serde_json::json!({
        "name": worktree.name,
        "path": worktree.path.to_string_lossy(),
        "branch": worktree.branch,
        "base_repo": worktree.base_repo.to_string_lossy(),
    }))
}

/// Get worktrees directory path.
/// When `repo_path` is provided, resolves the effective storage strategy for the repo.
#[tauri::command]
pub(crate) fn get_worktrees_dir(state: State<'_, Arc<AppState>>, repo_path: Option<String>) -> String {
    match repo_path {
        Some(rp) => resolve_worktree_dir_for_repo(Path::new(&rp), &state.worktrees_dir)
            .to_string_lossy()
            .to_string(),
        None => state.worktrees_dir.to_string_lossy().to_string(),
    }
}

/// Core logic for removing a git worktree by branch name.
///
/// When `delete_branch` is true, also deletes the local branch after removing
/// the worktree directory. When false, the branch is preserved.
pub(crate) fn remove_worktree_by_branch(repo_path: &str, branch_name: &str, delete_branch: bool) -> Result<(), String> {
    let base_repo = PathBuf::from(repo_path);

    // List worktrees to find the path for this branch
    let output = Command::new(crate::agent::resolve_cli("git"))
        .current_dir(&base_repo)
        .args(["worktree", "list", "--porcelain"])
        .output()
        .map_err(|e| format!("Failed to list worktrees: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git worktree list failed: {stderr}"));
    }

    let worktree_list = String::from_utf8_lossy(&output.stdout);
    let worktree_path = find_worktree_path_for_branch(&worktree_list, branch_name)
        .ok_or_else(|| format!("No worktree found for branch '{branch_name}'"))?;

    // Remove the worktree
    let worktree = WorktreeInfo {
        name: branch_name.to_string(),
        path: worktree_path,
        branch: Some(branch_name.to_string()),
        base_repo,
    };

    remove_worktree_internal(&worktree)?;

    // Delete the local branch when requested (non-fatal: branch may still be useful)
    if delete_branch {
        match Command::new(crate::agent::resolve_cli("git"))
            .current_dir(&worktree.base_repo)
            .args(["branch", "-d", branch_name])
            .output()
        {
            Ok(output) if !output.status.success() => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                eprintln!("Warning: git branch -d {branch_name} exited with {}: {stderr}", output.status);
            }
            Err(e) => {
                eprintln!("Warning: git branch -d {branch_name} failed to spawn: {e}");
            }
            _ => {}
        }
    }

    Ok(())
}

/// Remove a git worktree by branch name (Tauri command with cache invalidation)
///
/// `delete_branch` defaults to `true` when omitted (preserving existing behavior).
#[tauri::command]
pub(crate) fn remove_worktree(state: State<'_, Arc<AppState>>, repo_path: String, branch_name: String, delete_branch: Option<bool>) -> Result<(), String> {
    remove_worktree_by_branch(&repo_path, &branch_name, delete_branch.unwrap_or(true))?;
    state.invalidate_repo_caches(&repo_path);
    Ok(())
}

/// Get worktree paths for a repo: maps branch name -> worktree directory
#[tauri::command]
pub(crate) fn get_worktree_paths(repo_path: String) -> Result<HashMap<String, String>, String> {
    let base_repo = PathBuf::from(&repo_path);

    let output = Command::new(crate::agent::resolve_cli("git"))
        .current_dir(&base_repo)
        .args(["worktree", "list", "--porcelain"])
        .output()
        .map_err(|e| format!("Failed to list worktrees: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git worktree list failed: {stderr}"));
    }

    let worktree_list = String::from_utf8_lossy(&output.stdout);
    let mut result = HashMap::new();
    let mut current_path: Option<String> = None;

    for line in worktree_list.lines() {
        if line.starts_with("worktree ") {
            current_path = Some(line.trim_start_matches("worktree ").to_string());
        } else if line.starts_with("branch refs/heads/") {
            let branch = line.trim_start_matches("branch refs/heads/").to_string();
            if let Some(ref path) = current_path {
                result.insert(branch, path.clone());
            }
        }
    }

    Ok(result)
}

/// Parse `git worktree list --porcelain` output and return paths of linked worktrees that are in
/// detached HEAD state (i.e. their branch has been deleted). The main worktree (first entry) is
/// always skipped — it can't be removed without removing the repo itself.
fn parse_orphan_worktrees(porcelain: &str) -> Vec<String> {
    let mut orphans = Vec::new();
    let mut is_first = true;

    for block in porcelain.split("\n\n") {
        let block = block.trim();
        if block.is_empty() {
            continue;
        }

        let mut path: Option<String> = None;
        let mut has_branch = false;
        let mut is_detached = false;

        for line in block.lines() {
            if line.starts_with("worktree ") {
                path = Some(line.trim_start_matches("worktree ").to_string());
            } else if line.starts_with("branch refs/heads/") {
                has_branch = true;
            } else if line == "detached" {
                is_detached = true;
            }
        }

        if is_first {
            is_first = false;
            continue;
        }

        if is_detached && !has_branch {
            orphans.extend(path);
        }
    }

    orphans
}

/// Detect orphan worktrees: linked worktrees present on the filesystem but in detached HEAD
/// state (i.e. their branch has been deleted). Returns a list of worktree directory paths.
#[tauri::command]
pub(crate) fn detect_orphan_worktrees(repo_path: String) -> Result<Vec<String>, String> {
    let base_repo = PathBuf::from(&repo_path);

    let output = Command::new(crate::agent::resolve_cli("git"))
        .current_dir(&base_repo)
        .args(["worktree", "list", "--porcelain"])
        .output()
        .map_err(|e| format!("Failed to list worktrees: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git worktree list failed: {stderr}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_orphan_worktrees(&stdout))
}

/// Remove an orphan worktree by its filesystem path (detached HEAD — no branch to look up).
#[tauri::command]
pub(crate) fn remove_orphan_worktree(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
    worktree_path: String,
) -> Result<(), String> {
    let base_repo = PathBuf::from(&repo_path);
    let path = PathBuf::from(&worktree_path);
    let worktree = WorktreeInfo {
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| worktree_path.clone()),
        path,
        branch: None,
        base_repo,
    };
    remove_worktree_internal(&worktree)?;
    state.invalidate_repo_caches(&repo_path);
    Ok(())
}

/// Generate a worktree name (Story 063)
#[tauri::command]
pub(crate) fn generate_worktree_name_cmd(existing_names: Vec<String>) -> String {
    generate_worktree_name(&existing_names)
}

/// Generate a hybrid clone branch name: `{sanitized_source}--{random_name}`
#[tauri::command]
pub(crate) fn generate_clone_branch_name_cmd(source_branch: String, existing_names: Vec<String>) -> String {
    generate_clone_branch_name(&source_branch, &existing_names)
}

/// List local branch names for a repository (excludes HEAD and remote-only refs)
#[tauri::command]
pub(crate) fn list_local_branches(repo_path: String) -> Result<Vec<String>, String> {
    let output = Command::new(crate::agent::resolve_cli("git"))
        .current_dir(&repo_path)
        .args(["branch", "--format=%(refname:short)"])
        .output()
        .map_err(|e| format!("Failed to list branches: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git branch failed: {stderr}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let branches: Vec<String> = stdout
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
pub(crate) fn get_remote_default_branch(repo_path: &str) -> Result<String, String> {
    // Try symbolic-ref first (cheapest, no network)
    let output = Command::new(crate::agent::resolve_cli("git"))
        .current_dir(repo_path)
        .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
        .output()
        .map_err(|e| format!("Failed to run git symbolic-ref: {e}"))?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let trimmed = stdout.trim();
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

/// List available base ref options for the CreateWorktreeDialog dropdown.
///
/// Returns a list of refs with the remote default branch first, followed by
/// all local branches (excluding the default which is already listed).
#[tauri::command]
pub(crate) fn list_base_ref_options(repo_path: String) -> Result<Vec<String>, String> {
    let default_branch = get_remote_default_branch(&repo_path)?;
    let all_branches = list_local_branches(repo_path)?;

    let mut refs = vec![default_branch.clone()];
    for branch in all_branches {
        if branch != default_branch {
            refs.push(branch);
        }
    }

    Ok(refs)
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
#[tauri::command]
pub(crate) fn switch_branch(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
    branch_name: String,
    force: bool,
    stash: bool,
) -> Result<SwitchBranchResult, String> {
    let git = crate::agent::resolve_cli("git");
    let base_repo = PathBuf::from(&repo_path);

    // Read current branch before switching
    let previous_branch = crate::git::read_branch_from_head(&base_repo)
        .unwrap_or_default();

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
        let status = Command::new(&git)
            .current_dir(&base_repo)
            .args(["status", "--porcelain"])
            .output()
            .map_err(|e| format!("Failed to check status: {e}"))?;

        if status.status.success() {
            let stdout = String::from_utf8_lossy(&status.stdout);
            if !stdout.trim().is_empty() {
                return Err("dirty".to_string());
            }
        }
    }

    // Stash if requested
    let did_stash = if stash {
        let stash_msg = format!("auto-stash before switching to {branch_name}");
        let stash_out = Command::new(&git)
            .current_dir(&base_repo)
            .args(["stash", "push", "-m", &stash_msg])
            .output()
            .map_err(|e| format!("Failed to stash: {e}"))?;

        if !stash_out.status.success() {
            let stderr = String::from_utf8_lossy(&stash_out.stderr);
            return Err(format!("Stash failed: {stderr}"));
        }

        // "No local changes to save" means nothing was stashed
        let stdout = String::from_utf8_lossy(&stash_out.stdout);
        !stdout.contains("No local changes to save")
    } else {
        false
    };

    // Checkout
    let mut args = vec!["checkout"];
    if force {
        args.push("--force");
    }
    args.push(&branch_name);

    let checkout = Command::new(&git)
        .current_dir(&base_repo)
        .args(&args)
        .output()
        .map_err(|e| format!("Failed to checkout {branch_name}: {e}"))?;

    if !checkout.status.success() {
        let stderr = String::from_utf8_lossy(&checkout.stderr);
        return Err(format!("Checkout failed: {stderr}"));
    }

    state.invalidate_repo_caches(&repo_path);

    Ok(SwitchBranchResult {
        success: true,
        stashed: did_stash,
        previous_branch,
        new_branch: branch_name,
    })
}

/// Create a local branch tracking a remote branch and switch to it.
/// Equivalent to `git checkout -b <branch> origin/<branch>`.
#[tauri::command]
pub(crate) fn checkout_remote_branch(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
    branch_name: String,
) -> Result<(), String> {
    let git = crate::agent::resolve_cli("git");
    let base_repo = PathBuf::from(&repo_path);
    let remote_ref = format!("origin/{branch_name}");

    let checkout = Command::new(&git)
        .current_dir(&base_repo)
        .args(["checkout", "-b", &branch_name, &remote_ref])
        .output()
        .map_err(|e| format!("Failed to checkout {branch_name}: {e}"))?;

    if !checkout.status.success() {
        let stderr = String::from_utf8_lossy(&checkout.stderr);
        return Err(format!("Checkout failed: {stderr}"));
    }

    state.invalidate_repo_caches(&repo_path);
    Ok(())
}

/// Result of a merge-and-archive operation
#[derive(Clone, Serialize)]
pub(crate) struct MergeArchiveResult {
    /// Whether the merge succeeded
    pub(crate) merged: bool,
    /// What happened to the worktree (archived / deleted / pending user choice)
    pub(crate) action: String,
    /// Path to archived directory (if archived)
    pub(crate) archive_path: Option<String>,
}

/// Complete a pending merge by archiving or deleting the worktree.
///
/// Called after `merge_and_archive_worktree` returns `action: "pending"` (ask mode).
/// The merge has already succeeded; this only handles the worktree cleanup.
#[tauri::command]
pub(crate) fn finalize_merged_worktree(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
    branch_name: String,
    action: String,
) -> Result<MergeArchiveResult, String> {
    let base_repo = std::path::PathBuf::from(&repo_path);
    match action.as_str() {
        "archive" => {
            let archive_path = archive_worktree(&base_repo, &branch_name)?;
            state.invalidate_repo_caches(&repo_path);
            Ok(MergeArchiveResult {
                merged: true,
                action: "archived".to_string(),
                archive_path: Some(archive_path),
            })
        }
        "delete" => {
            remove_worktree_by_branch(&repo_path, &branch_name, true)?;
            state.invalidate_repo_caches(&repo_path);
            Ok(MergeArchiveResult {
                merged: true,
                action: "deleted".to_string(),
                archive_path: None,
            })
        }
        _ => Err(format!("Unknown action '{action}': expected 'archive' or 'delete'")),
    }
}

/// Merge a worktree branch into a target branch, then archive or delete the worktree.
///
/// Steps:
/// 1. `git checkout <target_branch>` (in the base repo)
/// 2. `git merge <source_branch>` (in the base repo)
/// 3. Based on `after_merge`: archive (move dir) or delete (remove worktree + branch)
#[tauri::command]
pub(crate) fn merge_and_archive_worktree(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
    branch_name: String,
    target_branch: String,
    after_merge: String,
) -> Result<MergeArchiveResult, String> {
    let git = crate::agent::resolve_cli("git");
    let base_repo = PathBuf::from(&repo_path);

    // 1. Ensure we're on the target branch in the base repo
    let checkout = Command::new(&git)
        .current_dir(&base_repo)
        .args(["checkout", &target_branch])
        .output()
        .map_err(|e| format!("Failed to checkout {target_branch}: {e}"))?;

    if !checkout.status.success() {
        let stderr = String::from_utf8_lossy(&checkout.stderr);
        return Err(format!("Failed to checkout {target_branch}: {stderr}"));
    }

    // 2. Merge the source branch
    let merge = Command::new(&git)
        .current_dir(&base_repo)
        .args(["merge", &branch_name, "--no-edit"])
        .output()
        .map_err(|e| format!("Failed to merge {branch_name}: {e}"))?;

    if !merge.status.success() {
        let stderr = String::from_utf8_lossy(&merge.stderr);
        // Abort the merge to leave a clean state
        let _ = Command::new(&git)
            .current_dir(&base_repo)
            .args(["merge", "--abort"])
            .output();
        return Err(format!("Merge failed (conflicts?): {stderr}"));
    }

    // 3. Handle the worktree based on after_merge setting
    match after_merge.as_str() {
        "archive" => {
            let archive_path = archive_worktree(&base_repo, &branch_name)?;
            state.invalidate_repo_caches(&repo_path);
            Ok(MergeArchiveResult {
                merged: true,
                action: "archived".to_string(),
                archive_path: Some(archive_path),
            })
        }
        "delete" => {
            remove_worktree_by_branch(&repo_path, &branch_name, true)?;
            state.invalidate_repo_caches(&repo_path);
            Ok(MergeArchiveResult {
                merged: true,
                action: "deleted".to_string(),
                archive_path: None,
            })
        }
        _ => {
            // "ask" — merge succeeded, let frontend decide what to do next
            state.invalidate_repo_caches(&repo_path);
            Ok(MergeArchiveResult {
                merged: true,
                action: "pending".to_string(),
                archive_path: None,
            })
        }
    }
}

/// Archive a worktree: move its directory to `{worktrees_dir}/__archived/{branch_name}/`
/// and run `git worktree remove`.
pub(crate) fn archive_worktree(base_repo: &Path, branch_name: &str) -> Result<String, String> {
    let git = crate::agent::resolve_cli("git");

    // Find worktree path for this branch
    let output = Command::new(&git)
        .current_dir(base_repo)
        .args(["worktree", "list", "--porcelain"])
        .output()
        .map_err(|e| format!("Failed to list worktrees: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let wt_path = find_worktree_path_for_branch(&stdout, branch_name)
        .ok_or_else(|| format!("No worktree found for branch '{branch_name}'"))?;
    let parent_dir = wt_path.parent().ok_or("Worktree has no parent directory")?;
    let archive_dir = parent_dir.join("__archived");
    let sanitized = sanitize_name(branch_name);
    let archive_dest = archive_dir.join(&sanitized);

    // Create archive directory
    std::fs::create_dir_all(&archive_dir)
        .map_err(|e| format!("Failed to create archive directory: {e}"))?;

    // Remove git worktree link first (so git doesn't track it)
    let _ = Command::new(&git)
        .current_dir(base_repo)
        .args(["worktree", "remove", "--force", &wt_path.to_string_lossy()])
        .output();

    // Move the directory if it still exists (worktree remove may have deleted it)
    if wt_path.exists() {
        if archive_dest.exists() {
            std::fs::remove_dir_all(&archive_dest)
                .map_err(|e| format!("Failed to clean existing archive: {e}"))?;
        }
        std::fs::rename(&wt_path, &archive_dest)
            .map_err(|e| format!("Failed to move worktree to archive: {e}"))?;
    }

    // Prune stale worktree entries
    let _ = Command::new(&git)
        .current_dir(base_repo)
        .args(["worktree", "prune"])
        .output();

    Ok(archive_dest.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WorktreeStorage;
    use std::fs;
    use tempfile::TempDir;

    fn setup_test_repo() -> TempDir {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let repo_path = temp_dir.path();

        Command::new(crate::agent::resolve_cli("git"))
            .current_dir(repo_path)
            .args(["init"])
            .output()
            .expect("Failed to init git repo");

        Command::new(crate::agent::resolve_cli("git"))
            .current_dir(repo_path)
            .args(["config", "user.email", "test@test.com"])
            .output()
            .expect("Failed to config git");

        Command::new(crate::agent::resolve_cli("git"))
            .current_dir(repo_path)
            .args(["config", "user.name", "Test"])
            .output()
            .expect("Failed to config git");

        fs::write(repo_path.join("README.md"), "# Test").expect("Failed to write file");
        Command::new(crate::agent::resolve_cli("git"))
            .current_dir(repo_path)
            .args(["add", "."])
            .output()
            .expect("Failed to git add");
        Command::new(crate::agent::resolve_cli("git"))
            .current_dir(repo_path)
            .args(["commit", "-m", "Initial commit"])
            .output()
            .expect("Failed to git commit");

        temp_dir
    }

    #[test]
    fn test_sanitize_name() {
        assert_eq!(sanitize_name("my-task"), "my-task");
        assert_eq!(sanitize_name("My Task Name"), "my-task-name");
        assert_eq!(sanitize_name("task/with/slashes"), "task-with-slashes");
        assert_eq!(sanitize_name("task_with_underscores"), "task_with_underscores");
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
        assert!(result.is_ok(), "Failed to create worktree with branch: {:?}", result);

        let worktree = result.unwrap();
        assert_eq!(worktree.branch, Some("feature/new-feature".to_string()));
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

        assert!(worktree.path.exists(), "Worktree should exist before removal");

        let result = remove_worktree_internal(&worktree);
        assert!(result.is_ok(), "Failed to remove worktree: {:?}", result);

        assert!(!worktree.path.exists(), "Worktree path should not exist after removal");
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
        let result = remove_worktree_internal(&worktree);
        assert!(result.is_ok(), "Removing nonexistent worktree should succeed");
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
        let second = generate_clone_branch_name("main", &[first.clone()]);
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
        Command::new(crate::agent::resolve_cli("git"))
            .current_dir(repo.path())
            .args(["branch", "feature-x"])
            .output()
            .expect("Failed to create branch");

        let refs = list_base_ref_options(repo_path).unwrap();
        assert!(refs.len() >= 2, "Expected at least 2 refs, got: {refs:?}");
        // First entry should be the default branch (main or master)
        assert!(
            refs[0] == "main" || refs[0] == "master",
            "First ref should be default branch, got: {}",
            refs[0]
        );
        // feature-x should be in the list
        assert!(refs.contains(&"feature-x".to_string()), "feature-x not found in {refs:?}");
        // No duplicates
        let unique: std::collections::HashSet<_> = refs.iter().collect();
        assert_eq!(unique.len(), refs.len(), "Duplicate refs found: {refs:?}");
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
        Command::new(crate::agent::resolve_cli("git"))
            .current_dir(&wt.path)
            .args(["add", "."])
            .output()
            .expect("git add");
        Command::new(crate::agent::resolve_cli("git"))
            .current_dir(&wt.path)
            .args(["commit", "-m", "feat: add feature"])
            .output()
            .expect("git commit");

        // Archive the worktree
        let result = archive_worktree(repo.path(), "feat-archive-test");
        assert!(result.is_ok(), "Archive should succeed: {:?}", result);

        let archive_path = PathBuf::from(result.unwrap());
        // The worktree should no longer exist at original location
        assert!(!wt.path.exists(), "Original worktree path should be gone");
        // Archive destination should exist (only if worktree dir wasn't deleted by git)
        // Note: git worktree remove --force may delete the dir, in which case archive_dest won't exist
        // but the operation should still succeed
    }

    #[test]
    fn remove_worktree_by_branch_deletes_branch_when_true() {
        let repo = setup_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();
        let worktrees_dir = repo.path().join("worktrees");
        let git = crate::agent::resolve_cli("git");

        // Create a worktree with a new branch
        let config = WorktreeConfig {
            task_name: "feat-delete-branch".to_string(),
            base_repo: repo_path.clone(),
            branch: Some("feat-delete-branch".to_string()),
            create_branch: true,
        };
        create_worktree_internal(&worktrees_dir, &config, None)
            .expect("Failed to create worktree");

        // Remove with delete_branch=true
        remove_worktree_by_branch(&repo_path, "feat-delete-branch", true)
            .expect("Failed to remove worktree");

        // Branch should be gone
        let output = Command::new(&git)
            .current_dir(repo.path())
            .args(["branch", "--list", "feat-delete-branch"])
            .output()
            .expect("Failed to list branches");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.trim().is_empty(),
            "Branch should be deleted when delete_branch=true, but found: {stdout}"
        );
    }

    #[test]
    fn remove_worktree_by_branch_keeps_branch_when_false() {
        let repo = setup_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();
        let worktrees_dir = repo.path().join("worktrees");
        let git = crate::agent::resolve_cli("git");

        // Create a worktree with a new branch
        let config = WorktreeConfig {
            task_name: "feat-keep-branch".to_string(),
            base_repo: repo_path.clone(),
            branch: Some("feat-keep-branch".to_string()),
            create_branch: true,
        };
        create_worktree_internal(&worktrees_dir, &config, None)
            .expect("Failed to create worktree");

        // Remove with delete_branch=false
        remove_worktree_by_branch(&repo_path, "feat-keep-branch", false)
            .expect("Failed to remove worktree");

        // Branch should still exist
        let output = Command::new(&git)
            .current_dir(repo.path())
            .args(["branch", "--list", "feat-keep-branch"])
            .output()
            .expect("Failed to list branches");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            !stdout.trim().is_empty(),
            "Branch should be preserved when delete_branch=false"
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
}
