# Git Operations

**Module:** `src-tauri/src/git.rs`

All git operations are performed by shelling out to the `git` CLI. This module handles repository info, diffs, branches, and utility functions.

## Tauri Commands

### Repository Info

| Command | Signature | Description |
|---------|-----------|-------------|
| `get_repo_info` | `(path: String) -> RepoInfo` | Get repo name, branch, status, initials |
| `get_git_branches` | `(path: String) -> Vec<Value>` | List all branches (sorted by rules below) |
| `check_is_main_branch` | `(branch: String) -> bool` | Check if branch is main/master/develop/trunk |
| `get_initials` | `(name: String) -> String` | Generate 2-char initials from repo name |

### Diff Operations

| Command | Signature | Description |
|---------|-----------|-------------|
| `get_git_diff` | `(path: String) -> String` | Full git diff (staged + unstaged) |
| `get_diff_stats` | `(path: String) -> DiffStats` | Addition/deletion counts |
| `get_changed_files` | `(path: String) -> Vec<ChangedFile>` | List changed files with per-file stats |
| `get_file_diff` | `(path: String, file: String) -> String` | Diff for a single file |

### Branch Operations

| Command | Signature | Description |
|---------|-----------|-------------|
| `rename_branch` | `(path, old_name, new_name) -> ()` | Rename a branch |

## Data Types

### RepoInfo

```rust
struct RepoInfo {
    path: String,        // Repository path
    name: String,        // Repository name (from directory)
    initials: String,    // 2-char initials (e.g., "TC" for tui-commander)
    branch: String,      // Current branch name
    status: String,      // "clean", "dirty", or "conflict"
    is_git_repo: bool,   // Whether path is a git repository
}
```

### DiffStats

```rust
struct DiffStats {
    additions: i32,
    deletions: i32,
}
```

### ChangedFile

```rust
struct ChangedFile {
    path: String,       // Relative file path
    status: String,     // "M" (modified), "A" (added), "D" (deleted), etc.
    additions: u32,     // Lines added
    deletions: u32,     // Lines deleted
}
```

## Utility Functions

### `get_repo_initials(name: &str) -> String`

Generates 2-character initials from a repository name:
- Split on hyphens, underscores, dots, spaces
- If multiple words: first letter of first two words (e.g., "tui-commander" → "TC")
- If single word: first two letters (e.g., "react" → "RE")
- Always uppercase

### `is_main_branch(branch_name: &str) -> bool`

Returns `true` for: `main`, `master`, `develop`, `trunk`, `dev`.

### `sort_branches(branches: &mut [Value])`

Sorts branches by priority:
1. Currently active branch (always first)
2. Main branches (main, master, develop)
3. Open PR branches (alphabetical)
4. Feature branches without PRs (alphabetical)
5. Merged/closed PR branches (alphabetical, always last)
