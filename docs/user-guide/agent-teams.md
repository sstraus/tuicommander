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
There is no separate `swarm` action: callers compose the `agent` and `session` primitives documented below.

### How It Works

Every PTY session gets a stable `TUIC_SESSION` UUID injected as an environment variable. Agents use this as their identity to register, discover peers, and exchange messages through TUICommander's MCP `agent` tool.

Managed workers receive a payload-free terminal notice that tells them to read their inbox. If a worker is busy, TUICommander waits for a safe idle composer before submitting the notice. This includes Claude Code workers with an SSE stream: a channel notification during one turn cannot start a later turn for unread mail. External Claude Code clients without a managed terminal can receive MCP channel notifications (`notifications/claude/channel`). Every message also lands in a buffered inbox.

A peer explicitly registered as an orchestrator keeps peer results and lifecycle payloads in its inbox. An idle/completed orchestrator receives only one coalesced notice that a message is available and should be read with `agent action=inbox` (`[TUIC] message available …`). The same safe notice may start a turn while background work keeps the task state `working`, but only when the shell is idle, the composer is confirmed ready and empty, and no question is active. Normal mail never injects or steers a genuinely busy or non-quiescent orchestrator. Explicit urgent mail uses the forced-steer flow described below. When that notice would cover nothing but child lifecycle events, it prints them instead (`[TUIC] child agent 8c261794 is now idle; child agent 8c261794 exited (exit 0)`) and no inbox read is needed — a peer message anywhere in the batch sends you back to the generic notice. An active `agent wait` receives the mail directly and suppresses that notice.

If writing that payload-free notice has an uncertain outcome, TUICommander retries it at most once after five seconds. A second uncertain result exhausts the wake budget for the whole unread-mail group; later and newly coalesced mail stays inbox-only until a successful `agent action=inbox` or `agent action=wait` observation clears the group. Later mail then receives a fresh initial wake and one-retry budget.

### What Gets Injected Automatically

TUICommander injects these into every Claude Code PTY session — no manual configuration needed:

| Variable / Flag | Value | Purpose |
|---|---|---|
| `TUIC_SESSION` | Stable UUID per tab | Agent identity for messaging |
| `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS` | `1` | Unlocks TeamCreate/TaskCreate/SendMessage |
| `--dangerously-load-development-channels server:tuicommander` | *(CLI flag, agent spawn only)* | Enables Claude's channel capability; managed peer mail still uses the PTY wake |

### Script callbacks and the inbound webhook

A managed agent's launch instructions include this sentence: a script you start can
call you back with its data when it ends, through the inbound webhook. The last step of
the script posts its result to the agent's own identity:

```bash
curl -sS --fail-with-body --unix-socket "$TUIC_SOCKET" --data-binary @result.txt \
  "http://localhost/agent/webhook/$TUIC_SESSION"
```

On Windows, which has no `$TUIC_SOCKET`, the sentence names
`tuic agent send <session> "<result text>"` instead.

Any process can address any agent the same way: the path takes a peer id, registered
name, PTY id or terminal alias. The body arrives as mail from sender `webhook` and wakes
the agent like other mail, with a payload-free inbox notice; it is never typed into the
terminal. An unknown recipient or a body over 64 KB is refused, and nothing is filed.
See the [HTTP API reference](../api/http-api.md#inbound-agent-webhook) for the format.

### Messaging Flow

> **Identity is automatic.** The bridge asserts your `$TUIC_SESSION` at connect
> (`x-tuic-session` header → server auto-bind), so an agent spawned inside TUICommander
> is already a registered peer. `agent action=register` is only needed to set a friendly
> name/project, or from a standalone/external session where the env-var route is unavailable.
> External MCP clients do not need a plain-shell identity tab: call `agent action=register`
> without `tuic_session` to receive an MCP-scoped UUID, or supply an explicit UUID when the same
> identity must be reclaimed after reconnect. Reconnecting under a *new* UUID? Add
> `replaces="<old-uuid>"` or the old inbox is left behind — nothing links the two identities
> otherwise, and TUIC never guesses one from a name. The reply reports `mail_migrated`, or
> `mail_stranded` with a warning when the old identity still has a live terminal of its own.

> **Prefer blocking waits over polling.** `agent action=wait` returns as soon
> as new mail arrives; `session action=wait session_id=<id> until=idle|exited` blocks on a
> peer's lifecycle. Both default to 60 seconds, cap at 300000 ms, and return
> `{met, timed_out}`. They are event-driven end to end; the bridge deadline follows the
> requested wait instead of its ordinary ten-second timeout. A successful agent wait also returns every retained fresh message (up to the
> 100-message inbox capacity) and a per-recipient logical `next_since` cursor, so the normal path
> needs no separate inbox call. Ordinary idle workers receive a terminal notice that points to the inbox. An idle
> orchestrator instead receives a payload-free `agent action=inbox` wake; an active wait suppresses it.

1. **Register** *(optional — sets name/path)* — the agent reads its `$TUIC_SESSION`:
   ```
   agent action=register tuic_session="$TUIC_SESSION" name="worker-1" path="/path/to/repo"
   ```

2. **Discover peers** — Find other agents connected to TUICommander:
   ```
   agent action=list_peers
   agent action=list_peers path="/path/to/repo"   # filter by repo
   ```

3. **Send a message** — Address by the recipient's `tuic_session` UUID:
   ```
   agent action=send to="<recipient-tuic-session>" message="PR review done, 3 issues found"
   agent action=send to="<recipient-tuic-session>" message="Stop the current edit; the requirement changed" urgency="urgent"
   ```

   `normal` is the default and follows the existing inbox, channel and idle-wake paths.
   Use `urgent` when the recipient must change course before its next step.
   Urgent mail keeps an inbox copy but also forces a safe working Claude Code,
   Codex or ego to stop its turn, then submits the full message after confirmed
   idle. `interrupt` is an alias. A draft, native dialog, unknown owner or timeout
   leaves the body inbox-only and returns the failure reason. It never falls back
   to an ordinary queued notice. The receipt proves input delivery, not model action.

4. **Wait for and receive messages** — one blocking call returns the message bodies:
   ```
   agent action=wait
   ```
   Omit `since`: the server remembers where you got to and resumes from there, so a plain
   `wait` never re-reads mail you already saw. Pass it only to override — `since=0`
   deliberately replays the whole inbox. `next_since` comes back on every response,
   timeouts included.

5. **Check inbox directly** — useful after a reported lifecycle eviction or if channel push was missed:
   ```
   agent action=inbox
   agent action=inbox limit=10 since=1712000000000
   ```

`agent action=send` also returns `recipient_state` with the recipient's `shell_state` and
`agent_state` when the recipient is a managed PTY. External generated peers omit this field.

Automatic lifecycle notifications contain state only (`idle`, `completed`, or `exited`). They do
not contain the worker's result. Every worker reports completed output or a real blocker with
`agent action=send`; use `session action=output` only to investigate the anomaly where a child did
not send that report.

### Channel Push vs Inbox

| Delivery | When | Latency | Requires |
|----------|------|---------|----------|
| **Channel push** | External Claude client has an SSE stream and no managed PTY | Real-time | `--dangerously-load-development-channels server:tuicommander` on the recipient's CC process |
| **Managed wake** | Ordinary managed peer has a safe idle composer | Immediate or next idle transition | Managed PTY |
| **Orchestrator wake** | Registered parent is idle/completed and not waiting | Real-time, coalesced | Managed PTY + authoritative lifecycle |
| **Forced steer** | Managed Claude Code, Codex or ego has a safe composer | Interrupt, confirmed idle, then full message | `urgency="urgent"` (`interrupt` is an alias) |
| **Inbox buffer** | Always | Poll-based | Registration only |

Urgent mail uses the same forced-steer flow as Chat Send now and Compose Steer. It sends Escape, waits up to ten seconds for confirmed idle, then submits the full message, including to an orchestrator. A running Stop hook does not make urgent mail an ordinary busy-queue entry. A draft, native question, unsupported agent or idle timeout leaves the message in the inbox only and returns `delivered=false` with the reason. There is no fallback Enter or queued wake. An active waiter may already own delivery and needs no interruption. `interrupt_payload_and_inbox` reports a written input, not proof the model obeyed it. Interrupted tools may leave partial work, and child processes may continue.

Historical native-key probes are retained in `src-tauri/src/fixtures/agent_mail/urgent_cli_probe_2026_09_27.json`. They show why ordinary busy Enter is not steering: Claude and Codex deferred that input to a tool boundary. Those probes predate the forced-steer contract.

Messages are always buffered in the inbox regardless of whether another delivery path succeeds. The inbox holds up to 100 messages per agent: it evicts safe lifecycle notices before peer mail, and rejects a peer send when only protected peer mail remains. Individual messages are capped at 64 KB.

### Using Messaging from a Standalone Claude Code Session

If you run Claude Code outside TUICommander but still want to use TUIC messaging:

1. **Connect to TUIC's MCP server** — the MCP channel is a Unix socket (Windows: named
   pipe), reached through the `tuic-bridge` stdio adapter, **not** a TCP port. TUICommander
   auto-installs this entry into each supported agent's config; to add it by hand:
   ```json
   {
     "mcpServers": {
       "tuicommander": {
         "command": "tuic-bridge",
         "args": []
       }
     }
   }
   ```
   The bridge finds the socket via `TUIC_SOCKET` → `mcp.sock` → any `mcp-*.sock` in the
   config dir.

2. **Register identity** — omit the UUID for a generated identity scoped to this MCP connection:
   ```text
   agent action=register name="external-reviewer" path="/path/to/repo"
   ```
   Pass `tuic_session="<stable-uuid>"` instead when a future reconnect must reclaim the same identity.
   Registration never creates a PTY.

3. **Enable channel push** *(optional, for real-time delivery)*:
   ```bash
   claude --dangerously-load-development-channels server:tuicommander
   ```

### Messaging vs Claude Code Native SendMessage

| Feature | TUIC Messaging | CC Native SendMessage |
|---------|---------------|----------------------|
| **Transport** | MCP tool call → server-side routing | File append + polling (`~/.claude/teams/`) |
| **Real-time push** | Managed terminal wake or external MCP channel notification | No (polling only) |
| **Cross-app** | Any MCP client can participate | Claude Code processes only |
| **Discovery** | `list_peers` with path filter | Team config file |
| **Persistence** | In-memory ring buffer (lost on TUIC restart) | Files on disk (survives restart) |

Both systems work simultaneously. Claude Code agents spawned by TUICommander can use either or both.

## Deprecated: it2 Shim

Earlier versions of TUICommander used an `it2` shell script shim that emulated iTerm2's CLI to intercept teammate creation. This approach is deprecated — teammate spawning now uses direct MCP tool calls (`agent spawn`). The shim at `~/.tuicommander/bin/it2` is no longer needed.

### Teams across configured remote connections

Desktop MCP peer discovery returns a connection-qualified `address` for every
remote peer. Use this address for mail instead of assuming a terminal alias is
globally unique. Mail reaches the owning daemon's native inbox and wake path;
remote replies and remote-to-remote messages pass through the desktop hub.
The mail link grants no remote spawn capability.

## Independent result readers

Use `agent action=subscribe subscription_id=pump filter=actionable` when a
coordinator and a watcher must not consume each other's mail. Read with
`agent action=inbox subscription_id=pump` or use the continuous HTTP SSE stream
`/agent/subscriptions/pump/events` with the bound `Mcp-Session-Id` header.
Routine lifecycle/idle messages do not wake this reader; peer RESULT/BLOCKED mail,
exits and failures remain actionable. Each batch has a cursor and has_more.
Process it, then call `agent action=ack subscription_id=pump cursor=<cursor>`.
Unacknowledged batches can replay after interruption, so deduplicate effects by
message ID. Acknowledged mail does not replay after reconnect with the same peer
UUID and subscription ID. The newest 1000 messages per peer survive restart;
missed_count reports matching unacknowledged retention losses. Use a different
subscription ID for each consumer. Legacy inbox readers keep their old cursor.

An ACP consumer can subscribe to `tuic://inbox/subscriptions/pump` for filtered
resource updates and idle wakes without a model wait loop. See the complete
[mail contract](../backend/mcp-http.md#durable-independent-mail-subscriptions)
for acknowledgement, retention, reader limits and HTTP transport details.

### Typed reports and agent stages

Claude, Codex and ego use the same MCP `agent` call (including ego over ACP).
The server binds the sender identity. `message` remains required:

```json
{"action":"send","to":"orchestrator","message":"Implementation ready","report":{"state":"ready"}}
```

`report.state` is `ready`, `waiting`, `blocked`, `failed`, `dropped` or `progress`.
`waiting` requires `wait_for`: `boss`, `review` or `dependency`. Other states must
omit it. An optional `question` is allowed only with `waiting` on `boss`:

```json
{"action":"send","to":"orchestrator","message":"Choose scope","report":{"state":"waiting","wait_for":"boss","question":{"id":"scope","text":"Which scope?","options":[{"id":"small","label":"Small"},{"id":"full","label":"Full"}],"recommended_option_id":"small"}}}
```

Question id/text and option id/label must be nonblank; option ids must be unique.
A recommendation must name an option. Invalid reports are rejected before mail
insertion. The combined message UTF-8 bytes and serialized report JSON must fit 64 KiB.
Inbox, wait and durable subscriptions return `report` as an object. Remote mail
preserves it. Free-text mail remains valid, has no `report` field and does not
change the latest typed report. Text prefixes are not a typed report protocol.

Session rows (`GET /sessions`, desktop IPC `list_active_sessions`, MCP session
list/status and peer inventories) expose `report`, `report_at` (host acceptance
Unix milliseconds), `report_message_id`, `stage` and `stage_reason`. Compact
inventories retain these fields. The latest accepted report is retained outside
the bounded inbox, independently of reads, ACK and eviction, for this daemon
lifetime. Historical acceptance time/id remain available after new input;
the report's stage effect ends when its PTY assignment or submitted-input turn
generation changes. A stage is not a claim that output passed review.

One backend table selects the first matching fact:

| Order | Fact | Stage |
|---|---|---|
| 0 | Closed/exited PTY or no live peer binding | `null` |
| 1 | Pending initial prompt, failed or uncertain input write | `undelivered_input` |
| 2 | Current report waits on Boss, or a native question is active | `waiting_on_boss` |
| 3 | Current report is ready or waits on review | `waiting_on_review` |
| 4 | Current report is blocked/failed/dropped or waits on a dependency | `blocked` |
| 5 | Commands are queued | `queued` |
| 6 | Agent is idle/completed without background work | `idle` |
| 7 | Agent is working or owns background work | `working` |
| otherwise | Lifecycle is unknown | `null` |

`progress` never selects a stage by itself: native facts select working/idle or
another matching rule. `ready` means available for review, not approved. An ACP
peer without a PTY receives mail without a fabricated terminal row. No separate
stage mutation action exists.

### Agent capabilities

A spawn fixes what the new agent may do. `agent action=spawn` (and
`spawn.capabilities` on `agent action=launch`) takes `capabilities`, a list of
`land`, `push`, `tag`, `children`, `progress` and `operator`. The `operator` capability permits the automation cutover, rollback and reconciliation actions. Uncapped roots have it; automation and workflow agents do not. TUIC saves an agent’s fixed set before launch and restores it with the session identity after restart. A legacy session without a record must be relaunched to use `operator`; desktop actions are unchanged. The local user can edit these files, so they are not a security boundary. A child, that is a spawn with a
parent, gets none of them unless the list names them. A spawn without a parent
and without the list stays uncapped, so hand-opened tabs keep their rights.

- `children` covers `agent action=spawn`, `agent action=launch` and
  `workflow action=launch`. Plain terminal sessions are not children.
- `progress` covers the `progress` tool.
- `land`, `push` and `tag` have no MCP call. TUIC exports the set as
  `TUIC_CAPABILITIES` (comma-separated, empty for none) and the repository
  `pre-push` hook refuses a branch push without `push` and a tag push without
  `tag`. For a capped agent TUIC also sets `core.hooksPath` (through
  `GIT_CONFIG_COUNT`/`KEY`/`VALUE`) to a TUIC hooks directory, so a push is
  refused in a repository that has no such hook, also from a login shell such
  as the Claude Code Bash tool. Each hook there runs the repository's own hook
  of the same name. A `git` shim first on `PATH` adds the `--no-verify` case,
  but a login shell can move it behind `/usr/bin`. Enforcement is advisory and
  holds only where TUIC starts the agent on macOS or Linux: `git push
  --no-verify` in a login shell, an absolute path to git, `git -c
  core.hooksPath=...`, `env -u GIT_CONFIG_COUNT`, a resumed tab (story 2757)
  and Windows (repository hook only) are not covered.

TUIC refuses a call outside the set with `code: "capability_denied"`, the
`capability` it needs and `effect: "not_started"`. The parent gets one
`capability_denied` mail per refused capability, however often the child
retries. A capped agent can grant a child only capabilities it holds. The set is
written only by the spawn and removed with the PTY: mail, prompts and the
caller's `env` (which cannot set `TUIC_CAPABILITIES`) cannot widen it.

### Silent managed workers

One silence rule covers every agent session, managed or opened by hand: an
agent whose stage is `working` or `queued`, with no new output, submitted
input, agent hook event or accepted typed report for 30 awake minutes, is
stalled. Session and peer inventories then report `stalled: true` and
`stage: "stalled"` (`stage_reason: "silent_past_threshold"`), the session state
pushed to clients carries `stalled: true`, and the tab and sidebar dots show it
in the warning color instead of busy. An idle agent never stalls: it waits for
its user, and the stop of a managed child is a child report (below). Any new
activity clears the stall at once.

Only a managed PTY (one with a parent) also gets recovery: one
`state_change` parent mail with `state: "stalled"`, `recovery: "nudged_worker"`
and `stop`, the command that stops the worker. The worker gets one nudge in its
inbox and terminal that asks for a progress, BLOCKED or RESULT report. Recovery
never stops the PTY: silence is not proof of a hang, so the stop is the parent's
decision.

Activity before the threshold starts a new silence window. Declared waits for
Boss, review or a dependency do not count as silence; neither does a BLOCKED
mail to the parent (until the parent answers). Running background work does not
exempt a worker, since the command can hang; the notice carries `background_work: true`.
A worker whose latest mail to its parent starts with `RESULT` gets no nudge; its
parent notice says `recovery: "parent_decides"`. `keep_open` does not exempt
a worker from stall recovery. An unknown or unbound peer is never a target.
The five-second maintenance sweep evaluates silence; sleep and daemon restarts
do not count toward the window. Exited workers and current terminal reports
remain excluded. Existing idle-close policy still applies to eligible workers.

Set `TUIC_AGENT_STALL_MINUTES` in the daemon environment to change the threshold;
`0` disables alarms and recovery. An absent or invalid value uses 30 minutes. This covers local
managed PTYs; an ACP conversation without a PTY has no silence alarm here.

### Child reports at a silent stop

Each parent mail or terminal submission to a managed child opens one report
obligation (the spawn task opens the first). A mail from the child to its parent
that starts with `RESULT` or `BLOCKED`, or any typed report except `progress`,
closes it. When the child stops while the obligation is open, TUIC closes it and
sends the parent one mail with `type: "child_report"` and subscription kind
`report`. The same sweep handles Claude, Codex, ego and every other agent:

| `state` | Stop | Delay |
|---|---|---|
| `ended_without_result` | agent idle or completed, no background work, no queued input, no live `tuic bg` runner and no background command under the agent (one started in the agent's first minute is not seen) | 15 s without input |
| `error` | the agent refused its turn (`agent_state: "error"`); `detail.reason` | 15 s |
| `blocked_dialog` | a question or dialog owns the composer; `detail.on` is its text | 5 s without input |
| `dead` | the agent exited by itself; `detail.exit_code` | at exit |

The mail also carries `obligation` (the opening message or submission id, or
`spawn`), `obligation_open`, `last_text` (the screen above the input box, at
most 2 KB) and `resume` (`agent`, the discovered `id` when known, and
`tuic_session`). A consecutive turn, a background wake or a queued input inside
the delay sends nothing. An exit that TUIC caused (kill, close, idle close,
suspend) sends no report. Subscribe with `filter=report` to read only these.
The report shares the maintenance sweep with the stall alarm, so
`TUIC_AGENT_STALL_MINUTES=0` does not disable it.

Before that report, TUIC takes one automatic recovery step for each
obligation. At the first `ended_without_result` stop it reprompts the child to
continue or to send its report. At the first `error` stop it waits 60 s, then
types one `continue` into the refused turn when the composer is idle and empty.
The parent gets a `child_report` notice of the step with `recovery: "reprompt"`
or `"retry"`, and the obligation stays open. A second stop for the same
obligation sends the report with `recovery: "exhausted"` and takes no further
step. A report without a step carries `recovery: "none"`. A new parent task
opens a new obligation with its own step.

### Mail delivery to an idle child

A parent mail to its idle managed child counts as delivered only when the child
starts a turn: TUIC types the wake and then sees the agent's own busy edge (a
native busy hook or a Working screen), or the child reads its inbox. The send
result then has `turn_check: "pending"`. When neither happens within 20 s, the
parent gets one mail with `type: "delivery_failed"` (subscription kind
`failure`): `reason` is `agent_not_ready` or `awaiting_input` (the wake is still
queued), `composer_not_confirmed`, `no_turn_started` or `exited`; `message_id`,
`pending` (unread mails of the child), `session_id`, `tuic_session` and `name`.
The mail stays in the child's inbox. The failure closes the report obligation
that mail opened, so no `child_report` and no recovery step follow for it. A child that was busy
when the mail arrived is not checked.

## Waiting on you

Plans and Stories shows **Waiting on you** above the plans: native stories waiting for manual checks and unanswered workflow questions of the open project. Select an entry to open its story, or the Run history of the question's run, where you answer it. Answered, superseded and cancelled workflow questions disappear through the workflow engine's inbox projection.

The activity bell notifies each waiting item once, in **WAITING ON YOU**, for every registered local project. The entry opens the same story or run. It leaves the bell when the story leaves manual checks or the question is answered; a dismissed entry stays dismissed while the item waits. Terminal questions are not waiting items: the terminal's awaiting badge and the Activity Dashboard row report them.

## Agent health

Activity Dashboard shows reported failures, blocked agents, undelivered input and the owning host's explicit stalled flag. The view does not run its own silence timer. Missing live stage data is shown as unknown. Refresh health retries a failed read. Connected remote sessions supply their host-owned stages.
