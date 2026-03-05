use parking_lot::Mutex;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde::Serialize;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

use crate::input_line_buffer::{InputAction, InputLineBuffer};
use crate::output_parser::{colorize_intent, conceal_suggest, OutputParser, ParsedEvent};
use crate::state::{
    AppState, EscapeAwareBuffer, KittyAction, KittyKeyboardState, OrchestratorStats,
    OutputRingBuffer, PtyConfig, PtyOutput, PtySession, Utf8ReadBuffer, VtLogBuffer,
    MAX_CONCURRENT_SESSIONS, OUTPUT_RING_BUFFER_CAPACITY, VT_LOG_BUFFER_CAPACITY,
    strip_kitty_sequences,
};
use crate::worktree::{create_worktree_internal, remove_worktree_internal, WorktreeConfig, WorktreeResult};

/// Get the platform-appropriate default shell when no override is configured.
pub(crate) fn default_shell() -> String {
    #[cfg(windows)]
    {
        std::env::var("COMSPEC").unwrap_or_else(|_| "powershell.exe".to_string())
    }
    #[cfg(not(windows))]
    {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
    }
}

/// Environment overrides for Agent Teams iTerm2 shim mode.
/// When enabled, these env vars make Claude Code detect an iTerm2 terminal
/// and find the `it2` shim on PATH, enabling session splitting (Agent Teams).
pub(crate) struct AgentTeamsEnv {
    pub session_id: String,
    pub http_port: u16,
    pub socket_path: String,
}

impl AgentTeamsEnv {
    /// Return the environment variable overrides to inject into the PTY.
    pub(crate) fn env_overrides(&self) -> Vec<(String, String)> {
        let home = std::env::var("HOME").unwrap_or_else(|_| "~".to_string());
        let tuic_bin = format!("{home}/.tuicommander/bin");
        let current_path = std::env::var("PATH").unwrap_or_default();
        vec![
            ("ITERM_SESSION_ID".to_string(), format!("w0t0p0:{}", self.session_id)),
            ("TERM_PROGRAM".to_string(), "iTerm.app".to_string()),
            ("PATH".to_string(), format!("{tuic_bin}:{current_path}")),
            ("TUIC_HTTP_PORT".to_string(), self.http_port.to_string()),
            ("TUIC_SOCKET_PATH".to_string(), self.socket_path.clone()),
        ]
    }
}

/// Build a CommandBuilder for the given shell with platform-appropriate flags.
pub(crate) fn build_shell_command(shell: &str, agent_teams: Option<&AgentTeamsEnv>) -> CommandBuilder {
    #[allow(unused_mut)]
    let mut cmd = CommandBuilder::new(shell);
    // Login shell flag is Unix-only; PowerShell/cmd.exe don't support -l
    #[cfg(not(windows))]
    cmd.arg("-l");
    // GUI-launched Tauri apps don't inherit terminal env vars from a parent shell.
    // Set the essentials so tools detect color/encoding support correctly.
    #[cfg(not(windows))]
    {
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        // Signal kitty keyboard protocol support so apps (e.g. Claude Code / Ink)
        // detect it via heuristic precheck and proceed to query confirmation.
        cmd.env("KITTY_WINDOW_ID", "1");
        // Announce as ghostty so Claude Code's terminal detection allow-list
        // enables kitty keyboard protocol. CC ≥v2.1.52 only recognizes
        // WezTerm, ghostty, and iTerm.app — "kitty" was removed from the list.
        // ghostty is chosen because it has no iTerm/WezTerm-specific side effects.
        // On macOS this also prevents /etc/zshrc sourcing zshrc_Apple_Terminal.
        cmd.env("TERM_PROGRAM", "ghostty");
        // CC also checks TERM_PROGRAM_VERSION — missing or matching /^[0-2]\./
        // causes rejection.  Use a value that passes the gate.
        cmd.env("TERM_PROGRAM_VERSION", "3.0.0");
        // Prevent nested-session detection when TUICommander itself runs
        // inside a Claude Code session (CLAUDECODE env var would propagate).
        cmd.env_remove("CLAUDECODE");
        if let Ok(lang) = std::env::var("LANG") {
            cmd.env("LANG", lang);
        } else {
            // Fallback: ensure UTF-8 is available even when LANG is completely unset
            cmd.env("LANG", "en_US.UTF-8");
        }
        // Agent Teams shim: override TERM_PROGRAM to iTerm.app, inject
        // ITERM_SESSION_ID, prepend ~/.tuicommander/bin to PATH, set TUIC_HTTP_PORT
        if let Some(at) = agent_teams {
            for (k, v) in at.env_overrides() {
                cmd.env(k, v);
            }
        }
    }
    cmd
}

/// Resolve the shell to use: explicit override > env default > platform default.
pub(crate) fn resolve_shell(override_shell: Option<String>) -> String {
    override_shell.unwrap_or_else(default_shell)
}

/// How long the agent must be silent after printing a `?`-ending line before
/// we treat it as a question waiting for input. 10s is long enough to avoid
/// false positives from AI agents that pause while thinking between API calls.
const SILENCE_QUESTION_THRESHOLD: std::time::Duration = std::time::Duration::from_secs(10);

/// How often the timer thread wakes up to check for silence.
const SILENCE_CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

/// Grace period after a PTY resize during which parsed events (Question, RateLimit,
/// ApiError) are suppressed. The shell redraws visible output after SIGWINCH, which
/// would otherwise re-trigger notifications for content already on screen.
const RESIZE_GRACE: std::time::Duration = std::time::Duration::from_millis(1000);

/// Shared state between the PTY reader thread and the silence-detection timer thread.
pub(crate) struct SilenceState {
    /// When the last chunk of output was received from the PTY.
    pub(crate) last_output_at: std::time::Instant,
    /// The last line ending with `?` that hasn't been resolved yet.
    pub(crate) pending_question_line: Option<String>,
    /// Whether a Question event has already been emitted for the current pending line
    /// (either by the instant regex detector or by the silence timer).
    pub(crate) question_already_emitted: bool,
    /// When the last resize was requested. Used to suppress re-parsing of redrawn output.
    last_resize_at: Option<std::time::Instant>,
}

impl SilenceState {
    fn new() -> Self {
        Self {
            last_output_at: std::time::Instant::now(),
            pending_question_line: None,
            question_already_emitted: false,
            last_resize_at: None,
        }
    }

    /// Called by resize_pty when the terminal is resized.
    /// Marks the start of a grace period during which parsed events are suppressed.
    pub(crate) fn on_resize(&mut self) {
        self.last_resize_at = Some(std::time::Instant::now());
    }

    /// Returns true if we are within the resize grace period.
    /// Parsed events (Question, RateLimit, ApiError) should be suppressed during this window
    /// because the shell redraws visible output after SIGWINCH, causing false re-detections.
    pub(crate) fn is_resize_grace(&self) -> bool {
        self.last_resize_at
            .map(|t| t.elapsed() < RESIZE_GRACE)
            .unwrap_or(false)
    }

    /// Called by the reader thread after each chunk.
    /// `regex_found_question`: true if `parse()` already emitted a Question event.
    /// `last_question_line`: the last `?`-ending line in the chunk, if any.
    pub(crate) fn on_chunk(&mut self, regex_found_question: bool, last_question_line: Option<String>) {
        self.last_output_at = std::time::Instant::now();

        if regex_found_question {
            // The instant detector already fired — suppress the silence timer.
            self.pending_question_line = None;
            self.question_already_emitted = true;
        } else if let Some(line) = last_question_line {
            // New candidate for silence-based detection.
            self.pending_question_line = Some(line);
            self.question_already_emitted = false;
        } else {
            // The agent printed more output that doesn't end with `?` — it moved on.
            self.pending_question_line = None;
        }
    }

    /// Called by write_pty when the user submits a line of input.
    /// Clears any pending question candidate since it was typed by the user, not the agent.
    pub(crate) fn suppress_user_input(&mut self) {
        self.pending_question_line = None;
        self.question_already_emitted = true;
    }

    /// Called by the timer thread. Returns the question text if the silence
    /// threshold has been reached and we haven't emitted yet.
    pub(crate) fn check_silence(&mut self) -> Option<String> {
        if self.question_already_emitted {
            return None;
        }
        if let Some(ref line) = self.pending_question_line
            && self.last_output_at.elapsed() >= SILENCE_QUESTION_THRESHOLD
        {
            self.question_already_emitted = true;
            return Some(line.clone());
        }
        None
    }
}

/// Spawn a reader thread that reads from a PTY, emits Tauri events, and writes to the ring buffer.
/// Shared by `create_pty`, `create_pty_with_worktree`, and `spawn_agent` to avoid duplication.
pub(crate) fn spawn_reader_thread(
    mut reader: Box<dyn Read + Send>,
    paused: Arc<AtomicBool>,
    session_id: String,
    app: AppHandle,
    state: Arc<AppState>,
) {
    let silence = Arc::new(Mutex::new(SilenceState::new()));
    let running = Arc::new(AtomicBool::new(true));

    // Register in AppState so write_pty can suppress user-typed question lines
    state.silence_states.insert(session_id.clone(), silence.clone());

    // Spawn silence-detection timer thread
    {
        let silence = silence.clone();
        let running = running.clone();
        let app = app.clone();
        let session_id = session_id.clone();
        let event_bus = state.event_bus.clone();
        std::thread::spawn(move || {
            while running.load(Ordering::Relaxed) {
                std::thread::sleep(SILENCE_CHECK_INTERVAL);
                if !running.load(Ordering::Relaxed) {
                    break;
                }
                let question = silence.lock().check_silence();
                if let Some(prompt_text) = question {
                    let parsed = ParsedEvent::Question { prompt_text };
                    // Broadcast to SSE/WebSocket consumers
                    if let Ok(json) = serde_json::to_value(&parsed) {
                        let _ = event_bus.send(crate::state::AppEvent::PtyParsed {
                            session_id: session_id.clone(),
                            parsed: json,
                        });
                    }
                    // Tauri IPC for desktop backward compat
                    let _ = app.emit(
                        &format!("pty-parsed-{session_id}"),
                        &parsed,
                    );
                }
            }
        });
    }

    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut utf8_buf = Utf8ReadBuffer::new();
        let mut esc_buf = EscapeAwareBuffer::new();
        let mut parser = OutputParser::new();
        // Dedup status-line events: only emit when task_name actually changes
        let mut last_status_task: Option<String> = None;
        // Resolve session CWD once for resolving relative plan-file paths
        let session_cwd: Option<String> = state
            .sessions
            .get(&session_id)
            .and_then(|s| s.lock().cwd.clone());
        loop {
            while paused.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    state.metrics.bytes_emitted.fetch_add(n, Ordering::Relaxed);
                    let utf8_data = utf8_buf.push(&buf[..n]);
                    let esc_data = esc_buf.push(&utf8_data);
                    // Strip kitty keyboard protocol sequences from output
                    let (kitty_clean, kitty_actions) = strip_kitty_sequences(&esc_data);
                    let data = kitty_clean;
                    // Process kitty actions: push/pop state, respond to queries
                    if !kitty_actions.is_empty() {
                        let entry = state.kitty_states
                            .entry(session_id.clone())
                            .or_insert_with(|| Mutex::new(KittyKeyboardState::new()));
                        let mut ks = entry.lock();
                        for action in &kitty_actions {
                            match action {
                                KittyAction::Push(flags) => ks.push(*flags),
                                KittyAction::Pop => ks.pop(),
                                KittyAction::Query => {
                                    // Respond to query by writing CSI ? flags u to PTY
                                    let flags = ks.current_flags();
                                    let response = format!("\x1b[?{}u", flags);
                                    if let Some(sess) = state.sessions.get(&session_id) {
                                        let mut sess = sess.lock();
                                        let _ = sess.writer.write_all(response.as_bytes());
                                        let _ = sess.writer.flush();
                                    }
                                }
                            }
                        }
                        let flags = ks.current_flags();
                        drop(ks);
                        let _ = app.emit(
                            &format!("kitty-keyboard-{session_id}"),
                            flags,
                        );
                    }
                    if !data.is_empty() {
                        // Feed raw data (post-kitty-strip) into VT100 log buffer.
                        // Capture changed rows for clean-text parsing (both normal and alternate screen).
                        let changed_rows = if let Some(vt_log) = state.vt_log_buffers.get(&session_id) {
                            vt_log.lock().process(data.as_bytes())
                        } else {
                            Vec::new()
                        };
                        // Write clean text to ring buffer for MCP consumers (no ANSI)
                        if let Some(ring) = state.output_buffers.get(&session_id) {
                            ring.lock().write(data.as_bytes());
                        }
                        // Broadcast to WebSocket clients
                        if let Some(mut clients) = state.ws_clients.get_mut(&session_id) {
                            clients.retain(|tx| tx.send(data.clone()).is_ok());
                        }
                        // Emit parsed events before raw output.
                        // Suppress notification-class events during resize grace period:
                        // the shell redraws visible output after SIGWINCH, which would
                        // re-trigger Question/RateLimit/ApiError for content already on screen.
                        let in_resize_grace = silence.lock().is_resize_grace();
                        // OSC 9;4 progress events stay on the raw stream — they are consumed
                        // by the vt100 crate and invisible in clean rows.
                        let mut events = Vec::new();
                        if let Some(evt) = crate::output_parser::parse_osc94(&data) {
                            events.push(evt);
                        }
                        // All other events come from clean VtLogBuffer rows (no strip_ansi).
                        events.extend(parser.parse_clean_lines(&changed_rows));

                        // Slash menu detection: only when the session is in slash_mode
                        // (user typed / in the agent's input). Reads the full screen
                        // snapshot because arrow navigation only changes 1-2 rows.
                        if state.slash_mode.get(&session_id)
                            .is_some_and(|v| v.load(std::sync::atomic::Ordering::Relaxed))
                            && let Some(vt_log) = state.vt_log_buffers.get(&session_id)
                        {
                            let screen = vt_log.lock().screen_rows();
                            if let Some(evt) = crate::output_parser::parse_slash_menu(&screen) {
                                events.push(evt);
                            }
                        }

                        let regex_found_question = if in_resize_grace { false } else {
                            events.iter().any(|e| matches!(e, ParsedEvent::Question { .. }))
                        };
                        for event in &events {
                            if in_resize_grace && matches!(event,
                                ParsedEvent::Question { .. }
                                | ParsedEvent::RateLimit { .. }
                                | ParsedEvent::ApiError { .. }
                            ) {
                                continue;
                            }
                            // Dedup status-line: skip if task_name hasn't changed
                            if let ParsedEvent::StatusLine { task_name, .. } = event {
                                if last_status_task.as_deref() == Some(task_name.as_str()) {
                                    continue;
                                }
                                last_status_task = Some(task_name.clone());
                            }
                            // Resolve relative plan-file paths to absolute using session CWD.
                            // Canonicalize to remove ".." segments so the frontend security
                            // check (which rejects paths containing "..") doesn't block valid paths.
                            // Skip plan-file events for files that don't exist on disk —
                            // prevents false positives from grep output, help text, or test paths.
                            let resolved = if let ParsedEvent::PlanFile { path } = event {
                                let abs_path = if !path.starts_with('/') {
                                    if let Some(ref cwd) = session_cwd {
                                        let joined = std::path::PathBuf::from(cwd).join(path);
                                        Some(normalize_path(&joined).to_string_lossy().into_owned())
                                    } else {
                                        None
                                    }
                                } else {
                                    Some(path.clone())
                                };
                                match abs_path {
                                    Some(p) if std::path::Path::new(&p).is_file() => {
                                        Some(ParsedEvent::PlanFile { path: p })
                                    }
                                    _ => {
                                        // File doesn't exist — suppress the event
                                        continue;
                                    }
                                }
                            } else {
                                None
                            };
                            let emit_event = resolved.as_ref().unwrap_or(event);
                            // Broadcast to SSE/WebSocket consumers
                            if let Ok(json) = serde_json::to_value(emit_event) {
                                let _ = state.event_bus.send(crate::state::AppEvent::PtyParsed {
                                    session_id: session_id.clone(),
                                    parsed: json,
                                });
                            }
                            // Tauri IPC for desktop backward compat
                            let _ = app.emit(
                                &format!("pty-parsed-{session_id}"),
                                emit_event,
                            );
                        }

                        // Update silence state for fallback question detection.
                        // Feed from the last non-empty changed row (clean text, no strip_ansi).
                        let last_q_line = changed_rows.iter().rev()
                            .find(|r| !r.text.is_empty())
                            .and_then(|r| if r.text.ends_with('?') { Some(r.text.clone()) } else { None });
                        silence.lock().on_chunk(regex_found_question, last_q_line);

                        // Colorize [intent: ...] tokens yellow before sending to xterm.
                        // Run on every chunk containing "intent:" — not just when
                        // parse_clean_lines detected the Intent event — because Claude Code
                        // re-renders lines with CUU/CUD cursor movements, and re-render chunks
                        // may contain the token without detection (cursor-split text).
                        let data = if data.contains("[intent:") { colorize_intent(&data) } else { data };

                        // Conceal [[suggest: ...]] tokens so they are invisible in xterm but
                        // still occupy their original character positions (preserving cursor layout).
                        // SGR 8 (conceal) hides text without altering width; SGR 28 (reveal) restores.
                        let data = if data.contains("suggest:") { conceal_suggest(&data) } else { data };

                        let _ = app.emit(
                            &format!("pty-output-{session_id}"),
                            PtyOutput {
                                session_id: session_id.clone(),
                                data,
                            },
                        );
                    }
                }
                Err(e) => {
                    eprintln!("Error: PTY reader error for session {session_id}: {e}");
                    break;
                }
            }
        }
        // Signal timer thread to stop
        running.store(false, Ordering::Relaxed);

        // Flush all buffers at EOF
        let utf8_tail = utf8_buf.flush();
        let esc_remaining = if utf8_tail.is_empty() {
            esc_buf.flush()
        } else {
            let mut flushed = esc_buf.push(&utf8_tail);
            flushed.push_str(&esc_buf.flush());
            flushed
        };
        let remaining = esc_remaining;
        if !remaining.is_empty() {
            if let Some(ring) = state.output_buffers.get(&session_id) {
                ring.lock().write(remaining.as_bytes());
            }
            if let Some(mut clients) = state.ws_clients.get_mut(&session_id) {
                clients.retain(|tx| tx.send(remaining.clone()).is_ok());
            }
            let _ = app.emit(
                &format!("pty-output-{session_id}"),
                PtyOutput {
                    session_id: session_id.clone(),
                    data: remaining,
                },
            );
        }
        // Broadcast to SSE/WebSocket consumers
        let _ = state.event_bus.send(crate::state::AppEvent::PtyExit {
            session_id: session_id.clone(),
        });
        // Tauri IPC for desktop backward compat
        let _ = app.emit(
            &format!("pty-exit-{session_id}"),
            serde_json::json!({ "session_id": session_id }),
        );
        // Only decrement active_sessions if we're the ones removing the session.
        // HTTP/MCP close paths may have already removed it and decremented.
        if state.sessions.remove(&session_id).is_some() {
            state.metrics.active_sessions.fetch_sub(1, Ordering::Relaxed);
        }
        state.output_buffers.remove(&session_id);
        state.vt_log_buffers.remove(&session_id);
        state.ws_clients.remove(&session_id);
        state.kitty_states.remove(&session_id);
        state.input_buffers.remove(&session_id);
        state.silence_states.remove(&session_id);
    });
}

/// Reader thread for sessions created via HTTP (no AppHandle available).
/// Writes to ring buffer only — MCP consumers poll via GET /sessions/{id}/output.
pub(crate) fn spawn_headless_reader_thread(
    mut reader: Box<dyn Read + Send>,
    paused: Arc<AtomicBool>,
    session_id: String,
    state: Arc<AppState>,
) {
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut utf8_buf = Utf8ReadBuffer::new();
        let mut esc_buf = EscapeAwareBuffer::new();
        // Silence-based question detection (mirrors main reader)
        let silence = Arc::new(Mutex::new(SilenceState::new()));
        state.silence_states.insert(session_id.clone(), silence.clone());
        let mut parser = OutputParser::new();
        loop {
            while paused.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    state.metrics.bytes_emitted.fetch_add(n, Ordering::Relaxed);
                    let utf8_data = utf8_buf.push(&buf[..n]);
                    let esc_data = esc_buf.push(&utf8_data);
                    // Strip kitty keyboard protocol sequences
                    let (kitty_clean, kitty_actions) = strip_kitty_sequences(&esc_data);
                    let data = kitty_clean;
                    if !kitty_actions.is_empty() {
                        let entry = state.kitty_states
                            .entry(session_id.clone())
                            .or_insert_with(|| Mutex::new(KittyKeyboardState::new()));
                        let mut ks = entry.lock();
                        for action in &kitty_actions {
                            match action {
                                KittyAction::Push(flags) => ks.push(*flags),
                                KittyAction::Pop => ks.pop(),
                                KittyAction::Query => {
                                    let flags = ks.current_flags();
                                    let response = format!("\x1b[?{}u", flags);
                                    if let Some(sess) = state.sessions.get(&session_id) {
                                        let mut sess = sess.lock();
                                        let _ = sess.writer.write_all(response.as_bytes());
                                        let _ = sess.writer.flush();
                                    }
                                }
                            }
                        }
                    }
                    if !data.is_empty() {
                        // Feed raw data into VT100 log buffer; capture changed rows for parsing.
                        let changed_rows = if let Some(vt_log) = state.vt_log_buffers.get(&session_id) {
                            vt_log.lock().process(data.as_bytes())
                        } else {
                            Vec::new()
                        };
                        // Write clean text to ring buffer for MCP consumers (no ANSI)
                        if let Some(ring) = state.output_buffers.get(&session_id) {
                            ring.lock().write(data.as_bytes());
                        }
                        // Broadcast to WebSocket clients
                        if let Some(mut clients) = state.ws_clients.get_mut(&session_id) {
                            clients.retain(|tx| tx.send(data.clone()).is_ok());
                        }
                        // Emit structured events via event_bus (no Tauri IPC for headless).
                        // OSC 9;4 progress stays on raw stream.
                        let mut events = Vec::new();
                        if let Some(evt) = crate::output_parser::parse_osc94(&data) {
                            events.push(evt);
                        }
                        events.extend(parser.parse_clean_lines(&changed_rows));

                        // Slash menu detection (same as desktop reader)
                        if state.slash_mode.get(&session_id)
                            .is_some_and(|v| v.load(std::sync::atomic::Ordering::Relaxed))
                            && let Some(vt_log) = state.vt_log_buffers.get(&session_id)
                        {
                            let screen = vt_log.lock().screen_rows();
                            if let Some(evt) = crate::output_parser::parse_slash_menu(&screen) {
                                events.push(evt);
                            }
                        }

                        let regex_found_question = events.iter()
                            .any(|e| matches!(e, ParsedEvent::Question { .. }));
                        for event in &events {
                            if let Ok(json) = serde_json::to_value(event) {
                                let _ = state.event_bus.send(crate::state::AppEvent::PtyParsed {
                                    session_id: session_id.clone(),
                                    parsed: json,
                                });
                            }
                        }
                        // Update silence state for fallback question detection.
                        let last_q_line = changed_rows.iter().rev()
                            .find(|r| !r.text.is_empty())
                            .and_then(|r| if r.text.ends_with('?') { Some(r.text.clone()) } else { None });
                        silence.lock().on_chunk(regex_found_question, last_q_line);
                    }
                }
                Err(e) => {
                    eprintln!("Error: PTY reader error for session {session_id}: {e}");
                    break;
                }
            }
        }
        let utf8_tail = utf8_buf.flush();
        let esc_remaining = if utf8_tail.is_empty() {
            esc_buf.flush()
        } else {
            let mut flushed = esc_buf.push(&utf8_tail);
            flushed.push_str(&esc_buf.flush());
            flushed
        };
        let remaining = esc_remaining;
        if !remaining.is_empty() {
            if let Some(ring) = state.output_buffers.get(&session_id) {
                ring.lock().write(remaining.as_bytes());
            }
            if let Some(mut clients) = state.ws_clients.get_mut(&session_id) {
                clients.retain(|tx| tx.send(remaining.clone()).is_ok());
            }
        }
        if state.sessions.remove(&session_id).is_some() {
            state.metrics.active_sessions.fetch_sub(1, Ordering::Relaxed);
        }
        state.output_buffers.remove(&session_id);
        state.vt_log_buffers.remove(&session_id);
        state.ws_clients.remove(&session_id);
        state.kitty_states.remove(&session_id);
        state.input_buffers.remove(&session_id);
        state.silence_states.remove(&session_id);
    });
}

/// Create a new PTY session with optional worktree
#[tauri::command]
pub(crate) async fn create_pty(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    config: PtyConfig,
) -> Result<String, String> {
    let session_id = Uuid::new_v4().to_string();
    let pty_system = native_pty_system();

    let shell = resolve_shell(config.shell);

    // Guard against invalid dimensions from zero-sized windows
    let rows = config.rows.max(24);
    let cols = config.cols.max(80);

    // Retry PTY spawn up to 3 times with increasing delay (Story 059)
    let max_retries = 3;
    let mut last_err = String::new();
    let mut pair_and_child = None;

    for attempt in 0..max_retries {
        let pair = match pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        }) {
            Ok(p) => p,
            Err(e) => {
                last_err = format!("Failed to open PTY (attempt {}): {}", attempt + 1, e);
                if attempt < max_retries - 1 {
                    tokio::time::sleep(std::time::Duration::from_millis(100 * (attempt as u64 + 1))).await;
                }
                continue;
            }
        };

        let agent_teams = {
            let cfg = state.config.read();
            if cfg.agent_teams_shim {
                Some(AgentTeamsEnv {
                    session_id: session_id.clone(),
                    http_port: cfg.remote_access_port,
                    socket_path: crate::mcp_http::socket_path().to_string_lossy().to_string(),
                })
            } else {
                None
            }
        };
        let mut cmd = build_shell_command(&shell, agent_teams.as_ref());

        if let Some(ref cwd) = config.cwd {
            cmd.cwd(cwd);
        }

        match pair.slave.spawn_command(cmd) {
            Ok(child) => {
                pair_and_child = Some((pair, child));
                break;
            }
            Err(e) => {
                last_err = format!("Failed to spawn shell (attempt {}): {}", attempt + 1, e);
                if attempt < max_retries - 1 {
                    tokio::time::sleep(std::time::Duration::from_millis(100 * (attempt as u64 + 1))).await;
                }
            }
        }
    }

    let (pair, child) = pair_and_child.ok_or(last_err)?;

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
    state.sessions.insert(
        session_id.clone(),
        Mutex::new(PtySession {
            writer,
            master: pair.master,
            _child: child,
            paused: paused.clone(),
            worktree: None,
            cwd: config.cwd,
        }),
    );
    state.metrics.total_spawned.fetch_add(1, Ordering::Relaxed);
    state.metrics.active_sessions.fetch_add(1, Ordering::Relaxed);

    // Create ring buffer and VT log buffer for this session
    state.output_buffers.insert(
        session_id.clone(),
        Mutex::new(OutputRingBuffer::new(OUTPUT_RING_BUFFER_CAPACITY)),
    );
    state.vt_log_buffers.insert(
        session_id.clone(),
        Mutex::new(VtLogBuffer::new(24, 220, VT_LOG_BUFFER_CAPACITY)),
    );

    spawn_reader_thread(
        reader,
        paused,
        session_id.clone(),
        app,
        state.inner().clone(),
    );

    Ok(session_id)
}

/// Create a PTY session with a dedicated git worktree
#[tauri::command]
pub(crate) async fn create_pty_with_worktree(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    pty_config: PtyConfig,
    worktree_config: WorktreeConfig,
) -> Result<WorktreeResult, String> {
    // Create the worktree first
    let worktrees_dir = crate::worktree::resolve_worktree_dir_for_repo(
        std::path::Path::new(&worktree_config.base_repo),
        &state.worktrees_dir,
    );
    let worktree = create_worktree_internal(&worktrees_dir, &worktree_config, None)?;
    let worktree_path = worktree.path.clone();

    // Wrap PTY creation so we can clean up the worktree on failure
    let pty_result = (|| -> Result<_, String> {
        let session_id = Uuid::new_v4().to_string();
        let pty_system = native_pty_system();

        // Guard against invalid dimensions from zero-sized windows
        let rows = pty_config.rows.max(24);
        let cols = pty_config.cols.max(80);

        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("Failed to open PTY: {e}"))?;

        let shell = resolve_shell(pty_config.shell);

        let agent_teams = {
            let cfg = state.config.read();
            if cfg.agent_teams_shim {
                Some(AgentTeamsEnv {
                    session_id: session_id.clone(),
                    http_port: cfg.remote_access_port,
                    socket_path: crate::mcp_http::socket_path().to_string_lossy().to_string(),
                })
            } else {
                None
            }
        };
        let mut cmd = build_shell_command(&shell, agent_teams.as_ref());
        cmd.cwd(&worktree_path);

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("Failed to spawn shell: {e}"))?;

        let writer = pair
            .master
            .take_writer()
            .map_err(|e| format!("Failed to get PTY writer: {e}"))?;

        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| format!("Failed to get PTY reader: {e}"))?;

        Ok((session_id, pair.master, child, writer, reader))
    })();

    let (session_id, master, child, writer, reader) = match pty_result {
        Ok(result) => result,
        Err(e) => {
            // Clean up the worktree since PTY creation failed
            if let Err(cleanup_err) = remove_worktree_internal(&worktree) {
                eprintln!("Warning: Failed to cleanup worktree after PTY failure: {cleanup_err}");
            }
            return Err(e);
        }
    };

    let branch = worktree.branch.clone();
    let worktree_cwd = Some(worktree.path.to_string_lossy().to_string());

    // Store session with worktree info (master handle kept for resize support)
    let paused = Arc::new(AtomicBool::new(false));
    state.sessions.insert(
        session_id.clone(),
        Mutex::new(PtySession {
            writer,
            master,
            _child: child,
            paused: paused.clone(),
            worktree: Some(worktree),
            cwd: worktree_cwd,
        }),
    );
    state.metrics.total_spawned.fetch_add(1, Ordering::Relaxed);
    state.metrics.active_sessions.fetch_add(1, Ordering::Relaxed);

    // Create ring buffer and VT log buffer for this session
    state.output_buffers.insert(
        session_id.clone(),
        Mutex::new(OutputRingBuffer::new(OUTPUT_RING_BUFFER_CAPACITY)),
    );
    state.vt_log_buffers.insert(
        session_id.clone(),
        Mutex::new(VtLogBuffer::new(24, 220, VT_LOG_BUFFER_CAPACITY)),
    );

    spawn_reader_thread(
        reader,
        paused,
        session_id.clone(),
        app,
        state.inner().clone(),
    );

    Ok(WorktreeResult {
        session_id,
        worktree_path: worktree_path.to_string_lossy().to_string(),
        branch,
    })
}

/// List all active worktrees
#[tauri::command]
pub(crate) fn list_worktrees(state: State<'_, Arc<AppState>>) -> Vec<serde_json::Value> {
    state.sessions
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

/// Write data to a PTY session
#[tauri::command]
pub(crate) fn write_pty(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    session_id: String,
    data: String,
) -> Result<(), String> {
    if let Some(entry) = state.sessions.get(&session_id) {
        let mut session = entry.lock();
        session
            .writer
            .write_all(data.as_bytes())
            .map_err(|e| format!("Failed to write to PTY: {e}"))?;
        session
            .writer
            .flush()
            .map_err(|e| format!("Failed to flush PTY: {e}"))?;

        // Feed input through the line buffer to reconstruct user-typed lines
        let input_entry = state
            .input_buffers
            .entry(session_id.clone())
            .or_insert_with(|| parking_lot::Mutex::new(InputLineBuffer::new()));
        let mut buf = input_entry.lock();
        let actions = buf.feed(&data);
        let mut line_submitted = false;
        for action in actions {
            match action {
                InputAction::Line(content) => {
                    line_submitted = true;
                    if !content.is_empty() {
                        // Store as last relevant prompt if >= 10 words
                        let word_count = content.split_whitespace().count();
                        if word_count >= 10 {
                            state.last_prompts.insert(session_id.clone(), content.clone());
                        }
                        let parsed = ParsedEvent::UserInput { content };
                        // Broadcast to SSE/WebSocket consumers
                        if let Ok(json) = serde_json::to_value(&parsed) {
                            let _ = state.event_bus.send(crate::state::AppEvent::PtyParsed {
                                session_id: session_id.clone(),
                                parsed: json,
                            });
                        }
                        // Tauri IPC for desktop backward compat
                        let _ = app.emit(
                            &format!("pty-parsed-{session_id}"),
                            &parsed,
                        );

                        // Suppress silence-based question detection for user-typed lines.
                        // The PTY will echo this input back — without suppression, a line
                        // ending with `?` would be mistaken for an agent question prompt.
                        if let Some(ss) = state.silence_states.get(&session_id) {
                            ss.lock().suppress_user_input();
                        }
                    }
                }
                InputAction::Interrupt => {
                    line_submitted = true;
                }
            }
        }

        // Track slash command mode: true when the input buffer starts with /
        let in_slash = if line_submitted {
            false
        } else {
            buf.content().starts_with('/')
        };
        state
            .slash_mode
            .entry(session_id.clone())
            .or_insert_with(|| std::sync::atomic::AtomicBool::new(false))
            .store(in_slash, std::sync::atomic::Ordering::Relaxed);

        Ok(())
    } else {
        Err("Session not found".to_string())
    }
}

/// Get the last relevant user prompt (>= 10 words) for a PTY session.
#[tauri::command]
pub(crate) fn get_last_prompt(
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> Option<String> {
    state.last_prompts.get(&session_id).map(|v| v.clone())
}

/// Resize a PTY session
#[tauri::command]
pub(crate) fn resize_pty(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    rows: u16,
    cols: u16,
) -> Result<(), String> {
    if rows == 0 || cols == 0 {
        return Err("Invalid dimensions: rows and cols must be > 0".to_string());
    }
    let entry = state.sessions
        .get(&session_id)
        .ok_or_else(|| format!("Session not found: {session_id}"))?;
    let session = entry.lock();
    session
        .master
        .resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("Failed to resize PTY: {e}"))?;
    // Resize VT log buffer dimensions to match new terminal size.
    if let Some(vt_log) = state.vt_log_buffers.get(&session_id) {
        vt_log.lock().resize(rows, cols);
    }
    // Mark resize in silence state so the reader thread suppresses re-parsed events
    // from the shell's prompt redraw triggered by SIGWINCH.
    if let Some(ss) = state.silence_states.get(&session_id) {
        ss.lock().on_resize();
    }
    Ok(())
}

/// Pause PTY reader thread (flow control: frontend buffer full)
#[tauri::command]
pub(crate) fn pause_pty(state: State<'_, Arc<AppState>>, session_id: String) -> Result<(), String> {
    let entry = state.sessions
        .get(&session_id)
        .ok_or_else(|| format!("Session not found: {session_id}"))?;
    entry.lock().paused.store(true, Ordering::Relaxed);
    state.metrics.pauses_triggered.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

/// Resume PTY reader thread (flow control: frontend buffer drained)
#[tauri::command]
pub(crate) fn resume_pty(state: State<'_, Arc<AppState>>, session_id: String) -> Result<(), String> {
    let entry = state.sessions
        .get(&session_id)
        .ok_or_else(|| format!("Session not found: {session_id}"))?;
    entry.lock().paused.store(false, Ordering::Relaxed);
    Ok(())
}

/// Query current kitty keyboard protocol flags for a session.
/// Returns 0 if the session has no kitty state (protocol not activated).
#[tauri::command]
pub(crate) fn get_kitty_flags(
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> u32 {
    state
        .kitty_states
        .get(&session_id)
        .map(|entry| entry.lock().current_flags())
        .unwrap_or(0)
}

/// Close a PTY session with graceful shutdown and optional worktree cleanup.
/// Sends Ctrl-C (0x03) and waits briefly for the process to exit cleanly
/// before forcibly dropping handles.
#[tauri::command]
pub(crate) fn close_pty(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    cleanup_worktree: bool,
) -> Result<(), String> {
    if let Some((_, session_mutex)) = state.sessions.remove(&session_id) {
        state.output_buffers.remove(&session_id);
        state.vt_log_buffers.remove(&session_id);
        state.ws_clients.remove(&session_id);
        state.kitty_states.remove(&session_id);
        state.input_buffers.remove(&session_id);
        state.silence_states.remove(&session_id);
        state.metrics.active_sessions.fetch_sub(1, Ordering::Relaxed);
        let mut session = session_mutex.into_inner();

        // Send Ctrl-C (0x03) to give the process a chance to clean up
        let _ = session.writer.write_all(&[0x03]);
        let _ = session.writer.flush();

        // Wait up to 100ms for process to exit gracefully
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(100);
        loop {
            match session._child.try_wait() {
                Ok(Some(_)) => break, // Process exited cleanly
                Ok(None) if std::time::Instant::now() >= deadline => break,
                _ => std::thread::sleep(std::time::Duration::from_millis(10)),
            }
        }

        // Extract worktree info before dropping session
        let worktree_to_cleanup = if cleanup_worktree {
            session.worktree.clone()
        } else {
            None
        };

        // Drop session to release file handles (forcibly kills if still running)
        drop(session);

        // Cleanup worktree if requested
        if let Some(worktree) = worktree_to_cleanup
            && let Err(e) = remove_worktree_internal(&worktree) {
                eprintln!("Warning: Failed to cleanup worktree: {e}");
            }
    }

    Ok(())
}

/// Look up the process name for a given PID using `ps`.
/// Returns None on Windows or if the lookup fails.
#[cfg(not(windows))]
pub(crate) fn process_name_from_pid(pid: u32) -> Option<String> {
    let output = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "comm="])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if name.is_empty() {
        return None;
    }
    // ps may return full path; extract just the binary name
    name.rsplit('/').next().map(|s| s.to_string())
}

#[cfg(windows)]
pub(crate) fn process_name_from_pid(pid: u32) -> Option<String> {
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32First, Process32Next,
        PROCESSENTRY32, TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::Foundation::CloseHandle;

    // SAFETY: CreateToolhelp32Snapshot/Process32First/Process32Next are Windows API
    // functions that operate on a process snapshot handle. We zero-initialize the
    // PROCESSENTRY32 struct and set dwSize before use (required by the API). The
    // snapshot handle is closed via CloseHandle before returning. All pointer
    // arguments point to stack-local owned memory with valid lifetimes.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
            return None;
        }

        let mut entry: PROCESSENTRY32 = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32>() as u32;

        let mut found = None;
        if Process32First(snapshot, &mut entry) != 0 {
            loop {
                if entry.th32ProcessID == pid {
                    // szExeFile is a [i8; 260] (MAX_PATH) null-terminated C string
                    let name_bytes: Vec<u8> = entry
                        .szExeFile
                        .iter()
                        .take_while(|&&b| b != 0)
                        .map(|&b| b as u8)
                        .collect();
                    // Use from_utf8_lossy to handle non-ASCII process names
                    // (e.g. apps with accented characters) instead of silently dropping them
                    let name = String::from_utf8_lossy(&name_bytes);
                    // Strip .exe suffix for consistent matching with classify_agent
                    let name = name.strip_suffix(".exe").unwrap_or(&name).to_string();
                    found = Some(name);
                    break;
                }
                if Process32Next(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }

        CloseHandle(snapshot);
        found
    }
}

/// Walk the process tree from `root_pid` and return the deepest descendant PID.
/// On Windows, this finds the "foreground" process in a PTY session by following
/// Normalize a path by resolving `.` and `..` components logically
/// (without requiring the path to exist on disk).
fn normalize_path(path: &std::path::Path) -> std::path::PathBuf {
    let mut result = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => { result.pop(); }
            std::path::Component::CurDir => {}
            other => result.push(other),
        }
    }
    result
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
        let p = normalize_path(Path::new("/home/user/repo__wt/feat/../../repo/plans/foo.md"));
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

/// the chain: shell → agent CLI (e.g. claude.exe).
#[cfg(windows)]
fn deepest_descendant_pid(root_pid: u32) -> Option<u32> {
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32First, Process32Next,
        PROCESSENTRY32, TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::Foundation::CloseHandle;

    // SAFETY: Same API contract as process_name_from_pid above. We take a full
    // process snapshot, iterate it to collect (pid, parent_pid) pairs into owned
    // Vecs, then close the handle. The PROCESSENTRY32 struct is zero-initialized
    // with dwSize set before the first call, satisfying the API precondition.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
            return None;
        }

        // Collect all (pid, parent_pid) pairs and build parent->children map
        let mut children_map: std::collections::HashMap<u32, Vec<u32>> = std::collections::HashMap::new();
        let mut entry: PROCESSENTRY32 = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32>() as u32;

        if Process32First(snapshot, &mut entry) != 0 {
            loop {
                children_map
                    .entry(entry.th32ParentProcessID)
                    .or_default()
                    .push(entry.th32ProcessID);
                if Process32Next(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snapshot);

        // Walk from root_pid to the deepest single child — O(depth) via HashMap
        let mut current = root_pid;
        while let Some([only_child]) = children_map.get(&current).map(Vec::as_slice) {
            current = *only_child;
        }

        Some(current)
    }
}

/// Map a process name to a known agent type, or None for non-agent processes.
pub(crate) fn classify_agent(process_name: &str) -> Option<&'static str> {
    match process_name {
        "claude" => Some("claude"),
        "gemini" => Some("gemini"),
        "opencode" => Some("opencode"),
        "aider" => Some("aider"),
        "codex" => Some("codex"),
        "amp" => Some("amp"),
        "cursor-agent" => Some("cursor"),
        "oz" => Some("warp"),
        _ => None,
    }
}

/// Get the foreground process of a PTY session and classify it as a known agent.
/// Returns the agent name (e.g. "claude") or None if the foreground process is
/// not a recognized agent or the session doesn't exist.
#[tauri::command]
pub(crate) fn get_session_foreground_process(
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> Option<String> {
    let entry = state.sessions.get(&session_id)?;
    let session = entry.value().lock();
    #[cfg(not(windows))]
    {
        let pgid = session.master.process_group_leader()?;
        let name = process_name_from_pid(pgid as u32)?;
        classify_agent(&name).map(|s| s.to_string())
    }
    #[cfg(windows)]
    {
        // On Windows, walk the process tree from the shell child to find the
        // deepest descendant — the equivalent of the "foreground process".
        let child_pid = session._child.process_id()?;
        let leaf = deepest_descendant_pid(child_pid)?;
        let name = process_name_from_pid(leaf)?;
        classify_agent(&name).map(|s| s.to_string())
    }
}

/// Get orchestrator stats
#[tauri::command]
pub(crate) fn get_orchestrator_stats(state: State<'_, Arc<AppState>>) -> OrchestratorStats {
    state.orchestrator_stats()
}

/// Get PTY session metrics for observability
#[tauri::command]
pub(crate) fn get_session_metrics(state: State<'_, Arc<AppState>>) -> serde_json::Value {
    state.session_metrics_json()
}

/// Check if we can spawn a new session
#[tauri::command]
pub(crate) fn can_spawn_session(state: State<'_, Arc<AppState>>) -> bool {
    state.sessions.len() < MAX_CONCURRENT_SESSIONS
}

/// Info about an active PTY session for frontend reconnection
#[derive(Clone, Serialize)]
pub(crate) struct ActiveSessionInfo {
    session_id: String,
    cwd: Option<String>,
    worktree_path: Option<String>,
    worktree_branch: Option<String>,
}

/// Update the working directory of a running PTY session.
/// Called from the frontend when an OSC 7 escape sequence is detected,
/// keeping the Rust-side cwd in sync for restart recovery.
#[tauri::command]
pub(crate) fn update_session_cwd(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    cwd: String,
) -> Result<(), String> {
    let path = std::path::Path::new(&cwd);
    if !path.is_absolute() {
        return Err("cwd must be an absolute path".into());
    }
    if cwd.contains('\0') {
        return Err("cwd must not contain null bytes".into());
    }
    let entry = state
        .sessions
        .get(&session_id)
        .ok_or_else(|| format!("Session not found: {session_id}"))?;
    entry.lock().cwd = Some(cwd);
    Ok(())
}

/// List all active PTY sessions for reconnection after frontend reload
#[tauri::command]
pub(crate) fn list_active_sessions(state: State<'_, Arc<AppState>>) -> Vec<ActiveSessionInfo> {
    state
        .sessions
        .iter()
        .map(|entry| {
            let session_id = entry.key().clone();
            let session = entry.value().lock();
            ActiveSessionInfo {
                session_id,
                cwd: session.cwd.clone(),
                worktree_path: session
                    .worktree
                    .as_ref()
                    .map(|w| w.path.to_string_lossy().to_string()),
                worktree_branch: session.worktree.as_ref().and_then(|w| w.branch.clone()),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_classify_agent_unknown_returns_none() {
        assert_eq!(classify_agent("bash"), None);
        assert_eq!(classify_agent("zsh"), None);
        assert_eq!(classify_agent("node"), None);
        assert_eq!(classify_agent("python"), None);
        assert_eq!(classify_agent("vim"), None);
    }

    // --- SilenceState tests ---

    #[test]
    fn test_silence_state_no_pending_returns_none() {
        let mut s = SilenceState::new();
        assert!(s.check_silence().is_none());
    }

    #[test]
    fn test_silence_state_pending_but_too_soon() {
        let mut s = SilenceState::new();
        s.on_chunk(false, Some("Continue?".to_string()));
        // Just set — not enough time has passed
        assert!(s.check_silence().is_none());
    }

    #[test]
    fn test_silence_state_pending_after_threshold() {
        let mut s = SilenceState::new();
        s.on_chunk(false, Some("Continue?".to_string()));
        // Simulate time passing by backdating last_output_at
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert_eq!(s.check_silence(), Some("Continue?".to_string()));
    }

    #[test]
    fn test_silence_state_no_double_emission() {
        let mut s = SilenceState::new();
        s.on_chunk(false, Some("Continue?".to_string()));
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert!(s.check_silence().is_some());
        // Second check should return None (already emitted)
        assert!(s.check_silence().is_none());
    }

    #[test]
    fn test_silence_state_regex_suppresses_timer() {
        let mut s = SilenceState::new();
        // regex_found_question = true means instant detection already fired
        s.on_chunk(true, Some("Would you like to proceed?".to_string()));
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert!(s.check_silence().is_none());
    }

    #[test]
    fn test_silence_state_new_output_clears_pending() {
        let mut s = SilenceState::new();
        s.on_chunk(false, Some("Continue?".to_string()));
        // Agent prints more non-question output — it moved past the question
        s.on_chunk(false, None);
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert!(s.check_silence().is_none());
    }

    #[test]
    fn test_silence_state_new_question_replaces_old() {
        let mut s = SilenceState::new();
        s.on_chunk(false, Some("First question?".to_string()));
        s.on_chunk(false, Some("Second question?".to_string()));
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert_eq!(s.check_silence(), Some("Second question?".to_string()));
    }

    #[test]
    fn test_silence_state_suppress_user_input() {
        let mut s = SilenceState::new();
        // User types a line ending with `?` — PTY will echo it back
        s.on_chunk(false, Some("c'è ancora una storia?".to_string()));
        // write_pty detects user input and suppresses
        s.suppress_user_input();
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        // Should NOT fire — the question was typed by the user
        assert!(s.check_silence().is_none());
    }

    // --- Resize grace period tests ---

    #[test]
    fn test_resize_grace_active_immediately_after_resize() {
        let mut s = SilenceState::new();
        s.on_resize();
        assert!(s.is_resize_grace(), "grace period should be active right after resize");
    }

    #[test]
    fn test_resize_grace_inactive_before_resize() {
        let s = SilenceState::new();
        assert!(!s.is_resize_grace(), "grace period should be inactive with no resize");
    }

    #[test]
    fn test_resize_grace_expires_after_threshold() {
        let mut s = SilenceState::new();
        s.on_resize();
        // Backdating the resize timestamp past the grace period
        s.last_resize_at = Some(std::time::Instant::now() - RESIZE_GRACE - std::time::Duration::from_millis(100));
        assert!(!s.is_resize_grace(), "grace period should have expired");
    }

    #[test]
    fn test_resize_grace_refreshed_on_second_resize() {
        let mut s = SilenceState::new();
        s.on_resize();
        // Expire the first grace period
        s.last_resize_at = Some(std::time::Instant::now() - RESIZE_GRACE - std::time::Duration::from_millis(100));
        assert!(!s.is_resize_grace());
        // Second resize refreshes the timer
        s.on_resize();
        assert!(s.is_resize_grace(), "second resize should restart grace period");
    }

    // --- Agent Teams env injection tests ---

    #[test]
    fn agent_teams_env_sets_iterm_session_id() {
        let env = AgentTeamsEnv { session_id: "abc-123".to_string(), http_port: 9876, socket_path: "/tmp/mcp.sock".to_string() };
        let vars = env.env_overrides();
        let val = vars.iter().find(|(k, _)| k == "ITERM_SESSION_ID").map(|(_, v)| v.as_str());
        assert_eq!(val, Some("w0t0p0:abc-123"));
    }

    #[test]
    fn agent_teams_env_sets_term_program() {
        let env = AgentTeamsEnv { session_id: "x".to_string(), http_port: 8080, socket_path: "/tmp/mcp.sock".to_string() };
        let vars = env.env_overrides();
        let val = vars.iter().find(|(k, _)| k == "TERM_PROGRAM").map(|(_, v)| v.as_str());
        assert_eq!(val, Some("iTerm.app"));
    }

    #[test]
    fn agent_teams_env_prepends_tuic_bin_to_path() {
        let env = AgentTeamsEnv { session_id: "x".to_string(), http_port: 8080, socket_path: "/tmp/mcp.sock".to_string() };
        let vars = env.env_overrides();
        let path_val = vars.iter().find(|(k, _)| k == "PATH").map(|(_, v)| v.clone()).unwrap();
        assert!(path_val.starts_with(&format!("{}/.tuicommander/bin:", std::env::var("HOME").unwrap_or_default())));
    }

    #[test]
    fn agent_teams_env_sets_tuic_http_port() {
        let env = AgentTeamsEnv { session_id: "x".to_string(), http_port: 9876, socket_path: "/tmp/mcp.sock".to_string() };
        let vars = env.env_overrides();
        let val = vars.iter().find(|(k, _)| k == "TUIC_HTTP_PORT").map(|(_, v)| v.as_str());
        assert_eq!(val, Some("9876"));
    }

    #[test]
    fn agent_teams_env_sets_tuic_socket_path() {
        let env = AgentTeamsEnv { session_id: "x".to_string(), http_port: 8080, socket_path: "/tmp/test.sock".to_string() };
        let vars = env.env_overrides();
        let val = vars.iter().find(|(k, _)| k == "TUIC_SOCKET_PATH").map(|(_, v)| v.as_str());
        assert_eq!(val, Some("/tmp/test.sock"));
    }

    // --- VtLogBuffer + parse_clean_lines pipeline tests ---

    /// VtLogBuffer changed rows feed parse_clean_lines and produce a StatusLine event
    /// for normal screen output.
    #[test]
    fn test_vt_log_pipeline_status_line_normal_screen() {
        use crate::state::VtLogBuffer;
        use crate::output_parser::{OutputParser, ParsedEvent};

        let mut vt_log = VtLogBuffer::new(24, 80, 1000);
        let mut parser = OutputParser::new();

        let changed = vt_log.process(b"* Reading files...");
        let events = parser.parse_clean_lines(&changed);
        assert!(
            events.iter().any(|e| matches!(e, ParsedEvent::StatusLine { .. })),
            "expected StatusLine from normal screen, got: {:?}", events
        );
    }

    /// VtLogBuffer changed rows feed parse_clean_lines and produce an Intent event
    /// during alternate screen (e.g. Claude Code / Ink).
    #[test]
    fn test_vt_log_pipeline_intent_alternate_screen() {
        use crate::state::VtLogBuffer;
        use crate::output_parser::{OutputParser, ParsedEvent};

        let mut vt_log = VtLogBuffer::new(24, 80, 1000);
        let mut parser = OutputParser::new();

        // Enter alternate screen (smcup: ESC[?1049h)
        vt_log.process(b"\x1b[?1049h");
        let changed = vt_log.process(b"[[intent: Doing work(Test)]]");
        let events = parser.parse_clean_lines(&changed);
        assert!(
            events.iter().any(|e| matches!(e, ParsedEvent::Intent { .. })),
            "expected Intent from alternate screen, got: {:?}", events
        );
    }

    /// parse_osc94 is called on raw data (OSC 9;4 is invisible in clean rows).
    #[test]
    fn test_osc94_from_raw_stream() {
        use crate::output_parser::{parse_osc94, ParsedEvent};

        let raw = "\x1b]9;4;1;50\x07"; // OSC 9;4 progress 50%
        let event = parse_osc94(raw);
        assert!(
            matches!(event, Some(ParsedEvent::Progress { .. })),
            "expected Progress from raw OSC 9;4, got: {:?}", event
        );
    }

    /// extract_last_question_line_from_changed_rows: given changed rows, we can derive
    /// the silence-tracker question line from the last non-empty row.
    #[test]
    fn test_silence_question_from_changed_rows() {
        use crate::state::VtLogBuffer;

        let mut vt_log = VtLogBuffer::new(24, 80, 1000);
        let changed = vt_log.process(b"Would you like to proceed?");
        let last_q = changed.iter().rev().find(|r| !r.text.is_empty()).map(|r| r.text.clone());
        assert_eq!(last_q.as_deref(), Some("Would you like to proceed?"));
    }

    // --- Headless reader structured event tests ---

    /// The headless reader logic: after process(), parse_clean_lines produces events.
    /// This verifies the core data flow without spawning a full AppState.
    #[test]
    fn test_headless_reader_intent_event_logic() {
        use crate::state::VtLogBuffer;
        use crate::output_parser::{OutputParser, ParsedEvent};

        let mut vt_log = VtLogBuffer::new(24, 80, 1000);
        let mut parser = OutputParser::new();

        let changed = vt_log.process(b"[[intent: Testing headless reader]]");
        let events = parser.parse_clean_lines(&changed);

        assert!(
            events.iter().any(|e| matches!(e, ParsedEvent::Intent { .. })),
            "expected Intent from headless reader logic, got: {:?}", events
        );
    }

    /// The headless reader emits events for alternate screen content (e.g. Claude Code).
    #[test]
    fn test_headless_reader_alternate_screen_events() {
        use crate::state::VtLogBuffer;
        use crate::output_parser::{OutputParser, ParsedEvent};

        let mut vt_log = VtLogBuffer::new(24, 80, 1000);
        let mut parser = OutputParser::new();

        vt_log.process(b"\x1b[?1049h"); // enter alternate screen
        let changed = vt_log.process(b"* Reading files...");
        let events = parser.parse_clean_lines(&changed);

        assert!(
            events.iter().any(|e| matches!(e, ParsedEvent::StatusLine { .. })),
            "headless reader must detect StatusLine during alternate screen, got: {:?}", events
        );
    }
}
