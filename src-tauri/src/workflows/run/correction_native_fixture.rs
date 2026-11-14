//! Real native ledger/check pipeline; substitute only the external ACP answer.
use super::native_test_support::Fixture;
use super::neutral_effects::{EffectCommand, EffectObservation, EffectOutcome, NeutralEffect};
use super::neutral_execution::NeutralExecution;
use super::neutral_execution::NeutralResult;
use super::neutral_execution_tests::step;
use super::neutral_runtime::{NeutralHost, NeutralInvocation, admitted};
use crate::workflows::catalog::{ActivityInstance, content_digest};
use crate::workflows::correction::{CorrectionArtifact, CorrectionPolicy, CorrectionState};
use crate::workflows::data_binding::DataSource;
use crate::workflows::neutral_control::ControlNode;
use crate::workflows::review_verdict::CodeVerdict;
use crate::workflows::{WorkflowStore, neutral_host::NeutralCatalogHost};
use serde_json::json;
use std::collections::BTreeMap;

pub(super) fn start(f: &Fixture) -> NeutralExecution {
    start_custom(f, |_| {})
}
pub(super) fn start_custom(
    f: &Fixture,
    customize: impl FnOnce(&mut crate::workflows::neutral_publication::NeutralDefinition),
) -> NeutralExecution {
    let path = f.context.workspace_path();
    let base = super::check::git_output(path, &["rev-parse", "HEAD"]).unwrap();
    std::fs::write(path.join("artifact.txt"), "candidate").unwrap();
    super::check::git_output(path, &["add", "artifact.txt"]).unwrap();
    super::check::git_output(path, &["commit", "-qm", "candidate"]).unwrap();
    let pin = f.context.pin_artifact(&["artifact.txt".into()]).unwrap();
    let template = f.start("workspace.pin", json!({}));
    let mut definition = template.publication.definition.clone();
    definition.inputs.fields.clear();
    let catalog = f.host.catalog(&template.publication.project).unwrap();
    let review = catalog
        .descriptors()
        .find(|d| d.handler.name == "native.review.change")
        .unwrap();
    let assess = catalog
        .descriptors()
        .find(|d| d.handler.name == "native.correction.assess")
        .unwrap();
    definition.instances = [review, assess]
        .into_iter()
        .map(|d| ActivityInstance {
            descriptor: d.id.clone(),
            config: BTreeMap::new(),
            execution_policy: "native".into(),
        })
        .collect();
    let policy = CorrectionPolicy {
        criteria: f
            .context
            .verification_check_names()
            .iter()
            .map(|n| format!("check:{n}"))
            .chain(["independent_review".into()])
            .collect(),
    };
    let state = CorrectionState::initial(CorrectionArtifact {
        generation: 1,
        head: pin.commit.clone(),
        base: base.clone(),
        material_digest: pin.tree.clone(),
        criteria_digest: content_digest(&policy).unwrap(),
    });
    definition.input_bindings = BTreeMap::from([
        (
            0,
            BTreeMap::from([(
                "request".into(),
                DataSource::Constant {
                    value: json!({"base":base,"head":pin.commit,"tree":pin.tree}),
                },
            )]),
        ),
        (
            1,
            BTreeMap::from([
                ("context".into(), DataSource::Constant { value: json!({}) }),
                (
                    "criteria".into(),
                    DataSource::Constant {
                        value: json!(policy),
                    },
                ),
                (
                    "state".into(),
                    DataSource::Constant {
                        value: json!(state),
                    },
                ),
            ]),
        ),
    ]);
    definition
        .control
        .nodes
        .insert("assess".into(), ControlNode::Activity { activity: 1 });
    definition
        .control
        .edges
        .insert("work".into(), [("ok".into(), "assess".into())].into());
    definition
        .control
        .edges
        .insert("assess".into(), [("assessed".into(), "done".into())].into());
    customize(&mut definition);
    let workflows = WorkflowStore::open_at(&f.dir.path().join("correction.sqlite3")).unwrap();
    let handlers = f.host.registry().available_handlers();
    let publication = workflows
        .save_neutral_publication(
            &template.publication.project,
            "correction-native",
            0,
            definition,
            &catalog,
            &handlers,
        )
        .unwrap();
    let mut request = template.run.request;
    request.publication.id = publication.id;
    request.request_id = "native-correction".into();
    request.inputs.clear();
    let run = f
        .owner
        .start_neutral(&workflows, &request, &catalog, &handlers)
        .unwrap();
    f.host
        .bind_run(&f.owner.store, &run.id, f.context.clone())
        .unwrap();
    let run = f.owner.store.neutral_execution(&run.id).unwrap();
    let at = super::store::now_ms();
    let run = step(&f.owner, &run, "start", at);
    step(&f.owner, &run, "review-ready", at)
}

pub(super) async fn reviewed(
    f: &Fixture,
    run: NeutralExecution,
    verdict: CodeVerdict,
) -> NeutralExecution {
    let (activity, attempt) = run.pending_invocations()[0];
    let mut invocation = NeutralInvocation {
        fence: attempt.fence.clone(),
        publication: run.publication.clone(),
        descriptor: run.descriptor(activity).unwrap().clone(),
        instance: run.publication.definition.instances[activity].clone(),
        inputs: attempt.inputs.clone(),
        subject: attempt.subject.clone(),
    };
    admitted(f.host.as_ref(), &run, &mut invocation, None).unwrap();
    let effect =
        NeutralEffect::prepare(invocation.clone(), f.host.prepare(&invocation).unwrap()).unwrap();
    let key = effect.key.clone();
    f.owner
        .neutral_effect_command(
            &run.run.id,
            EffectCommand::Prepare {
                effect: Box::new(effect),
            },
        )
        .unwrap();
    f.owner
        .neutral_effect_command(&run.run.id, EffectCommand::Dispatch { key: key.clone() })
        .unwrap();
    let request = serde_json::from_value(invocation.inputs["request"].clone()).unwrap();
    let receipt = super::native_review::review_with(
        &f.context,
        &f.owner.store,
        &key,
        &request,
        |diff| async move {
            assert!(diff.contains("candidate"));
            Ok((
                verdict,
                "Controlled independent answer".into(),
                Some("isolated-review-session".into()),
            ))
        },
    )
    .await
    .unwrap();
    let run = f
        .owner
        .neutral_effect_command(
            &run.run.id,
            EffectCommand::Observe {
                key,
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
    let at = super::store::now_ms();
    let run = step(&f.owner, &run, "to-assessment", at);
    step(&f.owner, &run, "assessment-ready", at)
}
