//! HTTP endpoints for the application log ring buffer.

use axum::Json;
use axum::extract::{ConnectInfo, Extension, Query, State};
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
    let mut entries = buf.get_entries(0);
    drop(buf);

    // Apply optional filters BEFORE the limit: filtering after slicing to the
    // last N entries starves any query where the matching entries aren't
    // among the most recent N (#655 — `?level=error&limit=50` would return
    // nothing if the last 50 lines happened to all be info).
    if let Some(ref level) = q.level {
        entries.retain(|e| e.level == *level);
    }
    if let Some(ref source) = q.source {
        entries.retain(|e| e.source == *source);
    }
    if let Some(ref audience) = q.audience {
        entries.retain(|e| e.audience == *audience);
    }

    if q.limit > 0 && entries.len() > q.limit {
        entries.drain(0..entries.len() - q.limit);
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

/// GET /diagnostics/memory — where the process's memory is.
///
/// The 2026-09-08 40 GB growth could not be attributed to any structure while it
/// was happening: a 40 GB process is not debuggable, so every candidate had to be
/// excluded by reading code. This answers the question in one request instead.
pub(crate) async fn memory_report_get(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    Json(crate::memory_report::report(&state))
}

/// GET /diagnostics/markers — per-session protocol-marker compliance.
///
/// Exists because the only way to ask "are the agents still emitting `intent:`
/// and `suggest:`?" used to be grepping scrollback, which counts every mention
/// of the word and is capped by buffer size (#4421). These are counts of parsed
/// events against submitted turns.
///
/// A session whose markers are switched off reports `enabled: false` rather than
/// zero compliance — the difference between an agent ignoring the protocol and
/// an agent that was never asked to follow it.
pub(crate) async fn marker_compliance_get(
    State(state): State<std::sync::Arc<crate::state::AppState>>,
) -> Json<serde_json::Value> {
    let sessions: Vec<serde_json::Value> = state
        .session_maps
        .session_states
        .iter()
        .map(|entry| {
            let session_id = entry.key().clone();
            let agent_type = entry.value().agent_type.clone();
            let stats = state.marker_stats_for(&session_id);
            let (intent_enabled, suggest_enabled) =
                crate::mcp_http::mcp_transport::marker_flags_for_agent(
                    &state,
                    agent_type.as_deref(),
                );
            serde_json::json!({
                "session_id": session_id,
                "agent_type": agent_type,
                "turns": stats.turns,
                "intent": stats.intent,
                "suggest": stats.suggest,
                "intent_enabled": intent_enabled,
                "suggest_enabled": suggest_enabled,
            })
        })
        .collect();
    Json(serde_json::json!({ "sessions": sessions }))
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
// Raw PTY capture tap
// ---------------------------------------------------------------------------

/// GET /diagnostics/capture — is the tap recording, and how much has it written.
pub(crate) async fn capture_get() -> Json<serde_json::Value> {
    Json(crate::pty_capture::status())
}

/// POST /diagnostics/capture — start/stop recording raw PTY bytes.
/// Body: `{ "enabled": true, "session_id": "<optional filter>" }`.
///
/// Turn it on when a state-detection bug is reproducible but not yet understood:
/// the capture it leaves behind becomes a `pty::tests` fixture, which is the only
/// way a detector regression stops recurring. See `pty_capture` for the why.
pub(crate) async fn capture_set(
    Json(body): Json<super::types::SetCaptureRequest>,
) -> Json<serde_json::Value> {
    Json(crate::pty_capture::set_enabled_in_config_dir(
        body.enabled,
        body.session_id,
    ))
}

// ---------------------------------------------------------------------------
// invoke_js — execute a debug script in the main WebView (local or authenticated)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub(crate) struct InvokeJsBody {
    script: String,
}

/// POST /debug/invoke_js — execute JavaScript in the main WebView.
///
/// Local or token-authenticated callers only (this is an RCE surface, and an
/// authenticated owner already has terminal execution — #2658-7301): mirrors
/// the MCP `debug action=invoke_js` path so the owner's phone and the dev
/// build can script diagnostics over HTTP. Fire-and-forget: the
/// result + captured console output are pushed to the ring buffer with
/// source="eval_js"; read them back via GET /logs?source=eval_js.
pub(crate) async fn invoke_js_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<super::guards::Authenticated>>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<InvokeJsBody>,
) -> impl IntoResponse {
    if let Err(resp) = super::guards::require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    Json(eval_debug_script(&state, &body.script)).into_response()
}

/// Wrap `script` in the standard debug harness (console capture + result
/// serialization) and evaluate it in the main WebView. The harness invokes the
/// `push_log` command so the result lands in the ring buffer as source="eval_js".
/// Shared by the MCP `debug` tool and the HTTP `/debug/invoke_js` route.
pub(crate) fn eval_debug_script(state: &Arc<AppState>, script: &str) -> serde_json::Value {
    // Checked before the build split, so a headless build refuses for the same reason.
    if state.secrets.tools_blocked() {
        return serde_json::json!({"error": "Agent inspection is disabled while a private secret form is open"});
    }
    eval_in_main_webview(state, script)
}

#[cfg(feature = "desktop")]
fn eval_in_main_webview(state: &Arc<AppState>, script: &str) -> serde_json::Value {
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

/// POST /debug/reload_webview — reload the main WebView from the native side.
///
/// The escape hatch for a WebView whose main JS thread is blocked or whose web
/// content process is gone: the UI is white, `/debug/invoke_js` is useless
/// because it needs that thread to run the script, and the only remedy left was
/// restarting the app — which kills every PTY session with it.
///
/// It runs entirely on the native side, so it works precisely when the JS side
/// does not. Sessions live in the backend, so this costs nothing but a repaint.
/// Local or token-authenticated callers, like its neighbour: the owner's phone
/// is the remaining remedy when the desktop window is white (#2658-7301).
pub(crate) async fn reload_webview_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<super::guards::Authenticated>>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    if let Err(resp) = super::guards::require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    tracing::info!(source = "webview", caller = %addr, "HTTP WebView reload requested");
    Json(reload_main_webview(&state)).into_response()
}

/// Put the `main` webview back on the app.
///
/// Deliberately a `navigate`, not a `reload`. On 2026-09-08 this endpoint
/// answered `{"ok":true}` and left the window white for an hour: the frame was
/// on `about:srcdoc`, and reloading a blank document reloads the blank document.
/// The same recovery serves the automatic poller — see `webview_recovery`.
pub(crate) fn reload_main_webview(state: &Arc<AppState>) -> serde_json::Value {
    crate::webview_recovery::navigate_home(state, "http_route")
}

#[cfg(not(feature = "desktop"))]
fn eval_in_main_webview(_state: &Arc<AppState>, _script: &str) -> serde_json::Value {
    serde_json::json!({"error": "invoke_js requires desktop feature"})
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::Query;

    /// The newest entries in the buffer are all "info"; the older ones are the
    /// 3 "error" lines we care about. A limit-then-filter implementation slices
    /// the last 50 (all info) before filtering, so `?level=error&limit=50`
    /// would come back empty. Filtering first must still surface the 3 older
    /// error entries, in their original chronological order.
    #[tokio::test]
    async fn get_logs_filters_before_applying_limit() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        {
            let mut buf = state.log_buffer.lock();
            for i in 0..3 {
                buf.push_with_audience(
                    "error".to_string(),
                    "app".to_string(),
                    format!("error-{i}"),
                    None,
                    None,
                );
            }
            for i in 0..200 {
                buf.push_with_audience(
                    "info".to_string(),
                    "app".to_string(),
                    format!("info-{i}"),
                    None,
                    None,
                );
            }
        }

        let query = Query(GetLogsQuery {
            limit: 50,
            level: Some("error".to_string()),
            source: None,
            audience: None,
        });
        let Json(entries) = get_logs(State(state), query).await;

        let messages: Vec<&str> = entries.iter().map(|e| e.message.as_str()).collect();
        assert_eq!(
            messages,
            vec!["error-0", "error-1", "error-2"],
            "expected the 3 older error entries, in order, not truncated away by the limit"
        );
    }
}

#[derive(Deserialize)]
pub(crate) struct FrameWorkloadBody {
    recording: String,
}

/// Same bounded, serialized writer as desktop IPC. Existing HTTP auth applies.
pub(crate) async fn save_frame_workload(Json(body): Json<FrameWorkloadBody>) -> impl IntoResponse {
    match crate::frame_workload::save_frame_workload(body.recording).await {
        Ok(path) => Json(serde_json::json!(path)).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": error})),
        )
            .into_response(),
    }
}
