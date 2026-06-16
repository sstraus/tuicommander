//! Native-agent hook command generation (emit side of hook-based agent state).
//!
//! Each supported agent (Claude, Gemini, …) drives its busy/idle/awaiting state
//! by running a small shell hook that emits `OSC 7770;state=…` to its controlling
//! tty. This module generates those hook commands and the per-agent event→state
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

/// A single hook registration: `(event, matcher, command)`.
/// `matcher == ""` means "all" (no tool-name filter).
pub(crate) type HookEntry = (&'static str, &'static str, String);

/// Resolve the controlling tty into `$__t`, even when the caller's stdout is
/// captured (hooks have no controlling tty of their own — read the parent's).
fn tty_resolve() -> &'static str {
    r#"__t=$(ps -o tty= -p "$PPID" 2>/dev/null|tr -d '[:space:]');case "$__t" in *[0-9]*)__t="/dev/${__t#/dev/}";;*)__t="/dev/tty";;esac"#
}

/// Generate the guarded, self-contained shell command that emits
/// `OSC 7770;state=<state>` to the controlling tty. Inert outside TUIC, always
/// exits 0, ends with the ownership sentinel.
pub(crate) fn hook_command(state: &str) -> String {
    format!(
        r#"[ -n "${{TUIC_SESSION:-}}" ] && {{ {tty}; printf '\033]7770;state={state}\033\\' > "$__t"; }} >/dev/null 2>&1 || true {SENTINEL}"#,
        tty = tty_resolve(),
    )
}

/// Claude hooks (tool-level). Array order matters: the broad `PreToolUse` busy
/// entry precedes the `AskUserQuestion|ExitPlanMode` awaiting entry so awaiting
/// wins for those tools.
pub(crate) fn claude_hook_map() -> Vec<HookEntry> {
    vec![
        ("UserPromptSubmit", "", hook_command("busy")),
        ("PreToolUse", "", hook_command("busy")),
        (
            "PreToolUse",
            "AskUserQuestion|ExitPlanMode",
            hook_command("awaiting"),
        ),
        (
            "PostToolUse",
            "AskUserQuestion|ExitPlanMode",
            hook_command("busy"),
        ),
        ("Stop", "", hook_command("idle")),
        ("SessionEnd", "", hook_command("idle")),
    ]
}

/// Gemini hooks (same shell-hook shape, different event names; v0.26+).
pub(crate) fn gemini_hook_map() -> Vec<HookEntry> {
    vec![
        ("BeforeAgent", "", hook_command("busy")),
        ("BeforeTool", "", hook_command("busy")),
        ("AfterAgent", "", hook_command("idle")),
        ("Notification", "", hook_command("awaiting")),
        ("SessionEnd", "", hook_command("idle")),
    ]
}

/// Grok hooks (Claude-compatible JSON schema, written to our OWN file
/// `~/.grok/hooks/tuic.json`). Event names verified against the in-app hooks doc
/// (`~/.grok/docs/user-guide/10-hooks.md`). Lifecycle events (UserPromptSubmit,
/// Stop, SessionEnd) reject a matcher, so all entries use an empty matcher (the
/// own-file writer omits it). Grok has no clean "awaiting" event — approval
/// prompts are covered by the existing OSC-0 title heuristic, which is not
/// suppressed under instrumentation.
pub(crate) fn grok_hook_map() -> Vec<HookEntry> {
    vec![
        ("UserPromptSubmit", "", hook_command("busy")),
        ("PreToolUse", "", hook_command("busy")),
        ("Stop", "", hook_command("idle")),
        ("SessionEnd", "", hook_command("idle")),
    ]
}

/// Codex hooks (Claude-compatible JSON schema, merged into `~/.codex/hooks.json`,
/// gated by a `[features] hooks = true` flag in `config.toml`). Turn-level only:
/// Codex doesn't expose PreToolUse/PostToolUse usefully (Bash-only) and has no
/// SessionEnd — the badge clears via the idle/Stop event. SessionStart fires on
/// the first turn (not session open), so busy appears once the user submits.
#[allow(dead_code)] // Incremental build: consumed by the Codex adapter (story 050)
pub(crate) fn codex_hook_map() -> Vec<HookEntry> {
    vec![
        ("SessionStart", "", hook_command("busy")),
        ("UserPromptSubmit", "", hook_command("busy")),
        ("Stop", "", hook_command("idle")),
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
}
