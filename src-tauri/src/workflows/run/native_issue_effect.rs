//! One exact issue write per effect; lost provider replies remain uncertain.
use super::native_git::NativeFailure;
use super::native_host::NativeBinding;
use super::native_issue_source::IssueSource;
use super::neutral_effects::EffectKey;
use crate::agent_capabilities::Capability;
use crate::github_pr_api::{PrApi, RepoApi};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct IssueEffectRequest {
    pub source: IssueSource,
    pub action: IssueAction,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum IssueAction {
    Comment { body: String },
    AddLabel { label: String },
    RejectionComment { triage: EffectKey },
    QuestionComment { triage: EffectKey },
    EvidenceComment { child: EffectKey },
    StatusComment { child: EffectKey },
    PublicationStatus { publication: EffectKey },
    AcceptedLabel { triage: EffectKey },
    RejectionLabel { triage: EffectKey },
}
impl IssueEffectRequest {
    pub(super) fn validate(&self) -> Result<(), String> {
        self.source.validate()?;
        let valid = match &self.action {
            IssueAction::Comment { body } => !body.trim().is_empty() && body.len() <= 32_768,
            IssueAction::AddLabel { label } => {
                !label.trim().is_empty()
                    && label.len() <= 256
                    && !label.chars().any(char::is_control)
            }
            _ => true,
        };
        if !valid {
            return Err("invalid native issue action".into());
        }
        Ok(())
    }
    /// The operator question: the action, the issue and the content that the
    /// write sends. Policy actions resolve their text at publication time.
    pub(super) fn approval_title(&self) -> String {
        let issue = format!("{}#{}", self.source.repository, self.source.number);
        match &self.action {
            IssueAction::Comment { body } => {
                format!("Post this comment on issue {issue}?\n\n{body}")
            }
            IssueAction::AddLabel { label } => {
                format!("Add the label \"{label}\" to issue {issue}?")
            }
            IssueAction::RejectionComment { .. } => {
                format!("Post the triage rejection comment on issue {issue}?")
            }
            IssueAction::QuestionComment { .. } => {
                format!("Post the triage question comment on issue {issue}?")
            }
            IssueAction::EvidenceComment { .. } => {
                format!("Post the child evidence comment on issue {issue}?")
            }
            IssueAction::StatusComment { .. } => {
                format!("Post the child status comment on issue {issue}?")
            }
            IssueAction::PublicationStatus { .. } => {
                format!("Post the publication status comment on issue {issue}?")
            }
            IssueAction::AcceptedLabel { .. } => {
                format!("Add the triage accepted label to issue {issue}?")
            }
            IssueAction::RejectionLabel { .. } => {
                format!("Add the triage rejection label to issue {issue}?")
            }
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct IssueEffectReceipt {
    pub source: IssueSource,
    pub action: IssueAction,
    pub effect: EffectKey,
    pub provider: Value,
}

pub(super) async fn publish(
    bound: &NativeBinding,
    key: &EffectKey,
    request: &IssueEffectRequest,
) -> Result<IssueEffectReceipt, NativeFailure> {
    request.validate()?;
    request.source.require(bound, key)?;
    let state = bound.context.authorize(
        &bound.store,
        key,
        "native.issue.effect",
        request,
        Some(Capability::Push),
    )?;
    let api = RepoApi::resolve(&state, &bound.context.workspace_path().to_string_lossy()).await?;
    if api.account.id != request.source.account
        || format!("{}/{}", api.owner, api.repo).to_ascii_lowercase() != request.source.repository
    {
        return Err(
            "native issue account/repository differs from trusted workspace"
                .to_owned()
                .into(),
        );
    }
    publish_with(bound, key, request, &api).await
}

pub(super) async fn publish_with(
    bound: &NativeBinding,
    key: &EffectKey,
    request: &IssueEffectRequest,
    api: &impl PrApi,
) -> Result<IssueEffectReceipt, NativeFailure> {
    request.validate()?;
    request.source.require(bound, key)?;
    bound.context.authorize(
        &bound.store,
        key,
        "native.issue.effect",
        request,
        Some(Capability::Push),
    )?;
    let number = request.source.number;
    let current = api
        .rest("GET", &format!("issues/{number}"), Value::Null)
        .await?;
    request.source.require_live(&current)?;
    // Recheck after the network read: revocation/cancellation while awaiting GET
    // must not turn a previously approved intent into a fresh write.
    request.source.require(bound, key)?;
    bound.context.authorize(
        &bound.store,
        key,
        "native.issue.effect",
        request,
        Some(Capability::Push),
    )?;
    let write = super::native_issue_policy::resolve(bound, key, request)?;
    let (endpoint, body) = match &write {
        super::native_issue_policy::IssueWrite::Comment(body) => {
            (format!("issues/{number}/comments"), json!({"body":body}))
        }
        super::native_issue_policy::IssueWrite::AddLabel(label) => {
            (format!("issues/{number}/labels"), json!({"labels":[label]}))
        }
    };
    let claim = match &write {
        super::native_issue_policy::IssueWrite::AddLabel(label) => {
            // The poll that sees this label must not admit the run's own write
            // as a new labeled event. Claimed before the write, so a poll that
            // races the provider reply still finds it.
            bound.store.record_self_label(
                &request.source.receipt,
                label,
                &crate::workflows::catalog::content_digest(key)?,
            )?;
            Some(label.as_str())
        }
        super::native_issue_policy::IssueWrite::Comment(_) => None,
    };
    let provider = match api.rest("POST", &endpoint, body).await {
        Ok(provider) => provider,
        Err(error) => {
            release_unwritten_claim(bound, request, api, claim).await;
            return Err(NativeFailure::Uncertain(error));
        }
    };
    let valid = match &write {
        super::native_issue_policy::IssueWrite::Comment(body) => {
            provider["id"].as_u64().is_some_and(|id| id > 0)
                && provider["body"].as_str() == Some(body.as_str())
        }
        super::native_issue_policy::IssueWrite::AddLabel(label) => {
            provider.as_array().is_some_and(|labels| {
                labels
                    .iter()
                    .any(|item| item["name"].as_str() == Some(label.as_str()))
            })
        }
    };
    if !valid {
        release_unwritten_claim(bound, request, api, claim).await;
        return Err(NativeFailure::Uncertain(
            "issue write reply lacks matching provider evidence".into(),
        ));
    }
    Ok(IssueEffectReceipt {
        source: request.source.clone(),
        action: request.action.clone(),
        effect: key.clone(),
        provider,
    })
}

/// A failed label write gives its self-event claim back only when the issue
/// shows the label absent. A reply lost after GitHub accepted the write keeps
/// the claim, so the run's own label still never admits a second run.
async fn release_unwritten_claim(
    bound: &NativeBinding,
    request: &IssueEffectRequest,
    api: &impl PrApi,
    label: Option<&str>,
) {
    let Some(label) = label else { return };
    let issue = match api
        .rest(
            "GET",
            &format!("issues/{}", request.source.number),
            Value::Null,
        )
        .await
    {
        Ok(issue) => issue,
        Err(error) => {
            tracing::warn!(source = "workflows", %error, "Issue label claim kept: state unknown");
            return;
        }
    };
    let present = issue["labels"].as_array().is_some_and(|labels| {
        labels.iter().any(|item| {
            item["name"]
                .as_str()
                .is_some_and(|name| name.eq_ignore_ascii_case(label))
        })
    });
    if !present
        && let Err(error) = bound
            .store
            .release_self_label(&request.source.receipt, label)
    {
        tracing::warn!(source = "workflows", %error, "Issue label claim release failed");
    }
}
