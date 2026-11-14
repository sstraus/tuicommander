//! Typed correction evidence. This classifies business results, never grants
//! native authority or schedules work. The neutral loop owns the budget.
#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "replacement-artifact constructors are used by authored correction activities"
    )
)]

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CorrectionArtifact {
    pub generation: u64,
    pub head: String,
    pub base: String,
    pub material_digest: String,
    pub criteria_digest: String,
}

/// Provider/resource revision, independent of webhook/poll delivery identity.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FindingSource {
    pub provider: String,
    pub resource: String,
    pub revision: String,
}

impl FindingSource {
    pub(super) fn valid(&self) -> bool {
        [&self.provider, &self.resource, &self.revision]
            .iter()
            .all(|value| !value.trim().is_empty())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FindingDisposition {
    Correct,
    IgnoreWithReason,
    NeedsDecision,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CorrectionFinding {
    pub source: FindingSource,
    pub artifact: CorrectionArtifact,
    pub criterion: String,
    pub relevant: bool,
    pub disposition: FindingDisposition,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EvidenceVerdict {
    Passed,
    CorrectionRequired,
    NeedsDecision,
}

/// Phase B's native-review/check and GitHub adapters supply backend-bound
/// receipts here. Deserializing this value does not authenticate a provider.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CorrectionReceipt {
    pub source: FindingSource,
    pub artifact: CorrectionArtifact,
    pub criterion: String,
    pub verdict: EvidenceVerdict,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CorrectionPolicy {
    pub criteria: BTreeSet<String>,
}

/// Carried data only: no mutable counter, worker identity, or authority grants.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CorrectionState {
    pub artifact: CorrectionArtifact,
    pub seen: BTreeSet<FindingSource>,
    pub findings: Vec<CorrectionFinding>,
    pub evidence: Vec<CorrectionReceipt>,
    pub approval: Option<CorrectionReceipt>,
}

impl CorrectionState {
    pub(crate) fn initial(artifact: CorrectionArtifact) -> Self {
        Self {
            artifact,
            seen: BTreeSet::new(),
            findings: Vec::new(),
            evidence: Vec::new(),
            approval: None,
        }
    }

    /// Called for an observed new artifact within the retained invocation.
    /// Historical findings/receipts remain inspectable; current evidence and
    /// approval are invalidated. The owner's correction scope is untouched.
    pub(crate) fn advance_artifact(&mut self, artifact: CorrectionArtifact) -> Result<(), String> {
        if artifact == self.artifact {
            return Ok(());
        }
        if artifact.generation <= self.artifact.generation {
            return Err("replacement artifact requires a newer generation".into());
        }
        self.artifact = artifact;
        self.evidence.clear();
        self.approval = None;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum CorrectionObservation {
    Evaluation {
        artifact: CorrectionArtifact,
        receipts: Vec<CorrectionReceipt>,
        findings: Vec<CorrectionFinding>,
    },
    /// Technical uncertainty is not a business failure or a new traversal.
    Uncertain { effect: String, reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CorrectionEvaluation {
    /// None routes the loop's unknown port to operator; false means that this
    /// assessment passed, not that a native publication has been authorized.
    Ready {
        state: Box<CorrectionState>,
        repeat: Option<bool>,
    },
    Waiting {
        reason: String,
    },
    Reconcile {
        effect: String,
        reason: String,
    },
}
