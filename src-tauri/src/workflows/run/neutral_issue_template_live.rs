//! Authorized sandbox only (sstraus/tuic-e2e-sandbox, command-only
//! GITHUB_E2E_TOKEN). The issue template without its model step: no model call.
use super::native_test_support::Fixture;
use super::neutral_execution::{NeutralExecution, NeutralStatus};
use super::neutral_runtime::NeutralHost;
use crate::github_pr_api::{PrApi, RepoApi};
use crate::workflows::neutral_api::model::NeutralAction;
use crate::workflows::{
    WorkflowStore,
    catalog::{HandlerRef, WorkflowCatalog},
    github_trigger::GithubResource,
    neutral_admission::NeutralAdmissionService,
    neutral_host::NeutralCatalogHost,
    neutral_publication::PublicationRef,
    trigger::*,
};
use futures_util::FutureExt;
use serde_json::{Value, json};
use std::collections::BTreeSet;

const REPO: &str = "sstraus/tuic-e2e-sandbox";

/// The next state that needs the test: an open approval or a finished run.
async fn settle(f: &Fixture, id: &str) -> NeutralExecution {
    let wait = async {
        loop {
            let changed = f.owner.neutral_effect_updates.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let run = f.owner.store.neutral_execution(id).unwrap();
            if run.status != NeutralStatus::Running
                || run
                    .effects
                    .values()
                    .any(|e| e.question.is_some() && e.answer.is_none())
            {
                return run;
            }
            changed.await;
        }
    };
    tokio::time::timeout(std::time::Duration::from_secs(120), wait)
        .await
        .expect("live issue run made no progress in 120 s")
}

/// One real issue read through the account-aware source adapter, admitted
/// through the production batch. Returns every run the admissions name.
async fn poll(
    f: &Fixture,
    api: &RepoApi<'_>,
    store: &WorkflowStore,
    catalog: &WorkflowCatalog,
    handlers: &BTreeSet<HandlerRef>,
    project: &str,
    number: i64,
) -> BTreeSet<String> {
    // A list right after a write need not show the issue; read it directly.
    let snapshot =
        crate::workflows::github_event::read_snapshot(api, REPO, GithubResource::Issue, number)
            .await
            .unwrap();
    f.state
        .workflow_runtime
        .admit_github_batch(
            store,
            catalog,
            handlers,
            project,
            REPO,
            GithubResource::Issue,
            vec![snapshot],
        )
        .unwrap()
        .into_iter()
        .filter_map(|admission| admission.run_id)
        .collect()
}

/// The issue template with the model and condition replaced by a fixed comment.
fn definition(
    marker: &str,
    done: &str,
) -> crate::workflows::neutral_publication::NeutralDefinition {
    let receipt = |pointer: &str| json!({"source":"projection","from":{"source":"activity_output","activity":0,"field":"receipt"},"pointer":pointer,"value_type":"object"});
    let effect = |action: Value| json!({"request":{"source":"object","fields":{"source":receipt("/source"),"action":{"source":"object","fields":action}}}});
    let native = |name: &str| json!({"descriptor":{"namespace":"native","name":name,"version":1},"config":{},"execution_policy":"native"});
    serde_json::from_value(json!({
        "control":{
            "nodes":{"start":{"type":"start"},"read":{"type":"activity","activity":0},
                "comment":{"type":"activity","activity":1},"mark":{"type":"activity","activity":2},
                "commented":{"type":"end","outcome":"commented"}},
            "edges":{"start":{"next":"read"},"read":{"ok":"comment"},"comment":{"ok":"mark"},"mark":{"ok":"commented"}}
        },
        "outputs":{"fields":{}},"outcomes":["commented"],"inputs":{"fields":{}},
        "subject":{"required":true,"fields":{"fields":{"workspace":{"value_type":"workspace_reference","required":true,"default":null}}}},
        "instances":[native("issue.read"),native("issue.effect"),native("issue.effect")],
        "input_bindings":{
            "0":{"request":{"source":"constant","value":{}}},
            "1":effect(json!({"type":{"source":"constant","value":"comment"},"body":{"source":"constant","value":format!("Triage {marker}")}})),
            "2":effect(json!({"type":{"source":"constant","value":"add_label"},"label":{"source":"constant","value":done}}))
        },
        "conditions":{},"collections":{},"loops":{},"forks":{},"output_bindings":{},"subflows":[]
    }))
    .unwrap()
}

// Catches: a real labelled issue not reaching the neutral run through the
// production poll; the run's own label write (or a repeated poll) starting a
// second triage; and the run posting zero or two triage comments.
#[tokio::test]
#[ignore = "requires explicit sandbox authority and command-only GITHUB_E2E_TOKEN"]
async fn sandbox_labelled_issue_gets_one_triage_comment_and_no_second_run() {
    let token = std::env::var("GITHUB_E2E_TOKEN").expect("authorized GITHUB_E2E_TOKEN required");
    assert_ne!(token, "");
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let f = Fixture::new();
    *f.state.github.token.write() = Some(token);
    let project = f.context.workspace_path().to_str().unwrap().to_owned();
    super::check::git_output(
        f.context.workspace_path(),
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/sstraus/tuic-e2e-sandbox.git",
        ],
    )
    .unwrap();
    *f.owner.state.lock() = std::sync::Arc::downgrade(&f.state);
    f.state.workflow_runtime.attach_owner(f.owner.clone());
    let api = RepoApi::resolve(&f.state, &project).await.unwrap();
    assert_eq!(format!("{}/{}", api.owner, api.repo), REPO);
    let marker = format!("wer3036-{}", &uuid::Uuid::new_v4().to_string()[..8]);
    let done = format!("{marker}-done");
    for name in [&marker, &done] {
        let label = api
            .rest("POST", "labels", json!({"name":name,"color":"ededed","description":"Temporary issue workflow acceptance"}))
            .await
            .unwrap();
        assert_eq!(label["name"], json!(name));
    }
    let store = WorkflowStore::open().unwrap();
    let registry = f.host.registry();
    let handlers = registry.available_handlers();
    let installed = f.host.catalog(&project).unwrap();
    let catalog = store
        .build_project_catalog(&project, &installed, &registry)
        .unwrap();
    let context = f.host.publication_context(&project).unwrap();
    let publication = NeutralAdmissionService {
        store: &store,
        catalog: &catalog,
        registry: &registry,
    }
    .publish(
        Some(context.as_ref()),
        &project,
        "issue-triage-live",
        0,
        definition(&marker, &done),
    )
    .unwrap();
    let binding = store
        .save_neutral_trigger_binding(
            &project,
            0,
            TriggerBindingDraft {
                id: "live-issue-label".into(),
                publication: PublicationRef {
                    project: project.clone(),
                    id: publication.id.clone(),
                    revision: publication.revision,
                },
                source: TriggerSource::Github,
                schedule: None,
                kind: "issue".into(),
                resource_prefix: format!("github:{REPO}/"),
                events: ["opened".into(), "labeled".into()].into(),
                filters: [("labels".into(), json!(marker))].into(),
                ignored_actors: Default::default(),
                mapping: TriggerMapping {
                    inputs: Default::default(),
                    subject: Default::default(),
                },
                limits: serde_json::from_value(
                    json!({"max_activations":20,"max_concurrency":1,"max_duration_ms":300000}),
                )
                .unwrap(),
            },
            &catalog,
            &handlers,
        )
        .unwrap();
    store
        .enable_neutral_trigger_binding(
            &project,
            &binding.draft.id,
            binding.revision,
            true,
            &Default::default(),
            &catalog,
            &handlers,
        )
        .unwrap();
    // The real poll records the baseline before the issue exists.
    crate::workflows::github_poll::observe(&f.state, &project, GithubResource::Issue)
        .await
        .unwrap();
    let created = api
        .rest("POST", "issues", json!({"title":marker,"body":"Temporary issue workflow acceptance; closed by its test.","labels":[marker]}))
        .await
        .unwrap();
    let number = created["number"]
        .as_i64()
        .expect("created sandbox issue number");
    let scenario = async {
        let admit = || poll(&f, &api, &store, &catalog, &handlers, &project, number);
        let runs = admit().await;
        assert_eq!(runs.len(), 1, "the labelled issue started {runs:?}");
        let id = runs.iter().next().unwrap().clone();
        let mut approvals = 0;
        let run = loop {
            let run = settle(&f, &id).await;
            let Some(effect) = run
                .effects
                .values()
                .find(|e| e.question.is_some() && e.answer.is_none())
            else {
                break run;
            };
            approvals += 1;
            crate::workflows::neutral_api::neutral_action(
                &f.state,
                &project,
                NeutralAction::Answer {
                    run_id: id.clone(),
                    command_id: format!("live-approval-{approvals}"),
                    expected_sequence: run.sequence,
                    key: effect.key.clone(),
                    revision: effect.question.as_ref().unwrap().revision,
                    approved: true,
                },
                Some(super::NativeCaller::Owner),
            )
            .unwrap();
        };
        assert_eq!(
            run.status,
            NeutralStatus::Completed {
                outcome: "commented".into()
            },
            "{run:?}"
        );
        assert_eq!(
            approvals, 2,
            "the comment and the label each need one approval"
        );
        // The poll that sees the run's own label: a self event, no second run.
        let again = admit().await;
        assert!(
            again.is_subset(&runs),
            "the run's own label started a second triage: {again:?}"
        );
        let resource = format!("github:{REPO}/issue/{number}");
        let own = f
            .owner
            .store
            .retained_github_events(REPO, GithubResource::Issue, false)
            .unwrap()
            .into_iter()
            .find(|e| e.resource == resource && e.values.get("label") == Some(&json!(done)))
            .expect("the poll saw the run's own label");
        assert!(
            f.owner
                .store
                .trigger_suppressed(&project, "live-issue-label", &own)
                .unwrap(),
            "the run's own label was not recorded as a self event"
        );
        let comments = api
            .rest("GET", &format!("issues/{number}/comments"), Value::Null)
            .await
            .unwrap();
        let triage: Vec<_> = comments
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["body"].as_str().is_some_and(|b| b.contains(&marker)))
            .collect();
        assert_eq!(triage.len(), 1, "expected one triage comment: {comments}");
        println!(
            "GITHUB_LIVE_ISSUE={number} URL={} RUN={id} SELF_EVENT_GENERATION={} COMMENT={}",
            created["html_url"], own.generation, triage[0]["html_url"]
        );
    };
    let result = std::panic::AssertUnwindSafe(scenario).catch_unwind().await;
    let closed = api
        .rest(
            "PATCH",
            &format!("issues/{number}"),
            json!({"state":"closed"}),
        )
        .await;
    for name in [&marker, &done] {
        let _ = api
            .rest("DELETE", &format!("labels/{name}"), Value::Null)
            .await;
    }
    assert!(
        closed.as_ref().is_ok_and(|v| v["state"] == "closed"),
        "owned sandbox issue {number} cleanup failed"
    );
    println!("GITHUB_LIVE_CLEANED_ISSUE={number}");
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}
