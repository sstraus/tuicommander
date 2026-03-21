# Claude Code — UI Layout Reference

Agent-specific layout reference for Claude Code (Anthropic).
See [agent-ui-analysis.md](../agent-ui-analysis.md) for shared concepts.

**Observed version**: v2.1.81 (2026-03-21)
**Rendering engine**: Ink (React for terminals)
**Rendering approach**: ANSI relative cursor positioning (`\033[NA`, `\033[1B`)

---

## Layout Anatomy (bottom → top)

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

---

## Real-world Examples (live sessions, 2026-03-21)

### Standard idle — no subprocess
```
❯
──────────────────────────────────────────────────────────────────────────
  [Opus 4.6 (1M context) | Max] │ tuicommander git:(main*)
  Context █░░░░░░░░░ 8% $0 (~$2.97) │ Usage ⚠ (429)
  ⏵⏵ bypass permissions on (shift+tab to cycle)
```

### With subprocess — new format (count left)
```
───────────────────────────────────────────────────────── extractor ──
❯
──────────────────────────────────────────────────────────────────────────
  [Opus 4.6 (1M context) | Max] │ mdkb git:(feat/document-organizer*)
  Context ░░░░░░░░░░ 4% $0 (~$79.88) │ Usage ⚠ (429)
  1 shell · ⏵⏵ bypass permissions on
```

### Subprocess only — no mode indicator
```
───────────────────────────────────────────────────────── extractor ──
❯
──────────────────────────────────────────────────────────────────────────
  [Opus 4.6 (1M context) | Max] │ mdkb git:(feat/document-organizer*)
  Context ░░░░░░░░░░ 4% $0 (~$79.81) │ Usage ⚠ (429)
  1 shell
```

### Default mode (no bypass) — single status line, no mode line
```
❯
───────────────────────────────────────────────────────────────────────────────
  [Opus 4.6 (1M context) | Max] ░░░░░░░░░░ 3% | tuicommander git:(main*) |… ○ low · /ef…
```

### Agent working — spinner above, no prompt box
```
  ⎿  $ ls -la /Users/stefano.straus/Documents/.mdkb/index.sqlite

✶ Undulating…

```

---

## Separator Line

A row containing a run of 4+ box-drawing characters (`─ ━ ═ — ╌ ╍`).
May contain embedded labels:

```
────────────────────────────────────────────────────────────────────────────────
──────────────────────────────────────────────────────────────── extractor ──
──────── ■■■ Medium /model ────────
```

---

## Spinner / Timer Lines (above upper separator)

```
✶ Undulating…
✻ Sautéed for 1m 19s
✳ Ideating… (1m 32s · ↓ 2.2k tokens)
✻ Sautéed for 2m 9s · 1 local agent still running
· Proofing… (1m 14s · ↓ 1.6k tokens)
```

Markers: `✶` (U+2736), `✻` (U+273B), `✳` (U+2733), `✢` (U+2722), `·` (U+00B7).
Detected by `is_chrome_row` (✻ check) and `parse_status_line` (dingbat range U+2720–U+273F).

---

## Status Lines (between lower separator and mode line)

**Zero or more lines.** Content is arbitrary and agent-customizable.
**Indented with 2 spaces** (via `\033[2C`).

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

Configured via `~/.claude/settings.json` → `statusLine`, but **rendered
through the PTY** using ANSI cursor positioning. They appear as `changed_rows`
and DO pass through `is_chrome_row`.

**Bug**: These lines contain none of the 4 chrome markers (⏵, ›, ✻, •),
so they are not classified as chrome.

---

## Mode Line (last row)

The final visible row. Always chrome. **Indented with 2 spaces**.

### Observed variants

| Format | Example | Source |
|--------|---------|--------|
| Mode only | `  ⏵⏵ bypass permissions on` | test |
| Mode + hint | `  ⏵⏵ bypass permissions on (shift+tab to cycle)` | live |
| Mode + subprocess (old) | `  ⏵⏵ bypass permissions on · 1 shell` | test |
| Mode + subprocess (old, plural) | `  ⏵⏵ bypass permissions on · 2 local agents` | test |
| Subprocess + mode (new) | `  1 shell · ⏵⏵ bypass permissions on` | live |
| Subprocess only | `  1 shell` | screenshot |
| Plan mode | `  ⏸ plan mode on (shift+tab to cycle)` | test |
| Accept edits | `  ⏵⏵ accept edits on (shift+tab to cycle)` | test |
| Auto mode | `  ⏵⏵ auto mode` | test |
| Empty | `` | test |
| Absent (default mode) | N/A — no mode line at all | live |

### Key markers

- `⏵⏵` (U+23F5 x2) — current mode-line prefix
- `››` (U+203A x2) — older mode-line prefix
- `⏸` (U+23F8) — plan mode prefix
- `·` (U+00B7) — separator between mode and subprocess count

### Subprocess types observed

`shell` / `shells`, `local agent` / `local agents`, `bash`, `background tasks`

---

## Interactive Menus

### Permission Prompt

Replaces the entire bottom zone when CC requests tool approval.

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

| Feature | Normal | Permission prompt |
|---------|--------|-------------------|
| Top separator | Gray `─` (rgb 136,136,136) | **Blue `─`** (rgb 177,185,249) |
| Content separator | None | **Dotted `╌`** (U+254C) |
| `❯` char | Gray prompt | **Blue selection** (rgb 177,185,249) |
| Mode line | ⏵⏵/⏸ on last row | **None** |
| Last line | Mode indicator | **`Esc to cancel · Tab to amend`** |

### Custom Ink Menu (e.g., /wiz:setup)

```
────────────────────────────────────── (dim gray separator, \033[2m)
← ☐ Essential  ☐ Workflow  ☐ Tools  ✔ Submit  →
Select essential components to install:
❯ 1. [ ] notifications              (checkbox, ❯ blue)
  ...
────────────────────────────────────── (dim gray separator)
  6. Chat about this
Enter to select · Tab/Arrow keys to navigate · Esc to cancel
```

May clear the entire screen (`\033[2J\033[3J\033[H`).

### Built-in Menu (/mcp)

```
───────────────────────────────────── (blue separator, rgb 177,185,249)
  Manage MCP servers
  ❯ tuicommander · ✔ connected
    context7 · ✔ connected
    mac · ✘ failed
  ※ Run claude --debug to see error logs
  https://code.claude.com/docs/en/mcp for help   (OSC 8 hyperlink)
 ↑↓ to navigate · Enter to confirm · Esc to cancel   (italic, \033[3m)
```

### Built-in (/stats)

```
───────────────────────────────────────────────────────────────────────
    Overview   Models
      Mar Apr May Jun ... Mar
      ··········░·▓██▓█·
  ...
  You've used ~29x more tokens than The Count of Monte Cristo
    Esc to cancel · r to cycle dates · ctrl+s to copy
```

### Built-in (/status)

```
───────────────────────────────────────────────────────────────────────
   Status   Config   Usage
  ╭───────────────────────────────────────────────────────────────────╮
  │ ⌕ Search settings...                                             │
  ╰───────────────────────────────────────────────────────────────────╯
    Auto-compact                              true
    ...
  ↓ 3 more below
  ←/→ tab to switch · ↓ to return · Esc to close
```

Uses box-drawing `╭╮╰╯│` for search box (not matched by `is_separator_line`).

---

## OSC 777 Notifications

CC emits terminal notifications for user attention events:

```
\033]777;notify;Claude Code;Claude needs your permission to use Write\007
\033]777;notify;Claude Code;Claude Code needs your attention\007
```

All share the prefix `\033]777;notify;Claude Code;`.

---

## Ink Rendering Mechanics

### Spinner animation (above separator)
```
\033[8A          ← cursor UP 8 rows (to spinner position)
✶               ← overwrite spinner character
\r\n × 7        ← 7 empty newlines back down to bottom
```

The cursor-up count equals the bottom zone height:
- **8** = bypass mode (spinner + empty + sep + prompt + sep + 2 status + mode)
- **6** = default mode (spinner + empty + sep + prompt + sep + 1 status, no mode)
- **10** = with subprocess in bypass mode

### Full bottom zone redraw
```
\r\033[1B  separator ────
\r\033[1B  ❯ [input]
\r\033[1B  separator ────
\r\n       \033[2C [status line 1]    ← 2C = cursor forward 2 = indent
\r\n       \033[2C [status line 2]
\r\n       \033[2C [mode line]
```

### Key implications
- All bottom-zone rows transit the PTY as ANSI sequences
- The vt100 parser processes them into screen buffer rows
- Spinner updates touch the spinner row AND all rows below it,
  causing the entire bottom zone to appear as changed rows
- The 2-space indent on status/mode lines comes from `\033[2C`
