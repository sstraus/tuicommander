#![cfg(unix)]
use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

// Catches: directory opens still invoke the OS URL handler instead of asking
// the selected running server to register the repository; errors look successful.
#[test]
fn directory_open_uses_server_registration_and_propagates_rejection() {
    for (explicit, rejected) in [(false, false), (true, false), (true, true)] {
        let tmp = tempfile::Builder::new()
            .prefix("open-")
            .tempdir_in(tuic_test_support::test_temp_root())
            .unwrap();
        let socket = tmp.path().join("mcp.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();
        let done = Arc::new(AtomicBool::new(false));
        let calls = Arc::new(Mutex::new(Vec::new()));
        let worker_done = done.clone();
        let worker_calls = calls.clone();
        let worker = std::thread::spawn(move || {
            while !worker_done.load(Ordering::Relaxed) {
                let (mut stream, _) = match listener.accept() {
                    Ok(value) => value,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(5));
                        continue;
                    }
                    Err(error) => panic!("{error}"),
                };
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                    .unwrap();
                let mut bytes = Vec::new();
                let (header_end, length) = loop {
                    let mut buffer = [0; 4096];
                    let n = stream.read(&mut buffer).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buffer[..n]);
                    if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..end]);
                        let length = headers
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse::<usize>().unwrap())
                            })
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + length {
                            break (end + 4, length);
                        }
                    }
                };
                let payload = if length > 0 {
                    serde_json::from_slice::<serde_json::Value>(
                        &bytes[header_end..header_end + length],
                    )
                    .unwrap()
                } else {
                    serde_json::json!({})
                };
                let body = if payload["method"] == "tools/call" {
                    worker_calls.lock().unwrap().push(payload["params"].clone());
                    let result = if rejected { serde_json::json!({"error":"registration refused"}) }
                        else { serde_json::json!({"ok":true}) };
                    serde_json::json!({"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":result.to_string()}],"isError":rejected}})
                } else { serde_json::json!({"jsonrpc":"2.0","id":1,"result":{}}) }.to_string();
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nMcp-Session-Id: test-open\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
            }
        });
        let project = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .canonicalize()
            .unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_tuic"));
        command
            .env("TUIC_SOCKET", &socket)
            .env("TUIC_SESSION", "test-open");
        if explicit {
            command.arg("open");
        }
        let output = command.arg(&project).output().unwrap();
        done.store(true, Ordering::Relaxed);
        worker.join().unwrap();
        assert_eq!(
            *calls.lock().unwrap(),
            vec![serde_json::json!({
                "name":"repo", "arguments":{"action":"add", "path":project.to_string_lossy()}
            })],
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.status.success(), !rejected);
        if rejected {
            assert!(String::from_utf8_lossy(&output.stderr).contains("registration refused"));
        }
    }
}
