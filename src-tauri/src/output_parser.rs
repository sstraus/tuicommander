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
    },
    /// Claude Code usage limit: "You've used X% of your weekly/session limit"
    #[serde(rename = "usage-limit")]
    UsageLimit {
        percentage: u8,
        limit_type: String, // "weekly" or "session"
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
    /// Emitted via `[intent: <text>]` or `[intent: <text>(tab title)]` token in agent output.
    #[serde(rename = "intent")]
    Intent {
        text: String,
        /// Optional short title (max ~3 words) for use as tab name
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
    },
    /// Suggested follow-up actions for the user to choose from.
    /// Emitted via `[[suggest: A | B | C]]` token in agent output.
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
}

/// A single item in a slash command autocomplete menu.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
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
        }
    }

    /// Parse a chunk of PTY output and return any detected events.
    ///
    /// Strips ANSI escape sequences via the vt100 crate before parsing.
    /// Only available in tests — the production pipeline uses [`parse_clean_lines`].
    #[cfg(test)]
    pub fn parse(&self, text: &str) -> Vec<ParsedEvent> {
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

        // Question/attention detection
        if let Some(evt) = parse_question(&clean) {
            events.push(evt);
        }

        // Plan file detection
        if let Some(evt) = parse_plan_file(&clean) {
            events.push(evt);
        }

        // Intent declaration: [intent: <text>] or [[intent: <text>]] or ⟦intent: <text>⟧
        if let Some(evt) = parse_intent(&clean) {
            events.push(evt);
        }

        // Suggest follow-up actions: [suggest: A | B | C] or [[suggest: ...]]
        if let Some(evt) = parse_suggest(&clean) {
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
    pub fn parse_clean_lines(&self, rows: &[crate::state::ChangedRow]) -> Vec<ParsedEvent> {
        let mut events = Vec::new();
        // Join rows into a single string so multi-line parsers (rate_limit, etc.) work.
        // Individual row texts are already clean — no stripping required.
        let joined: String = rows.iter().map(|r| r.text.as_str()).collect::<Vec<_>>().join("\n");

        // PR/MR URL detection (operates on text directly)
        if let Some(evt) = parse_pr_url(&joined) {
            events.push(evt);
        }

        // Status line — iterates lines internally
        if let Some(evt) = parse_status_line(&joined) {
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

        // Question/attention — iterates lines internally
        if let Some(evt) = parse_question(&joined) {
            events.push(evt);
        }

        // Plan file
        if let Some(evt) = parse_plan_file(&joined) {
            events.push(evt);
        }

        // Intent and suggest
        if let Some(evt) = parse_intent(&joined) {
            events.push(evt);
        }
        if let Some(evt) = parse_suggest(&joined) {
            events.push(evt);
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
                if line_is_source_code(match_line)
                    || line_is_diff_or_code_context(match_line)
                {
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

    fn parse_api_error(&self, text: &str) -> Option<ParsedEvent> {
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
                if line_is_source_code(match_line)
                    || line_is_diff_or_code_context(match_line)
                {
                    continue;
                }
                return Some(ParsedEvent::ApiError {
                    pattern_name: pattern.name.to_string(),
                    matched_text: m.as_str().to_string(),
                    error_kind: pattern.error_kind.to_string(),
                });
            }
        }
        None
    }
}

/// Returns true if a line looks like source code, documentation, or agent commentary
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

/// Detect when an agent is waiting for user input from pre-stripped text (question, confirmation, menu choice).
fn parse_question(clean: &str) -> Option<ParsedEvent> {
    // Fast path: skip chunks that cannot contain any question marker.
    if !clean.contains('?') && !clean.contains("[Y/") && !clean.contains("[y/")
        && !clean.contains("[N/") && !clean.contains("[n/")
        && !clean.contains("(yes/no)") && !clean.contains("Enter to select")
        && !clean.contains('\u{276F}') && !clean.contains('\u{203A}')
        && !clean.contains("> ")
    {
        return None;
    }

    lazy_static::lazy_static! {
        // Claude Code: "Would you like to proceed?" / "Do you want to..."
        static ref QUESTION_RE: regex::Regex =
            regex::Regex::new(r"(?i)(Would you like to proceed|Do you want to\b[^?]*\?|Is this (plan|approach) okay)").unwrap();
        // Numbered menu choices: ❯ (U+276F), › (U+203A), >, or ) before "1." followed by option text
        static ref MENU_RE: regex::Regex =
            regex::Regex::new(r"[❯›>\)]\s*1\.\s+\S").unwrap();
        // Generic Y/N prompts: [Y/n], [y/N], (yes/no)
        static ref YN_RE: regex::Regex =
            regex::Regex::new(r"\[([Yy]/[Nn]|[Nn]/[Yy])\]|\(yes/no\)").unwrap();
        // "? " prefix (inquirer-style prompts used by many CLI tools)
        static ref INQUIRER_RE: regex::Regex =
            regex::Regex::new(r"^\?\s+.+\??\s*$").unwrap();
        // Ink SelectInput navigation footer: "Enter to select · ↑/↓ to navigate"
        static ref INK_FOOTER_RE: regex::Regex =
            regex::Regex::new(r"Enter to select").unwrap();
    }

    for line in clean.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }

        // Skip lines that look like diff output, code, or documentation —
        // they may contain question-like patterns as content, not real prompts.
        if line_is_diff_or_code_context(line) {
            continue;
        }

        if QUESTION_RE.is_match(trimmed) {
            return Some(ParsedEvent::Question {
                prompt_text: trimmed.to_string(),
            });
        }
        if MENU_RE.is_match(trimmed) {
            return Some(ParsedEvent::Question {
                prompt_text: trimmed.to_string(),
            });
        }
        if YN_RE.is_match(trimmed) {
            return Some(ParsedEvent::Question {
                prompt_text: trimmed.to_string(),
            });
        }
        if INQUIRER_RE.is_match(trimmed) {
            return Some(ParsedEvent::Question {
                prompt_text: trimmed.to_string(),
            });
        }
        if INK_FOOTER_RE.is_match(trimmed) {
            return Some(ParsedEvent::Question {
                prompt_text: trimmed.to_string(),
            });
        }
        // Generic `?`-ending lines are NOT detected here. They are handled
        // exclusively by the silence-based detector in pty.rs — if the agent
        // goes idle for 10s after printing a `?` line, it's a real question.
        // Instant detection of streaming fragments causes massive false positives
        // (e.g., "ad?", "swap?", "linux?", "instead?").
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
        // Excludes <>, $, {}, ` to avoid template placeholders and interpolation
        static ref PLAN_RE: regex::Regex =
            regex::Regex::new(r#"(?:^|[\s'":])(/?(?:[^\s'"<>${}`]+/)?plans/[^\s'"<>${}`]+\.mdx?)"#).unwrap();
    }
    for line in clean.lines() {
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

/// Colorize intent tokens yellow: `[[intent: text(title)]]` → `\e[2;33mintent: text\e[0m`.
/// Operates on raw PTY text via replace_all — only the matched token span is replaced,
/// preserving surrounding cursor movements (CUU/CUD) that position the text on screen.
/// The body stays raw (ANSI codes intact) so xterm.js renders it faithfully.
/// Brackets and the optional `(title)` suffix are stripped.
pub fn colorize_intent(raw: &str) -> String {
    lazy_static::lazy_static! {
        static ref INTENT_REPLACE_RE: regex::Regex = {
            // Any CSI sequence that may appear between structural elements
            let c = r"(?:\x1b\[[\x30-\x3f]*[\x20-\x2f]*[\x40-\x7e])*";
            regex::Regex::new(&format!(
                r"(?:\[\[?|\x{{27E6}}){C}intent:\s*(.+?)(?:\s*\([^)]*\))?\s*{C}(?:\]?\]|\x{{27E7}})",
                C = c
            )).unwrap()
        };
    }
    INTENT_REPLACE_RE.replace_all(raw, |caps: &regex::Captures| {
        let body = &caps[1]; // raw body, ANSI codes intact for xterm.js
        format!("\x1b[2;33mintent: {}\x1b[0m", body)
    }).into_owned()
}

/// Hide `[[suggest: ...]]` tokens from the xterm stream by wrapping them in SGR
/// conceal/reveal sequences (`\x1b[8m` … `\x1b[28m`).
///
/// Stripping the token entirely causes layout corruption because the surrounding
/// cursor-movement sequences (CUF, CUU, CUD) remain and mis-position subsequent output.
/// Concealing instead keeps the character count intact so cursor positions are unaffected,
/// while xterm.js renders the token invisible.
///
/// The regex tolerates CSI sequences between structural bracket/keyword elements — the
/// same technique used by `colorize_intent` — to handle re-rendered lines that contain
/// partial ANSI codes mid-token.
pub fn conceal_suggest(raw: &str) -> String {
    lazy_static::lazy_static! {
        static ref SUGGEST_CONCEAL_RE: regex::Regex = {
            let c = r"(?:\x1b\[[\x30-\x3f]*[\x20-\x2f]*[\x40-\x7e])*";
            regex::Regex::new(&format!(
                r"(?:\[\[?|\x{{27E6}}){C}suggest:\s*[^\]\x{{27E7}}]+?\s*{C}(?:\]?\]|\x{{27E7}})",
                C = c
            )).unwrap()
        };
    }
    SUGGEST_CONCEAL_RE.replace_all(raw, |caps: &regex::Captures| {
        format!("\x1b[8m{}\x1b[28m", &caps[0])
    }).into_owned()
}

/// Detect agent-declared intent tokens: `[intent: <text>]`, `[[intent: <text>]]`,
/// or `⟦intent: <text>⟧`. Agents are instructed (via MCP) to emit this token when
/// starting a new action, so the activity board can show what the agent is currently doing.
fn parse_intent(clean: &str) -> Option<ParsedEvent> {
    // Fast path: must contain "intent:"
    if !clean.contains("intent:") {
        return None;
    }
    lazy_static::lazy_static! {
        // [intent: <text>]  — ASCII single brackets
        // [[intent: <text>]] — ASCII double brackets (also accepted)
        // ⟦intent: <text>⟧ — Unicode mathematical brackets (U+27E6 / U+27E7)
        // Optional trailing (title) before the closing bracket.
        // (?:^|\s) anchor: the opening bracket must be at line/string start or after whitespace,
        // preventing matches on ANSI-stripped garbage like `]che[[intent:`
        // (?s:.+?) — DOTALL mode for the body so the token can span multiple joined rows.
        // When VtLogBuffer rows are joined with '\n' (parse_clean_lines), long tokens that
        // wrap at 80 cols would otherwise not match because '.' skips '\n' by default.
        static ref INTENT_RE: regex::Regex =
            regex::Regex::new(r"(?:^|\s)(?:\[\[?|\x{27E6})intent:\s*(?s:(.+?))\s*(?:\]?\]|\x{27E7})").unwrap();
        // Separate regex to split out the optional (title) suffix from the captured text.
        // (?s) enables DOTALL so the body can span multiple lines (wrapped tokens).
        static ref TITLE_RE: regex::Regex =
            regex::Regex::new(r"(?s)^(.*?)\(([^)]+)\)\s*$").unwrap();
    }
    INTENT_RE.captures(clean).and_then(|caps| {
        let raw = caps[1].trim();
        // Filter out meaningless intents: ellipsis, bare punctuation, template placeholders
        if raw == "..." || raw == "<text>" || raw.len() < 4 {
            return None;
        }
        // Try to extract optional (title) from the end
        let (text, title) = if let Some(tc) = TITLE_RE.captures(raw) {
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

/// Detect suggested follow-up actions: `[suggest: A | B | C]`, `[[suggest: ...]]`,
/// or `⟦suggest: ...⟧`. Pipe-separated items. At least one non-empty item required.
fn parse_suggest(clean: &str) -> Option<ParsedEvent> {
    if !clean.contains("suggest:") {
        return None;
    }
    lazy_static::lazy_static! {
        static ref SUGGEST_RE: regex::Regex =
            regex::Regex::new(r"(?:^|\s)(?:\[\[?|\x{27E6})suggest:\s*([^\]\x{27E7}]+?)\s*(?:\]?\]|\x{27E7})").unwrap();
    }
    SUGGEST_RE.captures(clean).and_then(|caps| {
        let raw = caps[1].trim();
        let items: Vec<String> = raw
            .split('|')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if items.is_empty() {
            return None;
        }
        Some(ParsedEvent::Suggest { items })
    })
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
    if !items.iter().any(|it| it.highlighted) {
        if let Some(first) = items.first_mut() {
            first.highlighted = true;
        }
    }

    Some(ParsedEvent::SlashMenu { items })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limit_claude_429() {
        let parser = OutputParser::new();
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
        let parser = OutputParser::new();
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
        let parser = OutputParser::new();
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
        let parser = OutputParser::new();
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
        let parser = OutputParser::new();
        let events = parser.parse("Hello world, everything is fine");
        assert!(events.is_empty());
    }

    #[test]
    fn test_github_pr_url() {
        let parser = OutputParser::new();
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
        let parser = OutputParser::new();
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
        let parser = OutputParser::new();
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
        let parser = OutputParser::new();
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
        let parser = OutputParser::new();
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
        let parser = OutputParser::new();
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
        let parser = OutputParser::new();
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
        let parser = OutputParser::new();
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
        let parser = OutputParser::new();
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
        let parser = OutputParser::new();
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
        let parser = OutputParser::new();
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
        let parser = OutputParser::new();
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
        let parser = OutputParser::new();
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
        let parser = OutputParser::new();
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
        let parser = OutputParser::new();
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
        let parser = OutputParser::new();
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
        let parser = OutputParser::new();
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
        let parser = OutputParser::new();
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
        let parser = OutputParser::new();
        let events = parser.parse(r#"    ("*/*") ..."#);
        assert!(
            !events.iter().any(|e| matches!(e, ParsedEvent::StatusLine { .. })),
            "Glob pattern in code should not match status line: {:?}", events
        );
    }

    #[test]
    fn test_status_line_rejects_stats_suffix() {
        // Claude Code stats like ") | [C2 S31 K30 A30 M7 H..." must NOT match.
        let parser = OutputParser::new();
        let events = parser.parse("* ) | [C2 S31 K30 A30 M7 H...");
        assert!(
            !events.iter().any(|e| matches!(e, ParsedEvent::StatusLine { .. })),
            "Stats suffix should be rejected by content validation: {:?}", events
        );
    }

    #[test]
    fn test_status_line_rejects_mid_line_asterisk() {
        // Asterisk mid-line in code should not trigger status-line detection
        let parser = OutputParser::new();
        let events = parser.parse(r#"  let result = a * b... done"#);
        assert!(
            !events.iter().any(|e| matches!(e, ParsedEvent::StatusLine { .. })),
            "Mid-line asterisk should not match: {:?}", events
        );
    }

    #[test]
    fn test_status_line_rejects_path_with_asterisk() {
        // Paths with wildcard should not trigger
        let parser = OutputParser::new();
        let events = parser.parse("src/**/*.ts...");
        assert!(
            !events.iter().any(|e| matches!(e, ParsedEvent::StatusLine { .. })),
            "Path with asterisk should not match: {:?}", events
        );
    }

    #[test]
    fn test_question_would_you_like() {
        let parser = OutputParser::new();
        let events = parser.parse("Would you like to proceed?");
        assert!(events.iter().any(|e| matches!(e, ParsedEvent::Question { .. })));
    }

    #[test]
    fn test_question_do_you_want() {
        let parser = OutputParser::new();
        let events = parser.parse("Do you want to continue with this approach?");
        assert!(events.iter().any(|e| matches!(e, ParsedEvent::Question { .. })));
    }

    #[test]
    fn test_question_menu_choice() {
        let parser = OutputParser::new();
        let events = parser.parse("❯ 1. Yes, clear context and bypass permissions");
        assert!(events.iter().any(|e| matches!(e, ParsedEvent::Question { .. })));
    }

    #[test]
    fn test_question_yn_prompt() {
        let parser = OutputParser::new();
        let events = parser.parse("Apply changes? [Y/n]");
        assert!(events.iter().any(|e| matches!(e, ParsedEvent::Question { .. })));
    }

    #[test]
    fn test_question_inquirer_style() {
        let parser = OutputParser::new();
        let events = parser.parse("? Which template would you like to use?");
        assert!(events.iter().any(|e| matches!(e, ParsedEvent::Question { .. })));
    }

    #[test]
    fn test_no_question_normal_output() {
        let parser = OutputParser::new();
        let events = parser.parse("Building project... done");
        assert!(!events.iter().any(|e| matches!(e, ParsedEvent::Question { .. })));
    }

    #[test]
    fn test_no_false_positives() {
        let parser = OutputParser::new();
        // Normal terminal output should produce no events
        let events = parser.parse("ls -la\ntotal 42\ndrwxr-xr-x  5 user staff 160 Jan 1 00:00 .\n");
        assert!(events.is_empty());
    }

    #[test]
    fn test_usage_limit_weekly() {
        let parser = OutputParser::new();
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
        let parser = OutputParser::new();
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
        let parser = OutputParser::new();
        let events = parser.parse("\x1b[33mYou've used 90% of your weekly limit\x1b[0m");
        assert!(events.iter().any(|e| matches!(e, ParsedEvent::UsageLimit { percentage: 90, .. })));
    }

    #[test]
    fn test_usage_limit_smart_quote() {
        let parser = OutputParser::new();
        let events = parser.parse("You\u{2019}ve used 50% of your weekly limit");
        assert!(events.iter().any(|e| matches!(e, ParsedEvent::UsageLimit { percentage: 50, .. })));
    }

    // --- Ink SelectInput / broadened question detection tests ---

    #[test]
    fn test_question_ink_single_angle_bracket_cursor() {
        // Ink SelectInput uses › (U+203A) as cursor indicator
        let parser = OutputParser::new();
        let events = parser.parse("› 1. Create a new story");
        assert!(events.iter().any(|e| matches!(e, ParsedEvent::Question { .. })),
            "› cursor should trigger question detection");
    }

    #[test]
    fn test_question_ascii_greater_than_cursor() {
        // Some CLIs use plain > as cursor indicator
        let parser = OutputParser::new();
        let events = parser.parse("> 1. Yes, proceed with changes");
        assert!(events.iter().any(|e| matches!(e, ParsedEvent::Question { .. })),
            "> cursor should trigger question detection");
    }

    #[test]
    fn test_question_ink_navigation_footer() {
        // Ink renders a navigation footer below selection menus
        let parser = OutputParser::new();
        let events = parser.parse("Enter to select · ↑/↓ to navigate · Esc to cancel");
        assert!(events.iter().any(|e| matches!(e, ParsedEvent::Question { .. })),
            "Ink navigation footer should trigger question detection");
    }

    #[test]
    fn test_question_ink_navigation_footer_partial() {
        // Some Ink variants only show "Enter to select"
        let parser = OutputParser::new();
        let events = parser.parse("Enter to select · ↑↓ to navigate");
        assert!(events.iter().any(|e| matches!(e, ParsedEvent::Question { .. })),
            "Partial Ink navigation footer should trigger question detection");
    }

    #[test]
    fn test_question_generic_question_mark_not_instant() {
        // Generic `?`-ending lines are NOT detected by the instant parser —
        // they are handled by the silence-based detector in pty.rs to avoid
        // false positives from streaming fragments like "ad?", "swap?", "?"
        let parser = OutputParser::new();
        let events = parser.parse("What should we do with this story?");
        assert!(!events.iter().any(|e| matches!(e, ParsedEvent::Question { .. })),
            "Generic ?-ending lines should NOT trigger instant detection");
    }

    #[test]
    fn test_question_generic_not_prose() {
        // Lines that look like prose/code should NOT trigger the generic ? match
        let parser = OutputParser::new();
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
    fn test_question_ink_full_menu_block() {
        // A realistic Ink SelectInput output block
        let parser = OutputParser::new();
        let block = "\
What should we do with this story?

  1. Create a new story
› 2. Update existing story
  3. Skip it
  4. Other

Enter to select · ↑/↓ to navigate · Esc to cancel";
        let events = parser.parse(block);
        assert!(events.iter().any(|e| matches!(e, ParsedEvent::Question { .. })),
            "Full Ink menu block should trigger question detection");
    }

    #[test]
    fn test_no_question_blockquote_with_question() {
        let parser = OutputParser::new();
        assert!(!parser.parse("> Do you agree with this approach?")
            .iter().any(|e| matches!(e, ParsedEvent::Question { .. })));
    }

    #[test]
    fn test_no_question_bold_markdown() {
        let parser = OutputParser::new();
        assert!(!parser.parse("**Should we refactor this?**")
            .iter().any(|e| matches!(e, ParsedEvent::Question { .. })));
    }

    #[test]
    fn test_no_question_shell_prompt_with_greater_than() {
        // Shell prompts like "> command" should NOT trigger menu detection
        let parser = OutputParser::new();
        assert!(!parser.parse("> git status")
            .iter().any(|e| matches!(e, ParsedEvent::Question { .. })));
    }

    #[test]
    fn test_no_question_enter_in_prose() {
        // Prose mentioning "Enter to select" in a different context should still match,
        // but "Press Enter to continue" should NOT
        let parser = OutputParser::new();
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
        let parser = OutputParser::new();
        let events = parser.parse("Plan saved to plans/my-feature.md");
        assert_eq!(get_plan_path(&events), Some("plans/my-feature.md".to_string()));
    }

    #[test]
    fn test_plan_file_dot_claude() {
        let parser = OutputParser::new();
        let events = parser.parse("Writing plan: .claude/plans/auth-flow.md");
        assert_eq!(get_plan_path(&events), Some(".claude/plans/auth-flow.md".to_string()));
    }

    #[test]
    fn test_plan_file_absolute() {
        let parser = OutputParser::new();
        let events = parser.parse("Created /Users/dev/project/plans/refactor.md");
        assert_eq!(get_plan_path(&events), Some("/Users/dev/project/plans/refactor.md".to_string()));
    }

    #[test]
    fn test_plan_file_claude_private() {
        let parser = OutputParser::new();
        let events = parser.parse("Plan: .claude-private/plans/serene-waterfall.md");
        assert_eq!(get_plan_path(&events), Some(".claude-private/plans/serene-waterfall.md".to_string()));
    }

    #[test]
    fn test_plan_file_tilde_expanded() {
        let parser = OutputParser::new();
        let events = parser.parse("Plan saved to ~/.claude/plans/graceful-rolling-quasar.md");
        let path = get_plan_path(&events).expect("should detect tilde plan path");
        // Tilde must be expanded to an absolute path
        assert!(!path.starts_with("~"), "tilde should be expanded: {path}");
        assert!(path.ends_with("/.claude/plans/graceful-rolling-quasar.md"));
        assert!(path.starts_with("/"), "path should be absolute: {path}");
    }

    #[test]
    fn test_plan_file_no_match() {
        let parser = OutputParser::new();
        let events = parser.parse("Building project... done");
        assert!(get_plan_path(&events).is_none());
    }

    #[test]
    fn test_plan_file_not_md() {
        let parser = OutputParser::new();
        // "plans/foo.ts" should NOT match (not a markdown file)
        let events = parser.parse("Reading plans/foo.ts");
        assert!(get_plan_path(&events).is_none());
    }

    #[test]
    fn test_plan_file_template_placeholder_rejected() {
        let parser = OutputParser::new();
        // Template placeholders like <file> or <filename> should NOT match
        assert!(get_plan_path(&parser.parse("plans/<file>.md")).is_none());
        assert!(get_plan_path(&parser.parse("plans/<filename>.md")).is_none());
        assert!(get_plan_path(&parser.parse("Save to .claude/plans/<name>.md")).is_none());
    }

    #[test]
    fn test_plan_file_interpolation_rejected() {
        let parser = OutputParser::new();
        // Shell/JS interpolation and backticks should NOT match
        assert!(get_plan_path(&parser.parse("plans/new-${i}.md")).is_none());
        assert!(get_plan_path(&parser.parse("plans/${name}.md")).is_none());
        assert!(get_plan_path(&parser.parse("plans/`cmd`.md")).is_none());
        assert!(get_plan_path(&parser.parse("Save to plans/foo-${bar}-baz.md")).is_none());
    }

    // --- False positive prevention tests ---

    fn has_rate_limit(events: &[ParsedEvent]) -> bool {
        events.iter().any(|e| matches!(e, ParsedEvent::RateLimit { .. }))
    }

    #[test]
    fn test_no_false_positive_conversational_rate_limit() {
        let parser = OutputParser::new();
        // Agent discussing rate limits in prose should NOT trigger detection
        assert!(!has_rate_limit(&parser.parse("The rate limit detection was triggering false positives")));
        assert!(!has_rate_limit(&parser.parse("I fixed the rate-limited pattern matching")));
        assert!(!has_rate_limit(&parser.parse("We should handle too many requests gracefully")));
        assert!(!has_rate_limit(&parser.parse("The rate limiting logic needs improvement")));
    }

    #[test]
    fn test_no_false_positive_code_output() {
        let parser = OutputParser::new();
        // Code snippets mentioning rate limits should NOT trigger
        assert!(!has_rate_limit(&parser.parse("rl(\"rate-limit-keyword\", r\"rate[- ]?limit\", Some(60000))")));
        assert!(!has_rate_limit(&parser.parse("// Handle too many requests from the API")));
        assert!(!has_rate_limit(&parser.parse("fn handle_rate_limit(retry_after: u64) {")));
    }

    #[test]
    fn test_no_false_positive_tpm_rpm_acronyms() {
        let parser = OutputParser::new();
        // TPM/RPM in non-rate-limit context should NOT trigger
        assert!(!has_rate_limit(&parser.parse("TPM 2.0 module detected")));
        assert!(!has_rate_limit(&parser.parse("RPM package manager installed")));
        assert!(!has_rate_limit(&parser.parse("The disk spins at 7200 RPM")));
    }

    #[test]
    fn test_http_429_real_errors_still_detected() {
        let parser = OutputParser::new();
        // Real HTTP 429 errors should still be detected
        assert!(has_rate_limit(&parser.parse("HTTP/1.1 429 Too Many Requests")));
        assert!(has_rate_limit(&parser.parse("429 Too Many Requests")));
        assert!(has_rate_limit(&parser.parse("HTTP 429")));
    }

    #[test]
    fn test_real_api_errors_still_detected() {
        let parser = OutputParser::new();
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
        let parser = OutputParser::new();
        // Agent reading output_parser.rs — the exact lines that caused the bug
        assert!(!has_rate_limit(&parser.parse(r#"        rl("claude-http-429", r"(?i)rate_limit_error", Some(60000), false),"#)));
        assert!(!has_rate_limit(&parser.parse(r#"        rl("claude-overloaded", r"(?i)overloaded_error", Some(30000), false),"#)));
        assert!(!has_rate_limit(&parser.parse(r#"        rl("openai-http-429", r"RateLimitError", Some(60000), false),"#)));
        assert!(!has_rate_limit(&parser.parse(r#"        rl("gemini-resource-exhausted", r"RESOURCE_EXHAUSTED", Some(60000), false),"#)));
        assert!(!has_rate_limit(&parser.parse(r#"        rl("retry-after-header", r"(?i)Retry-After:\s*(\d+)", None, true),"#)));
    }

    #[test]
    fn test_no_false_positive_code_comments() {
        let parser = OutputParser::new();
        assert!(!has_rate_limit(&parser.parse("// Error: rate_limit_error")));
        assert!(!has_rate_limit(&parser.parse("# Handle RESOURCE_EXHAUSTED from Gemini")));
    }

    #[test]
    fn test_no_false_positive_test_assertions() {
        let parser = OutputParser::new();
        assert!(!has_rate_limit(&parser.parse(r#"        assert!(has_rate_limit(&parser.parse("Error: rate_limit_error")));"#)));
        assert!(!has_rate_limit(&parser.parse(r#"        assert!(has_rate_limit(&parser.parse("RateLimitError: exceeded quota")));"#)));
    }

    #[test]
    fn test_no_false_positive_markdown_code_fences() {
        let parser = OutputParser::new();
        assert!(!has_rate_limit(&parser.parse("```rust\nrl(\"claude-http-429\", r\"rate_limit_error\")")));
        assert!(!has_rate_limit(&parser.parse("- `rate_limit_error` — Claude API error code")));
        assert!(!has_rate_limit(&parser.parse("* Pattern `RateLimitError` matches OpenAI errors")));
    }

    #[test]
    fn test_no_false_positive_markdown_table() {
        let parser = OutputParser::new();
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
        let parser = OutputParser::new();
        let input = r#"API Error: 500 {"type":"error","error":{"type":"api_error","message":"Internal server error"},"request_id":"req_011CYV92oEFMbcz45mjVYssM"}"#;
        let events = parser.parse(input);
        let (name, _, kind) = get_api_error(&events).expect("should detect Claude API error");
        assert_eq!(name, "claude-api-error");
        assert_eq!(kind, "server");
    }

    #[test]
    fn test_api_error_claude_529_overloaded() {
        let parser = OutputParser::new();
        // 529 overloaded should be caught by rate limit (overloaded_error), not api-error
        // But the api_error JSON type should NOT match overloaded_error
        let input = r#"API Error: 529 {"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#;
        // overloaded_error is a rate limit pattern, not api-error
        assert!(!has_api_error(&parser.parse(input)));
        assert!(has_rate_limit(&parser.parse(input)));
    }

    #[test]
    fn test_api_error_claude_auth() {
        let parser = OutputParser::new();
        let input = r#"API Error: 401 {"type":"error","error":{"type":"authentication_error","message":"Invalid bearer token"}}"#;
        let events = parser.parse(input);
        let (name, _, kind) = get_api_error(&events).expect("should detect Claude auth error");
        assert_eq!(name, "claude-auth-error");
        assert_eq!(kind, "auth");
    }

    #[test]
    fn test_api_error_gemini_unavailable() {
        let parser = OutputParser::new();
        let input = r#"API Error: got status: UNAVAILABLE. {"error":{"code":503,"message":"The model is overloaded."}}"#;
        let events = parser.parse(input);
        let (name, _, kind) = get_api_error(&events).expect("should detect Gemini UNAVAILABLE");
        assert_eq!(name, "gemini-server-error");
        assert_eq!(kind, "server");
    }

    #[test]
    fn test_api_error_gemini_internal() {
        let parser = OutputParser::new();
        let input = r#"API Error: got status: INTERNAL. {"error":{"code":500,"message":"An internal error has occurred."}}"#;
        let events = parser.parse(input);
        let (name, _, kind) = get_api_error(&events).expect("should detect Gemini INTERNAL");
        assert_eq!(name, "gemini-server-error");
        assert_eq!(kind, "server");
    }

    #[test]
    fn test_api_error_aider_server() {
        let parser = OutputParser::new();
        let input = r#"litellm.InternalServerError: AnthropicException - {"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#;
        let events = parser.parse(input);
        let (name, _, kind) = get_api_error(&events).expect("should detect Aider server error");
        assert_eq!(name, "aider-server-error");
        assert_eq!(kind, "server");
    }

    #[test]
    fn test_api_error_aider_auth() {
        let parser = OutputParser::new();
        let input = "litellm.AuthenticationError: AnthropicException - invalid x-api-key";
        let events = parser.parse(input);
        let (name, _, kind) = get_api_error(&events).expect("should detect Aider auth error");
        assert_eq!(name, "aider-auth-error");
        assert_eq!(kind, "auth");
    }

    #[test]
    fn test_api_error_aider_translated_server() {
        let parser = OutputParser::new();
        let input = "The API provider's servers are down or overloaded.";
        let events = parser.parse(input);
        let (name, _, kind) = get_api_error(&events).expect("should detect Aider translated msg");
        assert_eq!(name, "aider-server-msg");
        assert_eq!(kind, "server");
    }

    #[test]
    fn test_api_error_aider_translated_auth() {
        let parser = OutputParser::new();
        let input = "The API provider is not able to authenticate you. Check your API key.";
        let events = parser.parse(input);
        let (name, _, kind) = get_api_error(&events).expect("should detect Aider auth msg");
        assert_eq!(name, "aider-auth-msg");
        assert_eq!(kind, "auth");
    }

    #[test]
    fn test_api_error_codex_stream_error() {
        let parser = OutputParser::new();
        let input = "⚠  stream error: exceeded retry limit, last status: 401 Unauthorized; retrying 5/5 in 3.087s…";
        let events = parser.parse(input);
        let (name, _, kind) = get_api_error(&events).expect("should detect Codex stream error");
        assert_eq!(name, "codex-stream-error");
        assert_eq!(kind, "server");
    }

    #[test]
    fn test_api_error_codex_500() {
        let parser = OutputParser::new();
        let input = "stream error: exceeded retry limit, last status: 500 Internal Server Error";
        let events = parser.parse(input);
        assert!(has_api_error(&events));
    }

    #[test]
    fn test_api_error_copilot_token() {
        let parser = OutputParser::new();
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
        let parser = OutputParser::new();
        let input = "request failed unexpectedly";
        let events = parser.parse(input);
        assert!(!has_api_error(&events));
    }

    // --- Provider-level API error tests ---

    #[test]
    fn test_api_error_openai_server_error() {
        let parser = OutputParser::new();
        let input = r#"{"error":{"message":"The server had an error","type":"server_error","param":null,"code":null}}"#;
        let events = parser.parse(input);
        let (name, _, kind) = get_api_error(&events).expect("should detect OpenAI server_error");
        assert_eq!(name, "openai-server-error");
        assert_eq!(kind, "server");
    }

    #[test]
    fn test_api_error_google_internal() {
        let parser = OutputParser::new();
        let input = r#"{"error":{"code":500,"message":"An internal error has occurred.","status":"INTERNAL"}}"#;
        let events = parser.parse(input);
        let (name, _, kind) = get_api_error(&events).expect("should detect Google INTERNAL");
        assert_eq!(name, "google-api-server");
        assert_eq!(kind, "server");
    }

    #[test]
    fn test_api_error_google_unavailable() {
        let parser = OutputParser::new();
        let input = r#"{"error":{"code":503,"message":"The service is currently unavailable.","status":"UNAVAILABLE"}}"#;
        let events = parser.parse(input);
        let (name, _, kind) = get_api_error(&events).expect("should detect Google UNAVAILABLE");
        assert_eq!(name, "google-api-server");
        assert_eq!(kind, "server");
    }

    #[test]
    fn test_api_error_google_auth() {
        let parser = OutputParser::new();
        let input = r#"{"error":{"code":401,"message":"Request had invalid authentication credentials.","status":"UNAUTHENTICATED"}}"#;
        let events = parser.parse(input);
        let (name, _, kind) = get_api_error(&events).expect("should detect Google UNAUTHENTICATED");
        assert_eq!(name, "google-api-auth");
        assert_eq!(kind, "auth");
    }

    #[test]
    fn test_api_error_openrouter() {
        let parser = OutputParser::new();
        let input = r#"{"error":{"code":502,"message":"Your chosen model is down","metadata":{"provider_name":"Anthropic"}}}"#;
        let events = parser.parse(input);
        let (name, _, kind) = get_api_error(&events).expect("should detect OpenRouter error");
        assert_eq!(name, "openrouter-server");
        assert_eq!(kind, "server");
    }

    #[test]
    fn test_api_error_minimax() {
        let parser = OutputParser::new();
        let input = r#"{"id":"abc","base_resp":{"status_code":1013,"status_msg":"internal service error"}}"#;
        let events = parser.parse(input);
        let (name, _, kind) = get_api_error(&events).expect("should detect MiniMax error");
        assert_eq!(name, "minimax-server");
        assert_eq!(kind, "server");
    }

    #[test]
    fn test_no_api_error_normal_output() {
        let parser = OutputParser::new();
        assert!(!has_api_error(&parser.parse("Building project... done")));
        assert!(!has_api_error(&parser.parse("ls -la\ntotal 42")));
        assert!(!has_api_error(&parser.parse("Hello world, everything is fine")));
    }

    #[test]
    fn test_no_api_error_false_positive_source_code() {
        let parser = OutputParser::new();
        // Agent reading this very source file should not trigger
        assert!(!has_api_error(&parser.parse("        ae(\"claude-api-error\", \"type\":\"api_error\", \"server\"),")));
        assert!(!has_api_error(&parser.parse("// detect \"type\":\"api_error\" in JSON")));
        assert!(!has_api_error(&parser.parse("# Handle authentication_error from Claude")));
    }

    // --- Diff output false-positive prevention tests ---

    fn has_question(events: &[ParsedEvent]) -> bool {
        events.iter().any(|e| matches!(e, ParsedEvent::Question { .. }))
    }

    #[test]
    fn test_no_question_diff_line_with_menu_pattern() {
        let parser = OutputParser::new();
        // Diff line from output_parser.rs containing ") 1." pattern — NOT a real menu
        assert!(!has_question(&parser.parse(
            "462 -        // Numbered menu choices: ❯ 1. or ) 1. followed by option text"
        )));
    }

    #[test]
    fn test_no_question_diff_line_with_yn_pattern() {
        let parser = OutputParser::new();
        // Diff line from docs containing [Y/n] pattern — NOT a real Y/N prompt
        assert!(!has_question(&parser.parse(
            "465 //GenericY/Nprompts:[Y/n],[y/N],(yes/no)"
        )));
    }

    #[test]
    fn test_no_question_diff_line_with_hardcoded_prompts() {
        let parser = OutputParser::new();
        // Markdown doc line in diff containing question patterns — NOT a real question
        assert!(!has_question(&parser.parse(
            r#"75 +- **Hardcoded prompts**: "Would you like to proceed?", "Do you want to...?", "Is this plan/a"#
        )));
    }

    #[test]
    fn test_no_question_diff_line_with_yn_doc() {
        let parser = OutputParser::new();
        // Markdown doc line in diff listing Y/N patterns — NOT a real prompt
        assert!(!has_question(&parser.parse(
            "77 +- **Y/N prompts**: `[Y/n]`, `[y/N]`, `(yes/no)`"
        )));
    }

    #[test]
    fn test_no_question_diff_hunk_with_code_changes() {
        let parser = OutputParser::new();
        // Claude Code diff summary block — NOT a real question
        assert!(!has_question(&parser.parse(
            "⏺⎿ Added16lines,removed2lines     459          // Claude Code: \"Would you like to proceed?\" / \"Do you want to...\""
        )));
    }

    #[test]
    fn test_no_rate_limit_in_diff_output() {
        let parser = OutputParser::new();
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
        let parser = OutputParser::new();
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
    fn test_intent_basic() {
        let parser = OutputParser::new();
        let events = parser.parse("[intent: Refactoring the auth module]");
        assert_eq!(get_intent(&events), Some("Refactoring the auth module".to_string()));
    }

    #[test]
    fn test_intent_double_brackets() {
        let parser = OutputParser::new();
        // Double brackets still accepted for backward compatibility
        let events = parser.parse("[[intent: Refactoring the auth module]]");
        assert_eq!(get_intent(&events), Some("Refactoring the auth module".to_string()));
    }

    #[test]
    fn test_intent_unicode_brackets() {
        let parser = OutputParser::new();
        let events = parser.parse("\u{27E6}intent: Writing unit tests\u{27E7}");
        assert_eq!(get_intent(&events), Some("Writing unit tests".to_string()));
    }

    #[test]
    fn test_intent_in_multiline_output() {
        let parser = OutputParser::new();
        let events = parser.parse("Some output\n[intent: Debugging login flow]\nMore output");
        assert_eq!(get_intent(&events), Some("Debugging login flow".to_string()));
    }

    #[test]
    fn test_intent_with_ansi() {
        let parser = OutputParser::new();
        let events = parser.parse("\x1b[33m[intent: Reviewing PR changes]\x1b[0m");
        assert_eq!(get_intent(&events), Some("Reviewing PR changes".to_string()));
    }

    #[test]
    fn test_no_intent_normal_output() {
        let parser = OutputParser::new();
        assert!(get_intent(&parser.parse("Building project... done")).is_none());
        assert!(get_intent(&parser.parse("The intent is to refactor")).is_none());
    }

    #[test]
    fn test_intent_single_brackets() {
        let parser = OutputParser::new();
        // Single brackets now accepted as the canonical format
        let events = parser.parse("[intent: something cool]");
        assert_eq!(get_intent(&events), Some("something cool".to_string()));
    }

    #[test]
    fn test_intent_trims_whitespace() {
        let parser = OutputParser::new();
        let events = parser.parse("[intent:   Fix the flaky test   ]");
        assert_eq!(get_intent(&events), Some("Fix the flaky test".to_string()));
    }

    #[test]
    fn test_intent_ellipsis_filtered() {
        let parser = OutputParser::new();
        assert!(get_intent(&parser.parse("[intent: ...]")).is_none());
    }

    #[test]
    fn test_intent_template_placeholder_filtered() {
        let parser = OutputParser::new();
        assert!(get_intent(&parser.parse("[intent: <text>]")).is_none());
    }

    #[test]
    fn test_intent_too_short_filtered() {
        let parser = OutputParser::new();
        assert!(get_intent(&parser.parse("[intent: ab]")).is_none());
    }

    #[test]
    fn test_colorize_intent_single_brackets() {
        let raw = "Some output\n[intent: Refactoring auth]\nMore output";
        let colored = colorize_intent(raw);
        assert_eq!(colored, "Some output\n\x1b[2;33mintent: Refactoring auth\x1b[0m\nMore output");
    }

    #[test]
    fn test_colorize_intent_double_brackets() {
        let raw = "Some output\n[[intent: Refactoring auth]]\nMore output";
        let colored = colorize_intent(raw);
        assert_eq!(colored, "Some output\n\x1b[2;33mintent: Refactoring auth\x1b[0m\nMore output");
    }

    #[test]
    fn test_colorize_intent_no_match_unchanged() {
        let raw = "Normal terminal output with no intent";
        assert_eq!(colorize_intent(raw), raw);
    }

    // ---- False positive regression tests ----

    fn has_status_line(events: &[ParsedEvent]) -> bool {
        events.iter().any(|e| matches!(e, ParsedEvent::StatusLine { .. }))
    }

    #[test]
    fn test_no_status_line_in_diff_output() {
        let parser = OutputParser::new();
        // Diff line containing * and ... in JSON — should not trigger status line
        assert!(!has_status_line(&parser.parse(
            "484 + *   - {\"type\":\"output\",\"data\":\"...\"} for raw PTY output"
        )));
    }

    #[test]
    fn test_no_status_line_in_css_comment() {
        let parser = OutputParser::new();
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
        let parser = OutputParser::new();
        // Code listing with line numbers and * in a comment
        assert!(!has_status_line(&parser.parse(
            "156  /* Last prompt sub-row */                                                                                                                    "
        )));
    }

    #[test]
    fn test_no_status_line_from_markdown_bullet() {
        let parser = OutputParser::new();
        // Markdown bullet list — should NOT trigger Codex bullet pattern
        assert!(!has_status_line(&parser.parse("• This is a bullet point in a list")));
        assert!(!has_status_line(&parser.parse("  • Another nested bullet item")));
    }

    #[test]
    fn test_no_intent_from_ansi_garbage() {
        let parser = OutputParser::new();
        // ANSI-stripped garbage producing ]che[[intent: — not a real intent token
        assert!(get_intent(&parser.parse("]che[[intent: some text]]")).is_none());
        // Must be at line start or after whitespace
        assert!(get_intent(&parser.parse("garbage[[intent: some text]]")).is_none());
    }

    // --- Intent title tests ---

    #[test]
    fn test_intent_with_title() {
        let parser = OutputParser::new();
        let events = parser.parse("[[intent: Reading auth module for token flow(Reading auth)]]");
        assert_eq!(get_intent(&events), Some("Reading auth module for token flow".to_string()));
        assert_eq!(get_intent_title(&events), Some("Reading auth".to_string()));
    }

    #[test]
    fn test_intent_with_title_single_brackets() {
        let parser = OutputParser::new();
        let events = parser.parse("[intent: Writing parser unit tests(Writing tests)]");
        assert_eq!(get_intent(&events), Some("Writing parser unit tests".to_string()));
        assert_eq!(get_intent_title(&events), Some("Writing tests".to_string()));
    }

    #[test]
    fn test_intent_with_title_unicode_brackets() {
        let parser = OutputParser::new();
        let events = parser.parse("\u{27E6}intent: Debugging login redirect(Debugging redirect)\u{27E7}");
        assert_eq!(get_intent(&events), Some("Debugging login redirect".to_string()));
        assert_eq!(get_intent_title(&events), Some("Debugging redirect".to_string()));
    }

    #[test]
    fn test_intent_without_title_still_works() {
        let parser = OutputParser::new();
        let events = parser.parse("[[intent: Refactoring the auth module]]");
        assert_eq!(get_intent(&events), Some("Refactoring the auth module".to_string()));
        assert_eq!(get_intent_title(&events), None);
    }

    #[test]
    fn test_intent_title_trimmed() {
        let parser = OutputParser::new();
        let events = parser.parse("[[intent: Some task here(  Tab title  )]]");
        assert_eq!(get_intent_title(&events), Some("Tab title".to_string()));
    }

    #[test]
    fn test_colorize_intent_strips_title() {
        let raw = "[[intent: Reading auth module for token flow(Reading auth)]]";
        let colored = colorize_intent(raw);
        assert_eq!(colored, "\x1b[2;33mintent: Reading auth module for token flow\x1b[0m");
    }

    #[test]
    fn test_colorize_intent_without_title_unchanged() {
        let raw = "[[intent: Refactoring auth]]";
        let colored = colorize_intent(raw);
        assert_eq!(colored, "\x1b[2;33mintent: Refactoring auth\x1b[0m");
    }

    #[test]
    fn test_intent_with_bullet_prefix() {
        // Claude Code prefixes output lines with ⏺ (U+25CF)
        let parser = OutputParser::new();
        let input = "\u{25CF} [[intent: Implementing agentTeamsShim config field(Config field)]]";
        let events = parser.parse(input);
        let text = get_intent(&events);
        let title = get_intent_title(&events);
        assert_eq!(text, Some("Implementing agentTeamsShim config field".to_string()),
            "intent text not extracted from bullet-prefixed input");
        assert_eq!(title, Some("Config field".to_string()),
            "intent title not extracted from bullet-prefixed input");
    }

    #[test]
    fn test_colorize_intent_with_bullet_prefix() {
        // Verify colorize strips brackets + title even with ⏺ prefix
        let raw = "\u{25CF} [[intent: Implementing agentTeamsShim config field(Config field)]]";
        let colored = colorize_intent(raw);
        assert!(colored.contains("\x1b[2;33m"), "should contain yellow ANSI");
        assert!(!colored.contains("(Config field)"), "should strip title from visible output");
        assert!(!colored.contains("[["), "should strip opening brackets");
        assert!(!colored.contains("]]"), "should strip closing brackets");
    }

    #[test]
    fn test_intent_with_ansi_codes_interleaved() {
        // ANSI codes wrapping bullet + dim around the intent token
        let parser = OutputParser::new();
        let raw = "\x1b[1m\u{25CF}\x1b[0m \x1b[2m[[intent: Implementing config(Config field)]]\x1b[0m";
        let events = parser.parse(raw);
        assert_eq!(get_intent(&events), Some("Implementing config".to_string()));
        assert_eq!(get_intent_title(&events), Some("Config field".to_string()));
        let colored = colorize_intent(raw);
        assert!(colored.contains("\x1b[2;33m"), "should colorize yellow");
        assert!(!colored.contains("(Config field)"), "should strip title");
        assert!(!colored.contains("[["), "should strip brackets");
    }

    #[test]
    fn test_colorize_intent_ansi_inside_brackets() {
        // ANSI codes scattered inside the [[intent:...]] token itself
        let raw = "\x1b[2m[[\x1b[0mintent: Implementing config(Config field)\x1b[2m]]\x1b[0m";
        let colored = colorize_intent(raw);
        assert!(colored.contains("\x1b[2;33m"),
            "should colorize even with ANSI inside brackets; got: {:?}", colored);
        assert!(!colored.contains("(Config field)"), "should strip title");
    }

    #[test]
    fn test_colorize_intent_crlf_line_endings() {
        // PTY output uses \r\n line endings. The \r at end of line must NOT
        // cause apply_carriage_returns to wipe the content.
        let raw = "some output\r\n\u{25CF} [[intent: Removing memory(Cleanup)]]\r\nmore output\r\n";
        let colored = colorize_intent(raw);
        assert!(colored.contains("\x1b[2;33m"),
            "should colorize with CRLF endings; got: {:?}", colored);
        assert!(!colored.contains("[["), "should strip opening brackets");
        assert!(!colored.contains("]]"), "should strip closing brackets");
        assert!(!colored.contains("(Cleanup)"), "should strip title");
        assert!(colored.contains("some output"), "non-intent lines preserved");
        assert!(colored.contains("more output"), "non-intent lines preserved");
    }

    #[test]
    fn test_colorize_intent_crlf_no_title() {
        // Same CRLF scenario but without a title suffix
        let raw = "\x1b[1m\u{25CF}\x1b[0m [[intent: Removing memory: local from reviewer agents]]\r\n";
        let colored = colorize_intent(raw);
        assert!(colored.contains("\x1b[2;33m"),
            "should colorize with CRLF; got: {:?}", colored);
        assert!(!colored.contains("[["), "should strip brackets");
        assert!(colored.contains("Removing memory"), "intent text preserved");
    }

    #[test]
    fn test_no_rate_limit_story_429() {
        let parser = OutputParser::new();
        // Conversational text mentioning "story 429" — not an HTTP 429
        assert!(!has_rate_limit(&parser.parse(
            "che sembrano provenire da altre sessioni (story 429"
        )));
    }

    #[test]
    fn test_no_rate_limit_ansi_bridged_429() {
        let parser = OutputParser::new();
        // Raw ANSI output where \S* bridges http/ to 429 through escape codes
        assert!(!has_rate_limit(&parser.parse(
            "http/\x1b[1C\x1b[39me\x1b[1C\x1b[38;2;177;185;249mstate.rs\x1b[1Cche\x1b[1Csembrano\x1b[1Cprovenire\x1b[1Cda\x1b[1Caltre\x1b[1Csessioni\x1b[1C(story\x1b[1C429"
        )));
    }

    #[test]
    fn test_intent_with_cursor_up_down() {
        // Real Claude Code PTY output: spinner uses CUU (\x1b[8A) to go back up,
        // then the intent token appears after cursor movements.
        // VtLogBuffer renders the VT100 and produces the clean row; we simulate that here.
        let parser = OutputParser::new();
        let raw = "\x1b[38;2;215;119;87m\u{273b}\x1b[39m\r\r\n\r\n\r\n\r\n\x1b[?2026l\x1b[?2026h\r\x1b[8A\x1b[38;2;153;153;153m\u{25cf}\x1b[1C\x1b[39m\x1b[1mBash\x1b[22m\r\x1b[1B  \x1b[38;2;177;185;249m[[intent: Fixing cursor handling(Fix cursor)]]\x1b[39m\r\x1b[1Bmore output";
        let mut vt_parser = vt100::Parser::new(50, 220, 0);
        vt_parser.process(raw.as_bytes());
        let screen = vt_parser.screen();
        let rows: Vec<crate::state::ChangedRow> = screen.rows(0, screen.size().1)
            .enumerate()
            .filter(|(_, r)| !r.trim_end().is_empty())
            .map(|(i, r)| crate::state::ChangedRow { row_index: i, text: r.trim_end().to_string() })
            .collect();
        let events = parser.parse_clean_lines(&rows);
        assert_eq!(
            get_intent(&events),
            Some("Fixing cursor handling".to_string()),
            "intent must be detected via VtLogBuffer-rendered rows; got: {:?}", events
        );
        assert_eq!(
            get_intent_title(&events),
            Some("Fix cursor".to_string()),
        );
    }

    #[test]
    fn test_intent_with_cursor_down_only() {
        // Intent token preceded by CUD (cursor down) — \x1b[nB
        // VtLogBuffer renders this to clean rows; simulate the result.
        let parser = OutputParser::new();
        let raw = "spinner output\x1b[3B[[intent: Running tests(Tests)]]\x1b[2Amore";
        let mut vt_parser = vt100::Parser::new(50, 220, 0);
        vt_parser.process(raw.as_bytes());
        let screen = vt_parser.screen();
        let rows: Vec<crate::state::ChangedRow> = screen.rows(0, screen.size().1)
            .enumerate()
            .filter(|(_, r)| !r.trim_end().is_empty())
            .map(|(i, r)| crate::state::ChangedRow { row_index: i, text: r.trim_end().to_string() })
            .collect();
        let events = parser.parse_clean_lines(&rows);
        assert_eq!(
            get_intent(&events),
            Some("Running tests".to_string()),
            "intent must be detected via VtLogBuffer-rendered rows; got: {:?}", events
        );
    }

    // --- Suggest detection tests ---

    fn get_suggest(events: &[ParsedEvent]) -> Option<Vec<String>> {
        events.iter().find_map(|e| match e {
            ParsedEvent::Suggest { items } => Some(items.clone()),
            _ => None,
        })
    }

    #[test]
    fn test_suggest_basic() {
        let parser = OutputParser::new();
        let events = parser.parse("[[suggest: Fix the test | Refactor code | Add docs]]");
        let items = get_suggest(&events).expect("should parse suggest");
        assert_eq!(items, vec!["Fix the test", "Refactor code", "Add docs"]);
    }

    #[test]
    fn test_suggest_single_brackets() {
        let parser = OutputParser::new();
        let events = parser.parse("[suggest: Option A | Option B]");
        let items = get_suggest(&events).expect("should parse suggest");
        assert_eq!(items, vec!["Option A", "Option B"]);
    }

    #[test]
    fn test_suggest_unicode_brackets() {
        let parser = OutputParser::new();
        let events = parser.parse("\u{27E6}suggest: Alpha | Beta | Gamma\u{27E7}");
        let items = get_suggest(&events).expect("should parse suggest");
        assert_eq!(items, vec!["Alpha", "Beta", "Gamma"]);
    }

    #[test]
    fn test_suggest_trims_whitespace() {
        let parser = OutputParser::new();
        let events = parser.parse("[[suggest:   Fix test  |  Refactor  |  Add docs  ]]");
        let items = get_suggest(&events).expect("should parse suggest");
        assert_eq!(items, vec!["Fix test", "Refactor", "Add docs"]);
    }

    #[test]
    fn test_suggest_filters_empty_items() {
        let parser = OutputParser::new();
        // Double pipe or trailing pipe should not produce empty items
        let events = parser.parse("[[suggest: Fix test || Add docs |]]");
        let items = get_suggest(&events).expect("should parse suggest");
        assert_eq!(items, vec!["Fix test", "Add docs"]);
    }

    #[test]
    fn test_suggest_needs_at_least_one_item() {
        let parser = OutputParser::new();
        assert!(get_suggest(&parser.parse("[[suggest: ]]")).is_none());
        assert!(get_suggest(&parser.parse("[[suggest: |  | ]]")).is_none());
    }

    #[test]
    fn test_no_suggest_normal_text() {
        let parser = OutputParser::new();
        assert!(get_suggest(&parser.parse("I suggest we refactor")).is_none());
        assert!(get_suggest(&parser.parse("Building project...")).is_none());
    }

    #[test]
    fn test_suggest_no_ansi_garbage() {
        let parser = OutputParser::new();
        assert!(get_suggest(&parser.parse("]garbage[[suggest: A | B]]")).is_none());
    }

    // --- conceal_suggest tests ---

    #[test]
    fn test_conceal_suggest_basic() {
        let raw = "[[suggest: Fix the test | Refactor code | Add docs]]";
        let out = conceal_suggest(raw);
        assert!(out.contains("\x1b[8m"), "should start conceal");
        assert!(out.contains("\x1b[28m"), "should end conceal");
        assert!(out.contains("[[suggest:"), "token content preserved for layout");
        assert!(out.contains("Fix the test"), "items preserved inside conceal");
    }

    #[test]
    fn test_conceal_suggest_preserves_surrounding_text() {
        let raw = "some text [[suggest: A | B]] more text";
        let out = conceal_suggest(raw);
        assert!(out.starts_with("some text "), "prefix preserved");
        assert!(out.contains("more text"), "suffix preserved");
        assert!(out.contains("\x1b[8m"), "conceal applied to token");
    }

    #[test]
    fn test_conceal_suggest_no_change_when_absent() {
        let raw = "normal output without suggest token";
        let out = conceal_suggest(raw);
        assert_eq!(out, raw, "unchanged when no suggest token");
    }

    #[test]
    fn test_conceal_suggest_single_brackets() {
        let raw = "[suggest: Option A | Option B]";
        let out = conceal_suggest(raw);
        assert!(out.contains("\x1b[8m"), "single brackets also concealed");
    }

    #[test]
    fn test_conceal_suggest_preserves_cursor_movements() {
        // Cursor movements around and inside the token must survive unchanged.
        let raw = "\x1b[8A text \x1b[1C[[suggest: A | B]]\x1b[1B more";
        let out = conceal_suggest(raw);
        assert!(out.contains("\x1b[8A"), "CUU must survive");
        assert!(out.contains("\x1b[1C"), "CUF must survive");
        assert!(out.contains("\x1b[1B"), "CUD must survive");
        assert!(out.contains("\x1b[8m"), "conceal applied to token");
        assert!(out.contains("more"), "text after token preserved");
    }

    #[test]
    fn test_colorize_intent_preserves_cursor_movements() {
        // Cursor movements (CUU, CUD, CUF) must survive colorize_intent unchanged.
        // The old line-replace approach lost these; replace_all only touches the token.
        let raw = "\x1b[8A\x1b[38;2;153;153;153m\u{25cf}\x1b[1C\x1b[39m \x1b[38;2;177;185;249m[[intent: Fixing bug(Fix)]]\x1b[39m\x1b[1Bmore";
        let colored = colorize_intent(raw);
        assert!(colored.contains("\x1b[2;33m"), "should colorize intent");
        assert!(colored.contains("\x1b[8A"), "CUU must survive");
        assert!(colored.contains("\x1b[1C"), "CUF must survive");
        assert!(colored.contains("\x1b[1B"), "CUD must survive");
        assert!(colored.contains("more"), "text after cursor movement preserved");
        assert!(!colored.contains("(Fix)"), "title stripped");
        assert!(!colored.contains("[["), "brackets stripped");
    }

    #[test]
    fn test_colorize_intent_cuf_between_words() {
        // Claude Code emits \x1b[1C (CUF) instead of spaces between words.
        // The body is kept raw (CUF intact) — xterm.js renders them as cursor movement.
        let raw = "[[intent: Project\x1b[1Conboarding\x1b[1Cand\x1b[1Cunderstanding(Prime session)]]";
        let colored = colorize_intent(raw);
        // Body should contain the raw CUF codes (xterm.js renders them correctly)
        assert!(colored.contains("Project\x1b[1Conboarding"),
            "raw CUF must be preserved in body; got: {:?}", colored);
        assert!(!colored.contains("(Prime session)"), "title stripped");
        assert!(!colored.contains("[["), "brackets stripped");
    }

    // --- parse_clean_lines tests ---

    fn row(i: usize, text: &str) -> crate::state::ChangedRow {
        crate::state::ChangedRow { row_index: i, text: text.to_string() }
    }

    #[test]
    fn test_parse_clean_lines_status_line() {
        let parser = OutputParser::new();
        let rows = vec![row(0, "* Reading files...")];
        let events = parser.parse_clean_lines(&rows);
        assert!(
            events.iter().any(|e| matches!(e, ParsedEvent::StatusLine { .. })),
            "expected StatusLine, got: {:?}", events
        );
    }

    #[test]
    fn test_parse_clean_lines_intent_with_title() {
        let parser = OutputParser::new();
        let rows = vec![row(0, "[[intent: Implementing feature(My title)]]")];
        let events = parser.parse_clean_lines(&rows);
        assert!(
            events.iter().any(|e| matches!(e, ParsedEvent::Intent { title: Some(_), .. })),
            "expected Intent with title, got: {:?}", events
        );
    }

    #[test]
    fn test_parse_clean_lines_suggest() {
        let parser = OutputParser::new();
        let rows = vec![row(0, "[[suggest: Run tests | Review diff | Deploy]]")];
        let events = parser.parse_clean_lines(&rows);
        assert!(
            events.iter().any(|e| matches!(e, ParsedEvent::Suggest { items } if items.len() == 3)),
            "expected Suggest with 3 items, got: {:?}", events
        );
    }

    #[test]
    fn test_parse_clean_lines_question() {
        let parser = OutputParser::new();
        let rows = vec![row(0, "Would you like to proceed?")];
        let events = parser.parse_clean_lines(&rows);
        assert!(
            events.iter().any(|e| matches!(e, ParsedEvent::Question { .. })),
            "expected Question, got: {:?}", events
        );
    }

    #[test]
    fn test_parse_clean_lines_usage_limit() {
        let parser = OutputParser::new();
        let rows = vec![row(0, "You've used 78% of your weekly limit")];
        let events = parser.parse_clean_lines(&rows);
        assert!(
            events.iter().any(|e| matches!(e, ParsedEvent::UsageLimit { .. })),
            "expected UsageLimit, got: {:?}", events
        );
    }

    #[test]
    fn test_parse_clean_lines_intent_with_bullet_prefix() {
        // Claude Code emits: ⏺ [[intent: text(title)]]
        // After vt100 rendering, CUF codes become spaces.
        let parser = OutputParser::new();
        let rows = vec![row(0, "\u{25CF} [[intent: Implementing feature(My title)]]")];
        let events = parser.parse_clean_lines(&rows);
        assert!(
            events.iter().any(|e| matches!(e, ParsedEvent::Intent { title: Some(t), .. } if t == "My title")),
            "expected Intent with title='My title', got: {:?}", events
        );
    }

    #[test]
    fn test_parse_clean_lines_intent_with_extra_spaces() {
        // CUF codes rendered as multiple spaces between words
        let parser = OutputParser::new();
        let rows = vec![row(0, "\u{25CF}  [[intent: Project  onboarding  and  understanding(Prime session)]]")];
        let events = parser.parse_clean_lines(&rows);
        assert!(
            events.iter().any(|e| matches!(e, ParsedEvent::Intent { title: Some(t), .. } if t == "Prime session")),
            "expected Intent with title='Prime session', got: {:?}", events
        );
    }

    #[test]
    fn test_parse_intent_normalizes_whitespace() {
        // VtLogBuffer renders CUF cursor-forward codes as multiple spaces.
        // parse_intent must collapse them so agentIntent text is clean.
        let parser = OutputParser::new();
        let rows = vec![row(0, "[[intent: Project   onboarding   and   understanding]]")];
        let events = parser.parse_clean_lines(&rows);
        let text = events.iter().find_map(|e| match e {
            ParsedEvent::Intent { text, .. } => Some(text.clone()),
            _ => None,
        });
        assert_eq!(
            text.as_deref(),
            Some("Project onboarding and understanding"),
            "extra spaces should be collapsed; got: {:?}", text
        );
    }

    #[test]
    fn test_parse_intent_wraps_across_rows() {
        // When [[intent: very long text(title)]] exceeds 80 cols it splits across two
        // VtLogBuffer rows joined with \n. INTENT_RE must cross the newline to extract both
        // text and title.
        let parser = OutputParser::new();
        // Simulate two rows: first row ends mid-token, second row has the closing bracket.
        let rows = vec![
            row(0, "⏺ [[intent: Implementing a very long feature description that wraps"),
            row(1, "across terminal rows(Long Feature)]]"),
        ];
        let events = parser.parse_clean_lines(&rows);
        let intent = events.iter().find_map(|e| match e {
            ParsedEvent::Intent { text, title } => Some((text.clone(), title.clone())),
            _ => None,
        });
        assert!(intent.is_some(), "intent must be detected even when token wraps; got: {:?}", events);
        let (text, title) = intent.unwrap();
        assert!(text.contains("Implementing"), "text should contain intent body; got: {:?}", text);
        assert_eq!(title.as_deref(), Some("Long Feature"), "title must be extracted; got: {:?}", title);
    }

    #[test]
    fn test_parse_clean_lines_rate_limit() {
        let parser = OutputParser::new();
        let rows = vec![row(0, "Error: rate_limit_error - please try again")];
        let events = parser.parse_clean_lines(&rows);
        assert!(
            events.iter().any(|e| matches!(e, ParsedEvent::RateLimit { .. })),
            "expected RateLimit, got: {:?}", events
        );
    }

    #[test]
    fn test_parse_clean_lines_plan_file() {
        let parser = OutputParser::new();
        let rows = vec![row(0, "Reading plans/vt100-clean-parsing.md")];
        let events = parser.parse_clean_lines(&rows);
        assert!(
            events.iter().any(|e| matches!(e, ParsedEvent::PlanFile { .. })),
            "expected PlanFile, got: {:?}", events
        );
    }

    #[test]
    fn test_parse_clean_lines_pr_url() {
        let parser = OutputParser::new();
        let rows = vec![row(0, "Pull request: https://github.com/owner/repo/pull/42")];
        let events = parser.parse_clean_lines(&rows);
        assert!(
            events.iter().any(|e| matches!(e, ParsedEvent::PrUrl { .. })),
            "expected PrUrl, got: {:?}", events
        );
    }

    #[test]
    fn test_parse_clean_lines_multiple_events() {
        let parser = OutputParser::new();
        let rows = vec![
            row(0, "[[intent: Working on feature(Test)]]"),
            row(1, "* Reading files..."),
        ];
        let events = parser.parse_clean_lines(&rows);
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

}
