//! Tailscale detection and TLS certificate provisioning.
//!
//! Detects Tailscale daemon status, FQDN, and HTTPS capability.
//! Provisions TLS certificates via the Tailscale Local API.

use serde::Deserialize;
use std::path::PathBuf;

/// Tailscale daemon state detected at runtime.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(tag = "state")]
pub(crate) enum TailscaleState {
    /// Tailscale binary not found on this system.
    NotInstalled,
    /// Binary found but daemon is not running or unreachable.
    NotRunning,
    /// Daemon running. FQDN and HTTPS availability reported.
    Running {
        fqdn: String,
        https_enabled: bool,
    },
}

/// Subset of `tailscale status --json` we care about.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct StatusJson {
    backend_state: Option<String>,
    #[serde(rename = "Self")]
    self_node: Option<SelfNode>,
    cert_domains: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SelfNode {
    #[serde(rename = "DNSName")]
    dns_name: String,
}

/// Find the Tailscale CLI binary path.
pub(crate) fn find_binary() -> Option<PathBuf> {
    // Try PATH first (works on all platforms if installed properly)
    if let Ok(output) = std::process::Command::new(if cfg!(windows) { "where" } else { "which" })
        .arg("tailscale")
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(PathBuf::from(path));
            }
        }
    }

    // Platform-specific fallback locations
    #[cfg(target_os = "macos")]
    {
        let app_path = PathBuf::from("/Applications/Tailscale.app/Contents/MacOS/Tailscale");
        if app_path.exists() {
            return Some(app_path);
        }
    }

    #[cfg(target_os = "windows")]
    {
        let prog = PathBuf::from(r"C:\Program Files\Tailscale\tailscale.exe");
        if prog.exists() {
            return Some(prog);
        }
        let prog86 = PathBuf::from(r"C:\Program Files (x86)\Tailscale\tailscale.exe");
        if prog86.exists() {
            return Some(prog86);
        }
    }

    None
}

/// Detect Tailscale daemon state by running `tailscale status --json`.
pub(crate) fn detect() -> TailscaleState {
    let binary = match find_binary() {
        Some(b) => b,
        None => return TailscaleState::NotInstalled,
    };

    let output = match std::process::Command::new(&binary)
        .args(["status", "--json"])
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            tracing::warn!(source = "tailscale", "Failed to run tailscale CLI: {e}");
            return TailscaleState::NotRunning;
        }
    };

    if !output.status.success() {
        return TailscaleState::NotRunning;
    }

    parse_status_json(&output.stdout)
}

/// Parse the JSON output from `tailscale status --json`.
pub(crate) fn parse_status_json(json_bytes: &[u8]) -> TailscaleState {
    let status: StatusJson = match serde_json::from_slice(json_bytes) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(source = "tailscale", "Failed to parse status JSON: {e}");
            return TailscaleState::NotRunning;
        }
    };

    // Check backend state
    match status.backend_state.as_deref() {
        Some("Running") => {}
        _ => return TailscaleState::NotRunning,
    }

    // Extract FQDN from Self.DNSName, strip trailing dot
    let fqdn = match status.self_node {
        Some(node) => strip_trailing_dot(&node.dns_name),
        None => return TailscaleState::NotRunning,
    };

    // Check if HTTPS certs are available
    let https_enabled = status
        .cert_domains
        .as_ref()
        .is_some_and(|domains| !domains.is_empty());

    TailscaleState::Running {
        fqdn,
        https_enabled,
    }
}

/// Strip trailing dot from DNS name (e.g. "host.ts.net." → "host.ts.net").
fn strip_trailing_dot(name: &str) -> String {
    name.strip_suffix('.').unwrap_or(name).to_string()
}

/// Provision a TLS certificate from the Tailscale Local API.
///
/// Returns (cert_pem, key_pem) as byte vectors.
/// Uses platform-specific transport:
/// - Unix: HTTP over Unix socket at /var/run/tailscale/tailscaled.sock
/// - Windows: `tailscale cert` CLI (writes to temp files, reads back)
pub(crate) async fn provision_cert(fqdn: &str) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
    #[cfg(unix)]
    {
        provision_cert_unix(fqdn).await
    }
    #[cfg(windows)]
    {
        provision_cert_cli(fqdn).await
    }
}

/// Provision cert via Tailscale Local API over Unix socket.
#[cfg(unix)]
async fn provision_cert_unix(fqdn: &str) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixStream;

    let socket_path = "/var/run/tailscale/tailscaled.sock";
    let mut stream = UnixStream::connect(socket_path).await.map_err(|e| {
        anyhow::anyhow!("Cannot connect to Tailscale socket at {socket_path}: {e}")
    })?;

    // Request cert PEM
    let cert_req = format!(
        "GET /localapi/v0/cert/{fqdn}?type=cert HTTP/1.1\r\nHost: local-tailscaled.sock\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(cert_req.as_bytes()).await?;
    stream.shutdown().await?;

    let mut cert_response = Vec::new();
    stream.read_to_end(&mut cert_response).await?;
    let cert_pem = extract_http_body(&cert_response)?;

    // New connection for key
    let mut stream = UnixStream::connect(socket_path).await?;
    let key_req = format!(
        "GET /localapi/v0/cert/{fqdn}?type=key HTTP/1.1\r\nHost: local-tailscaled.sock\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(key_req.as_bytes()).await?;
    stream.shutdown().await?;

    let mut key_response = Vec::new();
    stream.read_to_end(&mut key_response).await?;
    let key_pem = extract_http_body(&key_response)?;

    Ok((cert_pem, key_pem))
}

/// Provision cert via `tailscale cert` CLI (cross-platform fallback).
#[cfg(windows)]
async fn provision_cert_cli(fqdn: &str) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
    let binary = find_binary().ok_or_else(|| anyhow::anyhow!("Tailscale binary not found"))?;

    let temp_dir = std::env::temp_dir().join("tuicommander-certs");
    tokio::fs::create_dir_all(&temp_dir).await?;

    let cert_path = temp_dir.join(format!("{fqdn}.crt"));
    let key_path = temp_dir.join(format!("{fqdn}.key"));

    let output = tokio::process::Command::new(&binary)
        .args([
            "cert",
            "--cert-file",
            cert_path.to_str().unwrap_or_default(),
            "--key-file",
            key_path.to_str().unwrap_or_default(),
            fqdn,
        ])
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("tailscale cert failed: {stderr}");
    }

    let cert_pem = tokio::fs::read(&cert_path).await?;
    let key_pem = tokio::fs::read(&key_path).await?;

    // Clean up temp files (best effort)
    let _ = tokio::fs::remove_file(&cert_path).await;
    let _ = tokio::fs::remove_file(&key_path).await;

    Ok((cert_pem, key_pem))
}

/// Extract HTTP response body from raw HTTP/1.1 response bytes.
#[cfg(unix)]
fn extract_http_body(response: &[u8]) -> anyhow::Result<Vec<u8>> {
    // Find the \r\n\r\n separator between headers and body
    let separator = b"\r\n\r\n";
    let pos = response
        .windows(4)
        .position(|w| w == separator)
        .ok_or_else(|| anyhow::anyhow!("Malformed HTTP response: no header/body separator"))?;

    let headers = String::from_utf8_lossy(&response[..pos]);

    // Check for non-200 status
    if let Some(status_line) = headers.lines().next() {
        if !status_line.contains("200") {
            anyhow::bail!("Tailscale Local API error: {status_line}");
        }
    }

    Ok(response[pos + 4..].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUNNING_STATUS: &str = r#"{
        "Version": "1.94.1",
        "BackendState": "Running",
        "Self": {
            "DNSName": "myhost.tail-abc123.ts.net.",
            "HostName": "myhost",
            "OS": "macOS",
            "Online": true,
            "TailscaleIPs": ["100.80.90.53"]
        },
        "CertDomains": ["myhost.tail-abc123.ts.net"]
    }"#;

    const RUNNING_NO_HTTPS: &str = r#"{
        "Version": "1.94.1",
        "BackendState": "Running",
        "Self": {
            "DNSName": "dgqt92cjfp.tail911da.ts.net.",
            "HostName": "DGQT92CJFP",
            "OS": "macOS",
            "Online": true,
            "TailscaleIPs": ["100.80.90.53"]
        },
        "CertDomains": null
    }"#;

    const STOPPED_STATUS: &str = r#"{
        "Version": "1.94.1",
        "BackendState": "Stopped"
    }"#;

    #[test]
    fn parse_status_running_with_https() {
        let state = parse_status_json(RUNNING_STATUS.as_bytes());
        assert_eq!(
            state,
            TailscaleState::Running {
                fqdn: "myhost.tail-abc123.ts.net".to_string(),
                https_enabled: true,
            }
        );
    }

    #[test]
    fn parse_status_running_no_https() {
        let state = parse_status_json(RUNNING_NO_HTTPS.as_bytes());
        assert_eq!(
            state,
            TailscaleState::Running {
                fqdn: "dgqt92cjfp.tail911da.ts.net".to_string(),
                https_enabled: false,
            }
        );
    }

    #[test]
    fn parse_status_stopped() {
        let state = parse_status_json(STOPPED_STATUS.as_bytes());
        assert_eq!(state, TailscaleState::NotRunning);
    }

    #[test]
    fn parse_status_invalid_json() {
        let state = parse_status_json(b"not json at all");
        assert_eq!(state, TailscaleState::NotRunning);
    }

    #[test]
    fn fqdn_trailing_dot_stripped() {
        assert_eq!(strip_trailing_dot("host.ts.net."), "host.ts.net");
    }

    #[test]
    fn fqdn_no_trailing_dot_unchanged() {
        assert_eq!(strip_trailing_dot("host.ts.net"), "host.ts.net");
    }

    #[test]
    fn parse_status_empty_cert_domains() {
        let json = r#"{
            "Version": "1.94.1",
            "BackendState": "Running",
            "Self": { "DNSName": "host.ts.net.", "HostName": "host", "OS": "linux", "Online": true, "TailscaleIPs": [] },
            "CertDomains": []
        }"#;
        let state = parse_status_json(json.as_bytes());
        assert_eq!(
            state,
            TailscaleState::Running {
                fqdn: "host.ts.net".to_string(),
                https_enabled: false,
            }
        );
    }

    #[test]
    fn parse_real_status_format() {
        // Test with the full real format from `tailscale status --json`
        let json = r#"{
            "Version": "1.94.1-t62c6f1cd7-g09fea6572",
            "TUN": true,
            "BackendState": "Running",
            "HaveNodeKey": true,
            "AuthURL": "",
            "TailscaleIPs": ["100.80.90.53", "fd7a:115c:a1e0::c601:5a3a"],
            "Self": {
                "ID": "nsCqCKSu4721CNTRL",
                "PublicKey": "nodekey:abc123",
                "HostName": "DGQT92CJFP",
                "DNSName": "dgqt92cjfp.tail911da.ts.net.",
                "OS": "macOS",
                "UserID": 52479562055421475,
                "TailscaleIPs": ["100.80.90.53", "fd7a:115c:a1e0::c601:5a3a"],
                "Online": true,
                "ExitNode": false
            },
            "MagicDNSSuffix": "tail911da.ts.net",
            "CertDomains": null
        }"#;
        let state = parse_status_json(json.as_bytes());
        assert_eq!(
            state,
            TailscaleState::Running {
                fqdn: "dgqt92cjfp.tail911da.ts.net".to_string(),
                https_enabled: false,
            }
        );
    }

    #[cfg(unix)]
    #[test]
    fn extract_http_body_success() {
        let response = b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n\r\n-----BEGIN CERTIFICATE-----\nMIIB";
        let body = extract_http_body(response).unwrap();
        assert_eq!(body, b"-----BEGIN CERTIFICATE-----\nMIIB");
    }

    #[cfg(unix)]
    #[test]
    fn extract_http_body_error_status() {
        let response = b"HTTP/1.1 500 Internal Server Error\r\n\r\nerror details";
        let result = extract_http_body(response);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("500"));
    }

    #[cfg(unix)]
    #[test]
    fn extract_http_body_no_separator() {
        let response = b"garbage data without headers";
        let result = extract_http_body(response);
        assert!(result.is_err());
    }
}
