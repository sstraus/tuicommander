use super::check::git_output;
use super::native_test_support::Fixture;
use super::neutral_execution::NeutralStatus;
use crate::agent_capabilities::CapabilitySet;
use serde_json::json;

// Catches committing before exact approval, accepting a changed index, or broadening a push.
#[tokio::test]
async fn native_commit_and_local_push_require_separate_exact_approvals() {
    let f = Fixture::new();
    let path = f.context.workspace_path();
    let parent = git_output(path, &["rev-parse", "HEAD"]).unwrap();
    std::fs::write(path.join("artifact.txt"), "approved change").unwrap();
    git_output(path, &["add", "artifact.txt"]).unwrap();
    let tree = git_output(path, &["write-tree"]).unwrap();
    let review = super::native_review_tests::reviewed(
        &f,
        crate::workflows::review_verdict::CodeVerdict::Pass,
    )
    .await;
    let run = f.start(
        "commit.create",
        json!({"parent":parent,"tree":tree,"message":"approved native commit","review":review}),
    );
    let waiting = f.dispatch(&run).await;
    assert!(!waiting.effects.values().next().unwrap().dispatched);
    assert_eq!(git_output(path, &["rev-parse", "HEAD"]).unwrap(), parent);
    let approved = f.approve(&waiting);
    let committed = f.dispatch(&approved).await;
    let effect = committed.effects.values().next().unwrap();
    assert!(effect.settled(), "{effect:?}");
    let commit = git_output(path, &["rev-parse", "HEAD"]).unwrap();
    assert_ne!(commit, parent);
    assert_eq!(
        git_output(path, &["rev-parse", "HEAD^{tree}"]).unwrap(),
        tree
    );
    let bare = f.dir.path().join("local.git");
    std::fs::create_dir(&bare).unwrap();
    git_output(&bare, &["init", "--bare"]).unwrap();
    let push = f.start(
        "publication.push",
        json!({"commit":commit,"tree":tree,
        "remote":bare.to_string_lossy(),"target":"refs/heads/approved"}),
    );
    let waiting = f.dispatch(&push).await;
    assert!(git_output(&bare, &["rev-parse", "refs/heads/approved"]).is_err());
    let approved = f.approve(&waiting);
    let pushed = f.dispatch(&approved).await;
    let effect = pushed.effects.values().next().unwrap();
    assert!(effect.settled(), "{effect:?}");
    assert_eq!(
        git_output(&bare, &["rev-parse", "refs/heads/approved"]).unwrap(),
        commit
    );
    // A later dispatch loses actual native push authority, despite its retained approval.
    let next = f.start(
        "publication.push",
        json!({"commit":commit,"tree":tree,
        "remote":bare.to_string_lossy(),"target":"refs/heads/revoked"}),
    );
    let waiting = f.dispatch(&next).await;
    let approved = f.approve(&waiting);
    f.state
        .agent_capabilities
        .insert("trusted".into(), CapabilitySet::default());
    let refused = f.dispatch(&approved).await;
    assert!(!refused.effects.values().next().unwrap().dispatched);
    assert!(git_output(&bare, &["rev-parse", "refs/heads/revoked"]).is_err());
}

// Catches exact-effect consent accidentally covering a later staged tree.
#[tokio::test]
async fn native_commit_refuses_index_changed_after_approval() {
    let f = Fixture::new();
    let path = f.context.workspace_path();
    let parent = git_output(path, &["rev-parse", "HEAD"]).unwrap();
    std::fs::write(path.join("artifact.txt"), "first").unwrap();
    git_output(path, &["add", "."]).unwrap();
    let tree = git_output(path, &["write-tree"]).unwrap();
    let review = super::native_review_tests::reviewed(
        &f,
        crate::workflows::review_verdict::CodeVerdict::Pass,
    )
    .await;
    let run = f.start(
        "commit.create",
        json!({"parent":parent,"tree":tree,"message":"exact","review":review}),
    );
    let waiting = f.dispatch(&run).await;
    let approved = f.approve(&waiting);
    std::fs::write(path.join("artifact.txt"), "later").unwrap();
    git_output(path, &["add", "."]).unwrap();
    let after = f.dispatch(&approved).await;
    assert!(matches!(after.status, NeutralStatus::Failed { .. }));
    assert_eq!(git_output(path, &["rev-parse", "HEAD"]).unwrap(), parent);
}

// Catches run-start coverage (coordinator decision b, 2026-11-12) widening to a
// commit whose tree differs from the reviewed tree, a failing review, a review from
// another run, or the canonical branch.
#[tokio::test]
async fn native_commit_asks_unless_the_run_start_covers_its_exact_reviewed_tree() {
    use super::neutral_runtime::NeutralHost;
    use crate::workflows::review_verdict::CodeVerdict;
    let f = Fixture::new();
    let path = f.context.workspace_path();
    git_output(path, &["branch", "main"]).unwrap();
    let parent = git_output(path, &["rev-parse", "HEAD"]).unwrap();
    std::fs::write(path.join("artifact.txt"), "reviewed").unwrap();
    git_output(path, &["add", "."]).unwrap();
    let tree = git_output(path, &["write-tree"]).unwrap();
    let review = super::native_review_tests::reviewed(&f, CodeVerdict::Pass).await;
    let asks = |tree: &str, review: &super::neutral_effects::EffectKey, own_run: bool| {
        let request = json!({"parent":parent,"tree":tree,"message":"covered","review":review});
        let run = f.start("commit.create", request);
        let (activity, attempt) = run.pending_invocations().into_iter().next().unwrap();
        let mut invocation = super::neutral_runtime::NeutralInvocation {
            fence: attempt.fence.clone(),
            publication: run.publication.clone(),
            descriptor: run.descriptor(activity).unwrap().clone(),
            instance: run.publication.definition.instances[activity].clone(),
            inputs: attempt.inputs.clone(),
            subject: attempt.subject.clone(),
        };
        if own_run {
            invocation.fence.run = review.fence.run.clone();
        }
        f.host.prepare(&invocation).unwrap().approval.is_some()
    };
    assert!(
        !asks(&tree, &review, true),
        "the exact reviewed tree is covered"
    );
    assert!(
        asks(&tree, &review, false),
        "a review from another run covers nothing"
    );
    std::fs::write(path.join("artifact.txt"), "changed after review").unwrap();
    git_output(path, &["add", "."]).unwrap();
    let other = git_output(path, &["write-tree"]).unwrap();
    assert!(asks(&other, &review, true), "a different tree still asks");
    assert!(
        asks(&tree, &review, true),
        "a stale reviewed tree still asks"
    );
    let failing = super::native_review_tests::reviewed(&f, CodeVerdict::Findings).await;
    assert!(asks(&other, &failing, true), "a failing review still asks");
    let passing = super::native_review_tests::reviewed(&f, CodeVerdict::Pass).await;
    assert!(!asks(&other, &passing, true));
    git_output(path, &["symbolic-ref", "HEAD", "refs/heads/main"]).unwrap();
    assert!(
        asks(&other, &passing, true),
        "the canonical branch still asks"
    );
}
