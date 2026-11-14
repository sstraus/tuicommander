# Scoped roles and pinned children: targeted evidence

Stories: 020-6471 and 022-597e. Integrated base: 577b3172fc.

## Compatible results

- rb task `20261111T215027Z-1380`, commit `2d031ae4c6752f13a627ad289e103258d0c6a2fb`, tree `32b342ac307d8afb9be0b909887570980adf2a0d`: 27 passed, one cohort fixture failed before publication because an inherited generic condition referenced `0.next`.
- rb task `20261111T220014Z-8989`, commit `49edb6614b64828605a345f172d624258f7578f4`, tree `29641f13b0e74945bc32ba4c73fb23e590655fdd`: both cohort tests passed. Only cohort test support changed; the other 27 passing results remain compatible.
- Earlier task `20261111T214657Z-65786` failed compilation on a test-only BTreeMap mutation. No tests executed. The corrected test compiled in both subsequent tasks.
- Both executed tasks reported `RB_BOX_DIRTY=no`. Local `mbx doctor`: zero failures. Local Rust/Cargo 1.99.1; rb toolchain path identifies 1.99.1. Nextest and remote mbx versions were not measured.

The aggregate command, executed through the remote build slot:

```sh
PATH=/opt/homebrew/bin:$PATH TMPDIR=$HOME/Gits/.tmp/tuic-tests/ \
  /Users/stefano.straus/Gits/personal/orchestrator/tools/build/build-slot.sh --remote \
  scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib \
  -E 'test(native_agent_) | test(native_cohort_) | test(workflows::run::neutral_model_adapter_tests::) | test(workflows::run::native_tests::) | test(workflows::run::native_story_tests::) | test(subflow_frames_replay_same_owner_and_readmit_typed_child_arguments) | test(cancelled_pending_effect_keeps_claim_until_proven_stop_without_reopening_graph) | test(current_admission_refuses_and_invalid_effect_result_retains_uncertainty) | test(workflows::reusable_operation_tests::)'
```

The retry used the same wrapper and `-E 'test(workflows::run::native_cohort_tests::)'`.

## Behaviors protected

The new role cases catch profile/prompt drift, tool-bearing independent review,
foreign/stale reports, model-supplied ownership, session rebinding, coarse launcher
fallback, unapproved story writes, duplicate tool writes and revoked inherited
rights. The installed host executes the real native story service against a
throwaway project and the mediated ACP process against a deterministic protocol
fixture. The fixture substitutes the model provider only.

The cohort cases execute the saved operation through native admission and the
existing run actor, with real selected-backend stories. They catch partial cohorts
reported complete, latest publication substitution, duplicate start requests,
foreign scope, stale plan/story pins, and constant/model completion receipts.
Existing targeted cases protect subflow replay, current admission, inherited
scope, uncertain effects, cancellation claims and reusable-operation integrity.
One unrelated agent lifecycle test matched the broad `native_agent_` filter and
passed; it adds no evidence for these stories.

Logs and exact source hashes are retained under
`~/Gits/.tmp/workflow-rework/020-agent/`; task handles and results are under
`~/Gits/.tmp/rb/<task-id>/`. No full suite, clippy or mutation run was performed.

## Limits and integration obligations

- No live model-provider result or legacy template conversion is claimed. Source
  origin/generation must be verified by the composing domain consumer.
- The issue-intake peer received the types and report reader seam. Its separate
  IssueServices port has no production binding to these operations; integration
  remains required, without introducing another launcher or scheduler.
- Cohorts preserve ordered selection and exact child frames. `native.story.cohort.observe`
  still reports the batch obligations as `not_attested`/`blocked`: only a graph that
  composes the sequential batch operations below attests them.
- The integrator must regenerate the shared catalog fixtures once. This change
  adds `agent.role@1` / `host.agent.role@1`,
  `native.agent.report.read@1`, `native.story.cohort.observe@1`, and the reusable
  `host.story.subflow@1` handler plus `save_child_dispatch` action. Existing fixture
  changes inherited from 577b3172fc were preserved; no fixture was regenerated here.
- Rust changes require an authorized manual restart/build to load. No running
  application was restarted. Follow-up checks are in `to-test/workflows-stories-4.md`.

## Sequential batch operations (022 AC5)

Four native operations keep the legacy sequential batch as retained data and joins
over owned children. They are not a scheduler. One ledger per (run, plan) lives in
the run store (`run/native_batch.rs`). Evidence is verified first in
`run/native_batch_evidence.rs`.

| Operation | Request | Outcomes | Rule |
|---|---|---|---|
| `native.story.batch.admit` | `{cohort, story}` | `ok` | Retains the cohort on the first call; later calls must match it. Admits the first cohort member that has no admission, or the armed FIX owner. Refuses while another admitted member has not landed (one slot) and refuses a second start. |
| `native.story.batch.land` | `{plan, story}` | `ok` | The host selects the landing from the run ledger: the latest settled `native.publication.push` or `native.commit.create` of this run that was not known at the member's admission and is not credited to another member (decision G1). The caller supplies no key. |
| `native.story.batch.pin` | `{plan}` | `ok` | Needs a free slot and no armed FIX. Pins exactly the landed, unbatched members at the clean workspace commit/tree; every landing commit must be an ancestor. Generations are consecutive. |
| `native.story.batch.verdict` | `{plan, generation, report, source}` | `completed`, `continue`, `fix` | One verdict per latest pin, read through `read_report` from a reviewer or validator `agent.role` effect of this run. Its source commit/tree must equal the pin. The report body is `{decision: approve\|fix, story?}`. `fix` names one member of that batch and arms its next generation. An approval is `completed` only when every cohort story is in a batch; otherwise it is `continue`. |

Each effect retains its receipt by digest in the same transaction as its ledger
write. A repeat delivery returns that receipt and records nothing. Recovery returns
the retained receipt, or refuses when none exists; it never repeats a write.

Limits: a landing is a native push in the same run; the legacy integration saga is
not used. The legacy independent proof cohort is out of scope. A graph that
dispatches children without these operations gets no barrier attestation. No 017
conversion uses the operations yet (story_delivery is not converted).

Evidence: RED rb `20261112T020221Z-79672` (implementation stubbed, 0/4); GREEN rb
`20261112T020708Z-17933` (4/4 plus catalog capture, 41 -> 45 descriptors).

## Run-time cohorts and covered story steps (2026-11-12)

- `save_child_dispatch` without `cohort` registers a run-time cohort dispatch. The host admits the cohort input only when every member has a settled `native.story.create` receipt in the same run, at its current revision, of the same plan and source (decision a).
- `native.story.cohort.created {plan, review}` returns the cohort of stories this run created under `review`, in creation order (activation, then attempt). It reads the run ledger, and reads (never writes) the plan revision and source from the selected backend (decision B(a)).
- `native.story.read {story}` returns `{story, revision, criteria, status}` (criteria is a count) for an unarchived story of the run's project, and refuses any other story (decision G2). It never writes.
- `native.story.batch.land` also accepts a `native.commit.create` receipt of the same run made after the member's admission (decision 1(i)).
- `native.story.transition` asks no question in two cases (decisions A(a) and 2(i)):
  - `StartManual` of a story this run created and admitted to its batch slot.
  - A later step after this run holds a passing `native.correction.policy.assess` (explicit `repeat: false`) for exactly that story on the committed artifact at the clean HEAD. `CorrectionState.approval` is never set in production, so `repeat: false` is the verdict.

  Any other transition keeps its exact approval.

