//! Shared instance namespaces and HTTP framing, independent of sync/async I/O.

pub mod app_instance;
pub mod hook_clock;
pub mod http;

/// Backend deadlines shared with the CLI and stdio bridge.
pub const SECRET_FORM_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);
pub const SECRET_CHILD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// Allow private entry/consent and execution to finish before transport expiry.
pub fn secret_response_timeout(action: &str) -> Option<std::time::Duration> {
    let margin = std::time::Duration::from_secs(5);
    match action {
        "request" => Some(SECRET_FORM_TIMEOUT + margin),
        "run" => Some(SECRET_FORM_TIMEOUT + SECRET_CHILD_TIMEOUT + margin),
        _ => None,
    }
}

use app_instance::AppInstance;
use sha2::{Digest, Sha256};
use std::fmt::Write;
use std::path::{Path, PathBuf};

/// Resolve the server's Unix endpoint, including its short named-instance path.
pub fn socket_path(instance: &AppInstance, config_dir: &Path, temp_dir: &Path) -> PathBuf {
    match instance.named_id() {
        Some(id) => named_socket_path(id, temp_dir),
        None => config_dir.join("mcp.sock"),
    }
}

/// File in a named instance's config directory that names the socket the app bound.
///
/// A named socket lives in the app's TMPDIR, and a client in another shell can
/// have a different one (`make agent-test` gives its app a private TMPDIR). The
/// config directory is the base that both sides compute the same way.
pub const SOCKET_POINTER_FILE: &str = "mcp-socket";

/// Record `socket` as the endpoint of the instance that owns `config_dir`.
pub fn publish_socket_path(config_dir: &Path, socket: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(config_dir)?;
    let staged = config_dir.join(format!(".{SOCKET_POINTER_FILE}.{}", std::process::id()));
    std::fs::write(&staged, socket.as_os_str().as_encoded_bytes())?;
    std::fs::rename(&staged, config_dir.join(SOCKET_POINTER_FILE))
}

/// The socket a client should try for `instance`: the one the app published
/// in its config directory when that socket exists, else [`socket_path`].
pub fn client_socket_path(instance: &AppInstance, config_dir: &Path, temp_dir: &Path) -> PathBuf {
    if instance.named_id().is_some()
        && let Ok(published) = std::fs::read_to_string(config_dir.join(SOCKET_POINTER_FILE))
    {
        let published = PathBuf::from(published);
        if published.is_absolute() && published.exists() {
            return published;
        }
    }
    socket_path(instance, config_dir, temp_dir)
}

/// Preserve the deployed short socket name (first eight SHA-256 bytes).
pub fn named_socket_path(id: &str, temp_dir: &Path) -> PathBuf {
    let digest = Sha256::digest(id.as_bytes());
    let mut short_id = String::with_capacity(16);
    for byte in &digest[..8] {
        write!(short_id, "{byte:02x}").expect("writing to a String cannot fail");
    }
    temp_dir.join(format!("tuic-mcp-{short_id}.sock"))
}

/// The alternate socket a second process of the same named instance binds when
/// the primary is held: `tuic-mcp-<16 hex>-<pid>.sock` beside it. The bridge's
/// fallback scan and the Local-connect identity check accept exactly this shape.
pub fn named_alternate_socket_path(id: &str, temp_dir: &Path, pid: u32) -> PathBuf {
    let primary = named_socket_path(id, temp_dir);
    let stem = primary
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    primary.with_file_name(format!("{stem}-{pid}.sock"))
}

/// Host pathname capacity excluding the terminating NUL.
#[cfg(target_os = "linux")]
pub const UNIX_SOCKET_PATH_MAX: usize = 107;
/// macOS and the conservative fallback use a 104-byte `sun_path`.
#[cfg(not(target_os = "linux"))]
pub const UNIX_SOCKET_PATH_MAX: usize = 103;

/// `Err` naming the length, the budget and the way out when `path` is too long
/// to bind as a Unix socket, instead of the opaque "path must be shorter than
/// SUN_LEN" every bind attempt would report.
pub fn check_unix_socket_path(path: &Path) -> Result<(), String> {
    let len = path.as_os_str().len();
    if len <= UNIX_SOCKET_PATH_MAX {
        return Ok(());
    }
    Err(format!(
        "{} is {len} bytes, over the {UNIX_SOCKET_PATH_MAX}-byte Unix socket path limit; \
         a named instance's socket lives in TMPDIR, so start the app with a TMPDIR of \
         at most {} bytes for this name (TUIC_SOCKET only tells clients where an \
         existing socket is; it does not move the app's listener)",
        path.display(),
        UNIX_SOCKET_PATH_MAX.saturating_sub(1 + path.file_name().map_or(0, |name| name.len()))
    ))
}

/// Windows IPC endpoint of the default instance. Kept identical to the
/// server's existing pipe contract.
pub const PIPE_NAME: &str = r"\\.\pipe\tuicommander-mcp";

/// Resolve the server's Windows pipe. A named instance appends its id, so an
/// isolated instance never shares the default instance's pipe.
pub fn pipe_name(instance: &AppInstance) -> String {
    match instance.named_id() {
        Some(id) => format!("{PIPE_NAME}-{id}"),
        None => PIPE_NAME.to_string(),
    }
}

pub mod review_agents;
