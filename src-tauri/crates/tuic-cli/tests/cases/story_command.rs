#![cfg(unix)]

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixListener;
use std::process::{Command, Output};

fn assert_checkout_socket(socket: &std::path::Path) {
    let socket_root = tuic_test_support::short_socket_test_temp_root();
    assert!(
        socket.starts_with(&socket_root),
        "socket escaped this checkout's socket scratch: {}",
        socket.display()
    );
    assert!(
        socket.as_os_str().len() < 104,
        "socket exceeds Unix path budget"
    );
}

fn run_against_stub(
    status: u16,
    response_body: &str,
) -> (Output, String, String, std::path::PathBuf) {
    let scratch = tempfile::tempdir_in(tuic_test_support::short_socket_test_temp_root())
        .expect("socket scratch");
    let socket = scratch.path().join("story.sock");
    let listener = UnixListener::bind(&socket).expect("stub socket");
    let response_body = response_body.to_owned();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("CLI connection");
        let mut reader = BufReader::new(stream.try_clone().expect("clone socket"));
        let mut request_line = String::new();
        reader.read_line(&mut request_line).expect("request line");
        let mut content_length = None;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).expect("request header");
            if line == "\r\n" {
                break;
            }
            if line.to_ascii_lowercase().starts_with("content-length:") {
                let value = line.split_once(':').expect("Content-Length header").1;
                content_length = Some(value.trim().parse::<usize>().expect("body length"));
            }
        }
        let mut body = vec![0; content_length.expect("Content-Length")];
        reader.read_exact(&mut body).expect("request body");
        let status_text = if status == 200 { "OK" } else { "Forbidden" };
        write!(
            stream,
            "HTTP/1.1 {status} {status_text}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response_body}",
            response_body.len()
        )
        .expect("stub response");
        stream.flush().expect("flush stub response");
        (request_line, String::from_utf8(body).expect("request JSON"))
    });

    let output = Command::new(env!("CARGO_BIN_EXE_tuic"))
        .args([
            "story",
            r#"{"action":"list_plans"}"#,
            "--project",
            "/native story%?#&=",
            "--session-id",
            "pty-1",
        ])
        .env("TUIC_SOCKET", &socket)
        .output()
        .expect("run tuic story");
    let (request_line, body) = server.join().expect("stub server");
    std::fs::remove_file(&socket).expect("remove stub socket");
    (output, request_line, body, socket)
}

#[test]
fn story_command_sends_encoded_project_and_prints_reply() {
    let (output, request_line, body, socket) =
        run_against_stub(200, r#"{"type":"plans","value":[]}"#);
    assert_checkout_socket(&socket);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        request_line,
        "POST /stories/action?path=/native%20story%25%3F%23%26%3D HTTP/1.1\r\n"
    );
    let json: serde_json::Value = serde_json::from_str(&body).expect("request JSON");
    assert_eq!(json["action"], serde_json::json!({"action": "list_plans"}));
    assert_eq!(json["sessionId"], "pty-1");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "{\"type\":\"plans\",\"value\":[]}\n"
    );
}

#[test]
fn story_command_reports_http_failure_with_nonzero_exit() {
    let (output, _, _, socket) = run_against_stub(403, r#"{"error":"denied"}"#);
    assert_checkout_socket(&socket);
    assert!(!output.status.success());
    assert_eq!(output.stdout, [] as [u8; 0]);
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "tuic: Story request failed (HTTP 403): {\"error\":\"denied\"}\n"
    );
}

#[test]
fn story_command_uses_distinct_cleaned_sockets_for_consecutive_requests() {
    let (_, _, _, first) = run_against_stub(200, "{}");
    let (_, _, _, second) = run_against_stub(200, "{}");
    assert_ne!(first, second);
    assert!(!first.exists());
    assert!(!second.exists());
}
