//! HTTP-over-IPC client for communicating with a running TUICommander instance.
//!
//! Unix: connects via Unix domain socket at `<config_dir>/mcp.sock`
//! Windows: connects via named pipe at `\\.\pipe\tuicommander-mcp` (`-<instance>` for a named instance)

use std::io::{self, Read, Write};
use tuic_ipc::app_instance::current_app_instance;

pub(crate) fn config_dir() -> std::path::PathBuf {
    current_app_instance().config_dir_from(
        dirs::config_dir().as_deref(),
        &dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from(".")),
    )
}

#[cfg(unix)]
static EXPLICIT_INSTANCE_SOCKET: std::sync::OnceLock<std::path::PathBuf> =
    std::sync::OnceLock::new();

/// Freeze the explicit CLI target before dispatch, without changing process env.
#[cfg(unix)]
pub(crate) fn select_instance_socket() {
    let path =
        tuic_ipc::client_socket_path(current_app_instance(), &config_dir(), &std::env::temp_dir());
    if let Some(inherited) = std::env::var_os("TUIC_SOCKET")
        && std::path::Path::new(&inherited) != path
    {
        eprintln!(
            "tuic: using {} (--instance); ignoring TUIC_SOCKET",
            path.display()
        );
    }
    EXPLICIT_INSTANCE_SOCKET
        .set(path)
        .expect("explicit instance socket selected only once before dispatch");
}

#[cfg(unix)]
pub(crate) fn socket_path() -> std::path::PathBuf {
    if let Some(path) = EXPLICIT_INSTANCE_SOCKET.get() {
        return path.clone();
    }
    if let Ok(path) = std::env::var("TUIC_SOCKET") {
        return std::path::PathBuf::from(path);
    }
    tuic_ipc::client_socket_path(current_app_instance(), &config_dir(), &std::env::temp_dir())
}

#[cfg(unix)]
fn connect() -> io::Result<std::os::unix::net::UnixStream> {
    let path = socket_path();
    std::os::unix::net::UnixStream::connect(&path).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("Cannot connect to TUICommander at {}: {e}", path.display()),
        )
    })
}

#[cfg(windows)]
fn connect() -> io::Result<std::fs::File> {
    let path = tuic_ipc::pipe_name(current_app_instance());
    let open = || {
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
    };
    open_with_one_busy_retry(open, || wait_for_pipe(&path)).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("Cannot connect to TUICommander at {path}: {e}"),
        )
    })
}

/// `ERROR_PIPE_BUSY`: every listening instance of the pipe is taken.
#[cfg(any(windows, test))]
const ERROR_PIPE_BUSY: i32 = 231;

/// Open the pipe, and when every instance is busy (concurrent hooks of one
/// agent event, #2632-97ef) wait for a free one once and try again, so a busy
/// pipe is not mistaken for "no TUIC" and sent in-band.
#[cfg(any(windows, test))]
fn open_with_one_busy_retry<T>(
    mut open: impl FnMut() -> io::Result<T>,
    wait: impl FnOnce() -> bool,
) -> io::Result<T> {
    match open() {
        Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY) && wait() => open(),
        result => result,
    }
}

/// Wait up to 2 s for a free instance of the TUIC pipe.
#[cfg(windows)]
fn wait_for_pipe(pipe: &str) -> bool {
    let name: Vec<u16> = pipe.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: `name` is a NUL-terminated UTF-16 string that outlives the call.
    unsafe { windows_sys::Win32::System::Pipes::WaitNamedPipeW(name.as_ptr(), 2000) != 0 }
}

/// HTTP response parsed by the shared IPC framing module.
pub struct Response(tuic_ipc::http::Response);

impl From<tuic_ipc::http::Response> for Response {
    fn from(response: tuic_ipc::http::Response) -> Self {
        Self(response)
    }
}

impl std::ops::Deref for Response {
    type Target = tuic_ipc::http::Response;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Response {
    pub fn json(&self) -> serde_json::Result<serde_json::Value> {
        serde_json::from_str(&self.body)
    }
}

/// Send an HTTP request over the IPC socket and return the response.
pub fn request(method: &str, path: &str, body: Option<&str>) -> io::Result<Response> {
    request_with_headers(method, path, body, &[])
}

/// Same as [`request`], with caller-supplied extra request headers.
pub fn request_with_headers(
    method: &str,
    path: &str,
    body: Option<&str>,
    extra_headers: &[(&str, &str)],
) -> io::Result<Response> {
    request_with_headers_and_timeout(method, path, body, extra_headers, None)
}

pub fn request_with_headers_and_timeout(
    method: &str,
    path: &str,
    body: Option<&str>,
    extra_headers: &[(&str, &str)],
    read_timeout: Option<std::time::Duration>,
) -> io::Result<Response> {
    let mut stream = connect()?;
    exchange(&mut stream, method, path, body, extra_headers, read_timeout)
}

/// What became of an agent hook post.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum HookPost {
    /// The server answered with this status.
    Answered(u16),
    /// No server accepted the connection.
    Unreachable,
    /// The connection broke or timed out after the request was sent: the
    /// server may have processed it.
    Lost,
}

/// POST hook frames; connect failures stay distinct from a lost answer.
pub(crate) fn post_hook(path: &str, body: &str, headers: &[(&str, &str)]) -> HookPost {
    post_hook_on(connect(), path, body, headers)
}

fn post_hook_on(
    stream: io::Result<IpcStream>,
    path: &str,
    body: &str,
    headers: &[(&str, &str)],
) -> HookPost {
    let Ok(mut stream) = stream else {
        return HookPost::Unreachable;
    };
    match exchange(
        &mut stream,
        "POST",
        path,
        Some(body),
        headers,
        Some(std::time::Duration::from_secs(5)),
    ) {
        Ok(response) => HookPost::Answered(response.status),
        Err(_) => HookPost::Lost,
    }
}

#[cfg(unix)]
type IpcStream = std::os::unix::net::UnixStream;
#[cfg(windows)]
type IpcStream = std::fs::File;

fn exchange(
    stream: &mut IpcStream,
    method: &str,
    path: &str,
    body: Option<&str>,
    extra_headers: &[(&str, &str)],
    read_timeout: Option<std::time::Duration>,
) -> io::Result<Response> {
    #[cfg(unix)]
    {
        let timeout = Some(read_timeout.unwrap_or(std::time::Duration::from_secs(3)));
        stream.set_read_timeout(timeout)?;
        stream.set_write_timeout(Some(std::time::Duration::from_secs(3)))?;
    }
    #[cfg(windows)]
    let _ = read_timeout;

    stream.write_all(&tuic_ipc::http::request(method, path, body, extra_headers))?;
    stream.flush()?;
    let mut decoder = tuic_ipc::http::ResponseDecoder::default();
    let mut bytes = [0; 4096];
    loop {
        let count = stream.read(&mut bytes)?;
        decoder.push(&bytes[..count]);
        if let Some(response) = decoder.response(count == 0)? {
            return Ok(Response::from(response));
        }
    }
}

/// Convenience: GET request
pub fn get(path: &str) -> io::Result<Response> {
    request("GET", path, None)
}

/// Convenience: POST request with JSON body
pub fn post(path: &str, body: &str) -> io::Result<Response> {
    request("POST", path, Some(body))
}

/// Convenience: PUT request with JSON body
pub fn put(path: &str, body: &str) -> io::Result<Response> {
    request("PUT", path, Some(body))
}

/// Convenience: DELETE request
pub fn delete(path: &str) -> io::Result<Response> {
    request("DELETE", path, None)
}

/// Check if TUICommander is running
pub fn is_running() -> bool {
    get("/health").is_ok()
}

/// Try to launch TUICommander if not running
pub fn ensure_running() -> io::Result<()> {
    if is_running() {
        return Ok(());
    }
    // Every launcher below starts the default app. For a named instance that
    // would send the command to the user's main instance (#3019-86fa).
    if let Some(id) = current_app_instance().named_id() {
        #[cfg(unix)]
        let endpoint = socket_path().display().to_string();
        #[cfg(windows)]
        let endpoint = tuic_ipc::pipe_name(current_app_instance());
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "TUICommander instance '{id}' is not running (no server at {}); \
                 tuic starts only the default instance",
                endpoint
            ),
        ));
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-a")
            .arg("TUICommander")
            .spawn()
            .map_err(|e| io::Error::new(e.kind(), format!("Failed to launch TUICommander: {e}")))?;
    }

    #[cfg(target_os = "linux")]
    {
        // Try desktop entry first, fall back to direct binary
        let result = std::process::Command::new("xdg-open")
            .arg("tuic://")
            .spawn();
        if result.is_err() {
            std::process::Command::new("tuicommander")
                .spawn()
                .map_err(|e| {
                    io::Error::new(e.kind(), format!("Failed to launch TUICommander: {e}"))
                })?;
        }
    }

    #[cfg(target_os = "windows")]
    {
        let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_default();
        std::process::Command::new(format!("{local_app_data}\\TUICommander\\TUICommander.exe"))
            .spawn()
            .map_err(|e| io::Error::new(e.kind(), format!("Failed to launch TUICommander: {e}")))?;
    }

    // Wait for socket to become available (up to 10s)
    for _ in 0..100 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if is_running() {
            return Ok(());
        }
    }

    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        "TUICommander did not start within 10 seconds",
    ))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::net::{UnixListener, UnixStream};

    /// A server on a fresh socket that reads one request and then runs `answer`.
    fn serve(
        answer: impl FnOnce(&mut UnixStream) + Send + 'static,
    ) -> (tempfile::TempDir, UnixStream) {
        let dir = tempfile::Builder::new()
            .prefix("h")
            .tempdir_in(tuic_test_support::short_socket_test_temp_root())
            .unwrap();
        let path = dir.path().join("s");
        let listener = UnixListener::bind(&path).unwrap();
        std::thread::spawn(move || {
            let (mut peer, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let _ = peer.read(&mut buf);
            answer(&mut peer);
        });
        let stream = UnixStream::connect(&path).unwrap();
        (dir, stream)
    }

    // Catches: a connection the server drops mid-answer (the named pipe
    // disconnects) is mistaken for "no server" and the frame is replayed
    // in-band after the server may already have applied it.
    #[test]
    fn a_dropped_connection_is_lost_not_unreachable() {
        let (_dir, stream) = serve(|_| {});
        assert_eq!(
            post_hook_on(Ok(stream), "/sessions/t/hook", "x", &[]),
            HookPost::Lost
        );
        let refused = Err(io::Error::from(io::ErrorKind::NotFound));
        assert_eq!(
            post_hook_on(refused, "/sessions/t/hook", "x", &[]),
            HookPost::Unreachable
        );
    }

    // Catches: a pipe whose instances are all taken by concurrent hooks is
    // treated as "no TUIC", so the frame bypasses the ordering stamp and goes
    // in-band; or a busy pipe is retried without bound (#2632-97ef F3).
    #[test]
    fn a_busy_pipe_is_waited_for_once() {
        let busy = || Err::<(), _>(io::Error::from_raw_os_error(ERROR_PIPE_BUSY));
        let mut calls = 0;
        let result = open_with_one_busy_retry(
            || {
                calls += 1;
                if calls == 1 { busy() } else { Ok(()) }
            },
            || true,
        );
        assert!(result.is_ok());
        assert_eq!(calls, 2);

        let mut calls = 0;
        let result = open_with_one_busy_retry(
            || {
                calls += 1;
                busy()
            },
            || true,
        );
        assert_eq!(result.unwrap_err().raw_os_error(), Some(ERROR_PIPE_BUSY));
        assert_eq!(calls, 2, "one retry only");

        let mut waited = false;
        let result = open_with_one_busy_retry(
            || Err::<(), _>(io::Error::from(io::ErrorKind::NotFound)),
            || {
                waited = true;
                true
            },
        );
        assert!(result.is_err());
        assert!(!waited, "an absent pipe is not waited for");
    }

    // Catches: a refusal status is not read back, so the CLI cannot tell a
    // stale hook (no fallback) from an unknown session (fallback).
    #[test]
    fn the_answer_status_reaches_the_caller() {
        for status in [204u16, 404, 409] {
            let (_dir, stream) = serve(move |peer| {
                let _ = write!(peer, "HTTP/1.1 {status} X\r\ncontent-length: 0\r\n\r\n");
            });
            assert_eq!(
                post_hook_on(Ok(stream), "/sessions/t/hook", "x", &[]),
                HookPost::Answered(status)
            );
        }
    }
}
