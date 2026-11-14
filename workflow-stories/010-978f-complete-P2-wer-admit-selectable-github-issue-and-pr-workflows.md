---
id: 010-978f
title: "WER: Admit selectable GitHub issue and PR workflows"
status: complete
priority: P2
type: feature
created: "2026-11-10T08:29:18.827Z"
updated: "2026-11-12T00:57:23.000Z"
dependencies: ["009-e32f"]
plan: plans/workflow-engine-rework.md
plan_step: Step 4
depends_on: ["workflow-stories/009-e32f-complete-P2-wer-deliver-typed-http-operations-and-the-shared-c.md"]
---

# WER: Admit selectable GitHub issue and PR workflows

## Problem Statement

Workflow engine rework 010-978f. Scope: reuse GitHub integration and event sources; deliver missing operations and binding/admission behavior, not a PR-only scheduler.

Delivery prerequisites: 009-e32f. These native prerequisites also appear in the linked plan and dependency metadata. Backend delivery completed below; Designer demonstration is transferred to 014 by coordinator decision.

Contract-first feature: record consumer effect, authoritative state, external assumptions and affected consumers before implementation. Reuse existing services/evidence where compatible; no mandatory duplicate full suites or extra critic rounds. Planning creates pending work only and does not authorize implementation or live effects.

## Acceptance Criteria

- [x] Issue opened/configured-label and PR opened/reopened/updated events select an enabled pinned workflow and map typed snapshots, repo/number/action/actor/head/base into normal inputs.
- [x] Several explicitly enabled matching bindings can each run; show overlap before activation, validate mappings, and remove UNIQUE(source, repository)/kind restrictions from the new contract.
- [x] Deduplicate per binding plus normalized resource/event generation across manual/poll/webhook, independent of policy digest; workflow edits do not replay source events.
- [x] Explicit replay requires authority.
- [x] PRs have one active execution per binding/resource and coalesce pending updates to latest relevant head/base; old CI/review cannot publish for newer heads.
- [x] Reconcile uncertain effects before superseding conflicting work.
- [x] Issues use relevant material/action/label-admission epochs, not every timestamp/comment.
- [x] Suppress only self-events proven by receipts plus configured actor filters.
- [x] Designer demonstrates trigger/workflow selection and payload mapping.
- [x] Fixtures and an authorized live event where available cover admission; existing polling is acceptable without new public ingress.
- [x] TEST-CHANGE: src-tauri/src/workflows/run/neutral_github_tests.rs::issue_generations_track_material_and_label_epochs_not_comment_noise — old expectation wrong because an unchanged or comment-only snapshot re-emitted the retained event (F4 backlog replay); the retained identity is now asserted through retained_github_events.
- [x] TEST-CHANGE: src-tauri/src/workflows/run/neutral_github_tests.rs::pr_generations_require_exact_head_and_base_and_ignore_unrelated_ci_updates — old expectation wrong because a CI-only PR update re-emitted the retained event (F4 backlog replay).
- [x] TEST-CHANGE: src-tauri/src/workflows/github_event.rs::tests::github_source_discovery_pages_independently_and_refuses_truncation — old expectation wrong because discovery returned numbers only and forced one GET per open resource (F1); moved to neutral_github_poll_tests.rs and asserts list snapshots from open_snapshots.

## Work Log

### Contract before implementation — 2026-11-11

Consumer effect: each explicitly enabled neutral GitHub binding selects one exact publication and maps named typed snapshot fields into workflow inputs/subject. Multiple matching bindings are independent. Enabling requires current overlap acknowledgement and a publication-valid mapping. Designer rendering belongs to 014; neutral-action transport belongs to 015, with exact domain shapes routed through COORDINATOR2.

Authoritative state: publication/binding metadata remains in WorkflowStore; normalized source snapshots, generations, binding receipts, retained start intents and pending replacement state remain in the existing RunStore under RuntimeOwner. The existing neutral run/event/effect ledger is the execution authority. Admission persists a deterministic start request before start; retries use its original publication and request identity. Policy edits cannot mint event generations. Each binding/resource has at most one active run; newer PR material supersedes via cancellation only after owned effects settle. Unknown/unstopped effects retain ownership and block replacement. No second scheduler or credential store.

External assumptions: repository/account resolution and API execution reuse github_pr_api::RepoApi/PrApi; current polling/event_bus is the source. Source identity includes normalized repository and resource number, relevant material/head/base and lifecycle/label transitions; timestamps order observations but comments alone do not generate work. Polling cannot prove a sender; absent actor remains explicitly absent and configured actor restrictions fail closed. Public webhook ingress is excluded. Provider fixtures cite official GitHub REST/webhook documentation; no credential or live write is authorized. Live account admission remains a named gap until a throwaway resource/account is authorized.

Affected consumers: 014 trigger selector/mapping/overlap inspector; 015 single neutral action variants; existing poller and WorkflowRuntime owner; native GitHub adapters retain current-head fencing and lookup-only review reconciliation. Legacy bindings remain untouched until separately approved cutover.

| Operation | Existing owner / reuse | 010 decision |
|---|---|---|
| Poll issue/PR snapshots | github_poller, GitHubIntakeIssuesUpdate/GitHubPrUpdate | reuse source; normalize durable generations |
| Resolve account/repository and REST/GraphQL | github_pr_api::RepoApi/PrApi | reuse, no new client/token store |
| Read PR subject, CI/review | automations/pr_reviews/subject, current GitHub services | preserve exact head/base identity |
| Publish review, reconcile lost response | automations/pr_reviews/effect | reuse current-head probe + commit_id and lookup-only receipt |
| Neutral bindings, overlap, mapping, replay | missing | implement in domain; expose via 015 handoff |
| Normalized admission, coalescing and receipt suppression | missing | implement on existing owner/run ledger |
| Designer selection/mapping demo | 014 | deliver backend contract; visual verification transferred |

Named tests will protect cross-channel duplicate admission/policy edit replay, mapping and overlap refusal, PR coalescing and uncertain-effect ownership, issue comment noise/label epochs, unauthorized replay and overbroad self-event suppression. Targeted rb validation only after implementation.

### Source-neutral consumer decision and handoff

COORDINATOR2 requested a shared core because GitLab 012 is a second current consumer. TriggerSource is a closed source enum; trigger_schema(source, kind) matches installed normalizers, not a dynamic registry or caller catalog. Generic bindings retain source/kind, resource prefix, event names, typed input/subject mappings, scalar/array membership filters and optional ignored actors. The same RuntimeOwner admission and RunStore receipts serve both sources. GitHub remains only a normalizer/client consumer. Owner authentication (IPC local owner, HTTP/MCP existing authenticated owner) is sufficient explicit replay authority; ReplayAuthority is backend-only and not deserialized. No new policy/approval layer.

Contract checkpoints: 21530a7b7c236d285cdd8933cb1589eaaed22b5c (generic types only), d6d2e44b106d8dd3058d17769502078054d43489 (GitHub snapshot/schema and closed match, also fixes the source-enum validator). Both are required and were sent to 012/015. Authorized -x host-accessor seam db022c8e04 was consumed as eb1431aaa1; no NativeHost implementation or branch merge was taken. Core operational facade is a separate follow-up.

014 handoff for AC9: consume TriggerSchema, TriggerBindings, SaveTriggerBinding, EnableTriggerBinding with exact overlap id/revision acknowledgement, PreviewTriggerMapping, and authorized ReplayTrigger through 015's single neutral action. Preview returns validated normal inputs/subject and current overlaps. Source fixture src-tauri/src/workflows/fixtures/github-issue-opened.json is documented, normalized and suitable for Designer preview; raw REST fixtures and their official source/version are adjacent. 014 owns visual trigger/workflow selection and mapping demonstration; 010 owns the backend contract and evidence. No visual success is claimed here.

Production source integration uses the existing GitHub poller and account-aware RepoApi/PrApi. Neutral path subscriptions are independent from legacy intake; updates cannot replace either path set. Startup and the existing periodic intake refresh configure the same poller; saving/enabling takes effect next cycle without a transport callback. Missing native catalog is a logged admission refusal. REST author is never treated as the update actor. Closed resources are re-read once after disappearing, and only open known resources remain in future polling candidates. Source timestamps order observations but material/head/base/lifecycle/label transitions define epochs. No public webhook ingress, new scheduler or credentials store.

### First core compile and test evidence (not final GREEN)

rb task 20261111T114310Z-52323 compiled the core and passed both neutral_trigger_tests (nextest d5feef8f-0a6a-4544-8f38-1324f4458909, 2/2). Command: scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(neutral_trigger_tests)'. Wrapper exit 75: local tree changed after upload while independent source wiring proceeded; receipt is stale, not a code/test failure. This is not the final pinned GREEN. Source work and host seam have since changed; final targeted validation will freeze the tree. rb toolchain versions are unmeasured. Core facade checkpoint remains distinct from final story completion.

Source discovery reuses the account-aware REST client with complete bounded open issue/PR pagination, independent of legacy labels and client draft filters. Issues returned with pull_request metadata are excluded from issue admission. The scheduler refresh runs before the empty-legacy-path early return, so a newly enabled neutral-only binding is discovered on its next cycle. The named discovery regression covers page two, PR exclusion and truncation refusal.

Final targeted attempt task 20261111T121218Z-72670 failed compilation (exit 101): github_auth deferred-token poller fixture omitted the new neutral_intake_paths field. No tests ran. Added that initializer, kept crate-visible field consistent with existing test construction, and included the affected deferred-probe test in the retry. This is a compile integration error, not a transport failure.

Targeted task 20261111T122045Z-88163 compiled and ran 12 tests: 11 passed, missing-catalog regression failed because its fixture copied only the main SQLite database while WorkflowStore retains a WAL connection. The copied store contained no binding, so the no-bindings early return correctly succeeded. Corrected fixture to VACUUM INTO a consistent snapshot and assert the enabled binding precondition. Production source unchanged. Nextest dc341797-fc03-4b4a-bd4b-93cef9558f44; source tree SHA256 727e7c644e9b5fe9a0c0b1ac12953e2d54c3d297bb4c8c70151aaa42b8bc2b7d, stale=no, box_dirty=no. The other 11 passing cases remain compatible; retry only the changed neutral_github_tests file.

### Final evidence and acceptance disposition — 2026-11-11

Final targeted rb GREEN: task **20261111T122742Z-96651**, nextest **0e476212-dcd7-4f4e-be9b-2524a56c1f45**, 3/3 passed, exit 0, stale=no, box_dirty=no. Uploaded tree **a9698c41025fce67018f6ad1e1509f15cc60f622**, full source manifest SHA256 **aae17bb2f31813fcdac293d25717dabe8ed0b836d0a651bd243afbeebf487d20**. Command: `scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(neutral_github_tests)'`, through build-slot.sh --remote and tuic-bg. Rust toolchain 1.99.1 is evidenced by the preceding compiler invocation path; nextest/target versions remain unmeasured. Receipt directory: `~/Gits/.tmp/rb/20261111T122742Z-96651`; log: `~/Gits/.tmp/wer-010-github/source-fixture-rb.log`.

Compatible passing evidence from task 20261111T122045Z-88163 is retained rather than rerun: independent bindings/mapping/overlap/policy-edit dedup, authorized immutable replay, PR coalescing behind uncertain effects and exact receipt suppression, bounded multi-page discovery, both overlapping legacy client filters, deferred auth nudge, and both stale-head review cases. Comparison of every src-tauri/src file plus Cargo manifests/lock against both rb manifests proves only neutral_github_tests.rs changed between runs. Every current source hash matches the final rb receipt. The earlier source-file failures and corrections are recorded above; no full suite or clippy was run.

| Criterion | Disposition / evidence |
|---|---|
| 1 typed pinned source admission | Raw REST normalization tests prove issue lifecycle/labels and PR head/base; independent_bindings test proves typed normal inputs and original publication retention. Existing poller calls installed catalog host; missing host refuses before credentials/network. Poll actor is absent rather than fabricated. |
| 2 multiple bindings, overlap/mapping | independent_bindings_validate_overlap_and_keep_source_receipts_across_edits passes. New table primary key is project/id, with source/kind selectors and no legacy source/repository uniqueness. |
| 3 cross-channel dedup | Same normalized event shared across delivery/poll, durable binding/resource/generation receipt ignores delivery and policy digest; independent_bindings and source generation tests pass. No new public webhook ingress. |
| 4 explicit replay | replay_requires_project_authority_and_retains_original_start_request passes; backend-only authenticated project owner authority and idempotent replay id. |
| 5 latest PR execution | latest_pr_generation_waits_for_original_effect_certainty_and_stop proves cancelled original and coalesced latest head, no middle run; existing head_change_prevents_posting_stale_approval and two_pushes_discard_stale_acp_result_and_post_latest_head_once pass. |
| 6 uncertain effects | Same coalescing test retains dispatched uncertain write ownership and refuses replacement until successful completion plus stop evidence. |
| 7 issue epochs | issue_generations_track_material_and_label_epochs_not_comment_noise passes, including label removal/re-add and stale material refusal. |
| 8 receipt/actor suppression | Coalescing test rejects uncertain/foreign binding receipts, suppresses exact proven generation, preserves next unproven generation and unrelated actors. REST author is never event actor. |
| 9 Designer demonstration | **Transferred to 014 by coordinator decision**. Backend schema/preview/overlap/normalized fixture delivered and handoff recorded in both story worklogs. Checked here as transferred scope; no visual demonstration is claimed by 010. |
| 10 fixtures/live where available | Authored official-contract issue/PR REST fixtures plus normalized Designer fixture; discovery and source tests pass. No authorized live resource/account was available, so live validation is explicitly outstanding in to-test/workflows-stories-4.md; no external writes performed. |

Shared files changed additively: workflows/mod.rs, run/mod.rs, run/runtime.rs (startup refresh), run/neutral.rs (limits validation visibility in core), docs/backend/workflows.md and workflow-rework-inventory.md. Other affected consumers: github_poller subscription union, github_auth test initializer, automations/runtime repository resolver visibility. Coordinator requested next-cycle activation: transport does not call refresh. Existing periodic intake refresh reads current enabled bindings even when legacy paths are empty. Existing source/actor/effect owners remain authoritative; missing native provider installation and 014 visual verification retain their owner evidence requirements. Rust requires an operator-selected manual make dev restart or make build after integration; none was performed.

### Critic round 1 F1–F3 contract before fixes

Consumer effect: one bounded open-list request per page supplies full snapshots; only previously known open resources absent from that complete list require a per-number read. A failed missing-resource read warns, retires that known polling candidate and cannot block newer resources. Existing persisted source generations remain available for deduplication/replay. Binding save normalizes GitHub prefixes to the exact lowercase repository root with a trailing slash; malformed or deeper prefixes refuse save.

Authoritative state remains WorkflowStore bindings and RunStore source state; existing account-aware REST API remains the only provider client. Full REST list objects supply the same documented fields as individual GETs. No extra timer/cache/client. Affected consumers: GitHub poll adapter/source normalizer, shared binding save and existing domain tests. Named regressions: github_poll_cycle_uses_list_snapshots_without_per_open_resource_gets; github_poll_cycle_retires_failed_missing_number_and_continues; resource_prefix_selects_exactly_its_repository_in_any_case. F4 baseline is a separately authorized follow-up 2959-3ee2; F5–F8/O1 are accepted by coordinator without changes.

F1–F3 implemented: open-list objects become snapshots directly; known-but-missing reads are isolated with warnings and durable unavailable candidate retirement (no receipt/source-history deletion). Regression count for 100 issues plus 30 PRs is three list requests per cycle. GitHub binding save accepts an optional trailing slash, canonicalizes mixed-case repository names, and rejects deeper/malformed selectors. Source event identity remains unchanged. Targeted run covers changed source/poll tests and mapping/admission consumers only.

Critic-fix task 20261111T124827Z-85293 failed compilation before tests: both the critic-provided prefix regression and my appended regression used the same name. Consolidated into one unconditional test preserving both critic cases, adding explicit canonical saved-prefix and malformed-prefix assertions; no production change. Retry uses the same targeted filter because no tests ran.

### Critic F1–F3 final GREEN

rb task **20261111T125103Z-19332**, nextest **82eb4eca-ce71-4179-9ca7-66f8e0f7eeea**, **10/10 passed**, exit0, stale=no, box_dirty=no. Uploaded tree **040597f41bd3e5a798cf56c5330dca9918ef679a**, source SHA256 **e129ce01f3a0d3142f51c3ef7b9dd3e9695fa33e42d7b7997e51b56304513ba6**. Command: `scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(neutral_github) | test(neutral_trigger_tests) | test(neutral_trigger_coalescing_tests)'` through remote build-slot + tuic-bg. Rust1.99.1 observed from preceding compiler path; nextest/target versions unmeasured. Current Rust/Cargo file hashes match the receipt. Both new request-count/failure-isolation tests, the consolidated critic prefix test, existing source generation/refusal tests and all affected admission/coalescing consumers passed. F1, F2 and F3 resolved; F4 is tracked separately in 2959-3ee2 by coordinator instruction. No external effects, app restart or landing.

### Critic round 2 N1 contract before implementation

Consumer effect: a failed GET of a known-open resource must not remove the polling candidate on network timeout, 5xx, rate limit or other ambiguous error. Warn and preserve it for the next cycle so a closure and later reopen remain observable. Retire only confirmed 404/410 responses or the existing transfer-number-mismatch refusal. Existing account-aware PrApi returns strings; classify only the exact installed http_error_message status prefix and exact local mismatch diagnostic, never arbitrary digits in a server body. RunStore remains authoritative for known candidates. Affected consumers: poll_snapshots error classification, failed-resource fixture and one transient-closure-reopen regression. N2–N4 exceed one-line changes and remain optional/unmodified per coordinator instruction. One targeted rb run of neutral_github and neutral_trigger.

Late coordinator steering during N1 validation identified the installed issue-source prerequisite. Inspection confirms the neutral paths join intake paths, but when both legacy issue intake and old native issue-label bindings are absent, intake_label=None and no issue callback is emitted. This is a silent refusal for enabled issue bindings. Added optional returned binding-state refusal_reason (activation/listing), derived from the exact extracted poller predicate; no intake eligibility/configuration change. Internal admission uses raw stored bindings to avoid repeating source configuration reads per resource. Named regression enabled_issue_binding_exposes_disabled_source_refusal_without_widening_intake proves the visible reason, unchanged configuration and clearing when source becomes available. N1 passing evidence will be retained; this newly requested diagnostic gets one narrow test.

N1 targeted rb **20261111T132928Z-44677**, nextest **740e7557-ac97-453a-b058-d57665b29209**, **13/13 GREEN**, exit0, stale=no, box_dirty=no. Source SHA256 **51ccf248c198e2a4fd2354c15409f4e2582d709d06cca3b4dfef007b0caf6398**, uploaded tree **5949280354efe9112988e829b02f82253f91408d**. Command `scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(neutral_github) | test(neutral_trigger)'` through remote build-slot + tuic-bg. The transient/404/410/mismatch source/test hashes remain unchanged while the later binding-state diagnostic is added. Existing critic commits 63f8801457, fb694a150c and c482ba9639 are preserved; N2 not redone. Narrow binding-file run follows only because coordinator steering added a new source-refusal consumer during the N1 run.

Coordinator decision 04c279be supersedes the interim diagnostic-only behavior: an enabled neutral issue binding is itself the explicit source request. Final source predicate builds a per-repository intake map from enabled legacy settings, old native issue bindings, and enabled neutral Github/issue bindings. Batch grouping includes account plus requested label, so unrequested/sibling repositories never acquire intake merely because another repository has an enabled binding. No sources means no intake subscription or issue fetch. Removed obsolete absent-legacy refusal; optional binding-state refusal_reason now reports only real source configuration errors.

The interim binding-file run rb20261111T133626Z-30052 passed4/4, but its obsolete disabled-source diagnostic regression is replaced by the coordinator's required real-source cases: enabled_neutral_issue_binding_receives_issue_without_legacy_intake uses the existing TLS loopback production GraphQL fixture, verifies actual issue delivery for the enabled repository and no intake query for its sibling; no_enabled_issue_source_does_not_fetch_or_start_intake proves no subscription/poll request when nothing is enabled and no intake selection even in an independent sidebar/PR request. New narrow validation also covers existing overlapping legacy client delivery because per-repository grouping changed. N1 source/test hashes remain unchanged and its13 GREEN cases are reused.

Intake task20261111T134226Z-16837 failed compilation before tests: EventIntake::poll_label consumes self; using it inside a borrowed repository loop moved config.event_intake. Compute the configured label once from one clone before the loop and clone only its String per repository. Same narrow test scope retries; no tests ran in the compile-only attempt.

Source task20261111T134407Z-28681 compiled and passed3/4: both existing legacy/client-overlap cases and the no-enabled-source regression passed. Enabled-neutral-source regression failed because PollHttpFixture::start(true) copied issue labels without pageInfo; the full unlabelled intake path correctly requires that selected GraphQL metadata. Completed the existing simulator's labels connection with hasNextPage=false/endCursor=null. Production source unchanged. Nextestf9f7c401-daaf-4387-8cfc-2d8b54e14552; retry only the failed enabled-neutral-source case. The three unaffected passing cases remain compatible.

Fixture retry task20261111T134822Z-78633 lost SSH during compilation (Broken pipe then connection timeout; artifact listing failed), wrapper exit74. This is transport failure, not code/test evidence. Checked build-slot status (local running=0/waiting=0); remote result/cleanup unverified while rb is unreachable. Retrying exactly once with identical code and failed-case filter under the managed transport rule.

### Pending final source-case validation — external rb outage

The authorized transport retry exited69 before starting any remote command: SSH to rb100.121.52.117 timed out; wrapper reports box unreachable/runner deployment unavailable and no local fallback. Previous attempt20261111T134822Z-78633 exited74 on SSH broken pipe during compilation. One status check plus exactly one retry were used; no further retries or unauthorized native cargo run. Current native fallback authority applies only to exit78/macOS-path-changed and does not cover this outage.

Preserving the complete reviewable N1 + repository-scoped source fix in an own-branch commit. N1 has13/13 GREEN, the new source/no-enabled/legacy-overlap wave has3 compatible passing cases, and the final enabled-neutral-source case awaits validation after completing the test simulator's selected labels pageInfo. No production changes followed that3/4 run; only fixture metadata and documentation changed. This final source-case validation remains PENDING, not claimed GREEN. Integration needs rb restoration or explicit native-test authority from coordinator. No live effects/app restart/push/landing.

### Final source-case GREEN — explicitly authorized native macOS

Coordinator 772229b5 message569127e6 authorized exactly one native run after the rb outage, superseding the pending remote-only requirement for this case. On code commit **bd71873c5e1793d626e1bd5694077bccfeb4f27f**, tree **9f23379c7dd6cbc06bee62af5cbfe216a3425cc5**, the pending `enabled_neutral_issue_binding_receives_issue_without_legacy_intake` regression passed **1/1**, exit0. Native nextest run **20d49b0e-4a87-4d5a-9158-6bc816d36669**; background log `~/Gits/.tmp/wer-010-github/neutral-intake-native.log`. This is **native macOS evidence, not rb**. Command: `PATH="$HOME/Library/Application Support/mbx/bin:$PATH" TMPDIR="$HOME/Gits/.tmp/" scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(enabled_neutral_issue_binding_receives_issue_without_legacy_intake)'`, through tuic-bg, with CARGO_TARGET_DIR unset. `which cargo` confirmed the mbx shim; preceding `mbx doctor` reported0 failures/2 atime warnings. Native cargo1.99.1 (5f94df478), rustc1.99.1 (58cf8f9c3); nextest version unmeasured. No code changed during this run.

Final source-case validation is now GREEN. Reuse N1 rb13/13 and the three compatible source/legacy-consumer passes above; no broader suite or duplicate run. Only this evidence paragraph follows the tested code tree. All source fixes are committed; optional `refusal_reason` is additive returned binding state. Shared files changed minimally/additively include workflows/mod.rs, run/mod.rs and github.rs; no excluded agent/model files changed. No live external writes, restart, push or landing. Loading the Rust changes still requires an operator-selected restart after integration.

### 2026-11-12T00:57:23.000Z - Coordinator closure (integration record reconciliation, or-3 1787b94b): All acceptance criteria checked with evidence cited in this record; closed per ~/Gits/.tmp/workflow-rework/HANDOFF.md (implementation criteria complete, coordinator closure only). Documented limits in this record stay limits, not passes. No new test run.
