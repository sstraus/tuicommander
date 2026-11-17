#![cfg(unix)]

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

// The named socket's absolute path (`<TMPDIR>/tuic-mcp-<16 hex>.sock`, a
// production name) cannot fit sun_path under a long TMPDIR (the agent
// sandbox's is 72 bytes). Then the listener binds it by a RELATIVE name
// (`SocketSpelling::Relative`: chdir under a process-wide lock, then restore),
// and the child runs in that directory with `TMPDIR=.`. That needs one test
// per process: `cargo nextest` (the gate's runner), or RUST_TEST_THREADS=1;
// `tuic_test_support::in_dir` panics otherwise.
// Catches: bridge connecting/falling back to another instance instead of the selected socket.
#[test]
fn named_instance_bridge_reaches_its_socket_and_override_wins() {
    let dir = tuic_test_support::socket_dir();
    let fallback = "tuic-mcp-b199f45760a9ee6c-123.sock";
    let spelling = tuic_test_support::SocketSpelling::for_dir(dir.path(), fallback);
    for mode in ["flag", "env", "fallback", "override"] {
        let name = match mode {
            "override" => "override.sock",
            "fallback" => fallback,
            _ => "tuic-mcp-b199f45760a9ee6c.sock",
        };
        let path = dir.path().join(name);
        let listener = spelling.bind(dir.path(), name).unwrap();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let server_stop = stop.clone();
        let server = std::thread::spawn(move || {
            let mut seen = Vec::new();
            while !server_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        // macOS hands back an accepted socket that inherited the listener's
                        // non-blocking flag: a request not yet written then reads as an error and
                        // the stream is dropped, so the bridge sees EPIPE (flaky ~1 in 4).
                        stream.set_nonblocking(false).unwrap();
                        let request = match tuic_test_support::read_http_request(&mut stream) {
                            Ok(request) => request,
                            Err(_) => continue,
                        };
                        seen.push(request.request_line);
                        let body = r#"{"jsonrpc":"2.0","id":0,"result":{}}"#;
                        let _ = write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nMcp-Session-Id: NamedPeer\r\nContent-Length: {}\r\n\r\n{body}",
                            body.len()
                        );
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(5))
                    }
                    Err(error) => panic!("{error}"),
                }
            }
            seen
        });
        let mut command = Command::new(env!("CARGO_BIN_EXE_tuic-bridge"));
        command
            .env_remove("TUIC_SOCKET")
            .env_remove("TUIC_APP_INSTANCE")
            .current_dir(dir.path())
            .env("TMPDIR", spelling.temp_dir(dir.path()))
            .stdin(Stdio::null());
        if mode == "flag" {
            command.args(["--instance", "ipc-bridge-regression"]);
            command.env("TUIC_APP_INSTANCE", "INVALID");
        } else {
            command.env("TUIC_APP_INSTANCE", "ipc-bridge-regression");
        }
        if mode == "override" {
            command.env("TUIC_SOCKET", spelling.path(dir.path(), name));
        }
        let output = command.output().unwrap();
        stop.store(true, Ordering::Relaxed);
        let seen = server.join().unwrap();
        std::fs::remove_file(path).unwrap();
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("connected to TUIC"),
            "{mode}"
        );
        assert!(
            seen.iter().any(|line| line == "POST /mcp HTTP/1.1"),
            "{mode}: {seen:?}; binary: {}; stderr: {}",
            env!("CARGO_BIN_EXE_tuic-bridge"),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            seen.iter().any(|line| line == "DELETE /mcp HTTP/1.1"),
            "{mode}: {seen:?}; binary: {}; stderr: {}",
            env!("CARGO_BIN_EXE_tuic-bridge"),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
