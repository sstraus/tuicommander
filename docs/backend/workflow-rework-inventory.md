# Workflow rework preservation inventory

009-e32f adds the internal typed HTTP adapter over the existing plugin client,
neutral admission and effect ledger. Its local contract tests cover bounded
pagination into collections; they do not establish external-service parity.
See [connector contracts and evidence limits](workflow-http-connectors.md).

This inventory is not a parity or operational-readiness claim. Source inspection
is not runtime evidence. A blocked mapping prevents migration of the affected
capability until the named owner verifies it. The current runtime remains the
owner; this document does not authorize a cutover.

011-2a30 adds the installed Jira Cloud v3 catalog/host slice over the same account HTTP client and neutral ledger: lookup/search/create/update/comment/transitions, typed references, bounded pages and original-effect observation. The operation matrix and external evidence gaps are in [the Jira contract](workflows.md#installed-jira-cloud-activities-011-2a30). No live adapter write or migration parity is claimed.

## Recorded definitions and events

The actual v2 RunStore event serializations are
`src-tauri/src/fixtures/workflow_contracts/v2-active-pin.json` and
`src-tauri/src/fixtures/workflow_contracts/v2-exclusive-join.json`. Their
adjacent README records the production revision, capture command and rb
provenance. The graph-start event includes the serialized published definition;
these are actual serialized outputs of seeded test runs, not production-user
runs. They are not invented examples or evidence of live connector behavior.
The original files remain the fixture source; do not replay their scratch paths
as filesystem inputs. The graph replay consumers are `graph_tests.rs`.

Capture-file SHA-256 (the files are unchanged by this inventory):

- `v2-active-pin.json`: `320bfaa23c02ffef7b4e385e7af0ad9ae4de90c674ddcd5b69330f98797ba890`
- `v2-exclusive-join.json`: `b27a31c3da8c18f950c0bbc197b29d5620c11439b504c4356160d61875c99399`
- `workflow-baseline/capabilities.json`: `1995d3e90f0d5f65aede33a68ef578214e73e0f1281be237b6246497bc896e0b`

## Baseline capability disposition

Each ID below refers to its full source and historical preservation row in
`workflow-baseline/capabilities.json`. Its previous `required_not_operationally_verified`
status is retained. Candidate destinations derive from the earlier row’s WF responsibility and the
approved rework story scopes, not from an implemented replacement. They name
the contract or adapter owner, not a verified one-to-one operation. Owner
017-70f7 must resolve each exact operation and parity evidence before cutover. No row is approved for deletion by this inventory.

The source preservation intent below is retained verbatim from the existing
baseline. Its K/R/D shorthand belongs to the earlier inventory; it is not
an instruction to remove any current service.

| Source ID | Candidate rework stories | Prior preservation intent | Migration owner |
|---|---|---|---|
| A01 | 006-6885, 008-22c4; exact adapter and parity BLOCKED | K Author + daemon launch/profile/workspace contract | 017-70f7 |
| A02 | 006-6885, 008-22c4; exact adapter and parity BLOCKED | K managed delivery effect + peer identity; payload receipt not idle inference | 017-70f7 |
| A03 | 005-f907, 007-7ce3, 014-5d73; exact adapter and parity BLOCKED | K Run command async effect + cursor notification | 017-70f7 |
| A04 | 006-6885, 008-22c4; exact adapter and parity BLOCKED | K lifecycle transaction + canonical workspace identity | 017-70f7 |
| A05 | 006-6885, 008-22c4; exact adapter and parity BLOCKED | K workspace baseline and named writer ownership | 017-70f7 |
| A06 | 006-6885, 008-22c4; exact adapter and parity BLOCKED | K close transaction and owned-process gate | 017-70f7 |
| A07 | 006-6885, 008-22c4; exact adapter and parity BLOCKED | K continuation intent and separate delivered receipt | 017-70f7 |
| A08 | 006-6885, 008-22c4; exact adapter and parity BLOCKED | K maintenance close policy; R shell idle sweeper as scheduler | 017-70f7 |
| A09 | 005-f907, 014-5d73; exact adapter and parity BLOCKED | K run incident/attempt/session projection; unknown inventory preserved | 017-70f7 |
| A10 | 006-6885, 008-22c4, 005-f907, 014-5d73; exact adapter and parity BLOCKED | K attempt conversation/stop references + resume inspector | 017-70f7 |
| A11 | 003-e678, 004-2809, 008-22c4, 016-358f; exact adapter and parity BLOCKED | K bounded Author Fork/all-Join consultation under global routing | 017-70f7 |
| A12 | 005-f907, 014-5d73; exact adapter and parity BLOCKED | K usage adapter retains JSONL audit, generic coverage added | 017-70f7 |
| E01 | 004-2809, 016-358f; exact adapter and parity BLOCKED | K durable intake/disposition and cursors; R coordinator pump | 017-70f7 |
| E02 | 004-2809, 016-358f; exact adapter and parity BLOCKED | K same durable event subscription; R second shell watcher implementation | 017-70f7 |
| E03 | 004-2809, 016-358f, 006-6885, 008-22c4; exact adapter and parity BLOCKED | K portfolio projection + timer notifications; R refresher loop | 017-70f7 |
| E04 | 004-2809, 016-358f, 005-f907, 014-5d73; exact adapter and parity BLOCKED | K portfolio open-work index and bounded/full views | 017-70f7 |
| E05 | 004-2809, 016-358f; exact adapter and parity BLOCKED | K story-store queries + bounded legacy inventory adapter | 017-70f7 |
| E06 | 004-2809, 016-358f, 008-22c4; exact adapter and parity BLOCKED | K canonical-key portfolio inventory/holds | 017-70f7 |
| E07 | 006-6885, 008-22c4, 004-2809, 016-358f; exact adapter and parity BLOCKED | K durable report lifecycle/successor transaction | 017-70f7 |
| E08 | 004-2809, 016-358f; exact adapter and parity BLOCKED | K durable intake → story/run/dismissal relation | 017-70f7 |
| E09 | 004-2809, 016-358f, 017-70f7, 019-683f; exact adapter and parity BLOCKED | K cursor replay + legacy result import; R filesystem scan authority | 017-70f7 |
| E10 | 006-6885, 008-22c4, 005-f907, 014-5d73; exact adapter and parity BLOCKED | K incident policy with episode/generation key | 017-70f7 |
| E11 | 005-f907, 007-7ce3, 004-2809, 016-358f; exact adapter and parity BLOCKED | K resource/dependency admission + ownerless alarm | 017-70f7 |
| E12 | 008-22c4, 018-bb68, 006-6885; exact adapter and parity BLOCKED | K timer maintenance Calls and resource locks | 017-70f7 |
| E13 | 006-6885, 008-22c4; exact adapter and parity BLOCKED | K optional scoped Author template with escalation receipt | 017-70f7 |
| E14 | 008-22c4, 004-2809, 016-358f; exact adapter and parity BLOCKED | K portfolio integration-queue alarm | 017-70f7 |
| E15 | 004-2809, 016-358f; exact adapter and parity BLOCKED | K owner/hold classification Judge + reason evidence | 017-70f7 |
| S01 | 006-6885, 008-22c4, 004-2809, 016-358f; exact adapter and parity BLOCKED | K claim/branch/workspace admission invariant | 017-70f7 |
| S02 | 008-22c4; exact adapter and parity BLOCKED | K Timer → bounded classifier Author → Judge | 017-70f7 |
| S03 | 004-2809, 016-358f, 005-f907, 014-5d73; exact adapter and parity BLOCKED | K intake-to-nondoc-artifact lineage projection | 017-70f7 |
| S04 | 008-22c4, 016-358f; exact adapter and parity BLOCKED | K evidence Gate and story transition adapter | 017-70f7 |
| S05 | 008-22c4, 018-bb68, 003-e678, 004-2809, 016-358f; exact adapter and parity BLOCKED | K sequential delivery Call mode and pinned batch artifact | 017-70f7 |
| S06 | 004-2809, 016-358f, 008-22c4; exact adapter and parity BLOCKED | K portfolio release scope policy, never automatic publish promise | 017-70f7 |
| S07 | 010-978f; exact adapter and parity BLOCKED | K PR review subflow and GitHub effect adapter | 017-70f7 |
| Q01 | 006-6885, 008-22c4, 016-358f; exact adapter and parity BLOCKED | K Author template from actual pinned story/artifact + review counter | 017-70f7 |
| Q02 | 008-22c4, 016-358f; exact adapter and parity BLOCKED | K ordered deterministic triage policy | 017-70f7 |
| Q03 | 002-eeb6, 004-2809, 008-22c4, 016-358f; exact adapter and parity BLOCKED | R obsolete alternate text, active policy/template retained; approval item R5 | 017-70f7 |
| Q04 | 005-f907, 007-7ce3, 008-22c4, 018-bb68; exact adapter and parity BLOCKED | K proof batch Run commands on rb | 017-70f7 |
| Q05 | 008-22c4, 016-358f, 018-bb68; exact adapter and parity BLOCKED | K typed check history; legacy rows imported as historical only | 017-70f7 |
| Q06 | 008-22c4, 018-bb68; exact adapter and parity BLOCKED | K proof obligation store/cohort scheduler | 017-70f7 |
| Q07 | 008-22c4, 018-bb68; exact adapter and parity BLOCKED | K registry port + durable artifact references/backup | 017-70f7 |
| Q08 | 008-22c4, 018-bb68; exact adapter and parity BLOCKED | K deterministic mutation reuse Gate | 017-70f7 |
| Q09 | 005-f907, 007-7ce3, 008-22c4, 018-bb68; exact adapter and parity BLOCKED | K read-only rb measurement Tool, installed state verified separately | 017-70f7 |
| Q10 | 008-22c4, 018-bb68; exact adapter and parity BLOCKED | K requested-universe Tool and identity schema | 017-70f7 |
| Q11 | 008-22c4, 018-bb68, 006-6885; exact adapter and parity BLOCKED | K opt-in reduction subflow, no production writes, AWS-only mutation proof | 017-70f7 |
| Q12 | 008-22c4, 018-bb68; exact adapter and parity BLOCKED | K retained evidence comparison Tool, never auto-delete tests | 017-70f7 |
| L01 | 008-22c4; exact adapter and parity BLOCKED | K Integrate canonical resource + recovery | 017-70f7 |
| L02 | 005-f907, 007-7ce3, 008-22c4, 018-bb68; exact adapter and parity BLOCKED | K Integrate admission/merge/refresh/obligation phases | 017-70f7 |
| L03 | 008-22c4, 018-bb68; exact adapter and parity BLOCKED | K local JS cohort Run command and exact membership discharge | 017-70f7 |
| L04 | 008-22c4; exact adapter and parity BLOCKED | K lifecycle cleanup Gate + TUIC worktree adapter | 017-70f7 |
| L05 | 006-6885, 008-22c4; exact adapter and parity BLOCKED | K canonical repository/workspace adapter | 017-70f7 |
| L06 | 008-22c4; exact adapter and parity BLOCKED | K Timer maintenance cleanup Call with live-owner protection | 017-70f7 |
| L07 | 004-2809, 016-358f, 008-22c4; exact adapter and parity BLOCKED | K portfolio inventory and hold evidence | 017-70f7 |
| L08 | 008-22c4, 006-6885; exact adapter and parity BLOCKED | K maintenance Author/Judge → normal delivery; Ask for discard, WORK report-only | 017-70f7 |
| L09 | 008-22c4, 006-6885; exact adapter and parity BLOCKED | K typed archive Tool + explicit deletion decision, no guard bypass | 017-70f7 |
| L10 | 008-22c4; exact adapter and parity BLOCKED | K narrow merge-union Tool; malformed/code conflicts refuse | 017-70f7 |
| L11 | 006-6885, 008-22c4; exact adapter and parity BLOCKED | K typed submodule Tool with baseline exact-entry attribution | 017-70f7 |
| L12 | 010-978f, 008-22c4, 016-358f; exact adapter and parity BLOCKED | K CI observation Tool/Timer with success/failure/timeout | 017-70f7 |
| L13 | 008-22c4, 016-358f; exact adapter and parity BLOCKED | K CI evidence Gate and exact tag-only publication policy | 017-70f7 |
| L14 | 008-22c4, 016-358f; exact adapter and parity BLOCKED | K pinned repository release subflow + separate approval | 017-70f7 |
| B01 | 005-f907, 007-7ce3; exact adapter and parity BLOCKED | K resource adapter initially reusing build-slot/rb | 017-70f7 |
| B02 | 005-f907, 007-7ce3; exact adapter and parity BLOCKED | K durable admission freeze policy, existing bridge retained | 017-70f7 |
| B03 | 005-f907, 007-7ce3, 008-22c4; exact adapter and parity BLOCKED | K positioned format Tool, no unrelated module/vendor formatting | 017-70f7 |
| B04 | 005-f907, 007-7ce3, 006-6885, 008-22c4; exact adapter and parity BLOCKED | K explicit maintenance recovery Tool, stop owning builds first | 017-70f7 |
| B05 | 005-f907, 007-7ce3, 008-22c4; exact adapter and parity BLOCKED | K live-check Tool with readiness and owned launcher receipts | 017-70f7 |
| B06 | 006-6885, 008-22c4; exact adapter and parity BLOCKED | K authorized browser access-check Tool and retained evidence | 017-70f7 |
| B07 | 005-f907, 007-7ce3, 008-22c4; exact adapter and parity BLOCKED | K native live-check policy/resource keys/scoped screenshots | 017-70f7 |
| R01 | 005-f907, 007-7ce3; exact adapter and parity BLOCKED | K rb execution port preserves manifests and results | 017-70f7 |
| R02 | 005-f907, 007-7ce3; exact adapter and parity BLOCKED | K rb admission/runner adapter; verify installed version | 017-70f7 |
| R03 | 005-f907, 007-7ce3, 008-22c4, 018-bb68; exact adapter and parity BLOCKED | K remote job handle and reconciliation port | 017-70f7 |
| R04 | 005-f907, 007-7ce3; exact adapter and parity BLOCKED | K transport/lease/fingerprint port, no secret receipts | 017-70f7 |
| R05 | 005-f907, 007-7ce3, 006-6885, 008-22c4; exact adapter and parity BLOCKED | K explicit maintenance GC Tool preserving live reservations | 017-70f7 |
| R06 | 006-6885, 008-22c4; exact adapter and parity BLOCKED | K explicit provisioning Tool; optional experiment remains opt-in | 017-70f7 |
| R07 | 006-6885, 008-22c4, 005-f907, 007-7ce3; exact adapter and parity BLOCKED | K optional authorized executor template; missing token visible blocked | 017-70f7 |
| R08 | 006-6885, 008-22c4; exact adapter and parity BLOCKED | K authorized VM compatibility maintenance subflow; credentials never printed | 017-70f7 |
| C01 | 005-f907, 014-5d73; exact adapter and parity BLOCKED | K durable inbox/digest Ask policy | 017-70f7 |
| C02 | 005-f907, 014-5d73, 008-22c4; exact adapter and parity BLOCKED | K unread report/answer acknowledgment projection | 017-70f7 |
| C03 | 008-22c4, 016-358f; exact adapter and parity BLOCKED | K release countdown projection; D cross-release total, D2 below | 017-70f7 |
| C04 | 008-22c4, 016-358f; exact adapter and parity BLOCKED | K release readiness Gate + deduplicated Notify, no publish | 017-70f7 |
| C05 | 008-22c4; exact adapter and parity BLOCKED | K Timer/source-change → scoped report Author | 017-70f7 |
| C06 | 008-22c4; exact adapter and parity BLOCKED | K upkeep inventory, unread-protection Gate | 017-70f7 |
| C07 | 005-f907, 014-5d73, 008-22c4; exact adapter and parity BLOCKED | K retained embedding Tool until native assets proven equivalent; no assumed retirement | 017-70f7 |
| C08 | 005-f907, 014-5d73, 008-22c4; exact adapter and parity BLOCKED | K communication adapter and pinned presentation policy, no Mission Control iframe | 017-70f7 |
| C09 | 008-22c4, 016-358f; exact adapter and parity BLOCKED | K investigation/report Author output contract + evidence links | 017-70f7 |
| H01 | 005-f907, 014-5d73, 006-6885, 008-22c4; exact adapter and parity BLOCKED | K daemon resume snapshot and cursor replay; R coordinator-context reconstruction | 017-70f7 |
| H02 | 008-22c4; exact adapter and parity BLOCKED | K health observation Tool/Timer, report actual install/last run | 017-70f7 |
| H03 | 008-22c4; exact adapter and parity BLOCKED | K diagnosed recovery Tool, never app rebuild/replacement | 017-70f7 |
| H04 | 008-22c4; exact adapter and parity BLOCKED | K typed owned-PID restart Tool with approval, never daily-app auto-retry | 017-70f7 |
| H05 | 006-6885, 008-22c4; exact adapter and parity BLOCKED | K maintenance process registry/reap policy; unknown roster refuses | 017-70f7 |
| H06 | 008-22c4; exact adapter and parity BLOCKED | K Timer guard observations + deduplicated Notify | 017-70f7 |
| H07 | 006-6885, 008-22c4; exact adapter and parity BLOCKED | K retained optional external utility, not authoritative workflow telemetry | 017-70f7 |
| H08 | 008-22c4; exact adapter and parity BLOCKED | K metadata-only maintenance inventory; never auto-drop stash | 017-70f7 |
| H09 | 006-6885, 008-22c4; exact adapter and parity BLOCKED | K optional external diagnostics Tool, availability explicit | 017-70f7 |
| M01 | 006-6885, 008-22c4; exact adapter and parity BLOCKED | K research/memory policy and scoped tools, not run-state storage | 017-70f7 |
| M02 | 006-6885, 008-22c4; exact adapter and parity BLOCKED | K upkeep Tool and final deterministic check | 017-70f7 |
| M03 | 006-6885, 008-22c4; exact adapter and parity BLOCKED | K maintenance subflow with unread/expiry protections and final Gate | 017-70f7 |
| M04 | 002-eeb6, 004-2809, 006-6885, 008-22c4; exact adapter and parity BLOCKED | K authoritative policy import; R competing local routing tables | 017-70f7 |
| M05 | 006-6885, 008-22c4; exact adapter and parity BLOCKED | K failure ownership/escalation policy + memory reconciliation | 017-70f7 |
| G01 | 004-2809, 016-358f, 017-70f7, 019-683f; exact adapter and parity BLOCKED | K ownerless/wave alarms and admission; D forced staffing floor as turn-stop prerequisite, D1 | 017-70f7 |
| G02 | 006-6885, 008-22c4; exact adapter and parity BLOCKED | K effective harness policy/tool injection audit across clients | 017-70f7 |
| G03 | 005-f907, 014-5d73, 004-2809, 016-358f; exact adapter and parity BLOCKED | K daemon ledger visibility and branch-local inbox; R pump-shell guard after pump removal | 017-70f7 |
| G04 | 005-f907, 007-7ce3, 006-6885, 008-22c4; exact adapter and parity BLOCKED | K owned-process and executor-environment invariants plus effective hook audit | 017-70f7 |
| G05 | 008-22c4, 016-358f, 006-6885; exact adapter and parity BLOCKED | K proposal/hold owner validation and source commit guard; infra UNKNOWN explicit | 017-70f7 |
| G06 | 008-22c4, 018-bb68, 016-358f; exact adapter and parity BLOCKED | K proof-debt publication Gate with exact validated exception | 017-70f7 |
| G07 | 008-22c4, 016-358f; exact adapter and parity BLOCKED | K publication preflight Tool/policy, missing denylist refuses | 017-70f7 |
| F01 | 002-eeb6, 004-2809, 016-358f; exact adapter and parity BLOCKED | K Start input schema + immutable requirement snapshot | 017-70f7 |
| F02 | 003-e678, 004-2809, 006-6885, 008-22c4; exact adapter and parity BLOCKED | K Fork of three configurable Authors, per-node concurrency 1 | 017-70f7 |
| F03 | 006-6885, 008-22c4, 016-358f; exact adapter and parity BLOCKED | K independent Author/decision with globally allowed exact profile, no fixed vendor | 017-70f7 |
| F04 | 005-f907, 007-7ce3; exact adapter and parity BLOCKED | K Run command with explicit shell only when published as argv | 017-70f7 |
| F05 | 005-f907, 014-5d73, 008-22c4, 016-358f; exact adapter and parity BLOCKED | K deterministic policy Gate + asynchronous approval task where required | 017-70f7 |
| F06 | 008-22c4, 016-358f, 014-5d73, 005-f907; exact adapter and parity BLOCKED | K persisted visible reject counter separate from total-review/repair counters | 017-70f7 |
| F07 | 005-f907, 007-7ce3; exact adapter and parity BLOCKED | K mutating command effect with intent/reconciliation, no blind retry | 017-70f7 |
| F08 | 008-22c4, 016-358f, 018-bb68; exact adapter and parity BLOCKED | K End only after required current receipts/obligations | 017-70f7 |
| F09 | 003-e678, 004-2809, 014-5d73; exact adapter and parity BLOCKED | K typed explicit outcomes and canvas edge styling | 017-70f7 |
| F10 | 005-f907, 007-7ce3, 014-5d73; exact adapter and parity BLOCKED | K named refusal branch and explicit resume target/reconciliation | 017-70f7 |
| F11 | 006-6885, 008-22c4, 014-5d73, 016-358f; exact adapter and parity BLOCKED | K editable Author preset | 017-70f7 |
| F12 | 006-6885, 008-22c4, 014-5d73, 016-358f; exact adapter and parity BLOCKED | K editable Author preset | 017-70f7 |
| F13 | 006-6885, 008-22c4, 014-5d73, 016-358f; exact adapter and parity BLOCKED | K editable Author preset with artifact output schema | 017-70f7 |
| F14 | 003-e678, 004-2809, 014-5d73, 016-358f; exact adapter and parity BLOCKED | K bounded collection expansion into correlated Author activations | 017-70f7 |
| F15 | 006-6885, 008-22c4, 014-5d73, 016-358f; exact adapter and parity BLOCKED | K configurable Author preset, declared artifact/native evidence policy | 017-70f7 |
| F16 | 006-6885, 008-22c4, 014-5d73; exact adapter and parity BLOCKED | K Author inspector and pinned role | 017-70f7 |
| F17 | 006-6885, 008-22c4, 014-5d73; exact adapter and parity BLOCKED | K template/editor/final launch receipt distinction | 017-70f7 |
| F18 | 006-6885, 008-22c4, 014-5d73; exact adapter and parity BLOCKED | K exact profile/model/effort checked against global authority | 017-70f7 |
| F19 | 006-6885, 008-22c4, 014-5d73; exact adapter and parity BLOCKED | K declared capabilities and write/authority scope | 017-70f7 |
| F20 | 006-6885, 008-22c4, 014-5d73, 016-358f; exact adapter and parity BLOCKED | K Author implementer preset | 017-70f7 |
| F21 | 005-f907, 007-7ce3, 014-5d73; exact adapter and parity BLOCKED | K typed command node | 017-70f7 |
| F22 | 005-f907, 007-7ce3, 014-5d73; exact adapter and parity BLOCKED | K typed adapter/action node, same effect protocol | 017-70f7 |
| F23 | 006-6885, 008-22c4, 016-358f, 014-5d73; exact adapter and parity BLOCKED | K configurable validator Author plus deterministic checks as applicable | 017-70f7 |
| F24 | 008-22c4, 016-358f, 014-5d73; exact adapter and parity BLOCKED | K current Judge extended with typed subject/evidence/uncertainty | 017-70f7 |
| F25 | 002-eeb6, 004-2809; exact adapter and parity BLOCKED | D explicitly excluded by Boss; not a replacement requirement | 017-70f7 |
| F26 | 002-eeb6, 004-2809; exact adapter and parity BLOCKED | D explicitly excluded by Boss; not a replacement requirement | 017-70f7 |
| F27 | 003-e678, 004-2809, 014-5d73; exact adapter and parity BLOCKED | K deterministic typed predicate/outcome node, no arbitrary frontend expression engine | 017-70f7 |
| F28 | 003-e678, 004-2809, 014-5d73, 016-358f; exact adapter and parity BLOCKED | K bounded Fork/collection routing preset | 017-70f7 |
| F29 | 003-e678, 014-5d73; exact adapter and parity BLOCKED | K pinned subflow with expansion | 017-70f7 |
| F30 | 008-22c4, 016-358f, 014-5d73; exact adapter and parity BLOCKED | K existing bounded Loop retained, counters distinct | 017-70f7 |
| F31 | 005-f907, 014-5d73, 008-22c4; exact adapter and parity BLOCKED | K durable existing Notify extended by delivery adapters | 017-70f7 |
| F32 | 005-f907, 014-5d73; exact adapter and parity BLOCKED | K branch-local structured inbox question | 017-70f7 |
| F33 | 005-f907, 014-5d73, 008-22c4, 016-358f; exact adapter and parity BLOCKED | K policy/approval without blocking unrelated branches | 017-70f7 |
| F34 | 005-f907, 014-5d73; exact adapter and parity BLOCKED | K explicit pause scope and pinned resume target | 017-70f7 |
| F35 | 005-f907, 014-5d73; exact adapter and parity BLOCKED | K durable deadline/wake effect, restart-safe and no consent on expiry | 017-70f7 |
| F36 | 002-eeb6, 004-2809, 003-e678, 014-5d73; exact adapter and parity BLOCKED | K immutable active/transitive pins, concurrent draft revision | 017-70f7 |
| F37 | 005-f907, 014-5d73; exact adapter and parity BLOCKED | K inspect/resume explicit paused run independently of editor | 017-70f7 |
| F38 | 002-eeb6, 004-2809, 014-5d73; exact adapter and parity BLOCKED | K validated expected-revision publication, already exists at core | 017-70f7 |
| F39 | 014-5d73; exact adapter and parity BLOCKED | K discard draft changes only, preserve publications and runs | 017-70f7 |
| F40 | 005-f907, 014-5d73; exact adapter and parity BLOCKED | K usage projection with coverage/unknown disclosure | 017-70f7 |
| F41 | 005-f907, 014-5d73; exact adapter and parity BLOCKED | K estimated/settled amount and rate source, not fabricated money | 017-70f7 |
| F42 | 005-f907, 007-7ce3, 004-2809, 016-358f, 014-5d73; exact adapter and parity BLOCKED | K durable ready/admitted/blocked/running view with resource reasons | 017-70f7 |

## Current field and operation observations

| Surface | Current evidence | Rework destination or blocking unknown |
|---|---|---|
| Draft and published graph | `workflows/store.rs` defines `WorkflowDraft` and `PublishedWorkflow`; published fields include project, kind, closure, checks, graph and revision. | 002-eeb6 must define the neutral pin; transitive equality and handler availability remain unverified. |
| User actions | `workflows/api.rs` exposes draft create/list/update, closure/check updates, publication, definition inspection and file import/export. | 014-5d73 and 015-608c must preserve authoring and transport actions; no parity claim. |
| Native node fields | `workflows/definition.rs` defines agent role/capabilities/prompt, command, tool, call/return, branch, loop, join and other native node kinds. | 002-eeb6 and 003-e678 define descriptor and typed-flow contracts; mapping each field remains blocked until verified. |
| Export/import | `workflows/file.rs` saves published definitions with format version 1, portable-path validation, canonical JSON and atomic write; default destination is `.fabbrica/workflows`. | 017-70f7 must preserve file consumers and distinguish portable export from active-run migration. |
| Run identity | `workflows/run/model.rs` `RunSnapshot` carries project, plan, story-definition revisions, story executions and optional PR/issue subjects, plus effects and graph executions. | 004-2809 must remove mandatory domain coupling without discarding old run history. Reuse of snapshot and store APIs unchanged is not established. |
| Transaction and cursor | `workflows/run/store.rs` uses SQLite immediate transactions; `events_after`, `snapshot`, `replay` and expected-command APIs are real entry points. | 004-2809 must verify transactional boundaries and event cursors under neutral ownership. The present inspection does not prove unchanged reuse. |
| Signals | `docs/sync-matrix.md` documents `workflow-run-changed` as an IPC/SSE wake hint, not an event-replay substitute. | 015-608c must preserve cursor replay and transport parity. |
| Fabbrica attention | `fabbrica_attention.rs` reads the configured upstream; `fabbrica_attention_tests.rs` uses recorded local fixtures. | 017-70f7 must verify each consumer and retain unknown/unreachable separately from empty. |

## Runtime preservation boundaries

The source anchors below name existing contracts for targeted investigation.
They do not assert that a replacement already passes them.

| Boundary | Existing evidence / consumer | Destination and remaining proof |
|---|---|---|
| Publication pins and history | `workflow-baseline/contract.md` describes versioned graph-start pins; `workflows/run/graph_tests.rs` consumes the captured serial fixtures. | 002-eeb6 must pin its complete descriptor closure; 017-70f7 must preserve inspection of old publications. |
| Generation and stale result fences | `docs/backend/workflows.md` sections on correlated parallel regions and typed evidence; graph transitions in `run/graph.rs`. | 003-e678 must name exact join identity and stale-result tests; 004-2809 must retain durable ownership fences. |
| Resource admission and budgets | `RunSnapshot` contains run limits, loops, creations and spawns; `workflows.md` documents command, process and workspace admission. | 004-2809 and 005-f907 must verify resource reservation, rollback and counters; snapshot field presence is not enforcement proof. |
| Artifact freshness | `workflows.md` typed evidence section and `run/model.rs` receipt types. | 005-f907 and 016-358f must independently prove stale artifacts cannot release a later gate. |
| Branch-local questions | `workflows.md` branch-local questions section and `run/questions.rs`. | 005-f907 must retain answer identity, independent branches and explicit resolution. |
| Cancellation and recovery | `workflows.md` owned process close and pinned reusable subflows sections; `run/store.rs` includes `reconcile`. | 004-2809 and 005-f907 must prove stop ownership and recovery without replaying unknown external writes. |
| Duration and timer | `workflows.md` durable timers and `run/timer.rs`. | 004-2809 must preserve pause, persisted timer and restart behavior without treating wall-clock absence as success. |
| Desktop, HTTP, SSE, MCP and files | `workflows/api.rs`, `run/api.rs`, `file.rs`, `docs/sync-matrix.md` and `docs/backend/workflows.md`. | 015-608c owns parity; 017-70f7 owns portable-file compatibility. Each actual route and shape still needs a complete crosswalk. |

### Named current invariants to preserve

- Graph-start pins record a definition digest and policy imports; the v2
  fixtures retain exclusive Join behavior. New drafts do not migrate a run
  (`workflow-baseline/contract.md`, `run/graph_tests.rs`).
- A fork generation scopes branch check receipts. A superseded activity keeps
  its admitted slot and check claim until stop is observed; a previous daemon's
  empty registry is not proof of process exit (`docs/backend/workflows.md`,
  correlated parallel regions).
- Question answers carry a revision, subject digest and actor. A branch-local
  question leaves other branches able to run, but an unanswered worker keeps
  its owned slot. Expiry does not approve or automatically resume
  (`docs/backend/workflows.md`, branch-local questions).
- Durable timers retain key, owner, absolute due/expiry and delivery budget;
  they wake a consumer rather than prove its success (`docs/backend/workflows.md`,
  durable timers).
- The existing command journal recovers a recorded identity before launching;
  a command receipt names both the input subject and whether stop and resource
  release were observed (`command/executor.rs`, `command.rs`).
- The run snapshot records loop, spawn, story-creation and duration controls.
  Separate correction traversal accounting is the future 016-358f contract,
  not proof that current generic retry accounting already meets it.

Future correction metanodes belong to 016-358f: the configured region repeats on
CI failure or a relevant comment; the cap counts correction traversals after the
initial attempt and survives commits, replacement and restart. Duplicate events
must neither spend nor restore budget. Stale artifacts cannot pass; exhaustion
blocks or asks rather than silently accepting. No current scheduler policy is
introduced by this inventory.

## Definition and editor field inventory

`workflows/definition.rs` is the authority for the actual serialization; the
Designer is a consumer, not a second backend validator. The following groups
cover its current node variants. The destination is a planned contract, not a
claim that a new handler already exists.

| Existing nodes / fields | User action or effect | Destination / unresolved mapping |
|---|---|---|
| Start, End | Enter or finish a graph. | 003-e678 control contracts and 004-2809 owner; preserve terminal receipt semantics. |
| Agent: role, capability strings, prompt template, optional PTY or ACP route | Configure and launch an agent; the strings do not themselves grant effects. | 002-eeb6 admission, 006-6885 execution and 014-5d73 inspector; exact routes and effective authority remain separate. |
| RunCommand: typed command; Tool: ID/revision and command | Start a potentially mutating host operation with resources, workspace, output and secret references. | 005-f907 and 007-7ce3 must retain receipts and stop semantics; 008-22c4 owns native operations. |
| CreateStories; StoryDispatch: template ID/revision | Create or dispatch story work. | 008-22c4 typed service and 016-358f composite; 017-70f7 maps current policies. |
| Call: definition/revision, nullable boolean inputs, output names; Return: outcome/output map | Invoke a pinned same-daemon nested definition and return structured state. | 003-e678 general typed contracts; preserve current narrow input/output semantics when replaying old runs. |
| Judge, Gate, Pause: optional resume target, Loop: max iterations | Decide, pause or repeat with existing receipts and budgets. | 003-e678 control flow, 005-f907 approvals and 016-358f configured correction; old counts must not be guessed into a new run. |
| Join, Fork: join/branches/repair target, AllJoin: fork | Route exclusive or correlated parallel work. | 003-e678 join identities and 004-2809 persisted scheduling; retain historical exclusive Join. |
| ForEach: join/body, literal items or receipt evidence, max items and empty policy | Expand a bounded collection. | 003-e678 allows typed collections from declared sources; do not constrain future inputs to agent evidence. |
| IfElse: input or receipt predicate; Notify | Route on a declared predicate or issue notification. | 003-e678 closed typed predicates and 005-f907 durable notifications. |
| GitHubReview event; IssueTriage; GitHubIssue effect; GitHubPublication action; IssueHandoff template/revision | Read issue state, write scoped review/comment/label/publication effects, or start story delivery. | 010-978f and 008-22c4 preserve host authority, selected subject and uncertainty; generic graph labels are not authorization. |

`WorkflowDesigner.tsx`, `WorkflowPalette.tsx`, `WorkflowInspector.tsx`,
`WorkflowFiles.tsx`, `ReviewPolicy.tsx` and `useWorkflowDraft.ts` are the
current editor seams. `WorkflowGraph` also persists nodes, edges, optional
evidence policy and per-node spatial layout; layout is presentation, not
execution authority. The API exposes create, duplicate, discard, update,
publish, import and export actions. 014-5d73 must verify field-by-field
presentation and behavior during implementation; an editor control's presence
does not establish executable support.

## Neutral persistence implementation boundary (004-2809)

The first staged 004 slice uses additive schema-20 neutral run/event tables in
the existing run database, under the existing `RuntimeOwner` process lock.
It retains verified publication and descriptor bodies plus normalized input and
optional subject values. It reuses the immediate-transaction and payload-bound
idempotency pattern, not the domain-shaped `RunSnapshot` or event enum.
Inspection reads the retained event; it does not resolve a current catalog or
story backend. Legacy rows stay unchanged. The subsequent schema-21 execution
extension adds commands and decisions to the same neutral event table; there is
no legacy-row conversion or invented resume position. The existing actor selects
the retained contract before any legacy scheduling or effect path.

The focused 004 runtime wave (rb20261110T215857Z-68342) passed 37 tests, including
same-owner resolved host invocation, collection/fork/loop results, current child
admission, restart inspection, immutable results and duplicate join receipts.
The owner uses retained subflow frames, not a host activity that runs another
scheduler. Exact source hashes and earlier failed runs are retained with the
004 evidence. The runtime review found four defects: shared recovery was gated by
corrupt payloads, early actor returns retained dead mailbox entries, prepared
ancestor siblings consumed child capacity, and storage errors became workflow
failures. Corrections passed 20 affected tests in rb20261110T221941Z-12929 with
all 61 frozen source hashes matched, including actual actor recovery, nested
subflows at depths 2/4, failed fork producer settlement and SQLite-write retry.
The rb source-tree digest is
`7cfe1b867b70132f6dbf938d6dbef13a6c0ef613a4fcc5c10483b741eeb8cbb1`;
the local frozen subset manifest has a separate digest. The coordinator accepted
the correction disposition from the source and targeted result. Final schema-21
upgrade/legacy replay preservation and future-schema refusal passed 2/2 in
rb20261110T222725Z-16920. This is not native service readiness, migration
activation or an operational cutover.

The proposed cutover remains the separate 019 boundary: drain active legacy work
under its retained contract, preserve historical inspection, and start new neutral
runs only after explicit approval. Real v2 fixture bytes remain the comparison
source; no current legacy event is reinterpreted as a neutral position. Effect
correlation, uncertain outcomes and owned-resource stop receipts remain 005;
real native service authority/freshness and invariants remain 008.

## Store and executor reuse assessment

This is an assessment of current interfaces, not a test result for the future
owner. “Candidate” is not authorization to fork or duplicate a scheduler.

| Primitive | Verified current boundary | Reuse decision / migration consequence |
|---|---|---|
| Event storage | `run/store.rs` creates `workflow_events` with `(run_id, sequence)` primary key and `(run_id, command_id)` uniqueness. `insert_event` stores the event and receipt. | The sequence/unique-command database pattern is reusable. The concrete table also references `workflow_runs`; unchanged reuse by a new neutral owner is not proved. 004-2809 owns its transaction design. |
| Cursor read | `events_after` first reads `snapshot(run_id)`, rejects a negative cursor and limits outside 1..500, then orders `event_json` by `sequence`. | The pagination rule and query are reusable semantics; the method itself depends on the old snapshot and must not be declared unchanged for neutral runs without an integration proof. |
| Transaction and retries | `command_at_checked_with_precommit` checks command receipt/hash, snapshots and optional expected sequence, does external probes before the writer lock, takes an immediate SQLite transaction, rechecks preflight, persists event and optional report disposition, then commits. | The transaction sequence and idempotent-receipt rule are candidates. `choose_event`, story fingerprints, dispatch slots and `RunSnapshot` are domain-bound, so the full method is not reusable unchanged as a neutral command dispatcher. |
| Run admission | `workflow_runs` requires `plan_id`; `insert_started_run` enforces active uniqueness using project, plan and subject key. | This schema and admission cannot by themselves express a run without a plan. 004-2809 must provide neutral admission without reinterpreting existing runs. |
| Replay | `replay` uses `replay_events`, which decodes the existing typed events and rebuilds a `RunSnapshot`. | Historical replay remains owned by the existing contract versions. A neutral reducer cannot claim this implementation unchanged while it returns the domain-shaped snapshot. |
| Command execution | `command/executor.rs` exposes `CommandJournal::execute` with a typed definition, identity, workspace, runs DB and installed adapters; it recovers an existing receipt before starting a new process. `CommandIdentity` pins run/execution/activation/attempt/generation/effect; `CommandSubject` pins workspace/commit/tree. | This is a real adapter candidate for development commands, not a generic model or arbitrary HTTP executor. It currently requires a Git artifact and workflow run database, so unchanged reuse for non-repository runs is not established. 005-f907 and 007-7ce3 must preserve stop/reconciliation and only reuse it where those assumptions hold. |
| Host issue/PR services | `github_pr_api.rs` provides account-aware REST/GraphQL ports; `run/runtime/issue.rs` and `pull_request.rs` hold domain orchestration and subject checks. | Reuse the service ports where their authority and subject fit; do not copy the specialized graph scheduler into the neutral core. The issue source explicitly refuses its native triage evaluator until its documented cutover. |

Tests `concurrent_story_revision_rejects_a_prepared_run_event` and
`concurrent_run_revision_rejects_a_prepared_run_event` in `run/store.rs`
protect the current transaction against changed source state. They are
candidates for retained semantics, not proof that a neutral replacement is
correct or has been tested.

## User actions, authority and transports

| Action / field | Current entry point and consumer | Authority or preservation boundary |
|---|---|---|
| Edit graph and publish expected revision | `workflows/api.rs` and `src/components/WorkflowDesigner/`; `WorkflowDraft` holds draft and published revision counters. | A draft change must not mutate a pinned run. Descriptor-driven controls remain work for 014-5d73. |
| Start selected published graph | `RunAction::StartGraph` includes target, optional expected revision, definition ID/revision, request ID, limits and launch-only secrets. | `run/api.rs` states that secret values remain in daemon memory; neutral admission must not infer grants from a label. |
| Inspect run and events | `RunAction::Inspect`, `Get` and `Events` expose the sequence/cursor; StoriesDialog consumes `workflow_run_action`. | A wake is not the event stream; clients must resynchronize after a dropped event. |
| Answer a question | `RunCommand::AnswerQuestion` carries expected revision, subject digest, actor and structured answer. | 005-f907 must retain identity and subject freshness; an answer from another branch cannot settle this question. |
| Report an agent attempt | `RunCommand::ReportAttempt` carries generation; `mcp_transport_ancillary.rs` checks bound managed caller, project and transport. | 004-2809 must retain freshness and caller binding; do not equate declared capability text with authority. |
| Definition/run IPC and HTTP | `lib.rs` registers `workflow_definition_action` and `workflow_run_action`; `mcp_http/mod.rs` registers the matching `/workflows/definition/action` and `/workflows/run/action` routes. | 015-608c must validate the same request and result semantics on each transport. |
| Managed MCP | `mcp_transport_ancillary.rs` handles workflow actions and requires a bound live managed session for run mutation, with explicit exceptions in source. | Preserve the actual caller/project checks rather than treating a graph as the caller. |
| Managed agent PTY and ACP | `workflows/definition.rs` defines distinct PTY profile and ACP profile/model/effort routes; `run/runtime/effects.rs` selects the launch route and managed MCP reports check their bound transport. | 006-6885 must preserve generation and caller identity; an ACP peer is not a PTY session and a declared route is not a mediated write grant. |
| SSE and portable file | `sse_routes.rs` exposes workflow change wakeups; `workflows/file.rs` handles definition files. | 015-608c owns cursor and transport parity; 017-70f7 owns import/export mapping. |

### Candidate existing tests

`run/graph_tests.rs` names `old_exclusive_join_replays_without_parallel_wait`,
`draft_publish_does_not_repin_active_run` and
`contract_pin_refuses_tampering_and_legacy_child_start_stays_serial`.
`src/__tests__/transport.test.ts` contains transport mapping assertions and
`mcp_http/sse_routes.rs` contains
`workflow_run_changed_is_a_cursor_wake_hint`. These are candidates for reuse,
not evidence that they were run in this task or cover neutral contracts.
The earlier baseline's `sources.json` labels its test inventory source-only.

## Connector operation inventory (source inspection)

These are existing client operations, not proof of a neutral workflow adapter,
provider authority or successful live execution. Every row requires the owner
to verify the relevant external contract and implement missing workflow binding,
freshness, grants, effect reconciliation and recorded evidence before use.

| Required operation | Existing source seam | Missing or blocking unknown | Owner |
|---|---|---|---|
| HTTP method/path/query/headers/body and authentication references | `plugin_http.rs` implements plugin HTTP and account-bound credential injection, with host/origin checks; `plugin_github.rs` is an account-scoped PR service bridge. | Neither plugin port is a workflow capability grant or durable effect. 009-e32f needs typed response mapping, separate network/write grants, bounded fields and reconciliation. | 009-e32f |
| Jira issue lookup and search | `plugins/jira/provider.js` detail/list and provider request path | Plugin provider is not a workflow operation; fields, paging and subject authority need validation. | 011-2a30 |
| Jira create, update, comment, transition | `plugins/jira/provider.js` `act` sends create, edit, comment and transition requests. | No neutral workflow binding or effect reconciliation proved; recorded fixture tests are not live mutation evidence. | 011-2a30 |
| GitLab issue and merge-request lookup | `plugins/gitlab/provider.js` list/detail and `refs.js` issue/MR path selection. | Workflow binding, pagination and selected subject authority need proof. | 012-1981 |
| GitLab event intake | Provider read operations above do not establish event intake. | BLOCKED: intake source, correlation and delivery semantics not established. | 012-1981 |
| GitLab pinned diff/change review | `plugins/gitlab/model.js` notes limitations around diff positions. | BLOCKED: pinned revision/diff review contract and freshness not established. | 012-1981 |
| GitLab pipeline status | `plugins/gitlab/provider.js` reads MR details; `pipelines.js` supplies pipeline and job handling. | Exact workflow polling, subject revision and reconciliation remain unverified. | 012-1981 |
| GitLab comment/review publication | `plugins/gitlab/provider.js` sends notes and approval actions. | Workflow authority, pin freshness and unknown-write reconciliation remain unverified. | 012-1981 |
| GitHub issue/PR read and trigger intake | `workflows/store/bindings.rs` defines PR polling and issue-label polling, disabled-first revisioned bindings, with a unique source/repository constraint; workflow issue/PR subject code is specialized. | 010-978f must verify selection, event ownership and correlation; no generic adapter is established here. | 010-978f |
| GitHub PR diff and CI/check observation | `github.rs` exposes a pinned PR-diff path; `github_ci_checks.rs` and workflow PR activity code are seams for current checks. | Prove the diff, CI and review receipts all refer to the selected head; stale evidence must not release a later join. | 010-978f |
| GitHub issue search/create/update | Issue lookup and specialized issue intake exist; generic search/create/update workflow operations are not established by that fact. | BLOCKED: enumerate actual service contracts and grant scopes before claiming each operation. | 010-978f |
| GitHub trigger enable/disable and cutover | `workflows/store/bindings.rs` defines disabled-first, revision-checked PR-poll and issue-label-poll bindings. | 010-978f must preserve event identity and single-consumer handover; 019-683f owns operational transfer. | 010-978f, 019-683f |
| GitHub issue comment/label and PR publication/review | `workflows/definition.rs` declares host-owned effect variants; `run/runtime/issue.rs` and `github_pr_actions.rs` are implementation seams. | Generic grants, subject freshness and lost-response reconciliation must be validated independently. | 010-978f |
| Telegram callback decision and restart | `telegram/callbacks.rs` holds callback handles in a runtime vector, matches private chat/sender/message, and requires a recipient peer. | This is not a persistent workflow question binding. 013-66b3 must bind revision, subject and arguments durably and reject stale/duplicate callbacks after restart. | 013-66b3 |
| Telegram outbound and inbound response | `telegram/tool.rs` exposes register, begin, activity, finish and send; `telegram/inbound.rs` polls updates; `telegram/outbound.rs` owns sending. | Workflow-scoped correlation, replay, interactive answers and authority remain unverified. | 013-66b3 |

### Connector evidence and limits

| Connector | Evidence already present | Limits and next proof |
|---|---|---|
| Jira | `plugins/jira/fixtures/cloud.json` and its README describe scrubbed authorized scratch-site responses, including create, transitions, comments and search; `recorded-cloud.test.js` consumes them. | The explicitly synthetic `cloud-contract.json` is not a recording. The fixture README leaves rate-limit and other deployment variants unverified. 011-2a30 must bind individual operations to grants and reconcile writes. |
| GitLab | `plugins/gitlab/fixtures/recorded.json` records a scrubbed REST v4 container session; its provenance says pipeline/job rows were driven through the console without a runner. `provider.test.js` consumes fixture-backed replies. | This is not workflow dispatch or live event intake; pinned diff/change review and external writes need their own evidence. |
| GitHub | `src-tauri/fixtures/github-writes/README.md` identifies recorded read-only PR and issue responses reused as write response shapes; workflow issue publication tests use a local fake. | Read responses and fake writes are not proof that a live write occurred. 010-978f must prove authority, correlation and recovery on its named boundary. |
| Telegram | `src-tauri/src/telegram/fixtures/README.md` identifies a public example, not a live bot capture; later-required fields are explicitly adapted in tests. | Live response, callback and interruption evidence is not established by that fixture. 013-66b3 must retain this distinction. |

## Documentation obligations for later behavior changes

`docs/sync-matrix.md` requires the feature checklist (`docs/FEATURES.md`,
`CHANGELOG.md`, `SPEC.md`, domain docs and discoverability), IPC/HTTP parity
references (`docs/api/tauri-commands.md`, `docs/api/http-api.md`) and
`docs/backend/mcp-http.md` for transport changes. Update
`docs/backend/workflows.md`, `docs/design/workflow-graph-runtime.md`, and the
applicable workflow user guide when their described contracts change. Update
the matrix's `workflow-run-changed` entry if its payload or routing changes.
These are obligations for later implementation, not claims that this inventory
changed the product.

## Fabbrica consumer boundaries

| Consumer | Existing behavior / evidence | Rework obligation |
|---|---|---|
| Portable definition file | `workflows/file.rs` writes the published graph, checks and closure; `file_references.rs` exports named, content-hashed StoryDispatch/Call references and resolves them on import. `WorkflowFiles.tsx` loads a file as a new draft and does not publish it. | 017-70f7 must preserve format-1 import/export or provide an explicit migration decision; no active run may change from a file edit. |
| Control MCP attention | `fabbrica_attention.rs` consumes upstream task/help/outside-request results; `boss_attention.rs`, `waitingItems.ts`, `useWaitingItems.ts` and `WaitingOnYou.tsx` expose attention to clients. | 017-70f7 must inventory and verify ownership, unanswered/answered and unknown/unreachable handling; a missing upstream is not proof that no work exists. |
| Recorded control replies | `src-tauri/src/fixtures/fabbrica/README.md` records the upstream revisions and capture paths for task, help and outside-request calls. The source says later request scenarios used local fakes for external services. | Reuse for protocol shape only; do not present fake external services as live provider evidence. |

## Evidence and open work

`workflow-baseline/sources.json` names source-only native test inventory and
its original revision. `docs/backend/workflows.md` documents the existing
contracts, including parallel regions, evidence, timers, questions and subflows.
Neither document proves this new design has parity. Each candidate replacement remains blocked until its owner verifies it against
the current sources and affected consumers. In particular, unchanged reuse of
the domain-bound executor and complete transport behavior are not assumed.

## 005 neutral effect boundary

The shared neutral command ledger and actor now own exact effect preparation,
callback/approval correlation, uncertainty, reconciliation and cancellation.
Schema22 adds a same-transaction resource projection, not a second scheduling
queue. Initial preparation refusal and queued approval expiry settle only
the affected attempt. Declined/resolved approvals cannot reopen; closed graph
frames do not prevent valid stopped observations from releasing effect ownership.
Hosts retain deterministic preparation, including the original absolute question
deadline, across re-admission. The existing command journal remains a Git/process adapter boundary; its
private claims and story-derived question subject calculation are not generic
execution services. Native adapters must continue to enforce their own service
invariants. No native provider, live external write, trigger migration, operational
cutover or public neutral transport is introduced by this slice.

The owned Rust protocol tests use real SQLite and file writes in repository-local
scratch space. These tests can prove owner ordering, replay and correlation; they
do not prove an external connector's idempotency, a native process's stop semantics,
or real capability freshness. Those remain assigned to the native/connector and
integrated acceptance stories. A lost response is reconciled by observing the
retained operation, or remains blocked; generating another invocation is not a
recovery path.

## 006 model ownership work in progress

The existing effect ledger now supports an immutable original model-session
binding, host-pinned named operations and child tool-call/effect/result identity.
Kernel entry persists intent before dispatch, reads identical duplicate calls,
rechecks current admission and retains parent completion while children settle.
The coarse evidence helper reuses the existing clean Git artifact check and pins
regular-file blob IDs from the retained commit; it grants no process authority.

The exact-profile/model/effort adapter now uses the existing ACP manager with
fixed empty-built-in/no-hooks/bare flags, strict context/output/time bounds and
complete JSON/schema validation. It constructs its own named-tool MCP host, binds
the exact session before prompting, and returns tool responses only from committed
kernel effects. Other MCP methods, unknown tools, altered original identities and
stale origins refuse. Parent completion waits for owned effects. Recorded ACP
fixture and owned MCP/SQLite/file tests prove these internal paths.

Restricted coarse execution explicitly returns `restricted_execution_unavailable`
before process entry because existing advisory Git/capability guards cannot
withhold direct privileges. The unavailable entry has no request or original-handle
branches; the pinned artifact helper never supplies process-stop or individual
commit/push/API receipts.
This is the authorized unavailable branch, not a claim of working native isolation.

The original provider-call continuation remains an unresolved transport requirement
and criterion3 stays open. Stable IDs, kernel replay or another prompt cannot close
that gap. Native service installation/receipts remain008; public transport and
cutover remain015/019. Story006 is not complete and no paid provider, live write or
OS privilege-isolation evidence is claimed.

## 012 GitLab selected flow

`workflows/gitlab` and `run/neutral_gitlab*` reuse the existing Rust plugin HTTP
and account client, shared connector metadata, neutral admission and effect owner.
They do not execute the frontend Tasks provider or copy its credential handling.
Installed v1 operations cover single issue/MR reads, a supplied exact diff version,
the selected MR head pipeline and one general note bound to head/base/effect.
These single-subject responses do not need collection pagination. Arbitrary search,
inline positioned review publication and MR approval remain missing, outside this
selected flow. The prior provider's existence is not evidence for those adapters.

Event normalization preserves unknown base state and original delivery/generation.
The GitLab backend intake facade passes its installed typed snapshot to the shared
neutral binding owner for mapping, enablement, durable dedup and replay. Delivery
authentication and native source installation remain composition responsibilities;
this internal slice does not register an ingress endpoint or webhook.
Recorded CE 19.4.1 and official REST v4 examples have separate provenance in
`src-tauri/fixtures/gitlab-workflow/README.md`. No live credentials, webhook delivery,
runner execution or external write was supplied or authorized. General-note
publication has no atomic head precondition; uncertain sends remain owned and
blocked rather than resending. No scheduler, account store or transport was added.

## Telegram connector slice (013-66b3)

The [operation matrix and evidence limits](workflow-telegram-connectors.md) compare
existing peer/automation sends and volatile callbacks with installed typed
send/update/reply activities and durable effect/question answers. BotApi, the
serialized worker, current chat configuration and the sole poller remain shared.
Uncertain sends block; there is no automatic resend or new credential store.
Host composition (008), public execution (015) and authorized live-account
validation remain separate obligations.

008 now supplies a native catalog and one provider through the 015 seam. Typed
adapters reuse native clean artifact checks, named owned verification, story
transitions, commit and bounded Git push services. The selected push adapter is
limited to canonical local bare repositories. Backend-created run context retains
an authority ceiling and rechecks current session capabilities; current exact
ledger approval is required for each mutation. Unbound run provenance refuses.
No universal grants database or generic shell replacement was added. Exact-change
review, public caller/run binding, network/API publication remain explicit gaps;
local native tests do not prove those missing integrations or live rollout.

008 follow-up: native.review.change reuses native staged diff and ACP review, retaining caller/workspace/base/tree/diff-digest evidence; commit requires that exact passing persisted review effect. Binary/oversized review inputs refuse. Public trusted caller binding and network/API publication remain open.

### 010 source trigger contract

Generic source bindings and admission share RuntimeOwner/RunStore. GitHub normalizes official documented issue/PR snapshots; GitLab 012 consumes the same event contract. Existing GitHub client/review reconciliation remain owners. See [trigger admission](workflow-trigger-admission.md); live account and Designer evidence remain explicit consumer gaps.
