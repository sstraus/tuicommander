use std::io::Write;
use std::process::{Command, Stdio};

fn offline_response(method: &str, protocol_version: &str) -> serde_json::Value {
    let socket_dir = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
    let mut bridge = Command::new(env!("CARGO_BIN_EXE_tuic-bridge"))
        .env("TUIC_SOCKET", socket_dir.path().join("missing.sock"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": method,
        "params": {
            "name": "search_tools",
            "arguments": { "query": "session" },
            "_meta": { "io.modelcontextprotocol/protocolVersion": protocol_version }
        }
    });
    writeln!(bridge.stdin.take().unwrap(), "{request}").unwrap();
    let output = bridge.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response = String::from_utf8(output.stdout).unwrap();
    serde_json::from_str(
        response
            .lines()
            .find(|line| line.contains("\"id\":7"))
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn offline_current_tool_error_is_a_complete_result() {
    let result = offline_response("tools/call", "2026-07-28");
    assert_eq!(result["result"]["resultType"], "complete");
    assert_eq!(result["result"]["isError"], true);
    assert!(
        result["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("unavailable")
    );
}

#[test]
fn offline_legacy_tool_error_keeps_its_shape() {
    let result = offline_response("tools/call", "2025-11-25");
    assert!(result["result"].get("resultType").is_none());
    assert_eq!(result["result"]["isError"], true);
}

#[test]
fn offline_current_tool_list_is_a_complete_private_cache_result() {
    let result = offline_response("tools/list", "2026-07-28");
    assert_eq!(result["result"]["tools"], serde_json::json!([]));
    assert_eq!(result["result"]["resultType"], "complete");
    assert_eq!(result["result"]["ttlMs"], 0);
    assert_eq!(result["result"]["cacheScope"], "private");
}

/// Catches: the offline bridge claims that a method the online server supports
/// does not exist (-32601), so the client drops TUIC as a server without MCP
/// 2026-07-28 instead of retrying later (story 3044-ddeb).
#[test]
fn offline_handshake_methods_answer_unavailable_not_method_not_found() {
    for method in ["server/discover", "ping"] {
        let response = offline_response(method, "2026-07-28");
        assert_eq!(response["error"]["code"], -32000, "{method}: {response}");
        assert_eq!(
            response["error"]["message"], "TUICommander MCP is unavailable",
            "{method}: {response}"
        );
    }
}

#[test]
fn offline_legacy_tool_list_keeps_its_shape() {
    let result = offline_response("tools/list", "2025-11-25");
    assert_eq!(result["result"]["tools"], serde_json::json!([]));
    assert!(result["result"].get("resultType").is_none());
    assert!(result["result"].get("ttlMs").is_none());
    assert!(result["result"].get("cacheScope").is_none());
}
