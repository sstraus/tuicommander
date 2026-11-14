//! Explicit private-sandbox acceptance; never part of unattended test runs.
use super::native_issue_effect::{IssueAction, IssueEffectReceipt, IssueEffectRequest};
use super::native_issue_source::IssueSource;
use super::native_test_support::Fixture;
use super::neutral_effects::{EffectObservation, EffectOutcome};
use super::neutral_execution::NeutralResult;
use crate::github_pr_api::{PrApi, RepoApi};
use crate::workflows::github_trigger::GithubResource;
use futures_util::FutureExt;
use serde_json::{Value, json};

async fn effect(
    f: &Fixture,
    api: &RepoApi<'_>,
    issue: &Value,
    action: IssueAction,
) -> Result<IssueEffectReceipt, String> {
    let snapshot = crate::workflows::github_event::from_rest(
        "sstraus/tuic-e2e-sandbox",
        GithubResource::Issue,
        issue,
    )?;
    f.owner.store.normalize_github(snapshot.clone(), None)?;
    let events = f.owner.store.retained_github_events(
        "sstraus/tuic-e2e-sandbox",
        GithubResource::Issue,
        false,
    )?;
    let event = events
        .iter()
        .find(|event| event.values.get("number").and_then(Value::as_i64) == Some(snapshot.number))
        .ok_or("sandbox issue has no retained source event")?;
    let seed = f.start("workspace.pin", json!({}));
    let intent = f.owner.store.reserve_trigger(
        f.context
            .workspace_path()
            .to_str()
            .ok_or("workspace is not UTF8")?,
        "sandbox-intake",
        event.clone(),
        seed.run.request,
        &uuid::Uuid::new_v4().to_string(),
    )?;
    let request = IssueEffectRequest {
        source: IssueSource {
            receipt: intent.receipt.clone(),
            account: api.account.id.clone(),
            repository: "sstraus/tuic-e2e-sandbox".into(),
            number: snapshot.number,
            generation: event.generation.clone(),
            material_digest: snapshot.material_digest,
        },
        action,
    };
    let run = f.start("issue.effect", json!(request));
    f.owner.store.trigger_started(&intent, &run.run.id)?;
    let waiting = f.dispatch(&run).await;
    let effect = waiting
        .effects
        .values()
        .next()
        .ok_or("issue effect absent")?;
    if effect.dispatched || effect.question.is_none() {
        return Err("sandbox issue effect bypassed exact approval".into());
    }
    let approved = f.approve(&waiting);
    let complete = f.dispatch(&approved).await;
    match complete
        .effects
        .values()
        .next()
        .and_then(|effect| effect.observation.as_ref())
    {
        Some(EffectObservation {
            stopped: true,
            outcome:
                EffectOutcome::Completed {
                    result: NeutralResult::Completed { outputs, .. },
                },
        }) => serde_json::from_value(outputs["receipt"].clone()).map_err(|e| e.to_string()),
        other => Err(format!("sandbox issue effect did not complete: {other:?}")),
    }
}
// Catches: account-aware REST publication incompatibility hidden by controlled
// provider tests, and native approval producing no real issue comment/label.
#[tokio::test]
#[ignore = "requires explicitly authorized sstraus/tuic-e2e-sandbox and process GH_TOKEN"]
async fn native_issue_live_sandbox_comment_and_label_have_provider_receipts() {
    let token = std::env::var("GH_TOKEN").expect("command-only sandbox GH_TOKEN required");
    let f = Fixture::new();
    *f.state.github.token.write() = Some(token);
    super::check::git_output(
        f.context.workspace_path(),
        &[
            "remote",
            "add",
            "origin",
            "git@github.com:sstraus/tuic-e2e-sandbox.git",
        ],
    )
    .unwrap();
    let api = RepoApi::resolve(&f.state, &f.context.workspace_path().to_string_lossy())
        .await
        .unwrap();
    assert_eq!(
        format!("{}/{}", api.owner, api.repo),
        "sstraus/tuic-e2e-sandbox"
    );
    let marker = format!("wer023-{}", uuid::Uuid::new_v4());
    let label=api.rest("POST","labels",json!({"name":marker,"color":"ededed","description":"Temporary native issue adapter acceptance"})).await.unwrap();
    assert_eq!(label["name"], marker);
    let created=api.rest("POST","issues",json!({"title":marker,"body":"Temporary native issue source/effect acceptance; closed after verification."})).await;
    let issue = match created {
        Ok(issue) => issue,
        Err(error) => {
            let _ = api
                .rest("DELETE", &format!("labels/{marker}"), Value::Null)
                .await;
            panic!("sandbox issue creation: {error}")
        }
    };
    let number = issue["number"].as_i64().unwrap();
    let mut posted_comment = None;
    let scenario = std::panic::AssertUnwindSafe(async {
        let comment = effect(
            &f,
            &api,
            &issue,
            IssueAction::Comment {
                body: format!("Native exact issue approval {marker}"),
            },
        )
        .await?;
        let id = comment.provider["id"]
            .as_u64()
            .ok_or("comment id missing")?;
        posted_comment = Some(id);
        let observed = api
            .rest("GET", &format!("issues/comments/{id}"), Value::Null)
            .await?;
        if observed["body"] != format!("Native exact issue approval {marker}") {
            return Err("sandbox comment was not observed".into());
        }
        let current = api
            .rest("GET", &format!("issues/{number}"), Value::Null)
            .await?;
        let labelled = effect(
            &f,
            &api,
            &current,
            IssueAction::AddLabel {
                label: marker.clone(),
            },
        )
        .await?;
        if !labelled
            .provider
            .as_array()
            .is_some_and(|labels| labels.iter().any(|item| item["name"] == marker))
        {
            return Err("label receipt lacks approved label".into());
        }
        let observed = api
            .rest("GET", &format!("issues/{number}"), Value::Null)
            .await?;
        if !observed["labels"]
            .as_array()
            .is_some_and(|labels| labels.iter().any(|item| item["name"] == marker))
        {
            return Err("sandbox label was not observed".into());
        }
        api.rest("DELETE", &format!("issues/comments/{id}"), Value::Null)
            .await?;
        posted_comment = None;
        Ok::<_, String>(())
    })
    .catch_unwind()
    .await;
    let comment_removed = match posted_comment {
        Some(id) => {
            api.rest("DELETE", &format!("issues/comments/{id}"), Value::Null)
                .await
        }
        None => Ok(Value::Null),
    };
    // Always close the fixture issue and delete only its unique label, even on failure.
    let closed = api
        .rest(
            "PATCH",
            &format!("issues/{number}"),
            json!({"state":"closed"}),
        )
        .await;
    let removed = api
        .rest("DELETE", &format!("labels/{marker}"), Value::Null)
        .await;
    assert!(
        comment_removed.is_ok(),
        "sandbox fixture comment cleanup failed"
    );
    assert!(closed.is_ok(), "sandbox fixture issue closure failed");
    assert!(removed.is_ok(), "sandbox unique label cleanup failed");
    let observed = api
        .rest("GET", &format!("issues/{number}"), Value::Null)
        .await
        .unwrap();
    assert_eq!(observed["state"], "closed");
    match scenario {
        Ok(result) => result.unwrap(),
        Err(panic) => std::panic::resume_unwind(panic),
    }
}
