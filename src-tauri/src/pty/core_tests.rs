#[test]
fn agent_alternate_screen_warning_is_once_per_session() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "agent-alt-screen-warning";
    agent_session(&state, session_id, SHELL_BUSY);
    state
        .session_maps
        .session_states
        .get_mut(session_id)
        .unwrap()
        .agent_type = Some("codex".into());
    state.grid.vt_log_buffers.insert(
        session_id.to_string(),
        Mutex::new(VtLogBuffer::new(24, 80, 1000)),
    );
    let silence = state
        .session_maps
        .silence_states
        .get(session_id)
        .unwrap()
        .clone();
    let mut processor = ChunkProcessor::new(None, None);
    assert!(!processor.alt_screen_warned);
    processor.process_chunk("\x1b[?1049h", &silence, session_id, &state);
    assert!(processor.alt_screen_warned);
    processor.process_chunk("\x1b[?1049l\x1b[?1049h", &silence, session_id, &state);
    assert!(processor.alt_screen_warned);
    assert!(!processor.should_warn_alt_screen(None, true));
    assert!(!processor.should_warn_alt_screen(Some("codex"), false));
    assert!(!processor.should_warn_alt_screen(Some("codex"), true));
}

/// Feed `chunks` to a fresh processor for `session_id` and return the alt-screen
/// toasts the bus carried, as (title, message, origin_session_id).
fn alt_screen_toasts(
    session_id: &str,
    agent_type: Option<&str>,
    chunks: &[&str],
    startup_elapsed: std::time::Duration,
) -> Vec<(String, String, Option<String>)> {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, session_id, SHELL_BUSY);
    state
        .session_maps
        .session_states
        .get_mut(session_id)
        .unwrap()
        .agent_type = agent_type.map(String::from);
    state.grid.vt_log_buffers.insert(
        session_id.to_string(),
        Mutex::new(VtLogBuffer::new(24, 80, 1000)),
    );
    let silence = state
        .session_maps
        .silence_states
        .get(session_id)
        .unwrap()
        .clone();
    let mut processor = ChunkProcessor::new(None, None);
    processor.startup_deadline -= startup_elapsed;
    let mut events = state.event_bus.subscribe();
    for chunk in chunks {
        processor.process_chunk(chunk, &silence, session_id, &state);
    }
    std::iter::from_fn(|| events.try_recv().ok())
        .filter_map(|event| match event {
            crate::state::AppEvent::McpToast {
                title,
                message,
                origin_session_id,
                ..
            } => Some((title, message.unwrap_or_default(), origin_session_id)),
            _ => None,
        })
        .collect()
}

#[test]
fn startup_alt_screen_entry_toasts_once_naming_the_session() {
    for enter in ["\x1b[?1049h", "\x1b[?47h", "\x1b[?1047h"] {
        let toasts = alt_screen_toasts(
            "startup-alt-toast",
            Some("claude"),
            &[enter, "\x1b[?1049l\x1b[?1049h"],
            std::time::Duration::ZERO,
        );
        assert_eq!(toasts.len(), 1, "{enter:?}: one toast per session");
        let (title, message, origin) = &toasts[0];
        assert!(title.contains("alternate screen"), "{enter:?}: {title}");
        assert!(
            message.contains("startup-alt-toast"),
            "{enter:?}: {message}"
        );
        assert_eq!(origin.as_deref(), Some("startup-alt-toast"));
    }
}

#[test]
fn startup_alt_screen_toast_covers_plain_shells() {
    let toasts = alt_screen_toasts(
        "startup-alt-shell",
        None,
        &["\x1b[?1049h"],
        std::time::Duration::ZERO,
    );
    assert_eq!(toasts.len(), 1);
}

#[test]
fn alt_screen_after_the_startup_window_does_not_toast() {
    let toasts = alt_screen_toasts(
        "late-alt-toast",
        None,
        &["\x1b[?1049h"],
        std::time::Duration::from_secs(3600),
    );
    assert!(
        toasts.is_empty(),
        "vim/less later in the session: {toasts:?}"
    );
}

#[test]
fn alt_screen_entered_and_left_in_one_chunk_does_not_toast() {
    let toasts = alt_screen_toasts(
        "probe-alt-toast",
        Some("codex"),
        &["\x1b[?1049h\x1b[?1049l"],
        std::time::Duration::ZERO,
    );
    assert!(toasts.is_empty(), "transient probe: {toasts:?}");
}

#[test]
fn alt_screen_toast_text_names_the_fix_per_agent() {
    let claude = alt_screen_toast_message("s", Some("claude"));
    assert!(claude.contains("CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN"));
    assert!(claude.contains("CLAUDE_CODE_DISABLE_AGENT_VIEW"));
    assert!(alt_screen_toast_message("s", Some("codex")).contains("--no-alt-screen"));
    assert!(alt_screen_toast_message("s", Some("opencode")).contains("--mini"));
    for agent in [None, Some("aider")] {
        let text = alt_screen_toast_message("s", agent);
        assert!(text.contains("scrollback"), "{agent:?}: {text}");
        assert!(text.contains("state detection"), "{agent:?}: {text}");
    }
}

/// Closing a workspace's terminals is a loop over `close_pty`, and its body
/// waits on two 100 ms `sleep` deadlines per session before it may also delete
/// a worktree. As a plain `fn` command that ran inline on the IPC thread — the
/// macOS main thread — so the post-merge cleanup dialog froze the WebView for
/// the sum of those waits. See `docs/backend/command-threading.md`.
#[test]
fn close_pty_never_runs_on_the_ipc_thread() {
    let source = include_str!("commands.rs");
    let signature = "pub(crate) async fn close_pty(";
    let at = source
        .find(signature)
        .expect("close_pty must be async: a plain fn command runs on the macOS main thread");
    let body = &source[at..(at + 600).min(source.len())];
    assert!(
        body.contains("spawn_blocking"),
        "close_pty waits on process exit and must hand that wait to the blocking pool"
    );
}

fn fixture_rows(fixture: &str) -> Vec<String> {
    fixture
        .trim_end_matches('\n')
        .lines()
        .map(str::to_string)
        .collect()
}

/// The interactive-path threads raise their QoS to USER_INTERACTIVE. Verify
/// the syscall actually takes effect by reading the class back on the same
/// thread (default QoS for a fresh test thread is *not* USER_INTERACTIVE).
#[cfg(target_os = "macos")]
#[test]
fn raises_thread_to_user_interactive_qos() {
    // Run on a dedicated thread so we don't leave the test runner's worker
    // permanently bumped.
    let observed = std::thread::spawn(|| {
        raise_thread_for_interactive_io();
        thread_qos::current_qos_class()
    })
    .join()
    .expect("qos probe thread panicked");
    // QOS_CLASS_USER_INTERACTIVE == 0x21.
    assert_eq!(
        observed, 0x21,
        "thread QoS was not raised to USER_INTERACTIVE"
    );
}

/// A keystroke borrows a thread from the shared tokio blocking pool and gives
/// it back. Bumping that thread's QoS without putting it back promotes the
/// pool itself: the next git walk, content-index build, or config write to
/// land on that thread runs in the interactive band forever after — the one
/// band the keystroke path needs kept clear.
#[cfg(target_os = "macos")]
#[test]
fn a_keystroke_gives_the_shared_blocking_thread_back_at_its_original_qos() {
    // The whole pair, not just the class: restoring the band while dropping
    // the relative priority is still handing back a thread that is not the
    // one we borrowed, and a class-only assertion cannot see it.
    let (before, during, after) = std::thread::spawn(|| {
        let before = thread_qos::current_qos_pair();
        let during = {
            let _boost = interactive_io_boost();
            thread_qos::current_qos_pair()
        };
        (before, during, thread_qos::current_qos_pair())
    })
    .join()
    .expect("qos probe thread panicked");
    // QOS_CLASS_USER_INTERACTIVE == 0x21.
    assert_eq!(
        during.0, 0x21,
        "keystroke did not run in the interactive band"
    );
    assert_ne!(before.0, 0x21, "probe thread started already bumped");
    assert_eq!(
        after, before,
        "the blocking-pool thread stayed promoted after the keystroke"
    );
}

/// The restore is a `Drop`, so the path that matters most is the one nobody
/// writes on purpose: a panic inside the write. `spawn_blocking` catches it
/// and returns the thread to the pool either way, so a boost that only
/// unwound on the happy path would promote the pool exactly when something
/// is already going wrong.
#[cfg(target_os = "macos")]
#[test]
fn a_panicking_keystroke_still_gives_the_thread_back_at_its_original_qos() {
    let (before, after) = std::thread::spawn(|| {
        let before = thread_qos::current_qos_pair();
        let panicked = std::panic::catch_unwind(|| {
            let _boost = interactive_io_boost();
            panic!("write failed mid-keystroke");
        });
        assert!(panicked.is_err(), "the probe did not actually panic");
        (before, thread_qos::current_qos_pair())
    })
    .join()
    .expect("qos probe thread panicked");
    assert_ne!(before.0, 0x21, "probe thread started already bumped");
    assert_eq!(
        after, before,
        "a panicking keystroke left the blocking-pool thread promoted"
    );
}

/// The reader thread runs its whole body inside `catch_unwind`, and the
/// `running.store(false)` that stops the frame ticker and the 1 Hz silence
/// timer sits INSIDE that closure. A panic skips it: the ticker keeps waking
/// ~62 times a second and the tokio timer keeps ticking for the life of the
/// process, and the ticker never reaches the code after its loop that removes
/// the session's `grid_frame_dirty` / `sync_update_active` entries.
///
/// Liveness is read off the two things each loop owns: the ticker removes its
/// map entries only after the loop ends, and the silence timer holds a clone
/// of the `SilenceState` Arc that teardown drops from `silence_states`.
#[tokio::test(flavor = "current_thread", start_paused = false)]
async fn a_reader_panic_stops_the_ticker_and_the_silence_timer() {
    const PAYLOAD: &str = "simulated PTY reader panic";

    struct PanicOnRead(Arc<AtomicBool>);
    impl Read for PanicOnRead {
        fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
            while !self.0.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            panic!("{PAYLOAD}");
        }
    }

    // The injected panic is the fixture, not a failure. Swallow that one
    // payload so the suite prints no stray backtrace, record that it fired so
    // the test still proves the panic path ran, and delegate anything else.
    let observed = Arc::new(AtomicBool::new(false));
    let seen = observed.clone();
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if info
            .payload()
            .downcast_ref::<String>()
            .is_some_and(|s| s == PAYLOAD)
        {
            seen.store(true, Ordering::Relaxed);
        } else {
            previous(info);
        }
    }));

    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "reader-panic-session".to_string();
    let detonate = Arc::new(AtomicBool::new(false));
    spawn_reader_thread(
        Box::new(PanicOnRead(detonate.clone())),
        Arc::new(AtomicBool::new(false)),
        sid.clone(),
        state.clone(),
        None,
    );

    // Take the handle BEFORE the panic: teardown removes the map entry, so
    // afterwards the only clones left are this one and the timer's.
    let silence = state
        .session_maps
        .silence_states
        .get(&sid)
        .map(|e| Arc::clone(e.value()))
        .expect("silence state is registered before the threads start");
    assert!(
        state.grid.frame_dirty.contains_key(&sid)
            && state.grid.sync_update_active.contains_key(&sid),
        "precondition: the ticker owns both per-session entries"
    );

    detonate.store(true, Ordering::Relaxed);

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let ticker_alive = state.grid.frame_dirty.contains_key(&sid)
            || state.grid.sync_update_active.contains_key(&sid);
        let timer_alive = Arc::strong_count(&silence) > 1;
        if !ticker_alive && !timer_alive {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "reader panic leaked: ticker still live={ticker_alive}, \
                 1 Hz timer still live={timer_alive} ({} silence refs)",
            Arc::strong_count(&silence)
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    assert!(
        observed.load(Ordering::Relaxed),
        "the injected reader panic never fired — the test proved nothing"
    );
    let _ = std::panic::take_hook();
}

/// `pending_scroll` is the target `terminal_scroll_to_offset` writes on both
/// transports and the ticker consumes. It used to be created by
/// `subscribe_terminal_grid`, a desktop-only Tauri command, so a session nothing
/// desktop had ever rendered had no entry to write to at all — the wheel and the
/// scrollbar drag in browser mode wrote nowhere and answered ok. It belongs to
/// the session, like the `grid_frame_dirty` flag the same handler sets, and the
/// only funnel every creation path shares is `spawn_reader_thread`.
#[tokio::test(flavor = "current_thread", start_paused = false)]
async fn a_session_owns_its_pending_scroll_entry_from_the_start() {
    struct EofReader;
    impl Read for EofReader {
        fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
            Ok(0)
        }
    }

    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "pending-scroll-owner".to_string();
    spawn_reader_thread(
        Box::new(EofReader),
        Arc::new(AtomicBool::new(false)),
        sid.clone(),
        state.clone(),
        None,
    );

    assert!(
        state.grid.pending_scroll.contains_key(&sid),
        "a session with no desktop grid subscriber has nowhere to record a scroll"
    );
}

#[test]
fn grid_send_min_interval_policy() {
    // Short burst, no typing → no floor: full-speed for low latency.
    assert_eq!(grid_send_min_interval_ms(false, 0), 0);
    assert_eq!(grid_send_min_interval_ms(false, 5), 0);
    // Sustained animation (dirty ≥ 6 ticks), no typing → ~30 fps floor.
    assert_eq!(grid_send_min_interval_ms(false, 6), 33);
    assert_eq!(grid_send_min_interval_ms(false, 1000), 33);
    // Typing under load → ~20 fps floor, regardless of dirty_run (even a
    // short burst), because keystroke latency is what we protect.
    assert_eq!(grid_send_min_interval_ms(true, 0), 50);
    assert_eq!(grid_send_min_interval_ms(true, 1000), 50);
    // Typing floor must be the more aggressive (larger interval) of the two.
    assert!(grid_send_min_interval_ms(true, 1000) > grid_send_min_interval_ms(false, 1000));
}

#[test]
fn system_load_per_core_is_non_negative_and_finite() {
    // Links libc getloadavg/sysconf on Unix; returns 0.0 elsewhere. Either
    // way it must be a sane, non-negative, finite ratio.
    let v = system_load_per_core();
    assert!(v.is_finite());
    assert!(v >= 0.0);
}

#[test]
fn clean_action_required_title_strips_marker_spinner_and_separators() {
    // Real grok permission-prompt title (captured live).
    assert_eq!(
        clean_action_required_title(
            "⚠ Action Required - ⠙ - Running: echo hello - Execute Shell Command"
        ),
        "Running: echo hello - Execute Shell Command"
    );
}

#[test]
fn clean_action_required_title_handles_each_spinner_frame() {
    // Title repaints with a different braille frame each tick; cleaned output is stable.
    for frame in ["⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇"] {
        assert_eq!(
            clean_action_required_title(&format!("⚠ Action Required - {frame} - Running: ls")),
            "Running: ls"
        );
    }
}

#[test]
fn clean_action_required_title_fallback_when_empty() {
    assert_eq!(
        clean_action_required_title("⚠ Action Required - ⠙ - "),
        "grok is awaiting approval"
    );
}

#[test]
fn test_parse_osc133_exit_code() {
    assert_eq!(parse_osc133_exit_code('D', "0"), Some(0));
    assert_eq!(parse_osc133_exit_code('D', "127"), Some(127));
    assert_eq!(parse_osc133_exit_code('D', ""), None);
    assert_eq!(parse_osc133_exit_code('A', "0"), None);
}

/// Catches: OSC 7770 from ego being ignored for question suppression because the
/// user never set `hook_instrumentation` — ego has no hook to configure.
#[test]
fn test_ego_is_always_hook_instrumented() {
    let agents = crate::config::AgentsConfig::default();
    assert!(hook_instrumented_for(&agents, Some("ego")));
}

// --- parse_osc7_cwd tests (story 1558-81bb) ---

#[test]
fn osc7_simple_path() {
    assert_eq!(
        parse_osc7_cwd("file://hostname/Users/me"),
        Ok("/Users/me".into())
    );
}

#[test]
fn osc7_empty_hostname() {
    assert_eq!(parse_osc7_cwd("file:///home/user"), Ok("/home/user".into()));
}

#[test]
fn osc7_localhost() {
    assert_eq!(
        parse_osc7_cwd("file://localhost/tmp/foo"),
        Ok("/tmp/foo".into())
    );
}

#[test]
fn osc7_trailing_slash_stripped() {
    assert_eq!(
        parse_osc7_cwd("file:///home/user/"),
        Ok("/home/user".into())
    );
}

#[test]
fn osc7_root_path_preserved() {
    assert_eq!(parse_osc7_cwd("file:///"), Ok("/".into()));
}

#[test]
fn osc7_percent_encoded_space() {
    assert_eq!(
        parse_osc7_cwd("file:///home/user/my%20project"),
        Ok("/home/user/my project".into()),
    );
}

#[test]
fn osc7_percent_encoded_special_chars() {
    assert_eq!(
        parse_osc7_cwd("file:///tmp/%E2%9C%93"),
        Ok("/tmp/\u{2713}".into()),
    );
}

#[test]
fn osc7_missing_scheme() {
    assert!(parse_osc7_cwd("/home/user").is_err());
}

#[test]
fn osc7_wrong_scheme() {
    assert!(parse_osc7_cwd("http://localhost/foo").is_err());
}

#[test]
fn osc7_invalid_percent_encoding() {
    assert!(parse_osc7_cwd("file:///home/%GG").is_err());
}

// --- SilenceState tests ---

#[test]
fn test_silence_state_no_pending_returns_none() {
    let mut s = SilenceState::new();
    assert!(s.check_silence().is_none());
}

#[test]
fn test_tool_error_no_candidate_returns_none() {
    let mut s = SilenceState::new();
    assert!(s.check_tool_error().is_none());
}

#[test]
fn test_tool_error_fires_after_silence_threshold() {
    let mut s = SilenceState::new();
    s.mark_tool_error_candidate("Error: Exit code 128".to_string());
    // Force last_output_at past the threshold to simulate silence.
    s.last_output_at = std::time::Instant::now()
        - SILENCE_TOOL_ERROR_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert_eq!(
        s.check_tool_error(),
        Some("Error: Exit code 128".to_string())
    );
    // Dedup: second call returns None (already emitted).
    assert!(s.check_tool_error().is_none());
}

#[test]
fn test_tool_error_recovery_clears_candidate() {
    let mut s = SilenceState::new();
    s.mark_tool_error_candidate("Error: Exit code 1".to_string());
    s.clear_tool_error_on_recovery();
    s.last_output_at = std::time::Instant::now()
        - SILENCE_TOOL_ERROR_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert!(
        s.check_tool_error().is_none(),
        "recovery must clear pending tool error"
    );
}

#[test]
fn test_tool_error_does_not_refire_same_line_after_recovery() {
    // Reproduces the scroll-induced re-fire bug: once an error has been
    // surfaced, scrolling the Ink TUI viewport re-introduces the error line
    // in `changed_rows`. `clear_tool_error_on_recovery` must NOT re-enable
    // notification for a line the user already saw.
    let mut s = SilenceState::new();
    s.mark_tool_error_candidate("Error: Exit code 1".to_string());
    s.last_output_at = std::time::Instant::now()
        - SILENCE_TOOL_ERROR_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert_eq!(
        s.check_tool_error(),
        Some("Error: Exit code 1".to_string()),
        "first occurrence must fire"
    );

    // Agent produced real output → recovery.
    s.clear_tool_error_on_recovery();

    // Viewport scrolls, same error line reappears in changed_rows.
    s.mark_tool_error_candidate("Error: Exit code 1".to_string());
    s.last_output_at = std::time::Instant::now()
        - SILENCE_TOOL_ERROR_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert!(
        s.check_tool_error().is_none(),
        "same error line must not refire after recovery (scroll-induced)"
    );
}

#[test]
fn test_tool_error_refires_after_memory_reset() {
    // After the user submits a line (explicit re-engagement), a recurrence
    // of the same failure in a new turn must notify again.
    let mut s = SilenceState::new();
    s.mark_tool_error_candidate("Error: Exit code 1".to_string());
    s.last_output_at = std::time::Instant::now()
        - SILENCE_TOOL_ERROR_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert!(s.check_tool_error().is_some());

    s.reset_tool_error_memory();

    s.mark_tool_error_candidate("Error: Exit code 1".to_string());
    s.last_output_at = std::time::Instant::now()
        - SILENCE_TOOL_ERROR_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert_eq!(
        s.check_tool_error(),
        Some("Error: Exit code 1".to_string()),
        "after user input, same error text must be allowed to notify again"
    );
}

#[test]
fn test_is_retry_line_matches_connection_retries() {
    // Claude subagent SDK retry loop (the reported false-idle scenario).
    assert!(is_retry_line(
        "  Unable to connect to API (ECONNRESET) · Retrying in 0s · attempt 6/10"
    ));
    assert!(is_retry_line(
        "Teammate @spinach-mail-validate failed: API Error: Unable to connect to API (ConnectionRefused)"
    ));
    // Goose/Aider stream-error retry form.
    assert!(is_retry_line(
        "⚠  stream error: exceeded retry limit, last status: 401; retrying 5/5 in 3s…"
    ));
    // Non-retry prose / code must NOT latch busy — the N/M counter is required.
    assert!(!is_retry_line(
        "I'll be retrying the request in a moment if it fails."
    ));
    assert!(!is_retry_line(
        "let retrying = true; // attempt to reconnect"
    ));
    assert!(!is_retry_line(
        "Successfully connected to the API endpoint."
    ));
}

#[test]
fn test_api_retry_hold_active_then_expires() {
    let mut s = SilenceState::new();
    assert!(!s.is_api_retry_active(), "no hold armed initially");
    s.mark_api_retry();
    assert!(s.is_api_retry_active(), "hold active right after arming");
    // Simulate the hold window elapsing.
    s.api_retry_hold_until = Some(std::time::Instant::now() - std::time::Duration::from_millis(1));
    assert!(
        !s.is_api_retry_active(),
        "hold self-expires after AGENT_RETRY_HOLD"
    );
}

#[test]
fn test_api_retry_hold_cleared_on_recovery_and_user_input() {
    let mut s = SilenceState::new();
    s.mark_api_retry();
    // Real non-error output → agent recovered.
    s.clear_tool_error_on_recovery();
    assert!(!s.is_api_retry_active(), "recovery releases the retry hold");

    s.mark_api_retry();
    // User re-engages (submitted a line / Ctrl+C).
    s.reset_tool_error_memory();
    assert!(
        !s.is_api_retry_active(),
        "user input releases the retry hold"
    );
}

// --- is_tool_error_line tests ---

#[test]
fn test_tool_error_matches_claude_code_format() {
    // Claude Code prefixes tool-result rows with `⎿ `.
    assert!(is_tool_error_line("⎿  Error: Exit code 1"));
    assert!(is_tool_error_line("  ⎿  Error: Exit code 127"));
}

#[test]
fn test_tool_error_matches_bare_format() {
    assert!(is_tool_error_line("Error: Exit code 1"));
    assert!(is_tool_error_line("  Error: Exit code 128"));
}

#[test]
fn test_tool_error_rejects_source_code_literal() {
    // Exact string that triggered the false-positive in Boss's session:
    // the test file's own content displayed in a terminal armed a red
    // notification because the unanchored regex matched inside a string
    // literal. These must never fire.
    assert!(!is_tool_error_line(
        r#"s.mark_tool_error_candidate("Error: Exit code 2".to_string());"#
    ));
    assert!(!is_tool_error_line(
        r#"assert_eq!(s.check_tool_error(), Some("Error: Exit code 1".to_string()));"#
    ));
    assert!(!is_tool_error_line(
        r#"3895          s.mark_tool_error_candidate("Error: Exit code 2".to_string"#
    ));
}

#[test]
fn test_tool_error_rejects_markdown_mention() {
    // Commit messages, docs, release notes that quote the error text.
    assert!(!is_tool_error_line(
        r#"fix: resolve "Error: Exit code 1" in claude tool pipeline"#
    ));
}

#[test]
fn test_tool_error_allows_box_drawing_variations() {
    // Other box-drawing chars Claude uses for tool-call hierarchy rows.
    assert!(is_tool_error_line("╰  Error: Exit code 2"));
    assert!(is_tool_error_line("│  Error: Exit code 5"));
}

#[test]
fn test_suggest_drain_consumes_items() {
    let mut s = SilenceState::new();
    s.mark_suggest_candidate(vec!["a".to_string()], 0);
    let _ = s.drain_pending_suggest();
    assert!(
        s.drain_pending_suggest().is_none(),
        "second drain must return None — single-shot semantics"
    );
}

#[test]
fn test_suggest_drain_none_when_nothing_parked() {
    let mut s = SilenceState::new();
    assert!(s.drain_pending_suggest().is_none());
}

#[test]
fn test_suggest_reset_on_user_input() {
    let mut s = SilenceState::new();
    s.mark_suggest_candidate(vec!["stale".to_string()], 0);
    s.reset_suggest_memory();
    assert!(
        s.drain_pending_suggest().is_none(),
        "user input must drop pending suggest so it doesn't fire across turns"
    );
}

#[test]
fn test_silence_state_pending_but_too_soon() {
    let mut s = SilenceState::new();
    s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
    // Just set — not enough time has passed
    assert!(s.check_silence().is_none());
}

#[test]
fn test_silence_state_no_double_emission() {
    let mut s = SilenceState::new();
    s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert!(s.check_silence().is_some());
    // Second check should return None (already emitted)
    assert!(s.check_silence().is_none());
}

#[test]
fn test_silence_state_regex_suppresses_timer() {
    let mut s = SilenceState::new();
    // regex_found_question = true means instant detection already fired
    s.on_chunk(
        true,
        Some("Would you like to proceed?".to_string()),
        false,
        false,
        false,
    );
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert!(s.check_silence().is_none());
}

#[test]
fn test_silence_state_suppress_user_input() {
    let mut s = SilenceState::new();
    // User types a line ending with `?` — PTY will echo it back
    s.on_chunk(
        false,
        Some("c'è ancora una storia?".to_string()),
        false,
        false,
        false,
    );
    // write_pty detects user input and suppresses
    s.suppress_user_input();
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    // Should NOT fire — the question was typed by the user
    assert!(s.check_silence().is_none());
}

#[test]
fn test_silence_state_suppress_echo_after_user_input() {
    let mut s = SilenceState::new();
    // write_pty detects user input and suppresses BEFORE the echo arrives
    s.suppress_user_input();
    // PTY echoes the user's text back — this should NOT re-enable detection
    s.on_chunk(
        false,
        Some("lo hai mai provato?".to_string()),
        false,
        false,
        false,
    );
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    // Should NOT fire — the echo window blocks re-enabling
    assert!(
        s.check_silence().is_none(),
        "PTY echo after suppress should not trigger question detection"
    );
}

// --- is_chrome_row / chrome_only classification tests ---

#[test]
fn test_chrome_only_empty_changed_rows_is_chrome() {
    // Empty changed_rows means the chunk produced no visible change
    // (cursor blink, OSC title update, mouse report). It must count as
    // chrome-only so periodic re-emits don't latch the shell to busy.
    let rows: Vec<ChangedRow> = vec![];
    assert!(
        compute_chrome_only(&rows, false, false, false),
        "empty changed_rows should be chrome_only (no real output)"
    );
}

#[test]
fn test_chrome_only_plain_text_is_not_chrome() {
    let rows = make_rows(&["I will edit the file for you."]);
    let chrome_only = !rows.is_empty() && rows.iter().all(|r| is_chrome_row(&r.text));
    assert!(
        !chrome_only,
        "plain text without chrome markers is not chrome"
    );
}

#[test]
fn test_chrome_only_statusline_with_text_rows_is_not_chrome() {
    let rows = make_rows(&[
        "\u{23F5}\u{23F5} auto mode",
        "Here is the code change:",
        "  fn main() {",
        "    println!(\"hello\");",
    ]);
    let chrome_only = !rows.is_empty() && rows.iter().all(|r| is_chrome_row(&r.text));
    assert!(!chrome_only, "mode-line + text rows should not be chrome");
}

#[test]
fn test_chrome_only_single_statusline_row_is_chrome() {
    let rows = make_rows(&["\u{23F5}\u{23F5} auto mode"]);
    let chrome_only = !rows.is_empty() && rows.iter().all(|r| is_chrome_row(&r.text));
    assert!(chrome_only, "single mode-line row should be chrome");
}

#[test]
fn test_chrome_only_wrapped_statusline_is_chrome() {
    let rows = make_rows(&[
        "\u{23F5}\u{23F5} bypass permissions on",
        "\u{273B} Cogitated 3m 47s",
    ]);
    let chrome_only = !rows.is_empty() && rows.iter().all(|r| is_chrome_row(&r.text));
    assert!(chrome_only, "wrapped mode-line rows should all be chrome");
}

#[test]
fn test_chrome_only_subtasks_row_is_chrome() {
    let rows = make_rows(&["\u{203A}\u{203A} bypass permissions on \u{00B7} 1 local agent"]);
    let chrome_only = !rows.is_empty() && rows.iter().all(|r| is_chrome_row(&r.text));
    assert!(chrome_only, "subtask mode-line row should be chrome");
}

#[test]
fn test_chrome_only_codex_spinner_is_chrome() {
    let rows = make_rows(&["\u{2022} Boot"]);
    let chrome_only = !rows.is_empty() && rows.iter().all(|r| is_chrome_row(&r.text));
    assert!(chrome_only, "Codex spinner row should be chrome");
}

#[test]
fn test_chrome_only_gemini_braille_spinner_is_chrome() {
    // Gemini braille spinner chars (U+2800-28FF) are now in is_chrome_row
    let rows = make_rows(&["\u{280B} Connecting to MCP servers..."]);
    let chrome_only = !rows.is_empty() && rows.iter().all(|r| is_chrome_row(&r.text));
    assert!(chrome_only, "Gemini braille spinner should be chrome");
}

#[test]
fn test_chrome_only_tool_progress_spinner_is_chrome() {
    let rows = make_rows(&["\u{25D0} Bash: .../b... | \u{2713} Bash \u{00D7}9"]);
    let chrome_only = !rows.is_empty() && rows.iter().all(|r| is_chrome_row(&r.text));
    assert!(chrome_only, "CC tool progress spinner should be chrome");
    assert!(
        crate::chrome::is_spinner_row(&rows[0].text),
        "CC tool progress spinner should be detected as spinner (keepalive)"
    );
}

// --- chrome_only full formula tests (mirrors process_chunk logic) ---

/// Helper: compute chrome_only using the same formula as process_chunk.
fn compute_chrome_only(
    rows: &[ChangedRow],
    has_status_line: bool,
    regex_found_question: bool,
    last_q_line: bool,
) -> bool {
    let all_chrome_markers = rows.iter().all(|r| is_chrome_row(&r.text));
    let no_real_output = rows.iter().all(|r| {
        is_chrome_row(&r.text)
            || r.text.trim().is_empty()
            || crate::chrome::is_separator_line(&r.text)
            || crate::chrome::is_prompt_line(&r.text)
    });
    !regex_found_question
        && !last_q_line
        && (rows.is_empty() || all_chrome_markers || (has_status_line && no_real_output))
}

#[test]
fn test_chrome_only_formula_timer_tick_only() {
    // CC timer tick: only the timer row changed
    let rows = make_rows(&["\u{273B} Cogitated 3m 47s"]);
    assert!(
        compute_chrome_only(&rows, true, false, false),
        "timer-only tick should be chrome_only"
    );
}

#[test]
fn test_chrome_only_formula_timer_plus_separator() {
    // CC timer tick + separator repaint (ESC[2J full redraw)
    let rows = make_rows(&[
        "────────────────────────────────────",
        "\u{273B} Cogitated 3m 48s",
    ]);
    assert!(
        compute_chrome_only(&rows, true, false, false),
        "timer + separator should be chrome_only"
    );
}

#[test]
fn test_chrome_only_formula_timer_plus_prompt_and_separator() {
    // CC timer tick + prompt + separator (full bottom chrome zone)
    let rows = make_rows(&[
        "────────────────────────────────────",
        "❯",
        "────────────────────────────────────",
        "\u{23F5}\u{23F5} auto mode",
        "\u{273B} Cogitated 3m 48s",
    ]);
    assert!(
        compute_chrome_only(&rows, true, false, false),
        "timer + prompt + separator + mode-line should be chrome_only"
    );
}

#[test]
fn test_chrome_only_formula_timer_plus_blank_rows() {
    // CC timer tick with blank rows (padding in TUI)
    let rows = make_rows(&["", "\u{273B} Cogitated 3m 48s", ""]);
    assert!(
        compute_chrome_only(&rows, true, false, false),
        "timer + blank rows should be chrome_only"
    );
}

#[test]
fn test_chrome_only_formula_real_output_not_chrome() {
    // Real agent output mixed with status line
    let rows = make_rows(&["I will edit the file for you.", "\u{273B} Cogitated 3m 48s"]);
    assert!(
        !compute_chrome_only(&rows, true, false, false),
        "real text + timer should NOT be chrome_only"
    );
}

#[test]
fn test_chrome_only_formula_question_line_not_chrome() {
    // Even if all chrome, a pending question line disables chrome_only
    let rows = make_rows(&["\u{273B} Cogitated 3m 48s"]);
    assert!(
        !compute_chrome_only(&rows, true, false, true),
        "chrome with pending question should NOT be chrome_only"
    );
}

// --- Spinner → busy gate tests (mirrors process_chunk transition logic) ---

/// The busy transition gate `(!chrome_only || has_spinner)` must be true
/// when changed_rows contain an active spinner, even when chrome_only is true.
#[test]
fn test_spinner_only_chunk_can_trigger_busy() {
    let rows = make_rows(&["\u{2022} Working (1m 31s \u{2022} esc to interrupt)"]);
    let chrome_only = compute_chrome_only(&rows, false, false, false);
    let has_spinner = chrome_only && rows.iter().any(|r| crate::chrome::is_spinner_row(&r.text));
    assert!(chrome_only, "Codex spinner is chrome_only");
    assert!(has_spinner, "Codex spinner is detected as spinner");
    assert!(
        !chrome_only || has_spinner,
        "spinner-only chunk must pass the busy transition gate"
    );
}

#[test]
fn test_static_chrome_cannot_trigger_busy() {
    let rows = make_rows(&["\u{23F5}\u{23F5} auto mode"]);
    let chrome_only = compute_chrome_only(&rows, false, false, false);
    let has_spinner = chrome_only && rows.iter().any(|r| crate::chrome::is_spinner_row(&r.text));
    assert!(chrome_only, "mode-line is chrome_only");
    assert!(!has_spinner, "mode-line is NOT a spinner");
    assert!(
        chrome_only && !has_spinner,
        "static chrome must NOT pass the busy transition gate"
    );
}

/// Build ChangedRows for a subset of a full screen, mirroring how the VT
/// reader reports only the rows a chunk actually repainted (`row_index`
/// preserved against the full screen).
fn changed_at(screen: &[&str], indices: &[usize]) -> Vec<ChangedRow> {
    indices
        .iter()
        .map(|&i| ChangedRow {
            row_index: i,
            text: screen[i].to_string(),
        })
        .collect()
}

/// Mirror process_chunk's chrome-cutoff filter (pty.rs ~1996): drop changed
/// rows at or below the footer cutoff, keeping only the content zone.
fn filter_below_cutoff(screen: &[&str], changed: Vec<ChangedRow>) -> Vec<ChangedRow> {
    if changed.is_empty() {
        return changed;
    }
    match crate::chrome::find_chrome_cutoff(screen) {
        Some(cutoff) => changed
            .into_iter()
            .filter(|r| r.row_index < cutoff)
            .collect(),
        None => changed,
    }
}

/// Regression for the false-busy "flap" (root cause + agnostic fix,
/// 2026-06-17). Claude Code repaints its whole input area periodically.
/// Positionally, EVERYTHING below the second separator of the input box is
/// chrome — status bar, mode line, usage gauge — regardless of its glyphs.
/// The user's custom HUD renders a context gauge (`█░`) and a mode line
/// (`·`) there; `is_spinner_row` reads those glyphs as a live spinner.
///
/// The fix is positional, not glyph-based: spinner keepalive runs on the
/// SAME post-cutoff `changed_rows` as everything else. A repaint that only
/// touches footer rows below the cutoff yields an EMPTY post-filter set →
/// chrome_only with no spinner → no busy transition. Agnostic to whatever
/// the user puts in their status bar. These are the exact rows captured
/// from the live flapping instance.
#[test]
fn test_statusbar_repaint_below_cutoff_does_not_flap_busy() {
    let sep = "────────────────────────────────────────────────────────────────────────";
    let screen: Vec<&str> = vec![
        "Here is the answer to your question.",
        "",
        sep,
        "❯",
        sep,
        "[Opus 4.8 (1M) | Team] █░░░░░░░░░ 6% | gh-metrics git:(master)",
        "5h: 21% (50m) | 7d: 23% | $31.00 | 📅 $199.70 | 13h",
        "⏵⏵ bypass permissions on (shift+tab to cycle) · ← for agents",
    ];
    // The periodic repaint re-emits only the footer rows (indices 5-7).
    let changed = changed_at(&screen, &[5, 6, 7]);
    let filtered = filter_below_cutoff(&screen, changed);
    assert!(
        filtered.is_empty(),
        "all-footer repaint must yield an empty post-cutoff set"
    );
    let chrome_only = compute_chrome_only(&filtered, true, false, false);
    let has_spinner = chrome_only
        && filtered
            .iter()
            .any(|r| crate::chrome::is_spinner_row(&r.text));
    assert!(chrome_only, "empty post-cutoff set is chrome_only");
    assert!(
        chrome_only && !has_spinner,
        "statusbar repaint must NOT pass the busy transition gate (no flap)"
    );
}

/// Companion to the flap regression: a REAL working spinner renders ABOVE
/// the input separator (in the content zone), so it survives the chrome
/// cutoff and the post-filter spinner check fires — keeping the agent alive.
/// Otherwise the agent false-idles mid-think (the dangerous direction the
/// single-path design prevents). This is what makes the positional fix safe:
/// no supported agent renders a genuine working spinner below the separator.
#[test]
fn test_content_zone_spinner_still_keeps_alive() {
    let cc_sep = "────────────────────────────────────────────────────────────────────────";
    let gem_sep =
        "─────────────────────────────────────────────────────────────────────────────────";
    let gem_top = "▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀";
    let gem_bot = "▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▀▀";
    // (full screen, index of the working spinner row). Spinner is ABOVE the
    // input separator in every case — verified against the cutoff tests.
    let cases: Vec<(Vec<&str>, usize)> = vec![
        // Claude Code: dingbat thinking spinner in the transcript zone.
        (
            vec![
                "✻ Cogitating… (3m 47s · ↓ 2.2k tokens)",
                "",
                cc_sep,
                "❯",
                cc_sep,
                "[Opus 4.8 (1M) | Team] █░░░░░░░░░ 6% | gh-metrics git:(master)",
            ],
            0,
        ),
        // Gemini CLI: braille spinner above the separator (live layout).
        (
            vec![
                "✦ I will read the package.json file.",
                " ⠴ Check tool-specific usage stats… (esc to cancel, 14s)",
                gem_sep,
                " Shift+Tab to accept edits",
                gem_top,
                " >   Type your message or @path/to/file",
                gem_bot,
                " workspace (/directory)          branch          sandbox",
            ],
            1,
        ),
    ];
    for (screen, spinner_idx) in &cases {
        let changed = changed_at(screen, &[*spinner_idx]);
        let filtered = filter_below_cutoff(screen, changed);
        assert!(
            filtered.iter().any(|r| r.row_index == *spinner_idx),
            "content-zone spinner at row {spinner_idx} must survive the cutoff: {:?}",
            screen[*spinner_idx]
        );
        let chrome_only = compute_chrome_only(&filtered, false, false, false);
        let has_spinner = chrome_only
            && filtered
                .iter()
                .any(|r| crate::chrome::is_spinner_row(&r.text));
        assert!(
            !chrome_only || has_spinner,
            "content-zone spinner {:?} must pass the busy transition gate",
            screen[*spinner_idx]
        );
    }
}

/// Aider during generation has NO bottom input box (prompt_toolkit has
/// returned), so `find_chrome_cutoff` finds no separator/prompt and returns
/// None → nothing is filtered → the Knight Rider spinner survives and keeps
/// the agent alive. This is why the positional fix does not false-idle Aider
/// even though its spinner is a bare block run.
#[test]
fn test_aider_generation_spinner_keeps_alive() {
    let screen: Vec<&str> = vec![
        "Applied edit to src/main.rs",
        "█░  Waiting for openrouter/anthropic/claude-sonnet-4.5",
    ];
    assert_eq!(
        crate::chrome::find_chrome_cutoff(&screen),
        None,
        "Aider generation view has no input box → no cutoff"
    );
    let changed = changed_at(&screen, &[1]);
    let filtered = filter_below_cutoff(&screen, changed);
    assert!(
        filtered.iter().any(|r| r.row_index == 1),
        "Knight Rider spinner must survive (no cutoff to drop it)"
    );
    let chrome_only = !filtered.is_empty() && filtered.iter().all(|r| is_chrome_row(&r.text));
    // Aider's Knight Rider bar leads its row, so the structural is_spinner_row
    // matches it (#446-596f).
    let has_spinner = chrome_only
        && filtered
            .iter()
            .any(|r| crate::chrome::is_spinner_row(&r.text));
    assert!(
        !chrome_only || has_spinner,
        "Aider Knight Rider spinner must pass the busy transition gate"
    );
}

// --- Presence-driven working-status keepalive (Codex frozen-TUI false-idle) ---

/// Codex freezes its TUI during a child subprocess (long `cargo`/`git`): the
/// grid stops changing for minutes, so the change-driven spinner keepalive
/// cannot refresh `last_output_ms` and the idle timer would falsely flip
/// idle. The presence guard must still see the `• Working (… esc to
/// interrupt)` line in the content zone and hold the agent busy.
#[test]
fn test_codex_frozen_working_line_holds_busy() {
    // Real layout (mirrors the live capture): the working line sits directly
    // above the `›` input prompt, with the model footer below it.
    let screen: Vec<String> = vec![
        "• Ran cargo test -p agent2-transport --locked".into(),
        "  └     Blocking waiting for file lock on package cache".into(),
        "    … +30 lines (ctrl + t to view transcript)".into(),
        "• Working (14m 56s • esc to interrupt)".into(),
        "› Improve documentation in @filename".into(),
        "  gpt-5.5 high · ~/Gits/LS/agent2".into(),
    ];
    assert_eq!(
        detect_codex_screen_activity(&screen),
        AgentScreenActivity::Working,
        "a frozen Codex working line above the prompt must keep the agent busy"
    );
}

/// When Codex finishes a turn the working line is gone (only the ready
/// prompt remains) → the presence guard must NOT hold busy, so the idle
/// timer is free to transition busy→idle normally.
#[test]
fn test_codex_ready_prompt_allows_idle() {
    let screen: Vec<String> = vec![
        "• Done. Added deny.toml and updated Cargo.toml.".into(),
        "› Improve documentation in @filename".into(),
        "  gpt-5.5 high · ~/Gits/LS/agent2".into(),
    ];
    assert_eq!(
        detect_codex_screen_activity(&screen),
        AgentScreenActivity::Ready,
        "a ready prompt with no working line must allow the idle transition"
    );
}

/// Regression: Codex separators divide tool output from the answer; they are
/// not prompt-box chrome. The old presence helper applied find_chrome_cutoff,
/// chose this separator over the later prompt, and discarded Working.
#[test]
fn test_codex_working_after_tool_separator_is_detected() {
    let screen: Vec<String> = vec![
        "• Ran cargo test --workspace".into(),
        "────────────────────────────────────────────────────────".into(),
        "• I am checking the remaining failures.".into(),
        "• Working (2m 55s • esc to interrupt)".into(),
        "› Add tests for the activity detector".into(),
        "  gpt-5.5 high · ~/repo".into(),
    ];
    let refs: Vec<&str> = screen.iter().map(String::as_str).collect();
    assert_eq!(
        crate::chrome::find_chrome_cutoff(&refs),
        Some(1),
        "fixture must reproduce the misleading generic cutoff"
    );
    assert_eq!(
        detect_codex_screen_activity(&screen),
        AgentScreenActivity::Working
    );
}

/// Regression (live capture, session "Native Closure"): while a background
/// terminal runs Codex swaps the status verb to `Waiting for background
/// terminal`. The turn is still interruptible, but the verb-keyed presence
/// check read Ready and the session showed a green idle dot for minutes.
#[test]
fn test_codex_background_terminal_wait_holds_busy() {
    let screen: Vec<String> = vec![
        "• Il secondo pre-push ha già superato nuovamente check, Clippy e audit root/plugin.".into(),
        String::new(),
        "• Waiting for background terminal (41s • esc to interrupt) · 1 background terminal running · /ps to view · …".into(),
        "  └ rtk git fetch origin POC-00002-BLADES-REFINEMENT && rtk git rev-parse origin/POC-00002…".into(),
        String::new(),
        String::new(),
        "› Use /skills to list available skills".into(),
        "  gpt-5.6-sol medium · ~/Gits/CC_Playground/itview · master · Context 67% left".into(),
    ];
    assert_eq!(
        detect_codex_screen_activity(&screen),
        AgentScreenActivity::Working,
        "a running background terminal must keep the Codex session busy"
    );
}

/// Live 2026-07-28 regression: while a background command is running Codex
/// v0.145 renders the current composer with `»`, while submitted transcript
/// prompts still use `›`. Looking only for `›` selected the historical row,
/// missed the later Working marker, and flipped the session idle every few
/// seconds until the next user submission.
#[test]
fn test_codex_guillemet_composer_finds_later_working_status() {
    let screen = fixture_rows(include_str!(
        "../../../tests/terminal-stress/fixtures/codex-background-working.txt"
    ));
    assert_eq!(
        detect_codex_screen_activity(&screen),
        AgentScreenActivity::Working,
        "the lowest current composer must anchor the working neighborhood"
    );
}

/// Live 2026-07-29 regression: Codex may begin an internal continuation
/// after the previous task emitted `suggest:`. Its persistent goal HUD still
/// says `Goal achieved`, but the interruptible Working row is authoritative.
#[test]
fn test_codex_goal_achieved_hud_does_not_hide_current_working_status() {
    let screen = fixture_rows(include_str!(
        "../../../tests/terminal-stress/fixtures/codex-completed-internal-working.txt"
    ));
    assert_eq!(
        detect_codex_screen_activity(&screen),
        AgentScreenActivity::Working
    );
}

#[test]
fn test_codex_historical_working_far_from_prompt_does_not_latch_busy() {
    let mut screen = vec!["• Working (1m • esc to interrupt)".to_string()];
    screen.extend((0..8).map(|n| format!("old transcript row {n}")));
    screen.push("› Ready for the next request".into());
    screen.push("  gpt-5.5 high · ~/repo".into());
    assert_eq!(
        detect_codex_screen_activity(&screen),
        AgentScreenActivity::Ready
    );
}

#[test]
fn historical_codex_prompt_outside_current_chrome_is_not_ready() {
    let mut screen = vec!["› an old submitted request".to_string()];
    screen.extend((0..8).map(|n| format!("current output row {n}")));

    assert_eq!(
        detect_codex_screen_activity(&screen),
        AgentScreenActivity::Unknown
    );
}

#[test]
fn codex_draft_prompt_above_tall_hud_is_current_chrome() {
    let mut screen = vec![
        "current output".to_string(),
        "─".repeat(80),
        "› Run /review on my current changes".to_string(),
        "─".repeat(80),
    ];
    screen.extend((0..30).map(|n| format!("custom HUD row {n}")));

    assert_eq!(
        detect_codex_screen_activity(&screen),
        AgentScreenActivity::Ready
    );
}

#[test]
fn gemini_markdown_quote_in_history_is_not_a_ready_prompt() {
    let mut screen = vec!["> quoted user prose".to_string()];
    screen.extend((0..8).map(|n| format!("current output row {n}")));

    assert_eq!(
        detect_gemini_screen_activity(&screen),
        AgentScreenActivity::Unknown
    );
}

#[test]
fn gemini_prompt_above_tall_hud_is_current_chrome() {
    let mut screen = vec![
        "current output".to_string(),
        "─".repeat(80),
        "> Type your message".to_string(),
        "─".repeat(80),
    ];
    screen.extend((0..30).map(|n| format!("custom HUD row {n}")));

    assert_eq!(
        detect_gemini_screen_activity(&screen),
        AgentScreenActivity::Ready
    );
}

// ---------------------------------------------------------------------
// Stuck-busy battery (#446-596f).
//
// Symptom history: sessions pinned BUSY forever by STATIC glyphs the screen
// classifier read as a live spinner — a completed-turn summary (`✻ Sautéed
// for 1m 25s`), a `· run /mcp` hint, a wiz HUD `░░` bar. Each glyph fix
// regressed differently (a hash-based liveness gate blocked re-latching but
// never demoted the Working classification, so the idle path stayed
// unreachable).
//
// Definitive design: "if the text above the input area moves, the agent is
// active — period." BUSY is latched/kept ONLY by movement (post-cutoff
// `changed_rows` are text-equality diffed, so a frozen glyph produces no
// ChangedRow and is inert by construction), by user submission, and by
// hooks. The Claude/Gemini/Aider screen classifiers are PROMPT-based only
// (Ready/Unknown, never Working), so a static glyph can never mask the
// ready prompt or hold the idle path hostage. Codex is the one deliberate
// exception: its presence-based `• Working (… esc to interrupt)` line holds
// BUSY while its TUI legitimately freezes during a child process — accepted
// policy: for Codex we prefer false-BUSY over false-IDLE.
// ---------------------------------------------------------------------

/// A representative Claude idle screen: assistant output, a summary/spinner
/// line, a blank gap, then the input prompt.
fn claude_screen_with(mid_line: &str) -> Vec<String> {
    vec![
        "⏺ Fixed the bug and ran the tests — all green.".into(),
        String::new(),
        "  Searched for 1 pattern, read 1 file (ctrl+o to expand)".into(),
        String::new(),
        mid_line.into(),
        String::new(),
        "❯ ".into(),
        String::new(),
    ]
}

/// Completed summaries and inert decoration above an empty composer remain
/// Ready. Active phase names are covered separately because current Claude
/// versions can leave the composer visible during long tool calls.
#[test]
fn claude_completed_decorations_remain_ready() {
    for mid in [
        "✻ Sautéed for 1m 25s", // completed-turn summary
        "✳ Ideated for 2m 9s · 1 local agent still running",
        "· Proofed for 1m 14s (↓ 1.6k tokens)",
        "✽ Sautéed for 12s",
    ] {
        let screen = claude_screen_with(mid);
        assert_eq!(
            detect_claude_screen_activity(&screen),
            AgentScreenActivity::Ready,
            "{mid:?}: a visible empty ❯ composer is Ready — no glyph can mask it"
        );
    }
}

/// Live capture from session "DB corruption": Claude kept the empty `❯`
/// composer on screen throughout a long tool call. The active phase marker
/// must outrank that composer even if load/coalescing freezes its text.
#[test]
fn claude_active_phase_with_visible_composer_holds_busy() {
    let screen = fixture_rows(include_str!(
        "../../../tests/terminal-stress/fixtures/claude-blocking-stop-hook.txt"
    ));
    assert_eq!(
        detect_claude_screen_activity(&screen),
        AgentScreenActivity::Working
    );
}

/// A semantic active phase is Working with or without a visible composer.
/// This presence fallback is required when repaint movement freezes while a
/// long child or blocking hook still owns the turn.
#[test]
fn claude_active_phase_without_prompt_holds_busy() {
    let screen: Vec<String> = vec![
        "⏺ Editing src/main.rs…".into(),
        String::new(),
        "✻ Sautéing… (12s · esc to interrupt)".into(),
        String::new(),
    ];
    assert_eq!(
        detect_claude_screen_activity(&screen),
        AgentScreenActivity::Working
    );
}

/// Live 2026-07-19 regression: Claude echoes the submitted argv prompt as a
/// `❯ task` transcript row. While the turn is still running that historical
/// row can remain inside the bottom scan window beside an animated spinner;
/// it is not the empty composer and must never confirm idle.
#[test]
fn claude_submitted_prompt_row_is_not_a_ready_composer() {
    for prompt in [
        "❯ Read-only review the Windows native smoke scope",
        "  ❯ draft text not yet submitted",
    ] {
        let screen = vec![
            prompt.to_string(),
            "⏺ Reading 1 file…".into(),
            "✻ Boogieing…".into(),
        ];
        assert_eq!(
            detect_claude_screen_activity(&screen),
            AgentScreenActivity::Unknown,
            "only Claude's empty composer is Ready: {prompt:?}"
        );
    }
}

/// THE core invariant of the movement design: a byte-identical repaint of a
/// frozen "spinner" line produces NO ChangedRow (text-equality diff in
/// `TerminalGrid::process`), so it can never pass the reader's busy gate —
/// while a genuinely animating frame always does.
#[test]
fn frozen_summary_repaint_produces_no_movement() {
    let mut grid = crate::terminal_grid::TerminalGrid::new(24, 80, 1000);
    let frame = "\x1b[H\x1b[2K\u{273B} Saut\u{00E9}ed for 1m 25s";
    let first = grid.process(frame.as_bytes());
    assert!(
        first.iter().any(|r| crate::chrome::is_spinner_row(&r.text)),
        "first paint of the line IS movement"
    );
    let repaint = grid.process(frame.as_bytes());
    assert!(
        repaint.is_empty(),
        "byte-identical repaint must produce no ChangedRow → no busy evidence"
    );
    let animated = grid.process("\x1b[H\x1b[2K\u{273B} Saut\u{00E9}ing\u{2026} (13s)".as_bytes());
    assert!(
        animated
            .iter()
            .any(|r| crate::chrome::is_spinner_row(&r.text)),
        "an animating spinner frame IS movement and keeps/latches BUSY"
    );
}

/// A real captured Claude idle screen: the `▐▛███▜▌` welcome banner (█ art),
/// the empty `❯` input box framed by separators, and a wiz status-line HUD
/// whose progress bar is a run of `░`/`█` block glyphs. Nothing here is an
/// animated spinner — the turn is over and Claude waits for input.
fn claude_idle_with_banner_and_hud() -> Vec<String> {
    vec![
        "╭─── Claude Code v2.1.202 ──────────────────────────────╮".into(),
        "│                   ▐▛███▜▌                   │ What's new".into(),
        "│                  ▝▜█████▛▘                  │ Forked subagents".into(),
        "│      Opus 4.8 (1M context) · Claude Team    │           ".into(),
        "╰───────────────────────────────────────────────────────╯".into(),
        String::new(),
        " ⚠ 2 MCP servers need authentication · run /mcp".into(),
        String::new(),
        "───────────────────────────────────────────────────────────".into(),
        "❯ ".into(),
        "───────────────────────────────────────────────────────────".into(),
        "  [Opus 4.8 (1M) | Team] ░░░░░░░░░░ 0% | cerebro | [C1 S33]".into(),
        "  5h: 52% (52m) | 7d: 18% (22h) | $0 | 📅 $124.01 | 13m".into(),
        "  ⏵⏵ bypass permissions on (shift+tab to cycle) · ← for agents".into(),
    ]
}

/// #446-596f regression: block-glyph art (the welcome banner) and a status-
/// line HUD progress bar (`░░░░`) are NOT Claude's animated spinner. Claude's
/// spinner is dingbats (✻ ✳ ✶) / middle-dot `·`; solid blocks appear only in
/// static art. Before the fix, `is_spinner_row` matched `█`/`░`, so an idle
/// Claude prompt read Working and the session never returned to idle.
#[test]
fn claude_idle_with_wiz_hud_is_ready_not_working() {
    let screen = claude_idle_with_banner_and_hud();
    assert_eq!(
        detect_claude_screen_activity(&screen),
        AgentScreenActivity::Ready,
        "an idle Claude prompt under banner art, a `· run /mcp` hint and a \
             live wiz HUD is Ready, not Working"
    );
}

/// The wiz HUD ticks every second (elapsed timer, token counts), so the old
/// frozen-signature liveness gate could not save us — the bar is genuinely
/// changing. The only robust cut is that block glyphs are not a spinner.
#[test]
fn wiz_hud_progress_bar_is_not_a_claude_spinner() {
    let hud = "  [Opus 4.8 (1M) | Team] ██░░░░░░░░ 17% | cerebro".to_string();
    assert!(
        !crate::chrome::is_spinner_row(&hud),
        "a status-line progress bar is not an animated spinner"
    );
}

/// Guardrail: the fix must NOT break Aider, whose real "Knight Rider" spinner
/// IS a run of block glyphs that LEADS its row, so the structural
/// `is_spinner_row` still matches it — its movement latches/keeps BUSY via
/// the reader gate. Classification stays prompt-based: mid-generation Aider
/// has no input box, so the screen is Unknown (never a false Ready).
#[test]
fn aider_knight_rider_block_spinner_still_movement_evidence() {
    assert!(
        crate::chrome::is_spinner_row("░░░█░░░░░░"),
        "Aider's Knight Rider block spinner leads the row → still a spinner"
    );
    let generating: Vec<String> = vec!["Applied edit to src/main.rs".into(), "░░░█░░░░░░".into()];
    assert_eq!(
        detect_aider_screen_activity(&generating),
        AgentScreenActivity::Unknown,
        "no input box during generation → Unknown, BUSY held by movement"
    );
}

/// Codex v0.146.0 grew its status row from `<model> <effort> · <N>% left · <dir>` to
/// `<model> <effort> · <dir> · <branch> · Context <N>% left · <N>K window`. The adapter
/// must stay blind to that row: it anchors on the `›` prompt plus the interrupt hint,
/// both branch- and gauge-independent. Rows transcribed from a live v0.146.0 session
/// captured 2026-08-02.
#[test]
fn test_codex_v0_146_status_row_with_git_branch_does_not_change_detection() {
    const STATUS_ROW: &str = "  gpt-5.6-luna xhigh \u{00B7} ~/Gits/personal/tuicommander \u{00B7} main \u{00B7} Context 96% left \u{00B7} 247K window";

    let working = vec![
        "\u{203A} Run this shell command with your tool: sleep 25 && echo hello".to_string(),
        "\u{2022} Boss, eseguo il comando richiesto.".to_string(),
        "\u{2022} Working (3s \u{2022} esc to interrupt) \u{00B7} 1 background terminal running \u{00B7} /ps to view".to_string(),
        "\u{203A} Explain this codebase".to_string(),
        STATUS_ROW.to_string(),
    ];
    assert_eq!(
        detect_codex_screen_activity(&working),
        AgentScreenActivity::Working
    );

    // Finished turn: separators and `• Output:` sit in the prompt neighborhood, and the
    // status row still carries the branch. Nothing there is an interrupt hint.
    let finished = vec![
        "\u{2022} Ran sleep 25 && echo hello".to_string(),
        "  \u{2514} hello".to_string(),
        "\u{2500}".repeat(120),
        "\u{2022} Output:".to_string(),
        "  hello".to_string(),
        "\u{2500}".repeat(120),
        "\u{203A} Explain this codebase".to_string(),
        STATUS_ROW.to_string(),
    ];
    assert_eq!(
        detect_codex_screen_activity(&finished),
        AgentScreenActivity::Ready
    );

    // The branch field itself must never be mistaken for chrome that holds BUSY.
    assert!(!crate::chrome::is_working_status_row(STATUS_ROW));
}

/// Captured live from grok 0.2.114: the composer moved inside a rounded box, so the old
/// bare-`❯` match never fired and the tab stayed BUSY minutes after the turn finished.
#[test]
fn test_grok_boxed_composer_is_ready_and_spinner_still_wins() {
    let finished = vec![
        "     ❯ List the numbers 1 to 60, one per line.                    5:42 PM".to_string(),
        "     1 2 3 4 5 6 7 8 9 10                                                ".to_string(),
        "     Worked for 2.6s                                   stop  [hooks: 1]  ".to_string(),
        "  ╭────────────────────────────────────────────────────────────────────╮".to_string(),
        "  │ ❯                                                                  │".to_string(),
        "  ╰─────────────────────────────── Grok 4.5 (high) · always-approve ───╯".to_string(),
        "  Shift+Tab:mode  │  Ctrl+.:shortcuts".to_string(),
    ];
    assert_eq!(
        detect_agent_screen_activity(Some("grok"), &finished),
        AgentScreenActivity::Ready
    );

    // Mid-turn grok keeps the same composer box on screen, so the spinner must outrank it.
    let mut running = finished.clone();
    running.insert(
        2,
        "    ⠋ Waiting for response… 1.1s                     1.1s ⇣6.98k [stop]".to_string(),
    );
    assert_eq!(
        detect_agent_screen_activity(Some("grok"), &running),
        AgentScreenActivity::Working
    );
}

/// Captured live from pi 0.83.0. pi's composer is a bare reverse-video cursor block with
/// no prompt glyph, so readiness rests on the status row plus the absence of a spinner.
#[test]
fn test_pi_finished_turn_is_ready_and_working_row_wins() {
    let separator = "─".repeat(100);
    let finished = vec![
        " Count from 1 to 40, one number per line, no tools, no commentary.".to_string(),
        " 1".to_string(),
        " 2".to_string(),
        separator.clone(),
        "                                                                  ".to_string(),
        separator.clone(),
        "~/Gits/personal/tuicommander (main)".to_string(),
        "↑1.3k ↓1.8k R15k W6.0k CH88.5% $0.104 3.4%/272k (auto)      (openai) gpt-5.6-sol • medium"
            .to_string(),
    ];
    assert_eq!(
        detect_agent_screen_activity(Some("pi"), &finished),
        AgentScreenActivity::Ready
    );

    // Mid-turn pi keeps the same separators and status row; only the composer row swaps.
    let mut running = finished.clone();
    running[4] = " ⠏ Working...".to_string();
    assert_eq!(
        detect_agent_screen_activity(Some("pi"), &running),
        AgentScreenActivity::Working
    );
}

#[test]
fn test_pi_status_row_needs_the_context_gauge_and_model_separator() {
    assert!(is_pi_status_row(
        "0.0%/272k (auto)                          (openai) gpt-5.6-sol • medium"
    ));
    // Prose carrying a bullet but no context gauge is not chrome.
    assert!(!is_pi_status_row("read the file • then summarise it"));
    // A percentage that is not the context gauge must not qualify.
    assert!(!is_pi_status_row("coverage 88.5% • done"));
    assert!(!is_pi_status_row(""));
}

#[test]
fn test_pi_is_recognised_as_an_agent() {
    assert_eq!(classify_agent("pi"), Some("pi"));
    assert!(has_ready_screen_adapter(Some("pi")));
}

/// Rows below are transcribed from live opencode v1.18.5 screens captured on
/// 2026-08-02 in this repo (welcome, mid-turn, and finished-turn). The frame glyphs
/// were confirmed against a `script(1)` byte log: `┃` U+2503, `╹` U+2579, `▀` U+2580.
fn opencode_finished_screen() -> Vec<String> {
    vec![
        "     VT100 is dead. The terminal it defined will be with us for a long time.".into(),
        "     \u{25A3}  Build \u{00B7} Big Pickle \u{00B7} 31.3s".into(),
        "  \u{2503}".into(),
        "  \u{2503}".into(),
        "  \u{2503}".into(),
        "  \u{2503}  Build \u{00B7} Big Pickle OpenCode Zen".into(),
        format!("  \u{2579}{}", "\u{2580}".repeat(98)),
        "   /Users/stefano.straus/Gits/personal/tuicommander       18.1K (9%)  ctrl+p commands"
            .into(),
    ]
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn test_only_interpreters_take_the_argv0_detour() {
    assert!(is_script_interpreter("node"));
    assert!(is_script_interpreter("bun"));
    assert!(!is_script_interpreter("claude"));
    assert!(!is_script_interpreter("zsh"));
}

#[test]
fn stale_busy_prior_hook_busy_cannot_be_treated_as_prior_idle() {
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "stale-busy-prior-hook";
    agent_session(&state, sid, SHELL_BUSY);
    transition_explicit_shell_state(&state, sid, SHELL_BUSY, "busy", true);
    note_submitted_input(&state, sid);
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
    {
        let mut sl = silence.lock();
        sl.evidence.busy.as_mut().unwrap().at = std::time::Instant::now() - PROTOCOL_STALE_TIMEOUT;
        sl.last_output_at = std::time::Instant::now() - PROTOCOL_STALE_TIMEOUT;
    }
    assert!(
        !try_timer_idle_transition(
            &state,
            &silence,
            sid,
            AgentScreenActivity::Unknown,
            Some("claude"),
            Some(1)
        )
        .transitioned
    );
    assert_eq!(
        state
            .session_state_with_shell(sid)
            .unwrap()
            .agent_state
            .as_deref(),
        Some("working")
    );
}

#[test]
fn test_working_row_cannot_relatch_a_declared_completed_turn() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "completed-working-row";
    state.session_maps.session_states.insert(
        session_id.into(),
        crate::state::SessionState {
            agent_type: Some("codex".into()),
            ..Default::default()
        },
    );
    state.session_maps.shell_states.insert(
        session_id.into(),
        std::sync::atomic::AtomicU8::new(SHELL_IDLE),
    );
    let mut lifecycle = SilenceState::new();
    lifecycle.confirm_idle();
    lifecycle.mark_suggest_candidate(vec!["Review diff".into()], 0);
    let lifecycle = Arc::new(Mutex::new(lifecycle));

    apply_working_evidence(
        &state,
        &lifecycle,
        session_id,
        now_epoch_ms(),
        "working-screen",
    );

    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(session_id)
            .unwrap()
            .load(std::sync::atomic::Ordering::Acquire),
        SHELL_IDLE
    );
    assert!(lifecycle.lock().idle_confirmed());
}

#[test]
fn test_codex_moving_working_row_reopens_completed_internal_continuation() {
    use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "codex-completed-internal-continuation";
    state.session_maps.session_states.insert(
        session_id.into(),
        crate::state::SessionState {
            agent_type: Some("codex".into()),
            suggested_actions: Some(vec!["Review diff".into()]),
            ..Default::default()
        },
    );
    state
        .session_maps
        .shell_states
        .insert(session_id.into(), AtomicU8::new(SHELL_IDLE));
    state
        .session_maps
        .last_output_ms
        .insert(session_id.into(), AtomicU64::new(1));
    let mut lifecycle = SilenceState::new();
    lifecycle.confirm_idle();
    lifecycle.mark_suggest_candidate(vec!["Review diff".into()], 0);
    let lifecycle = Arc::new(Mutex::new(lifecycle));

    apply_working_evidence(
        &state,
        &lifecycle,
        session_id,
        now_epoch_ms(),
        "working-screen-movement",
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
    assert!(
        state
            .session_maps
            .session_states
            .get(session_id)
            .unwrap()
            .suggested_actions
            .is_none()
    );
    let lifecycle = lifecycle.lock();
    assert!(!lifecycle.completion_declared_for_epoch(0));
    assert!(!lifecycle.idle_confirmed());
}

#[test]
fn test_claude_active_marker_reopens_premature_stop_hook_completion() {
    use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "claude-blocking-stop-hook";
    state.session_maps.session_states.insert(
        session_id.into(),
        crate::state::SessionState {
            agent_type: Some("claude".into()),
            suggested_actions: Some(vec!["Premature follow-up".into()]),
            ..Default::default()
        },
    );
    state
        .session_maps
        .shell_states
        .insert(session_id.into(), AtomicU8::new(SHELL_IDLE));
    state
        .session_maps
        .last_output_ms
        .insert(session_id.into(), AtomicU64::new(1));
    let mut lifecycle = SilenceState::new();
    lifecycle.mark_suggest_candidate(vec!["Premature follow-up".into()], 0);
    lifecycle.note_explicit_state(SHELL_IDLE, true);
    let lifecycle = Arc::new(Mutex::new(lifecycle));
    state
        .session_maps
        .silence_states
        .insert(session_id.into(), lifecycle.clone());

    let screen = vec![
        "✽ Nucleating… (8m 47s · ↓ 29.0k tokens)".to_string(),
        "❯".to_string(),
    ];
    let activity = detect_agent_screen_activity(Some("claude"), &screen);
    assert_eq!(activity, AgentScreenActivity::Working);
    assert_eq!(
        completion_adjusted_screen_activity(&state, &lifecycle, session_id, activity,),
        AgentScreenActivity::Working,
        "premature completion must not downgrade current Claude work"
    );

    apply_working_evidence(
        &state,
        &lifecycle,
        session_id,
        now_epoch_ms(),
        "working-screen",
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
    assert!(
        state
            .session_maps
            .session_states
            .get(session_id)
            .unwrap()
            .suggested_actions
            .is_none()
    );
    let lifecycle = lifecycle.lock();
    assert!(!lifecycle.completion_declared_for_epoch(0));
    assert!(!lifecycle.explicit_idle());
    assert!(!lifecycle.idle_confirmed());
}

#[test]
fn test_declared_completion_turns_stale_working_screen_into_ready_evidence() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "completed-working-timer";
    agent_session(&state, session_id, SHELL_BUSY);
    {
        let mut session = state
            .session_maps
            .session_states
            .get_mut(session_id)
            .unwrap();
        session.agent_type = Some("codex".into());
        session.background_probe_satisfied_turn_epoch = Some(session.turn_epoch);
    }
    let lifecycle = state
        .session_maps
        .silence_states
        .get(session_id)
        .unwrap()
        .clone();
    {
        let mut lifecycle = lifecycle.lock();
        lifecycle.mark_suggest_candidate(vec!["Review diff".into()], 0);
        lifecycle.screen_ready_pending_since =
            Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    }

    let screen_activity = completion_adjusted_screen_activity(
        &state,
        &lifecycle,
        session_id,
        AgentScreenActivity::Working,
    );
    let transition = try_timer_idle_transition(
        &state,
        &lifecycle,
        session_id,
        screen_activity,
        Some("codex"),
        Some(0),
    );

    assert!(transition.screen_confirms_idle);
    assert!(transition.transitioned);
    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(session_id)
            .unwrap()
            .load(std::sync::atomic::Ordering::Acquire),
        SHELL_IDLE
    );
}

/// The measurement `SCREEN_CLASSIFY_CALLS` was built for and never got.
///
/// #744-138c claimed the reader chunk path and the silence timer no longer each
/// classify the screen independently, and added a counter plus a `#[cfg(test)]`
/// accessor to prove it. The accessor had zero callers, so the claim went
/// unmeasured and `--all-targets` clippy reported the accessor as dead code.
/// Deleting it was the wrong fix: it is not dead, it is the only surviving
/// trace that a de-duplication was asserted and never checked.
///
/// Two halves, because the claim has two:
/// 1. the reader classifies at most once for one chunk, and publishes the
///    verdict to `cached_screen_activity`;
/// 2. the timer *reads* that field instead of calling the classifier again —
///    a structural property of `spawn_silence_timer`, which is an async loop
///    with no practical unit-test entry point, so it is asserted against the
///    source the same way `close_pty_never_runs_on_the_ipc_thread` does.
///
/// The counter is a process-wide static, so the delta is only meaningful under
/// a process-per-test runner. That is nextest, which is what this project runs
/// (AGENTS.md); the bound is deliberately one-sided so a shared-process runner
/// cannot make it flaky in the other direction.
#[test]
fn the_screen_is_classified_once_per_chunk_not_once_per_reader_and_once_per_timer() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "screen-classify-once";
    agent_session(&state, session_id, SHELL_BUSY);
    state
        .session_maps
        .session_states
        .get_mut(session_id)
        .unwrap()
        .agent_type = Some("codex".into());
    state.grid.vt_log_buffers.insert(
        session_id.to_string(),
        Mutex::new(crate::state::VtLogBuffer::new(24, 80, 2000)),
    );
    let silence = state
        .session_maps
        .silence_states
        .get(session_id)
        .unwrap()
        .clone();

    let before = screen_classify_calls();
    let mut processor = ChunkProcessor::new(None, None);
    processor.process_chunk("working on it\r\n", &silence, session_id, &state);
    let delta = screen_classify_calls() - before;
    assert!(
        delta <= 1,
        "one chunk must not classify the screen more than once, saw {delta}"
    );

    // The timer must reuse that verdict rather than produce its own.
    let source = include_str!("../pty.rs");
    let at = source
        .find("fn spawn_silence_timer(")
        .expect("spawn_silence_timer must exist");
    let body = &source[at..];
    let end = body
        .find("\n}\n")
        .expect("spawn_silence_timer must be a closed function");
    let body = &body[..end];
    assert!(
        body.contains("cached_screen_activity"),
        "the silence timer must read the reader's cached verdict"
    );
    assert!(
        !body.contains("detect_agent_screen_activity("),
        "the silence timer must NOT classify the screen itself — that is the \
         second call #744-138c removed, and the counter above cannot see it \
         from a unit test"
    );
}

#[test]
fn test_interrupt_request_plus_interrupted_screen_confirms_idle() {
    let mut silence = SilenceState::new();
    silence.note_explicit_state(SHELL_BUSY, true);
    silence.note_interrupt_requested();
    assert!(silence.note_interrupted_screen());
    assert!(!silence.explicit_busy());
    assert!(silence.idle_confirmed());
}

#[test]
fn test_ctrl_c_alone_never_confirms_idle() {
    let mut silence = SilenceState::new();
    silence.note_explicit_state(SHELL_BUSY, true);
    silence.note_interrupt_requested();
    assert!(silence.explicit_busy());
    assert!(!silence.idle_confirmed());
}

#[test]
fn test_working_screen_recovers_idle_to_busy() {
    use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "codex-false-idle";
    state
        .session_maps
        .shell_states
        .insert(sid.into(), AtomicU8::new(SHELL_IDLE));
    state
        .session_maps
        .last_output_ms
        .insert(sid.into(), AtomicU64::new(1));
    let silence = Arc::new(Mutex::new(SilenceState::new()));
    state
        .session_maps
        .silence_states
        .insert(sid.into(), silence.clone());

    apply_working_evidence(&state, &silence, sid, now_epoch_ms(), "working-screen");

    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(sid)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_BUSY
    );
    assert!(!silence.lock().idle_confirmed());
}

#[test]
fn test_explicit_idle_outvotes_stale_working_screen() {
    use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "codex-stale-working";
    state
        .session_maps
        .shell_states
        .insert(sid.into(), AtomicU8::new(SHELL_IDLE));
    state
        .session_maps
        .last_output_ms
        .insert(sid.into(), AtomicU64::new(1));
    let mut sl = SilenceState::new();
    sl.note_explicit_state(SHELL_IDLE, true);
    let silence = Arc::new(Mutex::new(sl));
    state
        .session_maps
        .silence_states
        .insert(sid.into(), silence.clone());

    apply_working_evidence(&state, &silence, sid, now_epoch_ms(), "working-screen");

    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(sid)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_IDLE
    );
    assert!(silence.lock().idle_confirmed());
}

#[test]
fn active_screen_matrix_is_stable_and_repairs_false_idle_repeatedly() {
    use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

    let state = crate::state::tests_support::make_test_app_state();
    let cases = [
        (
            "codex-chevron",
            "codex",
            vec![
                "• Working (7s • esc to interrupt)".to_string(),
                "› Use /skills to list available skills".to_string(),
            ],
        ),
        (
            "codex-guillemet-background",
            "codex",
            vec![
                "› historical submitted prompt".to_string(),
                "• Waiting for background terminal (41s • esc to interrupt) · 1 background terminal running".to_string(),
                String::new(),
                "» Use /skills to list available skills".to_string(),
            ],
        ),
        (
            "claude-visible-composer",
            "claude",
            vec![
                "✽ Nucleating… (3m 50s · ↓ 7.8k tokens)".to_string(),
                String::new(),
                "────────────────────────────────────────".to_string(),
                "❯".to_string(),
                "────────────────────────────────────────".to_string(),
            ],
        ),
        (
            "claude-frozen-tool",
            "claude",
            vec![
                "✻ Sautéing… (12s · esc to interrupt)".to_string(),
                String::new(),
                "❯".to_string(),
            ],
        ),
        (
            "grok-responding",
            "grok",
            vec![
                "❯ Ask anything".to_string(),
                "⠴ Responding… 12s [stop]".to_string(),
                "⌘ Grok 4.3 OpenRouter · Medium effort".to_string(),
            ],
        ),
    ];

    for (sid, agent, rows) in cases {
        state.session_maps.session_states.insert(
            sid.into(),
            crate::state::SessionState {
                agent_type: Some(agent.into()),
                ..Default::default()
            },
        );
        state
            .session_maps
            .shell_states
            .insert(sid.into(), AtomicU8::new(SHELL_IDLE));
        state
            .session_maps
            .last_output_ms
            .insert(sid.into(), AtomicU64::new(1));
        let lifecycle = Arc::new(Mutex::new(SilenceState::new()));
        state
            .session_maps
            .silence_states
            .insert(sid.into(), lifecycle.clone());

        for iteration in 0..256 {
            state
                .session_maps
                .shell_states
                .get(sid)
                .unwrap()
                .store(SHELL_IDLE, Ordering::Release);
            lifecycle.lock().confirm_idle();

            let activity = detect_agent_screen_activity(Some(agent), &rows);
            assert_eq!(
                activity,
                AgentScreenActivity::Working,
                "{sid} iteration {iteration}"
            );
            apply_working_evidence(&state, &lifecycle, sid, now_epoch_ms(), "working-screen");
            assert_eq!(
                state
                    .session_maps
                    .shell_states
                    .get(sid)
                    .unwrap()
                    .load(Ordering::Acquire),
                SHELL_BUSY,
                "{sid} failed to repair false idle at iteration {iteration}"
            );
            assert!(!lifecycle.lock().idle_confirmed());
        }
    }
}

#[test]
fn completed_and_lookalike_screen_matrix_never_latches_working() {
    let claude_completed = fixture_rows(include_str!(
        "../../../tests/terminal-stress/fixtures/claude-completed.txt"
    ));
    let cases = [
        (
            "codex completed output",
            "codex",
            vec![
                "• Waited for background terminal · cargo test".to_string(),
                "» Use /skills to list available skills".to_string(),
            ],
            AgentScreenActivity::Ready,
        ),
        (
            "claude completed summary",
            "claude",
            claude_completed,
            AgentScreenActivity::Ready,
        ),
        (
            "claude hud progress",
            "claude",
            vec![
                "  [Opus | Team] ██░░░░░░░░ 17%".to_string(),
                "❯".to_string(),
            ],
            AgentScreenActivity::Ready,
        ),
        (
            "claude source lookalike below composer",
            "claude",
            vec![
                "Completed normally".to_string(),
                "❯".to_string(),
                "✻ source_example… (not live)".to_string(),
            ],
            AgentScreenActivity::Ready,
        ),
    ];

    for iteration in 0..256 {
        for (name, agent, rows, expected) in &cases {
            assert_eq!(
                detect_agent_screen_activity(Some(agent), rows),
                *expected,
                "{name} iteration {iteration}"
            );
        }
    }
}

#[test]
fn sanitized_codex_and_claude_trace_replay_requires_post_submit_consumption() {
    // Sanitized from the 2026-07-18 live sequence: ready prompt → injected
    // checkpoint → working/real output → final protocol text → ready prompt.
    // Repository paths, prompts, and response content are intentionally omitted.
    for agent in ["codex", "claude"] {
        let completed = replay_sanitized_agent_trace(
            agent,
            &[
                SanitizedTraceStep::Submit,
                SanitizedTraceStep::WorkingScreen,
                SanitizedTraceStep::RealActivity,
                SanitizedTraceStep::ReadyScreen,
            ],
        );
        assert!(completed.idle_confirmed(), "{agent} completed trace");

        let silent = replay_sanitized_agent_trace(
            agent,
            &[
                SanitizedTraceStep::Submit,
                SanitizedTraceStep::ReadyScreen,
                SanitizedTraceStep::ReadyScreen,
            ],
        );
        assert!(
            !silent.idle_confirmed(),
            "{agent} silent/no-op submission must remain conservative without positive consumption"
        );

        let partial_redraw = replay_sanitized_agent_trace(
            agent,
            &[
                SanitizedTraceStep::Submit,
                SanitizedTraceStep::UnknownScreen,
                SanitizedTraceStep::ReadyScreen,
            ],
        );
        assert!(
            !partial_redraw.idle_confirmed(),
            "{agent} partial/alternate-screen redraw must not prove consumption"
        );
    }
}

#[test]
fn a_bare_shell_reads_as_a_prompt() {
    assert!(is_prompt_shell_process(&process(1, 0, "sh", "sh")));
    assert!(is_prompt_shell_process(&process(1, 0, "bash", "bash -l")));
    // A login shell reports its name with a leading dash.
    assert!(is_prompt_shell_process(&process(1, 0, "-zsh", "-zsh")));
}

#[test]
fn a_shell_handed_a_command_is_work() {
    assert!(!is_prompt_shell_process(&process(
        1,
        0,
        "sh",
        "sh -c 'while true; do sleep 1; done'"
    )));
    assert!(!is_prompt_shell_process(&process(
        1,
        0,
        "bash",
        "bash -c make"
    )));
}

#[test]
fn a_non_shell_is_never_a_prompt() {
    assert!(!is_prompt_shell_process(&process(
        1,
        0,
        "dd",
        "dd if=/dev/rdisk11 of=/tmp/x.img"
    )));
    assert!(!is_prompt_shell_process(&process(1, 0, "vim", "vim a.txt")));
}

#[test]
fn nested_interactive_shell_is_a_prompt_not_work() {
    // The regression: `sudo su` latches the outer shell BUSY via OSC 133 and
    // the inner `sh` never emits the closing marker, so the tab reported
    // working for as long as the root shell lived.
    assert!(foreground_group_at_prompt(200, &sudo_su_tree()));
    assert!(foreground_group_at_prompt(
        300,
        &[process(300, 100, "sh", "sh")]
    ));
}

#[test]
fn a_wrapper_running_real_work_is_not_a_prompt() {
    let mut tree = sudo_su_tree();
    tree.push(process(204, 203, "dd", "dd if=/dev/rdisk11 of=/tmp/x.img"));
    assert!(
        !foreground_group_at_prompt(200, &tree),
        "work anywhere under the wrapper must keep the session busy"
    );
    assert!(!foreground_group_at_prompt(
        400,
        &[
            process(400, 100, "sudo", "sudo dd if=/dev/rdisk11"),
            process(401, 400, "dd", "dd if=/dev/rdisk11"),
        ]
    ));
}

#[test]
fn an_unknown_root_is_not_a_prompt() {
    // No snapshot entry for the pid means no evidence; fail toward busy.
    assert!(!foreground_group_at_prompt(999, &sudo_su_tree()));
}

fn busy_plain_shell(sid: &str, silent_for_ms: u64) -> AppState {
    use std::sync::atomic::{AtomicU8, AtomicU64};
    let state = crate::state::tests_support::make_test_app_state();
    state
        .session_maps
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));
    state
        .session_maps
        .session_states
        .insert(sid.to_string(), crate::state::SessionState::default());
    state.session_maps.last_output_ms.insert(
        sid.to_string(),
        AtomicU64::new(now_epoch_ms() - silent_for_ms),
    );
    state
}

#[test]
fn prompt_probe_waits_for_real_silence() {
    let sid = "s";
    assert!(prompt_probe_applies(
        &busy_plain_shell(sid, SHELL_PROMPT_PROBE_SILENCE_MS + 500),
        sid
    ));
    assert!(
        !prompt_probe_applies(&busy_plain_shell(sid, 200), sid),
        "a command that just printed is running, not parked at a prompt"
    );
}

#[test]
fn prompt_probe_leaves_agents_alone() {
    let sid = "s";
    let state = busy_plain_shell(sid, SHELL_PROMPT_PROBE_SILENCE_MS + 500);
    state.session_maps.session_states.insert(
        sid.to_string(),
        crate::state::SessionState {
            agent_type: Some("claude".to_string()),
            ..Default::default()
        },
    );
    assert!(
        !prompt_probe_applies(&state, sid),
        "agents own a ready-screen adapter; this probe must not second-guess it"
    );
}

#[test]
fn prompt_probe_ignores_an_idle_session() {
    use std::sync::atomic::AtomicU8;
    let sid = "s";
    let state = busy_plain_shell(sid, SHELL_PROMPT_PROBE_SILENCE_MS + 500);
    state
        .session_maps
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_IDLE));
    assert!(!prompt_probe_applies(&state, sid));
}

#[test]
fn prompt_probe_demands_the_process_snapshot() {
    let sid = "s";
    let state = busy_plain_shell(sid, SHELL_PROMPT_PROBE_SILENCE_MS + 500);
    state
        .session_maps
        .silence_states
        .insert(sid.to_string(), Arc::new(Mutex::new(SilenceState::new())));
    assert!(
        process_snapshot_is_demanded(&state),
        "without demand the cache stays empty and the probe can never fire"
    );
}

#[test]
fn persistent_helpers_are_not_background_work() {
    let mut processes = vec![
        process(10, 1, "codex", "codex"),
        process(11, 10, "tuic-bridge", "tuic-bridge"),
        process(12, 10, "mdkb", "mdkb serve"),
        // Descendants owned by helper plumbing are ignored with the helper.
        process(14, 12, "sqlite-worker", "sqlite-worker"),
    ];
    // Recognised by its command line rather than its name, which only holds
    // where the process snapshot reports one — Toolhelp gives the executable
    // name alone, so `node` stays meaningful work on Windows by design. See
    // `windows_helper_classification_does_not_guess_node_arguments`.
    if cfg!(not(windows)) {
        processes.push(process(13, 10, "node", "node /opt/codex/node_repl.js"));
    }
    assert!(!has_meaningful_descendant(10, &processes));

    let mut with_real_child = processes;
    with_real_child.push(process(20, 10, "cargo", "cargo test --locked"));
    assert!(has_meaningful_descendant(10, &with_real_child));
}

#[test]
fn missing_ages_leave_the_helper_name_list_in_charge() {
    // Windows reports no creation time. The rule must then behave exactly as
    // it did before ages existed — erring toward reporting work.
    let ageless = vec![
        process(10, 1, "codex", "codex"),
        process(11, 10, "codex-code-mode-host", "codex-code-mode-host"),
    ];
    assert!(has_meaningful_descendant(10, &ageless));

    // A descendant older than its own agent cannot be work that agent
    // spawned; a skewed `ps` sample must not invent background work.
    let skewed = vec![
        aged_process(10, 1, "codex", "codex", 100),
        aged_process(11, 10, "mystery", "mystery", 400),
    ];
    assert!(!has_meaningful_descendant(10, &skewed));
}

#[test]
fn windows_helper_classification_does_not_guess_node_arguments() {
    let node = process(40, 10, "node.exe", "node.exe node_repl.js");
    assert!(is_persistent_agent_helper_with_command_line(&node, true));
    assert!(
        !is_persistent_agent_helper_with_command_line(&node, false),
        "Toolhelp exposes only the executable name, so node.exe remains meaningful"
    );
    let dedicated = process(41, 10, "node_repl.exe", "");
    assert!(is_persistent_agent_helper_with_command_line(
        &dedicated, false
    ));
}

#[test]
fn failed_first_process_entry_is_not_a_valid_snapshot() {
    assert!(valid_process_snapshot(false, vec![process(10, 1, "zsh", "zsh")]).is_none());
    assert!(valid_process_snapshot(true, Vec::new()).is_none());
}

#[test]
fn the_foreground_probe_records_process_rank_idle_evidence() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "foreground-probe-records-evidence";
    let silence = probe_evidence_fixture(&state, session_id);

    // Nothing has been reconciled yet: the probe arms itself and holds the
    // turn open, contributing no evidence at all.
    let armed = try_timer_idle_transition(
        &state,
        &silence,
        session_id,
        AgentScreenActivity::Ready,
        Some("codex"),
        Some(0),
    );
    assert!(!armed.transitioned);
    assert!(
        armed.evidence.is_none(),
        "an unreconciled probe must not produce evidence"
    );

    // A snapshot in which the agent stands alone — no meaningful descendant.
    state
        .process_snapshot_cache
        .store(Some(vec![process(10, 1, "codex", "codex")]));
    assert!(refresh_background_work_from_cached_snapshot(
        &state,
        session_id,
        10,
        "codex",
        0,
        state.process_snapshot_cache.load(),
    ));
    assert!(
        !state
            .session_maps
            .session_states
            .get(session_id)
            .unwrap()
            .background_work
    );

    let closed = try_timer_idle_transition(
        &state,
        &silence,
        session_id,
        AgentScreenActivity::Ready,
        Some("codex"),
        Some(0),
    );
    assert!(closed.transitioned);
    let evidence = closed.evidence.expect("a close must carry its evidence");
    // These two assertions are the whole point of the story: reduce the probe
    // back to a boolean gate and the turn still closes, but on the ready
    // screen that asked for the probe (`Screen`/`agent-ready-screen`) instead
    // of on the process observation that answered it.
    assert_eq!(
        evidence.rank,
        EvidenceRank::Process,
        "the probe read the process table, so its evidence is Process rank"
    );
    assert_eq!(
        evidence.source, "process",
        "activity_source must name the probe, not the screen, and must stay \
         distinguishable from protocol-stale and from a silence timeout"
    );
}

#[test]
fn a_probe_that_still_sees_work_does_not_claim_process_evidence() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "foreground-probe-still-working";
    let silence = probe_evidence_fixture(&state, session_id);

    assert!(
        !try_timer_idle_transition(
            &state,
            &silence,
            session_id,
            AgentScreenActivity::Ready,
            Some("codex"),
            Some(0),
        )
        .transitioned
    );

    // Same reconciliation, but the tree still holds a build under the agent.
    state.process_snapshot_cache.store(Some(vec![
        process(10, 1, "codex", "codex"),
        process(11, 10, "cargo", "cargo test"),
    ]));
    assert!(refresh_background_work_from_cached_snapshot(
        &state,
        session_id,
        10,
        "codex",
        0,
        state.process_snapshot_cache.load(),
    ));
    assert!(
        state
            .session_maps
            .session_states
            .get(session_id)
            .unwrap()
            .background_work
    );

    let closed = try_timer_idle_transition(
        &state,
        &silence,
        session_id,
        AgentScreenActivity::Ready,
        Some("codex"),
        Some(0),
    );
    // The transition itself is unchanged by this story — a reconciled probe
    // opened the gate before it and still does. What must NOT happen is the
    // close claiming a process observation it did not make.
    assert!(closed.transitioned);
    let evidence = closed.evidence.expect("a close must carry its evidence");
    assert_eq!(evidence.rank, EvidenceRank::Screen);
    assert_eq!(
        evidence.source, "agent-ready-screen",
        "a tree that still shows work says nothing about this turn ending"
    );
}

/// Raising the close from `Screen` to `Process` raises what the NEXT turn has
/// to outrank, and a working screen is only `Screen` rank. The reopen survives
/// because `note_busy_evidence` clears the held idle evidence before
/// `record_busy` meets the rank gate — delete that `clear_idle` and a session
/// closed by the probe can never go busy again without a Protocol-rank signal
/// (#771-4733).
#[test]
fn a_process_rank_close_does_not_strand_the_next_turn_idle() {
    use std::sync::atomic::Ordering;
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "foreground-probe-reopen";
    let silence = probe_evidence_fixture(&state, session_id);

    assert!(
        !try_timer_idle_transition(
            &state,
            &silence,
            session_id,
            AgentScreenActivity::Ready,
            Some("codex"),
            Some(0),
        )
        .transitioned
    );
    state
        .process_snapshot_cache
        .store(Some(vec![process(10, 1, "codex", "codex")]));
    assert!(refresh_background_work_from_cached_snapshot(
        &state,
        session_id,
        10,
        "codex",
        0,
        state.process_snapshot_cache.load(),
    ));
    let closed = try_timer_idle_transition(
        &state,
        &silence,
        session_id,
        AgentScreenActivity::Ready,
        Some("codex"),
        Some(0),
    );
    assert!(closed.transitioned);
    assert_eq!(
        closed.evidence.map(|e| e.rank),
        Some(EvidenceRank::Process),
        "this test is only meaningful after a Process-rank close"
    );

    // No completion and no hook idle, so this is the ordinary `Screen`-rank
    // reopen — the weakest evidence that must still be able to start a turn.
    apply_working_evidence(
        &state,
        &silence,
        session_id,
        now_epoch_ms(),
        "working-screen",
    );

    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(session_id)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_BUSY,
        "a working screen must reopen a turn the process probe closed"
    );
}

// --- Screen verification tests ---

#[test]
fn test_verify_question_on_screen_found() {
    let screen = vec![
        String::new(),
        "Some output".to_string(),
        "Do you want to proceed?".to_string(),
        "⏵⏵ task_name".to_string(),
        String::new(),
    ];
    assert!(verify_question_on_screen(
        &screen,
        "Do you want to proceed?",
        5
    ));
}

#[test]
fn test_verify_question_on_screen_ink_indented() {
    // Ink agents indent text with leading whitespace. extract_question_line
    // captures "  Want me to do that?" (with spaces), screen_rows also has
    // the same. Verification must match despite leading whitespace.
    let screen = vec![
        "⏺ Boss, this is a plan file".to_string(),
        "  Is that right?".to_string(),
        String::new(),
        "  Want me to do that?".to_string(),
        String::new(),
    ];
    // Question stored with leading whitespace from extract_question_line
    assert!(verify_question_on_screen(
        &screen,
        "  Want me to do that?",
        5
    ));
    // Also works if question was stored without whitespace
    assert!(verify_question_on_screen(&screen, "Want me to do that?", 5));
}

#[test]
fn test_verify_question_on_screen_scrolled_away() {
    // Question is NOT among the last 5 rows
    let screen: Vec<String> = (0..24).map(|i| format!("line {i}")).collect();
    assert!(!verify_question_on_screen(
        &screen,
        "Do you want to proceed?",
        5
    ));
}

#[test]
fn test_verify_question_on_screen_empty() {
    let screen: Vec<String> = vec![];
    assert!(!verify_question_on_screen(&screen, "Continue?", 5));
}

#[test]
fn test_verify_question_on_screen_partial_match() {
    let screen = vec![
        "This is not a question? but has more text".to_string(),
        String::new(),
    ];
    // The stored question is just "question?" — substring should not match
    assert!(!verify_question_on_screen(&screen, "question?", 5));
}

#[test]
fn current_chat_question_distinguishes_history_from_missing_prompt_anchor() {
    let rows = vec![
        "Confermi questa rimozione?".to_string(),
        "› si".to_string(),
        "Removed the worktree successfully.".to_string(),
        "› ".to_string(),
    ];
    assert_eq!(
        current_chat_question(&rows),
        CurrentChatQuestion::PromptAnchored(None),
        "later answer and completion must make the old question historical"
    );
    assert_eq!(
        current_chat_question(&["Confermi questa rimozione?".to_string()]),
        CurrentChatQuestion::NoPromptAnchor,
        "headless/incomplete rendering may still use the bounded fallback"
    );
}

#[test]
fn test_silence_state_different_question_after_emitted_does_fire() {
    let mut s = SilenceState::new();
    // First question fires
    s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert!(s.check_silence().is_some());

    // Different question arrives — this IS a new question, must fire
    s.on_chunk(
        false,
        Some("Are you sure?".to_string()),
        false,
        false,
        false,
    );
    s.last_output_at = std::time::Instant::now()
        - SILENCE_QUESTION_THRESHOLD
        - std::time::Duration::from_millis(100);
    assert_eq!(s.check_silence(), Some("Are you sure?".to_string()));
}

// --- find_last_chat_question tests ---

fn screen(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|s| s.to_string()).collect()
}

#[test]
fn test_find_last_chat_question_basic() {
    let rows = screen(&[
        "Do you want to proceed?",
        "",
        "────────────────────────────────",
        "> ",
        "────────────────────────────────",
        "⏵⏵ bypass permissions on",
    ]);
    assert_eq!(
        find_last_chat_question(&rows),
        Some("Do you want to proceed?".to_string()),
    );
}

#[test]
fn test_find_last_chat_question_trailing_disclaimer_blocks_detection() {
    // When the agent emits trailing text AFTER the suggest block (e.g.
    // Claude Code's "(stopping here — waiting for your answer)" footer),
    // the last chat line is the disclaimer, not the question. We
    // deliberately do NOT scavenge past it — accepting this edge case
    // false negative in exchange for not crossing the agent-turn boundary
    // and matching the user's own previous `?`-ending input.
    let rows = screen(&[
        "⏺ TUICommander v1.0.2 is connected.",
        "  intent: await handshake then relay fixed response (Await ACK)",
        "  Do you want me to proceed with this fix?",
        "  suggest: 1) Screenshot overview panel | 2) Fix suggest scroll flicker | 3)",
        "   Fix Cmd+Shift+M keybinding collision | 4) Manual test OSC 133",
        "  (stopping here — waiting for your answer)",
        "────────",
        "❯ ",
        "────────",
        "  [Opus 4.6 | Max]",
        "  ⏵⏵ bypass permissions on",
    ]);
    assert_eq!(find_last_chat_question(&rows), None);
}

#[test]
fn test_find_last_chat_question_does_not_cross_previous_input() {
    // The user previously typed `tutto ok?` (ending with a `?`), the agent
    // replied with a plain statement, then arrives at an empty prompt.
    // The walker MUST NOT scavenge past the agent statement to pick up
    // the user's own prior input — doing so fires a phantom question
    // notification 10s after the reply.
    let rows = screen(&[
        "❯ tutto ok?",
        "────────",
        "⏺ Sì, tutto funziona correttamente.",
        "  Il fix è stato verificato.",
        "────────",
        "❯ ",
        "────────",
        "  ⏵⏵ bypass permissions on",
    ]);
    assert_eq!(find_last_chat_question(&rows), None);
}

#[test]
fn test_find_last_chat_question_skips_wrapped_suggest_block() {
    // Wrapped suggest between question and prompt must not block detection.
    let rows = screen(&[
        "Should I implement this approach?",
        "suggest: 1) Opzione A | 2) Opzione B | 3) Opzione molto lunga che continua",
        "su una seconda riga | 4) Quarta opzione",
        "────────────────────────────────",
        "> ",
        "────────────────────────────────",
        "⏵⏵ bypass permissions on",
    ]);
    assert_eq!(
        find_last_chat_question(&rows),
        Some("Should I implement this approach?".to_string()),
    );
}

#[test]
fn wrapped_suggest_question_is_not_an_agent_question() {
    let rows = screen(&[
        "The work is complete.",
        "suggest: [ Inspect the report | Review the changes |",
        "  Chi ha lanciato powermetrics?",
        "────────────────────────────────",
        "> ",
    ]);
    assert_eq!(find_last_chat_question(&rows), None);
    assert_eq!(
        extract_question_line(&[
            ChangedRow {
                row_index: 1,
                text: "suggest: [ Inspect the report | Review the changes |".into(),
            },
            ChangedRow {
                row_index: 2,
                text: "  Chi ha lanciato powermetrics?".into(),
            },
        ]),
        None,
        "a wrapped suggest item must not arm the silence fallback"
    );
}

#[test]
fn suggest_following_real_question_preserves_question_candidate() {
    let changed = [
        ChangedRow {
            row_index: 0,
            text: "Should I proceed?".into(),
        },
        ChangedRow {
            row_index: 1,
            text: "suggest: [ Review it | Inspect the report |".into(),
        },
        ChangedRow {
            row_index: 2,
            text: "  Chi ha lanciato powermetrics?".into(),
        },
    ];
    assert_eq!(
        extract_question_line(&changed),
        Some("Should I proceed?".into())
    );
}

#[test]
fn closing_suggest_does_not_hide_a_later_question() {
    let rows = screen(&[
        "suggest: [ Inspect the report | Review the changes |",
        "  Chi ha lanciato powermetrics? ]",
        "Should I proceed?",
        "────────────────────────────────",
        "> ",
    ]);
    assert_eq!(
        find_last_chat_question(&rows),
        Some("Should I proceed?".into())
    );
}

#[test]
fn ordinary_question_with_protocol_punctuation_remains_visible() {
    let question = "Should I use [safe] mode | continue?";
    let rows = screen(&[question, "────────────────────────────────", "> "]);
    assert_eq!(find_last_chat_question(&rows), Some(question.into()));
    assert_eq!(
        extract_question_line(&[ChangedRow {
            row_index: 0,
            text: question.into(),
        }]),
        Some(question.into())
    );
}

#[tokio::test(flavor = "current_thread")]
async fn hooked_claude_keeps_suggestions_distinct_from_real_questions() {
    let sid = "hooked-claude-suggest-question";
    let state = accumulating_state(sid);
    agent_session(&state, sid, SHELL_IDLE);
    {
        let mut session = state.session_maps.session_states.get_mut(sid).unwrap();
        session.agent_type = Some("claude".into());
        session.hook_instrumented = true;
    }
    let capture = crate::pty_capture::decode_capture(&agent_prompt_fixture(
        "claude-wrapped-suggest-question-synthetic.tcap",
    ))
    .expect("valid synthetic PTY capture");
    let (rows, cols) = capture.geometry.expect("recorded terminal geometry");
    state
        .grid
        .vt_log_buffers
        .insert(sid.into(), Mutex::new(VtLogBuffer::new(rows, cols, 2000)));
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
    let mut processor = ChunkProcessor::new(None, None);
    for record in capture.records {
        processor.process_chunk(
            std::str::from_utf8(&record.data).unwrap(),
            &silence,
            sid,
            &state,
        );
    }
    assert!(
        !silence.lock().hook_state_seen,
        "configured hooks need a runtime marker before heuristic suppression"
    );
    let screen = state
        .grid
        .vt_log_buffers
        .get(sid)
        .unwrap()
        .lock()
        .screen_rows();
    assert_eq!(
        current_chat_question(&screen),
        CurrentChatQuestion::PromptAnchored(None)
    );
    assert!(
        !state
            .session_maps
            .session_states
            .get(sid)
            .unwrap()
            .awaiting_input
    );

    // A plain question may be pending before hooks start. The next submitted
    // prompt and observed busy marker must clear its badge.
    state.emit_pty_event(heuristic_question(sid, "Shall I continue?"));
    assert!(await_session(&state, sid, |s| s.awaiting_input).await);
    note_submitted_input(&state, sid);
    processor.process_chunk("\x1b]7770;state=busy\x07", &silence, sid, &state);
    assert!(await_session(&state, sid, |s| !s.awaiting_input).await);

    // AskUserQuestion still has an authoritative awaiting signal.
    processor.process_chunk("\x1b]7770;state=awaiting\x07", &silence, sid, &state);
    assert!(
        await_session(&state, sid, |s| s.awaiting_input && s.question_confident).await,
        "AskUserQuestion must report a confident question"
    );
}

#[test]
fn test_find_last_chat_question_no_question() {
    // Agent statement (not a question) above prompt → None.
    let rows = screen(&[
        "I have completed the refactor.",
        "",
        "────────────────────────────────",
        "> ",
        "────────────────────────────────",
        "⏵⏵ bypass permissions on",
    ]);
    assert_eq!(find_last_chat_question(&rows), None);
}

#[test]
fn test_find_last_chat_question_only_checks_first_chat_line() {
    // With multiple chat lines above the prompt, only the immediately
    // preceding one is considered — even if an older line ends with `?`.
    let rows = screen(&[
        "Old question from earlier?",
        "Here is some context.",
        "Do you agree with this plan?",
        "",
        "────────────────────────────────",
        "> ",
        "────────────────────────────────",
        "⏵⏵ bypass permissions on",
    ]);
    // Last chat line is the empty line (skipped), then "Do you agree…?" → detected
    assert_eq!(
        find_last_chat_question(&rows),
        Some("Do you agree with this plan?".to_string()),
    );
}

#[test]
fn test_find_last_chat_question_non_question_last_line_blocks() {
    // If the last chat line above the prompt is not a question, we do NOT
    // keep walking upward to find an older question.
    let rows = screen(&[
        "Shall I proceed?",
        "Here is some unrelated follow-up text.",
        "────────────────────────────────",
        "> ",
        "────────────────────────────────",
        "⏵⏵ bypass permissions on",
    ]);
    assert_eq!(find_last_chat_question(&rows), None);
}

#[test]
fn test_find_last_chat_question_rejects_code_syntax() {
    // `?` in code syntax must not be treated as a question.
    let rows = screen(&[
        "let x = map.get(&key)?",
        "",
        "────────────────────────────────",
        "> ",
        "────────────────────────────────",
        "⏵⏵ bypass permissions on",
    ]);
    assert_eq!(find_last_chat_question(&rows), None);
}

#[test]
fn test_find_last_chat_question_codex_layout() {
    // Codex has no separator lines — the walk must still find the question.
    let rows = screen(&[
        "Do you want me to proceed?",
        "",
        "› ",
        "",
        "  gpt-5.3-codex high · 100% left · ~/project",
    ]);
    assert_eq!(
        find_last_chat_question(&rows),
        Some("Do you want me to proceed?".to_string()),
    );
}

// --- extract_question_line content filter tests ---

fn make_rows(texts: &[&str]) -> Vec<ChangedRow> {
    texts
        .iter()
        .enumerate()
        .map(|(i, t)| ChangedRow {
            row_index: i,
            text: t.to_string(),
        })
        .collect()
}

#[test]
fn test_extract_question_line_rejects_code_comment() {
    let rows = make_rows(&["// What is this?"]);
    assert_eq!(extract_question_line(&rows), None);
}

#[test]
fn test_extract_question_line_rejects_markdown_header() {
    let rows = make_rows(&["## FAQ?"]);
    assert_eq!(extract_question_line(&rows), None);
}

#[test]
fn test_extract_question_line_rejects_diff_context() {
    assert_eq!(extract_question_line(&make_rows(&["+  if x?"])), None);
    assert_eq!(extract_question_line(&make_rows(&["-  if x?"])), None);
    assert_eq!(extract_question_line(&make_rows(&[">  quoted?"])), None);
}

#[test]
fn test_extract_question_line_rejects_numbered_diff_context() {
    assert_eq!(
        extract_question_line(&make_rows(&["1 +Run the following release checklist?"])),
        None,
        "a numbered diff row must not seed awaiting-question silence detection"
    );
}

#[test]
fn test_extract_question_line_rejects_code_syntax() {
    assert_eq!(
        extract_question_line(&make_rows(&["fn foo() -> Option<bool>?"])),
        None
    );
    assert_eq!(
        extract_question_line(&make_rows(&["map.entry(key)?"])),
        None
    );
    assert_eq!(extract_question_line(&make_rows(&["let x = a::b?"])), None);
}

#[test]
fn test_extract_question_line_accepts_real_question() {
    let rows = make_rows(&["Do you want to proceed?"]);
    assert_eq!(
        extract_question_line(&rows),
        Some("Do you want to proceed?".to_string())
    );
}

#[test]
fn test_extract_question_line_accepts_yn_prompt() {
    // Y/n prompt ends with `]`, not `?` — extract_question_line only matches `?`-ending.
    // The actual question before the Y/n suffix ends with `?`:
    let rows = make_rows(&["Continue?"]);
    assert_eq!(extract_question_line(&rows), Some("Continue?".to_string()));
}

#[test]
fn test_extract_question_line_accepts_short_natural_question() {
    // Boss confirmed: "continuo?" is a valid question
    let rows = make_rows(&["continuo?"]);
    assert_eq!(extract_question_line(&rows), Some("continuo?".to_string()));
}

#[test]
fn test_extract_question_line_rejects_asterisk_comment() {
    let rows = make_rows(&["* What is this?"]);
    assert_eq!(extract_question_line(&rows), None);
}

#[test]
fn test_extract_question_line_accepts_parenthetical_options() {
    let rows = make_rows(&["Continue (yes/no)?"]);
    assert_eq!(
        extract_question_line(&rows),
        Some("Continue (yes/no)?".to_string())
    );
}

#[test]
fn test_extract_question_line_accepts_yn_parens() {
    let rows = make_rows(&["Procedo (s/n)?"]);
    assert_eq!(
        extract_question_line(&rows),
        Some("Procedo (s/n)?".to_string())
    );
}

#[test]
fn test_extract_question_line_accepts_option_prompt() {
    let rows = make_rows(&["Apply changes (y)?"]);
    assert_eq!(
        extract_question_line(&rows),
        Some("Apply changes (y)?".to_string())
    );
}

#[test]
fn test_extract_question_line_rejects_rust_try() {
    assert_eq!(extract_question_line(&make_rows(&["foo.bar()?"])), None);
}

#[test]
fn test_extract_question_line_rejects_generic_try() {
    // Also caught by `::` filter
    assert_eq!(extract_question_line(&make_rows(&["Vec::new()?"])), None);
}

#[test]
fn test_extract_question_line_rejects_method_chain_try() {
    assert_eq!(
        extract_question_line(&make_rows(&["iter().map(|x| x)?"])),
        None
    );
}

// --- Prompt-prefixed user input rejection ---

#[test]
fn test_extract_question_line_rejects_claude_prompt() {
    assert_eq!(extract_question_line(&make_rows(&["❯ tutto ok?"])), None);
}

#[test]
fn test_extract_question_line_rejects_codex_prompt() {
    assert_eq!(
        extract_question_line(&make_rows(&["› is this done?"])),
        None
    );
}

#[test]
fn test_extract_question_line_rejects_gemini_prompt() {
    assert_eq!(
        extract_question_line(&make_rows(&["> are you sure?"])),
        None
    );
}

// --- Resize grace period tests ---

#[test]
fn test_resize_grace_active_immediately_after_resize() {
    let mut s = SilenceState::new();
    s.on_resize();
    assert!(
        s.is_resize_grace(),
        "grace period should be active right after resize"
    );
}

#[test]
fn test_resize_grace_inactive_before_resize() {
    let s = SilenceState::new();
    assert!(
        !s.is_resize_grace(),
        "grace period should be inactive with no resize"
    );
}

/// The grace re-arm reads "the durable log did not grow" as "this chunk was a
/// SIGWINCH repaint". In the ALTERNATE screen that reading is always wrong:
/// `VtLogBuffer::process` skips log capture entirely while alt is active, so
/// `total_lines()` is frozen no matter how much the agent writes. Every chunk
/// therefore re-armed the grace, and one resize suppressed low-confidence
/// questions, rate-limit and API-error events plus the BUSY transition until
/// the agent went quiet for a full second.
///
/// The probe reads `last_resize_at` directly instead of sleeping out the
/// window: the re-arm IS that assignment, so whether the stamp moved is the
/// behaviour, not a proxy for it.
///
/// This latch cannot become a `.tcap` fixture. A capture records PTY output
/// and user input bytes, and `replay_capture` drives `VtLogBuffer::process` +
/// `raw_stream_events` + `parse_clean_lines` with no `SilenceState` at all —
/// but the trigger here is `resize_pty` calling `on_resize()`, which is
/// out-of-band and appears nowhere in the byte stream. The grace is only
/// reachable through `process_chunk`, so that is where the test has to sit.
#[test]
fn resize_grace_re_arms_only_on_a_repaint_never_on_agent_output() {
    use crate::state::VtLogBuffer;
    use std::sync::atomic::AtomicU64;

    /// Feeds `prelude` then `chunk` through the real `process_chunk` with the
    /// grace armed and 200 ms left to run, and reports whether `chunk`
    /// pushed the grace deadline forward.
    fn re_armed_by(sid: &str, prelude: &str, chunk: &str) -> bool {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let silence = Arc::new(Mutex::new(SilenceState::new()));
        state
            .session_maps
            .silence_states
            .insert(sid.to_string(), silence.clone());
        state.session_maps.shell_states.insert(
            sid.to_string(),
            std::sync::atomic::AtomicU8::new(SHELL_NULL),
        );
        // Six rows: a screenful plus one line is enough to scroll and grow
        // the durable log, which is what "real output" means in primary.
        state
            .grid
            .vt_log_buffers
            .insert(sid.to_string(), Mutex::new(VtLogBuffer::new(6, 40, 1000)));
        state
            .session_maps
            .output_buffers
            .insert(sid.to_string(), Mutex::new(OutputRingBuffer::new(4096)));
        state
            .session_maps
            .last_output_ms
            .insert(sid.to_string(), AtomicU64::new(0));

        let mut cp = ChunkProcessor::new(None, None);
        cp.process_chunk(prelude, &silence, sid, &state);

        // Arm the grace with 200 ms left: a re-arm is visible as a moved
        // stamp, and no sleep is needed to tell the two apart.
        let armed_at =
            std::time::Instant::now() - RESIZE_GRACE + std::time::Duration::from_millis(200);
        silence.lock().last_resize_at = Some(armed_at);
        assert!(
            silence.lock().is_resize_grace(),
            "precondition: the grace must still be running when the chunk lands"
        );

        cp.process_chunk(chunk, &silence, sid, &state);
        silence.lock().last_resize_at != Some(armed_at)
    }

    // Primary screen, pure repaint: no new line scrolled in, so this is the
    // post-SIGWINCH reflow the extension exists for. It must still re-arm.
    assert!(
        re_armed_by(
            "grace-primary-repaint",
            "one\r\ntwo\r\n",
            "\x1b[H\x1b[2Kone"
        ),
        "a primary-screen repaint must still extend the grace"
    );

    // Primary screen, real output: eight lines on a six-row screen scroll the
    // oldest into the durable log. Genuine work must end the extension.
    assert!(
        !re_armed_by(
            "grace-primary-growth",
            "boot\r\n",
            "l1\r\nl2\r\nl3\r\nl4\r\nl5\r\nl6\r\nl7\r\nl8\r\n"
        ),
        "growing output must not extend the grace"
    );

    // Alternate screen, the same real output. The log cannot grow here, so
    // the unfixed check calls it a repaint and latches the grace forever.
    assert!(
        !re_armed_by(
            "grace-alt-output",
            "\x1b[?1049h",
            "l1\r\nl2\r\nl3\r\nl4\r\nl5\r\nl6\r\nl7\r\nl8\r\n"
        ),
        "alternate-screen output must not extend the grace — the durable log \
             is frozen there, so a frozen total is not evidence of a repaint"
    );
}

// --- Startup grace period tests ---

#[test]
fn test_startup_grace_active_on_new_session() {
    let s = SilenceState::new();
    assert!(
        s.is_startup_grace(),
        "startup grace should be active on new session"
    );
}

#[test]
fn test_startup_grace_settles_after_silence() {
    let mut s = SilenceState::new();
    // Simulate output stopping long enough ago
    s.last_output_at =
        std::time::Instant::now() - STARTUP_SETTLE_SILENCE - std::time::Duration::from_millis(100);
    s.check_startup_settle();
    assert!(
        !s.is_startup_grace(),
        "startup grace should end after output silence"
    );
}

#[test]
fn test_startup_grace_persists_during_output() {
    let mut s = SilenceState::new();
    // Output is recent — grace should persist
    s.last_output_at = std::time::Instant::now();
    s.check_startup_settle();
    assert!(
        s.is_startup_grace(),
        "startup grace should persist while output is flowing"
    );
}

// --- VtLogBuffer + parse_clean_lines pipeline tests ---

/// VtLogBuffer changed rows feed parse_clean_lines and produce a StatusLine event
/// for normal screen output.
#[test]
fn test_vt_log_pipeline_status_line_normal_screen() {
    use crate::output_parser::{OutputParser, ParsedEvent};
    use crate::state::VtLogBuffer;

    let mut vt_log = VtLogBuffer::new(24, 80, 1000);
    let mut parser = OutputParser::new();

    let changed = vt_log.process(b"* Reading files...");
    let events = parser.parse_clean_lines(&changed, true);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ParsedEvent::StatusLine { .. })),
        "expected StatusLine from normal screen, got: {:?}",
        events
    );
}

/// The production chunk pipeline captures an alternate-screen intent.
#[cfg(unix)]
#[test]
fn test_vt_log_pipeline_intent_alternate_screen() {
    let (events, entries) =
        run_progress_intent_case(&["intent: Doing work (Test)"], 80, true, false);
    assert_eq!(events, [("Doing work".into(), Some("Test".into()))]);
    assert_eq!(entries, ["Doing work"]);
}

/// parse_osc94 is called on raw data (OSC 9;4 is invisible in clean rows).
#[test]
fn test_osc94_from_raw_stream() {
    use crate::output_parser::{ParsedEvent, parse_osc94};

    let raw = "\x1b]9;4;1;50\x07"; // OSC 9;4 progress 50%
    let event = parse_osc94(raw);
    assert!(
        matches!(event, Some(ParsedEvent::Progress { .. })),
        "expected Progress from raw OSC 9;4, got: {:?}",
        event
    );
}

/// extract_question_line finds `?`-ending rows from VtLogBuffer output.
#[test]
fn test_extract_question_line_basic() {
    use crate::state::VtLogBuffer;

    let mut vt_log = VtLogBuffer::new(24, 80, 1000);
    let changed = vt_log.process(b"Would you like to proceed?");
    assert_eq!(
        extract_question_line(&changed).as_deref(),
        Some("Would you like to proceed?")
    );
}

/// Question row must be found even when a mode line with a higher row index
/// arrives in the same chunk (e.g. Claude Code question + ⏵⏵ status line).
#[test]
fn test_extract_question_line_with_mode_line_same_chunk() {
    use crate::state::VtLogBuffer;

    let mut vt_log = VtLogBuffer::new(24, 80, 1000);
    let data = b"Le committo?\r\n\r\n\xe2\x8f\xb5\xe2\x8f\xb5 Reading files";
    let changed = vt_log.process(data);
    assert_eq!(
        extract_question_line(&changed).as_deref(),
        Some("Le committo?"),
        "question must be found even when mode line is on a later row; changed_rows: {:?}",
        changed
            .iter()
            .map(|r| format!("[{}] {:?}", r.row_index, r.text))
            .collect::<Vec<_>>()
    );
}

/// Question must be found in alternate screen with cursor-positioned rows.
#[test]
fn test_extract_question_line_alternate_screen() {
    use crate::state::VtLogBuffer;

    let mut vt_log = VtLogBuffer::new(24, 80, 1000);
    let _ = vt_log.process(b"\x1b[?1049h");
    let data = b"\x1b[5;1HDo you want to proceed?\x1b[23;1H* Thinking...";
    let changed = vt_log.process(data);
    assert_eq!(
        extract_question_line(&changed).as_deref(),
        Some("Do you want to proceed?"),
        "question must be found in alternate screen; changed_rows: {:?}",
        changed
            .iter()
            .map(|r| format!("[{}] {:?}", r.row_index, r.text))
            .collect::<Vec<_>>()
    );
}

// --- Headless reader structured event tests ---

/// A title-less marker reaches the journal at the idle boundary.
#[cfg(unix)]
#[test]
fn test_headless_reader_intent_event_logic() {
    let (events, entries) =
        run_progress_intent_case(&["intent: Testing headless reader"], 80, true, true);
    assert_eq!(events, [("Testing headless reader".into(), None)]);
    assert_eq!(entries, ["Testing headless reader"]);
}

/// The headless reader emits events for alternate screen content (e.g. Claude Code).
#[test]
fn test_headless_reader_alternate_screen_events() {
    use crate::output_parser::{OutputParser, ParsedEvent};
    use crate::state::VtLogBuffer;

    let mut vt_log = VtLogBuffer::new(24, 80, 1000);
    let mut parser = OutputParser::new();

    let _ = vt_log.process(b"\x1b[?1049h"); // enter alternate screen
    let changed = vt_log.process(b"* Reading files...");
    let events = parser.parse_clean_lines(&changed, true);

    assert!(
        events
            .iter()
            .any(|e| matches!(e, ParsedEvent::StatusLine { .. })),
        "headless reader must detect StatusLine during alternate screen, got: {:?}",
        events
    );
}

// --- Escape sequence handling diagnostics (using TerminalGrid) ---

/// Verify that `\x1b[<n>F` (CPL — Cursor Previous Line) is handled
/// and does NOT leak parameter digits into screen cell text.
#[test]
fn test_cpl_sequence_does_not_leak() {
    let mut grid = crate::terminal_grid::TerminalGrid::new(24, 80, 0);
    grid.process(b"\n");
    grid.process(b"old content here\n");
    grid.process(b"\x1b[1F");
    grid.process(b"new content");
    let row1 = grid.get_row_text(1);
    assert_eq!(
        row1.trim_end(),
        "new content here",
        "CPL should move cursor up; row1 = {:?}",
        row1
    );
    assert!(
        !row1.contains("1F"),
        "escape param '1F' leaked into screen text: {:?}",
        row1
    );
}

/// Verify that `\x1b[<n>E` (CNL — Cursor Next Line) is handled.
#[test]
fn test_cnl_sequence_does_not_leak() {
    let mut grid = crate::terminal_grid::TerminalGrid::new(24, 80, 0);
    grid.process(b"line0");
    grid.process(b"\x1b[1E");
    grid.process(b"line1");
    let row0 = grid.get_row_text(0);
    let row1 = grid.get_row_text(1);
    assert_eq!(
        row0.trim_end(),
        "line0",
        "row0 should be unchanged; got {:?}",
        row0
    );
    assert_eq!(
        row1.trim_end(),
        "line1",
        "CNL should move cursor down; got {:?}",
        row1
    );
}

/// Ink's CPL overwrite reaches the same chunk pipeline as a live PTY.
#[cfg(unix)]
#[test]
fn test_vt100_ink_style_intent_with_cpl() {
    let (events, entries) = run_progress_intent_case(
        &[
            "placeholder text\r\n",
            "\x1b[1F\x1b[2Kintent: Fix all 34 documentation gaps (Fixing gaps)",
        ],
        80,
        true,
        false,
    );
    assert_eq!(
        events,
        [(
            "Fix all 34 documentation gaps".into(),
            Some("Fixing gaps".into())
        )]
    );
    assert_eq!(entries, ["Fix all 34 documentation gaps"]);
}

/// Chunked delivery: CSI split across two process() calls.
/// Verifies the vt100 parser buffers incomplete escapes correctly.
#[test]
fn test_vt100_chunked_csi_does_not_leak() {
    use crate::state::VtLogBuffer;

    let mut vt_log = VtLogBuffer::new(24, 80, 1000);
    let _ = vt_log.process(b"\x1b[?1049h"); // alternate screen
    let _ = vt_log.process(b"old line\r\n");

    // Chunk 1: partial CSI (just the introducer)
    let changed1 = vt_log.process(b"\x1b[");
    // Chunk 2: parameter + final byte completing CPL, then text
    let changed2 = vt_log.process(b"1Fintent: Fix all gaps");

    // Check that no row contains literal "1F" as text
    for row in changed1.iter().chain(changed2.iter()) {
        assert!(
            !row.text.contains("1F"),
            "chunked CSI leaked '1F' into row text: {:?}",
            row.text
        );
    }
}

/// Test what happens when CSI is aborted by an unexpected byte.
#[test]
fn test_aborted_csi_does_not_leak_digits() {
    let mut grid = crate::terminal_grid::TerminalGrid::new(24, 80, 0);
    // \x1b[1\x1b[2K — the first CSI is aborted by the second ESC
    grid.process(b"\x1b[1\x1b[2KHello");
    let row = grid.get_row_text(0);
    eprintln!("aborted CSI row: {:?}", row);
    assert!(
        !row.starts_with('1'),
        "aborted CSI parameter '1' should not appear in cell text: {:?}",
        row
    );
}

/// Test that unknown private CSI sequences don't leak.
#[test]
fn test_unknown_private_csi_does_not_leak() {
    let mut grid = crate::terminal_grid::TerminalGrid::new(24, 80, 0);
    // \x1b[?1234z — fictional private sequence with unknown final byte 'z'
    grid.process(b"\x1b[?1234zVisible text");
    let row = grid.get_row_text(0);
    eprintln!("unknown private CSI row: {:?}", row);
    assert_eq!(
        row.trim_end(),
        "Visible text",
        "unknown private CSI should not leak; got: {:?}",
        row
    );
}

/// Simulate realistic Ink output with SGR + cursor movement + text.
/// This mimics what Claude Code actually sends through the PTY.
#[cfg(unix)]
#[test]
fn test_vt100_realistic_ink_render_cycle() {
    let (events, entries) = run_progress_intent_case(
        &[
            "\x1b[1;1H\x1b[38;2;128;128;128m●\x1b[0m \x1b[1mintent: Reading codebase structure (Reading code)\x1b[0m",
            "\x1b[1F\x1b[2K\x1b[38;2;128;128;128m●\x1b[0m \x1b[1mintent: Fix all 34 documentation gaps (Fixing gaps)\x1b[0m",
        ],
        80,
        true,
        false,
    );
    assert_eq!(
        events,
        [
            (
                "Reading codebase structure".into(),
                Some("Reading code".into())
            ),
            (
                "Fix all 34 documentation gaps".into(),
                Some("Fixing gaps".into())
            )
        ]
    );
    assert_eq!(
        entries,
        [
            "Fix all 34 documentation gaps",
            "Reading codebase structure"
        ]
    );
}

/// Multi-chunk Ink render: data arrives in small fragments.
#[test]
fn test_vt100_fragmented_ink_output() {
    use crate::state::VtLogBuffer;

    let mut vt_log = VtLogBuffer::new(24, 80, 1000);
    let _ = vt_log.process(b"\x1b[?1049h");

    // Simulate fragmented delivery of: \x1b[1F\x1b[2Kintent: Fix all gaps
    let fragments: Vec<&[u8]> = vec![
        b"\x1b[", // CSI introducer
        b"1",     // parameter
        b"F",     // final byte (CPL)
        b"\x1b[", // CSI introducer
        b"2K",    // erase line
        b"intent: Fix all gaps",
    ];

    let mut all_changed = Vec::new();
    for frag in fragments {
        let changed = vt_log.process(frag);
        all_changed.extend(changed);
    }

    // Check no row contains '1F' leak
    for row in &all_changed {
        eprintln!("fragmented row[{}]: {:?}", row.row_index, row.text);
        assert!(
            !row.text.contains("1F"),
            "fragmented delivery leaked '1F': {:?}",
            row.text
        );
    }
}

// --- Shell state transition tests ---

#[test]
fn test_shell_state_busy_on_real_output() {
    use std::sync::atomic::{AtomicU8, Ordering};
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "test-session";
    state
        .session_maps
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_NULL));
    state.session_maps.last_output_ms.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU64::new(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        ),
    );

    // Transition null → busy
    assert!(
        try_shell_transition(&state, sid, SHELL_NULL, SHELL_BUSY, true),
        "should transition null → busy"
    );
    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(sid)
            .unwrap()
            .load(Ordering::Relaxed),
        SHELL_BUSY
    );

    // Transition busy → busy should fail (already busy, no re-emit)
    assert!(
        !try_shell_transition(&state, sid, SHELL_NULL, SHELL_BUSY, true),
        "should NOT re-transition to busy"
    );
}

#[test]
fn test_shell_state_idle_after_500ms() {
    use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "test-session";
    state
        .session_maps
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));
    state
        .session_maps
        .session_states
        .insert(sid.to_string(), crate::state::SessionState::default());

    // Set last output to 600ms ago (> SHELL_IDLE_MS)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    state
        .session_maps
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(now - 600));

    assert!(
        should_transition_idle(&state, sid).should_transition,
        "should be ready to transition idle (600ms elapsed, no sub-tasks)"
    );
    assert!(
        try_shell_transition(&state, sid, SHELL_BUSY, SHELL_IDLE, true),
        "should transition busy → idle"
    );
    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(sid)
            .unwrap()
            .load(Ordering::Relaxed),
        SHELL_IDLE
    );
}

#[test]
fn test_shell_state_no_idle_with_subtasks() {
    use std::sync::atomic::{AtomicU8, AtomicU64};
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "test-session";
    state
        .session_maps
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));

    state.session_maps.session_states.insert(
        sid.to_string(),
        crate::state::SessionState {
            active_sub_tasks: 2,
            ..Default::default()
        },
    );

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    state
        .session_maps
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(now - 600));

    assert!(
        !should_transition_idle(&state, sid).should_transition,
        "should NOT transition idle when active_sub_tasks > 0 and elapsed < SUBTASK_STALE_MS"
    );
}

#[test]
fn test_shell_state_idle_stale_subtasks_force_cleared() {
    use std::sync::atomic::{AtomicU8, AtomicU64};
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "test-session";
    state
        .session_maps
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));

    state.session_maps.session_states.insert(
        sid.to_string(),
        crate::state::SessionState {
            active_sub_tasks: 2,
            ..Default::default()
        },
    );

    // Set last output to 31s ago (> SUBTASK_STALE_MS)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    state
        .session_maps
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(now - 31_000));

    assert!(
        should_transition_idle(&state, sid).should_transition,
        "should transition idle when active_sub_tasks > 0 but elapsed >= SUBTASK_STALE_MS"
    );
    // Verify the stale counter was force-cleared
    let sub = state
        .session_maps
        .session_states
        .get(sid)
        .map(|s| s.active_sub_tasks)
        .unwrap_or(999);
    assert_eq!(sub, 0, "active_sub_tasks should be force-cleared to 0");
}

// ---- Activity pulse (story 625-56b0) ----
//
// What these protect: commit cda39f31 deleted the `pty-output` emit and left
// the frontend listener subscribed to it, so desktop `lastDataAt` and the
// background-tab unread flag silently froze for a commit. Nothing failed,
// because no test tied a producer to a consumer.
//
// LIMIT, stated rather than papered over: these drive `ActivityPulse`
// directly. They prove the pulse throttles and reaches the bus, and the
// frontend suite (`transport.test.ts`) proves both transports route the
// event to `onActivity`. Neither proves the PTY reader loop still CALLS
// `pulse()` — that needs a live PTY, and this crate has no harness that
// spawns one. Deleting the call site would still pass; deleting or renaming
// either end of the signal would not.

/// A session that produces output must announce it on the bus.
#[test]
fn activity_pulse_emits_on_first_output() {
    let state = crate::state::tests_support::make_test_app_state();
    let mut rx = state.event_bus.subscribe();
    let mut pulse = ActivityPulse::new();

    pulse.pulse(&state, "sess-a");

    match rx.try_recv().expect("bus must receive the activity pulse") {
        crate::state::AppEvent::PtyActivity { session_id } => {
            assert_eq!(session_id, "sess-a");
        }
        other => panic!("unexpected event variant: {other:?}"),
    }
}

/// Repeated output inside the window collapses to one pulse. Dropping is the
/// intended behaviour here — the signal is payload-free and idempotent, so a
/// suppressed pulse carries nothing a later one does not.
#[test]
fn activity_pulse_suppresses_repeats_inside_window() {
    let state = crate::state::tests_support::make_test_app_state();
    let mut rx = state.event_bus.subscribe();
    let mut pulse = ActivityPulse::new();

    for _ in 0..50 {
        pulse.pulse(&state, "sess-a");
    }

    assert!(
        matches!(
            rx.try_recv(),
            Ok(crate::state::AppEvent::PtyActivity { .. })
        ),
        "first pulse must go out"
    );
    assert!(
        rx.try_recv().is_err(),
        "a burst inside one window must collapse to a single pulse"
    );
}

/// ...but the session must not go quiet forever: once the window has passed,
/// the next chunk pulses again. A latch here would freeze `lastDataAt` at the
/// first byte of a long-running command, which is the bug in a new costume.
#[test]
fn activity_pulse_resumes_after_window() {
    let state = crate::state::tests_support::make_test_app_state();
    let mut rx = state.event_bus.subscribe();
    let mut pulse = ActivityPulse::new();

    pulse.pulse(&state, "sess-a");
    let _ = rx.try_recv();
    // Reach back past the window instead of sleeping through it.
    pulse.last = Some(std::time::Instant::now() - ACTIVITY_PULSE_WINDOW);
    pulse.pulse(&state, "sess-a");

    assert!(
        matches!(
            rx.try_recv(),
            Ok(crate::state::AppEvent::PtyActivity { .. })
        ),
        "a chunk after the window must pulse again"
    );
}

// The matching guarantee — that the pulse must NOT restamp
// `SessionState.last_activity_ms` — is asserted in `state.rs`, next to the
// accumulator that owns that field.

/// Story 1366-2b3e/H1: when the stale-subtasks recovery path force-clears
/// the in-memory counter, the caller must emit ActiveSubtasks{count:0}
/// so the frontend store and notification gate also reset.
#[test]
fn test_force_cleared_subtasks_signal_propagates() {
    use std::sync::atomic::{AtomicU8, AtomicU64};
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "test-session";
    state
        .session_maps
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));
    state.session_maps.session_states.insert(
        sid.to_string(),
        crate::state::SessionState {
            active_sub_tasks: 3,
            ..Default::default()
        },
    );
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    state
        .session_maps
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(now - 31_000));

    let decision = should_transition_idle(&state, sid);
    assert!(
        decision.should_transition,
        "stale path must transition idle"
    );
    assert!(
        decision.force_cleared_subtasks,
        "stale path must signal force-clear so caller emits count=0"
    );

    // Subscribe BEFORE emitting so the broadcast is captured.
    let mut rx = state.event_bus.subscribe();
    emit_active_subtasks(&state, sid, 0, "");

    let event = rx.try_recv().expect("event bus must receive PtyParsed");
    match event {
        crate::state::AppEvent::PtyParsed { session_id, parsed } => {
            assert_eq!(session_id, sid);
            let kind = parsed.get("type").and_then(|v| v.as_str()).unwrap_or("");
            assert_eq!(kind, "active-subtasks", "wrong event variant: {parsed}");
            let count = parsed.get("count").and_then(|v| v.as_u64()).unwrap_or(999);
            assert_eq!(count, 0, "count must be 0 to clear the badge");
        }
        other => panic!("unexpected event variant: {other:?}"),
    }
}

/// Inverse: the normal idle path (no sub-tasks at all) must NOT signal
/// force_cleared_subtasks — otherwise we would emit redundant count=0
/// events on every healthy busy→idle.
#[test]
fn test_normal_idle_does_not_signal_force_clear() {
    use std::sync::atomic::{AtomicU8, AtomicU64};
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "test-session";
    state
        .session_maps
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));
    state
        .session_maps
        .session_states
        .insert(sid.to_string(), crate::state::SessionState::default());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    state
        .session_maps
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(now - 600));

    let decision = should_transition_idle(&state, sid);
    assert!(decision.should_transition);
    assert!(
        !decision.force_cleared_subtasks,
        "no-sub-tasks idle must not request a redundant count=0 emission"
    );
}

#[test]
fn test_shell_state_no_idle_agent_session_under_agent_threshold() {
    use std::sync::atomic::{AtomicU8, AtomicU64};
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "test-session";
    state
        .session_maps
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));

    // Agent session: agent_type is set
    state.session_maps.session_states.insert(
        sid.to_string(),
        crate::state::SessionState {
            agent_type: Some("claude".to_string()),
            ..Default::default()
        },
    );

    // 600ms elapsed — would trigger idle for a shell, but NOT for an agent session
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    state
        .session_maps
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(now - 600));

    assert!(
        !should_transition_idle(&state, sid).should_transition,
        "agent session should NOT transition idle at 600ms (under AGENT_IDLE_MS)"
    );
}

#[test]
fn test_shell_state_idle_agent_session_over_agent_threshold() {
    use std::sync::atomic::{AtomicU8, AtomicU64};
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "test-session";
    state
        .session_maps
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));

    // Agent session: agent_type is set
    state.session_maps.session_states.insert(
        sid.to_string(),
        crate::state::SessionState {
            agent_type: Some("claude".to_string()),
            ..Default::default()
        },
    );

    // 3000ms elapsed — over the 2500ms agent threshold
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    state
        .session_maps
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(now - 3000));

    assert!(
        should_transition_idle(&state, sid).should_transition,
        "agent session SHOULD transition idle after agent threshold"
    );
}

#[test]
fn test_shell_state_no_idle_before_500ms() {
    use std::sync::atomic::{AtomicU8, AtomicU64};
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "test-session";
    state
        .session_maps
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));
    state
        .session_maps
        .session_states
        .insert(sid.to_string(), crate::state::SessionState::default());

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    state
        .session_maps
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(now - 200));

    assert!(
        !should_transition_idle(&state, sid).should_transition,
        "should NOT transition idle when only 200ms elapsed"
    );
}

#[test]
fn test_shell_state_cas_prevents_duplicate_idle() {
    use std::sync::atomic::{AtomicU8, Ordering};
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "test-session";
    state
        .session_maps
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));

    // First CAS succeeds
    assert!(try_shell_transition(
        &state, sid, SHELL_BUSY, SHELL_IDLE, true
    ));
    // Second CAS fails (already idle)
    assert!(
        !try_shell_transition(&state, sid, SHELL_BUSY, SHELL_IDLE, true),
        "second idle transition must fail — already idle"
    );
    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(sid)
            .unwrap()
            .load(Ordering::Relaxed),
        SHELL_IDLE
    );
}

#[test]
fn test_shell_state_idle_to_busy_on_real_output() {
    use std::sync::atomic::{AtomicU8, Ordering};
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "test-session";
    state
        .session_maps
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_IDLE));

    assert!(
        try_shell_transition(&state, sid, SHELL_IDLE, SHELL_BUSY, true),
        "should transition idle → busy on real output"
    );
    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(sid)
            .unwrap()
            .load(Ordering::Relaxed),
        SHELL_BUSY
    );
}

// --- Backup idle guard: has_recent_chunks ---

#[test]
fn test_has_recent_chunks_true_after_any_chunk() {
    let mut s = SilenceState::new();
    // Any chunk (including chrome-only) updates last_chunk_at
    s.on_chunk(false, None, true, true, false);
    assert!(
        s.has_recent_chunks(),
        "has_recent_chunks should be true right after any chunk"
    );
}

#[test]
fn test_has_recent_chunks_true_after_real_chunk() {
    let mut s = SilenceState::new();
    s.on_chunk(false, None, false, false, false);
    assert!(
        s.has_recent_chunks(),
        "has_recent_chunks should be true right after a real output chunk"
    );
}

#[test]
fn test_has_recent_chunks_false_when_no_chunks_for_2s() {
    let mut s = SilenceState::new();
    s.on_chunk(false, None, true, true, false);
    // Backdate last_chunk_at to 3 seconds ago
    s.last_chunk_at = std::time::Instant::now() - std::time::Duration::from_secs(3);
    assert!(
        !s.has_recent_chunks(),
        "has_recent_chunks should be false when last chunk was 3s ago"
    );
}

#[test]
fn test_backup_idle_blocked_when_chunks_arriving() {
    use std::sync::atomic::{AtomicU8, AtomicU64};
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "test-session";
    state
        .session_maps
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));
    state
        .session_maps
        .session_states
        .insert(sid.to_string(), crate::state::SessionState::default());

    // last_output_ms is 600ms ago (stale — would normally trigger idle)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    state
        .session_maps
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(now - 600));

    // Any chunk just arrived (real or chrome-only)
    let mut silence = SilenceState::new();
    silence.on_chunk(false, None, false, false, false); // chunk just arrived

    // should_transition_idle says yes (based on last_output_ms alone)
    assert!(
        should_transition_idle(&state, sid).should_transition,
        "should_transition_idle sees stale last_output_ms"
    );
    // But has_recent_chunks blocks the backup timer (recent chunk activity)
    assert!(
        silence.has_recent_chunks(),
        "backup idle must be blocked because chunks are arriving"
    );
}

// Status-line idle transition: covered by test_backup_idle_blocked_by_chrome_only_ticks.
// Status-line ticking proves the agent is alive — the reader thread's !has_status_line
// guard blocks idle, and has_recent_chunks() (using last_chunk_at) blocks the backup timer.

#[test]
fn test_is_spinner_row_distinguishes_spinner_from_static_chrome() {
    // Spinner rows prove agent is alive
    assert!(crate::chrome::is_spinner_row("✻ Cogitated for 3m 47s"));
    assert!(crate::chrome::is_spinner_row("⠋ Generating..."));
    // Tool progress spinners prove agent is alive
    assert!(crate::chrome::is_spinner_row("◐ Bash: .../b..."));
    assert!(crate::chrome::is_spinner_row("◑ Read: src/main.rs"));
    // Static chrome does NOT prove agent is alive
    assert!(!crate::chrome::is_spinner_row("⏵ auto mode"));
    assert!(!crate::chrome::is_spinner_row("▀▀▀▀▀▀▀▀"));
}

// --- ChunkProcessor tests ---

#[test]
fn test_chunk_processor_new_has_correct_defaults() {
    let cp = ChunkProcessor::new(Some("/home/user/repo".to_string()), None);
    assert_eq!(cp.session_cwd, Some("/home/user/repo".to_string()));
    assert!(cp.last_status_task.is_none());
    assert!(cp.last_question_text.is_none());
    assert!(cp.last_choice_prompt_sig.is_none());
}

#[test]
fn test_chunk_processor_dedup_status_task() {
    use crate::state::VtLogBuffer;
    use std::sync::atomic::AtomicU64;

    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "test-cp-dedup";
    let silence = Arc::new(Mutex::new(SilenceState::new()));
    state
        .session_maps
        .silence_states
        .insert(sid.to_string(), silence.clone());
    state.session_maps.shell_states.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_NULL),
    );
    state
        .grid
        .vt_log_buffers
        .insert(sid.to_string(), Mutex::new(VtLogBuffer::new(24, 80, 1000)));
    state
        .session_maps
        .output_buffers
        .insert(sid.to_string(), Mutex::new(OutputRingBuffer::new(4096)));
    state
        .session_maps
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(0));

    let mut cp = ChunkProcessor::new(None, None);
    let mut utf8_buf = Utf8ReadBuffer::new();
    let mut esc_buf = EscapeAwareBuffer::new();

    // First chunk with status line "* Reading files..."
    let raw = b"* Reading files...";
    let utf8_data = utf8_buf.push(raw);
    let esc_data = esc_buf.push(&utf8_data);
    let result1 = cp.process_chunk(&esc_data, &silence, sid, &state);

    // Count how many PtyParsed events were sent with StatusLine
    let mut rx = state.event_bus.subscribe();
    // Second chunk with same status — should be deduped
    let raw2 = b"\r\n* Reading files...";
    let utf8_data2 = utf8_buf.push(raw2);
    let esc_data2 = esc_buf.push(&utf8_data2);
    let _result2 = cp.process_chunk(&esc_data2, &silence, sid, &state);

    // Collect events from the second call
    let mut status_count = 0;
    while let Ok(evt) = rx.try_recv() {
        if let crate::state::AppEvent::PtyParsed { parsed, .. } = evt
            && parsed.get("type").and_then(|t| t.as_str()) == Some("status-line")
        {
            status_count += 1;
        }
    }
    assert_eq!(
        status_count, 0,
        "duplicate StatusLine with same task_name should be deduped"
    );

    // Verify the result contains data
    assert!(result1, "first chunk should report data");
}

/// The api-error dedup must reopen when the user submits a line. The reset
/// used to live in `parse_clean_lines`, keyed on a `ParsedEvent::UserInput`
/// no output parser ever produces — that event is emitted on the INPUT
/// thread by `record_submitted_line` — so the branch was dead and the first
/// API error of a session silenced every later one for the whole session.
#[test]
fn user_submission_rearms_the_api_error_dedup() {
    use crate::state::VtLogBuffer;
    use std::sync::atomic::AtomicU64;

    const API_ERROR: &str = "API Error: 500 Internal server error.\r\n";

    /// Drain the bus and count the api-error notifications it carried.
    fn api_errors(rx: &mut tokio::sync::broadcast::Receiver<crate::state::AppEvent>) -> usize {
        let mut count = 0;
        while let Ok(evt) = rx.try_recv() {
            if let crate::state::AppEvent::PtyParsed { parsed, .. } = evt
                && parsed.get("type").and_then(|t| t.as_str()) == Some("api-error")
            {
                count += 1;
            }
        }
        count
    }

    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "api-error-rearm";
    let silence = Arc::new(Mutex::new(SilenceState::new()));
    // The startup grace drops ApiError outright; this test is about dedup.
    silence.lock().startup_settled = true;
    state
        .session_maps
        .silence_states
        .insert(sid.to_string(), silence.clone());
    state.session_maps.shell_states.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_NULL),
    );
    state
        .grid
        .vt_log_buffers
        .insert(sid.to_string(), Mutex::new(VtLogBuffer::new(24, 80, 1000)));
    state
        .session_maps
        .output_buffers
        .insert(sid.to_string(), Mutex::new(OutputRingBuffer::new(4096)));
    state
        .session_maps
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(0));

    let mut cp = ChunkProcessor::new(None, None);
    let mut rx = state.event_bus.subscribe();

    cp.process_chunk("boot\r\n", &silence, sid, &state);
    cp.process_chunk(API_ERROR, &silence, sid, &state);
    assert_eq!(api_errors(&mut rx), 1, "the first API error must notify");

    // Same error text, same turn: still on screen, so it stays deduped.
    cp.process_chunk("retrying\r\n", &silence, sid, &state);
    cp.process_chunk(API_ERROR, &silence, sid, &state);
    assert_eq!(
        api_errors(&mut rx),
        0,
        "a repaint of the same error inside one turn must stay deduped"
    );

    // The user answers. That is the signal that reopens the dedup.
    record_submitted_line(&state, sid, "try again".to_string(), -1);
    api_errors(&mut rx); // drop the submission bookkeeping events

    cp.process_chunk(API_ERROR, &silence, sid, &state);
    assert_eq!(
        api_errors(&mut rx),
        1,
        "the same API error after a user submission is a NEW failure and must notify"
    );
}

/// A new turn must re-emit its status line even when the task name is
/// identical to the previous turn's. Codex names every turn "Working", so a
/// session-lifetime dedup swallows the status line of every turn after the
/// first. Nothing then clears the prior turn's `suggested_actions`, which
/// `session_state_with_shell` reads as a completion marker — a busy agent is
/// reported completed/idle for the rest of the session.
#[test]
fn test_chunk_processor_status_dedup_is_scoped_to_turn() {
    use crate::state::VtLogBuffer;
    use std::sync::atomic::AtomicU64;

    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "test-cp-dedup-turn";
    let silence = Arc::new(Mutex::new(SilenceState::new()));
    state
        .session_maps
        .silence_states
        .insert(sid.to_string(), silence.clone());
    state.session_maps.shell_states.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_NULL),
    );
    state
        .grid
        .vt_log_buffers
        .insert(sid.to_string(), Mutex::new(VtLogBuffer::new(24, 80, 1000)));
    state
        .session_maps
        .output_buffers
        .insert(sid.to_string(), Mutex::new(OutputRingBuffer::new(4096)));
    state
        .session_maps
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(0));
    state.session_maps.session_states.insert(
        sid.to_string(),
        crate::state::SessionState {
            agent_type: Some("codex".to_string()),
            turn_epoch: 1,
            ..Default::default()
        },
    );

    let mut cp = ChunkProcessor::new(None, None);
    let mut utf8_buf = Utf8ReadBuffer::new();
    let mut esc_buf = EscapeAwareBuffer::new();

    let feed = |cp: &mut ChunkProcessor,
                utf8_buf: &mut Utf8ReadBuffer,
                esc_buf: &mut EscapeAwareBuffer,
                raw: &[u8]|
     -> usize {
        let mut rx = state.event_bus.subscribe();
        let utf8_data = utf8_buf.push(raw);
        let esc_data = esc_buf.push(&utf8_data);
        cp.process_chunk(&esc_data, &silence, sid, &state);
        let mut count = 0;
        while let Ok(evt) = rx.try_recv() {
            if let crate::state::AppEvent::PtyParsed { parsed, .. } = evt
                && parsed.get("type").and_then(|t| t.as_str()) == Some("status-line")
            {
                count += 1;
            }
        }
        count
    };

    let turn1 = feed(
        &mut cp,
        &mut utf8_buf,
        &mut esc_buf,
        "• Working (1s • esc to interrupt)".as_bytes(),
    );
    assert_eq!(turn1, 1, "first turn must emit its status line");

    // Spinner rotation inside the SAME turn stays deduped.
    let same_turn = feed(
        &mut cp,
        &mut utf8_buf,
        &mut esc_buf,
        "\r\n• Working (2s • esc to interrupt)".as_bytes(),
    );
    assert_eq!(same_turn, 0, "spinner rotation within a turn must dedup");

    // The user submits again: a new turn begins.
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .expect("session state")
        .turn_epoch = 2;

    let turn2 = feed(
        &mut cp,
        &mut utf8_buf,
        &mut esc_buf,
        "\r\n• Working (1s • esc to interrupt)".as_bytes(),
    );
    assert_eq!(
        turn2, 1,
        "a new turn must re-emit the status line even with an identical task name"
    );
}

#[test]
fn test_chunk_processor_dedup_choice_prompt() {
    use crate::state::VtLogBuffer;
    use std::sync::atomic::AtomicU64;

    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "test-cp-choice-dedup";
    let silence = Arc::new(Mutex::new(SilenceState::new()));
    state
        .session_maps
        .silence_states
        .insert(sid.to_string(), silence.clone());
    state.session_maps.shell_states.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_NULL),
    );
    state
        .grid
        .vt_log_buffers
        .insert(sid.to_string(), Mutex::new(VtLogBuffer::new(24, 80, 1000)));
    state
        .session_maps
        .output_buffers
        .insert(sid.to_string(), Mutex::new(OutputRingBuffer::new(4096)));
    state
        .session_maps
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(0));

    let mut cp = ChunkProcessor::new(None, None);
    let mut utf8_buf = Utf8ReadBuffer::new();
    let mut esc_buf = EscapeAwareBuffer::new();

    // Paint a Claude Code edit-confirm screen into the terminal.
    let screen_bytes = b"Do you want to make this edit to CLAUDE.md?\r\n\
              \xe2\x9d\xaf 1. Yes\r\n\
              \x20\x20 2. Yes, allow all edits (shift+tab)\r\n\
              \x20\x20 3. No\r\n\
              \r\n\
              Esc to cancel \xc2\xb7 Tab to amend\r\n";
    let utf8_data = utf8_buf.push(screen_bytes);
    let esc_data = esc_buf.push(&utf8_data);
    let _ = cp.process_chunk(&esc_data, &silence, sid, &state);

    // Drain events from the first chunk and count ChoicePrompt emits.
    let mut rx = state.event_bus.subscribe();

    // Second chunk: add an innocuous repaint (cursor home + re-emit same dialog).
    // Same (title, option keys) signature → must be deduped.
    let utf8_data2 = utf8_buf.push(screen_bytes);
    let esc_data2 = esc_buf.push(&utf8_data2);
    let _ = cp.process_chunk(&esc_data2, &silence, sid, &state);

    let mut choice_count = 0;
    while let Ok(evt) = rx.try_recv() {
        if let crate::state::AppEvent::PtyParsed { parsed, .. } = evt
            && parsed.get("type").and_then(|t| t.as_str()) == Some("choice-prompt")
        {
            choice_count += 1;
        }
    }
    assert_eq!(
        choice_count, 0,
        "second chunk with identical ChoicePrompt (same title + option keys) must be deduped",
    );
    assert!(
        cp.last_choice_prompt_sig.is_some(),
        "signature must be stored after first emission"
    );
}

/// Every Ink menu footer is byte-identical, so a session-lifetime question
/// dedup made the awaiting badge a one-shot: the first menu of a session
/// silently swallowed every later one. The marker must retire as soon as the
/// prompt leaves the screen.
#[test]
fn test_chunk_processor_question_dedup_retires_when_prompt_leaves_screen() {
    use crate::state::VtLogBuffer;
    use std::sync::atomic::AtomicU64;

    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "test-cp-question-dedup";
    let silence = Arc::new(Mutex::new(SilenceState::new()));
    state
        .session_maps
        .silence_states
        .insert(sid.to_string(), silence.clone());
    state.session_maps.shell_states.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_NULL),
    );
    state
        .grid
        .vt_log_buffers
        .insert(sid.to_string(), Mutex::new(VtLogBuffer::new(24, 80, 1000)));
    state
        .session_maps
        .output_buffers
        .insert(sid.to_string(), Mutex::new(OutputRingBuffer::new(4096)));
    state
        .session_maps
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(0));

    let mut cp = ChunkProcessor::new(None, None);
    let mut utf8_buf = Utf8ReadBuffer::new();
    let mut esc_buf = EscapeAwareBuffer::new();
    let mut feed = |cp: &mut ChunkProcessor, bytes: &[u8]| {
        let utf8_data = utf8_buf.push(bytes);
        let esc_data = esc_buf.push(&utf8_data);
        let _ = cp.process_chunk(&esc_data, &silence, sid, state.as_ref());
    };
    let count_questions = |rx: &mut tokio::sync::broadcast::Receiver<crate::state::AppEvent>| {
        let mut n = 0;
        while let Ok(evt) = rx.try_recv() {
            if let crate::state::AppEvent::PtyParsed { parsed, .. } = evt
                && parsed.get("type").and_then(|t| t.as_str()) == Some("question")
            {
                n += 1;
            }
        }
        n
    };

    // Ink menu footer — identical bytes for every Claude Code menu.
    const FOOTER: &[u8] =
        "\x1b[2J\x1b[HEnter to select · ↑/↓ to navigate · Esc to cancel\r\n".as_bytes();

    let mut rx = state.event_bus.subscribe();
    feed(&mut cp, FOOTER);
    assert_eq!(count_questions(&mut rx), 1, "first menu must be detected");

    // Repaint while the prompt is still on screen: must stay deduped.
    feed(&mut cp, "\x1b[H".as_bytes());
    feed(&mut cp, FOOTER);
    assert_eq!(
        count_questions(&mut rx),
        0,
        "a repaint of the same on-screen prompt must not re-notify"
    );

    // The user answers: the prompt leaves the screen and the agent works.
    feed(&mut cp, "\x1b[2J\x1b[Hrunning the fix\r\n".as_bytes());
    assert!(
        cp.last_question_text.is_none(),
        "dedup marker must retire once the prompt is off screen"
    );

    // A second menu, byte-identical footer: must be detected again.
    feed(&mut cp, FOOTER);
    assert_eq!(
        count_questions(&mut rx),
        1,
        "a later menu with the same footer must be detected again"
    );
}

/// Same one-shot trap as the question dedup: a dialog the user answers must be
/// detectable again the next time the agent raises it.
#[test]
fn test_chunk_processor_choice_prompt_dedup_retires_when_dialog_leaves_screen() {
    use crate::state::VtLogBuffer;
    use std::sync::atomic::AtomicU64;

    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "test-cp-choice-retire";
    let silence = Arc::new(Mutex::new(SilenceState::new()));
    state
        .session_maps
        .silence_states
        .insert(sid.to_string(), silence.clone());
    state.session_maps.shell_states.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_NULL),
    );
    state
        .grid
        .vt_log_buffers
        .insert(sid.to_string(), Mutex::new(VtLogBuffer::new(24, 80, 1000)));
    state
        .session_maps
        .output_buffers
        .insert(sid.to_string(), Mutex::new(OutputRingBuffer::new(4096)));
    state
        .session_maps
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(0));

    let mut cp = ChunkProcessor::new(None, None);
    let mut utf8_buf = Utf8ReadBuffer::new();
    let mut esc_buf = EscapeAwareBuffer::new();
    let mut feed = |cp: &mut ChunkProcessor, bytes: &[u8]| {
        let utf8_data = utf8_buf.push(bytes);
        let esc_data = esc_buf.push(&utf8_data);
        let _ = cp.process_chunk(&esc_data, &silence, sid, state.as_ref());
    };
    let count_choices = |rx: &mut tokio::sync::broadcast::Receiver<crate::state::AppEvent>| {
        let mut n = 0;
        while let Ok(evt) = rx.try_recv() {
            if let crate::state::AppEvent::PtyParsed { parsed, .. } = evt
                && parsed.get("type").and_then(|t| t.as_str()) == Some("choice-prompt")
            {
                n += 1;
            }
        }
        n
    };

    const DIALOG: &[u8] = "\x1b[2J\x1b[HDo you want to make this edit to CLAUDE.md?\r\n\
              ❯ 1. Yes\r\n\
                2. Yes, allow all edits (shift+tab)\r\n\
                3. No\r\n\
              \r\n\
              Esc to cancel · Tab to amend\r\n"
        .as_bytes();

    let mut rx = state.event_bus.subscribe();
    feed(&mut cp, DIALOG);
    assert_eq!(count_choices(&mut rx), 1, "first dialog must be detected");

    // Answered: the dialog leaves the screen while the agent applies the edit.
    feed(&mut cp, "\x1b[2J\x1b[Happlying the edit\r\n".as_bytes());
    assert!(
        cp.last_choice_prompt_sig.is_none(),
        "signature must retire once the dialog is off screen"
    );

    // The agent raises the identical dialog again.
    feed(&mut cp, DIALOG);
    assert_eq!(
        count_choices(&mut rx),
        1,
        "the same dialog raised again must be detected again"
    );
}

#[test]
fn test_chunk_processor_planfile_resolution() {
    let cp = ChunkProcessor::new(Some("/home/user/repo".to_string()), None);
    // Test that resolve_planfile_path resolves relative paths
    let resolved = cp.resolve_planfile_path("plans/foo.md");
    // Joined and normalised with the host separator, so compare in one spelling.
    assert_eq!(
        resolved.as_deref().map(crate::test_support::slashed),
        Some("/home/user/repo/plans/foo.md".to_string())
    );
}

#[test]
fn test_chunk_processor_planfile_resolution_absolute_passthrough() {
    let cp = ChunkProcessor::new(Some("/home/user/repo".to_string()), None);
    let resolved = cp.resolve_planfile_path("/absolute/path/plan.md");
    assert_eq!(resolved, Some("/absolute/path/plan.md".to_string()));
}

#[test]
fn test_chunk_processor_planfile_resolution_no_cwd() {
    let cp = ChunkProcessor::new(None, None);
    // Relative path with no CWD should return None
    let resolved = cp.resolve_planfile_path("plans/foo.md");
    assert_eq!(resolved, None);
}

#[test]
fn test_chunk_processor_planfile_normalizes_dotdot() {
    let cp = ChunkProcessor::new(Some("/home/user/repo__wt/feat".to_string()), None);
    let resolved = cp.resolve_planfile_path("../../repo/plans/foo.md");
    assert_eq!(
        resolved.as_deref().map(crate::test_support::slashed),
        Some("/home/user/repo/plans/foo.md".to_string())
    );
}

// --- transform_xterm tests ---

#[test]
fn test_transform_xterm_no_token_passes_through() {
    let mut cp = ChunkProcessor::new(None, None);
    let result = cp.transform_xterm("just regular output");
    assert_eq!(result.as_deref(), Some("just regular output"));
}

#[test]
fn test_transform_xterm_intent_passes_through() {
    // Intent coloring is now handled by the frontend MutationObserver.
    let mut cp = ChunkProcessor::new(None, None);
    let result = cp.transform_xterm("intent: Fix the bug\n");
    assert!(result.is_some());
    let data = result.unwrap();
    assert!(
        data.contains("intent: Fix the bug"),
        "intent must pass through to frontend"
    );
}

#[test]
fn test_transform_xterm_suggest_passes_through() {
    // Suggest lines are no longer concealed in Rust — the frontend handles it.
    let mut cp = ChunkProcessor::new(None, None);
    let result = cp.transform_xterm("suggest: A | B | C\n");
    assert!(result.is_some());
    let data = result.unwrap();
    assert!(
        data.contains("suggest:"),
        "suggest must pass through to frontend"
    );
}

#[test]
fn test_transform_xterm_incomplete_intent_passes_through() {
    let mut cp = ChunkProcessor::new(None, None);
    let r1 = cp.transform_xterm("intent: doing so");
    assert!(r1.is_some(), "incomplete intent must pass through");
}

// --- alt buffer clear injection tests ---

#[test]
fn test_transform_xterm_alt_buffer_injects_clear() {
    let mut cp = ChunkProcessor::new(None, None);
    // Enter alt buffer
    cp.transform_xterm("\x1b[?1049h");
    assert!(cp.in_alt_buffer);
    // Cursor home should get ESC[2J injected
    let result = cp.transform_xterm("\x1b[Hcontent").unwrap();
    assert!(
        result.contains("\x1b[2J\x1b[H"),
        "clear should be injected before cursor home"
    );
}

#[test]
fn test_inline_tui_mouse_mode_sets_fullscreen_without_1049() {
    let mut cp = ChunkProcessor::new(None, None);
    cp.apply_inline_tui_mode(false, true, Some("grok"));
    assert!(cp.terminal_mode.is_fullscreen());
    match &cp.terminal_mode {
        crate::ai_agent::tui_detect::TerminalMode::FullscreenTui { app_hint, depth } => {
            assert_eq!(app_hint.as_deref(), Some("grok"));
            assert_eq!(*depth, 1);
        }
        other => panic!("expected FullscreenTui, got {other:?}"),
    }
    cp.apply_inline_tui_mode(false, false, Some("grok"));
    assert!(!cp.terminal_mode.is_fullscreen());
}

#[test]
fn test_inline_tui_does_not_override_alt_screen_mode() {
    let mut cp = ChunkProcessor::new(None, None);
    cp.transform_xterm("\x1b[?1049h");
    cp.apply_inline_tui_mode(true, true, Some("grok"));
    match &cp.terminal_mode {
        crate::ai_agent::tui_detect::TerminalMode::FullscreenTui { depth, .. } => {
            assert_eq!(*depth, 1, "must not nest on top of 1049");
        }
        other => panic!("expected FullscreenTui, got {other:?}"),
    }
}

#[test]
fn test_transform_xterm_normal_buffer_no_inject() {
    let mut cp = ChunkProcessor::new(None, None);
    // NOT in alt buffer — no injection
    let result = cp.transform_xterm("\x1b[Hcontent").unwrap();
    assert!(
        !result.contains("\x1b[2J"),
        "should not inject clear in normal buffer"
    );
}

#[test]
fn test_transform_xterm_alt_buffer_exit_stops_inject() {
    let mut cp = ChunkProcessor::new(None, None);
    // Enter then exit alt buffer
    cp.transform_xterm("\x1b[?1049h");
    cp.transform_xterm("\x1b[?1049l");
    assert!(!cp.in_alt_buffer);
    let result = cp.transform_xterm("\x1b[Hcontent").unwrap();
    assert!(
        !result.contains("\x1b[2J"),
        "should not inject after leaving alt buffer"
    );
}

#[test]
fn test_transform_xterm_alt_buffer_no_clear_on_subsequent_redraws() {
    let mut cp = ChunkProcessor::new(None, None);
    // Enter alt buffer — first cursor-home gets clear
    cp.transform_xterm("\x1b[?1049h");
    let r1 = cp.transform_xterm("\x1b[Hfirst redraw").unwrap();
    assert!(r1.contains("\x1b[2J"), "first redraw must inject clear");

    // Subsequent redraws must NOT inject clear (prevents per-keystroke flicker)
    let r2 = cp.transform_xterm("\x1b[Hsecond redraw").unwrap();
    assert!(
        !r2.contains("\x1b[2J"),
        "subsequent redraws must not inject clear"
    );
}

#[test]
fn test_transform_xterm_alt_buffer_clear_on_shrink() {
    let mut cp = ChunkProcessor::new(None, None);
    // Enter alt buffer, consume initial clear
    cp.transform_xterm("\x1b[?1049h");
    cp.transform_xterm("\x1b[Hinit"); // consumes one-shot

    // Simulate growing content: cursor-up 50 lines
    cp.transform_xterm("\x1b[50A redraw tall");
    assert_eq!(cp.last_cursor_up_n, 50);

    // Simulate shrink: cursor-up only 20 lines (content got shorter)
    let r = cp.transform_xterm("\x1b[20A\x1b[Hredraw short").unwrap();
    assert!(
        r.contains("\x1b[2J"),
        "clear must be injected when content shrinks"
    );
    assert_eq!(cp.last_cursor_up_n, 20);

    // Next redraw at same height — no clear
    let r2 = cp.transform_xterm("\x1b[20A\x1b[Hsame height").unwrap();
    assert!(!r2.contains("\x1b[2J"), "no clear when height stays same");
}

#[test]
fn test_transform_xterm_alt_buffer_clear_on_growth() {
    let mut cp = ChunkProcessor::new(None, None);
    // Enter alt buffer, consume initial clear via cursor-home
    cp.transform_xterm("\x1b[?1049h");
    cp.transform_xterm("\x1b[Hinit");

    // Establish baseline height
    cp.transform_xterm("\x1b[20Aredraw");
    assert_eq!(cp.last_cursor_up_n, 20);

    // Height grows — clear must fire (chrome shifted down, old top row is ghost)
    let r = cp.transform_xterm("\x1b[25A\x1b[Hredraw taller").unwrap();
    assert!(
        r.contains("\x1b[2J"),
        "clear must be injected when content grows"
    );
}

#[test]
fn test_transform_xterm_cursor_up_fallback_on_entry() {
    let mut cp = ChunkProcessor::new(None, None);
    // Enter alt buffer (sets alt_buffer_needs_clear)
    cp.transform_xterm("\x1b[?1049h");

    // Ink re-renders with cursor-up only, no cursor-home.
    // The fallback must inject ESC[2J before the cursor-up.
    let r = cp.transform_xterm("\x1b[30Acontent").unwrap();
    assert!(
        r.contains("\x1b[2J\x1b[30A"),
        "clear must inject before cursor-up fallback"
    );
    assert!(!cp.alt_buffer_needs_clear, "flag must be consumed");
}

#[test]
fn test_transform_xterm_cursor_up_fallback_on_shrink() {
    let mut cp = ChunkProcessor::new(None, None);
    cp.transform_xterm("\x1b[?1049h");
    cp.transform_xterm("\x1b[Hinit"); // consume entry flag

    // Establish height
    cp.transform_xterm("\x1b[40Aredraw");

    // Shrink with cursor-up only (no cursor-home) — fallback path
    let r = cp.transform_xterm("\x1b[25Aredraw short").unwrap();
    assert!(
        r.contains("\x1b[2J\x1b[25A"),
        "cursor-up fallback must fire on shrink"
    );
}

#[test]
fn test_transform_xterm_no_clear_on_normal_buffer_cursor_up() {
    let mut cp = ChunkProcessor::new(None, None);
    // NOT in alt buffer — cursor-up must NOT trigger clear injection
    let r = cp.transform_xterm("\x1b[10Acontent").unwrap();
    assert!(!r.contains("\x1b[2J"), "must not inject in normal buffer");
}

#[test]
fn test_extract_largest_cursor_up() {
    assert_eq!(extract_largest_cursor_up("\x1b[5A"), Some(5));
    assert_eq!(extract_largest_cursor_up("\x1b[10Afoo\x1b[3A"), Some(10));
    assert_eq!(extract_largest_cursor_up("no cursor up here"), None);
    assert_eq!(extract_largest_cursor_up("\x1b[H"), None); // cursor home, not up
}

// --- inject_clear_before_cursor_up tests ---

#[test]
fn test_inject_clear_before_cursor_up_basic() {
    let result = inject_clear_before_cursor_up("\x1b[20Acontent");
    assert_eq!(result, "\x1b[2J\x1b[20Acontent");
}

#[test]
fn test_inject_clear_before_cursor_up_preserves_prefix() {
    let result = inject_clear_before_cursor_up("prefix\x1b[10Acontent");
    assert_eq!(result, "prefix\x1b[2J\x1b[10Acontent");
}

#[test]
fn test_inject_clear_before_cursor_up_no_match() {
    let input = "no cursor up \x1b[H here";
    let result = inject_clear_before_cursor_up(input);
    assert_eq!(
        result, input,
        "cursor-home must NOT match cursor-up injection"
    );
}

#[test]
fn test_inject_clear_before_cursor_up_bare_esc_a_ignored() {
    // ESC[A (no number) means cursor-up 1, but has no digit before A
    let input = "\x1b[Acontent";
    let result = inject_clear_before_cursor_up(input);
    assert_eq!(
        result, input,
        "bare ESC[A (no n) should not trigger injection"
    );
}

// --- log_anomalous_sequences tests ---

#[test]
fn log_anomalous_detects_clear_screen() {
    let found = detect_anomalous_sequences("\x1b[2J");
    assert_eq!(found, vec!["ESC[2J (Clear Screen)"]);
}

#[test]
fn log_anomalous_detects_cursor_home() {
    let found = detect_anomalous_sequences("\x1b[H");
    assert_eq!(found, vec!["ESC[H (Cursor Home)"]);
}

#[test]
fn log_anomalous_detects_cursor_home_explicit() {
    let found = detect_anomalous_sequences("\x1b[1;1H");
    assert_eq!(found, vec!["ESC[1;1H (Cursor Home)"]);
}

#[test]
fn log_anomalous_detects_clear_scrollback() {
    let found = detect_anomalous_sequences("\x1b[3J");
    assert_eq!(found, vec!["ESC[3J (Clear Scrollback)"]);
}

#[test]
fn log_anomalous_detects_alt_screen_enter() {
    let found = detect_anomalous_sequences("\x1b[?1049h");
    assert_eq!(found, vec!["ESC[?1049h (Alt Screen Enter)"]);
}

#[test]
fn log_anomalous_detects_alt_screen_exit() {
    let found = detect_anomalous_sequences("\x1b[?1049l");
    assert_eq!(found, vec!["ESC[?1049l (Alt Screen Exit)"]);
}

#[test]
fn log_anomalous_multiple_in_one_chunk() {
    let found = detect_anomalous_sequences("hello\x1b[2J\x1b[Hworld\x1b[3J");
    assert_eq!(
        found,
        vec![
            "ESC[2J (Clear Screen)",
            "ESC[H (Cursor Home)",
            "ESC[3J (Clear Scrollback)",
        ]
    );
}

#[test]
fn log_anomalous_ignores_normal_sequences() {
    let found = detect_anomalous_sequences("\x1b[5A\x1b[10B\x1b[32mhello\x1b[0m");
    assert!(found.is_empty());
}

#[test]
fn log_anomalous_ignores_cursor_position_not_home() {
    // ESC[5;10H is a regular cursor position, not anomalous
    let found = detect_anomalous_sequences("\x1b[5;10H");
    assert!(found.is_empty());
}

// --- inject_clear_before_cursor_home tests ---

#[test]
fn inject_clear_no_cursor_home() {
    let data = "hello world\x1b[5A\x1b[32mgreen\x1b[0m";
    assert_eq!(inject_clear_before_cursor_home(data), data);
}

#[test]
fn inject_clear_before_bare_home() {
    let data = "\x1b[Hcontent after home";
    assert_eq!(
        inject_clear_before_cursor_home(data),
        "\x1b[2J\x1b[Hcontent after home"
    );
}

#[test]
fn inject_clear_before_explicit_home() {
    let data = "\x1b[1;1Hcontent";
    assert_eq!(
        inject_clear_before_cursor_home(data),
        "\x1b[2J\x1b[1;1Hcontent"
    );
}

#[test]
fn inject_clear_preserves_prefix() {
    let data = "prefix output\x1b[Hredraw content";
    assert_eq!(
        inject_clear_before_cursor_home(data),
        "prefix output\x1b[2J\x1b[Hredraw content"
    );
}

#[test]
fn inject_clear_only_first_home() {
    // Only one ESC[2J should be injected, before the first ESC[H
    let data = "\x1b[Hline1\x1b[Hline2";
    let result = inject_clear_before_cursor_home(data);
    assert_eq!(result, "\x1b[2J\x1b[Hline1\x1b[Hline2");
    // Count occurrences of ESC[2J
    assert_eq!(result.matches("\x1b[2J").count(), 1);
}

#[test]
fn inject_clear_ignores_non_home_cursor_position() {
    // ESC[5;10H is a regular cursor position, not home — should NOT inject
    let data = "\x1b[5;10Hcontent";
    assert_eq!(inject_clear_before_cursor_home(data), data);
}

#[test]
fn inject_clear_preserves_utf8() {
    let data = "héllo → \x1b[Hworld 🌍";
    assert_eq!(
        inject_clear_before_cursor_home(data),
        "héllo → \x1b[2J\x1b[Hworld 🌍"
    );
}

// --- is_wsl_shell tests ---

#[test]
fn is_wsl_shell_bare() {
    assert!(super::is_wsl_shell("wsl.exe"));
    assert!(super::is_wsl_shell("WSL.EXE"));
    assert!(super::is_wsl_shell("wsl"));
}

#[test]
fn is_wsl_shell_with_args() {
    assert!(super::is_wsl_shell("wsl.exe -d Ubuntu"));
    assert!(super::is_wsl_shell(
        "wsl.exe --distribution Debian -- /bin/zsh"
    ));
}

#[test]
fn is_wsl_shell_full_path() {
    assert!(super::is_wsl_shell("C:\\Windows\\System32\\wsl.exe"));
    assert!(super::is_wsl_shell(
        "C:\\Windows\\System32\\wsl.exe -d Ubuntu"
    ));
}

#[test]
fn is_wsl_shell_non_wsl() {
    assert!(!super::is_wsl_shell("powershell.exe"));
    assert!(!super::is_wsl_shell("/bin/zsh"));
    assert!(!super::is_wsl_shell("cmd.exe"));
    assert!(!super::is_wsl_shell("wslconfig.exe"));
}

// --- PTY spawn retry parity (#493-fce6) ---

/// A binary that cannot exist, so `spawn_command` fails on every attempt
/// without depending on the host's PATH.
fn unspawnable_command() -> CommandBuilder {
    CommandBuilder::new("/nonexistent/tuic-spawn-retry-probe")
}

/// A command that spawns and exits at once, whatever the host is. `/bin/echo`
/// is not a path Windows can start.
fn trivial_command() -> CommandBuilder {
    let (shell, flag) = crate::test_support::host_shell();
    let mut command = CommandBuilder::new(shell);
    command.arg(flag);
    command.arg("echo tuic-spawn-probe");
    command
}

#[test]
fn permanent_allocation_failure_is_not_retried() {
    let mut attempts = 0;
    let result = retry_transient(
        || {
            attempts += 1;
            Err::<(), _>("permission denied")
        },
        |_| false,
        |_| panic!("permanent failure must not sleep"),
    );

    assert_eq!(result, Err((1, "permission denied")));
    assert_eq!(attempts, 1);
}

#[test]
fn permanent_command_spawn_failure_builds_once() {
    let attempts = std::cell::Cell::new(0);
    let Err(error) = spawn_pty_pair_with_retry(probe_size(), || {
        attempts.set(attempts.get() + 1);
        unspawnable_command()
    }) else {
        panic!("a nonexistent binary must not spawn");
    };

    assert_eq!(attempts.get(), 1);
    assert!(error.contains("Failed to spawn shell"), "{error}");
}

// --- build_shell_command arg splitting tests ---

#[test]
fn build_shell_command_splits_args() {
    let cmd = super::build_shell_command("wsl.exe -d Ubuntu");
    let argv = cmd.as_unix_command_line().unwrap();
    // The command line should contain the args as separate tokens
    assert!(argv.contains("-d"), "Expected -d in: {}", argv);
    assert!(argv.contains("Ubuntu"), "Expected Ubuntu in: {}", argv);
}

#[test]
fn build_shell_command_single_exe() {
    // Single executable should still work (no extra empty args)
    let cmd = super::build_shell_command("/bin/zsh");
    let argv = cmd.as_unix_command_line().unwrap();
    assert!(argv.contains("/bin/zsh"), "Expected /bin/zsh in: {}", argv);
}

// --- windows_to_wsl_path tests ---

#[test]
fn wsl_path_drive_letter_backslash() {
    assert_eq!(
        super::windows_to_wsl_path("C:\\Users\\foo\\repos"),
        "/mnt/c/Users/foo/repos"
    );
}

#[test]
fn wsl_path_drive_letter_forward_slash() {
    assert_eq!(
        super::windows_to_wsl_path("C:/Users/foo/repos"),
        "/mnt/c/Users/foo/repos"
    );
}

#[test]
fn wsl_path_lowercase_drive() {
    assert_eq!(super::windows_to_wsl_path("d:\\work"), "/mnt/d/work");
}

#[test]
fn wsl_path_already_linux() {
    assert_eq!(
        super::windows_to_wsl_path("/home/user/repos"),
        "/home/user/repos"
    );
}

#[test]
fn wsl_path_unc_unchanged() {
    // UNC paths are not drive-letter paths — returned as-is
    assert_eq!(
        super::windows_to_wsl_path("\\\\server\\share"),
        "\\\\server\\share"
    );
}

#[test]
fn wsl_path_root_drive() {
    assert_eq!(super::windows_to_wsl_path("C:\\"), "/mnt/c/");
}

const SUMMARY_CHILD: &str = "8c261794-91e5-44a4-bf63-ec8afafd2adc";
const SUMMARY_CHILD_B: &str = "9d3728a5-91e5-44a4-bf63-ec8afafd2adc";

#[test]
fn prompt_delivery_failure_reads_the_same_in_both_paths() {
    let payload = serde_json::json!({
        "type": "prompt_delivery_failed",
        "reason": "timeout",
        "session_id": SUMMARY_CHILD,
    });
    let line = describe_lifecycle_payload(SUMMARY_CHILD, &payload);
    assert!(line.starts_with("child agent 8c261794 "), "{line}");
    assert!(
        line.contains("queued"),
        "the notice must not read as a final failure when the prompt is still pending: {line}"
    );
    assert!(!line.contains('\n'), "the line is typed into a composer");
}

#[test]
fn cursor_prefix_completion_preserves_background_epoch_release() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "child-background-cursor-completed";
    // The turn is running when the suggest arrives. Seeding SHELL_IDLE and
    // relying on the output to flip it busy stopped working with ranked
    // evidence (#744-138c): `agent_session` confirms idle at Protocol rank,
    // and Screen-rank output must not reopen a protocol-idle turn.
    agent_session(&state, child_id, SHELL_BUSY);
    state
        .session_maps
        .session_states
        .get_mut(child_id)
        .unwrap()
        .background_work = true;
    state.grid.vt_log_buffers.insert(
        child_id.to_string(),
        Mutex::new(crate::state::VtLogBuffer::new(24, 80, 1000)),
    );
    let silence = state
        .session_maps
        .silence_states
        .get(child_id)
        .unwrap()
        .clone();
    let mut processor = ChunkProcessor::new(None, None);

    processor.process_chunk("........................| C ]", &silence, child_id, &state);
    processor.process_chunk("\rsuggest: [ A | B", &silence, child_id, &state);
    assert!(!silence.lock().completion_declared_for_epoch(0));
    processor.process_chunk("\r\x1b[", &silence, child_id, &state);
    assert!(!silence.lock().completion_declared_for_epoch(0));
    processor.process_chunk("Ksuggest: [ A | B | C ]", &silence, child_id, &state);

    {
        let guard = silence.lock();
        assert!(guard.completion_declared_for_epoch(0));
        assert_eq!(
            guard.pending_suggest_items.as_deref(),
            Some(&["A".to_string(), "B".to_string(), "C".to_string()][..])
        );
        assert_eq!(guard.pending_suggest_turn_epoch, 0);
    }
    assert!(try_shell_transition(
        &state, child_id, SHELL_BUSY, SHELL_IDLE, false
    ));
    let deferred = state.session_state_with_shell(child_id).unwrap();
    assert_eq!(deferred.agent_state.as_deref(), Some("working"));
    assert!(deferred.background_work);
    assert!(set_background_work_for_epoch(&state, child_id, 0, 1, false));
    let snapshot = state.session_state_with_shell(child_id).unwrap();
    assert_eq!(snapshot.agent_state.as_deref(), Some("completed"));
    assert!(!snapshot.background_work);
}

#[test]
fn physical_cursor_suggest_completes_after_wrapped_background_probe_clears() {
    for wrap_count in 1..=5 {
        let state = crate::state::tests_support::make_test_app_state();
        let child_id = format!("wrapped-background-physical-suggest-{wrap_count}");
        agent_session(&state, &child_id, SHELL_IDLE);
        state.grid.vt_log_buffers.insert(
            child_id.clone(),
            Mutex::new(crate::state::VtLogBuffer::new(10, 80, 1000)),
        );
        let silence = state
            .session_maps
            .silence_states
            .get(&child_id)
            .unwrap()
            .clone();
        let mut processor = ChunkProcessor::new(None, None);

        note_submitted_input(&state, &child_id);
        processor.process_chunk(
            &"x".repeat(wrap_count * 80 + 1),
            &silence,
            &child_id,
            &state,
        );
        {
            let mut session = state
                .session_maps
                .session_states
                .get_mut(&child_id)
                .unwrap();
            session.background_probe_turn_epoch = Some(1);
            session.background_probe_after_generation = Some(0);
        }
        assert!(set_background_work_for_epoch(&state, &child_id, 1, 1, true));
        assert!(
            state
                .session_maps
                .session_states
                .get(&child_id)
                .unwrap()
                .background_work
        );

        assert!(try_shell_transition(
            &state, &child_id, SHELL_BUSY, SHELL_IDLE, false,
        ));
        assert!(set_background_work_for_epoch(
            &state, &child_id, 1, 2, false
        ));
        assert_eq!(
            state
                .session_state_with_shell(&child_id)
                .unwrap()
                .agent_state
                .as_deref(),
            Some("idle"),
            "wrap_count={wrap_count}"
        );

        processor.process_chunk(
            "\r\x1b[2Ksuggest: [ background cleared | lifecycle complete | close smoke ]",
            &silence,
            &child_id,
            &state,
        );

        let snapshot = state.session_state_with_shell(&child_id).unwrap();
        assert_eq!(
            snapshot.agent_state.as_deref(),
            Some("completed"),
            "wrap_count={wrap_count}"
        );
        assert!(!snapshot.background_work, "wrap_count={wrap_count}");
        assert!(
            silence.lock().completion_declared_for_epoch(1),
            "wrap_count={wrap_count}"
        );
    }
}

#[test]
fn identical_suggest_reopens_only_after_fresh_work_in_a_new_turn() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "suggest-multiple-turns";
    agent_session(&state, child_id, SHELL_IDLE);
    state.grid.vt_log_buffers.insert(
        child_id.to_string(),
        Mutex::new(crate::state::VtLogBuffer::new(24, 80, 1000)),
    );
    let silence = state
        .session_maps
        .silence_states
        .get(child_id)
        .unwrap()
        .clone();
    let mut processor = ChunkProcessor::new(None, None);
    let marker = "suggest: [ lifecycle fixed | close smoke | continue parity ]";

    processor.process_chunk("first response\r\n", &silence, child_id, &state);
    processor.process_chunk(marker, &silence, child_id, &state);
    assert_eq!(
        silence.lock().drain_pending_suggest(),
        Some(vec![
            "lifecycle fixed".to_string(),
            "close smoke".to_string(),
            "continue parity".to_string(),
        ])
    );

    note_submitted_input(&state, child_id);
    assert_eq!(
        state
            .session_maps
            .session_states
            .get(child_id)
            .unwrap()
            .turn_epoch,
        1
    );

    // A previous-turn row can repaint as the input scrolls. Submission by
    // itself must not reopen the content deduplication boundary.
    processor.process_chunk(&format!("\r\n{marker}"), &silence, child_id, &state);
    assert_eq!(silence.lock().drain_pending_suggest(), None);
    processor.process_chunk(&format!("\r\n{marker}"), &silence, child_id, &state);
    assert_eq!(silence.lock().drain_pending_suggest(), None);

    // Real output proves the next response started. The identical marker
    // is now a valid completion, but a second repaint in the same turn is
    // still suppressed.
    processor.process_chunk("\r\nsecond response\r\n", &silence, child_id, &state);
    processor.process_chunk(marker, &silence, child_id, &state);
    assert_eq!(
        silence.lock().drain_pending_suggest(),
        Some(vec![
            "lifecycle fixed".to_string(),
            "close smoke".to_string(),
            "continue parity".to_string(),
        ])
    );
    processor.process_chunk(&format!("\r\n{marker}"), &silence, child_id, &state);
    assert_eq!(silence.lock().drain_pending_suggest(), None);
    assert!(silence.lock().completion_declared_for_epoch(1));
}

#[test]
fn cursor_prefix_rejects_stale_suffix_then_emits_real_completion_once() {
    for (index, bullet) in ["●", "⏺", "•", "◦"].into_iter().enumerate() {
        let state = crate::state::tests_support::make_test_app_state();
        let child_id = format!("cursor-stale-suffix-{index}");
        agent_session(&state, &child_id, SHELL_IDLE);
        state.grid.vt_log_buffers.insert(
            child_id.clone(),
            Mutex::new(crate::state::VtLogBuffer::new(24, 80, 1000)),
        );
        let silence = state
            .session_maps
            .silence_states
            .get(&child_id)
            .unwrap()
            .clone();
        let mut processor = ChunkProcessor::new(None, None);

        processor.process_chunk(
            "............................| C ]",
            &silence,
            &child_id,
            &state,
        );
        processor.process_chunk(
            &format!("\r{bullet} suggest: [ A | B"),
            &silence,
            &child_id,
            &state,
        );
        assert_eq!(silence.lock().drain_pending_suggest(), None, "{bullet}");

        processor.process_chunk("\r\x1b[", &silence, &child_id, &state);
        assert_eq!(silence.lock().drain_pending_suggest(), None, "{bullet}");
        processor.process_chunk(
            &format!("K{bullet} suggest: [ A | B | C ]"),
            &silence,
            &child_id,
            &state,
        );
        assert_eq!(
            silence.lock().drain_pending_suggest(),
            Some(vec!["A".to_string(), "B".to_string(), "C".to_string()]),
            "{bullet}"
        );

        processor.process_chunk(
            &format!("\r\x1b[K{bullet} suggest: [ A | B | C ]"),
            &silence,
            &child_id,
            &state,
        );
        assert_eq!(silence.lock().drain_pending_suggest(), None, "{bullet}");
    }
}

#[test]
fn wrapped_suggest_reconstructs_unchanged_anchor_across_chunks() {
    for (index, bullet) in ["●", "⏺", "•", "◦"].into_iter().enumerate() {
        let state = crate::state::tests_support::make_test_app_state();
        let child_id = format!("wrapped-suggest-across-chunks-{index}");
        agent_session(&state, &child_id, SHELL_IDLE);
        state.grid.vt_log_buffers.insert(
            child_id.clone(),
            Mutex::new(crate::state::VtLogBuffer::new(24, 14, 1000)),
        );
        let silence = state
            .session_maps
            .silence_states
            .get(&child_id)
            .unwrap()
            .clone();
        let mut processor = ChunkProcessor::new(None, None);

        processor.process_chunk(
            &format!("{bullet} suggest: [ A"),
            &silence,
            &child_id,
            &state,
        );
        assert_eq!(silence.lock().drain_pending_suggest(), None, "{bullet}");
        processor.process_chunk("界 | B | C ]", &silence, &child_id, &state);

        assert_eq!(
            silence.lock().drain_pending_suggest(),
            Some(vec!["A界".to_string(), "B".to_string(), "C".to_string()]),
            "{bullet}"
        );
    }
}

#[test]
fn bounded_cursor_prefix_refusal_suppresses_structured_completion() {
    for (child_id, columns, token) in [
        (
            "cursor-prefix-over-512-bytes",
            700,
            format!("suggest: [ A | B | C ]{}", "x".repeat(520)),
        ),
        (
            "cursor-prefix-over-four-wraps",
            20,
            format!(
                "suggest: [ {} | {} | {} | {} ]",
                "a".repeat(30),
                "b".repeat(30),
                "c".repeat(30),
                "d".repeat(30)
            ),
        ),
    ] {
        let state = crate::state::tests_support::make_test_app_state();
        agent_session(&state, child_id, SHELL_IDLE);
        state.grid.vt_log_buffers.insert(
            child_id.to_string(),
            Mutex::new(crate::state::VtLogBuffer::new(24, columns, 1000)),
        );
        let silence = state
            .session_maps
            .silence_states
            .get(child_id)
            .unwrap()
            .clone();
        let mut processor = ChunkProcessor::new(None, None);

        processor.process_chunk(&token, &silence, child_id, &state);

        assert_eq!(silence.lock().drain_pending_suggest(), None, "{child_id}");
    }
}

#[test]
fn wrapped_suggest_requires_complete_non_nested_prefix() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "wrapped-suggest-incomplete";
    agent_session(&state, child_id, SHELL_IDLE);
    state.grid.vt_log_buffers.insert(
        child_id.to_string(),
        Mutex::new(crate::state::VtLogBuffer::new(24, 10, 1000)),
    );
    let silence = state
        .session_maps
        .silence_states
        .get(child_id)
        .unwrap()
        .clone();
    let mut processor = ChunkProcessor::new(None, None);

    processor.process_chunk("suggest: [", &silence, child_id, &state);
    processor.process_chunk(" A | B", &silence, child_id, &state);
    assert_eq!(silence.lock().drain_pending_suggest(), None);

    processor.process_chunk("\r\x1b[2K\x1b[1A\r\x1b[2K", &silence, child_id, &state);
    processor.process_chunk("suggest: [", &silence, child_id, &state);
    processor.process_chunk(" A | EP[\"node\"] | C ]", &silence, child_id, &state);
    assert_eq!(silence.lock().drain_pending_suggest(), None);
}

#[test]
fn stale_background_clear_cannot_emit_after_new_turn() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "child-background-race";
    let parent_id = "parent-background-race";
    state
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    state.session_maps.session_states.insert(
        child_id.to_string(),
        crate::state::SessionState {
            agent_type: Some("codex".to_string()),
            background_work: true,
            turn_epoch: 7,
            ..Default::default()
        },
    );
    state.session_maps.shell_states.insert(
        child_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_IDLE),
    );
    state.session_maps.silence_states.insert(
        child_id.to_string(),
        Arc::new(Mutex::new(SilenceState::new())),
    );

    note_submitted_input(&state, child_id);
    assert!(!set_background_work_for_epoch(
        &state, child_id, 7, 1, false
    ));
    assert!(
        state
            .session_maps
            .session_states
            .get(child_id)
            .unwrap()
            .background_work
    );
    assert!(state.agent_inbox.get(parent_id).unwrap().is_empty());
}

#[test]
fn background_snapshot_teardown_does_not_recreate_lifecycle_or_notify() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "background-teardown";
    let parent_id = "background-teardown-parent";
    agent_session(&state, child_id, SHELL_IDLE);
    state
        .session_maps
        .session_states
        .get_mut(child_id)
        .unwrap()
        .background_work = true;
    state
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();

    assert!(!set_background_work_for_epoch_with_hook(
        &state,
        child_id,
        0,
        1,
        false,
        || {
            state.session_maps.silence_states.remove(child_id);
            state.session_maps.shell_states.remove(child_id);
            state.session_maps.session_states.remove(child_id);
        },
    ));
    assert!(!state.session_maps.silence_states.contains_key(child_id));
    assert!(state.agent_inbox.get(parent_id).unwrap().is_empty());
}

#[test]
fn failed_or_invalid_cached_snapshot_preserves_background_work() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "background-snapshot-failure";
    state.session_maps.session_states.insert(
        session_id.to_string(),
        crate::state::SessionState {
            agent_type: Some("codex".to_string()),
            background_work: true,
            turn_epoch: 3,
            ..Default::default()
        },
    );

    assert!(!refresh_background_work_from_cached_snapshot(
        &state, session_id, 10, "codex", 3, None,
    ));
    let invalid = Arc::new(vec![process(20, 1, "codex", "codex")]);
    assert!(!refresh_background_work_from_cached_snapshot(
        &state,
        session_id,
        10,
        "codex",
        3,
        Some((1, invalid)),
    ));
    assert!(
        state
            .session_maps
            .session_states
            .get(session_id)
            .unwrap()
            .background_work
    );
}

#[test]
fn try_shell_transition_non_agent_session_does_not_push_idle_notification() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "non-agent-sess";
    let parent_id = "parent-non-agent-sess";

    state
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    // No agent_type set — plain shell session
    state
        .session_maps
        .session_states
        .insert(child_id.to_string(), crate::state::SessionState::default());
    state.session_maps.shell_states.insert(
        child_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_BUSY),
    );

    try_shell_transition(&state, child_id, SHELL_BUSY, SHELL_IDLE, true);

    let inbox = state.agent_inbox.get(parent_id).unwrap();
    assert!(
        inbox.is_empty(),
        "non-agent sessions must not send idle notifications to parent"
    );
}

#[test]
fn try_shell_transition_exit_path_does_not_push_idle_to_parent() {
    // notify_parent=false (exit path): orchestrator must NOT receive spurious "idle"
    // before the "exited" message from mark_session_exited.
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "child-exit-path";
    let parent_id = "parent-exit-path";

    state
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    let ss = crate::state::SessionState {
        agent_type: Some("claude".to_string()),
        ..Default::default()
    };
    state
        .session_maps
        .session_states
        .insert(child_id.to_string(), ss);
    state.session_maps.shell_states.insert(
        child_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_BUSY),
    );

    let transitioned = try_shell_transition(&state, child_id, SHELL_BUSY, SHELL_IDLE, false);
    assert!(transitioned, "transition must succeed");

    let inbox = state.agent_inbox.get(parent_id).unwrap();
    assert!(
        inbox.is_empty(),
        "exit path must not push idle notification — mark_session_exited sends exited"
    );
}

#[test]
fn timer_idle_evidence_from_prior_turn_cannot_mutate_new_submission() {
    use std::sync::atomic::Ordering;

    for (session_id, activity) in [
        ("ready-idle-epoch", AgentScreenActivity::Ready),
        ("interrupted-idle-epoch", AgentScreenActivity::Interrupted),
        ("unknown-idle-epoch", AgentScreenActivity::Unknown),
    ] {
        let state = crate::state::tests_support::make_test_app_state();
        agent_session(&state, session_id, SHELL_BUSY);
        let evidence_turn_epoch = Some(0);
        note_submitted_input(&state, session_id);

        let transition = try_timer_idle_transition(
            &state,
            &state
                .session_maps
                .silence_states
                .get(session_id)
                .unwrap()
                .clone(),
            session_id,
            activity,
            Some("claude"),
            evidence_turn_epoch,
        );

        assert!(!transition.transitioned);
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
}

#[test]
fn should_inject_now_only_for_idle_agent() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "idle-agent", SHELL_IDLE);
    agent_session(&state, "busy-agent", SHELL_BUSY);
    assert!(
        should_inject_now(&state, "idle-agent"),
        "idle agent → inject"
    );
    assert!(
        !should_inject_now(&state, "busy-agent"),
        "busy agent → queue"
    );
    assert!(
        !should_inject_now(&state, "unknown"),
        "unknown session → never inject"
    );
}

#[test]
fn should_inject_now_false_for_shell_and_confident_question() {
    use std::sync::atomic::AtomicU8;
    let state = crate::state::tests_support::make_test_app_state();
    // Plain shell (no agent_type) — must never be injected into.
    state
        .session_maps
        .shell_states
        .insert("shell".to_string(), AtomicU8::new(SHELL_IDLE));
    state
        .session_maps
        .session_states
        .insert("shell".to_string(), crate::state::SessionState::default());
    assert!(!should_inject_now(&state, "shell"), "shell → never inject");

    // Agent idle but blocked on a CONFIDENT user-facing question (Ink menu,
    // cliclack prompt, "Action Required" title) — never answer it.
    state
        .session_maps
        .shell_states
        .insert("q".to_string(), AtomicU8::new(SHELL_IDLE));
    state.session_maps.session_states.insert(
        "q".to_string(),
        crate::state::SessionState {
            spawn_root_role: crate::state::SpawnRootRole::DirectProgram,
            agent_type: Some("claude".to_string()),
            awaiting_input: true,
            question_confident: true,
            ..Default::default()
        },
    );
    let mut ready_silence = SilenceState::new();
    ready_silence.confirm_idle();
    state
        .session_maps
        .silence_states
        .insert("ready".to_string(), Arc::new(Mutex::new(ready_silence)));
    assert!(
        !should_inject_now(&state, "q"),
        "confident question agent → do not answer its prompt"
    );

    // Agent idle at a mere ready prompt: the low-confidence silence heuristic
    // sets awaiting_input WITHOUT question_confident (codex parks here
    // permanently — story 091). Injection must proceed or delivery starves.
    state
        .session_maps
        .shell_states
        .insert("ready".to_string(), AtomicU8::new(SHELL_IDLE));
    state.session_maps.session_states.insert(
        "ready".to_string(),
        crate::state::SessionState {
            spawn_root_role: crate::state::SpawnRootRole::DirectProgram,
            agent_type: Some("codex".to_string()),
            awaiting_input: true,
            question_confident: false,
            ..Default::default()
        },
    );
    assert!(
        should_inject_now(&state, "ready"),
        "awaiting_input-only (ready prompt) agent → inject, do not starve"
    );
}

/// A quiet PTY is not a ready composer while Codex still paints Working.
/// The readiness retry must leave that queued user command untouched.
#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn queued_codex_command_waits_when_idle_shell_still_shows_working() {
    tokio::time::pause();
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "codex-working-on-idle-shell";
    agent_session(&state, sid, SHELL_BUSY);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .agent_type = Some("codex".into());
    let bytes = insert_recording_session(&state, sid);
    assert_eq!(
        enqueue_user_command(&state, sid, "wait for completion", None)
            .unwrap()
            .queued,
        1
    );
    assert!(try_shell_transition(
        &state, sid, SHELL_BUSY, SHELL_IDLE, true
    ));
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
    {
        let mut silence = silence.lock();
        silence.force_idle_unconfirmed();
        silence.cached_screen_activity = AgentScreenActivity::Working;
    }

    let running = Arc::new(AtomicBool::new(true));
    spawn_silence_timer(silence, running.clone(), sid.into(), state.clone());
    tokio::task::yield_now().await;
    tokio::time::advance(SILENCE_CHECK_INTERVAL + std::time::Duration::from_millis(1)).await;
    tokio::task::yield_now().await;
    running.store(false, Ordering::Release);

    assert!(
        bytes.lock().unwrap().is_empty(),
        "Working must not receive queued input"
    );
    assert_eq!(queued_command_count(&state, sid), 1);
}

#[cfg(unix)]
#[test]
fn enqueue_refuses_shells_and_dead_sessions() {
    use std::sync::atomic::AtomicU8;
    let state = crate::state::tests_support::make_test_app_state();
    state
        .session_maps
        .shell_states
        .insert("shell".to_string(), AtomicU8::new(SHELL_IDLE));
    state
        .session_maps
        .session_states
        .insert("shell".to_string(), crate::state::SessionState::default());
    insert_recording_session(&state, "shell");
    assert_eq!(
        enqueue_user_command(&state, "shell", "ls", None).unwrap_err(),
        "Session is not running an agent"
    );

    agent_session(&state, "gone", SHELL_IDLE);
    assert_eq!(
        enqueue_user_command(&state, "gone", "hi", None).unwrap_err(),
        "Session not found",
        "a tombstoned agent still has session_states — the PTY is what decides"
    );

    agent_session(&state, "blank", SHELL_IDLE);
    insert_recording_session(&state, "blank");
    assert_eq!(
        enqueue_user_command(&state, "blank", "   \n ", None).unwrap_err(),
        "Command text is empty"
    );
    assert_eq!(queued_command_count(&state, "blank"), 0);
}

/// A completed PTY write is not evidence that the child accepted Enter. A
/// silent raw-mode composer can retain the entire prompt after that write.
#[cfg(unix)]
#[test]
fn queued_wake_without_agent_response_reports_uncertain_delivery() {
    for agent_type in [
        "codex", "claude", "gemini", "opencode", "aider", "goose", "grok", "pi", "unknown",
    ] {
        let state = crate::state::tests_support::make_test_app_state();
        let sid = format!("silent-{agent_type}");
        agent_session(&state, &sid, SHELL_IDLE);
        state
            .session_maps
            .session_states
            .get_mut(&sid)
            .unwrap()
            .agent_type = Some(agent_type.into());
        let bytes = insert_recording_session(&state, &sid);
        let mut alerts = state.event_bus.subscribe();

        enqueue_user_command(&state, &sid, "wake the agent", None).unwrap();

        assert!(
            state
                .session_maps
                .silence_states
                .get(&sid)
                .unwrap()
                .lock()
                .injection_delivery_uncertain,
            "{agent_type} must not count a silent Enter as a confirmed submission"
        );
        assert!(bytes.lock().unwrap().ends_with(b"\r"));
        let alert = std::iter::from_fn(|| alerts.try_recv().ok())
            .find(|event| matches!(event, crate::state::AppEvent::McpToast { .. }))
            .expect("report uncertain delivery to clients");
        match alert {
            crate::state::AppEvent::McpToast {
                title,
                message,
                level,
                origin_session_id,
                ..
            } => {
                assert_eq!(title, "Agent input was not confirmed");
                assert!(
                    message
                        .as_deref()
                        .is_some_and(|text| text.contains("Do not retype"))
                );
                assert_eq!(level, "error");
                assert_eq!(origin_session_id.as_deref(), Some(sid.as_str()));
            }
            other => panic!("expected delivery failure notification, got {other:?}"),
        }
    }
}

/// These three agents have neither a verified screen adapter nor hook support.
/// Preserve their previous PTY-write result until a live capture can supply a
/// trustworthy acknowledgement signal.
#[cfg(unix)]
#[test]
fn unverified_agents_keep_legacy_queued_write_result() {
    for agent_type in ["amp", "cursor", "droid"] {
        let state = crate::state::tests_support::make_test_app_state();
        let sid = format!("legacy-{agent_type}");
        agent_session(&state, &sid, SHELL_IDLE);
        state
            .session_maps
            .session_states
            .get_mut(&sid)
            .unwrap()
            .agent_type = Some(agent_type.into());
        let bytes = insert_recording_session(&state, &sid);
        let mut alerts = state.event_bus.subscribe();
        enqueue_user_command(&state, &sid, "wake the agent", None).unwrap();
        assert!(bytes.lock().unwrap().ends_with(b"\r"));
        assert!(
            !state
                .session_maps
                .silence_states
                .get(&sid)
                .unwrap()
                .lock()
                .injection_delivery_uncertain
        );
        assert!(
            !std::iter::from_fn(|| alerts.try_recv().ok())
                .any(|event| matches!(event, crate::state::AppEvent::McpToast { .. }))
        );
    }
}

/// A human can resolve an uncertain composer by pressing Enter once. A later
/// confirmed ready prompt must let the next queued command through without
/// replaying the earlier text.
#[cfg(unix)]
#[test]
fn manual_submit_after_uncertain_delivery_reopens_next_queue_slot() {
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "uncertain-then-manual-enter";
    agent_session(&state, sid, SHELL_IDLE);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .agent_type = Some("codex".into());
    let bytes = insert_recording_session(&state, sid);

    enqueue_user_command(&state, sid, "first wake", None).unwrap();
    assert!(
        state
            .session_maps
            .silence_states
            .get(sid)
            .unwrap()
            .lock()
            .injection_delivery_uncertain
    );
    enqueue_user_command(&state, sid, "second wake", None).unwrap();
    assert_eq!(queued_command_count(&state, sid), 1);
    assert!(!String::from_utf8_lossy(&bytes.lock().unwrap()).contains("second wake"));

    // The user submits the retained composer, then the next ready prompt is
    // independently confirmed. Neither step asks TUIC to resend the first wake.
    note_submitted_input(&state, sid);
    assert!(
        !state
            .session_maps
            .silence_states
            .get(sid)
            .unwrap()
            .lock()
            .injection_delivery_uncertain,
        "a human Enter resolves the uncertain composer before another queued write"
    );
    state
        .session_maps
        .silence_states
        .get(sid)
        .unwrap()
        .lock()
        .confirm_idle();
    state
        .session_maps
        .shell_states
        .get(sid)
        .unwrap()
        .store(SHELL_IDLE, std::sync::atomic::Ordering::Release);

    flush_pending_injections_blocking(&state, sid);
    let written = String::from_utf8_lossy(&bytes.lock().unwrap()).into_owned();
    assert_eq!(written.matches("first wake").count(), 1);
    assert_eq!(
        written.matches("second wake").count(),
        1,
        "next queued command must not stay parked"
    );
}

/// The OSC idle marker is parsed on the PTY reader. It must finish publishing
/// that chunk before a queued write waits for the child's next output chunk.
#[cfg(unix)]
#[test]
fn queued_idle_on_reader_defers_delivery_until_chunk_is_published() {
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "reader-queued-idle";
    agent_session(&state, sid, SHELL_BUSY);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .agent_type = Some("codex".into());
    state
        .grid
        .vt_log_buffers
        .insert(sid.into(), Mutex::new(VtLogBuffer::new(24, 80, 1000)));
    state
        .session_maps
        .output_buffers
        .insert(sid.into(), Mutex::new(OutputRingBuffer::new(1024)));
    let bytes = insert_recording_session(&state, sid);
    state
        .pending_injections
        .entry(sid.into())
        .or_default()
        .push_back(crate::state::PendingInjection::notice("wake the agent"));
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
    let mut reader = ChunkProcessor::new(None, None);

    reader.process_chunk("\x1b]7770;state=idle\x07", &silence, sid, &state);

    assert!(
        bytes.lock().unwrap().is_empty(),
        "queued write must wait for reader publication"
    );
    assert_eq!(queued_command_count(&state, sid), 1);
}

/// rb-tool stayed in a stale Working/Busy state with a queued wake in its
/// composer. Its PTY emitted only cursor updates until a manual CR arrived;
/// that CR finally produced the BG DONE turn. Replaying those real cursor
/// chunks must not turn a pre-existing Working screen into a new receipt.
#[cfg(unix)]
#[test]
fn captured_codex_stale_working_screen_does_not_confirm_queued_enter() {
    struct RecordingChannel(std::sync::mpsc::Sender<Vec<u8>>);
    impl std::io::Write for RecordingChannel {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.send(bytes.to_vec()).expect("record PTY write");
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let capture = crate::pty_capture::decode_capture(&agent_prompt_fixture(
        "codex-queued-wake-stuck-20260929.tcap",
    ))
    .expect("authentic rb-tool capture");
    let first_input = capture
        .records
        .iter()
        .position(|record| record.direction == crate::pty_capture::CaptureDirection::Input)
        .expect("manual Enter in capture");
    assert_eq!(first_input, 4);
    assert_eq!(capture.records[first_input].data, b"\r");
    assert!(
        capture.records[first_input].elapsed_us - capture.records[0].elapsed_us
            > 30 * 60 * 1_000_000,
        "the composer persisted through a long quiet interval"
    );
    assert!(capture.records[first_input + 1..].iter().any(|record| {
        record.direction == crate::pty_capture::CaptureDirection::Output
            && record
                .data
                .windows(b"BG DONE".len())
                .any(|window| window == b"BG DONE")
    }));

    let state = crate::state::tests_support::make_test_app_state();
    let sid = "captured-codex-stale-working";
    agent_session(&state, sid, SHELL_IDLE);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .agent_type = Some("codex".into());
    let mut vt = VtLogBuffer::new(48, 110, 1000);
    vt.process(b"\x1b[2J\x1b[45;1H\xe2\x80\xa2 Working (1s \xe2\x80\xa2 esc to interrupt)\x1b[46;1H\xe2\x80\xba Ask Codex to do anything");
    state.grid.vt_log_buffers.insert(sid.into(), Mutex::new(vt));
    assert_eq!(agent_submission_ack_kind(&state, sid), "working_screen");
    state
        .session_maps
        .output_buffers
        .insert(sid.into(), Mutex::new(OutputRingBuffer::new(1024)));
    let (writes, received) = std::sync::mpsc::channel();
    insert_session_with_writer(
        &state,
        sid,
        Box::new(RecordingChannel(writes)),
        TtyMode::Raw,
    );

    std::thread::scope(|scope| {
        scope.spawn(|| enqueue_user_command(&state, sid, "wake the agent", None).unwrap());
        for expected in [b"\x15".as_slice(), b"wake the agent", b"\r"] {
            assert_eq!(
                received
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap(),
                expected
            );
        }
        for record in &capture.records[..first_input] {
            state
                .grid
                .vt_log_buffers
                .get(sid)
                .unwrap()
                .lock()
                .process(&record.data);
            state
                .session_maps
                .output_buffers
                .get(sid)
                .unwrap()
                .lock()
                .write(&record.data);
        }
    });

    assert!(
        state
            .session_maps
            .silence_states
            .get(sid)
            .unwrap()
            .lock()
            .injection_delivery_uncertain,
        "cursor movement beneath a stale Working screen must not acknowledge Enter"
    );
}

/// A Codex stop hook can delay the child's Working repaint well past Enter.
/// The sender must not see an error when that turn was actually accepted.
#[cfg(unix)]
#[test]
fn queued_codex_stop_hook_accepts_working_screen_three_seconds_after_enter() {
    struct ChannelWriter(std::sync::mpsc::Sender<Vec<u8>>);
    impl std::io::Write for ChannelWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.send(bytes.to_vec()).expect("record PTY write");
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let state = crate::state::tests_support::make_test_app_state();
    let sid = "codex-stop-hook-delayed-working";
    agent_session(&state, sid, SHELL_IDLE);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .agent_type = Some("codex".into());
    let mut vt = VtLogBuffer::new(24, 80, 1000);
    vt.process(b"\x1b[22;1H\xe2\x80\xba Ask Codex to do anything");
    state.grid.vt_log_buffers.insert(sid.into(), Mutex::new(vt));
    state
        .session_maps
        .output_buffers
        .insert(sid.into(), Mutex::new(OutputRingBuffer::new(1024)));
    assert_eq!(agent_submission_ack_kind(&state, sid), "ready_screen");
    let (writes, received) = std::sync::mpsc::channel();
    insert_session_with_writer(&state, sid, Box::new(ChannelWriter(writes)), TtyMode::Raw);
    let mut alerts = state.event_bus.subscribe();
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();

    std::thread::scope(|scope| {
        scope.spawn(|| enqueue_user_command(&state, sid, "wake the agent", None).unwrap());
        for expected in [b"\x15".as_slice(), b"wake the agent", b"\r"] {
            assert_eq!(
                received
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap(),
                expected
            );
        }
        std::thread::sleep(std::time::Duration::from_secs(3));
        let mut reader = ChunkProcessor::new(None, None);
        reader.process_chunk(
            "\x1b[21;1H• Working (1s • esc to interrupt)",
            &silence,
            sid,
            &state,
        );
    });

    assert!(
        !state
            .session_maps
            .silence_states
            .get(sid)
            .unwrap()
            .lock()
            .injection_delivery_uncertain,
        "a Codex Working screen three seconds after Enter confirms the queued turn"
    );
    assert!(
        std::iter::from_fn(|| alerts.try_recv().ok())
            .all(|event| !matches!(event, crate::state::AppEvent::McpToast { .. })),
        "the accepted turn must not show a false failure toast"
    );
}

/// Drive one queued Codex delivery whose first Enter draws `screen_after_enter`
/// (Ctrl-U, text and Enter are consumed first) and report every write the PTY
/// saw plus whether the delivery ended uncertain. `after_retry` runs once a
/// second Enter arrives, to draw the agent's reaction.
#[cfg(unix)]
fn run_codex_queued_delivery(
    sid: &str,
    screen_after_enter: &str,
    after_retry: Option<&str>,
) -> (Vec<Vec<u8>>, bool) {
    struct ChannelWriter(std::sync::mpsc::Sender<Vec<u8>>);
    impl std::io::Write for ChannelWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.send(bytes.to_vec()).expect("record PTY write");
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, sid, SHELL_IDLE);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .agent_type = Some("codex".into());
    let mut vt = VtLogBuffer::new(24, 80, 1000);
    vt.process(b"\x1b[22;1H\xe2\x80\xba Ask Codex to do anything");
    state.grid.vt_log_buffers.insert(sid.into(), Mutex::new(vt));
    state
        .session_maps
        .output_buffers
        .insert(sid.into(), Mutex::new(OutputRingBuffer::new(1024)));
    let (writes, received) = std::sync::mpsc::channel();
    insert_session_with_writer(&state, sid, Box::new(ChannelWriter(writes)), TtyMode::Raw);
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
    let mut seen = Vec::new();

    std::thread::scope(|scope| {
        scope.spawn(|| enqueue_user_command(&state, sid, "wake the agent", None).unwrap());
        for expected in [b"\x15".as_slice(), b"wake the agent", b"\r"] {
            let write = received
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap();
            assert_eq!(write, expected);
            seen.push(write);
        }
        let mut reader = ChunkProcessor::new(None, None);
        reader.process_chunk(screen_after_enter, &silence, sid, &state);
        if let Some(reaction) = after_retry
            && let Ok(retry) = received.recv_timeout(std::time::Duration::from_secs(10))
        {
            seen.push(retry);
            reader.process_chunk(reaction, &silence, sid, &state);
        }
    });

    let uncertain = silence.lock().injection_delivery_uncertain;
    seen.extend(std::iter::from_fn(|| received.try_recv().ok()));
    (seen, uncertain)
}

/// Codex can swallow the Enter (paste-burst window) and keep the text in its
/// composer. The retained composer gets exactly one more Enter, which submits.
#[cfg(unix)]
#[test]
fn ignored_codex_enter_with_text_still_in_composer_retries_once_and_submits() {
    let (writes, uncertain) = run_codex_queued_delivery(
        "codex-ignored-enter-retained",
        "\x1b[22;1H\x1b[2K\u{203a} wake the agent",
        Some(
            "\x1b[21;1H\u{2022} Working (1s \u{2022} esc to interrupt)\x1b[22;1H\x1b[2K\u{203a} Ask Codex to do anything",
        ),
    );
    assert_eq!(
        writes
            .iter()
            .filter(|write| write.as_slice() == b"\r")
            .count(),
        2,
        "one Enter for the delivery, exactly one retry"
    );
    assert!(!uncertain, "the retry's Working screen confirms the turn");
}

/// Real Codex 0.159 PTY capture (2026-09-30): a 1967-character brief typed into
/// a fresh composer collapses to `[Pasted Content 1967 chars]`. The first Enter
/// (200 ms after the plain text, as the old Codex gap did) is swallowed and the
/// placeholder stays in the composer; a bare Enter 5 s later submits and Codex
/// prints `Working` 0.18 s after it. The composer never shows the text, so the
/// retry must recognise the placeholder as the queued text.
#[cfg(unix)]
#[test]
fn swallowed_enter_on_a_long_codex_brief_is_retried_from_the_paste_placeholder() {
    struct ChannelWriter(std::sync::mpsc::Sender<Vec<u8>>);
    impl std::io::Write for ChannelWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.send(bytes.to_vec()).expect("record PTY write");
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    use crate::pty_capture::CaptureDirection::{Input, Output};

    let bytes = agent_prompt_fixture("codex-0.159-long-brief-swallowed-enter.tcap");
    let capture = crate::pty_capture::decode_capture(&bytes).expect("valid capture");
    let (rows, cols) = capture.geometry.expect("capture geometry");
    let records = capture.records;
    let inputs: Vec<usize> = (0..records.len())
        .filter(|&i| records[i].direction == Input)
        .collect();
    // Ctrl-U, brief, first Enter, second Enter.
    assert_eq!(inputs.len(), 4);
    let brief = String::from_utf8(records[inputs[1]].data.clone()).unwrap();
    // The capture predates the bracketed long-Codex payload: the brief was typed
    // as plain characters and its Enter was swallowed. Replaying it still proves
    // the retry from the placeholder; the write is now the framed paste.
    let framed_brief = format!("\x1b[200~{brief}\x1b[201~");
    let second_enter = inputs[3];

    let state = crate::state::tests_support::make_test_app_state();
    let sid = "codex-long-brief-swallowed-enter";
    agent_session(&state, sid, SHELL_IDLE);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .agent_type = Some("codex".into());
    let mut vt = VtLogBuffer::new(rows, cols, 2000);
    for record in records[..inputs[1]]
        .iter()
        .filter(|r| r.direction == Output)
    {
        vt.process(&record.data);
    }
    state.grid.vt_log_buffers.insert(sid.into(), Mutex::new(vt));
    state
        .session_maps
        .output_buffers
        .insert(sid.into(), Mutex::new(OutputRingBuffer::new(1 << 20)));
    assert_eq!(agent_submission_ack_kind(&state, sid), "ready_screen");
    let (writes, received) = std::sync::mpsc::channel();
    insert_session_with_writer(&state, sid, Box::new(ChannelWriter(writes)), TtyMode::Raw);
    let mut alerts = state.event_bus.subscribe();
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
    let replay = |reader: &mut ChunkProcessor, range: std::ops::Range<usize>| {
        for record in records[range].iter().filter(|r| r.direction == Output) {
            reader.process_chunk(
                &String::from_utf8_lossy(&record.data),
                &silence,
                sid,
                &state,
            );
        }
    };
    let mut seen = Vec::new();

    std::thread::scope(|scope| {
        scope.spawn(|| enqueue_user_command(&state, sid, &brief, None).unwrap());
        for expected in [b"\x15".as_slice(), framed_brief.as_bytes(), b"\r"] {
            let write = received
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap();
            assert_eq!(write, expected);
            seen.push(write);
        }
        let mut reader = ChunkProcessor::new(None, None);
        replay(&mut reader, inputs[1]..second_enter);
        // The terminal also answers the agent's queries through the PTY writer
        // during the replay; only the retry Enter ends the wait.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
            let Ok(write) = received.recv_timeout(left) else {
                break;
            };
            let is_retry = write == b"\r";
            seen.push(write);
            if is_retry {
                replay(&mut reader, second_enter..records.len());
                break;
            }
        }
    });

    assert_eq!(
        seen.iter().filter(|w| w.as_slice() == b"\r").count(),
        2,
        "the placeholder in the composer gets exactly one retry Enter"
    );
    assert!(
        !silence.lock().injection_delivery_uncertain,
        "the retry's Working screen confirms the turn"
    );
    assert!(
        std::iter::from_fn(|| alerts.try_recv().ok()).all(|event| !matches!(
            event,
            crate::state::AppEvent::McpToast { ref title, .. }
                if title == "Agent input was not confirmed"
        )),
        "no false failure toast"
    );
}

/// A retained composer that ignores the retry too stays uncertain; there is no
/// second retry.
#[cfg(unix)]
#[test]
fn codex_enter_retry_is_never_repeated() {
    let (writes, uncertain) = run_codex_queued_delivery(
        "codex-ignored-enter-twice",
        "\x1b[22;1H\x1b[2K\u{203a} wake the agent",
        Some("\x1b[22;1H\x1b[2K\u{203a} wake the agent"),
    );
    assert_eq!(
        writes
            .iter()
            .filter(|write| write.as_slice() == b"\r")
            .count(),
        2
    );
    assert!(uncertain);
}

/// An empty composer after the first Enter means nothing is left to submit.
#[cfg(unix)]
#[test]
fn codex_enter_is_not_retried_when_composer_is_empty() {
    let (writes, uncertain) = run_codex_queued_delivery(
        "codex-empty-composer",
        "\x1b[22;1H\x1b[2K\u{203a} Ask Codex to do anything",
        None,
    );
    assert_eq!(
        writes
            .iter()
            .filter(|write| write.as_slice() == b"\r")
            .count(),
        1,
        "an empty composer must not receive a second Enter"
    );
    assert!(uncertain, "no acknowledgement still reports uncertainty");
}

#[cfg(unix)]
#[test]
fn queued_agent_without_child_response_remains_uncertain() {
    for agent_type in ["codex", "claude"] {
        let state = crate::state::tests_support::make_test_app_state();
        let sid = format!("{agent_type}-silent-after-enter");
        agent_session(&state, &sid, SHELL_IDLE);
        state
            .session_maps
            .session_states
            .get_mut(&sid)
            .unwrap()
            .agent_type = Some(agent_type.into());
        state
            .grid
            .vt_log_buffers
            .insert(sid.clone(), Mutex::new(VtLogBuffer::new(24, 80, 1000)));
        state
            .session_maps
            .output_buffers
            .insert(sid.clone(), Mutex::new(OutputRingBuffer::new(1024)));
        let bytes = insert_recording_session(&state, &sid);
        let mut alerts = state.event_bus.subscribe();

        enqueue_user_command(&state, &sid, "wake the agent", None).unwrap();

        assert!(bytes.lock().unwrap().ends_with(b"\r"));
        assert!(
            state
                .session_maps
                .silence_states
                .get(&sid)
                .unwrap()
                .lock()
                .injection_delivery_uncertain,
            "a silent {agent_type} child cannot confirm its own queued turn"
        );
        assert!(
            std::iter::from_fn(|| alerts.try_recv().ok()).any(|event| matches!(
                event,
                crate::state::AppEvent::McpToast { level, .. } if level == "error"
            ))
        );
    }
}

#[cfg(unix)]
#[test]
fn queued_claude_hook_busy_four_seconds_after_enter_confirms_submission() {
    struct ChannelWriter(std::sync::mpsc::Sender<Vec<u8>>);
    impl std::io::Write for ChannelWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.send(bytes.to_vec()).expect("record PTY write");
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let state = crate::state::tests_support::make_test_app_state();
    let sid = "claude-hook-confirmed-queue";
    agent_session(&state, sid, SHELL_IDLE);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .agent_type = Some("claude".into());
    state
        .grid
        .vt_log_buffers
        .insert(sid.into(), Mutex::new(VtLogBuffer::new(24, 80, 1000)));
    state
        .session_maps
        .output_buffers
        .insert(sid.into(), Mutex::new(OutputRingBuffer::new(1024)));
    let (writes, received) = std::sync::mpsc::channel();
    insert_session_with_writer(&state, sid, Box::new(ChannelWriter(writes)), TtyMode::Raw);
    let mut alerts = state.event_bus.subscribe();
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
    let capture = String::from_utf8(agent_prompt_fixture(
        "claude-hooked-missing-question-20260921.raw",
    ))
    .expect("recorded Claude hook stream is UTF-8");
    let busy = capture.find("state=busy").expect("captured busy hook");
    let start = capture[..busy].rfind('\x1b').expect("hook escape start");
    let end = busy + capture[busy..].find("\x1b\\").expect("hook terminator") + 2;
    let busy_hook = &capture[start..end];

    std::thread::scope(|scope| {
        scope.spawn(|| enqueue_user_command(&state, sid, "wake the agent", None).unwrap());
        for expected in [b"\x15".as_slice(), b"wake the agent", b"\r"] {
            assert_eq!(
                received
                    .recv_timeout(std::time::Duration::from_secs(15))
                    .unwrap(),
                expected
            );
        }
        // The affected live session took 3.8 seconds to move a queued notice
        // from Enter into its transcript. Replay the real busy hook after that.
        std::thread::sleep(std::time::Duration::from_secs(4));
        let mut reader = ChunkProcessor::new(None, None);
        reader.process_chunk(busy_hook, &silence, sid, &state);
    });

    assert!(
        !state
            .session_maps
            .silence_states
            .get(sid)
            .unwrap()
            .lock()
            .injection_delivery_uncertain
    );
    assert!(
        std::iter::from_fn(|| alerts.try_recv().ok())
            .all(|event| !matches!(event, crate::state::AppEvent::McpToast { .. })),
        "Claude's busy hook must confirm queued Enter without a false toast"
    );
}

/// A Codex idle marker can precede its ready repaint. The old Working row is
/// still visible when Enter is written, then a fresh Ready and Working pair
/// arrive from the child. This is a real new turn despite that old row.
#[cfg(unix)]
#[test]
fn queued_codex_accepts_ready_then_working_after_enter() {
    struct ChannelWriter(std::sync::mpsc::Sender<Vec<u8>>);
    impl std::io::Write for ChannelWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.send(bytes.to_vec()).expect("record PTY write");
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let state = crate::state::tests_support::make_test_app_state();
    let sid = "late-ready-redraw";
    agent_session(&state, sid, SHELL_IDLE);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .agent_type = Some("codex".into());
    let mut vt = VtLogBuffer::new(24, 80, 1000);
    vt.process(b"\x1b[2J\x1b[21;1H\xe2\x80\xa2 Working (1s \xe2\x80\xa2 esc to interrupt)\x1b[22;1H\xe2\x80\xba Ask Codex to do anything");
    state.grid.vt_log_buffers.insert(sid.into(), Mutex::new(vt));
    state
        .session_maps
        .output_buffers
        .insert(sid.into(), Mutex::new(OutputRingBuffer::new(1024)));
    assert_eq!(agent_submission_ack_kind(&state, sid), "working_screen");
    let (writes, received) = std::sync::mpsc::channel();
    insert_session_with_writer(&state, sid, Box::new(ChannelWriter(writes)), TtyMode::Raw);
    let mut alerts = state.event_bus.subscribe();
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();

    std::thread::scope(|scope| {
        scope.spawn(|| enqueue_user_command(&state, sid, "wake the agent", None).unwrap());
        for expected in [b"\x15".as_slice(), b"wake the agent", b"\r"] {
            assert_eq!(
                received
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap(),
                expected
            );
        }
        let mut reader = ChunkProcessor::new(None, None);
        reader.process_chunk(
            "\x1b[2J\x1b[22;1H› Ask Codex to do anything",
            &silence,
            sid,
            &state,
        );
        reader.process_chunk(
            "\x1b[21;1H• Working (1s • esc to interrupt)",
            &silence,
            sid,
            &state,
        );
    });

    assert!(
        std::iter::from_fn(|| alerts.try_recv().ok())
            .all(|event| !matches!(event, crate::state::AppEvent::McpToast { .. })),
        "a new Ready-to-Working transition after Enter must confirm delivery"
    );
}

#[cfg(all(unix, feature = "dictation"))]
fn set_question_confident(state: &AppState, session_id: &str, confident: bool) {
    state
        .session_maps
        .session_states
        .get_mut(session_id)
        .expect("agent session")
        .question_confident = confident;
}

#[cfg(all(unix, feature = "dictation"))]
fn shell_state_of(state: &AppState, session_id: &str) -> u8 {
    state
        .session_maps
        .shell_states
        .get(session_id)
        .expect("shell atom")
        .load(std::sync::atomic::Ordering::Relaxed)
}

/// Refused outright, as before: not an agent, gone, or empty.
#[cfg(all(unix, feature = "dictation"))]
#[test]
fn a_voice_turn_is_refused_for_shells_dead_sessions_and_empty_text() {
    use std::sync::atomic::AtomicU8;
    let state = crate::state::tests_support::make_test_app_state();
    state
        .session_maps
        .shell_states
        .insert("shell".to_string(), AtomicU8::new(SHELL_IDLE));
    state
        .session_maps
        .session_states
        .insert("shell".to_string(), crate::state::SessionState::default());
    insert_recording_session(&state, "shell");
    assert!(write_voice_turn(&state, "shell", "ls").is_err());
    agent_session(&state, "gone", SHELL_BUSY);
    assert!(write_voice_turn(&state, "gone", "hi").is_err());
    agent_session(&state, "blank", SHELL_BUSY);
    insert_recording_session(&state, "blank");
    assert!(write_voice_turn(&state, "blank", "  ").is_err());
}

#[test]
fn deliver_noop_for_non_agent() {
    let state = crate::state::tests_support::make_test_app_state();
    // No session_states entry → not an agent.
    deliver_notice_to_pty(&state, "ghost", "hi");
    assert!(
        !state.pending_injections.contains_key("ghost"),
        "non-agent must never queue"
    );
}

#[test]
fn failed_not_started_injection_rolls_back_claim_and_requeues() {
    // The test state has no live PTY session, so composer lookup fails before
    // any byte can be written. The delivery claim must be rolled back and the
    // message kept pending instead of leaving a false BUSY state.
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "idle", SHELL_IDLE);
    deliver_notice_to_pty(&state, "idle", "now");
    assert!(
        state
            .session_maps
            .shell_states
            .get("idle")
            .is_some_and(|state| state.load(Ordering::Acquire) == SHELL_IDLE),
        "a claim that never reached PTY I/O must restore IDLE"
    );
    assert_eq!(
        state
            .pending_injections
            .get("idle")
            .and_then(|queue| queue.front().map(|entry| entry.text().to_string())),
        Some("now".to_string()),
        "a not-started delivery must remain retryable"
    );
}

#[test]
fn partial_write_is_uncertain_not_not_started() {
    struct PartialThenError {
        wrote_once: bool,
    }
    impl std::io::Write for PartialThenError {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.wrote_once {
                Err(std::io::Error::other("injected failure"))
            } else {
                self.wrote_once = true;
                Ok(bytes.len().min(2))
            }
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let mut writer = PartialThenError { wrote_once: false };
    let failure = write_all_with_progress(&mut writer, b"payload", 0).unwrap_err();
    assert_eq!(failure.0, 2, "partial progress must be retained");
    assert!(failure.1.contains("injected failure"));
}

#[cfg(unix)]
struct FailingWriter;

#[cfg(unix)]
#[test]
fn concurrent_user_input_and_terminal_reply_are_both_serialized() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let bytes = Arc::new(std::sync::Mutex::new(Vec::new()));
    insert_session_with_writer(
        &state,
        "serialized-writes",
        Box::new(RecordingWriter {
            bytes: Arc::clone(&bytes),
        }),
        TtyMode::Raw,
    );

    let writer = state.pty_writer("serialized-writes").unwrap();
    let held = writer.lock();
    let input_state = Arc::clone(&state);
    let input = std::thread::spawn(move || {
        input_state
            .write_pty_parts("serialized-writes", &[b"input"])
            .unwrap();
    });
    let reply_state = Arc::clone(&state);
    let reply = std::thread::spawn(move || {
        write_terminal_reply(&reply_state, "serialized-writes", b"reply", "test");
    });

    drop(held);
    input.join().unwrap();
    reply.join().unwrap();
    let output = bytes.lock().unwrap().clone();
    assert!(
        output == b"inputreply" || output == b"replyinput",
        "{output:?}"
    );
}

#[cfg(unix)]
#[test]
fn multiple_terminal_replies_keep_reader_order() {
    let state = crate::state::tests_support::make_test_app_state();
    let bytes = Arc::new(std::sync::Mutex::new(Vec::new()));
    insert_session_with_writer(
        &state,
        "reply-order",
        Box::new(RecordingWriter {
            bytes: Arc::clone(&bytes),
        }),
        TtyMode::Raw,
    );

    write_terminal_reply(&state, "reply-order", b"first", "test");
    write_terminal_reply(&state, "reply-order", b"second", "test");
    assert_eq!(*bytes.lock().unwrap(), b"firstsecond");
}

/// Claude Code emits `ESC[c` before it leaves cooked mode. Answering it there
/// does not reach Claude — a canonical read blocks for a newline the reply
/// never contains, and `ECHO` paints `ESC[?6c` as the literal `^[[?6c`, which
/// is the garbage Boss saw above the startup banner (capture `f2bddfb0`,
/// 2026-09-07). The querier re-asks from raw mode, so nothing is lost.
#[cfg(unix)]
#[test]
fn terminal_reply_is_withheld_while_the_tty_is_canonical() {
    let state = crate::state::tests_support::make_test_app_state();
    let bytes = Arc::new(std::sync::Mutex::new(Vec::new()));
    insert_session_with_writer(
        &state,
        "echoing-tty",
        Box::new(RecordingWriter {
            bytes: Arc::clone(&bytes),
        }),
        TtyMode::Cooked,
    );

    write_terminal_reply(&state, "echoing-tty", b"\x1b[?6c", "DA1");
    assert!(
        bytes.lock().unwrap().is_empty(),
        "a reply the querier cannot read must not be painted on its screen"
    );
}

/// The case that decides which flag the gate reads. In cbreak the tty still
/// echoes, so keying on `ECHO` would withhold here — but `ICANON` is off, so
/// the querier reads the reply immediately and nothing will ever resend it.
/// Withholding would trade Boss's cosmetic `^[[?6c` for a hung agent.
#[cfg(unix)]
#[test]
fn terminal_reply_is_delivered_in_cbreak_even_though_the_tty_echoes() {
    let state = crate::state::tests_support::make_test_app_state();
    let bytes = Arc::new(std::sync::Mutex::new(Vec::new()));
    insert_session_with_writer(
        &state,
        "cbreak-tty",
        Box::new(RecordingWriter {
            bytes: Arc::clone(&bytes),
        }),
        TtyMode::Cbreak,
    );

    write_terminal_reply(&state, "cbreak-tty", b"\x1b[?6c", "DA1");
    assert_eq!(
        *bytes.lock().unwrap(),
        b"\x1b[?6c",
        "cbreak delivers the reply; withholding it would hang the querier"
    );
}

#[cfg(unix)]
#[test]
fn shared_pty_write_reports_writer_failure_and_teardown() {
    let state = crate::state::tests_support::make_test_app_state();
    insert_session_with_writer(
        &state,
        "failed-writer",
        Box::new(FailingWriter),
        TtyMode::Raw,
    );

    let error = state
        .write_pty_parts("failed-writer", &[b"reply"])
        .expect_err("writer error must be observable");
    assert!(error.contains("injected PTY failure"), "{error}");

    state.session_maps.sessions.remove("failed-writer");
    let error = state
        .write_pty_parts("failed-writer", &[b"late reply"])
        .expect_err("removed session must reject writes");
    assert_eq!(error, "Session not found");
    write_terminal_reply(&state, "failed-writer", b"late reply", "test");
}

/// Accepts every write before `fail_at` whole. At `fail_at` it either fails
/// outright or, when `partial`, takes two bytes and fails the next call.
#[cfg(unix)]
struct FailsAtWrite {
    calls: usize,
    fail_at: usize,
    partial: bool,
}

/// Records every `write` call with the instant it arrived, so a test can tell
/// two writes apart in time rather than only in order.
#[cfg(unix)]
type TimedWrites = Arc<std::sync::Mutex<Vec<(std::time::Instant, Vec<u8>)>>>;

#[cfg(unix)]
struct TimedWriter {
    writes: TimedWrites,
}

#[cfg(unix)]
#[test]
fn agent_submission_with_unrecognized_type_keeps_the_unverified_gap_and_plain_payload() {
    let state = crate::state::tests_support::make_test_app_state();
    let writes = Arc::new(std::sync::Mutex::new(Vec::new()));
    insert_session_with_writer(
        &state,
        "unrecognized-agent",
        Box::new(TimedWriter {
            writes: Arc::clone(&writes),
        }),
        TtyMode::Raw,
    );
    agent_session(&state, "unrecognized-agent", SHELL_IDLE);
    state
        .session_maps
        .session_states
        .get_mut("unrecognized-agent")
        .unwrap()
        .agent_type = Some("future-agent".into());

    write_agent_command_to_pty(&state, "unrecognized-agent", "review this").unwrap();

    let writes = writes.lock().unwrap();
    assert_eq!(
        writes
            .iter()
            .map(|(_, bytes)| bytes.as_slice())
            .collect::<Vec<_>>(),
        vec![
            b"\x15".as_slice(),
            b"review this".as_slice(),
            b"\r".as_slice()
        ]
    );
    assert!(
        writes[2].0 - writes[1].0 >= std::time::Duration::from_millis(195),
        "an unverified agent keeps the long Enter gap"
    );
}

#[test]
fn ready_prompt_delivery_attempts_and_requeues_when_pty_is_missing() {
    // codex idles at its ready prompt with awaiting_input=true (low-confidence
    // silence heuristic) — delivery must inject, not queue forever (story 091).
    use std::sync::atomic::AtomicU8;
    let state = crate::state::tests_support::make_test_app_state();
    state
        .session_maps
        .shell_states
        .insert("codex".to_string(), AtomicU8::new(SHELL_IDLE));
    state.session_maps.session_states.insert(
        "codex".to_string(),
        crate::state::SessionState {
            agent_type: Some("codex".to_string()),
            awaiting_input: true,
            question_confident: false,
            ..Default::default()
        },
    );
    let mut silence = SilenceState::new();
    silence.confirm_idle();
    state
        .session_maps
        .silence_states
        .insert("codex".to_string(), Arc::new(Mutex::new(silence)));
    deliver_notice_to_pty(&state, "codex", "[TUIC message from lead] go");
    assert_eq!(
        state
            .pending_injections
            .get("codex")
            .and_then(|queue| queue.front().map(|entry| entry.text().to_string())),
        Some("[TUIC message from lead] go".to_string()),
        "ready-prompt delivery must stay retryable when PTY lookup fails"
    );
}

#[test]
fn tuic_osc_suggest_parsed_from_pty_stream() {
    use crate::terminal_grid::TerminalGrid;
    let mut grid = TerminalGrid::new(24, 80, 1000);
    grid.process(b"\x1b]7770;suggest=Fix bug|Run tests|Deploy\x07");
    let events = grid.drain_events();
    let tuic: Vec<_> = events
        .into_iter()
        .filter(|e| matches!(e, crate::terminal_grid::TermEvent::Tuic { .. }))
        .collect();
    assert_eq!(tuic.len(), 1);
    if let crate::terminal_grid::TermEvent::Tuic { verb, payload, .. } = &tuic[0] {
        assert_eq!(verb, "suggest");
        let items: Vec<String> = payload.split('|').map(|s| s.trim().to_string()).collect();
        assert_eq!(items, vec!["Fix bug", "Run tests", "Deploy"]);
    }
}

#[test]
fn tuic_osc_intent_with_title_parsed() {
    use crate::terminal_grid::TerminalGrid;
    let mut grid = TerminalGrid::new(24, 80, 1000);
    grid.process(b"\x1b]7770;intent=Refactoring auth (Auth)\x07");
    let events = grid.drain_events();
    let tuic: Vec<_> = events
        .into_iter()
        .filter(|e| matches!(e, crate::terminal_grid::TermEvent::Tuic { .. }))
        .collect();
    assert_eq!(tuic.len(), 1);
    if let crate::terminal_grid::TermEvent::Tuic { verb, payload, .. } = &tuic[0] {
        assert_eq!(verb, "intent");
        assert_eq!(payload, "Refactoring auth (Auth)");
    }
}

#[test]
fn tuic_osc_block_start_parsed() {
    use crate::terminal_grid::TerminalGrid;
    let mut grid = TerminalGrid::new(24, 80, 1000);
    grid.process(b"\x1b]7770;block=start\x07");
    let events = grid.drain_events();
    let tuic: Vec<_> = events
        .into_iter()
        .filter(|e| matches!(e, crate::terminal_grid::TermEvent::Tuic { .. }))
        .collect();
    assert_eq!(tuic.len(), 1);
    if let crate::terminal_grid::TermEvent::Tuic { verb, payload, .. } = &tuic[0] {
        assert_eq!(verb, "block");
        assert_eq!(payload, "start");
    }
}

#[test]
fn tuic_osc_block_end_with_exit_code_parsed() {
    let payload = "end;1".to_string();
    let (action, exit_code) = if let Some(rest) = payload.strip_prefix("end;") {
        ("end".to_string(), rest.parse::<i32>().ok())
    } else {
        (payload.clone(), None)
    };
    assert_eq!(action, "end");
    assert_eq!(exit_code, Some(1));
}

#[test]
fn tuic_osc_block_end_without_exit_code() {
    let payload = "end".to_string();
    let (action, exit_code) = if let Some(rest) = payload.strip_prefix("end;") {
        ("end".to_string(), rest.parse::<i32>().ok())
    } else {
        (payload.clone(), None)
    };
    assert_eq!(action, "end");
    assert_eq!(exit_code, None);
}

#[test]
fn tuic_osc_block_invalid_action_ignored() {
    let payload = "invalid".to_string();
    let (action, _exit_code) = if let Some(rest) = payload.strip_prefix("end;") {
        ("end".to_string(), rest.parse::<i32>().ok())
    } else {
        (payload.clone(), None)
    };
    let is_valid = action == "start" || action == "end";
    assert!(!is_valid, "invalid action should not produce an event");
}

#[test]
fn tuic_osc_state_transitions_shell_state() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-tuic-state";
    state.session_maps.shell_states.insert(
        session_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_IDLE),
    );
    state
        .session_maps
        .shell_state_since_ms
        .insert(session_id.to_string(), std::sync::atomic::AtomicU64::new(0));

    let proc = ChunkProcessor::new(None, None);
    proc.handle_tuic_state("busy", session_id, &state);

    let current = state
        .session_maps
        .shell_states
        .get(session_id)
        .unwrap()
        .load(std::sync::atomic::Ordering::Acquire);
    assert_eq!(current, SHELL_BUSY);

    proc.handle_tuic_state("idle", session_id, &state);
    let current = state
        .session_maps
        .shell_states
        .get(session_id)
        .unwrap()
        .load(std::sync::atomic::Ordering::Acquire);
    assert_eq!(current, SHELL_IDLE);
}

#[test]
fn tuic_osc_state_emits_shell_state_event() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-tuic-emit";
    state.session_maps.shell_states.insert(
        session_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_IDLE),
    );
    state
        .session_maps
        .shell_state_since_ms
        .insert(session_id.to_string(), std::sync::atomic::AtomicU64::new(0));

    let mut rx = state.event_bus.subscribe();

    let proc = ChunkProcessor::new(None, None);
    proc.handle_tuic_state("busy", session_id, &state);

    let evt = rx.try_recv();
    assert!(
        evt.is_ok(),
        "event_bus should have received a shell state event"
    );
    if let Ok(crate::state::AppEvent::PtyParsed {
        session_id: sid,
        parsed,
    }) = evt
    {
        assert_eq!(sid, session_id);
        assert_eq!(parsed["type"], "shell-state");
        assert_eq!(parsed["state"], "busy");
    } else {
        panic!("expected PtyParsed event with shell-state");
    }
}

#[test]
fn tuic_osc_state_unknown_verb_ignored() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-tuic-unknown";
    state.session_maps.shell_states.insert(
        session_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_IDLE),
    );

    let proc = ChunkProcessor::new(None, None);
    proc.handle_tuic_state("thinking", session_id, &state);

    let current = state
        .session_maps
        .shell_states
        .get(session_id)
        .unwrap()
        .load(std::sync::atomic::Ordering::Acquire);
    assert_eq!(
        current, SHELL_IDLE,
        "unknown state should not change shell_states"
    );
}

#[test]
fn tuic_state_awaiting_yields_confident_question() {
    match tuic_state_awaiting_event("awaiting", 0) {
        Some(ParsedEvent::Question {
            confident,
            prompt_text,
        }) => {
            assert!(confident, "hook awaiting must be a confident question");
            assert_eq!(prompt_text, "", "hook awaiting carries no prompt text");
        }
        other => panic!("expected confident Question, got {other:?}"),
    }
}

#[test]
fn tuic_state_prompt_yields_userinput_clear_with_prompt_line() {
    // The submit hook's absolute prompt row (history_size + cursor row, here 42)
    // must reach the UserInput event so the frontend can mark the user-prompt
    // line on the scrollbar.
    match tuic_state_awaiting_event("prompt", 42) {
        Some(ParsedEvent::UserInput { content, line }) => {
            assert_eq!(content, "", "prompt clear must not overwrite last_prompt");
            assert_eq!(line, 42, "prompt UserInput must carry the prompt row");
        }
        other => panic!("expected UserInput clear, got {other:?}"),
    }
}

#[test]
fn tuic_state_busy_carries_no_prompt_row() {
    // Catches #1388: PreToolUse fires state=busy on every tool call, and each one
    // used to mark a user-prompt tick at the cursor row, painting a solid green
    // band on the scrollbar. Busy still clears awaiting but has no prompt row.
    match tuic_state_awaiting_event("busy", 42) {
        Some(ParsedEvent::UserInput { content, line }) => {
            assert_eq!(content, "");
            assert_eq!(line, -1, "busy must not claim a prompt row");
        }
        other => panic!("expected UserInput clear, got {other:?}"),
    }
}

#[test]
fn tuic_state_prompt_drives_shell_busy() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-tuic-prompt";
    state.session_maps.shell_states.insert(
        session_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_IDLE),
    );
    state
        .session_maps
        .shell_state_since_ms
        .insert(session_id.to_string(), std::sync::atomic::AtomicU64::new(0));

    let proc = ChunkProcessor::new(None, None);
    proc.handle_tuic_state("prompt", session_id, &state);

    let current = state
        .session_maps
        .shell_states
        .get(session_id)
        .unwrap()
        .load(std::sync::atomic::Ordering::Acquire);
    assert_eq!(current, SHELL_BUSY);
}

#[test]
fn tuic_state_idle_yields_no_awaiting_event() {
    assert!(
        tuic_state_awaiting_event("idle", 0).is_none(),
        "idle only transitions shell_state; it pushes no awaiting event"
    );
}

#[test]
fn tuic_state_unknown_yields_no_awaiting_event() {
    assert!(
        tuic_state_awaiting_event("thinking", 0).is_none(),
        "unknown verb must push no awaiting event"
    );
}

#[test]
fn question_suppress_filters_only_questions_when_instrumented() {
    let q_low = ParsedEvent::Question {
        prompt_text: "?".into(),
        confident: false,
    };
    let q_high = ParsedEvent::Question {
        prompt_text: "Proceed?".into(),
        confident: true,
    };
    let other = ParsedEvent::UserInput {
        content: "hi".into(),
        line: -1,
    };
    // Instrumented: every Question (silence + regex) is suppressed.
    assert!(suppress_heuristic_question(true, &q_low));
    assert!(suppress_heuristic_question(true, &q_high));
    // Non-questions are never suppressed (idle/busy/etc. pass through).
    assert!(!suppress_heuristic_question(true, &other));
    // Not instrumented: nothing is suppressed.
    assert!(!suppress_heuristic_question(false, &q_low));
}

// --- Raw-stream OSC reassembly ----------------------------------------

/// The failure the fixture caught: a PTY read ends mid-`ESC]777;…`. Each
/// half alone matches nothing, so without a carry the awaiting signal is
/// lost — silently, which is how it reached Boss's screen.
#[test]
fn raw_stream_reassembles_an_osc_split_across_reads() {
    let whole = "\x1b]777;notify;Claude Code;Claude needs your permission\x07";
    for split in [1, 8, 20, whole.len() - 1] {
        let mut carry = String::new();
        let mut events = Vec::new();
        raw_stream_events(&mut carry, &whole[..split], &mut events);
        raw_stream_events(&mut carry, &whole[split..], &mut events);
        assert_eq!(
            awaiting_prompts(&events),
            vec!["Claude needs your permission".to_string()],
            "split at {split} must yield exactly one awaiting signal"
        );
    }
}

/// A sequence that arrived whole leaves nothing behind, so the next chunk
/// cannot re-match it. One notification, one badge.
#[test]
fn raw_stream_does_not_refire_a_complete_sequence() {
    let mut carry = String::new();
    let mut events = Vec::new();
    raw_stream_events(
        &mut carry,
        "\x1b]777;notify;Claude Code;Claude needs your permission\x07",
        &mut events,
    );
    assert!(carry.is_empty(), "complete sequence must not be carried");
    raw_stream_events(&mut carry, "ordinary output\r\n", &mut events);
    assert_eq!(awaiting_prompts(&events).len(), 1);
}

/// An OSC that never terminates (or one whose payload we do not parse, like
/// a long OSC 52 clipboard blob) must not pin memory for the session.
#[test]
fn raw_stream_carry_is_bounded() {
    let mut carry = String::new();
    let mut events = Vec::new();
    let huge = format!("\x1b]52;c;{}", "A".repeat(MAX_RAW_CARRY * 4));
    raw_stream_events(&mut carry, &huge, &mut events);
    assert!(
        carry.len() <= MAX_RAW_CARRY,
        "carry grew to {} bytes",
        carry.len()
    );
}

/// ST-terminated sequences close the carry too — otherwise every agent that
/// ends OSC with ESC-backslash would carry its whole stream forward.
#[test]
fn raw_stream_carry_recognises_st_termination() {
    assert!(unterminated_osc_tail("\x1b]777;notify;Codex;approval\x1b\\").is_empty());
    assert!(!unterminated_osc_tail("\x1b]777;notify;Codex;approval").is_empty());
}

// --- Awaiting-signal fixtures -----------------------------------------
//
// Captures of real agent output, replayed through the SAME composition the
// PTY hot path runs. Unit tests cover each parser in isolation; what kept
// breaking was the pipeline around them — which signals survive the hook
// suppression, and which never reach a parser at all. That gap is what a
// fixture closes: one file per observed failure, byte-for-byte off a live
// session, no mocks.
//
// Capturing a new one:
//   POST /diagnostics/capture with {"enabled":true,"session_id":"<id>"}
//   BEFORE reproducing, then POST {"enabled":false}. Copy the exact `.tcap`
//   file reported by GET /diagnostics/capture from the config-dir captures/
//   directory into src/fixtures/agent_prompts/. `/sessions/:id/output` is a
//   rendered/ring-buffer snapshot and is not valid raw-stream evidence.

fn agent_prompt_fixture(name: &str) -> Vec<u8> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/fixtures/agent_prompts")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|primary_error| {
        let supplied = std::env::var_os("TUIC_CAPTURE_FIXTURE_DIR")
            .map(std::path::PathBuf::from)
            .map(|dir| dir.join(name));
        supplied
            .as_deref()
            .and_then(|fallback| std::fs::read(fallback).ok())
            .unwrap_or_else(|| panic!("missing fixture {}: {primary_error}", path.display()))
    })
}

/// Codex CLI 0.156, captured through `/diagnostics/capture` on 2026-09-24.
/// The TUI redraws this marker through several growing cursor prefixes. Replay
/// the production chunk processor rather than the row parser alone: an open
/// intent must absorb every growing prefix before Progress sees it.
#[test]
#[cfg(unix)]
fn captured_codex_streaming_intent_emits_one_complete_marker() {
    #[cfg(not(feature = "desktop"))]
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test Tokio runtime");
    #[cfg(not(feature = "desktop"))]
    let _runtime_guard = runtime.enter();
    let config = tempfile::tempdir().expect("config directory");
    let _config_guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let project = tempfile::tempdir().expect("registered project");
    crate::config::replace_repositories_for_test(serde_json::json!({
        "repos": { project.path().to_string_lossy(): {} }
    }))
    .expect("register project");
    let capture = crate::pty_capture::decode_capture(&agent_prompt_fixture(
        "codex-streaming-intent-20260924.tcap",
    ))
    .expect("valid framed capture");
    // A capture taken before the session's grid is initialized has no recorded
    // geometry; replay it at a fixed, spacious size rather than rejecting valid
    // stream evidence before it reaches the parser.
    let (rows, cols) = capture.geometry.unwrap_or((41, 128));
    let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "captured-codex-streaming-intent";
    crate::repo_watcher::start_watching(project.path().to_str().expect("UTF-8 path"), &state)
        .expect("start project watcher");
    crate::state::tests_support::insert_dummy_session(&state, sid);
    crate::state::tests_support::set_session_cwd(
        &state,
        sid,
        project.path().to_str().expect("UTF-8 path"),
    );
    agent_session(&state, sid, SHELL_BUSY);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .expect("agent session")
        .agent_type = Some("codex".to_string());
    state.grid.vt_log_buffers.insert(
        sid.to_string(),
        Mutex::new(crate::state::VtLogBuffer::new(rows, cols, 2000)),
    );
    let silence = state
        .session_maps
        .silence_states
        .get(sid)
        .expect("agent silence state")
        .clone();
    let mut parsed_events = state.event_bus.subscribe();
    let mut processor = ChunkProcessor::new(None, None);

    for record in capture.records {
        if record.direction == crate::pty_capture::CaptureDirection::Output {
            processor.process_chunk(
                std::str::from_utf8(&record.data).expect("Codex capture is UTF-8"),
                &silence,
                sid,
                &state,
            );
        }
    }

    let intents: Vec<_> = std::iter::from_fn(|| parsed_events.try_recv().ok())
        .filter_map(|event| match event {
            crate::state::AppEvent::PtyParsed { parsed, .. }
                if parsed.get("type").and_then(serde_json::Value::as_str) == Some("intent") =>
            {
                Some((
                    parsed
                        .get("text")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    parsed
                        .get("title")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                ))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        intents,
        vec![(
            "validating the streaming capture".to_string(),
            Some("Capture test".to_string())
        )],
        "the captured growing prefixes must produce one complete intent"
    );
    let entries = crate::progress::ProgressStore::open()
        .expect("open Progress journal")
        .list(
            &project
                .path()
                .canonicalize()
                .expect("canonical project")
                .to_string_lossy(),
            &crate::progress::ProgressListInput::default(),
        )
        .expect("list Progress journal")
        .entries;
    assert!(matches!(entries.as_slice(), [entry]
        if entry.kind == crate::progress::ProgressKind::Intent
            && entry.text == "validating the streaming capture"));
}

/// Codex redraws its streaming response in place, then returns its cursor to
/// the input row. Cursor-local filtering therefore cannot see the marker.
/// This is deliberately raw VT input: CSI H moves back to the same row and
/// CSI K clears the prior render just as the captured Codex repaint stream
/// does.
#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn codex_cursor_away_repaint_journals_only_the_closed_intent_and_title() {
    let config = tempfile::tempdir().expect("config directory");
    let _config_guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let project = tempfile::tempdir().expect("registered project");
    crate::config::replace_repositories_for_test(serde_json::json!({
        "repos": { project.path().to_string_lossy(): {} }
    }))
    .expect("register project for ownership");

    let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
    crate::repo_watcher::start_watching(project.path().to_str().expect("UTF-8 path"), &state)
        .expect("register project watcher");
    let sid = "codex-narrow-streaming-intent";
    crate::state::tests_support::insert_dummy_session(&state, sid);
    crate::state::tests_support::set_session_cwd(
        &state,
        sid,
        project.path().to_str().expect("UTF-8 path"),
    );
    agent_session(&state, sid, SHELL_BUSY);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .expect("agent session")
        .agent_type = Some("codex".to_string());
    state.grid.vt_log_buffers.insert(
        sid.to_string(),
        Mutex::new(crate::state::VtLogBuffer::new(16, 128, 2000)),
    );
    let silence = state
        .session_maps
        .silence_states
        .get(sid)
        .expect("agent silence state")
        .clone();
    let mut parsed_events = state.event_bus.subscribe();
    let mut processor = ChunkProcessor::new(None, None);
    let prefixes = [
        "Individuo",
        "Individuo la regressione dell'a capo nel terminale mobile e ripristino",
        "Individuo la regressione dell'a capo nel terminale mobile e ripristino il comportamento corretto (Accapo mobile)",
    ];

    processor.process_chunk("\x1b[?1049h", &silence, sid, &state);
    for prefix in prefixes {
        processor.process_chunk(
            &format!("\x1b[4;1H\x1b[2K• intent: {prefix}\x1b[14;3H"),
            &silence,
            sid,
            &state,
        );
    }

    let intents: Vec<_> = std::iter::from_fn(|| parsed_events.try_recv().ok())
        .filter_map(|event| match event {
            crate::state::AppEvent::PtyParsed { parsed, .. }
                if parsed.get("type").and_then(serde_json::Value::as_str) == Some("intent") =>
            {
                Some((
                    parsed
                        .get("text")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    parsed
                        .get("title")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                ))
            }
            _ => None,
        })
        .collect();
    let text = prefixes
        .last()
        .expect("complete prefix")
        .split(" (")
        .next()
        .unwrap();
    assert_eq!(
        intents,
        vec![(text.to_string(), Some("Accapo mobile".to_string()))]
    );

    let entries = crate::progress::ProgressStore::open()
        .expect("open Progress journal")
        .list(
            &project
                .path()
                .canonicalize()
                .expect("canonical project")
                .to_string_lossy(),
            &crate::progress::ProgressListInput::default(),
        )
        .expect("list Progress journal")
        .entries;
    assert!(matches!(entries.as_slice(), [entry]
        if entry.kind == crate::progress::ProgressKind::Intent && entry.text == text));
}

/// A legacy intent need not have a title. Once its row is no longer live under
/// the cursor, the production parser must journal it rather than treating the
/// missing optional title as a streaming redraw.
#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn codex_cursor_away_titleless_intent_is_journaled_once() {
    let config = tempfile::tempdir().expect("config directory");
    let _config_guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let project = tempfile::tempdir().expect("registered project");
    crate::config::replace_repositories_for_test(serde_json::json!({
        "repos": { project.path().to_string_lossy(): {} }
    }))
    .expect("register project for ownership");

    let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
    crate::repo_watcher::start_watching(project.path().to_str().expect("UTF-8 path"), &state)
        .expect("register project watcher");
    let sid = "codex-titleless-intent";
    crate::state::tests_support::insert_dummy_session(&state, sid);
    crate::state::tests_support::set_session_cwd(
        &state,
        sid,
        project.path().to_str().expect("UTF-8 path"),
    );
    agent_session(&state, sid, SHELL_BUSY);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .expect("agent session")
        .agent_type = Some("codex".to_string());
    state.grid.vt_log_buffers.insert(
        sid.to_string(),
        Mutex::new(crate::state::VtLogBuffer::new(16, 128, 2000)),
    );
    let silence = state
        .session_maps
        .silence_states
        .get(sid)
        .expect("agent silence state")
        .clone();
    let mut parsed_events = state.event_bus.subscribe();
    let mut processor = ChunkProcessor::new(None, None);
    let text = "Read the configuration loader";

    processor.process_chunk("\x1b[?1049h", &silence, sid, &state);
    processor.process_chunk(
        &format!("\x1b[4;1H\x1b[2K• intent: {text}\r\n"),
        &silence,
        sid,
        &state,
    );

    let intents: Vec<_> = std::iter::from_fn(|| parsed_events.try_recv().ok())
        .filter_map(|event| match event {
            crate::state::AppEvent::PtyParsed { parsed, .. }
                if parsed.get("type").and_then(serde_json::Value::as_str) == Some("intent") =>
            {
                Some((
                    parsed
                        .get("text")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    parsed
                        .get("title")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                ))
            }
            _ => None,
        })
        .collect();
    assert_eq!(intents, vec![(text.to_string(), None)]);

    let entries = crate::progress::ProgressStore::open()
        .expect("open Progress journal")
        .list(
            &project
                .path()
                .canonicalize()
                .expect("canonical project")
                .to_string_lossy(),
            &crate::progress::ProgressListInput::default(),
        )
        .expect("list Progress journal")
        .entries;
    assert!(matches!(entries.as_slice(), [entry]
        if entry.kind == crate::progress::ProgressKind::Intent && entry.text == text));
}

/// A title can close an intent after it soft-wraps. The logical marker must be
/// retained when Codex returns the cursor to its composer in the same chunk.
#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn codex_narrow_cursor_away_intent_journals_full_text_and_title_once() {
    let config = tempfile::tempdir().expect("config directory");
    let _config_guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let project = tempfile::tempdir().expect("registered project");
    crate::config::replace_repositories_for_test(serde_json::json!({
        "repos": { project.path().to_string_lossy(): {} }
    }))
    .expect("register project for ownership");

    let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
    crate::repo_watcher::start_watching(project.path().to_str().expect("UTF-8 path"), &state)
        .expect("register project watcher");
    let sid = "codex-wrapped-away-intent";
    crate::state::tests_support::insert_dummy_session(&state, sid);
    crate::state::tests_support::set_session_cwd(
        &state,
        sid,
        project.path().to_str().expect("UTF-8 path"),
    );
    agent_session(&state, sid, SHELL_BUSY);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .expect("agent session")
        .agent_type = Some("codex".to_string());
    state.grid.vt_log_buffers.insert(
        sid.to_string(),
        Mutex::new(crate::state::VtLogBuffer::new(16, 20, 2000)),
    );
    let silence = state
        .session_maps
        .silence_states
        .get(sid)
        .expect("agent silence state")
        .clone();
    let mut parsed_events = state.event_bus.subscribe();
    let mut processor = ChunkProcessor::new(None, None);
    let text = "Restore the mobile terminal line wrapping behavior";
    let title = "Mobile wrapping";

    processor.process_chunk("\x1b[?1049h", &silence, sid, &state);
    processor.process_chunk(
        &format!("\x1b[4;1H\x1b[2K• intent: {text} ({title})\x1b[14;3H"),
        &silence,
        sid,
        &state,
    );

    let intents: Vec<_> = std::iter::from_fn(|| parsed_events.try_recv().ok())
        .filter_map(|event| match event {
            crate::state::AppEvent::PtyParsed { parsed, .. }
                if parsed.get("type").and_then(serde_json::Value::as_str) == Some("intent") =>
            {
                Some((
                    parsed
                        .get("text")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    parsed
                        .get("title")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                ))
            }
            _ => None,
        })
        .collect();
    assert_eq!(intents, vec![(text.to_string(), Some(title.to_string()))]);

    let entries = crate::progress::ProgressStore::open()
        .expect("open Progress journal")
        .list(
            &project
                .path()
                .canonicalize()
                .expect("canonical project")
                .to_string_lossy(),
            &crate::progress::ProgressListInput::default(),
        )
        .expect("list Progress journal")
        .entries;
    assert!(matches!(entries.as_slice(), [entry]
        if entry.kind == crate::progress::ProgressKind::Intent && entry.text == text));
}

/// At 20 columns the completed marker spans six soft-wrapped rows. The cursor
/// remains on the marker while Codex repaints it, so this exercises the grid
/// prefix path rather than parsing a hand-built logical line.
#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn codex_narrow_repaint_journals_only_the_closed_intent_and_title() {
    let config = tempfile::tempdir().expect("config directory");
    let _config_guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let project = tempfile::tempdir().expect("registered project");
    crate::config::replace_repositories_for_test(serde_json::json!({
        "repos": { project.path().to_string_lossy(): {} }
    }))
    .expect("register project for ownership");

    let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
    crate::repo_watcher::start_watching(project.path().to_str().expect("UTF-8 path"), &state)
        .expect("register project watcher");
    let sid = "codex-narrow-streaming-intent";
    crate::state::tests_support::insert_dummy_session(&state, sid);
    crate::state::tests_support::set_session_cwd(
        &state,
        sid,
        project.path().to_str().expect("UTF-8 path"),
    );
    agent_session(&state, sid, SHELL_BUSY);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .expect("agent session")
        .agent_type = Some("codex".to_string());
    state.grid.vt_log_buffers.insert(
        sid.to_string(),
        Mutex::new(crate::state::VtLogBuffer::new(16, 20, 2000)),
    );
    let silence = state
        .session_maps
        .silence_states
        .get(sid)
        .expect("agent silence state")
        .clone();
    let mut parsed_events = state.event_bus.subscribe();
    let mut processor = ChunkProcessor::new(None, None);
    let prefixes = [
        "Individuo",
        "Individuo la regressione dell'a capo nel terminale mobile e ripristino",
        "Individuo la regressione dell'a capo nel terminale mobile e ripristino il comportamento corretto (Accapo mobile)",
    ];

    processor.process_chunk("\x1b[?1049h", &silence, sid, &state);
    for prefix in prefixes {
        processor.process_chunk(
            &format!("\x1b[4;1H\x1b[2K• intent: {prefix}"),
            &silence,
            sid,
            &state,
        );
    }

    let intents: Vec<_> = std::iter::from_fn(|| parsed_events.try_recv().ok())
        .filter_map(|event| match event {
            crate::state::AppEvent::PtyParsed { parsed, .. }
                if parsed.get("type").and_then(serde_json::Value::as_str) == Some("intent") =>
            {
                Some((
                    parsed
                        .get("text")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    parsed
                        .get("title")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                ))
            }
            _ => None,
        })
        .collect();
    let text = prefixes
        .last()
        .expect("complete prefix")
        .split(" (")
        .next()
        .unwrap();
    assert_eq!(
        intents,
        vec![(text.to_string(), Some("Accapo mobile".to_string()))]
    );

    let entries = crate::progress::ProgressStore::open()
        .expect("open Progress journal")
        .list(
            &project
                .path()
                .canonicalize()
                .expect("canonical project")
                .to_string_lossy(),
            &crate::progress::ProgressListInput::default(),
        )
        .expect("list Progress journal")
        .entries;
    assert!(matches!(entries.as_slice(), [entry]
        if entry.kind == crate::progress::ProgressKind::Intent && entry.text == text));
}

/// A live Codex 80-column capture. Its marker grows across an indented second
/// row while the first row repaints; only the completed title is a journal row.
#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn captured_codex_narrow_hard_wrap_journals_complete_intent() {
    replay_captured_codex_narrow_intent(
        "codex-narrow-progress-intent-20260925.tcap",
        "Controllo la cattura a 80 colonne e preparo la prova del journal con righe di continuazione e titolo finale",
        "Verifica stretta",
    );
}

/// A second real narrow capture exercises Ink's continuation-row repaint after
/// the first fixture exposed an early partial publication.
#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn captured_codex_narrow_ink_replay_journals_complete_intent() {
    replay_captured_codex_narrow_intent(
        "codex-narrow-ink-replay-20260925.tcap",
        "Verifico il secondo replay Codex a larghezza stretta dopo la correzione dei frame Ink e degli intent ancora aperti",
        "Replay finale",
    );
}

#[cfg(unix)]
fn replay_captured_codex_narrow_intent(name: &str, text: &str, title: &str) {
    let capture = crate::pty_capture::decode_capture(&agent_prompt_fixture(name))
        .expect("decode Codex capture");
    let (rows, cols) = capture.geometry.expect("capture geometry");
    assert_eq!(cols, 80);
    let config = tempfile::tempdir().expect("config directory");
    let _config_guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let project = tempfile::tempdir().expect("registered project");
    crate::config::replace_repositories_for_test(serde_json::json!({
        "repos": { project.path().to_string_lossy(): {} }
    }))
    .expect("register project for ownership");
    let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
    crate::repo_watcher::start_watching(project.path().to_str().expect("UTF-8 path"), &state)
        .expect("register project watcher");
    let sid = "captured-codex-narrow-hard-wrap";
    crate::state::tests_support::insert_dummy_session(&state, sid);
    crate::state::tests_support::set_session_cwd(
        &state,
        sid,
        project.path().to_str().expect("UTF-8 path"),
    );
    agent_session(&state, sid, SHELL_BUSY);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .expect("agent session")
        .agent_type = Some("codex".to_string());
    state.grid.vt_log_buffers.insert(
        sid.to_string(),
        Mutex::new(crate::state::VtLogBuffer::new(rows, cols, 2000)),
    );
    let silence = state
        .session_maps
        .silence_states
        .get(sid)
        .expect("silence state")
        .clone();
    let mut parsed_events = state.event_bus.subscribe();
    let mut processor = ChunkProcessor::new(None, None);
    let mut intents = Vec::new();
    for record in capture.records {
        if record.direction == crate::pty_capture::CaptureDirection::Output {
            processor.process_chunk(
                std::str::from_utf8(&record.data).expect("complete UTF-8 chunk"),
                &silence,
                sid,
                &state,
            );
        }
        for event in std::iter::from_fn(|| parsed_events.try_recv().ok()) {
            if let crate::state::AppEvent::PtyParsed { parsed, .. } = event
                && parsed.get("type").and_then(serde_json::Value::as_str) == Some("intent")
            {
                intents.push((
                    parsed["text"].as_str().unwrap_or_default().to_string(),
                    parsed["title"].as_str().map(str::to_string),
                ));
            }
        }
    }
    assert_eq!(intents, [(text.to_string(), Some(title.to_string()))]);
    let entries = crate::progress::ProgressStore::open()
        .expect("open Progress journal")
        .list(
            &project
                .path()
                .canonicalize()
                .expect("canonical project")
                .to_string_lossy(),
            &crate::progress::ProgressListInput::default(),
        )
        .expect("list Progress journal")
        .entries;
    assert!(matches!(entries.as_slice(), [entry]
        if entry.kind == crate::progress::ProgressKind::Intent && entry.text == text));
}

#[cfg(unix)]
#[allow(clippy::too_many_arguments)]
fn run_progress_intent_case_grid(
    chunks: &[&str],
    cols: u16,
    agent: bool,
    timer_idle: bool,
    teardown: Option<&str>,
    idle_is_quiet: bool,
    fail_journal: bool,
    history_capacity: usize,
    alt_screen: bool,
) -> (Vec<(String, Option<String>)>, Vec<String>) {
    #[cfg(not(feature = "desktop"))]
    let runtime = tokio::runtime::Handle::try_current().is_err().then(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test Tokio runtime")
    });
    #[cfg(not(feature = "desktop"))]
    let _runtime_guard = runtime.as_ref().map(|runtime| runtime.enter());
    let config = tempfile::tempdir().expect("config directory");
    let _config_guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let project = tempfile::tempdir().expect("registered project");
    crate::config::replace_repositories_for_test(serde_json::json!({
        "repos": { project.path().to_string_lossy(): {} }
    }))
    .expect("register project for ownership");
    let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
    crate::repo_watcher::start_watching(project.path().to_str().expect("UTF-8 path"), &state)
        .expect("register project watcher");
    if fail_journal {
        std::fs::create_dir(config.path().join("progress.sqlite3"))
            .expect("block journal database path");
    }
    let sid = "progress-intent-matrix";
    crate::state::tests_support::insert_dummy_session(&state, sid);
    crate::state::tests_support::set_session_cwd(
        &state,
        sid,
        project.path().to_str().expect("UTF-8 path"),
    );
    agent_session(&state, sid, SHELL_BUSY);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .expect("session")
        .agent_type = agent.then(|| "codex".to_string());
    state.grid.vt_log_buffers.insert(
        sid.to_string(),
        Mutex::new(crate::state::VtLogBuffer::new(16, cols, history_capacity)),
    );
    let silence = state
        .session_maps
        .silence_states
        .get(sid)
        .expect("silence state")
        .clone();
    let mut parsed_events = state.event_bus.subscribe();
    let mut processor = ChunkProcessor::new(None, None);
    if alt_screen {
        processor.process_chunk("\x1b[?1049h", &silence, sid, &state);
    }
    let mut capped_intent_start = None;
    for (index, chunk) in chunks.iter().enumerate() {
        processor.process_chunk(chunk, &silence, sid, &state);
        if !alt_screen && history_capacity == 20 && index == 1 {
            capped_intent_start = Some(
                silence
                    .lock()
                    .open_intent
                    .as_ref()
                    .expect("the capped-scroll fixture opened an intent")
                    .start_row,
            );
        }
        if !alt_screen && history_capacity == 20 && index == 2 {
            assert_eq!(
                silence
                    .lock()
                    .open_intent
                    .as_ref()
                    .expect("scroll must retain the open intent")
                    .start_row,
                capped_intent_start.expect("captured the anchor before scrolling"),
                "the capped history origin must not move the open intent anchor"
            );
        }
    }
    if !alt_screen && history_capacity == 20 {
        let vt = state.grid.vt_log_buffers.get(sid).expect("terminal grid");
        let vt = vt.lock();
        assert!(
            vt.grid_screen_origin() > vt.grid_history_size(),
            "the main-screen test must scroll beyond the actual grid history cap"
        );
    }
    if timer_idle {
        if idle_is_quiet {
            silence.lock().last_output_at = std::time::Instant::now()
                - SILENCE_INTENT_THRESHOLD
                - std::time::Duration::from_millis(1);
        }
        state
            .session_maps
            .shell_states
            .get(sid)
            .expect("shell state")
            .store(SHELL_IDLE, std::sync::atomic::Ordering::Release);
        emit_open_intent_if_idle(&state, &silence, sid);
        emit_open_intent_if_idle(&state, &silence, sid);
    }
    match teardown {
        Some("close") => {
            close_pty_core(&state, sid, false);
            assert!(!state.session_maps.sessions.contains_key(sid));
        }
        Some("kill") => assert!(kill_pty_core(&state, sid)),
        Some("exit") => mark_session_exited(sid, &state),
        None => {}
        Some(other) => panic!("unknown teardown {other}"),
    }
    let events = std::iter::from_fn(|| parsed_events.try_recv().ok())
        .filter_map(|event| match event {
            crate::state::AppEvent::PtyParsed { parsed, .. }
                if parsed.get("type").and_then(serde_json::Value::as_str) == Some("intent") =>
            {
                Some((
                    parsed["text"].as_str().unwrap_or_default().to_string(),
                    parsed["title"].as_str().map(str::to_string),
                ))
            }
            _ => None,
        })
        .collect();
    let entries = if fail_journal {
        Vec::new()
    } else {
        crate::progress::ProgressStore::open()
            .expect("open Progress journal")
            .list(
                &project
                    .path()
                    .canonicalize()
                    .expect("canonical project")
                    .to_string_lossy(),
                &crate::progress::ProgressListInput::default(),
            )
            .expect("list Progress journal")
            .entries
            .into_iter()
            .filter(|entry| entry.kind == crate::progress::ProgressKind::Intent)
            .map(|entry| entry.text)
            .collect()
    };
    (events, entries)
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn progress_open_intent_close_matrix() {
    for ending in ["close", "kill", "exit"] {
        let (events, entries) = run_progress_intent_case_ending(
            &["\x1b[4;1H\x1b[2K• intent: Preserve the final journal entry"],
            80,
            true,
            false,
            Some(ending),
            true,
            false,
        );
        assert_eq!(
            events,
            [("Preserve the final journal entry".into(), None)],
            "{ending}"
        );
        assert_eq!(entries, ["Preserve the final journal entry"], "{ending}");
    }

    let (events, entries) = run_progress_intent_case(
        &[
            "\x1b[4;1H\x1b[2K● intent: Reviewing the streaming parser for the Progress journal\r\n  correctness guarantees (Review parser)",
        ],
        72,
        true,
        false,
    );
    assert_eq!(
        events,
        [(
            "Reviewing the streaming parser for the Progress journal correctness guarantees".into(),
            Some("Review parser".into())
        )]
    );
    assert_eq!(
        entries,
        ["Reviewing the streaming parser for the Progress journal correctness guarantees"]
    );

    let (events, entries) = run_progress_intent_case(
        &[
            "\x1b[4;1H\x1b[2K● intent: Reviewing the narrow terminal\r\n  and the Progress journal carefully\r\n  together (Narrow replay)",
        ],
        40,
        true,
        false,
    );
    assert_eq!(
        events,
        [(
            "Reviewing the narrow terminal and the Progress journal carefully together".into(),
            Some("Narrow replay".into())
        )]
    );
    assert_eq!(
        entries,
        ["Reviewing the narrow terminal and the Progress journal carefully together"]
    );

    let (events, entries) = run_progress_intent_case(
        &[
            "\x1b[4;1H\x1b[2K● intent: Inspect\n\x1b[5;1H\x1b[2K⠋ Working",
            "\x1b[5;1H\x1b[2K\x1b[4;1H\x1b[2K● intent: Inspect the auth\n\x1b[5;1H\x1b[2K⠋ Working",
            "\x1b[5;1H\x1b[2K\x1b[4;1H\x1b[2K● intent: Inspect the auth path (Auth path)\n\x1b[5;1H\x1b[2K⠋ Working",
        ],
        80,
        true,
        false,
    );
    assert_eq!(
        events,
        [("Inspect the auth path".into(), Some("Auth path".into()))]
    );
    assert_eq!(entries, ["Inspect the auth path"]);

    for chunks in [
        vec!["\x1b[4;1H\x1b[2K• intent: Read the configuration loader\x1b[14;3H"],
        vec!["\x1b[4;1H\x1b[2K• intent: Read the configuration loader\r"],
        vec!["\x1b[4;1H\x1b[2K• intent: Read the configuration loader"],
    ] {
        let (events, entries) = run_progress_intent_case(&chunks, 128, true, false);
        assert!(events.is_empty(), "cursor move or CR closed an open intent");
        assert!(entries.is_empty());
    }

    let (events, entries) = run_progress_intent_case(
        &["\x1b[4;1H\x1b[2K• intent: Restore terminal wrapping across narrow panes"],
        20,
        true,
        false,
    );
    assert!(events.is_empty(), "soft wrap closed an open intent");
    assert!(entries.is_empty());

    let (events, entries) = run_progress_intent_case(
        &[
            "\x1b[4;1H\x1b[2K• intent: Read",
            "\x1b[4;1H\x1b[2K• intent: Read the configuration loader\x1b[14;3H",
            "\x1b[5;1HChecking the settings now",
        ],
        128,
        true,
        false,
    );
    assert_eq!(events, [("Read the configuration loader".into(), None)]);
    assert_eq!(entries, ["Read the configuration loader"]);

    let (events, entries) = run_progress_intent_case(
        &["\x1b[4;1H\x1b[2K• intent: Inspect the startup path\x1b[14;3H"],
        128,
        true,
        true,
    );
    assert_eq!(events, [("Inspect the startup path".into(), None)]);
    assert_eq!(entries, ["Inspect the startup path"]);

    let (events, entries) = run_progress_intent_case_ending(
        &["\x1b[4;1H\x1b[2K• intent: Still streaming"],
        80,
        true,
        true,
        None,
        false,
        false,
    );
    assert!(events.is_empty());
    assert!(entries.is_empty());

    let (events, entries) = run_progress_intent_case_ending(
        &["\x1b[4;1H\x1b[2K• intent: Inspect a closed store (Store error)"],
        80,
        true,
        false,
        None,
        true,
        true,
    );
    assert!(
        events.is_empty(),
        "failed journal write must not publish an intent"
    );
    assert!(entries.is_empty());

    let secret = format!("ghp_{}", "A".repeat(40));
    let chunk = format!("\x1b[4;1H\x1b[2K• intent: Inspect {secret} (Secret check)");
    let (_, entries) = run_progress_intent_case(&[&chunk], 128, true, false);
    assert_eq!(entries.len(), 1);
    assert!(
        !entries[0].contains(&secret),
        "the journal persisted a secret"
    );

    let (events, entries) = run_progress_intent_case(
        &[
            "\x1b[4;1H\x1b[2K• intent: Inspect the session journal",
            "\r",
            "\n",
        ],
        128,
        true,
        false,
    );
    assert_eq!(events, [("Inspect the session journal".into(), None)]);
    assert_eq!(entries, ["Inspect the session journal"]);

    let (events, entries) = run_progress_intent_case(
        &[
            "\x1b[4;1H\x1b[2K• intent: Check the completion signal\x1b[14;3H",
            "\x1b]7770;state=idle\x07",
        ],
        128,
        true,
        false,
    );
    assert_eq!(events, [("Check the completion signal".into(), None)]);
    assert_eq!(entries, ["Check the completion signal"]);

    let (events, entries) = run_progress_intent_case(
        &[
            "\x1b[4;1H\x1b[2K• intent: Restore terminal wrapping across narrow panes",
            "\x1b[8;1HFollowing prose closes the wrapped line",
        ],
        20,
        true,
        false,
    );
    assert_eq!(
        events,
        [("Restore terminal wrapping across narrow panes".into(), None)]
    );
    assert_eq!(entries, ["Restore terminal wrapping across narrow panes"]);

    let (events, entries) = run_progress_intent_case(
        &[
            "\x1b[4;1H\x1b[2K• intent: Inspect auth (Auth)",
            "\x1b[4;1H\x1b[2K• intent: Inspect routing (Routing)",
            "\x1b[4;1H\x1b[2K• intent: Inspect auth (Auth)",
            "\x1b[4;1H\x1b[2K• intent: Inspect auth (Auth)",
        ],
        128,
        true,
        false,
    );
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].0, "Inspect auth");
    assert_eq!(events[1].0, "Inspect routing");
    assert_eq!(events[2].0, "Inspect auth");
    assert_eq!(entries.len(), 3);

    let (events, entries) = run_progress_intent_case(
        &[
            "\x1b[4;1H\x1b[2K• intent: Inspect auth (Auth)",
            "\x1b[4;1H\x1b[2K",
            "\x1b[4;1H\x1b[2K• intent: Inspect auth (Auth)",
        ],
        128,
        true,
        false,
    );
    assert_eq!(events.len(), 1);
    assert_eq!(entries.len(), 1);

    let (events, entries) = run_progress_intent_case(
        &["\x1b[4;1H\x1b[2K• intent: Shell output (Ignored)"],
        128,
        false,
        false,
    );
    assert!(events.is_empty());
    assert!(entries.is_empty());
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn indented_prose_after_a_short_intent_is_not_a_wrap() {
    let (events, entries) = run_progress_intent_case(
        &[
            "\x1b[4;1H\x1b[2K• intent: Inspect the auth path\r\n  Reading src/auth.rs (the entry point)",
        ],
        128,
        true,
        false,
    );
    assert_eq!(events, [("Inspect the auth path".into(), None)]);
    assert_eq!(entries, ["Inspect the auth path"]);
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn indented_prose_after_a_soft_wrapped_intent_is_not_a_hard_wrap() {
    let (events, entries) = run_progress_intent_case(
        &[
            "\x1b[4;1H\x1b[2K• intent: Inspect the authentication path carefully\r\n  Reading src/auth.rs (the entry point)",
        ],
        40,
        true,
        false,
    );
    assert_eq!(
        events,
        [("Inspect the authentication path carefully".into(), None)]
    );
    assert_eq!(entries, ["Inspect the authentication path carefully"]);
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn capped_main_screen_origin_closes_a_titleless_intent() {
    let filler = "filler\r\n".repeat(10_040);
    let (events, entries) = run_progress_intent_case_grid(
        &[
            &filler,
            "\x1b[12;1H\x1b[2K• intent: Inspect capped scrollback",
            "\x1b[S",
            // The prose is only one row below the scrolled anchor. A capped
            // history_size origin leaves the old row number in place.
            "\x1b[12;1H\x1b[2KFollowing prose after the scroll",
        ],
        80,
        true,
        false,
        None,
        true,
        false,
        20,
        false,
    );
    assert_eq!(events, [("Inspect capped scrollback".into(), None)]);
    assert_eq!(entries, ["Inspect capped scrollback"]);
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn narrow_intent_absorbs_three_hard_wrap_rows() {
    let (events, entries) = run_progress_intent_case(
        &[
            "\x1b[4;1H\x1b[2K● intent: Review narrow rows and\r\n  preserve every continuation while\r\n  collecting the complete title and\r\n  journal text (Narrow complete)",
        ],
        40,
        true,
        false,
    );
    let full = "Review narrow rows and preserve every continuation while collecting the complete title and journal text";
    assert_eq!(events, [(full.into(), Some("Narrow complete".into()))]);
    assert_eq!(entries, [full]);
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn incomplete_narrow_title_is_not_discarded_by_the_next_intent() {
    let (events, entries) = run_progress_intent_case(
        &[
            "\x1b[4;1H\x1b[2K● intent: Review narrow rows and\r\n  preserve every continuation while\r\n  collecting the complete title and (",
            "\r\n  Narrow complete)",
            "\r\n• intent: Begin the next step (Next)",
        ],
        40,
        true,
        false,
    );
    assert_eq!(
        events,
        [
            (
                "Review narrow rows and preserve every continuation while collecting the complete title and".into(),
                Some("Narrow complete".into())
            ),
            ("Begin the next step".into(), Some("Next".into()))
        ]
    );
    assert_eq!(
        entries,
        [
            "Begin the next step",
            "Review narrow rows and preserve every continuation while collecting the complete title and"
        ]
    );
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn long_intent_keeps_its_title_and_truncates_only_the_journal() {
    let full = format!("{} {}", "a".repeat(380), "b".repeat(220));
    let chunk = format!(
        "\x1b[4;1H\x1b[2K• intent: {}\r\n  {} (Long)",
        "a".repeat(380),
        "b".repeat(220)
    );
    let (events, entries) = run_progress_intent_case(&[&chunk], 400, true, false);
    assert_eq!(events, [(full.clone(), Some("Long".into()))]);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].chars().count(), crate::progress::MAX_TEXT_CHARS);
    assert!(entries[0].ends_with('…'));
    assert!(full.starts_with(entries[0].trim_end_matches('…')));
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn ordinary_agent_repaint_avoids_intent_grid_scans() {
    let mut frame = String::new();
    for row in 1..=16 {
        frame.push_str(&format!("\x1b[{row};1H\x1b[2KFrame row {row}"));
    }
    INTENT_CANDIDATE_GRID_READS.with(|reads| reads.set(0));
    let (events, entries) = run_progress_intent_case(&[&frame], 80, true, false);
    assert!(events.is_empty());
    assert!(entries.is_empty());
    INTENT_CANDIDATE_GRID_READS.with(|reads| {
        assert!(
            reads.get() <= 2,
            "{} logical grid scans for a non-intent repaint",
            reads.get()
        );
    });
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn wide_soft_wrapped_intent_keeps_complete_title_and_journal() {
    let text = "review ".repeat(76).trim_end().to_string();
    let chunk = format!("\x1b[4;1H\x1b[2K• intent: {text} (Wide review)");
    let (events, entries) = run_progress_intent_case(&[&chunk], 120, true, false);
    assert_eq!(events, [(text.clone(), Some("Wide review".into()))]);
    assert_eq!(entries.len(), 1);
    assert!(text.starts_with(entries[0].trim_end_matches('…')));
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn twelve_row_soft_wrapped_intent_keeps_progress_and_title() {
    let text = "review ".repeat(68).trim_end().to_string();
    let chunk = format!("\x1b[2;1H\x1b[2K• intent: {text} (Twelve rows)");
    let (events, entries) = run_progress_intent_case(&[&chunk], 40, true, false);
    assert_eq!(events, [(text.clone(), Some("Twelve rows".into()))]);
    assert_eq!(entries.len(), 1);
    assert!(text.starts_with(entries[0].trim_end_matches('…')));
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn intent_continuation_does_not_read_below_chrome_cutoff() {
    let chunk = "\x1b[4;1H\x1b[2K• intent: Inspect the module\x1b[5;1H\x1b[2K────────────────────────────────────────\x1b[6;1H\x1b[2K❯ ";
    INTENT_CONTINUATION_GRID_READS.with(|reads| reads.set(0));
    let (events, _) = run_progress_intent_case(&[chunk], 40, true, true);
    assert_eq!(events, [("Inspect the module".into(), None)]);
    INTENT_CONTINUATION_GRID_READS.with(|reads| {
        assert_eq!(
            reads.get(),
            0,
            "chrome rows must not be read as continuations"
        );
    });
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn unclosed_title_does_not_silently_discard_prior_intent() {
    let (events, entries) = run_progress_intent_case(
        &[
            "\x1b[4;1H\x1b[2K• intent: Inspect auth (",
            "\x1b[5;1H\x1b[2K• intent: Review routing (Routing)",
        ],
        80,
        true,
        false,
    );
    assert_eq!(
        events,
        [
            ("Inspect auth".into(), None),
            ("Review routing".into(), Some("Routing".into()))
        ]
    );
    assert_eq!(entries, ["Review routing", "Inspect auth"]);
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn repaint_below_open_intent_reads_each_continuation_once() {
    let mut frame = String::from("\x1b[1;1H\x1b[2K• intent: Inspect current state");
    for row in 2..=16 {
        frame.push_str(&format!("\x1b[{row};1H\x1b[2KFrame row {row}"));
    }
    INTENT_CONTINUATION_GRID_READS.with(|reads| reads.set(0));
    let _ = run_progress_intent_case(&[&frame], 80, true, false);
    INTENT_CONTINUATION_GRID_READS.with(|reads| {
        assert!(reads.get() <= 2, "{} continuation grid reads", reads.get());
    });

    let completed = frame.replace("Inspect current state", "Inspect current state (Current)");
    INTENT_CONTINUATION_GRID_READS.with(|reads| reads.set(0));
    let (events, _) = run_progress_intent_case(&[&completed], 80, true, false);
    assert_eq!(
        events,
        [("Inspect current state".into(), Some("Current".into()))]
    );
    INTENT_CONTINUATION_GRID_READS.with(|reads| {
        assert_eq!(reads.get(), 0, "closed title scanned continuation rows");
    });
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn silence_timer_drains_open_intent_after_idle() {
    tokio::time::pause();
    let config = tempfile::tempdir().expect("config directory");
    let _config_guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let project = tempfile::tempdir().expect("registered project");
    crate::config::replace_repositories_for_test(serde_json::json!({
        "repos": { project.path().to_string_lossy(): {} }
    }))
    .expect("register project for ownership");
    let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
    crate::repo_watcher::start_watching(project.path().to_str().expect("UTF-8 path"), &state)
        .expect("register project watcher");
    let sid = "progress-intent-timer";
    crate::state::tests_support::insert_dummy_session(&state, sid);
    crate::state::tests_support::set_session_cwd(
        &state,
        sid,
        project.path().to_str().expect("UTF-8 path"),
    );
    agent_session(&state, sid, SHELL_BUSY);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .agent_type = Some("codex".into());
    state.grid.vt_log_buffers.insert(
        sid.into(),
        Mutex::new(crate::state::VtLogBuffer::new(16, 80, 2000)),
    );
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
    let mut parsed_events = state.event_bus.subscribe();
    ChunkProcessor::new(None, None).process_chunk(
        "\x1b[4;1H\x1b[2K• intent: Review idle drain",
        &silence,
        sid,
        &state,
    );
    silence.lock().last_output_at =
        std::time::Instant::now() - STARTUP_SETTLE_SILENCE - std::time::Duration::from_secs(1);
    state
        .session_maps
        .shell_states
        .get(sid)
        .unwrap()
        .store(SHELL_IDLE, Ordering::Release);
    let running = std::sync::Arc::new(AtomicBool::new(true));
    spawn_silence_timer(silence, running.clone(), sid.into(), state);
    tokio::task::yield_now().await;
    tokio::time::advance(SILENCE_CHECK_INTERVAL + std::time::Duration::from_millis(1)).await;
    tokio::task::yield_now().await;
    running.store(false, Ordering::Release);
    let intents: Vec<_> = std::iter::from_fn(|| parsed_events.try_recv().ok())
        .filter_map(|event| match event {
            crate::state::AppEvent::PtyParsed { parsed, .. }
                if parsed.get("type").and_then(serde_json::Value::as_str) == Some("intent") =>
            {
                parsed["text"].as_str().map(str::to_string)
            }
            _ => None,
        })
        .collect();
    assert_eq!(intents, ["Review idle drain"]);
}

/// Live idle Codex animation from brainstorming (2026-09-21). The capture
/// starts after turn completion: seed that observed protocol boundary, then
/// replay the original repaint chunks through the production reader.
#[test]
fn protocol_idle_survives_captured_codex_animation() {
    use std::sync::atomic::Ordering;

    let capture = crate::pty_capture::decode_capture(&agent_prompt_fixture(
        "codex-idle-animation-20260921.tcap",
    ))
    .unwrap();
    assert!(capture.records.len() > 1);
    let (rows, cols) = capture.geometry.unwrap();
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "idle-codex-animation";
    agent_session(&state, sid, SHELL_BUSY);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .agent_type = Some("codex".into());
    state.grid.vt_log_buffers.insert(
        sid.into(),
        Mutex::new(crate::state::VtLogBuffer::new(rows, cols, 2000)),
    );
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
    let mut processor = ChunkProcessor::new(None, None);
    processor.process_chunk("\x1b]7770;state=idle\x07", &silence, sid, &state);
    for (index, record) in capture.records.iter().enumerate() {
        assert_eq!(
            record.direction,
            crate::pty_capture::CaptureDirection::Output
        );
        processor.process_chunk(
            std::str::from_utf8(&record.data)
                .expect("captured animation has complete UTF-8 chunks"),
            &silence,
            sid,
            &state,
        );
        assert_eq!(
            state
                .session_maps
                .shell_states
                .get(sid)
                .unwrap()
                .load(Ordering::Acquire),
            SHELL_IDLE,
            "idle animation reopened the turn at captured chunk {index}"
        );
        assert!(
            silence.lock().explicit_idle(),
            "chunk {index} erased completion"
        );
    }
    // Preserving completion must not latch idle across the next submitted turn.
    note_submitted_input(&state, sid);
    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(sid)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_BUSY
    );
    assert!(silence.lock().turn_started_by_input());
    assert!(!silence.lock().explicit_idle());
}

#[test]
fn historical_scenario_matrix_is_well_formed_and_fixture_backed() {
    let bytes = agent_prompt_fixture("scenario-matrix.json");
    let matrix: serde_json::Value = serde_json::from_slice(&bytes).expect("valid matrix JSON");
    let scenarios = matrix["scenarios"].as_array().expect("scenario array");
    assert!(
        scenarios
            .iter()
            .any(|scenario| scenario["agent"] == "claude")
    );
    assert!(
        scenarios
            .iter()
            .any(|scenario| scenario["agent"] == "codex")
    );

    let mut ids = std::collections::HashSet::new();
    for scenario in scenarios {
        let id = scenario["id"].as_str().expect("scenario id");
        assert!(ids.insert(id), "duplicate scenario id: {id}");
        let set = scenario["set"].as_str().expect("SET expectation");
        let clears = scenario["clear"].as_array().expect("CLEAR expectations");
        assert!(
            set == "none" || !clears.is_empty(),
            "every state SET needs at least one CLEAR path: {id}"
        );
        if scenario["source"] == "raw-fixture" {
            let fixture = scenario["fixture"].as_str().expect("raw fixture name");
            assert!(
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("src/fixtures/agent_prompts")
                    .join(fixture)
                    .is_file(),
                "matrix references missing fixture: {fixture}"
            );
        }
    }
}

/// The screen a capture leaves behind, rendered through the same VT the PTY hot
/// path uses. A screen-activity adapter is a function of the rendered grid, not
/// of the byte stream, so replaying to the grid is the only way a fixture can
/// prove one: goose repaints its footer in place with `\r\x1b[2K`, and asserting
/// on raw bytes would pass on output no terminal would ever display.
fn replay_final_screen(bytes: &[u8]) -> Vec<String> {
    let capture = crate::pty_capture::decode_capture(bytes).expect("valid capture");
    let (rows, cols) = capture.geometry.unwrap_or((41, 128));
    let mut vt_log = crate::state::VtLogBuffer::new(rows, cols, 2000);
    for record in capture.records {
        if record.direction == crate::pty_capture::CaptureDirection::Output {
            vt_log.process(&record.data);
        }
    }
    vt_log.screen_rows()
}

/// Captured from live Codex 0.157.1 (`--no-alt-screen`) and OpenCode 1.18.30
/// (`--mini`) launched through the HTTP agent route. Both kept TUIC's native
/// scrollback through startup and a resize; the Codex capture also spans an
/// approval prompt and its cancellation.
#[test]
fn live_native_scrollback_captures_never_enter_alternate_screen() {
    for fixture in [
        "codex-0.157.1-no-alt-approval-resize.tcap",
        "opencode-1.18.30-mini-resize.tcap",
    ] {
        let capture = crate::pty_capture::decode_capture(&agent_prompt_fixture(fixture))
            .expect("valid live capture");
        let (rows, cols) = capture.geometry.expect("recorded terminal geometry");
        let mut vt = VtLogBuffer::new(rows, cols, 2000);
        let mut output_count = 0;
        for record in capture.records {
            if record.direction != crate::pty_capture::CaptureDirection::Output {
                continue;
            }
            output_count += 1;
            vt.process(&record.data);
            assert!(
                !vt.is_alternate_screen(),
                "{fixture}: alternate screen entered at {} us",
                record.elapsed_us
            );
        }
        assert!(output_count > 0, "{fixture}: capture has no agent output");
    }
}

#[test]
fn codex_native_scrollback_capture_keeps_approval_and_idle_composer_visible() {
    let capture = crate::pty_capture::decode_capture(&agent_prompt_fixture(
        "codex-0.157.1-no-alt-approval-resize.tcap",
    ))
    .expect("valid live capture");
    let (rows, cols) = capture.geometry.expect("recorded terminal geometry");
    let mut vt = VtLogBuffer::new(rows, cols, 2000);
    let mut approval_visible = false;
    for record in capture.records {
        if record.direction == crate::pty_capture::CaptureDirection::Output {
            vt.process(&record.data);
            approval_visible |= vt
                .screen_rows()
                .iter()
                .any(|row| row.contains("Would you like to run the following command?"));
        }
    }
    assert!(
        approval_visible,
        "live approval prompt was lost during replay"
    );
    let screen = vt.screen_rows();
    let refs: Vec<_> = screen.iter().map(String::as_str).collect();
    let cutoff = crate::chrome::find_chrome_cutoff(&refs)
        .expect("the final Codex composer must anchor the chrome cutoff");
    assert!(
        screen[..cutoff]
            .iter()
            .any(|row| row.contains("You canceled the request")),
        "cancellation must remain in the transcript above the composer: {screen:#?}"
    );
    assert!(
        screen[cutoff..]
            .iter()
            .any(|row| row.contains("Ask Codex to do anything")),
        "the idle composer must remain visible below the cutoff: {screen:#?}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn codex_canceled_approval_capture_clears_the_waiting_badge() {
    // Exact records 1775..1826 from the committed live capture, with its
    // original geometry and chunk boundaries retained.
    let capture = crate::pty_capture::decode_capture(&agent_prompt_fixture(
        "codex-0.157.1-approval-cancel.tcap",
    ))
    .expect("valid live capture");
    let (rows, cols) = capture.geometry.expect("recorded geometry");
    let sid = "codex-canceled-approval";
    let (state, silence) = chunk_trace_state(sid);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .agent_type = Some("codex".into());
    state
        .grid
        .vt_log_buffers
        .insert(sid.into(), Mutex::new(VtLogBuffer::new(rows, cols, 2000)));
    crate::state::AppState::spawn_session_state_accumulator(state.clone());
    let mut processor = ChunkProcessor::new(None, None);
    let mut utf8 = Utf8ReadBuffer::new();
    let mut escape = EscapeAwareBuffer::new();
    let mut saw_approval = false;
    let mut approval_title = None;
    let mut ordinary_title = None;
    for record in capture.records {
        if record.direction != crate::pty_capture::CaptureDirection::Output {
            continue;
        }
        let data = utf8.push(&record.data);
        let data = escape.push(&data);
        let (clean, _) = crate::state::strip_kitty_sequences(&data);
        if clean.contains("Action Required") && approval_title.is_none() {
            approval_title = Some(clean.to_string());
        }
        if let Some(start) = clean.find("\x1b]0;Create approval marker")
            && ordinary_title.is_none()
        {
            let end = start + clean[start..].find('\x07').expect("complete OSC title") + 1;
            ordinary_title = Some(clean[start..end].to_string());
        }
        processor.process_chunk(&clean, &silence, sid, &state);
        let screen = state
            .grid
            .vt_log_buffers
            .get(sid)
            .unwrap()
            .lock()
            .screen_rows();
        if screen
            .iter()
            .any(|row| row.contains("Would you like to run the following command?"))
        {
            saw_approval = true;
            assert!(
                await_session(&state, sid, |session| session.awaiting_input
                    && session.question_confident)
                .await,
                "the live approval dialog must set the waiting badge"
            );
        }
    }
    assert!(saw_approval, "capture must include the approval dialog");
    assert!(
        await_session(&state, sid, |session| !session.awaiting_input
            && !session.question_confident)
        .await,
        "the canceled dialog and idle composer must clear the waiting badge"
    );
    processor.process_chunk(
        &approval_title.expect("capture must include an approval title"),
        &silence,
        sid,
        &state,
    );
    assert!(
        await_session(&state, sid, |session| session.awaiting_input
            && session.question_confident)
        .await,
        "a later approval must set the badge again"
    );
    let repaint = format!(
        "{}\x1b[1;1H.",
        ordinary_title.expect("capture must include the ordinary title after cancellation")
    );
    processor.process_chunk(&repaint, &silence, sid, &state);
    assert!(
        !await_session(&state, sid, |session| !session.awaiting_input).await,
        "an old cancellation in scrollback must not clear a new approval"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn canceled_approval_does_not_clear_a_different_confident_question() {
    let sid = "approval-followed-by-another-question";
    let state = accumulating_state(sid);
    for prompt in [
        "| Create approval marker | project",
        "Confirm a different operation?",
    ] {
        state.emit_pty_event(crate::state::AppEvent::PtyParsed {
            session_id: sid.into(),
            parsed: serde_json::json!({
                "type": "question",
                "prompt_text": prompt,
                "confident": true,
            })
            .into(),
        });
        assert!(
            await_session(&state, sid, |session| session.question_text.as_deref()
                == Some(prompt))
            .await
        );
    }
    state.emit_pty_event(crate::state::AppEvent::PtyParsed {
        session_id: sid.into(),
        parsed: serde_json::json!({
            "type": "protocol-question-cleared",
            "expected_question_text": "| Create approval marker | project",
        })
        .into(),
    });
    assert!(
        !await_session(&state, sid, |session| !session.awaiting_input).await,
        "cancellation of the earlier approval must preserve the later question"
    );
    assert!(
        state
            .session_maps
            .session_states
            .get(sid)
            .unwrap()
            .question_confident
    );
    state.emit_pty_event(crate::state::AppEvent::PtyParsed {
        session_id: sid.into(),
        parsed: serde_json::json!({
            "type": "protocol-question-cleared",
            "expected_question_text": "Confirm a different operation?",
        })
        .into(),
    });
    assert!(
        await_session(&state, sid, |session| !session.awaiting_input
            && !session.question_confident)
        .await,
        "clearing the current question must still work"
    );
}

#[test]
fn opencode_mini_resize_repaint_does_not_reopen_an_idle_turn() {
    let capture = crate::pty_capture::decode_capture(&agent_prompt_fixture(
        "opencode-1.18.30-mini-resize.tcap",
    ))
    .expect("valid live capture");
    let (rows, cols) = capture.geometry.expect("recorded terminal geometry");
    let sid = "opencode-mini-resize";
    let (state, silence) = chunk_trace_state(sid);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .agent_type = Some("opencode".into());
    state
        .grid
        .vt_log_buffers
        .insert(sid.into(), Mutex::new(VtLogBuffer::new(rows, cols, 2000)));
    let mut processor = ChunkProcessor::new(None, None);
    let mut resize_output = Vec::new();
    for record in capture.records {
        if record.direction != crate::pty_capture::CaptureDirection::Output {
            continue;
        }
        if record.elapsed_us < 10_000_000 {
            processor.process_chunk(
                std::str::from_utf8(&record.data).expect("UTF-8 terminal output"),
                &silence,
                sid,
                &state,
            );
        } else {
            resize_output.push(record);
        }
    }
    assert!(
        !resize_output.is_empty(),
        "fixture must contain SIGWINCH repaint"
    );
    silence.lock().force_idle_unconfirmed();
    state
        .session_maps
        .shell_states
        .get(sid)
        .unwrap()
        .store(SHELL_IDLE, std::sync::atomic::Ordering::Release);
    state
        .grid
        .vt_log_buffers
        .get(sid)
        .unwrap()
        .lock()
        .resize(32, 100);
    silence.lock().on_resize();
    let mut rx = state.event_bus.subscribe();
    for record in resize_output {
        processor.process_chunk(
            std::str::from_utf8(&record.data).expect("UTF-8 terminal output"),
            &silence,
            sid,
            &state,
        );
        assert_eq!(
            state
                .session_maps
                .shell_states
                .get(sid)
                .unwrap()
                .load(std::sync::atomic::Ordering::Acquire),
            SHELL_IDLE,
            "resize-only output must not mark OpenCode busy"
        );
    }
    while let Ok(event) = rx.try_recv() {
        if let crate::state::AppEvent::PtyParsed { parsed, .. } = event {
            assert!(
                parsed["type"] != "shell-state" || parsed["state"] != "busy",
                "resize emitted a BUSY edge: {parsed}"
            );
        }
    }
}

/// OpenCode 1.18.30 `--mini` has no composer frame, so the framed adapter read
/// every screen as Unknown: the queue deferred `idle_unconfirmed` forever and the
/// shell stayed busy after the turn (#1299-3ce1). Replays two live captures, wide
/// and 64 columns (the narrow one drops the `ctrl+p cmd` hint and runs a tool).
/// Catches an adapter that cannot see the turn at all, and one that reads Ready
/// while the turn still runs and would type the queue into it.
#[test]
fn opencode_mini_turn_captures_read_working_then_ready() {
    for fixture in [
        "opencode-1.18.30-mini-turn.tcap",
        "opencode-1.18.30-mini-narrow-tool-turn.tcap",
    ] {
        let capture = crate::pty_capture::decode_capture(&agent_prompt_fixture(fixture))
            .expect("valid live capture");
        let (rows, cols) = capture.geometry.expect("recorded terminal geometry");
        let mut vt = VtLogBuffer::new(rows, cols, 2000);
        let mut submitted = false;
        let mut after_submit = Vec::new();
        for record in capture.records {
            match record.direction {
                crate::pty_capture::CaptureDirection::Input => {
                    submitted |= record.data == b"\r";
                }
                crate::pty_capture::CaptureDirection::Output => {
                    vt.process(&record.data);
                    if submitted {
                        after_submit.push(detect_agent_screen_activity(
                            Some("opencode"),
                            &vt.screen_rows(),
                        ));
                    }
                }
            }
        }
        let first_working = after_submit
            .iter()
            .position(|activity| *activity == AgentScreenActivity::Working)
            .unwrap_or_else(|| panic!("{fixture}: the running turn never read Working"));
        let last_working = after_submit
            .iter()
            .rposition(|activity| *activity == AgentScreenActivity::Working)
            .unwrap();
        assert!(
            !after_submit[..first_working].contains(&AgentScreenActivity::Ready),
            "{fixture}: Ready between the submit and the first Working frame"
        );
        assert!(
            !after_submit[first_working..=last_working].contains(&AgentScreenActivity::Ready),
            "{fixture}: Ready while the turn was still running"
        );
        assert_eq!(
            after_submit.last(),
            Some(&AgentScreenActivity::Ready),
            "{fixture}: the finished turn must read Ready; screen: {:#?}",
            vt.screen_rows()
        );
    }
}

/// The user-visible failure: a command queued on OpenCode while it works never
/// reached the composer, because the finished turn was never recognised as
/// Ready and the busy shell never went idle. Replays the live turn through the
/// production reader path, then lets the silence timer run.
#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn queued_command_drains_after_a_captured_opencode_mini_turn() {
    tokio::time::pause();
    let capture = crate::pty_capture::decode_capture(&agent_prompt_fixture(
        "opencode-1.18.30-mini-turn.tcap",
    ))
    .expect("valid live capture");
    let (rows, cols) = capture.geometry.expect("recorded terminal geometry");
    let sid = "opencode-mini-queue";
    let (state, silence) = chunk_trace_state(sid);
    {
        // The recording writer uses a shell child, not the captured agent.
        // The explicitly seeded identity models a configured launch preset.
        let mut session = state.session_maps.session_states.get_mut(sid).unwrap();
        session.agent_type = Some("opencode".into());
        session.agent_type_from_run_config = true;
    }
    state
        .grid
        .vt_log_buffers
        .insert(sid.into(), Mutex::new(VtLogBuffer::new(rows, cols, 2000)));
    // Captured agent output owns this recording composer; the shell
    // child is only its PTY holder, not a foreground-detection scenario.
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .spawn_root_role = crate::state::SpawnRootRole::DirectProgram;
    let bytes = insert_recording_session(&state, sid);
    // The turn is running when the user queues: the Enter of its own prompt
    // left the shell busy, as observed live (`shell-state` stayed `busy`).
    state
        .session_maps
        .shell_states
        .get(sid)
        .unwrap()
        .store(SHELL_BUSY, std::sync::atomic::Ordering::Release);
    assert_eq!(
        enqueue_user_command(&state, sid, "resume queued work", None)
            .unwrap()
            .queued,
        1
    );

    let mut processor = ChunkProcessor::new(None, None);
    for record in capture.records {
        if record.direction == crate::pty_capture::CaptureDirection::Output {
            processor.process_chunk(
                std::str::from_utf8(&record.data).expect("UTF-8 terminal output"),
                &silence,
                sid,
                &state,
            );
        }
    }
    assert_eq!(
        silence.lock().cached_screen_activity,
        AgentScreenActivity::Ready,
        "the reader must classify the finished turn as Ready"
    );
    // The emulator answers the capture's own terminal queries through the
    // writer during replay; only what is written after that is the queue drain.
    let replayed = bytes.lock().unwrap().len();
    // Production satisfies the foreground probe from a process snapshot that
    // shows nothing left under the agent; the test has no process tree.
    {
        let mut session = state.session_maps.session_states.get_mut(sid).unwrap();
        session.background_probe_satisfied_turn_epoch = Some(session.turn_epoch);
    }
    {
        let mut silence = silence.lock();
        let settled = std::time::Instant::now() - std::time::Duration::from_secs(60);
        silence.last_output_at = settled;
        silence.last_chunk_at = settled;
        silence.screen_ready_pending_since = Some(settled);
    }

    let running = Arc::new(AtomicBool::new(true));
    spawn_silence_timer(silence, running.clone(), sid.into(), state.clone());
    tokio::task::yield_now().await;
    tokio::time::advance(SILENCE_CHECK_INTERVAL + std::time::Duration::from_millis(1)).await;
    tokio::task::yield_now().await;
    running.store(false, Ordering::Release);

    for _ in 0..300 {
        if bytes.lock().unwrap().ends_with(b"\r") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        String::from_utf8(bytes.lock().unwrap()[replayed..].to_vec()).unwrap(),
        "\u{15}resume queued work\r",
        "the Ready screen must release the queue into the composer"
    );
}

/// goose 1.49.0, captured live (#699-c6e0): the composer footer is on screen and
/// nothing is running, so the session must read Ready. Without this the OSC 133
/// busy bit set once by the long-lived `goose session` command survives for the
/// whole process and the tab never leaves "working".
#[test]
fn goose_idle_capture_reads_ready() {
    let screen = replay_final_screen(&agent_prompt_fixture("goose-1.49.0-idle.tcap"));
    assert_eq!(
        detect_agent_screen_activity(Some("goose"), &screen),
        AgentScreenActivity::Ready,
        "screen: {screen:#?}"
    );
}

/// The same session mid-turn. The spinner glyph cycles `◐◓◒` and the message
/// beside it is whimsical, so the assertion rests on the interrupt hint — the
/// one part of that row goose is not free to reword without changing what it
/// offers the user.
#[test]
fn goose_mid_turn_capture_reads_working() {
    let screen = replay_final_screen(&agent_prompt_fixture("goose-1.49.0-mid-turn.tcap"));
    assert_eq!(
        detect_agent_screen_activity(Some("goose"), &screen),
        AgentScreenActivity::Working,
        "screen: {screen:#?}"
    );
}

/// goose 1.49.0, captured live after Ctrl+C mid-turn (#1301-87fd): the composer
/// placeholder becomes `Interrupted, what should goose work on instead?` and the
/// `Enter to send` hint is gone. Catches the bug where the screen read Unknown, so
/// agent_state stayed working forever after an interrupt.
#[test]
fn goose_interrupted_capture_reads_ready() {
    let screen = replay_final_screen(&agent_prompt_fixture("goose-1.49.0-interrupted.tcap"));
    assert_eq!(
        detect_agent_screen_activity(Some("goose"), &screen),
        AgentScreenActivity::Ready,
        "screen: {screen:#?}"
    );
}

/// What a replay saw on the way through, beyond the events it produced.
///
/// The counter that matters is `ticks_without_cutoff`: `find_chrome_cutoff`
/// returning `None` means NO trim, so on that tick every status-line row
/// reached every parser. It is the fail-open branch, it is silent, and the
/// only way to know how often it fires on a real agent is to count it over
/// real bytes.
#[derive(Default)]
struct CaptureStats {
    output_records: usize,
    input_records: usize,
    bytes: usize,
    /// Output ticks whose screen held at least one non-blank row.
    ticks_with_content: usize,
    /// …of those, the ticks where no chrome anchor was found.
    ticks_without_cutoff: usize,
    rows_offered: usize,
    rows_trimmed: usize,
}

/// Replay framed captures using their original PTY read/write boundaries.
/// Legacy `.raw` files decode as one output record; they retain parser value
/// but cannot prove boundary-sensitive or input-state behavior.
fn replay_capture(bytes: &[u8], hook_instrumented: bool) -> Vec<ParsedEvent> {
    replay_capture_measured(bytes, hook_instrumented).0
}

fn replay_capture_measured(
    bytes: &[u8],
    hook_instrumented: bool,
) -> (Vec<ParsedEvent>, CaptureStats) {
    use crate::state::VtLogBuffer;

    let capture = crate::pty_capture::decode_capture(bytes).expect("valid capture");
    let (rows, cols) = capture.geometry.unwrap_or((41, 128));
    let mut vt_log = VtLogBuffer::new(rows, cols, 2000);
    let mut parser = crate::output_parser::OutputParser::new();
    let mut input = crate::input_line_buffer::InputLineBuffer::new();
    let mut carry = String::new();
    let mut events = Vec::new();
    let mut stats = CaptureStats::default();
    for record in capture.records {
        stats.bytes += record.data.len();
        match record.direction {
            crate::pty_capture::CaptureDirection::Output => {
                stats.output_records += 1;
                let mut changed = vt_log.process(&record.data);
                let offered = changed.len();
                let screen = vt_log.screen_rows();
                let refs: Vec<&str> = screen.iter().map(String::as_str).collect();
                let cutoff = crate::chrome::find_chrome_cutoff(&refs);
                if refs.iter().any(|row| !row.trim().is_empty()) {
                    stats.ticks_with_content += 1;
                    if cutoff.is_none() {
                        stats.ticks_without_cutoff += 1;
                    }
                }
                if let Some(cutoff) = cutoff {
                    changed.retain(|row| row.row_index < cutoff);
                }
                stats.rows_offered += offered;
                stats.rows_trimmed += offered - changed.len();
                let data = String::from_utf8_lossy(&record.data);
                raw_stream_events(&mut carry, &data, &mut events);
                events.extend(
                    parser
                        .parse_clean_lines(&changed, true)
                        .into_iter()
                        .filter(|e| !suppress_heuristic_question(hook_instrumented, e)),
                );
            }
            crate::pty_capture::CaptureDirection::Input => {
                stats.input_records += 1;
                if let Ok(text) = std::str::from_utf8(&record.data) {
                    for action in input.feed(text) {
                        match action {
                            crate::input_line_buffer::InputAction::Line(content) => {
                                events.push(ParsedEvent::UserInput { content, line: -1 });
                            }
                            crate::input_line_buffer::InputAction::Interrupt => {
                                events.push(ParsedEvent::UserInput {
                                    content: String::new(),
                                    line: -1,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    (events, stats)
}

/// Replay a real Codex turn at its recorded 63x160 geometry and at the
/// capture's monotonic timestamps. A Ready classification is only known to be
/// false retrospectively, when a later frame restores Codex's Working row.
/// Protocol-ranked submission evidence must keep the turn BUSY throughout
/// that interval, regardless of the adapter's transient verdict.
fn assert_codex_false_ready_capture(name: &str, expected_variant: &str) {
    let bytes = agent_prompt_fixture(name);
    let capture = crate::pty_capture::decode_capture(&bytes).expect("valid capture");
    let geometry = capture.geometry.unwrap_or((63, 160));
    assert_eq!(geometry, (63, 160), "{name}: wrong capture geometry");
    let mut vt = crate::state::VtLogBuffer::new(geometry.0, geometry.1, 2000);
    let mut silence = SilenceState::new();
    silence.note_user_submission(true);
    let mut ready_candidate: Option<(u64, Vec<String>)> = None;
    let mut false_ready_screen = None;
    let mut saw_variant = false;

    for record in capture.records {
        if record.direction != crate::pty_capture::CaptureDirection::Output {
            continue;
        }
        vt.process(&record.data);
        let screen = vt.screen_rows();
        saw_variant |= screen.iter().any(|row| row.contains(expected_variant));
        match detect_agent_screen_activity(Some("codex"), &screen) {
            AgentScreenActivity::Working => {
                if let Some((_, candidate)) = ready_candidate.take() {
                    false_ready_screen = Some(candidate);
                }
                silence.note_working_screen();
            }
            AgentScreenActivity::Ready if false_ready_screen.is_none() => {
                let (first_ready_us, _) =
                    ready_candidate.get_or_insert_with(|| (record.elapsed_us, screen.clone()));
                let stable_for = record.elapsed_us.saturating_sub(*first_ready_us);
                silence.screen_ready_pending_since =
                    Some(std::time::Instant::now() - std::time::Duration::from_micros(stable_for));
                assert!(
                    !silence.note_ready_screen(),
                    "{name}: a Ready screen overrode Protocol busy at {record:?}"
                );
            }
            AgentScreenActivity::Ready | AgentScreenActivity::Unknown => {}
            AgentScreenActivity::Interrupted => {
                panic!("{name}: capture unexpectedly contains an interrupted turn")
            }
        }
        if false_ready_screen.is_none() {
            assert!(silence.explicit_busy(), "{name}: lost BUSY during replay");
        }
    }

    let false_ready_screen = false_ready_screen.unwrap_or_else(|| {
        panic!("{name}: no Ready classification was followed by resumed Working")
    });
    assert!(
        saw_variant,
        "{name}: expected variant {expected_variant:?}; false Ready rows: {false_ready_screen:#?}"
    );
}

/// Measure, for one capture, the longest UNBROKEN run of Ready frames — the
/// same window `screen_ready_pending_since` accumulates in production.
///
/// The geometry is a caller argument because TUICCAP1 carries none and both
/// false-ready fixtures predate TUICCAP2. Read it off the capture: the highest
/// CSI CUP row and the widest horizontal box rule.
fn longest_ready_run_us(bytes: &[u8], rows: u16, cols: u16) -> u64 {
    let capture = crate::pty_capture::decode_capture(bytes).expect("valid capture");
    let mut vt = crate::state::VtLogBuffer::new(rows, cols, 2000);
    let mut ready_since: Option<u64> = None;
    let mut longest = 0u64;

    for record in capture.records {
        if record.direction != crate::pty_capture::CaptureDirection::Output {
            continue;
        }
        vt.process(&record.data);
        match detect_agent_screen_activity(Some("codex"), &vt.screen_rows()) {
            AgentScreenActivity::Ready => {
                let first = *ready_since.get_or_insert(record.elapsed_us);
                longest = longest.max(record.elapsed_us.saturating_sub(first));
            }
            // Anything that is not Ready ends the window, because production
            // does exactly that: `note_unknown_screen` clears
            // `screen_ready_pending_since`, so an Unknown frame in the middle
            // restarts the AGENT_READY_CONFIRM countdown from zero. Measuring
            // Ready-to-Ready across an Unknown gap reports a stability the
            // production code never sees.
            AgentScreenActivity::Working
            | AgentScreenActivity::Unknown
            | AgentScreenActivity::Interrupted => ready_since = None,
        }
    }
    longest
}

/// The two false-ready fixtures only prove anything while their Ready runs
/// still outlast `AGENT_READY_CONFIRM`: that is what made the pre-fix code
/// declare idle mid-turn (measured 2026-09-14 at f9803b00^: 1.552 s and
/// 1.567 s of stable Ready were enough). If a later change to
/// `detect_codex_screen_activity` shortened those runs below the threshold,
/// `codex_0154_false_ready_real_captures_stay_protocol_busy` would keep
/// passing while proving nothing, which is the same "green by absence" trap as
/// a skipped test.
///
/// This is also the measurement the 2026-09-13 audit of this story got wrong.
/// It timed the FIRST Ready transient — 233 ms and 143 ms — and concluded from
/// it that no capture could ever produce the RED. The longest run is 7.6 s and
/// 7.5 s. Pin the number so nobody has to take it on trust again.
#[test]
fn the_false_ready_fixtures_still_hold_ready_long_enough_to_matter() {
    for name in [
        "codex-0.154-mid-turn-false-ready.tcap",
        "codex-0.154-background-terminal-false-ready.tcap",
    ] {
        let longest = longest_ready_run_us(&agent_prompt_fixture(name), 63, 160);
        assert!(
            std::time::Duration::from_micros(longest) >= AGENT_READY_CONFIRM,
            "{name}: longest Ready run is {:.3}s, under AGENT_READY_CONFIRM, \
             so this fixture can no longer reproduce the false idle",
            longest as f64 / 1e6
        );
    }
}

#[test]
fn codex_0154_false_ready_real_captures_stay_protocol_busy() {
    for (fixture, variant) in [
        (
            "codex-0.154-mid-turn-false-ready.tcap",
            "background terminal running",
        ),
        (
            "codex-0.154-background-terminal-false-ready.tcap",
            "Enter to select",
        ),
    ] {
        assert_codex_false_ready_capture(fixture, variant);
    }
}

/// Real Codex 0.154 `codex exec` PTY transcript captured by the #746 runtime
/// proof. The framed fixture preserves the source transcript byte-for-byte:
/// `codex-pty.typescript` SHA-256
/// f935c77442e3c7e1d29f336021cc5c882c9e855bc43cd87e1a141430713a2124.
/// Its notify payload SHA-256 was
/// 5895c3bb3f9223af1409a6ca2c941d7881bd5ce8d92c5031f6470eb0e48fdf14.
#[test]
fn codex_0154_runtime_hook_idle_reaches_the_pty_state_machine() {
    let bytes = agent_prompt_fixture("codex-0.154-runtime-hook-idle.tcap");
    let capture = crate::pty_capture::decode_capture(&bytes).expect("valid framed capture");
    assert_eq!(capture.geometry, None, "typescript did not record geometry");
    assert_eq!(
        capture.records.len(),
        1,
        "transcript is one observed PTY write"
    );
    assert_eq!(capture.records[0].data.len(), 53_289);
    assert!(
        capture.records[0]
            .data
            .ends_with(b"\x1b]7770;state=idle\x1b\\"),
        "runtime transcript lost its terminal OSC 7770 idle marker"
    );

    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "codex-0154-runtime-hook-idle";
    agent_session(&state, session_id, SHELL_BUSY);
    {
        let mut session = state
            .session_maps
            .session_states
            .get_mut(session_id)
            .unwrap();
        session.agent_type = Some("codex".into());
        session.hook_instrumented = true;
    }
    state.grid.vt_log_buffers.insert(
        session_id.to_string(),
        Mutex::new(crate::state::VtLogBuffer::new(24, 80, 2000)),
    );
    let silence = state
        .session_maps
        .silence_states
        .get(session_id)
        .unwrap()
        .clone();
    let mut processor = ChunkProcessor::new(None, None);
    for record in capture.records {
        let chunk = std::str::from_utf8(&record.data).expect("captured Codex PTY is UTF-8");
        processor.process_chunk(chunk, &silence, session_id, &state);
    }

    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(session_id)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_IDLE
    );
    let silence = silence.lock();
    assert!(silence.hook_state_seen);
    assert!(silence.explicit_idle());
}

/// #745-8ff1 AC6(c): a Codex `notify` turn-complete ends in a Protocol-rank
/// idle — and the two halves of that path actually meet.
///
/// Both halves were already covered, separately, and that was the gap.
/// `codex_0154_runtime_hook_idle_reaches_the_pty_state_machine` proves those
/// bytes close the turn, from a real capture. `generated_assets_have_protocol_and_ownership_markers`
/// proves the shipped script mentions the right strings. Neither proves the
/// sequence the script *prints* is the sequence the state machine *accepts*:
/// change the `printf` and both stay green while every Codex turn silently
/// stops closing. So this test takes the literal out of the shipped artifact,
/// decodes it the way `/bin/sh printf` would, and feeds the result to the real
/// `ChunkProcessor` — nothing here restates the escape sequence by hand.
///
/// Running the script is deliberately not how this is done. It resolves its own
/// tty from `$PPID` and writes there, so there is nothing to capture, and a
/// freshly written executable pays a code-signing scan on macOS (AGENTS.md).
#[test]
fn codex_notify_turn_complete_emits_the_idle_bytes_the_state_machine_accepts() {
    let dir = tempfile::TempDir::new().unwrap();
    crate::agent_hook_launch::regenerate_launch_assets(dir.path()).unwrap();
    let script = std::fs::read_to_string(dir.path().join("agent-hooks/codex-notify.sh"))
        .expect("codex notify script must be generated");

    // Exactly one arm may emit, and it must be the turn-complete one: a notify
    // for any other event must not close the turn.
    assert_eq!(
        script.matches("7770;state=idle").count(),
        1,
        "only the agent-turn-complete arm may emit idle"
    );
    let emit_at = script.find("7770;state=idle").unwrap();
    let case_at = script
        .find(r#""type":"agent-turn-complete""#)
        .expect("script must match the agent-turn-complete payload");
    assert!(
        case_at < emit_at,
        "the idle emit must sit inside the agent-turn-complete case arm"
    );

    // Pull the printf literal out of the artifact instead of restating it.
    let start = script
        .find("printf '")
        .expect("script must printf a marker")
        + "printf '".len();
    let end = start
        + script[start..]
            .find('\'')
            .expect("unterminated printf literal");
    let decoded = script[start..end]
        .replace("\\033", "\x1b")
        .replace("\\\\", "\\");

    // That exact byte sequence, through the real chunk path, must close a turn
    // a hook-busy is holding — the thing AC6(c) actually asserts.
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "codex-notify-turn-complete";
    agent_session(&state, session_id, SHELL_BUSY);
    {
        let mut session = state
            .session_maps
            .session_states
            .get_mut(session_id)
            .unwrap();
        session.agent_type = Some("codex".into());
        session.hook_instrumented = true;
    }
    state.grid.vt_log_buffers.insert(
        session_id.to_string(),
        Mutex::new(crate::state::VtLogBuffer::new(24, 80, 2000)),
    );
    let silence = state
        .session_maps
        .silence_states
        .get(session_id)
        .unwrap()
        .clone();
    silence.lock().note_explicit_state(SHELL_BUSY, true);
    assert!(
        silence.lock().hook_busy(),
        "precondition: a Protocol-rank hook busy is holding the turn"
    );

    let mut processor = ChunkProcessor::new(None, None);
    processor.process_chunk(&decoded, &silence, session_id, &state);

    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(session_id)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_IDLE,
        "the notify script's own bytes must drive the session idle"
    );
    let silence = silence.lock();
    assert!(
        silence.explicit_idle(),
        "idle must be Protocol rank, not screen"
    );
    assert!(!silence.hook_busy());
}

/// An agent quoting an Ink dialog footer inside its own output, captured off a
/// live PTY on 2026-08-30. The line is byte-identical to the one a real
/// `AskUserQuestion` draws; only the indentation of the agent's frame differs.
///
/// This ran through the pipeline and set `question_confident`, which no clear
/// path retracts — the tab reported the agent as blocked on the user while it
/// was working, for the rest of the turn. `hook_instrumented` is false on
/// purpose: that is the state of a Claude session that has not yet raised an
/// `AskUserQuestion`, so `suppress_heuristic_question` was not covering it.
#[test]
fn quoted_ink_footer_in_agent_output_raises_no_question() {
    let events = replay_capture(
        &agent_prompt_fixture("claude-quoted-ink-footer.tcap"),
        false,
    );
    let questions: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, ParsedEvent::Question { .. }))
        .collect();
    assert!(
        questions.is_empty(),
        "an echoed footer is not a dialog, got: {questions:?}"
    );
}

/// grok in `screen_mode = "minimal"` — the mode it is actually run in —
/// replayed byte-for-byte off a live 1.0.5 session (40x120, one full turn:
/// prompt → thinking → answer → ready).
///
/// This is the whole grok chain in one assertion, because every link of it
/// has failed independently:
///   1. the foreground binary reports as `grok-1.0.5` (a resolved symlink),
///      and an exact-match table answers `None` — no `agent_type`, so
///      `session_is_agent` is false and NOTHING can ever be typed into the
///      composer: no peer message, no orchestrator mail wake;
///   2. minimal mode draws no composer box, so a boxed-prompt matcher never
///      fires `Ready` and the session stays BUSY for the whole process;
///   3. `completed` needs the `suggest:` marker to survive the chrome trim.
///
/// Ready must be the LAST verdict and Working must have occurred: a screen
/// adapter that only ever answers `Unknown` leaves `idle_confirmed` false,
/// which reads as "idle" to `agent_state` but blocks `should_inject_now` —
/// the mismatch that burns the orchestrator wake budget permanently.
#[test]
fn grok_minimal_capture_reaches_ready_and_declares_completion() {
    use crate::state::VtLogBuffer;

    assert_eq!(classify_agent("grok-1.0.5"), Some("grok"));
    assert!(has_ready_screen_adapter(classify_agent("grok-1.0.5")));

    let bytes = agent_prompt_fixture("grok-1.0.5-minimal-turn.tcap");
    let mut vt_log = VtLogBuffer::new(40, 120, 2000);
    let mut parser = crate::output_parser::OutputParser::new();
    let mut saw_working = false;
    let mut last_activity = AgentScreenActivity::Unknown;
    let mut suggested = None;

    for record in crate::pty_capture::decode(&bytes).expect("valid capture") {
        if record.direction != crate::pty_capture::CaptureDirection::Output {
            continue;
        }
        let mut changed = vt_log.process(&record.data);
        let screen = vt_log.screen_rows();
        let refs: Vec<&str> = screen.iter().map(String::as_str).collect();
        if let Some(cutoff) = crate::chrome::find_chrome_cutoff(&refs) {
            changed.retain(|row| row.row_index < cutoff);
        }
        for event in parser.parse_clean_lines(&changed, true) {
            if let ParsedEvent::Suggest { items } = event {
                suggested = Some(items);
            }
        }
        match detect_agent_screen_activity(Some("grok"), &screen) {
            AgentScreenActivity::Unknown => {}
            activity => {
                saw_working |= activity == AgentScreenActivity::Working;
                last_activity = activity;
            }
        }
    }

    assert!(
        saw_working,
        "grok's turn-status spinner must mark the session working, or a busy \
             turn reads as idle and a peer message is typed into a live composer"
    );
    assert_eq!(
        last_activity,
        AgentScreenActivity::Ready,
        "the bare `❯` composer row of minimal mode must end the turn Ready"
    );
    assert_eq!(
        suggested.as_deref(),
        Some(
            &[
                "Altra richiesta".to_string(),
                "Fermati".to_string(),
                "Ripeti il conteggio".to_string(),
            ][..]
        ),
        "the `suggest:` marker must survive the chrome trim — it is the only \
             thing that promotes grok from `idle` to `completed`"
    );
}

/// Replay a whole directory of real `.tcap` captures through the production
/// composition and report what the detection pipeline made of them.
///
/// Ignored by default: the corpus is whatever the operator recorded through
/// `POST /diagnostics/capture`, and those files hold real session content —
/// prompts, source, paths — so they are deliberately NOT committed. This is a
/// measurement harness, not a regression test. What it surfaces becomes
/// either a code fix or a single committed fixture, chosen deliberately.
///
/// ```text
/// TUIC_CAPTURE_CORPUS="$HOME/Library/Application Support/com.tuic.commander/captures" \
///   cargo test -p tuicommander detection_over_capture_corpus -- --ignored --nocapture
/// ```
#[test]
#[ignore = "needs a capture corpus; see TUIC_CAPTURE_CORPUS"]
fn detection_over_capture_corpus() {
    let Ok(dir) = std::env::var("TUIC_CAPTURE_CORPUS") else {
        panic!("set TUIC_CAPTURE_CORPUS to a directory of .tcap/.raw captures");
    };
    let mut paths: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .expect("readable corpus directory")
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("tcap" | "raw")
            )
            .then_some(path)
        })
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "corpus held no captures");

    for path in &paths {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let bytes = std::fs::read(path).expect("readable capture");
        let Ok(records) = crate::pty_capture::decode(&bytes) else {
            println!("{name:<42} UNDECODABLE");
            continue;
        };
        if records.is_empty() {
            println!("{name:<42} empty");
            continue;
        }

        let (events, stats) = replay_capture_measured(&bytes, false);
        let fail_open = if stats.ticks_with_content == 0 {
            0.0
        } else {
            100.0 * stats.ticks_without_cutoff as f64 / stats.ticks_with_content as f64
        };
        println!(
            "\n{name}\n  {:>6} out / {:>4} in records, {:>7} bytes | rows offered {:>6}, \
                 trimmed {:>6} | no-cutoff {:>5.1}% of {} content ticks",
            stats.output_records,
            stats.input_records,
            stats.bytes,
            stats.rows_offered,
            stats.rows_trimmed,
            fail_open,
            stats.ticks_with_content,
        );

        let mut kinds: std::collections::BTreeMap<String, usize> = Default::default();
        for event in &events {
            let kind = serde_json::to_value(event)
                .ok()
                .and_then(|v| v["type"].as_str().map(str::to_string))
                .unwrap_or_else(|| "?".to_string());
            *kinds.entry(kind).or_default() += 1;
        }
        if kinds.is_empty() {
            println!("  events: none");
        } else {
            let rendered: Vec<String> = kinds.iter().map(|(k, n)| format!("{k}×{n}")).collect();
            println!("  events: {}", rendered.join(", "));
        }

        // Awaiting is sticky by construction: whatever SETs it owns nothing
        // until something retracts it. Walk the sequence and report the
        // badge a tab would still be rendering at the end of the capture.
        let mut awaiting: Option<String> = None;
        let mut sets = 0usize;
        let mut clears = 0usize;
        for event in &events {
            match event {
                ParsedEvent::Question { prompt_text, .. } => {
                    sets += 1;
                    awaiting = Some(prompt_text.clone());
                }
                ParsedEvent::QuestionCleared | ParsedEvent::UserInput { .. } => {
                    clears += usize::from(awaiting.take().is_some());
                }
                _ => {}
            }
        }
        match awaiting {
            Some(prompt) => {
                println!("  awaiting: {sets} set / {clears} cleared → STILL SET at end: {prompt:?}")
            }
            None if sets > 0 => println!("  awaiting: {sets} set / {clears} cleared → clear"),
            None => {}
        }
    }
}

fn awaiting_prompts(events: &[ParsedEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            ParsedEvent::Question {
                prompt_text,
                confident: true,
            } => Some(prompt_text.clone()),
            _ => None,
        })
        .collect()
}

/// Every prompt the pipeline reported, whatever its confidence.
fn awaiting_prompts_any_confidence(events: &[ParsedEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            ParsedEvent::Question { prompt_text, .. } => Some(prompt_text.clone()),
            _ => None,
        })
        .collect()
}

/// The old fixture name calls this a plan picker, but its bytes show a ready
/// `❯` composer followed by Claude's generic desktop notification. There is
/// no Ink footer or visible picker in the captured suffix. This notification
/// is therefore insufficient evidence of awaiting even for a hooked session.
#[test]
fn hooked_claude_generic_notify_capture_is_not_a_question() {
    let events = replay_capture(&agent_prompt_fixture("claude-plan-picker.raw"), true);
    let prompts = awaiting_prompts_any_confidence(&events);
    assert!(
        !prompts
            .iter()
            .any(|p| p == "Claude is waiting for your input"),
        "the ready-composer notification is not a question: {prompts:?}"
    );
}

/// The OSC notification's ambiguity is independent of hook configuration.
#[test]
fn generic_osc777_notify_is_not_a_question_with_or_without_hooks() {
    for hook in [true, false] {
        let events = replay_capture(&agent_prompt_fixture("claude-plan-picker.raw"), hook);
        assert!(
            !awaiting_prompts_any_confidence(&events)
                .iter()
                .any(|p| p == "Claude is waiting for your input"),
            "generic notify badged a session with hook_instrumented={hook}"
        );
    }
}

/// Regression for the other observed Claude notification payload. OSC 777
/// is a desktop-notification transport, so the generic "needs your
/// attention" body is not proof that the composer awaits a response. Treating
/// it as a confident question latched awaiting after completion indefinitely.
#[test]
fn generic_osc777_attention_does_not_report_awaiting() {
    for hook in [true, false] {
        let events = replay_capture(&agent_prompt_fixture("claude-generic-attention.raw"), hook);
        assert!(
            awaiting_prompts_any_confidence(&events).is_empty(),
            "generic notification became awaiting with hook_instrumented={hook}: {events:?}"
        );
    }
}

/// The OSC body is taken from the recorded Claude PTY sequence in
/// `claude-plan-picker.raw`. A normal completed turn leaves a ready composer;
/// the same generic desktop notification arrives later even without a dialog.
/// Both prose endings were observed on live Claude screens on 2026-09-27.
#[test]
fn completed_claude_prose_notification_does_not_flash_awaiting() {
    let capture = agent_prompt_fixture("claude-plan-picker.raw");
    let notify = "\x1b]777;notify;Claude Code;Claude is waiting for your input\x07";
    assert!(
        capture
            .windows(notify.len())
            .any(|bytes| bytes == notify.as_bytes())
    );

    for answer in ["The review is complete.", "Would you like a summary?"] {
        let sid = "claude-completed-notify";
        let state = crate::state::tests_support::make_test_app_state();
        agent_session(&state, sid, SHELL_IDLE);
        {
            let mut session = state.session_maps.session_states.get_mut(sid).unwrap();
            session.agent_type = Some("claude".into());
            session.hook_instrumented = true;
        }
        state.grid.vt_log_buffers.insert(
            sid.into(),
            Mutex::new(crate::state::VtLogBuffer::new(24, 100, 2000)),
        );
        let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
        silence.lock().startup_settled = true;
        let mut processor = ChunkProcessor::new(None, None);
        let screen = format!(
            "\x1b[2J\x1b[H{answer}\r\n✻ Cooked for 15s · done\r\n────────────────────────\r\n❯\r\n────────────────────────"
        );
        processor.process_chunk(&screen, &silence, sid, &state);
        processor.process_chunk("\x1b]7770;state=idle\x07", &silence, sid, &state);
        assert!(silence.lock().hook_state_seen);
        let mut events = state.event_bus.subscribe();
        processor.process_chunk(notify, &silence, sid, &state);
        while let Ok(event) = events.try_recv() {
            assert!(
                !matches!(event, crate::state::AppEvent::PtyParsed { parsed, .. }
                    if parsed["type"] == "question"),
                "a desktop idle notification after {answer:?} must not badge Waiting input"
            );
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn hooked_claude_prose_question_stays_idle_after_silence_tick() {
    tokio::time::pause();
    let sid = "claude-ready-prose-question";
    let state = accumulating_state(sid);
    agent_session(&state, sid, SHELL_IDLE);
    {
        let mut session = state.session_maps.session_states.get_mut(sid).unwrap();
        session.agent_type = Some("claude".into());
        session.hook_instrumented = true;
    }
    let mut vt = crate::state::VtLogBuffer::new(24, 100, 2000);
    vt.process("\x1b[2J\x1b[HWould you like a summary?\r\n────\r\n❯".as_bytes());
    assert_eq!(
        current_chat_question(&vt.screen_rows()),
        CurrentChatQuestion::PromptAnchored(Some("Would you like a summary?".into())),
        "the screen really has the candidate the silence timer would consider"
    );
    state.grid.vt_log_buffers.insert(sid.into(), Mutex::new(vt));
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
    {
        let mut sl = silence.lock();
        sl.startup_settled = true;
        sl.note_explicit_state(SHELL_IDLE, true);
        sl.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD;
    }
    let mut events = state.event_bus.subscribe();
    let running = Arc::new(AtomicBool::new(true));
    spawn_silence_timer(silence, running.clone(), sid.into(), state.clone());
    tokio::task::yield_now().await;
    tokio::time::advance(SILENCE_CHECK_INTERVAL + std::time::Duration::from_millis(1)).await;
    tokio::task::yield_now().await;
    running.store(false, Ordering::Release);
    while let Ok(event) = events.try_recv() {
        assert!(
            !matches!(event, crate::state::AppEvent::PtyParsed { parsed, .. }
                if parsed["type"] == "question"),
            "hooked Claude prose must not produce a silence-timer question"
        );
    }
    assert!(
        !state
            .session_maps
            .session_states
            .get(sid)
            .unwrap()
            .awaiting_input
    );
}

#[test]
fn recorded_claude_ready_notification_does_not_report_awaiting() {
    let notify = b"\x1b]777;notify;Claude Code;Claude is waiting for your input\x07";
    for fixture in [
        "claude-ready-idle-notify.tcap",
        "claude-ready-idle-notify-statement.tcap",
    ] {
        let bytes = agent_prompt_fixture(fixture);
        let capture = crate::pty_capture::decode_capture(&bytes).expect("recorded PTY capture");
        assert!(
            capture
                .records
                .iter()
                .any(|record| { record.data.windows(notify.len()).any(|part| part == notify) })
        );
        assert!(
            awaiting_prompts_any_confidence(&replay_capture(&bytes, true)).is_empty(),
            "{fixture}: a ready composer plus generic idle notify must not report a question"
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn visible_dialog_and_explicit_permission_still_badge_awaiting() {
    let ink = askuserquestion_wizard_screen(0);
    let choice: Vec<String> = ["Proceed with deletion?", "❯ 1. Yes", "  2. No"]
        .into_iter()
        .map(str::to_string)
        .collect();
    let signals = [
        rearm_awaiting_for_open_dialog(&ink, false, false, false)
            .expect("the visible Ink footer is a real dialog"),
        crate::output_parser::parse_choice_prompt(&choice)
            .expect("the numbered choice is a real dialog"),
        crate::output_parser::parse_osc777_notify(
            "\x1b]777;notify;Claude Code;Claude needs your permission\x07",
        )
        .expect("permission wording requires a response"),
    ];
    for (index, signal) in signals.into_iter().enumerate() {
        let sid = format!("real-awaiting-{index}");
        let state = accumulating_state(&sid);
        state.emit_pty_event(crate::state::AppEvent::PtyParsed {
            session_id: sid.clone(),
            parsed: serde_json::to_value(&signal).unwrap().into(),
        });
        assert!(
            await_session(&state, &sid, |s| s.awaiting_input && s.question_confident).await,
            "a real interactive signal must reach the badge: {signal:?}"
        );
    }
}

// --- Awaiting RETRACTION -----------------------------------------------
//
// Why the fixtures above could not catch the stuck "question" badge: they
// replay bytes through the PARSERS and assert which events come out. The
// badge is not an event, it is `SessionState.awaiting_input` — and this
// failure was the ABSENCE of any event, so no capture can express it. The
// tests below close that gap by driving the real accumulator instead of
// the parser output: they assert the state a tab actually renders.

/// Wait for the event-bus accumulator to apply what we emitted. Polls
/// rather than sleeping a fixed amount so it neither flakes nor stalls.
async fn await_session<F: Fn(&crate::state::SessionState) -> bool>(
    state: &Arc<AppState>,
    session_id: &str,
    pred: F,
) -> bool {
    for _ in 0..200 {
        if state
            .session_maps
            .session_states
            .get(session_id)
            .is_some_and(|s| pred(&s))
        {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
    }
    false
}

fn accumulating_state(session_id: &str) -> Arc<AppState> {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    state.session_maps.session_states.insert(
        session_id.to_string(),
        crate::state::SessionState::default(),
    );
    crate::state::AppState::spawn_session_state_accumulator(state.clone());
    state
}

fn heuristic_question(session_id: &str, prompt: &str) -> crate::state::AppEvent {
    crate::state::AppEvent::PtyParsed {
        session_id: session_id.to_string(),
        parsed: serde_json::json!({
            "type": "question",
            "prompt_text": prompt,
            "confident": false,
        })
        .into(),
    }
}

/// Regression, observed 2026-08-10 on a live codex tab: the turn had
/// finished, the approval dialog was long gone, and the tab still read
/// "question".
///
/// The sequence, end to end: codex prints "Would you like to make the
/// following edits?", the silence heuristic verifies it on screen and emits
/// a low-confidence Question, Boss answers with a bare Enter. That Enter
/// produces no `user-input` (that arm needs a non-empty typed line), codex
/// goes busy through screen movement rather than a parsed `status-line`,
/// and no `choice-prompt` was ever set to resolve. Every existing clear
/// needs an event that never arrives — so the badge latched.
#[tokio::test(flavor = "current_thread", start_paused = false)]
async fn stale_heuristic_awaiting_is_retracted_when_the_prompt_leaves_the_screen() {
    use crate::state::VtLogBuffer;

    let question = "Would you like to make the following edits?";
    let mut vt = VtLogBuffer::new(41, 128, 2000);
    vt.process(format!("\x1b[2J\x1b[H{question}\r\n").as_bytes());
    assert!(
        verify_question_on_screen(&vt.screen_rows(), question, SCREEN_VERIFY_ROWS),
        "precondition: the prompt is on screen, so the heuristic fires"
    );

    let state = accumulating_state("s1");
    state.emit_pty_event(heuristic_question("s1", question));
    assert!(
        await_session(&state, "s1", |s| s.awaiting_input).await,
        "the low-confidence question must badge the tab"
    );

    // Bare Enter: codex repaints the finished turn, prompt gone.
    vt.process(b"\x1b[2J\x1b[HDone. 2 files changed.\r\n");
    assert!(
        !verify_question_on_screen(&vt.screen_rows(), question, SCREEN_VERIFY_ROWS),
        "the prompt must be gone — this is the branch that retracts"
    );

    emit_question_cleared_if_stale(&state, "s1");
    assert!(
        await_session(&state, "s1", |s| !s.awaiting_input
            && s.question_text.is_none())
        .await,
        "the badge must drop once the question left the screen"
    );
}

/// Permission wording remains a confident question; the generic idle wording
/// never reaches the accumulator as a Question.
#[tokio::test(flavor = "current_thread")]
async fn osc777_notify_only_badges_unambiguous_permission() {
    for body in ["Claude needs your permission", "approval required"] {
        let raw = format!("\x1b]777;notify;Claude Code;{body}\x07");
        let notify = crate::output_parser::parse_osc777_notify(&raw)
            .unwrap_or_else(|| panic!("{body:?} must still report awaiting"));

        let state = accumulating_state("s1");
        state.emit_pty_event(crate::state::AppEvent::PtyParsed {
            session_id: "s1".to_string(),
            parsed: serde_json::to_value(&notify).expect("serialisable").into(),
        });
        assert!(
            await_session(&state, "s1", |s| s.awaiting_input).await,
            "{body:?} must badge the tab"
        );

        // The screen is quiet and carries no prompt — the recap case.
        emit_question_cleared_if_stale(&state, "s1");
        let cleared = await_session(&state, "s1", |s| !s.awaiting_input).await;
        assert!(!cleared, "{body:?} must remain until user input");
    }
    assert!(
        crate::output_parser::parse_osc777_notify(
            "\x1b]777;notify;Claude Code;Claude is waiting for your input\x07"
        )
        .is_none()
    );
}

/// grok repaints while it waits, so "not on screen this tick" is not proof
/// that a confident prompt was answered. Retracting it would drop a real
/// approval request — the same reason the status-line arm keeps it sticky.
#[tokio::test(flavor = "current_thread")]
async fn confident_awaiting_is_never_retracted() {
    let state = accumulating_state("s1");
    state.emit_pty_event(crate::state::AppEvent::PtyParsed {
        session_id: "s1".to_string(),
        parsed: serde_json::json!({
            "type": "question",
            "prompt_text": "Run echo x",
            "confident": true,
        })
        .into(),
    });
    assert!(await_session(&state, "s1", |s| s.awaiting_input).await);

    emit_question_cleared_if_stale(&state, "s1");
    assert!(
        !await_session(&state, "s1", |s| !s.awaiting_input).await,
        "a confident question must survive the retraction"
    );
}

/// Live Claude 2.1.280 capture: AskUserQuestion notified a confident wait,
/// Esc dismissed it without a typed line, and the turn ended at the composer.
/// The badge must follow that completed turn, not the historical notification.
/// The mobile choice overlay is still set when the decline paints, and the shell
/// state must still leave BUSY: Claude sends no Stop hook after Esc.
#[tokio::test(flavor = "current_thread")]
async fn claude_askuser_esc_capture_retracts_awaiting_after_turn_done() {
    let capture = crate::pty_capture::decode_capture(&agent_prompt_fixture(
        "claude-askuser-esc-20260929.tcap",
    ))
    .expect("valid live capture");
    let (rows, cols) = capture.geometry.expect("recorded terminal geometry");
    let sid = "claude-askuser-esc";
    let (state, silence) = chunk_trace_state(sid);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .agent_type = Some("claude".into());
    state
        .grid
        .vt_log_buffers
        .insert(sid.into(), Mutex::new(VtLogBuffer::new(rows, cols, 2000)));
    crate::state::AppState::spawn_session_state_accumulator(state.clone());
    let mut processor = ChunkProcessor::new(None, None);
    let mut utf8 = Utf8ReadBuffer::new();
    let mut escape = EscapeAwareBuffer::new();
    let mut saw_wait = false;
    let mut saw_esc = false;
    let mut saw_done = false;
    for record in capture.records {
        match record.direction {
            crate::pty_capture::CaptureDirection::Output => {
                let data = utf8.push(&record.data);
                let data = escape.push(&data);
                let (clean, _) = crate::state::strip_kitty_sequences(&data);
                processor.process_chunk(&clean, &silence, sid, &state);
                if clean.contains("Claude needs your permission") {
                    saw_wait = true;
                    assert!(
                        await_session(&state, sid, |s| {
                            s.awaiting_input
                                && s.question_confident
                                && s.question_text.as_deref()
                                    == Some("Claude needs your permission")
                        })
                        .await,
                        "an open AskUserQuestion must keep the confident badge"
                    );
                    // Catches: awaiting_input is set, but the mobile choice
                    // overlay has no title or options for the live Claude dialog.
                    assert!(
                        await_session(&state, sid, |s| {
                            s.choice_prompt.as_ref().is_some_and(|prompt| {
                                prompt.title == "Which color do you prefer?"
                                    && prompt.options.len() == 5
                                    && prompt.selection_mode
                                        == Some(crate::output_parser::ChoiceSelectionMode::NavigateEnter)
                                    && prompt
                                        .options
                                        .iter()
                                        .any(|option| option.key == "2" && option.label == "Green")
                            })
                        })
                        .await,
                        "live AskUserQuestion must expose its choices to mobile"
                    );
                }
                saw_done |= clean.contains("Worked for 4s");
            }
            crate::pty_capture::CaptureDirection::Input if saw_wait && record.data == b"\x1b" => {
                saw_esc = true;
                assert!(
                    await_session(&state, sid, |s| s.awaiting_input).await,
                    "bare Esc must not retract a still-open dialog before Claude responds"
                );
            }
            crate::pty_capture::CaptureDirection::Input => {}
        }
    }
    assert!(
        saw_wait && saw_esc && saw_done,
        "capture must contain the full scenario"
    );
    assert!(
        await_session(&state, sid, |s| !s.awaiting_input && !s.question_confident).await,
        "the completed turn must retract Claude's dismissed question"
    );

    // Catches: Esc ends the turn with no Stop hook, so the hook-driven BUSY
    // stays latched and queued input never flushes. Nothing here stores IDLE by
    // hand: the dismissal itself must have released the session.
    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(sid)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_IDLE,
        "the dismissed AskUserQuestion must return the session to idle"
    );

    // The captured composer is ready after Claude finishes; exercise the same
    // PTY write used by MCP submit.
    #[cfg(unix)]
    {
        silence.lock().confirm_idle();
        // Captured agent output owns this recording composer; the shell
        // child is only its PTY holder, not a foreground-detection scenario.
        state
            .session_maps
            .session_states
            .get_mut(sid)
            .unwrap()
            .spawn_root_role = crate::state::SpawnRootRole::DirectProgram;
        let bytes = insert_recording_session(&state, sid);
        assert!(matches!(
            write_agent_submission_to_pty(&state, sid, "echo ready"),
            AgentSubmissionWrite::Complete { .. }
        ));
        assert!(
            bytes
                .lock()
                .unwrap()
                .windows(b"echo ready".len())
                .any(|part| part == b"echo ready")
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn protocol_awaiting_clears_on_protocol_busy_and_idle() {
    for (session_id, target, label) in [
        ("protocol-clear-busy", SHELL_BUSY, "busy"),
        ("protocol-clear-idle", SHELL_IDLE, "idle"),
    ] {
        let state = accumulating_state(session_id);
        state.session_maps.shell_states.insert(
            session_id.to_string(),
            std::sync::atomic::AtomicU8::new(if target == SHELL_BUSY {
                SHELL_IDLE
            } else {
                SHELL_BUSY
            }),
        );
        state.session_maps.silence_states.insert(
            session_id.to_string(),
            Arc::new(Mutex::new(SilenceState::new())),
        );
        state.emit_pty_event(crate::state::AppEvent::PtyParsed {
            session_id: session_id.to_string(),
            parsed: serde_json::json!({
                "type": "question",
                "prompt_text": "approval required",
                "confident": true,
            })
            .into(),
        });
        assert!(await_session(&state, session_id, |s| s.awaiting_input).await);

        transition_explicit_shell_state_with_hook(&state, session_id, target, label, true, || {});
        assert!(
            await_session(&state, session_id, |s| !s.awaiting_input
                && s.question_text.is_none())
            .await,
            "{label} must clear Protocol awaiting"
        );
        assert_eq!(
            state
                .session_maps
                .silence_states
                .get(session_id)
                .unwrap()
                .lock()
                .awaiting_rank(),
            None
        );
    }
}

/// A live choice prompt owns its own resolution (`resolve_choice_prompt_input`
/// fires on the option keypress). Retracting under it would clear the badge
/// while the dialog is still on screen waiting for a key.
#[test]
fn retraction_skips_a_session_with_a_live_choice_prompt() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let mut session = crate::state::SessionState {
        awaiting_input: true,
        question_confident: false,
        ..Default::default()
    };
    session.choice_prompt = Some(crate::output_parser::ChoicePromptPayload {
        title: "Which approach should I use?".to_string(),
        options: vec![],
        selection_mode: None,
        dismiss_key: None,
        amend_key: None,
    });
    state
        .session_maps
        .session_states
        .insert("s1".to_string(), session);

    let mut rx = state.event_bus.subscribe();
    emit_question_cleared_if_stale(&state, "s1");
    assert!(
        rx.try_recv().is_err(),
        "no retraction may be emitted while a choice prompt is live"
    );
}

/// The producer (here) and the consumer (state.rs) agree on one wire name.
/// A rename on either side would silently disable the retraction, which is
/// exactly the failure mode this whole path exists to prevent.
#[test]
fn retraction_is_emitted_as_question_cleared() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    state.session_maps.session_states.insert(
        "s1".to_string(),
        crate::state::SessionState {
            awaiting_input: true,
            question_confident: false,
            ..Default::default()
        },
    );

    let mut rx = state.event_bus.subscribe();
    emit_question_cleared_if_stale(&state, "s1");
    match rx.try_recv() {
        Ok(crate::state::AppEvent::PtyParsed { parsed, .. }) => {
            assert_eq!(
                parsed.get("type").and_then(|t| t.as_str()),
                Some("question-cleared")
            );
        }
        other => panic!("expected a PtyParsed retraction, got {other:?}"),
    }
}

/// A multi-question `AskUserQuestion` as Claude renders it: a tab bar of
/// sub-questions, the current one's title and options, and the Ink footer.
/// `answered` moves the ⊠ and swaps the body — everything except the footer.
fn askuserquestion_wizard_screen(answered: usize) -> Vec<String> {
    let tabs = ["CLI.md", "Exit codes", "Provider row"];
    let bar = tabs
        .iter()
        .enumerate()
        .map(|(i, t)| format!("{} {t}", if i < answered { "⊠" } else { "□" }))
        .collect::<Vec<_>>()
        .join("  ");
    vec![
        format!("←  {bar}  ✓ Submit  →"),
        String::new(),
        format!("Sub-question {}: what should step 14 send?", answered + 1),
        String::new(),
        format!("› 1. Option A for {}", tabs[answered.min(2)]),
        format!("  2. Option B for {}", tabs[answered.min(2)]),
        "  3. Type something.".to_string(),
        String::new(),
        "Enter to select · Tab/Arrow keys to navigate · Esc to cancel".to_string(),
    ]
}

/// Regression, observed 2026-08-21 on a live Claude tab: a multi-question
/// AskUserQuestion was on screen waiting on Boss and the tab read "working".
///
/// The first sub-question badges the tab. Answering it clears the badge — and
/// the second sub-question repaints its title and options while the footer row
/// stays byte-identical, so the changed-rows parser never fires again and no
/// clear path is at fault: the SET simply never came back. The re-arm is the
/// only thing standing between that and a tab that lies for the rest of the
/// wizard.
#[tokio::test(flavor = "current_thread", start_paused = false)]
async fn open_dialog_rearms_awaiting_after_a_sub_question_is_answered() {
    let first = askuserquestion_wizard_screen(0);
    let footer = crate::output_parser::ink_dialog_footer(&first)
        .expect("precondition: the Ink footer anchors the dialog");

    let state = accumulating_state("s1");
    state.emit_pty_event(crate::state::AppEvent::PtyParsed {
        session_id: "s1".to_string(),
        parsed: serde_json::json!({
            "type": "question", "prompt_text": footer, "confident": true,
        })
        .into(),
    });
    assert!(
        await_session(&state, "s1", |s| s.awaiting_input).await,
        "the first sub-question must badge the tab"
    );

    // Boss answers it. Whatever cleared the badge — a typed line here — the
    // wizard is still open on its next sub-question.
    state.emit_pty_event(crate::state::AppEvent::PtyParsed {
        session_id: "s1".to_string(),
        parsed: serde_json::json!({ "type": "user-input", "content": "1" }).into(),
    });
    assert!(
        await_session(&state, "s1", |s| !s.awaiting_input).await,
        "precondition: answering clears the badge"
    );

    let second = askuserquestion_wizard_screen(1);
    assert_ne!(second[2], first[2], "the body moved on");
    assert_eq!(
        second.last(),
        first.last(),
        "…but the footer did not — this is why the changed-rows parser is blind"
    );

    let evt = rearm_awaiting_for_open_dialog(&second, false, false, false)
        .expect("an open dialog with the badge off must re-arm");
    let ParsedEvent::Question {
        prompt_text,
        confident,
    } = &evt
    else {
        panic!("re-arm must be a Question, got {evt:?}");
    };
    assert_eq!(prompt_text, footer);
    assert!(confident, "an Ink footer is not a guess");

    state.emit_pty_event(crate::state::AppEvent::PtyParsed {
        session_id: "s1".to_string(),
        parsed: serde_json::json!({
            "type": "question", "prompt_text": prompt_text, "confident": confident,
        })
        .into(),
    });
    assert!(
        await_session(&state, "s1", |s| s.awaiting_input).await,
        "the tab must read awaiting again while the wizard is open"
    );
}

/// The re-arm must not fire per repaint, duplicate a pending question, or
/// step on a live choice prompt — each of those was a separate storm in the
/// history of this file.
#[test]
fn rearm_stays_silent_unless_the_badge_is_actually_off() {
    let screen = askuserquestion_wizard_screen(1);
    assert!(
        rearm_awaiting_for_open_dialog(&screen, true, false, false).is_none(),
        "already awaiting — re-arming every repaint would storm"
    );
    assert!(
        rearm_awaiting_for_open_dialog(&screen, false, true, false).is_none(),
        "a live choice prompt owns awaiting through its own resolve path"
    );
    let no_dialog = vec!["· Gallivanting… (15m 12s)".to_string(), "❯".to_string()];
    assert!(
        rearm_awaiting_for_open_dialog(&no_dialog, false, false, false).is_none(),
        "no dialog on screen, no badge"
    );
    let quoted = vec!["+  Enter to select · Esc to cancel".to_string()];
    assert!(
        rearm_awaiting_for_open_dialog(&quoted, false, false, false).is_none(),
        "a diff line quoting the footer is not a dialog"
    );
}

#[test]
fn hook_instrumented_open_dialog_recovers_a_missing_question_badge() {
    let screen = askuserquestion_wizard_screen(0);
    assert!(
        rearm_awaiting_for_open_dialog(&screen, false, false, false).is_some(),
        "a visible dialog still needs the user when the hook's awaiting signal is absent"
    );
}

/// Retained raw PTY bytes from md-2 / Story 131: awaiting is followed by
/// fifteen busy hooks, then a still-open dialog. Original read boundaries are
/// unavailable; a controlled split preserves the notification-before-busy order.
#[tokio::test]
async fn hooked_dialog_capture_preserves_question_until_the_dialog_is_answered() {
    use crate::state::VtLogBuffer;
    let sid = "hooked-dialog-capture";
    let state = accumulating_state(sid);
    agent_session(&state, sid, SHELL_BUSY);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .hook_instrumented = true;
    // Observed independently with stty on the session's PTY, not a default.
    state
        .grid
        .vt_log_buffers
        .insert(sid.into(), Mutex::new(VtLogBuffer::new(63, 236, 2000)));
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
    let mut processor = ChunkProcessor::new(None, None);
    let bytes = agent_prompt_fixture("claude-hooked-missing-question-20260921.raw");
    let capture = std::str::from_utf8(&bytes).expect("complete UTF-8 capture suffix");
    let notification = capture
        .find("\x1b]777;notify;")
        .expect("captured notification");
    let split = notification
        + capture[notification..]
            .find("\x1b]7770;state=busy")
            .expect("busy after notification");
    processor.process_chunk(&capture[..split], &silence, sid, &state);
    assert!(await_session(&state, sid, |s| s.awaiting_input).await);
    // Exact contiguous suffix of the retained live stream, starting at the
    // first busy hook after notify. The original PTY read boundaries are lost.
    let busy_tail = agent_prompt_fixture("claude-hooked-busy-choice-redraw-20260921.raw");
    assert_eq!(busy_tail, capture.as_bytes()[split..]);
    processor.process_chunk(
        std::str::from_utf8(&busy_tail).unwrap(),
        &silence,
        sid,
        &state,
    );
    let screen = state
        .grid
        .vt_log_buffers
        .get(sid)
        .unwrap()
        .lock()
        .screen_rows();
    crate::output_parser::ink_dialog_footer(&screen)
        .expect("the captured dialog must be visible at the observed geometry");
    assert!(silence.lock().hook_state_seen);
    assert!(
        await_session(&state, sid, |s| s.awaiting_input
            && s.question_text
                .as_deref()
                .is_some_and(|text| text.ends_with("procedo?"))
            && s.choice_prompt.is_some())
        .await,
        "a hooked session with an open dialog must report its visible question after all queued hooks"
    );

    let mut events = state.event_bus.subscribe();
    processor.process_chunk("\x1b[?25h", &silence, sid, &state);
    assert!(
        std::iter::from_fn(|| events.try_recv().ok()).all(|event| !matches!(event,
            crate::state::AppEvent::PtyParsed { parsed, .. } if parsed["type"] == "question")),
        "an already-badged dialog must not emit a question on each repaint"
    );

    // A hook alone cannot prove the cached dialog is still live. Clear first;
    // only an actual repaint of a choice row may restore the same choice.
    processor.process_chunk("\x1b]7770;state=busy\x07", &silence, sid, &state);
    state.emit_pty_event(crate::state::AppEvent::PtyParsed {
        session_id: sid.to_string(),
        parsed: serde_json::json!({ "type": "active-subtasks", "count": 6 }).into(),
    });
    assert!(
        await_session(&state, sid, |s| s.active_sub_tasks == 6
            && !s.awaiting_input
            && s.choice_prompt.is_none())
        .await,
        "a pure busy hook must not resurrect a stale dialog"
    );

    let option_row = screen
        .iter()
        .position(|row| row.contains("Tengo la riscrittura"))
        .expect("captured choice option");
    let repainted_option = screen[option_row].replacen('❯', " ", 1);
    assert_ne!(repainted_option, screen[option_row]);
    // Repaint a real option row without changing the option set. A busy hook
    // clears the old choice, so signature dedup must admit this fresh screen evidence.
    processor.process_chunk(
        &format!("\x1b[{};1H\x1b[2K{}", option_row + 1, repainted_option),
        &silence,
        sid,
        &state,
    );
    // A queued marker makes the accumulator drain every event from this chunk
    // before the assertion observes the state; the previous true bit is not proof.
    state.emit_pty_event(crate::state::AppEvent::PtyParsed {
        session_id: sid.to_string(),
        parsed: serde_json::json!({ "type": "active-subtasks", "count": 7 }).into(),
    });
    assert!(
        await_session(&state, sid, |s| s.active_sub_tasks == 7
            && s.awaiting_input
            && s.question_text
                .as_deref()
                .is_some_and(|text| text.ends_with("procedo?"))
            && s.choice_prompt.is_some())
        .await,
        "the open choice dialog must retain its question after queued hook transitions"
    );
    processor.process_chunk(
        "\x1b[2J\x1b[H\x1b]7770;state=busy\x07",
        &silence,
        sid,
        &state,
    );
    assert!(
        await_session(&state, sid, |s| !s.awaiting_input
            && s.question_text.is_none())
        .await,
        "once the dialog disappears and work resumes, question must clear"
    );
}

/// The opening frame of a non-hook `AskUserQuestion`: the footer row genuinely
/// changed, so `parse_clean_lines` already parsed the real question this tick.
/// `SessionState.awaiting_input` is still false — this tick's events have not
/// reached it — so the badge guard alone lets the re-arm fire as well. The
/// accumulator keeps the LAST `prompt_text`, so the second event replaces the
/// question with the footer and the tab reads `⊠ … ✓ Submit`. It also resets
/// `last_question_text`, so the real question re-emits on the next repaint.
#[test]
fn rearm_yields_to_a_question_already_parsed_in_the_same_tick() {
    let screen = askuserquestion_wizard_screen(0);
    assert!(
        rearm_awaiting_for_open_dialog(&screen, false, false, false).is_some(),
        "precondition: this screen re-arms when nothing else spoke"
    );
    assert!(
        rearm_awaiting_for_open_dialog(&screen, false, false, true).is_none(),
        "the parsed question is the better text; the footer must not overwrite it"
    );
}

#[test]
fn question_suppress_resolves_from_agent_config() {
    use crate::config::{AgentSettings, AgentsConfig};
    let mut agents = AgentsConfig::default();
    let enabled = AgentSettings {
        hook_instrumentation: Some(true),
        ..Default::default()
    };
    agents.agents.insert("claude".into(), enabled);
    let disabled = AgentSettings {
        native_status_signals: Some(false),
        ..Default::default()
    };
    agents.agents.insert("codex".into(), disabled);

    assert!(hook_instrumented_for(&agents, Some("claude")));
    assert!(
        !hook_instrumented_for(&agents, Some("codex")),
        "explicit false"
    );
    assert!(
        !hook_instrumented_for(&agents, Some("gemini")),
        "no override"
    );
    assert!(!hook_instrumented_for(&agents, None), "no agent type");
}

#[test]
fn osc133_a_transitions_to_idle_immediately() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-osc133-idle";
    state.session_maps.shell_states.insert(
        session_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_BUSY),
    );
    state
        .session_maps
        .shell_state_since_ms
        .insert(session_id.to_string(), std::sync::atomic::AtomicU64::new(0));
    state
        .session_maps
        .has_osc133_integration
        .insert(session_id.to_string(), ());

    let mut proc = ChunkProcessor::new(None, None);
    proc.handle_osc133_event('A', "", session_id, &state);

    let current = state
        .session_maps
        .shell_states
        .get(session_id)
        .unwrap()
        .load(std::sync::atomic::Ordering::Acquire);
    assert_eq!(
        current, SHELL_IDLE,
        "OSC 133 A should transition to idle immediately"
    );
}

#[test]
fn osc133_c_transitions_to_busy_immediately() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-osc133-busy";
    state.session_maps.shell_states.insert(
        session_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_IDLE),
    );
    state
        .session_maps
        .shell_state_since_ms
        .insert(session_id.to_string(), std::sync::atomic::AtomicU64::new(0));
    state
        .session_maps
        .has_osc133_integration
        .insert(session_id.to_string(), ());

    let mut proc = ChunkProcessor::new(None, None);
    proc.handle_osc133_event('C', "", session_id, &state);

    let current = state
        .session_maps
        .shell_states
        .get(session_id)
        .unwrap()
        .load(std::sync::atomic::Ordering::Acquire);
    assert_eq!(
        current, SHELL_BUSY,
        "OSC 133 C should transition to busy immediately"
    );
}

#[test]
fn osc133_a_emits_shell_state_event() {
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-osc133-emit";
    state.session_maps.shell_states.insert(
        session_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_BUSY),
    );
    state
        .session_maps
        .shell_state_since_ms
        .insert(session_id.to_string(), std::sync::atomic::AtomicU64::new(0));

    // Subscribe to event bus before transition
    let mut rx = state.event_bus.subscribe();

    let mut proc = ChunkProcessor::new(None, None);
    proc.handle_osc133_event('A', "", session_id, &state);

    // Check event_bus received a state change
    let evt = rx.try_recv();
    assert!(
        evt.is_ok(),
        "event_bus should have received a shell state event"
    );
    if let Ok(crate::state::AppEvent::PtyParsed {
        session_id: sid,
        parsed,
    }) = evt
    {
        assert_eq!(sid, session_id);
        assert_eq!(parsed["type"], "shell-state");
        assert_eq!(parsed["state"], "idle");
    } else {
        panic!("expected PtyParsed event with shell-state");
    }
}

#[test]
fn osc133_d_does_not_transition_alone() {
    // D means "command finished" but idle only happens when A arrives (prompt shown)
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-osc133-d";
    state.session_maps.shell_states.insert(
        session_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_BUSY),
    );
    state
        .session_maps
        .shell_state_since_ms
        .insert(session_id.to_string(), std::sync::atomic::AtomicU64::new(0));
    state
        .session_maps
        .has_osc133_integration
        .insert(session_id.to_string(), ());

    let mut proc = ChunkProcessor::new(None, None);
    proc.handle_osc133_event('D', "0", session_id, &state);

    let current = state
        .session_maps
        .shell_states
        .get(session_id)
        .unwrap()
        .load(std::sync::atomic::Ordering::Acquire);
    assert_eq!(
        current, SHELL_BUSY,
        "OSC 133 D alone should NOT transition — wait for A"
    );
}

#[test]
fn osc133_d_without_c_records_no_outcome() {
    // A 'D' (command finished) without a preceding 'C' (command started) —
    // e.g. Enter on an empty prompt — must NOT record a phantom outcome
    // (empty command, "unknown" error) that would pollute the knowledge
    // panel and the agent's injected prompt.
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-osc133-d-no-c";
    state
        .session_maps
        .has_osc133_integration
        .insert(session_id.to_string(), ());

    let mut proc = ChunkProcessor::new(None, None);
    proc.handle_osc133_event('D', "1", session_id, &state);

    let recorded = state
        .ai
        .session_knowledge
        .get(session_id)
        .map(|k| k.lock().commands.len())
        .unwrap_or(0);
    assert_eq!(recorded, 0, "D without C must not record an outcome");
}

#[test]
fn osc133_c_then_d_records_outcome() {
    // Regression guard: the normal path still records — C captures the
    // command start, D finalizes the outcome.
    let state = crate::state::tests_support::make_test_app_state();
    let session_id = "test-osc133-c-then-d";
    state.session_maps.shell_states.insert(
        session_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_IDLE),
    );
    state
        .session_maps
        .shell_state_since_ms
        .insert(session_id.to_string(), std::sync::atomic::AtomicU64::new(0));
    state
        .session_maps
        .has_osc133_integration
        .insert(session_id.to_string(), ());

    let mut proc = ChunkProcessor::new(None, None);
    proc.handle_osc133_event('C', "", session_id, &state);
    proc.handle_osc133_event('D', "0", session_id, &state);

    let recorded = state
        .ai
        .session_knowledge
        .get(session_id)
        .map(|k| k.lock().commands.len())
        .unwrap_or(0);
    assert_eq!(recorded, 1, "C→D should record exactly one outcome");
}

// --- is_cc_tool_call_header tests ---

#[test]
fn cc_tool_call_bash() {
    assert!(is_cc_tool_call_header(
        "⏺ Bash(curl -s 'http://localhost:9876/logs')"
    ));
}

#[test]
fn cc_tool_call_read() {
    assert!(is_cc_tool_call_header("⏺ Read(src/foo.rs)"));
}

#[test]
fn cc_tool_call_edit() {
    assert!(is_cc_tool_call_header("⏺ Edit(file_path=/tmp/a.rs)"));
}

#[test]
fn cc_tool_call_mcp() {
    assert!(is_cc_tool_call_header(
        "⏺ mcp__tuicommander__ui(action=tab)"
    ));
}

#[test]
fn cc_tool_call_with_leading_whitespace() {
    assert!(is_cc_tool_call_header("  ⏺ Bash(ls)"));
}

#[test]
fn cc_prose_not_tool_call() {
    assert!(!is_cc_tool_call_header("⏺ Boss, ci sono molti tipi di OSC"));
}

#[test]
fn cc_prose_with_paren_not_tool_call() {
    assert!(!is_cc_tool_call_header(
        "⏺ Nessun errore (tutti i log puliti)"
    ));
}

#[test]
fn cc_calling_collapsed_not_tool_call() {
    assert!(!is_cc_tool_call_header(
        "⏺ Calling tuicommander 2 times… (ctrl+o to expand)"
    ));
}

#[test]
fn cc_mission_control_not_tool_call() {
    assert!(!is_cc_tool_call_header(
        "⏺ Mission Control: opened in TUIC tab"
    ));
}

#[test]
fn cc_empty_after_bullet_not_tool_call() {
    assert!(!is_cc_tool_call_header("⏺ "));
    assert!(!is_cc_tool_call_header("⏺"));
}

#[test]
fn cc_no_bullet_not_tool_call() {
    assert!(!is_cc_tool_call_header("Bash(ls)"));
    assert!(!is_cc_tool_call_header("plain text"));
}

// ── process_kitty_actions ───────────────────────────────────────

#[test]
fn process_kitty_actions_empty_is_noop() {
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "kitty-empty";
    process_kitty_actions(&[], sid, &state);
    assert!(
        !state.session_maps.kitty_states.contains_key(sid),
        "empty action list must not allocate per-session kitty state"
    );
}

#[test]
fn process_kitty_actions_push_pop_query_tracks_flag_stack() {
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "kitty-stack";

    // Two pushes: current flags follow the top of the stack.
    process_kitty_actions(&[KittyAction::Push(1), KittyAction::Push(5)], sid, &state);
    assert_eq!(
        state
            .session_maps
            .kitty_states
            .get(sid)
            .unwrap()
            .lock()
            .current_flags(),
        5
    );

    // Pop returns to the first pushed value.
    process_kitty_actions(&[KittyAction::Pop], sid, &state);
    assert_eq!(
        state
            .session_maps
            .kitty_states
            .get(sid)
            .unwrap()
            .lock()
            .current_flags(),
        1
    );

    // Query with no live PTY session must not panic (writer path is skipped)
    // and must leave the flag stack untouched.
    process_kitty_actions(&[KittyAction::Query], sid, &state);
    assert_eq!(
        state
            .session_maps
            .kitty_states
            .get(sid)
            .unwrap()
            .lock()
            .current_flags(),
        1
    );
}

// ── cleanup_session ─────────────────────────────────────────────

#[test]
fn cleanup_session_clears_transient_session_maps() {
    use std::sync::atomic::{AtomicU8, AtomicU64};
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "cleanup-maps";
    state
        .session_maps
        .output_buffers
        .insert(sid.to_string(), Mutex::new(OutputRingBuffer::new(4096)));
    state.grid.vt_log_buffers.insert(
        sid.to_string(),
        Mutex::new(crate::state::VtLogBuffer::new(24, 80, 1000)),
    );
    state
        .session_maps
        .kitty_states
        .insert(sid.to_string(), Mutex::new(KittyKeyboardState::new()));
    state
        .session_maps
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(SHELL_IDLE));
    state
        .session_maps
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(0));
    state
        .session_maps
        .term_aliases
        .insert(sid.to_string(), "alias".to_string());
    state.session_maps.exit_codes.insert(sid.to_string(), 0);

    cleanup_session(sid, &state);

    assert!(!state.session_maps.output_buffers.contains_key(sid));
    assert!(!state.grid.vt_log_buffers.contains_key(sid));
    assert!(!state.session_maps.kitty_states.contains_key(sid));
    assert!(!state.session_maps.shell_states.contains_key(sid));
    assert!(!state.session_maps.last_output_ms.contains_key(sid));
    assert!(!state.session_maps.term_aliases.contains_key(sid));
    assert!(!state.session_maps.exit_codes.contains_key(sid));
}

// ── ChunkProcessor::check_pending_planfiles ─────────────────────

#[test]
fn check_pending_planfiles_empty_is_noop() {
    let state = crate::state::tests_support::make_test_app_state();
    let mut cp = ChunkProcessor::new(None, None);
    cp.check_pending_planfiles("sid", &state);
    assert!(cp.pending_planfiles.is_empty());
}

#[test]
fn check_pending_planfiles_drops_expired_and_tombstones() {
    use std::time::{Duration, Instant};
    let state = crate::state::tests_support::make_test_app_state();
    let mut cp = ChunkProcessor::new(None, None);
    let missing = "/no/such/planfile/expired.md".to_string();
    // Deadline in the (immediate) past: the internal `Instant::now()` runs
    // after the sleep, so `now > deadline` holds.
    cp.pending_planfiles.push((missing.clone(), Instant::now()));
    std::thread::sleep(Duration::from_millis(2));

    cp.check_pending_planfiles("sid", &state);

    assert!(
        cp.pending_planfiles.is_empty(),
        "an expired retry must be dropped from the queue"
    );
    assert!(
        cp.gaveup_planfiles.contains(&missing),
        "a dropped retry must be tombstoned so it is not re-queued forever"
    );
}

#[test]
fn check_pending_planfiles_keeps_missing_file_until_deadline() {
    use std::time::{Duration, Instant};
    let state = crate::state::tests_support::make_test_app_state();
    let mut cp = ChunkProcessor::new(None, None);
    let missing = "/no/such/planfile/pending.md".to_string();
    cp.pending_planfiles
        .push((missing, Instant::now() + Duration::from_secs(30)));

    cp.check_pending_planfiles("sid", &state);

    assert_eq!(
        cp.pending_planfiles.len(),
        1,
        "a not-yet-existing file with a live deadline stays queued"
    );
}

#[test]
fn check_pending_planfiles_emits_when_file_appears() {
    use std::time::{Duration, Instant};
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "planfile-emit";
    let mut cp = ChunkProcessor::new(None, None);

    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let file = dir.path().join("plan.md");
    std::fs::write(&file, "# plan").expect("write plan file");
    let path = file.to_string_lossy().to_string();

    cp.pending_planfiles
        .push((path.clone(), Instant::now() + Duration::from_secs(30)));
    let mut rx = state.event_bus.subscribe();

    cp.check_pending_planfiles(sid, &state);

    assert!(
        cp.pending_planfiles.is_empty(),
        "a resolved file must leave the retry queue"
    );
    assert!(
        cp.emitted_planfiles.contains(&path),
        "a resolved path must be recorded as emitted"
    );
    let mut got = false;
    while let Ok(evt) = rx.try_recv() {
        if let crate::state::AppEvent::PtyParsed { parsed, .. } = evt
            && parsed.get("type").and_then(|t| t.as_str()) == Some("plan-file")
            && parsed.get("path").and_then(|p| p.as_str()) == Some(path.as_str())
        {
            got = true;
        }
    }
    assert!(
        got,
        "a resolved plan file must emit a plan-file PtyParsed event"
    );
}

// ── wake_session ────────────────────────────────────────────────

#[cfg(unix)]
#[test]
fn wake_session_returns_false_when_not_in_standby() {
    let state = crate::state::tests_support::make_test_app_state();
    assert_eq!(wake_session(&state, "not-parked"), Ok(false));
}

#[cfg(unix)]
#[test]
fn wake_session_errors_and_consumes_entry_when_session_missing() {
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "parked-but-gone";
    state
        .session_maps
        .standby_sessions
        .insert(sid.to_string(), 0);

    let res = wake_session(&state, sid);

    assert!(
        res.is_err(),
        "a standby entry without a live session must error"
    );
    assert!(res.unwrap_err().contains("Session not found"));
    assert!(
        !state.session_maps.standby_sessions.contains_key(sid),
        "the standby entry is consumed even on the error path"
    );
}

#[cfg(not(windows))]
#[test]
fn query_process_stats_empty_input_is_empty() {
    assert!(query_process_stats(&[]).is_empty());
}

// --- Chunk-path characterization ---------------------------------------
//
// The chunk path is a refactor target: cutoff placement, snapshot reuse,
// lock coalescing and clone removal must all be observationally silent.
// These tests pin what a session actually sees — the emitted `PtyParsed`
// payloads, the shell state and the awaiting badge — for a real capture
// replayed through the real `process_chunk`, so a regression shows up as a
// diff in the recorded trace rather than as a subtle live-session bug.

#[test]
fn captured_codex_request_user_input_reaches_choice_prompt() {
    let bytes = agent_prompt_fixture("codex-request-user-input-20260929.tcap");
    let capture = crate::pty_capture::decode_capture(&bytes).expect("real Codex capture");
    let (rows, cols) = capture.geometry.expect("capture records terminal geometry");
    let sid = "codex-question-capture";
    let (state, silence) = chunk_trace_state(sid);
    state.grid.vt_log_buffers.insert(
        sid.to_string(),
        Mutex::new(crate::state::VtLogBuffer::new(rows, cols, 2000)),
    );
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .agent_type = Some("codex".into());
    let mut rx = state.event_bus.subscribe();
    let mut processor = ChunkProcessor::new(None, None);
    let mut utf8 = Utf8ReadBuffer::new();
    let mut escape = EscapeAwareBuffer::new();
    let mut prompts = Vec::new();

    for record in capture.records {
        if record.direction != crate::pty_capture::CaptureDirection::Output {
            continue;
        }
        let data = escape.push(&utf8.push(&record.data));
        let (clean, _) = crate::state::strip_kitty_sequences(&data);
        processor.process_chunk(&clean, &silence, sid, &state);
        while let Ok(event) = rx.try_recv() {
            if let crate::state::AppEvent::PtyParsed { parsed, .. } = event
                && parsed.get("type").and_then(serde_json::Value::as_str) == Some("choice-prompt")
            {
                prompts.push(parsed);
            }
        }
    }

    assert!(
        prompts.iter().any(|prompt| {
            prompt.get("title").and_then(serde_json::Value::as_str)
                == Some("Boss, scegli rosso o blu?")
                && prompt["options"].as_array().is_some_and(|options| {
                    options
                        .iter()
                        .any(|option| option["key"] == "2" && option["label"] == "Blu")
                })
        }),
        "captured Codex question must reach the mobile choice-prompt state"
    );
}

/// Everything a refactor of `process_chunk` is allowed to leave unchanged.
#[derive(Debug, PartialEq)]
struct ChunkTrace {
    /// One entry per chunk: the parsed-event payloads it emitted, in order.
    per_chunk_events: Vec<Vec<serde_json::Value>>,
    /// Shell state after the last chunk.
    shell_state: Option<u8>,
    /// Non-event side effects the chunk path writes directly.
    last_question_text: Option<String>,
    last_choice_prompt_sig: Option<String>,
    terminal_mode_fullscreen: bool,
    /// Bytes the ring buffer accumulated (proves the passthrough is intact).
    ring_len: usize,
}

/// Build the exact set of per-session maps `process_chunk` touches.
fn chunk_trace_state(sid: &str) -> (Arc<AppState>, Arc<Mutex<SilenceState>>) {
    use crate::state::VtLogBuffer;
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let silence = Arc::new(Mutex::new(SilenceState::new()));
    state
        .session_maps
        .silence_states
        .insert(sid.to_string(), silence.clone());
    state.session_maps.shell_states.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_NULL),
    );
    state
        .grid
        .vt_log_buffers
        .insert(sid.to_string(), Mutex::new(VtLogBuffer::new(41, 128, 2000)));
    state.session_maps.output_buffers.insert(
        sid.to_string(),
        Mutex::new(OutputRingBuffer::new(OUTPUT_RING_BUFFER_CAPACITY)),
    );
    state
        .session_maps
        .last_output_ms
        .insert(sid.to_string(), AtomicU64::new(0));
    state
        .session_maps
        .session_states
        .insert(sid.to_string(), crate::state::SessionState::default());
    (state, silence)
}

/// Replay a capture's OUTPUT records through the production `process_chunk`,
/// preserving the original chunk boundaries, and record everything observable.
fn trace_capture_through_process_chunk(bytes: &[u8], agent_type: Option<&str>) -> ChunkTrace {
    let sid = "chunk-trace";
    let (state, silence) = chunk_trace_state(sid);
    if let Some(agent) = agent_type
        && let Some(mut entry) = state.session_maps.session_states.get_mut(sid)
    {
        entry.agent_type = Some(agent.to_string());
    }
    let mut rx = state.event_bus.subscribe();
    let mut cp = ChunkProcessor::new(None, None);
    let mut utf8_buf = Utf8ReadBuffer::new();
    let mut esc_buf = EscapeAwareBuffer::new();
    let mut per_chunk_events = Vec::new();

    for record in crate::pty_capture::decode(bytes).expect("valid capture") {
        if record.direction != crate::pty_capture::CaptureDirection::Output {
            continue;
        }
        let utf8_data = utf8_buf.push(&record.data);
        let esc_data = esc_buf.push(&utf8_data);
        let (kitty_clean, _actions) = crate::state::strip_kitty_sequences(&esc_data);
        let _ = cp.process_chunk(&kitty_clean, &silence, sid, state.as_ref());
        // Drain per chunk: the bus holds 256 messages and a long capture
        // would otherwise lag and silently drop the evidence.
        let mut this_chunk = Vec::new();
        while let Ok(evt) = rx.try_recv() {
            if let crate::state::AppEvent::PtyParsed { parsed, .. } = evt {
                this_chunk.push((*parsed).clone());
            }
        }
        per_chunk_events.push(this_chunk);
    }

    ChunkTrace {
        per_chunk_events,
        shell_state: state
            .session_maps
            .shell_states
            .get(sid)
            .map(|a| a.load(std::sync::atomic::Ordering::Acquire)),
        last_question_text: cp.last_question_text.clone(),
        last_choice_prompt_sig: cp.last_choice_prompt_sig.clone(),
        terminal_mode_fullscreen: cp.terminal_mode.is_fullscreen(),
        ring_len: state
            .session_maps
            .output_buffers
            .get(sid)
            .map(|r| r.lock().len())
            .unwrap_or(0),
    }
}

/// The trace is the refactor's contract. Replaying the same bytes twice
/// through two independent pipelines must produce the identical trace —
/// if this is flaky, every characterization assertion below is worthless.
#[test]
fn chunk_trace_is_deterministic_for_a_real_capture() {
    for fixture in [
        "grok-1.0.5-minimal-turn.tcap",
        "claude-quoted-ink-footer.tcap",
    ] {
        let bytes = agent_prompt_fixture(fixture);
        let a = trace_capture_through_process_chunk(&bytes, Some("claude"));
        let b = trace_capture_through_process_chunk(&bytes, Some("claude"));
        assert_eq!(a, b, "{fixture}: chunk trace must be reproducible");
        assert!(
            a.per_chunk_events.iter().any(|c| !c.is_empty()),
            "{fixture}: a capture that emits nothing cannot characterize anything"
        );
    }
}

/// The golden numbers below were recorded against the pre-refactor chunk
/// path (2026-09-05). They are deliberately concrete: a refactor that
/// changes WHICH chunk emits an event, or how many, breaks this.
#[test]
fn chunk_trace_matches_recorded_baseline() {
    for (fixture, agent) in [
        ("grok-1.0.5-minimal-turn.tcap", "grok"),
        ("claude-quoted-ink-footer.tcap", "claude"),
        ("claude-plan-picker.raw", "claude"),
        ("claude-generic-attention.raw", "claude"),
    ] {
        let bytes = agent_prompt_fixture(fixture);
        let trace = trace_capture_through_process_chunk(&bytes, Some(agent));
        let summary: Vec<(usize, Vec<String>)> = trace
            .per_chunk_events
            .iter()
            .enumerate()
            .filter(|(_, evts)| !evts.is_empty())
            .map(|(i, evts)| {
                (
                    i,
                    evts.iter()
                        .map(|e| {
                            e.get("type")
                                .and_then(|t| t.as_str())
                                .unwrap_or("?")
                                .to_string()
                        })
                        .collect(),
                )
            })
            .collect();
        let actual = format!(
            "{summary:?} shell={:?} q={:?} sig={:?} alt={} ring={}",
            trace.shell_state,
            trace.last_question_text,
            trace.last_choice_prompt_sig,
            trace.terminal_mode_fullscreen,
            trace.ring_len
        );
        let expected = match fixture {
            // The `intent` at 225 was ABSENT from the 2026-09-05 recording, and
            // its absence was the defect, not the baseline: this capture holds
            // `TUICommander v1.7.4 is connected. intent: Conto da 1 a 5.
            // (Count)` — the two markers the protocol forces into the same
            // first message, written as one sentence run. A column-0-only
            // anchor captured nothing at all from it. See
            // `the_ack_prefixed_intent_reaches_a_session` below for the text.
            "grok-1.0.5-minimal-turn.tcap" => {
                "[(0, [\"shell-state\"]), (225, [\"status-line\", \"intent\"])] \
                 shell=Some(1) q=None sig=None alt=false ring=28282"
            }
            "claude-quoted-ink-footer.tcap" => {
                "[(1, [\"shell-state\"]), (3, [\"shell-state\"]), (115, [\"shell-state\"]), \
                 (118, [\"shell-state\"])] shell=Some(1) q=None sig=None alt=false ring=1902"
            }
            "claude-plan-picker.raw" => {
                "[(0, [\"status-line\", \"shell-state\"])] \
                 shell=Some(1) q=None sig=None alt=false ring=8196"
            }
            "claude-generic-attention.raw" => "[] shell=Some(0) q=None sig=None alt=false ring=59",
            other => panic!("no recorded baseline for {other}"),
        };
        // The literals above wrap with `\` continuations; normalise the run of
        // spaces that produces before comparing.
        let normalise = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
        assert_eq!(
            normalise(&actual),
            normalise(expected),
            "{fixture}: the chunk path changed what a session observes"
        );
    }
}

/// The baseline above records event TYPES; a tab title is made of the text.
/// This replays the same recorded grok turn through the same real chunk path
/// and asserts what the session actually reads out of an ack-prefixed intent —
/// body and `(title)` split at the right place, with the ack sentence gone.
#[test]
fn the_ack_prefixed_intent_reaches_a_session() {
    let bytes = agent_prompt_fixture("grok-1.0.5-minimal-turn.tcap");
    let trace = trace_capture_through_process_chunk(&bytes, Some("grok"));
    let intent = trace
        .per_chunk_events
        .iter()
        .flatten()
        .find(|e| e.get("type").and_then(|t| t.as_str()) == Some("intent"))
        .expect("the recorded turn declares an intent");
    assert_eq!(
        intent.get("text").and_then(|t| t.as_str()),
        Some("Conto da 1 a 5."),
        "the ack sentence must not survive into the intent body"
    );
    assert_eq!(
        intent.get("title").and_then(|t| t.as_str()),
        Some("Count"),
        "the (title) is the tab name — losing it is the whole cost of the bug"
    );
}

/// Screens the fixtures do not contain: a choice dialog, an Ink question
/// footer, a status-line HUD BELOW the input box, and a slash menu. Each is
/// fed through the real `process_chunk`; the assertion is the set of event
/// types the session observes.
///
/// The HUD case is criterion 1's real subject: rows under the input-box
/// separator must never reach a parser, and moving the cutoff earlier must
/// not change that verdict in either direction.
#[test]
fn chunk_path_scenarios_emit_the_same_events() {
    // (name, screen bytes, slash_mode, expected event types in order)
    let scenarios: &[(&str, &str, bool, &[&str])] = &[
        (
            "choice dialog",
            "\x1b[2J\x1b[HDo you want to make this edit to CLAUDE.md?\r\n\
                 \x20\u{276f} 1. Yes\r\n\
                 \x20\x20 2. Yes, allow all edits\r\n\
                 \x20\x20 3. No\r\n",
            false,
            &["choice-prompt", "shell-state"],
        ),
        (
            "ink question footer",
            "\x1b[2J\x1b[HEnter to select \u{b7} \u{2191}/\u{2193} to navigate \u{b7} Esc to cancel\r\n",
            false,
            &["question", "shell-state"],
        ),
        (
            // A rate-limit line ABOVE the input box: real agent output, the
            // cutoff must keep it.
            "output above the input box",
            "\x1b[2J\x1b[HError: rate_limit_error\r\n\
                 \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\r\n\
                 \u{276f}\r\n\
                 \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\r\n\
                 [Opus 5] 5h: 0% | $15.48\r\n",
            false,
            &["rate-limit", "shell-state"],
        ),
        (
            // The SAME status line, with nothing above the box. Everything
            // that changed is chrome, so nothing may be parsed.
            "status line below the input box only",
            "\x1b[2J\x1b[H\r\n\
                 \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\r\n\
                 \u{276f}\r\n\
                 \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\r\n\
                 Error: rate_limit_error\r\n",
            false,
            &[],
        ),
        (
            "slash menu",
            "\x1b[2J\x1b[H\u{276f} /rev\r\n\
                 \x20 /review    Review a pull request\r\n\
                 \x20 /revert    Undo the last change\r\n",
            true,
            &["slash-menu"],
        ),
    ];

    let mut mismatches: Vec<String> = Vec::new();
    let mut observed: Vec<(&str, Vec<String>)> = Vec::new();
    for (name, screen, slash_on, expected) in scenarios {
        let sid = "chunk-scenario";
        let (state, silence) = chunk_trace_state(sid);
        if *slash_on {
            state
                .session_maps
                .slash_mode
                .insert(sid.to_string(), std::sync::atomic::AtomicBool::new(true));
        }
        settle_startup_grace(&silence);
        let mut rx = state.event_bus.subscribe();
        let mut cp = ChunkProcessor::new(None, None);
        cp.process_chunk(screen, &silence, sid, state.as_ref());
        let mut kinds: Vec<String> = Vec::new();
        while let Ok(evt) = rx.try_recv() {
            if let crate::state::AppEvent::PtyParsed { parsed, .. } = evt {
                kinds.push(
                    parsed
                        .get("type")
                        .and_then(|t| t.as_str())
                        .unwrap_or("?")
                        .to_string(),
                );
            }
        }
        observed.push((*name, kinds.clone()));
        if kinds != *expected {
            mismatches.push(format!("{name:?}: expected {expected:?}, got {kinds:?}"));
        }
    }
    assert!(
        mismatches.is_empty(),
        "chunk-path scenarios changed:\n  {}\nfull trace: {observed:?}",
        mismatches.join("\n  ")
    );
}

/// `find_chrome_cutoff` fails OPEN: no anchor means NO trim. A screen with
/// no input box must therefore still deliver every changed row to the
/// parsers — the failure mode of moving the cutoff earlier is turning that
/// "parse everything" into "parse nothing".
#[test]
fn no_chrome_anchor_still_parses_every_row() {
    let sid = "chunk-no-cutoff";
    let (state, silence) = chunk_trace_state(sid);
    let mut rx = state.event_bus.subscribe();
    let mut cp = ChunkProcessor::new(None, None);

    // Plain scrolling output: no separator, no prompt, no input box at all.
    let screen = "\x1b[2J\x1b[HError: rate_limit_error\r\n";
    let refs: Vec<String> = {
        use crate::state::VtLogBuffer;
        let mut vt = VtLogBuffer::new(41, 128, 2000);
        vt.process(screen.as_bytes());
        vt.screen_rows()
    };
    let borrowed: Vec<&str> = refs.iter().map(String::as_str).collect();
    assert!(
        crate::chrome::find_chrome_cutoff(&borrowed).is_none(),
        "precondition: this screen has no chrome anchor"
    );

    settle_startup_grace(&silence);
    cp.process_chunk(screen, &silence, sid, state.as_ref());
    let mut kinds = Vec::new();
    while let Ok(evt) = rx.try_recv() {
        if let crate::state::AppEvent::PtyParsed { parsed, .. } = evt {
            kinds.push(
                parsed
                    .get("type")
                    .and_then(|t| t.as_str())
                    .unwrap_or("?")
                    .to_string(),
            );
        }
    }
    assert!(
        kinds.iter().any(|k| k == "rate-limit"),
        "a cutoff of None must mean parse everything, got {kinds:?}"
    );
}

/// Repeatable CPU measurement for the chunk path. Ignored by default —
/// run with `cargo test --lib -- --ignored --nocapture bench_chunk_path`.
///
/// Per-session setup (`make_test_app_state`) costs milliseconds and would
/// swamp the measurement, so it is hoisted out of the timed region: the
/// loop reuses one session and replays the capture into it, which is also
/// what a long-lived agent tab actually does.
#[test]
#[ignore = "benchmark: run explicitly with --nocapture"]
fn bench_chunk_path_replay() {
    const ITERATIONS: u32 = 200;
    for (fixture, agent) in [
        ("grok-1.0.5-minimal-turn.tcap", "grok"),
        ("claude-plan-picker.raw", "claude"),
        ("claude-quoted-ink-footer.tcap", "claude"),
    ] {
        let bytes = agent_prompt_fixture(fixture);
        let chunks: Vec<Vec<u8>> = crate::pty_capture::decode(&bytes)
            .expect("valid capture")
            .into_iter()
            .filter(|r| r.direction == crate::pty_capture::CaptureDirection::Output)
            .map(|r| r.data)
            .collect();
        let total_bytes: usize = chunks.iter().map(Vec::len).sum();

        let sid = "chunk-bench";
        let (state, silence) = chunk_trace_state(sid);
        if let Some(mut entry) = state.session_maps.session_states.get_mut(sid) {
            entry.agent_type = Some(agent.to_string());
        }
        let mut rx = state.event_bus.subscribe();
        let mut cp = ChunkProcessor::new(None, None);
        let mut utf8_buf = Utf8ReadBuffer::new();
        let mut esc_buf = EscapeAwareBuffer::new();

        let mut run = |cp: &mut ChunkProcessor,
                       utf8_buf: &mut Utf8ReadBuffer,
                       esc_buf: &mut EscapeAwareBuffer| {
            for chunk in &chunks {
                let utf8_data = utf8_buf.push(chunk);
                let esc_data = esc_buf.push(&utf8_data);
                let (kitty_clean, _) = crate::state::strip_kitty_sequences(&esc_data);
                let _ = cp.process_chunk(&kitty_clean, &silence, sid, state.as_ref());
            }
            while rx.try_recv().is_ok() {}
        };

        // Warm the lazy_static regexes and the grid allocations.
        for _ in 0..3 {
            run(&mut cp, &mut utf8_buf, &mut esc_buf);
        }
        // Report the MINIMUM, not the mean: this machine runs many parallel
        // builds, and a mean is dominated by scheduler interference. The
        // fastest observed replay is the one least contaminated by it.
        let mut best = std::time::Duration::MAX;
        let mut total = std::time::Duration::ZERO;
        for _ in 0..ITERATIONS {
            let start = std::time::Instant::now();
            run(&mut cp, &mut utf8_buf, &mut esc_buf);
            let elapsed = start.elapsed();
            best = best.min(elapsed);
            total += elapsed;
        }
        eprintln!(
            "BENCH {fixture}: {ITERATIONS} x {} chunks / {total_bytes} B \
                 = min {:.3} ms, mean {:.3} ms per replay ({:.3} us per chunk at min)",
            chunks.len(),
            best.as_secs_f64() * 1000.0,
            total.as_secs_f64() * 1000.0 / f64::from(ITERATIONS),
            best.as_secs_f64() * 1e6 / chunks.len() as f64,
        );
    }
}

#[cfg(not(windows))]
#[test]
fn collect_process_stats_includes_tuicommander_itself() {
    let state = crate::state::tests_support::make_test_app_state();
    let stats = collect_process_stats(&state);
    let own = std::process::id();
    assert!(
        stats
            .iter()
            .any(|s| s.session_id.is_none() && s.pid == own && s.name == "TUICommander"),
        "TUIC's own process must appear with no session id"
    );
}

#[cfg(test)]
mod grid_subscriber_tests {
    use super::*;

    /// F28. The frame ticker used to take the vt lock and run a full
    /// `serialize_dirty_rows` on every dirty tick, and only then discover in
    /// `send_grid_frame` that there was no channel and no watch receiver to hand
    /// the bytes to. A session whose tab is closed but whose PTY still runs — an
    /// agent working in an unmounted pane — paid a whole encode per tick for
    /// nothing. This is the check that now runs first, so it has to agree
    /// exactly with what `send_grid_frame` treats as a consumer.

    #[test]
    fn nobody_is_subscribed_to_a_session_with_neither_channel_nor_watch() {
        let state = crate::state::tests_support::make_test_app_state();
        assert!(!grid_has_subscriber(&state, "s1"));
    }

    #[test]
    fn a_watch_whose_receivers_have_all_gone_is_not_a_subscriber() {
        let state = crate::state::tests_support::make_test_app_state();
        // The sender outlives its clients: a browser tab that closed leaves the
        // entry behind. Counting the entry rather than its receivers would keep
        // every such session serializing forever.
        state
            .grid
            .watch
            .insert("s1".to_string(), crate::grid_watch::new_grid_watch());
        assert!(!grid_has_subscriber(&state, "s1"));
    }

    #[test]
    fn a_live_watch_receiver_is_a_subscriber() {
        let state = crate::state::tests_support::make_test_app_state();
        let tx = crate::grid_watch::new_grid_watch();
        let rx = tx.subscribe();
        state.grid.watch.insert("s1".to_string(), tx);

        assert!(grid_has_subscriber(&state, "s1"));

        drop(rx);
        assert!(
            !grid_has_subscriber(&state, "s1"),
            "the last receiver going away must close the session again"
        );
    }

    #[test]
    fn one_session_having_a_subscriber_says_nothing_about_another() {
        let state = crate::state::tests_support::make_test_app_state();
        let tx = crate::grid_watch::new_grid_watch();
        let _rx = tx.subscribe();
        state.grid.watch.insert("watched".to_string(), tx);

        assert!(grid_has_subscriber(&state, "watched"));
        assert!(!grid_has_subscriber(&state, "unwatched"));
    }
}

#[cfg(test)]
mod vt_read_tests {
    use super::*;

    // Grid reads take the VT mutex, and the PTY reader holds that same mutex
    // through a whole `serialize_dirty_rows`. Waiting for it inline in the IPC
    // handler — the macOS main thread — freezes the WebView for the length of
    // someone else's serialize. `vt_read` is the one door they all go through.

    #[tokio::test]
    async fn a_read_against_a_live_session_returns_the_buffers_answer() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        state.grid.vt_log_buffers.insert(
            "s1".to_string(),
            parking_lot::Mutex::new(crate::state::VtLogBuffer::new(24, 80, 1000)),
        );

        let lines = vt_read(&state, "s1".to_string(), |vt| vt.grid_screen_lines())
            .await
            .unwrap();

        assert_eq!(lines, 24);
    }

    // A tab can be closed while a hover or a selection read is in flight. That
    // is not an error to surface — the caller gets the empty answer it would
    // have got from an empty grid.
    #[tokio::test]
    async fn a_read_against_a_session_that_is_gone_is_the_default() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());

        let text: String = vt_read(&state, "nope".to_string(), |vt| vt.grid_get_cursor_line())
            .await
            .unwrap();

        assert!(text.is_empty());
    }

    // The lock is taken inside the closure, on the pool thread. If it were taken
    // before the hop, the caller would wait for it on the thread it is trying to
    // keep free — so two reads must be able to overlap without deadlocking.
    #[tokio::test]
    async fn two_reads_on_the_same_session_do_not_deadlock() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        state.grid.vt_log_buffers.insert(
            "s1".to_string(),
            parking_lot::Mutex::new(crate::state::VtLogBuffer::new(24, 80, 1000)),
        );

        let (a, b) = tokio::join!(
            vt_read(&state, "s1".to_string(), |vt| vt.grid_total_lines()),
            vt_read(&state, "s1".to_string(), |vt| vt.grid_total_lines()),
        );

        assert_eq!(a.unwrap(), b.unwrap());
    }

    // The point of the whole change: the closure — and therefore the wait for
    // the vt mutex — must not run on the thread that called the command. This
    // is the assertion that fails if someone "simplifies" the helper back into
    // a direct lock.
    #[tokio::test]
    async fn the_read_does_not_run_on_the_calling_thread() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        state.grid.vt_log_buffers.insert(
            "s1".to_string(),
            parking_lot::Mutex::new(crate::state::VtLogBuffer::new(24, 80, 1000)),
        );
        let caller = std::thread::current().id();

        let worker = vt_try_read(&state, "s1".to_string(), |_| std::thread::current().id())
            .await
            .unwrap();

        assert_ne!(worker, Some(caller));
        assert!(worker.is_some());
    }

    // `vt_try_read` keeps the distinction the HTTP routes answer 404 with;
    // `vt_read` is the same call with the miss folded into the default.
    #[tokio::test]
    async fn a_missing_session_is_none_rather_than_an_error() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());

        let seen = vt_try_read(&state, "nope".to_string(), |vt| vt.grid_screen_lines())
            .await
            .unwrap();

        assert_eq!(seen, None);
    }
}

#[cfg(all(test, unix))]
mod standby_tests {
    use super::*;

    /// timeout=0 must wake ALL parked sessions. Entries with no live session
    /// (the "killed between listing and wake" case) must not panic — each is
    /// removed from the map before wake_session errors on the missing session.
    #[test]
    fn wake_all_standby_clears_every_parked_session() {
        let state = crate::state::tests_support::make_test_app_state();
        state
            .session_maps
            .standby_sessions
            .insert("gone-1".to_string(), 111);
        state
            .session_maps
            .standby_sessions
            .insert("gone-2".to_string(), 222);
        state
            .session_maps
            .standby_sessions
            .insert("gone-3".to_string(), 333);

        let attempted = wake_all_standby(&state);

        assert_eq!(attempted, 3, "wake attempted for every parked session");
        assert!(
            state.session_maps.standby_sessions.is_empty(),
            "standby map must be empty after wake-all even when the sessions are gone"
        );
    }

    /// Empty standby map is a no-op — no panic, nothing to wake.
    #[test]
    fn wake_all_standby_empty_is_noop() {
        let state = crate::state::tests_support::make_test_app_state();
        assert_eq!(wake_all_standby(&state), 0);
        assert!(state.session_maps.standby_sessions.is_empty());
    }

    /// After a timeout=0 wake-all, standby can re-arm normally when the user
    /// sets a positive timeout again — wake_all_standby sets no persistent
    /// "disabled" flag, it only clears the current standby set.
    #[test]
    fn wake_all_standby_leaves_map_ready_to_rearm() {
        let state = crate::state::tests_support::make_test_app_state();
        state
            .session_maps
            .standby_sessions
            .insert("gone-1".to_string(), 111);
        wake_all_standby(&state);
        assert!(state.session_maps.standby_sessions.is_empty());

        // Re-arming (as the checker would on the next tick with timeout>0) works.
        state
            .session_maps
            .standby_sessions
            .insert("re-armed".to_string(), 444);
        assert_eq!(state.session_maps.standby_sessions.len(), 1);
    }
}

#[cfg(test)]
mod normalize_path_tests {
    use super::normalize_path;
    use std::path::Path;

    #[test]
    fn resolves_parent_segments() {
        let p = normalize_path(Path::new("/a/b/../../c/d"));
        assert_eq!(p, Path::new("/c/d"));
    }

    #[test]
    fn resolves_worktree_relative_plan() {
        let p = normalize_path(Path::new(
            "/home/user/repo__wt/feat/../../repo/plans/foo.md",
        ));
        assert_eq!(p, Path::new("/home/user/repo/plans/foo.md"));
    }

    #[test]
    fn strips_dot_segments() {
        let p = normalize_path(Path::new("/a/./b/./c"));
        assert_eq!(p, Path::new("/a/b/c"));
    }

    #[test]
    fn preserves_clean_path() {
        let p = normalize_path(Path::new("/home/user/plans/bar.md"));
        assert_eq!(p, Path::new("/home/user/plans/bar.md"));
    }
}

#[cfg(test)]
mod grid_delivery_tests {
    use super::*;
    use crate::grid_gate::GridGate;
    use crate::grid_watch::new_grid_watch;

    // --- Frame ordering (670-b9a2) ---
    //
    // Every producer serializes under the vt lock and calls `send_grid_frame`
    // after releasing it, so two of them can reach the transport in the opposite
    // order. The frames are DELTAS whose damage was consumed when they were cut,
    // so the older one carries rows the newer one does not have: painting it last
    // reverts those rows and nothing ever sends them again. The resize path makes
    // it visible — it cuts a FULL frame, and a full frame landing after a delta is
    // the "blank after zoom" the resize flush exists to prevent.
    //
    // These tests inject the reordering directly rather than racing two threads
    // for it: a race that happens to come out in order proves nothing, and one
    // that comes out reversed proves it only on the run where it did.

    /// A session as the spawn paths leave it: a vt buffer, a grid watch and the
    /// ticker's dirty flag. Returns a live watch receiver — without one the watch
    /// has no subscribers and `send_grid_frame` hands it nothing.
    fn grid_session(
        state: &Arc<AppState>,
        session_id: &str,
    ) -> tokio::sync::watch::Receiver<crate::grid_watch::GridWatchFrame> {
        state.grid.vt_log_buffers.insert(
            session_id.to_string(),
            Mutex::new(crate::state::VtLogBuffer::new(24, 80, 1000)),
        );
        state
            .grid
            .frame_dirty
            .insert(session_id.to_string(), Arc::new(AtomicBool::new(false)));
        let tx = new_grid_watch();
        let rx = tx.subscribe();
        state.grid.watch.insert(session_id.to_string(), tx);
        rx
    }

    /// Feed the grid and cut the frame that carries what just changed, exactly as
    /// a producer does inside its own vt critical section.
    fn cut_frame(
        state: &Arc<AppState>,
        session_id: &str,
        text: &str,
    ) -> crate::grid_gate::GridFrame {
        let vt = state
            .grid
            .vt_log_buffers
            .get(session_id)
            .expect("session exists");
        let mut vt = vt.lock();
        vt.process(text.as_bytes());
        vt.serialize_dirty_rows()
    }

    /// Rows a frame carries, off the `row_count` header field.
    fn row_count(frame: &[u8]) -> u16 {
        u16::from_le_bytes([frame[0], frame[1]])
    }

    #[test]
    fn a_frame_that_lost_the_ordering_race_does_not_repaint_the_newer_screen() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let rx = grid_session(&state, "reorder");

        // Two producers, cut in this order under the vt lock.
        let older = cut_frame(&state, "reorder", "first\r\n");
        let newer = cut_frame(&state, "reorder", "second\r\n");
        assert!(!older.is_empty() && !newer.is_empty());

        // Both released the lock before sending, and they arrive reversed.
        send_grid_frame(&state, "reorder", newer.clone());
        send_grid_frame(&state, "reorder", older);

        assert_eq!(
            rx.borrow().frame,
            newer.bytes,
            "a frame cut earlier painted over the newer screen"
        );
    }

    #[test]
    fn the_rows_a_dropped_frame_carried_come_back_as_a_full_repaint() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let _rx = grid_session(&state, "repaint");

        let older = cut_frame(&state, "repaint", "first\r\n");
        let newer = cut_frame(&state, "repaint", "second\r\n");
        send_grid_frame(&state, "repaint", newer);
        send_grid_frame(&state, "repaint", older);

        // Dropping the loser silently is not an option: both frames consumed the
        // damage that produced them, so the rows in the dropped one reach nobody
        // unless the grid is damaged again.
        assert!(
            state
                .grid
                .frame_dirty
                .get("repaint")
                .expect("flag exists")
                .load(Ordering::Relaxed),
            "the repair has to be armed or the dropped rows are lost for good"
        );
        let repaint = {
            let vt = state
                .grid
                .vt_log_buffers
                .get("repaint")
                .expect("session exists");
            let mut vt = vt.lock();
            vt.serialize_dirty_rows()
        };
        assert_eq!(
            row_count(&repaint.bytes),
            24,
            "the repair must be a whole screen — a delta cannot name rows nobody tracked"
        );
    }

    #[test]
    fn frames_arriving_in_the_order_they_were_cut_are_all_delivered() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mut rx = grid_session(&state, "in-order");

        let first = cut_frame(&state, "in-order", "first\r\n");
        send_grid_frame(&state, "in-order", first.clone());
        assert_eq!(rx.borrow_and_update().frame, first.bytes);

        let second = cut_frame(&state, "in-order", "second\r\n");
        send_grid_frame(&state, "in-order", second.clone());
        assert_eq!(rx.borrow_and_update().frame, second.bytes);

        // The ordering check must not cost a repaint on the path every frame
        // takes: only a genuine reversal may arm one.
        assert!(
            !state
                .grid
                .frame_dirty
                .get("in-order")
                .expect("flag exists")
                .load(Ordering::Relaxed),
            "the ordinary path armed a full repaint it does not need"
        );
    }

    // --- A stalled WebView must not starve the browser (670-b9a2) ---
    //
    // `GridGate` belongs to the desktop IPC channel: it counts frames sent
    // against frames the WebView reported painting. The ticker used to check it
    // before serializing anything, so a WebView blocked on its own main thread
    // stopped the frames going to browser/PWA clients — a different transport,
    // with its own flow control, that had not fallen behind at all.

    /// A reader that keeps the session alive until the test releases it, then
    /// reports EOF so the reader and ticker threads shut down normally.
    struct StopOnFlag(Arc<AtomicBool>);

    impl Read for StopOnFlag {
        fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
            while !self.0.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            Ok(0)
        }
    }

    /// Start the frame ticker for a session that already has a vt buffer and a
    /// grid watch. Returns the stop flag — set it to tear the threads down.
    fn start_ticker(state: &Arc<AppState>, session_id: &str) -> Arc<AtomicBool> {
        let stop = Arc::new(AtomicBool::new(false));
        spawn_reader_thread(
            Box::new(StopOnFlag(stop.clone())),
            Arc::new(AtomicBool::new(false)),
            session_id.to_string(),
            state.clone(),
            None,
        );
        stop
    }

    /// Outer bound only: it answers "did the ticker ever publish", nothing about
    /// how fast. The ticker runs on a 16 ms interval, so any real delivery is
    /// three orders of magnitude inside this; a timeout means no frame was ever
    /// serialized, which is the defect itself.
    const TICKER_LIVENESS_BOUND: std::time::Duration = std::time::Duration::from_secs(10);

    /// Keep the grid changing and the ticker armed, the way a live PTY reader
    /// does. Returns a handle that stops the feed when the test drops it.
    fn feed_continuously(state: &Arc<AppState>, session_id: &str) -> Arc<AtomicBool> {
        let stop = Arc::new(AtomicBool::new(false));
        let feeder_stop = stop.clone();
        let feeder_state = state.clone();
        let feeder_sid = session_id.to_string();
        std::thread::spawn(move || {
            let mut n = 0u32;
            while !feeder_stop.load(Ordering::Relaxed) {
                if let Some(vt) = feeder_state.grid.vt_log_buffers.get(&feeder_sid) {
                    vt.lock().process(format!("line {n}\r\n").as_bytes());
                }
                if let Some(dirty) = feeder_state.grid.frame_dirty.get(&feeder_sid) {
                    dirty.store(true, Ordering::Relaxed);
                }
                n += 1;
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        });
        stop
    }

    /// Every frame a subscriber was handed, in the order it was handed them.
    #[cfg(feature = "desktop")]
    type RecordedFrames = Arc<Mutex<Vec<Vec<u8>>>>;

    /// A desktop subscriber that records every frame it is handed and acks none —
    /// a WebView whose JS thread is blocked.
    ///
    /// Real, not a stand-in: registering the channel makes the production path
    /// mark the gate sent, so the gate closes the way it closes in the app rather
    /// than being pinned closed by the test, and the recording is what actually
    /// left Rust for that channel.
    #[cfg(feature = "desktop")]
    fn subscribe_a_frozen_webview(
        state: &Arc<AppState>,
        session_id: &str,
    ) -> (Arc<GridGate>, RecordedFrames) {
        let gate = Arc::new(GridGate::new());
        state
            .grid
            .gates
            .insert(session_id.to_string(), gate.clone());
        let received: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = received.clone();
        state.grid.channels.insert(
            session_id.to_string(),
            crate::state::DesktopGridChannel {
                channel: tauri::ipc::Channel::new(move |body| {
                    if let tauri::ipc::InvokeResponseBody::Raw(bytes) = body {
                        sink.lock().push(bytes);
                    }
                    Ok(())
                }),
                webview_label: "main".to_string(),
                epoch: gate.epoch(),
            },
        );
        (gate, received)
    }

    /// How long the browser is watched for while the desktop never acks.
    ///
    /// This bound IS the subject: the question is not whether a frame ever
    /// arrives, it is at what rate. Stopping the ticker on a closed gate does not
    /// silence the browser forever — the ticker gives the outstanding frame up
    /// after `MAX_IN_FLIGHT_MS` (500 ms) and the next tick sends one, which the
    /// frozen WebView immediately closes the gate with again. So the browser was
    /// throttled from the 16 ms tick to roughly 2 frames a second, and every third
    /// give-up adds a 1 s pause. Measured over this window: 3 frames on that
    /// path, 33 at the tick rate.
    const BROWSER_FEED_WINDOW: std::time::Duration = std::time::Duration::from_millis(1200);

    /// Between the two: twice what the give-up path produced in
    /// `BROWSER_FEED_WINDOW`, a fifth of what the tick rate produced. Sized for
    /// the gap, not for the expected value, so a machine five times slower than
    /// this one still passes.
    const BROWSER_FRAMES_EXPECTED: u64 = 6;

    #[cfg(feature = "desktop")]
    #[tokio::test(flavor = "current_thread", start_paused = false)]
    async fn a_stalled_desktop_gate_does_not_stop_the_browser_frames() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mut rx = grid_session(&state, "stalled-webview");
        let (gate, _received) = subscribe_a_frozen_webview(&state, "stalled-webview");

        let stop = start_ticker(&state, "stalled-webview");
        // `spawn_reader_thread` installs the dirty flag the ticker reads, so the
        // feed can only start once the threads are up.
        let stop_feed = feed_continuously(&state, "stalled-webview");

        let first_seq = rx.borrow_and_update().seq;
        tokio::time::sleep(BROWSER_FEED_WINDOW).await;
        let frames = rx.borrow_and_update().seq - first_seq;

        stop_feed.store(true, Ordering::Relaxed);
        stop.store(true, Ordering::Relaxed);

        assert!(
            !gate.is_open(),
            "the WebView under test has to still be behind, or nothing was throttling"
        );
        assert!(
            frames >= BROWSER_FRAMES_EXPECTED,
            "the browser got {frames} frames in {BROWSER_FEED_WINDOW:?} while the desktop \
             gate was closed; a stalled WebView is still throttling a transport that \
             never fell behind"
        );
    }

    #[cfg(feature = "desktop")]
    #[tokio::test(flavor = "current_thread", start_paused = false)]
    async fn the_desktop_is_owed_a_full_frame_after_it_catches_up() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mut rx = grid_session(&state, "caught-up");
        let (gate, received) = subscribe_a_frozen_webview(&state, "caught-up");

        // The WebView has caught up, but frames went out to the WebSocket
        // subscribers while it was behind: those rows left the shared damage
        // without ever reaching its channel, so a delta now would land on a row
        // map with holes in it.
        gate.note_missed();
        assert!(
            gate.is_open(),
            "the debt is only repayable once it catches up"
        );

        // Drain the first frame a fresh grid always owes — it has no previous
        // viewport to diff against, so it is full by construction and would say
        // nothing about the repair below.
        let _ = cut_frame(&state, "caught-up", "already painted\r\n");

        let stop = start_ticker(&state, "caught-up");
        {
            let vt = state
                .grid
                .vt_log_buffers
                .get("caught-up")
                .expect("session exists");
            vt.lock().process(b"one more line\r\n");
        }
        state
            .grid
            .frame_dirty
            .get("caught-up")
            .expect("the ticker owns this flag")
            .store(true, Ordering::Relaxed);

        let delivered = tokio::time::timeout(TICKER_LIVENESS_BOUND, rx.changed()).await;
        stop.store(true, Ordering::Relaxed);
        delivered
            .expect("the frame ticker never took a tick")
            .expect("the watch sender outlives the test");

        // Assert on the FIRST frame the channel got, not the last: the WebView
        // stays frozen, so every later tick finds the gate shut again and the
        // count keeps moving after the assertion is made.
        let first = received
            .lock()
            .first()
            .cloned()
            .expect("the desktop channel got nothing at all");
        assert_eq!(
            row_count(&first),
            24,
            "the desktop's first frame after the gap has to be the whole screen — \
             a delta would paint onto rows it never received"
        );
        // The repair is private to that channel: it consumes no damage, so what
        // the browser got on the same tick is still the ordinary delta.
        assert!(
            row_count(&rx.borrow_and_update().frame) < 24,
            "repairing the desktop must not cost the browser a full frame"
        );
    }
}

// ---------------------------------------------------------------------
// Critic tests for #1312-3ba6: the live-prompt window widened from 3 to 4 rows
// is shared with Gemini, and the paste-placeholder probe reads 8 bottom rows.
// ---------------------------------------------------------------------

#[test]
fn gemini_quote_four_rows_above_the_bottom_is_not_a_ready_prompt() {
    // catches: the widened 4-row window lets a markdown quote in history read
    // as the live Gemini composer, flipping a working agent to Ready.
    let screen: Vec<String> = ["> quoted user prose", "output a", "output b", "output c"]
        .iter()
        .map(|s| s.to_string())
        .collect();

    assert_eq!(
        detect_gemini_screen_activity(&screen),
        AgentScreenActivity::Unknown
    );
}

#[test]
fn codex_old_prompt_four_rows_above_the_bottom_is_not_ready() {
    // catches: a submitted `›` row in history, with no live composer, read as Ready.
    let screen: Vec<String> = ["› earlier prompt", "• Ran ls", "  file_a", "  file_b"]
        .iter()
        .map(|s| s.to_string())
        .collect();

    assert_ne!(
        detect_codex_screen_activity(&screen),
        AgentScreenActivity::Ready
    );
}

fn codex_state_showing(sid: &str, lines: &[&str]) -> crate::state::AppState {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, sid, SHELL_IDLE);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .agent_type = Some("codex".into());
    let mut vt = VtLogBuffer::new(24, 100, 2000);
    vt.process(lines.join("\r\n").as_bytes());
    state.grid.vt_log_buffers.insert(sid.into(), Mutex::new(vt));
    state
}

#[test]
fn placeholder_in_transcript_history_does_not_trigger_a_retry_enter() {
    // catches: a stale `[Pasted Content` in an already-submitted transcript row
    // within the bottom 8 rows makes composer_retains_text true for an EMPTY
    // composer, so a second Enter is sent for a turn that was already submitted.
    let state = codex_state_showing(
        "crit-history-placeholder",
        &[
            "› [Pasted Content 1967 chars]",
            "",
            "• Ran cargo test",
            "  ok",
            "",
            "› ",
            "",
            "  gpt-5 · ~/repo",
        ],
    );

    assert!(!composer_retains_text(
        &state,
        "crit-history-placeholder",
        "run the next step please"
    ));
}

#[test]
fn placeholder_probe_counts_non_empty_rows_not_screen_rows() {
    // catches: the 8-row bound counting blank padding rows, or off-by-one at the edge.
    let mut inside = vec!["› [Pasted Content 1967 chars]"];
    inside.extend(["r2", "r3", "r4", "r5", "r6", "r7", "r8"]); // placeholder is 8th from bottom
    let state = codex_state_showing("crit-edge-in", &inside);
    assert!(composer_retains_text(&state, "crit-edge-in", "brief"));

    let mut outside = vec!["› [Pasted Content 1967 chars]"];
    outside.extend(["r2", "r3", "r4", "r5", "r6", "r7", "r8", "r9"]); // 9th from bottom
    let state = codex_state_showing("crit-edge-out", &outside);
    assert!(!composer_retains_text(&state, "crit-edge-out", "brief"));
}

// ---- critic-1302: dismissed AskUserQuestion, attack cases -----------------

/// Replays the live Claude Esc capture through the production chunk processor
/// and returns once the whole capture was consumed.
#[cfg(test)]
async fn replay_claude_askuser_esc(
    sid: &str,
) -> (Arc<AppState>, Arc<Mutex<SilenceState>>, ChunkProcessor) {
    let capture = crate::pty_capture::decode_capture(&agent_prompt_fixture(
        "claude-askuser-esc-20260929.tcap",
    ))
    .expect("valid live capture");
    let (rows, cols) = capture.geometry.expect("recorded terminal geometry");
    let (state, silence) = chunk_trace_state(sid);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .agent_type = Some("claude".into());
    state
        .grid
        .vt_log_buffers
        .insert(sid.into(), Mutex::new(VtLogBuffer::new(rows, cols, 2000)));
    crate::state::AppState::spawn_session_state_accumulator(state.clone());
    let mut processor = ChunkProcessor::new(None, None);
    let mut utf8 = Utf8ReadBuffer::new();
    let mut escape = EscapeAwareBuffer::new();
    for record in capture.records {
        if record.direction == crate::pty_capture::CaptureDirection::Output {
            let data = utf8.push(&record.data);
            let data = escape.push(&data);
            let (clean, _) = crate::state::strip_kitty_sequences(&data);
            processor.process_chunk(&clean, &silence, sid, &state);
            // The declined-result branch reads the awaiting state the accumulator
            // owns. On a current-thread runtime it applies nothing until the test
            // yields, so a replay that never yields sees no question to release.
            for _ in 0..200 {
                if state.session_maps.session_state_events.depth() == 0 {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
        }
    }
    (state, silence, processor)
}

/// Catches: the declined-question idle is recorded at a rank that later hooks
/// cannot override, so the next turn's `state=busy` hook is ignored and the tab
/// shows idle while Claude works.
#[tokio::test(flavor = "current_thread")]
async fn critic_1302_hook_busy_after_a_dismissed_question_is_honoured() {
    let sid = "critic-1302-late-busy";
    let (state, silence, processor) = replay_claude_askuser_esc(sid).await;
    assert_eq!(shell_of(&state, sid), SHELL_IDLE);

    processor.handle_tuic_state("busy", sid, &state);
    assert_eq!(
        shell_of(&state, sid),
        SHELL_BUSY,
        "a busy hook after the dismissal starts a new turn"
    );
    assert!(silence.lock().hook_busy());

    processor.handle_tuic_state("idle", sid, &state);
    assert_eq!(shell_of(&state, sid), SHELL_IDLE);
}

/// Catches: the decline wording in ordinary output (an agent quoting it, a
/// diff) idles a Claude that is busy and not awaiting any question.
#[tokio::test(flavor = "current_thread")]
async fn critic_1302_decline_text_without_a_pending_question_leaves_busy_alone() {
    let sid = "critic-1302-quote";
    let (state, silence, mut processor) = replay_claude_askuser_esc(sid).await;
    processor.handle_tuic_state("busy", sid, &state);
    assert_eq!(shell_of(&state, sid), SHELL_BUSY);

    processor.process_chunk(
        "\r\nUser declined to answer questions\r\n",
        &silence,
        sid,
        &state,
    );
    assert_eq!(
        shell_of(&state, sid),
        SHELL_BUSY,
        "no confident question was pending, so the text proves nothing"
    );
}

// --- critic-1299: attacks on the OpenCode --mini status-row detector. A false Ready
// feeds auto-standby (SIGSTOP) and the queue drain, so every row below is a live turn
// or a screen that must not be read as idle.

fn opencode_mini_rows(last: &str) -> Vec<String> {
    vec![
        "  I will run the build now.".to_string(),
        last.to_string(),
        String::new(),
    ]
}

/// Catches: a label made only of `-`/`_` reads as an agent label, so a diff header or
/// markdown rule printed by a tool mid-turn flips the session to Ready.
#[test]
fn opencode_mini_dash_run_tool_output_is_not_an_agent_label() {
    assert_opencode_mini_not_ready("--- a/src/lib.rs");
    assert_opencode_mini_not_ready("---");
    assert_opencode_mini_not_ready("___");
}

/// Catches: a digits-only first token satisfies the label rule, so tool output such as a
/// match count reads as an idle status row.
#[test]
fn opencode_mini_numeric_tool_output_is_not_an_agent_label() {
    assert_opencode_mini_not_ready("1234 files matched");
    assert_opencode_mini_not_ready("42");
}

/// Catches: any uppercase first word of the last painted row (test runner `PASS`, `OK`,
/// `NOTE`) is taken for the agent label while the status row is not painted.
#[test]
fn opencode_mini_uppercase_tool_output_is_not_an_agent_label() {
    assert_opencode_mini_not_ready("PASS src/foo.test.ts");
    assert_opencode_mini_not_ready("OK");
    assert_opencode_mini_not_ready("NOTE remember to rebase");
}

/// Catches: at very narrow widths `esc interrupt` is cut mid-word, the contains check
/// misses it and a running turn reads Ready.
#[test]
fn opencode_mini_truncated_interrupt_hint_is_not_ready() {
    assert_opencode_mini_not_ready(
        " BUILD  \u{2B1D}\u{2B1D}\u{25A0}\u{25A0}\u{25A0}\u{25A0} esc inte",
    );
    assert_opencode_mini_not_ready(" BUILD  \u{2B1D}\u{2B1D}\u{25A0}\u{25A0}\u{25A0}\u{25A0} esc");
}

/// Catches: the progress bar is painted before its `esc interrupt` text, so a status row
/// that already shows the running bar but not yet the hint reads Ready.
#[test]
fn opencode_mini_progress_bar_without_hint_is_not_ready() {
    assert_opencode_mini_not_ready(
        " BUILD  \u{2B1D}\u{2B1D}\u{2B1D}\u{25A0}\u{25A0}\u{25A0}\u{25A0}\u{25A0}",
    );
}

/// Catches: a one-letter uppercase token (`A`, `I`) opening the last row passes the label
/// rule.
#[test]
fn opencode_mini_single_letter_token_is_not_an_agent_label() {
    assert_opencode_mini_not_ready("A");
    assert_opencode_mini_not_ready("I think the build passed");
}

/// Catches: the queue drain of a captured OpenCode mini turn happens for a reason other
/// than the Ready classification (silence, a stale epoch), so the drain test passes with
/// the adapter blind. The same replay, but the status row is overwritten by a permission
/// prompt before the timer runs: the screen is not Ready and nothing may be typed.
#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn queued_command_does_not_drain_when_the_captured_mini_screen_is_not_ready() {
    tokio::time::pause();
    let capture = crate::pty_capture::decode_capture(&agent_prompt_fixture(
        "opencode-1.18.30-mini-turn.tcap",
    ))
    .expect("valid live capture");
    let (rows, cols) = capture.geometry.expect("recorded terminal geometry");
    let sid = "opencode-mini-queue-held";
    let (state, silence) = chunk_trace_state(sid);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .agent_type = Some("opencode".into());
    state
        .grid
        .vt_log_buffers
        .insert(sid.into(), Mutex::new(VtLogBuffer::new(rows, cols, 2000)));
    let bytes = insert_recording_session(&state, sid);
    state
        .session_maps
        .shell_states
        .get(sid)
        .unwrap()
        .store(SHELL_BUSY, std::sync::atomic::Ordering::Release);
    enqueue_user_command(&state, sid, "resume queued work", None).unwrap();

    let mut processor = ChunkProcessor::new(None, None);
    for record in capture.records {
        if record.direction == crate::pty_capture::CaptureDirection::Output {
            processor.process_chunk(
                std::str::from_utf8(&record.data).expect("UTF-8 terminal output"),
                &silence,
                sid,
                &state,
            );
        }
    }
    processor.process_chunk(
        &format!("\x1b[{rows};1H\x1b[2K  Allow once   Allow always   Reject"),
        &silence,
        sid,
        &state,
    );
    assert_ne!(
        silence.lock().cached_screen_activity,
        AgentScreenActivity::Ready,
        "a permission prompt on the last row is not an idle composer"
    );
    let replayed = bytes.lock().unwrap().len();
    {
        let mut session = state.session_maps.session_states.get_mut(sid).unwrap();
        session.background_probe_satisfied_turn_epoch = Some(session.turn_epoch);
    }
    {
        let mut silence = silence.lock();
        let settled = std::time::Instant::now() - std::time::Duration::from_secs(60);
        silence.last_output_at = settled;
        silence.last_chunk_at = settled;
        silence.screen_ready_pending_since = Some(settled);
    }

    let running = Arc::new(AtomicBool::new(true));
    spawn_silence_timer(silence, running.clone(), sid.into(), state.clone());
    tokio::task::yield_now().await;
    tokio::time::advance(SILENCE_CHECK_INTERVAL + std::time::Duration::from_millis(1)).await;
    tokio::task::yield_now().await;
    running.store(false, Ordering::Release);
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert_eq!(
        bytes.lock().unwrap()[replayed..].to_vec(),
        Vec::<u8>::new(),
        "nothing may be typed into a session that is not at an idle composer"
    );
}

fn critic_mini_rows(last: &str) -> Vec<String> {
    vec![
        "some tool output".to_string(),
        String::new(),
        last.to_string(),
    ]
}

/// Below the narrowest width at which OpenCode paints the status row (46 columns) the
/// row is absent (observed at 40), so a bare `BUILD` last line is tool output.
/// Catches: the width never reaching the adapter, so assistant text that happens to end
/// on `BUILD` or `PLAN` reads Ready mid-turn on a narrow pane.
#[test]
fn opencode_mini_bare_label_below_the_narrowest_observed_width_is_tool_output() {
    let rows = vec![
        "  I will run the build now.".to_string(),
        String::new(),
        " BUILD".to_string(),
    ];
    assert_eq!(
        detect_agent_screen_activity_at(Some("opencode"), &rows, Some(40)),
        AgentScreenActivity::Unknown
    );
    assert_eq!(
        detect_agent_screen_activity_at(Some("opencode"), &rows, Some(64)),
        AgentScreenActivity::Ready,
        "the 64-column capture paints exactly this row, and so does every width from 46"
    );
}

/// Same defect through the reader: `process_chunk` must hand the grid width to the
/// adapter, otherwise the unit above passes while production still reads Ready.
#[test]
fn opencode_mini_reader_passes_the_grid_width_to_the_adapter() {
    let sid = "opencode-mini-narrow-reader";
    let (state, silence) = chunk_trace_state(sid);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .agent_type = Some("opencode".into());
    state
        .grid
        .vt_log_buffers
        .insert(sid.into(), Mutex::new(VtLogBuffer::new(20, 40, 200)));
    let mut processor = ChunkProcessor::new(None, None);
    processor.process_chunk(
        "\x1b[2J\x1b[H  I will run the build now.\r\n\r\n BUILD",
        &silence,
        sid,
        &state,
    );
    assert_eq!(
        silence.lock().cached_screen_activity,
        AgentScreenActivity::Unknown,
        "a 40-column screen has no status row"
    );
}

// --- critic-1299 round 4: width gate of the OpenCode --mini status row. Live tmux capture on
// opencode 1.18.30 (fresh screen): the ` BUILD` row is painted from 46 columns up and absent at
// 45 and below, where the placeholder wraps onto a second line.

fn critic4_bare_build_rows() -> Vec<String> {
    vec![
        "  I will run the build now.".to_string(),
        String::new(),
        " BUILD".to_string(),
    ]
}

/// Catches: the width reaching the adapter in the reader as a constant, or as the wrong
/// dimension (rows instead of columns): a 50-column idle pane must reach Ready through
/// `process_chunk`, the path that feeds the queue drain.
#[test]
fn critic4_mini_reader_reads_ready_on_a_fifty_column_screen() {
    let sid = "opencode-mini-mid-width-reader";
    let (state, silence) = chunk_trace_state(sid);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .agent_type = Some("opencode".into());
    state
        .grid
        .vt_log_buffers
        .insert(sid.into(), Mutex::new(VtLogBuffer::new(20, 50, 200)));
    let mut processor = ChunkProcessor::new(None, None);
    processor.process_chunk(
        "\x1b[2J\x1b[H  I will run the build now.\r\n\r\n BUILD",
        &silence,
        sid,
        &state,
    );
    assert_eq!(
        silence.lock().cached_screen_activity,
        AgentScreenActivity::Ready,
        "a 50-column opencode --mini pane paints ` BUILD`"
    );
}

fn rows_of(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|line| (*line).to_string()).collect()
}

/// Catches: the three mutants of the `take_while` clause that bounds the composer block
/// in `composer_holds_paste_placeholder`.
/// - `||` -> `&&` and delete `!`: the block shrinks to the `›` row, so a placeholder on a
///   continuation row of a wrapped composer is missed and the swallowed Enter is not retried.
/// - `==` -> `!=`: the block no longer stops at the first blank row, so a stale placeholder in
///   the history below the composer draws a second Enter.
#[test]
fn paste_placeholder_detection_each_clause() {
    let on_prompt_row = rows_of(&["› [Pasted Content 1967 chars]", "", "  gpt-5 · ~/repo"]);
    assert!(composer_holds_paste_placeholder(&on_prompt_row));

    let on_continuation_row = rows_of(&[
        "› summarise this",
        "  [Pasted Content 1967 chars]",
        "  gpt-5 · ~/repo",
    ]);
    assert!(
        composer_holds_paste_placeholder(&on_continuation_row),
        "a wrapped composer keeps the placeholder below the › row"
    );

    let behind_a_blank_row = rows_of(&["› run", "", "  [Pasted Content 1967 chars]"]);
    assert!(
        !composer_holds_paste_placeholder(&behind_a_blank_row),
        "the composer block ends at the first blank row"
    );

    let no_prompt_row = rows_of(&["  [Pasted Content 1967 chars]"]);
    assert!(composer_holds_paste_placeholder(&no_prompt_row));
}

#[cfg(unix)]
enum RetryWriterFault {
    None,
    Write,
    Flush,
}

/// A PTY writer that, on Enter, makes the screen show Codex working and the output
/// advance, i.e. everything `wait_for_queued_submission` needs to confirm. Only the
/// `fault` decides whether `retry_enter_for_retained_composer` may reach that wait.
#[cfg(unix)]
struct WorkingOnEnterWriter {
    state: Arc<AppState>,
    sid: &'static str,
    fault: RetryWriterFault,
}

/// Catches: `||` -> `&&` on the write-or-flush failure check of
/// `retry_enter_for_retained_composer`. A failed write (or flush) of the retry Enter must
/// report false at once; with `&&` it falls through and claims the submission was confirmed.
/// The healthy writer proves the fixture does reach the confirmation otherwise.
#[cfg(unix)]
#[test]
fn retry_enter_only_when_composer_retained_and_clause_two() {
    let text = "run the next step please";
    let retry = |sid: &'static str, fault: RetryWriterFault| {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        agent_session(&state, sid, SHELL_IDLE);
        state
            .session_maps
            .session_states
            .get_mut(sid)
            .unwrap()
            .agent_type = Some("codex".into());
        let mut vt = VtLogBuffer::new(24, 100, 1000);
        vt.process("› run the next step please\r\n\r\n  gpt-5 · ~/repo".as_bytes());
        state.grid.vt_log_buffers.insert(sid.into(), Mutex::new(vt));
        state
            .session_maps
            .output_buffers
            .insert(sid.into(), Mutex::new(OutputRingBuffer::new(4096)));
        insert_session_with_writer(
            &state,
            sid,
            Box::new(WorkingOnEnterWriter {
                state: Arc::clone(&state),
                sid,
                fault,
            }),
            TtyMode::Raw,
        );
        retry_enter_for_retained_composer(&state, sid, text, "ready_screen")
    };

    assert!(
        retry("retry-healthy", RetryWriterFault::None),
        "a delivered Enter that makes Codex work is confirmed"
    );
    assert!(
        !retry("retry-write-fails", RetryWriterFault::Write),
        "a failed write is not a confirmed submission"
    );
    assert!(
        !retry("retry-flush-fails", RetryWriterFault::Flush),
        "a failed flush is not a confirmed submission"
    );
}

/// Catches: confirmation samples only the current Working screen and forgets a
/// submitted turn that has already reached its question before the worker runs.
/// The response is the real Codex request_user_input capture, not invented ANSI.
#[cfg(unix)]
#[test]
fn submit_paths_do_not_toast_when_captured_codex_turn_already_reached_a_question() {
    // The capture holds terminal queries whose replies need this writer's lock,
    // which Enter still holds while the response is consumed inline. A cooked tty
    // withholds replies (`tty_would_swallow_reply`), so they cannot self-deadlock.
    struct CapturedTurnWriter {
        state: Arc<AppState>,
        sid: String,
        response: Vec<Vec<u8>>,
        writes: Arc<std::sync::Mutex<Vec<Vec<u8>>>>,
    }
    impl std::io::Write for CapturedTurnWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.writes.lock().unwrap().push(bytes.to_vec());
            if bytes == b"\r" {
                let silence = self
                    .state
                    .session_maps
                    .silence_states
                    .get(&self.sid)
                    .unwrap()
                    .clone();
                let mut reader = ChunkProcessor::new(None, None);
                for chunk in &self.response {
                    reader.process_chunk(
                        &String::from_utf8_lossy(chunk),
                        &silence,
                        &self.sid,
                        &self.state,
                    );
                }
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    use crate::pty_capture::CaptureDirection::{Input, Output};
    let capture = crate::pty_capture::decode_capture(&agent_prompt_fixture(
        "codex-request-user-input-20260929.tcap",
    ))
    .unwrap();
    let (rows, cols) = capture.geometry.unwrap();
    let text_index = capture
        .records
        .iter()
        .position(|record| {
            record.direction == Input && record.data.starts_with(b"Before doing any work")
        })
        .unwrap();
    let enter_index = text_index + 1;
    assert_eq!(capture.records[enter_index].data, b"\r");
    let text = String::from_utf8(capture.records[text_index].data.clone()).unwrap();
    let question_end = capture
        .records
        .iter()
        .enumerate()
        .skip(enter_index + 1)
        .find(|(_, record)| {
            record
                .data
                .windows(b"state=idle".len())
                .any(|w| w == b"state=idle")
        })
        .map(|(index, _)| index + 1)
        .unwrap();
    let response: Vec<Vec<u8>> = capture.records[enter_index + 1..question_end]
        .iter()
        .filter(|record| record.direction == Output)
        .map(|record| record.data.clone())
        .collect();

    for path in ["brief", "queued_input", "lifecycle_wake", "mail_notice"] {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let sid = format!("captured-confirmation-{path}");
        agent_session(&state, &sid, SHELL_IDLE);
        state
            .session_maps
            .session_states
            .get_mut(&sid)
            .unwrap()
            .agent_type = Some("codex".into());
        let mut vt = VtLogBuffer::new(rows, cols, 2000);
        for record in capture.records[..text_index]
            .iter()
            .filter(|record| record.direction == Output)
        {
            vt.process(&record.data);
        }
        state
            .grid
            .vt_log_buffers
            .insert(sid.clone(), Mutex::new(vt));
        state
            .session_maps
            .output_buffers
            .insert(sid.clone(), Mutex::new(OutputRingBuffer::new(1 << 20)));
        let writes = Arc::new(std::sync::Mutex::new(Vec::new()));
        insert_session_with_writer(
            &state,
            &sid,
            Box::new(CapturedTurnWriter {
                state: Arc::clone(&state),
                sid: sid.clone(),
                response: response.clone(),
                writes: Arc::clone(&writes),
            }),
            TtyMode::Cooked,
        );
        let injection = match path {
            "brief" => crate::state::PendingInjection::initial_prompt(&text),
            "queued_input" => crate::state::PendingInjection::user_command(&text),
            _ => crate::state::PendingInjection::notice(&text),
        };
        state
            .pending_injections
            .entry(sid.clone())
            .or_default()
            .push_back(injection);
        let mut alerts = state.event_bus.subscribe();
        flush_pending_injections_blocking(&state, &sid);
        assert!(std::iter::from_fn(|| alerts.try_recv().ok()).all(|event| !matches!(event,
            crate::state::AppEvent::McpToast { ref title, .. } if title == "Agent input was not confirmed"
        )), "{path}: the accepted turn must not produce a false failure toast");
        assert_eq!(
            writes
                .lock()
                .unwrap()
                .iter()
                .filter(|write| write.as_slice() == b"\r")
                .count(),
            1,
            "{path}: a confirmed turn must not receive a duplicate Enter"
        );
    }
}

/// Catches: a headless terminal needs UI polling to discover an agent, or retains
/// a shell-era screen cache after the foreground identity becomes known.
#[cfg(unix)]
#[tokio::test]
async fn headless_foreground_timer_discovers_claude_and_reclassifies_quiet_screen() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "headless-foreground-discovery";
    let _probe = crate::test_support::ForegroundIdentityProbe::new(state.clone(), sid, "claude");
    let capture = crate::pty_capture::decode_capture(&agent_prompt_fixture(
        "claude-askuser-esc-20260929.tcap",
    ))
    .unwrap();
    let (rows, cols) = capture.geometry.unwrap();
    let mut vt = VtLogBuffer::new(rows, cols, 2000);
    state
        .session_maps
        .output_buffers
        .insert(sid.into(), Mutex::new(OutputRingBuffer::new(4096)));
    // Recorded ready composer after Esc, before record 195 paints the next draft.
    for record in capture.records.into_iter().take(195) {
        if record.direction == crate::pty_capture::CaptureDirection::Output {
            vt.process(&record.data);
            state
                .session_maps
                .output_buffers
                .get(sid)
                .unwrap()
                .lock()
                .write(&record.data);
        }
    }
    let output_offset = state
        .session_maps
        .output_buffers
        .get(sid)
        .unwrap()
        .lock()
        .total_written;
    state.grid.vt_log_buffers.insert(sid.into(), Mutex::new(vt));
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
    let running = Arc::new(AtomicBool::new(true));
    // Paused Tokio time advances the timer without asserting startup latency.
    tokio::time::pause();
    spawn_silence_timer(silence.clone(), running.clone(), sid.into(), state.clone());
    // Sleeping past the first scheduled tick lets Tokio dispatch its sleeper;
    // advance(interval) followed by yield can inspect before that dispatch.
    tokio::time::sleep(SILENCE_CHECK_INTERVAL * 2).await;
    running.store(false, Ordering::Release);
    assert_eq!(
        state
            .session_state_with_shell(sid)
            .unwrap()
            .agent_state
            .as_deref(),
        Some("idle")
    );
    assert_eq!(
        silence.lock().cached_screen_activity,
        AgentScreenActivity::Ready
    );
    assert_eq!(silence.lock().last_ready_screen_offset, output_offset);
}

/// Catches: a bash-script wrapper is mistaken for the owning shell on macOS,
/// never observed, and leaves its preset armed after returning to the real root.
#[cfg(unix)]
#[test]
fn bash_script_wrapper_is_observed_and_revoked_only_when_its_shell_root_returns() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "bash-script-wrapper-root";
    let mut probe = crate::test_support::ForegroundIdentityProbe::bash_wrapper(
        state.clone(),
        sid,
        crate::state::SpawnRootRole::Shell,
    );
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .seed_configured_agent(Some("claude".into()));
    assert_eq!(
        refresh_session_agent(&state, sid).as_deref(),
        Some("claude")
    );
    assert!(
        state
            .session_maps
            .session_states
            .get(sid)
            .unwrap()
            .agent_foreground_observed
    );
    probe.return_to_root();
    assert_eq!(refresh_session_agent(&state, sid), None);
    assert!(!should_inject_now(&state, sid));
    assert!(probe.bytes.lock().unwrap().is_empty());
}

/// Catches: exec into a different shell image at the same root PID is promoted
/// to an agent or retains an old discovered agent's identity.
#[cfg(unix)]
#[test]
fn shell_root_exec_into_another_shell_still_revokes_agent_identity() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "exec-root-shell";
    let probe =
        crate::test_support::ForegroundIdentityProbe::shell_root(state.clone(), sid, "bash");
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .agent_type = Some("claude".into());
    assert_eq!(refresh_session_agent(&state, sid), None);
    assert_eq!(
        state
            .session_maps
            .session_states
            .get(sid)
            .unwrap()
            .agent_type,
        None
    );
    assert!(!should_inject_now(&state, sid));
    assert!(probe.bytes.lock().unwrap().is_empty());
}

/// Catches: a newly committed capture or a changed state/question decision escapes
/// the small hand-picked baseline. The filesystem walk of `src-tauri/` is authoritative,
/// including crate fixtures; it needs no Git index, so replicas without one run it too.
#[test]
fn replay_oracle_all_committed_tcap_preserves_chunk_decisions() {
    use serde_json::json;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    fn collect_captures(dir: &std::path::Path, root: &std::path::Path, out: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                let name = path.file_name().unwrap().to_string_lossy();
                if !matches!(name.as_ref(), "target" | ".tmp" | "node_modules") {
                    collect_captures(&path, root, out);
                }
            } else if path.extension().is_some_and(|ext| ext == "tcap") {
                let relative = path.strip_prefix(root).unwrap();
                out.push(
                    relative
                        .components()
                        .map(|c| c.as_os_str().to_str().expect("UTF-8 fixture path"))
                        .collect::<Vec<_>>()
                        .join("/"),
                );
            }
        }
    }
    let mut fixtures = Vec::new();
    collect_captures(&root.join("src-tauri"), root, &mut fixtures);
    fixtures.sort();
    assert!(!fixtures.is_empty(), "empty corpus cannot establish parity");
    let mut manifest = Vec::new();
    for fixture in fixtures {
        let agent = [
            "claude", "codex", "grok", "goose", "opencode", "gemini", "aider", "pi", "amp",
            "cursor", "droid",
        ]
        .into_iter()
        .find(|agent| {
            std::path::Path::new(&fixture)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(agent)
                || fixture.contains(&format!("/{agent}-"))
        })
        .unwrap_or_else(|| panic!("{fixture}: declare capture's agent before recording a golden"));
        let capture =
            crate::pty_capture::decode_capture(&std::fs::read(root.join(&fixture)).unwrap())
                .unwrap();
        let sid = "replay-oracle";
        let (state, silence) = chunk_trace_state(sid);
        state
            .session_maps
            .session_states
            .get_mut(sid)
            .unwrap()
            .agent_type = Some(agent.into());
        let (rows, cols) = capture.geometry.unwrap_or((41, 128));
        state
            .grid
            .vt_log_buffers
            .get(sid)
            .unwrap()
            .lock()
            .resize(rows, cols);
        let mut rx = state.event_bus.subscribe();
        let mut cp = ChunkProcessor::new(None, None);
        let mut utf8 = Utf8ReadBuffer::new();
        let mut escapes = EscapeAwareBuffer::new();
        let scenario_path = root.join(&fixture).with_extension("scenario.json");
        let mut trace = vec![json!({"fixture":fixture,"agent":agent,"geometry":[rows,cols]})];
        for (record_index, record) in capture.records.iter().enumerate() {
            if record.direction != crate::pty_capture::CaptureDirection::Output {
                // New scenario captures also preserve the real submit boundary.
                // Keep legacy output-only goldens unchanged; use the production
                // input FSM and state transitions rather than hand-setting busy.
                if scenario_path.exists() {
                    crate::mcp_http::session::apply_input_bookkeeping(
                        &state,
                        sid,
                        std::str::from_utf8(&record.data).expect("UTF-8 scenario input"),
                    );
                }
                continue;
            }
            let text = utf8.push(&record.data);
            let escaped = escapes.push(&text);
            let (clean, _) = crate::state::strip_kitty_sequences(&escaped);
            let _ = cp.process_chunk(&clean, &silence, sid, &state);
            loop {
                match rx.try_recv() {
                    Ok(event) => {
                        crate::state::tests_support::apply_replay_event(&state, &event);
                        if let crate::state::AppEvent::PtyParsed { parsed, .. } = event {
                            trace.push(json!({"record":record_index,"event":*parsed}));
                        }
                    }
                    Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
                    Err(error) => panic!("{fixture}: replay lost events: {error}"),
                }
            }
            let snapshot = state.session_state_with_shell(sid).unwrap();
            trace.push(json!({"record":record_index,"state": {
                "shell":state.session_maps.shell_states.get(sid).unwrap().load(std::sync::atomic::Ordering::Acquire),
                "agent":snapshot.agent_state,"awaiting":snapshot.awaiting_input,
                "question":snapshot.question_text,"confident":snapshot.question_confident,
                "last_question":cp.last_question_text,"choice_sig":cp.last_choice_prompt_sig,
                "fullscreen":cp.terminal_mode.is_fullscreen(),
            }}));
        }
        trace.push(
            json!({"ring_len":state.session_maps.output_buffers.get(sid).unwrap().lock().len()}),
        );
        if scenario_path.exists() {
            let scenario: serde_json::Value = serde_json::from_slice(
                &std::fs::read(&scenario_path).expect("read capture scenario"),
            )
            .expect("valid capture scenario");
            assert_eq!(
                scenario["agent"], agent,
                "{fixture}: scenario agent mismatch"
            );
            let expected = scenario["expected_states"]
                .as_array()
                .expect("scenario expected_states must be an array");
            crate::replay_oracle::assert_expected_states(&fixture, expected, &trace);
        }
        manifest.push(json!({"fixture":fixture,"events":trace.len()}));
        crate::replay_oracle::assert_golden(
            &std::path::Path::new("captures").join(format!("{fixture}.jsonl")),
            &trace,
        );
    }
    crate::replay_oracle::assert_golden(std::path::Path::new("corpus.jsonl"), &manifest);
}

/// Catches: OSC titles only emit on AppHandle, leaving headless remote tabs named shell.
#[test]
fn remote_title_parser_publishes_title_and_reset() {
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "remote-title";
    agent_session(&state, sid, SHELL_IDLE);
    state
        .grid
        .vt_log_buffers
        .insert(sid.into(), Mutex::new(VtLogBuffer::new(24, 80, 1000)));
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
    let mut processor = ChunkProcessor::new(None, None);
    let mut events = state.event_bus.subscribe();
    processor.process_chunk("\x1b]0;Claude Code\x07", &silence, sid, &state);
    processor.process_chunk("\x1b]2;\x07", &silence, sid, &state);
    let titles: Vec<_> = std::iter::from_fn(|| events.try_recv().ok())
        .filter_map(|event| match event {
            crate::state::AppEvent::PtyTitle { session_id, title } => {
                assert_eq!(session_id, sid);
                Some(title)
            }
            _ => None,
        })
        .collect();
    assert_eq!(titles, vec!["Claude Code", ""]);
}

#[cfg(unix)]
mod discovery_offsets {
    //! Round-2 critic tests for story 1420-f3de. Child of `pty` so they can read
    //! `SilenceState` offsets that no public accessor exposes.

    use super::*;
    use crate::state::VtLogBuffer;
    use crate::test_support::ForegroundIdentityProbe;

    #[cfg(unix)]
    fn discovered_claude_probe(
        sid: &str,
        screen: &[&str],
    ) -> (Arc<AppState>, ForegroundIdentityProbe) {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let probe = ForegroundIdentityProbe::new(state.clone(), sid, "claude");
        let mut vt = VtLogBuffer::new(24, 80, 1000);
        vt.process(screen.join("\r\n").as_bytes());
        state
            .grid
            .vt_log_buffers
            .insert(sid.to_string(), Mutex::new(vt));
        let mut ring = OutputRingBuffer::new(OUTPUT_RING_BUFFER_CAPACITY);
        ring.write(b"already-seen output");
        state
            .session_maps
            .output_buffers
            .insert(sid.to_string(), Mutex::new(ring));
        (state, probe)
    }

    /// Catches: identity discovery on a quiet WORKING screen caches the verdict
    /// but records no offset, so `fresh_working_transition` (submit ack) never
    /// sees Ready-then-Working and a submit into a working agent is mis-acked.
    #[cfg(unix)]
    #[test]
    fn discovery_on_a_quiet_working_screen_records_the_working_offset() {
        let sid = "critic-1420r2-working";
        let (state, _probe) =
            discovered_claude_probe(sid, &["✻ Cogitating… (3m 47s · ↓ 2.2k tokens)", "", "❯"]);
        assert_eq!(
            refresh_session_agent(&state, sid).as_deref(),
            Some("claude")
        );
        let total = state
            .session_maps
            .output_buffers
            .get(sid)
            .unwrap()
            .lock()
            .total_written;
        let silence = state.session_maps.silence_states.get(sid).unwrap();
        let silence = silence.lock();
        assert_eq!(silence.cached_screen_activity, AgentScreenActivity::Working);
        assert_eq!(silence.last_working_screen_offset, total);
        assert_eq!(silence.last_ready_screen_offset, 0);
    }

    /// Catches: the same for a quiet READY screen (offset left at 0, so a later
    /// submit offset compares as "Ready happened before").
    #[cfg(unix)]
    #[test]
    fn discovery_on_a_quiet_ready_screen_records_the_ready_offset() {
        let sid = "critic-1420r2-ready";
        let (state, _probe) = discovered_claude_probe(sid, &["done", "", "❯"]);
        assert_eq!(
            refresh_session_agent(&state, sid).as_deref(),
            Some("claude")
        );
        let total = state
            .session_maps
            .output_buffers
            .get(sid)
            .unwrap()
            .lock()
            .total_written;
        let silence = state.session_maps.silence_states.get(sid).unwrap();
        let silence = silence.lock();
        assert_eq!(silence.cached_screen_activity, AgentScreenActivity::Ready);
        assert_eq!(silence.last_ready_screen_offset, total);
    }

    /// Catches: a repeated refresh (timer tick every second plus HTTP polls) with
    /// no identity change re-records the offset, moving "Ready at" forward past a
    /// submit that happened in between.
    #[cfg(unix)]
    #[test]
    fn repeat_refresh_does_not_move_the_recorded_offset() {
        let sid = "critic-1420r2-repeat";
        let (state, _probe) = discovered_claude_probe(sid, &["done", "", "❯"]);
        refresh_session_agent(&state, sid);
        let first = state
            .session_maps
            .silence_states
            .get(sid)
            .unwrap()
            .lock()
            .last_ready_screen_offset;
        state
            .session_maps
            .output_buffers
            .get(sid)
            .unwrap()
            .lock()
            .write(b"later bytes");
        refresh_session_agent(&state, sid);
        let second = state
            .session_maps
            .silence_states
            .get(sid)
            .unwrap()
            .lock()
            .last_ready_screen_offset;
        assert_eq!(first, second);
    }

    /// Catches: a discovered agent that has exited back to a shell stays
    /// `agent_type=Some` forever, so automated submit/mail writes text into a bare
    /// shell prompt instead of being rejected `not_managed_agent`. (Policy
    /// question for the coordinator: preset run-config sessions also show a shell
    /// foreground while the shell boots, so clearing must not touch them.)
    #[cfg(unix)]
    #[test]
    fn shell_foreground_after_agent_exit_is_not_submittable() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let sid = "critic-1420r2-stale";
        let probe = ForegroundIdentityProbe::shell_root(state.clone(), sid, "bash");
        // What a previous refresh left behind while claude was foreground.
        state
            .session_maps
            .session_states
            .get_mut(sid)
            .unwrap()
            .agent_type = Some("claude".into());
        assert_eq!(refresh_session_agent(&state, sid), None);
        assert!(
            matches!(
                write_agent_submission_to_pty(&state, sid, "rm -rf scratch"),
                AgentSubmissionWrite::Rejected {
                    reason: "not_managed_agent",
                    ..
                }
            ),
            "stale agent_type let a submit through to a shell foreground; bytes={:?}",
            probe.bytes.lock().unwrap()
        );
    }

    /// Catches: the `claude/versions/<n>` layout is honoured by the macOS path
    /// lookup only; Linux `/proc/<pid>/comm` yields `2.1.5`, so a claude started
    /// by its versioned path is never classified on the platform the story is about.
    /// Negatives: non-numeric leaf, wrong parent, or missing `versions` segment
    /// must stay unclassified.
    #[cfg(unix)]
    #[test]
    fn versioned_claude_layout_is_classified_and_lookalikes_are_not() {
        use std::process::{Command, Stdio};
        let scratch = tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap();
        let run = |rel: &str| -> Option<&'static str> {
            let exe = scratch.path().join(rel);
            std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
            std::fs::copy("/bin/cat", &exe).unwrap();
            #[cfg(target_os = "macos")]
            assert!(
                Command::new("/usr/bin/codesign")
                    .args(["--force", "--sign", "-"])
                    .arg(&exe)
                    .status()
                    .unwrap()
                    .success()
            );
            let mut child = Command::new(&exe).stdin(Stdio::piped()).spawn().unwrap();
            let mut name = None;
            for _ in 0..200 {
                name = process_name_from_pid(child.id());
                if name.as_deref().is_some_and(|n| n != "cat") {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            child.kill().unwrap();
            child.wait().unwrap();
            classify_agent(&name.expect("process name"))
        };
        assert_eq!(run("claude/versions/2.1.5"), Some("claude"));
        assert_eq!(run("claude/versions/latest"), None);
        assert_eq!(run("xclaude/versions/2.1.5"), None);
        assert_eq!(run("claude/other/2.1.5"), None);
        assert_eq!(run("claude/versions/x/2.1.5"), None);
    }
}

mod identity_provenance {
    //! Round-3 critic tests for story 1420-f3de: provenance of `agent_type`.

    use super::*;
    #[cfg(unix)]
    use crate::test_support::ForegroundIdentityProbe;

    #[cfg(unix)]
    fn set_identity(state: &AppState, sid: &str, agent: Option<&str>, from_run_config: bool) {
        let mut s = state.session_maps.session_states.get_mut(sid).unwrap();
        s.agent_type = agent.map(str::to_string);
        s.agent_type_from_run_config = from_run_config;
    }

    #[cfg(unix)]
    fn identity(state: &AppState, sid: &str) -> Option<String> {
        state
            .session_maps
            .session_states
            .get(sid)
            .unwrap()
            .agent_type
            .clone()
    }

    /// Catches: a discovered agent loses its identity whenever the foreground pgid
    /// briefly points at an unrecognised NON-shell program (a `git`/`rg` child of
    /// the agent). Revocation must need positive evidence of a shell, not mere
    /// absence of an agent name; otherwise `suggest:`/`intent:` parsing and submit
    /// flap off while the agent is still alive.
    #[cfg(unix)]
    #[test]
    fn discovered_agent_survives_a_transient_unrecognised_non_shell_foreground() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let sid = "critic-1420r3-transient";
        let _probe = ForegroundIdentityProbe::shell_parent(state.clone(), sid, "git");
        set_identity(&state, sid, Some("claude"), false);
        refresh_session_agent(&state, sid);
        assert_eq!(
            identity(&state, sid).as_deref(),
            Some("claude"),
            "a transient non-shell foreground revoked a live agent's identity"
        );
    }

    /// Catches: a run-config preset (claude) overwritten by a hand-launched,
    /// different discovered agent (codex) keeps `agent_type_from_run_config=true`,
    /// so codex becomes unrevocable: after it exits back to a shell the session
    /// still reads `codex` and unattended submit/mail write into the shell.
    #[cfg(unix)]
    #[test]
    fn discovered_agent_replacing_a_preset_is_still_revocable() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let sid = "critic-1420r3-replace";
        let probe = ForegroundIdentityProbe::new(state.clone(), sid, "codex");
        set_identity(&state, sid, Some("claude"), true);
        assert_eq!(refresh_session_agent(&state, sid).as_deref(), Some("codex"));
        let flag = state
            .session_maps
            .session_states
            .get(sid)
            .unwrap()
            .agent_type_from_run_config;
        drop(probe);
        let _shell = ForegroundIdentityProbe::shell_root(state.clone(), sid, "bash");
        set_identity(&state, sid, Some("codex"), flag);
        refresh_session_agent(&state, sid);
        assert_eq!(
            identity(&state, sid),
            None,
            "discovered codex stayed pinned by the replaced preset's provenance"
        );
    }

    /// Catches: Linux `/proc/<pid>/exe` of a running native Claude whose version
    /// file was replaced by the updater reads `.../claude/versions/2.1.5 (deleted)`;
    /// the numeric-leaf check rejects the suffix and the agent is never classified.
    #[test]
    fn deleted_versioned_claude_exe_is_still_classified() {
        assert_eq!(
            classify_agent_name_or_path("/home/u/.local/share/claude/versions/2.1.5 (deleted)"),
            Some("claude")
        );
    }
}

mod preset_identity {
    //! Round-4 critic tests for story 1420-f3de: preset arming and revocation.

    use super::*;
    #[cfg(unix)]
    use crate::test_support::ForegroundIdentityProbe;

    #[cfg(unix)]
    fn flags(state: &AppState, sid: &str) -> (Option<String>, bool, bool) {
        let s = state.session_maps.session_states.get(sid).unwrap();
        (
            s.agent_type.clone(),
            s.agent_type_from_run_config,
            s.agent_foreground_observed,
        )
    }

    #[cfg(unix)]
    fn restore(state: &AppState, sid: &str, f: (Option<String>, bool, bool)) {
        let mut s = state.session_maps.session_states.get_mut(sid).unwrap();
        s.agent_type = f.0;
        s.agent_type_from_run_config = f.1;
        s.agent_foreground_observed = f.2;
    }

    /// Catches: a configured wrapper whose process name `classify_agent` does not
    /// recognise (alias/script/symlink) is never "observed", so the preset stays
    /// armed after the wrapper exits and unattended submit/mail write into the
    /// returned shell. Revocation must treat a non-shell foreground that carried
    /// the preset as evidence the agent started.
    #[cfg(unix)]
    #[test]
    fn unrecognised_configured_wrapper_is_revoked_when_the_shell_returns() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let sid = "critic-1420r4-wrapper";
        let probe = ForegroundIdentityProbe::shell_parent(state.clone(), sid, "mywrapper");
        restore(&state, sid, (Some("claude".into()), true, false));
        assert_eq!(
            refresh_session_agent(&state, sid).as_deref(),
            Some("claude")
        );
        let carried = flags(&state, sid);
        drop(probe);
        let _shell = ForegroundIdentityProbe::shell_root(state.clone(), sid, "bash");
        restore(&state, sid, carried);
        refresh_session_agent(&state, sid);
        assert_eq!(
            flags(&state, sid).0,
            None,
            "wrapper exited to a shell but the never-classified preset stayed armed"
        );
    }

    /// Catches: the preset is disarmed during shell startup (first poll sees the
    /// shell before the agent is launched), or revoked on the second shell poll.
    #[cfg(unix)]
    #[test]
    fn preset_survives_repeated_shell_polls_before_the_agent_starts() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let sid = "critic-1420r4-startup";
        let _shell = ForegroundIdentityProbe::shell_root(state.clone(), sid, "bash");
        restore(&state, sid, (Some("claude".into()), true, false));
        for _ in 0..3 {
            refresh_session_agent(&state, sid);
        }
        assert_eq!(flags(&state, sid), (Some("claude".into()), true, false));
    }

    /// Catches: after the observed preset agent exits and the identity is revoked,
    /// further shell polls or a hand-launched agent leave stale provenance: the
    /// relaunched agent must be discovered (not preset) and revocable again.
    #[cfg(unix)]
    #[test]
    fn revoked_preset_session_can_rediscover_and_revoke_a_hand_launched_agent() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let sid = "critic-1420r4-cycle";
        let agent = ForegroundIdentityProbe::new(state.clone(), sid, "claude");
        restore(&state, sid, (Some("claude".into()), true, false));
        refresh_session_agent(&state, sid);
        let after_agent = flags(&state, sid);
        assert_eq!(after_agent, (Some("claude".into()), true, true));
        drop(agent);

        let shell = ForegroundIdentityProbe::shell_root(state.clone(), sid, "bash");
        restore(&state, sid, after_agent);
        refresh_session_agent(&state, sid);
        refresh_session_agent(&state, sid);
        assert_eq!(flags(&state, sid), (None, false, true));
        let revoked = flags(&state, sid);
        drop(shell);

        let again = ForegroundIdentityProbe::new(state.clone(), sid, "claude");
        restore(&state, sid, revoked);
        assert_eq!(
            refresh_session_agent(&state, sid).as_deref(),
            Some("claude")
        );
        let relaunched = flags(&state, sid);
        assert!(!relaunched.1, "a rediscovered agent must not be a preset");
        drop(again);

        let _shell = ForegroundIdentityProbe::shell_root(state.clone(), sid, "bash");
        restore(&state, sid, relaunched);
        refresh_session_agent(&state, sid);
        assert_eq!(flags(&state, sid).0, None);
    }

    /// Catches: a preset for one agent (codex) replaced by a different discovered
    /// agent (claude) stays flagged as run-config and unrevocable.
    #[cfg(unix)]
    #[test]
    fn different_discovered_agent_drops_preset_and_is_revoked_on_exit() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let sid = "critic-1420r4-swap";
        let probe = ForegroundIdentityProbe::new(state.clone(), sid, "claude");
        restore(&state, sid, (Some("codex".into()), true, false));
        refresh_session_agent(&state, sid);
        let seen = flags(&state, sid);
        assert_eq!(seen, (Some("claude".into()), false, true));
        drop(probe);
        let _shell = ForegroundIdentityProbe::shell_root(state.clone(), sid, "bash");
        restore(&state, sid, seen);
        refresh_session_agent(&state, sid);
        assert_eq!(flags(&state, sid).0, None);
    }

    /// Catches: `(deleted)` handling that over-matches (a non-agent or shell path
    /// with the suffix classified as an agent), misses a bare agent path, or only
    /// strips the suffix from the versioned-claude layout.
    #[test]
    fn deleted_suffix_normalisation_is_exact() {
        assert_eq!(
            classify_agent_name_or_path("/usr/local/bin/claude (deleted)"),
            Some("claude")
        );
        assert_eq!(
            classify_agent_name_or_path("/usr/local/bin/codex (deleted)"),
            Some("codex")
        );
        assert_eq!(classify_agent_name_or_path("/usr/bin/bash (deleted)"), None);
        assert_eq!(
            classify_agent_name_or_path("/home/u/claude/versions/not-a-version (deleted)"),
            None
        );
        assert_eq!(
            classify_agent_name_or_path("/home/u/claude/other/2.1.5 (deleted)"),
            None
        );
    }
}

#[cfg(unix)]
mod shell_identity {
    //! Round-5 critic test for story 1420-f3de: shells missing from the shell list.

    use super::*;
    use crate::test_support::ForegroundIdentityProbe;

    type Flags = (Option<String>, bool, bool);

    /// Run one `refresh_session_agent` per foreground name, carrying the identity
    /// provenance across the replaced probe PTYs. Returns the stored flags after
    /// each step.
    #[cfg(unix)]
    fn run(sid: &str, initial: Flags, foregrounds: &[&str]) -> Vec<Flags> {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mut carried = initial;
        let mut out = Vec::new();
        for name in foregrounds {
            let probe = if *name == "ash" {
                ForegroundIdentityProbe::shell_root(state.clone(), sid, name)
            } else {
                ForegroundIdentityProbe::shell_parent(state.clone(), sid, name)
            };
            {
                let mut s = state.session_maps.session_states.get_mut(sid).unwrap();
                s.agent_type = carried.0.clone();
                s.agent_type_from_run_config = carried.1;
                s.agent_foreground_observed = carried.2;
            }
            refresh_session_agent(&state, sid);
            let s = state.session_maps.session_states.get(sid).unwrap();
            carried = (
                s.agent_type.clone(),
                s.agent_type_from_run_config,
                s.agent_foreground_observed,
            );
            drop(s);
            out.push(carried.clone());
            drop(probe);
        }
        out
    }

    /// Catches: a login shell missing from `SHELLS` (busybox/Alpine `ash`, common
    /// on remote Linux) is read as a non-shell helper, so the discovered agent's
    /// identity is retained forever after exit and the returned shell is submittable.
    #[cfg(unix)]
    #[test]
    fn busybox_ash_after_an_agent_revokes_identity() {
        let steps = run(
            "critic-1420r5-ash",
            (None, false, false),
            &["claude", "ash"],
        );
        assert_eq!(
            steps[1].0, None,
            "ash treated as a helper; agent identity stuck"
        );
    }
}

#[cfg(unix)]
mod root_identity {
    //! Round-6 critic tests for story 1420-f3de: spawn-root role contract.

    use super::*;
    use crate::test_support::ForegroundIdentityProbe;

    /// Catches: a Shell-role session whose root process was `exec`'d into the
    /// agent (`exec claude`, same pid, no job-control child) is read as "foreground
    /// == root pid, so the shell returned": the live agent is never detected and
    /// its identity is revoked, so submit/mail and state parsing are refused for a
    /// running agent.
    #[cfg(unix)]
    #[test]
    fn shell_root_exec_ed_into_an_agent_is_still_detected() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let sid = "critic-1420r6-exec-root";
        let _probe = ForegroundIdentityProbe::shell_root(state.clone(), sid, "claude");
        let returned = refresh_session_agent(&state, sid);
        let stored = state
            .session_maps
            .session_states
            .get(sid)
            .unwrap()
            .agent_type
            .clone();
        assert_eq!(
            (returned.as_deref(), stored.as_deref()),
            (Some("claude"), Some("claude")),
            "a Shell-role root running claude was treated as a returned shell"
        );
    }
}
