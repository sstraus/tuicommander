//! Read the admitted issue through the workspace's installed GitHub account.
use super::native_host::NativeBinding;
use super::native_issue_source::IssueSource;
use super::neutral_effects::EffectKey;
use crate::github_pr_api::{PrApi, RepoApi};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct IssueReadRequest {}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct IssueReadReceipt {
    pub source: IssueSource,
    pub issue: Value,
    /// The issue as model input: the JSON of `context()`. A workflow binds a
    /// model `text` input to it, so the model sees the title and labels too.
    #[serde(default)]
    pub text: String,
}
impl IssueReadReceipt {
    pub(super) fn new(source: IssueSource, issue: Value) -> Self {
        let mut receipt = Self {
            source,
            issue,
            text: String::new(),
        };
        receipt.text = receipt.context().to_string();
        receipt
    }
    /// Repository, number, title, body and label names of the issue.
    pub(super) fn context(&self) -> Value {
        let labels: Vec<&str> = self.issue["labels"]
            .as_array()
            .map(|labels| labels.iter().filter_map(|l| l["name"].as_str()).collect())
            .unwrap_or_default();
        serde_json::json!({
            "repository": self.source.repository,
            "number": self.source.number,
            "title": self.issue["title"],
            "body": self.issue["body"],
            "labels": labels,
        })
    }
}
pub(super) async fn read(
    bound: &NativeBinding,
    key: &EffectKey,
) -> Result<IssueReadReceipt, String> {
    let state = bound.context.current(None)?;
    let run = bound.store.neutral_execution(&key.fence.run)?;
    let effect = run.effect(key)?;
    if !effect.dispatched
        || effect.cancel_started
        || effect.observation.is_some()
        || run.status != super::neutral_execution::NeutralStatus::Running
        || effect.invocation.descriptor.handler.name != "native.issue.read"
        || run.deadline_ms <= super::store::now_ms()
    {
        return Err("issue read lacks current effect ownership".into());
    }
    let receipt = run
        .run
        .request
        .request_id
        .strip_prefix("trigger:")
        .ok_or("issue read requires a trigger-started run")?;
    let intent = bound
        .store
        .trigger_intent(receipt)?
        .ok_or("issue trigger receipt unavailable")?;
    let api = RepoApi::resolve(&state, &bound.context.workspace_path().to_string_lossy()).await?;
    let source = IssueSource {
        receipt: receipt.into(),
        account: api.account.id.clone(),
        repository: format!("{}/{}", api.owner, api.repo).to_ascii_lowercase(),
        number: intent
            .event
            .values
            .get("number")
            .and_then(Value::as_i64)
            .ok_or("issue source number missing")?,
        generation: intent.event.generation,
        material_digest: intent
            .event
            .values
            .get("material_digest")
            .and_then(Value::as_str)
            .ok_or("issue source digest missing")?
            .into(),
    };
    source.require(bound, key)?;
    let issue = api
        .rest("GET", &format!("issues/{}", source.number), Value::Null)
        .await?;
    source.require_live(&issue)?;
    source.require(bound, key)?;
    Ok(IssueReadReceipt::new(source, issue))
}
