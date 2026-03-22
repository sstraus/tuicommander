# Aider — UI Layout Reference

Agent-specific layout reference for Aider.
See [agent-ui-analysis.md](../agent-ui-analysis.md) for shared concepts.

**Observed version**: v0.86.2 (2026-03-22)
**Rendering engine**: Python readline + rich (no TUI framework)
**Rendering approach**: Sequential CLI output with ANSI colors

---

## Key Characteristics

Aider is the simplest of all supported agents — a sequential CLI tool
with no TUI framework, no screen management, no cursor positioning.
Output flows linearly top-to-bottom like a normal shell command.

- No Ink, no Bubble Tea — just Python with rich text formatting
- Prompt is simple `>` (green, ANSI 256 color 40)
- Spinner uses `░█`/`█░` Knight Rider pattern with backspace overwrite
- No mode line, no status bar, no panels
- File approval uses inline Y/N/A/S/D prompts

---

## Observed States

### Startup Banner

```
─────────────────────────────────────────────────────────────────────
Aider v0.86.2
Main model: openrouter/anthropic/claude-sonnet-4.5 with diff edit format, infinite output
Weak model: openrouter/anthropic/claude-haiku-4-5
Git repo: .git with 855 files
Repo-map: using 4096 tokens, auto refresh
─────────────────────────────────────────────────────────────────────
>
```

- Green separators `─────` (rgb 0,204,0)
- Model info, git repo stats, repo-map config
- Bare `>` prompt (green)

### Working State (spinner)

```
░█  Updating repo map: examples/plugins/repo-dashboard/main.js
█░  Waiting for openrouter/anthropic/claude-sonnet-4.5
```

- **Knight Rider scanner**: `░█` and `█░` alternate using backspace (`\b`) to overwrite
- `░` (U+2591, light shade) and `█` (U+2588, full block)
- Task description after the scanner chars
- Already detected by `parse_status_line` via `AIDER_SPINNER_RE`

### Agent Response

```
To find the version of this project, I need to check the version files.
 • package.json (for the frontend/Node.js part)
 • src-tauri/Cargo.toml (for the Rust/Tauri part)
Please add these files to the chat so I can tell you the version.
Tokens: 11k sent, 75 received. Cost: $0.03 message, $0.03 session.
```

- Blue text (rgb 0,136,255) for agent output
- Bold bullets `•` for lists
- File names with inverted background (rgb 0,0,0 on rgb 248,248,248)
- **Token report** after every response: `Tokens: Nk sent, N received. Cost: $X.XX`
- Already detected by `parse_status_line` via `AIDER_TOKENS_RE`

### File Approval Prompt

```
package.json
Add file to the chat? (Y)es/(N)o/(A)ll/(S)kip all/(D)on't ask again [Yes]:
```

- File name shown in reverse video (`\033[7m`)
- Inline prompt with 5 options: Y/N/A/S/D
- Green text (same as main prompt)
- Default answer in brackets: `[Yes]`
- This is a readline prompt — Enter submits, no special handling needed

### After Response (idle)

```
Tokens: 8.0k sent, 106 received. Cost: $0.03 message, $0.06 session.
─────────────────────────────────────────────────────────────────────
package.json src-tauri/Cargo.toml
>
```

- Green separator between conversation turns
- Active file list shown before prompt (files in chat context)
- Bare `>` prompt

### Error State

```
litellm.AuthenticationError: AuthenticationError: OpenrouterException - {"error":{"message":"User not found.","code":401}}
The API provider is not able to authenticate you. Check your API key.
```

- Orange warning text (rgb 255,165,0) for non-fatal warnings
- Red error text (rgb 255,34,34) for fatal errors

---

## Detection Signals

### Agent Identification
- `Aider v` in startup banner
- `░█` / `█░` Knight Rider spinner
- `Tokens:` + `Cost:` report after responses
- `Add file to the chat?` approval prompt

### Chrome Detection (`is_chrome_row`)
- `░█` / `█░` — not in current marker set but detected by `parse_status_line`
- Separator `─────` — detected by `is_separator_line` ✓
- Prompt `>` — detected by `is_prompt_line` ✓
- Token report lines — not chrome markers, but not agent output either

### Subtask / Subprocess Count
**None.** Aider does not have subprocess/subtask concepts.

### Permission / Approval
- No tool approval system — Aider auto-applies edits (or asks about file adds)
- File add approval: `Add file to the chat? (Y)es/(N)o/...`
- Edit confirmation: only with `--auto-commits` disabled, shows diff for review

---

## Rendering Mechanics (raw ANSI)

### Sequential output (no cursor positioning)
```
\r\n    — standard newlines, no cursor movement
\b      — backspace for spinner animation only
```

### Color scheme
```
\033[0;38;5;40m        — green (ANSI 256 #40) for prompt and UI elements
\033[38;2;0;136;255m   — blue (rgb 0,136,255) for agent response text
\033[38;2;0;204;0m     — green (rgb 0,204,0) for separators
\033[38;2;255;165;0m   — orange for warnings
\033[38;2;255;34;34m   — red for errors
\033[7m                — reverse video for file names
\033[1m                — bold for list bullets
```

### Spinner (Knight Rider)
```
░█\b\b       — write 2 chars, backspace 2
█░\b\b       — overwrite with swapped chars
  \b\b       — clear with spaces
```

Uses backspace (`\b`) to overwrite in place. No cursor positioning.

### Readline integration
```
\033[?2004h    — enable bracketed paste
\033[6n        — request cursor position (readline)
\033[?25l/h    — hide/show cursor during drawing
```

---

## Implications for TUICommander

### Parsing Strategy
Aider is the ideal case for `chrome.rs` changed-rows detection:
- Sequential output → each new line is a new changed row
- No full-screen redraws → delta analysis works perfectly
- Spinner overwrites in place → appears as single changed row

### Already Supported
- `AIDER_SPINNER_RE` in `parse_status_line` detects the Knight Rider scanner
- `AIDER_TOKENS_RE` detects token reports
- `is_separator_line` matches the green `─────` separators
- `is_prompt_line` matches the bare `>` prompt
- `░` (U+2591) and `█` (U+2588) detected by `is_chrome_row` — Knight Rider spinner classified as chrome
- `has_status_line` in `chrome_only` calculation — spinner-only chunks don't reset silence timer
- `find_chrome_cutoff` correctly trims Aider bottom zone (separator + file list + prompt)

### Not Yet Supported
- File approval prompt detection (`Add file to the chat?`)
- Token/cost extraction from the report line
- File context list (shown before prompt) — not classified as chrome
