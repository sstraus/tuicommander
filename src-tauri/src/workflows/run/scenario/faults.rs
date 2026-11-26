//! Fault driver. A durable commit point is one row of `neutral_workflow_events`.
//! A crash after commit `k` is a SQLite trigger that refuses every later event
//! insert, so the owner keeps running but can never commit again; a restart
//! then drops the owner and boots a new one on the same database.
use super::invariants::{self, Evidence};
use super::rig::Rig;
use super::run::neutral_effects::{EffectCommand, EffectObservation, EffectOutcome};
use super::run::neutral_execution::{NeutralExecution, NeutralResult, NeutralStatus};
use super::scenarios::Scenario;
use serde_json::Value;
use std::time::Duration;

const PATIENCE: Duration = Duration::from_millis(400);
/// After a restart a script or model child process may need to start again.
const SLOW: Duration = Duration::from_millis(2500);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Fault {
    None,
    /// Host restart right after durable commit `k`.
    Restart(usize),
    /// Operator cancel right after durable commit `k`.
    Cancel(usize),
    /// The reply of adapter entry `n` (1-based) never arrives.
    LostReply(usize),
    /// Lost reply of entry `n`, then a host restart.
    LostReplyRestart(usize),
    /// Restart after commit `k`, then every recorded callback and answer is delivered again.
    Duplicate(usize),
    /// Cancel after commit `k`, then a late completion callback for every open effect.
    LateCallback(usize),
}

pub(super) struct Outcome {
    pub fault: Fault,
    pub run: NeutralExecution,
    pub events: Vec<Value>,
    /// False when the scenario finished before the fault point was reached.
    pub reached: bool,
    /// The run was left without a caller binding by the restart (needs a resume).
    pub unbound_after_restart: bool,
    /// The run only progressed after a second wake: a lost wake-up after restart.
    pub needed_second_wake: bool,
}

fn events(rig: &Rig, id: &str) -> Vec<Value> {
    rig.owner
        .store
        .neutral_events_after(id, 0, 500)
        .unwrap()
        .iter()
        .map(|e| serde_json::to_value(e).unwrap())
        .collect()
}

fn freeze_after(rig: &Rig, k: usize) {
    rig.owner
        .store
        .connect()
        .unwrap()
        .execute_batch(&format!(
            "CREATE TRIGGER IF NOT EXISTS scenario_freeze BEFORE INSERT ON neutral_workflow_events \
         WHEN NEW.sequence > {k} BEGIN SELECT RAISE(ABORT,'scenario: injected crash'); END;"
        ))
        .unwrap();
}

fn thaw(rig: &Rig) {
    rig.owner
        .store
        .connect()
        .unwrap()
        .execute_batch("DROP TRIGGER IF EXISTS scenario_freeze;")
        .unwrap();
}

fn cancel(rig: &Rig, run: &NeutralExecution) {
    let action = crate::workflows::neutral_api::model::NeutralAction::Cancel {
        run_id: run.run.id.clone(),
        command_id: "scenario-cancel".into(),
        expected_sequence: run.sequence,
    };
    crate::workflows::neutral_api::neutral_action(
        &rig.state,
        &rig.project,
        action,
        Some(super::run::NativeCaller::Owner),
    )
    .unwrap();
}

/// Run `scenario` once with `fault` and check every invariant.
pub(super) async fn execute(
    scenario: &Scenario,
    fault: Fault,
    golden: Option<&Outcome>,
) -> Outcome {
    let mut rig = (scenario.build)();
    let freeze = match fault {
        Fault::Restart(k) | Fault::Cancel(k) | Fault::Duplicate(k) | Fault::LateCallback(k) => {
            Some(k)
        }
        _ => None,
    };
    if let Fault::LostReply(n) | Fault::LostReplyRestart(n) = fault {
        rig.calls
            .lose
            .store(n, std::sync::atomic::Ordering::Release);
    }
    if let Some(k) = freeze {
        freeze_after(&rig, k);
    }
    let id = (rig.start)(&rig);
    let mut unbound = false;
    let mut second_wake = false;
    let mut run = rig.settle(&id, PATIENCE).await;
    let reached =
        freeze.is_none_or(|k| events(&rig, &id).len() == k && run.status == NeutralStatus::Running);
    if reached {
        // Evidence at the crash point, before any recovery action.
        invariants::check_frozen(&rig, &run, fault);
        match fault {
            Fault::Restart(_) | Fault::LostReplyRestart(_) | Fault::Duplicate(_) => {
                rig.restart().await;
                thaw(&rig);
                assert_eq!((rig.start)(&rig), id);
                rig.wake(&id);
                run = rig.settle(&id, SLOW).await;
                if run.status == NeutralStatus::Running && !rig.bound(&id) {
                    unbound = true;
                    if rig.rebind(&id) {
                        rig.wake(&id);
                        run = rig.settle(&id, SLOW).await;
                    }
                }
            }
            Fault::Cancel(_) | Fault::LateCallback(_) => {
                thaw(&rig);
                cancel(&rig, &run);
                run = rig.settle(&id, PATIENCE).await;
                if matches!(fault, Fault::LateCallback(_)) {
                    for effect in run.effects.values().filter(|e| e.observation.is_none()) {
                        let late = EffectCommand::Observe {
                            key: effect.key.clone(),
                            observation: EffectObservation {
                                stopped: true,
                                outcome: EffectOutcome::Completed {
                                    result: NeutralResult::Failed {
                                        diagnostic: "late callback".into(),
                                    },
                                },
                            },
                        };
                        let _ = rig.owner.neutral_effect_command(&id, late);
                    }
                    run = rig.settle(&id, PATIENCE).await;
                }
            }
            _ => {}
        }
    } else if matches!(fault, Fault::LostReplyRestart(_)) {
        rig.restart().await;
        assert_eq!((rig.start)(&rig), id);
        rig.wake(&id);
        run = rig.settle(&id, SLOW).await;
        if run.status == NeutralStatus::Running && !rig.bound(&id) {
            unbound = true;
            if rig.rebind(&id) {
                rig.wake(&id);
                run = rig.settle(&id, SLOW).await;
            }
        }
    }
    if matches!(
        fault,
        Fault::Restart(_) | Fault::LostReplyRestart(_) | Fault::Duplicate(_)
    ) && run.status == NeutralStatus::Running
        && rig.bound(&id)
        && !run
            .effects
            .values()
            .any(|e| e.question.is_some() && e.answer.is_none())
    {
        let before = run.sequence;
        rig.wake(&id);
        run = rig.settle(&id, SLOW).await;
        second_wake = run.sequence != before;
    }
    if matches!(fault, Fault::Duplicate(_)) && reached {
        // Every committed callback and answer is delivered again after recovery.
        let before = events(&rig, &id).len();
        for event in events(&rig, &id) {
            if let Some(command) = event["kind"]["command"].get("command")
                && let Ok(command) = serde_json::from_value::<EffectCommand>(command.clone())
            {
                let _ = rig.owner.neutral_effect_command(&id, command);
            }
        }
        assert_eq!(
            events(&rig, &id).len(),
            before,
            "duplicate delivery committed an event"
        );
    }
    let run = rig.owner.store.neutral_execution(&id).unwrap_or(run);
    let outcome = Outcome {
        fault,
        events: events(&rig, &id),
        run,
        reached,
        unbound_after_restart: unbound,
        needed_second_wake: second_wake,
    };
    if std::env::var_os("SCENARIO_DUMP").is_some() {
        for e in &outcome.events {
            eprintln!("EV {} {}", e["sequence"], e["kind"]);
        }
    }
    invariants::check_final(
        &rig,
        &Evidence {
            outcome: &outcome,
            golden: golden.map(|g| &g.run),
            canon: scenario.canon,
        },
    );
    outcome
}
