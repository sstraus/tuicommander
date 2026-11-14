---
id: 011-2a30
title: "WER: Deliver the Jira connector slice from an explicit operation inventory"
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

# WER: Deliver the Jira connector slice from an explicit operation inventory

## Problem Statement

Workflow engine rework 011-2a30. Scope: inventory existing Jira clients/tools and deliver missing required issue lookup/search, issue creation/update, comment and transition operations needed by the selected issue/development flows; settle exact resource fields in 001-52f6 rather than claim support from the integration name.

Delivery prerequisites: 009-e32f. These native prerequisites also appear in the linked plan and dependency metadata. All work remains pending.

Contract-first feature: record consumer effect, authoritative state, external assumptions and affected consumers before implementation. Reuse existing services/evidence where compatible; no mandatory duplicate full suites or extra critic rounds. Planning creates pending work only and does not authorize implementation or live effects.

## Acceptance Criteria

- [x] Record supported/missing operation matrix with owners and reuse decisions, then make required actions callable through catalog contracts with typed issue/resource references, auth references, pagination/rate-limit and reconciled mutations.
- [x] Map a classified GitHub issue into a configured Jira operation and return typed identifiers/results without a Story root.
- [x] Fixture-based contract checks name wrong-field mapping and duplicate-create-after-lost-response risks.
- [x] Authorized test-resource writes only; absent credentials are an explicit validation gap and unresolved required operations block this slice, not a silently reduced scope.

## Work Log

### Contract before implementation — 2026-11-11

REALITY: effect=an installed Jira catalog activity sends the documented REST request and returns validated issue identifiers or a bounded page; mutations remain owned until their original effect is observed ; state=host-installed site/account/plugin binding, current HttpAuthority grants, Jira server metadata and responses, existing neutral effect ledger ; assumptions=Jira Cloud REST v3, documented API plus recorded 2026-10-27 Cloud responses; no credentials or live writes authorized ; radius=additive Jira catalog/NeutralHost adapter using plugin_http and the existing neutral owner, no scheduler, credential store, Story root, public transport or Designer edits.

Inventory (MDKB first, then scoped source inspection):

| Required operation | Existing owner/support | Missing workflow boundary / reuse decision |
|---|---|---|
| Issue lookup | plugins/jira/provider.js detail, REST v3 GET issue | Rust typed catalog binding; reuse plugin_http/account destination/authentication |
| Issue search | plugins/jira/query.js enhanced search opaque token | Rust bounded page, same documented endpoint; no JS execution in backend |
| Create | plugins/jira/provider.js act(create), fields.js metadata | Typed project/type/summary/ADF and custom fields; original-effect correlation and observation |
| Update | plugins/jira/provider.js act(edit_field), editmeta | Typed editable field set plus read-back reconciliation; reject status/project/type edits |
| Comment | plugins/jira/provider.js act(comment), comments paging | ADF body and original-effect property; bounded comment observation |
| Transitions | plugins/jira/provider.js detail/act(transition) | Available transition ID and required field checks, authoritative target status, read-back |
| Auth/rate limit | plugin_http.rs, plugin_http_account.rs, accounts; plugins/jira/client.js | Reuse account credential reference and destination checks; 429 hint, no retry |

All required Cloud operations remain in scope. Existing JavaScript clients cannot be called by the Rust owner without introducing a second execution platform. Their API semantics and recorded fixtures are reused; the existing Rust plugin_http client remains the actual network/auth implementation. Data Center is outside the selected Cloud v3 contract, not claimed from Cloud fixtures.

Implementation plan: typed installed Jira binding and operation schemas; adapter over NeutralHost and shared HttpAuthority/ConnectorContract; bounded REST requests and effect-only reconciliation; fixture boundary tests and classified GitHub snapshot input mapping; additive docs/manual follow-up; one filtered rb run, commit, result.

Named risks to falsify: GitHub number mapped as Jira issue type/project; stale/foreign issue reference; mutation without current write grant; lost create response repeating POST; 204 parsed as JSON; wrong transition/status ID; truncated or repeated search/comment pagination; 429 repeated write.

External evidence: plugins/jira/fixtures/cloud.json is a scrubbed real recording (2026-10-27, provenance.records). Official Jira Cloud REST API v3 docs inspected 2026-11-11: https://developer.atlassian.com/cloud/jira/platform/rest/v3/api-group-issues/ , https://developer.atlassian.com/cloud/jira/platform/rest/v3/api-group-issue-search/ , https://developer.atlassian.com/cloud/jira/platform/rest/v3/api-group-issue-comments/ . New local scenarios derived from these contracts are labeled constructed, never recorded.

Validation gap: newly installed workflow adapter needs an operator-supplied Jira Cloud account/site, permitted scratch project and issue type, required custom fields, and explicit authorization for create/update/comment/transition plus lost-response read-only reconciliation. Existing live plugin evidence is not new adapter evidence. No service credential will be read or live write sent by this assignment.

### First targeted rb result and correction

Tree 281ec6a00f30e878c0ed4707878a4e9988d2ab5f; rb task 20261111T112848Z-58617; nextest a269f377-a4f1-43be-8410-f638de25f778: 11 executed, 9 passed, 2 failed. No transport failure. Build 4m36s, tests 0.581s, peak RSS 16444840 KiB. Two unused imports also removed.

The no-Story runtime case caught missing Jira capability registration: the handler existed but HostRegistry.capabilities lacked jira.network. Registered the installed admission adapter under that requirement; current grants still come from HttpAuthority. The search test had a wrong oracle: both recorded pages have isLast=false and distinct continuation tokens. Corrected the test to require the second recorded token, preserving the recording and production pagination semantics. No assertion was weakened to ignore pagination.

The corrected final wave reruns neutral_jira_tests (its four tests): the two failed cases plus the two cases in the changed test file. Seven passing tests in unchanged mutation/recovery test files remain compatible: the only production behavior change is registry capability publication; those tests dispatch/reconcile directly and do not use that registry. No duplicate HTTP foundation run: 009's client/admission semantics are unchanged; the shared reconciliation enum addition is exercised through Jira binding construction and observation.

### Final validation and criterion evidence

- AC1: `workflows/jira/{catalog,request,mod}.rs` defines installed operation descriptors, closed typed issue/resource/field/request/result data and credential references. `run/neutral_jira*.rs` executes every required lookup/search/create/update/comment/transitions/transition path through plugin_http and the existing neutral owner. Current grants, metadata, pagination, 429 and original-effect observation are covered by the targeted tests. The matrix above identifies existing owners and reuse. Shared `connector.rs` adds only ReadOnlyServiceObservation; `workflows/mod.rs` and `run/mod.rs` add modules. No parallel execution/credential owner added.
- AC2: `classified_github_issue_creates_configured_jira_issue_without_story_root` passes through the actual neutral run actor, catalog binding and HTTP boundary, returning canonical account/issue ID 10004. It asserts project 10001/type 10006 rather than source issue number 73, title/source mapping and correlation. Uses existing typed GitHubIssue only; no 010 dependency or Story root.
- AC3: recorded Cloud fixtures plus explicitly constructed faults protect wrong-field mapping, duplicate-create-after-lost-response, required custom fields, stale transitions, authoritative update/status read-back, repeated opaque cursor, foreign binding/grant revocation, rate limits and unique original-effect comment/create recovery. No recording was edited. Local transport replay does not claim vendor acceptance.
- AC4: no live external write, credential read, app launch/restart, push or merge. Every required Cloud operation is implemented. The exact live-account/write gap remains the contract's assumption ledger and `to-test/workflows-stories-4.md`; it is not a mock/live success claim. It needs an operator-authorized Jira Cloud scratch account/site/project/type/custom metadata and explicit mutation/recovery authorization. Data Center is not claimed.

Final rb task **20261111T113603Z-65913**, nextest **33481c6f-996f-4967-b8e1-599cfbd77a95**: **4/4 passed**, no compiler warnings, tests 1.760s, build 4m49s, admission 247s, peak RSS 16593392 KiB. Command:

`scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(workflows::run::neutral_jira_tests)'`

Launched via tuic-bg.sh and build-slot.sh --remote. Tested Git tree **a7a249211de13352dfde69dce4333a7d6c7c4e60**; rb source tree SHA-256 **79c30af92ea5be6a3ad64169ddd7a36810f2c198217b498adc415c067b69b56a**. Remote command exit **0**, wrapper exit **75** because rustfmt reordered cfg/mod declarations in `run/mod.rs` after synchronization. The only difference from that tested Git tree was that declaration reorder; all Jira production/test bytes are identical. Direct diff verified cfg associations and the whole-file line multiset is identical. Evidence: `~/Gits/.tmp/results/wer-011-jira-rb-compatibility.json`. No duplicate run for formatting. The previous task's **7/7 compatible passes** in unchanged mutation/recovery test files are reused, giving coverage of all **11** new tests. Local rustc/nextest versions printed by the wrapper are not rb versions: **rb toolchain unmeasured**, landing records it once per coordinator instruction.

Logs: `~/Gits/.tmp/results/wer-011-jira-targeted.log` and `wer-011-jira-final.log`. Evidence manifest: `wer-011-jira-evidence-manifest.json`. The final test wave is green for the tested code; supervisor staleness is explicitly retained above. No full suite, JS/TS checks (no JS/TS change), clippy, mutation, critic, or live-service acceptance run. Instruction-link check passes (two canonical pairs), git diff --check passes. Formatting checked with the required fmt-changed command; final commit confirmation follows in the result file. Story lifecycle remains with the coordinator/landing; criterion checks are direct in this workflow-stories file as instructed.

Risk: medium/high — external mutation protocol and shared effect/resource state. Missing live credentials and new adapter authentication/marker persistence/search visibility/tenant permissions remain material limits. Rust code needs Boss's manual make dev restart or make build after composition; no automatic backend reload or daily-app restart was attempted.

### Authorized critic fix round — contract before edits

COORDINATOR2 unfroze this branch and directed OE1/F1/F2/F3 fixes; N1/N2/N3 remain accepted. Critic commit 60694433b6 records two RED regressions on rb task 20261111T115541Z-34052 (both failed): spaced GitHub labels and integer custom-field updates read back as doubles. Reuse that RED evidence; no duplicate run.

Consumer effect: ordinary classified GitHub issues create Jira-valid deterministic labels; landed numeric/multiselect field updates settle from authoritative reads; a lost comment receipt is found through the documented comment-properties endpoint. Authoritative state remains Jira response data, installed binding/current grants and original effect identity. No new authority, state store or executor.

External contract verified 2026-11-11 against official Jira Cloud REST v3 OpenAPI https://dac-static.atlassian.com/cloud/jira/platform/swagger-v3.v3.json?_v=1.8516.155 and https://developer.atlassian.com/cloud/jira/platform/rest/v3/api-group-issue-comments/ : GET issue comments expands renderedBody only; POST /rest/api/3/comment/list accepts integer IDs and expand=properties. The corrected constructed fixture must expose properties only on that POST read endpoint and assert no mutation endpoint is used during recovery. Existing real GET page recordings stay unchanged.

Changes: replace each whitespace character in mapped GitHub labels with '-'; numeric comparison accepts equivalent integer/double representations without collapsing distinct large integers; match expanded option objects by supplied id/value with one-to-one option array comparison while preserving ordered document arrays. Collect comment IDs from each bounded GET page and read their properties via POST comment/list; missing/duplicate/foreign records stay uncertain. Remove requested host-URL credential/query/fragment checks and request/value byte caps; retain existing installed account/destination owner checks and documented page bounds.

Affected consumers: classified issue create, update/transition read-back and reconciliation, comment reconciliation, constructor/input validation. One final targeted rb wave covers changed test files and the critic regressions; unchanged compatible recovery evidence is retained. No live endpoint/account supplied or used.

Fix-round implementation: F1 maps every source-label whitespace character to '-' and strengthens the critic's deterministic expected label list. F2 compares numeric representations without losing integer precision and recursively observes expanded objects/options; option arrays match one-to-one, other arrays retain order. Its added boundary table checks option expansion/reordering, duplicate matching, ordered text content, unequal numeric values and integers beyond exact f64 precision. F3 moves the corrected comment recovery fixture to neutral_jira_comment_tests.rs, using recorded GET pages without properties and documented constructed POST comment/list expansions; asserts integer ids and that every POST is read-only comment/list. Duplicate effect properties remain uncertain. OE1 deletes the four URL component checks and both requested byte caps. Existing auth destination and response/page contracts remain.

Final-round validation scope: neutral_jira_tests (including critic label RED), neutral_jira_mutation_tests (including critic double RED), neutral_jira_comment_tests (corrected API contract), neutral_jira_observation_tests (expanded options and adjacent wrong-result checks). These changed files run once together on rb. The two unchanged neutral_jira_recovery_tests remain compatible with the previous recorded passes. No native/live-service run, critic spawn or full suite is authorized/performed by this peer.

### Fix-round final GREEN

rb task **20261111T121029Z-47848**, nextest **448013ba-d224-4349-b949-ed938fd43cea**, tested Git tree **1dc5f19056748edc62a3e10345248080d78bb852**: **12/12 passed**, command/wrapper exit **0**, no compiler warnings, no snapshot drift, box dirty=no. Build 2m15s, tests 1.047s, peak RSS 12342896 KiB. rb toolchain remains unmeasured per COORDINATOR2.

Command (through tuic-bg.sh and build-slot.sh --remote):

`scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib --no-fail-fast -E 'test(workflows::run::neutral_jira_tests) | test(workflows::run::neutral_jira_mutation_tests) | test(workflows::run::neutral_jira_comment_tests) | test(workflows::run::neutral_jira_observation_tests)'`

Both critic RED tests now pass. The corrected comment-property contract and the adjacent option/numeric wrong-result cases also pass. The two unchanged recovery tests retain compatible earlier GREEN evidence. Formatting and git diff --check pass. All four acceptance criteria remain checked with the live-service gaps retained; OE1/F1/F2/F3 are implemented and verified. No live credential/write, app restart, push, merge, full suite or second critic run by this peer. N1/N2/N3 remain accepted by COORDINATOR2.

TEST-CHANGE: neutral_jira_tests::classified_github_issue_with_spaced_label_maps_to_valid_jira_labels — strengthened the expected result to the exact deterministic label mapping, rather than only absence of whitespace. TEST-CHANGE: comment_recovery_observes_original_property_across_bounded_pages — moved to neutral_jira_comment_tests.rs and corrected its old synthetic GET properties oracle to documented POST comment/list; preserves pagination coverage and adds duplicate-marker uncertainty.

TOOL-ANOMALY: HUD daemon emitted `unknown type: pre_tool_check` on the successful handoff-cli write; command exited zero and saved the handoff. No implementation/test failure or workaround inferred from that warning.

### 2026-11-12T00:57:23.000Z - Coordinator closure (integration record reconciliation, or-3 1787b94b): All acceptance criteria checked with evidence cited in this record; closed per ~/Gits/.tmp/workflow-rework/HANDOFF.md (implementation criteria complete, coordinator closure only). Documented limits in this record stay limits, not passes. No new test run.
