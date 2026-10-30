//! Shared wire contracts, not an executor or authority to activate imported policy.
use crate::workflows::PublishedWorkflow;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Version 2 remains the serial replay contract. Version 3 adds immutable pins;
/// it deliberately does not introduce Fork/all-Join or branch-local questions.
pub const SERIAL_EVENT_CONTRACT_VERSION: u16 = 2;
pub const CURRENT_EVENT_CONTRACT_VERSION: u16 = 3;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PolicyDomain {
    GlobalRouting,
    RepositoryGates,
    PublicationAuthority,
}

/// Import exact source bytes/revision, not a mutable path or an inferred grant.
/// Global routing is authoritative; repository gates can add requirements but
/// cannot weaken it. Publication always needs separate, explicit authority.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PolicyImport {
    pub domain: PolicyDomain,
    pub source: String,
    pub revision: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GraphContract {
    pub version: u16,
    pub definition_sha256: String,
    /// Empty means only the legacy native graph/check/closure policy is pinned.
    /// It does not mean global routing or publication authority was imported.
    pub policy_imports: Vec<PolicyImport>,
}

impl GraphContract {
    pub(super) fn serial(definition: &PublishedWorkflow) -> Result<Self, String> {
        let bytes = serde_json::to_vec(definition)
            .map_err(|error| format!("encode pinned workflow definition: {error}"))?;
        Ok(Self {
            version: CURRENT_EVENT_CONTRACT_VERSION,
            definition_sha256: hex::encode(Sha256::digest(bytes)),
            // DEFERRED (2026-10-30) — WF09 resolves transitive policy imports at
            // publication/start. WF01 must not invent installed or approved policy.
            policy_imports: vec![],
        })
    }
}
