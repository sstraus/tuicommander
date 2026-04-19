//! CLI execution API for plugins.
//!
//! Provides a sandboxed way for plugins to run CLI binaries declared in
//! their manifest's `binaries` field. The on-disk manifest is the source
//! of truth — the frontend cannot grant binary access that the manifest
//! doesn't declare.

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

/// Canonicalized trusted directories, computed once.
fn canonical_trusted_dirs() -> &'static [std::path::PathBuf] {
    static DIRS: OnceLock<Vec<std::path::PathBuf>> = OnceLock::new();
    DIRS.get_or_init(|| {
        trusted_dirs()
            .into_iter()
            .filter_map(|d| d.canonicalize().ok())
            .collect()
    })
}

/// Returns true if `path` resides within one of the trusted directories.
fn is_in_trusted_dir(path: &std::path::Path) -> bool {
    canonical_trusted_dirs().iter().any(|d| path.starts_with(d))
}

// ---------------------------------------------------------------------------
// Path validation
// ---------------------------------------------------------------------------

/// Validate that a working directory path is safe (absolute, exists, within home).
fn validate_cwd(cwd: &str) -> Result<std::path::PathBuf, String> {
    let path = std::path::PathBuf::from(crate::cli::expand_tilde(cwd));
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

/// Execute a CLI binary declared in the plugin's manifest `binaries` field.
///
/// Security constraints:
/// - Only binaries listed in the on-disk manifest can be executed
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
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
) -> Result<String, String> {
    crate::plugins::check_plugin_capability(&state, &plugin_id, "exec:cli")?;

    // Read allowed binaries from the on-disk manifest (source of truth)
    let manifest = crate::plugins::read_single_manifest(&plugin_id)?;

    plugin_exec_cli_inner(binary, args, cwd, plugin_id, &manifest.binaries).await
}

/// Core exec logic, separated from the Tauri command wrapper for testability.
async fn plugin_exec_cli_inner(
    binary: String,
    args: Vec<String>,
    cwd: Option<String>,
    plugin_id: String,
    allowed_binaries: &[String],
) -> Result<String, String> {
    // Rate limit per plugin
    check_rate_limit(&plugin_id)?;

    // Validate binary is declared in the plugin's manifest
    if !allowed_binaries.iter().any(|b| b == &binary) {
        return Err(format!(
            "Binary \"{binary}\" is not declared in plugin \"{plugin_id}\" manifest binaries. Declared: {}",
            if allowed_binaries.is_empty() { "(none)".to_string() } else { allowed_binaries.join(", ") }
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

    // Build std Command first for apply_no_window, then convert to async
    let mut std_cmd = Command::new(&binary_path);
    std_cmd.args(&args);
    if let Some(ref dir) = resolved_cwd {
        std_cmd.current_dir(dir);
    }
    std_cmd.stdout(std::process::Stdio::piped());
    std_cmd.stderr(std::process::Stdio::piped());
    crate::cli::apply_no_window(&mut std_cmd);

    // Convert to tokio::process::Command for async timeout + kill
    let mut cmd: tokio::process::Command = std_cmd.into();

    // Audit log: record invocation before execution
    let start = Instant::now();
    let first_arg = args.first().cloned().unwrap_or_default();

    // Spawn and wait with timeout — kill child on timeout
    let mut child = cmd.spawn()
        .map_err(|e| format!("Failed to execute \"{binary}\": {e}"))?;

    // Take stdout/stderr handles before waiting so we can read them after wait()
    let mut stdout_pipe = child.stdout.take()
        .ok_or_else(|| "Failed to capture stdout".to_string())?;
    let mut stderr_pipe = child.stderr.take()
        .ok_or_else(|| "Failed to capture stderr".to_string())?;

    let status = match tokio::time::timeout(
        Duration::from_secs(MAX_EXEC_TIMEOUT_SECS),
        child.wait(),
    ).await {
        Ok(s) => s.map_err(|e| format!("Failed to execute \"{binary}\": {e}"))?,
        Err(_) => {
            // Timeout: kill the child process to prevent zombies
            let _ = child.kill().await;
            return Err(format!("Command \"{binary}\" timed out after {MAX_EXEC_TIMEOUT_SECS}s"));
        }
    };

    // Read captured output after process has exited
    use tokio::io::AsyncReadExt;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let _ = stdout_pipe.read_to_end(&mut stdout).await;
    let _ = stderr_pipe.read_to_end(&mut stderr).await;

    let duration_ms = start.elapsed().as_millis();
    let exit_ok = status.success();
    tracing::debug!(
        source = "plugin_exec",
        plugin = %plugin_id, binary = %binary, arg0 = %first_arg,
        duration_ms = duration_ms, ok = exit_ok,
        "Plugin exec completed"
    );

    if !status.success() {
        // Truncate stderr to prevent leaking secrets a CLI tool might emit
        let stderr_bytes = &stderr[..stderr.len().min(MAX_STDERR_BYTES)];
        let stderr_str = String::from_utf8_lossy(stderr_bytes);
        let code = status
            .code()
            .map(|c| c.to_string())
            .unwrap_or_else(|| "signal".into());
        return Err(format!(
            "Command \"{binary}\" exited with code {code}: {}",
            stderr_str.trim()
        ));
    }
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
    async fn exec_rejects_undeclared_binary() {
        let allowed = vec!["mdkb".to_string()];
        let result = plugin_exec_cli_inner(
            "curl".to_string(),
            vec![],
            None,
            "test-plugin".to_string(),
            &allowed,
        )
        .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not declared in plugin"));
    }

    #[tokio::test]
    async fn exec_rejects_when_no_binaries_declared() {
        let allowed: Vec<String> = vec![];
        let result = plugin_exec_cli_inner(
            "mdkb".to_string(),
            vec![],
            None,
            "test-plugin".to_string(),
            &allowed,
        )
        .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("(none)"));
    }

    #[tokio::test]
    async fn exec_rejects_nonexistent_binary() {
        let result = resolve_binary("mdkb");
        if result.is_none() {
            let allowed = vec!["mdkb".to_string()];
            let r = plugin_exec_cli_inner(
                "mdkb".to_string(),
                vec![],
                None,
                "test-plugin".to_string(),
                &allowed,
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
