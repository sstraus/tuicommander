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
}

/// OutputParser: detects structured events in PTY output text.
/// Designed to run in the Rust reader thread, eliminating JS regex overhead.
pub struct OutputParser {
    rate_limit_patterns: Vec<RateLimitPattern>,
}

struct RateLimitPattern {
    name: &'static str,
    regex: regex::Regex,
    retry_after_ms: Option<u64>,
    has_retry_capture: bool,
}

impl OutputParser {
    pub fn new() -> Self {
        Self {
            rate_limit_patterns: build_rate_limit_patterns(),
        }
    }

    /// Parse a chunk of PTY output and return any detected events.
    pub fn parse(&self, text: &str) -> Vec<ParsedEvent> {
        let mut events = Vec::new();

        // OSC 9;4 progress (cheap byte scan first)
        if let Some(evt) = parse_osc94(text) {
            events.push(evt);
        }

        // PR/MR URL detection
        if let Some(evt) = parse_pr_url(text) {
            events.push(evt);
        }

        // Status line detection (needs ANSI stripping)
        if let Some(evt) = parse_status_line(text) {
            events.push(evt);
        }

        // Rate limit detection
        if let Some(evt) = self.parse_rate_limit(text) {
            events.push(evt);
        }

        events
    }

    fn parse_rate_limit(&self, text: &str) -> Option<ParsedEvent> {
        for pattern in &self.rate_limit_patterns {
            if let Some(m) = pattern.regex.find(text) {
                let retry_after_ms = if pattern.has_retry_capture {
                    pattern.regex.captures(text).and_then(|caps| {
                        caps.get(1).and_then(|g| {
                            g.as_str().parse::<u64>().ok().map(|s| s * 1000)
                        })
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
}

fn build_rate_limit_patterns() -> Vec<RateLimitPattern> {
    vec![
        // Claude
        rl("claude-http-429", r"(?i)rate_limit_error|429.*Too Many Requests", Some(60000), false),
        rl("claude-overloaded", r"(?i)overloaded_error", Some(30000), false),
        rl("claude-retry-after", r"(?i)retry[- ]?after[:\s]*(\d+)", None, true),
        rl("claude-token-limit", r"(?i)rate limit.*token|tokens.*exceeded", Some(60000), false),
        // OpenAI/Codex
        rl("openai-http-429", r"(?i)RateLimitError|429.*rate limit", Some(60000), false),
        rl("openai-retry-after", r"(?i)Retry after (\d+) seconds?", None, true),
        rl("openai-tpm-limit", r"(?i)TPM|tokens per minute.*limit", Some(60000), false),
        rl("openai-rpm-limit", r"(?i)RPM|requests per minute.*limit", Some(60000), false),
        // Gemini
        rl("gemini-resource-exhausted", r"(?i)RESOURCE_EXHAUSTED|quota exceeded", Some(60000), false),
        // Generic
        rl("http-429", r"(?i)HTTP\s*429|429\s*Too Many Requests", Some(60000), false),
        rl("rate-limit-keyword", r"(?i)rate[- ]?limit(?:ed|ing)?", Some(60000), false),
        rl("too-many-requests", r"(?i)too many requests", Some(60000), false),
        rl("retry-after-header", r"(?i)Retry-After:\s*(\d+)", None, true),
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
fn strip_ansi(text: &str) -> String {
    let stripped = strip_ansi_escapes::strip(text);
    String::from_utf8(stripped).unwrap_or_else(|_| text.to_string())
}

/// Parse status line patterns from terminal output
fn parse_status_line(text: &str) -> Option<ParsedEvent> {
    let clean = strip_ansi(text);
    lazy_static::lazy_static! {
        // Claude Code: "* Task name... (time)"
        static ref CLAUDE_STATUS_RE: regex::Regex =
            regex::Regex::new(r"\*\s+([^.…]+)(?:\.{2,3}|…)").unwrap();
        // "[Running] Task name"
        static ref RUNNING_STATUS_RE: regex::Regex =
            regex::Regex::new(r"(?i)\[Running\]\s+(.+)").unwrap();
        // Spinner: "⠋ Task name"
        static ref SPINNER_STATUS_RE: regex::Regex =
            regex::Regex::new(r"[⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏]\s+([^.…]+)").unwrap();
        // Time info
        static ref TIME_RE: regex::Regex =
            regex::Regex::new(r"\((\d+[smh])").unwrap();
        // Token info
        static ref TOKEN_RE: regex::Regex =
            regex::Regex::new(r"(?i)(\d+(?:\.\d+)?k?\s*tokens)").unwrap();
    }

    for line in clean.lines() {
        // Try each pattern
        let patterns: &[&regex::Regex] = &[&CLAUDE_STATUS_RE, &RUNNING_STATUS_RE, &SPINNER_STATUS_RE];
        for pattern in patterns {
            if let Some(caps) = pattern.captures(line) {
                let task_name = caps[1].trim().to_string();
                if task_name.len() < 3 {
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
    fn test_strip_ansi() {
        assert_eq!(strip_ansi("\x1b[32mhello\x1b[0m"), "hello");
        assert_eq!(strip_ansi("no escapes"), "no escapes");
    }

    #[test]
    fn test_no_false_positives() {
        let parser = OutputParser::new();
        // Normal terminal output should produce no events
        let events = parser.parse("ls -la\ntotal 42\ndrwxr-xr-x  5 user staff 160 Jan 1 00:00 .\n");
        assert!(events.is_empty());
    }
}
