#![cfg(unix)]

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixListener;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static NEXT_SOCKET: AtomicU64 = AtomicU64::new(0);

fn reply(
    stream: &mut std::os::unix::net::UnixStream,
    payload: &str,
    session: bool,
) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n{}Connection: close\r\n\r\n{payload}",
        payload.len(),
        if session {
            "Mcp-Session-Id: test-session\r\n"
        } else {
            ""
        }
    )?;
    Ok(())
}

fn request(stream: &mut std::os::unix::net::UnixStream) -> serde_json::Value {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    let mut length = 0;
    loop {
        line.clear();
        reader.read_line(&mut line).unwrap();
        if line == "\r\n" {
            break;
        }
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            length = value.trim().parse().unwrap();
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).unwrap();
    serde_json::from_slice(&body).unwrap()
}

fn wait_response(args: &[&str], delay: Duration) -> (std::process::Output, Duration) {
    let socket = tuic_test_support::short_socket_path(&format!(
        "w{}.sock",
        NEXT_SOCKET.fetch_add(1, Ordering::Relaxed)
    ));
    let listener = UnixListener::bind(&socket).unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        reply(&mut stream, "{}", false).unwrap();
        let (mut stream, _) = listener.accept().unwrap();
        request(&mut stream);
        reply(&mut stream, r#"{"jsonrpc":"2.0","result":{}}"#, true).unwrap();
        let (mut stream, _) = listener.accept().unwrap();
        let call = request(&mut stream);
        assert_eq!(call["method"], "tools/call");
        std::thread::sleep(delay);
        let payload = r#"{"jsonrpc":"2.0","result":{"content":[{"type":"text","text":"{\"timed_out\":true}"}]}}"#;
        let _ = reply(&mut stream, payload, false);
    });
    let start = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_tuic"))
        .args(args)
        .env("TUIC_SOCKET", &socket)
        .env("TUIC_SESSION", "wait-test")
        .output()
        .unwrap();
    let elapsed = start.elapsed();
    server.join().unwrap();
    std::fs::remove_file(socket).unwrap();
    (output, elapsed)
}

#[test]
fn agent_wait_accepts_response_after_three_seconds_with_four_second_request() {
    let (output, elapsed) = wait_response(
        &["agent", "wait", "--timeout-ms", "4000", "--json"],
        Duration::from_millis(3500),
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(elapsed >= Duration::from_millis(3500));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "{\"timed_out\":true}\n"
    );
}

#[test]
fn session_wait_accepts_response_after_four_seconds_with_six_second_request() {
    let (output, elapsed) = wait_response(
        &[
            "session",
            "wait",
            "target",
            "--timeout-ms",
            "6000",
            "--json",
        ],
        Duration::from_secs(4),
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(elapsed >= Duration::from_secs(4));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "{\"timed_out\":true}\n"
    );
}

#[test]
fn default_agent_wait_accepts_response_after_three_seconds() {
    let (output, _) = wait_response(&["agent", "wait", "--json"], Duration::from_millis(3500));
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn non_wait_request_still_times_out_after_about_three_seconds() {
    let socket = tuic_test_support::short_socket_path(&format!(
        "nw{}.sock",
        NEXT_SOCKET.fetch_add(1, Ordering::Relaxed)
    ));
    let listener = UnixListener::bind(&socket).unwrap();
    let server = std::thread::spawn(move || {
        let (_stream, _) = listener.accept().unwrap();
        std::thread::sleep(Duration::from_millis(3500));
    });
    let start = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_tuic"))
        .args(["ls"])
        .env("TUIC_SOCKET", &socket)
        .output()
        .unwrap();
    let elapsed = start.elapsed();
    server.join().unwrap();
    std::fs::remove_file(socket).unwrap();
    assert!(!output.status.success());
    assert!(
        elapsed >= Duration::from_secs(3) && elapsed < Duration::from_secs(5),
        "{elapsed:?}"
    );
}
