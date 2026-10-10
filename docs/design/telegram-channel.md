# Telegram channel for TUICommander

**Story:** 1438-79b4. **Updated:** 2026-10-04. **Branch:** feat/w3-telegram.
Offline implementation is available; live mint deployment remains pending.

## Consumer contract and registration

Any TUIC agent may opt in with `telegram {"action":"register"}`. Caller identity
comes from its authenticated MCP session, never an argument or configured target.
One agent is registered at a time. A new registration replaces the previous agent
and sends it one native TUIC mail: "Your Telegram registration was replaced by
another agent." Repeated registration by the same agent sends no notice.
Replacement retires pending correlation, active drafts and current button handles;
old requests and choices do not migrate to the new agent.

`telegram {"action":"unregister"}` releases the caller's own registration.
Another agent cannot unregister the current owner. Registration ends when its MCP
session ends, its PTY closes, or foreground detection observes that the agent exited
or changed type, even if no intervening shell was observed. Unrecognized non-shell
probes do not prove an identity change and retain registration. The foreground detector retires the volatile
registration lifetime immediately; reusing the same terminal and MCP identity
does not transfer opt-in to a replacement agent. Agents without a PTY are
supported through their MCP session identity. Registration lives only in memory; daemon restart requires
another register call. No agent dropdown or target UUID belongs in configuration.

An allowlisted private chat delivers text as JSON in native TUIC mail to the
registered agent. Text never becomes shell input. Native inbox and wake arbitration
remain authoritative: busy agents receive mail at a safe boundary. Without a live
registration, the bot replies exactly "Nessun agent registrato", drops the text
and advances the cursor. Non-allowlisted chats stay silently dropped.

## Placement and native state

The Rust `telegram` module starts explicitly in `run_remote`, never desktop boot.
One sequential inbound poller and one outbound runtime share a command channel.
Registration, native text handoff, callback handling and tool actions are serialized
by that runtime. Inbound parsing does not select an agent; delivery resolves the
current live registration after the long poll. Native mail uses the existing
stable-ID send service, normal urgency and its inbox-first safe wake policy.
The adapter is a daemon-local registered peer without a PTY.

The native MCP session map, peer registry, live PTY map and derived session snapshot
own identity and lifecycle. Foreground-agent discovery already runs headlessly.
Shared PTY input parsing and submission semantics remain authoritative. The only
adapter-originated terminal input is one fenced Escape for the current draft Stop.

## Agent MCP surface

One native tool, `telegram`, is available over the existing HTTP MCP and local
bridge transports. No new Tauri command is needed. Unknown fields are rejected;
there are no target peer, token, chat selector, approval or idempotency arguments.

| Action | Input and result |
| --- | --- |
| `register` | No extra input; registers the caller and returns `registered:true`. |
| `unregister` | No extra input; releases its registration and returns `registered:false`. |
| `begin` | `request_id` from inbound mail; requires its pending request and current working/awaiting-input turn; returns `draft_id`. |
| `activity` | `request_id`, `text`; reports concise safe activity for that live turn. |
| `finish` | `request_id`, `text`; sends the exact final reply and returns known `message_ids`. |
| `send` | `text`, optional rows of `{label,data}` buttons; returns known `message_ids`. |

Only the registered agent may begin/activity/finish/send. Others receive
`telegram_not_registered: call telegram register first`. The agent must call begin
when it accepts phone mail, activity at material tool boundaries and finish with
its complete authored reply. No raw PTY text, tool arguments or hidden reasoning
is streamed. Tool arguments cannot register a different agent.

## Drafts, outbound delivery and callbacks

Begin binds request, caller, PTY and turn epoch. An empty Thinking draft starts
immediately; nonempty activity carries an unapproved preview label. Activity
coalesces at two seconds and unchanged drafts refresh at twenty seconds. Idle,
completed or replaced turns retire unfinished drafts without inventing a reply.
Finalization retires refresh before persistent sends. Plain-text chunks preserve
exact UTF-8 text under a conservative 4096 UTF-16-unit ceiling.

Send and authored done/blocked notices use the first numeric ID in the current
allowlist as the single outbound destination; several allowlisted chats may send
inbound text. Reply drafts and finals use the request's originating chat.
Notifications require a committed `ProgressRecorded` event for the registered
agent's current live PTY. Intent supplies activity, never a final reply. Event-bus
lag retires active drafts and reports degraded delivery, without journal replay.

Each outbound operation waits at least one second and rechecks the allowlist
before its API call. A 429 delays subsequent operations; a long delay returns a
rate-limited error rather than holding the runtime asleep. An uncertain or partial
final returns an error and is never blindly retried.

Buttons contain random opaque wire handles bound to the current chat and message.
New persistent messages retire old handles. A valid choice sends one structured
native mail to the current registered agent; successful mail consumes all handles
before one acknowledgement and one selected-label edit attempt. The selected
keyboard uses `{"text":label,"disabled":{}}` without `callback_data`.
There is no callback expiry, eviction quota or acknowledgement retry state.
Buttons confer no publish approval; durable receipts and publisher integration
are outside this story. [Telegram button schema](https://core.telegram.org/bots/api#inlinekeyboardbutton).

## Phone Stop

Draft creation and refresh set `can_stop: true` and `keep_on_stop: false`.
A `stopped_message_generation` update must name the current allowlisted private
chat and draft. The runtime consumes the draft before attempting interruption;
duplicate, stale, wrong-chat and replaced-turn updates cannot write Escape.

`begin` arms an in-memory ownership token under the PTY writer mutex. Every
native user-input path retires that token under the same mutex before its first
byte, including split text/Enter and managed submissions. This fences Stop even
when the replacement writer has not yet applied input bookkeeping. Terminal
protocol replies do not retire the token. Stop also checks the registered peer,
its live PTY and the captured turn epoch before writing one Escape. Escape is an
interrupt request, not proof that the agent stopped. It does not enter the line
editor, where a bare escape would corrupt the replacement input. No lifecycle
lock is held while the native writer runs, so captured writers can process output
synchronously. No retries or replacement-turn recovery are introduced.

[Telegram draft and Stop schema](https://core.telegram.org/bots/api#sendmessagedraft).

## Configuration, secrets and polling cursor

`~/.config/tuic-telegram/config.json` contains only `enabled` and `bot_alias`:

```json
{"enabled":true,"bot_alias":"phone"}
```

The old `target_tuic_session` field is removed and rejected as an unknown field;
remove it before loading the new backend. The sole allowlist is the private
`allowed_chat_ids` file beside `bot.token`, one positive decimal chat ID per line.
Missing, empty, unreadable or malformed authorization fails closed. Never learn
permissions from first contact or `/start`. Refresh authorization after each long
poll and before outbound sends. Deployment normally configures one chat ID.

Missing/disabled config returns before opening token or owner files. A process-held
file lock permits one local polling owner; 409 stops a conflicting owner. Read the
private regular token file for each API request, use fixed-host HTTPS without
redirects or proxies, and report only typed errors. Source token/formatted request
buffers are zeroized; reqwest URL copies are not, a remaining memory limitation.
Never log URLs, tokens, chat text or real chat IDs.

Ordinary polling uses `getUpdates(limit=10)`, a 1 MiB response cap and a 25-second
long poll with a larger HTTP timeout. Oversize/malformed data does not advance the
cursor. Network faults, temporary rejections and capacity failures use bounded
exponential backoff. 401/403/404/409 stop polling with a safe TUIC alert until
restart. 429 honors `retry_after`, clamped to one second–one hour.

Persist only `next_offset`, atomically at mode 0600 after the complete batch is
handed off or dropped, including strangers and no-agent drops. Failed handoff
preserves the old cursor and may reoffer earlier mail with stable IDs. There is
no adapter payload journal, lost-mail detection, consumption receipt or durable
outbound operation. A restart can lose unread native inbox mail; Boss resends if
no Thinking/reply arrives. A lost send response can leave delivery uncertain.

A missing/corrupt cursor logs a reset alert, samples and discards the last backlog
update with `offset=-1, limit=1, timeout=0`, then saves tail+1 (zero when empty).
No extra bootstrap markers or recovery state machine are added.
[Telegram offset semantics](https://core.telegram.org/bots/api#getupdates).

## Verification and operational limits

Tests observe native inbox mail, actual fake-server requests, cursor files and
production foreground detection. Fake HTTP replies are synthetic adversarial
inputs, not recorded Telegram fixtures or proof of live-service compatibility.
Credentials and real allowlist IDs are never read or copied by automated checks.
Registration tests cover replacement notice, unregistered actions, unregister,
restart, MCP removal, PTY close, agent exit with a surviving shell and no-agent
inbound versus silent strangers. Existing Unicode, callback retirement, lifecycle,
allowlist, backoff and polling tests remain.

Live mint verification and deployment require Boss's authorization. Rust changes
require an explicit backend rebuild/restart; they do not hot-reload. No desktop
instance, token store or real Telegram account is used for offline validation.

### Settings setup (1515-cb81)

Settings now writes the same private token/config/allowlist files. Both a one-use six-character ten-minute code and an explicitly typed private-chat ID can authorize a chat. The code lives in private `pairing.json` so desktop and daemon share it; bare `/start` stays ignored. An explicitly empty allowlist supports initial pairing without permitting outbound work. The adapter supervisor responds to local setup changes without taking a second poller lock; desktop and `tuic-remote` both start the same supervisor at boot. The first process to acquire the existing polling lock owns the adapter; the standby leaves shared status and registration untouched and retries the lock once per second, so it can take over after the owner exits. Disabled or stopped supervisors reject MCP requests instead of leaving them queued. Status includes the polling process identity and expires after sixty seconds. Settings identifies the owner relative to the serving process and enables pairing only while connected. Status exposes safe categories, timestamps and the registered agent name, never token or message text. The volatile MCP registration is published through the same private status file; enablement writes no target UUID. This section supersedes hand-written setup and multi-destination selection language in §5; 1438 uses one outbound destination and no phone Stop.
