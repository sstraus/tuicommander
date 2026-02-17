use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use tauri::State;

use crate::state::{AppState, GIT_CACHE_TTL};

/// Repository info for sidebar display
#[derive(Clone, Serialize)]
pub(crate) struct RepoInfo {
    pub(crate) path: String,
    pub(crate) name: String,
    pub(crate) initials: String,
    pub(crate) branch: String,
    pub(crate) status: String, // "clean", "dirty", "conflict"
    pub(crate) is_git_repo: bool,
}

/// Diff stats (additions/deletions)
#[derive(Clone, Serialize)]
pub(crate) struct DiffStats {
    additions: i32,
    deletions: i32,
}

/// Changed file information (for diff browser)
#[derive(Clone, Serialize)]
pub(crate) struct ChangedFile {
    path: String,
    status: String,
    additions: u32,
    deletions: u32,
}

/// Core logic for fetching git repository info (no caching).
pub(crate) fn get_repo_info_impl(path: &str) -> RepoInfo {
    let repo_path = PathBuf::from(path);

    // Check if it's a git repo
    let git_dir = repo_path.join(".git");
    if !git_dir.exists() && !repo_path.join("../.git").exists() {
        let name = repo_path.file_name().map_or_else(|| path.to_string(), |n| n.to_string_lossy().to_string());
        let initials = get_repo_initials(&name);
        return RepoInfo {
            path: path.to_string(),
            name,
            initials,
            branch: String::new(),
            status: "not-git".to_string(),
            is_git_repo: false,
        };
    }

    // Get branch name
    let branch = Command::new("git")
        .current_dir(&repo_path)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".to_string());

    // Get status
    let status = Command::new("git")
        .current_dir(&repo_path)
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .map(|o| {
            if o.status.success() {
                let output = String::from_utf8_lossy(&o.stdout);
                if output.is_empty() {
                    "clean".to_string()
                } else if output.contains("UU") || output.contains("AA") || output.contains("DD") {
                    "conflict".to_string()
                } else {
                    "dirty".to_string()
                }
            } else {
                "unknown".to_string()
            }
        })
        .unwrap_or_else(|| "unknown".to_string());

    let name = repo_path.file_name().map_or_else(|| path.to_string(), |n| n.to_string_lossy().to_string());
    let initials = get_repo_initials(&name);

    RepoInfo {
        path: path.to_string(),
        name,
        initials,
        branch,
        status,
        is_git_repo: true,
    }
}

/// Get git repository info for a path (cached, 5s TTL)
#[tauri::command]
pub(crate) fn get_repo_info(state: State<'_, Arc<AppState>>, path: String) -> RepoInfo {
    if let Some(cached) = AppState::get_cached(&state.repo_info_cache, &path, GIT_CACHE_TTL) {
        return cached;
    }

    let info = get_repo_info_impl(&path);
    AppState::set_cached(&state.repo_info_cache, path, info.clone());
    info
}

/// Core logic for renaming a git branch.
pub(crate) fn rename_branch_impl(path: &str, old_name: &str, new_name: &str) -> Result<(), String> {
    let repo_path = PathBuf::from(path);

    // Validate new branch name (no spaces, valid git branch name characters)
    if new_name.is_empty() {
        return Err("Branch name cannot be empty".to_string());
    }
    if new_name.contains(' ') {
        return Err("Branch name cannot contain spaces".to_string());
    }
    if new_name.starts_with('-') {
        return Err("Branch name cannot start with a hyphen".to_string());
    }
    if new_name.contains("..") {
        return Err("Branch name cannot contain '..'".to_string());
    }
    if new_name.ends_with(".lock") {
        return Err("Branch name cannot end with '.lock'".to_string());
    }

    // Execute git branch -m oldname newname
    let output = Command::new("git")
        .current_dir(&repo_path)
        .args(["branch", "-m", old_name, new_name])
        .output()
        .map_err(|e| format!("Failed to execute git branch: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("not found") || stderr.contains("does not exist") {
            return Err(format!("Branch '{old_name}' does not exist"));
        }
        if stderr.contains("already exists") {
            return Err(format!("Branch '{new_name}' already exists"));
        }
        return Err(format!("git branch rename failed: {stderr}"));
    }

    Ok(())
}

/// Rename a git branch (Tauri command with cache invalidation)
#[tauri::command]
pub(crate) fn rename_branch(state: State<'_, Arc<AppState>>, path: String, old_name: String, new_name: String) -> Result<(), String> {
    rename_branch_impl(&path, &old_name, &new_name)?;
    state.invalidate_repo_caches(&path);
    Ok(())
}

/// Get git diff for a repository
#[tauri::command]
pub(crate) fn get_git_diff(path: String) -> Result<String, String> {
    let repo_path = PathBuf::from(&path);

    let output = Command::new("git")
        .current_dir(&repo_path)
        .args(["diff", "--color=never"])
        .output()
        .map_err(|e| format!("Failed to execute git diff: {e}"))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("git diff failed: {stderr}"))
    }
}

/// Get diff stats (additions/deletions) for a repository
#[tauri::command]
pub(crate) fn get_diff_stats(path: String) -> DiffStats {
    let repo_path = PathBuf::from(&path);

    let output = Command::new("git")
        .current_dir(&repo_path)
        .args(["diff", "--shortstat"])
        .output();

    if let Ok(output) = output
        && output.status.success() {
            let stat_line = String::from_utf8_lossy(&output.stdout);
            // Parse: "1 file changed, 10 insertions(+), 5 deletions(-)"
            let mut additions = 0;
            let mut deletions = 0;

            for part in stat_line.split(',') {
                let part = part.trim();
                if part.contains("insertion") {
                    if let Some(num) = part.split_whitespace().next() {
                        additions = num.parse().unwrap_or(0);
                    }
                } else if part.contains("deletion")
                    && let Some(num) = part.split_whitespace().next() {
                        deletions = num.parse().unwrap_or(0);
                    }
            }

            return DiffStats { additions, deletions };
        }

    DiffStats { additions: 0, deletions: 0 }
}

/// Get list of changed files with status and stats
#[tauri::command]
pub(crate) fn get_changed_files(path: String) -> Result<Vec<ChangedFile>, String> {
    let repo_path = PathBuf::from(&path);

    // Get file status (M, A, D, R)
    let status_output = Command::new("git")
        .current_dir(&repo_path)
        .args(["diff", "--name-status"])
        .output()
        .map_err(|e| format!("Failed to execute git diff --name-status: {e}"))?;

    if !status_output.status.success() {
        let stderr = String::from_utf8_lossy(&status_output.stderr);
        return Err(format!("git diff --name-status failed: {stderr}"));
    }

    // Get per-file stats (additions/deletions)
    let stats_output = Command::new("git")
        .current_dir(&repo_path)
        .args(["diff", "--numstat"])
        .output()
        .map_err(|e| format!("Failed to execute git diff --numstat: {e}"))?;

    if !stats_output.status.success() {
        let stderr = String::from_utf8_lossy(&stats_output.stderr);
        return Err(format!("git diff --numstat failed: {stderr}"));
    }

    // Parse status output into map: filepath -> status
    let status_text = String::from_utf8_lossy(&status_output.stdout);
    let mut status_map: HashMap<String, String> = HashMap::new();
    for line in status_text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let status = parts[0].to_string();
            let file_path = parts[1..].join(" "); // Handle filenames with spaces
            status_map.insert(file_path, status);
        }
    }

    // Parse stats output and combine with status
    let stats_text = String::from_utf8_lossy(&stats_output.stdout);
    let mut files = Vec::new();
    for line in stats_text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let additions = parts[0].parse::<u32>().unwrap_or(0);
            let deletions = parts[1].parse::<u32>().unwrap_or(0);
            let file_path = parts[2..].join(" ");
            let status = status_map.get(&file_path).cloned().unwrap_or_else(|| "M".to_string());
            files.push(ChangedFile {
                path: file_path,
                status,
                additions,
                deletions,
            });
        }
    }

    Ok(files)
}

/// Get diff for a single file
#[tauri::command]
pub(crate) fn get_file_diff(path: String, file: String) -> Result<String, String> {
    let repo_path = PathBuf::from(&path);

    let output = Command::new("git")
        .current_dir(&repo_path)
        .args(["diff", "--color=never", "--", &file])
        .output()
        .map_err(|e| format!("Failed to execute git diff for file: {e}"))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("git diff failed for file {file}: {stderr}"))
    }
}

/// Generate 2-character initials from a repository name
pub(crate) fn get_repo_initials(name: &str) -> String {
    // Strip control characters (including null bytes) before processing
    let sanitized: String = name.chars().filter(|c| !c.is_control()).collect();
    let words: Vec<&str> = sanitized
        .split(|c: char| c == '-' || c == '_' || c.is_whitespace())
        .filter(|w| !w.is_empty())
        .collect();
    if words.len() >= 2 {
        let first = words[0].chars().next().unwrap_or_default();
        let second = words[1].chars().next().unwrap_or_default();
        format!("{}{}", first, second).to_uppercase()
    } else if !words.is_empty() {
        words[0].chars().take(2).collect::<String>().to_uppercase()
    } else {
        String::new()
    }
}

/// Generate initials from a repository name (Tauri command)
#[tauri::command]
pub(crate) fn get_initials(name: String) -> String {
    get_repo_initials(&name)
}

/// Check if a branch name is a main/primary branch
pub(crate) fn is_main_branch(branch_name: &str) -> bool {
    matches!(
        branch_name.to_lowercase().as_str(),
        "main" | "master" | "develop" | "development" | "dev"
    )
}

/// Check if a branch name is a main/primary branch (Tauri command)
#[tauri::command]
pub(crate) fn check_is_main_branch(branch: String) -> bool {
    is_main_branch(&branch)
}

/// Sort branches: main/primary branches first, then alphabetical by name
pub(crate) fn sort_branches(branches: &mut [serde_json::Value]) {
    branches.sort_by(|a, b| {
        let a_main = a.get("is_main").and_then(|v| v.as_bool()).unwrap_or(false);
        let b_main = b.get("is_main").and_then(|v| v.as_bool()).unwrap_or(false);
        let a_name = a.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let b_name = b.get("name").and_then(|v| v.as_str()).unwrap_or("");

        // Main branches first
        match (a_main, b_main) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a_name.cmp(b_name),
        }
    });
}

/// Get git branches for a repository (Story 052)
#[tauri::command]
pub(crate) fn get_git_branches(path: String) -> Result<Vec<serde_json::Value>, String> {
    let repo_path = PathBuf::from(&path);

    let output = Command::new("git")
        .current_dir(&repo_path)
        .args(["branch", "-a", "--format=%(refname:short) %(HEAD)"])
        .output()
        .map_err(|e| format!("Failed to execute git branch: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git branch failed: {stderr}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut branches: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            let parts: Vec<&str> = line.splitn(2, ' ').collect();
            let name = parts[0].trim().to_string();
            let is_current = parts.get(1).is_some_and(|s| s.trim() == "*");
            let is_remote = name.starts_with("origin/");
            serde_json::json!({
                "name": name,
                "is_current": is_current,
                "is_remote": is_remote,
                "is_main": is_main_branch(&name),
            })
        })
        .collect();

    sort_branches(&mut branches);

    Ok(branches)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_repo_initials_splits_on_hyphens() {
        assert_eq!(get_repo_initials("my-repo"), "MR");
    }

    #[test]
    fn get_repo_initials_splits_on_underscores() {
        assert_eq!(get_repo_initials("hello_world"), "HW");
    }

    #[test]
    fn get_repo_initials_splits_on_spaces() {
        assert_eq!(get_repo_initials("hello world"), "HW");
    }

    #[test]
    fn get_repo_initials_single_word_takes_first_two_chars() {
        assert_eq!(get_repo_initials("single"), "SI");
    }

    #[test]
    fn get_repo_initials_three_plus_words_uses_first_two() {
        assert_eq!(get_repo_initials("my-cool-repo"), "MC");
    }

    #[test]
    fn get_repo_initials_single_char() {
        assert_eq!(get_repo_initials("a"), "A");
    }

    #[test]
    fn get_repo_initials_empty_string() {
        assert_eq!(get_repo_initials(""), "");
    }

    #[test]
    fn is_main_branch_recognizes_main_branches() {
        assert!(is_main_branch("main"));
        assert!(is_main_branch("master"));
        assert!(is_main_branch("develop"));
        assert!(is_main_branch("development"));
        assert!(is_main_branch("dev"));
    }

    #[test]
    fn is_main_branch_is_case_insensitive() {
        assert!(is_main_branch("Main"));
        assert!(is_main_branch("MASTER"));
        assert!(is_main_branch("Develop"));
        assert!(is_main_branch("DEVELOPMENT"));
        assert!(is_main_branch("DEV"));
    }

    #[test]
    fn is_main_branch_rejects_non_main_branches() {
        assert!(!is_main_branch("feature/foo"));
        assert!(!is_main_branch("feature/main"));
        assert!(!is_main_branch("bugfix/master-fix"));
        assert!(!is_main_branch("staging"));
        assert!(!is_main_branch("release/1.0"));
        assert!(!is_main_branch("hotfix/urgent"));
        assert!(!is_main_branch(""));
    }

    /// Helper to create a branch JSON value for sort tests
    fn make_branch(name: &str, is_main: bool) -> serde_json::Value {
        serde_json::json!({
            "name": name,
            "is_current": false,
            "is_remote": false,
            "is_main": is_main,
        })
    }

    fn branch_names(branches: &[serde_json::Value]) -> Vec<&str> {
        branches.iter().map(|b| b["name"].as_str().unwrap()).collect()
    }

    #[test]
    fn sort_branches_main_first_then_alphabetical() {
        let mut branches = vec![
            make_branch("feature/z", false),
            make_branch("main", true),
            make_branch("feature/a", false),
        ];
        sort_branches(&mut branches);
        assert_eq!(branch_names(&branches), vec!["main", "feature/a", "feature/z"]);
    }

    #[test]
    fn sort_branches_multiple_main_branches_sorted_alphabetically() {
        let mut branches = vec![
            make_branch("feature/x", false),
            make_branch("master", true),
            make_branch("develop", true),
            make_branch("main", true),
        ];
        sort_branches(&mut branches);
        assert_eq!(branch_names(&branches), vec!["develop", "main", "master", "feature/x"]);
    }

    #[test]
    fn sort_branches_all_feature_branches_alphabetical() {
        let mut branches = vec![
            make_branch("feature/c", false),
            make_branch("feature/a", false),
            make_branch("feature/b", false),
        ];
        sort_branches(&mut branches);
        assert_eq!(branch_names(&branches), vec!["feature/a", "feature/b", "feature/c"]);
    }

    #[test]
    fn sort_branches_empty_input() {
        let mut branches: Vec<serde_json::Value> = vec![];
        sort_branches(&mut branches);
        assert!(branches.is_empty());
    }

    #[test]
    fn sort_branches_single_branch() {
        let mut branches = vec![make_branch("main", true)];
        sort_branches(&mut branches);
        assert_eq!(branch_names(&branches), vec!["main"]);
    }

    #[test]
    fn get_repo_initials_strips_null_bytes() {
        let result = get_repo_initials("my\0repo");
        assert!(!result.contains('\0'), "initials must not contain null bytes");
        assert_eq!(result, "MY");
    }

    #[test]
    fn get_repo_initials_null_byte_at_word_start() {
        let result = get_repo_initials("\0my-repo");
        assert!(!result.contains('\0'), "initials must not contain null bytes");
        assert_eq!(result, "MR");
    }

    #[test]
    fn get_repo_initials_all_separators() {
        assert_eq!(get_repo_initials("---"), "");
    }

    #[test]
    fn get_repo_initials_strips_control_characters() {
        let result = get_repo_initials("he\x01llo-wo\x02rld");
        for ch in result.chars() {
            assert!(!ch.is_control(), "initials must not contain control characters: {:?}", ch);
        }
        assert_eq!(result, "HW");
    }

    #[test]
    fn get_repo_initials_leading_separator() {
        assert_eq!(get_repo_initials("-my-repo"), "MR");
    }

    #[test]
    fn get_repo_initials_trailing_separator() {
        assert_eq!(get_repo_initials("my-repo-"), "MR");
    }

    #[test]
    fn get_repo_initials_consecutive_separators() {
        assert_eq!(get_repo_initials("my--repo"), "MR");
    }
}
