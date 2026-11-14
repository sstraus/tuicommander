//! Exec-probe for executables the kernel may refuse to start.
//!
//! macOS kills a new exec of a binary whose code-signature check fails (SIGKILL,
//! "Code Signature Invalid") after an outside tool rewrote its file metadata,
//! even though the bytes and `codesign -v` are valid. Copying the file to a new
//! inode clears the cached verdict (#2902-dcc7, #2129-1dfd). Running processes
//! keep working, so only the next MCP session notices.

use std::path::Path;
#[cfg(feature = "desktop")]
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Longest a probe waits: a process still alive after this was started by the kernel.
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

/// Arguments that make each shipped executable return quickly without side effects.
pub(crate) const BRIDGE_ARGS: &[&str] = &[];
#[cfg(feature = "desktop")]
pub(crate) const CLI_ARGS: &[&str] = &["--version"];
#[cfg(feature = "desktop")]
pub(crate) const REMOTE_ARGS: &[&str] = &["--build-info"];

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Probe {
    /// The process ran, or was still running when the probe stopped waiting.
    Started,
    /// The kernel or a signal ended the process: exit 137 or termination by signal.
    Killed(String),
    /// Not spawnable at all (missing, not a native executable). Not this probe's concern.
    Unrunnable,
}

fn classify(status: std::process::ExitStatus) -> Probe {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return Probe::Killed(format!("signal {signal}"));
        }
    }
    match status.code() {
        Some(137) => Probe::Killed("exit 137".into()),
        _ => Probe::Started,
    }
}

pub(crate) fn probe(path: &Path, args: &[&str]) -> Probe {
    let Ok(mut child) = Command::new(path)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return Probe::Unrunnable;
    };
    let deadline = Instant::now() + PROBE_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return classify(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Probe::Started;
            }
        }
    }
}

/// Replace `path` by a byte-identical copy on a new inode, atomically.
fn republish_fresh_inode(path: &Path) -> std::io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| std::io::Error::other("executable has no parent directory"))?;
    let temp = tempfile::Builder::new()
        .prefix(".republish-")
        .tempfile_in(dir)?;
    // `fs::copy` carries the permission bits of the source onto the temp file.
    std::fs::copy(path, temp.path())?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

/// Probe `path`; when the kernel killed it, republish it on a new inode and log once.
/// Returns true when a republish happened.
pub(crate) fn heal(path: &Path, args: &[&str]) -> bool {
    let Probe::Killed(reason) = probe(path, args) else {
        return false;
    };
    match republish_fresh_inode(path) {
        Ok(()) => {
            tracing::warn!(
                source = "exec_probe",
                path = %path.display(),
                "Executable was killed on exec ({reason}); republished it on a new inode"
            );
            true
        }
        Err(error) => {
            tracing::warn!(
                source = "exec_probe",
                path = %path.display(),
                "Executable was killed on exec ({reason}); republish failed: {error}"
            );
            false
        }
    }
}

/// Heal every executable TUIC published or shipped: the installed bridge
/// revisions, the `tuic` CLI and the siblings of the running executable.
#[cfg(feature = "desktop")]
pub(crate) fn heal_startup() {
    if crate::agent_test::active() {
        return;
    }
    for bridge in crate::agent_mcp::installed_bridge_paths() {
        heal(&bridge, BRIDGE_ARGS);
    }
    if let Some(cli) = crate::tuic_cli::installed_cli_path() {
        heal(&cli, CLI_ARGS);
    }
    let beside: Option<PathBuf> = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    if let Some(dir) = beside {
        let suffix = std::env::consts::EXE_SUFFIX;
        for (name, args) in [
            ("tuic-bridge", BRIDGE_ARGS),
            ("tuic", CLI_ARGS),
            ("tuic-remote", REMOTE_ARGS),
        ] {
            let candidate = dir.join(format!("{name}{suffix}"));
            if candidate.is_file() {
                heal(&candidate, args);
            }
        }
    }
}
