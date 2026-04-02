# AI Agents

TUICommander detects, monitors, and manages AI coding agents running in your terminals.

## Supported Agents

| Agent | Binary | Resume Command | Session Binding |
|-------|--------|----------------|-----------------|
| Claude Code | `claude` | `claude --continue` | `claude --resume $TUIC_SESSION` |
| Codex CLI | `codex` | `codex resume --last` | `codex resume $TUIC_SESSION` |
| Aider | `aider` | `aider --restore-chat-history` | — |
| Gemini CLI | `gemini` | `gemini --resume` | `gemini --resume $TUIC_SESSION` |
| OpenCode | `opencode` | `opencode -c` | — |
| Amp | `amp` | `amp threads continue` | — |
| Cursor Agent | `cursor-agent` | `cursor-agent resume` | — |
| Warp Oz | `oz` | — | — |
| Droid (Factory) | `droid` | — | — |

## Agent Detection

TUICommander auto-detects which agent is running in each terminal by matching output patterns. Detection uses agent-specific status line markers:

- **Claude Code**: Middle dot `·` (U+00B7), dingbat asterisks `✢` `✳` `✶` `✻` `✽` (U+2720–273F), or ASCII `*`
- **Copilot CLI**: Therefore sign `∴` (U+2234), filled circle `●` (U+25CF), empty circle `○` (U+25CB)
- **Aider**: Knight Rider scanner blocks `░█`
- **Gemini CLI / Amazon Q / Cline**: Braille spinners `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`
- **Codex CLI**: Bullets `•` `◦`

When detected:

- The **status bar** shows the agent's brand logo and name
- The **tab indicator** updates to reflect agent state
- Rate limit and question detection activate for that provider's patterns

Binary detection uses `resolve_cli()` — Rust probes well-known directories so agents are found even in release builds where the user's shell PATH isn't available.

## Rate Limit Detection

When an agent hits a rate limit, TUICommander detects it from terminal output:

- **Status bar warning** — Shows a badge with the number of rate-limited sessions and a countdown timer
- **Per-session tracking** — Each session's rate limit is tracked independently with automatic cleanup when expired
- **Provider-specific patterns** — Custom regex for Claude ("overloaded", "rate limit"), Gemini ("429", "quota exceeded"), OpenAI ("too many requests"), and generic patterns

## Question Detection

When an agent asks an interactive question (Y/N, multiple choice, numbered options), TUICommander:

1. Changes the **tab indicator** to a `?` icon
2. Shows a **prompt overlay** with keyboard navigation:
   - `↑/↓` to navigate options
   - `Enter` to select
   - Number keys `1-9` for numbered options
   - `Escape` to dismiss
3. Plays a **notification sound** (if enabled in Settings → Notifications)

For unrecognized agents, silence-based detection kicks in — if the terminal stops producing output for 10 seconds after a line ending with `?`, it's treated as a potential prompt. User-typed lines ending with `?` are suppressed from question detection for 500ms (echo window) to avoid false positives from PTY echo.

## Usage Limit Tracking

For Claude Code, TUICommander detects weekly and session usage limit messages from terminal output:

- **Unified agent badge** — When Claude is the active agent, the status bar shows a single badge combining the agent icon with usage data. The badge displays rate limit countdowns (when rate-limited), Claude Usage API data (5h/7d utilization percentages), or terminal-detected usage limits, in that priority order.
  - Blue: < 70% utilization
  - Yellow: 70–89%
  - Red (pulsing): >= 90%
- Clicking the badge opens the Claude Usage Dashboard.

This helps you pace your usage across the week.

## Claude Usage Dashboard

A native feature (not a plugin) that provides detailed analytics for your Claude Code usage. Enable it in **Settings** > **Agents** > expand **Claude Code** > **Features** > **Usage Dashboard**.

When enabled, TUICommander polls the Claude API every 5 minutes and shows:

- **Rate limits** — 5-hour and 7-day utilization bars with reset countdowns. Color-coded: green (OK), yellow (70%+), red (90%+).
- **Usage Over Time** — 7-day token usage chart (input vs. output tokens) with hover tooltips.
- **Insights** — Session count, message counts, input/output/cache token totals.
- **Activity heatmap** — 52-week GitHub-style heatmap of daily message counts with per-project drill-down on hover.
- **Model usage** — Breakdown by model (messages, input, output, cache created, cache read).
- **Per-project breakdown** — All projects ranked by token usage. Click a project to filter the dashboard to that project.

The dashboard opens as a tab in the Activity Center. You can also reach it by clicking the Claude usage badge in the status bar.

## Agent Teams

Agent Teams lets Claude Code spawn teammate agents as TUIC terminal tabs. Enable it in **Settings** > **Agents** > **Agent Teams**.

When enabled, PTY sessions receive the `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1` environment variable, which unlocks Claude Code's `TeamCreate`, `TaskCreate`, and `SendMessage` tools. Agent spawning uses direct MCP tool calls (`agent spawn`) — the earlier it2 shim approach (iTerm2 CLI emulation) is deprecated.

Spawned sessions automatically emit lifecycle events (`session-created`, `session-closed`) so they appear as tabs and clean up on exit.

## Session Binding (TUIC_SESSION)

Every terminal tab has a stable UUID that persists across app restarts. This UUID is injected into the PTY shell as the `TUIC_SESSION` environment variable.

### How It Works

1. When a terminal tab is created, a UUID is generated via `crypto.randomUUID()`
2. The UUID is saved with the tab and restored when the app restarts
3. On PTY creation, the UUID is injected as `TUIC_SESSION=<uuid>` in the shell environment
4. Agents can use `$TUIC_SESSION` for session-specific operations

### Use Cases

**Start a Claude Code session bound to this tab:**

```bash
claude --session-id $TUIC_SESSION
```

Claude Code stores the session locally. When you restart TUICommander and switch to this branch, the session resumes automatically via `claude --resume <uuid>`.

**Resume a specific session (manual):**

```bash
claude --resume $TUIC_SESSION
```

**Gemini CLI session binding:**

```bash
gemini --resume $TUIC_SESSION
```

**Custom scripts that persist state per-tab:**

```bash
# Use TUIC_SESSION as a stable key for any tab-specific state
echo "Last run: $(date)" > "/tmp/tuic-$TUIC_SESSION.log"
```

### Automatic Resume

When TUICommander restores saved terminals after a restart, it checks whether the agent session file exists on disk before deciding the resume strategy:

1. **Verified session** — If `$TUIC_SESSION` maps to an existing session file (e.g. `~/.claude/projects/…/<uuid>.jsonl`), the agent resumes with `--resume <uuid>`
2. **No session file** — Falls back to the agent's default resume behavior (e.g. `claude --continue` for the last session)
3. **No agent detected** — Tab opens a plain shell; `$TUIC_SESSION` is still available for manual use

### UI Agent Spawn

When you spawn an agent via the context menu or command palette, TUICommander automatically uses the tab's `TUIC_SESSION` as the `--session-id`. This ensures the spawned session is bound to the tab and will resume correctly on restart.

## Sleep Prevention

When agents are actively working, TUICommander can keep your machine awake:

- Enable in **Settings** → **General** → **Prevent sleep when busy**
- Uses the `keepawake` system integration
- Automatically releases when all agents are idle

## Environment Flags

Per-agent environment variables can be injected into every new terminal session. Configure in Settings > Agents > expand an agent > Environment Flags.

This is useful for enabling feature flags (e.g., `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`) without manually running `export` commands. Flags are organized by category with toggle, enum, and number types.

## Tips

- **Multiple agents on the same repo** — Use split panes (`Cmd+\`) to run two agents side by side on the same branch
- **Different agents per branch** — Each worktree is independent, so you can run Claude on one branch and Aider on another
- **Monitor all at once** — Use the Activity Dashboard (`Cmd+Shift+A`) to see every terminal's agent status in one view
