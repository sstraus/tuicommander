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

### Question

Agent is waiting for user input (question, confirmation, menu choice):

```rust
ParsedEvent::Question {
    prompt_text: String,  // The detected prompt line
}
```

Detected via multiple patterns:
- **Hardcoded prompts**: "Would you like to proceed?", "Do you want to...?", "Is this plan/approach okay"
- **Numbered menus**: Lines with `❯`, `›` (Ink), `>`, or `)` before `1.` followed by option text
- **Y/N prompts**: `[Y/n]`, `[y/N]`, `(yes/no)`
- **Inquirer-style**: Lines starting with `? ` (standard inquirer prefix)
- **Ink navigation footer**: "Enter to select" (Ink SelectInput menus)
- **Generic questions**: Any line ending with `?` that passes false-positive filters (rejects code comments, markdown, indented code, backtick fragments, bold markers, long prose >120 chars)

Additionally, `extract_last_question_line()` provides silence-based detection: if the last non-empty line ends with `?` and isn't code/prose, the session may be waiting for input.

### ApiError

API errors from agents and providers (5xx server errors, auth failures):

```rust
ParsedEvent::ApiError {
    pattern_name: String,    // e.g., "claude-api-error", "openai-server-error"
    matched_text: String,    // The matched text
    error_kind: String,      // "server", "auth", or "unknown"
}
```

Detects errors from two tiers:
- **Agent-specific**: Claude Code, Aider, Codex CLI, Gemini CLI, Copilot CLI
- **Provider-level**: OpenAI, Anthropic, Google, OpenRouter, MiniMax JSON error structures

Frontend plays an error notification sound and logs via `appLogger.error()`.

### Intent

Agent-declared intent — what the LLM is currently working on:

```rust
ParsedEvent::Intent {
    text: String,  // Short action description
}
```

Detected when the agent emits `[[intent: <text>]]` or `⟦intent: <text>⟧` on its own line. Agents receive this instruction automatically via MCP init. To use manually without MCP, add to CLAUDE.md or equivalent:

```
## Intent Declaration
At the start of each distinct work phase, emit on its own line:
[[intent: <action, present tense, <60 chars>]]
Examples: `Reading auth module for token flow` · `Writing parser unit tests` · `Debugging login redirect`
```

The activity dashboard shows intent (crosshair icon) when available, falling back to user prompt (speech bubble) otherwise.

## Pattern Detection

The parser uses regex patterns to detect:
- Rate limit messages from Claude, Aider, OpenCode, Gemini, Codex
- Questions and interactive prompts (hardcoded, Y/N, inquirer, Ink menus, generic `?` lines)
- API errors from agents and API providers (5xx, auth failures)
- GitHub/GitLab PR URLs in `gh pr create` output
- OSC 9;4 terminal progress sequences
- Agent status lines with timing/token info

Patterns are compiled once at `OutputParser::new()` and reused across calls.
