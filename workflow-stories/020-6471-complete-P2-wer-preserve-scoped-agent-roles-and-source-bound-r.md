---
id: 020-6471
title: "WER: Preserve scoped agent roles and source-bound reports"
status: complete
priority: P2
type: feature
created: "2026-11-11T16:47:29.938Z"
updated: "2026-11-13T00:51:09.000Z"
dependencies: ["006-6885", "008-22c4", "015-608c"]
depends_on: ["workflow-stories/006-6885-complete-P2-wer-execute-mediated-model-calls-and-scoped-develo.md", "workflow-stories/008-22c4-complete-P2-wer-expose-native-repository-and-development-opera.md", "workflow-stories/015-608c-complete-P2-wer-expose-the-same-workflow-capabilities-through-.md"]
plan: plans/workflow-engine-rework.md
plan_step: Step 5
risk: high
started_at: "2026-11-11T21:29:31.507Z"
completed_at: "2026-11-13T00:51:09.000Z"
---

# WER: Preserve scoped agent roles and source-bound reports

## Problem Statement

Old consumers: story_delivery implement, resolve_plan coordinate, and pr_review reviewer/validator. Their execution profile, prompt, inherited capabilities and story/run reports are not equivalent to an arbitrary model.request. The installed restricted-agent adapter explicitly refuses coarse execution because direct native privileges cannot be withheld. Close this remaining adapter gap by reusing mediated model calls and existing native service boundaries, without weakening that refusal or relaunching a legacy PTY scheduler.

This story owns the remaining role/profile/report adapter contract after 006; do not rebuild its mediated ACP transport or its existing tool/effect ledger.

## Acceptance Criteria

- [x] Before implementation, record the observable consumer effect, authoritative native state, external assumptions, affected consumers and owned paths in this worklog; audit existing implementation and compatible evidence first.
- [x] A configured role runs through an explicitly translated, pinned execution profile/prompt and typed input/subject contract. Return source-bound structured reports with run/execution/activation/attempt, artifact/generation and independent reviewer identity; preserve role responsibilities without moving scheduling into the model. _(verified: main d3a98e4562: run/native_agent_tests.rs native_agent_roles_execute_exact_mediated_profile_and_retain_source_bound_reports runs implementer/reviewer/validator/coordinator on the pinned profile and asserts source, effect, role and model-session identity in the report; read_report refuses a foreign generation. rb 20261111T215027Z-1380. Independent reviewer identity reaches the batch verdict through native_batch_evidence_requires_owned_settled_push_and_exact_reviewer_report)_
- [x] Reuse 006 mediated ACP/model tools, ModelToolPolicy, native admission and existing workspace/file/story services. Graph labels, requested tool names and source capability strings grant no rights. Recheck current inherited scope/revocation before every native tool effect; forbidden shell/Git/service access remains unavailable.
- [x] Demonstrate an actual bounded implement/review/report path under the installed native host. Unsupported source PTY/profile semantics must produce a precise conversion blocker, never a silent fallback or a success fixture. Retain tool-less fresh review isolation when required by the native review service. _(verified: main d3a98e4562: installed NativeHost runs the role reports above and native_agent_implementer_uses_owned_native_story_tool_and_current_scope (rb 20261111T215027Z-1380); converted consumers run them in composed_plan_execution_delivers_and_reenters_its_fix_owner (rb 20261112T215442Z-95613, 2962-554e). The converter refuses a role for a template without that agent (017 worklog). The tool-less reviewer rule is asserted in native_agent_contract_refuses_profile_drift_tools_and_stale_reports. Live model provider evidence stays an explicit gap)_
- [x] Targeted cases catch privilege amplification, stale/foreign report acceptance, resumed agent resetting owner state, changed profile/prompt altering a pinned run and restricted execution falling back to an ordinary launcher. _(verified: main d3a98e4562: native_agent_contract_refuses_profile_drift_tools_and_stale_reports refuses a profile override, a root role label, a tool on a reviewer, a stale source tree, a forged owner field, a replacement model session (BindModel) and asserts restricted_agent() fails instead of a launcher fallback; prompt edits change the effect key. native_agent_implementer_uses_owned_native_story_tool_and_current_scope covers current-scope privilege. rb 20261111T215027Z-1380)_
- [x] Expose versioned catalog inputs/outputs, named outcomes and exact handler/descriptor/prompt/subflow pins through existing neutral admission and the shared Designer/IPC/HTTP/MCP contracts; no legacy scheduler wrapper, role-specific kernel branch or caller-supplied authority. _(verified: main d3a98e4562: host.agent.role and native/agent.report.read@1 are in src-tauri/src/workflows/fixtures/neutral-native-catalog.json; native_catalog_wire_matches_shared_designer_fixture and the NeutralDesigner.native Vitest consume it (54 descriptors, 2962-554e worklog))_
- [x] Reuse the existing effect ledger for ownership, exact approvals, cancellation and original-effect reconciliation. A lost reply or uncertain effect never authorizes a fresh write; draft edits cannot alter an active run.
- [x] Prove named failure modes with real native services in throwaway local repositories and focused tests of changed paths/consumers. Record tree, command, task id, tool versions or unmeasured versions and result; reuse compatible passing evidence. No per-story full suite, clippy, mutation run or automatic extra critic.
- [x] Planning leaves this story pending and does not authorize implementation, cutover, restart, push, live user-data/store migration or live writes outside the already approved private sandbox repository. Any sandbox effect must keep its existing exact action/target approval and cleanup; missing live evidence remains an explicit gap. _(verified: no cutover, restart, push or live store migration was done for this story; live model evidence stays an explicit gap)_

## Work Log


### 2026-11-11T21:29:32.142Z - Contract before implementation (transferred from deleted duplicate020-2544): effect=delivery implementers, plan coordinators and PR reviewers receive exact mediated profiles, literal prompts, scoped tools and typed source-bound reports; state=backend ACP settings, NativeContext ceiling/current authority, original effect/session ledger and actual native Git artifact; assumptions=external model availability explicit, fixture only substitutes model protocol; consumers=native host/model, delivery/coordinate/review adapters; owned paths=native_agent modules, native_model and host integration. No broad launcher, role grants or fallback. Source origin/generation is retained source data and must be bound by upstream domain operation, never authority. Restricted coarse adapter unchanged.


### 2026-11-11T22:05:51.747Z - Native role/report adapter implemented and proved under installed NativeHost. rb20261111T215027Z-1380:27 passing compatible cases, including all new role tests; tree32b342ac307d8afb9be0b909887570980adf2a0d. Cohort-only support fix retains role evidence. Exact commands, task trees, failure classification and versions in docs/backend/workflow-agent-child-evidence.md. Real native story/Git services and mediated protocol fixture; live model provider and legacy consumer composition remain unproved. Issue peer received seam, production IssueServices binding still absent. No fixture regeneration/full suite/clippy/live restart/push.


## Delivery Notes

Size estimate: L. Planning only; status stays pending.

This story owns the remaining role/profile/report adapter contract after 006; do not rebuild its mediated ACP transport or its existing tool/effect ledger.

Authoritative existing seams: `src-tauri/src/workflows/run/neutral_model_adapter.rs`, `src-tauri/src/workflows/run/neutral_model_tools.rs`, `src-tauri/src/workflows/run/neutral_agent_adapter.rs`. The operation/consumer audit is `docs/backend/workflow-template-migration.md`; the actual catalog capture and evidence qualifications are `src-tauri/src/workflows/fixtures/neutral-native-catalog.json` and its sibling README. Inspect and reuse these; fixture presence is not live-service success.

### 2026-11-12 recovery reconciliation

Removed the stale pending copy of 020-6471. Both copies had the same acceptance criteria; this in_progress copy retains the checked evidence and both implementation worklog entries. No criterion is waived.

### 2026-11-13T00:51:09.000Z - Closed after the landing of 2962-554e on main d3a98e4562. The remaining gap (role/report composition through converted consumers) is closed by the 2962-554e composed plan execution. Evidence was checked against main by code and test-name inspection; no test was rerun.
