---
id: 009-e32f
title: "WER: Deliver typed HTTP operations and the shared connector contract"
status: complete
priority: P2
type: feature
created: "2026-11-10T08:29:18.826Z"
updated: "2026-11-12T00:57:23.000Z"
dependencies: ["005-f907"]
plan: plans/workflow-engine-rework.md
plan_step: Step 4
depends_on: ["workflow-stories/005-f907-complete-P2-wer-own-durable-asynchronous-activities-effects-an.md"]
---

# WER: Deliver typed HTTP operations and the shared connector contract

## Problem Statement

Workflow engine rework 009-e32f. Scope: typed HTTP operation and shared contract for installed connector/client/tool-backed operations, not a parallel integration platform.

Delivery prerequisites: 005-f907. These native prerequisites also appear in the linked plan and dependency metadata. All work remains pending.

Contract-first feature: record consumer effect, authoritative state, external assumptions and affected consumers before implementation. Reuse existing services/evidence where compatible; no mandatory duplicate full suites or extra critic rounds. Planning creates pending work only and does not authorize implementation or live effects.

## Acceptance Criteria

- [x] Configure method, endpoint, query, headers, request body and typed response mapping.
- [x] Credentials are references, never graph literals.
- [x] Descriptors define authentication reference, resource schema, pagination, rate-limit handling, callback/webhook correlation and effect reconciliation.
- [x] Dynamic bounded body/path/argv/safe-header fields are allowed by contract; untrusted provenance cannot select credential destinations, executables or write authority.
- [x] Both publication and dispatch check their respective known/resolved boundaries.
- [x] Network access and writes have distinct grants.
- [x] Adapter fixtures come from real service evidence or documented recorded contracts; credentials/endpoints absent from live checks are explicit gaps, not mocked success.
- [x] A paginated response can drive 003-e678 collections.

- [x] GREEN: rb 20261111T105428Z-41473, tree 1bb50b6dc7413416e4c8d46565839e5f337a0cc5, targeted HTTP retry 4/4 PASS; compatible consumer/admission evidence 39 PASS from 20261111T104712Z-64046, commands and hashes in Work Log.
- [x] REALITY: effect=actual local HTTP request bytes, retained typed output and collection expansion; state=pinned descriptor defaults, current backend authority, existing effect ledger; assumptions=documented local HTTP contract, external endpoint/credentials unavailable, rb Linux; radius=HTTP adapter plus neutral runtime/effect/client/collection consumers.

## Work Log

### 2026-11-11 — implementation contract

Effect: a published HTTP activity resolves method, endpoint, bounded path/query/header/body fields, executes through the installed plugin HTTP client, and returns schema-checked outputs. Cursor pagination produces one bounded array usable by the existing collection contract.
State: the pinned activity descriptor and request config describe work; the backend-installed connector contract binds the plugin, credential reference, destination, resource schema, pagination/rate-limit/callback/reconciliation policy. Current backend authority supplies separate network and write grants. The existing neutral admission and effect ledger own dispatch and uncertainty.
Assumptions: HTTP fixtures exercise a documented local HTTP/JSON contract through the real installed client. No external service credentials or endpoint were supplied; no vendor compatibility or live external writes are claimed. The existing account client owns credential lookup, redaction and destination binding.
Radius: new connector/HTTP modules, additive module declarations, documentation and manual-check tracker. No model/coarse-agent code changes. Native host composition remains owned by 008; this adapter implements its existing NeutralHost boundary.
Validation: one final rb nextest filter for connector/HTTP tests plus existing neutral runtime/effect and collection consumers. Named bugs: data selecting authority, changed grants bypassing dispatch, wrong typed output, unbounded or looping pagination, writes reissued during reconciliation, and rate-limited requests silently retried.

### Implementation and focused validation scope

Added the shared connector metadata and bounded data-slot contract, typed HTTP projections, and the existing-neutral-host HTTP adapter over plugin_http_fetch_impl. Installed metadata is included in the descriptor schema default and cannot be overridden by instances. Dispatch/preparation/client entry recheck current grants. Writes own schema-derived resource keys; recovery never reissues a request.

Behavior cases: actual POST path/query/header/body bytes and retained typed result; real client cursor pages into existing collection expansion; publication/resolved dispatch boundary and grant failures; malformed/oversized/looping pagination and wrong output types; 202/callback and 429 write uncertainty without retry. Existing tests did not exercise an HTTP NeutralHost. Tests observe the installed client, actor, effect ledger and collection boundaries; no test-only production seam was added.

External evidence gap: no external provider endpoint or credentials were supplied. Local HTTP/1.1 JSON contract semantics are documented in docs/backend/workflow-http-connectors.md; live provider compatibility, callback authentication and remote stop remain provider-installation evidence requirements, not claimed successes. Native host composition is 008 scope.

### First rb validation — fixture diagnosis

Tree 976a2f98804bf3738d47b26ffed232376d9272d9; rb task 20261111T104712Z-64046; nextest run d0fcb4fa-a0ba-4a56-8b5c-b078ab0244af. Command: scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(neutral_http) | test(neutral_runtime_tests) | test(neutral_effect_tests) | test(collection_tests)'. Result: 39 passed, 4 failed. All existing affected consumers and HTTP publication/dispatch checks passed. The collection substring also selected seven settings collection tests; no further settings runs are needed.

Failure classification: the local HTTP fixture omitted the manifest's required nonempty allowedUrls. The existing client correctly refused it before network entry. Direct HTTP cases reported that refusal, and the two actor cases retained uncertainty and reached expiry. Fix: declare the local contract URL in the fixture manifest, retaining the owner-configured local destination. Removed one unused test import. No production change. Rerun only the four failed HTTP cases; prior compatible consumer/admission results remain valid. Log: ~/Gits/.tmp/wer-009-http/targeted-rb.log.

### Final rb GREEN and acceptance evidence

Final targeted rb task: 20261111T105428Z-41473. Nextest run: 1688148c-6edb-43c0-95f7-41d835180e2d. Uploaded rb tree: 1bb50b6dc7413416e4c8d46565839e5f337a0cc5; source_tree_sha256: 79703be817fbcbe239bab2d374102b13d4027555698c2e39a736ff18fdeb0293; staged source checkpoint: 35cc88ce5f0a3c054c235e44657e04440d50554c.

Command (through build-slot.sh --remote, started by tuic-bg.sh): scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(typed_request_reaches_installed_client) | test(paginated_activity_output) | test(pagination_and_response_failures) | test(asynchronous_and_rate_limited_writes)'. Result: 4/4 PASS, 1.140 s test runtime, exit 0; rb receipt confirms command_exit=0, stopped=true, released=true and box_dirty=false. No native cargo run, clippy or full suite. rb does not measure toolchain/target versions in its receipt; nextest run identities and Cargo.lock blob a60770915f5d0a618d27f88aac4e6e7ef61ecb51 identify the recorded environment without inventing versions.

The final manifest matches every current workflow Rust source and Cargo input. The prior 39 PASS results remain compatible: only the HTTP server fixture's missing allowedUrls and unused test import changed between runs. Together the two runs prove all 43 selected tests, including all five new HTTP tests and affected neutral runtime/effect/collection consumers. Final log and source identity: ~/Gits/.tmp/wer-009-http/http-retry-rb.log and final-source-identity.json.

Acceptance evidence:

1. Configure method/endpoint/query/headers/body/typed response: HttpOperation and typed_request_reaches_installed_client_and_completes_owned_run observe actual POST wire bytes, output value 42, completed run and settled ledger. Wrong/missing output fields fail in pagination_and_response_failures_never_become_success.
2. Credential references: ConnectorContract accepts only DataReference::Credential; credentials are resolved by the existing plugin account client. publication_and_dispatch_enforce_installed_boundaries_and_current_grants rejects graph credential fields, Authorization literals and wrong reference kinds. No credential store was read in this task.
3. Shared descriptor contract: ConnectorContract carries authentication, resource schema, bounded cursor pagination, rate-limit policy, callback correlation and reconciliation. HttpConnector::new pins the installed binding in the descriptor schema default; instance metadata overrides refuse. The async/rate-limit test retains original-key pending/uncertain effects and completes through existing Observe.
4. Dynamic data slots: BoundedValue supports bounded body/path/query/safe-header values and data argv slots for installed clients. HTTP path segments are encoded, traversal and invalid header literals refuse. Input values cannot select destination/method/authentication/header name/executable/write grants. No argv launcher was added.
5. Publication/dispatch boundaries: the publication/dispatch test covers known invalid values, unknown dynamic destinations, resolved oversize input and revoked network grant after the durable dispatch claim. Invocation rechecks preparation against current host state.
6. Distinct grants: HttpContext keeps network and write grants separate; publication/dispatch test permits GET with network alone and refuses POST without its write grant. Mutating methods cannot paginate.
7. Evidence: documented local HTTP/1.1 JSON contract exercised through the real installed client, actual request bytes and retained run/effect outputs. External provider endpoint/credentials were absent and remain an explicit installation evidence gap. No external provider success or write is claimed.
8. Pagination into 003-e678: paginated_activity_output_drives_existing_collection_in_source_order completes an owned activity, expands its retained activity-output Datum through the existing ForEach contract, and verifies ordered first/third values, index 2 and provenance. Pagination bounds, repeated cursors, malformed cursors and cumulative byte limits refuse incomplete success.

REALITY: effect=actual local HTTP bytes, retained typed output and collection expansion; state=pinned catalog/schema/defaults plus current backend authority and existing effect transactions; assumptions=documented local contract only, external service evidence unavailable, rb Linux; radius=additive HTTP/connector modules and existing neutral/client/collection consumers.

Shared files changed additively: workflows/mod.rs, run/mod.rs, docs/backend/workflows.md, docs/backend/workflow-rework-inventory.md. No neutral agent/model modules or 006 story files changed. Rust requires a manual make dev restart or make build to load after integration; none was performed. Native host composition remains 008 scope.

Tracking note: the main native stories-cli store returned story 009-e32f not found; this workflow-stories file is the supplied authoritative story and its eight criteria were checked directly from the evidence above. Status remains in_progress for the coordinator's landing transition.

### Final static checks and tracking boundary

Current source matches all 231 workflow/Cargo entries in the final rb manifest. git diff --check and targeted rustfmt --check passed. Instruction-link check passed (two pairs). fmt-changed.sh main...HEAD passed before the new commit; it will also check the completed branch.

The required generic story-close.sh --check against the main checkout refused: no story 009-e32f in its stories directory. The supplied workflow-stories namespace is independent of that native store; no native story was created or altered to bypass this. This file holds the checked criteria and complete evidence, with landing/status transition reserved to COORDINATOR2.

TOOL-ANOMALY: mdkb memory confirm on prior clu-0435a1cda8e9c8b2 returned a multiple-open-sessions/ambiguous mutation error; no duplicate memory was written. The named long-command detach/wake memory was subsequently confirmed successfully.

### Coordinator-authorized fix round — contract before edits

F1: failed read requests must terminate as failed activities with stopped=true; only writes can remain uncertain on generic client errors. F2: confirmed 2xx empty bodies are JSON null before typed output mapping, so optional/no-output writes and HEAD complete. Existing ledger/host admission remain authoritative. O1 removes the unused installed methods map in favor of GET/HEAD/OPTIONS reads, all other methods writes. O2 removes only the credential-name substring denylist from allowed data headers; typed credential references and existing client destination/auth handling remain. Docs registration added. Affected consumers: HTTP adapter/client/fixtures; targeted tests cover refused reads, pagination/output failure and confirmed empty DELETE/HEAD. No external writes or app restart.

Fix-round GREEN: rb task 20261111T112339Z-81531, nextest 3cfbc663-7694-4be4-8f75-c2929775154e, command scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(neutral_http)'. All 7 tests PASS (1.170 s), including the critic's refused-read regression and the new empty DELETE/HEAD regression. Tree fa9f0cc392b771854eeca0a47e4b2904fedd6299; source_tree_sha256 95846c7fcdb71ff9a6d6572a8e7cb3c41429f224b3c508de420a328e7679fb89. Final receipt command_exit=0, stopped/released=true; box_dirty=false. All 231 workflow/Cargo source hashes match the receipt; toolchain/target versions remain unmeasured by rb. rustfmt and git diff --check passed. F1/F2/O1/O2 and docs registration are resolved; N1 HTTP-date Retry-After and N2 optional absent input omission are accepted unchanged by coordinator. No live external provider validation.

### 2026-11-12T00:57:23.000Z - Coordinator closure (integration record reconciliation, or-3 1787b94b): All acceptance criteria checked with evidence cited in this record; closed per ~/Gits/.tmp/workflow-rework/HANDOFF.md (implementation criteria complete, coordinator closure only). Documented limits in this record stay limits, not passes. No new test run.
