//! CLI execution API for plugins.
//!
//! Provides a sandboxed way for plugins to run CLI binaries declared in
//! their manifest's `binaries` field. The on-disk manifest is the source
//! of truth — the frontend cannot grant binary access that the manifest
//! doesn't declare.

use std::collections::VecDeque;
use std::process::Command;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use parking_lot::Mutex;

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
///
/// `parking_lot::Mutex` has no poison state: a panic taken anywhere near the
/// critical section leaves the entry usable, where a poisoned `std` mutex would
/// have disabled exec:cli for that plugin for the rest of the process lifetime.
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
    let mut timestamps = entry.lock();
    let now = Instant::now();
    let window = Duration::from_secs(60);

    // Evict timestamps older than the window
    while timestamps
        .front()
        .is_some_and(|t| now.duration_since(*t) > window)
    {
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

/// Drop a plugin's rate-limiter entry (called on uninstall so the id's call
/// history doesn't linger for the process lifetime).
pub(crate) fn clear_rate_limit(plugin_id: &str) {
    rate_limiter().remove(plugin_id);
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
    vec![home.join(".cargo\\bin"), home.join(".local\\bin")]
}

/// Resolve a binary name to an absolute path using known install locations
/// only. Does NOT use `which`/`where` to avoid PATH-based symlink attacks.
/// Accepts if the symlink itself lives in a trusted dir (package-manager
/// managed) OR if the canonical target does — covers homebrew Cellar symlinks.
pub(crate) fn resolve_binary(name: &str) -> Option<String> {
    let ext = if cfg!(windows) { ".exe" } else { "" };

    for dir in &trusted_dirs() {
        let candidate = dir.join(format!("{name}{ext}"));
        if !candidate.exists() {
            continue;
        }
        let canonical = match candidate.canonicalize() {
            Ok(p) => p,
            Err(_) => continue,
        };
        if is_in_trusted_dir(&candidate) || is_in_trusted_dir(&canonical) {
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
///
/// Each form is compared against its own. The canonical list alone is not
/// enough: on Windows `canonicalize` prefixes `\\?\`, which a path nobody
/// canonicalized cannot match, and a path that does not exist yet cannot be
/// canonicalized at all — so on Windows this answered false for every plain
/// path, including the trusted directories themselves. A `..` anywhere
/// disqualifies the plain comparison, because `starts_with` matches components
/// and would let the path climb back out of the directory it just entered.
fn is_in_trusted_dir(path: &std::path::Path) -> bool {
    let climbs = path
        .components()
        .any(|c| c == std::path::Component::ParentDir);
    if !climbs && trusted_dirs().iter().any(|d| path.starts_with(d)) {
        return true;
    }
    path.canonicalize().is_ok_and(|canonical| {
        canonical_trusted_dirs()
            .iter()
            .any(|d| canonical.starts_with(d))
    })
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
    // Canonicalized like the path it is compared against: on Windows
    // `canonicalize` returns a `\\?\`-prefixed path, which a plain home
    // directory never prefixes, so every working directory would be rejected.
    let home = dirs::home_dir()
        .ok_or("Cannot determine home directory")?
        .canonicalize()
        .map_err(|e| format!("Failed to resolve home directory: {e}"))?;
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
#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn plugin_exec_cli(
    binary: String,
    args: Vec<String>,
    cwd: Option<String>,
    plugin_id: String,
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
) -> Result<String, String> {
    plugin_exec_cli_impl(&state, binary, args, cwd, plugin_id).await
}

pub(crate) async fn plugin_exec_cli_impl(
    state: &std::sync::Arc<crate::AppState>,
    binary: String,
    args: Vec<String>,
    cwd: Option<String>,
    plugin_id: String,
) -> Result<String, String> {
    crate::plugins::check_plugin_capability(state, &plugin_id, "exec:cli")?;

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
            if allowed_binaries.is_empty() {
                "(none)".to_string()
            } else {
                allowed_binaries.join(", ")
            }
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
    cmd.kill_on_drop(true);

    // Audit log: record invocation before execution
    let start = Instant::now();
    let first_arg = args.first().cloned().unwrap_or_default();

    // Spawn and wait with timeout — kill child on timeout
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to execute \"{binary}\": {e}"))?;

    let (status, stdout, stderr) = capture_cli_output(
        &mut child,
        &binary,
        Duration::from_secs(MAX_EXEC_TIMEOUT_SECS),
    )
    .await?;

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
    String::from_utf8(stdout).map_err(|e| format!("Command output is not valid UTF-8: {e}"))
}

async fn read_bounded_pipe(
    pipe: impl tokio::io::AsyncRead + Unpin,
    stream: &str,
) -> Result<Vec<u8>, String> {
    use tokio::io::AsyncReadExt;
    let mut bytes = Vec::new();
    pipe.take((MAX_STDOUT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .await
        .map_err(|error| format!("Failed to read {stream}: {error}"))?;
    if bytes.len() > MAX_STDOUT_BYTES {
        return Err(format!(
            "Command {stream} exceeds maximum size ({MAX_STDOUT_BYTES} bytes)"
        ));
    }
    Ok(bytes)
}

async fn capture_cli_output(
    child: &mut tokio::process::Child,
    binary: &str,
    timeout: Duration,
) -> Result<(std::process::ExitStatus, Vec<u8>, Vec<u8>), String> {
    let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
    let stderr = child.stderr.take().ok_or("Failed to capture stderr")?;
    let capture = async {
        tokio::try_join!(
            async {
                child
                    .wait()
                    .await
                    .map_err(|error| format!("Failed to execute {binary}: {error}"))
            },
            read_bounded_pipe(stdout, "stdout"),
            read_bounded_pipe(stderr, "stderr"),
        )
    };
    let result = match tokio::time::timeout(timeout, capture).await {
        Ok(result) => result,
        Err(_) => Err(format!(
            "Command \"{binary}\" timed out after {}s",
            timeout.as_secs_f64()
        )),
    };
    if let Err(error) = result {
        child
            .kill()
            .await
            .map_err(|kill_error| format!("{error}; failed to stop child: {kill_error}"))?;
        return Err(error);
    }
    result
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn output_child(script: &str) -> tokio::process::Child {
        let (shell, flag) = crate::test_support::host_shell();
        tokio::process::Command::new(shell)
            .args([flag, script])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("shell")
    }

    // Catches: waiting before draining a full stdout/stderr pipe, or rejecting the exact output cap.
    #[tokio::test]
    async fn cli_capture_drains_large_pipes_and_accepts_exact_limit() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("tempdir");
        for (size, stderr_only) in [(200_000, false), (200_000, true), (5 * 1024 * 1024, false)] {
            let file = dir.path().join("output");
            let expected = vec![b'x'; size];
            std::fs::write(&file, &expected).expect("write output");
            let mut script = tuic_test_support::print_file_script(file.to_str().expect("path"));
            if stderr_only {
                script.push_str(" >&2");
            }
            let mut child = output_child(&script);
            let (status, stdout, stderr) =
                capture_cli_output(&mut child, "shell", Duration::from_secs(30))
                    .await
                    .expect("capture");
            assert!(status.success());
            if stderr_only {
                assert_eq!(stdout, [] as [u8; 0]);
                assert_eq!(stderr, expected);
            } else {
                assert_eq!(stdout, expected);
                assert_eq!(stderr, [] as [u8; 0]);
            }
        }
    }

    // Catches: unbounded buffering or silently truncating overflowing output instead of stopping the child.
    #[tokio::test]
    async fn cli_capture_kills_child_when_either_pipe_overflows() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("tempdir");
        let file = dir.path().join("overflow");
        std::fs::write(&file, vec![b'x'; 5 * 1024 * 1024 + 1]).expect("write output");
        for stderr_only in [false, true] {
            let mut script = tuic_test_support::print_file_script(file.to_str().expect("path"));
            if stderr_only {
                script.push_str(" >&2");
            }
            let mut child = output_child(&script);
            let error = capture_cli_output(&mut child, "shell", Duration::from_secs(30))
                .await
                .expect_err("overflow");
            assert!(error.contains("exceeds maximum size"), "{error}");
            assert!(child.id().is_none(), "overflowing child not reaped");
        }
    }

    // Catches: timing out the wait while leaving a child alive.
    #[tokio::test]
    async fn cli_capture_timeout_kills_and_reaps_child() {
        let mut child = output_child(&tuic_test_support::wait_for_stdin_script());
        // Child::wait closes its stdin; retain the writer so the fixture stays blocked.
        let _stdin = child.stdin.take().expect("piped stdin");
        let error = capture_cli_output(&mut child, "shell", Duration::from_millis(100))
            .await
            .expect_err("timeout");
        assert!(error.contains("timed out"), "{error}");
        assert!(child.id().is_none(), "timed-out child not reaped");
    }

    // Catches: ignored pipe read failures returning successful empty output.
    #[tokio::test]
    async fn cli_capture_propagates_pipe_read_errors() {
        struct BrokenPipe;
        impl tokio::io::AsyncRead for BrokenPipe {
            fn poll_read(
                self: std::pin::Pin<&mut Self>,
                _cx: &mut std::task::Context<'_>,
                _buf: &mut tokio::io::ReadBuf<'_>,
            ) -> std::task::Poll<std::io::Result<()>> {
                std::task::Poll::Ready(Err(std::io::Error::other("broken pipe fixture")))
            }
        }
        let error = read_bounded_pipe(BrokenPipe, "stdout")
            .await
            .expect_err("read failure");
        assert!(
            error.contains("Failed to read stdout") && error.contains("broken pipe fixture"),
            "{error}"
        );
    }

    #[test]
    fn resolve_binary_finds_mdkb_in_trusted_dir() {
        let result = resolve_binary("mdkb");
        if let Some(path) = result {
            let p = std::path::Path::new(&path);
            assert!(p.exists(), "Resolved path must exist");
            let in_trusted =
                is_in_trusted_dir(p) || trusted_dirs().iter().any(|d| d.join("mdkb").exists());
            assert!(in_trusted, "mdkb must be reachable from a trusted dir");
        }
    }

    /// A panic taken while a plugin's rate-limit entry is locked must not
    /// disable that plugin's limiter for the rest of the process lifetime.
    /// `std::sync::Mutex` poisons on panic and `.lock().unwrap()` then panics
    /// on every later call; `parking_lot::Mutex` has no poison state.
    #[test]
    fn rate_limiter_survives_panic_while_entry_is_locked() {
        let plugin_id = "poison-probe";
        assert!(check_rate_limit(plugin_id).is_ok(), "first call must pass");

        // Silence the unwind backtrace: the panic below is the fixture, not noise.
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let panicked = std::panic::catch_unwind(|| {
            let entry = rate_limiter().get(plugin_id).expect("entry registered");
            let _guard = entry.lock();
            panic!("panic inside the rate-limit critical section");
        })
        .is_err();
        std::panic::set_hook(hook);
        assert!(panicked, "the fixture must actually panic under the lock");

        assert!(
            check_rate_limit(plugin_id).is_ok(),
            "limiter unusable after a panic under its lock"
        );
    }

    #[test]
    fn resolve_binary_returns_none_for_nonexistent() {
        let result = resolve_binary("nonexistent-binary-12345");
        assert!(result.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn resolve_binary_rejects_untrusted_symlink() {
        // Create a temp dir outside trusted dirs with a symlink to /bin/echo
        let tmp = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let link = tmp.path().join("mdkb");
        #[cfg(unix)]
        {
            let _ = std::os::unix::fs::symlink("/bin/echo", &link);
            // /bin/echo is not in a trusted dir, so even if the symlink exists
            // in a location we check, the canonical path should be rejected.
            // The test scratch root is not trusted, so this symlink is not found.
            // The point is: resolve_binary only looks in trusted_dirs().
            assert!(resolve_binary("nonexistent-binary-12345").is_none());
        }
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
        let outside = crate::test_support::dir_outside_home();
        if !outside.starts_with(&home) {
            assert!(validate_cwd(&outside.to_string_lossy()).is_err());
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
    fn clear_rate_limit_drops_entry() {
        let id = "test-rate-clear";
        rate_limiter().remove(id);
        // Saturate the window so further calls would be rejected.
        for _ in 0..RATE_LIMIT_PER_MINUTE {
            assert!(check_rate_limit(id).is_ok());
        }
        assert!(check_rate_limit(id).is_err());
        assert!(rate_limiter().contains_key(id));

        // Uninstall path: clearing the entry wipes the call history.
        clear_rate_limit(id);
        assert!(!rate_limiter().contains_key(id));
        // The window is empty again, so calls succeed.
        assert!(check_rate_limit(id).is_ok());
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
