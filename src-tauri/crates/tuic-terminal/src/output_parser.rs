pub use crate::ego_question::{
    parse_ego_approval, parse_ego_free_text_question, parse_ego_question,
};
use serde::Serialize;

fn expand_tilde(path: &str) -> String {
    let suffix = if path == "~" {
        ""
    } else if path.starts_with("~/") {
        &path[1..]
    } else {
        return path.to_string();
    };
    dirs::home_dir()
        .map(|home| format!("{}{suffix}", home.display()))
        .unwrap_or_else(|| path.to_string())
}

/// Native ego recovery snapshot. Bounded before deserialization at the OSC boundary.
#[derive(Clone, Debug, Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProviderRecovery {
    pub phase: String,
    pub next_attempt_ms: Option<u64>,
    pub attempt: u32,
}

pub fn parse_provider_recovery(payload: &str) -> Option<ParsedEvent> {
    if payload == "clear" {
        return Some(ParsedEvent::ProviderRecovery { recovery: None });
    }
    if payload.len() > 256 {
        return None;
    }
    // Require null explicitly for retrying, rather than accepting a missing field.
    let value: serde_json::Value = serde_json::from_str(payload).ok()?;
    value.get("next_attempt_ms")?;
    let recovery: ProviderRecovery = serde_json::from_value(value).ok()?;
    let valid = recovery.attempt > 0
        && match recovery.phase.as_str() {
            "waiting" => recovery
                .next_attempt_ms
                .is_some_and(|time| time <= 8_640_000_000_000_000),
            "retrying" => recovery.next_attempt_ms.is_none(),
            _ => false,
        };
    valid.then_some(ParsedEvent::ProviderRecovery {
        recovery: Some(recovery),
    })
}

/// Structured events parsed from PTY output
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type")]
pub enum ParsedEvent {
    #[serde(rename = "provider-recovery")]
    ProviderRecovery { recovery: Option<ProviderRecovery> },
    #[serde(rename = "rate-limit")]
    RateLimit {
        pattern_name: String,
        matched_text: String,
        retry_after_ms: Option<u64>,
    },
    #[serde(rename = "status-line")]
    StatusLine {
        task_name: String,
        full_line: String,
        time_info: Option<String>,
        token_info: Option<String>,
    },
    #[serde(rename = "pr-url")]
    PrUrl {
        number: u32,
        url: String,
        platform: String, // "github" or "gitlab"
    },
    #[serde(rename = "progress")]
    Progress {
        state: u8, // 0=remove, 1=normal, 2=error, 3=indeterminate
        value: u8, // 0-100
    },
    /// Agent is waiting for user input (question, confirmation, menu)
    #[serde(rename = "question")]
    Question {
        prompt_text: String,
        /// True for high-confidence detections (explicit regex match on known patterns).
        /// False for low-confidence detections (silence-based `?` heuristic).
        /// Desktop uses this to skip the "busy = false positive" guard on confident questions.
        confident: bool,
    },
    /// A low-confidence question left the screen without being answered through
    /// a channel that clears it (typed line, choice-prompt key, status line).
    /// Emitted by the silence timer in pty.rs, not by the parser.
    ///
    /// Without it `awaiting_input` survives the rest of the turn: an agent that
    /// answers its own prompt, or a user who picks an option with a bare Enter,
    /// produces no `UserInput`, and an agent whose spinner is not parsed as a
    /// status line produces no clear either. The session then reports "question"
    /// with the prompt long gone from the screen.
    #[serde(rename = "question-cleared")]
    QuestionCleared,
    /// Claude Code usage limit: "You've used X% of your weekly/session limit"
    #[serde(rename = "usage-limit")]
    UsageLimit {
        percentage: u8,
        limit_type: String, // "weekly" or "session"
    },
    /// Claude Code usage exhausted: "You're out of (extra) usage · resets TIME (TZ)"
    #[serde(rename = "usage-exhausted")]
    UsageExhausted {
        /// Raw reset time text (e.g. "8pm (Europe/Madrid)"), None if not present
        reset_time: Option<String>,
    },
    /// Plan file detected in terminal output (e.g. plans/foo.md, .claude/plans/bar.md)
    #[serde(rename = "plan-file")]
    PlanFile { path: String },
    /// User submitted a line of input via the PTY.
    ///
    /// `line` is the absolute prompt row (`history_size + cursor row`, the same
    /// coordinate space as AgentBlock/OSC133) captured at the OSC 7770
    /// `state=busy` transition — i.e. the row the cursor sat on when the agent
    /// resumed work after the user submitted, which is the user's prompt line.
    /// `-1` when the event has no grid context (keystroke-reconstructed path).
    #[serde(rename = "user-input")]
    UserInput { content: String, line: i64 },
    /// API error from an agent (5xx server error, auth failure, etc.)
    #[serde(rename = "api-error")]
    ApiError {
        pattern_name: String,
        matched_text: String,
        error_kind: String, // "server", "auth", "unknown"
    },
    /// Tool-error turn end: agent printed `Error: Exit code N` and the session
    /// went idle (only chrome chunks) without recovering. Fires `playError()`
    /// on the frontend. Emitted by the silence timer in pty.rs, not by parser.
    #[serde(rename = "tool-error")]
    ToolError { matched_text: String },
    /// Agent-declared intent: what the LLM is currently working on.
    /// Emitted via `intent: <text>` or `intent: <text> (<tab title>)`, at the
    /// start of a row or right after the ack sentence (see ACK_SENTENCE_PREFIX).
    #[serde(rename = "intent")]
    Intent {
        text: String,
        /// Optional short title (max ~3 words) for use as tab name
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
    },
    /// Suggested follow-up actions for the user to choose from.
    /// Emitted via `suggest: A | B | C` at column 0.
    #[serde(rename = "suggest")]
    Suggest { items: Vec<String> },
    /// Slash command autocomplete menu detected on bottom screen rows.
    /// Fired when the user types / in an agent TUI and a menu appears.
    #[serde(rename = "slash-menu")]
    SlashMenu { items: Vec<SlashMenuItem> },
    /// Numbered choice dialog rendered by an agent. The selection mode records
    /// whether its labels are direct keys or an Ink menu needs arrow navigation.
    #[serde(rename = "choice-prompt")]
    ChoicePrompt {
        title: String,
        options: Vec<ChoiceOption>,
        #[serde(skip_serializing_if = "Option::is_none")]
        selection_mode: Option<ChoiceSelectionMode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        dismiss_key: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        amend_key: Option<String>,
    },
    /// A previously detected numbered choice dialog is no longer active.
    #[serde(rename = "choice-cleared")]
    ChoiceCleared,
    /// Claude Code sub-task indicator: `›› task · N local agents` or `›› task · 1 bash`.
    /// Count > 0 means the agent has background work in progress; 0 means all sub-tasks finished.
    #[serde(rename = "active-subtasks")]
    ActiveSubtasks {
        count: u32,
        task_type: String, // "local agents", "bash", "background tasks", etc.
    },
    /// Shell activity state derived from PTY output timing.
    /// Emitted by the reader thread on real-output→busy and idle transitions.
    /// The frontend consumes this instead of deriving busy/idle from raw PTY data.
    #[serde(rename = "shell-state")]
    ShellState {
        state: String, // "busy" | "idle"
        #[serde(skip_serializing_if = "Option::is_none")]
        agent_type: Option<String>,
    },
    /// Agent-emitted block boundary via OSC 7770;block=start|end.
    /// Lets AI agents (Claude Code etc.) define their own block regions
    /// independent of shell OSC 133 markers.
    #[serde(rename = "agent-block")]
    AgentBlock {
        action: String, // "start" | "end"
        line: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
    },
    /// Agent failed to start because of a session-id conflict or missing session.
    /// Emitted when Claude Code prints startup errors like
    /// "Session ID X is already in use." or "No conversation found with session ID: X".
    /// The PTY reader consumes this to auto-reset `TUIC_SESSION` so the shell
    /// wrapper's `--session-id` injection stops wedging the terminal across
    /// repeated invocations.
    #[serde(rename = "agent-session-conflict")]
    AgentSessionConflict {
        matched_text: String,
        /// "in-use" (session id already locked) or "not-found" (session id not resumable)
        kind: String,
    },
}

/// Payload for ParsedEvent::ChoicePrompt. Separate struct so it can be reused
/// as a SessionState field without duplicating shape.
#[derive(Clone, Debug, Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ChoicePromptPayload {
    pub title: String,
    pub options: Vec<ChoiceOption>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection_mode: Option<ChoiceSelectionMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dismiss_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amend_key: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ChoiceSelectionMode {
    /// A clipped native form exposes navigation controls, not a complete choice list.
    NavigateOnly,
    NavigateEnter,
    ToggleEnter,
}

/// A single option in a numbered choice dialog (edit-confirm, bash-confirm, etc.).
#[derive(Clone, Debug, Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ChoiceOption {
    /// Keypress the agent expects to select this option (e.g. "1", "2", "3").
    pub key: String,
    /// Primary label shown to the user (e.g. "Yes", "No", "Yes, allow all edits").
    pub label: String,
    /// Whether this option is currently highlighted (cursor position).
    pub highlighted: bool,
    /// Best-effort flag: label contains a "reject/cancel/no" verb.
    pub destructive: bool,
    /// Optional parenthetical hint (e.g. "(shift+tab)"), stripped from label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

/// A single item in a slash command autocomplete menu.
#[derive(Clone, Debug, Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SlashMenuItem {
    pub command: String,
    pub description: String,
    pub highlighted: bool,
}

/// OutputParser: detects structured events in PTY output text.
/// Designed to run in the Rust reader thread, eliminating JS regex overhead.
/// Borrows pre-compiled patterns from lazy_static globals — zero per-instance allocation.
pub struct OutputParser {
    rate_limit_patterns: &'static [RateLimitPattern],
    api_error_patterns: &'static [ApiErrorPattern],
    /// Dedup: last emitted suggest items to suppress re-emission on scroll.
    last_suggest_items: Option<Vec<String>>,
    /// Input-turn epoch whose real working evidence reopened suggest dedup.
    /// A submission alone is insufficient because the previous suggest row can
    /// repaint while it is still visible; fresh work proves a new response has
    /// actually started and may legitimately end with identical suggestions.
    suggest_working_turn_epoch: Option<u64>,
    /// Dedup: last emitted api-error matched text to suppress re-emission
    /// when the same error remains visible in the terminal buffer.
    last_api_error_match: Option<String>,
    /// Dedup: true after first session conflict detected — suppresses re-emission
    /// when the same error text remains visible in the VT log buffer.
    session_conflict_fired: bool,
}

struct RateLimitPattern {
    name: &'static str,
    regex: regex::Regex,
    retry_after_ms: Option<u64>,
    has_retry_capture: bool,
}

struct ApiErrorPattern {
    name: &'static str,
    regex: regex::Regex,
    error_kind: &'static str, // "server", "auth", "unknown"
}

lazy_static::lazy_static! {
    /// Pre-built rate limit patterns — compiled once at first use.
    static ref RATE_LIMIT_PATTERNS: Vec<RateLimitPattern> = build_rate_limit_patterns();
    /// Pre-built API error patterns — compiled once at first use.
    static ref API_ERROR_PATTERNS: Vec<ApiErrorPattern> = build_api_error_patterns();
    /// Claude Code session-id conflict on startup: the value injected via
    /// `--session-id` is already held by another process (or a stale lock).
    static ref SESSION_IN_USE_RE: regex::Regex =
        regex::Regex::new(r"Session ID\s+[0-9a-fA-F-]{8,}\s+is already in use").unwrap();
    /// Claude Code `--resume <id>` against a session file that no longer exists
    /// (wrong config dir, pruned history, etc.).
    static ref SESSION_NOT_FOUND_RE: regex::Regex =
        regex::Regex::new(r"No conversation found with session ID:\s*[0-9a-fA-F-]{8,}").unwrap();
}

impl OutputParser {
    pub fn new() -> Self {
        Self {
            rate_limit_patterns: &RATE_LIMIT_PATTERNS,
            api_error_patterns: &API_ERROR_PATTERNS,
            last_suggest_items: None,
            suggest_working_turn_epoch: None,
            last_api_error_match: None,
            session_conflict_fired: false,
        }
    }

    /// Parse a chunk of PTY output and return any detected events.
    ///
    /// Strips ANSI escape sequences via the vt100 crate before parsing.
    /// Only available in tests — the production pipeline uses [`parse_clean_lines`].
    #[cfg(test)]
    pub fn parse(&mut self, text: &str) -> Vec<ParsedEvent> {
        let mut events = Vec::new();

        // OSC 9;4 progress (cheap byte scan first, no stripping needed)
        if let Some(evt) = parse_osc94(text) {
            events.push(evt);
        }

        // PR/MR URL detection (no stripping needed)
        if let Some(evt) = parse_pr_url(text) {
            events.push(evt);
        }

        // Strip ANSI escape sequences for parsers that need clean text.
        // Uses the vt100 crate (already a dependency) to render a virtual screen
        // and extract clean rows.
        let clean = strip_ansi_via_vt100(text);

        // Status line detection
        if let Some(evt) = parse_status_line(&clean) {
            events.push(evt);
        }

        // Active sub-task indicator (Claude Code ›› lines)
        if let Some(evt) = parse_active_subtasks(&clean) {
            events.push(evt);
        }

        // Rate limit detection (operates on clean text to avoid ANSI escapes bridging unrelated tokens)
        if let Some(evt) = self.parse_rate_limit(&clean) {
            events.push(evt);
        }

        // API error detection (5xx, auth errors — distinct from rate limits)
        if let Some(evt) = self.parse_api_error(&clean) {
            events.push(evt);
        }

        // Usage limit detection
        if let Some(evt) = parse_usage_limit(&clean) {
            events.push(evt);
        }

        // Usage exhaustion detection
        if let Some(evt) = parse_usage_exhausted(&clean) {
            events.push(evt);
        }

        // Question/attention detection
        if let Some(evt) = parse_question(&clean) {
            events.push(evt);
        }

        // Plan file detection
        if let Some(evt) = parse_plan_file(&clean) {
            events.push(evt);
        }

        // Suggest follow-up actions: `suggest: A | B | C` at column 0
        if let Some(evt) = parse_suggest(&clean, true) {
            events.push(evt);
        }

        if !self.session_conflict_fired
            && let Some(evt) = parse_agent_session_conflict(&clean)
        {
            self.session_conflict_fired = true;
            events.push(evt);
        }

        events
    }

    /// Parse already-clean VtLogBuffer rows and return any detected events.
    ///
    /// Unlike [`parse`], this method receives rows that have already been rendered
    /// by the VT100 parser — no ANSI stripping is needed.  Each row is processed
    /// independently so parsers see a single, stable line rather than a raw chunk
    /// that may span cursor movements.
    ///
    /// OSC 9;4 progress events are NOT emitted here: those sequences are consumed
    /// by the vt100 crate and invisible in clean rows; they remain on the raw stream.
    pub fn parse_clean_lines(
        &mut self,
        rows: &[crate::vt_log::ChangedRow],
        agent_active: bool,
    ) -> Vec<ParsedEvent> {
        self.parse_clean_lines_with_suggest_floor(rows, agent_active, None)
    }

    /// Keep historical suggestions above a submitted composer out of this turn
    /// without suppressing unrelated event types from those same changed rows.
    pub fn parse_clean_lines_with_suggest_floor(
        &mut self,
        rows: &[crate::vt_log::ChangedRow],
        agent_active: bool,
        suggest_floor: Option<usize>,
    ) -> Vec<ParsedEvent> {
        let mut events = Vec::new();
        // Join rows into a single string so multi-line parsers (rate_limit, etc.) work.
        // Individual row texts are already clean — no ANSI stripping required.
        // Strip backtick characters: Claude Code renders file paths and tokens as
        // markdown inline code (`path`), which leaves literal backticks in the clean
        // text. Removing them lets all parsers match paths/tokens without needing
        // backtick-aware regexes — plugins benefit too via structured events.
        let mut joined = String::new();
        for (i, r) in rows.iter().enumerate() {
            if i > 0 {
                joined.push('\n');
            }
            if r.text.contains('`') {
                for ch in r.text.chars() {
                    if ch != '`' {
                        joined.push(ch);
                    }
                }
            } else {
                joined.push_str(&r.text);
            }
        }

        // PR/MR URL detection (operates on text directly)
        if let Some(evt) = parse_pr_url(&joined) {
            events.push(evt);
        }

        // Status line — iterates lines internally
        if let Some(evt) = parse_status_line(&joined) {
            events.push(evt);
        }

        // Active sub-task indicator (Claude Code ›› lines)
        if let Some(evt) = parse_active_subtasks(&joined) {
            events.push(evt);
        }

        // Rate limit and API error — pattern-match on joined text
        if let Some(evt) = self.parse_rate_limit(&joined) {
            events.push(evt);
        }
        if let Some(evt) = self.parse_api_error(&joined) {
            events.push(evt);
        }

        // Usage limit
        if let Some(evt) = parse_usage_limit(&joined) {
            events.push(evt);
        }

        // Usage exhaustion
        if let Some(evt) = parse_usage_exhausted(&joined) {
            events.push(evt);
        }

        // Question/attention — iterates lines internally
        if let Some(evt) = parse_question(&joined) {
            events.push(evt);
        }

        // Plan file
        if let Some(evt) = parse_plan_file(&joined) {
            events.push(evt);
        }

        // Suggest follow-up actions: `suggest: [ A | B | C ]` on one bounded
        // logical line. The token is fully self-contained (bounded by `[ … ]`),
        // so there is no parser-owned cross-chunk buffer — dedup against the last
        // emission to avoid re-firing while the same suggest stays on screen.
        let current_suggest_rows = suggest_floor.map(|floor| {
            rows.iter()
                .filter(|row| row.row_index > floor)
                .map(|row| row.text.replace('`', ""))
                .collect::<Vec<_>>()
                .join("\n")
        });
        let suggest_text = current_suggest_rows.as_deref().unwrap_or(&joined);
        if let Some((evt, _canonical)) = parse_suggest_with_line(suggest_text, agent_active)
            && let ParsedEvent::Suggest { ref items } = evt
            && self.last_suggest_items.as_ref() != Some(items)
        {
            self.last_suggest_items = Some(items.clone());
            events.push(evt);
        }

        if !self.session_conflict_fired
            && let Some(evt) = parse_agent_session_conflict(&joined)
        {
            self.session_conflict_fired = true;
            events.push(evt);
        }

        events
    }

    /// Whether the existing suggest grammar accepts a complete token.
    /// This deliberately does not touch emission deduplication state.
    pub fn is_complete_suggest(&self, text: &str, agent_active: bool) -> bool {
        parse_suggest_with_line(text, agent_active).is_some()
    }

    /// Reopen the error dedup when the user submits a line: the agent is being
    /// asked again, so a recurrence of the same failure is a NEW failure and
    /// must notify.
    ///
    /// Called by the reader thread, not from `parse_clean_lines`: no parser
    /// here produces `UserInput` — that event is emitted on the input thread by
    /// `record_submitted_line` — so a submission is invisible to this type and
    /// has to be pushed in. `pty.rs` parks the request on `SilenceState` and
    /// `ChunkProcessor::process_chunk` drains it before the next parse.
    ///
    /// `last_suggest_items` is deliberately left alone: the old suggest text is
    /// still on screen and changed-row detection re-parses it once the input
    /// scrolls the viewport, so clearing it here would make stale chips
    /// reappear on the next idle. `begin_suggest_working_turn` reopens that one
    /// on real working evidence instead.
    pub fn reset_input_dedup(&mut self) {
        self.last_api_error_match = None;
        self.session_conflict_fired = false;
    }

    /// Reopen suggest emission once for a turn that has produced real working
    /// evidence. Keeping this separate from user submission prevents a stale
    /// previous-turn row repaint from recreating completed chips.
    pub fn begin_suggest_working_turn(&mut self, turn_epoch: u64) {
        if self.suggest_working_turn_epoch != Some(turn_epoch) {
            self.suggest_working_turn_epoch = Some(turn_epoch);
            self.last_suggest_items = None;
        }
    }

    fn parse_rate_limit(&self, text: &str) -> Option<ParsedEvent> {
        // Fast path: every rate-limit pattern requires at least one of these keywords.
        if !text.contains("rate_limit") && !text.contains("overloaded")
            && !text.contains("RateLimit") && !text.contains("429")
            && !text.contains("RESOURCE_EXHAUSTED") && !text.contains("etry") // Retry/retry
            && !text.contains("Rate Limit") && !text.contains("per minute")
            // Claude Code friendly overload message ("temporarily limiting requests")
            && !text.contains("limiting requests")
        {
            return None;
        }
        for pattern in self.rate_limit_patterns {
            // Use captures() uniformly — group 0 is the full match (subsumes find())
            if let Some(caps) = pattern.regex.captures(text) {
                let m: regex::Match<'_> = caps.get(0).unwrap();
                // Guard: reject matches that appear inside source code or documentation.
                // Real API errors appear on their own line (e.g. "Error: rate_limit_error"),
                // not inside string literals, comments, regex patterns, or test assertions.
                if line_is_code_or_diff(line_containing_match(text, m.start())) {
                    continue;
                }

                let retry_after_ms = if pattern.has_retry_capture {
                    caps.get(1)
                        .and_then(|g| g.as_str().parse::<u64>().ok().map(|s| s * 1000))
                } else {
                    pattern.retry_after_ms
                };
                return Some(ParsedEvent::RateLimit {
                    pattern_name: pattern.name.to_string(),
                    matched_text: m.as_str().to_string(),
                    retry_after_ms: retry_after_ms.or(Some(60000)),
                });
            }
        }
        None
    }

    fn parse_api_error(&mut self, text: &str) -> Option<ParsedEvent> {
        // Fast path: every api-error pattern requires at least one of these keywords.
        if !text.contains("api_error")
            && !text.contains("API Error:")
            && !text.contains("■ ")
            && !text.contains("εgo: the provider returned HTTP")
            && !text.contains("Invalid API key")
            && !text.contains("Invalid auth token")
            && !text.contains("Authentication required")
            && !text.contains("authentication_error")
            && !text.contains("server_error")
            && !text.contains("UNAVAILABLE")
            && !text.contains("INTERNAL")
            && !text.contains("UNAUTHENTICATED")
            && !text.contains("litellm")
            && !text.contains("copilot")
            && !text.contains("provider_name")
            && !text.contains("base_resp")
            && !text.contains("stream error")
            && !text.contains("servers are down")
            && !text.contains("not able to authenticate")
            && !text.contains("request failed")
        {
            return None;
        }
        for pattern in self.api_error_patterns {
            if let Some(m) = pattern.regex.find(text) {
                // Guard: reject matches inside source code or documentation.
                if line_is_code_or_diff(line_containing_match(text, m.start())) {
                    continue;
                }
                // Dedup: suppress re-emission when the same error text is still
                // visible in the terminal buffer (e.g. prompt redraws, user typing).
                let matched = m.as_str();
                if self.last_api_error_match.as_deref() == Some(matched) {
                    return None;
                }
                self.last_api_error_match = Some(matched.to_string());
                return Some(ParsedEvent::ApiError {
                    pattern_name: pattern.name.to_string(),
                    matched_text: matched.to_string(),
                    error_kind: pattern.error_kind.to_string(),
                });
            }
        }
        None
    }
}

impl Default for OutputParser {
    fn default() -> Self {
        Self::new()
    }
}

/// Returns true if a line looks like source code, documentation, or agent commentary
/// Combined guard: returns true if a line looks like source code, diff output,
/// or documentation context — not a real agent output line.
/// Extract the line containing a regex match from the full text.
fn line_containing_match(text: &str, match_start: usize) -> &str {
    let line = text[..match_start]
        .rfind('\n')
        .map(|nl| &text[nl + 1..])
        .unwrap_or(text);
    line.lines().next().unwrap_or(line)
}

fn line_is_code_or_diff(line: &str) -> bool {
    line_is_source_code(line) || line_is_diff_or_code_context(line)
}

/// Detect whether a line looks like source code rather than a real API error or
/// session conflict. Prevents false-positive detection when agents read/discuss
/// source files that contain error-code strings.
fn line_is_source_code(line: &str) -> bool {
    let trimmed = line.trim();
    // Rust raw string literals: r"...", r#"..."# (must be preceded by whitespace or line start)
    if trimmed.contains("r#\"") {
        return true;
    }
    // r" preceded by whitespace/punctuation (not alphanumeric) — avoids matching "error", "server", etc.
    if let Some(idx) = trimmed.find("r\"")
        && (idx == 0 || !trimmed.as_bytes()[idx - 1].is_ascii_alphanumeric())
    {
        return true;
    }
    // Line comments (Rust, JS, Python, shell)
    if trimmed.starts_with("//") || trimmed.starts_with('#') {
        return true;
    }
    // Function/const/let/var/test declarations (code context)
    if trimmed.starts_with("fn ")
        || trimmed.starts_with("const ")
        || trimmed.starts_with("let ")
        || trimmed.starts_with("rl(")
        || trimmed.starts_with("assert")
    {
        return true;
    }
    // Indented code (4+ leading spaces or tab) with string delimiters around the match
    let leading_ws = line.len() - line.trim_start().len();
    if leading_ws >= 4 && (trimmed.contains('"') || trimmed.contains('\'')) {
        return true;
    }
    // Markdown code fences or bullet points discussing patterns
    if trimmed.starts_with("```") || trimmed.starts_with("- ") || trimmed.starts_with("* ") {
        return true;
    }
    // Line contains pipe characters (markdown tables, test output)
    if trimmed.contains(" | ") && trimmed.starts_with('|') {
        return true;
    }
    false
}

fn build_rate_limit_patterns() -> Vec<RateLimitPattern> {
    // Patterns are checked in order; first match wins.
    // Only match structured error output (API error codes, HTTP status lines, error class names).
    // NEVER match plain English phrases — agents discuss rate limits in conversational output.
    vec![
        // Claude: specific API error codes (snake_case identifiers)
        rl(
            "claude-http-429",
            r"(?i)rate_limit_error",
            Some(60000),
            false,
        ),
        rl(
            "claude-overloaded",
            r"(?i)overloaded_error",
            Some(30000),
            false,
        ),
        // Claude Code: user-facing friendly overload message (Claude auto-retries
        // internally; we only record the rate-limit, never inject — see AGENTS.md).
        // Full UI string: "API Error: Server is temporarily limiting requests
        // (not your usage limit) · Rate limited".
        rl(
            "claude-overloaded-friendly",
            r"temporarily limiting requests",
            Some(30000),
            false,
        ),
        // OpenAI / Cursor: specific error class names (PascalCase/structured)
        rl("openai-http-429", r"RateLimitError", Some(60000), false),
        // Cursor: exact API error message emitted by Cursor's backend (not conversational)
        rl(
            "cursor-rate-limit",
            r"User Provided API Key Rate Limit Exceeded",
            Some(60000),
            false,
        ),
        // Gemini: gRPC error code (UPPER_SNAKE_CASE)
        rl(
            "gemini-resource-exhausted",
            r"RESOURCE_EXHAUSTED",
            Some(60000),
            false,
        ),
        // HTTP status line — requires "429" adjacent to HTTP-like context
        // HTTP/ must be followed by a version (e.g. 1.1, 2) then whitespace then 429;
        // HTTP<space>429 (no version) also accepted. Prevents ANSI garbage bridging.
        rl(
            "http-429",
            r"(?i)\b429\b.{0,20}Too Many Requests|HTTP/\d[\d.]*\s+429|HTTP\s+429",
            Some(60000),
            false,
        ),
        // Retry-After HTTP header (colon-separated, very specific format)
        rl(
            "retry-after-header",
            r"(?i)Retry-After:\s*(\d+)",
            None,
            true,
        ),
        // OpenAI structured retry message (requires "Retry after N seconds" exact phrasing)
        rl(
            "openai-retry-after",
            r"Retry after (\d+) seconds?",
            None,
            true,
        ),
        // Token/request limit errors — require structured error context (quotes, colons, or error prefix)
        rl(
            "openai-tpm-limit",
            r"(?i)tokens per minute.*limit|TPM limit",
            Some(60000),
            false,
        ),
        rl(
            "openai-rpm-limit",
            r"(?i)requests per minute.*limit|RPM limit",
            Some(60000),
            false,
        ),
    ]
}

fn rl(
    name: &'static str,
    pattern: &str,
    retry_after_ms: Option<u64>,
    has_retry_capture: bool,
) -> RateLimitPattern {
    RateLimitPattern {
        name,
        regex: regex::Regex::new(pattern).unwrap(),
        retry_after_ms,
        has_retry_capture,
    }
}

fn build_api_error_patterns() -> Vec<ApiErrorPattern> {
    // Patterns for API errors that are NOT rate limits (5xx, auth, server errors).
    // Checked in order; first match wins.
    //
    // Two tiers:
    //   1. Agent-specific patterns (exact CLI output formats)
    //   2. Provider-level patterns (JSON error structures from any CLI that prints raw API responses)
    vec![
        // === Agent-specific patterns ===

        // Claude Code: API Error: 5xx with JSON body
        // Include the native JSON heading when available: a wrapped `error`
        // key can start its continuation with `r":`, which alone resembles
        // a Rust raw string to the code-context guard. The heading preserves
        // the real output context without relaxing that guard for quoted code.
        ae(
            "claude-api-error",
            r#"(?:API Error: 5\d\d \{[^}]*?)?"type":"api_error""#,
            "server",
        ),
        // Claude Code: authentication_error in JSON body (401) — also used by OpenAI
        ae(
            "claude-auth-error",
            r#"(?:API Error: (?:401|403) \{[^}]*?)?"type":"authentication_error""#,
            "auth",
        ),
        // Claude Code: user-facing friendly 5xx message, no JSON body. Full UI string:
        // "API Error: 500 Internal server error. This is a server-side issue, usually
        // temporary — try again in a moment. If it persists, check https://status.claude.com."
        //
        // Anchored on the status code because it opens the message: the prose wraps at
        // the terminal width and rendered rows are joined with '\n', so any phrase
        // further in can be split mid-match. Requiring a letter after the code excludes
        // the JSON-body variants (`API Error: 5xx {"type":…`), which the patterns above
        // classify more precisely — 529 overloaded stays a rate limit, not a server error.
        ae(
            "claude-server-error-friendly",
            r"API Error: 5\d\d [A-Za-z]",
            "server",
        ),
        // Native failure prefixes fit narrow panes; the remaining prose can wrap.
        // Claude 2.1.308 renders either a tool-result marker or an assistant bullet.
        ae(
            "claude-transport-error",
            r"(?m)^\s*(?:[⎿⏺]\s*)?API Error: (?:Connection (?:lost|to the|dropped|refused)\b|Request timed out\b|The response (?:stopped|stalled|stream)\b|Server error\b|Part of the\b|Unable to connect\b)",
            "server",
        ),
        ae(
            "claude-auth-friendly",
            r"(?m)^\s*(?:[⎿⏺]\s*)?(?:Invalid API key\b|Invalid auth token\b|Authentication required\b)",
            "auth",
        ),
        // Codex 0.163.0 only commits non-retrying errors as a native ■ row.
        // Retry notifications update the status indicator instead.
        ae(
            "codex-native-auth",
            r"(?m)^\s*■ (?:unexpected status (?:401|403)\b|exceeded retry limit,\s+last status:\s+(?:401|403)\b)",
            "auth",
        ),
        ae(
            "codex-native-transport",
            r"(?m)^\s*■ (?:stream disconnected\b|request timed out\b|exceeded retry limit,\s+last status:\s+5\d\d\b|unexpected status 5\d\d\b)",
            "server",
        ),
        // ego may refuse authentication before opening a turn (no final OSC).
        ae(
            "ego-startup-auth",
            r"(?m)^εgo: the provider returned HTTP (?:401|403)\b",
            "auth",
        ),
        // Gemini CLI: API Error: got status: UNAVAILABLE/INTERNAL
        ae(
            "gemini-server-error",
            r"API Error: got status: (?:UNAVAILABLE|INTERNAL)",
            "server",
        ),
        // LiteLLM provider exceptions (shared by CLI clients)
        ae(
            "provider-server-error",
            r"litellm\.(?:InternalServerError|ServiceUnavailableError|APIError):",
            "server",
        ),
        ae(
            "provider-auth-error",
            r"litellm\.AuthenticationError:",
            "auth",
        ),
        // Provider-facing service and authentication messages
        ae(
            "provider-server-msg",
            r"The API provider's servers are down or overloaded",
            "server",
        ),
        ae(
            "provider-auth-msg",
            r"The API provider is not able to authenticate you",
            "auth",
        ),
        // Codex CLI: stream error with retry exhaustion (non-429 status)
        ae(
            "codex-stream-error",
            r"stream error: exceeded retry limit,\s+last\s+status:\s+[45]\d\d",
            "server",
        ),
        // Copilot CLI: token/auth failures
        // Note: "request failed unexpectedly" removed — too generic, triggers on Claude output
        ae(
            "copilot-auth-error",
            r"(?:Failed to get copilot token|copilot token.*expired)",
            "auth",
        ),
        // === Provider-level JSON error patterns ===
        // These fire when any CLI prints the raw API error response.

        // OpenAI: {"error":{"type":"server_error",...}}
        ae(
            "openai-server-error",
            r#""type"\s*:\s*"server_error""#,
            "server",
        ),
        // Google Gemini/Vertex: {"error":{"status":"INTERNAL"}} or "UNAVAILABLE"
        ae(
            "google-api-server",
            r#""status"\s*:\s*"(?:INTERNAL|UNAVAILABLE)""#,
            "server",
        ),
        // Google auth: {"error":{"status":"UNAUTHENTICATED"}} or API_KEY_INVALID
        ae(
            "google-api-auth",
            r#""status"\s*:\s*"UNAUTHENTICATED""#,
            "auth",
        ),
        // OpenRouter: error JSON with provider_name metadata and error code
        ae(
            "openrouter-server",
            r#""error"\s*:\s*\{[^}]*"provider_name"\s*:"#,
            "server",
        ),
        // MiniMax: {"base_resp":{"status_code":1013,...}} — non-zero status_code indicates error
        ae(
            "minimax-server",
            r#""base_resp"\s*:\s*\{[^}]*"status_code"\s*:\s*[1-9]"#,
            "server",
        ),
    ]
}

fn ae(name: &'static str, pattern: &str, error_kind: &'static str) -> ApiErrorPattern {
    ApiErrorPattern {
        name,
        regex: regex::Regex::new(pattern).unwrap(),
        error_kind,
    }
}

/// Parse OSC 9;4 progress sequences: \x1b]9;4;STATE;VALUE\x07
pub fn parse_osc94(text: &str) -> Option<ParsedEvent> {
    // Fast path: check for ESC ] before running regex
    if !text.contains("\x1b]9;4;") {
        return None;
    }
    lazy_static::lazy_static! {
        static ref OSC94_RE: regex::Regex =
            regex::Regex::new(r"\x1b\]9;4;(\d);(\d{1,3})(?:\x07|\x1b\\)").unwrap();
    }
    OSC94_RE.captures_iter(text).last().map(|caps| {
        let state: u8 = caps[1].parse().unwrap_or(0);
        let value: u8 = caps[2].parse().unwrap_or(0).min(100);
        ParsedEvent::Progress { state, value }
    })
}

/// Parse response-required OSC 777 desktop notifications:
/// `\x1b]777;notify;TITLE;BODY\x07`.
///
/// OSC 777 describes a desktop notification, not an awaiting-state transition.
/// Claude also emits `Claude Code needs your attention` and `Claude is waiting
/// for your input` after a completed turn at its ready composer. Neither body
/// proves a question is on screen; promoting the latter flashed an awaiting
/// badge until the silence timer retracted it. Ink/choice picker screen signals
/// and OSC 7770 `state=awaiting` cover actual dialogs. Only unambiguous
/// permission and approval wording becomes a confident `Question` here.
///
/// The `prompt_text` is the body when present, else the title.
pub fn parse_osc777_notifies(text: &str) -> Vec<ParsedEvent> {
    // Fast path: skip the regex unless the introducer is present.
    if !text.contains("\x1b]777;notify;") {
        return Vec::new();
    }
    lazy_static::lazy_static! {
        // Terminated by BEL or ST. Field bodies stop at `;`, BEL or ESC so a
        // truncated sequence at a chunk boundary cannot swallow later output.
        static ref OSC777_RE: regex::Regex =
            regex::Regex::new(r"\x1b\]777;notify;([^;\x07\x1b]*)(?:;([^\x07\x1b]*))?(?:\x07|\x1b\\)")
                .unwrap();
    }
    OSC777_RE
        .captures_iter(text)
        .filter_map(|caps| {
            let title = caps.get(1).map(|m| m.as_str().trim()).unwrap_or_default();
            let body = caps.get(2).map(|m| m.as_str().trim()).unwrap_or_default();
            let prompt_text = if body.is_empty() { title } else { body };
            if prompt_text.is_empty() {
                return None;
            }
            // ProviderTurnNotices emits this suffix only for an unsuccessful
            // Provider termination, never for a retry or a cancelled turn.
            if title == "εgo"
                && body.ends_with("; review the output, then send a new prompt to continue")
            {
                return Some(ParsedEvent::ApiError {
                    pattern_name: "ego-provider-failure".into(),
                    matched_text: body.into(),
                    error_kind: if body.contains("HTTP 401") || body.contains("HTTP 403") {
                        "auth"
                    } else {
                        "server"
                    }
                    .into(),
                });
            }
            let normalized = prompt_text.to_ascii_lowercase();
            let confident = normalized.contains("needs your permission")
                || normalized.contains("approval required");
            confident.then(|| ParsedEvent::Question {
                prompt_text: prompt_text.to_string(),
                confident,
            })
        })
        .collect()
}

/// Compatibility helper for callers that classify one notification. When a
/// buffer contains several, the newest qualifying event retains the historical
/// single-result behavior; production uses `parse_osc777_notifies` and keeps all,
/// so this exists for tests only.
pub fn parse_osc777_notify(text: &str) -> Option<ParsedEvent> {
    parse_osc777_notifies(text).pop()
}

/// Parse GitHub/GitLab PR/MR URLs
fn parse_pr_url(text: &str) -> Option<ParsedEvent> {
    // Fast path
    if !text.contains("github.com") && !text.contains("gitlab.com") {
        return None;
    }
    lazy_static::lazy_static! {
        static ref GH_RE: regex::Regex =
            regex::Regex::new(r"https?://github\.com/[^/\s]+/[^/\s]+/pull/(\d+)").unwrap();
        static ref GL_RE: regex::Regex =
            regex::Regex::new(r"https?://gitlab\.com/[^/\s]+/[^/\s]+/-/merge_requests/(\d+)").unwrap();
    }
    if let Some(caps) = GH_RE.captures(text) {
        let number: u32 = caps[1].parse().unwrap_or(0);
        return Some(ParsedEvent::PrUrl {
            number,
            url: caps[0].to_string(),
            platform: "github".to_string(),
        });
    }
    if let Some(caps) = GL_RE.captures(text) {
        let number: u32 = caps[1].parse().unwrap_or(0);
        return Some(ParsedEvent::PrUrl {
            number,
            url: caps[0].to_string(),
            platform: "gitlab".to_string(),
        });
    }
    None
}

/// Strip ANSI escape sequences from raw PTY text.
///
/// Uses the shared VTE 0.15 stripper to remove CSI/OSC/ESC sequences.
/// Only used by the test-only [`OutputParser::parse`] method.
#[cfg(test)]
fn strip_ansi_via_vt100(text: &str) -> String {
    let stripped = tuic_ansi::strip(text);
    String::from_utf8_lossy(&stripped)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Byte-level rejection for [`parse_status_line`]: can this blob contain ANY
/// status-line marker at all?
///
/// Checks for ASCII markers and the UTF-8 lead bytes of supported spinner glyphs.
/// It is not selective on agent output: box drawing also uses 0xE2. Per-line
/// dispatch in [`status_line_patterns_for`] rejects unrelated rows.
fn status_line_fast_path_admits(clean: &str) -> bool {
    clean.contains('*')
        || clean.contains("[Running]")
        || clean.contains("Ctrl+C")
        || clean.as_bytes().contains(&0xe2)
        || clean.as_bytes().contains(&0xc2)
}

lazy_static::lazy_static! {
    // All status-line patterns are anchored to ^\s* because agent status lines
    // are always the first thing on the visible line (agents use \r to overwrite).
    // Without anchoring, `*` in code/output would false-positive.

    // Claude Code: "* Task name... (time)" or "✢Task name… (time)" or "· Verb…"
    // Accepts ASCII *, middle dot · (U+00B7), and dingbat asterisks U+2720-273F.
    static ref CLAUDE_STATUS_RE: regex::Regex =
        regex::Regex::new(r"^\s*[*\u{00B7}\u{2720}-\u{273F}]\s*([^.…\n]+)(?:\.{2,3}|…)").unwrap();
    // "[Running] Task name"
    static ref RUNNING_STATUS_RE: regex::Regex =
        regex::Regex::new(r"(?i)^\s*\[Running\]\s+(.+)").unwrap();
    // Braille spinner: "⠋ Task name" (Gemini CLI dots, generic)
    static ref SPINNER_STATUS_RE: regex::Regex =
        regex::Regex::new(r"^\s*[⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏]\s+([^.…]+)").unwrap();
    // Codex CLI bullet spinner: "• Working (5s • esc to interrupt)" or "◦ Working (12s)"
    // • (U+2022) and ◦ (U+25E6) alternate as a blink spinner.
    // Requires parenthesized time suffix to avoid false positives from markdown bullets.
    static ref CODEX_BULLET_RE: regex::Regex =
        regex::Regex::new(r"^\s*[\u{2022}\u{25E6}]\s+(\w[^(]*?)\s*\(\d+[smh]").unwrap();
    // Goose CLI spinner: "Honking thoughtfully...  (Ctrl+C to interrupt)"
    // Random whimsical messages followed by "..." and "(Ctrl+C to interrupt)" suffix.
    static ref GOOSE_SPINNER_RE: regex::Regex =
        regex::Regex::new(r"^\s*([A-Z][^.…\n]{2,}?)\.{2,3}\s+\(Ctrl\+C to interrupt\)").unwrap();
    // Copilot CLI indicators: ∴ (U+2234 thinking), ● (U+25CF active), ○ (U+25CB queued)
    // Format: "∴ Thinking…" or "● Read file..." or "○ Verify..."
    static ref COPILOT_STATUS_RE: regex::Regex =
        regex::Regex::new(r"^\s*[\u{2234}\u{25CF}\u{25CB}]\s+([^.…\n]+)(?:\.{2,3}|…)").unwrap();
    // Time info — captures compound times like "4m 55s" or "1m32s"
    static ref TIME_RE: regex::Regex =
        regex::Regex::new(r"\((\d+[smh](?:\s*\d+[smh])*)").unwrap();
    // Token info
    static ref TOKEN_RE: regex::Regex =
        regex::Regex::new(r"(?i)(\d+(?:[.,]\d+)?k?\s*tokens)").unwrap();
}

/// Which status patterns a line's leading glyph can possibly match.
///
/// Every pattern is anchored `^\s*` followed by a fixed character class, so
/// the first non-whitespace character decides the candidates. The flat table
/// ran all six regexes per line; on an agent screen the byte fast path admits
/// nearly every chunk, so that was six matches per changed row, per chunk.
///
/// Order inside a bucket is the order the flat table had — the first match
/// wins. `trim_start` and the regex
/// `\s` agree on what whitespace is: both are Unicode `White_Space`.
fn status_line_patterns_for(lead: char) -> &'static [&'static regex::Regex] {
    lazy_static::lazy_static! {
        static ref CLAUDE_ONLY: Vec<&'static regex::Regex> = vec![&CLAUDE_STATUS_RE];
        static ref RUNNING_ONLY: Vec<&'static regex::Regex> = vec![&RUNNING_STATUS_RE];
        static ref GOOSE_ONLY: Vec<&'static regex::Regex> = vec![&GOOSE_SPINNER_RE];
        static ref COPILOT_ONLY: Vec<&'static regex::Regex> = vec![&COPILOT_STATUS_RE];
        static ref CODEX_ONLY: Vec<&'static regex::Regex> = vec![&CODEX_BULLET_RE];
        static ref BRAILLE_ONLY: Vec<&'static regex::Regex> = vec![&SPINNER_STATUS_RE];
    }
    match lead {
        '*' | '\u{00B7}' | '\u{2720}'..='\u{273F}' => &CLAUDE_ONLY,
        '[' => &RUNNING_ONLY,
        'A'..='Z' => &GOOSE_ONLY,
        '\u{2234}' | '\u{25CF}' | '\u{25CB}' => &COPILOT_ONLY,
        '\u{2022}' | '\u{25E6}' => &CODEX_ONLY,
        '\u{280B}' | '\u{2819}' | '\u{2839}' | '\u{2838}' | '\u{283C}' | '\u{2834}'
        | '\u{2826}' | '\u{2827}' | '\u{2807}' | '\u{280F}' => &BRAILLE_ONLY,
        _ => &[],
    }
}

/// Parse status line patterns from pre-stripped terminal output.
fn parse_status_line(clean: &str) -> Option<ParsedEvent> {
    if !status_line_fast_path_admits(clean) {
        return None;
    }

    for line in clean.lines() {
        let trimmed = line.trim();
        // Skip lines that look like diff output, code, or documentation
        if line_is_diff_or_code_context(line) {
            continue;
        }
        // Skip C-style block comment lines (/* ... */ or lines ending with */)
        if trimmed.starts_with("/*") || trimmed.ends_with("*/") {
            continue;
        }

        let Some(lead) = line.chars().find(|c| !c.is_whitespace()) else {
            continue;
        };
        for pattern in status_line_patterns_for(lead) {
            if let Some(caps) = pattern.captures(line) {
                let task_name = caps[1].trim().to_string();
                if task_name.len() < 3 {
                    continue;
                }
                // Reject task names containing code/data artifacts — real agent
                // status lines are natural language (e.g. "Reading files").
                if task_name.contains('[')
                    || task_name.contains(']')
                    || task_name.contains('"')
                    || task_name.contains('\'')
                    || task_name.contains('|')
                    || task_name.contains('{')
                    || task_name.contains('}')
                    || task_name.contains('\\')
                    || task_name.contains('/')
                {
                    continue;
                }
                let time_info = TIME_RE.captures(line).map(|c| c[1].to_string());
                let token_info = TOKEN_RE.captures(line).map(|c| c[1].to_string());
                return Some(ParsedEvent::StatusLine {
                    task_name,
                    full_line: line.trim().to_string(),
                    time_info,
                    token_info,
                });
            }
        }
    }
    None
}

/// Detect Claude Code active sub-task indicators from the mode line.
///
/// Supports all observed formats (order-agnostic):
/// 1. Old: `⏵⏵ <mode> · N <type>` — markers first, count last
/// 2. New: `N <type> · ⏵⏵ <mode>` — count first, markers last
/// 3. Bare: `⏵⏵ <mode>` — markers only, no count (count = 0)
///
/// The function looks for `⏵⏵`/`››` anywhere in the line (confirms it's a
/// mode line), then searches for `N <type>` anywhere in the same line
/// regardless of position relative to the markers.
fn parse_active_subtasks(clean: &str) -> Option<ParsedEvent> {
    // Fast path: requires ›› (U+203A), ⏵⏵ (U+23F5), or a digit (bare count)
    let has_mode_marker = clean.contains('\u{203A}') || clean.contains('\u{23F5}');
    let has_digit = clean.bytes().any(|b| b.is_ascii_digit());
    if !has_mode_marker && !has_digit {
        return None;
    }

    lazy_static::lazy_static! {
        // Detect ⏵⏵ or ›› anywhere in the line (confirms mode line)
        static ref MODE_MARKER_RE: regex::Regex =
            regex::Regex::new(r"(?:\u{203A}\u{203A}|\u{23F5}\u{23F5})").unwrap();
        // Extract count + type from anywhere in the line: "N <type>"
        // Separated from mode by · (U+00B7). Matches: "· 2 local agents" or "2 local agents ·"
        static ref SUBTASK_COUNT_RE: regex::Regex =
            regex::Regex::new(r"(\d+)\s+([\w][\w\s]*?)(?:\s*(?:\u{00B7}|\(|$))").unwrap();
        // Bare subprocess count without mode marker (e.g. "  1 shell").
        // Restricted to known subprocess types to avoid false positives.
        static ref BARE_SUBTASK_RE: regex::Regex =
            regex::Regex::new(r"^\s*(\d+)\s+((?:local )?agents?|shells?|bash|background tasks?)\s*$").unwrap();
    }

    for line in clean.lines() {
        let trimmed = line.trim();

        // Path 1: mode marker present (⏵⏵ or ››)
        if MODE_MARKER_RE.is_match(trimmed) {
            if let Some(caps) = SUBTASK_COUNT_RE.captures(trimmed) {
                let count: u32 = caps[1].parse().unwrap_or(0);
                let task_type = caps[2].trim().to_string();
                if count > 0 {
                    return Some(ParsedEvent::ActiveSubtasks { count, task_type });
                }
                return Some(ParsedEvent::ActiveSubtasks {
                    count: 0,
                    task_type: String::new(),
                });
            }
            // Mode line present but no count → sub-tasks finished
            return Some(ParsedEvent::ActiveSubtasks {
                count: 0,
                task_type: String::new(),
            });
        }

        // Path 2: bare count without mode marker (e.g. "  1 shell")
        if let Some(caps) = BARE_SUBTASK_RE.captures(trimmed) {
            let count: u32 = caps[1].parse().unwrap_or(0);
            let task_type = caps[2].trim().to_string();
            return Some(ParsedEvent::ActiveSubtasks { count, task_type });
        }
    }
    None
}

/// Detect Claude Code usage limit messages from pre-stripped text:
/// "You've used 78% of your weekly limit" or "You've used 45% of your session limit"
fn parse_usage_limit(clean: &str) -> Option<ParsedEvent> {
    // Fast path
    if !clean.contains("% of your") {
        return None;
    }
    lazy_static::lazy_static! {
        static ref USAGE_LIMIT_RE: regex::Regex =
            regex::Regex::new(r"(?i)You['\u{2019}]ve used (\d{1,3})% of your (weekly|session) limit").unwrap();
    }
    for line in clean.lines() {
        if let Some(caps) = USAGE_LIMIT_RE.captures(line) {
            let percentage: u8 = caps[1].parse().unwrap_or(0).min(100);
            let limit_type = caps[2].to_lowercase();
            return Some(ParsedEvent::UsageLimit {
                percentage,
                limit_type,
            });
        }
    }
    None
}

/// Detect Claude Code usage exhaustion from pre-stripped text:
/// "You're out of extra usage · resets 8pm (Europe/Madrid)"
/// "You're out of usage · resets Feb 21 at 9am (US/Eastern)"
fn parse_usage_exhausted(clean: &str) -> Option<ParsedEvent> {
    // Fast path
    if !clean.contains("out of") || !clean.contains("usage") {
        return None;
    }
    lazy_static::lazy_static! {
        static ref EXHAUSTED_RE: regex::Regex =
            regex::Regex::new(r"(?i)out of (?:extra )?usage").unwrap();
        static ref RESETS_RE: regex::Regex =
            regex::Regex::new(r"(?:\u{00b7}|·)\s*resets\s+(.+)$").unwrap();
    }
    for line in clean.lines() {
        if EXHAUSTED_RE.is_match(line) {
            let reset_time = RESETS_RE
                .captures(line)
                .map(|caps| caps[1].trim().to_string());
            return Some(ParsedEvent::UsageExhausted { reset_time });
        }
    }
    None
}

/// Claude Code startup session-id conflict.
///
/// Fires on two distinct startup failures:
///   * `Session ID <uuid> is already in use.` — the injected `--session-id`
///     collides with a running process or a stale lock.
///   * `No conversation found with session ID: <uuid>` — a `--resume <uuid>`
///     pointed at a session file that doesn't exist (usually wrong config dir).
///
/// Both wedge the shell: the auto-injecting wrapper keeps re-using the same
/// `$TUIC_SESSION`, so every subsequent `claude` invocation in the tab hits
/// the same error. The PTY reader consumes this event to auto-reset the env
/// var (see `pty.rs`) so the next invocation gets a fresh UUID.
fn parse_agent_session_conflict(clean: &str) -> Option<ParsedEvent> {
    // Fast path: the substring "session ID" (case-insensitive) is the only
    // common thread. Bail early on chunks that can't possibly match.
    if !clean.contains("Session ID") && !clean.contains("session ID") {
        return None;
    }
    let try_match = |re: &regex::Regex, kind: &str| -> Option<ParsedEvent> {
        let m = re.find(clean)?;
        // Guard: reject matches that live inside source code, diff hunks, or
        // markdown fences — this exact file will match its own regex when an
        // agent pastes it back into a terminal.
        if line_is_code_or_diff(line_containing_match(clean, m.start())) {
            return None;
        }
        // Reject matches inside markdown fenced code blocks (``` ... ```)
        let fences_before = clean[..m.start()]
            .lines()
            .filter(|l| l.trim().starts_with("```"))
            .count();
        if fences_before % 2 == 1 {
            return None;
        }
        Some(ParsedEvent::AgentSessionConflict {
            matched_text: m.as_str().to_string(),
            kind: kind.to_string(),
        })
    };
    try_match(&SESSION_IN_USE_RE, "in-use")
        .or_else(|| try_match(&SESSION_NOT_FOUND_RE, "not-found"))
}

/// Whether a rendered row is the Ink dialog footer itself, rather than a copy of
/// it somewhere in an agent's own output.
///
/// The anchor is matched against the row **unindented**. A dialog is drawn
/// full-bleed by the TUI and its footer starts at column 0; everything an agent
/// streams — prose, code blocks, quoted screens — is indented inside the agent's
/// own frame. That indentation is the only thing telling the two apart, because
/// the text is otherwise identical byte for byte.
///
/// Observed 2026-08-30: an agent pasted a screen it had just read, footer
/// included, into a fenced code block. The two leading spaces were the entire
/// difference; without this check the row matched, the tab latched
/// `question_confident`, and no clear path retracts a confident question — the
/// agent had marked *itself* as blocked on the user while it was working.
fn is_ink_dialog_footer_row(row: &str) -> bool {
    lazy_static::lazy_static! {
        // Anchored at column 0 of the rendered row, NOT of its trimmed text. See
        // the doc comment: the indentation is the signal.
        static ref INK_FOOTER_RE: regex::Regex =
            regex::Regex::new(r"^Enter to select").unwrap();
    }
    INK_FOOTER_RE.is_match(row) && !line_is_diff_or_code_context(row)
}

/// The Ink dialog footer, if one is visible anywhere on the screen.
///
/// This is the same anchor `parse_question` matches, read as a **level** instead
/// of an edge: as long as the row is on screen, an interactive dialog is open and
/// the agent is blocked on the user. Deliberately no structure parsing — no
/// title, no options, no wizard tab bar. Those all move between the sub-questions
/// of a multi-question `AskUserQuestion`, which is exactly what made the
/// changed-rows path miss them: the footer is the one row that stays byte-identical
/// from the first sub-question to the last, so it is useless as a *change* signal
/// and perfect as a *presence* signal.
pub fn ink_dialog_footer(screen_rows: &[String]) -> Option<&str> {
    screen_rows
        .iter()
        .find_map(|row| is_ink_dialog_footer_row(row).then(|| row.trim()))
}

/// Question detection: most detection is handled by the silence-based detector
/// in pty.rs (last line ending with `?` + 10s of silence = real question).
///
/// Only Ink interactive menu footers are detected instantly — "Enter to select"
/// is ultra-specific to real interactive prompts and never appears in streaming
/// agent output, so it doesn't suffer from the false-positive problem that
/// killed all the other regex patterns.
fn parse_question(clean: &str) -> Option<ParsedEvent> {
    lazy_static::lazy_static! {
        // cliclack interactive prompt: "◆  Do you allow this tool call?"
        // ◆ (U+25C6) is Goose's cliclack active-prompt marker. It is NOT unique to
        // interactive prompts, though: grok reuses ◆ as a decorative timeline bullet
        // ("◆ Thought for 1.3s", "◆ user_prompt_submit", tool-call labels). Those
        // repaint every frame with a changing duration, so matching them emits an
        // endless stream of (confident) Question events that never dedup — a
        // notification storm. Goose's real prompts are all questions, so the match
        // also requires the captured text to end with '?'. grok's decorative labels
        // never do, so this cleanly separates the two.
        static ref CLICLACK_PROMPT_RE: regex::Regex =
            regex::Regex::new(r"^\s*\u{25C6}\s+(.+)").unwrap();
    }
    for line in clean.lines() {
        let trimmed = line.trim();
        // Column-0 anchored — see `is_ink_dialog_footer_row`. An agent quoting a
        // dialog it read reproduces the footer's text exactly; only the frame's
        // indentation separates the copy from the original.
        if is_ink_dialog_footer_row(line) {
            return Some(ParsedEvent::Question {
                prompt_text: trimmed.to_string(),
                confident: true,
            });
        }
        if let Some(caps) = CLICLACK_PROMPT_RE.captures(line) {
            let text = caps[1].trim();
            // Require a question mark: Goose's cliclack prompts are all questions
            // ("…do you allow?", "Are you sure…?"). grok's decorative ◆ timeline
            // bullets ("Thought for 1.3s", "user_prompt_submit") never end in '?',
            // so this excludes them and kills the notification storm.
            if text.len() >= 5 && text.ends_with('?') {
                return Some(ParsedEvent::Question {
                    prompt_text: text.to_string(),
                    confident: true,
                });
            }
        }
    }
    None
}

/// Returns true if a line looks like diff output, code context, or documentation
/// Returns true if a line looks like diff output, code context, or documentation
/// rather than a genuine interactive prompt. Applied to ALL question regex matches
/// to prevent false positives from diff hunks containing question-like patterns.
pub fn line_is_diff_or_code_context(line: &str) -> bool {
    let trimmed = line.trim();

    // Line-number prefix from code listings: "462 -...", "75 +-...", "465 //...", "1226    assert!(..."
    // Distinguished from HTTP status codes ("429 Too Many Requests") by requiring either:
    //   - diff markers (+, -, //) after the number, OR
    //   - 2+ spaces after the number (code listing indentation)
    if trimmed.len() > 3
        && trimmed.as_bytes()[0].is_ascii_digit()
        && let Some(pos) = trimmed.find(|c: char| !c.is_ascii_digit())
    {
        let after_digits = &trimmed[pos..];
        let rest = after_digits.trim_start();
        // Diff markers after line number
        if rest.starts_with('+') || rest.starts_with('-') || rest.starts_with("//") {
            return true;
        }
        // 2+ whitespace chars after digits = code listing with padded line numbers
        if after_digits.len() >= 2
            && after_digits.as_bytes()[0] == b' '
            && (after_digits.as_bytes()[1] == b' ' || after_digits.as_bytes()[1] == b'\t')
        {
            return true;
        }
    }

    // Unified diff lines: start with + or - followed by content
    // Real diff lines: "+  code", "- old line", "++ file", "-- file"
    if line.len() > 1 {
        let first = line.as_bytes()[0];
        let second = line.as_bytes()[1];
        if (first == b'+' || first == b'-')
            && (second == b' ' || second == b'\t' || second == first)
        {
            return true;
        }
    }

    // Claude Code diff summary blocks: "⏺⎿ Added16lines..."
    if trimmed.contains("⏺⎿") {
        return true;
    }
    // Diff summary: "Added16lines" or "removed2lines" (no space between number and "lines")
    // Pattern: keyword followed immediately by digits then "lines" — unique to diff summaries
    if (trimmed.contains("Added") || trimmed.contains("removed"))
        && trimmed.contains("lines")
        && trimmed.chars().any(|c| c.is_ascii_digit())
    {
        // Extra check: the digit must be adjacent to "lines" (no space)
        if let Some(pos) = trimmed.find("lines")
            && pos > 0
            && trimmed.as_bytes()[pos - 1].is_ascii_digit()
        {
            return true;
        }
    }

    // Lines containing "//" as code comments (but not URLs like http://)
    if trimmed.starts_with("//") || trimmed.contains(" //") {
        return true;
    }

    // Lines with markdown bold/italic containing question-pattern keywords
    // (e.g., "**Hardcoded prompts**: ...")
    if trimmed.contains("**")
        && (trimmed.contains("prompts")
            || trimmed.contains("patterns")
            || trimmed.contains("detection"))
    {
        return true;
    }

    false
}

/// Detect plan file paths in pre-stripped terminal output.
/// Matches paths like `plans/foo.md`, `.claude/plans/bar.md`, absolute paths ending in plans/*.md
fn parse_plan_file(clean: &str) -> Option<ParsedEvent> {
    // Fast path: must contain "plans/" and ".md"
    if !clean.contains("plans/") || !clean.contains(".md") {
        return None;
    }
    lazy_static::lazy_static! {
        // Match plan file paths: optional leading path, then plans/<name>.md(x)
        // Captures the full path including any prefix directory.
        // Excludes <>, $, {}, `, * to avoid template placeholders, interpolation, and globs
        // Trailing punctuation (period, comma, etc.) after .md/.mdx is stripped
        static ref PLAN_RE: regex::Regex =
            regex::Regex::new(r#"(?:^|[\s'"`:])(/?(?:[^\s'"<>${}`*]+/)?plans/[^\s'"<>${}`*]+\.mdx?)[.,;:!?)}\]`]*(?:\s|$)"#).unwrap();
    }
    tracing::debug!(
        "[plan-file] fast-path hit, scanning lines (len={})",
        clean.len()
    );
    for line in clean.lines() {
        if line.contains("plans/") {
            tracing::debug!("[plan-file] candidate line: {:?}", line);
        }
        if let Some(caps) = PLAN_RE.captures(line) {
            // Expand leading ~/ to the user's home directory so the
            // frontend always receives an absolute path it can open.
            let path = expand_tilde(&caps[1]);
            return Some(ParsedEvent::PlanFile { path });
        }
    }
    None
}

/// Regex matching any CSI escape sequence (SGR colors, cursor movement, erase,
/// Bullet glyphs an agent may prefix the first line of an assistant message
/// with, before a column-0 protocol token (`intent:` / `suggest:`). Ink-hosted
/// agents (Claude Code) decorate with ● U+25CF / ⏺ U+23FA; Codex uses
/// • U+2022 / ◦ U+25E6. Single source of truth for the four token anchors below
/// — they MUST stay in sync, and they drifted once: the Codex bullet was known
/// to the spinner detector (`CODEX_BULLET_RE`) but not to these token regexes,
/// so a Codex-emitted `• intent:` / `• suggest:` (first message line) never
/// parsed. Char-class body only (no surrounding `[ ]`) so callers write `[{…}]`.
const AGENT_LINE_BULLETS: &str = r"\x{25CF}\x{23FA}\x{2022}\x{25E6}";

/// The same glyph set as [`AGENT_LINE_BULLETS`], for the one anchor that tests
/// leading characters directly instead of through a regex (`dewrap_suggest_keyword`).
/// Kept in sync by `agent_line_bullet_chars_match_the_regex_class`.
const AGENT_LINE_BULLET_CHARS: [char; 4] = ['\u{25CF}', '\u{23FA}', '\u{2022}', '\u{25E6}'];

/// The ack sentence (`TUICommander v1.7.7 is connected.`) and its trailing gap,
/// allowed between the bullet and an `intent:` token. Char-class-free body so
/// callers wrap it themselves: `(?:{a})?` to skip it, `({a})?` to keep it.
///
/// The protocol puts the two markers in the same message by construction: the
/// ack MUST open the agent's first message and an `intent:` MUST appear in it.
/// An agent that writes them as one sentence run therefore leaves the token
/// mid-row, where a column-0-only anchor captures **nothing at all** — not a
/// truncated intent, no intent. This one sentence is the whole relaxation: any
/// other leading prose still fails the anchor, so `The intent: of this code`
/// stays prose. Intent only — a `suggest:` never follows the ack.
const ACK_SENTENCE_PREFIX: &str = r"TUICommander[\t ]+v[0-9][^\s]*[\t ]+is[\t ]+connected\.[\t ]+";

/// Remove `suggest:` plain-prefix tokens from a rendered row before it is
/// served to PWA/REST clients. Intent text stays visible for client-side tinting.
///
/// Built from the same [`AGENT_LINE_BULLETS`] and [`ACK_SENTENCE_PREFIX`] the
/// parser anchors on, because the two had already drifted apart: `state.rs`
/// carried its own copy knowing only the two Ink bullets, so every Codex
/// `• suggest: [ … ]` was parsed by TUIC and then shown to the user anyway.
/// Suggest chips replace their source marker; intent rows remain readable.
///
/// The ack sentence is KEPT (capture group 1 is the replacement) — it is the
/// agent's own greeting and the user is meant to read it. Everything from the
/// keyword rightward goes, as does the bullet and indent of a row that carried
/// nothing else.
pub fn strip_plain_prefix_tokens(text: &str) -> std::borrow::Cow<'_, str> {
    lazy_static::lazy_static! {
        static ref PLAIN_PREFIX_RE: regex::Regex = regex::Regex::new(&format!(
            r"(?m)^[\t ]*(?:[{b}][\t ]+)?({a})?suggest:[\t ]+.*$",
            b = AGENT_LINE_BULLETS,
            a = ACK_SENTENCE_PREFIX
        ))
        .unwrap();
    }
    PLAIN_PREFIX_RE.replace_all(text, "$1")
}

/// How many rows after `head` are the wrapped tail of the bracketed
/// `suggest: [ … ]` token that `head` opens (#1380-05d8).
///
/// [`strip_plain_prefix_tokens`] works one row at a time, so on a narrow
/// terminal it removed the head row and left the continuation rows — the
/// `Federico Coletto ]` tail — as plain text. The parser reads the same token
/// bracket-bounded ([`dewrap_suggest_brackets`]); this applies the same bounds
/// to the rows to hide: the token must close with `]` within
/// [`MAX_WRAP_ROWS`] rows, with no nested `[`, blank row or new token before it.
/// An unclosed token is not parsed, so none of its rows are hidden either.
pub fn suggest_tail_rows<'a>(head: &str, following: impl IntoIterator<Item = &'a str>) -> usize {
    if structured_token_anchor(head) != Some(StructuredTokenAnchor::Suggest) {
        return 0;
    }
    let Some(keyword) = head.find("suggest:") else {
        return 0;
    };
    let Some(items) = head[keyword + "suggest:".len()..]
        .trim_start()
        .strip_prefix('[')
    else {
        return 0;
    };
    if items.contains(['[', ']']) {
        return 0;
    }
    for (index, row) in following.into_iter().take(MAX_WRAP_ROWS).enumerate() {
        if row.trim().is_empty() || row.contains('[') || structured_token_anchor(row).is_some() {
            return 0;
        }
        if row.contains(']') {
            return index + 1;
        }
    }
    0
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructuredTokenAnchor {
    Intent,
    Suggest,
}

/// One marker grammar for backend capture and frontend row styling. Keep the
/// data file portable between Rust regex and JavaScript RegExp.
fn intent_anchor_pattern() -> &'static str {
    lazy_static::lazy_static! {
        static ref PATTERNS: serde_json::Value =
            serde_json::from_str(include_str!("intent_marker.json")).expect("intent marker grammar");
    }
    PATTERNS["anchor"].as_str().expect("intent anchor")
}

lazy_static::lazy_static! {
    static ref INTENT_ANCHOR_RE: regex::Regex =
        regex::Regex::new(intent_anchor_pattern()).expect("intent anchor regex");
}

/// Identify a structured token at the start of a logical terminal line.
/// Keep this shared with the parser grammar so grid filtering accepts exactly
/// the same optional agent bullets as the authoritative token regexes.
pub fn structured_token_anchor(clean: &str) -> Option<StructuredTokenAnchor> {
    if INTENT_ANCHOR_RE.is_match(clean) {
        return Some(StructuredTokenAnchor::Intent);
    }
    lazy_static::lazy_static! {
        static ref SUGGEST_ANCHOR_RE: regex::Regex = regex::Regex::new(&format!(
            r"^[\t ]*(?:[{b}][\t ]+)?suggest:",
            b = AGENT_LINE_BULLETS,
        )).unwrap();
    }
    SUGGEST_ANCHOR_RE
        .is_match(clean)
        .then_some(StructuredTokenAnchor::Suggest)
}

/// Detect agent-declared intent tokens: `intent: <text>` or `intent: <text> (<title>)`
/// at column 0. Only parsed when an agent is active — prevents false positives from
/// prose like "The intent: of this code".
pub fn parse_intent(clean: &str, agent_active: bool) -> Option<ParsedEvent> {
    if !agent_active || !clean.contains("intent:") {
        return None;
    }
    lazy_static::lazy_static! {
        static ref INTENT_PLAIN_RE: regex::Regex = regex::Regex::new(&format!(
            r"(?m){}[\t ]+(.+)$", intent_anchor_pattern()
        )).unwrap();
    }

    // The token is a single logical line, but the agent that wrote it wraps its
    // own output: rejoin the continuation rows before the single-row regex runs.
    let dewrapped = dewrap_intent_continuation(clean);

    let raw_match = INTENT_PLAIN_RE
        .captures(dewrapped.as_ref())
        .map(|caps| caps[1].trim().to_string());

    build_intent_event(raw_match, &INTENT_TITLE_RE)
}

lazy_static::lazy_static! {
    /// The shared title closure also bounds frontend tinting. Trailing prose
    /// is not part of the token; earlier parenthesized asides stay in the body.
    static ref INTENT_TITLE_RE: regex::Regex = {
        let grammar: serde_json::Value =
            serde_json::from_str(include_str!("intent_marker.json")).expect("intent marker grammar");
        regex::Regex::new(grammar["title"].as_str().expect("intent title grammar")).unwrap()
    };
}

/// Keep pathological narrow-pane output bounded while allowing long intents.
pub const MAX_INTENT_CONTINUATION_ROWS: usize = 64;
pub const MAX_INTENT_CANDIDATE_CHARS: usize = 2048;

/// Rejoin an `intent:` token that the agent hard-wrapped across physical rows.
///
/// `suggest:` is bounded by its closing `]` and gets three dewrap passes;
/// `intent:` has no terminator at all, so [`INTENT_PLAIN_RE`]'s `$` simply cuts
/// the token at the wrap. The loss is not cosmetic — the `(title)` rides at the
/// END of the token, so the wrapped case drops precisely the tab title the
/// event exists to set.
///
/// Ink-hosted agents wrap at a word boundary and indent the continuation by the
/// bullet width. The PTY supplies its column count so an indented row is joined
/// only when the preceding row could not fit its first word. Blank rows,
/// bullets, prompts, and a closed `(title)` stop the join. Direct parser calls
/// have no terminal geometry and retain the legacy indentation check.
///
/// Returns `Cow::Borrowed` when there is nothing to rejoin.
fn dewrap_intent_continuation(text: &str) -> std::borrow::Cow<'_, str> {
    dewrap_intent_continuation_with_rows(text, None).0
}

pub fn dewrap_intent_continuation_with_rows<'a>(
    text: &'a str,
    geometry: Option<(usize, &[usize])>,
) -> (std::borrow::Cow<'a, str>, usize) {
    if !text.contains('\n') {
        return (std::borrow::Cow::Borrowed(text), 0);
    }
    let lines: Vec<&str> = text.split('\n').collect();
    let Some(anchor) = lines
        .iter()
        .position(|line| structured_token_anchor(line) == Some(StructuredTokenAnchor::Intent))
    else {
        return (std::borrow::Cow::Borrowed(text), 0);
    };

    let mut merged = lines[anchor].trim_end().to_string();
    if intent_row_is_complete(&merged) {
        return (std::borrow::Cow::Borrowed(text), 0);
    }
    let mut absorbed = 0;
    for line in lines.iter().skip(anchor + 1) {
        if absorbed == MAX_INTENT_CONTINUATION_ROWS
            || merged.chars().count() >= MAX_INTENT_CANDIDATE_CHARS
            || !is_intent_continuation_row(line)
        {
            break;
        }
        if let Some((columns, physical_widths)) = geometry {
            let previous = lines[anchor + absorbed].trim_end();
            let first_word = line.split_whitespace().next().unwrap_or_default();
            let previous_width = physical_widths
                .get(absorbed)
                .copied()
                .unwrap_or_else(|| unicode_width::UnicodeWidthStr::width(previous));
            if previous_width + 1 + unicode_width::UnicodeWidthStr::width(first_word) <= columns {
                break;
            }
        }
        if merged.chars().count() + 1 + line.trim().chars().count() > MAX_INTENT_CANDIDATE_CHARS {
            break;
        }
        merged.push(' ');
        merged.push_str(line.trim());
        absorbed += 1;
        if intent_row_is_complete(&merged) {
            break;
        }
    }
    if absorbed == 0 {
        return (std::borrow::Cow::Borrowed(text), 0);
    }

    let mut kept: Vec<&str> = Vec::with_capacity(lines.len() - absorbed);
    kept.extend_from_slice(&lines[..anchor]);
    kept.push(merged.as_str());
    kept.extend_from_slice(&lines[anchor + 1 + absorbed..]);
    (std::borrow::Cow::Owned(kept.join("\n")), absorbed)
}

/// True when the last group is closed, with no unfinished later title.
pub fn intent_row_is_complete(row: &str) -> bool {
    INTENT_TITLE_RE.is_match(row.trim_end())
}

/// True when the row is the wrapped tail of the `intent:` row above it.
fn is_intent_continuation_row(row: &str) -> bool {
    // The wrap indent is the whole signal: without it the row is a new logical
    // line rather than the tail of the one above.
    if !row.starts_with([' ', '\t']) {
        return false;
    }
    let trimmed = row.trim();
    let Some(first) = trimmed.chars().next() else {
        // A blank row ends the paragraph, and with it the token.
        return false;
    };
    if structured_token_anchor(row).is_some() || AGENT_LINE_BULLET_CHARS.contains(&first) {
        return false;
    }
    // A prompt glyph or box drawing opens chrome, never agent prose.
    !matches!(
        first,
        '>' | '\u{203A}' | '\u{276F}' | '\u{2500}'..='\u{259F}'
    )
}

/// Shared logic for building an Intent event from a raw match string.
fn build_intent_event(raw_match: Option<String>, title_re: &regex::Regex) -> Option<ParsedEvent> {
    raw_match.and_then(|raw| {
        let raw = raw.trim();
        // Filter out meaningless intents: ellipsis, bare punctuation, template placeholders
        if raw == "..." || raw == "<text>" || raw.len() < 4 {
            return None;
        }
        // Extract the title at the shared end-of-token boundary.
        let (text, title) = if let Some(tc) = title_re.captures(raw) {
            let body = tc[1].trim();
            let t = tc[2].trim();
            if body.is_empty() {
                // No text before title — treat the whole thing as text
                (raw.to_string(), None)
            } else {
                (body.to_string(), Some(t.to_string()))
            }
        } else {
            (raw.to_string(), None)
        };
        // Normalize whitespace: VtLogBuffer renders CUF cursor-forward codes as
        // multiple spaces when it renders the VT100 screen buffer to text.
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        let title = title.map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "));
        Some(ParsedEvent::Intent { text, title })
    })
}

lazy_static::lazy_static! {
    /// Bracketed suggest token, anchored at column 0 (optional leading
    /// whitespace and a bullet glyph — see AGENT_LINE_BULLETS). Two constraints
    /// make a false capture impossible:
    ///   1. the capture excludes `\r`/`\n`, so it matches a single logical row,
    ///   2. the content contains NO nested `[`/`]`.
    /// A token wider than the terminal wraps across physical rows; that case is
    /// rejoined upstream by [`dewrap_suggest_brackets`] (bounded by the closing
    /// `]`, a small row window, and the same nested-bracket rejection) before
    /// this regex runs. A mermaid edge (`EP["…"] -->|x|`) or markdown table
    /// still fails to match — it lacks the column-0 `suggest: [` anchor or
    /// carries a nested `[`. Capture group 1 is the item list.
    static ref SUGGEST_RE: regex::Regex = regex::Regex::new(&format!(
        r"(?m)^[\t ]*(?:[{b}][\t ]+)?suggest:[\t ]*\[([^\[\]\r\n]*)\]",
        b = AGENT_LINE_BULLETS
    ))
    .unwrap();
}

/// Detect suggested follow-up actions: `suggest: [ A | B | C ]` at column 0.
/// The whole token lives on one bounded logical line and its bracketed content
/// holds no nested brackets, so stray pipes/brackets in surrounding output
/// (mermaid, markdown tables, prose) can never be captured. Inner items are
/// pipe-separated, 2–4 per the TUIC protocol. Only parsed when an agent is active.
#[cfg(test)]
fn parse_suggest(clean: &str, agent_active: bool) -> Option<ParsedEvent> {
    parse_suggest_with_line(clean, agent_active).map(|(evt, _)| evt)
}

/// Like [`parse_suggest`] but also returns the canonical `suggest: …` line.
fn parse_suggest_with_line(clean: &str, agent_active: bool) -> Option<(ParsedEvent, String)> {
    if !agent_active {
        return None;
    }
    // Pass 1: dewrap mid-word wrap of the keyword `suggest:` itself.
    // On very narrow terminals the VT buffer can split the word across two
    // physical rows (e.g. `sugges\nt:` or `suggest\n:`). We rejoin these so
    // the downstream regex can match at column 0.
    let kw_cow = dewrap_suggest_keyword(clean);
    // Pass 2: when the terminal is so narrow that `suggest: ` fills the row
    // entirely (≤9 cols for a plain prefix), the content lands on the next
    // physical row.  Remove the stray newline so SUGGEST_START_RE can see it.
    let content_cow = dewrap_suggest_content(kw_cow.as_ref());
    // Pass 3: the bracketed list itself wrapped across physical rows because the
    // token is wider than the terminal (e.g. a ~78-col `suggest: [ … ]` on a
    // 76-col window). Rejoin the newlines inside the latest `[ … ]` so the
    // single-row SUGGEST_RE can match. Bounded by the closing `]`, a small row
    // window, and nested-bracket rejection so surrounding output is never
    // absorbed.
    let bracket_cow = dewrap_suggest_brackets(content_cow.as_ref());
    let clean = bracket_cow.as_ref();
    if !clean.contains("suggest:") {
        return None;
    }
    // A full-screen repaint can include completed turns above the current
    // response. The lowest complete marker is the latest choice list; choosing
    // the first would republish consumed suggestions when Claude redraws history.
    let caps = SUGGEST_RE.captures_iter(clean).last()?;
    let items: Vec<String> = caps[1]
        .split('|')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    // Protocol: 2–4 short actions (spec says exactly 3; the overlay binds number
    // keys 1–4). Reject anything outside that range.
    if items.len() < 2 || items.len() > 4 {
        return None;
    }

    // Canonical, normalized form for callers that need the matched token text.
    let canonical_line = format!("suggest: [ {} ]", items.join(" | "));
    Some((ParsedEvent::Suggest { items }, canonical_line))
}

/// Rejoin a `suggest:` line when the terminal is so narrow that the keyword
/// plus its trailing space/tab fills the entire physical row, pushing the
/// item content onto the next line.
///
/// Example (9-col terminal):
/// ```text
/// suggest: \nA | B | C
/// ```
/// → `suggest: A | B | C`
///
/// Returns `Cow::Borrowed` when no such split is detected.
fn dewrap_suggest_content(text: &str) -> std::borrow::Cow<'_, str> {
    if !text.contains("suggest:") || !text.contains('\n') {
        return std::borrow::Cow::Borrowed(text);
    }
    lazy_static::lazy_static! {
        // Matches a `suggest:` prefix line (with optional whitespace/bullet)
        // that ends with only whitespace before the newline — no item content.
        // Capture group 1: everything up to (but not including) trailing space+\n.
        // The trailing run is `*`, not `+`: after the keyword rejoin of a
        // colon-alone row (`\u{2022} suggest` / `  :` at 9 cols) there is no
        // space left after the colon at all, and requiring one dropped the
        // narrowest real Codex shape.
        static ref SUGGEST_TRAILING_RE: regex::Regex = regex::Regex::new(&format!(
            r"(?m)^([\t ]*(?:[{b}][\t ]+)?suggest:[\t ]*)[ \t]*\n",
            b = AGENT_LINE_BULLETS
        ))
        .unwrap();
    }
    if !SUGGEST_TRAILING_RE.is_match(text) {
        return std::borrow::Cow::Borrowed(text);
    }
    // Replace `suggest: <trailing spaces>\n` with `suggest: ` so the next
    // physical row's content is treated as the first item line.
    std::borrow::Cow::Owned(SUGGEST_TRAILING_RE.replace_all(text, "$1").to_string())
}

/// Continuation rows a wrapped `suggest: [ … ]` token may span. 4 physical rows
/// comfortably hold a max-size token (4 items × ≤40 chars + separators +
/// brackets ≈ 170 cols ≈ 3 wraps at a typical narrow width).
const MAX_WRAP_ROWS: usize = 4;

/// Rejoin a `suggest: [ … ]` token whose bracketed content wrapped across
/// physical terminal rows. The single-row [`SUGGEST_RE`] cannot match once the
/// VT buffer has split the list with `\n` (a token wider than the terminal,
/// e.g. ~78 cols of items on a 76-col window), so the overlay never fires.
/// We reconstruct the logical line by collapsing newlines (and the horizontal
/// whitespace the wrap leaves behind) inside the latest `[ … ]` into single
/// spaces.
///
/// Because a terminal hard-wrap is column-based, it may break mid-word; the
/// space-collapse is therefore a best-effort rejoin (correct when the break
/// lands on a separator, slightly off mid-word). That is strictly better than
/// the prior behaviour of not rendering the overlay at all.
///
/// The same safety invariants the single-row regex relies on are preserved:
///   - we only act on a column-0 `suggest: [` anchor,
///   - we bail if a nested `[` appears before the closing `]` (mermaid/table),
///   - we scan at most [`MAX_WRAP_ROWS`] rows after the anchor, so a stray
///     `suggest: [` with no closing `]` cannot swallow unbounded output.
///
/// Returns `Cow::Borrowed` when no wrapped suggest is detected.
fn dewrap_suggest_brackets(text: &str) -> std::borrow::Cow<'_, str> {
    if !text.contains("suggest:") || !text.contains('\n') {
        return std::borrow::Cow::Borrowed(text);
    }
    lazy_static::lazy_static! {
        // Column-0 `suggest:` anchor up to and including the opening `[`.
        static ref SUGGEST_OPEN_RE: regex::Regex = regex::Regex::new(&format!(
            r"(?m)^[\t ]*(?:[{b}][\t ]+)?suggest:[\t ]*\[",
            b = AGENT_LINE_BULLETS
        ))
        .unwrap();
        // A newline plus the horizontal whitespace surrounding it.
        static ref WRAP_WS_RE: regex::Regex = regex::Regex::new(r"[\t ]*\n[\t ]*").unwrap();
    }
    let Some(m) = SUGGEST_OPEN_RE.find_iter(text).last() else {
        return std::borrow::Cow::Borrowed(text);
    };
    let content_start = m.end(); // byte index just after the opening `[`

    // Scan forward for the closing `]`, bailing on a nested `[` (preserve the
    // mermaid/table rejection) or after the row window is exhausted.
    let mut newline_count = 0usize;
    let mut close_idx = None;
    for (off, ch) in text[content_start..].char_indices() {
        match ch {
            ']' => {
                close_idx = Some(content_start + off);
                break;
            }
            '[' => return std::borrow::Cow::Borrowed(text),
            '\n' => {
                newline_count += 1;
                if newline_count > MAX_WRAP_ROWS {
                    return std::borrow::Cow::Borrowed(text);
                }
            }
            _ => {}
        }
    }
    let Some(close_idx) = close_idx else {
        // No closing `]` within reach — leave untouched so an incomplete token
        // does not parse.
        return std::borrow::Cow::Borrowed(text);
    };

    let content = &text[content_start..close_idx];
    if !content.contains('\n') {
        // Already single-row; the regex handles it without a rewrite.
        return std::borrow::Cow::Borrowed(text);
    }

    let collapsed = WRAP_WS_RE.replace_all(content, " ");
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..content_start]);
    out.push_str(&collapsed);
    out.push_str(&text[close_idx..]);
    std::borrow::Cow::Owned(out)
}

/// Rejoin a `suggest:` keyword that got split across a newline by terminal
/// auto-wrap (e.g. zoomed / narrow window). Handles every split position of
/// the literal word: `s\nuggest:`, `su\nggest:`, ..., `suggest\n:`, including
/// when the continuation row carries the agent's own hanging wrap indent
/// (`suggest\n  :`), which is what Codex emits in a narrow pane.
///
/// Returns `Cow::Borrowed` when no wrap is detected so the caller pays no
/// allocation cost in the common (wide terminal) case.
fn dewrap_suggest_keyword(text: &str) -> std::borrow::Cow<'_, str> {
    if !text.contains('\n') {
        return std::borrow::Cow::Borrowed(text);
    }
    const SUGGEST: &str = "suggest:";

    // Every way "suggest:" can split across a newline at column 0:
    //   prefix (non-empty proper prefix of "suggest:") + "\n" + wrap indent
    //   + suffix, where prefix + suffix == "suggest:".
    // Iterate 1..=7 split points: s|uggest:, su|ggest:, …, suggest|:.
    //
    // Stay Borrowed until we actually rewrite. PTY chunks land here on every
    // newline flush, so allocating a String up-front defeats the Cow return
    // whenever no suggest-keyword split is present (the common case).
    let mut current: std::borrow::Cow<'_, str> = std::borrow::Cow::Borrowed(text);

    for split in 1..SUGGEST.len() {
        let (prefix, suffix) = SUGGEST.split_at(split);
        // Match on the prefix + newline alone: the suffix may sit behind the
        // agent's own wrap indent, so it cannot be part of a fixed needle.
        let wrap_at = format!("{prefix}\n");
        if !current.contains(&wrap_at) {
            continue;
        }

        let src: &str = current.as_ref();
        // Allocate only once a match actually qualifies — a bare `s\n` ends
        // plenty of ordinary prose lines and must stay on the borrowed path.
        let mut buf: Option<String> = None;
        let mut last = 0;
        for (idx, _) in src.match_indices(&wrap_at) {
            // A previous rewrite already consumed this region.
            if idx < last {
                continue;
            }
            // Only dewrap when the prefix begins at column 0 — optionally
            // after whitespace or an agent bullet marker. Accept the same
            // glyphs as the token regexes: this check knew only the Ink
            // bullets, so a wrapped Codex `• sugg\nest:` never rejoined.
            let line_start = src[..idx].rfind('\n').map_or(0, |n| n + 1);
            let leading = &src[line_start..idx];
            let leading_ok = leading
                .chars()
                .all(|c| c == ' ' || c == '\t' || AGENT_LINE_BULLET_CHARS.contains(&c));
            if !leading_ok {
                continue;
            }
            // Skip the continuation row's hanging indent: Codex soft-wraps its
            // own output with two leading spaces, so the tail of the keyword
            // does not start at column 0 (captured live in a 9-column pane).
            let after_newline = idx + wrap_at.len();
            let rest = &src[after_newline..];
            let indent = rest.len() - rest.trim_start_matches([' ', '\t']).len();
            if !rest[indent..].starts_with(suffix) {
                continue;
            }
            let out = buf.get_or_insert_with(|| String::with_capacity(src.len()));
            out.push_str(&src[last..idx]);
            out.push_str(prefix);
            out.push_str(suffix);
            last = after_newline + indent + suffix.len();
        }
        if let Some(mut out) = buf {
            out.push_str(&src[last..]);
            current = std::borrow::Cow::Owned(out);
        }
    }

    current
}

/// Detect a slash command autocomplete menu from screen bottom rows.
///
/// Called separately from `parse_clean_lines` because it needs the *full*
/// screen snapshot (not just changed rows) — navigating the menu with arrows
/// only changes the highlighted row while the rest stays unchanged.
///
/// Expected format (Claude Code / Codex):
/// ```text
///    /help      Get help with using Claude Code
///  ❯ /review    Review your code
///    /clear     Clear conversation history
/// ```
///
/// Called only while the input FSM is in slash mode. A filtered menu may have
/// just one matching row at the bottom of the screen.
/// Highlighted item: detected by `❯` prefix or falls back to first item.
// DEFERRED (2026-10-19) — Codex 0.162.1 paints completion above its composer and
// status line, with a `›` selection marker. Supporting that distinct layout is
// outside the ego single-match/retraction fix; live evidence is in story 2030-84bb.
pub fn parse_slash_menu(screen_rows: &[String]) -> Option<ParsedEvent> {
    lazy_static::lazy_static! {
        // Each menu row: optional leading whitespace, optional ❯ marker, /command, 2+ spaces, description
        static ref MENU_ROW_RE: regex::Regex =
            regex::Regex::new(r"^\s*(?:❯\s+)?(/\S+)\s{2,}(.+)$").unwrap();
        // Continuation row: deeply indented text that wraps from a long description.
        // Requires 8+ leading spaces to distinguish from chrome lines (status bar,
        // permission toggle) which have only 2-4 spaces of indent.
        static ref CONTINUATION_RE: regex::Regex =
            regex::Regex::new(r"^\s{8,}\S").unwrap();
    }

    // Scan from the bottom to find the contiguous block of menu rows.
    // Menu items are at the bottom of the screen; stop at the first non-matching row.
    // Continuation lines (wrapped descriptions) are skipped — they don't start with /.
    let mut items: Vec<SlashMenuItem> = Vec::new();
    for row in screen_rows.iter().rev() {
        let trimmed = row.trim();
        if trimmed.is_empty() {
            // Skip trailing empty rows at screen bottom
            if items.is_empty() {
                continue;
            }
            break;
        }
        if let Some(caps) = MENU_ROW_RE.captures(row) {
            let command = caps[1].to_string();
            let description = caps[2].trim().to_string();
            let highlighted = row.contains('❯');
            items.push(SlashMenuItem {
                command,
                description,
                highlighted,
            });
        } else if CONTINUATION_RE.is_match(row) {
            // Wrapped description line — skip without breaking
            continue;
        } else {
            break;
        }
    }

    // Reverse to restore top-to-bottom order (we scanned bottom-to-top).
    items.reverse();

    if items.is_empty() {
        return None;
    }

    // Fallback: if no item has ❯, highlight the first item.
    if !items.iter().any(|it| it.highlighted)
        && let Some(first) = items.first_mut()
    {
        first.highlighted = true;
    }

    Some(ParsedEvent::SlashMenu { items })
}

/// Detect a numbered choice dialog rendered below the prompt line.
///
/// Shape expected (Claude Code edit-confirm example):
/// ```text
/// Do you want to make this edit to CLAUDE.md?
/// ❯ 1. Yes
///   2. Yes, allow all edits during this session (shift+tab)
///   3. No
///
/// Esc to cancel · Tab to amend
/// ```
///
/// Strategy — scan bottom-up:
///   1. Skip trailing empty rows and optional footer hints (`Esc to .* · Tab .*`).
///   2. Collect contiguous numbered option rows (`  1. label`, `❯ 2. label`, etc.).
///   3. Walk up past blank rows to find the first non-empty row preceding the
///      block — that's the title. Title must end in `?` OR be preceded by a
///      `Do you want` / `Proceed` / `Continue` / `Should I` verb (guardrail
///      against markdown numbered lists).
///   4. Require ≥ 2 options to reduce false positives.
///
/// Deliberately does NOT match Claude Code's `AskUserQuestion` dialog: its
/// options are separated by wrapped description lines and a `───` rule, and its
/// footer is `Enter to select · …`, not `Esc to … · Tab to …`. Relaxing steps 1–2
/// enough to absorb it would also match any prose "Which one do you want?\n1. …
/// \n   detail\n2. …" an agent prints — and a false ChoicePrompt renders a real
/// interactive overlay. That dialog has a separate, footer-anchored parser.
pub fn parse_choice_prompt(screen_rows: &[String]) -> Option<ParsedEvent> {
    lazy_static::lazy_static! {
        // Option: optional ❯/›/> marker, digit(s), . or ), space, label.
        static ref OPTION_RE: regex::Regex =
            regex::Regex::new(r"^\s*(?:[❯›>]\s*)?(\d+)[.)]\s+(.+?)\s*$").unwrap();
        // Footer: `Esc to cancel · Tab to amend` style.
        static ref FOOTER_RE: regex::Regex =
            regex::Regex::new(r"^\s*(?:Esc|esc|ESC)\s+to\s+(\S+).*?(?:·|\||•)\s*(?:Tab|tab|TAB)\s+to\s+(\S+)").unwrap();
        // Supported Codex 0.162 request_user_input opens with Shift+Left.
        static ref CODEX_FOOTER_RE: regex::Regex =
            regex::Regex::new(r"(?i)^\s*enter\s+submit\s+⌃\]\s+skip\s+⇧→\s+main prompt\s*$").unwrap();
        // Narrow /plan layouts put each footer action on its own row.
        static ref CODEX_PLAN_NOTES_RE: regex::Regex = regex::Regex::new(
            r"^(?:option [1-9][0-9]*/[1-9][0-9]* \| )?tab to add notes$"
        ).unwrap();
        // Title sentinel: question mark OR imperative verb. Keeps us off markdown lists.
        static ref TITLE_VERB_RE: regex::Regex =
            regex::Regex::new(r"(?i)^\s*(?:do you want|proceed with|continue|should i|confirm|apply|allow)\b").unwrap();
        // Hint in parens at end of label: "(shift+tab)".
        static ref HINT_RE: regex::Regex =
            regex::Regex::new(r"\s*\(([^()]+)\)\s*$").unwrap();
    }

    if screen_rows.is_empty() {
        return None;
    }

    let mut idx = screen_rows.len();
    let mut dismiss_key: Option<String> = None;
    let mut amend_key: Option<String> = None;
    let mut codex_footer = false;
    let mut codex_approval = false;
    let mut codex_plan = false;

    // Step 1: skip trailing blanks + optional footer.
    while idx > 0 {
        let row = screen_rows[idx - 1].trim();
        if row.is_empty() {
            idx -= 1;
            continue;
        }
        if let Some(caps) = FOOTER_RE.captures(&screen_rows[idx - 1]) {
            dismiss_key = Some(caps[1].to_string());
            amend_key = Some(caps[2].to_string());
            idx -= 1;
            continue;
        }
        if row == "Press enter to confirm or esc to cancel" {
            dismiss_key = Some("cancel".to_owned());
            codex_approval = true;
            idx -= 1;
            continue;
        }
        if idx >= 3
            && row == "esc to interrupt"
            && screen_rows[idx - 2].trim() == "enter to submit answer"
            && CODEX_PLAN_NOTES_RE.is_match(screen_rows[idx - 3].trim())
        {
            dismiss_key = Some("cancel".to_owned());
            amend_key = Some("amend".to_owned());
            codex_plan = true;
            idx -= 3;
            continue;
        }
        if row == "tab to add notes | enter to submit answer | esc to interrupt" {
            dismiss_key = Some("cancel".to_owned());
            amend_key = Some("amend".to_owned());
            codex_plan = true;
            idx -= 1;
            continue;
        }
        if CODEX_FOOTER_RE.is_match(&screen_rows[idx - 1]) {
            dismiss_key = Some("ctrl+]".to_string());
            amend_key = Some("shift+right".to_string());
            codex_footer = true;
            idx -= 1;
            continue;
        }
        break;
    }

    // Step 2: collect contiguous option rows bottom-up.
    let mut options_rev: Vec<ChoiceOption> = Vec::new();
    let mut continuations: Vec<&str> = Vec::new();
    while idx > 0 {
        let raw = &screen_rows[idx - 1];
        if raw.trim().is_empty() {
            break;
        }
        if let Some(caps) = OPTION_RE.captures(raw) {
            let key = caps[1].to_string();
            let mut label = caps[2].trim().to_string();
            let highlighted = raw.contains('❯') || raw.contains('›');

            // /plan renders descriptions in a second column, including its
            // notes hint. They are not part of the selectable answer label.
            let hint = if codex_plan {
                let label_column = raw[..caps.get(2)?.start()].chars().count();
                let hint_column = label.find("  ").map(|split| {
                    label_column
                        + label[..split].chars().count()
                        + label[split..]
                            .chars()
                            .take_while(|ch| ch.is_whitespace())
                            .count()
                });
                let mut hint = label.split_once("  ").map(|(answer, description)| {
                    (answer.trim().to_owned(), description.trim().to_owned())
                });
                if let Some((answer, _)) = &hint {
                    label = answer.clone();
                }
                // Native /plan wraps the answer and its description independently.
                // Keep their columns separate; a description is never an answer key.
                for continuation in continuations.drain(..).rev() {
                    let columns: Vec<char> = continuation.chars().collect();
                    if columns
                        .iter()
                        .take(label_column)
                        .any(|ch| !ch.is_whitespace())
                    {
                        return None;
                    }
                    let split = hint_column.unwrap_or(columns.len()).min(columns.len());
                    let answer: String = columns
                        .iter()
                        .skip(label_column)
                        .take(split.saturating_sub(label_column))
                        .collect();
                    append_codex_plan_fragment(&mut label, answer.trim());
                    if let Some((_, description)) = &mut hint {
                        let fragment: String = columns.iter().skip(split).collect();
                        append_codex_plan_fragment(description, fragment.trim());
                    }
                }
                hint.map(|(_, description)| description)
            } else {
                let hint = HINT_RE.captures(&label).map(|c| c[1].to_string());
                if hint.is_some() {
                    label = HINT_RE.replace(&label, "").trim().to_string();
                }
                hint
            };

            let destructive = {
                let lower = label.to_lowercase();
                matches!(
                    lower.as_str(),
                    "no" | "cancel" | "reject" | "abort" | "deny"
                ) || lower.starts_with("no,")
                    || lower.starts_with("no ")
                    || lower.starts_with("don't")
                    || lower.starts_with("do not")
            };

            options_rev.push(ChoiceOption {
                key,
                label,
                highlighted,
                destructive,
                hint,
            });
            idx -= 1;
        } else if codex_plan && raw.starts_with("       ") {
            continuations.push(raw.as_str());
            idx -= 1;
        } else {
            break;
        }
    }

    if !continuations.is_empty() || options_rev.len() < 2 {
        return None;
    }

    // Step 3: walk past blank rows, find title row.
    while idx > 0 && screen_rows[idx - 1].trim().is_empty() {
        idx -= 1;
    }
    if idx == 0 {
        return None;
    }
    // The command approval puts Environment, Reason and the command between
    // its question and options. Its exact footer distinguishes it from the
    // queued native question panel, whose numbers also submit immediately.
    let title_row = if codex_approval {
        screen_rows[..idx]
            .iter()
            .rev()
            .find(|row| row.trim() == "Would you like to run the following command?")?
            .trim()
    } else {
        screen_rows[idx - 1].trim()
    };
    // A quoted numbered list can include the footer text. Codex's actual
    // request_user_input panel has this heading above the question.
    if codex_footer
        && !screen_rows[..idx - 1]
            .iter()
            .rev()
            .take(4)
            .any(|row| row.trim() == "• Queued follow-up inputs")
    {
        return None;
    }
    // /plan is a distinct native form, not the queued-follow-up panel. Require
    // its counter immediately above the title rather than accepting prose lists
    // solely because their title is imperative or their footer was quoted.
    if codex_plan {
        let header = screen_rows[..idx - 1]
            .iter()
            .rev()
            .find(|row| !row.trim().is_empty())?
            .trim();
        let counter = header.strip_prefix("Question ")?;
        let (position, unanswered) = counter.split_once(" (")?;
        let (current, total) = position.split_once('/')?;
        let current = current.parse::<usize>().ok()?;
        let total = total.parse::<usize>().ok()?;
        let unanswered = unanswered
            .strip_suffix(" unanswered)")?
            .parse::<usize>()
            .ok()?;
        if current == 0
            || current > total
            || unanswered == 0
            || unanswered > total
            || options_rev
                .iter()
                .rev()
                .enumerate()
                .any(|(index, option)| option.key.parse::<usize>().ok() != Some(index + 1))
        {
            return None;
        }
    }
    // The exact Codex footer plus queued-panel heading is authoritative even
    // for imperative titles such as the recorded `Pick a color.`.
    let title_qualifies =
        codex_footer || codex_plan || title_row.ends_with('?') || TITLE_VERB_RE.is_match(title_row);
    if !title_qualifies {
        return None;
    }
    let title = title_row.to_string();

    // Step 4: restore top-to-bottom order.
    options_rev.reverse();
    let mut options = options_rev;

    // Fallback highlight: if none marked, highlight the first option.
    if !options.iter().any(|o| o.highlighted)
        && let Some(first) = options.first_mut()
    {
        first.highlighted = true;
    }

    Some(ParsedEvent::ChoicePrompt {
        title,
        options,
        selection_mode: None,
        dismiss_key,
        amend_key,
    })
}

// /plan wraps at word boundaries, but punctuation can start a continuation.
fn append_codex_plan_fragment(text: &mut String, fragment: &str) {
    if fragment.is_empty() {
        return;
    }
    if !text.is_empty() && !fragment.starts_with([',', '.', ';', ':', ')']) {
        text.push(' ');
    }
    text.push_str(fragment);
}

/// Grok's native Ask is a framed radio form, not a numbered Markdown list.
/// Numeric keys settle immediately; z switches to an in-place text composer.
pub fn parse_grok_question(screen_rows: &[String]) -> Option<ParsedEvent> {
    lazy_static::lazy_static! {
        static ref RADIO_RE: regex::Regex = regex::Regex::new(
            r"^\s*(?:┃\s*)?([1-9]\d*|z)\s+\(([○●])\)\s+(.+?)\s*$"
        ).unwrap();
    }
    let blank = |row: &str| matches!(row.trim(), "" | "┃");
    let mut end = screen_rows.len();
    while end > 0 && blank(&screen_rows[end - 1]) {
        end -= 1;
    }
    let mut first = end;
    let mut options = Vec::new();
    let mut free_text = false;
    while first > 0 {
        let Some(caps) = RADIO_RE.captures(&screen_rows[first - 1]) else {
            break;
        };
        let key = caps[1].to_owned();
        let text = caps[3].trim();
        free_text |= key == "z" && &caps[2] == "●" && text.starts_with('❯');
        let (label, hint) = text.split_once("  ").map_or((text, None), |(label, hint)| {
            (label, Some(hint.trim().to_owned()))
        });
        options.push(ChoiceOption {
            key,
            label: label.to_owned(),
            highlighted: &caps[2] == "●",
            destructive: label.eq_ignore_ascii_case("no"),
            hint,
        });
        first -= 1;
    }
    options.reverse();
    let ask = options.last()?.key == "z";
    let numbered = options.len() - usize::from(ask);
    if numbered < 2
        || options[..numbered]
            .iter()
            .enumerate()
            .any(|(index, option)| option.key.parse::<usize>().ok() != Some(index + 1))
    {
        return None;
    }
    let mut title_end = first;
    while title_end > 0 && blank(&screen_rows[title_end - 1]) {
        title_end -= 1;
    }
    let title = if ask {
        let mut title_start = title_end;
        while title_start > 0 {
            let row = screen_rows[title_start - 1].trim();
            if !row.starts_with('┃') || blank(row) {
                break;
            }
            title_start -= 1;
        }
        if title_start == title_end
            || !screen_rows[..title_start].iter().rev().take(4).any(|row| {
                row.trim()
                    .strip_prefix('◆')
                    .is_some_and(|row| row.trim_start().starts_with("Waiting on answers for "))
            })
        {
            return None;
        }
        screen_rows[title_start..title_end]
            .iter()
            .map(|row| row.trim().trim_start_matches('┃').trim())
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        // Enforced approvals have a framed title, command and scope controls,
        // not Ask's waiting header or z option. Their numeric replies are also
        // immediate (the recorded 4 rejects without an Enter).
        let scope = screen_rows
            .get(title_end.checked_sub(1)?)?
            .trim()
            .strip_prefix('┃')?
            .trim();
        if !scope.starts_with("← → narrow scope") || !scope.ends_with("e edit pattern") {
            return None;
        }
        let header = (0..title_end - 1).rev().find(|&index| {
            screen_rows[index]
                .trim()
                .strip_prefix('┃')
                .is_some_and(|row| row.trim().starts_with('›'))
        })?;
        let command = (header + 1..title_end - 1).find(|&index| {
            screen_rows[index]
                .trim()
                .strip_prefix('┃')
                .is_some_and(|row| row.trim().starts_with('│'))
        })?;
        let title = screen_rows[header..command]
            .iter()
            .map(|row| {
                row.trim()
                    .trim_start_matches('┃')
                    .trim()
                    .trim_start_matches('›')
                    .trim()
            })
            .collect::<Vec<_>>()
            .join(" ");
        for option in &mut options {
            if option.label.ends_with(')')
                && let Some((label, hint)) = option.label.rsplit_once(" (")
            {
                option.hint = Some(hint.trim_end_matches(')').to_owned());
                option.label = label.to_owned();
            }
            option.destructive = option.label.to_ascii_lowercase().starts_with("no,");
        }
        title
    };
    if free_text {
        return Some(ParsedEvent::Question {
            prompt_text: title,
            confident: true,
        });
    }
    if !options.iter().any(|option| option.highlighted) {
        options[0].highlighted = true;
    }
    Some(ParsedEvent::ChoicePrompt {
        title,
        options,
        selection_mode: None,
        dismiss_key: None,
        amend_key: None,
    })
}

/// Exclusive end of a Claude multi-question Submit review: the separator row
/// after the tab bar `← ☒ A  ☒ B  ✔ Submit →`, which Claude draws full-bleed at
/// column zero like the dialog footer, or the end of the screen.
fn submit_review_end(screen_rows: &[String]) -> Option<usize> {
    let tab_bar = screen_rows
        .iter()
        .rposition(|row| row.starts_with('←') && row.contains("✔ Submit"))?;
    let end = screen_rows[tab_bar + 1..]
        .iter()
        .position(|row| row.starts_with("──"))
        .map_or(screen_rows.len(), |offset| tab_bar + 1 + offset);
    Some(end)
}

/// Claude Ink AskUserQuestion uses arrow navigation and Enter, even though its
/// displayed options are numbered. Parse only while its column-zero footer is
/// present; descriptions and wrapped text may separate option rows.
///
/// The Submit review that closes a multi-question form (`← ☒ A  ☒ B  ✔ Submit →`
/// over `Ready to submit your answers?`) has no footer. It is anchored on that
/// column-zero tab bar instead and ends at the next separator row.
pub fn parse_claude_ask_user_question(screen_rows: &[String]) -> Option<ParsedEvent> {
    let footer = screen_rows
        .iter()
        .rposition(|row| is_ink_dialog_footer_row(row))
        .or_else(|| submit_review_end(screen_rows))?;
    lazy_static::lazy_static! {
        static ref OPTION_RE: regex::Regex =
            regex::Regex::new(r"^\s*([❯›>]\s*)?(\d+)[.)]\s+(.+?)\s*$").unwrap();
    }
    // Claude replaces a clipped option's cursor with a column-zero scroll
    // arrow. Hidden rows are not an ordered choice list: expose only the
    // footer's native navigation instead of guessing labels or cursor offsets.
    let last_first_option = (0..footer).rev().find(|&idx| {
        OPTION_RE
            .captures(&screen_rows[idx])
            .is_some_and(|caps| &caps[2] == "1")
    });
    let clipped = (last_first_option.unwrap_or(0)..footer).rev().find(|&idx| {
        let row = &screen_rows[idx];
        row.strip_prefix('↑')
            .or_else(|| row.strip_prefix('↓'))
            .is_some_and(|rest| OPTION_RE.is_match(rest))
    });
    let clipped_start = clipped.map(|idx| {
        screen_rows[..idx]
            .iter()
            .rposition(|row| row.starts_with("──"))
            .map_or(0, |separator| separator + 1)
    });
    // Native titles can be imperative ("Pick a colour"). Locate the title
    // from the active numbered form, not punctuation in earlier output or in
    // an option description. The column-zero footer/review remains the anchor.
    let first_option = if let Some(start) = clipped_start {
        (start..footer).find(|&idx| {
            let row = screen_rows[idx].trim_start_matches(['↑', '↓']);
            OPTION_RE.is_match(row)
        })
    } else {
        last_first_option
    }?;
    let title = (0..first_option)
        .rev()
        .find(|&idx| !screen_rows[idx].trim().is_empty())?;
    let title_text = screen_rows[title].trim();
    if title_text.starts_with("──") || title_text.starts_with('←') {
        return None;
    }
    let navigation = || ParsedEvent::ChoicePrompt {
        title: title_text.to_string(),
        options: [
            ("↑", "Previous option"),
            ("↓", "Next option"),
            ("↵", "Select highlighted option"),
        ]
        .into_iter()
        .map(|(key, label)| ChoiceOption {
            key: key.to_string(),
            label: label.to_string(),
            highlighted: false,
            destructive: false,
            hint: None,
        })
        .collect(),
        selection_mode: Some(ChoiceSelectionMode::NavigateOnly),
        dismiss_key: Some("cancel".to_string()),
        amend_key: None,
    };
    if clipped.is_some() {
        return Some(navigation());
    }
    let mut options = Vec::new();
    for row in &screen_rows[title + 1..footer] {
        let Some(caps) = OPTION_RE.captures(row) else {
            continue;
        };
        let key = caps[2].to_string();
        if key.parse::<usize>().ok() != Some(options.len() + 1) {
            // At the last visible option Claude can omit the scroll arrow while
            // still clipping the native text alternative. Its separate Chat
            // row is an exact native boundary, not an arbitrary numbered gap.
            let native_chat_boundary = key == "5"
                && caps[3].trim() == "Chat about this"
                && screen_rows
                    .get(footer)
                    .is_some_and(|row| is_ink_dialog_footer_row(row))
                && screen_rows[title + 1..footer]
                    .iter()
                    .any(|row| row.starts_with("──"));
            return native_chat_boundary.then(navigation);
        }
        let label = caps[3].trim().to_string();
        let lower = label.to_lowercase();
        options.push(ChoiceOption {
            key,
            label,
            highlighted: caps.get(1).is_some(),
            destructive: matches!(lower.as_str(), "no" | "cancel" | "reject")
                || lower.starts_with("no "),
            hint: None,
        });
    }
    if options.len() < 2 {
        return None;
    }
    if !options.iter().any(|option| option.highlighted) {
        options[0].highlighted = true;
    }
    Some(ParsedEvent::ChoicePrompt {
        title: screen_rows[title].trim().to_string(),
        options,
        selection_mode: Some(ChoiceSelectionMode::NavigateEnter),
        dismiss_key: Some("cancel".to_string()),
        amend_key: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_error_provider_translated_auth() {
        let mut parser = OutputParser::new();
        let input = "The API provider is not able to authenticate you. Check your API key.";
        let events = parser.parse(input);
        let (name, _, kind) = get_api_error(&events).expect("should detect provider auth msg");
        assert_eq!(name, "provider-auth-msg");
        assert_eq!(kind, "auth");
    }

    #[test]
    fn test_api_error_provider_translated_server() {
        let mut parser = OutputParser::new();
        let input = "The API provider's servers are down or overloaded.";
        let events = parser.parse(input);
        let (name, _, kind) =
            get_api_error(&events).expect("should detect provider translated msg");
        assert_eq!(name, "provider-server-msg");
        assert_eq!(kind, "server");
    }

    #[test]
    fn test_api_error_provider_auth() {
        let mut parser = OutputParser::new();
        let input = "litellm.AuthenticationError: AnthropicException - invalid x-api-key";
        let events = parser.parse(input);
        let (name, _, kind) = get_api_error(&events).expect("should detect provider auth error");
        assert_eq!(name, "provider-auth-error");
        assert_eq!(kind, "auth");
    }

    #[test]
    fn test_api_error_provider_server() {
        let mut parser = OutputParser::new();
        let input = r#"litellm.InternalServerError: AnthropicException - {"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#;
        let events = parser.parse(input);
        let (name, _, kind) = get_api_error(&events).expect("should detect provider server error");
        assert_eq!(name, "provider-server-error");
        assert_eq!(kind, "server");
    }

    #[test]
    fn test_rate_limit_claude_429() {
        let mut parser = OutputParser::new();
        let events = parser.parse("Error: rate_limit_error - please try again");
        assert_eq!(events.len(), 1);
        match &events[0] {
            ParsedEvent::RateLimit { pattern_name, .. } => {
                assert_eq!(pattern_name, "claude-http-429");
            }
            _ => panic!("Expected RateLimit event"),
        }
    }

    #[test]
    fn test_rate_limit_with_retry_after() {
        let mut parser = OutputParser::new();
        let events = parser.parse("retry-after: 30");
        assert_eq!(events.len(), 1);
        match &events[0] {
            ParsedEvent::RateLimit { retry_after_ms, .. } => {
                assert_eq!(*retry_after_ms, Some(30000));
            }
            _ => panic!("Expected RateLimit event"),
        }
    }

    #[test]
    fn test_rate_limit_openai() {
        let mut parser = OutputParser::new();
        let events = parser.parse("RateLimitError: Too many requests");
        assert_eq!(events.len(), 1);
        match &events[0] {
            ParsedEvent::RateLimit { pattern_name, .. } => {
                assert_eq!(pattern_name, "openai-http-429");
            }
            _ => panic!("Expected RateLimit event"),
        }
    }

    #[test]
    fn test_rate_limit_gemini() {
        let mut parser = OutputParser::new();
        let events = parser.parse("RESOURCE_EXHAUSTED: quota exceeded");
        assert_eq!(events.len(), 1);
        match &events[0] {
            ParsedEvent::RateLimit { pattern_name, .. } => {
                assert_eq!(pattern_name, "gemini-resource-exhausted");
            }
            _ => panic!("Expected RateLimit event"),
        }
    }

    #[test]
    fn test_no_rate_limit() {
        let mut parser = OutputParser::new();
        let events = parser.parse("Hello world, everything is fine");
        assert!(events.is_empty());
    }

    #[test]
    fn test_agent_session_conflict_in_use() {
        let mut parser = OutputParser::new();
        let events = parser
            .parse("Error: Session ID 48ab1308-3b52-4ed3-af0f-e67c8f4ee890 is already in use.");
        let conflict = events
            .iter()
            .find(|e| matches!(e, ParsedEvent::AgentSessionConflict { .. }))
            .expect("Expected AgentSessionConflict event");
        match conflict {
            ParsedEvent::AgentSessionConflict { kind, matched_text } => {
                assert_eq!(kind, "in-use");
                assert!(matched_text.contains("48ab1308"));
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_agent_session_conflict_not_found() {
        let mut parser = OutputParser::new();
        let events = parser
            .parse("No conversation found with session ID: ac9a1640-f3bf-4707-8b38-49ad6be4e2f1");
        let conflict = events
            .iter()
            .find(|e| matches!(e, ParsedEvent::AgentSessionConflict { .. }))
            .expect("Expected AgentSessionConflict event");
        match conflict {
            ParsedEvent::AgentSessionConflict { kind, .. } => {
                assert_eq!(kind, "not-found");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_agent_session_conflict_ignores_source_code() {
        let mut parser = OutputParser::new();
        // Indented string literal like our own regex source — must NOT fire.
        let events = parser.parse(
            "    let msg = \"Session ID abc12345-0000-0000-0000-000000000000 is already in use\";",
        );
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ParsedEvent::AgentSessionConflict { .. })),
            "source-code context should be rejected: {events:?}"
        );
    }

    #[test]
    fn test_agent_session_conflict_no_match() {
        let mut parser = OutputParser::new();
        let events = parser.parse("Session ID is used but not quite in use here");
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ParsedEvent::AgentSessionConflict { .. })),
        );
    }

    #[test]
    fn test_agent_session_conflict_ignores_diff_hunk() {
        let mut parser = OutputParser::new();
        let events = parser.parse(
            "+        \"Error: Session ID abc12345-0000-0000-0000-000000000000 is already in use.\"",
        );
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ParsedEvent::AgentSessionConflict { .. })),
            "diff hunk prefix should be rejected: {events:?}"
        );
    }

    #[test]
    fn test_agent_session_conflict_ignores_markdown_fence() {
        let mut parser = OutputParser::new();
        let events = parser.parse(
            "```\nError: Session ID abc12345-0000-0000-0000-000000000000 is already in use.\n```",
        );
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ParsedEvent::AgentSessionConflict { .. })),
            "markdown fence should be rejected: {events:?}"
        );
    }

    #[test]
    fn test_agent_session_conflict_ignores_not_found_in_code() {
        let mut parser = OutputParser::new();
        let events = parser.parse(
            "    let msg = \"No conversation found with session ID: abc12345-0000-0000-0000-000000000000\";",
        );
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ParsedEvent::AgentSessionConflict { .. })),
            "not-found variant in source-code should be rejected: {events:?}"
        );
    }

    #[test]
    fn test_github_pr_url() {
        let mut parser = OutputParser::new();
        let events = parser.parse("Created PR: https://github.com/owner/repo/pull/42");
        assert_eq!(events.len(), 1);
        match &events[0] {
            ParsedEvent::PrUrl {
                number,
                platform,
                url,
            } => {
                assert_eq!(*number, 42);
                assert_eq!(platform, "github");
                assert!(url.contains("pull/42"));
            }
            _ => panic!("Expected PrUrl event"),
        }
    }

    #[test]
    fn test_gitlab_mr_url() {
        let mut parser = OutputParser::new();
        let events = parser.parse("MR: https://gitlab.com/org/repo/-/merge_requests/7");
        assert_eq!(events.len(), 1);
        match &events[0] {
            ParsedEvent::PrUrl {
                number, platform, ..
            } => {
                assert_eq!(*number, 7);
                assert_eq!(platform, "gitlab");
            }
            _ => panic!("Expected PrUrl event"),
        }
    }

    #[test]
    fn test_osc94_progress() {
        let mut parser = OutputParser::new();
        let events = parser.parse("\x1b]9;4;1;75\x07");
        assert_eq!(events.len(), 1);
        match &events[0] {
            ParsedEvent::Progress { state, value } => {
                assert_eq!(*state, 1);
                assert_eq!(*value, 75);
            }
            _ => panic!("Expected Progress event"),
        }
    }

    #[test]
    fn test_osc94_progress_clear() {
        let mut parser = OutputParser::new();
        let events = parser.parse("\x1b]9;4;0;0\x07");
        assert_eq!(events.len(), 1);
        match &events[0] {
            ParsedEvent::Progress { state, value } => {
                assert_eq!(*state, 0);
                assert_eq!(*value, 0);
            }
            _ => panic!("Expected Progress event"),
        }
    }

    #[test]
    fn test_osc94_progress_multiple_in_chunk() {
        let chunk = "\x1b]9;4;1;5\x07some output\x1b]9;4;1;50\x07more\x1b]9;4;1;100\x07";
        let event = parse_osc94(chunk);
        match event {
            Some(ParsedEvent::Progress { state, value }) => {
                assert_eq!(state, 1);
                assert_eq!(value, 100, "should return the LAST progress value in chunk");
            }
            _ => panic!("Expected Progress event"),
        }
    }

    #[test]
    fn test_status_line_claude() {
        let mut parser = OutputParser::new();
        let events = parser.parse("* Reading files... (12s)");
        assert_eq!(events.len(), 1);
        match &events[0] {
            ParsedEvent::StatusLine {
                task_name,
                time_info,
                ..
            } => {
                assert_eq!(task_name, "Reading files");
                assert_eq!(time_info.as_deref(), Some("12s"));
            }
            _ => panic!("Expected StatusLine event"),
        }
    }

    #[test]
    fn test_status_line_with_tokens() {
        let mut parser = OutputParser::new();
        let events =
            parser.parse("* Updating synthesis phase... (57s \u{b7} \u{2193} 2.4k tokens)");
        assert_eq!(events.len(), 1);
        match &events[0] {
            ParsedEvent::StatusLine {
                task_name,
                token_info,
                ..
            } => {
                assert_eq!(task_name, "Updating synthesis phase");
                assert!(token_info.is_some());
            }
            _ => panic!("Expected StatusLine event"),
        }
    }

    #[test]
    fn test_status_line_claude_dingbat() {
        // Claude Code v2.1.63+ uses ✢ (U+2722) instead of * for status lines
        let mut parser = OutputParser::new();
        let events = parser.parse("\u{2722}Updating verification document\u{2026} (5m1s\u{b7} \u{2191} 4.6k tokens \u{b7} thought for 4s)");
        assert_eq!(events.len(), 1);
        match &events[0] {
            ParsedEvent::StatusLine {
                task_name,
                time_info,
                token_info,
                ..
            } => {
                assert_eq!(task_name, "Updating verification document");
                assert_eq!(time_info.as_deref(), Some("5m1s"));
                assert!(token_info.is_some());
            }
            _ => panic!("Expected StatusLine event, got {:?}", events[0]),
        }
    }

    #[test]
    fn test_status_line_running() {
        let mut parser = OutputParser::new();
        let events = parser.parse("[Running] npm test");
        assert_eq!(events.len(), 1);
        match &events[0] {
            ParsedEvent::StatusLine { task_name, .. } => {
                assert_eq!(task_name, "npm test");
            }
            _ => panic!("Expected StatusLine event"),
        }
    }

    #[test]
    fn test_status_line_codex_working() {
        // Codex CLI: "• Working (5s • esc to interrupt)"
        let mut parser = OutputParser::new();
        let events = parser.parse("\u{2022} Working (5s \u{2022} esc to interrupt)");
        assert_eq!(events.len(), 1);
        match &events[0] {
            ParsedEvent::StatusLine {
                task_name,
                time_info,
                ..
            } => {
                assert_eq!(task_name, "Working");
                assert_eq!(time_info.as_deref(), Some("5s"));
            }
            _ => panic!("Expected StatusLine, got {:?}", events[0]),
        }
    }

    #[test]
    fn test_status_line_codex_compound_time() {
        // Codex CLI with compound time: "• Working (4m 55s • esc to interrupt)"
        let mut parser = OutputParser::new();
        let events = parser.parse("\u{2022} Working (4m 55s \u{2022} esc to interrupt)");
        assert_eq!(events.len(), 1);
        match &events[0] {
            ParsedEvent::StatusLine {
                task_name,
                time_info,
                ..
            } => {
                assert_eq!(task_name, "Working");
                assert_eq!(time_info.as_deref(), Some("4m 55s"));
            }
            _ => panic!("Expected StatusLine, got {:?}", events[0]),
        }
    }

    #[test]
    fn test_status_line_codex_working_hollow() {
        // Codex CLI alternate spinner: "◦ Working (12s)"
        let mut parser = OutputParser::new();
        let events = parser.parse("\u{25E6} Working (12s)");
        assert_eq!(events.len(), 1);
        match &events[0] {
            ParsedEvent::StatusLine {
                task_name,
                time_info,
                ..
            } => {
                assert_eq!(task_name, "Working");
                assert_eq!(time_info.as_deref(), Some("12s"));
            }
            _ => panic!("Expected StatusLine, got {:?}", events[0]),
        }
    }

    #[test]
    fn test_status_line_goose_spinner() {
        // Goose CLI: "Honking thoughtfully...  (Ctrl+C to interrupt)"
        let mut parser = OutputParser::new();
        let events = parser.parse("Honking thoughtfully...  (Ctrl+C to interrupt)");
        assert_eq!(events.len(), 1);
        match &events[0] {
            ParsedEvent::StatusLine { task_name, .. } => {
                assert_eq!(task_name, "Honking thoughtfully");
            }
            _ => panic!("Expected StatusLine, got {:?}", events[0]),
        }
    }

    #[test]
    fn test_status_line_goose_thinking() {
        // Goose CLI fallback: "Thinking...  (Ctrl+C to interrupt)"
        let mut parser = OutputParser::new();
        let events = parser.parse("Thinking...  (Ctrl+C to interrupt)");
        assert_eq!(events.len(), 1);
        match &events[0] {
            ParsedEvent::StatusLine { task_name, .. } => {
                assert_eq!(task_name, "Thinking");
            }
            _ => panic!("Expected StatusLine, got {:?}", events[0]),
        }
    }

    #[test]
    fn test_status_line_gemini_braille_with_phrase() {
        // Gemini CLI: braille spinner + loading phrase
        let mut parser = OutputParser::new();
        let events = parser.parse("\u{280B} Analyzing your codebase");
        assert_eq!(events.len(), 1);
        match &events[0] {
            ParsedEvent::StatusLine { task_name, .. } => {
                assert_eq!(task_name, "Analyzing your codebase");
            }
            _ => panic!("Expected StatusLine, got {:?}", events[0]),
        }
    }

    #[test]
    fn test_status_line_claude_middle_dot() {
        // Claude Code uses · (U+00B7 middle dot) as one of its spinner frames
        let mut parser = OutputParser::new();
        let events = parser.parse("\u{00B7} Considering\u{2026}");
        assert_eq!(events.len(), 1);
        match &events[0] {
            ParsedEvent::StatusLine { task_name, .. } => {
                assert_eq!(task_name, "Considering");
            }
            _ => panic!("Expected StatusLine, got {:?}", events[0]),
        }
    }

    #[test]
    fn test_status_line_copilot_therefore() {
        // GitHub Copilot CLI uses ∴ (U+2234 THEREFORE) for thinking state
        let mut parser = OutputParser::new();
        let events = parser.parse("\u{2234} Thinking\u{2026}");
        assert_eq!(events.len(), 1);
        match &events[0] {
            ParsedEvent::StatusLine { task_name, .. } => {
                assert_eq!(task_name, "Thinking");
            }
            _ => panic!("Expected StatusLine, got {:?}", events[0]),
        }
    }

    #[test]
    fn test_status_line_copilot_bullet() {
        // GitHub Copilot CLI uses ● (U+25CF) for active tool calls
        let mut parser = OutputParser::new();
        let events = parser.parse("\u{25CF} Read file...");
        assert_eq!(events.len(), 1);
        match &events[0] {
            ParsedEvent::StatusLine { task_name, .. } => {
                assert_eq!(task_name, "Read file");
            }
            _ => panic!("Expected StatusLine, got {:?}", events[0]),
        }
    }

    #[test]
    fn test_status_line_amazon_q_thinking() {
        // Amazon Q: "⠹ Thinking..." (braille + ASCII dots, not Unicode ellipsis)
        let mut parser = OutputParser::new();
        let events = parser.parse("\u{2839} Thinking...");
        assert_eq!(events.len(), 1);
        match &events[0] {
            ParsedEvent::StatusLine { task_name, .. } => {
                assert_eq!(task_name, "Thinking");
            }
            _ => panic!("Expected StatusLine, got {:?}", events[0]),
        }
    }

    #[test]
    fn test_status_line_rejects_glob_pattern() {
        // Code output like ("*/*") ... must NOT be captured as a status line.
        // The `*` is not at the start of the line.
        let mut parser = OutputParser::new();
        let events = parser.parse(r#"    ("*/*") ..."#);
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ParsedEvent::StatusLine { .. })),
            "Glob pattern in code should not match status line: {:?}",
            events
        );
    }

    #[test]
    fn test_status_line_rejects_stats_suffix() {
        // Claude Code stats like ") | [C2 S31 K30 A30 M7 H..." must NOT match.
        let mut parser = OutputParser::new();
        let events = parser.parse("* ) | [C2 S31 K30 A30 M7 H...");
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ParsedEvent::StatusLine { .. })),
            "Stats suffix should be rejected by content validation: {:?}",
            events
        );
    }

    #[test]
    fn test_status_line_rejects_mid_line_asterisk() {
        // Asterisk mid-line in code should not trigger status-line detection
        let mut parser = OutputParser::new();
        let events = parser.parse(r#"  let result = a * b... done"#);
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ParsedEvent::StatusLine { .. })),
            "Mid-line asterisk should not match: {:?}",
            events
        );
    }

    #[test]
    fn test_status_line_rejects_path_with_asterisk() {
        // Paths with wildcard should not trigger
        let mut parser = OutputParser::new();
        let events = parser.parse("src/**/*.ts...");
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ParsedEvent::StatusLine { .. })),
            "Path with asterisk should not match: {:?}",
            events
        );
    }

    // All question detection is silence-based only (10s idle after `?`-ending line).
    // No instant regex detection — it causes false positives from streaming output.
    #[test]
    fn test_no_instant_question_would_you_like() {
        let mut parser = OutputParser::new();
        let events = parser.parse("Would you like to proceed?");
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ParsedEvent::Question { .. }))
        );
    }

    #[test]
    fn test_no_instant_question_do_you_want() {
        let mut parser = OutputParser::new();
        let events = parser.parse("Do you want to continue with this approach?");
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ParsedEvent::Question { .. }))
        );
    }

    #[test]
    fn test_no_instant_question_menu_choice() {
        let mut parser = OutputParser::new();
        let events = parser.parse("❯ 1. Yes, clear context and bypass permissions");
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ParsedEvent::Question { .. }))
        );
    }

    #[test]
    fn test_no_instant_question_yn_prompt() {
        let mut parser = OutputParser::new();
        let events = parser.parse("Apply changes? [Y/n]");
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ParsedEvent::Question { .. }))
        );
    }

    #[test]
    fn test_no_instant_question_inquirer_style() {
        let mut parser = OutputParser::new();
        let events = parser.parse("? Which template would you like to use?");
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ParsedEvent::Question { .. }))
        );
    }

    #[test]
    fn test_no_question_normal_output() {
        let mut parser = OutputParser::new();
        let events = parser.parse("Building project... done");
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ParsedEvent::Question { .. }))
        );
    }

    // These phrases end with `?` and are handled by the silence-based detector
    // (10s idle threshold in pty.rs), NOT by instant regex detection.
    // Instant detection of generic question-like phrases causes massive false
    // positives during AI agent streaming output.
    #[test]
    fn test_no_instant_question_want_me_to() {
        let mut parser = OutputParser::new();
        let events = parser.parse("Want me to commit these changes?");
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ParsedEvent::Question { .. })),
            "\"Want me to\" must NOT instant-detect (silence-based only)"
        );
    }

    #[test]
    fn test_no_instant_question_should_i() {
        let mut parser = OutputParser::new();
        let events = parser.parse("Should I proceed with the refactor?");
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ParsedEvent::Question { .. })),
            "\"Should I\" must NOT instant-detect (silence-based only)"
        );
    }

    #[test]
    fn test_no_instant_question_what_would_you_like() {
        let mut parser = OutputParser::new();
        let events = parser.parse("What would you like me to do next?");
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ParsedEvent::Question { .. })),
            "\"What would you like\" must NOT instant-detect (silence-based only)"
        );
    }

    #[test]
    fn test_no_instant_question_something_else() {
        let mut parser = OutputParser::new();
        let events = parser.parse("Something else?");
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ParsedEvent::Question { .. })),
            "\"Something else?\" must NOT instant-detect (silence-based only)"
        );
    }

    #[test]
    fn test_no_instant_question_what_do_you_think() {
        let mut parser = OutputParser::new();
        let events = parser.parse("What do you think?");
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ParsedEvent::Question { .. })),
            "\"What do you think\" must NOT instant-detect (silence-based only)"
        );
    }

    #[test]
    fn test_no_instant_question_whats_your_preference() {
        let mut parser = OutputParser::new();
        let events = parser.parse("What's your preference?");
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ParsedEvent::Question { .. })),
            "\"What's your\" must NOT instant-detect (silence-based only)"
        );
    }

    #[test]
    fn test_no_instant_question_shall_i() {
        let mut parser = OutputParser::new();
        let events = parser.parse("Shall I run the tests first?");
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ParsedEvent::Question { .. })),
            "\"Shall I\" must NOT instant-detect (silence-based only)"
        );
    }

    #[test]
    fn test_no_question_should_in_prose() {
        let mut parser = OutputParser::new();
        // "should" in middle of prose — not a question prompt
        let events = parser.parse("The function should handle edge cases properly.");
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ParsedEvent::Question { .. })),
            "\"should\" in prose should NOT trigger question detection"
        );
    }

    #[test]
    fn test_no_question_want_in_prose() {
        let mut parser = OutputParser::new();
        let events = parser.parse("If you want to learn more about this pattern, see the docs.");
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ParsedEvent::Question { .. })),
            "\"want\" in prose should NOT trigger question detection"
        );
    }

    #[test]
    fn test_no_false_positives() {
        let mut parser = OutputParser::new();
        // Normal terminal output should produce no events
        let events = parser.parse("ls -la\ntotal 42\ndrwxr-xr-x  5 user staff 160 Jan 1 00:00 .\n");
        assert!(events.is_empty());
    }

    #[test]
    fn test_usage_limit_weekly() {
        let mut parser = OutputParser::new();
        let events = parser
            .parse("You've used 78% of your weekly limit · resets Feb 21 at 9am (Europe/Madrid)");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, ParsedEvent::UsageLimit { percentage: 78, .. }))
        );
        match events
            .iter()
            .find(|e| matches!(e, ParsedEvent::UsageLimit { .. }))
        {
            Some(ParsedEvent::UsageLimit {
                percentage,
                limit_type,
            }) => {
                assert_eq!(*percentage, 78);
                assert_eq!(limit_type, "weekly");
            }
            _ => panic!("Expected UsageLimit event"),
        }
    }

    #[test]
    fn test_usage_limit_session() {
        let mut parser = OutputParser::new();
        let events = parser.parse("You've used 45% of your session limit");
        match events
            .iter()
            .find(|e| matches!(e, ParsedEvent::UsageLimit { .. }))
        {
            Some(ParsedEvent::UsageLimit {
                percentage,
                limit_type,
            }) => {
                assert_eq!(*percentage, 45);
                assert_eq!(limit_type, "session");
            }
            _ => panic!("Expected UsageLimit event"),
        }
    }

    #[test]
    fn test_usage_limit_with_ansi() {
        let mut parser = OutputParser::new();
        let events = parser.parse("\x1b[33mYou've used 90% of your weekly limit\x1b[0m");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, ParsedEvent::UsageLimit { percentage: 90, .. }))
        );
    }

    #[test]
    fn test_usage_limit_smart_quote() {
        let mut parser = OutputParser::new();
        let events = parser.parse("You\u{2019}ve used 50% of your weekly limit");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, ParsedEvent::UsageLimit { percentage: 50, .. }))
        );
    }

    // --- Usage exhaustion tests ---

    #[test]
    fn test_extra_usage_exhausted_with_time_and_tz() {
        let mut parser = OutputParser::new();
        let events = parser.parse("You're out of extra usage · resets 8pm (Europe/Madrid)");
        match events
            .iter()
            .find(|e| matches!(e, ParsedEvent::UsageExhausted { .. }))
        {
            Some(ParsedEvent::UsageExhausted { reset_time }) => {
                assert_eq!(reset_time.as_deref(), Some("8pm (Europe/Madrid)"));
            }
            _ => panic!("Expected UsageExhausted event, got: {:?}", events),
        }
    }

    #[test]
    fn test_extra_usage_exhausted_with_date_time_tz() {
        let mut parser = OutputParser::new();
        let events =
            parser.parse("You're out of extra usage · resets Feb 21 at 9am (Europe/Madrid)");
        match events
            .iter()
            .find(|e| matches!(e, ParsedEvent::UsageExhausted { .. }))
        {
            Some(ParsedEvent::UsageExhausted { reset_time }) => {
                assert_eq!(reset_time.as_deref(), Some("Feb 21 at 9am (Europe/Madrid)"));
            }
            _ => panic!("Expected UsageExhausted event, got: {:?}", events),
        }
    }

    #[test]
    fn test_extra_usage_exhausted_no_reset() {
        let mut parser = OutputParser::new();
        let events = parser.parse("You're out of extra usage");
        match events
            .iter()
            .find(|e| matches!(e, ParsedEvent::UsageExhausted { .. }))
        {
            Some(ParsedEvent::UsageExhausted { reset_time }) => {
                assert!(reset_time.is_none(), "expected no reset_time");
            }
            _ => panic!("Expected UsageExhausted event, got: {:?}", events),
        }
    }

    #[test]
    fn test_usage_limit_unchanged_with_reset_time() {
        // Existing UsageLimit should still work and NOT produce UsageExhausted
        let mut parser = OutputParser::new();
        let events = parser
            .parse("You've used 78% of your weekly limit · resets Feb 21 at 9am (Europe/Madrid)");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, ParsedEvent::UsageLimit { percentage: 78, .. })),
            "should still produce UsageLimit"
        );
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ParsedEvent::UsageExhausted { .. })),
            "should NOT produce UsageExhausted for percentage-based limits"
        );
    }

    #[test]
    fn test_extra_usage_exhausted_unparseable_reset() {
        let mut parser = OutputParser::new();
        let events = parser.parse("You're out of extra usage · resets soon");
        match events
            .iter()
            .find(|e| matches!(e, ParsedEvent::UsageExhausted { .. }))
        {
            Some(ParsedEvent::UsageExhausted { reset_time }) => {
                assert_eq!(reset_time.as_deref(), Some("soon"));
            }
            _ => panic!("Expected UsageExhausted event, got: {:?}", events),
        }
    }

    #[test]
    fn test_extra_usage_exhausted_with_ansi() {
        let mut parser = OutputParser::new();
        let events =
            parser.parse("\x1b[33mYou're out of extra usage · resets 8pm (Europe/Madrid)\x1b[0m");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, ParsedEvent::UsageExhausted { .. })),
            "should parse through ANSI escapes"
        );
    }

    #[test]
    fn test_out_of_usage_without_extra() {
        // "out of usage" (without "extra") should also be caught
        let mut parser = OutputParser::new();
        let events = parser.parse("You're out of usage · resets 3pm (US/Eastern)");
        match events
            .iter()
            .find(|e| matches!(e, ParsedEvent::UsageExhausted { .. }))
        {
            Some(ParsedEvent::UsageExhausted { reset_time }) => {
                assert_eq!(reset_time.as_deref(), Some("3pm (US/Eastern)"));
            }
            _ => panic!("Expected UsageExhausted event, got: {:?}", events),
        }
    }

    // --- Ink SelectInput / broadened question detection tests ---

    #[test]
    fn test_no_instant_question_ink_cursor() {
        let mut parser = OutputParser::new();
        let events = parser.parse("› 1. Create a new story");
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ParsedEvent::Question { .. }))
        );
    }

    #[test]
    fn test_no_instant_question_ascii_cursor() {
        let mut parser = OutputParser::new();
        let events = parser.parse("> 1. Yes, proceed with changes");
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ParsedEvent::Question { .. }))
        );
    }

    #[test]
    fn test_instant_question_ink_footer() {
        // Ink interactive menu footer IS detected instantly — it's ultra-specific
        // and only appears when the agent is genuinely waiting for menu selection.
        let mut parser = OutputParser::new();
        let events = parser.parse("Enter to select · ↑/↓ to navigate · Esc to cancel");
        assert!(
            events.iter().any(|e| matches!(
                e,
                ParsedEvent::Question {
                    confident: true,
                    ..
                }
            )),
            "Ink footer should be detected as confident question"
        );
    }

    #[test]
    fn test_instant_question_ink_footer_partial() {
        let mut parser = OutputParser::new();
        let events = parser.parse("Enter to select · ↑↓ to navigate");
        assert!(
            events.iter().any(|e| matches!(
                e,
                ParsedEvent::Question {
                    confident: true,
                    ..
                }
            )),
            "Partial Ink footer should also be detected"
        );
    }

    #[test]
    fn test_question_generic_question_mark_not_instant() {
        // Generic `?`-ending lines are NOT detected by the instant parser —
        // they are handled by the silence-based detector in pty.rs to avoid
        // false positives from streaming fragments like "ad?", "swap?", "?"
        let mut parser = OutputParser::new();
        let events = parser.parse("What should we do with this story?");
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ParsedEvent::Question { .. })),
            "Generic ?-ending lines should NOT trigger instant detection"
        );
    }

    #[test]
    fn test_question_generic_not_prose() {
        // Lines that look like prose/code should NOT trigger the generic ? match
        let mut parser = OutputParser::new();
        // Code comment
        assert!(
            !parser
                .parse("// should we handle this case?")
                .iter()
                .any(|e| matches!(e, ParsedEvent::Question { .. })),
            "code comment should not trigger question detection"
        );
        // Markdown list item
        assert!(
            !parser
                .parse("- What about this edge case?")
                .iter()
                .any(|e| matches!(e, ParsedEvent::Question { .. })),
            "markdown list should not trigger question detection"
        );
        // Indented code
        assert!(
            !parser
                .parse("    if condition.valid?")
                .iter()
                .any(|e| matches!(e, ParsedEvent::Question { .. })),
            "indented code should not trigger question detection"
        );
        // Backtick-wrapped code
        assert!(
            !parser
                .parse("Have you tried `foo.bar()?` instead?")
                .iter()
                .any(|e| matches!(e, ParsedEvent::Question { .. })),
            "backtick code should not trigger question detection"
        );
        // Long prose line
        let long = format!("{}?", "a".repeat(121));
        assert!(
            !parser
                .parse(&long)
                .iter()
                .any(|e| matches!(e, ParsedEvent::Question { .. })),
            "long prose should not trigger question detection"
        );
    }

    #[test]
    fn test_instant_question_ink_full_menu_block() {
        // Full Ink menu blocks ARE detected as questions now — the "Enter to select"
        // footer is ultra-specific to real interactive menus.
        let mut parser = OutputParser::new();
        let block = "\
What should we do with this story?

  1. Create a new story
› 2. Update existing story
  3. Skip it
  4. Other

Enter to select · ↑/↓ to navigate · Esc to cancel";
        let events = parser.parse(block);
        assert!(
            events.iter().any(|e| matches!(
                e,
                ParsedEvent::Question {
                    confident: true,
                    ..
                }
            )),
            "Full Ink menu block should be detected as confident question"
        );
    }

    #[test]
    fn test_no_question_blockquote_with_question() {
        let mut parser = OutputParser::new();
        assert!(
            !parser
                .parse("> Do you agree with this approach?")
                .iter()
                .any(|e| matches!(e, ParsedEvent::Question { .. }))
        );
    }

    #[test]
    fn test_no_question_bold_markdown() {
        let mut parser = OutputParser::new();
        assert!(
            !parser
                .parse("**Should we refactor this?**")
                .iter()
                .any(|e| matches!(e, ParsedEvent::Question { .. }))
        );
    }

    #[test]
    fn test_no_question_shell_prompt_with_greater_than() {
        // Shell prompts like "> command" should NOT trigger menu detection
        let mut parser = OutputParser::new();
        assert!(
            !parser
                .parse("> git status")
                .iter()
                .any(|e| matches!(e, ParsedEvent::Question { .. }))
        );
    }

    #[test]
    fn test_no_question_enter_in_prose() {
        // Prose mentioning "Enter to select" in a different context should still match,
        // but "Press Enter to continue" should NOT
        let mut parser = OutputParser::new();
        assert!(
            !parser
                .parse("Press Enter to continue installing")
                .iter()
                .any(|e| matches!(e, ParsedEvent::Question { .. }))
        );
    }

    // --- Plan file detection tests ---

    fn get_plan_path(events: &[ParsedEvent]) -> Option<String> {
        events.iter().find_map(|e| match e {
            ParsedEvent::PlanFile { path } => Some(path.clone()),
            _ => None,
        })
    }

    #[test]
    fn test_plan_file_relative() {
        let mut parser = OutputParser::new();
        let events = parser.parse("Plan saved to plans/my-feature.md");
        assert_eq!(
            get_plan_path(&events),
            Some("plans/my-feature.md".to_string())
        );
    }

    #[test]
    fn test_plan_file_dot_claude() {
        let mut parser = OutputParser::new();
        let events = parser.parse("Writing plan: .claude/plans/auth-flow.md");
        assert_eq!(
            get_plan_path(&events),
            Some(".claude/plans/auth-flow.md".to_string())
        );
    }

    #[test]
    fn test_plan_file_absolute() {
        let mut parser = OutputParser::new();
        let events = parser.parse("Created /Users/dev/project/plans/refactor.md");
        assert_eq!(
            get_plan_path(&events),
            Some("/Users/dev/project/plans/refactor.md".to_string())
        );
    }

    #[test]
    fn test_plan_file_claude_private() {
        let mut parser = OutputParser::new();
        let events = parser.parse("Plan: .claude-private/plans/serene-waterfall.md");
        assert_eq!(
            get_plan_path(&events),
            Some(".claude-private/plans/serene-waterfall.md".to_string())
        );
    }

    #[test]
    fn test_plan_file_tilde_expanded() {
        let mut parser = OutputParser::new();
        let events = parser.parse("Plan saved to ~/.claude/plans/graceful-rolling-quasar.md");
        let path = get_plan_path(&events).expect("should detect tilde plan path");
        // Tilde must be expanded to an absolute path
        assert!(!path.starts_with("~"), "tilde should be expanded: {path}");
        assert!(path.ends_with("/.claude/plans/graceful-rolling-quasar.md"));
        assert!(
            std::path::Path::new(&path).is_absolute(),
            "path should be absolute: {path}"
        );
    }

    #[test]
    fn test_plan_file_no_match() {
        let mut parser = OutputParser::new();
        let events = parser.parse("Building project... done");
        assert!(get_plan_path(&events).is_none());
    }

    #[test]
    fn test_plan_file_not_md() {
        let mut parser = OutputParser::new();
        // "plans/foo.ts" should NOT match (not a markdown file)
        let events = parser.parse("Reading plans/foo.ts");
        assert!(get_plan_path(&events).is_none());
    }

    #[test]
    fn test_plan_file_template_placeholder_rejected() {
        let mut parser = OutputParser::new();
        // Template placeholders like <file> or <filename> should NOT match
        assert!(get_plan_path(&parser.parse("plans/<file>.md")).is_none());
        assert!(get_plan_path(&parser.parse("plans/<filename>.md")).is_none());
        assert!(get_plan_path(&parser.parse("Save to .claude/plans/<name>.md")).is_none());
    }

    #[test]
    fn test_plan_file_interpolation_rejected() {
        let mut parser = OutputParser::new();
        // Shell/JS interpolation and backticks should NOT match
        assert!(get_plan_path(&parser.parse("plans/new-${i}.md")).is_none());
        assert!(get_plan_path(&parser.parse("plans/${name}.md")).is_none());
        assert!(get_plan_path(&parser.parse("plans/`cmd`.md")).is_none());
        assert!(get_plan_path(&parser.parse("Save to plans/foo-${bar}-baz.md")).is_none());
    }

    #[test]
    fn test_plan_file_glob_rejected() {
        let mut parser = OutputParser::new();
        // Glob patterns should NOT match as plan files
        assert!(get_plan_path(&parser.parse("plans/*.md")).is_none());
        assert!(get_plan_path(&parser.parse("/repo/plans/*.md")).is_none());
        assert!(get_plan_path(&parser.parse("ls plans/*.md")).is_none());
    }

    #[test]
    fn test_plan_file_trailing_punctuation_stripped() {
        let mut parser = OutputParser::new();
        // Sentence-ending period should not be included in the path
        let events = parser.parse("Piano scritto in plans/wiz-memory-integration.md.");
        assert_eq!(
            get_plan_path(&events),
            Some("plans/wiz-memory-integration.md".to_string())
        );

        // Same with tilde path
        let events = parser.parse("Fatto, piano in ~/Gits/project/plans/my-plan.md.");
        let path = get_plan_path(&events).expect("should detect plan path with trailing period");
        assert!(
            path.ends_with("/plans/my-plan.md"),
            "trailing period should be stripped: {path}"
        );
    }

    #[test]
    fn test_plan_file_trailing_comma() {
        let mut parser = OutputParser::new();
        let events = parser.parse("See plans/foo.md, plans/bar.md for details");
        assert_eq!(get_plan_path(&events), Some("plans/foo.md".to_string()));
    }

    #[test]
    fn test_plan_file_backtick_wrapped() {
        let mut parser = OutputParser::new();
        // Claude Code renders paths in backticks (inline code markdown)
        let events = parser.parse("Piano scritto in `plans/document-organizer.md`.");
        assert_eq!(
            get_plan_path(&events),
            Some("plans/document-organizer.md".to_string())
        );
    }

    #[test]
    fn test_plan_file_backtick_wrapped_absolute() {
        let mut parser = OutputParser::new();
        let events = parser.parse("Plan saved to `/Users/dev/project/plans/my-plan.md`.");
        assert_eq!(
            get_plan_path(&events),
            Some("/Users/dev/project/plans/my-plan.md".to_string())
        );
    }

    // --- False positive prevention tests ---

    fn has_rate_limit(events: &[ParsedEvent]) -> bool {
        events
            .iter()
            .any(|e| matches!(e, ParsedEvent::RateLimit { .. }))
    }

    #[test]
    fn test_no_false_positive_conversational_rate_limit() {
        let mut parser = OutputParser::new();
        // Agent discussing rate limits in prose should NOT trigger detection
        assert!(!has_rate_limit(&parser.parse(
            "The rate limit detection was triggering false positives"
        )));
        assert!(!has_rate_limit(
            &parser.parse("I fixed the rate-limited pattern matching")
        ));
        assert!(!has_rate_limit(
            &parser.parse("We should handle too many requests gracefully")
        ));
        assert!(!has_rate_limit(
            &parser.parse("The rate limiting logic needs improvement")
        ));
    }

    #[test]
    fn test_no_false_positive_code_output() {
        let mut parser = OutputParser::new();
        // Code snippets mentioning rate limits should NOT trigger
        assert!(!has_rate_limit(&parser.parse(
            "rl(\"rate-limit-keyword\", r\"rate[- ]?limit\", Some(60000))"
        )));
        assert!(!has_rate_limit(
            &parser.parse("// Handle too many requests from the API")
        ));
        assert!(!has_rate_limit(
            &parser.parse("fn handle_rate_limit(retry_after: u64) {")
        ));
    }

    #[test]
    fn test_no_false_positive_tpm_rpm_acronyms() {
        let mut parser = OutputParser::new();
        // TPM/RPM in non-rate-limit context should NOT trigger
        assert!(!has_rate_limit(&parser.parse("TPM 2.0 module detected")));
        assert!(!has_rate_limit(
            &parser.parse("RPM package manager installed")
        ));
        assert!(!has_rate_limit(&parser.parse("The disk spins at 7200 RPM")));
    }

    #[test]
    fn test_http_429_real_errors_still_detected() {
        let mut parser = OutputParser::new();
        // Real HTTP 429 errors should still be detected
        assert!(has_rate_limit(
            &parser.parse("HTTP/1.1 429 Too Many Requests")
        ));
        assert!(has_rate_limit(&parser.parse("429 Too Many Requests")));
        assert!(has_rate_limit(&parser.parse("HTTP 429")));
    }

    #[test]
    fn test_real_api_errors_still_detected() {
        let mut parser = OutputParser::new();
        // Real API error codes should still be detected
        assert!(has_rate_limit(&parser.parse("Error: rate_limit_error")));
        assert!(has_rate_limit(
            &parser.parse("overloaded_error: service busy")
        ));
        assert!(has_rate_limit(
            &parser.parse("RateLimitError: exceeded quota")
        ));
        assert!(has_rate_limit(&parser.parse("RESOURCE_EXHAUSTED")));
        assert!(has_rate_limit(&parser.parse("Retry-After: 60")));
    }

    #[test]
    fn test_claude_friendly_overload_message_detected() {
        let mut parser = OutputParser::new();
        // Claude Code's user-facing transient-overload message (Claude auto-retries
        // internally). Must be classified as rate-limit so TUIC records it without
        // injecting a competing retry.
        assert!(has_rate_limit(&parser.parse(
            "API Error: Server is temporarily limiting requests (not your usage limit) · Rate limited"
        )));
    }

    #[test]
    fn test_no_false_positive_friendly_overload_in_code() {
        let mut parser = OutputParser::new();
        // Same phrase inside source/markdown context must NOT fire.
        assert!(!has_rate_limit(&parser.parse(
            r#"        rl("claude-overloaded-friendly", r"temporarily limiting requests", Some(30000), false),"#
        )));
        assert!(!has_rate_limit(&parser.parse(
            "// handles \"temporarily limiting requests\" from Claude"
        )));
    }

    // --- Source code false-positive guard tests ---

    #[test]
    fn test_no_false_positive_rust_source_reading() {
        let mut parser = OutputParser::new();
        // Agent reading output_parser.rs — the exact lines that caused the bug
        assert!(!has_rate_limit(&parser.parse(
            r#"        rl("claude-http-429", r"(?i)rate_limit_error", Some(60000), false),"#
        )));
        assert!(!has_rate_limit(&parser.parse(
            r#"        rl("claude-overloaded", r"(?i)overloaded_error", Some(30000), false),"#
        )));
        assert!(!has_rate_limit(&parser.parse(
            r#"        rl("openai-http-429", r"RateLimitError", Some(60000), false),"#
        )));
        assert!(!has_rate_limit(&parser.parse(
            r#"        rl("gemini-resource-exhausted", r"RESOURCE_EXHAUSTED", Some(60000), false),"#
        )));
        assert!(!has_rate_limit(&parser.parse(
            r#"        rl("retry-after-header", r"(?i)Retry-After:\s*(\d+)", None, true),"#
        )));
    }

    #[test]
    fn test_no_false_positive_code_comments() {
        let mut parser = OutputParser::new();
        assert!(!has_rate_limit(&parser.parse("// Error: rate_limit_error")));
        assert!(!has_rate_limit(
            &parser.parse("# Handle RESOURCE_EXHAUSTED from Gemini")
        ));
    }

    #[test]
    fn test_no_false_positive_test_assertions() {
        let mut parser = OutputParser::new();
        assert!(!has_rate_limit(&parser.parse(
            r#"        assert!(has_rate_limit(&parser.parse("Error: rate_limit_error")));"#
        )));
        assert!(!has_rate_limit(&parser.parse(
            r#"        assert!(has_rate_limit(&parser.parse("RateLimitError: exceeded quota")));"#
        )));
    }

    #[test]
    fn test_no_false_positive_markdown_code_fences() {
        let mut parser = OutputParser::new();
        assert!(!has_rate_limit(&parser.parse(
            "```rust\nrl(\"claude-http-429\", r\"rate_limit_error\")"
        )));
        assert!(!has_rate_limit(
            &parser.parse("- `rate_limit_error` — Claude API error code")
        ));
        assert!(!has_rate_limit(
            &parser.parse("* Pattern `RateLimitError` matches OpenAI errors")
        ));
    }

    #[test]
    fn test_no_false_positive_markdown_table() {
        let mut parser = OutputParser::new();
        assert!(!has_rate_limit(&parser.parse(
            "| `claude-http-429` | `rate_limit_error` | Claude API |"
        )));
    }

    #[test]
    fn test_line_is_source_code_fn() {
        // Direct unit tests for the guard function
        assert!(line_is_source_code(
            r#"        rl("claude-http-429", r"(?i)rate_limit_error", Some(60000), false),"#
        ));
        assert!(line_is_source_code("// rate_limit_error handling"));
        assert!(line_is_source_code("# RESOURCE_EXHAUSTED"));
        assert!(line_is_source_code("fn handle_rate_limit() {"));
        assert!(line_is_source_code(
            r#"        assert!(has_rate_limit(&parser.parse("rate_limit_error")));"#
        ));
        assert!(line_is_source_code("```Error: rate_limit_error"));
        assert!(line_is_source_code(
            "- `rate_limit_error` is the error code"
        ));
        assert!(line_is_source_code("| pattern | rate_limit_error | desc |"));

        // Real errors must NOT be classified as source code
        assert!(!line_is_source_code("Error: rate_limit_error"));
        assert!(!line_is_source_code("overloaded_error: service busy"));
        assert!(!line_is_source_code("RateLimitError: exceeded quota"));
        assert!(!line_is_source_code("RESOURCE_EXHAUSTED"));
        assert!(!line_is_source_code("HTTP/1.1 429 Too Many Requests"));
        assert!(!line_is_source_code("Retry-After: 60"));
    }

    // --- API error detection tests ---

    fn has_api_error(events: &[ParsedEvent]) -> bool {
        events
            .iter()
            .any(|e| matches!(e, ParsedEvent::ApiError { .. }))
    }

    fn get_api_error(events: &[ParsedEvent]) -> Option<(&str, &str, &str)> {
        events.iter().find_map(|e| match e {
            ParsedEvent::ApiError {
                pattern_name,
                matched_text,
                error_kind,
            } => Some((
                pattern_name.as_str(),
                matched_text.as_str(),
                error_kind.as_str(),
            )),
            _ => None,
        })
    }

    #[test]
    fn test_api_error_claude_500() {
        let mut parser = OutputParser::new();
        let input = r#"API Error: 500 {"type":"error","error":{"type":"api_error","message":"Internal server error"},"request_id":"req_011CYV92oEFMbcz45mjVYssM"}"#;
        let events = parser.parse(input);
        let (name, _, kind) = get_api_error(&events).expect("should detect Claude API error");
        assert_eq!(name, "claude-api-error");
        assert_eq!(kind, "server");
    }

    #[test]
    fn test_api_error_claude_529_overloaded() {
        let mut parser = OutputParser::new();
        // 529 overloaded should be caught by rate limit (overloaded_error), not api-error
        // But the api_error JSON type should NOT match overloaded_error
        let input = r#"API Error: 529 {"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#;
        // overloaded_error is a rate limit pattern, not api-error
        assert!(!has_api_error(&parser.parse(input)));
        assert!(has_rate_limit(&parser.parse(input)));
    }

    /// Catches: Claude's final connection-loss message has no HTTP status code.
    #[test]
    fn api_error_2335_claude_connection_lost() {
        let mut parser = OutputParser::new();
        let events = parser.parse_clean_lines(
            &[row(
                0,
                "API Error: Connection lost mid-response. The response above may be incomplete.",
            )],
            true,
        );
        let (_, _, kind) =
            get_api_error(&events).expect("final connection failure must report ApiError");
        assert_eq!(kind, "server");
    }

    /// Catches: Claude's native narrow-pane error wraps before the full signature.
    #[test]
    fn api_error_2335_captured_wrapped_claude_failure() {
        let capture = crate::pty_capture::decode_capture(include_bytes!(
            "../../../src/fixtures/agent_prompts/claude-wrapped-provider-failure-2335.tcap"
        ))
        .unwrap();
        let (rows, cols) = capture.geometry.unwrap();
        assert_eq!(cols, 40);
        let mut grid = crate::terminal_grid::TerminalGrid::new(rows, cols, 2000);
        let mut detected = false;
        let mut wrapped = false;
        let mut parser = OutputParser::new();
        for record in capture.records {
            if record.direction != crate::pty_capture::CaptureDirection::Output {
                continue;
            }
            grid.process(&record.data);
            let screen = grid.screen_text_rows();
            wrapped |= screen
                .iter()
                .any(|text| text.contains("API Error: Connection to the"))
                && screen
                    .iter()
                    .any(|text| text.contains("API was lost (ECONNRESET)"));
            let clean_rows: Vec<_> = screen
                .iter()
                .enumerate()
                .map(|(index, text)| row(index, text))
                .collect();
            detected |= has_api_error(&parser.parse_clean_lines(&clean_rows, true));
        }
        assert!(wrapped, "exercise the recorded physical-row boundary");
        assert!(detected, "wrapped native failure must report ApiError");
    }

    /// Catches: final native failures missed, or ordinary quoted diagnostics turning red.
    #[test]
    fn api_error_2335_final_failures_reject_quoted_prose() {
        let lines = [
            "API Error: Connection lost mid-response. The response above may be incomplete.",
            "API Error: Connection lost before a response was produced. Try again.",
            "API Error: Request timed out. Check your internet connection and proxy settings",
            "API Error: Request timed out",
            "API Error: The response stopped arriving. The response above may be incomplete.",
            "API Error: Server error mid-response. The response above may be incomplete.",
            "API Error: Part of the response never arrived. The response above may be incomplete.",
            "API Error: The response stream was malformed. The response above may be incomplete.",
            "API Error: Connection dropped (ECONNRESET)",
            "API Error: Connection refused — a firewall or proxy may be blocking it (ECONNREFUSED)",
            "API Error: Unable to connect to API. Check your internet connection",
            "API Error: The response stalled before a response was produced. Try again.",
            "API Error: Part of the response never arrived and no response was produced. Try again.",
            "API Error: The response stream was malformed and no response was produced. Try again.",
            "Invalid API key · Fix external API key",
            "Invalid auth token · Fix external auth token",
            "Authentication required · Sign in again to continue",
            "■ stream disconnected before completion: HTTP response body failed",
            "■ request timed out",
            "■ exceeded retry limit, last status: 503 Service Unavailable",
            "■ unexpected status 500 Internal Server Error: upstream unavailable",
            "■ unexpected status 401 Unauthorized: invalid credential",
            "■ exceeded retry limit, last status: 403 Forbidden",
            "εgo: the provider returned HTTP 401: 2335 injected authentication refusal",
        ];
        let mut missed = Vec::new();
        for line in lines {
            for width in [36, 40, 128] {
                let chars: Vec<_> = line.chars().collect();
                let texts: Vec<String> = chars
                    .chunks(width)
                    .map(|part| part.iter().collect())
                    .collect();
                let rows: Vec<_> = texts
                    .iter()
                    .enumerate()
                    .map(|(i, text)| row(i, text))
                    .collect();
                let events = OutputParser::new().parse_clean_lines(&rows, true);
                if !has_api_error(&events) {
                    missed.push(format!("missed at {width} columns: {line}"));
                }
            }
            for quoted in [
                format!("The harness printed: {line}"),
                format!("> {line}"),
                format!("- `{line}`"),
            ] {
                let events = OutputParser::new().parse_clean_lines(&[row(0, &quoted)], true);
                if has_api_error(&events) {
                    missed.push(format!("false positive {quoted}"));
                }
            }
        }
        assert!(missed.is_empty(), "{}", missed.join("\n"));
    }

    /// Catches: native provider retry or unrelated OSC notifications promoted to final failure.
    #[test]
    fn api_error_2335_ego_final_notification_not_retry_or_prose() {
        let body = "the response stream failed: HTTP response body failed; review the output, then send a new prompt to continue";
        for end in ["\x07", "\x1b\\"] {
            let events = parse_osc777_notifies(&format!("\x1b]777;notify;εgo;{body}{end}"));
            assert_eq!(
                get_api_error(&events).map(|(name, _, kind)| (name, kind)),
                Some(("ego-provider-failure", "server"))
            );
        }
        for text in [
            format!("The CLI printed {body}"),
            format!("\x1b]777;notify;Other CLI;{body}\x07"),
            "\x1b]7770;ego-recovery={\"phase\":\"retrying\",\"attempt\":1,\"next_attempt_ms\":null}\x07".into(),
            "Reconnecting... 1/5 (stream disconnected before completion: request timed out)".into(),
        ] {
            assert!(!has_api_error(&parse_osc777_notifies(&text)), "{text:?}");
            assert!(!has_api_error(&OutputParser::new().parse_clean_lines(&[row(0, &text)], true)), "{text:?}");
        }
    }

    #[test]
    fn test_api_error_claude_500_friendly() {
        // Claude Code renders 5xx as prose with no JSON body. Nothing in this string
        // hits the JSON patterns, so before the friendly pattern existed the message
        // never reached auto-retry — it fell out at the keyword fast path.
        let mut parser = OutputParser::new();
        let input = "API Error: 500 Internal server error. This is a server-side issue, usually temporary — try again in a moment. If it persists, check https://status.claude.com.";
        let events = parser.parse(input);
        let (name, _, kind) = get_api_error(&events).expect("should detect friendly Claude 5xx");
        assert_eq!(name, "claude-server-error-friendly");
        assert_eq!(kind, "server");
    }

    #[test]
    fn test_api_error_claude_500_friendly_survives_wrapping() {
        // The prose wraps at the terminal width and rows are joined with '\n', so the
        // pattern must anchor on the status code that opens the message — a phrase
        // further in gets split mid-match.
        let mut parser = OutputParser::new();
        let rows = vec![
            row(0, "API Error: 500 Internal server error. This is a server-"),
            row(1, "side issue, usually temporary — try again in a moment."),
        ];
        let events = parser.parse_clean_lines(&rows, true);
        let (name, _, kind) = get_api_error(&events).expect("should detect across a row wrap");
        assert_eq!(name, "claude-server-error-friendly");
        assert_eq!(kind, "server");
    }

    #[test]
    fn test_api_error_friendly_pattern_yields_to_json_variants() {
        // The friendly pattern is deliberately last among the Claude patterns: a JSON
        // body classifies more precisely, and 529 must stay a rate limit.
        let mut parser = OutputParser::new();
        let json_500 = r#"API Error: 500 {"type":"error","error":{"type":"api_error","message":"Internal server error"}}"#;
        let events = parser.parse(json_500);
        let (name, _, _) = get_api_error(&events).expect("json 500 detected");
        assert_eq!(name, "claude-api-error");

        let mut parser = OutputParser::new();
        let json_529 = r#"API Error: 529 {"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#;
        assert!(!has_api_error(&parser.parse(json_529)));
    }

    #[test]
    fn test_no_api_error_friendly_401() {
        // Only 5xx is retryable; an auth failure must not match the friendly pattern.
        let mut parser = OutputParser::new();
        assert!(!has_api_error(&parser.parse(
            "API Error: 401 Unauthorized. Run /login to reauthenticate."
        )));
    }

    #[test]
    fn test_api_error_claude_auth() {
        let mut parser = OutputParser::new();
        let input = r#"API Error: 401 {"type":"error","error":{"type":"authentication_error","message":"Invalid bearer token"}}"#;
        let events = parser.parse(input);
        let (name, _, kind) = get_api_error(&events).expect("should detect Claude auth error");
        assert_eq!(name, "claude-auth-error");
        assert_eq!(kind, "auth");
    }

    #[test]
    fn test_api_error_gemini_unavailable() {
        let mut parser = OutputParser::new();
        let input = r#"API Error: got status: UNAVAILABLE. {"error":{"code":503,"message":"The model is overloaded."}}"#;
        let events = parser.parse(input);
        let (name, _, kind) = get_api_error(&events).expect("should detect Gemini UNAVAILABLE");
        assert_eq!(name, "gemini-server-error");
        assert_eq!(kind, "server");
    }

    #[test]
    fn test_api_error_gemini_internal() {
        let mut parser = OutputParser::new();
        let input = r#"API Error: got status: INTERNAL. {"error":{"code":500,"message":"An internal error has occurred."}}"#;
        let events = parser.parse(input);
        let (name, _, kind) = get_api_error(&events).expect("should detect Gemini INTERNAL");
        assert_eq!(name, "gemini-server-error");
        assert_eq!(kind, "server");
    }

    #[test]
    fn test_api_error_codex_stream_error() {
        let mut parser = OutputParser::new();
        let input = "⚠  stream error: exceeded retry limit, last status: 401 Unauthorized; retrying 5/5 in 3.087s…";
        let events = parser.parse(input);
        let (name, _, kind) = get_api_error(&events).expect("should detect Codex stream error");
        assert_eq!(name, "codex-stream-error");
        assert_eq!(kind, "server");
    }

    #[test]
    fn test_api_error_codex_500() {
        let mut parser = OutputParser::new();
        let input = "stream error: exceeded retry limit, last status: 500 Internal Server Error";
        let events = parser.parse(input);
        assert!(has_api_error(&events));
    }

    #[test]
    fn test_api_error_copilot_token() {
        let mut parser = OutputParser::new();
        let input = "Failed to get copilot token";
        let events = parser.parse(input);
        let (name, _, kind) = get_api_error(&events).expect("should detect Copilot token error");
        assert_eq!(name, "copilot-auth-error");
        assert_eq!(kind, "auth");
    }

    #[test]
    fn test_no_api_error_generic_request_failed() {
        // "request failed unexpectedly" is too generic — should NOT trigger api error
        // (was causing false positives on Claude Code output, see log scan 2026-03-01)
        let mut parser = OutputParser::new();
        let input = "request failed unexpectedly";
        let events = parser.parse(input);
        assert!(!has_api_error(&events));
    }

    // --- Provider-level API error tests ---

    #[test]
    fn test_api_error_openai_server_error() {
        let mut parser = OutputParser::new();
        let input = r#"{"error":{"message":"The server had an error","type":"server_error","param":null,"code":null}}"#;
        let events = parser.parse(input);
        let (name, _, kind) = get_api_error(&events).expect("should detect OpenAI server_error");
        assert_eq!(name, "openai-server-error");
        assert_eq!(kind, "server");
    }

    #[test]
    fn test_api_error_google_internal() {
        let mut parser = OutputParser::new();
        let input = r#"{"error":{"code":500,"message":"An internal error has occurred.","status":"INTERNAL"}}"#;
        let events = parser.parse(input);
        let (name, _, kind) = get_api_error(&events).expect("should detect Google INTERNAL");
        assert_eq!(name, "google-api-server");
        assert_eq!(kind, "server");
    }

    #[test]
    fn test_api_error_google_unavailable() {
        let mut parser = OutputParser::new();
        let input = r#"{"error":{"code":503,"message":"The service is currently unavailable.","status":"UNAVAILABLE"}}"#;
        let events = parser.parse(input);
        let (name, _, kind) = get_api_error(&events).expect("should detect Google UNAVAILABLE");
        assert_eq!(name, "google-api-server");
        assert_eq!(kind, "server");
    }

    #[test]
    fn test_api_error_google_auth() {
        let mut parser = OutputParser::new();
        let input = r#"{"error":{"code":401,"message":"Request had invalid authentication credentials.","status":"UNAUTHENTICATED"}}"#;
        let events = parser.parse(input);
        let (name, _, kind) = get_api_error(&events).expect("should detect Google UNAUTHENTICATED");
        assert_eq!(name, "google-api-auth");
        assert_eq!(kind, "auth");
    }

    #[test]
    fn test_api_error_openrouter() {
        let mut parser = OutputParser::new();
        let input = r#"{"error":{"code":502,"message":"Your chosen model is down","metadata":{"provider_name":"Anthropic"}}}"#;
        let events = parser.parse(input);
        let (name, _, kind) = get_api_error(&events).expect("should detect OpenRouter error");
        assert_eq!(name, "openrouter-server");
        assert_eq!(kind, "server");
    }

    #[test]
    fn test_api_error_minimax() {
        let mut parser = OutputParser::new();
        let input = r#"{"id":"abc","base_resp":{"status_code":1013,"status_msg":"internal service error"}}"#;
        let events = parser.parse(input);
        let (name, _, kind) = get_api_error(&events).expect("should detect MiniMax error");
        assert_eq!(name, "minimax-server");
        assert_eq!(kind, "server");
    }

    #[test]
    fn test_no_api_error_normal_output() {
        let mut parser = OutputParser::new();
        assert!(!has_api_error(&parser.parse("Building project... done")));
        assert!(!has_api_error(&parser.parse("ls -la\ntotal 42")));
        assert!(!has_api_error(
            &parser.parse("Hello world, everything is fine")
        ));
    }

    #[test]
    fn test_no_api_error_false_positive_source_code() {
        let mut parser = OutputParser::new();
        // Agent reading this very source file should not trigger
        assert!(!has_api_error(&parser.parse(
            "        ae(\"claude-api-error\", \"type\":\"api_error\", \"server\"),"
        )));
        assert!(!has_api_error(
            &parser.parse("// detect \"type\":\"api_error\" in JSON")
        ));
        assert!(!has_api_error(
            &parser.parse("# Handle authentication_error from Claude")
        ));
    }

    #[test]
    fn test_api_error_dedup_same_text() {
        let mut parser = OutputParser::new();
        let input = r#"API Error: 500 {"type":"error","error":{"type":"api_error","message":"Internal server error"}}"#;
        // First parse should detect the error
        assert!(has_api_error(&parser.parse(input)));
        // Same text again (e.g. prompt redraw) should be suppressed
        assert!(!has_api_error(&parser.parse(input)));
    }

    #[test]
    fn test_api_error_dedup_resets_when_cleared() {
        let mut parser = OutputParser::new();
        let error_input = r#"API Error: 500 {"type":"error","error":{"type":"api_error","message":"Internal server error"}}"#;
        // First detection
        assert!(has_api_error(&parser.parse(error_input)));
        // Suppressed on repeat
        assert!(!has_api_error(&parser.parse(error_input)));
        // The production reset: the reader calls this when the user submits.
        parser.reset_input_dedup();
        // Same error text should fire again (new agent cycle)
        assert!(has_api_error(&parser.parse(error_input)));
    }

    /// `parse_clean_lines` is the production pipeline, and none of its parsers
    /// emits `UserInput` — the dedup reset must therefore be driven from the
    /// outside. Without it the FIRST api error of a session silences every
    /// later identical one, for the whole session.
    #[test]
    fn test_parse_clean_lines_api_error_dedup_reopens_on_input_reset() {
        let mut parser = OutputParser::new();
        let rows = vec![row(0, "API Error: 500 Internal server error.")];

        assert!(
            has_api_error(&parser.parse_clean_lines(&rows, true)),
            "first API error must be reported"
        );
        assert!(
            !has_api_error(&parser.parse_clean_lines(&rows, true)),
            "the same error still on screen must stay deduped"
        );

        parser.reset_input_dedup();
        assert!(
            has_api_error(&parser.parse_clean_lines(&rows, true)),
            "the same API error after user input is a new failure and must be reported again"
        );
    }

    /// `session_conflict_fired` is a session-lifetime latch. It has to be
    /// released on user input too, or a second conflict — a new agent launched
    /// in the same tab hitting the same stale session id — is never surfaced
    /// and the no-inject mitigation never fires for it.
    #[test]
    fn test_parse_clean_lines_session_conflict_reopens_on_input_reset() {
        fn has_conflict(events: &[ParsedEvent]) -> bool {
            events
                .iter()
                .any(|e| matches!(e, ParsedEvent::AgentSessionConflict { .. }))
        }

        let mut parser = OutputParser::new();
        let rows = vec![row(
            0,
            "Error: Session ID 48ab1308-3b52-4ed3-af0f-e67c8f4ee890 is already in use.",
        )];

        assert!(
            has_conflict(&parser.parse_clean_lines(&rows, true)),
            "first session conflict must be reported"
        );
        assert!(
            !has_conflict(&parser.parse_clean_lines(&rows, true)),
            "the same conflict still in the buffer must stay deduped"
        );

        parser.reset_input_dedup();
        assert!(
            has_conflict(&parser.parse_clean_lines(&rows, true)),
            "a session conflict after the reset must be reported again"
        );
    }

    #[test]
    fn test_api_error_dedup_different_error_fires() {
        let mut parser = OutputParser::new();
        let error1 = r#"API Error: 500 {"type":"error","error":{"type":"api_error","message":"Internal server error"}}"#;
        let error2 =
            r#"{"error":{"message":"The server had an error","type":"server_error","param":null}}"#;
        assert!(has_api_error(&parser.parse(error1)));
        // Different error text should fire even without user-input reset
        assert!(has_api_error(&parser.parse(error2)));
    }

    // --- Diff output false-positive prevention tests ---

    fn has_question(events: &[ParsedEvent]) -> bool {
        events
            .iter()
            .any(|e| matches!(e, ParsedEvent::Question { .. }))
    }

    #[test]
    fn test_no_question_diff_line_with_menu_pattern() {
        let mut parser = OutputParser::new();
        // Diff line from output_parser.rs containing ") 1." pattern — NOT a real menu
        assert!(!has_question(&parser.parse(
            "462 -        // Numbered menu choices: ❯ 1. or ) 1. followed by option text"
        )));
    }

    #[test]
    fn test_no_question_diff_line_with_yn_pattern() {
        let mut parser = OutputParser::new();
        // Diff line from docs containing [Y/n] pattern — NOT a real Y/N prompt
        assert!(!has_question(
            &parser.parse("465 //GenericY/Nprompts:[Y/n],[y/N],(yes/no)")
        ));
    }

    #[test]
    fn test_no_question_diff_line_with_hardcoded_prompts() {
        let mut parser = OutputParser::new();
        // Markdown doc line in diff containing question patterns — NOT a real question
        assert!(!has_question(&parser.parse(
            r#"75 +- **Hardcoded prompts**: "Would you like to proceed?", "Do you want to...?", "Is this plan/a"#
        )));
    }

    #[test]
    fn test_no_question_diff_line_with_yn_doc() {
        let mut parser = OutputParser::new();
        // Markdown doc line in diff listing Y/N patterns — NOT a real prompt
        assert!(!has_question(
            &parser.parse("77 +- **Y/N prompts**: `[Y/n]`, `[y/N]`, `(yes/no)`")
        ));
    }

    #[test]
    fn test_no_question_diff_hunk_with_code_changes() {
        let mut parser = OutputParser::new();
        // Claude Code diff summary block — NOT a real question
        assert!(!has_question(&parser.parse(
            "⏺⎿ Added16lines,removed2lines     459          // Claude Code: \"Would you like to proceed?\" / \"Do you want to...\""
        )));
    }

    #[test]
    fn test_no_rate_limit_in_diff_output() {
        let mut parser = OutputParser::new();
        // Diff lines showing test assertions that mention RESOURCE_EXHAUSTED — NOT real errors
        assert!(!has_rate_limit(&parser.parse(
            "+        assert!(has_rate_limit(&parser.parse(\"RESOURCE_EXHAUSTED\")));"
        )));
        assert!(!has_rate_limit(&parser.parse(
            "-        assert!(has_rate_limit(&parser.parse(\"RESOURCE_EXHAUSTED\")));"
        )));
        // Diff line with line number prefix
        assert!(!has_rate_limit(&parser.parse(
            "1226         assert!(has_rate_limit(&parser.parse(\"RESOURCE_EXHAUSTED\")));"
        )));
    }

    #[test]
    fn test_no_question_unified_diff_plus_minus_lines() {
        let mut parser = OutputParser::new();
        // Unified diff lines with + or - prefix containing question patterns
        assert!(!has_question(
            &parser.parse("+        if QUESTION_RE.is_match(trimmed) {")
        ));
        assert!(!has_question(&parser.parse(
            "-        // Numbered menu choices: ❯ 1. or ) 1. followed by option text"
        )));
        assert!(!has_question(
            &parser.parse("+- **Y/N prompts**: `[Y/n]`, `[y/N]`, `(yes/no)`")
        ));
    }

    // --- Intent detection tests ---

    fn parse_intent_events(clean: &str) -> Vec<ParsedEvent> {
        parse_intent(clean, true).into_iter().collect()
    }

    fn get_intent(events: &[ParsedEvent]) -> Option<String> {
        events.iter().find_map(|e| match e {
            ParsedEvent::Intent { text, .. } => Some(text.clone()),
            _ => None,
        })
    }

    fn get_intent_title(events: &[ParsedEvent]) -> Option<String> {
        events.iter().find_map(|e| match e {
            ParsedEvent::Intent { title, .. } => title.clone(),
            _ => None,
        })
    }

    #[test]
    fn test_no_intent_normal_output() {
        assert!(get_intent(&parse_intent_events("Building project... done")).is_none());
        assert!(get_intent(&parse_intent_events("The intent is to refactor")).is_none());
    }

    // --- Plain-prefix intent tests ---

    #[test]
    fn test_intent_plain_prefix_with_ink_bullet() {
        // Claude Code (and other Ink-hosted agents) prepend every assistant
        // output line with `● ` (U+25CF). The plain-prefix parser must still
        // recognise the token even when the bullet is present.
        let events = parse_intent_events("\u{25CF} intent: wiring the parser (Parser Fix)");
        assert_eq!(get_intent(&events), Some("wiring the parser".to_string()));
        assert_eq!(get_intent_title(&events), Some("Parser Fix".to_string()));
    }

    #[test]
    fn test_intent_plain_prefix_with_record_bullet() {
        let events = parse_intent_events("\u{23FA} intent: fixing the layout bug");
        assert_eq!(
            get_intent(&events),
            Some("fixing the layout bug".to_string())
        );
    }

    #[test]
    fn test_intent_plain_prefix_with_codex_bullet() {
        // Codex decorates the first line of an assistant message with `• `
        // (U+2022), so an intent emitted as the opening line arrives bulleted.
        // Regression: this glyph was excluded from the token anchor, so Codex
        // intent capture silently never fired.
        let events =
            parse_intent_events("\u{2022} intent: avvio watcher upstream (Upstream Watcher)");
        assert_eq!(
            get_intent(&events),
            Some("avvio watcher upstream".to_string())
        );
        assert_eq!(
            get_intent_title(&events),
            Some("Upstream Watcher".to_string())
        );
    }

    #[test]
    fn test_intent_plain_prefix_with_codex_hollow_bullet() {
        // Codex alternates • (U+2022) with ◦ (U+25E6) as a blink; both anchor.
        let events = parse_intent_events("\u{25E6} intent: ricostruisco il modello");
        assert_eq!(
            get_intent(&events),
            Some("ricostruisco il modello".to_string())
        );
    }

    #[test]
    fn test_suggest_plain_prefix_with_codex_bullet() {
        let mut parser = OutputParser::new();
        let events = parser.parse("\u{2022} suggest: [ Rebuild | Retry | Abort ]");
        let items = events.iter().find_map(|e| match e {
            ParsedEvent::Suggest { items } => Some(items.clone()),
            _ => None,
        });
        assert_eq!(
            items,
            Some(vec![
                "Rebuild".to_string(),
                "Retry".to_string(),
                "Abort".to_string()
            ])
        );
    }

    #[test]
    fn test_suggest_plain_prefix_with_ink_bullet() {
        let mut parser = OutputParser::new();
        let events = parser.parse("\u{25CF} suggest: [ Rebuild | Retry | Abort ]");
        let items = events.iter().find_map(|e| match e {
            ParsedEvent::Suggest { items } => Some(items.clone()),
            _ => None,
        });
        assert_eq!(
            items,
            Some(vec![
                "Rebuild".to_string(),
                "Retry".to_string(),
                "Abort".to_string()
            ])
        );
    }

    #[test]
    fn test_intent_plain_prefix_basic() {
        let events = parse_intent_events("intent: reading the config file");
        assert_eq!(
            get_intent(&events),
            Some("reading the config file".to_string())
        );
    }

    #[test]
    fn intent_ends_at_title_closure() {
        // Catches: same-row prose after a closed title leaking into CONTEXT.
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../src/dev/fixtures/ego-intent-2060.json"
        ))
        .unwrap();
        let events = parse_intent_events(fixture["text"].as_str().unwrap());
        assert_eq!(
            get_intent(&events).as_deref(),
            Some(
                "Verificando l’ultima release e riconciliando main senza perdere il lavoro locale"
            )
        );
        assert_eq!(
            get_intent_title(&events).as_deref(),
            Some("Allineamento Agent2")
        );
    }

    #[test]
    fn intent_aside_does_not_close_an_unfinished_later_title() {
        // Catches: an earlier aside publishing a false title during repaint or hard wrap.
        let incomplete = "intent: checking (local) configuration (Config";
        assert!(!intent_row_is_complete(incomplete));
        assert_eq!(get_intent_title(&parse_intent_events(incomplete)), None);
        for input in [
            "intent: checking (local) configuration (Config\n  review)",
            "intent: checking (local) configuration (\n  Config)Next sentence",
        ] {
            let events = parse_intent_events(input);
            assert_eq!(
                get_intent(&events).as_deref(),
                Some("checking (local) configuration")
            );
            let expected = if input.contains("review") {
                "Config review"
            } else {
                "Config"
            };
            assert_eq!(get_intent_title(&events).as_deref(), Some(expected));
        }
    }

    #[test]
    fn intent_title_closure_preserves_asides_wraps_and_untitled_rows() {
        // Catches: truncating an earlier aside, losing a wrapped title, or requiring a title.
        for (input, text, title) in [
            (
                "intent: checking (local) configuration (Config)Next sentence",
                "checking (local) configuration",
                Some("Config"),
            ),
            (
                "• intent: checking configuration\n  across modules (Config)Next sentence",
                "checking configuration across modules",
                Some("Config"),
            ),
            (
                "intent: checking configuration",
                "checking configuration",
                None,
            ),
        ] {
            let events = parse_intent_events(input);
            assert_eq!(get_intent(&events).as_deref(), Some(text), "{input}");
            assert_eq!(get_intent_title(&events).as_deref(), title, "{input}");
        }
    }

    #[test]
    fn test_intent_plain_prefix_with_title() {
        let events = parse_intent_events("intent: analyzing code (Analysis)");
        assert_eq!(get_intent(&events), Some("analyzing code".to_string()));
        assert_eq!(get_intent_title(&events), Some("Analysis".to_string()));
    }

    #[test]
    fn test_intent_plain_prefix_indented_matches() {
        // Indented continuation lines produced by Ink-hosted agents must
        // still be detected: Claude Code indents every line after the
        // first by the bullet width, so plain-prefix intents emitted mid-
        // message arrive as `  intent: ...` rather than at column 0.
        assert_eq!(
            get_intent(&parse_intent_events("  intent: indented continuation")),
            Some("indented continuation".to_string()),
        );
    }

    #[test]
    fn test_intent_plain_prefix_midline_no_match() {
        // Mid-line intent: should NOT match
        assert!(get_intent(&parse_intent_events("The intent: of this code is clear")).is_none());
    }

    #[test]
    fn test_intent_plain_prefix_in_multiline() {
        // Use \r\n to simulate real PTY output (LF without CR leaves cursor at same column)
        let events =
            parse_intent_events("some output\r\nintent: debugging login flow\r\nmore output");
        assert_eq!(
            get_intent(&events),
            Some("debugging login flow".to_string())
        );
    }

    #[test]
    fn test_intent_plain_prefix_too_short_filtered() {
        assert!(get_intent(&parse_intent_events("intent: ab")).is_none());
    }

    #[test]
    fn test_intent_plain_prefix_ellipsis_filtered() {
        assert!(get_intent(&parse_intent_events("intent: ...")).is_none());
    }

    #[test]
    fn test_intent_plain_prefix_no_space_after_colon_no_match() {
        // Requires space after colon per `^intent:\s+`
        assert!(get_intent(&parse_intent_events("intent:nospace")).is_none());
    }

    // ---- The two markers the protocol forces into the same first message ----

    #[test]
    fn test_intent_after_the_ack_sentence_on_one_row() {
        // The protocol requires the ack to OPEN the first assistant message and
        // an `intent:` INSIDE that same message, so an agent that writes both as
        // one sentence run puts the token mid-row. A column-0-only anchor then
        // captured nothing at all — not a truncated intent, no intent.
        let events = parse_intent_events(
            "\u{2022} TUICommander v1.7.7 is connected. intent: fixing the parser (Parser Fix)",
        );
        assert_eq!(get_intent(&events), Some("fixing the parser".to_string()));
        assert_eq!(get_intent_title(&events), Some("Parser Fix".to_string()));
    }

    #[test]
    fn test_ack_prefix_is_the_only_prose_allowed_before_the_token() {
        // The relaxation above is that one sentence and nothing else: any other
        // leading prose keeps the token rejected.
        assert!(
            get_intent(&parse_intent_events(
                "Ready when you are. intent: reading the config"
            ))
            .is_none()
        );
        assert!(get_intent(&parse_intent_events("The intent: of this code is clear")).is_none());
    }

    #[test]
    fn test_intent_dewraps_an_ink_continuation_row() {
        // Ink-hosted agents hard-wrap their own output and indent every
        // continuation row by the bullet width. Without a rejoin the regex `$`
        // cuts the token at the wrap, so the `(title)` — the tab title — is lost.
        let events = parse_intent_events(
            "\u{2022} intent: definisco piano, story e worktree isolato prima\n  dell'implementazione (SQLite plugin)",
        );
        assert_eq!(
            get_intent(&events),
            Some(
                "definisco piano, story e worktree isolato prima dell'implementazione".to_string()
            )
        );
        assert_eq!(get_intent_title(&events), Some("SQLite plugin".to_string()));
    }

    #[test]
    fn test_intent_dewraps_both_defects_at_once() {
        // The reported shape: the ack sentence, the token mid-row, and the tail
        // wrapped onto the next row.
        let events = parse_intent_events(
            "\u{2022} TUICommander v1.7.7 is connected. intent: definisco piano, story e worktree isolato prima\n  dell'implementazione (SQLite plugin)",
        );
        assert_eq!(
            get_intent(&events),
            Some(
                "definisco piano, story e worktree isolato prima dell'implementazione".to_string()
            )
        );
        assert_eq!(get_intent_title(&events), Some("SQLite plugin".to_string()));
    }

    #[test]
    fn test_intent_dewrap_stops_at_a_closed_title() {
        // A row that already carries `(title)` is a complete token. The row
        // below it is the agent's next paragraph, never a continuation.
        let events = parse_intent_events(
            "\u{2022} intent: reading the config (Config)\n  and then some prose",
        );
        assert_eq!(get_intent(&events), Some("reading the config".to_string()));
        assert_eq!(get_intent_title(&events), Some("Config".to_string()));
    }

    #[test]
    fn test_intent_dewrap_needs_the_wrap_indent() {
        // An un-indented next row is a new logical line, not a wrap.
        let events =
            parse_intent_events("\u{2022} intent: reading the config\nunrelated output line");
        assert_eq!(get_intent(&events), Some("reading the config".to_string()));
        assert_eq!(get_intent_title(&events), None);
    }

    #[test]
    fn test_intent_dewrap_stops_at_a_bulleted_row() {
        // A bullet opens a new agent message even when Ink indents it.
        let events =
            parse_intent_events("\u{2022} intent: reading the config\n  \u{2022} next message");
        assert_eq!(get_intent(&events), Some("reading the config".to_string()));
    }

    #[test]
    fn test_structured_token_anchor_accepts_the_ack_prefixed_intent() {
        // The grid-side anchor must see the same token the parser does,
        // otherwise a half-typed ack-prefixed intent is never deferred.
        assert_eq!(
            structured_token_anchor("\u{2022} TUICommander v1.7.7 is connected. intent: x"),
            Some(StructuredTokenAnchor::Intent)
        );
        assert_eq!(
            structured_token_anchor("suggest: [ A | B ]"),
            Some(StructuredTokenAnchor::Suggest)
        );
        assert_eq!(structured_token_anchor("Ready. intent: x"), None);
    }

    // ---- False positive regression tests ----

    fn has_status_line(events: &[ParsedEvent]) -> bool {
        events
            .iter()
            .any(|e| matches!(e, ParsedEvent::StatusLine { .. }))
    }

    #[test]
    fn test_no_status_line_in_diff_output() {
        let mut parser = OutputParser::new();
        // Diff line containing * and ... in JSON — should not trigger status line
        assert!(!has_status_line(&parser.parse(
            "484 + *   - {\"type\":\"output\",\"data\":\"...\"} for raw PTY output"
        )));
    }

    #[test]
    fn test_no_status_line_in_css_comment() {
        let mut parser = OutputParser::new();
        // CSS block comment with * prefix — should not trigger status line
        assert!(!has_status_line(&parser.parse("/* Last prompt sub-row */")));
        assert!(!has_status_line(
            &parser.parse(" * Strip ANSI escape codes from text */")
        ));
    }

    #[test]
    fn test_no_status_line_in_code_listing() {
        let mut parser = OutputParser::new();
        // Code listing with line numbers and * in a comment
        assert!(!has_status_line(&parser.parse(
            "156  /* Last prompt sub-row */                                                                                                                    "
        )));
    }

    #[test]
    fn test_no_status_line_from_markdown_bullet() {
        let mut parser = OutputParser::new();
        // Markdown bullet list — should NOT trigger Codex bullet pattern
        assert!(!has_status_line(
            &parser.parse("• This is a bullet point in a list")
        ));
        assert!(!has_status_line(
            &parser.parse("  • Another nested bullet item")
        ));
    }

    #[test]
    fn test_no_rate_limit_story_429() {
        let mut parser = OutputParser::new();
        // Conversational text mentioning "story 429" — not an HTTP 429
        assert!(!has_rate_limit(
            &parser.parse("che sembrano provenire da altre sessioni (story 429")
        ));
    }

    #[test]
    fn test_no_rate_limit_ansi_bridged_429() {
        let mut parser = OutputParser::new();
        // Raw ANSI output where \S* bridges http/ to 429 through escape codes
        assert!(!has_rate_limit(&parser.parse(
            "http/\x1b[1C\x1b[39me\x1b[1C\x1b[38;2;177;185;249mstate.rs\x1b[1Cche\x1b[1Csembrano\x1b[1Cprovenire\x1b[1Cda\x1b[1Caltre\x1b[1Csessioni\x1b[1C(story\x1b[1C429"
        )));
    }

    // --- Suggest detection tests ---

    fn get_suggest(events: &[ParsedEvent]) -> Option<Vec<String>> {
        events.iter().find_map(|e| match e {
            ParsedEvent::Suggest { items } => Some(items.clone()),
            _ => None,
        })
    }

    #[test]
    fn test_no_suggest_normal_text() {
        let mut parser = OutputParser::new();
        assert!(get_suggest(&parser.parse("I suggest we refactor")).is_none());
        assert!(get_suggest(&parser.parse("Building project...")).is_none());
    }

    // --- Plain-prefix suggest tests ---

    #[test]
    fn test_suggest_plain_prefix_basic() {
        let mut parser = OutputParser::new();
        let events = parser.parse("suggest: [ Run tests | Check logs | Push changes ]");
        let items = get_suggest(&events).expect("should parse plain-prefix suggest");
        assert_eq!(items, vec!["Run tests", "Check logs", "Push changes"]);
    }

    #[test]
    fn test_suggest_brackets_bound_stray_pipes() {
        // Regression: a `suggest:` followed by mermaid / markdown pipe content
        // (||--o{, table rows) was greedily collected into 20+ garbage chips.
        // The `[ … ]` bracket bounds the token — the closing `]` terminates the
        // walk, so pipes in the surrounding output are ignored entirely.
        let mut parser = OutputParser::new();
        let events = parser.parse(
            "suggest: [ A | B | C ]\n\
             software_product ||--o{ software_signature | product_id\n\
             file_artifact ||--o{ software_instance | sha256\n",
        );
        assert_eq!(
            get_suggest(&events),
            Some(vec!["A".to_string(), "B".to_string(), "C".to_string()]),
            "only the bracketed items count; trailing pipe content is ignored"
        );
    }

    #[test]
    fn test_suggest_plain_prefix_single_item_rejected() {
        // Single item (no `|` separator) must NOT match — prevents false
        // positives on prose that happens to be bracketed.
        let mut parser = OutputParser::new();
        let events = parser.parse("suggest: [ Single option ]");
        assert!(
            get_suggest(&events).is_none(),
            "single item without | must not parse as suggest"
        );
    }

    #[test]
    fn test_suggest_plain_prefix_indented_matches() {
        // See test_intent_plain_prefix_indented_matches.
        let mut parser = OutputParser::new();
        let items = get_suggest(&parser.parse("  suggest: [ A | B ]"))
            .expect("indented suggest must match");
        assert_eq!(items, vec!["A", "B"]);
    }

    #[test]
    fn test_suggest_plain_prefix_midline_no_match() {
        let mut parser = OutputParser::new();
        assert!(get_suggest(&parser.parse("I suggest: [ A | B ]")).is_none());
    }

    #[test]
    fn test_suggest_prose_at_column_zero_no_match() {
        // A column-0 `suggest:` WITHOUT the bracketed list must NOT parse — prose
        // like "suggest: we should investigate ..." has no `[ … ]` token.
        let mut parser = OutputParser::new();
        assert!(
            get_suggest(&parser.parse("suggest: we should investigate the streaming issue"))
                .is_none(),
            "prose at column 0 without a bracketed list must not match"
        );
    }

    #[test]
    fn test_suggest_plain_prefix_trims_whitespace() {
        let mut parser = OutputParser::new();
        let events = parser.parse("suggest: [   Fix test  |  Refactor  |  Add docs   ]");
        let items = get_suggest(&events).expect("should parse");
        assert_eq!(items, vec!["Fix test", "Refactor", "Add docs"]);
    }

    // Catches: a repainted historical marker wins over the current response,
    // including when the latest list needs bounded terminal dewrapping.
    #[test]
    fn suggest_repaint_uses_latest_complete_list() {
        for latest in [
            "suggest: [ New alpha | New beta ]",
            "suggest: [ New alpha |\n  New beta ]",
            "⏺ suggest: [ New alpha | New beta ]",
        ] {
            let text = format!("suggest: [ Old alpha | Old beta ]\nresponse\n{latest}");
            let events = OutputParser::new().parse(&text);
            assert_eq!(
                get_suggest(&events),
                Some(vec!["New alpha".into(), "New beta".into()])
            );
        }
    }

    // Catches: rearming content dedup accepts a consumed marker above the
    // submitted prompt, or incorrectly blocks identical fresh choices below it.
    #[test]
    fn suggest_floor_excludes_history_but_allows_identical_fresh_choices() {
        let mut parser = OutputParser::new();
        let marker = "suggest: [ Reply alpha | Reply beta ]";
        let mut rows = vec![crate::vt_log::ChangedRow {
            row_index: 0,
            text: marker.into(),
        }];
        assert!(
            get_suggest(&parser.parse_clean_lines_with_suggest_floor(&rows, true, Some(1)))
                .is_none()
        );
        rows.push(crate::vt_log::ChangedRow {
            row_index: 2,
            text: marker.into(),
        });
        assert_eq!(
            get_suggest(&parser.parse_clean_lines_with_suggest_floor(&rows, true, Some(1))),
            Some(vec!["Reply alpha".into(), "Reply beta".into()])
        );
    }

    #[test]
    fn test_suggest_plain_prefix_in_multiline() {
        let mut parser = OutputParser::new();
        let events = parser.parse("some output\r\nsuggest: [ Option A | Option B ]\r\nmore output");
        let items = get_suggest(&events).expect("should parse in multiline");
        assert_eq!(items, vec!["Option A", "Option B"]);
    }

    #[test]
    fn test_suggest_dewraps_multiline_content() {
        // A token wider than the terminal wraps its `[ … ]` across physical
        // rows. `dewrap_suggest_brackets` rejoins the newline (plus the wrap's
        // leading whitespace) so the four items parse. The closing `]` still
        // bounds the token, so it cannot run into surrounding output.
        let input = "suggest: [ 1) Screenshot overview panel | 2) Fix scroll flicker | 3) Fix\n\
                      Cmd+Shift+M collision | 4) Manual test ]";
        let items = match parse_suggest(input, true) {
            Some(ParsedEvent::Suggest { items }) => items,
            _ => panic!("wrapped suggest content must dewrap and parse"),
        };
        assert_eq!(
            items,
            vec![
                "1) Screenshot overview panel",
                "2) Fix scroll flicker",
                "3) Fix Cmd+Shift+M collision",
                "4) Manual test"
            ]
        );
    }

    #[test]
    fn test_suggest_dewraps_realistic_wrap() {
        // The exact shape that failed in practice: a ~78-col token on a ~76-col
        // terminal, wrapping between (here) the second and third item.
        let input =
            "suggest: [ Crea v1.3.1-nightly | Lascia solo\nrolling | Indaga il gap versionato ]";
        let items = match parse_suggest(input, true) {
            Some(ParsedEvent::Suggest { items }) => items,
            _ => panic!("realistic wrapped suggest must parse"),
        };
        assert_eq!(
            items,
            vec![
                "Crea v1.3.1-nightly",
                "Lascia solo rolling",
                "Indaga il gap versionato"
            ]
        );
    }

    #[test]
    fn test_suggest_wrapped_without_closing_bracket_no_match() {
        // An opening `suggest: [` whose `]` never arrives (truncated/streaming)
        // must NOT parse — the dewrap bails without a closing bracket, so it
        // cannot absorb the unbounded output that follows.
        let input = "suggest: [ A | B\nthis is just more terminal output, not a list";
        assert!(
            parse_suggest(input, true).is_none(),
            "wrapped suggest with no closing `]` must not parse"
        );
    }

    #[test]
    fn test_suggest_wrapped_nested_bracket_no_match() {
        // A nested `[` before the closing `]` invalidates the match even when
        // wrapped, so a mermaid label split across rows can never be captured.
        let input = "suggest: [ A | EP[\"node\"\n] | C ]";
        assert!(
            parse_suggest(input, true).is_none(),
            "a nested bracket must invalidate the wrapped suggest match"
        );
    }

    #[test]
    fn test_suggest_rejects_nested_bracket() {
        // Condition 2: a `[` inside the content invalidates the match, so a
        // mermaid node label like `EP["…"]` sharing the suggest line can never
        // be captured as items.
        let input = "suggest: [ A | EP[\"node\"] | C ]";
        assert!(
            parse_suggest(input, true).is_none(),
            "a nested bracket must invalidate the suggest match"
        );
    }

    #[test]
    fn test_suggest_wrap_stops_at_new_token() {
        // The `]` terminator bounds the token — content after it (here a new
        // `intent:` token) is never absorbed into the item list.
        let input = "suggest: [ A | B | C ]\nintent: doing something";
        let items = parse_suggest(input, true);
        let items = match items {
            Some(ParsedEvent::Suggest { items }) => items,
            _ => panic!("should parse"),
        };
        assert_eq!(items, vec!["A", "B", "C"]);
    }

    #[test]
    fn test_suggest_keyword_split_mid_word() {
        // Extremely narrow terminal splits the `suggest:` keyword itself:
        // `sugges` on one row, `t:` on the next. The dewrap pass must
        // rejoin these before the outer regex tries to match.
        for split in [
            "s\nuggest:",
            "su\nggest:",
            "sug\ngest:",
            "sugg\nest:",
            "sugge\nst:",
            "sugges\nt:",
            "suggest\n:",
        ] {
            let input = format!("{split} [ A | B | C ]");
            let items = parse_suggest(&input, true);
            let items = match items {
                Some(ParsedEvent::Suggest { items }) => items,
                _ => panic!("should dewrap and parse split={split:?}"),
            };
            assert_eq!(items, vec!["A", "B", "C"], "split={split:?}");
        }
    }

    #[test]
    fn test_suggest_keyword_split_not_at_column_zero_ignored() {
        // Only dewrap when the prefix is at column 0 (after optional
        // whitespace / ● marker). Mid-line `suggest` splits are prose.
        let input = "I will sugges\nt: we should refactor | no";
        assert!(
            parse_suggest(input, true).is_none(),
            "mid-line split must NOT be dewrapped"
        );
    }

    #[test]
    fn test_dewrap_stays_borrowed_when_no_keyword_match() {
        // Hot path: most PTY chunks have newlines but no broken `suggest:`
        // keyword. `dewrap_suggest_keyword` must return `Cow::Borrowed`
        // without any heap allocation in that case (story 1275-c45e).
        let inputs = [
            "",
            "no newlines here",
            "line one\nline two\nline three",
            "prose with the word suggest inside\nbut not a split keyword",
            "  ● some bullet\nnothing to dewrap",
        ];
        for input in inputs {
            let result = super::dewrap_suggest_keyword(input);
            assert!(
                matches!(result, std::borrow::Cow::Borrowed(_)),
                "must stay borrowed for input={input:?}"
            );
        }
    }

    #[test]
    fn test_dewrap_returns_owned_only_when_it_actually_rewrites() {
        // Sanity check: the happy path (real keyword split at column 0) does
        // allocate. Complements the no-alloc test above.
        let result = super::dewrap_suggest_keyword("sugges\nt: A | B");
        assert!(
            matches!(result, std::borrow::Cow::Owned(_)),
            "must allocate when dewrap actually rewrites"
        );
        assert_eq!(result.as_ref(), "suggest: A | B");
    }

    #[test]
    fn test_dewrap_accepts_every_agent_bullet_as_leading() {
        // Same drift as the token regexes had: this leading check knew only the
        // Ink bullets, so a wrapped Codex `• sugg\nest:` stayed broken and the
        // suggest never parsed. Every glyph in AGENT_LINE_BULLET_CHARS must
        // anchor the dewrap.
        for bullet in super::AGENT_LINE_BULLET_CHARS {
            let input = format!("{bullet} sugges\nt: A | B");
            assert_eq!(
                super::dewrap_suggest_keyword(&input).as_ref(),
                format!("{bullet} suggest: A | B"),
                "bullet {bullet:?} must anchor the dewrap"
            );
        }
    }

    #[test]
    fn test_dewrap_rejoins_across_the_wrap_indent() {
        // Captured live from Codex in a 9-column pane: it soft-wraps its own
        // output with a two-space hanging indent, so the tail of the split
        // keyword does NOT start at column 0. Requiring the suffix immediately
        // after the newline dropped every narrow-terminal suggest.
        assert_eq!(
            super::dewrap_suggest_keyword("\u{2022} suggest\n  : [ X | Y | Z ]").as_ref(),
            "\u{2022} suggest: [ X | Y | Z ]"
        );
        assert_eq!(
            super::dewrap_suggest_keyword("\u{2022} sugges\n  t: [ X | Y ]").as_ref(),
            "\u{2022} suggest: [ X | Y ]"
        );
    }

    #[test]
    fn test_suggest_parses_when_the_keyword_wraps_with_indent() {
        // End-to-end shape of the live 9-column capture: the keyword split plus
        // the bracket body wrapped across three rows must still yield the items.
        let items = match parse_suggest("\u{2022} suggest\n  : [ X |\n  Y | Z ]", true) {
            Some(ParsedEvent::Suggest { items }) => items,
            _ => panic!("indented narrow-terminal wrap must parse"),
        };
        assert_eq!(items, vec!["X", "Y", "Z"]);
    }

    #[test]
    fn test_suggest_parses_when_the_colon_row_holds_nothing_else() {
        // Captured live from Codex at 9 columns (2026-07-28): the bullet plus
        // `suggest` fills the row, so the colon lands alone on the continuation
        // row and the bracket body starts on the row after that. The keyword
        // rejoin then yields `suggest:` with NOTHING after it, which the
        // trailing-space pull refused to join.
        let items = match parse_suggest(
            "\u{2022} suggest\n  :\n  [ Alpha\n  | Beta\n  |\n  Gamma ]",
            true,
        ) {
            Some(ParsedEvent::Suggest { items }) => items,
            _ => panic!("colon-alone narrow-terminal wrap must parse"),
        };
        assert_eq!(items, vec!["Alpha", "Beta", "Gamma"]);
    }

    #[test]
    fn test_colon_alone_row_still_needs_a_bracket_body() {
        // Relaxing the trailing-space run to `*` lets any column-0 `suggest:`
        // pull the next row up. The bracket + item-count guards must still
        // reject ordinary prose that happens to follow.
        assert!(
            parse_suggest("\u{2022} suggest\n  :\n  and then I will refactor", true).is_none(),
            "a colon-alone row followed by prose must not parse"
        );
    }

    #[test]
    fn test_colon_alone_row_parses_for_an_earlier_keyword_split() {
        // The colon need not be the split point: at other widths the break
        // lands mid-word and the colon still ends its row alone.
        let items = match parse_suggest("\u{2022} sugges\n  t:\n  [ A | B ]", true) {
            Some(ParsedEvent::Suggest { items }) => items,
            _ => panic!("mid-word split with a colon-alone row must parse"),
        };
        assert_eq!(items, vec!["A", "B"]);
    }

    #[test]
    fn test_colon_alone_row_rejects_prose_prefix() {
        // The column-0 anchor is what keeps prose out; the `*` relaxation must
        // not weaken it.
        assert!(
            parse_suggest("I will suggest:\n  [ A | B ]", true).is_none(),
            "leading prose must not anchor a wrapped suggest"
        );
    }

    #[test]
    fn test_dewrap_rejects_prose_before_the_split_keyword() {
        // The column-0 constraint still holds: arbitrary text before the split
        // means this is prose, not a protocol token.
        let input = "I will sugges\nt: A | B";
        assert_eq!(
            super::dewrap_suggest_keyword(input).as_ref(),
            input,
            "non-bullet, non-whitespace leading text must not dewrap"
        );
    }

    #[test]
    fn agent_line_bullet_chars_match_the_regex_class() {
        // The char array and the regex char-class body are two spellings of one
        // glyph set; drift between them is exactly the bug story 458-4d7b fixed.
        let class = regex::Regex::new(&format!(r"^[{}]$", super::AGENT_LINE_BULLETS)).unwrap();
        for bullet in super::AGENT_LINE_BULLET_CHARS {
            assert!(
                class.is_match(&bullet.to_string()),
                "{bullet:?} is in AGENT_LINE_BULLET_CHARS but not in AGENT_LINE_BULLETS"
            );
        }
        for other in ['\u{25CB}', '\u{2234}', '-', '*'] {
            assert!(
                !class.is_match(&other.to_string()),
                "{other:?} must not be treated as an agent line bullet"
            );
        }
    }

    #[test]
    fn test_suggest_short_two_items_does_not_grab_tail() {
        // The closing `]` ends the token — unrelated prose on the next row is
        // never grabbed as a continuation.
        let input = "suggest: [ A | B ]\nsome unrelated prose here";
        let items = parse_suggest(input, true);
        let items = match items {
            Some(ParsedEvent::Suggest { items }) => items,
            _ => panic!("should still parse the suggest"),
        };
        assert_eq!(items, vec!["A", "B"]);
    }

    #[test]
    fn test_suggest_canonical_roundtrips() {
        // The returned canonical line must re-parse to the same items, so a
        // re-render of the same suggest row dedups instead of re-firing.
        let input = "suggest: [ A | B | C ]";
        let (evt, canonical) = parse_suggest_with_line(input, true).expect("should parse");
        let items = match evt {
            ParsedEvent::Suggest { items } => items,
            _ => panic!("expected Suggest event"),
        };
        assert_eq!(items, vec!["A", "B", "C"]);
        let roundtrip = parse_suggest(&canonical, true).expect("canonical must re-parse");
        assert!(
            matches!(roundtrip, ParsedEvent::Suggest { items } if items == vec!["A", "B", "C"]),
            "canonical must roundtrip to the same items"
        );
    }

    #[test]
    fn test_suggest_keyword_alone_on_narrow_row() {
        // Terminal so narrow (≤9 cols) that `suggest: ` fills the row entirely;
        // item content lands on the next physical row.
        let input = "suggest: \n[ A | B | C ]";
        let items = parse_suggest(input, true);
        let items = match items {
            Some(ParsedEvent::Suggest { items }) => items,
            _ => panic!("should parse when suggest: fills its row"),
        };
        assert_eq!(items, vec!["A", "B", "C"]);
    }

    #[test]
    fn test_suggest_bullet_keyword_alone_on_narrow_row() {
        // With bullet prefix (⏺): `⏺ suggest: ` takes ~12 cols; content on next row.
        let input = "⏺ suggest: \n[ A | B | C ]";
        let items = parse_suggest(input, true);
        let items = match items {
            Some(ParsedEvent::Suggest { items }) => items,
            _ => panic!("should parse with bullet when suggest: fills row"),
        };
        assert_eq!(items, vec!["A", "B", "C"]);
    }

    // --- Agent-gating tests for parse_intent / parse_suggest ---
    // Tokens must only be parsed when agent_active=true.

    #[test]
    fn test_intent_not_parsed_without_agent() {
        assert!(
            parse_intent("intent: reading the config file", false).is_none(),
            "intent should not parse when agent_active=false"
        );
    }

    #[test]
    fn test_intent_parsed_with_agent() {
        assert!(
            parse_intent("intent: reading the config file", true).is_some(),
            "intent should parse when agent_active=true"
        );
    }

    #[test]
    fn test_suggest_not_parsed_without_agent() {
        assert!(
            parse_suggest("suggest: [ Run tests | Check logs ]", false).is_none(),
            "suggest should not parse when agent_active=false"
        );
    }

    #[test]
    fn test_suggest_parsed_with_agent() {
        assert!(
            parse_suggest("suggest: [ Run tests | Check logs ]", true).is_some(),
            "suggest should parse when agent_active=true"
        );
    }

    #[test]
    fn test_parse_clean_lines_gated_by_agent() {
        use crate::vt_log::ChangedRow;
        let mut parser = OutputParser::new();
        let rows = vec![ChangedRow {
            row_index: 0,
            text: "suggest: [ A | B | C ]".into(),
        }];

        // Without agent: should NOT be parsed
        let events = parser.parse_clean_lines(&rows, false);
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ParsedEvent::Suggest { .. })),
            "suggest should be gated when agent_active=false"
        );

        // With agent: should be parsed
        let events = parser.parse_clean_lines(&rows, true);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, ParsedEvent::Suggest { .. })),
            "suggest should parse when agent_active=true"
        );
    }

    // --- parse_clean_lines tests ---

    fn row(i: usize, text: &str) -> crate::vt_log::ChangedRow {
        crate::vt_log::ChangedRow {
            row_index: i,
            text: text.to_string(),
        }
    }

    #[test]
    fn test_parse_clean_lines_status_line() {
        let mut parser = OutputParser::new();
        let rows = vec![row(0, "* Reading files...")];
        let events = parser.parse_clean_lines(&rows, true);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, ParsedEvent::StatusLine { .. })),
            "expected StatusLine, got: {:?}",
            events
        );
    }

    #[test]
    fn test_parse_clean_lines_codex_working_with_background_terminal_suffix() {
        // Captured live from a running Codex turn. The trailing hint suffix
        // (added when Codex holds a background terminal) carries `/ps` and
        // `/stop`, which must not disqualify the status line — losing it
        // leaves a prior turn's `suggest:` sticky, which the snapshot reads
        // as completion and reports the working agent as idle.
        let mut parser = OutputParser::new();
        let rows = vec![row(
            34,
            "• Working (1m 39s • esc to interrupt) · 1 background terminal running · /ps to view · /stop to close",
        )];
        let events = parser.parse_clean_lines(&rows, true);
        assert!(
            events.iter().any(
                |e| matches!(e, ParsedEvent::StatusLine { task_name, .. } if task_name == "Working")
            ),
            "expected StatusLine(Working), got: {:?}",
            events
        );
    }

    #[test]
    fn test_parse_clean_lines_intent_with_title() {
        let mut parser = OutputParser::new();
        let rows = vec![row(0, "intent: Implementing feature (My title)")];
        let events = parser.parse_clean_lines(&rows, true);
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ParsedEvent::Intent { .. }))
        );
        assert!(matches!(
            parse_intent(&rows[0].text, true),
            Some(ParsedEvent::Intent { title: Some(_), .. })
        ));
    }

    #[test]
    fn test_parse_clean_lines_suggest() {
        let mut parser = OutputParser::new();
        let rows = vec![row(0, "suggest: [ Run tests | Review diff | Deploy ]")];
        let events = parser.parse_clean_lines(&rows, true);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, ParsedEvent::Suggest { items } if items.len() == 3)),
            "expected Suggest with 3 items, got: {:?}",
            events
        );
    }

    #[test]
    fn test_parse_clean_lines_suggest_dedups_on_rerender() {
        // The same suggest row re-rendered in a later frame must NOT re-fire —
        // dedup against the last emitted items.
        let mut parser = OutputParser::new();
        let rows = vec![row(0, "suggest: [ A | B | C ]")];

        let first = parser.parse_clean_lines(&rows, true);
        assert!(
            first
                .iter()
                .any(|e| matches!(e, ParsedEvent::Suggest { items } if items.len() == 3)),
            "first render emits the suggest"
        );

        let second = parser.parse_clean_lines(&rows, true);
        assert!(
            !second
                .iter()
                .any(|e| matches!(e, ParsedEvent::Suggest { .. })),
            "re-render of the identical suggest must be deduped"
        );
    }

    #[test]
    fn test_parse_clean_lines_suggest_filters_empty_items() {
        let mut parser = OutputParser::new();
        let rows = vec![row(0, "suggest: [ Run tests |  | Deploy ]")];
        let events = parser.parse_clean_lines(&rows, true);
        let items = events.iter().find_map(|e| match e {
            ParsedEvent::Suggest { items } => Some(items.clone()),
            _ => None,
        });
        assert_eq!(
            items.as_deref(),
            Some(&["Run tests".to_string(), "Deploy".to_string()][..]),
            "empty items should be filtered; got: {:?}",
            items
        );
    }

    #[test]
    fn test_parse_clean_lines_no_instant_question() {
        let mut parser = OutputParser::new();
        let rows = vec![row(0, "Would you like to proceed?")];
        let events = parser.parse_clean_lines(&rows, true);
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ParsedEvent::Question { .. })),
            "no instant question detection — silence-based only"
        );
    }

    #[test]
    fn test_parse_clean_lines_usage_limit() {
        let mut parser = OutputParser::new();
        let rows = vec![row(0, "You've used 78% of your weekly limit")];
        let events = parser.parse_clean_lines(&rows, true);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, ParsedEvent::UsageLimit { .. })),
            "expected UsageLimit, got: {:?}",
            events
        );
    }

    #[test]
    fn test_parse_clean_lines_intent_with_bullet_prefix() {
        // Claude Code prepends a bullet glyph (● U+25CF) at column 0 before the token.
        let mut parser = OutputParser::new();
        let rows = vec![row(0, "\u{25CF} intent: Implementing feature (My title)")];
        let events = parser.parse_clean_lines(&rows, true);
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ParsedEvent::Intent { .. }))
        );
        assert!(
            matches!(parse_intent(&rows[0].text, true), Some(ParsedEvent::Intent { title: Some(t), .. }) if t == "My title")
        );
    }

    #[test]
    fn test_parse_clean_lines_rate_limit() {
        let mut parser = OutputParser::new();
        let rows = vec![row(0, "Error: rate_limit_error - please try again")];
        let events = parser.parse_clean_lines(&rows, true);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, ParsedEvent::RateLimit { .. })),
            "expected RateLimit, got: {:?}",
            events
        );
    }

    #[test]
    fn test_parse_clean_lines_plan_file() {
        let mut parser = OutputParser::new();
        let rows = vec![row(0, "Reading plans/vt100-clean-parsing.md")];
        let events = parser.parse_clean_lines(&rows, true);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, ParsedEvent::PlanFile { .. })),
            "expected PlanFile, got: {:?}",
            events
        );
    }

    #[test]
    fn test_parse_clean_lines_plan_file_backtick_stripped() {
        let mut parser = OutputParser::new();
        // Backticks around the path should be stripped by parse_clean_lines
        let rows = vec![row(0, "Piano scritto in `plans/document-organizer.md`.")];
        let events = parser.parse_clean_lines(&rows, true);
        let path = events.iter().find_map(|e| match e {
            ParsedEvent::PlanFile { path } => Some(path.clone()),
            _ => None,
        });
        assert_eq!(
            path,
            Some("plans/document-organizer.md".to_string()),
            "backtick-wrapped plan path should be detected via clean_lines strip; got: {:?}",
            events
        );
    }

    #[test]
    fn test_parse_clean_lines_pr_url() {
        let mut parser = OutputParser::new();
        let rows = vec![row(
            0,
            "Pull request: https://github.com/owner/repo/pull/42",
        )];
        let events = parser.parse_clean_lines(&rows, true);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, ParsedEvent::PrUrl { .. })),
            "expected PrUrl, got: {:?}",
            events
        );
    }

    #[test]
    fn test_parse_clean_lines_multiple_events() {
        let mut parser = OutputParser::new();
        let rows = vec![
            row(0, "intent: Working on feature (Test)"),
            row(1, "* Reading files..."),
        ];
        let events = parser.parse_clean_lines(&rows, true);
        let has_status = events
            .iter()
            .any(|e| matches!(e, ParsedEvent::StatusLine { .. }));
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ParsedEvent::Intent { .. }))
        );
        assert!(has_status, "expected StatusLine event, got: {:?}", events);
    }

    /// The PTY chunk pipeline owns intent dedup; the generic row parser never
    /// emits intents, even when an Ink frame repeats or changes the marker.
    #[test]
    fn a_repainted_intent_is_left_to_the_chunk_pipeline() {
        let mut parser = OutputParser::new();
        let first = vec![row(0, "intent: Rewriting the store (Store)")];
        let intents = |events: Vec<ParsedEvent>| {
            events
                .iter()
                .filter(|e| matches!(e, ParsedEvent::Intent { .. }))
                .count()
        };

        assert_eq!(intents(parser.parse_clean_lines(&first, true)), 0);
        assert_eq!(
            intents(parser.parse_clean_lines(&first, true)),
            0,
            "the same intent repainted is the same intent"
        );

        let second = vec![row(0, "intent: Building the dialog (Dialog)")];
        assert_eq!(
            intents(parser.parse_clean_lines(&second, true)),
            0,
            "the chunk pipeline reports a new phase"
        );
    }

    // --- parse_slash_menu tests ---

    fn make_screen(rows: &[&str], total: usize) -> Vec<String> {
        let mut screen: Vec<String> = vec![String::new(); total.saturating_sub(rows.len())];
        screen.extend(rows.iter().map(|r| r.to_string()));
        screen
    }

    // Catches: a one-result ego completion is discarded, leaving the old broad menu cached.
    #[test]
    fn recorded_ego_slash_filter_keeps_the_single_goal_match() {
        let bytes = std::fs::read(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../src/fixtures/agent_prompts/ego-slash-filter-2030.tcap"),
        )
        .unwrap();
        let capture = crate::pty_capture::decode_capture(&bytes).unwrap();
        let (rows, cols) = capture.geometry.unwrap();
        let mut grid = crate::terminal_grid::TerminalGrid::new(rows, cols, 2000);
        let mut typed = String::new();
        let mut checked = false;
        for record in capture.records {
            if record.direction == crate::pty_capture::CaptureDirection::Input {
                if record.data == b"a" && typed == "/go" {
                    let screen = grid.screen_text_rows();
                    assert!(screen.iter().any(|row| row.trim() == "| /go"));
                    let ParsedEvent::SlashMenu { items } = parse_slash_menu(&screen)
                        .expect("the real /go screen must replace the unfiltered menu")
                    else {
                        panic!("expected slash menu");
                    };
                    assert_eq!(items.len(), 1);
                    assert_eq!(items[0].command, "/goal");
                    checked = true;
                    break;
                }
                if !record.data.starts_with(b"\x1b") {
                    typed.push_str(std::str::from_utf8(&record.data).unwrap());
                }
            } else {
                grid.process(&record.data);
            }
        }
        assert!(checked, "capture must contain the /go boundary");
    }

    #[test]
    fn test_slash_menu_claude_code_basic() {
        let screen = make_screen(
            &[
                "   /help      Get help with using Claude Code",
                " ❯ /review    Review your code",
                "   /clear     Clear conversation history",
            ],
            24,
        );
        let evt = parse_slash_menu(&screen).expect("should detect slash menu");
        match evt {
            ParsedEvent::SlashMenu { items } => {
                assert_eq!(items.len(), 3);
                assert_eq!(items[0].command, "/help");
                assert_eq!(items[0].description, "Get help with using Claude Code");
                assert!(!items[0].highlighted);
                assert_eq!(items[1].command, "/review");
                assert!(items[1].highlighted);
                assert_eq!(items[2].command, "/clear");
                assert!(!items[2].highlighted);
            }
            _ => panic!("Expected SlashMenu event"),
        }
    }

    #[test]
    fn test_slash_menu_no_highlight_fallback_first() {
        let screen = make_screen(
            &[
                "   /help      Get help with using Claude Code",
                "   /review    Review your code",
            ],
            24,
        );
        let evt = parse_slash_menu(&screen).expect("should detect slash menu");
        match evt {
            ParsedEvent::SlashMenu { items } => {
                assert_eq!(items.len(), 2);
                assert!(
                    items[0].highlighted,
                    "first item should be highlighted as fallback"
                );
                assert!(!items[1].highlighted);
            }
            _ => panic!("Expected SlashMenu event"),
        }
    }

    #[test]
    fn test_slash_menu_single_filtered_row_detected() {
        let screen = make_screen(&["   /help      Get help with using Claude Code"], 24);
        let Some(ParsedEvent::SlashMenu { items }) = parse_slash_menu(&screen) else {
            panic!("a filtered menu can contain only one command");
        };
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].command, "/help");
    }

    #[test]
    fn test_slash_menu_no_match() {
        let screen = make_screen(&["some random output", "another line"], 24);
        assert!(parse_slash_menu(&screen).is_none());
    }

    #[test]
    fn test_slash_menu_trailing_empty_rows() {
        let screen = make_screen(
            &[
                "   /help      Get help",
                " ❯ /review    Review code",
                "   /clear     Clear history",
                "",
                "",
            ],
            24,
        );
        let evt = parse_slash_menu(&screen).expect("should skip trailing empty rows");
        match evt {
            ParsedEvent::SlashMenu { items } => {
                assert_eq!(items.len(), 3);
            }
            _ => panic!("Expected SlashMenu event"),
        }
    }

    #[test]
    fn test_slash_menu_mixed_content_below() {
        // Menu rows are at bottom, non-menu content above
        let mut screen: Vec<String> = vec![String::new(); 20];
        screen.push("$ claude-code".to_string()); // non-menu line
        screen.push("   /help      Get help".to_string());
        screen.push(" ❯ /review    Review code".to_string());
        screen.push("   /clear     Clear history".to_string());
        let evt = parse_slash_menu(&screen).expect("should detect menu below non-menu content");
        match evt {
            ParsedEvent::SlashMenu { items } => {
                assert_eq!(items.len(), 3);
            }
            _ => panic!("Expected SlashMenu event"),
        }
    }

    #[test]
    fn test_slash_menu_codex_style_no_arrow() {
        // Codex-style: no ❯, just indented /commands
        let screen = make_screen(
            &[
                "  /help        Show help information",
                "  /edit        Edit a file",
                "  /run         Run a command",
            ],
            24,
        );
        let evt = parse_slash_menu(&screen).expect("should detect codex-style menu");
        match evt {
            ParsedEvent::SlashMenu { items } => {
                assert_eq!(items.len(), 3);
                assert!(items[0].highlighted, "first should be highlighted fallback");
                assert!(!items[1].highlighted);
                assert!(!items[2].highlighted);
            }
            _ => panic!("Expected SlashMenu event"),
        }
    }

    #[test]
    fn test_slash_menu_description_with_spaces() {
        let screen = make_screen(
            &[
                "   /compact   Compact the conversation to reduce context window usage",
                "   /review    Review a pull request with detailed feedback",
            ],
            24,
        );
        let evt = parse_slash_menu(&screen).expect("should parse long descriptions");
        match evt {
            ParsedEvent::SlashMenu { items } => {
                assert_eq!(items[0].command, "/compact");
                assert_eq!(
                    items[0].description,
                    "Compact the conversation to reduce context window usage"
                );
            }
            _ => panic!("Expected SlashMenu event"),
        }
    }

    #[test]
    fn test_slash_menu_empty_screen() {
        let screen: Vec<String> = vec![String::new(); 24];
        assert!(parse_slash_menu(&screen).is_none());
    }

    #[test]
    fn test_slash_menu_above_prompt_chrome_needs_trim() {
        // Real Claude Code screen: menu renders above the prompt chrome.
        // parse_slash_menu receives pre-trimmed rows (chrome stripped by caller).
        let mut screen: Vec<String> = vec![String::new(); 18];
        screen.push("some output".to_string());
        screen.push(String::new());
        // Menu items (these are what remains after chrome trimming)
        screen.push(
            "  /wiz:plan                   (wiz) Transform feature descriptions into well"
                .to_string(),
        );
        screen.push(
            "  /wiz:stories                (wiz) Persistent file-based task tracking with"
                .to_string(),
        );
        screen.push(
            "  /wiz:review                 (wiz) Exhaustive multi-agent code review for P"
                .to_string(),
        );
        screen.push(
            "  /wiz:work                   (wiz) Execute work plans efficiently while mai"
                .to_string(),
        );
        let evt = parse_slash_menu(&screen).expect("should detect menu from pre-trimmed screen");
        match evt {
            ParsedEvent::SlashMenu { items } => {
                assert_eq!(items.len(), 4);
                assert_eq!(items[0].command, "/wiz:plan");
                assert_eq!(items[3].command, "/wiz:work");
            }
            _ => panic!("Expected SlashMenu event"),
        }
    }

    #[test]
    fn test_slash_menu_fails_with_chrome_below() {
        // When chrome is at the very bottom (below menu), the parser can't
        // reach the menu items because it breaks on the status bar row.
        let mut screen: Vec<String> = vec![String::new(); 18];
        screen.push("  /help      Get help with using Claude Code".to_string());
        screen.push("  /review    Review your code".to_string());
        // Chrome below the menu (untrimmed)
        screen.push("────────────────────────────────────────".to_string());
        screen.push("❯ /".to_string());
        screen.push("────────────────────────────────────────".to_string());
        screen.push("  [Opus 4.6 | Max] project git:(main)".to_string());
        assert!(
            parse_slash_menu(&screen).is_none(),
            "untrimmed chrome should prevent detection"
        );
    }

    #[test]
    fn test_slash_menu_below_chrome_cc_v2() {
        // Claude Code v2.1+ renders the autocomplete menu BELOW the prompt
        // chrome. The parser receives the full (untrimmed) screen and must
        // find the menu items by scanning bottom-up past trailing empties.
        let mut screen: Vec<String> = vec![String::new(); 4]; // banner rows
        screen[0] = "➜  tuicommander git:(main) ✗ c".into();
        screen[1] = " ▐▛███▜▌   Claude Code v2.1.104".into();
        screen[2] = "▝▜█████▛▘  Opus 4.6".into();
        screen[3] = "  ▘▘ ▝▝    ~/Gits/personal/tuicommander".into();
        screen.push(String::new()); // empty
        screen.push("─────────────────────────────────────────────────".into());
        screen.push("❯ /".into());
        screen.push("─────────────────────────────────────────────────".into());
        screen.push("/wiz:plan                             (wiz) Transform feature descriptions into well-structured project plans".into());
        screen.push("/wiz:work                             (wiz) Execute work plans efficiently while maintaining quality".into());
        screen.push("/wiz:review                           (wiz) Exhaustive multi-agent code review for PRs".into());
        screen.push(
            "/wiz:stories                          (wiz) Persistent file-based task tracking"
                .into(),
        );
        screen.push("/add-dir                              Add a new working directory".into());
        // Trailing empty rows (terminal padding)
        for _ in 0..20 {
            screen.push(String::new());
        }
        let evt = parse_slash_menu(&screen).expect("should detect menu below chrome");
        match evt {
            ParsedEvent::SlashMenu { items } => {
                assert_eq!(items.len(), 5);
                assert_eq!(items[0].command, "/wiz:plan");
                assert_eq!(items[4].command, "/add-dir");
            }
            _ => panic!("Expected SlashMenu event"),
        }
    }

    #[test]
    fn test_slash_menu_wrapped_descriptions() {
        // Real-world scenario: descriptions wrap when terminal is narrower than
        // the description text. The parser must skip continuation lines.
        let mut screen: Vec<String> = vec![String::new(); 4];
        screen.push("──────────────────────────────────────────────────────────────────────────────────────────────────────────────────".into());
        screen.push("❯\u{a0}/".into());
        screen.push("──────────────────────────────────────────────────────────────────────────────────────────────────────────────────".into());
        screen.push("/wiz:stories                  (wiz) Persistent file-based task tracking with merge-safe IDs, dependency".into());
        screen.push("                              awareness, and unattended auto-continue across sessions. Use when managing stor…".into());
        screen.push("/handoff                      (wiz) Use at session end to write handoff + capture session memory to mdkb".into());
        screen.push("/wiz:work                     (wiz) Execute work plans efficiently while maintaining quality with systematic".into());
        screen.push("                              task tracking".into());
        for _ in 0..20 {
            screen.push(String::new());
        }
        let evt = parse_slash_menu(&screen).expect("should detect menu with wrapped descriptions");
        match evt {
            ParsedEvent::SlashMenu { items } => {
                assert_eq!(items.len(), 3);
                assert_eq!(items[0].command, "/wiz:stories");
                assert_eq!(items[1].command, "/handoff");
                assert_eq!(items[2].command, "/wiz:work");
            }
            _ => panic!("Expected SlashMenu event"),
        }
    }

    #[test]
    fn test_slash_menu_continuation_only_after_item() {
        // A continuation-like row before any menu item should break the scan
        let screen = make_screen(
            &[
                "                              some random indented text",
                "/help      Get help",
                "/clear     Clear history",
            ],
            24,
        );
        let evt = parse_slash_menu(&screen).expect("should still detect menu items");
        match evt {
            ParsedEvent::SlashMenu { items } => {
                assert_eq!(items.len(), 2);
            }
            _ => panic!("Expected SlashMenu event"),
        }
    }

    #[test]
    fn test_slash_menu_chrome_not_skipped_as_continuation() {
        // Chrome lines (2-space indent) must NOT be treated as continuation.
        // If they were, the parser would skip past chrome and find stale
        // menu items from a previous render → false positive.
        let mut screen: Vec<String> = vec![String::new(); 20];
        screen.push("/help      Get help".to_string());
        screen.push("/clear     Clear history".to_string());
        screen.push("──────────────────────────────────────".to_string());
        screen.push("❯".to_string());
        screen.push("──────────────────────────────────────".to_string());
        screen.push("  [Opus 4.6 | Team] ██████░░░░ 64%".to_string());
        screen.push("  ⏵⏵ bypass permissions on (shift+tab to cycle)".to_string());
        assert!(
            parse_slash_menu(&screen).is_none(),
            "chrome lines should stop the scan"
        );
    }

    // ── ActiveSubtasks tests ──────────────────────────────────────────

    #[test]
    fn test_active_subtasks_local_agents() {
        let mut parser = OutputParser::new();
        let events = parser.parse("\u{203A}\u{203A} bypass permissions on \u{00B7} 2 local agents");
        match events
            .iter()
            .find(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. }))
        {
            Some(ParsedEvent::ActiveSubtasks { count, task_type }) => {
                assert_eq!(*count, 2);
                assert_eq!(task_type, "local agents");
            }
            _ => panic!("Expected ActiveSubtasks event, got: {:?}", events),
        }
    }

    #[test]
    fn test_active_subtasks_single_bash() {
        let mut parser = OutputParser::new();
        let events = parser.parse("\u{203A}\u{203A} reading config files \u{00B7} 1 bash");
        match events
            .iter()
            .find(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. }))
        {
            Some(ParsedEvent::ActiveSubtasks { count, task_type }) => {
                assert_eq!(*count, 1);
                assert_eq!(task_type, "bash");
            }
            _ => panic!("Expected ActiveSubtasks event, got: {:?}", events),
        }
    }

    #[test]
    fn test_active_subtasks_background_tasks() {
        let mut parser = OutputParser::new();
        let events = parser.parse("\u{203A}\u{203A} fixing tests \u{00B7} 3 background tasks");
        match events
            .iter()
            .find(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. }))
        {
            Some(ParsedEvent::ActiveSubtasks { count, task_type }) => {
                assert_eq!(*count, 3);
                assert_eq!(task_type, "background tasks");
            }
            _ => panic!("Expected ActiveSubtasks event, got: {:?}", events),
        }
    }

    #[test]
    fn test_active_subtasks_single_local_agent() {
        let mut parser = OutputParser::new();
        let events = parser.parse("\u{203A}\u{203A} writing code \u{00B7} 1 local agent");
        match events
            .iter()
            .find(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. }))
        {
            Some(ParsedEvent::ActiveSubtasks { count, task_type }) => {
                assert_eq!(*count, 1);
                assert_eq!(task_type, "local agent");
            }
            _ => panic!("Expected ActiveSubtasks event, got: {:?}", events),
        }
    }

    #[test]
    fn test_active_subtasks_bare_mode_line_resets_to_zero() {
        let mut parser = OutputParser::new();
        // ›› line without · N type → count=0
        let events = parser.parse("\u{203A}\u{203A} bypass permissions on");
        match events
            .iter()
            .find(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. }))
        {
            Some(ParsedEvent::ActiveSubtasks { count, .. }) => {
                assert_eq!(*count, 0);
            }
            _ => panic!("Expected ActiveSubtasks with count=0, got: {:?}", events),
        }
    }

    #[test]
    fn test_active_subtasks_explicit_zero_count() {
        let mut parser = OutputParser::new();
        // ›› mode · 0 bash → count=0 (sub-tasks finished, via count-regex branch)
        let events = parser.parse("\u{203A}\u{203A} finishing \u{00B7} 0 bash");
        match events
            .iter()
            .find(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. }))
        {
            Some(ParsedEvent::ActiveSubtasks { count, .. }) => {
                assert_eq!(*count, 0);
            }
            _ => panic!("Expected ActiveSubtasks with count=0, got: {:?}", events),
        }
    }

    #[test]
    fn test_active_subtasks_embedded_in_multiline_output() {
        let mut parser = OutputParser::new();
        let input =
            "some other output\n\u{203A}\u{203A} working \u{00B7} 2 bash\nmore output after";
        let events = parser.parse(input);
        match events
            .iter()
            .find(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. }))
        {
            Some(ParsedEvent::ActiveSubtasks { count, task_type }) => {
                assert_eq!(*count, 2);
                assert_eq!(task_type, "bash");
            }
            _ => panic!(
                "Expected ActiveSubtasks in multiline input, got: {:?}",
                events
            ),
        }
    }

    #[test]
    fn test_active_subtasks_not_triggered_by_regular_text() {
        let mut parser = OutputParser::new();
        let events = parser.parse("some regular output with \u{203A} single guillemet");
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. })),
            "Should not match single guillemet: {:?}",
            events
        );
    }

    // ── ActiveSubtasks tests with ⏵⏵ (U+23F5) prefix ──────────────────

    #[test]
    fn test_active_subtasks_triangle_local_agents() {
        let mut parser = OutputParser::new();
        let events = parser.parse("\u{23F5}\u{23F5} bypass permissions on \u{00B7} 2 local agents");
        match events
            .iter()
            .find(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. }))
        {
            Some(ParsedEvent::ActiveSubtasks { count, task_type }) => {
                assert_eq!(*count, 2);
                assert_eq!(task_type, "local agents");
            }
            _ => panic!(
                "Expected ActiveSubtasks event with ⏵⏵ prefix, got: {:?}",
                events
            ),
        }
    }

    #[test]
    fn test_active_subtasks_triangle_bare_resets_to_zero() {
        let mut parser = OutputParser::new();
        let events = parser.parse("\u{23F5}\u{23F5} bypass permissions on");
        match events
            .iter()
            .find(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. }))
        {
            Some(ParsedEvent::ActiveSubtasks { count, .. }) => {
                assert_eq!(*count, 0);
            }
            _ => panic!(
                "Expected ActiveSubtasks with count=0 for ⏵⏵ prefix, got: {:?}",
                events
            ),
        }
    }

    #[test]
    fn test_active_subtasks_triangle_single_bash() {
        let mut parser = OutputParser::new();
        let events = parser.parse("\u{23F5}\u{23F5} reading config files \u{00B7} 1 bash");
        match events
            .iter()
            .find(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. }))
        {
            Some(ParsedEvent::ActiveSubtasks { count, task_type }) => {
                assert_eq!(*count, 1);
                assert_eq!(task_type, "bash");
            }
            _ => panic!(
                "Expected ActiveSubtasks event with ⏵⏵ prefix, got: {:?}",
                events
            ),
        }
    }

    #[test]
    fn test_active_subtasks_not_triggered_by_single_triangle() {
        let mut parser = OutputParser::new();
        let events = parser.parse("some output with \u{23F5} single triangle");
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. })),
            "Should not match single ⏵: {:?}",
            events
        );
    }

    #[test]
    fn test_active_subtasks_triangle_explicit_zero_count() {
        let mut parser = OutputParser::new();
        let events = parser.parse("\u{23F5}\u{23F5} finishing \u{00B7} 0 bash");
        match events
            .iter()
            .find(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. }))
        {
            Some(ParsedEvent::ActiveSubtasks { count, .. }) => assert_eq!(*count, 0),
            _ => panic!("Expected ActiveSubtasks with count=0, got: {:?}", events),
        }
    }

    #[test]
    fn test_active_subtasks_triangle_embedded_in_multiline() {
        let mut parser = OutputParser::new();
        let input =
            "some other output\n\u{23F5}\u{23F5} working \u{00B7} 2 bash\nmore output after";
        let events = parser.parse(input);
        match events
            .iter()
            .find(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. }))
        {
            Some(ParsedEvent::ActiveSubtasks { count, task_type }) => {
                assert_eq!(*count, 2);
                assert_eq!(task_type, "bash");
            }
            _ => panic!(
                "Expected ActiveSubtasks in multiline input, got: {:?}",
                events
            ),
        }
    }

    #[test]
    fn test_active_subtasks_triangle_background_tasks() {
        let mut parser = OutputParser::new();
        let events = parser.parse("\u{23F5}\u{23F5} fixing tests \u{00B7} 3 background tasks");
        match events
            .iter()
            .find(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. }))
        {
            Some(ParsedEvent::ActiveSubtasks { count, task_type }) => {
                assert_eq!(*count, 3);
                assert_eq!(task_type, "background tasks");
            }
            _ => panic!(
                "Expected ActiveSubtasks event with ⏵⏵ prefix, got: {:?}",
                events
            ),
        }
    }

    // --- ActiveSubtasks: new format (count LEFT of markers) — captured from live sessions ---

    #[test]
    fn test_active_subtasks_new_format_count_left() {
        // Real: "1 shell · ⏵⏵ bypass permissions on" (CC v2.1.81, 2026-03-21)
        let mut parser = OutputParser::new();
        let events = parser.parse("1 shell \u{00B7} \u{23F5}\u{23F5} bypass permissions on");
        match events
            .iter()
            .find(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. }))
        {
            Some(ParsedEvent::ActiveSubtasks { count, task_type }) => {
                assert_eq!(*count, 1);
                assert_eq!(task_type, "shell");
            }
            _ => panic!(
                "Expected ActiveSubtasks with count-left format, got: {:?}",
                events
            ),
        }
    }

    #[test]
    fn test_active_subtasks_new_format_plural() {
        let mut parser = OutputParser::new();
        let events = parser.parse("2 local agents \u{00B7} \u{23F5}\u{23F5} bypass permissions on");
        match events
            .iter()
            .find(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. }))
        {
            Some(ParsedEvent::ActiveSubtasks { count, task_type }) => {
                assert_eq!(*count, 2);
                assert_eq!(task_type, "local agents");
            }
            _ => panic!(
                "Expected ActiveSubtasks with count-left plural, got: {:?}",
                events
            ),
        }
    }

    #[test]
    fn test_active_subtasks_new_format_with_hint() {
        // Mode line with both subprocess count and shift+tab hint
        let mut parser = OutputParser::new();
        let events = parser
            .parse("1 shell \u{00B7} \u{23F5}\u{23F5} bypass permissions on (shift+tab to cycle)");
        match events
            .iter()
            .find(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. }))
        {
            Some(ParsedEvent::ActiveSubtasks { count, task_type }) => {
                assert_eq!(*count, 1);
                assert_eq!(task_type, "shell");
            }
            _ => panic!(
                "Expected ActiveSubtasks with hint suffix, got: {:?}",
                events
            ),
        }
    }

    #[test]
    fn test_active_subtasks_mode_line_with_hint_no_count() {
        // "⏵⏵ bypass permissions on (shift+tab to cycle)" — no subprocess count
        let mut parser = OutputParser::new();
        let events = parser.parse("\u{23F5}\u{23F5} bypass permissions on (shift+tab to cycle)");
        match events
            .iter()
            .find(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. }))
        {
            Some(ParsedEvent::ActiveSubtasks { count, .. }) => {
                assert_eq!(*count, 0);
            }
            _ => panic!("Expected ActiveSubtasks with count=0, got: {:?}", events),
        }
    }

    #[test]
    fn test_active_subtasks_bare_count_no_mode_marker() {
        // "  1 shell" without ⏵⏵ — new CC format where subprocess count
        // appears alone without mode markers
        let mut parser = OutputParser::new();
        let events = parser.parse("  1 shell");
        match events
            .iter()
            .find(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. }))
        {
            Some(ParsedEvent::ActiveSubtasks { count, task_type }) => {
                assert_eq!(*count, 1);
                assert_eq!(task_type, "shell");
            }
            _ => panic!("Expected ActiveSubtasks with count=1, got: {:?}", events),
        }
    }

    #[test]
    fn test_active_subtasks_bare_count_plural() {
        let mut parser = OutputParser::new();
        let events = parser.parse("  2 local agents");
        match events
            .iter()
            .find(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. }))
        {
            Some(ParsedEvent::ActiveSubtasks { count, task_type }) => {
                assert_eq!(*count, 2);
                assert_eq!(task_type, "local agents");
            }
            _ => panic!("Expected ActiveSubtasks with count=2, got: {:?}", events),
        }
    }

    #[test]
    fn test_active_subtasks_bare_count_not_triggered_by_random_number() {
        // "  3 files changed" should NOT trigger subtask detection
        let mut parser = OutputParser::new();
        let events = parser.parse("  3 files changed");
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. })),
            "random numbered line should not trigger subtasks: {:?}",
            events
        );
    }

    // --- OSC 777 notify: the agent asking for the user in protocol ---

    /// Permission and approval wording requires a response. The generic idle
    /// wording does not, even when it follows a reply ending in a question mark.
    #[test]
    fn osc777_notify_reports_only_unambiguous_awaiting() {
        for (raw, expected) in [
            (
                "\x1b]777;notify;Claude Code;Claude needs your permission\x07",
                "Claude needs your permission",
            ),
            (
                "\x1b]777;notify;Codex;approval required\x07",
                "approval required",
            ),
        ] {
            match parse_osc777_notify(raw) {
                Some(ParsedEvent::Question {
                    prompt_text,
                    confident,
                }) => {
                    assert_eq!(prompt_text, expected);
                    assert!(confident, "permission wording is a certain wait");
                }
                other => panic!("expected Question for {raw:?}, got {other:?}"),
            }
        }
        assert!(
            parse_osc777_notify("\x1b]777;notify;Claude Code;Claude is waiting for your input\x07")
                .is_none()
        );
    }

    /// ST-terminated form: some terminals/agents close OSC with ESC-backslash
    /// instead of BEL. Both must parse or the signal is agent-specific again.
    #[test]
    fn osc777_notify_accepts_st_terminator() {
        let evt = parse_osc777_notify("\x1b]777;notify;Codex;approval required\x1b\\");
        assert!(matches!(
            evt,
            Some(ParsedEvent::Question { ref prompt_text, .. }) if prompt_text == "approval required"
        ));
    }

    #[test]
    fn osc777_notify_keeps_every_response_required_event_in_one_chunk() {
        let raw = concat!(
            "\x1b]777;notify;Claude Code;Claude needs your permission\x07",
            "\x1b]777;notify;Claude Code;Claude Code needs your attention\x07",
            "\x1b]777;notify;Codex;approval required\x07"
        );
        let events = parse_osc777_notifies(raw);
        assert_eq!(
            events.len(),
            2,
            "generic attention is filtered, both waits survive"
        );
        assert!(matches!(
            &events[0],
            ParsedEvent::Question { prompt_text, .. } if prompt_text == "Claude needs your permission"
        ));
        assert!(matches!(
            &events[1],
            ParsedEvent::Question { prompt_text, .. } if prompt_text == "approval required"
        ));
    }

    /// A title-only or generic notification carries no response-required
    /// semantics and must not park the session in confident awaiting.
    #[test]
    fn osc777_notify_ignores_generic_attention() {
        assert!(parse_osc777_notify("\x1b]777;notify;Claude Code\x07").is_none());
        assert!(
            parse_osc777_notify("\x1b]777;notify;Claude Code;Claude Code needs your attention\x07")
                .is_none()
        );
    }

    /// A truncated sequence at a chunk boundary must not match: matching it
    /// would let the field bodies run on into unrelated later output.
    #[test]
    fn osc777_notify_ignores_an_unterminated_sequence() {
        assert!(parse_osc777_notify("\x1b]777;notify;Claude Code;Claude nee").is_none());
    }

    /// The literal text of a notify, quoted in prose or in a code listing, is
    /// not a notify. Only the escape sequence is.
    #[test]
    fn osc777_notify_ignores_prose_and_other_osc_verbs() {
        assert!(parse_osc777_notify("we emit 777;notify;Claude Code;permission here").is_none());
        assert!(parse_osc777_notify("\x1b]777;precmd\x07").is_none());
        assert!(parse_osc777_notify("\x1b]9;4;1;50\x07").is_none());
    }

    /// An ambiguous notification after a permission request must not replace it.
    #[test]
    fn osc777_notify_keeps_permission_before_ambiguous_idle_notification() {
        let raw = "\x1b]777;notify;Claude Code;Claude needs your permission\x07 output \
                   \x1b]777;notify;Claude Code;Claude is waiting for your input\x07";
        assert!(matches!(
            parse_osc777_notify(raw),
            Some(ParsedEvent::Question { ref prompt_text, .. }) if prompt_text == "Claude needs your permission"
        ));
    }

    // --- Ink footer question detection ---

    #[test]
    fn test_ink_footer_detected_as_question() {
        let input = "Enter to select · ↑/↓ to navigate · Esc to cancel";
        let evt = parse_question(input);
        assert!(
            evt.is_some(),
            "Ink footer should trigger question detection"
        );
        match evt.unwrap() {
            ParsedEvent::Question { confident, .. } => {
                assert!(confident, "Ink footer should be confident");
            }
            other => panic!("Expected Question, got: {:?}", other),
        }
    }

    /// Regression: a grep hit that quotes the footer is not a footer. This exact
    /// line came off a live session whose tab showed the awaiting badge while the
    /// agent was mid-task — the agent was grepping its own detector's source.
    #[test]
    fn ink_footer_quoted_in_a_grep_hit_is_not_a_prompt() {
        for input in [
            r#"src-tauri/src/output_parser.rs:1150:            regex::Regex::new(r"Enter to select").unwrap();"#,
            "docs/parser.md:12: the footer reads Enter to select · Esc to cancel",
            "  see Enter to select below",
        ] {
            assert!(
                parse_question(input).is_none(),
                "quoted footer must not park the badge: {input:?}"
            );
        }
    }

    /// Regression, observed 2026-08-30 on this repo's own tab: the agent read the
    /// screen of another session, pasted it into a fenced code block to explain it,
    /// and the footer came back out inside its own output. Claude Code indents
    /// streamed text by two columns; the dialog that produced the line drew it at
    /// column 0. The tab latched `question_confident`, which nothing retracts, so
    /// it read "awaiting" for the rest of the turn while the agent worked.
    ///
    /// The exact row, copied from the live grid.
    #[test]
    fn ink_footer_indented_inside_agent_output_is_not_a_prompt() {
        let quoted = "  Enter to select · ↑/↓ to navigate · n to add notes · Tab to switch questions · Esc to cancel";
        assert!(
            parse_question(quoted).is_none(),
            "an agent quoting the footer must not park its own badge"
        );
        assert!(
            ink_dialog_footer(&[quoted.to_string()]).is_none(),
            "the re-arm reads the same row off the full screen — it must agree"
        );

        // Same text, drawn by the dialog itself: full-bleed at column 0.
        let real = "Enter to select · ↑/↓ to navigate · n to add notes · Tab to switch questions · Esc to cancel";
        assert!(
            parse_question(real).is_some(),
            "the real footer must still be detected"
        );
        assert!(ink_dialog_footer(&[real.to_string()]).is_some());
    }

    #[test]
    fn test_ink_footer_in_diff_context_ignored() {
        // "Enter to select" inside a diff hunk should NOT trigger
        let input = "+  Enter to select an item";
        let evt = parse_question(input);
        assert!(
            evt.is_none(),
            "Ink footer in diff context should be ignored"
        );
    }

    #[test]
    fn test_regular_question_not_detected_by_parser() {
        // Regular questions should NOT be detected by parse_question — they use
        // the silence-based detector in pty.rs instead.
        let input = "Do you want to proceed?";
        let evt = parse_question(input);
        assert!(
            evt.is_none(),
            "Regular questions use silence detector, not parse_question"
        );
    }

    #[test]
    fn test_cliclack_prompt_detected_as_question() {
        // Goose uses cliclack prompts: "◆  Do you allow this tool call?"
        let input = "\u{25C6}  Goose would like to call the above tool, do you allow?";
        let evt = parse_question(input);
        assert!(evt.is_some(), "cliclack prompt should be detected");
        match evt.unwrap() {
            ParsedEvent::Question {
                prompt_text,
                confident,
            } => {
                assert!(prompt_text.contains("do you allow"));
                assert!(confident);
            }
            _ => panic!("Expected Question"),
        }
    }

    #[test]
    fn test_cliclack_compact_confirmation() {
        let input = "  \u{25C6}  Are you sure you want to compact this conversation?";
        let evt = parse_question(input);
        assert!(evt.is_some(), "cliclack confirm should be detected");
    }

    #[test]
    fn test_cliclack_short_text_ignored() {
        // Very short text after ◆ should be ignored (likely not a real prompt)
        let input = "\u{25C6}  hi";
        let evt = parse_question(input);
        assert!(evt.is_none(), "Short cliclack text should be ignored");
    }

    #[test]
    fn test_grok_decorative_bullet_not_a_question() {
        // grok reuses ◆ as a decorative timeline bullet, NOT a cliclack prompt.
        // Its text repaints every frame with a changing duration, so detecting it
        // emits an endless stream of confident Questions that never dedup — the
        // notification storm. Only ◆ lines ending in '?' are real prompts.
        for input in [
            "\u{25C6} Thought for 1.3s",
            "\u{25C6} Thought for 12.7s",
            "\u{25C6} user_prompt_submit",
            "\u{25C6} Run Shell Command echo hello",
        ] {
            assert!(
                parse_question(input).is_none(),
                "grok decorative bullet must not be a question: {input:?}"
            );
        }
    }

    // --- parse_choice_prompt fixture-driven golden tests ---

    // Fixture corpus lives at src-tauri/src/fixtures/choice_prompts/. Each
    // positive fixture is a pair <name>.txt (raw screen rows) + <name>.json
    // (expected ChoicePrompt). Negatives are under negative/ and must parse
    // to None. To add a new fixture: drop the files in the folder — the test
    // auto-discovers. When an agent layout drifts, add a new .txt/.json pair
    // and loosen the parser only as needed.

    fn fixtures_dir() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../src/fixtures/choice_prompts")
    }

    fn load_rows(path: &std::path::Path) -> Vec<String> {
        let text = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("read fixture {}: {}", path.display(), e));
        text.lines().map(|l| l.to_string()).collect()
    }

    fn assert_matches_expected(
        fixture_name: &str,
        actual: &ParsedEvent,
        expected: &serde_json::Value,
    ) {
        let ParsedEvent::ChoicePrompt {
            title,
            options,
            dismiss_key,
            amend_key,
            ..
        } = actual
        else {
            panic!(
                "fixture {}: expected ChoicePrompt, got {:?}",
                fixture_name, actual
            );
        };
        assert_eq!(
            title,
            expected["title"].as_str().unwrap(),
            "fixture {} title mismatch",
            fixture_name
        );
        let exp_opts = expected["options"].as_array().unwrap();
        assert_eq!(
            options.len(),
            exp_opts.len(),
            "fixture {} option count mismatch",
            fixture_name
        );
        for (i, (got, want)) in options.iter().zip(exp_opts.iter()).enumerate() {
            assert_eq!(
                got.key,
                want["key"].as_str().unwrap(),
                "fixture {} opt[{}] key",
                fixture_name,
                i
            );
            assert_eq!(
                got.label,
                want["label"].as_str().unwrap(),
                "fixture {} opt[{}] label",
                fixture_name,
                i
            );
            assert_eq!(
                got.highlighted,
                want["highlighted"].as_bool().unwrap(),
                "fixture {} opt[{}] highlighted",
                fixture_name,
                i
            );
            assert_eq!(
                got.destructive,
                want["destructive"].as_bool().unwrap(),
                "fixture {} opt[{}] destructive",
                fixture_name,
                i
            );
            let want_hint = want.get("hint").and_then(|v| v.as_str()).map(String::from);
            assert_eq!(
                got.hint, want_hint,
                "fixture {} opt[{}] hint",
                fixture_name, i
            );
        }
        let want_dismiss = expected
            .get("dismiss_key")
            .and_then(|v| v.as_str())
            .map(String::from);
        let want_amend = expected
            .get("amend_key")
            .and_then(|v| v.as_str())
            .map(String::from);
        assert_eq!(
            dismiss_key, &want_dismiss,
            "fixture {} dismiss_key",
            fixture_name
        );
        assert_eq!(amend_key, &want_amend, "fixture {} amend_key", fixture_name);
    }

    #[test]
    fn test_choice_prompt_positive_fixtures() {
        let dir = fixtures_dir();
        let mut count = 0;
        for entry in
            std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("read_dir {}: {}", dir.display(), e))
        {
            let path = entry.unwrap().path();
            if path.extension().and_then(|s| s.to_str()) != Some("txt") {
                continue;
            }
            let name = path.file_stem().unwrap().to_string_lossy().into_owned();
            let json_path = path.with_extension("json");
            let expected: serde_json::Value = serde_json::from_str(
                &std::fs::read_to_string(&json_path)
                    .unwrap_or_else(|_| panic!("missing expected JSON for {}", name)),
            )
            .unwrap_or_else(|e| panic!("parse {}.json: {}", name, e));

            let rows = load_rows(&path);
            let actual = parse_choice_prompt(&rows)
                .unwrap_or_else(|| panic!("fixture {} did not parse — parser regression?", name));
            assert_matches_expected(&name, &actual, &expected);
            count += 1;
        }
        assert!(
            count >= 1,
            "no positive fixtures found in {}",
            dir.display()
        );
    }

    #[test]
    fn test_choice_prompt_negative_fixtures() {
        let dir = fixtures_dir().join("negative");
        let mut count = 0;
        for entry in
            std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("read_dir {}: {}", dir.display(), e))
        {
            let path = entry.unwrap().path();
            if path.extension().and_then(|s| s.to_str()) != Some("txt") {
                continue;
            }
            let rows = load_rows(&path);
            let result = parse_choice_prompt(&rows);
            assert!(
                result.is_none(),
                "negative fixture {} should NOT parse, got {:?}",
                path.file_stem().unwrap().to_string_lossy(),
                result,
            );
            count += 1;
        }
        assert!(
            count >= 1,
            "no negative fixtures found in {}",
            dir.display()
        );
    }

    #[test]
    fn test_choice_prompt_no_footer() {
        // Dialog without "Esc to cancel" footer — still valid.
        let rows: Vec<String> = ["Proceed with deletion?", "❯ 1. Yes", "  2. No"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let evt = parse_choice_prompt(&rows).expect("should parse without footer");
        let ParsedEvent::ChoicePrompt {
            dismiss_key,
            amend_key,
            ..
        } = evt
        else {
            panic!("wrong variant");
        };
        assert!(dismiss_key.is_none());
        assert!(amend_key.is_none());
    }

    #[test]
    fn test_choice_prompt_single_option_rejected() {
        let rows: Vec<String> = ["Continue?", "❯ 1. Yes"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!(
            parse_choice_prompt(&rows).is_none(),
            "single option should not match"
        );
    }

    #[test]
    fn test_choice_prompt_wire_deserializes_into_payload() {
        // Contract test: state.rs dispatches by deserializing the serialized
        // ParsedEvent::ChoicePrompt JSON directly into ChoicePromptPayload.
        // If the wire shape drifts from the payload struct (field rename, type
        // change), this test fails loudly before the runtime dispatcher does.
        let evt = ParsedEvent::ChoicePrompt {
            title: "Do you want to proceed?".into(),
            options: vec![
                ChoiceOption {
                    key: "1".into(),
                    label: "Yes".into(),
                    highlighted: true,
                    destructive: false,
                    hint: None,
                },
                ChoiceOption {
                    key: "2".into(),
                    label: "No".into(),
                    highlighted: false,
                    destructive: true,
                    hint: None,
                },
            ],
            selection_mode: None,
            dismiss_key: Some("cancel".into()),
            amend_key: Some("amend".into()),
        };
        let json = serde_json::to_value(&evt).expect("serialize");
        assert_eq!(json["type"], "choice-prompt");
        let payload: ChoicePromptPayload = serde_json::from_value(json)
            .expect("state.rs-style round-trip through ChoicePromptPayload");
        assert_eq!(payload.title, "Do you want to proceed?");
        assert_eq!(payload.options.len(), 2);
        assert_eq!(payload.dismiss_key.as_deref(), Some("cancel"));
        assert_eq!(payload.amend_key.as_deref(), Some("amend"));
    }

    fn recorded_codex_plan_question_rows() -> Vec<String> {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../src/fixtures/agent_prompts/codex-plan-question-20261019.json"
        ))
        .unwrap();
        // The retained ring is not a TCAP: original geometry, input and chunk
        // timing are unavailable. Replay on a canvas large enough for its last
        // complete repaint (row 63, column 220); assert parser content only.
        let mut grid = crate::terminal_grid::TerminalGrid::new(63, 220, 2000);
        grid.process(fixture["data"].as_str().unwrap().as_bytes());
        grid.screen_text_rows()
    }

    // Catches: the /plan request_user_input form losing its title and options
    // because only the separate queued-follow-up footer is recognized.
    #[test]
    fn recorded_codex_plan_question_exports_clean_answer_controls() {
        assert_codex_plan_answer_controls(&recorded_codex_plan_question_rows());
    }

    fn assert_codex_plan_answer_controls(rows: &[String]) {
        let ParsedEvent::ChoicePrompt {
            title,
            options,
            selection_mode,
            dismiss_key,
            amend_key,
        } = parse_choice_prompt(rows).expect("recorded /plan question must export options")
        else {
            panic!("expected choice prompt");
        };
        assert_eq!(title, "Pick a colour");
        assert_eq!(
            options
                .iter()
                .map(|o| (o.key.as_str(), o.label.as_str(), o.highlighted))
                .collect::<Vec<_>>(),
            [
                ("1", "Red", true),
                ("2", "Green", false),
                ("3", "Blue", false),
                ("4", "None of the above", false)
            ]
        );
        assert_eq!(options[0].hint.as_deref(), Some("Select Red."));
        assert_eq!(
            options[3].hint.as_deref(),
            Some("Optionally, add details in notes (tab)")
        );
        assert_eq!(
            selection_mode, None,
            "recorded numeric answer submits immediately"
        );
        assert_eq!(dismiss_key.as_deref(), Some("cancel"));
        assert_eq!(amend_key.as_deref(), Some("amend"));
    }

    // Catches: the same native /plan form failing at its real captured geometry.
    #[test]
    fn captured_codex_plan_question_exports_clean_answer_controls() {
        let capture = crate::pty_capture::decode_capture(include_bytes!(
            "../../../src/fixtures/agent_prompts/codex-plan-question-20261019-real.tcap"
        ))
        .unwrap();
        let (rows, cols) = capture.geometry.expect("real recorded geometry");
        let mut grid = crate::terminal_grid::TerminalGrid::new(rows, cols, 2000);
        for record in capture.records {
            if record.direction == crate::pty_capture::CaptureDirection::Input
                && record.data == b"\x03"
            {
                break;
            }
            if record.direction == crate::pty_capture::CaptureDirection::Output {
                grid.process(&record.data);
            }
        }
        assert_codex_plan_answer_controls(&grid.screen_text_rows());
    }

    // Catches: quoted imperative lists becoming live controls without both
    // native anchors, or a partial repaint exporting a missing answer key.
    #[test]
    fn recorded_codex_plan_question_requires_header_footer_and_numbering() {
        let rows = recorded_codex_plan_question_rows();
        for needle in ["Question 1/1", "tab to add notes", "2. Green"] {
            let mut incomplete = rows.clone();
            let row = incomplete
                .iter_mut()
                .find(|row| row.contains(needle))
                .unwrap();
            *row = String::new();
            assert!(
                parse_choice_prompt(&incomplete).is_none(),
                "missing {needle}"
            );
        }
        let mut answered = rows;
        let header = answered
            .iter_mut()
            .find(|row| row.contains("Question 1/1"))
            .unwrap();
        *header = "  Question 1/1 (0 unanswered)".to_owned();
        assert!(parse_choice_prompt(&answered).is_none());
    }

    fn captured_codex_narrow_plan_rows() -> Vec<String> {
        let capture = crate::pty_capture::decode_capture(include_bytes!(
            "../../../src/fixtures/agent_prompts/codex-first-plan-narrow-2094.tcap"
        ))
        .unwrap();
        let (rows, cols) = capture.geometry.unwrap();
        let mut grid = crate::terminal_grid::TerminalGrid::new(rows, cols, 2000);
        for record in capture.records {
            if record.direction == crate::pty_capture::CaptureDirection::Output {
                grid.process(&record.data);
            }
        }
        grid.screen_text_rows()
    }

    // Catches: native wrapped descriptions or labels preventing first-question controls.
    #[test]
    fn captured_codex_narrow_plan_exports_clean_columns() {
        let rows = captured_codex_narrow_plan_rows();
        let parsed = parse_choice_prompt(&rows);
        assert!(parsed.is_some(), "native narrow rows: {rows:#?}");
        let ParsedEvent::ChoicePrompt { options, .. } = parsed.unwrap() else {
            panic!("native question must export choices");
        };
        assert_eq!(options[3].label, "None of the above");
        assert_eq!(
            options[3].hint.as_deref(),
            Some("Optionally, add details in notes (tab)")
        );
    }

    // Catches: relaxing the narrow footer accepting a departed or incomplete dialog.
    #[test]
    fn captured_codex_narrow_plan_requires_live_footer_and_counter() {
        let rows = captured_codex_narrow_plan_rows();
        for needle in [
            "Question 1/1",
            "tab to add notes",
            "enter to submit answer",
            "esc to interrupt",
        ] {
            let mut incomplete = rows.clone();
            *incomplete
                .iter_mut()
                .find(|row| row.contains(needle))
                .unwrap() = String::new();
            assert!(
                parse_choice_prompt(&incomplete).is_none(),
                "missing {needle}"
            );
        }
        let mut departed = rows;
        departed.push("New agent output after the answered question".into());
        assert!(parse_choice_prompt(&departed).is_none());
    }

    // Catches: prose outside the native answer/description columns becoming a choice label.
    #[test]
    fn captured_codex_narrow_plan_rejects_misaligned_continuations() {
        let mut rows = captured_codex_narrow_plan_rows();
        let continuation = rows
            .iter_mut()
            .find(|row| row.trim() == "above            , add")
            .unwrap();
        *continuation = "       outside the answer column".into();
        // A nonblank prefix before the answer column is not a native wrap.
        continuation.replace_range(..1, "x");
        assert!(parse_choice_prompt(&rows).is_none());
    }

    fn recorded_claude_question_rows() -> Vec<String> {
        let bytes = std::fs::read(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../src/fixtures/agent_prompts/claude-askuser-esc-20260929.tcap"),
        )
        .unwrap();
        let capture = crate::pty_capture::decode_capture(&bytes).unwrap();
        let (rows, cols) = capture.geometry.unwrap();
        let mut grid = crate::terminal_grid::TerminalGrid::new(rows, cols, 2000);
        // Record 185 is Esc; the preceding frame is the complete recorded dialog.
        for record in capture.records.into_iter().take(185) {
            if record.direction == crate::pty_capture::CaptureDirection::Output {
                grid.process(&record.data);
            }
        }
        grid.screen_text_rows()
    }

    // Catches: lost title/option offsets, skipped AskUserQuestion parsing, or inverted highlight fallback.
    #[test]
    fn recorded_claude_question_preserves_title_options_and_selection_contract() {
        let rows = recorded_claude_question_rows();
        let ParsedEvent::ChoicePrompt {
            title,
            options,
            selection_mode,
            dismiss_key,
            amend_key,
        } = parse_claude_ask_user_question(&rows).expect("recorded dialog must parse")
        else {
            panic!("expected choice prompt")
        };
        assert_eq!(title, "Which color do you prefer?");
        assert_eq!(
            options
                .iter()
                .map(|o| (o.key.as_str(), o.label.as_str(), o.highlighted))
                .collect::<Vec<_>>(),
            vec![
                ("1", "Red", true),
                ("2", "Green", false),
                ("3", "Blue", false),
                ("4", "Type something.", false),
                ("5", "Chat about this", false)
            ]
        );
        assert_eq!(selection_mode, Some(ChoiceSelectionMode::NavigateEnter));
        assert_eq!(dismiss_key.as_deref(), Some("cancel"));
        assert_eq!(amend_key, None);
    }

    // Catches: off-by-one minimum option counts, nonsequential numbering, and overwritten selection.
    #[test]
    fn claude_question_derived_screen_boundaries_reject_incomplete_numbering() {
        let recorded = recorded_claude_question_rows();
        let title = recorded
            .iter()
            .position(|row| row.trim() == "Which color do you prefer?")
            .unwrap();
        let footer = recorded
            .iter()
            .position(|row| is_ink_dialog_footer_row(row))
            .unwrap();
        let options: Vec<String> = recorded[title + 1..footer]
            .iter()
            .filter(|row| {
                row.trim_start().starts_with("❯ 1.")
                    || (2..=5).any(|n| row.trim_start().starts_with(&format!("{n}.")))
            })
            .cloned()
            .collect();
        assert_eq!(options.len(), 5);
        for count in 0..=3 {
            let mut rows = vec![recorded[title].clone()];
            rows.extend(options[..count].iter().cloned());
            rows.push(recorded[footer].clone());
            let parsed = parse_claude_ask_user_question(&rows);
            if count < 2 {
                assert!(parsed.is_none(), "{count} choices are incomplete");
            } else {
                let Some(ParsedEvent::ChoicePrompt { options, .. }) = parsed else {
                    panic!("{count} choices must parse")
                };
                assert_eq!(options.len(), count);
            }
        }
        let rows = vec![
            recorded[title].clone(),
            options[0].clone(),
            options[2].clone(),
            recorded[footer].clone(),
        ];
        assert!(
            parse_claude_ask_user_question(&rows).is_none(),
            "a gap in numbering must be refused"
        );
        for marked in [false, true] {
            let first = options[0].replace('❯', " ");
            let second = if marked {
                format!("❯ {}", options[1].trim())
            } else {
                options[1].clone()
            };
            let rows = vec![
                recorded[title].clone(),
                first,
                second,
                recorded[footer].clone(),
            ];
            let Some(ParsedEvent::ChoicePrompt { options, .. }) =
                parse_claude_ask_user_question(&rows)
            else {
                panic!("two choices must parse")
            };
            assert_eq!(options[0].highlighted, !marked);
            assert_eq!(options[1].highlighted, marked);
        }
    }

    // Catches: combining destructive-label alternatives with AND hides cancellation choices.
    #[test]
    fn claude_question_derived_labels_keep_each_cancellation_choice_destructive() {
        let recorded = recorded_claude_question_rows();
        let footer = recorded
            .iter()
            .find(|row| is_ink_dialog_footer_row(row))
            .unwrap();
        for label in ["No", "Cancel", "Reject", "No thanks", "Continue"] {
            // Only labels change; retain the captured dialog's numbering and footer grammar.
            let rows = vec![
                "Which color do you prefer?".to_string(),
                "❯ 1. Red".to_string(),
                format!("  2. {label}"),
                footer.clone(),
            ];
            let Some(ParsedEvent::ChoicePrompt { options, .. }) =
                parse_claude_ask_user_question(&rows)
            else {
                panic!("derived labels must parse")
            };
            assert!(!options[0].destructive);
            assert_eq!(options[1].destructive, label != "Continue", "{label}");
        }
    }

    // Catches: blank-row decrement or title indexing mutations lose a shifted dialog and its selected row.
    #[test]
    fn choice_prompt_derived_recording_preserves_second_selection_after_padding() {
        let mut rows = load_rows(&fixtures_dir().join("claude-code_edit-confirm.txt"));
        rows[1] = rows[1].replace('❯', " ");
        rows[2] = format!("❯ {}", rows[2].trim());
        rows.insert(0, "earlier output".into());
        rows.insert(2, String::new());
        rows.extend([String::new(), String::new()]);
        let Some(ParsedEvent::ChoicePrompt { title, options, .. }) = parse_choice_prompt(&rows)
        else {
            panic!("padded dialog must parse")
        };
        assert_eq!(title, "Do you want to make this edit to CLAUDE.md?");
        assert_eq!(options.len(), 3);
        assert!(!options[0].highlighted);
        assert!(options[1].highlighted);
        assert!(!options[2].highlighted);
        assert_eq!(options[1].hint.as_deref(), Some("shift+tab"));
    }

    // Catches: scrolling option 1 offscreen losing navigation, or quoted clipped forms becoming controls.
    #[test]
    fn captured_claude_clipped_forms_require_native_footer_and_keep_navigation() {
        let bytes = std::fs::read(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../src/fixtures/agent_prompts/claude-narrow-2093.tcap"),
        )
        .unwrap();
        let capture = crate::pty_capture::decode_capture(&bytes).unwrap();
        let (rows, cols) = capture.geometry.unwrap();
        let mut grid = crate::terminal_grid::TerminalGrid::new(rows, cols, 2000);
        for record in capture.records {
            if record.direction == crate::pty_capture::CaptureDirection::Output {
                grid.process(&record.data);
            }
        }
        let recorded = grid.screen_text_rows();
        for (scrolled, arrowless) in [(false, false), (true, false), (false, true)] {
            let mut rows = recorded.clone();
            if arrowless {
                for row in &mut rows {
                    *row = row.replacen('↓', " ", 1);
                }
            }
            if scrolled {
                let first = rows.iter().position(|row| row.contains("1. Red")).unwrap();
                rows[first].clear();
                rows[first + 1].clear();
            }
            let Some(ParsedEvent::ChoicePrompt {
                title,
                options,
                selection_mode,
                ..
            }) = parse_claude_ask_user_question(&rows)
            else {
                panic!("clipped form needs controls");
            };
            assert_eq!(title, "Pick a colour?");
            assert_eq!(selection_mode, Some(ChoiceSelectionMode::NavigateOnly));
            assert_eq!(
                options.iter().map(|o| o.key.as_str()).collect::<Vec<_>>(),
                ["↑", "↓", "↵"]
            );
            let quoted: Vec<_> = rows.iter().map(|row| format!("  {row}")).collect();
            assert!(parse_claude_ask_user_question(&quoted).is_none());
        }
    }

    #[test]
    fn quoted_ink_dialog_footer_does_not_create_mobile_choices() {
        let rows = [
            "  Which color do you prefer?",
            "  ❯ 1. Red",
            "  2. Green",
            "  Enter to select · ↑/↓ to navigate · Esc to cancel",
        ]
        .map(str::to_string);
        assert!(parse_claude_ask_user_question(&rows).is_none());
    }

    #[test]
    fn test_choice_prompt_variant_glyphs_and_numbering() {
        // Chaos table: agents drift glyph/numbering/padding. The parser must
        // tolerate the common combinations that appear in the wild without
        // requiring a new fixture for each permutation.
        let cases: &[(&str, &str)] = &[
            // (highlight_glyph, number_suffix)
            ("❯", "."),
            ("›", "."),
            (">", "."),
            ("❯", ")"),
            ("›", ")"),
            (">", ")"),
        ];
        for (glyph, suffix) in cases {
            let rows: Vec<String> = [
                "Do you want to proceed?".to_string(),
                format!("{} 1{} Yes", glyph, suffix),
                format!("  2{} No", suffix),
            ]
            .to_vec();
            let evt = parse_choice_prompt(&rows).unwrap_or_else(|| {
                panic!("variant glyph={:?} suffix={:?} should parse", glyph, suffix);
            });
            let ParsedEvent::ChoicePrompt { options, .. } = evt else {
                panic!("wrong variant");
            };
            assert_eq!(
                options.len(),
                2,
                "variant glyph={:?} suffix={:?}: expected 2 options",
                glyph,
                suffix
            );
            assert_eq!(options[0].key, "1");
            assert!(
                options[0].highlighted,
                "variant glyph={:?} suffix={:?}: option 1 must be highlighted",
                glyph, suffix
            );
            assert_eq!(options[1].key, "2");
            assert!(!options[1].highlighted);
        }
    }

    #[test]
    fn test_choice_prompt_extra_blank_padding() {
        // Trailing blank rows (common after repaint) must not defeat bottom-up scan.
        let rows: Vec<String> = ["Do you want to proceed?", "❯ 1. Yes", "  2. No", "", "", ""]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let evt = parse_choice_prompt(&rows).expect("trailing blanks must be skipped");
        let ParsedEvent::ChoicePrompt { options, .. } = evt else {
            panic!("wrong variant");
        };
        assert_eq!(options.len(), 2);
    }

    #[test]
    fn test_choice_prompt_title_must_be_question_or_verb() {
        // Plain numbered list with a non-question, non-verb preceding line.
        let rows: Vec<String> = ["Results:", "  1. apple", "  2. banana", "  3. cherry"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!(
            parse_choice_prompt(&rows).is_none(),
            "numbered list without question/verb title must not match"
        );
    }

    // --- status-line pre-filters ---------------------------------------------

    /// Decode a real PTY capture into the screen text an agent painted.
    fn agent_capture_lines(name: &str) -> Vec<String> {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../src/fixtures/agent_prompts")
            .join(name);
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("fixture {name}: {e}"));
        let mut vt = crate::vt_log::VtLogBuffer::new(41, 128, 2000);
        let mut lines = Vec::new();
        for record in crate::pty_capture::decode(&bytes).expect("valid capture") {
            if record.direction != crate::pty_capture::CaptureDirection::Output {
                continue;
            }
            for row in vt.process(&record.data) {
                lines.push(row.text);
            }
        }
        lines
    }

    /// The byte fast path is named like a filter but is not one on agent
    /// output: 0xE2 leads every box-drawing glyph an Ink TUI paints, so it
    /// admits essentially every row and the eight-regex table ran anyway.
    ///
    /// Measured, not reasoned about — this is the premise the per-line dispatch
    /// was built on, so it is asserted against real captures.
    #[test]
    fn status_line_byte_fast_path_barely_rejects_agent_output() {
        for fixture in [
            "grok-1.0.5-minimal-turn.tcap",
            "claude-quoted-ink-footer.tcap",
        ] {
            let lines = agent_capture_lines(fixture);
            let nonblank: Vec<&String> = lines.iter().filter(|l| !l.trim().is_empty()).collect();
            assert!(
                nonblank.len() > 20,
                "{fixture}: too little content to measure ({} rows)",
                nonblank.len()
            );
            let admitted = nonblank
                .iter()
                .filter(|l| status_line_fast_path_admits(l))
                .count();
            let candidates = nonblank
                .iter()
                .filter(|l| status_line_fast_path_admits(l))
                .filter(|l| {
                    l.chars()
                        .find(|c| !c.is_whitespace())
                        .is_some_and(|lead| !status_line_patterns_for(lead).is_empty())
                })
                .count();
            let admit_pct = admitted * 100 / nonblank.len();
            eprintln!(
                "{fixture}: {}/{} rows admitted by the byte fast path ({admit_pct}%), \
                 {candidates} of those have any candidate pattern",
                admitted,
                nonblank.len()
            );
            assert!(
                admit_pct >= 50,
                "{fixture}: the byte fast path is claimed to be non-selective on \
                 agent output, but it rejected {}% of rows",
                100 - admit_pct
            );
            assert!(
                candidates < admitted,
                "{fixture}: the per-line dispatch must reject rows the byte scan \
                 admitted, otherwise it saves nothing"
            );
        }
    }

    /// The dispatch must be an exact partition of the flat table: for every
    /// leading glyph, a pattern that CAN match must be offered. Brute-forced
    /// over the whole BMP against the anchored classes.
    #[test]
    fn status_line_dispatch_offers_every_pattern_that_could_match() {
        let all: [(&regex::Regex, &str); 6] = [
            (&CLAUDE_STATUS_RE, "claude"),
            (&RUNNING_STATUS_RE, "running"),
            (&GOOSE_SPINNER_RE, "goose"),
            (&COPILOT_STATUS_RE, "copilot"),
            (&CODEX_BULLET_RE, "codex"),
            (&SPINNER_STATUS_RE, "braille"),
        ];
        // A body that satisfies every pattern's tail requirements at once, so a
        // pattern that fails can only have failed on the leading glyph.
        let body = "  Tokens: 5.2k sent, 1.3k received... (Ctrl+C to interrupt).";
        for code in 0u32..=0xFFFF {
            let Some(lead) = char::from_u32(code) else {
                continue;
            };
            if lead.is_whitespace() || lead.is_control() {
                continue;
            }
            let line = format!("{lead}{body}");
            let offered = status_line_patterns_for(lead);
            for (re, name) in all {
                if re.is_match(&line) {
                    assert!(
                        offered.iter().any(|o| std::ptr::eq(*o, re)),
                        "U+{code:04X} {lead:?}: {name} matches but is not dispatched"
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod provider_recovery_tests {
    use super::*;

    /// Catches: malformed recovery frames overwriting the last trustworthy snapshot.
    #[test]
    fn provider_recovery_rejects_invalid_contract_fields() {
        for payload in [
            r#"{"phase":"waiting","next_attempt_ms":null,"attempt":1}"#,
            r#"{"phase":"retrying","next_attempt_ms":1,"attempt":1}"#,
            r#"{"phase":"retrying","attempt":1}"#,
            r#"{"phase":"retrying","next_attempt_ms":null,"attempt":0}"#,
            r#"{"phase":"unknown","next_attempt_ms":null,"attempt":1}"#,
            r#"{"phase":"waiting","next_attempt_ms":8640000000000001,"attempt":1}"#,
            r#"{"phase":"retrying","next_attempt_ms":null,"attempt":1,"extra":true}"#,
        ] {
            assert!(parse_provider_recovery(payload).is_none(), "{payload}");
        }
        assert!(parse_provider_recovery(&"x".repeat(257)).is_none());
    }

    /// Catches: valid retrying or clear transitions being rejected at numeric boundaries.
    #[test]
    fn provider_recovery_accepts_snapshots_and_explicit_clear() {
        for payload in [
            r#"{"phase":"waiting","next_attempt_ms":0,"attempt":1}"#,
            r#"{"phase":"waiting","next_attempt_ms":8640000000000000,"attempt":4294967295}"#,
            r#"{"phase":"retrying","next_attempt_ms":null,"attempt":2}"#,
            "clear",
        ] {
            assert!(parse_provider_recovery(payload).is_some(), "{payload}");
        }
    }
}
