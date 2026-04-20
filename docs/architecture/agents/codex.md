# Codex CLI — UI Layout Reference

Agent-specific layout reference for Codex CLI (OpenAI).
See [agent-ui-analysis.md](../agent-ui-analysis.md) for shared concepts.

**Observed version**: v0.116.0 → latest (2026-04-19)
**Rendering engine**: Ink (React for terminals)
**Rendering approach**: ANSI absolute positioning (`\033[row;colH`) + scroll regions

---

## Layout Anatomy

Codex uses a fundamentally different approach from Claude Code:
- **No separator-framed prompt box** — uses background color instead
- **Absolute cursor positioning** — `\033[12;2H` (row 12, col 2)
- **Terminal scroll regions** — `\033[12;41r` to define scrollable content area
- **Reverse index** — `\033M` to scroll content upward

```
[agent output with • bullet prefix]
[empty line]
 ← dark background (rgb 57,57,57) starts here
› [user input]                       (prompt, bold ›, dark bg)
                                     (dark bg continues)
                                     (dark bg continues)
  gpt-5.4 high · 100% left · ~/project   (status line, dim, normal bg)
```

---

## Real-world Examples (live session, 2026-03-21)

### Startup banner
```
╭────────────────────────────────────────────╮
│ >_ OpenAI Codex (v0.116.0)                 │
│                                            │
│ model:     gpt-5.4 high   /model to change │
│ directory: ~/Gits/personal/tuicommander    │
╰────────────────────────────────────────────╯
  Tip: Use /mcp to list configured MCP tools.
```

### Idle (waiting for input)
```
› Summarize recent commits           (ghost text, dim)
  gpt-5.4 high · 100% left · ~/Gits/personal/tuicommander
```

### After tool execution (simple file create)
```
› create a file called /tmp/codex-test.txt with "hello"
• Creating /tmp/codex-test.txt with the requested contents.
• Added /tmp/codex-test.txt (+1 -0)
    1 +hello
───────────────────────────────────────────────────────────────────────────────
• Created /tmp/codex-test.txt with hello.
› Summarize recent commits
  gpt-5.4 high · 98% left · ~/Gits/personal/tuicommander
```

### After tool execution (shell commands + MCP calls, 2026-04-19)
```
• PreToolUse hook (failed)
  error: hook exited with code 127
• Ran git status --short
  └ ?? .gitignore
    ?? .serena/
    … +2 lines (ctrl + t to view transcript)
    ?? StepsWidgetDemo/
    ?? project.yml
• PostToolUse hook (failed)
  error: hook exited with code 127
• Ran xcodegen generate
  └ ⚙️  Generating plists...
    ⚙️  Generating project...
    ⚙️  Writing project...
    Created project at /Users/.../StepsWidgetDemo.xcodeproj
• Ran xcodebuild -project StepsWidgetDemo.xcodeproj -scheme StepsWidgetDemo ...
  └ 2026-04-19 22:27:42.803 xcodebuild[67191:4346310]  DVTFilePathFSEvents: ...
    … +109 lines (ctrl + t to view transcript)
    ** BUILD SUCCEEDED **
• Waited for background terminal
──────────────────────────────────────────────────────────────────────────────
• Il progetto compila. Prima di chiudere salvo ...
• Called
  └ serena.write_memory({"memory_name":"project_overview","content":"..."})
    Memory project_overview written.
• Working (4m 55s • esc to interrupt)
› Improve documentation in @filename
  gpt-5.4 high · ~/Gits/personal/steps
```

**Tool display patterns (v0.116.0+):**

| Pattern | Meaning |
|---------|---------|
| `• Ran <command>` | Shell command execution |
| `• Called` + `└ <fn>(...)` | MCP/function call with args on next line |
| `• Added <path> (+N -M)` | File created/modified with diff stats |
| `• Creating <path>` | File operation in progress |
| `• Waited for background terminal` | Background job completed |
| `• PreToolUse hook (failed)` | Hook error (with `error:` detail below) |
| `• PostToolUse hook (failed)` | Hook error (with `error:` detail below) |
| `… +N lines (ctrl + t to view transcript)` | Truncated output (N lines hidden) |
| `└` (U+2514) | Tree connector for tool output/results |

### After interrupt (Escape)
```
■ Conversation interrupted - tell the model what to do differently.
› Summarize recent commits
  esc again to edit previous message
```

---

## Key Differences from Claude Code

| Feature | Claude Code | Codex CLI |
|---------|------------|-----------|
| Prompt char | `❯` (U+276F) | `›` (U+203A, bold) |
| Prompt box | Separator-framed (`────`) | Background color (rgb 57,57,57) |
| Cursor positioning | Relative (`\033[8A`) | Absolute (`\033[12;2H`) |
| Scrolling | `\r\n` padding | Scroll regions (`\033[12;41r`) + reverse index (`\033M`) |
| Status line | Multi-line, indented 2sp | Single line, indented 2sp, dim |
| Mode line | `⏵⏵ bypass permissions on` etc. | None observed |
| Separator usage | Around prompt box | Between tool output and summary |
| System messages | `⏺` prefix (white/green/red) | `•` prefix (bullet) |
| Warnings | N/A | `⚠` prefix (yellow) |
| Interrupt marker | N/A | `■` prefix |
| Ghost text | Via `dim` cell attribute | Via `\033[2m` dim |
| Submit key | Enter | Enter (but multiline: Enter = newline in prompt) |

---

## Prompt Line

- Character: `›` (U+203A) — bold `\033[1m`
- Background: dark gray `rgb(57,57,57)` — `\033[48;2;57;57;57m`
- The prompt area spans multiple rows with dark background
- Ghost text (placeholder) shown in dim: `\033[2m`

**Multiline input**: Codex supports multiline prompts where Enter adds a
newline. Submit is also Enter (single line). This makes programmatic input
tricky — sending text + Enter may add a newline instead of submitting.

---

## Status Line

Single line below the prompt area, always present:

```
  gpt-5.4 high · 100% left · ~/Gits/personal/tuicommander
  gpt-5.4 high · ~/Gits/personal/steps
```

Format: `  <model> <effort> · [<quota>% left ·] <directory>`

The quota segment (`N% left`) is optional — observed absent in some sessions
(possibly when quota is unlimited or when using API keys without usage tracking).

Rendered in dim (`\033[2m`) with normal background (not dark bg).

---

## Separator Usage

Codex uses `────` separators differently from CC — they appear **between
tool output and the agent's summary response**, not as a prompt box frame:

```
• Added /tmp/codex-test.txt (+1 -0)
    1 +hello
───────────────────────────────────────────────────────────────────────────
• Created /tmp/codex-test.txt with hello.
```

---

## Spinner

Codex uses `•` (U+2022) as a spinner/working indicator:

```
• Working (10s • esc to interrupt)
```

The `•` is already in `is_chrome_row`'s marker set.

---

## OSC Sequences

```
\033]10;?\033\\    — query terminal foreground color
\033]11;?\033\\    — query terminal background color
\033]0;...\007     — window title updates
```

No `\033]777;notify;` observed — Codex does not emit terminal notifications
for approval prompts.

---

## Approval Modes

CLI flag: `-a` or `--ask-for-approval <POLICY>`

| Policy | Behavior |
|--------|----------|
| `untrusted` | Sandbox commands (does NOT prompt for approval) |
| `on-failure` | DEPRECATED — auto-run, ask only on failure |
| `on-request` | Model decides when to ask |
| `never` | Never ask |

**Note**: In `untrusted` mode, Codex auto-approves tool use within the
sandbox. No interactive approval prompt was observed. The approval UI
may only appear in specific edge cases or with `on-request` mode.

---

## Slash Commands Observed

```
• Unrecognized command '/mode'. Type "/" for a list of supported commands.
```

Available: `/model`, `/mcp`, `/fast`, `/feedback`, `/help`, and others.
Not observed: `/stats`, `/status` (CC-specific).

---

## Known Issues

### Enter key handling

Codex likely uses the kitty keyboard protocol to distinguish Enter
(submit) from Enter (newline in multiline prompt). The TUICommander
`session action=input special_key=enter` sends `\r` which Codex may
interpret as newline. Workaround: send text and Enter in separate
calls, but this is unreliable for multiline content.

### ask_user_question tool (proposed)

GitHub issue [openai/codex#9926](https://github.com/openai/codex/issues/9926)
proposes a tabbed questionnaire UI similar to Claude Code's skill menus.
Currently available via `request_user_input` tool with `collaboration_modes = true`
in config, but only in plan mode (Shift+Tab).

---

## Rendering Mechanics (raw ANSI)

### Absolute positioning
```
\033[12;2H     — cursor to row 12, col 2 (absolute)
\033[K         — erase to end of line
```

### Scroll regions
```
\033[12;41r    — set scroll region rows 12-41
\033M          — reverse index (scroll content up within region)
\033[r         — reset scroll region
```

### Prompt area rendering
```
\033[48;2;57;57;57m  — dark background starts
\033[1m›\033[22m     — bold › then unbold
\033[2m...          — dim ghost text
\033[49m             — background reset for status line
```

Unlike CC which uses relative cursor movement (`\033[8A`), Codex uses
absolute positioning. This means changed_rows detection works differently —
Codex updates specific rows by address rather than painting top-down.
