use crate::git_cli::git_cmd;
use crate::state::{AppState, WorktreeInfo};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::State;

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
    let base_repo_path = PathBuf::from(&config.base_repo);
    let wt_path_str = worktree_path.to_string_lossy().to_string();
    let mut args: Vec<String> = vec!["worktree".into(), "add".into()];

    if config.create_branch
        && let Some(ref branch) = config.branch {
            args.push("-b".into());
            args.push(branch.clone());
        }

    args.push(wt_path_str);

    if let Some(ref branch) = config.branch
        && !config.create_branch {
            args.push(branch.clone());
        }

    // Append base_ref as start-point when creating a new branch
    if config.create_branch
        && let Some(start_point) = base_ref {
            // Auto-fetch if the base ref is a remote tracking branch
            fetch_if_remote(&config.base_repo, start_point)?;
            args.push(start_point.to_string());
        }

    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    match git_cmd(&base_repo_path).args(&args_str).run() {
        Ok(_) => {}
        Err(crate::git_cli::GitError::NonZeroExit { ref stderr, .. })
            if stderr.contains("already exists") || stderr.contains("already checked out") =>
        {
            return Ok(WorktreeInfo {
                name: worktree_name,
                path: worktree_path,
                branch: config.branch.clone(),
                base_repo: PathBuf::from(&config.base_repo),
            });
        }
        Err(e) => return Err(format!("Git worktree failed: {e}")),
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

/// Remove a git worktree
pub(crate) fn remove_worktree_internal(worktree: &WorktreeInfo) -> Result<(), String> {
    // First, run git worktree remove
    let wt_path_str = worktree.path.to_string_lossy().to_string();
    match git_cmd(&worktree.base_repo)
        .args(["worktree", "remove", "--force", &wt_path_str])
        .run()
    {
        Ok(_) => {}
        Err(crate::git_cli::GitError::NonZeroExit { ref stderr, .. })
            if stderr.contains("not a working tree") || stderr.contains("No such file") =>
        {
            // Worktree doesn't exist — that's fine
        }
        Err(e) => return Err(format!("Git worktree remove failed: {e}")),
    }

    // Cleanup the directory if it still exists
    if worktree.path.exists() {
        std::fs::remove_dir_all(&worktree.path)
            .map_err(|e| format!("Failed to remove worktree directory: {e}"))?;
    }

    // Prune worktrees (non-fatal: stale entries are harmless)
    if let Err(e) = git_cmd(&worktree.base_repo).args(["worktree", "prune"]).run() {
        tracing::warn!(source = "worktree", "git worktree prune failed: {e}");
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
pub(crate) fn remove_worktree_by_branch(repo_path: &str, branch_name: &str, delete_branch: bool, archive_script: Option<&str>) -> Result<(), String> {
    let base_repo = PathBuf::from(repo_path);

    // List worktrees to find the path for this branch
    let out = git_cmd(&base_repo)
        .args(["worktree", "list", "--porcelain"])
        .run()
        .map_err(|e| format!("git worktree list failed: {e}"))?;

    let worktree_path = find_worktree_path_for_branch(&out.stdout, branch_name)
        .ok_or_else(|| format!("No worktree found for branch '{branch_name}'"))?;

    // Run archive/cleanup script before deletion (if configured)
    if let Some(script) = archive_script
        && !script.is_empty()
    {
        run_script_in_dir(script, &worktree_path)
            .map_err(|e| format!("Archive script failed: {e}"))?;
    }

    // Remove the worktree
    let worktree = WorktreeInfo {
        name: branch_name.to_string(),
        path: worktree_path,
        branch: Some(branch_name.to_string()),
        base_repo,
    };

    remove_worktree_internal(&worktree)?;

    // Delete the local branch when requested (non-fatal: branch may still be useful)
    if delete_branch
        && let Err(e) = git_cmd(&worktree.base_repo)
            .args(["branch", "-d", branch_name])
            .run()
        {
            tracing::warn!(source = "worktree", branch = %branch_name, "git branch -d failed: {e}");
        }

    Ok(())
}

/// Remove a git worktree by branch name (Tauri command with cache invalidation)
///
/// `delete_branch` defaults to `true` when omitted (preserving existing behavior).
#[tauri::command]
pub(crate) fn remove_worktree(state: State<'_, Arc<AppState>>, repo_path: String, branch_name: String, delete_branch: Option<bool>) -> Result<(), String> {
    let script = resolve_archive_script(&repo_path);
    remove_worktree_by_branch(&repo_path, &branch_name, delete_branch.unwrap_or(true), script.as_deref())?;
    state.invalidate_repo_caches(&repo_path);
    Ok(())
}

/// Check whether a branch's working directory has uncommitted changes.
///
/// If the branch has a linked worktree, runs `git status --porcelain` in that directory.
/// If no worktree exists (bare local ref), returns `false` — there's nothing to be dirty.
#[tauri::command]
pub(crate) fn check_worktree_dirty(repo_path: String, branch_name: String) -> Result<bool, String> {
    let base_repo = PathBuf::from(&repo_path);

    let wt_list = git_cmd(&base_repo)
        .args(["worktree", "list", "--porcelain"])
        .run()
        .map_err(|e| format!("Failed to list worktrees: {e}"))?
        .stdout;

    let wt_path = match find_worktree_path_for_branch(&wt_list, &branch_name) {
        Some(p) => p,
        None => return Ok(false), // No worktree = not dirty
    };

    let status_out = git_cmd(&wt_path)
        .args(["status", "--porcelain"])
        .run()
        .map_err(|e| format!("Failed to check worktree status: {e}"))?;
    Ok(!status_out.stdout.trim().is_empty())
}

/// Delete a local branch, removing its worktree first if one exists.
///
/// Safety: refuses to delete the repository's default branch.
/// Uses `git branch -d` (safe delete) which fails if the branch has unmerged commits.
pub(crate) fn delete_local_branch_impl(repo_path: &str, branch_name: &str) -> Result<(), String> {
    // Refuse to delete the default branch
    let default_branch = get_remote_default_branch(repo_path).unwrap_or_else(|_| "main".to_string());
    if branch_name == default_branch {
        return Err(format!("Refusing to delete default branch '{branch_name}'"));
    }

    let base_repo = PathBuf::from(repo_path);

    // Check if branch has a linked worktree
    let has_worktree = git_cmd(&base_repo)
        .args(["worktree", "list", "--porcelain"])
        .run_silent()
        .map(|o| find_worktree_path_for_branch(&o.stdout, branch_name).is_some())
        .unwrap_or(false);

    if has_worktree {
        // Remove worktree + branch in one go
        remove_worktree_by_branch(repo_path, branch_name, true, None)?;
    } else {
        // Just delete the branch ref
        git_cmd(&base_repo)
            .args(["branch", "-d", branch_name])
            .run()
            .map_err(|e| format!("git branch -d {branch_name} failed: {e}"))?;
    }

    Ok(())
}

/// Tauri command: delete a local branch (and its worktree if linked).
#[tauri::command]
pub(crate) fn delete_local_branch(state: State<'_, Arc<AppState>>, repo_path: String, branch_name: String) -> Result<(), String> {
    delete_local_branch_impl(&repo_path, &branch_name)?;
    state.invalidate_repo_caches(&repo_path);
    Ok(())
}

/// Get worktree paths for a repo: maps branch name -> worktree directory
#[tauri::command]
pub(crate) fn get_worktree_paths(repo_path: String) -> Result<HashMap<String, String>, String> {
    let base_repo = PathBuf::from(&repo_path);

    // Prune stale worktree entries (directories that no longer exist on disk)
    let _ = git_cmd(&base_repo).args(["worktree", "prune"]).run();

    let out = git_cmd(&base_repo)
        .args(["worktree", "list", "--porcelain"])
        .run()
        .map_err(|e| format!("git worktree list failed: {e}"))?;

    let mut result = HashMap::new();
    let mut current_path: Option<String> = None;

    for line in out.stdout.lines() {
        if line.starts_with("worktree ") {
            current_path = Some(line.trim_start_matches("worktree ").to_string());
        } else if line.starts_with("branch refs/heads/") {
            let branch = line.trim_start_matches("branch refs/heads/").to_string();
            if let Some(ref path) = current_path {
                // Skip entries whose directory no longer exists (double safety after prune)
                if Path::new(path).exists() {
                    result.insert(branch, path.clone());
                }
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

    let out = git_cmd(&base_repo)
        .args(["worktree", "list", "--porcelain"])
        .run()
        .map_err(|e| format!("git worktree list failed: {e}"))?;

    Ok(parse_orphan_worktrees(&out.stdout))
}

/// Remove an orphan worktree by its filesystem path (detached HEAD — no branch to look up).
///
/// Safety: `worktree_path` is validated against the repo's actual worktree list to prevent
/// arbitrary directory deletion via a crafted path.
#[tauri::command]
pub(crate) fn remove_orphan_worktree(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
    worktree_path: String,
) -> Result<(), String> {
    validate_worktree_path(&repo_path, &worktree_path)?;

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

/// Validate that `worktree_path` is a known worktree of the given repo by checking it against
/// `git worktree list --porcelain` output. Prevents arbitrary directory deletion.
pub(crate) fn validate_worktree_path(repo_path: &str, worktree_path: &str) -> Result<(), String> {
    let path = PathBuf::from(worktree_path);
    if !path.is_absolute() {
        return Err("worktree_path must be an absolute path".to_string());
    }

    let out = git_cmd(Path::new(repo_path))
        .args(["worktree", "list", "--porcelain"])
        .run()
        .map_err(|e| format!("git worktree list failed: {e}"))?;

    let known_paths: Vec<&str> = out
        .stdout
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .collect();

    if !known_paths.contains(&worktree_path) {
        return Err(format!(
            "Refused: '{}' is not a known worktree of '{}'",
            worktree_path, repo_path
        ));
    }

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
    let out = git_cmd(Path::new(&repo_path))
        .args(["branch", "--format=%(refname:short)"])
        .run()
        .map_err(|e| format!("git branch failed: {e}"))?;

    let branches: Vec<String> = out.stdout
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

/// Fetch a remote ref if the ref name looks like a remote tracking branch (e.g. "origin/main").
/// Local refs are a no-op. Returns Ok(()) on success or if the ref is local.
pub(crate) fn fetch_if_remote(repo_path: &str, ref_name: &str) -> Result<(), String> {
    // Remote refs contain a "/" and start with a remote name (e.g. "origin/branch")
    if let Some(slash_pos) = ref_name.find('/') {
        let remote = &ref_name[..slash_pos];
        let branch = &ref_name[slash_pos + 1..];
        if !remote.is_empty() && !branch.is_empty() {
            git_cmd(Path::new(repo_path))
                .args(["fetch", remote, branch])
                .run()
                .map_err(|e| format!("Failed to fetch {ref_name}: {e}"))?;
        }
    }
    Ok(())
}

/// Persist the base ref for a branch in git config.
/// Stored as `branch.<name>.tuicommander-base` in `.git/config`.
pub(crate) fn set_branch_base(repo_path: &str, branch_name: &str, base_ref: &str) -> Result<(), String> {
    let key = format!("branch.{branch_name}.tuicommander-base");
    git_cmd(Path::new(repo_path))
        .args(["config", &key, base_ref])
        .run()
        .map_err(|e| format!("Failed to set branch base: {e}"))?;
    Ok(())
}

/// Read the stored base ref for a branch from git config.
/// Returns None if not set.
pub(crate) fn get_branch_base(repo_path: &str, branch_name: &str) -> Option<String> {
    let key = format!("branch.{branch_name}.tuicommander-base");
    git_cmd(Path::new(repo_path))
        .args(["config", &key])
        .run_silent()
        .map(|out| out.stdout.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// A base ref option with metadata for grouped dropdown display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct BaseRefOption {
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
#[tauri::command]
pub(crate) fn list_base_ref_options(repo_path: String) -> Result<Vec<BaseRefOption>, String> {
    let default_branch = get_remote_default_branch(&repo_path)?;
    let repo = Path::new(&repo_path);

    // Get all refs (local + remote) in one git call
    let out = git_cmd(repo)
        .args(["for-each-ref", "--format=%(refname:short)\t%(refname)", "refs/heads/", "refs/remotes/"])
        .run()
        .map_err(|e| format!("git for-each-ref failed: {e}"))?;

    let mut local_refs: Vec<BaseRefOption> = Vec::new();
    let mut remote_refs: Vec<BaseRefOption> = Vec::new();
    let mut local_names: std::collections::HashSet<String> = std::collections::HashSet::new();

    for line in out.stdout.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }

        let parts: Vec<&str> = line.splitn(2, '\t').collect();
        if parts.len() != 2 { continue; }

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
            if short_name.ends_with("/HEAD") { continue; }
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
            .map_err(|e| format!("Stash failed: {e}"))?;

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

    git_cmd(&base_repo)
        .args(&args)
        .run()
        .map_err(|e| format!("Checkout failed: {e}"))?;

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
    let base_repo = PathBuf::from(&repo_path);
    let remote_ref = format!("origin/{branch_name}");

    git_cmd(&base_repo)
        .args(["checkout", "-b", &branch_name, &remote_ref])
        .run()
        .map_err(|e| format!("Checkout failed: {e}"))?;

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
    let script = resolve_archive_script(&repo_path);
    let base_repo = std::path::PathBuf::from(&repo_path);
    match action.as_str() {
        "archive" => {
            let archive_path = archive_worktree(&base_repo, &branch_name, script.as_deref())?;
            state.invalidate_repo_caches(&repo_path);
            Ok(MergeArchiveResult {
                merged: true,
                action: "archived".to_string(),
                archive_path: Some(archive_path),
            })
        }
        "delete" => {
            remove_worktree_by_branch(&repo_path, &branch_name, true, script.as_deref())?;
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
    let script = resolve_archive_script(&repo_path);
    let base_repo = PathBuf::from(&repo_path);

    // 1. Ensure we're on the target branch in the base repo
    git_cmd(&base_repo)
        .args(["checkout", &target_branch])
        .run()
        .map_err(|e| format!("Failed to checkout {target_branch}: {e}"))?;

    // 2. Merge the source branch
    if let Err(e) = git_cmd(&base_repo)
        .args(["merge", &branch_name, "--no-edit"])
        .run()
    {
        // Abort the merge to leave a clean state
        let _ = git_cmd(&base_repo).args(["merge", "--abort"]).run();
        return Err(format!("Merge failed (conflicts?): {e}"));
    }

    // 3. Handle the worktree based on after_merge setting
    match after_merge.as_str() {
        "archive" => {
            let archive_path = archive_worktree(&base_repo, &branch_name, script.as_deref())?;
            state.invalidate_repo_caches(&repo_path);
            Ok(MergeArchiveResult {
                merged: true,
                action: "archived".to_string(),
                archive_path: Some(archive_path),
            })
        }
        "delete" => {
            remove_worktree_by_branch(&repo_path, &branch_name, true, script.as_deref())?;
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
///
/// If `archive_script` is provided (non-empty), it runs in the worktree directory
/// before archiving. A non-zero exit code aborts the operation.
pub(crate) fn archive_worktree(base_repo: &Path, branch_name: &str, archive_script: Option<&str>) -> Result<String, String> {
    // Find worktree path for this branch
    let wt_list_out = git_cmd(base_repo)
        .args(["worktree", "list", "--porcelain"])
        .run()
        .map_err(|e| format!("Failed to list worktrees: {e}"))?;

    let wt_path = find_worktree_path_for_branch(&wt_list_out.stdout, branch_name)
        .ok_or_else(|| format!("No worktree found for branch '{branch_name}'"))?;

    // Run archive script before archiving (if configured)
    if let Some(script) = archive_script
        && !script.is_empty()
    {
        run_script_in_dir(script, &wt_path)
            .map_err(|e| format!("Archive script failed: {e}"))?;
    }
    let parent_dir = wt_path.parent().ok_or("Worktree has no parent directory")?;
    let archive_dir = parent_dir.join("__archived");
    let sanitized = sanitize_name(branch_name);
    let archive_dest = archive_dir.join(&sanitized);

    // Create archive directory
    std::fs::create_dir_all(&archive_dir)
        .map_err(|e| format!("Failed to create archive directory: {e}"))?;

    // Remove git worktree link first (so git doesn't track it)
    let wt_path_str = wt_path.to_string_lossy().to_string();
    if let Err(e) = git_cmd(base_repo)
        .args(["worktree", "remove", "--force", &wt_path_str])
        .run()
    {
        tracing::warn!(source = "worktree", "Archive: failed to remove worktree link: {e}");
    }

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
    let _ = git_cmd(base_repo).args(["worktree", "prune"]).run();

    Ok(archive_dest.to_string_lossy().to_string())
}

/// Run a shell script in a directory and return an error if it exits non-zero.
///
/// Used by archive/delete operations to run cleanup scripts before the operation.
fn run_script_in_dir(script: &str, cwd: &Path) -> Result<(), String> {
    let (shell, flag) = if cfg!(target_os = "windows") {
        ("cmd", "/C")
    } else {
        ("sh", "-c")
    };

    // TODO: add a timeout — a hung script with CREATE_NO_WINDOW has no visible
    // window, so users can't see or interrupt it (issue #7 follow-up).
    let mut cmd = std::process::Command::new(shell);
    cmd.arg(flag).arg(script).current_dir(cwd);
    crate::cli::apply_no_window(&mut cmd);
    let output = cmd.output()
        .map_err(|e| format!("Failed to execute script: {e}"))?;

    let exit_code = output.status.code().unwrap_or(-1);
    if exit_code != 0 {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Script failed with exit code {exit_code}: {stderr}"));
    }
    Ok(())
}

/// Run a shell script in a given directory and return exit code + output.
///
/// Used to execute setup/run scripts after worktree creation.
/// The script is passed to `sh -c` (Unix) or `cmd /C` (Windows).
#[tauri::command]
pub(crate) fn run_setup_script(script: String, cwd: String) -> Result<serde_json::Value, String> {
    let cwd_path = Path::new(&cwd);
    if !cwd_path.exists() {
        return Err(format!("Working directory does not exist: {cwd}"));
    }

    let (shell, flag) = if cfg!(target_os = "windows") {
        ("cmd", "/C")
    } else {
        ("sh", "-c")
    };

    // TODO: add a timeout — a hung script with CREATE_NO_WINDOW has no visible
    // window, so users can't see or interrupt it (issue #7 follow-up).
    let mut cmd = std::process::Command::new(shell);
    cmd.arg(flag).arg(&script).current_dir(cwd_path);
    crate::cli::apply_no_window(&mut cmd);
    let output = cmd.output()
        .map_err(|e| format!("Failed to execute script: {e}"))?;

    Ok(serde_json::json!({
        "exit_code": output.status.code().unwrap_or(-1),
        "stdout": String::from_utf8_lossy(&output.stdout),
        "stderr": String::from_utf8_lossy(&output.stderr),
    }))
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

        git_cmd(repo_path).args(["init"]).run().expect("Failed to init git repo");
        git_cmd(repo_path).args(["config", "user.email", "test@test.com"]).run().expect("Failed to config git");
        git_cmd(repo_path).args(["config", "user.name", "Test"]).run().expect("Failed to config git");

        fs::write(repo_path.join("README.md"), "# Test").expect("Failed to write file");
        git_cmd(repo_path).args(["add", "."]).run().expect("Failed to git add");
        git_cmd(repo_path).args(["commit", "-m", "Initial commit"]).run().expect("Failed to git commit");

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
        git_cmd(repo.path()).args(["branch", "feature-x"]).run().expect("Failed to create branch");

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
        assert!(names.contains(&"feature-x"), "feature-x not found in {names:?}");
        // No duplicate names
        let unique: std::collections::HashSet<&str> = names.iter().copied().collect();
        assert_eq!(unique.len(), refs.len(), "Duplicate refs found: {names:?}");
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
        git_cmd(&wt.path).args(["commit", "-m", "feat: add feature"]).run().expect("git commit");

        // Archive the worktree
        let result = archive_worktree(repo.path(), "feat-archive-test", None);
        assert!(result.is_ok(), "Archive should succeed: {:?}", result);

        let _archive_path = PathBuf::from(result.unwrap());
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
        remove_worktree_by_branch(&repo_path, "feat-delete-branch", true, None)
            .expect("Failed to remove worktree");

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
    fn remove_worktree_by_branch_keeps_branch_when_false() {
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
        create_worktree_internal(&worktrees_dir, &config, None)
            .expect("Failed to create worktree");

        // Remove with delete_branch=false
        remove_worktree_by_branch(&repo_path, "feat-keep-branch", false, None)
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

    #[test]
    fn delete_local_branch_removes_bare_branch() {
        let repo = setup_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();

        // Create a branch from current HEAD
        git_cmd(repo.path()).args(["branch", "feat-to-delete"]).run().expect("Failed to create branch");

        // Verify it exists
        let branches = list_local_branches(repo_path.clone()).unwrap();
        assert!(branches.contains(&"feat-to-delete".to_string()));

        // Delete it
        let result = delete_local_branch_impl(&repo_path, "feat-to-delete");
        assert!(result.is_ok(), "delete_local_branch_impl failed: {:?}", result);

        // Verify it's gone
        let branches = list_local_branches(repo_path).unwrap();
        assert!(!branches.contains(&"feat-to-delete".to_string()));
    }

    #[test]
    fn delete_local_branch_refuses_default_branch() {
        let repo = setup_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();

        let default_branch = get_remote_default_branch(&repo_path).unwrap();
        let result = delete_local_branch_impl(&repo_path, &default_branch);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Refusing to delete default branch"));
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

        // Delete via delete_local_branch_impl
        let result = delete_local_branch_impl(&repo_path, &wt.name);
        assert!(result.is_ok(), "delete_local_branch_impl failed: {:?}", result);

        // Worktree directory should be removed
        assert!(!wt.path.exists(), "Worktree directory should be gone");
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
        assert!(dirty.unwrap(), "Worktree with uncommitted changes should be dirty");
    }

    #[test]
    fn check_worktree_dirty_no_worktree() {
        let repo = setup_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();

        // Create a branch without a worktree
        git_cmd(repo.path()).args(["branch", "bare-branch"]).run().expect("Failed to create branch");

        let dirty = check_worktree_dirty(repo_path, "bare-branch".to_string());
        assert!(dirty.is_ok());
        assert!(!dirty.unwrap(), "Branch without worktree should not be dirty");
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

    #[test]
    fn run_setup_script_failure() {
        let dir = TempDir::new().expect("temp dir");
        let cwd = dir.path().to_string_lossy().to_string();

        let result = run_setup_script("exit 42".to_string(), cwd).expect("should return result even on non-zero exit");
        assert_eq!(result["exit_code"], 42);
    }

    #[test]
    fn run_setup_script_captures_stderr() {
        let dir = TempDir::new().expect("temp dir");
        let cwd = dir.path().to_string_lossy().to_string();

        let result = run_setup_script("echo oops >&2; exit 1".to_string(), cwd).expect("should return result");
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

        let result = run_setup_script("cat marker.txt".to_string(), cwd).expect("should succeed");
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
        let result = run_script_in_dir("cat test-file.txt", dir.path());
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
        let _wt = create_worktree_internal(&worktrees_dir, &config, None)
            .expect("create worktree");
        // Script creates a marker file inside the worktree dir; archive should still succeed
        let marker = worktrees_dir.join("archive-marker.txt");
        let script = format!("touch {}", marker.display());
        let result = archive_worktree(repo.path(), "archive-script-test", Some(&script));
        assert!(result.is_ok(), "archive with script should succeed: {:?}", result);
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
        let wt = create_worktree_internal(&worktrees_dir, &config, None)
            .expect("create worktree");
        // Script exits non-zero — archive should be blocked
        let result = archive_worktree(repo.path(), "archive-block-test", Some("exit 1"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Archive script failed"));
        // Worktree should still exist (not archived)
        assert!(wt.path.exists(), "worktree should still exist after failed script");
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
        create_worktree_internal(&worktrees_dir, &config, None)
            .expect("create worktree");
        // None script — should proceed normally
        let result = archive_worktree(repo.path(), "archive-noscript-test", None);
        assert!(result.is_ok(), "archive without script should succeed: {:?}", result);
    }

    #[test]
    fn test_set_and_get_branch_base() {
        let repo = setup_test_repo();
        let path = repo.path().to_string_lossy().to_string();

        // Initially no base set
        let base = get_branch_base(&path, "main");
        assert!(base.is_none(), "expected no base initially, got: {:?}", base);

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
    fn test_fetch_remote_ref_for_local_is_noop() {
        let repo = setup_test_repo();
        let path = repo.path().to_string_lossy().to_string();

        // A local ref like "main" should not trigger a fetch (no-op)
        let result = fetch_if_remote(&path, "main");
        assert!(result.is_ok(), "local ref should not fail: {:?}", result);
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

    #[test]
    fn test_list_base_ref_options_returns_structured_refs() {
        let repo = setup_test_repo();
        let path = repo.path().to_string_lossy().to_string();

        // Create extra local branches
        git_cmd(repo.path()).args(["branch", "feature-a"]).run().unwrap();
        git_cmd(repo.path()).args(["branch", "feature-b"]).run().unwrap();

        let refs = list_base_ref_options(path).unwrap();

        // Should have at least the default + 2 feature branches
        assert!(refs.len() >= 3, "expected at least 3 refs, got {}", refs.len());

        // First ref should be the default branch, flagged is_default
        let default_ref = &refs[0];
        assert!(default_ref.is_default, "first ref should be the default branch");
        assert_eq!(default_ref.kind, "local");

        // All refs should have non-empty names
        for r in &refs {
            assert!(!r.name.is_empty(), "ref name should not be empty");
            assert!(r.kind == "local" || r.kind == "remote", "kind should be local or remote");
        }

        // feature-a and feature-b should be present as local
        let names: Vec<&str> = refs.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"feature-a"), "feature-a should be in refs");
        assert!(names.contains(&"feature-b"), "feature-b should be in refs");

        // No origin/HEAD should appear
        assert!(!names.contains(&"origin/HEAD"), "origin/HEAD should be filtered out");
    }

    #[test]
    fn test_list_base_ref_options_includes_remote_refs() {
        let repo = setup_test_repo();
        let path_str = repo.path().to_string_lossy().to_string();

        // Create a bare remote and push to it to get remote tracking refs
        let remote_dir = TempDir::new().unwrap();
        git_cmd(remote_dir.path()).args(["init", "--bare"]).run().unwrap();
        git_cmd(repo.path())
            .args(["remote", "add", "origin", &remote_dir.path().to_string_lossy()])
            .run().unwrap();
        git_cmd(repo.path()).args(["push", "-u", "origin", "main"]).run()
            .or_else(|_| git_cmd(repo.path()).args(["push", "-u", "origin", "master"]).run())
            .unwrap();

        // Create a remote-only branch
        git_cmd(repo.path()).args(["branch", "remote-only"]).run().unwrap();
        git_cmd(repo.path()).args(["push", "origin", "remote-only"]).run().unwrap();
        git_cmd(repo.path()).args(["branch", "-D", "remote-only"]).run().unwrap();

        // Fetch so we have remote tracking refs
        git_cmd(repo.path()).args(["fetch", "origin"]).run().unwrap();

        let refs = list_base_ref_options(path_str).unwrap();

        // Should include remote refs
        let remote_refs: Vec<&BaseRefOption> = refs.iter().filter(|r| r.kind == "remote").collect();
        assert!(!remote_refs.is_empty(), "should include remote refs, got: {:?}", refs);

        // origin/remote-only should appear as remote
        let names: Vec<&str> = refs.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"origin/remote-only"), "origin/remote-only should be in refs, got: {:?}", names);
    }
}
