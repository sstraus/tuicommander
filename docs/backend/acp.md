# ACP Client for ego

TUICommander speaks the Agent Client Protocol (v1) as a **client**, to
[ego](https://github.com/sstraus/ego) as the agent. Two repositories, no shared
crate: both pin `agent-client-protocol` and agree on the wire, not on types.

AI Chat uses this backend through `acp_*` Tauri commands and, identically,
through `/acp` HTTP routes.

## Layout

| File | What lives there |
|------|------------------|
| `src-tauri/src/acp/mod.rs` | The vocabulary: ids, snapshots, events, errors, notices, and the capability snapshot taken at `initialize` |
| `src-tauri/src/acp/manager.rs` | `AcpClientManager` — one supervised child per connection, and every operation a caller can ask for |
| `src-tauri/src/acp/connection.rs` | The per-connection actor: decides and writes serially, waits concurrently |
| `src-tauri/src/acp/events.rs` | `AcpEventJournal` — the ordered, bounded record every subscriber reads from |
| `src-tauri/src/acp/ego_ext.rs` | `_ego/pause`, `_ego/resume`, `_ego/compact` |
| `src-tauri/src/acp/mcp_host.rs` | MCP-over-ACP: the host contract and the per-connection channel |
| `src-tauri/src/mcp_http/acp_mcp.rs` | The host: `tuicommander` served through the `/mcp` handler |
| `src-tauri/src/acp_commands.rs` | The Tauri surface: one command per manager method, nothing else |
| `src-tauri/src/mcp_http/acp_routes.rs` | The HTTP surface, calling the same cores |

## Conversational message actions

`acp_message_actions` is the shared IPC/HTTP service for AI Chat reactions and comments.
It sends the native source message ID, a quote capped at 4000 characters, and the action
through the original attachment's prompt queue. The text states that the action is
conversational, not an evaluation or authorization. No provider-specific feedback path,
permission response, reconnect, or PTY fallback is used.

Accepted reactions are stored by session and native message ID in configuration deltas.
The service serializes acceptance through persistence; failed queue acceptance does not
change marks. A persistence error after acceptance explicitly tells the client not to
resend. Marks mean queue acceptance, not model consumption. Removing a mark sends a
new conversational turn and cannot retract earlier context. Clients reread marks after
settings invalidation, reconnect, and action receipts.

## MCP over ACP

Every attended session is given one MCP server,
`{"type":"acp","name":"tuicommander","serverId":"tuicommander"}` (schema 1.5.0,
`unstable_mcp_over_acp`). ego reaches it on this connection:

| ego sends | TUIC answers |
|-----------|--------------|
| `mcp/connect {serverId}` | `{connectionId}`; any other `serverId` is `-32602` |
| `mcp/message {connectionId, method, params}` request | the inner MCP `result`, or the inner MCP error as the ACP error |
| `mcp/message` notification | accepted, nothing sent back |
| `$/cancel_request {requestId}` for a pending `mcp/message` | that request answers `-32800` and its handler future is dropped |
| `mcp/disconnect {connectionId}` | `{}` |

TUIC sends `mcp/message` notifications on the same `connectionId` for what the
HTTP SSE stream carries: `notifications/tools/list_changed` and channel
notifications. An unknown `connectionId` is `-32602` and never reaches the host.

The connection layer only routes; `McpOverAcpHost` is injected with
`set_mcp_host` because the handler needs the application state the manager must
not hold. `AcpMcpHost` opens one MCP protocol session per `mcp/connect`, bound to
the peer id ego was launched under (its `TUIC_SESSION`), and sends every request
through `mcp_post` — the same handler, tool registry and collapsed surface as
HTTP `/mcp`. `mcp/disconnect` and the end of the ACP connection both run
`end_mcp_session`, the teardown DELETE `/mcp` uses. There is no `tuic-bridge`
process and no HTTP round trip for ego.

Two resources exist only on this channel (its `initialize`/`server/discover`
add `"resources":{"subscribe":true}`; HTTP `/mcp` does not):

| URI | Contents |
|-----|----------|
| `tuic://workspace` | `{repos:[{name,path,branch,liveAgents:[{sessionId,name,agentType}]}],viewedRepo}` — `repositories.json` order, agents by PTY cwd, `activeRepoPath` |
| `tuic://inbox` | `{messages:[{id,from:{session,name},body,timestamp}],nextSince}` — unread mail after the peer's cursor; reading does not advance it, `agent action=inbox` does |

`resources/subscribe {uri}` opts in; a subscribed resource gets one
`notifications/resources/updated {uri}` per 250 ms burst, and the workspace only
when its content changed. Mail also wakes an **idle** ego: two seconds after the
first message of a burst, if any of it is still unread and no turn is running on
the connection, `wake_idle_peer` sends one `session/prompt` carrying
`PEER_MAIL_WAKE`. A busy ego hears about mail at its next tool boundary through
`resources/updated` instead.

## The actor

One `ConnectionActor` per connection. Deciding and writing is serial — a single
task owns the decision of whether an operation is allowed and the write that
follows, so two callers cannot both pass a gate that only one of them should
have. Waiting is concurrent: replies are awaited in a `FuturesUnordered`, so a
turn that takes a minute does not hold up a cancel.

Agent→client requests (`session/request_permission`,
`session/create_elicitation`) arrive on the **same** channel as `session/update`.
That is deliberate: the question and the updates that explain what is being
asked about must stay in the order the agent sent them, and two channels would
reorder them.

The SDK hands a responder to a callback that holds the dispatch loop. Answering
there would freeze the connection for as long as a person takes to decide — so
the responder is carried out of the callback and parked. What the tests pin is
that it is always answered exactly once: by the person, by the cancel that took
the question away, or immediately when there was no seat to offer.

**Capabilities are checked on the way in too.** `initialize` advertises
`elicitation.form` and nothing else, so an `elicitation/create` in `url` mode —
or in one from a version this client has never seen — is declined on arrival
rather than seated. Seating is how rendering happens here: the request lands on
the attachment and a frontend draws the form it knows how to draw, which is
precisely what the protocol says a client must not do with a mode it does not
understand. A URL elicitation drawn as a form is worse than a refusal, because
the address it is actually asking about is what a form has nowhere to put.

**A bad answer never costs the person their question.** An answer is validated
before the seat is taken, so a permission option the agent never offered, or an
elicitation action outside the protocol's three, is refused with `invalid_input`
and the question stays open for a real answer. Forwarding either would spend the
seat on something the agent cannot read, and nobody could then try again.

## The journal

Every accepted callback and every local settlement is stamped with a sequence
number, appended, and only then fanned out. That gives a host two things a
snapshot cannot: the order the agent said things in, and the ability to join
late.

It is bounded (1024 events). A subscriber asking for a cursor that has fallen
off the end is told `stream_gap` rather than being resumed past the hole: the
missing chunks exist nowhere in this client, and a stream that silently skipped
them would render as a turn where part of what the model said never happened.
Recovery is a fresh connection and `session/load`, which replays from ego — the
only place that still knows.

Nothing in `append` can block the SDK reader: an uncontended lock and a
broadcast send that drops the slowest subscriber's view rather than waiting for
it.

## Capabilities are decided before the wire

The `initialize` response becomes an immutable capability snapshot. Every
operation is checked against it, and an unadvertised one is refused with
`capability_unavailable` **without sending a byte**. The same holds for content
blocks a session never said it accepts, for extra roots, and for ego's own
extensions — whose advertised version must match the one this client speaks.

This is why `capability_unavailable` is never retryable: the answer comes from a
snapshot taken once, so the identical request on this connection will refuse
identically. Reaching an agent that has it is a different action, not a retry.

There is no provider surface, and that is the same rule rather than a gap.
`providers/list|set|disable` are a draft the pinned schema keeps behind an
unstable feature, so ego does not serve them and never advertises them — and
`providers/set` would let whatever is connected rewrite the base URL that every
generation egresses to. Model, effort and mode arrive instead as
`session/set_config_option` options the session itself publishes, so a host
picks from what the agent offered rather than from a list this client made up.

A `config_option_update` may arrive after `session/new`, before any prompt.
It replaces the matching attachment’s complete option set and publishes the
connection snapshot before the update enters the journal. The frontend refreshes
that snapshot on the update, using the same path as an explicit set-config reply.
An empty offer clears the controls; other session attachments keep their options.

The snapshot is also what makes `method_not_found` fatal. Nothing reaches the
wire that the snapshot did not allow, so a method-not-found coming back is not
the agent declining — it is the agent disowning what it published, and the one
thing this client knows about it is no longer true. The caller is answered with
`protocol_violation`, and only then does the connection settle as failed: a host
that asked deserves to be told why, not just that the connection is gone.

When ego answers a prompt with its dedicated `-32011` pause error, the client
settles that turn and records the attachment as `paused`. The AI Chat control
bar therefore keeps Resume available until `_ego/resume` reports `running`.
Other prompt errors settle the attachment to `idle`, even if their message
mentions a pause; the numeric error code is the state signal.
Each prompt error also emits `turnFailed` with the agent's diagnostic and the
resulting attachment state. The event carries the turn ID in its envelope so
every subscribed view can show the failure in the conversation.

## Two transports, one set of judgements

`acp_commands.rs` and `acp_routes.rs` are both thin. Neither decides what is
allowed, validates a permission option, or invents a fallback — those are the
client's judgements, made once in `crate::acp`, so the desktop and a phone
cannot disagree.

The error body is the serialized `AcpClientError` on both. Over HTTP the status
is a translation of its `code`, never a second opinion: 400/404 for the caller,
501 when the agent never advertised the operation, 410 when the connection or
the cursor is genuinely gone, 502 when the agent answered with a refusal.

The one asymmetry is the stream, and it is a transport detail: a Tauri Channel
on the desktop, a WebSocket in the browser, carrying byte-identical frames.

## Lifecycle invariants

Written down because each of them is a variant the plan named and this client
deliberately does not have:

- **One live process per peer.** Concurrent connects for the same peer wait
  through launch and initialization, then reuse the registered connection.
  Connects for different peers remain independent.
- **A connection id is not public until initialization has succeeded.** It is
  minted inside `connect` and does not escape until v1 is negotiated, so there
  is no `starting` or `initializing` state — nothing that is still coming up can
  be named, listed, subscribed to, or asked about. Launch and negotiation are
  phases of the `connect` operation, not states of a connection.
- **A failed attempt is never registered and never settles.** It is answered
  with `initialization_failed` as an *error code*; there is no settlement reason
  by that name, because there is nothing there to settle.
- **A settled connection stays readable, and then is forgotten.** Ending is
  exactly when a host wants to read one — the reason, the attachments it had,
  the tail of its stream — so the last eight are kept. Kept forever they would
  be a leak: each holds a journal of up to a thousand events, and a `reconnect`
  loop against an agent that keeps dying is an ordinary thing to happen.
- **`transport_error` covers write failures too.** A child that closes its pipes
  or exits produces a write failure, a stdout EOF and an SDK shutdown at once,
  and which one the supervisor sees first is a scheduling accident. A separate
  `write_error` would be an attribution no one here can make.
- **A line that is not a frame does not end the connection.** The SDK owns the
  framing: an unparseable line is answered with JSON-RPC's `-32700` and the
  reader carries on, and nothing about it reaches this client. So the mistake is
  reported to the only party that can do anything about it — the agent that made
  it — and the stream stays in sync, because a line is a frame boundary and the
  next one starts clean. The exception is a malformed line shaped like a
  *response*, which the SDK drops in silence: answering an answer is not a thing
  JSON-RPC can do, so a request it might have been for waits out the connection.

- **A session is attached to a connection at most once.** A `load` or `resume`
  naming a session this connection already holds is refused with
  `invalid_input`, not merged and not re-run. The attachment is where the
  running turn, the usage totals and the ids of the questions a person has open
  live, and attaching writes a fresh one — so a second attach would blank the
  turn, and the response that settles it would arrive for a turn nothing names
  any more and be dropped as stale, leaving a host watching a turn that never
  ends. The seats, meanwhile, outlive the overwrite, so the two views would then
  disagree about what is being asked. A host that wants the history replayed
  detaches first, which says what it means. A fork is not affected: it names the
  session it forks *from* and comes back with an id of its own. A `load` or
  `resume` still waiting for its answer holds the session too, so a second one
  sent meanwhile is refused the same way: each load makes ego admit every MCP
  server again. Every attach is logged (`ACP attach`, with the wire method and
  session id) so a storm of them can be attributed. The AI Chat panel keeps one
  replay per tab in flight and does not replay a tab whose load failed until a
  person selects it or presses Retry.
- **Configured conversation reopen preserves live output.** `acp_chat_open`
  reports whether history was loaded using `replayed`. The client keeps the
  existing transcript while the request is pending or refused. On a genuine
  replay, it clears the projection before subscribing to the returned journal;
  an already attached conversation keeps chunks received during the request.
- **A replay starts before the load response.** Ego may send `session/update`
  chunks while `session/load` is pending. The requested session ID is already
  known, so those updates enter the ordered journal before the attachment is
  published. Updates for other unattached sessions are still discarded. Usage
  received during the replay is included in the resulting attachment snapshot.

Changing the first two means a different, asynchronous connect API — one that
hands back an id before there is a connection behind it, with its own
cancellation, retention and ownership rules. That is a decision to make when
something asks for it, not a set of enum variants to leave lying around.

## Process authority

**Shared runtime decision (2026-11-06, #2859-fdff):** AI Chat keeps separate processes for distinct peers. An isolated ego 0.4.3 raw ACP probe measured later same-process `session/new` calls at 37–41 ms, versus 109–255 ms for separate-process launch, initialization and first-session creation; these samples exclude MCP and model prompts and are not TUIC end-to-end measurements. Pooling is not implemented: `src-tauri/src/acp/manager.rs` sets the peer's `TUIC_SESSION` on the process launch (near line 304), while ego's `crates/ego-tools/src/exec/backend.rs:152–170` takes inherited tool environments from that process. Separate listener connections can preserve MCP peer identity, but cannot give simultaneous tool subprocesses different host identities. A future ego per-session host-environment extension would need an explicit negotiated contract that binds the host-issued identity to each session's execution and subprocess environment, keeps concurrent sessions isolated without process-global environment changes, and defines identity ownership across load, fork, compaction and reconnect. It would also need to preserve per-session MCP routing and cancellation. The coordinator rejected pooling without that capability; this is not a measured ego defect.

The ego binary is the `ego_executable` setting, read at each `connect`. It is
not held by the manager — a copy taken once would keep launching the previous
binary after somebody corrected the setting, without ever saying so — and it is
not an argument of any command or route, so no request body can name what this
machine runs. An empty setting refuses every connect, because "not configured"
and "configured wrongly" are different things to be told.

`POST /acp/connections` and `.../reconnect` are the only routes that launch a
process, and the only two that take the loopback-or-authenticated guard.

## Notices

`AcpEventJournal::append` is the single place every event is stamped, so it is
where the wake signal is derived. Four kinds — `ready`, `settled`,
`interaction_pending`, `interaction_settled` — become an `AppEvent::AcpNotice`
that `spawn_acp_notice_pump` mirrors to the desktop window and to `/events`.

A turn's chunks never ride that bus: one turn emits more of them per second than
the 256-entry broadcast can carry without lagging every unrelated subscriber in
the app. A notice says where to look; the payload stays on the stream, the
snapshot, or the interactions list.

`settled` covers both a turn finishing and a connection ending. They are told
apart by whether the notice names a session, not by a fifth kind.

## Tests

`src-tauri/tests/story092_*.rs`, against `tuic-acp-fixture-agent` — a
non-bundled binary that replays a `tests/fixtures/acp/*.jsonl` scenario through
the exact production launch path. `tests/fixtures/acp/ego-initialize.json` is a
recording of what real ego answers, copied from ego's own committed golden;
editing it to make a test pass would turn the recording into a wish.

### Mid-turn text steering

The shared Rust connection actor routes text-only `acp_session_prompt` submissions
through advertised `agentCapabilities._meta.ego.steer` v1 (`_ego/steer`, text
content). Only a running, prompting attachment with an empty FIFO qualifies. Accepted input returns
the existing turn ID, emits no `PromptSent`, and appears through ego's
`user_message_chunk`. Steering reserves its submission position in the existing
FIFO until the response arrives; later submissions queue behind it and cannot drain
past it. Accepted or uncertain outcomes consume the reservation. `not-busy` and
`rejected` retain it for normal prompt serialization;
attachments and absent or mismatched capabilities retain the queue. An uncertain
transport outcome is reported without retry because acceptance may be durable.
Only the `_ego/steer` request has a 10-second deadline (`EGO_STEER_TIMEOUT`):
this is a durable append that ego normally acknowledges in milliseconds. A timeout
reports that delivery is uncertain, releases the FIFO reservation through the
existing error settlement, and never resends the text. Other ACP requests retain
their existing behavior.

## Repository profile ceiling

Desktop `acp_session_new`, HTTP session creation and new conversation-specific
chat launches call the same
`acp_commands::session_new` core. The manager reads `.tuic.json` at the canonical
session cwd. If it contains `ego_profile`, the request includes
`_meta.ego.profile` and the machine Settings selection as `ceilingProfile`.
An empty machine selection refuses creation. TUIC does not calculate policy.
Ego resolves and restricts mode, sandbox, roots and executable paths.

TUIC requires ego to acknowledge both selected names, an effective policy object
and a string-array `warnings` result before attaching the session. This prevents
an older ego that ignores the extension from being exposed as a bounded session.
Attachment snapshots carry nonempty warnings as `profileWarnings` on both
transports. AI Chat shows them once per visible conversation through its existing
warning notice path. With no repo selection, requests and snapshots retain their
previous shape. Unattended sessions retain their existing launch behavior.
Load, resume, fork and compact rely on ego's durable ceiling, rather than sending
a replacement authority from the current repository file.

When ego reports a failed session setup (`config_option_update` with
`_meta.ego.configOptions.state = "failed"`), the attachment snapshot keeps ego's
reason as `configFailure` on both transports until a later config update
replaces it. The managed-launch readiness wait and the workflow model adapter
read it from the snapshot, so a failure sent before a wait subscribes ends that
wait at once with ego's error instead of at the config deadline.


Ego settings (#1775-330c): model/provider defaults and perimeter reads/writes now
share one captured trusted executable, configured profile and AI Chat workspace
per operation. These owner CLI calls do not change any live ACP session. The
internal settings adapter preserves this distinction; launch defaults stay in the
`app` store and apply to later launches.


### Session command offers

`AcpAttachmentSnapshot.availableCommands` holds the latest complete
`available_commands_update` offer, serialized identically over IPC and HTTP.
Every offer replaces the prior list, including an empty list. The ordered stream
continues to carry the original update; the frontend projects it into its owning
attachment, and a snapshot restores the list for a late subscriber.

Initial offers can reach the actor before a new/fork response settles, even when
the agent wrote the response first. The actor retains command metadata for those
pending attachments and for claimed load/resume ids, then adopts only the offer
whose id matches the successful attachment. The holding map is bounded by pending
attachments, never creates an attachment, and drops unclaimed entries when no
unidentified attachment remains. Normal unknown-session transcript updates remain
ignored. The composer sends advertised agent invocations as unchanged slash text;
its host actions call existing chat handlers instead.

## Workflow launch readiness

A workflow launch allows up to 30 seconds for its attached conversation to
advertise the requested effort and model values. Offers may arrive after
`session/new` through `config_option_update`. Effort is applied first because
it can change the model offer. Readiness waits release the global launch lock;
configuration reacquires it and reloads and validates the retained launch before
applying settings. Unrelated launches, receipt reads and prompt delivery remain
available during the wait. The absolute deadline is saved with the launch;
interrupted waits retain the same launch key, connection, generation and
conversation. Missing live inventory is uncertain and never starts a replacement.

The offer event wakes a fresh attachment check. Missing options or values still
return the existing not-offered error when the deadline expires. Binding commits
before prompt delivery. Once readiness succeeds, cancellation that won during
the wait prevents binding and disconnects the unbound child.
