use super::correction_driver::{assessment_outputs, command, drive};
use super::correction_fixture::fixture;
use super::neutral_execution::{NeutralExecutionCommand as Command, NeutralStatus};
use super::neutral_execution_tests::step;
use super::neutral_scopes::NeutralScopeKind;
use super::runtime::RuntimeOwner;
use crate::workflows::correction::CorrectionState;
use crate::workflows::correction_region::CorrectionRegion;
use serde_json::json;
use std::collections::BTreeMap;

// Catches: N counting the initial attempt, zero being invalid/unbounded,
// exhaustion releasing success, or a loop accidentally re-entering setup.
#[test]
fn correction_zero_and_two_are_after_initial_and_repeat_only_selected_region() {
    for (limit, failures, expected_bodies, expected_outcome) in [
        (0, 4, 1, "operator"),
        (2, 4, 3, "operator"),
        (2, 1, 2, "success"),
    ] {
        let (_dir, owner, mut run, at) = fixture(limit);
        let mut setup = 0;
        let mut bodies = 0;
        loop {
            run = drive(&owner, run, &mut setup, at);
            if run.status != NeutralStatus::Running {
                break;
            }
            assert!(bodies < 4, "unbounded corrections");
            let outputs = assessment_outputs(&run, bodies < failures);
            bodies += 1;
            run = command(
                &owner,
                &run,
                Command::Complete {
                    fence: run.pending_invocations()[0].1.fence.clone(),
                    outcome: "assessed".into(),
                    outputs,
                },
                at,
            );
        }
        assert_eq!(bodies, expected_bodies);
        assert_eq!(setup, 3, "parent issue/workspace/PR must each execute once");
        assert_eq!(
            run.status,
            NeutralStatus::Completed {
                outcome: expected_outcome.into()
            }
        );
    }
}

// Catches: recovery re-dispatch or a new worker/head replenishing the correction
// allowance, duplicate CI completion spending twice, and old approval carryover.
#[test]
fn correction_admission_survives_head_worker_and_reopen_without_double_spend() {
    let (dir, owner, run, at) = fixture(2);
    let mut setup = 0;
    let run = drive(&owner, run, &mut setup, at);
    let old_fence = run.pending_invocations()[0].1.fence.clone();
    let complete = Command::Complete {
        fence: old_fence.clone(),
        outcome: "assessed".into(),
        outputs: assessment_outputs(&run, true),
    };
    let complete_id = format!("command:{}", run.sequence);
    let completed = command(&owner, &run, complete.clone(), at);
    let mut run = completed.clone();
    // Return through the selected body to the durable enclosing loop.
    for _ in 0..5 {
        if run.publication.id == "bounded-correction" {
            break;
        }
        run = step(&owner, &run, &format!("return:{}", run.sequence), at);
    }
    assert_eq!(run.publication.id, "bounded-correction");
    let identity = run.active_scope.as_ref().unwrap().identity.clone();
    let admission_id = format!("admit:{}", run.sequence);
    run = step(&owner, &run, &admission_id, at);
    let scope = run.active_scope.as_ref().unwrap();
    assert!(matches!(
        scope.kind,
        NeutralScopeKind::Loop { iteration: 2, .. }
    ));
    assert!(
        scope.slots["2"].attempt.is_none(),
        "restart window is after admission, before dispatch"
    );
    drop(owner);
    let replacement = RuntimeOwner::acquire(&dir.path().join("runs.sqlite3")).unwrap();
    assert_eq!(
        replacement.store.neutral_execution(&run.run.id).unwrap(),
        run
    );
    assert_eq!(
        replacement
            .neutral_command_at(&run.run.id, &admission_id, 0, Command::Step, at)
            .unwrap(),
        run
    );
    assert_eq!(
        replacement
            .neutral_command_at(&run.run.id, &complete_id, 0, complete, at)
            .unwrap(),
        completed
    );
    assert_eq!(
        replacement.store.neutral_execution(&run.run.id).unwrap(),
        run,
        "duplicate result cannot mutate current owner"
    );
    let mut run = drive(&replacement, run, &mut setup, at);
    let state: CorrectionState =
        serde_json::from_value(run.pending_invocations()[0].1.inputs["state"].clone()).unwrap();
    assert_eq!(state.artifact.generation, 2);
    assert!(state.approval.is_none());
    assert_eq!(
        state.evidence,
        [] as [crate::workflows::correction::CorrectionReceipt; 0]
    );
    assert_eq!(state.seen.len(), 1);
    assert!(
        replacement
            .neutral_command_at(
                &run.run.id,
                "stale-ci",
                run.sequence,
                Command::Complete {
                    fence: old_fence,
                    outcome: "assessed".into(),
                    outputs: BTreeMap::from([
                        ("state".into(), json!(state)),
                        ("repeat".into(), json!(false))
                    ])
                },
                at
            )
            .is_err()
    );
    for _ in 0..2 {
        let outputs = assessment_outputs(&run, true);
        run = command(
            &replacement,
            &run,
            Command::Complete {
                fence: run.pending_invocations()[0].1.fence.clone(),
                outcome: "assessed".into(),
                outputs,
            },
            at,
        );
        run = drive(&replacement, run, &mut setup, at);
    }
    assert_eq!(setup, 3);
    assert_eq!(
        run.status,
        NeutralStatus::Completed {
            outcome: "operator".into()
        }
    );
    // The scope identity is in the retained subflow invocation, independent of
    // both changing leaf fences and artifacts. Reopen did not create a new run.
    assert_eq!(identity.run, run.run.id);
}

// Catches: ambiguity becoming false/success, and technical failure being
// converted into either a successful assessment or an automatic correction.
#[test]
fn correction_unknown_and_technical_failure_never_release_success() {
    for technical in [false, true] {
        let (_dir, owner, run, at) = fixture(2);
        let mut setup = 0;
        let run = drive(&owner, run, &mut setup, at);
        let fence = run.pending_invocations()[0].1.fence.clone();
        let command_value = if technical {
            Command::Fail {
                fence,
                diagnostic: "internal activity failed before an assessment".into(),
            }
        } else {
            Command::Complete {
                fence,
                outcome: "assessed".into(),
                outputs: BTreeMap::from([(
                    "state".into(),
                    run.pending_invocations()[0].1.inputs["state"].clone(),
                )]),
            }
        };
        let run = command(&owner, &run, command_value, at);
        let run = drive(&owner, run, &mut setup, at);
        assert_eq!(
            run.status,
            NeutralStatus::Completed {
                outcome: if technical {
                    "technical_failure"
                } else {
                    "operator"
                }
                .into()
            }
        );
        assert_eq!(setup, 3);
    }
}

// Catches: silently wrapping u32::MAX+1 or accepting an opaque script instead
// of an inspectable selected publication.
#[test]
fn correction_region_rejects_overflow_and_unpinned_body() {
    let (_dir, _owner, run, _) = fixture(0);
    let mut catalog = crate::workflows::catalog::WorkflowCatalog::new();
    let root_instance = &run.publication.definition.instances[0];
    catalog
        .register(run.descriptor(0).unwrap().clone())
        .unwrap();
    let region = CorrectionRegion {
        max_corrections: u32::MAX,
        criteria: crate::workflows::correction_tests::policy(),
        selected: root_instance.clone(),
    };
    assert!(
        region
            .definition(&catalog)
            .unwrap_err()
            .contains("overflows")
    );
    assert!(
        CorrectionRegion {
            max_corrections: 2,
            ..region
        }
        .definition(&catalog)
        .unwrap_err()
        .contains("pinned subflow")
    );
}
