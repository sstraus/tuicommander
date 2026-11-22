#![cfg(unix)]

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

// Catches: `--instance <id>` ignoring the socket the instance published in its
// `mcp-socket` pointer (an agent-test app binds outside the TMPDIR-derived
// name) and dialling the derived/default endpoint instead, or hiding which
// socket it used.
#[test]
fn instance_flag_connects_to_the_published_socket_and_names_it() {
    let sockets = tuic_test_support::socket_dir();
    let home = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
    let platform = if cfg!(target_os = "macos") {
        home.path().join("Library/Application Support")
    } else {
        home.path().join(".config")
    };
    let id = "pointer-regression";
    let config_dir = tuic_ipc::app_instance::AppInstance::named(id)
        .unwrap()
        .config_dir_from(Some(&platform), home.path());
    let published = sockets.path().join("p.sock");
    tuic_ipc::publish_socket_path(&config_dir, &published).unwrap();

    let listener = std::os::unix::net::UnixListener::bind(&published).unwrap();
    listener.set_nonblocking(true).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let server_stop = stop.clone();
    let server = std::thread::spawn(move || {
        let mut seen = Vec::new();
        while !server_stop.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream.set_nonblocking(false).unwrap();
                    let Ok(request) = tuic_test_support::read_http_request(&mut stream) else {
                        continue;
                    };
                    seen.push(request.request_line);
                    let body = r#"{"jsonrpc":"2.0","id":0,"result":{}}"#;
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nMcp-Session-Id: PointerPeer\r\nContent-Length: {}\r\n\r\n{body}",
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

    // A TMPDIR with no tuic-mcp-*.sock: the derived name cannot be reached.
    let empty_tmp = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_tuic-bridge"))
        .args(["--instance", id])
        .env_remove("TUIC_SOCKET")
        .env_remove("TUIC_APP_INSTANCE")
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", &platform)
        .env("TMPDIR", empty_tmp.path())
        .stdin(Stdio::null())
        .output()
        .unwrap();
    stop.store(true, Ordering::Relaxed);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let seen = server.join().unwrap();
    assert!(output.status.success(), "{stderr}");
    assert!(stderr.contains("connected to TUIC"), "{stderr}");
    assert!(
        stderr.contains(&published.display().to_string()),
        "startup line must name the published socket: {stderr}"
    );
    assert!(
        seen.iter().any(|line| line == "POST /mcp HTTP/1.1"),
        "{seen:?}: {stderr}"
    );
}
