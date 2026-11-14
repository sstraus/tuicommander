//! Authorized sandbox only. No branch publication, model call or desktop launch.
use super::super as run;
use super::parity_support::*;
use crate::github_pr_api::{PrApi, RepoApi};
use crate::workflows::{WorkflowStore, catalog::*, github_trigger::GithubResource, trigger::*};
use futures_util::FutureExt;
use run::{
    native_test_support::Fixture, neutral_execution::NeutralStatus, neutral_runtime::NeutralHost,
};
use serde_json::{Value, json};

async fn finished(f: &Fixture, id: &str) -> run::neutral_execution::NeutralExecution {
    loop {
        let changed = f.owner.neutral_effect_updates.notified();
        tokio::pin!(changed);
        changed.as_mut().enable();
        let snapshot = f.owner.store.neutral_execution(id).unwrap();
        if snapshot.status != NeutralStatus::Running {
            return snapshot;
        }
        changed.await;
    }
}

// Catches: a real selectable issue event or owner-replayed PR never reaches the
// installed authenticated connector; a provider fixture cannot pass this test.
#[tokio::test]
#[ignore = "requires explicit sandbox authority and command-only GITHUB_E2E_TOKEN"]
async fn github_sandbox_issue_and_pr_sources_execute_installed_authenticated_connector() {
    let token = std::env::var("GITHUB_E2E_TOKEN").expect("authorized GITHUB_E2E_TOKEN required");
    assert_ne!(token, "");
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let f = Fixture::new();
    *f.state.github.token.write() = Some(token);
    let project = f.context.workspace_path().to_str().unwrap();
    run::check::git_output(
        f.context.workspace_path(),
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/sstraus/tuic-e2e-sandbox.git",
        ],
    )
    .unwrap();
    let api = RepoApi::resolve(&f.state, project).await.unwrap();
    assert_eq!(
        format!("{}/{}", api.owner, api.repo),
        "sstraus/tuic-e2e-sandbox"
    );
    let prs = api
        .rest("GET", "pulls?state=open&per_page=100", Value::Null)
        .await
        .unwrap();
    let pr = prs
        .as_array()
        .and_then(|a| a.first())
        .expect("sandbox requires one open PR for explicit retained replay");
    let pr_number = pr["number"].as_i64().unwrap();
    let pr_title = pr["title"].as_str().unwrap();
    let title = format!("WER parity 018 {}", uuid::Uuid::new_v4());
    let plugin = "workflow-github-live";
    let base = "https://api.github.com/repos/sstraus/tuic-e2e-sandbox/";
    let plugin_dir = config.path().join("plugins").join(plugin);
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::write(plugin_dir.join("manifest.json"), serde_json::to_vec(&json!({
        "id":plugin,"name":"Sandbox parity","version":"1.0.0","minAppVersion":"0.0.0","main":"main.js",
        "capabilities":["net:http"],"allowedUrls":[format!("{base}*")],
        "providers":["github"]
    })).unwrap()).unwrap();
    f.state
        .loaded_plugins
        .insert(plugin.into(), vec!["net:http".into()]);
    *f.owner.state.lock() = std::sync::Arc::downgrade(&f.state);
    *f.state.workflow_runtime.owner.lock() = Some(f.owner.clone());
    let store = WorkflowStore::open().unwrap();
    let mut http = run::native_family_tests::http();
    let mut contract = run::neutral_http_fixture::contract(false);
    contract.id = "github-sandbox-read".into();
    contract.authentication = Some(
        crate::workflows::data_reference::DataReference::Credential {
            id: api.account.id.clone(),
        },
    );
    http.target = Some(ReusableTarget::Http {
        connector: contract,
        plugin: plugin.into(),
        base: base.into(),
    });
    http.inputs = schema(json!({"number":field("integer")}));
    http.outputs = schema(json!({"number":field("integer"),"title":field("string")}));
    http.config.fields.get_mut("request").unwrap().default = Some(json!({
        "method":"GET","endpoint":format!("{base}issues"),"path":[{"source":"input","field":"number","max_bytes":32}],
        "query":{},"headers":{"Accept":{"source":"literal","value":"application/vnd.github+json"}},"body":null,
        "response":{"number":"/number","title":"/title"},"max_response_bytes":262144
    }));
    let catalog = save(&f, &store, http.clone());
    let mut definition = graph(&[http]);
    definition.input_bindings = serde_json::from_value(
        json!({"0":{"number":{"source":"workflow_input","field":"number"}}}),
    )
    .unwrap();
    definition.output_bindings = serde_json::from_value(json!({"number":{"source":"activity_output","activity":0,"field":"number"},"title":{"source":"activity_output","activity":0,"field":"title"}})).unwrap();
    let publication = publish(&f, &store, &catalog, "live-github-read", definition);
    for (kind, filter_title) in [("issue", title.as_str()), ("pull_request", pr_title)] {
        let binding = TriggerBindingDraft {
            id: format!("live-{kind}"),
            publication: crate::workflows::neutral_publication::PublicationRef {
                project: project.into(),
                id: publication.id.clone(),
                revision: publication.revision,
            },
            source: TriggerSource::Github,
            schedule: None,
            kind: kind.into(),
            resource_prefix: "github:sstraus/tuic-e2e-sandbox/".into(),
            events: ["opened".into()].into(),
            filters: [("title".into(), json!(filter_title))].into(),
            ignored_actors: Default::default(),
            mapping: TriggerMapping {
                inputs: [("number".into(), "number".into())].into(),
                subject: Default::default(),
            },
            limits: serde_json::from_value(
                json!({"max_activations":20,"max_concurrency":1,"max_duration_ms":90000}),
            )
            .unwrap(),
        };
        let registry = f.host.registry();
        let binding = store
            .save_neutral_trigger_binding(
                project,
                0,
                binding,
                &catalog,
                &registry.available_handlers(),
            )
            .unwrap();
        store
            .enable_neutral_trigger_binding(
                project,
                &binding.draft.id,
                binding.revision,
                true,
                &Default::default(),
                &catalog,
                &registry.available_handlers(),
            )
            .unwrap();
    }
    // Capture actual source baselines before creating the uniquely selected issue.
    for kind in [GithubResource::Issue, GithubResource::PullRequest] {
        crate::workflows::github_poll::observe(&f.state, project, kind)
            .await
            .unwrap();
    }
    let created = api.rest("POST","issues",json!({"title":title,"body":"Authorized workflow parity evidence; this issue is closed by its test."})).await.unwrap();
    let number = created["number"]
        .as_i64()
        .expect("created sandbox issue number");
    let scenario = async {
        // A list immediately after a write need not expose that resource. Read
        // the actual issue through the installed account-aware source adapter;
        // this proves snapshot admission, not eventual poll discovery latency.
        let snapshot = crate::workflows::github_event::read_snapshot(
            &api,
            "sstraus/tuic-e2e-sandbox",
            GithubResource::Issue,
            number,
        )
        .await
        .unwrap();
        let registry = f.host.registry();
        f.state
            .workflow_runtime
            .admit_github_batch(
                &store,
                &catalog,
                &registry.available_handlers(),
                project,
                "sstraus/tuic-e2e-sandbox",
                GithubResource::Issue,
                vec![snapshot],
            )
            .unwrap();
        let issue = f
            .owner
            .store
            .trigger_original(
                project,
                "live-issue",
                &format!("github:sstraus/tuic-e2e-sandbox/issue/{number}"),
                "1",
            )
            .unwrap();
        assert_eq!(issue.admission.disposition, "started");
        let issue_run = finished(&f, issue.admission.run_id.as_ref().unwrap()).await;
        assert_eq!(
            issue_run.status,
            NeutralStatus::Completed {
                outcome: "done".into()
            },
            "{issue_run:?}"
        );
        let outputs = &issue_run.outputs.as_ref().unwrap().values;
        assert_eq!(outputs["number"], json!(number));
        assert_eq!(outputs["title"], json!(title));
        // The actual open PR was baselined rather than auto-started. Explicit
        // owner replay selects that retained provider generation through the API.
        let action = crate::workflows::neutral_api::model::NeutralAction::ReplayTrigger {
            binding_id: "live-pull_request".into(),
            resource: format!("github:sstraus/tuic-e2e-sandbox/pull_request/{pr_number}"),
            generation: "1".into(),
            replay_id: "authorized-live-pr".into(),
        };
        let reply = crate::workflows::neutral_api::neutral_action(
            &f.state,
            project,
            action,
            Some(run::NativeCaller::Owner),
        )
        .unwrap();
        let reply = serde_json::to_value(reply).unwrap();
        let pr_id = reply["value"][0]["run_id"]
            .as_str()
            .expect("replayed PR run receipt");
        let pr_run = finished(&f, pr_id).await;
        assert_eq!(
            pr_run.status,
            NeutralStatus::Completed {
                outcome: "done".into()
            },
            "{pr_run:?}"
        );
        assert_eq!(
            pr_run.outputs.as_ref().unwrap().values["number"],
            json!(pr_number)
        );
        assert_eq!(
            pr_run.outputs.as_ref().unwrap().values["title"],
            json!(pr_title)
        );
        let artifact = json!({"repository":"sstraus/tuic-e2e-sandbox","issue":number,"issue_url":created["html_url"],"issue_admission":issue.admission,"issue_run":issue_run,"pr":pr_number,"pr_replay":reply,"pr_run":pr_run});
        if let Ok(path) = std::env::var("TUIC_PARITY_EVIDENCE") {
            std::fs::write(path, serde_json::to_vec_pretty(&artifact).unwrap()).unwrap();
        }
        println!(
            "GITHUB_LIVE_ISSUE={number} PR_REPLAY={pr_number} ISSUE_RUN={} PR_RUN={pr_id}",
            issue_run.run.id
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
    assert!(
        closed.as_ref().is_ok_and(|v| v["state"] == "closed"),
        "owned sandbox issue {number} cleanup failed"
    );
    println!("GITHUB_LIVE_CLEANED_ISSUE={number}");
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}
