use super::check::git_output;
use super::{
    native_test_support::Fixture, neutral_effects::*, neutral_execution::*, neutral_runtime::*,
};
use crate::workflows::review_verdict::CodeVerdict;
use crate::workflows::{
    data_binding::DataSource, neutral_control::ControlNode, review_policy::ReviewAdmissionPolicy,
};
use serde_json::json;
use std::collections::BTreeMap;

fn prepared(f: &Fixture, run: &NeutralExecution) -> NeutralEffect {
    let (activity, attempt) = run.pending_invocations().into_iter().next().unwrap();
    let mut invocation = NeutralInvocation {
        fence: attempt.fence.clone(),
        publication: run.publication.clone(),
        descriptor: run.descriptor(activity).unwrap().clone(),
        instance: run.publication.definition.instances[activity].clone(),
        inputs: attempt.inputs.clone(),
        subject: attempt.subject.clone(),
    };
    admitted(f.host.as_ref(), run, &mut invocation, None).unwrap();
    let effect =
        NeutralEffect::prepare(invocation.clone(), f.host.prepare(&invocation).unwrap()).unwrap();
    f.owner
        .neutral_effect_command(
            &run.run.id,
            EffectCommand::Prepare {
                effect: Box::new(effect.clone()),
            },
        )
        .unwrap();
    f.owner
        .neutral_effect_command(
            &run.run.id,
            EffectCommand::Dispatch {
                key: effect.key.clone(),
            },
        )
        .unwrap();
    effect
}

// Catches replacement reviewers/new artifacts resetting maxReviews, retrying a
// lost reply spending/dispatching twice, and review counts becoming maxRepairs.
#[tokio::test]
async fn bounded_native_review_retains_admissions_across_replacement_and_reopen() {
    let f = Fixture::new();
    let path = f.context.workspace_path();
    let base = git_output(path, &["rev-parse", "HEAD"]).unwrap();
    std::fs::write(path.join("artifact.txt"), "first candidate").unwrap();
    git_output(path, &["add", "."]).unwrap();
    let first = git_output(path, &["write-tree"]).unwrap();
    std::fs::write(path.join("artifact.txt"), "replacement candidate").unwrap();
    git_output(path, &["add", "."]).unwrap();
    let replacement = git_output(path, &["write-tree"]).unwrap();
    std::fs::write(path.join("artifact.txt"), "first candidate").unwrap();
    git_output(path, &["add", "."]).unwrap();
    let policy = ReviewAdmissionPolicy {
        scope: "delivery-review".into(),
        evidence: Default::default(),
    };
    let request = json!({"base":base,"tree":first});
    let mut run = f.start_custom("review.bounded", request.clone(), |d| {
        d.instances[0].config.insert("policy".into(), json!(policy));
        d.instances.push(d.instances[0].clone());
        d.instances.push(d.instances[0].clone());
        for i in 1..3 {
            d.input_bindings.insert(
                i,
                BTreeMap::from([(
                    "request".into(),
                    DataSource::Constant {
                        value: if i == 2 {
                            json!({"base":base,"tree":replacement})
                        } else {
                            request.clone()
                        },
                    },
                )]),
            );
            let node = format!("work{i}");
            d.control
                .nodes
                .insert(node.clone(), ControlNode::Activity { activity: i });
            d.control.edges.insert(
                node,
                BTreeMap::from([
                    ("ok".into(), "done".into()),
                    ("exhausted".into(), "done".into()),
                ]),
            );
        }
        d.control
            .edges
            .get_mut("work")
            .unwrap()
            .insert("ok".into(), "work1".into());
        d.control
            .edges
            .get_mut("work")
            .unwrap()
            .insert("exhausted".into(), "done".into());
        d.control
            .edges
            .get_mut("work1")
            .unwrap()
            .insert("ok".into(), "work2".into());
    });
    let mut calls = 0;
    for i in 0..3 {
        if i == 2 {
            std::fs::write(path.join("artifact.txt"), "replacement candidate").unwrap();
            git_output(path, &["add", "."]).unwrap();
        }
        let effect = prepared(&f, &run);
        let request = serde_json::from_value(effect.invocation.inputs["request"].clone()).unwrap();
        let receipt = super::native_review::review_with(
            &f.context,
            &f.owner.store,
            &effect.key,
            &request,
            |_| {
                calls += 1;
                async move {
                    Ok((
                        CodeVerdict::Findings,
                        "correction needed".into(),
                        Some(format!("replacement-{i}")),
                    ))
                }
            },
        )
        .await;
        if i == 2 {
            assert!(
                matches!(receipt,Err(super::native_git::NativeFailure::Refused(ref why)) if why.contains("budget exhausted"))
            );
            assert_eq!(calls, 2);
            let reopened = super::RunStore::open_at(f.owner.store.database_path()).unwrap();
            let current = reopened.neutral_execution(&run.run.id).unwrap();
            let count =
                super::native_review_policy::accounting(&reopened, &current, &policy).unwrap();
            assert_eq!(count["reviews"], 2);
            assert_eq!(count["max_repairs"], 1);
            assert_eq!(count["rejected_reviews"], 2);
            assert_eq!(count["check_attempts"], 0);
            break;
        }
        let receipt = receipt.unwrap();
        assert_eq!(
            super::native_review_policy::ordinal(&f.owner.store, &effect.key, &policy).unwrap(),
            i + 1
        );
        assert!(
            super::native_review_policy::reserve(&f.context, &f.owner.store, &effect.key).is_err(),
            "lost response must reconcile, not enter a replacement provider"
        );
        let mut altered = policy.clone();
        altered.evidence.max_reviews = 3;
        assert!(
            super::native_review_policy::ordinal(&f.owner.store, &effect.key, &altered).is_err()
        );
        f.owner
            .neutral_effect_command(
                &run.run.id,
                EffectCommand::Observe {
                    key: effect.key,
                    observation: EffectObservation {
                        stopped: true,
                        outcome: EffectOutcome::Completed {
                            result: NeutralResult::Completed {
                                outcome: "ok".into(),
                                outputs: BTreeMap::from([("receipt".into(), json!(receipt))]),
                            },
                        },
                    },
                },
            )
            .unwrap();
        run = f.owner.store.neutral_execution(&run.run.id).unwrap();
        let at = super::store::now_ms();
        run = super::neutral_execution_tests::step(&f.owner, &run, &format!("next-{i}"), at);
        run = super::neutral_execution_tests::step(&f.owner, &run, &format!("ready-{i}"), at);
    }
}
