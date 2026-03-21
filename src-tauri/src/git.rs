use serde::Serialize;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::State;

use crate::git_cli::git_cmd;
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
    let status = git_cmd(&repo_path)
        .args(["status", "--porcelain"])
        .run_silent()
        .map(|o| {
            if o.stdout.is_empty() {
                "clean".to_string()
            } else if o.stdout.contains("UU") || o.stdout.contains("AA") || o.stdout.contains("DD") {
                "conflict".to_string()
            } else {
                "dirty".to_string()
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
    if let Some(cached) = AppState::get_cached(&state.git_cache.repo_info, &path, GIT_CACHE_TTL) {
        return cached;
    }

    let info = get_repo_info_impl(&path);
    AppState::set_cached(&state.git_cache.repo_info, path, info.clone());
    info
}

/// Get the origin remote URL for a repository (returns None if not a git repo or no remote).
#[tauri::command]
pub(crate) fn get_remote_url(path: String) -> Option<String> {
    read_remote_url(Path::new(&path))
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
    match git_cmd(&repo_path).args(["branch", "-m", old_name, new_name]).run() {
        Ok(_) => Ok(()),
        Err(crate::git_cli::GitError::NonZeroExit { stderr, .. }) => {
            if stderr.contains("not found") || stderr.contains("does not exist") {
                Err(format!("Branch '{old_name}' does not exist"))
            } else if stderr.contains("already exists") {
                Err(format!("Branch '{new_name}' already exists"))
            } else {
                Err(format!("git branch rename failed: {stderr}"))
            }
        }
        Err(e) => Err(e.to_string()),
    }
}

/// Rename a git branch (Tauri command with cache invalidation)
#[tauri::command]
pub(crate) fn rename_branch(state: State<'_, Arc<AppState>>, path: String, old_name: String, new_name: String) -> Result<(), String> {
    rename_branch_impl(&path, &old_name, &new_name)?;
    state.invalidate_repo_caches(&path);
    Ok(())
}

/// Core logic for creating a git branch.
pub(crate) fn create_branch_impl(path: &str, name: &str, start_point: Option<&str>, checkout: bool) -> Result<(), String> {
    let repo_path = PathBuf::from(path);

    // Validate branch name (same rules as rename_branch_impl)
    if name.is_empty() {
        return Err("Branch name cannot be empty".to_string());
    }
    if name.contains(' ') {
        return Err("Branch name cannot contain spaces".to_string());
    }
    if name.starts_with('-') {
        return Err("Branch name cannot start with a hyphen".to_string());
    }
    if name.contains("..") {
        return Err("Branch name cannot contain '..'".to_string());
    }
    if name.ends_with(".lock") {
        return Err("Branch name cannot end with '.lock'".to_string());
    }

    // Build `git branch <name> [<start_point>]`
    let mut args = vec!["branch", name];
    if let Some(sp) = start_point {
        args.push(sp);
    }

    match git_cmd(&repo_path).args(&args).run() {
        Ok(_) => {}
        Err(crate::git_cli::GitError::NonZeroExit { stderr, .. }) => {
            if stderr.contains("already exists") {
                return Err(format!("Branch '{name}' already exists"));
            } else {
                return Err(format!("git branch failed: {stderr}"));
            }
        }
        Err(e) => return Err(e.to_string()),
    }

    if checkout {
        match git_cmd(&repo_path).args(["checkout", name]).run() {
            Ok(_) => {}
            Err(crate::git_cli::GitError::NonZeroExit { stderr, .. }) => {
                return Err(format!("git checkout failed: {stderr}"));
            }
            Err(e) => return Err(e.to_string()),
        }
    }

    Ok(())
}

/// Create a git branch (Tauri command with cache invalidation)
#[tauri::command]
pub(crate) fn create_branch(state: State<'_, Arc<AppState>>, path: String, name: String, start_point: Option<String>, checkout: bool) -> Result<(), String> {
    create_branch_impl(&path, &name, start_point.as_deref(), checkout)?;
    state.invalidate_repo_caches(&path);
    Ok(())
}

/// Result of a branch deletion operation.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct DeleteBranchResult {
    pub deleted: bool,
    pub branch: String,
    pub was_force: bool,
}

/// Core logic for deleting a git branch.
///
/// Refuses to delete protected main branches or the currently checked-out branch.
/// Use `force=true` to delete branches with unmerged commits (`git branch -D`).
pub(crate) fn delete_branch_impl(path: &str, name: &str, force: bool) -> Result<DeleteBranchResult, String> {
    let repo_path = PathBuf::from(path);

    if name.is_empty() {
        return Err("Branch name cannot be empty".to_string());
    }

    // Refuse to delete main/primary branches
    if is_main_branch(name) {
        return Err(format!("Cannot delete protected branch '{name}'"));
    }

    // Refuse to delete the currently checked-out branch
    if let Some(current) = read_branch_from_head(&repo_path) {
        if current == name {
            return Err(format!("Cannot delete the currently checked-out branch '{name}'"));
        }
    }

    let flag = if force { "-D" } else { "-d" };
    match git_cmd(&repo_path).args(["branch", flag, name]).run() {
        Ok(_) => Ok(DeleteBranchResult {
            deleted: true,
            branch: name.to_string(),
            was_force: force,
        }),
        Err(crate::git_cli::GitError::NonZeroExit { stderr, .. }) => {
            Err(format!("git branch delete failed: {stderr}"))
        }
        Err(e) => Err(e.to_string()),
    }
}

/// Delete a git branch (Tauri command with cache invalidation)
#[tauri::command]
pub(crate) fn delete_branch(state: State<'_, Arc<AppState>>, path: String, name: String, force: bool) -> Result<DeleteBranchResult, String> {
    let result = delete_branch_impl(&path, &name, force)?;
    state.invalidate_repo_caches(&path);
    Ok(result)
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

    let out = git_cmd(&repo_path)
        .args(["log", "--format=%H%x00%h%x00%s", "-n", &n])
        .run()
        .map_err(|e| format!("git log failed: {e}"))?;

    let commits = out.stdout
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
fn diff_base_args(scope: &Option<String>) -> Result<Vec<String>, String> {
    match scope.as_deref() {
        Some("staged") => Ok(vec!["diff".into(), "--cached".into()]),
        Some(hash) if !hash.is_empty() => {
            validate_git_hash(hash)?;
            Ok(vec!["diff".into(), format!("{hash}^"), hash.into()])
        }
        _ => Ok(vec!["diff".into()]),
    }
}

/// Get git diff for a repository
#[tauri::command]
pub(crate) fn get_git_diff(path: String, scope: Option<String>) -> Result<String, String> {
    let repo_path = PathBuf::from(&path);

    let mut args = diff_base_args(&scope)?;
    args.push("--color=never".into());

    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let out = git_cmd(&repo_path)
        .args(&args_str)
        .run()
        .map_err(|e| format!("git diff failed: {e}"))?;

    Ok(out.stdout)
}

/// Get diff stats (additions/deletions) for a repository
#[tauri::command]
pub(crate) fn get_diff_stats(path: String, scope: Option<String>) -> DiffStats {
    let repo_path = PathBuf::from(&path);

    let args = match diff_base_args(&scope) {
        Ok(a) => a,
        Err(_) => return DiffStats { additions: 0, deletions: 0 },
    };
    let mut args = args;
    args.push("--shortstat".into());

    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    if let Some(out) = git_cmd(&repo_path).args(&args_str).run_silent() {
        // Parse: "1 file changed, 10 insertions(+), 5 deletions(-)"
        let mut additions = 0;
        let mut deletions = 0;

        for part in out.stdout.split(',') {
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
    let mut status_args = diff_base_args(&scope)?;
    status_args.push("--name-status".into());

    let status_args_str: Vec<&str> = status_args.iter().map(|s| s.as_str()).collect();
    let status_out = git_cmd(&repo_path)
        .args(&status_args_str)
        .run()
        .map_err(|e| format!("git diff --name-status failed: {e}"))?;

    // Get per-file stats (additions/deletions)
    let mut stats_args = diff_base_args(&scope)?;
    stats_args.push("--numstat".into());

    let stats_args_str: Vec<&str> = stats_args.iter().map(|s| s.as_str()).collect();
    let stats_out = git_cmd(&repo_path)
        .args(&stats_args_str)
        .run()
        .map_err(|e| format!("git diff --numstat failed: {e}"))?;

    // Parse status output into map: filepath -> status
    let status_text = &status_out.stdout;
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
    let stats_text = &stats_out.stdout;
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
        let untracked_out = git_cmd(&repo_path)
            .args(["ls-files", "--others", "--exclude-standard"])
            .run_silent();

        if let Some(ref out) = untracked_out {
            for line in out.stdout.lines() {
                let file_path = line.trim();
                if file_path.is_empty() {
                    continue;
                }
                // Count lines by streaming (avoids loading large files into memory).
                // Stops on first UTF-8 error (binary file) and returns 0.
                let full_path = repo_path.join(file_path);
                let additions = File::open(&full_path)
                    .map(|f| {
                        let mut count = 0u32;
                        for line in BufReader::new(f).lines() {
                            match line {
                                Ok(_) => count += 1,
                                Err(_) => return 0, // binary / invalid UTF-8
                            }
                        }
                        count
                    })
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

/// Get diff for a single file.
/// When `untracked` is `Some(true)`, skip the `ls-files` probe and go directly
/// to `--no-index` diff (the frontend already knows the file status).
#[tauri::command]
pub(crate) fn get_file_diff(path: String, file: String, scope: Option<String>, untracked: Option<bool>) -> Result<String, String> {
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

        // If the frontend told us it's untracked, skip the subprocess probe.
        let is_untracked = if untracked == Some(true) {
            true
        } else {
            match git_cmd(&repo_path)
                .args(["ls-files", "--error-unmatch", &file])
                .run()
            {
                Ok(_) => false,
                Err(crate::git_cli::GitError::NonZeroExit { .. }) => true,
                Err(crate::git_cli::GitError::SpawnFailed(e)) => {
                    return Err(format!("Failed to check file tracking status: {e}"));
                }
            }
        };

        if is_untracked {
            let full_path_str = full_path.to_string_lossy();
            let raw = git_cmd(&repo_path)
                .args(["diff", "--color=never", "--no-index", "--", NULL_DEVICE, &full_path_str])
                .run_raw()
                .map_err(|e| format!("Failed to diff untracked file: {e}"))?;
            // --no-index exits with 1 when files differ (expected vs null device),
            // but exit code > 1 indicates an actual error.
            let code = raw.status.code().unwrap_or(-1);
            if code > 1 {
                let stderr = String::from_utf8_lossy(&raw.stderr);
                return Err(format!("git diff --no-index failed (exit {code}): {stderr}"));
            }
            return Ok(String::from_utf8_lossy(&raw.stdout).to_string());
        }
    }

    let mut args = diff_base_args(&scope)?;
    args.push("--color=never".into());
    args.push("--".into());
    args.push(file.clone());
    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

    let out = git_cmd(&repo_path)
        .args(&args_str)
        .run()
        .map_err(|e| format!("git diff failed for file {file}: {e}"))?;

    Ok(out.stdout)
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
/// Returns branch names whose tips are reachable from the main branch HEAD,
/// excluding branches whose tip is identical to main (never diverged).
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

    // Single command: get branch name + SHA together, plus main SHA for filtering
    let out = git_cmd(repo_path)
        .args(["branch", "--merged", &main_branch, "--format=%(objectname) %(refname:short)"])
        .run()
        .map_err(|e| format!("git branch --merged failed: {e}"))?;

    let main_sha = git_cmd(repo_path)
        .args(["rev-parse", &main_branch])
        .run()
        .map(|o| o.stdout.trim().to_string())
        .unwrap_or_default();

    // Filter out branches whose tip SHA matches main — they never diverged
    Ok(out.stdout.lines()
        .filter_map(|line| {
            let line = line.trim();
            let (sha, name) = line.split_once(' ')?;
            if name.is_empty() { return None; }
            // Exclude branches at the exact same SHA as main
            if !main_sha.is_empty() && sha == main_sha { return None; }
            Some(name.to_string())
        })
        .collect())
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
    if let Some(cached) = AppState::get_cached(&state.git_cache.merged_branches, &path, GIT_CACHE_TTL) {
        return Ok(cached);
    }

    let state_arc = state.inner().clone();
    let path_clone = path.clone();
    let result = tokio::task::spawn_blocking(move || {
        get_merged_branches_impl(Path::new(&path_clone))
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))??;
    AppState::set_cached(&state_arc.git_cache.merged_branches, path, result.clone());
    Ok(result)
}

/// Lightweight structural snapshot: worktree paths + merged branches.
/// Returns fast (two git subprocesses, no per-worktree diff stats).
#[derive(Serialize)]
pub(crate) struct RepoStructure {
    worktree_paths: HashMap<String, String>,
    merged_branches: Vec<String>,
}

/// Per-worktree diff stats + last-commit timestamps.
/// Expensive: runs N×`git diff --stat` + 1×`git for-each-ref`.
#[derive(Serialize)]
pub(crate) struct RepoDiffStats {
    diff_stats: HashMap<String, DiffStats>,
    last_commit_ts: HashMap<String, Option<i64>>,
}

/// Aggregate repo snapshot returned by `get_repo_summary`.
/// Collapses the N+2 IPC storm (get_worktree_paths + get_merged_branches + N×get_diff_stats)
/// into a single round-trip.
#[derive(Serialize)]
pub(crate) struct RepoSummary {
    worktree_paths: HashMap<String, String>,
    merged_branches: Vec<String>,
    /// Per-worktree diff stats, keyed by worktree path (matches keys of worktree_paths values).
    diff_stats: HashMap<String, DiffStats>,
    /// Unix timestamp of the last commit on each branch, keyed by branch name.
    last_commit_ts: HashMap<String, Option<i64>>,
}

/// Get the unix timestamp of the last commit on each branch using a single
/// `git for-each-ref` call instead of N sequential `git log` subprocesses.
fn get_last_commit_timestamps(
    repo_path: &Path,
    branches: &[String],
) -> HashMap<String, Option<i64>> {
    let mut result: HashMap<String, Option<i64>> = branches
        .iter()
        .map(|b| (b.clone(), None))
        .collect();

    let out = match git_cmd(repo_path)
        .args(["for-each-ref", "--format=%(refname:short)\t%(creatordate:unix)", "refs/heads/"])
        .run()
    {
        Ok(out) => out,
        Err(_) => return result,
    };

    for line in out.stdout.lines() {
        if let Some((name, ts_str)) = line.split_once('\t')
            && let Some(entry) = result.get_mut(name)
        {
            *entry = ts_str.parse::<i64>().ok();
        }
    }

    result
}

/// Core implementation of get_repo_summary, callable from both Tauri command and HTTP route.
/// Runs worktree_paths + merged_branches concurrently, then diff stats for each path concurrently.
pub(crate) async fn get_repo_summary_impl(state: &AppState, repo_path: String) -> Result<RepoSummary, String> {
    // Spawn worktree_paths concurrently while we fetch/check merged_branches cache.
    let wt_path = repo_path.clone();
    let worktree_handle = tokio::task::spawn_blocking(move || {
        crate::worktree::get_worktree_paths(wt_path)
    });

    let merged_branches = if let Some(cached) = AppState::get_cached(&state.git_cache.merged_branches, &repo_path, GIT_CACHE_TTL) {
        cached
    } else {
        let mb_path = repo_path.clone();
        let branches = tokio::task::spawn_blocking(move || {
            get_merged_branches_impl(Path::new(&mb_path))
        })
        .await
        .map_err(|e| format!("spawn_blocking error: {e}"))??;
        AppState::set_cached(&state.git_cache.merged_branches, repo_path.clone(), branches.clone());
        branches
    };

    let worktree_paths = worktree_handle
        .await
        .map_err(|e| format!("spawn_blocking error: {e}"))?
        .map_err(|e| format!("get_worktree_paths failed: {e}"))?;

    // Run diff stats and last-commit timestamps concurrently.
    let paths: Vec<String> = worktree_paths.values().cloned().collect();
    let mut diff_handles = Vec::with_capacity(paths.len());
    for path in paths {
        diff_handles.push(tokio::task::spawn_blocking(move || {
            let stats = get_diff_stats(path.clone(), None);
            (path, stats)
        }));
    }

    let branch_names: Vec<String> = worktree_paths.keys().cloned().collect();
    let ts_repo_path = repo_path.clone();
    let ts_handle = tokio::task::spawn_blocking(move || {
        get_last_commit_timestamps(Path::new(&ts_repo_path), &branch_names)
    });

    let mut diff_stats = HashMap::new();
    for handle in diff_handles {
        let (path, stats) = handle
            .await
            .map_err(|e| format!("spawn_blocking error: {e}"))?;
        diff_stats.insert(path, stats);
    }

    let last_commit_ts = ts_handle
        .await
        .map_err(|e| format!("spawn_blocking error: {e}"))?;

    Ok(RepoSummary { worktree_paths, merged_branches, diff_stats, last_commit_ts })
}

/// Single IPC replacement for the N+2 calls in refreshAllBranchStats.
#[tauri::command]
pub(crate) async fn get_repo_summary(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
) -> Result<RepoSummary, String> {
    get_repo_summary_impl(&state, repo_path).await
}

/// Fast structural snapshot: worktree paths + merged branches only.
/// Used by progressive loading Phase 1 — returns before expensive diff stats.
pub(crate) async fn get_repo_structure_impl(state: &AppState, repo_path: String) -> Result<RepoStructure, String> {
    let wt_path = repo_path.clone();
    let worktree_handle = tokio::task::spawn_blocking(move || {
        crate::worktree::get_worktree_paths(wt_path)
    });

    let merged_branches = if let Some(cached) = AppState::get_cached(&state.git_cache.merged_branches, &repo_path, GIT_CACHE_TTL) {
        cached
    } else {
        let mb_path = repo_path.clone();
        let branches = tokio::task::spawn_blocking(move || {
            get_merged_branches_impl(Path::new(&mb_path))
        })
        .await
        .map_err(|e| format!("spawn_blocking error: {e}"))??;
        AppState::set_cached(&state.git_cache.merged_branches, repo_path.clone(), branches.clone());
        branches
    };

    let worktree_paths = worktree_handle
        .await
        .map_err(|e| format!("spawn_blocking error: {e}"))?
        .map_err(|e| format!("get_worktree_paths failed: {e}"))?;

    Ok(RepoStructure { worktree_paths, merged_branches })
}

#[tauri::command]
pub(crate) async fn get_repo_structure(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
) -> Result<RepoStructure, String> {
    get_repo_structure_impl(&state, repo_path).await
}

/// Per-worktree diff stats + last-commit timestamps.
/// Used by progressive loading Phase 2 — runs after structure is already displayed.
pub(crate) async fn get_repo_diff_stats_impl(_state: &AppState, repo_path: String) -> Result<RepoDiffStats, String> {
    // Need worktree paths to know which directories to diff
    let wt_path = repo_path.clone();
    let worktree_paths = tokio::task::spawn_blocking(move || {
        crate::worktree::get_worktree_paths(wt_path)
    })
    .await
    .map_err(|e| format!("spawn_blocking error: {e}"))?
    .map_err(|e| format!("get_worktree_paths failed: {e}"))?;

    let paths: Vec<String> = worktree_paths.values().cloned().collect();
    let mut diff_handles = Vec::with_capacity(paths.len());
    for path in paths {
        diff_handles.push(tokio::task::spawn_blocking(move || {
            let stats = get_diff_stats(path.clone(), None);
            (path, stats)
        }));
    }

    let branch_names: Vec<String> = worktree_paths.keys().cloned().collect();
    let ts_repo_path = repo_path.clone();
    let ts_handle = tokio::task::spawn_blocking(move || {
        get_last_commit_timestamps(Path::new(&ts_repo_path), &branch_names)
    });

    let mut diff_stats = HashMap::new();
    for handle in diff_handles {
        let (path, stats) = handle
            .await
            .map_err(|e| format!("spawn_blocking error: {e}"))?;
        diff_stats.insert(path, stats);
    }

    let last_commit_ts = ts_handle
        .await
        .map_err(|e| format!("spawn_blocking error: {e}"))?;

    Ok(RepoDiffStats { diff_stats, last_commit_ts })
}

#[tauri::command]
pub(crate) async fn get_repo_diff_stats(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
) -> Result<RepoDiffStats, String> {
    get_repo_diff_stats_impl(&state, repo_path).await
}

/// Get git branches for a repository (Story 052)
#[tauri::command]
pub(crate) fn get_git_branches(path: String) -> Result<Vec<serde_json::Value>, String> {
    let repo_path = PathBuf::from(&path);

    let out = git_cmd(&repo_path)
        .args(["branch", "-a", "--format=%(refname:short) %(HEAD)"])
        .run()
        .map_err(|e| format!("git branch failed: {e}"))?;

    let mut branches: Vec<serde_json::Value> = out.stdout
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

/// Rich per-branch information returned by `get_branches_detail`.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct BranchDetail {
    pub name: String,
    pub is_current: bool,
    pub is_remote: bool,
    pub is_main: bool,
    pub is_merged: bool,
    pub ahead: Option<u32>,
    pub behind: Option<u32>,
    pub upstream: Option<String>,
    pub last_commit_date: Option<String>,     // ISO 8601
    pub last_commit_message: Option<String>,  // first line only
    pub last_commit_author: Option<String>,
}

/// Field separator used in `git for-each-ref` output.
/// Must not appear in branch names, commit messages, author names, or tracking info.
const BRANCH_FIELD_SEP: &str = "|||";

/// Core logic for fetching rich branch details (no Tauri state).
///
/// Uses `git for-each-ref` for a single-pass data collection across all local and
/// remote branches. Fields are delimited by `|||` which cannot appear in any of
/// the output fields (branch names, subjects, author names, or tracking tokens).
pub(crate) fn get_branches_detail_impl(path: &Path) -> Result<Vec<BranchDetail>, String> {
    // Each ref line: refname|||HEAD|||upstream:short|||upstream:track|||committerdate|||subject|||authorname
    let sep = BRANCH_FIELD_SEP;
    let fmt = format!(
        "%(refname:short){sep}%(HEAD){sep}%(upstream:short){sep}%(upstream:track){sep}%(committerdate:iso8601){sep}%(subject){sep}%(authorname)"
    );

    let out = git_cmd(path)
        .args(["for-each-ref", &format!("--format={fmt}"), "refs/heads/", "refs/remotes/"])
        .run()
        .map_err(|e| format!("git for-each-ref failed: {e}"))?;

    // Collect merged branch names once so we can do O(1) lookups.
    let merged_set: std::collections::HashSet<String> =
        get_merged_branches_impl(path).unwrap_or_default().into_iter().collect();

    let mut branches: Vec<BranchDetail> = out.stdout
        .lines()
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let parts: Vec<&str> = line.splitn(7, BRANCH_FIELD_SEP).collect();
            if parts.len() < 7 { return None; }

            let name = parts[0].trim().to_string();

            // Skip the synthetic origin/HEAD pointer
            if name == "origin/HEAD" || name.ends_with("/HEAD") { return None; }

            let is_current = parts[1].trim() == "*";
            let is_remote = name.contains('/');

            let upstream_raw = parts[2].trim();
            let upstream = if upstream_raw.is_empty() { None } else { Some(upstream_raw.to_string()) };

            // upstream:track looks like "[ahead 2, behind 3]", "[ahead 1]", "[behind 4]", or ""
            let track = parts[3].trim();
            let ahead = parse_track_value(track, "ahead");
            let behind = parse_track_value(track, "behind");

            let commit_date_raw = parts[4].trim();
            let last_commit_date = if commit_date_raw.is_empty() { None } else { Some(commit_date_raw.to_string()) };

            let subject = parts[5].trim();
            let last_commit_message = if subject.is_empty() { None } else { Some(subject.to_string()) };

            let author = parts[6].trim();
            let last_commit_author = if author.is_empty() { None } else { Some(author.to_string()) };

            let is_merged = merged_set.contains(&name);

            Some(BranchDetail {
                is_main: is_main_branch(&name),
                name,
                is_current,
                is_remote,
                is_merged,
                ahead,
                behind,
                upstream,
                last_commit_date,
                last_commit_message,
                last_commit_author,
            })
        })
        .collect();

    // Sort: main/primary branches first, then alphabetical
    branches.sort_by(|a, b| {
        match (a.is_main, b.is_main) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.cmp(&b.name),
        }
    });

    Ok(branches)
}

/// Parse `ahead` or `behind` count from a `%(upstream:track)` string.
///
/// The format is `[ahead N]`, `[behind N]`, or `[ahead N, behind M]`.
fn parse_track_value(track: &str, key: &str) -> Option<u32> {
    // Find "ahead N" or "behind N" within the brackets
    let search = format!("{key} ");
    let start = track.find(&search)? + search.len();
    let rest = &track[start..];
    // Number ends at the next comma, closing bracket, or end of string
    let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// Get rich branch details for a repository.
#[tauri::command]
pub(crate) fn get_branches_detail(path: String) -> Result<Vec<BranchDetail>, String> {
    get_branches_detail_impl(Path::new(&path))
}

/// Core logic for fetching recently checked-out branch names from the reflog.
///
/// Parses `git reflog show --format='%gs' -n 100`, extracts the target branch
/// from each `checkout: moving from X to Y` line, deduplicates preserving
/// order (most recent first), and returns up to `limit` entries.
pub(crate) fn get_recent_branches_impl(path: &Path, limit: usize) -> Result<Vec<String>, String> {
    let output = git_cmd(path)
        .args(["reflog", "show", "--format=%gs", "-n", "100"])
        .run()
        .map_err(|e| e.to_string())?;

    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();

    for line in output.stdout.lines() {
        // Match "checkout: moving from <from> to <to>"
        if let Some(rest) = line.strip_prefix("checkout: moving from ") {
            if let Some(to_pos) = rest.rfind(" to ") {
                let target = &rest[to_pos + 4..];
                let target = target.trim().to_string();
                if !target.is_empty() && seen.insert(target.clone()) {
                    result.push(target);
                    if result.len() >= limit {
                        break;
                    }
                }
            }
        }
    }

    Ok(result)
}

/// Get recently checked-out branch names for a repository (most recent first).
#[tauri::command]
pub(crate) fn get_recent_branches(path: String, limit: Option<usize>) -> Result<Vec<String>, String> {
    get_recent_branches_impl(Path::new(&path), limit.unwrap_or(5))
}

/// Rich context for the Git Operations Panel (single IPC round-trip).
#[derive(Debug, Clone, Serialize)]
pub(crate) struct GitPanelContext {
    pub branch: String,
    pub is_detached: bool,
    pub status: String, // "clean", "dirty", "conflict"
    pub ahead: Option<u32>,
    pub behind: Option<u32>,
    pub staged_count: u32,
    pub changed_count: u32,
    pub stash_count: u32,
    pub last_commit: Option<RecentCommit>,
    pub in_rebase: bool,
    pub in_cherry_pick: bool,
}

/// Core logic for fetching git panel context (no caching, no Tauri state).
pub(crate) fn get_git_panel_context_impl(path: &Path) -> GitPanelContext {
    let git_dir = resolve_git_dir(path);

    // Branch & detached state
    let head_branch = read_branch_from_head(path);
    let is_detached = head_branch.is_none();
    let branch = head_branch.unwrap_or_else(|| {
        // Detached HEAD: show short hash
        git_cmd(path)
            .args(["rev-parse", "--short", "HEAD"])
            .run_silent()
            .map(|o| o.stdout.trim().to_string())
            .unwrap_or_default()
    });

    // Status (porcelain v2 for staged vs unstaged)
    let (status, staged_count, changed_count) = {
        let porcelain = git_cmd(path)
            .args(["status", "--porcelain=v2"])
            .run_silent()
            .map(|o| o.stdout)
            .unwrap_or_default();

        let mut staged = 0u32;
        let mut changed = 0u32;
        let mut has_conflict = false;

        for line in porcelain.lines() {
            if let Some(rest) = line.strip_prefix("1 ") {
                // Ordinary entry: "1 XY ..."
                let xy: Vec<char> = rest.chars().take(2).collect();
                if xy.len() == 2 {
                    if xy[0] != '.' {
                        staged += 1;
                    }
                    if xy[1] != '.' {
                        changed += 1;
                    }
                }
            } else if let Some(rest) = line.strip_prefix("2 ") {
                // Rename/copy entry: "2 XY ..."
                let xy: Vec<char> = rest.chars().take(2).collect();
                if xy.len() == 2 {
                    if xy[0] != '.' {
                        staged += 1;
                    }
                    if xy[1] != '.' {
                        changed += 1;
                    }
                }
            } else if line.starts_with("u ") {
                // Unmerged entry
                has_conflict = true;
                changed += 1;
            } else if line.starts_with("? ") {
                // Untracked
                changed += 1;
            }
        }

        let status_str = if has_conflict {
            "conflict"
        } else if staged > 0 || changed > 0 {
            "dirty"
        } else {
            "clean"
        };
        (status_str.to_string(), staged, changed)
    };

    // Ahead/behind (only when there's an upstream)
    let (ahead, behind) = if !is_detached {
        git_cmd(path)
            .args(["rev-list", "--left-right", "--count", &format!("{branch}...{branch}@{{u}}")])
            .run_silent()
            .and_then(|o| {
                let parts: Vec<&str> = o.stdout.trim().split('\t').collect();
                if parts.len() == 2 {
                    Some((
                        parts[0].parse::<u32>().ok(),
                        parts[1].parse::<u32>().ok(),
                    ))
                } else {
                    None
                }
            })
            .unwrap_or((None, None))
    } else {
        (None, None)
    };

    // Stash count
    let stash_count = git_cmd(path)
        .args(["stash", "list"])
        .run_silent()
        .map(|o| o.stdout.lines().count() as u32)
        .unwrap_or(0);

    // Last commit
    let last_commit = git_cmd(path)
        .args(["log", "--format=%H%x00%h%x00%s", "-n", "1"])
        .run_silent()
        .and_then(|o| {
            let parts: Vec<&str> = o.stdout.trim().splitn(3, '\0').collect();
            if parts.len() == 3 {
                Some(RecentCommit {
                    hash: parts[0].to_string(),
                    short_hash: parts[1].to_string(),
                    subject: parts[2].to_string(),
                })
            } else {
                None
            }
        });

    // Rebase / cherry-pick detection via .git directory markers
    let (in_rebase, in_cherry_pick) = match &git_dir {
        Some(gd) => (
            gd.join("rebase-merge").exists() || gd.join("rebase-apply").exists(),
            gd.join("CHERRY_PICK_HEAD").exists(),
        ),
        None => (false, false),
    };

    GitPanelContext {
        branch,
        is_detached,
        status,
        ahead,
        behind,
        staged_count,
        changed_count,
        stash_count,
        last_commit,
        in_rebase,
        in_cherry_pick,
    }
}

/// Get rich git panel context in a single IPC call (cached, 5s TTL).
#[tauri::command]
pub(crate) fn get_git_panel_context(
    state: State<'_, Arc<AppState>>,
    path: String,
) -> GitPanelContext {
    if let Some(cached) = AppState::get_cached(&state.git_cache.git_panel_context, &path, GIT_CACHE_TTL) {
        return cached;
    }

    let ctx = get_git_panel_context_impl(Path::new(&path));
    AppState::set_cached(&state.git_cache.git_panel_context, path, ctx.clone());
    ctx
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

        let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let mut builder = git_cmd(&repo_path).args(&args_str);

        // Enable GUI-based SSH authentication so passphrase-protected keys work
        // without a TTY. SSH_ASKPASS_REQUIRE=prefer tells SSH to use the askpass
        // program even when stdin looks like it could be a terminal.
        if let Some(ref askpass_path) = askpass {
            let askpass_str = askpass_path.to_string_lossy();
            builder = builder
                .env("SSH_ASKPASS", &askpass_str)
                .env("SSH_ASKPASS_REQUIRE", "prefer")
                .env("DISPLAY", ":0"); // Required on Linux for SSH_ASKPASS
        }

        match builder.run_raw() {
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

// --- Working tree status (porcelain v2) ---

/// A single staged or unstaged file entry.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct StatusEntry {
    pub path: String,
    /// Status code: "M", "A", "D", "R", etc.
    pub status: String,
    /// Original path for renames/copies.
    pub original_path: Option<String>,
    /// Lines added (from --numstat). 0 for binary or unknown.
    pub additions: u32,
    /// Lines deleted (from --numstat). 0 for binary or unknown.
    pub deletions: u32,
}

/// Full working tree status parsed from `git status --porcelain=v2`.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct WorkingTreeStatus {
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub stash_count: u32,
    pub staged: Vec<StatusEntry>,
    pub unstaged: Vec<StatusEntry>,
    pub untracked: Vec<String>,
}

/// Parse porcelain v2 output into a `WorkingTreeStatus`.
pub(crate) fn parse_porcelain_v2(output: &str) -> WorkingTreeStatus {
    let mut branch: Option<String> = None;
    let mut upstream: Option<String> = None;
    let mut ahead: u32 = 0;
    let mut behind: u32 = 0;
    let mut stash_count: u32 = 0;
    let mut staged = Vec::new();
    let mut unstaged = Vec::new();
    let mut untracked = Vec::new();

    for line in output.lines() {
        if let Some(rest) = line.strip_prefix("# branch.head ") {
            branch = if rest == "(detached)" { None } else { Some(rest.to_string()) };
        } else if let Some(rest) = line.strip_prefix("# branch.upstream ") {
            upstream = Some(rest.to_string());
        } else if let Some(rest) = line.strip_prefix("# branch.ab ") {
            // Format: "+N -M"
            for part in rest.split_whitespace() {
                if let Some(n) = part.strip_prefix('+') {
                    ahead = n.parse().unwrap_or(0);
                } else if let Some(n) = part.strip_prefix('-') {
                    behind = n.parse().unwrap_or(0);
                }
            }
        } else if let Some(rest) = line.strip_prefix("# stash ") {
            stash_count = rest.parse().unwrap_or(0);
        } else if let Some(rest) = line.strip_prefix("1 ") {
            // Ordinary changed entry: 1 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <path>
            parse_ordinary_entry(rest, &mut staged, &mut unstaged);
        } else if let Some(rest) = line.strip_prefix("2 ") {
            // Renamed/copied entry: 2 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <X><score> <path>\t<origPath>
            parse_rename_entry(rest, &mut staged, &mut unstaged);
        } else if let Some(rest) = line.strip_prefix("? ") {
            untracked.push(rest.to_string());
        }
        // We ignore "u " (unmerged) entries for now — they are conflict markers
    }

    WorkingTreeStatus { branch, upstream, ahead, behind, stash_count, staged, unstaged, untracked }
}

/// Map a porcelain v2 status character to a human-readable status code.
/// `.` means no change (returns None to skip), `?` is untracked.
fn status_char_to_code(c: char) -> Option<&'static str> {
    match c {
        'M' => Some("M"),
        'T' => Some("T"),
        'A' => Some("A"),
        'D' => Some("D"),
        'R' => Some("R"),
        'C' => Some("C"),
        _ => None, // '.' or unknown
    }
}

/// Parse an ordinary (type 1) porcelain v2 entry.
fn parse_ordinary_entry(rest: &str, staged: &mut Vec<StatusEntry>, unstaged: &mut Vec<StatusEntry>) {
    // Fields are space-separated: XY sub mH mI mW hH hI path
    // We need XY (index 0), sub (index 1), and path (index 7)
    let fields: Vec<&str> = rest.splitn(8, ' ').collect();
    if fields.len() < 8 { return; }
    // Skip submodule entries (sub field starts with 'S')
    if fields[1].starts_with('S') { return; }
    let xy = fields[0];
    let path = fields[7].to_string();
    let mut chars = xy.chars();
    let x = chars.next().unwrap_or('.');
    let y = chars.next().unwrap_or('.');
    if let Some(code) = status_char_to_code(x) {
        staged.push(StatusEntry { path: path.clone(), status: code.to_string(), original_path: None, additions: 0, deletions: 0 });
    }
    if let Some(code) = status_char_to_code(y) {
        unstaged.push(StatusEntry { path, status: code.to_string(), original_path: None, additions: 0, deletions: 0 });
    }
}

/// Parse a rename/copy (type 2) porcelain v2 entry.
fn parse_rename_entry(rest: &str, staged: &mut Vec<StatusEntry>, unstaged: &mut Vec<StatusEntry>) {
    // Fields: XY sub mH mI mW hH hI Xscore path\torigPath
    // 9 space-separated fields, but last contains tab-separated path pair
    let fields: Vec<&str> = rest.splitn(9, ' ').collect();
    if fields.len() < 9 { return; }
    let xy = fields[0];
    let path_part = fields[8]; // "newpath\torigpath"
    let (path, orig) = match path_part.split_once('\t') {
        Some((p, o)) => (p.to_string(), Some(o.to_string())),
        None => (path_part.to_string(), None),
    };
    let mut chars = xy.chars();
    let x = chars.next().unwrap_or('.');
    let y = chars.next().unwrap_or('.');
    if let Some(code) = status_char_to_code(x) {
        staged.push(StatusEntry { path: path.clone(), status: code.to_string(), original_path: orig.clone(), additions: 0, deletions: 0 });
    }
    if let Some(code) = status_char_to_code(y) {
        unstaged.push(StatusEntry { path, status: code.to_string(), original_path: orig, additions: 0, deletions: 0 });
    }
}

/// Parse `git diff --numstat` output into a path→(additions, deletions) map.
fn parse_numstat(output: &str) -> HashMap<String, (u32, u32)> {
    let mut map = HashMap::new();
    for line in output.lines() {
        let parts: Vec<&str> = line.splitn(3, '\t').collect();
        if parts.len() != 3 { continue; }
        // Binary files show "-" for additions/deletions
        let add = parts[0].parse::<u32>().unwrap_or(0);
        let del = parts[1].parse::<u32>().unwrap_or(0);
        map.insert(parts[2].to_string(), (add, del));
    }
    map
}

/// Enrich status entries with line counts from `git diff --numstat`.
pub(crate) fn enrich_with_numstat(repo_path: &Path, entries: &mut [StatusEntry], staged: bool) {
    let mut args = vec!["diff", "--numstat"];
    if staged {
        args.push("--cached");
    }
    let Ok(out) = git_cmd(repo_path).args(&args).run() else { return };
    let stats = parse_numstat(&out.stdout);
    for entry in entries.iter_mut() {
        if let Some(&(add, del)) = stats.get(&entry.path) {
            entry.additions = add;
            entry.deletions = del;
        }
    }
}

/// Get full working tree status from porcelain v2 output.
#[tauri::command]
pub(crate) async fn get_working_tree_status(path: String) -> Result<WorkingTreeStatus, String> {
    tokio::task::spawn_blocking(move || {
        let repo_path = PathBuf::from(&path);
        let out = git_cmd(&repo_path)
            .args(["status", "--porcelain=v2", "--branch", "--show-stash"])
            .run()
            .map_err(|e| format!("git status failed: {e}"))?;
        let mut status = parse_porcelain_v2(&out.stdout);
        enrich_with_numstat(&repo_path, &mut status.staged, true);
        enrich_with_numstat(&repo_path, &mut status.unstaged, false);
        Ok(status)
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

// --- Stage / unstage / discard ---

/// Validate that all file paths stay within the repo root.
/// Returns an error message if any path escapes.
fn validate_paths_within_repo(repo_path: &Path, files: &[String]) -> Result<(), String> {
    let canonical_repo = repo_path.canonicalize()
        .map_err(|e| format!("Failed to resolve repo path: {e}"))?;
    for file in files {
        // Reject absolute paths — all file args must be relative to repo root
        if Path::new(file).is_absolute() {
            return Err(format!("Access denied: absolute path '{}' not allowed", file));
        }
        let full = repo_path.join(file);
        // For files that don't exist yet (e.g. deleted), canonicalize will fail.
        // In that case, do a manual check: normalize the joined path and verify prefix.
        match full.canonicalize() {
            Ok(canonical) => {
                if !canonical.starts_with(&canonical_repo) {
                    return Err(format!("Access denied: path '{}' is outside repository", file));
                }
            }
            Err(_) => {
                // File doesn't exist on disk — normalize manually and verify prefix.
                // Always check, not only when ".." is present, to catch all traversal vectors.
                let mut components = Vec::new();
                for component in full.components() {
                    match component {
                        std::path::Component::ParentDir => {
                            if components.is_empty() {
                                return Err(format!("Access denied: path '{}' is outside repository", file));
                            }
                            components.pop();
                        }
                        std::path::Component::Normal(c) => components.push(c.to_os_string()),
                        std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                            components.clear();
                            components.push(component.as_os_str().to_os_string());
                        }
                        std::path::Component::CurDir => {}
                    }
                }
                let normalized: PathBuf = components.iter().collect();
                if !normalized.starts_with(&canonical_repo) {
                    return Err(format!("Access denied: path '{}' is outside repository", file));
                }
            }
        }
    }
    Ok(())
}

/// Stage files (`git add -- <files>`).
#[tauri::command]
pub(crate) fn git_stage_files(path: String, files: Vec<String>) -> Result<(), String> {
    let repo_path = PathBuf::from(&path);
    validate_paths_within_repo(&repo_path, &files)?;
    let mut args: Vec<String> = vec!["add".into(), "--".into()];
    args.extend(files);
    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    git_cmd(&repo_path)
        .args(&args_str)
        .run()
        .map_err(|e| format!("git add failed: {e}"))?;
    Ok(())
}

/// Unstage files (`git restore --staged -- <files>`).
#[tauri::command]
pub(crate) fn git_unstage_files(path: String, files: Vec<String>) -> Result<(), String> {
    let repo_path = PathBuf::from(&path);
    validate_paths_within_repo(&repo_path, &files)?;
    let mut args: Vec<String> = vec!["restore".into(), "--staged".into(), "--".into()];
    args.extend(files);
    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    git_cmd(&repo_path)
        .args(&args_str)
        .run()
        .map_err(|e| format!("git restore --staged failed: {e}"))?;
    Ok(())
}

/// Discard working tree changes (`git restore -- <files>`). Destructive!
#[tauri::command]
pub(crate) fn git_discard_files(path: String, files: Vec<String>) -> Result<(), String> {
    let repo_path = PathBuf::from(&path);
    validate_paths_within_repo(&repo_path, &files)?;
    let mut args: Vec<String> = vec!["restore".into(), "--".into()];
    args.extend(files);
    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    git_cmd(&repo_path)
        .args(&args_str)
        .run()
        .map_err(|e| format!("git restore failed: {e}"))?;
    Ok(())
}

// --- git commit ---

/// Commit staged changes and return the new commit hash.
#[tauri::command]
pub(crate) fn git_commit(path: String, message: String, amend: Option<bool>) -> Result<String, String> {
    let repo_path = PathBuf::from(&path);
    let mut args: Vec<String> = vec!["commit".into(), "-m".into(), message];
    if amend == Some(true) {
        args.push("--amend".into());
    }
    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    git_cmd(&repo_path)
        .args(&args_str)
        .run()
        .map_err(|e| format!("git commit failed: {e}"))?;

    // Read back the new commit hash
    let hash_out = git_cmd(&repo_path)
        .args(["rev-parse", "HEAD"])
        .run()
        .map_err(|e| format!("Failed to read commit hash: {e}"))?;
    Ok(hash_out.stdout.trim().to_string())
}

// --- Commit log, stash, file history, blame commands ---

/// A commit log entry with full metadata for the GitLens-style panel.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct CommitLogEntry {
    pub hash: String,
    pub parents: Vec<String>,
    pub refs: Vec<String>,
    pub author_name: String,
    pub author_date: String,
    pub subject: String,
}

/// Parse a NUL-delimited commit log line into a `CommitLogEntry`.
fn parse_commit_log_line(line: &str) -> Option<CommitLogEntry> {
    let parts: Vec<&str> = line.splitn(6, '\0').collect();
    if parts.len() != 6 {
        return None;
    }
    let parents = if parts[1].is_empty() {
        vec![]
    } else {
        parts[1].split(' ').map(|s| s.to_string()).collect()
    };
    let refs = if parts[2].is_empty() {
        vec![]
    } else {
        parts[2].split(", ").map(|s| s.trim().to_string()).collect()
    };
    Some(CommitLogEntry {
        hash: parts[0].to_string(),
        parents,
        refs,
        author_name: parts[3].to_string(),
        author_date: parts[4].to_string(),
        subject: parts[5].to_string(),
    })
}

const COMMIT_LOG_FORMAT: &str = "%H%x00%P%x00%D%x00%an%x00%aI%x00%s";
const COMMIT_LOG_MAX_COUNT: u32 = 500;
const COMMIT_LOG_DEFAULT_COUNT: u32 = 50;

/// Validate a git object hash (4-40 hex chars). Prevents injection via `after` parameters.
fn validate_git_hash(hash: &str) -> Result<(), String> {
    if hash.len() < 4 || hash.len() > 40 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("Invalid git hash: '{hash}'"));
    }
    Ok(())
}

/// Sync implementation of commit log retrieval.
pub(crate) fn get_commit_log_impl(path: String, count: Option<u32>, after: Option<String>) -> Result<Vec<CommitLogEntry>, String> {
    let repo_path = PathBuf::from(&path);
    let n = count.unwrap_or(COMMIT_LOG_DEFAULT_COUNT).min(COMMIT_LOG_MAX_COUNT);
    let n_str = n.to_string();

    let mut args = vec![
        "log".to_string(),
        "--topo-order".to_string(),
        "-n".to_string(),
        n_str,
        format!("--pretty=format:{COMMIT_LOG_FORMAT}"),
    ];

    if let Some(ref hash) = after {
        validate_git_hash(hash)?;
        args.push(hash.clone());
    }

    let out = git_cmd(&repo_path)
        .args(&args)
        .run()
        .map_err(|e| format!("git log failed: {e}"))?;

    let commits = out
        .stdout
        .lines()
        .filter_map(parse_commit_log_line)
        .collect();

    Ok(commits)
}

/// Get paginated commit log with full metadata.
#[tauri::command]
pub(crate) async fn get_commit_log(
    path: String,
    count: Option<u32>,
    after: Option<String>,
) -> Result<Vec<CommitLogEntry>, String> {
    tokio::task::spawn_blocking(move || get_commit_log_impl(path, count, after))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// A stash entry.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct StashEntry {
    pub index: u32,
    pub ref_name: String,
    pub message: String,
    pub hash: String,
}

/// List all stash entries.
#[tauri::command]
pub(crate) fn get_stash_list(path: String) -> Result<Vec<StashEntry>, String> {
    let repo_path = PathBuf::from(&path);

    let out = git_cmd(&repo_path)
        .args(["stash", "list", "--format=%gd%x00%s%x00%H"])
        .run_silent();

    let Some(out) = out else {
        // No stashes or not a git repo — return empty
        return Ok(vec![]);
    };

    if out.stdout.trim().is_empty() {
        return Ok(vec![]);
    }

    let entries = out
        .stdout
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.splitn(3, '\0').collect();
            if parts.len() != 3 {
                return None;
            }
            let ref_name = parts[0].to_string();
            // Parse index from "stash@{N}"
            let index = ref_name
                .strip_prefix("stash@{")
                .and_then(|s| s.strip_suffix('}'))
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0);
            Some(StashEntry {
                index,
                ref_name,
                message: parts[1].to_string(),
                hash: parts[2].to_string(),
            })
        })
        .collect();

    Ok(entries)
}

/// Validate a stash ref format (e.g. "stash@{0}").
fn validate_stash_ref(stash_ref: &str) -> Result<(), String> {
    if !stash_ref.starts_with("stash@{")
        || !stash_ref.ends_with('}')
        || stash_ref["stash@{".len()..stash_ref.len() - 1]
            .parse::<u32>()
            .is_err()
    {
        return Err(format!("Invalid stash ref: '{stash_ref}'"));
    }
    Ok(())
}

/// Apply a stash without removing it.
#[tauri::command]
pub(crate) fn git_stash_apply(path: String, stash_ref: String) -> Result<(), String> {
    let repo_path = PathBuf::from(&path);
    validate_stash_ref(&stash_ref)?;
    git_cmd(&repo_path)
        .args(["stash", "apply", &stash_ref])
        .run()
        .map_err(|e| format!("git stash apply failed: {e}"))?;
    Ok(())
}

/// Apply and remove a stash.
#[tauri::command]
pub(crate) fn git_stash_pop(path: String, stash_ref: String) -> Result<(), String> {
    let repo_path = PathBuf::from(&path);
    validate_stash_ref(&stash_ref)?;
    git_cmd(&repo_path)
        .args(["stash", "pop", &stash_ref])
        .run()
        .map_err(|e| format!("git stash pop failed: {e}"))?;
    Ok(())
}

/// Drop (delete) a stash.
#[tauri::command]
pub(crate) fn git_stash_drop(path: String, stash_ref: String) -> Result<(), String> {
    let repo_path = PathBuf::from(&path);
    validate_stash_ref(&stash_ref)?;
    git_cmd(&repo_path)
        .args(["stash", "drop", &stash_ref])
        .run()
        .map_err(|e| format!("git stash drop failed: {e}"))?;
    Ok(())
}

/// Show diff for a stash entry.
#[tauri::command]
pub(crate) fn git_stash_show(path: String, stash_ref: String) -> Result<String, String> {
    let repo_path = PathBuf::from(&path);
    validate_stash_ref(&stash_ref)?;
    let out = git_cmd(&repo_path)
        .args(["stash", "show", "-p", &stash_ref])
        .run()
        .map_err(|e| format!("git stash show failed: {e}"))?;
    Ok(out.stdout)
}

/// Get commit log for a specific file, following renames.
#[tauri::command]
pub(crate) async fn get_file_history(
    path: String,
    file: String,
    count: Option<u32>,
    after: Option<String>,
) -> Result<Vec<CommitLogEntry>, String> {
    tokio::task::spawn_blocking(move || {
        let repo_path = PathBuf::from(&path);
        validate_paths_within_repo(&repo_path, std::slice::from_ref(&file))?;
        let n = count.unwrap_or(COMMIT_LOG_DEFAULT_COUNT).min(COMMIT_LOG_MAX_COUNT);
        let n_str = n.to_string();

        let mut args = vec![
            "log".to_string(),
            "--follow".to_string(),
            "--topo-order".to_string(),
            "-n".to_string(),
            n_str,
            format!("--pretty=format:{COMMIT_LOG_FORMAT}"),
        ];

        if let Some(ref hash) = after {
            validate_git_hash(hash)?;
            args.push(hash.clone());
        }

        args.push("--".to_string());
        args.push(file);

        let out = git_cmd(&repo_path)
            .args(&args)
            .run()
            .map_err(|e| format!("git log failed: {e}"))?;

        let commits = out
            .stdout
            .lines()
            .filter_map(parse_commit_log_line)
            .collect();

        Ok(commits)
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// A single blame line with commit metadata.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct BlameLine {
    pub hash: String,
    pub author: String,
    pub author_time: i64,
    pub line_number: u32,
    pub content: String,
}

/// Parse `git blame --porcelain` output into `BlameLine` entries.
fn parse_blame_porcelain(output: &str) -> Vec<BlameLine> {
    let mut lines = Vec::new();
    let mut current_hash = String::new();
    let mut current_line_number: u32 = 0;

    // Cache commit metadata to avoid re-parsing for consecutive lines from same commit
    let mut commit_cache: HashMap<String, (String, i64)> = HashMap::new();

    let mut author = String::new();
    let mut author_time: i64 = 0;
    let mut expecting_hash = true; // true when the next non-header line should be a commit hash

    for line in output.lines() {
        if let Some(content) = line.strip_prefix('\t') {
            // Content line — finalize this blame entry
            let (cached_author, cached_time) = commit_cache
                .entry(current_hash.clone())
                .or_insert_with(|| (author.clone(), author_time));

            lines.push(BlameLine {
                hash: current_hash.clone(),
                author: cached_author.clone(),
                author_time: *cached_time,
                line_number: current_line_number,
                content: content.to_string(),
            });

            expecting_hash = true;
        } else if expecting_hash && line.len() >= 40 && line.as_bytes().iter().take(40).all(|b| b.is_ascii_hexdigit()) {
            // Hash line: "<hash> <orig_line> <final_line> [<num_lines>]"
            let parts: Vec<&str> = line.split(' ').collect();
            current_hash = parts[0].to_string();
            // final_line is the second or third number depending on whether this is the first
            // line of a group. In porcelain format, the line number we want is the "final line"
            // which is always the third field (index 2).
            current_line_number = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);

            if commit_cache.contains_key(&current_hash) {
                // Already cached — skip header lines until content
                expecting_hash = false;
            } else {
                // Need to parse headers
                author.clear();
                author_time = 0;
                expecting_hash = false;
            }
        } else if let Some(rest) = line.strip_prefix("author ") {
            author = rest.to_string();
        } else if let Some(rest) = line.strip_prefix("author-time ") {
            author_time = rest.parse().unwrap_or(0);
        }
    }

    lines
}

/// Get per-line blame information for a file.
#[tauri::command]
pub(crate) async fn get_file_blame(
    path: String,
    file: String,
) -> Result<Vec<BlameLine>, String> {
    tokio::task::spawn_blocking(move || {
        let repo_path = PathBuf::from(&path);
        validate_paths_within_repo(&repo_path, std::slice::from_ref(&file))?;

        let out = git_cmd(&repo_path)
            .args(["blame", "--porcelain", &file])
            .run()
            .map_err(|e| format!("git blame failed: {e}"))?;

        Ok(parse_blame_porcelain(&out.stdout))
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))?
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
        let git_branch = git_cmd(&repo_root)
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .run_silent()
            .and_then(|o| {
                let b = o.stdout.trim().to_string();
                if b == "HEAD" { None } else { Some(b) }
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
        let git_url = git_cmd(&repo_root)
            .args(["remote", "get-url", "origin"])
            .run_silent()
            .map(|o| o.stdout.trim().to_string());

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

        // The main branch should NOT appear — it has the same SHA as itself,
        // so the "never diverged" filter correctly excludes it.
        let has_main = merged.iter().any(|b| MAIN_BRANCH_CANDIDATES.contains(&b.as_str()));
        assert!(!has_main, "main branch should not appear in its own merged list, got: {merged:?}");

        // All returned branches should be truly merged (not main itself)
        for branch in &merged {
            assert!(!is_main_branch(branch), "main branch should not be in merged list");
        }
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
            None,
        );
        let err = result.unwrap_err();
        assert!(
            err.contains("outside repository") || err.contains("Failed to resolve"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn diff_base_args_none_returns_plain_diff() {
        let args = diff_base_args(&None).unwrap();
        assert_eq!(args, vec!["diff"]);
    }

    #[test]
    fn diff_base_args_staged_returns_cached() {
        let args = diff_base_args(&Some("staged".into())).unwrap();
        assert_eq!(args, vec!["diff", "--cached"]);
    }

    #[test]
    fn diff_base_args_commit_hash_returns_parent_diff() {
        let args = diff_base_args(&Some("abc123".into())).unwrap();
        assert_eq!(args, vec!["diff", "abc123^", "abc123"]);
    }

    #[test]
    fn null_device_constant_is_correct() {
        #[cfg(not(windows))]
        assert_eq!(NULL_DEVICE, "/dev/null");
        #[cfg(windows)]
        assert_eq!(NULL_DEVICE, "NUL");
    }

    // --- parse_porcelain_v2 unit tests ---

    #[test]
    fn parse_porcelain_v2_branch_info() {
        let output = "# branch.oid abc123\n# branch.head main\n# branch.upstream origin/main\n# branch.ab +3 -1\n";
        let status = parse_porcelain_v2(output);
        assert_eq!(status.branch, Some("main".to_string()));
        assert_eq!(status.upstream, Some("origin/main".to_string()));
        assert_eq!(status.ahead, 3);
        assert_eq!(status.behind, 1);
    }

    #[test]
    fn parse_porcelain_v2_detached_head() {
        let output = "# branch.oid abc123\n# branch.head (detached)\n";
        let status = parse_porcelain_v2(output);
        assert_eq!(status.branch, None);
    }

    #[test]
    fn parse_porcelain_v2_stash_count() {
        let output = "# branch.oid abc123\n# branch.head main\n# stash 5\n";
        let status = parse_porcelain_v2(output);
        assert_eq!(status.stash_count, 5);
    }

    #[test]
    fn parse_porcelain_v2_staged_modified() {
        // Ordinary entry: staged modification
        let output = "1 M. N... 100644 100644 100644 abc123 def456 src/main.rs\n";
        let status = parse_porcelain_v2(output);
        assert_eq!(status.staged.len(), 1);
        assert_eq!(status.staged[0].path, "src/main.rs");
        assert_eq!(status.staged[0].status, "M");
        assert!(status.unstaged.is_empty());
    }

    #[test]
    fn parse_porcelain_v2_unstaged_modified() {
        let output = "1 .M N... 100644 100644 100644 abc123 def456 src/lib.rs\n";
        let status = parse_porcelain_v2(output);
        assert!(status.staged.is_empty());
        assert_eq!(status.unstaged.len(), 1);
        assert_eq!(status.unstaged[0].path, "src/lib.rs");
        assert_eq!(status.unstaged[0].status, "M");
    }

    #[test]
    fn parse_porcelain_v2_both_staged_and_unstaged() {
        // File is partially staged (modified in both index and worktree)
        let output = "1 MM N... 100644 100644 100644 abc123 def456 src/both.rs\n";
        let status = parse_porcelain_v2(output);
        assert_eq!(status.staged.len(), 1);
        assert_eq!(status.unstaged.len(), 1);
        assert_eq!(status.staged[0].path, "src/both.rs");
        assert_eq!(status.unstaged[0].path, "src/both.rs");
    }

    #[test]
    fn parse_porcelain_v2_added_file() {
        let output = "1 A. N... 000000 100644 100644 0000000 abc123 new_file.rs\n";
        let status = parse_porcelain_v2(output);
        assert_eq!(status.staged.len(), 1);
        assert_eq!(status.staged[0].status, "A");
    }

    #[test]
    fn parse_porcelain_v2_deleted_file() {
        let output = "1 D. N... 100644 000000 000000 abc123 0000000 removed.rs\n";
        let status = parse_porcelain_v2(output);
        assert_eq!(status.staged.len(), 1);
        assert_eq!(status.staged[0].status, "D");
    }

    #[test]
    fn parse_porcelain_v2_untracked() {
        let output = "? new_untracked.txt\n? another.log\n";
        let status = parse_porcelain_v2(output);
        assert_eq!(status.untracked, vec!["new_untracked.txt", "another.log"]);
    }

    #[test]
    fn parse_porcelain_v2_rename_staged() {
        // Type 2 entry: rename in index
        let output = "2 R. N... 100644 100644 100644 abc123 def456 R100 new_name.rs\told_name.rs\n";
        let status = parse_porcelain_v2(output);
        assert_eq!(status.staged.len(), 1);
        assert_eq!(status.staged[0].path, "new_name.rs");
        assert_eq!(status.staged[0].status, "R");
        assert_eq!(status.staged[0].original_path, Some("old_name.rs".to_string()));
    }

    #[test]
    fn parse_porcelain_v2_empty_output() {
        let status = parse_porcelain_v2("");
        assert_eq!(status.branch, None);
        assert_eq!(status.upstream, None);
        assert_eq!(status.ahead, 0);
        assert_eq!(status.behind, 0);
        assert_eq!(status.stash_count, 0);
        assert!(status.staged.is_empty());
        assert!(status.unstaged.is_empty());
        assert!(status.untracked.is_empty());
    }

    #[test]
    fn parse_porcelain_v2_full_scenario() {
        let output = "\
# branch.oid deadbeef
# branch.head feature/test
# branch.upstream origin/feature/test
# branch.ab +2 -0
# stash 1
1 M. N... 100644 100644 100644 abc123 def456 src/staged.rs
1 .M N... 100644 100644 100644 abc123 def456 src/unstaged.rs
1 A. N... 000000 100644 100644 0000000 abc123 src/new.rs
? untracked.txt
";
        let status = parse_porcelain_v2(output);
        assert_eq!(status.branch, Some("feature/test".to_string()));
        assert_eq!(status.upstream, Some("origin/feature/test".to_string()));
        assert_eq!(status.ahead, 2);
        assert_eq!(status.behind, 0);
        assert_eq!(status.stash_count, 1);
        assert_eq!(status.staged.len(), 2); // M. and A.
        assert_eq!(status.unstaged.len(), 1); // .M
        assert_eq!(status.untracked, vec!["untracked.txt"]);
    }

    #[test]
    fn parse_porcelain_v2_skips_submodules() {
        // Submodule entry: sub field starts with 'S' (e.g. S..U, SC.., SM.U)
        let output = "1 .M S..U 160000 160000 160000 abc123 abc123 plugins\n\
                       1 .M N... 100644 100644 100644 def456 def456 src/main.rs";
        let status = parse_porcelain_v2(output);
        assert_eq!(status.unstaged.len(), 1, "submodule should be skipped");
        assert_eq!(status.unstaged[0].path, "src/main.rs");
    }

    // --- Integration tests for get_working_tree_status ---

    #[tokio::test]
    async fn get_working_tree_status_on_real_repo() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let result = get_working_tree_status(repo.to_string_lossy().to_string()).await;
        assert!(result.is_ok(), "should succeed on real repo");
        let status = result.unwrap();
        // We're in a git repo, so branch should be set (unless detached)
        // At minimum, the parse should not panic
        let _ = status.ahead; // confirms the field was populated without panic
    }

    #[tokio::test]
    async fn get_working_tree_status_nonexistent_path() {
        let result = get_working_tree_status("/nonexistent/repo/xyz".to_string()).await;
        assert!(result.is_err());
    }

    // --- validate_paths_within_repo tests ---

    #[test]
    fn validate_paths_rejects_traversal() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let result = validate_paths_within_repo(&repo, &["../../etc/passwd".to_string()]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("outside repository"));
    }

    #[test]
    fn validate_paths_accepts_normal_paths() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let result = validate_paths_within_repo(&repo, &["src/git.rs".to_string()]);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_paths_rejects_absolute_paths() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let result = validate_paths_within_repo(&repo, &["/etc/passwd".to_string()]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("absolute path"));
    }

    // --- Integration tests for stage/unstage/discard ---

    /// Helper: create a temp git repo with an initial commit.
    fn setup_test_repo_with_commit() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().to_path_buf();
        std::process::Command::new("git").current_dir(&path).args(["init"]).output().expect("git init");
        std::process::Command::new("git").current_dir(&path).args(["config", "user.email", "test@test.com"]).output().expect("config email");
        std::process::Command::new("git").current_dir(&path).args(["config", "user.name", "Test"]).output().expect("config name");
        // Create an initial file and commit
        std::fs::write(path.join("initial.txt"), "hello").expect("write initial");
        std::process::Command::new("git").current_dir(&path).args(["add", "initial.txt"]).output().expect("add");
        std::process::Command::new("git").current_dir(&path).args(["commit", "-m", "initial"]).output().expect("commit");
        (dir, path)
    }

    #[tokio::test]
    async fn stage_files_adds_to_index() {
        let (_dir, path) = setup_test_repo_with_commit();
        std::fs::write(path.join("new.txt"), "content").expect("write");
        let result = git_stage_files(path.to_string_lossy().to_string(), vec!["new.txt".to_string()]);
        assert!(result.is_ok());
        // Verify it's staged
        let status = get_working_tree_status(path.to_string_lossy().to_string()).await.unwrap();
        assert!(status.staged.iter().any(|e| e.path == "new.txt"), "new.txt should be staged");
    }

    #[tokio::test]
    async fn unstage_files_removes_from_index() {
        let (_dir, path) = setup_test_repo_with_commit();
        std::fs::write(path.join("staged.txt"), "content").expect("write");
        std::process::Command::new("git").current_dir(&path).args(["add", "staged.txt"]).output().expect("add");
        let result = git_unstage_files(path.to_string_lossy().to_string(), vec!["staged.txt".to_string()]);
        assert!(result.is_ok());
        // Verify it's no longer staged (should be untracked now)
        let status = get_working_tree_status(path.to_string_lossy().to_string()).await.unwrap();
        assert!(!status.staged.iter().any(|e| e.path == "staged.txt"), "staged.txt should not be staged");
        assert!(status.untracked.contains(&"staged.txt".to_string()), "staged.txt should be untracked");
    }

    #[test]
    fn discard_files_restores_working_tree() {
        let (_dir, path) = setup_test_repo_with_commit();
        // Modify the initial file
        std::fs::write(path.join("initial.txt"), "modified").expect("write");
        let result = git_discard_files(path.to_string_lossy().to_string(), vec!["initial.txt".to_string()]);
        assert!(result.is_ok());
        // Content should be restored
        let content = std::fs::read_to_string(path.join("initial.txt")).expect("read");
        assert_eq!(content, "hello");
    }

    #[test]
    fn stage_files_rejects_path_traversal() {
        let (_dir, path) = setup_test_repo_with_commit();
        let result = git_stage_files(path.to_string_lossy().to_string(), vec!["../../etc/passwd".to_string()]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("outside repository"));
    }

    #[test]
    fn unstage_files_rejects_path_traversal() {
        let (_dir, path) = setup_test_repo_with_commit();
        let result = git_unstage_files(path.to_string_lossy().to_string(), vec!["../../etc/passwd".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn discard_files_rejects_path_traversal() {
        let (_dir, path) = setup_test_repo_with_commit();
        let result = git_discard_files(path.to_string_lossy().to_string(), vec!["../../etc/passwd".to_string()]);
        assert!(result.is_err());
    }

    // --- git_commit tests ---

    #[test]
    fn git_commit_creates_commit_and_returns_hash() {
        let (_dir, path) = setup_test_repo_with_commit();
        std::fs::write(path.join("commit_test.txt"), "data").expect("write");
        std::process::Command::new("git").current_dir(&path).args(["add", "commit_test.txt"]).output().expect("add");
        let result = git_commit(path.to_string_lossy().to_string(), "test commit".to_string(), None);
        assert!(result.is_ok());
        let hash = result.unwrap();
        assert_eq!(hash.len(), 40, "should return full 40-char SHA");
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()), "hash should be hex");
    }

    #[test]
    fn git_commit_amend_works() {
        let (_dir, path) = setup_test_repo_with_commit();
        let result = git_commit(path.to_string_lossy().to_string(), "amended message".to_string(), Some(true));
        assert!(result.is_ok());
        // Verify the commit message changed
        let out = git_cmd(&path).args(["log", "--format=%s", "-1"]).run().unwrap();
        assert_eq!(out.stdout.trim(), "amended message");
    }

    #[test]
    fn git_commit_fails_with_nothing_staged() {
        let (_dir, path) = setup_test_repo_with_commit();
        let result = git_commit(path.to_string_lossy().to_string(), "empty commit".to_string(), None);
        assert!(result.is_err(), "commit with nothing staged should fail");
    }

    // --- get_last_commit_timestamps tests ---

    #[test]
    fn get_last_commit_timestamps_returns_timestamp_for_main() {
        // Uses the current repo (tuicommander) as a real git repo.
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let result = get_last_commit_timestamps(&repo, &["main".to_string()]);
        assert!(result.contains_key("main"), "should contain 'main' key");
        let ts = result["main"];
        assert!(ts.is_some(), "main branch should have a commit timestamp");
        // Timestamp should be reasonable (after 2024-01-01 = 1704067200)
        assert!(ts.unwrap() > 1_704_067_200, "timestamp should be after 2024");
    }

    #[test]
    fn get_last_commit_timestamps_nonexistent_branch_returns_none() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let result = get_last_commit_timestamps(
            &repo,
            &["nonexistent-branch-abc123".to_string()],
        );
        assert!(result.contains_key("nonexistent-branch-abc123"));
        assert!(result["nonexistent-branch-abc123"].is_none());
    }

    #[test]
    fn get_last_commit_timestamps_empty_input_returns_empty() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let result = get_last_commit_timestamps(&repo, &[]);
        assert!(result.is_empty());
    }

    // --- get_git_panel_context_impl tests ---

    #[test]
    fn git_panel_context_returns_valid_branch() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let ctx = get_git_panel_context_impl(&repo);
        assert!(!ctx.branch.is_empty(), "branch should not be empty");
    }

    #[test]
    fn git_panel_context_status_is_known_value() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let ctx = get_git_panel_context_impl(&repo);
        assert!(
            ["clean", "dirty", "conflict"].contains(&ctx.status.as_str()),
            "status should be clean/dirty/conflict, got: {}",
            ctx.status
        );
    }

    #[test]
    fn git_panel_context_has_last_commit() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let ctx = get_git_panel_context_impl(&repo);
        assert!(ctx.last_commit.is_some(), "repo should have at least one commit");
        let commit = ctx.last_commit.unwrap();
        assert!(!commit.hash.is_empty());
        assert!(!commit.short_hash.is_empty());
        assert!(!commit.subject.is_empty());
    }

    #[test]
    fn git_panel_context_not_in_rebase_or_cherry_pick() {
        // This repo should not be in rebase/cherry-pick during tests
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let ctx = get_git_panel_context_impl(&repo);
        assert!(!ctx.in_rebase, "should not be in rebase");
        assert!(!ctx.in_cherry_pick, "should not be in cherry-pick");
    }

    #[test]
    fn git_panel_context_nonexistent_repo_returns_defaults() {
        let ctx = get_git_panel_context_impl(Path::new("/nonexistent/repo"));
        assert!(ctx.branch.is_empty(), "branch should be empty for non-repo");
        assert_eq!(ctx.staged_count, 0);
        assert_eq!(ctx.changed_count, 0);
        assert_eq!(ctx.stash_count, 0);
        assert!(ctx.last_commit.is_none());
    }

    // --- parse_commit_log_line tests ---

    #[test]
    fn parse_commit_log_line_basic() {
        let line = "abc123\x00def456 ghi789\x00HEAD -> main, tag: v1.0\x00Alice\x002024-01-15T10:30:00+01:00\x00Initial commit";
        let entry = parse_commit_log_line(line).expect("should parse");
        assert_eq!(entry.hash, "abc123");
        assert_eq!(entry.parents, vec!["def456", "ghi789"]);
        assert_eq!(entry.refs, vec!["HEAD -> main", "tag: v1.0"]);
        assert_eq!(entry.author_name, "Alice");
        assert_eq!(entry.author_date, "2024-01-15T10:30:00+01:00");
        assert_eq!(entry.subject, "Initial commit");
    }

    #[test]
    fn parse_commit_log_line_no_parents_no_refs() {
        let line = "abc123\x00\x00\x00Bob\x002024-01-15T10:30:00Z\x00Root commit";
        let entry = parse_commit_log_line(line).expect("should parse");
        assert!(entry.parents.is_empty());
        assert!(entry.refs.is_empty());
    }

    #[test]
    fn parse_commit_log_line_malformed_returns_none() {
        assert!(parse_commit_log_line("not enough fields").is_none());
        assert!(parse_commit_log_line("a\0b\0c").is_none());
    }

    // --- get_commit_log integration tests ---

    #[tokio::test]
    async fn get_commit_log_returns_commits_for_real_repo() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let result = get_commit_log(repo.to_string_lossy().to_string(), Some(5), None).await;
        let commits = result.expect("should succeed on real repo");
        assert!(!commits.is_empty(), "repo should have commits");
        assert!(commits.len() <= 5, "should respect count limit");
        // First commit should have a valid hash (40 hex chars)
        assert_eq!(commits[0].hash.len(), 40);
        assert!(commits[0].hash.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(!commits[0].author_name.is_empty());
        assert!(!commits[0].author_date.is_empty());
        assert!(!commits[0].subject.is_empty());
    }

    #[tokio::test]
    async fn get_commit_log_default_count_is_50() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let result = get_commit_log(repo.to_string_lossy().to_string(), None, None).await;
        let commits = result.expect("should succeed");
        // We know this repo has many commits; default limit is 50
        assert!(commits.len() <= 50);
    }

    #[tokio::test]
    async fn get_commit_log_count_clamped_to_500() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        // Requesting 9999 should be clamped to 500
        let result = get_commit_log(repo.to_string_lossy().to_string(), Some(9999), None).await;
        let commits = result.expect("should succeed");
        assert!(commits.len() <= 500);
    }

    #[tokio::test]
    async fn get_commit_log_pagination_with_after() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let repo_str = repo.to_string_lossy().to_string();

        // Get first page
        let page1 = get_commit_log(repo_str.clone(), Some(3), None).await.expect("page 1");
        assert!(page1.len() >= 3, "need at least 3 commits for this test");

        // Get second page starting from the last commit of page 1
        let last_hash = &page1[2].hash;
        let page2 = get_commit_log(repo_str, Some(3), Some(last_hash.clone())).await.expect("page 2");
        assert!(!page2.is_empty(), "page 2 should have commits");

        // First commit of page 2 should be the same as last of page 1 (the `after` hash)
        assert_eq!(page2[0].hash, *last_hash, "pagination should start from the `after` commit");
    }

    #[tokio::test]
    async fn get_commit_log_fails_for_nonexistent_repo() {
        let result = get_commit_log("/nonexistent/repo".to_string(), None, None).await;
        assert!(result.is_err());
    }

    // --- get_stash_list tests ---

    #[test]
    fn get_stash_list_real_repo_does_not_error() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let result = get_stash_list(repo.to_string_lossy().to_string());
        // Should succeed regardless of whether there are stashes
        assert!(result.is_ok(), "get_stash_list should not error on a real repo");
    }

    #[test]
    fn get_stash_list_nonexistent_repo_returns_empty() {
        let result = get_stash_list("/nonexistent/repo".to_string());
        // run_silent returns None for non-git dir, so we get empty vec
        assert_eq!(result.unwrap(), vec![]);
    }

    // --- get_file_history integration tests ---

    #[tokio::test]
    async fn get_file_history_returns_commits_for_known_file() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let result = get_file_history(
            repo.to_string_lossy().to_string(),
            "src-tauri/src/git.rs".to_string(),
            Some(5),
            None,
        ).await;
        let commits = result.expect("should succeed for a file in the repo");
        assert!(!commits.is_empty(), "git.rs should have commit history");
        assert!(commits.len() <= 5);
    }

    #[tokio::test]
    async fn get_file_history_nonexistent_file_returns_empty() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let result = get_file_history(
            repo.to_string_lossy().to_string(),
            "nonexistent-file-xyz.txt".to_string(),
            Some(5),
            None,
        ).await;
        // git log with a nonexistent file returns empty output, not an error
        let commits = result.expect("should not error");
        assert!(commits.is_empty());
    }

    #[tokio::test]
    async fn get_file_history_fails_for_nonexistent_repo() {
        let result = get_file_history(
            "/nonexistent/repo".to_string(),
            "file.txt".to_string(),
            None,
            None,
        ).await;
        assert!(result.is_err());
    }

    // --- validate_git_hash tests ---

    #[test]
    fn validate_git_hash_accepts_40_hex_chars() {
        assert!(validate_git_hash("abc1234567890123456789012345678901234abc").is_ok());
    }

    #[test]
    fn validate_git_hash_accepts_short_hash() {
        assert!(validate_git_hash("abcd").is_ok()); // minimum 4 chars
    }

    #[test]
    fn validate_git_hash_accepts_uppercase_hex() {
        assert!(validate_git_hash("ABCDEF1234567890abcdef1234567890abcdef12").is_ok());
    }

    #[test]
    fn validate_git_hash_rejects_empty() {
        assert!(validate_git_hash("").is_err());
    }

    #[test]
    fn validate_git_hash_rejects_too_short() {
        assert!(validate_git_hash("abc").is_err()); // 3 chars < minimum 4
    }

    #[test]
    fn validate_git_hash_rejects_too_long() {
        assert!(validate_git_hash("a".repeat(41).as_str()).is_err());
    }

    #[test]
    fn validate_git_hash_rejects_non_hex() {
        assert!(validate_git_hash("ghij1234567890123456789012345678901234gh").is_err());
    }

    #[test]
    fn validate_git_hash_rejects_injection_attempt() {
        assert!(validate_git_hash("abcd; rm -rf /").is_err());
    }

    // --- validate_stash_ref tests ---

    #[test]
    fn validate_stash_ref_accepts_valid_zero() {
        assert!(validate_stash_ref("stash@{0}").is_ok());
    }

    #[test]
    fn validate_stash_ref_accepts_valid_large_index() {
        assert!(validate_stash_ref("stash@{42}").is_ok());
    }

    #[test]
    fn validate_stash_ref_rejects_empty() {
        assert!(validate_stash_ref("").is_err());
    }

    #[test]
    fn validate_stash_ref_rejects_wrong_prefix() {
        assert!(validate_stash_ref("refs@{0}").is_err());
    }

    #[test]
    fn validate_stash_ref_rejects_missing_brace() {
        assert!(validate_stash_ref("stash@{0").is_err());
    }

    #[test]
    fn validate_stash_ref_rejects_non_numeric_index() {
        assert!(validate_stash_ref("stash@{abc}").is_err());
    }

    #[test]
    fn validate_stash_ref_rejects_negative_index() {
        assert!(validate_stash_ref("stash@{-1}").is_err());
    }

    #[test]
    fn validate_stash_ref_rejects_injection_attempt() {
        assert!(validate_stash_ref("stash@{0}; echo pwned").is_err());
    }

    // --- parse_blame_porcelain tests ---

    #[test]
    fn parse_blame_porcelain_single_line() {
        let output = "\
abc1234567890123456789012345678901234abcd 1 1 1
author Alice
author-mail <alice@example.com>
author-time 1700000000
author-tz +0100
committer Alice
committer-mail <alice@example.com>
committer-time 1700000000
committer-tz +0100
summary Initial commit
filename test.txt
\tHello, world!
";
        let lines = parse_blame_porcelain(output);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].hash, "abc1234567890123456789012345678901234abcd");
        assert_eq!(lines[0].author, "Alice");
        assert_eq!(lines[0].author_time, 1700000000);
        assert_eq!(lines[0].line_number, 1);
        assert_eq!(lines[0].content, "Hello, world!");
    }

    #[test]
    fn parse_blame_porcelain_multiple_lines_same_commit() {
        let output = "\
aaaa234567890123456789012345678901234aaaa 1 1 2
author Bob
author-mail <bob@example.com>
author-time 1700000001
author-tz +0000
committer Bob
committer-mail <bob@example.com>
committer-time 1700000001
committer-tz +0000
summary Add two lines
filename test.txt
\tLine one
aaaa234567890123456789012345678901234aaaa 2 2
\tLine two
";
        let lines = parse_blame_porcelain(output);
        assert_eq!(lines.len(), 2);
        // Both lines should share the same commit metadata
        assert_eq!(lines[0].hash, lines[1].hash);
        assert_eq!(lines[0].author, "Bob");
        assert_eq!(lines[1].author, "Bob");
        assert_eq!(lines[0].line_number, 1);
        assert_eq!(lines[1].line_number, 2);
        assert_eq!(lines[0].content, "Line one");
        assert_eq!(lines[1].content, "Line two");
    }

    #[test]
    fn parse_blame_porcelain_empty_output() {
        let lines = parse_blame_porcelain("");
        assert!(lines.is_empty());
    }

    #[test]
    fn parse_blame_porcelain_malformed_hash_line_ignored() {
        // A line that looks nothing like porcelain output should produce no entries
        let output = "this is not porcelain\nneither is this\n";
        let lines = parse_blame_porcelain(output);
        assert!(lines.is_empty());
    }

    #[test]
    fn parse_blame_porcelain_content_with_tab_prefix_preserved() {
        // Content lines start with \t — verify that leading whitespace in the
        // actual source line is preserved after stripping the initial \t.
        let output = "\
abc1234567890123456789012345678901234abcd 1 1 1
author Carol
author-mail <carol@example.com>
author-time 1700000002
author-tz +0000
committer Carol
committer-mail <carol@example.com>
committer-time 1700000002
committer-tz +0000
summary Indented code
filename test.txt
\t    indented line
";
        let lines = parse_blame_porcelain(output);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].content, "    indented line");
    }

    #[test]
    fn parse_blame_porcelain_two_different_commits() {
        let output = "\
aaaa234567890123456789012345678901234aaaa 1 1 1
author Alice
author-time 1700000000
committer Alice
committer-time 1700000000
summary First
filename test.txt
\tLine from Alice
bbbb234567890123456789012345678901234bbbb 2 2 1
author Bob
author-time 1700000001
committer Bob
committer-time 1700000001
summary Second
filename test.txt
\tLine from Bob
";
        let lines = parse_blame_porcelain(output);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].author, "Alice");
        assert_eq!(lines[0].hash, "aaaa234567890123456789012345678901234aaaa");
        assert_eq!(lines[1].author, "Bob");
        assert_eq!(lines[1].hash, "bbbb234567890123456789012345678901234bbbb");
    }

    #[test]
    fn parse_blame_porcelain_empty_content_line() {
        // An empty source line is represented as just a tab character
        let output = "\
abc1234567890123456789012345678901234abcd 1 1 1
author Dan
author-time 1700000003
committer Dan
committer-time 1700000003
summary Blank line
filename test.txt
\t
";
        let lines = parse_blame_porcelain(output);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].content, "");
    }

    // --- get_file_blame integration test ---

    #[tokio::test]
    async fn get_file_blame_returns_lines_for_known_file() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let result = get_file_blame(
            repo.to_string_lossy().to_string(),
            "src-tauri/src/git.rs".to_string(),
        ).await;
        let lines = result.expect("should succeed for a file in the repo");
        assert!(!lines.is_empty(), "git.rs should have blame lines");
        // Every line should have a 40-char hex hash
        for bl in &lines {
            assert_eq!(bl.hash.len(), 40, "hash should be 40 chars: {}", bl.hash);
            assert!(!bl.author.is_empty(), "author should not be empty");
            assert!(bl.author_time > 0, "author_time should be positive");
            assert!(bl.line_number > 0, "line_number should be positive");
        }
        // Line numbers should be sequential
        for (i, bl) in lines.iter().enumerate() {
            assert_eq!(bl.line_number, (i + 1) as u32, "line numbers should be sequential");
        }
    }

    #[tokio::test]
    async fn get_file_blame_fails_for_nonexistent_file() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let result = get_file_blame(
            repo.to_string_lossy().to_string(),
            "nonexistent-file-xyz.txt".to_string(),
        ).await;
        assert!(result.is_err(), "blame on nonexistent file should fail");
    }

    #[tokio::test]
    async fn get_file_blame_fails_for_nonexistent_repo() {
        let result = get_file_blame(
            "/nonexistent/repo".to_string(),
            "file.txt".to_string(),
        ).await;
        assert!(result.is_err());
    }

    #[test]
    fn get_merged_branches_excludes_branch_at_same_sha_as_main() {
        let (_dir, path) = setup_test_repo_with_commit();
        // Rename to "main" for predictability
        std::process::Command::new("git")
            .current_dir(&path)
            .args(["branch", "-M", "main"])
            .output()
            .expect("rename to main");

        // Create a new branch at the same commit (no new commits)
        std::process::Command::new("git")
            .current_dir(&path)
            .args(["branch", "new-worktree-branch"])
            .output()
            .expect("create branch");

        let merged = get_merged_branches_impl(&path)
            .expect("get_merged_branches_impl should succeed");

        // The new branch has the same SHA as main — it should NOT be in the merged list
        assert!(
            !merged.iter().any(|b| b == "new-worktree-branch"),
            "branch at same SHA as main should not be considered merged, got: {merged:?}"
        );
    }

    #[test]
    fn get_merged_branches_includes_truly_merged_branch() {
        let (_dir, path) = setup_test_repo_with_commit();
        std::process::Command::new("git")
            .current_dir(&path)
            .args(["branch", "-M", "main"])
            .output()
            .expect("rename to main");

        // Create a feature branch, add a commit, then merge it back
        std::process::Command::new("git")
            .current_dir(&path)
            .args(["checkout", "-b", "feat-merged"])
            .output()
            .expect("checkout -b");
        std::fs::write(path.join("feature.txt"), "feature").expect("write");
        std::process::Command::new("git")
            .current_dir(&path)
            .args(["add", "feature.txt"])
            .output()
            .expect("add");
        std::process::Command::new("git")
            .current_dir(&path)
            .args(["commit", "-m", "feat"])
            .output()
            .expect("commit");
        std::process::Command::new("git")
            .current_dir(&path)
            .args(["checkout", "main"])
            .output()
            .expect("checkout main");
        std::process::Command::new("git")
            .current_dir(&path)
            .args(["merge", "--no-ff", "feat-merged", "-m", "merge feat"])
            .output()
            .expect("merge");

        let merged = get_merged_branches_impl(&path)
            .expect("get_merged_branches_impl should succeed");

        // feat-merged is truly merged — it should be in the list
        assert!(
            merged.iter().any(|b| b == "feat-merged"),
            "truly merged branch should be in the merged list, got: {merged:?}"
        );
    }

    // --- create_branch_impl tests ---

    #[test]
    fn test_create_branch_from_head() {
        let (_dir, path) = setup_test_repo_with_commit();
        let result = create_branch_impl(path.to_str().unwrap(), "feature-x", None, false);
        assert!(result.is_ok(), "should create branch: {result:?}");
        // Verify branch exists
        let out = git_cmd(&path).args(["branch", "--list", "feature-x"]).run().unwrap();
        assert!(out.stdout.contains("feature-x"), "branch should exist after creation");
    }

    #[test]
    fn test_create_branch_with_checkout() {
        let (_dir, path) = setup_test_repo_with_commit();
        let result = create_branch_impl(path.to_str().unwrap(), "feature-checkout", None, true);
        assert!(result.is_ok(), "should create and checkout branch: {result:?}");
        // Verify HEAD points to the new branch
        let out = git_cmd(&path).args(["rev-parse", "--abbrev-ref", "HEAD"]).run().unwrap();
        assert_eq!(out.stdout.trim(), "feature-checkout");
    }

    #[test]
    fn test_create_branch_from_ref() {
        let (_dir, path) = setup_test_repo_with_commit();
        // Get HEAD commit hash to use as start point
        let out = git_cmd(&path).args(["rev-parse", "HEAD"]).run().unwrap();
        let commit_hash = out.stdout.trim().to_string();
        let result = create_branch_impl(path.to_str().unwrap(), "from-ref", Some(&commit_hash), false);
        assert!(result.is_ok(), "should create branch from ref: {result:?}");
        let out = git_cmd(&path).args(["branch", "--list", "from-ref"]).run().unwrap();
        assert!(out.stdout.contains("from-ref"), "branch should exist after creation from ref");
    }

    #[test]
    fn test_create_branch_refuses_empty_name() {
        let (_dir, path) = setup_test_repo_with_commit();
        let result = create_branch_impl(path.to_str().unwrap(), "", None, false);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"), "error should mention empty name");
    }

    #[test]
    fn test_create_branch_refuses_invalid_name() {
        let (_dir, path) = setup_test_repo_with_commit();
        let result = create_branch_impl(path.to_str().unwrap(), "bad name with spaces", None, false);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("spaces"), "error should mention spaces");
    }

    #[test]
    fn test_create_branch_refuses_duplicate() {
        let (_dir, path) = setup_test_repo_with_commit();
        // Create once — should succeed
        let first = create_branch_impl(path.to_str().unwrap(), "dup-branch", None, false);
        assert!(first.is_ok(), "first creation should succeed: {first:?}");
        // Create again — should fail
        let second = create_branch_impl(path.to_str().unwrap(), "dup-branch", None, false);
        assert!(second.is_err(), "duplicate branch creation should fail");
        assert!(second.unwrap_err().contains("already exists"), "error should mention already exists");
    }

    // --- get_branches_detail tests ---

    #[test]
    fn test_get_branches_detail_returns_rich_info() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let repo_root = PathBuf::from(manifest_dir).parent().unwrap().to_path_buf();

        let branches = get_branches_detail_impl(&repo_root)
            .expect("get_branches_detail_impl should succeed on real repo");

        assert!(!branches.is_empty(), "should return at least one branch");

        // Branch names must not be empty
        for b in &branches {
            assert!(!b.name.is_empty(), "branch name should not be empty");
        }

        // Exactly one branch must be current
        let current_branches: Vec<&BranchDetail> = branches.iter().filter(|b| b.is_current).collect();
        assert_eq!(
            current_branches.len(), 1,
            "exactly one branch should be current, got: {:?}",
            current_branches.iter().map(|b| &b.name).collect::<Vec<_>>()
        );

        // The current branch must have a last_commit_date
        let current = current_branches[0];
        assert!(
            current.last_commit_date.is_some(),
            "current branch should have a last_commit_date"
        );

        // At least one branch should have a non-empty last_commit_date
        assert!(
            branches.iter().any(|b| b.last_commit_date.is_some()),
            "at least one branch should have a last_commit_date"
        );

        // No origin/HEAD pseudo-ref should appear
        assert!(
            !branches.iter().any(|b| b.name.ends_with("/HEAD")),
            "origin/HEAD should be filtered out"
        );
    }

    #[test]
    fn test_parse_track_value_various_formats() {
        assert_eq!(parse_track_value("[ahead 3]", "ahead"), Some(3));
        assert_eq!(parse_track_value("[behind 7]", "behind"), Some(7));
        assert_eq!(parse_track_value("[ahead 2, behind 5]", "ahead"), Some(2));
        assert_eq!(parse_track_value("[ahead 2, behind 5]", "behind"), Some(5));
        assert_eq!(parse_track_value("", "ahead"), None);
        assert_eq!(parse_track_value("[behind 7]", "ahead"), None);
        assert_eq!(parse_track_value("[ahead 3]", "behind"), None);
    }

    // --- delete_branch_impl tests ---

    #[test]
    fn test_delete_branch_safe() {
        let (_dir, path) = setup_test_repo_with_commit();
        let path_str = path.to_string_lossy().to_string();
        // Ensure we're on main
        std::process::Command::new("git").current_dir(&path).args(["branch", "-M", "main"]).output().expect("rename to main");
        // Create a branch to delete at same commit as main (fully merged)
        std::process::Command::new("git").current_dir(&path).args(["branch", "to-delete"]).output().expect("create branch");

        let result = delete_branch_impl(&path_str, "to-delete", false);
        assert!(result.is_ok(), "safe delete of merged branch should succeed: {result:?}");
        let r = result.unwrap();
        assert_eq!(r.branch, "to-delete");
        assert!(!r.was_force);
        assert!(r.deleted);
    }

    #[test]
    fn test_delete_branch_force() {
        let (_dir, path) = setup_test_repo_with_commit();
        let path_str = path.to_string_lossy().to_string();
        std::process::Command::new("git").current_dir(&path).args(["branch", "-M", "main"]).output().expect("rename to main");
        // Create a branch with unmerged commits
        std::process::Command::new("git").current_dir(&path).args(["checkout", "-b", "unmerged-branch"]).output().expect("checkout -b");
        std::fs::write(path.join("unmerged.txt"), "data").expect("write");
        std::process::Command::new("git").current_dir(&path).args(["add", "unmerged.txt"]).output().expect("add");
        std::process::Command::new("git").current_dir(&path).args(["commit", "-m", "unmerged commit"]).output().expect("commit");
        // Switch back to main so we can delete the unmerged branch
        std::process::Command::new("git").current_dir(&path).args(["checkout", "main"]).output().expect("checkout main");

        // Safe delete should fail (unmerged)
        let safe_result = delete_branch_impl(&path_str, "unmerged-branch", false);
        assert!(safe_result.is_err(), "safe delete of unmerged branch should fail");

        // Force delete should succeed
        let result = delete_branch_impl(&path_str, "unmerged-branch", true);
        assert!(result.is_ok(), "force delete should succeed: {result:?}");
        let r = result.unwrap();
        assert_eq!(r.branch, "unmerged-branch");
        assert!(r.was_force);
        assert!(r.deleted);
    }

    #[test]
    fn test_delete_branch_refuses_main() {
        let (_dir, path) = setup_test_repo_with_commit();
        let path_str = path.to_string_lossy().to_string();
        std::process::Command::new("git").current_dir(&path).args(["branch", "-M", "main"]).output().expect("rename to main");

        let result = delete_branch_impl(&path_str, "main", false);
        assert!(result.is_err(), "should refuse to delete main branch");
        let err = result.unwrap_err();
        assert!(err.contains("main") || err.contains("protected"), "error should mention protection: {err}");
    }

    #[test]
    fn test_delete_branch_refuses_current() {
        let (_dir, path) = setup_test_repo_with_commit();
        let path_str = path.to_string_lossy().to_string();
        // Rename to something non-main so is_main_branch check doesn't fire first
        std::process::Command::new("git").current_dir(&path).args(["branch", "-M", "feature-branch"]).output().expect("rename");

        let result = delete_branch_impl(&path_str, "feature-branch", false);
        assert!(result.is_err(), "should refuse to delete current branch");
        let err = result.unwrap_err();
        assert!(err.contains("current") || err.contains("checked out"), "error should mention current branch: {err}");
    }

    #[test]
    fn test_delete_branch_refuses_empty_name() {
        let (_dir, path) = setup_test_repo_with_commit();
        let path_str = path.to_string_lossy().to_string();

        let result = delete_branch_impl(&path_str, "", false);
        assert!(result.is_err(), "empty branch name should fail");
        let err = result.unwrap_err();
        assert!(err.to_lowercase().contains("empty"), "error should mention empty: {err}");
    }

    // --- get_recent_branches_impl tests ---

    #[test]
    fn test_get_recent_branches_on_real_repo() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let repo_root = PathBuf::from(manifest_dir).parent().unwrap().to_path_buf();

        let result = get_recent_branches_impl(&repo_root, 5);
        assert!(result.is_ok(), "should not error on a real repo: {result:?}");
        // Result may be empty on a fresh repo, but it must be a Vec
        let branches = result.unwrap();
        // Branch names must not be empty strings
        for b in &branches {
            assert!(!b.is_empty(), "branch names must not be empty");
        }
    }

    #[test]
    fn test_get_recent_branches_limit() {
        let (_dir, path) = setup_test_repo_with_commit();

        // Create and switch between several branches to populate reflog
        for name in &["branch-a", "branch-b", "branch-c", "branch-d"] {
            std::process::Command::new("git")
                .current_dir(&path)
                .args(["checkout", "-b", name])
                .output()
                .expect("checkout -b");
        }

        // Switch back to the initial branch (master or main) to ensure some checkouts happened
        let out = std::process::Command::new("git")
            .current_dir(&path)
            .args(["checkout", "master"])
            .output()
            .expect("checkout master/main");
        if !out.status.success() {
            std::process::Command::new("git")
                .current_dir(&path)
                .args(["checkout", "main"])
                .output()
                .ok();
        }

        let result = get_recent_branches_impl(&path, 2);
        assert!(result.is_ok(), "should succeed: {result:?}");
        let branches = result.unwrap();
        assert!(branches.len() <= 2, "limit of 2 must be respected, got: {branches:?}");
    }

    #[test]
    fn test_get_recent_branches_on_nonexistent_path() {
        let result = get_recent_branches_impl(Path::new("/nonexistent/path/xyz"), 5);
        // A non-existent path must produce an error (git spawn or non-zero exit)
        assert!(result.is_err(), "nonexistent path should return an error");
    }
}
