---
id: 013-66b3
title: "WER: Deliver the Telegram connector slice from an explicit operation inventory"
status: complete
priority: P2
type: feature
created: "2026-11-10T08:29:18.829Z"
updated: "2026-11-12T00:57:23.000Z"
dependencies: ["009-e32f"]
plan: plans/workflow-engine-rework.md
plan_step: Step 4
depends_on: ["workflow-stories/009-e32f-complete-P2-wer-deliver-typed-http-operations-and-the-shared-c.md"]
---

# WER: Deliver the Telegram connector slice from an explicit operation inventory

## Problem Statement

Workflow engine rework 013-66b3. Scope: reuse the current Telegram integration for workflow notifications and operator interaction, inventorying missing send/update/reply/callback operations required by those flows.

Delivery prerequisites: 009-e32f. These native prerequisites also appear in the linked plan and dependency metadata. All work remains pending.

Contract-first feature: record consumer effect, authoritative state, external assumptions and affected consumers before implementation. Reuse existing services/evidence where compatible; no mandatory duplicate full suites or extra critic rounds. Planning creates pending work only and does not authorize implementation or live effects.

## Acceptance Criteria

- [x] Document existing support versus delivered missing operations.
- [x] Typed notification/message/action references and reply correlation use shared activity/wait/effect contracts.
- [x] Callback decisions bind exact question revision, arguments and subject; duplicates/stale answers cannot resolve a newer wait, and restart does not need a live agent session.
- [x] Secrets remain references and uncertain sends reconcile or block instead of resending blindly.
- [x] Record connector fixtures and an authorized chat test where available; missing credentials are reported, not replaced with mock success.

## Work Log

### REALITY contract — 2026-11-11

Consumer effect: installed Telegram activities send one bounded notification, update one retained message, or reply to one exact message. Outputs retain typed bot/chat/message and notification-effect references. Actions bind the complete target EffectKey (arguments and subject digest) and question revision; phone input reaches the existing EffectCommand::Answer transaction before volatile peer routing.
Authoritative state: the existing neutral SQLite event ledger owns invocation, question, answer and send receipt. The existing Telegram worker owns current configuration, chat authorization, credential resolution, throttling and BotApi calls. No new poller, credential store, executor or callback database. Restart reconstructs actions from retained effects, with no agent registration.
External assumptions: Telegram Bot API 10.4 (2026-11-09), https://core.telegram.org/bots/api, documented sendMessage/editMessageText/ReplyParameters/CallbackQuery/answerCallbackQuery contracts. Contract fixtures are explicitly synthetic schema examples, not recorded live success. No account, token or authorized destination was supplied: live send/edit/reply/callback and restart checks are a validation gap requiring a configured bot and authorized private chat.
Affected consumers: neutral effect approval/revision replay and resource ownership; Telegram serialized commands, inbound callbacks, native standby, ordinary peer buttons. Shared host installation stays with 008, public transport with 015. Coordinator confirmed this boundary in mail 7ef057a3-fb30-46b5-aaea-6e2116e52f93.

Operation matrix before implementation:

| Operation | Existing | Delivery |
|---|---|---|
| sendMessage | BotApi + Outbound, peer and automation notifications | typed installed send activity and durable receipt |
| editMessageText | BotApi supports host-owned methods; no workflow operation | typed update of exact bot/chat/message |
| reply_parameters | peer routing uses volatile request identity | typed reply to exact message |
| inline callback + answerCallbackQuery | current-message handles and live peer mail | durable workflow action, exact effect/question revision; existing ack client |
| credential/config/polling/throttle | existing Telegram services | reuse unchanged ownership |
| uncertain send | automation preclaim; no workflow adapter | neutral effect uncertainty, no automatic resend |

Test contract: wire projection catches wrong method/reply target; owner/restart callback tests catch stale/duplicate/wrong-subject approval; installed-host admission tests catch descriptor/config substitution; uncertainty test catches resend after a lost receipt. Existing tests do not cover this new installed adapter. Tests use production boundaries and reuse the existing ledger fixture; no test-only production export. No critic or full suite per assignment.

Implementation complete before targeted execution: installed descriptor metadata, typed references and request projection, serialized worker command, callback replay and shared Answer transaction. Callback tokens locate the notification run; action targets can belong to another run, so a notification composes with a previously prepared wait. Module tests cover current admission, exact send/edit/reply wire data, ingress before peer routing, cross-run receipt replay after owner reopen, stale/changed/duplicate answers and receipt-loss blocking.

Planned targeted filter: new neutral_telegram tests, Telegram workflow module, existing peer button callback and standby command consumers, plus the existing exact approval/revision regression. Named risks: new adapter behavior and callback routing could bypass revision fencing or break ordinary buttons/disabled command replies. No UI/TS files changed; no JS suite is relevant.

### Targeted evidence — 2026-11-11

GREEN: rb task `20261111T113234Z-10183`, Nextest run `58e997e3-5cdf-4dab-8b72-3f10f5dc136c`, 10/10 passed. Command: `/Users/stefano.straus/Gits/personal/orchestrator/tools/build/build-slot.sh --remote scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(neutral_telegram) | test(telegram::workflow::) | test(buttons_route_one_opaque_choice) | test(standby_rejects_commands) | test(exact_approval_revision_has_one_committed_winner)'`, launched by tuic-bg. Default features/profile; Linux rb, CARGO_BUILD_JOBS=10; repository temp setup passed. No native test, full suite, clippy or critic run.

Source identity: base `a2d7a3127dd2bb6f20789805d0ac2ec1dd50d16b`; rb dirty-source SHA-256 `5d1bf9f50bec51b80f58618b5d0427546e43511f812e2c2b8fb0e4ad5ee36b8d`. All 280 workflow/Telegram/Cargo/support manifest hashes match after the run; only evidence/documentation changes follow. Detailed per-file evidence: `~/Gits/.tmp/wer-013-telegram/evidence.json`; log: `~/Gits/.tmp/wer-013-telegram/targeted-1.log`. Local tooling: rustc 1.99.1 (58cf8f9c3, 2026-10-13), cargo-nextest 0.9.140 (a9fef2964, 2026-07-05). Remote tool versions are unmeasured in rb metadata; supplemental direct SSH failed `Permission denied (publickey)`, with no alternate transport. This limits toolchain provenance, not the recorded test result.

Criterion evidence:
1. Existing/delivered support matrix: docs/backend/workflow-telegram-connectors.md and this contract; inventory updated additively.
2. Strict MessageReference/NotificationReference/ActionReference and installed descriptor metadata; send/update/reply reach the real serialized worker against documented schema fixtures. Current chat and pinned bot metadata are checked by installed_telegram_metadata_is_pinned_and_current_chat_is_checked.
3. retained_telegram_receipt_resolves_exact_wait_after_restart_once reopens the real SQLite owner with no live peer; wrong bot/chat/message and duplicate answer leave the wait unchanged. telegram_old_question_and_changed_target_cannot_select_new_wait covers stale question and argument/subject/answer identity. Ingress test proves workflow callbacks reach the owner before volatile peer routing. Existing exact-approval and ordinary-button consumer tests also pass.
4. Only credential references enter connector metadata. BotApi retains credential resolution. telegram_missing_send_receipt_blocks_callback_and_reconcile_never_resends observes blocked consent and retained uncertainty without a network replay; existing resource ledger owns the claim.
5. Official Bot API 10.4 schema fixture and provenance recorded. LIVE GAP: no bot credentials, authorized destination or live-write authority supplied; send/edit/reply, real callback/ack and restart in Telegram require that exact setup and Boss authorization. Synthetic fixtures prove only the local documented contract. Post-rebuild item added to to-test/voice-telegram-5.md.

Main-store `wiz-run stories-cli.js check-criterion 013-66b3 1` reports story not found; supplied workflow-stories namespace is the acceptance record. Story status remains coordinator-owned. Shared edits are additive in workflows/mod.rs, run/mod.rs, run/runtime.rs, docs/backend/workflows.md and workflow-rework-inventory.md; Telegram command/ingress edits preserve existing worker and callbacks. No public IPC/HTTP surface added. Rust changes require a manual make dev restart (or make build) after installation; no app restart performed. Instruction-link validation: two pairs valid.

### Critic fix contract — 2026-11-11

F1 verified: a workflow callback error escaped Runtime::update before Inbound persisted its offset, so one undecidable tap could starve all later messages. Consumer effect: every workflow callback is consumed as accepted/rejected with best-effort acknowledgment; inability to decide never retries phone consent or blocks the inbound cursor. Approval state remains solely in the shared Answer transaction. External acknowledgment is still best effort and no live write is authorized. Affected consumers: workflow callback ingress, diagnostics, ordinary peer callbacks and exact approval revision handling.
RED reused from critic commit 8c78c83ee19c9bff6af45a530a1661d5a0457434: rb task 20261111T115034Z-63058, undecidable_workflow_callback_is_consumed_so_polling_advances failed at the expected Runtime::update assertion on production 720977b490. No duplicate RED run. O1 removes redundant candidate-label ambiguity. O2 derives the operation from retained pinned configuration; current dispatch admission, worker alias and Outbound chat checks remain authoritative. N1-N4 accepted by coordinator, with no added mechanism.

Critic fix GREEN: rb `20261111T115954Z-94634`, Nextest `2c7f700f-521c-4a73-b9e4-9aeebf07dd2c`, 11/11 passed. Same original 013 filter plus `test(undecidable_workflow_callback_is_consumed_so_polling_advances)`, through build-slot --remote and scripts/with-test-tmp.sh. Default features/profile, CARGO_BUILD_JOBS=16, rb toolchain unmeasured. Source SHA-256 `c9380656edce417bd654effc9066604c4d06ae2a386b67b45ceef0a3c3ee7b41`; all 280 relevant source/config hashes match. Detailed command/hashes in `~/Gits/.tmp/wer-013-telegram/fix-r1-evidence.json`; log `~/Gits/.tmp/wer-013-telegram/fix-r1.log`. Critic RED reused; no duplicate or broad runs.

F1: workflow_callback is now infallible, maps undecidable actions to rejection, attempts acknowledgment when configuration permits, and lets Runtime::update record the button diagnostic and consume the update. The critic regression also covers acknowledgment transport failure, invalid chat policy and disabled configuration; no callback fault poisons the polling cursor. Existing ordinary buttons and approval-revision tests remain green. O1 removed the label-sensitive ambiguity error. O2 removed repeated prepare/check disk reads from invoke_effect; operation derives from retained pinned config while the dispatch/worker/Outbound authority checks remain in place. fmt-changed check and git diff --check pass. Live-account and host-installation gaps are unchanged.

### 2026-11-12T00:57:23.000Z - Coordinator closure (integration record reconciliation, or-3 1787b94b): All acceptance criteria checked with evidence cited in this record; closed per ~/Gits/.tmp/workflow-rework/HANDOFF.md (implementation criteria complete, coordinator closure only). Documented limits in this record stay limits, not passes. No new test run.
