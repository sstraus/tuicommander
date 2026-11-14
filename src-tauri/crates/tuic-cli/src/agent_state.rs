//! `tuic agent-state` — the native agent hook transport (#2632-97ef).
//!
//! Native Windows has no `sh`, `base64`, `ps` or `curl --unix-socket`, so the
//! POSIX hook commands cannot run there. Each generated hook instead runs
//! `tuic agent-state <verb> [state]`, which builds the same OSC 7770 frames and
//! posts them to `POST /sessions/$TUIC_SESSION/hook` over the existing local IPC
//! (the named pipe on Windows), where the PTY reader's own processor handles
//! them like terminal output.
//!
//! Order: the post carries the system-wide monotonic clock reading taken when
//! this process started, plus its pid. The server refuses a stamp older than
//! the PTY generation or the last settled turn and a repeated `(pid, stamp)`,
//! so a hook the agent gave up on cannot revive a settled turn. Arrival order
//! and the wall clock are never used.
//!
//! Fallback: when no server answers, or the server that answers does not know
//! the session (the single Windows pipe can belong to another instance), the
//! frames go in-band to the console (`CONOUT$`), which ConPTY delivers
//! unchanged (#2631-1f9f). A refused, malformed or lost post never falls back:
//! the server may already have applied it.
//!
//! Inert outside TUIC and, on Windows, without a console (a headless agent
//! started from a tool call). Always exits 0 so a hook never blocks its agent.

use std::io::Read;

use base64::Engine as _;

/// Raw stdin kept for one receipt: its base64 (1 MiB) is what the session
/// reader keeps for one hook receipt. A larger event sends its coarse state only.
pub(crate) const STDIN_LIMIT: usize = 768 * 1024;

/// How long a hook waits for the server before giving up (as `curl -m 5`).
const POST_DEADLINE: std::time::Duration = std::time::Duration::from_secs(5);

fn osc(content: &str) -> String {
    format!("\x1b]7770;{content}\x1b\\")
}

fn coarse(state: Option<&str>) -> String {
    state
        .map(|state| osc(&format!("state={state}")))
        .unwrap_or_default()
}

/// Whether a Codex `notify` payload reports a finished turn (the POSIX script
/// matches the same two spellings).
fn codex_turn_complete(payload: &str) -> bool {
    payload.contains(r#""type":"agent-turn-complete""#)
        || payload.contains(r#""type": "agent-turn-complete""#)
}

/// The frames one hook invocation sends: byte-identical to the POSIX hook of
/// the same verb. `None` means nothing to send (unknown verb or no event).
pub(crate) fn frames(
    verb: &str,
    state: Option<&str>,
    stdin: Option<&[u8]>,
    tab: &str,
    pid: u32,
) -> Option<String> {
    let receipt = stdin.map(|bytes| base64::engine::general_purpose::STANDARD.encode(bytes));
    let state = state.filter(|state| is_state(state));
    match verb {
        "state" => state.map(|state| osc(&format!("state={state}"))),
        "claude" => Some(match receipt {
            Some(payload) => format!(
                "{}{}{}",
                osc(&format!("claude-hook={tab}:{pid}:{payload}")),
                osc(&format!("claude-hook={tab}:{pid}:")),
                coarse(state)
            ),
            None => coarse(state),
        })
        .filter(|frames| !frames.is_empty()),
        "codex-session" => Some(match receipt {
            Some(payload) => format!(
                "{}{}",
                osc(&format!("codex-session={tab}:{payload}")),
                coarse(state)
            ),
            None => coarse(state),
        })
        .filter(|frames| !frames.is_empty()),
        "codex-tool" => receipt.map(|payload| osc(&format!("codex-tool={tab}:{payload}"))),
        "grok" => receipt.map(|payload| osc(&format!("grok-hook={tab}:{payload}"))),
        _ => None,
    }
}

fn is_state(state: &str) -> bool {
    matches!(state, "busy" | "idle" | "awaiting" | "prompt")
}

/// Verbs whose event arrives on stdin.
fn reads_stdin(verb: &str) -> bool {
    matches!(verb, "claude" | "codex-session" | "codex-tool" | "grok")
}

/// Read stdin up to [`STDIN_LIMIT`]; `None` when it is larger or unreadable.
fn bounded_stdin(input: impl Read) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    input
        .take(STDIN_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() <= STDIN_LIMIT).then_some(bytes)
}

/// What the server's answer means for the in-band fallback.
pub(crate) fn falls_back(post: &crate::ipc::HookPost) -> bool {
    use crate::ipc::HookPost;
    matches!(post, HookPost::Unreachable | HookPost::Answered(404))
}

/// Entry point for `tuic agent-state <verb> [state] [-- notify argv]`.
/// Returns the process exit code: 0, or the chained Codex notify's code.
pub(crate) fn run(args: &[String]) -> i32 {
    let stamp = tuic_ipc::hook_clock::now();
    let pid = std::process::id();
    let Some((verb, rest)) = args.split_first() else {
        return 0;
    };
    let (own, chain) = match rest.iter().position(|arg| arg == "--") {
        Some(at) => (&rest[..at], &rest[at + 1..]),
        None => (rest, &[][..]),
    };
    // The ownership marker the installer looks for rides as a last argument.
    let state = own
        .first()
        .map(String::as_str)
        .filter(|arg| !arg.starts_with('#'));
    if verb == "codex-notify" {
        // Codex appends its JSON payload as the last argument.
        let payload = chain.last().map(String::as_str).unwrap_or_default();
        if codex_turn_complete(payload) {
            send(stamp, pid, "state", Some("idle"), None);
        }
        return chain_notify(chain);
    }
    let stdin = if reads_stdin(verb) {
        bounded_stdin(std::io::stdin().lock())
    } else {
        None
    };
    send(stamp, pid, verb, state, stdin.as_deref());
    0
}

fn send(stamp: u64, pid: u32, verb: &str, state: Option<&str>, stdin: Option<&[u8]>) {
    let Some(tab) = std::env::var("TUIC_SESSION")
        .ok()
        .filter(|tab| !tab.is_empty())
    else {
        return;
    };
    let Some(mut console) = console::open() else {
        return;
    };
    let Some(body) = frames(verb, state, stdin, &tab, pid) else {
        return;
    };
    let path = format!("/sessions/{tab}/hook");
    let (stamp, pid) = (stamp.to_string(), pid.to_string());
    let (sender, receiver) = std::sync::mpsc::channel();
    let posted = body.clone();
    std::thread::spawn(move || {
        let headers = [
            ("x-tuic-hook-stamp", stamp.as_str()),
            ("x-tuic-hook-pid", pid.as_str()),
        ];
        let _ = sender.send(crate::ipc::post_hook(&path, &posted, &headers));
    });
    let post = receiver
        .recv_timeout(POST_DEADLINE)
        .unwrap_or(crate::ipc::HookPost::Lost);
    if falls_back(&post) {
        use std::io::Write as _;
        let _ = console.write_all(body.as_bytes());
        let _ = console.flush();
    }
}

/// Run the user's own Codex `notify` command with the payload, as the POSIX
/// script `exec`s it.
fn chain_notify(chain: &[String]) -> i32 {
    // The last argument is the payload Codex appended; anything before it is
    // the user's own command.
    let Some((program, args)) = chain.split_first().filter(|_| chain.len() > 1) else {
        return 0;
    };
    std::process::Command::new(program)
        .args(args)
        .status()
        .ok()
        .and_then(|status| status.code())
        .unwrap_or(0)
}

/// The console this hook's agent draws on: the in-band fallback channel, and
/// the proof that this hook belongs to an interactive session at all.
mod console {
    // DEFERRED (2026-11-05) — a headless agent that a TUIC agent starts from a
    // tool call (`codex exec` from a Claude tab) shares the parent's console on
    // Windows, so `CONOUT$` opens and its hooks post to the parent tab. Unix
    // keeps such hooks inert through the controlling-tty check (#2749-bee7);
    // the Windows equivalent needs a lab capture of the console and handle
    // inheritance first (to-test.md, #2632-97ef).
    #[cfg(windows)]
    pub(super) fn open() -> Option<std::fs::File> {
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("CONOUT$")
            .ok()
    }

    /// Unix agents use the POSIX hooks; this verb only posts there.
    #[cfg(unix)]
    pub(super) fn open() -> Option<std::io::Sink> {
        Some(std::io::sink())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::HookPost;

    const ST: &str = "\x1b\\";

    // Catches: the Windows transport sends frames the session reader does not
    // accept or decode, so a hook that worked on Unix is silently lost.
    #[test]
    fn frames_match_the_posix_hook_shapes() {
        let event = br#"{"hook_event_name":"PreToolUse"}"#;
        let b64 = "eyJob29rX2V2ZW50X25hbWUiOiJQcmVUb29sVXNlIn0=";
        assert_eq!(
            frames("state", Some("idle"), None, "tab", 7).unwrap(),
            format!("\x1b]7770;state=idle{ST}")
        );
        assert_eq!(
            frames("claude", Some("busy"), Some(event), "tab", 7).unwrap(),
            format!(
                "\x1b]7770;claude-hook=tab:7:{b64}{ST}\x1b]7770;claude-hook=tab:7:{ST}\x1b]7770;state=busy{ST}"
            )
        );
        assert_eq!(
            frames("claude", None, Some(event), "tab", 7).unwrap(),
            format!("\x1b]7770;claude-hook=tab:7:{b64}{ST}\x1b]7770;claude-hook=tab:7:{ST}")
        );
        assert_eq!(
            frames("codex-tool", None, Some(event), "tab", 7).unwrap(),
            format!("\x1b]7770;codex-tool=tab:{b64}{ST}")
        );
        assert_eq!(
            frames("grok", None, Some(event), "tab", 7).unwrap(),
            format!("\x1b]7770;grok-hook=tab:{b64}{ST}")
        );
    }

    // Catches: Codex SessionStart/UserPromptSubmit stdin (thread id, rollout
    // path, cwd) is dropped on Windows, so Chat never binds the Codex thread.
    #[test]
    fn codex_identity_stdin_rides_with_its_state() {
        let event = br#"{"session_id":"019a","transcript_path":"C:\\Users\\\xc3\xa9 x\\r.jsonl"}"#;
        let body = frames("codex-session", Some("prompt"), Some(event), "tab", 1).unwrap();
        let b64 = base64::engine::general_purpose::STANDARD.encode(event);
        assert_eq!(
            body,
            format!("\x1b]7770;codex-session=tab:{b64}{ST}\x1b]7770;state=prompt{ST}")
        );
    }

    // Catches: an unknown verb, an invented state or a stdin-less receipt
    // reaches the reducer as a half frame.
    #[test]
    fn nothing_is_sent_for_unknown_or_empty_input() {
        assert_eq!(frames("state", Some("working"), None, "tab", 1), None);
        assert_eq!(frames("bogus", Some("idle"), None, "tab", 1), None);
        assert_eq!(frames("grok", None, None, "tab", 1), None);
        assert_eq!(frames("claude", None, None, "tab", 1), None);
    }

    // Catches: an unbounded stdin read lets one huge tool result grow the hook
    // without limit, or a receipt over the reader cap is sent and then dropped.
    #[test]
    fn stdin_is_bounded_to_the_reader_receipt_cap() {
        assert_eq!(
            bounded_stdin(&vec![b'x'; STDIN_LIMIT][..]).map(|b| b.len()),
            Some(STDIN_LIMIT)
        );
        assert_eq!(bounded_stdin(&vec![b'x'; STDIN_LIMIT + 1][..]), None);
        let encoded = base64::engine::general_purpose::STANDARD.encode(vec![0u8; STDIN_LIMIT]);
        assert!(encoded.len() <= 1024 * 1024);
    }

    // Catches: a refused (stale or duplicate) or possibly-applied post is
    // replayed in-band and revives the turn the server just refused, or an
    // unreachable/foreign-instance server loses the frame entirely.
    #[test]
    fn only_unreachable_or_unknown_session_falls_back_in_band() {
        assert!(falls_back(&HookPost::Unreachable));
        assert!(falls_back(&HookPost::Answered(404)));
        for kept in [
            HookPost::Answered(204),
            HookPost::Answered(409),
            HookPost::Answered(400),
            HookPost::Answered(500),
            HookPost::Lost,
        ] {
            assert!(!falls_back(&kept), "{kept:?}");
        }
    }

    // Catches: the Codex notify bridge misses a finished turn or claims one
    // for another notification type.
    #[test]
    fn codex_notify_detects_turn_complete_in_both_spellings() {
        assert!(codex_turn_complete(r#"{"type":"agent-turn-complete"}"#));
        assert!(codex_turn_complete(r#"{"type": "agent-turn-complete"}"#));
        assert!(!codex_turn_complete(r#"{"type":"approval-requested"}"#));
    }
}
