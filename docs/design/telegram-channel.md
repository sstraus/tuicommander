# Telegram channel for TUICommander

**Stories:** 1438-79b4, 1959-162a, 1983-1b47, 2119-d5de. **Updated:** 2026-10-22.
Offline implementation is available; live mint deployment remains pending.

## Consumer contract and registration

Any TUIC agent may opt in with `telegram {"action":"register"}`. Caller identity
comes from its authenticated MCP session, never an argument or configured target.
Without explicit registration, the latest-connected live AI Chat peer receives
allowlisted text. If several chats are connected, connection order selects the
fallback, not the focused tab. Explicit registration always wins and is not
replaced by opening or reconnecting AI Chat. Only when neither owner exists does
the adapter send the no-agent instruction.

One agent is registered at a time. A new registration replaces the previous agent
and sends it one native TUIC mail: "Your Telegram registration was replaced by
another agent." Repeated registration by the same agent sends no notice.
Replacement retires the active draft and current button handles. Open requests
receive one stopped notice; their correlation does not move to the new agent.

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
registered agent, or the live AI Chat fallback. Text never becomes shell input. Native inbox and wake arbitration
remain authoritative: busy agents receive mail at a safe boundary. With neither a live registration nor an AI Chat fallback, the bot replies exactly "No agent is listening. Ask an agent in TUICommander to register for Telegram.", drops the text
and advances the cursor. Non-allowlisted chats stay silently dropped.

## Placement and native state

The desktop and headless daemon start the same Rust `telegram` supervisor;
the existing process-held polling lock selects one owner.
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
| `begin` | Optional `request_id` from inbound mail; returns the existing or new `draft_id`, including for AI Chat without a PTY. |
| `activity` | `request_id`, `text`; updates the active request with a concise unapproved preview. |
| `finish` | `request_id`, `text`; sends the exact final reply even after draft retirement and returns known `message_ids`. Completed requests return the same IDs. |
| `send` | `text`, optional rows of `{label,data}` buttons; returns known `message_ids`. |

Request-specific begin/activity/finish stay bound to the peer that received the request, even if inbound ownership changes. Unrelated callers receive
`telegram_not_registered: call telegram register first`. Receipt and working
acknowledgements are TUIC-driven; begin and activity are optional. A PTY agent
calls finish with its complete authored reply. No raw PTY text, tool arguments or hidden reasoning
is streamed. Tool arguments cannot register a different agent.

## Non-owner notifications and reply routing

Any live MCP-bound agent may call `send` without registering. This does not change
registration, inbound ownership, pending requests or the owner’s current buttons.
The destination remains the single allowlisted chat. Non-owner buttons are refused
with `telegram_non_owner_buttons`: this preserves the existing owner-only callback
contract without a second callback-routing map.

The server derives `[<session name>] ` from the caller identity and adds it to
every outgoing non-owner chunk. The agent cannot choose the attribution. Names must
be non-empty, contain no brackets or control characters, and use at most 128 UTF-16
units; invalid names return `telegram_invalid_sender_name`. Each non-owner peer
may send once per 30 seconds; an earlier attempt returns
`telegram_sender_rate_limited`. The interval is charged before network I/O, including
uncertain failures, and expires in memory. Owners have no sender interval or prefix.
Owner text remains byte-exact, including leading brackets. Splitting includes
attribution within the conservative 4096 UTF-16-unit message budget. An owner
message that starts with a live peer name in brackets can route a Reply to that
peer: this ambiguity is accepted instead of changing the authored text.

For Telegram Reply, the adapter reads `message.reply_to_message.text`. A leading
`[<name>] ` selects a unique live peer with that exact name. New messages, unprefixed
replies, unknown names, dead peers and duplicate live names use the
current inbound owner. Reply routing does not register the sender; the request’s
begin/activity/finish authority follows its delivered recipient as before. No sent
message-ID cache is kept. Renamed or replaced peers therefore use current names,
not historical sender identity.

The reply fixture is synthetic and derived from the official
[Message schema](https://core.telegram.org/bots/api#message), not recorded traffic.
The schema defines optional `reply_to_message` as a Message in the same chat and
thread, without nested `reply_to_message` fields.

## Drafts, outbound delivery and callbacks

An accepted message receives a 👀 reaction on the user's message. With a
registered recipient, TUIC also creates a native `sendMessageDraft` bubble.
With no agent, only the receipt reaction and existing registration instruction
are sent, not a draft. Request identity binds to the peer and recipient, not a
turn epoch. A broadcast lag preserves the request and draft.

A second acknowledgement, 👨‍💻, requires the native inbox read cursor to pass
the delivered message and a recipient turn to start after delivery. A previously
running turn is not evidence that the new message was read. Waiting changes the
reaction to 🤔. Success sets 👍; failure sets 👎 and sends one safe notice.
System status text is Italian and has no approval label. Agent-written previews,
including streamed ACP text, keep `Preview — not approved.`. These reversible
presentation choices are centralized in the Telegram module.

The oldest open request owns the chat's one draft; additional open requests
appear as a queued count. After ten minutes without the second acknowledgement,
one persistent queued notice replaces the draft. Later work restarts it. An idle
read request with no reply produces one notice after sixty seconds from the
observed Read. Exit, registration loss and ACP failure produce one stopped notice.
Late finish still sends after a notice. All request state is bounded and in memory.

PTY agents stream safe intent/activity, never raw terminal output. AI Chat uses
a separate reader on the real ACP journal so Bot API I/O cannot stall chunk
collection. Only assistant message chunks enter the preview, never thought or
tool chunks. The first unclaimed post-delivery turn is associated after inbox
Read. If that turn was started by TUIC's peer-mail wake and settles without
finish, its exact accumulated assistant text becomes the final answer. An
unrelated user turn never auto-publishes. The reader retains up to eight turns
and 256 KiB of answer text per request; overflow or a journal gap fails visibly
rather than publishing an incomplete answer. Completion aborts its reader.

Activity coalesces at two seconds and unchanged drafts refresh at twenty
seconds. Finalization retires refresh before persistent sends. Plain-text chunks
preserve exact UTF-8 text under a conservative 4096 UTF-16-unit ceiling. Draft
text uses the bounded tail while preserving the preview label. Native drafts
expire within Telegram's documented thirty-second TTL after a crash.

Send and proactive authored done/blocked notices use the first numeric ID in the
current allowlist. Request drafts and finals use the originating chat. A bound
PTY's progress done can complete its working open request; progress and finish
share completion correlation and do not send a second final for that turn.
Intent is activity, not a final reply. No text is invented from terminal state.
Stage transitions in Recent messages record only safe stage names, not content;
per-token draft refreshes do not fill the ring.

Each outbound operation waits at least one second and rechecks the allowlist
before its API call. A 429 delays subsequent operations; a long delay returns a
rate-limited error rather than holding the runtime asleep. An uncertain or partial
final returns an error and is never blindly retried.

Buttons contain random opaque wire handles bound to the current chat and message.
New persistent messages retire old handles. A valid choice sends one structured
native mail to the current registered agent or AI Chat fallback; successful mail consumes all handles
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

A working PTY request arms an in-memory ownership token under the PTY writer
mutex (manual begin retains the same fence). Every
native user-input path retires that token under the same mutex before its first
byte, including split text/Enter and managed submissions. This fences Stop even
when the replacement writer has not yet applied input bookkeeping. Terminal
protocol replies do not retire the token. Stop also checks the registered peer,
its live PTY and the captured turn epoch before writing one Escape. Escape is an
interrupt request, not proof that the agent stopped. It does not enter the line
editor, where a bare escape would corrupt the replacement input. No lifecycle
lock is held while the native writer runs, so captured writers can process output
synchronously. No retries or replacement-turn recovery are introduced.

Queued Stop withdraws only that request. AI Chat Stop passes the captured
connection, session and turn identity to the ACP actor; the actor checks the
expected turn atomically before cancellation, so a delayed Stop cannot cancel
a replacement turn.

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
file lock permits one local polling owner; a 409 (another process polls the same bot token) backs off and retries while send stays available. Read the
private regular token file for each API request, use fixed-host HTTPS without
redirects or proxies, and report only typed errors. Source token/formatted request
buffers are zeroized; reqwest URL copies are not, a remaining memory limitation.
Never log URLs, tokens, chat text or real chat IDs.

Ordinary polling uses `getUpdates(limit=10)`, a 1 MiB response cap and a 25-second
long poll with a larger HTTP timeout. Oversize/malformed data does not advance the
cursor. Network faults, temporary rejections and capacity failures use bounded
exponential backoff. 401/403/404 stop polling with a safe TUIC alert until
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

Settings now writes the same private token/config/allowlist files. Both a one-use six-character ten-minute code and an explicitly typed private-chat ID can authorize a chat. The code lives in private `pairing.json` so desktop and daemon share it; bare `/start` returns the fixed Link chat instruction. An explicitly empty allowlist supports initial pairing without permitting outbound work. The adapter supervisor responds to local setup changes without taking a second poller lock; desktop and `tuic-remote` both start the same supervisor at boot. The first process to acquire the existing polling lock owns the adapter; the standby leaves shared status and registration untouched and retries the lock once per second, so it can take over after the owner exits. Disabled or stopped supervisors reject MCP requests instead of leaving them queued. Status includes the polling process identity and expires after sixty seconds. Settings identifies the owner relative to the serving process and enables pairing only while connected. Status exposes safe categories, timestamps and the registered agent name, never token or message text. The volatile MCP registration is published through the same private status file; enablement writes no target UUID. The deployment uses one proactive outbound destination; correlated replies use their originating chat.

### Pairing feedback

The poller consumes private-chat `/start` and six-character hexadecimal setup inputs before agent mail. Successful linking replies `Chat linked.`; `/start` explains Link chat; wrong or expired codes from unauthorized chats receive a generic retry instruction. Fixed setup replies use the existing serialized outbound worker and throttle. Only these fixed replies can reach an unauthorized private chat; agent sends still require current allowlist membership. Replies never include a code, token or another chat ID. Settings reads `pairing_pending` on its existing status refresh and clears a consumed local hint.

### Recent activity

The app state owns a process-local 20-entry ring. Inbound native mail, setup replies, callbacks and the serialized outbound path record outcomes there. Settings snapshots return `recent_events` newest first with `id`, `time` (epoch milliseconds), `direction`, `kind`, `outcome` and `preview`. Previews are redacted before truncation to 60 Unicode characters; setup input and unauthorized message text are always replaced with `[redacted]`. Token-shaped text and six-character hexadecimal credentials are also removed from authorized previews. Draft refreshes do not fill the log. Safe errors use the existing Telegram error categories, never network response bodies. The ring is not part of `status.json` or any other persisted state; restart starts empty. Each process shows only its own activity, not a separate polling owner's history.
