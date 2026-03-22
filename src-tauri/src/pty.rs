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
    AppState, ChangedRow, EscapeAwareBuffer, KittyAction, KittyKeyboardState, OrchestratorStats,
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

/// Build a CommandBuilder for the given shell with platform-appropriate flags.
pub(crate) fn build_shell_command(shell: &str) -> CommandBuilder {
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
        // Agent Teams: always inject feature flag so CC unlocks team tools
        cmd.env("CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS", "1");
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

/// Maximum non-`?` chunks allowed after a `?` candidate before considering it stale.
/// Claude Code prints 2-3 decoration chunks after a question (mode line, separator).
/// Anything beyond this threshold means the agent continued working — not waiting.
const STALE_QUESTION_CHUNKS: u32 = 10;

/// How often the timer thread wakes up to check for silence.
const SILENCE_CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

/// Grace period after a PTY resize during which parsed events (Question, RateLimit,
/// ApiError) are suppressed. The shell redraws visible output after SIGWINCH, which
/// would otherwise re-trigger notifications for content already on screen.
const RESIZE_GRACE: std::time::Duration = std::time::Duration::from_millis(1000);

/// How long after user input to ignore `?`-ending echo lines from the PTY.
const ECHO_SUPPRESS_WINDOW: std::time::Duration = std::time::Duration::from_millis(500);

/// Shell idle threshold: 500ms without real PTY output → transition busy→idle.
/// Matches the frontend's previous 500ms setTimeout in checkIdle.
const SHELL_IDLE_MS: u64 = 500;

/// AtomicU8 encoding for shell_states DashMap.
const SHELL_NULL: u8 = 0;
const SHELL_BUSY: u8 = 1;
const SHELL_IDLE: u8 = 2;

// Re-export from chrome module for use by this module and tests.
use crate::chrome::is_chrome_row;

/// Searches all changed rows (not just the last non-empty one) so a question row
/// is found even when a mode/status line with a higher row index arrives in the same chunk.
/// Applies content filters to reject lines that are clearly not questions (code comments,
/// diff context, markdown headers, code syntax).
pub(crate) fn extract_question_line(changed_rows: &[ChangedRow]) -> Option<String> {
    changed_rows.iter().rev()
        .find(|r| !r.text.is_empty() && r.text.ends_with('?') && is_plausible_question(&r.text))
        .map(|r| r.text.clone())
}

/// Returns false for lines that are clearly not questions: code comments, diff context,
/// markdown headers, and lines containing code-specific syntax.
fn is_plausible_question(line: &str) -> bool {
    let trimmed = line.trim_start();
    // Comment/diff/markdown prefixes
    if trimmed.starts_with("//")
        || trimmed.starts_with('#')
        || trimmed.starts_with('*')
        || trimmed.starts_with('+')
        || trimmed.starts_with('-')
        || trimmed.starts_with('>')
    {
        return false;
    }
    // Code syntax markers — real questions don't contain these
    if line.contains("->") || line.contains("=>") || line.contains("::") {
        return false;
    }
    // Code try-syntax: word_or_> followed by (...)? — e.g. foo()?, bar(x)?, Vec<T>()?
    // But NOT human option parentheticals like (y/n)?, (yes/no)? where `(` is
    // preceded by whitespace or start-of-line, not a word character.
    lazy_static::lazy_static! {
        static ref CODE_TRY_RE: regex::Regex =
            regex::Regex::new(r"[\w>]\([^)]*\)\?").unwrap();
    }
    if CODE_TRY_RE.is_match(line) {
        return false;
    }
    true
}

/// Verify that a question candidate is still visible among the bottom rows of the
/// terminal screen. Returns true only if the exact question text appears as a
/// complete row (trimmed) within the last `max_bottom_rows` non-empty lines.
/// This prevents ghost notifications from stale `?` lines that have scrolled off.
pub(crate) fn verify_question_on_screen(screen_rows: &[String], question: &str, max_bottom_rows: usize) -> bool {
    let q = question.trim();
    screen_rows.iter().rev()
        .filter(|r| !r.is_empty())
        .take(max_bottom_rows)
        .any(|r| r.trim() == q)
}

use crate::chrome::{is_separator_line, is_prompt_line};

/// Extract the last chat line from the terminal screen by locating the prompt
/// line and returning the first non-empty, non-chrome line above it.
///
/// Works across agent UIs:
/// - **Claude Code**: separator / `> input` / separator / mode line
/// - **Codex**: (empty) / `› input` / status line
/// - **Gemini**: `> input` / status line
///
/// Algorithm:
/// 1. Scan from bottom, find the prompt line (`>`, `› `, `❯`)
/// 2. Walk up past separator lines and empty lines
/// 3. First non-empty, non-separator line = last chat line
pub(crate) fn extract_last_chat_line(screen_rows: &[String]) -> Option<String> {
    // Scan from the bottom, find the prompt line.
    let prompt_idx = screen_rows.iter().enumerate().rev()
        .find(|(_, row)| is_prompt_line(row))?
        .0;

    // Walk upward past separator lines and empty lines to find the last chat line.
    for i in (0..prompt_idx).rev() {
        let trimmed = screen_rows[i].trim();
        if !trimmed.is_empty() && !is_separator_line(trimmed) {
            return Some(trimmed.to_string());
        }
    }
    None
}

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
    /// Deadline until which `on_chunk` ignores `?`-ending lines (suppresses PTY echo).
    /// Set by `suppress_user_input()` so the echo of user-typed text doesn't
    /// re-enable silence-based question detection.
    pub(crate) suppress_echo_until: Option<std::time::Instant>,
    /// When the last StatusLine (spinner) event was seen. If recent,
    /// silence-based question detection is suppressed — spinner means the agent is working.
    pub(crate) last_status_line_at: Option<std::time::Instant>,
    /// How many non-`?` chunks arrived after the current pending question candidate.
    /// Used to detect stale candidates: if the agent continued producing significant
    /// output after the `?` line, it was not a real question.
    output_chunks_after_question: u32,
    /// The text of the last question emitted (by silence timer or check_silence).
    /// Used to prevent re-emission of the same question when scrolling causes the
    /// `?` line to reappear in changed_rows at a different row position.
    /// Cleared on user input (new conversation cycle).
    last_emitted_text: Option<String>,
}

impl SilenceState {
    fn new() -> Self {
        Self {
            last_output_at: std::time::Instant::now(),
            pending_question_line: None,
            question_already_emitted: false,
            last_resize_at: None,
            suppress_echo_until: None,
            last_status_line_at: None,
            output_chunks_after_question: 0,
            last_emitted_text: None,
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
    /// `has_status_line`: true if the chunk contained a StatusLine parsed event.
    /// `status_line_only`: true if the chunk contained ONLY status-line/mode-line updates.
    ///   Mode-line timer ticks (elapsed time updating every second) are not significant
    ///   output — they must not reset the silence timer or the spinner timestamp,
    ///   or questions asked by Ink agents will never be detected.
    pub(crate) fn on_chunk(&mut self, regex_found_question: bool, last_question_line: Option<String>, has_status_line: bool, status_line_only: bool) {
        if !status_line_only {
            self.last_output_at = std::time::Instant::now();
        }

        // Only mark spinner active when the status line accompanies real output.
        // Mode-line timer ticks (status_line_only) just update elapsed time —
        // they are not agent activity and must not suppress question detection.
        if has_status_line && !status_line_only {
            self.last_status_line_at = Some(std::time::Instant::now());
        }

        // Within the echo suppress window, ignore `?`-ending lines — they are
        // the PTY echoing back user-typed text, not agent questions.
        let in_echo_window = self.suppress_echo_until
            .map(|deadline| std::time::Instant::now() < deadline)
            .unwrap_or(false);

        if regex_found_question {
            // The instant detector already fired — suppress the silence timer.
            self.pending_question_line = None;
            self.question_already_emitted = true;
            self.output_chunks_after_question = 0;
        } else if let Some(line) = last_question_line {
            if in_echo_window {
                // Ignore — this is the PTY echo of user input.
            } else if self.question_already_emitted
                && (self.pending_question_line.as_deref() == Some(&line)
                    || self.last_emitted_text.as_deref() == Some(&line))
            {
                // Same `?` text as already emitted (either still pending, or
                // previously emitted and reappearing because new output scrolled
                // it to a different row). Don't reset — otherwise the silence
                // timer will re-fire for every scroll of the same question.
            } else {
                // New candidate for silence-based detection.
                self.pending_question_line = Some(line);
                self.question_already_emitted = false;
                self.output_chunks_after_question = 0;
            }
        } else if self.pending_question_line.is_some() && !status_line_only {
            // Non-`?` chunk with real output after a pending candidate — track staleness.
            // Mode-line timer ticks (status_line_only) are NOT real output and must not
            // count toward staleness, or they will clear the pending question before
            // the silence timer has a chance to detect it.
            self.output_chunks_after_question = self.output_chunks_after_question.saturating_add(1);
            // Once stale, clear pending so the repaint guard won't block the
            // same question text from being detected again in a future session.
            if self.output_chunks_after_question > STALE_QUESTION_CHUNKS {
                self.pending_question_line = None;
            }
        }
    }

    /// Called by write_pty when the user submits a line of input.
    /// Clears any pending question candidate since it was typed by the user, not the agent.
    /// Also opens a time window to ignore the PTY echo of the typed text.
    pub(crate) fn suppress_user_input(&mut self) {
        self.pending_question_line = None;
        self.question_already_emitted = true;
        self.last_emitted_text = None;
        self.suppress_echo_until = Some(std::time::Instant::now() + ECHO_SUPPRESS_WINDOW);
    }


    /// Returns true if a spinner/status-line was seen recently.
    /// Uses the same threshold as silence detection (10s) so that agents with
    /// pauses between status-line updates (API calls, file reads) don't trigger
    /// false question notifications during those gaps.
    fn is_spinner_active(&self) -> bool {
        self.last_status_line_at
            .map(|t| t.elapsed() < SILENCE_QUESTION_THRESHOLD)
            .unwrap_or(false)
    }

    /// Called by the timer thread. Returns the question text if the silence
    /// threshold has been reached and we haven't emitted yet.
    pub(crate) fn check_silence(&mut self) -> Option<String> {
        if self.question_already_emitted {
            return None;
        }
        // Spinner active means the agent is working — not waiting for input.
        if self.is_spinner_active() {
            return None;
        }
        // Too much output after the `?` line — the agent continued working,
        // so the `?` was not a real question (e.g. code comment, markdown).
        if self.output_chunks_after_question > STALE_QUESTION_CHUNKS {
            return None;
        }
        if let Some(ref line) = self.pending_question_line
            && self.last_output_at.elapsed() >= SILENCE_QUESTION_THRESHOLD
        {
            self.question_already_emitted = true;
            self.last_emitted_text = Some(line.clone());
            return Some(line.clone());
        }
        None
    }

    /// Clear a stale question candidate that failed screen verification.
    /// Prevents the timer from re-checking the same stale candidate every second.
    pub(crate) fn clear_stale_question(&mut self) {
        self.pending_question_line = None;
        self.question_already_emitted = true;
    }

    /// Returns true if the session has been silent long enough and the spinner
    /// is not active. Used by the timer thread before reading the screen.
    pub(crate) fn is_silent(&self) -> bool {
        !self.question_already_emitted
            && !self.is_spinner_active()
            && self.last_output_at.elapsed() >= SILENCE_QUESTION_THRESHOLD
    }

    /// Mark that a question has been emitted (prevents re-emission).
    /// Stores the emitted text so that scroll-induced reappearances of the same
    /// `?` line in changed_rows are recognized as duplicates, not new questions.
    pub(crate) fn mark_emitted(&mut self, text: &str) {
        self.question_already_emitted = true;
        self.last_emitted_text = Some(text.to_string());
    }
}

/// Attempt a shell state transition using compare_exchange.
/// Returns true if the transition was performed (and a ShellState event should be emitted).
fn try_shell_transition(
    state: &crate::state::AppState,
    session_id: &str,
    expected: u8,
    new: u8,
) -> bool {
    if let Some(atom) = state.shell_states.get(session_id) {
        atom.compare_exchange(
            expected,
            new,
            std::sync::atomic::Ordering::AcqRel,
            std::sync::atomic::Ordering::Relaxed,
        ).is_ok()
    } else {
        false
    }
}

/// Check whether the session should transition to idle (busy → idle).
/// Conditions: last real output > SHELL_IDLE_MS ago AND no active sub-tasks.
fn should_transition_idle(state: &crate::state::AppState, session_id: &str) -> bool {
    let last_ms = state.last_output_ms.get(session_id)
        .map(|ts| ts.load(std::sync::atomic::Ordering::Relaxed))
        .unwrap_or(0);
    if last_ms == 0 {
        return false;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let elapsed = now.saturating_sub(last_ms);
    if elapsed < SHELL_IDLE_MS {
        return false;
    }
    let sub_tasks = state.session_states.get(session_id)
        .map(|s| s.active_sub_tasks)
        .unwrap_or(0);
    sub_tasks == 0
}

/// Emit a ShellState parsed event via both event bus and Tauri IPC.
fn emit_shell_state(
    state: &crate::state::AppState,
    app: Option<&tauri::AppHandle>,
    session_id: &str,
    shell_state: &str,
) {
    let parsed = ParsedEvent::ShellState { state: shell_state.to_string() };
    if let Ok(json) = serde_json::to_value(&parsed) {
        let _ = state.event_bus.send(crate::state::AppEvent::PtyParsed {
            session_id: session_id.to_string(),
            parsed: json,
        });
    }
    if let Some(app) = app {
        let _ = app.emit(
            &format!("pty-parsed-{session_id}"),
            &parsed,
        );
    }
}

/// How many bottom screen rows to check when verifying a question candidate.
const SCREEN_VERIFY_ROWS: usize = 5;

/// Spawn the silence-detection timer thread. Shared by desktop and headless readers.
///
/// Two strategies run in priority order:
/// 1. **Screen-based**: read the terminal screen, find the last chat line above the
///    prompt box (delimited by two separator lines), check if it ends with `?`.
/// 2. **Chunk-based fallback**: use `check_silence()` with `pending_question_line`
///    for agents that don't have a prompt box (plain shell, etc.).
fn spawn_silence_timer(
    silence: Arc<Mutex<SilenceState>>,
    running: Arc<AtomicBool>,
    session_id: String,
    state: Arc<AppState>,
    app: Option<AppHandle>,
) {
    let event_bus = state.event_bus.clone();
    tokio::spawn(async move {
        while running.load(Ordering::Relaxed) {
            tokio::time::sleep(SILENCE_CHECK_INTERVAL).await;
            if !running.load(Ordering::Relaxed) {
                break;
            }

            // Backup idle check: when no chunks arrive at all (agent truly silent),
            // the reader thread never gets a chance to emit idle. The timer catches this.
            if let Some(atom) = state.shell_states.get(&session_id)
                && atom.load(std::sync::atomic::Ordering::Acquire) == SHELL_BUSY
                && should_transition_idle(&state, &session_id)
                && try_shell_transition(&state, &session_id, SHELL_BUSY, SHELL_IDLE)
            {
                emit_shell_state(&state, app.as_ref(), &session_id, "idle");
            }

            // Check temporal conditions first (shared by both strategies).
            let is_silent = silence.lock().is_silent();
            if !is_silent {
                continue;
            }

            // Strategy 1: screen-based — find last chat line above the prompt box.
            let screen_question = state.vt_log_buffers.get(&session_id)
                .and_then(|vt| {
                    let rows = vt.lock().screen_rows();
                    let line = extract_last_chat_line(&rows)?;
                    if line.ends_with('?') && is_plausible_question(&line) {
                        Some(line)
                    } else {
                        None
                    }
                });

            // Strategy 2: chunk-based fallback — pending_question_line + screen verify.
            let prompt_text = if let Some(line) = screen_question {
                line
            } else {
                let question = silence.lock().check_silence();
                match question {
                    Some(ref text) => {
                        let on_screen = state.vt_log_buffers.get(&session_id)
                            .map(|vt| verify_question_on_screen(
                                &vt.lock().screen_rows(),
                                text,
                                SCREEN_VERIFY_ROWS,
                            ))
                            .unwrap_or(false);
                        if !on_screen {
                            silence.lock().clear_stale_question();
                            continue;
                        }
                        text.clone()
                    }
                    None => continue,
                }
            };

            // Emit question event.
            silence.lock().mark_emitted(&prompt_text);
            let parsed = ParsedEvent::Question {
                prompt_text: prompt_text.clone(),
                confident: false,
            };
            if let Ok(json) = serde_json::to_value(&parsed) {
                let _ = event_bus.send(crate::state::AppEvent::PtyParsed {
                    session_id: session_id.clone(),
                    parsed: json,
                });
            }
            if let Some(ref app) = app {
                let _ = app.emit(
                    &format!("pty-parsed-{session_id}"),
                    &parsed,
                );
            }
        }
    });
}

// ---------------------------------------------------------------------------
// ChunkProcessor: shared output processing logic for desktop & headless readers
// ---------------------------------------------------------------------------

/// Per-session mutable state for processing PTY output chunks.
/// Holds dedup state, parser, and session CWD for PlanFile resolution.
/// Used by both `spawn_reader_thread` (desktop) and `spawn_headless_reader_thread`.
struct ChunkProcessor {
    parser: OutputParser,
    /// Dedup: only emit StatusLine when task_name actually changes
    last_status_task: Option<String>,
    /// Dedup: don't re-emit the same question prompt_text
    last_question_text: Option<String>,
    /// Session CWD for resolving relative plan-file paths
    session_cwd: Option<String>,
    /// Buffer for incomplete `[[intent:` or `[[suggest:` tokens split across chunks.
    /// When a chunk contains an opener without the closing `]]`, we hold the data
    /// and prepend it to the next chunk so colorize/conceal regexes see the full token.
    pending_xterm: Option<String>,
}

impl ChunkProcessor {
    fn new(session_cwd: Option<String>) -> Self {
        Self {
            parser: OutputParser::new(),
            last_status_task: None,
            last_question_text: None,
            session_cwd,
            pending_xterm: None,
        }
    }

    /// Apply colorize_intent / conceal_suggest to data destined for xterm,
    /// buffering incomplete tokens across chunks to prevent cosmetic flash.
    ///
    /// Returns `Some(data)` ready to emit, or `None` if the data was buffered
    /// (waiting for the closing `]]` in the next chunk).
    fn transform_xterm(&mut self, data: String) -> Option<String> {
        // Prepend any pending data from a previous incomplete token.
        let combined = if let Some(pending) = self.pending_xterm.take() {
            pending + &data
        } else {
            data
        };

        // Check for unclosed token openers.  We look for `[[intent:` or `[intent:`
        // (or suggest) WITHOUT a matching `]]` or `]` after them.
        if has_unclosed_token(&combined) {
            self.pending_xterm = Some(combined);
            return None;
        }

        // Full token present (or no token at all) — apply replacements.
        let result = if combined.contains("[intent:") {
            colorize_intent(&combined)
        } else {
            combined
        };
        let result = if result.contains("suggest:") {
            conceal_suggest(&result)
        } else {
            result
        };
        Some(result)
    }

    /// Flush any pending xterm data (e.g. on session end or when we decide
    /// the opener was a false positive). Returns data as-is without replacement.
    fn flush_pending_xterm(&mut self) -> Option<String> {
        self.pending_xterm.take()
    }

    /// Resolve a relative plan-file path to absolute using session CWD.
    /// Returns None if the path is relative and no CWD is available.
    fn resolve_planfile_path(&self, path: &str) -> Option<String> {
        if path.starts_with('/') {
            Some(path.to_string())
        } else if let Some(ref cwd) = self.session_cwd {
            let joined = std::path::PathBuf::from(cwd).join(path);
            Some(normalize_path(&joined).to_string_lossy().into_owned())
        } else {
            None
        }
    }

    /// Process a chunk of PTY output after kitty-sequence stripping.
    /// Handles: VT log buffer, ring buffer, WebSocket broadcast, event parsing,
    /// dedup, resize-grace filtering, PlanFile resolution, event emission,
    /// silence state, last_output_ms, and shell state transitions.
    ///
    /// Returns the data string if non-empty (for callers that need to emit raw output to xterm).
    /// `app` is Some for desktop mode (emits Tauri IPC), None for headless.
    fn process_chunk(
        &mut self,
        data: &str,
        silence: &Arc<Mutex<SilenceState>>,
        session_id: &str,
        state: &AppState,
        app: Option<&AppHandle>,
    ) -> Option<String> {
        if data.is_empty() {
            return None;
        }

        // Feed raw data (post-kitty-strip) into VT100 log buffer.
        let changed_rows = if let Some(vt_log) = state.vt_log_buffers.get(session_id) {
            vt_log.lock().process(data.as_bytes())
        } else {
            Vec::new()
        };

        // Write clean text to ring buffer for MCP consumers (no ANSI)
        if let Some(ring) = state.output_buffers.get(session_id) {
            ring.lock().write(data.as_bytes());
        }

        // Broadcast to WebSocket clients
        if let Some(mut clients) = state.ws_clients.get_mut(session_id) {
            let owned = data.to_owned();
            clients.retain(|tx| tx.send(owned.clone()).is_ok());
        }

        // Parse events: OSC 9;4 progress from raw stream, others from clean rows.
        let in_resize_grace = silence.lock().is_resize_grace();
        let mut events = Vec::new();
        if let Some(evt) = crate::output_parser::parse_osc94(data) {
            events.push(evt);
        }
        events.extend(self.parser.parse_clean_lines(&changed_rows));

        // Slash menu detection
        if state.slash_mode.get(session_id)
            .is_some_and(|v| v.load(std::sync::atomic::Ordering::Relaxed))
            && let Some(vt_log) = state.vt_log_buffers.get(session_id)
        {
            let screen = vt_log.lock().screen_rows();
            if let Some(evt) = crate::output_parser::parse_slash_menu(&screen) {
                events.push(evt);
            }
        }

        let regex_found_question = if in_resize_grace { false } else {
            events.iter().any(|e| matches!(e, ParsedEvent::Question { .. }))
        };

        // Emit events with dedup, resize-grace filtering, and PlanFile resolution.
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
                if self.last_status_task.as_deref() == Some(task_name.as_str()) {
                    continue;
                }
                self.last_status_task = Some(task_name.clone());
            }

            // Dedup question: skip if same prompt_text already emitted.
            if let ParsedEvent::Question { prompt_text, .. } = event {
                if self.last_question_text.as_deref() == Some(prompt_text.as_str()) {
                    continue;
                }
                self.last_question_text = Some(prompt_text.clone());
            }

            // Resolve relative plan-file paths to absolute using session CWD.
            // Skip plan-file events for files that don't exist on disk.
            let resolved = if let ParsedEvent::PlanFile { path } = event {
                match self.resolve_planfile_path(path) {
                    Some(p) if std::path::Path::new(&p).is_file() => {
                        tracing::info!("[plan-file] Detected: {p} (cwd={:?})", self.session_cwd);
                        Some(ParsedEvent::PlanFile { path: p })
                    }
                    Some(p) => {
                        tracing::warn!("[plan-file] File not found on disk: {p} (raw={path}, cwd={:?})", self.session_cwd);
                        continue;
                    }
                    None => {
                        tracing::warn!("[plan-file] Cannot resolve relative path: {path} (cwd={:?})", self.session_cwd);
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
                    session_id: session_id.to_string(),
                    parsed: json,
                });
            }

            // Tauri IPC for desktop mode
            if let Some(app) = app {
                let _ = app.emit(
                    &format!("pty-parsed-{session_id}"),
                    emit_event,
                );
            }
        }

        // Update silence state for fallback question detection.
        let has_status_line = events.iter().any(|e| matches!(e, ParsedEvent::StatusLine { .. }));
        let last_q_line = extract_question_line(&changed_rows);
        // A chunk is chrome-only when ALL changed rows are UI decoration.
        // Path 1: every row has a chrome marker (is_chrome_row).
        // Path 2: parse_status_line detected a spinner pattern (Gemini braille,
        //   Aider Knight Rider) AND no row contains real agent output. A row is
        //   "real output" if it is not chrome and not blank — this prevents
        //   has_status_line from suppressing chunks that mix spinner + output.
        let all_chrome_markers = changed_rows.iter().all(|r| is_chrome_row(&r.text));
        let no_real_output = changed_rows.iter().all(|r| {
            is_chrome_row(&r.text) || r.text.trim().is_empty()
        });
        let chrome_only = !regex_found_question
            && last_q_line.is_none()
            && !changed_rows.is_empty()
            && (all_chrome_markers || (has_status_line && no_real_output));
        {
            let mut sl = silence.lock();
            sl.on_chunk(regex_found_question, last_q_line, has_status_line, chrome_only);
        }

        // Stamp last_output_ms only for real output (not chrome-only ticks).
        if !chrome_only
            && let Some(ts) = state.last_output_ms.get(session_id)
        {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            ts.store(now, std::sync::atomic::Ordering::Relaxed);
        }

        // Shell state transitions (Rust is the single source of truth).
        if !chrome_only {
            if !silence.lock().is_resize_grace()
                && let Some(atom) = state.shell_states.get(session_id)
            {
                let prev = atom.load(std::sync::atomic::Ordering::Acquire);
                if prev != SHELL_BUSY
                    && try_shell_transition(state, session_id, prev, SHELL_BUSY)
                {
                    emit_shell_state(state, app, session_id, "busy");
                }
            }
        } else if let Some(atom) = state.shell_states.get(session_id)
            && atom.load(std::sync::atomic::Ordering::Acquire) == SHELL_BUSY
            && should_transition_idle(state, session_id)
            && try_shell_transition(state, session_id, SHELL_BUSY, SHELL_IDLE)
        {
            emit_shell_state(state, app, session_id, "idle");
        }

        Some(data.to_owned())
    }
}

/// Check if `data` contains an opening `[[intent:` or `[[suggest:` (or single-bracket
/// variants) without the corresponding closing bracket(s).  Used to detect tokens
/// that were split across streaming chunks so we can buffer them.
fn has_unclosed_token(data: &str) -> bool {
    // Find the LAST opener position for intent or suggest.
    // We only care about the last one — earlier complete tokens are fine.
    let last_intent = data.rfind("[intent:");
    let last_suggest = data.rfind("[suggest:");
    let opener_pos = match (last_intent, last_suggest) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };
    let Some(pos) = opener_pos else { return false };
    // Check if there's a closing `]` after the opener.
    // The token ends with `]]` or `]` or `⟧` (U+27E7).
    let after = &data[pos..];
    // Skip past the "intent:" or "suggest:" keyword to avoid matching the opener bracket
    let keyword_end = after.find(':').map(|i| i + 1).unwrap_or(0);
    let body = &after[keyword_end..];
    // Must contain at least one `]` to be considered closed
    !body.contains(']') && !body.contains('\u{27E7}')
}

/// Process kitty keyboard actions (push/pop/query) shared by both reader threads.
fn process_kitty_actions(
    kitty_actions: &[KittyAction],
    session_id: &str,
    state: &AppState,
    app: Option<&AppHandle>,
) {
    if kitty_actions.is_empty() {
        return;
    }
    let entry = state.kitty_states
        .entry(session_id.to_string())
        .or_insert_with(|| Mutex::new(KittyKeyboardState::new()));
    let mut ks = entry.lock();
    for action in kitty_actions {
        match action {
            KittyAction::Push(flags) => ks.push(*flags),
            KittyAction::Pop => ks.pop(),
            KittyAction::Query => {
                let flags = ks.current_flags();
                let response = format!("\x1b[?{}u", flags);
                if let Some(sess) = state.sessions.get(session_id) {
                    let mut sess = sess.lock();
                    let _ = sess.writer.write_all(response.as_bytes());
                    let _ = sess.writer.flush();
                }
            }
        }
    }
    let flags = ks.current_flags();
    drop(ks);
    if let Some(app) = app {
        let _ = app.emit(
            &format!("kitty-keyboard-{session_id}"),
            flags,
        );
    }
}

/// Flush remaining bytes at EOF and write to ring buffer + WebSocket.
/// Returns the flushed data (may be empty).
fn flush_eof(
    utf8_buf: &mut Utf8ReadBuffer,
    esc_buf: &mut EscapeAwareBuffer,
    session_id: &str,
    state: &AppState,
) -> String {
    let utf8_tail = utf8_buf.flush();
    let esc_remaining = if utf8_tail.is_empty() {
        esc_buf.flush()
    } else {
        let mut flushed = esc_buf.push(&utf8_tail);
        flushed.push_str(&esc_buf.flush());
        flushed
    };
    if !esc_remaining.is_empty() {
        if let Some(ring) = state.output_buffers.get(session_id) {
            ring.lock().write(esc_remaining.as_bytes());
        }
        if let Some(mut clients) = state.ws_clients.get_mut(session_id) {
            clients.retain(|tx| tx.send(esc_remaining.clone()).is_ok());
        }
    }
    esc_remaining
}

/// Clean up session state from all DashMaps after a reader thread exits.
fn cleanup_session(session_id: &str, state: &AppState) {
    if state.sessions.remove(session_id).is_some() {
        state.metrics.active_sessions.fetch_sub(1, Ordering::Relaxed);
    }
    state.output_buffers.remove(session_id);
    state.vt_log_buffers.remove(session_id);
    state.ws_clients.remove(session_id);
    state.kitty_states.remove(session_id);
    state.input_buffers.remove(session_id);
    state.silence_states.remove(session_id);
    state.shell_states.remove(session_id);
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
    state.shell_states.insert(session_id.clone(), std::sync::atomic::AtomicU8::new(SHELL_NULL));

    // Spawn silence-detection timer thread
    spawn_silence_timer(
        silence.clone(),
        running.clone(),
        session_id.clone(),
        state.clone(),
        Some(app.clone()),
    );

    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut utf8_buf = Utf8ReadBuffer::new();
        let mut esc_buf = EscapeAwareBuffer::new();
        let session_cwd: Option<String> = state
            .sessions
            .get(&session_id)
            .and_then(|s| s.lock().cwd.clone());
        let mut processor = ChunkProcessor::new(session_cwd);
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
                    let (kitty_clean, kitty_actions) = strip_kitty_sequences(&esc_data);
                    let data = kitty_clean;

                    process_kitty_actions(&kitty_actions, &session_id, &state, Some(&app));

                    if let Some(processed) = processor.process_chunk(&data, &silence, &session_id, &state, Some(&app)) {
                        // Colorize intent / conceal suggest tokens, buffering incomplete
                        // tokens across streaming chunks to prevent cosmetic flash.
                        if let Some(data) = processor.transform_xterm(processed) {
                            let _ = app.emit(
                                &format!("pty-output-{session_id}"),
                                PtyOutput {
                                    session_id: session_id.clone(),
                                    data,
                                },
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(session_id = %session_id, "PTY reader error: {e}");
                    break;
                }
            }
        }
        // Signal timer thread to stop
        running.store(false, Ordering::Relaxed);

        // Ensure shell state is idle on session end
        if try_shell_transition(&state, &session_id, SHELL_BUSY, SHELL_IDLE) {
            emit_shell_state(&state, Some(&app), &session_id, "idle");
        }

        // Flush any pending xterm token buffer (incomplete token at EOF — emit as-is)
        if let Some(pending) = processor.flush_pending_xterm() {
            let _ = app.emit(
                &format!("pty-output-{session_id}"),
                PtyOutput {
                    session_id: session_id.clone(),
                    data: pending,
                },
            );
        }

        // Flush remaining bytes at EOF
        let remaining = flush_eof(&mut utf8_buf, &mut esc_buf, &session_id, &state);
        if !remaining.is_empty() {
            let _ = app.emit(
                &format!("pty-output-{session_id}"),
                PtyOutput {
                    session_id: session_id.clone(),
                    data: remaining,
                },
            );
        }

        // Broadcast exit events
        let _ = state.event_bus.send(crate::state::AppEvent::PtyExit {
            session_id: session_id.clone(),
        });
        let _ = app.emit(
            &format!("pty-exit-{session_id}"),
            serde_json::json!({ "session_id": session_id }),
        );
        let _ = state.event_bus.send(crate::state::AppEvent::SessionClosed {
            session_id: session_id.clone(),
        });
        let _ = app.emit("session-closed", serde_json::json!({
            "session_id": session_id,
        }));

        cleanup_session(&session_id, &state);
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
    let silence = Arc::new(Mutex::new(SilenceState::new()));
    let running = Arc::new(AtomicBool::new(true));

    state.silence_states.insert(session_id.clone(), silence.clone());
    state.shell_states.insert(session_id.clone(), std::sync::atomic::AtomicU8::new(SHELL_NULL));

    // Spawn silence-detection timer (headless: event_bus only, no Tauri IPC)
    spawn_silence_timer(
        silence.clone(),
        running.clone(),
        session_id.clone(),
        state.clone(),
        None,
    );

    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut utf8_buf = Utf8ReadBuffer::new();
        let mut esc_buf = EscapeAwareBuffer::new();
        let session_cwd: Option<String> = state
            .sessions
            .get(&session_id)
            .and_then(|s| s.lock().cwd.clone());
        let mut processor = ChunkProcessor::new(session_cwd);
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
                    let (kitty_clean, kitty_actions) = strip_kitty_sequences(&esc_data);
                    let data = kitty_clean;

                    process_kitty_actions(&kitty_actions, &session_id, &state, None);

                    // Headless: no xterm output — just process for events and state
                    processor.process_chunk(&data, &silence, &session_id, &state, None);
                }
                Err(e) => {
                    tracing::error!(session_id = %session_id, "PTY reader error: {e}");
                    break;
                }
            }
        }
        // Signal silence timer thread to stop
        running.store(false, Ordering::Relaxed);

        // Ensure shell state is idle on session end
        if try_shell_transition(&state, &session_id, SHELL_BUSY, SHELL_IDLE) {
            emit_shell_state(&state, None, &session_id, "idle");
        }

        // Flush remaining bytes at EOF
        flush_eof(&mut utf8_buf, &mut esc_buf, &session_id, &state);

        // Broadcast exit so SSE/WebSocket consumers and Tauri frontend can clean up
        let _ = state.event_bus.send(crate::state::AppEvent::SessionClosed {
            session_id: session_id.clone(),
        });
        if let Some(app) = state.app_handle.read().as_ref() {
            let _ = app.emit("session-closed", serde_json::json!({
                "session_id": session_id,
            }));
        }

        cleanup_session(&session_id, &state);
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

        let mut cmd = build_shell_command(&shell);

        if let Some(ref cwd) = config.cwd {
            cmd.cwd(cwd);
        }

        // Inject stable session UUID so agents can use it for session binding
        // (e.g. `claude --session-id $TUIC_SESSION`, then `claude --resume $TUIC_SESSION`)
        if let Some(ref tuic_session) = config.tuic_session {
            cmd.env("TUIC_SESSION", tuic_session);
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
            display_name: None,
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
    state.last_output_ms.insert(session_id.clone(), std::sync::atomic::AtomicU64::new(0));

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

        let mut cmd = build_shell_command(&shell);
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
                tracing::warn!("Failed to cleanup worktree after PTY failure: {cleanup_err}");
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
            display_name: None,
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
    state.last_output_ms.insert(session_id.clone(), std::sync::atomic::AtomicU64::new(0));

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
pub(crate) async fn write_pty(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    session_id: String,
    data: String,
) -> Result<(), String> {
    let state = Arc::clone(&state);
    let app = app.clone();
    tokio::task::spawn_blocking(move || {
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
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

/// Get the last relevant user prompt (>= 10 words) for a PTY session.
#[tauri::command]
pub(crate) fn get_last_prompt(
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> Option<String> {
    state.last_prompts.get(&session_id).map(|v| v.clone())
}

/// Get the current shell state for a PTY session.
/// Used by the frontend on remount to sync state missed while unsubscribed.
/// Returns "busy", "idle", or null (session never produced output / removed).
#[tauri::command]
pub(crate) fn get_shell_state(
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> Option<String> {
    state.shell_states.get(&session_id).map(|atom| {
        match atom.load(std::sync::atomic::Ordering::Relaxed) {
            SHELL_BUSY => "busy".to_string(),
            SHELL_IDLE => "idle".to_string(),
            _ => "idle".to_string(), // null → treat as idle for frontend
        }
    })
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
        state.shell_states.remove(&session_id);
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
                tracing::warn!("Failed to cleanup worktree: {e}");
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

/// Set the display name of a PTY session (syncs tab title to backend for PWA visibility).
#[tauri::command]
pub(crate) fn set_session_name(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    name: Option<String>,
) -> Result<(), String> {
    let entry = state
        .sessions
        .get(&session_id)
        .ok_or_else(|| format!("Session not found: {session_id}"))?;
    entry.lock().display_name = name;
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
        s.on_chunk(false, Some("Continue?".to_string()), false, false);
        // Just set — not enough time has passed
        assert!(s.check_silence().is_none());
    }

    #[test]
    fn test_silence_state_pending_after_threshold() {
        let mut s = SilenceState::new();
        s.on_chunk(false, Some("Continue?".to_string()), false, false);
        // Simulate time passing by backdating last_output_at
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert_eq!(s.check_silence(), Some("Continue?".to_string()));
    }

    #[test]
    fn test_silence_state_no_double_emission() {
        let mut s = SilenceState::new();
        s.on_chunk(false, Some("Continue?".to_string()), false, false);
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert!(s.check_silence().is_some());
        // Second check should return None (already emitted)
        assert!(s.check_silence().is_none());
    }

    #[test]
    fn test_silence_state_regex_suppresses_timer() {
        let mut s = SilenceState::new();
        // regex_found_question = true means instant detection already fired
        s.on_chunk(true, Some("Would you like to proceed?".to_string()), false, false);
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert!(s.check_silence().is_none());
    }

    #[test]
    fn test_silence_state_regex_clears_prior_pending() {
        let mut s = SilenceState::new();
        // Silence detector has a pending question from an earlier chunk
        s.on_chunk(false, Some("Earlier question?".to_string()), false, false);
        assert!(s.pending_question_line.is_some());
        // Regex fires on a different event — no question line in this chunk
        s.on_chunk(true, None, false, false);
        assert!(s.pending_question_line.is_none(), "prior pending should be cleared when regex fires");
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert!(s.check_silence().is_none());
    }

    #[test]
    fn test_silence_state_non_question_output_preserves_pending() {
        let mut s = SilenceState::new();
        s.on_chunk(false, Some("Continue?".to_string()), false, false);
        // Non-`?` output (spinners, prompts, decorations) must NOT clear pending.
        s.on_chunk(false, None, false, false);
        s.on_chunk(false, None, false, false);
        s.on_chunk(false, None, false, false);
        // Standard 10s threshold fires normally
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert_eq!(s.check_silence(), Some("Continue?".to_string()));
    }

    #[test]
    fn test_silence_state_new_question_replaces_old() {
        let mut s = SilenceState::new();
        s.on_chunk(false, Some("First question?".to_string()), false, false);
        s.on_chunk(false, Some("Second question?".to_string()), false, false);
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert_eq!(s.check_silence(), Some("Second question?".to_string()));
    }

    #[test]
    fn test_silence_state_suppress_user_input() {
        let mut s = SilenceState::new();
        // User types a line ending with `?` — PTY will echo it back
        s.on_chunk(false, Some("c'è ancora una storia?".to_string()), false, false);
        // write_pty detects user input and suppresses
        s.suppress_user_input();
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        // Should NOT fire — the question was typed by the user
        assert!(s.check_silence().is_none());
    }

    #[test]
    fn test_silence_state_suppress_echo_after_user_input() {
        let mut s = SilenceState::new();
        // write_pty detects user input and suppresses BEFORE the echo arrives
        s.suppress_user_input();
        // PTY echoes the user's text back — this should NOT re-enable detection
        s.on_chunk(false, Some("lo hai mai provato?".to_string()), false, false);
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        // Should NOT fire — the echo window blocks re-enabling
        assert!(s.check_silence().is_none(), "PTY echo after suppress should not trigger question detection");
    }

    #[test]
    fn test_silence_state_suppress_echo_expires() {
        let mut s = SilenceState::new();
        s.suppress_user_input();
        // Expire the echo suppress window with a past deadline (not None,
        // which means "never suppressed" — a different code path).
        s.suppress_echo_until = Some(std::time::Instant::now() - std::time::Duration::from_millis(1));
        // Agent asks a genuine question after the window expires
        s.on_chunk(false, Some("Would you like to proceed?".to_string()), false, false);
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        // Should fire — this is a real agent question
        assert_eq!(s.check_silence(), Some("Would you like to proceed?".to_string()));
    }

    #[test]
    fn test_silence_state_spinner_suppresses_question() {
        let mut s = SilenceState::new();
        // Agent prints a `?`-line alongside a status-line/spinner in the same chunk
        s.on_chunk(false, Some("Want me to proceed?".to_string()), true, false);
        // Simulate 10s+ of silence
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        // Should NOT emit question — spinner was recently active
        assert_eq!(s.check_silence(), None, "spinner active → no question");
    }

    #[test]
    fn test_silence_state_spinner_expired_allows_question() {
        let mut s = SilenceState::new();
        s.on_chunk(false, Some("Want me to proceed?".to_string()), true, false);
        // Spinner was active but long ago (>10s, matching SILENCE_QUESTION_THRESHOLD)
        s.last_status_line_at = Some(std::time::Instant::now() - std::time::Duration::from_secs(12));
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        // Spinner expired, question should fire
        assert_eq!(s.check_silence(), Some("Want me to proceed?".to_string()));
    }

    #[test]
    fn test_silence_state_spinner_within_10s_suppresses() {
        let mut s = SilenceState::new();
        s.on_chunk(false, Some("Want me to proceed?".to_string()), true, false);
        // Spinner was 8s ago — still within the 10s window
        s.last_status_line_at = Some(std::time::Instant::now() - std::time::Duration::from_secs(8));
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert_eq!(s.check_silence(), None, "spinner within 10s should suppress question");
    }

    // --- Status-line-only chunk tests ---

    #[test]
    fn test_silence_state_status_line_only_does_not_reset_silence() {
        let mut s = SilenceState::new();
        s.on_chunk(false, Some("Continue?".to_string()), false, false);
        // Backdate last_output_at to simulate 10s of silence
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        // Mode-line timer tick: status_line_only = true, should NOT reset last_output_at
        s.on_chunk(false, None, true, true);
        // The silence threshold should still be met
        assert_eq!(s.check_silence(), Some("Continue?".to_string()),
            "status_line_only chunks must not reset the silence timer");
    }

    #[test]
    fn test_silence_state_mode_line_ticks_do_not_suppress_question() {
        // Reproduces the bug: Claude Code asks a question, then the mode line
        // keeps updating every ~1s while waiting for input. Status-line-only chunks
        // were keeping `is_spinner_active()` true forever, preventing question
        // detection even after 10s of silence.
        let mut s = SilenceState::new();
        // Agent outputs question + status line in same chunk (not status-line-only)
        s.on_chunk(false, Some("Vuoi fare un commit?".to_string()), true, false);

        // Simulate 10s+ passing: both last_output_at and last_status_line_at
        // age beyond the threshold (in real life, wall-clock time handles this).
        let past = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        s.last_output_at = past;
        s.last_status_line_at = Some(past);

        // Mode-line-only ticks keep coming — they must NOT refresh either timer.
        for _ in 0..10 {
            s.on_chunk(false, None, true, true);
        }

        // After 10s+ of silence, the question MUST be detected even though
        // mode-line ticks kept coming in.
        assert_eq!(s.check_silence(), Some("Vuoi fare un commit?".to_string()),
            "mode-line-only ticks must not keep is_spinner_active() alive");
    }

    #[test]
    fn test_silence_state_mode_line_ticks_do_not_stale_question() {
        // Regression: mode-line timer ticks (status_line_only=true) were incrementing
        // output_chunks_after_question, clearing the pending question as "stale"
        // before the silence timer could detect it.
        let mut s = SilenceState::new();
        s.on_chunk(false, Some("Procedo?".to_string()), true, false);

        // Simulate 15 mode-line ticks (> STALE_QUESTION_CHUNKS=10)
        for _ in 0..15 {
            s.on_chunk(false, None, true, true);
        }

        // pending_question_line must still be present — mode-line ticks are not real output
        assert_eq!(s.pending_question_line.as_deref(), Some("Procedo?"),
            "mode-line-only ticks must not count toward staleness");

        // Backdate to simulate silence threshold reached
        let past = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        s.last_output_at = past;
        s.last_status_line_at = Some(past);

        assert_eq!(s.check_silence(), Some("Procedo?".to_string()),
            "question must be detectable after mode-line-only ticks");
    }

    #[test]
    fn test_silence_state_regular_chunk_resets_silence() {
        let mut s = SilenceState::new();
        s.on_chunk(false, Some("Continue?".to_string()), false, false);
        // Backdate to simulate 10s silence
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        // Regular (non-status-line) chunk resets the timer
        s.on_chunk(false, None, false, false);
        // Now we need to wait another 10s — should NOT fire yet
        assert_eq!(s.check_silence(), None,
            "regular chunk should reset silence timer");
    }

    // --- is_chrome_row / chrome_only classification tests ---

    #[test]
    fn test_chrome_only_empty_changed_rows_is_not_chrome() {
        let rows: Vec<ChangedRow> = vec![];
        let chrome_only = !rows.is_empty() && rows.iter().all(|r| is_chrome_row(&r.text));
        assert!(!chrome_only, "empty changed_rows should not be chrome");
    }

    #[test]
    fn test_chrome_only_plain_text_is_not_chrome() {
        let rows = make_rows(&["I will edit the file for you."]);
        let chrome_only = !rows.is_empty() && rows.iter().all(|r| is_chrome_row(&r.text));
        assert!(!chrome_only, "plain text without chrome markers is not chrome");
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

    // --- Staleness counter tests ---

    #[test]
    fn test_silence_state_stale_after_many_output_chunks() {
        let mut s = SilenceState::new();
        s.on_chunk(false, Some("Continue?".to_string()), false, false);
        // Simulate 15 non-`?` chunks (well beyond STALE_QUESTION_CHUNKS)
        for _ in 0..15 {
            s.on_chunk(false, None, false, false);
        }
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert_eq!(s.check_silence(), None, "stale question after many chunks should not fire");
    }

    #[test]
    fn test_silence_state_few_decoration_chunks_still_fires() {
        let mut s = SilenceState::new();
        s.on_chunk(false, Some("Continue?".to_string()), false, false);
        // 3 decoration chunks (mode line, separator, prompt) — within threshold
        s.on_chunk(false, None, false, false);
        s.on_chunk(false, None, false, false);
        s.on_chunk(false, None, false, false);
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert_eq!(s.check_silence(), Some("Continue?".to_string()), "few decoration chunks should still fire");
    }

    #[test]
    fn test_silence_state_counter_resets_on_new_question() {
        let mut s = SilenceState::new();
        s.on_chunk(false, Some("First?".to_string()), false, false);
        // Many non-`?` chunks → stale
        for _ in 0..15 {
            s.on_chunk(false, None, false, false);
        }
        // New `?` line resets the counter
        s.on_chunk(false, Some("Second?".to_string()), false, false);
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert_eq!(s.check_silence(), Some("Second?".to_string()), "new question should reset staleness");
    }

    // --- Screen verification tests ---

    #[test]
    fn test_verify_question_on_screen_found() {
        let screen = vec![
            "".to_string(),
            "Some output".to_string(),
            "Do you want to proceed?".to_string(),
            "⏵⏵ task_name".to_string(),
            "".to_string(),
        ];
        assert!(verify_question_on_screen(&screen, "Do you want to proceed?", 5));
    }

    #[test]
    fn test_verify_question_on_screen_ink_indented() {
        // Ink agents indent text with leading whitespace. extract_question_line
        // captures "  Want me to do that?" (with spaces), screen_rows also has
        // the same. Verification must match despite leading whitespace.
        let screen = vec![
            "⏺ Boss, this is a plan file".to_string(),
            "  Is that right?".to_string(),
            "".to_string(),
            "  Want me to do that?".to_string(),
            "".to_string(),
        ];
        // Question stored with leading whitespace from extract_question_line
        assert!(verify_question_on_screen(&screen, "  Want me to do that?", 5));
        // Also works if question was stored without whitespace
        assert!(verify_question_on_screen(&screen, "Want me to do that?", 5));
    }

    #[test]
    fn test_verify_question_on_screen_scrolled_away() {
        // Question is NOT among the last 5 rows
        let screen: Vec<String> = (0..24).map(|i| format!("line {i}")).collect();
        assert!(!verify_question_on_screen(&screen, "Do you want to proceed?", 5));
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
            "".to_string(),
        ];
        // The stored question is just "question?" — substring should not match
        assert!(!verify_question_on_screen(&screen, "question?", 5));
    }

    // --- Clear stale question tests ---

    #[test]
    fn test_silence_state_clear_stale_resets_pending() {
        let mut s = SilenceState::new();
        s.on_chunk(false, Some("Continue?".to_string()), false, false);
        s.clear_stale_question();
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert_eq!(s.check_silence(), None, "cleared stale should not fire");
    }

    #[test]
    fn test_silence_state_clear_stale_allows_new_question() {
        let mut s = SilenceState::new();
        s.on_chunk(false, Some("Old?".to_string()), false, false);
        s.clear_stale_question();
        // New question after clear
        s.on_chunk(false, Some("New?".to_string()), false, false);
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert_eq!(s.check_silence(), Some("New?".to_string()), "new question after clear should fire");
    }

    #[test]
    fn test_silence_state_repaint_same_question_does_not_refire() {
        let mut s = SilenceState::new();
        // Question arrives, silence fires, mark emitted
        s.on_chunk(false, Some("Continue?".to_string()), false, false);
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert!(s.check_silence().is_some());
        assert!(s.question_already_emitted);

        // Terminal repaint: same `?` line re-appears as a changed row.
        // This must NOT reset question_already_emitted.
        s.on_chunk(false, Some("Continue?".to_string()), false, false);
        assert!(s.question_already_emitted, "repaint of same question must not reset emitted flag");
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert!(s.check_silence().is_none(), "same question repaint must not re-fire");
    }

    #[test]
    fn test_silence_state_stale_same_question_scroll_does_not_refire() {
        let mut s = SilenceState::new();
        let past = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        // Question fires via chunk-based detection (Strategy 2)
        s.on_chunk(false, Some("Continue?".to_string()), false, false);
        s.last_output_at = past;
        assert!(s.check_silence().is_some());

        // Agent resumes: 15 non-`?` chunks (above STALE_QUESTION_CHUNKS)
        for _ in 0..15 {
            s.on_chunk(false, None, false, false);
        }
        assert!(s.pending_question_line.is_none(), "pending should be cleared by staleness");

        // Same "Continue?" reappears in changed_rows because new output scrolled it
        // to a different row. This is NOT a new question — must not re-fire.
        s.on_chunk(false, Some("Continue?".to_string()), false, false);
        assert!(s.question_already_emitted,
            "scroll of previously emitted question must not reset emitted flag");
        s.last_output_at = past;
        assert!(s.check_silence().is_none(),
            "same question text from scroll must not re-fire");
    }

    #[test]
    fn test_silence_state_stale_same_question_refires_after_user_input() {
        let mut s = SilenceState::new();
        let past = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        // Question fires
        s.on_chunk(false, Some("Continue?".to_string()), false, false);
        s.last_output_at = past;
        assert!(s.check_silence().is_some());

        // Agent resumes: 15 non-`?` chunks
        for _ in 0..15 {
            s.on_chunk(false, None, false, false);
        }

        // User provides input → new conversation cycle
        s.suppress_user_input();
        // Expire the echo suppression window so the next `?` line is not ignored
        s.suppress_echo_until = Some(std::time::Instant::now() - std::time::Duration::from_millis(1));

        // Same question arrives again — now it IS a new question (user answered)
        s.on_chunk(false, Some("Continue?".to_string()), false, false);
        s.last_output_at = past;
        assert_eq!(s.check_silence(), Some("Continue?".to_string()),
            "same question text after user input must fire as new question");
    }

    #[test]
    fn test_silence_state_screen_emitted_question_scroll_does_not_refire() {
        let mut s = SilenceState::new();
        let past = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);

        // Question arrives in a chunk
        s.on_chunk(false, Some("Continue?".to_string()), false, false);

        // 15 non-? chunks → pending cleared by staleness
        for _ in 0..15 {
            s.on_chunk(false, None, false, false);
        }
        assert!(s.pending_question_line.is_none());

        // Silence timer (Strategy 1) finds "Continue?" on screen and emits.
        s.last_output_at = past;
        s.mark_emitted("Continue?");

        // New output causes scroll → same "Continue?" appears in changed_rows
        s.on_chunk(false, Some("Continue?".to_string()), false, false);

        // Must NOT reset question_already_emitted — it's a scroll artifact
        assert!(s.question_already_emitted,
            "scroll of screen-emitted question must not reset emitted flag");
        s.last_output_at = past;
        assert!(!s.is_silent(),
            "same question after screen emission must not allow re-detection");
    }

    #[test]
    fn test_silence_state_different_question_after_emitted_does_fire() {
        let mut s = SilenceState::new();
        // First question fires
        s.on_chunk(false, Some("Continue?".to_string()), false, false);
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert!(s.check_silence().is_some());

        // Different question arrives — this IS a new question, must fire
        s.on_chunk(false, Some("Are you sure?".to_string()), false, false);
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert_eq!(s.check_silence(), Some("Are you sure?".to_string()));
    }

    // --- extract_last_chat_line tests ---

    fn screen(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_extract_last_chat_line_standard_claude_code() {
        // Standard Claude Code layout: question, empty, separator, prompt, separator, mode line
        let rows = screen(&[
            "Some earlier output",
            "Do you want to proceed?",
            "",
            "────────────────────────────────",
            "> yes please",
            "────────────────────────────────",
            "⏵⏵ bypass permissions on (shift+tab to cycle)",
        ]);
        assert_eq!(
            extract_last_chat_line(&rows),
            Some("Do you want to proceed?".to_string()),
        );
    }

    #[test]
    fn test_extract_last_chat_line_no_empty_line_above_separator() {
        // No empty line between content and upper separator
        let rows = screen(&[
            "template minimale o anche CI (lint manifest, validate structure)?",
            "────────────────────────────────",
            "> repo separato",
            "────────────────────────────────",
            "⏵⏵ accept edits on (shift+tab to cycle)",
        ]);
        assert_eq!(
            extract_last_chat_line(&rows),
            Some("template minimale o anche CI (lint manifest, validate structure)?".to_string()),
        );
    }

    #[test]
    fn test_extract_last_chat_line_with_wiz_hud() {
        // Wiz HUD adds extra lines below the lower separator
        let rows = screen(&[
            "Quale delle due hai in mente?",
            "",
            "────────────────────────────────",
            "> something",
            "────────────────────────────────",
            "[Opus 4.6 | Team] 54% | wiz-agents git:(main)",
            "5h: 42% (3h) | 7d: 27% (2d)",
            "✓ Edit ×7 | ✓ Bash ×5",
            "⏵⏵ bypass permissions on (shift+tab to cycle)",
        ]);
        assert_eq!(
            extract_last_chat_line(&rows),
            Some("Quale delle due hai in mente?".to_string()),
        );
    }

    #[test]
    fn test_extract_last_chat_line_empty_prompt() {
        // User hasn't typed anything yet — prompt box is just `>`
        let rows = screen(&[
            "What should I do next?",
            "",
            "────────────────────────────────",
            ">",
            "────────────────────────────────",
            "⏵⏵ plan mode on (shift+tab to cycle)",
        ]);
        assert_eq!(
            extract_last_chat_line(&rows),
            Some("What should I do next?".to_string()),
        );
    }

    #[test]
    fn test_extract_last_chat_line_no_prompt_line() {
        // No prompt line at all
        let rows = screen(&[
            "Just some output",
            "More output",
            "",
        ]);
        assert_eq!(extract_last_chat_line(&rows), None);
    }

    #[test]
    fn test_extract_last_chat_line_empty_screen() {
        let rows: Vec<String> = vec![];
        assert_eq!(extract_last_chat_line(&rows), None);
    }

    #[test]
    fn test_extract_last_chat_line_interrupted_separator() {
        // Separator with label in the middle (e.g. model indicator)
        let rows = screen(&[
            "Vuoi fare un commit?",
            "",
            "──────── ■■■ Medium /model ────────",
            "> si",
            "──────── ■■■ Medium /model ────────",
            "⏵⏵ bypass permissions on",
        ]);
        assert_eq!(
            extract_last_chat_line(&rows),
            Some("Vuoi fare un commit?".to_string()),
        );
    }

    #[test]
    fn test_extract_last_chat_line_plan_mode() {
        // Plan mode has a different mode line prefix
        let rows = screen(&[
            "Should I implement this approach?",
            "",
            "────────────────────────────────",
            "> ",
            "────────────────────────────────",
            "⏸ plan mode on (shift+tab to cycle)",
        ]);
        assert_eq!(
            extract_last_chat_line(&rows),
            Some("Should I implement this approach?".to_string()),
        );
    }

    #[test]
    fn test_extract_last_chat_line_multiple_empty_lines() {
        // Multiple empty lines between content and separator
        let rows = screen(&[
            "Continue with the refactor?",
            "",
            "",
            "",
            "────────────────────────────────",
            "> ok",
            "────────────────────────────────",
            "",
        ]);
        assert_eq!(
            extract_last_chat_line(&rows),
            Some("Continue with the refactor?".to_string()),
        );
    }

    #[test]
    fn test_extract_last_chat_line_codex() {
        // Codex layout: no separator lines, prompt is `›`
        let rows = screen(&[
            "⚠ MCP startup incomplete (failed: serena)",
            "",
            "Do you want me to proceed?",
            "",
            "› Implement {feature}",
            "",
            "  gpt-5.3-codex high · 100% left · ~/Gits/personal/tuicommander",
        ]);
        assert_eq!(
            extract_last_chat_line(&rows),
            Some("Do you want me to proceed?".to_string()),
        );
    }

    #[test]
    fn test_extract_last_chat_line_codex_no_question() {
        // Codex with no question — last chat line doesn't end with ?
        let rows = screen(&[
            "I'll implement the feature now.",
            "",
            "› ",
            "",
            "  gpt-5.3-codex high · 100% left · ~/project",
        ]);
        assert_eq!(
            extract_last_chat_line(&rows),
            Some("I'll implement the feature now.".to_string()),
        );
    }

    #[test]
    fn test_extract_last_chat_line_gemini() {
        // Gemini layout: `> ` prompt
        let rows = screen(&[
            "Should I refactor the module?",
            "",
            "> yes",
            "⠋ Processing...",
        ]);
        assert_eq!(
            extract_last_chat_line(&rows),
            Some("Should I refactor the module?".to_string()),
        );
    }

    #[test]
    fn test_extract_last_chat_line_prompt_with_separator_above() {
        // Prompt with only one separator above (no lower separator)
        let rows = screen(&[
            "Want to continue?",
            "────────────────────────────────",
            "> ok",
            "⏵⏵ mode line",
        ]);
        assert_eq!(
            extract_last_chat_line(&rows),
            Some("Want to continue?".to_string()),
        );
    }

    // --- extract_question_line content filter tests ---

    fn make_rows(texts: &[&str]) -> Vec<ChangedRow> {
        texts.iter().enumerate().map(|(i, t)| ChangedRow { row_index: i, text: t.to_string() }).collect()
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
    fn test_extract_question_line_rejects_code_syntax() {
        assert_eq!(extract_question_line(&make_rows(&["fn foo() -> Option<bool>?"])), None);
        assert_eq!(extract_question_line(&make_rows(&["map.entry(key)?"])), None);
        assert_eq!(extract_question_line(&make_rows(&["let x = a::b?"])), None);
    }

    #[test]
    fn test_extract_question_line_accepts_real_question() {
        let rows = make_rows(&["Do you want to proceed?"]);
        assert_eq!(extract_question_line(&rows), Some("Do you want to proceed?".to_string()));
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
        assert_eq!(extract_question_line(&rows), Some("Continue (yes/no)?".to_string()));
    }

    #[test]
    fn test_extract_question_line_accepts_yn_parens() {
        let rows = make_rows(&["Procedo (s/n)?"]);
        assert_eq!(extract_question_line(&rows), Some("Procedo (s/n)?".to_string()));
    }

    #[test]
    fn test_extract_question_line_accepts_option_prompt() {
        let rows = make_rows(&["Apply changes (y)?"]);
        assert_eq!(extract_question_line(&rows), Some("Apply changes (y)?".to_string()));
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
        assert_eq!(extract_question_line(&make_rows(&["iter().map(|x| x)?"])), None);
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

    /// extract_question_line finds `?`-ending rows from VtLogBuffer output.
    #[test]
    fn test_extract_question_line_basic() {
        use crate::state::VtLogBuffer;

        let mut vt_log = VtLogBuffer::new(24, 80, 1000);
        let changed = vt_log.process(b"Would you like to proceed?");
        assert_eq!(extract_question_line(&changed).as_deref(), Some("Would you like to proceed?"));
    }

    /// Question row must be found even when a mode line with a higher row index
    /// arrives in the same chunk (e.g. Claude Code question + ⏵⏵ status line).
    #[test]
    fn test_extract_question_line_with_mode_line_same_chunk() {
        use crate::state::VtLogBuffer;

        let mut vt_log = VtLogBuffer::new(24, 80, 1000);
        let data = b"Le committo?\r\n\r\n\xe2\x8f\xb5\xe2\x8f\xb5 Reading files";
        let changed = vt_log.process(data);
        assert_eq!(extract_question_line(&changed).as_deref(), Some("Le committo?"),
            "question must be found even when mode line is on a later row; changed_rows: {:?}",
            changed.iter().map(|r| format!("[{}] {:?}", r.row_index, &r.text)).collect::<Vec<_>>()
        );
    }

    /// Question must be found in alternate screen with cursor-positioned rows.
    #[test]
    fn test_extract_question_line_alternate_screen() {
        use crate::state::VtLogBuffer;

        let mut vt_log = VtLogBuffer::new(24, 80, 1000);
        vt_log.process(b"\x1b[?1049h");
        let data = b"\x1b[5;1HDo you want to proceed?\x1b[23;1H* Thinking...";
        let changed = vt_log.process(data);
        assert_eq!(extract_question_line(&changed).as_deref(), Some("Do you want to proceed?"),
            "question must be found in alternate screen; changed_rows: {:?}",
            changed.iter().map(|r| format!("[{}] {:?}", r.row_index, &r.text)).collect::<Vec<_>>()
        );
    }

    /// End-to-end: VtLogBuffer → extract_question_line → SilenceState → check_silence.
    /// Question + mode line arrive together → fires at 10s.
    #[test]
    fn test_e2e_question_detection_with_mode_line() {
        use crate::state::VtLogBuffer;

        let mut vt_log = VtLogBuffer::new(24, 80, 1000);
        let mut silence = SilenceState::new();

        let changed = vt_log.process(b"Le committo?\r\n\r\n\xe2\x8f\xb5\xe2\x8f\xb5 Reading files");
        silence.on_chunk(false, extract_question_line(&changed), false, false);

        assert_eq!(silence.pending_question_line.as_deref(), Some("Le committo?"));

        silence.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
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
        silence.on_chunk(false, extract_question_line(&changed), false, false);

        // Mode line / prompt decoration arrives in a separate chunk
        let changed = vt_log.process(b"\r\n\xe2\x8f\xb5\xe2\x8f\xb5 Idle");
        silence.on_chunk(false, extract_question_line(&changed), false, false);

        // 10s silence → fires
        silence.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert_eq!(silence.check_silence(), Some("Le committo?".to_string()));
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

    // --- vt100 escape sequence handling diagnostics ---

    /// Verify that `\x1b[<n>F` (CPL — Cursor Previous Line) is handled by vt100
    /// and does NOT leak parameter digits into screen cell text.
    #[test]
    fn test_vt100_cpl_sequence_does_not_leak() {
        let mut parser = vt100::Parser::new(24, 80, 0);
        // Write on row 2, then CPL to go back to row 1 and overwrite
        parser.process(b"\n");                     // move to row 1
        parser.process(b"old content here\n");     // row 1 has text, cursor at row 2
        parser.process(b"\x1b[1F");                // CPL: go up 1 line, col 0
        parser.process(b"new content");            // overwrite row 1
        let screen = parser.screen();
        let row1: String = screen.rows(0, 80).nth(1).unwrap().trim_end().to_string();
        assert_eq!(row1, "new content here",
            "CPL should move cursor up; row1 = {:?}", row1);
        // The critical check: no "1F" should appear anywhere
        assert!(!row1.contains("1F"),
            "escape param '1F' leaked into screen text: {:?}", row1);
    }

    /// Verify that `\x1b[<n>E` (CNL — Cursor Next Line) is handled.
    #[test]
    fn test_vt100_cnl_sequence_does_not_leak() {
        let mut parser = vt100::Parser::new(24, 80, 0);
        parser.process(b"line0");
        parser.process(b"\x1b[1E");  // CNL: go down 1 line, col 0
        parser.process(b"line1");
        let screen = parser.screen();
        let row0: String = screen.rows(0, 80).next().unwrap().trim_end().to_string();
        let row1: String = screen.rows(0, 80).nth(1).unwrap().trim_end().to_string();
        assert_eq!(row0, "line0", "row0 should be unchanged; got {:?}", row0);
        assert_eq!(row1, "line1", "CNL should move cursor down; got {:?}", row1);
    }

    /// Simulate Ink-style rendering: write intent, then use CPL to update it.
    /// This is what Claude Code does when updating its status line.
    #[test]
    fn test_vt100_ink_style_intent_with_cpl() {
        use crate::state::VtLogBuffer;
        use crate::output_parser::{OutputParser, ParsedEvent};

        let mut vt_log = VtLogBuffer::new(24, 80, 1000);
        let mut parser = OutputParser::new();

        // Simulate Ink render: write placeholder, then CPL + overwrite with intent
        vt_log.process(b"\x1b[?1049h");  // alternate screen
        vt_log.process(b"placeholder text\r\n");
        // Ink update: go up, clear line, write intent
        let changed = vt_log.process(
            b"\x1b[1F\x1b[2K[[intent: Fix all 34 documentation gaps(Fixing gaps)]]"
        );
        let events = parser.parse_clean_lines(&changed);
        let intent = events.iter().find_map(|e| match e {
            ParsedEvent::Intent { text, title } => Some((text.clone(), title.clone())),
            _ => None,
        });
        assert!(intent.is_some(),
            "intent must be detected after CPL overwrite; changed={:?}, events={:?}",
            changed, events);
        let (text, title) = intent.unwrap();
        assert_eq!(text, "Fix all 34 documentation gaps",
            "intent text must be clean (no '1F' leak); got: {:?}", text);
        assert_eq!(title.as_deref(), Some("Fixing gaps"));
    }

    /// Chunked delivery: CSI split across two process() calls.
    /// Verifies the vt100 parser buffers incomplete escapes correctly.
    #[test]
    fn test_vt100_chunked_csi_does_not_leak() {
        use crate::state::VtLogBuffer;

        let mut vt_log = VtLogBuffer::new(24, 80, 1000);
        vt_log.process(b"\x1b[?1049h");  // alternate screen
        vt_log.process(b"old line\r\n");

        // Chunk 1: partial CSI (just the introducer)
        let changed1 = vt_log.process(b"\x1b[");
        // Chunk 2: parameter + final byte completing CPL, then text
        let changed2 = vt_log.process(b"1F[[intent: Fix all gaps]]");

        // Check that no row contains literal "1F" as text
        for row in changed1.iter().chain(changed2.iter()) {
            assert!(!row.text.contains("1F"),
                "chunked CSI leaked '1F' into row text: {:?}", row.text);
        }
    }

    /// Test what happens when CSI is aborted by an unexpected byte.
    /// Some terminal emulators discard the sequence; others print the chars.
    #[test]
    fn test_vt100_aborted_csi_does_not_leak_digits() {
        // Scenario: \x1b[1 followed by a non-CSI byte (e.g. ESC starting
        // a new sequence). The pending "1" should NOT become visible text.
        let mut parser = vt100::Parser::new(24, 80, 0);
        // \x1b[1\x1b[2K — the first CSI is aborted by the second ESC
        parser.process(b"\x1b[1\x1b[2KHello");
        let screen = parser.screen();
        let row: String = screen.rows(0, 80).next().unwrap().trim_end().to_string();
        eprintln!("aborted CSI row: {:?}", row);
        assert!(!row.starts_with('1'),
            "aborted CSI parameter '1' should not appear in cell text: {:?}", row);
    }

    /// Test intermediate characters in CSI (e.g. \x1b[?25l has '?' intermediate).
    /// If vt100 doesn't recognize a private-mode sequence, does it leak?
    #[test]
    fn test_vt100_unknown_private_csi_does_not_leak() {
        let mut parser = vt100::Parser::new(24, 80, 0);
        // \x1b[?1234z — fictional private sequence with unknown final byte 'z'
        parser.process(b"\x1b[?1234zVisible text");
        let screen = parser.screen();
        let row: String = screen.rows(0, 80).next().unwrap().trim_end().to_string();
        eprintln!("unknown private CSI row: {:?}", row);
        assert_eq!(row, "Visible text",
            "unknown private CSI should not leak; got: {:?}", row);
    }

    /// Simulate realistic Ink output with SGR + cursor movement + text.
    /// This mimics what Claude Code actually sends through the PTY.
    #[test]
    fn test_vt100_realistic_ink_render_cycle() {
        use crate::state::VtLogBuffer;
        use crate::output_parser::{OutputParser, ParsedEvent};

        let mut vt_log = VtLogBuffer::new(24, 80, 1000);
        let mut parser = OutputParser::new();

        vt_log.process(b"\x1b[?1049h");  // alternate screen

        // Frame 1: Ink renders initial content with colors
        vt_log.process(
            b"\x1b[1;1H\x1b[38;2;128;128;128m\xe2\x97\x8f\x1b[0m \x1b[1m[[intent: Reading codebase structure(Reading code)]]\x1b[0m"
        );

        // Frame 2: Ink updates — cursor up, erase line, rewrite
        // This is how Ink typically does incremental updates
        let changed = vt_log.process(
            b"\x1b[1F\x1b[2K\x1b[38;2;128;128;128m\xe2\x97\x8f\x1b[0m \x1b[1m[[intent: Fix all 34 documentation gaps(Fixing gaps)]]\x1b[0m"
        );

        let events = parser.parse_clean_lines(&changed);
        let intent = events.iter().find_map(|e| match e {
            ParsedEvent::Intent { text, title } => Some((text.clone(), title.clone())),
            _ => None,
        });

        // Print all changed rows for debugging
        eprintln!("changed rows:");
        for r in &changed {
            eprintln!("  row[{}]: {:?}", r.row_index, r.text);
        }
        eprintln!("events: {:?}", events);

        assert!(intent.is_some(),
            "intent must be detected in realistic Ink render; events={:?}", events);
        let (text, title) = intent.unwrap();
        assert!(!text.contains("1F"),
            "intent text must not contain escape leak '1F'; got: {:?}", text);
        assert_eq!(text, "Fix all 34 documentation gaps");
        assert_eq!(title.as_deref(), Some("Fixing gaps"));
    }

    /// Multi-chunk Ink render: data arrives in small fragments.
    #[test]
    fn test_vt100_fragmented_ink_output() {
        use crate::state::VtLogBuffer;

        let mut vt_log = VtLogBuffer::new(24, 80, 1000);
        vt_log.process(b"\x1b[?1049h");

        // Simulate fragmented delivery of: \x1b[1F\x1b[2K[[intent: Fix]]
        let fragments: Vec<&[u8]> = vec![
            b"\x1b[",      // CSI introducer
            b"1",           // parameter
            b"F",           // final byte (CPL)
            b"\x1b[",      // CSI introducer
            b"2K",          // erase line
            b"[[intent: Fix all gaps]]",
        ];

        let mut all_changed = Vec::new();
        for frag in fragments {
            let changed = vt_log.process(frag);
            all_changed.extend(changed);
        }

        // Check no row contains '1F' leak
        for row in &all_changed {
            eprintln!("fragmented row[{}]: {:?}", row.row_index, row.text);
            assert!(!row.text.contains("1F"),
                "fragmented delivery leaked '1F': {:?}", row.text);
        }
    }

    // --- Shell state transition tests ---

    #[test]
    fn test_shell_state_busy_on_real_output() {
        use std::sync::atomic::{AtomicU8, Ordering};
        let state = crate::state::tests_support::make_test_app_state();
        let sid = "test-session";
        state.shell_states.insert(sid.to_string(), AtomicU8::new(SHELL_NULL));
        state.last_output_ms.insert(sid.to_string(), std::sync::atomic::AtomicU64::new(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap().as_millis() as u64
        ));

        // Transition null → busy
        assert!(try_shell_transition(&state, sid, SHELL_NULL, SHELL_BUSY),
            "should transition null → busy");
        assert_eq!(state.shell_states.get(sid).unwrap().load(Ordering::Relaxed), SHELL_BUSY);

        // Transition busy → busy should fail (already busy, no re-emit)
        assert!(!try_shell_transition(&state, sid, SHELL_NULL, SHELL_BUSY),
            "should NOT re-transition to busy");
    }

    #[test]
    fn test_shell_state_idle_after_500ms() {
        use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
        let state = crate::state::tests_support::make_test_app_state();
        let sid = "test-session";
        state.shell_states.insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));
        state.session_states.insert(sid.to_string(), crate::state::SessionState::default());

        // Set last output to 600ms ago (> SHELL_IDLE_MS)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap().as_millis() as u64;
        state.last_output_ms.insert(sid.to_string(), AtomicU64::new(now - 600));

        assert!(should_transition_idle(&state, sid),
            "should be ready to transition idle (600ms elapsed, no sub-tasks)");
        assert!(try_shell_transition(&state, sid, SHELL_BUSY, SHELL_IDLE),
            "should transition busy → idle");
        assert_eq!(state.shell_states.get(sid).unwrap().load(Ordering::Relaxed), SHELL_IDLE);
    }

    #[test]
    fn test_shell_state_no_idle_with_subtasks() {
        use std::sync::atomic::{AtomicU8, AtomicU64};
        let state = crate::state::tests_support::make_test_app_state();
        let sid = "test-session";
        state.shell_states.insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));

        state.session_states.insert(sid.to_string(), crate::state::SessionState {
            active_sub_tasks: 2,
            ..Default::default()
        });

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap().as_millis() as u64;
        state.last_output_ms.insert(sid.to_string(), AtomicU64::new(now - 600));

        assert!(!should_transition_idle(&state, sid),
            "should NOT transition idle when active_sub_tasks > 0");
    }

    #[test]
    fn test_shell_state_no_idle_before_500ms() {
        use std::sync::atomic::{AtomicU8, AtomicU64};
        let state = crate::state::tests_support::make_test_app_state();
        let sid = "test-session";
        state.shell_states.insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));
        state.session_states.insert(sid.to_string(), crate::state::SessionState::default());

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap().as_millis() as u64;
        state.last_output_ms.insert(sid.to_string(), AtomicU64::new(now - 200));

        assert!(!should_transition_idle(&state, sid),
            "should NOT transition idle when only 200ms elapsed");
    }

    #[test]
    fn test_shell_state_cas_prevents_duplicate_idle() {
        use std::sync::atomic::{AtomicU8, Ordering};
        let state = crate::state::tests_support::make_test_app_state();
        let sid = "test-session";
        state.shell_states.insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));

        // First CAS succeeds
        assert!(try_shell_transition(&state, sid, SHELL_BUSY, SHELL_IDLE));
        // Second CAS fails (already idle)
        assert!(!try_shell_transition(&state, sid, SHELL_BUSY, SHELL_IDLE),
            "second idle transition must fail — already idle");
        assert_eq!(state.shell_states.get(sid).unwrap().load(Ordering::Relaxed), SHELL_IDLE);
    }

    #[test]
    fn test_shell_state_idle_to_busy_on_real_output() {
        use std::sync::atomic::{AtomicU8, Ordering};
        let state = crate::state::tests_support::make_test_app_state();
        let sid = "test-session";
        state.shell_states.insert(sid.to_string(), AtomicU8::new(SHELL_IDLE));

        assert!(try_shell_transition(&state, sid, SHELL_IDLE, SHELL_BUSY),
            "should transition idle → busy on real output");
        assert_eq!(state.shell_states.get(sid).unwrap().load(Ordering::Relaxed), SHELL_BUSY);
    }

    // --- ChunkProcessor tests ---

    #[test]
    fn test_chunk_processor_new_has_correct_defaults() {
        let cp = ChunkProcessor::new(Some("/home/user/repo".to_string()));
        assert_eq!(cp.session_cwd, Some("/home/user/repo".to_string()));
        assert!(cp.last_status_task.is_none());
        assert!(cp.last_question_text.is_none());
    }

    #[test]
    fn test_chunk_processor_dedup_status_task() {
        use crate::state::VtLogBuffer;
        use std::sync::atomic::AtomicU64;

        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let sid = "test-cp-dedup";
        let silence = Arc::new(Mutex::new(SilenceState::new()));
        state.silence_states.insert(sid.to_string(), silence.clone());
        state.shell_states.insert(sid.to_string(), std::sync::atomic::AtomicU8::new(SHELL_NULL));
        state.vt_log_buffers.insert(sid.to_string(), Mutex::new(VtLogBuffer::new(24, 80, 1000)));
        state.output_buffers.insert(sid.to_string(), Mutex::new(OutputRingBuffer::new(4096)));
        state.last_output_ms.insert(sid.to_string(), AtomicU64::new(0));

        let mut cp = ChunkProcessor::new(None);
        let mut utf8_buf = Utf8ReadBuffer::new();
        let mut esc_buf = EscapeAwareBuffer::new();

        // First chunk with status line "* Reading files..."
        let raw = b"* Reading files...";
        let utf8_data = utf8_buf.push(raw);
        let esc_data = esc_buf.push(&utf8_data);
        let result1 = cp.process_chunk(&esc_data, &silence, sid, &state, None);

        // Count how many PtyParsed events were sent with StatusLine
        let mut rx = state.event_bus.subscribe();
        // Second chunk with same status — should be deduped
        let raw2 = b"\r\n* Reading files...";
        let utf8_data2 = utf8_buf.push(raw2);
        let esc_data2 = esc_buf.push(&utf8_data2);
        let _result2 = cp.process_chunk(&esc_data2, &silence, sid, &state, None);

        // Collect events from the second call
        let mut status_count = 0;
        while let Ok(evt) = rx.try_recv() {
            if let crate::state::AppEvent::PtyParsed { parsed, .. } = evt
                && parsed.get("type").and_then(|t| t.as_str()) == Some("StatusLine")
            {
                status_count += 1;
            }
        }
        assert_eq!(status_count, 0, "duplicate StatusLine with same task_name should be deduped");

        // Verify the result contains data
        assert!(result1.is_some(), "first chunk should return data");
    }

    #[test]
    fn test_chunk_processor_planfile_resolution() {
        let cp = ChunkProcessor::new(Some("/home/user/repo".to_string()));
        // Test that resolve_planfile_path resolves relative paths
        let resolved = cp.resolve_planfile_path("plans/foo.md");
        assert_eq!(resolved, Some("/home/user/repo/plans/foo.md".to_string()));
    }

    #[test]
    fn test_chunk_processor_planfile_resolution_absolute_passthrough() {
        let cp = ChunkProcessor::new(Some("/home/user/repo".to_string()));
        let resolved = cp.resolve_planfile_path("/absolute/path/plan.md");
        assert_eq!(resolved, Some("/absolute/path/plan.md".to_string()));
    }

    #[test]
    fn test_chunk_processor_planfile_resolution_no_cwd() {
        let cp = ChunkProcessor::new(None);
        // Relative path with no CWD should return None
        let resolved = cp.resolve_planfile_path("plans/foo.md");
        assert_eq!(resolved, None);
    }

    #[test]
    fn test_chunk_processor_planfile_normalizes_dotdot() {
        let cp = ChunkProcessor::new(Some("/home/user/repo__wt/feat".to_string()));
        let resolved = cp.resolve_planfile_path("../../repo/plans/foo.md");
        assert_eq!(resolved, Some("/home/user/repo/plans/foo.md".to_string()));
    }

    // --- has_unclosed_token tests ---

    #[test]
    fn test_has_unclosed_token_complete_intent() {
        assert!(!has_unclosed_token("[[intent: Fix the bug(Fixing)]]"));
    }

    #[test]
    fn test_has_unclosed_token_incomplete_intent() {
        assert!(has_unclosed_token("[[intent: Fix the bug"));
    }

    #[test]
    fn test_has_unclosed_token_complete_suggest() {
        assert!(!has_unclosed_token("[[suggest: A | B | C]]"));
    }

    #[test]
    fn test_has_unclosed_token_incomplete_suggest() {
        assert!(has_unclosed_token("[[suggest: A | B"));
    }

    #[test]
    fn test_has_unclosed_token_no_token() {
        assert!(!has_unclosed_token("just regular output"));
    }

    #[test]
    fn test_has_unclosed_token_single_bracket() {
        assert!(!has_unclosed_token("[intent: some text]"));
    }

    #[test]
    fn test_has_unclosed_token_single_bracket_incomplete() {
        assert!(has_unclosed_token("[intent: some text"));
    }

    #[test]
    fn test_has_unclosed_token_unicode_close() {
        assert!(!has_unclosed_token("\u{27E6}intent: text\u{27E7}"));
    }

    #[test]
    fn test_has_unclosed_token_text_before_token() {
        // Text before + incomplete token at end
        assert!(has_unclosed_token("hello world\n[[intent: doing stuff"));
    }

    #[test]
    fn test_has_unclosed_token_complete_then_incomplete() {
        // First token complete, second incomplete — should detect unclosed
        assert!(has_unclosed_token("[[intent: done]] then [[suggest: A | B"));
    }

    // --- transform_xterm buffering tests ---

    #[test]
    fn test_transform_xterm_complete_token_passes_through() {
        let mut cp = ChunkProcessor::new(None);
        let result = cp.transform_xterm("[[intent: Fix bug(Fix)]]".to_string());
        assert!(result.is_some(), "complete token should pass through");
        assert!(result.unwrap().contains("intent:"), "should be colorized");
    }

    #[test]
    fn test_transform_xterm_incomplete_buffers() {
        let mut cp = ChunkProcessor::new(None);
        let result = cp.transform_xterm("[[intent: Fix the".to_string());
        assert!(result.is_none(), "incomplete token should be buffered");
    }

    #[test]
    fn test_transform_xterm_buffered_completes_on_next_chunk() {
        let mut cp = ChunkProcessor::new(None);
        // First chunk: incomplete
        let r1 = cp.transform_xterm("[[intent: Fix the".to_string());
        assert!(r1.is_none());
        // Second chunk: completes the token
        let r2 = cp.transform_xterm(" bug(Fix)]]".to_string());
        assert!(r2.is_some(), "completed token should be emitted");
        let data = r2.unwrap();
        assert!(data.contains("intent:"), "should be colorized");
        assert!(!data.contains("[[intent:"), "raw brackets should be replaced");
    }

    #[test]
    fn test_transform_xterm_no_token_passes_through() {
        let mut cp = ChunkProcessor::new(None);
        let result = cp.transform_xterm("just regular output".to_string());
        assert_eq!(result, Some("just regular output".to_string()));
    }

    #[test]
    fn test_transform_xterm_suggest_concealed() {
        let mut cp = ChunkProcessor::new(None);
        let result = cp.transform_xterm("[[suggest: A | B | C]]".to_string());
        assert!(result.is_some());
        let data = result.unwrap();
        // Should not contain the raw suggest text (concealed)
        assert!(!data.contains("suggest:"), "suggest token should be concealed");
    }

    #[test]
    fn test_transform_xterm_suggest_incomplete_buffers() {
        let mut cp = ChunkProcessor::new(None);
        let r1 = cp.transform_xterm("[[suggest: A | B".to_string());
        assert!(r1.is_none());
        let r2 = cp.transform_xterm(" | C]]".to_string());
        assert!(r2.is_some());
        assert!(!r2.unwrap().contains("suggest:"));
    }

    #[test]
    fn test_flush_pending_xterm() {
        let mut cp = ChunkProcessor::new(None);
        cp.transform_xterm("[[intent: incomplete".to_string());
        let flushed = cp.flush_pending_xterm();
        assert_eq!(flushed, Some("[[intent: incomplete".to_string()));
        // After flush, pending should be empty
        assert!(cp.flush_pending_xterm().is_none());
    }
}
