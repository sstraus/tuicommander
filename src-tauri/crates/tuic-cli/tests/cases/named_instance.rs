#![cfg(unix)]

use std::io::Write;
use std::os::unix::net::UnixListener;
use std::process::Command;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

// Catches: CLI --instance/env routing to the default socket, or ignoring TUIC_SOCKET.
#[test]
fn named_instance_cli_reaches_its_socket_and_override_wins() {
    let root = tuic_test_support::short_socket_test_temp_root();
    let dir = tempfile::Builder::new()
        .prefix("ipc")
        .tempdir_in(root)
        .unwrap();
    let named = dir.path().join("tuic-mcp-1953e60b4e4d5340.sock");
    let explicit = dir.path().join("override.sock");
    for mode in ["flag", "flag-inherited", "flag-same", "env", "override"] {
        let path = if mode == "override" {
            &explicit
        } else {
            &named
        };
        let listener = UnixListener::bind(path).unwrap();
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
            .env("TMPDIR", dir.path());
        if mode.starts_with("flag") {
            command.args(["--instance", "ipc-cli-regression"]);
            // Explicit selection must override even an invalid inherited env.
            command.env("TUIC_APP_INSTANCE", "INVALID");
        } else {
            command.env("TUIC_APP_INSTANCE", "ipc-cli-regression");
        }
        if mode == "override" {
            command.env("TUIC_SOCKET", path);
        }
        // Catches: an inherited socket defeating explicit --instance selection.
        if mode == "flag-inherited" {
            command.env("TUIC_SOCKET", &explicit);
        } else if mode == "flag-same" {
            command.env("TUIC_SOCKET", &named);
        }
        let output = command.args(["ls", "--json"]).output().unwrap();
        stop.store(true, Ordering::Relaxed);
        let seen = server.join().unwrap();
        std::fs::remove_file(path).unwrap();
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        if mode == "flag-inherited" {
            assert!(stderr.contains(&*named.to_string_lossy()), "{stderr}");
            assert!(stderr.contains("ignoring TUIC_SOCKET"), "{stderr}");
        } else {
            assert!(stderr.is_empty(), "{mode}: {stderr}");
        }
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

/// The config directory the `tuic` child computes for `id` under `home`.
fn sandbox_instance_config_dir(home: &std::path::Path, id: &str) -> std::path::PathBuf {
    let base = if cfg!(target_os = "macos") {
        home.join("Library/Application Support")
    } else {
        home.join(".config")
    };
    base.join("com.tuic.commander/instances").join(id)
}

fn sandboxed_tuic(home: &std::path::Path, client_tmp: &std::path::Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tuic"));
    command
        .env_remove("TUIC_SOCKET")
        .env_remove("TUIC_APP_INSTANCE")
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("TMPDIR", client_tmp);
    command
}

// Catches: the CLI looking for a named socket only in its own TMPDIR, so
// `--instance agent-test-<n>` never reaches an app started with another TMPDIR.
#[test]
fn named_instance_cli_reaches_the_socket_its_app_published() {
    let dir = tempfile::Builder::new()
        .prefix("pub")
        .tempdir_in(tuic_test_support::short_socket_test_temp_root())
        .unwrap();
    let home = dir.path().join("home");
    let client_tmp = dir.path().join("client-tmp");
    std::fs::create_dir_all(&client_tmp).unwrap();
    let socket = dir.path().join("app.sock");
    let config = sandbox_instance_config_dir(&home, "zz-published");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(
        config.join("mcp-socket"),
        socket.as_os_str().as_encoded_bytes(),
    )
    .unwrap();

    let listener = UnixListener::bind(&socket).unwrap();
    let server = std::thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let (mut stream, _) = loop {
            match listener.accept() {
                Ok(accepted) => break accepted,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if std::time::Instant::now() > deadline {
                        return None;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(error) => panic!("{error}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        let request = tuic_test_support::read_http_request(&mut stream).unwrap();
        let body = r#"[{"marker":"published"}]"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        Some(request.request_line)
    });
    let output = sandboxed_tuic(&home, &client_tmp)
        .args(["--instance", "zz-published", "ls", "--json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let seen = server.join().unwrap();
    assert_eq!(seen.as_deref(), Some("GET /sessions HTTP/1.1"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("published"));
}

// Catches: a command for a stopped named instance starting the user's DEFAULT
// app (`open -a TUICommander`), so a test command lands in the main instance.
#[test]
fn named_instance_cli_never_launches_the_default_app() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::Builder::new()
        .prefix("down")
        .tempdir_in(tuic_test_support::short_socket_test_temp_root())
        .unwrap();
    let home = dir.path().join("home");
    let client_tmp = dir.path().join("client-tmp");
    let fake_bin = dir.path().join("bin");
    std::fs::create_dir_all(&client_tmp).unwrap();
    std::fs::create_dir_all(&fake_bin).unwrap();
    let marker = dir.path().join("launched");
    for launcher in ["open", "xdg-open", "tuicommander"] {
        let script = fake_bin.join(launcher);
        std::fs::write(
            &script,
            "#!/bin/sh\necho \"$0 $*\" >> \"$TUIC_FAKE_LAUNCH_MARK\"\n",
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let path = format!(
        "{}:{}",
        fake_bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let output = sandboxed_tuic(&home, &client_tmp)
        .env("PATH", path)
        .env("TUIC_FAKE_LAUNCH_MARK", &marker)
        .args(["--instance", "zz-stopped", "new"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !marker.exists(),
        "launched: {}",
        std::fs::read_to_string(&marker).unwrap_or_default()
    );
    assert!(!output.status.success());
    assert!(stderr.contains("'zz-stopped' is not running"), "{stderr}");
}
