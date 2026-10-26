use super::*;
use serde_json::{Value, json};
use std::sync::Mutex;

const SANDBOX: &str = "sstraus/tuic-test-sandbox";

#[derive(Default)]
struct Stub {
    calls: Mutex<Vec<(String, String, Value)>>,
}

impl PrApi for Stub {
    async fn graphql(&self, _: &str, _: Value) -> Result<Value, String> {
        panic!("publishing must use REST");
    }

    fn rest(
        &self,
        method: &str,
        endpoint: &str,
        body: Value,
    ) -> impl std::future::Future<Output = Result<Value, String>> + Send {
        self.calls
            .lock()
            .unwrap()
            .push((method.into(), endpoint.into(), body));
        std::future::ready(Ok(json!({
            "number": 9, "html_url": format!("https://github.com/{SANDBOX}/pull/9"), "title": "t"
        })))
    }
}

fn intake() -> EventIntake {
    EventIntake {
        repositories: vec![SANDBOX.into()],
        label: "tuic-run".into(),
        definition_id: "pilot".into(),
    }
}

fn request<'a>(repository: &'a str, head: &'a str) -> DraftPr<'a> {
    DraftPr {
        repository,
        head,
        base: "main",
        issue_number: 3,
    }
}

// Catches a publisher that reaches the API for a repository outside the allowlist,
// for a workspace bound to a different repository, or when the allowlist is empty.
#[tokio::test]
async fn nothing_is_published_outside_the_allowlist_or_for_another_bound_repository() {
    let api = Stub::default();
    for (repository, bound) in [
        ("sstraus/other", "sstraus/other"),
        ("evil/tuic-test-sandbox", "evil/tuic-test-sandbox"),
        (SANDBOX, "sstraus/other"),
        ("sstraus/other", SANDBOX),
    ] {
        let result = publish_draft_pr(&intake(), &api, bound, &request(repository, "fix-3")).await;
        assert!(result.is_err(), "{repository} / {bound}");
    }
    let result = publish_draft_pr(
        &EventIntake::default(),
        &api,
        SANDBOX,
        &request(SANDBOX, "fix-3"),
    )
    .await;
    assert!(result.is_err());
    assert_eq!(*api.calls.lock().unwrap(), Vec::new());
}

// Catches a PR that is not a draft, or that loses its head, base or issue link.
#[tokio::test]
async fn the_pr_is_a_draft_with_the_issue_link_on_the_bound_repository() {
    let api = Stub::default();
    let url = publish_draft_pr(
        &intake(),
        &api,
        "SStraus/TUIC-test-sandbox",
        &request(SANDBOX, "fix-3"),
    )
    .await
    .unwrap();
    assert!(url.ends_with("/pull/9"));
    let calls = api.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        (calls[0].0.as_str(), calls[0].1.as_str()),
        ("POST", "pulls")
    );
    assert_eq!(calls[0].2["draft"], true);
    assert_eq!(calls[0].2["head"], "fix-3");
    assert_eq!(calls[0].2["base"], "main");
    assert!(
        calls[0].2["body"]
            .as_str()
            .unwrap()
            .starts_with("Closes #3")
    );
}

// Catches a branch name that is passed on as an option, or a detached HEAD.
#[tokio::test]
async fn an_option_looking_or_detached_head_is_refused() {
    let api = Stub::default();
    for head in ["--web", "HEAD", "", "a b"] {
        assert!(
            publish_draft_pr(&intake(), &api, SANDBOX, &request(SANDBOX, head))
                .await
                .is_err()
        );
    }
    assert_eq!(*api.calls.lock().unwrap(), Vec::new());
}
