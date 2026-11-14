//! Issue identity comes from a retained admission, never a label or graph claim.
use super::native_host::NativeBinding;
use super::neutral_effects::EffectKey;
use crate::workflows::github_trigger::GithubResource;
use crate::workflows::trigger::TriggerSource;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct IssueSource {
    pub receipt: String,
    pub account: String,
    pub repository: String,
    pub number: i64,
    pub generation: String,
    pub material_digest: String,
}
impl IssueSource {
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.number <= 0
            || self.receipt.len() != 64
            || !self.receipt.bytes().all(|c| c.is_ascii_hexdigit())
            || self.account.is_empty()
            || self.account.len() > 256
            || self.generation.is_empty()
            || self.generation.len() > 128
            || self.material_digest.is_empty()
            || self.material_digest.len() > 128
            || crate::workflows::github_trigger::repository(&self.repository)? != self.repository
        {
            return Err("invalid native issue source pin".into());
        }
        Ok(())
    }
    pub(super) fn resource(&self) -> String {
        format!("github:{}/issue/{}", self.repository, self.number)
    }
    /// The same run id survives nested neutral subflows. A caller cannot borrow
    /// another intake's receipt, even when its public input/subject is identical.
    pub(super) fn require(&self, bound: &NativeBinding, key: &EffectKey) -> Result<(), String> {
        self.validate()?;
        bound.context.current(None)?;
        let snapshot = bound.store.neutral_execution(&key.fence.run)?;
        let intent = bound
            .store
            .trigger_intent(&self.receipt)?
            .ok_or("native issue has no retained trigger admission")?;
        if intent.project != snapshot.run.closure.root.project
            || intent.admission.run_id.as_deref() != Some(key.fence.run.as_str())
            || intent.event.source != TriggerSource::Github
            || intent.event.kind != "issue"
            || intent.event.resource != self.resource()
            || intent.event.generation != self.generation
            || intent
                .event
                .values
                .get("material_digest")
                .and_then(Value::as_str)
                != Some(self.material_digest.as_str())
            || intent
                .event
                .values
                .get("repository")
                .and_then(Value::as_str)
                != Some(self.repository.as_str())
            || intent.event.values.get("number").and_then(Value::as_i64) != Some(self.number)
        {
            return Err("native issue source differs from this run's retained admission".into());
        }
        let current =
            bound
                .store
                .retained_github_events(&self.repository, GithubResource::Issue, false)?;
        if !current.iter().any(|event| {
            event.resource == self.resource()
                && event.generation == self.generation
                && event.values.get("material_digest").and_then(Value::as_str)
                    == Some(self.material_digest.as_str())
        }) {
            return Err("native issue generation is superseded or unavailable".into());
        }
        Ok(())
    }
    pub(super) fn require_live(&self, issue: &Value) -> Result<(), String> {
        let current = crate::workflows::github_event::from_rest(
            &self.repository,
            GithubResource::Issue,
            issue,
        )?;
        if !current.open
            || current.number != self.number
            || current.material_digest != self.material_digest
        {
            return Err("native issue requirements or open state changed after admission".into());
        }
        Ok(())
    }
}
