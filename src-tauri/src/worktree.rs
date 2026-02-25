use crate::config::WorktreeStorage;
use crate::state::{AppState, WorktreeInfo};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use tauri::State;

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
    strategy: &WorktreeStorage,
    app_worktrees_dir: &Path,
) -> PathBuf {
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
    if config.create_branch {
        if let Some(start_point) = base_ref {
            cmd.arg(start_point);
        }
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

    let worktree = create_worktree_internal(&state.worktrees_dir, &config, base_ref.as_deref())?;
    state.invalidate_repo_caches(&config.base_repo);

    Ok(serde_json::json!({
        "name": worktree.name,
        "path": worktree.path.to_string_lossy(),
        "branch": worktree.branch,
        "base_repo": worktree.base_repo.to_string_lossy(),
    }))
}

/// Get worktrees directory path
#[tauri::command]
pub(crate) fn get_worktrees_dir(state: State<'_, Arc<AppState>>) -> String {
    state.worktrees_dir.to_string_lossy().to_string()
}

/// Core logic for removing a git worktree by branch name.
pub(crate) fn remove_worktree_by_branch(repo_path: &str, branch_name: &str) -> Result<(), String> {
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
    let mut worktree_path: Option<PathBuf> = None;
    let mut current_path: Option<PathBuf> = None;

    // Parse porcelain output to find worktree with matching branch
    for line in worktree_list.lines() {
        if line.starts_with("worktree ") {
            current_path = Some(PathBuf::from(line.trim_start_matches("worktree ")));
        } else if line.starts_with("branch refs/heads/") {
            let branch = line.trim_start_matches("branch refs/heads/");
            if branch == branch_name {
                worktree_path = current_path;
                break;
            }
        }
    }

    let worktree_path = worktree_path.ok_or_else(|| {
        format!("No worktree found for branch '{branch_name}'")
    })?;

    // Remove the worktree
    let worktree = WorktreeInfo {
        name: branch_name.to_string(),
        path: worktree_path,
        branch: Some(branch_name.to_string()),
        base_repo,
    };

    remove_worktree_internal(&worktree)?;

    // Also delete the local branch (non-fatal: branch may still be useful)
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

    Ok(())
}

/// Remove a git worktree by branch name (Tauri command with cache invalidation)
#[tauri::command]
pub(crate) fn remove_worktree(state: State<'_, Arc<AppState>>, repo_path: String, branch_name: String) -> Result<(), String> {
    remove_worktree_by_branch(&repo_path, &branch_name)?;
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
        if let Some(branch) = trimmed.strip_prefix("refs/remotes/origin/") {
            if !branch.is_empty() {
                return Ok(branch.to_string());
            }
        }
    }

    // Fallback: check if main or master branches exist locally
    let branches = list_local_branches(repo_path.to_string()).unwrap_or_default();
    if branches.contains(&"main".to_string()) {
        return Ok("main".to_string());
    }
    if branches.contains(&"master".to_string()) {
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

#[cfg(test)]
mod tests {
    use super::*;
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
}
