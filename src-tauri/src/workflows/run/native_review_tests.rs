use super::check::git_output;
use super::native_review::{ReviewRequest, require_review, review_with};
use super::native_test_support::Fixture;
use super::neutral_effects::{
    EffectCommand, EffectKey, EffectObservation, EffectOutcome, NeutralEffect,
};
use super::neutral_execution::NeutralResult;
use super::neutral_runtime::{NeutralHost, NeutralInvocation, admitted};
use crate::workflows::review_verdict::CodeVerdict;
use std::collections::BTreeMap;

// Only the external model answer is substituted. Native diff and persisted
// effect ownership are real, and this helper never fabricates a receipt body.
pub(super) async fn reviewed(f: &Fixture, verdict: CodeVerdict) -> EffectKey {
    let path = f.context.workspace_path();
    let request = ReviewRequest {
        head: None,
        base: git_output(path, &["rev-parse", "HEAD"]).unwrap(),
        tree: git_output(path, &["write-tree"]).unwrap(),
    };
    reviewed_request(f, request, verdict).await
}

pub(super) async fn reviewed_request(
    f: &Fixture,
    request: ReviewRequest,
    verdict: CodeVerdict,
) -> EffectKey {
    let run = f.start("review.change", serde_json::to_value(&request).unwrap());
    let (activity, attempt) = run.pending_invocations().into_iter().next().unwrap();
    let mut invocation = NeutralInvocation {
        fence: attempt.fence.clone(),
        publication: run.publication.clone(),
        descriptor: run.descriptor(activity).unwrap().clone(),
        instance: run.publication.definition.instances[activity].clone(),
        inputs: attempt.inputs.clone(),
        subject: attempt.subject.clone(),
    };
    admitted(f.host.as_ref(), &run, &mut invocation, None).unwrap();
    let effect =
        NeutralEffect::prepare(invocation.clone(), f.host.prepare(&invocation).unwrap()).unwrap();
    let key = effect.key.clone();
    f.owner
        .neutral_effect_command(
            &run.run.id,
            EffectCommand::Prepare {
                effect: Box::new(effect),
            },
        )
        .unwrap();
    f.owner
        .neutral_effect_command(&run.run.id, EffectCommand::Dispatch { key: key.clone() })
        .unwrap();
    let receipt = review_with(
        &f.context,
        &f.owner.store,
        &key,
        &request,
        |diff| async move {
            assert!(diff.contains("artifact.txt")); // Empty diffs must never call the reviewer.
            Ok((
                verdict,
                "Controlled reviewer answer".into(),
                Some("controlled-review-session".into()),
            ))
        },
    )
    .await
    .unwrap();
    f.owner
        .neutral_effect_command(
            &run.run.id,
            EffectCommand::Observe {
                key: key.clone(),
                observation: EffectObservation {
                    stopped: true,
                    outcome: EffectOutcome::Completed {
                        result: NeutralResult::Completed {
                            outcome: "ok".into(),
                            outputs: BTreeMap::from([(
                                "receipt".into(),
                                serde_json::to_value(receipt).unwrap(),
                            )]),
                        },
                    },
                },
            },
        )
        .unwrap();
    key
}

// Catches accepting findings, a made-up receipt reference, or reusing a review
// after the index changes even when the commit request pins the new tree.
#[tokio::test]
async fn native_review_receipt_covers_only_passing_exact_diff() {
    let f = Fixture::new();
    let path = f.context.workspace_path();
    let base = git_output(path, &["rev-parse", "HEAD"]).unwrap();
    std::fs::write(path.join("artifact.txt"), "reviewed change").unwrap();
    git_output(path, &["add", "."]).unwrap();
    let tree = git_output(path, &["write-tree"]).unwrap();
    let findings = reviewed(&f, CodeVerdict::Findings).await;
    assert!(require_review(&f.context, &f.owner.store, &findings, &base, &tree).is_err());
    let review = reviewed(&f, CodeVerdict::Pass).await;
    require_review(&f.context, &f.owner.store, &review, &base, &tree).unwrap();
    let mut forged = review.clone();
    forged.digest = "invented".into();
    assert!(require_review(&f.context, &f.owner.store, &forged, &base, &tree).is_err());
    std::fs::write(path.join("artifact.txt"), "different change").unwrap();
    git_output(path, &["add", "."]).unwrap();
    let changed_tree = git_output(path, &["write-tree"]).unwrap();
    assert!(require_review(&f.context, &f.owner.store, &review, &base, &changed_tree).is_err());
    let run = f.start("commit.create", serde_json::json!({"parent":base,"tree":changed_tree,"message":"must refuse","review":review}));
    let waiting = f.dispatch(&run).await;
    let approved = f.approve(&waiting);
    let refused = f.dispatch(&approved).await;
    assert!(matches!(
        refused.status,
        super::neutral_execution::NeutralStatus::Failed { .. }
    ));
    assert_eq!(git_output(path, &["rev-parse", "HEAD"]).unwrap(), base);
}

// Catches dropping findings/uncertain observations or accepting a receipt for
// another committed candidate, base, tree or caller.
#[tokio::test]
async fn native_committed_review_observation_retains_verdict_and_exact_artifact() {
    use super::native_review::observed_review;
    let f = Fixture::new();
    let path = f.context.workspace_path();
    let base = git_output(path, &["rev-parse", "HEAD"]).unwrap();
    std::fs::write(path.join("artifact.txt"), "candidate").unwrap();
    git_output(path, &["add", "."]).unwrap();
    git_output(path, &["commit", "-m", "candidate"]).unwrap();
    let head = git_output(path, &["rev-parse", "HEAD"]).unwrap();
    let tree = git_output(path, &["rev-parse", "HEAD^{tree}"]).unwrap();
    for verdict in [
        CodeVerdict::Findings,
        CodeVerdict::Uncertain,
        CodeVerdict::Pass,
    ] {
        let key = reviewed_request(
            &f,
            ReviewRequest {
                base: base.clone(),
                head: Some(head.clone()),
                tree: tree.clone(),
            },
            verdict,
        )
        .await;
        let receipt =
            observed_review(&f.context, &f.owner.store, &key, &base, &head, &tree).unwrap();
        assert_eq!(receipt.verdict, verdict);
        assert_eq!(receipt.head.as_deref(), Some(head.as_str()));
        assert_eq!(
            receipt.reviewer_session_id.as_deref(),
            Some("controlled-review-session")
        );
        assert!(observed_review(&f.context, &f.owner.store, &key, &head, &head, &tree).is_err());
        assert!(observed_review(&f.context, &f.owner.store, &key, &base, &head, &base).is_err());
        std::fs::write(path.join("artifact.txt"), "unreviewed").unwrap();
        assert!(observed_review(&f.context, &f.owner.store, &key, &base, &head, &tree).is_err());
        std::fs::write(path.join("artifact.txt"), "candidate").unwrap();
    }
}

// Catches granting approval to a no-op or launching a paid reviewer for no diff.
#[tokio::test]
async fn native_empty_review_is_uncertain_without_reviewer_session() {
    use super::native_review::observed_review;
    let f = Fixture::new();
    let path = f.context.workspace_path();
    let base = git_output(path, &["rev-parse", "HEAD"]).unwrap();
    let tree = git_output(path, &["write-tree"]).unwrap();
    let staged = reviewed(&f, CodeVerdict::Pass).await;
    assert!(require_review(&f.context, &f.owner.store, &staged, &base, &tree).is_err());
    let key = reviewed_request(
        &f,
        ReviewRequest {
            base: base.clone(),
            head: Some(base.clone()),
            tree: tree.clone(),
        },
        CodeVerdict::Pass,
    )
    .await;
    let receipt = observed_review(&f.context, &f.owner.store, &key, &base, &base, &tree).unwrap();
    assert_eq!(receipt.verdict, CodeVerdict::Uncertain);
    assert!(receipt.body.contains("Empty diff"));
    assert!(receipt.reviewer_session_id.is_none());
}
