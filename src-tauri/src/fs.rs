use serde::Serialize;
use std::path::PathBuf;
use std::process::Command;

/// A directory entry returned by `list_directory`.
#[derive(Debug, Clone, Serialize)]
pub struct DirEntry {
    pub name: String,
    /// Path relative to repo root, always using `/` as separator.
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
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
fn parse_git_status(repo_path: &str, subdir: &str) -> std::collections::HashMap<String, String> {
    let mut statuses = std::collections::HashMap::new();

    let git = crate::cli::resolve_cli("git");
    let mut args = vec!["status", "--porcelain", "-z"];
    if !subdir.is_empty() && subdir != "." {
        args.push("--");
        args.push(subdir);
    }

    let output = Command::new(git)
        .current_dir(repo_path)
        .args(&args)
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return statuses,
    };

    let text = String::from_utf8_lossy(&output.stdout);
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
fn get_ignored_paths(repo_path: &str, paths: &[String]) -> std::collections::HashSet<String> {
    let mut ignored = std::collections::HashSet::new();
    if paths.is_empty() {
        return ignored;
    }

    let git = crate::cli::resolve_cli("git");
    let mut cmd = Command::new(git);
    cmd.current_dir(repo_path).arg("check-ignore");
    for p in paths {
        cmd.arg(p);
    }

    if let Ok(output) = cmd.output() {
        // git check-ignore outputs one ignored path per line (exit code 0 = some ignored, 1 = none)
        let text = String::from_utf8_lossy(&output.stdout);
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

    // Validate the subdir is within the repo
    let dir_to_read = if subdir.is_empty() || subdir == "." {
        let canonical = repo
            .canonicalize()
            .map_err(|e| format!("Failed to resolve repo path: {e}"))?;
        canonical
    } else {
        let (_canonical_repo, canonical_dir) = validate_path(&repo_path, &subdir)?;
        canonical_dir
    };

    if !dir_to_read.is_dir() {
        return Err(format!("Not a directory: {subdir}"));
    }

    // Get git statuses for this subdir
    let git_statuses = parse_git_status(&repo_path, &subdir);

    let canonical_repo = repo
        .canonicalize()
        .map_err(|e| format!("Failed to resolve repo path: {e}"))?;

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

        // Compute relative path from repo root
        let canonical_entry = entry
            .path()
            .canonicalize()
            .map_err(|e| format!("Failed to canonicalize {name}: {e}"))?;
        let relative = canonical_entry
            .strip_prefix(&canonical_repo)
            .map_err(|_| format!("Entry {name} is outside repo"))?
            .to_string_lossy()
            .replace('\\', "/");

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
            // Priority: staged > modified > untracked
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
            git_status,
            is_ignored: false, // populated after collecting all entries
        });
    }

    // Detect gitignored paths
    let all_relative_paths: Vec<String> = entries.iter().map(|e| e.path.clone()).collect();
    let ignored_set = get_ignored_paths(&repo_path, &all_relative_paths);
    for entry in &mut entries {
        entry.is_ignored = ignored_set.contains(&entry.path);
    }

    // Sort: directories first, then alphabetical (case-insensitive)
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(entries)
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

/// Delete a file within a repository. Does NOT delete directories (safety).
#[tauri::command]
pub fn delete_path(repo_path: String, path: String) -> Result<(), String> {
    let (_canonical_repo, canonical_target) = validate_path(&repo_path, &path)?;

    if canonical_target.is_dir() {
        return Err("Cannot delete directories. Only files can be deleted.".to_string());
    }

    std::fs::remove_file(&canonical_target)
        .map_err(|e| format!("Failed to delete file: {e}"))
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
        Command::new("git")
            .current_dir(repo_path)
            .args(["init"])
            .output()
            .unwrap();
        Command::new("git")
            .current_dir(repo_path)
            .args(["config", "user.email", "test@test.com"])
            .output()
            .unwrap();
        Command::new("git")
            .current_dir(repo_path)
            .args(["config", "user.name", "Test"])
            .output()
            .unwrap();

        // Create some files and directories
        fs::write(repo_path.join("README.md"), "# Test").unwrap();
        fs::write(repo_path.join("main.rs"), "fn main() {}").unwrap();
        fs::create_dir(repo_path.join("src")).unwrap();
        fs::write(repo_path.join("src/lib.rs"), "pub fn hello() {}").unwrap();

        // Commit everything
        Command::new("git")
            .current_dir(repo_path)
            .args(["add", "-A"])
            .output()
            .unwrap();
        Command::new("git")
            .current_dir(repo_path)
            .args(["commit", "-m", "init"])
            .output()
            .unwrap();

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
    fn test_delete_path_rejects_directory() {
        let dir = setup_test_repo();
        let repo_path = dir.path().to_string_lossy().to_string();

        let result = delete_path(repo_path, "src".to_string());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Cannot delete directories"));
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
}
