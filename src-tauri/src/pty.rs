use parking_lot::Mutex;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde::Serialize;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;

use crate::input_line_buffer::{InputAction, InputLineBuffer};
use crate::output_parser::{OutputParser, ParsedEvent};
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

/// Convert a Windows drive-letter path to a WSL `/mnt/` path.
/// E.g. `C:\Users\foo\repos` → `/mnt/c/Users/foo/repos`.
/// Returns the input unchanged if it's not a Windows drive-letter path.
pub(crate) fn windows_to_wsl_path(path: &str) -> String {
    let bytes = path.as_bytes();
    // Match "X:\" or "X:/" where X is an ASCII letter
    if bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
    {
        let drive = (bytes[0] as char).to_ascii_lowercase();
        let rest = &path[3..].replace('\\', "/");
        format!("/mnt/{drive}/{rest}")
    } else {
        path.to_string()
    }
}

/// Check whether a shell string targets WSL (e.g. `wsl.exe`, `wsl.exe -d Ubuntu`).
/// Handles both forward-slash and backslash path separators so it works
/// correctly regardless of compilation target (cross-compiled from macOS/Linux).
pub(crate) fn is_wsl_shell(shell: &str) -> bool {
    let exe = shell.split_whitespace().next().unwrap_or("");
    // Extract filename from the last path separator (either / or \)
    let filename = exe.rsplit(['/', '\\']).next().unwrap_or(exe);
    // Strip .exe extension if present
    let stem = filename.strip_suffix(".exe")
        .or_else(|| filename.strip_suffix(".EXE"))
        .unwrap_or(filename);
    stem.eq_ignore_ascii_case("wsl")
}

/// Inject the Unix-style env vars that Claude Code / Ink need to detect
/// terminal capabilities (color, kitty keyboard protocol, etc.).
fn inject_unix_terminal_env(cmd: &mut CommandBuilder) {
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

/// Build a CommandBuilder for the given shell with platform-appropriate flags.
///
/// The `shell` string may contain arguments (e.g. `wsl.exe -d Ubuntu`).
/// The first whitespace-delimited token is the executable; the rest are args.
pub(crate) fn build_shell_command(shell: &str) -> CommandBuilder {
    let mut parts = shell.split_whitespace();
    let exe = parts.next().unwrap_or(shell);
    #[allow(unused_mut)]
    let mut cmd = CommandBuilder::new(exe);
    for arg in parts {
        cmd.arg(arg);
    }

    #[cfg(not(windows))]
    {
        // Login shell flag is Unix-only; PowerShell/cmd.exe don't support -l
        cmd.arg("-l");
        inject_unix_terminal_env(&mut cmd);
    }

    #[cfg(windows)]
    {
        // On Windows, if the shell targets WSL, inject Unix-style env vars
        // so that tools inside WSL (Claude Code, etc.) detect terminal
        // capabilities correctly. These are passed through to the Linux
        // environment by wsl.exe.
        if is_wsl_shell(shell) {
            inject_unix_terminal_env(&mut cmd);
        }
    }

    cmd
}

/// Resolve the shell to use: explicit override > env default > platform default.
pub(crate) fn resolve_shell(override_shell: Option<String>) -> String {
    let shell = override_shell.unwrap_or_else(default_shell);
    crate::cli::expand_tilde(&shell)
}

/// Which family of shell is running inside a PTY.
///
/// Used by the frontend to decide whether control characters like Ctrl-U are
/// honoured (POSIX readline) or echoed literally (`cmd.exe`, PowerShell).
/// Classifying by the shell command rather than by host OS is the whole point
/// of story 1274-2e38: Git Bash, Cygwin, MSYS and WSL all run on Windows yet
/// support Ctrl-U, so a host-OS check alone is wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ShellFamily {
    /// POSIX shell with readline semantics: sh, bash, zsh, fish, dash, ksh,
    /// and friends — including WSL (spawns a Linux shell) and Git Bash /
    /// Cygwin / MSYS (bash compiled for Windows).
    Posix,
    /// Native Windows shell that treats Ctrl-U as a literal character:
    /// cmd.exe, PowerShell, pwsh.
    WindowsNative,
    /// Shell basename didn't match any known set. Callers should fall back to
    /// the safer default for their host (on Windows: skip Ctrl-U; on
    /// Unix: send it).
    Unknown,
}

/// Classify a shell command string (as passed to `portable_pty`) into a
/// [`ShellFamily`]. Pure function — no I/O, no env lookups — so it's easy to
/// test against the set of strings the UI actually produces.
///
/// Parses the leading binary path first (supports Windows paths with spaces
/// like `C:\Program Files\Git\bin\bash.exe`), then matches the basename
/// case-insensitively with any `.exe` suffix stripped.
pub(crate) fn classify_shell(cmd: &str) -> ShellFamily {
    let trimmed = cmd.trim().trim_matches('"');
    // Locate the binary portion: if there's a case-insensitive `.exe`, take
    // everything up to and including it; otherwise split on first whitespace.
    // This keeps `C:\Program Files\...\bash.exe` intact while still trimming
    // trailing args like `wsl.exe -d Ubuntu`.
    let exe = match trimmed.to_ascii_lowercase().find(".exe") {
        Some(idx) => &trimmed[..idx + ".exe".len()],
        None => trimmed.split_whitespace().next().unwrap_or(""),
    };
    let filename = exe.rsplit(['/', '\\']).next().unwrap_or(exe);
    let stem = filename
        .strip_suffix(".exe")
        .or_else(|| filename.strip_suffix(".EXE"))
        .or_else(|| filename.strip_suffix(".Exe"))
        .unwrap_or(filename)
        .to_ascii_lowercase();

    match stem.as_str() {
        // POSIX shells (same set we pattern-match elsewhere in pty.rs)
        "sh" | "bash" | "zsh" | "fish" | "dash" | "ksh" | "ash" | "tcsh" | "csh" | "mksh" => {
            ShellFamily::Posix
        }
        // WSL spawns a Linux shell — readline semantics apply.
        "wsl" => ShellFamily::Posix,
        // Native Windows shells: Ctrl-U is not line-kill.
        "cmd" | "powershell" | "pwsh" => ShellFamily::WindowsNative,
        _ => ShellFamily::Unknown,
    }
}

/// How long the agent must be silent after printing a `?`-ending line before
/// we treat it as a question waiting for input. 10s is long enough to avoid
/// false positives from AI agents that pause while thinking between API calls.
const SILENCE_QUESTION_THRESHOLD: std::time::Duration = std::time::Duration::from_secs(10);

/// Maximum non-`?` chunks allowed after a `?` candidate before considering it stale.
/// Claude Code prints 2-3 decoration chunks after a question (mode line, separator).
/// Anything beyond this threshold means the agent continued working — not waiting.
const STALE_QUESTION_CHUNKS: u32 = 10;

/// How long the agent must be silent after printing a tool-error line before
/// we treat it as a turn-ending error (fire `playError()`). Shorter than the
/// question threshold because tool errors are typically followed by immediate
/// turn end (no retry) — 5s is enough to rule out a same-chunk recovery.
const SILENCE_TOOL_ERROR_THRESHOLD: std::time::Duration = std::time::Duration::from_secs(5);

/// Detect a turn-ending tool-failure line like Claude Code's
/// `⎿  Error: Exit code 1`. Anchored to line-start with only non-letter,
/// non-quote prefix characters (whitespace, box-drawing glyphs) so source
/// code or markdown that merely quotes the literal `"Error: Exit code N"`
/// does NOT match — avoids false-positive red notifications when the user's
/// own pty.rs tests are displayed in a terminal.
fn is_tool_error_line(line: &str) -> bool {
    lazy_static::lazy_static! {
        static ref TOOL_ERROR_RE: regex::Regex =
            regex::Regex::new(r#"^[^A-Za-z"]*Error:\s*Exit code\s+\d+"#).unwrap();
    }
    TOOL_ERROR_RE.is_match(line)
}

/// How often the timer thread wakes up to check for silence.
const SILENCE_CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

/// Grace period after a PTY resize during which parsed events (Question, RateLimit,
/// ApiError) are suppressed. The shell redraws visible output after SIGWINCH, which
/// would otherwise re-trigger notifications for content already on screen.
const RESIZE_GRACE: std::time::Duration = std::time::Duration::from_millis(1000);

/// How long after user input to ignore `?`-ending echo lines from the PTY.
const ECHO_SUPPRESS_WINDOW: std::time::Duration = std::time::Duration::from_millis(500);

/// Grace period after PTY session start during which notifications (Question,
/// RateLimit, ApiError) are suppressed. When a CLI tool replays conversation
/// history (e.g. `claude --continue`), the burst of historical output contains
/// old errors and questions that would otherwise trigger stale notifications.
/// The grace ends when output pauses for STARTUP_SETTLE_SILENCE seconds,
/// indicating the replay is over and live output is starting.
const STARTUP_SETTLE_SILENCE: std::time::Duration = std::time::Duration::from_secs(5);

/// Safety cap: startup grace never lasts longer than this, even if output
/// never pauses (e.g. continuous build log).
const STARTUP_GRACE_MAX: std::time::Duration = std::time::Duration::from_secs(120);

/// Shell idle threshold: 500ms without real PTY output → transition busy→idle.
/// Matches the frontend's previous 500ms setTimeout in checkIdle.
const SHELL_IDLE_MS: u64 = 500;

/// Agent idle threshold: 2.5s without real PTY output → transition busy→idle.
/// AI agents produce output in bursts with natural thinking pauses (>500ms).
/// Using the shell threshold causes visible blue→green→blue oscillation.
/// Combined with the 2s frontend debounce, this gives ~4.5s total hold.
const AGENT_IDLE_MS: u64 = 2500;

/// Maximum time active_sub_tasks can block idle transition (30s).
/// If the parser sets active_sub_tasks > 0 but the agent exits or the
/// mode-line disappears without emitting count=0, the terminal would stay
/// busy forever. After this timeout with no real output, we force-clear
/// the stale counter and allow idle transition.
const SUBTASK_STALE_MS: u64 = 30_000;

/// AtomicU8 encoding for shell_states DashMap.
pub(crate) const SHELL_NULL: u8 = 0;
pub(crate) const SHELL_BUSY: u8 = 1;
pub(crate) const SHELL_IDLE: u8 = 2;

/// Converts a shell_states AtomicU8 value into its wire string ("busy"/"idle").
/// Unknown values (including SHELL_NULL) map to "idle" — the frontend treats
/// a just-created session with no output yet as idle, not busy.
pub(crate) fn shell_state_str(state: u8) -> &'static str {
    match state {
        SHELL_BUSY => "busy",
        SHELL_IDLE => "idle",
        _ => "idle",
    }
}

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

/// Returns true if a changed_row text looks like a suggest token line.
/// Used to exclude suggest rows from "real output" classification so they
/// don't reset the silence timer or stale pending questions.
fn is_suggest_row(text: &str) -> bool {
    let t = text.trim();
    t.contains("suggest:") && t.contains('|')
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
        .any(|r| {
            let t = r.trim();
            // Exact match or prefix match (question may be truncated/wrapped on screen)
            t == q || (!q.is_empty() && t.starts_with(q))
        })
}

use crate::chrome::{is_separator_line, is_prompt_line};

/// Returns true when the line is a TUIC protocol token (`suggest:` or `intent:`
/// with pipe-separated items). These are structural markers consumed by the
/// frontend, not agent chat content — they must be skipped by question detection.
fn is_protocol_token_line(text: &str) -> bool {
    let t = text.trim_start();
    (t.starts_with("suggest:") || t.starts_with("intent:")) && t.contains('|')
}

/// Returns the set of row indices occupied by a protocol token (including
/// terminal-wrapped continuation rows). A continuation row is a row that
/// immediately follows a `suggest:` or `intent:` row and contains `|` but
/// does NOT start a new token prefix. Used to exclude the entire suggest/intent
/// block from "last chat line" detection — without this, the continuation row
/// gets mistaken for real chat content and steals the question slot.
fn collect_protocol_token_indices(screen_rows: &[String]) -> std::collections::HashSet<usize> {
    let mut indices = std::collections::HashSet::new();
    for (i, row) in screen_rows.iter().enumerate() {
        if is_protocol_token_line(row) {
            indices.insert(i);
            // Walk forward to find continuation rows (wrapped by terminal width)
            for (j, row) in screen_rows.iter().enumerate().skip(i + 1) {
                let trimmed = row.trim();
                if trimmed.is_empty() {
                    break;
                }
                // Stop at rows that start a new protocol token or chat content
                if is_protocol_token_line(row)
                    || trimmed.starts_with('>')
                    || trimmed.starts_with('›')
                    || trimmed.starts_with('❯')
                    || trimmed.starts_with('●')
                    || trimmed.starts_with('⏺')
                {
                    break;
                }
                // A continuation row must contain the `|` separator — without
                // it, the row is regular text (like an answer) that happens
                // to follow the suggest line.
                if !trimmed.contains('|') {
                    break;
                }
                indices.insert(j);
            }
        }
    }
    indices
}

/// Find the last chat line above the prompt box and, if it is a plausible
/// `?`-ending question, return it. Suggest/intent protocol blocks (including
/// wrapped continuations) are transparently skipped because they sit between
/// the agent's question and the prompt but are not real chat content — the
/// agent emits the question first and the suggest arrives after.
///
/// Only the single last chat line is inspected. We deliberately do NOT walk
/// deeper looking for an older `?`: a multi-line scan would scavenge past
/// the current agent turn and pick up the user's own previous input (e.g.
/// `❯ tutto ok?`) or stale content from earlier in the conversation, firing
/// phantom notifications 10s after the reply.
pub(crate) fn find_last_chat_question(screen_rows: &[String]) -> Option<String> {
    let prompt_idx = screen_rows.iter().enumerate().rev()
        .find(|(_, row)| is_prompt_line(row))?
        .0;

    let protocol_indices = collect_protocol_token_indices(screen_rows);

    for i in (0..prompt_idx).rev() {
        if protocol_indices.contains(&i) {
            continue;
        }
        let trimmed = screen_rows[i].trim();
        if trimmed.is_empty() || is_separator_line(trimmed) || is_chrome_row(trimmed) {
            continue;
        }
        // First non-skip row above the prompt — this is the last chat line.
        // Check it for a question, otherwise give up: we do not scavenge
        // deeper into the buffer.
        if trimmed.ends_with('?') && is_plausible_question(trimmed) {
            return Some(trimmed.to_string());
        }
        return None;
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
    /// When the last chunk of ANY kind (real or chrome-only) was processed.
    /// Used by the backup idle timer to distinguish "no output at all" (reader
    /// blocked on read()) from "only chrome-only ticks arriving". The backup
    /// timer should only fire when truly no chunks arrive.
    pub(crate) last_chunk_at: std::time::Instant,
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
    /// When this session was created. Used with `startup_settled` to suppress
    /// notifications during the initial output burst (e.g. `--continue` replay).
    created_at: std::time::Instant,
    /// True once the session has settled after the initial output burst.
    /// Settled = output paused for STARTUP_SETTLE_SILENCE seconds, or
    /// STARTUP_GRACE_MAX has elapsed since creation.
    pub(crate) startup_settled: bool,
    /// The last `Error: Exit code N` line seen, awaiting silence verification.
    /// Cleared if real output (non-chrome, non-error) arrives — that means the
    /// agent recovered and the error is not turn-ending.
    pending_tool_error: Option<String>,
    /// Error lines already surfaced via `ToolError` in the current "input epoch"
    /// (since the last user line submit / session start). Persists across
    /// `clear_tool_error_on_recovery` so that scroll-induced reappearances of
    /// the same error in `changed_rows` do not re-fire the notification.
    /// Cleared on explicit user input so a recurring failure in a later turn
    /// can notify again.
    surfaced_tool_errors: std::collections::HashSet<String>,
    /// Parsed `suggest:` items awaiting silence-based flush. The parser detects
    /// the token synchronously with output, but we hold the event here until
    /// `check_suggest` confirms the turn has ended (`SILENCE_SUGGEST_THRESHOLD`
    /// elapsed since the last real output chunk). Eliminates the frontend
    /// `pendingSuggest` race: the event never reaches the UI before idle.
    pending_suggest_items: Option<Vec<String>>,
    /// Timestamp when `pending_suggest_items` was parked. Currently for
    /// diagnostics only — the flush decision is driven by `last_output_at`,
    /// not the park time.
    pending_suggest_at: Option<std::time::Instant>,
}

impl SilenceState {
    fn new() -> Self {
        Self {
            last_output_at: std::time::Instant::now(),
            pending_question_line: None,
            question_already_emitted: false,
            last_chunk_at: std::time::Instant::now(),
            last_resize_at: None,
            suppress_echo_until: None,
            last_status_line_at: None,
            output_chunks_after_question: 0,
            last_emitted_text: None,
            created_at: std::time::Instant::now(),
            startup_settled: false,
            pending_tool_error: None,
            surfaced_tool_errors: std::collections::HashSet::new(),
            pending_suggest_items: None,
            pending_suggest_at: None,
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

    /// Returns true if we are still in the startup grace period.
    /// During this window, notifications are suppressed to avoid reacting to
    /// historical output replayed by `--continue` or similar session restore.
    pub(crate) fn is_startup_grace(&self) -> bool {
        !self.startup_settled
    }

    /// Check if the startup grace should end and update the flag.
    /// Called by the silence timer thread every second.
    pub(crate) fn check_startup_settle(&mut self) {
        if self.startup_settled {
            return;
        }
        // Safety cap: always settle after STARTUP_GRACE_MAX
        if self.created_at.elapsed() >= STARTUP_GRACE_MAX {
            self.startup_settled = true;
            return;
        }
        // Settle after STARTUP_SETTLE_SILENCE without output
        if self.last_output_at.elapsed() >= STARTUP_SETTLE_SILENCE {
            self.startup_settled = true;
        }
    }

    /// Called by the reader thread after each chunk.
    /// `regex_found_question`: true if `parse()` already emitted a Question event.
    /// `last_question_line`: the last `?`-ending line in the chunk, if any.
    /// `has_status_line`: true if the chunk contained a StatusLine parsed event.
    /// `status_line_only`: true if the chunk contained ONLY status-line/mode-line updates.
    ///   Mode-line timer ticks (elapsed time updating every second) are not significant
    ///   output — they must not reset the silence timer or the spinner timestamp,
    ///   or questions asked by Ink agents will never be detected.
    pub(crate) fn on_chunk(&mut self, regex_found_question: bool, last_question_line: Option<String>, has_status_line: bool, status_line_only: bool, suggest_only: bool) {
        // Always track that a chunk arrived — used by the backup idle timer
        // to distinguish "reader blocked on read()" from "chrome ticks arriving".
        self.last_chunk_at = std::time::Instant::now();

        // Suggest-only chunks are not significant output — they are protocol
        // tokens consumed by the frontend, not real agent text.
        let insignificant = status_line_only || suggest_only;

        if !insignificant {
            self.last_output_at = std::time::Instant::now();
        }

        // Only mark spinner active when the status line accompanies real output.
        // Mode-line timer ticks and suggest-only chunks are not agent activity
        // and must not suppress question detection.
        if has_status_line && !insignificant {
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
        } else if self.pending_question_line.is_some() && !insignificant {
            // Non-`?` chunk with real output after a pending candidate — track staleness.
            // Insignificant chunks (mode-line ticks, suggest tokens) are NOT real output
            // and must not count toward staleness, or they will clear the pending question
            // before the silence timer has a chance to detect it.
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

    /// Returns true if any chunk (real or chrome-only) was received recently.
    /// The backup idle timer uses this to avoid false idle transitions when the
    /// reader thread IS processing chunks (even chrome-only status-line ticks).
    /// Status-line ticking proves the agent is alive — the backup timer should
    /// only fire when truly no chunks arrive (reader blocked on read()).
    /// The 2s threshold matches the frontend debounce hold (BUSY_HOLD_MS).
    #[allow(dead_code)] // called from tests; kept for backup-idle-timer reintegration
    pub(crate) fn has_recent_chunks(&self) -> bool {
        self.last_chunk_at.elapsed() < std::time::Duration::from_secs(2)
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

    /// Register an `Error: Exit code N` line seen in visible output. The silence
    /// timer will emit a ToolError event if the session goes idle without any
    /// real-output chunk clearing the candidate (= agent did not recover).
    ///
    /// Idempotent across scroll-induced re-appearances: if this exact line has
    /// already surfaced in the current input epoch, we drop it. Without this,
    /// Ink-based TUIs (Claude Code, Codex) cause `changed_rows` to include the
    /// old error line every time the viewport scrolls, re-arming the candidate
    /// and re-firing the red notification long after the user has resumed.
    pub(crate) fn mark_tool_error_candidate(&mut self, line: String) {
        if self.surfaced_tool_errors.contains(&line) {
            return;
        }
        if self.pending_tool_error.as_deref() == Some(&line) {
            return;
        }
        self.pending_tool_error = Some(line);
    }

    /// Called on every real-output chunk that is NOT an error line. Clears the
    /// pending tool-error candidate: if the agent produced real output after an
    /// error, it recovered (e.g. retry) and the error is not turn-ending.
    ///
    /// Does NOT reset `surfaced_tool_errors` — recovery is a transient backend
    /// signal; the user-facing "I've already told you about this error" state
    /// must survive it and only reset on explicit user input.
    pub(crate) fn clear_tool_error_on_recovery(&mut self) {
        self.pending_tool_error = None;
    }

    /// Clear the "already surfaced" memory so the next occurrence of any error
    /// line — including one we've already fired — can notify again. Called
    /// from `write_pty` when the user submits a line (or Ctrl+C), mirroring
    /// the api-error dedup reset in `OutputParser::parse_clean_lines`.
    pub(crate) fn reset_tool_error_memory(&mut self) {
        self.pending_tool_error = None;
        self.surfaced_tool_errors.clear();
    }

    /// Called by the timer thread. Returns the error text if the silence
    /// threshold has been reached and we haven't emitted yet. Semantics mirror
    /// `check_silence` but use the shorter tool-error threshold.
    pub(crate) fn check_tool_error(&mut self) -> Option<String> {
        if self.is_spinner_active() {
            return None;
        }
        let should_fire = self
            .pending_tool_error
            .is_some()
            && self.last_output_at.elapsed() >= SILENCE_TOOL_ERROR_THRESHOLD;
        if !should_fire {
            return None;
        }
        let line = self.pending_tool_error.take()?;
        self.surfaced_tool_errors.insert(line.clone());
        Some(line)
    }

    /// Park `suggest:` items parsed from output. The silence timer will flush
    /// them to the frontend once the shell state transitions to idle — this
    /// is the single source of truth for "turn ended". A newer set overwrites
    /// an older pending set: if the agent updates its suggestions mid-turn,
    /// we deliver the latest.
    pub(crate) fn mark_suggest_candidate(&mut self, items: Vec<String>) {
        if items.is_empty() {
            return;
        }
        self.pending_suggest_items = Some(items);
        self.pending_suggest_at = Some(std::time::Instant::now());
    }

    /// Drain parked suggest items. No gates — trust the caller to invoke only
    /// when the shell state is IDLE (the silence timer does exactly that).
    /// Returns the items once and clears the park slot; a second call returns
    /// `None` until new items are parked.
    pub(crate) fn drain_pending_suggest(&mut self) -> Option<Vec<String>> {
        self.pending_suggest_at = None;
        self.pending_suggest_items.take()
    }

    /// Drop any parked suggest on user input. Parallels `reset_tool_error_memory`:
    /// the user is engaging again, so stale suggestions from the previous turn
    /// must not fire after a new input cycle starts.
    pub(crate) fn reset_suggest_memory(&mut self) {
        self.pending_suggest_items = None;
        self.pending_suggest_at = None;
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
/// Attempt an atomic shell-state transition.
///
/// When `notify_parent` is true and the transition is BUSY→IDLE, pushes a
/// state_change message to the parent's inbox (used during normal idle detection).
/// Pass `notify_parent=false` from process-exit paths — the sole "exited"
/// notification from `mark_session_exited` is sufficient; suppressing the
/// intermediate "idle" avoids the orchestrator double-firing on exit.
fn try_shell_transition(
    state: &crate::state::AppState,
    session_id: &str,
    expected: u8,
    new: u8,
    notify_parent: bool,
) -> bool {
    if let Some(atom) = state.shell_states.get(session_id) {
        let ok = atom.compare_exchange(
            expected,
            new,
            std::sync::atomic::Ordering::AcqRel,
            std::sync::atomic::Ordering::Relaxed,
        ).is_ok();
        if ok {
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            // Insert with the correct timestamp immediately so concurrent
            // readers never observe a transient 0 between or_insert and store.
            state.shell_state_since_ms
                .entry(session_id.to_string())
                .and_modify(|a| a.store(now_ms, std::sync::atomic::Ordering::Relaxed))
                .or_insert_with(|| std::sync::atomic::AtomicU64::new(now_ms));
            // Notify orchestrator when an agent goes idle (BUSY→IDLE only).
            // Plain shell sessions are excluded — only registered agent sessions qualify.
            if notify_parent && expected == SHELL_BUSY && new == SHELL_IDLE {
                let is_agent = state.session_states.get(session_id)
                    .map(|s| s.agent_type.is_some())
                    .unwrap_or(false);
                if is_agent {
                    push_state_change_to_parent(state, session_id, serde_json::json!({
                        "type": "state_change",
                        "state": "idle",
                        "session_id": session_id,
                    }));
                }
            }
        }
        ok
    } else {
        false
    }
}

/// Decision from `should_transition_idle`.
///
/// `force_cleared_subtasks` is true only on the stale-subtask recovery path —
/// callers must emit `ActiveSubtasks { count: 0 }` so the frontend store and
/// notification gate reset (story 1366-2b3e/H1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct IdleDecision {
    should_transition: bool,
    force_cleared_subtasks: bool,
}

impl IdleDecision {
    const NO: Self = Self { should_transition: false, force_cleared_subtasks: false };
    const YES: Self = Self { should_transition: true, force_cleared_subtasks: false };
}

/// Check whether the session should transition to idle (busy → idle).
/// Conditions: last real output > threshold ago AND no active sub-tasks.
/// Agent sessions use a longer threshold (AGENT_IDLE_MS) because AI agents
/// produce output in bursts with natural thinking pauses between them.
fn should_transition_idle(state: &crate::state::AppState, session_id: &str) -> IdleDecision {
    let last_ms = state.last_output_ms.get(session_id)
        .map(|ts| ts.load(std::sync::atomic::Ordering::Relaxed))
        .unwrap_or(0);
    if last_ms == 0 {
        return IdleDecision::NO;
    }
    // Read snapshot in a scoped block so the DashMap shard read-lock is
    // released before we take a write-lock below — same shard would otherwise
    // deadlock the runtime in the force-clear branch.
    let (is_agent, sub_tasks) = {
        let session = state.session_states.get(session_id);
        (
            session.as_ref().map(|s| s.agent_type.is_some()).unwrap_or(false),
            session.as_ref().map(|s| s.active_sub_tasks).unwrap_or(0),
        )
    };
    let threshold = if is_agent { AGENT_IDLE_MS } else { SHELL_IDLE_MS };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let elapsed = now.saturating_sub(last_ms);
    if elapsed < threshold {
        return IdleDecision::NO;
    }
    if sub_tasks == 0 {
        return IdleDecision::YES;
    }
    // Sub-tasks are active but no output for SUBTASK_STALE_MS — the mode-line
    // disappeared without emitting count=0 (agent exited, user cleared, etc.).
    // Force-clear the stale counter so we don't stay busy forever.
    if elapsed >= SUBTASK_STALE_MS {
        if let Some(mut entry) = state.session_states.get_mut(session_id) {
            entry.active_sub_tasks = 0;
        }
        return IdleDecision { should_transition: true, force_cleared_subtasks: true };
    }
    IdleDecision::NO
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

/// Emit an ActiveSubtasks parsed event via both event bus and Tauri IPC.
/// Used by the stale-subtasks recovery path to keep the frontend store in
/// sync after `should_transition_idle` force-clears the in-memory counter.
fn emit_active_subtasks(
    state: &crate::state::AppState,
    app: Option<&tauri::AppHandle>,
    session_id: &str,
    count: u32,
    task_type: &str,
) {
    let parsed = ParsedEvent::ActiveSubtasks { count, task_type: task_type.to_string() };
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

/// Emit an `Inferred` command outcome for shells that don't speak OSC 133.
/// Called right after a busy→idle transition; no-op once we've ever observed
/// a marker for this session (shell-integration path is authoritative then).
/// The command text is unknown in this mode, but cwd + snippet still populate
/// context summary and cwd history.
fn record_inferred_outcome_if_no_osc133(state: &AppState, session_id: &str) {
    use crate::ai_agent::knowledge::{CommandOutcome, OutcomeClass};

    if state.has_osc133_integration.contains_key(session_id) {
        return;
    }
    // try_lock to avoid blocking the timer thread if write_pty holds
    // the session lock. Inferred outcomes are best-effort — missing cwd
    // for one record is acceptable vs risking contention.
    let cwd = state
        .sessions
        .get(session_id)
        .and_then(|s| s.try_lock().and_then(|s| s.cwd.clone()))
        .unwrap_or_default();
    let output_snippet = state
        .vt_log_buffers
        .get(session_id)
        .map(|b| {
            let buf = b.lock();
            buf.screen_rows().join("\n")
        })
        .unwrap_or_default();
    let mut tail_start = output_snippet.len().saturating_sub(500);
    while tail_start > 0 && !output_snippet.is_char_boundary(tail_start) {
        tail_start += 1;
    }
    let output_snippet = output_snippet[tail_start..].to_string();

    let outcome = CommandOutcome {
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        command: String::new(),
        cwd,
        exit_code: None,
        output_snippet,
        classification: OutcomeClass::Inferred,
        duration_ms: 0,
        id: 0,
        semantic_intent: None,
    };
    state.record_outcome(session_id, outcome);
}

/// How many bottom screen rows to check when verifying a question candidate.
/// Wide enough to cover agent footer layouts (mode line, spinner, Wiz HUD,
/// suggest/intent blocks, trailing disclaimer text) that push the actual
/// question several rows above the prompt box.
const SCREEN_VERIFY_ROWS: usize = 20;

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

            // Sole idle path: the silence timer is the only code that transitions
            // busy → idle. The reader thread only does → busy on real output.
            // `should_transition_idle` checks elapsed time vs threshold (500ms shell /
            // 2500ms agent) and sub-task count. Spinner rows keep last_output_ms
            // fresh in the reader, so this won't fire while a spinner is active.
            if let Some(atom) = state.shell_states.get(&session_id)
                && atom.load(std::sync::atomic::Ordering::Acquire) == SHELL_BUSY
            {
                let decision = should_transition_idle(&state, &session_id);
                if decision.should_transition
                    && try_shell_transition(&state, &session_id, SHELL_BUSY, SHELL_IDLE, true)
                {
                    if decision.force_cleared_subtasks {
                        // Story 1366-2b3e/H1: the stale-recovery path inside
                        // should_transition_idle reset active_sub_tasks in-memory
                        // but the frontend store only learns from this stream.
                        // Without an explicit count=0 emission, the UI keeps a
                        // non-zero badge and notifications stay suppressed.
                        emit_active_subtasks(&state, app.as_ref(), &session_id, 0, "");
                    }
                    emit_shell_state(&state, app.as_ref(), &session_id, "idle");
                    record_inferred_outcome_if_no_osc133(&state, &session_id);
                }
            }

            // Update startup grace state (checks if output has settled).
            {
                let mut sl = silence.lock();
                sl.check_startup_settle();
                if sl.is_startup_grace() {
                    continue; // Still in startup burst — suppress question detection
                }
            }

            // Tool-error turn-end: `Error: Exit code N` + silence = fire playError.
            // Checked before question detection — a tool error is not a question.
            if let Some(text) = silence.lock().check_tool_error() {
                let parsed = ParsedEvent::ToolError { matched_text: text };
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

            // Suggest turn-end: drain parked `suggest:` items once the shell
            // has transitioned to IDLE. The reader parks them at parse time
            // (see write_pty's emit loop); gating the drain on shell_state ==
            // IDLE makes the frontend's `pendingSuggest` race impossible —
            // the event physically cannot reach the UI before idle.
            let shell_is_idle = state.shell_states.get(&session_id)
                .map(|atom| atom.load(std::sync::atomic::Ordering::Acquire) == SHELL_IDLE)
                .unwrap_or(false);
            if shell_is_idle
                && let Some(items) = silence.lock().drain_pending_suggest()
            {
                let parsed = ParsedEvent::Suggest { items };
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

            // Check temporal conditions first (shared by both strategies).
            let is_silent = silence.lock().is_silent();
            if !is_silent {
                continue;
            }

            // Strategy 1: screen-based — walk upward from the prompt box looking
            // for the most recent plausible question within a bounded window.
            // This is robust to trailing non-question text between the question
            // and the prompt box (e.g. "(stopping here — waiting for your answer)").
            let screen_question = state.vt_log_buffers.get(&session_id)
                .and_then(|vt| {
                    let rows = vt.lock().screen_rows();
                    let line = find_last_chat_question(&rows);
                    tracing::trace!(
                        session_id = %session_id,
                        found = line.is_some(),
                        line = line.as_deref().unwrap_or(""),
                        "DIAG silence_timer: screen strategy"
                    );
                    line
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
                        tracing::debug!(
                            session_id = %session_id,
                            question = %text,
                            on_screen = on_screen,
                            "silence_timer: chunk fallback"
                        );
                        if !on_screen {
                            silence.lock().clear_stale_question();
                            continue;
                        }
                        text.clone()
                    }
                    None => {
                        tracing::trace!(
                            session_id = %session_id,
                            "silence_timer: silent but no question candidate"
                        );
                        continue;
                    },
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
    /// Dedup: last emitted ChoicePrompt signature (title + option keys).
    /// Prevents re-emit on repaint while the dialog stays on screen.
    last_choice_prompt_sig: Option<String>,
    /// Session CWD for resolving relative plan-file paths
    session_cwd: Option<String>,
    /// Plan files awaiting creation on disk (agent announces before writing).
    /// Tuples of (absolute_path, deadline). Checked each chunk until file appears
    /// or 10s deadline expires. Already-emitted paths tracked for dedup.
    pending_planfiles: Vec<(String, std::time::Instant)>,
    /// Plan file paths already emitted — prevents re-emitting on spinner redraws.
    emitted_planfiles: std::collections::HashSet<String>,
    /// Tracks whether the terminal is in alternate screen buffer mode.
    /// Set on ESC[?1049h, cleared on ESC[?1049l.
    pub(crate) in_alt_buffer: bool,
    /// Structured terminal mode with nesting depth and app detection.
    terminal_mode: crate::ai_agent::tui_detect::TerminalMode,
    /// One-shot flag: inject ESC[2J before the next ESC[H cursor-home.
    /// Set on alt-buffer entry and when content may have shrunk (detected via
    /// cursor-up ESC[nA with n > previous). Consumed after inject fires.
    alt_buffer_needs_clear: bool,
    /// Tracks the largest cursor-up (ESC[nA) value seen since last clear.
    /// When a new ESC[nA arrives with n < last_cursor_up_n, content has shrunk
    /// and we need a clear to prevent ghost artifacts.
    last_cursor_up_n: u16,
    /// Last VtLogBuffer total_lines observed — used to detect growth and emit
    /// `pty-vt-log-total-{session_id}` for the scrollback overlay.
    last_vt_log_total: usize,
    /// Last VtLogBuffer oldest_offset observed — emit when buffer rotation
    /// advances oldest so the frontend can invalidate stale chunks proactively.
    last_vt_log_oldest: usize,
    /// Last time we emitted a `pty-vt-log-total-*` event. Throttled to ~100 ms
    /// to avoid flooding the frontend during heavy output.
    last_vt_log_emit: Option<std::time::Instant>,
    /// Command text captured on OSC 133 C — used when the matching D arrives
    /// to build a `CommandOutcome`. Cleared after D.
    pending_command: Option<String>,
    /// `Instant` when OSC 133 C arrived; used for `duration_ms`.
    pending_command_started: Option<std::time::Instant>,
}

impl ChunkProcessor {
    fn new(session_cwd: Option<String>) -> Self {
        Self {
            parser: OutputParser::new(),
            last_status_task: None,
            last_question_text: None,
            last_choice_prompt_sig: None,
            session_cwd,
            pending_planfiles: Vec::new(),
            emitted_planfiles: std::collections::HashSet::new(),
            in_alt_buffer: false,
            terminal_mode: crate::ai_agent::tui_detect::TerminalMode::Shell,
            alt_buffer_needs_clear: false,
            last_cursor_up_n: 0,
            last_vt_log_total: 0,
            last_vt_log_oldest: 0,
            last_vt_log_emit: None,
            pending_command: None,
            pending_command_started: None,
        }
    }

    /// Drives the OSC 133 state machine. Call once per chunk, before the
    /// chunk is transformed for display. On `C` captures the command text
    /// from the input line buffer; on `D(code)` builds a `CommandOutcome`
    /// and records it into `state.session_knowledge`.
    fn record_osc133_outcomes(&mut self, data: &str, session_id: &str, state: &AppState) {
        use crate::ai_agent::knowledge::{
            classify_error, scan_osc133, CommandOutcome, Osc133Marker, OutcomeClass,
            SessionKnowledge,
        };

        let markers = scan_osc133(data);
        if markers.is_empty() {
            return;
        }
        state
            .has_osc133_integration
            .insert(session_id.to_string(), ());

        for m in markers {
            match m {
                Osc133Marker::C => {
                    let cmd = state
                        .input_buffers
                        .get(session_id)
                        .map(|b| b.lock().content())
                        .unwrap_or_default();
                    self.pending_command = Some(cmd);
                    self.pending_command_started = Some(std::time::Instant::now());
                }
                Osc133Marker::D(exit_code) => {
                    let command = self.pending_command.take().unwrap_or_default();
                    let duration_ms = self
                        .pending_command_started
                        .take()
                        .map(|t| t.elapsed().as_millis() as u64)
                        .unwrap_or(0);
                    // Use cached cwd — avoids sessions.lock() in the reader
                    // thread (deadlock with write_pty holding that lock during
                    // blocking kernel write).
                    let cwd = self.session_cwd.clone().unwrap_or_default();
                    let output_snippet = state
                        .vt_log_buffers
                        .get(session_id)
                        .map(|b| {
                            let buf = b.lock();
                            buf.screen_rows().join("\n")
                        })
                        .unwrap_or_default();
                    let mut tail_start = output_snippet.len().saturating_sub(500);
                    while tail_start > 0 && !output_snippet.is_char_boundary(tail_start) {
                        tail_start += 1;
                    }
                    let output_snippet = output_snippet[tail_start..].to_string();

                    let classification = if exit_code == 0 {
                        OutcomeClass::Success
                    } else if let Some(error_type) = classify_error(&output_snippet) {
                        OutcomeClass::Error { error_type }
                    } else {
                        OutcomeClass::Error {
                            error_type: "unknown".into(),
                        }
                    };

                    let outcome = CommandOutcome {
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0),
                        command,
                        cwd,
                        exit_code: Some(exit_code),
                        output_snippet,
                        classification,
                        duration_ms,
                        id: 0,
                        semantic_intent: None,
                    };
                    {
                        let entry = state
                            .session_knowledge
                            .entry(session_id.to_string())
                            .or_insert_with(|| Mutex::new(SessionKnowledge::new()));
                        entry.lock().terminal_mode = self.terminal_mode.clone();
                    }
                    let snapshot = outcome.clone();
                    let outcome_id = state.record_outcome(session_id, outcome);
                    // Opt-in AI enrichment: non-blocking enqueue, dropped on
                    // full queue or disabled setting. Worker assigns
                    // `semantic_intent` asynchronously.
                    let mut enriched = snapshot;
                    enriched.id = outcome_id;
                    crate::ai_agent::enrichment::try_enqueue_outcome(session_id, &enriched);
                }
            }
        }
    }

    /// Colorize `intent:` tokens and apply alternate-buffer fixes on the xterm
    /// stream. Suggest tokens are NOT concealed here — the frontend's
    /// `eraseSuggestFromBuffer()` handles that via rAF after xterm renders.
    fn transform_xterm(&mut self, data: String) -> Option<String> {
        // Track alternate screen buffer state for the clear-before-home fix below.
        if data.contains("\x1b[?1049h") {
            self.in_alt_buffer = true;
            self.alt_buffer_needs_clear = true;
            self.terminal_mode = self.terminal_mode.on_alt_enter();
        } else if data.contains("\x1b[?1049l") {
            self.in_alt_buffer = false;
            self.alt_buffer_needs_clear = false;
            self.terminal_mode = self.terminal_mode.on_alt_exit();
        }

        // Detect content shrink in alternate buffer: when Ink's cursor-up (ESC[nA)
        // value decreases, the rendered content has gotten shorter and old lines
        // will persist as ghost artifacts. Schedule a clear for the next cursor-home.
        if self.in_alt_buffer
            && let Some(n) = extract_largest_cursor_up(&data) {
                if n < self.last_cursor_up_n && self.last_cursor_up_n > 0 {
                    self.alt_buffer_needs_clear = true;
                }
                self.last_cursor_up_n = n;
        }

        // Inject ESC[2J (clear screen) before ESC[H (cursor home) when needed.
        // Triggered on alt-buffer entry and when content shrinks. One-shot: consumed
        // after inject fires to prevent per-keystroke flicker.
        let data = if self.alt_buffer_needs_clear {
            let injected = inject_clear_before_cursor_home(&data);
            if injected.len() != data.len() {
                // inject happened — consume the flag
                self.alt_buffer_needs_clear = false;
            }
            injected
        } else {
            data
        };

        if data.is_empty() {
            return Some(String::new());
        }

        Some(data)
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

    /// Drain pending plan files: emit event for files that now exist, drop expired ones.
    fn check_pending_planfiles(
        &mut self,
        session_id: &str,
        state: &AppState,
        app: Option<&AppHandle>,
    ) {
        if self.pending_planfiles.is_empty() {
            return;
        }
        let now = std::time::Instant::now();
        let mut i = 0;
        while i < self.pending_planfiles.len() {
            let (ref path, deadline) = self.pending_planfiles[i];
            if now > deadline {
                tracing::info!("[plan-file] Retry expired (10s), dropping: {path}");
                self.pending_planfiles.swap_remove(i);
                continue;
            }
            if std::path::Path::new(path).is_file() {
                let path = self.pending_planfiles.swap_remove(i).0;
                tracing::info!("[plan-file] Retry succeeded: {path}");
                self.emitted_planfiles.insert(path.clone());
                let evt = ParsedEvent::PlanFile { path };
                if let Ok(json) = serde_json::to_value(&evt) {
                    let _ = state.event_bus.send(crate::state::AppEvent::PtyParsed {
                        session_id: session_id.to_string(),
                        parsed: json.clone(),
                    });
                    if let Some(a) = app {
                        let _ = a.emit("pty-parsed", serde_json::json!({
                            "session_id": session_id,
                            "parsed": json,
                        }));
                    }
                }
                continue;
            }
            i += 1;
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

        // Drive OSC 133 shell-integration state machine (command start/end markers).
        // No-op when the shell has no integration — falls back to busy→idle inferrer.
        self.record_osc133_outcomes(data, session_id, state);

        // Check pending plan files: emit if file appeared, drop if deadline expired.
        self.check_pending_planfiles(session_id, state, app);

        // Feed raw data (post-kitty-strip) into VT100 log buffer.
        // Also capture the post-process `total_lines` and `oldest_offset` so
        // we can emit a throttled growth/rotation event for the scrollback overlay.
        let (changed_rows, vt_log_total, vt_log_oldest) = if let Some(vt_log) = state.vt_log_buffers.get(session_id) {
            let mut vt = vt_log.lock();
            let changed = vt.process(data.as_bytes());
            let total = vt.total_lines();
            let oldest = vt.oldest_offset();

            // Filter out changed rows below the input area border (horizontal rule).
            // Claude Code (and similar agents) render a quota/budget status bar below
            // the input box separator. Those rows are cosmetic chrome — processing them
            // resets the silence timer and causes false busy→idle→question transitions.
            let changed = if !changed.is_empty() {
                let screen = vt.screen_rows();
                let refs: Vec<&str> = screen.iter().map(|s| s.as_str()).collect();
                if let Some(cutoff) = crate::chrome::find_chrome_cutoff(&refs) {
                    changed.into_iter().filter(|r| r.row_index < cutoff).collect()
                } else {
                    changed
                }
            } else {
                changed
            };

            (changed, Some(total), Some(oldest))
        } else {
            (Vec::new(), None, None)
        };

        // Emit scrollback-overlay growth/rotation event (throttled to 100ms).
        // Frontend listens to `pty-vt-log-total-{session_id}` and updates
        // cache.total and cache.oldest for the scrollback overlay.
        if let Some(new_total) = vt_log_total
            && new_total > self.last_vt_log_total
        {
            let should_emit = self
                .last_vt_log_emit
                .map(|t| t.elapsed() >= std::time::Duration::from_millis(100))
                .unwrap_or(true);
            if should_emit
                && let Some(a) = app
            {
                // Emit as a bare number — wrapping in an object corrupts
                // the cache's internal counter (Tauri serialization issue).
                let _ = a.emit(
                    &format!("pty-vt-log-total-{session_id}"),
                    new_total,
                );
                self.last_vt_log_emit = Some(std::time::Instant::now());
            }
            self.last_vt_log_total = new_total;
        }
        // Update oldest tracking (no event needed — frontend reads it on chunk fetch)
        if let Some(new_oldest) = vt_log_oldest {
            self.last_vt_log_oldest = new_oldest;
        }

        // Write to ring buffer and broadcast to WebSocket clients while
        // holding the ring lock. Serializing these two steps prevents a race
        // with WS catch-up: a newly-connecting handler that also takes
        // ring.lock() for its snapshot cannot observe a state where the byte
        // is in the ring but also still queued for live delivery, which
        // would cause the catch-up and the live stream to replay the same
        // bytes to the client.
        if let Some(ring) = state.output_buffers.get(session_id) {
            let mut ring_guard = ring.lock();
            ring_guard.write(data.as_bytes());
            if let Some(mut clients) = state.ws_clients.get_mut(session_id) {
                let owned = data.to_owned();
                clients.retain(|tx| tx.send(owned.clone()).is_ok());
            }
            drop(ring_guard);
        }

        // Parse events: OSC 9;4 progress from raw stream, others from clean rows.
        let (in_resize_grace, in_startup_grace) = {
            let sl = silence.lock();
            (sl.is_resize_grace(), sl.is_startup_grace())
        };
        let suppress_notifications = in_resize_grace || in_startup_grace;
        let mut events = Vec::new();
        if let Some(evt) = crate::output_parser::parse_osc94(data) {
            events.push(evt);
        }
        let agent_active_for_parse = state.session_states.get(session_id)
            .map(|s| s.agent_type.is_some())
            .unwrap_or(false);
        events.extend(self.parser.parse_clean_lines(&changed_rows, agent_active_for_parse));

        // Slash menu detection — trim prompt/status chrome from the bottom
        // of the screen before scanning, because the menu renders above the
        // prompt line (separator + ❯ + status bar) and the parser scans
        // bottom-up, breaking on the first non-matching row.
        // Slash menu detection — use full screen rows (not chrome-trimmed).
        // Claude Code v2.1+ renders autocomplete items BELOW the prompt chrome,
        // so trimming to above-chrome would discard the menu. parse_slash_menu
        // scans bottom-up, skips empty rows, and stops at the first non-matching
        // row (separator/chrome), so it safely finds items regardless of position.
        let slash_on = state.slash_mode.get(session_id)
            .is_some_and(|v| v.load(std::sync::atomic::Ordering::Relaxed));
        if slash_on
            && let Some(vt_log) = state.vt_log_buffers.get(session_id)
        {
            let screen = vt_log.lock().screen_rows();
            let menu = crate::output_parser::parse_slash_menu(&screen);
            tracing::debug!("slash_menu parse: sid={session_id} found={} rows={}", menu.is_some(), screen.len());
            if let Some(evt) = menu {
                events.push(evt);
            }
        }

        // ChoicePrompt detection — numbered confirmation dialogs rendered below
        // the prompt line (edit-confirm, bash-confirm, apply-patch). Runs on
        // every chunk (unlike slash_menu which is gated by slash_mode) because
        // these dialogs appear asynchronously when the agent requests input.
        // Parser uses a strict shape (title with ?/verb + ≥2 numbered options)
        // so false-positive cost is low. Dedup via last_choice_prompt_sig
        // guards against repaint re-emission.
        if let Some(vt_log) = state.vt_log_buffers.get(session_id) {
            let screen = vt_log.lock().screen_rows();
            if let Some(evt) = crate::output_parser::parse_choice_prompt(&screen) {
                events.push(evt);
            }
        }

        let regex_found_question = if suppress_notifications { false } else {
            events.iter().any(|e| matches!(e, ParsedEvent::Question { .. }))
        };

        // Emit events with dedup, grace filtering, and PlanFile resolution.
        for event in &events {
            if suppress_notifications && matches!(event,
                ParsedEvent::Question { .. }
                | ParsedEvent::RateLimit { .. }
                | ParsedEvent::ApiError { .. }
            ) {
                continue;
            }

            // Suggest: park in SilenceState and defer emission until silence
            // confirms the turn has ended. The frontend used to buffer these
            // events in `pendingSuggest` to compensate for suggest arriving
            // before `shell-state: idle`; gating the emission backend-side
            // removes the race and simplifies the Terminal event handler.
            if let ParsedEvent::Suggest { items } = event {
                silence.lock().mark_suggest_candidate(items.clone());
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

            // Dedup choice-prompt: skip if same (title + option keys) already emitted.
            // Signature keeps option order but ignores highlighted drift so cursor
            // movement within the dialog doesn't re-fire.
            if let ParsedEvent::ChoicePrompt { title, options, .. } = event {
                let sig = format!(
                    "{}|{}",
                    title,
                    options.iter().map(|o| o.key.as_str()).collect::<Vec<_>>().join(","),
                );
                if self.last_choice_prompt_sig.as_deref() == Some(sig.as_str()) {
                    continue;
                }
                self.last_choice_prompt_sig = Some(sig);
            }

            // Resolve relative plan-file paths to absolute using session CWD.
            // If the file doesn't exist yet (agent announces before writing),
            // queue it for retry — checked each chunk for up to 10 seconds.
            let resolved = if let ParsedEvent::PlanFile { path } = event {
                match self.resolve_planfile_path(path) {
                    Some(p) if self.emitted_planfiles.contains(&p) => {
                        // Already emitted — skip (spinner redraws re-parse the same line)
                        continue;
                    }
                    Some(p) if std::path::Path::new(&p).is_file() => {
                        tracing::info!("[plan-file] Detected: {p} (cwd={:?})", self.session_cwd);
                        self.emitted_planfiles.insert(p.clone());
                        Some(ParsedEvent::PlanFile { path: p })
                    }
                    Some(p) => {
                        // File not on disk yet — queue for retry if not already pending
                        if !self.pending_planfiles.iter().any(|(pp, _)| pp == &p) {
                            tracing::info!("[plan-file] Queued for retry: {p} (cwd={:?})", self.session_cwd);
                            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                            self.pending_planfiles.push((p, deadline));
                        }
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

            // Serialize once, reuse for both broadcast and Tauri IPC
            if let Ok(json) = serde_json::to_value(emit_event) {
                // Tauri IPC for desktop mode (emit the pre-serialized Value)
                if let Some(app) = app {
                    let _ = app.emit(
                        &format!("pty-parsed-{session_id}"),
                        &json,
                    );
                }
                // Broadcast to SSE/WebSocket consumers
                let _ = state.event_bus.send(crate::state::AppEvent::PtyParsed {
                    session_id: session_id.to_string(),
                    parsed: json,
                });
            }
        }

        // Update silence state for fallback question detection.
        let has_status_line = events.iter().any(|e| matches!(e, ParsedEvent::StatusLine { .. }));
        let last_q_line = extract_question_line(&changed_rows);
        // A chunk is chrome-only when no real output reached the screen.
        // Path 0: changed_rows is empty — nothing visible happened (cursor
        //   blink, OSC title update, mouse report, SGR-only sequence). Must
        //   count as chrome-only or these periodic re-emits latch the shell
        //   state to busy forever during genuine idle.
        // Path 1: every row has a chrome marker (is_chrome_row).
        // Path 2: parse_status_line detected a spinner pattern (Gemini braille,
        //   Aider Knight Rider) AND no row contains real agent output. A row is
        //   "real output" if it is not chrome and not blank — this prevents
        //   has_status_line from suppressing chunks that mix spinner + output.
        let all_chrome_markers = changed_rows.iter().all(|r| is_chrome_row(&r.text));
        let has_suggest = events.iter().any(|e| matches!(e, ParsedEvent::Suggest { .. }));
        let no_real_output = changed_rows.iter().all(|r| {
            is_chrome_row(&r.text)
                || r.text.trim().is_empty()
                || crate::chrome::is_separator_line(&r.text)
                || crate::chrome::is_prompt_line(&r.text)
                // Suggest tokens are protocol markers, not real agent output.
                // Without this, a visible suggest row makes the chunk look like
                // "real output" and increments the question staleness counter.
                || (has_suggest && is_suggest_row(&r.text))
        });
        let chrome_only = !regex_found_question
            && last_q_line.is_none()
            && (changed_rows.is_empty()
                || all_chrome_markers
                || ((has_status_line || has_suggest) && no_real_output));
        // Suggest-only: chunk produced only Suggest events (no real text).
        let suggest_only = has_suggest
            && !regex_found_question
            && last_q_line.is_none()
            && !has_status_line
            && no_real_output;
        {
            let mut sl = silence.lock();
            sl.on_chunk(regex_found_question, last_q_line, has_status_line, chrome_only, suggest_only);

            // Tool-error detection: scan visible rows for `Error: Exit code N`
            // emitted by Claude Code / Codex at the end of a failing tool call.
            // Fires playError() via silence_timer when followed only by chrome
            // until SILENCE_TOOL_ERROR_THRESHOLD elapses (= turn ended on error).
            let mut error_line: Option<String> = None;
            for row in changed_rows.iter() {
                if is_tool_error_line(&row.text) {
                    error_line = Some(row.text.trim().to_string());
                }
            }
            if let Some(line) = error_line {
                sl.mark_tool_error_candidate(line);
            } else if !chrome_only {
                // Real output without an error line → agent recovered/continued.
                sl.clear_tool_error_on_recovery();
            }
        }

        // Stamp last_output_ms for real output and for active spinner repaints.
        // Spinner rows (dingbats ✻, braille ⠋, Aider ░█) prove the agent is
        // alive even though they are chrome-only — keeping the timestamp fresh
        // prevents should_transition_idle from firing mid-think.
        let has_spinner = chrome_only
            && changed_rows.iter().any(|r| crate::chrome::is_spinner_row(&r.text));
        if (!chrome_only || has_spinner)
            && let Some(ts) = state.last_output_ms.get(session_id)
        {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            ts.store(now, std::sync::atomic::Ordering::Relaxed);
        }

        // Shell state: reader only transitions → BUSY on real output.
        // Idle transitions are handled exclusively by the silence timer to
        // eliminate the two-path race that caused 15+ fix/revert cycles.
        if !chrome_only && !silence.lock().is_resize_grace()
            && let Some(atom) = state.shell_states.get(session_id)
        {
            let prev = atom.load(std::sync::atomic::Ordering::Acquire);
            if prev != SHELL_BUSY
                && try_shell_transition(state, session_id, prev, SHELL_BUSY, true)
            {
                emit_shell_state(state, app, session_id, "busy");
            }
        }

        // Update terminal mode in SessionState when it changes.
        // Detect TUI app from visible screen rows while in alternate buffer.
        if self.terminal_mode.is_fullscreen() {
            let row_texts: Vec<&str> = changed_rows.iter().map(|r| r.text.as_str()).collect();
            if let Some(app) = crate::ai_agent::tui_detect::detect_app_from_rows(&row_texts) {
                self.terminal_mode = self.terminal_mode.with_app_hint(app.to_string());
            }
        }
        if let Some(mut entry) = state.session_states.get_mut(session_id) {
            let new_mode = if self.terminal_mode.is_fullscreen() {
                Some(self.terminal_mode.clone())
            } else {
                None
            };
            if entry.terminal_mode != new_mode {
                entry.terminal_mode = new_mode;
            }
        }

        Some(data.to_owned())
    }
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
                // try_lock: MUST NOT block the reader thread — blocking here
                // while write_pty holds the lock causes a circular deadlock
                // (reader blocked → kernel buffer fills → write_pty blocks on write).
                if let Some(sess) = state.sessions.get(session_id) {
                    if let Some(mut s) = sess.try_lock() {
                        let _ = s.writer.write_all(response.as_bytes());
                        let _ = s.writer.flush();
                    } else {
                        tracing::debug!(session_id = %session_id,
                            "kitty query response dropped — session lock contended");
                    }
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
    if !esc_remaining.is_empty()
        && let Some(ring) = state.output_buffers.get(session_id)
    {
        let mut ring_guard = ring.lock();
        ring_guard.write(esc_remaining.as_bytes());
        if let Some(mut clients) = state.ws_clients.get_mut(session_id) {
            clients.retain(|tx| tx.send(esc_remaining.clone()).is_ok());
        }
        drop(ring_guard);
    }
    esc_remaining
}

/// Fully remove session state from all DashMaps.
/// Called on explicit close/kill — caller has already consumed any output they need.
pub(crate) fn cleanup_session(session_id: &str, state: &AppState) {
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
    state.last_output_ms.remove(session_id);
    state.last_prompts.remove(session_id);
    state.terminal_rows.remove(session_id);
    state.exit_codes.remove(session_id);
}

/// Reap transient per-session state that has no post-mortem value, and stamp
/// `last_output_ms` so the tombstone sweeper can age the entry out.
/// Intentionally keeps: `output_buffers`, `vt_log_buffers`, `last_output_ms`, `exit_codes`.
fn tombstone_transient_cleanup(session_id: &str, state: &AppState) {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    state
        .last_output_ms
        .entry(session_id.to_string())
        .or_insert_with(|| AtomicU64::new(0))
        .store(now_ms, Ordering::Relaxed);
    state.ws_clients.remove(session_id);
    state.kitty_states.remove(session_id);
    state.input_buffers.remove(session_id);
    state.silence_states.remove(session_id);
    state.shell_states.remove(session_id);
    state.last_prompts.remove(session_id);
    state.terminal_rows.remove(session_id);
    // Swarm maps — inserted at spawn/register time, must be cleaned on exit.
    state.shell_state_since_ms.remove(session_id);
    state.session_parent.remove(session_id);
    // mcp_to_session maps mcp_session_id → tuic_session. The reverse index
    // session_to_mcp lets us drop O(k) entries (k = mcp sessions for this
    // tuic_session, typically 1) instead of scanning every entry.
    if let Some((_, mcp_sids)) = state.session_to_mcp.remove(session_id) {
        for sid in &mcp_sids {
            state.mcp_to_session.remove(sid);
        }
    }
}

/// Tombstone a session after its process exited.
/// Push a state_change message to the parent's inbox if this session has a registered parent.
/// Used for automatic orchestrator notifications on exit and idle transitions.
fn push_state_change_to_parent(state: &AppState, session_id: &str, payload: serde_json::Value) {
    let Some(parent_id) = state.session_parent.get(session_id).map(|e| e.value().clone()) else {
        return;
    };
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let msg = crate::state::AgentMessage {
        id: format!("tuic-auto-{}-{}", session_id, now_ms),
        from_tuic_session: session_id.to_string(),
        from_name: "tuic".to_string(),
        content: serde_json::to_string(&payload).unwrap_or_default(),
        timestamp: now_ms,
        delivered_via_channel: false,
    };
    let mut inbox = state.agent_inbox.entry(parent_id).or_default();
    if inbox.len() >= crate::state::AGENT_INBOX_CAPACITY {
        inbox.pop_front();
        // eviction counting intentionally skipped for system messages (no orchestrator opt-in needed)
    }
    inbox.push_back(msg);
}

/// Keeps `output_buffers`, `vt_log_buffers`, `last_output_ms`, and `exit_codes`
/// alive so MCP consumers can read final output + exit status post-mortem.
/// Tombstones are reaped by `spawn_tombstone_sweeper` after `TOMBSTONE_TTL_MS`.
pub(crate) fn mark_session_exited(session_id: &str, state: &AppState) {
    // Capture exit code before dropping the session entry.
    if let Some(entry) = state.sessions.get(session_id)
        && let Ok(Some(status)) = entry.value().lock()._child.try_wait()
    {
        state.exit_codes.insert(session_id.to_string(), status.exit_code() as i32);
    }
    if state.sessions.remove(session_id).is_some() {
        state.metrics.active_sessions.fetch_sub(1, Ordering::Relaxed);
    }

    // Notify orchestrator (if any) that this agent has exited.
    let exit_code = state.exit_codes.get(session_id).map(|e| *e.value());
    push_state_change_to_parent(state, session_id, serde_json::json!({
        "type": "state_change",
        "state": "exited",
        "session_id": session_id,
        "exit_code": exit_code,
    }));

    // SIMP-1: drain HTML tabs registered by this session and emit close.
    // Same helper used by `session(close)` and `session(kill)` so all three
    // exit paths drain `session_html_tabs` identically (no orphan tabs).
    crate::mcp_http::mcp_transport::emit_close_html_tabs(state, session_id);

    tombstone_transient_cleanup(session_id, state);
}

/// Time a tombstoned session's buffers remain readable after process exit.
pub(crate) const TOMBSTONE_TTL_MS: u64 = 5 * 60 * 1000; // 5 minutes

/// Background sweeper that reaps tombstoned session buffers once they age out.
/// Started once at boot from the HTTP server runtime.
pub(crate) fn spawn_tombstone_sweeper(state: Arc<AppState>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(30));
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        // A tombstone is: buffer entry present, session entry absent, aged past TTL.
        let candidates: Vec<String> = state
            .output_buffers
            .iter()
            .filter_map(|entry| {
                let id = entry.key();
                if state.sessions.contains_key(id) {
                    return None;
                }
                let last_ms = state
                    .last_output_ms
                    .get(id)
                    .map(|m| m.load(Ordering::Relaxed))
                    .unwrap_or(0);
                if last_ms == 0 || now_ms.saturating_sub(last_ms) < TOMBSTONE_TTL_MS {
                    return None;
                }
                Some(id.clone())
            })
            .collect();
        for id in candidates {
            state.output_buffers.remove(&id);
            state.vt_log_buffers.remove(&id);
            state.last_output_ms.remove(&id);
            state.exit_codes.remove(&id);
            tracing::debug!(source = "pty", session_id = %id, "Tombstone reaped");
        }
    });
}

/// Return the byte length of a UTF-8 character given its leading byte.
#[allow(dead_code)] // Used by clamp_cursor_up (currently disabled, see TODO May 2026)
#[inline]
fn utf8_char_width(lead: u8) -> usize {
    match lead {
        0..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF7 => 4,
        _ => 1, // continuation byte — shouldn't be a lead, advance 1
    }
}

/// Detect anomalous ANSI sequences that may cause scroll-jump-to-top or viewport resets.
/// Returns a list of human-readable labels for each detected sequence.
/// These are logged as warnings for diagnostic purposes — data is never modified.
fn detect_anomalous_sequences(data: &str) -> Vec<&'static str> {
    let bytes = data.as_bytes();
    let len = bytes.len();
    let mut found = Vec::new();
    let mut i = 0;

    while i < len {
        if bytes[i] == 0x1b && i + 1 < len && bytes[i + 1] == b'[' {
            i += 2; // skip ESC[

            // Check for ESC[? private mode sequences (alt screen)
            if i < len && bytes[i] == b'?' {
                i += 1;
                let num_start = i;
                while i < len && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                if i < len {
                    let num_str = std::str::from_utf8(&bytes[num_start..i]).unwrap_or("");
                    match (num_str, bytes[i]) {
                        ("1049", b'h') => found.push("ESC[?1049h (Alt Screen Enter)"),
                        ("1049", b'l') => found.push("ESC[?1049l (Alt Screen Exit)"),
                        _ => {}
                    }
                    i += 1;
                }
                // No continue — let the outer while loop re-evaluate i < len
            } else {
                // Parse numeric params: n or n;m
                let num_start = i;
                while i < len && (bytes[i].is_ascii_digit() || bytes[i] == b';') {
                    i += 1;
                }
                if i < len {
                    let params = std::str::from_utf8(&bytes[num_start..i]).unwrap_or("");
                    match bytes[i] {
                        b'J' => {
                            match params {
                                "2" => found.push("ESC[2J (Clear Screen)"),
                                "3" => found.push("ESC[3J (Clear Scrollback)"),
                                _ => {}
                            }
                        }
                        b'H' => {
                            // ESC[H or ESC[1;1H = Cursor Home
                            if params.is_empty() {
                                found.push("ESC[H (Cursor Home)");
                            } else if params == "1;1" {
                                found.push("ESC[1;1H (Cursor Home)");
                            }
                            // Other ESC[n;mH = regular cursor position, not anomalous
                        }
                        _ => {}
                    }
                    i += 1;
                }
            }
        } else {
            i += 1;
        }
    }

    found
}

/// Extract the largest ESC[nA (cursor-up) value from `data`.
/// Ink emits ESC[nA where n equals the previous render height before redrawing.
/// A decrease in n between consecutive redraws signals content shrinkage.
fn extract_largest_cursor_up(data: &str) -> Option<u16> {
    let bytes = data.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    let mut max_n: Option<u16> = None;

    while i < len {
        if bytes[i] == 0x1b && i + 1 < len && bytes[i + 1] == b'[' {
            i += 2;
            let num_start = i;
            while i < len && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i < len && bytes[i] == b'A' && i > num_start
                && let Ok(n) = std::str::from_utf8(&bytes[num_start..i]).unwrap_or("").parse::<u16>() {
                    max_n = Some(max_n.map_or(n, |prev: u16| prev.max(n)));
            }
            if i < len {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    max_n
}


/// Inject ESC[2J (clear screen) before the first ESC[H or ESC[1;1H (cursor home) in `data`.
///
/// Ink-based TUIs render differentially: they position the cursor at home and overwrite
/// changed cells but never send ESC[K (erase to end of line). When output shrinks between
/// redraws, old characters — especially box-drawing separators — persist as ghost artifacts.
///
/// Injecting a single ESC[2J before the cursor-home ensures the screen is blank before
/// the redraw starts. Because xterm.js processes the entire write() atomically (clear +
/// cursor home + new content happen before the next paint), no intermediate blank frame
/// is ever rendered to the user.
///
/// Only injects once per call (before the first cursor-home) to avoid unnecessary clears
/// for chunks that contain multiple ESC[H sequences (common in Ink's rapid redraws).
fn inject_clear_before_cursor_home(data: &str) -> String {
    let bytes = data.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == 0x1b && i + 1 < len && bytes[i + 1] == b'[' {
            let seq_start = i;
            i += 2; // skip ESC[
            // Parse optional numeric parameters
            let num_start = i;
            while i < len && (bytes[i].is_ascii_digit() || bytes[i] == b';') {
                i += 1;
            }
            if i < len && bytes[i] == b'H' {
                let params = std::str::from_utf8(&bytes[num_start..i]).unwrap_or("");
                // ESC[H (no params) or ESC[1;1H — both mean cursor home
                if params.is_empty() || params == "1;1" {
                    // Inject ESC[2J before this cursor-home sequence
                    let mut result = String::with_capacity(len + 4);
                    result.push_str(&data[..seq_start]);
                    result.push_str("\x1b[2J");
                    result.push_str(&data[seq_start..]);
                    return result;
                }
            }
            if i < len {
                i += 1; // skip command byte
            }
        } else {
            i += 1;
        }
    }

    // No cursor-home found — return as-is
    data.to_string()
}

/// Clamp cursor-up ANSI sequences (ESC[nA) so `n` never exceeds the viewport height.
///
/// Ink-based TUI agents (Claude Code, Codex) emit ESC[nA where n equals the previous
/// render height — potentially hundreds of lines. Terminals follow the cursor above the
/// visible viewport, causing a scroll jump to top. Clamping n to the viewport rows keeps
/// the cursor within the visible area without affecting rendering.
///
/// Also clamps ESC[nF (Cursor Previous Line) which has the same jump-to-top effect.
#[allow(dead_code)] // Disabled 2026-04-15 (scrollback proliferation). TODO: remove May 2026.
fn clamp_cursor_up(data: &str, max_rows: u16) -> String {
    use std::fmt::Write;

    let max = max_rows as usize;
    let bytes = data.as_bytes();
    let len = bytes.len();
    let mut result = String::with_capacity(len);
    let mut i = 0;

    while i < len {
        if bytes[i] == 0x1b && i + 1 < len && bytes[i + 1] == b'[' {
            // Parse ESC[ parameters
            let seq_start = i;
            i += 2; // skip ESC[
            let num_start = i;
            while i < len && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i < len && (bytes[i] == b'A' || bytes[i] == b'F') {
                // ESC[nA (Cursor Up) or ESC[nF (Cursor Previous Line)
                let n: usize = if num_start == i {
                    1 // ESC[A with no number means 1
                } else {
                    std::str::from_utf8(&bytes[num_start..i])
                        .unwrap_or("1")
                        .parse()
                        .unwrap_or(1)
                };
                let clamped = n.min(max);
                let cmd = bytes[i] as char;
                i += 1; // skip A/F
                let _ = write!(result, "\x1b[{clamped}{cmd}");
            } else {
                // Not a cursor-up sequence — emit as-is
                let end = if i < len { i + 1 } else { i };
                result.push_str(&data[seq_start..end]);
                i = end;
            }
        } else {
            // Decode UTF-8 character properly (bytes[i] as char would re-encode
            // high bytes as Latin-1 codepoints, corrupting multi-byte characters).
            let ch_len = utf8_char_width(bytes[i]);
            if i + ch_len <= len {
                // SAFETY: input `data` is a valid &str, so byte boundaries are valid UTF-8
                result.push_str(&data[i..i + ch_len]);
            } else {
                // Incomplete UTF-8 at end — emit raw byte (shouldn't happen with valid &str)
                result.push(bytes[i] as char);
            }
            i += ch_len.min(len - i).max(1);
        }
    }
    result
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
        let sid_for_panic = session_id.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut buf = [0u8; 65536];
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

                    if let Some(processed) = processor.process_chunk(&data, &silence, &session_id, &state, Some(&app))
                        && let Some(xterm_data) = processor.transform_xterm(processed)
                    {
                        let clamped_data = xterm_data;

                        let agent_active = state.session_states.get(&session_id)
                            .map(|s| s.agent_type.is_some())
                            .unwrap_or(false);
                        if !processor.in_alt_buffer && !agent_active && clamped_data.as_bytes().contains(&0x1b) {
                            let anomalies = detect_anomalous_sequences(&clamped_data);
                            for label in &anomalies {
                                tracing::warn!(source = "terminal", session_id = %session_id, "Anomalous ANSI sequence: {label}");
                            }
                        }

                        let _ = app.emit(
                            &format!("pty-output-{session_id}"),
                            PtyOutput {
                                session_id: session_id.clone(),
                                data: clamped_data,
                            },
                        );
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

        // Ensure shell state is idle on session end (frontend indicator only).
        // notify_parent=false: mark_session_exited sends the sole "exited" notification.
        if try_shell_transition(&state, &session_id, SHELL_BUSY, SHELL_IDLE, false) {
            emit_shell_state(&state, Some(&app), &session_id, "idle");
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
        tracing::info!(source = "pty", session_id = %session_id, "Session closed: process exited");
        let _ = state.event_bus.send(crate::state::AppEvent::SessionClosed {
            session_id: session_id.clone(),
            reason: "process_exit".to_string(),
        });
        let agent_type = state.session_states.get(&session_id)
            .and_then(|s| s.agent_type.clone());
        let _ = app.emit("session-closed", serde_json::json!({
            "session_id": session_id,
            "reason": "process_exit",
            "agent_type": agent_type,
        }));

        mark_session_exited(&session_id, &state);
        })); // end catch_unwind
        if let Err(panic_info) = result {
            let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic payload".to_string()
            };
            tracing::error!(session_id = %sid_for_panic, "READER THREAD PANICKED: {msg}");
        }
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
        let mut buf = [0u8; 65536];
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

        // Ensure shell state is idle on session end (frontend indicator only).
        // notify_parent=false: mark_session_exited sends the sole "exited" notification.
        if try_shell_transition(&state, &session_id, SHELL_BUSY, SHELL_IDLE, false) {
            emit_shell_state(&state, None, &session_id, "idle");
        }

        // Flush remaining bytes at EOF
        flush_eof(&mut utf8_buf, &mut esc_buf, &session_id, &state);

        // Broadcast exit so SSE/WebSocket consumers and Tauri frontend can clean up
        tracing::info!(source = "pty", session_id = %session_id, "Headless session closed: process exited");
        let _ = state.event_bus.send(crate::state::AppEvent::SessionClosed {
            session_id: session_id.clone(),
            reason: "process_exit".to_string(),
        });
        if let Some(app) = state.app_handle.read().as_ref() {
            let agent_type = state.session_states.get(&session_id)
                .and_then(|s| s.agent_type.clone());
            let _ = app.emit("session-closed", serde_json::json!({
                "session_id": session_id,
                "reason": "process_exit",
                "agent_type": agent_type,
            }));
        }

        mark_session_exited(&session_id, &state);
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
            let cwd = crate::cli::expand_tilde(cwd);
            if is_wsl_shell(&shell) {
                cmd.cwd(windows_to_wsl_path(&cwd));
            } else {
                cmd.cwd(cwd);
            }
        }

        // Inject OSC 133 shell integration (command block markers)
        if let Ok(data_dir) = app.path().app_data_dir() {
            crate::shell_integration::inject(&data_dir, &shell, &mut cmd);
        }

        // Inject stable session UUID so agents can use it for session binding
        // (e.g. `claude --session-id $TUIC_SESSION`, then `claude --resume $TUIC_SESSION`)
        if let Some(ref tuic_session) = config.tuic_session {
            cmd.env("TUIC_SESSION", tuic_session);
        }

        // Inject env flags (feature flags configured in Settings → Agents)
        for (key, value) in &config.env {
            cmd.env(key, value);
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
            shell: shell.clone(),
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
    state.terminal_rows.insert(session_id.clone(), std::sync::atomic::AtomicU16::new(rows));
    state.session_states.insert(session_id.clone(), crate::state::SessionState::default());

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
    let pty_rows = pty_config.rows.max(24);
    let _pty_cols = pty_config.cols.max(80);
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
        // Translate Windows drive-letter paths to /mnt/ for WSL shells
        if is_wsl_shell(&shell) {
            cmd.cwd(windows_to_wsl_path(&worktree_path.to_string_lossy()));
        } else {
            cmd.cwd(&worktree_path);
        }

        // Inject OSC 133 shell integration (command block markers)
        if let Ok(data_dir) = app.path().app_data_dir() {
            crate::shell_integration::inject(&data_dir, &shell, &mut cmd);
        }

        // Inject env flags (feature flags configured in Settings → Agents)
        for (key, value) in &pty_config.env {
            cmd.env(key, value);
        }

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

        Ok((session_id, pair.master, child, writer, reader, shell))
    })();

    let (session_id, master, child, writer, reader, shell) = match pty_result {
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
            shell,
        }),
    );
    state.metrics.total_spawned.fetch_add(1, Ordering::Relaxed);
    state.metrics.active_sessions.fetch_add(1, Ordering::Relaxed);

    // Create ring buffer, VT log buffer, and diff renderer for this session
    state.output_buffers.insert(
        session_id.clone(),
        Mutex::new(OutputRingBuffer::new(OUTPUT_RING_BUFFER_CAPACITY)),
    );
    state.vt_log_buffers.insert(
        session_id.clone(),
        Mutex::new(VtLogBuffer::new(24, 220, VT_LOG_BUFFER_CAPACITY)),
    );
    state.last_output_ms.insert(session_id.clone(), std::sync::atomic::AtomicU64::new(0));
    state.terminal_rows.insert(session_id.clone(), std::sync::atomic::AtomicU16::new(pty_rows));
    state.session_states.insert(session_id.clone(), crate::state::SessionState::default());

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
        tracing::trace!(session_id = %session_id, data_len = data.len(), "write_pty");
        let t0 = std::time::Instant::now();
        {
            let mut session = entry.lock();
            let lock_ms = t0.elapsed().as_millis();
            session
                .writer
                .write_all(data.as_bytes())
                .map_err(|e| format!("Failed to write to PTY: {e}"))?;
            session
                .writer
                .flush()
                .map_err(|e| format!("Failed to flush PTY: {e}"))?;
            let total_ms = t0.elapsed().as_millis();
            if total_ms > 100 {
                tracing::warn!(session_id = %session_id, lock_ms = %lock_ms, total_ms = %total_ms,
                    data_len = %data.len(), "write_pty SLOW — lock or write blocked");
            }
        }

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

        // On any line submit (Enter or Ctrl+C) reset the tool-error dedup
        // memory: the user is explicitly engaging again, so a recurrence of
        // the same failure in a later turn must be allowed to notify.
        // Mirrors `OutputParser`'s reset of `last_api_error_match` on UserInput.
        if line_submitted
            && let Some(ss) = state.silence_states.get(&session_id)
        {
            let mut sl = ss.lock();
            sl.reset_tool_error_memory();
            sl.reset_suggest_memory();
        }

        // Track slash command mode: true when the input buffer starts with /
        // Fallback: when ESC is sent before "/" (TerminalKeybar's handleSlash),
        // the InputLineBuffer consumes "/" as an unknown escape-sequence suffix
        // and never inserts it. Detect bare "/" writes that the buffer missed.
        let in_slash = if line_submitted {
            false
        } else {
            buf.content().starts_with('/') || (buf.content().is_empty() && data == "/")
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
        shell_state_str(atom.load(std::sync::atomic::Ordering::Relaxed)).to_string()
    })
}

/// Return the classified shell family for a PTY session.
/// Lets the frontend pick the correct control sequences (e.g. Ctrl-U as
/// line-kill for POSIX readline vs. literal-char on cmd.exe/PowerShell)
/// without re-deriving the classification on every keystroke.
#[tauri::command]
pub(crate) fn get_session_shell_family(
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> Option<ShellFamily> {
    state
        .sessions
        .get(&session_id)
        .map(|entry| classify_shell(&entry.lock().shell))
}

/// Enable or disable VT100 diff rendering for a PTY session.
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
    // Update terminal rows for cursor-up clamping in the reader thread.
    if let Some(r) = state.terminal_rows.get(&session_id) {
        r.store(rows, Ordering::Relaxed);
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
    tracing::debug!(session_id = %session_id, "PTY reader paused (flow control)");
    Ok(())
}

/// Resume PTY reader thread (flow control: frontend buffer drained)
#[tauri::command]
pub(crate) fn resume_pty(state: State<'_, Arc<AppState>>, session_id: String) -> Result<(), String> {
    let entry = state.sessions
        .get(&session_id)
        .ok_or_else(|| format!("Session not found: {session_id}"))?;
    entry.lock().paused.store(false, Ordering::Relaxed);
    tracing::debug!(session_id = %session_id, "PTY reader resumed (flow control)");
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

/// Close a PTY session core: sends Ctrl-C, waits briefly for graceful exit,
/// captures the exit code for the tombstone, and leaves `output_buffers` +
/// `vt_log_buffers` + `last_output_ms` + `exit_codes` alive so post-mortem
/// MCP reads can still return final output and exit status.
///
/// Shared between the Tauri `close_pty` command and the MCP `close` action —
/// both paths must tombstone identically, or post-mortem reads break.
/// Returns the worktree path when `cleanup_worktree` is true and the session
/// had one, so the caller can run `remove_worktree_internal` outside this fn.
pub(crate) fn close_pty_core(
    state: &AppState,
    session_id: &str,
    cleanup_worktree: bool,
) -> Option<crate::state::WorktreeInfo> {
    let (_, session_mutex) = state.sessions.remove(session_id)?;
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

    // If the child is still alive after the grace window, force-kill it.
    // Without this, agents that ignore Ctrl-C (e.g. claude) become orphans —
    // the cloned reader fd keeps the pty master alive, the slave never sees
    // EOF, and the reader thread spins forever.
    if matches!(session._child.try_wait(), Ok(None)) {
        if let Err(e) = session._child.kill() {
            tracing::warn!(session_id = %session_id, "close_pty_core SIGKILL fallback failed: {e}");
        }
        // Brief wait so try_wait can observe the termination and record the code.
        let kill_deadline = std::time::Instant::now() + std::time::Duration::from_millis(100);
        loop {
            match session._child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if std::time::Instant::now() >= kill_deadline => break,
                _ => std::thread::sleep(std::time::Duration::from_millis(10)),
            }
        }
    }

    // Capture exit code for the tombstone before dropping the child handle.
    if let Ok(Some(status)) = session._child.try_wait() {
        state
            .exit_codes
            .insert(session_id.to_string(), status.exit_code() as i32);
    }

    // Preserve output_buffers, vt_log_buffers, last_output_ms, exit_codes.
    // Tombstone sweeper reaps them after TOMBSTONE_TTL_MS.
    tombstone_transient_cleanup(session_id, state);

    let worktree_to_cleanup = if cleanup_worktree {
        session.worktree.clone()
    } else {
        None
    };

    // Drop session to release file handles (forcibly kills if still running)
    drop(session);

    worktree_to_cleanup
}

/// Force-kill a PTY session and tombstone it. Used by the MCP `kill` action.
/// Unlike `close_pty_core`, skips the Ctrl-C grace period — sends SIGKILL
/// immediately. The child exits near-instantly so `try_wait` captures the
/// exit code before the tombstone is stamped.
pub(crate) fn kill_pty_core(state: &AppState, session_id: &str) -> bool {
    let Some((_, session_mutex)) = state.sessions.remove(session_id) else {
        return false;
    };
    state.metrics.active_sessions.fetch_sub(1, Ordering::Relaxed);
    let mut session = session_mutex.into_inner();

    if let Err(e) = session._child.kill() {
        tracing::warn!(session_id = %session_id, "SIGKILL failed: {e}");
    }

    // Give the kernel a brief window to reap the child so try_wait sees it.
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(100);
    loop {
        match session._child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if std::time::Instant::now() >= deadline => break,
            _ => std::thread::sleep(std::time::Duration::from_millis(10)),
        }
    }

    if let Ok(Some(status)) = session._child.try_wait() {
        state
            .exit_codes
            .insert(session_id.to_string(), status.exit_code() as i32);
    }

    tombstone_transient_cleanup(session_id, state);
    drop(session);
    true
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
    if let Some(worktree) = close_pty_core(&state, &session_id, cleanup_worktree)
        && let Err(e) = remove_worktree_internal(&worktree)
    {
        tracing::warn!("Failed to cleanup worktree: {e}");
    }
    Ok(())
}

/// Look up the process name for a given PID using OS-native syscalls.
/// On macOS uses `proc_pidpath`, on Linux reads `/proc/{pid}/comm`.
/// Returns None if the lookup fails.
#[cfg(target_os = "macos")]
pub(crate) fn process_name_from_pid(pid: u32) -> Option<String> {
    let mut buf = [0u8; libc::MAXPATHLEN as usize];
    // SAFETY: proc_pidpath writes into the provided buffer up to buffersize bytes.
    // The buffer is stack-allocated with known size. pid is a valid u32 cast to i32.
    let ret = unsafe {
        libc::proc_pidpath(pid as i32, buf.as_mut_ptr().cast(), buf.len() as u32)
    };
    if ret <= 0 {
        return None;
    }
    let path = std::str::from_utf8(&buf[..ret as usize]).ok()?;
    // Extract just the binary name from the full path
    let basename = path.rsplit('/').next().unwrap_or(path);

    // Some agents install versioned binaries where the filename is a version number
    // (e.g. claude: ~/.local/share/claude/versions/2.1.87). When the basename
    // doesn't look like a program name, check the path for known agent directories.
    if classify_agent(basename).is_some() {
        return Some(basename.to_string());
    }
    // Fall back: match parent directory names against known agents
    for segment in path.rsplit('/').skip(1) {
        if classify_agent(segment).is_some() {
            return Some(segment.to_string());
        }
    }
    Some(basename.to_string())
}

#[cfg(target_os = "linux")]
pub(crate) fn process_name_from_pid(pid: u32) -> Option<String> {
    std::fs::read_to_string(format!("/proc/{pid}/comm"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
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
pub(crate) fn deepest_descendant_pid(root_pid: u32) -> Option<u32> {
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
    let detected: Option<String> = {
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
    };

    // Mirror the detected agent type into session_states so the PTY reader's
    // `agent_active_for_parse` check flips on and plain-prefix structured
    // tokens (`intent:`, `action:`, `suggest:`) start being parsed. Without
    // this sync, sessions started by running `claude` inside a plain shell
    // (as opposed to via the /agent spawn route) never enable plain-prefix
    // parsing, so intents never rename the tab.
    //
    // Sticky: only set on Some, never clear on None. Foreground-pgid sampling
    // is inherently flaky during subprocess transitions — when claude spawns a
    // short-lived grandchild (git, sed, rg) the pgid leader briefly points to
    // that unrecognized binary and classify_agent returns None. Writing that
    // None back would flip agent_active off and drop the very next
    // `suggest:`/`intent:` token even though claude is still the live agent.
    // Frontend useAgentPolling.ts applies the same stickiness (streak +
    // source=idle) on its store mirror; backend must match or the parser
    // gates off while the UI still shows the agent active. Session teardown
    // clears session_states entirely, so no explicit reset is needed here.
    if let Some(mut entry) = state.session_states.get_mut(&session_id)
        && detected.is_some()
        && entry.agent_type != detected
    {
        entry.agent_type = detected.clone();
    }

    detected
}

/// Check if a PTY session has a non-shell foreground process running.
/// Returns the process name (e.g. "htop", "node", "claude") or None if
/// the foreground is the shell itself (zsh, bash, fish, etc.).
#[tauri::command]
pub(crate) fn has_foreground_process(
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> Option<String> {
    const SHELLS: &[&str] = &[
        "zsh", "bash", "fish", "sh", "dash", "ksh", "csh", "tcsh",
        "nushell", "nu", "powershell", "pwsh", "cmd",
    ];
    let entry = state.sessions.get(&session_id)?;
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
    if SHELLS.contains(&name.as_str()) { None } else { Some(name) }
}

/// Debug: diagnose agent detection for a PTY session.
/// Returns each step of the detection pipeline so failures can be pinpointed.
#[tauri::command]
pub(crate) fn debug_agent_detection(
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> serde_json::Value {
    let entry = match state.sessions.get(&session_id) {
        Some(e) => e,
        None => return serde_json::json!({ "error": "session not found", "session_id": session_id }),
    };
    let session = entry.value().lock();

    #[cfg(not(windows))]
    {
        let raw_fd = session.master.as_raw_fd();
        let pgid = session.master.process_group_leader();
        let name = pgid.and_then(|p| process_name_from_pid(p as u32));
        let classified = name.as_deref().and_then(classify_agent);
        serde_json::json!({
            "session_id": session_id,
            "master_raw_fd": raw_fd,
            "process_group_leader": pgid,
            "process_name": name,
            "classified_agent": classified,
            "child_pid": session._child.process_id(),
        })
    }
    #[cfg(windows)]
    {
        let child_pid = session._child.process_id();
        let leaf = child_pid.and_then(deepest_descendant_pid);
        let name = leaf.and_then(process_name_from_pid);
        let classified = name.as_deref().and_then(classify_agent);
        serde_json::json!({
            "session_id": session_id,
            "child_pid": child_pid,
            "leaf_pid": leaf,
            "process_name": name,
            "classified_agent": classified,
        })
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

    // --- classify_shell tests (story 1274-2e38) ---

    #[test]
    fn classify_shell_bare_posix_basenames() {
        for s in ["sh", "bash", "zsh", "fish", "dash", "ksh", "ash", "tcsh", "csh", "mksh"] {
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
        for s in ["wsl", "wsl.exe", "wsl.exe -d Ubuntu", "C:\\Windows\\System32\\wsl.exe"] {
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
        assert_eq!(classify_shell("powershell.exe -NoProfile"), ShellFamily::WindowsNative);
    }

    #[test]
    fn classify_shell_unknown_for_other_binaries() {
        // Intentionally unknown — callers should fall back to a safe default.
        for s in ["python", "node", "/usr/bin/env", "", "   "] {
            assert_eq!(classify_shell(s), ShellFamily::Unknown, "{s:?}");
        }
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
    fn test_tool_error_mark_is_idempotent_while_pending() {
        let mut s = SilenceState::new();
        s.mark_tool_error_candidate("Error: Exit code 1".to_string());
        // Second mark for the same line while still pending → no-op.
        s.mark_tool_error_candidate("Error: Exit code 1".to_string());
        s.last_output_at = std::time::Instant::now()
            - SILENCE_TOOL_ERROR_THRESHOLD
            - std::time::Duration::from_millis(100);
        assert_eq!(
            s.check_tool_error(),
            Some("Error: Exit code 1".to_string())
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

    // --- Suggest backend-gating tests ---

    #[test]
    fn test_suggest_drain_returns_parked_items() {
        let mut s = SilenceState::new();
        s.mark_suggest_candidate(vec!["alpha".to_string(), "beta".to_string()]);
        assert_eq!(
            s.drain_pending_suggest(),
            Some(vec!["alpha".to_string(), "beta".to_string()])
        );
    }

    #[test]
    fn test_suggest_drain_consumes_items() {
        let mut s = SilenceState::new();
        s.mark_suggest_candidate(vec!["a".to_string()]);
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
    fn test_suggest_newer_items_overwrite_older() {
        let mut s = SilenceState::new();
        s.mark_suggest_candidate(vec!["old".to_string()]);
        s.mark_suggest_candidate(vec!["new1".to_string(), "new2".to_string()]);
        assert_eq!(
            s.drain_pending_suggest(),
            Some(vec!["new1".to_string(), "new2".to_string()]),
            "latest parked set must win (agent updated suggestions mid-turn)"
        );
    }

    #[test]
    fn test_suggest_reset_on_user_input() {
        let mut s = SilenceState::new();
        s.mark_suggest_candidate(vec!["stale".to_string()]);
        s.reset_suggest_memory();
        assert!(
            s.drain_pending_suggest().is_none(),
            "user input must drop pending suggest so it doesn't fire across turns"
        );
    }

    #[test]
    fn test_suggest_empty_items_ignored() {
        let mut s = SilenceState::new();
        s.mark_suggest_candidate(vec![]);
        assert!(s.pending_suggest_items.is_none(), "empty items must not park");
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
    fn test_silence_state_pending_but_too_soon() {
        let mut s = SilenceState::new();
        s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
        // Just set — not enough time has passed
        assert!(s.check_silence().is_none());
    }

    #[test]
    fn test_silence_state_pending_after_threshold() {
        let mut s = SilenceState::new();
        s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
        // Simulate time passing by backdating last_output_at
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert_eq!(s.check_silence(), Some("Continue?".to_string()));
    }

    #[test]
    fn test_silence_state_no_double_emission() {
        let mut s = SilenceState::new();
        s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert!(s.check_silence().is_some());
        // Second check should return None (already emitted)
        assert!(s.check_silence().is_none());
    }

    #[test]
    fn test_silence_state_regex_suppresses_timer() {
        let mut s = SilenceState::new();
        // regex_found_question = true means instant detection already fired
        s.on_chunk(true, Some("Would you like to proceed?".to_string()), false, false, false);
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert!(s.check_silence().is_none());
    }

    #[test]
    fn test_silence_state_regex_clears_prior_pending() {
        let mut s = SilenceState::new();
        // Silence detector has a pending question from an earlier chunk
        s.on_chunk(false, Some("Earlier question?".to_string()), false, false, false);
        assert!(s.pending_question_line.is_some());
        // Regex fires on a different event — no question line in this chunk
        s.on_chunk(true, None, false, false, false);
        assert!(s.pending_question_line.is_none(), "prior pending should be cleared when regex fires");
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
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
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert_eq!(s.check_silence(), Some("Continue?".to_string()));
    }

    #[test]
    fn test_silence_state_new_question_replaces_old() {
        let mut s = SilenceState::new();
        s.on_chunk(false, Some("First question?".to_string()), false, false, false);
        s.on_chunk(false, Some("Second question?".to_string()), false, false, false);
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert_eq!(s.check_silence(), Some("Second question?".to_string()));
    }

    #[test]
    fn test_silence_state_suppress_user_input() {
        let mut s = SilenceState::new();
        // User types a line ending with `?` — PTY will echo it back
        s.on_chunk(false, Some("c'è ancora una storia?".to_string()), false, false, false);
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
        s.on_chunk(false, Some("lo hai mai provato?".to_string()), false, false, false);
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
        s.on_chunk(false, Some("Would you like to proceed?".to_string()), false, false, false);
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        // Should fire — this is a real agent question
        assert_eq!(s.check_silence(), Some("Would you like to proceed?".to_string()));
    }

    #[test]
    fn test_silence_state_spinner_suppresses_question() {
        let mut s = SilenceState::new();
        // Agent prints a `?`-line alongside a status-line/spinner in the same chunk
        s.on_chunk(false, Some("Want me to proceed?".to_string()), true, false, false);
        // Simulate 10s+ of silence
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        // Should NOT emit question — spinner was recently active
        assert_eq!(s.check_silence(), None, "spinner active → no question");
    }

    #[test]
    fn test_silence_state_spinner_expired_allows_question() {
        let mut s = SilenceState::new();
        s.on_chunk(false, Some("Want me to proceed?".to_string()), true, false, false);
        // Spinner was active but long ago (>10s, matching SILENCE_QUESTION_THRESHOLD)
        s.last_status_line_at = Some(std::time::Instant::now() - std::time::Duration::from_secs(12));
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        // Spinner expired, question should fire
        assert_eq!(s.check_silence(), Some("Want me to proceed?".to_string()));
    }

    #[test]
    fn test_silence_state_spinner_within_10s_suppresses() {
        let mut s = SilenceState::new();
        s.on_chunk(false, Some("Want me to proceed?".to_string()), true, false, false);
        // Spinner was 8s ago — still within the 10s window
        s.last_status_line_at = Some(std::time::Instant::now() - std::time::Duration::from_secs(8));
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert_eq!(s.check_silence(), None, "spinner within 10s should suppress question");
    }

    // --- Status-line-only chunk tests ---

    #[test]
    fn test_silence_state_status_line_only_does_not_reset_silence() {
        let mut s = SilenceState::new();
        s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
        // Backdate last_output_at to simulate 10s of silence
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        // Mode-line timer tick: status_line_only = true, should NOT reset last_output_at
        s.on_chunk(false, None, true, true, false);
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
        s.on_chunk(false, Some("Vuoi fare un commit?".to_string()), true, false, false);

        // Simulate 10s+ passing: both last_output_at and last_status_line_at
        // age beyond the threshold (in real life, wall-clock time handles this).
        let past = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        s.last_output_at = past;
        s.last_status_line_at = Some(past);

        // Mode-line-only ticks keep coming — they must NOT refresh either timer.
        for _ in 0..10 {
            s.on_chunk(false, None, true, true, false);
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
        s.on_chunk(false, Some("Procedo?".to_string()), true, false, false);

        // Simulate 15 mode-line ticks (> STALE_QUESTION_CHUNKS=10)
        for _ in 0..15 {
            s.on_chunk(false, None, true, true, false);
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
        s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
        // Backdate to simulate 10s silence
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        // Regular (non-status-line) chunk resets the timer
        s.on_chunk(false, None, false, false, false);
        // Now we need to wait another 10s — should NOT fire yet
        assert_eq!(s.check_silence(), None,
            "regular chunk should reset silence timer");
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
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert_eq!(s.check_silence(), Some("Continue?".to_string()),
            "suggest-only chunks must not count toward question staleness");
    }

    // --- is_chrome_row / chrome_only classification tests ---

    #[test]
    fn test_chrome_only_empty_changed_rows_is_chrome() {
        // Empty changed_rows means the chunk produced no visible change
        // (cursor blink, OSC title update, mouse report). It must count as
        // chrome-only so periodic re-emits don't latch the shell to busy.
        let rows: Vec<ChangedRow> = vec![];
        assert!(compute_chrome_only(&rows, false, false, false),
            "empty changed_rows should be chrome_only (no real output)");
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
            && (rows.is_empty()
                || all_chrome_markers
                || (has_status_line && no_real_output))
    }

    #[test]
    fn test_chrome_only_formula_timer_tick_only() {
        // CC timer tick: only the timer row changed
        let rows = make_rows(&["\u{273B} Cogitated 3m 47s"]);
        assert!(compute_chrome_only(&rows, true, false, false),
            "timer-only tick should be chrome_only");
    }

    #[test]
    fn test_chrome_only_formula_timer_plus_separator() {
        // CC timer tick + separator repaint (ESC[2J full redraw)
        let rows = make_rows(&[
            "────────────────────────────────────",
            "\u{273B} Cogitated 3m 48s",
        ]);
        assert!(compute_chrome_only(&rows, true, false, false),
            "timer + separator should be chrome_only");
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
        assert!(compute_chrome_only(&rows, true, false, false),
            "timer + prompt + separator + mode-line should be chrome_only");
    }

    #[test]
    fn test_chrome_only_formula_timer_plus_blank_rows() {
        // CC timer tick with blank rows (padding in TUI)
        let rows = make_rows(&[
            "",
            "\u{273B} Cogitated 3m 48s",
            "",
        ]);
        assert!(compute_chrome_only(&rows, true, false, false),
            "timer + blank rows should be chrome_only");
    }

    #[test]
    fn test_chrome_only_formula_real_output_not_chrome() {
        // Real agent output mixed with status line
        let rows = make_rows(&[
            "I will edit the file for you.",
            "\u{273B} Cogitated 3m 48s",
        ]);
        assert!(!compute_chrome_only(&rows, true, false, false),
            "real text + timer should NOT be chrome_only");
    }

    #[test]
    fn test_chrome_only_formula_question_line_not_chrome() {
        // Even if all chrome, a pending question line disables chrome_only
        let rows = make_rows(&["\u{273B} Cogitated 3m 48s"]);
        assert!(!compute_chrome_only(&rows, true, false, true),
            "chrome with pending question should NOT be chrome_only");
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
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert_eq!(s.check_silence(), None, "stale question after many chunks should not fire");
    }

    #[test]
    fn test_silence_state_few_decoration_chunks_still_fires() {
        let mut s = SilenceState::new();
        s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
        // 3 decoration chunks (mode line, separator, prompt) — within threshold
        s.on_chunk(false, None, false, false, false);
        s.on_chunk(false, None, false, false, false);
        s.on_chunk(false, None, false, false, false);
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert_eq!(s.check_silence(), Some("Continue?".to_string()), "few decoration chunks should still fire");
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
        s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
        s.clear_stale_question();
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert_eq!(s.check_silence(), None, "cleared stale should not fire");
    }

    #[test]
    fn test_silence_state_clear_stale_allows_new_question() {
        let mut s = SilenceState::new();
        s.on_chunk(false, Some("Old?".to_string()), false, false, false);
        s.clear_stale_question();
        // New question after clear
        s.on_chunk(false, Some("New?".to_string()), false, false, false);
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert_eq!(s.check_silence(), Some("New?".to_string()), "new question after clear should fire");
    }

    #[test]
    fn test_silence_state_repaint_same_question_does_not_refire() {
        let mut s = SilenceState::new();
        // Question arrives, silence fires, mark emitted
        s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert!(s.check_silence().is_some());
        assert!(s.question_already_emitted);

        // Terminal repaint: same `?` line re-appears as a changed row.
        // This must NOT reset question_already_emitted.
        s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
        assert!(s.question_already_emitted, "repaint of same question must not reset emitted flag");
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert!(s.check_silence().is_none(), "same question repaint must not re-fire");
    }

    #[test]
    fn test_silence_state_stale_same_question_scroll_does_not_refire() {
        let mut s = SilenceState::new();
        let past = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        // Question fires via chunk-based detection (Strategy 2)
        s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
        s.last_output_at = past;
        assert!(s.check_silence().is_some());

        // Agent resumes: 15 non-`?` chunks (above STALE_QUESTION_CHUNKS)
        for _ in 0..15 {
            s.on_chunk(false, None, false, false, false);
        }
        assert!(s.pending_question_line.is_none(), "pending should be cleared by staleness");

        // Same "Continue?" reappears in changed_rows because new output scrolled it
        // to a different row. This is NOT a new question — must not re-fire.
        s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
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

        // Same question arrives again — now it IS a new question (user answered)
        s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
        s.last_output_at = past;
        assert_eq!(s.check_silence(), Some("Continue?".to_string()),
            "same question text after user input must fire as new question");
    }

    #[test]
    fn test_silence_state_screen_emitted_question_scroll_does_not_refire() {
        let mut s = SilenceState::new();
        let past = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);

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
        s.on_chunk(false, Some("Continue?".to_string()), false, false, false);
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
        assert!(s.check_silence().is_some());

        // Different question arrives — this IS a new question, must fire
        s.on_chunk(false, Some("Are you sure?".to_string()), false, false, false);
        s.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD - std::time::Duration::from_millis(100);
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


    // --- Startup grace period tests ---

    #[test]
    fn test_startup_grace_active_on_new_session() {
        let s = SilenceState::new();
        assert!(s.is_startup_grace(), "startup grace should be active on new session");
    }

    #[test]
    fn test_startup_grace_settles_after_silence() {
        let mut s = SilenceState::new();
        // Simulate output stopping long enough ago
        s.last_output_at = std::time::Instant::now() - STARTUP_SETTLE_SILENCE - std::time::Duration::from_millis(100);
        s.check_startup_settle();
        assert!(!s.is_startup_grace(), "startup grace should end after output silence");
    }

    #[test]
    fn test_startup_grace_persists_during_output() {
        let mut s = SilenceState::new();
        // Output is recent — grace should persist
        s.last_output_at = std::time::Instant::now();
        s.check_startup_settle();
        assert!(s.is_startup_grace(), "startup grace should persist while output is flowing");
    }

    #[test]
    fn test_startup_grace_safety_cap() {
        let mut s = SilenceState::new();
        // Created long ago, but output is recent — safety cap should force settle
        s.created_at = std::time::Instant::now() - STARTUP_GRACE_MAX - std::time::Duration::from_secs(1);
        s.last_output_at = std::time::Instant::now(); // output still flowing
        s.check_startup_settle();
        assert!(!s.is_startup_grace(), "startup grace should end at safety cap");
    }

    #[test]
    fn test_startup_grace_idempotent_after_settle() {
        let mut s = SilenceState::new();
        s.last_output_at = std::time::Instant::now() - STARTUP_SETTLE_SILENCE - std::time::Duration::from_millis(100);
        s.check_startup_settle();
        assert!(s.startup_settled);
        // Calling again doesn't change anything
        s.check_startup_settle();
        assert!(s.startup_settled);
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
        let events = parser.parse_clean_lines(&changed, true);
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
        let changed = vt_log.process(b"intent: Doing work (Test)");
        let events = parser.parse_clean_lines(&changed, true);
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
        silence.on_chunk(false, extract_question_line(&changed), false, false, false);

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
        silence.on_chunk(false, extract_question_line(&changed), false, false, false);

        // Mode line / prompt decoration arrives in a separate chunk
        let changed = vt_log.process(b"\r\n\xe2\x8f\xb5\xe2\x8f\xb5 Idle");
        silence.on_chunk(false, extract_question_line(&changed), false, false, false);

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

        let changed = vt_log.process(b"intent: Testing headless reader");
        let events = parser.parse_clean_lines(&changed, true);

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
        let events = parser.parse_clean_lines(&changed, true);

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
            b"\x1b[1F\x1b[2Kintent: Fix all 34 documentation gaps (Fixing gaps)"
        );
        let events = parser.parse_clean_lines(&changed, true);
        let intent = events.iter().find_map(|e| match e {
            ParsedEvent::Intent { text, title, .. } => Some((text.clone(), title.clone())),
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
        let changed2 = vt_log.process(b"1Fintent: Fix all gaps");

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
            b"\x1b[1;1H\x1b[38;2;128;128;128m\xe2\x97\x8f\x1b[0m \x1b[1mintent: Reading codebase structure (Reading code)\x1b[0m"
        );

        // Frame 2: Ink updates — cursor up, erase line, rewrite
        // This is how Ink typically does incremental updates
        let changed = vt_log.process(
            b"\x1b[1F\x1b[2K\x1b[38;2;128;128;128m\xe2\x97\x8f\x1b[0m \x1b[1mintent: Fix all 34 documentation gaps (Fixing gaps)\x1b[0m"
        );

        let events = parser.parse_clean_lines(&changed, true);
        let intent = events.iter().find_map(|e| match e {
            ParsedEvent::Intent { text, title, .. } => Some((text.clone(), title.clone())),
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

        // Simulate fragmented delivery of: \x1b[1F\x1b[2Kintent: Fix all gaps
        let fragments: Vec<&[u8]> = vec![
            b"\x1b[",      // CSI introducer
            b"1",           // parameter
            b"F",           // final byte (CPL)
            b"\x1b[",      // CSI introducer
            b"2K",          // erase line
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
        assert!(try_shell_transition(&state, sid, SHELL_NULL, SHELL_BUSY, true),
            "should transition null → busy");
        assert_eq!(state.shell_states.get(sid).unwrap().load(Ordering::Relaxed), SHELL_BUSY);

        // Transition busy → busy should fail (already busy, no re-emit)
        assert!(!try_shell_transition(&state, sid, SHELL_NULL, SHELL_BUSY, true),
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

        assert!(should_transition_idle(&state, sid).should_transition,
            "should be ready to transition idle (600ms elapsed, no sub-tasks)");
        assert!(try_shell_transition(&state, sid, SHELL_BUSY, SHELL_IDLE, true),
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

        assert!(!should_transition_idle(&state, sid).should_transition,
            "should NOT transition idle when active_sub_tasks > 0 and elapsed < SUBTASK_STALE_MS");
    }

    #[test]
    fn test_shell_state_idle_stale_subtasks_force_cleared() {
        use std::sync::atomic::{AtomicU8, AtomicU64};
        let state = crate::state::tests_support::make_test_app_state();
        let sid = "test-session";
        state.shell_states.insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));

        state.session_states.insert(sid.to_string(), crate::state::SessionState {
            active_sub_tasks: 2,
            ..Default::default()
        });

        // Set last output to 31s ago (> SUBTASK_STALE_MS)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap().as_millis() as u64;
        state.last_output_ms.insert(sid.to_string(), AtomicU64::new(now - 31_000));

        assert!(should_transition_idle(&state, sid).should_transition,
            "should transition idle when active_sub_tasks > 0 but elapsed >= SUBTASK_STALE_MS");
        // Verify the stale counter was force-cleared
        let sub = state.session_states.get(sid).map(|s| s.active_sub_tasks).unwrap_or(999);
        assert_eq!(sub, 0, "active_sub_tasks should be force-cleared to 0");
    }

    /// Story 1366-2b3e/H1: when the stale-subtasks recovery path force-clears
    /// the in-memory counter, the caller must emit ActiveSubtasks{count:0}
    /// so the frontend store and notification gate also reset.
    #[test]
    fn test_force_cleared_subtasks_signal_propagates() {
        use std::sync::atomic::{AtomicU8, AtomicU64};
        let state = crate::state::tests_support::make_test_app_state();
        let sid = "test-session";
        state.shell_states.insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));
        state.session_states.insert(sid.to_string(), crate::state::SessionState {
            active_sub_tasks: 3,
            ..Default::default()
        });
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap().as_millis() as u64;
        state.last_output_ms.insert(sid.to_string(), AtomicU64::new(now - 31_000));

        let decision = should_transition_idle(&state, sid);
        assert!(decision.should_transition, "stale path must transition idle");
        assert!(decision.force_cleared_subtasks,
            "stale path must signal force-clear so caller emits count=0");

        // Subscribe BEFORE emitting so the broadcast is captured.
        let mut rx = state.event_bus.subscribe();
        emit_active_subtasks(&state, None, sid, 0, "");

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
        state.shell_states.insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));
        state.session_states.insert(sid.to_string(), crate::state::SessionState::default());
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap().as_millis() as u64;
        state.last_output_ms.insert(sid.to_string(), AtomicU64::new(now - 600));

        let decision = should_transition_idle(&state, sid);
        assert!(decision.should_transition);
        assert!(!decision.force_cleared_subtasks,
            "no-sub-tasks idle must not request a redundant count=0 emission");
    }

    #[test]
    fn test_shell_state_no_idle_agent_session_under_agent_threshold() {
        use std::sync::atomic::{AtomicU8, AtomicU64};
        let state = crate::state::tests_support::make_test_app_state();
        let sid = "test-session";
        state.shell_states.insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));

        // Agent session: agent_type is set
        state.session_states.insert(sid.to_string(), crate::state::SessionState {
            agent_type: Some("claude".to_string()),
            ..Default::default()
        });

        // 600ms elapsed — would trigger idle for a shell, but NOT for an agent session
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap().as_millis() as u64;
        state.last_output_ms.insert(sid.to_string(), AtomicU64::new(now - 600));

        assert!(!should_transition_idle(&state, sid).should_transition,
            "agent session should NOT transition idle at 600ms (under AGENT_IDLE_MS)");
    }

    #[test]
    fn test_shell_state_idle_agent_session_over_agent_threshold() {
        use std::sync::atomic::{AtomicU8, AtomicU64};
        let state = crate::state::tests_support::make_test_app_state();
        let sid = "test-session";
        state.shell_states.insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));

        // Agent session: agent_type is set
        state.session_states.insert(sid.to_string(), crate::state::SessionState {
            agent_type: Some("claude".to_string()),
            ..Default::default()
        });

        // 3000ms elapsed — over the 2500ms agent threshold
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap().as_millis() as u64;
        state.last_output_ms.insert(sid.to_string(), AtomicU64::new(now - 3000));

        assert!(should_transition_idle(&state, sid).should_transition,
            "agent session SHOULD transition idle after agent threshold");
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

        assert!(!should_transition_idle(&state, sid).should_transition,
            "should NOT transition idle when only 200ms elapsed");
    }

    #[test]
    fn test_shell_state_cas_prevents_duplicate_idle() {
        use std::sync::atomic::{AtomicU8, Ordering};
        let state = crate::state::tests_support::make_test_app_state();
        let sid = "test-session";
        state.shell_states.insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));

        // First CAS succeeds
        assert!(try_shell_transition(&state, sid, SHELL_BUSY, SHELL_IDLE, true));
        // Second CAS fails (already idle)
        assert!(!try_shell_transition(&state, sid, SHELL_BUSY, SHELL_IDLE, true),
            "second idle transition must fail — already idle");
        assert_eq!(state.shell_states.get(sid).unwrap().load(Ordering::Relaxed), SHELL_IDLE);
    }

    #[test]
    fn test_shell_state_idle_to_busy_on_real_output() {
        use std::sync::atomic::{AtomicU8, Ordering};
        let state = crate::state::tests_support::make_test_app_state();
        let sid = "test-session";
        state.shell_states.insert(sid.to_string(), AtomicU8::new(SHELL_IDLE));

        assert!(try_shell_transition(&state, sid, SHELL_IDLE, SHELL_BUSY, true),
            "should transition idle → busy on real output");
        assert_eq!(state.shell_states.get(sid).unwrap().load(Ordering::Relaxed), SHELL_BUSY);
    }

    // --- Backup idle guard: has_recent_chunks ---

    #[test]
    fn test_has_recent_chunks_true_after_any_chunk() {
        let mut s = SilenceState::new();
        // Any chunk (including chrome-only) updates last_chunk_at
        s.on_chunk(false, None, true, true, false);
        assert!(s.has_recent_chunks(),
            "has_recent_chunks should be true right after any chunk");
    }

    #[test]
    fn test_has_recent_chunks_true_after_real_chunk() {
        let mut s = SilenceState::new();
        s.on_chunk(false, None, false, false, false);
        assert!(s.has_recent_chunks(),
            "has_recent_chunks should be true right after a real output chunk");
    }

    #[test]
    fn test_has_recent_chunks_false_when_no_chunks_for_2s() {
        let mut s = SilenceState::new();
        s.on_chunk(false, None, true, true, false);
        // Backdate last_chunk_at to 3 seconds ago
        s.last_chunk_at = std::time::Instant::now() - std::time::Duration::from_secs(3);
        assert!(!s.has_recent_chunks(),
            "has_recent_chunks should be false when last chunk was 3s ago");
    }

    #[test]
    fn test_backup_idle_blocked_when_chunks_arriving() {
        use std::sync::atomic::{AtomicU8, AtomicU64};
        let state = crate::state::tests_support::make_test_app_state();
        let sid = "test-session";
        state.shell_states.insert(sid.to_string(), AtomicU8::new(SHELL_BUSY));
        state.session_states.insert(sid.to_string(), crate::state::SessionState::default());

        // last_output_ms is 600ms ago (stale — would normally trigger idle)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap().as_millis() as u64;
        state.last_output_ms.insert(sid.to_string(), AtomicU64::new(now - 600));

        // Any chunk just arrived (real or chrome-only)
        let mut silence = SilenceState::new();
        silence.on_chunk(false, None, false, false, false); // chunk just arrived

        // should_transition_idle says yes (based on last_output_ms alone)
        assert!(should_transition_idle(&state, sid).should_transition,
            "should_transition_idle sees stale last_output_ms");
        // But has_recent_chunks blocks the backup timer (recent chunk activity)
        assert!(silence.has_recent_chunks(),
            "backup idle must be blocked because chunks are arriving");
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
        assert!(silence.has_recent_chunks(),
            "backup idle MUST be blocked when chrome-only ticks are arriving — agent is alive");
    }

    // Status-line idle transition: covered by test_backup_idle_blocked_by_chrome_only_ticks.
    // Status-line ticking proves the agent is alive — the reader thread's !has_status_line
    // guard blocks idle, and has_recent_chunks() (using last_chunk_at) blocks the backup timer.

    #[test]
    fn test_is_spinner_row_distinguishes_spinner_from_static_chrome() {
        // Spinner rows prove agent is alive
        assert!(crate::chrome::is_spinner_row("✻ Cogitated for 3m 47s"));
        assert!(crate::chrome::is_spinner_row("⠋ Generating..."));
        // Static chrome does NOT prove agent is alive
        assert!(!crate::chrome::is_spinner_row("⏵ auto mode"));
        assert!(!crate::chrome::is_spinner_row("▀▀▀▀▀▀▀▀"));
    }

    // --- ChunkProcessor tests ---

    #[test]
    fn test_chunk_processor_new_has_correct_defaults() {
        let cp = ChunkProcessor::new(Some("/home/user/repo".to_string()));
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
    fn test_chunk_processor_dedup_choice_prompt() {
        use crate::state::VtLogBuffer;
        use std::sync::atomic::AtomicU64;

        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let sid = "test-cp-choice-dedup";
        let silence = Arc::new(Mutex::new(SilenceState::new()));
        state.silence_states.insert(sid.to_string(), silence.clone());
        state.shell_states.insert(sid.to_string(), std::sync::atomic::AtomicU8::new(SHELL_NULL));
        state.vt_log_buffers.insert(sid.to_string(), Mutex::new(VtLogBuffer::new(24, 80, 1000)));
        state.output_buffers.insert(sid.to_string(), Mutex::new(OutputRingBuffer::new(4096)));
        state.last_output_ms.insert(sid.to_string(), AtomicU64::new(0));

        let mut cp = ChunkProcessor::new(None);
        let mut utf8_buf = Utf8ReadBuffer::new();
        let mut esc_buf = EscapeAwareBuffer::new();

        // Paint a Claude Code edit-confirm screen into the terminal.
        let screen_bytes =
            b"Do you want to make this edit to CLAUDE.md?\r\n\
              \xe2\x9d\xaf 1. Yes\r\n\
              \x20\x20 2. Yes, allow all edits (shift+tab)\r\n\
              \x20\x20 3. No\r\n\
              \r\n\
              Esc to cancel \xc2\xb7 Tab to amend\r\n";
        let utf8_data = utf8_buf.push(screen_bytes);
        let esc_data = esc_buf.push(&utf8_data);
        let _ = cp.process_chunk(&esc_data, &silence, sid, &state, None);

        // Drain events from the first chunk and count ChoicePrompt emits.
        let mut rx = state.event_bus.subscribe();

        // Second chunk: add an innocuous repaint (cursor home + re-emit same dialog).
        // Same (title, option keys) signature → must be deduped.
        let utf8_data2 = utf8_buf.push(screen_bytes);
        let esc_data2 = esc_buf.push(&utf8_data2);
        let _ = cp.process_chunk(&esc_data2, &silence, sid, &state, None);

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
        assert!(cp.last_choice_prompt_sig.is_some(),
            "signature must be stored after first emission");
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

    // --- transform_xterm tests ---

    #[test]
    fn test_transform_xterm_no_token_passes_through() {
        let mut cp = ChunkProcessor::new(None);
        let result = cp.transform_xterm("just regular output".to_string());
        assert_eq!(result, Some("just regular output".to_string()));
    }

    #[test]
    fn test_transform_xterm_intent_passes_through() {
        // Intent coloring is now handled by the frontend MutationObserver.
        let mut cp = ChunkProcessor::new(None);
        let result = cp.transform_xterm("intent: Fix the bug\n".to_string());
        assert!(result.is_some());
        let data = result.unwrap();
        assert!(data.contains("intent: Fix the bug"), "intent must pass through to frontend");
    }

    #[test]
    fn test_transform_xterm_suggest_passes_through() {
        // Suggest lines are no longer concealed in Rust — the frontend handles it.
        let mut cp = ChunkProcessor::new(None);
        let result = cp.transform_xterm("suggest: A | B | C\n".to_string());
        assert!(result.is_some());
        let data = result.unwrap();
        assert!(data.contains("suggest:"), "suggest must pass through to frontend");
    }

    #[test]
    fn test_transform_xterm_incomplete_intent_passes_through() {
        let mut cp = ChunkProcessor::new(None);
        let r1 = cp.transform_xterm("intent: doing so".to_string());
        assert!(r1.is_some(), "incomplete intent must pass through");
    }

    // --- alt buffer clear injection tests ---

    #[test]
    fn test_transform_xterm_alt_buffer_injects_clear() {
        let mut cp = ChunkProcessor::new(None);
        // Enter alt buffer
        cp.transform_xterm("\x1b[?1049h".to_string());
        assert!(cp.in_alt_buffer);
        // Cursor home should get ESC[2J injected
        let result = cp.transform_xterm("\x1b[Hcontent".to_string()).unwrap();
        assert!(result.contains("\x1b[2J\x1b[H"), "clear should be injected before cursor home");
    }

    #[test]
    fn test_transform_xterm_normal_buffer_no_inject() {
        let mut cp = ChunkProcessor::new(None);
        // NOT in alt buffer — no injection
        let result = cp.transform_xterm("\x1b[Hcontent".to_string()).unwrap();
        assert!(!result.contains("\x1b[2J"), "should not inject clear in normal buffer");
    }

    #[test]
    fn test_transform_xterm_alt_buffer_exit_stops_inject() {
        let mut cp = ChunkProcessor::new(None);
        // Enter then exit alt buffer
        cp.transform_xterm("\x1b[?1049h".to_string());
        cp.transform_xterm("\x1b[?1049l".to_string());
        assert!(!cp.in_alt_buffer);
        let result = cp.transform_xterm("\x1b[Hcontent".to_string()).unwrap();
        assert!(!result.contains("\x1b[2J"), "should not inject after leaving alt buffer");
    }

    #[test]
    fn test_transform_xterm_alt_buffer_no_clear_on_subsequent_redraws() {
        let mut cp = ChunkProcessor::new(None);
        // Enter alt buffer — first cursor-home gets clear
        cp.transform_xterm("\x1b[?1049h".to_string());
        let r1 = cp.transform_xterm("\x1b[Hfirst redraw".to_string()).unwrap();
        assert!(r1.contains("\x1b[2J"), "first redraw must inject clear");

        // Subsequent redraws must NOT inject clear (prevents per-keystroke flicker)
        let r2 = cp.transform_xterm("\x1b[Hsecond redraw".to_string()).unwrap();
        assert!(!r2.contains("\x1b[2J"), "subsequent redraws must not inject clear");
    }

    #[test]
    fn test_transform_xterm_alt_buffer_clear_on_shrink() {
        let mut cp = ChunkProcessor::new(None);
        // Enter alt buffer, consume initial clear
        cp.transform_xterm("\x1b[?1049h".to_string());
        cp.transform_xterm("\x1b[Hinit".to_string()); // consumes one-shot

        // Simulate growing content: cursor-up 50 lines
        cp.transform_xterm("\x1b[50A redraw tall".to_string());
        assert_eq!(cp.last_cursor_up_n, 50);

        // Simulate shrink: cursor-up only 20 lines (content got shorter)
        let r = cp.transform_xterm("\x1b[20A\x1b[Hredraw short".to_string()).unwrap();
        assert!(r.contains("\x1b[2J"), "clear must be injected when content shrinks");
        assert_eq!(cp.last_cursor_up_n, 20);

        // Next redraw at same height — no clear
        let r2 = cp.transform_xterm("\x1b[20A\x1b[Hsame height".to_string()).unwrap();
        assert!(!r2.contains("\x1b[2J"), "no clear when height stays same");
    }

    #[test]
    fn test_extract_largest_cursor_up() {
        assert_eq!(extract_largest_cursor_up("\x1b[5A"), Some(5));
        assert_eq!(extract_largest_cursor_up("\x1b[10Afoo\x1b[3A"), Some(10));
        assert_eq!(extract_largest_cursor_up("no cursor up here"), None);
        assert_eq!(extract_largest_cursor_up("\x1b[H"), None); // cursor home, not up
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
        assert_eq!(found, vec![
            "ESC[2J (Clear Screen)",
            "ESC[H (Cursor Home)",
            "ESC[3J (Clear Scrollback)",
        ]);
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

    // --- clamp_cursor_up tests ---

    #[test]
    fn clamp_cursor_up_no_sequences() {
        assert_eq!(clamp_cursor_up("hello world", 24), "hello world");
    }

    #[test]
    fn clamp_cursor_up_small_n_unchanged() {
        // ESC[5A with viewport=24 → unchanged
        assert_eq!(clamp_cursor_up("\x1b[5A", 24), "\x1b[5A");
    }

    #[test]
    fn clamp_cursor_up_large_n_clamped() {
        // ESC[500A with viewport=24 → ESC[24A
        assert_eq!(clamp_cursor_up("\x1b[500A", 24), "\x1b[24A");
    }

    #[test]
    fn clamp_cursor_up_bare_a() {
        // ESC[A (no number, means 1) → ESC[1A
        assert_eq!(clamp_cursor_up("\x1b[A", 24), "\x1b[1A");
    }

    #[test]
    fn clamp_cursor_up_f_sequence() {
        // ESC[300F (Cursor Previous Line) clamped to viewport
        assert_eq!(clamp_cursor_up("\x1b[300F", 30), "\x1b[30F");
    }

    #[test]
    fn clamp_cursor_up_preserves_other_sequences() {
        // ESC[10B (cursor down), ESC[2J (clear screen) — left untouched
        let input = "\x1b[10B\x1b[2J\x1b[100Ahello";
        let result = clamp_cursor_up(input, 24);
        assert_eq!(result, "\x1b[10B\x1b[2J\x1b[24Ahello");
    }

    #[test]
    fn clamp_cursor_up_multiple_sequences() {
        let input = "before\x1b[200Amiddle\x1b[5Aend";
        let result = clamp_cursor_up(input, 30);
        assert_eq!(result, "before\x1b[30Amiddle\x1b[5Aend");
    }

    #[test]
    fn clamp_cursor_up_exact_viewport() {
        // n == viewport rows → unchanged
        assert_eq!(clamp_cursor_up("\x1b[24A", 24), "\x1b[24A");
    }

    #[test]
    fn clamp_cursor_up_preserves_utf8_multibyte() {
        // Box-drawing characters (3-byte UTF-8) and emoji (4-byte UTF-8)
        let input = "├── hello 🦀 ─── end";
        assert_eq!(clamp_cursor_up(input, 24), input);
    }

    #[test]
    fn clamp_cursor_up_utf8_with_sequences() {
        // Mix of UTF-8 text + ANSI cursor-up sequences
        let input = "├──\x1b[100A🦀──";
        let result = clamp_cursor_up(input, 10);
        assert_eq!(result, "├──\x1b[10A🦀──");
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
        assert!(super::is_wsl_shell("wsl.exe --distribution Debian -- /bin/zsh"));
    }

    #[test]
    fn is_wsl_shell_full_path() {
        assert!(super::is_wsl_shell("C:\\Windows\\System32\\wsl.exe"));
        assert!(super::is_wsl_shell("C:\\Windows\\System32\\wsl.exe -d Ubuntu"));
    }

    #[test]
    fn is_wsl_shell_non_wsl() {
        assert!(!super::is_wsl_shell("powershell.exe"));
        assert!(!super::is_wsl_shell("/bin/zsh"));
        assert!(!super::is_wsl_shell("cmd.exe"));
        assert!(!super::is_wsl_shell("wslconfig.exe"));
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
        assert_eq!(super::windows_to_wsl_path("C:\\Users\\foo\\repos"), "/mnt/c/Users/foo/repos");
    }

    #[test]
    fn wsl_path_drive_letter_forward_slash() {
        assert_eq!(super::windows_to_wsl_path("C:/Users/foo/repos"), "/mnt/c/Users/foo/repos");
    }

    #[test]
    fn wsl_path_lowercase_drive() {
        assert_eq!(super::windows_to_wsl_path("d:\\work"), "/mnt/d/work");
    }

    #[test]
    fn wsl_path_already_linux() {
        assert_eq!(super::windows_to_wsl_path("/home/user/repos"), "/home/user/repos");
    }

    #[test]
    fn wsl_path_unc_unchanged() {
        // UNC paths are not drive-letter paths — returned as-is
        assert_eq!(super::windows_to_wsl_path("\\\\server\\share"), "\\\\server\\share");
    }

    #[test]
    fn wsl_path_root_drive() {
        assert_eq!(super::windows_to_wsl_path("C:\\"), "/mnt/c/");
    }

    // ---- Layer 3: state_change auto-notifications (#1164-2571) ----

    #[test]
    fn mark_session_exited_pushes_state_change_to_parent_inbox() {
        let state = crate::state::tests_support::make_test_app_state();
        let child_id = "child-sess";
        let parent_id = "parent-sess";

        // Register parent-child relationship
        state.session_parent.insert(child_id.to_string(), parent_id.to_string());
        // Pre-init parent inbox
        state.agent_inbox.entry(parent_id.to_string()).or_default();

        mark_session_exited(child_id, &state);

        let inbox = state.agent_inbox.get(parent_id).expect("parent inbox must exist");
        assert!(!inbox.is_empty(), "parent inbox must have received state_change message");
        let msg = inbox.front().unwrap();
        let content: serde_json::Value = serde_json::from_str(&msg.content).expect("content must be valid JSON");
        assert_eq!(content["type"], "state_change");
        assert_eq!(content["state"], "exited");
    }

    #[test]
    fn try_shell_transition_busy_to_idle_pushes_state_change_to_parent_inbox() {
        let state = crate::state::tests_support::make_test_app_state();
        let child_id = "child-idle-sess";
        let parent_id = "parent-idle-sess";

        state.session_parent.insert(child_id.to_string(), parent_id.to_string());
        state.agent_inbox.entry(parent_id.to_string()).or_default();
        // Must have a session_state with agent_type to qualify for idle notification
        let mut ss = crate::state::SessionState::default();
        ss.agent_type = Some("claude".to_string());
        state.session_states.insert(child_id.to_string(), ss);
        state.shell_states.insert(child_id.to_string(), std::sync::atomic::AtomicU8::new(SHELL_BUSY));

        let transitioned = try_shell_transition(&state, child_id, SHELL_BUSY, SHELL_IDLE, true);
        assert!(transitioned, "transition must succeed");

        let inbox = state.agent_inbox.get(parent_id).expect("parent inbox must exist");
        assert!(!inbox.is_empty(), "parent inbox must have received state_change message");
        let msg = inbox.front().unwrap();
        let content: serde_json::Value = serde_json::from_str(&msg.content).expect("content must be valid JSON");
        assert_eq!(content["type"], "state_change");
        assert_eq!(content["state"], "idle");
    }

    #[test]
    fn try_shell_transition_non_agent_session_does_not_push_idle_notification() {
        let state = crate::state::tests_support::make_test_app_state();
        let child_id = "non-agent-sess";
        let parent_id = "parent-non-agent-sess";

        state.session_parent.insert(child_id.to_string(), parent_id.to_string());
        state.agent_inbox.entry(parent_id.to_string()).or_default();
        // No agent_type set — plain shell session
        state.session_states.insert(child_id.to_string(), crate::state::SessionState::default());
        state.shell_states.insert(child_id.to_string(), std::sync::atomic::AtomicU8::new(SHELL_BUSY));

        try_shell_transition(&state, child_id, SHELL_BUSY, SHELL_IDLE, true);

        let inbox = state.agent_inbox.get(parent_id).unwrap();
        assert!(inbox.is_empty(), "non-agent sessions must not send idle notifications to parent");
    }

    #[test]
    fn try_shell_transition_exit_path_does_not_push_idle_to_parent() {
        // notify_parent=false (exit path): orchestrator must NOT receive spurious "idle"
        // before the "exited" message from mark_session_exited.
        let state = crate::state::tests_support::make_test_app_state();
        let child_id = "child-exit-path";
        let parent_id = "parent-exit-path";

        state.session_parent.insert(child_id.to_string(), parent_id.to_string());
        state.agent_inbox.entry(parent_id.to_string()).or_default();
        let mut ss = crate::state::SessionState::default();
        ss.agent_type = Some("claude".to_string());
        state.session_states.insert(child_id.to_string(), ss);
        state.shell_states.insert(child_id.to_string(), std::sync::atomic::AtomicU8::new(SHELL_BUSY));

        let transitioned = try_shell_transition(&state, child_id, SHELL_BUSY, SHELL_IDLE, false);
        assert!(transitioned, "transition must succeed");

        let inbox = state.agent_inbox.get(parent_id).unwrap();
        assert!(inbox.is_empty(), "exit path must not push idle notification — mark_session_exited sends exited");
    }

    #[test]
    fn tombstone_transient_cleanup_removes_swarm_maps() {
        // F3: session_parent, shell_state_since_ms, mcp_to_session must all be cleaned on exit.
        let state = crate::state::tests_support::make_test_app_state();
        let sid = "sess-cleanup";
        let mcp_sid = "mcp-sess-cleanup";

        state.session_parent.insert(sid.to_string(), "parent-sess".to_string());
        state.shell_state_since_ms.insert(
            sid.to_string(),
            std::sync::atomic::AtomicU64::new(42),
        );
        state.mcp_to_session.insert(mcp_sid.to_string(), sid.to_string());
        state.session_to_mcp.insert(sid.to_string(), vec![mcp_sid.to_string()]);

        tombstone_transient_cleanup(sid, &state);

        assert!(!state.session_parent.contains_key(sid), "session_parent must be removed");
        assert!(!state.shell_state_since_ms.contains_key(sid), "shell_state_since_ms must be removed");
        assert!(!state.mcp_to_session.contains_key(mcp_sid), "mcp_to_session entry must be removed");
        assert!(!state.session_to_mcp.contains_key(sid), "session_to_mcp entry must be removed");
    }

    #[test]
    fn mark_session_exited_sends_single_exited_notification() {
        // F1/DATA-1: only one state_change("exited") must reach parent inbox on exit.
        // The BUSY→IDLE transition in the exit path uses notify_parent=false, so the
        // orchestrator must never see a spurious "idle" before "exited".
        let state = crate::state::tests_support::make_test_app_state();
        let child_id = "child-exit-dedup";
        let parent_id = "parent-exit-dedup";

        state.session_parent.insert(child_id.to_string(), parent_id.to_string());
        state.agent_inbox.entry(parent_id.to_string()).or_default();
        let mut ss = crate::state::SessionState::default();
        ss.agent_type = Some("claude".to_string());
        state.session_states.insert(child_id.to_string(), ss);
        state.shell_states.insert(child_id.to_string(), std::sync::atomic::AtomicU8::new(SHELL_BUSY));

        // Simulate exit path: transition (notify_parent=false) + mark_session_exited.
        try_shell_transition(&state, child_id, SHELL_BUSY, SHELL_IDLE, false);
        // mark_session_exited needs a sessions entry to attempt exit-code capture
        // (it's OK if there's none — it just skips the exit code).
        push_state_change_to_parent(&state, child_id, serde_json::json!({
            "type": "state_change",
            "state": "exited",
            "session_id": child_id,
            "exit_code": null,
        }));

        let inbox = state.agent_inbox.get(parent_id).expect("parent inbox must exist");
        assert_eq!(inbox.len(), 1, "inbox must have exactly one message");
        let content: serde_json::Value =
            serde_json::from_str(&inbox.front().unwrap().content).unwrap();
        assert_eq!(content["state"], "exited", "the single message must be 'exited'");
    }
}
