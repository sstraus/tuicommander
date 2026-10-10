use super::*;

struct IndependentRuns {
    broken_id: String,
}

impl CompletionEffects for IndependentRuns {
    fn observe(&self, _: &AutomationRun) -> Observation {
        Observation {
            live: true,
            ..Default::default()
        }
    }

    fn stop(&self, run: &AutomationRun) -> Result<(), String> {
        if run.id == self.broken_id {
            Err("owned process could not be stopped".into())
        } else {
            Ok(())
        }
    }

    fn publish(&self, _: &AutomationRun) {}
}

// Catches: one cancellation error prevents every later run from ever expiring.
#[test]
fn cancellation_error_does_not_leave_another_run_past_its_deadline() {
    let (_dir, owner, first) = setup();
    let mut definition = first.definition.clone();
    definition.id = "independent".into();
    let second = owner
        .store()
        .reserve(&definition, RunTrigger::Manual, 101)
        .unwrap()
        .unwrap();
    owner.store().claim_dispatch(&second.id, 151).unwrap();
    owner
        .store()
        .transition(
            &second.id,
            RunStatus::Running,
            RunDetails {
                session_id: Some("independent-owned-session".into()),
                ..Default::default()
            },
            201,
        )
        .unwrap();

    let effects = IndependentRuns {
        broken_id: first.id.clone(),
    };
    let _ = reconcile(owner.store(), &effects, 1200);

    assert_eq!(
        owner.store().get(&first.id).unwrap().status,
        RunStatus::Running
    );
    assert_eq!(
        owner.store().get(&second.id).unwrap().status,
        RunStatus::TimedOut,
        "a different run's stop error must not disable this run's maximum duration"
    );
}
