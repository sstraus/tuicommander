# Windows CI failure investigation (2310-6e76)

## Scope and evidence

Run 40134469538, commit `ff47f4a6788d675734b539ac331eae90a3ec1a39`, Windows-x64. Sources: `windows-failures.md`, `windows.log`, and `windows-annotations.json` under `~/Gits/.tmp/results/ci-ff47/`; current source was inspected against the failing commit. No native Windows runtime is available. Classification is not a claim that Windows GREEN has been observed.

## Classification

Test names below omit their common module prefixes where the source column supplies them. F = fixture defect; P = product defect; U = unresolved classification. A counter or latency failure can establish a violated performance contract without establishing its cause.

| # | Test | Class | Root cause / evidence and action |
|---|---|---|---|
| 1 | `archive_refuses_an_allocation_marker_that_is_a_directory` | F | `tuic-git/worktree.rs`: Git inventory returns `D:/...`; oracle used native `D:\...`. Use `portable_spelling` for the expected path. |
| 2 | `archive_journal_failure_restores_the_original_checkout` | F | Same inventory oracle mismatch; keep exact restored-checkout assertion. |
| 3 | `orphan_whose_directory_is_gone_stays_out_of_fingerprint_cleanup` | F | Both membership assertions compared portable diagnostic paths with a native fixture path. Normalize both oracles, including the negative assertion. |
| 4 | `recycling_finished_checkout_keeps_artifacts_and_removes_stale_paths` | F | Recycled provenance path is read from portable Git inventory, while archive return contains native joined separators. Normalize the archive oracle; retain artifact and stale-path checks. |
| 5 | `recycling_journal_failure_restores_the_original_checkout` | F | Same restored-inventory path oracle mismatch. |
| 6 | `removal_keeps_local_branch_when_cleanup_prunes_its_remote_proof` | U, intermittent native Git subprocess fault | Run 40193236753 on `d3404c764` fails all three attempts. Failure probes restore the stale ref successfully, then both direct `C:\Program Files\Git\ucrt64\bin\git.exe fetch --prune origin` and the original `cmd /C` form exit `3221225477` (`0xC0000005`) with empty streams under the same captured PATH/cwd (lines 5104/5126/5149). The unchanged owner passed in run 40179831695 in 11.812s. The direct probe excludes a cmd-only parsing failure, not a native Git/runtime/environment fault. Faulting module and product-versus-runner/fixture ownership remain unproven; keep diagnostics and safety assertions. |
| 7 | `temporary_detached_worktrees_are_never_orphan_candidates` | F | Raw porcelain uses forward slashes; registration-presence oracle used backslashes. Keep the registration and no-cleanup assertions. |
| 8 | `attachment_resume_drops_only_generated_directory_grants` | P | `agent_session.rs`: only configured root was canonicalized. Windows verbatim/nonverbatim roots compare unequal, so a deleted generated grant survives into the resume command. Resolve the existing config root on both sides, not the expired session child. A real symlink-root reproducer fails on Linux too. |
| 9 | `codex_warm_discovery_does_not_restat_historical_rollouts` | P, invalidation cause proven | Run 40150611058 logs an available watcher and `Modify(Any)` on `sessions/2026`, with `rescan=false`, unchanged days, and `directory_changed=true`. `CodexIndex::candidates` invalidates the cached index for a date ancestor and rescans its files; the following lookup creates an uninitialized index and scans them again. Two scans of 65 files account for the exact 130 extra metadata reads. The fixture performs no writes between its first discovery and ten repeated lookups. This is over-broad directory-event invalidation, not watcher unavailability. The source of the native event remains unknown. Preserve the zero-read assertion; this build-only follow-up does not change watcher semantics. |
| 10 | `compose_launch_command_keeps_a_prompt_that_is_not_the_start_work_brief` | F | Host shell correctly quotes with double quotes on Windows; oracle hard-coded Unix single quotes. Use the existing shell quote policy. |
| 11 | `native_worktree_uses_configured_base_and_existing_workspace_uses_repository` | P | Automation canonicalizes the destination; Git receives a verbatim Windows path and reports `//?/D:/...: Invalid argument`. Normalize the external Git destination argument once in `create_worktree_internal`, including its existing-branch retry. Keep native filesystem paths. |
| 12 | `dispatch_persists_clean_workspace_precheck_and_launches_exact_prompt` | F, native differential proven | Run 40179831695 on `d12b5106e`, lines 4192/4221/4262: original IF chain returns exit 0 and empty stdout for both verbatim and native cwd. Under the same real `run_precheck` entry point, grouped `if defined TUIC_TEST_TMP_ROOT (exit /b 9) else (type input.txt)` and plain `type input.txt` both return exit 0 and exact `resolved-output` for both cwd spellings. The input file also reads `Ok("resolved-output")` from Rust. The fixture command grammar, not cwd spelling or stream capture, suppresses the read. Use the verified grouped form and preserve every assertion and deadline. Native post-repair owner test now passes in run 40193236753 in 0.743s (line 6760). |
| 13 | `cleanup_keeps_a_fresh_branch_without_merge_proof` | P | Same invalid canonical Git destination as #11; failure occurs in workspace creation, before cleanup. |
| 14 | `cleanup_rechecks_dirty_unmerged_and_locked_worktrees_without_discarding_work` | P | Same workspace-creation failure; cleanup assertions have not run. |
| 15 | `cleanup_requires_archive_for_content_superset_integration` | P | Same workspace-creation failure. |
| 16 | `final_run_cleanup_retains_provenance_after_definition_deletion_and_reopen` | P | Same workspace-creation failure. |
| 17 | `session_stats_only_stats_the_changed_transcript_then_reconciles` | P, invalidation cause proven | Run 40150611058 lines 7294–7304 show an available watcher, no reconciliation due, and native `Modify(Any)` events for both `project-a` and `active.jsonl`. `UsageScan::prepare` marks every depth-one project event as a full rescan; it clears `seen` and `pending`, so both active and history are statted. The exact two-versus-one assertion is valid; this build follow-up does not change watcher semantics. |
| 18 | `session_stats_warm_refresh_does_not_stat_unchanged_history` | P, invalidation cause proven | Lines 7326–7333 show an available watcher and a `Modify(Any)` event for `project-a` between unchanged refreshes. The depth-one directory event sets `rescan=true` despite `reconciliation_due=false`; clearing `seen` makes the unchanged file stat again. This is the same broad directory-event invalidation as #17, not unavailable watcher fallback. |
| 19 | `planner_ideas_are_scoped_and_validate_before_writing` | F | Story API resolves the canonical project owner before storing the idea; oracle compared against the uncanonicalized input. Expect the owner supplied by the same authoritative ownership boundary. |
| 20 | `session_stats_cache_tracks_growth_truncation_and_deletion` | F (proven mismatch) | Production watches the canonical projects root; readiness helper compared its canonical events with a noncanonical fixture file path. Canonicalize the readiness oracle. This removes that mismatch, not a claim that native watcher delivery is proven. |
| 21 | `repo_add_persists_workspace_and_preserves_repeat_registration` | F | `registered_spelling` deliberately persists a portable canonical key; oracle used verbatim canonical key. Use the registration spelling for lookup and exact values. |
| 22 | `repo_add_uses_real_git_head_and_invalid_paths_do_not_change_config` | F | Git HEAD assertion already passed; line 108 looks up config with the wrong native key and returns null. Same fixture root cause as #21, not a product failure to discover the branch. |
| 23 | `native_mcp_worktree_list_reuses_merge_proofs_for_forty_worktrees` | P, cause open | Warm listing takes 13.6210139s against the 3s CLI response contract. The preceding base-repo fork-budget assertion passed, so repeated base integration subprocesses are not established as the cause. Native per-checkout profiling is needed. Do not raise the bound or remove the timing assertion. |
| 24 | `invalid_pause_publish_is_a_client_error` | F | Fixture seeds the store directly with a noncanonical project, while the HTTP API canonicalizes ownership. It fails project scope before graph validation. Seed with the API's resolved owner; keep client-error and `resume_to` assertions. |

The patch addresses the proven causes in 18 rows. Six rows remain unresolved: #6, #9, #12, #17, #18 and #23. Inline dated deferrals identify their evidence gaps at the test sites. No speculative watcher or removal changes were made. The complete-classification criterion remains unchecked because #6 still lacks fault ownership; the later native differential proves #12 is a fixture defect (see follow-up below).

## Why the job reached 90 minutes

The annotation says `The job has exceeded the maximum execution time of 1h30m0s`. This is a job bound, not evidence of a single 90-minute test.

The log starts at 21:31:05. It reports dev builds of 9m39s and 5m12s, then a test-profile build of 44m40s ending at 22:31:47. Nextest starts 8364 tests at 22:31:48. Its scoped fixture setup builds another dev profile for 17m44s, ending at 22:49:35. Thus about 77 minutes are consumed before ordinary tests make substantial progress. At cancellation around 23:01:32, the suite is only partly complete.

The CI profile retries twice with a fixed one-second DELAY. Each failing test can execute three times. The 40-worktree test exceeds 30 seconds including setup on multiple attempts; its measured repeat-list operation alone takes 13.621 seconds on the first failure. Other logged monitoring tests hit 120-second test deadlines and retry. DELAY is the retry backoff, not a paused or deadlocked runner. For 24 persistent panics, the fixed sleeps alone can total 48 seconds; repeated test setup/execution costs more. The partial log does not bound failures after cancellation.

These changes remove known deterministic failure/retry causes. They do not remove the large compile/setup cost, prove the remaining latency cause fixed, or prove the whole Windows job fits. A fresh authorized CI run must establish that. No job or test time bound was raised.

## Regression and validation

Pre-fix source: `72e1fcc518021a5c70a186d6883c28036e6c3507` plus the new regression test only. `attachment_resume_drops_expired_grants_through_config_alias` ran on rb Linux and failed with a retained `alias/attachments/pty-old` argument. Log: `~/Gits/.tmp/results/ci-win-red.log`; compilation 3m24s; one test failed in 0.029s. This isolates root canonicalization independently of Windows separators.

Existing Windows RED evidence for the Git boundary is the five creation failures in rows 11 and 13–16. The pre-existing `windows_paths_are_rewritten_the_way_git_prints_them` test protects drive, verbatim, UNC and already-portable spellings; Unix literal-backslash preservation is also covered in `path_spelling`.

Final targeted validation and cross-target availability are recorded in the terminal handoff and story worklog. Native Windows GREEN, watcher diagnosis, native precheck/cleanup-hook diagnosis, and response-time profiling remain required. Rust changes do not hot-reload into the daily application; no application was restarted.

## Build follow-up: run 40150611058 on d27aa1739

The Windows log at `~/Gits/.tmp/ci-d27a-win.txt` proves two profiles in the Tests
step: `test` finishes in 45m36s at line 3299; scoped fixture setup starts at line
3305 and its `dev` build finishes in 18m15s at line 3807. Together they consume
63m51s before ordinary tests start. The scripts used `cargo build
--no-default-features`, while nextest used the test profile with dictation.
Dev has line-table debug information; test disables debug information. Both
profile differences and feature unification differences require new artifacts.

The repair keeps local lib-only fixture builds headless with `--profile test`.
Desktop CI supplies `TUIC_FIXTURE_FEATURES=dictation`, which selects `cargo test
--workspace --no-run --features dictation` in both setup scripts. This retains
nextest's dev-dependency feature unification as well as its profile/defaults.
The first version used a binary-only build and missed that unification; see
critic F1 below. Executable names and the `debug` output directory are unchanged.
The existing nextest consumer filters are unchanged. Windows workspace harness
optimization changes from O1
to O0, as already used on macOS. Dependencies and the terminal replay crate keep
their explicit O1 settings. No assertion, retry, test filter or deadline changes.

The target-cache restore reports `No cache found` at line 688. Its post-action
uses the default `cache-on-failure=false`, so failed/cancelled jobs do not save
that cache. The action definition at
`https://raw.githubusercontent.com/Swatinem/rust-cache/v2/action.yml` gates its
save hook on `success() || env.CACHE_ON_FAILURE == 'true'`. The Windows workflow
now enables that input, retaining the existing `C:/t` cache directory.

sccache did run: 2791 requests, 841 hits, 1575 misses; Rust alone had 131 hits and
1291 misses (9.21%). Non-cacheable calls included 152 binary and 93 proc-macro
requests. It cannot cache final links, and it cannot make distinct profile or
feature keys identical. The log does not identify every miss cause, so low hit
rate is not attributed solely to the fixture scripts. No cache deletion,
compiler flag workaround, or job timeout increase is part of this repair.

The two requested failure classifications are updated in rows 6 and 9. Row 6
now proves a native subprocess fault but not its owner. Additional pre-cancel
failures include the automation precheck and two Usage metadata-counter tests;
they are not resolved by a build-profile change. A complete Windows run and
native crash diagnosis remain required before story completion.

### Initial native profile validation (before critic F1)

This validates profile selection and reuse between two identical binary-only
builds. It does not prove reuse after CI's full workspace test graph; critic F1
identified that missing check and the correction below replaces the CI command.

`~/Gits/.tmp/results/ci-win-profile-validation.log` completed with exit 0 through
native build-slot and mbx. `mbx doctor` reported zero failures and warnings.

- The new argv regression failed against exact `d27aa1739` builder scripts:
  expected `--profile test` was absent. Both current shell and PowerShell
  builders passed the headless/default, dictation-feature and exit-7 propagation
  cases. This protects the Cargo invocation boundary, not compilation itself.
- `scripts/with-test-tmp.sh cargo nextest run --manifest-path
  src-tauri/Cargo.toml --features dictation --lib --no-run -E
  'test(=mcp_proxy::stdio_client::tests::is_alive_returns_false_after_process_exits)'`
  compiled successfully: test profile, 7m52s. No Rust test was executed.
- With `CARGO_PROFILE_TEST_OPT_LEVEL=0` and `TUIC_FIXTURE_FEATURES=dictation`,
  the shell fixture builder completed in the test profile in 2m55s. A lib-only
  compile does not build the normal library/binaries that the full workspace
  integration build produces, so this local first fixture build is not expected
  to be empty.
- The PowerShell builder then reused those artifacts in the test profile in
  1.71s. Neither fixture command selected the dev profile.
- Bash syntax, PowerShell AST parse, workflow YAML parse, instruction-link,
  `git diff --check` and changed-file formatting checks passed. `actionlint` was
  not installed. The nextest setup-scope integration script was not rerun; its
  configuration and assertions were not changed.

The build emitted the compact-unwind size warning and mbx cache-bypass warnings
for missing build-script declarations/output names and changed `OUT_DIR`.
No Windows execution, complete workspace suite, runtime performance result or
90-minute completion is inferred from this native Mac build evidence.

### Critic F1: resolver-2 graph correction

The independent critic found that
`cargo build --profile test --features dictation` still omits dev-dependency unification. Its real Cargo probe showed
`tokio/test-util` absent in the binary-only graph and present in the workspace
test graph. The same difference affects `tuic-core/test-support` and
`tuic-git/test-support`. Top-level features and output paths alone were
insufficient evidence of full graph reuse.

The committed graph regression captures the actual shell/PowerShell builder
argv, resolves its package-feature graph with real `cargo tree`, and compares
all entries with the CI workspace test selection. It also requires the three
features above in the oracle, preventing an empty or wrong-kind comparison.
On pre-fix `79b59184e`, it failed for the named bug: the captured command was
`build --profile test --no-default-features --features dictation --bin ...`,
with missing test dependencies and different Tokio/core/git feature sets.
Evidence: `~/Gits/.tmp/results/ci-win-f1-red.log`.

CI setup now selects `cargo test --workspace --no-run --features dictation`,
the same workspace/default-feature/dev-dependency selection as nextest.
Cargo's integration targets build the normal fixture executables. No tests
are executed by this setup command. The scoped local headless fallback stays
unchanged. This corrects F1 without maintaining a manual list of dependency
features. A complete Windows workspace warm-build/runtime measurement remains
required; graph equality is not claimed to be that measurement.

Final targeted validation passed through build-slot and the mbx Cargo shim:
`check-fixture-build-profile.sh` passed both shell/PowerShell default, CI argv
and exit propagation cases. `check-fixture-build-graph.py` resolved the actual
CI selection of each builder to the same 793 package-feature entries as the
workspace test oracle in this run, including Tokio test-util and core/git
test-support. Log: `~/Gits/.tmp/results/ci-win-f1-final.log`, exit 0. These
commands compile and execute no Rust targets. Bash/Python syntax, workflow
YAML and `git diff --check` also passed; `actionlint` remains unavailable.

## Native classification follow-up: run 40172919921 on f16be3873

The complete Windows job log for job `121107564809` is saved at
`~/Gits/.tmp/ci-f16b-win.txt`. Both requested owner tests fail on all three
attempts. The new run confirms the earlier symptoms; it does not add a
native differential result that identifies their cause.

- Row 6 owner: `worktree::tests::removal_keeps_local_branch_when_cleanup_prunes_its_remote_proof`.
  Lines 5071–5073 report the actual cleanup command `cmd /C "git fetch --prune
  origin"`, a portable worktree cwd, status `3221225477` (`0xC0000005`), and
  empty stdout/stderr. The removal operation returns `Archive script failed`.
  The assertion correctly detects that the tracking ref was not pruned; it
  does not prove a branch-safety defect. Product versus fixture/environment
  ownership remains unresolved because neither the crashing executable nor
  its fault is identified. No deletion guard, Git proof check, or assertion
  has been removed or weakened.
- Row 12 owner: `automations::dispatcher::tests::dispatch_persists_clean_workspace_precheck_and_launches_exact_prompt`.
  Lines 6730–6733 report `cmd.exe /D /S /C "if defined TUIC_TEST_TMP_ROOT exit
  /b 9 & type input.txt"`, the canonical verbatim workspace cwd, exit zero,
  empty stdout/stderr, and no truncation. The first attempt takes 29ms. The
  exact expected stdout remains `resolved-output`. The log does not isolate
  conditional parsing from native shell/cwd behavior, so assigning this to
  either the fixture or product would still be a guess. The clean-environment
  policy and exact persisted-output assertion remain unchanged.

The coordinator authorized a tiny non-build Windows lab probe. Its matrix
compares the original IF chain with grouped IF/ELSE and plain TYPE, each with
and without the environment sentinel and with native/verbatim cwd spellings.
It also compares direct Git with cmd-mediated Git under the same inherited or
synthetic bounded PATH. A synthetic PATH is not claimed to reproduce the CI
process's actual PATH. The probe uses a throwaway local bare origin, does not
install software, and removes its scratch tree in a finally block.

The probe did **not execute**: SSH to the configured Windows lab host timed
out on port 22 and the launcher exited 255. Evidence:
`~/Gits/.tmp/results/ci-win-native-probe.log`. No remote files were created and
no native Windows success or RED/GREEN result is claimed. The retained probe
source is `~/Gits/.tmp/results/ci-win-native-probe.ps1`; it is an unexecuted
probe, not a captured Windows fixture. A reachable authorized Windows runner,
or equivalent native differential output from the CI environment, is required
before rows 6 and 12 can be classified definitively. No speculative code fix
was made from the repeated diagnostics.

### CI-native failure-only probes

After the lab connection failed, the coordinator selected CI as the Windows
runner. The two owner tests now attach differential results only when their
original tracking-ref or persisted-stdout check has already failed. Successful
probes cannot replace the original result or make the test pass. No production
code, assertion expectation, removal guard or normal-path execution changes.

- Row 6 captures the actual cleanup PATH and cwd under `cfg(test)` on Windows.
  The failure probe selects the first `git.exe` in that PATH, then compares
  direct `git fetch --prune origin` with the original `cmd /C` form, both using
  the captured PATH and cwd. Each probe starts with the same stale tracking
  ref restored inside this test's disposable repository. The panic includes
  invocation, signed exit code, native status and both streams. Each of the
  two resets and two subprocess probes has a ten-second execution deadline.
- Row 12 calls the real `run_precheck` entry point for the original IF chain,
  grouped IF/ELSE and plain file read under both the original verbatim cwd and
  its native spelling. All six cases retain the same clean-environment,
  cmd-flag, process-ownership and stream-capture policy as the failed precheck.
  Each has a two-second execution budget; existing stream cleanup may take
  another two seconds. Setup is outside that execution budget. The panic
  includes each cwd, command and complete precheck result (termination, exit
  code, streams and truncation), plus the fixture input file read by Rust.

These are diagnostic probes, not fixes or new test oracles. Formatting and
static diff checks apply locally. Native compilation and execution of the
Windows-only branches remain for the next main CI run; neither classification
nor Windows GREEN is claimed before that evidence exists.

## EGO-DEFECTS

The admitted adversarial-tdd skill requests its README, but resource retrieval failed with the exact line: `/Users/stefano.straus/.ego/skills/adversarial-tdd: no such file or directory`. No README contents were invented.

## Validation outcome and current blockers

- rb RED: one new attachment regression failed as expected (log above).
- First final rb attempt exited 78 because `src-tauri/src/claude_usage.rs` requires Mac validation; no tests started.
- First Mac compile exposed a private-helper reference in the registration test fixture. The fixture now uses public `path_spelling::portable_spelling` on its canonical path, without widening production visibility.
- Mac retry: `scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(session_stats_)'` through build-slot and mbx: **5 passed**, test summary 0.156s, build 2m45s. Linker warned that `__eh_frame` exceeds the compact-unwind encoding limit. `mbx doctor`: 0 failures and 0 warnings before the run.
- Forced rb retry did not start: `ssh: connect to host 100.121.52.117 port 22: Operation timed out` and `rb: box unreachable or runner deployment failed; no local fallback; remote command was not started` (exit 69). Log: `~/Gits/.tmp/results/ci-win-targeted-force.log`.
- Windows MSVC target is installed on this Mac, but all cross-target Rust commands belong on rb. rb target availability and `cargo check --target x86_64-pc-windows-msvc` could not run because the connection failed. No Windows compile or runtime success is claimed.
- The queued final filter covers the affected worktree, resume, automation, stats, registration, workflow and path-spelling tests. Its Mac prerequisite passed; the full filter still needs rb recovery. No full suite, mutation run, desktop launch or push was performed.

## Diagnostic follow-up for the next Windows run

The follow-up adds test-only evidence; it does not resolve the six open rows or change product decisions, assertions, or time bounds.

- Row 6: the cleanup shell invocation records its program, arguments, cwd, exit status, stdout, stderr, or execution error. The tracking-ref assertion includes this record and the removal outcome.
- Row 12: the actual precheck execution records its program, arguments and canonical cwd. Failure messages include that invocation and the persisted outcome, including termination, stdout and stderr.
- Row 9: each Codex refresh records watcher availability/setup errors, native events, rescan/day/directory invalidation flags, pending paths and per-file metadata-read deltas. The zero-extra-read assertion includes these records and per-path totals.
- Rows 17 and 18: Usage refresh records watcher creation/watch errors, native events, rescan/reconciliation state and pending paths. Each metadata read records the file and whether it was unseen, pending or in a symlinked project. The existing exact-counter assertions include this history.
- Row 23: warm listing records inventory time and each checkout's blocking-queue and inspection time. The existing three-second assertion includes those measurements.

Diagnostic state and capture operations are guarded by `cfg(test)`. The native Mac touched-filter run is authorized explicitly for this follow-up; rb must not be retried. Windows classification and the Windows CI completion criterion remain open until the next authorized CI run supplies evidence.

### Follow-up validation

Native macOS validation of the diagnostic source in `354bcc86fa0c1737b52000e37d5322bbbbec9279` passed all six touched tests across the `tuicommander` and `tuic-git` lib test binaries. Command: `scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml -p tuicommander -p tuic-git --lib -E '<the six test names in rows 6, 9, 12, 17, 18 and 23>'`, through `tuic-bg.sh`, native `build-slot.sh` and the mbx cargo shim, with the required test TMPDIR. Full command and output: `~/Gits/.tmp/results/ci-win-diagnostics-native.log`. Build: 4m49s; test summary: 11.988s, six passed. The forty-worktree test's 11.882s total includes setup; its unchanged three-second warm-operation assertion passed. The linker emitted the existing compact-unwind size warning; mbx reported cache bypass/miss and automatic low-disk GC messages. No cache deletion or target override was requested by the agent.

`git diff --check` and `fmt-changed.sh <worktree> main...HEAD` passed. The story closure check still refuses criterion 1 (complete product-versus-fixture classification); native Mac success cannot resolve the six Windows evidence gaps or satisfy the Windows CI criterion. No rb retry, push, desktop launch, deadline increase or assertion relaxation was performed.

## Diagnostic run 40179831695: rows 6 and 12

Source: coordinator-fetched native Windows job 121128653791 log at
`~/Gits/.tmp/ci-d12b-win.txt`, main SHA `d12b5106e79215e749090655222c07bbc809f5eb`.
Clippy succeeded and the test job completed with failures. The initial watcher
lost GitHub connectivity; the coordinator supplied the job log. That transient
retrieval failure is not a test classification.

Row 6, line 2553:

```text
PASS [ 11.812s] ... worktree::tests::removal_keeps_local_branch_when_cleanup_prunes_its_remote_proof
```

The probes run only on failure, so this pass supplies no direct-versus-cmd
comparison. The earlier access violation remains unresolved. No product change,
retry policy, assertion change or runner attribution follows from one pass.

Row 12, excerpts from line 4192 (repeated on attempts 2 and 3 at lines 4221/4262):

```text
input file=Ok("resolved-output")
spelling=original; form=chain; ... termination: Exited(Some(0)), stdout: ""
spelling=original; form=grouped; ... termination: Exited(Some(0)), stdout: "resolved-output"
spelling=original; form=plain; ... termination: Exited(Some(0)), stdout: "resolved-output"
spelling=native; form=chain; ... termination: Exited(Some(0)), stdout: ""
spelling=native; form=grouped; ... termination: Exited(Some(0)), stdout: "resolved-output"
spelling=native; form=plain; ... termination: Exited(Some(0)), stdout: "resolved-output"
```

These excerpts omit paths and unchanged result fields. All six recorded results
have empty stderr and no truncation. The grouped command used by the probe is
exactly `if defined TUIC_TEST_TMP_ROOT (exit /b 9) else (type input.txt)`.
The repair uses that command in the fixture, leaving production execution,
persisted-output expectations, clean-environment guard and literal-prompt
assertions unchanged. Existing native owner-test failures provide RED on
`d12b5106e`; grouped differential success is native boundary evidence, not a
post-repair owner-test GREEN. Native Windows post-repair CI is still required.

The final targeted Linux run on rb passed the repaired owner test: one test,
summary 0.177s, test-profile build 5m54s. Command through `build-slot.sh --remote`:
`scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(dispatch_persists_clean_workspace_precheck_and_launches_exact_prompt)'`.
Log: `~/Gits/.tmp/results/ci-win-c-targeted.log`. This covers the unchanged Unix
command and dispatcher consumer; it does not execute the Windows command branch.

## Complete native run 40193236753 on d3404c764

Native Windows job `121170353740` completed with failed Tests, not cancellation.
Evidence: `~/Gits/.tmp/results/ci-win-d-d340-windows.log`; ANSI-free copy:
`ci-win-d-d340-windows-clean.log` in the same directory. Line numbers below refer
to both copies (escape removal does not remove lines). Job metadata is retained
in `ci-win-d-d340-jobs.json`; the frontend log is `ci-win-d-d340-frontend.log`.

Earlier watched runs `40185491370` (`9f91a82bf`) and `40186478883`
(`44998188e`) were superseded and cancelled before an owner-test result.
Their logs are retained as `ci-win-d-<run-id>-cancelled.log`. Run
`40189776374` (`72c7c6823`) was superseded by the coordinator's later push;
no owner-test result from that run is claimed here.

### Row 6: what the new probes prove

The owner fails all three attempts (9.146s, 8.600s, 8.748s). Each original
cleanup returns the access-violation status. The captured PATH starts with
`C:\Program Files\Git\ucrt64\bin`; its direct probe selects the executable
in that directory. Both probe ref resets through `git_cmd` succeed. Direct
Git and cmd-mediated Git then fail with the same signed code `-1073741819`,
unsigned status `3221225477`, and empty stdout/stderr. This is actual native
execution evidence, not a manually constructed Windows fixture.

This narrows the fault to native Git execution (or a child/runtime it loads),
not cmd conditional grammar. It does not identify the faulting module, explain
why the different `git_cmd` setup/reset environment succeeds, or prove whether
the hook PATH transformation causes the fault. `run_shell_script` prepends the
inherited `which_cli("git")` directory and bounds PATH, while `git_cmd` uses
`resolve_cli("git")`, enriched PATH and extra Git flags. Those differences are
verified source facts, but none is independently varied by the present probe.
Changing PATH selection, Git flags, cwd, or branch deletion from this evidence
would choose an unproven cause. Classification remains **U: intermittent native
Git subprocess fault**, with a narrower proven layer; no product or fixture
repair is claimed. Native fault/module evidence or a controlled comparison of
the successful `git_cmd` environment with the failing hook environment is still
needed. Existing diagnostic probes and all safety checks remain unchanged.

### All original rows: latest native results

| Rows | Result and evidence |
|---|---|
| 1-5 | PASS, lines 4904-4905, 5047, 5065-5066. |
| 6 | FAIL on all attempts; differential panics at 5104, 5126, 5149. Previous diagnostic run 40179831695 passed unchanged. |
| 7-8 | PASS, lines 5173 and 6513. |
| 9 | FAIL on all attempts; unchanged classification. Line 6532 again records available watcher, `Modify(Any)` date ancestors, `directory_changed=true`, and 130 extra reads (6533-6534). |
| 10-11 | PASS, lines 6545 and 6756. |
| 12 | **Native post-repair PASS**, 0.743s, line 6760. This closes the Windows-only verification gap for the grouped IF/ELSE fixture repair, not the whole Windows job. |
| 13-16 | PASS, lines 6852, 6922, 7321 and 6853. |
| 17-18 | FAIL on all attempts; unchanged classification. Lines 7005-7017 and 7037-7047 repeat directory `Modify(Any)` forcing a rescan while reconciliation is not due. |
| 19-22 | PASS, lines 7176, 6987, 8680 and 8684. |
| 23 | Still FAIL; attempt 1 now reaches the 120s harness timeout (11077). Attempts 2/3 report warm-operation latency 14.2819049s/14.2309158s (11299/12025). Inventory costs only 85.972ms/123.4101ms; examples of checkout inspection cost are 8.2547934s/9.2323387s and 9.3004421s. These locate substantial time inside checkout inspection, not inventory; they do not identify its internal cause. No deadline or assertion changed. |
| 24 | PASS, line 9025. |

Thus the changed verification states are row 6's renewed native failure with
actual differential evidence, row 12's native post-repair success, and row 23's
first-attempt harness timeout plus per-checkout latency evidence. The other
classifications do not change. All 19 previously repaired rows pass in this run.
Rows 6, 9, 17, 18 and 23 remain failing.

The complete nextest summary (line 14225) is 8374 run: 8348 passed, 25 failed,
one timed out, and 32 skipped; elapsed 1290.627s. The job finishes within its
job bound but does not satisfy the zero-failure criterion. The final failure
list at 14226-14251 also names failures outside the original 24-row inventory:
monitoring Git-poll timeout, unchanged-draft publishing, vanished-checkout
registration, five monitoring-status cases, repo-watcher commit classification,
two resume-command cases, execution policy confirmation, story archive/list/note
cases, and a relay command case. These are reported for coordinator ownership;
this evidence-only change does not alter their code.

### Other job conclusions for 2292-786b

- Rust Linux-x64 job `121170353888`: success.
- Rust macOS-ARM job `121170353919`: success.
- Remote daemon job `121170353526`: success.
- Frontend job `121170353647`: failure. The only failed test is
  `src/mobile/__tests__/MobileFontSize.test.tsx` > `restores independent Terminal
  and Chat sizes without resizing the composer`, which hits its 5000ms test
  timeout. Summary: 912 passed/1 failed/2 skipped files; 10871 passed/1 failed/
  3 skipped tests. This is not proof of a font-sizing product defect; diagnosis
  belongs to the coordinator. Neither the all-platform nor frontend GREEN
  criterion can be inferred from this run.

No local Rust build, test execution, product change, assertion change, timeout
increase, merge or push was performed for this evidence-only follow-up.
