# OpenCode — UI Layout Reference

Agent-specific layout reference for OpenCode.
See [agent-ui-analysis.md](../agent-ui-analysis.md) for shared concepts.

**Observed version**: v1.2.20 (2026-03-22)
**Rendering engine**: Bubble Tea (Go TUI framework)
**Rendering approach**: Full-screen TUI, ANSI absolute positioning, mouse tracking

---

## Key Difference from Other Agents

OpenCode is a **full-screen TUI application**, not a CLI that renders inline
in the terminal like Claude Code or Codex. It takes over the entire terminal
screen with its own layout, panels, and navigation. This means:

- The "bottom zone" concept from CC/Codex does not directly apply
- OpenCode manages its own screen regions (panels, status bar, prompt)
- Mouse tracking is enabled (`\033[?1000h`, `\033[?1003h`, `\033[?1006h`)
- It uses bracketed paste (`\033[?2004h`) and focus events (`\033[?1004h`)

---

## Observed States

### Welcome Screen

Only shown on fresh start, before first message:

```
                                                     █▀▀█ █▀▀█ █▀▀█ █▀▀▄ █▀▀▀ █▀▀█ █▀▀█ █▀▀█
                                                     █  █ █  █ █▀▀▀ █  █ █    █  █ █  █ █▀▀▀
                                                     ▀▀▀▀ █▀▀▀ ▀▀▀▀ ▀▀▀▀ ▀▀▀▀ ▀▀▀▀ ▀▀▀▀ ▀▀▀▀
                                   ┃
                                   ┃  Ask anything... "What is the tech stack of this project?"
                                   ┃
                                   ┃  Build  Claude Sonnet 4.5 lansweeper.ai
                                   ╹▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀
                                                                          tab agents  ctrl+p commands
                                         ● Tip Set agent temperature from 0.0 (focused) to 1.0 (creative)
  ~/Gits/personal/tuicommander:main                                                          1.2.20
```

### Conversation (after first message) — two-panel layout

```
  ┃  what version is this project                                           █  Project version inquiry
  ┃                                                                         █
                                                                            █  Context
     Leggo la versione del progetto...                                      █  20,192 tokens
                                                                            █  0% used
     → Read SPEC.md [limit=50]                                              █  $0.00 spent
     → Read package.json [limit=20]                                         █
     → Read src-tauri/tauri.conf.json [limit=30]                            █  LSP
                                                                            █  LSPs will activate...
     Questo progetto è alla versione 0.9.5...                               █
                                                                            █
     ▣  Build · anthropic/claude-sonnet-4.5 · 10.2s                         █
                                                                            █
  ┃
  ┃  Build  Claude Sonnet 4.5 lansweeper.ai          ~/path:main
  ╹▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀
                                          tab agents  ctrl+p commands    • OpenCode 1.2.20
```

Key elements:
- **Left panel**: conversation (messages, tool calls, results)
- **Right panel**: sidebar with `█` border — title, context tokens, cost, LSP info
- **Prompt box**: bottom, framed with `┃╹▀`
- **Model info**: inside prompt box `Build  Claude Sonnet 4.5 lansweeper.ai`

### Permission Prompt

```
  ┃  △ Permission required
  ┃    ← Access external directory /tmp
  ┃
  ┃  Patterns
  ┃
  ┃  - /tmp/*
  ┃
  ┃                                                              ~/path:main
  ┃   Allow once   Allow always   Reject   ctrl+f fullscreen  ⇆ select  enter confirm
  ┃                                                              • OpenCode 1.2.20
```

### Working State (during tool execution)

Footer changes to show progress bar and interrupt hint:

```
   ■■⬝⬝⬝⬝⬝⬝  esc interrupt                            tab agents  ctrl+p commands    • OpenCode 1.2.20
```

- `■` (U+25A0) — completed steps
- `⬝` (U+2B1D) — remaining steps
- Mode label may switch: `Build` → `Plan` in prompt box

### Error State

```
  ┃  Error: Unable to connect. Is the computer able to access the url?
```

Errors displayed inline in the `┃` frame, same area as conversation.

---

Key elements:
- **`△` (U+25B3)**: permission required marker
- **`←`**: tool call prefix (Write direction)
- **3 inline options**: `Allow once   Allow always   Reject` — not numbered, not multiselect
- **Footer**: `ctrl+f fullscreen  ⇆ select  enter confirm`
- **`⇆` (U+21C6)**: select/navigate hint
- **Pattern display**: shows glob pattern (`/tmp/*`)
- **Entire dialog inside `┃` frame** — prompt box expands to contain it

---

## UI Element Reference

### Prompt Frame
- Left border: `┃` (U+2503, heavy vertical)
- Bottom border: `╹▀▀▀▀...` (U+2579 corner + U+2580 upper half blocks)
- No `❯`, `›`, or `>` prompt char
- Model info inline: `Build  Claude Sonnet 4.5 lansweeper.ai`

### Right Panel Border
- `█` (U+2588, full block) — vertical border for sidebar
- `▄` (U+2584, lower half block) — top corner of sidebar

### Tool Call Prefixes
- `→` — Read operations (files read by the agent)
- `←` — Write operations (files written/modified by the agent)

### Completion Marker
- `▣` (U+25A3, white square with rounded corners) — marks completed tool calls
- Format: `▣  Build · anthropic/claude-sonnet-4.5 · 10.2s`

### Tips
- `●` (U+25CF) — orange marker (rgb 245,167,66)
- Format: `● Tip <highlighted_word> <gray description>`

### Status Bar
- Left: `~/Gits/personal/tuicommander:main` (path + branch)
- Right: `1.2.20` or `• OpenCode 1.2.20`

### Navigation Hints
- `tab agents` — switch to agents panel
- `ctrl+p commands` — command palette
- `ctrl+f fullscreen` — toggle fullscreen (in permission dialog)
- `⇆ select` — select between options
- `enter confirm` — confirm selection

---

## Rendering Mechanics

### Full-screen with background
```
\033[48;2;10;10;10m    — near-black background fills entire screen
\033[48;2;30;30;30m    — slightly lighter for input box
```

### Absolute cursor positioning
```
\033[29;42H            — cursor to row 29, col 42
```

### Mouse tracking (enabled on startup)
```
\033[?1000h   — normal mouse tracking
\033[?1002h   — button-event tracking
\033[?1003h   — all-motion tracking
\033[?1006h   — SGR mouse mode
\033[?1004h   — focus events
```

### Kitty keyboard protocol
```
\033[?2026h / \033[?2026l   — toggled very frequently (polling pattern)
```

### Cursor
```
\033[1 q      — blinking block cursor
\033[?25h     — show cursor
\033[?25l     — hide cursor (during redraws)
```

### Color palette queries (on startup)
```
\033]4;0;?\007 through \033]4;15;?\007    — all 16 ANSI palette colors
\033]10;?\007 through \033]19;?\007       — foreground, background, etc.
```

---

## Implications for TUICommander

### Chrome Detection
OpenCode is a full-screen TUI — every row changes on every update. The
`changed_rows` / `is_chrome_row` approach does not work. Needs:
- Full-screen TUI detection mode (mouse tracking + full background = TUI)
- Screen-snapshot-based parsing instead of changed-row delta analysis

### Prompt Detection
No standard prompt char (`❯`, `›`, `>`). Would need to detect `┃` frame
or input box background color change.

### Permission Detection
- `△ Permission required` is a unique text signal
- `Allow once   Allow always   Reject` footer is unique to OpenCode
- No OSC 777 notifications observed

### Subtask / Subprocess Count
**None.** OpenCode does not expose subprocess/subtask counts. Instead:
- Tool calls shown inline in conversation panel (`→ Read`, `← Write`)
- Progress bar in footer: `⬝■■■■■■⬝` (filled/empty squares)
- Completion marker: `▣  Build · model · time`

### Working State
- **Progress bar**: `■■⬝⬝⬝⬝⬝⬝` in footer row — graphical, not numeric
- **Mode label**: changes from `Build` to `Plan` in prompt box
- **Interrupt hint**: `esc interrupt` in footer during work
- **No spinner chars** — uses progress bar instead

### Error Display
Errors shown inline in the `┃` frame:
```
  ┃  Error: Unable to connect. Is the computer able to access the url?
```

### Agent Identification
OpenCode can be detected by:
- ASCII art banner with `OPENCODE` text on first screen
- `┃╹▀` vertical frame chars
- Mouse tracking enabled on startup
- `• OpenCode X.Y.Z` in status bar
