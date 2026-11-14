//! Strict typed requests; the native provider never accepts argv or shell text.
use super::native_git::{CommitRequest, NativeFailure, PushRequest};
use super::native_host::NativeBinding;
use super::native_story::StoryRequest;
use super::neutral_effects::{EffectObservation, EffectOutcome, NeutralEffect};
use super::neutral_execution::NeutralResult;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactRequest {
    paths: Vec<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyRequest {}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct VerificationRequest {
    check: String,
    commit: String,
    tree: String,
}
fn request<T: serde::de::DeserializeOwned>(inputs: &BTreeMap<String, Value>) -> Result<T, String> {
    if inputs.len() != 1 {
        return Err("native operation requires one typed request".into());
    }
    serde_json::from_value(
        inputs
            .get("request")
            .ok_or("native request absent")?
            .clone(),
    )
    .map_err(|e| format!("native request: {e}"))
}
pub(super) fn validate_request(
    handler: &str,
    inputs: &BTreeMap<String, Value>,
) -> Result<(), String> {
    if handler == super::correction_policy::HANDLER {
        return super::correction_policy::validate(inputs);
    }
    if handler == "native.correction.assess" {
        return super::correction_native::validate(inputs);
    }
    match handler {
        super::native_stage::HANDLER => {
            request::<super::native_stage::StageRequest>(inputs)?.validate()?
        }
        super::native_batch::PENDING => {
            let r = request::<super::native_batch::PendingRequest>(inputs)?;
            r.cohort.validate()?;
            if !(1..=128).contains(&r.limit) {
                return Err("batch pending limit must be between 1 and 128".into());
            }
        }
        super::native_batch::ADMIT => request::<super::native_batch::AdmitRequest>(inputs)?
            .cohort
            .validate()?,
        super::native_batch::LAND => {
            request::<super::native_batch::LandRequest>(inputs)?;
        }
        super::native_batch::PIN => {
            request::<super::native_batch::PinRequest>(inputs)?;
        }
        super::native_batch::VERDICT => {
            request::<super::native_batch::VerdictRequest>(inputs)?;
        }
        "native.issue.triage" => {
            request::<super::native_issue_triage::IssueTriageRequest>(inputs)?.validate()?
        }
        "native.issue.handoff" => super::native_issue_handoff::validate(&request::<
            super::native_issue_service::IssueChildRequest,
        >(inputs)?)?,
        super::native_issue_graph::HANDLER => {
            let r = request::<super::native_issue_graph::Request>(inputs)?;
            r.source.validate()?;
            r.cohort.validate()?;
        }
        super::native_issue_publication::HANDLER => {
            request::<super::native_issue_publication::Request>(inputs)?.validate()?;
        }
        super::native_issue_prepare::HANDLER => {
            request::<super::native_issue_prepare::Request>(inputs)?
                .source
                .validate()?;
        }
        "native.issue.read" => {
            request::<super::native_issue_read::IssueReadRequest>(inputs)?;
        }
        "native.issue.effect" => {
            request::<super::native_issue_effect::IssueEffectRequest>(inputs)?.validate()?
        }
        super::native_operator::HANDLER | super::native_operator::AUTO_HANDLER => {
            super::native_operator::request(inputs)?;
        }
        super::native_role_context::HANDLER => {
            request::<super::native_role_context::Request>(inputs)?;
        }
        "native.agent.report.read" => {
            request::<super::native_agent::ReportRequest>(inputs)?;
        }
        "native.workspace.pin" => {
            request::<EmptyRequest>(inputs)?;
        }
        "native.artifact.pin" => {
            request::<ArtifactRequest>(inputs)?;
        }
        "native.verification.run" => {
            request::<VerificationRequest>(inputs)?;
        }
        "native.review.change" | "native.review.bounded" => {
            request::<super::native_review::ReviewRequest>(inputs)?;
        }
        "native.commit.create" => {
            request::<CommitRequest>(inputs)?;
        }
        "native.publication.github" => {
            let r = request::<super::native_github::GitHubRequest>(inputs)?;
            super::native_github::validate(&r)?;
        }
        "native.publication.push" => {
            request::<PushRequest>(inputs)?;
        }
        "native.story.cohort.observe" => {
            request::<super::native_cohort::Cohort>(inputs)?.validate()?;
        }
        "native.story.cohort.created" => {
            request::<super::native_cohort::CreatedRequest>(inputs)?;
        }
        "native.story.read" => {
            request::<super::native_story::ReadRequest>(inputs)?;
        }
        "native.story.create" => {
            let r = request::<super::native_story_create::CreateRequest>(inputs)?;
            super::native_story_create::validate(&r)?;
        }
        "native.story.transition" => {
            request::<StoryRequest>(inputs)?;
        }
        _ => return Err(format!("native handler unavailable: {handler}")),
    }
    Ok(())
}
fn value(receipt: &impl Serialize) -> Result<Value, NativeFailure> {
    serde_json::to_value(receipt).map_err(|e| NativeFailure::Uncertain(e.to_string()))
}
fn execute(bound: &NativeBinding, effect: &NeutralEffect) -> Result<Value, NativeFailure> {
    let context = &bound.context;
    context.current(None)?;
    context.git_workspace()?;
    let inputs = &effect.invocation.inputs;
    match effect.invocation.descriptor.handler.name.as_str() {
        super::native_stage::HANDLER => value(&super::native_stage::stage(
            context,
            &request::<super::native_stage::StageRequest>(inputs)?,
        )?),
        handler if super::native_batch::HANDLERS.contains(&handler) => {
            Ok(super::native_batch_evidence::execute(bound, effect)?)
        }
        super::native_issue_publication::HANDLER => {
            tokio::runtime::Handle::current().block_on(super::native_issue_publication::publish(
                bound,
                &effect.key,
                &request::<super::native_issue_publication::Request>(inputs)?,
            ))
        }
        super::native_issue_graph::HANDLER => value(&super::native_issue_graph::observe(
            bound,
            &effect.key,
            &request::<super::native_issue_graph::Request>(inputs)?,
            crate::stories::story_backend_for(&context.workspace_path().to_string_lossy())?
                .as_ref(),
        )?),
        super::native_issue_prepare::HANDLER => value(&super::native_issue_prepare::prepare(
            bound,
            &effect.key,
            &request::<super::native_issue_prepare::Request>(inputs)?,
            crate::stories::story_backend_for(&context.workspace_path().to_string_lossy())?
                .as_ref(),
        )?),
        "native.issue.read" => value(
            &tokio::runtime::Handle::current()
                .block_on(super::native_issue_read::read(bound, &effect.key))?,
        ),
        "native.issue.effect" => value(&tokio::runtime::Handle::current().block_on(
            super::native_issue_effect::publish(
                bound,
                &effect.key,
                &request::<super::native_issue_effect::IssueEffectRequest>(inputs)?,
            ),
        )?),
        super::native_operator::HANDLER | super::native_operator::AUTO_HANDLER => {
            super::native_operator::execute(bound, effect)
        }
        super::native_role_context::HANDLER => value(&super::native_role_context::read(
            bound,
            &effect.key,
            crate::stories::story_backend_for(&context.workspace_path().to_string_lossy())?
                .as_ref(),
            &request::<super::native_role_context::Request>(inputs)?,
        )?),
        "native.agent.report.read" => {
            let request = request::<super::native_agent::ReportRequest>(inputs)?;
            if request.report.fence.run != effect.key.fence.run {
                return Err(NativeFailure::Refused(
                    "role report belongs to a foreign run".into(),
                ));
            }
            value(&super::native_agent::read_report(
                bound,
                &request.report,
                &request.source,
            )?)
        }
        "native.workspace.pin" => {
            request::<EmptyRequest>(inputs)?;
            value(context.git_workspace()?)
        }
        "native.artifact.pin" => {
            value(&context.pin_artifact(&request::<ArtifactRequest>(inputs)?.paths)?)
        }
        "native.verification.run" => {
            let r = request::<VerificationRequest>(inputs)?;
            value(&context.verify(
                &r.check,
                &r.commit,
                &r.tree,
                bound.store.database_path(),
                &effect.key.fence.run,
            )?)
        }
        "native.review.change" | "native.review.bounded" => {
            let request = request::<super::native_review::ReviewRequest>(inputs)?;
            let state = context.current(None)?;
            let path = context.workspace_path().to_string_lossy().into_owned();
            value(&tokio::runtime::Handle::current().block_on(
                super::native_review::review_with(
                    context,
                    &bound.store,
                    &effect.key,
                    &request,
                    |diff| async move {
                        crate::workflows::review_verdict::acp_code_review(
                            &state, &path, &diff, None,
                        )
                        .await
                    },
                ),
            )?)
        }
        "native.commit.create" => value(&super::native_git::commit(
            context,
            &bound.store,
            &effect.key,
            &request::<CommitRequest>(inputs)?,
        )?),
        "native.publication.github" => {
            let r = request::<super::native_github::GitHubRequest>(inputs)?;
            value(
                &tokio::runtime::Handle::current().block_on(super::native_github::publish(
                    bound,
                    &effect.key,
                    &r,
                ))?,
            )
        }
        "native.publication.push" => value(&super::native_git::push(
            context,
            &bound.store,
            &effect.key,
            &request::<PushRequest>(inputs)?,
        )?),
        "native.story.cohort.observe" => value(&super::native_cohort::observe(
            context,
            &effect.key,
            crate::stories::story_backend_for(&context.workspace_path().to_string_lossy())?
                .as_ref(),
            &request::<super::native_cohort::Cohort>(inputs)?,
        )?),
        "native.story.read" => value(&super::native_story::read(
            context,
            crate::stories::story_backend_for(&context.workspace_path().to_string_lossy())?
                .as_ref(),
            &request::<super::native_story::ReadRequest>(inputs)?,
        )?),
        "native.story.cohort.created" => value(&super::native_cohort::created_receipt(
            &super::native_cohort::created_cohort(
                context,
                &bound.store,
                &effect.key,
                crate::stories::story_backend_for(&context.workspace_path().to_string_lossy())?
                    .as_ref(),
                &request::<super::native_cohort::CreatedRequest>(inputs)?,
            )?,
        )),
        "native.story.create" => value(&super::native_story_create::create(
            context,
            &bound.store,
            &effect.key,
            crate::stories::story_backend_for(&context.workspace_path().to_string_lossy())?
                .as_ref(),
            &request::<super::native_story_create::CreateRequest>(inputs)?,
        )?),
        "native.story.transition" => value(&super::native_story::transition(
            context,
            &bound.store,
            &effect.key,
            crate::stories::story_backend()?.as_ref(),
            &request::<StoryRequest>(inputs)?,
        )?),
        handler => Err(NativeFailure::Refused(format!(
            "native handler unavailable: {handler}"
        ))),
    }
}
pub(super) fn invoke(bound: &NativeBinding, effect: &NeutralEffect) -> EffectObservation {
    if matches!(
        effect.invocation.descriptor.handler.name.as_str(),
        "native.correction.assess" | "native.correction.policy.assess"
    ) {
        return super::correction_native_output::invoke(bound, effect);
    }
    let outcome = match execute(bound, effect) {
        Ok(receipt) => EffectOutcome::Completed {
            result: NeutralResult::Completed {
                outcome: if effect.invocation.descriptor.handler.name
                    == super::native_issue_publication::HANDLER
                {
                    "published".into()
                } else if effect.invocation.descriptor.handler.name == "native.story.cohort.observe"
                    || effect.invocation.descriptor.handler.name == super::native_batch::VERDICT
                    || effect
                        .invocation
                        .descriptor
                        .handler
                        .name
                        .starts_with("native.operator.")
                {
                    receipt["outcome"]
                        .as_str()
                        .expect("native cohort observation outcome")
                        .into()
                } else {
                    "ok".into()
                },
                outputs: BTreeMap::from([("receipt".into(), receipt)]),
            },
        },
        Err(NativeFailure::Refused(diagnostic))
            if effect.invocation.descriptor.handler.name
                == super::native_review_policy::HANDLER
                && diagnostic == "independent reviewer admission budget exhausted" =>
        {
            EffectOutcome::Completed {
                result: NeutralResult::Completed {
                    outcome: "exhausted".into(),
                    outputs: BTreeMap::from([(
                        "receipt".into(),
                        serde_json::json!({"reason":diagnostic}),
                    )]),
                },
            }
        }
        Err(NativeFailure::Refused(diagnostic)) => EffectOutcome::Completed {
            result: NeutralResult::Failed { diagnostic },
        },
        Err(NativeFailure::Uncertain(diagnostic)) => EffectOutcome::Uncertain { diagnostic },
    };
    EffectObservation {
        outcome,
        stopped: true,
    }
}
