//! Domain adapter tests use the real source/effect stores and external-owner ports.
use super::native_git::NativeFailure;
use super::native_host::NativeBinding;
use super::native_issue_effect::IssueAction;
use super::native_issue_read::IssueReadReceipt;
use super::native_issue_service::*;
use super::native_issue_tests::{api, approved, setup};
use super::native_issue_triage::{IssueTriageRequest, triage_with};
use super::native_test_support::Fixture;
use super::neutral_effects::EffectKey;
use crate::workflows::review_verdict::{TriageDecision, TriageOutcome};
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct Services {
    outcome: TriageOutcome,
    foreign: bool,
    calls: AtomicUsize,
}
impl IssueServices for Services {
    fn triage<'a>(
        &'a self,
        _: &'a super::native_host::NativeBinding,
        _: &'a EffectKey,
        issue: &'a IssueReadReceipt,
        profile: &'a IssueProfile,
    ) -> IssueFuture<'a, ScopedTriage> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            let mut source = issue.source.clone();
            if self.foreign {
                source.generation = "foreign".into();
            }
            Ok(ScopedTriage {
                source,
                profile: profile.clone(),
                decision: TriageDecision {
                    outcome: self.outcome,
                    reason: "Controlled scoped evaluation".into(),
                },
                report: "scoped-report".into(),
            })
        })
    }
    fn handoff<'a>(
        &'a self,
        _: &'a super::native_host::NativeBinding,
        _: &'a EffectKey,
        _: &'a IssueChildRequest,
    ) -> IssueFuture<'a, IssueChildReceipt> {
        Box::pin(async { Err("child owner not installed in triage scenario".into()) })
    }
    fn observe<'a>(&'a self, _: &'a EffectKey) -> IssueFuture<'a, IssueChildReceipt> {
        Box::pin(async { Err("no child in triage scenario".into()) })
    }
    fn cancel<'a>(&'a self, _: &'a EffectKey) -> IssueFuture<'a, IssueChildReceipt> {
        Box::pin(async { Err("no child in triage scenario".into()) })
    }
}
fn service(outcome: TriageOutcome, foreign: bool) -> Arc<Services> {
    Arc::new(Services {
        outcome,
        foreign,
        calls: AtomicUsize::new(0),
    })
}
async fn triage_run(f: &Fixture) -> (IssueTriageRequest, EffectKey, NativeBinding) {
    triage_run_with(
        f,
        IssueProfile {
            profile: "scoped-triage".into(),
            model: "exact-model".into(),
            effort: "medium".into(),
            instructions: "Evaluate issue requirements".into(),
        },
    )
    .await
}
async fn triage_run_with(
    f: &Fixture,
    profile: IssueProfile,
) -> (IssueTriageRequest, EffectKey, NativeBinding) {
    let (effect, _, bound) = setup(
        f,
        IssueAction::Comment {
            body: "unused".into(),
        },
    );
    let request = IssueTriageRequest {
        source: effect.source,
        profile,
        policy: IssuePolicy {
            accepted_label: "accepted".into(),
            rejection_label: "rejected".into(),
            needs_input_comment: true,
        },
    };
    let run = f.start("issue.triage", json!(request));
    let intent = bound
        .store
        .trigger_intent(&request.source.receipt)
        .unwrap()
        .unwrap();
    bound.store.trigger_started(&intent, &run.run.id).unwrap();
    let run = approved(f, &run).await;
    (
        request,
        run.effects.values().next().unwrap().key.clone(),
        bound,
    )
}
// Catches: losing needs-input/rejected dispositions or accepting a scoped
// report for another source generation as this issue's publishable verdict.
#[tokio::test]
async fn native_issue_triage_retains_all_dispositions_and_refuses_foreign_report() {
    for outcome in [
        TriageOutcome::Accepted,
        TriageOutcome::Rejected,
        TriageOutcome::NeedsInput,
    ] {
        let f = Fixture::new();
        let (request, key, bound) = triage_run(&f).await;
        let services = service(outcome, false);
        f.host.install_issue_services(services.clone());
        let receipt = triage_with(&bound, &key, &request, services.as_ref(), &api())
            .await
            .unwrap();
        assert_eq!(receipt.assessment.decision.outcome, outcome);
        assert_eq!(receipt.assessment.source, request.source);
        assert_eq!(receipt.effect, key);
        let foreign = service(outcome, true);
        assert!(matches!(
            triage_with(&bound, &key, &request, foreign.as_ref(), &api()).await,
            Err(NativeFailure::Refused(_))
        ));
        use super::neutral_effects::{EffectCommand, EffectObservation, EffectOutcome};
        f.owner
            .neutral_effect_command(
                &key.fence.run,
                EffectCommand::Observe {
                    key: key.clone(),
                    observation: EffectObservation {
                        stopped: true,
                        outcome: EffectOutcome::Completed {
                            result: super::neutral_execution::NeutralResult::Completed {
                                outcome: "ok".into(),
                                outputs: std::collections::BTreeMap::from([(
                                    "receipt".into(),
                                    json!(receipt),
                                )]),
                            },
                        },
                    },
                },
            )
            .unwrap();
        let action = match outcome {
            TriageOutcome::Accepted => IssueAction::AcceptedLabel {
                triage: key.clone(),
            },
            TriageOutcome::Rejected => IssueAction::RejectionComment {
                triage: key.clone(),
            },
            TriageOutcome::NeedsInput => IssueAction::QuestionComment {
                triage: key.clone(),
            },
        };
        let effect = super::native_issue_effect::IssueEffectRequest {
            source: request.source.clone(),
            action,
        };
        let resolved = super::native_issue_policy::resolve(&bound, &key, &effect).unwrap();
        match resolved {
            super::native_issue_policy::IssueWrite::AddLabel(label) => {
                assert_eq!(label, "accepted")
            }
            super::native_issue_policy::IssueWrite::Comment(body) => {
                assert_eq!(body, "Controlled scoped evaluation")
            }
        }
        let mut wrong = effect.clone();
        wrong.action = IssueAction::AcceptedLabel {
            triage: key.clone(),
        };
        if outcome != TriageOutcome::Accepted {
            assert!(
                super::native_issue_policy::resolve(&bound, &key, &wrong)
                    .unwrap_err()
                    .contains("disposition")
            );
        }
        if outcome != TriageOutcome::Accepted {
            let child = IssueChildRequest {
                source: request.source.clone(),
                triage: key.clone(),
                publication: f
                    .owner
                    .store
                    .neutral_execution(&key.fence.run)
                    .unwrap()
                    .run
                    .request
                    .publication,
                inputs: Default::default(),
            };
            assert!(
                matches!(super::native_issue_handoff::handoff_owned(&bound,&key,&child,services.as_ref()).await,Err(NativeFailure::Refused(message)) if message.contains("only accepted"))
            );
        }
    }
}
// Catches: the production NativeHost installing no IssueServices owner (every
// converted issue graph fails outside tests); triage leaving the one mediated
// model path or the pinned profile; a production handoff dispatching a child
// instead of refusing with the 022 conversion blocker.
#[tokio::test]
async fn native_issue_production_owner_triages_through_mediated_profile_and_refuses_child() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let f = Fixture::new();
    f.state.workflow_runtime.attach_owner(f.owner.clone());
    let (protocol, runtime) = super::neutral_model_adapter_tests::fixture(
        r#"{"outcome":"accepted","reason":"Requirements are clear","report":"scoped-report"}"#,
        false,
    );
    for entry in std::fs::read_dir(protocol.path()).unwrap() {
        let path = entry.unwrap().path();
        std::fs::copy(
            &path,
            f.context.workspace_path().join(path.file_name().unwrap()),
        )
        .unwrap();
    }
    {
        let mut config = f.state.config.write();
        config.ego_executable = runtime.executable.to_string_lossy().into_owned();
        config.ego_profile = runtime.profile.clone();
    }
    let profile = IssueProfile {
        profile: runtime.profile.clone(),
        model: "fixture-model".into(),
        effort: "high".into(),
        instructions: "Evaluate issue requirements".into(),
    };
    let (request, key, bound) = triage_run_with(&f, profile.clone()).await;
    let services = f
        .host
        .issue_services()
        .expect("production NativeHost installs an IssueServices owner");
    let receipt = triage_with(&bound, &key, &request, services.as_ref(), &api())
        .await
        .unwrap();
    assert_eq!(receipt.assessment.decision.outcome, TriageOutcome::Accepted);
    assert_eq!(receipt.assessment.decision.reason, "Requirements are clear");
    assert_eq!(receipt.assessment.report, "scoped-report");
    assert_eq!(receipt.assessment.source, request.source);
    assert_eq!(receipt.assessment.profile, profile);
    let run = f.owner.store.neutral_execution(&key.fence.run).unwrap();
    assert!(
        run.effect(&key).unwrap().model_session.is_some(),
        "triage must bind its mediated session to the triage effect"
    );
    // A profile other than the configured mediated runtime is refused, never substituted.
    let mut foreign = profile.clone();
    foreign.profile = "another-profile".into();
    let issue = IssueReadReceipt::new(request.source.clone(), api().issue);
    assert!(
        services
            .triage(&bound, &key, &issue, &foreign)
            .await
            .unwrap_err()
            .contains("exact isolated runtime/profile")
    );
    // H2: the production owner never starts a child; graphs compose 022 instead.
    let child = IssueChildRequest {
        source: request.source.clone(),
        triage: key.clone(),
        publication: run.run.request.publication.clone(),
        inputs: Default::default(),
    };
    assert!(matches!(
        super::native_issue_handoff::handoff_owned(&bound, &key, &child, services.as_ref()).await,
        Err(NativeFailure::Refused(message)) if message.contains("host.story.subflow")
    ));
    for result in [services.observe(&key).await, services.cancel(&key).await] {
        assert!(result.unwrap_err().contains("host.story.subflow"));
    }
}
// Catches: the presence of an intake label bypassing live requirements checks
// and allowing an edited issue to reach the evaluator or publication path.
#[tokio::test]
async fn native_issue_triage_checks_live_requirements_before_model_call() {
    let f = Fixture::new();
    let (request, key, bound) = triage_run(&f).await;
    let services = service(TriageOutcome::Accepted, false);
    let mut changed = api();
    changed.issue["title"] = json!("Changed requirements");
    assert!(matches!(
        triage_with(&bound, &key, &request, services.as_ref(), &changed).await,
        Err(NativeFailure::Refused(_))
    ));
    assert_eq!(services.calls.load(Ordering::SeqCst), 0);
}
