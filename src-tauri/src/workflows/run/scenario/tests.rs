//! Pilot sweeps. Each sweep re-runs one scenario for every durable commit point
//! and adapter entry of its fault-free run and checks the shared invariants.
use super::faults::{Fault, Outcome, execute};
use super::rig::Rig;
use super::scenarios::*;

fn dispatches(golden: &Outcome) -> usize {
    golden
        .events
        .iter()
        .filter(|e| e["kind"]["decision"]["decision"]["type"] == "dispatch")
        .count()
}

/// Returns the number of permutations executed.
async fn sweep(scenario: Scenario) -> usize {
    let golden = execute(&scenario, Fault::None, None).await;
    (scenario.expect)(&golden.run);
    let commits = golden.events.len();
    let entries = dispatches(&golden);
    let mut runs = 1;
    let mut unbound = 0;
    let mut second_wake = 0;
    // Crash points: the last commit is terminal, so a fault there is a no-op.
    for k in 1..commits {
        let stalls = scenario.known_stalls.contains(&k);
        for fault in [
            Fault::Restart(k),
            Fault::Cancel(k),
            Fault::Duplicate(k),
            Fault::LateCallback(k),
        ] {
            if stalls && matches!(fault, Fault::Restart(_) | Fault::Duplicate(_)) {
                continue;
            }
            let outcome = execute(&scenario, fault, Some(&golden)).await;
            assert!(
                outcome.reached,
                "{}: {fault:?} never reached its commit point",
                scenario.name
            );
            unbound += usize::from(outcome.unbound_after_restart);
            second_wake += usize::from(outcome.needed_second_wake);
            runs += 1;
        }
    }
    for n in 1..=entries {
        for fault in [Fault::LostReply(n), Fault::LostReplyRestart(n)] {
            let outcome = execute(&scenario, fault, Some(&golden)).await;
            unbound += usize::from(outcome.unbound_after_restart);
            second_wake += usize::from(outcome.needed_second_wake);
            runs += 1;
        }
    }
    eprintln!(
        "SWEEP {}: {commits} commits, {entries} adapter entries, {runs} runs, {unbound} left unbound by restart, {second_wake} needed a second wake",
        scenario.name
    );
    runs
}

// Catches: a restart, cancel, lost reply or duplicate callback at any durable point of
// scheduled text -> model -> condition -> exact approval writes twice, advances the
// graph without output, releases a claim early, or reconciles to another result.
#[tokio::test]
async fn sweep_a_scheduled_text_model_condition_approval() {
    assert!(sweep(a_scheduled_text()).await > 10);
}

// Catches: the same faults around script -> native artifact pin -> pinned subflow.
#[tokio::test]
async fn sweep_b_script_native_pinned_subflow() {
    assert!(sweep(b_script_native_subflow()).await > 10);
}

// Catches: a scheduled tick after a daemon restart builds its registry before the fresh
// host loads the project catalog, so the first due occurrence fails with "handler
// unavailable: host.approval@1" and is not admitted.
#[tokio::test]
async fn first_tick_after_restart_admits_the_due_occurrence() {
    let mut rig = (a_scheduled_text().build)();
    rig.restart().await;
    let admitted = crate::workflows::scheduled_trigger::tick(&rig.state, chrono::Utc::now());
    // tick() logs a failing binding instead of returning Err, so assert the admission.
    assert_eq!(admitted.map(|a| a.len()), Ok(1));
}

// Catches: a scheduled run created before a daemon restart is never rebound to its
// trusted caller by the repeated tick, so the actor admits nothing and the run stays
// Running until its deadline.
#[tokio::test]
async fn repeated_tick_after_restart_rebinds_the_existing_run() {
    let mut rig = (a_scheduled_text().build)();
    let id = (rig.start)(&rig);
    rig.restart().await;
    let now = chrono::Utc::now();
    let _ = crate::workflows::scheduled_trigger::tick(&rig.state, now);
    let _ = crate::workflows::scheduled_trigger::tick(&rig.state, now);
    assert!(
        rig.bound(&id),
        "run {id} is still unbound after the repeated tick"
    );
}

/// A trigger resource that still names a run whose rows cannot be read. It sorts
/// before every real run id, so a rebind pass meets it first.
fn hold_resource_with_unreadable_run(rig: &Rig) {
    rig.owner
        .store
        .trigger_connection()
        .unwrap()
        .execute(
            "INSERT INTO neutral_trigger_resources(project,binding_id,resource,active_run) \
             VALUES(?1,'ghost-binding','ghost-resource','000-unreadable-run')",
            [&rig.project],
        )
        .unwrap();
}

// Catches: one old run whose rebind fails aborts the whole admission pass of its
// project, so a different due occurrence is never admitted (starved every tick).
#[tokio::test]
async fn an_unreadable_old_run_does_not_starve_the_due_occurrence() {
    let mut rig = (a_scheduled_text().build)();
    hold_resource_with_unreadable_run(&rig);
    rig.restart().await;
    let admitted = crate::workflows::scheduled_trigger::tick(&rig.state, chrono::Utc::now());
    assert_eq!(admitted.map(|a| a.len()), Ok(1));
}

// Catches: the failed rebind of one old run stops the pass (early return instead of
// moving on), so a later genuinely unbound run stays unbound after the restart.
#[tokio::test]
async fn an_unreadable_old_run_does_not_block_the_rebind_of_another_run() {
    let mut rig = (a_scheduled_text().build)();
    let id = (rig.start)(&rig);
    hold_resource_with_unreadable_run(&rig);
    rig.restart().await;
    let now = chrono::Utc::now();
    let _ = crate::workflows::scheduled_trigger::tick(&rig.state, now);
    let _ = crate::workflows::scheduled_trigger::tick(&rig.state, now);
    assert!(
        rig.bound(&id),
        "run {id} is still unbound behind the unreadable run"
    );
}

// Catches: the pending intent of a real scheduled binding whose active run cannot be read
// makes flush_trigger_intents return Err for the whole project, so a later due binding
// (sorts after it) is reserved but never started on any tick. The adjacent tests above
// hold a ghost binding with no pending intent and never reach that read.
#[tokio::test]
async fn an_unreadable_active_run_with_a_pending_intent_does_not_starve_a_later_binding() {
    use crate::workflows::{
        WorkflowStore, neutral_host::NeutralCatalogHost, run::neutral_runtime::NeutralHost,
    };
    let rig = (a_scheduled_text().build)();
    let store = WorkflowStore::open().unwrap();
    let first = store.all_neutral_trigger_bindings().unwrap().remove(0);
    let catalog = rig.host.catalog(&rig.project).unwrap();
    let handlers = rig.host.registry().available_handlers();
    let mut draft = first.draft.clone();
    draft.id = "weekly-text".into();
    draft.resource_prefix = "schedule:weekly-text".into();
    let second = store
        .save_neutral_trigger_binding(&rig.project, 0, draft, &catalog, &handlers)
        .unwrap();
    store
        .enable_neutral_trigger_binding(
            &rig.project,
            "weekly-text",
            second.revision,
            true,
            &Default::default(),
            &catalog,
            &handlers,
        )
        .unwrap();
    // The first binding's resource names a run that cannot be read; its due
    // occurrence becomes a pending intent behind that run.
    rig.owner
        .store
        .trigger_connection()
        .unwrap()
        .execute(
            "INSERT INTO neutral_trigger_resources(project,binding_id,resource,active_run) \
             VALUES(?1,'daily-text','schedule:daily-text','000-unreadable-run')",
            [&rig.project],
        )
        .unwrap();
    let admitted = crate::workflows::scheduled_trigger::tick(&rig.state, chrono::Utc::now())
        .unwrap_or_default();
    let weekly = admitted.iter().find(|a| a.binding_id == "weekly-text");
    assert!(
        weekly.is_some_and(|a| a.run_id.is_some()),
        "later binding was not started behind the unreadable run: {admitted:?}"
    );
    assert!(
        admitted
            .iter()
            .any(|a| a.binding_id == "daily-text" && a.run_id.is_none()),
        "the intent behind the unreadable run must stay pending: {admitted:?}"
    );
}

// Catches: a host restart between the dispatch commit and the result commit of a script
// activity leaves the run waiting with no question and no way on. The operator question
// (answered by the scripted operator of `settle`) is the documented way on.
#[tokio::test]
async fn b_restart_between_script_dispatch_and_observe_completes() {
    let scenario = b_script_native_subflow();
    let outcome = super::faults::execute(&scenario, Fault::Restart(5), None).await;
    assert!(
        matches!(
            outcome.run.status,
            super::run::neutral_execution::NeutralStatus::Completed { .. }
        ),
        "{:?}",
        outcome.run.status
    );
}

// Catches: after a restart that lost a script reply, the script runs again without an
// operator decision (a mutating one writes twice), or a read-only one stalls silently.
// The "adapter twice" invariant cannot see this, so the script counts its own executions.
// Mutating: one run, uncertainty kept. Read-only: one run and an operator question;
// "run again" runs it exactly once more and completes; "abandon" fails the activity.
#[tokio::test]
async fn restart_after_a_lost_script_reply_asks_the_operator_before_any_rerun() {
    use super::run::neutral_effects::EffectOutcome;
    use super::run::neutral_execution::NeutralStatus;
    let patience = std::time::Duration::from_millis(2500);
    let runs = |rig: &super::rig::Rig| {
        std::fs::read_to_string(std::path::Path::new(&rig.project).join("script-runs.log"))
            .unwrap()
            .lines()
            .count()
    };
    for (mutation, decision) in [
        (true, None),
        (false, None),
        (false, Some(true)),
        (false, Some(false)),
    ] {
        let label = format!("mutation={mutation} answer={decision:?}");
        let mut rig = build_script_marker(mutation);
        rig.calls
            .lose
            .store(1, std::sync::atomic::Ordering::Release);
        let id = (rig.start)(&rig);
        rig.settle_unanswered(&id, patience).await;
        rig.restart().await;
        assert_eq!((rig.start)(&rig), id);
        rig.wake(&id);
        let run = rig.settle_unanswered(&id, patience).await;
        assert!(rig.bound(&id), "{label}: run unbound after resume");
        assert_eq!(runs(&rig), 1, "{label}: rerun without a decision: {run:?}");
        assert_eq!(run.status, NeutralStatus::Running, "{label}: {run:?}");
        let open = run
            .effects
            .values()
            .any(|e| e.question.is_some() && e.answer.is_none());
        assert_eq!(open, !mutation, "{label}: operator question: {run:?}");
        if mutation {
            // Reconcile ran after the restart and kept the manual uncertainty.
            assert!(
                run.effects.values().any(|e| matches!(
                    e.observation.as_ref().map(|o| &o.outcome),
                    Some(EffectOutcome::Uncertain { diagnostic }) if diagnostic.contains("retry is never automatic")
                )),
                "{label}: {run:?}"
            );
            continue;
        }
        let Some(approved) = decision else { continue };
        rig.answer_open(&id, approved);
        let run = rig.settle_unanswered(&id, patience).await;
        if approved {
            assert_eq!(runs(&rig), 2, "{label}: {run:?}");
            assert!(
                matches!(run.status, NeutralStatus::Completed { .. }),
                "{label}: {:?}",
                run.status
            );
        } else {
            assert_eq!(runs(&rig), 1, "{label}: {run:?}");
            assert!(
                matches!(&run.status, NeutralStatus::Failed { diagnostic } if diagnostic.contains("abandoned")),
                "{label}: {:?}",
                run.status
            );
        }
    }
}

// Catches: an operator "run again" starts the script while the process group of the
// original attempt still runs (it survived a TUIC kill), so two copies run at once; or
// the refusal leaves the run Running with no question and no failure until its deadline.
// The group is planted where the original attempt records it.
#[cfg(unix)]
#[tokio::test]
async fn run_again_is_refused_while_the_original_process_group_survives() {
    use super::run::neutral_execution::NeutralStatus;
    use std::os::unix::process::CommandExt;
    let patience = std::time::Duration::from_millis(2500);
    let mut rig = build_script_marker(false);
    rig.calls
        .lose
        .store(1, std::sync::atomic::Ordering::Release);
    let id = (rig.start)(&rig);
    rig.settle_unanswered(&id, patience).await;
    rig.restart().await;
    assert_eq!((rig.start)(&rig), id);
    rig.wake(&id);
    let run = rig.settle_unanswered(&id, patience).await;
    let effect = run
        .effects
        .values()
        .find(|e| e.question.is_some() && e.answer.is_none())
        .expect("an operator question");
    let owner = format!("{}:script:{}", effect.key.fence.run, effect.key.digest);
    let name: String = owner
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let record = rig
        .owner
        .store
        .database_path()
        .with_extension("processes")
        .join(name);
    let mut survivor = std::process::Command::new("sleep")
        .arg("30")
        .process_group(0)
        .spawn()
        .unwrap();
    let survivor_id = survivor.id();
    std::fs::create_dir_all(record.parent().unwrap()).unwrap();
    std::fs::write(&record, survivor_id.to_string()).unwrap();
    rig.answer_open(&id, true);
    let run = rig.settle_unanswered(&id, patience).await;
    survivor.kill().unwrap();
    survivor.wait().unwrap();
    let runs = std::fs::read_to_string(std::path::Path::new(&rig.project).join("script-runs.log"))
        .unwrap()
        .lines()
        .count();
    assert_eq!(runs, 1, "rerun started beside a live original: {run:?}");
    // The refusal goes through the ledger as a new question that names the group and
    // asks the operator again; the run neither fails nor waits without a question.
    let effect = run
        .effects
        .values()
        .find(|e| e.question.is_some() && e.answer.is_none())
        .unwrap_or_else(|| panic!("refused rerun left no operator question: {run:?}"));
    let question = effect.question.as_ref().unwrap();
    assert_eq!(question.revision, 2, "{question:?}");
    assert!(
        question.title.contains("may still be running")
            && question.title.contains(&survivor_id.to_string()),
        "{question:?}"
    );
    assert_eq!(run.status, NeutralStatus::Running);
    rig.answer_open(&id, false);
    let run = rig.settle_unanswered(&id, patience).await;
    assert!(
        matches!(&run.status, NeutralStatus::Failed { diagnostic } if diagnostic.contains("abandoned")),
        "{:?}",
        run.status
    );
    assert_eq!(runs_of(&rig), 1);
}

#[cfg(unix)]
fn runs_of(rig: &super::rig::Rig) -> usize {
    std::fs::read_to_string(std::path::Path::new(&rig.project).join("script-runs.log"))
        .unwrap()
        .lines()
        .count()
}

// Catches: a leftover record of an earlier attempt (its group long gone) blocks a
// legitimate rerun, or the record outlives the settled effect and misleads a later one.
#[cfg(unix)]
#[tokio::test]
async fn a_stale_process_record_neither_blocks_a_rerun_nor_outlives_the_effect() {
    use super::run::neutral_execution::NeutralStatus;
    let patience = std::time::Duration::from_millis(2500);
    let mut rig = build_script_marker(false);
    rig.calls
        .lose
        .store(1, std::sync::atomic::Ordering::Release);
    let id = (rig.start)(&rig);
    rig.settle_unanswered(&id, patience).await;
    rig.restart().await;
    assert_eq!((rig.start)(&rig), id);
    rig.wake(&id);
    let run = rig.settle_unanswered(&id, patience).await;
    let effect = run.effects.values().next().unwrap();
    let name: String = format!("{}:script:{}", effect.key.fence.run, effect.key.digest)
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let record = rig
        .owner
        .store
        .database_path()
        .with_extension("processes")
        .join(name);
    // A process that ran and was reaped: its group id is gone.
    let mut done = std::process::Command::new("true").spawn().unwrap();
    let dead = done.id();
    done.wait().unwrap();
    std::fs::create_dir_all(record.parent().unwrap()).unwrap();
    std::fs::write(&record, dead.to_string()).unwrap();
    rig.answer_open(&id, true);
    let run = rig.settle_unanswered(&id, patience).await;
    assert!(
        matches!(run.status, NeutralStatus::Completed { .. }),
        "{:?}",
        run.status
    );
    assert_eq!(runs_of(&rig), 2);
    assert!(!record.exists(), "the record outlived the settled effect");
}
