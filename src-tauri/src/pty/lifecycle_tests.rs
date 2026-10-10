#[test]
fn pty_identity_defaults_claude_to_native_scrollback() {
    let state = crate::state::tests_support::make_test_app_state();
    let mut cmd = CommandBuilder::new("claude");
    bind_pty_identity(&state, &mut cmd, "screen-default", None);
    assert_eq!(
        cmd.get_env("CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN"),
        Some(std::ffi::OsStr::new("1"))
    );
    cmd.env("CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN", "0");
    assert_eq!(
        cmd.get_env("CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN"),
        Some(std::ffi::OsStr::new("0"))
    );

    let mut ipc = CommandBuilder::new("claude");
    bind_pty_identity(&state, &mut ipc, "screen-ipc", None);
    let mut http = CommandBuilder::new("claude");
    let env = std::collections::HashMap::new();
    apply_agent_screen_env(&mut ipc, &env);
    apply_agent_screen_env(&mut http, &env);
    assert_eq!(
        ipc.get_env("CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN"),
        http.get_env("CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN")
    );
    assert_eq!(
        ipc.get_env("CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN"),
        Some(std::ffi::OsStr::new("1"))
    );
}

#[test]
fn spawned_claude_disables_the_alt_screen_agent_view() {
    let state = crate::state::tests_support::make_test_app_state();
    let mut cmd = CommandBuilder::new("claude");
    bind_pty_identity(&state, &mut cmd, "agent-view-default", None);
    assert_eq!(
        cmd.get_env("CLAUDE_CODE_DISABLE_AGENT_VIEW"),
        Some(std::ffi::OsStr::new("1"))
    );
    // A caller's explicit value, applied afterwards, keeps precedence.
    cmd.env("CLAUDE_CODE_DISABLE_AGENT_VIEW", "0");
    assert_eq!(
        cmd.get_env("CLAUDE_CODE_DISABLE_AGENT_VIEW"),
        Some(std::ffi::OsStr::new("0"))
    );
}

#[test]
fn claude_screen_setting_off_preserves_explicit_environment() {
    let dir = tempfile::TempDir::new().unwrap();
    let _guard = tuic_core::config_dir::set_override(dir.path().to_path_buf());
    let mut config = crate::config::AgentsConfig::default();
    config.agents.insert(
        "claude".into(),
        crate::config::AgentSettings {
            prevent_alt_screen: Some(false),
            ..Default::default()
        },
    );
    crate::config::save_agents_config(crate::config::AgentsConfig::default(), config).unwrap();
    let state = crate::state::tests_support::make_test_app_state();
    let mut cmd = CommandBuilder::new("claude");
    cmd.env("CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN", "custom");
    bind_pty_identity(&state, &mut cmd, "screen-setting-off", None);
    apply_agent_screen_env(&mut cmd, &std::collections::HashMap::new());
    assert_eq!(
        cmd.get_env("CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN"),
        Some(std::ffi::OsStr::new("custom"))
    );
}

#[cfg(unix)]
#[test]
fn prefill_agent_input_preserves_partial_input_and_accumulates_grabs() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let bytes = insert_recording_session(&state, "agent-prefill");
    agent_session(&state, "agent-prefill", SHELL_IDLE);
    crate::mcp_http::session::write_pty_input(&state, "agent-prefill", "my note ").unwrap();
    prefill_agent_input(&state, "agent-prefill", "<first>\n").unwrap();
    prefill_agent_input(&state, "agent-prefill", "<second>\n").unwrap();
    assert_eq!(
        *bytes.lock().unwrap(),
        b"my note \x1b[200~<first>\n\x1b[201~\x1b[200~<second>\n\x1b[201~"
    );
}

#[test]
fn test_parse_signal_number_killed() {
    assert_eq!(parse_signal_number("Killed: 9"), 9);
}

#[test]
fn test_parse_signal_number_interrupt() {
    assert_eq!(parse_signal_number("Interrupt: 2"), 2);
}

#[test]
fn test_parse_signal_number_format_variant() {
    assert_eq!(parse_signal_number("Signal 15"), 15);
}

#[test]
fn test_parse_signal_number_unknown() {
    assert_eq!(parse_signal_number("unknown signal"), 0);
}

/// Catches: `not_managed_agent` for an idle terminal ego (story 1080-a62f).
#[cfg(unix)]
#[test]
fn test_submit_to_idle_terminal_ego_is_accepted() {
    let state = crate::state::tests_support::make_test_app_state();
    insert_recording_session(&state, "ego-tab");
    agent_session(&state, "ego-tab", SHELL_IDLE);
    state
        .session_maps
        .session_states
        .get_mut("ego-tab")
        .unwrap()
        .agent_type = Some("ego".into());
    assert_eq!(agent_submission_rejection(&state, "ego-tab", false), None);
}

// --- classify_shell tests (story 1274-2e38) ---

#[test]
fn classify_shell_bare_posix_basenames() {
    for s in [
        "sh", "bash", "zsh", "fish", "dash", "ksh", "ash", "tcsh", "csh", "mksh",
    ] {
        assert_eq!(classify_shell(s), ShellFamily::Posix, "{s}");
    }
}

#[test]
fn classify_shell_absolute_posix_paths() {
    for s in [
        "/bin/bash",
        "/usr/bin/zsh",
        "/opt/homebrew/bin/fish",
        "/usr/local/bin/sh",
    ] {
        assert_eq!(classify_shell(s), ShellFamily::Posix, "{s}");
    }
}

#[test]
fn classify_shell_windows_native() {
    for s in [
        "cmd",
        "cmd.exe",
        "C:\\Windows\\System32\\cmd.exe",
        "powershell",
        "powershell.exe",
        "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe",
        "pwsh",
        "pwsh.exe",
    ] {
        assert_eq!(classify_shell(s), ShellFamily::WindowsNative, "{s}");
    }
}

/// Critical regression case for story 1274-2e38: Git Bash / Cygwin / MSYS
/// ship `bash.exe` on Windows and DO support Ctrl-U. Classifying by host
/// OS would wrongly skip the prefix here; classifying by shell basename
/// correctly keeps them in the Posix family.
#[test]
fn classify_shell_git_bash_on_windows_is_posix() {
    for s in [
        "bash.exe",
        "C:\\Program Files\\Git\\bin\\bash.exe",
        "C:\\Program Files\\Git\\usr\\bin\\bash.exe",
        "C:/Program Files/Git/bin/bash.exe",
        "C:\\cygwin64\\bin\\bash.exe",
        "C:\\msys64\\usr\\bin\\bash.exe",
    ] {
        assert_eq!(classify_shell(s), ShellFamily::Posix, "{s}");
    }
}

#[test]
fn classify_shell_wsl_is_posix() {
    for s in [
        "wsl",
        "wsl.exe",
        "wsl.exe -d Ubuntu",
        "C:\\Windows\\System32\\wsl.exe",
    ] {
        assert_eq!(classify_shell(s), ShellFamily::Posix, "{s}");
    }
}

#[test]
fn classify_shell_case_insensitive() {
    assert_eq!(classify_shell("BASH.EXE"), ShellFamily::Posix);
    assert_eq!(classify_shell("Cmd.Exe"), ShellFamily::WindowsNative);
    assert_eq!(classify_shell("PowerShell.exe"), ShellFamily::WindowsNative);
}

#[test]
fn classify_shell_ignores_trailing_arguments() {
    // Arguments after the first whitespace must not affect classification.
    assert_eq!(classify_shell("bash --login"), ShellFamily::Posix);
    assert_eq!(
        classify_shell("powershell.exe -NoProfile"),
        ShellFamily::WindowsNative
    );
}

#[test]
fn classify_shell_unknown_for_other_binaries() {
    // Intentionally unknown — callers should fall back to a safe default.
    for s in ["python", "node", "/usr/bin/env", "", "   "] {
        assert_eq!(classify_shell(s), ShellFamily::Unknown, "{s:?}");
    }
}

#[test]
fn test_tool_error_different_line_fires_after_first() {
    let mut s = SilenceState::new();
    s.mark_tool_error_candidate("Error: Exit code 1".to_string());
    s.last_output_at = std::time::Instant::now()
        - SILENCE_TOOL_ERROR_THRESHOLD
        - std::time::Duration::from_millis(100);
    let _ = s.check_tool_error();
    s.clear_tool_error_on_recovery();

    // A different error appears in a later turn — must still fire.
    s.mark_tool_error_candidate("Error: Exit code 128".to_string());
    s.last_output_at = std::time::Instant::now()
        - SILENCE_TOOL_ERROR_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert_eq!(
        s.check_tool_error(),
        Some("Error: Exit code 128".to_string()),
        "distinct error text must not be suppressed by prior surface"
    );
}

#[test]
fn test_api_retry_blocks_ready_screen_confirm() {
    // Claude Code keeps its `❯` prompt visible while auto-retrying, so a stable
    // ready screen would otherwise confirm idle after AGENT_READY_CONFIRM. The
    // retry hold must refuse that confirmation.
    let mut s = SilenceState::new();
    s.mark_api_retry();
    // Force the ready prompt to look long-stable.
    s.screen_ready_pending_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM * 2);
    assert!(
        !s.note_ready_screen(),
        "ready screen must not confirm idle while an API retry is in flight"
    );

    // Once the hold expires, the same stable ready prompt confirms idle.
    s.api_retry_hold_until = Some(std::time::Instant::now() - std::time::Duration::from_millis(1));
    s.screen_ready_pending_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM * 2);
    assert!(
        s.note_ready_screen(),
        "ready screen confirms idle after the retry hold expires"
    );
}

#[test]
fn test_tool_error_mark_is_idempotent_while_pending() {
    let mut s = SilenceState::new();
    s.mark_tool_error_candidate("Error: Exit code 1".to_string());
    // Second mark for the same line while still pending → no-op.
    s.mark_tool_error_candidate("Error: Exit code 1".to_string());
    s.last_output_at = std::time::Instant::now()
        - SILENCE_TOOL_ERROR_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert_eq!(s.check_tool_error(), Some("Error: Exit code 1".to_string()));
}

// --- Suggest backend-gating tests ---

#[test]
fn test_suggest_drain_returns_parked_items() {
    let mut s = SilenceState::new();
    s.mark_suggest_candidate(vec!["alpha".to_string(), "beta".to_string()], 0);
    assert_eq!(
        s.drain_pending_suggest(),
        Some(vec!["alpha".to_string(), "beta".to_string()])
    );
}

#[test]
fn test_suggest_newer_items_overwrite_older() {
    let mut s = SilenceState::new();
    s.mark_suggest_candidate(vec!["old".to_string()], 0);
    s.mark_suggest_candidate(vec!["new1".to_string(), "new2".to_string()], 0);
    assert_eq!(
        s.drain_pending_suggest(),
        Some(vec!["new1".to_string(), "new2".to_string()]),
        "latest parked set must win (agent updated suggestions mid-turn)"
    );
}

#[test]
fn test_suggest_empty_items_ignored() {
    let mut s = SilenceState::new();
    s.mark_suggest_candidate(vec![], 0);
    assert!(
        s.pending_suggest_items.is_none(),
        "empty items must not park"
    );
    assert!(s.drain_pending_suggest().is_none());
}

#[test]
fn test_tool_error_suppressed_while_spinner_active() {
    let mut s = SilenceState::new();
    s.mark_tool_error_candidate("Error: Exit code 2".to_string());
    s.last_status_line_at = Some(std::time::Instant::now());
    s.last_output_at = std::time::Instant::now()
        - SILENCE_TOOL_ERROR_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert!(
        s.check_tool_error().is_none(),
        "spinner active means agent still working — no notification"
    );
}

#[test]
fn test_silence_state_pending_after_threshold() {
    let mut s = SilenceState::new();
    s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
    // Simulate time passing by backdating last_output_at
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert_eq!(s.check_silence(), Some("Continue?".to_string()));
}

#[test]
fn test_silence_state_regex_clears_prior_pending() {
    let mut s = SilenceState::new();
    // Silence detector has a pending question from an earlier chunk
    s.on_chunk(
        false,
        Some("Earlier question?".to_string()),
        false,
        false,
        false,
    );
    assert!(s.pending_question_line.is_some());
    // Regex fires on a different event — no question line in this chunk
    s.on_chunk(true, None, false, false, false);
    assert!(
        s.pending_question_line.is_none(),
        "prior pending should be cleared when regex fires"
    );
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert!(s.check_silence().is_none());
}

#[test]
fn test_silence_state_non_question_output_preserves_pending() {
    let mut s = SilenceState::new();
    s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
    // Non-`?` output (spinners, prompts, decorations) must NOT clear pending.
    s.on_chunk(false, None, false, false, false);
    s.on_chunk(false, None, false, false, false);
    s.on_chunk(false, None, false, false, false);
    // Standard 10s threshold fires normally
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert_eq!(s.check_silence(), Some("Continue?".to_string()));
}

#[test]
fn test_silence_state_new_question_replaces_old() {
    let mut s = SilenceState::new();
    s.on_chunk(
        false,
        Some("First question?".to_string()),
        false,
        false,
        false,
    );
    s.on_chunk(
        false,
        Some("Second question?".to_string()),
        false,
        false,
        false,
    );
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert_eq!(s.check_silence(), Some("Second question?".to_string()));
}

#[test]
fn test_silence_state_suppress_echo_expires() {
    let mut s = SilenceState::new();
    s.suppress_user_input();
    // Expire the echo suppress window with a past deadline (not None,
    // which means "never suppressed" — a different code path).
    s.suppress_echo_until = Some(std::time::Instant::now() - std::time::Duration::from_millis(1));
    // Agent asks a genuine question after the window expires
    s.on_chunk(
        false,
        Some("Would you like to proceed?".to_string()),
        false,
        false,
        false,
    );
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    // Should fire — this is a real agent question
    assert_eq!(
        s.check_silence(),
        Some("Would you like to proceed?".to_string())
    );
}

#[test]
fn test_silence_state_spinner_suppresses_question() {
    let mut s = SilenceState::new();
    // Agent prints a `?`-line alongside a status-line/spinner in the same chunk
    s.on_chunk(
        false,
        Some("Want me to proceed?".to_string()),
        true,
        false,
        false,
    );
    // Simulate 10s+ of silence
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    // Should NOT emit question — spinner was recently active
    assert_eq!(s.check_silence(), None, "spinner active → no question");
}

#[test]
fn test_silence_state_spinner_expired_allows_question() {
    let mut s = SilenceState::new();
    s.on_chunk(
        false,
        Some("Want me to proceed?".to_string()),
        true,
        false,
        false,
    );
    // Spinner was active but long ago (>10s, matching SILENCE_QUESTION_THRESHOLD)
    s.last_status_line_at = Some(std::time::Instant::now() - std::time::Duration::from_secs(12));
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    // Spinner expired, question should fire
    assert_eq!(s.check_silence(), Some("Want me to proceed?".to_string()));
}

#[test]
fn test_silence_state_spinner_within_10s_suppresses() {
    let mut s = SilenceState::new();
    s.on_chunk(
        false,
        Some("Want me to proceed?".to_string()),
        true,
        false,
        false,
    );
    // Spinner was 8s ago — still within the 10s window
    s.last_status_line_at = Some(std::time::Instant::now() - std::time::Duration::from_secs(8));
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert_eq!(
        s.check_silence(),
        None,
        "spinner within 10s should suppress question"
    );
}

// --- Status-line-only chunk tests ---

#[test]
fn test_silence_state_status_line_only_does_not_reset_silence() {
    let mut s = SilenceState::new();
    s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
    // Backdate last_output_at to simulate 10s of silence
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    // Mode-line timer tick: status_line_only = true, should NOT reset last_output_at
    s.on_chunk(false, None, true, true, false);
    // The silence threshold should still be met
    assert_eq!(
        s.check_silence(),
        Some("Continue?".to_string()),
        "status_line_only chunks must not reset the silence timer"
    );
}

#[test]
fn test_silence_state_mode_line_ticks_do_not_suppress_question() {
    // Reproduces the bug: Claude Code asks a question, then the mode line
    // keeps updating every ~1s while waiting for input. Status-line-only chunks
    // were keeping `is_spinner_active()` true forever, preventing question
    // detection even after 10s of silence.
    let mut s = SilenceState::new();
    // Agent outputs question + status line in same chunk (not status-line-only)
    s.on_chunk(
        false,
        Some("Vuoi fare un commit?".to_string()),
        true,
        false,
        false,
    );

    // Simulate 10s+ passing: both last_output_at and last_status_line_at
    // age beyond the threshold (in real life, wall-clock time handles this).
    let past = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    s.last_output_at = past;
    s.last_status_line_at = Some(past);

    // Mode-line-only ticks keep coming — they must NOT refresh either timer.
    for _ in 0..10 {
        s.on_chunk(false, None, true, true, false);
    }

    // After 10s+ of silence, the question MUST be detected even though
    // mode-line ticks kept coming in.
    assert_eq!(
        s.check_silence(),
        Some("Vuoi fare un commit?".to_string()),
        "mode-line-only ticks must not keep is_spinner_active() alive"
    );
}

#[test]
fn test_silence_state_mode_line_ticks_do_not_stale_question() {
    // Regression: mode-line timer ticks (status_line_only=true) were incrementing
    // output_chunks_after_question, clearing the pending question as "stale"
    // before the silence timer could detect it.
    let mut s = SilenceState::new();
    s.on_chunk(false, Some("Procedo?".to_string()), true, false, false);

    // Simulate 15 mode-line ticks (> STALE_QUESTION_CHUNKS=10)
    for _ in 0..15 {
        s.on_chunk(false, None, true, true, false);
    }

    // pending_question_line must still be present — mode-line ticks are not real output
    assert_eq!(
        s.pending_question_line.as_deref(),
        Some("Procedo?"),
        "mode-line-only ticks must not count toward staleness"
    );

    // Backdate to simulate silence threshold reached
    let past = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    s.last_output_at = past;
    s.last_status_line_at = Some(past);

    assert_eq!(
        s.check_silence(),
        Some("Procedo?".to_string()),
        "question must be detectable after mode-line-only ticks"
    );
}

#[test]
fn test_silence_state_regular_chunk_resets_silence() {
    let mut s = SilenceState::new();
    s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
    // Backdate to simulate 10s silence
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    // Regular (non-status-line) chunk resets the timer
    s.on_chunk(false, None, false, false, false);
    // Now we need to wait another 10s — should NOT fire yet
    assert_eq!(
        s.check_silence(),
        None,
        "regular chunk should reset silence timer"
    );
}

#[test]
fn test_silence_state_suggest_only_does_not_stale_question() {
    // A suggest-only chunk (protocol token, not real output) must not
    // increment output_chunks_after_question or reset the silence timer.
    let mut s = SilenceState::new();
    s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
    // 15 suggest-only chunks — should NOT stale the pending question
    for _ in 0..15 {
        s.on_chunk(false, None, false, false, true);
    }
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert_eq!(
        s.check_silence(),
        Some("Continue?".to_string()),
        "suggest-only chunks must not count toward question staleness"
    );
}

/// The rank decision behind every `*_recovers_long_lived_shell_busy` test
/// (#745-8ff1), pinned directly so the next person to change it fails a test
/// rather than a review.
///
/// OSC 133 is shell integration, not agent instrumentation: `133;C` fires when
/// a foreground command starts and `133;D` when it exits, so on a long-lived
/// TUI agent it is set once at launch and cleared only at death. It is
/// process-granularity evidence that knows nothing about turns, so it records
/// at Screen rank and a stable Ready screen is allowed to close it. An agent
/// hook (OSC 7770) does know about turns, records at Protocol rank, and holds.
///
/// Rank is about what the evidence knows, not how it travelled — arriving in an
/// escape sequence does not make something Protocol rank.
#[test]
fn osc133_busy_is_screen_rank_and_yields_to_a_ready_screen() {
    let aged = || Some(std::time::Instant::now() - AGENT_READY_CONFIRM);

    // Shell integration: Screen rank, and the Ready screen closes the turn.
    let mut shell = SilenceState::new();
    shell.note_explicit_state(SHELL_BUSY, false);
    assert_eq!(
        shell.evidence.busy.map(|b| (b.rank, b.source)),
        Some((EvidenceRank::Screen, "osc133-busy")),
        "OSC 133 knows a process started, never that a turn started"
    );
    shell.note_real_activity();
    shell.screen_ready_pending_since = aged();
    assert!(
        shell.note_ready_screen(),
        "a stable Ready screen must close an osc133-held turn (#535-d4f5)"
    );
    assert!(shell.idle_confirmed());

    // Agent hook: Protocol rank, and the identical Ready screen does NOT close it.
    let mut hooked = SilenceState::new();
    hooked.note_explicit_state(SHELL_BUSY, true);
    assert_eq!(
        hooked.evidence.busy.map(|b| (b.rank, b.source)),
        Some((EvidenceRank::Protocol, "hook-busy")),
        "an agent hook is turn-granular and outranks the screen"
    );
    hooked.note_real_activity();
    hooked.screen_ready_pending_since = aged();
    assert!(
        !hooked.note_ready_screen(),
        "a Ready screen must never close a turn a protocol signal holds"
    );
    assert!(!hooked.idle_confirmed());

    // `explicit_busy()` spans both ranks on purpose: it reports provenance
    // (an explicit marker set this), NOT authority. Anything deciding whether
    // evidence may hold a turn must read the rank instead.
    let mut provenance = SilenceState::new();
    provenance.note_explicit_state(SHELL_BUSY, false);
    assert!(provenance.explicit_busy());
    assert_eq!(
        provenance.evidence.busy.map(|b| b.rank),
        Some(EvidenceRank::Screen),
        "explicit_busy() is true here at Screen rank — it is not a rank test"
    );
}

#[test]
fn test_agent_ready_requires_stable_observation() {
    let mut silence = SilenceState::new();
    assert!(!silence.note_ready_screen());
    assert!(!silence.idle_confirmed());
    silence.screen_ready_pending_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    assert!(silence.note_ready_screen());
    assert!(silence.idle_confirmed());
}

#[test]
fn test_grok_ready_composer_recovers_long_lived_shell_busy() {
    let rows = vec![
        "Finished the response.".to_string(),
        "❯ Ask anything".to_string(),
        "⌘ Grok 4.3 OpenRouter · Medium effort".to_string(),
    ];
    assert_eq!(
        detect_agent_screen_activity(Some("grok"), &rows),
        AgentScreenActivity::Ready
    );

    let mut silence = SilenceState::new();
    // OSC 133 marks the long-lived `grok` shell command busy. Without a
    // Grok ready-screen adapter this bit survived for the whole process.
    //
    // These three assertions were inverted by e17c79b8 and restored by
    // #745-8ff1. They are byte-identical in setup to the goose and opencode
    // cases below/above, and a `SilenceState` carries no agent type, so all
    // three MUST agree — see `osc133_busy_is_screen_rank_and_yields_to_a_ready_screen`
    // for the rank decision they rest on. If you are here because one of the
    // three is red, the answer is never to make this trio disagree again.
    silence.note_explicit_state(SHELL_BUSY, false);
    silence.note_real_activity();
    silence.screen_ready_pending_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    assert!(silence.note_ready_screen());
    assert!(!silence.explicit_busy());
    assert!(silence.idle_confirmed());
}

#[test]
fn test_opencode_ready_screen_recovers_long_lived_shell_busy() {
    assert_eq!(classify_agent("opencode"), Some("opencode"));
    assert!(has_ready_screen_adapter(Some("opencode")));

    let mut silence = SilenceState::new();
    // OSC 133 marks the long-lived `opencode` foreground command busy. Without a
    // ready-screen adapter this bit survived for the whole process (#535-d4f5).
    silence.note_explicit_state(SHELL_BUSY, false);
    silence.note_real_activity();
    silence.screen_ready_pending_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    assert!(silence.note_ready_screen());
    assert!(!silence.explicit_busy());
    assert!(silence.idle_confirmed());
}

#[test]
fn protocol_submission_requires_post_submit_consumption_before_ready_screen() {
    let mut silence = SilenceState::new();
    silence.note_user_submission(true);
    silence.screen_ready_pending_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    assert!(!silence.note_ready_screen());
    assert!(silence.explicit_busy());
    assert!(!silence.idle_confirmed());

    silence.note_real_activity();
    silence.screen_ready_pending_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    assert!(silence.note_ready_screen());
    assert!(!silence.explicit_busy());
    assert!(silence.idle_confirmed());
}

#[test]
fn stable_ready_prompt_does_not_recover_a_missed_hook_idle() {
    let mut silence = SilenceState::new();
    silence.note_user_submission(true);
    silence.note_explicit_state(SHELL_BUSY, true);
    silence.note_real_activity();
    silence.screen_ready_pending_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    assert!(!silence.note_ready_screen());
    assert!(silence.explicit_busy());
    assert!(silence.hook_busy());
    assert!(!silence.idle_confirmed());
}

#[test]
fn test_hook_busy_cannot_be_overridden_before_turn_activity() {
    let mut silence = SilenceState::new();
    silence.note_user_submission(true);
    silence.note_explicit_state(SHELL_BUSY, true);
    silence.screen_ready_pending_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    assert!(!silence.note_ready_screen());
    assert!(silence.explicit_busy());
    assert!(silence.hook_busy());
    assert!(!silence.idle_confirmed());
}

#[test]
fn test_fresh_hook_busy_blocks_ready_after_prior_recovery() {
    let mut silence = SilenceState::new();
    silence.note_user_submission(true);
    silence.note_explicit_state(SHELL_BUSY, true);
    silence.note_real_activity();
    silence.screen_ready_pending_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    assert!(!silence.note_ready_screen());

    silence.note_explicit_state(SHELL_BUSY, true);
    silence.screen_ready_pending_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    assert!(!silence.note_ready_screen());
    assert!(silence.explicit_busy());
    assert!(silence.hook_busy());
    assert!(!silence.idle_confirmed());
}

#[test]
fn screen_only_submission_keeps_ready_adapter_fallback() {
    let mut silence = SilenceState::new();
    silence.note_user_submission(false);
    silence.note_real_activity();
    silence.screen_ready_pending_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    assert!(silence.note_ready_screen());
    assert!(silence.idle_confirmed());
}

#[test]
fn protocol_busy_requires_both_old_signal_and_old_output_to_be_stale() {
    let mut silence = SilenceState::new();
    silence.note_explicit_state(SHELL_BUSY, true);
    silence.evidence.busy.as_mut().unwrap().at = std::time::Instant::now() - PROTOCOL_STALE_TIMEOUT;
    silence.screen_ready_pending_since = Some(std::time::Instant::now() - PROTOCOL_STALE_TIMEOUT);
    assert!(
        !silence.protocol_busy_is_stale(),
        "fresh output keeps the hold"
    );
    silence.last_output_at = std::time::Instant::now() - PROTOCOL_STALE_TIMEOUT;
    assert!(silence.protocol_busy_is_stale());
}

#[test]
fn protocol_stale_recovery_requires_a_ready_screen_for_the_full_timeout() {
    let mut silence = SilenceState::new();
    silence.note_explicit_state(SHELL_BUSY, true);
    silence.evidence.busy.as_mut().unwrap().at = std::time::Instant::now() - PROTOCOL_STALE_TIMEOUT;
    silence.last_output_at = std::time::Instant::now() - PROTOCOL_STALE_TIMEOUT;
    assert!(!silence.protocol_busy_is_stale(), "no Ready observation");
    assert!(
        !silence.note_ready_screen(),
        "the first Ready starts the clock"
    );
    assert!(!silence.protocol_busy_is_stale(), "fresh Ready observation");
    silence.screen_ready_pending_since = Some(std::time::Instant::now() - PROTOCOL_STALE_TIMEOUT);
    assert!(silence.protocol_busy_is_stale());
}

#[test]
fn protocol_busy_parks_suggest_without_downgrading_working_screen() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "protocol-suggest";
    agent_session(&state, session_id, SHELL_BUSY);
    let lifecycle = state
        .session_maps
        .silence_states
        .get(session_id)
        .unwrap()
        .clone();
    {
        let mut lifecycle = lifecycle.lock();
        lifecycle.note_explicit_state(SHELL_BUSY, true);
        lifecycle.mark_suggest_candidate(vec!["Review diff".into()], 0);
    }
    assert_eq!(
        completion_adjusted_screen_activity(
            &state,
            &lifecycle,
            session_id,
            AgentScreenActivity::Working,
        ),
        AgentScreenActivity::Working
    );
    assert_eq!(
        lifecycle.lock().drain_pending_suggest(),
        Some(vec!["Review diff".into()])
    );
}

/// #745-8ff1 AC1, at the level the rule actually has to hold: the silence
/// timer.
///
/// The unit tests around `note_ready_screen` prove a Ready *screen* cannot
/// close a protocol-held turn, but the timer is a second, independent way in —
/// it reaches `IdleDecision` through `try_timer_idle_transition` and can idle a
/// session with no screen evidence at all. The guard is
/// `silence.explicit_busy() && !nested_prompt` (pty.rs:4305); nothing asserted
/// it, so removing it would have left every `note_ready_screen` test green
/// while a hook-held turn quietly went idle on the timer.
///
/// Both ways in are checked: a Ready screen the protocol hold refuses to
/// confirm, and no screen classification at all, which is the pure silence
/// path.
#[test]
fn the_silence_timer_cannot_idle_a_protocol_held_turn() {
    for screen in [AgentScreenActivity::Ready, AgentScreenActivity::Unknown] {
        let state = crate::state::tests_support::make_test_app_state();
        let session_id = "protocol-held-timer";
        agent_session(&state, session_id, SHELL_BUSY);
        state
            .session_maps
            .session_states
            .get_mut(session_id)
            .unwrap()
            .agent_type = Some("codex".into());
        let silence = state
            .session_maps
            .silence_states
            .get(session_id)
            .unwrap()
            .clone();
        {
            let mut silence = silence.lock();
            silence.note_explicit_state(SHELL_BUSY, true);
            assert!(silence.hook_busy(), "precondition: Protocol-rank busy");
            // Aged well past the confirm window: the turn is held by rank, not
            // by the screen being too fresh to trust.
            silence.screen_ready_pending_since =
                Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
        }

        let transition =
            try_timer_idle_transition(&state, &silence, session_id, screen, Some("codex"), Some(0));

        assert!(
            !transition.transitioned,
            "{screen:?}: the timer must not close a turn a protocol signal holds"
        );
        assert_eq!(
            state
                .session_maps
                .shell_states
                .get(session_id)
                .unwrap()
                .load(std::sync::atomic::Ordering::Acquire),
            SHELL_BUSY,
            "{screen:?}: session must still read BUSY"
        );
        assert!(
            silence.lock().hook_busy(),
            "{screen:?}: the protocol evidence must survive the timer tick"
        );
    }
}

#[test]
fn test_explicit_busy_suppresses_silence_question_until_idle() {
    let mut silence = SilenceState::new();
    silence.note_explicit_state(SHELL_BUSY, true);
    silence.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD;
    silence.pending_question_line = Some("Continue?".into());
    assert!(!silence.is_silent());
    silence.note_explicit_state(SHELL_IDLE, true);
    assert!(silence.is_silent());
}

fn replay_sanitized_agent_trace(agent: &str, steps: &[SanitizedTraceStep]) -> SilenceState {
    let mut silence = SilenceState::new();
    silence.confirm_idle();
    for step in steps {
        match step {
            SanitizedTraceStep::Submit => silence.note_user_submission(true),
            SanitizedTraceStep::RealActivity => silence.note_real_activity(),
            SanitizedTraceStep::WorkingScreen => {
                let rows = if agent == "codex" {
                    vec!["• Working".to_string(), "› sanitized prompt".to_string()]
                } else {
                    vec!["sanitized animated output".to_string()]
                };
                if detect_agent_screen_activity(Some(agent), &rows) == AgentScreenActivity::Working
                {
                    silence.note_working_screen();
                }
            }
            SanitizedTraceStep::ReadyScreen => {
                let rows = match agent {
                    "codex" => vec!["sanitized final".into(), "› sanitized prompt".into()],
                    "claude" => vec!["sanitized final".into(), "❯".into()],
                    "gemini" => vec!["> Type your message".into()],
                    "aider" => vec![">".into()],
                    _ => Vec::new(),
                };
                if detect_agent_screen_activity(Some(agent), &rows) == AgentScreenActivity::Ready {
                    silence.screen_ready_pending_since =
                        Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
                    silence.note_ready_screen();
                }
            }
            SanitizedTraceStep::UnknownScreen => silence.note_unknown_screen(),
        }
    }
    silence
}

/// Build a codex agent session held BUSY by a Protocol-rank submitted line
/// that has since produced output, with a ready screen already stable for
/// `AGENT_READY_CONFIRM`. That is the exact state in which the foreground
/// probe — and nothing else — decides whether the turn ends (#771-4733).
fn probe_evidence_fixture(
    state: &crate::state::AppState,
    session_id: &str,
) -> std::sync::Arc<parking_lot::Mutex<SilenceState>> {
    agent_session(state, session_id, SHELL_BUSY);
    state
        .session_maps
        .session_states
        .get_mut(session_id)
        .unwrap()
        .agent_type = Some("codex".into());
    let silence = state
        .session_maps
        .silence_states
        .get(session_id)
        .unwrap()
        .clone();
    {
        let mut sl = silence.lock();
        sl.note_user_submission(true);
        sl.note_real_activity();
        sl.screen_ready_pending_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    }
    silence
}

#[test]
fn explicit_agent_idle_waits_for_newer_snapshot_and_repairs_working() {
    for (session_id, hook_state) in [
        ("background-hook-idle", true),
        ("background-osc133-idle", false),
    ] {
        let state = crate::state::tests_support::make_test_app_state();
        let parent_id = format!("{session_id}-parent");
        agent_session(&state, session_id, SHELL_BUSY);
        state
            .session_maps
            .session_states
            .get_mut(session_id)
            .unwrap()
            .agent_type = Some("codex".into());
        state
            .session_maps
            .session_parent
            .insert(session_id.to_string(), parent_id.clone());
        state.agent_inbox.entry(parent_id.clone()).or_default();
        state
            .process_snapshot_cache
            .store(Some(vec![process(10, 1, "codex", "codex")]));

        transition_explicit_shell_state_with_hook(
            &state,
            session_id,
            SHELL_IDLE,
            "idle",
            hook_state,
            || {},
        );

        assert_eq!(
            state
                .session_maps
                .shell_states
                .get(session_id)
                .unwrap()
                .load(Ordering::Acquire),
            SHELL_IDLE
        );
        assert!(state.agent_inbox.get(&parent_id).unwrap().is_empty());
        assert!(!refresh_background_work_from_cached_snapshot(
            &state,
            session_id,
            10,
            "codex",
            0,
            state.process_snapshot_cache.load(),
        ));
        assert!(state.agent_inbox.get(&parent_id).unwrap().is_empty());

        state.process_snapshot_cache.store(Some(vec![
            process(10, 1, "codex", "codex"),
            process(11, 10, "cargo", "cargo test --locked"),
        ]));
        assert!(refresh_background_work_from_cached_snapshot(
            &state,
            session_id,
            10,
            "codex",
            0,
            state.process_snapshot_cache.load(),
        ));
        let snapshot = state.session_state_with_shell(session_id).unwrap();
        assert_eq!(snapshot.shell_state.as_deref(), Some("idle"));
        assert_eq!(snapshot.agent_state.as_deref(), Some("working"));
        assert!(snapshot.background_work);
        assert!(state.agent_inbox.get(&parent_id).unwrap().is_empty());
    }
}

#[test]
fn explicit_non_agent_idle_keeps_immediate_shell_semantics() {
    for (session_id, hook_state) in [("plain-hook-idle", true), ("plain-osc133-idle", false)] {
        let state = crate::state::tests_support::make_test_app_state();
        state.session_maps.session_states.insert(
            session_id.to_string(),
            crate::state::SessionState::default(),
        );
        state.session_maps.shell_states.insert(
            session_id.to_string(),
            std::sync::atomic::AtomicU8::new(SHELL_BUSY),
        );
        state.session_maps.silence_states.insert(
            session_id.to_string(),
            Arc::new(Mutex::new(SilenceState::new())),
        );

        transition_explicit_shell_state_with_hook(
            &state,
            session_id,
            SHELL_IDLE,
            "idle",
            hook_state,
            || {},
        );

        assert_eq!(
            state
                .session_maps
                .shell_states
                .get(session_id)
                .unwrap()
                .load(Ordering::Acquire),
            SHELL_IDLE
        );
        let session = state.session_maps.session_states.get(session_id).unwrap();
        assert_eq!(session.background_probe_turn_epoch, None);
        assert_eq!(session.background_probe_after_generation, None);
    }
}

// --- Staleness counter tests ---

#[test]
fn test_silence_state_stale_after_many_output_chunks() {
    let mut s = SilenceState::new();
    s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
    // Simulate 15 non-`?` chunks (well beyond STALE_QUESTION_CHUNKS)
    for _ in 0..15 {
        s.on_chunk(false, None, false, false, false);
    }
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert_eq!(
        s.check_silence(),
        None,
        "stale question after many chunks should not fire"
    );
}

#[test]
fn test_silence_state_few_decoration_chunks_still_fires() {
    let mut s = SilenceState::new();
    s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
    // 3 decoration chunks (mode line, separator, prompt) — within threshold
    s.on_chunk(false, None, false, false, false);
    s.on_chunk(false, None, false, false, false);
    s.on_chunk(false, None, false, false, false);
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert_eq!(
        s.check_silence(),
        Some("Continue?".to_string()),
        "few decoration chunks should still fire"
    );
}

#[test]
fn test_silence_state_counter_resets_on_new_question() {
    let mut s = SilenceState::new();
    s.on_chunk(false, Some("First?".to_string()), false, false, false);
    // Many non-`?` chunks → stale
    for _ in 0..15 {
        s.on_chunk(false, None, false, false, false);
    }
    // New `?` line resets the counter
    s.on_chunk(false, Some("Second?".to_string()), false, false, false);
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert_eq!(
        s.check_silence(),
        Some("Second?".to_string()),
        "new question should reset staleness"
    );
}

// --- Clear stale question tests ---

#[test]
fn test_silence_state_clear_stale_resets_pending() {
    let mut s = SilenceState::new();
    s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
    s.clear_stale_question();
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert_eq!(s.check_silence(), None, "cleared stale should not fire");
}

#[test]
fn test_silence_state_clear_stale_allows_new_question() {
    let mut s = SilenceState::new();
    s.on_chunk(false, Some("Old?".to_string()), false, false, false);
    s.clear_stale_question();
    // New question after clear
    s.on_chunk(false, Some("New?".to_string()), false, false, false);
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert_eq!(
        s.check_silence(),
        Some("New?".to_string()),
        "new question after clear should fire"
    );
}

#[test]
fn test_silence_state_repaint_same_question_does_not_refire() {
    let mut s = SilenceState::new();
    // Question arrives, silence fires, mark emitted
    s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert!(s.check_silence().is_some());
    assert!(s.question_already_emitted);

    // Terminal repaint: same `?` line re-appears as a changed row.
    // This must NOT reset question_already_emitted.
    s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
    assert!(
        s.question_already_emitted,
        "repaint of same question must not reset emitted flag"
    );
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert!(
        s.check_silence().is_none(),
        "same question repaint must not re-fire"
    );
}

#[test]
fn test_silence_state_stale_same_question_scroll_does_not_refire() {
    let mut s = SilenceState::new();
    let past = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    // Question fires via chunk-based detection (Strategy 2)
    s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
    s.last_output_at = past;
    assert!(s.check_silence().is_some());

    // Agent resumes: 15 non-`?` chunks (above STALE_QUESTION_CHUNKS)
    for _ in 0..15 {
        s.on_chunk(false, None, false, false, false);
    }
    assert!(
        s.pending_question_line.is_none(),
        "pending should be cleared by staleness"
    );

    // Same "Continue?" reappears in changed_rows because new output scrolled it
    // to a different row. This is NOT a new question — must not re-fire.
    s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
    assert!(
        s.question_already_emitted,
        "scroll of previously emitted question must not reset emitted flag"
    );
    s.last_output_at = past;
    assert!(
        s.check_silence().is_none(),
        "same question text from scroll must not re-fire"
    );
}

#[test]
fn test_silence_state_stale_same_question_does_not_refire_after_user_input() {
    let mut s = SilenceState::new();
    let past = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    // Question fires
    s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
    s.last_output_at = past;
    assert!(s.check_silence().is_some());

    // Agent resumes: 15 non-`?` chunks
    for _ in 0..15 {
        s.on_chunk(false, None, false, false, false);
    }

    // User provides input → new conversation cycle
    s.suppress_user_input();
    // Expire the echo suppression window so the next `?` line is not ignored
    s.suppress_echo_until = Some(std::time::Instant::now() - std::time::Duration::from_millis(1));

    // The same historical row is moved by a repaint after the answer. Text
    // alone cannot prove that the agent asked it again, so it must remain
    // suppressed; current-turn screen position/protocol evidence owns a
    // genuinely repeated prompt.
    s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
    s.last_output_at = past;
    assert!(
        s.check_silence().is_none(),
        "historical question repaint after user input must not re-arm awaiting"
    );
}

#[test]
fn test_silence_state_screen_emitted_question_scroll_does_not_refire() {
    let mut s = SilenceState::new();
    let past = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);

    // Question arrives in a chunk
    s.on_chunk(false, Some("Continue?".to_string()), false, false, false);

    // 15 non-? chunks → pending cleared by staleness
    for _ in 0..15 {
        s.on_chunk(false, None, false, false, false);
    }
    assert!(s.pending_question_line.is_none());

    // Silence timer (Strategy 1) finds "Continue?" on screen and emits.
    s.last_output_at = past;
    s.mark_emitted("Continue?");

    // New output causes scroll → same "Continue?" appears in changed_rows
    s.on_chunk(false, Some("Continue?".to_string()), false, false, false);

    // Must NOT reset question_already_emitted — it's a scroll artifact
    assert!(
        s.question_already_emitted,
        "scroll of screen-emitted question must not reset emitted flag"
    );
    s.last_output_at = past;
    assert!(
        !s.is_silent(),
        "same question after screen emission must not allow re-detection"
    );
}

#[test]
fn test_find_last_chat_question_rejects_user_prompt_line() {
    let rows: Vec<String> = vec![
        "❯ hai cambiato qualcosa?".into(),
        "────────────────────────────────────────────────".into(),
        "❯".into(),
        "────────────────────────────────────────────────".into(),
    ];
    assert_eq!(find_last_chat_question(&rows), None);
}

#[test]
fn test_find_last_chat_question_agent_question_after_user_input() {
    let rows: Vec<String> = vec![
        "❯ tell me about this".into(),
        "Would you like me to continue?".into(),
        "────────────────────────────────────────────────".into(),
        "❯".into(),
        "────────────────────────────────────────────────".into(),
    ];
    assert_eq!(
        find_last_chat_question(&rows),
        Some("Would you like me to continue?".to_string())
    );
}

#[test]
fn test_resize_grace_expires_after_threshold() {
    let mut s = SilenceState::new();
    s.on_resize();
    // Backdating the resize timestamp past the grace period
    s.last_resize_at =
        Some(std::time::Instant::now() - RESIZE_GRACE - std::time::Duration::from_millis(100));
    assert!(!s.is_resize_grace(), "grace period should have expired");
}

#[test]
fn test_resize_grace_refreshed_on_second_resize() {
    let mut s = SilenceState::new();
    s.on_resize();
    // Expire the first grace period
    s.last_resize_at =
        Some(std::time::Instant::now() - RESIZE_GRACE - std::time::Duration::from_millis(100));
    assert!(!s.is_resize_grace());
    // Second resize refreshes the timer
    s.on_resize();
    assert!(
        s.is_resize_grace(),
        "second resize should restart grace period"
    );
}

#[test]
fn test_startup_grace_safety_cap() {
    let mut s = SilenceState::new();
    // Created long ago, but output is recent — safety cap should force settle
    s.created_at =
        std::time::Instant::now() - STARTUP_GRACE_MAX - std::time::Duration::from_secs(1);
    s.last_output_at = std::time::Instant::now(); // output still flowing
    s.check_startup_settle();
    assert!(
        !s.is_startup_grace(),
        "startup grace should end at safety cap"
    );
}

#[test]
fn test_startup_grace_idempotent_after_settle() {
    let mut s = SilenceState::new();
    s.last_output_at =
        std::time::Instant::now() - STARTUP_SETTLE_SILENCE - std::time::Duration::from_millis(100);
    s.check_startup_settle();
    assert!(s.startup_settled);
    // Calling again doesn't change anything
    s.check_startup_settle();
    assert!(s.startup_settled);
}

/// End-to-end: VtLogBuffer → extract_question_line → SilenceState → check_silence.
/// Question + mode line arrive together → fires at 10s.
#[test]
fn test_e2e_question_detection_with_mode_line() {
    use crate::state::VtLogBuffer;

    let mut vt_log = VtLogBuffer::new(24, 80, 1000);
    let mut silence = SilenceState::new();

    let changed = vt_log.process(b"Le committo?\r\n\r\n\xe2\x8f\xb5\xe2\x8f\xb5 Reading files");
    silence.on_chunk(false, extract_question_line(&changed), false, false, false);

    assert_eq!(
        silence.pending_question_line.as_deref(),
        Some("Le committo?")
    );

    silence.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert_eq!(silence.check_silence(), Some("Le committo?".to_string()));
}

/// End-to-end: question in chunk 1, mode line in chunk 2, then silence.
/// Non-`?` output must NOT prevent the question from firing at 10s.
#[test]
fn test_e2e_question_then_decoration_then_silence() {
    use crate::state::VtLogBuffer;

    let mut vt_log = VtLogBuffer::new(24, 80, 1000);
    let mut silence = SilenceState::new();

    let changed = vt_log.process(b"Le committo?");
    silence.on_chunk(false, extract_question_line(&changed), false, false, false);

    // Mode line / prompt decoration arrives in a separate chunk
    let changed = vt_log.process(b"\r\n\xe2\x8f\xb5\xe2\x8f\xb5 Idle");
    silence.on_chunk(false, extract_question_line(&changed), false, false, false);

    // 10s silence → fires
    silence.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert_eq!(silence.check_silence(), Some("Le committo?".to_string()));
}

#[test]
fn test_backup_idle_blocked_by_chrome_only_ticks() {
    // Chrome-only ticks (status-line) MUST block the backup idle timer
    // because they prove the reader thread is active and the agent is alive.
    // Regression: f5c07388 changed has_recent_chunks() to use last_output_at,
    // which let the backup timer fire during tool calls (>3s of no real output
    // while status-line ticks every ~1s), causing false busy→idle oscillation.
    let mut silence = SilenceState::new();
    // Backdate real output to 5s ago (simulates a tool call in progress)
    silence.last_output_at = std::time::Instant::now() - std::time::Duration::from_secs(5);
    // Chrome-only tick just arrived (status-line timer tick)
    silence.on_chunk(false, None, true, true, false);
    assert!(
        silence.has_recent_chunks(),
        "backup idle MUST be blocked when chrome-only ticks are arriving — agent is alive"
    );
}

/// Kill and reap a probe child without blocking the test: `wait()` on a live
/// PTY child does not return while the pair is still open in this process.
fn reap(mut child: Box<dyn portable_pty::Child + Send + Sync>) {
    let _ = child.kill();
    for _ in 0..100 {
        if matches!(child.try_wait(), Ok(Some(_))) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

fn probe_size() -> PtySize {
    PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    }
}

#[test]
fn transient_allocation_recovers_and_uses_bounded_backoff() {
    let mut attempts = 0;
    let mut sleeps = Vec::new();
    let result = retry_transient(
        || {
            attempts += 1;
            if attempts < 3 {
                Err("busy")
            } else {
                Ok("pair")
            }
        },
        |_| true,
        |attempt| sleeps.push(attempt),
    );

    assert_eq!(result, Ok("pair"));
    assert_eq!(attempts, 3);
    assert_eq!(sleeps, vec![1, 2]);
}

#[test]
fn transient_allocation_stops_after_the_attempt_limit() {
    let mut attempts = 0;
    let result = retry_transient(
        || {
            attempts += 1;
            Err::<(), _>("busy")
        },
        |_| true,
        |_| {},
    );

    assert_eq!(result, Err((PTY_SPAWN_ATTEMPTS, "busy")));
    assert_eq!(attempts, PTY_SPAWN_ATTEMPTS);
}

#[cfg(unix)]
#[test]
fn pty_allocation_classifier_retries_resource_pressure_not_permissions() {
    let exhausted = anyhow::Error::new(std::io::Error::from_raw_os_error(libc::EMFILE));
    let denied = anyhow::Error::new(std::io::Error::from_raw_os_error(libc::EACCES));

    assert!(is_transient_pty_open_error(&exhausted));
    assert!(!is_transient_pty_open_error(&denied));
}

/// A spawn that works first time must be built exactly once.
#[test]
fn a_working_spawn_is_built_once() {
    let attempts = std::cell::Cell::new(0);
    let (_pair, child) = spawn_pty_pair_with_retry(probe_size(), || {
        attempts.set(attempts.get() + 1);
        trivial_command()
    })
    .expect("echo must spawn");

    assert_eq!(attempts.get(), 1);
    reap(child);
}

#[tokio::test(flavor = "current_thread")]
async fn async_spawn_wrapper_does_not_block_the_runtime_worker() {
    let started = std::time::Instant::now();
    let spawn = tokio::spawn(run_pty_spawn_blocking(|| {
        std::thread::sleep(std::time::Duration::from_millis(100));
        Ok::<_, String>(())
    }));

    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    assert!(
        started.elapsed() < std::time::Duration::from_millis(80),
        "blocking spawn work occupied the async runtime"
    );
    spawn.await.unwrap().unwrap();
}

#[test]
fn pty_parent_env_sanitizer_removes_no_color_and_allows_override() {
    let mut cmd = CommandBuilder::new("/bin/sh");
    // Simulate CommandBuilder's inherited parent snapshot without mutating
    // the process-global environment used by other tests.
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("NO_COLOR", "1");

    sanitize_pty_parent_env(&mut cmd);

    assert_eq!(
        cmd.get_env("TERM"),
        Some(std::ffi::OsStr::new("xterm-256color"))
    );
    assert_eq!(
        cmd.get_env("COLORTERM"),
        Some(std::ffi::OsStr::new("truecolor"))
    );
    assert_eq!(cmd.get_env("NO_COLOR"), None);

    cmd.env("NO_COLOR", "intentional");
    assert_eq!(
        cmd.get_env("NO_COLOR"),
        Some(std::ffi::OsStr::new("intentional"))
    );
}

#[test]
fn pty_spawn_env_child_checks_inherited_build_context() {
    if std::env::var_os("TUIC_TEST_PTY_BUILD_ENV").is_none() {
        return;
    }
    assert_eq!(
        std::env::var("CARGO_TARGET_DIR").unwrap(),
        "/tuic-dev-build-target"
    );
    let mut agent = CommandBuilder::new("agent");
    assert_eq!(
        agent.get_env("CARGO_TARGET_DIR"),
        Some(std::ffi::OsStr::new("/tuic-dev-build-target"))
    );
    sanitize_pty_parent_env(&mut agent);
    let shell = build_shell_command("/bin/sh");
    for cmd in [&agent, &shell] {
        for key in [
            "CARGO_TARGET_DIR",
            "CARGO_MANIFEST_DIR",
            "CARGO_MANIFEST_PATH",
            "CARGO_MANIFEST_LINKS",
            "CARGO_PKG_NAME",
            "OUT_DIR",
            "RUSTDOC",
            "CARGO_INCREMENTAL",
            "RUSTC_WRAPPER",
            "RUSTC_WORKSPACE_WRAPPER",
            "HOST_CC",
            "HOST_CXX",
            "MBX_SOCKET",
            "MBX_FUTURE_BUILD_KEY",
            "CARGO",
            "CARGO_PRIMARY_PACKAGE",
            "CARGO_BIN_NAME",
            "CARGO_CRATE_NAME",
            "CARGO_MAKEFLAGS",
            "CARGO_TARGET_TMPDIR",
            "CARGO_BIN_EXE_tuicommander",
            "CARGO_FEATURE_DESKTOP",
            "CARGO_CFG_TARGET_OS",
            "RUSTFLAGS",
            "RUSTC",
            "RUSTC_LINKER",
            "DEP_TUIC_NATIVE_PATH",
            "CARGO_ENCODED_RUSTFLAGS",
            "TARGET",
            "HOST",
            "PROFILE",
            "NUM_JOBS",
            "OPT_LEVEL",
            "DEBUG",
        ] {
            assert_eq!(cmd.get_env(key), None, "{key} leaked into a PTY");
        }
        assert_eq!(
            cmd.get_env("CARGO_HOME"),
            Some(std::ffi::OsStr::new("/user/cargo"))
        );
        assert_eq!(
            cmd.get_env("CARGO_TERM_COLOR"),
            Some(std::ffi::OsStr::new("always"))
        );
        assert_eq!(
            cmd.get_env("TUIC_TEST_USER_ENV"),
            Some(std::ffi::OsStr::new("keep-me"))
        );
    }
    agent.env("CARGO_TARGET_DIR", "/intentional-agent-target");
    agent.env("CARGO_INCREMENTAL", "1");
    agent.env("RUSTC_WRAPPER", "/intentional-agent-wrapper");
    agent.env("MBX_SOCKET", "/intentional-agent-mbx.sock");
    assert_eq!(
        agent.get_env("CARGO_TARGET_DIR"),
        Some(std::ffi::OsStr::new("/intentional-agent-target"))
    );
    assert_eq!(
        agent.get_env("CARGO_INCREMENTAL"),
        Some(std::ffi::OsStr::new("1"))
    );
    assert_eq!(
        agent.get_env("RUSTC_WRAPPER"),
        Some(std::ffi::OsStr::new("/intentional-agent-wrapper"))
    );
    assert_eq!(
        agent.get_env("MBX_SOCKET"),
        Some(std::ffi::OsStr::new("/intentional-agent-mbx.sock"))
    );
}

#[cfg(unix)]
#[test]
fn standby_refuses_session_with_background_work() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "background-standby";
    state.session_maps.session_states.insert(
        session_id.to_string(),
        crate::state::SessionState {
            agent_type: Some("codex".to_string()),
            background_work: true,
            ..Default::default()
        },
    );
    assert_eq!(standby_session(&state, session_id), Ok(false));
    assert!(!state.session_maps.standby_sessions.contains_key(session_id));
}

#[cfg(unix)]
#[test]
fn standby_refuses_session_with_pending_background_probe() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "background-probe-standby";
    state.session_maps.session_states.insert(
        session_id.to_string(),
        crate::state::SessionState {
            agent_type: Some("codex".to_string()),
            background_probe_turn_epoch: Some(3),
            turn_epoch: 3,
            ..Default::default()
        },
    );

    assert!(background_activity_blocks_standby(&state, session_id));
    assert_eq!(standby_session(&state, session_id), Ok(false));
    assert!(!state.session_maps.standby_sessions.contains_key(session_id));
}

#[test]
fn tombstone_transient_cleanup_removes_swarm_maps() {
    // F3: all per-child swarm state must be cleaned on exit.
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "sess-cleanup";
    let mcp_sid = "mcp-sess-cleanup";

    state
        .session_maps
        .session_parent
        .insert(sid.to_string(), "parent-sess".to_string());
    state
        .session_maps
        .shell_state_since_ms
        .insert(sid.to_string(), std::sync::atomic::AtomicU64::new(42));
    state
        .mcp
        .to_session
        .insert(mcp_sid.to_string(), sid.to_string());
    state
        .mcp
        .session_to_mcp
        .insert(sid.to_string(), vec![mcp_sid.to_string()]);
    state.peer_agents.insert(
        sid.to_string(),
        crate::state::PeerAgent {
            tuic_session: sid.to_string(),
            mcp_session_id: mcp_sid.to_string(),
            name: "worker".to_string(),
            project: None,
            registered_at: 1,
        },
    );
    state.agent_inbox.entry(sid.to_string()).or_default();
    state.agent_inbox_evictions.insert(sid.to_string(), 2);

    tombstone_transient_cleanup(sid, &state);

    assert!(
        !state.session_maps.session_parent.contains_key(sid),
        "session_parent must be removed"
    );
    assert!(
        !state.session_maps.shell_state_since_ms.contains_key(sid),
        "shell_state_since_ms must be removed"
    );
    assert!(
        !state.mcp.to_session.contains_key(mcp_sid),
        "mcp_to_session entry must be removed"
    );
    assert!(
        !state.mcp.session_to_mcp.contains_key(sid),
        "session_to_mcp entry must be removed"
    );
    assert!(!state.peer_agents.contains_key(sid));
    assert!(!state.agent_inbox.contains_key(sid));
    assert!(!state.agent_inbox_evictions.contains_key(sid));
}

fn completed_agent_session(state: &crate::state::AppState, sid: &str) {
    agent_session(state, sid, SHELL_IDLE);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .suggested_actions = Some(vec!["Review result".to_string()]);
    state
        .session_maps
        .silence_states
        .get(sid)
        .unwrap()
        .lock()
        .mark_suggest_candidate(vec!["Review result".to_string()], 0);
}

fn assert_new_turn_silence_evidence(silence: &SilenceState) {
    assert!(silence.explicit_busy());
    assert!(!silence.hook_busy());
    assert!(!silence.explicit_idle());
    assert!(!silence.idle_confirmed());
    assert!(silence.last_status_line_at.is_some());
    assert!(silence.screen_ready_pending_since.is_none());
    assert!(silence.interrupt_requested_at.is_none());
    assert!(silence.turn_started_by_input());
    assert!(!silence.evidence.activity_seen);
    assert!(!silence.completion_declared);
    assert!(silence.pending_suggest_items.is_none());
}

#[test]
fn submitted_epoch_and_busy_transition_are_one_critical_section() {
    use std::sync::atomic::Ordering;

    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let session_id = "submitted-atomic";
    completed_agent_session(&state, session_id);
    let (start_tx, start_rx) = std::sync::mpsc::channel();
    let (lock_held_tx, lock_held_rx) = std::sync::mpsc::channel();
    let (finished_tx, finished_rx) = std::sync::mpsc::channel();
    let observer_state = Arc::clone(&state);
    let observer = std::thread::spawn(move || {
        start_rx.recv().unwrap();
        let silence = observer_state
            .session_maps
            .silence_states
            .get(session_id)
            .unwrap()
            .clone();
        lock_held_tx.send(silence.try_lock().is_none()).unwrap();
        let transitioned =
            try_shell_transition(&observer_state, session_id, SHELL_IDLE, SHELL_IDLE, false);
        finished_tx.send(transitioned).unwrap();
    });

    note_submitted_input_with_hook(&state, session_id, || {
        assert_eq!(
            state
                .session_maps
                .session_states
                .get(session_id)
                .unwrap()
                .turn_epoch,
            1
        );
        start_tx.send(()).unwrap();
        assert!(
            lock_held_rx.recv().unwrap(),
            "epoch mutation must retain the lifecycle lock until BUSY"
        );
    });

    assert!(
        !finished_rx.recv().unwrap(),
        "observer must see BUSY after the submitted-turn reservation"
    );
    observer.join().unwrap();
    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(session_id)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_BUSY
    );
}

#[test]
fn new_turn_wins_before_queued_old_idle_transition() {
    use std::sync::atomic::Ordering;

    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let child_id = "inverse-idle-child";
    let parent_id = "inverse-idle-parent";
    agent_session(&state, child_id, SHELL_BUSY);
    state
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();

    let (snapshotted_tx, snapshotted_rx) = std::sync::mpsc::channel();
    let (continue_tx, continue_rx) = std::sync::mpsc::channel();
    let old_transition_state = Arc::clone(&state);
    let old_transition = std::thread::spawn(move || {
        let observed_turn_epoch = old_transition_state
            .session_maps
            .session_states
            .get(child_id)
            .map(|session| session.turn_epoch);
        try_shell_transition_with_hooks(
            ShellTransitionRequest {
                state: &old_transition_state,
                session_id: child_id,
                expected: SHELL_BUSY,
                new: SHELL_IDLE,
                notify_parent: true,
                observed_turn_epoch,
            },
            ShellTransitionHooks {
                after_epoch_snapshot: || {
                    snapshotted_tx.send(()).unwrap();
                    continue_rx.recv().unwrap();
                },
                after_cas: || {},
                before_parent_dispatch: || {},
            },
        )
    });

    snapshotted_rx.recv().unwrap();
    note_submitted_input(&state, child_id);
    continue_tx.send(()).unwrap();

    assert!(
        !old_transition.join().unwrap(),
        "an idle transition from the prior epoch must not publish"
    );
    assert_eq!(
        state
            .session_maps
            .session_states
            .get(child_id)
            .unwrap()
            .turn_epoch,
        1
    );
    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(child_id)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_BUSY
    );
    assert!(state.agent_inbox.get(parent_id).unwrap().is_empty());
}

#[test]
fn explicit_idle_evidence_from_prior_turn_cannot_idle_new_submission() {
    use std::sync::atomic::Ordering;

    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "explicit-idle-epoch";
    agent_session(&state, session_id, SHELL_BUSY);

    transition_explicit_shell_state_with_hook(&state, session_id, SHELL_IDLE, "idle", true, || {
        note_submitted_input(&state, session_id)
    });

    assert_eq!(
        state
            .session_maps
            .session_states
            .get(session_id)
            .unwrap()
            .turn_epoch,
        1
    );
    assert_new_turn_silence_evidence(
        &state
            .session_maps
            .silence_states
            .get(session_id)
            .unwrap()
            .lock(),
    );
    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(session_id)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_BUSY,
        "an explicit idle marker observed before the new input must be discarded"
    );
}

#[test]
fn silence_idle_decision_from_prior_turn_cannot_idle_new_submission() {
    use std::sync::atomic::{AtomicU64, Ordering};

    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "silence-idle-epoch";
    agent_session(&state, session_id, SHELL_BUSY);
    state.session_maps.last_output_ms.insert(
        session_id.to_string(),
        AtomicU64::new(now_epoch_ms().saturating_sub(AGENT_IDLE_MS + 1)),
    );

    let decision = should_transition_idle_with_hook(&state, session_id, || {
        note_submitted_input(&state, session_id);
    });
    assert!(decision.should_transition);
    assert_eq!(decision.turn_epoch, Some(0));

    assert!(!try_shell_transition_for_epoch(
        &state,
        session_id,
        SHELL_BUSY,
        SHELL_IDLE,
        true,
        decision.turn_epoch,
    ));
    assert_eq!(
        state
            .session_maps
            .session_states
            .get(session_id)
            .unwrap()
            .turn_epoch,
        1
    );
    assert_new_turn_silence_evidence(
        &state
            .session_maps
            .silence_states
            .get(session_id)
            .unwrap()
            .lock(),
    );
    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(session_id)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_BUSY
    );
}

#[test]
fn parent_dispatch_runs_after_child_lifecycle_lock_release() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "dispatch-child";
    let parent_id = "dispatch-parent";
    agent_session(&state, child_id, SHELL_BUSY);
    agent_session(&state, parent_id, SHELL_IDLE);
    state
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    let observed_turn_epoch = state
        .session_maps
        .session_states
        .get(child_id)
        .map(|session| session.turn_epoch);

    assert!(try_shell_transition_with_hooks(
        ShellTransitionRequest {
            state: &state,
            session_id: child_id,
            expected: SHELL_BUSY,
            new: SHELL_IDLE,
            notify_parent: true,
            observed_turn_epoch,
        },
        ShellTransitionHooks {
            after_epoch_snapshot: || {},
            after_cas: || {},
            before_parent_dispatch: || {
                let silence = state
                    .session_maps
                    .silence_states
                    .get(child_id)
                    .unwrap()
                    .clone();
                assert!(
                    silence.try_lock().is_some(),
                    "child lifecycle lock must be released before parent PTY dispatch"
                );
            },
        },
    ));
    assert_eq!(state.agent_inbox.get(parent_id).unwrap().len(), 1);
}

#[test]
fn codex_heuristic_idle_is_not_safe_for_injection_or_standby() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "codex-heuristic", SHELL_IDLE);
    state
        .session_maps
        .session_states
        .get_mut("codex-heuristic")
        .unwrap()
        .agent_type = Some("codex".to_string());
    state
        .session_maps
        .silence_states
        .get("codex-heuristic")
        .unwrap()
        .lock()
        .force_idle_unconfirmed();

    assert!(!idle_is_confirmed(&state, "codex-heuristic"));
    assert!(!should_inject_now(&state, "codex-heuristic"));

    state
        .session_maps
        .silence_states
        .get("codex-heuristic")
        .unwrap()
        .lock()
        .confirm_idle();
    assert!(idle_is_confirmed(&state, "codex-heuristic"));
    assert!(should_inject_now(&state, "codex-heuristic"));
}

#[test]
fn uncertain_injection_cannot_be_cleared_by_a_stale_ready_screen() {
    let mut silence = SilenceState::new();
    let token = silence.begin_injection_claim(true);
    silence.mark_injection_uncertain(token);
    silence.screen_ready_pending_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);

    assert!(!silence.note_ready_screen());
    assert!(silence.injection_delivery_uncertain);
    assert!(!silence.idle_confirmed());
}

#[test]
fn deliver_reenqueue_recovers_message_when_idle_races_enqueue() {
    // CONC-A (story 101-20e3): the sender's should_inject_now read and its
    // pending_injections push are not atomic vs a concurrent BUSY→IDLE flush. If the
    // silence timer transitions to idle and drains the (still-empty) queue between
    // them, the message would be stranded until the NEXT idle cycle. The post-enqueue
    // re-check must recover it. Stress the race with a barrier: after both the sender
    // and the transition+flush complete with the session ending idle, the queue MUST
    // be empty (message delivered) regardless of interleaving. Pre-fix this fails in
    // the bug window (sender reads busy, timer flushes empty, sender enqueues with no
    // recovery). With no live PTY in this unit test, the recovered delivery
    // must remain queued exactly once rather than being lost or duplicated.
    use std::sync::{Arc, Barrier};
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    for i in 0..500 {
        agent_session(&state, "race", SHELL_BUSY);
        state.pending_injections.remove("race");

        let barrier = Arc::new(Barrier::new(2));
        let (s1, b1) = (Arc::clone(&state), Arc::clone(&barrier));
        let sender = std::thread::spawn(move || {
            b1.wait();
            deliver_notice_to_pty(&s1, "race", "[TUIC message from lead] go");
        });
        let (s2, b2) = (Arc::clone(&state), Arc::clone(&barrier));
        let timer = std::thread::spawn(move || {
            b2.wait();
            // Silence timer: BUSY→IDLE also runs flush_pending_injections on idle.
            s2.session_maps
                .silence_states
                .get("race")
                .unwrap()
                .lock()
                .confirm_idle();
            try_shell_transition(&s2, "race", SHELL_BUSY, SHELL_IDLE, false);
            emit_shell_state(&s2, "race", "idle");
            flush_pending_injections(&s2, "race")
        });
        sender.join().unwrap();
        if let Some(worker) = timer.join().unwrap() {
            worker.join().unwrap();
        }

        let queued = state
            .pending_injections
            .get("race")
            .map(|q| q.len())
            .unwrap_or(0);
        assert_eq!(queued, 1, "iteration {i}: message lost or duplicated");
    }
}

// ---- CONC-B (story 100-e303 / commit 5410cc3d): resize_session_core ----
// resize_session_core serializes the whole grid+PTY resize for a session
// under one per-session lock so two concurrent differing resizes can never
// interleave and leave grid and PTY at mismatched dimensions. These cover
// the invalid-dims edge, the no-op guard, the (0,0) startup-dims seed, and
// the concurrent-race invariant (mirrors the CONC-A barrier test above).

/// Insert a live VtLogBuffer at the given dims so the grid path in
/// resize_session_core runs against a real grid.
fn seed_vt_grid(state: &crate::state::AppState, sid: &str, rows: u16, cols: u16) {
    state.grid.vt_log_buffers.insert(
        sid.to_string(),
        Mutex::new(VtLogBuffer::new(rows, cols, 1000)),
    );
}

/// The reflow rewraps the whole ring and serializes a full frame under the VT
/// mutex. Run inline in the IPC handler — on macOS, the main thread — a
/// drag-resize froze the WebView for the length of every reflow it fired.
#[tokio::test]
async fn a_resize_reflows_off_the_calling_thread() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "resize-off-thread";
    seed_vt_grid(&state, sid, 24, 80);

    let caller = std::thread::current().id();
    // The session has no PTY, so this ends in "Session not found" — after the
    // grid reflow, which is precisely the work that must not run here.
    let _ = resize_session_off_thread(&state, sid.to_string(), 40, 120).await;

    assert!(resize_thread(sid).is_some(), "the reflow never ran at all");
    assert_ne!(
        resize_thread(sid),
        Some(caller),
        "the reflow ran on the calling thread"
    );
}

#[test]
fn resize_rejects_zero_dims() {
    let state = crate::state::tests_support::make_test_app_state();
    // rows==0 / cols==0 are rejected before any lock, grid, or PTY work — the
    // (0,0) pair is reserved as the "never applied" sentinel inside the lock.
    assert!(resize_session_core(&state, "s", 0, 80).is_err());
    assert!(resize_session_core(&state, "s", 24, 0).is_err());
    // A rejected resize must not even create a resize_locks entry.
    assert!(!state.session_maps.resize_locks.contains_key("s"));
}

#[test]
fn resize_noop_guard_returns_none_on_matching_dims() {
    let state = crate::state::tests_support::make_test_app_state();
    // Pre-seed the last-applied dims, as if a prior resize reached the PTY.
    state
        .session_maps
        .resize_locks
        .insert("s".to_string(), Arc::new(Mutex::new((24, 80))));
    // Same dims → no-op returning None WITHOUT touching the (absent) session.
    // Without the guard this would fall through to sessions.get and fail with
    // "Session not found", so Ok(None) proves the guard short-circuited first.
    assert_eq!(resize_session_core(&state, "s", 24, 80), Ok(None));
}

#[test]
fn resize_seeds_applied_from_grid_and_noops_at_startup_dims() {
    let state = crate::state::tests_support::make_test_app_state();
    // Grid exists at the startup dims but resize_locks is empty → the lock
    // opens at the (0,0) never-applied sentinel.
    seed_vt_grid(&state, "s", 24, 80);
    // A first resize matching only the startup dims must seed *applied from
    // the live grid and then no-op — no gratuitous SIGWINCH, no session touch.
    assert_eq!(resize_session_core(&state, "s", 24, 80), Ok(None));
    // The seed must have populated resize_locks with the live grid dims.
    assert_eq!(
        *state.session_maps.resize_locks.get("s").unwrap().lock(),
        (24, 80),
        "first resize must seed the last-applied dims from the live grid"
    );
}

/// Build a real PTY session (openpty + a long-lived child) at the given dims,
/// plus a matching VtLogBuffer, so resize_session_core reaches the real
/// master.resize() ioctl and get_size() reflects it.
#[cfg(unix)]
fn spawn_real_pty_session(state: &crate::state::AppState, sid: &str, rows: u16, cols: u16) {
    let pair = native_pty_system()
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("openpty");
    let mut cmd = CommandBuilder::new("/bin/sh");
    cmd.args(["-c", "sleep 30"]);
    let child = pair.slave.spawn_command(cmd).expect("spawn shell");
    let master = pair.master;
    let writer = master.take_writer().expect("writer");
    state.session_maps.sessions.insert(
        sid.to_string(),
        Mutex::new(PtySession {
            launch_receipt: None,
            writer: Arc::new(Mutex::new(writer)),
            master,
            _child: child,
            paused: Arc::new(AtomicBool::new(false)),
            worktree: None,
            initial_cwd: None,
            cwd: None,
            display_name: None,
            display_name_is_custom: false,
            display_name_from_spawn: false,
            is_remote: false,
            shell: "/bin/sh".to_string(),
        }),
    );
    seed_vt_grid(state, sid, rows, cols);
}

/// CONC-B invariant: two concurrent resizes with different dims must leave the
/// grid AND the PTY at the same dimensions — both equal to whichever call
/// acquired the per-session lock last — never a grid/PTY mismatch. Stress the
/// race with a barrier over many iterations, like the CONC-A test above.
#[cfg(unix)]
#[test]
fn concurrent_differing_resizes_leave_grid_and_pty_consistent() {
    use std::sync::Barrier;
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "resize-race";
    spawn_real_pty_session(&state, sid, 24, 80);

    const A: (u16, u16) = (30, 100);
    const B: (u16, u16) = (40, 120);

    for i in 0..100 {
        let barrier = Arc::new(Barrier::new(2));
        let (s1, b1) = (Arc::clone(&state), Arc::clone(&barrier));
        let t1 = std::thread::spawn(move || {
            b1.wait();
            let _ = resize_session_core(&s1, sid, A.0, A.1);
        });
        let (s2, b2) = (Arc::clone(&state), Arc::clone(&barrier));
        let t2 = std::thread::spawn(move || {
            b2.wait();
            let _ = resize_session_core(&s2, sid, B.0, B.1);
        });
        t1.join().unwrap();
        t2.join().unwrap();

        // The recorded applied dims, the live grid dims, and the real PTY size
        // must all agree, and agree on one of the two racing targets.
        let applied = *state.session_maps.resize_locks.get(sid).unwrap().lock();
        let (grid_rows, grid_cols) = {
            let vt = state.grid.vt_log_buffers.get(sid).unwrap();
            let vt = vt.lock();
            (vt.grid_screen_lines() as u16, vt.grid_columns() as u16)
        };
        let pty_size = state
            .session_maps
            .sessions
            .get(sid)
            .unwrap()
            .lock()
            .master
            .get_size()
            .expect("get_size");
        assert_eq!(
            (grid_rows, grid_cols),
            applied,
            "iter {i}: grid dims must match recorded applied dims"
        );
        assert_eq!(
            (pty_size.rows, pty_size.cols),
            applied,
            "iter {i}: PTY size must match recorded applied dims"
        );
        assert!(
            applied == A || applied == B,
            "iter {i}: applied {applied:?} must be one of the racing targets"
        );
    }
}

#[test]
fn idle_transition_emits_before_submitting_one_pending_message() {
    use std::collections::VecDeque;
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "sess", SHELL_BUSY);
    let mut q = VecDeque::new();
    q.push_back(crate::state::PendingInjection::notice("msg-1"));
    q.push_back(crate::state::PendingInjection::notice("msg-2"));
    state.pending_injections.insert("sess".to_string(), q);

    // The transition is driven by verified ready-screen/Stop evidence in
    // production. Model that evidence before testing its delivery side effect.
    state
        .session_maps
        .silence_states
        .get("sess")
        .unwrap()
        .lock()
        .confirm_idle();

    let mut events = state.event_bus.subscribe();
    assert!(try_shell_transition(
        &state, "sess", SHELL_BUSY, SHELL_IDLE, false
    ));
    emit_shell_state(&state, "sess", "idle");
    flush_one_pending_as_submitted(&state, "sess");

    assert_eq!(
        state.pending_injections.get("sess").map(|q| q.len()),
        Some(1),
        "only one queued message may be submitted per idle turn"
    );
    let states: Vec<String> = std::iter::from_fn(|| events.try_recv().ok())
        .filter_map(|event| match event {
            crate::state::AppEvent::PtyParsed { parsed, .. }
                if parsed.get("type").and_then(|v| v.as_str()) == Some("shell-state") =>
            {
                parsed
                    .get("state")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            }
            _ => None,
        })
        .collect();
    assert_eq!(states, vec!["idle", "busy"]);
}

#[test]
fn flush_keeps_pending_while_question_confident() {
    use std::collections::VecDeque;
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    use std::sync::atomic::AtomicU8;
    state
        .session_maps
        .shell_states
        .insert("sess".to_string(), AtomicU8::new(SHELL_BUSY));
    state.session_maps.session_states.insert(
        "sess".to_string(),
        crate::state::SessionState {
            spawn_root_role: crate::state::SpawnRootRole::DirectProgram,
            agent_type: Some("claude".to_string()),
            awaiting_input: true,
            question_confident: true,
            ..Default::default()
        },
    );
    let mut q = VecDeque::new();
    q.push_back(crate::state::PendingInjection::notice("later"));
    state.pending_injections.insert("sess".to_string(), q);

    state.session_maps.silence_states.insert(
        "sess".to_string(),
        Arc::new(Mutex::new(SilenceState::new())),
    );
    state
        .session_maps
        .silence_states
        .get("sess")
        .unwrap()
        .lock()
        .confirm_idle();

    try_shell_transition(&state, "sess", SHELL_BUSY, SHELL_IDLE, false);
    emit_shell_state(&state, "sess", "idle");
    flush_pending_injections(&state, "sess")
        .expect("pending flush starts a session worker")
        .join()
        .expect("session worker finishes before question clears");
    assert_eq!(
        state.pending_injections.get("sess").map(|q| q.len()),
        Some(1),
        "must not answer a confident user prompt — keep queued until it clears"
    );

    // The question clears (user answered) while the session is already idle:
    // the unblock flush must drain the queue with no further transition.
    state
        .session_maps
        .session_states
        .get_mut("sess")
        .unwrap()
        .question_confident = false;
    flush_one_pending_as_submitted(&state, "sess");
    assert_eq!(
        state.pending_injections.get("sess").map(|q| q.len()),
        Some(0),
        "unblock flush must submit once the confident question clears"
    );
}

#[cfg(unix)]
impl std::io::Write for TimedWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.writes
            .lock()
            .unwrap()
            .push((std::time::Instant::now(), bytes.to_vec()));
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn state_change_to_parent_without_managed_pty_stays_inbox_only() {
    // Logical agent state alone is not proof of a managed PTY. A child state
    // change must remain available in the inbox without creating a phantom
    // terminal injection for an external peer.
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    agent_session(&state, "parent", SHELL_BUSY);
    state
        .session_maps
        .session_parent
        .insert("child".to_string(), "parent".to_string());

    push_state_change_to_parent(
        &state,
        "child",
        serde_json::json!({"type":"state_change","state":"idle","session_id":"child"}),
    );
    // The wake is dispatched on the injection worker; drain it, or "no terminal
    // input happened" would pass merely by asserting too early.
    wait_for_injection_queue();

    // Inbox got the JSON payload…
    assert_eq!(
        state.agent_inbox.get("parent").map(|q| q.len()),
        Some(1),
        "parent inbox must receive the state_change"
    );
    assert!(
        !state.pending_injections.contains_key("parent"),
        "an external peer without a managed PTY must not receive terminal input"
    );
    let message_id = state
        .agent_inbox
        .get("parent")
        .unwrap()
        .front()
        .unwrap()
        .id
        .clone();
    assert_eq!(state.agent_delivery_owner("parent", &message_id), None);
}

#[cfg(unix)]
fn run_progress_intent_case(
    chunks: &[&str],
    cols: u16,
    agent: bool,
    timer_idle: bool,
) -> (Vec<(String, Option<String>)>, Vec<String>) {
    run_progress_intent_case_ending(chunks, cols, agent, timer_idle, None, true, false)
}

#[cfg(unix)]
fn run_progress_intent_case_ending(
    chunks: &[&str],
    cols: u16,
    agent: bool,
    timer_idle: bool,
    teardown: Option<&str>,
    idle_is_quiet: bool,
    fail_journal: bool,
) -> (Vec<(String, Option<String>)>, Vec<String>) {
    run_progress_intent_case_grid(
        chunks,
        cols,
        agent,
        timer_idle,
        teardown,
        idle_is_quiet,
        fail_journal,
        2000,
        true,
    )
}

#[test]
fn protocol_idle_survives_ready_timer() {
    let mut silence = SilenceState::new();
    silence.note_explicit_state(SHELL_IDLE, true);
    silence.screen_ready_pending_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    assert!(silence.note_ready_screen());
    assert!(
        silence.explicit_idle(),
        "Ready must not downgrade a completion hook"
    );
    assert!(
        !silence
            .evidence
            .record_busy(EvidenceRank::Screen, "real-activity")
    );
    assert!(silence.explicit_idle());
}

/// The whole point of the adapter: goose can now recover to idle from the
/// screen, exactly as opencode does.
#[test]
fn goose_ready_screen_recovers_long_lived_shell_busy() {
    assert!(has_ready_screen_adapter(Some("goose")));

    let mut silence = SilenceState::new();
    silence.note_explicit_state(SHELL_BUSY, false);
    silence.note_real_activity();
    silence.screen_ready_pending_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    assert!(silence.note_ready_screen());
    assert!(!silence.explicit_busy());
    assert!(silence.idle_confirmed());
}

/// 744-138c baseline/after evidence: dump the exact `ParsedEvent` sequence
/// produced by replaying every committed `.tcap` fixture, one line per event.
/// Run before and after the SilenceState/decide() refactor and diff the two
/// captures — an empty diff is the "bit for bit" proof the story requires.
/// `#[ignore]` because it is a evidence-capture harness, not a pass/fail gate;
/// `replay_capture` never touches SilenceState (see its doc comment), so this
/// is expected to be stable across that refactor by construction.
#[test]
#[ignore = "744-138c evidence capture — run manually before/after the refactor"]
fn dump_committed_tcap_fixture_event_sequences_744() {
    let mut names: Vec<_> = std::fs::read_dir(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/fixtures/agent_prompts"),
    )
    .expect("readable fixtures dir")
    .filter_map(|entry| {
        let path = entry.ok()?.path();
        (path.extension().and_then(|e| e.to_str()) == Some("tcap"))
            .then(|| path.file_name().unwrap().to_string_lossy().into_owned())
    })
    .collect();
    names.sort();
    for name in names {
        let bytes = agent_prompt_fixture(&name);
        let events = replay_capture(&bytes, false);
        println!("=== {name} ===");
        for event in &events {
            println!("{event:?}");
        }
    }
}

#[test]
fn codex_ready_prompt_remains_idle_and_deliverable() {
    let sid = "codex-ready-no-question";
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, sid, SHELL_IDLE);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .agent_type = Some("codex".into());
    state
        .session_maps
        .silence_states
        .get(sid)
        .unwrap()
        .lock()
        .confirm_idle();
    let rows = vec![
        "• Done. Updated Cargo.toml.".to_string(),
        "› Improve documentation in @filename".to_string(),
        "  gpt-5.5 high · ~/Gits/LS/agent2".to_string(),
    ];
    assert_eq!(
        detect_codex_screen_activity(&rows),
        AgentScreenActivity::Ready
    );
    assert!(
        !state
            .session_maps
            .session_states
            .get(sid)
            .unwrap()
            .awaiting_input
    );
    assert!(
        should_inject_now(&state, sid),
        "the idle composer must accept the next turn"
    );
}

/// Closing a tab must kill the agent grandchild, not just the shell.
///
/// Mirrors `claude` launched inside the PTY's shell: shell → grandchild,
/// both ignoring SIGINT/SIGTERM/SIGHUP so only the SIGKILL on the foreground
/// process group can reap them. Before the killpg fix, `close_pty_core`
/// SIGKILLed the shell alone and the grandchild was orphaned to init.
#[cfg(unix)]
#[test]
fn close_pty_core_kills_agent_grandchild() {
    use std::time::{Duration, Instant};

    let scratch = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let pidfile = scratch.path().join("grandchild.pid");

    let pty = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("openpty");

    // Outer shell ignores the catchable signals and backgrounds a grandchild
    // that also ignores them, recording the grandchild PID for the probe.
    let script = format!(
        "trap '' INT TERM HUP; sh -c 'trap \"\" INT TERM HUP; sleep 30' & echo $! > {}; wait",
        pidfile.display()
    );
    let mut cmd = CommandBuilder::new("/bin/sh");
    cmd.args(["-c", &script]);
    let child = pty.slave.spawn_command(cmd).expect("spawn shell");

    let master = pty.master;
    let writer = master.take_writer().expect("writer");

    let state = crate::state::tests_support::make_test_app_state();
    let sid = "test-pgkill";
    state
        .metrics
        .active_sessions
        .fetch_add(1, Ordering::Relaxed);
    state.session_maps.sessions.insert(
        sid.to_string(),
        Mutex::new(PtySession {
            launch_receipt: None,
            writer: Arc::new(Mutex::new(writer)),
            master,
            _child: child,
            paused: Arc::new(AtomicBool::new(false)),
            worktree: None,
            initial_cwd: None,
            cwd: None,
            display_name: None,
            display_name_is_custom: false,
            display_name_from_spawn: false,
            is_remote: false,
            shell: "/bin/sh".to_string(),
        }),
    );

    let pid_alive = |pid: libc::pid_t| unsafe { libc::kill(pid, 0) } == 0;
    let read_pid = || {
        std::fs::read_to_string(&pidfile)
            .ok()
            .and_then(|s| s.trim().parse::<libc::pid_t>().ok())
    };

    // Wait for the grandchild to come up and record its PID (up to ~3s).
    let mut grandchild = None;
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if let Some(pid) = read_pid() {
            grandchild = Some(pid);
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let grandchild = grandchild.expect("grandchild PID should be written");
    assert!(
        pid_alive(grandchild),
        "grandchild should be alive before close"
    );

    close_pty_core(&state, sid, false);

    // killpg(SIGKILL) is untrappable: the grandchild must be gone shortly.
    let mut dead = false;
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if !pid_alive(grandchild) {
            dead = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        dead,
        "grandchild {grandchild} survived tab close — orphaned process tree"
    );
}

/// Populate the per-session maps that no teardown phase used to own, plus the
/// two the post-mortem read needs. Deliberately independent of the production
/// enumeration: a teardown test that reuses the list it verifies proves nothing.
fn populate_unowned_session_maps(state: &crate::state::AppState, sid: &str) {
    use std::sync::atomic::{AtomicBool, AtomicU64};
    state
        .session_maps
        .slash_mode
        .insert(sid.to_string(), AtomicBool::new(true));
    state
        .session_maps
        .last_input_ms
        .insert(sid.to_string(), AtomicU64::new(7));
    state
        .session_maps
        .has_osc133_integration
        .insert(sid.to_string(), ());
    state.agent_read_cursor.insert(sid.to_string(), 3);
    state
        .session_maps
        .marker_stats
        .insert(sid.to_string(), crate::state::MarkerStats::default());
    state.ai.session_knowledge.insert(
        sid.to_string(),
        Mutex::new(crate::ai_agent::knowledge::SessionKnowledge::new()),
    );
    state
        .session_maps
        .session_visibility
        .insert(sid.to_string(), true);
    state
        .session_maps
        .term_aliases
        .insert(sid.to_string(), "tc-9".to_string());
}

/// The swarm/identity maps. `tombstone_transient_cleanup` has always reaped
/// these; `cleanup_session` never did, which is the divergence F8 removes.
fn populate_swarm_session_maps(state: &crate::state::AppState, sid: &str, mcp_sid: &str) {
    state
        .session_maps
        .session_parent
        .insert(sid.to_string(), "parent-sess".to_string());
    state
        .session_maps
        .shell_state_since_ms
        .insert(sid.to_string(), std::sync::atomic::AtomicU64::new(42));
    state
        .mcp
        .to_session
        .insert(mcp_sid.to_string(), sid.to_string());
    state
        .mcp
        .session_to_mcp
        .insert(sid.to_string(), vec![mcp_sid.to_string()]);
    state.peer_agents.insert(
        sid.to_string(),
        crate::state::PeerAgent {
            tuic_session: sid.to_string(),
            mcp_session_id: mcp_sid.to_string(),
            name: "worker".to_string(),
            project: None,
            registered_at: 1,
        },
    );
    state.agent_inbox.entry(sid.to_string()).or_default();
    state.agent_inbox_evictions.insert(sid.to_string(), 2);
}

#[test]
fn closing_a_session_reaps_the_swarm_maps_too() {
    // The two teardowns were enumerated by hand and drifted: an explicit close
    // over HTTP DELETE goes straight to cleanup_session, which left every peer
    // identity, inbox and mcp mapping behind for the life of the process.
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "close-swarm";
    let mcp_sid = "mcp-close-swarm";
    populate_swarm_session_maps(&state, sid, mcp_sid);

    cleanup_session(sid, &state);

    assert!(!state.session_maps.session_parent.contains_key(sid));
    assert!(!state.session_maps.shell_state_since_ms.contains_key(sid));
    assert!(!state.mcp.to_session.contains_key(mcp_sid));
    assert!(!state.mcp.session_to_mcp.contains_key(sid));
    assert!(!state.peer_agents.contains_key(sid));
    assert!(!state.agent_inbox.contains_key(sid));
    assert!(!state.agent_inbox_evictions.contains_key(sid));
}

#[test]
fn closing_a_session_reaps_the_maps_no_phase_owned() {
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "close-unowned";
    populate_unowned_session_maps(&state, sid);

    cleanup_session(sid, &state);

    assert!(!state.session_maps.slash_mode.contains_key(sid));
    assert!(!state.session_maps.last_input_ms.contains_key(sid));
    assert!(!state.session_maps.has_osc133_integration.contains_key(sid));
    assert!(!state.agent_read_cursor.contains_key(sid));
    assert!(!state.session_maps.marker_stats.contains_key(sid));
    assert!(!state.session_maps.session_visibility.contains_key(sid));
    assert!(!state.session_maps.term_aliases.contains_key(sid));

    // Owned elsewhere, deliberately untouched — see the DEFERRED note on
    // remove_post_mortem_session_state. Knowledge is what the next session
    // inherits.
    assert!(state.ai.session_knowledge.contains_key(sid));
}

#[test]
fn a_tombstone_drops_live_process_state_and_keeps_the_post_mortem_maps() {
    // A tombstone is still readable for TOMBSTONE_TTL_MS, so the split is not
    // "reap everything": what the dead process owned goes, what a post-mortem
    // read needs stays.
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "tombstone-split";
    populate_unowned_session_maps(&state, sid);

    tombstone_transient_cleanup(sid, &state);

    assert!(
        !state.session_maps.slash_mode.contains_key(sid),
        "input mode belongs to the dead process"
    );
    assert!(!state.session_maps.last_input_ms.contains_key(sid));
    assert!(
        !state.session_maps.has_osc133_integration.contains_key(sid),
        "shell integration belongs to the dead shell"
    );
    assert!(!state.agent_read_cursor.contains_key(sid));

    assert!(
        state.session_maps.marker_stats.contains_key(sid),
        "marker tallies are exactly what a post-mortem question asks for"
    );
    assert!(
        state.ai.session_knowledge.contains_key(sid),
        "knowledge is flushed to disk by a 2s task — reaping it here loses it"
    );
    assert!(
        state.session_maps.term_aliases.contains_key(sid),
        "the tab still shows"
    );
    assert!(state.session_maps.session_visibility.contains_key(sid));
}

#[test]
fn reaping_a_tombstone_leaves_no_session_state_behind() {
    // The normal exit path is tombstone → sweeper, and cleanup_session is never
    // called on it. Anything the sweeper's list forgot therefore leaked for the
    // life of the process, not for TOMBSTONE_TTL_MS — which is what happened to
    // the terminal alias.
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "tombstone-reaped";
    populate_unowned_session_maps(&state, sid);
    populate_swarm_session_maps(&state, sid, "mcp-tombstone-reaped");

    tombstone_transient_cleanup(sid, &state);
    remove_post_mortem_session_state(sid, &state);

    assert!(!state.session_maps.term_aliases.contains_key(sid));
    assert!(!state.session_maps.marker_stats.contains_key(sid));
    assert!(!state.session_maps.session_visibility.contains_key(sid));
    assert!(!state.session_maps.last_output_ms.contains_key(sid));
    assert!(!state.session_maps.slash_mode.contains_key(sid));
    assert!(!state.peer_agents.contains_key(sid));
}

/// Stamp a tombstone that is already older than the TTL.
fn stamp_aged_tombstone(state: &crate::state::AppState, sid: &str, now_ms: u64) {
    state.session_maps.last_output_ms.insert(
        sid.to_string(),
        AtomicU64::new(now_ms - TOMBSTONE_TTL_MS - 1),
    );
}

#[test]
fn a_timestamp_left_without_buffers_is_still_reaped() {
    // An explicit DELETE runs the full cleanup, and the reader thread can then
    // reach EOF and re-stamp last_output_ms through the tombstone path. The
    // buffers are already gone, so a sweeper that discovers candidates by
    // walking output_buffers never sees that lone entry again.
    let state = crate::state::tests_support::make_test_app_state();
    let now_ms = 10 * TOMBSTONE_TTL_MS;
    stamp_aged_tombstone(&state, "orphan-stamp", now_ms);

    assert_eq!(
        aged_out_tombstones(&state, now_ms),
        vec!["orphan-stamp".to_string()],
        "a stamp with no buffers is still session state to reap"
    );
}

#[cfg(unix)]
#[test]
fn a_session_id_reused_before_the_sweep_is_not_reaped() {
    // The HTTP spawn path accepts a caller-supplied id, so an aged tombstone's
    // id can come back to life between candidate selection and removal. Reaping
    // it then deletes the LIVE session's buffers, alias and visibility.
    let state = crate::state::tests_support::make_test_app_state();
    let now_ms = 10 * TOMBSTONE_TTL_MS;
    let sid = "reused-id";
    stamp_aged_tombstone(&state, sid, now_ms);
    state
        .session_maps
        .term_aliases
        .insert(sid.to_string(), "tc-1".to_string());
    let candidates = aged_out_tombstones(&state, now_ms);
    assert_eq!(candidates, vec![sid.to_string()]);

    // The race: a client recreates the id after selection, before removal.
    crate::state::tests_support::insert_dummy_session(&state, sid);
    reap_tombstones(&state, &candidates);

    assert!(
        state.session_maps.term_aliases.contains_key(sid),
        "the live session that reclaimed this id must keep its state"
    );
}

#[cfg(unix)]
#[test]
fn cleanup_session_removes_session_and_decrements_metrics() {
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "cleanup-real";
    spawn_short_session(&state, sid);
    let before = state.metrics.active_sessions.load(Ordering::Relaxed);
    assert!(state.session_maps.sessions.contains_key(sid));

    cleanup_session(sid, &state);

    assert!(
        !state.session_maps.sessions.contains_key(sid),
        "the live session entry must be removed"
    );
    assert_eq!(
        state.metrics.active_sessions.load(Ordering::Relaxed),
        before - 1,
        "removing a live session must decrement the active-session gauge"
    );
}

/// Insert a minimal real PTY session (short-lived `sleep`) so functions that
/// require a live `PtySession` can be exercised. Mirrors `create_pty`'s
/// active-session bookkeeping.
#[cfg(unix)]
fn spawn_short_session(state: &crate::state::AppState, sid: &str) {
    let pty = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("openpty");
    let mut cmd = CommandBuilder::new("/bin/sh");
    cmd.args(["-c", "sleep 5"]);
    let child = pty.slave.spawn_command(cmd).expect("spawn");
    let master = pty.master;
    let writer = master.take_writer().expect("writer");
    state
        .metrics
        .active_sessions
        .fetch_add(1, Ordering::Relaxed);
    state.session_maps.sessions.insert(
        sid.to_string(),
        Mutex::new(PtySession {
            launch_receipt: None,
            writer: Arc::new(Mutex::new(writer)),
            master,
            _child: child,
            paused: Arc::new(AtomicBool::new(false)),
            worktree: None,
            initial_cwd: None,
            cwd: None,
            display_name: None,
            display_name_is_custom: false,
            display_name_from_spawn: false,
            is_remote: false,
            shell: "/bin/sh".to_string(),
        }),
    );
}

/// End the 120 s startup grace so grace-suppression of low-confidence
/// questions, rate limits and API errors does not mask what the chrome
/// cutoff decided. Mirrors `test_startup_grace_safety_cap`.
fn settle_startup_grace(silence: &Arc<Mutex<SilenceState>>) {
    let mut sl = silence.lock();
    sl.created_at =
        std::time::Instant::now() - STARTUP_GRACE_MAX - std::time::Duration::from_secs(1);
    sl.last_output_at = std::time::Instant::now();
    sl.check_startup_settle();
    assert!(!sl.is_startup_grace(), "startup grace must be settled");
}

fn shell_of(state: &AppState, sid: &str) -> u8 {
    state
        .session_maps
        .shell_states
        .get(sid)
        .unwrap()
        .load(Ordering::Acquire)
}

/// Catches: `finish_session_tasks` dropping the `error` message of a failed exit
/// or the `result` payload of a clean one, so a polled task shows no outcome.
#[test]
fn session_exit_records_task_error_or_result_from_the_exit_code() {
    use crate::tasks::{TaskKind, TaskStatus};
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let cases: [(&str, Option<i32>); 3] =
        [("failed", Some(3)), ("clean", Some(0)), ("unknown", None)];
    for (sid, code) in cases {
        let task = state
            .tasks
            .create(TaskKind::AgentSpawn, "orchestrator", Some(sid));
        if let Some(code) = code {
            state.session_maps.exit_codes.insert(sid.to_string(), code);
        }
        mark_session_exited(sid, &state);
        let rec = state.tasks.get(&task).expect("task");
        if code.is_some_and(|c| c != 0) {
            assert_eq!(rec.status, TaskStatus::Failed, "{sid}");
            assert_eq!(
                rec.error.as_deref(),
                Some("agent session exited with code 3")
            );
            assert_eq!(rec.result, None);
        } else {
            assert_eq!(rec.status, TaskStatus::Completed, "{sid}");
            assert_eq!(
                rec.result,
                Some(serde_json::json!({ "session_id": sid, "exit_code": code })),
                "{sid}"
            );
            assert_eq!(rec.error, None);
        }
    }
}

#[cfg(unix)]
impl std::io::Write for WorkingOnEnterWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.state
            .grid
            .vt_log_buffers
            .get(self.sid)
            .unwrap()
            .lock()
            .process(
                b"\x1b[2J\x1b[H\xe2\x80\xa2 Working (2s \xe2\x80\xa2 esc to interrupt)\r\n\r\n\xe2\x80\xba run the next step please\r\n\r\n  gpt-5 \xc2\xb7 ~/repo",
            );
        self.state
            .session_maps
            .output_buffers
            .get(self.sid)
            .unwrap()
            .lock()
            .write(b"working");
        match self.fault {
            RetryWriterFault::Write => Err(std::io::Error::other("write failed")),
            _ => Ok(bytes.len()),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self.fault {
            RetryWriterFault::Flush => Err(std::io::Error::other("flush failed")),
            _ => Ok(()),
        }
    }
}
