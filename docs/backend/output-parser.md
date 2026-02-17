# Output Parser

**Module:** `src-tauri/src/output_parser.rs`

Parses terminal output to detect structured events: rate limits, status lines, PR URLs, and progress indicators.

## Usage

```rust
let parser = OutputParser::new();
let events: Vec<ParsedEvent> = parser.parse(terminal_output);
```

## ParsedEvent Variants

### RateLimit

Detected when terminal output matches known rate limit patterns from AI agents:

```rust
ParsedEvent::RateLimit {
    pattern_name: String,    // e.g., "claude_rate_limit"
    matched_text: String,    // The matched text
    retry_after_ms: Option<u64>, // Parsed retry delay
}
```

### StatusLine

Agent status output (e.g., token usage, timing):

```rust
ParsedEvent::StatusLine {
    task_name: String,
    full_line: String,
    time_info: Option<String>,
    token_info: Option<String>,
}
```

### PrUrl

Pull request URL detected in output:

```rust
ParsedEvent::PrUrl {
    number: u32,     // PR number
    url: String,     // Full URL
    platform: String, // "github", "gitlab", etc.
}
```

### Progress

OSC 9;4 progress indicator:

```rust
ParsedEvent::Progress {
    state: u8,  // 0=remove, 1=set, 2=error, 3=indeterminate, 4=warning
    value: u8,  // 0-100 progress percentage
}
```

## Pattern Detection

The parser uses regex patterns to detect:
- Rate limit messages from Claude, Aider, OpenCode, Gemini, Codex
- GitHub/GitLab PR URLs in `gh pr create` output
- OSC 9;4 terminal progress sequences
- Agent status lines with timing/token info

Patterns are compiled once at `OutputParser::new()` and reused across calls.
