# Desktop access to a remote daemon through the relay

- Story: `2085-721d`, acceptance criterion 1 only.
- Date: 2026-10-20.
- Status: **Proposal. Boss approval is required before implementation.**
- This note does not mark criterion 1 complete: that criterion includes approval.
- Scope: one native desktop client and one already-installed headless daemon per relay room. No code changes, deployment, mobile onboarding, or hosted-service claim.

## 1. Result and roles

The desktop must connect to a daemon that has no inbound route from the desktop. Both endpoints make outbound WSS connections to a relay. After endpoint authentication, the desktop uses the same remote connection, HTTP handlers, events, and terminal protocols as a direct connection.

```text
Desktop app                  Relay                         Remote host
remote_runtime               tools/relay                   tuic-remote
    | outbound WSS              | outbound WSS <---------------- |
    +-------------------------->|                                |
    |<====== authenticated, encrypted endpoint channel =========>|
    |                                                            |
local loopback listener     opaque forwarding          fixed daemon listener
    |<------ multiplexed HTTP / SSE / WS byte streams ----------->|
```

| Role | Owns | Must not own |
|---|---|---|
| Desktop client | Saved connection ID/name, endpoint pairing secret, daemon credential, local tunnel listener, remote UI projection | Remote PTYs, remote filesystem state, a second implementation of daemon commands |
| Remote daemon | Repositories, sessions, PTYs, existing request handlers/auth, outbound relay agent, fixed local upstream target | Desktop window state, arbitrary relay-requested TCP destinations |
| Relay | Bearer admission, room occupancy, bounded forwarding, peer-loss hints | Pairing secret, daemon credential, application plaintext, command execution |

The daemon must already be running and configured through host access or a trusted administrator. A relay is not a way to install a daemon on an inaccessible host. Start with an installed daemon; do not add relay-driven SSH deployment or automatic daemon upgrades in this story.

## 2. Observed baseline, not product acceptance

The starting audit is the orchestrator report `reports/tuicommander/2026-10-16-cloud-relay-status.md`, audited at `904970561bcc1d271558c08e5094687a684f11b6`. It says that the hosted service was not deployed and that the prototype was not end-to-end encrypted. No new hosted-service probe was made for this note.

This note also checks source at `8d6c4430c2a1e1698fb8a8e7b6f6e0e2d0e9f579`. Important differences and seams:

| Source | Observation |
|---|---|
| [`tools/relay/src/routes.rs`](../../tools/relay/src/routes.rs), `build_router`, `register` | The relay has `/register` and `/ws/{session_id}`. Registration issues a relay bearer, not an endpoint identity or pairing secret. |
| [`tools/relay/src/relay.rs`](../../tools/relay/src/relay.rs), `handle_ws` | The current source requires the first `Bearer` frame and atomically checks room ownership, capacity, and token limits. The old audit's admission bugs must not be described as all still present. Rooms still allow only two peers. |
| Same file, `forward_to_others` | `try_send` still silently drops a full peer queue. This cannot carry a reliable tunnel without a defined close/backpressure policy. |
| [`src-tauri/src/relay_client.rs`](../../src-tauri/src/relay_client.rs), `derive_key`, `connect_and_run` | Encryption still derives from the relay bearer. The decrypted command branch still logs that dispatch is not implemented. Its mobile event bridge is not a remote-daemon transport. |
| [`src-tauri/src/remote_connection.rs`](../../src-tauri/src/remote_connection.rs), `RemoteTransport` | Saved transports are SSH and Direct. The store and existing connect/disconnect commands are the connection-management seam. |
| [`src-tauri/src/remote_runtime.rs`](../../src-tauri/src/remote_runtime.rs), `resolve_base_url`, `authenticate`, `teardown` | SSH already resolves to a loopback HTTP URL. Runtime owns status, auth, version/health probes, supervision, and generation-safe teardown. |
| [`src/transport/http.ts`](../../src/transport/http.ts), `owningConnectionFor`, `rpc`; [`src/transportRuntime.ts`](../../src/transportRuntime.ts) | Repository/session ownership selects the connection; HTTP mappings and remote base URL/token select the daemon. |
| [`src-tauri/src/remote_mirror.rs`](../../src-tauri/src/remote_mirror.rs), `seed`, `consume_stream`, `store_seed` | Existing mirroring seeds `GET /sessions`, consumes `/events`, replaces owner-scoped rows, and re-seeds on reconnect. |
| [`src/components/Terminal/canvasTerminalTransport.ts`](../../src/components/Terminal/canvasTerminalTransport.ts), `WsTransport` | Remote terminals use the connection's URL/token and the existing grid WebSocket, including compression negotiation and stale-socket guards. |
| [`src-tauri/src/lib.rs`](../../src-tauri/src/lib.rs), `spawn_daemon_background_tasks`, `run_remote` | Headless startup has its own task list. A daemon-side outbound tunnel supervisor must be added explicitly; the desktop relay supervisor is not evidence that it runs there. |

Source inspection is not a fresh build, test run, or end-to-end demonstration.

## 3. Pairing and trust boundary

### Separate three credentials

1. **R: relay admission bearer.** The relay issues it and receives it in the first WSS text frame. Initially the two peers share the same R because current room ownership requires the same token. R grants relay capacity only.
2. **P: endpoint pairing secret.** The daemon generates 32 random bytes with the OS CSPRNG, independently of R, room ID, passwords, and daemon tokens. P authenticates the encrypted desktop-to-daemon channel. The relay must never receive P or a key derived from P that permits decryption.
3. **D: existing daemon API credential.** Keep the daemon's existing authenticated HTTP boundary. P is not a replacement for the HTTP auth middleware, and R must never authenticate daemon requests.

P and D are separate values with separate storage. Existing `RemotePairingToken` is a daemon API credential used by `remote_runtime::authenticate`; despite its name, it must not silently become P. `run_remote` can set the daemon session token from its pairing-token option. It does not provide per-client API revocation today.

### Proposed first pairing sequence

1. On the remote host, the operator enables a new relay pairing through a host-local setup operation. It obtains R through the existing relay registration contract (or accepts an operator-provisioned R), chooses a random room ID, and generates P. This is an explicit setup action, not a registration on every reconnect.
2. Store R, room, relay origin, P, pairing version/ID and daemon credential association in instance-scoped backend storage. Secrets use the existing credential-storage abstraction, not plaintext `connections.json` or ordinary settings exports. A host without a usable credential backend needs an explicit protected-file or ephemeral setup choice; never silently write plaintext secrets.
3. The operator transfers a versioned pairing bundle to the desktop by a trusted channel independent of the relay: a file copied through authenticated SSH, a local QR, or a deliberate copy/paste from the host console. It contains the relay WSS origin, room, R, pairing ID and P. Neither registration nor the relay WebSocket URL contains P.
4. The native desktop imports the bundle into an ordinary saved connection with proposed `Relay` transport metadata. Store P separately from public connection data. Do not upload the bundle, put it in process arguments, or log it. Do not load pairing code from a relay-served page.
5. Both peers connect outbound to `/ws/{room}`, authenticate to the relay with R, then perform mutual endpoint authentication using P. The desktop is the initiator; the daemon is the responder regardless of arrival order. A plaintext `relay:status connected` is only a hint to start that handshake, never proof of daemon identity.
6. Only after endpoint authentication, the daemon supplies D inside the encrypted control channel to the paired desktop backend. The desktop stores D through the existing remote credential mechanism and runs the ordinary health/version/auth probes through the tunnel. On reconnect, refresh D over that authenticated channel if the daemon has changed it. Do not expose D in outer relay frames. Giving D to a holder of P deliberately grants the same full API authority as a direct paired client.
7. Advertise `connected` only after those probes succeed. Start the existing mirror and allow frontend routing at that point, not at WSS open.

A copied bundle is a capability: anyone holding it can impersonate either endpoint for that pairing. Initial scope is one desktop/daemon pair, not multi-user authorization. Delete pairing means remove P on the daemon, terminate its channel, and delete the desktop's stored association. Rotate P and use a fresh room/R after bundle exposure. Revoking P alone does not revoke an already disclosed D; rotate the daemon credential too, with an explicit warning that other direct clients using it must re-pair. Do not claim per-client revocation until the daemon supports it.

### Encrypted channel requirements

Use a versioned, reviewed PSK-authenticated handshake with fresh session keys and mutual key confirmation. Recommendation: evaluate a maintained Noise PSK implementation (for example the `NNpsk0` pattern) rather than inventing a handshake around the prototype's `encrypt` function. The exact suite/library, dependency/license impact, record limits and handshake transcript must be reviewed and fixed before the crypto implementation step. This note is not an assertion that such a library is installed or verified.

Bind protocol version, pairing ID, room and expected roles into the authenticated handshake context. Use separate directional keys and ordered authenticated records. Each new attachment uses a fresh handshake/epoch; reject reflected frames, duplicate/out-of-order records, unknown versions, and old-epoch data before opening upstream sockets. Bound handshake time and record length. Never fall back to bearer-derived v1 encryption after a handshake failure. Do not claim forward secrecy without checking the selected handshake's properties.

The relay sees R, room, IPs, timing, sizes, and attachment metadata. It can deny service, substitute peers, reorder/drop data, or lie about presence. It must not be able to authenticate an endpoint, derive P, decrypt payloads, or replay commands into a new epoch. Endpoint compromise or theft of the bundle is outside that guarantee. WSS protects each hop; endpoint authentication protects against an active relay. Native client distribution and the host setup channel are trusted. A future web client needs a separately trusted origin: relay-served JavaScript could steal P even if it arrives in a URL fragment.

No push hints with session names or command data are needed for this desktop tunnel. Keep push/mobile work separate.

## 4. Reuse the direct-remote contracts

### Recommended adapter: a multiplexed byte tunnel

Add a backend-owned local loopback listener per active relay connection. It returns an ordinary `http://127.0.0.1:<port>` base URL, just as the SSH path does. Each accepted TCP socket becomes one logical stream inside the authenticated relay channel. On the daemon side, `Open(stream_id)` connects only to the daemon's own configured loopback HTTP listener. The peer cannot supply a hostname, port, Unix socket, executable, or Tauri command name.

The new wire vocabulary is transport control only: `Open`, `Opened`/`OpenFailed`, `Data`, `WindowUpdate`, `Fin`, `Reset`, and authenticated liveness/control messages. Stream IDs are unique within an epoch. Data preserves byte order; half-close and cancellation are explicit. Tunnel frames are encrypted before the relay sees them. TCP socket lifetime provides HTTP response correlation; existing HTTP and WebSocket protocols retain their own request/frame semantics. There is no new application command/reply envelope or relay-specific handler dispatcher.

Keep daemon authentication enabled. The listener is not a trusted-local IPC bypass: the bytes terminate on the existing authenticated headless TCP server. The frontend and backend keep using D through current query/cookie/header handling. P and R never reach frontend URL builders. HTTP requests, status codes, error bodies, SSE framing, WebSocket upgrade/subprotocol headers and binary frames pass through unchanged. The desktop listener binds loopback only and lives only as long as that connection generation; it is not a relay-side HTTP gateway.

| Desktop operation | Existing contract to carry unchanged |
|---|---|
| List remote repositories | `load_repositories` mapping → `GET /config/repositories` |
| List sessions and seed badges | `list_active_sessions` / mirror → `GET /sessions` |
| Open a terminal | `create_pty` mapping → `POST /sessions`, same request and `{ session_id }` reply; existing mapper extracts the ID |
| Input and resize | Existing `/sessions/{id}/write`, `/resize` contracts; keep current write ordering/coalescing |
| Terminal output | Existing `/sessions/{id}/stream?format=grid` WS, full initial frame, deltas, ACKs, compression subprotocol |
| State and lifecycle | Existing `/events` SSE, mirror owner tagging, reconnect seed and registered frontend consumers |
| Errors and compatibility | Existing auth, health/version probes, route errors and `HttpRpcError`; no application-level translation |

Reuse the same `connection_id` for repository/session ownership. Existing IPC calls for remote-owned work must continue through `owningConnectionFor` and the HTTP table, not accidentally invoke local Tauri handlers. Keep desktop-only commands intentionally unmapped. The tunnel exposes only what the headless daemon already exposes; it does not add desktop routes to it.

Backend consumers such as `remote_mirror`, version probes and remote file operations also use the loopback route. This is why a frontend-only relay `rpc()` branch is insufficient. Preserve the existing IPC/HTTP connection-management parity when extending the saved transport union and status metadata; no new command surface is needed for relay business operations.

Keep this pairing and its room separate from `services.relay` and the existing mobile event bridge. Do not repurpose that global desktop configuration as a saved remote connection or silently upgrade its bearer-derived protocol. The new version must fail closed with old peers; direct/SSH connections and the existing bridge retain their current explicit behavior until separately migrated.

### Boundedness and congestion

The relay's current 32-entry queues must not silently discard tunnel records. Initially close the affected room/attachment on queue overflow and notify both peers; closing only one forwarding direction can leave the other endpoint falsely connected. Never continue a byte stream after loss. Endpoint detection of missing ordered records and authenticated heartbeat deadlines is a second protection, including against a malicious relay.

Proposed initial limits, not measured performance claims: 32 logical streams, 16 KiB data chunks, and 64 KiB receive credit per stream. That is at most 2 MiB of credited data per receiving endpoint, plus separately bounded control, cipher and socket buffers. Charge queued bytes globally as well as per stream; queue counts alone do not bound memory. Bound outer WS message size before allocation. Reserve control capacity and schedule data fairly so one busy PTY cannot indefinitely block health replies or input. On credit exhaustion stop reading that socket; on a stalled peer close after a bounded deadline. These limits require integration tests before being treated as supported capacity.

### Relay policy as implemented

- A full peer queue waits up to 5 s (`peer_send_timeout`), then the relay closes the room with code 4004. No frame is dropped silently.
- The relay pings every peer every 20 s and reaps a peer after 60 s of silence. A Pong counts as activity, so an idle but live peer is never reaped.
- The queue-overflow close and the reaper apply to every room, including the phone rooms of the existing event bridge, not only tunnel rooms.
- Outer WebSocket messages are capped at 1 MiB before allocation.
- The Noise prologue binds protocol version, pairing ID and room, not the relay hostname, because the two ends may reach one relay under different names.

## 5. Failure, reconnect and session identity

Use `RemoteStatus` for the public connection state and typed relay phase/reason metadata for detail. Proposed phases are `dialing`, `waiting_peer`, `authenticating`, `probing`, `ready`, and `retry_wait`; reasons include `relay_unreachable`, `peer_absent`, `admission_rejected`, `pairing_rejected`, `congested`, `daemon_unreachable`, and `protocol_mismatch`. Carry any new status fields on the same dual-emitted IPC/SSE `remote-connection-status` payload, with no secrets.

| Situation | Public state and behavior |
|---|---|
| Dialing, alone in room, handshake, probes | `connecting`, with a typed phase. No routable base URL/token until ready. |
| Mutual authentication and daemon probes pass | `connected`; publish the route, start one mirror and normal terminal sockets. |
| Relay loss, peer loss, authenticated heartbeat timeout or overflow | Immediately leave `connected`; `disconnected` with typed reason, then `connecting/retry_wait` while automatic retry is enabled. Close all old logical streams and fail pending I/O. |
| Wrong P, revoked P, invalid D or rejected R | `unauthenticated`, with a reason that distinguishes admission, pairing and daemon auth. No endless retries with the same rejected credential. |
| Incompatible protocol or invalid framing | `error`, typed reason, no legacy downgrade. |
| User disconnect/delete/disable | `disconnected`; existing `teardown()` retires the generation, closes listener, tasks, streams, mirror and credentials-in-memory. No retry may resurrect it. |
| Daemon restarts | Old streams fail. New handshake and probes, refresh D if needed, then re-list repositories/sessions and reopen subscriptions. Never infer that a prior PTY survived. |

Reuse the runtime's single claimed supervisor and retirement generations. The relay adapter must report loss promptly into that owner, not create a competing reconnect loop that bypasses explicit disconnect. Reuse bounded exponential backoff with jitter; distinguish successful WSS admission from successful endpoint/daemon readiness before resetting it. Proposed endpoint liveness: authenticated ping every 15 seconds, consider the channel dead after 45 seconds without an authenticated response. Relay-generated status alone cannot refresh that deadline. Setup budgets are separate from liveness budgets.

Reconnect means **reattach**, not recreate. Keep daemon-issued session UUIDs and owner-scoped UI identities. Reuse the mirror's replace-on-seed behavior and terminal initial-frame recovery. Do not replay old grid deltas, HTTP requests, terminal input, or `POST /sessions` after tunnel loss. If the daemon executed a mutation but its reply was lost, report an uncertain outcome and reconcile with current daemon state; do not automatically retry it. Existing handlers do not establish exactly-once creation across a broken connection. An operator can inspect the re-listed sessions before deliberately creating another one.

“No duplicate sessions on reconnect” therefore means the reconnect path performs no create request. It does not promise exactly-once retry of an interrupted user action. If transparent mutation retries become a requirement, an idempotency contract is a separate design decision, not a hidden relay feature.

Relay room attachment does not itself count as a live user for idle lifetime. Existing HTTP SSE/PTY subscriptions reach the daemon and use its existing client counters. Loss must close those upstream sockets. Recommend installed-daemon operation for this scope; if the operator chooses a finite survive time, warn that it may expire while only waiting in a relay room and desktop reconnect cannot restart that host process.

## 6. Open choices for Boss

All boxes are intentionally unchecked. Recommendations are not approval.

- [ ] **Transport adapter: use the backend loopback byte tunnel (recommended).** It preserves HTTP, SSE, WS and backend consumers with one adapter. Cost: logical stream framing, flow control and a loopback listener. Alternative: relay-native RPC plus explicit event/PTY streams; it avoids the listener but adds adapters to every frontend/backend consumer and risks a parallel command surface. Do not choose a new command dispatcher.
- [ ] **Pairing: host-generated, high-entropy bundle through a trusted independent channel (recommended).** It requires one host-side setup action but works without inbound access. Alternative: manually provision the same random secret on both endpoints. A short relay-issued code alone is not an independent secret; a short-code PAKE flow would need a separate reviewed design.
- [ ] **Crypto: select a maintained PSK protocol implementation before coding (recommended).** It adds a dependency/review decision but avoids a custom cryptographic handshake. Alternative: design a protocol with existing ring primitives; this needs a precise transcript, nonce/replay/key-confirmation specification and independent crypto review. Never retain bearer-derived encryption as fallback.
- [ ] **Initial topology: dedicated two-peer room, one installed daemon and one desktop (recommended).** It matches current capacity and limits lifecycle ambiguity. Multiple desktops require separate pairings/rooms and a separate review of daemon-credential revocation; increasing room capacity alone is not sufficient.
- [ ] **Recovery: fail in-flight mutations and reattach without replay (recommended).** This preserves current handler contracts and prevents reconnect-created duplicates. Transparent retry would require backend idempotency and receipt semantics, beyond this note's scope.

Approval must record the selected options and any changes to the scope. Only then can criterion 1 be checked and implementation begin.

## 7. Smallest implementation sequence after approval

Each step must preserve a runnable direct/SSH path; no deployment is required to validate the tunnel.

1. **Freeze the wire/pairing contract and repair reliable forwarding.** Select/review the PSK library and fixed protocol version, encode bounds/roles/epoch rules, and specify admission versus endpoint errors. Change relay overflow handling from silent drop to attachment failure. Keep the already-repaired admission transaction. Tests: full queue disconnects both sides rather than corrupting a stream; a third peer cannot displace a valid pair; same-token second peer is permitted at the room cap; foreign token rejected. These protect named loss/admission regressions, not new application behavior.
2. **Add daemon pairing and outbound lifecycle.** Add host-local setup/import, credential persistence, explicit daemon task startup and shutdown, and a connection to the existing relay endpoint. No inbound firewall change. Tests: R is independent of P; setup/reconnect never transmits P; wrong P never opens an upstream socket; missing/failed credential storage does not silently persist plaintext. Use isolated instance storage and injected credential stores, not the operator's keychain.
3. **Add the encrypted stream adapter and desktop transport variant.** Implement bounded multiplexing, fixed upstream target, loopback listener and runtime-owned teardown. Wire it through existing connection save/connect/disconnect HTTP and IPC handlers. Tests: HTTP reply correlation on concurrent sockets, WS upgrade plus binary data, incremental SSE delivery, half-close/reset, oversized frame rejection, ordered/replay/epoch rejection, flow-control isolation, delete-during-handshake cannot resurrect a route. Compare direct and relayed status codes/bodies at the existing router boundary.
4. **Attach existing consumers and UI state.** Feed the listener URL/D through normal health/version/auth and ownership routing; extend typed status projection only. Keep current mirror and terminal consumer code unless a failing boundary test proves an adapter change is needed. Tests: repository/session routing stays on the remote owner, local commands remain local, stale-epoch frames do not repaint a replacement terminal, re-seed removes vanished sessions and preserves surviving UUIDs. Update the Remote Connection Manager, daemon and API/config docs identified by the [sync matrix](../sync-matrix.md); do not advertise the feature as shipped before acceptance.
5. **One local end-to-end harness, then failure cases.** Run a standalone local relay, isolated headless daemon and a desktop transport client using the production runtime adapter, not the daemon's API directly. Make the daemon address unavailable to that client except through the adapter. Register/attach outbound, list a fixture repository and sessions, create one PTY, send a deterministic command through the existing submit/input contract, and assert output through the existing terminal stream. Wrong P must fail before any command executes. Drop/restart the relay and restart the daemon, reconnect, and prove no extra create request, no duplicate session row, clean initial terminal state, typed disconnect and successful recovery. Count daemon-side creations to catch invisible duplicates, not only visible rows. Drop the reply after a create to prove uncertain mutations are not retried. Use setup budgets separate from asserted disconnect deadlines; retain harness logs with secrets redacted.

Existing source tests to retain include `remote_mirror::tests::every_stream_reconnect_re_reads_the_session_list`, `a_stream_that_stops_speaking_is_retired_rather_than_awaited_forever`, and the remote-lifetime counter tests. They protect unchanged consumers; they do not prove a relay implementation exists.

Implementation validation belongs to the coordinator's batch gates. Rust commands use the rb build-slot runner and targeted filters; frontend checks use the local build-slot runner. No Rust build, frontend suite, desktop instance, browser, or network registration was started for this documentation task. A design review cannot substitute for the later integration test in criterion 5.
