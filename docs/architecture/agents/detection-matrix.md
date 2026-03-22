# Agent Detection Matrix

Standardized checklist for analyzing and onboarding new AI coding agent CLIs.
Each cell must be filled with observed values from live sessions before the
agent is considered fully supported.

## Detection Matrix

### 1. Identity & Rendering

| Property | Claude Code | Codex CLI | Gemini CLI | Aider | OpenCode |
|----------|------------|-----------|------------|-------|----------|
| **Version tested** | v2.1.81 | v0.116.0 | v0.34.0 | v0.86.2 | v1.2.20 |
| **Date tested** | 2026-03-21 | 2026-03-21 | 2026-03-22 | 2026-03-22 | 2026-03-22 |
| **Rendering engine** | Ink (React) | Ink (React) | Ink-like (Node.js) | Python rich + readline | Bubble Tea (Go) |
| **Cursor positioning** | Relative (`\033[NA]`) | Absolute (`\033[r;cH`) | Relative (`\033[1A]`) | Sequential (no cursor) | Absolute (`\033[r;cH`) |
| **Scroll mechanism** | `\r\n` padding | Scroll regions (`\033[n;mr]`) | `\r\n` padding | Normal scroll | Full-screen redraw |
| **Screen clear on menus** | Sometimes (`\033[2J`) | No | No | N/A | Full-screen TUI |
| **Parsing strategy** | Changed-rows delta | Changed-rows delta | Changed-rows delta | Changed-rows delta | Screen snapshot |

### 2. Prompt Line

| Property | Claude Code | Codex CLI | Gemini CLI | Aider | OpenCode |
|----------|------------|-----------|------------|-------|----------|
| **Prompt char** | `❯` (U+276F) | `›` (U+203A, bold) | `>` (purple, rgb 215,175,255) | `>` (green, ANSI #40) | None (framed `┃` box) |
| **Prompt background** | None | Dark gray (rgb 57,57,57) | Dark gray (rgb 65,65,65) | None | Dark (rgb 30,30,30) |
| **Prompt box border** | `────` separators | Background color only | `▀▀▀` top / `▄▄▄` bottom | None | `┃╹▀` vertical frame |
| **Ghost text style** | `dim` cell attribute | `\033[2m` dim | Gray (rgb 175,175,175) | N/A | Gray placeholder |
| **Multiline input** | Enter = submit | Enter = newline | Enter = submit | Enter = submit | Unknown |

### 3. Separator Lines

| Property | Claude Code | Codex CLI | Gemini CLI | Aider | OpenCode |
|----------|------------|-----------|------------|-------|----------|
| **Uses separators** | Yes | Partially | Yes | Yes (green `─────`) | No (uses `┃╹▀`) |
| **Separator chars** | `─` (U+2500) | `─` (U+2500) | `─` (U+2500) | `─` (U+2500) | `┃` `╹` `▀` (vertical frame) |
| **Separator color** | Gray (rgb 136,136,136) | Standard | Dark gray (rgb 88,88,88) | Green (rgb 0,204,0) | N/A |
| **Separator purpose** | Frame prompt box | Between tool output & summary | Above prompt area | Between conversation turns | Prompt box border |
| **Decorated separators** | Yes (`──── label ──`) | No | No | No | N/A |
| **Min run length** | 4+ chars | Full width | Full width | Full width | N/A |

### 4. Status / Chrome Lines

| Property | Claude Code | Codex CLI | Gemini CLI | Aider | OpenCode |
|----------|------------|-----------|------------|-------|----------|
| **Mode line** | `⏵⏵ <mode>` (last row) | None | None | None | Mode in prompt box (`Build`/`Plan`) |
| **Status line(s)** | 0-N below separator | 1 line below prompt | 2-row status bar (4 columns) | Token report after response | Right panel (context, cost, LSP) |
| **Status indent** | 2 spaces (`\033[2C`) | 2 spaces | 1 space | None | N/A (panel layout) |
| **Info line** | None | None | `Shift+Tab to accept edits` + MCP/skills count | None | `tab agents · ctrl+p commands` |
| **Subprocess count** | In mode line | None | None | None | None (progress bar instead) |

### 5. Spinner / Working Indicators

| Property | Claude Code | Codex CLI | Gemini CLI | Aider | OpenCode |
|----------|------------|-----------|------------|-------|----------|
| **Spinner chars** | `✶✻✳✢·` (U+2720-273F) | `•` (U+2022) | `⠋⠙⠹⠸⠴⠦⠧⠇` (braille) | `░█` / `█░` (Knight Rider) | `■⬝` (progress bar) |
| **Spinner color** | White | Standard | Blue/green (varies) | Standard | Standard |
| **Spinner position** | Above separator | Inline with output | Below output, above separator | Inline (backspace overwrite) | Footer row |
| **Time display** | `(1m 32s)` | `(10s • esc to interrupt)` | `(esc to cancel, Ns)` | None | None |
| **Token display** | `↓ 2.2k tokens` | None | None | `Tokens: Nk sent, N received. Cost: $X.XX` | None |
| **Tip text** | Spinner verb names | None | Italic tips during spinner | None | None |
| **Detected by** | `is_chrome_row` ✓ | `is_chrome_row` ✓ | `parse_status_line` ✓ | `parse_status_line` ✓ | N/A (full TUI) |

### 6. Interactive Menus

| Property | Claude Code | Codex CLI | Gemini CLI | Aider | OpenCode |
|----------|------------|-----------|------------|-------|----------|
| **Permission prompt** | Multiselect (`❯ 1. Yes`) | Not observed (sandbox) | None (model-level refusal) | File add: `Y/N/A/S/D` | `△ Permission required` inline |
| **Selection char** | `❯` (blue) | Not observed | N/A | N/A | `⇆ select` |
| **Footer pattern** | `Esc to cancel/close` | `esc to interrupt` | `esc to cancel` (in spinner) | None | `enter confirm` |
| **OSC 777 notify** | Yes | No | No | No | No |
| **OSC 0 window title** | Yes (task + spinner) | Yes | Yes (`◇ Ready (workspace)`) | No | No |
| **Slash commands** | `/mcp`, `/stats`, `/status` | `/model`, `/mcp`, `/fast` | `/help`, `/settings`, `/model`, `/stats` | `/help` | None observed |

### 7. System Messages

| Property | Claude Code | Codex CLI | Gemini CLI | Aider | OpenCode |
|----------|------------|-----------|------------|-------|----------|
| **Output prefix** | `⏺` (white/green/red) | `•` (U+2022) | `✦` (U+2726, purple) | None (blue text) | None (inline in panel) |
| **Tool call display** | `⏺` + verb | `•` + description | `╭───╮ ✓ ToolName ╰───╯` box | None | `→` read / `←` write |
| **Warning prefix** | N/A | `⚠` (U+26A0) | N/A | Orange text | N/A |
| **Error indicator** | `⏺` (red) | `✗` | `✦` + error text | Red text | `┃ Error:` inline |
| **Interrupt marker** | N/A | `■` | Not observed | `^C` | `esc interrupt` hint |
| **Tool result** | `⎿` (U+23BF) | `└` or inline | Inside `╭───╮` box | Inline | `▣` completion marker |

---

## Trigger Procedures

How to force each UI state for analysis and testing.

### Procedure A: Start agent in each permission mode

| Agent | Restricted mode | Permissive mode |
|-------|----------------|-----------------|
| Claude Code | `claude --permission-mode default` | `claude --permission-mode bypassPermissions` |
| Codex CLI | `codex -a untrusted` | `codex` (suggest mode, default) |
| Gemini CLI | `gemini` (default, workspace-restricted) | `gemini --sandbox=false` (unconfirmed) |
| Aider | N/A (no sandbox) | N/A |
| OpenCode | Unknown | Unknown |

### Procedure B: Trigger permission/approval prompt

| Agent | Action | Expected result |
|-------|--------|-----------------|
| Claude Code (default mode) | "create a file /tmp/test.txt with hello" | Multiselect: Yes/Yes+allow/No |
| Codex CLI (untrusted) | Same | Not observed — auto-approves in sandbox |
| Gemini CLI | "create a file /tmp/test.txt with hello" | Text refusal (workspace restriction) |
| Aider | Open file not in chat | `Add file to the chat? (Y)es/(N)o/(A)ll/(S)kip all/(D)on't ask again` |
| OpenCode | Access external directory | `△ Permission required` with `Allow once / Allow always / Reject` |

### Procedure C: Trigger interactive menus

| Agent | Command | Expected result |
|-------|---------|-----------------|
| Claude Code | `/mcp` | Server list with `❯` selection |
| Claude Code | `/stats` | Usage heatmap with date cycling |
| Claude Code | `/status` | Settings panel with search box |
| Codex CLI | `/model` | Model selector |
| Codex CLI | `/mcp` | MCP server list |
| Gemini CLI | `/settings` | Settings panel (unconfirmed) |
| Gemini CLI | `/stats` | Usage stats |

### Procedure D: Observe working state

| Agent | Action | What to capture |
|-------|--------|-----------------|
| Any | Send a complex multi-tool task | Spinner animation, cursor-up distance |
| Any | Send task during active subprocess | Subprocess count display |
| Any | Press Escape during work | Interrupt marker |

### Procedure E: Capture raw ANSI

For each state above:
```
session action=output session_id=<id> limit=8000 format=raw
```

Look for:
- Cursor positioning: `\033[NA]` (relative up), `\033[r;cH` (absolute)
- Colors: `\033[38;2;R;G;Bm` (RGB foreground)
- Background: `\033[48;2;R;G;Bm`
- Screen clear: `\033[2J`
- Scroll regions: `\033[n;mr`
- OSC sequences: `\033]777;...`, `\033]0;...`, `\033]8;...`

---

## Onboarding a New Agent

1. Fill the detection matrix columns by running procedures A-E
2. Create `docs/architecture/agents/<name>.md` with observed layouts
3. Update `chrome.rs` if new markers/chars are needed
4. Add test cases from real captured text
5. Run `/agent-ui-audit` skill to verify parser compatibility
