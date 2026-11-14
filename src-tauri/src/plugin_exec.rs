//! CLI execution API for plugins.
//!
//! Provides a sandboxed way for plugins to run CLI binaries declared in
//! their manifest's `binaries` field. The on-disk manifest is the source
//! of truth — the frontend cannot grant binary access that the manifest
//! doesn't declare.

use std::process::Command;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use dashmap::DashMap;
use tokio::sync::Semaphore;

/// Execution time for a CLI command when the plugin asks for none.
const DEFAULT_EXEC_TIMEOUT: Duration = Duration::from_secs(30);

/// Longest execution time a plugin may request (1 hour): room for an owner's
/// indexing or build command, still finite.
const MAX_EXEC_TIMEOUT: Duration = Duration::from_secs(60 * 60);

/// Maximum stdout size (5 MB).
const MAX_STDOUT_BYTES: usize = 5 * 1024 * 1024;

/// Maximum stderr bytes retained and returned in error messages (5 MiB).
/// Registered secrets are masked before the text is returned.
const MAX_STDERR_BYTES: usize = 5 * 1024 * 1024;

/// Appended to an error message when stderr exceeded `MAX_STDERR_BYTES`.
const STDERR_TRUNCATION_MARK: &str = "\n[stderr truncated at 5 MiB]";

/// Captured stderr: at most `MAX_STDERR_BYTES`, plus whether more was produced.
#[derive(Debug, PartialEq)]
struct CapturedStderr {
    bytes: Vec<u8>,
    truncated: bool,
}

/// Commands one plugin may run at the same time. A further call waits for a
/// free slot; there is no calls-per-minute quota.
const MAX_CONCURRENT_EXECS: usize = 4;

// ---------------------------------------------------------------------------
// Execution budget
// ---------------------------------------------------------------------------

/// Validate a plugin's requested deadline. Out-of-range values are refused,
/// not clamped, so a plugin never runs with a deadline it did not ask for.
fn exec_timeout(timeout_ms: Option<u64>) -> Result<Duration, String> {
    let Some(ms) = timeout_ms else {
        return Ok(DEFAULT_EXEC_TIMEOUT);
    };
    let timeout = Duration::from_millis(ms);
    if ms == 0 || timeout > MAX_EXEC_TIMEOUT {
        return Err(format!(
            "exec:cli timeoutMs must be between 1 and {} ms, got {ms}",
            MAX_EXEC_TIMEOUT.as_millis()
        ));
    }
    Ok(timeout)
}

/// Per-plugin execution slots.
fn exec_slots() -> &'static DashMap<String, Arc<Semaphore>> {
    static SLOTS: OnceLock<DashMap<String, Arc<Semaphore>>> = OnceLock::new();
    SLOTS.get_or_init(DashMap::new)
}

/// Drop a plugin's execution slots (called on uninstall so the id's entry
/// doesn't linger for the process lifetime). Running commands keep the slot
/// they hold until they end.
pub(crate) fn clear_exec_slots(plugin_id: &str) {
    exec_slots().remove(plugin_id);
}

/// Run `cmd` inside the plugin's concurrency bound. The deadline starts after
/// the slot is acquired and the child is spawned, so waiting for a slot never
/// consumes it. The slot is released on every exit path, including when the
/// caller drops this future; the process-tree guard then stops the child and
/// its subprocesses.
async fn run_budgeted(
    plugin_id: &str,
    binary: &str,
    mut std_cmd: Command,
    timeout: Duration,
) -> Result<(std::process::ExitStatus, Vec<u8>, CapturedStderr), String> {
    let slots = exec_slots()
        .entry(plugin_id.to_string())
        .or_insert_with(|| Arc::new(Semaphore::new(MAX_CONCURRENT_EXECS)))
        .clone();
    let _slot = slots
        .acquire_owned()
        .await
        .map_err(|error| format!("exec:cli slots closed: {error}"))?;
    // The command owns a process group (Unix) or job object (Windows), so
    // timeout and cancel stop its subprocesses too, not only the direct child.
    let tree = crate::agent::ScreenProbeTree::prepare(&mut std_cmd)
        .map_err(|e| format!("Failed to prepare \"{binary}\" process tree: {e}"))?;
    let mut cmd: tokio::process::Command = std_cmd.into();
    cmd.kill_on_drop(true);
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to execute \"{binary}\": {e}"))?;
    let mut tree_guard = ProcessTreeGuard {
        tree: Some(tree),
        pid: child.id().unwrap_or_default(),
    };
    tree_guard
        .tree
        .as_ref()
        .expect("tree set above")
        .assign_tokio(&child)
        .map_err(|e| format!("Failed to contain \"{binary}\": {e}"))?;
    let output = capture_cli_output(&mut child, binary, timeout).await?;
    // Finished normally: leave anything it deliberately detached alone.
    tree_guard.disarm();
    Ok(output)
}

// ---------------------------------------------------------------------------
// Binary resolution
// ---------------------------------------------------------------------------

/// Folders searched for a declared program, in order: the effective backend
/// PATH (the install folders that desktop-launched apps miss, then the
/// inherited PATH), then the user install folders that release builds do not
/// inherit. The manifest declaration is the boundary, not the folder. The
/// spawned program gets the same list as its PATH.
fn search_dirs() -> Vec<std::path::PathBuf> {
    let path = crate::cli::enriched_path();
    let mut dirs: Vec<std::path::PathBuf> = std::env::split_paths(&path).collect();
    if let Some(home) = dirs::home_dir() {
        // `enriched_path` already holds ~/.cargo/bin, and ~/.local/bin on Linux.
        #[cfg(not(target_os = "linux"))]
        dirs.push(home.join(".local").join("bin"));
        dirs.extend([
            home.join(".npm-global").join("bin"),
            home.join("go").join("bin"),
        ]);
    }
    dirs
}

/// Resolve a declared program name to the first executable in `search_dirs`.
/// The Homebrew folders come before the inherited PATH, so the result can
/// differ from the user's shell. Returns the path as found, not its canonical
/// target: a multi-call program behind a symlink selects its tool from `argv[0]`.
pub(crate) fn resolve_binary(name: &str) -> Option<String> {
    resolve_binary_in(name, search_dirs())
}

fn resolve_binary_in(
    name: &str,
    dirs: impl IntoIterator<Item = std::path::PathBuf>,
) -> Option<String> {
    // The manifest contract allows only bare names: a path never resolves.
    let bare = std::path::Path::new(name);
    let mut components = bare.components();
    if !matches!(
        (components.next(), components.next()),
        (Some(std::path::Component::Normal(_)), None)
    ) {
        return None;
    }
    #[cfg(windows)]
    let file_names: Vec<String> = if bare.extension().is_some() {
        vec![name.to_string()]
    } else {
        vec![format!("{name}.exe"), format!("{name}.cmd")]
    };
    #[cfg(not(windows))]
    let file_names = [name.to_string()];

    dirs.into_iter()
        .filter(|dir| dir.is_absolute())
        .flat_map(|dir| file_names.iter().map(move |file| dir.join(file)))
        .find(|candidate| is_executable(candidate))
        .map(|candidate| candidate.to_string_lossy().to_string())
}

/// True for a regular file (after symlinks) that the user may execute.
fn is_executable(path: &std::path::Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        metadata.is_file()
    }
}

// ---------------------------------------------------------------------------
// Path validation
// ---------------------------------------------------------------------------

/// Validate that a working directory is an absolute path to an existing directory.
fn validate_cwd(cwd: &str) -> Result<std::path::PathBuf, String> {
    let path = std::path::PathBuf::from(crate::cli::expand_tilde(cwd));
    if !path.is_absolute() {
        return Err("Working directory must be an absolute path".into());
    }
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("Failed to resolve working directory: {e}"))?;
    Ok(canonical)
}

// ---------------------------------------------------------------------------
// Tauri command
// ---------------------------------------------------------------------------

/// Execute a CLI binary declared in the plugin's manifest `binaries` field.
///
/// Security constraints:
/// - Only binaries listed in the on-disk manifest can be executed
/// - Working directory must be an absolute path to an existing directory
/// - 30-second default timeout, raisable to `MAX_EXEC_TIMEOUT`
/// - at most `MAX_CONCURRENT_EXECS` concurrent commands per plugin
/// - 5 MB stdout limit
/// - stderr (5 MiB, secrets masked, truncation marked) is returned on failure
#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn plugin_exec_cli(
    binary: String,
    args: Vec<String>,
    cwd: Option<String>,
    plugin_id: String,
    timeout_ms: Option<u64>,
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
) -> Result<String, String> {
    plugin_exec_cli_impl(&state, binary, args, cwd, plugin_id, timeout_ms).await
}

pub(crate) async fn plugin_exec_cli_impl(
    state: &std::sync::Arc<crate::AppState>,
    binary: String,
    args: Vec<String>,
    cwd: Option<String>,
    plugin_id: String,
    timeout_ms: Option<u64>,
) -> Result<String, String> {
    crate::plugins::check_plugin_capability(state, &plugin_id, "exec:cli")?;

    // Read allowed binaries from the on-disk manifest (source of truth)
    let manifest = crate::plugins::read_single_manifest(&plugin_id)?;

    plugin_exec_cli_inner(
        binary,
        args,
        cwd,
        plugin_id,
        timeout_ms,
        &manifest.binaries,
        &state.secrets,
    )
    .await
}

/// Core exec logic, separated from the Tauri command wrapper for testability.
async fn plugin_exec_cli_inner(
    binary: String,
    args: Vec<String>,
    cwd: Option<String>,
    plugin_id: String,
    timeout_ms: Option<u64>,
    allowed_binaries: &[String],
    secrets: &crate::secrets::SecretStore,
) -> Result<String, String> {
    let timeout = exec_timeout(timeout_ms)?;

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

    run_cli(
        &binary,
        &binary_path,
        &args,
        cwd,
        &plugin_id,
        timeout,
        secrets,
    )
    .await
}

/// Stops the command's whole process tree when dropped, which covers timeout,
/// output overflow and cancellation (the future being dropped).
struct ProcessTreeGuard {
    tree: Option<crate::agent::ScreenProbeTree>,
    pid: u32,
}

impl ProcessTreeGuard {
    fn disarm(&mut self) {
        // Windows closes the job handle here, which also ends any descendant
        // the finished command left behind; Unix leaves them running.
        self.tree.take();
    }
}

impl Drop for ProcessTreeGuard {
    fn drop(&mut self) {
        if let Some(tree) = self.tree.take() {
            tree.terminate(self.pid);
        }
    }
}

/// Spawn a resolved binary in a validated working directory, bounded by the
/// timeout and output limits.
async fn run_cli(
    binary: &str,
    binary_path: &str,
    args: &[String],
    cwd: Option<String>,
    plugin_id: &str,
    timeout: Duration,
    secrets: &crate::secrets::SecretStore,
) -> Result<String, String> {
    // Validate and resolve working directory
    let resolved_cwd = if let Some(ref dir) = cwd {
        Some(validate_cwd(dir)?)
    } else {
        None
    };

    // Build std Command first for apply_no_window, then convert to async
    let mut std_cmd = Command::new(binary_path);
    std_cmd.args(args);
    // The PATH the program was resolved from, so a `#!/usr/bin/env node`
    // script in a user install folder finds its interpreter.
    if let Ok(path) = std::env::join_paths(search_dirs()) {
        std_cmd.env("PATH", path);
    }
    if let Some(ref dir) = resolved_cwd {
        std_cmd.current_dir(dir);
    }
    std_cmd.stdout(std::process::Stdio::piped());
    std_cmd.stderr(std::process::Stdio::piped());
    crate::cli::apply_no_window(&mut std_cmd);

    // Audit log: record invocation before execution
    let start = Instant::now();
    let first_arg = args.first().cloned().unwrap_or_default();

    let (status, stdout, stderr) = run_budgeted(plugin_id, binary, std_cmd, timeout).await?;

    let duration_ms = start.elapsed().as_millis();
    let exit_ok = status.success();
    tracing::debug!(
        source = "plugin_exec",
        plugin = %plugin_id, binary = %binary, arg0 = %first_arg,
        duration_ms = duration_ms, ok = exit_ok,
        "Plugin exec completed"
    );

    if !status.success() {
        // Masking 5 MiB costs seconds per stored secret: keep it off the async workers.
        let masker = secrets.masker();
        let binary = binary.to_string();
        let code = status.code();
        let message =
            tokio::task::spawn_blocking(move || failure_message(&binary, code, &stderr, &masker))
                .await
                .map_err(|error| format!("Failed to mask stderr: {error}"))?;
        return Err(message);
    }
    String::from_utf8(stdout).map_err(|e| format!("Command output is not valid UTF-8: {e}"))
}

/// Error text for a failed command: exit code, stderr with registered secrets
/// masked, and a mark when stderr was cut at the bound.
fn failure_message(
    binary: &str,
    code: Option<i32>,
    stderr: &CapturedStderr,
    masker: &crate::secrets::Masker,
) -> String {
    let code = code.map_or_else(|| "signal".into(), |c| c.to_string());
    let text = masker.mask(&String::from_utf8_lossy(&stderr.bytes));
    let mark = if stderr.truncated {
        STDERR_TRUNCATION_MARK
    } else {
        ""
    };
    format!(
        "Command \"{binary}\" exited with code {code}: {}{mark}",
        text.trim()
    )
}

/// Keep the first `MAX_STDERR_BYTES`; drain the rest so the child never blocks.
async fn read_stderr_pipe(
    pipe: impl tokio::io::AsyncRead + Unpin,
) -> Result<CapturedStderr, String> {
    use tokio::io::AsyncReadExt;
    let mut pipe = pipe;
    let mut bytes = Vec::new();
    (&mut pipe)
        .take(MAX_STDERR_BYTES as u64)
        .read_to_end(&mut bytes)
        .await
        .map_err(|error| format!("Failed to read stderr: {error}"))?;
    let dropped = tokio::io::copy(&mut pipe, &mut tokio::io::sink())
        .await
        .map_err(|error| format!("Failed to read stderr: {error}"))?;
    Ok(CapturedStderr {
        bytes,
        truncated: dropped > 0,
    })
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
) -> Result<(std::process::ExitStatus, Vec<u8>, CapturedStderr), String> {
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
            read_stderr_pipe(stderr),
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
                assert_eq!(stderr.bytes, expected);
                assert!(!stderr.truncated);
            } else {
                assert_eq!(stdout, expected);
                assert_eq!(stderr.bytes, [] as [u8; 0]);
            }
        }
    }

    // Catches: unbounded buffering or silently truncating overflowing stdout instead of stopping the child.
    #[tokio::test]
    async fn cli_capture_kills_child_when_stdout_overflows() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("tempdir");
        let file = dir.path().join("overflow");
        std::fs::write(&file, vec![b'x'; 5 * 1024 * 1024 + 1]).expect("write output");
        let script = tuic_test_support::print_file_script(file.to_str().expect("path"));
        let mut child = output_child(&script);
        let error = capture_cli_output(&mut child, "shell", Duration::from_secs(30))
            .await
            .expect_err("overflow");
        assert!(error.contains("exceeds maximum size"), "{error}");
        assert!(child.id().is_none(), "overflowing child not reaped");
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

    // Catches: kill_on_drop stopping only the direct child, so a cancelled command's subprocesses keep running.
    #[cfg(unix)]
    #[tokio::test]
    async fn exec_cli_cancel_kills_the_whole_process_group() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("tempdir");
        let pid_file = dir.path().join("grandchild.pid");
        let script = format!("sleep 600 & echo $! > '{}'; wait", pid_file.display());
        let args = vec!["-c".to_string(), script];
        let secrets = crate::secrets::SecretStore::default();
        let run = run_cli(
            "sh",
            "/bin/sh",
            &args,
            None,
            "test-plugin",
            DEFAULT_EXEC_TIMEOUT,
            &secrets,
        );
        let _ = tokio::time::timeout(Duration::from_secs(2), run).await;
        let pid: libc::pid_t = std::fs::read_to_string(&pid_file)
            .expect("grandchild pid")
            .trim()
            .parse()
            .expect("pid");
        // Signal 0 probes existence; allow the kernel a moment to deliver SIGKILL.
        let mut alive = true;
        for _ in 0..50 {
            alive = unsafe { libc::kill(pid, 0) } == 0;
            if !alive {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        if alive {
            unsafe { libc::kill(pid, libc::SIGKILL) };
        }
        assert!(!alive, "grandchild {pid} survived the cancelled command");
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
    fn resolve_binary_returns_none_for_nonexistent() {
        let result = resolve_binary("nonexistent-binary-12345");
        assert!(result.is_none());
    }

    /// Write an executable fixture program, or a plain file when `mode` lacks
    /// execute bits.
    #[cfg(unix)]
    fn program(dir: &std::path::Path, name: &str, mode: u32) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        std::fs::write(&path, "#!/bin/sh\n").expect("write program");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).expect("chmod");
        path
    }

    // Catches: canonicalizing a symlinked program (a version-manager or
    // multi-call proxy selects its tool from argv[0]), skipping a symlink whose
    // target lives outside the folder, or following PATH out of order.
    #[cfg(unix)]
    #[test]
    fn resolve_binary_returns_first_path_symlink_as_found() {
        let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("tempdir");
        let (shims, store, later) = (
            root.path().join("shims"),
            root.path().join("store"),
            root.path().join("later"),
        );
        for dir in [&shims, &store, &later] {
            std::fs::create_dir(dir).expect("mkdir");
        }
        let target = program(&store, "multi-call", 0o755);
        std::os::unix::fs::symlink(&target, shims.join("tool")).expect("symlink");
        program(&later, "tool", 0o755);

        let resolved = resolve_binary_in("tool", [shims.clone(), later]);
        assert_eq!(
            resolved,
            Some(shims.join("tool").to_string_lossy().to_string())
        );
    }

    // Catches: resolving a file that cannot run (no execute bit, dangling
    // symlink, directory) instead of the next executable on PATH, or reporting
    // a missing program as found.
    #[cfg(unix)]
    #[test]
    fn resolve_binary_skips_entries_that_cannot_run() {
        let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("tempdir");
        let dirs: Vec<_> = ["plain", "dangling", "folder", "real"]
            .iter()
            .map(|name| root.path().join(name))
            .collect();
        for dir in &dirs {
            std::fs::create_dir(dir).expect("mkdir");
        }
        program(&dirs[0], "tool", 0o644);
        std::os::unix::fs::symlink(root.path().join("absent"), dirs[1].join("tool"))
            .expect("symlink");
        std::fs::create_dir(dirs[2].join("tool")).expect("mkdir tool");

        assert_eq!(resolve_binary_in("tool", dirs[..3].to_vec()), None);
        let real = program(&dirs[3], "tool", 0o755);
        assert_eq!(
            resolve_binary_in("tool", dirs.clone()),
            Some(real.to_string_lossy().to_string())
        );
        assert_eq!(resolve_binary_in("absent-tool", dirs), None);
    }

    // Catches: a request with a path, or a relative PATH entry, reaching a
    // program outside the declared bare name (the manifest forbids paths).
    #[cfg(unix)]
    #[test]
    fn resolve_binary_never_resolves_a_path() {
        let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("tempdir");
        let bin = root.path().join("bin");
        std::fs::create_dir(&bin).expect("mkdir");
        let tool = program(&bin, "tool", 0o755);
        for name in [
            tool.to_string_lossy().to_string(),
            "bin/tool".to_string(),
            "../bin/tool".to_string(),
            "..".to_string(),
            String::new(),
        ] {
            assert_eq!(
                resolve_binary_in(&name, [root.path().to_path_buf()]),
                None,
                "{name}"
            );
        }
        assert_eq!(
            resolve_binary_in("tool", [std::path::PathBuf::from("relative/bin")]),
            None
        );
    }

    // Catches: Windows resolution that looks only for `.exe`, missing the
    // `.cmd` shims that npm and version managers install.
    #[cfg(windows)]
    #[test]
    fn resolve_binary_applies_windows_program_suffixes() {
        let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("tempdir");
        let (cmd_dir, exe_dir) = (root.path().join("cmd"), root.path().join("exe"));
        std::fs::create_dir(&cmd_dir).expect("mkdir");
        std::fs::create_dir(&exe_dir).expect("mkdir");
        std::fs::write(cmd_dir.join("tool.cmd"), "@echo off\r\n").expect("write");
        std::fs::write(exe_dir.join("tool.exe"), "").expect("write");
        std::fs::write(exe_dir.join("tool"), "").expect("write");

        let found = resolve_binary_in("tool", [cmd_dir.clone(), exe_dir.clone()]);
        assert_eq!(
            found,
            Some(cmd_dir.join("tool.cmd").to_string_lossy().to_string())
        );
        let found = resolve_binary_in("tool", [exe_dir.clone()]);
        assert_eq!(
            found,
            Some(exe_dir.join("tool.exe").to_string_lossy().to_string())
        );
    }

    #[test]
    fn validate_cwd_rejects_relative() {
        assert!(validate_cwd("relative/path").is_err());
    }

    async fn run_host_shell(script: &str, cwd: &str) -> Result<String, String> {
        let (shell, flag) = crate::test_support::host_shell();
        run_cli(
            "shell",
            shell,
            &[flag.to_string(), script.to_string()],
            Some(cwd.to_string()),
            "test-plugin",
            DEFAULT_EXEC_TIMEOUT,
            &crate::secrets::SecretStore::default(),
        )
        .await
    }

    // Catches: the owner cannot run a declared plugin tool in an external checkout.
    #[tokio::test]
    async fn plugin_external_cwd_is_used() {
        let outside = crate::test_support::dir_outside_home();
        let print_cwd = if cfg!(windows) { "cd" } else { "pwd -P" };
        let stdout = run_host_shell(print_cwd, &outside.to_string_lossy())
            .await
            .expect("an existing absolute folder outside home is accepted");
        let child_cwd = std::path::PathBuf::from(stdout.trim())
            .canonicalize()
            .expect("child cwd exists");
        assert_eq!(child_cwd, outside.canonicalize().expect("outside dir"));
    }

    // Catches: malformed external input selects an unintended fallback working directory.
    #[tokio::test]
    async fn plugin_relative_missing_or_file_cwd_never_spawns() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("tempdir");
        let marker = dir.path().join("spawned");
        let file = dir.path().join("plain-file");
        std::fs::write(&file, b"x").expect("write file");
        let missing = dir.path().join("missing");
        let script = format!("echo spawned > \"{}\"", marker.display());
        for (cwd, expected) in [
            ("relative/path".to_string(), "must be an absolute path"),
            (
                missing.to_string_lossy().into_owned(),
                "Failed to resolve working directory",
            ),
            (file.to_string_lossy().into_owned(), "Failed to execute"),
        ] {
            let error = run_host_shell(&script, &cwd)
                .await
                .expect_err(&format!("{cwd} must be refused"));
            assert!(error.contains(expected), "{cwd}: {error}");
            assert!(!marker.exists(), "{cwd}: child was spawned");
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
            None,
            &allowed,
            &crate::secrets::SecretStore::default(),
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
            None,
            &allowed,
            &crate::secrets::SecretStore::default(),
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
                None,
                &allowed,
                &crate::secrets::SecretStore::default(),
            )
            .await;
            assert!(r.is_err());
            assert!(r.unwrap_err().contains("not found"));
        }
    }

    // Catches: a declared owner tool that lives in a system PATH folder
    // (`/usr/bin`) is not found, because resolution searches only a fixed list
    // of install folders instead of the backend PATH.
    #[cfg(unix)]
    #[tokio::test]
    async fn plugin_declared_program_resolves_from_path() {
        assert!(
            ["/usr/bin/uname", "/bin/uname"]
                .iter()
                .any(|p| std::path::Path::new(p).is_file()),
            "fixture: uname must be installed in a system folder"
        );
        let output = plugin_exec_cli_inner(
            "uname".to_string(),
            vec![],
            None,
            "path-probe".to_string(),
            None,
            &["uname".to_string()],
            &crate::secrets::SecretStore::default(),
        )
        .await
        .expect("a declared tool on PATH must run");
        assert!(!output.trim().is_empty(), "uname printed nothing");
    }

    // Catches: an untrusted request runs a program the installed manifest does
    // not declare, either by another name that PATH resolves or by an explicit
    // path to a declared name.
    #[cfg(unix)]
    #[tokio::test]
    async fn plugin_undeclared_program_never_spawns() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("tempdir");
        let marker = dir.path().join("spawned");
        let marker_arg = marker.to_string_lossy().to_string();
        let touch = ["/usr/bin/touch", "/bin/touch"]
            .into_iter()
            .find(|p| std::path::Path::new(p).is_file())
            .expect("fixture: touch must be installed in a system folder");
        let requests = [
            ("touch", vec!["mdkb".to_string()]),
            (touch, vec!["touch".to_string()]),
        ];
        for (binary, declared) in requests {
            let error = plugin_exec_cli_inner(
                binary.to_string(),
                vec![marker_arg.clone()],
                None,
                "undeclared-probe".to_string(),
                None,
                &declared,
                &crate::secrets::SecretStore::default(),
            )
            .await
            .expect_err("an undeclared program must be refused");
            assert!(error.contains("not declared in plugin"), "{error}");
            assert!(!marker.exists(), "{binary} was spawned");
        }
    }

    // Catches: a declared program found in a user install folder runs with the
    // launcher's minimal PATH, so a `#!/usr/bin/env node` script (npm, pnpm and
    // version-manager shims) resolves, then fails to start its interpreter in a
    // release build.
    #[cfg(unix)]
    #[tokio::test]
    async fn plugin_program_runs_with_the_path_it_was_resolved_from() {
        let home = dirs::home_dir().expect("home");
        let output = plugin_exec_cli_inner(
            "printenv".to_string(),
            vec!["PATH".to_string()],
            None,
            "path-env-probe".to_string(),
            None,
            &["printenv".to_string()],
            &crate::secrets::SecretStore::default(),
        )
        .await
        .expect("printenv must run");
        let child: Vec<_> = std::env::split_paths(output.trim()).collect();
        for folder in [
            home.join(".cargo").join("bin"),
            home.join(".local").join("bin"),
            home.join(".npm-global").join("bin"),
            home.join("go").join("bin"),
        ] {
            assert!(
                child.contains(&folder),
                "{} missing from the child PATH: {}",
                folder.display(),
                output.trim()
            );
        }
    }

    fn store_with_secret(secret: &str) -> crate::secrets::SecretStore {
        use crate::secrets::{Field, FieldKind, Form, Submission};
        let store = crate::secrets::SecretStore::default();
        let form = Form::request(
            vec![Field {
                name: "TOKEN".into(),
                kind: FieldKind::Password,
                display: None,
            }],
            "test".into(),
        )
        .expect("form");
        let form = store.open(form).expect("open");
        store
            .submit(
                &form.nonce,
                Submission {
                    nonce: String::new(),
                    status: "stored".into(),
                    values: std::collections::BTreeMap::from([("TOKEN".into(), secret.into())]),
                    template: None,
                },
            )
            .expect("submit");
        store
    }

    /// Script that prints `content` on stderr and exits with `code`, on any host shell.
    fn stderr_file_script(dir: &std::path::Path, content: &[u8], code: i32) -> String {
        let file = dir.join("stderr.txt");
        std::fs::write(&file, content).expect("write stderr fixture");
        let print = tuic_test_support::print_file_script(file.to_str().expect("path"));
        if cfg!(windows) {
            format!("{print} 1>&2& exit /b {code}")
        } else {
            format!("{print} >&2; exit {code}")
        }
    }

    /// Run `script` to failure through the real capture path and return the error text.
    async fn failure_text(script: &str, secrets: &crate::secrets::SecretStore) -> String {
        let mut child = output_child(script);
        let (status, _stdout, stderr) =
            capture_cli_output(&mut child, "shell", Duration::from_secs(30))
                .await
                .expect("capture");
        assert!(!status.success());
        failure_message("shell", status.code(), &stderr, &secrets.masker())
    }

    // Catches: SecretStore::mask holding the store mutex while it masks, so one large
    // failing-command stderr stalls every other secret operation.
    #[test]
    fn large_mask_does_not_block_other_secret_operations() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let store = std::sync::Arc::new(store_with_secret("hunter2-secret-value"));
        let started = std::sync::Arc::new(AtomicBool::new(false));
        let big = "error: build failed with a long line of compiler output\n".repeat(20_000);
        let worker = {
            let (store, started) = (store.clone(), started.clone());
            std::thread::spawn(move || {
                started.store(true, Ordering::SeqCst);
                store.mask(&big)
            })
        };
        while !started.load(Ordering::SeqCst) {
            std::thread::yield_now();
        }
        std::thread::sleep(Duration::from_millis(200));
        assert!(
            !worker.is_finished(),
            "fixture too small: the large mask finished before the probe"
        );
        assert_eq!(store.mask("token hunter2-secret-value"), "token [REDACTED]");
        assert!(
            !worker.is_finished(),
            "second secret operation only completed after the large mask finished"
        );
        worker.join().expect("mask thread");
    }

    // Catches: owner loses the actionable compiler/login error near the end of stderr.
    #[tokio::test]
    async fn plugin_error_tail_after_256_bytes_is_returned() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("tempdir");
        let mut content = vec![b'x'; 1000];
        content.extend_from_slice(b" FINAL-ERROR-LINE");
        let script = stderr_file_script(dir.path(), &content, 3);
        let text = failure_text(&script, &crate::secrets::SecretStore::default()).await;
        assert!(text.contains("exited with code 3"), "{text}");
        assert!(text.ends_with("FINAL-ERROR-LINE"), "{text}");
        assert!(!text.contains("truncated"), "{text}");
    }

    // Catches: untrusted command output leaks a registered secret, or stderr is retained without bound.
    // Masking cost is O(needles x bytes), so the secret is checked on a small stream and the
    // bound on a 6 MB one; the shared masker is not re-measured at 5 MiB under the O0 test profile.
    #[tokio::test]
    async fn plugin_error_masks_known_secret_and_bounds_capture() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("tempdir");
        let mut small = b"login failed for token hunter2-secret-value\n".to_vec();
        small.resize(20_000, b'y');
        let script = stderr_file_script(dir.path(), &small, 1);
        let secrets = store_with_secret("hunter2-secret-value");
        let text = failure_text(&script, &secrets).await;
        assert!(!text.contains("hunter2-secret-value"), "secret leaked");
        assert!(text.contains("login failed for token"), "{}", &text[..80]);

        let big = vec![b'y'; 6_000_000];
        let script = stderr_file_script(dir.path(), &big, 1);
        let text = failure_text(&script, &crate::secrets::SecretStore::default()).await;
        assert!(text.ends_with(STDERR_TRUNCATION_MARK), "no truncation mark");
        assert!(
            text.len() <= MAX_STDERR_BYTES + 200,
            "unbounded: {}",
            text.len()
        );
    }

    // Catches: off-by-one at the bound — exact limit marked truncated, or one byte over unmarked.
    #[tokio::test]
    async fn stderr_capture_marks_truncation_only_above_the_limit() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("tempdir");
        for (extra, truncated) in [(0usize, false), (1, true)] {
            let content = vec![b'z'; MAX_STDERR_BYTES + extra];
            let script = stderr_file_script(dir.path(), &content, 0);
            let mut child = output_child(&script);
            let (_, _, stderr) = capture_cli_output(&mut child, "shell", Duration::from_secs(30))
                .await
                .expect("capture");
            assert_eq!(stderr.bytes.len(), MAX_STDERR_BYTES);
            assert_eq!(stderr.truncated, truncated);
        }
    }

    // Catches: a cut inside a multi-byte character panicking or corrupting the message.
    #[test]
    fn stderr_cut_inside_utf8_character_still_yields_text() {
        let mut bytes = "é".repeat(10).into_bytes();
        bytes.pop();
        let stderr = CapturedStderr {
            bytes,
            truncated: true,
        };
        let text = failure_message(
            "b",
            Some(1),
            &stderr,
            &crate::secrets::SecretStore::default().masker(),
        );
        assert!(
            text.starts_with("Command \"b\" exited with code 1: éééééééé"),
            "{text}"
        );
        assert!(text.ends_with(STDERR_TRUNCATION_MARK));
    }

    fn budget_command(script: &str) -> Command {
        let (shell, flag) = crate::test_support::host_shell();
        let mut cmd = Command::new(shell);
        cmd.args([flag, script])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        cmd
    }

    /// A long command that marks the moment it starts; on unix the marker holds its pid.
    fn holder_command(marker: &std::path::Path) -> Command {
        let marker = marker.to_str().expect("marker path");
        if cfg!(windows) {
            budget_command(&tuic_test_support::chain(
                &tuic_test_support::touch_script(marker),
                &crate::test_support::sleep_script(),
            ))
        } else {
            budget_command(&format!("echo $$ > {marker}; exec sleep 60"))
        }
    }

    async fn wait_for_file(path: &std::path::Path) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !path.exists() {
            assert!(Instant::now() < deadline, "setup: {path:?} never appeared");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    // Catches: an owner indexing or build command held to the fixed 30 s deadline,
    // or a cheap serial batch refused by a calls-per-minute quota.
    #[tokio::test]
    async fn plugin_raised_timeout_and_cheap_serial_burst_work() {
        assert_eq!(exec_timeout(None), Ok(DEFAULT_EXEC_TIMEOUT));
        assert_eq!(DEFAULT_EXEC_TIMEOUT, Duration::from_secs(30));
        assert_eq!(
            exec_timeout(Some(10 * 60 * 1000)),
            Ok(Duration::from_secs(600))
        );
        let ceiling_ms = u64::try_from(MAX_EXEC_TIMEOUT.as_millis()).expect("ceiling");
        assert_eq!(exec_timeout(Some(ceiling_ms)), Ok(MAX_EXEC_TIMEOUT));

        // The requested deadline, not the default, governs the child.
        let requested = exec_timeout(Some(200)).expect("valid override");
        let sleeper = budget_command(&crate::test_support::sleep_script());
        let error = tokio::time::timeout(
            Duration::from_secs(10),
            run_budgeted("budget-raised", "shell", sleeper, requested),
        )
        .await
        .expect("the requested deadline was ignored")
        .expect_err("the sleeper must hit its deadline");
        assert!(error.contains("timed out after 0.2s"), "{error}");

        for call in 0..150 {
            let (status, _, _) = run_budgeted(
                "budget-burst",
                "shell",
                budget_command("exit 0"),
                Duration::from_secs(30),
            )
            .await
            .unwrap_or_else(|error| panic!("serial call {call} refused: {error}"));
            assert!(status.success(), "serial call {call}: {status}");
        }
    }

    // Catches: unbounded concurrent commands per plugin, a queued call whose deadline
    // runs while it waits, a cancelled call that keeps its child or its slot, an
    // infinite or zero override, and output past the cap.
    #[tokio::test]
    async fn plugin_concurrency_deadline_cancel_and_output_stay_bounded() {
        let ceiling_ms = u64::try_from(MAX_EXEC_TIMEOUT.as_millis()).expect("ceiling");
        assert!(
            exec_timeout(Some(0)).is_err(),
            "a zero deadline was accepted"
        );
        assert!(
            exec_timeout(Some(ceiling_ms + 1)).is_err(),
            "a deadline past the ceiling was accepted"
        );
        assert!(
            exec_timeout(Some(u64::MAX)).is_err(),
            "an effectively infinite deadline was accepted"
        );

        let plugin = "budget-concurrency";
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("tempdir");
        let mut holders = Vec::new();
        let mut markers = Vec::new();
        for slot in 0..MAX_CONCURRENT_EXECS {
            let marker = dir.path().join(format!("holder-{slot}"));
            let cmd = holder_command(&marker);
            holders.push(tokio::spawn(async move {
                run_budgeted(plugin, "shell", cmd, Duration::from_secs(60)).await
            }));
            markers.push(marker);
        }
        for marker in &markers {
            wait_for_file(marker).await;
        }

        // A quick call beyond the bound waits; its own short deadline is not running yet.
        let queued = tokio::spawn(async move {
            run_budgeted(
                plugin,
                "shell",
                budget_command("exit 0"),
                Duration::from_millis(300),
            )
            .await
        });
        // The bound is per plugin: another plugin still runs at once.
        tokio::time::timeout(
            Duration::from_secs(10),
            run_budgeted(
                "budget-other",
                "shell",
                budget_command("exit 0"),
                Duration::from_secs(30),
            ),
        )
        .await
        .expect("another plugin was blocked by this plugin's bound")
        .expect("another plugin's call");
        tokio::time::sleep(Duration::from_millis(600)).await;
        assert!(
            !queued.is_finished(),
            "a call beyond the concurrency bound ran (or timed out) while every slot was held"
        );

        // Cancelling a holder releases its slot and stops its child.
        holders[0].abort();
        assert!(
            holders
                .remove(0)
                .await
                .expect_err("cancelled")
                .is_cancelled()
        );
        let (status, _, _) = tokio::time::timeout(Duration::from_secs(10), queued)
            .await
            .expect("the cancelled call kept its slot")
            .expect("queued task")
            .expect("the queued call must run with its full deadline after it gets a slot");
        assert!(status.success());
        #[cfg(unix)]
        {
            let pid = std::fs::read_to_string(&markers[0]).expect("pid");
            let pid = pid.trim();
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                let stat = std::process::Command::new("ps")
                    .args(["-o", "stat=", "-p", pid])
                    .output()
                    .expect("ps");
                let stat = String::from_utf8_lossy(&stat.stdout);
                if stat.trim().is_empty() || stat.trim_start().starts_with('Z') {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "cancelled child {pid} still runs"
                );
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }

        // Output past the cap fails and gives its slot back.
        let overflow = dir.path().join("overflow");
        std::fs::write(&overflow, vec![b'x'; MAX_STDOUT_BYTES + 1]).expect("write output");
        let error = run_budgeted(
            plugin,
            "shell",
            budget_command(&tuic_test_support::print_file_script(
                overflow.to_str().expect("path"),
            )),
            Duration::from_secs(30),
        )
        .await
        .expect_err("overflow");
        assert!(error.contains("exceeds maximum size"), "{error}");
        tokio::time::timeout(
            Duration::from_secs(10),
            run_budgeted(
                plugin,
                "shell",
                budget_command("exit 0"),
                Duration::from_secs(30),
            ),
        )
        .await
        .expect("a failed call kept its slot")
        .expect("call after overflow");

        for holder in holders {
            holder.abort();
        }
    }

    // Catches: an uninstalled plugin's execution slots lingering for the process lifetime.
    #[tokio::test]
    async fn clear_exec_slots_drops_entry() {
        let id = "budget-clear";
        run_budgeted(id, "shell", budget_command("exit 0"), DEFAULT_EXEC_TIMEOUT)
            .await
            .expect("call");
        assert!(exec_slots().contains_key(id));
        clear_exec_slots(id);
        assert!(!exec_slots().contains_key(id));
    }
}
