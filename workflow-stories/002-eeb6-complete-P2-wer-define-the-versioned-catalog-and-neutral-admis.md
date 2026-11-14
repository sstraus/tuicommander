---
id: 002-eeb6
title: "WER: Define the versioned catalog and neutral admission contract"
status: complete
priority: P2
type: feature
created: "2026-11-10T08:29:18.821Z"
updated: "2026-11-10T15:25:24.567Z"
dependencies: ["001-52f6"]
plan: plans/workflow-engine-rework.md
plan_step: Step 2
depends_on: ["workflow-stories/001-52f6-complete-P2-wer-inventory-preservation-and-freeze-the-neutral-.md"]
started_at: "2026-11-10T09:59:35.312Z"
completed_at: "2026-11-10T15:25:24.566Z"
---

# WER: Define the versioned catalog and neutral admission contract

## Problem Statement

Workflow engine rework 002-eeb6. Scope: specify and implement the stable catalog and publication validator without adding business-specific kernel variants.

Delivery prerequisites: 001-52f6. These native prerequisites also appear in the linked plan and dependency metadata. All work remains pending.

Contract-first feature: record consumer effect, authoritative state, external assumptions and affected consumers before implementation. Reuse existing services/evidence where compatible; no mandatory duplicate full suites or extra critic rounds. Planning creates pending work only and does not authorize implementation or live effects.

## Acceptance Criteria

- [x] Descriptors declare namespaced ID/version, purpose, handler, config/defaults, IO schemas, outcomes, capabilities and supported execution policy; instances pin those contracts.
- [x] Persisted publications pin exact transitive definitions, schemas, handler references, inline prompt/config content and subflows; actual run closure pinning belongs to 004-2809.
- [x] Configure instances, save presets and persist reusable script/subflow descriptors with explicit version/content-pinned targets without kernel edits; privileged native capabilities still need host adapters.
- [x] Neutral definitions validate typed workflow inputs and an optional typed subject without mandatory Plan/Story/PR/Issue kind; actual run snapshots belong to 004-2809.
- [x] Operations declare required subjects/capabilities and expose trusted fail-closed publication and resolved-value admission interfaces; explicit mapping checks belong to 003-e678 and actual every-dispatch admission to 004-2809.
- [x] Missing pinned handlers produce a diagnostic and graph labels/pins supply no grants; real native revocation/freshness and inherited-rights proof belongs to 008-22c4.
- [x] Neutral descriptor contracts express model-only, script-only and native-component definitions without business kernel kinds; executable adapters and UI/transport parity remain required in their consumer stories.
- [x] Contract validation rejects unknown outcomes, missing required values and client labels falsely supplying authority; concrete dispatch enforcement remains required in 004-2809/008-22c4.
- [x] No runtime plugin loader or marketplace.
- [x] REALITY: effect=persisted exact neutral contracts or a diagnostic, not execution permission ; state=WorkflowStore SQLite, WorkflowCatalog and backend-owned HostRegistry (catalog/publication/preset/reusable/admission tests; rb 20261110T142738Z-87957 and 20261110T144713Z-85753) ; assumptions=none about external systems ; radius=neutral contract/storage/admission consumers only. This records the existing pre-implementation contracts in the worklog; runtime/authority proofs remain in 004/008.
## QA

Verified the clarified contract/storage/interface scope against source and unchanged hash-bound rb evidence (38 tests plus 15 changed-file tests and exact-pre-fix behavioral RED/GREEN). No real execution/authority/UI parity claim; obligations explicitly retained in 003/004/005/008/018.

## Work Log

### 2026-11-10 — 002E integrity coverage correction

The preset tamper test now checks rehashed row/document identity mismatch and a valid descriptor B pin paired with instance A. All four targeted preset tests passed on rb; receipt `20261110T115040Z-3919`, exit 0. Hashes check consistency, not authenticity; story 002 remains in progress.

### 2026-11-10 — 002E preset implementation

Added internal project-scoped immutable activity presets with descriptor pins, verified read/apply, and stale-writer refusal. Corrected the subject delivery test to assert both handler and capability observations, including absent subjects. On rb, four preset tests, seven admission tests and the no-default-features library check passed; receipt `20261110T114120Z-91895`, exit 0. Presets and port fakes grant no execution authority; story 002 remains in progress.

### 2026-11-10 — 002E preset contract

A caller saves and later clones an exact project-scoped configured activity revision; stale writes, changed pins, invalid config and missing handlers are refused. Truth comes from WorkflowStore SQLite and the current catalog/handler inventory; no external service behavior or authority is assumed. Risk is high for durable integrity and admission. Each save or apply validates one instance and its finite config fields, with no field-count or document-size bound in the existing schema; revision arithmetic rejects overflow and an IMMEDIATE transaction serializes competing writers.

### 2026-11-10 — 002D operation subject implementation

Descriptors declare optional typed subject requirements, checked against each publication at save and verified read; preflight resolves actual operation subjects and input defaults before host callbacks. Internal neutral format 3 refuses older formats. On rb, catalog (9), publication (13) and admission (7) targeted tests plus the no-default-features library check passed; receipt `20261110T113048Z-31859`, exit 0. Tests cover port contracts, not real authority or freshness; story 002 remains in progress.

### 2026-11-10 — 002D operation subject contract

Publication refuses workflow subjects that cannot satisfy each activity's declared typed subject; root-only preflight sends resolved values to trusted host ports. State comes from verified WorkflowStore publications and catalog schemas; host adapters own current authority and freshness, with no external-service assumption. Risk is high at the admission boundary. Compatibility compares the finite declared field maps; per instance it scans both field maps and performs BTreeMap lookups, with no explicit field-count bound; closure traversal is limited to 32 expanded edges and depth 4. Absent subjects stay absent; explicit null is invalid, defaults apply only to missing fields, and mappings remain outside this slice.

### 2026-11-10 — 002C3 admission defect correction

The root-only preflight refuses subflows with unresolved child arguments. It validates identity-mapped root inputs against every activity input schema and leaves subject compatibility to the host adapter. Both callback modes receive the checked instance with resolved configuration and its execution policy. Publication callbacks must make bounded local checks without external I/O or WorkflowStore re-entry. Regression tests: `dispatch_refuses_unmapped_child_arguments`, `dispatch_refuses_incompatible_activity_inputs`, `host_checks_resolved_instance_in_both_modes`; pre-fix commit `5074343184fb739b78f5e3d69962d6983279f501`. On rb, six targeted admission tests and the no-default-features library check passed (receipt `20261110T111843Z-22500`, exit 0); the first run passed but its receipt was stale after a worklog edit. No pre-fix RED was run under the adversarial-TDD in-story rule; source evidence remains the pre-fix defect proof. These are port-contract checks, not real authority or effect-boundary proof. Risk: high. Story 002 remains in progress.

### 2026-11-10 — 002C3 admission defect contract

Dispatch preflight must refuse unresolved child arguments and validate identity-mapped root activity inputs; host checks must receive each actual instance with schema-resolved configuration. Authoritative state comes from the verified publication, catalog schema and trusted host port, not graph labels. No external service behavior is assumed. Risk: high because incorrect activity arguments or configuration can mislead authorization; this remains a preflight, not an effect permit.

### 2026-11-10 — 002C3 admission implementation

Added an internal publication/pre-dispatch service with host-owned handler and capability adapters. Publication checks run before the insert in the verified transaction; dispatch validation re-reads the closure and current host checks. Three port-contract tests, eleven affected publication tests and the no-default-features lib check passed on rb. These tests do not establish a real authority, freshness or effect-boundary adapter; 002 remains in progress.

### 2026-11-10 — 002C3 admission contract

The neutral service refuses publication or dispatch validation when current host context, a handler or a required capability adapter is missing, or the trusted host denies the operation; persisted pins and typed runtime values are rechecked. State comes from WorkflowStore, WorkflowCatalog and host-owned adapter implementations, not client declarations. No real host authority or remote provider behavior is assumed. Risk: high because this port is an authorization seam; runtime dispatch and concrete host enforcement remain unimplemented.

### 2026-11-10 — 002C2 transitive pin implementation

Neutral format 2 derives and rechecks bounded same-project subflow revision/content pins in a SQLite snapshot; format 1 is refused. Eleven targeted publication tests and the no-default-features lib check passed on rb. External prompt assets, actual subflow invocation, authority and freshness remain outside this slice; 002 remains in progress.

### 2026-11-10 — 002C2 transitive pin contract

Saving or resolving a neutral publication derives exact same-project child revision and content pins, and refuses missing, altered, cyclic, or over-bound dependencies. Authoritative state is the transactional WorkflowStore plus the current catalog and host handler inventory; no remote provider behavior is assumed. Risk: high because durable closure integrity and recursion bounds protect later consumers, but do not authorize execution.

### 2026-11-10 — 002C1 storage implementation

The workflow database now stores project-scoped neutral revisions with a full-content hash and derived flat catalog pins; reads verify content and current pinned handlers. On rb, five neutral publication tests and nine catalog tests passed, and the no-default-features lib check completed. This internal store is not publication admission, subject freshness, transitive prompt/subflow closure or execution; 002 remains in progress.

### 2026-11-10 — 002C1 storage contract

Consumers save a project-scoped neutral revision and retrieve the same typed definition and catalog pins after reopen; stale writers and altered records are refused. WorkflowStore owns the SQLite transaction and the current catalog and host handler inventory supply pin verification. No remote service behavior is assumed. Risk: high because immutable publication integrity and concurrent writes are shared boundaries; publication is not admission or execution.

### 2026-11-10 — 002B pin implementation

The catalog now pins complete descriptor content, checks exact handler versions, and derives a sorted flat instance pin set. On rb, the targeted `workflows::catalog_tests` run passed nine tests and the no-default-features lib check completed. The set does not pin transitive prompt or subflow contents, provide authority, or persist publication; 002 remains in progress.

### 2026-11-10 — 002B pin contract

Consumers derive sorted descriptor pins from activity instances and receive a diagnostic if a persisted pin no longer matches the current catalog body or exact handler version. The authoritative state is the current WorkflowCatalog and host-provided handler inventory; serialized pins are untrusted claims, not grants. No external provider is assumed. Risk: high because content integrity feeds later publication; subject freshness, authority and transitive closure are outside this step.

### 2026-11-10 — 002A catalog implementation

Added the neutral catalog API and four focused tests. On rb, `scripts/with-test-tmp.sh cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --lib workflows::catalog_tests` passed all four tests; `cargo check --manifest-path src-tauri/Cargo.toml --no-default-features --lib` completed. This is not publication, persistence, transitive closure pinning, host authority, subject freshness or execution; 002 remains in progress.

### 2026-11-10 — 002A catalog contract

Consumers register immutable versioned descriptors and validate an instance against a selected handler and typed values, receiving resolved defaults or a diagnostic. The catalog and host-supplied handler inventory are the authoritative inputs for this deterministic boundary; declarations do not authorize effects. No external provider or runtime behavior is assumed. Risk: high, because later publication and admission depend on these contracts; live authority and closure pinning remain outside 002A.

### 2026-11-10T09:59:34.864Z - Feature contract before implementation: consumers publish a neutral graph and receive a pinned catalog closure or a diagnostic for invalid inputs/outcomes/authority; dispatch admission must refuse missing handlers, revoked grants and stale subjects. Authoritative state must be host-owned persisted publications and current host authority/freshness, not graph labels or caller-supplied grants. No external provider behavior is assumed. Risk: high, because publication and admission define a shared authorization and version boundary; runtime execution belongs to later stories.

### 2026-11-10T10:02:33.481Z - Source investigation before implementation: workflows/api.rs definition_action handles desktop/HTTP/MCP definition actions without an AppState or caller argument; run/contracts.rs pins routing imports, agent_policy.rs selects role-bound pinned routes, mcp_capabilities.rs governs MCP domain/action switches, and run/runtime/issue_publication.rs checks its operation-specific current repository allowlist. I have not established an existing host-owned generic grant and subject-freshness source that neutral dispatch can safely reuse. No production code or acceptance criteria were changed; no runtime test was run.

### 2026-11-10T10:04:45.267Z - Clarification of approved plan lines 51, 174, 176 and 180: publication checks known bindings and adapter admission; dispatch rechecks resolved subjects, current adapter authority and freshness under inherited non-amplifying rights. Host-owned adapter ports supply these checks, never client JSON or pinned decisions. Missing adapters and unknown authority fail closed. No new grant database or sandbox is authorized.

### 2026-11-10T10:05:41.456Z - The coordinator clarified the adapter-owned fail-closed authority boundary and the approved plan now records it. No missing universal authority store is assumed. I have not implemented the catalog, production entrypoints or tests; all acceptance criteria remain open. A safe implementation must bind public publication and dispatch to a trusted host-owned adapter port without serializing caller-supplied authority and must preserve legacy behavior.

### 2026-11-10 — 002F reusable operation contract

Consumers save and load immutable project-scoped script/subflow descriptors, then publish instances whose descriptor and target content are pinned. Script source uses the existing explicit versioned shell invocation; subflow targets use exact publication revision/content pins, with descriptor-level IO/outcomes matched to the child interface. WorkflowStore transactions and the current catalog/handler inventory supply state; no external execution or authority is assumed. Consumers are catalog validation, publication closure, presets and admission. Revision checks reject concurrent stale writers; closure traversal retains depth 4 and 32 expanded-edge bounds. This slice adds contracts and storage, not mappings, execution, connector writes or the pending A/B acceptance change.

### 2026-11-10 — 002F verification scope before execution

The targeted final run covers reusable-operation storage/output contracts, catalog pins, neutral publication closure, presets, and admission. It catches source edits changing old pins, omitted descriptor-backed child edges, interface/pin mismatch, stale writes, rehashed identity mismatch, malformed results and unresolved child arguments. Existing unaffected runtime/domain suites are not rerun. All nine acceptance criteria remain open: actual neutral runs, live dispatch authority/freshness, model/script/native adapters and authoring are not proved by this slice. The unselected A/B sequencing choice is unchanged.

### 2026-11-10 — 002F targeted result and remaining sequencing dependency

Implemented immutable project-scoped reusable descriptors, explicit inline script and exact subflow targets, declared workflow outputs/outcomes, target/interface closure verification, and descriptor output validation. Descriptor-backed child calls now refuse unresolved dispatch arguments just like direct child references. Neutral publication format 4 explicitly refuses formats 1–3; no existing runtime or live data was migrated.

GREEN: On rb, `scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(workflows::catalog_tests::) | test(workflows::neutral_publication_tests::) | test(workflows::neutral_admission_tests::) | test(workflows::activity_preset_tests::) | test(workflows::reusable_operation_tests::)'` passed 38 targeted tests. Invoked through build-slot.sh --remote and tuic-bg.sh; receipt `~/Gits/.tmp/rb/20261110T142738Z-87957/final-receipt.json` has command_exit 0, stale false and box_dirty false in companion metadata. Source snapshot SHA-256: `9c323b5022119b1aa4ccd45abf4e3542ef1d0a2303b09e3321f0095943c42546`, based on HEAD `0c54c38d0735e6f261a7ba760b22420092cd5751` plus this slice. All eleven tested Rust source/test hashes matched after completion; retained at `~/Gits/.tmp/workflow-rework/002f-source-hashes.txt`. Test-profile compilation took 3m40s; test execution 0.184s; peak RSS 14976296 KiB. Runner metadata explicitly leaves target/toolchain versions unmeasured; no version-specific claim is made. One existing dead-code warning names HostContext::as_any; no test failed. Rustfmt, whitespace and instruction-link checks passed. No native Mac, GUI, live model/connector or external-write evidence is claimed.

The independently executable operation-contract gap is addressed, but no whole acceptance criterion is checked prematurely. In particular criterion 5 requires current authority/freshness at actual dispatch, criterion 4 describes an actual run consumer, and criterion 2 describes run-level closure pins. Those consumers are owned by 004-2809 and 008-22c4 (with 003-e678 mappings and 005-f907 effects), while 003 depends on completing 002. The report's A/B choice remains unselected. The smallest decision is to approve the explicit verification-layer clarification (A), or authorize pulling the minimal real dispatch/native-adapter work into 002 (B). No new runtime or generic permission store is invented to evade that dependency. Story 002 remains in progress; 003–019 remain pending.

### 2026-11-10 — 002F review correction contract

Consumers must reject a user-defined target naming a native or wrong-kind handler, reject a saved ID colliding with the supplied catalog, and build a project catalog only from host descriptors plus verified stored operation versions. Exact handler-kind bindings come from the backend-owned HostRegistry, never descriptor JSON or caller-supplied claims. Stored row identity/content and child publication interfaces remain authoritative data, not permission; no external execution is assumed. Risk is high at the admission/integrity boundary. Named bugs: native-handler impersonation with empty capabilities (F1), isolated validation concealing descriptor collisions (F2), and dependency-order-sensitive catalog loading. Builder work is bounded by the finite stored project versions; subflow verification retains the existing depth 4 / 32 expanded-edge limits, with no new persistent authority or scheduler. Pre-fix source is 2f58b2cf2568; decisive review evidence is the reusable descriptor's availability-only handler check. No in-story RED run under the active adversarial-TDD rule; subsequent targeted tests cover changed files only and preserve the unchanged 002F results.

### 2026-11-10 — 002F correction validation scope

The changed reusable-operation and admission test files alone cover the correction: a target cannot impersonate a native/wrong-kind/other-version handler even with empty capabilities; save/get/project-catalog loading and direct admission all refuse it. Save and catalog build reject ID collisions, while exact verified existing-operation reads remain valid. The project builder loads all verified versions before resolving child dependencies and isolates projects. Existing catalog, publication and preset test-file hashes are unchanged and retain the compatible 38-pass slice evidence; they are not rerun. No positive live authority or A/B selection is claimed.

### 2026-11-10 — 002F review correction result

Corrected F1 by binding exact Script/Subflow handler versions through trusted HostRegistry registration and checking that binding on save, verified read, project-catalog load and direct publication/dispatch preflight. Native/wrong-kind/version impersonation with empty capabilities is rejected before adapter invocation. Corrected F2 by refusing catalog ID collisions at save and build, while preserving exact verified existing-operation reads. The store-backed project builder hydrates all verified saved versions before resolving subflow cross-references; its reopen test deliberately sorts a composite before its script dependency. No routes, scheduler, storage schema, positive grant or runtime adapter were added.

GREEN: `scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(workflows::neutral_admission_tests::) | test(workflows::reusable_operation_tests::)'` passed 15 tests on rb through build-slot.sh --remote and tuic-bg.sh; receipt `~/Gits/.tmp/rb/20261110T144713Z-85753/final-receipt.json`, exit 0, stale false, box_dirty false. Snapshot SHA-256 `2efbfb7159821297f7f5a7ffaf110ce629ba2df018d593290fcc620c02fd8606` is based on 2f58b2cf2568 plus the correction; all eleven source/test hashes match the remote files map and `~/Gits/.tmp/workflow-rework/002f-correction-source-hashes.txt`. Compile took 3m41s; tests 1.096s; peak RSS 15745320 KiB. The existing as_any dead-code warning remains; no test failed. Runner toolchain/target versions remain unmeasured. Catalog/publication/preset source and test-file hashes are unchanged, so their compatible prior green evidence is reused without rerun. No native/GUI/live adapter or external-write claim.

TEST-CHANGE: neutral_admission_tests.rs::dispatch_refuses_unmapped_child_arguments — the former composite used a native host.read handler; it now uses a backend-registered Subflow handler because target kind is a host-owned routing invariant. Reusable-operation tests now pass the trusted registry rather than an availability set; their behavioral expectations are retained. The pre-fix commit is 2f58b2cf2568; no in-story RED was run, and after-the-fact pre-fix regression verification remains separate.

All nine 002 criteria remain unchecked; the genuine A/B runtime-acceptance sequencing block is unchanged. This correction completes the bounded review requests and the independently authorized project-catalog consumer, not real execution or live authority.


### 2026-11-10 — 002F after-the-fact behavioral regression proof

RED verified against exact pre-fix commit `2f58b2cf25688e988e160ab0dcd0e83a15c0cd27`, not against reverted guards on current code. The immutable rb ref snapshot compiled and executed both existing regression oracles in a test-only projection: `reusable_targets_cannot_impersonate_native_or_other_handler_kinds` and `save_and_project_builder_refuse_catalog_collisions`. Both failed on behavior (nextest exit 100), not compilation or missing APIs. Script targets naming native, Subflow or wrong-version handlers were accepted by real store save/get; native publication/dispatch reached the original `NativeMustNotRun` panic sentinels, while the wrong-kind Subflow adapter returned success. A Subflow target naming the Script handler also saved successfully. The collision test failed at its original save rejection assertion: `called Result::unwrap_err() on an Ok value: ()`.

Projection adaptations: retained original descriptor/native-host fixtures and rejection predicates; changed only old store API wiring from HostRegistry to its handler inventory and old registry setup to direct handler insertion. Removed the new registry-registration assertion and project-builder assertions because those APIs do not exist before the fix. The collision projection retains the first save assertion only; builder/reopen behavior is a new feature, not a pre-fix regression. F1 catches each assertion panic and aggregates failures so get, publication and dispatch assertions execute after save fails. Each forged save uses a fresh real WorkflowStore to avoid the seeded row causing an unrelated stale-version error; publication uses a separate ID so an incorrectly successful publication does not prevent dispatch setup. No production behavior was copied into a helper or changed. Six production-source hashes were checked before execution; the temporary cfg(test) module declaration and replay file were restored/removed in finally. Evidence inputs and exact generated sources remain under `~/Gits/.tmp/workflow-rework/red-proof/` (`old-payload.json`, `fixed-payload.json`, `old-replay.rs`, `fixed-replay.rs`, `worker.py`, `run.sh`, `proof-receipt.json`).

Commands: launched once through `tools/agents/tuic-bg.sh .../red-proof/aggregate.log -- bash .../red-proof/run.sh`. The script serializes two `RB_REF=<exact commit> RB_STDIN_FILE=<test-only payload> RB_HANDLE_FILE=<handle> tools/build/build-slot.sh --remote python3 -c <worker.py>` invocations. Each worker runs only `scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib --no-fail-fast -E 'test(workflows::red_proof_tests::)'`, with a 1800-second command lifetime bound. No local cargo, full suite, app, coverage, mutation or clippy run occurred.

RED receipt: `~/Gits/.tmp/rb/20261110T150325Z-74330/final-receipt.json`, command_exit/exit 100, stopped/released true, artifacts collected; result stale=no, box_dirty=no. Source snapshot SHA-256 `f2cbeabbcfef2b1c75c8cee7db4ca3c0e43f2ff5d9fbb711ea914ce5386344ce`; replay SHA-256 `ec4e93c78730fcf984324952e48a8286e356c207240ac88f30ec77066ba2fb32`. Pre-fix reusable-operation source SHA-256 `8734b63854f6b8f05360bb291051c5d06cd38168921ef051700868e85182c5e2`; admission source SHA-256 `c3bf7caa8eb16b3a1bb4c8d13b0ec46b85640d3b53bab899ca3374d45366720a`. Compile 3m43s, test summary 0.669s, peak RSS 15723516 KiB. Full intended assertion diagnostics are in `red-proof/old.log` and the receipt directory log.

Focused false-RED check: the same two projected tests passed on exact fixed commit `b9ca056d37593eb44947a96a087d96312bd5dc6d`, with only the reverse API wiring adaptation. Receipt `~/Gits/.tmp/rb/20261110T150855Z-29109/final-receipt.json`, command_exit/exit 0, stopped/released true, artifacts collected; result stale=no, box_dirty=no. Snapshot SHA-256 `345dda5a9f2c7d6c840c37f2c0b4b88a410c50267a9e4a00d6717dc300ee80fc`; replay SHA-256 `8cf7583181afcf01976111e3e16bcc10497fbc1ba43af7c855f2e2991865a4c8`. Compile 3m33s, two tests passed in 2.124s, peak RSS 16093604 KiB. Logs: `red-proof/fixed.log`. Both runs recorded rustc 1.99.1 (58cf8f9c3), cargo 1.99.1 (5f94df478), cargo-nextest 0.9.146 (8af696ddc), host aarch64-unknown-linux-gnu. Both emitted only the existing HostContext::as_any dead-code compiler warning in GREEN; RED panic output is the expected behavioral evidence.

The eleven current production/test hashes still match `002f-correction-source-hashes.txt`, including original regression test source SHA-256 `2d2d65b5c08373e2045c9def5e60f565f51e401dd42d16650ef656d10cc1dab7`. Therefore the recorded 15-test correction GREEN and compatible initial 38-test evidence remain relevant and were not rerun. Risk remains high at the authority boundary; this task changes evidence only. No acceptance checkbox, story status, A/B decision, production source, native execution, live authority or dispatch-effect claim is changed.

### 2026-11-10T15:25:03.559Z - Boss authorized the verification-layer correction on 2026-11-10: "quindi correggi le storie e parti!". Obligations are transferred, not removed: 002 owns persisted neutral descriptors/catalog/instances/presets/reusable targets, typed inputs/optional subject, exact transitive definition/schema/handler/inline-prompt/subflow pins, output validation and fail-closed host interfaces; 003 owns explicit mappings/structured subflow IO/publication compatibility; 004 owns actual run closure/input/subject pins and trusted admission before EVERY resolved dispatch including subflows; 005 owns exact effect/approval correlation, uncertainty and cancellation; 008 proves real native authority/freshness/revocation, stale-artifact refusal, non-amplifying rights and missing handlers with service invariants; 018 retains all integrated execution/Designer/MCP/connector acceptance. Port/storage fixtures are not runtime or real authority proof. No dependency was dropped, and no cutover/live write/restart/story-store migration is authorized.

### 2026-11-10T15:25:04.762Z - Acceptance audit at HEAD 9d3b4bf9df24af341adc959529b3ae51a5a41ca0: all eleven hashes match 002f-correction-source-hashes.txt. C1: catalog register/validate_instance and duplicate/default/type/pin tests. C2: canonical full descriptor/config hashes, CatalogClosure and derive_subflow_pins/read_verified; child_revision_and_full_content_remain_pinned_across_reopen and descriptor drift tests. C3: activity_preset save/get/apply plus reusable-operation save/get/build_project_catalog; exact inline script and subflow targets covered by preset/reusable tests. C4: NeutralDefinition::validate_values and workflow_input_and_subject_schemas_are_enforced, including absent optional subject coverage in admission tests. C5: HostContext/HostAdmission/HostRegistry backend-only interfaces, publication checks every subject and resolved-instance callbacks; missing context/handler/capability refusal tested. C6: resolve_pinned exact handler diagnostic; native impersonation/collision regressions pass and graph JSON cannot supply grants. C7: NeutralDefinition has no domain kind, instances use open namespaced descriptors; model/script/tuic descriptor fixtures express contracts only, not executable adapters. C8: validate_output and validate_instance reject unknown outcome/missing/wrong values; client_labels_cannot_supply_authority rejects forged grants. C9: implementation is immutable descriptor storage plus backend-owned registry, no code/plugin loader. Reuse 38-test rb receipt 20261110T142738Z-87957 plus changed-file 15-test receipt 20261110T144713Z-85753 and exact-pre-fix RED/current GREEN receipts 20261110T150325Z-74330/20261110T150855Z-29109. All are already root hash-verified; no duplicate run. Toolchain rustc/cargo 1.99.1 and nextest 0.9.146 measured only in behavioral replay, aarch64 Linux. No native, UI, live adapter or external-write evidence claimed. Risk high: shared integrity/admission contract. Downstream runtime requirements now have explicit owners and unchecked criteria.

### 2026-11-10T15:25:22.630Z - Lifecycle preflight required the existing pre-implementation reality contract in its machine-readable criterion form. Added and verified that projection from the earlier 002A–F contracts, without claiming a new test or runtime behavior.
