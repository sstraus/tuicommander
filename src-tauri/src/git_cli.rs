//! Unified git subprocess helper.
//!
//! Every git CLI invocation in the app should go through this module.
//! It wraps `Command::new(resolve_cli("git"))`, captures output, and
//! returns typed results with consistent error handling.

use std::fmt;
use std::path::Path;
use std::process::Command;

use crate::cli::resolve_cli;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Error from a git subprocess call.
#[derive(Debug)]
pub(crate) enum GitError {
    /// The git process could not be spawned (missing binary, permission error).
    SpawnFailed(std::io::Error),
    /// Git exited with a non-zero status code.
    NonZeroExit {
        code: Option<i32>,
        stderr: String,
    },
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
    pub stderr: String,
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
}

impl GitCmd {
    /// Add a single argument.
    pub fn arg(mut self, arg: &str) -> Self {
        self.cmd.arg(arg);
        self
    }

    /// Add multiple arguments.
    pub fn args(mut self, args: &[&str]) -> Self {
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
    /// Returns the trimmed stdout on success.
    pub fn run(mut self) -> Result<GitOutput, GitError> {
        let output = self.cmd.output().map_err(GitError::SpawnFailed)?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

        if !output.status.success() {
            return Err(GitError::NonZeroExit {
                code: output.status.code(),
                stderr,
            });
        }

        Ok(GitOutput { stdout, stderr })
    }

    /// Run the git command, returning `None` on any error (spawn failure or
    /// non-zero exit). Use this for optional/non-fatal calls.
    pub fn run_silent(self) -> Option<GitOutput> {
        self.run().ok()
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

/// Create a git command builder rooted at the given directory.
pub(crate) fn git_cmd(cwd: &Path) -> GitCmd {
    let mut cmd = Command::new(resolve_cli("git"));
    cmd.current_dir(cwd);
    // Prevent git from prompting for credentials in a GUI/TTY context.
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    GitCmd { cmd }
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
    fn test_run_success() {
        let (_dir, path) = setup_test_repo();
        let out = git_cmd(&path).args(&["status", "--porcelain"]).run();
        assert!(out.is_ok());
    }

    #[test]
    fn test_run_non_zero_exit() {
        let (_dir, path) = setup_test_repo();
        // Asking for log in a repo with no commits → non-zero exit
        let result = git_cmd(&path).args(&["log", "--oneline"]).run();
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
        let gc = GitCmd { cmd };
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
        let result = git_cmd(&path).args(&["log", "--oneline"]).run_silent();
        assert!(result.is_none());
    }

    #[test]
    fn test_run_silent_returns_some_on_success() {
        let (_dir, path) = setup_test_repo();
        let result = git_cmd(&path).args(&["status", "--porcelain"]).run_silent();
        assert!(result.is_some());
    }

    #[test]
    fn test_run_raw_returns_output_on_failure() {
        let (_dir, path) = setup_test_repo();
        // log in empty repo → non-zero but raw still returns Ok(Output)
        let result = git_cmd(&path).args(&["log", "--oneline"]).run_raw();
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
        assert_eq!(err_signal.to_string(), "git exited with code signal: killed");
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
            .args(&["status", "--porcelain"])
            .run();
        assert!(out.is_ok());
    }
}
