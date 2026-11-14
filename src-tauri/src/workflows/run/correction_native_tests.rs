use super::correction_native_fixture::{reviewed, start};
use super::native_test_support::Fixture;
use super::neutral_execution::{NeutralResult, NeutralStatus};
use crate::workflows::CheckDefinition;
use crate::workflows::review_verdict::CodeVerdict;

// Catches: a configured check which was omitted/not run being treated as passed,
// nonzero checks losing their semantic verdict, or review findings granting pass.
#[tokio::test]
async fn correction_native_runs_configured_checks_and_preserves_review_verdict() {
    for (fails, verdict, repeat) in [
        (false, CodeVerdict::Pass, Some(false)),
        (true, CodeVerdict::Pass, Some(true)),
        (false, CodeVerdict::Findings, Some(true)),
        (false, CodeVerdict::Uncertain, None),
    ] {
        let mut f = Fixture::new();
        if fails {
            f.host
                .install_project_checks(
                    f.context.workspace_path(),
                    vec![CheckDefinition {
                        id: "exact-tree".into(),
                        argv: vec![
                            "git".into(),
                            "rev-parse".into(),
                            "--verify".into(),
                            "missing-ref".into(),
                        ],
                        timeout_secs: 30,
                    }],
                )
                .unwrap();
            f.context = super::NativeContext::capture(
                &f.state,
                super::NativeCaller::Agent("trusted".into()),
                f.context.workspace_path(),
                vec!["exact-tree".into()],
            )
            .unwrap();
        }
        let run = start(&f);
        let run = reviewed(&f, run, verdict).await;
        let after = f.dispatch(&run).await;
        let attempt = after.attempt.as_ref().unwrap();
        let Some(NeutralResult::Completed { outcome, outputs }) = &attempt.result else {
            panic!("native assessment unavailable: {after:?}");
        };
        assert_eq!(outcome, "assessed");
        assert_eq!(outputs.get("repeat").and_then(|v| v.as_bool()), repeat);
        let evidence = outputs["state"]["evidence"].as_array().unwrap();
        assert_eq!(
            evidence.len(),
            2,
            "both review and configured check must execute"
        );
        assert!(
            evidence
                .iter()
                .any(|r| r["criterion"] == "check:exact-tree")
        );
    }
}

// Catches: a retained passing review accepting a replacement candidate before
// new review/checks, despite the original native effect being fully settled.
#[tokio::test]
async fn correction_native_refuses_retained_review_after_candidate_changes() {
    let f = Fixture::new();
    let run = start(&f);
    let run = reviewed(&f, run, CodeVerdict::Pass).await;
    let path = f.context.workspace_path();
    std::fs::write(path.join("artifact.txt"), "replacement").unwrap();
    super::check::git_output(path, &["add", "artifact.txt"]).unwrap();
    super::check::git_output(path, &["commit", "-qm", "replacement"]).unwrap();
    let after = f.dispatch(&run).await;
    assert!(
        matches!(after.status, NeutralStatus::Failed { .. }),
        "{after:?}"
    );
    assert!(!matches!(
        &after.attempt.as_ref().unwrap().result,
        Some(NeutralResult::Completed { .. })
    ));
}

// Catches: graph completion or caller-written review JSON substituting for a
// stopped observation from the actual independent review service.
#[tokio::test]
async fn correction_native_refuses_review_not_executed_by_service() {
    let f = Fixture::new();
    let run = start(&f);
    let run = f
        .owner
        .neutral_command(
            &run.run.id,
            "fake-review",
            run.sequence,
            super::neutral_execution::NeutralExecutionCommand::Complete {
                fence: run.attempt.as_ref().unwrap().fence.clone(),
                outcome: "ok".into(),
                outputs: std::collections::BTreeMap::from([(
                    "receipt".into(),
                    serde_json::json!({"verdict":"pass"}),
                )]),
            },
        )
        .unwrap();
    let at = super::store::now_ms();
    let run = super::neutral_execution_tests::step(&f.owner, &run, "to-assessment", at);
    let run = super::neutral_execution_tests::step(&f.owner, &run, "assessment-ready", at);
    let after = f.dispatch(&run).await;
    let NeutralStatus::Failed { diagnostic } = after.status else {
        panic!("unexecuted review accepted")
    };
    assert!(
        diagnostic.contains("current stopped independent review"),
        "{diagnostic}"
    );
}
