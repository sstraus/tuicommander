---
id: "2959-3ee2"
title: "WER 010: enabling a trigger binding must not start one run per existing open issue or PR"
status: complete
priority: P1
type: fix
created: 2026-11-11T12:45:22.813Z
updated: 2026-11-11
dependencies: []
---

# WER 010: baseline enabled GitHub trigger bindings

## Problem Statement

Critic-010-r1 F4 on 63cdc33fe2: first observation emits opened and unchanged observations re-emitted retained source events, so enabling a binding could run the existing open-resource backlog. Coordinator assigned this follow-up after F1–F3 fix 27d30141dc. Original native story in main/stories disappeared during validation; stories-cli cannot find it. This committed worklog preserves the assigned criteria and evidence for coordinator reconciliation; no task cancellation was received.

## Acceptance Criteria

- [x] RED: enabling_a_binding_does_not_admit_preexisting_open_resources - catches: one run per existing open issue/PR at enable time. _(Evidence: intended RED failure in rb 20261111T125817Z-18827; final rb 20261111T130902Z-54984, 12/12 GREEN.)_
- [x] RED: unchanged_snapshot_does_not_re_emit_retained_event - catches: backlog replay on every poll. _(Evidence: intended RED failure in rb 20261111T125817Z-18827; final rb 20261111T130902Z-54984, 12/12 GREEN.)_
- [x] An enable-time baseline records the current generation per resource for that binding; only later generations admit; the baseline survives restart. _(Evidence: complete-source-poll activation cut, persisted baseline intents and real owner release/reacquire in enabling_a_binding_does_not_admit_preexisting_open_resources; rb 20261111T130902Z-54984, 12/12 GREEN.)_
- [x] Explicit replay of a pre-existing resource stays possible only through the owner-authorized replay path. _(Evidence: wrong-project refusal and authenticated-owner replay of retained issue/PR baseline snapshots in enabling_a_binding_does_not_admit_preexisting_open_resources; rb 20261111T130902Z-54984, 12/12 GREEN.)_
- [x] GREEN: `scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(neutral_trigger) | test(neutral_github)'` on rb 20261111T130902Z-54984, 12/12 passed (author, tree 0acbbf79b0); critic-010-r2 fix 63f8801457 verified by rb 20261111T132330Z-64373 (`-E test(github_source_refuses_missing_installed_catalog_before_network)`, 1/1 passed, exit 0).
- [x] REALITY: effect=enabling a GitHub binding starts no run for an issue/PR that exists before its first complete poll, and starts exactly one run per later generation ; state=RunStore neutral_trigger_baselines marker per project/binding/revision plus baseline-disposition intents in neutral_trigger_intents ; assumptions=GitHub list pages return the same fields as the single GET (title, body, labels, head/base sha, updated_at), per REST API version 2026-03-10 named in from_rest, not live-verified by the critic ; radius=all GitHub neutral trigger bindings of every project polled by github_poller.
- [x] REALITY: effect=an unchanged poll emits no new source event ; state=neutral_github_sources document per resource ; assumptions=updated_at is RFC3339 and monotonic per resource ; radius=GitHub source normalization shared by all projects.
- [x] SYMPTOM: enabling a binding started one run per existing open issue/PR (critic-010-r1 F4) — replayed by: enabling_a_binding_does_not_admit_preexisting_open_resources
- [x] SYMPTOM: every poll re-emitted the retained event of each unchanged resource (critic-010-r1 F4) — replayed by: unchanged_snapshot_does_not_re_emit_retained_event
- [x] TEST-CHANGE: src-tauri/src/workflows/run/neutral_github_tests.rs::issue_generations_track_material_and_label_epochs_not_comment_noise — old expectation wrong because an unchanged or comment-only snapshot re-emitted the retained event (F4 backlog replay); the retained identity is now asserted through retained_github_events.
- [x] TEST-CHANGE: src-tauri/src/workflows/run/neutral_github_tests.rs::pr_generations_require_exact_head_and_base_and_ignore_unrelated_ci_updates — old expectation wrong because a CI-only PR update re-emitted the retained event (F4 backlog replay).
- [x] TEST-CHANGE: src-tauri/src/workflows/github_event.rs::tests::github_source_discovery_pages_independently_and_refuses_truncation — old expectation wrong because discovery returned numbers only and forced one GET per open resource (F1); moved to neutral_github_poll_tests.rs and asserts list snapshots from open_snapshots.

## Work Log

Reconstruction: the main-store file vanished during this run, cause unknown. Per coordinator instruction, preserved the original id/title/P1/fix and four criteria in workflow-stories/, the rework story home.

### Contract before implementation

Consumer effect: enabling a GitHub binding establishes its current resource generations before automatic work. Existing synchronous enable records the revision; the next complete existing source poll captures the baseline and completes activation before any run. Events before that source activation cut are baseline. Subsequent generations admit. Each new/re-enabled binding establishes its own cut. Authenticated-owner explicit replay can select retained baseline snapshots, including actions not selected for automatic admission.

Authoritative state: WorkflowStore enabled revision, RunStore persisted activation marker plus existing immutable trigger intents/source snapshots. Baseline intents retain the pinned typed start request without pending execution. Readiness persists last; an incomplete/crashed baseline remains inactive. Unchanged normalization returns no new events, while retained-source outbox data supports crash recovery. No extra scheduler, credentials, source client or enable-time network callback. Affected consumers: GitHub batch adapter, shared owner admission/replay, source normalizer and activation fixtures. Contract recorded in the assigned native story before implementation and sent to coordinator message19e689bf-348b-41a1-8d92-d6b62c041a9a.

### RED before production edits

rb **20261111T125817Z-18827**, nextest **f25b3f59-fe66-486b-b1ae-2d8e5f452b98**, both named tests compiled and failed the intended assertions: pre-existing resource started at enable time; unchanged snapshot re-emitted retained event. Exit100, box_dirty=no. Command: `scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib --no-fail-fast -E 'test(neutral_trigger_baseline_tests)'`, remote build-slot + tuic-bg. Production tree was 27d30141dc with only regression tests/module registration added.

### Implementation and GREEN

Added neutral_trigger_baselines activation markers keyed by project/binding enabled revision in the existing RunStore. Baseline immutable intents have disposition baseline and never enqueue pending execution. Complete GitHub source batches normalize, baseline unready bindings, then admit ready bindings under the existing owner lock. Marking readiness last permits safe retry after interruption. Re-enabling/revising invalidates the old readiness marker through the existing revision. Explicit replay retains the existing authenticated-project-owner check and uses the stored baseline start request.

Unchanged observations emit no new normalized source event. The durable source outbox remains available after a normalization/admission crash gap. Unavailable resource histories may be baselined but are excluded from automatic admission. Existing domain fixtures explicitly establish an empty complete source cycle before ordinary admission tests; comment/CI noise tests still prove no new generation and separately inspect retained source identity.

Final targeted rb **20261111T130902Z-54984**, nextest **49b553a4-9291-47c7-9c54-8017b28305a8**, **12/12 GREEN**, exit0, stale=no, box_dirty=no. Uploaded HEAD tree **0acbbf79b0e8615eb227bbbaf7f5ac046020db42**, full source manifest SHA256 **a05343e936ac468e642941fcaa76a7f85b7654d0f25086056452793d6c256d85**. Command: `scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(neutral_trigger) | test(neutral_github)'`, remote build-slot + tuic-bg. All 1708 Rust source/Cargo manifest hashes match the final receipt. Rust1.99.1 observed compiler path; nextest/target versions unmeasured. Receipt ~/Gits/.tmp/rb/20261111T130902Z-54984; log ~/Gits/.tmp/wer-010-github/baseline-green-rb.log.

The final baseline regression covers both issues and PRs, real owner release/reacquire, retained per-resource generation/readiness, a later new binding, wrong-project replay refusal, authenticated replay with original typed inputs, later admission to both ready bindings after a normalization/admission gap, updated-only PR selectors and exclusion of unavailable retained source events. The second named regression proves unchanged normalization returns no events. Existing mapping, prefix, overlap, dedup, replay, uncertain-effect coalescing, bounded-list request count, failed-resource isolation and missing-host tests also passed.

Only shared file run/mod.rs gains additive module declarations. No excluded agent/model/native files changed. Docs describe the first-complete-poll cut and owner-only explicit replay; to-test records the post-rebuild live check. No live external writes, app restart, push or landing. Rust changes require an operator-chosen rebuild/restart after integration; none performed.
