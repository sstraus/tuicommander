/// Resilient MCP stdio ↔ Unix socket transport adapter for TUICommander.
/// Proxies JSON-RPC messages from stdin to POST /mcp on the local Unix socket,
/// forwarding responses back to stdout. Stays alive even without TUIC running,
/// reconnects automatically, and emits `notifications/tools/list_changed` when
/// the connection state changes.
use serde_json::Value;
use std::io::{self, BufRead, Write};
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn config_dir() -> std::path::PathBuf {
    dirs::config_dir()
        .map(|d| d.join("tuicommander"))
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join(".tuicommander")
        })
}

fn socket_path() -> std::path::PathBuf {
    config_dir().join("mcp.sock")
}

/// Write a JSON line to stdout (MCP stdio transport delimiter is \n).
fn emit(json: &Value) {
    let mut stdout = io::stdout().lock();
    let _ = writeln!(stdout, "{}", serde_json::to_string(json).unwrap_or_default());
    let _ = stdout.flush();
}

fn emit_tools_changed() {
    emit(&serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/tools/list_changed"
    }));
}

/// Send an HTTP POST to /mcp over a Unix socket connection.
/// Returns the response body and any mcp-session-id header value.
async fn post_mcp(body: &str, session_id: Option<&str>) -> Result<(String, Option<String>), String> {
    let mut stream = tokio::net::UnixStream::connect(socket_path())
        .await
        .map_err(|e| format!("socket connect: {e}"))?;

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
    stream.shutdown().await.map_err(|e| format!("shutdown: {e}"))?;

    let mut buf = Vec::new();
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
                Some(line.splitn(2, ':').nth(1)?.trim().to_string())
            } else {
                None
            }
        });

    Ok((response_body.to_string(), sid))
}

/// Establish an MCP session with the TUIC server. Returns the session ID.
async fn server_initialize() -> Result<String, String> {
    let init_body = serde_json::json!({
        "jsonrpc": "2.0", "id": 0,
        "method": "initialize",
        "params": { "protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": { "name": "tuic-mcp-bridge", "version": env!("CARGO_PKG_VERSION") } }
    });
    let (_, sid) = post_mcp(&serde_json::to_string(&init_body).unwrap(), None).await?;
    sid.ok_or_else(|| "server did not return mcp-session-id".into())
}

struct BridgeState {
    session_id: Mutex<Option<String>>,
    connected: AtomicBool,
    /// Handle to the SSE listener task — aborted and restarted on reconnect.
    sse_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

/// Open a persistent GET /mcp SSE connection and forward server notifications to stdout.
/// Runs until the connection is closed or an error occurs.
async fn sse_listener(session_id: String) {
    let Ok(mut stream) = tokio::net::UnixStream::connect(socket_path()).await else {
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

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let state = Arc::new(BridgeState {
        session_id: Mutex::new(None),
        connected: AtomicBool::new(false),
        sse_handle: Mutex::new(None),
    });

    // Try initial connection
    if let Ok(sid) = server_initialize().await {
        eprintln!("tuic-mcp-bridge: connected to TUIC");
        *state.session_id.lock().unwrap() = Some(sid);
        state.connected.store(true, Ordering::Relaxed);
        start_sse_listener(&state);
    } else {
        eprintln!("tuic-mcp-bridge: TUIC not running, will retry in background");
    }

    // Background reconnection loop
    let bg_state = Arc::clone(&state);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            if bg_state.connected.load(Ordering::Relaxed) {
                // Verify connection is still alive via health check
                let sid = bg_state.session_id.lock().unwrap().clone();
                let health = post_mcp(
                    &serde_json::to_string(&serde_json::json!({"jsonrpc":"2.0","id":0,"method":"tools/list"})).unwrap(),
                    sid.as_deref(),
                ).await;
                if health.is_err() {
                    eprintln!("tuic-mcp-bridge: connection lost");
                    *bg_state.session_id.lock().unwrap() = None;
                    bg_state.connected.store(false, Ordering::Relaxed);
                    // Abort SSE listener
                    if let Some(h) = bg_state.sse_handle.lock().unwrap().take() {
                        h.abort();
                    }
                    emit_tools_changed();
                }
            } else {
                // Try reconnect
                if let Ok(sid) = server_initialize().await {
                    eprintln!("tuic-mcp-bridge: reconnected to TUIC");
                    *bg_state.session_id.lock().unwrap() = Some(sid);
                    bg_state.connected.store(true, Ordering::Relaxed);
                    start_sse_listener(&bg_state);
                    emit_tools_changed();
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
                eprintln!("tuic-mcp-bridge: invalid JSON: {e}");
                continue;
            }
        };

        let method = request["method"].as_str().unwrap_or("");
        let id = request.get("id").cloned().unwrap_or(Value::Null);

        match method {
            // Handle locally — must work even without TUIC
            "initialize" => {
                emit(&serde_json::json!({
                    "jsonrpc": "2.0", "id": id,
                    "result": {
                        "protocolVersion": "2025-03-26",
                        "capabilities": { "tools": { "listChanged": true } },
                        "serverInfo": { "name": "tuicommander", "version": env!("CARGO_PKG_VERSION") }
                    }
                }));
            }
            "notifications/initialized" => {} // Acknowledgment, no response

            // Proxy to server
            _ => {
                // Lazy reconnect attempt if disconnected
                if !state.connected.load(Ordering::Relaxed) {
                    if let Ok(sid) = server_initialize().await {
                        eprintln!("tuic-mcp-bridge: reconnected to TUIC");
                        *state.session_id.lock().unwrap() = Some(sid);
                        state.connected.store(true, Ordering::Relaxed);
                        start_sse_listener(&state);
                        emit_tools_changed();
                    }
                }

                if state.connected.load(Ordering::Relaxed) {
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
                            eprintln!("tuic-mcp-bridge: proxy error: {e}");
                            state.connected.store(false, Ordering::Relaxed);
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
