//! HTTP endpoints for the application log ring buffer.

use axum::Json;
use axum::extract::{ConnectInfo, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Deserialize;
use std::net::SocketAddr;
use std::sync::Arc;

use crate::AppState;
use crate::app_logger::LogEntry;

#[derive(Deserialize)]
pub(crate) struct GetLogsQuery {
    #[serde(default)]
    limit: usize,
    /// Optional minimum level filter: "debug", "info", "warn", "error"
    #[serde(default)]
    level: Option<String>,
    /// Optional source filter: "app", "plugin", "git", "terminal", etc.
    #[serde(default)]
    source: Option<String>,
    /// Optional audience filter: "user" or "diagnostic".
    #[serde(default)]
    audience: Option<String>,
}

/// GET /logs — retrieve log entries from the ring buffer.
pub(crate) async fn get_logs(
    State(state): State<Arc<AppState>>,
    Query(q): Query<GetLogsQuery>,
) -> Json<Vec<LogEntry>> {
    let buf = state.log_buffer.lock();
    let mut entries = buf.get_entries(q.limit);

    // Apply optional filters
    if let Some(ref level) = q.level {
        entries.retain(|e| e.level == *level);
    }
    if let Some(ref source) = q.source {
        entries.retain(|e| e.source == *source);
    }
    if let Some(ref audience) = q.audience {
        entries.retain(|e| e.audience == *audience);
    }

    Json(entries)
}

#[derive(Deserialize)]
pub(crate) struct PushLogBody {
    level: String,
    source: String,
    message: String,
    data_json: Option<String>,
    #[serde(default)]
    audience: Option<String>,
}

/// POST /logs — push a log entry into the ring buffer.
pub(crate) async fn push_log(
    State(state): State<Arc<AppState>>,
    Json(body): Json<PushLogBody>,
) -> StatusCode {
    let mut buf = state.log_buffer.lock();
    buf.push_with_audience(
        body.level,
        body.source,
        body.message,
        body.data_json,
        body.audience,
    );
    StatusCode::NO_CONTENT
}

/// DELETE /logs — clear all log entries.
pub(crate) async fn clear_logs(State(state): State<Arc<AppState>>) -> StatusCode {
    let mut buf = state.log_buffer.lock();
    buf.clear();
    StatusCode::NO_CONTENT
}

// ---------------------------------------------------------------------------
// Diagnostic mode toggle
// ---------------------------------------------------------------------------

/// GET /diagnostics — current diagnostic mode state.
pub(crate) async fn diagnostics_get() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "enabled": crate::cpu_watchdog::diagnostic_mode(),
    }))
}

/// POST /diagnostics — toggle diagnostic mode. Body: `{ "enabled": true }`.
pub(crate) async fn diagnostics_set(
    Json(body): Json<super::types::SetApiDebugRequest>,
) -> Json<serde_json::Value> {
    crate::cpu_watchdog::set_diagnostic_mode(body.enabled);
    Json(serde_json::json!({
        "ok": true,
        "enabled": body.enabled,
    }))
}

// ---------------------------------------------------------------------------
// invoke_js — execute a debug script in the main WebView (loopback only)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub(crate) struct InvokeJsBody {
    script: String,
}

/// POST /debug/invoke_js — execute JavaScript in the main WebView.
///
/// Loopback-only (this is an RCE surface): mirrors the MCP `debug
/// action=invoke_js` path so the dev build — reachable only over HTTP, not the
/// MCP stdio transport — is scriptable for diagnostics. Fire-and-forget: the
/// result + captured console output are pushed to the ring buffer with
/// source="eval_js"; read them back via GET /logs?source=eval_js.
pub(crate) async fn invoke_js_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<InvokeJsBody>,
) -> impl IntoResponse {
    if let Err(resp) = super::guards::localhost_only(&addr) {
        return resp.into_response();
    }
    Json(eval_debug_script(&state, &body.script)).into_response()
}

/// Wrap `script` in the standard debug harness (console capture + result
/// serialization) and evaluate it in the main WebView. The harness invokes the
/// `push_log` command so the result lands in the ring buffer as source="eval_js".
/// Shared by the MCP `debug` tool and the HTTP `/debug/invoke_js` route.
#[cfg(feature = "desktop")]
pub(crate) fn eval_debug_script(state: &Arc<AppState>, script: &str) -> serde_json::Value {
    use tauri::Manager;
    let app_handle = state.app_handle.read().clone();
    let Some(handle) = app_handle else {
        return serde_json::json!({"error": "AppHandle not initialized"});
    };
    let Some(window) = handle.get_webview_window("main") else {
        return serde_json::json!({"error": "main window not found"});
    };
    let wrapped = format!(
        r#"(async () => {{
  const __src = "eval_js";
  const __logs = [];
  const __origLog = console.log;
  const __origWarn = console.warn;
  const __origError = console.error;
  const __origInfo = console.info;
  const __fmt = (a) => typeof a === "string" ? a : JSON.stringify(a);
  console.log = (...a) => {{ __logs.push(a.map(__fmt).join(" ")); __origLog(...a); }};
  console.info = (...a) => {{ __logs.push(a.map(__fmt).join(" ")); __origInfo(...a); }};
  console.warn = (...a) => {{ __logs.push("[WARN] " + a.map(__fmt).join(" ")); __origWarn(...a); }};
  console.error = (...a) => {{ __logs.push("[ERROR] " + a.map(__fmt).join(" ")); __origError(...a); }};
  try {{
    const __result = await (async () => {{ {script} }})();
    const __val = __result === undefined ? "(undefined)" : JSON.stringify(__result, null, 2);
    const __msg = __logs.length > 0 ? __logs.join("\n") + "\n---\n" + __val : __val;
    window.__TAURI__.core.invoke("push_log", {{ level: "info", source: __src, message: __msg, dataJson: null }});
  }} catch (__e) {{
    const __val = __e instanceof Error ? `${{__e.name}}: ${{__e.message}}\n${{__e.stack}}` : String(__e);
    const __msg = __logs.length > 0 ? __logs.join("\n") + "\n---\n" + __val : __val;
    window.__TAURI__.core.invoke("push_log", {{ level: "error", source: __src, message: __msg, dataJson: null }});
  }} finally {{
    console.log = __origLog;
    console.info = __origInfo;
    console.warn = __origWarn;
    console.error = __origError;
  }}
}})()"#
    );
    match window.eval(&wrapped) {
        Ok(()) => serde_json::json!({
            "ok": true,
            "hint": "Result logged with source='eval_js'. Read via: GET /logs?source=eval_js&limit=1"
        }),
        Err(e) => serde_json::json!({"error": format!("eval failed: {e}")}),
    }
}

#[cfg(not(feature = "desktop"))]
pub(crate) fn eval_debug_script(_state: &Arc<AppState>, _script: &str) -> serde_json::Value {
    serde_json::json!({"error": "invoke_js requires desktop feature"})
}
