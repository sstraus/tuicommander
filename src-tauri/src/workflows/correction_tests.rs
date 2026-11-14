use super::correction::*;
use std::collections::BTreeSet;

pub(super) fn artifact(generation: u64) -> CorrectionArtifact {
    CorrectionArtifact {
        generation,
        head: format!("head-{generation}"),
        base: "base".into(),
        material_digest: format!("tree-{generation}"),
        criteria_digest: "criteria".into(),
    }
}

pub(super) fn source(revision: &str) -> FindingSource {
    FindingSource {
        provider: "internal-contract".into(),
        resource: "check".into(),
        revision: revision.into(),
    }
}

pub(super) fn policy() -> CorrectionPolicy {
    CorrectionPolicy {
        criteria: BTreeSet::from(["checks".into()]),
    }
}

pub(super) fn observation(
    generation: u64,
    revision: &str,
    verdict: EvidenceVerdict,
) -> CorrectionObservation {
    CorrectionObservation::Evaluation {
        artifact: artifact(generation),
        receipts: vec![CorrectionReceipt {
            source: source(revision),
            artifact: artifact(generation),
            criterion: "checks".into(),
            verdict,
            reason: "internal assessment".into(),
        }],
        findings: vec![],
    }
}

// Catches: poll/webhook delivery aliases spending twice, stale evidence releasing
// new work, and a retained approval/check becoming current after a new head.
#[test]
fn correction_assessment_deduplicates_sources_and_invalidates_old_approval() {
    let state = CorrectionState::initial(artifact(1));
    let failed = observation(1, "ci-attempt-1", EvidenceVerdict::CorrectionRequired);
    let CorrectionEvaluation::Ready { mut state, repeat } =
        policy().assess(&state, failed.clone()).unwrap()
    else {
        panic!("assessment missing")
    };
    assert_eq!(repeat, Some(true));
    assert!(matches!(
        policy().assess(&state, failed).unwrap(),
        CorrectionEvaluation::Waiting { .. }
    ));
    state.approval = Some(state.evidence[0].clone());
    let seen = state.seen.clone();
    state.advance_artifact(artifact(2)).unwrap();
    assert!(state.approval.is_none());
    assert_eq!(
        state.evidence,
        [] as [crate::workflows::correction::CorrectionReceipt; 0]
    );
    assert_eq!(state.seen, seen);
    assert!(state.advance_artifact(artifact(1)).is_err());
    assert!(matches!(
        policy()
            .assess(&state, observation(1, "late", EvidenceVerdict::Passed))
            .unwrap(),
        CorrectionEvaluation::Waiting { .. }
    ));
    let CorrectionEvaluation::Ready { repeat, .. } = policy()
        .assess(&state, observation(2, "current", EvidenceVerdict::Passed))
        .unwrap()
    else {
        panic!("current assessment missing")
    };
    assert_eq!(repeat, Some(false));
}

// Catches: silently ignoring an ambiguous finding or dropping its source/reason,
// and treating omitted/contradictory configured checks as a passing assessment.
#[test]
fn correction_findings_keep_source_reason_and_ambiguity_routes_operator() {
    for disposition in [
        FindingDisposition::Correct,
        FindingDisposition::IgnoreWithReason,
        FindingDisposition::NeedsDecision,
    ] {
        let mut batch = observation(1, "check-1", EvidenceVerdict::Passed);
        let finding = CorrectionFinding {
            source: source("comment-1"),
            artifact: artifact(1),
            criterion: "checks".into(),
            relevant: true,
            disposition: disposition.clone(),
            reason: "retained rationale".into(),
        };
        if let CorrectionObservation::Evaluation { findings, .. } = &mut batch {
            findings.push(finding.clone());
        }
        let CorrectionEvaluation::Ready { state, repeat } = policy()
            .assess(&CorrectionState::initial(artifact(1)), batch)
            .unwrap()
        else {
            panic!("assessment missing")
        };
        assert_eq!(state.findings, vec![finding]);
        assert_eq!(
            repeat,
            match disposition {
                FindingDisposition::Correct => Some(true),
                FindingDisposition::IgnoreWithReason => Some(false),
                FindingDisposition::NeedsDecision => None,
            }
        );
    }
    let mut batch = observation(1, "check-1", EvidenceVerdict::Passed);
    if let CorrectionObservation::Evaluation {
        receipts, findings, ..
    } = &mut batch
    {
        receipts.clear();
        findings.push(CorrectionFinding {
            source: source("comment"),
            artifact: artifact(1),
            criterion: "checks".into(),
            relevant: true,
            disposition: FindingDisposition::IgnoreWithReason,
            reason: String::new(),
        });
    }
    assert!(matches!(
        policy()
            .assess(&CorrectionState::initial(artifact(1)), batch)
            .unwrap(),
        CorrectionEvaluation::Ready { repeat: None, .. }
    ));
}

// Catches: using a technical retry as a business correction or fabricating a
// completed body when the original mutation still requires reconciliation.
#[test]
fn correction_unknown_effect_requires_original_reconciliation() {
    assert_eq!(
        policy()
            .assess(
                &CorrectionState::initial(artifact(1)),
                CorrectionObservation::Uncertain {
                    effect: "original-effect".into(),
                    reason: "response lost".into()
                }
            )
            .unwrap(),
        CorrectionEvaluation::Reconcile {
            effect: "original-effect".into(),
            reason: "response lost".into()
        }
    );
}
