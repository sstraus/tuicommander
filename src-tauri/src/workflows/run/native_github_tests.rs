//! Explicitly authorized private sandbox test, never part of unattended runs.
use super::check::git_output;
use super::native_github::{GitHubAction, GitHubReceipt, GitHubRequest};
use super::native_github_git::{Transfer, absent, transfer};
use super::native_test_support::Fixture;
use super::neutral_effects::{EffectObservation, EffectOutcome};
use super::neutral_execution::NeutralResult;
use crate::github_pr_api::{PrApi, RepoApi};
use futures_util::FutureExt;
use serde_json::Value;

async fn action(f: &Fixture, request: &GitHubRequest) -> Result<GitHubReceipt, String> {
    let run = f.start("publication.github", serde_json::to_value(request).unwrap());
    let waiting = f.dispatch(&run).await;
    let effect = waiting
        .effects
        .values()
        .next()
        .ok_or("publication effect absent")?;
    if effect.dispatched || effect.question.is_none() {
        return Err("publication bypassed exact approval".into());
    }
    let approved = f.approve(&waiting);
    let completed = f.dispatch(&approved).await;
    match &completed
        .effects
        .values()
        .next()
        .ok_or("publication effect absent")?
        .observation
    {
        Some(EffectObservation {
            stopped: true,
            outcome:
                EffectOutcome::Completed {
                    result: NeutralResult::Completed { outputs, .. },
                },
        }) => serde_json::from_value(outputs["receipt"].clone()).map_err(|e| e.to_string()),
        observation => Err(format!("publication did not complete: {observation:?}")),
    }
}

async fn cleanup(
    api: &RepoApi<'_>,
    path: &std::path::Path,
    remote: &str,
    branch: &str,
    commit: &str,
) -> Result<(), String> {
    let mut errors = Vec::new();
    let prs = api
        .rest(
            "GET",
            &format!("pulls?state=open&head={}:{branch}&per_page=100", api.owner),
            Value::Null,
        )
        .await;
    let prs = match prs {
        Ok(value) => value.as_array().cloned().unwrap_or_else(|| {
            errors.push("cleanup PR list missing".into());
            Vec::new()
        }),
        Err(error) => {
            errors.push(error);
            Vec::new()
        }
    };
    for pr in &prs {
        if pr["head"]["ref"] == branch
            && pr["head"]["sha"] == commit
            && let Some(number) = pr["number"].as_u64()
            && let Err(error) = api
                .rest(
                    "PATCH",
                    &format!("pulls/{number}"),
                    serde_json::json!({"state":"closed"}),
                )
                .await
        {
            errors.push(error);
        }
    }
    // Attempt branch cleanup even if PR closure failed; exact lease protects a
    // ref that somebody else changed after this test created it.
    match absent(path, remote, &api.token, branch) {
        Ok(true) => {}
        Ok(false) => {
            if let Err(error) = transfer(
                path,
                remote,
                &api.token,
                Transfer::Delete { branch, commit },
            ) {
                errors.push(error);
            }
        }
        Err(error) => errors.push(error),
    }
    if !absent(path, remote, &api.token, branch)? {
        errors.push("sandbox branch cleanup not observed".into());
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

// Catches publication without exact approvals, destination/artifact drift, and
// intent-only receipts by reading actual sandbox branch/PR/comment state.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires explicitly authorized private GitHub sandbox and command-only GH_TOKEN"]
async fn native_github_live_sandbox_publication() {
    let token = std::env::var("GH_TOKEN").expect("authorized command-only GH_TOKEN required");
    assert_ne!(token, "");
    let f = Fixture::new();
    *f.state.github.token.write() = Some(token);
    let path = f.context.workspace_path();
    let remote = "https://github.com/sstraus/tuic-e2e-sandbox.git";
    git_output(path, &["remote", "add", "origin", remote]).unwrap();
    let api = RepoApi::resolve(&f.state, &path.to_string_lossy())
        .await
        .unwrap();
    assert_eq!(
        format!("{}/{}", api.owner, api.repo),
        "sstraus/tuic-e2e-sandbox"
    );
    // RepoApi::rest is for repository subresources and appends a slash.
    // The repository metadata endpoint itself requires the root without it.
    let request = crate::github::github_rest_request(
        &f.state.http_client,
        reqwest::Method::GET,
        &api.account.host,
        &format!("/repos/{}/{}", api.owner, api.repo),
        &api.token,
        crate::github::GITHUB_JSON,
    );
    let response = crate::github::send_rest_with_breaker(&f.state, &api.account, request)
        .await
        .unwrap();
    assert!(
        response.status().is_success(),
        "sandbox repository preflight refused"
    );
    let metadata: Value = response.json().await.unwrap();
    assert_eq!(metadata["private"], true);
    let base = metadata["default_branch"].as_str().unwrap();
    transfer(path, remote, &api.token, Transfer::Fetch(base)).unwrap();
    git_output(path, &["reset", "--hard", "FETCH_HEAD"]).unwrap();
    let branch = format!("wer-native-{}", uuid::Uuid::new_v4());
    let file = format!("{branch}.txt");
    std::fs::write(
        path.join(&file),
        "Authorized native workflow publication evidence\n",
    )
    .unwrap();
    git_output(path, &["add", "--", &file]).unwrap();
    let commit = tuic_git::git::git_commit_blocking(
        path.to_string_lossy().into_owned(),
        "test: verify native workflow publication".into(),
        Some(false),
    )
    .unwrap();
    let tree = git_output(path, &["rev-parse", "HEAD^{tree}"]).unwrap();
    let request = GitHubRequest {
        source_intent: None,
        account: api.account.id.clone(),
        repository: "sstraus/tuic-e2e-sandbox".into(),
        commit: commit.clone(),
        tree,
        action: GitHubAction::PushBranch {
            branch: branch.clone(),
        },
    };
    let scenario = async {
        if !absent(path, remote, &api.token, &branch)? {
            return Err("unique sandbox branch unexpectedly exists".into());
        }
        action(&f, &request).await?;
        let mut next = request.clone();
        next.action = GitHubAction::OpenPr {
            branch: branch.clone(),
            base: base.into(),
            title: format!("Native workflow evidence {branch}"),
            body: "Authorized isolated workflow test; closed and cleaned by the test.".into(),
        };
        let opened = action(&f, &next).await?;
        let number = opened.number.ok_or("PR number absent")?;
        let url = opened.url.ok_or("PR URL absent")?;
        println!("NATIVE_GITHUB_PR_NUMBER={number} NATIVE_GITHUB_PR_URL={url}");
        next.action = GitHubAction::Comment {
            number,
            body: format!("Native exact approved publication receipt for {commit}"),
        };
        let comment = action(&f, &next).await?;
        if comment.number.is_none() || comment.url.is_none() {
            return Err("comment provider evidence absent".into());
        }
        next.action = GitHubAction::ClosePr { number };
        action(&f, &next).await?;
        next.action = GitHubAction::DeleteBranch {
            branch: branch.clone(),
        };
        action(&f, &next).await?;
        Ok::<_, String>((number, url))
    };
    let result = std::panic::AssertUnwindSafe(scenario).catch_unwind().await;
    let cleanup_result = cleanup(&api, path, remote, &branch, &commit).await;
    assert!(
        cleanup_result.is_ok(),
        "sandbox cleanup failed: {cleanup_result:?}"
    );
    match result {
        Ok(Ok((number, url))) => {
            println!("NATIVE_GITHUB_CLEANED_PR={number} URL={url} BRANCH={branch} COMMIT={commit}")
        }
        Ok(Err(error)) => panic!("native sandbox publication failed after cleanup: {error}"),
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

/// Catches: a user-level `url.<ssh>.insteadOf https://<host>/` rewrite (a common
/// setup, present in Boss's global Git config for github.com) makes every native
/// GitHub branch push, delete and ls-remote refuse before any network action, so
/// publication works only under the isolated empty test Git configuration.
/// Offline: the remote is the closed loopback discard port.
#[test]
fn github_transfer_is_not_refused_by_a_user_level_url_rewrite() {
    let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let global = root.path().join("gitconfig");
    std::fs::write(
        &global,
        "[url \"git@127.0.0.1:\"]\n\tinsteadOf = https://127.0.0.1:9/\n",
    )
    .unwrap();
    // SAFETY: nextest runs every test in its own process; no other thread reads it yet.
    unsafe { std::env::set_var("GIT_CONFIG_GLOBAL", &global) };
    let init = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(&repo)
        .status()
        .unwrap();
    assert!(init.success());
    let error = transfer(
        &repo,
        "https://127.0.0.1:9/owner/repo.git",
        "not-a-token",
        Transfer::Inspect("main"),
    )
    .expect_err("nothing listens on the loopback discard port");
    assert_ne!(
        error, "Git URL rewrite configuration refuses native publication",
        "a user-level URL rewrite must not disable native publication"
    );
}

// Catches a created PR being marked uncertain because GitHub represents its
// empty body as null. No provider response fixture or live success is claimed.
#[test]
fn github_empty_pr_body_null_readback_is_accepted() {
    for body in [Value::Null, serde_json::json!("")] {
        let observed =
            serde_json::json!({"state":"open","base":{"ref":"main"},"title":"Draft","body":body});
        super::native_github::verify_open_pr(&observed, "main", "Draft", "").unwrap();
        assert!(
            super::native_github::verify_open_pr(&observed, "main", "Draft", "required text")
                .is_err()
        );
    }
    let observed = serde_json::json!({"state":"open","base":{"ref":"main"},"title":"Draft","body":"different"});
    assert!(super::native_github::verify_open_pr(&observed, "main", "Draft", "").is_err());
}

// Catches HTTPS being redirected to SSH or a different host despite token scoping.
#[test]
fn github_literal_remote_overrides_host_rewrites_for_fetch_and_push() {
    use super::native_github_git::publication_git;
    let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let path = root.path();
    git_output(path, &["init", "-q"]).unwrap();
    let remote = "https://github.com/owner/repo.git";
    for (key, value) in [
        ("url.git@github.com:.insteadOf", "https://github.com/"),
        ("url.ssh://elsewhere/.pushInsteadOf", "https://github.com/"),
        ("remote.publication.url", remote),
    ] {
        git_output(path, &["config", key, value]).unwrap();
    }
    for args in [
        vec!["remote", "get-url", "publication"],
        vec!["remote", "get-url", "--push", "publication"],
    ] {
        let output = publication_git(path, remote, "not-a-token")
            .args(args)
            .run_raw()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), remote);
    }
    git_output(path, &["config", "url.ssh://elsewhere/.insteadOf", remote]).unwrap();
    assert_eq!(
        transfer(path, remote, "not-a-token", Transfer::Inspect("main")).unwrap_err(),
        "Competing exact Git URL rewrite refuses native publication"
    );
}
