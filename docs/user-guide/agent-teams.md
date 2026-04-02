# Agent Teams

Agent Teams let Claude Code spawn teammate agents that work in parallel, each in its own TUICommander terminal tab. Teammates share a task list, communicate directly with each other, and coordinate autonomously.

## How It Works

TUICommander automatically injects `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1` into every PTY session. This unlocks Claude Code's `TeamCreate`, `TaskCreate`, and `SendMessage` tools. When Claude Code spawns a teammate, TUICommander creates a new terminal tab via its MCP `agent spawn` tool — no external dependencies required.

## Setup

No configuration needed. Agent Teams is enabled by default for all Claude Code sessions launched from TUICommander.

To verify it's active, check the environment inside any terminal:

```bash
echo $CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS
# Should print: 1
```

## Usage

Tell Claude Code to create a team using natural language:

```
Create an agent team with 3 teammates to review this PR:
- One focused on security
- One on performance
- One on test coverage
```

Claude Code handles team creation, task assignment, and coordination. Each teammate appears as a separate tab in TUICommander's sidebar.

### Navigating Teammates

Claude Code supports two display modes for teammates:

| Mode | How it works | Requirement |
|------|-------------|-------------|
| **In-process** | All teammates run inside the lead's terminal. Use `Shift+Down` to cycle between them. | None |
| **Split panes** | Each teammate gets its own pane. | tmux or iTerm2 |

TUICommander works with both modes. In-process mode is the default and requires no extra setup. With split panes, each teammate appears as a separate TUICommander tab.

### Key Controls (In-process Mode)

| Key | Action |
|-----|--------|
| `Shift+Down` | Cycle to next teammate |
| `Enter` | View a teammate's session |
| `Escape` | Interrupt a teammate's current turn |
| `Ctrl+T` | Toggle the shared task list |

### What Teams Can Do

- **Shared task list** — All teammates see task status and self-claim available work
- **Direct messaging** — Teammates message each other without going through the lead
- **Plan approval** — Require teammates to plan before implementing; the lead reviews and approves
- **Parallel work** — Each teammate has its own context window and works independently

## Good Use Cases

- **Code review** — Split review criteria across security, performance, and test coverage reviewers
- **Research** — Multiple teammates investigate different aspects of a problem simultaneously
- **Competing hypotheses** — Teammates test different debugging theories in parallel and challenge each other
- **New features** — Each teammate owns a separate module with no file conflicts

## Limitations

Agent Teams is an experimental Claude Code feature. Current limitations:

- **No session resumption** — `/resume` does not restore in-process teammates
- **One team per session** — Clean up before starting a new team
- **No nested teams** — Teammates cannot spawn their own teams
- **Token cost** — Each teammate is a separate Claude instance; costs scale linearly with team size
- **File conflicts** — Two teammates editing the same file leads to overwrites; assign distinct files to each

## Troubleshooting

**Teammates not appearing as tabs:**
- Verify the env var is set: `echo $CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS` should print `1`
- Check that TUICommander's MCP server is running (status bar shows the MCP icon)

**Teammates not spawning at all:**
- Claude Code decides whether to create a team based on task complexity. Be explicit: "Create an agent team with N teammates"
- Check Claude Code version: Agent Teams requires a recent version

**Too many permission prompts:**
- Pre-approve common operations in Claude Code's permission settings before spawning teammates

## Inter-Agent Messaging

TUICommander includes a built-in messaging system that lets agents in different terminal tabs communicate directly. This works alongside (and independently from) Claude Code's native Agent Teams messaging.

### How It Works

Every PTY session gets a stable `TUIC_SESSION` UUID injected as an environment variable. Agents use this as their identity to register, discover peers, and exchange messages through TUICommander's MCP `messaging` tool.

When both sender and recipient are connected via SSE (the default for Claude Code agents), messages are **pushed in real-time** as MCP channel notifications (`notifications/claude/channel`). They also land in a buffered inbox as a fallback.

### What Gets Injected Automatically

TUICommander injects these into every Claude Code PTY session — no manual configuration needed:

| Variable / Flag | Value | Purpose |
|---|---|---|
| `TUIC_SESSION` | Stable UUID per tab | Agent identity for messaging |
| `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS` | `1` | Unlocks TeamCreate/TaskCreate/SendMessage |
| `--dangerously-load-development-channels server:tuicommander` | *(CLI flag, agent spawn only)* | Enables real-time channel push from TUICommander |

### Messaging Flow

1. **Register** — The agent reads its `$TUIC_SESSION` and registers as a peer:
   ```
   messaging action=register tuic_session="$TUIC_SESSION" name="worker-1" project="/path/to/repo"
   ```

2. **Discover peers** — Find other agents connected to TUICommander:
   ```
   messaging action=list_peers
   messaging action=list_peers project="/path/to/repo"   # filter by repo
   ```

3. **Send a message** — Address by the recipient's `tuic_session` UUID:
   ```
   messaging action=send to="<recipient-tuic-session>" message="PR review done, 3 issues found"
   ```

4. **Check inbox** — Read buffered messages (useful if channel push was missed):
   ```
   messaging action=inbox
   messaging action=inbox limit=10 since=1712000000000
   ```

### Channel Push vs Inbox

| Delivery | When | Latency | Requires |
|----------|------|---------|----------|
| **Channel push** | Recipient has active SSE stream | Real-time | `--dangerously-load-development-channels server:tuicommander` on the recipient's CC process |
| **Inbox buffer** | Always | Poll-based | Registration only |

Messages are always buffered in the inbox regardless of whether channel push succeeds. The inbox holds up to 128 messages per agent (FIFO eviction). Individual messages are capped at 64 KB.

### Using Messaging from a Standalone Claude Code Session

If you run Claude Code outside TUICommander but still want to use TUIC messaging, you need to:

1. **Set `TUIC_SESSION`** — export a stable UUID:
   ```bash
   export TUIC_SESSION=$(uuidgen)
   ```

2. **Connect to TUIC's MCP server** — add to your `.mcp.json`:
   ```json
   {
     "mcpServers": {
       "tuicommander": {
         "url": "http://localhost:17463/mcp"
       }
     }
   }
   ```

3. **Enable channel push** *(optional, for real-time delivery)*:
   ```bash
   claude --dangerously-load-development-channels server:tuicommander
   ```

4. **Register on first turn** — the agent must call `messaging action=register` with its `$TUIC_SESSION` before sending or receiving.

### Messaging vs Claude Code Native SendMessage

| Feature | TUIC Messaging | CC Native SendMessage |
|---------|---------------|----------------------|
| **Transport** | MCP tool call → server-side routing | File append + polling (`~/.claude/teams/`) |
| **Real-time push** | Yes (MCP channel notifications) | No (polling only) |
| **Cross-app** | Any MCP client can participate | Claude Code processes only |
| **Discovery** | `list_peers` with project filter | Team config file |
| **Persistence** | In-memory ring buffer (lost on TUIC restart) | Files on disk (survives restart) |

Both systems work simultaneously. Claude Code agents spawned by TUICommander can use either or both.

## Deprecated: it2 Shim

Earlier versions of TUICommander used an `it2` shell script shim that emulated iTerm2's CLI to intercept teammate creation. This approach is deprecated — teammate spawning now uses direct MCP tool calls (`agent spawn`). The shim at `~/.tuicommander/bin/it2` is no longer needed.
