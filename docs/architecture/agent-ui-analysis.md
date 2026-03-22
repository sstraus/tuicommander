# Agent UI Analysis — General Reference

Cross-agent reference for parsing AI agent terminal UIs in TUICommander.
Agent-specific layouts are documented in `agents/<name>.md`.

## Scope

TUICommander supports multiple AI coding agents. Each has a unique terminal
UI with different rendering approaches, chrome patterns, and interaction
models. This document covers:

1. Shared concepts and detection strategies
2. Code architecture and known gaps
3. Research methodology for ongoing verification

Agent-specific documents:
- [Claude Code](agents/claude-code.md) — Ink-based, ANSI cursor positioning
- [Codex CLI](agents/codex.md) — Ink-based, absolute positioning + scroll regions
- [Gemini CLI](agents/gemini-cli.md) — Ink-like, relative positioning + prompt box
- [Aider](agents/aider.md) — Sequential CLI, no TUI framework
- [OpenCode](agents/opencode.md) — Bubble Tea full-screen TUI

## Detection Strategy Per Agent

Each agent class requires a different parsing strategy:

| Agent | UI Type | Parsing Strategy | `chrome.rs` applies? |
|-------|---------|-----------------|---------------------|
| Claude Code | CLI inline (Ink) | Changed-rows delta analysis | Yes |
| Codex CLI | CLI inline (Ink) | Changed-rows delta analysis | Yes |
| OpenCode | Full-screen TUI (Bubble Tea) | Screen snapshot analysis | No (all rows are "chrome") |
| Gemini CLI | CLI inline | Changed-rows delta analysis | Yes |
| Aider | CLI sequential | Changed-rows delta analysis | Yes |

**CLI inline agents** (CC, Codex, Gemini, Aider) render output into the
terminal sequentially, with chrome at specific positions. `chrome.rs`
functions work for these — `is_separator_line`, `is_prompt_line`,
`is_chrome_row` classify individual rows.

**Full-screen TUI agents** (OpenCode) take over the entire screen. Every
row changes on every update, making delta analysis useless. These need
screen-snapshot-based parsing: identify panels by position, extract text
from known regions, detect state changes by content comparison.

---

## Shared Concepts

### Chrome Detection

"Chrome" = UI decoration rows that are NOT real agent output (separators,
mode lines, status bars, spinners, menus). Correctly classifying chrome
is critical for:

- **Silence-based question detection**: chrome-only chunks should not
  reset the silence timer or invalidate pending questions
- **Shell state transitions**: chrome-only output should not prevent
  BUSY → IDLE transitions
- **Log trimming**: chrome should be stripped from mobile logs and
  REST API responses

### Prompt Line

The row where the user types input. Each agent uses a different character:

| Agent | Prompt char | Unicode |
|-------|------------|---------|
| Claude Code | `❯` | U+276F |
| Codex CLI | `›` | U+203A |
| Gemini CLI | `>` | ASCII |

### Separator Lines

Horizontal rules that delineate sections. Detected by a run of 4+
box-drawing characters (`─ ━ ═ — ╌ ╍`). Not all agents use separators.

| Agent | Uses separators | Style |
|-------|----------------|-------|
| Claude Code | Yes | `────` around prompt box |
| Codex CLI | Partially | `────` between tool output and summary only |
| Gemini CLI | No | — |
| Aider | No | — |

### Interactive Menu Detection

All observed agent menus share the pattern `Esc to` in their footer:

| Footer variant | Agent / Context |
|---------------|-----------------|
| `Esc to cancel · Tab to amend` | CC permission prompt |
| `Enter to select · Tab/Arrow keys to navigate · Esc to cancel` | CC custom Ink menu |
| `↑↓ to navigate · Enter to confirm · Esc to cancel` | CC built-in (/mcp) |
| `Esc to cancel · r to cycle dates · ctrl+s to copy` | CC built-in (/stats) |
| `←/→ tab to switch · ↓ to return · Esc to close` | CC built-in (/status) |
| `Enter to select · ↑/↓ to navigate · Esc to cancel` | CC Ink select |
| `esc again to edit previous message` | Codex (after interrupt) |

`Esc to` is the most reliable cross-agent signal for "interactive menu active."

### OSC Sequences

Terminal escape sequences that carry structured metadata:

| Sequence | Purpose | Agent |
|----------|---------|-------|
| `\033]777;notify;Claude Code;...\007` | User attention notification | CC |
| `\033]0;...\007` | Window title (task name + spinner) | CC, Codex |
| `\033]8;;url\007` | Hyperlink | CC |
| `\033]9;4;N;\007` | Progress notification | CC |
| `\033]10;?\033\\` | Query foreground color | Codex |
| `\033]11;?\033\\` | Query background color | Codex |

---

## Code Architecture

### Unified `chrome.rs` module

All chrome detection is centralized in `src-tauri/src/chrome.rs`. The three
pipelines (pty.rs, session.rs, state.rs) all import from this single module:

```
src-tauri/src/chrome.rs
├── is_separator_line()    — run-of-4 box-drawing chars (─ ━ ═ — ╌ ╍)
├── is_prompt_line()       — all agent prompt chars: ❯ › >
├── is_chrome_row()        — 10 marker chars + dingbat range + Codex • disambiguation
├── CHROME_SCAN_ROWS       — single constant (15)
└── find_chrome_cutoff()   — unified trim logic for REST and mobile pipelines
```

| Pipeline | File | What it uses from `chrome.rs` |
|----------|------|------------------------------|
| Changed-rows parser | `pty.rs` | `is_chrome_row` (for `chrome_only`), `is_separator_line`, `is_prompt_line` |
| Screen trim (REST) | `session.rs` | `find_chrome_cutoff` (replaces local `trim_screen_chrome` body) |
| Log trim (mobile) | `state.rs` | `find_chrome_cutoff` (replaces local `find_prompt_cutoff` body) |

### Parsing Functions

#### Chrome Detection (`is_chrome_row`) — chrome.rs

Classifies changed terminal rows as "UI decoration" vs "real agent output".

Detected markers:
- `⏵` (U+23F5) — CC mode-line prefix
- `⏸` (U+23F8) — CC plan mode prefix
- `›` (U+203A) — CC/Codex mode-line prefix
- `·` (U+00B7) — CC middle-dot spinner prefix
- `▀` (U+2580) — Gemini prompt box top border
- `▄` (U+2584) — Gemini prompt box bottom border
- `░` (U+2591) — Aider Knight Rider spinner
- `█` (U+2588) — Aider Knight Rider spinner / CC context bar
- `■` (U+25A0) — Codex interrupt marker
- `•` (U+2022) — Codex spinner (disambiguated: `• Working` = chrome, `• Created` = output)
- U+2720–U+273F — CC spinner dingbats (✶✻✳✢ etc.)

Used by `chrome_only` calculation (pty.rs) which also considers `has_status_line`
from `parse_status_line` events (for Gemini braille/Aider spinners). Gates:
- `last_output_ms` timestamp updates
- `SHELL_BUSY` → `SHELL_IDLE` transitions
- `SilenceState::on_chunk()`

**Gaps:**
- Missing `⏸` (U+23F8) — plan mode chunks not classified as chrome
- `1 shell` (no ⏵⏵) not detected — new CC format without mode-line markers
- No positional awareness — cannot use "last row = always chrome" heuristic
- CC status lines (below separator) transit PTY but have no chrome markers

#### Subprocess Count (`parse_active_subtasks`) — output_parser.rs:706

Extracts subprocess count from the mode line. Must handle:

1. **Old format**: `⏵⏵ <mode> · N <type>` — markers first, count last
2. **New format**: `N <type> · ⏵⏵ <mode>` — count first, markers last
3. **Count only**: `N <type>` — no markers at all (e.g., `1 shell`)
4. **Bare mode**: `⏵⏵ <mode>` — markers only, no count (count = 0)

**Gap**: Only format 1 and 4 are currently implemented.

#### Question Detection (`extract_last_chat_line`) — pty.rs:207

Finds the last agent chat line above the prompt box:
1. Scan from bottom, find prompt line (`❯`, `›`, `>`)
2. Walk up past separators and empty lines
3. First non-empty, non-separator line = last chat line

**Robust**: Does not depend on mode line format.

---

## Test Expectations

Tests that hardcode specific bottom-zone layouts. Update these when adding new
format support.

### output_parser.rs — `parse_active_subtasks` tests

All use format: `⏵⏵|›› <mode> · N <type>` (old format only)

- `test_active_subtasks_local_agents` — `›› bypass permissions on · 2 local agents`
- `test_active_subtasks_single_bash` — `›› reading config files · 1 bash`
- `test_active_subtasks_background_tasks` — `›› fixing tests · 3 background tasks`
- `test_active_subtasks_single_local_agent` — `›› writing code · 1 local agent`
- `test_active_subtasks_bare_mode_line_resets_to_zero` — `›› bypass permissions on`
- `test_active_subtasks_explicit_zero_count` — `›› finishing · 0 bash`
- `test_active_subtasks_triangle_*` — same patterns with `⏵⏵` prefix

### pty.rs — `extract_last_chat_line` tests

- `test_extract_last_chat_line_standard_claude_code` — `⏵⏵ bypass permissions on (shift+tab to cycle)`
- `test_extract_last_chat_line_with_wiz_hud` — 3 HUD lines + mode line
- `test_extract_last_chat_line_plan_mode` — `⏸ plan mode on (shift+tab to cycle)`

### pty.rs — `is_chrome_row` / chrome_only tests

- `test_chrome_only_single_statusline_row_is_chrome` — `⏵⏵ auto mode`
- `test_chrome_only_wrapped_statusline_is_chrome` — `⏵⏵ bypass permissions on` + `✻ timer`
- `test_chrome_only_subtasks_row_is_chrome` — `›› bypass permissions on · 1 local agent`

---

## Verification Methodology

These documents must be re-verified weekly (or after agent version updates)
to catch layout changes before they break parsing.

### Procedure 1: Capture current layout from live sessions

Use `session action=list` to find active sessions, then for each:

```
session action=output session_id=<id> limit=4000               → clean text
session action=output session_id=<id> limit=8000 format=raw     → raw ANSI
```

Compare against documented layouts. Look for:
- Changed cursor-up distances (`\033[NA`) → bottom zone height changed
- New Unicode characters in mode/status lines → update `is_chrome_row`
- Changed OSC sequences → update notification detection
- New footer text → update question detection

### Procedure 2: Trigger interactive menus

1. Create a fresh session: `session action=create`
2. Start agent in restricted mode (CC: `--permission-mode default`, Codex: `-a untrusted`)
3. Request operations that trigger approval prompts
4. Capture raw output — compare separators, colors, footers
5. Cancel and clean up

### Procedure 3: Audit parser compatibility

```bash
cd src-tauri && cargo test -- --test-threads=1 \
  chrome_only \
  active_subtasks \
  extract_last_chat_line \
  separator \
  prompt_line \
  question \
  2>&1 | head -100
```

### Research Techniques

1. **Raw ANSI capture via MCP**: `session action=output format=raw` reveals
   cursor positioning, colors, and OSC sequences invisible in clean output
2. **Cursor-up distance as height probe**: `\033[NA` reveals bottom zone height
3. **OSC sequence interception**: `\033]777;notify;...` and `\033]0;...` carry metadata
4. **Color as semantic signal**: RGB colors distinguish interactive vs chrome elements
5. **Forced state transitions**: specific CLI flags surface all UI variants
6. **Screen clear detection**: `\033[2J\033[3J\033[H` distinguishes full-screen menus
