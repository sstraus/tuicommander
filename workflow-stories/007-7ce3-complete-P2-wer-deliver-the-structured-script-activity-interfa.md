---
id: 007-7ce3
title: "WER: Deliver the structured script activity interface"
status: complete
priority: P2
type: feature
created: "2026-11-10T08:29:18.825Z"
updated: "2026-11-12T00:57:23.000Z"
dependencies: ["005-f907"]
plan: plans/workflow-engine-rework.md
plan_step: Step 4
depends_on: ["workflow-stories/005-f907-complete-P2-wer-own-durable-asynchronous-activities-effects-an.md"]
---

# WER: Deliver the structured script activity interface

## Problem Statement

Workflow engine rework 007-7ce3. Scope: reuse command executor seams for versioned executable/interpreter/script references, working context, structured input and bounded result.

Delivery prerequisites: 005-f907. These native prerequisites also appear in the linked plan and dependency metadata. All work remains pending.

Contract-first feature: record consumer effect, authoritative state, external assumptions and affected consumers before implementation. Reuse existing services/evidence where compatible; no mandatory duplicate full suites or extra critic rounds. Planning creates pending work only and does not authorize implementation or live effects.

## Acceptance Criteria

- [x] Configuration declares IO/config schemas, named outcomes, timeout, output/resource limits, environment/secret references and side-effect/retry/reconciliation policy.
- [x] The result envelope has contract version, activation/attempt identity, outcome, data and artifact references; stderr diagnostics stay separate.
- [x] Exit zero with invalid/missing required JSON is protocol failure; explicit exit-status mode remains available for simple commands.
- [x] Lost output after a mutation yields uncertainty and declared reconciliation, not an assumed safe idempotent retry.
- [x] Cancellation records process-stop and effect certainty separately.
- [x] A custom script activity registers, renders and executes without a kernel case or bespoke form.
- [x] External scripts own one honest activity boundary, not the outer workflow.

- [x] REALITY: effect=bounded owned process returns fenced typed JSON and separate diagnostics; state=retained catalog publication, RuntimeOwner admission and durable NeutralEffect dispatch (neutral actor test); assumptions=owned Python test script on rb Linux, existing process executor, explicit daemon environment references; radius=script adapter, command environment/capture helpers, cancellation registry and neutral ledger; no installed provider or live external service claimed.
- [x] GREEN: adapter cases 11/11 on rb task 20261111T110147Z-11743 (source SHA256 826457b99898a1186850f0efde86ddc37db7eb2d90fd0f97cbdb3c05bb11ae71), plus the added descriptor drift assertion 1/1 on rb task 20261111T111123Z-51879 (source SHA256 1f0512beab73f40d627fa47cd80ac5768fb4b09572543a1ebc87ca4b08ef1f3a); exact targeted commands and reuse scope below.

## Work Log


### 2026-11-11 — Contract before implementation

REALITY: effect=one bounded owned process receives immutable versioned JSON input and returns a fenced typed outcome/data/artifact envelope, with stderr separate; state=RuntimeOwner commits NeutralEffect intent and dispatch before the script adapter enters, catalog schemas and the retained NeutralInvocation supply resolved config/input/subject and identity; assumptions=local interpreter from the pinned inline Invocation contract, process ownership/cancellation from execute_process, daemon environment references resolved at entry (portable owned-process tests will execute on rb; no installed external scripts or live effects claimed); radius=neutral script handler, reusable descriptor admission, shared command invocation/environment/capture seams, existing cancellation registry; graph routing remains owned by NeutralExecution.

Implementation plan: add a generic script adapter using the existing owned-process executor (not the Git-specific command journal), exact reusable handler registration, bounded policy and request/result types, strict output checks and conservative observation-only reconciliation. Script configuration cannot choose a new executable or move workflow routing into the process. Reuse the ledger for pre-entry correlation; lost output never causes dispatch. Cancellation requests stop through the existing registry and retains uncertainty until the original invocation supplies evidence.

Named boundary cases: real interpreter JSON echo preserves null/arrays/objects and separate stderr; malformed/missing/foreign result envelopes refuse even at exit zero; explicit exit-status mode; output and input limits; a mutating process with lost output is uncertain and never executed again by reconcile; cancellation stops its owned process but does not imply rollback. Existing coverage does not exercise a neutral script adapter because none is installed. Generic Designer rendering ownership is an open question sent to the parent: the current UI is legacy-only and 014 owns neutral authoring. No descriptor fixture will be counted as rendering proof.

### Accepted rendering proof handoff

COORDINATOR2 confirmed (mail 984eda35-7240-4b89-b360-7b9976dc5cb0) that 007 must not build a duplicate Designer/transport slice. Under the accepted verification-layer correction, AC6 here proves generic registration, published renderer-consumable schemas/outcomes/policies, and neutral-runtime execution. The visual obligation “custom script renders in the Designer without a bespoke form” moves to its implementing consumer 014-5d73; its worklog records the inheritance. AC6 text remains unchanged. No visual proof is claimed by a fixture.

### First targeted rb result and setup correction

Frozen staged tree `016cf3a6db3e927376305138b8abc501756c80dd`; rb task `20261111T104733Z-68861`, nextest run `87f80872-ad0a-47ce-b101-e1058dd1f90e`: 11 executed, 2 passed, 9 failed, exit 100. All nine failures occurred before process entry at fixture preparation: the fixture supplied unresolved instance config, while the durable ledger correctly requires the same resolved config used by production admission. The real neutral actor/custom descriptor test passed, as did policy validation. Fix: the shared fixture now calls the actual `admitted` path before retaining the effect; remove one unused test import. No production behavior changed for this correction. Rerun the affected module because the shared test fixture and test file changed; no unrelated suite.

### Adapter verification GREEN

Corrected frozen staged tree `040e97ae848ffb7997fd558d836207b2d9ce6ab5`; targeted command `build-slot.sh --remote scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(workflows::run::neutral_script::)'`. rb task `20261111T110147Z-11743`, nextest `7399cb89-e0c9-4946-a53d-d3627a074cb2`: 11/11 PASS, exit 0, no compiler warning. All 213 frozen source/config hashes match after the run. The receipt must still be checked before final handoff. No native test, provider install, app restart, live write, push or landing occurred.

Parent's additional fixture requirement (mail bc3634dc-b01b-49d6-9bd4-69047ca83257): capture the backend-serialized published custom script descriptor and check it against the serializer for 014. Add one independent wire-drift test. Its explicit capture mode is an artifact generator, not GREEN assertion proof; final assertion mode will run against the committed fixture. Existing 11 behavior tests retain their matching source hashes; only the new fixture test requires execution after capture.

Fixture capture: rb task `20261111T110652Z-89920` emitted exactly one `TUIC_SCRIPT_DESCRIPTOR=` line from the published retained descriptor serializer. Save those actual bytes as `src-tauri/src/workflows/fixtures/custom_script_descriptor.json`; no hand-written descriptor was substituted. Capture mode is not final drift-check proof. Adapter GREEN receipt `20261111T110147Z-11743` checked: source tree SHA256 `826457b99898a1186850f0efde86ddc37db7eb2d90fd0f97cbdb3c05bb11ae71`, clean/nonstale, exit 0, matching task/generation/fence, stopped/released, artifacts collected. Only the new fixture-drift test now needs a final default-mode run; the preceding 11 test files and helper contents remain unchanged. The sole existing module-file change adds a cfg(test) module declaration.

### Final criterion evidence

| AC | Evidence |
| --- | --- |
| 1 | `ScriptPolicy::resolve` declares bounded policy in generic descriptor config; `script_policy_checks_limits_references_and_mutation_requirements`, stream/input limit and environment-reference cases PASS. Descriptor owns config/IO/subject schemas and named outcomes. Resources are bounded ledger claims, not OS CPU/RAM quotas. |
| 2 | `ScriptRequest`/`ScriptResult` retain version and full EffectKey fence; `structured_process_preserves_json_artifacts_identity_and_separate_stderr` proves null/array/object preservation, artifact ports and diagnostic separation. |
| 3 | `exit_zero_invalid_missing_foreign_or_wrong_shape_json_is_protocol_failure` covers eight rejected protocol cases; `explicit_exit_status_accepts_plain_stdout_and_no_required_data` proves opt-in simple commands. |
| 4 | `mutation_with_lost_output_remains_uncertain_after_stop_replay_and_reconciliation` observes one actual file write, retained uncertainty after owner replay, declared manual procedure, no second write or result. |
| 5 | Active cancellation, pre-entry cancellation and timeout cases prove process-stop independently of mutation certainty. A successful signal alone returns no stop/rollback proof; original worker supplies actual stopped evidence. |
| 6 | `custom_descriptor_publishes_renderer_data_and_executes_through_neutral_actor` proves exact generic handler registration, retained renderer-consumable descriptor fields and real process execution through the existing actor. The captured published descriptor and 014 worklog handoff carry the inherited Designer visual obligation, as explicitly directed by COORDINATOR2. `published_custom_script_descriptor_matches_shared_renderer_fixture` default mode now PASS. |
| 7 | Script receives one retained activation and returns one typed outcome; only NeutralExecution routes its successors. Actor case reaches workflow output `result=7` and one effect; adapter has no successor, retry, loop or scheduler operation. |

Final fixture assertion: staged Git tree `3b19d0055fce3f88c13145323084f05b8846ddbf`; command `build-slot.sh --remote scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(published_custom_script_descriptor_matches_shared_renderer_fixture)'`, rb task `20261111T111123Z-51879`, nextest `4c965ac9-8c0e-4edd-a908-deab74d0c6c3`: 1/1 PASS, exit 0, no warning, 215 frozen source/config/fixture hashes match. Handle/final receipt task, generation and fence match; stopped=true, released=true, artifacts=collected, stale=no, box_dirty=no. Existing 11 behavior cases reuse the prior matching GREEN; no full suite or duplicate gate. Capture-only run is excluded from assertion proof.

Fixture SHA256 `e5bd28582d2cd2184f59b3e09f74bc53b6f769209566dc53c693f0255608bf82`. It is actual backend-serialized JSON from rb capture task `20261111T110652Z-89920`, not authored response data. The final default test checks the entire published value.

Story routing limit: the mandated main-checkout `wiz-run stories-cli.js show 007-7ce3` fails with “story '007-7ce3' not found”; its resolver selects `tuicommander/stories`, while this approved story lives in `workflow-stories`. Criteria and evidence are recorded in this owned story file. No story-store migration or configuration change is authorized or performed. Parent owns acceptance/landing and eventual status completion; this branch remains in_progress until accepted. Internal adapter installation, native service authority, public transport and visual proof are the named later consumers, not claimed by these tests.

Risk: medium (shared process cancellation and effect ownership). Linux/rb process evidence only; no macOS live or desktop proof. Rust changes require an authorized manual rebuild/restart to load; no daily app restart, installed binary replacement, external write, push or merge occurred. Shared edits are additive `run/mod.rs`, three helper visibility changes in `command/executor.rs`, `docs/backend/workflows.md`, and the specifically requested 014 worklog handoff. Files of 006 were not edited.

Toolchain evidence limit: the rb mutation metadata explicitly records target/toolchain as unmeasured. A read-only post-run version probe (`ssh -o BatchMode=yes -o ConnectTimeout=10 rb@100.121.52.117` invoking only version commands) failed with `Permission denied (publickey)` / exit 255. No alternate key, transport, install or remote retry was attempted. Parent was asked for recorded versions or disposition; the passing run configuration and receipt identities above remain the measured evidence.

### Critic F1 contract (before fix)

Consumer effect: ordinary environment reference values must survive unchanged in typed script results; only secret reference values are redacted from captures. Authoritative classification is ScriptPolicy.environment_refs versus secret_refs; both maps still supply the process environment. No external assumption or transport changes. Affected consumers: structured stdout parsing and separate stderr diagnostics. Reuse critic RED commit 06d32a7f8c / rb task 20261111T112835Z-55984; run the requested script module filter once after this fix.

O1 follow-up contract: preserve the earlier unconditional Manual procedure bounds validation; mutation admission only needs to require the Manual variant and resources. Coordinator explicitly authorized removal of the duplicated bounds expression.

Critic fixes GREEN: F1 redaction uses only secret_refs while both reference maps still populate the process; docs now distinguish ordinary values. O1 removes duplicate bounds and retains the unconditional Manual procedure validation. Critic RED commit `06d32a7f8c`, rb `20261111T112835Z-55984`; final targeted command `build-slot.sh --remote scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(workflows::run::neutral_script::)'`, task `20261111T113538Z-63671`, nextest `3b491108-9a16-40b9-abe7-a0a1e31cc07b`: 13/13 PASS (includes critic regression). Frozen staged Git tree `6a2e510dc37e4e6653a7830398ca755a84a82e3c`; rb source tree SHA256 `cab4b446680f3a7b080deda56ee4adfacda2927015687be14e0cff0ddef12252`; 230 source/config hashes match after run. Receipt task/generation/fence match, exit=0, stopped/released=true, artifacts collected, stale=false, box_dirty=false. Toolchain remains unmeasured under accepted parent disposition. Prior attempt `20261111T113423Z-34595` exited 75 because O1 changed source during sync; remote command never started. rustfmt --check and git diff --check PASS. O2 accepted unchanged.

### 2026-11-12T00:57:23.000Z - Coordinator closure (integration record reconciliation, or-3 1787b94b): All acceptance criteria checked with evidence cited in this record; closed per ~/Gits/.tmp/workflow-rework/HANDOFF.md (implementation criteria complete, coordinator closure only). Documented limits in this record stay limits, not passes. No new test run.
