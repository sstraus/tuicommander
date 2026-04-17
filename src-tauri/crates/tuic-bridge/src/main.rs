//! Resilient MCP stdio ↔ IPC transport adapter for TUICommander.
//! Proxies JSON-RPC messages from stdin to POST /mcp on the local IPC endpoint,
//! forwarding responses back to stdout. Stays alive even without TUIC running,
//! reconnects automatically, and emits `notifications/tools/list_changed` when
//! the connection state changes.
//!
//! Unix: connects via Unix domain socket at `<config_dir>/mcp.sock`
//! Windows: connects via named pipe at `\\.\pipe\tuicommander-mcp`

use serde_json::Value;
use std::io::{self, BufRead, Write};
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

// ---------------------------------------------------------------------------
// Platform-specific IPC connection
// ---------------------------------------------------------------------------

#[cfg(unix)]
fn config_dir() -> std::path::PathBuf {
    dirs::config_dir()
        .map(|d| d.join("com.tuic.commander"))
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join(".tuicommander")
        })
}

#[cfg(unix)]
fn ipc_endpoint() -> String {
    config_dir().join("mcp.sock").to_string_lossy().to_string()
}

#[cfg(windows)]
fn ipc_endpoint() -> String {
    r"\\.\pipe\tuicommander-mcp".to_string()
}

/// Wrapper that provides a unified IPC stream type across platforms.
/// Both inner types implement AsyncRead + AsyncWrite + Unpin.
enum IpcStream {
    #[cfg(unix)]
    Unix(tokio::net::UnixStream),
    #[cfg(windows)]
    Pipe(tokio::net::windows::named_pipe::NamedPipeClient),
}

impl tokio::io::AsyncRead for IpcStream {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        match self.get_mut() {
            #[cfg(unix)]
            IpcStream::Unix(s) => std::pin::Pin::new(s).poll_read(cx, buf),
            #[cfg(windows)]
            IpcStream::Pipe(s) => std::pin::Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl tokio::io::AsyncWrite for IpcStream {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<io::Result<usize>> {
        match self.get_mut() {
            #[cfg(unix)]
            IpcStream::Unix(s) => std::pin::Pin::new(s).poll_write(cx, buf),
            #[cfg(windows)]
            IpcStream::Pipe(s) => std::pin::Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        match self.get_mut() {
            #[cfg(unix)]
            IpcStream::Unix(s) => std::pin::Pin::new(s).poll_flush(cx),
            #[cfg(windows)]
            IpcStream::Pipe(s) => std::pin::Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        match self.get_mut() {
            #[cfg(unix)]
            IpcStream::Unix(s) => std::pin::Pin::new(s).poll_shutdown(cx),
            #[cfg(windows)]
            IpcStream::Pipe(s) => std::pin::Pin::new(s).poll_shutdown(cx),
        }
    }
}

/// Open a connection to the TUIC IPC endpoint.
/// Tries `TUIC_SOCKET` env var first, then `mcp.sock`, then any `mcp-*.sock` in config_dir.
async fn connect_ipc() -> Result<IpcStream, String> {
    #[cfg(unix)]
    {
        // Explicit override via environment variable
        if let Ok(explicit) = std::env::var("TUIC_SOCKET") {
            let path = std::path::PathBuf::from(&explicit);
            let stream = tokio::net::UnixStream::connect(&path)
                .await
                .map_err(|e| format!("connect {}: {e}", path.display()))?;
            return Ok(IpcStream::Unix(stream));
        }

        let dir = config_dir();
        let primary = dir.join("mcp.sock");

        // Try primary socket first
        if let Ok(stream) = tokio::net::UnixStream::connect(&primary).await {
            return Ok(IpcStream::Unix(stream));
        }

        // Fall back to mcp-*.sock alternatives
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let Some(name_str) = name.to_str() else { continue };
                if name_str.starts_with("mcp-") && name_str.ends_with(".sock") {
                    if let Ok(stream) = tokio::net::UnixStream::connect(&entry.path()).await {
                        return Ok(IpcStream::Unix(stream));
                    }
                }
            }
        }

        Err(format!("connect {}: no live socket found", primary.display()))
    }
    #[cfg(windows)]
    {
        const PIPE_NAME: &str = r"\\.\pipe\tuicommander-mcp";
        let client = tokio::net::windows::named_pipe::ClientOptions::new()
            .open(PIPE_NAME)
            .map_err(|e| format!("connect {PIPE_NAME}: {e}"))?;
        Ok(IpcStream::Pipe(client))
    }
}

// ---------------------------------------------------------------------------
// JSON-RPC helpers
// ---------------------------------------------------------------------------

/// Write a JSON line to stdout (MCP stdio transport delimiter is \n).
/// Exits the process if stdout is closed — the MCP client is gone, nothing left to do.
fn emit(json: &Value) {
    let mut stdout = io::stdout().lock();
    if writeln!(stdout, "{}", serde_json::to_string(json).unwrap_or_default()).is_err() {
        std::process::exit(0);
    }
    let _ = stdout.flush();
}

fn emit_tools_changed() {
    emit(&serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/tools/list_changed"
    }));
}

// ---------------------------------------------------------------------------
// HTTP-over-IPC transport
// ---------------------------------------------------------------------------

/// Send an HTTP POST to /mcp over an IPC connection.
/// Returns the response body and any mcp-session-id header value.
async fn post_mcp(body: &str, session_id: Option<&str>) -> Result<(String, Option<String>), String> {
    let mut stream = connect_ipc().await?;

    let mut headers = format!(
        "POST /mcp HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    if let Some(sid) = session_id {
        headers.push_str(&format!("mcp-session-id: {sid}\r\n"));
    }
    headers.push_str("\r\n");

    stream.write_all(headers.as_bytes()).await.map_err(|e| format!("write headers: {e}"))?;
    stream.write_all(body.as_bytes()).await.map_err(|e| format!("write body: {e}"))?;

    // Read the response first, then shutdown.  Calling shutdown() before read
    // signals EOF to the server, which may drop the connection before responding.
    let mut buf = Vec::new();
    // Read until Connection: close triggers server-side EOF
    stream.read_to_end(&mut buf).await.map_err(|e| format!("read: {e}"))?;
    let raw = String::from_utf8_lossy(&buf);

    // Split headers from body
    let (header_section, response_body) = raw
        .split_once("\r\n\r\n")
        .ok_or("invalid HTTP response")?;

    // Extract mcp-session-id from response headers
    let sid = header_section
        .lines()
        .find_map(|line| {
            let lower = line.to_lowercase();
            if lower.starts_with("mcp-session-id:") {
                Some(line.split_once(':')?.1.trim().to_string())
            } else {
                None
            }
        });

    Ok((response_body.to_string(), sid))
}

/// Establish an MCP session with the TUIC server.
/// Returns (session_id, server_response_body).
async fn server_initialize() -> Result<(String, String), String> {
    let init_body = serde_json::json!({
        "jsonrpc": "2.0", "id": 0,
        "method": "initialize",
        "params": { "protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": { "name": "tuic-bridge", "version": env!("CARGO_PKG_VERSION") } }
    });
    let (body, sid) = post_mcp(&serde_json::to_string(&init_body).unwrap(), None).await?;
    let sid = sid.ok_or_else(|| "server did not return mcp-session-id".to_string())?;
    Ok((sid, body))
}

// ---------------------------------------------------------------------------
// SSE listener
// ---------------------------------------------------------------------------

struct BridgeState {
    session_id: Mutex<Option<String>>,
    connected: AtomicBool,
    /// Handle to the SSE listener task — aborted and restarted on reconnect.
    sse_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

/// Open a persistent GET /mcp SSE connection and forward server notifications to stdout.
/// Runs until the connection is closed or an error occurs.
async fn sse_listener(session_id: String) {
    let Ok(mut stream) = connect_ipc().await else {
        return;
    };

    let request = format!(
        "GET /mcp HTTP/1.1\r\nHost: localhost\r\nAccept: text/event-stream\r\nmcp-session-id: {session_id}\r\n\r\n"
    );
    if stream.write_all(request.as_bytes()).await.is_err() {
        return;
    }

    // Read SSE events line by line using a simple buffer
    let mut buf = Vec::with_capacity(4096);
    let mut tmp = [0u8; 1024];
    loop {
        let n = match stream.read(&mut tmp).await {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        buf.extend_from_slice(&tmp[..n]);

        // Process complete lines (SSE uses \n-delimited frames)
        while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
            let line = String::from_utf8_lossy(&buf[..pos]).trim().to_string();
            buf.drain(..=pos);

            if let Some(data) = line.strip_prefix("data:") {
                let data = data.trim();
                if data.contains("tools/list_changed") {
                    emit_tools_changed();
                }
            }
        }
    }
}

/// Spawn (or restart) the SSE listener background task.
fn start_sse_listener(state: &Arc<BridgeState>) {
    let sid = state.session_id.lock().unwrap().clone();
    let Some(sid) = sid else { return };

    // Abort previous listener if any
    if let Some(handle) = state.sse_handle.lock().unwrap().take() {
        handle.abort();
    }

    let handle = tokio::spawn(async move {
        sse_listener(sid).await;
    });
    *state.sse_handle.lock().unwrap() = Some(handle);
}

/// Respond when TUIC is not available.
fn emit_offline_response(method: &str, id: &Value) {
    match method {
        "tools/list" => emit(&serde_json::json!({
            "jsonrpc": "2.0", "id": id,
            "result": { "tools": [] }
        })),
        "tools/call" => emit(&serde_json::json!({
            "jsonrpc": "2.0", "id": id,
            "result": {
                "content": [{ "type": "text", "text": "TUICommander is not running. Start it to use MCP tools." }],
                "isError": true
            }
        })),
        _ => emit(&serde_json::json!({
            "jsonrpc": "2.0", "id": id,
            "error": { "code": -32601, "message": format!("Method not found: {method}") }
        })),
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    eprintln!("tuic-bridge v{} starting ({})", env!("CARGO_PKG_VERSION"), ipc_endpoint());

    let state = Arc::new(BridgeState {
        session_id: Mutex::new(None),
        connected: AtomicBool::new(false),
        sse_handle: Mutex::new(None),
    });

    // Try initial connection
    if let Ok((sid, _)) = server_initialize().await {
        eprintln!("tuic-bridge: connected to TUIC");
        *state.session_id.lock().unwrap() = Some(sid);
        state.connected.store(true, Ordering::Release);
        start_sse_listener(&state);
    } else {
        eprintln!("tuic-bridge: TUIC not running, will retry in background");
    }

    // Background reconnection loop. Hysteresis: disconnect only after N consecutive
    // health failures — a single transient (GC pause, socket accept lag, EOF during
    // Tauri bg work) must not flip the bridge offline.
    const HEALTH_FAIL_THRESHOLD: u32 = 3;
    let bg_state = Arc::clone(&state);
    tokio::spawn(async move {
        let mut consecutive_failures: u32 = 0;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            if bg_state.connected.load(Ordering::Acquire) {
                let sid = bg_state.session_id.lock().unwrap().clone();
                let health = post_mcp(
                    &serde_json::to_string(&serde_json::json!({"jsonrpc":"2.0","id":0,"method":"tools/list"})).unwrap(),
                    sid.as_deref(),
                ).await;
                if health.is_err() {
                    consecutive_failures += 1;
                    eprintln!(
                        "tuic-bridge: health check failed ({}/{})",
                        consecutive_failures, HEALTH_FAIL_THRESHOLD
                    );
                    if consecutive_failures >= HEALTH_FAIL_THRESHOLD {
                        eprintln!("tuic-bridge: connection lost");
                        *bg_state.session_id.lock().unwrap() = None;
                        bg_state.connected.store(false, Ordering::Release);
                        if let Some(h) = bg_state.sse_handle.lock().unwrap().take() {
                            h.abort();
                        }
                        emit_tools_changed();
                        consecutive_failures = 0;
                    }
                } else {
                    consecutive_failures = 0;
                }
            } else {
                if let Ok((sid, _)) = server_initialize().await {
                    eprintln!("tuic-bridge: reconnected to TUIC");
                    *bg_state.session_id.lock().unwrap() = Some(sid);
                    bg_state.connected.store(true, Ordering::Release);
                    start_sse_listener(&bg_state);
                    emit_tools_changed();
                    consecutive_failures = 0;
                }
            }
        }
    });

    // Stdin reader in blocking thread → channel → async handler
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    std::thread::spawn(move || {
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            match line {
                Ok(l) if !l.trim().is_empty() => { if tx.send(l).is_err() { break; } }
                Err(_) => break,
                _ => {}
            }
        }
    });

    while let Some(line) = rx.recv().await {
        let request: Value = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("tuic-bridge: invalid JSON: {e}");
                continue;
            }
        };

        let method = request["method"].as_str().unwrap_or("");
        let id = request.get("id").cloned().unwrap_or(Value::Null);

        match method {
            "initialize" => {
                // Proxy to server when connected to get dynamic instructions.
                // The server response includes intent protocol, active sessions, etc.
                // Fall back to a minimal local response only when offline.
                let proxied = if state.connected.load(Ordering::Acquire) || {
                    // Try lazy connect if not yet connected
                    if let Ok((sid, _)) = server_initialize().await {
                        eprintln!("tuic-bridge: connected to TUIC");
                        *state.session_id.lock().unwrap() = Some(sid);
                        state.connected.store(true, Ordering::Release);
                        start_sse_listener(&state);
                        true
                    } else {
                        false
                    }
                } {
                    let sid = state.session_id.lock().unwrap().clone();
                    match post_mcp(&line, sid.as_deref()).await {
                        Ok((body, new_sid)) => {
                            if let Some(s) = new_sid {
                                *state.session_id.lock().unwrap() = Some(s);
                            }
                            // Parse server response, inject listChanged capability
                            // (the server doesn't advertise it but the bridge supports it)
                            if let Ok(mut resp) = serde_json::from_str::<Value>(&body) {
                                resp["result"]["capabilities"]["tools"]["listChanged"] = Value::Bool(true);
                                Some(resp)
                            } else {
                                None
                            }
                        }
                        Err(e) => {
                            eprintln!("tuic-bridge: initialize proxy error: {e}");
                            state.connected.store(false, Ordering::Release);
                            *state.session_id.lock().unwrap() = None;
                            None
                        }
                    }
                } else {
                    None
                };

                emit(&proxied.unwrap_or_else(|| serde_json::json!({
                    "jsonrpc": "2.0", "id": id,
                    "result": {
                        "protocolVersion": "2025-03-26",
                        "capabilities": { "tools": { "listChanged": true } },
                        "serverInfo": { "name": "tuicommander", "version": env!("CARGO_PKG_VERSION") }
                    }
                })));
            }
            "notifications/initialized" => {} // Acknowledgment, no response

            // Proxy to server
            _ => {
                // Lazy reconnect attempt if disconnected
                if !state.connected.load(Ordering::Acquire)
                    && let Ok((sid, _)) = server_initialize().await
                {
                    eprintln!("tuic-bridge: reconnected to TUIC");
                    *state.session_id.lock().unwrap() = Some(sid);
                    state.connected.store(true, Ordering::Release);
                    start_sse_listener(&state);
                    emit_tools_changed();
                }

                if state.connected.load(Ordering::Acquire) {
                    let sid = state.session_id.lock().unwrap().clone();
                    match post_mcp(&line, sid.as_deref()).await {
                        Ok((body, new_sid)) => {
                            // Update session ID if server returned a new one
                            if let Some(s) = new_sid {
                                *state.session_id.lock().unwrap() = Some(s);
                            }
                            // Forward raw JSON response to stdout
                            let mut stdout = io::stdout().lock();
                            let _ = writeln!(stdout, "{body}");
                            let _ = stdout.flush();
                        }
                        Err(e) => {
                            eprintln!("tuic-bridge: proxy error: {e}");
                            state.connected.store(false, Ordering::Release);
                            *state.session_id.lock().unwrap() = None;
                            // Fall through to offline response
                            emit_offline_response(method, &id);
                        }
                    }
                } else {
                    emit_offline_response(method, &id);
                }
            }
        }
    }
}
