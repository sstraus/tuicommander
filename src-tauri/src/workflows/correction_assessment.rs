//! Pure adapter seam: only committed body outputs can advance the neutral loop.
use super::correction::{
    CorrectionEvaluation, CorrectionObservation, CorrectionPolicy, CorrectionState,
    EvidenceVerdict, FindingDisposition,
};
use std::collections::BTreeSet;

impl CorrectionPolicy {
    pub(crate) fn assess(
        &self,
        previous: &CorrectionState,
        observation: CorrectionObservation,
    ) -> Result<CorrectionEvaluation, String> {
        if self.criteria.is_empty() || self.criteria.iter().any(|name| name.trim().is_empty()) {
            return Err("correction requires named assessment criteria".into());
        }
        let CorrectionObservation::Evaluation {
            artifact,
            receipts,
            findings,
        } = observation
        else {
            let CorrectionObservation::Uncertain { effect, reason } = observation else {
                unreachable!()
            };
            if effect.trim().is_empty() || reason.trim().is_empty() {
                return Err("uncertainty requires the original effect and reason".into());
            }
            return Ok(CorrectionEvaluation::Reconcile { effect, reason });
        };
        if artifact != previous.artifact {
            return Ok(CorrectionEvaluation::Waiting {
                reason: "assessment does not cover the current artifact".into(),
            });
        }
        let mut state = previous.clone();
        let mut operator = false;
        let mut correct = false;
        let sources: BTreeSet<_> = receipts
            .iter()
            .map(|r| r.source.clone())
            .chain(findings.iter().map(|f| f.source.clone()))
            .collect();
        if !sources.is_empty() && sources.iter().all(|source| state.seen.contains(source)) {
            return Ok(CorrectionEvaluation::Waiting {
                reason: "no new assessment revision".into(),
            });
        }
        // A batch is for exactly one artifact. Do not combine an old passing
        // review with a new failed check or silently discard a stale finding.
        if receipts.iter().any(|r| r.artifact != artifact)
            || findings.iter().any(|f| f.artifact != artifact)
        {
            return Ok(CorrectionEvaluation::Waiting {
                reason: "mixed artifact assessment".into(),
            });
        }
        for receipt in &receipts {
            if !receipt.source.valid()
                || receipt.reason.trim().is_empty()
                || !self.criteria.contains(&receipt.criterion)
            {
                operator = true;
            }
            match receipt.verdict {
                EvidenceVerdict::Passed => {}
                EvidenceVerdict::CorrectionRequired => correct = true,
                EvidenceVerdict::NeedsDecision => operator = true,
            }
        }
        // Exactly one result for each configured criterion. Missing and
        // contradictory results are questions, never a successful assessment.
        for criterion in &self.criteria {
            if receipts
                .iter()
                .filter(|r| &r.criterion == criterion)
                .count()
                != 1
            {
                operator = true;
            }
        }
        for finding in &findings {
            if !finding.relevant {
                continue;
            }
            if !finding.source.valid()
                || finding.reason.trim().is_empty()
                || !self.criteria.contains(&finding.criterion)
            {
                operator = true;
            }
            match finding.disposition {
                FindingDisposition::Correct => correct = true,
                FindingDisposition::IgnoreWithReason => {}
                FindingDisposition::NeedsDecision => operator = true,
            }
        }
        state.seen.extend(sources);
        state.findings.extend(findings);
        state.evidence = receipts;
        Ok(CorrectionEvaluation::Ready {
            state: Box::new(state),
            repeat: if operator { None } else { Some(correct) },
        })
    }
}
