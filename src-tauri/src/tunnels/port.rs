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
/// Uses `lsof` to find PIDs, then verifies each is an `ssh` process before sending SIGTERM.
pub async fn kill_ssh_on_port(port: u16) {
    let output = tokio::process::Command::new("lsof")
        .args(["-ti", &format!("tcp:{port}"), "-sTCP:LISTEN"])
        .output()
        .await;

    let Ok(output) = output else { return };
    let pids = String::from_utf8_lossy(&output.stdout);
    for pid_str in pids.split_whitespace() {
        let Ok(pid) = pid_str.parse::<i32>() else {
            continue;
        };
        let ps = tokio::process::Command::new("ps")
            .args(["-p", pid_str, "-o", "comm="])
            .output()
            .await;
        let is_ssh = ps
            .as_ref()
            .map(|o| {
                let comm = String::from_utf8_lossy(&o.stdout);
                comm.trim().ends_with("ssh")
            })
            .unwrap_or(false);
        if is_ssh {
            unsafe {
                libc::kill(pid, libc::SIGTERM);
            }
        }
    }
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

    #[tokio::test]
    async fn check_local_port_returns_err_when_bound() {
        let addr: SocketAddr = ([127, 0, 0, 1], 0).into();
        let listener = TcpListener::bind(addr).await.unwrap();
        let port = listener.local_addr().unwrap().port();

        assert!(check_local_port(port).await.is_err());
    }

    #[tokio::test]
    async fn check_local_port_returns_ok_for_free_port() {
        let addr: SocketAddr = ([127, 0, 0, 1], 0).into();
        let listener = TcpListener::bind(addr).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        assert!(check_local_port(port).await.is_ok());
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
