# Agent Detection Matrix

Standardized checklist for analyzing and onboarding new AI coding agent CLIs.
Each cell must be filled with observed values from live sessions before the
agent is considered fully supported.

## Detection Matrix

### 1. Identity & Rendering

| Property | Claude Code | Codex CLI | Gemini CLI | Aider | _New Agent_ |
|----------|------------|-----------|------------|-------|-------------|
| **Version tested** | v2.1.81 | v0.116.0 | — | — | v1.2.20 |
| **Date tested** | 2026-03-21 | 2026-03-21 | — | — | 2026-03-22 |
| **Rendering engine** | Ink (React) | Ink (React) | ? | stdin/stdout | Bubble Tea (Go) |
| **Cursor positioning** | Relative (`\033[NA]`) | Absolute (`\033[r;cH`) | ? | Sequential | Absolute (`\033[r;cH`) |
| **Scroll mechanism** | `\r\n` padding | Scroll regions (`\033[n;mr]`) | ? | Normal scroll | Full-screen redraw |
| **Screen clear on menus** | Sometimes (`\033[2J`) | No | ? | N/A | Full-screen TUI |

### 2. Prompt Line

| Property | Claude Code | Codex CLI | Gemini CLI | Aider | _New Agent_ |
|----------|------------|-----------|------------|-------|-------------|
| **Prompt char** | `❯` (U+276F) | `›` (U+203A, bold) | `>` | `>` | None (framed box) |
| **Prompt background** | None | Dark gray (rgb 57,57,57) | None | None | Dark (rgb 30,30,30) |
| **Ghost text style** | `dim` cell attribute | `\033[2m` dim | ? | N/A | Gray placeholder |
| **Multiline input** | Enter = submit | Enter = newline | ? | Enter = submit | ? |

### 3. Separator Lines

| Property | Claude Code | Codex CLI | Gemini CLI | Aider | _New Agent_ |
|----------|------------|-----------|------------|-------|-------------|
| **Uses separators** | Yes | Partially | No | No | No (uses `┃╹▀`) |
| **Separator chars** | `─` (U+2500) | `─` (U+2500) | — | — | `┃` `╹` `▀` (vertical frame) |
| **Separator purpose** | Frame prompt box | Between tool output & summary | — | — | Prompt box border |
| **Decorated separators** | Yes (`──── label ──`) | No | — | — | N/A |
| **Min run length** | 4+ chars | Full width | — | — | N/A |

### 4. Status / Chrome Lines

| Property | Claude Code | Codex CLI | Gemini CLI | Aider | _New Agent_ |
|----------|------------|-----------|------------|-------|-------------|
| **Mode line** | `⏵⏵ <mode>` (last row) | None | None | N/A | |
| **Status line(s)** | 0-N below separator | 1 line below prompt | Below prompt | N/A | |
| **Status indent** | 2 spaces (`\033[2C`) | 2 spaces | ? | N/A | |
| **Subprocess count** | In mode line | None | None | N/A | |

### 5. Spinner / Working Indicators

| Property | Claude Code | Codex CLI | Gemini CLI | Aider | _New Agent_ |
|----------|------------|-----------|------------|-------|-------------|
| **Spinner chars** | `✶✻✳✢·` (U+2720-273F) | `•` (U+2022) | `⠋⠙⠹...` (braille) | `░█` | ? (full-screen) |
| **Spinner position** | Above separator | Inline with output | Below prompt | Inline | ? (in panel) |
| **Time display** | `(1m 32s)` | `(10s • esc to interrupt)` | ? | N/A | ? |
| **Token display** | `↓ 2.2k tokens` | None | ? | `Tokens: N sent, N received.` | ? |
| **Detected by** | `is_chrome_row` ✓ | `is_chrome_row` ✓ | `parse_status_line` ✓ | `parse_status_line` ✓ | N/A (full TUI) |

### 6. Interactive Menus

| Property | Claude Code | Codex CLI | Gemini CLI | Aider | _New Agent_ |
|----------|------------|-----------|------------|-------|-------------|
| **Permission prompt** | Multiselect (`❯ 1. Yes`) | Not observed | ? | N/A | |
| **Selection char** | `❯` (blue) | ? | ? | N/A | |
| **Footer pattern** | `Esc to cancel/close` | `esc to interrupt` | ? | N/A | |
| **OSC 777 notify** | Yes | No | ? | No | |
| **Slash commands** | `/mcp`, `/stats`, `/status` | `/model`, `/mcp`, `/fast` | ? | `/help` | |

### 7. System Messages

| Property | Claude Code | Codex CLI | Gemini CLI | Aider | _New Agent_ |
|----------|------------|-----------|------------|-------|-------------|
| **Output prefix** | `⏺` (white/green/red) | `•` (U+2022) | None | None | |
| **Warning prefix** | N/A | `⚠` (U+26A0) | ? | N/A | |
| **Error indicator** | `⏺` (red) | `✗` ? | ? | Error text | |
| **Interrupt marker** | N/A | `■` | ? | `^C` | |
| **Tool result** | `⎿` (U+23BF) | `└` or inline | ? | Inline | |

---

## Trigger Procedures

How to force each UI state for analysis and testing.

### Procedure A: Start agent in each permission mode

| Agent | Restricted mode | Permissive mode |
|-------|----------------|-----------------|
| Claude Code | `claude --permission-mode default` | `claude --permission-mode bypassPermissions` |
| Codex CLI | `codex -a untrusted` | `codex` (suggest mode, default) |
| Gemini CLI | ? | ? |
| Aider | N/A (no sandbox) | N/A |

### Procedure B: Trigger permission/approval prompt

| Agent | Action | Expected result |
|-------|--------|-----------------|
| Claude Code (default mode) | "create a file /tmp/test.txt with hello" | Multiselect: Yes/Yes+allow/No |
| Codex CLI (untrusted) | Same | Not observed — auto-approves in sandbox |
| Codex CLI (on-request) | Complex task | Model decides to ask |

### Procedure C: Trigger interactive menus

| Agent | Command | Expected result |
|-------|---------|-----------------|
| Claude Code | `/mcp` | Server list with `❯` selection |
| Claude Code | `/stats` | Usage heatmap with date cycling |
| Claude Code | `/status` | Settings panel with search box |
| Codex CLI | `/model` | Model selector |
| Codex CLI | `/mcp` | MCP server list |

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
