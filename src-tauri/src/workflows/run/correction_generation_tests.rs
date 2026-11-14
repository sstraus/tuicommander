use super::neutral_execution::NeutralExecutionCommand as Command;
use super::neutral_trigger_fixture::{fixture, pr_event};
use crate::workflows::correction_tests::artifact;
use serde_json::json;
use std::collections::BTreeMap;

// Catches: the admission/flush window accepting old review or CI business
// completion, or matching only a generation label while ignoring its pinned head.
#[test]
fn correction_refuses_old_trigger_head_before_cancellation_flush() {
    let f = fixture();
    let mut draft = f.draft.clone();
    draft.kind = "pull_request".into();
    draft.mapping.inputs.insert("head".into(), "head".into());
    f.enable(draft, 0);
    let id = f.admit(pr_event("1", 'a'))[0].run_id.clone().unwrap();
    let mut candidate = artifact(1);
    candidate.head = "a".repeat(40);
    candidate.base = "f".repeat(40);
    super::correction_generation::require_artifact(&f.owner.store, &id, &candidate).unwrap();
    candidate.head = "b".repeat(40);
    assert!(
        super::correction_generation::require_artifact(&f.owner.store, &id, &candidate)
            .unwrap_err()
            .contains("pinned trigger")
    );
    for name in ["entry", "prepare"] {
        let run = f.owner.store.neutral_execution(&id).unwrap();
        f.owner
            .neutral_command(&id, name, run.sequence, Command::Step)
            .unwrap();
    }
    let original = f
        .owner
        .store
        .trigger_original("/project", "binding-a", &pr_event("1", 'a').resource, "1")
        .unwrap();
    f.owner
        .store
        .reserve_trigger(
            "/project",
            "binding-a",
            pr_event("2", 'b'),
            original.request,
            "",
        )
        .unwrap();
    let before = f.owner.store.neutral_execution(&id).unwrap();
    assert!(
        f.owner
            .neutral_command(
                &id,
                "old-review",
                before.sequence,
                Command::Complete {
                    fence: before.attempt.as_ref().unwrap().fence.clone(),
                    outcome: "ok".into(),
                    outputs: BTreeMap::from([("next".into(), json!(7))]),
                }
            )
            .unwrap_err()
            .contains("generation superseded")
    );
    assert_eq!(f.owner.store.neutral_execution(&id).unwrap(), before);
    candidate.head = "a".repeat(40);
    assert!(
        super::correction_generation::require_artifact(&f.owner.store, &id, &candidate)
            .unwrap_err()
            .contains("generation superseded")
    );
    f.owner
        .neutral_command(&id, "cancel", before.sequence, Command::Cancel)
        .unwrap();
}

// Catches: refusing stale business results by dropping stop evidence and leaving
// the owned resource locked forever, or settling it by advancing the old graph.
#[test]
fn correction_stale_effect_settles_without_advancing_old_generation() {
    use super::neutral_effects::{
        EffectCommand, EffectObservation, EffectOutcome, EffectPreparation, NeutralEffect,
    };
    use super::neutral_execution::{NeutralResult, NeutralStatus};
    use super::neutral_runtime::NeutralInvocation;
    let f = fixture();
    let mut draft = f.draft.clone();
    draft.kind = "pull_request".into();
    draft.mapping.inputs.insert("head".into(), "head".into());
    f.enable(draft, 0);
    let id = f.admit(pr_event("1", 'a'))[0].run_id.clone().unwrap();
    for name in ["entry", "prepare"] {
        let run = f.owner.store.neutral_execution(&id).unwrap();
        f.owner
            .neutral_command(&id, name, run.sequence, Command::Step)
            .unwrap();
    }
    let run = f.owner.store.neutral_execution(&id).unwrap();
    let attempt = run.attempt.as_ref().unwrap();
    let effect = NeutralEffect::prepare(
        NeutralInvocation {
            fence: attempt.fence.clone(),
            publication: run.publication.clone(),
            descriptor: run.descriptor(0).unwrap().clone(),
            instance: run.publication.definition.instances[0].clone(),
            inputs: attempt.inputs.clone(),
            subject: attempt.subject.clone(),
        },
        EffectPreparation {
            mutation: true,
            resources: ["correction-resource".into()].into(),
            approval: None,
            model_tools: None,
        },
    )
    .unwrap();
    let key = effect.key.clone();
    f.owner
        .neutral_effect_command(
            &id,
            EffectCommand::Prepare {
                effect: Box::new(effect),
            },
        )
        .unwrap();
    f.owner
        .neutral_effect_command(&id, EffectCommand::Dispatch { key: key.clone() })
        .unwrap();
    let original = f
        .owner
        .store
        .trigger_original("/project", "binding-a", &pr_event("1", 'a').resource, "1")
        .unwrap();
    f.owner
        .store
        .reserve_trigger(
            "/project",
            "binding-a",
            pr_event("2", 'b'),
            original.request,
            "",
        )
        .unwrap();
    let after = f
        .owner
        .neutral_effect_command(
            &id,
            EffectCommand::Observe {
                key: key.clone(),
                observation: EffectObservation {
                    stopped: true,
                    outcome: EffectOutcome::Completed {
                        result: NeutralResult::Completed {
                            outcome: "ok".into(),
                            outputs: BTreeMap::from([("next".into(), json!(7))]),
                        },
                    },
                },
            },
        )
        .unwrap();
    assert_eq!(after.status, NeutralStatus::Cancelled);
    assert!(after.effect(&key).unwrap().settled());
    assert_eq!(f.owner.store.neutral_execution(&id).unwrap(), after);
    let count: i64 = f
        .owner
        .store
        .connect()
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM neutral_effect_resources WHERE resource='correction-resource'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0, "stale stop must release ownership");
    let latest = f.admit(pr_event("2", 'b'))[0].run_id.clone().unwrap();
    assert_ne!(latest, id);
}
