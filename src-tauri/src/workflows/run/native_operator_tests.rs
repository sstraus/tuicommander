use super::native_operator::*;
use super::native_test_support::Fixture;
use super::neutral_effects::{EffectCommand, EffectOutcome};
use super::neutral_execution::{NeutralExecution, NeutralResult};
use super::neutral_runtime::NeutralHost;
use crate::workflows::neutral_host::NeutralCatalogHost;
use serde_json::json;

fn fixture() -> Fixture {
    let mut f = Fixture::new();
    f.context = super::NativeContext::capture(
        &f.state,
        super::NativeCaller::Owner,
        f.context.workspace_path(),
        vec!["exact-tree".into()],
    )
    .unwrap();
    f
}
fn request(f: &Fixture) -> OperatorRequest {
    OperatorRequest {
        source_node: "work".into(),
        resume_to: "done".into(),
        source_generation: None,
        artifact: OperatorArtifact {
            commit: super::check::git_output(f.context.workspace_path(), &["rev-parse", "HEAD"])
                .unwrap(),
            tree: super::check::git_output(f.context.workspace_path(), &["write-tree"]).unwrap(),
        },
        decision: OperatorKind::Resume,
    }
}
fn start(f: &Fixture, r: &OperatorRequest, automatic: bool) -> NeutralExecution {
    f.start_custom(
        if automatic {
            "operator.automatic_close"
        } else {
            "operator.wait"
        },
        json!(r),
        |d| {
            d.control.edges.insert(
                "work".into(),
                [
                    ("resumed".into(), "done".into()),
                    ("declined".into(), "done".into()),
                ]
                .into(),
            );
        },
    )
}
fn answer(f: &Fixture, run: &NeutralExecution, r: &OperatorRequest, accept: bool) -> EffectCommand {
    let effect = run.effects.values().next().unwrap();
    f.host
        .operator_answer(
            &f.owner.store,
            &run.run.id,
            &effect.key,
            1,
            run.sequence,
            &OperatorAnswer {
                request: r.clone(),
                accept,
                reason: "Explicit exact decision".into(),
            },
            Some(&super::NativeCaller::Owner),
        )
        .unwrap()
}
// Catches bool approval masquerading as resume, tampered continuation/subject,
#[tokio::test]
async fn operator_wait_retains_exact_typed_answer_and_only_fixed_continuation() {
    let f = fixture();
    let r = request(&f);
    let run = start(&f, &r, false);
    let wait = f.dispatch(&run).await;
    let effect = wait.effects.values().next().unwrap();
    assert!(!effect.dispatched && effect.answer.is_none());
    let reopened = super::RunStore::open_at(f.owner.store.database_path())
        .unwrap()
        .neutral_execution(&run.run.id)
        .unwrap();
    assert_eq!(reopened.effect(&effect.key).unwrap(), effect);
    assert!(
        f.host
            .authorize_effect_answer(
                &f.owner.store,
                &run.run.id,
                &effect.key,
                Some(&super::NativeCaller::Owner)
            )
            .unwrap_err()
            .contains("typed")
    );
    let mut altered = r.clone();
    altered.resume_to = "start".into();
    assert!(
        f.host
            .operator_answer(
                &f.owner.store,
                &run.run.id,
                &effect.key,
                1,
                wait.sequence,
                &OperatorAnswer {
                    request: altered,
                    accept: true,
                    reason: "wrong target".into()
                },
                Some(&super::NativeCaller::Owner)
            )
            .is_err()
    );
    assert!(
        f.host
            .operator_answer(
                &f.owner.store,
                &run.run.id,
                &effect.key,
                1,
                wait.sequence,
                &OperatorAnswer {
                    request: r.clone(),
                    accept: true,
                    reason: "not human".into()
                },
                Some(&super::NativeCaller::Agent("trusted".into()))
            )
            .is_err()
    );
    let command = answer(&f, &wait, &r, true);
    let approved = f
        .owner
        .neutral_effect_command(&run.run.id, command)
        .unwrap();
    let retained = super::RunStore::open_at(f.owner.store.database_path())
        .unwrap()
        .neutral_execution(&run.run.id)
        .unwrap();
    assert!(
        retained
            .effect(&effect.key)
            .unwrap()
            .answer_receipt
            .is_some()
    );
    assert!(
        f.host
            .operator_answer(
                &f.owner.store,
                &run.run.id,
                &effect.key,
                1,
                wait.sequence,
                &OperatorAnswer {
                    request: r.clone(),
                    accept: true,
                    reason: "duplicate".into()
                },
                Some(&super::NativeCaller::Owner)
            )
            .is_err()
    );
    let observed = f.dispatch(&approved).await;
    let done = observed.effect(&effect.key).unwrap();
    let EffectOutcome::Completed {
        result: NeutralResult::Completed { outcome, outputs },
    } = &done.observation.as_ref().unwrap().outcome
    else {
        panic!("typed resume did not complete")
    };
    assert_eq!(outcome, "resumed");
    assert_eq!(outputs["receipt"]["result"]["continuation"], "done");
    assert_eq!(outputs["receipt"]["answer"]["actor"], "owner");
    assert_eq!(outputs["receipt"]["answer"]["sequence"], wait.sequence);
    let (finished, _) = f.owner.drive_neutral_turn(&run.run.id).unwrap();
    assert_eq!(finished.node, "done");
    assert!(matches!(
        finished.status,
        super::neutral_execution::NeutralStatus::Completed { .. }
    ));
    assert_eq!(finished.publication, run.publication);
    let second = f.dispatch(&start(&f, &r, false)).await;
    assert!(
        f.host
            .operator_answer(
                &f.owner.store,
                &second.run.id,
                &effect.key,
                1,
                second.sequence,
                &OperatorAnswer {
                    request: r,
                    accept: true,
                    reason: "foreign wait".into()
                },
                Some(&super::NativeCaller::Owner)
            )
            .is_err()
    );
}
// Catches internal generic approvals bypassing typed intent and readiness/model
// labels granting automatic closure without actual independent/check evidence.
#[tokio::test]
async fn operator_generic_approval_and_automatic_missing_proof_fail_closed() {
    let f = fixture();
    let r = request(&f);
    let wait = f.dispatch(&start(&f, &r, false)).await;
    let approved = f.approve(&wait);
    let observed = f.dispatch(&approved).await;
    assert!(
        matches!(&observed.effects.values().next().unwrap().observation.as_ref().unwrap().outcome,
        EffectOutcome::Completed{result:NeutralResult::Failed{diagnostic}} if diagnostic.contains("typed authenticated"))
    );
    let mut automatic = r.clone();
    automatic.decision = OperatorKind::AutomaticClosure {
        story: "unrelated".into(),
        revision: 1,
        proof: ClosureProof {
            base: r.artifact.commit.clone(),
        },
    };
    let run = start(&f, &automatic, true);
    let observed = f.dispatch(&run).await;
    assert!(
        matches!(&observed.effects.values().next().unwrap().observation.as_ref().unwrap().outcome,
        EffectOutcome::Completed{result:NeutralResult::Failed{diagnostic}} if diagnostic.contains("current stopped independent review"))
    );
}
// Catches a stored human answer authorizing a replacement artifact, and loss of
// an original pure resume reply causing a new side effect rather than reconciliation.
#[tokio::test]
async fn operator_answer_fences_artifact_and_reconciles_original_resume() {
    let f = fixture();
    let r = request(&f);
    let wait = f.dispatch(&start(&f, &r, false)).await;
    let approved = f
        .owner
        .neutral_effect_command(&wait.run.id, answer(&f, &wait, &r, false))
        .unwrap();
    let key = approved.effects.values().next().unwrap().key.clone();
    let dispatched = f
        .owner
        .neutral_effect_command(&wait.run.id, EffectCommand::Dispatch { key: key.clone() })
        .unwrap();
    let observation = f
        .host
        .reconcile(dispatched.effect(&key).unwrap().clone())
        .await
        .unwrap();
    assert!(
        matches!(observation.outcome,EffectOutcome::Completed{result:NeutralResult::Completed{outcome,..}} if outcome=="declined")
    );
    std::fs::write(
        f.context.workspace_path().join("artifact.txt"),
        "replacement",
    )
    .unwrap();
    assert!(f.host.reconcile(dispatched.effect(&key).unwrap().clone()).await.is_ok_and(|o|matches!(o.outcome,EffectOutcome::Completed{result:NeutralResult::Failed{diagnostic}} if diagnostic.contains("clean worktree"))));
}

#[path = "native_operator_tests/closure.rs"]
mod closure;

// Catches a normalized generation label accepted after a newer source intent
// owns the resource, including the admission-to-cancellation-flush window.
#[test]
fn operator_source_generation_uses_current_retained_trigger_ownership() {
    use super::neutral_trigger_fixture::{fixture, pr_event};
    let f = fixture();
    let mut draft = f.draft.clone();
    draft.kind = "pull_request".into();
    draft.mapping.inputs.insert("head".into(), "head".into());
    f.enable(draft, 0);
    let event = pr_event("1", 'a');
    let id = f.admit(event.clone())[0].run_id.clone().unwrap();
    super::correction_generation::require_source(&f.owner.store, &id, Some("1")).unwrap();
    assert!(super::correction_generation::require_source(&f.owner.store, &id, Some("2")).is_err());
    assert!(super::correction_generation::require_source(&f.owner.store, &id, None).is_err());
    let original = f
        .owner
        .store
        .trigger_original("/project", "binding-a", &event.resource, "1")
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
    assert!(super::correction_generation::require_source(&f.owner.store, &id, Some("1")).is_err());
}
