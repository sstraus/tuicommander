# Scenario harness (L1)

Code: `src-tauri/src/workflows/run/scenario/` (test-only). It drives the real
`RuntimeOwner`, `RunStore`, `NativeHost` and neutral publications. Only external
ports are doubles (ACP model fixture, scripted operator).

## How it works

- A **commit point** is one row of `neutral_workflow_events`.
- `Fault::Restart(k)` installs a SQLite trigger that refuses every event insert
  after sequence `k`. The owner can no longer commit, as after a kill. The harness
  then drops the owner, boots a new owner and a new host on the same database,
  repeats the start request and wakes the run.
- `Cancel(k)`, `Duplicate(k)` and `LateCallback(k)` use the same freeze.
  `LostReply(n)` drops the reply of adapter entry `n`; `LostReplyRestart(n)` adds a restart.
- No production hook is needed: the trigger and the existing `neutral_effect_updates`
  notification are enough.
- Invariants (`invariants.rs`) run after every execution: no second adapter entry or
  dispatch for a mutating effect; no route or finish while an activity has no committed
  completion; no dispatch after cancel; claims held until stop evidence; terminal or
  documented waiting (open approval, uncertain effect, unbound caller); terminal result
  equals the fault-free run (`Scenario::canon`).

## Add a scenario

1. In `scenarios.rs` write `fn build_x() -> Rig`: build the publication with the
   `parity_support` helpers (`graph`, `script`, `save`, `publish`) or the wire JSON, then
   `Rig::attach(owner, state, db, project, host, Arc::new(Calls::default()), start, keep)`.
   `start` must be idempotent (it runs again after a restart) and return the run id.
2. Add `fn x() -> Scenario { name, build, expect, canon, known_stalls: &[] }`.
   `expect` is the oracle for the fault-free run; `canon` removes run-specific values.
3. In `tests.rs` add `#[tokio::test] async fn sweep_x() { assert!(sweep(x()).await > 10) }`.
4. Run: `scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(scenario::tests)'`.
   `SCENARIO_DUMP=1 ... --no-capture` prints the event log of every run.

Use the default `#[tokio::test]` (current thread): the config override is thread-local.

## Naming

Scenario names start with the acceptance letter of `STUDY.md` section 2.1 (`A.`, `B.`).
Durable points follow STUDY section 1.11: seq 2-3 DP2, `prepare` DP3, `question/answer`
DP4, `dispatch` DP5, `observe` DP6, `complete` DP7, `finish/cancel` DP8,
`enter_subflow` DP11.

## Known defects (ignored RED tests, run with `--run-ignored all`)

- `b_restart_between_script_dispatch_and_observe_completes`
