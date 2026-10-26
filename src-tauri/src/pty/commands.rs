//! Tauri IPC command wrappers for PTY sessions.
//!
//! Moved verbatim from `pty.rs`. `pty.rs` re-exports every item declared
//! here, so no call site changes.

use super::*;

/// Create a new PTY session with optional worktree
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn create_pty(
    _app: AppHandle,
    state: State<'_, Arc<AppState>>,
    config: PtyConfig,
) -> Result<String, String> {
    let session_id = Uuid::new_v4().to_string();

    let shell = resolve_shell(config.shell.clone());

    // Guard against invalid dimensions from zero-sized windows
    let rows = config.rows.max(24);
    let cols = config.cols.max(80);

    let spawn_config = config.clone();
    let spawn_shell = shell.clone();
    let data_dir = state.data_dir.clone();
    let state_for_env = state.inner().clone();
    let session_id_for_env = session_id.clone();
    let (pair, child) = spawn_pty_pair_with_retry_async(
        PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        },
        move || {
            let mut cmd = build_shell_command(&spawn_shell);

            if let Some(ref cwd) = spawn_config.cwd {
                let cwd = crate::cli::expand_tilde(cwd);
                // Don't convert drive paths for WSL — cmd.cwd() sets the Windows
                // process CWD via CreateProcessW, which can't resolve Linux paths.
                // Windows translates drive paths to /mnt/... automatically when
                // spawning wsl.exe. (GitHub #27)
                cmd.cwd(cwd);
            }

            // Inject OSC 133 shell integration (command block markers)
            crate::shell_integration::inject(&data_dir, &spawn_shell, &mut cmd);

            // Inject stable session UUID so agents can use it for session binding
            // (e.g. `claude --session-id $TUIC_SESSION`, then `claude --resume $TUIC_SESSION`)
            bind_pty_identity(
                &state_for_env,
                &mut cmd,
                &session_id_for_env,
                spawn_config.tuic_session.as_deref(),
            );
            apply_agent_screen_env(&mut cmd, &spawn_config.env);

            // Inject env flags (feature flags configured in Settings → Agents)
            for (key, value) in &spawn_config.env {
                cmd.env(key, value);
            }

            cmd
        },
    )
    .await?;
    lower_pty_child_priority(child.process_id());

    let tuic_session = config.tuic_session.clone();

    let writer = pair
        .master
        .take_writer()
        .map_err(|e| format!("Failed to get PTY writer: {e}"))?;

    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| format!("Failed to get PTY reader: {e}"))?;

    // Store session (master handle kept for resize support)
    let paused = Arc::new(AtomicBool::new(false));
    state.session_maps.sessions.insert(
        session_id.clone(),
        Mutex::new(PtySession {
            launch_receipt: None,
            writer: Arc::new(Mutex::new(writer)),
            master: pair.master,
            _child: child,
            paused: paused.clone(),
            worktree: None,
            initial_cwd: config.cwd.clone(),
            cwd: config.cwd,
            display_name: None,
            display_name_is_custom: false,
            display_name_from_spawn: false,
            is_remote: false,
            shell: shell.clone(),
        }),
    );
    state.assign_term_alias(&session_id, config.alias.as_deref());
    state.metrics.total_spawned.fetch_add(1, Ordering::Relaxed);
    state
        .metrics
        .active_sessions
        .fetch_add(1, Ordering::Relaxed);

    // Create ring buffer and VT log buffer for this session
    state.session_maps.output_buffers.insert(
        session_id.clone(),
        Mutex::new(OutputRingBuffer::new(OUTPUT_RING_BUFFER_CAPACITY)),
    );
    let vt_log = state.new_vt_log_buffer(rows, cols, VT_LOG_BUFFER_CAPACITY);
    state
        .grid
        .vt_log_buffers
        .insert(session_id.clone(), Mutex::new(vt_log));
    let grid_watch_tx = crate::grid_watch::new_grid_watch();
    state.grid.watch.insert(session_id.clone(), grid_watch_tx);
    state
        .session_maps
        .last_output_ms
        .insert(session_id.clone(), std::sync::atomic::AtomicU64::new(0));
    state
        .session_maps
        .terminal_rows
        .insert(session_id.clone(), std::sync::atomic::AtomicU16::new(rows));
    let mut ss = crate::state::SessionState {
        spawn_root_role: crate::state::SpawnRootRole::Shell,
        ..Default::default()
    };
    if config.agent_type.is_some() {
        ss.seed_configured_agent(config.agent_type);
        ss.hook_instrumented = hook_instrumented_for(
            &crate::config::load_agents_config(),
            ss.agent_type.as_deref(),
        );
    }
    state
        .session_maps
        .session_states
        .insert(session_id.clone(), ss);

    spawn_reader_thread(
        reader,
        paused,
        session_id.clone(),
        state.inner().clone(),
        tuic_session,
    );

    Ok(session_id)
}

/// Create a PTY session with a dedicated git worktree
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn create_pty_with_worktree(
    _app: AppHandle,
    state: State<'_, Arc<AppState>>,
    pty_config: PtyConfig,
    worktree_config: WorktreeConfig,
) -> Result<WorktreeResult, String> {
    let pty_rows = pty_config.rows.max(24);
    let _pty_cols = pty_config.cols.max(80);
    // Create the worktree first
    let worktrees_dir = crate::worktree::resolve_worktree_dir_for_repo(
        std::path::Path::new(&worktree_config.base_repo),
        &state.worktrees_dir,
    );
    // Run the blocking git worktree calls off the async executor so a slow
    // checkout (LFS, large repo) doesn't stall other Tauri commands.
    // Uses the stale-recovery wrapper so orphaned directories are cleaned up
    // and retried automatically (single retry, no background task — PTY
    // creation is synchronous from the caller's perspective).
    let worktree = {
        let d = worktrees_dir.clone();
        let c = worktree_config.clone();
        tokio::task::spawn_blocking(move || create_worktree_with_stale_recovery(&d, &c, None))
            .await
            .map_err(|e| format!("create_worktree task panic: {e}"))??
    };
    let worktree_path = worktree.path.clone();

    // Wrap PTY creation so we can clean up the worktree on failure.
    let session_id = Uuid::new_v4().to_string();
    let rows = pty_config.rows.max(24);
    let cols = pty_config.cols.max(80);
    let shell = resolve_shell(pty_config.shell.clone());
    let spawn_shell = shell.clone();
    let spawn_worktree_path = worktree_path.clone();
    let spawn_env = pty_config.env.clone();
    let data_dir = state.data_dir.clone();
    let state_for_env = state.inner().clone();
    let session_id_for_env = session_id.clone();
    let spawn_tuic_session = pty_config.tuic_session.clone();
    let pty_result = spawn_pty_pair_with_retry_async(
        PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        },
        move || {
            let mut cmd = build_shell_command(&spawn_shell);
            cmd.cwd(&spawn_worktree_path);
            crate::shell_integration::inject(&data_dir, &spawn_shell, &mut cmd);
            bind_pty_identity(
                &state_for_env,
                &mut cmd,
                &session_id_for_env,
                spawn_tuic_session.as_deref(),
            );
            apply_agent_screen_env(&mut cmd, &spawn_env);
            for (key, value) in &spawn_env {
                cmd.env(key, value);
            }
            cmd
        },
    )
    .await
    .and_then(|(pair, child)| {
        lower_pty_child_priority(child.process_id());

        let writer = pair
            .master
            .take_writer()
            .map_err(|e| format!("Failed to get PTY writer: {e}"))?;

        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| format!("Failed to get PTY reader: {e}"))?;

        Ok((session_id, pair.master, child, writer, reader, shell))
    });

    let (session_id, master, child, writer, reader, shell) = match pty_result {
        Ok(result) => result,
        Err(e) => {
            // Clean up the worktree since PTY creation failed
            if let Err(cleanup_err) = remove_worktree_internal(&worktree, false) {
                tracing::warn!("Failed to cleanup worktree after PTY failure: {cleanup_err}");
            }
            return Err(e);
        }
    };

    let branch = worktree.branch.clone();
    let worktree_cwd = Some(worktree.path.to_string_lossy().to_string());

    // Store session with worktree info (master handle kept for resize support)
    let paused = Arc::new(AtomicBool::new(false));
    state.session_maps.sessions.insert(
        session_id.clone(),
        Mutex::new(PtySession {
            launch_receipt: None,
            writer: Arc::new(Mutex::new(writer)),
            master,
            _child: child,
            paused: paused.clone(),
            worktree: Some(worktree),
            initial_cwd: worktree_cwd.clone(),
            cwd: worktree_cwd,
            display_name: None,
            display_name_is_custom: false,
            display_name_from_spawn: false,
            is_remote: false,
            shell,
        }),
    );
    state.assign_term_alias(&session_id, pty_config.alias.as_deref());
    state.metrics.total_spawned.fetch_add(1, Ordering::Relaxed);
    state
        .metrics
        .active_sessions
        .fetch_add(1, Ordering::Relaxed);

    // Create ring buffer, VT log buffer, and diff renderer for this session
    state.session_maps.output_buffers.insert(
        session_id.clone(),
        Mutex::new(OutputRingBuffer::new(OUTPUT_RING_BUFFER_CAPACITY)),
    );
    let vt_log = state.new_vt_log_buffer(rows, cols, VT_LOG_BUFFER_CAPACITY);
    state
        .grid
        .vt_log_buffers
        .insert(session_id.clone(), Mutex::new(vt_log));
    let grid_watch_tx = crate::grid_watch::new_grid_watch();
    state.grid.watch.insert(session_id.clone(), grid_watch_tx);
    state
        .session_maps
        .last_output_ms
        .insert(session_id.clone(), std::sync::atomic::AtomicU64::new(0));
    state.session_maps.terminal_rows.insert(
        session_id.clone(),
        std::sync::atomic::AtomicU16::new(pty_rows),
    );
    let mut ss = crate::state::SessionState {
        spawn_root_role: crate::state::SpawnRootRole::Shell,
        ..Default::default()
    };
    if pty_config.agent_type.is_some() {
        ss.seed_configured_agent(pty_config.agent_type);
        ss.hook_instrumented = hook_instrumented_for(
            &crate::config::load_agents_config(),
            ss.agent_type.as_deref(),
        );
    }
    state
        .session_maps
        .session_states
        .insert(session_id.clone(), ss);

    spawn_reader_thread(
        reader,
        paused,
        session_id.clone(),
        state.inner().clone(),
        None,
    );

    Ok(WorktreeResult {
        session_id,
        worktree_path: worktree_path.to_string_lossy().to_string(),
        branch,
    })
}

/// List all active worktrees
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn list_worktrees(state: State<'_, Arc<AppState>>) -> Vec<serde_json::Value> {
    state
        .session_maps
        .sessions
        .iter()
        .filter_map(|entry| {
            let session = entry.value().lock();
            session.worktree.as_ref().map(|wt| {
                serde_json::json!({
                    "session_id": entry.key(),
                    "name": wt.name,
                    "path": wt.path.to_string_lossy(),
                    "branch": wt.branch,
                    "base_repo": wt.base_repo.to_string_lossy(),
                })
            })
        })
        .collect()
}

/// Write data to a PTY session.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn write_pty(
    _app: AppHandle,
    state: State<'_, Arc<AppState>>,
    session_id: String,
    data: String,
) -> Result<(), String> {
    write_pty_parts_off_thread(Arc::clone(&state), session_id, vec![data]).await
}

/// Write multiple input requests atomically while preserving their bookkeeping
/// boundaries.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn write_pty_parts(
    _app: AppHandle,
    state: State<'_, Arc<AppState>>,
    session_id: String,
    parts: Vec<String>,
) -> Result<(), String> {
    write_pty_parts_off_thread(Arc::clone(&state), session_id, parts).await
}

/// Return the current content of the input line buffer for a PTY session.
/// Empty string when the user has not started typing.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn get_input_buffer_content(
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> String {
    state
        .session_maps
        .input_buffers
        .get(&session_id)
        .map(|entry| entry.lock().content())
        .unwrap_or_default()
}

/// Get the last relevant user prompt (>= 10 words) for a PTY session.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn get_last_prompt(
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> Option<String> {
    state
        .session_maps
        .last_prompts
        .get(&session_id)
        .map(|v| v.clone())
}

/// Get the current shell state for a PTY session.
/// Used by the frontend on remount to sync state missed while unsubscribed.
/// Returns "busy", "idle", or null (session never produced output / removed).
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn get_shell_state(
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> Option<String> {
    state
        .session_maps
        .shell_states
        .get(&session_id)
        .and_then(|atom| {
            shell_state_wire(atom.load(std::sync::atomic::Ordering::Relaxed)).map(str::to_string)
        })
}

/// Return the classified shell family for a PTY session.
/// Lets the frontend pick the correct control sequences (e.g. Ctrl-U as
/// line-kill for POSIX readline vs. literal-char on cmd.exe/PowerShell)
/// without re-deriving the classification on every keystroke.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn get_session_shell_family(
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> Option<ShellFamily> {
    state
        .session_maps
        .sessions
        .get(&session_id)
        .map(|entry| classify_shell(&entry.lock().shell))
}

/// Enable or disable VT100 diff rendering for a PTY session.
/// Resize a PTY session
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn resize_pty(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    rows: u16,
    cols: u16,
) -> Result<(), String> {
    let state = Arc::clone(&state);
    let resize_frame =
        resize_session_off_thread(&state, session_id.clone(), rows, cols, "ipc").await?;
    // Flush the post-resize frame so the viewport repaints without waiting for the
    // next PTY data event (fixes blank screen after zoom on static content).
    if let Some(frame) = resize_frame {
        send_grid_frame(&state, &session_id, frame);
    }
    Ok(())
}

/// Apply theme ANSI colors (indices 0-15) to all terminal grids.
/// Each color is a `[r, g, b]` triple. Called by the frontend when the theme changes.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn set_ansi_colors(
    state: State<'_, Arc<AppState>>,
    colors: [[u8; 3]; 16],
) -> Result<(), String> {
    *state.ansi_colors.write() = Some(colors);
    for entry in state.grid.vt_log_buffers.iter() {
        entry.value().lock().set_ansi_colors(&colors);
    }
    Ok(())
}

/// Pause PTY reader thread (flow control: frontend buffer full)
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn pause_pty(state: State<'_, Arc<AppState>>, session_id: String) -> Result<(), String> {
    let entry = state
        .session_maps
        .sessions
        .get(&session_id)
        .ok_or_else(|| format!("Session not found: {session_id}"))?;
    entry.lock().paused.store(true, Ordering::Relaxed);
    state
        .metrics
        .pauses_triggered
        .fetch_add(1, Ordering::Relaxed);
    tracing::debug!(session_id = %session_id, "PTY reader paused (flow control)");
    Ok(())
}

/// Resume PTY reader thread (flow control: frontend buffer drained)
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn resume_pty(
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> Result<(), String> {
    let entry = state
        .session_maps
        .sessions
        .get(&session_id)
        .ok_or_else(|| format!("Session not found: {session_id}"))?;
    entry.lock().paused.store(false, Ordering::Relaxed);
    drop(entry);
    #[cfg(unix)]
    if let Err(e) = crate::session_wake::ensure_live(&state, &session_id) {
        tracing::debug!(session_id = %session_id, error = %e, "Wake on resume (may not be in standby)");
    }
    tracing::debug!(session_id = %session_id, "PTY reader resumed (flow control)");
    Ok(())
}

/// Query current kitty keyboard protocol flags for a session.
/// Returns 0 if the session has no kitty state (protocol not activated).
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn get_kitty_flags(state: State<'_, Arc<AppState>>, session_id: String) -> u32 {
    state
        .session_maps
        .kitty_states
        .get(&session_id)
        .map(|entry| entry.lock().current_flags())
        .unwrap_or(0)
}

/// Close a PTY session with graceful shutdown and optional worktree cleanup.
/// Sends Ctrl-C (0x03) and waits briefly for the process to exit cleanly
/// before forcibly dropping handles.
///
/// Async + `spawn_blocking` because that wait is two `sleep` loops of up to
/// 100 ms each, and the worktree cleanup is a recursive delete. Inline on the
/// IPC thread — the macOS main thread — closing a workspace's terminals one by
/// one froze the WebView for the sum of those waits.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn close_pty(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    cleanup_worktree: bool,
) -> Result<(), String> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        if let Some(worktree) = close_pty_core(&state, &session_id, cleanup_worktree)
            && let Err(e) = remove_worktree_internal(&worktree, false)
        {
            tracing::warn!("Failed to cleanup worktree: {e}");
        }
    })
    .await
    .map_err(|e| format!("Task panic: {e}"))
}

/// Get the foreground process of a PTY session and classify it as a known agent.
/// Returns the agent name (e.g. "claude") or None if the foreground process is
/// not a recognized agent or the session doesn't exist.
///
/// When the foreground is a non-shell process that `classify_agent` doesn't
/// recognise (custom aliases, symlinks, wrapper scripts like "C2"), falls back
/// to the pre-set `session_states.agent_type` so run-config launches are
/// detected correctly without hardcoding every possible alias.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn get_session_foreground_process(
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> Option<String> {
    super::refresh_session_agent(&state, &session_id)
}

/// Get the PID of the deepest foreground process in a PTY session.
///
/// On Unix: uses the process group leader to find the foreground process.
/// On Windows: walks the process tree from the child PID to the deepest descendant.
///
/// Returns `None` if the session doesn't exist or the process has exited.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn get_session_leaf_pid(
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> Option<u32> {
    super::session_leaf_pid(&state, &session_id)
}

/// Check if a PTY session has a non-shell foreground process running.
/// Returns the process name (e.g. "htop", "node", "claude") or None if
/// the foreground is the shell itself (zsh, bash, fish, etc.).
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn has_foreground_process(
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> Option<String> {
    const SHELLS: &[&str] = &[
        "zsh",
        "bash",
        "fish",
        "sh",
        "dash",
        "ksh",
        "csh",
        "tcsh",
        "nushell",
        "nu",
        "powershell",
        "pwsh",
        "cmd",
    ];
    let entry = state.session_maps.sessions.get(&session_id)?;
    // Extract pid under lock, then drop before the blocking syscall
    #[cfg(not(windows))]
    let pid = {
        let session = entry.value().lock();
        let pgid = session.master.process_group_leader()?;
        u32::try_from(pgid).ok()?
    };
    #[cfg(windows)]
    let pid = {
        let session = entry.value().lock();
        let child_pid = session._child.process_id()?;
        deepest_descendant_pid(child_pid)?
    };
    let name = process_name_from_pid(pid)?;
    if SHELLS.contains(&name.as_str()) {
        None
    } else {
        Some(name)
    }
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn get_orchestrator_stats(state: State<'_, Arc<AppState>>) -> OrchestratorStats {
    state.orchestrator_stats()
}

/// Get PTY session metrics for observability
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn get_session_metrics(state: State<'_, Arc<AppState>>) -> serde_json::Value {
    state.session_metrics_json()
}

/// Check if we can spawn a new session
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn can_spawn_session(state: State<'_, Arc<AppState>>) -> bool {
    state.session_maps.sessions.len() < MAX_CONCURRENT_SESSIONS
}

/// Set the display name of a PTY session (syncs tab title to backend for PWA visibility).
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn set_session_name(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    name: Option<String>,
    is_custom: Option<bool>,
) -> Result<(), String> {
    let entry = state
        .session_maps
        .sessions
        .get(&session_id)
        .ok_or_else(|| format!("Session not found: {session_id}"))?;
    entry
        .lock()
        .set_display_name(name, is_custom.unwrap_or(true));
    Ok(())
}

/// Queue a user-composed command for an agent session (Compose panel enqueue).
/// Typed at once when the agent is idle, otherwise delivered on its next
/// BUSY→IDLE transition so a running turn is never steered.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn enqueue_agent_command(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    text: String,
    idempotency_key: Option<String>,
    expected_draft: Option<String>,
) -> Result<EnqueuedCommand, String> {
    if let Some(draft) = expected_draft {
        return enqueue_owned_draft(&state, &session_id, &text, &draft);
    }
    let session_id = crate::session_wake::ensure_live(&state, &session_id)?;
    enqueue_user_command(&state, &session_id, &text, idempotency_key.as_deref())
}

/// Discard every command still queued for a session. Returns how many were dropped.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn clear_queued_agent_commands(
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> usize {
    clear_queued_commands(&state, &session_id)
}

/// The queued commands themselves, so the Compose panel can show what waits.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn list_queued_agent_commands(
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> Vec<QueuedCommand> {
    list_queued_commands(&state, &session_id)
}

/// Drop one queued command by id. Returns false when it is already gone.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn remove_queued_agent_command(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    command_id: u64,
) -> bool {
    remove_queued_command(&state, &session_id, command_id)
}

/// List all active PTY sessions for reconnection after frontend reload.
///
/// Sessions a connected remote machine runs are in the list too, each carrying
/// its `connection_id` — the desktop reconnects to them exactly like local ones.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn list_active_sessions(
    state: State<'_, Arc<AppState>>,
) -> Vec<crate::mcp_http::types::SessionInfo> {
    crate::mcp_http::session::session_rows_including_remote(&state)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn get_process_stats(state: State<'_, Arc<AppState>>) -> Vec<ProcessStats> {
    collect_process_stats(&state)
}

/// Register a Tauri Channel for binary grid frame streaming on a session.
/// The frontend calls this once per terminal; subsequent PTY output triggers
/// `serialize_dirty_rows()` on the session's TerminalGrid and sends the result
/// via the channel. Replaces any previously registered channel for the session.
///
/// Returns the subscription epoch. The frontend must carry it back on every
/// `ack_terminal_frame` and on `unsubscribe_terminal_grid`: a terminal that
/// remounts subscribes again before the old instance has finished tearing down,
/// and without the epoch the late ack of the dead instance credits the fresh
/// gate (opening it for frames nobody received) while its late unsubscribe
/// deletes the fresh channel (blanking a mounted terminal).
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn subscribe_terminal_grid(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    channel: tauri::ipc::Channel<tauri::ipc::Response>,
    webview: tauri::Webview,
    visible: bool,
) -> u64 {
    // A fresh gate, counting from zero — the frontend resets its receipt counter
    // on the same call.
    let gate = Arc::new(crate::grid_gate::GridGate::new());
    let epoch = gate.epoch();
    state.grid.gates.insert(session_id.clone(), gate);
    state.grid.channels.insert(
        session_id,
        crate::state::DesktopGridChannel {
            channel,
            webview_label: webview.label().to_string(),
            epoch,
            visible: AtomicBool::new(visible),
        },
    );
    epoch
}

/// Report how many grid frames the frontend has received in total, which opens
/// the delivery gate once it has caught up with what was sent.
///
/// The ticker (16 ms interval) is the sole normal damage-driven frame sender; this
/// path only releases the gate. That caps the frame rate at ~60 Hz and prevents
/// the tight ack→flush→ack loop that saturated the main thread.
///
/// `received` is a total, not a delta, so a duplicated or reordered ack is
/// idempotent — and an ack for a frame the ticker already abandoned is a number
/// in the past, which is what stops the burst-when-behind of story 601-82ef.
///
/// `epoch` is the value `subscribe_terminal_grid` returned; an ack that carries
/// any other epoch belongs to a previous subscription and is dropped.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn ack_terminal_frame(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    epoch: u64,
    received: u64,
    visible: Option<bool>,
) {
    ack_terminal_frame_core(&state, &session_id, epoch, received, visible);
}

#[cfg(feature = "desktop")]
pub(super) fn ack_terminal_frame_core(
    state: &AppState,
    session_id: &str,
    epoch: u64,
    received: u64,
    visible: Option<bool>,
) {
    if let Some(visible) = visible
        && let Some(channel) = state.grid.channels.get(session_id)
        && channel.epoch == epoch
    {
        channel.visible.store(visible, Ordering::Relaxed);
    }
    if let Some(gate) = state.grid.gates.get(session_id) {
        // A receipt reopens delivery; it is not new terminal damage. The ticker
        // retains actual reader damage and missed-frame repair debt while gated.
        gate.ack(epoch, received);
    }
}

/// Request a full frame for a session (used after subscribe to get initial state).
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn terminal_request_frame(state: State<'_, Arc<AppState>>, session_id: String) {
    if !grid_has_subscriber(&state, &session_id) {
        return;
    }
    if let Some(gate) = state.grid.gates.get(&session_id) {
        gate.resume_for_full_frame();
    }
    if let Some(vt) = state.grid.vt_log_buffers.get(&session_id) {
        let frame = {
            let mut vt = vt.lock();
            vt.grid_force_full_damage();
            vt.serialize_dirty_rows()
        };
        send_grid_frame(&state, &session_id, frame);
    }
}

/// Unregister the grid channel for a session (called by the frontend on unmount).
///
/// Only the subscription that owns `epoch` may tear the channel down. A terminal
/// that remounts subscribes before the outgoing instance unsubscribes, so
/// honouring a stale call would delete the live channel and leave a mounted
/// terminal with no frames at all.
///
/// `pending_scroll` is deliberately NOT removed here: it is owned by the session
/// (`spawn_reader_thread` creates it, `remove_live_session_state` drops it), and
/// taking it away with the desktop channel left an attached browser unable to
/// scroll the moment the desktop terminal closed.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn unsubscribe_terminal_grid(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    epoch: u64,
) {
    let is_current = state
        .grid
        .gates
        .get(&session_id)
        .is_some_and(|gate| gate.epoch() == epoch);
    if !is_current {
        return;
    }
    state.grid.channels.remove(&session_id);
    state.grid.gates.remove(&session_id);
}

/// Exit alternate screen via the terminal grid (display side only, never touches PTY stdin).
/// Only injects the exit sequences when the grid is actually in alternate-screen mode,
/// preventing escape leaks into the shell when the agent already cleaned up normally.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn terminal_exit_alt_screen(
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> bool {
    if let Some(vt) = state.grid.vt_log_buffers.get(&session_id) {
        let (was_alt, frame) = {
            let mut vt = vt.lock();
            if !vt.is_alternate_screen() {
                return false;
            }
            vt.process_local(b"\x1b[?1049l\x1b[?1047l\x1b[?47l\x1b[?25h\x1b[0m");
            (true, vt.serialize_dirty_rows())
        };
        if was_alt {
            send_grid_frame(&state, &session_id, frame);
        }
        was_alt
    } else {
        false
    }
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn terminal_scroll(state: State<'_, Arc<AppState>>, session_id: String, delta: i32) {
    if let Some(vt) = state.grid.vt_log_buffers.get(&session_id) {
        let frame = {
            let mut vt = vt.lock();
            vt.grid_scroll(delta);
            vt.serialize_dirty_rows()
        };
        send_grid_frame(&state, &session_id, frame);
    }
}

/// Coalesced scroll: record the target absolute display offset and mark the grid
/// dirty so the frame ticker applies it under the lock it already holds. Crucially
/// takes NO vt lock here, so scrolling never contends with the PTY output processor.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn terminal_scroll_to_offset(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    offset: usize,
) {
    if let Some(p) = state.grid.pending_scroll.get(&session_id) {
        p.store(offset as i64, std::sync::atomic::Ordering::Relaxed);
    }
    if let Some(d) = state.grid.frame_dirty.get(&session_id) {
        d.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Coalesced eviction-stable history scroll. None resumes following output.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn terminal_scroll_to_anchor(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    anchor: Option<usize>,
) {
    queue_scroll_anchor(&state, &session_id, anchor);
}

/// Fetch a range of styled rows by absolute index, to fill the frontend's
/// client-side row cache for smooth local scroll rendering. Read-only; called in
/// background chunks as the viewport approaches uncached rows, not per frame.
///
/// Returns `tauri::ipc::Response` — a bare `Vec<u8>` would take the blanket
/// `IpcResponse` impl and cross the IPC as a JSON array of decimal numbers
/// (~140 KB of bytes becoming a ~350 KB string per chunk, plus a `number[]` for
/// the JS engine to build and walk). `Response` marks the body raw, so the
/// webview receives the bytes as an ArrayBuffer.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn terminal_styled_rows(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    start: usize,
    count: usize,
) -> Result<tauri::ipc::Response, String> {
    let bytes = vt_read(&state, session_id, move |vt| {
        vt.grid_serialize_styled_range(start, count)
    })
    .await?;
    Ok(tauri::ipc::Response::new(bytes))
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn terminal_scroll_to(state: State<'_, Arc<AppState>>, session_id: String, line: usize) {
    if let Some(vt) = state.grid.vt_log_buffers.get(&session_id) {
        let frame = {
            let mut vt = vt.lock();
            vt.grid_scroll_to_line(line);
            vt.serialize_dirty_rows()
        };
        send_grid_frame(&state, &session_id, frame);
    }
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn terminal_scroll_info(
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> Result<(usize, usize, usize), String> {
    vt_read(&state, session_id, |vt| {
        (
            vt.grid_display_offset(),
            vt.grid_total_lines(),
            vt.grid_screen_lines(),
        )
    })
    .await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn terminal_search(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    query: String,
) -> Result<Vec<crate::terminal_grid::SearchMatch>, String> {
    let sid = session_id.clone();
    let q = query.clone();
    let found = vt_try_read(&state, session_id.clone(), move |buf| {
        let is_alt = buf.is_alternate_screen();
        let results = buf.grid_search(&q);
        tracing::info!(
            session_id = %sid,
            query = %q,
            is_alt_screen = is_alt,
            result_count = results.len(),
            history_size = buf.grid_history_size(),
            screen_lines = buf.grid_screen_lines(),
            "terminal_search"
        );
        results
    })
    .await?;
    Ok(found.unwrap_or_else(|| {
        tracing::warn!(session_id = %session_id, query = %query, "terminal_search: session not found in vt_log_buffers");
        Vec::new()
    }))
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn terminal_search_buffer(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    query: String,
) -> Result<Vec<crate::terminal_grid::BufferSearchMatch>, String> {
    vt_read(&state, session_id, move |vt| vt.grid_search_buffer(&query)).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn terminal_get_row_text(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    row: usize,
) -> Result<String, String> {
    vt_read(&state, session_id, move |vt| vt.grid_get_row_text(row)).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn terminal_get_logical_line(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    row: usize,
) -> Result<(usize, String), String> {
    // Not `vt_read`: the fallback for a gone session is the requested row with
    // no text, not row zero.
    Ok(
        vt_try_read(&state, session_id, move |vt| vt.grid_get_logical_line(row))
            .await?
            .unwrap_or((row, String::new())),
    )
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn terminal_get_selection_text(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    start_row: usize,
    start_col: usize,
    end_row: usize,
    end_col: usize,
    history_base: Option<usize>,
) -> Result<String, String> {
    match vt_try_read(&state, session_id, move |vt| {
        vt.grid_get_selection_text(start_row, start_col, end_row, end_col, history_base)
    })
    .await?
    {
        Some(result) => result,
        None => Ok(String::new()),
    }
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn terminal_get_lines(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    start: usize,
    end: usize,
) -> Result<Vec<String>, String> {
    vt_read(&state, session_id, move |vt| vt.grid_get_lines(start, end)).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn terminal_get_cursor_line(
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> Result<String, String> {
    vt_read(&state, session_id, |vt| vt.grid_get_cursor_line()).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn terminal_hyperlink_span(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    row: usize,
    col: usize,
) -> Result<Option<(usize, usize, String)>, String> {
    vt_read(&state, session_id, move |vt| {
        vt.grid_hyperlink_span(row, col)
    })
    .await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn set_session_visible(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    visible: bool,
) -> Result<(), String> {
    crate::background_mode::hidden::set_visible(&state, &session_id, visible, true);
    Ok(())
}

/// Guarded automated input: serialize readiness and draft checks with all raw input.
#[tauri::command]
pub(crate) async fn send_agent_input_if_empty(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    text: String,
) -> Result<bool, String> {
    let state = Arc::clone(&state);
    tauri::async_runtime::spawn_blocking(move || {
        send_agent_input_if_empty_blocking(&state, &session_id, &text)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Retry an ego provider outage without overwriting a tracked composer draft.
#[tauri::command]
pub(crate) async fn force_retry_provider_recovery_command(
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> Result<(), String> {
    let state = Arc::clone(&state);
    tauri::async_runtime::spawn_blocking(move || force_retry_provider_recovery(&state, &session_id))
        .await
        .map_err(|error| error.to_string())?
}
