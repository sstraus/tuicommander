#[cfg(unix)]
use super::*;

// Catches: generated Claude shell commands lose native receipts or emit escaped text instead of OSC.
#[cfg(unix)]
#[test]
fn claude_permission_commands_emit_receipts_and_remain_inert_outside_tuic() {
    use base64::Engine;
    use std::io::Write;
    use std::process::{Command, Stdio};
    let scratch = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let output = scratch.path().join("frames");
    let events: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/claude_hooks_2627/accept.json")).unwrap();
    for (index, coarse) in [(1, Some("prompt")), (2, Some("busy")), (3, None), (4, None)] {
        let event = &events[index];
        let payload = event.to_string();
        let command = claude_hook_map()
            .into_iter()
            .find(|(e, m, _)| *e == event["hook_event_name"].as_str().unwrap() && m.is_empty())
            .unwrap()
            .2;
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
            .write_all(payload.as_bytes())
            .unwrap();
        assert!(child.wait().unwrap().success());
        let written = std::fs::read_to_string(&output).unwrap();
        let writer = written
            .strip_prefix("\x1b]7770;claude-hook=own-tab:")
            .and_then(|rest| rest.split_once(':'))
            .unwrap()
            .0;
        let encoded = base64::engine::general_purpose::STANDARD.encode(&payload);
        let mut expected = String::new();
        for piece in encoded.as_bytes().chunks(768) {
            let piece = std::str::from_utf8(piece).unwrap();
            expected.push_str(&format!(
                "\x1b]7770;claude-hook=own-tab:{writer}:{piece}\x1b\\"
            ));
        }
        expected.push_str(&format!("\x1b]7770;claude-hook=own-tab:{writer}:\x1b\\"));
        if let Some(coarse) = coarse {
            expected.push_str(&format!("\x1b]7770;state={coarse}\x1b\\"));
        }
        assert_eq!(written, expected);
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
