//! Immutable domain admission policy; the loop retains its separate correction bound.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReviewAdmissionPolicy {
    pub scope: String,
    pub evidence: super::run::evidence::EvidencePolicy,
}
impl ReviewAdmissionPolicy {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.scope.trim().is_empty() || self.scope.len() > 128 {
            return Err("review admission scope must be named and bounded".into());
        }
        self.evidence.validate()
    }
}
