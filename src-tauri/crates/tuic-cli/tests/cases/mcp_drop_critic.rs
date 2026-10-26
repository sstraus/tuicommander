//! Critic 1148: the session DELETE that `McpClient` sends from `Drop`.
#![cfg(unix)]

use std::io::Write;
use std::os::unix::net::{UnixListener, UnixStream};
use std::process::{Command, Output};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

const PEERS: &[&str] = &["agent", "list-peers", "--json"];
const INBOX: &[&str] = &["agent", "inbox", "--json"];

static NEXT_SOCKET: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Normal,
    HangOnDelete,
    VanishAfterCall,
    InitFails,
    InitWithoutSession,
}

struct Seen {
    method: String,
    headers: Vec<String>,
}

impl Seen {
    fn header(&self, name: &str) -> Option<String> {
        let prefix = format!("{name}:");
        self.headers.iter().find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix(&prefix)
                .map(|v| v.trim().to_string())
        })
    }
}

fn read_request(stream: &mut UnixStream) -> Option<(Seen, String)> {
    let request = tuic_test_support::read_http_request(stream).ok()?;
    let method = request.request_line.split_whitespace().next()?.to_string();
    Some((
        Seen {
            method,
            headers: request
                .headers
                .iter()
                .map(|line| line.trim_end().to_string())
                .collect(),
        },
        String::from_utf8_lossy(&request.body).into_owned(),
    ))
}

fn reply(stream: &mut UnixStream, status: &str, body: &str, session: bool) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\n{}Connection: close\r\n\r\n{body}",
        body.len(),
        if session {
            "Mcp-Session-Id: test-session\r\n"
        } else {
            ""
        }
    );
}

fn tool_body(text: &str, is_error: bool) -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "result": {"isError": is_error, "content": [{"type": "text", "text": text}]}
    })
    .to_string()
}

fn run(
    args: &[&str],
    mode: Mode,
    tuic_session: Option<&str>,
    tool_text: &str,
    is_error: bool,
) -> (Output, Vec<Seen>, Duration) {
    let root =
        std::path::PathBuf::from(std::env::var("HOME").unwrap()).join("Gits/.tmp/tuic-tests");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join(format!(
        "tuic-drop-{}-{}.sock",
        std::process::id(),
        NEXT_SOCKET.fetch_add(1, Ordering::Relaxed)
    ));
    let listener = UnixListener::bind(&path).unwrap();
    listener.set_nonblocking(true).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = Arc::clone(&stop);
    let text = tool_text.to_string();
    let server = std::thread::spawn(move || {
        let mut seen = Vec::new();
        let mut held = Vec::new();
        while !stop_thread.load(Ordering::Relaxed) {
            let Ok((mut stream, _)) = listener.accept() else {
                std::thread::sleep(Duration::from_millis(5));
                continue;
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let Some((request, body)) = read_request(&mut stream) else {
                continue;
            };
            let method = request.method.clone();
            seen.push(request);
            if method == "DELETE" {
                if mode == Mode::HangOnDelete {
                    held.push(stream);
                } else {
                    reply(&mut stream, "200 OK", "", false);
                }
            } else if method == "GET" {
                reply(&mut stream, "200 OK", "{}", false);
            } else if body.contains("\"initialize\"") {
                match mode {
                    Mode::InitFails => reply(&mut stream, "500 Internal Server Error", "", false),
                    Mode::InitWithoutSession => reply(&mut stream, "200 OK", "{}", false),
                    _ => reply(&mut stream, "200 OK", "{}", true),
                }
            } else {
                reply(&mut stream, "200 OK", &tool_body(&text, is_error), false);
                if mode == Mode::VanishAfterCall {
                    // The socket file stays; connecting to it now fails.
                    return seen;
                }
            }
        }
        drop(held);
        seen
    });

    let mut command = Command::new(env!("CARGO_BIN_EXE_tuic"));
    command
        .args(args)
        .env("TUIC_SOCKET", &path)
        .env_remove("TUIC_SESSION");
    if let Some(sid) = tuic_session {
        command.env("TUIC_SESSION", sid);
    }
    let started = Instant::now();
    let output = command.output().unwrap();
    let elapsed = started.elapsed();
    stop.store(true, Ordering::Relaxed);
    let seen = server.join().unwrap();
    let _ = std::fs::remove_file(&path);
    (output, seen, elapsed)
}

fn methods(seen: &[Seen]) -> Vec<&str> {
    seen.iter()
        .map(|s| s.method.as_str())
        .filter(|m| *m != "GET")
        .collect()
}

/// Catches: Drop sending the DELETE without the session id, the PTY identity or
/// the pid, or sending it twice.
#[test]
fn one_delete_follows_the_call_and_names_session_identity_and_pid() {
    let (output, seen, _) = run(
        PEERS,
        Mode::Normal,
        Some("peer-crit"),
        "{\"peers\":[]}",
        false,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(methods(&seen), ["POST", "POST", "DELETE"]);
    let delete = seen.iter().find(|s| s.method == "DELETE").unwrap();
    assert_eq!(
        delete.header("mcp-session-id").as_deref(),
        Some("test-session")
    );
    assert_eq!(
        delete.header("x-tuic-session").as_deref(),
        Some("peer-crit")
    );
    let pid = delete
        .header("x-tuic-client-pid")
        .expect("DELETE must carry the pid");
    assert!(
        !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit()),
        "{pid}"
    );
}

/// Catches: the external-caller path (register + call on one client) sending
/// DELETE before the register or once per inner call instead of once at the end.
#[test]
fn an_external_caller_releases_its_session_once_after_register_and_call() {
    let (output, seen, _) = run(INBOX, Mode::Normal, None, "{\"ok\":true}", false);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(methods(&seen), ["POST", "POST", "POST", "DELETE"]);
}

/// Catches: the error path skipping the release (an early return or exit before
/// the client drops), which would keep one session per failed command.
#[test]
fn a_failed_tool_call_still_releases_the_session() {
    let (output, seen, _) = run(
        PEERS,
        Mode::Normal,
        Some("peer-crit"),
        "{\"error\":\"boom\"}",
        true,
    );
    assert!(
        !output.status.success(),
        "a tool error must still fail the command"
    );
    assert_eq!(
        methods(&seen).last().copied(),
        Some("DELETE"),
        "{:?}",
        methods(&seen)
    );
}

/// Catches: Drop blocking without bound on a server that accepted the DELETE
/// and never answers, adding unbounded latency to every tuic call.
#[test]
fn a_server_that_never_answers_delete_cannot_hold_the_command() {
    let (output, seen, elapsed) = run(
        PEERS,
        Mode::HangOnDelete,
        Some("peer-crit"),
        "{\"peers\":[]}",
        false,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !output.stdout.is_empty(),
        "the result must still be printed"
    );
    assert_eq!(methods(&seen).last().copied(), Some("DELETE"));
    assert!(
        elapsed < Duration::from_secs(6),
        "DELETE held the command for {elapsed:?}"
    );
}

/// Catches: Drop panicking or turning a finished call into a failure when the
/// server is gone by the time the DELETE is sent.
#[test]
fn a_server_gone_before_delete_does_not_change_the_outcome() {
    let (output, seen, _) = run(
        PEERS,
        Mode::VanishAfterCall,
        Some("peer-crit"),
        "{\"peers\":[]}",
        false,
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(!stderr.contains("panicked"), "{stderr}");
    assert_ne!(output.stdout, [] as [u8; 0]);
    assert_eq!(methods(&seen), ["POST", "POST"]);
}

/// Catches: a DELETE for a session that was never established (initialize
/// refused, or accepted without a session id).
#[test]
fn no_delete_is_sent_when_initialize_never_produced_a_session() {
    for mode in [Mode::InitFails, Mode::InitWithoutSession] {
        let (output, seen, _) = run(PEERS, mode, Some("peer-crit"), "{}", false);
        assert!(!output.status.success());
        assert_eq!(methods(&seen), ["POST"]);
    }
}
