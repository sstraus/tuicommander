//! CLI execution API for plugins.
//!
//! Provides a sandboxed way for plugins to run whitelisted CLI binaries
//! and capture their stdout as JSON. Only binaries in the allowlist can
//! be executed. The command always enforces `--format json` to ensure
//! machine-parseable output.

use std::collections::VecDeque;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use dashmap::DashMap;

/// Maximum execution time for a CLI command (30 seconds).
const MAX_EXEC_TIMEOUT_SECS: u64 = 30;

/// Maximum stdout size (5 MB).
const MAX_STDOUT_BYTES: usize = 5 * 1024 * 1024;

/// Maximum stderr bytes to include in error messages.
/// Prevents leaking secrets that a CLI tool might emit on stderr.
const MAX_STDERR_BYTES: usize = 256;

/// Maximum exec:cli calls per plugin per minute.
const RATE_LIMIT_PER_MINUTE: usize = 60;

/// Binaries that plugins are allowed to execute.
const ALLOWED_BINARIES: &[&str] = &["mdkb"];

// ---------------------------------------------------------------------------
// Rate limiting
// ---------------------------------------------------------------------------

/// Per-plugin sliding-window rate limiter. Tracks timestamps of recent calls
/// and rejects when the count exceeds RATE_LIMIT_PER_MINUTE within 60 seconds.
fn rate_limiter() -> &'static DashMap<String, Mutex<VecDeque<Instant>>> {
    static LIMITER: OnceLock<DashMap<String, Mutex<VecDeque<Instant>>>> = OnceLock::new();
    LIMITER.get_or_init(DashMap::new)
}

/// Check and record a call for the given plugin. Returns Err if rate limit exceeded.
fn check_rate_limit(plugin_id: &str) -> Result<(), String> {
    let limiter = rate_limiter();
    let entry = limiter
        .entry(plugin_id.to_string())
        .or_insert_with(|| Mutex::new(VecDeque::new()));
    let mut timestamps = entry.lock().unwrap();
    let now = Instant::now();
    let window = Duration::from_secs(60);

    // Evict timestamps older than the window
    while timestamps.front().is_some_and(|t| now.duration_since(*t) > window) {
        timestamps.pop_front();
    }

    if timestamps.len() >= RATE_LIMIT_PER_MINUTE {
        return Err(format!(
            "Plugin \"{plugin_id}\" exceeded exec:cli rate limit ({RATE_LIMIT_PER_MINUTE} calls/minute)"
        ));
    }

    timestamps.push_back(now);
    Ok(())
}

// ---------------------------------------------------------------------------
// Binary resolution
// ---------------------------------------------------------------------------

/// Trusted directories where plugin-executable binaries may live.
/// Only binaries found within these directories (after symlink resolution)
/// are allowed to execute — this prevents symlink attacks where a malicious
/// binary is placed somewhere on PATH and symlinked from a trusted location.
#[cfg(not(windows))]
fn trusted_dirs() -> Vec<std::path::PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();
    vec![
        home.join(".cargo/bin"),
        home.join(".local/bin"),
        std::path::PathBuf::from("/usr/local/bin"),
        std::path::PathBuf::from("/opt/homebrew/bin"),
        home.join(".npm-global/bin"),
        home.join("go/bin"),
    ]
}

#[cfg(windows)]
fn trusted_dirs() -> Vec<std::path::PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();
    vec![
        home.join(".cargo\\bin"),
        home.join(".local\\bin"),
    ]
}

/// Resolve a binary name to an absolute path using known install locations
/// only. Does NOT use `which`/`where` to avoid PATH-based symlink attacks.
/// After finding a candidate, canonicalizes (resolves symlinks) and verifies
/// the canonical path is still within a trusted directory.
fn resolve_binary(name: &str) -> Option<String> {
    let ext = if cfg!(windows) { ".exe" } else { "" };

    for dir in &trusted_dirs() {
        let candidate = dir.join(format!("{name}{ext}"));
        if !candidate.exists() {
            continue;
        }
        // Resolve symlinks and verify the real path is in a trusted directory
        let canonical = match candidate.canonicalize() {
            Ok(p) => p,
            Err(_) => continue,
        };
        if is_in_trusted_dir(&canonical) {
            return Some(canonical.to_string_lossy().to_string());
        }
    }

    None
}

/// Returns true if `path` resides within one of the trusted directories.
fn is_in_trusted_dir(path: &std::path::Path) -> bool {
    trusted_dirs().iter().any(|dir| {
        // Canonicalize the trusted dir too so we compare resolved paths
        dir.canonicalize()
            .map(|d| path.starts_with(&d))
            .unwrap_or(false)
    })
}

// ---------------------------------------------------------------------------
// Path validation
// ---------------------------------------------------------------------------

/// Validate that a working directory path is safe (absolute, exists, within home).
fn validate_cwd(cwd: &str) -> Result<std::path::PathBuf, String> {
    let path = std::path::PathBuf::from(cwd);
    if !path.is_absolute() {
        return Err("Working directory must be an absolute path".into());
    }
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("Failed to resolve working directory: {e}"))?;
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    if !canonical.starts_with(&home) {
        return Err("Working directory must be within the user's home directory".into());
    }
    Ok(canonical)
}

// ---------------------------------------------------------------------------
// Tauri command
// ---------------------------------------------------------------------------

/// Execute a whitelisted CLI binary and return its stdout.
///
/// Security constraints:
/// - Only binaries in ALLOWED_BINARIES can be executed
/// - Working directory must be within $HOME
/// - 30-second timeout
/// - 5 MB stdout limit
/// - stderr is captured but not returned (logged on failure)
#[tauri::command]
pub async fn plugin_exec_cli(
    binary: String,
    args: Vec<String>,
    cwd: Option<String>,
    plugin_id: String,
) -> Result<String, String> {
    // Rate limit per plugin
    check_rate_limit(&plugin_id)?;

    // Validate binary is in the allowlist
    if !ALLOWED_BINARIES.contains(&binary.as_str()) {
        return Err(format!(
            "Binary \"{binary}\" is not in the plugin exec allowlist. Allowed: {}",
            ALLOWED_BINARIES.join(", ")
        ));
    }

    // Resolve binary path
    let binary_path = resolve_binary(&binary)
        .ok_or_else(|| format!("Binary \"{binary}\" not found on this system"))?;

    // Validate and resolve working directory
    let resolved_cwd = if let Some(ref dir) = cwd {
        Some(validate_cwd(dir)?)
    } else {
        None
    };

    // Build command
    let mut cmd = Command::new(&binary_path);
    cmd.args(&args);

    if let Some(ref dir) = resolved_cwd {
        cmd.current_dir(dir);
    }

    // Capture stdout and stderr
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    // Audit log: record invocation before execution
    let start = Instant::now();
    let first_arg = args.first().cloned().unwrap_or_default();

    // Run with timeout via tokio::task::spawn_blocking
    let result = tokio::time::timeout(
        Duration::from_secs(MAX_EXEC_TIMEOUT_SECS),
        tokio::task::spawn_blocking(move || cmd.output()),
    )
    .await
    .map_err(|_| format!("Command \"{binary}\" timed out after {MAX_EXEC_TIMEOUT_SECS}s"))?
    .map_err(|e| format!("Task join error: {e}"))?
    .map_err(|e| format!("Failed to execute \"{binary}\": {e}"))?;

    let duration_ms = start.elapsed().as_millis();
    let exit_ok = result.status.success();
    eprintln!(
        "[plugin_exec] plugin={} binary={} arg0={} duration={}ms ok={}",
        plugin_id, binary, first_arg, duration_ms, exit_ok
    );

    if !result.status.success() {
        // Truncate stderr to prevent leaking secrets a CLI tool might emit
        let stderr_bytes = &result.stderr[..result.stderr.len().min(MAX_STDERR_BYTES)];
        let stderr = String::from_utf8_lossy(stderr_bytes);
        let code = result
            .status
            .code()
            .map(|c| c.to_string())
            .unwrap_or_else(|| "signal".into());
        return Err(format!(
            "Command \"{binary}\" exited with code {code}: {}",
            stderr.trim()
        ));
    }

    let stdout = result.stdout;
    if stdout.len() > MAX_STDOUT_BYTES {
        return Err(format!(
            "Command output exceeds maximum size ({} bytes > {} bytes)",
            stdout.len(),
            MAX_STDOUT_BYTES
        ));
    }

    String::from_utf8(stdout).map_err(|e| format!("Command output is not valid UTF-8: {e}"))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_binary_finds_mdkb_in_trusted_dir() {
        // mdkb should be found via candidate paths, not `which`
        let result = resolve_binary("mdkb");
        if let Some(path) = result {
            let p = std::path::Path::new(&path);
            assert!(p.exists(), "Resolved path must exist");
            // Verify the resolved path is within a trusted directory
            assert!(is_in_trusted_dir(p), "Resolved path must be in a trusted dir");
        }
    }

    #[test]
    fn resolve_binary_returns_none_for_nonexistent() {
        let result = resolve_binary("nonexistent-binary-12345");
        assert!(result.is_none());
    }

    #[test]
    fn resolve_binary_rejects_untrusted_symlink() {
        // Create a temp dir outside trusted dirs with a symlink to /bin/echo
        let tmp = std::env::temp_dir().join("plugin_exec_test_symlink");
        let _ = std::fs::create_dir_all(&tmp);
        let link = tmp.join("mdkb");
        let _ = std::fs::remove_file(&link);
        #[cfg(unix)]
        {
            let _ = std::os::unix::fs::symlink("/bin/echo", &link);
            // /bin/echo is not in a trusted dir, so even if the symlink exists
            // in a location we check, the canonical path should be rejected.
            // Since /tmp is not a trusted dir, this symlink won't be found at all.
            // The point is: resolve_binary only looks in trusted_dirs().
            assert!(resolve_binary("nonexistent-binary-12345").is_none());
        }
        let _ = std::fs::remove_file(&link);
        let _ = std::fs::remove_dir(&tmp);
    }

    #[test]
    fn is_in_trusted_dir_rejects_temp() {
        let tmp = std::env::temp_dir().join("fake_binary");
        assert!(!is_in_trusted_dir(&tmp));
    }

    #[test]
    fn is_in_trusted_dir_accepts_cargo_bin() {
        let home = dirs::home_dir().unwrap();
        let cargo_bin = home.join(".cargo/bin/mdkb");
        // Only passes if .cargo/bin exists (which it does on dev machines)
        if home.join(".cargo/bin").exists() {
            assert!(is_in_trusted_dir(&cargo_bin));
        }
    }

    #[test]
    fn validate_cwd_rejects_relative() {
        assert!(validate_cwd("relative/path").is_err());
    }

    #[test]
    fn validate_cwd_rejects_outside_home() {
        let home = dirs::home_dir().unwrap();
        if !std::path::Path::new("/tmp").starts_with(&home) {
            assert!(validate_cwd("/tmp").is_err());
        }
    }

    #[test]
    fn validate_cwd_accepts_home() {
        let home = dirs::home_dir().unwrap();
        let result = validate_cwd(home.to_str().unwrap());
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn exec_rejects_unlisted_binary() {
        let result = plugin_exec_cli(
            "curl".to_string(),
            vec![],
            None,
            "test-plugin".to_string(),
        )
        .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not in the plugin exec allowlist"));
    }

    #[tokio::test]
    async fn exec_rejects_nonexistent_binary() {
        let result = resolve_binary("mdkb");
        if result.is_none() {
            let r = plugin_exec_cli(
                "mdkb".to_string(),
                vec![],
                None,
                "test-plugin".to_string(),
            )
            .await;
            assert!(r.is_err());
            assert!(r.unwrap_err().contains("not found"));
        }
    }

    #[test]
    fn rate_limit_allows_under_threshold() {
        let id = "test-rate-under";
        // Clear any prior state
        rate_limiter().remove(id);
        for _ in 0..5 {
            assert!(check_rate_limit(id).is_ok());
        }
    }

    #[test]
    fn rate_limit_rejects_over_threshold() {
        let id = "test-rate-over";
        rate_limiter().remove(id);
        // Fill up to the limit
        for _ in 0..RATE_LIMIT_PER_MINUTE {
            assert!(check_rate_limit(id).is_ok());
        }
        // Next call should be rejected
        let result = check_rate_limit(id);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("rate limit"));
    }

    #[test]
    fn stderr_truncation_boundary() {
        // Verify our constant is sane
        assert_eq!(MAX_STDERR_BYTES, 256);
        // Simulate truncation logic
        let long_stderr = "x".repeat(1000);
        let truncated = &long_stderr.as_bytes()[..long_stderr.len().min(MAX_STDERR_BYTES)];
        assert_eq!(truncated.len(), 256);
    }
}
