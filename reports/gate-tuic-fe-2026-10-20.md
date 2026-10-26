# Frontend gate evidence — 2026-10-20

Status: in progress. This report does not close a story or assert that any gate converged.

Source revision: `a778245a871c15e65b83061d88a4ff8bfdf60cfc` (current main when the gate branch was created). Mutation scope is each named complete current source file, not the historical ranges in the queue.

All frontend gates run locally through `build-slot.sh --gate` and `scripts/with-test-tmp.sh`, one at a time. No Rust or desktop checks run.

## Initial observations

- `public/sw.js`: 8 tests passed. V8 reports 0% coverage for the dynamically evaluated worker source. Initial Stryker sandbox excluded `public/`; an evidence-only config adds `!public/**/*`. The corrected run found 215 mutants: 41 killed, 172 survived, 2 timeout. The raw score is 20%, but the push-test VM does not expose `process`, so Stryker cannot activate push mutants in that VM. Do not interpret those survivors as proven missing tests.
- `useAppInit.ts`, `mdTabs.ts`: coverage and Stryker exited 1. Detailed logs were lost when the unbound worktree was removed during the batch. No convergence claim.
- `ContentRenderer.tsx`: 773 mutants: 394 killed, 373 survived, 6 timeout. Stryker exited 0; coverage exited 1. Detailed survivor evidence was lost with the removed worktree. Exit 0 is not a gate pass.
- `tweakComments.ts`: 798 mutants: 755 killed, 41 survived, 2 timeout (raw score 94.86%). Coverage exited 0. Mutation exited 1 during worktree loss; recovered JSON records the counts and survivors. Evidence is retained under `~/Gits/.tmp/gate-tuic-fe-recovered/1062/`.
- Editor zoom: 91 tests passed; coverage was below the repository 80% thresholds: statements 61.75%, branches 54.97%, functions 55.85%, lines 66.25%. Mutation has not run yet.

## Execution interruption

The first linked worktree was automatically archived and then removed while the batch was running. A second creation with `spawn_session=true` was also archived. The source branch remains at the recorded revision. Durable run output is now stored outside the worktree under `~/Gits/.tmp/` so that another path lifecycle change cannot erase it.

Production code and tests are unchanged. Remaining gates are still pending.
