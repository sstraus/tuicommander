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
| Droid (Factory) | `droid` | — | — |
| Goose | `goose` | `goose session --resume` | `goose session --resume --name $TUIC_SESSION` |
| Grok | `grok` | `grok --continue` | `grok --resume <discovered id>` |
| pi | `pi` | `pi --continue` | — |

### Claude MCP profiles

At startup, the primary TUICommander instance repairs its MCP bridge in the
default Claude config, an existing `~/.claude-private/.claude.json`, and profiles
selected by inherited `CLAUDE_CONFIG_DIR` or Claude agent/run-config environment
settings. Use absolute profile paths (or `~/…`). Custom remote transports and
disabled integrations are preserved. A project `.mcp.json` entry is unnecessary
when the user profile already contains TUICommander; remove obsolete duplicate
entries pointing into a build target directory. Startup logs name any configured
absolute bridge path that is missing or not executable.

### Native scrollback on launch

**Prevent alternate screen** in Settings → AI → Agents is enabled by default for every agent. TUIC applies Claude Code's `CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN=1` and `CLAUDE_CODE_DISABLE_AGENT_VIEW=1` environment controls when enabled. A terminal that enters the alternate screen within its first seconds raises a warning toast naming the session and the fix. Codex and Grok receive `--no-alt-screen` when their installed CLI advertises it in `--help`; OpenCode receives `--mini` when available. On Windows, the probe selects an executable `.exe` or `.cmd` launcher ahead of an extensionless npm shell shim. TUICommander checks the executable with a two-second deadline and shares concurrent checks. A timed-out check is not retried until that binary changes; a quick inconclusive exit is retried after a short cooldown. Older versions still start without an unsupported option. Existing flags are not duplicated. Rust applies the same policy to structured IPC, HTTP and MCP launches and exports it to new TUIC shells, where zsh, bash and fish wrappers apply it to commands typed manually. `command <agent>` bypasses those wrappers. Reopen an existing shell after changing the setting.

Gemini currently defaults to the primary screen (`ui.useAlternateBuffer: false`), Cursor enables fullscreen only when requested, and pi defaults to regular TUI mode. Other agents without a documented alternate-screen control keep their own defaults. If an agent enters the alternate screen despite these defaults, TUICommander records one warning for that session with the agent name and detected version.

To allow alternate screen, turn off **Prevent alternate screen** for that agent. The preference applies to new TUIC agent launches and new shells. An explicit `CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN` value in a Claude command's environment retains precedence.

### Startup updates for managed Codex peers

Managed MCP Codex spawns pass `-c check_for_update_on_startup=false` so an available update does not park a child at the Update now / Skip prompt. This applies to the launch; it does not change the user's saved Codex configuration. Real-binary update-trigger and composer checks are tracked in `to-test.md` (1373-85a3).

### Workspace trust for managed spawns

**Accept workspace trust for managed spawns** is on by default for Claude Code and Codex in Settings → AI → Agents. When an agent starts another agent through TUICommander's `agent spawn` action in a new folder, the child starts without waiting for a workspace trust answer. Codex receives a trust setting for that one launch and folder, including when a custom launcher forwards its arguments to Codex. TUICommander answers Claude Code's initial trust picker only when it shows the expected question with **No, exit** selected. Neither path edits the CLI's saved trust configuration.

Turn this setting off for an agent if you want to answer its normal trust question during managed spawns. Terminals that you open yourself always use the agent's normal trust behavior.

## Agent Detection

A supported agent started by hand in a terminal is detected from its foreground process on macOS, Linux and Windows. The backend records that identity in both desktop and headless builds, without requiring an open UI. HTTP and desktop foreground queries use the same detection logic. `TUIC_SESSION` identifies the terminal; it does not turn a plain shell into an agent. An agent stops being eligible when its foreground returns to a shell. Configured launch presets remain available during startup until a non-shell foreground is first observed, including a custom wrapper; startup helpers can intentionally disarm the preset early to prevent unattended shell input; after that they are also revoked on return to a shell. Transient tool subprocesses preserve agent identity. Submission and mail wake still require a ready composer with no partial user input or approval prompt.

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

## Native Hook Instrumentation

Claude and Codex status signals are enabled by default and scoped to each TUIC launch. Claude receives an additional `--settings <config-dir>/agent-hooks/claude.json`; Codex receives `-c notify=["<config-dir>/agent-hooks/codex-notify.sh"]`. Existing explicit overrides win. The Codex adapter emits idle on `agent-turn-complete` and then chains the user's configured `notify` command with the original JSON payload. No global agent configuration is written by this default path.

After a completion hook, decorative terminal animation and redraws keep the
session idle. New input and recognized active work can start another turn.

An open interactive selection dialog restores the question badge even when
hooks are enabled and another hook cleared it before you answered.

Disable this per agent with **Settings → Agents → Native status signals** to restore screen-only heuristics. Gemini, Grok, and OpenCode retain a separate, explicit **Install hooks globally** toggle.

Instead of inferring busy/idle/waiting from terminal output, TUICommander can drive an agent's status directly from the agent's **own hook system**.

When enabled, TUIC writes small shell hooks into the agent's settings file that emit `OSC 7770;state=…` on each lifecycle event (busy on prompt/tool start, `awaiting` on an approval/question prompt, idle on stop). The session state then follows the hooks precisely, and the heuristic question-detection above is suppressed for that agent. Screen and ordinary silence cannot override a protocol-held turn. If its completion signal is lost, a stable Ready screen plus five minutes without PTY output recovers the session and writes a `protocol-stale` warning.

For Claude, `awaiting` also covers **MCP elicitation** — the dialog an MCP server raises to ask you for input (`MCP server "…" requests your input`, with Accept/Decline). It arrives through Claude's `Elicitation` event and is retracted by `ElicitationResult`; no screen scraping is involved, because that dialog matches none of the question heuristics.

**Ownership is safe and reversible.** Each managed hook carries a `# tuic-managed-hook` sentinel; enabling installs only TUIC's entries and disabling removes only them — your own (and wiz/mdkb) hooks in the same file are never touched. The toggle is the source of truth; the effect applies on the agent's **next launch** (hooks are read at startup).

| Agent | Hooks | Status |
|-------|-------|--------|
| Claude | `~/.claude/settings.json` | Supported |
| Gemini | `~/.gemini/settings.json` | Supported |
| Codex | `~/.codex/hooks.json` + `~/.codex/config.toml` (`[features] hooks = true`) | Supported |
| Grok | `~/.grok/hooks/tuic.json` (own file) | Supported |
| OpenCode | `~/.config/opencode/plugin/tuic.ts` (Bun/TS plugin) | Supported |
| Others (Aider, Amp, Cursor, Goose, Droid, pi) | — | No TUIC-managed hook system — stays heuristic |

> **Platform note:** Hook instrumentation is **macOS/Linux only** — it resolves the controlling tty via `ps`/`/dev/tty`, which has no Windows equivalent. On Windows the toggle is hidden and agents keep heuristic detection (no regression).

## Usage Limit Tracking

For Claude Code, TUICommander detects weekly and session usage limit messages from terminal output:

- **Unified agent badge** — When Claude is the active agent, the status bar shows a single badge combining the agent icon with usage data. The badge displays rate limit countdowns (when rate-limited), Claude Usage API data (5h/7d utilization percentages), or terminal-detected usage limits, in that priority order.
  - Blue: < 70% utilization
  - Yellow: 70–89%
  - Red (pulsing): >= 90%
- Clicking the badge opens the Claude Usage Dashboard.

The API quota follows the focused Claude session's `CLAUDE_CONFIG_DIR`. Switching between Claude sessions using different accounts updates the badge. If the session's profile or its credentials cannot be read, the badge shows unknown rather than the default account's quota.

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
When opened from the badge, its rate-limit data uses that session's credential profile. The transcript-based charts and project statistics currently scan the default Claude projects directory.

## Agent Teams

Agent Teams lets Claude Code spawn teammate agents as TUIC terminal tabs. Enable it in **Settings** > **Agents** > **Agent Teams**.

When enabled, PTY sessions receive the `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1` environment variable, which unlocks Claude Code's `TeamCreate`, `TaskCreate`, and `SendMessage` tools. Agent spawning uses direct MCP tool calls (`agent spawn`) — the earlier it2 shim approach (iTerm2 CLI emulation) is deprecated.

Spawned sessions automatically emit lifecycle events (`session-created`, `session-closed`) so they appear as tabs and clean up on exit.

## TUICommander Agents vs Codex Internal Subagents

This comparison primarily concerns Codex internal subagents. For substantial or
long-running work, prefer agents spawned through TUICommander. Codex coordinates an
internal subagent inside the parent conversation and runtime. A TUICommander agent
runs in its own managed process and session, with a visible terminal and a stable
address that the parent or user can use later.

| Concern | Codex internal subagent | TUICommander agent |
|---------|-------------------------|---------------------|
| **Control** | Controlled through the parent conversation | Visible session with explicit status, input, interrupt, close, and kill controls |
| **Parent interruption** | Its lifecycle remains coupled to the parent runtime | Continues independently after spawn; its terminal and task handle remain available |
| **Context** | May inherit parent conversation history | Receives only the assigned brief and files it reads |
| **Worker output** | Final output normally returns directly to the parent context | Terminal output stays outside the parent context until explicitly requested |
| **Handoff** | Returned through Codex's internal collaboration mechanism | Worker sends a concise result or blocker through the TUICommander inbox |
| **Recovery** | Depends on Codex's internal subagent lifecycle | Parent can reconnect, inspect task state, read the inbox, or resume supported agent sessions |
| **User visibility** | Usually summarized by the parent | Full terminal and lifecycle state remain observable and controllable |

Each worker performs its own model inference and consumes tokens in its own context.
TUICommander does not reduce total model-token usage: MCP calls, briefs, messages,
and handoffs also consume coordination tokens. The gain is operational efficiency.
The parent can assign a focused context, let the worker continue when the parent turn
or window stops, supervise it directly, and retrieve only the result needed for
coordination. Unread terminal output does not enter the parent context, but this does
not make the worker's inference cheaper.

The terminal display itself has no model-token cost; it is rendered locally. A
provider-wide quota or outage can still affect every agent using that provider.
Recovery after a process or application restart depends on the selected agent's
documented session-resume support.

Codex internal subagents remain useful for short, tightly coupled work whose result
is needed immediately in the current turn. TUICommander agents are more operationally
efficient for parallel work that benefits from independent context, persistent
supervision, direct human control, and selective result collection. Claude Code's
native Agent Teams use a separate lifecycle described in [Agent Teams](agent-teams.md).

## Session Binding (TUIC_SESSION)

Every terminal tab has a stable UUID that persists across app restarts. This UUID is injected into the PTY shell as the `TUIC_SESSION` environment variable.

### How It Works

1. When a terminal tab is created, a UUID is generated via `crypto.randomUUID()`
2. The UUID is saved with the tab and restored when the app restarts
3. On PTY creation, the UUID is injected as `TUIC_SESSION=<uuid>` in the shell environment
4. Agents can use `$TUIC_SESSION` for session-specific operations

### Use Cases

**Automatic session binding (Claude Code):**

Shell integration automatically injects `--session-id $TUIC_SESSION` into every `claude` invocation via a shell function wrapper. You don't need to pass it manually — just type `claude` and the session is bound to this tab. The wrapper is bypassed when you explicitly pass `--session-id`, `--resume`, or `--continue`.

```bash
# These are equivalent — the wrapper handles it transparently:
claude                              # wrapper adds --session-id $TUIC_SESSION
claude --session-id $TUIC_SESSION   # explicit, wrapper bypassed
```

Claude Code stores the session locally. When you restart TUICommander and switch to this branch, the session resumes automatically via `claude --resume <uuid>`.

**Automatic session binding (Goose):**

Shell integration injects `--name $TUIC_SESSION` into `goose session` and `goose run` subcommands. The wrapper is bypassed when you explicitly pass `--name`, `-n`, `--resume`, or `-r`.

```bash
# These are equivalent:
goose session "fix the bug"                               # wrapper adds --name $TUIC_SESSION
goose session --name $TUIC_SESSION "fix the bug"          # explicit, wrapper bypassed
```

**Gemini CLI session binding (manual):**

```bash
gemini --resume $TUIC_SESSION
```

**Custom scripts that persist state per-tab:**

```bash
# Use TUIC_SESSION as a stable key for any tab-specific state
echo "Last run: $(date)" > "/tmp/tuic-$TUIC_SESSION.log"
```

### Automatic Resume

When TUICommander restores saved terminals after a restart, only tabs that had an active agent session (`agentType` set) are restored. Plain shell tabs are discarded and a fresh terminal is spawned instead. For agent tabs, TUICommander checks whether the session file exists on disk before deciding the resume strategy:

1. **Verified session** — If the terminal's saved agent session ID maps to an existing session file (e.g. `~/.claude/projects/…/<uuid>.jsonl`), the agent resumes with that agent's ID-specific command. On Windows, Claude's project slug replaces the drive colon with a dash: `C:\Users\foo\bar` becomes `C--Users-foo-bar`.
2. **No saved session ID** — Falls back to the agent's default resume behavior (e.g. `claude --continue` for the last session)
3. **Saved ID no longer verifies** — Refuses automatic resume instead of opening an unrelated last session

The resume command honours the agent's **default run config**: TUICommander swaps the binary in the resume command (`claude`) for the run config's `command` (e.g. `c2`) and appends the run config's args after the resume flag. So a user with the default run config `c2 --model claude-opus-4-6` will resume with `c2 --resume <uuid> --model claude-opus-4-6`, not `claude --resume <uuid>`.

### UI Agent Spawn

When you spawn an agent via the context menu or command palette, TUICommander automatically uses the tab's `TUIC_SESSION` as the `--session-id`. This ensures the spawned session is bound to the tab and will resume correctly on restart.

Sidebar agent launches wait for shell readiness and submit once, even if the first idle event arrives before the command is prepared or the terminal subscribes. Remote repositories use the owning daemon's run configurations, environment and launch-scoped hook paths; desktop profile paths are not copied to that machine.

The active-terminal and sidebar agent menus apply the selected run configuration's environment to that agent launch. The values do not persist in the tab's shell for later commands. `TUIC_SESSION` and `TUIC_PARENT` remain controlled by TUICommander, including differently cased names on Windows. Windows menu launches preserve Unicode characters in environment values and commands.

When the run config's command is a custom alias, symlink, or wrapper (e.g. `c2`, `c`), the foreground-process name no longer matches `"claude"` in `classify_agent`. TUICommander compensates by pre-seeding the session's `agent_type` from the run config at PTY creation time, so intent/suggest parsing and tab-title binding work from the first output line. The foreground-process detector also falls back to the pre-seeded type whenever it sees a non-shell process it doesn't recognise, which covers aliases and wrapper scripts without requiring every name to be hardcoded.

## The embedded agent loop is gone

Unsafe mode, the per-conversation cost footer, per-phase model overrides and the
cron scheduler all belonged to TUICommander's own ReAct loop, which was deleted
in #784-0aec. `ego` runs its own loop over ACP and reaches terminals from
outside, through the `session` MCP tool family — so none of those controls has a
TUICommander-side equivalent any more. Nothing on this page below is affected:
it describes PTY agents (Claude Code, Codex, Gemini, …), which are unchanged.

## Sleep Prevention

When agents are actively working, TUICommander can keep your machine awake:

- Enable in **Settings** → **General** → **Power Management** → **Prevent sleep when busy**
- Uses the `keepawake` system integration
- Automatically releases when all agents are idle

## Environment Flags

Per-agent environment variables can be injected into every new terminal session. Configure in Settings > Agents > expand an agent > Environment Flags.

This is useful for enabling feature flags (e.g., `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`) without manually running `export` commands. Flags are organized by category with toggle, enum, and number types.

## Tips

- **Multiple agents on the same repo** — Use split panes (`Cmd+\`) to run two agents side by side on the same branch
- **Different agents per branch** — Each worktree is independent, so you can run Claude on one branch and Aider on another
- **Monitor all at once** — Use the Activity Dashboard (`Cmd+Shift+A`) to see every terminal's agent status in one view

### ego launch permissions

In **Settings > Agents > ego**, choose a permission mode and filesystem sandbox for new terminal launches and managed spawns. **Use ego configuration** leaves that option unchanged. Mode choices are Read-only (`plan`), Ask (`default`), Auto-edit (`edits`), Auto reviewed (`auto`), and Full access (`yolo`). Sandbox choices are Read-only (`ro`) and Workspace writes (`workspace`). The selected choices replace matching raw launch flags. AI Chat uses separate ACP settings.

## Inspect launch instructions

Right-click a terminal and select **Inspect Launch Instructions…**. Expand a section to read its captured text, source and original UTF-8 byte size. Secrets use the same redaction policy as logs. A queued brief is not yet sent; reopen the inspector after submission to read its updated state.

Capture starts with new managed launches after the backend update. It includes the final brief with TUIC peer context, `--system-prompt`/`--append-system-prompt` values, snapshots of explicitly supplied `--system-prompt-file`/`--append-system-prompt-file` files, and TUIC MCP initialize responses. File snapshots show the bytes available to TUIC before spawn, not confirmation of what the agent read. Shell-launched agents, older sessions and independently loaded files such as AGENTS.md are **Not observable by TUIC**. TUIC does not reconstruct them from current settings.

Redacted text is limited to 32 KiB per section and 64 KiB across 16 captured sections. Larger text is labeled truncated; missing, non-regular, non-UTF-8 or over-1-MiB instruction files are labeled unavailable. Byte counts describe the original payload, not the shorter redacted preview. Receipts survive a frontend reload, but not session closure or a backend restart.
