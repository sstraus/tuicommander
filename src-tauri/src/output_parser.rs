use serde::Serialize;

/// Structured events parsed from PTY output
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type")]
pub enum ParsedEvent {
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
        state: u8,  // 0=remove, 1=normal, 2=error, 3=indeterminate
        value: u8,  // 0-100
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
    PlanFile {
        path: String,
    },
    /// User submitted a line of input via the PTY (reconstructed from keystrokes)
    #[serde(rename = "user-input")]
    UserInput {
        content: String,
    },
    /// API error from an agent (5xx server error, auth failure, etc.)
    #[serde(rename = "api-error")]
    ApiError {
        pattern_name: String,
        matched_text: String,
        error_kind: String, // "server", "auth", "unknown"
    },
    /// Agent-declared intent: what the LLM is currently working on.
    /// Emitted via `intent: <text>` or `intent: <text> (<tab title>)` at column 0.
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
    Suggest {
        items: Vec<String>,
    },
    /// Slash command autocomplete menu detected on bottom screen rows.
    /// Fired when the user types / in an agent TUI and a menu appears.
    #[serde(rename = "slash-menu")]
    SlashMenu {
        items: Vec<SlashMenuItem>,
    },
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
    },
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
    /// Dedup: last emitted api-error matched text to suppress re-emission
    /// when the same error remains visible in the terminal buffer.
    last_api_error_match: Option<String>,
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
}

impl OutputParser {
    pub fn new() -> Self {
        Self {
            rate_limit_patterns: &RATE_LIMIT_PATTERNS,
            api_error_patterns: &API_ERROR_PATTERNS,
            last_suggest_items: None,
            last_api_error_match: None,
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
        // and extract clean rows, avoiding the strip_ansi_escapes crate.
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

        // Intent declaration: `intent: text` at column 0
        // Test-only parse assumes agent context (agent_active=true)
        if let Some(evt) = parse_intent(&clean, true) {
            events.push(evt);
        }

        // Suggest follow-up actions: `suggest: A | B | C` at column 0
        if let Some(evt) = parse_suggest(&clean, true) {
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
    pub fn parse_clean_lines(&mut self, rows: &[crate::state::ChangedRow], agent_active: bool) -> Vec<ParsedEvent> {
        let mut events = Vec::new();
        // Join rows into a single string so multi-line parsers (rate_limit, etc.) work.
        // Individual row texts are already clean — no ANSI stripping required.
        // Strip backtick characters: Claude Code renders file paths and tokens as
        // markdown inline code (`path`), which leaves literal backticks in the clean
        // text. Removing them lets all parsers match paths/tokens without needing
        // backtick-aware regexes — plugins benefit too via structured events.
        let joined: String = rows.iter()
            .map(|r| if r.text.contains('`') { r.text.replace('`', "") } else { r.text.clone() })
            .collect::<Vec<_>>()
            .join("\n");

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

        // Intent and suggest.
        // Plain-prefix tokens (`intent:`, `suggest:` at column 0) are only parsed
        // when an agent is detected — this prevents false positives from regular
        // CLI tools that might output text starting with these keywords.
        if let Some(evt) = parse_intent(&joined, agent_active) {
            events.push(evt);
        }
        if let Some(evt) = parse_suggest(&joined, agent_active) {
            if let ParsedEvent::Suggest { ref items } = evt {
                if self.last_suggest_items.as_ref() != Some(items) {
                    self.last_suggest_items = Some(items.clone());
                    events.push(evt);
                }
            } else {
                events.push(evt);
            }
        }

        // Reset dedup state on user-input (new agent cycle may produce new errors/suggestions)
        if events.iter().any(|e| matches!(e, ParsedEvent::UserInput { .. })) {
            self.last_suggest_items = None;
            self.last_api_error_match = None;
        }

        events
    }

    fn parse_rate_limit(&self, text: &str) -> Option<ParsedEvent> {
        // Fast path: every rate-limit pattern requires at least one of these keywords.
        if !text.contains("rate_limit") && !text.contains("overloaded")
            && !text.contains("RateLimit") && !text.contains("429")
            && !text.contains("RESOURCE_EXHAUSTED") && !text.contains("etry") // Retry/retry
            && !text.contains("Rate Limit") && !text.contains("per minute")
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
                let match_line = text[..m.start()]
                    .rfind('\n')
                    .map(|nl| &text[nl + 1..])
                    .unwrap_or(text);
                let match_line = match_line.lines().next().unwrap_or(match_line);
                if line_is_code_or_diff(match_line) {
                    continue;
                }

                let retry_after_ms = if pattern.has_retry_capture {
                    caps.get(1).and_then(|g| {
                        g.as_str().parse::<u64>().ok().map(|s| s * 1000)
                    })
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
        if !text.contains("api_error") && !text.contains("authentication_error")
            && !text.contains("server_error") && !text.contains("UNAVAILABLE")
            && !text.contains("INTERNAL") && !text.contains("UNAUTHENTICATED")
            && !text.contains("litellm") && !text.contains("copilot")
            && !text.contains("provider_name") && !text.contains("base_resp")
            && !text.contains("stream error") && !text.contains("servers are down")
            && !text.contains("not able to authenticate") && !text.contains("request failed")
        {
            return None;
        }
        for pattern in self.api_error_patterns {
            if let Some(m) = pattern.regex.find(text) {
                // Guard: reject matches inside source code or documentation.
                let match_line = text[..m.start()]
                    .rfind('\n')
                    .map(|nl| &text[nl + 1..])
                    .unwrap_or(text);
                let match_line = match_line.lines().next().unwrap_or(match_line);
                if line_is_code_or_diff(match_line) {
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

/// Returns true if a line looks like source code, documentation, or agent commentary
/// Combined guard: returns true if a line looks like source code, diff output,
/// or documentation context — not a real agent output line.
fn line_is_code_or_diff(line: &str) -> bool {
    line_is_source_code(line) || line_is_diff_or_code_context(line)
}

/// rather than a real API error. This prevents false-positive rate-limit detection when
/// agents read/discuss source files that contain error-code strings.
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
        rl("claude-http-429", r"(?i)rate_limit_error", Some(60000), false),
        rl("claude-overloaded", r"(?i)overloaded_error", Some(30000), false),
        // OpenAI / Cursor: specific error class names (PascalCase/structured)
        rl("openai-http-429", r"RateLimitError", Some(60000), false),
        // Cursor: exact API error message emitted by Cursor's backend (not conversational)
        rl("cursor-rate-limit", r"User Provided API Key Rate Limit Exceeded", Some(60000), false),
        // Gemini: gRPC error code (UPPER_SNAKE_CASE)
        rl("gemini-resource-exhausted", r"RESOURCE_EXHAUSTED", Some(60000), false),
        // HTTP status line — requires "429" adjacent to HTTP-like context
        // HTTP/ must be followed by a version (e.g. 1.1, 2) then whitespace then 429;
        // HTTP<space>429 (no version) also accepted. Prevents ANSI garbage bridging.
        rl("http-429", r"(?i)\b429\b.{0,20}Too Many Requests|HTTP/\d[\d.]*\s+429|HTTP\s+429", Some(60000), false),
        // Retry-After HTTP header (colon-separated, very specific format)
        rl("retry-after-header", r"(?i)Retry-After:\s*(\d+)", None, true),
        // OpenAI structured retry message (requires "Retry after N seconds" exact phrasing)
        rl("openai-retry-after", r"Retry after (\d+) seconds?", None, true),
        // Token/request limit errors — require structured error context (quotes, colons, or error prefix)
        rl("openai-tpm-limit", r"(?i)tokens per minute.*limit|TPM limit", Some(60000), false),
        rl("openai-rpm-limit", r"(?i)requests per minute.*limit|RPM limit", Some(60000), false),
    ]
}

fn rl(name: &'static str, pattern: &str, retry_after_ms: Option<u64>, has_retry_capture: bool) -> RateLimitPattern {
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
        ae("claude-api-error", r#""type":"api_error""#, "server"),
        // Claude Code: authentication_error in JSON body (401) — also used by OpenAI
        ae("claude-auth-error", r#""type":"authentication_error""#, "auth"),
        // Gemini CLI: API Error: got status: UNAVAILABLE/INTERNAL
        ae("gemini-server-error", r"API Error: got status: (?:UNAVAILABLE|INTERNAL)", "server"),
        // Aider: litellm server/auth exceptions
        ae("aider-server-error", r"litellm\.(?:InternalServerError|ServiceUnavailableError|APIError):", "server"),
        ae("aider-auth-error", r"litellm\.AuthenticationError:", "auth"),
        // Aider: user-facing translated messages
        ae("aider-server-msg", r"The API provider's servers are down or overloaded", "server"),
        ae("aider-auth-msg", r"The API provider is not able to authenticate you", "auth"),
        // Codex CLI: stream error with retry exhaustion (non-429 status)
        ae("codex-stream-error", r"stream error: exceeded retry limit, last status: [45]\d\d", "server"),
        // Copilot CLI: token/auth failures
        // Note: "request failed unexpectedly" removed — too generic, triggers on Claude output
        ae("copilot-auth-error", r"(?:Failed to get copilot token|copilot token.*expired)", "auth"),

        // === Provider-level JSON error patterns ===
        // These fire when any CLI prints the raw API error response.

        // OpenAI: {"error":{"type":"server_error",...}}
        ae("openai-server-error", r#""type"\s*:\s*"server_error""#, "server"),
        // Google Gemini/Vertex: {"error":{"status":"INTERNAL"}} or "UNAVAILABLE"
        ae("google-api-server", r#""status"\s*:\s*"(?:INTERNAL|UNAVAILABLE)""#, "server"),
        // Google auth: {"error":{"status":"UNAUTHENTICATED"}} or API_KEY_INVALID
        ae("google-api-auth", r#""status"\s*:\s*"UNAUTHENTICATED""#, "auth"),
        // OpenRouter: error JSON with provider_name metadata and error code
        ae("openrouter-server", r#""error"\s*:\s*\{[^}]*"provider_name"\s*:"#, "server"),
        // MiniMax: {"base_resp":{"status_code":1013,...}} — non-zero status_code indicates error
        ae("minimax-server", r#""base_resp"\s*:\s*\{[^}]*"status_code"\s*:\s*[1-9]"#, "server"),
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
pub(crate) fn parse_osc94(text: &str) -> Option<ParsedEvent> {
    // Fast path: check for ESC ] before running regex
    if !text.contains("\x1b]9;4;") {
        return None;
    }
    lazy_static::lazy_static! {
        static ref OSC94_RE: regex::Regex =
            regex::Regex::new(r"\x1b\]9;4;(\d);(\d{1,3})(?:\x07|\x1b\\)").unwrap();
    }
    OSC94_RE.captures(text).map(|caps| {
        let state: u8 = caps[1].parse().unwrap_or(0);
        let value: u8 = caps[2].parse().unwrap_or(0).min(100);
        ParsedEvent::Progress { state, value }
    })
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

/// Strip ANSI escape sequences from raw PTY text using the vt100 crate.
///
/// Renders the text through a virtual 220×50 screen and extracts the visible
/// rows, correctly handling cursor movement, carriage returns, and all CSI/OSC
/// sequences. Only used by the test-only [`OutputParser::parse`] method.
#[cfg(test)]
fn strip_ansi_via_vt100(text: &str) -> String {
    let mut parser = vt100::Parser::new(50, 220, 0);
    parser.process(text.as_bytes());
    let screen = parser.screen();
    let cols = screen.size().1;
    screen
        .rows(0, cols)
        .map(|r| r.trim_end().to_string())
        .filter(|r| !r.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Parse status line patterns from pre-stripped terminal output.
fn parse_status_line(clean: &str) -> Option<ParsedEvent> {
    // Fast path: skip chunks that cannot contain any status line marker.
    // Checks for: '*' (Claude), "[Running]", "Tokens:" (Aider), or multi-byte UTF-8:
    //   0xC2 = lead byte for · (U+00B7, Claude middle dot)
    //   0xE2 = lead byte for braille U+2800, dingbat asterisks U+2720-273F,
    //          block elements ░█, bullets •◦, ∴ (U+2234), ● (U+25CF), ○ (U+25CB)
    if !clean.contains('*') && !clean.contains("[Running]") && !clean.contains("Tokens:")
        && !clean.as_bytes().contains(&0xe2)
        && !clean.as_bytes().contains(&0xc2)
    {
        return None;
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
        // Aider Knight Rider scanner: "░█" or "█░" prefix followed by spaces + task text
        // The scanner chars (░ U+2591, █ U+2588) bounce back and forth across the line.
        static ref AIDER_SPINNER_RE: regex::Regex =
            regex::Regex::new(r"^\s*[\u{2588}\u{2591}#=]{1,2}\s{2,}(.+)").unwrap();
        // Aider token report: "Tokens: 5.2k sent, 1.3k received."
        static ref AIDER_TOKENS_RE: regex::Regex =
            regex::Regex::new(r"^\s*Tokens:\s+(.+?)\.$").unwrap();
        // Codex CLI bullet spinner: "• Working (5s • esc to interrupt)" or "◦ Working (12s)"
        // • (U+2022) and ◦ (U+25E6) alternate as a blink spinner.
        // Requires parenthesized time suffix to avoid false positives from markdown bullets.
        static ref CODEX_BULLET_RE: regex::Regex =
            regex::Regex::new(r"^\s*[\u{2022}\u{25E6}]\s+(\w[^(]*?)\s*\(\d+[smh]").unwrap();
        // Copilot CLI indicators: ∴ (U+2234 thinking), ● (U+25CF active), ○ (U+25CB queued)
        // Format: "∴ Thinking…" or "● Read file..." or "○ Verify..."
        static ref COPILOT_STATUS_RE: regex::Regex =
            regex::Regex::new(r"^\s*[\u{2234}\u{25CF}\u{25CB}]\s+([^.…\n]+)(?:\.{2,3}|…)").unwrap();
        // Time info
        static ref TIME_RE: regex::Regex =
            regex::Regex::new(r"\((\d+[smh])").unwrap();
        // Token info
        static ref TOKEN_RE: regex::Regex =
            regex::Regex::new(r"(?i)(\d+(?:[.,]\d+)?k?\s*tokens)").unwrap();
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

        // Try each pattern (order matters: more specific first)
        let patterns: &[&regex::Regex] = &[
            &CLAUDE_STATUS_RE,
            &RUNNING_STATUS_RE,
            &AIDER_TOKENS_RE,
            &AIDER_SPINNER_RE,
            &COPILOT_STATUS_RE,
            &CODEX_BULLET_RE,
            &SPINNER_STATUS_RE,
        ];
        for pattern in patterns {
            if let Some(caps) = pattern.captures(line) {
                let task_name = caps[1].trim().to_string();
                if task_name.len() < 3 {
                    continue;
                }
                // Reject task names containing code/data artifacts — real agent
                // status lines are natural language (e.g. "Reading files").
                if task_name.contains('[') || task_name.contains(']')
                    || task_name.contains('"') || task_name.contains('\'')
                    || task_name.contains('|') || task_name.contains('{')
                    || task_name.contains('}') || task_name.contains('\\')
                    || task_name.contains('/')
                {
                    continue;
                }
                let time_info = TIME_RE.captures(line).map(|c| c[1].to_string());
                // For Aider token reports the whole line IS the token info
                let token_info = if std::ptr::eq(*pattern, &*AIDER_TOKENS_RE) {
                    Some(task_name.clone())
                } else {
                    TOKEN_RE.captures(line).map(|c| c[1].to_string())
                };
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
                return Some(ParsedEvent::ActiveSubtasks { count: 0, task_type: String::new() });
            }
            // Mode line present but no count → sub-tasks finished
            return Some(ParsedEvent::ActiveSubtasks { count: 0, task_type: String::new() });
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
            let reset_time = RESETS_RE.captures(line)
                .map(|caps| caps[1].trim().to_string());
            return Some(ParsedEvent::UsageExhausted { reset_time });
        }
    }
    None
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
        static ref INK_FOOTER_RE: regex::Regex =
            regex::Regex::new(r"Enter to select").unwrap();
    }
    for line in clean.lines() {
        let trimmed = line.trim();
        if INK_FOOTER_RE.is_match(trimmed) && !line_is_diff_or_code_context(line) {
            return Some(ParsedEvent::Question {
                prompt_text: trimmed.to_string(),
                confident: true,
            });
        }
    }
    None
}


/// Returns true if a line looks like diff output, code context, or documentation
/// Returns true if a line looks like diff output, code context, or documentation
/// rather than a genuine interactive prompt. Applied to ALL question regex matches
/// to prevent false positives from diff hunks containing question-like patterns.
fn line_is_diff_or_code_context(line: &str) -> bool {
    let trimmed = line.trim();

    // Line-number prefix from code listings: "462 -...", "75 +-...", "465 //...", "1226    assert!(..."
    // Distinguished from HTTP status codes ("429 Too Many Requests") by requiring either:
    //   - diff markers (+, -, //) after the number, OR
    //   - 2+ spaces after the number (code listing indentation)
    if trimmed.len() > 3 && trimmed.as_bytes()[0].is_ascii_digit()
        && let Some(pos) = trimmed.find(|c: char| !c.is_ascii_digit()) {
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
            && pos > 0 && trimmed.as_bytes()[pos - 1].is_ascii_digit() {
                return true;
            }
    }

    // Lines containing "//" as code comments (but not URLs like http://)
    if trimmed.starts_with("//")
        || trimmed.contains(" //")
    {
        return true;
    }

    // Lines with markdown bold/italic containing question-pattern keywords
    // (e.g., "**Hardcoded prompts**: ...")
    if trimmed.contains("**") && (
        trimmed.contains("prompts")
        || trimmed.contains("patterns")
        || trimmed.contains("detection")
    ) {
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
    tracing::debug!("[plan-file] fast-path hit, scanning lines (len={})", clean.len());
    for line in clean.lines() {
        if line.contains("plans/") {
            tracing::debug!("[plan-file] candidate line: {:?}", line);
        }
        if let Some(caps) = PLAN_RE.captures(line) {
            let mut path = caps[1].to_string();
            // Expand leading ~/ to the user's home directory so the
            // frontend always receives an absolute path it can open.
            if path.starts_with("~/")
                && let Some(home) = dirs::home_dir()
            {
                path = format!("{}{}", home.display(), &path[1..]);
            }
            return Some(ParsedEvent::PlanFile { path });
        }
    }
    None
}

/// Regex matching any CSI escape sequence (SGR colors, cursor movement, erase,
/// Detect agent-declared intent tokens: `intent: <text>` or `intent: <text> (<title>)`
/// at column 0. Only parsed when an agent is active — prevents false positives from
/// prose like "The intent: of this code".
fn parse_intent(clean: &str, agent_active: bool) -> Option<ParsedEvent> {
    if !agent_active || !clean.contains("intent:") {
        return None;
    }
    lazy_static::lazy_static! {
        // Plain prefix: `intent:` at line start, with optional leading
        // horizontal whitespace and/or an Ink bullet glyph (● U+25CF /
        // ⏺ U+23FA). Ink-hosted agents (Claude Code) decorate the first
        // line of an assistant message with `● ` and indent every
        // continuation line by the bullet width, so plain-prefix tokens
        // emitted after the first line arrive as `  intent: ...`. The
        // leading whitespace must be horizontal only — any non-whitespace
        // character before the keyword is rejected.
        static ref INTENT_PLAIN_RE: regex::Regex =
            regex::Regex::new(r"(?m)^[\t ]*(?:[\x{25CF}\x{23FA}][\t ]+)?intent:[\t ]+(.+)$").unwrap();
        // Separate regex to split out the optional (title) suffix from the captured text
        static ref TITLE_RE: regex::Regex =
            regex::Regex::new(r"^(.*?)\(([^)]+)\)\s*$").unwrap();
    }

    let raw_match = INTENT_PLAIN_RE
        .captures(clean)
        .map(|caps| caps[1].trim().to_string());

    build_intent_event(raw_match, &TITLE_RE)
}

/// Shared logic for building an Intent event from a raw match string.
fn build_intent_event(
    raw_match: Option<String>,
    title_re: &regex::Regex,
) -> Option<ParsedEvent> {
    raw_match.and_then(|raw| {
        let raw = raw.trim();
        // Filter out meaningless intents: ellipsis, bare punctuation, template placeholders
        if raw == "..." || raw == "<text>" || raw.len() < 4 {
            return None;
        }
        // Try to extract optional (title) from the end
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

/// Detect suggested follow-up actions: `suggest: A | B | C` at column 0.
/// Requires at least 2 pipe-separated items (TUIC protocol: 2–4 items).
/// The `|` requirement prevents false positives on prose containing "suggest:"
/// that happens to wrap to column 0 in Ink's \r-segment rendering.
/// Only parsed when an agent is active.
///
/// Handles terminal line-wrap: when the suggest text is wider than the terminal,
/// vt100 splits it across multiple `ChangedRow`s. The regex matches the first
/// line, then continuation lines are joined until a line without `|` or starting
/// with a known token prefix is reached.
fn parse_suggest(clean: &str, agent_active: bool) -> Option<ParsedEvent> {
    if !agent_active || !clean.contains("suggest:") {
        return None;
    }
    lazy_static::lazy_static! {
        // Match the suggest: prefix line. Captures everything after `suggest: `.
        static ref SUGGEST_START_RE: regex::Regex =
            regex::Regex::new(r"(?m)^[\t ]*(?:[\x{25CF}\x{23FA}][\t ]+)?suggest:[\t ]+(.+)$").unwrap();
    }

    let caps = SUGGEST_START_RE.captures(clean)?;
    let first_line = caps[1].trim();
    let match_end = caps.get(0).unwrap().end();

    // Collect continuation lines: text after the matched line that is part of
    // the same suggest token (wrapped by the terminal). A continuation line
    // must not start with a recognized token prefix and should contain `|`
    // (unless it's the tail of the last item).
    // Skip past the newline that `$` matched before.
    let remainder = clean[match_end..].strip_prefix('\n').unwrap_or(&clean[match_end..]);
    let mut full = first_line.to_string();
    for line in remainder.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        // Stop at lines that look like a new token (intent:, suggest:, etc.)
        if trimmed.starts_with("intent:")
            || trimmed.starts_with("suggest:")
            || trimmed.starts_with("●")
            || trimmed.starts_with("⏺")
            || trimmed.starts_with("❯")
            || trimmed.starts_with(">")
            || trimmed.starts_with("›")
        {
            break;
        }
        // Only join if the line contains a `|` — that means it's a
        // continuation of the pipe-separated list. Lines without `|` are
        // unrelated output that follows the suggest token.
        if !trimmed.contains('|') {
            break;
        }
        full.push(' ');
        full.push_str(trimmed);
    }

    // Must have at least one `|` separator
    if !full.contains('|') {
        return None;
    }

    let items: Vec<String> = full
        .split('|')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if items.len() < 2 {
        return None;
    }
    Some(ParsedEvent::Suggest { items })
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
/// Requires at least 2 consecutive matching rows from the bottom of the screen.
/// Highlighted item: detected by `❯` prefix or falls back to first item.
pub fn parse_slash_menu(screen_rows: &[String]) -> Option<ParsedEvent> {
    lazy_static::lazy_static! {
        // Each menu row: optional leading whitespace, optional ❯ marker, /command, 2+ spaces, description
        static ref MENU_ROW_RE: regex::Regex =
            regex::Regex::new(r"^\s*(?:❯\s+)?(/\S+)\s{2,}(.+)$").unwrap();
    }

    // Scan from the bottom to find the contiguous block of menu rows.
    // Menu items are at the bottom of the screen; stop at the first non-matching row.
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
            items.push(SlashMenuItem { command, description, highlighted });
        } else {
            break;
        }
    }

    // Reverse to restore top-to-bottom order (we scanned bottom-to-top).
    items.reverse();

    if items.len() < 2 {
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_github_pr_url() {
        let mut parser = OutputParser::new();
        let events = parser.parse("Created PR: https://github.com/owner/repo/pull/42");
        assert_eq!(events.len(), 1);
        match &events[0] {
            ParsedEvent::PrUrl { number, platform, url } => {
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
            ParsedEvent::PrUrl { number, platform, .. } => {
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
    fn test_status_line_claude() {
        let mut parser = OutputParser::new();
        let events = parser.parse("* Reading files... (12s)");
        assert_eq!(events.len(), 1);
        match &events[0] {
            ParsedEvent::StatusLine { task_name, time_info, .. } => {
                assert_eq!(task_name, "Reading files");
                assert_eq!(time_info.as_deref(), Some("12s"));
            }
            _ => panic!("Expected StatusLine event"),
        }
    }

    #[test]
    fn test_status_line_with_tokens() {
        let mut parser = OutputParser::new();
        let events = parser.parse("* Updating synthesis phase... (57s \u{b7} \u{2193} 2.4k tokens)");
        assert_eq!(events.len(), 1);
        match &events[0] {
            ParsedEvent::StatusLine { task_name, token_info, .. } => {
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
            ParsedEvent::StatusLine { task_name, time_info, token_info, .. } => {
                assert_eq!(task_name, "Updating verification document");
                assert_eq!(time_info.as_deref(), Some("5m"));
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
    fn test_status_line_aider_spinner() {
        // Aider uses a Knight Rider scanner: "░█        Waiting for claude-3-5-sonnet"
        let mut parser = OutputParser::new();
        let events = parser.parse("\u{2591}\u{2588}        Waiting for claude-3-5-sonnet");
        assert_eq!(events.len(), 1);
        match &events[0] {
            ParsedEvent::StatusLine { task_name, .. } => {
                assert_eq!(task_name, "Waiting for claude-3-5-sonnet");
            }
            _ => panic!("Expected StatusLine, got {:?}", events[0]),
        }
    }

    #[test]
    fn test_status_line_aider_token_report() {
        // Aider token report: "Tokens: 5.2k sent, 1.3k received."
        let mut parser = OutputParser::new();
        let events = parser.parse("Tokens: 5.2k sent, 1.3k received.");
        assert_eq!(events.len(), 1);
        match &events[0] {
            ParsedEvent::StatusLine { task_name, token_info, .. } => {
                assert_eq!(task_name, "5.2k sent, 1.3k received");
                assert!(token_info.is_some(), "Expected token_info");
            }
            _ => panic!("Expected StatusLine, got {:?}", events[0]),
        }
    }

    #[test]
    fn test_status_line_aider_token_report_with_cache() {
        // Aider with cache: "Tokens: 2,345 sent, 123 cache write, 456 cache hit, 789 received."
        let mut parser = OutputParser::new();
        let events = parser.parse("Tokens: 2,345 sent, 123 cache write, 456 cache hit, 789 received.");
        assert_eq!(events.len(), 1);
        match &events[0] {
            ParsedEvent::StatusLine { task_name, .. } => {
                assert!(task_name.contains("sent"));
                assert!(task_name.contains("received"));
            }
            _ => panic!("Expected StatusLine, got {:?}", events[0]),
        }
    }

    #[test]
    fn test_status_line_codex_working() {
        // Codex CLI: "• Working (5s • esc to interrupt)"
        let mut parser = OutputParser::new();
        let events = parser.parse("\u{2022} Working (5s \u{2022} esc to interrupt)");
        assert_eq!(events.len(), 1);
        match &events[0] {
            ParsedEvent::StatusLine { task_name, time_info, .. } => {
                assert_eq!(task_name, "Working");
                assert_eq!(time_info.as_deref(), Some("5s"));
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
            ParsedEvent::StatusLine { task_name, time_info, .. } => {
                assert_eq!(task_name, "Working");
                assert_eq!(time_info.as_deref(), Some("12s"));
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
            !events.iter().any(|e| matches!(e, ParsedEvent::StatusLine { .. })),
            "Glob pattern in code should not match status line: {:?}", events
        );
    }

    #[test]
    fn test_status_line_rejects_stats_suffix() {
        // Claude Code stats like ") | [C2 S31 K30 A30 M7 H..." must NOT match.
        let mut parser = OutputParser::new();
        let events = parser.parse("* ) | [C2 S31 K30 A30 M7 H...");
        assert!(
            !events.iter().any(|e| matches!(e, ParsedEvent::StatusLine { .. })),
            "Stats suffix should be rejected by content validation: {:?}", events
        );
    }

    #[test]
    fn test_status_line_rejects_mid_line_asterisk() {
        // Asterisk mid-line in code should not trigger status-line detection
        let mut parser = OutputParser::new();
        let events = parser.parse(r#"  let result = a * b... done"#);
        assert!(
            !events.iter().any(|e| matches!(e, ParsedEvent::StatusLine { .. })),
            "Mid-line asterisk should not match: {:?}", events
        );
    }

    #[test]
    fn test_status_line_rejects_path_with_asterisk() {
        // Paths with wildcard should not trigger
        let mut parser = OutputParser::new();
        let events = parser.parse("src/**/*.ts...");
        assert!(
            !events.iter().any(|e| matches!(e, ParsedEvent::StatusLine { .. })),
            "Path with asterisk should not match: {:?}", events
        );
    }

    // All question detection is silence-based only (10s idle after `?`-ending line).
    // No instant regex detection — it causes false positives from streaming output.
    #[test]
    fn test_no_instant_question_would_you_like() {
        let mut parser = OutputParser::new();
        let events = parser.parse("Would you like to proceed?");
        assert!(!events.iter().any(|e| matches!(e, ParsedEvent::Question { .. })));
    }

    #[test]
    fn test_no_instant_question_do_you_want() {
        let mut parser = OutputParser::new();
        let events = parser.parse("Do you want to continue with this approach?");
        assert!(!events.iter().any(|e| matches!(e, ParsedEvent::Question { .. })));
    }

    #[test]
    fn test_no_instant_question_menu_choice() {
        let mut parser = OutputParser::new();
        let events = parser.parse("❯ 1. Yes, clear context and bypass permissions");
        assert!(!events.iter().any(|e| matches!(e, ParsedEvent::Question { .. })));
    }

    #[test]
    fn test_no_instant_question_yn_prompt() {
        let mut parser = OutputParser::new();
        let events = parser.parse("Apply changes? [Y/n]");
        assert!(!events.iter().any(|e| matches!(e, ParsedEvent::Question { .. })));
    }

    #[test]
    fn test_no_instant_question_inquirer_style() {
        let mut parser = OutputParser::new();
        let events = parser.parse("? Which template would you like to use?");
        assert!(!events.iter().any(|e| matches!(e, ParsedEvent::Question { .. })));
    }

    #[test]
    fn test_no_question_normal_output() {
        let mut parser = OutputParser::new();
        let events = parser.parse("Building project... done");
        assert!(!events.iter().any(|e| matches!(e, ParsedEvent::Question { .. })));
    }

    // These phrases end with `?` and are handled by the silence-based detector
    // (10s idle threshold in pty.rs), NOT by instant regex detection.
    // Instant detection of generic question-like phrases causes massive false
    // positives during AI agent streaming output.
    #[test]
    fn test_no_instant_question_want_me_to() {
        let mut parser = OutputParser::new();
        let events = parser.parse("Want me to commit these changes?");
        assert!(!events.iter().any(|e| matches!(e, ParsedEvent::Question { .. })),
            "\"Want me to\" must NOT instant-detect (silence-based only)");
    }

    #[test]
    fn test_no_instant_question_should_i() {
        let mut parser = OutputParser::new();
        let events = parser.parse("Should I proceed with the refactor?");
        assert!(!events.iter().any(|e| matches!(e, ParsedEvent::Question { .. })),
            "\"Should I\" must NOT instant-detect (silence-based only)");
    }

    #[test]
    fn test_no_instant_question_what_would_you_like() {
        let mut parser = OutputParser::new();
        let events = parser.parse("What would you like me to do next?");
        assert!(!events.iter().any(|e| matches!(e, ParsedEvent::Question { .. })),
            "\"What would you like\" must NOT instant-detect (silence-based only)");
    }

    #[test]
    fn test_no_instant_question_something_else() {
        let mut parser = OutputParser::new();
        let events = parser.parse("Something else?");
        assert!(!events.iter().any(|e| matches!(e, ParsedEvent::Question { .. })),
            "\"Something else?\" must NOT instant-detect (silence-based only)");
    }

    #[test]
    fn test_no_instant_question_what_do_you_think() {
        let mut parser = OutputParser::new();
        let events = parser.parse("What do you think?");
        assert!(!events.iter().any(|e| matches!(e, ParsedEvent::Question { .. })),
            "\"What do you think\" must NOT instant-detect (silence-based only)");
    }

    #[test]
    fn test_no_instant_question_whats_your_preference() {
        let mut parser = OutputParser::new();
        let events = parser.parse("What's your preference?");
        assert!(!events.iter().any(|e| matches!(e, ParsedEvent::Question { .. })),
            "\"What's your\" must NOT instant-detect (silence-based only)");
    }

    #[test]
    fn test_no_instant_question_shall_i() {
        let mut parser = OutputParser::new();
        let events = parser.parse("Shall I run the tests first?");
        assert!(!events.iter().any(|e| matches!(e, ParsedEvent::Question { .. })),
            "\"Shall I\" must NOT instant-detect (silence-based only)");
    }

    #[test]
    fn test_no_question_should_in_prose() {
        let mut parser = OutputParser::new();
        // "should" in middle of prose — not a question prompt
        let events = parser.parse("The function should handle edge cases properly.");
        assert!(!events.iter().any(|e| matches!(e, ParsedEvent::Question { .. })),
            "\"should\" in prose should NOT trigger question detection");
    }

    #[test]
    fn test_no_question_want_in_prose() {
        let mut parser = OutputParser::new();
        let events = parser.parse("If you want to learn more about this pattern, see the docs.");
        assert!(!events.iter().any(|e| matches!(e, ParsedEvent::Question { .. })),
            "\"want\" in prose should NOT trigger question detection");
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
        let events = parser.parse("You've used 78% of your weekly limit · resets Feb 21 at 9am (Europe/Madrid)");
        assert!(events.iter().any(|e| matches!(e, ParsedEvent::UsageLimit { percentage: 78, .. })));
        match events.iter().find(|e| matches!(e, ParsedEvent::UsageLimit { .. })) {
            Some(ParsedEvent::UsageLimit { percentage, limit_type }) => {
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
        match events.iter().find(|e| matches!(e, ParsedEvent::UsageLimit { .. })) {
            Some(ParsedEvent::UsageLimit { percentage, limit_type }) => {
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
        assert!(events.iter().any(|e| matches!(e, ParsedEvent::UsageLimit { percentage: 90, .. })));
    }

    #[test]
    fn test_usage_limit_smart_quote() {
        let mut parser = OutputParser::new();
        let events = parser.parse("You\u{2019}ve used 50% of your weekly limit");
        assert!(events.iter().any(|e| matches!(e, ParsedEvent::UsageLimit { percentage: 50, .. })));
    }

    // --- Usage exhaustion tests ---

    #[test]
    fn test_extra_usage_exhausted_with_time_and_tz() {
        let mut parser = OutputParser::new();
        let events = parser.parse("You're out of extra usage · resets 8pm (Europe/Madrid)");
        match events.iter().find(|e| matches!(e, ParsedEvent::UsageExhausted { .. })) {
            Some(ParsedEvent::UsageExhausted { reset_time }) => {
                assert_eq!(reset_time.as_deref(), Some("8pm (Europe/Madrid)"));
            }
            _ => panic!("Expected UsageExhausted event, got: {:?}", events),
        }
    }

    #[test]
    fn test_extra_usage_exhausted_with_date_time_tz() {
        let mut parser = OutputParser::new();
        let events = parser.parse("You're out of extra usage · resets Feb 21 at 9am (Europe/Madrid)");
        match events.iter().find(|e| matches!(e, ParsedEvent::UsageExhausted { .. })) {
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
        match events.iter().find(|e| matches!(e, ParsedEvent::UsageExhausted { .. })) {
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
        let events = parser.parse("You've used 78% of your weekly limit · resets Feb 21 at 9am (Europe/Madrid)");
        assert!(events.iter().any(|e| matches!(e, ParsedEvent::UsageLimit { percentage: 78, .. })),
            "should still produce UsageLimit");
        assert!(!events.iter().any(|e| matches!(e, ParsedEvent::UsageExhausted { .. })),
            "should NOT produce UsageExhausted for percentage-based limits");
    }

    #[test]
    fn test_extra_usage_exhausted_unparseable_reset() {
        let mut parser = OutputParser::new();
        let events = parser.parse("You're out of extra usage · resets soon");
        match events.iter().find(|e| matches!(e, ParsedEvent::UsageExhausted { .. })) {
            Some(ParsedEvent::UsageExhausted { reset_time }) => {
                assert_eq!(reset_time.as_deref(), Some("soon"));
            }
            _ => panic!("Expected UsageExhausted event, got: {:?}", events),
        }
    }

    #[test]
    fn test_extra_usage_exhausted_with_ansi() {
        let mut parser = OutputParser::new();
        let events = parser.parse("\x1b[33mYou're out of extra usage · resets 8pm (Europe/Madrid)\x1b[0m");
        assert!(events.iter().any(|e| matches!(e, ParsedEvent::UsageExhausted { .. })),
            "should parse through ANSI escapes");
    }

    #[test]
    fn test_out_of_usage_without_extra() {
        // "out of usage" (without "extra") should also be caught
        let mut parser = OutputParser::new();
        let events = parser.parse("You're out of usage · resets 3pm (US/Eastern)");
        match events.iter().find(|e| matches!(e, ParsedEvent::UsageExhausted { .. })) {
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
        assert!(!events.iter().any(|e| matches!(e, ParsedEvent::Question { .. })));
    }

    #[test]
    fn test_no_instant_question_ascii_cursor() {
        let mut parser = OutputParser::new();
        let events = parser.parse("> 1. Yes, proceed with changes");
        assert!(!events.iter().any(|e| matches!(e, ParsedEvent::Question { .. })));
    }

    #[test]
    fn test_instant_question_ink_footer() {
        // Ink interactive menu footer IS detected instantly — it's ultra-specific
        // and only appears when the agent is genuinely waiting for menu selection.
        let mut parser = OutputParser::new();
        let events = parser.parse("Enter to select · ↑/↓ to navigate · Esc to cancel");
        assert!(events.iter().any(|e| matches!(e, ParsedEvent::Question { confident: true, .. })),
            "Ink footer should be detected as confident question");
    }

    #[test]
    fn test_instant_question_ink_footer_partial() {
        let mut parser = OutputParser::new();
        let events = parser.parse("Enter to select · ↑↓ to navigate");
        assert!(events.iter().any(|e| matches!(e, ParsedEvent::Question { confident: true, .. })),
            "Partial Ink footer should also be detected");
    }

    #[test]
    fn test_question_generic_question_mark_not_instant() {
        // Generic `?`-ending lines are NOT detected by the instant parser —
        // they are handled by the silence-based detector in pty.rs to avoid
        // false positives from streaming fragments like "ad?", "swap?", "?"
        let mut parser = OutputParser::new();
        let events = parser.parse("What should we do with this story?");
        assert!(!events.iter().any(|e| matches!(e, ParsedEvent::Question { .. })),
            "Generic ?-ending lines should NOT trigger instant detection");
    }

    #[test]
    fn test_question_generic_not_prose() {
        // Lines that look like prose/code should NOT trigger the generic ? match
        let mut parser = OutputParser::new();
        // Code comment
        assert!(!parser.parse("// should we handle this case?")
            .iter().any(|e| matches!(e, ParsedEvent::Question { .. })),
            "code comment should not trigger question detection");
        // Markdown list item
        assert!(!parser.parse("- What about this edge case?")
            .iter().any(|e| matches!(e, ParsedEvent::Question { .. })),
            "markdown list should not trigger question detection");
        // Indented code
        assert!(!parser.parse("    if condition.valid?")
            .iter().any(|e| matches!(e, ParsedEvent::Question { .. })),
            "indented code should not trigger question detection");
        // Backtick-wrapped code
        assert!(!parser.parse("Have you tried `foo.bar()?` instead?")
            .iter().any(|e| matches!(e, ParsedEvent::Question { .. })),
            "backtick code should not trigger question detection");
        // Long prose line
        let long = format!("{}?", "a".repeat(121));
        assert!(!parser.parse(&long)
            .iter().any(|e| matches!(e, ParsedEvent::Question { .. })),
            "long prose should not trigger question detection");
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
        assert!(events.iter().any(|e| matches!(e, ParsedEvent::Question { confident: true, .. })),
            "Full Ink menu block should be detected as confident question");
    }

    #[test]
    fn test_no_question_blockquote_with_question() {
        let mut parser = OutputParser::new();
        assert!(!parser.parse("> Do you agree with this approach?")
            .iter().any(|e| matches!(e, ParsedEvent::Question { .. })));
    }

    #[test]
    fn test_no_question_bold_markdown() {
        let mut parser = OutputParser::new();
        assert!(!parser.parse("**Should we refactor this?**")
            .iter().any(|e| matches!(e, ParsedEvent::Question { .. })));
    }

    #[test]
    fn test_no_question_shell_prompt_with_greater_than() {
        // Shell prompts like "> command" should NOT trigger menu detection
        let mut parser = OutputParser::new();
        assert!(!parser.parse("> git status")
            .iter().any(|e| matches!(e, ParsedEvent::Question { .. })));
    }

    #[test]
    fn test_no_question_enter_in_prose() {
        // Prose mentioning "Enter to select" in a different context should still match,
        // but "Press Enter to continue" should NOT
        let mut parser = OutputParser::new();
        assert!(!parser.parse("Press Enter to continue installing")
            .iter().any(|e| matches!(e, ParsedEvent::Question { .. })));
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
        assert_eq!(get_plan_path(&events), Some("plans/my-feature.md".to_string()));
    }

    #[test]
    fn test_plan_file_dot_claude() {
        let mut parser = OutputParser::new();
        let events = parser.parse("Writing plan: .claude/plans/auth-flow.md");
        assert_eq!(get_plan_path(&events), Some(".claude/plans/auth-flow.md".to_string()));
    }

    #[test]
    fn test_plan_file_absolute() {
        let mut parser = OutputParser::new();
        let events = parser.parse("Created /Users/dev/project/plans/refactor.md");
        assert_eq!(get_plan_path(&events), Some("/Users/dev/project/plans/refactor.md".to_string()));
    }

    #[test]
    fn test_plan_file_claude_private() {
        let mut parser = OutputParser::new();
        let events = parser.parse("Plan: .claude-private/plans/serene-waterfall.md");
        assert_eq!(get_plan_path(&events), Some(".claude-private/plans/serene-waterfall.md".to_string()));
    }

    #[test]
    fn test_plan_file_tilde_expanded() {
        let mut parser = OutputParser::new();
        let events = parser.parse("Plan saved to ~/.claude/plans/graceful-rolling-quasar.md");
        let path = get_plan_path(&events).expect("should detect tilde plan path");
        // Tilde must be expanded to an absolute path
        assert!(!path.starts_with("~"), "tilde should be expanded: {path}");
        assert!(path.ends_with("/.claude/plans/graceful-rolling-quasar.md"));
        assert!(path.starts_with("/"), "path should be absolute: {path}");
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
        assert_eq!(get_plan_path(&events), Some("plans/wiz-memory-integration.md".to_string()));

        // Same with tilde path
        let events = parser.parse("Fatto, piano in ~/Gits/project/plans/my-plan.md.");
        let path = get_plan_path(&events).expect("should detect plan path with trailing period");
        assert!(path.ends_with("/plans/my-plan.md"), "trailing period should be stripped: {path}");
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
        assert_eq!(get_plan_path(&events), Some("plans/document-organizer.md".to_string()));
    }

    #[test]
    fn test_plan_file_backtick_wrapped_absolute() {
        let mut parser = OutputParser::new();
        let events = parser.parse("Plan saved to `/Users/dev/project/plans/my-plan.md`.");
        assert_eq!(get_plan_path(&events), Some("/Users/dev/project/plans/my-plan.md".to_string()));
    }

    // --- False positive prevention tests ---

    fn has_rate_limit(events: &[ParsedEvent]) -> bool {
        events.iter().any(|e| matches!(e, ParsedEvent::RateLimit { .. }))
    }

    #[test]
    fn test_no_false_positive_conversational_rate_limit() {
        let mut parser = OutputParser::new();
        // Agent discussing rate limits in prose should NOT trigger detection
        assert!(!has_rate_limit(&parser.parse("The rate limit detection was triggering false positives")));
        assert!(!has_rate_limit(&parser.parse("I fixed the rate-limited pattern matching")));
        assert!(!has_rate_limit(&parser.parse("We should handle too many requests gracefully")));
        assert!(!has_rate_limit(&parser.parse("The rate limiting logic needs improvement")));
    }

    #[test]
    fn test_no_false_positive_code_output() {
        let mut parser = OutputParser::new();
        // Code snippets mentioning rate limits should NOT trigger
        assert!(!has_rate_limit(&parser.parse("rl(\"rate-limit-keyword\", r\"rate[- ]?limit\", Some(60000))")));
        assert!(!has_rate_limit(&parser.parse("// Handle too many requests from the API")));
        assert!(!has_rate_limit(&parser.parse("fn handle_rate_limit(retry_after: u64) {")));
    }

    #[test]
    fn test_no_false_positive_tpm_rpm_acronyms() {
        let mut parser = OutputParser::new();
        // TPM/RPM in non-rate-limit context should NOT trigger
        assert!(!has_rate_limit(&parser.parse("TPM 2.0 module detected")));
        assert!(!has_rate_limit(&parser.parse("RPM package manager installed")));
        assert!(!has_rate_limit(&parser.parse("The disk spins at 7200 RPM")));
    }

    #[test]
    fn test_http_429_real_errors_still_detected() {
        let mut parser = OutputParser::new();
        // Real HTTP 429 errors should still be detected
        assert!(has_rate_limit(&parser.parse("HTTP/1.1 429 Too Many Requests")));
        assert!(has_rate_limit(&parser.parse("429 Too Many Requests")));
        assert!(has_rate_limit(&parser.parse("HTTP 429")));
    }

    #[test]
    fn test_real_api_errors_still_detected() {
        let mut parser = OutputParser::new();
        // Real API error codes should still be detected
        assert!(has_rate_limit(&parser.parse("Error: rate_limit_error")));
        assert!(has_rate_limit(&parser.parse("overloaded_error: service busy")));
        assert!(has_rate_limit(&parser.parse("RateLimitError: exceeded quota")));
        assert!(has_rate_limit(&parser.parse("RESOURCE_EXHAUSTED")));
        assert!(has_rate_limit(&parser.parse("Retry-After: 60")));
    }

    // --- Source code false-positive guard tests ---

    #[test]
    fn test_no_false_positive_rust_source_reading() {
        let mut parser = OutputParser::new();
        // Agent reading output_parser.rs — the exact lines that caused the bug
        assert!(!has_rate_limit(&parser.parse(r#"        rl("claude-http-429", r"(?i)rate_limit_error", Some(60000), false),"#)));
        assert!(!has_rate_limit(&parser.parse(r#"        rl("claude-overloaded", r"(?i)overloaded_error", Some(30000), false),"#)));
        assert!(!has_rate_limit(&parser.parse(r#"        rl("openai-http-429", r"RateLimitError", Some(60000), false),"#)));
        assert!(!has_rate_limit(&parser.parse(r#"        rl("gemini-resource-exhausted", r"RESOURCE_EXHAUSTED", Some(60000), false),"#)));
        assert!(!has_rate_limit(&parser.parse(r#"        rl("retry-after-header", r"(?i)Retry-After:\s*(\d+)", None, true),"#)));
    }

    #[test]
    fn test_no_false_positive_code_comments() {
        let mut parser = OutputParser::new();
        assert!(!has_rate_limit(&parser.parse("// Error: rate_limit_error")));
        assert!(!has_rate_limit(&parser.parse("# Handle RESOURCE_EXHAUSTED from Gemini")));
    }

    #[test]
    fn test_no_false_positive_test_assertions() {
        let mut parser = OutputParser::new();
        assert!(!has_rate_limit(&parser.parse(r#"        assert!(has_rate_limit(&parser.parse("Error: rate_limit_error")));"#)));
        assert!(!has_rate_limit(&parser.parse(r#"        assert!(has_rate_limit(&parser.parse("RateLimitError: exceeded quota")));"#)));
    }

    #[test]
    fn test_no_false_positive_markdown_code_fences() {
        let mut parser = OutputParser::new();
        assert!(!has_rate_limit(&parser.parse("```rust\nrl(\"claude-http-429\", r\"rate_limit_error\")")));
        assert!(!has_rate_limit(&parser.parse("- `rate_limit_error` — Claude API error code")));
        assert!(!has_rate_limit(&parser.parse("* Pattern `RateLimitError` matches OpenAI errors")));
    }

    #[test]
    fn test_no_false_positive_markdown_table() {
        let mut parser = OutputParser::new();
        assert!(!has_rate_limit(&parser.parse("| `claude-http-429` | `rate_limit_error` | Claude API |")));
    }

    #[test]
    fn test_line_is_source_code_fn() {
        // Direct unit tests for the guard function
        assert!(line_is_source_code(r#"        rl("claude-http-429", r"(?i)rate_limit_error", Some(60000), false),"#));
        assert!(line_is_source_code("// rate_limit_error handling"));
        assert!(line_is_source_code("# RESOURCE_EXHAUSTED"));
        assert!(line_is_source_code("fn handle_rate_limit() {"));
        assert!(line_is_source_code(r#"        assert!(has_rate_limit(&parser.parse("rate_limit_error")));"#));
        assert!(line_is_source_code("```Error: rate_limit_error"));
        assert!(line_is_source_code("- `rate_limit_error` is the error code"));
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
        events.iter().any(|e| matches!(e, ParsedEvent::ApiError { .. }))
    }

    fn get_api_error(events: &[ParsedEvent]) -> Option<(&str, &str, &str)> {
        events.iter().find_map(|e| match e {
            ParsedEvent::ApiError { pattern_name, matched_text, error_kind } =>
                Some((pattern_name.as_str(), matched_text.as_str(), error_kind.as_str())),
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
    fn test_api_error_aider_server() {
        let mut parser = OutputParser::new();
        let input = r#"litellm.InternalServerError: AnthropicException - {"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#;
        let events = parser.parse(input);
        let (name, _, kind) = get_api_error(&events).expect("should detect Aider server error");
        assert_eq!(name, "aider-server-error");
        assert_eq!(kind, "server");
    }

    #[test]
    fn test_api_error_aider_auth() {
        let mut parser = OutputParser::new();
        let input = "litellm.AuthenticationError: AnthropicException - invalid x-api-key";
        let events = parser.parse(input);
        let (name, _, kind) = get_api_error(&events).expect("should detect Aider auth error");
        assert_eq!(name, "aider-auth-error");
        assert_eq!(kind, "auth");
    }

    #[test]
    fn test_api_error_aider_translated_server() {
        let mut parser = OutputParser::new();
        let input = "The API provider's servers are down or overloaded.";
        let events = parser.parse(input);
        let (name, _, kind) = get_api_error(&events).expect("should detect Aider translated msg");
        assert_eq!(name, "aider-server-msg");
        assert_eq!(kind, "server");
    }

    #[test]
    fn test_api_error_aider_translated_auth() {
        let mut parser = OutputParser::new();
        let input = "The API provider is not able to authenticate you. Check your API key.";
        let events = parser.parse(input);
        let (name, _, kind) = get_api_error(&events).expect("should detect Aider auth msg");
        assert_eq!(name, "aider-auth-msg");
        assert_eq!(kind, "auth");
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
        assert!(!has_api_error(&parser.parse("Hello world, everything is fine")));
    }

    #[test]
    fn test_no_api_error_false_positive_source_code() {
        let mut parser = OutputParser::new();
        // Agent reading this very source file should not trigger
        assert!(!has_api_error(&parser.parse("        ae(\"claude-api-error\", \"type\":\"api_error\", \"server\"),")));
        assert!(!has_api_error(&parser.parse("// detect \"type\":\"api_error\" in JSON")));
        assert!(!has_api_error(&parser.parse("# Handle authentication_error from Claude")));
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
        // Manually clear dedup (simulates user-input reset in production)
        parser.last_api_error_match = None;
        // Same error text should fire again (new agent cycle)
        assert!(has_api_error(&parser.parse(error_input)));
    }

    #[test]
    fn test_api_error_dedup_different_error_fires() {
        let mut parser = OutputParser::new();
        let error1 = r#"API Error: 500 {"type":"error","error":{"type":"api_error","message":"Internal server error"}}"#;
        let error2 = r#"{"error":{"message":"The server had an error","type":"server_error","param":null}}"#;
        assert!(has_api_error(&parser.parse(error1)));
        // Different error text should fire even without user-input reset
        assert!(has_api_error(&parser.parse(error2)));
    }

    // --- Diff output false-positive prevention tests ---

    fn has_question(events: &[ParsedEvent]) -> bool {
        events.iter().any(|e| matches!(e, ParsedEvent::Question { .. }))
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
        assert!(!has_question(&parser.parse(
            "465 //GenericY/Nprompts:[Y/n],[y/N],(yes/no)"
        )));
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
        assert!(!has_question(&parser.parse(
            "77 +- **Y/N prompts**: `[Y/n]`, `[y/N]`, `(yes/no)`"
        )));
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
        assert!(!has_question(&parser.parse(
            "+        if QUESTION_RE.is_match(trimmed) {"
        )));
        assert!(!has_question(&parser.parse(
            "-        // Numbered menu choices: ❯ 1. or ) 1. followed by option text"
        )));
        assert!(!has_question(&parser.parse(
            "+- **Y/N prompts**: `[Y/n]`, `[y/N]`, `(yes/no)`"
        )));
    }

    // --- Intent detection tests ---

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
        let mut parser = OutputParser::new();
        assert!(get_intent(&parser.parse("Building project... done")).is_none());
        assert!(get_intent(&parser.parse("The intent is to refactor")).is_none());
    }

    // --- Plain-prefix intent tests ---

    #[test]
    fn test_intent_plain_prefix_with_ink_bullet() {
        // Claude Code (and other Ink-hosted agents) prepend every assistant
        // output line with `● ` (U+25CF). The plain-prefix parser must still
        // recognise the token even when the bullet is present.
        let mut parser = OutputParser::new();
        let events = parser.parse("\u{25CF} intent: wiring the parser (Parser Fix)");
        assert_eq!(get_intent(&events), Some("wiring the parser".to_string()));
        assert_eq!(get_intent_title(&events), Some("Parser Fix".to_string()));
    }

    #[test]
    fn test_intent_plain_prefix_with_record_bullet() {
        let mut parser = OutputParser::new();
        let events = parser.parse("\u{23FA} intent: fixing the layout bug");
        assert_eq!(get_intent(&events), Some("fixing the layout bug".to_string()));
    }

    #[test]
    fn test_suggest_plain_prefix_with_ink_bullet() {
        let mut parser = OutputParser::new();
        let events = parser.parse("\u{25CF} suggest: Rebuild | Retry | Abort");
        let items = events.iter().find_map(|e| match e {
            ParsedEvent::Suggest { items } => Some(items.clone()),
            _ => None,
        });
        assert_eq!(
            items,
            Some(vec!["Rebuild".to_string(), "Retry".to_string(), "Abort".to_string()])
        );
    }

    #[test]
    fn test_intent_plain_prefix_basic() {
        let mut parser = OutputParser::new();
        let events = parser.parse("intent: reading the config file");
        assert_eq!(get_intent(&events), Some("reading the config file".to_string()));
    }

    #[test]
    fn test_intent_plain_prefix_with_title() {
        let mut parser = OutputParser::new();
        let events = parser.parse("intent: analyzing code (Analysis)");
        assert_eq!(get_intent(&events), Some("analyzing code".to_string()));
        assert_eq!(get_intent_title(&events), Some("Analysis".to_string()));
    }

    #[test]
    fn test_intent_plain_prefix_indented_matches() {
        // Indented continuation lines produced by Ink-hosted agents must
        // still be detected: Claude Code indents every line after the
        // first by the bullet width, so plain-prefix intents emitted mid-
        // message arrive as `  intent: ...` rather than at column 0.
        let mut parser = OutputParser::new();
        assert_eq!(
            get_intent(&parser.parse("  intent: indented continuation")),
            Some("indented continuation".to_string()),
        );
    }

    #[test]
    fn test_intent_plain_prefix_midline_no_match() {
        let mut parser = OutputParser::new();
        // Mid-line intent: should NOT match
        assert!(get_intent(&parser.parse("The intent: of this code is clear")).is_none());
    }

    #[test]
    fn test_intent_plain_prefix_in_multiline() {
        // Use \r\n to simulate real PTY output (LF without CR leaves cursor at same column)
        let mut parser = OutputParser::new();
        let events = parser.parse("some output\r\nintent: debugging login flow\r\nmore output");
        assert_eq!(get_intent(&events), Some("debugging login flow".to_string()));
    }

    #[test]
    fn test_intent_plain_prefix_too_short_filtered() {
        let mut parser = OutputParser::new();
        assert!(get_intent(&parser.parse("intent: ab")).is_none());
    }

    #[test]
    fn test_intent_plain_prefix_ellipsis_filtered() {
        let mut parser = OutputParser::new();
        assert!(get_intent(&parser.parse("intent: ...")).is_none());
    }

    #[test]
    fn test_intent_plain_prefix_no_space_after_colon_no_match() {
        let mut parser = OutputParser::new();
        // Requires space after colon per `^intent:\s+`
        assert!(get_intent(&parser.parse("intent:nospace")).is_none());
    }

    // ---- False positive regression tests ----

    fn has_status_line(events: &[ParsedEvent]) -> bool {
        events.iter().any(|e| matches!(e, ParsedEvent::StatusLine { .. }))
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
        assert!(!has_status_line(&parser.parse(
            "/* Last prompt sub-row */"
        )));
        assert!(!has_status_line(&parser.parse(
            " * Strip ANSI escape codes from text */"
        )));
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
        assert!(!has_status_line(&parser.parse("• This is a bullet point in a list")));
        assert!(!has_status_line(&parser.parse("  • Another nested bullet item")));
    }

    #[test]
    fn test_no_rate_limit_story_429() {
        let mut parser = OutputParser::new();
        // Conversational text mentioning "story 429" — not an HTTP 429
        assert!(!has_rate_limit(&parser.parse(
            "che sembrano provenire da altre sessioni (story 429"
        )));
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
        let events = parser.parse("suggest: Run tests | Check logs | Push changes");
        let items = get_suggest(&events).expect("should parse plain-prefix suggest");
        assert_eq!(items, vec!["Run tests", "Check logs", "Push changes"]);
    }

    #[test]
    fn test_suggest_plain_prefix_single_item_rejected() {
        // Single item (no `|` separator) must NOT match — prevents false
        // positives on prose like "suggest: we should refactor" that wraps
        // to column 0 in Ink's \r-segment rendering.
        let mut parser = OutputParser::new();
        let events = parser.parse("suggest: Single option");
        assert!(get_suggest(&events).is_none(),
            "single item without | must not parse as suggest");
    }

    #[test]
    fn test_suggest_plain_prefix_indented_matches() {
        // See test_intent_plain_prefix_indented_matches.
        let mut parser = OutputParser::new();
        let items = get_suggest(&parser.parse("  suggest: A | B"))
            .expect("indented suggest must match");
        assert_eq!(items, vec!["A", "B"]);
    }

    #[test]
    fn test_suggest_plain_prefix_midline_no_match() {
        let mut parser = OutputParser::new();
        assert!(get_suggest(&parser.parse("I suggest: we should refactor")).is_none());
    }

    #[test]
    fn test_suggest_prose_at_column_zero_no_match() {
        // Prose containing "suggest:" that wraps to column 0 in a VtLogBuffer row
        // must NOT be parsed as a suggest event.
        let mut parser = OutputParser::new();
        assert!(get_suggest(&parser.parse("suggest: we should investigate the streaming issue")).is_none(),
            "prose at column 0 without | must not match");
    }

    #[test]
    fn test_suggest_plain_prefix_trims_whitespace() {
        let mut parser = OutputParser::new();
        let events = parser.parse("suggest:   Fix test  |  Refactor  |  Add docs  ");
        let items = get_suggest(&events).expect("should parse");
        assert_eq!(items, vec!["Fix test", "Refactor", "Add docs"]);
    }

    #[test]
    fn test_suggest_plain_prefix_in_multiline() {
        let mut parser = OutputParser::new();
        let events = parser.parse("some output\r\nsuggest: Option A | Option B\r\nmore output");
        let items = get_suggest(&events).expect("should parse in multiline");
        assert_eq!(items, vec!["Option A", "Option B"]);
    }

    #[test]
    fn test_suggest_terminal_line_wrap() {
        // When the suggest text is wider than the terminal, vt100 splits it
        // across multiple rows. The parser must join continuation lines.
        let input = "suggest: 1) Screenshot overview panel | 2) Fix suggest scroll flicker | 3) Fix\n\
                      Cmd+Shift+M keybinding collision | 4) Manual test OSC 133";
        let items = parse_suggest(input, true);
        let items = match items {
            Some(ParsedEvent::Suggest { items }) => items,
            _ => panic!("should parse wrapped suggest"),
        };
        assert_eq!(items.len(), 4);
        assert_eq!(items[0], "1) Screenshot overview panel");
        assert_eq!(items[3], "4) Manual test OSC 133");
    }

    #[test]
    fn test_suggest_wrap_stops_at_new_token() {
        // Continuation must stop when a new token prefix appears.
        let input = "suggest: A | B | C\nintent: doing something";
        let items = parse_suggest(input, true);
        let items = match items {
            Some(ParsedEvent::Suggest { items }) => items,
            _ => panic!("should parse"),
        };
        assert_eq!(items, vec!["A", "B", "C"]);
    }

    // --- Agent-gating tests for parse_intent / parse_suggest ---
    // Tokens must only be parsed when agent_active=true.

    #[test]
    fn test_intent_not_parsed_without_agent() {
        assert!(parse_intent("intent: reading the config file", false).is_none(),
            "intent should not parse when agent_active=false");
    }

    #[test]
    fn test_intent_parsed_with_agent() {
        assert!(parse_intent("intent: reading the config file", true).is_some(),
            "intent should parse when agent_active=true");
    }

    #[test]
    fn test_suggest_not_parsed_without_agent() {
        assert!(parse_suggest("suggest: Run tests | Check logs", false).is_none(),
            "suggest should not parse when agent_active=false");
    }

    #[test]
    fn test_suggest_parsed_with_agent() {
        assert!(parse_suggest("suggest: Run tests | Check logs", true).is_some(),
            "suggest should parse when agent_active=true");
    }

    #[test]
    fn test_parse_clean_lines_gated_by_agent() {
        use crate::state::ChangedRow;
        let mut parser = OutputParser::new();
        let rows = vec![ChangedRow { row_index: 0, text: "suggest: A | B | C".into() }];

        // Without agent: should NOT be parsed
        let events = parser.parse_clean_lines(&rows, false);
        assert!(!events.iter().any(|e| matches!(e, ParsedEvent::Suggest { .. })),
            "suggest should be gated when agent_active=false");

        // With agent: should be parsed
        let events = parser.parse_clean_lines(&rows, true);
        assert!(events.iter().any(|e| matches!(e, ParsedEvent::Suggest { .. })),
            "suggest should parse when agent_active=true");
    }

    // --- parse_clean_lines tests ---

    fn row(i: usize, text: &str) -> crate::state::ChangedRow {
        crate::state::ChangedRow { row_index: i, text: text.to_string() }
    }

    #[test]
    fn test_parse_clean_lines_status_line() {
        let mut parser = OutputParser::new();
        let rows = vec![row(0, "* Reading files...")];
        let events = parser.parse_clean_lines(&rows, true);
        assert!(
            events.iter().any(|e| matches!(e, ParsedEvent::StatusLine { .. })),
            "expected StatusLine, got: {:?}", events
        );
    }

    #[test]
    fn test_parse_clean_lines_intent_with_title() {
        let mut parser = OutputParser::new();
        let rows = vec![row(0, "intent: Implementing feature (My title)")];
        let events = parser.parse_clean_lines(&rows, true);
        assert!(
            events.iter().any(|e| matches!(e, ParsedEvent::Intent { title: Some(_), .. })),
            "expected Intent with title, got: {:?}", events
        );
    }

    #[test]
    fn test_parse_clean_lines_suggest() {
        let mut parser = OutputParser::new();
        let rows = vec![row(0, "suggest: Run tests | Review diff | Deploy")];
        let events = parser.parse_clean_lines(&rows, true);
        assert!(
            events.iter().any(|e| matches!(e, ParsedEvent::Suggest { items } if items.len() == 3)),
            "expected Suggest with 3 items, got: {:?}", events
        );
    }

    #[test]
    fn test_parse_clean_lines_suggest_filters_empty_items() {
        let mut parser = OutputParser::new();
        let rows = vec![row(0, "suggest: Run tests |  | Deploy")];
        let events = parser.parse_clean_lines(&rows, true);
        let items = events.iter().find_map(|e| match e {
            ParsedEvent::Suggest { items } => Some(items.clone()),
            _ => None,
        });
        assert_eq!(items.as_deref(), Some(&["Run tests".to_string(), "Deploy".to_string()][..]),
            "empty items should be filtered; got: {:?}", items);
    }

    #[test]
    fn test_parse_clean_lines_no_instant_question() {
        let mut parser = OutputParser::new();
        let rows = vec![row(0, "Would you like to proceed?")];
        let events = parser.parse_clean_lines(&rows, true);
        assert!(
            !events.iter().any(|e| matches!(e, ParsedEvent::Question { .. })),
            "no instant question detection — silence-based only"
        );
    }

    #[test]
    fn test_parse_clean_lines_usage_limit() {
        let mut parser = OutputParser::new();
        let rows = vec![row(0, "You've used 78% of your weekly limit")];
        let events = parser.parse_clean_lines(&rows, true);
        assert!(
            events.iter().any(|e| matches!(e, ParsedEvent::UsageLimit { .. })),
            "expected UsageLimit, got: {:?}", events
        );
    }

    #[test]
    fn test_parse_clean_lines_intent_with_bullet_prefix() {
        // Claude Code prepends a bullet glyph (● U+25CF) at column 0 before the token.
        let mut parser = OutputParser::new();
        let rows = vec![row(0, "\u{25CF} intent: Implementing feature (My title)")];
        let events = parser.parse_clean_lines(&rows, true);
        assert!(
            events.iter().any(|e| matches!(e, ParsedEvent::Intent { title: Some(t), .. } if t == "My title")),
            "expected Intent with title='My title', got: {:?}", events
        );
    }

    #[test]
    fn test_parse_clean_lines_rate_limit() {
        let mut parser = OutputParser::new();
        let rows = vec![row(0, "Error: rate_limit_error - please try again")];
        let events = parser.parse_clean_lines(&rows, true);
        assert!(
            events.iter().any(|e| matches!(e, ParsedEvent::RateLimit { .. })),
            "expected RateLimit, got: {:?}", events
        );
    }

    #[test]
    fn test_parse_clean_lines_plan_file() {
        let mut parser = OutputParser::new();
        let rows = vec![row(0, "Reading plans/vt100-clean-parsing.md")];
        let events = parser.parse_clean_lines(&rows, true);
        assert!(
            events.iter().any(|e| matches!(e, ParsedEvent::PlanFile { .. })),
            "expected PlanFile, got: {:?}", events
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
        assert_eq!(path, Some("plans/document-organizer.md".to_string()),
            "backtick-wrapped plan path should be detected via clean_lines strip; got: {:?}", events);
    }

    #[test]
    fn test_parse_clean_lines_pr_url() {
        let mut parser = OutputParser::new();
        let rows = vec![row(0, "Pull request: https://github.com/owner/repo/pull/42")];
        let events = parser.parse_clean_lines(&rows, true);
        assert!(
            events.iter().any(|e| matches!(e, ParsedEvent::PrUrl { .. })),
            "expected PrUrl, got: {:?}", events
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
        let has_intent = events.iter().any(|e| matches!(e, ParsedEvent::Intent { .. }));
        let has_status = events.iter().any(|e| matches!(e, ParsedEvent::StatusLine { .. }));
        assert!(has_intent, "expected Intent event, got: {:?}", events);
        assert!(has_status, "expected StatusLine event, got: {:?}", events);
    }

    // --- parse_slash_menu tests ---

    fn make_screen(rows: &[&str], total: usize) -> Vec<String> {
        let mut screen: Vec<String> = vec![String::new(); total.saturating_sub(rows.len())];
        screen.extend(rows.iter().map(|r| r.to_string()));
        screen
    }

    #[test]
    fn test_slash_menu_claude_code_basic() {
        let screen = make_screen(&[
            "   /help      Get help with using Claude Code",
            " ❯ /review    Review your code",
            "   /clear     Clear conversation history",
        ], 24);
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
        let screen = make_screen(&[
            "   /help      Get help with using Claude Code",
            "   /review    Review your code",
        ], 24);
        let evt = parse_slash_menu(&screen).expect("should detect slash menu");
        match evt {
            ParsedEvent::SlashMenu { items } => {
                assert_eq!(items.len(), 2);
                assert!(items[0].highlighted, "first item should be highlighted as fallback");
                assert!(!items[1].highlighted);
            }
            _ => panic!("Expected SlashMenu event"),
        }
    }

    #[test]
    fn test_slash_menu_single_row_not_detected() {
        let screen = make_screen(&[
            "   /help      Get help with using Claude Code",
        ], 24);
        assert!(parse_slash_menu(&screen).is_none(), "single row should not trigger menu");
    }

    #[test]
    fn test_slash_menu_no_match() {
        let screen = make_screen(&[
            "some random output",
            "another line",
        ], 24);
        assert!(parse_slash_menu(&screen).is_none());
    }

    #[test]
    fn test_slash_menu_trailing_empty_rows() {
        let screen = make_screen(&[
            "   /help      Get help",
            " ❯ /review    Review code",
            "   /clear     Clear history",
            "",
            "",
        ], 24);
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
        screen.push("$ claude-code".to_string());  // non-menu line
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
        let screen = make_screen(&[
            "  /help        Show help information",
            "  /edit        Edit a file",
            "  /run         Run a command",
        ], 24);
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
        let screen = make_screen(&[
            "   /compact   Compact the conversation to reduce context window usage",
            "   /review    Review a pull request with detailed feedback",
        ], 24);
        let evt = parse_slash_menu(&screen).expect("should parse long descriptions");
        match evt {
            ParsedEvent::SlashMenu { items } => {
                assert_eq!(items[0].command, "/compact");
                assert_eq!(items[0].description, "Compact the conversation to reduce context window usage");
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
        screen.push("  /wiz:plan                   (wiz) Transform feature descriptions into well".to_string());
        screen.push("  /wiz:stories                (wiz) Persistent file-based task tracking with".to_string());
        screen.push("  /wiz:review                 (wiz) Exhaustive multi-agent code review for P".to_string());
        screen.push("  /wiz:work                   (wiz) Execute work plans efficiently while mai".to_string());
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
        // Without chrome trimming, the parser would fail because it scans
        // bottom-up and hits the prompt/status bar before the menu rows.
        let mut screen: Vec<String> = vec![String::new(); 18];
        screen.push("  /help      Get help with using Claude Code".to_string());
        screen.push("  /review    Review your code".to_string());
        // Chrome below the menu (untrimmed)
        screen.push("────────────────────────────────────────".to_string());
        screen.push("❯ /".to_string());
        screen.push("────────────────────────────────────────".to_string());
        screen.push("  [Opus 4.6 | Max] project git:(main)".to_string());
        assert!(parse_slash_menu(&screen).is_none(), "untrimmed chrome should prevent detection");
    }

    // ── ActiveSubtasks tests ──────────────────────────────────────────

    #[test]
    fn test_active_subtasks_local_agents() {
        let mut parser = OutputParser::new();
        let events = parser.parse("\u{203A}\u{203A} bypass permissions on \u{00B7} 2 local agents");
        match events.iter().find(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. })) {
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
        match events.iter().find(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. })) {
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
        match events.iter().find(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. })) {
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
        match events.iter().find(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. })) {
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
        match events.iter().find(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. })) {
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
        match events.iter().find(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. })) {
            Some(ParsedEvent::ActiveSubtasks { count, .. }) => {
                assert_eq!(*count, 0);
            }
            _ => panic!("Expected ActiveSubtasks with count=0, got: {:?}", events),
        }
    }

    #[test]
    fn test_active_subtasks_embedded_in_multiline_output() {
        let mut parser = OutputParser::new();
        let input = "some other output\n\u{203A}\u{203A} working \u{00B7} 2 bash\nmore output after";
        let events = parser.parse(input);
        match events.iter().find(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. })) {
            Some(ParsedEvent::ActiveSubtasks { count, task_type }) => {
                assert_eq!(*count, 2);
                assert_eq!(task_type, "bash");
            }
            _ => panic!("Expected ActiveSubtasks in multiline input, got: {:?}", events),
        }
    }

    #[test]
    fn test_active_subtasks_not_triggered_by_regular_text() {
        let mut parser = OutputParser::new();
        let events = parser.parse("some regular output with \u{203A} single guillemet");
        assert!(
            !events.iter().any(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. })),
            "Should not match single guillemet: {:?}", events
        );
    }

    // ── ActiveSubtasks tests with ⏵⏵ (U+23F5) prefix ──────────────────

    #[test]
    fn test_active_subtasks_triangle_local_agents() {
        let mut parser = OutputParser::new();
        let events = parser.parse("\u{23F5}\u{23F5} bypass permissions on \u{00B7} 2 local agents");
        match events.iter().find(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. })) {
            Some(ParsedEvent::ActiveSubtasks { count, task_type }) => {
                assert_eq!(*count, 2);
                assert_eq!(task_type, "local agents");
            }
            _ => panic!("Expected ActiveSubtasks event with ⏵⏵ prefix, got: {:?}", events),
        }
    }

    #[test]
    fn test_active_subtasks_triangle_bare_resets_to_zero() {
        let mut parser = OutputParser::new();
        let events = parser.parse("\u{23F5}\u{23F5} bypass permissions on");
        match events.iter().find(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. })) {
            Some(ParsedEvent::ActiveSubtasks { count, .. }) => {
                assert_eq!(*count, 0);
            }
            _ => panic!("Expected ActiveSubtasks with count=0 for ⏵⏵ prefix, got: {:?}", events),
        }
    }

    #[test]
    fn test_active_subtasks_triangle_single_bash() {
        let mut parser = OutputParser::new();
        let events = parser.parse("\u{23F5}\u{23F5} reading config files \u{00B7} 1 bash");
        match events.iter().find(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. })) {
            Some(ParsedEvent::ActiveSubtasks { count, task_type }) => {
                assert_eq!(*count, 1);
                assert_eq!(task_type, "bash");
            }
            _ => panic!("Expected ActiveSubtasks event with ⏵⏵ prefix, got: {:?}", events),
        }
    }

    #[test]
    fn test_active_subtasks_not_triggered_by_single_triangle() {
        let mut parser = OutputParser::new();
        let events = parser.parse("some output with \u{23F5} single triangle");
        assert!(
            !events.iter().any(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. })),
            "Should not match single ⏵: {:?}", events
        );
    }

    #[test]
    fn test_active_subtasks_triangle_explicit_zero_count() {
        let mut parser = OutputParser::new();
        let events = parser.parse("\u{23F5}\u{23F5} finishing \u{00B7} 0 bash");
        match events.iter().find(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. })) {
            Some(ParsedEvent::ActiveSubtasks { count, .. }) => assert_eq!(*count, 0),
            _ => panic!("Expected ActiveSubtasks with count=0, got: {:?}", events),
        }
    }

    #[test]
    fn test_active_subtasks_triangle_embedded_in_multiline() {
        let mut parser = OutputParser::new();
        let input = "some other output\n\u{23F5}\u{23F5} working \u{00B7} 2 bash\nmore output after";
        let events = parser.parse(input);
        match events.iter().find(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. })) {
            Some(ParsedEvent::ActiveSubtasks { count, task_type }) => {
                assert_eq!(*count, 2);
                assert_eq!(task_type, "bash");
            }
            _ => panic!("Expected ActiveSubtasks in multiline input, got: {:?}", events),
        }
    }

    #[test]
    fn test_active_subtasks_triangle_background_tasks() {
        let mut parser = OutputParser::new();
        let events = parser.parse("\u{23F5}\u{23F5} fixing tests \u{00B7} 3 background tasks");
        match events.iter().find(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. })) {
            Some(ParsedEvent::ActiveSubtasks { count, task_type }) => {
                assert_eq!(*count, 3);
                assert_eq!(task_type, "background tasks");
            }
            _ => panic!("Expected ActiveSubtasks event with ⏵⏵ prefix, got: {:?}", events),
        }
    }

    // --- ActiveSubtasks: new format (count LEFT of markers) — captured from live sessions ---

    #[test]
    fn test_active_subtasks_new_format_count_left() {
        // Real: "1 shell · ⏵⏵ bypass permissions on" (CC v2.1.81, 2026-03-21)
        let mut parser = OutputParser::new();
        let events = parser.parse("1 shell \u{00B7} \u{23F5}\u{23F5} bypass permissions on");
        match events.iter().find(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. })) {
            Some(ParsedEvent::ActiveSubtasks { count, task_type }) => {
                assert_eq!(*count, 1);
                assert_eq!(task_type, "shell");
            }
            _ => panic!("Expected ActiveSubtasks with count-left format, got: {:?}", events),
        }
    }

    #[test]
    fn test_active_subtasks_new_format_plural() {
        let mut parser = OutputParser::new();
        let events = parser.parse("2 local agents \u{00B7} \u{23F5}\u{23F5} bypass permissions on");
        match events.iter().find(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. })) {
            Some(ParsedEvent::ActiveSubtasks { count, task_type }) => {
                assert_eq!(*count, 2);
                assert_eq!(task_type, "local agents");
            }
            _ => panic!("Expected ActiveSubtasks with count-left plural, got: {:?}", events),
        }
    }

    #[test]
    fn test_active_subtasks_new_format_with_hint() {
        // Mode line with both subprocess count and shift+tab hint
        let mut parser = OutputParser::new();
        let events = parser.parse("1 shell \u{00B7} \u{23F5}\u{23F5} bypass permissions on (shift+tab to cycle)");
        match events.iter().find(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. })) {
            Some(ParsedEvent::ActiveSubtasks { count, task_type }) => {
                assert_eq!(*count, 1);
                assert_eq!(task_type, "shell");
            }
            _ => panic!("Expected ActiveSubtasks with hint suffix, got: {:?}", events),
        }
    }

    #[test]
    fn test_active_subtasks_mode_line_with_hint_no_count() {
        // "⏵⏵ bypass permissions on (shift+tab to cycle)" — no subprocess count
        let mut parser = OutputParser::new();
        let events = parser.parse("\u{23F5}\u{23F5} bypass permissions on (shift+tab to cycle)");
        match events.iter().find(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. })) {
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
        match events.iter().find(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. })) {
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
        match events.iter().find(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. })) {
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
            !events.iter().any(|e| matches!(e, ParsedEvent::ActiveSubtasks { .. })),
            "random numbered line should not trigger subtasks: {:?}", events
        );
    }

    // --- Ink footer question detection ---

    #[test]
    fn test_ink_footer_detected_as_question() {
        let input = "Enter to select · ↑/↓ to navigate · Esc to cancel";
        let evt = parse_question(input);
        assert!(evt.is_some(), "Ink footer should trigger question detection");
        match evt.unwrap() {
            ParsedEvent::Question { confident, .. } => {
                assert!(confident, "Ink footer should be confident");
            }
            other => panic!("Expected Question, got: {:?}", other),
        }
    }

    #[test]
    fn test_ink_footer_in_diff_context_ignored() {
        // "Enter to select" inside a diff hunk should NOT trigger
        let input = "+  Enter to select an item";
        let evt = parse_question(input);
        assert!(evt.is_none(), "Ink footer in diff context should be ignored");
    }

    #[test]
    fn test_regular_question_not_detected_by_parser() {
        // Regular questions should NOT be detected by parse_question — they use
        // the silence-based detector in pty.rs instead.
        let input = "Do you want to proceed?";
        let evt = parse_question(input);
        assert!(evt.is_none(), "Regular questions use silence detector, not parse_question");
    }

}
