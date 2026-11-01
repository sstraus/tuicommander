# Settings startup workflows and current limits

This document describes the published contract, not a claim that every startup
acceptance check has passed. The native inventory gap ledger and UI parity
ledgers have different purposes: `inventoryComplete` reports the native ledger,
not live UI convergence or complete authentication onboarding.

## Discover before editing

1. A person enables the `settings` MCP tool in trusted Settings UI. A startup
   model cannot enable its own access. The removed `config` tool has no alias.
2. Discover `settings` through the current MCP tool surface, then call
   `{"action":"describe","store":"app"}`. Follow `nextCursor` until it is absent.
   The response includes Rust-generated field metadata and the action schema.
   Use the current schema, not a saved list of paths or action names.
3. Read the required fields with an explicit target. `machine` addresses the
   current machine; `instance` addresses instance configuration; `repository`
   requires a registered repository ID; Ego `profile` requires the configured
   profile ID. A remote connection is not implicitly selected by a local request.
4. Preserve each owner's read revision. A set/reset or per-store patch uses
   `expectedRevision`. Never combine different stores in one patch. Stable
   collection keys are not array indexes or arbitrary file paths.

The opt-in `settings-ipc-test` Cargo feature tests the generated
`settings_action` command with Tauri's mock runtime and real `InvokeRequest`
argument extraction. Run the targeted
`settings_generated_mock_ipc_agrees_with_mcp_and_http` test through the repository
test wrapper. It compares durable notification edits across MCP, HTTP and mock
IPC, rejects a missing `{request: ...}` wrapper without writing, and preserves
native access when MCP is disabled. It does not start the desktop setup closure,
open an OS WebView, or prove the installed app's registration and ACL state.
Live desktop parity remains a separate check after a Boss-controlled restart.

Tauri `settings_action`, HTTP `POST /settings/action`, and MCP `settings` use the
same request DTOs and owner services. MCP mutation is loopback-only. HTTP keeps
its local-or-authenticated caller policy; native IPC keeps host-issued trust.
Do not treat these different caller policies as interchangeable credentials.

## GitHub

Describe `github_accounts` and `github_bindings`, and inspect the published
`perform` operation schema. Workflow requests use `target:{"kind":"machine"}`
and `store:"github_accounts"`, including repository binding operations.

| Operation | Required behavior |
|---|---|
| `begin_cloud_login` | Return an opaque `flowId`, `verificationUri`, `userCode`, `expiresIn` and `interval`. Only the person authenticates with GitHub. `additional` selects an additional named account. |
| `cloud_login_status` | Read the host-owned flow state; it does not exchange the device credential with GitHub. |
| `complete_cloud_login` | Attempt a poll when the owner's interval permits. Handle `pending`, `busy`, `expired`, `access_denied` and `success`; `pending` is not authenticated. A slow-down updates the returned interval. |
| `add_enterprise` | Verify the host and PAT through the account owner before storing it. A github.com PAT cannot bypass cloud device login. |
| `set_label`, `remove_account`, `set_secret`, `clear_secret` | Use the existing account owner. Cloud token replacement is not an Enterprise PAT workflow. Reads expose presence, never token values. |
| `bind_repo`, `unbind_repo` | Use a registered repository, canonical remote and the binding owner's `expectedRevision`. Do not substitute the account snapshot revision for the binding revision. |

Do not persist a device credential, poll a completed flow again, or infer success
from a browser closing. Unknown flows are `not_found`; expiry or disconnect can
invalidate an existing flow. A successful durable account action includes an
owner snapshot for readback. A credential source such as environment, OAuth or PAT
remains provenance, not writable account-token storage.

The read-only `github_accounts.auth.source` descriptor publishes the existing
`/auth/source` owner projection. Its values come from the owner source enum:
`env`, `oauth`, `pat` or `none`; this build does not resolve a `gh` CLI login.
`auth.secret` stays unpublished: provenance is not permission to reveal a token,
credential reference or the full owner authentication object.

Use the typed `list_accounts` operation to discover stable account IDs, labels,
canonical sites and credential presence before keyed reads or edits. It returns
`accounts`, the owner `revision`, and `persisted:false`. It does not return the
full authentication object, secret values or credential references. An empty list
is a valid result; a failed owner read is not an empty list.

## Ego

Read `app.ego_executable`, `app.ego_profile` and `app.ai_chat_workspace` through
the `app` store before using the Ego owner. The executable change is a trusted
execution-authority edit and requires the existing human confirmation policy.
Do not run a caller-selected candidate to probe it before acceptance.

Describe `ego`; use targeted owner reads/choice refresh for model, roots, network
and executable-path settings. Ordinary describe is cache-only. The separate
`{"action":"refresh","store":"ego"}` explicitly refreshes the configured
owner's provider catalogue. It is not a login action.

The owner invokes the configured Ego process with argument vectors in the
configured AI Chat workspace. It does not edit Ego files. Profile edits target
only the configured profile; machine reads and base-only provider auth actions
use the base document. `perform` for `store:"ego"` takes an `expectedRevision`
and one of `set_headers`, `clear_headers`, `set_credential_command` or
`clear_credential_command`. Inputs are write-only; presence is not a secret
replacement value. Read back after an edit, and report next-session effects.
Existing ACP conversations retain their launch/options; no PTY fallback replaces
an unavailable ACP or settings owner.

### Interactive login

The same `store:"ego"` perform envelope also publishes `begin_login` and
`login_status`. Both require the envelope's `expectedRevision`; obtain it from a
machine-targeted Ego read (not a profile read). Only `begin_login` compares that
revision because it starts a process; status never retries or changes configuration.

- `operation:{action:"begin_login",provider:"openai-codex"}` starts the configured
  executable with argument vector `auth login -- PROVIDER` in the configured AI
  Chat workspace, without a shell or profile override. It returns `flowId`,
  `sessionId`, `status:"interactive"`, `expiresIn:900`, `persisted:false` and
  `nextAction:"login_status"`. The person completes the browser/device/API-key
  exchange in that terminal. No Settings action accepts a login code or token.
- `operation:{action:"login_status",flow_id:FLOW}` reports `interactive` while the
  process lives. After it exits or the person closes the terminal, status is
  `closed`, with the provider's fresh `credential` projection from Ego doctor:
  `stored`, `expired`, `missing` or `unknown`. `closed` is not authentication success.
  An unavailable readback is an error, never a fabricated missing credential.
  Follow `nextAction:"refresh"` to refresh models/provider state before selecting
  a default model. No existing ACP conversation is relaunched.
- The host retains at most 32 handles for 15 minutes. An expired handle returns
  `status:"expired"`, its `sessionId` and `nextAction:"begin_login"`; unknown or
  pruned handles return `not_found`. Handle expiry does not imply that the
  provider credential expired and does not kill a person's terminal. Close it
  through the existing session control before starting another attempt.
- If the configured executable or workspace changes, status returns `conflict`
  instead of reading a different owner. A stale begin revision also returns
  `conflict` before spawning. Missing executable/capability returns unavailable.

The existing AI Chat Settings button remains a separate human UI entry point.
The typed owner action is not an ACP-to-PTY fallback and never accepts another
executable from its caller.

## Refusal and recovery

| Outcome | Consumer response |
|---|---|
| `confirmation_required` | Retain the exact validated edit and proposal ID. MCP asks the existing `ui confirm` with `proposalId`, then replays the exact edit with `approval`. HTTP/IPC can replay it with `approval` and `confirm:true` to ask the same dialog. |
| Refusal, expiry or disconnect | Write nothing. A proposal expires after 300 seconds and cannot be reused for a different connection, target, revision or content. Do not forge `confirmed:true`. |
| `conflict` | Read the owner again and reconcile intent. Do not silently move a dirty draft to the new CAS token. |
| Auth `pending`, `expired` or `access_denied` | Report the actual state. Respect the owner interval; only start a new login after the old flow is no longer usable and the person wants another attempt. |
| `persisted:true` with a failed apply/readback | The durable phase happened. Read back and follow `nextAction`; do not repeat the mutation as a retry. |
| Lost transport outcome | Treat durability as unknown. Read back before another write. |
| Missing binary/capability | Report unavailable and the required owner recovery. Do not use arbitrary commands or another transport as a fallback. |

Secrets are write-only throughout replies, errors, previews and events. A failed
vault read is unavailable, not proof that no credential exists. After durable
writes, `settings-changed` contains only `section`, process-local `generation`,
`persisted` and `applied`. The generation is an invalidation cursor, not an owner
revision. Reconnect requires fresh reads; dirty UI drafts remain local.

## Evidence boundaries

The source tests, replay fixtures and generated catalogue do not authenticate a
real account or prove live desktop/browser convergence. The Ego replay uses
captured owner responses; real interactive authentication remains the person's
responsibility. Step 10 must remain incomplete while its schema-only startup and
refusal/expiry/conflict/apply-failure integration criteria are unverified.
See [Settings](../user-guide/settings.md), [HTTP API](../api/http-api.md), and
[configuration ownership](config.md) for the corresponding user and transport
surfaces.

### Focused recovery contracts

The startup regression tests resolve operation constants from the described
owner schema before calling GitHub metadata workflows and the configured Ego
login workflow. GitHub account IDs come from `list_accounts`; Ego model reads
use the published field descriptor. The Ego replay refuses authentication and
therefore proves configured launch and recovery, not a successful provider login.

`provider_authorization_deadline_cannot_authenticate_or_persist` covers the
GitHub device authorization deadline in TUIC-owned flow state, including status
before completion and repeated completion. It verifies unchanged account and
runtime credential state and disposal of the private device code. This is not
an expired provider HTTP response fixture, a real authentication attempt, or an
Ego host-handle expiry test.

`persisted_apply_failure_publishes_unapplied_generation` covers the facade's
persisted-but-unapplied result. Recovery reads the durable value and new owner
revision, checks the persisted file, and emits no second mutation event. These
contracts passed coordinator-owned targeted Linux validation at `b4ace7721`:
33 tests covered startup workflows, the device deadline, provenance and account/
Ego consumers; a separate `--no-default-features` run passed both
`startup_uses_only_published_schema` and the persisted-apply-failure test.

These results validate the isolated contracts, not complete live onboarding.
Step 10 remains open for human-assisted GitHub/Ego authentication and recorded
provider-expiry evidence. No successful authentication, external expired-token
response, or native desktop validation is inferred from these runs.

### Boss-controlled acceptance procedure

Prerequisites: a Boss-controlled backend rebuild/restart, a disposable isolated
instance and registered test repository, a test GitHub account, and a configured
Ego executable/workspace with a provider that supports interactive login. Do not
launch a second desktop app or use production credentials for this check.

1. Enable Settings MCP from trusted UI. Discover the current schema and account
   operations. Run `begin_cloud_login`, complete the returned GitHub verification
   in the browser, and poll `complete_cloud_login` only at the returned interval.
   Verify success through `list_accounts` and presence-only owner readback. Use
   the described preference and binding operations with each owner's revision;
   read back the preference and repository binding rather than inspecting files.
2. Discover Ego providers and the published model field. Read the machine owner
   revision, call `begin_login` for the selected supported provider, and complete
   authentication in the returned terminal. Call `login_status` after exit;
   require fresh credential presence, not merely `closed`. Refresh provider
   state, select the model through the published field and read it back. Confirm
   the next-session effect; do not relaunch an existing ACP conversation.
3. In separate test flows, decline GitHub authorization and abandon an unapproved
   flow until it expires. Preserve a real provider-issued `expired_token` response
   only if the provider returns one before TUIC's local deadline takes effect.
   A local `expired` result does not establish that external response. If TUIC's
   deadline wins, record that limitation; do not invent a response or weaken the
   deadline. Ego host-handle expiry is a different condition.
4. Retain only redacted action names, outcome/status, provider error code and
   readback evidence. Never retain device codes, user codes, tokens, cookies,
   authorization headers, terminal authentication text or credential-store data.

The source-level refusal/confirmation-expiry, stale-revision, owner deadline and
persisted-apply-failure tests cover automatable recovery independently. They do
not require a live provider, and passing them must not be reported as completion
of the human authentication or external-response steps above.

## Contextual opt-in tips

The tip catalogue includes contextual entries for disabled opt-ins. After one
minute of active agent work, a terminal shows idle-sleep guidance when sleep
prevention is off. This is conservative guidance, not a battery or OS policy
probe. Expanded setup for Gemini, Grok and OpenCode offers the global hook tip
when global instrumentation is off; Claude and Codex use their separate
launch-scoped status controls. A recoverable server API error offers retry
guidance only when retry is disabled for that agent. Auth and tool errors do
not trigger it.

Each tip has a separate client-local dismissal key. Dismissal propagates to
mounted copies and other windows through storage events and persists across
reloads. If browser storage is unavailable, dismissal lasts for the document.
These tips never enable settings, install hooks or submit a retry.

The AI Chat legacy provider projection treats doctor as diagnostic enrichment, not a prerequisite for config and model reads. Exit 1 with valid JSON retains credential evidence and exposes a non-blocking `diagnostic`; unusable health output leaves credentials unknown. The shared Rust projection serves IPC and HTTP.
