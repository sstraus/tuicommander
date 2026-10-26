//! Git repository fixtures shared by this crate's tests and by the app crate's
//! tests of the `AppState`-coupled worktree and cache paths. Compiled for this
//! crate's own tests and, elsewhere, only behind the `test-support` feature,
//! which consumers enable from `[dev-dependencies]`.

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use crate::git_cli::git_cmd;
use crate::worktree::{WorktreeConfig, create_worktree_internal};

pub fn setup_test_repo() -> TempDir {
    setup_test_repo_at(TempDir::new().expect("Failed to create temp dir"))
}

pub fn setup_test_repo_at(temp_dir: TempDir) -> TempDir {
    let repo_path = temp_dir.path();

    git_cmd(repo_path)
        .args(["init"])
        .run()
        .expect("Failed to init git repo");
    git_cmd(repo_path)
        .args(["config", "user.email", "test@test.com"])
        .run()
        .expect("Failed to config git");
    git_cmd(repo_path)
        .args(["config", "user.name", "Test"])
        .run()
        .expect("Failed to config git");

    fs::write(repo_path.join("README.md"), "# Test").expect("Failed to write file");
    git_cmd(repo_path)
        .args(["add", "."])
        .run()
        .expect("Failed to git add");
    git_cmd(repo_path)
        .args(["commit", "-m", "Initial commit"])
        .run()
        .expect("Failed to git commit");

    temp_dir
}

/// Build a worktree on `branch` and return its path. Optionally commit a file
/// so the branch is genuinely ahead of the base branch.
pub fn worktree_with(repo: &Path, branch: &str, commit: bool) -> PathBuf {
    let repo_path = repo.to_string_lossy().to_string();
    let config = WorktreeConfig {
        task_name: branch.to_string(),
        base_repo: repo_path,
        branch: Some(branch.to_string()),
        create_branch: true,
    };
    let wt =
        create_worktree_internal(&repo.join("worktrees"), &config, None).expect("create worktree");
    if commit {
        fs::write(wt.path.join("work.txt"), "real work").expect("write");
        git_cmd(&wt.path).args(["add", "."]).run().expect("add");
        git_cmd(&wt.path)
            .args(["commit", "-m", "feat: real work"])
            .run()
            .expect("commit");
    }
    wt.path
}

/// The base branch of `setup_test_repo` — git's default name varies by version.
pub fn base_branch_of(repo: &Path) -> String {
    git_cmd(repo)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .run()
        .expect("rev-parse HEAD")
        .stdout
        .trim()
        .to_string()
}

/// A worktree on `branch` with an uncommitted file in it, plus the commit
/// count the branch is ahead of base. Returns the worktree path.
pub fn dirty_worktree_with(repo: &Path, branch: &str, commit: bool) -> PathBuf {
    let wt = worktree_with(repo, branch, commit);
    fs::write(wt.join("scratch.txt"), "hours of uncommitted work").expect("write scratch");
    wt
}

/// Helper: create a temp git repo with an initial commit.
pub fn setup_test_repo_with_commit() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().to_path_buf();
    std::process::Command::new("git")
        .current_dir(&path)
        .args(["init"])
        .output()
        .expect("git init");
    std::process::Command::new("git")
        .current_dir(&path)
        .args(["config", "user.email", "test@test.com"])
        .output()
        .expect("config email");
    std::process::Command::new("git")
        .current_dir(&path)
        .args(["config", "user.name", "Test"])
        .output()
        .expect("config name");
    // Create an initial file and commit
    std::fs::write(path.join("initial.txt"), "hello").expect("write initial");
    std::process::Command::new("git")
        .current_dir(&path)
        .args(["add", "initial.txt"])
        .output()
        .expect("add");
    std::process::Command::new("git")
        .current_dir(&path)
        .args(["commit", "-m", "initial"])
        .output()
        .expect("commit");
    (dir, path)
}
