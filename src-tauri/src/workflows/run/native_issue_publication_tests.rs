//! Real ledger routing and resource ownership, without a provider write.
use super::native_issue_prepare_tests::{dispatch, next};
use super::native_issue_publication::*;
use super::native_test_support::Fixture;
use super::neutral_effects::{EffectCommand, EffectObservation, EffectOutcome};
use super::neutral_execution::{NeutralExecution, NeutralResult, NeutralStatus};
use crate::workflows::{
    catalog::{ActivityInstance, DescriptorId, ValueType},
    data_binding::DataSource,
    neutral_control::ControlNode,
};
use serde_json::json;
use std::collections::BTreeMap;

fn setup(mode: &str) -> (Fixture, NeutralExecution) {
    let f = Fixture::new();
    let (source, intent, _) = super::native_issue_tests::admission(&f);
    let path = f.context.workspace_path();
    let request = Request {
        source: source.clone(),
        publication: super::native_github::GitHubRequest {
            account: source.account.clone(),
            repository: source.repository.clone(),
            commit: super::check::git_output(path, &["rev-parse", "HEAD"]).unwrap(),
            tree: super::check::git_output(path, &["rev-parse", "HEAD^{tree}"]).unwrap(),
            source_intent: None,
            action: super::native_github::GitHubAction::OpenPr {
                branch: "delivery".into(),
                base: "main".into(),
                title: "Issue delivery".into(),
                body: "Source-bound change".into(),
            },
        },
    };
    let receipt = |pointer: &str| DataSource::Projection {
        from: Box::new(DataSource::ActivityOutput {
            activity: 0,
            field: "receipt".into(),
        }),
        pointer: pointer.into(),
        value_type: ValueType::Object,
    };
    let run = f.start_custom("issue.publication", json!(request), |d| {
        d.control.edges.insert(
            "work".into(),
            BTreeMap::from([
                ("published".into(), "done".into()),
                ("refused".into(), "status".into()),
                ("uncertain".into(), "status".into()),
            ]),
        );
        let operation = if mode == "publication" {
            "issue.publication"
        } else {
            "issue.effect"
        };
        let count = if mode == "second_status" { 2 } else { 1 };
        for activity in 1..=count {
            d.instances.push(ActivityInstance {
                descriptor: DescriptorId {
                    namespace: "native".into(),
                    name: operation.into(),
                    version: 1,
                },
                config: BTreeMap::new(),
                execution_policy: "native".into(),
            });
            let action = if mode == "comment" {
                DataSource::Constant {
                    value: json!({"type":"comment","body":"Must not write"}),
                }
            } else {
                let reference = if mode == "reference" {
                    DataSource::Object {
                        fields: BTreeMap::from([
                            ("fence".into(), receipt("/effect/fence")),
                            (
                                "digest".into(),
                                DataSource::Constant {
                                    value: json!("foreign"),
                                },
                            ),
                        ]),
                    }
                } else {
                    receipt("/effect")
                };
                DataSource::Object {
                    fields: BTreeMap::from([
                        (
                            "type".into(),
                            DataSource::Constant {
                                value: json!("publication_status"),
                            },
                        ),
                        ("publication".into(), reference),
                    ]),
                }
            };
            let mut wrong = source.clone();
            wrong.number += 1;
            let input = if mode == "publication" {
                DataSource::Constant {
                    value: json!(request),
                }
            } else {
                DataSource::Object {
                    fields: BTreeMap::from([
                        (
                            "source".into(),
                            if mode == "source" {
                                DataSource::Constant {
                                    value: json!(wrong),
                                }
                            } else {
                                receipt("/source")
                            },
                        ),
                        ("action".into(), action),
                    ]),
                }
            };
            d.input_bindings
                .insert(activity, BTreeMap::from([("request".into(), input)]));
            let node = if activity == 1 { "status" } else { "second" };
            let target = if count == 2 && activity == 1 {
                "second"
            } else {
                "done"
            };
            d.control
                .nodes
                .insert(node.into(), ControlNode::Activity { activity });
            d.control.edges.insert(
                node.into(),
                if mode == "publication" {
                    BTreeMap::from([
                        ("published".into(), target.into()),
                        ("refused".into(), target.into()),
                        ("uncertain".into(), target.into()),
                    ])
                } else {
                    BTreeMap::from([("ok".into(), target.into())])
                },
            );
        }
    });
    f.owner.store.trigger_started(&intent, &run.run.id).unwrap();
    (f, run)
}
fn observe(
    f: &Fixture,
    effect: &super::neutral_effects::NeutralEffect,
    stopped: bool,
) -> NeutralExecution {
    f.owner
        .neutral_effect_command(
            &effect.key.fence.run,
            EffectCommand::Observe {
                key: effect.key.clone(),
                observation: EffectObservation {
                    stopped,
                    outcome: EffectOutcome::Uncertain {
                        diagnostic: "controlled provider reply lost".into(),
                    },
                },
            },
        )
        .unwrap()
}

// Catches: unknown publication relabelled completed, resources released, or a
// live writer enabling status. This drives the exact effect-command boundary.
#[tokio::test]
async fn issue_publication_unknown_keeps_original_result_and_claims_while_exact_status_can_dispatch()
 {
    let (f, run) = setup("status");
    let original = dispatch(&f, &run);
    let run = observe(&f, &original, false);
    assert_eq!(
        run.pending_invocations(),
        [] as [(
            usize,
            &crate::workflows::run::neutral_execution::NeutralAttempt
        ); 0]
    );
    assert!(run.attempt.as_ref().unwrap().result.is_none());
    let run = observe(&f, &original, true);
    let run = next(&f, run);
    let status = dispatch(&f, &run);
    assert_eq!(
        status.invocation.descriptor.handler.name,
        "native.issue.effect"
    );
    let run = f.owner.store.neutral_execution(&run.run.id).unwrap();
    assert!(matches!(
        &run.effect(&original.key).unwrap().observation,
        Some(EffectObservation {
            stopped: true,
            outcome: EffectOutcome::Uncertain { .. }
        })
    ));
    assert!(!run.effect(&original.key).unwrap().settled());
    let conn = rusqlite::Connection::open(f.owner.store.database_path()).unwrap();
    let owner: String = conn
        .query_row(
            "SELECT effect_id FROM neutral_effect_resources WHERE resource=?1",
            [format!(
                "native:workspace:{}",
                f.context.git_workspace().unwrap().git_dir
            )],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(owner, original.key.digest);
    assert!(
        f.owner
            .neutral_effect_command(
                &run.run.id,
                EffectCommand::Observe {
                    key: original.key.clone(),
                    observation: EffectObservation {
                        stopped: true,
                        outcome: EffectOutcome::Completed {
                            result: NeutralResult::Completed {
                                outcome: "published".into(),
                                outputs: BTreeMap::from([("receipt".into(), json!({}))])
                            }
                        }
                    }
                }
            )
            .unwrap_err()
            .contains("uncertainty is immutable")
    );
    let request: super::native_issue_effect::IssueEffectRequest =
        serde_json::from_value(status.invocation.inputs["request"].clone()).unwrap();
    let bound = super::native_host::NativeBinding {
        context: f.context.clone(),
        store: f.owner.store.clone(),
    };
    let resolved = super::native_issue_policy::resolve(&bound, &status.key, &request).unwrap();
    let super::native_issue_policy::IssueWrite::Comment(body) = resolved else {
        panic!("expected source status");
    };
    assert!(body.contains(&original.key.digest));
    assert!(body.contains("uncertain"));
}

// Catches: a caller using the failure port for arbitrary comments, altered
// provenance, replacement publication, or a repeated status attempt.
#[tokio::test]
async fn issue_publication_disposition_refuses_other_mutations_and_second_status() {
    for mode in [
        "comment",
        "reference",
        "source",
        "publication",
        "second_status",
    ] {
        let (f, run) = setup(mode);
        let original = dispatch(&f, &run);
        let run = next(&f, observe(&f, &original, true));
        if mode == "second_status" {
            let first = dispatch(&f, &run);
            let run = f
                .owner
                .neutral_effect_command(
                    &run.run.id,
                    EffectCommand::Observe {
                        key: first.key,
                        observation: EffectObservation {
                            stopped: true,
                            outcome: EffectOutcome::Completed {
                                result: NeutralResult::Completed {
                                    outcome: "ok".into(),
                                    outputs: BTreeMap::from([("receipt".into(), json!({}))]),
                                },
                            },
                        },
                    },
                )
                .unwrap();
            let run = next(&f, run);
            assert_dispatch_refused(&f, &run, mode);
        } else {
            assert_dispatch_refused(&f, &run, mode);
        }
    }
}
fn assert_dispatch_refused(f: &Fixture, run: &NeutralExecution, mode: &str) {
    use super::neutral_runtime::{NeutralHost, NeutralInvocation};
    let (activity, attempt) = run.pending_invocations().into_iter().next().unwrap();
    let invocation = NeutralInvocation {
        fence: attempt.fence.clone(),
        publication: run.publication.clone(),
        descriptor: run.descriptor(activity).unwrap().clone(),
        instance: run.publication.definition.instances[activity].clone(),
        inputs: attempt.inputs.clone(),
        subject: attempt.subject.clone(),
    };
    let effect = super::neutral_effects::NeutralEffect::prepare(
        invocation.clone(),
        f.host.prepare(&invocation).unwrap(),
    )
    .unwrap();
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
            EffectCommand::Answer {
                key: effect.key.clone(),
                revision: effect.question.unwrap().revision,
                approved: true,
            },
        )
        .unwrap();
    let error = f
        .owner
        .neutral_effect_command(&run.run.id, EffectCommand::Dispatch { key: effect.key })
        .unwrap_err();
    assert!(
        error.contains("issue") && (error.contains("status") || error.contains("source")),
        "{mode}: {error}"
    );
    assert_eq!(
        f.owner.store.neutral_execution(&run.run.id).unwrap().status,
        NeutralStatus::Running
    );
}

// Catches: native approval refusal still killing the issue graph instead of
// reaching only the exact status, without pretending a provider write ran.
#[tokio::test]
async fn issue_publication_approval_decline_routes_refusal_without_dispatch() {
    use super::neutral_runtime::{NeutralHost, NeutralInvocation};
    let (f, run) = setup("status");
    let (activity, attempt) = run.pending_invocations().into_iter().next().unwrap();
    let invocation = NeutralInvocation {
        fence: attempt.fence.clone(),
        publication: run.publication.clone(),
        descriptor: run.descriptor(activity).unwrap().clone(),
        instance: run.publication.definition.instances[activity].clone(),
        inputs: attempt.inputs.clone(),
        subject: attempt.subject.clone(),
    };
    let effect = super::neutral_effects::NeutralEffect::prepare(
        invocation.clone(),
        f.host.prepare(&invocation).unwrap(),
    )
    .unwrap();
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
            EffectCommand::Answer {
                key: effect.key.clone(),
                revision: effect.question.unwrap().revision,
                approved: false,
            },
        )
        .unwrap();
    assert!(!run.effect(&effect.key).unwrap().dispatched);
    assert_eq!(
        run.attempt.as_ref().unwrap().result.as_ref().unwrap(),
        &NeutralResult::Completed {
            outcome: "refused".into(),
            outputs: BTreeMap::from([(
                "receipt".into(),
                json!({"source":run.effect(&effect.key).unwrap().invocation.inputs["request"]["source"], "effect":effect.key,"disposition":"refused","diagnostic":"neutral effect approval declined"})
            )])
        }
    );
    dispatch(&f, &next(&f, run));
}
