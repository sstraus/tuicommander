#[cfg(unix)]
use super::*;

/// One HTTP request received on the test socket: request line and body.
#[cfg(unix)]
type Received = std::sync::Arc<std::sync::Mutex<Vec<(String, String)>>>;

/// Serve a Unix socket that records each request and answers 204, as the
/// local TUIC API does after it processed the frames.
#[cfg(unix)]
fn recording_socket(path: &std::path::Path) -> Received {
    use std::io::{BufRead, BufReader, Read, Write};
    let listener = std::os::unix::net::UnixListener::bind(path).unwrap();
    let received = Received::default();
    let sink = received.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request = String::new();
            reader.read_line(&mut request).unwrap();
            let mut length = 0;
            loop {
                let mut header = String::new();
                reader.read_line(&mut header).unwrap();
                if header.trim().is_empty() {
                    break;
                }
                if let Some((name, value)) = header.split_once(':')
                    && name.eq_ignore_ascii_case("content-length")
                {
                    length = value.trim().parse().unwrap();
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            sink.lock()
                .unwrap()
                .push((request.trim().to_owned(), String::from_utf8(body).unwrap()));
            stream
                .write_all(
                    b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
        }
    });
    received
}

/// Run a generated hook as an agent does: JSON on stdin, the tty redirected to
/// `tty` so every byte the hook would put on the agent screen is captured.
#[cfg(unix)]
fn run_hook(command: &str, stdin: &str, socket: Option<&std::path::Path>, tty: &std::path::Path) {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let command = command.replace(tty_resolve(), &format!("__t='{}'", tty.display()));
    let mut child = Command::new("sh");
    child
        .args(["-c", &command])
        .env("TUIC_SESSION", "own-tab")
        .env_remove("TUIC_SOCKET")
        .stdin(Stdio::piped());
    if let Some(socket) = socket {
        child.env("TUIC_SOCKET", socket);
    }
    let mut child = child.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    assert!(child.wait().unwrap().success());
}

// Catches: a hook (Claude, Gemini, Codex or Grok) still writes its OSC 7770
// frames to the tty the agent TUI redraws, where a split write prints them as
// text over the agent screen (#2749-bee7).
#[cfg(unix)]
#[test]
fn every_hook_posts_its_frames_to_the_tuic_socket_and_not_the_tty() {
    let dir = crate::test_support::short_socket_tempdir();
    let socket = dir.path().join("h.sock");
    let received = recording_socket(&socket);
    let tty = dir.path().join("tty");
    let stdin = r#"{"session_id":"native","hook_event_name":"PreToolUse"}"#;
    let maps = [
        ("claude", claude_hook_map()),
        ("gemini", gemini_hook_map()),
        ("codex", codex_hook_map()),
        ("grok", grok_hook_map()),
    ];
    for (agent, map) in maps {
        for (event, matcher, command) in map {
            let before = received.lock().unwrap().len();
            run_hook(&command, stdin, Some(&socket), &tty);
            assert!(
                !tty.exists(),
                "{agent} {event} {matcher}: hook wrote to the tty: {:?}",
                std::fs::read(&tty).unwrap_or_default()
            );
            let requests = received.lock().unwrap();
            assert_eq!(requests.len(), before + 1, "{agent} {event}: no post");
            let (request, body) = requests.last().unwrap();
            assert_eq!(request, "POST /sessions/own-tab/hook HTTP/1.1");
            assert!(
                body.starts_with("\x1b]7770;") && body.ends_with("\x1b\\"),
                "{agent} {event}: {body:?}"
            );
        }
    }
}

// Catches: the side channel drops a large Claude receipt (the tty path caps
// receipts at 32 KB of base64), so a PostToolUse with a large tool_response
// never retires its tool (#2749-bee7).
#[cfg(unix)]
#[test]
fn claude_hook_posts_a_large_receipt_as_one_frame() {
    use base64::Engine;
    let dir = crate::test_support::short_socket_tempdir();
    let socket = dir.path().join("h.sock");
    let received = recording_socket(&socket);
    let tty = dir.path().join("tty");
    let payload = serde_json::json!({
        "hook_event_name": "PostToolUse",
        "tool_response": {"content": "x".repeat(200 * 1024)},
    })
    .to_string();
    let command = claude_hook_map()
        .into_iter()
        .find(|(event, matcher, _)| *event == "PostToolUse" && matcher.is_empty())
        .unwrap()
        .2;
    run_hook(&command, &payload, Some(&socket), &tty);
    assert!(!tty.exists());
    let (_, body) = received.lock().unwrap().pop().unwrap();
    let encoded = base64::engine::general_purpose::STANDARD.encode(&payload);
    let (writer, piece) = body
        .strip_prefix("\x1b]7770;claude-hook=own-tab:")
        .and_then(|rest| rest.split_once(':'))
        .unwrap();
    assert_eq!(
        piece,
        format!("{encoded}\x1b\\\x1b]7770;claude-hook=own-tab:{writer}:\x1b\\")
    );
}

// Catches: with no TUIC socket, or a socket nobody serves, the hook loses its
// frames instead of using the tty as before.
#[cfg(unix)]
#[test]
fn hook_falls_back_to_the_tty_when_the_socket_is_unreachable() {
    let dir = crate::test_support::short_socket_tempdir();
    let tty = dir.path().join("tty");
    let command = hook_command("idle");
    for socket in [None, Some(dir.path().join("absent.sock"))] {
        run_hook(&command, "", socket.as_deref(), &tty);
        assert_eq!(
            std::fs::read_to_string(&tty).unwrap(),
            "\x1b]7770;state=idle\x1b\\"
        );
        std::fs::remove_file(&tty).unwrap();
    }
}
