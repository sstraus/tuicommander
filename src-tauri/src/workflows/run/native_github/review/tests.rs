use super::*;
use std::sync::Mutex;
type Step = (String, String, Value, Result<Value, String>);
struct Api(Mutex<Vec<Step>>);
impl PrApi for Api {
    async fn graphql(&self, query: &str, variables: Value) -> Result<Value, String> {
        assert!(query.contains("markPullRequestReadyForReview"));
        self.rest("GRAPHQL", "ready", variables).await
    }
    fn rest(
        &self,
        method: &str,
        path: &str,
        body: Value,
    ) -> impl Future<Output = Result<Value, String>> {
        let (m, p, b, result) = self.0.lock().unwrap().remove(0);
        assert_eq!((method, path, body), (m.as_str(), p.as_str(), b));
        std::future::ready(result)
    }
}
fn step(m: &str, p: &str, b: Value, r: Value) -> Step {
    (m.into(), p.into(), b, Ok(r))
}
/// GitHub's documented review wire values, written out independently of serde.
fn wire(v: ReviewVerdict) -> (&'static str, &'static str) {
    match v {
        ReviewVerdict::Comment => ("COMMENT", "COMMENTED"),
        ReviewVerdict::Approve => ("APPROVE", "APPROVED"),
        ReviewVerdict::RequestChanges => ("REQUEST_CHANGES", "CHANGES_REQUESTED"),
    }
}
fn head() -> Value {
    json!({"state":"open","head":{"sha":"exact"},"base":{"sha":"base"}})
}
fn receipt(v: ReviewVerdict) -> Value {
    json!({"id":7,"commit_id":"exact","state":wire(v).1,"body":"Review","html_url":"https://example.test/review/7"})
}
fn api(v: ReviewVerdict, observed: Value, after: Value) -> Api {
    Api(Mutex::new(vec![
        step("GET", "pulls/3", Value::Null, head()),
        step(
            "POST",
            "pulls/3/reviews",
            json!({"commit_id":"exact","body":"Review","event":wire(v).0}),
            json!({"id":7}),
        ),
        step("GET", "pulls/3/reviews/7", Value::Null, observed),
        step("GET", "pulls/3", Value::Null, after),
    ]))
}
// Catches issue-comment substitution and missing commit_id or verdict.
#[tokio::test]
async fn review_publication_pins_all_three_verdicts() {
    for v in [
        ReviewVerdict::Comment,
        ReviewVerdict::Approve,
        ReviewVerdict::RequestChanges,
    ] {
        let a = api(v, receipt(v), head());
        assert_eq!(
            publish(&a, 3, "exact", "base", "Review", v).await.unwrap()["id"],
            7
        );
        assert!(a.0.lock().unwrap().is_empty());
    }
}
// Catches stale intent sent to GitHub despite changed PR head.
#[tokio::test]
async fn review_publication_refuses_stale_head_before_write() {
    let a = Api(Mutex::new(vec![step(
        "GET",
        "pulls/3",
        Value::Null,
        json!({"state":"open","head":{"sha":"other"}}),
    )]));
    assert!(matches!(
        publish(&a, 3, "exact", "base", "Review", ReviewVerdict::Comment).await,
        Err(NativeFailure::Refused(_))
    ));
    assert!(a.0.lock().unwrap().is_empty());
}
// Catches intent-only success despite wrong observed verdict, body, identity or head.
#[tokio::test]
async fn review_publication_divergent_readback_stays_uncertain() {
    for field in ["commit_id", "state", "body", "id", "html_url"] {
        let v = ReviewVerdict::Approve;
        let mut observed = receipt(v);
        observed[field] = Value::Null;
        assert!(matches!(
            publish(&api(v, observed, head()), 3, "exact", "base", "Review", v).await,
            Err(NativeFailure::Uncertain(_))
        ));
    }
    let v = ReviewVerdict::Comment;
    assert!(matches!(
        publish(
            &api(v, receipt(v), json!({"head":{"sha":"new"}})),
            3,
            "exact",
            "base",
            "Review",
            v
        )
        .await,
        Err(NativeFailure::Uncertain(_))
    ));
}
// Catches a possible accepted write being reported as safe to retry.
#[tokio::test]
async fn review_publication_lost_post_response_stays_uncertain() {
    let a = api(
        ReviewVerdict::Comment,
        receipt(ReviewVerdict::Comment),
        head(),
    );
    a.0.lock().unwrap()[1].3 = Err("connection lost".into());
    assert!(matches!(
        publish(&a, 3, "exact", "base", "Review", ReviewVerdict::Comment).await,
        Err(NativeFailure::Uncertain(_))
    ));
}

fn draft() -> Value {
    json!({"number":3,"node_id":"PR_exact","state":"open","draft":true,"head":{"sha":"exact"}})
}
// Catches MarkReady targeting another PR, a moved head, or claiming success without read-back.
#[tokio::test]
async fn review_publication_mark_ready_pins_node_and_observes_state() {
    let mut ready = draft();
    ready["draft"] = json!(false);
    let make = |observed| {
        Api(Mutex::new(vec![
            step(
                "GRAPHQL",
                "ready",
                json!({"id":"PR_exact"}),
                json!({"data":{}}),
            ),
            step("GET", "pulls/3", Value::Null, observed),
        ]))
    };
    mark_ready(&make(ready.clone()), 3, "exact", &draft())
        .await
        .unwrap();
    for field in ["node_id", "draft", "head", "state"] {
        let mut changed = ready.clone();
        changed[field] = Value::Null;
        assert!(matches!(
            mark_ready(&make(changed), 3, "exact", &draft()).await,
            Err(NativeFailure::Uncertain(_))
        ));
    }
    let empty = Api(Mutex::new(vec![]));
    assert!(matches!(
        mark_ready(&empty, 3, "different", &draft()).await,
        Err(NativeFailure::Refused(_))
    ));
}
// Catches APPROVE or REQUEST_CHANGES published against contrary independent review
// evidence, a body the reviewer did not write, or a review key from another run.
#[tokio::test]
async fn review_publication_verdict_requires_matching_independent_evidence() {
    use crate::workflows::review_verdict::CodeVerdict;
    use crate::workflows::run::check::git_output;
    use crate::workflows::run::native_host::NativeBinding;
    use crate::workflows::run::native_review::{ReviewRequest, observed_review};
    use crate::workflows::run::native_review_tests::reviewed_request;
    use crate::workflows::run::native_test_support::Fixture;
    let f = Fixture::new();
    let path = f.context.workspace_path();
    let base = git_output(path, &["rev-parse", "HEAD"]).unwrap();
    std::fs::write(path.join("artifact.txt"), "candidate").unwrap();
    git_output(path, &["add", "."]).unwrap();
    git_output(path, &["commit", "-m", "candidate"]).unwrap();
    let commit = git_output(path, &["rev-parse", "HEAD"]).unwrap();
    let tree = git_output(path, &["rev-parse", "HEAD^{tree}"]).unwrap();
    let bound = NativeBinding {
        context: f.context.clone(),
        store: f.owner.store.clone(),
    };
    let request = super::super::GitHubRequest {
        source_intent: None,
        account: "default".into(),
        repository: "owner/repo".into(),
        commit: commit.clone(),
        tree: tree.clone(),
        action: super::super::GitHubAction::ClosePr { number: 3 },
    };
    let none = || Api(Mutex::new(vec![]));
    let mut first = None;
    for (evidence, contrary, matching) in [
        (
            CodeVerdict::Findings,
            ReviewVerdict::Approve,
            ReviewVerdict::RequestChanges,
        ),
        (
            CodeVerdict::Pass,
            ReviewVerdict::RequestChanges,
            ReviewVerdict::Approve,
        ),
    ] {
        let key = reviewed_request(
            &f,
            ReviewRequest {
                base: base.clone(),
                head: Some(commit.clone()),
                tree: tree.clone(),
            },
            evidence,
        )
        .await;
        let body = observed_review(&f.context, &f.owner.store, &key, &base, &commit, &tree)
            .unwrap()
            .body;
        let empty = none();
        assert!(matches!(
            publish_approved(
                &empty, &bound, &key, &request, 3, &body, contrary, &base, &key
            )
            .await,
            Err(NativeFailure::Refused(_))
        ));
        assert!(matches!(
            publish_approved(
                &empty,
                &bound,
                &key,
                &request,
                3,
                "Edited review",
                matching,
                &base,
                &key
            )
            .await,
            Err(NativeFailure::Refused(_))
        ));
        let (event, state) = wire(matching);
        let pr = json!({"state":"open","head":{"sha":commit},"base":{"sha":base}});
        let provider = Api(Mutex::new(vec![
            step("GET", "pulls/3", Value::Null, pr.clone()),
            step(
                "POST",
                "pulls/3/reviews",
                json!({"commit_id":commit,"body":body,"event":event}),
                json!({"id":9}),
            ),
            step(
                "GET",
                "pulls/3/reviews/9",
                Value::Null,
                json!({"id":9,"commit_id":commit,"state":state,"body":body,"html_url":"https://example.test/review/9"}),
            ),
            step("GET", "pulls/3", Value::Null, pr),
        ]));
        publish_approved(
            &provider, &bound, &key, &request, 3, &body, matching, &base, &key,
        )
        .await
        .unwrap();
        assert!(provider.0.lock().unwrap().is_empty());
        if let Some(previous) = first.replace(key.clone()) {
            assert!(matches!(
                publish_approved(
                    &empty, &bound, &previous, &request, 3, &body, matching, &base, &key
                )
                .await,
                Err(NativeFailure::Refused(_))
            ));
        }
    }
}

// Catches a review or publication whose reply was lost being written a second
// time by a retry or by reconciliation of the original effect.
#[tokio::test]
async fn review_publication_lost_reply_is_never_written_again() {
    use crate::workflows::run::check::git_output;
    use crate::workflows::run::native_host::NativeBinding;
    use crate::workflows::run::native_test_support::Fixture;
    use crate::workflows::run::neutral_effects::{EffectCommand, EffectObservation, EffectOutcome};
    use crate::workflows::run::neutral_runtime::NeutralHost;
    let mut evidence = None;
    for review in [false, true] {
        let f = Fixture::new();
        let path = f.context.workspace_path();
        let mut request = super::super::GitHubRequest {
            source_intent: None,
            account: "default".into(),
            repository: "owner/repo".into(),
            commit: git_output(path, &["rev-parse", "HEAD"]).unwrap(),
            tree: git_output(path, &["rev-parse", "HEAD^{tree}"]).unwrap(),
            action: super::super::GitHubAction::MarkReady { number: 3 },
        };
        if review {
            request.action = super::super::GitHubAction::Review {
                base: "b".repeat(40),
                review: evidence.clone().unwrap(),
                number: 3,
                body: "Review".into(),
                verdict: ReviewVerdict::Comment,
            };
        }
        let run = f.start(
            "publication.github",
            serde_json::to_value(&request).unwrap(),
        );
        let waiting = f.dispatch(&run).await;
        let approved = f.approve(&waiting);
        let key = approved.effects.values().next().unwrap().key.clone();
        evidence = Some(key.clone());
        f.owner
            .neutral_effect_command(&run.run.id, EffectCommand::Dispatch { key: key.clone() })
            .unwrap();
        let uncertain = f
            .owner
            .neutral_effect_command(
                &run.run.id,
                EffectCommand::Observe {
                    key: key.clone(),
                    observation: EffectObservation {
                        stopped: true,
                        outcome: EffectOutcome::Uncertain {
                            diagnostic: "connection lost after POST".into(),
                        },
                    },
                },
            )
            .unwrap();
        let bound = NativeBinding {
            context: f.context.clone(),
            store: f.owner.store.clone(),
        };
        // Refused before account resolution, so no provider request can follow.
        assert!(matches!(
            super::super::publish(&bound, &key, &request).await,
            Err(NativeFailure::Refused(_))
        ));
        let effect = uncertain.effect(&key).unwrap().clone();
        let reconciled = f.host.reconcile(effect).await.unwrap_err();
        assert!(reconciled.contains("no operation repeated"), "{reconciled}");
    }
}

// Catches graph inputs reusing an approval granted for a different publication action or PR.
#[tokio::test]
async fn review_publication_exact_approval_cannot_change_action_or_target() {
    use crate::agent_capabilities::Capability;
    use crate::workflows::run::check::git_output;
    use crate::workflows::run::native_test_support::Fixture;
    let f = Fixture::new();
    let path = f.context.workspace_path();
    let request = super::super::GitHubRequest {
        source_intent: None,
        account: "default".into(),
        repository: "owner/repo".into(),
        commit: git_output(path, &["rev-parse", "HEAD"]).unwrap(),
        tree: git_output(path, &["rev-parse", "HEAD^{tree}"]).unwrap(),
        action: super::super::GitHubAction::MarkReady { number: 3 },
    };
    let run = f.start(
        "publication.github",
        serde_json::to_value(&request).unwrap(),
    );
    let waiting = f.dispatch(&run).await;
    let approved = f.approve(&waiting);
    let key = approved.effects.values().next().unwrap().key.clone();
    f.owner
        .neutral_effect_command(
            &run.run.id,
            crate::workflows::run::neutral_effects::EffectCommand::Dispatch { key: key.clone() },
        )
        .unwrap();
    f.context
        .authorize(
            &f.owner.store,
            &key,
            "native.publication.github",
            &request,
            Some(Capability::Push),
        )
        .unwrap();
    let mut changed = request.clone();
    changed.action = super::super::GitHubAction::MarkReady { number: 4 };
    assert!(
        f.context
            .authorize(
                &f.owner.store,
                &key,
                "native.publication.github",
                &changed,
                Some(Capability::Push)
            )
            .is_err()
    );
    changed.action = super::super::GitHubAction::ClosePr { number: 3 };
    assert!(
        f.context
            .authorize(
                &f.owner.store,
                &key,
                "native.publication.github",
                &changed,
                Some(Capability::Push)
            )
            .is_err()
    );
}
