//! Production IssueServices owner. Triage runs one turn on the shared mediated
//! model path; child dispatch belongs to graph composition, never to this owner.
use super::native_host::NativeBinding;
use super::native_issue_read::IssueReadReceipt;
use super::native_issue_service::{
    IssueChildReceipt, IssueChildRequest, IssueFuture, IssueProfile, IssueServices, ScopedTriage,
};
use super::neutral_effects::{EffectCommand, EffectKey};
use super::neutral_model_request::{ModelLimits, ModelRequest, UnavailablePolicy};
use crate::workflows::catalog::{FieldSchema, ObjectSchema, ValueType};
use crate::workflows::review_verdict::TriageDecision;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Coordinator decision T1+H2 (2026-11-12): no effect-owned child dispatch.
const CHILD_BLOCKER: &str = "issue child dispatch requires conversion: compose native.story.create + host.story.subflow; the issue service owner never starts a child";

pub(super) struct MediatedIssueServices;

fn triage_request(
    issue: &IssueReadReceipt,
    profile: &IssueProfile,
) -> Result<ModelRequest, String> {
    let text = || FieldSchema {
        value_type: ValueType::String,
        required: true,
        default: None,
    };
    let mut context = issue.context();
    context["outcomes"] = json!(["accepted", "rejected", "needs_input"]);
    Ok(ModelRequest {
        profile: profile.profile.clone(),
        model: profile.model.clone(),
        effort: profile.effort.clone(),
        instructions: profile.instructions.clone(),
        context: vec![serde_json::to_string(&context).map_err(|e| e.to_string())?],
        output_schema: ObjectSchema {
            fields: BTreeMap::from([
                ("outcome".into(), text()),
                ("reason".into(), text()),
                ("report".into(), text()),
            ]),
        },
        limits: ModelLimits {
            timeout_ms: 300_000,
            max_context_bytes: 1_048_576,
            max_output_bytes: 8_192,
        },
        unavailable_policy: UnavailablePolicy::Fail,
    })
}

impl IssueServices for MediatedIssueServices {
    fn triage<'a>(
        &'a self,
        bound: &'a NativeBinding,
        key: &'a EffectKey,
        issue: &'a IssueReadReceipt,
        profile: &'a IssueProfile,
    ) -> IssueFuture<'a, ScopedTriage> {
        Box::pin(async move {
            profile.validate()?;
            let state = bound.context.current(None)?;
            let runtime = crate::acp_commands::ego_config(&state).map_err(|e| e.message)?;
            let request = triage_request(issue, profile)?;
            // Refuse a foreign profile before any process starts.
            request.validate(&runtime)?;
            let owner = state.workflow_runtime.neutral_owner()?;
            let host = super::neutral_model_mcp::ModelMcpHost::new(&owner, key.clone())?;
            let output = super::neutral_model_adapter::run_request(
                &state.acp,
                &request,
                &runtime,
                bound.context.workspace_path().into(),
                host,
                |session| {
                    owner
                        .neutral_effect_command(
                            &key.fence.run,
                            EffectCommand::BindModel {
                                key: key.clone(),
                                session,
                            },
                        )
                        .map(|_| ())
                },
            )
            .await?;
            let field = |name: &str| output.get(name).cloned().unwrap_or(Value::Null);
            let decision = TriageDecision {
                outcome: serde_json::from_value(field("outcome"))
                    .map_err(|e| format!("triage outcome: {e}"))?,
                reason: serde_json::from_value(field("reason")).map_err(|e| e.to_string())?,
            };
            Ok(ScopedTriage {
                source: issue.source.clone(),
                profile: profile.clone(),
                decision,
                report: serde_json::from_value(field("report")).map_err(|e| e.to_string())?,
            })
        })
    }
    fn child_dispatch(&self) -> Result<(), String> {
        Err(CHILD_BLOCKER.into())
    }
    fn handoff<'a>(
        &'a self,
        _: &'a NativeBinding,
        _: &'a EffectKey,
        _: &'a IssueChildRequest,
    ) -> IssueFuture<'a, IssueChildReceipt> {
        Box::pin(async { Err(CHILD_BLOCKER.into()) })
    }
    fn observe<'a>(&'a self, _: &'a EffectKey) -> IssueFuture<'a, IssueChildReceipt> {
        Box::pin(async { Err(CHILD_BLOCKER.into()) })
    }
    fn cancel<'a>(&'a self, _: &'a EffectKey) -> IssueFuture<'a, IssueChildReceipt> {
        Box::pin(async { Err(CHILD_BLOCKER.into()) })
    }
}
