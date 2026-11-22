# Agent Detection Matrix

Standardized checklist for analyzing and onboarding new AI coding agent CLIs.
Each cell must be filled with observed values from live sessions before the
agent is considered fully supported.

## Mobile question and choice contract (2026-09-29)

The mobile client renders the backend's `SessionState.choice_prompt` and follows
its `selection_mode` for Claude. Codex's existing `Other` notes path still has
a client-side special case. Only rows marked as captured below have a verified
end-to-end choice contract;
unknown forms stay as terminal output until they have a real capture and test.

| Registry type | Observed question or approval UI | Selection contract and evidence |
|---|---|---|
| `claude` | Ink AskUserQuestion: title, numbered options separated by description rows, column-zero `Enter to select` footer | `navigate-enter`: arrows move the highlight, Enter submits, Esc cancels. Claude 2.1.280 capture `src-tauri/src/fixtures/agent_prompts/claude-askuser-esc-20260929.tcap`; the older permission menu is documented in [Claude Code](claude-code.md). |
| `codex` | Queued `request_user_input`: title and numbered choices under `• Queued follow-up inputs`; a separate command approval panel exists | Numbered key for ordinary options; `Other` navigates to notes. Captures and limits in [Codex](codex.md). |
| `agy` | No interactive permission UI in the v0.34.0 workspace-restriction observation | No captured question-option contract; see [Antigravity CLI](agy-cli.md). |
| `opencode` | Framed `△ Permission required` with inline `Allow once / Allow always / Reject` | Navigation and Enter shown in the footer, but no captured mobile option contract. See [OpenCode](opencode.md). |
| `pi` | Permission and menu screens are reported | Specific question rows and selection bytes not captured; see [pi](pi.md). |
| `amp`, `cursor`, `goose`, `grok`, `droid` | No question-option capture in this repository | Unknown; no agent-specific mobile choice parser is enabled. |
| `git`, `api` | Registry execution types, not a captured interactive agent dialog | No choice contract. |

The generic numbered confirmation parser remains available for agents with its
strict title and option shape. The Claude Ink parser is gated by `agent_type`
and the full-bleed footer so a quoted dialog in agent output cannot become a
clickable choice. A missing `choice_prompt` is not evidence that Yes/No is a
safe substitute for a multi-option question.

## Detection Matrix

### 1. Identity & Rendering

| Property | Claude Code | Codex CLI | Antigravity CLI (unverified) | OpenCode |
|----------|------------|-----------|------------|----------|
| **Version tested** | v2.1.81 | v0.116.0+ (re-verified) | v0.34.0 | v1.2.20 |
| **Date tested** | 2026-03-21 | 2026-04-19 | 2026-03-22 | 2026-03-22 |
| **Rendering engine** | Ink (React) | Ink (React) | Ink-like (Node.js) | Bubble Tea (Go) |
| **Cursor positioning** | Relative (`\033[NA]`) | Absolute (`\033[r;cH`) | Relative (`\033[1A]`) | Absolute (`\033[r;cH`) |
| **Scroll mechanism** | `\r\n` padding | Scroll regions (`\033[n;mr]`) | `\r\n` padding | Full-screen redraw |
| **Screen clear on menus** | Sometimes (`\033[2J`) | No | No | Full-screen TUI |
| **Parsing strategy** | Changed-rows delta | Changed-rows delta | Changed-rows delta | Screen snapshot |

### 2. Prompt Line

| Property | Claude Code | Codex CLI | Antigravity CLI (unverified) | OpenCode |
|----------|------------|-----------|------------|----------|
| **Prompt char** | `❯` (U+276F) | `›` (U+203A, bold) | `>` (purple, rgb 215,175,255) | None (framed `┃` box) |
| **Prompt background** | None | Dark gray (rgb 57,57,57) | Dark gray (rgb 65,65,65) | Dark (rgb 30,30,30) |
| **Prompt box border** | `────` separators | Background color only | `▀▀▀` top / `▄▄▄` bottom | `┃╹▀` vertical frame |
| **Ghost text style** | `dim` cell attribute | `\033[2m` dim | Gray (rgb 175,175,175) | Gray placeholder |
| **Multiline input** | Enter = submit | Enter = newline | Enter = submit | Unknown |

### 3. Separator Lines

| Property | Claude Code | Codex CLI | Antigravity CLI (unverified) | OpenCode |
|----------|------------|-----------|------------|----------|
| **Uses separators** | Yes | Partially | Yes | No (uses `┃╹▀`) |
| **Separator chars** | `─` (U+2500) | `─` (U+2500) | `─` (U+2500) | `┃` `╹` `▀` (vertical frame) |
| **Separator color** | Gray (rgb 136,136,136) | Standard | Dark gray (rgb 88,88,88) | N/A |
| **Separator purpose** | Frame prompt box | Between tool output & summary | Above prompt area | Prompt box border |
| **Decorated separators** | Yes (`──── label ──`) | No | No | N/A |
| **Min run length** | 4+ chars | Full width | Full width | N/A |

### 4. Status / Chrome Lines

| Property | Claude Code | Codex CLI | Antigravity CLI (unverified) | OpenCode |
|----------|------------|-----------|------------|----------|
| **Mode line** | `⏵⏵ <mode>` (last row) | None | None | Mode in prompt box (`Build`/`Plan`) |
| **Status line(s)** | 0-N below separator | 1 line below prompt | 2-row status bar (4 columns) | Right panel (context, cost, LSP) |
| **Status indent** | 2 spaces (`\033[2C`) | 2 spaces | 1 space | N/A (panel layout) |
| **Info line** | None | None | `Shift+Tab to accept edits` + MCP/skills count | `tab agents · ctrl+p commands` |
| **Subprocess count** | In mode line | None | None | None (progress bar instead) |

### 5. Spinner / Working Indicators

| Property | Claude Code | Codex CLI | Antigravity CLI (unverified) | OpenCode |
|----------|------------|-----------|------------|----------|
| **Spinner chars** | `✶✻✳✢·` (U+2720-273F) | `•` (U+2022) | `⠋⠙⠹⠸⠴⠦⠧⠇` (braille) | `■⬝` (progress bar) |
| **Spinner color** | White | Standard | Blue/green (varies) | Standard |
| **Spinner position** | Above separator | Inline with output | Below output, above separator | Footer row |
| **Time display** | `(1m 32s)` | `(10s • esc to interrupt)` | `(esc to cancel, Ns)` | None |
| **Token display** | `↓ 2.2k tokens` | None | None | None |
| **Tip text** | Spinner verb names | None | Italic tips during spinner | None |
| **Detected by** | `is_chrome_row` ✓ | `is_chrome_row` ✓ | `parse_status_line` ✓ | `detect_opencode_screen_activity` ✓ (footer `esc interrupt`, not the bar glyphs) |

### 6. Interactive Menus

| Property | Claude Code | Codex CLI | Antigravity CLI (unverified) | OpenCode |
|----------|------------|-----------|------------|----------|
| **Permission prompt** | Multiselect (`❯ 1. Yes`) | Not observed (sandbox) | None (model-level refusal) | `△ Permission required` inline |
| **Selection char** | `❯` (blue) | Not observed | N/A | `⇆ select` |
| **Footer pattern** | `Esc to cancel/close` | `esc to interrupt` | `esc to cancel` (in spinner) | `enter confirm` |
| **OSC 777 notify** | Yes — `needs your permission` (blocked, high-confidence); `is waiting for your input` (blocked picker OR 60s idle timer, low-confidence); `needs your attention` (ignored) | No | No | No |
| **OSC 0 window title** | Yes (task + spinner) | Yes | Yes (`◇ Ready (workspace)`) | No |
| **Slash commands** | `/mcp`, `/stats`, `/status` | `/model`, `/mcp`, `/fast` | `/help`, `/settings`, `/model`, `/stats` | None observed |

### 7. System Messages

| Property | Claude Code | Codex CLI | Antigravity CLI (unverified) | OpenCode |
|----------|------------|-----------|------------|----------|
| **Output prefix** | `⏺` (white/green/red) | `•` (U+2022) | `✦` (U+2726, purple) | None (inline in panel) |
| **Tool call display** | `⏺` + verb | `• Ran <cmd>` (shell), `• Called` + `└ fn()` (MCP) | `╭───╮ ✓ ToolName ╰───╯` box | `→` read / `←` write |
| **Warning prefix** | N/A | `⚠` (U+26A0) | N/A | N/A |
| **Error indicator** | `⏺` (red) | `✗` or `• …hook (failed)` | `✦` + error text | `┃ Error:` inline |
| **Interrupt marker** | N/A | `■` | Not observed | `esc interrupt` hint |
| **Tool result** | `⎿` (U+23BF) | `└` (U+2514) tree connector | Inside `╭───╮` box | `▣` completion marker |
| **Truncation** | `… +N lines` | `… +N lines (ctrl + t to view transcript)` | Not observed | Not observed |

---

## Trigger Procedures

How to force each UI state for analysis and testing.

### Procedure A: Start agent in each permission mode

| Agent | Restricted mode | Permissive mode |
|-------|----------------|-----------------|
| Claude Code | `claude --permission-mode default` | `claude --permission-mode bypassPermissions` |
| Codex CLI | `codex -a untrusted` | `codex` (suggest mode, default) |
| Antigravity CLI (unverified) | `agy` (default) | `agy --sandbox` (unconfirmed) |
| OpenCode | Unknown | Unknown |

### Procedure B: Trigger permission/approval prompt

| Agent | Action | Expected result |
|-------|--------|-----------------|
| Claude Code (default mode) | "create a file /tmp/test.txt with hello" | Multiselect: Yes/Yes+allow/No |
| Codex CLI (untrusted) | Same | Not observed — auto-approves in sandbox |
| Antigravity CLI (unverified) | "create a file /tmp/test.txt with hello" | Text refusal (workspace restriction) |
| OpenCode | Access external directory | `△ Permission required` with `Allow once / Allow always / Reject` |

### Procedure C: Trigger interactive menus

| Agent | Command | Expected result |
|-------|---------|-----------------|
| Claude Code | `/mcp` | Server list with `❯` selection |
| Claude Code | `/stats` | Usage heatmap with date cycling |
| Claude Code | `/status` | Settings panel with search box |
| Codex CLI | `/model` | Model selector |
| Codex CLI | `/mcp` | MCP server list |
| Antigravity CLI (unverified) | `/settings` | Settings panel (unconfirmed) |
| Antigravity CLI (unverified) | `/stats` | Usage stats |

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
