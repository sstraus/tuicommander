# Neutral source trigger admission

Bindings select an exact neutral publication without a WorkflowKind restriction. GitHub repository prefixes save as `github:<lowercase owner/repository>/`, including the boundary slash; malformed or deeper prefixes are rejected.
Each has a stable id, source/kind, resource prefix, enabled revision, selected
events, typed input/subject mapping, optional value filters and ignored actors.
Several matching bindings each run independently. Changes save disabled;
activation requires valid mappings and acknowledgement of every current overlap
by id and revision. Overlap is conservative across intersecting resource prefixes
and events. Scalars compare by equality; an array field matches a required member.

`TriggerSource` and `trigger_schema(source, kind)` are a closed installed-source
match. The schema supplied with an event must equal that installed schema.
Graph/client JSON supplies neither a catalog nor replay authority. The 015 single
neutral action exposes schema, binding CRUD/activation, mapping preview and replay;
014 consumes these contracts for the Designer. The backend preview returns typed
workflow inputs/subject and overlap revisions. No separate trigger transport exists.

WorkflowStore owns binding/publication metadata. RuntimeOwner owns admission and
starts through the existing neutral run ledger. RunStore retains normalized
source state, immutable event/start intents, binding receipts and active/pending
resource identities. Deduplication is binding + resource + source generation;
delivery id and workflow revision do not mint another event. A crash between
intent and run linking retries the exact retained request and publication.
Explicit replay requires authenticated owner authority for the same project,
uses the retained snapshot/start request and deduplicates its explicit replay id.

One execution per binding/resource remains active. A newer relevant generation
cancels the superseded graph frame, retains original effects for reconciliation,
and coalesces pending work to the latest snapshot. No replacement starts while
any dispatched owned effect lacks completion and stop evidence. The existing
actor/effect service remains the only executor; no trigger worker can replay an
uncertain operation. Service adapters still check current subject and grants:
GitHub review publication probes exact head/base and posts with `commit_id`;
lost-response recovery only looks up the original receipt.

GitHub uses the existing poller and account-aware PrApi/RepoApi client. Neutral
subscriptions are independent of legacy subscriptions. The consumer re-reads
full open-list payloads, including exact PR head/base SHAs. Individual GETs are limited to previously known open resources absent from the complete list. Failed individual reads warn without blocking later resources. Confirmed 404/410 or a transferred response with a different resource number retires that polling candidate; timeouts, 5xx, rate limits and other ambiguous errors preserve it for retry. Retained source generations remain intact. Complete open issue/PR discovery
is bounded to ten pages of 100; excess or malformed data refuses admission.
Known closed resources leave future poll candidates. A missing installed native
catalog produces a diagnostic refusal. No public webhook endpoint is added.

Issue generations follow relevant title/body material, lifecycle and explicit
label admission epochs. Comment counts and observed timestamps alone do not
start work. PR generations follow lifecycle and exact head/base, not CI-only
updates. Polls do not prove who caused an update; actor stays absent. Configured
actor filters are optional. Self-event suppression requires the exact binding's
own label-write claim and exact normalized resource/generation;
a shared bot account alone suppresses nothing. A native issue label write
claims the next generation of its admitted issue before the POST; the poll that
sees only the claimed labels added records the self event for that binding.
Any other change in the same generation is not suppressed.

The documented fixtures and version notes live in
[src-tauri/src/workflows/fixtures](../../src-tauri/src/workflows/fixtures/README.md).
They are API-contract evidence, not live account validation. Story 018 separately
records authorized sandbox source/connector checks and their failures; a fixture
pass cannot substitute for that evidence. After integration a manual `make dev`
restart or `make build` is required to load Rust changes; no restart was performed.
Native provider installation and the Designer visual consumer retain their
respective owner evidence requirements.

GitHub enablement establishes an activation cut on the next complete source poll.
Until that baseline completes, automatic and manual source admission is held.
The existing RunStore retains each current resource/generation and its typed
pinned start request with baseline disposition, without queuing an execution;
only then is the binding's enabled revision marked ready. Empty repositories
also complete this cut. Each later new or re-enabled binding establishes its own
cut. Resources created or changed before this first complete poll are baseline.
Later relevant generations admit normally. Baseline intents remain available to
the authenticated-owner replay path, including a resource whose present action
is not among automatic event selectors. This does not grant automatic replay.

Unchanged source observations emit no new normalized event. The durable source
outbox retains its current event for recovery if a crash separates normalization
from admission. Binding receipts distinguish a captured baseline from later
pending/started work; retrying baseline capture cannot enqueue historical work.
The activation marker is persisted last, after all baseline intents, so an
incomplete capture remains inactive across a restart.

An enabled neutral GitHub issue binding explicitly requests issue intake for its
named repository, alongside existing legacy and native issue sources. No legacy
source configuration is required for a neutral binding. Poll batches group by
account and requested intake label, so an enabled binding cannot fetch intake
issues from a sibling or unrelated repository. With no enabled binding or source
setting, no issue intake subscription/fetch is created. Independent PR/sidebar
requests continue to use their own existing polling paths.

Activation/listing may expose optional `refusal_reason` for a real source
configuration failure. An absent legacy source is not a refusal. This diagnostic
neither changes requested enablement nor writes intake configuration.

## Scheduled neutral starts

A binding with `source: "scheduled"`, `kind: "schedule"`, resource
`"schedule:<binding id>"` and event `["due"]` carries
`schedule: {"cron":"0 9 * * *","timezone":"UTC","text":"Review the summary"}`.
`text` (string) and `occurrence_ms` (integer) are its installed mapping schema.
The existing automation runtime supplies the same 30-second tick and timezone-aware
schedule parser; there is no additional timer or legacy Story/check dependency.
A wake considers the latest due cron occurrence, rather than replaying missed
intermediate occurrences. The enabled binding revision and UTC occurrence form
its generation, retained with the exact publication/start request in the ordinary
trigger ledger. Retries reuse the occurrence's run. A different binding uses an
exact schedule resource match, never a neighboring id's prefix.

Backend source admission captures Owner provenance and binds it before waking
an actor. Text/model, HTTP and local approval workflows can use a plain directory;
native repository operations still require the captured exact Git workspace.
`model.request@1` accepts optional literal `text`, appended to its configured
request context and checked against the same byte bound. `inbox.approval@1` takes
bounded nonempty `text` and optional `details` (shown below the text, cut at
8 KiB) and waits in the existing retained effect inbox. The
existing exact Answer action resolves it; an affirmative answer returns
`approved: true`. No Telegram account or separate approval transport is involved.

## GitHub pull request review

The built-in `github-pr-review` template (`templates` action,
`src-tauri/src/workflows/templates/github-pr-review.json`) is built only from
catalog activities. Set the model profile, model and effort of both model
activities in the Designer, publish, then enable a `pull_request` binding for
the repository (events `opened`, `reopened`, `updated`) that maps `repository`,
`number`, `head` and `base`.

The graph is: `github/pull_request.snapshot@1` captures the open PR at the exact
head/base and the base...head patch (outcome `stale` when the PR moved or closed);
a fork runs a review model and a verification model in parallel; a condition
checks `passed`; a failed verification asks the operator through
`inbox.approval@1`, with the verification `summary` as the question details; `github/pull_request.review@1` publishes one review.

The review operation reads the PR again and returns `stale` without a write when
the head, base or state changed. It posts with `commit_id` set to the captured
head and reads the review back. The body carries a `tuic-effect` marker with the
effect digest; reconciliation looks up that marker and never posts again. A
complete review list without the marker is a definite failure. The project
path selects the account and repository through the existing PR client; a
workflow repository that differs from the project repository fails before any
request. The review asks no extra question: the graph decides when the operator
answers.

Overlap, deduplication and self-event filtering come from the ordinary trigger
ledger above: a newer head supersedes the active run, a repeated poll of the
same head is the same generation, and a published review changes neither head,
base nor state, so it starts no run.
