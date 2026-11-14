use super::check::git_output;
use super::native_test_support::Fixture;
use super::neutral_effects::EffectOutcome;
use super::neutral_execution::NeutralResult;
use serde_json::json;

// Catches a real nonzero check being converted to a failed activity, losing
// the candidate-bound receipt needed to distinguish findings from infrastructure.
#[tokio::test]
async fn native_nonzero_check_retains_candidate_receipt() {
    let mut f = Fixture::new();
    let path = f.context.workspace_path().to_owned();
    std::fs::write(path.join("artifact.txt"), "candidate").unwrap();
    git_output(&path, &["add", "."]).unwrap();
    git_output(&path, &["commit", "-m", "candidate"]).unwrap();
    let commit = git_output(&path, &["rev-parse", "HEAD"]).unwrap();
    let tree = git_output(&path, &["write-tree"]).unwrap();
    f.host
        .install_project_checks(
            &path,
            vec![crate::workflows::CheckDefinition {
                id: "candidate-diff".into(),
                argv: ["git", "diff", "--exit-code", "HEAD~1", "HEAD"]
                    .map(str::to_owned)
                    .to_vec(),
                timeout_secs: 30,
            }],
        )
        .unwrap();
    f.context = super::NativeContext::capture(
        &f.state,
        super::NativeCaller::Owner,
        &path,
        vec!["candidate-diff".into()],
    )
    .unwrap();
    assert_eq!(f.context.verification_check_names(), vec!["candidate-diff"]);
    let run = f.start(
        "verification.run",
        json!({"check":"candidate-diff","commit":commit,"tree":tree}),
    );
    let observed = f.dispatch(&run).await;
    let effect = observed.effects.values().next().unwrap();
    assert!(effect.settled());
    let EffectOutcome::Completed {
        result: NeutralResult::Completed { outputs, .. },
    } = &effect.observation.as_ref().unwrap().outcome
    else {
        panic!("missing real CheckReceipt")
    };
    let receipt = &outputs["receipt"];
    assert_eq!(receipt["exitCode"], 1);
    assert_eq!(receipt["commit"], commit);
    assert_eq!(receipt["tree"], tree);
    let direct = f
        .context
        .verify_observed(
            "candidate-diff",
            &commit,
            &tree,
            f.owner.store.database_path(),
            &run.run.id,
        )
        .unwrap();
    assert_eq!(direct.exit_code, 1);
    assert!(
        f.context
            .verify_observed(
                "candidate-diff",
                &commit,
                &commit,
                f.owner.store.database_path(),
                &run.run.id
            )
            .is_err()
    );
}
