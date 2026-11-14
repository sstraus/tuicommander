//! Native-agent hook command generation (emit side of hook-based agent state).
//!
//! Each supported agent (Claude, Gemini, …) drives its busy/idle/awaiting state
//! by running a small shell hook that posts `OSC 7770;state=…` frames to the
//! local TUIC socket (`post_frames`), with its controlling tty as the fallback. This module generates those hook commands and the per-agent event→state
//! maps the installer (see `agent_hook_installer`) writes into the agent's
//! settings file.
//!
//! The command is inert outside TUIC (guarded on `TUIC_SESSION`), resolves the
//! controlling tty from a context where stdout is captured by the agent
//! (`ps -o tty= -p $PPID`, validated end-to-end by the injection spike, story
//! 042), and always exits 0 so a hook can never block the agent. Ownership is a
//! trailing shell-comment sentinel so the installer prunes only TUIC's entries
//! and never touches user/wiz hooks.

/// Trailing shell comment marking a hook command as TUIC-owned. The installer
/// keys ownership off this — a valid comment in Claude/Gemini/Codex command
/// fields alike.
pub(crate) const SENTINEL: &str = "# tuic-managed-hook";

/// The label Codex `/hooks` shows for a TUIC entry (its `statusMessage` field);
/// without it Codex lists the entry as "Unnamed hook" (proved live, codex 0.166.0).
pub(crate) const CODEX_HOOK_LABEL: &str = "TUICommander session";

/// A single hook registration: `(event, matcher, command)`.
/// `matcher == ""` means "all" (no tool-name filter).
pub(crate) type HookEntry = (&'static str, &'static str, String);

/// How a generated hook delivers its frames (#2632-97ef).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum HookTransport {
    /// `sh` hooks: the side-channel socket post with the tty fallback.
    Posix,
    /// Native Windows, which has no `sh`, `ps` or Unix socket: the bundled
    /// `tuic.exe agent-state` posts over the named pipe, with `CONOUT$` as the
    /// in-band fallback. Holds the CLI path.
    Cli(String),
}

impl HookTransport {
    /// This host's transport; `None` when generated hooks cannot run here
    /// (native Windows without a usable bundled CLI).
    pub(crate) fn host() -> Option<Self> {
        if !cfg!(windows) {
            return Some(Self::Posix);
        }
        static CLI: std::sync::OnceLock<Option<HookTransport>> = std::sync::OnceLock::new();
        CLI.get_or_init(|| {
            let exe = std::env::current_exe().ok()?;
            let cli = exe.parent()?.join("tuic.exe");
            cli.is_file().then(|| Self::windows_cli(&cli)).flatten()
        })
        .clone()
    }

    /// The Windows transport for the CLI at `cli`, or `None` when its path
    /// cannot be quoted the same way for `cmd`, `sh` and `bash`: inside double
    /// quotes `cmd` expands `%` (and `!`), and `sh` expands `$` and backticks.
    pub(crate) fn windows_cli(cli: &std::path::Path) -> Option<Self> {
        let path = cli.to_str()?;
        (!path.contains(['"', '%', '!', '$', '`'])).then(|| Self::Cli(path.to_owned()))
    }

    /// Generated hooks need a transport; an unsupported host falls back to the
    /// POSIX shape, which nothing installs there.
    fn host_or_posix() -> Self {
        Self::host().unwrap_or(Self::Posix)
    }
}

/// Whether generated agent hooks can run on this host.
pub(crate) fn hooks_supported_on_host() -> bool {
    HookTransport::host().is_some()
}

/// `"<cli>" agent-state <args> "# tuic-managed-hook"`: the quoted path keeps
/// spaces and non-ASCII names intact in `cmd`, `sh` and `bash`; the quoted
/// sentinel is the installer's ownership mark and an argument the CLI ignores.
fn cli_command(cli: &str, args: &[&str]) -> String {
    format!("\"{cli}\" agent-state {} \"{SENTINEL}\"", args.join(" "))
}

fn state_hook(transport: &HookTransport, state: &str) -> String {
    match transport {
        HookTransport::Posix => hook_command(state),
        HookTransport::Cli(cli) => cli_command(cli, &["state", state]),
    }
}

fn claude_hook(transport: &HookTransport, state: Option<&str>) -> String {
    match transport {
        HookTransport::Posix => claude_hook_command(state),
        HookTransport::Cli(cli) => cli_command(cli, &[&["claude"][..], state.as_slice()].concat()),
    }
}

/// Resolve the controlling tty into `$__t`, even when the caller's stdout is
/// captured (hooks have no controlling tty of their own — read the parent's).
pub(crate) fn tty_resolve() -> &'static str {
    r#"__t=$(ps -o tty= -p "$PPID" 2>/dev/null|tr -d '[:space:]');case "$__t" in *[0-9]*)__t="/dev/${__t#/dev/}";;*)__t="/dev/tty";;esac"#
}

/// Generate the guarded, self-contained shell command that emits
/// `OSC 7770;state=<state>` through the side channel. `prompt` is busy plus the
/// "user submitted a prompt here" mark; only the submit-prompt hooks use it. Inert outside TUIC, always
/// exits 0, ends with the ownership sentinel.
pub(crate) fn hook_command(state: &str) -> String {
    hook_shell(
        &format!(r"__f=$(printf '\033]7770;state={state}\033\\')"),
        TTY_FRAMES,
    )
}

/// Write the frames in `$__f` to the tty `$__t` (the fallback channel).
pub(crate) const TTY_FRAMES: &str = r#"printf %s "$__f" > "$__t""#;

/// Post the frames in `$__f` to `POST /sessions/$TUIC_SESSION/hook` on the
/// local TUIC socket, so they never share the tty with the agent TUI
/// (#2749-bee7). When no socket answers, `curl` is missing or TUIC refuses the
/// post, run `fallback` with the controlling tty in `$__t`. A timeout (exit
/// 28) does not fall back: TUIC may have processed the frames already.
///
/// Post only when the parent has a controlling tty. A headless agent that a
/// TUIC agent starts from a tool call (`codex exec` from a Claude tab) inherits
/// `TUIC_SESSION` and `TUIC_SOCKET` but has no tty; its hooks must stay inert,
/// as they were on the tty path, or they close the parent tab's turn.
pub(crate) fn post_frames(fallback: &str) -> String {
    format!(
        r#"{tty};__r=1;case "$__t" in /dev/tty);;*)[ -S "${{TUIC_SOCKET:-}}" ]&&{{ printf %s "$__f"|curl -sf -o /dev/null -m 5 -H 'Expect:' --unix-socket "$TUIC_SOCKET" --data-binary @- "http://tuic/sessions/$TUIC_SESSION/hook";__r=$?;}};;esac;[ $__r -eq 0 ]||[ $__r -eq 28 ]||{{ {fallback};}}"#,
        tty = tty_resolve(),
    )
}

/// A guarded hook: `build` sets `$__f` (it may read stdin), then the frames are
/// posted. Inert outside TUIC, always exits 0, ends with the ownership sentinel.
fn hook_shell(build: &str, fallback: &str) -> String {
    format!(
        r#"[ -n "${{TUIC_SESSION:-}}" ] && {{ {build};{post}; }} >/dev/null 2>&1 || true {SENTINEL}"#,
        post = post_frames(fallback),
    )
}

/// Preserve stdin metadata for permission ownership, optionally followed by the
/// existing coarse signal. Questions and elicitation keep their current hooks.
///
/// The side channel carries the whole receipt in one frame
/// `claude-hook=<tab>:<hook pid>:<base64>`, then an empty piece ends it; the
/// hook pid keeps concurrent hooks apart. The reader keeps at most 1 MB of
/// base64 for each receipt.
///
/// The tty fallback shares the tty with the agent redraw. `sh` printf writes in
/// 1024-byte pieces and a redraw escape cut one large frame (#2747-636f), so
/// each fallback frame carries at most 768 base64 bytes. One write is not
/// atomic either: above the output high-water mark a macOS pty writer sleeps
/// mid-write and another writer gets in, so a fallback receipt over 32 KB of
/// base64 is not sent.
fn claude_hook_command(state: Option<&str>) -> String {
    let coarse = state
        .map(|state| format!("\\033]7770;state={state}\\033\\\\"))
        .unwrap_or_default();
    hook_shell(
        &format!(
            r#"__p=$(base64 | tr -d '\r\n');__f=$(printf '\033]7770;claude-hook=%s:%s:%s\033\\\033]7770;claude-hook=%s:%s:\033\\{coarse}' "$TUIC_SESSION" "$$" "$__p" "$TUIC_SESSION" "$$")"#
        ),
        &format!(
            r#"[ ${{#__p}} -le 32768 ] && for __c in $(printf %s "$__p" | fold -w 768); do printf '\033]7770;claude-hook=%s:%s:%s\033\\' "$TUIC_SESSION" "$$" "$__c" >> "$__t"; done; printf '\033]7770;claude-hook=%s:%s:\033\\{coarse}' "$TUIC_SESSION" "$$" >> "$__t""#
        ),
    )
}

/// Claude hooks (tool-level). Array order matters: the broad `PreToolUse` busy
/// entry precedes the `AskUserQuestion|ExitPlanMode` awaiting entry so awaiting
/// wins for those tools.
///
/// `Elicitation` covers MCP `elicitation/create` — an MCP server asking the user
/// for input mid tool call. It is NOT a tool call, so no `PreToolUse` matcher can
/// reach it, and its dialog matches none of the screen heuristics either: the
/// options render horizontally (`Accept  Decline`) instead of numbered, and the
/// footer is `Esc to cancel · ↑/↓ to navigate · …`, not `Enter to select`. Without
/// this entry the tab stays "busy" while the agent is blocked on the user.
/// `ElicitationResult` fires once the user answers and is the paired retraction —
/// awaiting is sticky, so a set with no clear latches the badge forever.
pub(crate) fn claude_hook_map() -> Vec<HookEntry> {
    claude_hook_map_on(&HookTransport::host_or_posix())
}

pub(crate) fn claude_hook_map_on(t: &HookTransport) -> Vec<HookEntry> {
    vec![
        ("UserPromptSubmit", "", claude_hook(t, Some("prompt"))),
        ("PreToolUse", "", claude_hook(t, Some("busy"))),
        (
            "PreToolUse",
            "AskUserQuestion|ExitPlanMode",
            state_hook(t, "awaiting"),
        ),
        (
            "PostToolUse",
            "AskUserQuestion|ExitPlanMode",
            state_hook(t, "busy"),
        ),
        ("PermissionRequest", "", claude_hook(t, None)),
        ("Notification", "permission_prompt", claude_hook(t, None)),
        ("PostToolUse", "", claude_hook(t, None)),
        ("PostToolUseFailure", "", claude_hook(t, None)),
        ("Elicitation", "", claude_hook(t, Some("awaiting"))),
        ("ElicitationResult", "", claude_hook(t, Some("busy"))),
        ("Stop", "", state_hook(t, "idle")),
        ("StopFailure", "", state_hook(t, "idle")),
        ("SessionEnd", "", state_hook(t, "idle")),
    ]
}

/// Gemini hooks (same shell-hook shape, different event names; v0.26+).
pub(crate) fn gemini_hook_map() -> Vec<HookEntry> {
    gemini_hook_map_on(&HookTransport::host_or_posix())
}

pub(crate) fn gemini_hook_map_on(t: &HookTransport) -> Vec<HookEntry> {
    vec![
        ("BeforeAgent", "", state_hook(t, "prompt")),
        ("BeforeTool", "", state_hook(t, "busy")),
        ("AfterAgent", "", state_hook(t, "idle")),
        ("Notification", "", state_hook(t, "awaiting")),
        ("SessionEnd", "", state_hook(t, "idle")),
    ]
}

/// Carry native ids together so delayed cancellation and parallel tools cannot
/// settle another request. Rust decodes only lifecycle metadata for consumers.
fn grok_hook_command() -> String {
    hook_shell(
        r#"__p=$(base64 | tr -d '\r\n');__f=$(printf '\033]7770;grok-hook=%s:%s\033\\' "$TUIC_SESSION" "$__p")"#,
        TTY_FRAMES,
    )
}

/// Native events recorded from Grok 1.0.70. Only permission notifications ask
/// for human input; idle/task notifications are deliberately not registered.
pub(crate) fn grok_hook_map() -> Vec<HookEntry> {
    grok_hook_map_on(&HookTransport::host_or_posix())
}

pub(crate) fn grok_hook_map_on(t: &HookTransport) -> Vec<HookEntry> {
    let command = match t {
        HookTransport::Posix => grok_hook_command(),
        HookTransport::Cli(cli) => cli_command(cli, &["grok"]),
    };
    [
        ("SessionStart", ""),
        ("UserPromptSubmit", ""),
        ("PreToolUse", ""),
        ("Notification", "permission_prompt"),
        ("PostToolUse", ""),
        ("PostToolUseFailure", ""),
        ("PermissionDenied", ""),
        ("Stop", ""),
        ("StopCancelled", ""),
        ("StopFailure", ""),
        ("SessionEnd", ""),
    ]
    .into_iter()
    .map(|(event, matcher)| (event, matcher, command.clone()))
    .collect()
}

/// Codex native SessionStart/UserPromptSubmit stdin carries the exact thread id,
/// rollout path and cwd (recorded with 0.162.1). Reuse the guarded OSC channel;
/// older/untrusted hooks simply leave Chat unavailable.
fn codex_identity_command(state: &str) -> String {
    hook_shell(
        &format!(
            r#"__p=$(base64 | tr -d '\r\n');__f=$(printf '\033]7770;codex-session=%s:%s\033\\\033]7770;state={state}\033\\' "$TUIC_SESSION" "$__p")"#
        ),
        TTY_FRAMES,
    )
}

/// Preserve native session/turn/tool ids without putting JSON delimiters on the OSC wire.
fn codex_tool_command() -> String {
    hook_shell(
        r#"__p=$(base64 | tr -d '\r\n');__f=$(printf '\033]7770;codex-tool=%s:%s\033\\' "$TUIC_SESSION" "$__p")"#,
        TTY_FRAMES,
    )
}

pub(crate) fn codex_hook_map() -> Vec<HookEntry> {
    codex_hook_map_on(&HookTransport::host_or_posix())
}

pub(crate) fn codex_hook_map_on(t: &HookTransport) -> Vec<HookEntry> {
    let identity = |state: &str| match t {
        HookTransport::Posix => codex_identity_command(state),
        HookTransport::Cli(cli) => cli_command(cli, &["codex-session", state]),
    };
    let tool = match t {
        HookTransport::Posix => codex_tool_command(),
        HookTransport::Cli(cli) => cli_command(cli, &["codex-tool"]),
    };
    vec![
        ("SessionStart", "", identity("busy")),
        ("UserPromptSubmit", "", identity("prompt")),
        ("PreToolUse", "", tool.clone()),
        ("PostToolUse", "", tool.clone()),
        ("Interrupt", "", tool.clone()),
        // Child roster (#2635-8052): recorded with Codex 0.166.0.
        ("SubagentStart", "", tool.clone()),
        ("SubagentStop", "", tool.clone()),
        ("SessionEnd", "", tool),
        ("Stop", "", state_hook(t, "idle")),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_command_starts_with_tuic_session_guard() {
        let cmd = hook_command("busy");
        assert!(
            cmd.starts_with(r#"[ -n "${TUIC_SESSION"#),
            "must guard on TUIC_SESSION first: {cmd}"
        );
    }

    #[test]
    fn hook_command_contains_tty_resolve() {
        let cmd = hook_command("busy");
        assert!(
            cmd.contains(r#"ps -o tty= -p "$PPID""#),
            "must resolve the controlling tty: {cmd}"
        );
    }

    #[test]
    fn hook_command_emits_state_osc() {
        assert!(hook_command("busy").contains(r"\033]7770;state=busy\033"));
        assert!(hook_command("awaiting").contains(r"\033]7770;state=awaiting\033"));
        assert!(hook_command("idle").contains(r"\033]7770;state=idle\033"));
    }

    #[test]
    fn hook_command_ends_with_sentinel() {
        assert!(
            hook_command("idle").trim_end().ends_with(SENTINEL),
            "must end with the ownership sentinel"
        );
    }

    #[test]
    fn hook_command_always_exits_zero() {
        assert!(
            hook_command("busy").contains("|| true"),
            "must never block the agent (exit 0)"
        );
    }

    #[test]
    fn claude_map_has_awaiting_for_askuserquestion_and_stop_idle() {
        let map = claude_hook_map();
        let awaiting = map
            .iter()
            .find(|(e, m, _)| *e == "PreToolUse" && m.contains("AskUserQuestion"));
        let (_, _, cmd) =
            awaiting.expect("claude map must have a PreToolUse AskUserQuestion awaiting entry");
        assert!(cmd.contains("state=awaiting"));
        assert!(
            map.iter()
                .any(|(e, _, c)| *e == "Stop" && c.contains("state=idle")),
            "Stop must drive idle"
        );
        assert!(
            map.iter()
                .any(|(e, m, c)| *e == "PreToolUse" && m.is_empty() && c.contains("state=busy")),
            "broad PreToolUse must drive busy"
        );
    }

    /// Catches #1388: if UserPromptSubmit shared PreToolUse's `state=busy`, the
    /// scrollbar could not tell a prompt from a tool call and ticked both.
    #[test]
    fn submit_prompt_hooks_emit_prompt_not_busy() {
        for (map, event) in [
            (claude_hook_map(), "UserPromptSubmit"),
            (gemini_hook_map(), "BeforeAgent"),
            (codex_hook_map(), "UserPromptSubmit"),
        ] {
            let (_, _, cmd) = map.iter().find(|(e, _, _)| *e == event).unwrap();
            assert!(cmd.contains("state=prompt"), "{event}: {cmd}");
        }
        let map = grok_hook_map();
        let (_, _, command) = map
            .iter()
            .find(|(event, _, _)| *event == "UserPromptSubmit")
            .unwrap();
        assert!(command.contains("grok-hook="));
    }

    /// MCP elicitation blocks the agent on the user but is not a tool call, so
    /// the awaiting signal can only come from the dedicated `Elicitation` event —
    /// and it must be retracted by `ElicitationResult`, or the badge latches.
    #[test]
    fn claude_map_pairs_elicitation_awaiting_with_a_retraction() {
        let map = claude_hook_map();
        let find = |event: &str| {
            map.iter()
                .find(|(e, m, _)| *e == event && m.is_empty())
                .map(|(_, _, c)| c.clone())
        };
        let set = find("Elicitation").expect("MCP elicitation must set awaiting");
        assert!(set.contains("state=awaiting"), "{set}");
        let clear = find("ElicitationResult").expect("answered elicitation must clear awaiting");
        assert!(clear.contains("state=busy"), "{clear}");
    }

    #[test]
    fn gemini_map_has_notification_awaiting_and_afteragent_idle() {
        let map = gemini_hook_map();
        assert!(
            map.iter()
                .any(|(e, _, c)| *e == "Notification" && c.contains("state=awaiting"))
        );
        assert!(
            map.iter()
                .any(|(e, _, c)| *e == "AfterAgent" && c.contains("state=idle"))
        );
        assert!(
            map.iter()
                .any(|(e, _, c)| *e == "BeforeTool" && c.contains("state=busy"))
        );
    }

    // Catches: a headless agent that another TUIC agent starts from a tool call
    // (`codex exec` from a Claude tab) inherits TUIC_SESSION and TUIC_SOCKET but
    // has no controlling tty. The tty path made its hooks inert (/dev/tty fails);
    // the socket path posts its Stop `state=idle` into the parent tab mid-turn.
    #[cfg(unix)]
    #[test]
    fn a_hook_without_a_controlling_tty_posts_nothing_to_the_parent_session() {
        use std::os::unix::process::CommandExt;
        let dir = crate::test_support::short_socket_tempdir();
        let socket = dir.path().join("t.sock");
        let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        // The outer sh plays the headless agent, so the hook's $PPID has no tty.
        let mut command = std::process::Command::new("sh");
        command
            .args(["-c", r#"sh -c "$1""#, "agent"])
            .arg(hook_command("idle"))
            .env("TUIC_SESSION", "parent-tab")
            .env("TUIC_SOCKET", &socket)
            .stdin(std::process::Stdio::null());
        // SAFETY: setsid is async-signal-safe; it detaches the child from any
        // controlling tty, as a tool subprocess of an agent is.
        unsafe {
            command.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
        let status = command.status().unwrap();
        assert!(status.success());
        listener.set_nonblocking(true).unwrap();
        assert!(
            listener.accept().is_err(),
            "a hook with no controlling tty posted its frames to the parent session"
        );
    }
}

#[cfg(test)]
#[path = "agent_hook_codex_tests.rs"]
mod codex_tests;

#[cfg(test)]
#[path = "agent_hook_claude_tests.rs"]
mod claude_tests;

#[cfg(test)]
#[path = "agent_hook_channel_tests.rs"]
mod channel_tests;

#[cfg(test)]
#[path = "agent_hook_windows_tests.rs"]
mod windows_tests;
