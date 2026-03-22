# Gemini CLI — UI Layout Reference

Agent-specific layout reference for Gemini CLI (Google).
See [agent-ui-analysis.md](../agent-ui-analysis.md) for shared concepts.

**Observed version**: v0.34.0 (2026-03-22)
**Rendering engine**: Ink-like (Node.js, ANSI relative positioning)
**Rendering approach**: ANSI relative cursor positioning (`\033[1A`, `\033[2K`, `\033[G`)

---

## Layout Anatomy (bottom → top)

```
[agent output with ✦ prefix]
[suggest line]
                                                          ? for shortcuts
───────────────────────────────── (separator, gray rgb 88,88,88)
 Shift+Tab to accept edits                    1 MCP server | 3 skills
▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀ (prompt box top, dark rgb 30,30,30 on bg 65,65,65)
 > [user input]                  (prompt, purple >, ghost text gray)
▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄ (prompt box bottom)
 workspace (/directory)          branch          sandbox              /model
 ~/path                          main            no sandbox           Auto (Gemini 3)
```

Bottom zone = 8 rows: shortcuts hint, separator, info line, prompt box (3 rows), status labels, status values.

---

## Real-world Examples (live session, 2026-03-22)

### Startup banner
```
  ▝▜▄     Gemini CLI v0.34.0
    ▝▜▄
   ▗▟▀    Signed in with Google: user@example.com /auth
  ▝▀      Plan: Gemini Code Assist for individuals /upgrade
╭───────────────────────────────────────────────────────────────────────╮
│ We're making changes to Gemini CLI that may impact your workflow.     │
│ What's Changing: ...                                                  │
│ Read more: https://goo.gle/geminicli-updates                          │
╰───────────────────────────────────────────────────────────────────────╯
Tips for getting started:
1. Create GEMINI.md files to customize your interactions
2. /help for more information
3. Ask coding questions, edit code or run commands
4. Be specific for the best results
```

- Geometric ASCII art logo: `▝▜▄` / `▗▟▀` / `▝▀`
- Auth + plan info inline
- Notification box with `╭╮╰╯│` border (like CC /status search box)
- Numbered tips list

### Idle (waiting for input)
```
                                                                  ? for shortcuts
─────────────────────────────────────────────────────────────────────────────────
 Shift+Tab to accept edits                              1 MCP server | 3 skills
▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀
 >   Type your message or @path/to/file
▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▀▀
 workspace (/directory)          branch          sandbox              /model
 ~/Gits/personal/tuicommander   main            no sandbox           Auto (Gemini 3)
```

### Working state (spinner)
```
✦ [[intent: read package.json version(package.json)]]
  I will read the package.json file to find the project version.
 ⠴ Check tool-specific usage stats with /stats tools… (esc to cancel, 14s)
─────────────────────────────────────────────────────────────────────────────────
 Shift+Tab to accept edits
▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀
 >   Type your message or @path/to/file
▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▀▀
 workspace (/directory)          branch          sandbox              /model
 ~/Gits/personal/tuicommander   main            no sandbox           Auto (Gemini 3)
```

- Spinner line appears between agent output and separator
- Braille spinner + italic tip text + `(esc to cancel, Ns)` timer
- During work, `? for shortcuts` disappears, info line loses right-side text

### Tool call (in response)
```
╭─────────────────────────────────────────────────────────────────────────╮
│ ✓  ReadFile package.json                                                │
│                                                                         │
╰─────────────────────────────────────────────────────────────────────────╯
```

- Bordered box with `╭╮╰╯│` (same chars as startup notification)
- `✓` prefix for completed tool calls
- Tool name + arguments inline

### Agent response (completed)
```
✦ The version of this project is 0.9.5, as specified in the package.json file.
  [[suggest: View CHANGELOG.md | Check README.md | List active sessions | Search codebase]]
```

- `✦` (U+2726, purple rgb 215,175,255) prefix for agent output
- `[[suggest: ...]]` line follows response (TUICommander protocol)
- No token/cost report

### Out-of-workspace write rejection
```
✦ I am unable to create the file at /tmp/gemini-test.txt because it is outside
  the allowed workspace directories. I can, however, create it within the
  project directory or the project's temporary directory.
  Would you like me to create it at ~/.gemini/tmp/tuicommander/gemini-test.txt?
```

- No interactive permission prompt — Gemini refuses with text explanation
- Workspace restriction enforced at model level, not via UI prompt

---

## Prompt Line

- Character: `>` (ASCII, colored purple rgb 215,175,255)
- Background: dark gray `rgb(65,65,65)` — `\033[48;2;65;65;65m`
- Prompt box bordered by `▀▀▀` (U+2580, upper half block) top and `▄▄▄` (U+2584, lower half block) bottom
- Border colors: dark `rgb(30,30,30)` foreground on `rgb(65,65,65)` background
- Ghost text: gray `rgb(175,175,175)` — `Type your message or @path/to/file`
- Cursor shown with reverse video `\033[7m`
- Enter = submit (single line prompt)

---

## Separator Line

Single horizontal rule above the prompt area:

```
─────────────────────────────────────────────────── (gray, rgb 88,88,88)
```

- Always present in idle and working states
- No decorated separators (no embedded labels)
- Uses `─` (U+2500) — same char as CC and Codex

---

## Spinner / Working Indicator

```
 ⠴ Check tool-specific usage stats with /stats tools… (esc to cancel, 14s)
 ⠋ Exclude specific tools from being used (settings.json)… (esc to cancel, 33s)
```

- **Braille spinner**: `⠋⠙⠹⠸⠴⠦⠧⠇` (U+2800 range)
- **Color varies**: blue `rgb(135,189,241)`, green `rgb(224,255,206)` — colors shift during animation
- **Format**: `⠋ <italic tip text>… (esc to cancel, Ns)`
- **Tip text**: italic (`\033[3m`), shows contextual tips/suggestions during work
- **Timer**: `(esc to cancel, Ns)` in gray `rgb(175,175,175)`
- **Position**: between agent output and separator (above bottom zone)

Already detected by `parse_status_line` via `GEMINI_SPINNER_RE` (braille range check).

---

## Status Bar (bottom 2 rows)

Always visible. 4 columns with label/value pairs:

```
 workspace (/directory)          branch          sandbox              /model
 ~/Gits/personal/tuicommander   main            no sandbox           Auto (Gemini 3)
```

- **Labels**: gray `rgb(175,175,175)` — `workspace (/directory)`, `branch`, `sandbox`, `/model`
- **Values**: white `rgb(255,255,255)` — path, branch name, sandbox status, model name
- **Sandbox warning**: pink `rgb(255,135,175)` when `no sandbox`

---

## Info Line (between separator and prompt box)

```
 Shift+Tab to accept edits                              1 MCP server | 3 skills
```

- Left: `Shift+Tab to accept edits` (gray)
- Right: `N MCP server | N skills` (gray) — MCP and skill counts
- Above the prompt box, below the separator

A second info hint `? for shortcuts` appears right-aligned above the separator when idle.

---

## Window Title (OSC 0)

```
\033]0;◇  Ready (tuicommander)\007
```

- `◇` (U+25C7, white diamond) — state indicator
- `Ready` — current state
- `(tuicommander)` — workspace name
- Updates on state changes

---

## Detection Signals

### Agent Identification
- `Gemini CLI v` in startup banner
- Geometric ASCII logo `▝▜▄`
- `✦` (U+2726) output prefix
- Braille spinner `⠋⠙⠹⠸⠴⠦⠧⠇`
- `? for shortcuts` hint line
- OSC 0 with `◇` diamond

### Chrome Detection (`is_chrome_row`)
- Braille spinner chars — detected by `parse_status_line` via `GEMINI_SPINNER_RE`
- Separator `─────` — detected by `is_separator_line` ✓
- Prompt `>` — detected by `is_prompt_line` ✓
- `▀▀▀` / `▄▄▄` prompt box borders — NOT in chrome marker set
- Status bar labels/values — NOT chrome markers
- `✦` (U+2726) — NOT in `is_chrome_row` marker set

### Subtask / Subprocess Count
**None.** Gemini CLI does not expose subprocess/subtask counts.
Tool calls shown inline in bordered boxes (`╭───╮ ✓ ReadFile ╰───╯`).

### Permission / Approval
- **No interactive permission UI** — workspace restriction enforced at model level
- Out-of-scope writes rejected with text explanation
- No `Esc to cancel` permission footer
- No OSC 777 notifications observed

---

## Rendering Mechanics (raw ANSI)

### Relative cursor positioning
```
\033[1A     — cursor UP 1 row (repeated for multi-row updates)
\033[2K     — erase entire line
\033[G      — cursor to column 1
\033[4A     — cursor UP 4 (bottom zone jump)
\033[4G     — cursor to column 4
\033[4B     — cursor DOWN 4 (back to bottom)
```

Uses `\033[1A]` repeated (like CC) rather than absolute `\033[r;cH` (like Codex/OpenCode).
Bottom zone updates use `\033[4A...\033[4B` pattern to jump up, redraw, jump back.

### Prompt box rendering
```
\033[48;2;65;65;65m              — dark gray background
\033[38;2;30;30;30m▀▀▀▀...      — dark top border on gray bg
\033[38;2;215;175;255m>           — purple prompt char
\033[7m \033[27m                 — cursor (reverse video block)
\033[38;2;175;175;175m...        — gray ghost text
\033[38;2;30;30;30m▄▄▄▄...      — dark bottom border
\033[49m                         — reset background
```

### Color scheme
```
\033[38;2;215;175;255m   — purple (prompt char >, output prefix ✦, file names)
\033[38;2;255;255;255m   — white (agent text, status values)
\033[38;2;175;175;175m   — gray (labels, hints, timer)
\033[38;2;88;88;88m      — dark gray (separator ─────)
\033[38;2;255;135;175m   — pink (sandbox warning)
\033[38;2;135;189;241m   — blue (spinner, varies)
\033[38;2;224;255;206m   — green (spinner, varies)
```

### Spinner animation
```
\033[3m     — italic on (for tip text)
\033[23m    — italic off
```

Spinner updates use the same `\033[1A]\033[2K]` erase-and-redraw pattern as the bottom zone.

---

## Implications for TUICommander

### Parsing Strategy
Gemini CLI is a **CLI inline agent** — changed-rows delta analysis works.
Similar to CC in rendering mechanics (relative cursor positioning).

### Already Supported
- Braille spinner detected by `parse_status_line` via `GEMINI_SPINNER_RE`
- Separator `─────` detected by `is_separator_line`
- Prompt `>` detected by `is_prompt_line`

### Not Yet Supported
- `✦` (U+2726) not in `is_chrome_row` marker set — agent output lines won't be classified
- `▀▀▀` / `▄▄▄` prompt box borders not detected as chrome
- Status bar (bottom 2 rows) has no chrome markers
- Tool call boxes (`╭╮╰╯│`) not classified
- `? for shortcuts` hint line not classified
- No sandbox/permission prompt detection needed (Gemini handles this at model level)

### Potential `chrome.rs` Updates
- Add `✦` (U+2726) to `is_chrome_row` marker set
- Consider `▀` (U+2580) and `▄` (U+2584) half-block detection for prompt box borders
- Status bar detection may need positional awareness (always last 2 rows)
