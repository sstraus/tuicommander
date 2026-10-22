use serde::Serialize;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use crate::git_cli::git_cmd;
use crate::git_reads::git_reads;

// --- File-based git helpers (no subprocess) ---

/// Resolve the .git directory for a repo, handling linked worktrees.
///
/// - Normal repo: `<repo>/.git/` (a directory)
/// - Linked worktree: `<repo>/.git` is a file containing `gitdir: <path>`
pub fn resolve_git_dir(repo_path: &Path) -> Option<PathBuf> {
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

/// Resolve a repo path to its canonical main-worktree root.
///
/// Linked worktrees share the binding of their main repo: a linked worktree's
/// gitdir contains a `commondir` file pointing at the main `.git`; the main
/// worktree root is that common dir's parent. A normal repo resolves to the
/// parent of its own `.git`. Falls back to the canonicalized input when the path
/// is not a git repo, so callers always get a stable key.
pub fn canonical_repo_root(repo_path: &Path) -> PathBuf {
    if let Some(git_dir) = resolve_git_dir(repo_path) {
        // A linked worktree's gitdir has a `commondir` file → the main `.git`.
        let common_git_dir = match fs::read_to_string(git_dir.join("commondir")) {
            Ok(rel) => {
                let rel = rel.trim();
                let p = Path::new(rel);
                if p.is_absolute() {
                    p.to_path_buf()
                } else {
                    git_dir.join(p)
                }
            }
            Err(_) => git_dir.clone(),
        };
        let common_git_dir = common_git_dir.canonicalize().unwrap_or(common_git_dir);
        if let Some(parent) = common_git_dir.parent() {
            return parent
                .canonicalize()
                .unwrap_or_else(|_| parent.to_path_buf());
        }
    }
    repo_path
        .canonicalize()
        .unwrap_or_else(|_| repo_path.to_path_buf())
}

/// Read the current branch name from .git/HEAD (file I/O, no subprocess).
/// Returns None for detached HEAD or if the file can't be read.
pub fn read_branch_from_head(repo_path: &Path) -> Option<String> {
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

/// Resolve the git config file holding a repo's remotes, handling linked
/// worktrees.
///
/// A normal repo has `<gitdir>/config`. A linked worktree's gitdir has NO
/// `config` of its own — remotes live in the COMMON config (pointed at by the
/// worktree's `commondir` file). Falling back to the common config makes remote
/// resolution work in worktrees too. For a normal checkout `<gitdir>/config`
/// exists, so behavior there is unchanged.
fn resolve_git_config_path(repo_path: &Path) -> Option<PathBuf> {
    let git_dir = resolve_git_dir(repo_path)?;
    let own = git_dir.join("config");
    if own.is_file() {
        return Some(own);
    }
    // Linked worktree: the config (with the remotes) lives in the common dir.
    let cfg = common_git_dir(&git_dir).join("config");
    cfg.is_file().then_some(cfg)
}

/// The common git dir for a (possibly-worktree) gitdir.
///
/// A linked worktree's gitdir has a `commondir` file pointing at the main `.git`,
/// where the shared refs (`refs/heads`, `refs/remotes`, `packed-refs`) and config
/// live. A normal gitdir has no `commondir`, so this returns it unchanged —
/// making callers worktree-aware without altering normal-checkout behavior.
fn common_git_dir(git_dir: &Path) -> PathBuf {
    match fs::read_to_string(git_dir.join("commondir")) {
        Ok(rel) => {
            let rel = rel.trim();
            let common = if Path::new(rel).is_absolute() {
                PathBuf::from(rel)
            } else {
                git_dir.join(rel)
            };
            common.canonicalize().unwrap_or(common)
        }
        Err(_) => git_dir.to_path_buf(),
    }
}

/// Read the origin remote URL from the git config (file I/O, no subprocess).
/// Parses the `[remote "origin"]` section for the `url` key.
pub fn read_remote_url(repo_path: &Path) -> Option<String> {
    let config_content = fs::read_to_string(resolve_git_config_path(repo_path)?).ok()?;
    parse_git_config_remote_url(&config_content, "origin")
}

/// Enumerate ALL git remotes as `(name, url)` from the git config (file I/O, no
/// subprocess). Unlike [`read_remote_url`] — which only reads `origin` — this
/// surfaces every remote so the caller can detect multi-remote ambiguity
/// (origin + upstream + fork) when binding a repo to a GitHub account.
pub fn list_remotes(repo_path: &Path) -> Vec<(String, String)> {
    let Some(cfg) = resolve_git_config_path(repo_path) else {
        return Vec::new();
    };
    let Ok(config_content) = fs::read_to_string(cfg) else {
        return Vec::new();
    };
    parse_git_config_remotes(&config_content)
}

/// Parse all `[remote "<name>"] url = <url>` pairs from a git config string.
fn parse_git_config_remotes(config: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut current: Option<String> = None;
    for line in config.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            current = parse_remote_section_name(trimmed);
            continue;
        }
        if let Some(name) = &current
            && let Some(rest) = trimmed.strip_prefix("url")
        {
            let rest = rest.trim_start();
            if let Some(value) = rest.strip_prefix('=') {
                out.push((name.clone(), value.trim().to_string()));
            }
        }
    }
    out
}

/// Extract `origin` from a `[remote "origin"]` section header, or `None` for
/// any other section.
fn parse_remote_section_name(header: &str) -> Option<String> {
    header
        .strip_prefix("[remote \"")?
        .strip_suffix("\"]")
        .map(|name| name.to_string())
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
        if in_section && let Some(rest) = trimmed.strip_prefix("url") {
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
pub struct RepoInfo {
    pub path: String,
    pub name: String,
    pub initials: String,
    pub branch: String,
    pub status: String, // "clean", "dirty", "conflict"
    pub is_git_repo: bool,
}

/// Diff stats (additions/deletions)
#[derive(Clone, Serialize)]
pub struct DiffStats {
    additions: i32,
    deletions: i32,
}

impl DiffStats {
    /// Construct from line counts (used by the gix diff-stats adapter).
    pub fn from_counts(additions: i32, deletions: i32) -> Self {
        Self {
            additions,
            deletions,
        }
    }
}

/// Changed file information (for diff browser)
#[derive(Clone, Serialize)]
pub struct ChangedFile {
    pub path: String,
    pub status: String,
    pub additions: u32,
    pub deletions: u32,
}

/// Core logic for fetching git repository info (no caching).
pub fn get_repo_info_impl(path: &str) -> RepoInfo {
    let repo_path = PathBuf::from(path);

    // Check if it's a git repo
    let git_dir = repo_path.join(".git");
    if !git_dir.exists() && !repo_path.join("../.git").exists() {
        let name = repo_path
            .file_name()
            .map_or_else(|| path.to_string(), |n| n.to_string_lossy().to_string());
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
    let branch = read_branch_from_head(&repo_path).unwrap_or_else(|| "unknown".to_string());

    // Status comes from the same porcelain-v2 read the git panel uses, through
    // the GitReads port — one status source per repo instead of this badge
    // forking its own `git status --porcelain` beside the panel's.
    let status = git_reads().status_counts(&repo_path).status;

    let name = repo_path
        .file_name()
        .map_or_else(|| path.to_string(), |n| n.to_string_lossy().to_string());
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

/// Get the origin remote URL for a repository (returns None if not a git repo or no remote).
#[cfg(test)]
pub async fn get_remote_url(path: String) -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(move || get_remote_url_blocking(path))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// Validate that a branch name is a legal git branch name.
pub fn validate_branch_name(name: &str) -> Result<(), String> {
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
    Ok(())
}

/// Core logic for renaming a git branch.
pub fn rename_branch_impl(path: &str, old_name: &str, new_name: &str) -> Result<(), String> {
    let repo_path = PathBuf::from(path);

    validate_branch_name(new_name)?;

    // Execute git branch -m oldname newname
    match git_cmd(&repo_path)
        .args(["branch", "-m", "--", old_name, new_name])
        .run()
    {
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

/// Core logic for creating a git branch.
pub fn create_branch_impl(
    path: &str,
    name: &str,
    start_point: Option<&str>,
    checkout: bool,
) -> Result<(), String> {
    let repo_path = PathBuf::from(path);

    validate_branch_name(name)?;

    // Auto-fetch if the start point is a remote tracking branch
    if let Some(sp) = start_point {
        crate::worktree::fetch_if_remote(path, sp)?;
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

    // Persist the base ref in git config for "Update from base" support
    if let Some(sp) = start_point {
        let _ = crate::worktree::set_branch_base(path, name, sp);
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

/// Read the stored base ref for a branch (Tauri command).
#[cfg(test)]
pub async fn get_branch_base(path: String, branch_name: String) -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(move || get_branch_base_blocking(path, branch_name))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// Result of a branch deletion operation.
#[derive(Debug, Clone, Serialize)]
pub struct DeleteBranchResult {
    pub deleted: bool,
    pub branch: String,
    pub was_force: bool,
}

/// Core logic for deleting a git branch.
///
/// Refuses to delete protected main branches or the currently checked-out branch.
/// Use `force=true` to archive and delete branches with unmerged commits.
pub fn delete_branch_impl(
    path: &str,
    name: &str,
    force: bool,
) -> Result<DeleteBranchResult, String> {
    let repo_path = PathBuf::from(path);

    if name.is_empty() {
        return Err("Branch name cannot be empty".to_string());
    }

    // Refuse to delete main/primary branches
    if is_main_branch(name) {
        return Err(format!("Cannot delete protected branch '{name}'"));
    }

    // Refuse to delete the currently checked-out branch
    if let Some(current) = read_branch_from_head(&repo_path)
        && current == name
    {
        return Err(format!(
            "Cannot delete the currently checked-out branch '{name}'"
        ));
    }

    if !force {
        crate::worktree::delete_integrated_local_branch(path, name)?;
        return Ok(DeleteBranchResult {
            deleted: true,
            branch: name.to_owned(),
            was_force: false,
        });
    }
    crate::worktree::delete_local_branch_with_archive(&repo_path, name)?;
    Ok(DeleteBranchResult {
        deleted: true,
        branch: name.to_owned(),
        was_force: true,
    })
}

/// A recent commit entry for the dropdown
#[derive(Debug, Clone, Serialize)]
pub struct RecentCommit {
    pub hash: String,
    pub short_hash: String,
    pub subject: String,
}

/// Get the N most recent commits
#[cfg(test)]
pub async fn get_recent_commits(
    path: String,
    count: Option<u32>,
) -> Result<Vec<RecentCommit>, String> {
    tokio::task::spawn_blocking(move || get_recent_commits_blocking(path, count))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// Build the base git diff args for the given scope.
/// A commit hash compares `<hash>..HEAD`; empty/None compares working tree.
fn diff_base_args(scope: &Option<String>) -> Result<Vec<String>, String> {
    match scope.as_deref() {
        Some("staged") => Ok(vec!["diff".into(), "--cached".into()]),
        // "head" compares the working tree against the last commit (staged +
        // unstaged), i.e. the editor gutter's "vs committed version" view.
        Some("head") => Ok(vec!["diff".into(), "HEAD".into()]),
        Some(hash) if !hash.is_empty() => {
            validate_git_hash(hash)?;
            Ok(vec!["diff".into(), format!("{hash}^"), hash.into()])
        }
        _ => Ok(vec!["diff".into()]),
    }
}

/// Get git diff for a repository
#[cfg(test)]
pub async fn get_git_diff(path: String, scope: Option<String>) -> Result<String, String> {
    tokio::task::spawn_blocking(move || get_git_diff_blocking(path, scope))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}
/// Sync implementation of diff stats retrieval.
pub fn get_diff_stats_impl(path: &str, scope: Option<&str>) -> DiffStats {
    let repo_path = PathBuf::from(path);
    let scope_owned = scope.map(|s| s.to_string());

    let args = match diff_base_args(&scope_owned) {
        Ok(a) => a,
        Err(_) => {
            return DiffStats {
                additions: 0,
                deletions: 0,
            };
        }
    };
    let mut args = args;
    args.push("--shortstat".into());

    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    if let Some(out) = git_cmd(&repo_path).args(&args_str).run_silent() {
        let mut additions = 0;
        let mut deletions = 0;

        for part in out.stdout.split(',') {
            let part = part.trim();
            if part.contains("insertion") {
                if let Some(num) = part.split_whitespace().next() {
                    additions = num.parse().unwrap_or(0);
                }
            } else if part.contains("deletion")
                && let Some(num) = part.split_whitespace().next()
            {
                deletions = num.parse().unwrap_or(0);
            }
        }

        return DiffStats {
            additions,
            deletions,
        };
    }

    DiffStats {
        additions: 0,
        deletions: 0,
    }
}

/// Get diff stats (additions/deletions) for a repository
#[cfg(test)]
pub async fn get_diff_stats(path: String, scope: Option<String>) -> Result<DiffStats, String> {
    tokio::task::spawn_blocking(move || get_diff_stats_blocking(path, scope))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// Get list of changed files with status and stats
#[cfg(test)]
pub async fn get_changed_files(
    path: String,
    scope: Option<String>,
) -> Result<Vec<ChangedFile>, String> {
    tokio::task::spawn_blocking(move || get_changed_files_blocking(path, scope))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// Null device path — `/dev/null` on Unix, `NUL` on Windows
#[cfg(not(windows))]
const NULL_DEVICE: &str = "/dev/null";
#[cfg(windows)]
const NULL_DEVICE: &str = "NUL";

/// Get diff for a single file.
/// When `untracked` is `Some(true)`, skip the `ls-files` probe and go directly
/// to `--no-index` diff (the frontend already knows the file status).
#[cfg(test)]
pub async fn get_file_diff(
    path: String,
    file: String,
    scope: Option<String>,
    untracked: Option<bool>,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || get_file_diff_blocking(path, file, scope, untracked))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// Per-line git change status for the editor gutter / scrollbar overview ruler.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GutterChangeType {
    Added,
    Modified,
    Deleted,
}

/// A single gutter marker: a 1-based line in the *new* file and its status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GutterChange {
    /// 1-based line number in the new (current) file.
    pub line: u32,
    #[serde(rename = "type")]
    pub change_type: GutterChangeType,
}

/// Parse the `+NNN` new-file start line from a unified-diff hunk header
/// (`@@ -a,b +c,d @@`). Returns `0` when the header is malformed (mirrors the
/// frontend's prior regex, which left `newLine` at 0 on no match).
fn hunk_new_start(header: &str) -> u32 {
    let Some(plus) = header.find('+') else {
        return 0;
    };
    let rest = &header[plus + 1..];
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().unwrap_or(0)
}

/// Parse a unified diff (`git diff` output) into per-line gutter markers for the
/// new file. Classification per contiguous change block (a run of `-`/`+` lines
/// between context lines), matching the common gitgutter convention:
///   - only additions        → Added    (each new line)
///   - additions + deletions → Modified (the new lines that replaced old ones)
///   - only deletions        → Deleted  (one marker on the line that now sits
///     where the removed content was)
///
/// This is the Rust home of the parser that used to live in `gitGutter.ts`
/// (all business logic in Rust); the frontend only renders the returned markers.
pub fn parse_diff_to_changes(diff: &str) -> Vec<GutterChange> {
    let mut changes = Vec::new();
    if diff.is_empty() {
        return changes;
    }

    let mut new_line: u32 = 0; // 1-based line in the new file at the cursor
    let mut in_hunk = false;
    let mut del_count: u32 = 0; // consecutive deletions in the current block
    let mut add_count: u32 = 0; // consecutive additions in the current block
    let mut add_start: u32 = 0; // new_line where the current addition run began

    let flush = |changes: &mut Vec<GutterChange>,
                 add_count: &mut u32,
                 del_count: &mut u32,
                 add_start: u32,
                 new_line: u32| {
        if *add_count > 0 {
            let change_type = if *del_count > 0 {
                GutterChangeType::Modified
            } else {
                GutterChangeType::Added
            };
            for i in 0..*add_count {
                changes.push(GutterChange {
                    line: add_start + i,
                    change_type: change_type.clone(),
                });
            }
        } else if *del_count > 0 {
            // Pure deletion: mark the line now sitting where content was removed.
            changes.push(GutterChange {
                line: new_line,
                change_type: GutterChangeType::Deleted,
            });
        }
        *del_count = 0;
        *add_count = 0;
    };

    for raw in diff.split('\n') {
        // A new file section resets hunk tracking (multi-file diffs).
        if raw.starts_with("diff --git") {
            flush(
                &mut changes,
                &mut add_count,
                &mut del_count,
                add_start,
                new_line,
            );
            in_hunk = false;
            continue;
        }
        if raw.starts_with("@@") {
            flush(
                &mut changes,
                &mut add_count,
                &mut del_count,
                add_start,
                new_line,
            );
            new_line = hunk_new_start(raw);
            in_hunk = true;
            continue;
        }
        if !in_hunk {
            continue;
        }

        match raw.as_bytes().first() {
            Some(b' ') => {
                flush(
                    &mut changes,
                    &mut add_count,
                    &mut del_count,
                    add_start,
                    new_line,
                );
                new_line += 1;
            }
            Some(b'+') => {
                if add_count == 0 {
                    add_start = new_line;
                }
                add_count += 1;
                new_line += 1;
            }
            Some(b'-') => {
                // An addition run ending in a deletion means two separate blocks.
                if add_count > 0 {
                    flush(
                        &mut changes,
                        &mut add_count,
                        &mut del_count,
                        add_start,
                        new_line,
                    );
                }
                del_count += 1;
            }
            // "\ No newline at end of file" — not a content line.
            Some(b'\\') => {}
            _ => flush(
                &mut changes,
                &mut add_count,
                &mut del_count,
                add_start,
                new_line,
            ),
        }
    }
    flush(
        &mut changes,
        &mut add_count,
        &mut del_count,
        add_start,
        new_line,
    );
    changes
}

/// Editor gutter / scrollbar-overview change markers for a single file vs a git
/// scope (default "head"). Returns structured per-line markers — the unified
/// diff is produced and parsed entirely in Rust; the frontend only renders.
///
/// An untracked file has no committed version to mark against: every line
/// would be "added", which says nothing and costs one marker per line (409k
/// on a 23 MB JSON, measured 2026-09-06). It gets no markers.
/// The diff viewer (`get_file_diff`) still shows such a file as all added.
#[cfg(test)]
pub async fn get_gutter_changes(
    path: String,
    file: String,
    scope: Option<String>,
) -> Result<Vec<GutterChange>, String> {
    tokio::task::spawn_blocking(move || get_gutter_changes_blocking(path, file, scope))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// Generate 2-character initials from a repository name
pub fn get_repo_initials(name: &str) -> String {
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
pub fn get_initials(name: String) -> String {
    get_repo_initials(&name)
}

/// Canonical list of branch names considered "main" / primary.
/// Used by both `is_main_branch()` and `get_merged_branches_impl()`.
pub const MAIN_BRANCH_CANDIDATES: &[&str] = &["main", "master", "develop", "development", "dev"];

/// Check if a branch name is a main/primary branch
pub fn is_main_branch(branch_name: &str) -> bool {
    MAIN_BRANCH_CANDIDATES.contains(&branch_name.to_lowercase().as_str())
}

/// Check if a branch name is a main/primary branch (Tauri command)
pub fn check_is_main_branch(branch: String) -> bool {
    is_main_branch(&branch)
}

/// Sort branches: main/primary branches first, then alphabetical by name
pub fn sort_branches(branches: &mut [serde_json::Value]) {
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
    // Refs live in the common dir for linked worktrees (identical for normal
    // repos), so resolve them there.
    let refs_dir = common_git_dir(git_dir);
    let git_dir = refs_dir.as_path();
    // 1. Check well-known candidate names in local refs
    if let Some(name) = MAIN_BRANCH_CANDIDATES.iter().find(|name| {
        git_dir.join("refs/heads").join(name).exists()
            || packed_ref_exists(git_dir, &format!("refs/heads/{name}"))
    }) {
        return Some(name.to_string());
    }

    // 2. Check well-known candidate names in remote tracking refs (CI detached-HEAD: no local
    //    branches exist, only refs/remotes/origin/* which may live in packed-refs)
    if let Some(name) = MAIN_BRANCH_CANDIDATES.iter().find(|name| {
        git_dir.join("refs/remotes/origin").join(name).exists()
            || packed_ref_exists(git_dir, &format!("refs/remotes/origin/{name}"))
    }) {
        return Some(format!("origin/{name}"));
    }

    // 3. Read origin/HEAD symref (e.g. "ref: refs/remotes/origin/main\n")
    let origin_head = git_dir.join("refs/remotes/origin/HEAD");
    if let Ok(content) = fs::read_to_string(&origin_head)
        && let Some(target) = content.trim().strip_prefix("ref: refs/remotes/origin/")
    {
        let branch = target.trim();
        if git_dir.join("refs/heads").join(branch).exists()
            || packed_ref_exists(git_dir, &format!("refs/heads/{branch}"))
        {
            return Some(branch.to_string());
        }
        if git_dir.join("refs/remotes/origin").join(branch).exists()
            || packed_ref_exists(git_dir, &format!("refs/remotes/origin/{branch}"))
        {
            return Some(format!("origin/{branch}"));
        }
    }

    None
}

/// Local branches integrated in the default branch, using the same proof
/// classifier as MCP and lifecycle inspection. Same-tip branches stay hidden.
pub fn get_merged_branches_impl(repo_path: &Path) -> Result<Vec<String>, String> {
    let Some(git_dir) = resolve_git_dir(repo_path) else {
        return Ok(vec![]);
    };
    let Some(default_branch) = detect_default_branch(&git_dir) else {
        return Ok(vec![]);
    };
    use gix::bstr::ByteSlice;
    let grepo = gix::open(repo_path).map_err(|e| e.to_string())?;
    let main_tip = grepo
        .find_reference(&format!("refs/heads/{default_branch}"))
        .map_err(|e| e.to_string())?
        .peel_to_id()
        .map_err(|e| e.to_string())?
        .detach();
    let refs = grepo.references().map_err(|e| e.to_string())?;
    let mut merged = Vec::new();
    for reference in refs.prefixed("refs/heads/").map_err(|e| e.to_string())? {
        let mut reference = reference.map_err(|e| e.to_string())?;
        let branch = reference.name().shorten().to_str_lossy().into_owned();
        if branch == default_branch
            || reference.peel_to_id().map_err(|e| e.to_string())?.detach() == main_tip
        {
            continue;
        }
        let (status, _) = crate::worktree::monitoring_branch_merge(
            repo_path,
            &branch,
            &crate::worktree::monitoring_ref_key(repo_path, &branch)?,
            false,
            |_, _, _| false,
        )?;
        if status == crate::worktree::WorkspaceCommitStatus::Merged {
            merged.push(branch);
        }
    }
    Ok(merged)
}

/// Check whether a ref exists in .git/packed-refs (for repos that have been gc'd).
fn packed_ref_exists(git_dir: &Path, ref_name: &str) -> bool {
    let packed_refs = git_dir.join("packed-refs");
    fs::read_to_string(packed_refs)
        .map(|content| {
            content.lines().any(|line| {
                // Lines are "<sha> <ref>" or comments starting with '#'/'^'
                line.split_whitespace().nth(1) == Some(ref_name)
            })
        })
        .unwrap_or(false)
}

/// Get the unix timestamp of the last commit on each branch using a single
/// `git for-each-ref` call instead of N sequential `git log` subprocesses.
pub fn get_last_commit_timestamps(
    repo_path: &Path,
    branches: &[String],
) -> HashMap<String, Option<i64>> {
    let mut result: HashMap<String, Option<i64>> =
        branches.iter().map(|b| (b.clone(), None)).collect();

    let out = match git_cmd(repo_path)
        .args([
            "for-each-ref",
            "--format=%(refname:short)\t%(creatordate:unix)",
            "refs/heads/",
            "refs/remotes/",
        ])
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

/// Get git branches for a repository (Story 052)
#[cfg(test)]
pub async fn get_git_branches(path: String) -> Result<Vec<serde_json::Value>, String> {
    tokio::task::spawn_blocking(move || get_git_branches_blocking(path))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// Rich per-branch information returned by `get_branches_detail`.
#[derive(Debug, Clone, Serialize)]
pub struct BranchDetail {
    pub name: String,
    pub is_current: bool,
    pub is_remote: bool,
    pub is_main: bool,
    pub is_merged: bool,
    pub ahead: Option<u32>,
    pub behind: Option<u32>,
    pub upstream: Option<String>,
    pub last_commit_date: Option<String>,    // ISO 8601
    pub last_commit_message: Option<String>, // first line only
    pub last_commit_author: Option<String>,
    /// Commits ahead of the stored base branch (tuicommander-base)
    pub base_ahead: Option<u32>,
    /// Commits behind the stored base branch (tuicommander-base)
    pub base_behind: Option<u32>,
    /// The stored base branch name, if any
    pub base_branch: Option<String>,
}

/// Field separator used in `git for-each-ref` output.
/// Must not appear in branch names, commit messages, author names, or tracking info.
const BRANCH_FIELD_SEP: &str = "|||";

/// Core logic for fetching rich branch details (no Tauri state).
///
/// Uses `git for-each-ref` for a single-pass data collection across all local and
/// remote branches. Fields are delimited by `|||` which cannot appear in any of
/// the output fields (branch names, subjects, author names, or tracking tokens).
pub fn get_branches_detail_impl(path: &Path) -> Result<Vec<BranchDetail>, String> {
    // Each ref line: refname|||refname:short|||HEAD|||upstream:short|||upstream:track|||committerdate|||subject|||authorname
    let sep = BRANCH_FIELD_SEP;
    let fmt = format!(
        "%(refname){sep}%(refname:short){sep}%(HEAD){sep}%(upstream:short){sep}%(upstream:track){sep}%(committerdate:iso8601){sep}%(subject){sep}%(authorname)"
    );

    let out = git_cmd(path)
        .args([
            "for-each-ref",
            &format!("--format={fmt}"),
            "refs/heads/",
            "refs/remotes/",
        ])
        .run()
        .map_err(|e| format!("git for-each-ref failed: {e}"))?;

    // Collect merged branch names once so we can do O(1) lookups.
    let merged_set = merged_branch_set(path);

    let mut branches: Vec<BranchDetail> = out
        .stdout
        .lines()
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let parts: Vec<&str> = line.splitn(8, BRANCH_FIELD_SEP).collect();
            if parts.len() < 8 {
                return None;
            }

            let refname = parts[0].trim();
            let name = parts[1].trim().to_string();

            // Skip a remote's HEAD symref (refs/remotes/<remote>/HEAD, e.g.
            // refs/remotes/origin/HEAD). This must be checked against the FULL
            // refname, not the short name: git's %(refname:short) collapses this
            // symref all the way down to just the remote name itself (e.g. "origin",
            // not "origin/HEAD"), for ANY remote name — confirmed empirically with a
            // non-origin remote too. The old `name.ends_with("/HEAD")` check on the
            // short name never actually matched this ref and let a phantom branch
            // entry named after the remote (e.g. "origin") leak into every normally
            // cloned repo's results.
            if refname.starts_with("refs/remotes/") && refname.ends_with("/HEAD") {
                return None;
            }

            let is_current = parts[2].trim() == "*";
            // Use the full refname (refs/remotes/…) to reliably detect remote branches,
            // avoiding false positives from local branches with slashes (e.g. feature/auth).
            let is_remote = refname.starts_with("refs/remotes/");

            let upstream_raw = parts[3].trim();
            let upstream = if upstream_raw.is_empty() {
                None
            } else {
                Some(upstream_raw.to_string())
            };

            // upstream:track looks like "[ahead 2, behind 3]", "[ahead 1]", "[behind 4]", or ""
            let track = parts[4].trim();
            let ahead = parse_track_value(track, "ahead");
            let behind = parse_track_value(track, "behind");

            let commit_date_raw = parts[5].trim();
            let last_commit_date = if commit_date_raw.is_empty() {
                None
            } else {
                Some(commit_date_raw.to_string())
            };

            let subject = parts[6].trim();
            let last_commit_message = if subject.is_empty() {
                None
            } else {
                Some(subject.to_string())
            };

            let author = parts[7].trim();
            let last_commit_author = if author.is_empty() {
                None
            } else {
                Some(author.to_string())
            };

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
                base_ahead: None,
                base_behind: None,
                base_branch: None,
            })
        })
        .collect();

    apply_base_ahead_behind_and_sort(path, &mut branches);
    Ok(branches)
}

/// Set of branch names integrated into main, used for `is_merged`.
/// Empty (with a warning) if the merge check fails. Shared by both adapters.
pub fn merged_branch_set(path: &Path) -> std::collections::HashSet<String> {
    match get_merged_branches_impl(path) {
        Ok(names) => names.into_iter().collect(),
        Err(e) => {
            tracing::warn!(source = "git", "Failed to determine merged branches: {e}");
            std::collections::HashSet::new()
        }
    }
}

/// Backend-agnostic tail for `branches_detail`: fill base ahead/behind for local
/// branches that have a `tuicommander-base` set, then sort (main first, then
/// alphabetical). Routed through the GitReads port so it auto-upgrades with the
/// ahead/behind backend. Shared by the CLI and gix adapters for byte parity.
pub fn apply_base_ahead_behind_and_sort(path: &Path, branches: &mut [BranchDetail]) {
    let path_str = path.to_string_lossy();
    // One `git config --get-regexp` for all branch bases, not one subprocess
    // per local branch.
    let bases = crate::worktree::get_branch_bases(&path_str);
    for branch in branches.iter_mut() {
        if branch.is_remote {
            continue;
        }
        if let Some(base) = bases.get(&branch.name) {
            let base = base.clone();
            // left=base, right=branch, so (left-not-right, right-not-left) =
            // (base_behind, base_ahead).
            if let Ok((behind, ahead)) = git_reads().ahead_behind(path, &base, &branch.name) {
                branch.base_behind = Some(behind);
                branch.base_ahead = Some(ahead);
            }
            branch.base_branch = Some(base);
        }
    }

    branches.sort_by(|a, b| match (a.is_main, b.is_main) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.cmp(&b.name),
    });
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
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// Core logic for fetching recently checked-out branch names from the reflog.
///
/// Parses `git reflog show --format='%gs' -n 100`, extracts the target branch
/// from each `checkout: moving from X to Y` line, deduplicates preserving
/// order (most recent first), and returns up to `limit` entries.
pub fn get_recent_branches_impl(path: &Path, limit: usize) -> Result<Vec<String>, String> {
    let output = git_cmd(path)
        .args(["reflog", "show", "--format=%gs", "-n", "100"])
        .run()
        .map_err(|e| e.to_string())?;

    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();

    for line in output.stdout.lines() {
        // Match "checkout: moving from <from> to <to>"
        if let Some(rest) = line.strip_prefix("checkout: moving from ")
            && let Some(to_pos) = rest.rfind(" to ")
        {
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

    Ok(result)
}

/// Get recently checked-out branch names for a repository (most recent first).
#[cfg(test)]
pub async fn get_recent_branches(
    path: String,
    limit: Option<usize>,
) -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(move || get_recent_branches_blocking(path, limit))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// Core logic for fetching git panel context (no caching, no Tauri state).
/// CLI status counts via `git status --porcelain=v2` (staged/changed/conflict).
/// Untracked entries count toward `changed` (matches the panel's prior behavior).
pub fn status_counts_cli(path: &Path) -> crate::git_reads::StatusCounts {
    let Some(out) = git_cmd(path)
        // `--untracked-files=all` recurses untracked directories so the badge
        // count matches the per-file expansion in get_working_tree_status (a
        // collapsed `? dir/` would otherwise count as 1 while the panel lists N).
        .args(["status", "--porcelain=v2", "--untracked-files=all"])
        .run_silent()
    else {
        // An unreadable repo is not a clean one. Reporting "clean" here is a
        // silent failure the sidebar renders as a green tick, and it is the
        // verdict `get_repo_info` shows too.
        return crate::git_reads::StatusCounts {
            status: "unknown".to_string(),
            staged: 0,
            changed: 0,
        };
    };
    status_counts_from_porcelain_v2(&out.stdout)
}

/// Fold `status --porcelain=v2` output into the staged/changed counts and the
/// clean/dirty/conflict verdict. The single place those semantics are defined:
/// the repo badge, the git panel and the CLI fallback all read it from here.
fn status_counts_from_porcelain_v2(porcelain: &str) -> crate::git_reads::StatusCounts {
    let mut staged = 0u32;
    let mut changed = 0u32;
    let mut has_conflict = false;

    for line in porcelain.lines() {
        if let Some(rest) = line.strip_prefix("1 ").or_else(|| line.strip_prefix("2 ")) {
            // Ordinary ("1 XY ...") or rename/copy ("2 XY ...") entry.
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

    let status = if has_conflict {
        "conflict"
    } else if staged > 0 || changed > 0 {
        "dirty"
    } else {
        "clean"
    }
    .to_string();

    crate::git_reads::StatusCounts {
        status,
        staged,
        changed,
    }
}

/// CLI ahead/behind via `git rev-list --left-right --count <left>...<right>`.
/// Returns `(ahead, behind)` = (commits in left not right, in right not left).
pub fn ahead_behind_cli(repo: &Path, left: &str, right: &str) -> Result<(u32, u32), String> {
    let range = format!("{left}...{right}");
    let out = git_cmd(repo)
        .args(["rev-list", "--left-right", "--count", &range])
        .run()
        .map_err(|e| e.to_string())?;
    let parts: Vec<&str> = out.stdout.trim().split('\t').collect();
    if parts.len() == 2
        && let (Ok(a), Ok(b)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>())
    {
        return Ok((a, b));
    }
    Err(format!("unexpected rev-list output: {:?}", out.stdout))
}

// --- Working tree status (porcelain v2) ---

/// A single staged or unstaged file entry.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct StatusEntry {
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
pub struct WorkingTreeStatus {
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub stash_count: u32,
    pub staged: Vec<StatusEntry>,
    pub unstaged: Vec<StatusEntry>,
    pub untracked: Vec<String>,
    /// Unmerged (conflicted) files from `u` porcelain-v2 records. `status` holds
    /// the two-char conflict code (e.g. "UU", "AA", "DD").
    pub conflicted: Vec<StatusEntry>,
}

/// Parse porcelain v2 output into a `WorkingTreeStatus`.
pub fn parse_porcelain_v2(output: &str) -> WorkingTreeStatus {
    let mut branch: Option<String> = None;
    let mut upstream: Option<String> = None;
    let mut ahead: u32 = 0;
    let mut behind: u32 = 0;
    let mut stash_count: u32 = 0;
    let mut staged = Vec::new();
    let mut unstaged = Vec::new();
    let mut untracked = Vec::new();
    let mut conflicted = Vec::new();

    for line in output.lines() {
        if let Some(rest) = line.strip_prefix("# branch.head ") {
            branch = if rest == "(detached)" {
                None
            } else {
                Some(rest.to_string())
            };
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
        } else if let Some(rest) = line.strip_prefix("u ") {
            // Unmerged entry: u <XY> <sub> <m1> <m2> <m3> <mW> <h1> <h2> <h3> <path>
            parse_unmerged_entry(rest, &mut conflicted);
        }
    }

    WorkingTreeStatus {
        branch,
        upstream,
        ahead,
        behind,
        stash_count,
        staged,
        unstaged,
        untracked,
        conflicted,
    }
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
fn parse_ordinary_entry(
    rest: &str,
    staged: &mut Vec<StatusEntry>,
    unstaged: &mut Vec<StatusEntry>,
) {
    // Fields are space-separated: XY sub mH mI mW hH hI path
    // We need XY (index 0), sub (index 1), and path (index 7)
    let fields: Vec<&str> = rest.splitn(8, ' ').collect();
    if fields.len() < 8 {
        return;
    }
    // Skip submodule entries (sub field starts with 'S')
    if fields[1].starts_with('S') {
        return;
    }
    let xy = fields[0];
    let path = fields[7].to_string();
    let mut chars = xy.chars();
    let x = chars.next().unwrap_or('.');
    let y = chars.next().unwrap_or('.');
    if let Some(code) = status_char_to_code(x) {
        staged.push(StatusEntry {
            path: path.clone(),
            status: code.to_string(),
            original_path: None,
            additions: 0,
            deletions: 0,
        });
    }
    if let Some(code) = status_char_to_code(y) {
        unstaged.push(StatusEntry {
            path,
            status: code.to_string(),
            original_path: None,
            additions: 0,
            deletions: 0,
        });
    }
}

/// Parse a rename/copy (type 2) porcelain v2 entry.
fn parse_rename_entry(rest: &str, staged: &mut Vec<StatusEntry>, unstaged: &mut Vec<StatusEntry>) {
    // Fields: XY sub mH mI mW hH hI Xscore path\torigPath
    // 9 space-separated fields, but last contains tab-separated path pair
    let fields: Vec<&str> = rest.splitn(9, ' ').collect();
    if fields.len() < 9 {
        return;
    }
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
        staged.push(StatusEntry {
            path: path.clone(),
            status: code.to_string(),
            original_path: orig.clone(),
            additions: 0,
            deletions: 0,
        });
    }
    if let Some(code) = status_char_to_code(y) {
        unstaged.push(StatusEntry {
            path,
            status: code.to_string(),
            original_path: orig,
            additions: 0,
            deletions: 0,
        });
    }
}

/// Parse an unmerged/conflicted (type `u`) porcelain v2 entry.
fn parse_unmerged_entry(rest: &str, conflicted: &mut Vec<StatusEntry>) {
    // Fields: XY sub m1 m2 m3 mW h1 h2 h3 path (10 space-separated fields)
    let fields: Vec<&str> = rest.splitn(10, ' ').collect();
    if fields.len() < 10 {
        return;
    }
    conflicted.push(StatusEntry {
        path: fields[9].to_string(),
        status: fields[0].to_string(),
        original_path: None,
        additions: 0,
        deletions: 0,
    });
}

/// Parse `git diff --numstat` output into a path→(additions, deletions) map.
fn parse_numstat(output: &str) -> HashMap<String, (u32, u32)> {
    let mut map = HashMap::new();
    for line in output.lines() {
        let parts: Vec<&str> = line.splitn(3, '\t').collect();
        if parts.len() != 3 {
            continue;
        }
        // Binary files show "-" for additions/deletions
        let add = parts[0].parse::<u32>().unwrap_or(0);
        let del = parts[1].parse::<u32>().unwrap_or(0);
        map.insert(parts[2].to_string(), (add, del));
    }
    map
}

/// Enrich status entries with line counts from `git diff --numstat`.
pub fn enrich_with_numstat(repo_path: &Path, entries: &mut [StatusEntry], staged: bool) {
    let mut args = vec!["diff", "--numstat"];
    if staged {
        args.push("--cached");
    }
    let Ok(out) = git_cmd(repo_path).args(&args).run() else {
        return;
    };
    let stats = parse_numstat(&out.stdout);
    for entry in entries.iter_mut() {
        if let Some(&(add, del)) = stats.get(&entry.path) {
            entry.additions = add;
            entry.deletions = del;
        }
    }
}

/// Test-only counter of how many times the porcelain read actually forked git.
/// Proves the single-flight in `get_working_tree_status` collapses callers
/// instead of merely returning the same value.
///
/// Keyed by repo path, not process-wide: the suite runs tests in parallel, and a
/// single counter let any other git test's compute land between the reset and the
/// assertion — the test failed on load, not on a regression.
#[cfg(test)]
static WT_STATUS_COMPUTES: std::sync::LazyLock<dashmap::DashMap<String, usize>> =
    std::sync::LazyLock::new(dashmap::DashMap::new);

/// How many times `path` was actually read. See [`WT_STATUS_COMPUTES`].
#[cfg(test)]
pub fn wt_status_computes(path: &str) -> usize {
    WT_STATUS_COMPUTES.get(path).map(|n| *n).unwrap_or(0)
}

/// Read the working tree status: one `git status --porcelain=v2` plus one
/// `git diff --numstat` per side. Three subprocesses.
#[cfg(test)]
async fn compute_working_tree_status(path: String) -> Result<WorkingTreeStatus, String> {
    *WT_STATUS_COMPUTES.entry(path.clone()).or_insert(0) += 1;
    tokio::task::spawn_blocking(move || compute_working_tree_status_blocking(path))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// Working-tree generation per repo, bumped by every mutating command here.
static WT_STATUS_EPOCH: std::sync::LazyLock<dashmap::DashMap<String, u64>> =
    std::sync::LazyLock::new(dashmap::DashMap::new);

/// Mark the working tree of `path` as changed, so no later read joins one that
/// started before the change. Call after the mutation has completed.
fn bump_working_tree_epoch(path: &str) {
    *WT_STATUS_EPOCH.entry(path.to_string()).or_insert(0) += 1;
}

pub fn working_tree_epoch(path: &str) -> u64 {
    WT_STATUS_EPOCH.get(path).map(|e| *e).unwrap_or(0)
}

#[cfg(test)]
mod single_flight_test_only {
    use super::*;
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

    static WT_STATUS_IN_FLIGHT: std::sync::LazyLock<
        dashmap::DashMap<WtStatusKey, WtStatusPublisher>,
    > = std::sync::LazyLock::new(dashmap::DashMap::new);

    /// Either this call leads the read for its generation, or it waits for the one
    /// already running.
    pub(super) enum Flight {
        Lead(WtStatusLeader),
        Follow(tokio::sync::broadcast::Receiver<Result<WorkingTreeStatus, String>>),
    }

    /// Join the read in flight for the current generation of `path`, or become its
    /// leader.
    pub(super) fn enter_flight(path: &str) -> Flight {
        use dashmap::mapref::entry::Entry;

        let key: WtStatusKey = (path.to_string(), working_tree_epoch(path));
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
    pub(super) struct WtStatusLeader(WtStatusKey);

    impl Drop for WtStatusLeader {
        fn drop(&mut self) {
            WT_STATUS_IN_FLIGHT.remove(&self.0);
        }
    }

    /// Get full working tree status from porcelain v2 output.
    ///
    /// ChangesTab and StatusBar both refetch on every repo-revision bump, so one
    /// `repo-changed` would fork six git processes for the same answer. Concurrent
    /// callers for a repo share the leader's single read.
    pub async fn get_working_tree_status(path: String) -> Result<WorkingTreeStatus, String> {
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
}
#[cfg(test)]
pub use single_flight_test_only::get_working_tree_status;
#[cfg(test)]
use single_flight_test_only::{Flight, enter_flight};
// --- Stage / unstage / discard ---

/// Validate that all file paths stay within the repo root.
/// Returns an error message if any path escapes.
fn validate_paths_within_repo(repo_path: &Path, files: &[String]) -> Result<(), String> {
    let canonical_repo = repo_path
        .canonicalize()
        .map_err(|e| format!("Failed to resolve repo path: {e}"))?;
    for file in files {
        // Reject absolute paths — all file args must be relative to repo root
        if tuic_core::path_spelling::is_absolute_on_any_platform(file) {
            return Err(format!(
                "Access denied: absolute path '{}' not allowed",
                file
            ));
        }
        let full = repo_path.join(file);
        // For files that don't exist yet (e.g. deleted), canonicalize will fail.
        // In that case, do a manual check: normalize the joined path and verify prefix.
        match full.canonicalize() {
            Ok(canonical) => {
                if !canonical.starts_with(&canonical_repo) {
                    return Err(format!(
                        "Access denied: path '{}' is outside repository",
                        file
                    ));
                }
            }
            Err(_) => {
                // File doesn't exist on disk — validate the relative path doesn't escape.
                // We already rejected absolute paths above, so `file` is relative.
                // Walk its components: any upward traversal above the root is rejected.
                let mut depth: usize = 0;
                for component in Path::new(file).components() {
                    match component {
                        std::path::Component::Normal(_) => {
                            depth += 1;
                        }
                        std::path::Component::ParentDir => {
                            if depth == 0 {
                                return Err(format!(
                                    "Access denied: path '{}' is outside repository",
                                    file
                                ));
                            }
                            depth -= 1;
                        }
                        std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                            return Err(format!(
                                "Access denied: path '{}' is outside repository",
                                file
                            ));
                        }
                        std::path::Component::CurDir => {}
                    }
                }
            }
        }
    }
    Ok(())
}

/// Wrap a failed git-command error with a stale-lock diagnosis when one
/// explains it (naming the lock file and its age), falling back to the
/// original `context: {e}` message otherwise. See
/// [`crate::git_locks::describe_stale_lock`] — detection is read-only, so
/// this only ever makes a failure *more* specific, never invents a cause.
fn diagnose_git_failure(repo_path: &Path, context: &str, e: impl std::fmt::Display) -> String {
    crate::git_locks::describe_stale_lock(repo_path).unwrap_or_else(|| format!("{context}: {e}"))
}

/// Stage files (`git add -- <files>`).
#[cfg(test)]
pub async fn git_stage_files(path: String, files: Vec<String>) -> Result<(), String> {
    tokio::task::spawn_blocking(move || git_stage_files_blocking(path, files))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// Unstage files (`git restore --staged -- <files>`).
#[cfg(test)]
pub async fn git_unstage_files(path: String, files: Vec<String>) -> Result<(), String> {
    tokio::task::spawn_blocking(move || git_unstage_files_blocking(path, files))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// Discard working tree changes (`git restore -- <files>`). Destructive!
#[cfg(test)]
pub async fn git_discard_files(path: String, files: Vec<String>) -> Result<(), String> {
    tokio::task::spawn_blocking(move || git_discard_files_blocking(path, files))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

// --- Hunk-level discard / unstage via reverse patch ---

/// Apply a unified diff patch in reverse to revert specific hunks.
///
/// - `scope = None` → working tree discard (`git apply --reverse`)
/// - `scope = Some("staged")` → unstage from index (`git apply --reverse --cached`)
///
/// The `patch` must be a valid unified diff (starting with `diff --git` or `---`).
/// Pipe the patch via stdin to avoid temp files.
#[cfg(test)]
pub async fn git_apply_reverse_patch(
    path: String,
    patch: String,
    scope: Option<String>,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || git_apply_reverse_patch_blocking(path, patch, scope))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

// --- git commit ---

/// Commit staged changes and return the new commit hash.
#[cfg(test)]
pub async fn git_commit(
    path: String,
    message: String,
    amend: Option<bool>,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || git_commit_blocking(path, message, amend))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

// --- Commit log, stash, file history, blame commands ---

/// A commit log entry with full metadata for the GitLens-style panel.
#[derive(Debug, Clone, Serialize)]
pub struct CommitLogEntry {
    pub hash: String,
    pub parents: Vec<String>,
    pub refs: Vec<String>,
    pub author_name: String,
    pub author_date: String,
    pub subject: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

/// Parse a NUL-delimited commit log line into a `CommitLogEntry`.
fn parse_commit_log_line(line: &str) -> Option<CommitLogEntry> {
    let parts: Vec<&str> = line.splitn(7, '\0').collect();
    if parts.len() < 6 {
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
    let body = parts.get(6).and_then(|b| {
        let trimmed = b.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    });
    Some(CommitLogEntry {
        hash: parts[0].to_string(),
        parents,
        refs,
        author_name: parts[3].to_string(),
        author_date: parts[4].to_string(),
        subject: parts[5].to_string(),
        body,
    })
}

const COMMIT_LOG_FORMAT: &str = "%x1e%H%x00%P%x00%D%x00%an%x00%aI%x00%s%x00%b";
pub const COMMIT_LOG_MAX_COUNT: u32 = 500;
pub const COMMIT_LOG_DEFAULT_COUNT: u32 = 50;

/// Validate a git object hash (4-40 hex chars). Prevents injection via `after` parameters.
fn validate_git_hash(hash: &str) -> Result<(), String> {
    if hash.len() < 4 || hash.len() > 40 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("Invalid git hash: '{hash}'"));
    }
    Ok(())
}

/// Sync implementation of commit log retrieval.
pub fn get_commit_log_impl(
    path: String,
    count: Option<u32>,
    after: Option<String>,
) -> Result<Vec<CommitLogEntry>, String> {
    let repo_path = PathBuf::from(&path);
    let n = count
        .unwrap_or(COMMIT_LOG_DEFAULT_COUNT)
        .min(COMMIT_LOG_MAX_COUNT);
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
        .split('\x1e')
        .filter(|s| !s.is_empty())
        .filter_map(|s| parse_commit_log_line(s.trim_matches('\n')))
        .collect();

    Ok(commits)
}

/// Get paginated commit log with full metadata.
#[cfg(test)]
pub async fn get_commit_log(
    path: String,
    count: Option<u32>,
    after: Option<String>,
) -> Result<Vec<CommitLogEntry>, String> {
    tokio::task::spawn_blocking(move || get_commit_log_blocking(path, count, after))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// A stash entry.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StashEntry {
    pub index: u32,
    pub ref_name: String,
    pub message: String,
    pub hash: String,
}

/// List all stash entries.
#[cfg(test)]
pub async fn get_stash_list(path: String) -> Result<Vec<StashEntry>, String> {
    tokio::task::spawn_blocking(move || get_stash_list_blocking(path))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
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
#[cfg(test)]
pub async fn git_stash_apply(path: String, stash_ref: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || git_stash_apply_blocking(path, stash_ref))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// Apply and remove a stash.
#[cfg(test)]
pub async fn git_stash_pop(path: String, stash_ref: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || git_stash_pop_blocking(path, stash_ref))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// Drop (delete) a stash.
#[cfg(test)]
pub async fn git_stash_drop(path: String, stash_ref: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || git_stash_drop_blocking(path, stash_ref))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// Show diff for a stash entry.
#[cfg(test)]
pub async fn git_stash_show(path: String, stash_ref: String) -> Result<String, String> {
    tokio::task::spawn_blocking(move || git_stash_show_blocking(path, stash_ref))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// Get commit log for a specific file, following renames.
#[cfg(test)]
pub async fn get_file_history(
    path: String,
    file: String,
    count: Option<u32>,
    after: Option<String>,
) -> Result<Vec<CommitLogEntry>, String> {
    tokio::task::spawn_blocking(move || get_file_history_blocking(path, file, count, after))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// A single blame line with commit metadata.
#[derive(Debug, Clone, Serialize)]
pub struct BlameLine {
    pub hash: String,
    pub author: String,
    pub author_time: i64,
    /// First line of the commit message (the subject). Shown in the editor's
    /// inline-blame annotation; empty for commits with no message.
    pub summary: String,
    pub line_number: u32,
    pub content: String,
}

/// Parse `git blame --porcelain` output into `BlameLine` entries.
fn parse_blame_porcelain(output: &str) -> Vec<BlameLine> {
    let mut lines = Vec::new();
    let mut current_hash = String::new();
    let mut current_line_number: u32 = 0;

    // Cache commit metadata (author, author-time, summary) to avoid re-parsing for
    // consecutive lines from the same commit.
    let mut commit_cache: HashMap<String, (String, i64, String)> = HashMap::new();

    let mut author = String::new();
    let mut author_time: i64 = 0;
    let mut summary = String::new();
    let mut expecting_hash = true; // true when the next non-header line should be a commit hash

    for line in output.lines() {
        if let Some(content) = line.strip_prefix('\t') {
            // Content line — finalize this blame entry
            let (cached_author, cached_time, cached_summary) = commit_cache
                .entry(current_hash.clone())
                .or_insert_with(|| (author.clone(), author_time, summary.clone()));

            lines.push(BlameLine {
                hash: current_hash.clone(),
                author: cached_author.clone(),
                author_time: *cached_time,
                summary: cached_summary.clone(),
                line_number: current_line_number,
                content: content.to_string(),
            });

            expecting_hash = true;
        } else if expecting_hash
            && line.len() >= 40
            && line
                .as_bytes()
                .iter()
                .take(40)
                .all(|b| b.is_ascii_hexdigit())
        {
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
                summary.clear();
                expecting_hash = false;
            }
        } else if let Some(rest) = line.strip_prefix("author ") {
            author = rest.to_string();
        } else if let Some(rest) = line.strip_prefix("author-time ") {
            author_time = rest.parse().unwrap_or(0);
        } else if let Some(rest) = line.strip_prefix("summary ") {
            summary = rest.to_string();
        }
    }

    lines
}

/// Get per-line blame information for a file.
/// True if `file`'s history contains a rename. `git blame` (and gix blame)
/// without -C/-M stop at the rename boundary, but git follows the path's rename
/// in history while gix does not — so a renamed file must use the CLI blame for
/// matching per-line attribution. Detected via `git log --follow --diff-filter=R`.
pub fn file_history_has_rename(repo: &Path, file: &str) -> bool {
    git_cmd(repo)
        .args([
            "log",
            "--follow",
            "--diff-filter=R",
            "--format=%H",
            "--",
            file,
        ])
        .run_silent()
        .map(|o| !o.stdout.trim().is_empty())
        .unwrap_or(false)
}

/// Whether git tracks `file` (`ls-files --error-unmatch`). A non-zero exit is
/// "untracked"; a git that could not run at all is an error, not an answer.
pub fn is_file_tracked(repo: &Path, file: &str) -> Result<bool, String> {
    match git_cmd(repo)
        .args(["ls-files", "--error-unmatch", file])
        .run()
    {
        Ok(_) => Ok(true),
        Err(crate::git_cli::GitError::NonZeroExit { .. }) => Ok(false),
        Err(e) => Err(format!("Failed to check file tracking status: {e}")),
    }
}

/// Verify `file` is tracked, returning a friendly error if not (instead of
/// git's cryptic "no such path in HEAD"). Shared by both blame adapters.
pub fn ensure_file_tracked(repo: &Path, file: &str) -> Result<(), String> {
    if !is_file_tracked(repo, file)? {
        return Err(format!(
            "File is not tracked by git — blame unavailable: {file}"
        ));
    }
    Ok(())
}

/// CLI blame via `git blame --porcelain`. Verifies the file is tracked first so
/// callers get a clear error instead of git's cryptic "no such path in HEAD".
pub fn blame_cli(repo: &Path, file: &str) -> Result<Vec<BlameLine>, String> {
    ensure_file_tracked(repo, file)?;

    let out = git_cmd(repo)
        .args(["blame", "--porcelain", file])
        .run()
        .map_err(|e| format!("git blame failed: {e}"))?;

    Ok(parse_blame_porcelain(&out.stdout))
}

#[cfg(test)]
pub async fn get_file_blame(path: String, file: String) -> Result<Vec<BlameLine>, String> {
    tokio::task::spawn_blocking(move || get_file_blame_blocking(path, file))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::setup_test_repo_with_commit;

    // Catches blocking adapters returning empty/default data and losing numstat
    // counts, even though the lower-level parsers still pass their unit tests.
    #[test]
    fn blocking_reads_expose_real_commits_branches_and_diff_counts_1451() {
        let (_temp, repo) = empty_fixture_repo();
        assert!(
            repo.as_path()
                .canonicalize()
                .unwrap()
                .starts_with(tuic_test_support::test_temp_root().canonicalize().unwrap())
        );
        std::fs::write(repo.join("tracked.txt"), "before\n").unwrap();
        git_in(&repo, &["add", "tracked.txt"]);
        git_in(&repo, &["commit", "-qm", "adapter baseline"]);
        git_in(
            &repo,
            &[
                "remote",
                "add",
                "origin",
                "https://example.invalid/fixture.git",
            ],
        );
        crate::worktree::set_branch_base(&repo.to_string_lossy(), "main", "develop").unwrap();
        std::fs::write(repo.join("tracked.txt"), "after\nextra\n").unwrap();
        let path = repo.to_string_lossy().into_owned();
        assert_eq!(
            get_remote_url_blocking(path.clone()).unwrap().as_deref(),
            Some("https://example.invalid/fixture.git")
        );
        assert_eq!(
            get_branch_base_blocking(path.clone(), "main".into())
                .unwrap()
                .as_deref(),
            Some("develop")
        );
        let commits = get_recent_commits_blocking(path.clone(), Some(1)).unwrap();
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].subject, "adapter baseline");
        assert_eq!(
            commits[0].hash,
            git_in(&repo, &["rev-parse", "HEAD"]).trim()
        );
        let branches = get_git_branches_blocking(path.clone()).unwrap();
        let main = branches
            .iter()
            .find(|branch| branch["name"] == "main")
            .unwrap();
        assert_eq!(main["is_current"], true);
        assert_eq!(main["is_main"], true);
        assert_eq!(main["is_remote"], false);
        let diff = get_git_diff_blocking(path.clone(), None).unwrap();
        assert!(diff.contains("-before") && diff.contains("+after"));
        let files = get_changed_files_blocking(path.clone(), None).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "tracked.txt");
        assert_eq!(files[0].status, "M");
        assert_eq!((files[0].additions, files[0].deletions), (2, 1));
        let stats = get_diff_stats_blocking(path.clone(), None).unwrap();
        assert_eq!((stats.additions, stats.deletions), (2, 1));
        let file_diff =
            get_file_diff_blocking(path, "tracked.txt".into(), None, Some(false)).unwrap();
        assert!(file_diff.contains("-before") && file_diff.contains("+extra"));
    }

    // Catches the raw/numstat reader confusing rename source and destination,
    // splitting literal paths, or accidentally reading unstaged work in staged scope.
    #[test]
    fn changed_file_counts_preserve_scope_and_rename_paths_1499() {
        let (_temp, repo) = empty_fixture_repo();
        assert!(
            repo.canonicalize()
                .unwrap()
                .starts_with(tuic_test_support::test_temp_root().canonicalize().unwrap())
        );
        let original = "old name.txt";
        let renamed = "new name.txt";
        std::fs::write(repo.join(original), "one\ntwo\nthree\nfour\nfive\n").unwrap();
        git_in(&repo, &["add", "."]);
        git_in(&repo, &["commit", "-qm", "baseline"]);
        let path = repo.to_string_lossy().into_owned();
        git_in(&repo, &["mv", original, renamed]);
        std::fs::write(repo.join(renamed), "one\ntwo\nthree\nfour\nfive\nadded\n").unwrap();
        git_in(&repo, &["add", "."]);
        let staged = get_changed_files_blocking(path.clone(), Some("staged".into())).unwrap();
        assert_eq!(staged.len(), 1);
        assert_eq!(staged[0].path, renamed);
        assert!(staged[0].status.starts_with('R'), "{}", staged[0].status);
        assert_eq!((staged[0].additions, staged[0].deletions), (1, 0));
        std::fs::write(
            repo.join(renamed),
            "one\ntwo\nthree\nfour\nfive\nadded\nunstaged\n",
        )
        .unwrap();
        std::fs::write(repo.join("untracked name.txt"), "new\n").unwrap();
        let working = get_changed_files_blocking(path.clone(), None).unwrap();
        let changed = working.iter().find(|file| file.path == renamed).unwrap();
        assert_eq!((changed.additions, changed.deletions), (1, 0));
        assert!(working.iter().any(|file| file.path == "untracked name.txt"
            && file.status == "?"
            && file.additions == 1));
        git_in(&repo, &["commit", "-qm", "rename"]);
        let head = git_in(&repo, &["rev-parse", "HEAD"]).trim().to_owned();
        let committed = get_changed_files_blocking(path, Some(head)).unwrap();
        assert_eq!(committed.len(), 1);
        assert_eq!(committed[0].path, renamed);
        assert_eq!((committed[0].additions, committed[0].deletions), (1, 0));
    }

    // Catches splitting control-character paths or misaligning binary/deleted
    // raw records with their numstat records in the public changed-file result.
    #[cfg(unix)]
    #[test]
    fn changed_files_preserve_control_paths_binary_and_deleted_status_critic_1499() {
        let (_temp, repo) = empty_fixture_repo();
        let literal = "tracked\tline\nname.txt";
        std::fs::write(repo.join(literal), "first\nsecond\n").unwrap();
        std::fs::write(repo.join("deleted.txt"), "removed\n").unwrap();
        std::fs::write(repo.join("binary.bin"), [0, 1, 2, 3]).unwrap();
        git_in(&repo, &["add", "."]);
        git_in(&repo, &["commit", "-qm", "critic baseline"]);
        std::fs::write(repo.join(literal), "first\nreplacement\nthird\n").unwrap();
        std::fs::remove_file(repo.join("deleted.txt")).unwrap();
        std::fs::write(repo.join("binary.bin"), [0, 9, 8, 7]).unwrap();
        let path = repo.to_string_lossy().into_owned();
        let check = |files: Vec<ChangedFile>| {
            assert_eq!(files.len(), 3, "unexpected number of changed-file records");
            for (name, status, additions, deletions) in [
                (literal, "M", 2, 1),
                ("deleted.txt", "D", 0, 1),
                ("binary.bin", "M", 0, 0),
            ] {
                let file = files.iter().find(|file| file.path == name).unwrap();
                assert_eq!(file.status, status, "status for {name:?}");
                assert_eq!((file.additions, file.deletions), (additions, deletions));
            }
        };
        check(get_changed_files_blocking(path.clone(), None).unwrap());
        git_in(&repo, &["add", "-A"]);
        check(get_changed_files_blocking(path.clone(), Some("staged".into())).unwrap());
        assert!(
            get_changed_files_blocking(path.clone(), None)
                .unwrap()
                .is_empty()
        );
        git_in(&repo, &["commit", "-qm", "critic changes"]);
        let head = git_in(&repo, &["rev-parse", "HEAD"]).trim().to_owned();
        check(get_changed_files_blocking(path, Some(head)).unwrap());
    }

    // Catches exposing Git's C-quoted display spelling as an untracked path,
    // which makes the consumer open a nonexistent file and report zero lines.
    #[cfg(unix)]
    #[test]
    fn changed_files_return_literal_untracked_control_path_critic_1499() {
        let (_temp, repo) = empty_fixture_repo();
        let literal = "untracked\tline\nname.txt";
        std::fs::write(repo.join(literal), "first\nsecond\n").unwrap();
        let files = get_changed_files_blocking(repo.to_string_lossy().into_owned(), None).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(
            files[0].path, literal,
            "untracked path must be usable by file-diff consumers"
        );
        assert_eq!(files[0].status, "?");
        assert_eq!((files[0].additions, files[0].deletions), (2, 0));
    }

    // Catches trimming boundary spaces, dropping ordinary untracked files,
    // or leaking untracked entries into staged/committed scopes.
    #[cfg(unix)]
    #[test]
    fn changed_files_preserve_untracked_boundary_spaces_and_scopes_1502() {
        let (_temp, repo) = empty_fixture_repo();
        git_in(&repo, &["commit", "-qm", "empty baseline", "--allow-empty"]);
        std::fs::write(repo.join("tracked.txt"), "baseline\n").unwrap();
        git_in(&repo, &["add", "tracked.txt"]);
        git_in(&repo, &["commit", "-qm", "baseline"]);
        let head = git_in(&repo, &["rev-parse", "HEAD"]).trim().to_owned();
        let literals = [" leading.txt", "trailing.txt ", "ordinary.txt"];
        for literal in literals {
            std::fs::write(repo.join(literal), "first\nsecond\n").unwrap();
        }
        let path = repo.to_string_lossy().into_owned();
        let files = get_changed_files_blocking(path.clone(), None).unwrap();
        assert_eq!(files.len(), literals.len());
        for literal in literals {
            let file = files
                .iter()
                .find(|file| file.path == literal)
                .unwrap_or_else(|| panic!("missing literal untracked path {literal:?}"));
            assert_eq!(file.status, "?");
            assert_eq!((file.additions, file.deletions), (2, 0));
            let diff =
                get_file_diff_blocking(path.clone(), file.path.clone(), None, Some(true)).unwrap();
            assert!(diff.contains("+first") && diff.contains("+second"));
        }
        assert!(
            get_changed_files_blocking(path.clone(), Some("staged".into()))
                .unwrap()
                .is_empty()
        );
        let committed = get_changed_files_blocking(path, Some(head)).unwrap();
        assert_eq!(committed.len(), 1);
        assert_eq!(committed[0].path, "tracked.txt");
        assert_eq!(committed[0].status, "A");
    }

    // --- Fixture repositories ---
    //
    // No test in this module resolves its subject repository from
    // `CARGO_MANIFEST_DIR`. The crate is not guaranteed to sit inside a git
    // repository — a `git archive` export, a source tarball and a vendored
    // build each produce a plain directory — and `get_commit_log` and friends
    // reject a path that is not a repository rather than letting git discover
    // one upward. A test pointed at the surrounding checkout therefore fails
    // there for a reason that has nothing to do with the code under test, and
    // in the meantime asserts against whatever history that checkout happens
    // to carry.

    /// Run `git` in `dir`, panicking with stderr when it fails.
    fn git_in(dir: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .current_dir(dir)
            .args(args)
            .output()
            .unwrap_or_else(|e| panic!("spawn git {args:?}: {e}"));
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// An empty repository on `main`, with an identity configured so commits work.
    fn empty_fixture_repo() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        // Canonicalized because `/var` is a symlink to `/private/var` on macOS
        // and `validate_paths_within_repo` compares canonical prefixes.
        let path = dir.path().canonicalize().expect("canonicalize tempdir");
        git_in(&path, &["init", "-q", "-b", "main"]);
        git_in(&path, &["config", "user.email", "test@test.com"]);
        git_in(&path, &["config", "user.name", "Test"]);
        (dir, path)
    }

    /// Author/committer time of the first fixture commit: 2025-01-01, so the
    /// "timestamp is after 2024" assertion keeps its meaning.
    const FIXTURE_EPOCH: i64 = 1_735_689_600;

    /// A repository whose `main` carries `commits` commits, each appending one
    /// line to `log.txt` — so every line of that file blames to a different
    /// commit, and `git log -- log.txt` has the full history.
    ///
    /// Built with `git fast-import`: one process for the whole history, so the
    /// 505-commit fixture the clamp test needs costs ~40 ms instead of 505
    /// `git commit` spawns.
    fn history_fixture_repo(commits: u32) -> (tempfile::TempDir, PathBuf) {
        use std::io::Write as _;
        assert!(commits > 0, "a history fixture needs at least one commit");
        let (dir, path) = empty_fixture_repo();

        let mut stream = String::new();
        let mut file = String::new();
        for i in 1..=commits {
            file.push_str(&format!("line {i}\n"));
            let blob_mark = i * 2;
            let commit_mark = blob_mark + 1;
            let ts = FIXTURE_EPOCH + i64::from(i);
            let subject = format!("commit {i}");
            stream.push_str(&format!("blob\nmark :{blob_mark}\ndata {}\n", file.len()));
            stream.push_str(&file);
            stream.push_str(&format!("commit refs/heads/main\nmark :{commit_mark}\n"));
            stream.push_str(&format!("author Test <test@test.com> {ts} +0000\n"));
            stream.push_str(&format!("committer Test <test@test.com> {ts} +0000\n"));
            stream.push_str(&format!("data {}\n{subject}\n", subject.len()));
            if i > 1 {
                stream.push_str(&format!("from :{}\n", commit_mark - 2));
            }
            stream.push_str(&format!("M 100644 :{blob_mark} log.txt\n"));
        }

        let mut child = std::process::Command::new("git")
            .current_dir(&path)
            .args(["fast-import", "--quiet"])
            .stdin(std::process::Stdio::piped())
            .spawn()
            .expect("spawn git fast-import");
        child
            .stdin
            .take()
            .expect("fast-import stdin")
            .write_all(stream.as_bytes())
            .expect("write fast-import stream");
        assert!(
            child.wait().expect("wait fast-import").success(),
            "git fast-import failed"
        );

        // fast-import writes objects and refs but leaves the index and working
        // tree empty; blame and file reads want the checked-out copy.
        git_in(&path, &["read-tree", "HEAD"]);
        git_in(&path, &["checkout-index", "-a", "-f"]);
        (dir, path)
    }

    // --- list_remotes / parse_git_config_remotes ---

    #[test]
    fn parse_config_remotes_enumerates_all() {
        let config = r#"[core]
    repositoryformatversion = 0
[remote "origin"]
    url = git@github.com:octocat/hello.git
    fetch = +refs/heads/*:refs/remotes/origin/*
[branch "main"]
    remote = origin
[remote "upstream"]
    url = https://github.com/upstream/hello.git
"#;
        let remotes = parse_git_config_remotes(config);
        assert_eq!(
            remotes,
            vec![
                (
                    "origin".to_string(),
                    "git@github.com:octocat/hello.git".to_string()
                ),
                (
                    "upstream".to_string(),
                    "https://github.com/upstream/hello.git".to_string()
                ),
            ]
        );
    }

    #[test]
    fn parse_config_remotes_ignores_pushurl_and_non_remotes() {
        let config = r#"[remote "origin"]
    url = git@github.com:octocat/hello.git
    pushurl = git@github.com:octocat/fork.git
"#;
        let remotes = parse_git_config_remotes(config);
        assert_eq!(
            remotes,
            vec![(
                "origin".to_string(),
                "git@github.com:octocat/hello.git".to_string()
            )]
        );
    }

    #[test]
    fn list_remotes_reads_from_git_dir() {
        let dir = tempfile::tempdir().expect("tempdir");
        let git_dir = dir.path().join(".git");
        std::fs::create_dir_all(&git_dir).expect("mkdir .git");
        std::fs::write(
            git_dir.join("config"),
            "[remote \"origin\"]\n\turl = git@github.com:octocat/hello.git\n",
        )
        .expect("write config");
        let remotes = list_remotes(dir.path());
        assert_eq!(
            remotes,
            vec![(
                "origin".to_string(),
                "git@github.com:octocat/hello.git".to_string()
            )]
        );
    }

    #[test]
    fn list_remotes_empty_for_non_repo() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(list_remotes(dir.path()).is_empty());
    }

    // --- canonical_repo_root ---

    #[test]
    fn canonical_repo_root_normal_repo_is_its_own_root() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("main");
        std::fs::create_dir_all(root.join(".git")).expect("mkdir .git");
        assert_eq!(canonical_repo_root(&root), root.canonicalize().unwrap());
    }

    #[test]
    fn canonical_repo_root_linked_worktree_resolves_to_main() {
        let dir = tempfile::tempdir().expect("tempdir");
        // Main repo: <base>/main/.git/ with a linked-worktree gitdir.
        let main = dir.path().join("main");
        let main_git = main.join(".git");
        let wt_gitdir = main_git.join("worktrees").join("feat");
        std::fs::create_dir_all(&wt_gitdir).expect("mkdir worktree gitdir");
        // commondir points back at the main .git (relative, as git writes it).
        std::fs::write(wt_gitdir.join("commondir"), "../..\n").expect("write commondir");

        // Linked worktree checkout: <base>/feat/.git is a file → gitdir.
        let wt = dir.path().join("feat");
        std::fs::create_dir_all(&wt).expect("mkdir worktree");
        std::fs::write(
            wt.join(".git"),
            format!("gitdir: {}\n", wt_gitdir.display()),
        )
        .expect("write .git file");

        // Both the main repo and its linked worktree canonicalize to the same root.
        assert_eq!(canonical_repo_root(&wt), canonical_repo_root(&main));
        assert_eq!(canonical_repo_root(&main), main.canonicalize().unwrap());
    }

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
        branches
            .iter()
            .map(|b| b["name"].as_str().unwrap())
            .collect()
    }

    #[test]
    fn sort_branches_main_first_then_alphabetical() {
        let mut branches = vec![
            make_branch("feature/z", false),
            make_branch("main", true),
            make_branch("feature/a", false),
        ];
        sort_branches(&mut branches);
        assert_eq!(
            branch_names(&branches),
            vec!["main", "feature/a", "feature/z"]
        );
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
        assert_eq!(
            branch_names(&branches),
            vec!["develop", "main", "master", "feature/x"]
        );
    }

    #[test]
    fn sort_branches_all_feature_branches_alphabetical() {
        let mut branches = vec![
            make_branch("feature/c", false),
            make_branch("feature/a", false),
            make_branch("feature/b", false),
        ];
        sort_branches(&mut branches);
        assert_eq!(
            branch_names(&branches),
            vec!["feature/a", "feature/b", "feature/c"]
        );
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
        assert!(
            !result.contains('\0'),
            "initials must not contain null bytes"
        );
        assert_eq!(result, "MY");
    }

    #[test]
    fn get_repo_initials_null_byte_at_word_start() {
        let result = get_repo_initials("\0my-repo");
        assert!(
            !result.contains('\0'),
            "initials must not contain null bytes"
        );
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
            assert!(
                !ch.is_control(),
                "initials must not contain control characters: {:?}",
                ch
            );
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
    // These run against a fixture repository to validate correctness.

    #[test]
    fn test_read_branch_matches_git_rev_parse() {
        let (_dir, repo_root) = history_fixture_repo(1);

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

        assert_eq!(
            file_branch,
            Some("main".to_string()),
            "the fixture is checked out on main"
        );
        assert_eq!(
            file_branch, git_branch,
            "read_branch_from_head() must match `git rev-parse --abbrev-ref HEAD`"
        );
    }

    #[test]
    fn test_read_remote_url_matches_git_remote() {
        let (_dir, repo_root) = history_fixture_repo(1);
        git_in(
            &repo_root,
            &[
                "remote",
                "add",
                "origin",
                "git@github.com:octocat/hello.git",
            ],
        );

        // File I/O approach
        let file_url = read_remote_url(&repo_root);

        // Subprocess approach (ground truth)
        let git_url = git_cmd(&repo_root)
            .args(["remote", "get-url", "origin"])
            .run_silent()
            .map(|o| o.stdout.trim().to_string());

        assert_eq!(
            file_url, git_url,
            "read_remote_url() must match `git remote get-url origin`"
        );
    }

    #[test]
    fn test_resolve_git_dir_for_local_repo() {
        let (_dir, repo_root) = history_fixture_repo(1);

        let git_dir = resolve_git_dir(&repo_root);
        assert!(git_dir.is_some(), "Should resolve .git dir for this repo");
        assert!(
            git_dir.unwrap().join("HEAD").exists(),
            ".git dir should contain HEAD"
        );
    }

    #[test]
    fn test_get_merged_branches_against_a_fixture_repo() {
        let (_dir, repo_root) = history_fixture_repo(3);
        // An ancestor of main: merged, and at a different SHA, so it survives
        // the "never diverged" filter.
        git_in(&repo_root, &["branch", "merged-feature", "main~1"]);
        // At main's exact SHA: merged, but filtered out as never diverged.
        git_in(&repo_root, &["branch", "same-as-main", "main"]);
        // Its own commit on top of main: not merged.
        git_in(&repo_root, &["switch", "-q", "-c", "diverged"]);
        std::fs::write(repo_root.join("side.txt"), "side").expect("write side.txt");
        git_in(&repo_root, &["add", "side.txt"]);
        git_in(&repo_root, &["commit", "-q", "-m", "side commit"]);
        git_in(&repo_root, &["switch", "-q", "main"]);

        let merged = get_merged_branches_impl(&repo_root)
            .expect("get_merged_branches_impl should succeed on a repo");

        assert!(
            merged.contains(&"merged-feature".to_string()),
            "an ancestor branch at its own SHA is merged, got: {merged:?}"
        );
        assert!(
            !merged.contains(&"diverged".to_string()),
            "a branch with commits main does not contain is not merged, got: {merged:?}"
        );
        assert!(
            !merged.contains(&"same-as-main".to_string()),
            "a branch at main's SHA never diverged and is filtered, got: {merged:?}"
        );
        // The main branch must not appear in its own merged list.
        assert!(
            !merged.iter().any(|b| is_main_branch(b)),
            "main branch should not be in merged list, got: {merged:?}"
        );
    }

    #[test]
    fn test_get_merged_branches_returns_empty_for_nonexistent_path() {
        let result = get_merged_branches_impl(Path::new("/nonexistent/path/xyz"));
        assert_eq!(
            result.unwrap(),
            Vec::<String>::new(),
            "should return empty vec for nonexistent path"
        );
    }

    #[test]
    fn test_get_merged_branches_returns_empty_for_non_git_directory() {
        let result = get_merged_branches_impl(&std::env::temp_dir());
        assert_eq!(
            result.unwrap(),
            Vec::<String>::new(),
            "should return empty vec for non-git directory"
        );
    }

    #[test]
    fn test_detect_default_branch_for_a_fixture_repo() {
        let (_dir, repo_root) = history_fixture_repo(1);
        let git_dir = resolve_git_dir(&repo_root).expect("should resolve git dir");

        assert_eq!(
            detect_default_branch(&git_dir),
            Some("main".to_string()),
            "a local refs/heads/main is the default branch"
        );
    }

    /// The detached-HEAD shape a CI checkout produces: no local branch at all,
    /// only `refs/remotes/origin/main`. This used to hide inside the real-repo
    /// test as an `== "main" || == "origin/main"` disjunction, which meant
    /// neither branch of it was ever asserted on purpose.
    #[test]
    fn test_detect_default_branch_falls_back_to_the_remote_tracking_ref() {
        let (_dir, repo_root) = history_fixture_repo(1);
        let head = git_in(&repo_root, &["rev-parse", "HEAD"])
            .trim()
            .to_string();
        git_in(&repo_root, &["checkout", "-q", "--detach", &head]);
        git_in(
            &repo_root,
            &["update-ref", "refs/remotes/origin/main", &head],
        );
        git_in(&repo_root, &["branch", "-q", "-D", "main"]);
        let git_dir = resolve_git_dir(&repo_root).expect("should resolve git dir");

        assert_eq!(
            detect_default_branch(&git_dir),
            Some("origin/main".to_string()),
            "with no local branch the remote-tracking ref answers"
        );
    }

    /// Story 001-92d0: in a LINKED worktree, `.git` is a file and shared data
    /// (config, refs) lives in the common dir. A self-contained regression that
    /// builds a real worktree and asserts `read_remote_url` + `detect_default_branch`
    /// resolve via `common_git_dir` (would return None on pre-fix code).
    #[test]
    fn worktree_common_dir_resolves_remote_and_default_branch() {
        use std::process::Command;
        let dir = tempfile::tempdir().expect("tempdir");
        let main = dir.path().join("main");
        std::fs::create_dir(&main).unwrap();
        let git = |cwd: &std::path::Path, args: &[&str]| {
            let out = Command::new("git")
                .current_dir(cwd)
                .args(args)
                .env("GIT_AUTHOR_NAME", "T")
                .env("GIT_AUTHOR_EMAIL", "t@t")
                .env("GIT_COMMITTER_NAME", "T")
                .env("GIT_COMMITTER_EMAIL", "t@t")
                .output()
                .unwrap_or_else(|e| panic!("git {args:?}: {e}"));
            assert!(
                out.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        git(&main, &["init", "-b", "main"]);
        git(&main, &["config", "core.hooksPath", "/dev/null"]);
        git(
            &main,
            &["remote", "add", "origin", "git@github.com:acme/widget.git"],
        );
        std::fs::write(main.join("a.txt"), "a\n").unwrap();
        git(&main, &["add", "a.txt"]);
        git(&main, &["commit", "-m", "init", "--no-verify"]);

        // Linked worktree: its `.git` is a FILE pointing at the per-worktree gitdir.
        let wt = dir.path().join("wt");
        git(
            &main,
            &["worktree", "add", "-b", "feature", wt.to_str().unwrap()],
        );
        assert!(
            wt.join(".git").is_file(),
            "linked worktree .git should be a file"
        );

        // Pre-fix these would read the per-worktree gitdir (no config/refs) → None.
        assert_eq!(
            read_remote_url(&wt).as_deref(),
            Some("git@github.com:acme/widget.git"),
            "origin URL must resolve from inside a worktree (common dir)"
        );
        let gitdir = resolve_git_dir(&wt).expect("worktree gitdir");
        assert_eq!(
            detect_default_branch(&gitdir).as_deref(),
            Some("main"),
            "default branch must resolve from inside a worktree (common dir)"
        );
    }

    #[test]
    fn test_detect_default_branch_returns_none_for_non_git() {
        let tmp = std::env::temp_dir();
        // temp_dir has no .git — resolve_git_dir returns None, so we
        // test detect_default_branch with a fake path that has no refs
        assert!(detect_default_branch(&tmp).is_none());
    }

    #[tokio::test]
    async fn get_file_diff_rejects_path_traversal() {
        // A repository with a real file next to it, reachable only by
        // traversing out: `get_file_diff` runs its traversal guard solely when
        // the joined path EXISTS, so a target that is merely absent falls
        // through to `git diff` and the assertion then reads git's message
        // instead of ours.
        //
        // This test used to take `std::env::current_dir()` as "a valid git
        // repo" and ask for `../../etc/passwd`. That path does not exist
        // either, so the guard never ran: it passed on git's own
        // "outside repository" wording, and only while the crate happened to
        // sit inside a repository. In a `git archive` export git printed its
        // usage instead and the test failed (#772-f7c2).
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().canonicalize().expect("canonicalize tempdir");
        let repo = root.join("repo");
        std::fs::create_dir(&repo).expect("create repo dir");
        git_in(&repo, &["init", "-q", "-b", "main"]);
        std::fs::write(root.join("outside.txt"), "secret").expect("write outside.txt");

        let result = get_file_diff(
            repo.to_string_lossy().to_string(),
            "../outside.txt".to_string(),
            None,
            None,
        )
        .await;

        assert_eq!(
            result.unwrap_err(),
            "Access denied: file is outside repository",
            "the guard in get_file_diff must reject it, not git"
        );
    }

    // --- parse_diff_to_changes unit tests (ported from gitGutter.test.ts) ---

    /// Collect, sorted, the 1-based new-file lines of a given change type.
    fn lines_of(changes: &[GutterChange], ty: GutterChangeType) -> Vec<u32> {
        let mut v: Vec<u32> = changes
            .iter()
            .filter(|c| c.change_type == ty)
            .map(|c| c.line)
            .collect();
        v.sort_unstable();
        v
    }

    #[test]
    fn gutter_empty_diff_is_empty() {
        assert_eq!(parse_diff_to_changes(""), vec![]);
    }

    #[test]
    fn gutter_pure_insertions_are_added() {
        let diff = "diff --git a/f.txt b/f.txt\n--- a/f.txt\n+++ b/f.txt\n@@ -2,1 +2,3 @@\n context\n+new one\n+new two";
        let c = parse_diff_to_changes(diff);
        assert_eq!(lines_of(&c, GutterChangeType::Added), vec![3, 4]);
        assert_eq!(lines_of(&c, GutterChangeType::Modified), Vec::<u32>::new());
        assert_eq!(lines_of(&c, GutterChangeType::Deleted), Vec::<u32>::new());
    }

    #[test]
    fn gutter_replaced_lines_are_modified() {
        let diff = "diff --git a/f.txt b/f.txt\n--- a/f.txt\n+++ b/f.txt\n@@ -5,1 +5,1 @@\n-old text\n+new text";
        let c = parse_diff_to_changes(diff);
        assert_eq!(lines_of(&c, GutterChangeType::Modified), vec![5]);
        assert_eq!(lines_of(&c, GutterChangeType::Added), Vec::<u32>::new());
        assert_eq!(lines_of(&c, GutterChangeType::Deleted), Vec::<u32>::new());
    }

    #[test]
    fn gutter_add_heavy_replacement_is_modified() {
        let diff = "@@ -3,1 +3,3 @@\n-old\n+a\n+b\n+c";
        let c = parse_diff_to_changes(diff);
        assert_eq!(lines_of(&c, GutterChangeType::Modified), vec![3, 4, 5]);
    }

    #[test]
    fn gutter_pure_deletion_marks_following_line() {
        let diff = "@@ -3,3 +3,1 @@\n keep\n-gone one\n-gone two";
        let c = parse_diff_to_changes(diff);
        assert_eq!(lines_of(&c, GutterChangeType::Deleted), vec![4]);
        assert_eq!(lines_of(&c, GutterChangeType::Added), Vec::<u32>::new());
        assert_eq!(lines_of(&c, GutterChangeType::Modified), Vec::<u32>::new());
    }

    #[test]
    fn gutter_new_untracked_file_all_added() {
        let diff = "diff --git a/new.txt b/new.txt\nnew file mode 100644\n--- /dev/null\n+++ b/new.txt\n@@ -0,0 +1,3 @@\n+line1\n+line2\n+line3";
        let c = parse_diff_to_changes(diff);
        assert_eq!(lines_of(&c, GutterChangeType::Added), vec![1, 2, 3]);
    }

    #[test]
    fn gutter_multiple_hunks_classified_independently() {
        let diff = "diff --git a/f.txt b/f.txt\n--- a/f.txt\n+++ b/f.txt\n@@ -1,2 +1,2 @@\n-first old\n+first new\n second\n@@ -10,2 +10,3 @@\n ctx\n+inserted\n tail";
        let c = parse_diff_to_changes(diff);
        assert_eq!(lines_of(&c, GutterChangeType::Modified), vec![1]);
        assert_eq!(lines_of(&c, GutterChangeType::Added), vec![11]);
    }

    #[test]
    fn gutter_ignores_no_newline_marker() {
        let diff = "@@ -1,1 +1,1 @@\n-a\n+b\n\\ No newline at end of file";
        let c = parse_diff_to_changes(diff);
        assert_eq!(lines_of(&c, GutterChangeType::Modified), vec![1]);
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
        assert_eq!(
            status.staged[0].original_path,
            Some("old_name.rs".to_string())
        );
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
        assert!(status.conflicted.is_empty());
    }

    #[test]
    fn parse_porcelain_v2_unmerged_conflict() {
        // Type u entry: both-modified conflict during a merge.
        // Format: u <XY> <sub> <m1> <m2> <m3> <mW> <h1> <h2> <h3> <path>
        let output = "u UU N... 100644 100644 100644 100644 abc123 def456 789abc src/conflict.rs\n";
        let status = parse_porcelain_v2(output);
        assert_eq!(status.conflicted.len(), 1);
        assert_eq!(status.conflicted[0].path, "src/conflict.rs");
        assert_eq!(status.conflicted[0].status, "UU");
        // Conflicted files must not leak into staged/unstaged.
        assert!(status.staged.is_empty());
        assert!(status.unstaged.is_empty());
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

    /// ChangesTab and StatusBar both refetch on every repo-revision bump, so one
    /// `repo-changed` used to fork six git processes for the same answer. The
    /// concurrent callers must collapse onto one computation.
    #[tokio::test]
    async fn concurrent_working_tree_status_calls_share_one_computation() {
        let (_dir, path) = setup_test_repo_with_commit();
        std::fs::write(path.join("dirty.txt"), "edited").expect("write");
        let repo = path.to_string_lossy().to_string();

        let mut set = tokio::task::JoinSet::new();
        for _ in 0..4 {
            let p = repo.clone();
            set.spawn(async move { get_working_tree_status(p).await });
        }
        let mut results = Vec::new();
        while let Some(joined) = set.join_next().await {
            results.push(joined.expect("task").expect("status"));
        }

        assert_eq!(results.len(), 4);
        for r in &results[1..] {
            assert_eq!(*r, results[0], "every caller must get the same answer");
        }
        assert_eq!(
            wt_status_computes(&repo),
            1,
            "four concurrent callers must fork git once, not four times"
        );

        // Single-flight, NOT a cache: a later call must re-read, otherwise a
        // refetch right after a stage/unstage would answer with pre-mutation
        // state.
        let _ = get_working_tree_status(repo.clone()).await.expect("status");
        assert_eq!(
            wt_status_computes(&repo),
            2,
            "a sequential call must recompute — no value may be retained"
        );
    }

    #[tokio::test]
    async fn get_working_tree_status_nonexistent_path() {
        let result = get_working_tree_status("/nonexistent/repo/xyz".to_string()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn get_working_tree_status_expands_untracked_dir() {
        // A wholly-untracked directory with multiple files must surface as
        // individual file paths (`--untracked-files=all`), not a single
        // collapsed `providers/` entry that has no diff to open.
        let (dir, path) = setup_test_repo_with_commit();
        std::fs::create_dir(path.join("providers")).expect("mkdir providers");
        std::fs::write(path.join("providers/a.txt"), "a").expect("write a");
        std::fs::write(path.join("providers/b.txt"), "b").expect("write b");

        let status = get_working_tree_status(path.to_string_lossy().to_string())
            .await
            .unwrap();

        assert!(
            status.untracked.contains(&"providers/a.txt".to_string()),
            "providers/a.txt should be listed individually, got {:?}",
            status.untracked
        );
        assert!(
            status.untracked.contains(&"providers/b.txt".to_string()),
            "providers/b.txt should be listed individually, got {:?}",
            status.untracked
        );
        assert!(
            !status.untracked.iter().any(|p| p == "providers/"),
            "collapsed providers/ entry must not appear, got {:?}",
            status.untracked
        );
        drop(dir);
    }

    // --- validate_paths_within_repo tests ---

    /// A directory holding `src/tracked.rs`, so the existing-file branch of
    /// `validate_paths_within_repo` (the one that canonicalizes) is exercised.
    fn validate_paths_fixture() -> (tempfile::TempDir, PathBuf) {
        let (dir, path) = empty_fixture_repo();
        std::fs::create_dir_all(path.join("src")).expect("mkdir src");
        std::fs::write(path.join("src/tracked.rs"), "// tracked").expect("write tracked.rs");
        (dir, path)
    }

    #[test]
    fn validate_paths_rejects_traversal() {
        let (_dir, repo) = validate_paths_fixture();
        let result = validate_paths_within_repo(&repo, &["../../etc/passwd".to_string()]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("outside repository"));
    }

    #[test]
    fn validate_paths_accepts_normal_paths() {
        let (_dir, repo) = validate_paths_fixture();
        let result = validate_paths_within_repo(&repo, &["src/tracked.rs".to_string()]);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_paths_rejects_absolute_paths() {
        let (_dir, repo) = validate_paths_fixture();
        let result = validate_paths_within_repo(&repo, &["/etc/passwd".to_string()]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("absolute path"));
    }

    /// The absolute shapes of the *other* platform have to go too. Windows
    /// `Path::join` replaces the root when the joined path carries one, so a
    /// `/etc/passwd` that slipped through as "relative" would land at the root
    /// of the repo's drive instead of inside the repo.
    #[test]
    fn validate_paths_rejects_the_other_platforms_absolute_shapes() {
        let (_dir, repo) = validate_paths_fixture();
        for file in ["/etc/passwd", "C:\\Windows\\win.ini", "\\\\host\\share\\f"] {
            let result = validate_paths_within_repo(&repo, &[file.to_string()]);
            assert!(
                result.is_err_and(|error| error.contains("absolute path")),
                "{file} must be rejected as absolute"
            );
        }
    }

    // --- Integration tests for stage/unstage/discard ---

    // --- get_gutter_changes vs get_file_diff on an untracked file ---

    #[tokio::test]
    async fn gutter_changes_skip_an_untracked_file() {
        let (_dir, path) = setup_test_repo_with_commit();
        std::fs::write(path.join("new.json"), "[\n1,\n2\n]\n").expect("write");
        let changes = get_gutter_changes(
            path.to_string_lossy().to_string(),
            "new.json".to_string(),
            Some("head".to_string()),
        )
        .await
        .expect("gutter changes");
        assert!(
            changes.is_empty(),
            "an untracked file has no HEAD version to mark against: {changes:?}"
        );
    }

    #[tokio::test]
    async fn gutter_changes_mark_a_modified_tracked_file() {
        let (_dir, path) = setup_test_repo_with_commit();
        let file = path.join("lines.txt");
        std::fs::write(&file, "a\nb\nc\n").expect("write");
        git_cmd(&path)
            .args(["add", "lines.txt"])
            .run()
            .expect("add");
        git_cmd(&path)
            .args(["commit", "-m", "lines", "--no-verify"])
            .run()
            .expect("commit");
        std::fs::write(&file, "a\nb\nc\nd\n").expect("append");
        let changes = get_gutter_changes(
            path.to_string_lossy().to_string(),
            "lines.txt".to_string(),
            Some("head".to_string()),
        )
        .await
        .expect("gutter changes");
        assert_eq!(
            changes,
            vec![GutterChange {
                line: 4,
                change_type: GutterChangeType::Added,
            }],
            "only the appended line is marked"
        );
    }

    #[tokio::test]
    async fn gutter_changes_stay_empty_for_an_unmodified_tracked_file() {
        let (_dir, path) = setup_test_repo_with_commit();
        let changes = get_gutter_changes(
            path.to_string_lossy().to_string(),
            "initial.txt".to_string(),
            Some("head".to_string()),
        )
        .await
        .expect("gutter changes");
        assert_eq!(changes, vec![]);
    }

    /// The diff viewer keeps the all-added view of a new file; only the gutter
    /// short-circuits.
    #[tokio::test]
    async fn file_diff_still_shows_an_untracked_file_as_all_added() {
        let (_dir, path) = setup_test_repo_with_commit();
        std::fs::write(path.join("new.json"), "[\n1\n]\n").expect("write");
        let diff = get_file_diff(
            path.to_string_lossy().to_string(),
            "new.json".to_string(),
            Some("head".to_string()),
            None,
        )
        .await
        .expect("file diff");
        assert_eq!(parse_diff_to_changes(&diff).len(), 3, "{diff}");
    }

    #[test]
    fn is_file_tracked_distinguishes_tracked_from_untracked() {
        let (_dir, path) = setup_test_repo_with_commit();
        std::fs::write(path.join("loose.txt"), "x").expect("write");
        assert_eq!(is_file_tracked(&path, "initial.txt"), Ok(true));
        assert_eq!(is_file_tracked(&path, "loose.txt"), Ok(false));
        assert_eq!(is_file_tracked(&path, "missing.txt"), Ok(false));
    }

    /// Capture every `tracing` event emitted on this thread while `f` runs.
    fn capture_tracing<T>(f: impl FnOnce() -> T) -> (T, String) {
        use std::sync::{Arc, Mutex};

        #[derive(Clone)]
        struct Sink(Arc<Mutex<Vec<u8>>>);
        impl std::io::Write for Sink {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().expect("sink").extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Sink {
            type Writer = Sink;
            fn make_writer(&'a self) -> Self::Writer {
                self.clone()
            }
        }

        let sink = Arc::new(Mutex::new(Vec::new()));
        let subscriber = tracing_subscriber::fmt()
            .with_writer(Sink(sink.clone()))
            .with_ansi(false)
            .with_max_level(tracing::Level::TRACE)
            .finish();
        let out = tracing::subscriber::with_default(subscriber, f);
        let text = String::from_utf8(sink.lock().expect("sink").clone()).expect("utf8");
        (out, text)
    }

    /// A failed merge check must reach the app log through `tracing`, not
    /// stderr: `GET /logs` is where the diagnostic is looked for, and an
    /// a bare stderr print never lands there.
    #[test]
    fn merged_branch_set_reports_failure_through_tracing() {
        let (_dir, path) = setup_test_repo_with_commit();
        std::process::Command::new("git")
            .current_dir(&path)
            .args(["branch", "-M", "main"])
            .output()
            .expect("rename to main");
        // Point the default-branch ref at an object that does not exist. The
        // ref file still exists, so `detect_default_branch` succeeds and
        // `git branch --merged main` is reached and fails — the only route
        // into the error arm of `merged_branch_set`.
        std::fs::write(
            path.join(".git/refs/heads/main"),
            "0000000000000000000000000000000000000000\n",
        )
        .expect("corrupt ref");

        let (merged, logs) = capture_tracing(|| merged_branch_set(&path));

        assert!(merged.is_empty(), "a failed merge check yields no branches");
        assert!(
            logs.contains("Failed to determine merged branches"),
            "merge-check failure was not logged through tracing; captured: {logs:?}"
        );
        assert!(
            logs.contains("WARN"),
            "merge-check failure must be logged at warn level; captured: {logs:?}"
        );
    }

    // --- get_repo_info_impl status classification ---

    #[test]
    fn repo_info_status_ignores_conflict_codes_inside_file_names() {
        let (_dir, path) = setup_test_repo_with_commit();
        std::fs::write(path.join("UUID.md"), "notes").expect("write UUID.md");
        let info = get_repo_info_impl(&path.to_string_lossy());
        assert_eq!(
            info.status, "dirty",
            "an untracked file named UUID.md is not a conflict"
        );
    }

    /// The repo badge must not fork a `git status` of its own — it reads the
    /// same porcelain-v2 verdict the git panel does, through the GitReads port
    /// (gix in the default config, so no subprocess at all).
    #[test]
    fn repo_info_status_forks_no_git_of_its_own() {
        let (_dir, path) = setup_test_repo_with_commit();
        std::fs::write(path.join("untracked.txt"), "u").expect("write untracked");

        let before = crate::git_cli::git_cmd_forks(&path);
        let info = get_repo_info_impl(&path.to_string_lossy());
        let forked = crate::git_cli::git_cmd_forks(&path) - before;

        assert_eq!(info.status, "dirty");
        assert_eq!(
            forked, 0,
            "repo_info must reuse the shared status read, not fork its own"
        );
    }

    /// The badge and the panel must never disagree: one read, one verdict.
    #[test]
    fn repo_info_status_matches_the_git_panel_verdict() {
        let (_dir, path) = setup_test_repo_with_commit();
        for expected in ["clean", "dirty"] {
            if expected == "dirty" {
                std::fs::write(path.join("untracked.txt"), "u").expect("write untracked");
            }
            let info = get_repo_info_impl(&path.to_string_lossy());
            assert_eq!(info.status, expected);
            assert_eq!(
                info.status,
                git_reads().status_counts(&path).status,
                "badge and panel must share the verdict"
            );
        }
    }

    /// A repo git cannot read is "unknown", never "clean" — a green tick on an
    /// unreadable repo is a silent failure.
    #[test]
    fn status_counts_cli_reports_unknown_when_the_repo_cannot_be_read() {
        let counts = status_counts_cli(Path::new("/nonexistent/repo/xyz"));
        assert_eq!(counts.status, "unknown");
        assert_eq!((counts.staged, counts.changed), (0, 0));
    }

    #[test]
    fn repo_info_status_reports_unmerged_paths_as_conflict() {
        let (_dir, path) = setup_test_repo_with_commit();
        let git = |args: &[&str]| {
            std::process::Command::new("git")
                .current_dir(&path)
                .args(args)
                .output()
                .expect("git");
        };
        std::fs::write(path.join("file.rs"), "base\n").expect("write base");
        git(&["add", "file.rs"]);
        git(&["commit", "-m", "base"]);
        git(&["checkout", "-b", "other"]);
        std::fs::write(path.join("file.rs"), "other\n").expect("write other");
        git(&["commit", "-am", "other"]);
        git(&["checkout", "-"]);
        std::fs::write(path.join("file.rs"), "main\n").expect("write main");
        git(&["commit", "-am", "main"]);
        git(&["merge", "other"]);

        let info = get_repo_info_impl(&path.to_string_lossy());
        assert_eq!(info.status, "conflict", "an unmerged file.rs is a conflict");
    }

    #[tokio::test]
    async fn stage_files_adds_to_index() {
        let (_dir, path) = setup_test_repo_with_commit();
        std::fs::write(path.join("new.txt"), "content").expect("write");
        let result = git_stage_files(
            path.to_string_lossy().to_string(),
            vec!["new.txt".to_string()],
        )
        .await;
        assert!(result.is_ok());
        // Verify it's staged
        let status = get_working_tree_status(path.to_string_lossy().to_string())
            .await
            .unwrap();
        assert!(
            status.staged.iter().any(|e| e.path == "new.txt"),
            "new.txt should be staged"
        );
    }

    #[tokio::test]
    async fn unstage_files_removes_from_index() {
        let (_dir, path) = setup_test_repo_with_commit();
        std::fs::write(path.join("staged.txt"), "content").expect("write");
        std::process::Command::new("git")
            .current_dir(&path)
            .args(["add", "staged.txt"])
            .output()
            .expect("add");
        let result = git_unstage_files(
            path.to_string_lossy().to_string(),
            vec!["staged.txt".to_string()],
        )
        .await;
        assert!(result.is_ok());
        // Verify it's no longer staged (should be untracked now)
        let status = get_working_tree_status(path.to_string_lossy().to_string())
            .await
            .unwrap();
        assert!(
            !status.staged.iter().any(|e| e.path == "staged.txt"),
            "staged.txt should not be staged"
        );
        assert!(
            status.untracked.contains(&"staged.txt".to_string()),
            "staged.txt should be untracked"
        );
    }

    #[tokio::test]
    async fn discard_files_restores_working_tree() {
        let (_dir, path) = setup_test_repo_with_commit();
        // Modify the initial file
        std::fs::write(path.join("initial.txt"), "modified").expect("write");
        let result = git_discard_files(
            path.to_string_lossy().to_string(),
            vec!["initial.txt".to_string()],
        )
        .await;
        assert!(result.is_ok());
        // Content should be restored
        let content = std::fs::read_to_string(path.join("initial.txt")).expect("read");
        assert_eq!(content, "hello");
    }

    #[tokio::test]
    async fn stage_files_rejects_path_traversal() {
        let (_dir, path) = setup_test_repo_with_commit();
        let result = git_stage_files(
            path.to_string_lossy().to_string(),
            vec!["../../etc/passwd".to_string()],
        )
        .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("outside repository"));
    }

    #[tokio::test]
    async fn unstage_files_rejects_path_traversal() {
        let (_dir, path) = setup_test_repo_with_commit();
        let result = git_unstage_files(
            path.to_string_lossy().to_string(),
            vec!["../../etc/passwd".to_string()],
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn discard_files_rejects_path_traversal() {
        let (_dir, path) = setup_test_repo_with_commit();
        let result = git_discard_files(
            path.to_string_lossy().to_string(),
            vec!["../../etc/passwd".to_string()],
        )
        .await;
        assert!(result.is_err());
    }

    // --- git_commit tests ---

    #[tokio::test]
    async fn git_commit_creates_commit_and_returns_hash() {
        let (_dir, path) = setup_test_repo_with_commit();
        std::fs::write(path.join("commit_test.txt"), "data").expect("write");
        std::process::Command::new("git")
            .current_dir(&path)
            .args(["add", "commit_test.txt"])
            .output()
            .expect("add");
        let result = git_commit(
            path.to_string_lossy().to_string(),
            "test commit".to_string(),
            None,
        )
        .await;
        assert!(result.is_ok());
        let hash = result.unwrap();
        assert_eq!(hash.len(), 40, "should return full 40-char SHA");
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "hash should be hex"
        );
    }

    #[tokio::test]
    async fn git_commit_amend_works() {
        let (_dir, path) = setup_test_repo_with_commit();
        let result = git_commit(
            path.to_string_lossy().to_string(),
            "amended message".to_string(),
            Some(true),
        )
        .await;
        assert!(result.is_ok());
        // Verify the commit message changed
        let out = git_cmd(&path)
            .args(["log", "--format=%s", "-1"])
            .run()
            .unwrap();
        assert_eq!(out.stdout.trim(), "amended message");
    }

    #[tokio::test]
    async fn git_commit_fails_with_nothing_staged() {
        let (_dir, path) = setup_test_repo_with_commit();
        let result = git_commit(
            path.to_string_lossy().to_string(),
            "empty commit".to_string(),
            None,
        )
        .await;
        assert!(result.is_err(), "commit with nothing staged should fail");
    }

    // --- get_last_commit_timestamps tests ---

    #[test]
    fn get_last_commit_timestamps_returns_timestamp_for_main() {
        let (_dir, repo) = history_fixture_repo(2);
        let git_dir = resolve_git_dir(&repo).expect("should resolve git dir");
        let main_ref = detect_default_branch(&git_dir).expect("should detect a default branch");
        let result = get_last_commit_timestamps(&repo, std::slice::from_ref(&main_ref));
        assert!(
            result.contains_key(&main_ref),
            "should contain the main ref key"
        );
        let ts = result[&main_ref];
        assert!(ts.is_some(), "main branch should have a commit timestamp");
        // The fixture's last commit, not "some plausible-looking number".
        assert_eq!(ts.unwrap(), FIXTURE_EPOCH + 2);
        // Timestamp should be reasonable (after 2024-01-01 = 1704067200)
        assert!(
            ts.unwrap() > 1_704_067_200,
            "timestamp should be after 2024"
        );
    }

    #[test]
    fn get_last_commit_timestamps_nonexistent_branch_returns_none() {
        let (_dir, repo) = history_fixture_repo(1);
        let result = get_last_commit_timestamps(&repo, &["nonexistent-branch-abc123".to_string()]);
        assert!(result.contains_key("nonexistent-branch-abc123"));
        assert!(result["nonexistent-branch-abc123"].is_none());
    }

    #[test]
    fn get_last_commit_timestamps_empty_input_returns_empty() {
        let (_dir, repo) = history_fixture_repo(1);
        let result = get_last_commit_timestamps(&repo, &[]);
        assert!(result.is_empty());
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
        assert!(entry.body.is_none());
    }

    #[test]
    fn parse_commit_log_line_with_body() {
        let line = "abc123\x00def456\x00\x00Alice\x002024-01-15T10:30:00+01:00\x00feat: add feature\x00Detailed description\nof the change.";
        let entry = parse_commit_log_line(line).expect("should parse");
        assert_eq!(entry.subject, "feat: add feature");
        assert_eq!(
            entry.body.as_deref(),
            Some("Detailed description\nof the change.")
        );
    }

    #[test]
    fn parse_commit_log_line_with_empty_body() {
        let line = "abc123\x00def456\x00\x00Alice\x002024-01-15T10:30:00+01:00\x00feat: add feature\x00  \n  ";
        let entry = parse_commit_log_line(line).expect("should parse");
        assert!(entry.body.is_none());
    }

    #[test]
    fn parse_commit_log_line_no_parents_no_refs() {
        let line = "abc123\x00\x00\x00Bob\x002024-01-15T10:30:00Z\x00Root commit";
        let entry = parse_commit_log_line(line).expect("should parse");
        assert!(entry.parents.is_empty());
        assert!(entry.refs.is_empty());
        assert!(entry.body.is_none());
    }

    #[test]
    fn parse_commit_log_line_malformed_returns_none() {
        assert!(parse_commit_log_line("not enough fields").is_none());
        assert!(parse_commit_log_line("a\0b\0c").is_none());
    }

    // --- get_commit_log integration tests ---

    /// Enough commits for both limits to bite: above `COMMIT_LOG_MAX_COUNT`, so
    /// a request over the limit is distinguishable from one under it. Without
    /// that, `len() <= 500` passes on a repository with three commits and on a
    /// build with the clamp deleted alike.
    const COMMIT_LOG_FIXTURE_COMMITS: u32 = COMMIT_LOG_MAX_COUNT + 5;

    #[tokio::test]
    async fn get_commit_log_returns_commits_for_a_fixture_repo() {
        let (_dir, repo) = history_fixture_repo(8);
        let result = get_commit_log(repo.to_string_lossy().to_string(), Some(5), None).await;
        let commits = result.expect("should succeed on a repo");
        assert_eq!(commits.len(), 5, "should respect count limit");
        // First commit should have a valid hash (40 hex chars)
        assert_eq!(commits[0].hash.len(), 40);
        assert!(commits[0].hash.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(commits[0].author_name, "Test");
        assert!(!commits[0].author_date.is_empty());
        assert_eq!(commits[0].subject, "commit 8", "newest commit comes first");
    }

    #[tokio::test]
    async fn get_commit_log_default_count_is_50() {
        let (_dir, repo) = history_fixture_repo(COMMIT_LOG_DEFAULT_COUNT + 5);
        let result = get_commit_log(repo.to_string_lossy().to_string(), None, None).await;
        let commits = result.expect("should succeed");
        // The fixture carries more than the default, so the default has to bite.
        assert_eq!(commits.len(), COMMIT_LOG_DEFAULT_COUNT as usize);
    }

    #[tokio::test]
    async fn get_commit_log_count_clamped_to_500() {
        let (_dir, repo) = history_fixture_repo(COMMIT_LOG_FIXTURE_COMMITS);
        let repo_str = repo.to_string_lossy().to_string();

        // Requesting 9999 is clamped to COMMIT_LOG_MAX_COUNT...
        let clamped = get_commit_log(repo_str.clone(), Some(9999), None)
            .await
            .expect("should succeed");
        assert_eq!(clamped.len(), COMMIT_LOG_MAX_COUNT as usize);

        // ...while a request below the limit is honoured as asked, which is
        // what makes the assertion above about the clamp and not about the
        // size of the fixture.
        let under = get_commit_log(repo_str, Some(COMMIT_LOG_MAX_COUNT - 100), None)
            .await
            .expect("should succeed");
        assert_eq!(under.len(), (COMMIT_LOG_MAX_COUNT - 100) as usize);
    }

    #[tokio::test]
    async fn get_commit_log_pagination_with_after() {
        let (_dir, repo) = history_fixture_repo(8);
        let repo_str = repo.to_string_lossy().to_string();

        // Get first page
        let page1 = get_commit_log(repo_str.clone(), Some(3), None)
            .await
            .expect("page 1");
        assert!(page1.len() >= 3, "need at least 3 commits for this test");

        // Get second page starting from the last commit of page 1
        let last_hash = &page1[2].hash;
        let page2 = get_commit_log(repo_str, Some(3), Some(last_hash.clone()))
            .await
            .expect("page 2");
        assert!(!page2.is_empty(), "page 2 should have commits");

        // First commit of page 2 should be the same as last of page 1 (the `after` hash)
        assert_eq!(
            page2[0].hash, *last_hash,
            "pagination should start from the `after` commit"
        );
    }

    #[tokio::test]
    async fn get_commit_log_fails_for_nonexistent_repo() {
        let result = get_commit_log("/nonexistent/repo".to_string(), None, None).await;
        assert!(result.is_err());
    }

    // --- get_stash_list tests ---

    #[tokio::test]
    async fn get_stash_list_nonexistent_repo_returns_empty() {
        let result = get_stash_list("/nonexistent/repo".to_string()).await;
        // run_silent returns None for non-git dir, so we get empty vec
        assert_eq!(result.unwrap(), vec![]);
    }

    // --- get_file_history integration tests ---

    #[tokio::test]
    async fn get_file_history_returns_commits_for_known_file() {
        let (_dir, repo) = history_fixture_repo(8);
        let result = get_file_history(
            repo.to_string_lossy().to_string(),
            "log.txt".to_string(),
            Some(5),
            None,
        )
        .await;
        let commits = result.expect("should succeed for a file in the repo");
        // Every fixture commit touches log.txt, so the count limit bites.
        assert_eq!(commits.len(), 5);
        assert_eq!(commits[0].subject, "commit 8", "newest commit comes first");
    }

    #[tokio::test]
    async fn get_file_history_nonexistent_file_returns_empty() {
        let (_dir, repo) = history_fixture_repo(1);
        let result = get_file_history(
            repo.to_string_lossy().to_string(),
            "nonexistent-file-xyz.txt".to_string(),
            Some(5),
            None,
        )
        .await;
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
        )
        .await;
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
        assert_eq!(lines[0].summary, "Initial commit");
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
        assert_eq!(lines[0].summary, "First");
        assert_eq!(lines[1].author, "Bob");
        assert_eq!(lines[1].hash, "bbbb234567890123456789012345678901234bbbb");
        assert_eq!(lines[1].summary, "Second");
    }

    #[test]
    fn parse_blame_porcelain_summary_cached_across_group_lines() {
        // The `summary` header appears only on the first line of a commit group;
        // subsequent lines of the same commit must inherit it from the cache.
        let output = "\
aaaa234567890123456789012345678901234aaaa 1 1 2
author Bob
author-time 1700000001
summary Add two lines
filename test.txt
\tLine one
aaaa234567890123456789012345678901234aaaa 2 2
\tLine two
";
        let lines = parse_blame_porcelain(output);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].summary, "Add two lines");
        assert_eq!(lines[1].summary, "Add two lines");
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
        // Each fixture commit appends one line, so line N blames to commit N —
        // a blame result the test can name instead of only describe.
        let (_dir, repo) = history_fixture_repo(6);
        let result =
            get_file_blame(repo.to_string_lossy().to_string(), "log.txt".to_string()).await;
        let lines = result.expect("should succeed for a file in the repo");
        assert_eq!(lines.len(), 6, "one line per fixture commit");
        // This test exercises the gix blame path (get_file_blame →
        // git_reads().blame()), so the `summary` assertion covers gix's
        // commit-summary population.
        for (i, bl) in lines.iter().enumerate() {
            let n = i + 1;
            assert_eq!(bl.hash.len(), 40, "hash should be 40 chars: {}", bl.hash);
            assert!(bl.hash.chars().all(|c| c.is_ascii_hexdigit()));
            assert_eq!(bl.author, "Test");
            assert_eq!(bl.author_time, FIXTURE_EPOCH + n as i64);
            assert_eq!(bl.summary, format!("commit {n}"));
            assert_eq!(
                bl.line_number, n as u32,
                "line numbers should be sequential"
            );
        }
    }

    #[tokio::test]
    async fn get_file_blame_fails_for_nonexistent_file() {
        let (_dir, repo) = history_fixture_repo(1);
        let result = get_file_blame(
            repo.to_string_lossy().to_string(),
            "nonexistent-file-xyz.txt".to_string(),
        )
        .await;
        assert!(result.is_err(), "blame on nonexistent file should fail");
    }

    #[tokio::test]
    async fn get_file_blame_untracked_file_returns_friendly_error() {
        let (_dir, repo) = history_fixture_repo(1);
        // A file that exists on disk but is not tracked by git.
        std::fs::write(repo.join("untracked.tmp"), "hello").expect("write untracked.tmp");
        let result = get_file_blame(
            repo.to_string_lossy().to_string(),
            "untracked.tmp".to_string(),
        )
        .await;
        let err = result.unwrap_err();
        assert!(
            err.contains("not tracked by git"),
            "expected user-friendly untracked message, got: {err}"
        );
    }

    #[tokio::test]
    async fn get_file_blame_fails_for_nonexistent_repo() {
        let result = get_file_blame("/nonexistent/repo".to_string(), "file.txt".to_string()).await;
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

        let merged =
            get_merged_branches_impl(&path).expect("get_merged_branches_impl should succeed");

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

        let merged =
            get_merged_branches_impl(&path).expect("get_merged_branches_impl should succeed");

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
        let out = git_cmd(&path)
            .args(["branch", "--list", "feature-x"])
            .run()
            .unwrap();
        assert!(
            out.stdout.contains("feature-x"),
            "branch should exist after creation"
        );
    }

    #[test]
    fn test_create_branch_with_checkout() {
        let (_dir, path) = setup_test_repo_with_commit();
        let result = create_branch_impl(path.to_str().unwrap(), "feature-checkout", None, true);
        assert!(
            result.is_ok(),
            "should create and checkout branch: {result:?}"
        );
        // Verify HEAD points to the new branch
        let out = git_cmd(&path)
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .run()
            .unwrap();
        assert_eq!(out.stdout.trim(), "feature-checkout");
    }

    #[test]
    fn test_create_branch_from_ref() {
        let (_dir, path) = setup_test_repo_with_commit();
        // Get HEAD commit hash to use as start point
        let out = git_cmd(&path).args(["rev-parse", "HEAD"]).run().unwrap();
        let commit_hash = out.stdout.trim().to_string();
        let result = create_branch_impl(
            path.to_str().unwrap(),
            "from-ref",
            Some(&commit_hash),
            false,
        );
        assert!(result.is_ok(), "should create branch from ref: {result:?}");
        let out = git_cmd(&path)
            .args(["branch", "--list", "from-ref"])
            .run()
            .unwrap();
        assert!(
            out.stdout.contains("from-ref"),
            "branch should exist after creation from ref"
        );
    }

    #[test]
    fn test_create_branch_refuses_empty_name() {
        let (_dir, path) = setup_test_repo_with_commit();
        let result = create_branch_impl(path.to_str().unwrap(), "", None, false);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().contains("empty"),
            "error should mention empty name"
        );
    }

    #[test]
    fn test_create_branch_refuses_invalid_name() {
        let (_dir, path) = setup_test_repo_with_commit();
        let result =
            create_branch_impl(path.to_str().unwrap(), "bad name with spaces", None, false);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().contains("spaces"),
            "error should mention spaces"
        );
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
        assert!(
            second.unwrap_err().contains("already exists"),
            "error should mention already exists"
        );
    }

    // --- get_branches_detail tests ---

    #[test]
    fn test_get_branches_detail_returns_rich_info() {
        let (_dir, repo_root) = history_fixture_repo(2);
        git_in(&repo_root, &["branch", "feature", "main~1"]);
        let head = git_in(&repo_root, &["rev-parse", "main"])
            .trim()
            .to_string();
        git_in(
            &repo_root,
            &["update-ref", "refs/remotes/origin/main", &head],
        );
        // The synthetic pointer the filter exists to drop.
        git_in(
            &repo_root,
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/main",
            ],
        );

        let branches = get_branches_detail_impl(&repo_root)
            .expect("get_branches_detail_impl should succeed on a repo");

        let names: Vec<&str> = branches.iter().map(|b| b.name.as_str()).collect();
        assert!(names.contains(&"main"), "got: {names:?}");
        assert!(names.contains(&"feature"), "got: {names:?}");
        assert!(names.contains(&"origin/main"), "got: {names:?}");

        // Branch names must not be empty
        for b in &branches {
            assert!(!b.name.is_empty(), "branch name should not be empty");
        }

        let current_branches: Vec<&BranchDetail> =
            branches.iter().filter(|b| b.is_current).collect();
        assert_eq!(
            current_branches.len(),
            1,
            "exactly one branch should be current, got: {:?}",
            current_branches.iter().map(|b| &b.name).collect::<Vec<_>>()
        );
        assert_eq!(current_branches[0].name, "main");
        assert!(
            current_branches[0].last_commit_date.is_some(),
            "current branch should have a last_commit_date"
        );

        // Every branch should have a non-empty last_commit_date
        assert!(
            branches.iter().all(|b| b.last_commit_date.is_some()),
            "every branch should have a last_commit_date, got: {branches:?}"
        );

        // No origin/HEAD pseudo-ref should appear. Note: refs/remotes/origin/HEAD's
        // short name collapses to just "origin" (not "origin/HEAD"), so the
        // `ends_with("/HEAD")` check alone does NOT catch it — this repo's real
        // `origin` remote is exactly the case that check silently missed before the
        // refname-based filter was added (see test_get_branches_detail_filters_remote_head_symref
        // below for a direct, controlled regression test of that).
        assert!(
            !branches.iter().any(|b| b.name.ends_with("/HEAD")),
            "origin/HEAD should be filtered out"
        );
        assert!(
            !branches.iter().any(|b| b.name == "origin"),
            "a phantom branch named exactly after the remote (from its collapsed HEAD symref) \
             should be filtered out: {:?}",
            branches.iter().map(|b| &b.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_get_branches_detail_filters_remote_head_symref() {
        // Direct regression test (independent of the ambient real repo used above)
        // for the bug found by review: a remote's HEAD symref
        // (refs/remotes/<remote>/HEAD, set by `git clone` or `git remote set-head`)
        // has a short name that collapses to just the remote's name — e.g. "origin"
        // — not "origin/HEAD". The original `name.ends_with("/HEAD")` filter (on the
        // short name) never matched this, so it leaked a phantom branch entry named
        // after the remote into every normally cloned repo's results.
        let (_dir, path) = setup_test_repo_with_commit();
        let (_remote_dir, remote_path) = setup_test_repo_with_commit();
        git_cmd(&path)
            .args(["remote", "add", "upstream", &remote_path.to_string_lossy()])
            .run()
            .expect("git remote add");
        git_cmd(&path)
            .args(["fetch", "upstream"])
            .run()
            .expect("git fetch");
        git_cmd(&path)
            .args(["remote", "set-head", "upstream", "-a"])
            .run()
            .expect("git remote set-head");

        let branches =
            get_branches_detail_impl(&path).expect("get_branches_detail_impl should succeed");

        assert!(
            !branches.iter().any(|b| b.name == "upstream"),
            "remote HEAD symref must not leak a phantom branch named after the remote: {:?}",
            branches.iter().map(|b| &b.name).collect::<Vec<_>>()
        );
        // The real upstream/<branch> entry must still be present and correctly remote.
        assert!(
            branches
                .iter()
                .any(|b| b.name.starts_with("upstream/") && b.is_remote),
            "expected a real upstream/* remote branch to still be present: {:?}",
            branches.iter().map(|b| &b.name).collect::<Vec<_>>()
        );
    }

    // --- get_git_branches tests ---

    /// Sets up a second local repo to act as a fetchable remote, adds it to `path`
    /// under `remote_name`, and fetches it. Returns the remote's TempDir too so it
    /// isn't dropped (and cleaned up) before the fetch/test body runs.
    fn add_and_fetch_named_remote(path: &Path, remote_name: &str) -> tempfile::TempDir {
        let (remote_dir, remote_path) = setup_test_repo_with_commit();
        git_cmd(path)
            .args(["remote", "add", remote_name, &remote_path.to_string_lossy()])
            .run()
            .expect("git remote add");
        git_cmd(path)
            .args(["fetch", remote_name])
            .run()
            .expect("git fetch");
        // `git clone` sets the remote's HEAD symref (refs/remotes/<remote>/HEAD)
        // automatically; a plain `remote add` + `fetch` does not. Set it explicitly
        // so tests built on this helper exercise the same ref shape a normally
        // cloned repo has — this symref is exactly what caused the "phantom branch
        // named after the remote" bug (its short name collapses to just the remote
        // name, e.g. "origin", not "origin/HEAD").
        git_cmd(path)
            .args(["remote", "set-head", remote_name, "-a"])
            .run()
            .expect("git remote set-head");
        remote_dir
    }

    #[tokio::test]
    async fn test_get_git_branches_basic_case() {
        let (_dir, path) = setup_test_repo_with_commit();
        // Rename whatever the default init branch is to something outside
        // MAIN_BRANCH_CANDIDATES so this test's "main"/is_main assertions are
        // deterministic regardless of the environment's init.defaultBranch.
        git_cmd(&path)
            .args(["branch", "-m", "trunk"])
            .run()
            .expect("rename default branch");
        git_cmd(&path)
            .args(["checkout", "-b", "main"])
            .run()
            .expect("create+checkout main");
        git_cmd(&path)
            .args(["branch", "feature-x"])
            .run()
            .expect("create feature-x");

        let branches = get_git_branches(path.to_string_lossy().to_string())
            .await
            .expect("get_git_branches should succeed");

        assert_eq!(
            branches.len(),
            3,
            "expected exactly 3 local branches, got: {branches:?}"
        );

        let current: Vec<_> = branches
            .iter()
            .filter(|b| b["is_current"].as_bool() == Some(true))
            .collect();
        assert_eq!(
            current.len(),
            1,
            "exactly one branch should be current, got: {branches:?}"
        );
        assert_eq!(current[0]["name"].as_str(), Some("main"));

        for b in &branches {
            let name = b["name"].as_str().unwrap();
            assert_eq!(
                b["is_main"].as_bool(),
                Some(is_main_branch(name)),
                "is_main mismatch for {name}: {b:?}"
            );
            assert_eq!(
                b["is_remote"].as_bool(),
                Some(false),
                "no branch should be reported remote in a repo with no remotes: {b:?}"
            );
        }
    }

    #[tokio::test]
    async fn test_get_git_branches_detects_non_origin_remote() {
        // Regression guard for the `is_remote = name.starts_with("origin/")` heuristic:
        // a remote added under any other name (a common fork-workflow shape) must still
        // be detected as remote.
        let (_dir, path) = setup_test_repo_with_commit();
        let _remote_dir = add_and_fetch_named_remote(&path, "upstream");

        let branches = get_git_branches(path.to_string_lossy().to_string())
            .await
            .expect("get_git_branches should succeed");

        let upstream_branches: Vec<_> = branches
            .iter()
            .filter(|b| b["name"].as_str().unwrap_or("").starts_with("upstream/"))
            .collect();
        assert!(
            !upstream_branches.is_empty(),
            "expected at least one upstream/* branch after fetch, got: {branches:?}"
        );
        for b in &upstream_branches {
            assert_eq!(
                b["is_remote"].as_bool(),
                Some(true),
                "a fetched non-origin remote branch must be reported is_remote: true: {b:?}"
            );
        }

        // Regression guard for the remote-HEAD-symref leak: refs/remotes/upstream/HEAD's
        // short name collapses to just "upstream", which must not appear as its own
        // phantom branch entry.
        assert!(
            !branches
                .iter()
                .any(|b| b["name"].as_str() == Some("upstream")),
            "remote HEAD symref must not leak a phantom branch named after the remote: {branches:?}"
        );
    }

    #[tokio::test]
    async fn test_get_git_branches_detached_head_produces_no_garbage_entry() {
        // Regression guard: in a detached-HEAD state (checked-out commit/tag,
        // mid-rebase, mid-bisect), `git branch -a` emits a synthetic pseudo-entry
        // like `(HEAD detached at abc1234)` whose fields aren't a real ref and
        // contain spaces. Confirmed pre-existing (present before this session's
        // is_remote fix too): naive splitn(3, ' ') parsing turned this into a
        // bogus branch named "(HEAD", polluting the branch switcher's list.
        let (_dir, path) = setup_test_repo_with_commit();
        git_cmd(&path)
            .args(["checkout", "--detach", "HEAD"])
            .run()
            .expect("git checkout --detach");

        let branches = get_git_branches(path.to_string_lossy().to_string())
            .await
            .expect("get_git_branches should succeed");

        assert!(
            !branches
                .iter()
                .any(|b| b["name"].as_str().unwrap_or("").starts_with("(HEAD")),
            "detached HEAD pseudo-entry leaked into results: {branches:?}"
        );
        assert!(
            !branches
                .iter()
                .any(|b| b["is_current"].as_bool() == Some(true)),
            "no real branch should be current while HEAD is detached: {branches:?}"
        );
    }

    #[tokio::test]
    async fn test_get_git_branches_unborn_head_returns_empty() {
        // Edge case: a freshly `git init`'d repo with zero commits has no refs at
        // all yet (unborn HEAD) — get_git_branches must return an empty list, not
        // error or panic.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().to_path_buf();
        git_cmd(&path).args(["init"]).run().expect("git init");

        let branches = get_git_branches(path.to_string_lossy().to_string())
            .await
            .expect("get_git_branches should succeed on an unborn-HEAD repo");

        assert!(
            branches.is_empty(),
            "a repo with no commits should have no branches, got: {branches:?}"
        );
    }

    #[tokio::test]
    async fn test_get_git_branches_local_branch_named_like_origin_prefix() {
        // Regression guard: a LOCAL branch that happens to be named `origin/foo` (legal
        // via `git checkout -b origin/foo`) must not be misreported as remote just
        // because its short name starts with "origin/".
        let (_dir, path) = setup_test_repo_with_commit();
        git_cmd(&path)
            .args(["branch", "origin/foo"])
            .run()
            .expect("create local branch named origin/foo");

        let branches = get_git_branches(path.to_string_lossy().to_string())
            .await
            .expect("get_git_branches should succeed");

        let b = branches
            .iter()
            .find(|b| b["name"].as_str() == Some("origin/foo"))
            .unwrap_or_else(|| panic!("expected a branch named origin/foo, got: {branches:?}"));
        assert_eq!(
            b["is_remote"].as_bool(),
            Some(false),
            "a LOCAL branch literally named origin/foo must not be misreported as remote: {b:?}"
        );
    }

    #[tokio::test]
    async fn test_get_git_branches_and_get_branches_detail_agree_on_shared_fields() {
        // These are two independently implemented branch enumerators backing two
        // different UI surfaces (BranchSwitcher.tsx and GitPanel/BranchesTab.tsx).
        // Nothing else checks they classify the same repo state the same way — this
        // guards against future drift between them.
        let (_dir, path) = setup_test_repo_with_commit();
        git_cmd(&path)
            .args(["branch", "-m", "trunk"])
            .run()
            .expect("rename default branch");
        git_cmd(&path)
            .args(["checkout", "-b", "main"])
            .run()
            .expect("create+checkout main");
        git_cmd(&path)
            .args(["branch", "feature-x"])
            .run()
            .expect("create feature-x");
        let _remote_dir = add_and_fetch_named_remote(&path, "upstream");

        let simple = get_git_branches(path.to_string_lossy().to_string())
            .await
            .expect("get_git_branches should succeed");
        let detailed =
            get_branches_detail_impl(&path).expect("get_branches_detail_impl should succeed");

        let simple_names: std::collections::HashSet<&str> =
            simple.iter().map(|b| b["name"].as_str().unwrap()).collect();
        let detailed_names: std::collections::HashSet<&str> =
            detailed.iter().map(|b| b.name.as_str()).collect();

        for name in simple_names.intersection(&detailed_names) {
            let s = simple
                .iter()
                .find(|b| b["name"].as_str() == Some(*name))
                .unwrap();
            let d = detailed.iter().find(|b| b.name == *name).unwrap();
            assert_eq!(
                s["is_remote"].as_bool(),
                Some(d.is_remote),
                "get_git_branches and get_branches_detail_impl disagree on is_remote for {name}: {s:?} vs {d:?}"
            );
            assert_eq!(
                s["is_main"].as_bool(),
                Some(d.is_main),
                "get_git_branches and get_branches_detail_impl disagree on is_main for {name}: {s:?} vs {d:?}"
            );
        }

        // Branch names present in one result but not the other must be reported
        // explicitly, not silently intersected away.
        let only_in_simple: Vec<&&str> = simple_names.difference(&detailed_names).collect();
        let only_in_detailed: Vec<&&str> = detailed_names.difference(&simple_names).collect();
        assert!(
            only_in_simple.is_empty() && only_in_detailed.is_empty(),
            "branch sets diverge: only in get_git_branches: {only_in_simple:?}, only in get_branches_detail_impl: {only_in_detailed:?}"
        );

        // Regression guard: this repo now has a real refs/remotes/upstream/HEAD symref
        // (add_and_fetch_named_remote sets it, matching what `git clone` does), so this
        // is exactly the scenario where both functions' remote-HEAD-symref filtering
        // must actually be exercised, not just agree by accident.
        assert!(
            !simple_names.contains("upstream") && !detailed_names.contains("upstream"),
            "remote HEAD symref must not leak a phantom branch named after the remote in \
             either function: simple={simple_names:?} detailed={detailed_names:?}"
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
        std::process::Command::new("git")
            .current_dir(&path)
            .args(["branch", "-M", "main"])
            .output()
            .expect("rename to main");
        // Create a branch to delete at same commit as main (fully merged)
        std::process::Command::new("git")
            .current_dir(&path)
            .args(["branch", "to-delete"])
            .output()
            .expect("create branch");

        let result = delete_branch_impl(&path_str, "to-delete", false);
        assert!(
            result.is_ok(),
            "safe delete of merged branch should succeed: {result:?}"
        );
        let r = result.unwrap();
        assert_eq!(r.branch, "to-delete");
        assert!(!r.was_force);
        assert!(r.deleted);
    }

    #[test]
    fn test_delete_branch_force() {
        let (_dir, path) = setup_test_repo_with_commit();
        let path_str = path.to_string_lossy().to_string();
        std::process::Command::new("git")
            .current_dir(&path)
            .args(["branch", "-M", "main"])
            .output()
            .expect("rename to main");
        // Create a branch with unmerged commits
        std::process::Command::new("git")
            .current_dir(&path)
            .args(["checkout", "-b", "unmerged-branch"])
            .output()
            .expect("checkout -b");
        std::fs::write(path.join("unmerged.txt"), "data").expect("write");
        std::process::Command::new("git")
            .current_dir(&path)
            .args(["add", "unmerged.txt"])
            .output()
            .expect("add");
        std::process::Command::new("git")
            .current_dir(&path)
            .args(["commit", "-m", "unmerged commit"])
            .output()
            .expect("commit");
        // Switch back to main so we can delete the unmerged branch
        std::process::Command::new("git")
            .current_dir(&path)
            .args(["checkout", "main"])
            .output()
            .expect("checkout main");

        // Safe delete should fail (unmerged)
        let safe_result = delete_branch_impl(&path_str, "unmerged-branch", false);
        assert!(
            safe_result.is_err(),
            "safe delete of unmerged branch should fail"
        );

        // Force delete should succeed
        let result = delete_branch_impl(&path_str, "unmerged-branch", true);
        assert!(result.is_ok(), "force delete should succeed: {result:?}");
        let r = result.unwrap();
        assert_eq!(r.branch, "unmerged-branch");
        assert!(r.was_force);
        assert!(r.deleted);
    }

    // Catches: force deletion losing unique commits, clobbering archives, or deleting checked-out branches.
    #[test]
    fn force_delete_preserves_unique_tip_archives_collisions_at_a_suffix_and_refuses_checkouts() {
        let (dir, path) = setup_test_repo_with_commit();
        let repo = path.to_string_lossy();
        let git = |args: &[&str]| {
            let output = std::process::Command::new("git")
                .current_dir(&path)
                .args(args)
                .output()
                .expect("git");
            assert!(
                output.status.success(),
                "{args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout)
                .expect("utf8")
                .trim()
                .to_owned()
        };
        let branch_exists = |name: &str| {
            std::process::Command::new("git")
                .current_dir(&path)
                .args(["show-ref", "--verify", &format!("refs/heads/{name}")])
                .output()
                .expect("git")
                .status
                .success()
        };
        git(&["branch", "-M", "main"]);
        let original = git(&["rev-parse", "HEAD"]);
        git(&["checkout", "-b", "unique"]);
        git(&["commit", "--allow-empty", "-m", "unique commit"]);
        let unique = git(&["rev-parse", "HEAD"]);
        assert_ne!(original, unique);
        assert!(delete_branch_impl(&repo, "unique", true).is_err());
        git(&["checkout", "main"]);
        delete_branch_impl(&repo, "unique", true).expect("force delete unique");
        assert_eq!(git(&["rev-parse", "refs/archive/unique"]), unique);
        assert!(!branch_exists("unique"));
        git(&["branch", "integrated"]);
        delete_branch_impl(&repo, "integrated", true).expect("force delete integrated");
        assert_eq!(git(&["rev-parse", "refs/archive/integrated"]), original);
        git(&["branch", "collision", &unique]);
        git(&["update-ref", "refs/archive/collision", &original]);
        delete_branch_impl(&repo, "collision", true).expect("archive collision uses a suffix");
        assert!(!branch_exists("collision"));
        assert_eq!(git(&["rev-parse", "refs/archive/collision"]), original);
        let suffixed = format!("refs/archive/collision-{}", &unique[..7]);
        assert_eq!(git(&["rev-parse", &suffixed]), unique);
        // Catches: a branch name force-deleted once, recreated, becoming undeletable.
        git(&["branch", "collision", &unique]);
        delete_branch_impl(&repo, "collision", true)
            .expect("second deletion reuses suffixed archive");
        assert_eq!(git(&["rev-parse", &suffixed]), unique);
        git(&["branch", "same-archive"]);
        git(&["update-ref", "refs/archive/same-archive", &original]);
        delete_branch_impl(&repo, "same-archive", true).expect("reuse exact archive");
        assert_eq!(git(&["rev-parse", "refs/archive/same-archive"]), original);
        let checkout = dir.path().join("linked-checkout");
        git(&[
            "worktree",
            "add",
            "-b",
            "linked",
            checkout.to_str().expect("path"),
        ]);
        assert!(delete_branch_impl(&repo, "linked", true).is_err());
        assert_eq!(git(&["rev-parse", "refs/heads/linked"]), original);
    }

    #[test]
    fn test_delete_branch_refuses_main() {
        let (_dir, path) = setup_test_repo_with_commit();
        let path_str = path.to_string_lossy().to_string();
        std::process::Command::new("git")
            .current_dir(&path)
            .args(["branch", "-M", "main"])
            .output()
            .expect("rename to main");

        let result = delete_branch_impl(&path_str, "main", false);
        assert!(result.is_err(), "should refuse to delete main branch");
        let err = result.unwrap_err();
        assert!(
            err.contains("main") || err.contains("protected"),
            "error should mention protection: {err}"
        );
    }

    #[test]
    fn test_delete_branch_refuses_current() {
        let (_dir, path) = setup_test_repo_with_commit();
        let path_str = path.to_string_lossy().to_string();
        // Rename to something non-main so is_main_branch check doesn't fire first
        std::process::Command::new("git")
            .current_dir(&path)
            .args(["branch", "-M", "feature-branch"])
            .output()
            .expect("rename");

        let result = delete_branch_impl(&path_str, "feature-branch", false);
        assert!(result.is_err(), "should refuse to delete current branch");
        let err = result.unwrap_err();
        assert!(
            err.contains("current") || err.contains("checked out"),
            "error should mention current branch: {err}"
        );
    }

    #[test]
    fn test_delete_branch_refuses_empty_name() {
        let (_dir, path) = setup_test_repo_with_commit();
        let path_str = path.to_string_lossy().to_string();

        let result = delete_branch_impl(&path_str, "", false);
        assert!(result.is_err(), "empty branch name should fail");
        let err = result.unwrap_err();
        assert!(
            err.to_lowercase().contains("empty"),
            "error should mention empty: {err}"
        );
    }

    // --- get_recent_branches_impl tests ---

    #[test]
    fn test_get_recent_branches_returns_checkouts_most_recent_first() {
        let (_dir, repo_root) = history_fixture_repo(1);
        for name in ["alpha", "beta", "gamma"] {
            git_in(&repo_root, &["switch", "-q", "-c", name]);
        }
        git_in(&repo_root, &["switch", "-q", "main"]);

        let branches = get_recent_branches_impl(&repo_root, 5).expect("should not error on a repo");
        assert_eq!(branches, vec!["main", "gamma", "beta", "alpha"]);
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
        assert!(
            branches.len() <= 2,
            "limit of 2 must be respected, got: {branches:?}"
        );
    }

    #[test]
    fn test_get_recent_branches_on_nonexistent_path() {
        let result = get_recent_branches_impl(Path::new("/nonexistent/path/xyz"), 5);
        // A non-existent path must produce an error (git spawn or non-zero exit)
        assert!(result.is_err(), "nonexistent path should return an error");
    }

    // --- Working-tree status single-flight ---

    /// Collapsing concurrent reads is only safe between reads of the same
    /// working tree. A read requested after a stage or a discard used to join
    /// one that started before it, so the panel that refetches right after the
    /// mutation was answered with pre-mutation state and stayed wrong until an
    /// unrelated watcher event forced another read.
    #[test]
    fn a_read_after_a_mutation_does_not_join_one_started_before_it() {
        let repo = "/test/single-flight-epoch";
        let leader = enter_flight(repo);
        assert!(matches!(leader, Flight::Lead(_)));
        assert!(
            matches!(enter_flight(repo), Flight::Follow(_)),
            "a concurrent read of the same working tree collapses onto the leader"
        );

        bump_working_tree_epoch(repo);
        let after = enter_flight(repo);
        assert!(
            matches!(after, Flight::Lead(_)),
            "the mutation ended that generation: this read must run itself"
        );
        // Two callers after the same mutation still collapse — that is the pair
        // (ChangesTab and StatusBar) the single-flight exists for.
        assert!(matches!(enter_flight(repo), Flight::Follow(_)));

        drop(leader);
        drop(after);
    }

    #[test]
    fn a_finished_read_leaves_no_entry_behind() {
        let repo = "/test/single-flight-cleanup";
        let leader = enter_flight(repo);
        drop(leader);
        assert!(
            matches!(enter_flight(repo), Flight::Lead(_)),
            "the next read leads rather than waiting on a computation that ended"
        );
    }

    // --- Tests for git_apply_reverse_patch ---

    #[tokio::test]
    async fn apply_reverse_patch_reverts_working_tree_hunk() {
        let (_dir, path) = setup_test_repo_with_commit();
        // Write a multi-line file and commit it
        std::fs::write(path.join("multi.txt"), "line1\nline2\nline3\n").expect("write");
        std::process::Command::new("git")
            .current_dir(&path)
            .args(["add", "multi.txt"])
            .output()
            .expect("add");
        std::process::Command::new("git")
            .current_dir(&path)
            .args(["commit", "-m", "add multi"])
            .output()
            .expect("commit");

        // Modify the file (working tree change)
        std::fs::write(path.join("multi.txt"), "line1\nMODIFIED\nline3\n").expect("modify");

        // Build a patch that represents this change (get it from git diff)
        let diff_out = std::process::Command::new("git")
            .current_dir(&path)
            .args(["diff", "--color=never", "--", "multi.txt"])
            .output()
            .expect("diff");
        let patch = String::from_utf8_lossy(&diff_out.stdout).to_string();
        assert!(!patch.is_empty(), "diff should produce output");

        // Apply the reverse patch to revert the hunk
        let result = git_apply_reverse_patch(path.to_string_lossy().to_string(), patch, None).await;
        assert!(
            result.is_ok(),
            "reverse patch should succeed: {:?}",
            result.err()
        );

        // Verify file is restored to committed state
        let content = std::fs::read_to_string(path.join("multi.txt")).expect("read");
        assert_eq!(content, "line1\nline2\nline3\n");
    }

    #[tokio::test]
    async fn apply_reverse_patch_unstages_cached_hunk() {
        let (_dir, path) = setup_test_repo_with_commit();
        std::fs::write(path.join("staged.txt"), "original\n").expect("write");
        std::process::Command::new("git")
            .current_dir(&path)
            .args(["add", "staged.txt"])
            .output()
            .expect("add");
        std::process::Command::new("git")
            .current_dir(&path)
            .args(["commit", "-m", "add staged"])
            .output()
            .expect("commit");

        // Modify and stage the change
        std::fs::write(path.join("staged.txt"), "modified\n").expect("modify");
        std::process::Command::new("git")
            .current_dir(&path)
            .args(["add", "staged.txt"])
            .output()
            .expect("stage");

        // Get the staged diff
        let diff_out = std::process::Command::new("git")
            .current_dir(&path)
            .args(["diff", "--cached", "--color=never", "--", "staged.txt"])
            .output()
            .expect("diff --cached");
        let patch = String::from_utf8_lossy(&diff_out.stdout).to_string();
        assert!(!patch.is_empty(), "staged diff should produce output");

        // Apply reverse patch with staged scope (unstage the hunk)
        let result = git_apply_reverse_patch(
            path.to_string_lossy().to_string(),
            patch,
            Some("staged".to_string()),
        )
        .await;
        assert!(
            result.is_ok(),
            "reverse patch --cached should succeed: {:?}",
            result.err()
        );

        // Verify: staged diff should now be empty (unstaged)
        let verify = std::process::Command::new("git")
            .current_dir(&path)
            .args(["diff", "--cached", "--name-only"])
            .output()
            .expect("verify");
        let staged_files = String::from_utf8_lossy(&verify.stdout).trim().to_string();
        assert!(
            staged_files.is_empty() || !staged_files.contains("staged.txt"),
            "staged.txt should no longer be staged"
        );
    }

    #[tokio::test]
    async fn apply_reverse_patch_rejects_malformed_patch() {
        let (_dir, path) = setup_test_repo_with_commit();
        let result = git_apply_reverse_patch(
            path.to_string_lossy().to_string(),
            "this is not a valid patch".to_string(),
            None,
        )
        .await;
        assert!(result.is_err(), "malformed patch should be rejected");
    }

    #[tokio::test]
    async fn apply_reverse_patch_rejects_empty_patch() {
        let (_dir, path) = setup_test_repo_with_commit();
        let result =
            git_apply_reverse_patch(path.to_string_lossy().to_string(), String::new(), None).await;
        assert!(result.is_err(), "empty patch should be rejected");
    }
}

// Blocking domain operations. Runtime scheduling belongs to the app adapter.
pub fn compute_working_tree_status_blocking(path: String) -> Result<WorkingTreeStatus, String> {
    let repo_path = PathBuf::from(&path);
    let out = git_cmd(&repo_path)
        // `--untracked-files=all` expands a wholly-untracked directory into its
        // individual files (`? dir/a`, `? dir/b`) instead of one collapsed
        // `? dir/` entry that ChangesTab can't open a diff for.
        .args([
            "status",
            "--porcelain=v2",
            "--branch",
            "--show-stash",
            "--untracked-files=all",
        ])
        .run()
        .map_err(|e| format!("git status failed: {e}"))?;
    let mut status = parse_porcelain_v2(&out.stdout);
    enrich_with_numstat(&repo_path, &mut status.staged, true);
    enrich_with_numstat(&repo_path, &mut status.unstaged, false);
    Ok(status)
}
pub fn get_remote_url_blocking(path: String) -> Result<Option<String>, String> {
    Ok(read_remote_url(Path::new(&path)))
}

pub fn get_branch_base_blocking(
    path: String,
    branch_name: String,
) -> Result<Option<String>, String> {
    Ok(crate::worktree::get_branch_base(&path, &branch_name))
}

pub fn get_recent_commits_blocking(
    path: String,
    count: Option<u32>,
) -> Result<Vec<RecentCommit>, String> {
    {
        let repo_path = PathBuf::from(&path);
        let n = count.unwrap_or(5).min(20).to_string();

        let out = git_cmd(&repo_path)
            .args(["log", "--format=%H%x00%h%x00%s", "-n", &n])
            .run()
            .map_err(|e| format!("git log failed: {e}"))?;

        let commits = out
            .stdout
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
}

pub fn get_git_diff_blocking(path: String, scope: Option<String>) -> Result<String, String> {
    {
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
}

pub fn get_diff_stats_blocking(path: String, scope: Option<String>) -> Result<DiffStats, String> {
    Ok(git_reads().diff_stats(Path::new(&path), scope.as_deref()))
}

pub fn get_changed_files_blocking(
    path: String,
    scope: Option<String>,
) -> Result<Vec<ChangedFile>, String> {
    {
        let repo_path = PathBuf::from(&path);

        if resolve_git_dir(&repo_path).is_none() {
            return Ok(vec![]);
        }

        // --name-status suppresses --numstat. Raw status records coexist with
        // numstat; NUL delimiters preserve literal paths and rename destinations.
        let mut args = diff_base_args(&scope)?;
        args.extend(["--raw".into(), "--numstat".into(), "-z".into()]);
        let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let combined_out = git_cmd(&repo_path)
            .args(&args_str)
            .run()
            .map_err(|e| format!("git diff failed: {e}"))?;

        let mut stats_map: HashMap<String, (u32, u32)> = HashMap::new();
        let mut status_map: HashMap<String, String> = HashMap::new();
        let mut records = combined_out.stdout.split('\0');
        while let Some(record) = records.next() {
            if record.starts_with(':') {
                let Some(status) = record.split_whitespace().last() else {
                    continue;
                };
                let Some(path) = records.next() else { continue };
                let path = if status.starts_with(['R', 'C']) {
                    let Some(destination) = records.next() else {
                        continue;
                    };
                    destination
                } else {
                    path
                };
                status_map.insert(path.to_owned(), status.to_owned());
            } else if let Some((add, rest)) = record.split_once('\t') {
                let Some((del, path)) = rest.split_once('\t') else {
                    continue;
                };
                let path = if path.is_empty() {
                    // A rename numstat record carries old and new paths separately.
                    let Some(_source) = records.next() else {
                        continue;
                    };
                    let Some(destination) = records.next() else {
                        continue;
                    };
                    destination
                } else {
                    path
                };
                stats_map.insert(
                    path.to_owned(),
                    (add.parse().unwrap_or(0), del.parse().unwrap_or(0)),
                );
            }
        }

        // Combine stats and status. Use stats_map as primary (has all diffed files).
        let mut files: Vec<ChangedFile> = stats_map
            .into_iter()
            .map(|(path, (additions, deletions))| {
                let status = status_map.remove(&path).unwrap_or_else(|| "M".to_string());
                ChangedFile {
                    path,
                    status,
                    additions,
                    deletions,
                }
            })
            .collect();

        // Any remaining status_map entries (no stats) — shouldn't happen but handle gracefully
        for (path, status) in status_map {
            files.push(ChangedFile {
                path,
                status,
                additions: 0,
                deletions: 0,
            });
        }

        // For working tree scope, also include untracked files
        if scope.is_none() {
            let untracked_out = git_cmd(&repo_path)
                .args(["ls-files", "--others", "--exclude-standard", "-z"])
                .run_silent();

            if let Some(ref out) = untracked_out {
                for file_path in out.stdout.split('\0') {
                    if file_path.is_empty() {
                        continue;
                    }
                    let full_path = repo_path.join(file_path);
                    let additions = File::open(&full_path)
                        .map(|f| {
                            let mut count = 0u32;
                            for line in BufReader::new(f).lines() {
                                match line {
                                    Ok(_) => count += 1,
                                    Err(_) => return 0,
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
}

pub fn get_file_diff_blocking(
    path: String,
    file: String,
    scope: Option<String>,
    untracked: Option<bool>,
) -> Result<String, String> {
    {
        let repo_path = PathBuf::from(&path);

        // For untracked files, use --no-index to generate a diff against the null device.
        // Deleted files don't exist on disk — skip this block entirely and fall
        // through to the regular `git diff` which reads from the index.
        // Also handle the "head" scope (editor gutter): an untracked file has no
        // HEAD entry, so `git diff HEAD` would be empty — fall back to the
        // all-added --no-index diff so new files show every line as added.
        let scope_uses_worktree = scope.is_none() || scope.as_deref() == Some("head");
        if scope_uses_worktree && repo_path.join(&file).exists() {
            let full_path = repo_path.join(&file);

            // Security: prevent path traversal (e.g. "../../etc/passwd")
            let canonical_repo = repo_path
                .canonicalize()
                .map_err(|e| format!("Failed to resolve repo path: {e}"))?;
            let canonical_file = full_path
                .canonicalize()
                .map_err(|e| format!("Failed to resolve file path: {e}"))?;
            if !canonical_file.starts_with(&canonical_repo) {
                return Err("Access denied: file is outside repository".to_string());
            }

            // A caller that already knows the status skips the subprocess probe.
            let is_untracked = match untracked {
                Some(known) => known,
                None => !is_file_tracked(&repo_path, &file)?,
            };

            if is_untracked {
                let full_path_str = full_path.to_string_lossy();
                let raw = git_cmd(&repo_path)
                    .args([
                        "diff",
                        "--color=never",
                        "--no-index",
                        "--",
                        NULL_DEVICE,
                        &full_path_str,
                    ])
                    .run_raw()
                    .map_err(|e| format!("Failed to diff untracked file: {e}"))?;
                let code = raw.status.code().unwrap_or(-1);
                if code > 1 {
                    let stderr = String::from_utf8_lossy(&raw.stderr);
                    return Err(format!(
                        "git diff --no-index failed (exit {code}): {stderr}"
                    ));
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
}

pub fn get_gutter_changes_blocking(
    path: String,
    file: String,
    scope: Option<String>,
) -> Result<Vec<GutterChange>, String> {
    let repo_path = PathBuf::from(&path);
    let tracked = is_file_tracked(&repo_path, &file)?;
    if !tracked {
        return Ok(Vec::new());
    }
    let diff = get_file_diff_blocking(path, file, scope, Some(false))?;
    Ok(parse_diff_to_changes(&diff))
}

pub fn get_git_branches_blocking(path: String) -> Result<Vec<serde_json::Value>, String> {
    {
        let repo_path = PathBuf::from(&path);

        let out = git_cmd(&repo_path)
            .args([
                "branch",
                "-a",
                "--format=%(refname:short) %(HEAD) %(refname)",
            ])
            .run()
            .map_err(|e| format!("git branch failed: {e}"))?;

        let mut branches: Vec<serde_json::Value> = out
            .stdout
            .lines()
            .filter(|line| !line.is_empty())
            .filter_map(|line| {
                let parts: Vec<&str> = line.splitn(3, ' ').collect();
                let name = parts[0].trim().to_string();
                let is_current = parts.get(1).is_some_and(|s| s.trim() == "*");
                // Use the full refname (refs/remotes/…), not the short name, so a
                // local branch literally named `origin/foo` isn't misreported as
                // remote, and a non-`origin` remote (e.g. `upstream/main`) is still
                // correctly detected. Mirrors get_branches_detail_impl's approach.
                let refname = parts.get(2).map(|s| s.trim()).unwrap_or("");
                // In a detached-HEAD state (checked-out commit/tag, mid-rebase,
                // mid-bisect), `git branch -a` emits a synthetic pseudo-entry like
                // `(HEAD detached at abc1234)` whose refname:short/refname/HEAD
                // fields are NOT a real ref (and contain spaces, which also breaks
                // the splitn(3, ' ') assumption that a ref has none). Skip anything
                // that isn't a real ref — get_branches_detail_impl is immune to this
                // by construction, since for-each-ref only ever walks real refs.
                if !refname.starts_with("refs/") {
                    return None;
                }
                let is_remote = refname.starts_with("refs/remotes/");
                // Skip a remote's HEAD symref (refs/remotes/<remote>/HEAD), which
                // otherwise leaks a phantom branch named after the remote itself
                // (e.g. "origin") in every normally cloned repo — see
                // get_branches_detail_impl's identical check for why this must use
                // the full refname, not the short name.
                if is_remote && refname.ends_with("/HEAD") {
                    return None;
                }
                Some(serde_json::json!({
                    "name": name,
                    "is_current": is_current,
                    "is_remote": is_remote,
                    "is_main": is_main_branch(&name),
                }))
            })
            .collect();

        sort_branches(&mut branches);

        Ok(branches)
    }
}

pub fn get_recent_branches_blocking(
    path: String,
    limit: Option<usize>,
) -> Result<Vec<String>, String> {
    get_recent_branches_impl(Path::new(&path), limit.unwrap_or(5))
}

pub fn git_stage_files_blocking(path: String, files: Vec<String>) -> Result<(), String> {
    let repo = path.clone();
    let result = (|| {
        let repo_path = PathBuf::from(&path);
        validate_paths_within_repo(&repo_path, &files)?;
        let mut args: Vec<String> = vec!["add".into(), "--".into()];
        args.extend(files);
        let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        git_cmd(&repo_path)
            .args(&args_str)
            .run()
            .map_err(|e| diagnose_git_failure(&repo_path, "git add failed", e))?;
        Ok(())
    })();
    result.inspect(|_| bump_working_tree_epoch(&repo))
}

pub fn git_unstage_files_blocking(path: String, files: Vec<String>) -> Result<(), String> {
    let repo = path.clone();
    let result = (|| {
        let repo_path = PathBuf::from(&path);
        validate_paths_within_repo(&repo_path, &files)?;
        let mut args: Vec<String> = vec!["restore".into(), "--staged".into(), "--".into()];
        args.extend(files);
        let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        git_cmd(&repo_path)
            .args(&args_str)
            .run()
            .map_err(|e| diagnose_git_failure(&repo_path, "git restore --staged failed", e))?;
        Ok(())
    })();
    result.inspect(|_| bump_working_tree_epoch(&repo))
}

pub fn git_discard_files_blocking(path: String, files: Vec<String>) -> Result<(), String> {
    let repo = path.clone();
    let result = (|| {
        let repo_path = PathBuf::from(&path);
        validate_paths_within_repo(&repo_path, &files)?;
        let mut args: Vec<String> = vec!["restore".into(), "--".into()];
        args.extend(files);
        let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        git_cmd(&repo_path)
            .args(&args_str)
            .run()
            .map_err(|e| diagnose_git_failure(&repo_path, "git restore failed", e))?;
        Ok(())
    })();
    result.inspect(|_| bump_working_tree_epoch(&repo))
}

pub fn git_apply_reverse_patch_blocking(
    path: String,
    patch: String,
    scope: Option<String>,
) -> Result<(), String> {
    let repo = path.clone();
    let result = (|| {
        let repo_path = PathBuf::from(&path);

        // Validate patch is non-empty and looks like a unified diff
        let trimmed = patch.trim();
        if trimmed.is_empty() {
            return Err("Patch is empty".to_string());
        }
        if !trimmed.starts_with("diff --git") && !trimmed.starts_with("---") {
            return Err("Invalid patch: must start with 'diff --git' or '---'".to_string());
        }

        let mut args = vec!["apply", "--reverse"];
        match scope.as_deref() {
            None => {}
            Some("staged") => args.push("--cached"),
            Some(other) => {
                return Err(format!(
                    "Invalid scope: {:?}. Expected None or \"staged\"",
                    other
                ));
            }
        }

        use std::io::Write;
        use std::process::{Command, Stdio};

        let git_bin = tuic_core::cli::resolve_cli("git");
        let mut child = Command::new(&git_bin)
            .current_dir(&repo_path)
            .arg("--no-optional-locks")
            .args(&args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn git apply: {e}"))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(patch.as_bytes())
                .map_err(|e| format!("Failed to write patch to stdin: {e}"))?;
        }

        let output = child
            .wait_with_output()
            .map_err(|e| format!("Failed to wait for git apply: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(format!("git apply --reverse failed: {stderr}"));
        }

        Ok(())
    })();
    result.inspect(|_| bump_working_tree_epoch(&repo))
}

pub fn git_commit_blocking(
    path: String,
    message: String,
    amend: Option<bool>,
) -> Result<String, String> {
    let repo = path.clone();
    let result = (|| {
        let repo_path = PathBuf::from(&path);
        let mut args: Vec<String> = vec!["commit".into(), "-m".into(), message];
        if amend == Some(true) {
            args.push("--amend".into());
        }
        let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        git_cmd(&repo_path)
            .args(&args_str)
            .run()
            .map_err(|e| diagnose_git_failure(&repo_path, "git commit failed", e))?;

        let hash_out = git_cmd(&repo_path)
            .args(["rev-parse", "HEAD"])
            .run()
            .map_err(|e| format!("Failed to read commit hash: {e}"))?;
        Ok(hash_out.stdout.trim().to_string())
    })();
    result.inspect(|_| bump_working_tree_epoch(&repo))
}

pub fn get_commit_log_blocking(
    path: String,
    count: Option<u32>,
    after: Option<String>,
) -> Result<Vec<CommitLogEntry>, String> {
    git_reads().commit_log(Path::new(&path), count, after)
}

pub fn get_stash_list_blocking(path: String) -> Result<Vec<StashEntry>, String> {
    {
        let repo_path = PathBuf::from(&path);

        let out = git_cmd(&repo_path)
            .args(["stash", "list", "--format=%gd%x00%s%x00%H"])
            .run_silent();

        let Some(out) = out else {
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
}

pub fn git_stash_apply_blocking(path: String, stash_ref: String) -> Result<(), String> {
    let repo = path.clone();
    let result = (|| {
        let repo_path = PathBuf::from(&path);
        validate_stash_ref(&stash_ref)?;
        git_cmd(&repo_path)
            .args(["stash", "apply", &stash_ref])
            .run()
            .map_err(|e| diagnose_git_failure(&repo_path, "git stash apply failed", e))?;
        Ok(())
    })();
    result.inspect(|_| bump_working_tree_epoch(&repo))
}

pub fn git_stash_pop_blocking(path: String, stash_ref: String) -> Result<(), String> {
    let repo = path.clone();
    let result = (|| {
        let repo_path = PathBuf::from(&path);
        validate_stash_ref(&stash_ref)?;
        git_cmd(&repo_path)
            .args(["stash", "pop", &stash_ref])
            .run()
            .map_err(|e| diagnose_git_failure(&repo_path, "git stash pop failed", e))?;
        Ok(())
    })();
    result.inspect(|_| bump_working_tree_epoch(&repo))
}

pub fn git_stash_drop_blocking(path: String, stash_ref: String) -> Result<(), String> {
    {
        let repo_path = PathBuf::from(&path);
        validate_stash_ref(&stash_ref)?;
        git_cmd(&repo_path)
            .args(["stash", "drop", &stash_ref])
            .run()
            .map_err(|e| format!("git stash drop failed: {e}"))?;
        Ok(())
    }
}

pub fn git_stash_show_blocking(path: String, stash_ref: String) -> Result<String, String> {
    {
        let repo_path = PathBuf::from(&path);
        validate_stash_ref(&stash_ref)?;
        let out = git_cmd(&repo_path)
            .args(["stash", "show", "-p", &stash_ref])
            .run()
            .map_err(|e| format!("git stash show failed: {e}"))?;
        Ok(out.stdout)
    }
}

pub fn get_file_history_blocking(
    path: String,
    file: String,
    count: Option<u32>,
    after: Option<String>,
) -> Result<Vec<CommitLogEntry>, String> {
    {
        let repo_path = PathBuf::from(&path);
        validate_paths_within_repo(&repo_path, std::slice::from_ref(&file))?;
        let n = count
            .unwrap_or(COMMIT_LOG_DEFAULT_COUNT)
            .min(COMMIT_LOG_MAX_COUNT);
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
            .split('\x1e')
            .filter(|s| !s.is_empty())
            .filter_map(|s| parse_commit_log_line(s.trim_matches('\n')))
            .collect();

        Ok(commits)
    }
}

pub fn get_file_blame_blocking(path: String, file: String) -> Result<Vec<BlameLine>, String> {
    {
        let repo_path = PathBuf::from(&path);
        validate_paths_within_repo(&repo_path, std::slice::from_ref(&file))?;
        // Routed through the GitReads port (Step 13 may flip to gix).
        git_reads().blame(&repo_path, &file)
    }
}
