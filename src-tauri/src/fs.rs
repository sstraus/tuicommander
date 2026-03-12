use serde::Serialize;
use std::path::PathBuf;

/// A directory entry returned by `list_directory`.
#[derive(Debug, Clone, Serialize)]
pub struct DirEntry {
    pub name: String,
    /// Path relative to repo root, always using `/` as separator.
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    /// Last modification time as seconds since UNIX epoch.
    pub modified_at: u64,
    /// Git status: "modified", "staged", "untracked", or "" (clean).
    pub git_status: String,
    /// Whether the file is listed in .gitignore.
    pub is_ignored: bool,
}

/// Validate that a resolved path is within the repo root.
/// Returns the canonical repo path and the canonical target path.
fn validate_path(repo_path: &str, relative: &str) -> Result<(PathBuf, PathBuf), String> {
    let repo = PathBuf::from(repo_path);
    let target = repo.join(relative);

    let canonical_repo = repo
        .canonicalize()
        .map_err(|e| format!("Failed to resolve repo path: {e}"))?;
    let canonical_target = target
        .canonicalize()
        .map_err(|e| format!("Failed to resolve path: {e}"))?;

    if !canonical_target.starts_with(&canonical_repo) {
        return Err("Access denied: path is outside repository".to_string());
    }

    Ok((canonical_repo, canonical_target))
}

/// Validate a path that may not exist yet (for write/create operations).
/// Canonicalizes the parent directory and checks it's within the repo.
fn validate_path_for_creation(repo_path: &str, relative: &str) -> Result<(PathBuf, PathBuf), String> {
    let repo = PathBuf::from(repo_path);
    let target = repo.join(relative);

    let canonical_repo = repo
        .canonicalize()
        .map_err(|e| format!("Failed to resolve repo path: {e}"))?;

    // For new files, canonicalize the parent directory
    let parent = target
        .parent()
        .ok_or_else(|| "Invalid path: no parent directory".to_string())?;

    let canonical_parent = parent
        .canonicalize()
        .map_err(|e| format!("Failed to resolve parent directory: {e}"))?;

    if !canonical_parent.starts_with(&canonical_repo) {
        return Err("Access denied: path is outside repository".to_string());
    }

    // Reconstruct full path using canonical parent + filename
    let file_name = target
        .file_name()
        .ok_or_else(|| "Invalid path: no file name".to_string())?;
    let canonical_target = canonical_parent.join(file_name);

    Ok((canonical_repo, canonical_target))
}

/// Parse `git status --porcelain -z` output into a map of relative_path -> status string.
pub(crate) fn parse_git_status(repo_path: &str, subdir: &str) -> std::collections::HashMap<String, String> {
    let mut statuses = std::collections::HashMap::new();

    let mut args = vec!["status", "--porcelain", "-z"];
    if !subdir.is_empty() && subdir != "." {
        args.push("--");
        args.push(subdir);
    }

    let out = match crate::git_cli::git_cmd(std::path::Path::new(repo_path))
        .args(&args)
        .run_silent()
    {
        Some(o) => o,
        None => return statuses,
    };

    let text = &out.stdout;
    // Porcelain -z format: entries separated by NUL, each entry is "XY path"
    // Renames have an additional NUL-separated original path after the entry.
    let entries: Vec<&str> = text.split('\0').collect();
    let mut i = 0;
    while i < entries.len() {
        let entry = entries[i];
        if entry.len() < 4 {
            i += 1;
            continue;
        }
        let xy = &entry[..2];
        let path = &entry[3..];

        let status = match xy {
            // Index has changes (staged)
            s if s.starts_with('A') => "staged",
            s if s.starts_with('M') || s.starts_with('R') || s.starts_with('D') => "staged",
            // Worktree has changes (modified)
            s if s.ends_with('M') || s.ends_with('D') => "modified",
            // Untracked
            "??" => "untracked",
            _ => "",
        };

        if !status.is_empty() {
            statuses.insert(path.to_string(), status.to_string());
        }

        // Renames (R) have an extra path entry
        if xy.starts_with('R') {
            i += 1; // skip the original path
        }

        i += 1;
    }

    statuses
}

/// Get a set of ignored paths within a directory using `git check-ignore`.
pub(crate) fn get_ignored_paths(repo_path: &str, paths: &[String]) -> std::collections::HashSet<String> {
    let mut ignored = std::collections::HashSet::new();
    if paths.is_empty() {
        return ignored;
    }

    let mut args: Vec<&str> = vec!["check-ignore", "--no-index", "--"];
    for p in paths {
        args.push(p);
    }

    // git check-ignore exits 0 = some ignored, 1 = none ignored
    if let Ok(raw) = crate::git_cli::git_cmd(std::path::Path::new(repo_path))
        .args(&args)
        .run_raw()
    {
        let text = String::from_utf8_lossy(&raw.stdout);
        for line in text.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                ignored.insert(trimmed.replace('\\', "/"));
            }
        }
    }

    ignored
}

/// List entries in a directory within a repository.
#[tauri::command]
pub fn list_directory(repo_path: String, subdir: String) -> Result<Vec<DirEntry>, String> {
    let repo = PathBuf::from(&repo_path);

    // Canonicalize repo root ONCE — all relative paths derived via join + strip_prefix
    let canonical_repo = repo
        .canonicalize()
        .map_err(|e| format!("Failed to resolve repo path: {e}"))?;

    // Validate the subdir is within the repo
    let dir_to_read = if subdir.is_empty() || subdir == "." {
        canonical_repo.clone()
    } else {
        let (_cr, canonical_dir) = validate_path(&repo_path, &subdir)?;
        canonical_dir
    };

    if !dir_to_read.is_dir() {
        return Err(format!("Not a directory: {subdir}"));
    }

    // Get git statuses for this subdir
    let git_statuses = parse_git_status(&repo_path, &subdir);

    // Build gitignore matcher from the repo's .gitignore (no subprocess)
    let gitignore_path = canonical_repo.join(".gitignore");
    let gitignore = if gitignore_path.exists() {
        let mut builder = ignore::gitignore::GitignoreBuilder::new(&canonical_repo);
        builder.add(&gitignore_path);
        builder.build().ok()
    } else {
        None
    };

    let mut entries = Vec::new();
    let read_dir = std::fs::read_dir(&dir_to_read)
        .map_err(|e| format!("Failed to read directory: {e}"))?;

    for entry in read_dir {
        let entry = entry.map_err(|e| format!("Failed to read entry: {e}"))?;
        let name = entry.file_name().to_string_lossy().to_string();

        // Skip .git directory
        if name == ".git" {
            continue;
        }

        let metadata = entry
            .metadata()
            .map_err(|e| format!("Failed to read metadata for {name}: {e}"))?;

        let is_dir = metadata.is_dir();
        let size = if is_dir { 0 } else { metadata.len() };
        let modified_at = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs());

        // Compute relative path via join + strip_prefix (no canonicalize per entry)
        let abs_path = dir_to_read.join(&name);
        let relative = abs_path
            .strip_prefix(&canonical_repo)
            .map_err(|_| format!("Entry {name} is outside repo"))?
            .to_string_lossy()
            .replace('\\', "/");

        // Check gitignore status without subprocess
        let is_ignored = gitignore.as_ref().map_or(false, |gi| {
            gi.matched_path_or_any_parents(&abs_path, is_dir).is_ignore()
        });

        // Look up git status — for dirs, propagate the most relevant child status
        let git_status = if is_dir {
            let prefix = format!("{relative}/");
            let mut has_staged = false;
            let mut has_modified = false;
            let mut has_untracked = false;
            for (p, s) in &git_statuses {
                if p.starts_with(&prefix) {
                    match s.as_str() {
                        "staged" => has_staged = true,
                        "modified" => has_modified = true,
                        "untracked" => has_untracked = true,
                        _ => {}
                    }
                }
            }
            if has_staged {
                "staged".to_string()
            } else if has_modified {
                "modified".to_string()
            } else if has_untracked {
                "untracked".to_string()
            } else {
                String::new()
            }
        } else {
            git_statuses.get(&relative).cloned().unwrap_or_default()
        };

        entries.push(DirEntry {
            name,
            path: relative,
            is_dir,
            size,
            modified_at,
            git_status,
            is_ignored,
        });
    }

    // Sort: directories first, then alphabetical (case-insensitive)
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(entries)
}

/// Recursively search files in a repository matching a glob-like query.
/// Returns up to `limit` results (default 200) to avoid blowing up on huge repos.
/// Respects .gitignore natively via the `ignore` crate (no subprocess).
#[tauri::command]
pub fn search_files(
    repo_path: String,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<DirEntry>, String> {
    let repo = PathBuf::from(&repo_path);
    let canonical_repo = repo
        .canonicalize()
        .map_err(|e| format!("Failed to resolve repo path: {e}"))?;

    let max_results = limit.unwrap_or(200);
    let pattern = build_search_pattern(&query);

    let mut results = Vec::new();

    // Walk using the `ignore` crate: respects .gitignore, .git/info/exclude,
    // global gitignore — skips ignored directories entirely during traversal.
    let walker = ignore::WalkBuilder::new(&canonical_repo)
        .hidden(false) // show dotfiles (except .git which is always skipped)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .build();

    for entry in walker {
        if results.len() >= max_results {
            break;
        }
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let is_file = entry.file_type().map_or(false, |ft| ft.is_file());
        if !is_file {
            continue;
        }

        let relative = match entry.path().strip_prefix(&canonical_repo) {
            Ok(p) => p.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };

        let name = entry.file_name().to_string_lossy().to_string();

        // Match against file name or relative path
        if !pattern.is_match(&name) && !pattern.is_match(&relative) {
            continue;
        }

        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        let modified_at = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs());

        results.push(DirEntry {
            name,
            path: relative,
            is_dir: false,
            size: metadata.len(),
            modified_at,
            git_status: String::new(), // populated below
            is_ignored: false, // walker already filtered gitignored entries
        });
    }

    // Get git statuses only for matched results (not the whole repo)
    if !results.is_empty() {
        let git_statuses = parse_git_status(&repo_path, ".");
        for entry in &mut results {
            if let Some(status) = git_statuses.get(&entry.path) {
                entry.git_status = status.clone();
            }
        }
    }

    // Sort by path for predictable results
    results.sort_by(|a, b| a.path.to_lowercase().cmp(&b.path.to_lowercase()));

    Ok(results)
}

/// Build a case-insensitive regex from a user search query.
/// Supports `*` (any within name) and `**` (any including path separators).
fn build_search_pattern(query: &str) -> regex::Regex {
    let mut regex_str = String::from("(?i)");
    let mut chars = query.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' if chars.peek() == Some(&'*') => {
                chars.next(); // consume second *
                regex_str.push_str(".*");
            }
            '*' => regex_str.push_str("[^/]*"),
            '?' => regex_str.push('.'),
            '.' | '(' | ')' | '[' | ']' | '{' | '}' | '+' | '^' | '$' | '|' | '\\' => {
                regex_str.push('\\');
                regex_str.push(c);
            }
            _ => regex_str.push(c),
        }
    }
    regex::Regex::new(&regex_str).unwrap_or_else(|_| {
        // Fallback: treat the whole query as a literal substring
        regex::Regex::new(&format!("(?i){}", regex::escape(query))).unwrap()
    })
}

/// Recursively walk a directory, collecting entries that match the pattern.
fn walk_directory(
    root: &PathBuf,
    dir: &PathBuf,
    pattern: &regex::Regex,
    git_statuses: &std::collections::HashMap<String, String>,
    results: &mut Vec<DirEntry>,
    max_results: usize,
) {
    if results.len() >= max_results {
        return;
    }

    let read_dir = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return,
    };

    for entry in read_dir {
        if results.len() >= max_results {
            return;
        }

        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let name = entry.file_name().to_string_lossy().to_string();

        // Skip hidden/special directories
        if name == ".git" || name == "node_modules" || name == ".DS_Store" {
            continue;
        }

        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        let canonical_entry = match entry.path().canonicalize() {
            Ok(p) => p,
            Err(_) => continue,
        };

        let relative = match canonical_entry.strip_prefix(root) {
            Ok(p) => p.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };

        let is_dir = metadata.is_dir();

        if is_dir {
            // Recurse into subdirectories
            walk_directory(root, &canonical_entry, pattern, git_statuses, results, max_results);
        } else {
            // Check if file name or path matches the search pattern
            if pattern.is_match(&name) || pattern.is_match(&relative) {
                let git_status = git_statuses.get(&relative).cloned().unwrap_or_default();
                let modified_at = metadata
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_secs());
                results.push(DirEntry {
                    name,
                    path: relative,
                    is_dir: false,
                    size: metadata.len(),
                    modified_at,
                    git_status,
                    is_ignored: false,
                });
            }
        }
    }
}

/// Read a file's content within a repository.
/// Re-uses the existing `read_file_impl` from lib.rs.
#[tauri::command]
pub fn fs_read_file(repo_path: String, file: String) -> Result<String, String> {
    crate::read_file_impl(repo_path, file)
}

/// Write content to a file within a repository.
#[tauri::command]
pub fn write_file(repo_path: String, file: String, content: String) -> Result<(), String> {
    let (_canonical_repo, canonical_target) = if PathBuf::from(&repo_path).join(&file).exists() {
        validate_path(&repo_path, &file)?
    } else {
        validate_path_for_creation(&repo_path, &file)?
    };

    std::fs::write(&canonical_target, &content)
        .map_err(|e| format!("Failed to write file: {e}"))
}

/// Create a directory (and parents) within a repository.
#[tauri::command]
pub fn create_directory(repo_path: String, dir: String) -> Result<(), String> {
    let repo = PathBuf::from(&repo_path);
    let target = repo.join(&dir);

    let canonical_repo = repo
        .canonicalize()
        .map_err(|e| format!("Failed to resolve repo path: {e}"))?;

    // For new directories we can't canonicalize the full path yet.
    // Walk up to find an existing ancestor and verify it's within the repo.
    let mut check = target.clone();
    loop {
        if check.exists() {
            let canonical_check = check
                .canonicalize()
                .map_err(|e| format!("Failed to resolve path: {e}"))?;
            if !canonical_check.starts_with(&canonical_repo) {
                return Err("Access denied: path is outside repository".to_string());
            }
            break;
        }
        if !check.pop() {
            return Err("Cannot resolve path".to_string());
        }
    }

    std::fs::create_dir_all(&target)
        .map_err(|e| format!("Failed to create directory: {e}"))
}

/// Delete a file or directory within a repository.
#[tauri::command]
pub fn delete_path(repo_path: String, path: String) -> Result<(), String> {
    let (_canonical_repo, canonical_target) = validate_path(&repo_path, &path)?;

    if canonical_target.is_dir() {
        std::fs::remove_dir_all(&canonical_target)
            .map_err(|e| format!("Failed to delete directory: {e}"))
    } else {
        std::fs::remove_file(&canonical_target)
            .map_err(|e| format!("Failed to delete file: {e}"))
    }
}

/// Rename/move a file or directory within a repository.
#[tauri::command]
pub fn rename_path(
    repo_path: String,
    from: String,
    to: String,
) -> Result<(), String> {
    let (_canonical_repo, canonical_from) = validate_path(&repo_path, &from)?;
    let (_, canonical_to) = if PathBuf::from(&repo_path).join(&to).exists() {
        validate_path(&repo_path, &to)?
    } else {
        validate_path_for_creation(&repo_path, &to)?
    };

    std::fs::rename(&canonical_from, &canonical_to)
        .map_err(|e| format!("Failed to rename: {e}"))
}

/// Copy a file within a repository.
#[tauri::command]
pub fn copy_path(
    repo_path: String,
    from: String,
    to: String,
) -> Result<(), String> {
    let (_canonical_repo, canonical_from) = validate_path(&repo_path, &from)?;
    let (_, canonical_to) = if PathBuf::from(&repo_path).join(&to).exists() {
        validate_path(&repo_path, &to)?
    } else {
        validate_path_for_creation(&repo_path, &to)?
    };

    if canonical_from.is_dir() {
        return Err("Cannot copy directories. Only files can be copied.".to_string());
    }

    std::fs::copy(&canonical_from, &canonical_to)
        .map_err(|e| format!("Failed to copy file: {e}"))?;

    Ok(())
}

/// Result of resolving a terminal path candidate.
#[derive(Debug, Clone, Serialize)]
pub struct ResolvedFilePath {
    pub absolute_path: String,
    pub is_directory: bool,
}

/// Strip trailing `:line` or `:line:col` suffix from a path candidate.
/// Returns the path portion only.
pub fn strip_line_col_suffix(candidate: &str) -> &str {
    // Match `:digits` or `:digits:digits` at the end
    let bytes = candidate.as_bytes();
    let mut end = bytes.len();

    // Try stripping `:col` (rightmost numeric segment)
    if let Some(colon_pos) = candidate[..end].rfind(':')
        && candidate[colon_pos + 1..end].chars().all(|c| c.is_ascii_digit())
        && colon_pos + 1 < end
    {
        end = colon_pos;

        // Try stripping `:line` (second rightmost numeric segment)
        if let Some(colon_pos2) = candidate[..end].rfind(':')
            && candidate[colon_pos2 + 1..end]
                .chars()
                .all(|c| c.is_ascii_digit())
            && colon_pos2 + 1 < end
        {
            end = colon_pos2;
        }
    }

    &candidate[..end]
}

/// macOS TCC-protected directory names under $HOME.
/// Probing these with `.exists()` or `.canonicalize()` triggers permission dialogs.
const TCC_PROTECTED_DIRS: &[&str] = &[
    "Desktop", "Documents", "Downloads", "Movies", "Music", "Pictures",
    "Library", "Photos Library.photoslibrary",
];

/// Returns true if `path` falls under a macOS TCC-protected directory.
fn is_tcc_protected_path(path: &std::path::Path) -> bool {
    let Some(home) = dirs::home_dir() else { return false };
    if !path.starts_with(&home) {
        return false;
    }
    if let Ok(rel) = path.strip_prefix(&home)
        && let Some(first) = rel.components().next()
    {
        let name = first.as_os_str().to_string_lossy();
        return TCC_PROTECTED_DIRS.iter().any(|d| d.eq_ignore_ascii_case(&name));
    }
    false
}

/// Validate a path candidate from terminal output against the filesystem.
/// Strips `:line:col` suffixes, resolves relative paths against `cwd`,
/// and checks existence.
///
/// SAFETY: Refuses to probe macOS TCC-protected directories to avoid
/// triggering system permission dialogs.
#[tauri::command]
pub fn resolve_terminal_path(cwd: String, candidate: String) -> Option<ResolvedFilePath> {
    let path_str = strip_line_col_suffix(&candidate);

    // Expand ~ to home directory
    let expanded = if let Some(rest) = path_str.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            home.join(rest).to_string_lossy().to_string()
        } else {
            path_str.to_string()
        }
    } else {
        path_str.to_string()
    };
    let path = PathBuf::from(&expanded);

    let absolute = if path.is_absolute() {
        path
    } else {
        PathBuf::from(&cwd).join(&path)
    };

    // Never probe TCC-protected directories
    if is_tcc_protected_path(&absolute) {
        return None;
    }

    // Canonicalize to resolve symlinks and verify existence
    match absolute.canonicalize() {
        Ok(canonical) => Some(ResolvedFilePath {
            absolute_path: canonical.to_string_lossy().to_string(),
            is_directory: canonical.is_dir(),
        }),
        Err(_) => None,
    }
}

/// Append a path pattern to the repo's .gitignore file.
#[tauri::command]
pub fn add_to_gitignore(repo_path: String, pattern: String) -> Result<(), String> {
    let repo = PathBuf::from(&repo_path);
    let canonical_repo = repo
        .canonicalize()
        .map_err(|e| format!("Failed to resolve repo path: {e}"))?;

    let gitignore = canonical_repo.join(".gitignore");
    let mut content = if gitignore.exists() {
        std::fs::read_to_string(&gitignore)
            .map_err(|e| format!("Failed to read .gitignore: {e}"))?
    } else {
        String::new()
    };

    // Check if pattern already exists
    if content.lines().any(|line| line.trim() == pattern.trim()) {
        return Ok(()); // Already ignored
    }

    // Ensure trailing newline before appending
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(pattern.trim());
    content.push('\n');

    std::fs::write(&gitignore, &content)
        .map_err(|e| format!("Failed to write .gitignore: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_test_repo() -> TempDir {
        let dir = TempDir::new().unwrap();
        let repo_path = dir.path();

        // Initialize a git repo
        crate::git_cli::git_cmd(repo_path).args(["init"]).run().unwrap();
        crate::git_cli::git_cmd(repo_path).args(["config", "user.email", "test@test.com"]).run().unwrap();
        crate::git_cli::git_cmd(repo_path).args(["config", "user.name", "Test"]).run().unwrap();

        // Create some files and directories
        fs::write(repo_path.join("README.md"), "# Test").unwrap();
        fs::write(repo_path.join("main.rs"), "fn main() {}").unwrap();
        fs::create_dir(repo_path.join("src")).unwrap();
        fs::write(repo_path.join("src/lib.rs"), "pub fn hello() {}").unwrap();

        // Commit everything
        crate::git_cli::git_cmd(repo_path).args(["add", "-A"]).run().unwrap();
        crate::git_cli::git_cmd(repo_path).args(["commit", "-m", "init"]).run().unwrap();

        dir
    }

    #[test]
    fn test_list_directory_root() {
        let dir = setup_test_repo();
        let repo_path = dir.path().to_string_lossy().to_string();

        let entries = list_directory(repo_path, ".".to_string()).unwrap();

        // Should have: src/ dir, README.md, main.rs (no .git)
        assert!(entries.len() >= 3);

        // Directories should come first
        let first_dir_idx = entries.iter().position(|e| e.is_dir);
        let first_file_idx = entries.iter().position(|e| !e.is_dir);
        if let (Some(di), Some(fi)) = (first_dir_idx, first_file_idx) {
            assert!(di < fi, "Directories should sort before files");
        }

        // .git should not be listed
        assert!(entries.iter().all(|e| e.name != ".git"));

        // src directory should exist
        assert!(entries.iter().any(|e| e.name == "src" && e.is_dir));
    }

    #[test]
    fn test_list_directory_subdir() {
        let dir = setup_test_repo();
        let repo_path = dir.path().to_string_lossy().to_string();

        let entries = list_directory(repo_path, "src".to_string()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "lib.rs");
        assert!(!entries[0].is_dir);
        assert_eq!(entries[0].path, "src/lib.rs");
    }

    #[test]
    fn test_list_directory_path_traversal_rejected() {
        let dir = setup_test_repo();
        let repo_path = dir.path().to_string_lossy().to_string();

        let result = list_directory(repo_path, "../".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn test_list_directory_git_status() {
        let dir = setup_test_repo();
        let repo_path = dir.path().to_string_lossy().to_string();

        // Modify a tracked file
        fs::write(dir.path().join("README.md"), "# Modified").unwrap();

        // Add an untracked file
        fs::write(dir.path().join("new_file.txt"), "new").unwrap();

        let entries = list_directory(repo_path, ".".to_string()).unwrap();

        let readme = entries.iter().find(|e| e.name == "README.md").unwrap();
        assert_eq!(readme.git_status, "modified");

        let new_file = entries.iter().find(|e| e.name == "new_file.txt").unwrap();
        assert_eq!(new_file.git_status, "untracked");
    }

    #[test]
    fn test_write_file_creates_and_overwrites() {
        let dir = setup_test_repo();
        let repo_path = dir.path().to_string_lossy().to_string();

        // Write a new file
        write_file(repo_path.clone(), "new.txt".to_string(), "hello".to_string()).unwrap();
        assert_eq!(fs::read_to_string(dir.path().join("new.txt")).unwrap(), "hello");

        // Overwrite
        write_file(repo_path, "new.txt".to_string(), "world".to_string()).unwrap();
        assert_eq!(fs::read_to_string(dir.path().join("new.txt")).unwrap(), "world");
    }

    #[test]
    fn test_write_file_path_traversal_rejected() {
        let dir = setup_test_repo();
        let repo_path = dir.path().to_string_lossy().to_string();

        let result = write_file(repo_path, "../escape.txt".to_string(), "bad".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn test_create_directory() {
        let dir = setup_test_repo();
        let repo_path = dir.path().to_string_lossy().to_string();

        create_directory(repo_path.clone(), "nested/deep/dir".to_string()).unwrap();
        assert!(dir.path().join("nested/deep/dir").is_dir());
    }

    #[test]
    fn test_delete_path_file() {
        let dir = setup_test_repo();
        let repo_path = dir.path().to_string_lossy().to_string();

        assert!(dir.path().join("README.md").exists());
        delete_path(repo_path, "README.md".to_string()).unwrap();
        assert!(!dir.path().join("README.md").exists());
    }

    #[test]
    fn test_delete_path_directory() {
        let dir = setup_test_repo();
        let repo_path = dir.path().to_string_lossy().to_string();

        assert!(dir.path().join("src").exists());
        delete_path(repo_path, "src".to_string()).unwrap();
        assert!(!dir.path().join("src").exists());
    }

    #[test]
    fn test_rename_path() {
        let dir = setup_test_repo();
        let repo_path = dir.path().to_string_lossy().to_string();

        rename_path(
            repo_path,
            "main.rs".to_string(),
            "app.rs".to_string(),
        )
        .unwrap();

        assert!(!dir.path().join("main.rs").exists());
        assert!(dir.path().join("app.rs").exists());
    }

    #[test]
    fn test_rename_path_traversal_rejected() {
        let dir = setup_test_repo();
        let repo_path = dir.path().to_string_lossy().to_string();

        let result = rename_path(
            repo_path,
            "main.rs".to_string(),
            "../escaped.rs".to_string(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_paths_use_forward_slashes() {
        let dir = setup_test_repo();
        let repo_path = dir.path().to_string_lossy().to_string();

        let entries = list_directory(repo_path.clone(), "src".to_string()).unwrap();
        for entry in &entries {
            assert!(!entry.path.contains('\\'), "Path should use / not \\: {}", entry.path);
        }

        let root_entries = list_directory(repo_path, ".".to_string()).unwrap();
        for entry in &root_entries {
            assert!(!entry.path.contains('\\'), "Path should use / not \\: {}", entry.path);
        }
    }

    #[test]
    fn test_list_directory_modified_at_populated() {
        let dir = setup_test_repo();
        let repo_path = dir.path().to_string_lossy().to_string();

        let entries = list_directory(repo_path, ".".to_string()).unwrap();

        for entry in &entries {
            assert!(entry.modified_at > 0, "modified_at should be non-zero for {}", entry.name);
        }
    }

    #[test]
    fn test_list_directory_marks_ignored() {
        let dir = setup_test_repo();
        let repo_path = dir.path().to_string_lossy().to_string();

        // Create a file and a gitignore that ignores it
        fs::write(dir.path().join("build.log"), "build output").unwrap();
        fs::write(dir.path().join(".gitignore"), "build.log\n").unwrap();

        let entries = list_directory(repo_path, ".".to_string()).unwrap();

        let build_log = entries.iter().find(|e| e.name == "build.log");
        assert!(build_log.is_some(), "build.log should still appear in listing");
        assert!(
            build_log.unwrap().is_ignored,
            "build.log should be marked as ignored"
        );

        // .gitignore itself should NOT be ignored
        let gitignore = entries.iter().find(|e| e.name == ".gitignore");
        assert!(gitignore.is_some(), ".gitignore should appear in listing");
        assert!(
            !gitignore.unwrap().is_ignored,
            ".gitignore should NOT be marked as ignored"
        );
    }

    // --- search_files tests ---

    #[test]
    fn test_search_files_basic() {
        let dir = setup_test_repo();
        let repo_path = dir.path().to_string_lossy().to_string();

        let results = search_files(repo_path, "lib".to_string(), None).unwrap();
        assert!(
            results.iter().any(|e| e.name == "lib.rs"),
            "Should find lib.rs matching 'lib', got: {:?}",
            results.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
        // All results should have forward-slash paths
        for entry in &results {
            assert!(!entry.path.contains('\\'), "Path should use / not \\: {}", entry.path);
        }
    }

    #[test]
    fn test_search_files_respects_gitignore() {
        let dir = setup_test_repo();
        let repo_path = dir.path().to_string_lossy().to_string();

        // Create an ignored directory with files
        fs::create_dir(dir.path().join("build_output")).unwrap();
        fs::write(dir.path().join("build_output/artifact.rs"), "// build").unwrap();
        fs::write(dir.path().join(".gitignore"), "build_output/\n").unwrap();

        let results = search_files(repo_path, "artifact".to_string(), None).unwrap();
        assert!(
            results.iter().all(|e| !e.path.contains("build_output")),
            "Should NOT find files inside gitignored directory, got: {:?}",
            results.iter().map(|e| &e.path).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_search_files_limit() {
        let dir = setup_test_repo();
        let repo_path = dir.path().to_string_lossy().to_string();

        // Create many files
        fs::create_dir(dir.path().join("many")).unwrap();
        for i in 0..20 {
            fs::write(dir.path().join(format!("many/file_{i}.txt")), "content").unwrap();
        }

        let results = search_files(repo_path, "file_".to_string(), Some(5)).unwrap();
        assert!(
            results.len() <= 5,
            "Should respect limit of 5, got {}",
            results.len()
        );
    }

    // --- strip_line_col_suffix tests ---

    #[test]
    fn test_strip_no_suffix() {
        assert_eq!(strip_line_col_suffix("src/lib.rs"), "src/lib.rs");
        assert_eq!(strip_line_col_suffix("/usr/bin/test"), "/usr/bin/test");
    }

    #[test]
    fn test_strip_line_only() {
        assert_eq!(strip_line_col_suffix("src/lib.rs:42"), "src/lib.rs");
    }

    #[test]
    fn test_strip_line_and_col() {
        assert_eq!(strip_line_col_suffix("src/lib.rs:42:10"), "src/lib.rs");
    }

    #[test]
    fn test_strip_preserves_non_numeric_colons() {
        // Windows-style C: drive prefix should be preserved
        assert_eq!(strip_line_col_suffix("C:\\Users\\file.rs"), "C:\\Users\\file.rs");
        // Colon followed by non-digits should be preserved
        assert_eq!(strip_line_col_suffix("src/lib.rs:abc"), "src/lib.rs:abc");
    }

    #[test]
    fn test_strip_empty_after_colon() {
        assert_eq!(strip_line_col_suffix("src/lib.rs:"), "src/lib.rs:");
    }

    // --- resolve_terminal_path tests ---

    #[test]
    fn test_resolve_absolute_existing_file() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("hello.rs");
        fs::write(&file, "fn main() {}").unwrap();

        let result = resolve_terminal_path(
            dir.path().to_string_lossy().to_string(),
            file.to_string_lossy().to_string(),
        );
        assert!(result.is_some());
        let resolved = result.unwrap();
        assert!(!resolved.is_directory);
        assert!(resolved.absolute_path.ends_with("hello.rs"));
    }

    #[test]
    fn test_resolve_relative_existing_file() {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/lib.rs"), "pub fn hello() {}").unwrap();

        let result = resolve_terminal_path(
            dir.path().to_string_lossy().to_string(),
            "src/lib.rs".to_string(),
        );
        assert!(result.is_some());
        let resolved = result.unwrap();
        assert!(!resolved.is_directory);
        assert!(resolved.absolute_path.ends_with("src/lib.rs"));
    }

    #[test]
    fn test_resolve_with_line_suffix() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

        let result = resolve_terminal_path(
            dir.path().to_string_lossy().to_string(),
            "main.rs:42".to_string(),
        );
        assert!(result.is_some());
        assert!(result.unwrap().absolute_path.ends_with("main.rs"));
    }

    #[test]
    fn test_resolve_with_line_col_suffix() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

        let result = resolve_terminal_path(
            dir.path().to_string_lossy().to_string(),
            "main.rs:42:10".to_string(),
        );
        assert!(result.is_some());
        assert!(result.unwrap().absolute_path.ends_with("main.rs"));
    }

    #[test]
    fn test_resolve_nonexistent_returns_none() {
        let dir = TempDir::new().unwrap();
        let result = resolve_terminal_path(
            dir.path().to_string_lossy().to_string(),
            "does_not_exist.rs".to_string(),
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_resolve_directory() {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("src")).unwrap();

        let result = resolve_terminal_path(
            dir.path().to_string_lossy().to_string(),
            "src".to_string(),
        );
        assert!(result.is_some());
        assert!(result.unwrap().is_directory);
    }
}
