use super::neutral_effects::{
    EffectCommand, EffectObservation, EffectOutcome, EffectPreparation, NeutralEffect,
};
use super::neutral_execution::{NeutralExecutionCommand as Command, NeutralResult, NeutralStatus};
use super::neutral_runtime::NeutralInvocation;
use super::neutral_trigger_fixture::*;
use serde_json::json;
use std::collections::BTreeMap;

// Catches: superseding an uncertain write releasing its resource, starting every
// intermediate PR head, stale graph work continuing, and actor-wide suppression.
#[test]
fn latest_pr_generation_waits_for_original_effect_certainty_and_stop() {
    let f = fixture();
    let mut draft = f.draft.clone();
    draft.kind = "pull_request".into();
    draft.mapping.inputs.insert("head".into(), "head".into());
    draft.ignored_actors.insert("ignored-bot".into());
    f.enable(draft, 0);
    let first = f.admit(pr_event("1", 'a'))[0].run_id.clone().unwrap();
    for (id, command) in [("enter", Command::Step), ("prepare", Command::Step)] {
        let snapshot = f.owner.store.neutral_execution(&first).unwrap();
        f.owner
            .neutral_command(&first, id, snapshot.sequence, command)
            .unwrap();
    }
    let snapshot = f.owner.store.neutral_execution(&first).unwrap();
    let attempt = snapshot.attempt.as_ref().unwrap();
    let effect = NeutralEffect::prepare(
        NeutralInvocation {
            fence: attempt.fence.clone(),
            publication: snapshot.publication.clone(),
            descriptor: snapshot.descriptor(0).unwrap().clone(),
            instance: snapshot.publication.definition.instances[0].clone(),
            inputs: attempt.inputs.clone(),
            subject: attempt.subject.clone(),
        },
        EffectPreparation {
            mutation: true,
            resources: ["github:octo-org/example/pull_request/1347".into()].into(),
            approval: None,
            model_tools: None,
        },
    )
    .unwrap();
    let key = effect.key.clone();
    f.owner
        .neutral_effect_command(
            &first,
            EffectCommand::Prepare {
                effect: Box::new(effect),
            },
        )
        .unwrap();
    f.owner
        .neutral_effect_command(&first, EffectCommand::Dispatch { key: key.clone() })
        .unwrap();
    f.owner
        .neutral_effect_command(
            &first,
            EffectCommand::Observe {
                key: key.clone(),
                observation: EffectObservation {
                    stopped: false,
                    outcome: EffectOutcome::Uncertain {
                        diagnostic: "response lost".into(),
                    },
                },
            },
        )
        .unwrap();
    assert!(f.admit(pr_event("2", 'b'))[0].run_id.is_none());
    assert!(f.admit(pr_event("3", 'c'))[0].run_id.is_none());
    let cancelled = f.owner.store.neutral_execution(&first).unwrap();
    assert_eq!(cancelled.status, NeutralStatus::Cancelled);
    assert!(!cancelled.effect(&key).unwrap().settled());
    f.owner
        .neutral_effect_command(
            &first,
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
    let latest = f.admit(pr_event("3", 'c'))[0].run_id.clone().unwrap();
    assert_ne!(first, latest);
    assert_eq!(
        f.owner.store.neutral_run(&latest).unwrap().inputs["head"],
        json!("c".repeat(40))
    );
    let original = f
        .owner
        .store
        .trigger_original("/project", "binding-a", &pr_event("2", 'b').resource, "2")
        .unwrap();
    assert!(
        original.admission.run_id.is_none(),
        "coalesced middle head must never start"
    );
    let mut ignored = pr_event("4", 'd');
    ignored.actor = Some("ignored-bot".into());
    assert_eq!(
        f.admit(ignored),
        [] as [crate::workflows::trigger::TriggerAdmission; 0]
    );
    let mut human = pr_event("4", 'd');
    human.actor = Some("a-human".into());
    assert_eq!(f.admit(human).len(), 1, "unrelated actor remains eligible");
}
