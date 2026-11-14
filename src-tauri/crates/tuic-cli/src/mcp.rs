//! Minimal MCP client for the peer registry.
//!
//! `tuic agent send` must reach the SAME authoritative delivery path the MCP
//! `agent action=send` tool uses, so a message to a registered peer lands in
//! that peer's inbox instead of being typed into a PTY. That path resolves the
//! SENDER from the MCP protocol session, so this client initializes MCP with
//! the same `$TUIC_SESSION` header as `tuic-bridge`, then makes the tool call.
//!
//! Deliberately NOT a second copy of the delivery logic: duplicating it would
//! give two implementations of exactly-once inbox routing that could drift.

use serde_json::{Value, json};

use crate::ipc;

/// Protocol revision this client speaks. Kept in step with the server's
/// advertised revision; the server negotiates down if it is older.
const PROTOCOL_VERSION: &str = "2025-06-18";
pub(crate) const EXTERNAL_CALLER_NOTICE: &str =
    "tuic: no TUIC_SESSION; registering an external MCP caller";

/// The tuic session UUID of the PTY this CLI runs inside, injected by TUIC.
/// Absent when `tuic` is run from a plain terminal outside TUICommander.
fn tuic_session() -> Option<String> {
    std::env::var("TUIC_SESSION").ok().filter(|s| !s.is_empty())
}

fn requires_external_registration(tuic_session: Option<&str>) -> bool {
    tuic_session.is_none()
}

/// The server logs this pid on every initialize, so a storm of fresh sessions
/// can be traced to the process that opened them.
fn client_pid() -> String {
    std::process::id().to_string()
}

fn post(
    body: &Value,
    session: Option<&str>,
    tuic_session: Option<&str>,
    read_timeout: Option<std::time::Duration>,
) -> Result<ipc::Response, String> {
    let pid = client_pid();
    let extra = mcp_headers(session, tuic_session, &pid);
    ipc::request_with_headers_and_timeout(
        "POST",
        "/mcp",
        Some(&body.to_string()),
        &extra,
        read_timeout,
    )
    .map_err(|error| match error.kind() {
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock =>
            "TUICommander reply timed out; the server may still complete the action. Check its state before retrying.".to_string(),
        _ => error.to_string(),
    })
}

fn mcp_read_timeout(tool: &str, arguments: &Value) -> Option<std::time::Duration> {
    if tool == "secret" {
        return arguments
            .get("action")
            .and_then(Value::as_str)
            .and_then(tuic_ipc::secret_response_timeout);
    }
    if tool == "repo"
        && matches!(
            arguments.get("action").and_then(Value::as_str),
            Some("worktree_create" | "worktree_remove")
        )
    {
        // The server allows 301 s for a cold worktree operation. Keep a margin
        // so the server's own timeout response reaches the caller first.
        return Some(std::time::Duration::from_secs(305));
    }
    if tool == "agent"
        && arguments.get("action").and_then(Value::as_str) == Some("send")
        && matches!(
            arguments.get("urgency").and_then(Value::as_str),
            Some("urgent" | "interrupt")
        )
    {
        // The server waits up to 5 s for the recipient to go idle before it
        // replies; the 3 s default would cut the reply of a delivered message.
        return Some(std::time::Duration::from_secs(10));
    }
    if !matches!(tool, "agent" | "session")
        || arguments.get("action").and_then(Value::as_str) != Some("wait")
    {
        return None;
    }
    let wait_ms = arguments
        .get("timeout_ms")
        .and_then(Value::as_u64)
        .filter(|timeout| *timeout > 0)
        .unwrap_or(60_000)
        .min(300_000);
    Some(std::time::Duration::from_millis(wait_ms + 5_000))
}

fn mcp_headers<'a>(
    session: Option<&'a str>,
    tuic_session: Option<&'a str>,
    client_pid: &'a str,
) -> Vec<(&'a str, &'a str)> {
    let mut headers = vec![
        ("Accept", "application/json, text/event-stream"),
        ("x-tuic-client-pid", client_pid),
    ];
    if let Some(sid) = session {
        headers.push(("Mcp-Session-Id", sid));
    }
    if let Some(sid) = tuic_session {
        headers.push(("x-tuic-session", sid));
    }
    headers
}

/// Unwrap a JSON-RPC envelope, then the MCP `content[0].text` payload the
/// tools return. Both layers can carry an error and both are reported verbatim:
/// a caller must never see a success for a message the registry refused.
fn unwrap_tool_text(resp: &ipc::Response) -> Result<String, String> {
    if !resp.is_success() {
        return Err(format!("HTTP {}: {}", resp.status, resp.body));
    }
    let envelope: Value = resp
        .json()
        .map_err(|e| format!("Malformed MCP response: {e}"))?;
    if let Some(err) = envelope.get("error") {
        let message = err
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown MCP error");
        return Err(message.to_string());
    }
    let text = envelope
        .pointer("/result/content/0/text")
        .and_then(Value::as_str)
        .ok_or("MCP response carried no tool payload")?;
    let payload = serde_json::from_str::<Value>(text).ok();
    if envelope.pointer("/result/isError").and_then(Value::as_bool) == Some(true) {
        return Err(payload
            .as_ref()
            .and_then(|value| value.get("error").or_else(|| value.get("message")))
            .and_then(Value::as_str)
            .unwrap_or(text)
            .to_string());
    }
    if let Some(err) = payload
        .as_ref()
        .and_then(|value| value.get("error"))
        .and_then(Value::as_str)
    {
        return Err(err.to_string());
    }
    Ok(text.to_string())
}

/// Open an MCP session and bind it to this PTY's peer identity.
pub struct McpClient {
    session: String,
    tuic_session: Option<String>,
    instructions: Option<String>,
}

impl McpClient {
    /// Initialize the authoritative MCP server over the local IPC socket.
    /// The same header used by tuic-bridge lets the server auto-bind this CLI
    /// invocation to the PTY identity instead of creating a second peer.
    pub fn connect() -> Result<Self, String> {
        let tuic_session = tuic_session();
        let init = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "tuic-cli", "version": env!("CARGO_PKG_VERSION") },
            }
        });
        let resp = post(&init, None, tuic_session.as_deref(), None)?;
        if !resp.is_success() {
            return Err(format!("MCP initialize failed: HTTP {}", resp.status));
        }
        let session = resp
            .header("mcp-session-id")
            .ok_or("MCP initialize returned no Mcp-Session-Id header")?
            .to_string();

        let instructions = resp.json().ok().and_then(|v| {
            v.pointer("/result/instructions")?
                .as_str()
                .map(str::to_owned)
        });

        Ok(Self {
            session,
            tuic_session,
            instructions,
        })
    }

    /// The instructions the server returned from `initialize` for this caller's
    /// session: the one text MCP clients read, so a shell caller can read it too.
    pub fn instructions(&self) -> Option<&str> {
        self.instructions.as_deref()
    }

    /// Establish an MCP-scoped identity when this command did not inherit a
    /// managed terminal identity.
    pub fn connect_for_orchestration() -> Result<Self, String> {
        let client = Self::connect()?;
        if requires_external_registration(client.tuic_session.as_deref()) {
            eprintln!("{EXTERNAL_CALLER_NOTICE}");
            let registered =
                client.call("agent", json!({"action": "register", "name": "tuic-cli"}))?;
            if registered["ok"] != true {
                return Err("External MCP caller registration did not succeed".to_string());
            }
        }
        Ok(client)
    }

    /// Call one server-owned MCP tool and return its unmodified payload.
    pub fn call(&self, tool: &str, arguments: Value) -> Result<Value, String> {
        let text = self.call_text(tool, arguments)?;
        serde_json::from_str(&text).map_err(|e| format!("Malformed tool payload: {e}"))
    }

    /// Call one MCP tool and retain its exact text payload for shell consumers.
    pub fn call_text(&self, tool: &str, arguments: Value) -> Result<String, String> {
        let call = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": { "name": tool, "arguments": arguments }
        });
        let response = post(
            &call,
            Some(&self.session),
            self.tuic_session.as_deref(),
            mcp_read_timeout(tool, &arguments),
        )?;
        unwrap_tool_text(&response)
    }
}

impl Drop for McpClient {
    /// Release the protocol session. Without it every CLI call leaves one
    /// session behind until the server reaper runs: 324k fresh initializes were
    /// never deleted on 2026-09-29/30. Best effort: the call already finished.
    fn drop(&mut self) {
        let pid = client_pid();
        let extra = mcp_headers(Some(&self.session), self.tuic_session.as_deref(), &pid);
        let _ = ipc::request_with_headers_and_timeout(
            "DELETE",
            "/mcp",
            None,
            &extra,
            Some(std::time::Duration::from_secs(2)),
        );
    }
}

/// Human-readable outcome line for a delivery report.
///
/// `accepted`/`ok` only mean "buffered". `delivered` is false exactly when the
/// route is `inbox_only`: nothing will surface the message until the recipient
/// polls. Printing that as "Delivered" is how a reply to an agent with no PTY
/// silently vanished once — so the two cases read differently here.
pub fn delivery_line(to: &str, report: &Value) -> String {
    let route = report["delivery_path"].as_str().unwrap_or("inbox_only");
    if report["delivered"].as_bool() == Some(true) {
        return format!("Delivered to {to} ({route})");
    }
    let mut line =
        format!("Buffered for {to} ({route}) — unread until the recipient polls its inbox");
    if let Some(warning) = report["warning"].as_str() {
        line.push_str(&format!("\nwarning: {warning}"));
    }
    line
}

/// Validate the current registry send contract without reviving removed
/// compatibility fields. Both delivered and inbox-only routes are accepted:
/// the boolean describes whether something surfaced the message, not whether
/// the registry stored it.
fn validate_delivery_report(payload: Value) -> Result<Value, String> {
    let valid = payload.get("message_id").and_then(Value::as_str).is_some()
        && payload.get("delivered").and_then(Value::as_bool).is_some()
        && payload
            .get("delivery_path")
            .and_then(Value::as_str)
            .is_some();
    if valid {
        Ok(payload)
    } else {
        Err(format!(
            "Registry returned a malformed delivery report: {payload}"
        ))
    }
}

/// Deliver `message` to peer `to` through the registry.
///
/// Returns the delivery report so the caller can print the route. The current
/// contract is `message_id` + `delivered` + `delivery_path`; the removed
/// `accepted` field must not be required by this client.
pub fn agent_send_report(
    client: &McpClient,
    to: &str,
    message: &str,
    report: Option<&Value>,
) -> Result<Value, String> {
    let mut arguments = json!({ "action": "send", "to": to, "message": message });
    if let Some(report) = report {
        arguments["report"] = report.clone();
    }
    let payload = client.call("agent", arguments)?;
    validate_delivery_report(payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Catches ordinary IPC expiry before human entry or consent plus execution.
    #[test]
    fn secret_calls_do_not_expire_before_form_and_child_deadlines() {
        for (action, seconds) in [("request", 305), ("run", 425)] {
            assert_eq!(
                mcp_read_timeout("secret", &json!({"action": action})),
                Some(std::time::Duration::from_secs(seconds))
            );
        }
        assert_eq!(
            mcp_read_timeout("secret", &json!({"action": "remove"})),
            None
        );
        assert_eq!(
            mcp_read_timeout("session", &json!({"action": "list"})),
            None
        );
    }

    /// Catches: urgent/interrupt send left on the 3 s default read timeout while
    /// the server waits up to 5 s for idle, or normal send widened needlessly.
    #[test]
    fn urgent_send_waits_for_the_server_idle_budget_plus_margin() {
        for urgency in ["urgent", "interrupt"] {
            assert_eq!(
                mcp_read_timeout("agent", &json!({"action": "send", "urgency": urgency})),
                Some(std::time::Duration::from_secs(10))
            );
        }
        assert_eq!(mcp_read_timeout("agent", &json!({"action": "send"})), None);
        assert_eq!(
            mcp_read_timeout("agent", &json!({"action": "send", "urgency": "normal"})),
            None
        );
    }

    /// Catches: `*t > 0` filter mutated so a zero timeout_ms is accepted as a
    /// real wait (zero read timeout), or the 60 s default dropped.
    #[test]
    fn a_zero_wait_timeout_uses_the_default_read_timeout() {
        for tool in ["agent", "session"] {
            assert_eq!(
                mcp_read_timeout(tool, &json!({"action": "wait", "timeout_ms": 0})),
                Some(std::time::Duration::from_secs(65))
            );
            assert_eq!(
                mcp_read_timeout(tool, &json!({"action": "wait"})),
                Some(std::time::Duration::from_secs(65))
            );
        }
    }

    /// Catches: the +5 s margin mutated to a product or dropped, and the
    /// 300 s cap lost.
    #[test]
    fn the_wait_read_timeout_is_the_wait_plus_five_seconds() {
        assert_eq!(
            mcp_read_timeout("agent", &json!({"action": "wait", "timeout_ms": 10_000})),
            Some(std::time::Duration::from_secs(15))
        );
        assert_eq!(
            mcp_read_timeout("session", &json!({"action": "wait", "timeout_ms": 1})),
            Some(std::time::Duration::from_millis(5_001))
        );
        assert_eq!(
            mcp_read_timeout("agent", &json!({"action": "wait", "timeout_ms": 900_000})),
            Some(std::time::Duration::from_secs(305))
        );
    }

    #[test]
    fn managed_cli_identity_is_sent_as_the_bridge_header() {
        let headers = mcp_headers(Some("mcp-1"), Some("peer-1"), "4242");
        assert!(headers.contains(&("Mcp-Session-Id", "mcp-1")));
        assert!(headers.contains(&("x-tuic-session", "peer-1")));
    }

    /// Catches: a session storm that cannot be traced to a process because the
    /// CLI never says which pid opened the connection.
    #[test]
    fn every_cli_request_names_its_process() {
        let headers = mcp_headers(None, None, "4242");
        assert!(headers.contains(&("x-tuic-client-pid", "4242")));
    }

    #[test]
    fn an_external_cli_call_requires_headerless_registration() {
        assert!(requires_external_registration(None));
        assert!(!requires_external_registration(Some("managed-peer")));
        assert_eq!(
            EXTERNAL_CALLER_NOTICE,
            "tuic: no TUIC_SESSION; registering an external MCP caller"
        );
    }

    fn response(status: u16, body: &str, headers: &[(&str, &str)]) -> ipc::Response {
        ipc::Response::from(tuic_ipc::http::Response {
            raw_headers: String::new(),
            status,
            body: body.to_string(),
            headers: headers
                .iter()
                .map(|(k, v)| (k.to_ascii_lowercase(), v.to_string()))
                .collect(),
        })
    }

    #[test]
    fn header_lookup_is_case_insensitive() {
        let resp = response(200, "", &[("Mcp-Session-Id", "abc")]);
        assert_eq!(resp.header("mcp-session-id"), Some("abc"));
        assert_eq!(resp.header("MCP-SESSION-ID"), Some("abc"));
        assert_eq!(resp.header("absent"), None);
    }

    #[test]
    fn tool_payload_is_unwrapped_through_both_envelopes() {
        let resp = response(
            200,
            r#"{"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text",
               "text":"{\"accepted\":true,\"delivery_path\":\"sse_channel_and_inbox\"}"}]}}"#,
            &[],
        );
        let payload = unwrap_tool_text(&resp).expect("payload");
        assert_eq!(
            payload,
            r#"{"accepted":true,"delivery_path":"sse_channel_and_inbox"}"#
        );
    }

    /// The whole point of the story: a refusal must not read as a success.
    #[test]
    fn a_tool_level_error_is_surfaced_verbatim() {
        let resp = response(
            200,
            r#"{"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text",
               "text":"{\"error\":\"Recipient not found\"}"}]}}"#,
            &[],
        );
        assert_eq!(unwrap_tool_text(&resp).unwrap_err(), "Recipient not found");
    }

    #[test]
    fn a_jsonrpc_level_error_is_surfaced_verbatim() {
        let resp = response(
            200,
            r#"{"jsonrpc":"2.0","id":3,"error":{"code":-32000,
               "message":"TUICommander IPC request failed"}}"#,
            &[],
        );
        assert_eq!(
            unwrap_tool_text(&resp).unwrap_err(),
            "TUICommander IPC request failed"
        );
    }

    #[test]
    fn a_live_route_reads_as_delivered() {
        let report = json!({
            "message_id": "message-1",
            "delivered": true,
            "delivery_path": "sse_channel_and_inbox",
        });
        let report = validate_delivery_report(report).expect("valid current contract");
        assert_eq!(
            delivery_line("peer-1", &report),
            "Delivered to peer-1 (sse_channel_and_inbox)"
        );
    }

    /// `accepted` alone means "buffered". Announcing that as delivery is how a
    /// reply to a peer with no PTY vanished in silence — so it must not read
    /// as delivered, and the warning the registry sends must reach the user.
    #[test]
    fn an_inbox_only_route_is_never_announced_as_delivered() {
        let report = json!({
            "message_id": "message-1",
            "delivered": false,
            "delivery_path": "inbox_only",
            "warning": "Recipient has no live channel",
        });
        let report = validate_delivery_report(report).expect("valid inbox-only contract");
        let line = delivery_line("peer-1", &report);
        assert!(!line.contains("Delivered"), "{line}");
        assert!(line.contains("inbox_only"), "{line}");
        assert!(line.contains("Recipient has no live channel"), "{line}");
    }

    /// A report that omits the flag is unproven, not proven good.
    #[test]
    fn a_report_without_a_delivered_flag_is_not_delivered() {
        let line = delivery_line("peer-1", &json!({ "accepted": true }));
        assert!(!line.contains("Delivered"), "{line}");
    }

    #[test]
    fn a_malformed_delivery_report_is_rejected() {
        let error = validate_delivery_report(json!({
            "message_id": "message-1",
            "delivery_path": "wake_notification_and_inbox",
        }))
        .unwrap_err();
        assert!(error.contains("malformed delivery report"), "{error}");
    }

    #[test]
    fn a_non_2xx_response_is_never_a_success() {
        let resp = response(503, "unavailable", &[]);
        assert!(unwrap_tool_text(&resp).unwrap_err().starts_with("HTTP 503"));
    }
}
