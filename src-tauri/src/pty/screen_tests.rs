#[test]
fn test_classify_agent_claude() {
    assert_eq!(classify_agent("claude"), Some("claude"));
}

#[test]
fn test_classify_agent_gemini() {
    assert_eq!(classify_agent("gemini"), Some("gemini"));
}

#[test]
fn test_classify_agent_aider() {
    assert_eq!(classify_agent("aider"), Some("aider"));
}

#[test]
fn test_classify_agent_codex() {
    assert_eq!(classify_agent("codex"), Some("codex"));
}

#[test]
fn test_classify_agent_opencode() {
    assert_eq!(classify_agent("opencode"), Some("opencode"));
}

#[test]
fn test_classify_agent_goose() {
    assert_eq!(classify_agent("goose"), Some("goose"));
}

#[test]
fn test_classify_agent_droid() {
    assert_eq!(classify_agent("droid"), Some("droid"));
}

/// Catches: the terminal ego tab keeping no agent_type, so submit is rejected
/// with `not_managed_agent` and no state is shown. Also pins the neighbours that
/// must NOT classify: a different binary sharing the prefix.
#[test]
fn test_classify_agent_ego() {
    assert_eq!(classify_agent("ego"), Some("ego"));
    assert_eq!(classify_agent("ego-0.1.0"), Some("ego"));
    assert_eq!(classify_agent("egoist"), None);
    assert_eq!(classify_agent("ego-acp"), None);
}

#[test]
fn test_classify_agent_unknown_returns_none() {
    assert_eq!(classify_agent("bash"), None);
    assert_eq!(classify_agent("zsh"), None);
    assert_eq!(classify_agent("node"), None);
    assert_eq!(classify_agent("python"), None);
    assert_eq!(classify_agent("vim"), None);
}

/// grok 1.0.5 ships `~/.grok/bin/grok` as a symlink to `grok-1.0.5`, and
/// `proc_pidpath` resolves the link. Missing the versioned basename left the
/// session with no `agent_type`, hence no ready-screen adapter, hence a tab
/// that never left BUSY.
#[test]
fn test_classify_agent_versioned_basename() {
    assert_eq!(classify_agent("grok-1.0.5"), Some("grok"));
    assert_eq!(classify_agent("claude-2.1.81"), Some("claude"));
    assert_eq!(classify_agent("codex-0.116"), Some("codex"));
    assert_eq!(
        classify_agent_name_or_path("/Users/me/.grok/bin/grok-1.0.5"),
        Some("grok")
    );
}

/// The suffix must start with a digit, so a hyphenated tool name keeps its
/// own identity and an unrelated binary is not promoted to an agent.
#[test]
fn test_classify_agent_version_strip_does_not_overreach() {
    assert_eq!(classify_agent("cursor-agent"), Some("cursor"));
    assert_eq!(classify_agent("grok-wrapper"), None);
    assert_eq!(classify_agent("not-grok"), None);
    assert_eq!(classify_agent("postgres-16"), None);
    // Catches: installer support misidentifies any agent-named ancestor.
    assert_eq!(classify_agent_name_or_path("/opt/pi/tool"), None);
    assert_eq!(classify_agent_name_or_path("/opt/claude/bin/tool"), None);
    assert_eq!(
        classify_agent_name_or_path("/opt/claude/versions/tool"),
        None
    );
    assert_eq!(
        classify_agent_name_or_path("/opt/claude/versions/2.1.87"),
        Some("claude")
    );
}

/// The ready-screen adapter is the whole point of detecting the agent: grok
/// runs as one long-lived foreground command, so OSC 133 marks the shell busy
/// once and only the screen can take it back to idle.
#[test]
fn test_grok_minimal_screen_is_ready_once_classified() {
    assert!(has_ready_screen_adapter(classify_agent("grok-1.0.5")));
    // Captured live from `grok --minimal` 1.0.5: no composer box, no
    // separators — a bare prompt glyph above the model status row.
    let rows: Vec<String> = vec![
        "Abbiamo scritto l’analisi completa in `grok-report.md`.".to_string(),
        "minimal · /help".to_string(),
        "\u{276F}".to_string(),
        "Grok 4.6 (high) · always-approve · 186K / 500K (37%) · ctrl+o transcript".to_string(),
    ];
    assert_eq!(
        detect_agent_screen_activity(Some("grok"), &rows),
        AgentScreenActivity::Ready
    );
}

#[test]
fn codex_working_is_presence_based_by_policy() {
    // Codex Working comes from the "esc to interrupt" status line presence,
    // NOT from movement: its TUI legitimately freezes for minutes while a
    // child process (cargo, git) runs. Accepted policy: prefer false-BUSY
    // over false-IDLE for Codex.
    let working = vec![
        "• Ran cargo test --workspace".to_string(),
        "• Working (2m 55s • esc to interrupt)".into(),
        "› Add tests".into(),
        "  gpt-5.6 high · ~/repo".into(),
    ];
    assert_eq!(
        detect_codex_screen_activity(&working),
        AgentScreenActivity::Working
    );
}

/// A pi screen must be identified by its own status row, not by any bottom row: without
/// this the adapter would report Ready for whatever happens to be on screen after pi exits.
#[test]
fn test_pi_screen_without_status_row_is_unknown() {
    let rows = vec![
        "$ ls".to_string(),
        "README.md  src".to_string(),
        "$ ".to_string(),
    ];
    assert_eq!(
        detect_agent_screen_activity(Some("pi"), &rows),
        AgentScreenActivity::Unknown
    );
}

#[test]
fn test_opencode_finished_turn_is_ready_and_interrupt_hint_wins() {
    assert_eq!(
        detect_agent_screen_activity(Some("opencode"), &opencode_finished_screen()),
        AgentScreenActivity::Ready
    );

    // Mid-turn opencode keeps the very same composer frame on screen; only the status
    // bar swaps the cwd for a `⬝`/`■` progress bar plus the interrupt hint.
    let mut working = opencode_finished_screen();
    let last = working.len() - 1;
    working[last] = "   \u{2B1D}\u{2B1D}\u{2B1D}\u{2B1D}\u{2B1D}\u{25A0}\u{25A0}\u{25A0}  esc interrupt        18.1K (9%)  ctrl+p commands".into();
    assert_eq!(
        detect_agent_screen_activity(Some("opencode"), &working),
        AgentScreenActivity::Working
    );
}

/// The welcome screen (before any turn) carries a different status bar — `tab agents`
/// plus a tip row and a `path:branch … version` row — and must still read Ready.
#[test]
fn test_opencode_welcome_screen_is_ready() {
    let rows = vec![
        "                    \u{2588}\u{2580}\u{2580}\u{2588} \u{2588}\u{2580}\u{2580}\u{2588} \u{2588}\u{2580}\u{2580}\u{2588}".into(),
        "                       \u{2503}".into(),
        "                       \u{2503}  Ask anything... \"Fix a TODO in the codebase\"".into(),
        "                       \u{2503}".into(),
        "                       \u{2503}  Build \u{00B7} Big Pickle OpenCode Zen".into(),
        format!("                       \u{2579}{}", "\u{2580}".repeat(78)),
        "                       tab agents  ctrl+p commands".into(),
        "                                \u{25CF} Tip Run /connect to add an AI provider".into(),
        "  ~/Gits/personal/tuicommander:main                                        1.18.5".into(),
    ];
    assert_eq!(
        detect_agent_screen_activity(Some("opencode"), &rows),
        AgentScreenActivity::Ready
    );
}

/// The interrupt hint survives a tool phase — captured while opencode ran
/// `sleep 20 && echo done` — which is precisely when a false idle would let
/// auto-standby SIGSTOP the session.
#[test]
fn test_opencode_tool_phase_is_working() {
    let mut rows = opencode_finished_screen();
    rows.insert(2, "  \u{2503}  \u{283C} sleep 20 && echo done".into());
    rows.insert(3, "     \u{25A3}  Build \u{00B7} Big Pickle".into());
    let last = rows.len() - 1;
    rows[last] = "   \u{2B1D}\u{2B1D}\u{2B1D}\u{2B1D}\u{25A0}\u{25A0}\u{25A0}\u{25A0}  esc interrupt        18.1K (9%)  ctrl+p commands".into();
    assert_eq!(
        detect_agent_screen_activity(Some("opencode"), &rows),
        AgentScreenActivity::Working
    );
}

/// Readiness must rest on OpenCode's own frame, not on whatever happens to be on
/// screen: a plain shell — and a frame whose status bar has not been painted — are
/// both Unknown rather than Ready.
#[test]
fn test_opencode_requires_its_own_frame_and_status_bar() {
    let shell = vec![
        "$ ls".to_string(),
        "README.md  src".to_string(),
        "$ ".to_string(),
    ];
    assert_eq!(
        detect_agent_screen_activity(Some("opencode"), &shell),
        AgentScreenActivity::Unknown
    );

    // Frame close row without any `┃` frame row above it is not an OpenCode composer.
    let close_only = vec![
        format!("  \u{2579}{}", "\u{2580}".repeat(98)),
        "   /Users/x  ctrl+p commands".to_string(),
    ];
    assert_eq!(
        detect_agent_screen_activity(Some("opencode"), &close_only),
        AgentScreenActivity::Unknown
    );

    // Half-painted screen: frame present, status bar not yet drawn.
    let mut unpainted = opencode_finished_screen();
    unpainted.pop();
    assert_eq!(
        detect_agent_screen_activity(Some("opencode"), &unpainted),
        AgentScreenActivity::Unknown
    );
}

#[test]
fn test_agent_screen_adapter_baselines() {
    // Claude and Codex have presence-based active markers because both can
    // freeze or leave a composer visible during long tools. Gemini/Aider
    // remain prompt-based and use movement to hold BUSY.
    let claude_working = vec!["✻ Cogitating… (12s)".into()];
    let claude_ready = vec!["Answer complete".into(), "❯".into()];
    let gemini_working_prompt_visible = vec![
        "⠴ Checking files… (esc to cancel, 14s)".into(),
        "────────────────────────".into(),
        "> Type your message".into(),
    ];
    let gemini_ready = vec!["> Type your message".into()];
    let aider_working = vec!["█░  Waiting for model".into()];
    let aider_ready = vec!["Tokens: 10k sent".into(), ">".into()];
    let grok_working = vec![
        "❯ Ask anything".into(),
        "⠹ Responding… 12s [stop]".into(),
        "⌘ Grok 4.3 OpenRouter · Medium effort".into(),
    ];
    let grok_ready = vec![
        "Done.".into(),
        "❯ Ask anything".into(),
        "⌘ Grok 4.3 OpenRouter · Medium effort".into(),
    ];

    for (agent, rows, expected) in [
        ("claude", claude_working, AgentScreenActivity::Working),
        ("claude", claude_ready, AgentScreenActivity::Ready),
        (
            "gemini",
            gemini_working_prompt_visible,
            AgentScreenActivity::Ready,
        ),
        ("gemini", gemini_ready, AgentScreenActivity::Ready),
        ("aider", aider_working, AgentScreenActivity::Unknown),
        ("aider", aider_ready, AgentScreenActivity::Ready),
        ("grok", grok_working, AgentScreenActivity::Working),
        ("grok", grok_ready, AgentScreenActivity::Ready),
    ] {
        assert_eq!(detect_agent_screen_activity(Some(agent), &rows), expected);
    }
}

fn process(pid: u32, parent_pid: u32, name: &str, command: &str) -> ProcessTreeEntry {
    ProcessTreeEntry {
        pid,
        parent_pid,
        name: name.to_string(),
        command: command.to_string(),
        age_seconds: None,
    }
}

/// `sudo su` as the OS actually reports it: sudo re-execs itself, and on
/// macOS the second hop allocates its own PTY, so the inner shell is only
/// reachable through the parent chain.
fn sudo_su_tree() -> Vec<ProcessTreeEntry> {
    vec![
        process(100, 1, "zsh", "/bin/zsh"),
        process(200, 100, "sudo", "sudo su"),
        process(201, 200, "sudo", "sudo su"),
        process(202, 201, "su", "su"),
        process(203, 202, "sh", "sh"),
    ]
}

/// A process the snapshot could age. Ageless entries keep the name list as
/// the only rule, which is what every pre-existing case here asserts.
fn aged_process(
    pid: u32,
    parent_pid: u32,
    name: &str,
    command: &str,
    age_seconds: u64,
) -> ProcessTreeEntry {
    ProcessTreeEntry {
        age_seconds: Some(age_seconds),
        ..process(pid, parent_pid, name, command)
    }
}

#[test]
fn sanitized_background_command_keeps_agent_working_across_adapters() {
    // Sanitized from the 2026-07-19 live Codex sequence. The same lifecycle
    // contract applies to Claude: a ready composer is not proof that an
    // autonomous background command has completed.
    for (index, agent) in ["codex", "claude"].into_iter().enumerate() {
        let session_root = 100 + index as u32 * 100;
        let agent_pid = session_root + 1;
        let processes = vec![
            process(session_root, 1, "zsh", "zsh"),
            process(agent_pid, session_root, agent, agent),
            process(
                agent_pid + 1,
                agent_pid,
                "rtk",
                "rtk env CARGO_BUILD_JOBS=4 cargo test --locked -p agent2-core",
            ),
            process(agent_pid + 2, agent_pid + 1, "cargo", "cargo test --locked"),
            process(
                agent_pid + 3,
                agent_pid + 2,
                "agent2_core-test",
                "target/debug/deps/agent2_core-test",
            ),
        ];
        let root = agent_process_root(session_root, agent, &processes).unwrap();
        assert_eq!(root, agent_pid, "{agent} adapter root");
        assert!(
            has_meaningful_descendant(root, &processes),
            "{agent} must retain autonomous work while cargo descendants live"
        );

        let silence = replay_sanitized_agent_trace(
            agent,
            &[
                SanitizedTraceStep::Submit,
                SanitizedTraceStep::RealActivity,
                SanitizedTraceStep::ReadyScreen,
            ],
        );
        assert!(
            silence.idle_confirmed(),
            "{agent} composer is terminal-ready"
        );

        let state = crate::state::tests_support::make_test_app_state();
        let sid = format!("background-{agent}");
        state.session_maps.session_states.insert(
            sid.clone(),
            crate::state::SessionState {
                spawn_root_role: crate::state::SpawnRootRole::DirectProgram,
                agent_type: Some(agent.to_string()),
                background_work: true,
                ..Default::default()
            },
        );
        state
            .session_maps
            .shell_states
            .insert(sid.clone(), std::sync::atomic::AtomicU8::new(SHELL_IDLE));
        state
            .session_maps
            .silence_states
            .insert(sid.clone(), Arc::new(Mutex::new(silence)));

        let snapshot = state.session_state_with_shell(&sid).unwrap();
        assert_eq!(snapshot.shell_state.as_deref(), Some("idle"));
        assert_eq!(snapshot.agent_state.as_deref(), Some("working"));
        assert!(
            should_inject_now(&state, &sid),
            "terminal-ready must remain usable independently of task lifecycle"
        );
    }
}

#[test]
fn daemons_started_with_the_agent_are_not_background_work() {
    // Sanitized from a live 14-session instance on 2026-08-23, where every
    // agent reported `working` forever. Neither name here can go on the
    // helper list: `codex-code-mode-host` shipped with Codex 0.149.0 and the
    // next release may rename it, and `npm` must keep meaning work.
    let agent_age = 129_050;
    let processes = vec![
        aged_process(10, 1, "codex", "codex", agent_age),
        aged_process(
            11,
            10,
            "codex-code-mode-host",
            "/opt/homebrew/Caskroom/codex/0.149.0/bin/codex-code-mode-host",
            agent_age - 18,
        ),
        aged_process(
            12,
            10,
            "npm",
            "npm exec @upstash/context7-mcp",
            agent_age - 1,
        ),
    ];
    assert!(
        !has_meaningful_descendant(10, &processes),
        "daemons that came up with the agent are plumbing"
    );

    // The same two names, spawned by a turn instead of at startup.
    let mut spawned_by_a_turn = processes.clone();
    spawned_by_a_turn.push(aged_process(20, 10, "npm", "npm run build", 12));
    assert!(
        has_meaningful_descendant(10, &spawned_by_a_turn),
        "work must stay visible under a name the startup window also sees"
    );

    // One second past the window is already work.
    let mut just_outside = processes;
    just_outside.push(aged_process(
        21,
        10,
        "cargo",
        "cargo test",
        agent_age - AGENT_STARTUP_WINDOW_SECS - 1,
    ));
    assert!(has_meaningful_descendant(10, &just_outside));
}

#[cfg(not(windows))]
#[test]
fn elapsed_time_field_parses_every_ps_shape() {
    assert_eq!(parse_elapsed_time("05:12"), Some(312));
    assert_eq!(parse_elapsed_time("01:00:00"), Some(3600));
    assert_eq!(parse_elapsed_time("2-13:45:02"), Some(222_302));
    // `ps` never emits a bare second count, so one is not a valid reading.
    assert_eq!(parse_elapsed_time("42"), None);
    assert_eq!(parse_elapsed_time("-"), None);
    assert_eq!(parse_elapsed_time("1:2:3:4"), None);
    assert_eq!(parse_elapsed_time("aa:bb"), None);
}

#[cfg(not(windows))]
#[test]
fn timed_caffeinate_is_not_background_work_with_authoritative_argv() {
    let processes = vec![
        process(10, 1, "claude", "claude"),
        process(11, 10, "caffeinate", "caffeinate -i -t 300"),
    ];
    assert!(!has_meaningful_descendant(10, &processes));
}

#[test]
fn timed_caffeinate_helper_does_not_hide_wrapped_work() {
    for command in ["caffeinate -i -t 300", "/usr/bin/caffeinate -t 300 -i"] {
        assert!(is_standalone_timed_caffeinate(command));
        assert!(is_persistent_agent_helper_with_command_line(
            &process(11, 10, "caffeinate", command),
            true
        ));
    }
    for command in [
        "caffeinate -i -t 0",
        "caffeinate -i",
        "caffeinate -i cargo test",
        "caffeinate -i -t 300 cargo test",
    ] {
        assert!(!is_standalone_timed_caffeinate(command));
    }

    let wrapped_work = vec![
        process(10, 1, "claude", "claude"),
        process(11, 10, "caffeinate", "caffeinate -i cargo test"),
        process(12, 11, "cargo", "cargo test"),
    ];
    assert!(has_meaningful_descendant(10, &wrapped_work));
}

#[cfg(not(windows))]
#[test]
fn background_snapshot_macos_truncated_comm_fixture_excludes_helpers() {
    // Sanitized from macOS `ps -ww -axo pid=,ppid=,etime=,comm=,args=`
    // output. Darwin may truncate `comm` while unlimited-width `args`
    // retains the executable path needed to identify persistent integration
    // helpers. `codex-code-mode-host` is on no name list and is excluded
    // purely by having started with the agent.
    const MACOS_PS: &str = r#"
  700     1    01:00:05 /bin/zsh         /bin/zsh
  701   700    01:00:00 /Applications/C  /Applications/Codex.app/Contents/MacOS/codex
  702   701       59:58 /Users/boss/.lo  /Users/boss/.local/bin/mdkb serve
  703   701       59:58 /Users/boss/.ca  /Users/boss/.cache/tuic/tuic-bridge --stdio
  704   701       59:58 /opt/homebrew/b  /opt/homebrew/bin/node /Users/boss/.cache/tuic/node_repl.js
  705   702       59:57 sqlite-worker    sqlite-worker
  706   701       59:45 /opt/homebrew/Ca /opt/homebrew/Caskroom/codex/0.149.0/bin/codex-code-mode-host
"#;
    let processes = parse_process_tree_snapshot(true, MACOS_PS).unwrap();
    assert_eq!(
        processes[0].age_seconds,
        Some(3605),
        "the elapsed column must survive the truncated-comm layout"
    );
    assert_eq!(agent_process_root(700, "codex", &processes), Some(701));
    assert!(!has_meaningful_descendant(701, &processes));

    let mut with_turn_work = processes;
    with_turn_work.push(aged_process(707, 701, "cargo", "cargo test", 30));
    assert!(has_meaningful_descendant(701, &with_turn_work));
}

#[test]
fn version_named_claude_path_is_the_agent_root() {
    let processes = vec![
        process(10, 1, "zsh", "zsh"),
        process(
            11,
            10,
            "/Users/test/.local/share/claude/versions/2.1.87",
            "2.1.87",
        ),
        process(12, 11, "cargo", "cargo test"),
    ];
    assert_eq!(agent_process_root(10, "claude", &processes), Some(11));
    assert_eq!(
        background_work_from_snapshot(10, "claude", &processes),
        Some(true)
    );
}

#[test]
fn wrapper_is_not_counted_as_permanent_agent_work() {
    let idle = vec![
        process(20, 1, "claude-wrapper", "claude-wrapper"),
        process(21, 20, "/opt/claude/versions/2.1.87", "2.1.87"),
    ];
    assert_eq!(agent_process_root(20, "claude", &idle), Some(21));
    assert_eq!(
        background_work_from_snapshot(20, "claude", &idle),
        Some(false)
    );

    let custom_alias = vec![
        process(30, 1, "C2", "C2"),
        process(31, 30, "mdkb", "mdkb serve"),
    ];
    assert_eq!(agent_process_root(30, "claude", &custom_alias), Some(30));
    assert_eq!(
        background_work_from_snapshot(30, "claude", &custom_alias),
        Some(false)
    );
    let mut active_alias = custom_alias;
    active_alias.push(process(32, 30, "cargo", "cargo test"));
    assert_eq!(
        background_work_from_snapshot(30, "claude", &active_alias),
        Some(true)
    );
}

#[cfg(not(windows))]
#[test]
fn process_snapshot_rejects_nonzero_and_malformed_output() {
    assert!(parse_process_tree_snapshot(false, "10 1 05:12 zsh zsh").is_none());
    assert!(parse_process_tree_snapshot(true, "10 invalid 05:12 zsh zsh").is_none());
    assert!(parse_process_tree_snapshot(true, "10 1 05:12").is_none());
    // An unreadable elapsed column costs the age, not the whole snapshot:
    // the name list still has to work.
    let ageless = parse_process_tree_snapshot(true, "10 1 ? zsh zsh").unwrap();
    assert_eq!(ageless[0].age_seconds, None);
}

#[test]
fn process_snapshot_cache_is_shared_across_sessions() {
    let cache = ProcessSnapshotCache::default();
    cache.store(Some(vec![
        process(10, 1, "codex", "codex"),
        process(11, 10, "cargo", "cargo test"),
        process(20, 1, "claude", "claude"),
    ]));
    let (first_generation, first) = cache.load().unwrap();
    let (second_generation, second) = cache.load().unwrap();
    assert_eq!(first_generation, second_generation);
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(
        background_work_from_snapshot(10, "codex", &first),
        Some(true)
    );
    assert_eq!(
        background_work_from_snapshot(20, "claude", &second),
        Some(false)
    );
}

#[test]
// Catches: a reused ready probe or a missing second idle notice after same-epoch work.
fn same_epoch_working_evidence_requires_a_new_ready_probe_boundary() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "background-same-epoch-ready";
    let parent_id = "background-same-epoch-parent";
    agent_session(&state, child_id, SHELL_BUSY);
    state
        .session_maps
        .session_states
        .get_mut(child_id)
        .unwrap()
        .agent_type = Some("codex".into());
    state
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    let silence = state
        .session_maps
        .silence_states
        .get(child_id)
        .unwrap()
        .clone();

    state
        .process_snapshot_cache
        .store(Some(vec![process(10, 1, "codex", "codex")]));
    silence.lock().screen_ready_pending_since =
        Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    assert!(
        !try_timer_idle_transition(
            &state,
            &silence,
            child_id,
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
        child_id,
        10,
        "codex",
        0,
        state.process_snapshot_cache.load(),
    ));
    assert!(
        try_timer_idle_transition(
            &state,
            &silence,
            child_id,
            AgentScreenActivity::Ready,
            Some("codex"),
            Some(0),
        )
        .transitioned
    );
    let first_notice = state.agent_inbox.get(parent_id).unwrap();
    assert_eq!(first_notice.len(), 1);
    let first_notice_timestamp = first_notice.front().unwrap().timestamp;
    drop(first_notice);
    assert_eq!(
        state
            .session_maps
            .session_states
            .get(child_id)
            .unwrap()
            .background_probe_satisfied_turn_epoch,
        Some(0)
    );

    apply_working_evidence(&state, &silence, child_id, now_epoch_ms(), "working-screen");
    {
        let session = state.session_maps.session_states.get(child_id).unwrap();
        assert_eq!(session.background_probe_satisfied_turn_epoch, None);
        assert_eq!(session.background_probe_turn_epoch, None);
        assert_eq!(session.background_probe_after_generation, None);
        assert_eq!(session.background_snapshot_generation, 2);
        assert!(!session.background_work);
    }

    silence.lock().screen_ready_pending_since =
        Some(std::time::Instant::now() - AGENT_READY_CONFIRM);
    assert!(
        !try_timer_idle_transition(
            &state,
            &silence,
            child_id,
            AgentScreenActivity::Ready,
            Some("codex"),
            Some(0),
        )
        .transitioned
    );
    {
        let session = state.session_maps.session_states.get(child_id).unwrap();
        assert_eq!(session.background_probe_turn_epoch, Some(0));
        assert_eq!(session.background_probe_after_generation, Some(2));
        assert_eq!(session.background_probe_satisfied_turn_epoch, None);
    }

    state.process_snapshot_cache.store(Some(vec![
        process(10, 1, "codex", "codex"),
        process(11, 10, "rtk", "rtk cargo test"),
        process(12, 11, "cargo", "cargo test"),
        process(13, 12, "rustc", "rustc --crate-name tuicommander"),
    ]));
    assert!(refresh_background_work_from_cached_snapshot(
        &state,
        child_id,
        10,
        "codex",
        0,
        state.process_snapshot_cache.load(),
    ));
    let working = state.session_state_with_shell(child_id).unwrap();
    assert_eq!(working.agent_state.as_deref(), Some("working"));
    assert!(working.background_work);

    assert!(
        try_timer_idle_transition(
            &state,
            &silence,
            child_id,
            AgentScreenActivity::Ready,
            Some("codex"),
            Some(0),
        )
        .transitioned
    );
    assert_eq!(state.agent_inbox.get(parent_id).unwrap().len(), 1);

    state
        .process_snapshot_cache
        .store(Some(vec![process(10, 1, "codex", "codex")]));
    assert!(refresh_background_work_from_cached_snapshot(
        &state,
        child_id,
        10,
        "codex",
        0,
        state.process_snapshot_cache.load(),
    ));
    let final_notice = state.agent_inbox.get(parent_id).unwrap();
    assert_eq!(final_notice.len(), 1);
    let final_notice_timestamp = final_notice.front().unwrap().timestamp;
    assert!(final_notice_timestamp > first_notice_timestamp);
    drop(final_notice);
    let content: serde_json::Value = serde_json::from_str(
        &state
            .agent_inbox
            .get(parent_id)
            .unwrap()
            .back()
            .unwrap()
            .content,
    )
    .unwrap();
    assert_eq!(content["state"], "idle");

    state
        .process_snapshot_cache
        .store(Some(vec![process(10, 1, "codex", "codex")]));
    assert!(!refresh_background_work_from_cached_snapshot(
        &state,
        child_id,
        10,
        "codex",
        0,
        state.process_snapshot_cache.load(),
    ));
    let retained_notice = state.agent_inbox.get(parent_id).unwrap();
    assert_eq!(retained_notice.len(), 1);
    assert_eq!(
        retained_notice.front().unwrap().timestamp,
        final_notice_timestamp
    );
}

#[test]
fn already_busy_working_evidence_invalidates_only_probe_boundaries() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "background-already-busy";
    agent_session(&state, child_id, SHELL_BUSY);
    {
        let mut session = state.session_maps.session_states.get_mut(child_id).unwrap();
        session.agent_type = Some("codex".into());
        session.background_work = true;
        session.background_snapshot_generation = 9;
        session.background_probe_turn_epoch = Some(0);
        session.background_probe_after_generation = Some(8);
        session.background_probe_satisfied_turn_epoch = Some(0);
    }
    let silence = state
        .session_maps
        .silence_states
        .get(child_id)
        .unwrap()
        .clone();

    apply_working_evidence(&state, &silence, child_id, now_epoch_ms(), "working-screen");
    {
        let session = state.session_maps.session_states.get(child_id).unwrap();
        assert_eq!(session.background_probe_turn_epoch, None);
        assert_eq!(session.background_probe_after_generation, None);
        assert_eq!(session.background_probe_satisfied_turn_epoch, None);
        assert!(session.background_work);
        assert_eq!(session.background_snapshot_generation, 9);
    }

    {
        let mut session = state.session_maps.session_states.get_mut(child_id).unwrap();
        session.background_probe_turn_epoch = Some(0);
        session.background_probe_after_generation = Some(9);
        session.background_probe_satisfied_turn_epoch = Some(0);
    }
    transition_explicit_shell_state_with_hook(&state, child_id, SHELL_BUSY, "busy", true, || {});
    let session = state.session_maps.session_states.get(child_id).unwrap();
    assert_eq!(session.background_probe_turn_epoch, None);
    assert_eq!(session.background_probe_after_generation, None);
    assert_eq!(session.background_probe_satisfied_turn_epoch, None);
    assert!(session.background_work);
    assert_eq!(session.background_snapshot_generation, 9);
    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(child_id)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_BUSY
    );
}

#[test]
fn background_snapshot_refresher_is_demand_gated_without_sleeping() {
    let state = crate::state::tests_support::make_test_app_state();
    let calls = std::sync::atomic::AtomicUsize::new(0);
    assert!(!refresh_process_snapshot_if_demanded(&state, || {
        calls.fetch_add(1, Ordering::Relaxed);
        None
    }));
    assert_eq!(calls.load(Ordering::Relaxed), 0);

    let session_id = "background-demand";
    agent_session(&state, session_id, SHELL_BUSY);
    state
        .session_maps
        .session_states
        .get_mut(session_id)
        .unwrap()
        .agent_type = Some("codex".into());
    state
        .session_maps
        .session_states
        .get_mut(session_id)
        .unwrap()
        .background_probe_turn_epoch = Some(0);
    state
        .session_maps
        .session_states
        .get_mut(session_id)
        .unwrap()
        .background_probe_after_generation = Some(0);

    assert!(refresh_process_snapshot_if_demanded(&state, || {
        calls.fetch_add(1, Ordering::Relaxed);
        Some(vec![process(10, 1, "codex", "codex")])
    }));
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert!(refresh_background_work_from_cached_snapshot(
        &state,
        session_id,
        10,
        "codex",
        0,
        state.process_snapshot_cache.load(),
    ));
    assert!(!process_snapshot_is_demanded(&state));
    assert!(!refresh_process_snapshot_if_demanded(&state, || {
        calls.fetch_add(1, Ordering::Relaxed);
        None
    }));
    assert_eq!(calls.load(Ordering::Relaxed), 1);
}

#[test]
fn pty_spawn_env_does_not_inherit_tuic_build_context() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "pty::tests::pty_spawn_env_child_checks_inherited_build_context",
        ])
        .env("TUIC_TEST_PTY_BUILD_ENV", "1")
        .env("CARGO_TARGET_DIR", "/tuic-dev-build-target")
        .env("CARGO_MANIFEST_DIR", "/tuic/src-tauri")
        .env("CARGO_MANIFEST_PATH", "/tuic/src-tauri/Cargo.toml")
        .env("CARGO_MANIFEST_LINKS", "tuic-native")
        .env("CARGO_PKG_NAME", "tuicommander")
        .env("OUT_DIR", "/tuic-dev-build-target/debug/build/out")
        .env("RUSTDOC", "/tuic-dev-rustdoc")
        .env("CARGO_INCREMENTAL", "0")
        .env("RUSTC_WRAPPER", "/tuic-dev-mbx-shim")
        .env("RUSTC_WORKSPACE_WRAPPER", "/tuic-dev-workspace-shim")
        .env("HOST_CC", "/tuic-dev-cc")
        .env("HOST_CXX", "/tuic-dev-cxx")
        .env("MBX_SOCKET", "/tuic-dev-mbx.sock")
        .env("MBX_FUTURE_BUILD_KEY", "tuic-only")
        .env("CARGO", "/tuic-dev-cargo")
        .env("CARGO_PRIMARY_PACKAGE", "1")
        .env("CARGO_BIN_NAME", "tuicommander")
        .env("CARGO_CRATE_NAME", "tuicommander")
        .env("CARGO_MAKEFLAGS", "--jobserver-auth=3,4")
        .env("CARGO_TARGET_TMPDIR", "/tuic-dev-target/tmp")
        .env(
            "CARGO_BIN_EXE_tuicommander",
            "/tuic-dev-target/tuicommander",
        )
        .env("CARGO_FEATURE_DESKTOP", "1")
        .env("CARGO_CFG_TARGET_OS", "macos")
        .env("RUSTFLAGS", "-Ctarget-cpu=native")
        .env("RUSTC", "/tuic-dev-rustc")
        .env("RUSTC_LINKER", "/tuic-dev-linker")
        .env("DEP_TUIC_NATIVE_PATH", "/tuic-dev-native")
        .env("CARGO_ENCODED_RUSTFLAGS", "-Ctarget-cpu=native")
        .env("TARGET", "aarch64-apple-darwin")
        .env("HOST", "aarch64-apple-darwin")
        .env("PROFILE", "dev")
        .env("NUM_JOBS", "12")
        .env("OPT_LEVEL", "0")
        .env("DEBUG", "true")
        .env("CARGO_HOME", "/user/cargo")
        .env("CARGO_TERM_COLOR", "always")
        .env("TUIC_TEST_USER_ENV", "keep-me")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "isolated spawn-env check failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("1 passed"),
        "spawn-env child was not selected: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[cfg(unix)]
impl std::io::Write for FailingWriter {
    fn write(&mut self, _bytes: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("injected PTY failure"))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(unix)]
impl std::io::Write for FailsAtWrite {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let call = self.calls;
        self.calls += 1;
        if call < self.fail_at || (self.partial && call == self.fail_at) {
            return Ok(if call < self.fail_at {
                bytes.len()
            } else {
                bytes.len().min(2)
            });
        }
        Err(std::io::Error::other("injected PTY failure"))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// A goose screen whose footer has not been painted yet must read Unknown, not
/// Ready. Ready is the expensive direction to get wrong: it is what lets
/// auto-standby SIGSTOP a live turn.
#[test]
fn goose_screen_without_a_footer_is_unknown() {
    let rows: Vec<String> = [
        "  __( O)>  ● new session · ollama gemma4:12b-mlx",
        "   \\____)",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    assert_eq!(
        detect_agent_screen_activity(Some("goose"), &rows),
        AgentScreenActivity::Unknown
    );
}

// ── process-stats helpers ───────────────────────────────────────

#[cfg(not(windows))]
#[test]
fn query_process_stats_reports_own_process() {
    let own = std::process::id();
    let map = query_process_stats(&[own]);
    assert!(map.contains_key(&own), "ps must report our own pid");
    let (rss, _cpu) = map[&own];
    assert!(
        rss > 0,
        "resident set size of a live process must be positive"
    );
}

#[cfg(not(windows))]
#[test]
fn process_parent_map_covers_the_live_process_table() {
    let parent_map = process_parent_map().expect("walking the live process table must succeed");
    let own = std::process::id();
    assert!(
        parent_map.values().any(|children| children.contains(&own)),
        "the shared map must place our own pid under its parent"
    );
}

#[cfg(not(windows))]
#[test]
fn parse_process_parent_map_skips_unreadable_rows() {
    let parent_map = parse_process_parent_map(
        "  PID  PPID\n    1     0\n  100     1\nbogus row\n  101   100\n  102\n  103   100\n",
    );
    assert_eq!(
        parent_map.get(&1).map(Vec::as_slice),
        Some([100].as_slice())
    );
    assert_eq!(
        parent_map.get(&100).map(Vec::as_slice),
        Some([101, 103].as_slice()),
        "a header, a word row and a truncated row must not drop the rows around them"
    );
    assert!(
        !parent_map.contains_key(&102),
        "a row without a parent column contributes nothing"
    );
}

/// The refresh queries `ps` once and walks one subtree per session, so the
/// walk must be a pure function of the shared map: each root gets its own
/// transitive closure, and an extra root costs no extra query.
#[cfg(not(windows))]
#[test]
fn descendants_are_transitive_and_distributed_per_root() {
    let parent_map = parse_process_parent_map(
        "  PID  PPID\n  100     1\n  101   100\n  102   101\n  200     1\n  201   200\n",
    );
    let mut first = descendants_from_parent_map(&parent_map, 100);
    first.sort_unstable();
    assert_eq!(first, vec![101, 102], "the walk must reach grandchildren");
    let mut second = descendants_from_parent_map(&parent_map, 200);
    second.sort_unstable();
    assert_eq!(second, vec![201], "a second root sees only its own subtree");
    assert!(
        descendants_from_parent_map(&parent_map, 999).is_empty(),
        "an unknown root has no descendants"
    );
}

#[cfg(not(windows))]
#[test]
fn descendants_walk_terminates_on_a_self_parented_row() {
    let parent_map = parse_process_parent_map("  0     0\n  100     0\n");
    assert_eq!(
        descendants_from_parent_map(&parent_map, 0),
        vec![100],
        "a self-parented row must neither spin the walk nor make the root its own descendant"
    );
}

#[cfg(not(windows))]
#[test]
fn process_tree_snapshot_reports_own_process() {
    let own = std::process::id();
    let snapshot = process_tree_snapshot().expect("ps process-tree snapshot");
    let mine = snapshot
        .iter()
        .find(|process| process.pid == own)
        .expect("the process-tree parser must preserve live PIDs");
    // The startup window silently degrades to the old name-only rule when
    // ages are missing, so this platform's `etime` column has to be proven
    // readable here rather than inferred from the fixture tests.
    assert!(
        mine.age_seconds.is_some(),
        "this platform's ps must yield a parsable elapsed time"
    );
}

/// Catches: the idle released by the dismissal is not confirmed idle, so the
/// queued message of the stuck-queue symptom still waits for the silence timer.
/// Nothing here calls `confirm_idle` by hand.
#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn critic_1302_queued_injection_flushes_after_a_dismissed_question() {
    use std::collections::VecDeque;
    let sid = "critic-1302-queue";
    let (state, _silence, _processor) = replay_claude_askuser_esc(sid).await;
    // Captured agent output owns this recording composer; the shell
    // child is only its PTY holder, not a foreground-detection scenario.
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .spawn_root_role = crate::state::SpawnRootRole::DirectProgram;
    let bytes = insert_recording_session(&state, sid);
    let mut queue = VecDeque::new();
    queue.push_back(crate::state::PendingInjection::notice("after esc"));
    state.pending_injections.insert(sid.to_string(), queue);

    flush_pending_injections(&state, sid);
    for _ in 0..300 {
        if bytes.lock().unwrap().ends_with(b"\r") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(
        String::from_utf8_lossy(&bytes.lock().unwrap()).contains("after esc"),
        "the queued injection must reach the composer once Esc released BUSY"
    );
}

fn assert_opencode_mini_not_ready(last: &str) {
    assert_ne!(
        detect_agent_screen_activity(Some("opencode"), &opencode_mini_rows(last)),
        AgentScreenActivity::Ready,
        "last painted row {last:?} must not read as an idle OpenCode"
    );
}

/// Catches: a permission dialog below a running status row is read as Ready because the
/// label row is still on screen.
#[test]
fn opencode_mini_permission_prompt_under_working_status_is_not_ready() {
    let rows = vec![
        " BUILD  \u{2B1D}\u{25A0}\u{25A0}\u{25A0}\u{25A0}\u{25A0}\u{25A0} esc interrupt"
            .to_string(),
        "  Permission required: bash".to_string(),
        "  Allow once   Allow always   Reject".to_string(),
    ];
    assert_ne!(
        detect_agent_screen_activity(Some("opencode"), &rows),
        AgentScreenActivity::Ready
    );
}

/// Catches: trailing blank rows below the status row hide it, so an idle mini screen
/// never reads Ready and the queue never drains.
#[test]
fn opencode_mini_ready_status_row_survives_trailing_blank_rows() {
    let rows = vec![
        "  done".to_string(),
        " BUILD                                 52.9K (26%) \u{00B7} ctrl+p cmd".to_string(),
        String::new(),
        "   ".to_string(),
    ];
    assert_eq!(
        detect_agent_screen_activity(Some("opencode"), &rows),
        AgentScreenActivity::Ready
    );
}

#[test]
fn critic_mini_numeric_tool_output_with_uppercase_word_is_not_ready() {
    // Catches: an `ERROR 404` / `FAIL 2 (50%)` / `DONE 100%` last row passes the
    // "uppercase label + numeric tokens" rule and reads Ready on a live turn.
    for row in [
        "ERROR 404",
        "FAIL 2 (50%)",
        "DONE 100%",
        "TOTAL 1,234.56",
        "HTTP 200",
    ] {
        assert_eq!(
            detect_opencode_screen_activity(&critic_mini_rows(row)),
            AgentScreenActivity::Unknown,
            "row {row:?} is tool output, not the status row"
        );
    }
}

#[test]
fn critic_mini_bare_tool_word_build_percent_is_not_ready() {
    // Catches: progress output `BUILD 45%` read as the idle status row.
    assert_eq!(
        detect_opencode_screen_activity(&critic_mini_rows("BUILD 45%")),
        AgentScreenActivity::Unknown
    );
}

#[test]
fn critic_mini_real_ready_forms_are_ready() {
    // Catches: an observed idle form the whole-row rule rejects, stalling the drain
    // forever. Every row is a shape captured live from OpenCode 1.18.30 --mini: the fresh
    // session (no usage yet), the wide idle row with context usage, and the narrow label.
    for row in [
        " BUILD                                 52.9K (26%) \u{00B7} ctrl+p cmd",
        " BUILD                                                       ctrl+p cmd",
        " BUILD",
    ] {
        assert_eq!(
            detect_opencode_screen_activity(&critic_mini_rows(row)),
            AgentScreenActivity::Ready,
            "row {row:?}"
        );
    }
}

#[test]
fn critic_mini_unobserved_forms_stay_unknown() {
    // Catches: widening the status-row shape for forms nobody captured. A false Ready
    // stops a live turn; a false Unknown only delays the queue. Not seen live: a cost
    // token, a lowercase `k`, a user agent label bare or two words, usage without hints.
    for row in [
        " BUILD  950 (1%) \u{00B7} $0.12 \u{00B7} ctrl+p cmd",
        " BUILD  52.9k (26%) \u{00B7} ctrl+p cmd",
        " BUILD  52.9K (26%)",
        " EXPLORE",
        " CODE REVIEW  52.9K (26%) \u{00B7} ctrl+p cmd",
    ] {
        assert_eq!(
            detect_opencode_screen_activity(&critic_mini_rows(row)),
            AgentScreenActivity::Unknown,
            "row {row:?}"
        );
    }
}

#[test]
fn critic_mini_interrupt_beats_status_tokens() {
    // Catches: reordering so a numeric tail wins over `esc interrupt`.
    assert_eq!(
        detect_opencode_screen_activity(&critic_mini_rows(" BUILD  12 esc interrupt")),
        AgentScreenActivity::Working
    );
    assert_eq!(
        detect_opencode_screen_activity(&critic_mini_rows(" BUILD  ⬝⬝■■■■■")),
        AgentScreenActivity::Working
    );
}

#[test]
fn critic3_mini_usage_number_boundaries_are_ready() {
    // Catches: the usage/percent parser rejecting a boundary of the observed
    // `<n>[.<n>][K|M|B] (<n>%)` shape (zero, no suffix, 100% and over), which would
    // stall the queue drain for that session.
    for usage in [
        "0 (0%)",
        "999K (100%)",
        "1.2M (80%)",
        "3B (150%)",
        "950 (1%)",
    ] {
        let row = format!(" BUILD  {usage} \u{00B7} ctrl+p cmd");
        assert_eq!(
            detect_opencode_screen_activity(&critic_mini_rows(&row)),
            AgentScreenActivity::Ready,
            "row {row:?}"
        );
    }
}

#[test]
fn critic3_mini_malformed_usage_or_extra_tokens_are_unknown() {
    // Catches: a loosened usage/percent check or a tail wildcard that lets a malformed
    // number or a stray token ride on `ctrl+p cmd` and read Ready on a live turn.
    for row in [
        " BUILD  1.2.3K (26%) \u{00B7} ctrl+p cmd",
        " BUILD  .5K (26%) \u{00B7} ctrl+p cmd",
        " BUILD  5.K (26%) \u{00B7} ctrl+p cmd",
        " BUILD  52.9KM (26%) \u{00B7} ctrl+p cmd",
        " BUILD  K (26%) \u{00B7} ctrl+p cmd",
        " BUILD  52.9K (%) \u{00B7} ctrl+p cmd",
        " BUILD  52.9K (26.5%) \u{00B7} ctrl+p cmd",
        " BUILD  ctrl+p cmd extra",
        " BUILD  extra ctrl+p cmd",
        " BUILD  ctrl+p",
    ] {
        assert_eq!(
            detect_opencode_screen_activity(&critic_mini_rows(row)),
            AgentScreenActivity::Unknown,
            "row {row:?}"
        );
    }
}

#[test]
fn critic3_mini_working_row_with_unusual_agent_label_is_working() {
    // Catches: the label gate running before the interrupt/bar check, so a live turn of
    // an agent whose name has a dot (`MY.AGENT`) reads Unknown instead of Working and
    // the queue can be typed into the turn once the screen goes quiet.
    assert_eq!(
        detect_opencode_screen_activity(&critic_mini_rows(
            " MY.AGENT  \u{2B1D}\u{2B1D}\u{25A0}\u{25A0}\u{25A0} esc interrupt"
        )),
        AgentScreenActivity::Working
    );
}

/// Catches: `MIN_STATUS_ROW_COLUMNS` set to the narrowest width probed (64) instead of the
/// narrowest width that paints the row (46). An idle OpenCode in a 46..=63 column split pane
/// then reads Unknown for ever, and its queued commands never drain.
#[test]
fn critic4_mini_bare_label_reads_ready_wherever_opencode_paints_the_row() {
    for columns in [46usize, 47, 48, 52, 56, 60, 63, 64, 120] {
        assert_eq!(
            detect_agent_screen_activity_at(
                Some("opencode"),
                &critic4_bare_build_rows(),
                Some(columns)
            ),
            AgentScreenActivity::Ready,
            "opencode paints ` BUILD` at {columns} columns (tmux capture)"
        );
    }
}

/// Catches: the width boundary moving up from the observed 45/46 edge, or a width gate that
/// disappears below it. At 45 and below the row is absent, so a ` BUILD` line is tool output.
#[test]
fn critic4_mini_bare_label_is_unknown_where_opencode_paints_no_row() {
    for columns in [20usize, 40, 41, 44, 45] {
        assert_eq!(
            detect_agent_screen_activity_at(
                Some("opencode"),
                &critic4_bare_build_rows(),
                Some(columns)
            ),
            AgentScreenActivity::Unknown,
            "opencode paints no status row at {columns} columns"
        );
    }
}

/// Catches: the width gate dropping a running turn on a narrow screen — the bar and the cut
/// `esc interrupt` text must stay Working at every width, or auto-standby stops a live turn.
#[test]
fn critic4_mini_running_turn_is_working_at_every_width() {
    for columns in [
        Some(10usize),
        Some(40),
        Some(46),
        Some(63),
        Some(64),
        Some(200),
        Some(0),
        None,
    ] {
        for row in [
            " BUILD  \u{2B1D}\u{2B1D}\u{25A0}\u{25A0}\u{25A0}\u{25A0}\u{25A0} esc interrupt",
            " BUILD  \u{2B1D}\u{2B1D}\u{25A0}\u{25A0}\u{25A0}\u{25A0}\u{25A0}\u{25A0}",
        ] {
            let rows = vec!["  working".to_string(), row.to_string()];
            assert_eq!(
                detect_agent_screen_activity_at(Some("opencode"), &rows, columns),
                AgentScreenActivity::Working,
                "{row:?} at {columns:?}"
            );
        }
    }
}

/// Catches: a full usage status row withheld on a 46..=63 column pane (the gate returns Unknown
/// before looking at the row), so a turn that produced usage never reads Ready there.
#[test]
fn critic4_mini_usage_status_row_reads_ready_on_a_mid_width_pane() {
    let rows = vec![
        "  done".to_string(),
        " BUILD  52.9K (26%) \u{00B7} ctrl+p cmd".to_string(),
    ];
    for columns in [58usize, 63, 64, 120] {
        assert_eq!(
            detect_agent_screen_activity_at(Some("opencode"), &rows, Some(columns)),
            AgentScreenActivity::Ready,
            "{columns} columns"
        );
    }
}

/// Catches: the width parameter leaking into another agent's adapter.
#[test]
fn critic4_width_does_not_change_other_agents() {
    let rows = vec!["> Enter to send \u{00B7} Ctrl+J newline".to_string()];
    for columns in [Some(10usize), Some(40), Some(63), None] {
        assert_eq!(
            detect_agent_screen_activity_at(Some("goose"), &rows, columns),
            detect_agent_screen_activity(Some("goose"), &rows)
        );
    }
}

/// Catches: the width gate off by one (`<=` instead of `<`, or 45/47 typed in the constant):
/// OpenCode paints ` BUILD` from exactly 46 columns and nothing at 45.
#[test]
fn critic5_mini_bare_label_gate_is_exact_at_45_46_47() {
    for label in ["BUILD", "PLAN"] {
        let rows = vec!["  tool output".to_string(), format!(" {label}")];
        for (columns, expected) in [
            (Some(45), AgentScreenActivity::Unknown),
            (Some(46), AgentScreenActivity::Ready),
            (Some(47), AgentScreenActivity::Ready),
            (None, AgentScreenActivity::Ready),
        ] {
            assert_eq!(
                detect_agent_screen_activity_at(Some("opencode"), &rows, columns),
                expected,
                "{label} at {columns:?}"
            );
        }
    }
}

/// Catches: the lowered gate also admitting non-status shapes at 46..63 columns
/// (a bare non-primary label or a truncated usage row reading Ready mid-turn).
#[test]
fn critic5_mini_narrow_non_status_rows_stay_unknown_at_46() {
    for row in [
        " BUILD 52.9K",
        " BUILD 52.9K (26%)",
        " REVIEW",
        " BUILD  ctrl+p",
        " Build",
    ] {
        let rows = vec![row.to_string()];
        assert_eq!(
            detect_agent_screen_activity_at(Some("opencode"), &rows, Some(46)),
            AgentScreenActivity::Unknown,
            "{row:?}"
        );
    }
}

/// Catches: a slow shell snapshot sampled before a newer agent snapshot revokes
/// the live agent, or a late agent snapshot re-arms a newer returned shell.
#[test]
fn stale_foreground_snapshot_cannot_revoke_newer_agent_or_rearm_returned_shell() {
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "foreground-snapshot-order";
    agent_session(&state, sid, SHELL_IDLE);
    assert_eq!(
        apply_foreground_agent_observation(
            &state,
            sid,
            2,
            Some("claude".into()),
            false,
            false,
            "claude".into()
        )
        .as_deref(),
        Some("claude")
    );
    // The older sample completes after generation 2 committed.
    assert_eq!(
        apply_foreground_agent_observation(&state, sid, 1, None, true, false, "bash".into())
            .as_deref(),
        Some("claude")
    );
    assert_eq!(
        state
            .session_maps
            .session_states
            .get(sid)
            .unwrap()
            .agent_type
            .as_deref(),
        Some("claude")
    );
    assert_eq!(
        apply_foreground_agent_observation(&state, sid, 3, None, true, false, "bash".into()),
        None
    );
    assert_eq!(
        apply_foreground_agent_observation(
            &state,
            sid,
            2,
            Some("claude".into()),
            false,
            false,
            "claude".into()
        ),
        None
    );
    let session = state.session_maps.session_states.get(sid).unwrap();
    assert_eq!(session.agent_type, None);
    assert_eq!(session.foreground_probe_generation, 3);
}
