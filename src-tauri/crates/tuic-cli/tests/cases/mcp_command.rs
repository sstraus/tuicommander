#![cfg(unix)]

use std::io::Write;
use std::os::unix::net::UnixListener;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static NEXT_SOCKET: AtomicU64 = AtomicU64::new(0);

fn socket() -> (std::path::PathBuf, UnixListener) {
    let root =
        std::path::PathBuf::from(std::env::var("HOME").unwrap()).join("Gits/.tmp/tuic-tests");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join(format!(
        "tuic-mcp-{}-{}.sock",
        std::process::id(),
        NEXT_SOCKET.fetch_add(1, Ordering::Relaxed)
    ));
    let listener = UnixListener::bind(&path).unwrap();
    (path, listener)
}

fn read_request(stream: &mut std::os::unix::net::UnixStream) -> (Vec<String>, serde_json::Value) {
    let request = tuic_test_support::read_http_request(stream).unwrap();
    let mut headers = vec![format!("{}\r\n", request.request_line)];
    headers.extend(request.headers);
    (
        headers,
        if request.body.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_slice(&request.body).unwrap()
        },
    )
}

fn respond(
    stream: &mut std::os::unix::net::UnixStream,
    body: &str,
    init: bool,
) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n{}Connection: close\r\n\r\n{body}",
        body.len(),
        if init {
            "Mcp-Session-Id: test-session\r\n"
        } else {
            ""
        }
    )
}

fn tool_response(text: &str, is_error: bool) -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "result": {"isError": is_error, "content": [{"type": "text", "text": text}]}
    })
    .to_string()
}

fn run_with_stub(
    args: &[&str],
    stdin: Option<&str>,
    session: Option<&str>,
    tool_text: &str,
    is_error: bool,
) -> (Output, Vec<(Vec<String>, serde_json::Value)>) {
    run_with_stub_delay(args, stdin, session, tool_text, is_error, Duration::ZERO)
}

fn run_with_stub_delay(
    args: &[&str],
    stdin: Option<&str>,
    session: Option<&str>,
    tool_text: &str,
    is_error: bool,
    delay: Duration,
) -> (Output, Vec<(Vec<String>, serde_json::Value)>) {
    let (path, listener) = socket();
    let response = tool_response(tool_text, is_error);
    let external = session.is_none();
    let with_health = !matches!(args.first(), Some(&"mcp" | &"automations"));
    let server = std::thread::spawn(move || {
        let mut requests = Vec::new();
        let total = (if external { 3 } else { 2 }) + usize::from(with_health) + 1;
        for index in 0..total {
            let (mut stream, _) = listener.accept().unwrap();
            requests.push(read_request(&mut stream));
            let protocol_index = index.saturating_sub(usize::from(with_health));
            let body = if index + 1 == total {
                // The CLI's DELETE /mcp that releases its protocol session.
                String::new()
            } else if with_health && index == 0 {
                r#"{"ok":true}"#.to_string()
            } else if protocol_index == 0 {
                r#"{"jsonrpc":"2.0","result":{}}"#.to_string()
            } else if external && protocol_index == 1 {
                tool_response(r#"{"ok":true}"#, false)
            } else {
                std::thread::sleep(delay);
                response.clone()
            };
            let reply = respond(&mut stream, &body, index == usize::from(with_health));
            if delay.is_zero() || body != response {
                reply.unwrap();
            } else if let Err(error) = reply {
                assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe, "{error}");
            }
        }
        requests
    });
    let mut command = Command::new(env!("CARGO_BIN_EXE_tuic"));
    command
        .args(args)
        .env("TUIC_SOCKET", &path)
        .env_remove("TUIC_SESSION");
    if let Some(sid) = session {
        command.env("TUIC_SESSION", sid);
    }
    let output = if let Some(input) = stdin {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    } else {
        command.output().unwrap()
    };
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("TUICommander did not start"),
        "CLI did not call MCP: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_ne!(
        output.status.code(),
        Some(2),
        "CLI rejected valid MCP command: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let requests = server.join().unwrap();
    std::fs::remove_file(path).unwrap();
    (output, requests)
}

#[test]
fn agent_stats_uses_the_equivalent_single_request_http_route() {
    let (path, listener) = socket();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let health = read_request(&mut stream);
        respond(&mut stream, r#"{"ok":true}"#, false).unwrap();
        let (mut stream, _) = listener.accept().unwrap();
        let stats = read_request(&mut stream);
        respond(
            &mut stream,
            r#"{"active_sessions":2,"max_sessions":4,"available_slots":2}"#,
            false,
        )
        .unwrap();
        (health, stats)
    });
    let output = Command::new(env!("CARGO_BIN_EXE_tuic"))
        .args(["agent", "stats", "--json"])
        .env("TUIC_SOCKET", &path)
        .output()
        .unwrap();
    let (health, stats) = server.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        health.0[0].starts_with("GET /health HTTP/1.1"),
        "{:?}",
        health.0
    );
    assert!(
        stats.0[0].starts_with("GET /stats HTTP/1.1"),
        "{:?}",
        stats.0
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["available_slots"], 2);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn renamed_cli_inputs_reach_mcp_with_path_and_branch() {
    let (output, requests) = run_with_stub(
        &["agent", "list-peers", "--path", "/repo"],
        None,
        Some("peer-1"),
        r#"{"peers":[]}"#,
        false,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(requests[2].1["params"]["arguments"]["path"], "/repo");
    assert!(
        requests[2].1["params"]["arguments"]
            .get("project")
            .is_none()
    );

    let (output, requests) = run_with_stub(
        &["repo", "worktree-remove", ".", "feature"],
        None,
        Some("peer-1"),
        r#"{"ok":true}"#,
        false,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(requests[2].1["params"]["arguments"]["branch"], "feature");
    assert!(
        requests[2].1["params"]["arguments"]
            .get("workspace_id")
            .is_none()
    );
}

#[test]
fn mcp_agent_wait_keeps_the_socket_open_for_eight_seconds() {
    let start = Instant::now();
    let (output, requests) = run_with_stub_delay(
        &["mcp", "agent", r#"{"action":"wait","timeout_ms":8000}"#],
        None,
        Some("peer-1"),
        r#"{"timed_out":true}"#,
        false,
        Duration::from_secs(8),
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(start.elapsed() >= Duration::from_secs(8));
    assert_eq!(requests[1].1["params"]["arguments"]["timeout_ms"], 8000);
}

#[test]
fn secret_request_accepts_user_entry_after_four_seconds() {
    // Catches the real CLI socket dropping a user-entry request after 3 seconds.
    let (output, requests) = run_with_stub_delay(
        &[
            "mcp",
            "secret",
            r#"{"action":"request","fields":[{"name":"TEST_USER","kind":"username"}],"reason":"IPC regression"}"#,
        ],
        None,
        Some("peer-1"),
        r#"{"names":["TEST_USER"],"status":"declined"}"#,
        false,
        Duration::from_secs(4),
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        result,
        serde_json::json!({"names": ["TEST_USER"], "status": "declined"})
    );
    assert_eq!(requests[1].1["params"]["name"], "secret");
}

#[test]
fn mcp_worktree_remove_accepts_a_reply_after_four_seconds() {
    let (output, requests) = run_with_stub_delay(
        &[
            "mcp",
            "repo",
            r#"{"action":"worktree_remove","path":"/repo","branch":"feature"}"#,
        ],
        None,
        Some("peer-1"),
        r#"{"ok":true}"#,
        false,
        Duration::from_secs(4),
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "{\"ok\":true}\n");
    assert_eq!(
        requests[1].1["params"]["arguments"]["action"],
        "worktree_remove"
    );
}

#[test]
fn mcp_worktree_create_accepts_a_reply_after_four_seconds() {
    let (output, requests) = run_with_stub_delay(
        &[
            "mcp",
            "repo",
            r#"{"action":"worktree_create","path":"/repo","branch":"feature"}"#,
        ],
        None,
        Some("peer-1"),
        r#"{"ok":true}"#,
        false,
        Duration::from_secs(4),
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "{\"ok\":true}\n");
    assert_eq!(
        requests[1].1["params"]["arguments"]["action"],
        "worktree_create"
    );
}

#[test]
fn mcp_short_read_timeout_warns_that_the_action_may_finish() {
    let (output, _) = run_with_stub_delay(
        &["mcp", "session", r#"{"action":"list"}"#],
        None,
        Some("peer-1"),
        r#"{"sessions":[]}"#,
        false,
        Duration::from_secs(4),
    );
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("may still complete"), "{error}");
    assert!(
        !error.contains("Resource temporarily unavailable"),
        "{error}"
    );
}

#[test]
fn ls_still_times_out_against_a_server_that_never_replies() {
    let (path, listener) = socket();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_request(&mut stream);
        std::thread::sleep(Duration::from_secs(4));
        request
    });
    let start = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_tuic"))
        .args(["ls", "--json"])
        .env("TUIC_SOCKET", &path)
        .output()
        .unwrap();
    let elapsed = start.elapsed();
    let request = server.join().unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(request.0[0].starts_with("GET /sessions HTTP/1.1"));
    assert!(!output.status.success());
    assert!(elapsed >= Duration::from_millis(2500), "{elapsed:?}");
    assert!(elapsed < Duration::from_millis(4500), "{elapsed:?}");
}

#[test]
fn mcp_prints_tool_text_without_reencoding() {
    let (output, requests) = run_with_stub(
        &["mcp", "session", r#"{"action":"list"}"#],
        None,
        Some("peer-1"),
        "{\"a\":1, \"b\":2}",
        false,
    );
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "{\"a\":1, \"b\":2}\n"
    );
    assert_eq!(requests[1].1["params"]["name"], "session");
    assert_eq!(
        requests[1].1["params"]["arguments"],
        serde_json::json!({"action":"list"})
    );
}

#[test]
fn mcp_reports_is_error_on_stderr_with_failure_exit() {
    let (output, _) = run_with_stub(
        &["mcp", "agent", r#"{"action":"send"}"#],
        None,
        Some("peer-1"),
        r#"{"message":"Recipient not found"}"#,
        true,
    );
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, [] as [u8; 0]);
    assert!(String::from_utf8_lossy(&output.stderr).contains("Recipient not found"));
}

#[test]
fn mcp_reports_payload_error_on_stderr_with_failure_exit() {
    let (output, _) = run_with_stub(
        &["mcp", "agent", r#"{"action":"send"}"#],
        None,
        Some("peer-1"),
        r#"{"error":"Recipient not found"}"#,
        false,
    );
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, [] as [u8; 0]);
    assert!(String::from_utf8_lossy(&output.stderr).contains("Recipient not found"));
}

/// Catches: every CLI call leaving its MCP protocol session behind (324k
/// fresh initializes were never deleted on 2026-09-29/30).
#[test]
fn mcp_deletes_its_protocol_session_after_the_call() {
    let (output, requests) = run_with_stub(
        &["mcp", "session", "{}"],
        None,
        Some("peer-1"),
        "{\"a\":1}",
        false,
    );
    assert!(output.status.success());
    let (headers, _) = requests.last().unwrap();
    assert!(
        headers[0].starts_with("DELETE /mcp HTTP/1.1"),
        "{headers:?}"
    );
    assert!(
        headers
            .iter()
            .any(|line| line.eq_ignore_ascii_case("mcp-session-id: test-session\r\n")),
        "{headers:?}"
    );
}

/// Catches: a storm that cannot be traced to a process because the CLI never
/// sends its pid.
#[test]
fn mcp_initialize_names_the_cli_process() {
    let (_, requests) = run_with_stub(
        &["mcp", "session", "{}"],
        None,
        Some("peer-1"),
        "{\"a\":1}",
        false,
    );
    let prefix = "x-tuic-client-pid: ";
    let pid = requests[0]
        .0
        .iter()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix(prefix)
                .map(str::to_owned)
        })
        .expect("initialize carries x-tuic-client-pid");
    assert!(pid.trim().parse::<u32>().is_ok(), "{pid:?}");
}

#[test]
fn mcp_sends_managed_identity_header() {
    let (output, requests) = run_with_stub(
        &["mcp", "session", "{}"],
        None,
        Some("peer-1"),
        "{\"a\":1}",
        false,
    );
    assert!(output.status.success());
    assert_eq!(requests.len(), 3);
    assert!(requests.iter().all(|(headers, _)| {
        headers
            .iter()
            .any(|line| line.eq_ignore_ascii_case("x-tuic-session: peer-1\r\n"))
    }));
}

#[test]
fn mcp_registers_external_caller_before_tool() {
    let (output, requests) =
        run_with_stub(&["mcp", "session", "{}"], None, None, "{\"a\":1}", false);
    assert!(output.status.success());
    assert_eq!(requests[1].1["params"]["name"], "agent");
    assert_eq!(requests[1].1["params"]["arguments"]["action"], "register");
    assert_eq!(requests[2].1["params"]["name"], "session");
    assert!(requests.iter().all(|(headers, _)| {
        !headers
            .iter()
            .any(|line| line.to_ascii_lowercase().starts_with("x-tuic-session:"))
    }));
}

#[test]
fn mcp_reads_stdin_json_with_apostrophe() {
    let (output, requests) = run_with_stub(
        &["mcp", "agent", "-"],
        Some("{\"message\":\"it's ready\"}"),
        Some("peer-1"),
        "{\"a\":1}",
        false,
    );
    assert!(output.status.success());
    assert_eq!(
        requests[1].1["params"]["arguments"]["message"],
        "it's ready"
    );
}

#[test]
fn mcp_rejects_bad_input_before_connecting() {
    let nonexistent = std::path::PathBuf::from(std::env::var("HOME").unwrap())
        .join("Gits/.tmp/tuic-tests/no-mcp.sock");
    for args in [
        vec!["mcp", "agent", "{"],
        vec!["mcp", "agent", "{}", "extra"],
        vec!["mcp"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_tuic"))
            .args(args)
            .env("TUIC_SOCKET", &nonexistent)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(2),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, [] as [u8; 0]);
        assert!(!String::from_utf8_lossy(&output.stderr).contains("Cannot connect"));
    }
}

// Catches: automation CLI reshaping action inputs or hiding the backend validation error.
#[test]
fn automations_command_preserves_shared_action_values_and_errors() {
    for (input, reply, error) in [
        (
            r#"{"action":"preset","preset":{"kind":"daily","hour":9,"minute":30}}"#,
            r#"{"cron":"30 9 * * *"}"#,
            false,
        ),
        (
            r#"{"action":"update","id":"absent","enabled":false}"#,
            r#"{"error":"Automation not found"}"#,
            true,
        ),
        (
            r#"{"action":"list","unexpected":true}"#,
            r#"{"error":"Invalid automation action: unknown field `unexpected`, there are no fields"}"#,
            true,
        ),
    ] {
        let (output, requests) =
            run_with_stub(&["automations", input], None, Some("peer-1"), reply, error);
        assert_eq!(requests[1].1["params"]["name"], "automations");
        assert_eq!(
            requests[1].1["params"]["arguments"],
            serde_json::from_str::<serde_json::Value>(input).unwrap()
        );
        assert_eq!(output.status.success(), !error);
        if error {
            let value: serde_json::Value = serde_json::from_str(reply).unwrap();
            assert_eq!(
                String::from_utf8_lossy(&output.stderr),
                format!("tuic: {}\n", value["error"].as_str().unwrap())
            );
            assert_eq!(output.stdout, [] as [u8; 0]);
        } else {
            assert_eq!(
                String::from_utf8_lossy(&output.stdout),
                format!("{reply}\n")
            );
            assert_eq!(output.stderr, [] as [u8; 0]);
        }
    }
    let (output, requests) = run_with_stub(
        &["automations", "-"],
        Some(r#"{"action":"list"}"#),
        Some("peer-1"),
        "[]",
        false,
    );
    assert!(output.status.success());
    assert_eq!(
        requests[1].1["params"]["arguments"],
        serde_json::json!({"action":"list"})
    );
}
