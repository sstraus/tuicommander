# Agent Bottom Zone — Layout Reference

Reference document for parsing the bottom zone of AI agent terminal UIs.
Used by `output_parser.rs` and `pty.rs` for chrome detection, question detection,
and subprocess counting.

## Claude Code Layout

Claude Code's terminal UI is structured bottom-up from the last visible row.

### Anatomy (bottom → top)

```
[agent output / response text]
[empty line(s)]
✶ Undulating… (1m 32s · ↓ 2.2k tokens)     (spinner — above separator, while working)
[empty line]
──────────────────────────────────── (upper separator, may contain label)
❯ [user input]                       (prompt line)
──────────────────────────────────── (lower separator)
  [status line(s)]                   (0-N lines, indented 2 spaces)
  [mode line]                        (last row, indented 2 spaces)
```

### Real-world examples (from live sessions, 2026-03-21, CC v2.1.81)

#### Standard idle — no subprocess
```
❯
──────────────────────────────────────────────────────────────────────────
  [Opus 4.6 (1M context) | Max] │ tuicommander git:(main*)
  Context █░░░░░░░░░ 8% $0 (~$2.97) │ Usage ⚠ (429)
  ⏵⏵ bypass permissions on (shift+tab to cycle)
```

#### With subprocess — new format (count left)
```
───────────────────────────────────────────────────────── extractor ──
❯
──────────────────────────────────────────────────────────────────────────
  [Opus 4.6 (1M context) | Max] │ mdkb git:(feat/document-organizer*)
  Context ░░░░░░░░░░ 4% $0 (~$79.88) │ Usage ⚠ (429)
  1 shell · ⏵⏵ bypass permissions on
```

#### Subprocess only — no mode indicator
```
───────────────────────────────────────────────────────── extractor ──
❯
──────────────────────────────────────────────────────────────────────────
  [Opus 4.6 (1M context) | Max] │ mdkb git:(feat/document-organizer*)
  Context ░░░░░░░░░░ 4% $0 (~$79.81) │ Usage ⚠ (429)
  1 shell
```

#### Multiselect permission prompt (replaces everything below lower separator)
```
 Do you want to proceed?
 ❯ 1. Yes
   2. Yes, and don't ask again for mdkb - status commands in
      /Users/stefano.straus/Gits/personal/mdkb
   3. No

 Esc to cancel · Tab to amend
```

#### Agent working — spinner above, no prompt box
```
  ⎿  $ ls -la /Users/stefano.straus/Documents/.mdkb/index.sqlite

✶ Undulating…

```

### Separator Line

A row containing a run of 4+ box-drawing characters (`─ ━ ═ — ╌ ╍`).
May contain embedded labels:

```
────────────────────────────────────────────────────────────────────────────────
──────────────────────────────────────────────────────────────── extractor ──
──────── ■■■ Medium /model ────────
```

### Prompt Line

Starts with one of:
- `❯` (U+276F) — Claude Code / Ink
- `›` (U+203A) — Codex CLI
- `> ` or bare `>` — Gemini CLI, generic

### Spinner / Timer Lines (above upper separator)

When the agent is working, spinner/timer lines appear above the separator:

```
✶ Undulating…
✻ Sautéed for 1m 19s
✳ Ideating… (1m 32s · ↓ 2.2k tokens)
✻ Sautéed for 2m 9s · 1 local agent still running
✻ Cooked for 44s
✻ Baked for 48s
✻ Brewed for 53s
```

Markers: `✶` (U+2736), `✻` (U+273B), `✳` (U+2733), `✢` (U+2722).
These are already detected by `is_chrome_row` via the `✻` check and by
`parse_status_line` via the dingbat range U+2720–U+273F.

### Status Lines (between lower separator and mode line)

**Zero or more lines.** Content is arbitrary and agent-customizable.
**Indented with 2 spaces** in CC v2.1.81+.

```
  [Opus 4.6 (1M context) | Max] │ tuicommander git:(main*)
  Context █░░░░░░░░░ 5% $0 (~$0.64) │ Usage ⚠ (429)
```

Or Wiz HUD:
```
  [Opus 4.6 | Team] 54% | wiz-agents git:(main)
  5h: 42% (3h) | 7d: 27% (2d)
  ✓ Edit ×7 | ✓ Bash ×5
```

Or nothing at all. Content is configured via Claude Code's status line API
(`~/.claude/settings.json` → `statusLine`), but **rendered through the PTY**
using ANSI cursor positioning (`\033[2C` = 2-column indent). They appear as
`changed_rows` in the VT100 screen buffer and DO pass through `is_chrome_row`.

Since these lines contain none of the 4 chrome markers (⏵, ›, ✻, •), they are
**not classified as chrome** — this is a bug. A chunk containing only status
line updates is incorrectly treated as real agent output.

### Mode Line (last row)

The final visible row of the screen. Always chrome. **Indented with 2 spaces** in
CC v2.1.81+. Has many observed variants:

#### Permission mode indicators

| Format | Example | Source |
|--------|---------|--------|
| Mode only | `  ⏵⏵ bypass permissions on` | test |
| Mode + hint | `  ⏵⏵ bypass permissions on (shift+tab to cycle)` | live session |
| Mode + subprocess (old) | `  ⏵⏵ bypass permissions on · 1 shell` | test |
| Mode + subprocess (old, plural) | `  ⏵⏵ bypass permissions on · 2 local agents` | test |
| Subprocess + mode (new) | `  1 shell · ⏵⏵ bypass permissions on` | live session |
| Subprocess only | `  1 shell` | screenshot |
| Plan mode | `  ⏸ plan mode on (shift+tab to cycle)` | test |
| Accept edits | `  ⏵⏵ accept edits on (shift+tab to cycle)` | test |
| Auto mode | `  ⏵⏵ auto mode` | test |
| Empty | `` | test |

Note: the `(shift+tab to cycle)` hint appears when there are no active
subprocesses. When subprocesses are present, it may be omitted.

#### Key markers

- `⏵⏵` (U+23F5 x2) — current CC mode-line prefix
- `››` (U+203A x2) — older CC mode-line prefix
- `⏸` (U+23F8) — plan mode prefix
- `·` (U+00B7) — middle dot separator between mode and subprocess count

#### Subprocess types observed

- `shell` / `shells`
- `local agent` / `local agents`
- `bash`
- `background tasks`

### Multiselect Permission Prompt (replaces bottom zone)

When CC presents a permission confirmation, the entire bottom zone is replaced.
Observed structure (from raw ANSI, CC v2.1.81):

```
⏺ Write(/tmp/test-permission-prompt.txt)
────────────────────────────────────────── (BLUE separator, rgb 177,185,249)
 Create file
 ../../../../../tmp/test-permission-prompt.txt
╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌ (dotted ╌ U+254C, rgb 80,80,80)
  1 hello
╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌
 Do you want to create test-permission-prompt.txt?
 ❯ 1. Yes                                (❯ colored BLUE, rgb 177,185,249)
   2. Yes, allow all edits in tmp/ during this session (shift+tab)
   3. No

 Esc to cancel · Tab to amend
```

#### Key differences from normal bottom zone

| Feature | Normal | Multiselect |
|---------|--------|-------------|
| Top separator | Gray `─` (rgb 136,136,136) | **Blue `─`** (rgb 177,185,249) |
| Content separator | None | **Dotted `╌`** (U+254C) |
| `❯` char | Gray prompt (rgb 153,153,153) | **Blue selection** (rgb 177,185,249) |
| Mode line | ⏵⏵/⏸ on last row | **None** |
| Last line | Mode indicator | **`Esc to cancel · Tab to amend`** |
| Status lines | Present below lower sep | **None** |

#### OSC 777 notifications

CC emits **OSC 777 terminal notifications** in the raw PTY stream for user
attention events. Observed variants:

```
\033]777;notify;Claude Code;Claude needs your permission to use Write\007
\033]777;notify;Claude Code;Claude Code needs your attention\007
```

All share the prefix `\033]777;notify;Claude Code;`. This is a structured
terminal notification, not parseable text content. It appears in the raw
byte stream and could be intercepted before the vt100 parser processes it.

### Custom Ink Menu (e.g., skill UI, slash commands)

When a skill presents a custom interactive menu, the rendering is different
from the permission prompt. Observed with `/wiz:setup` (CC v2.1.81):

```
────────────────────────────────────── (dim gray separator, \033[2m)
← ☐ Essential  ☐ Workflow  ☐ Tools  ✔ Submit  →   (tab bar, highlight rgb 177,185,249)

Select essential components to install:

❯ 1. [ ] notifications              (checkbox, ❯ blue)
  Terminal bell for task completion
  2. [ ] journaling
  ...
  5. [ ] Type something
     Next
────────────────────────────────────── (dim gray separator)
  6. Chat about this

Enter to select · Tab/Arrow keys to navigate · Esc to cancel
```

### Built-in Menu (/mcp, /model, etc.)

CC's built-in slash commands show a different menu style:

```
───────────────────────────────────── (blue separator, rgb 177,185,249)
  Manage MCP servers
  8 servers
    Local MCPs (...)
  ❯ tuicommander · ✔ connected      (❯ blue, rgb 177,185,249)
    context7 · ✔ connected
    mac · ✘ failed
    ...
  ※ Run claude --debug to see error logs
  https://code.claude.com/docs/en/mcp for help   (OSC 8 hyperlink)
 ↑↓ to navigate · Enter to confirm · Esc to cancel   (italic, \033[3m)
```

### All interactive menu variants compared

| Menu type | Footer text |
|-----------|------------|
| Permission prompt | `Esc to cancel · Tab to amend` |
| Custom Ink (wiz:setup) | `Enter to select · Tab/Arrow keys to navigate · Esc to cancel` |
| Built-in (/mcp) | `↑↓ to navigate · Enter to confirm · Esc to cancel` (italic) |
| Built-in (/stats) | `Esc to cancel · r to cycle dates · ctrl+s to copy` |
| Built-in (/status) | `←/→ tab to switch · ↓ to return · Esc to close` |
| Ink select (from tests) | `Enter to select · ↑/↓ to navigate · Esc to cancel` |

Other common traits:
- Blue separator `─` (rgb 177,185,249) in permission and built-in menus
- `❯` (blue, rgb 177,185,249) as selection indicator
- Custom Ink menus may clear the screen (`\033[2J\033[3J\033[H`)
- `/status` uses box-drawing `╭╮╰╯│` for the search box (not matched by
  `is_separator_line` which only checks `─━═—╌╍`)

**Common signal across ALL menus**: `Esc to` appears in every footer variant
(`Esc to cancel` or `Esc to close`). This is the most reliable text-based
signal for detecting an active interactive menu.

#### Implications for all interactive menus

- `is_prompt_line` will match `❯ 1. Yes` as a prompt line (false positive)
- `extract_last_chat_line` will anchor on `❯ 1. Yes` and walk up, finding
  `Do you want to create test-permission-prompt.txt?` — which is actually
  correct for question detection
- `is_separator_line` will match both the blue `─` separator and the dotted
  `╌` separators (both chars are in its charset)
- The question `Do you want to...?` ends with `?` → question detection fires

### Agent Working (no prompt visible)

When the agent is actively working, the separator + prompt + mode line are
NOT visible. Only spinner/timer lines appear at the bottom of the output.

The prompt box (separator + `❯` + separator + status + mode line) appears
only when the agent is waiting for user input.

### Ink Rendering Mechanics

Claude Code uses Ink (React for terminals) which renders via ANSI cursor
positioning, NOT sequential line output. Key patterns observed (CC v2.1.81):

#### Spinner animation (above separator)
```
\033[8A          ← cursor UP 8 rows (to spinner position)
✶               ← overwrite spinner character
\r\n × 7        ← 7 empty newlines back down to bottom
```

The `8A` count equals the total height of the bottom zone (1 spinner +
1 empty + 1 separator + 1 prompt + 1 separator + 2 status + 1 mode = 8).

#### Full bottom zone redraw
```
\r\033[1B  separator ────
\r\033[1B  ❯ [input]
\r\033[1B  separator ────
\r\n       \033[2C [status line 1]    ← 2C = cursor forward 2 = indent
\r\n       \033[2C [status line 2]
\r\n       \033[2C [mode line]
```

#### Implications for parsing
- All bottom-zone rows transit the PTY as ANSI sequences
- The vt100 parser processes them into screen buffer rows
- Changed rows appear in `changed_rows` with correct `row_index`
- The 2-space indent on status/mode lines comes from `\033[2C`
- Spinner updates touch the spinner row AND all rows below it (because
  of the `\r\n` padding), causing the entire bottom zone to appear as
  changed rows even when only the spinner character changed

---

## Other Agent Layouts

### Codex CLI

```
[output]
[empty]
› [user input]
[empty]
  gpt-5.3-codex high · 100% left · ~/project
```

No separators. Prompt is `›`. Status line below prompt.

### Gemini CLI

```
[output]
[empty]
> [user input]
⠋ Processing...
```

No separators. Prompt is `>`. Braille spinner below.

### Aider

```
[output]
░█   Working on task...
```

Knight Rider scanner. No prompt box — interactive via stdin.

---

## Code Architecture

### Current state: 3 divergent pipelines

The bottom-zone parsing logic is duplicated across three separate pipelines
that share concepts but diverge in implementation:

| Pipeline | File | Purpose | Consumers |
|----------|------|---------|-----------|
| Changed-rows parser | `pty.rs` | Classify chrome vs real output, detect questions | Desktop + PWA (shared reader thread) |
| Screen trim (REST) | `session.rs` | Strip chrome from screen snapshot for API | PWA/MCP REST clients |
| Log trim (mobile) | `state.rs` | Strip chrome from scrollback for mobile log | Mobile log view |

The event parser (`output_parser.rs` → `parse_clean_lines`) is shared across
all pipelines. But the helper functions are duplicated:

| Function | pty.rs | session.rs | state.rs | Notes |
|----------|--------|------------|----------|-------|
| `is_separator_line` | run-of-4 ✓ | run-of-4 ✓ | all-chars ✗ | state.rs broken for decorated separators |
| `is_prompt_line` | `❯›>` ✓ | `❯>` (inline) | `❯>` ✗ | state.rs missing Codex `›` |
| `is_chrome_row` | 4 markers | N/A | N/A | Missing `⏸`, no positional awareness |
| Scan window | 15 rows | 15 rows | **8 rows** | state.rs too small for Wiz HUD |

### Recommendation: shared `chrome.rs` module

Extract canonical versions of all shared functions into a single module:

```
src-tauri/src/chrome.rs
├── is_separator_line()    — run-of-4 logic (from pty.rs)
├── is_prompt_line()       — with all 3 chars ❯ › > (from pty.rs)
├── is_chrome_row()        — expanded markers + positional awareness
├── CHROME_SCAN_ROWS       — single constant (15)
└── find_chrome_cutoff()   — unified logic for all trim operations
```

All three pipelines import from `chrome.rs`. No more divergence.

---

## Parsing Functions

### Chrome Detection (`is_chrome_row`) — pty.rs:111

Classifies changed terminal rows as "UI decoration" vs "real agent output".
Returns true if row text contains any of:
⏵ (U+23F5), › (U+203A), ✻ (U+273B), • (U+2022).

Used by `chrome_only` calculation (pty.rs:794) which gates:
- `last_output_ms` timestamp updates (chrome-only ticks don't stamp real output time)
- `SHELL_BUSY` → `SHELL_IDLE` transitions
- `SilenceState::on_chunk()` — chrome-only chunks don't invalidate pending questions

**Gaps:**
- Missing `⏸` (U+23F8) — plan mode chunks not classified as chrome
- `1 shell` (no ⏵⏵) not detected — new CC format without mode-line markers
- No positional awareness — cannot use "last row = always chrome" heuristic

### Subprocess Count (`parse_active_subtasks`) — output_parser.rs:706

Extracts subprocess count from the mode line. Must handle:

1. **Old format**: `⏵⏵ <mode> · N <type>` — markers first, count last
2. **New format**: `N <type> · ⏵⏵ <mode>` — count first, markers last
3. **Count only**: `N <type>` — no markers at all (e.g., `1 shell`)
4. **Bare mode**: `⏵⏵ <mode>` — markers only, no count (count = 0)

Result consumed by `Terminal.tsx:414` → `terminalsStore.activeSubTasks`.
If count > 0 and question confidence is low, question events are dropped (line 452).

**Gap**: Only format 1 and 4 are currently implemented. Formats 2 and 3 silently
fail, leaving `activeSubTasks = 0` → false question notifications.

### Question Detection (`extract_last_chat_line`) — pty.rs:207

Finds the last agent chat line above the prompt box. Algorithm:

1. Scan from bottom, find prompt line (`❯`, `›`, `>`)
2. Walk up past separators and empty lines
3. First non-empty, non-separator line = last chat line

**Robust**: Does not depend on mode line format.

### Screen Verification (`verify_question_on_screen`) — pty.rs:163

Confirms a detected question is still visible in the last N rows of the screen.
Uses `max_bottom_rows = 15` (hardcoded at call site in silence timer).

### Prompt Line Detection (`is_prompt_line`)

Three copies with diverging behavior:

| Location | `❯` | `>` / `> ` | `›` (Codex) |
|----------|-----|------------|-------------|
| pty.rs:190 | ✓ | ✓ | ✓ |
| state.rs:1685 | ✓ | ✓ | ✗ |
| session.rs (inline) | ✓ | ✓ | ✗ |

### Separator Line Detection (`is_separator_line`)

Three copies with diverging logic:

| Location | Algorithm | Decorated separators |
|----------|-----------|---------------------|
| pty.rs:173 | Run of 4+ box chars | ✓ works |
| session.rs:902 | Run of 4+ box chars | ✓ works |
| state.rs:1691 | ALL chars must be box | ✗ broken |

The `state.rs` version fails on `──── extractor ──` because not all chars
are box-drawing. This breaks mobile log chrome trimming for decorated separators.

### Chrome Scan Window Sizes

Three hardcoded constants for the same concept — "how many rows from the
bottom to scan for agent chrome":

| Location | Value | Context |
|----------|-------|---------|
| state.rs:1682 `CHROME_SCAN_ROWS` | 8 | Mobile log trim |
| pty.rs (caller) `max_bottom_rows` | 15 | Question screen verify |
| session.rs `trim_screen_chrome` | 15 | REST API screen trim |

`CHROME_SCAN_ROWS = 8` is too small. Wiz HUD alone uses 7 rows below the
last chat line. One extra row and the prompt falls outside the scan window.

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

- `test_extract_last_chat_line_standard_claude_code` — mode line: `⏵⏵ bypass permissions on (shift+tab to cycle)`
- `test_extract_last_chat_line_with_wiz_hud` — 3 HUD lines + `⏵⏵ bypass permissions on (shift+tab to cycle)`
- `test_extract_last_chat_line_plan_mode` — mode line: `⏸ plan mode on (shift+tab to cycle)`
- `test_extract_last_chat_line_prompt_with_separator_above` — mode line: `⏵⏵ mode line`

### pty.rs — `is_chrome_row` / chrome_only tests

- `test_chrome_only_single_statusline_row_is_chrome` — `⏵⏵ auto mode`
- `test_chrome_only_wrapped_statusline_is_chrome` — `⏵⏵ bypass permissions on` + `✻ timer`
- `test_chrome_only_subtasks_row_is_chrome` — `›› bypass permissions on · 1 local agent`

### pty.rs — question detection e2e tests

- `test_e2e_question_detection_with_mode_line` — question + `⏵⏵ Reading files`
- `test_e2e_question_then_decoration_then_silence` — question, then `⏵⏵ Idle`

---

## Verification Methodology

This document must be re-verified weekly (or after CC version updates) to
catch layout changes before they break our parsing. The following procedures
are designed to be run by an AI agent with MCP access to TUICommander.

### Procedure 1: Capture current layout from live sessions

Use `session action=list` to find active CC sessions, then for each:

```
session action=output session_id=<id> limit=4000               → clean text
session action=output session_id=<id> limit=8000 format=raw     → raw ANSI
```

Compare against documented layouts. Look for:
- Changed cursor-up distances (`\033[NA`) → bottom zone height changed
- New Unicode characters in mode line → update `is_chrome_row`
- Changed OSC sequences → update notification detection
- New footer text → update question detection

### Procedure 2: Trigger permission prompt

1. Create a fresh CC session: `session action=create`
2. Start CC in default mode: `claude --permission-mode default`
3. Request a write operation: `"create a file called /tmp/test.txt with hello"`
4. Capture raw output of the multiselect prompt
5. Compare separator colors, footer text, selection characters
6. Press Escape to cancel, then `/exit`

### Procedure 3: Trigger custom Ink menu

1. Use an existing CC session or create a new one
2. Send a slash command that produces a menu (e.g., `/wiz:setup`)
3. Capture raw output
4. Compare tab bar, checkbox, footer patterns
5. Press Escape to cancel

### Procedure 4: Audit parser compatibility

After capturing new layouts, run:

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

Any test failures indicate the parser has not been updated for the new layout.

### Research techniques used in this analysis

These techniques proved effective for reverse-engineering Ink's rendering:

1. **Raw ANSI capture via MCP**: `session action=output format=raw` gives
   the actual byte stream including cursor positioning, colors, and OSC
   sequences that are invisible in clean output.

2. **Cursor-up distance as height probe**: The `\033[NA` sequence before
   spinner updates reveals the total bottom zone height. Comparing this
   value across different states (idle, working, subprocess, permission
   prompt) shows how Ink restructures the layout.

3. **OSC sequence interception**: `\033]777;notify;...` and `\033]0;...`
   (window title) sequences carry structured metadata. The window title
   contains the current task name and spinner state.

4. **Color as semantic signal**: Different bottom zone elements use
   distinct RGB colors (blue for interactive elements, gray for chrome,
   dim for separators). These could be used as parsing signals via vt100
   cell attributes.

5. **Forced state transitions**: Creating sessions with specific
   `--permission-mode` flags and sending requests that trigger different
   tool permissions surfaces all multiselect variants.

6. **Screen clear detection**: Custom Ink menus emit `\033[2J\033[3J\033[H`
   (clear screen + scrollback + cursor home) which is detectable in the
   raw stream and distinguishes them from inline permission prompts.
