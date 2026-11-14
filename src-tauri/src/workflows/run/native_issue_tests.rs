//! Native source/effect boundaries with the real Git/SQLite/effect owners.
use super::native_git::NativeFailure;
use super::native_host::NativeBinding;
use super::native_issue_effect::{IssueAction, IssueEffectRequest, publish_with};
use super::native_issue_source::IssueSource;
use super::native_test_support::Fixture;
use super::neutral_effects::EffectCommand;
use super::neutral_execution::NeutralExecution;
use crate::github_pr_api::PrApi;
use crate::workflows::github_trigger::GithubResource;
use serde_json::{Value, json};
use std::sync::Mutex;

pub(super) struct Api {
    pub issue: Value,
    writes: Mutex<Vec<(String, Value)>>,
    lost: bool,
}
impl PrApi for Api {
    fn graphql(&self, _: &str, _: Value) -> impl Future<Output = Result<Value, String>> {
        std::future::poll_fn(|_| unreachable!())
    }
    fn rest(
        &self,
        method: &str,
        endpoint: &str,
        body: Value,
    ) -> impl Future<Output = Result<Value, String>> {
        if method == "GET" {
            return std::future::ready(Ok(self.issue.clone()));
        }
        self.writes
            .lock()
            .unwrap()
            .push((endpoint.into(), body.clone()));
        if self.lost {
            return std::future::ready(Err("reply lost after provider accepted write".into()));
        }
        std::future::ready(if endpoint.ends_with("/comments") {
            Ok(json!({"id":1,"body":body["body"]}))
        } else {
            Ok(json!([{"name":"bug"},{"name":body["labels"][0]}]))
        })
    }
}
pub(super) fn api() -> Api {
    Api {
        issue: serde_json::from_str(include_str!("../fixtures/github-issue-rest.json")).unwrap(),
        writes: Mutex::new(vec![]),
        lost: false,
    }
}
pub(super) fn setup(
    f: &Fixture,
    action: IssueAction,
) -> (IssueEffectRequest, NeutralExecution, NativeBinding) {
    let (source, intent, bound) = admission(f);
    let request = IssueEffectRequest { source, action };
    let run = f.start("issue.effect", json!(request));
    f.owner.store.trigger_started(&intent, &run.run.id).unwrap();
    (request, run, bound)
}

/// A real pending admission, attached exactly once to the test's chosen run.
pub(super) fn admission(
    f: &Fixture,
) -> (
    IssueSource,
    super::neutral_trigger_store::TriggerIntent,
    NativeBinding,
) {
    admission_with_issue(f, &api().issue)
}
pub(super) fn admission_with_issue(
    f: &Fixture,
    issue: &Value,
) -> (
    IssueSource,
    super::neutral_trigger_store::TriggerIntent,
    NativeBinding,
) {
    let snapshot =
        crate::workflows::github_event::from_rest("octo-org/example", GithubResource::Issue, issue)
            .unwrap();
    let events = f
        .owner
        .store
        .normalize_github(snapshot.clone(), None)
        .unwrap();
    let event = events.iter().find(|event| event.event == "opened").unwrap();
    // Retain admission through the real source store, then link its admitted run.
    let placeholder = f.start("workspace.pin", json!({}));
    let intent = f
        .owner
        .store
        .reserve_trigger(
            f.context.workspace_path().to_str().unwrap(),
            "intake",
            event.clone(),
            placeholder.run.request.clone(),
            "",
        )
        .unwrap();
    let source = IssueSource {
        receipt: intent.receipt.clone(),
        account: "account".into(),
        repository: "octo-org/example".into(),
        number: 1347,
        generation: event.generation.clone(),
        material_digest: snapshot.material_digest,
    };
    let bound = NativeBinding {
        context: f.context.clone(),
        store: f.owner.store.clone(),
    };
    (source, intent, bound)
}
fn invocation(run: &NeutralExecution) -> super::neutral_runtime::NeutralInvocation {
    let (activity, attempt) = run.pending_invocations().into_iter().next().unwrap();
    super::neutral_runtime::NeutralInvocation {
        fence: attempt.fence.clone(),
        publication: run.publication.clone(),
        descriptor: run.descriptor(activity).unwrap().clone(),
        instance: run.publication.definition.instances[activity].clone(),
        inputs: attempt.inputs.clone(),
        subject: attempt.subject.clone(),
    }
}
pub(super) async fn approved(f: &Fixture, run: &NeutralExecution) -> NeutralExecution {
    // Reserve/approve/dispatch in the real ledger, without contacting a live API.
    use super::neutral_runtime::NeutralHost;
    let invocation = invocation(run);
    let preparation = f.host.prepare(&invocation).unwrap();
    let effect = super::neutral_effects::NeutralEffect::prepare(invocation, preparation).unwrap();
    let run = if run.effects.is_empty() {
        f.owner
            .neutral_effect_command(
                &run.run.id,
                EffectCommand::Prepare {
                    effect: Box::new(effect),
                },
            )
            .unwrap()
    } else {
        f.owner.store.neutral_execution(&run.run.id).unwrap()
    };
    let run = if run.effects.values().next().unwrap().question.is_some() {
        f.approve(&run)
    } else {
        run
    };
    let key = run.effects.values().next().unwrap().key.clone();
    f.owner
        .neutral_effect_command(&run.run.id, EffectCommand::Dispatch { key })
        .unwrap()
}
// Catches: trusting graph labels/source fields without this run's retained admission,
// or letting superseded/withdrawn generations and edited live requirements write.
#[tokio::test]
async fn native_issue_source_refuses_foreign_stale_and_edited_requirements() {
    let f = Fixture::new();
    let (request, run, bound) = setup(
        &f,
        IssueAction::Comment {
            body: "Reason".into(),
        },
    );
    let run = approved(&f, &run).await;
    let key = &run.effects.values().next().unwrap().key;
    let mut foreign = request.source.clone();
    foreign.number += 1;
    assert!(
        foreign
            .require(&bound, key)
            .unwrap_err()
            .contains("retained admission")
    );
    let mut changed = api();
    changed.issue["body"] = json!("New requirements");
    assert!(matches!(
        publish_with(&bound, key, &request, &changed).await,
        Err(NativeFailure::Refused(_))
    ));
    assert!(changed.writes.lock().unwrap().is_empty());
    let mut next = crate::workflows::github_event::from_rest(
        "octo-org/example",
        GithubResource::Issue,
        &changed.issue,
    )
    .unwrap();
    next.updated_at = "2026-11-11T11:00:00Z".into();
    f.owner.store.normalize_github(next, None).unwrap();
    assert!(
        request
            .source
            .require(&bound, key)
            .unwrap_err()
            .contains("superseded")
    );
}
// Catches: label replacement dropping existing labels; publishing before exact
// approval; treating a lost POST reply as success instead of uncertainty.
#[tokio::test]
async fn native_issue_effect_requires_approval_adds_label_and_preserves_lost_write() {
    let f = Fixture::new();
    let (request, run, bound) = setup(
        &f,
        IssueAction::AddLabel {
            label: "accepted".into(),
        },
    );
    use super::neutral_runtime::NeutralHost;
    let invocation = invocation(&run);
    let effect = super::neutral_effects::NeutralEffect::prepare(
        invocation.clone(),
        f.host.prepare(&invocation).unwrap(),
    )
    .unwrap();
    let key = effect.key.clone();
    f.owner
        .neutral_effect_command(
            &run.run.id,
            EffectCommand::Prepare {
                effect: Box::new(effect),
            },
        )
        .unwrap();
    assert!(matches!(
        publish_with(&bound, &key, &request, &api()).await,
        Err(NativeFailure::Refused(_))
    ));
    let run = approved(&f, &run).await;
    let key = &run.effects.values().next().unwrap().key;
    let provider = api();
    let receipt = publish_with(&bound, key, &request, &provider)
        .await
        .unwrap();
    assert_eq!(receipt.source, request.source);
    assert_eq!(
        *provider.writes.lock().unwrap(),
        vec![("issues/1347/labels".into(), json!({"labels":["accepted"]}))]
    );
    let f_lost = Fixture::new();
    let (request, run, bound) = setup(
        &f_lost,
        IssueAction::Comment {
            body: "Controlled question".into(),
        },
    );
    let run = approved(&f_lost, &run).await;
    let key = &run.effects.values().next().unwrap().key;
    let mut lost = api();
    lost.lost = true;
    let result = publish_with(&bound, key, &request, &lost).await;
    let diagnostic = match result {
        Err(NativeFailure::Uncertain(reason)) => reason,
        other => panic!("expected uncertainty: {other:?}"),
    };
    f_lost
        .owner
        .neutral_effect_command(
            &run.run.id,
            EffectCommand::Observe {
                key: key.clone(),
                observation: super::neutral_effects::EffectObservation {
                    stopped: true,
                    outcome: super::neutral_effects::EffectOutcome::Uncertain { diagnostic },
                },
            },
        )
        .unwrap();
    assert!(matches!(
        publish_with(&bound, key, &request, &lost).await,
        Err(NativeFailure::Refused(_))
    ));
    assert_eq!(
        lost.writes.lock().unwrap().len(),
        1,
        "uncertain write was repeated"
    );
}
// Catches: an issue effect approval that names neither the action nor its
// content (the generic "Run exact native.issue.effect operation?"), so the
// operator approves a comment text or a label without seeing which one.
#[test]
fn issue_effect_approval_names_the_action_and_its_content() {
    use super::neutral_runtime::NeutralHost;
    let title = |action: IssueAction| {
        let f = Fixture::new();
        let (_, run, _) = setup(&f, action);
        f.host
            .prepare(&invocation(&run))
            .unwrap()
            .approval
            .expect("an issue write needs an approval")
            .title
    };
    let comment = title(IssueAction::Comment {
        body: "Which version shows the missing row?".into(),
    });
    assert!(
        comment.contains("comment")
            && comment.contains("octo-org/example#1347")
            && comment.contains("Which version shows the missing row?"),
        "{comment}"
    );
    let label = title(IssueAction::AddLabel {
        label: "triaged".into(),
    });
    assert!(
        label.contains("label")
            && label.contains("\"triaged\"")
            && label.contains("octo-org/example#1347")
            && !label.contains("comment"),
        "{label}"
    );
}
/// Write the `accepted` label through `provider`, then poll the issue with that
/// label added. Returns whether the poll recorded the run's own self event.
async fn label_write_then_poll(provider: Api) -> bool {
    let f = Fixture::new();
    let (request, run, bound) = setup(
        &f,
        IssueAction::AddLabel {
            label: "accepted".into(),
        },
    );
    let run = approved(&f, &run).await;
    let key = &run.effects.values().next().unwrap().key;
    let lost = provider.lost;
    let result = publish_with(&bound, key, &request, &provider).await;
    assert_eq!(result.is_err(), lost, "{result:?}");
    let mut labeled = api().issue;
    labeled["labels"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"accepted"}));
    let mut next = crate::workflows::github_event::from_rest(
        "octo-org/example",
        GithubResource::Issue,
        &labeled,
    )
    .unwrap();
    next.updated_at = "2026-11-11T11:00:00Z".into();
    let events = f.owner.store.normalize_github(next, None).unwrap();
    let event = events.iter().find(|e| e.event == "labeled").unwrap();
    assert_eq!(event.generation, "2");
    f.owner
        .store
        .trigger_suppressed(
            f.context.workspace_path().to_str().unwrap(),
            "intake",
            event,
        )
        .unwrap()
}
// Catches: the production label write (publish_with) never claiming the next
// generation of its issue, so neutral_trigger_self_events stays empty in
// production and the poll admits the run's own label as a new event.
#[tokio::test]
async fn native_label_write_claims_its_own_next_generation() {
    assert!(
        label_write_then_poll(api()).await,
        "the run's own label write was not recorded as a self event"
    );
}
// Catches: a failed label write keeping its claim, so the same label added
// later by someone else is taken for the run's own write and starts no run;
// and the opposite bug, a reply lost after GitHub accepted the label releasing
// the claim, so the run's own label admits a second run.
#[tokio::test]
async fn failed_label_write_releases_its_claim_only_when_the_label_is_absent() {
    let mut refused = api();
    refused.lost = true;
    assert!(
        !label_write_then_poll(refused).await,
        "a label the run never wrote was suppressed as its own"
    );
    let mut accepted = api();
    accepted.lost = true;
    accepted.issue["labels"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"Accepted"}));
    assert!(
        label_write_then_poll(accepted).await,
        "a lost reply after an accepted label released the claim"
    );
}
