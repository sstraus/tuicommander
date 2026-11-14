## Speech and download progress on `/events` (story `833-6fd4`, 2026-09-22) — **Rust, needs a `make dev` restart** (continued)
- [ ] Talk over a reply and confirm the last frame for that utterance is _(NOT VERIFIED 2026-09-30: needs the desktop frontend; deferred until after Boss restart. Backend: headless tuic-remote instance (rust0930) has no /dictation/* routes (curl UDS+TCP -> 404) and voice status says 'This TUICommander build has no audio support'; needs the desktop build. Talk-over needs armed conversation; audio can come via WS client, hearing cannot.)_ _(audit 2026-10-13: HUMAN — Armed microphone/speaker/model conversation required. Read commands.rs:2514-2570 and dictation_routes.rs:361-401; safe read-only speech status proven unarmed…)_ _(triage 2026-10-14: WAITS — Needs real microphone/speaker/hotkey observation; do not disturb Boss’s voice session.)_ _(HUMAN 2026-11-05: Read dictation/commands.rs:542 (doc: `interrupted` by whoever talked over the reply). Talk-over needs microphone + live reply; not armed on Boss's instance.)_
      `interrupted` rather than `finished`, and that it arrives — the transition
      happens on the render thread after `speak` has long returned, which is the
      case a polling client used to miss entirely.
## SSH local-forward readiness (story `1159-4e28`, 2026-09-28) — **Rust, needs a `make dev` restart**

- [ ] After restarting an isolated test instance, connect the Installed-service _(NOT VERIFIED 2026-09-30: blocked — second physical machine: aws-graviton (56481148) SSH remote; AWS box is DOWN per coordinator brief, and instance config must not be edited by hand.)_ _(audit 2026-10-13: HUMAN — Configured aws-graviton connection needs remote login/host readiness; no connection or credential changes attempted.)_ _(triage 2026-10-14: WAITS — Needs the specified remote daemon/SSH fixture and coordinated rollout; no live daemon restart.)_ _(HUMAN 2026-11-05: Read tunnels/supervisor.rs:280-298 (wait_until_ready under TUNNEL_CONNECT_TIMEOUT before reporting). Needs a second machine/SSH remote; not available.)_
      `aws-graviton` remote (56481148) over SSH. It should progress from
      Connecting to Connected once the local forward listens and `/health`
      answers, without an intermediate "installed daemon not answering" error.
      Record the elapsed time and the tunnel status transitions. Use only the
      configured host and credentials; the targeted Rust tests cover delayed
      local ports and delayed health independently.
## Error messages name the reorganized Settings pages (story `861-977b`, 2026-09-24) — **Rust, needs a `make dev` restart**

- [ ] [VISUAL] Settings → General with Experimental Features **off**: an **ego** _(NOT VERIFIED 2026-09-30: blocked — VISUAL-owned by tuic-live-checks)_ _(audit 2026-10-13: VISUAL-OWED — Pure visual requirement; screenshot/image inspection unavailable per brief; not attempted.)_ _(triage 2026-10-14: WAITS — GeneralTab.tsx:385/441/483/487 shows native Code Intelligence then ego, tooltip and Select/Clear outside the experimental gate; AiChatTab owns model/providers. Browser mode uses a text input instead of native file selection; prior GeneralTab DOM and AI Chat setup tests pass. Native visual arrangement remains: MacControl recording denied and locked Chrome cannot capture images. Evidence: ~/Gits/.tmp/results/to-test-1014/phase2-chat-setup-proof.json and ego-profile-row-proof.json.)_ _(HUMAN 2026-11-05: Read GeneralTab.tsx:133-134 (ego executable picker, settingsStore.state.egoExecutable). Visual arrangement with Experimental off needs a screenshot; no image capture allowed here.)_
      section sits directly after Code Intelligence, with a `?` tooltip, the
      "Configured at …" / "Not configured" line, **Select…** and **Clear**.
      Settings → AI Chat (Experimental Features on) shows only Default Model
      and Providers. With the ego path empty, AI Chat says to name the binary
      in Settings → General.
## Branch integration proofs (story 1295-a2ce)

- [ ] After Boss restarts `make dev` or rebuilds the release, query `repo action=branch_integrations` and `repo action=branch_integration` on throwaway fixtures and confirm the new proof fields are available. Check that an integrated squash branch appears in the branch panel and sidebar, and a content-based proof requires an archive of the current tip before safe deletion. Automated real-Git lifecycle, MCP and deletion checks passed; the installed process has not been restarted to load this Rust change. _(PARTIAL 2026-10-11: branch_integrations and branch_integration returned proof fields (in_sync, ancestry, patch_equivalence for a squash branch); deletion with an archived tip succeeded (proof archived) and an unarchived unmerged branch was refused. A content_superset proof could not be reproduced and the branch panel/sidebar was not checked. Evidence `~/Gits/.tmp/totest-1011b/ev/git2.json`. Isolated headless daemon built from main d2f8f13df.)_ _(audit 2026-10-13: VISUAL-OWED — Prior 2026-10-11 partial proof fields retained; integrated squash branch panel/sidebar and content_superset archive not exercised.)_ _(triage 2026-10-14: WAITS — Prior actual proof-fields/deletion evidence retained; content_superset archive branch case and live branch-panel/sidebar remain incomplete. Owned runtime unavailable and native screenshot denied.)_ _(HUMAN 2026-11-05: Read mcp_http/mcp_transport_ancillary.rs:220-240 (branch_integrations/branch_integration actions exist). Needs a registered fixture repo (would alter Boss repo list); panel/sidebar visual.)_
## Design Mode v2 S0 spike (1672-cd3d)

- [ ] [HUMAN] Once MacControl Screen Recording works (`capture_screen` returns an image), run S0 spike items (a), (b), (e) and (f) and record the evidence in `docs/design/design-mode-v2-s0-results.md`. Needs the Screen Recording grant of Boss; no restart of TUIC. _(Moved from the story by Boss's rule 2026-10-13.)_ _(triage 2026-10-14: WAITS — MacControl capture returned Screen Recording permission required on 2026-10-14.)_ _(HUMAN 2026-11-05: [HUMAN] S0 spike items (a),(b),(e),(f) need Boss's Design Mode browser. Precondition now met: mcp maccontrol capture_screen returned an image today (display 0 3024x1964). Spike results doc not written by this worker.)_
## Native story mdkb search (#2333-b498)

  `make agent-test AGENT_TEST_INSTANCE=agent-test-2333` instance and a scratch
  project admitted by mdkb. Create, amend, add a note, check and close a native
  story; `mdkb search "criterion phrase" --root <scratch-project>` must find its
  read-only record. Stop/restart only the scratch mdkb daemon and confirm changes
  made while it is down become searchable after reconnect. Check that no story
  file or `.tuic/stories` directory appears. Automated isolated-daemon coverage:
  `ingest_closed_story_is_findable_in_real_mdkb_without_a_story_file` (requires
  installed mdkb; explicitly ignored in unattended runs).

- [ ] Issue triage (#2324-424d): after a manual `make dev` restart or release rebuild, configure a throwaway allowlisted repository and an operator-attested read-only triage profile. With explicit GitHub-write authorization, verify accepted/rejected/needs-input decisions and comment/label delivery across restart. No live GitHub-write pilot was run by the implementer. TUIC does not enforce absence of git/gh credentials. _(HUMAN 2026-11-05: requires explicit GitHub write authorization on a throwaway allowlisted repo + restart. Code-verified: automations/triage/tests.rs:116 triage_rejection_publishes_one_comment_and_label_across_restart, :418; automations:: nextest 244/244 pass (loopback mock).)_
## WF01 shared workflow contracts (2508-3a15)

- [ ] After the next authorized Rust rebuild/restart, confirm a newly started workflow _(NOTE: description outdated 2026-11-05: item expects eventContractVersion 3, but workflows/run/contracts.rs:8-14 now defines SERIAL=2, PINNED_SERIAL=3, PARALLEL=4, BRANCH_INPUT=5, CURRENT=6 (runtime_start.rs:166 stamps CURRENT). Retained v2 exclusive Join covered by graph_tests.rs:790 old_exclusive_join_replays_without_parallel_wait (passes in workflows:: run: 436/438).)_
  returns eventContractVersion 3 with graph contract metadata; retained version-2
  runs stay inspectable/cancelable and use exclusive Join. Do not migrate live runs.
  Targeted recorded-run and HTTP/service parity tests provide automated coverage;
  the running development backend does not hot-reload this Rust change.

## WF06 — typed workflow evidence policy (2513-c433)

  draft into `evidencePolicy`, publish it, and inspect the pinned policy and
  review counters in the reviewer prompt. Verify that a stale artifact or
  failed required check cannot approve the story. This Rust change does not
  hot-reload into the current app. Targeted native-store automation covers the
  policy; no desktop instance was launched for this story.

- [ ] PR conversation links (#2559-dc9e): with GitHub Work loaded, click a sidebar PR’s Comments action, unresolved-thread count and PR notification. Confirm the exact PR/account opens in Tasks with Comments and review threads; return through Open branch. Unload the provider and confirm the GitHub fallback. _(HUMAN 2026-11-05: needs GitHub Work provider loaded with a real PR/account. Seen: Sidebar/PrStateBadge.tsx:61-62 unresolvedThreads props; no automated test for the Comments->Tasks navigation found in src/__tests__ (searched).)_

- [ ] After the next Rust rebuild and restart, verify MCP story discovery and the compact workflow tool schema; Rust changes do not hot-reload.

- [ ] After a manual rebuild/restart, complete an automation agent run that repaints a spinner, then inspect its saved output after the session closes. Confirm the final answer and retained scrollback are readable without lone spinner frames. Rust does not hot-reload; no daily instance was restarted (#2947-b80f).

- [ ] After an authorized rebuild, confirm existing workflow draft and published lists still open. The 002-eeb6 reusable-operation contract adds internal storage only; it does not add executable palette nodes or migrate existing definitions. The running backend does not hot-reload this Rust change.

- [ ] After an authorized Rust rebuild/restart, confirm existing workflow draft and publication lists still open after 003-e678 typed data contracts. Internal neutral format9 is fail-closed for older neutral formats; no active run or legacy definition is migrated. Contract tests cover the new store/data surface; this is only a daily-app smoke check, not proof of the later neutral runtime. Rust does not hot-reload, and no daily instance was restarted.

## Neutral execution ledger (004-2809, staged)

- [ ] After the next authorized Rust rebuild/restart, confirm existing workflow
  history opens after the schema-21 neutral-decision extension. Do not migrate or
  resume live runs for this check. Internal execution-ledger and actor tests target retained
  path/results/waits/cancel, command/attempt fences and scoped host-port execution,
  not public neutral dispatch or real native-service authority. Rust does not hot-reload; the agent did not restart the daily app.

## Neutral run store foundation (004-2809, staged)

- [ ] After the next authorized Rust rebuild/restart, confirm existing workflow
  history remains readable after the additive run-store schema-20 upgrade.
  Do not migrate or resume live runs for this check. The internal neutral start
  boundary has no public transport or activity dispatch in this persistence
  slice; its closure/retry/reopen behavior is covered by targeted Rust tests.
  Rust does not hot-reload, and no daily-app restart was performed by the agent.


- [ ] After an authorized Rust rebuild/restart, confirm existing workflow history
  and publication lists still open after the 004 explicit scope-output contract.
  Internal neutral format11 refuses prior internal formats; no live run migration
  or new public dispatch is enabled. Direct scope binding/condition/return tests
  cover source schemas and settled data only, not owner execution or restart
  fences. Rust does not hot-reload; no daily app was restarted by the agent.

## Automatic story backend selection — Rust restart required

- [ ] After an authorized isolated rebuild, inspect the Stories header and details for native, detected Wiz, missing connector, and unapproved connector. Approve the displayed installed connector once, then open a second Wiz repository without another approval. Change the installed entry/version between display and approval and confirm refusal plus refresh messaging; an installation update must require renewed approval.
- [ ] In a disposable Wiz-marked repository, confirm an open native plan retains native access and native records remain unchanged. Check native/automatic override controls and that running/paused executions prevent backend changes.
- [ ] In disposable projects after an authorized isolated rebuild, approve two scope-selected Wiz installations and confirm the second approval preserves the first. Change only the Node version path and confirm an ongoing pinned run keeps its backend identity; changed connector entry/version/installation must still refuse a mid-run switch.
- [ ] Confirm an unmarked native project reports no Wiz markers, while a marked project with open native work explains native retention. With a run started by the previous release still active in a disposable unmarked native-only project, confirm its native stories stay readable; explicit cancellation must remain reachable without resolving its backend. No historical run may be reassigned or restarted automatically.
- [ ] Verify desktop IPC and authenticated browser/remote HTTP show identical provenance and supported controls after project switches. No desktop launch or live-session check was authorized for implementation; use an isolated test instance, never the operator's live sessions.

Rust is not hot-reloaded. Use only an authorized isolated rebuild for these checks;
no daily application restart was performed. Browser fixture evidence does not prove
the desktop or remote runtime path.

## Bounded external story edits — Rust restart required

- [ ] After the next authorized isolated Rust rebuild, use a disposable Wiz
  project to confirm Edit remains available with `update_story:false` plus
  individual title/priority/criterion flags. Save exactly one supported change;
  multiple simultaneous edits must disable Save. Unsupported metadata, existing
  criteria and editor dependencies must stay disabled. Native and aggregate
  legacy editing must retain their current behavior.
- [x] Isolated final-source StoryEditForm render inspected (2026-11-12): disabled
  metadata/dependency/existing-criterion controls and the one-change hint are
  visible without overlap. Evidence is in
  `~/Gits/.tmp/story-backend-auto/bounded-consumer/bounded-edit-final.png`; this
  is a standalone source-importing browser fixture, not a desktop/full-dialog or
  real-Wiz mutation check. No live application was started or restarted.
- [ ] After an authorized Rust rebuild/restart, confirm existing workflow history
  still opens with the additive schema22 neutral effect/resource projection.
  Do not migrate or resume live runs, install a provider or perform a live write
  for this check. Internal callback/approval/cancel/restart behavior uses targeted
  owned-protocol Rust tests; public neutral transports and real native authority
  remain later stories. Rust does not hot-reload; no daily app restart is authorized
  or performed by this implementation.

- [ ] After an authorized Rust rebuild/restart, confirm existing workflow history
  still opens with the optional internal 006 model-tool ownership fields. These
  fields are omitted for existing effects; no public model route, provider install,
  paid model call or live write is enabled by this check. Kernel correlation and
  coarse committed-artifact evidence use targeted owned-protocol tests. Exact
  model requests and scoped host MCP now have recorded protocol coverage; restricted
  coarse execution explicitly reports unavailable before process entry. Restart
  continuation now sends reconciled ledger tool results in a new model prompt;
  the interrupted provider turn closes (Boss A2 option B). Rust does not hot-reload; no daily
  instance was restarted by this implementation.


- [ ] After an authorized Rust rebuild/restart, confirm the native operation catalog
  is installed through the same provider for authoring and execution. An unbound
  neutral run must refuse native dispatch with a caller/run-binding diagnosis;
  publication must not grant commit/push authority. Native commit must refuse a
  missing/nonpassing review or a staged tree different from its reviewed artifact.
  Public starts now bind the backend caller before dispatch; Owner and bound Operator answers retain exact approval checks. Native GitHub publication has
  recorded private sandbox PR8 creation/comment/close and branch deletion evidence;
  every action still needs separate approval. No daily restart was performed.
- [ ] After an authorized Rust rebuild/restart, confirm existing workflow history
  still opens with the staged internal script adapter. No provider is installed
  by 007; no live script write, public neutral transport or Designer availability
  is claimed. Adapter execution/cancellation/replay use targeted owned-process
  tests. Rust does not hot-reload; this task does not restart the daily instance.
- [ ] After a manual Rust rebuild/restart and native neutral-host composition, install a service binding from recorded service evidence. Verify account destination, typed request/output, bounded pagination into ForEach, separate network/write grants and original-key callback reconciliation. No external endpoint/credentials were supplied for 009-e32f; local contract tests do not claim provider compatibility. Rust does not hot-reload; no daily instance was restarted.

- [ ] After an intentional rebuild/restart, exercise the installed GitLab workflow adapter on an authorized scratch project: exact diff version/head/base, selected pipeline SHA, and one general note with artifact/effect receipt. Credentials and live writes were not authorized for 012-1981; local protocol checks do not prove this. Rust changes require a manual `make dev` restart or `make build` to load; do this only when current sessions may close.
## Jira workflow adapter (011-2a30) — Rust, requires a manual restart after composition

- [ ] After an authorized manual rebuild/restart, inspect neutral catalog/preview with installed connector configuration. Verify absent families are omitted, saved HTTP bindings survive reopen, and preview needs no run binding. Public trusted start integration and live configured-service acceptance remain separate; this implementation did not restart the daily app. Rust changes load only after a manual `make dev` restart or `make build` when current sessions may close.

- [ ] After 008/014/015 compose and expose the installed Jira activities, use an explicitly authorized Cloud scratch account/project/type to create from classified issue data, read the canonical ID, update allowed fields, add a comment and take a currently legal transition. Supply tenant-required custom fields. Verify account/site binding, persisted effect label/property and authoritative read-back. No live credentials or write authorization were supplied for the implementation; existing plugin recordings do not prove the adapter. Rust changes load only after Boss manually restarts `make dev` or runs `make build` when ready to lose current sessions.
- [ ] With the same authorized test resources, suppress a create/comment receipt and reconcile the original effect without a second mutation. Observe index lag as uncertainty; verify a unique label/property match (comment properties through POST comment/list), integer/double and expanded multiselect read-back, whitespace-normalized source labels, changed/missing desired update/status state and a naturally encountered 429. Never generate load to force a rate limit.

- [ ] After an authorized Rust rebuild, verify native committed-candidate review retains Findings/Uncertain and its exact tree; empty diff cannot approve. Named failing verification must show a nonzero receipt rather than lose the evidence. No live reviewer call was exercised by implementation tests.

- [ ] After phase B integration and a manual rebuild/restart, inspect a pinned correction metanode through Designer and MCP: selected region, criteria, entries/exits, configured correction limit and consumed count agree; a new head clears old review/check/approval evidence without resetting the count. Rust changes require a manual restart when losing current sessions is acceptable. Phase A supplies internal contracts only; no desktop instance was started (#016-358f).

- [ ] After an authorized rebuild/restart, request `preview_migration` for a throwaway draft through IPC and HTTP: verify identical source pins, configuration diff/Markdown and all preservation blockers; stale revision must refuse and no draft/run/trigger changes may occur. Rust does not hot-reload. This is the preview only; Designer rendering and executable conversion remain blocked (017-70f7).
- [ ] After phase B integration and a manual rebuild/restart, inspect a pinned correction metanode through Designer and MCP: selected region, criteria, entries/exits, configured correction limit and consumed count agree; a new head clears old review/check/approval evidence without resetting the count. Rust changes require a manual restart when losing current sessions is acceptable. Phase B supplies native assessment and MCP authoring; Designer rendering remains pending. No desktop instance was started (#016-358f).
## Neutral GitHub trigger integration (#010-978f)

- [ ] After an operator-chosen Rust rebuild/restart, with the native catalog installed and an authorized throwaway GitHub repository/account, enable two mapped pinned bindings using current overlap acknowledgement, observe one new issue/PR generation selecting both, edit a binding without replay, and verify an explicit owner replay. Backend contract fixtures do not prove live credentials/poll provider behavior. Designer visual demonstration belongs to 014. No restart or live write was performed by 010.

- [ ] After the operator-selected rebuild/restart, enable an opened/labeled issue or PR trigger on a throwaway repository with existing open resources: the first complete poll baselines without runs; later relevant changes admit, and owner replay can explicitly select a retained baseline snapshot. No live account or external writes were authorized for the implementation run.

- [ ] After the operator-selected rebuild/restart, a neutral issue binding on a throwaway registered repository receives issues with legacy intake and old native issue bindings disabled; sibling repositories stay outside intake and no source poller starts when nothing is enabled. Implementation uses loopback fixtures; no live-account write was authorized.

- [ ] After a manual backend rebuild, request neutral `migration_preview` for a current template: verify source revision, installed component pins and explicit missing operations; no converted draft or run should appear (017-70f7).
- [ ] After an authorized Rust rebuild/restart, start a published native workflow from desktop or verified HTTP, confirm its caller/workspace binding exists before dispatch, and check Owner versus current/captured Operator answers. A retry from another caller and an MCP caller without a backend capability binding must refuse. Targeted backend tests cover these contracts; live wrapper parity belongs to the separate transport acceptance. Rust does not hot-reload; no daily instance was restarted.


- [ ] After an authorized rebuild/restart, inspect the installed `agent.role`, `native.agent.report.read` and `native.story.cohort.observe` descriptors. Publish a scratch cohort child through `save_child_dispatch`; a partial cohort must return blocked, and changing a draft must not repin its active run. Check a role report retains its source/artifact and observed session. These Rust changes require manual `make dev` restart or `make build`; do so only when ready to lose the current session. No daily-app restart or external write was used for implementation verification.
- [ ] After loading a rebuilt backend, exercise `native.story.create` with an approved review scope and cap; verify story backend selection, receipt responsibility and refusal on repeated proposal or exhausted cap (021-6e1b).
- [ ] After a manual backend restart, publish an exact-head `native.publication.github` review in the authorized sandbox; check COMMENT, APPROVE and REQUEST_CHANGES using an account permitted to review the PR. Verify a moved head refuses. Rust changes require restart to load (024-c9ee).
- [ ] After an operator-chosen Rust rebuild/restart, run a neutral `native.issue.triage` on a `sstraus/tuic-e2e-sandbox` issue with the configured ego ACP profile: the verdict comes from one mediated ego session bound to the triage effect, a different profile is refused, and a `native.issue.handoff` refuses with the `host.story.subflow` conversion blocker. Fixture tests substitute only the model process.
- [ ] After an operator-chosen Rust rebuild/restart, a neutral issue workflow on `sstraus/tuic-e2e-sandbox` reads its retained source, asks exact approval for one comment/additive label, and refuses an edited/closed issue or changed generation. Confirm an unknown provider write remains uncertain and is never reposted. Full triage/scoped-role/handoff integration requires its dependency contracts. Rust does not hot-reload; no daily instance was restarted.

Native GitHub publication keeps its approved literal HTTPS destination and token through command-scoped identity rewrites, overriding host-wide Git URL rewrites without modifying saved configuration. Competing exact rewrites refuse; artifact, approval and ref lease checks still apply.

- [ ] After an authorized manual Rust rebuild/restart, author a policy correction region with separate maxReviews/maxRepairs, native bounded review and source role report. Inspect independent admission/check/proposal counts; replacement review at exhaustion must reach operator without ACP dispatch. This implementation is not loaded by Rust hot reload.

- [ ] After an authorized manual Rust rebuild/restart, inspect an unanswered typed operator wait, resume its exact pinned edge, and confirm generic effect approval cannot resume or authorize release publication. This implementation performs no live restart.

- [ ] After an authorized manual Rust rebuild/restart, compose a two-story sequential batch graph with `native.story.batch.admit`, pinned `host.story.subflow` children, native push, `native.story.batch.land`, `native.story.batch.pin`, a reviewer `agent.role` critic and `native.story.batch.verdict`. Confirm the second story cannot start before the first lands, a `fix` verdict reopens only its owner as generation 2, and the run completes only after an approving verdict covers both stories (#022-597e).
- [ ] After loading the integrated Rust build, enable a scheduled neutral text/model/condition/local approval workflow in an isolated directory without Git. Confirm one run per occurrence, literal text in the retained model effect, an exact approval question, and completion only after Answer. No daily app restart was performed for this change.

- [ ] After an authorized manual Rust rebuild/restart, run an implementer `agent.role` with `native.workspace.stage` as its only tool on a throwaway repository. Confirm it stages exactly the files it names, its report carries the staged tree, `native.review.change` reviews that tree and `native.commit.create` commits it after approval; a path outside the workspace or through a symlink is refused (#2960-d562).

- [ ] After a manual backend rebuild/restart, inspect an inactive converted resolve_plan with explicit role executions: eight-member sequential batches, one exact combined-artifact verdict, and an eight-round limit. A FIX must dispatch only its owner at its current story revision, without changing the full retained admission cohort. Inspect each role's retained read-only snapshot for the pinned plan document, actual criteria/member data, artifact diff and host-derived review subject/gate digest; confirm binary summaries remain visible and oversized diffs carry an explicit truncation marker and bounded stat; malformed or oversized snapshot parts refuse without widening tools. Do not activate a real run before separate 019 cutover approval (#2962-554e).

- [ ] After an authorized manual Rust rebuild/restart, inspect an inactive seeded issue_delivery conversion with explicit role executions and publication coordinates. Confirm accepted triage leads through visible preparation, owned story creation, pinned child return, artifact pin, push, draft PR and developer-ready approval. A blocked child or declined/stopped-uncertain publication must lead only to its exact status node, not another publication. Confirm the persisted issue problem retains the source URL within its character cap. Do not transfer or enable live triggers without separate 019 approval (#2962-554e).
