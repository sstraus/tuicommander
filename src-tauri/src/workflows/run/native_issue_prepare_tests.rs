//! Controlled provider observations, real source admission and backend writes.
use super::native_issue_prepare::*;
use super::native_issue_tests::{admission_with_issue, api};
use super::native_test_support::Fixture;
use super::neutral_effects::{EffectCommand, EffectObservation, EffectOutcome, NeutralEffect};
use super::neutral_execution::{NeutralExecution, NeutralResult};
use super::neutral_runtime::{NeutralHost, NeutralInvocation};
use crate::stories::StoryStore;
use crate::workflows::review_verdict::TriageOutcome;
use crate::workflows::{
    catalog::ActivityInstance, data_binding::DataSource, neutral_control::ControlNode,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(super) fn dispatch(f: &Fixture, run: &NeutralExecution) -> NeutralEffect {
    let (activity, attempt) = run.pending_invocations().into_iter().next().unwrap();
    let invocation = NeutralInvocation {
        fence: attempt.fence.clone(),
        publication: run.publication.clone(),
        descriptor: run.descriptor(activity).unwrap().clone(),
        instance: run.publication.definition.instances[activity].clone(),
        inputs: attempt.inputs.clone(),
        subject: attempt.subject.clone(),
    };
    let effect =
        NeutralEffect::prepare(invocation.clone(), f.host.prepare(&invocation).unwrap()).unwrap();
    let run = f
        .owner
        .neutral_effect_command(
            &run.run.id,
            EffectCommand::Prepare {
                effect: Box::new(effect.clone()),
            },
        )
        .unwrap();
    let run = if let Some(question) = &effect.question {
        f.owner
            .neutral_effect_command(
                &run.run.id,
                EffectCommand::Answer {
                    key: effect.key.clone(),
                    revision: question.revision,
                    approved: true,
                },
            )
            .unwrap()
    } else {
        run
    };
    f.owner
        .neutral_effect_command(
            &run.run.id,
            EffectCommand::Dispatch {
                key: effect.key.clone(),
            },
        )
        .unwrap()
        .effect(&effect.key)
        .unwrap()
        .clone()
}
fn observe(f: &Fixture, effect: &NeutralEffect, receipt: Value) -> NeutralExecution {
    f.owner
        .neutral_effect_command(
            &effect.key.fence.run,
            EffectCommand::Observe {
                key: effect.key.clone(),
                observation: EffectObservation {
                    stopped: true,
                    outcome: EffectOutcome::Completed {
                        result: NeutralResult::Completed {
                            outcome: "ok".into(),
                            outputs: BTreeMap::from([("receipt".into(), receipt)]),
                        },
                    },
                },
            },
        )
        .unwrap()
}
pub(super) fn next(f: &Fixture, mut run: NeutralExecution) -> NeutralExecution {
    while run.pending_invocations().is_empty() {
        assert_eq!(
            run.status,
            super::neutral_execution::NeutralStatus::Running,
            "unexpected terminal at {}: {:?}",
            run.node,
            run.status
        );
        let id = format!("advance:{}", run.sequence);
        run = super::neutral_execution_tests::step(&f.owner, &run, &id, super::store::now_ms());
    }
    run
}

// Catches: issue content lost behind IDs, forged read material, or ambiguous
// preparation replay creating another plan. The read and model observations are
// controlled from the recorded REST fixture; no live-provider claim is made.
#[tokio::test]
async fn native_issue_prepare_preserves_actual_material_and_reuses_owned_plan() {
    for body in [None, Some("x".repeat(20_001)), Some("界".repeat(20_001))] {
        prepare_material_case(body, TriageOutcome::Accepted).await;
    }
}

// Catches: the composed handoff preparing a plan/story for a rejected or
// needs-input issue, or treating the accepted label already on the issue as
// authority in place of this run's accepted triage receipt.
#[tokio::test]
async fn native_issue_prepare_refuses_non_accepted_triage_despite_accepted_label() {
    for outcome in [TriageOutcome::Rejected, TriageOutcome::NeedsInput] {
        prepare_material_case(None, outcome).await;
    }
}

async fn prepare_material_case(body: Option<String>, outcome: TriageOutcome) {
    let f = Fixture::new();
    // Extend the recorded REST fixture at the material-cap boundary, then derive
    // its true admitted digest through production normalization.
    let mut issue = api().issue;
    if let Some(body) = body {
        issue["body"] = json!(body);
    }
    if outcome != TriageOutcome::Accepted {
        issue["labels"]
            .as_array_mut()
            .unwrap()
            .push(json!({"name": "accepted"}));
    }
    let (source, intent, bound) = admission_with_issue(&f, &issue);
    let profile = super::native_issue_service::IssueProfile {
        profile: "fixture".into(),
        model: "fixture".into(),
        effort: "high".into(),
        instructions: "Evaluate requirements".into(),
    };
    let triage_request = super::native_issue_triage::IssueTriageRequest {
        source: source.clone(),
        profile: profile.clone(),
        policy: super::native_issue_service::IssuePolicy {
            accepted_label: "accepted".into(),
            rejection_label: "rejected".into(),
            needs_input_comment: true,
        },
    };
    let run = f.start_custom("issue.read", json!({}), |d| {
        for name in ["issue.triage", "issue.prepare"] {
            d.instances.push(ActivityInstance {
                descriptor: crate::workflows::catalog::DescriptorId {
                    namespace: "native".into(),
                    name: name.into(),
                    version: 1,
                },
                config: BTreeMap::new(),
                execution_policy: "native".into(),
            });
        }
        d.input_bindings.insert(
            1,
            BTreeMap::from([(
                "request".into(),
                DataSource::Constant {
                    value: json!(triage_request),
                },
            )]),
        );
        d.input_bindings.insert(
            2,
            BTreeMap::from([(
                "request".into(),
                DataSource::Object {
                    fields: BTreeMap::from([
                        (
                            "source".into(),
                            DataSource::Constant {
                                value: json!(source),
                            },
                        ),
                        (
                            "read".into(),
                            DataSource::ActivityOutput {
                                activity: 0,
                                field: "receipt".into(),
                            },
                        ),
                        (
                            "triage".into(),
                            DataSource::Projection {
                                from: Box::new(DataSource::ActivityOutput {
                                    activity: 1,
                                    field: "receipt".into(),
                                }),
                                pointer: "/effect".into(),
                                value_type: crate::workflows::catalog::ValueType::Object,
                            },
                        ),
                    ]),
                },
            )]),
        );
        d.control
            .nodes
            .insert("triage".into(), ControlNode::Activity { activity: 1 });
        d.control
            .nodes
            .insert("prepare".into(), ControlNode::Activity { activity: 2 });
        d.control
            .edges
            .get_mut("work")
            .unwrap()
            .insert("ok".into(), "triage".into());
        d.control.edges.insert(
            "triage".into(),
            BTreeMap::from([("ok".into(), "prepare".into())]),
        );
        d.control.edges.insert(
            "prepare".into(),
            BTreeMap::from([("ok".into(), "done".into())]),
        );
    });
    bound.store.trigger_started(&intent, &run.run.id).unwrap();
    let read = super::native_issue_read::IssueReadReceipt::new(source.clone(), issue);
    let read_effect = dispatch(&f, &run);
    let run = next(&f, observe(&f, &read_effect, json!(read)));
    let triage_effect = dispatch(&f, &run);
    let triage = super::native_issue_triage::IssueTriageReceipt {
        assessment: super::native_issue_service::ScopedTriage {
            source: source.clone(),
            profile,
            decision: crate::workflows::review_verdict::TriageDecision {
                outcome,
                reason: "Controlled accepted requirements".into(),
            },
            report: "scoped-report".into(),
        },
        policy: triage_request.policy,
        effect: triage_effect.key.clone(),
    };
    let run = next(&f, observe(&f, &triage_effect, json!(triage)));
    let request = Request {
        source: source.clone(),
        read: read.clone(),
        triage: triage_effect.key,
    };
    let effect = dispatch(&f, &run);
    let key = &effect.key;
    let stories = StoryStore::open_at(&f.dir.path().join("prepared-stories.sqlite3")).unwrap();
    if outcome != TriageOutcome::Accepted {
        assert!(matches!(
            prepare(&bound, key, &request, &stories),
            Err(super::native_git::NativeFailure::Refused(message)) if message.contains("only accepted")
        ));
        assert_eq!(
            stories
                .list_plans(f.context.workspace_path().to_str().unwrap())
                .unwrap(),
            [] as [crate::stories::Plan; 0]
        );
        return;
    }
    let mut forged = request.clone();
    forged.read.issue["body"] = json!("forged material");
    assert!(prepare(&bound, key, &forged, &stories).is_err());
    assert_eq!(
        stories
            .list_plans(f.context.workspace_path().to_str().unwrap())
            .unwrap(),
        [] as [crate::stories::Plan; 0]
    );
    let receipt = prepare(&bound, key, &request, &stories).unwrap();
    let problem = receipt.create.problem.as_deref().unwrap();
    assert!(problem.contains(read.issue["title"].as_str().unwrap()));
    let body = read.issue["body"].as_str().unwrap();
    assert!(problem.contains(&body.chars().take(100).collect::<String>()));
    assert!(problem.chars().count() <= 20_000);
    if body.chars().count() < 1_000 {
        assert_eq!(
            problem,
            format!(
                "{}\n\n{}\n\nhttps://github.com/octo-org/example/issues/1347",
                read.issue["title"].as_str().unwrap(),
                body
            )
        );
    }
    assert!(problem.ends_with("https://github.com/octo-org/example/issues/1347"));
    assert_eq!(
        receipt.create.story.origin,
        crate::stories::StoryOrigin::GitHubIssue { number: 1347 }
    );
    assert_eq!(
        reconcile(&bound, &effect, &stories).unwrap().plan.id,
        receipt.plan.id
    );
    assert_eq!(
        prepare(&bound, key, &request, &stories).unwrap().plan.id,
        receipt.plan.id
    );
    f.owner
        .neutral_effect_command(
            &run.run.id,
            EffectCommand::Observe {
                key: key.clone(),
                observation: EffectObservation {
                    stopped: true,
                    outcome: EffectOutcome::Uncertain {
                        diagnostic: "controlled reply lost".into(),
                    },
                },
            },
        )
        .unwrap();
    let run = f.owner.store.neutral_execution(&run.run.id).unwrap();
    assert!(
        reconcile(&bound, run.effect(key).unwrap(), &stories)
            .unwrap_err()
            .contains("remains uncertain")
    );
    assert_eq!(
        stories
            .list_plans(f.context.workspace_path().to_str().unwrap())
            .unwrap()
            .len(),
        1
    );
}
