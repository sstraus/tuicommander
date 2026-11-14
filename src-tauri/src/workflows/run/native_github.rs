//! Account-aware native publication, correlated by the existing exact effect ledger.
mod intent;
mod review;
mod source;
#[cfg(test)]
mod source_tests;
use super::native_git::NativeFailure;
use super::native_github_git::{Transfer, transfer};
use super::native_host::NativeBinding;
use super::neutral_effects::EffectKey;
use crate::agent_capabilities::Capability;
use crate::github_pr_api::{PrApi, RepoApi};
use review::ReviewVerdict;
pub(super) use review::verify_open_pr;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GitHubRequest {
    pub account: String,
    pub repository: String,
    pub commit: String,
    pub tree: String,
    pub action: GitHubAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_intent: Option<crate::workflows::PublicationAction>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum GitHubAction {
    PushBranch {
        branch: String,
    },
    OpenPr {
        branch: String,
        base: String,
        title: String,
        body: String,
    },
    Comment {
        number: u64,
        body: String,
    },
    MarkReady {
        number: u64,
    },
    Review {
        base: String,
        review: EffectKey,
        number: u64,
        body: String,
        verdict: ReviewVerdict,
    },
    ClosePr {
        number: u64,
    },
    DeleteBranch {
        branch: String,
    },
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GitHubReceipt {
    pub account: String,
    pub repository: String,
    pub commit: String,
    pub action: GitHubAction,
    pub number: Option<u64>,
    pub url: Option<String>,
    pub source_generation: Option<String>,
}
fn segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        && value != "."
        && value != ".."
}
fn branch(value: &str) -> bool {
    !value.is_empty() && value.len() <= 200 && value.split('/').all(segment)
}
pub(super) fn validate(r: &GitHubRequest) -> Result<(), String> {
    if let Some(intent) = r.source_intent {
        intent::require(intent, &r.action)?;
    }
    let parts: Vec<_> = r.repository.split('/').collect();
    let hash =
        |s: &str| (s.len() == 40 || s.len() == 64) && s.bytes().all(|b| b.is_ascii_hexdigit());
    if !segment(&r.account)
        || parts.len() != 2
        || !parts.iter().all(|p| segment(p))
        || !hash(&r.commit)
        || !hash(&r.tree)
    {
        return Err("Invalid native GitHub account/repository/artifact".into());
    }
    let valid = match &r.action {
        GitHubAction::PushBranch { branch: b } | GitHubAction::DeleteBranch { branch: b } => {
            branch(b)
        }
        GitHubAction::OpenPr {
            branch: b,
            base,
            title,
            body,
        } => {
            branch(b)
                && branch(base)
                && !title.trim().is_empty()
                && title.len() <= 256
                && body.len() <= 32_768
        }
        GitHubAction::Comment { number, body } | GitHubAction::Review { number, body, .. } => {
            *number > 0 && !body.trim().is_empty() && body.len() <= 32_768
        }
        GitHubAction::ClosePr { number } | GitHubAction::MarkReady { number } => *number > 0,
    };
    if !valid {
        return Err("Invalid native GitHub action".into());
    }
    Ok(())
}
use review::{branch_head, pr};
pub(super) async fn publish(
    bound: &NativeBinding,
    key: &EffectKey,
    r: &GitHubRequest,
) -> Result<GitHubReceipt, NativeFailure> {
    publish_owned(bound, key, r, "native.publication.github", r).await
}

pub(super) async fn publish_owned<T: Serialize + serde::de::DeserializeOwned + Sync>(
    bound: &NativeBinding,
    key: &EffectKey,
    r: &GitHubRequest,
    handler: &str,
    original: &T,
) -> Result<GitHubReceipt, NativeFailure> {
    validate(r)?;
    let context = &bound.context;
    let state = context.authorize(&bound.store, key, handler, original, Some(Capability::Push))?;
    context.require_artifact(&r.commit, &r.tree)?;
    let path = context.workspace_path();
    let api = RepoApi::resolve(&state, &path.to_string_lossy()).await?;
    if api.account.id != r.account || format!("{}/{}", api.owner, api.repo) != r.repository {
        return Err(NativeFailure::Refused(
            "GitHub destination/account differs from approved request".into(),
        ));
    }
    let source = source::current(&bound.store, &key.fence.run, r)?;
    let remote = format!("https://{}/{}.git", api.account.host.as_str(), r.repository);
    let mut number = None;
    let mut url = None;
    match &r.action {
        GitHubAction::PushBranch { branch } => {
            super::check::git_output(path, &["check-ref-format", &format!("refs/heads/{branch}")])?;
            transfer(
                path,
                &remote,
                &api.token,
                Transfer::Create {
                    branch,
                    commit: &r.commit,
                },
            )
            .map_err(NativeFailure::Uncertain)?;
            if branch_head(&api, branch)
                .await
                .map_err(NativeFailure::Uncertain)?
                != r.commit
            {
                return Err(NativeFailure::Uncertain("GitHub pushed ref differs".into()));
            }
        }
        GitHubAction::OpenPr {
            branch,
            base,
            title,
            body,
        } => {
            if branch_head(&api, branch).await? != r.commit {
                return Err(NativeFailure::Refused(
                    "GitHub branch differs from approved artifact".into(),
                ));
            }
            let created = crate::github::create_pr_with(&api, title, body, base, branch, true)
                .await
                .map_err(NativeFailure::Uncertain)?;
            let id = u64::try_from(created.number)
                .map_err(|_| NativeFailure::Uncertain("GitHub PR number invalid".into()))?;
            let observed = pr(&api, id, r).await.map_err(NativeFailure::Uncertain)?;
            verify_open_pr(&observed, base, title, body)?;
            number = Some(id);
            url = Some(created.url);
        }
        GitHubAction::Comment { number: id, body } => {
            pr(&api, *id, r).await?;
            let created = api
                .rest(
                    "POST",
                    &format!("issues/{id}/comments"),
                    json!({"body":body}),
                )
                .await
                .map_err(NativeFailure::Uncertain)?;
            let comment = created["id"]
                .as_u64()
                .ok_or_else(|| NativeFailure::Uncertain("GitHub comment id missing".into()))?;
            let observed = api
                .rest("GET", &format!("issues/comments/{comment}"), Value::Null)
                .await
                .map_err(NativeFailure::Uncertain)?;
            if observed["body"] != *body {
                return Err(NativeFailure::Uncertain(
                    "GitHub comment read-back differs".into(),
                ));
            }
            number = Some(comment);
            url = observed["html_url"].as_str().map(str::to_owned);
        }
        GitHubAction::Review {
            number: id,
            body,
            verdict,
            base,
            review: evidence,
        } => {
            let observed =
                review::publish_approved(&api, bound, key, r, *id, body, *verdict, base, evidence)
                    .await?;
            number = observed["id"].as_u64();
            url = observed["html_url"].as_str().map(str::to_owned);
        }
        GitHubAction::MarkReady { number: id } => {
            let observed = pr(&api, *id, r).await?;
            review::source_base(source.as_ref(), &observed)?;
            source::require_issue_ready_base(
                &bound.store,
                &key.fence.run,
                r,
                source.as_ref(),
                &observed,
            )?;
            review::mark_ready(&api, *id, &r.commit, &observed).await?;
            number = Some(*id);
            url = observed["html_url"].as_str().map(str::to_owned);
        }
        GitHubAction::ClosePr { number: id } => {
            pr(&api, *id, r).await?;
            api.rest("PATCH", &format!("pulls/{id}"), json!({"state":"closed"}))
                .await
                .map_err(NativeFailure::Uncertain)?;
            let observed = pr(&api, *id, r).await.map_err(NativeFailure::Uncertain)?;
            if observed["state"] != "closed" {
                return Err(NativeFailure::Uncertain(
                    "GitHub PR close not observed".into(),
                ));
            }
            number = Some(*id);
            url = observed["html_url"].as_str().map(str::to_owned);
        }
        GitHubAction::DeleteBranch { branch } => {
            if branch_head(&api, branch).await? != r.commit {
                return Err(NativeFailure::Refused("GitHub delete head differs".into()));
            }
            transfer(
                path,
                &remote,
                &api.token,
                Transfer::Delete {
                    branch,
                    commit: &r.commit,
                },
            )
            .map_err(NativeFailure::Uncertain)?;
            // ls-remote distinguishes an absent ref from an API/network failure.
            let output = super::native_github_git::absent(path, &remote, &api.token, branch)
                .map_err(NativeFailure::Uncertain)?;
            if !output {
                return Err(NativeFailure::Uncertain(
                    "GitHub deleted branch still exists".into(),
                ));
            }
        }
    }
    context
        .authorize(&bound.store, key, handler, original, Some(Capability::Push))
        .map_err(NativeFailure::Uncertain)?;
    context
        .require_artifact(&r.commit, &r.tree)
        .map_err(NativeFailure::Uncertain)?;
    source::current(&bound.store, &key.fence.run, r).map_err(NativeFailure::Uncertain)?;
    Ok(GitHubReceipt {
        source_generation: source.map(|intent| intent.event.generation),
        account: r.account.clone(),
        repository: r.repository.clone(),
        commit: r.commit.clone(),
        action: r.action.clone(),
        number,
        url,
    })
}
