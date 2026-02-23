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

/// Create a git worktree for a task
pub(crate) fn create_worktree_internal(
    worktrees_dir: &Path,
    config: &WorktreeConfig,
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

/// Create a worktree without a PTY session
#[tauri::command]
pub(crate) fn create_worktree(
    state: State<'_, Arc<AppState>>,
    base_repo: String,
    branch_name: String,
    create_branch: Option<bool>,
) -> Result<serde_json::Value, String> {
    let config = WorktreeConfig {
        task_name: branch_name.clone(),
        base_repo,
        branch: Some(branch_name),
        create_branch: create_branch.unwrap_or(true),
    };

    let worktree = create_worktree_internal(&state.worktrees_dir, &config)?;
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

        let result = create_worktree_internal(&worktrees_dir, &config);
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

        let result = create_worktree_internal(&worktrees_dir, &config);
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
        let result1 = create_worktree_internal(&worktrees_dir, &config);
        assert!(result1.is_ok());

        let result2 = create_worktree_internal(&worktrees_dir, &config);
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

        let worktree = create_worktree_internal(&worktrees_dir, &config)
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

        let result = create_worktree_internal(&worktrees_dir, &config);
        assert!(result.is_ok());

        let worktree = result.unwrap();
        assert_eq!(worktree.name, "fix-bug--123--add-feature-");
        assert!(worktree.path.exists());
    }
}
