# WF01 preservation baseline and shared contract

Baseline date: 2026-10-30. Story: 2508-3a15. This is a contract and source
baseline, not an operational migration or a certificate of installed health.
The approved implementation plan, section 2, remains the authority for later
stories. This baseline does not activate a policy, scheduler, subscription,
integration writer or provider action.

## Frozen inputs

- [Source pins and native test inventory](sources.json) records exact SHA-256,
  byte length and observed revision for the feature report, study register,
  approved plan, global policies, repository policies and current native workflow
  modules. Test names are source inventory, not a claim that they passed today.
- [Feature report snapshot](feature-report.txt) retains the original report,
  including its source-health qualifications and I1–I10. Its declared source
  revision differs from the later observed repository revision; both are retained.
- [Preservation matrix](capabilities.json) binds every one of the report's 105
  capability IDs and all 42 screenshot feature IDs to the approved destination
  and WF validation owner. Every row starts `required_not_operationally_verified`.
  WF17 must add operational evidence before retirement; static coverage is not
  parity. Optional nodes cannot retire their retained deterministic tools.
- [Recorded native event streams](../../../src-tauri/src/fixtures/workflow_contracts/README.md)
  come from the pre-change RunStore, not handwritten serialized model fixtures.

Source hashes pin observed bytes. A source path is not an import grant. Host
policies have an observation date rather than a fabricated Git revision. Their
bytes are retained in `host-policies.txt`. A later source change requires a new
baseline/import revision and an explicit comparison, not replacement of history.

## Event upgrade and retained replay

Run-store layout 5 admits event contracts through version 3 and causes older
binaries to refuse the database. This is a compatibility marker, not conversion
of old events. Opening a database does not rewrite a run's event contract.

| Contract | Retained semantics |
| --- | --- |
| 0 / absent field | Legacy snapshots remain readable and cancelable; no invented graph position or resume policy. |
| 1 | Existing record-only command behavior remains; graph events remain refused. |
| 2 | Retained serial graph transitions, exclusive Join, run-wide NeedsInput and explicit resume. New children of an old run remain contract 2. |
| 3 | The same serial semantics plus a graph-start `contract` pin. No Fork, all-Join, token ledger, Call or branch-local Ask is activated by WF01. |
| Newer / unsupported | Refuse start/replay/mutation rather than guess semantics. |

The immutable graph-start event stores the whole `PublishedWorkflow`. Contract 3
also stores `{version:3,definitionSha256,policyImports}`. The definition hash is
SHA-256 over serde JSON bytes of the typed publication, including project, ID,
revision, graph, prompts, closure and required checks. It is not a portable
cross-language canonical-JSON hash; producers and replay use the Rust codec.
The reducer verifies the pin at graph start and dispatches old serial transitions
through the retained implementation. Future token semantics must use a distinct
version branch, not reinterpret `Join {}`.

A new draft/publication cannot change an active run's publication or check policy.
There is no active-run upgrade API in WF01. A future migration must name run ID,
expected sequence, old/new contract, old/new definition and policy digests,
identity/receipt mapping, unsettled effects and stop/reconciliation proof. It must
commit an explicit migration receipt before new semantics can execute. Draft save,
publish, database open and daemon recovery are never implicit migration requests.
Legacy work drains under its original owner. Unknown effects stay unknown.

## Policy-import boundary

`PolicyImport` has exactly `domain`, `source`, `revision` and `sha256`.
Domains are `global_routing`, `repository_gates` and `publication_authority`.
Import source bytes/revisions, not mutable defaults or credentials.

1. Global routing comes only from the named global orchestration authority.
   Repository templates and spawn wrappers do not override it. Missing exact
   approved model/profile is a visible block in the future imported-policy lane.
2. Repository gates add the applicable project requirements: native versus rb
   proof, target/config/test scope, build admission/freeze, one mutation process,
   canonical Git writer, GUI/device isolation and owned-process cleanup.
   Conflicts need a named Boss exception; do not select the cheaper gate.
3. Publication authority is separate from permission to run a workflow, integrate
   or mark proof complete. It binds exact repository and actions. The approved
   unattended-PR creation repository list is `[]`; no repository is inferred from
   a local path. Push, merge, release and ready actions remain distinct grants.

WF01 new starts pin the existing native graph/check/closure policy. Their
`policyImports` is explicitly empty: global policy was **not** loaded or enforced
as a new runtime import. WF09 resolves/pins the complete transitive publication
and policy set; WF04/WF16A enforce effective launch/routing imports. This boundary
avoids claiming that today's default-profile fallback already meets the future
exact-model contract. Importing source inventory is not permission to activate it.
Built-in updates offer a new draft/diff; customized definitions and active runs
are not overwritten. WF13/WF14 perform versioned, expected-revision settings
imports, disabled first; only WF17 can transfer a consumer after parity/approval.

## Effect and receipt schemas

`run/contracts.rs` defines the active graph/policy pin shapes. The standalone
[execution v1 JSON Schema](execution-v1.schema.json) defines typed adapter intents
and receipts without adding unused runtime effect types. It is a contract for
later adapters, not a new callable effect. Existing
`EffectIntent`, `RunReceipt`, `CheckReceipt`, `IntegrationReceipt` and
`CanonicalReceipt` keep their existing wire formats and state machines.

- `EffectIdentity`: run, execution, activation, attempt, generation and effect IDs.
- `ExecutionSubject`: either an artifact ID/canonical repository key/commit/tree
  and requirements digest, or an external provider/resource ID/expected revision
  and exact payload/requirements digests. Workspace identity is separate from a
  machine path. Both intents and receipts identify their owning daemon.
- `ExecutionEffect`: schema version, identity, kind, payload-bound idempotency key,
  subject/workspace, explicit argv or prompt/profile or typed Tool payload digest,
  target requirements, resource keys, deadline, secret references, output schema
  and exact policy imports. There is no implicit shell; an explicit shell must be
  a visible, versioned command contract. No secret values enter the ledger.
- `ExecutionReceipt`: schema version and matching identity/payload/subject,
  executor identity/fence, exact target/toolchain, command/config/test-scope digest,
  logs/artifact digests, succeeded/failed/cancelled/unknown outcome, stop proof,
  matched-test count and usage receipt references. Missing usage is unknown, not
  zero. A Linux receipt cannot satisfy a native Mac proof requirement.

Adapters must validate supported schema versions and full identity/payload before
execution or acceptance. Intent commits before any external effect. Same key and
same payload returns the original receipt; a different payload fails. Lost replies
use lookup, never blind retry. A terminal report is evidence, not acceptance.
Cancellation requested differs from observed stop; expiry does not release a
shared resource or authorize cleanup/replacement. Disconnection is unknown.
Only the owning daemon selects successor edges. ACP identity is not a PTY.

Evidence acceptance additionally binds criterion/requirement revision, pre-fix
commit for RED, clean source commit/tree, merge base/result, target/toolchain,
command/config/test scope, independent reviewer and freshness inputs. Preserve
REALITY/SYMPTOM/TEST-CHANGE and omissions at the story boundary. Nonempty intended
scope with zero matched tests cannot pass. Integration, proof completion and
release readiness are separate states. A pinned cohort cannot clear later debt;
mutation reuse needs the entire requested universe and compatible evidence.
These are WF05–WF08 validation obligations, not invented WF01 successful receipts.

## Source-conflict decisions: I1–I10

These decisions select the import contract. They do not edit or retire sibling
scripts, move live reports, install hooks, create backups or run a scheduler.

| Issue | Resolution for this baseline | Later verification owner |
| --- | --- | --- |
| I1 stale critic text | Select `critic-brief.sh` and its active `critic-brief.template.md`, actual five-argument signature and FIX/STORY/ACCEPT policy. The unused `briefs/critic.md` is historical, not a merged instruction source. Keep strict required gates and no generic Proof filling. | WF06/WF16A audit callers before retirement. |
| I2 stale digest paths | Persistent bilingual repository reports are authoritative. Temp reports are not mobile delivery. Preserve/apply ticks and comments before any move; no live move here. | WF16C report/acknowledgment migration. |
| I3 unsafe cleanup/landing skill | TUIC worktree tracking, review, child-close ownership, integration/stop proof and preservation of unattributed dirt win over raw Git deletion/direct low-risk landing. No destructive action from the stale skill. | WF04/WF07/WF16A. |
| I4 competing routing/gates | Global ORCHESTRATION is the sole routing authority. Exact task authority and repository gates apply; no cheaper fallback or blended template. This task explicitly requires the two RED tests and same-call strict clippy. The older deferred-GREEN text is not a model/profile or gate override. | WF04/WF06/WF16A import/effective-policy comparison. |
| I5 combined countdown | Keep per-release counts, release work/Boss waits/next separation and one zero notification. The aggregate `all:` line is not imported as required presentation. | WF15/WF16C. |
| I6 mutation docs drift | Pin the current producer, fixture and e2e source; do not preserve a stale claim that they are absent. No live measurement pass and no deletion-only reuse bridge is inferred. | WF08/WF16A compatible evidence checks. |
| I7 compatibility helpers | Retain helpers until their target product behavior and actual callers are checked. An old replacement story number is not removal evidence. | WF16A and WF17 retirement gate. |
| I8 unequal CLI hooks | Record Claude/Codex source wiring separately; effective global/runtime and ego coverage are unknown. Unwired no-pkill/no-zsh-path source is not universal enforcement. | WF16A effective per-harness audit. |
| I9 installed health | Hook source, launchd/systemd files, model names and status-line settings do not establish installed/loaded/available health. Record unknown until read-only runtime/last-run evidence exists. | WF16A/WF16B; WF17 blocks relevant parity claims. |
| I10 registry sole copy | Preserve the existing registry, never replace it with historical tested rows. WF08 owns naming an operational backup/restore owner and SQLite-consistent backup procedure; owner/last backup remain unknown now. No sole-copy reliance or destructive cutover until verified. | WF08 backup/restore evidence before WF17. |

## Verification record and limits

The two named RED tests failed on pre-change commit
`0fd8fb21e23ac31a949ce560c6d9b4fff88f4e86` with test additions only:
`old_exclusive_join_replays_without_parallel_wait` reported version 2 instead of
3; `draft_publish_does_not_repin_active_run` reported missing contract metadata.
Both ran (zero matched tests was not treated as success). Their old behavior
assertions passed. Same-call `cargo clippy -p tuicommander --all-targets -- -D
warnings` passed on rb. The log and captured fixture provenance are recorded in
the story worklog and fixture README. No Mac-native proof is claimed by that run.

The targeted final run covers workflow graph replay/publication consumers and
existing HTTP/service start parity. API commands/routes remain unchanged; the
snapshot gains an optional graph `contract` field. Old fixture JSON has no field
and round-trips without adding one. No JavaScript scheduler or transport-specific
policy is added. Full suites, mutation, coverage, live installation audit,
provider actions and operational migration belong to their later owners.

Final targeted evidence: 62 tests passed on rb, followed by strict all-targets
clippy with zero warnings. Log: `/Users/stefano.straus/Gits/.tmp/wf01-final-fixed.log`.
The earlier final attempt passed the same 62 tests but failed clippy on unused
future Rust schema types and one assertion lint. Moving unimplemented adapter
shapes to the standalone schema and correcting the assertion resolved those
failures; the repeated affected run passed. JSON syntax/local schema references,
source hashes and complete matrix IDs passed static checks. A JSON Schema
meta-schema validator was not installed, so no such validation pass is claimed.
