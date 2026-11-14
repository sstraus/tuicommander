//! Legacy issue effect meanings resolve only from retained native evidence.
use super::native_host::NativeBinding;
use super::native_issue_effect::{IssueAction, IssueEffectRequest};
use super::native_issue_service::{IssueChildReceipt, IssueChildStatus};
use super::native_issue_triage::require_triage;
use super::neutral_effects::{EffectKey, EffectOutcome};
use super::neutral_execution::NeutralResult;
use crate::workflows::review_verdict::TriageOutcome;

#[derive(Debug)]
pub(super) enum IssueWrite {
    Comment(String),
    AddLabel(String),
}
fn child(
    bound: &NativeBinding,
    key: &EffectKey,
    reference: &EffectKey,
    request: &IssueEffectRequest,
) -> Result<IssueChildReceipt, String> {
    if key.fence.run != reference.fence.run {
        return Err("child belongs to another issue intake".into());
    }
    let run = bound.store.neutral_execution(&key.fence.run)?;
    let effect = run.effect(reference)?;
    if !matches!(
        effect.invocation.descriptor.handler.name.as_str(),
        "native.issue.handoff" | super::native_issue_graph::HANDLER
    ) || !effect.settled()
    {
        return Err("original issue child has no stopped receipt".into());
    }
    let outputs = match effect
        .observation
        .as_ref()
        .map(|observation| &observation.outcome)
    {
        Some(EffectOutcome::Completed {
            result: NeutralResult::Completed { outputs, .. },
        }) => outputs,
        _ => return Err("original issue child result is uncertain".into()),
    };
    let value = outputs.get("receipt").ok_or("child receipt absent")?;
    let receipt: IssueChildReceipt =
        if effect.invocation.descriptor.handler.name == super::native_issue_graph::HANDLER {
            let retained: super::native_issue_graph::Receipt =
                serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
            if retained.effect != *reference {
                return Err("issue graph receipt effect differs from original observer".into());
            }
            retained.child
        } else {
            serde_json::from_value(value.clone()).map_err(|e| e.to_string())?
        };
    if effect.invocation.descriptor.handler.name != super::native_issue_graph::HANDLER {
        let original = serde_json::from_value(effect.invocation.inputs["request"].clone())
            .map_err(|e| e.to_string())?;
        super::native_issue_handoff::require_child(&original, &receipt)?;
    }
    if receipt.source != request.source {
        return Err("child issue source mismatch".into());
    }
    Ok(receipt)
}
pub(super) fn resolve(
    bound: &NativeBinding,
    key: &EffectKey,
    request: &IssueEffectRequest,
) -> Result<IssueWrite, String> {
    Ok(match &request.action {
        IssueAction::Comment { body } => IssueWrite::Comment(body.clone()),
        IssueAction::AddLabel { label } => IssueWrite::AddLabel(label.clone()),
        IssueAction::RejectionComment { triage }
        | IssueAction::QuestionComment { triage }
        | IssueAction::AcceptedLabel { triage }
        | IssueAction::RejectionLabel { triage } => {
            let receipt = require_triage(bound, key, triage, &request.source)?;
            match (&request.action, receipt.assessment.decision.outcome) {
                (IssueAction::RejectionComment { .. }, TriageOutcome::Rejected) => {
                    IssueWrite::Comment(receipt.assessment.decision.reason)
                }
                (IssueAction::QuestionComment { .. }, TriageOutcome::NeedsInput)
                    if receipt.policy.needs_input_comment =>
                {
                    IssueWrite::Comment(receipt.assessment.decision.reason)
                }
                (IssueAction::AcceptedLabel { .. }, TriageOutcome::Accepted) => {
                    IssueWrite::AddLabel(receipt.policy.accepted_label)
                }
                (IssueAction::RejectionLabel { .. }, TriageOutcome::Rejected) => {
                    IssueWrite::AddLabel(receipt.policy.rejection_label)
                }
                _ => return Err("issue disposition/policy does not permit this effect".into()),
            }
        }
        IssueAction::EvidenceComment { child: reference } => {
            let receipt = child(bound, key, reference, request)?;
            if receipt.status != IssueChildStatus::Completed {
                return Err("issue child did not complete successfully".into());
            }
            let evidence = receipt
                .evidence
                .filter(|body| !body.trim().is_empty())
                .ok_or("issue child has no delivery evidence")?;
            IssueWrite::Comment(format!("Child {} completed.\n\n{evidence}", receipt.child))
        }
        IssueAction::PublicationStatus { publication } => {
            let snapshot = bound.store.neutral_execution(&key.fence.run)?;
            IssueWrite::Comment(super::native_issue_publication::status(
                &snapshot,
                key,
                publication,
                &request.source,
            )?)
        }
        IssueAction::StatusComment { child: reference } => {
            let receipt = child(bound, key, reference, request)?;
            IssueWrite::Comment(format!(
                "TUICommander issue child {} is {:?}. Check its retained evidence before acting.",
                receipt.child, receipt.status
            ))
        }
    })
}
