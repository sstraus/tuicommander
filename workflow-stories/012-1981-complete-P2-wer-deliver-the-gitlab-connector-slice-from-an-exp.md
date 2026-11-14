---
id: 012-1981
title: "WER: Deliver the GitLab connector slice from an explicit operation inventory"
status: complete
priority: P2
type: feature
created: "2026-11-10T08:29:18.828Z"
updated: "2026-11-12T00:57:23.000Z"
dependencies: ["009-e32f"]
plan: plans/workflow-engine-rework.md
plan_step: Step 4
depends_on: ["workflow-stories/009-e32f-complete-P2-wer-deliver-typed-http-operations-and-the-shared-c.md"]
---

# WER: Deliver the GitLab connector slice from an explicit operation inventory

## Problem Statement

Workflow engine rework 012-1981. Scope: inventory existing GitLab clients/tools and deliver required issue/MR lookup and event intake, pinned diff/change review, pipeline status and permitted comment/review publication for a selected development flow; confirm exact service-supported actions in 001-52f6.

Delivery prerequisites: 009-e32f. These native prerequisites also appear in the linked plan and dependency metadata. All work remains pending.

Contract-first feature: record consumer effect, authoritative state, external assumptions and affected consumers before implementation. Reuse existing services/evidence where compatible; no mandatory duplicate full suites or extra critic rounds. Planning creates pending work only and does not authorize implementation or live effects.

## Acceptance Criteria

- [x] An explicit operation/support matrix distinguishes reused clients from missing adapters.
- [x] Catalog operations map typed project/issue/MR/head/base/pipeline fields, secrets by reference and semantic outcomes.
- [x] Use shared admission, pagination/rate-limit, callback/event deduplication and effect reconciliation instead of a GitLab scheduler.
- [x] Pipeline/review receipts bind the exact artifact; duplicate events or uncertain publication cannot create repeat writes.
- [x] Record service contract fixtures and authorized live evidence where available; unsupported required operations block delivery explicitly rather than treating connector existence as parity.

## Work Log

### 2026-11-11 — Contract before implementation

Consumer effect: selected flow reads one issue/MR, a specific MR diff version and an exact pipeline, then may publish one general MR note containing the reviewed head/base and the original effect identity. Semantic outcomes distinguish stale, incomplete, pending and failed evidence from success. No inline discussion or MR approval is required (COORDINATOR2 decision).
Authoritative state: GitLab REST v4 replies through the existing Rust plugin HTTP/account client; current host grants and the neutral effect ledger own authority and write reconciliation. Published descriptors pin the installed instance/account/project, never secret bytes.
External assumptions: REST v4 documented schemas, recorded CE 19.4.1 issue/MR/pipeline data; documentation examples for absent diff/webhook responses. No credentials or live writes are authorized. A general note cannot atomically assert the MR head: the body/receipt identifies the reviewed artifact and never grants approval of a later head.
Affected consumers: workflow catalog/admission, neutral host/effect owner, future native host composition and the shared neutral intake binding owned by 010. No frontend, transport or scheduler changes.

| Operation | Existing seam | Selected delivery | Support limit |
|---|---|---|---|
| issue/MR lookup | plugins/gitlab/provider.js, refs.js; Rust plugin_http account client | fixed typed single-subject reads | plugin UI mapping is not a workflow grant |
| event intake | no existing GitLab workflow source | typed issue/MR/pipeline normalization, shared 010 binding | binding contract requested; no new polling loop |
| pinned change review | model.js reads DiffNotes only | REST v4 exact diff version, head/base/start checks, reject incomplete diffs | no inference that displayed threads prove complete review |
| pipeline status | pipelines.js latestPipeline; recorded pipeline rows | exact project/pipeline/head receipt | no runner execution or live status claim |
| general MR note | provider.js notes POST | pinned body/receipt; neutral ledger ownership; uncertain remains blocked | no automatic resend |
| inline positioned discussion | model.js read mapping only | missing, not required by selected flow | no publication adapter |
| MR approval | provider.js approve/unapprove | missing, not required by selected flow | not exposed as workflow activity |

Validation cases: wrong artifact pipeline/diff, incomplete diff, changed head before note, forged installed config/current grants, lost response reconciliation without another write, duplicate event identity. Existing HTTP/neutral owner tests cover generic ledger replay and HTTP admission; targeted GitLab tests must cover the service-specific boundary.

### Service evidence and test design

The official REST v4/webhook examples were extracted verbatim as JSON on 2026-11-11 into `src-tauri/fixtures/gitlab-workflow/documented.json`; source URLs and version limits are in its README. Existing CE 19.4.1 recorded issue/MR/pipeline responses are reused unchanged. The official issue webhook example contradicts its project IDs; normalization refuses it instead of repairing the fixture. Local account/HTTP responders exercise the real account client with the test-support mock vault, not a live service.

New tests protect service-specific receipt identity, diff completeness, event generation/delivery identity, installed admission, exact note body, duplicate dispatch and uncertain publication. Existing HTTP/neutral tests cannot catch wrong GitLab field mappings or service-specific freshness checks. No test-only production flag or external live effect is added. Single-subject lookup, explicit diff-version retrieval and exact pipeline retrieval have no paginated response; collection pagination is not claimed by this slice. Rate-limit and uncertainty reuse the installed connector policy and neutral ledger; there is no retry scheduler.

### Shared intake contract correction

010 supplied `TriggerEvent` and the planned `WorkflowRuntime::admit_neutral_trigger` seam. Its source generation is independent of binding policy and delivery identity. GitLab normalization now hashes installed project, source event kind and original object attributes for generation; a redelivery with a different delivery ID retains the same generation. Binding revision/dedup/replay stay in 010. Requested an isolated committed dependency checkpoint to compile against the same types without copying them.

Catalog outputs expose individually typed project/issue/MR/head/base/version/pipeline fields alongside the full receipt; optional absence remains explicit on pending/stale outcomes. General-note text rejects service quick actions so comment authority cannot accidentally execute merge/approval commands. Added an owned pipeline case for MR association and typed downstream fields; this is a distinct service boundary from the pure response projection checks.

### Preliminary rb evidence (not final-tree GREEN)

Task `20261111T112902Z-61429`, remote nextest run `9e2cf10e-962c-40ec-af9e-0b26643d62b1`, compiled in 4m55s and passed 8/8 targeted tests (61.858s). Wrapper exit 75 correctly marked source drift because typed outputs and source-generation normalization were improved while that snapshot ran. No test failed. The matching source manifest and complete log remain under `~/Gits/.tmp/rb/20261111T112902Z-61429/`. This evidence proves only its synced snapshot and is not the final GREEN criterion. The final run must include the added pipeline boundary case and new shared intake wiring. No local build or duplicate full suite was run.

### Shared binding integration

COORDINATOR2 authorized dependency imports `21530a7b7c236d285cdd8933cb1589eaaed22b5c` and `d6d2e44b106d8dd3058d17769502078054d43489`, cherry-picked with provenance as `63b6886d84` and `e83fd22a7f`. They supply the shared closed source schema and trigger types; GitLab adds only its source variant and installed schema arm. The operational shared binding owner remains owned by 010. GitLab's backend facade forwards normalized snapshots to that owner after the ingress owner authenticates delivery. No separate binding table, dedup store, replay grant or polling loop is added.

The GitLab integration case uses the documented MR snapshot, maps its typed IID through the real shared binding to a retained neutral publication, reopens the run owner, and redelivers under another delivery ID. It must retain the original run ID. This protects the source-to-shared-owner boundary that the generic binding tests cannot prove for GitLab field mappings. Source schemas are installed; a webhook cannot supply a wider schema.

Validation reporting follows COORDINATOR2: rb toolchain versions are unmeasured; cite build-slot task IDs. The landing records the toolchain once. No manual SSH or version probe is added. Story criteria/status are maintained directly here because the ordinary story CLI does not index `workflow-stories/`.

### Adapter retry disposition

Task `20261111T114121Z-30083` exited 75 during source sync: **the remote command did not start**, so this run supplies no test result. Shared contract imports and the GitLab intake edits changed the tree during sync. The final integrated run will start only after the shared operational dependency is committed and all local edits are complete, with no concurrent source edits. rb toolchain remains unmeasured.

COORDINATOR2 authorized the shared host accessor `db022c8e0482c8b94042d15ae58bdee4e479dead` and operational binding core `c3e4974dc10317e1ed2c6eb86d44fd4beaa12ea6`. Imported with `-x` as `3ade2c76aa` and `d551ba9108`; the only cherry-pick conflict was additive module declarations, retaining both GitLab and GitHub modules. GitLab's own facade and intake integration test are now declared against these exact committed APIs. Shared implementation remains unchanged; GitLab extends only the closed source enum/schema match.

Final validation scope: the four service projection/normalization cases, five owned account/HTTP activity cases and the GitLab-to-shared-binding restart/dedup case. This covers the touched source schema consumer, account adapter, artifact outcomes and duplicate-write fence. It does not duplicate 010's generic binding suite or the batch's full suite.

### Final GREEN — implementation ready for integration

Validated commit `8d912227402f016a2bad5f23d51e33799d403a30`, tree `78a2189378de6916c3ae2be41d408052f77c1123`. rb task `20261111T121050Z-52750`, nextest run `c0a9f255-1f60-4768-9105-3dbeeeb01bd8`: **10/10 PASS**, 0.499s execution after 4m48s compile; wrapper and command exit 0, source manifests unchanged before/after sync/run, remote stopped/released and dirty=no. rb toolchain versions **unmeasured** by coordinator instruction. Logs/receipts: `~/Gits/.tmp/rb/20261111T121050Z-52750/`; launcher log `~/Gits/.tmp/wer-012-gitlab/integrated-final.log`.

Command (default features/profile; no flags or target override):

```sh
PATH="/opt/homebrew/bin:$PATH" /Users/stefano.straus/Gits/personal/orchestrator/tools/agents/tuic-bg.sh /Users/stefano.straus/Gits/.tmp/wer-012-gitlab/integrated-final.log -- /Users/stefano.straus/Gits/personal/orchestrator/tools/build/build-slot.sh --remote scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(workflows::gitlab::tests::) | test(workflows::run::neutral_gitlab_tests::) | test(workflows::run::neutral_gitlab_intake_tests::)'
```

Criterion evidence:

1. The contract-first operation matrix above inventories the reused plugin/account client and explicitly missing inline discussion/approval adapters. No connector-existence parity claim.
2. Five installed descriptors expose strict typed inputs/outputs and semantic outcomes; account authentication is `DataReference::Credential`. `installed_admission_rejects_forged_metadata_and_revoked_grants` proves current read/write separation and metadata refusal. `exact_mr_pipeline_receipt_keeps_failure_and_typed_artifact_fields` proves downstream artifact fields and pipeline failure semantics.
3. The GitLab facade uses the imported shared trigger schema/binding/run owner; `documented_mr_event_uses_shared_mapping_and_durable_generation_dedup` proves actual typed mapping and dedup after owner reopen. Single-resource endpoints require no collection pagination. Shared installed rate-limit policy retains Retry-After without a retry loop; no callback/polling scheduler is installed. Generic binding/coalescing behavior remains 010's validation responsibility.
4. Domain tests reject wrong subject/head/base/start and incomplete diff; the owned pipeline case checks current MR association. `stale_head_returns_stale_without_a_post`, `general_note_runs_through_owned_effect_and_is_not_duplicated` and `uncertain_note_reconciliation_never_reposts` prove stale refusal, one POST, and retained uncertain ownership without another request. The intake test proves duplicate delivery returns the original run.
5. Official rolling REST v4 examples (diff flags since18.4) and existing real CE19.4.1 recordings have distinct cited provenance in the fixture README. Exact live gaps remain: an authorized scratch GitLab account/project and permission to register/deliver/retry webhooks, fetch fresh diff versions, run a pipeline, publish a note and observe lost-response recovery. None was supplied/authorized. Unsupported inline discussion/approval is not required by the coordinator-selected flow; if required later, delivery must block until adapters/evidence exist.

`fmt-changed.sh <worktree> main...HEAD` passed; individual changed GitLab Rust files were formatted via rustfmt edition2024 stdin without traversing other writers' modules. No JS/TS production files changed, so no unrelated frontend suite was run. The remaining evidence-only commit changes this worklog, with production/test/fixture content unchanged from the validated tree. Story remains in_progress for parent integration; all implementation criteria are checked.

Risk: medium (service protocol, current admission, durable effect ownership). General notes cannot atomically assert current MR head and are never MR approvals. This internal adapter and authenticated-source facade require native host/ingress composition; no installed native provider, public transport, webhook registration or live service success is claimed. Rust does not hot-reload: load through a manual restart/rebuild when safe; the exact live check is in `to-test/workflows-stories-4.md`. No app restart, external live write, push or merge occurred.

Shared changes by012 are additive: `workflows/mod.rs`, `run/mod.rs`, `docs/backend/workflows.md`, `docs/backend/workflow-rework-inventory.md`, plus the two-line GitLab source enum/schema extension in `workflows/trigger.rs`. Imported source-neutral/host-accessor files are attributable to the authorized cherry-picks above.

### Critic round1 — O1 simplification contract

`critic-012-r1` requested deletion of the second authority/check call and per-fetch guard in `invoke_effect`/the account client. Verified against the shared HTTP host: retain `prepare()` and equality with the original approved/dispatched preparation immediately before client invocation; select the already-validated installed operation without another grant lookup. Remove the redundant guard argument/closure. No endpoint, artifact freshness check, receipt, grant definition or reconciliation behavior changes. Existing five account/HTTP activity cases cover this affected consumer; pure projection/intake results remain compatible. No new assertion or test-only seam is needed. Critic notesN1–N6 are accepted limitations, not scope expansion; the full report is `~/Gits/.tmp/results/critic-012-1981-r1.md`.

O1 GREEN: commit `a68c1a2d8a4fb54225aa07de06014434c8b551d9`, tree `f1a08450d57dbdc9bf55a315eec3984a24f3186a`; rb task `20261111T122842Z-10850`, nextest `fd6cae95-5d2e-4ccf-8510-d6717b582e73`, **5/5 PASS** in0.500s (2m44s compile), wrapper exit0 and dirty=no. Command: same build-slot/with-test-tmp/tuic-bg procedure, filter `test(workflows::run::neutral_gitlab_tests::)`, launcher log `~/Gits/.tmp/wer-012-gitlab/critic-o1.log`. Source remained unchanged throughout. Toolchain unmeasured. No tests changed.

### Promoted N2 — contract before fix

COORDINATOR2 promoted read-error settlement to a required fix: a completed non-mutating GET error must settle as `Completed { Failed }` with `stopped=true`; a potentially committed write must retain uncertainty. GitLab has its own service-specific preflight/projection path, so the shared009 correction does not reach it automatically. Coordinator approved extracting the unchanged settlement match from fixed009 client into `pub(super) settle_result`, called by both clients, with one policy owner. The coordinator will supply a pinned integration HEAD containing009 fix `52ada40bfa`; merge it once before making the shared change. No copied settlement match will be added to GitLab.

The regression will send an actual read through the account client with a malformed service response, observe the real durable effect, and require stopped/failed settlement without any POST. It catches permanent read uncertainty rather than asserting a private helper. Existing uncertain-write test protects the opposite branch. N1 and N3–N6 remain accepted limitations per coordinator triage.

Merged the authorized pinned base `398b3d18a6` once as `4fc7c671d8cc77cb10637c1fba58b8c823fd6537` (pre-fix tree `cf4841ed571a808b5b87b6b1ab85e0c2966f48f4`). Resolved four additive declaration/doc conflicts retaining GitLab plus incoming activities and the newer coarse-adapter documentation. Added only the N2 regression before fixing production: `neutral_gitlab_read_tests::malformed_read_reply_settles_failed_instead_of_remaining_uncertain`. The RED run must fail because the owned read observation has stopped=false/Uncertain.

N2 RED confirmed on pre-fix regression commit `90938e88bb95a50ef8372f30003282734a468b28`, tree `5a4cbacd3776e2e657e0583b90c610c3f458876b`. rb task `20261111T123913Z-49549`, nextest `b2179d55-3c44-4abe-a947-490f8d372b57`, one regression failed in0.883s after4m13s compile, wrapper exit100/nextest failure. Exact assertion: completed non-mutating GET must release uncertainty; actual `EffectObservation { outcome: Uncertain { diagnostic: "GitLab response project_id differs from requested subject" }, stopped: false }`. This is the requested runtime symptom, not a setup/transport failure. Filter was `test(workflows::run::neutral_gitlab_read_tests::malformed_read_reply_settles_failed_instead_of_remaining_uncertain)` through the established remote wrapper. No source drift; dirty=no. Log: `~/Gits/.tmp/wer-012-gitlab/read-red.log`; rb toolchain unmeasured.

N2 implementation: extract009's existing final result match unchanged into `neutral_http_client::settle_result`; both HTTP and GitLab execute functions call it. The service-specific GitLab MR preflight, semantic outcomes and receipt projection remain in their existing function. This shares the read-error-vs-write-uncertainty decision without another client/executor or duplicated policy. Final narrow consumers: new owned GitLab read regression, existing GitLab lost-response write case,009 owned read-refusal case and009 response-error boundary case. These distinguish both policy branches and the existing caller; successful service projections and intake are unchanged and reuse prior GREEN.

### Fix round final GREEN

Fixed commit `2140dd3eeedd5ce765b17c4d7ca5d56445b7fd71`, tree `3f21be856c1eb809a395f586738e55d1d03416ba`. rb task `20261111T124643Z-59333`, nextest `31d23364-dbff-42a2-b42f-9c2e7f805ec4`: **4/4 PASS** in1.192s after2m08s compile, wrapper/command exit0, source unchanged, remote stopped/released, dirty=no. rb toolchain versions remain unmeasured. Full receipts and logs are under `~/Gits/.tmp/rb/20261111T124643Z-59333/`; launcher log `~/Gits/.tmp/wer-012-gitlab/read-green.log`.

```sh
PATH="/opt/homebrew/bin:$PATH" /Users/stefano.straus/Gits/personal/orchestrator/tools/agents/tuic-bg.sh /Users/stefano.straus/Gits/.tmp/wer-012-gitlab/read-green.log -- /Users/stefano.straus/Gits/personal/orchestrator/tools/build/build-slot.sh --remote scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(workflows::run::neutral_gitlab_read_tests::) | test(workflows::run::neutral_gitlab_tests::uncertain_note_reconciliation_never_reposts) | test(workflows::run::neutral_http_tests::refused_read_fails_the_activity_instead_of_holding_uncertainty) | test(workflows::run::neutral_http_boundary_tests::pagination_and_response_failures_never_become_success)'
```

This closes O1 and promoted N2. RED reproduced the actual read-error symptom before the shared-policy fix; GREEN proves GitLab stopped/failed settlement, unchanged write uncertainty/no repost, and the existing009 caller's read/error behavior. Remaining GitLab projection/intake cases reuse the earlier10/10 and O1 five-case evidence; the authorized pinned integration base carries the coordinator's80/80 result. No test assertions were weakened, removed or changed after RED. No additional critic or full suite was run.

The fix adds one shared-file change beyond the initial RESULT: `run/neutral_http_client.rs` exposes the existing unchanged settlement match as `settle_result`; both HTTP and GitLab call it. `run/mod.rs` adds only the regression module declaration. The one authorized merge `4fc7c671d8` brings pinned `398b3d18a6`; no second merge was performed. All five criteria remain checked; live-account/native-composition gaps and accepted N1/N3–N6 limits remain as recorded. Thirteen compiler warnings concern inherited/unwired internal APIs and checkpoint imports; no new clippy run or unrelated cleanup was performed. Final format gate passed before GREEN; the final evidence commit changes only this story.

### 2026-11-12T00:57:23.000Z - Coordinator closure (integration record reconciliation, or-3 1787b94b): All acceptance criteria checked with evidence cited in this record; closed per ~/Gits/.tmp/workflow-rework/HANDOFF.md (implementation criteria complete, coordinator closure only). Documented limits in this record stay limits, not passes. No new test run.
