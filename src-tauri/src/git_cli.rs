//! Unified git subprocess helper.
//!
//! Every git CLI invocation in the app should go through this module.
//! It wraps `Command::new(resolve_cli("git"))`, captures output, and
//! returns typed results with consistent error handling.

use std::ffi::OsStr;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cli::{enriched_path, resolve_cli};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Error from a git subprocess call.
#[derive(Debug)]
pub(crate) enum GitError {
    /// The git process could not be spawned (missing binary, permission error).
    SpawnFailed(std::io::Error),
    /// Git exited with a non-zero status code.
    NonZeroExit { code: Option<i32>, stderr: String },
}

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SpawnFailed(e) => write!(f, "Failed to spawn git: {e}"),
            Self::NonZeroExit { code, stderr } => {
                let code_str = code
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "signal".to_string());
                if stderr.is_empty() {
                    write!(f, "git exited with code {code_str}")
                } else {
                    write!(f, "git exited with code {code_str}: {stderr}")
                }
            }
        }
    }
}

impl From<GitError> for String {
    fn from(e: GitError) -> String {
        e.to_string()
    }
}

// ---------------------------------------------------------------------------
// Output type
// ---------------------------------------------------------------------------

/// Successful output from a git subprocess.
#[derive(Debug)]
pub(crate) struct GitOutput {
    pub stdout: String,
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

/// Builder for configuring and running a git subprocess.
///
/// # Examples
/// ```ignore
/// let out = git_cmd(repo_path)
///     .args(&["log", "--oneline", "-5"])
///     .run()?;
/// ```
pub(crate) struct GitCmd {
    cmd: Command,
    cwd: PathBuf,
}

impl GitCmd {
    /// Add multiple arguments.
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.cmd.args(args);
        self
    }

    /// Set an environment variable for the subprocess.
    pub fn env(mut self, key: &str, val: &str) -> Self {
        self.cmd.env(key, val);
        self
    }

    /// Run the git command, requiring success (non-zero exit → `Err`).
    ///
    /// Returns `GitOutput` containing raw (untrimmed) stdout on success.
    pub fn run(mut self) -> Result<GitOutput, GitError> {
        let output = self.cmd.output().map_err(GitError::SpawnFailed)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(GitError::NonZeroExit {
                code: output.status.code(),
                stderr,
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(GitOutput { stdout })
    }

    /// Run the git command, returning `None` on non-zero exit.
    /// Spawn failures are logged to stderr (they indicate a broken git
    /// installation, not normal git behavior).
    pub fn run_silent(self) -> Option<GitOutput> {
        let cwd = self.cwd.clone();
        match self.run() {
            Ok(o) => Some(o),
            Err(GitError::SpawnFailed(e)) => {
                // Use warn for "No such file or directory" — stale worktree entries are expected.
                // Reserve error for unexpected spawn failures (broken git installation).
                if e.kind() == std::io::ErrorKind::NotFound {
                    tracing::warn!(
                        source = "git_cli",
                        "Spawn failed (dir missing): {}",
                        cwd.display()
                    );
                } else {
                    tracing::error!(source = "git_cli", "Spawn failed in {}: {e}", cwd.display());
                }
                None
            }
            Err(GitError::NonZeroExit { .. }) => None,
        }
    }

    /// Run the git command, returning the full `Output` struct regardless
    /// of exit code. Use for callsites that need to inspect exit code and
    /// stderr independently (e.g. `run_git_command` which never returns Err).
    pub fn run_raw(mut self) -> Result<std::process::Output, GitError> {
        self.cmd.output().map_err(GitError::SpawnFailed)
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Remove a stale `.git/index.lock` left behind by a crashed process so the
/// next git invocation isn't blocked with `Unable to create '.git/index.lock':
/// File exists` / `could not write index`.
///
/// Staleness is age-based, with two thresholds because the two crash modes
/// produce locks of different sizes and we want a wide margin over any live
/// git process:
///
/// - **Empty lock** (0 bytes): git created the lock but crashed before writing
///   the new index. A real in-progress write fills the lock almost immediately,
///   so a 0-byte lock older than [`EMPTY_LOCK_STALE_SECS`] is certainly orphaned.
/// - **Non-empty lock**: git wrote the new index into the lock but died before
///   renaming it over `.git/index` (e.g. Claude Code killed mid-`git stash`).
///   A legitimate index write finishes in well under a second; we wait
///   [`NONEMPTY_LOCK_STALE_SECS`] to stay safely clear of even large
///   `stash`/`add` operations before reclaiming.
///
/// A 0-byte lock is reclaimed after this many seconds (early crash, no index written yet).
const EMPTY_LOCK_STALE_SECS: u64 = 5;
/// A non-empty lock (index written, rename never happened) is reclaimed after this
/// many seconds — wide margin over even large `stash`/`add` index writes.
const NONEMPTY_LOCK_STALE_SECS: u64 = 30;

/// Pure staleness rule for an `index.lock` of the given byte size and age.
/// Split out from [`remove_stale_index_lock`] so the thresholds are unit-testable
/// without touching the filesystem clock.
fn is_index_lock_stale(len: u64, age_secs: u64) -> bool {
    let threshold = if len == 0 {
        EMPTY_LOCK_STALE_SECS
    } else {
        NONEMPTY_LOCK_STALE_SECS
    };
    age_secs >= threshold
}

fn remove_stale_index_lock(cwd: &Path) {
    let lock = cwd.join(".git/index.lock");
    let Ok(meta) = std::fs::metadata(&lock) else {
        return;
    };

    // Without a reliable age we can't tell a stale lock from a live one — leave it.
    let Some(age_secs) = meta
        .modified()
        .ok()
        .and_then(|t| t.elapsed().ok())
        .map(|d| d.as_secs())
    else {
        return;
    };

    if !is_index_lock_stale(meta.len(), age_secs) {
        return;
    }

    match std::fs::remove_file(&lock) {
        Ok(()) => {
            tracing::info!(
                source = "git_cli",
                "Removed stale index.lock ({} bytes, {age_secs}s old) in {}",
                meta.len(),
                cwd.display()
            );
        }
        Err(e) => {
            tracing::warn!(
                source = "git_cli",
                "Failed to remove stale index.lock in {}: {e}",
                cwd.display()
            );
        }
    }
}

/// Create a git command builder rooted at the given directory.
pub(crate) fn git_cmd(cwd: &Path) -> GitCmd {
    remove_stale_index_lock(cwd);
    let mut cmd = Command::new(resolve_cli("git"));
    cmd.current_dir(cwd);
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    cmd.env("PATH", enriched_path());
    cmd.arg("--no-optional-locks");
    crate::cli::apply_no_window(&mut cmd);
    GitCmd {
        cmd,
        cwd: cwd.to_path_buf(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Helper: create a temp dir with `git init`.
    fn setup_test_repo() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().to_path_buf();
        Command::new("git")
            .current_dir(&path)
            .args(["init"])
            .output()
            .expect("git init");
        Command::new("git")
            .current_dir(&path)
            .args(["config", "user.email", "test@test.com"])
            .output()
            .expect("git config email");
        Command::new("git")
            .current_dir(&path)
            .args(["config", "user.name", "Test"])
            .output()
            .expect("git config name");
        (dir, path)
    }

    #[test]
    fn test_empty_lock_kept_while_fresh_removed_when_old() {
        // 0-byte lock: kept under 5s, reclaimed at/after 5s.
        assert!(!is_index_lock_stale(0, 0));
        assert!(!is_index_lock_stale(0, 4));
        assert!(is_index_lock_stale(0, 5));
        assert!(is_index_lock_stale(0, 60));
    }

    #[test]
    fn test_nonempty_lock_kept_until_30s() {
        // Non-empty lock (index written, rename never happened): kept under 30s
        // so we never nuke a live large stash/add, reclaimed at/after 30s.
        assert!(!is_index_lock_stale(4096, 0));
        assert!(!is_index_lock_stale(4096, 29));
        assert!(is_index_lock_stale(4096, 30));
        assert!(is_index_lock_stale(4096, 120));
    }

    #[test]
    fn test_run_success() {
        let (_dir, path) = setup_test_repo();
        let out = git_cmd(&path).args(["status", "--porcelain"]).run();
        assert!(out.is_ok());
    }

    #[test]
    fn test_run_non_zero_exit() {
        let (_dir, path) = setup_test_repo();
        // Asking for log in a repo with no commits → non-zero exit
        let result = git_cmd(&path).args(["log", "--oneline"]).run();
        assert!(result.is_err());
        let err = result.unwrap_err();
        match &err {
            GitError::NonZeroExit { code, stderr: _ } => {
                assert!(code.is_some());
            }
            _ => panic!("Expected NonZeroExit, got {err:?}"),
        }
        // Display impl should produce a readable message
        let msg = err.to_string();
        assert!(msg.contains("git exited with code"));
    }

    #[test]
    fn test_run_spawn_failed() {
        let (_dir, path) = setup_test_repo();
        // Use a non-existent binary to trigger spawn failure
        let mut cmd = Command::new("/nonexistent/git-binary-that-does-not-exist");
        cmd.current_dir(&path);
        cmd.args(["status"]);
        let gc = GitCmd {
            cmd,
            cwd: path.clone(),
        };
        let result = gc.run();
        assert!(result.is_err());
        match result.unwrap_err() {
            GitError::SpawnFailed(_) => {} // expected
            other => panic!("Expected SpawnFailed, got {other:?}"),
        }
    }

    #[test]
    fn test_run_silent_returns_none_on_error() {
        let (_dir, path) = setup_test_repo();
        // log in empty repo → non-zero → None
        let result = git_cmd(&path).args(["log", "--oneline"]).run_silent();
        assert!(result.is_none());
    }

    #[test]
    fn test_run_silent_returns_some_on_success() {
        let (_dir, path) = setup_test_repo();
        let result = git_cmd(&path).args(["status", "--porcelain"]).run_silent();
        assert!(result.is_some());
    }

    #[test]
    fn test_run_raw_returns_output_on_failure() {
        let (_dir, path) = setup_test_repo();
        // log in empty repo → non-zero but raw still returns Ok(Output)
        let result = git_cmd(&path).args(["log", "--oneline"]).run_raw();
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(!output.status.success());
    }

    #[test]
    fn test_git_error_display() {
        let err = GitError::NonZeroExit {
            code: Some(128),
            stderr: "fatal: not a git repository".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "git exited with code 128: fatal: not a git repository"
        );

        let err_empty = GitError::NonZeroExit {
            code: Some(1),
            stderr: String::new(),
        };
        assert_eq!(err_empty.to_string(), "git exited with code 1");

        let err_signal = GitError::NonZeroExit {
            code: None,
            stderr: "killed".to_string(),
        };
        assert_eq!(
            err_signal.to_string(),
            "git exited with code signal: killed"
        );
    }

    #[test]
    fn test_git_error_into_string() {
        let err = GitError::NonZeroExit {
            code: Some(1),
            stderr: "oops".to_string(),
        };
        let s: String = err.into();
        assert!(s.contains("oops"));
    }

    #[test]
    fn test_env_is_passed() {
        let (_dir, path) = setup_test_repo();
        // GIT_AUTHOR_NAME env var should be visible in the subprocess
        let out = git_cmd(&path)
            .env("GIT_AUTHOR_NAME", "TestBot")
            .args(["status", "--porcelain"])
            .run();
        assert!(out.is_ok());
    }
}
