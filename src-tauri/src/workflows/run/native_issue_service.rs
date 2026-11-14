//! Issue-domain ports compose scoped evaluation and pinned child dispatch.
//! The installed provider owns execution; graph values never launch an agent.
use super::native_issue_read::IssueReadReceipt;
use super::native_issue_source::IssueSource;
use super::neutral_effects::EffectKey;
use crate::workflows::neutral_publication::PublicationRef;
use crate::workflows::review_verdict::TriageDecision;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, future::Future, pin::Pin};

pub(super) type IssueFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, String>> + Send + 'a>>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct IssueProfile {
    pub profile: String,
    pub model: String,
    pub effort: String,
    pub instructions: String,
}
impl IssueProfile {
    pub(super) fn validate(&self) -> Result<(), String> {
        if [&self.profile, &self.model, &self.effort, &self.instructions]
            .iter()
            .any(|value| value.trim().is_empty())
            || self.instructions.len() > 65_536
            || [&self.profile, &self.model, &self.effort]
                .iter()
                .any(|value| value.len() > 256)
        {
            return Err("issue triage requires an exact bounded scoped profile".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ScopedTriage {
    pub source: IssueSource,
    pub profile: IssueProfile,
    pub decision: TriageDecision,
    pub report: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct IssueChildRequest {
    pub source: IssueSource,
    pub triage: EffectKey,
    pub publication: PublicationRef,
    pub inputs: BTreeMap<String, Value>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct IssueChildReceipt {
    pub source: IssueSource,
    pub triage: EffectKey,
    pub publication: PublicationRef,
    pub child: String,
    pub status: IssueChildStatus,
    pub evidence: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum IssueChildStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
    Uncertain,
}

/// Installed by the backend service owner, never supplied by a graph. The
/// parent effect is the idempotency/ownership key: implementations must observe
/// or cancel that exact child and must never fall back to a fresh dispatch.
pub(super) trait IssueServices: Send + Sync {
    fn triage<'a>(
        &'a self,
        bound: &'a super::native_host::NativeBinding,
        key: &'a EffectKey,
        issue: &'a IssueReadReceipt,
        profile: &'a IssueProfile,
    ) -> IssueFuture<'a, ScopedTriage>;
    /// An owner that cannot own a child refuses before any request or dispatch.
    fn child_dispatch(&self) -> Result<(), String> {
        Ok(())
    }
    fn handoff<'a>(
        &'a self,
        bound: &'a super::native_host::NativeBinding,
        key: &'a EffectKey,
        request: &'a IssueChildRequest,
    ) -> IssueFuture<'a, IssueChildReceipt>;
    fn observe<'a>(&'a self, key: &'a EffectKey) -> IssueFuture<'a, IssueChildReceipt>;
    fn cancel<'a>(&'a self, key: &'a EffectKey) -> IssueFuture<'a, IssueChildReceipt>;
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct IssuePolicy {
    pub accepted_label: String,
    pub rejection_label: String,
    pub needs_input_comment: bool,
}
impl IssuePolicy {
    pub(super) fn validate(&self) -> Result<(), String> {
        if [&self.accepted_label, &self.rejection_label]
            .iter()
            .any(|label| {
                label.trim().is_empty() || label.len() > 256 || label.chars().any(char::is_control)
            })
        {
            return Err("issue policy requires bounded accepted/rejection labels".into());
        }
        Ok(())
    }
}
