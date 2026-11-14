use super::super::{
    native_host::NativeBinding,
    native_review::{ReviewRequest, review_with},
    neutral_effects::{EffectObservation, NeutralEffect},
    neutral_runtime::{NeutralInvocation, admitted},
};
use super::*;
use crate::stories::{NewPlan, NewStory, StoryCommand, StoryOrigin, StoryStatus, StoryStore};
use crate::workflows::review_verdict::CodeVerdict;
use crate::workflows::{
    catalog::{ActivityInstance, DescriptorId},
    data_binding::DataSource,
    neutral_control::ControlNode,
};
use std::collections::BTreeMap;

fn claim(f: &Fixture, run: &NeutralExecution) -> NeutralEffect {
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
    let run = f
        .owner
        .neutral_effect_command(
            &run.run.id,
            EffectCommand::Dispatch {
                key: effect.key.clone(),
            },
        )
        .unwrap();
    run.effect(&effect.key).unwrap().clone()
}
fn story(f: &Fixture, backend: &StoryStore) -> crate::stories::Story {
    let plan = backend
        .create_plan(NewPlan {
            project: f.context.workspace_path().to_string_lossy().into_owned(),
            title: "Closure fixture".into(),
            source: "fixture".into(),
        })
        .unwrap();
    let mut story = backend
        .create_story(NewStory {
            plan_id: plan.id,
            title: "Close exact reviewed artifact".into(),
            criteria: vec!["Verified".into()],
            priority: 2,
            story_type: None,
            origin: StoryOrigin::Native,
            file_scope: vec![],
        })
        .unwrap();
    for command in [
        StoryCommand::StartManual,
        StoryCommand::CheckCriterion(0),
        StoryCommand::SubmitReview,
    ] {
        story = backend
            .transition(&story.id, story.revision, command)
            .unwrap();
    }
    story
}
// Catches an actual passing review closing with a missing configured check, or
// a positive native closure bypassing the selected backend revision/status rules.
#[tokio::test]
async fn operator_automatic_closure_requires_actual_review_and_every_check() {
    for (include_check, verdict) in [
        (false, CodeVerdict::Pass),
        (true, CodeVerdict::Findings),
        (true, CodeVerdict::Pass),
    ] {
        let f = fixture();
        let backend = StoryStore::open_at(&f.dir.path().join("closure-stories.sqlite3")).unwrap();
        let story = story(&f, &backend);
        let mut r = request(&f);
        let base = r.artifact.commit.clone();
        std::fs::write(
            f.context.workspace_path().join("artifact.txt"),
            "reviewed candidate",
        )
        .unwrap();
        super::super::check::git_output(f.context.workspace_path(), &["add", "."]).unwrap();
        super::super::check::git_output(f.context.workspace_path(), &["commit", "-m", "candidate"])
            .unwrap();
        r.artifact = request(&f).artifact;
        r.source_node = "close".into();
        r.decision = OperatorKind::AutomaticClosure {
            story: story.id.clone(),
            revision: story.revision,
            proof: ClosureProof { base: base.clone() },
        };
        let review = ReviewRequest {
            base,
            head: Some(r.artifact.commit.clone()),
            tree: r.artifact.tree.clone(),
        };
        let check = json!({"check":"exact-tree","commit":r.artifact.commit,"tree":r.artifact.tree});
        let run = f.start_custom("operator.automatic_close", json!(r), |d| {
            let operator = d.instances[0].clone();
            let native = |name: &str| ActivityInstance {
                descriptor: DescriptorId {
                    namespace: "native".into(),
                    name: name.into(),
                    version: 1,
                },
                config: BTreeMap::new(),
                execution_policy: "native".into(),
            };
            d.instances = if include_check {
                vec![
                    native("review.change"),
                    native("verification.run"),
                    operator,
                ]
            } else {
                vec![native("review.change"), operator]
            };
            d.input_bindings = BTreeMap::from([
                (
                    0,
                    BTreeMap::from([(
                        "request".into(),
                        DataSource::Constant {
                            value: json!(review),
                        },
                    )]),
                ),
                (
                    1,
                    BTreeMap::from([(
                        "request".into(),
                        DataSource::Constant {
                            value: check.clone(),
                        },
                    )]),
                ),
                (
                    2,
                    BTreeMap::from([(
                        "request".into(),
                        DataSource::WorkflowInput {
                            field: "request".into(),
                        },
                    )]),
                ),
            ]);
            if include_check {
                d.control
                    .nodes
                    .insert("verify".into(), ControlNode::Activity { activity: 1 });
            } else {
                let binding = d.input_bindings.remove(&2).unwrap();
                d.input_bindings.insert(1, binding);
            }
            d.control.nodes.insert(
                "close".into(),
                ControlNode::Activity {
                    activity: if include_check { 2 } else { 1 },
                },
            );
            d.control.edges.insert(
                "work".into(),
                BTreeMap::from([(
                    "ok".into(),
                    if include_check { "verify" } else { "close" }.into(),
                )]),
            );
            if include_check {
                d.control.edges.insert(
                    "verify".into(),
                    BTreeMap::from([("ok".into(), "close".into())]),
                );
            }
            d.control.edges.insert(
                "close".into(),
                BTreeMap::from([
                    ("resumed".into(), "done".into()),
                    ("declined".into(), "done".into()),
                ]),
            );
        });
        let reviewed = claim(&f, &run);
        let receipt = review_with(
            &f.context,
            &f.owner.store,
            &reviewed.key,
            &review,
            |diff| async move {
                assert!(diff.contains("artifact.txt"));
                Ok((
                    verdict,
                    "Independent controlled native reviewer".into(),
                    Some("native-closure-review".into()),
                ))
            },
        )
        .await
        .unwrap();
        f.owner
            .neutral_effect_command(
                &run.run.id,
                EffectCommand::Observe {
                    key: reviewed.key.clone(),
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
        let (mut current, _) = f.owner.drive_neutral_turn(&run.run.id).unwrap();
        if include_check {
            f.dispatch(&current).await;
            current = f.owner.drive_neutral_turn(&run.run.id).unwrap().0;
        }
        let closure = claim(&f, &current);
        let result = super::super::native_operator_actions::close_story(
            &NativeBinding {
                context: f.context.clone(),
                store: f.owner.store.clone(),
            },
            &closure,
            &r,
            &backend,
        );
        if include_check && verdict == CodeVerdict::Pass {
            assert_eq!(result.unwrap()["status"], "done");
            assert_eq!(
                backend.get_story(&story.id).unwrap().status,
                StoryStatus::Done
            );
            assert!(
                super::super::native_operator_actions::close_story(
                    &NativeBinding {
                        context: f.context.clone(),
                        store: f.owner.store.clone()
                    },
                    &closure,
                    &r,
                    &backend
                )
                .is_err()
            );
        } else {
            assert!(
                matches!(result,Err(super::super::native_git::NativeFailure::Refused(reason)) if reason.contains(if include_check {"independent passing review"}else{"every configured"}))
            );
            assert_ne!(
                backend.get_story(&story.id).unwrap().status,
                StoryStatus::Done
            );
        }
    }
}
