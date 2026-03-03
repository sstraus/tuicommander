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

        // Strip ANSI once for all parsers that need clean text
        let clean = strip_ansi(text);

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
                    || line_is_diff_or_code_context(match_line, match_line.trim())
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
                    || line_is_diff_or_code_context(match_line, match_line.trim())
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
fn parse_osc94(text: &str) -> Option<ParsedEvent> {
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

/// Strip ANSI escape sequences from text using the strip-ansi-escapes crate.
/// Handles all ANSI escape types: CSI, OSC, SGR, and simple escapes.
///
/// Pre-processes cursor movement sequences before stripping, because
/// `strip-ansi-escapes` silently drops them and concatenates surrounding text:
/// - CUF (`\x1b[nC`, cursor forward) → n spaces
/// - CUU (`\x1b[nA`, cursor up) → newline
/// - CUD (`\x1b[nB`, cursor down) → newline
/// - CUB (`\x1b[nD`, cursor back) → stripped (no replacement)
///
/// This preserves line structure for parsers that rely on line-based matching
/// (intent tokens, status lines, etc.) even when the source uses full-screen
/// rendering with cursor positioning (e.g. Claude Code's Ink-based TUI).
pub(crate) fn strip_ansi(text: &str) -> String {
    lazy_static::lazy_static! {
        static ref CURSOR_RE: regex::Regex =
            regex::Regex::new(r"\x1b\[(\d*)([ABCD])").unwrap();
    }
    // Replace cursor movement sequences BEFORE carriage-return processing:
    // CUU/CUD become newlines, so a preceding \r becomes \r\n (normal line ending)
    // rather than triggering overwrite-from-start semantics that eats content.
    let cursor_replaced = CURSOR_RE.replace_all(text, |caps: &regex::Captures| {
        let n: usize = caps[1].parse().unwrap_or(1).max(1);
        match &caps[2] {
            "C" => " ".repeat(n),        // CUF → spaces
            "A" | "B" => "\n".to_string(), // CUU/CUD → newline
            "D" => String::new(),         // CUB → strip
            _ => String::new(),
        }
    });
    // Apply carriage return semantics AFTER cursor replacement, because
    // strip_ansi_escapes::strip() silently removes \r without applying its
    // overwrite-from-start-of-line semantics. This is critical for animated
    // status lines (Claude Code, Codex, etc.) that use \r to update in place.
    let cr_applied = apply_carriage_returns(&cursor_replaced);
    let stripped = strip_ansi_escapes::strip(cr_applied.as_bytes());
    String::from_utf8(stripped).unwrap_or_else(|_| text.to_string())
}

/// Simulate carriage return semantics: for each line, text after `\r`
/// overwrites from the beginning of the line. `\r\n` is treated as a
/// normal line ending (not an overwrite).
fn apply_carriage_returns(text: &str) -> String {
    // Normalize \r\n to \n first so they don't trigger overwrite logic
    let normalized = text.replace("\r\n", "\n");
    let mut result = String::with_capacity(normalized.len());
    for (i, line) in normalized.split('\n').enumerate() {
        if i > 0 {
            result.push('\n');
        }
        if let Some(last_cr) = line.rfind('\r') {
            // Take the segment after the last \r — it overwrites everything before
            result.push_str(&line[last_cr + 1..]);
        } else {
            result.push_str(line);
        }
    }
    result
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
        if line_is_diff_or_code_context(line, trimmed) {
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
        if line_is_diff_or_code_context(line, trimmed) {
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

/// Extract the last non-empty line ending with `?` from a chunk of text.
/// Used by the silence-based question detector: if an agent prints a line ending
/// with `?` and then goes silent, it's likely waiting for input.
/// Returns None if no line ends with `?` or if the line looks like code/markdown
/// rather than a genuine interactive prompt.
pub(crate) fn extract_last_question_line(text: &str) -> Option<String> {
    let clean = strip_ansi(text);
    // Only check the last non-empty line — a question buried mid-output
    // with more content after it is not an unanswered prompt.
    let last_clean = clean.lines().rev().find(|l| !l.trim().is_empty())?;
    let trimmed = last_clean.trim();
    if !trimmed.ends_with('?') {
        return None;
    }
    // Find the corresponding raw line for structural checks (indentation, tabs).
    // strip_ansi removes control chars like \t, so we need the original text.
    let last_raw = text.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or(last_clean);
    // Reject lines that look like source code, comments, or markdown prose
    // rather than genuine interactive prompts.
    if line_is_likely_not_a_prompt(last_raw, trimmed) {
        return None;
    }
    Some(trimmed.to_string())
}

/// Returns true if a line looks like diff output, code context, or documentation
/// rather than a genuine interactive prompt. Applied to ALL question regex matches
/// to prevent false positives from diff hunks containing question-like patterns.
fn line_is_diff_or_code_context(raw_line: &str, clean_trimmed: &str) -> bool {
    // Line-number prefix from code listings: "462 -...", "75 +-...", "465 //...", "1226    assert!(..."
    // Distinguished from HTTP status codes ("429 Too Many Requests") by requiring either:
    //   - diff markers (+, -, //) after the number, OR
    //   - 2+ spaces after the number (code listing indentation)
    if clean_trimmed.len() > 3 && clean_trimmed.as_bytes()[0].is_ascii_digit()
        && let Some(pos) = clean_trimmed.find(|c: char| !c.is_ascii_digit()) {
            let after_digits = &clean_trimmed[pos..];
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
    if raw_line.len() > 1 {
        let first = raw_line.as_bytes()[0];
        let second = raw_line.as_bytes()[1];
        if (first == b'+' || first == b'-')
            && (second == b' ' || second == b'\t' || second == first)
        {
            return true;
        }
    }

    // Claude Code diff summary blocks: "⏺⎿ Added16lines..."
    if clean_trimmed.contains("⏺⎿") {
        return true;
    }
    // Diff summary: "Added16lines" or "removed2lines" (no space between number and "lines")
    // Pattern: keyword followed immediately by digits then "lines" — unique to diff summaries
    if (clean_trimmed.contains("Added") || clean_trimmed.contains("removed"))
        && clean_trimmed.contains("lines")
        && clean_trimmed.chars().any(|c| c.is_ascii_digit())
    {
        // Extra check: the digit must be adjacent to "lines" (no space)
        if let Some(pos) = clean_trimmed.find("lines")
            && pos > 0 && clean_trimmed.as_bytes()[pos - 1].is_ascii_digit() {
                return true;
            }
    }

    // Lines containing "//" as code comments (but not URLs like http://)
    if clean_trimmed.starts_with("//")
        || clean_trimmed.contains(" //")
    {
        return true;
    }

    // Lines with markdown bold/italic containing question-pattern keywords
    // (e.g., "**Hardcoded prompts**: ...")
    if clean_trimmed.contains("**") && (
        clean_trimmed.contains("prompts")
        || clean_trimmed.contains("patterns")
        || clean_trimmed.contains("detection")
    ) {
        return true;
    }

    false
}

/// Returns true if a `?`-ending line is likely agent prose, code, or documentation
/// rather than an interactive prompt waiting for user input.
/// `raw_line`: the original PTY line (may contain tabs, ANSI), used for indentation checks.
/// `clean_trimmed`: the ANSI-stripped, trimmed content, used for content-based checks.
fn line_is_likely_not_a_prompt(raw_line: &str, clean_trimmed: &str) -> bool {
    // Indented code (4+ spaces or tab prefix = code block) — check raw line for structural indent
    if raw_line.starts_with("    ") || raw_line.starts_with('\t') {
        return true;
    }
    // Code comments (check trimmed content since indent is already handled above)
    if clean_trimmed.starts_with("//")
        || clean_trimmed.starts_with('#')
        || clean_trimmed.starts_with("/*")
        || clean_trimmed.starts_with('*')
    {
        return true;
    }
    // Markdown list items or blockquotes
    if clean_trimmed.starts_with("- ") || clean_trimmed.starts_with("> ") {
        return true;
    }
    // Lines containing backtick-wrapped code fragments
    if clean_trimmed.contains('`') {
        return true;
    }
    // Long lines (>120 chars) are almost always prose, not prompts
    if clean_trimmed.len() > 120 {
        return true;
    }
    // Lines with markdown bold/italic markers
    if clean_trimmed.contains("**") || clean_trimmed.contains("__") {
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

/// Strip brackets from `[intent: ...]` tokens, keeping the text colorized yellow.
/// `[[intent: Refactoring auth]]` becomes `\e[2;33mintent: Refactoring auth\e[0m`.
/// Strips the optional `(title)` suffix before colorizing.
/// Only invoke when `parse()` returned an Intent event to avoid regex on every chunk.
///
/// Uses inline `replace_all` so only the matched tag is replaced — all surrounding
/// content (bullet prefixes, ANSI formatting, line endings) is preserved intact.
pub fn colorize_intent(raw: &str) -> String {
    lazy_static::lazy_static! {
        static ref RAW_INTENT_RE: regex::Regex =
            regex::Regex::new(r"(?:\[\[?|\x{27E6})(intent:\s*)(.+?)\s*(?:\]?\]|\x{27E7})").unwrap();
        static ref STRIP_TITLE_RE: regex::Regex =
            regex::Regex::new(r"\([^)]+\)\s*$").unwrap();
    }
    RAW_INTENT_RE.replace_all(raw, |caps: &regex::Captures| {
        let prefix = &caps[1]; // "intent: "
        let body = &caps[2];
        let clean_body = STRIP_TITLE_RE.replace(body, "");
        let trimmed = clean_body.trim_end();
        format!("\x1b[2;33m{}{}\x1b[0m", prefix, trimmed)
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
        static ref INTENT_RE: regex::Regex =
            regex::Regex::new(r"(?:^|\s)(?:\[\[?|\x{27E6})intent:\s*(.+?)\s*(?:\]?\]|\x{27E7})").unwrap();
        // Separate regex to split out the optional (title) suffix from the captured text
        static ref TITLE_RE: regex::Regex =
            regex::Regex::new(r"^(.*?)\(([^)]+)\)\s*$").unwrap();
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

/// Strip suggest tokens from raw PTY output so they don't appear in the terminal.
/// Removes the entire line containing the token (including surrounding newlines).
pub fn strip_suggest(raw: &str) -> String {
    lazy_static::lazy_static! {
        static ref STRIP_RE: regex::Regex =
            regex::Regex::new(r"(?m)^[^\S\n]*(?:\[\[?|\x{27E6})suggest:\s*.+?\s*(?:\]?\]|\x{27E7})[^\S\n]*\n?").unwrap();
    }
    STRIP_RE.replace_all(raw, "").into_owned()
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
    fn test_strip_ansi() {
        assert_eq!(strip_ansi("\x1b[32mhello\x1b[0m"), "hello");
        assert_eq!(strip_ansi("no escapes"), "no escapes");
    }

    #[test]
    fn test_strip_ansi_cuf_replaced_with_spaces() {
        // CUF (Cursor Forward) \x1b[nC should become n spaces, not be silently dropped.
        // Without this fix, "hello" and "world" would be concatenated as "helloworld".
        assert_eq!(strip_ansi("hello\x1b[3Cworld"), "hello   world");
        // CUF with n=1 (implicit and explicit)
        assert_eq!(strip_ansi("a\x1b[Cb"), "a b");
        assert_eq!(strip_ansi("a\x1b[1Cb"), "a b");
        // Multiple CUF sequences
        assert_eq!(strip_ansi("x\x1b[2Cy\x1b[4Cz"), "x  y    z");
    }

    #[test]
    fn test_strip_ansi_carriage_return_overwrites_line() {
        // Claude Code animated status line: \r overwrites the current line
        assert_eq!(strip_ansi("old text\rnew text"), "new text");
        // Multiple \r — only last segment survives
        assert_eq!(strip_ansi("first\rsecond\rthird"), "third");
        // \r followed by shorter text — overwrites only the beginning
        assert_eq!(strip_ansi("long content here\rhi"), "hi");
        // Mixed: newlines + carriage returns
        assert_eq!(strip_ansi("line1\nold\rnew\nline3"), "line1\nnew\nline3");
        // No \r — unchanged
        assert_eq!(strip_ansi("hello\nworld"), "hello\nworld");
        // \r\n is a normal line ending — should not collapse
        assert_eq!(strip_ansi("line1\r\nline2"), "line1\nline2");
    }

    #[test]
    fn test_strip_ansi_claude_status_line_realistic() {
        // Realistic Claude Code output: colored status line overwritten by \r
        let input = "\x1b[33m* Reading files...\x1b[0m\r\x1b[33m* Writing code...\x1b[0m\r\x1b[33m* Done\x1b[0m\n";
        assert_eq!(strip_ansi(input), "* Done\n");
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

    // --- extract_last_question_line tests ---

    #[test]
    fn test_extract_question_line_simple() {
        assert_eq!(
            extract_last_question_line("Do you want to continue?"),
            Some("Do you want to continue?".to_string())
        );
    }

    #[test]
    fn test_extract_question_line_last_line_is_question() {
        let text = "First line\nSecond line\nThird question?";
        assert_eq!(
            extract_last_question_line(text),
            Some("Third question?".to_string())
        );
    }

    #[test]
    fn test_extract_question_line_question_mid_output_ignored() {
        // Question buried mid-output with non-question content after — not a prompt
        let text = "Do you want to continue?\n- [ ] Task 1\n- [ ] Task 2";
        assert_eq!(extract_last_question_line(text), None);
    }

    #[test]
    fn test_extract_question_line_no_question() {
        assert_eq!(extract_last_question_line("Normal output here"), None);
    }

    #[test]
    fn test_extract_question_line_ansi_wrapped() {
        assert_eq!(
            extract_last_question_line("\x1b[33mContinue?\x1b[0m"),
            Some("Continue?".to_string())
        );
    }

    #[test]
    fn test_extract_question_line_url_with_query_not_matched() {
        // A URL with ? in the middle of a line, but the line itself doesn't end with ?
        assert_eq!(
            extract_last_question_line("Fetching https://example.com/api?foo=bar done"),
            None,
        );
    }

    #[test]
    fn test_extract_question_line_empty_lines_skipped() {
        assert_eq!(
            extract_last_question_line("Ready?\n\n\n"),
            Some("Ready?".to_string())
        );
    }

    // --- Silence detector false-positive guard tests ---

    #[test]
    fn test_extract_question_rejects_code_comments() {
        assert_eq!(extract_last_question_line("// should we handle this case?"), None);
        assert_eq!(extract_last_question_line("# what about edge cases?"), None);
        assert_eq!(extract_last_question_line("/* is this correct?"), None);
        assert_eq!(extract_last_question_line("* should we retry?"), None);
    }

    #[test]
    fn test_extract_question_rejects_indented_code() {
        assert_eq!(extract_last_question_line("    if condition.is_valid?"), None);
        assert_eq!(extract_last_question_line("\tis_ready?"), None);
    }

    #[test]
    fn test_extract_question_rejects_markdown_prose() {
        assert_eq!(extract_last_question_line("- What should we do about this?"), None);
        assert_eq!(extract_last_question_line("> Do you agree with this approach?"), None);
        assert_eq!(extract_last_question_line("Have you tried using `foo.bar()?` instead?"), None);
        assert_eq!(extract_last_question_line("**Should we refactor this?**"), None);
    }

    #[test]
    fn test_extract_question_rejects_long_prose() {
        let long_line = format!("{}?", "a".repeat(121));
        assert_eq!(extract_last_question_line(&long_line), None);
    }

    #[test]
    fn test_extract_question_accepts_real_prompts() {
        // Short, plain prompts that ARE genuine interactive questions
        assert!(extract_last_question_line("Continue?").is_some());
        assert!(extract_last_question_line("Do you want to continue?").is_some());
        assert!(extract_last_question_line("Apply changes?").is_some());
        assert!(extract_last_question_line("Proceed with deletion?").is_some());
        assert!(extract_last_question_line("Enter filename?").is_some());
    }

    #[test]
    fn test_line_is_likely_not_a_prompt_fn() {
        // Helper: for non-indented lines, raw and trimmed are the same
        let check = |line: &str| line_is_likely_not_a_prompt(line, line.trim());

        // Should be filtered (not prompts)
        assert!(check("// is this a question?"));
        assert!(check("# what about this?"));
        assert!(line_is_likely_not_a_prompt("    indented code question?", "indented code question?"));
        assert!(line_is_likely_not_a_prompt("\tindented code question?", "indented code question?"));
        assert!(check("- list item question?"));
        assert!(check("> blockquote question?"));
        assert!(check("uses `backticks` question?"));
        assert!(check("**bold** question?"));

        // Should pass (real prompts)
        assert!(!check("Continue?"));
        assert!(!check("Apply changes?"));
        assert!(!check("Do you want to proceed?"));
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
    fn test_colorize_intent_ansi_inside_brackets_passthrough() {
        // When ANSI codes break the bracket tokens apart, the regex can't match
        // on raw text. The tag passes through uncolorized — acceptable because
        // the parser still detects the event via strip_ansi.
        let raw = "\x1b[2m[[\x1b[0mintent: Implementing config(Config field)\x1b[2m]]\x1b[0m";
        let colored = colorize_intent(raw);
        assert_eq!(colored, raw, "should pass through unchanged when ANSI breaks bracket tokens");
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
    fn test_colorize_intent_preserves_surrounding_content() {
        // colorize_intent must only replace the tag, not the entire line.
        // The bullet prefix and any ANSI formatting around it must survive.
        let raw = "\x1b[1m\u{25CF}\x1b[0m \x1b[2m[[intent: Implementing config(Config field)]]\x1b[0m";
        let colored = colorize_intent(raw);
        // Bullet must be preserved
        assert!(colored.contains("\u{25CF}"), "bullet prefix must survive; got: {:?}", colored);
        // Intent text must be colorized
        assert!(colored.contains("\x1b[2;33mintent: Implementing config\x1b[0m"),
            "intent must be colorized yellow; got: {:?}", colored);
        // Title must be stripped
        assert!(!colored.contains("(Config field)"), "title must be stripped");
        // Brackets must be stripped
        assert!(!colored.contains("[["), "opening brackets must be stripped");
    }

    #[test]
    fn test_colorize_intent_preserves_multiline_context() {
        // Lines before and after the intent must be completely untouched
        let raw = "line one\n\u{25CF} [[intent: Doing stuff(Stuff)]]\nline three";
        let colored = colorize_intent(raw);
        assert!(colored.starts_with("line one\n"), "first line untouched; got: {:?}", colored);
        assert!(colored.ends_with("\nline three"), "third line untouched; got: {:?}", colored);
        assert!(colored.contains("\u{25CF}"), "bullet preserved; got: {:?}", colored);
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
        // strip_ansi must convert CUU/CUD to newlines to preserve line structure.
        let parser = OutputParser::new();
        let raw = "\x1b[38;2;215;119;87m\u{273b}\x1b[39m\r\r\n\r\n\r\n\r\n\x1b[?2026l\x1b[?2026h\r\x1b[8A\x1b[38;2;153;153;153m\u{25cf}\x1b[1C\x1b[39m\x1b[1mBash\x1b[22m\r\x1b[1B  \x1b[38;2;177;185;249m[[intent: Fixing strip_ansi cursor handling(Fix strip_ansi)]]\x1b[39m\r\x1b[1Bmore output";
        let events = parser.parse(raw);
        assert_eq!(
            get_intent(&events),
            Some("Fixing strip_ansi cursor handling".to_string()),
            "intent must be detected even with CUU/CUD cursor movements in surrounding output"
        );
        assert_eq!(
            get_intent_title(&events),
            Some("Fix strip_ansi".to_string()),
        );
    }

    #[test]
    fn test_intent_with_cursor_down_only() {
        // Intent token preceded by CUD (cursor down) — \x1b[nB
        let parser = OutputParser::new();
        let raw = "spinner output\x1b[3B[[intent: Running tests(Tests)]]\x1b[2Amore";
        let events = parser.parse(raw);
        assert_eq!(
            get_intent(&events),
            Some("Running tests".to_string()),
            "CUD before intent should become newline, making intent matchable"
        );
    }

    #[test]
    fn test_strip_ansi_converts_cursor_movements_to_whitespace() {
        // CUU and CUD should become newlines, CUB should be stripped
        let result = strip_ansi("hello\x1b[3Aworld\x1b[2Bfoo\x1b[1Dbar");
        assert!(result.contains("hello"), "text before CUU preserved");
        assert!(result.contains("world"), "text after CUU preserved");
        assert!(result.contains("foo"), "text between movements preserved");
        // CUU/CUD should insert newlines to separate text from different screen positions
        assert!(result.contains('\n'), "cursor up/down should produce newlines");
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

    #[test]
    fn test_strip_suggest_removes_token() {
        let raw = "Some output\n[[suggest: Fix test | Refactor | Add docs]]\nMore output";
        let stripped = strip_suggest(raw);
        // Should remove the entire suggest line, leaving no blank line
        assert!(!stripped.contains("suggest"));
        assert!(stripped.contains("Some output"));
        assert!(stripped.contains("More output"));
    }

    #[test]
    fn test_strip_suggest_no_match_unchanged() {
        let raw = "Normal terminal output";
        assert_eq!(strip_suggest(raw), raw);
    }
}
