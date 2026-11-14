//! Source reports supply findings; only native review/check observations approve.
use super::evidence::{ReviewEvidence, ReviewSubject, TriageDecision};
use super::{
    native_host::NativeBinding,
    neutral_effects::{EffectKey, NeutralEffect},
};
use crate::workflows::{
    catalog::content_digest, correction::*, review_policy::ReviewAdmissionPolicy,
};
use serde_json::Value;
use std::collections::BTreeMap;

pub(super) const HANDLER: &str = "native.correction.policy.assess";

pub(super) fn validate(inputs: &BTreeMap<String, Value>) -> Result<(), String> {
    let mut base = inputs.clone();
    let report = base
        .remove("report")
        .ok_or("source role report reference missing")?;
    if report["admission_exhausted"] == true {
        serde_json::from_value::<EffectKey>(report["effect"].clone()).map_err(|e| e.to_string())?;
    } else {
        report_reference(&report)?;
    }
    let proposals = base.remove("proposals").unwrap_or(serde_json::json!([]));
    let keys: Vec<EffectKey> = serde_json::from_value(proposals).map_err(|e| e.to_string())?;
    if keys.len() > 100 {
        return Err("too many proposal receipts".into());
    }
    super::correction_native::validate(&base)
}

pub(super) fn findings(
    bound: &NativeBinding,
    effect: &NeutralEffect,
    state: &CorrectionState,
    policy: &ReviewAdmissionPolicy,
) -> Result<Vec<CorrectionFinding>, String> {
    let supplied = &effect.invocation.inputs["report"];
    let request = report_reference(supplied)?;
    if request.report.fence.run != effect.key.fence.run
        || request.source.commit != state.artifact.head
        || request.source.tree != state.artifact.material_digest
        || request.source.generation != state.artifact.generation.to_string()
        || effect.invocation.inputs["context"]["origin"].as_str()
            != Some(request.source.origin.as_str())
    {
        return Err("correction role report is foreign or stale for this source/artifact".into());
    }
    let receipt = super::native_agent::read_report(bound, &request.report, &request.source)?;
    let retained = bound.store.neutral_execution(&request.report.fence.run)?;
    let producer = retained.effect(&request.report)?;
    if super::native_review_policy::policy(&producer.invocation.instance)? != *policy {
        return Err("role report review admission policy differs from assessment".into());
    }
    super::native_review_policy::ordinal(&bound.store, &request.report, policy)?;
    if &receipt != supplied {
        return Err("report input differs from its retained native receipt".into());
    }
    if !matches!(receipt["role"].as_str(), Some("reviewer" | "validator")) {
        return Err("correction findings require an independent reviewer/validator report".into());
    }
    let evidence: ReviewEvidence = serde_json::from_value(receipt["report"]["evidence"].clone())
        .map_err(|e| format!("typed source review evidence: {e}"))?;
    let expected = match &evidence.subject {
        ReviewSubject::Story { id, .. } if id == &request.source.origin => ReviewSubject::Story {
            id: id.clone(),
            commit: state.artifact.head.clone(),
            tree: state.artifact.material_digest.clone(),
            requirements: state.artifact.criteria_digest.clone(),
        },
        ReviewSubject::PullRequest {
            repository, number, ..
        } if format!("{repository}#{number}") == request.source.origin => {
            ReviewSubject::PullRequest {
                repository: repository.clone(),
                number: *number,
                base: state.artifact.base.clone(),
                head: state.artifact.head.clone(),
                tree: state.artifact.material_digest.clone(),
                requirements: state.artifact.criteria_digest.clone(),
            }
        }
        _ => return Err("review subject differs from retained source identity".into()),
    };
    evidence.validate(&expected, &policy.evidence)?;
    let source = FindingSource {
        provider: "native_role_report".into(),
        resource: request.report.digest.clone(),
        revision: content_digest(&receipt)?,
    };
    if state.seen.contains(&source) {
        return Ok(Vec::new());
    }
    let keys: Vec<EffectKey> = serde_json::from_value(
        effect
            .invocation
            .inputs
            .get("proposals")
            .cloned()
            .unwrap_or(serde_json::json!([])),
    )
    .map_err(|e| e.to_string())?;
    let run = bound.store.neutral_execution(&effect.key.fence.run)?;
    let proposals = keys
        .iter()
        .map(|key| {
            if key.fence.run != effect.key.fence.run {
                return Err("proposal receipt belongs to another run".to_owned());
            }
            let owned = run.effect(key)?;
            let Some(super::neutral_effects::EffectObservation {
                stopped: true,
                outcome:
                    super::neutral_effects::EffectOutcome::Completed {
                        result: super::neutral_execution::NeutralResult::Completed { outputs, .. },
                    },
            }) = &owned.observation
            else {
                return Err("proposal is not a stopped native result".into());
            };
            let receipt: super::native_story_create::CreateReceipt = serde_json::from_value(
                outputs
                    .get("receipt")
                    .cloned()
                    .ok_or("proposal receipt absent")?,
            )
            .map_err(|e| e.to_string())?;
            if owned.invocation.descriptor.handler.name != "native.story.create"
                || receipt.effect != *key
                || serde_json::to_value(&receipt.source).map_err(|e| e.to_string())?
                    != owned.invocation.inputs["request"]
                || receipt.review != request.report.digest
                || receipt.source.cap != u32::from(policy.evidence.max_story_proposals)
                || !receipt.source.evidence.contains(&request.report.digest)
            {
                return Err("proposal does not retain this review/cap/evidence".into());
            }
            Ok(receipt)
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut findings = Vec::new();
    for finding in evidence.findings {
        let (disposition, reason) = match finding.decision() {
            TriageDecision::Fix => (
                FindingDisposition::Correct,
                format!("{}: immediate correction", finding.id),
            ),
            TriageDecision::Accept => {
                match finding.accepted_risk.filter(|s| !s.trim().is_empty()) {
                    Some(reason) => (
                        FindingDisposition::IgnoreWithReason,
                        format!("{}: accepted risk: {reason}", finding.id),
                    ),
                    None => (
                        FindingDisposition::NeedsDecision,
                        format!("{}: accepted-risk reason missing", finding.id),
                    ),
                }
            }
            TriageDecision::Story => match proposals.iter().find(|p| {
                Some(&p.proposal) == finding.proposal.as_ref()
                    && p.source.evidence.contains(&finding.id)
            }) {
                Some(p) => (
                    FindingDisposition::IgnoreWithReason,
                    format!(
                        "{}: separate work {} ({})",
                        finding.id, p.story.id, p.responsible
                    ),
                ),
                None => (
                    FindingDisposition::NeedsDecision,
                    format!("{}: separate-work receipt missing", finding.id),
                ),
            },
            TriageDecision::Unresolved => (
                FindingDisposition::NeedsDecision,
                format!("{}: unresolved disposition", finding.id),
            ),
        };
        findings.push(CorrectionFinding {
            source: source.clone(),
            artifact: state.artifact.clone(),
            criterion: "independent_review".into(),
            relevant: true,
            disposition,
            reason,
        });
    }
    // Model-supplied proof metadata never authenticates these additional floors.
    if policy.evidence.require_red
        || policy.evidence.require_strict_clippy
        || !policy.evidence.native_targets.is_empty()
    {
        findings.push(CorrectionFinding{source,artifact:state.artifact.clone(),criterion:"independent_review".into(),relevant:true,disposition:FindingDisposition::NeedsDecision,reason:"Additional RED/strict-Clippy/native-target policy requires its owning native proof binding; model report cannot grant it".into()});
    }
    Ok(findings)
}

fn report_reference(value: &Value) -> Result<super::native_agent::ReportRequest, String> {
    Ok(super::native_agent::ReportRequest {
        report: serde_json::from_value(
            value
                .get("effect")
                .cloned()
                .ok_or("report effect missing")?,
        )
        .map_err(|e| e.to_string())?,
        source: serde_json::from_value(
            value
                .get("source")
                .cloned()
                .ok_or("report source missing")?,
        )
        .map_err(|e| e.to_string())?,
    })
}
