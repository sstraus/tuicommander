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

Detected exclusively via **screen-verified silence detection** (all instant regex patterns were removed due to false positives from Ink agent streaming):

1. `extract_question_line()` scans changed terminal rows for `?`-ending lines, applying content filters to reject code comments (`//`), markdown headers (`#`), diff context (`+/-`), and code syntax (`->`, `=>`, `::`, `)?`)
2. `SilenceState` stores the candidate and starts a 10s silence timer
3. When the timer fires, it verifies the candidate is still visible in the bottom 5 rows of the terminal screen (via `VtLogBuffer.screen_rows()`)
4. If verified, emits `ParsedEvent::Question { confident: false }`

Guards against false positives:
- **Spinner suppression**: If a status-line event was seen within the last 10s, detection is suppressed
- **Staleness counter**: If >10 non-`?` output chunks arrived after the candidate, it's considered stale
- **Screen verification**: Candidate must still be among the last 5 visible lines at fire time
- **User echo suppression**: 500ms window after user input ignores PTY echo of typed text
- **Resize grace**: 1s suppression after terminal resize to avoid re-detection of redrawn content

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
- **Agent-specific**: Claude Code, Aider, Codex CLI, Gemini CLI, Copilot CLI (note: the generic "request failed unexpectedly" pattern was removed from Copilot detection due to false positives on Claude Code output)
- **Provider-level**: OpenAI, Anthropic, Google, OpenRouter, MiniMax JSON error structures

Frontend plays an error notification sound and logs via `appLogger.error()`.

### Intent

Agent-declared intent — what the LLM is currently working on:

```rust
ParsedEvent::Intent {
    text: String,   // Short action description
    title: Option<String>,  // Optional tab title from (parenthesized) suffix
}
```

Detected when the agent emits `[[intent: <text>(<title>)]]` or `⟦intent: <text>⟧` on its own line. Agents receive this instruction automatically via MCP init. To use manually without MCP, add to CLAUDE.md or equivalent:

```
## Intent Declaration
At the start of each distinct work phase, emit on its own line:
[[intent: <action, present tense, <60 chars>]]
Examples: `Reading auth module for token flow` · `Writing parser unit tests` · `Debugging login redirect`
```

The activity dashboard shows intent (crosshair icon) when available, falling back to user prompt (speech bubble) otherwise.

**Colorization:** `colorize_intent()` wraps the intent text in `\x1b[2;33m` (dim yellow) for the xterm.js stream. Embedded ANSI codes (SGR colors, bold, resets) from the agent's Ink renderer are stripped from the body to prevent color interruption. CUF (cursor-forward) sequences used by Ink as inter-word spacing are converted to space characters.

**PWA/REST stripping:** `LogLine::strip_structural_tokens()` removes raw `[[intent:...]]` and `[[suggest:...]]` tokens from log line spans before serving to mobile/browser clients, where the raw xterm colorization pipeline does not apply.

**Active subtask detection:** The output parser recognizes `⏵⏵` (U+23F5) and `››` (U+203A) mode-line prefixes as active subtask indicators. The `active_sub_tasks` count is tracked in `SessionState` and used to suppress premature completion notifications.

### PlanFile

Plan file path detected in agent output:

```rust
ParsedEvent::PlanFile {
    path: String,  // Absolute path to the plan file
}
```

### Suggest

Agent-proposed follow-up actions:

```rust
ParsedEvent::Suggest {
    items: Vec<String>,  // e.g., ["Run tests", "Review diff", "Deploy"]
}
```

Detected via `[[suggest: A | B | C]]`, `[suggest: ...]`, or `⟦suggest: ...⟧` tokens. Items are pipe-delimited.

The `conceal_suggest()` function replaces the raw token in the terminal stream with SGR invisible sequences so it never appears on screen.

### UsageLimit

Claude Code usage limit percentage:

```rust
ParsedEvent::UsageLimit {
    percentage: u8,      // 0-100
    limit_type: String,  // "weekly" or "session"
}
```

Detected via regex matching `"You've used X% of your weekly/session limit"`. Supports both ASCII and Unicode smart-quote apostrophes (`'` and `\u{2019}`).

### UsageExhausted

Claude Code usage fully exhausted (no remaining quota):

```rust
ParsedEvent::UsageExhausted {
    reset_time: Option<String>,  // Raw text, e.g. "8pm (Europe/Madrid)"
}
```

Detected via `"out of (extra) usage"` pattern. The optional `reset_time` is extracted from `"· resets <text>"` suffix. The raw string is passed to plugins for scheduling; no timezone parsing is done in Rust.

### ActiveSubtasks

Agent sub-task indicator from `›› task · N local agents` mode-line:

```rust
ParsedEvent::ActiveSubtasks {
    count: u32,       // Number of active sub-tasks (0 = all finished)
    task_type: String, // "local agents", "bash", "background tasks", etc.
}
```

### ShellState

Shell activity state derived from PTY output timing:

```rust
ParsedEvent::ShellState {
    state: String, // "busy" | "idle"
}
```

Emitted by the reader thread on real-output→busy and idle transitions. The frontend consumes this instead of deriving busy/idle from raw PTY data. See `docs/backend/pty.md` for idle detection details.

### SlashMenu

Slash command menu detected from VT100 screen rows:

```rust
ParsedEvent::SlashMenu {
    items: Vec<SlashMenuItem>,  // { command, highlighted }
}
```

Detected by `parse_slash_menu()` when `slash_mode` is active — scans the bottom screen rows for 2+ consecutive `/command` patterns. The `❯` prefix marks the highlighted item.

## VT100-Aware Parsing

### `parse_clean_lines(rows: &[ChangedRow]) -> Vec<ParsedEvent>`

Primary entry point for VT100-aware parsing. Accepts `ChangedRow` vectors from `VtLogBuffer.process()` — each row contains clean text extracted from the VT100 screen emulator. This replaces the legacy ANSI-stripping pipeline for mobile/MCP consumers.

### `parse_slash_menu(screen_rows: &[String]) -> Option<ParsedEvent>`

Scans screen bottom rows (from VtLogBuffer) for slash command menus. Only called when `slash_mode` is active (user typed `/`). Returns `SlashMenu` event with all detected commands.

## Pattern Detection

The parser uses regex patterns to detect:
- Rate limit messages from Claude, Aider, OpenCode, Gemini, Codex
- Questions and interactive prompts (hardcoded, Y/N, inquirer, Ink menus, generic `?` lines)
- API errors from agents and API providers (5xx, auth failures)
- GitHub/GitLab PR URLs in `gh pr create` output
- OSC 9;4 terminal progress sequences
- Agent status lines with timing/token info (see below)

Patterns are compiled once at `OutputParser::new()` and reused across calls.

### False-Positive Guards

Two guard functions prevent false-positive detection when agents read or display source code, diffs, or documentation containing error-like or question-like patterns:

- **`line_is_source_code(line)`** — Returns `true` for lines that look like source code rather than real errors. Detects: Rust raw string literals (`r"..."`, `r#"..."#`), line comments (`//`, `#`), function/const/let declarations, indented code with string delimiters (4+ leading spaces), markdown fences (`` ``` ``), bullet points (`- `, `* `), and markdown tables (`| ... |`).
- **`line_is_diff_or_code_context(raw_line, trimmed)`** — Returns `true` for lines that look like diff output or code listings. Detects: unified diff lines (`+`, `-` prefixes), line-number prefixed code (`462 -...`), Claude Code diff summary blocks (`⏺⎿`), and diff summary lines (`Added16lines`).

Both guards are applied to rate limit, API error, and question pattern matches before emitting events.

### ANSI Pre-Processing

The `strip_ansi()` function pre-processes CUF (Cursor Forward, `\x1b[nC`) escape sequences by replacing them with the equivalent number of spaces before stripping all ANSI escapes. Without this, `strip-ansi-escapes` silently drops cursor movement sequences and would concatenate surrounding text (e.g., `"hello\x1b[3Cworld"` would become `"helloworld"` instead of `"hello   world"`).

### Status Line Detection by Agent

| Agent | Pattern | Example |
|-------|---------|---------|
| Claude Code | `·`/`✢`/`✳`/`✶`/`✻`/`✽`/`*` + ellipsis | `✢Reading files… (12s)` or `· Considering…` |
| Aider | Knight Rider scanner `░█` / `█░` + task text | `░█        Waiting for claude-3-5-sonnet` |
| Aider | Token report `Tokens:` prefix | `Tokens: 5.2k sent, 1.3k received.` |
| Codex CLI | Bullet `•`/`◦` + task + parenthesized time | `• Working (5s • esc to interrupt)` |
| Copilot CLI | `∴`/`●`/`○` + task + dots/ellipsis | `∴ Thinking…` or `● Read file...` |
| Gemini CLI | Braille spinner `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏` + phrase | `⠋ Analyzing your codebase` |
| Amazon Q | Braille spinner + task + ASCII dots | `⠹ Thinking...` |
| Cline | Braille spinner + mode + optional timer | `⠙ Planning (45s · esc to interrupt)` |
| Generic | `[Running]` prefix | `[Running] npm test` |
