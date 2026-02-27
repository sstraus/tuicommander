use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use tauri::State;

use crate::state::{AppState, GIT_CACHE_TTL};

// --- File-based git helpers (no subprocess) ---

/// Resolve the .git directory for a repo, handling linked worktrees.
///
/// - Normal repo: `<repo>/.git/` (a directory)
/// - Linked worktree: `<repo>/.git` is a file containing `gitdir: <path>`
pub(crate) fn resolve_git_dir(repo_path: &Path) -> Option<PathBuf> {
    let git_entry = repo_path.join(".git");
    if git_entry.is_dir() {
        Some(git_entry)
    } else if git_entry.is_file() {
        let content = fs::read_to_string(&git_entry).ok()?;
        let gitdir = content.strip_prefix("gitdir: ")?.trim();
        let gitdir_path = if Path::new(gitdir).is_absolute() {
            PathBuf::from(gitdir)
        } else {
            repo_path.join(gitdir)
        };
        if gitdir_path.is_dir() {
            Some(gitdir_path)
        } else {
            None
        }
    } else {
        None
    }
}

/// Read the current branch name from .git/HEAD (file I/O, no subprocess).
/// Returns None for detached HEAD or if the file can't be read.
pub(crate) fn read_branch_from_head(repo_path: &Path) -> Option<String> {
    let git_dir = resolve_git_dir(repo_path)?;
    let head_content = fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let trimmed = head_content.trim();
    // HEAD is either "ref: refs/heads/<branch>" or a raw commit hash (detached)
    let ref_prefix = "ref: refs/heads/";
    if let Some(branch) = trimmed.strip_prefix(ref_prefix)
        && !branch.is_empty()
    {
        return Some(branch.to_string());
    }
    None // detached HEAD
}

/// Read the origin remote URL from .git/config (file I/O, no subprocess).
/// Parses the `[remote "origin"]` section for the `url` key.
pub(crate) fn read_remote_url(repo_path: &Path) -> Option<String> {
    let git_dir = resolve_git_dir(repo_path)?;
    let config_content = fs::read_to_string(git_dir.join("config")).ok()?;
    parse_git_config_remote_url(&config_content, "origin")
}

/// Parse a git config string for a remote's URL.
fn parse_git_config_remote_url(config: &str, remote_name: &str) -> Option<String> {
    let section_header = format!("[remote \"{remote_name}\"]");
    let mut in_section = false;
    for line in config.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_section = trimmed == section_header;
            continue;
        }
        if in_section
            && let Some(rest) = trimmed.strip_prefix("url")
        {
            let rest = rest.trim_start();
            if let Some(value) = rest.strip_prefix('=') {
                return Some(value.trim().to_string());
            }
        }
    }
    None
}

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

    // Read branch from .git/HEAD (no subprocess)
    let branch = read_branch_from_head(&repo_path)
        .unwrap_or_else(|| "unknown".to_string());

    // Get status
    let status = Command::new(crate::agent::resolve_cli("git"))
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
    let output = Command::new(crate::agent::resolve_cli("git"))
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

/// A recent commit entry for the dropdown
#[derive(Debug, Clone, Serialize)]
pub(crate) struct RecentCommit {
    pub hash: String,
    pub short_hash: String,
    pub subject: String,
}

/// Get the N most recent commits
#[tauri::command]
pub(crate) fn get_recent_commits(path: String, count: Option<u32>) -> Result<Vec<RecentCommit>, String> {
    let repo_path = PathBuf::from(&path);
    let n = count.unwrap_or(5).min(20).to_string();

    let output = Command::new(crate::agent::resolve_cli("git"))
        .current_dir(&repo_path)
        .args(["log", "--format=%H%x00%h%x00%s", "-n", &n])
        .output()
        .map_err(|e| format!("Failed to execute git log: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git log failed: {stderr}"));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let commits = text
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.splitn(3, '\0').collect();
            if parts.len() == 3 {
                Some(RecentCommit {
                    hash: parts[0].to_string(),
                    short_hash: parts[1].to_string(),
                    subject: parts[2].to_string(),
                })
            } else {
                None
            }
        })
        .collect();

    Ok(commits)
}

/// Build the base git diff args for the given scope.
/// A commit hash compares `<hash>..HEAD`; empty/None compares working tree.
fn diff_base_args(scope: &Option<String>) -> Vec<String> {
    match scope.as_deref() {
        Some(hash) if !hash.is_empty() => {
            vec!["diff".into(), format!("{hash}^"), hash.into()]
        }
        _ => vec!["diff".into()],
    }
}

/// Get git diff for a repository
#[tauri::command]
pub(crate) fn get_git_diff(path: String, scope: Option<String>) -> Result<String, String> {
    let repo_path = PathBuf::from(&path);

    let mut args = diff_base_args(&scope);
    args.push("--color=never".into());

    let output = Command::new(crate::agent::resolve_cli("git"))
        .current_dir(&repo_path)
        .args(&args)
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
pub(crate) fn get_diff_stats(path: String, scope: Option<String>) -> DiffStats {
    let repo_path = PathBuf::from(&path);

    let mut args = diff_base_args(&scope);
    args.push("--shortstat".into());

    let output = Command::new(crate::agent::resolve_cli("git"))
        .current_dir(&repo_path)
        .args(&args)
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
pub(crate) fn get_changed_files(path: String, scope: Option<String>) -> Result<Vec<ChangedFile>, String> {
    let repo_path = PathBuf::from(&path);

    // Get file status (M, A, D, R)
    let mut status_args = diff_base_args(&scope);
    status_args.push("--name-status".into());

    let status_output = Command::new(crate::agent::resolve_cli("git"))
        .current_dir(&repo_path)
        .args(&status_args)
        .output()
        .map_err(|e| format!("Failed to execute git diff --name-status: {e}"))?;

    if !status_output.status.success() {
        let stderr = String::from_utf8_lossy(&status_output.stderr);
        return Err(format!("git diff --name-status failed: {stderr}"));
    }

    // Get per-file stats (additions/deletions)
    let mut stats_args = diff_base_args(&scope);
    stats_args.push("--numstat".into());

    let stats_output = Command::new(crate::agent::resolve_cli("git"))
        .current_dir(&repo_path)
        .args(&stats_args)
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

    // For working tree scope, also include untracked files
    if scope.is_none() {
        let untracked_output = Command::new(crate::agent::resolve_cli("git"))
            .current_dir(&repo_path)
            .args(["ls-files", "--others", "--exclude-standard"])
            .output()
            .map_err(|e| format!("Failed to list untracked files: {e}"))?;

        if untracked_output.status.success() {
            let untracked_text = String::from_utf8_lossy(&untracked_output.stdout);
            for line in untracked_text.lines() {
                let file_path = line.trim();
                if file_path.is_empty() {
                    continue;
                }
                // Count lines in the new file for the additions stat
                let full_path = repo_path.join(file_path);
                let additions = std::fs::read_to_string(&full_path)
                    .map(|c| c.lines().count() as u32)
                    .unwrap_or(0);
                files.push(ChangedFile {
                    path: file_path.to_string(),
                    status: "?".to_string(),
                    additions,
                    deletions: 0,
                });
            }
        }
    }

    Ok(files)
}

/// Null device path — `/dev/null` on Unix, `NUL` on Windows
#[cfg(not(windows))]
const NULL_DEVICE: &str = "/dev/null";
#[cfg(windows)]
const NULL_DEVICE: &str = "NUL";

/// Get diff for a single file
#[tauri::command]
pub(crate) fn get_file_diff(path: String, file: String, scope: Option<String>) -> Result<String, String> {
    let repo_path = PathBuf::from(&path);

    // For untracked files, use --no-index to generate a diff against the null device
    if scope.is_none() {
        let full_path = repo_path.join(&file);

        // Security: prevent path traversal (e.g. "../../etc/passwd")
        let canonical_repo = repo_path.canonicalize()
            .map_err(|e| format!("Failed to resolve repo path: {e}"))?;
        let canonical_file = full_path.canonicalize()
            .map_err(|e| format!("Failed to resolve file path: {e}"))?;
        if !canonical_file.starts_with(&canonical_repo) {
            return Err("Access denied: file is outside repository".to_string());
        }

        let is_untracked = match Command::new(crate::agent::resolve_cli("git"))
            .current_dir(&repo_path)
            .args(["ls-files", "--error-unmatch", &file])
            .output()
        {
            Ok(o) => !o.status.success(),
            Err(e) => {
                eprintln!("[git] ls-files spawn failed for {file}: {e}");
                false
            }
        };

        if is_untracked {
            let output = Command::new(crate::agent::resolve_cli("git"))
                .current_dir(&repo_path)
                .args(["diff", "--color=never", "--no-index", "--", NULL_DEVICE])
                .arg(&full_path)
                .output()
                .map_err(|e| format!("Failed to diff untracked file: {e}"))?;
            // --no-index exits with 1 when files differ (expected vs null device),
            // but exit code > 1 indicates an actual error.
            let code = output.status.code().unwrap_or(-1);
            if code > 1 {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(format!("git diff --no-index failed (exit {code}): {stderr}"));
            }
            return Ok(String::from_utf8_lossy(&output.stdout).to_string());
        }
    }

    let mut args = diff_base_args(&scope);
    args.push("--color=never".into());
    args.push("--".into());

    let output = Command::new(crate::agent::resolve_cli("git"))
        .current_dir(&repo_path)
        .args(&args)
        .arg(&file)
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

/// Canonical list of branch names considered "main" / primary.
/// Used by both `is_main_branch()` and `get_merged_branches_impl()`.
pub(crate) const MAIN_BRANCH_CANDIDATES: &[&str] = &["main", "master", "develop", "development", "dev"];

/// Check if a branch name is a main/primary branch
pub(crate) fn is_main_branch(branch_name: &str) -> bool {
    MAIN_BRANCH_CANDIDATES.contains(&branch_name.to_lowercase().as_str())
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

/// Detect the default/primary branch for a repository via file I/O.
///
/// Strategy (in order):
/// 1. Check `MAIN_BRANCH_CANDIDATES` against local refs
/// 2. Read `refs/remotes/origin/HEAD` symref (set by `git clone` or `git remote set-head`)
/// 3. Return `None` if no default branch can be determined
fn detect_default_branch(git_dir: &Path) -> Option<String> {
    // 1. Check well-known candidate names in local refs
    if let Some(name) = MAIN_BRANCH_CANDIDATES.iter().find(|name| {
        git_dir.join("refs/heads").join(name).exists()
            || packed_ref_exists(git_dir, &format!("refs/heads/{name}"))
    }) {
        return Some(name.to_string());
    }

    // 2. Read origin/HEAD symref (e.g. "ref: refs/remotes/origin/main\n")
    let origin_head = git_dir.join("refs/remotes/origin/HEAD");
    if let Ok(content) = fs::read_to_string(&origin_head)
        && let Some(target) = content.trim().strip_prefix("ref: refs/remotes/origin/")
    {
        let branch = target.trim();
        // Verify the branch exists locally before using it
        if git_dir.join("refs/heads").join(branch).exists()
            || packed_ref_exists(git_dir, &format!("refs/heads/{branch}"))
        {
            return Some(branch.to_string());
        }
    }

    None
}

/// Get local branches that are fully merged into the repo's main branch.
/// Returns branch names whose tips are reachable from the main branch HEAD.
/// Returns an empty vec (not an error) when the repo has no detectable default branch.
pub(crate) fn get_merged_branches_impl(repo_path: &Path) -> Result<Vec<String>, String> {
    let git_dir = match resolve_git_dir(repo_path) {
        Some(d) => d,
        None => return Ok(vec![]),  // Not a git repo — graceful no-op
    };

    let main_branch = match detect_default_branch(&git_dir) {
        Some(b) => b,
        None => return Ok(vec![]),  // No default branch — graceful no-op
    };

    let output = Command::new(crate::agent::resolve_cli("git"))
        .current_dir(repo_path)
        .args(["branch", "--merged", &main_branch, "--format=%(refname:short)"])
        .output()
        .map_err(|e| format!("Failed to run git branch --merged: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git branch --merged failed: {stderr}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
}

/// Check whether a ref exists in .git/packed-refs (for repos that have been gc'd).
fn packed_ref_exists(git_dir: &Path, ref_name: &str) -> bool {
    let packed_refs = git_dir.join("packed-refs");
    fs::read_to_string(packed_refs)
        .map(|content| content.lines().any(|line| {
            // Lines are "<sha> <ref>" or comments starting with '#'/'^'
            line.split_whitespace().nth(1) == Some(ref_name)
        }))
        .unwrap_or(false)
}

/// Tauri command: get branches merged into the main branch (cached, 5s TTL)
#[tauri::command]
pub(crate) async fn get_merged_branches(
    state: State<'_, Arc<AppState>>,
    path: String,
) -> Result<Vec<String>, String> {
    if let Some(cached) = AppState::get_cached(&state.merged_branches_cache, &path, GIT_CACHE_TTL) {
        return Ok(cached);
    }

    let state_arc = state.inner().clone();
    let path_clone = path.clone();
    let result = tokio::task::spawn_blocking(move || {
        get_merged_branches_impl(Path::new(&path_clone))
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))??;
    AppState::set_cached(&state_arc.merged_branches_cache, path, result.clone());
    Ok(result)
}

/// Get git branches for a repository (Story 052)
#[tauri::command]
pub(crate) fn get_git_branches(path: String) -> Result<Vec<serde_json::Value>, String> {
    let repo_path = PathBuf::from(&path);

    let output = Command::new(crate::agent::resolve_cli("git"))
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
fn ensure_askpass_script() -> Option<PathBuf> {
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
        eprintln!("[git] Failed to write askpass script: {e}");
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

/// Run an arbitrary git command in the background (no PTY, no terminal).
/// Used by the sidebar Git Quick Actions (pull, push, fetch, stash).
/// Async so network operations (pull/push/fetch) don't block the IPC thread.
/// Sets SSH_ASKPASS so passphrase prompts show a native GUI dialog.
#[tauri::command]
pub(crate) async fn run_git_command(
    state: State<'_, Arc<AppState>>,
    path: String,
    args: Vec<String>,
) -> Result<GitCommandResult, String> {
    let state_arc = state.inner().clone();
    let path_clone = path.clone();
    let askpass = ensure_askpass_script();

    tokio::task::spawn_blocking(move || {
        let repo_path = PathBuf::from(&path_clone);

        let mut cmd = Command::new(crate::agent::resolve_cli("git"));
        cmd.current_dir(&repo_path).args(&args);

        // Enable GUI-based SSH authentication so passphrase-protected keys work
        // without a TTY. SSH_ASKPASS_REQUIRE=prefer tells SSH to use the askpass
        // program even when stdin looks like it could be a terminal.
        if let Some(ref askpass_path) = askpass {
            cmd.env("SSH_ASKPASS", askpass_path);
            cmd.env("SSH_ASKPASS_REQUIRE", "prefer");
            cmd.env("DISPLAY", ":0"); // Required on Linux for SSH_ASKPASS
        }
        // Prevent git itself from trying to prompt on a non-existent terminal
        cmd.env("GIT_TERMINAL_PROMPT", "0");

        let output = cmd.output();

        match output {
            Ok(o) => {
                let success = o.status.success();
                let result = GitCommandResult {
                    success,
                    stdout: String::from_utf8_lossy(&o.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&o.stderr).to_string(),
                    exit_code: o.status.code().unwrap_or(-1),
                };
                if success {
                    state_arc.invalidate_repo_caches(&path_clone);
                }
                result
            }
            Err(e) => GitCommandResult {
                success: false,
                stdout: String::new(),
                stderr: format!("Failed to execute git: {e}"),
                exit_code: -1,
            },
        }
    })
    .await
    .map_err(|e| format!("Git command task failed: {e}"))
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

    // --- parse_git_config_remote_url unit tests ---

    #[test]
    fn test_parse_config_remote_url_basic() {
        let config = r#"
[core]
	repositoryformatversion = 0
[remote "origin"]
	url = git@github.com:owner/repo.git
	fetch = +refs/heads/*:refs/remotes/origin/*
[branch "main"]
	remote = origin
"#;
        assert_eq!(
            parse_git_config_remote_url(config, "origin"),
            Some("git@github.com:owner/repo.git".to_string())
        );
    }

    #[test]
    fn test_parse_config_remote_url_https() {
        let config = "[remote \"origin\"]\n\turl = https://github.com/owner/repo.git\n";
        assert_eq!(
            parse_git_config_remote_url(config, "origin"),
            Some("https://github.com/owner/repo.git".to_string())
        );
    }

    #[test]
    fn test_parse_config_remote_url_no_origin() {
        let config = "[remote \"upstream\"]\n\turl = git@github.com:other/repo.git\n";
        assert_eq!(parse_git_config_remote_url(config, "origin"), None);
    }

    #[test]
    fn test_parse_config_remote_url_multiple_remotes() {
        let config = r#"
[remote "upstream"]
	url = git@github.com:upstream/repo.git
[remote "origin"]
	url = git@github.com:fork/repo.git
"#;
        assert_eq!(
            parse_git_config_remote_url(config, "origin"),
            Some("git@github.com:fork/repo.git".to_string())
        );
    }

    #[test]
    fn test_parse_config_remote_url_spaces_around_equals() {
        let config = "[remote \"origin\"]\n\turl   =   git@github.com:owner/repo.git  \n";
        assert_eq!(
            parse_git_config_remote_url(config, "origin"),
            Some("git@github.com:owner/repo.git".to_string())
        );
    }

    #[test]
    fn test_parse_config_empty() {
        assert_eq!(parse_git_config_remote_url("", "origin"), None);
    }

    // --- Integration tests: compare file I/O vs git subprocess ---
    // These run against the actual tuicommander repo to validate correctness.

    #[test]
    fn test_read_branch_matches_git_rev_parse() {
        // Find the repo root (this file lives in src-tauri/src/)
        let manifest_dir = env!("CARGO_MANIFEST_DIR"); // src-tauri/
        let repo_root = PathBuf::from(manifest_dir).parent().unwrap().to_path_buf();

        // File I/O approach
        let file_branch = read_branch_from_head(&repo_root);

        // Subprocess approach (ground truth)
        let git_branch = Command::new(crate::agent::resolve_cli("git"))
            .current_dir(&repo_root)
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    let b = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    if b == "HEAD" { None } else { Some(b) }
                } else {
                    None
                }
            });

        assert_eq!(file_branch, git_branch,
            "read_branch_from_head() must match `git rev-parse --abbrev-ref HEAD`");
    }

    #[test]
    fn test_read_remote_url_matches_git_remote() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let repo_root = PathBuf::from(manifest_dir).parent().unwrap().to_path_buf();

        // File I/O approach
        let file_url = read_remote_url(&repo_root);

        // Subprocess approach (ground truth)
        let git_url = Command::new(crate::agent::resolve_cli("git"))
            .current_dir(&repo_root)
            .args(["remote", "get-url", "origin"])
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                } else {
                    None
                }
            });

        assert_eq!(file_url, git_url,
            "read_remote_url() must match `git remote get-url origin`");
    }

    #[test]
    fn test_resolve_git_dir_for_local_repo() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let repo_root = PathBuf::from(manifest_dir).parent().unwrap().to_path_buf();

        let git_dir = resolve_git_dir(&repo_root);
        assert!(git_dir.is_some(), "Should resolve .git dir for this repo");
        assert!(git_dir.unwrap().join("HEAD").exists(), ".git dir should contain HEAD");
    }

    #[test]
    fn test_get_merged_branches_against_real_repo() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let repo_root = PathBuf::from(manifest_dir).parent().unwrap().to_path_buf();

        let merged = get_merged_branches_impl(&repo_root)
            .expect("get_merged_branches_impl should succeed on real repo");

        // The main branch itself should always appear in its own --merged list
        let has_main = merged.iter().any(|b| MAIN_BRANCH_CANDIDATES.contains(&b.as_str()));
        assert!(has_main, "at least one main branch candidate should be in the merged list, got: {merged:?}");

        // On main, the current branch must be in the list; on a feature branch it may not be
        if let Some(current) = read_branch_from_head(&repo_root) {
            if is_main_branch(&current) {
                assert!(merged.contains(&current), "main branch '{current}' should be in its own merged list");
            }
        }
        // Detached HEAD: the merged list is still non-empty (at minimum the main branch itself)
    }

    #[test]
    fn test_get_merged_branches_returns_empty_for_nonexistent_path() {
        let result = get_merged_branches_impl(Path::new("/nonexistent/path/xyz"));
        assert_eq!(result.unwrap(), Vec::<String>::new(), "should return empty vec for nonexistent path");
    }

    #[test]
    fn test_get_merged_branches_returns_empty_for_non_git_directory() {
        let result = get_merged_branches_impl(&std::env::temp_dir());
        assert_eq!(result.unwrap(), Vec::<String>::new(), "should return empty vec for non-git directory");
    }

    #[test]
    fn test_detect_default_branch_for_real_repo() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let repo_root = PathBuf::from(manifest_dir).parent().unwrap().to_path_buf();
        let git_dir = resolve_git_dir(&repo_root).expect("should resolve git dir");

        let branch = detect_default_branch(&git_dir);
        assert!(branch.is_some(), "should detect a default branch for this repo");
        // This repo uses 'main'
        assert_eq!(branch.unwrap(), "main");
    }

    #[test]
    fn test_detect_default_branch_returns_none_for_non_git() {
        let tmp = std::env::temp_dir();
        // temp_dir has no .git — resolve_git_dir returns None, so we
        // test detect_default_branch with a fake path that has no refs
        assert!(detect_default_branch(&tmp).is_none());
    }

    #[test]
    fn get_file_diff_rejects_path_traversal() {
        // Use this repo's own path as a valid git repo
        let repo_path = std::env::current_dir().unwrap();
        let result = get_file_diff(
            repo_path.to_string_lossy().to_string(),
            "../../etc/passwd".to_string(),
            None,
        );
        let err = result.unwrap_err();
        assert!(
            err.contains("outside repository") || err.contains("Failed to resolve"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn null_device_constant_is_correct() {
        #[cfg(not(windows))]
        assert_eq!(NULL_DEVICE, "/dev/null");
        #[cfg(windows)]
        assert_eq!(NULL_DEVICE, "NUL");
    }
}
