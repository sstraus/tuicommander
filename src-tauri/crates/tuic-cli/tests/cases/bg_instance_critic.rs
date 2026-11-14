#![cfg(unix)]

use std::process::Command;
use std::time::{Duration, Instant};

// Catches: bg appends instances/<id> twice, uses inherited identity for its
// marker, or forgets to propagate an explicit identity to its detached runner.
#[test]
fn bg_explicit_instance_writes_one_namespace_and_runner_inherits_selection() {
    let temp = tempfile::Builder::new()
        .prefix("bic-")
        .tempdir_in(tuic_test_support::short_socket_test_temp_root())
        .unwrap();
    let log = temp.path().join("child.log");
    let output = Command::new(env!("CARGO_BIN_EXE_tuic"))
        .args(["--instance", "critic-bg", "bg"])
        .arg(&log)
        .args(["--", "/usr/bin/env"])
        .env("HOME", temp.path())
        .env("XDG_CONFIG_HOME", temp.path())
        .env("TMPDIR", temp.path())
        .env("TUIC_APP_INSTANCE", "INVALID")
        .env("TUIC_SESSION", "critic-marker")
        .env("TUIC_SOCKET", temp.path().join("absent.sock"))
        .env_remove("TUIC_BG_WAKE_DIR")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let pid: i32 = stdout
        .split("pid=")
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let harness_deadline = Instant::now() + Duration::from_secs(120);
    let recorded_env = loop {
        let text = std::fs::read_to_string(&log).unwrap_or_default();
        if text.contains("TUIC_APP_INSTANCE=") || Instant::now() > harness_deadline {
            break text;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    // The launched runner owns its fresh process group. Stop only that group;
    // its mock socket deliberately cannot reach any real agent inbox.
    unsafe extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    // SAFETY: pid is obtained from this test's own launch; negative pid selects
    // the new group created by bg::launch, never an ancestor's process group.
    unsafe { kill(-pid, 9) };
    assert!(
        recorded_env
            .lines()
            .any(|line| line == "TUIC_APP_INSTANCE=critic-bg"),
        "{recorded_env}"
    );
    // Catches: the runner reverting to the inherited socket after --instance
    // was applied only in the launcher. Digest is SHA-256 of "critic-bg".
    let socket = temp.path().join("tuic-mcp-3c22e8785b4188ce.sock");
    assert!(
        recorded_env
            .lines()
            .any(|line| line == format!("TUIC_SOCKET={}", socket.display())),
        "{recorded_env}"
    );
    let base = if cfg!(target_os = "macos") {
        temp.path().join("Library/Application Support")
    } else {
        temp.path().to_path_buf()
    };
    let namespace = base.join("com.tuic.commander/instances/critic-bg");
    let marker = namespace.join("bg-wakes/critic-marker.json");
    assert!(marker.is_file(), "missing marker {}", marker.display());
    assert!(
        !namespace.join("instances").exists(),
        "instance suffix appended twice"
    );
    assert!(
        !base.join("com.tuic.commander/bg-wakes").exists(),
        "default namespace was written"
    );
}
