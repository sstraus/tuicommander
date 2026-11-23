//! Manual PR review start: binding selection, refusals and the head GitHub
//! reports at the moment of the request, on the real PR review template.
use super::neutral::NeutralStartRequest;
use super::neutral_execution::NeutralStatus;
use super::neutral_pr_review::{self as pr, PullRequestUrl};
use super::pr_review_workflow_tests::{Fixture, fixture};
use crate::workflows::neutral_api::refusal::{self, Refusal, classify};
use serde_json::json;

const URL: &str = "https://github.com/octo-org/example/pull/1347";

async fn request(f: &Fixture) -> Result<NeutralStartRequest, String> {
    let pull = pr::parse_url(URL)?;
    let binding = pr::select(
        pr::bindings(&f.workflows, &f.project, &pull.repository)?,
        &pull.repository,
        None,
    )?;
    pr::start_request(
        f.github.as_ref(),
        &f.workflows,
        &f.catalog,
        &f.handlers,
        &f.project,
        &binding,
        &pull,
    )
    .await
}

fn move_head(f: &Fixture, head: char) {
    f.github.state.lock().head = head.to_string().repeat(40);
}

// Catches: a manual review of an old head — the start reusing the head the
// poller retained (or a head the page had loaded) after a push GitHub already
// reports, so the review lands on a commit that is no longer the PR head.
#[tokio::test]
async fn a_manual_review_reviews_the_head_github_reports_at_the_request() {
    let f = fixture();
    f.poll(Some('a'));
    move_head(&f, 'b');
    let request = request(&f).await.unwrap();
    assert_eq!(request.inputs["head"], json!("b".repeat(40)));
    let run = f
        .owner
        .start_neutral(&f.workflows, &request, &f.catalog, &f.handlers)
        .unwrap();
    assert_eq!(
        f.drive(&run.id).await,
        NeutralStatus::Completed {
            outcome: "published".into()
        }
    );
    assert!(f.github.reviewed_heads().contains(&"b".repeat(40)));
}

// Catches: a second click on the same head starting a second run, which posts
// a second COMMENT review on that head; and a push reusing the old run.
#[tokio::test]
async fn the_same_head_keeps_the_request_identity_and_a_push_changes_it() {
    let f = fixture();
    f.poll(Some('a'));
    let first = request(&f).await.unwrap();
    assert_eq!(first, request(&f).await.unwrap());
    move_head(&f, 'c');
    let pushed = request(&f).await.unwrap();
    assert_ne!(first.request_id, pushed.request_id);
}

// Catches: a project without a PR review binding for the repository starting
// another workflow, or failing as a server fault instead of a refusal the UI
// can show.
#[test]
fn a_repository_without_a_binding_refuses_as_not_found() {
    let f = fixture();
    let error = pr::select(
        pr::bindings(&f.workflows, &f.project, "octo-org/other").unwrap(),
        "octo-org/other",
        None,
    )
    .unwrap_err();
    assert!(
        error.starts_with(refusal::PR_REVIEW_NOT_CONFIGURED),
        "{error}"
    );
    assert_eq!(classify(&error), Some(Refusal::NotFound));
    assert!(
        pr::targets(&f.workflows, &f.project, "octo-org/other")
            .unwrap()
            .is_empty()
    );
}

// Catches: guessing between two PR review workflows of one repository — the
// start picking the first binding instead of refusing with the choices.
#[test]
fn several_bindings_refuse_until_the_caller_names_one() {
    let f = fixture();
    let mut second = pr::bindings(&f.workflows, &f.project, "octo-org/example").unwrap()[0]
        .draft
        .clone();
    second.id = "second-review".into();
    f.workflows
        .save_neutral_trigger_binding(&f.project, 0, second, &f.catalog, &f.handlers)
        .unwrap();
    let candidates = || pr::bindings(&f.workflows, &f.project, "octo-org/example").unwrap();
    let error = pr::select(candidates(), "octo-org/example", None).unwrap_err();
    assert!(error.starts_with(refusal::PR_REVIEW_AMBIGUOUS), "{error}");
    assert!(error.contains("pr-review-binding") && error.contains("second-review"));
    assert_eq!(classify(&error), Some(Refusal::Conflict));
    let chosen = pr::select(candidates(), "octo-org/example", Some("second-review")).unwrap();
    assert_eq!(chosen.draft.id, "second-review");
    let listed = pr::targets(&f.workflows, &f.project, "octo-org/example").unwrap();
    assert_eq!(listed.len(), 2);
    assert!(
        listed
            .iter()
            .any(|t| t.binding_id == "second-review" && !t.enabled)
    );
}

// Catches: an issue URL, a non-https page or a malformed number starting a
// review of whatever pull request carries the parsed number.
#[test]
fn only_a_pull_request_url_names_a_pull_request() {
    assert_eq!(
        pr::parse_url("https://github.com/Octo-Org/Example/pull/1347/files?w=1").unwrap(),
        PullRequestUrl {
            host: "github.com".into(),
            repository: "octo-org/example".into(),
            number: 1347,
        }
    );
    for url in [
        "https://github.com/octo-org/example/issues/1347",
        "http://github.com/octo-org/example/pull/1347",
        "https://github.com/octo-org/example/pull/0",
        "https://github.com/octo-org/example/pull/x",
        "https://github.com/octo-org/pull/1347",
        "not a url",
    ] {
        let error = pr::parse_url(url).unwrap_err();
        assert_eq!(classify(&error), Some(Refusal::Invalid), "{url}: {error}");
    }
}
