use std::net::SocketAddr;
use tokio::net::TcpListener;

/// Check if a local port is available by attempting to bind on 127.0.0.1.
/// Returns Ok(()) if available, Err(message) explaining why not.
pub async fn check_local_port(port: u16) -> Result<(), String> {
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    match TcpListener::bind(addr).await {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => Err(format!(
            "local port {port} requires elevated privileges (ports < 1024 need root)"
        )),
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            Err(format!("local port {port} is already in use"))
        }
        Err(e) => Err(format!("local port {port} unavailable: {e}")),
    }
}

/// Kill orphaned SSH processes holding a local port.
/// Uses `lsof` to find PIDs, then verifies each is an `ssh` process (natively,
/// see [`is_ssh_process`]) before sending SIGTERM.
#[cfg(unix)]
pub async fn kill_ssh_on_port(port: u16) {
    let output = tokio::process::Command::new("lsof")
        .args(["-ti", &format!("tcp:{port}"), "-sTCP:LISTEN"])
        .output()
        .await;

    let Ok(output) = output else { return };
    terminate_ssh_pids(&String::from_utf8_lossy(&output.stdout));
}

/// SIGTERM every pid in `lsof -t` output that is an `ssh` process; anything
/// else holding the port is left alone. Returns the pids signalled.
#[cfg(unix)]
fn terminate_ssh_pids(lsof_pids: &str) -> Vec<u32> {
    lsof_pids
        .split_whitespace()
        .filter_map(|pid| pid.parse::<u32>().ok())
        .filter(|&pid| is_ssh_process(pid))
        .filter(|&pid| tuic_core::process_info::terminate(pid))
        .collect()
}

/// Whether `pid` runs an `ssh` binary, by executable path or kernel name.
/// Replaces `ps -p <pid> -o comm=`, which a sandboxed host refuses to exec —
/// there the check read "not ssh" for everything and the orphan survived.
#[cfg(unix)]
fn is_ssh_process(pid: u32) -> bool {
    // The executable path when it is readable; the kernel name only as a fallback. On
    // macOS `comm` is cut at 16 bytes, so `xxxxxxxxxxxxxssh-agent-helper` reads
    // `xxxxxxxxxxxxxssh`: trusting it over a readable path would SIGTERM an unrelated listener.
    match tuic_core::process_info::exe(pid) {
        Some(exe) => names_ssh(&exe.to_string_lossy()),
        None => tuic_core::process_info::info(pid).is_some_and(|process| names_ssh(&process.comm)),
    }
}

/// The `ends_with("ssh")` rule the `ps comm` check applied.
#[cfg(unix)]
fn names_ssh(name: &str) -> bool {
    name.trim().ends_with("ssh")
}

/// Find a free port on 127.0.0.1 by letting the OS assign one.
/// Returns the assigned port number.
pub async fn find_free_port() -> std::io::Result<u16> {
    let addr: SocketAddr = ([127, 0, 0, 1], 0).into();
    let listener = TcpListener::bind(addr).await?;
    Ok(listener.local_addr()?.port())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn ssh_detection_reads_the_process_natively() {
        assert!(names_ssh("/usr/bin/ssh"));
        assert!(names_ssh("ssh"));
        assert!(!names_ssh("/bin/sleep"));
        let mut sleeper = std::process::Command::new("/bin/sleep")
            .arg("30")
            .spawn()
            .expect("spawn sleep");
        let is_ssh = is_ssh_process(sleeper.id());
        let _ = sleeper.kill();
        let _ = sleeper.wait();
        assert!(!is_ssh, "a sleep is not an ssh");
    }

    /// The positive half: an `ssh` among the port's holders is
    /// terminated, a non-ssh holder and junk lines are not. The `ssh` is the
    /// identity probe installed under that name (a copied system binary would
    /// be SIGKILLed by the sandbox), blocked reading a pipe nobody writes.
    #[cfg(unix)]
    #[test]
    fn an_ssh_holding_the_port_is_terminated_and_nothing_else() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let spawn = |name: &str| {
            let probe = tuic_test_support::install_identity_probe(dir.path(), name);
            std::process::Command::new(probe)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::null())
                .spawn()
                .expect("spawn the identity probe")
        };
        let mut ssh = spawn("ssh");
        let mut other = spawn("not-an-ssh-daemon");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !is_ssh_process(ssh.id()) {
            assert!(
                std::time::Instant::now() < deadline,
                "the ssh probe never showed up"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let signalled = terminate_ssh_pids(&format!("{}\n{}\nnot-a-pid\n", ssh.id(), other.id()));

        assert_eq!(signalled, vec![ssh.id()]);
        let status = ssh.wait().expect("reap the ssh probe");
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(status.signal(), Some(libc::SIGTERM), "{status:?}");
        assert!(
            matches!(other.try_wait(), Ok(None)),
            "a non-ssh holder must survive"
        );
        let _ = other.kill();
        let _ = other.wait();
    }

    #[tokio::test]
    async fn check_local_port_returns_err_when_bound() {
        let addr: SocketAddr = ([127, 0, 0, 1], 0).into();
        let listener = TcpListener::bind(addr).await.unwrap();
        let port = listener.local_addr().unwrap().port();

        assert!(check_local_port(port).await.is_err());
    }

    #[tokio::test]
    async fn check_local_port_returns_ok_for_free_port() {
        // Catches: an available bind being reported as occupied. Let the OS
        // choose the free port inside the check, rather than release one first.
        assert_eq!(check_local_port(0).await, Ok(()));
    }

    #[tokio::test]
    async fn find_free_port_returns_valid_port() {
        let port = find_free_port().await.unwrap();
        assert!(port > 0);
    }

    #[tokio::test]
    async fn find_free_port_returns_different_ports() {
        let port1 = find_free_port().await.unwrap();
        let port2 = find_free_port().await.unwrap();
        assert_ne!(port1, port2);
    }
}
