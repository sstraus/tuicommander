#![cfg(unix)]

use std::io::Write;
use std::process::Command;
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
// Catches: CLI --instance/env routing to the default socket, or ignoring TUIC_SOCKET.
#[test]
fn named_instance_cli_reaches_its_socket_and_override_wins() {
    let dir = tuic_test_support::socket_dir();
    let named = "tuic-mcp-1953e60b4e4d5340.sock";
    let spelling = tuic_test_support::SocketSpelling::for_dir(dir.path(), named);
    for mode in ["flag", "env", "override"] {
        let name = if mode == "override" {
            "override.sock"
        } else {
            named
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
                        let request = tuic_test_support::read_http_request(&mut stream).unwrap();
                        seen.push(request.request_line);
                        let body = r#"[{"marker":"named-instance"}]"#;
                        write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                            body.len()
                        )
                        .unwrap();
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(5))
                    }
                    Err(error) => panic!("{error}"),
                }
            }
            seen
        });
        let mut command = Command::new(env!("CARGO_BIN_EXE_tuic"));
        command
            .env_remove("TUIC_SOCKET")
            .env_remove("TUIC_APP_INSTANCE")
            .current_dir(dir.path())
            .env("TMPDIR", spelling.temp_dir(dir.path()));
        if mode == "flag" {
            command.args(["--instance", "ipc-cli-regression"]);
            // Explicit selection must override even an invalid inherited env.
            command.env("TUIC_APP_INSTANCE", "INVALID");
        } else {
            command.env("TUIC_APP_INSTANCE", "ipc-cli-regression");
        }
        if mode == "override" {
            command.env("TUIC_SOCKET", spelling.path(dir.path(), name));
        }
        let output = command.args(["ls", "--json"]).output().unwrap();
        stop.store(true, Ordering::Relaxed);
        let seen = server.join().unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("named-instance"),
            "{mode}"
        );
        assert!(
            seen.iter().any(|line| line == "GET /sessions HTTP/1.1"),
            "{mode}: {seen:?}"
        );
    }
}

// Catches: invalid instance silently falling back to Boss's default namespace.
#[test]
fn named_instance_cli_rejects_invalid_selection_before_connecting() {
    let output = Command::new(env!("CARGO_BIN_EXE_tuic"))
        .args(["--instance", "../escape", "ls", "--json"])
        .env_remove("TUIC_APP_INSTANCE")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Invalid application instance"));
}
