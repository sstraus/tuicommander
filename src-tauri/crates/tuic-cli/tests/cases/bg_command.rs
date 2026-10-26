#![cfg(unix)]

use std::io::Write;
use std::os::unix::net::UnixListener;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static NEXT_JOB: AtomicU64 = AtomicU64::new(0);

fn test_path(name: &str) -> std::path::PathBuf {
    let root = std::path::PathBuf::from(std::env::var("TUIC_TEST_TMP_ROOT").unwrap());
    std::fs::create_dir_all(&root).unwrap();
    root.join(format!(
        "bg-{name}-{}-{}",
        std::process::id(),
        NEXT_JOB.fetch_add(1, Ordering::Relaxed)
    ))
}

fn socket_path(name: &str) -> std::path::PathBuf {
    tuic_test_support::short_socket_test_temp_root().join(format!(
        "bg-{name}-{}-{}",
        std::process::id(),
        NEXT_JOB.fetch_add(1, Ordering::Relaxed)
    ))
}

fn bg_command(log: &std::path::Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tuic"));
    command.env("TUIC_BG_WAKE_DIR", format!("{}.markers", log.display()));
    command
}

fn read_request(stream: &mut std::os::unix::net::UnixStream) -> (String, serde_json::Value) {
    let request = tuic_test_support::read_http_request(stream).unwrap();
    (
        request.request_line,
        if request.body.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_slice(&request.body).unwrap()
        },
    )
}

fn reply(stream: &mut std::os::unix::net::UnixStream, body: &str) {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
}

fn wait_for(path: &std::path::Path) {
    let deadline = Instant::now() + Duration::from_secs(12);
    while !path.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(path.exists(), "{} was not written", path.display());
}

fn wait_for_terminal_wake(path: &std::path::Path) -> serde_json::Value {
    let deadline = Instant::now() + Duration::from_secs(90);
    while Instant::now() < deadline {
        if let Ok(contents) = std::fs::read(path) {
            let status: serde_json::Value = serde_json::from_slice(&contents).unwrap();
            if status["status"] != "retrying" {
                return status;
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("{} never reached a terminal wake state", path.display());
}

#[test]
fn bg_returns_before_command_exits_and_queues_one_exact_wake() {
    let log = test_path("wake.log");
    std::fs::write(&log, "previous\n").unwrap();
    let wake_file = std::path::PathBuf::from(format!("{}.wake", log.display()));
    std::fs::write(&wake_file, r#"{"status":"failed","error":"stale"}"#).unwrap();
    let socket = socket_path("wake.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let first = read_request(&mut stream);
        reply(
            &mut stream,
            r#"[{"session_id":"pty-1","tuic_session":"caller-1"}]"#,
        );
        let (mut stream, _) = listener.accept().unwrap();
        let second = read_request(&mut stream);
        reply(&mut stream, r#"{"typed":false,"queued":1}"#);
        listener.set_nonblocking(true).unwrap();
        std::thread::sleep(Duration::from_millis(250));
        assert!(listener.accept().is_err(), "duplicate wake request");
        (first, second)
    });
    // Warm the freshly linked executable before measuring only launcher behavior.
    bg_command(&log).arg("--version").output().unwrap();
    let start = Instant::now();
    let output = bg_command(&log)
        .args([
            "bg",
            log.to_str().unwrap(),
            "--",
            "sh",
            "-c",
            "sleep 2; echo body; echo problem >&2; exit 7",
        ])
        .env("TUIC_SESSION", "caller-1")
        .env("TUIC_SOCKET", &socket)
        .output()
        .unwrap();
    let launch_time = start.elapsed();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        launch_time < Duration::from_secs(1),
        "launcher blocked for {launch_time:?}"
    );
    let exit_file = std::path::PathBuf::from(format!("{}.exit", log.display()));
    assert!(
        !exit_file.exists(),
        "launcher wrote the exit before the command ended"
    );
    assert!(!wake_file.exists(), "launcher left a stale wake failure");
    let active_marker: serde_json::Value = serde_json::from_slice(
        &std::fs::read(format!("{}.markers/caller-1.json", log.display())).unwrap(),
    )
    .unwrap();
    assert_eq!(active_marker["status"], "retrying");
    assert_eq!(active_marker["session_id"], "caller-1");
    wait_for(&exit_file);
    wait_for(&wake_file);
    assert_eq!(std::fs::read_to_string(&exit_file).unwrap().trim(), "7");
    let wake_status: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&wake_file).unwrap()).unwrap();
    assert_eq!(wake_status["status"], "queued");
    let log_text = std::fs::read_to_string(&log).unwrap();
    assert!(
        log_text.contains("previous\n")
            && log_text.contains("body")
            && log_text.contains("problem")
    );
    let (first, second) = server.join().unwrap();
    assert!(
        !std::fs::read_to_string(&log)
            .unwrap()
            .contains("wake failed")
    );
    assert_eq!(first.0, "GET /sessions HTTP/1.1");
    assert_eq!(second.0, "POST /sessions/pty-1/queue HTTP/1.1");
    let wake = second.1["text"].as_str().unwrap();
    assert!(
        wake.starts_with(&format!("BG DONE exit=7 log={} cmd=sh -c", log.display())),
        "{wake}"
    );
    std::fs::remove_file(exit_file).unwrap();
    std::fs::remove_file(wake_file).unwrap();
    std::fs::remove_file(log).unwrap();
    std::fs::remove_file(socket).unwrap();
}

// Catches: a detached reviewer inherits the foreground peer and can block its
// composer via progress, stranding the runner's own BG DONE behind that wait.
#[test]
fn bg_command_drops_parent_peer_identity_but_runner_still_wakes_caller_2100() {
    let log = test_path("peer-isolation.log");
    let socket = socket_path("peer-isolation.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        assert_eq!(read_request(&mut stream).0, "GET /sessions HTTP/1.1");
        reply(
            &mut stream,
            r#"[{"session_id":"pty-1","tuic_session":"caller-1"}]"#,
        );
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_request(&mut stream);
        assert_eq!(request.0, "POST /sessions/pty-1/queue HTTP/1.1");
        reply(&mut stream, r#"{"accepted":true}"#);
        request.1
    });
    let output = bg_command(&log)
        .args(["bg", log.to_str().unwrap(), "--", "env"])
        .env("TUIC_SESSION", "caller-1")
        .env("TUIC_PARENT", "foreground-parent")
        .env("TUIC_SOCKET", &socket)
        .env("BG_KEEP_CONFIG", "preserved")
        .output()
        .unwrap();
    assert!(output.status.success());
    let wake_file = std::path::PathBuf::from(format!("{}.wake", log.display()));
    let status = wait_for_terminal_wake(&wake_file);
    let request = server.join().unwrap();
    assert_eq!(status["status"], "queued");
    assert_eq!(status["tuic_session"], "caller-1");
    assert!(
        request["text"]
            .as_str()
            .unwrap()
            .starts_with("BG DONE exit=0 ")
    );
    let contents = std::fs::read_to_string(&log).unwrap();
    assert!(
        contents
            .lines()
            .any(|line| line == "BG_KEEP_CONFIG=preserved")
    );
    assert!(
        contents
            .lines()
            .any(|line| line == format!("TUIC_SOCKET={}", socket.display()))
    );
    assert!(contents.lines().any(|line| line == "TUIC_BG_COMMAND=1"));
    for key in ["TUIC_SESSION=", "TUIC_PARENT="] {
        assert!(
            !contents.lines().any(|line| line.starts_with(key)),
            "background command inherited foreground identity {key}"
        );
    }
    std::fs::remove_file(format!("{}.exit", log.display())).unwrap();
    std::fs::remove_file(wake_file).unwrap();
    std::fs::remove_file(&log).unwrap();
    std::fs::remove_file(socket).unwrap();
    std::fs::remove_dir_all(format!("{}.markers", log.display())).unwrap();
}

#[test]
fn bg_retries_after_a_socket_read_timeout_and_queues_the_wake() {
    let log = test_path("retry-timeout.log");
    let wake_file = std::path::PathBuf::from(format!("{}.wake", log.display()));
    let socket = socket_path("retry-timeout.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server_wake_file = wake_file.clone();
    let marker_dir = std::path::PathBuf::from(format!("{}.markers", log.display()));
    let marker = marker_dir.join("caller-1.json");
    let server_marker = marker.clone();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let first = read_request(&mut stream);
        assert_eq!(first.0, "GET /sessions HTTP/1.1");
        // The real Unix socket read deadline is three seconds. Withhold the
        // first response long enough for the client to observe EAGAIN.
        std::thread::sleep(Duration::from_secs(4));
        drop(stream);

        let (mut stream, _) = listener.accept().unwrap();
        let fallback = read_request(&mut stream);
        assert_eq!(fallback.0, "POST /mcp HTTP/1.1");
        drop(stream);

        listener.set_nonblocking(true).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut requests = Vec::new();
        while requests.len() < 2 && Instant::now() < deadline {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let request = read_request(&mut stream);
                    if request.0 == "GET /sessions HTTP/1.1" {
                        let in_progress: serde_json::Value =
                            serde_json::from_slice(&std::fs::read(&server_wake_file).unwrap())
                                .unwrap();
                        assert_eq!(in_progress["status"], "retrying");
                        assert_eq!(in_progress["tuic_session"], "caller-1");
                        assert_eq!(in_progress["attempts"], 1);
                        let marker: serde_json::Value =
                            serde_json::from_slice(&std::fs::read(&server_marker).unwrap())
                                .unwrap();
                        assert_eq!(marker["session_id"], "caller-1");
                        assert_eq!(marker["status"], "retrying");
                        reply(
                            &mut stream,
                            r#"[{"session_id":"pty-1","tuic_session":"caller-1"}]"#,
                        );
                    } else if request.0 == "POST /sessions/pty-1/queue HTTP/1.1" {
                        reply(&mut stream, r#"{"typed":false,"queued":1}"#);
                    }
                    requests.push(request.0);
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(error) => panic!("accept failed: {error}"),
            }
        }
        requests
    });

    bg_command(&log).arg("--version").output().unwrap();
    let launch = bg_command(&log)
        .args(["bg", log.to_str().unwrap(), "--", "sh", "-c", "exit 0"])
        .env("TUIC_SESSION", "caller-1")
        .env("TUIC_SOCKET", &socket)
        .output()
        .unwrap();
    assert!(launch.status.success());
    let outcome = wait_for_terminal_wake(&wake_file);
    let requests = server.join().unwrap();
    assert_eq!(outcome["status"], "queued", "{outcome}");
    assert_eq!(outcome["tuic_session"], "caller-1");
    assert!(outcome["attempts"].as_u64().unwrap_or(0) >= 2, "{outcome}");
    let final_marker: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&marker).unwrap()).unwrap();
    assert_eq!(final_marker["status"], "queued");
    assert_eq!(final_marker["session_id"], "caller-1");
    assert_eq!(
        requests,
        [
            "GET /sessions HTTP/1.1",
            "POST /sessions/pty-1/queue HTTP/1.1"
        ]
    );
    std::fs::remove_file(format!("{}.exit", log.display())).unwrap();
    std::fs::remove_file(wake_file).unwrap();
    std::fs::remove_file(log).unwrap();
    std::fs::remove_file(socket).unwrap();
    std::fs::remove_dir_all(marker_dir).unwrap();
}

#[test]
fn bg_mails_completion_when_session_lookup_cannot_find_caller() {
    let log = test_path("unbound.log");
    let socket = socket_path("unbound.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let lookup = read_request(&mut stream);
        reply(&mut stream, "[]");
        let (mut stream, _) = listener.accept().unwrap();
        let initialize = read_request(&mut stream);
        let init_body = r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","capabilities":{},"serverInfo":{"name":"test","version":"1"}}}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nMcp-Session-Id: mcp-1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{init_body}",
            init_body.len()
        )
        .unwrap();
        let (mut stream, _) = listener.accept().unwrap();
        let mail = read_request(&mut stream);
        let report = serde_json::json!({
            "message_id": "mail-1",
            "delivered": true,
            "delivery_path": "wake_notification_and_inbox"
        });
        let response = serde_json::json!({
            "jsonrpc": "2.0", "id": 2,
            "result": {"content": [{"type": "text", "text": report.to_string()}]}
        });
        reply(&mut stream, &response.to_string());
        (lookup, initialize, mail)
    });
    let output = bg_command(&log)
        .args(["bg", log.to_str().unwrap(), "--", "sh", "-c", "echo done"])
        .env("TUIC_SESSION", "caller-1")
        .env("TUIC_SOCKET", &socket)
        .output()
        .unwrap();
    assert!(output.status.success());
    let exit_file = std::path::PathBuf::from(format!("{}.exit", log.display()));
    let wake_file = std::path::PathBuf::from(format!("{}.wake", log.display()));
    wait_for(&wake_file);
    assert_eq!(std::fs::read_to_string(&exit_file).unwrap().trim(), "0");
    let wake_status: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&wake_file).unwrap()).unwrap();
    assert_eq!(wake_status["status"], "mailed");
    let (lookup, initialize, mail) = server.join().unwrap();
    assert_eq!(lookup.0, "GET /sessions HTTP/1.1");
    assert_eq!(initialize.0, "POST /mcp HTTP/1.1");
    assert_eq!(initialize.1["method"], "initialize");
    assert_eq!(mail.0, "POST /mcp HTTP/1.1");
    assert_eq!(mail.1["params"]["name"], "agent");
    assert_eq!(mail.1["params"]["arguments"]["action"], "send");
    assert_eq!(mail.1["params"]["arguments"]["to"], "caller-1");
    let message = mail.1["params"]["arguments"]["message"].as_str().unwrap();
    assert!(message.contains("BG DONE exit=0"));
    assert!(message.contains("wake failed"));
    assert!(message.contains("found 0"));
    std::fs::remove_file(exit_file).unwrap();
    std::fs::remove_file(wake_file).unwrap();
    std::fs::remove_file(log).unwrap();
    std::fs::remove_file(socket).unwrap();
}

#[test]
fn bg_does_not_call_inbox_only_mail_a_wake() {
    let log = test_path("inbox-only.log");
    let socket = socket_path("inbox-only.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let lookup = read_request(&mut stream);
        reply(&mut stream, "[]");
        let (mut stream, _) = listener.accept().unwrap();
        let initialize = read_request(&mut stream);
        let init_body = r#"{"jsonrpc":"2.0","id":1,"result":{}}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nMcp-Session-Id: mcp-1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{init_body}",
            init_body.len()
        )
        .unwrap();
        let (mut stream, _) = listener.accept().unwrap();
        let mail = read_request(&mut stream);
        let report = serde_json::json!({
            "message_id": "mail-2", "delivered": false, "delivery_path": "inbox_only"
        });
        let response = serde_json::json!({
            "jsonrpc": "2.0", "id": 2,
            "result": {"content": [{"type": "text", "text": report.to_string()}]}
        });
        reply(&mut stream, &response.to_string());
        (lookup, initialize, mail)
    });
    let output = bg_command(&log)
        .args(["bg", log.to_str().unwrap(), "--", "sh", "-c", "exit 3"])
        .env("TUIC_SESSION", "caller-1")
        .env("TUIC_SOCKET", &socket)
        .output()
        .unwrap();
    assert!(output.status.success());
    let exit_file = std::path::PathBuf::from(format!("{}.exit", log.display()));
    let wake_file = std::path::PathBuf::from(format!("{}.wake", log.display()));
    wait_for(&wake_file);
    assert_eq!(std::fs::read_to_string(&exit_file).unwrap().trim(), "3");
    let wake_status: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&wake_file).unwrap()).unwrap();
    assert_eq!(wake_status["status"], "failed");
    assert!(
        wake_status["error"]
            .as_str()
            .unwrap()
            .contains("inbox_only")
    );
    let (lookup, initialize, mail) = server.join().unwrap();
    assert_eq!(lookup.0, "GET /sessions HTTP/1.1");
    assert_eq!(initialize.1["method"], "initialize");
    assert_eq!(mail.1["params"]["arguments"]["to"], "caller-1");
    std::fs::remove_file(exit_file).unwrap();
    std::fs::remove_file(wake_file).unwrap();
    std::fs::remove_file(log).unwrap();
    std::fs::remove_file(socket).unwrap();
}

#[test]
fn bg_keeps_command_exit_separate_from_rejected_wake() {
    let log = test_path("rejected.log");
    let socket = socket_path("rejected.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let first = read_request(&mut stream);
        reply(
            &mut stream,
            r#"[{"session_id":"pty-1","tuic_session":"caller-1"}]"#,
        );
        let (mut stream, _) = listener.accept().unwrap();
        drop(listener);
        let second = read_request(&mut stream);
        let body = r#"{"error":"Session is not running an agent"}"#;
        write!(
            stream,
            "HTTP/1.1 400 Bad Request\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        (first, second)
    });
    let output = bg_command(&log)
        .args(["bg", log.to_str().unwrap(), "--", "sh", "-c", "exit 7"])
        .env("TUIC_SESSION", "caller-1")
        .env("TUIC_SOCKET", &socket)
        .output()
        .unwrap();
    assert!(output.status.success());
    let exit_file = std::path::PathBuf::from(format!("{}.exit", log.display()));
    let wake_file = std::path::PathBuf::from(format!("{}.wake", log.display()));
    wait_for(&wake_file);
    assert_eq!(std::fs::read_to_string(&exit_file).unwrap().trim(), "7");
    let wake_status: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&wake_file).unwrap()).unwrap();
    assert_eq!(wake_status["status"], "failed");
    assert_eq!(wake_status["attempts"], 1);
    assert_eq!(wake_status["tuic_session"], "caller-1");
    let failed_marker: serde_json::Value = serde_json::from_slice(
        &std::fs::read(format!("{}.markers/caller-1.json", log.display())).unwrap(),
    )
    .unwrap();
    assert_eq!(failed_marker["status"], "failed");
    assert_eq!(failed_marker["session_id"], "caller-1");
    assert!(wake_status["error"].as_str().unwrap().contains("HTTP 400"));
    assert!(wake_status["error"].as_str().unwrap().contains("mail:"));
    let (first, second) = server.join().unwrap();
    assert_eq!(first.0, "GET /sessions HTTP/1.1");
    assert_eq!(second.0, "POST /sessions/pty-1/queue HTTP/1.1");
    std::fs::remove_file(exit_file).unwrap();
    std::fs::remove_file(wake_file).unwrap();
    std::fs::remove_file(log).unwrap();
    std::fs::remove_file(socket).unwrap();
}

#[test]
fn bg_records_wake_failure_when_tuic_is_unavailable() {
    let log = test_path("tuic-down.log");
    let missing_socket = socket_path("tuic-down.sock");
    let output = bg_command(&log)
        .args(["bg", log.to_str().unwrap(), "--", "sh", "-c", "exit 9"])
        .env("TUIC_SESSION", "caller-1")
        .env("TUIC_SOCKET", &missing_socket)
        .output()
        .unwrap();
    assert!(output.status.success());
    let exit_file = std::path::PathBuf::from(format!("{}.exit", log.display()));
    let wake_file = std::path::PathBuf::from(format!("{}.wake", log.display()));
    let wake_status = wait_for_terminal_wake(&wake_file);
    assert_eq!(std::fs::read_to_string(&exit_file).unwrap().trim(), "9");
    assert_eq!(wake_status["status"], "failed");
    assert_eq!(wake_status["attempts"], 6);
    assert_eq!(wake_status["tuic_session"], "caller-1");
    assert!(
        wake_status["error"]
            .as_str()
            .unwrap()
            .contains("Cannot connect")
    );
    std::fs::remove_file(exit_file).unwrap();
    std::fs::remove_file(wake_file).unwrap();
    std::fs::remove_file(log).unwrap();
}

#[test]
fn bg_refuses_to_start_without_a_managed_session() {
    let log = test_path("missing.log");
    let marker = test_path("missing.marker");
    let output = bg_command(&log)
        .args([
            "bg",
            log.to_str().unwrap(),
            "--",
            "sh",
            "-c",
            "touch \"$1\"",
            "_",
            marker.to_str().unwrap(),
        ])
        .env_remove("TUIC_SESSION")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(!marker.exists() && !log.exists());
}

#[test]
fn bg_creates_missing_log_directories_and_wakes_caller() {
    let root = test_path("new-parent");
    let log = root.join("nested/job.log");
    let socket = socket_path("new-parent.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let first = read_request(&mut stream);
        reply(
            &mut stream,
            r#"[{"session_id":"pty-1","tuic_session":"caller-1"}]"#,
        );
        let (mut stream, _) = listener.accept().unwrap();
        let second = read_request(&mut stream);
        reply(&mut stream, r#"{"typed":false,"queued":1}"#);
        (first, second)
    });
    let output = bg_command(&log)
        .args(["bg", log.to_str().unwrap(), "--", "sh", "-c", "echo five"])
        .env("TUIC_SESSION", "caller-1")
        .env("TUIC_SOCKET", &socket)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    wait_for(&std::path::PathBuf::from(format!("{}.exit", log.display())));
    assert!(std::fs::read_to_string(&log).unwrap().contains("five"));
    let (first, second) = server.join().unwrap();
    assert_eq!(first.0, "GET /sessions HTTP/1.1");
    assert_eq!(second.0, "POST /sessions/pty-1/queue HTTP/1.1");
    let wake_file = std::path::PathBuf::from(format!("{}.wake", log.display()));
    assert_eq!(wait_for_terminal_wake(&wake_file)["status"], "queued");
    std::fs::remove_dir_all(root).unwrap();
    std::fs::remove_file(socket).unwrap();
}

#[test]
fn bg_refuses_an_uncreatable_log_before_running_command() {
    let parent_file = test_path("blocked-parent");
    let marker = test_path("blocked-marker");
    std::fs::write(&parent_file, "not a directory").unwrap();
    let log = parent_file.join("job.log");
    let output = bg_command(&log)
        .args([
            "bg",
            log.to_str().unwrap(),
            "--",
            "sh",
            "-c",
            "touch \"$1\"",
            "_",
            marker.to_str().unwrap(),
        ])
        .env("TUIC_SESSION", "caller-1")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!marker.exists(), "command ran despite unusable log");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Cannot create background log directory")
    );
    std::fs::remove_file(parent_file).unwrap();
}

#[test]
fn bg_command_survives_killing_its_launchers_process_group() {
    use std::os::unix::process::CommandExt;
    unsafe extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }

    let log = test_path("group.log");
    let marker = test_path("group.marker");
    let mut shell = Command::new("sh");
    shell.args([
        "-c",
        "\"$1\" bg \"$2\" -- sh -c 'echo started; sleep 2; touch \"$1\"' _ \"$3\"; sleep 30",
        "sh",
        env!("CARGO_BIN_EXE_tuic"),
        log.to_str().unwrap(),
        marker.to_str().unwrap(),
    ]);
    shell
        .env("TUIC_SESSION", "caller-1")
        .env("TUIC_SOCKET", socket_path("absent.sock"))
        .env("TUIC_BG_WAKE_DIR", format!("{}.markers", log.display()))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    let mut child = shell.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(12);
    while !std::fs::read_to_string(&log)
        .unwrap_or_default()
        .contains("started")
        && Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        std::fs::read_to_string(&log)
            .unwrap_or_default()
            .contains("started")
    );
    assert_eq!(unsafe { kill(-(child.id() as i32), 9) }, 0);
    let _ = child.wait();
    wait_for(&std::path::PathBuf::from(format!("{}.exit", log.display())));
    assert!(
        marker.exists(),
        "detached command died with the launcher group"
    );
    std::fs::remove_file(format!("{}.exit", log.display())).unwrap();
    std::fs::remove_file(log).unwrap();
    std::fs::remove_file(marker).unwrap();
}

// Catches: a lost accepted reply plus failed mail uses a new key or reports a drained wake as lost.
#[test]
fn bg_lost_queue_reply_and_failed_mail_retries_the_same_key_and_records_acceptance() {
    let log = test_path("lost-reply.log");
    let wake_file = std::path::PathBuf::from(format!("{}.wake", log.display()));
    let socket = socket_path("lost-reply.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        assert_eq!(read_request(&mut stream).0, "GET /sessions HTTP/1.1");
        reply(
            &mut stream,
            r#"[{"session_id":"pty-1","tuic_session":"caller-1"}]"#,
        );
        let (mut stream, _) = listener.accept().unwrap();
        let first = read_request(&mut stream);
        assert_eq!(first.0, "POST /sessions/pty-1/queue HTTP/1.1");
        // The backend response is lost after the full POST was read.
        drop(stream);
        let (mut stream, _) = listener.accept().unwrap();
        assert_eq!(read_request(&mut stream).0, "POST /mcp HTTP/1.1");
        drop(stream); // failed mail fallback
        let (mut stream, _) = listener.accept().unwrap();
        assert_eq!(read_request(&mut stream).0, "GET /sessions HTTP/1.1");
        reply(
            &mut stream,
            r#"[{"session_id":"pty-1","tuic_session":"caller-1"}]"#,
        );
        let (mut stream, _) = listener.accept().unwrap();
        let retry = read_request(&mut stream);
        assert_eq!(retry.0, "POST /sessions/pty-1/queue HTTP/1.1");
        reply(&mut stream, r#"{"accepted":true,"typed":false,"queued":0}"#);
        (first.1, retry.1)
    });
    let output = bg_command(&log)
        .args(["bg", log.to_str().unwrap(), "--", "sh", "-c", "exit 0"])
        .env("TUIC_SESSION", "caller-1")
        .env("TUIC_SOCKET", &socket)
        .output()
        .unwrap();
    assert!(output.status.success());
    let status = wait_for_terminal_wake(&wake_file);
    let (first, retry) = server.join().unwrap();
    let key = first["idempotencyKey"].as_str().expect("stable job key");
    assert_ne!(key, "");
    assert_eq!(first, retry);
    assert_eq!(status["status"], "queued");
    assert_eq!(status["attempts"], 2);
    let marker: serde_json::Value = serde_json::from_slice(
        &std::fs::read(format!("{}.markers/caller-1.json", log.display())).unwrap(),
    )
    .unwrap();
    assert_eq!(marker["status"], "queued");
    std::fs::remove_file(socket).unwrap();
}

// Catches: exhausted ambiguous replies plus failed mail claim a possibly accepted wake was lost.
#[test]
fn bg_exhausted_lost_queue_replies_remain_uncertain_instead_of_failed() {
    let log = test_path("all-lost.log");
    let wake_file = std::path::PathBuf::from(format!("{}.wake", log.display()));
    let socket = socket_path("all-lost.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = std::thread::spawn(move || {
        let mut bodies = Vec::new();
        for _ in 0..6 {
            let (mut stream, _) = listener.accept().unwrap();
            assert_eq!(read_request(&mut stream).0, "GET /sessions HTTP/1.1");
            reply(
                &mut stream,
                r#"[{"session_id":"pty-1","tuic_session":"caller-1"}]"#,
            );
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            assert_eq!(request.0, "POST /sessions/pty-1/queue HTTP/1.1");
            bodies.push(request.1);
            drop(stream);
            let (mut stream, _) = listener.accept().unwrap();
            assert_eq!(read_request(&mut stream).0, "POST /mcp HTTP/1.1");
            drop(stream);
        }
        bodies
    });
    let output = bg_command(&log)
        .args(["bg", log.to_str().unwrap(), "--", "sh", "-c", "exit 0"])
        .env("TUIC_SESSION", "caller-1")
        .env("TUIC_SOCKET", &socket)
        .output()
        .unwrap();
    assert!(output.status.success());
    let status = wait_for_terminal_wake(&wake_file);
    let bodies = server.join().unwrap();
    assert_eq!(status["status"], "uncertain");
    assert_eq!(status["attempts"], 6);
    assert!(bodies.iter().all(|body| body == &bodies[0]));
    std::fs::remove_file(socket).unwrap();
}

// Runs the real detached producer; the socket captures only its outbound queue request.
fn bounded_completion_job(script: &str) -> (String, String) {
    let log = test_path("bounded.log");
    let socket = socket_path("bounded.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        assert_eq!(read_request(&mut stream).0, "GET /sessions HTTP/1.1");
        reply(
            &mut stream,
            r#"[{"session_id":"pty-1","tuic_session":"caller-1"}]"#,
        );
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_request(&mut stream);
        reply(&mut stream, r#"{"accepted":true}"#);
        request.1["text"].as_str().unwrap().to_string()
    });
    let output = bg_command(&log)
        .args(["bg", log.to_str().unwrap(), "--", "sh", "-c", script])
        .env("TUIC_SESSION", "caller-1")
        .env("TUIC_SOCKET", &socket)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let wake_file = std::path::PathBuf::from(format!("{}.wake", log.display()));
    assert_eq!(wait_for_terminal_wake(&wake_file)["status"], "queued");
    let wake = server.join().unwrap();
    let prefix = format!("BG DONE exit=0 log={} cmd=", log.display());
    let label = wake
        .strip_prefix(&prefix)
        .expect("unchanged completion fields")
        .to_string();
    let contents = std::fs::read_to_string(&log).unwrap();
    std::fs::remove_file(format!("{}.exit", log.display())).unwrap();
    std::fs::remove_file(wake_file).unwrap();
    std::fs::remove_file(&log).unwrap();
    std::fs::remove_file(socket).unwrap();
    std::fs::remove_dir_all(format!("{}.markers", log.display())).unwrap();
    (label, contents)
}

// Catches: multi-kilobyte argv floods the agent composer instead of a bounded label.
#[test]
fn bg_bounded_wake_with_10kb_command() {
    let script = format!("# {}", "A".repeat(10_240));
    let (label, _) = bounded_completion_job(&script);
    assert_eq!(
        label.chars().count(),
        120,
        "command label must include its ellipsis in the bound"
    );
    assert_eq!(label, format!("sh -c # {}…", "A".repeat(111)));
}

// Catches: truncating the wake loses the full command or writes it after command output.
#[test]
fn bg_bounded_log_header_preserves_full_multiline_command() {
    let script = format!("echo body\n# {}", "payload".repeat(1500));
    let (_, contents) = bounded_completion_job(&script);
    assert!(
        contents.starts_with(&format!("tuic bg: command=sh -c {script}\n")),
        "full command must head the job log"
    );
    assert!(
        contents.ends_with("body\n"),
        "command output must follow the header"
    );
}

// Catches: byte slicing breaks Unicode, boundary labels truncate early, or later lines leak.
#[test]
fn bg_bounded_wake_handles_unicode_boundaries_and_multiline_commands() {
    for length in [119, 120, 121] {
        let script = format!("# {}", "é".repeat(length - 8));
        let (label, _) = bounded_completion_job(&script);
        let expected = if length <= 120 {
            format!("sh -c {script}")
        } else {
            format!("sh -c # {}…", "é".repeat(111))
        };
        assert_eq!(label, expected);
    }
    for script in ["# first\n# second", "# first\r\n# second"] {
        let (label, _) = bounded_completion_job(script);
        assert_eq!(label, "sh -c # first…");
    }
}
