# Partial live outcome recording (#2010-30a7)

This is **not** the required four-outcome regression fixture. Permission denial
and rendering success are missing. Do not mark the captured-outcome criterion
complete or use a successful status response as rendering evidence.

## Source and extraction

`outcomes-partial-2010.json` was recorded from the running desktop instance at
2026-10-19 15:18 UTC. `GET /api/version` reported `c281128313`. Requests used
HTTP over the authorized local Unix socket, not TCP authentication or a second
application instance. The JSON file was generated from actual endpoint results;
no response or audit row was hand-written.

SHA-256 of the JSON fixture:
`cb8cb15f698f9129044860bae2f76032ff506597389019ba10ec1bac1138b4de`.

The recording keeps selected `/mcp/audit` rows unchanged, version information,
client exit codes, and five speech prerequisite fields. It excludes MCP response
bodies, error text, arguments, other callers' audit rows, and inbox content.
The capture script and local wire evidence are retained in
`~/Gits/.tmp/results/feat-2010-live/` on the recording Mac.

## Observations

- An actual ego-selected `agent.list_peers` call addressed this peer's own
  host-issued identity, `c2a21fed-a788-4c48-83db-be662c6241f7`, with `compact=true`.
  Its exact target digest and start time identify the recorded row. Ego supplied
  client UUID `01a154bc-6e5c-75c0-a222-128c9f7bb7f1`; TUIC retained it alongside
  server UUID `54abdfd2-d2bd-4d2f-808a-41bd66703bf3`. The recorder did not inject
  this client UUID. This is selected-call correlation evidence, not an independent
  client diagnostic trace or proof that every ego transport correlates calls.
- A `session.status` request for a fresh nonexistent UUID returned an application
  refusal. The server recorded `application_error=true` and `effect=unknown`.
  A refusal without an explicit effect contract does not prove rollback.
- A read-only `session.wait` for this peer to exit had a 1000 ms server deadline.
  Curl disconnected after 154 ms with zero reply bytes and exit 28. The server
  recorded `cancelled_before_reply`, null bytes/application outcome, and unknown
  effect. No inbox reader, acknowledgement, or cursor was used.
- Speech status reported unavailable, no utterance, and no active rendering.
  Hands-free was not armed. No microphone, speaker, user session, or configuration
  was changed to manufacture a rendering result.

## Ego coordination and remaining prerequisite

The existing ego consumer generates `params._meta["tuic/call_id"]` once in
`crates/ego-mcp/src/operation.rs::operation_request`; the selected HTTP guard
binds that same metadata. Its guarded-effect regression is
`selected_tool_correlation_is_fresh_at_guarded_http_effect`. The server accepts
the same field in `mcp_call_audit.rs::Call::start`. This recording confirms that
a real selected call reaches the restarted server with a client UUID. No ego
change or new ego story is required by this observation. No parent intent is
invented, and optional OTel export remains parked.

## Isolated-capture preflight (2026-10-22)

The proposed `make agent-test AGENT_TEST_INSTANCE=agent-test-2010
AGENT_TEST_PORT=19887` capture cannot supply the rendering-success case.
`src-tauri/src/agent_test.rs::enable` rejects builds with the `dictation`
feature. Without that feature,
`src-tauri/src/mcp_http/mcp_transport_ancillary.rs::handle_voice` selects the
no-audio implementation: an unbound caller is refused, and a bound caller
receives `available=false`. Neither path creates a speech utterance or reports
`rendering`. Changing client permission mode cannot change this build boundary.

The launcher dry run confirmed that dictation/microphone are disabled. No
native build, desktop launch, or new client capture was started after this
prerequisite failed. The existing JSON remains unchanged; this preflight is
source evidence, not a new runtime fixture. Completing the capture needs an
approved isolated audio-capable setup. Do not remove the agent-test safety
check or use the daily instance to manufacture the missing outcome.

A full recording still needs a real client permission-denial event and a real
rendering response from an owner-armed hands-free conversation. This peer runs
in yolo mode and has no permission-denial event to record. Do not replace that
case with a server application refusal. Do not inject a rendering object through
JavaScript, a mock server, or a hand-written fixture. Until those observations
exist, no four-outcome RED or GREEN result is claimed. The original null-error
regression and its historical RED remain separate evidence in the story.
