# Worktree removal identity — 2334-ca53

## Observed defect

The sidebar uses `GitReads.worktree_inventory`. Its gix implementation reads the
literal branch name from each checkout's admin HEAD. Removal previously parsed
`git worktree list --porcelain -z` independently. Git follows symbolic branch
refs in that output. For a checkout whose HEAD names `feature`, when
`refs/heads/feature` is symbolic to another branch, the sidebar lists `feature`
but removal cannot resolve it.

This reproduces the reported error class. It does **not** prove that Roberto's
checkout has a symbolic branch: we cannot access his machine.

## State map

Recent history includes fixes 2147, 2156, 2157, 2160 and 2310. The investigation
therefore traced discovery, operation recovery, lifecycle inspection, removal,
missing registrations and pooled checkouts before changing production code.

Real Git-created scratch states in `sidebar_removal_identity_2334_candidate_states`:

| State | Sidebar inventory | Pre-fix removal resolution |
| --- | --- | --- |
| Ordinary checkout; directory differs from branch | Branch row | Same checkout |
| Explicit detached HEAD | Read-only detached row | Not a branch workspace |
| Branch renamed inside checkout | New branch row | Same checkout under new id |
| Branch ref deleted with `update-ref -d` | Literal attached HEAD row | Same checkout; subsequent missing-ref safety remains separate |
| Checkout switched to another branch | Current branch row | Same checkout under current id |
| Locked registration | Branch row | Same checkout; lock confirmation remains separate |
| Missing/prunable registration | Repair diagnostic | Not a selectable row; explicit missing-checkout removal remains separate |
| Linked checkout of bare repository | Branch row | Same linked checkout |
| Input path is a linked checkout | Branch row | Same checkout |
| Symbolic branch ref | Literal HEAD branch row | **No workspace found for id** |

`sidebar_removal_identity_2334_symlinked_base` also proves identical resolution
through a symlinked base on Unix. Existing real-Git discovery regressions exercised
forced duplicate checkouts (one deterministic row plus a diagnostic), bisect,
relative-gitdir interrupted rebase, and missing-registration repair guidance.
No claim is made that stale frontend rows survive an external branch rename:
refresh supplies the current id; the old id intentionally does not retarget.

## Change

Fresh workspace lookup now consumes the same discovery inventory as the sidebar.
The separate archived lookup and separate `resolve_workspace` porcelain lookup
are removed. Live-only callers still exclude pooled entries; removal still
supports missing registrations and pooled checkouts. Shared deterministic ordering
prefers a live checkout over an archived duplicate, so reading inventory without
the configured pool filter cannot target the hidden pooled copy.

A symbolic ref is an alias, not its target. Both branch-deletion paths now use
no-dereference semantics so deleting an alias cannot delete the target branch.
The remote-containment path still verifies and locks its containing remote ref
in the same transaction.

## Verification

- Pre-fix commit: `af7fc19785d1785c571eafaf5aaede5dbb825057`.
- RED on rb: candidate matrix failed only `symbolic_branch`; symlink test passed.
  Log: `~/Gits/.tmp/tuic-2334-evidence/red.log`.
- First GREEN attempt found a missing Rust import; no tests executed. Corrected
  import and reran the same targeted scope: nine passed.
- Final rb scope after covering the second deletion path: fourteen tests passed,
  including local and remotely contained symbolic aliases, missing/duplicate
  inventory, rebase/bisect, unknown-repository errors and remote containment.
  Log: `~/Gits/.tmp/tuic-2334-evidence/green-final.log` (3.98 s build, 0.252 s tests).
- Native reality: `make agent-test AGENT_TEST_INSTANCE=agent-test-2334
  AGENT_TEST_PORT=19834`, after the dry run. Registered only a scratch repository
  through that instance's Unix-socket HTTP API. Opened the sidebar row, clicked
  **Clean up**, verified the fresh dirty-file confirmation, then clicked
  **Delete anyway**. Checkout and alias disappeared; `main` remained at its tip.
  Only window 21875 was captured. Evidence includes `sidebar-listed.png`,
  `removal-dirty-confirm.png`, `sidebar-after.png`, `agent-test.log` and
  `reality/git-state-{before,after}.txt` under the evidence directory above.
- The desktop replay predates the final remote-containment-only transaction
  change. That variant has rb regression evidence, not native UI evidence.
- Stopped with `make agent-test-stop` for the same instance and port. Supervisor
  reports signal 143 / make exit 2 after this requested stop, not a runtime crash.
- No frontend source changed; no Vitest, TypeScript, full suite, mutation or
  Windows-native run was performed. Batch integration and independent critique
  remain the coordinator's responsibility. Risk is high: destructive Git state.

A final real-Git pooled-duplicate test failed against the first local fix commit
`c2b0600d3`: unfiltered removal chose the pooled checkout while configured sidebar
discovery chose the live checkout. Shared inventory ordering now prefers the
non-archived checkout before comparing paths. This is a regression found and
fixed before handoff, not a state established on the colleague machine. The final
expanded run is recorded in `green-pooled-duplicate.log`. The native replay above
also predates this duplicate-only ordering change.

## Colleague confirmation

Run this single read-only Git command. A symbolic target confirms the reproduced
state; a fatal "not a symbolic ref" means this diagnosis does not establish his
case and the raw checkout inventory is still needed.

```sh
git -C /Users/roberto.serafin/work/agent2 symbolic-ref --no-recurse refs/heads/POC-00001-slack-flap
```

## Unrelated observations

The isolated instance logged `notification.is_permission_granted not allowed`
and `Command get_dictation_config not found` during boot. Neither prevented this
removal replay. They were not changed in this story.

## EGO-DEFECTS

None observed that can be attributed to ego from a quoted screen line.
