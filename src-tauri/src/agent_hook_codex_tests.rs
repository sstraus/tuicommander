#[cfg(unix)]
use super::*;

// Catches: shell quoting/base64 changes losing native identity or allowing paths to split OSC frames.
#[cfg(unix)]
#[test]
fn codex_hook_command_transports_recorded_payload_and_inert_outside_tuic() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let scratch = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let output = scratch.path().join("native-frames");
    let native = include_str!("fixtures/chat_view/codex/session-start.json");
    let command = codex_hook_map()
        .into_iter()
        .find(|entry| entry.0 == "SessionStart")
        .unwrap()
        .2;
    // Only redirect the existing tty destination to capture producer bytes.
    let command = command.replace(tty_resolve(), &format!("__t='{}'", output.display()));
    let mut child = Command::new("sh")
        .args(["-c", &command])
        .env("TUIC_SESSION", "own-tab")
        // The tty fallback: never post to the TUIC that runs this test.
        .env_remove("TUIC_SOCKET")
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(native.as_bytes())
        .unwrap();
    assert!(child.wait().unwrap().success());
    use base64::Engine;
    let expected = base64::engine::general_purpose::STANDARD.encode(native);
    assert_eq!(
        std::fs::read_to_string(&output).unwrap(),
        format!("\x1b]7770;codex-session=own-tab:{expected}\x1b\\\x1b]7770;state=busy\x1b\\")
    );
    std::fs::remove_file(&output).unwrap();
    let status = Command::new("sh")
        .args(["-c", &command])
        .env_remove("TUIC_SESSION")
        .stdin(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    assert!(!output.exists());
}

// Catches: installed tool hooks emitting unscoped busy or requesting attention for automatic approvals.
#[cfg(unix)]
#[test]
fn codex_installed_tool_commands_preserve_native_receipts_without_permission_hooks() {
    use base64::Engine;
    use std::io::Write;
    use std::process::{Command, Stdio};

    let scratch = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let output = scratch.path().join("tool-frames");
    let map = codex_hook_map();
    assert!(!map.iter().any(|entry| entry.0 == "PermissionRequest"));
    for index in [2, 3] {
        let payload = include_str!("fixtures/codex_tools_2626/hooks.jsonl")
            .lines()
            .nth(index)
            .unwrap();
        let receipt: serde_json::Value = serde_json::from_str(payload).unwrap();
        let command = map
            .iter()
            .find(|entry| entry.0 == receipt["hook_event_name"].as_str().unwrap())
            .expect("native tool edge must be installed")
            .2
            .replace(tty_resolve(), &format!("__t='{}'", output.display()));
        let mut child = Command::new("sh")
            .args(["-c", &command])
            .env("TUIC_SESSION", "own-tab")
            // The tty fallback: never post to the TUIC that runs this test.
            .env_remove("TUIC_SOCKET")
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(payload.as_bytes())
            .unwrap();
        assert!(child.wait().unwrap().success());
        assert_eq!(
            std::fs::read_to_string(&output).unwrap(),
            format!(
                "\x1b]7770;codex-tool=own-tab:{}\x1b\\",
                base64::engine::general_purpose::STANDARD.encode(payload)
            )
        );
        std::fs::remove_file(&output).unwrap();
        assert!(
            Command::new("sh")
                .args(["-c", &command])
                .env_remove("TUIC_SESSION")
                .stdin(Stdio::null())
                .status()
                .unwrap()
                .success()
        );
        assert!(!output.exists());
    }
}
