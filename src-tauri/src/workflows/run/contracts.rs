//! Shared wire contracts, not an executor or authority to activate imported policy.
use crate::workflows::PublishedWorkflow;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Versions 2 and 3 retain serial replay; 3 adds immutable definition pins.
/// Version 4 introduces correlated parallel regions without reinterpreting Join.
pub const SERIAL_EVENT_CONTRACT_VERSION: u16 = 2;
pub const PINNED_SERIAL_EVENT_CONTRACT_VERSION: u16 = 3;
pub const PARALLEL_EVENT_CONTRACT_VERSION: u16 = 4;
/// Version 5 adds durable branch-local input without changing versions 2–4 replay.
pub const CURRENT_EVENT_CONTRACT_VERSION: u16 = 5;

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_policy: Option<crate::workflows::AgentPolicy>,
}

impl GraphContract {
    pub(super) fn serial(definition: &PublishedWorkflow) -> Result<Self, String> {
        let bytes = serde_json::to_vec(definition)
            .map_err(|error| format!("encode pinned workflow definition: {error}"))?;
        Ok(Self {
            version: CURRENT_EVENT_CONTRACT_VERSION,
            definition_sha256: hex::encode(Sha256::digest(bytes)),
            // WF09 still owns transitive repository/publication policy imports.
            // This is only the explicitly imported host routing authority.
            policy_imports: vec![],
            agent_policy: None,
        })
    }
    pub(super) fn with_agent_policy(
        definition: &PublishedWorkflow,
        policy: Option<crate::workflows::AgentPolicy>,
    ) -> Result<Self, String> {
        let mut contract = Self::serial(definition)?;
        if let Some(policy) = policy {
            policy.validate_definition(definition)?;
            contract.policy_imports.push(PolicyImport {
                domain: PolicyDomain::GlobalRouting,
                source: policy.source.clone(),
                revision: format!("{}:{}", policy.source_revision, policy.revision),
                sha256: policy.sha256.clone(),
            });
            contract.agent_policy = Some(policy);
        }
        Ok(contract)
    }
}
