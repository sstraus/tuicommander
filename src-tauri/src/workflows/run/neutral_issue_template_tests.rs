use super::native_test_support::Fixture;
use super::neutral_execution::NeutralStatus;
use crate::workflows::{
    WorkflowStore,
    catalog::{HandlerRef, WorkflowCatalog},
    github_event::from_rest,
    github_trigger::GithubResource,
    neutral_api::{templates::templates, validation},
    neutral_publication::PublicationRef,
    trigger::*,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// The native operations plus the model request a configured ego installs.
fn catalog() -> (WorkflowCatalog, BTreeSet<HandlerRef>) {
    let mut catalog = crate::workflows::native_operation_catalog().unwrap();
    let runtime = crate::acp::EgoAcpConfig {
        executable: std::env::current_exe().unwrap(),
        profile: "exact-profile".into(),
        master_prompt: String::new(),
        ai_chat: false,
    };
    catalog
        .register(super::native_model::descriptor(&runtime).unwrap())
        .unwrap();
    let handlers = catalog.descriptors().map(|d| d.handler.clone()).collect();
    (catalog, handlers)
}

// Catches: the triage model seeing only the issue body, so it judges an issue
// without its title and labels; and an issue with no body (GitHub sends null)
// leaving the model input unknown, so the run fails before the model runs.
#[test]
fn triage_model_input_carries_the_issue_title_and_labels() {
    use crate::workflows::data_binding::{DataState, Datum, resolve_bindings};
    let definition = templates()
        .unwrap()
        .into_iter()
        .find(|t| t.key == "github-issue-triage")
        .expect("built-in issue triage template")
        .definition;
    let (catalog, _) = catalog();
    let model = catalog
        .descriptors()
        .find(|d| d.handler.name == "host.model")
        .unwrap()
        .clone();
    let text = |issue: Value| {
        let source = super::native_issue_source::IssueSource {
            receipt: "a".repeat(64),
            account: "account".into(),
            repository: "octo-org/example".into(),
            number: 1347,
            generation: "1".into(),
            material_digest: "digest".into(),
        };
        let receipt = json!(super::native_issue_read::IssueReadReceipt::new(
            source, issue
        ));
        let read = BTreeMap::from([(
            "receipt".to_owned(),
            Datum {
                state: DataState::Present(receipt),
                provenance: BTreeSet::new(),
            },
        )]);
        resolve_bindings(
            &definition.input_bindings[&1],
            &BTreeMap::new(),
            &BTreeMap::from([(0, read)]),
            &model.inputs,
            &BTreeMap::new(),
        )
        .unwrap()
        .values["text"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let mut issue: Value =
        serde_json::from_str(include_str!("../fixtures/github-issue-rest.json")).unwrap();
    issue["labels"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"needs-repro"}));
    let full = text(issue.clone());
    for expected in [
        "Found a bug",
        "\"bug\"",
        "needs-repro",
        "I am having a problem.",
    ] {
        assert!(full.contains(expected), "{expected} missing from {full}");
    }
    issue["body"] = Value::Null;
    assert!(text(issue).contains("Found a bug"));
}

// Catches: the built-in issue template drifting from the catalog (unknown
// descriptor, port or binding), so the Designer cannot publish it; a trigger
// binding that cannot supply the workspace subject every native issue operation
// needs, so the template never binds to a repository; one issue generation
// (opened with the label: two matching events) starting a second triage run,
// also on every later poll; and the run's own triaged label superseding it.
#[test]
fn issue_template_binds_per_repository_and_label_and_starts_one_run_per_generation() {
    let f = Fixture::new();
    let project = f.context.workspace_path().to_str().unwrap().to_owned();
    let (catalog, handlers) = catalog();
    let template = templates()
        .unwrap()
        .into_iter()
        .find(|t| t.key == "github-issue-triage")
        .expect("built-in issue triage template");
    let checked = validation::validate(&template.definition, &catalog, &handlers);
    assert!(checked.valid, "{:?}", checked.diagnostics);
    let workflows = WorkflowStore::open_at(&f.dir.path().join("definitions.sqlite3")).unwrap();
    let publication = workflows
        .save_neutral_publication(
            &project,
            "issue-triage",
            0,
            template.definition,
            &catalog,
            &handlers,
        )
        .unwrap();
    let draft = TriggerBindingDraft {
        id: "issue-label".into(),
        publication: PublicationRef {
            project: project.clone(),
            id: publication.id.clone(),
            revision: publication.revision,
        },
        source: TriggerSource::Github,
        schedule: None,
        kind: "issue".into(),
        resource_prefix: "github:octo-org/example/".into(),
        events: ["opened".into(), "labeled".into()].into(),
        filters: BTreeMap::from([("labels".into(), json!("triage"))]),
        ignored_actors: BTreeSet::new(),
        mapping: TriggerMapping {
            inputs: BTreeMap::new(),
            subject: BTreeMap::new(),
        },
        limits: serde_json::from_value(
            json!({"max_activations":20,"max_concurrency":1,"max_duration_ms":600000}),
        )
        .unwrap(),
    };
    let saved = workflows
        .save_neutral_trigger_binding(&project, 0, draft, &catalog, &handlers)
        .unwrap();
    workflows
        .enable_neutral_trigger_binding(
            &project,
            &saved.draft.id,
            saved.revision,
            true,
            &BTreeMap::new(),
            &catalog,
            &handlers,
        )
        .unwrap();
    let poll = |issues: Vec<&Value>| {
        f.owner
            .admit_github_batch(
                &workflows,
                &catalog,
                &handlers,
                &project,
                "octo-org/example",
                GithubResource::Issue,
                issues
                    .into_iter()
                    .map(|raw| from_rest("octo-org/example", GithubResource::Issue, raw).unwrap())
                    .collect(),
            )
            .unwrap()
            .into_iter()
            .filter_map(|admission| admission.run_id)
            .collect::<BTreeSet<_>>()
    };
    assert!(poll(Vec::new()).is_empty(), "baseline poll started a run");
    let mut raw: Value =
        serde_json::from_str(include_str!("../fixtures/github-issue-rest.json")).unwrap();
    raw["labels"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"Triage"}));
    let first = poll(vec![&raw]);
    assert_eq!(first.len(), 1, "one issue generation started {first:?}");
    assert_eq!(
        poll(vec![&raw]),
        first,
        "a repeated poll started another run"
    );
    let run = first.iter().next().unwrap();
    assert_eq!(
        f.owner.store.neutral_run(run).unwrap().subject,
        Some(BTreeMap::from([(
            "workspace".into(),
            json!({"reference":"workspace","id":project}),
        )])),
        "the binding did not give the run its project workspace"
    );

    // The template's own label write: claimed before the POST, then polled.
    let resource = "github:octo-org/example/issue/1347";
    let intent = f
        .owner
        .store
        .trigger_original(&project, "issue-label", resource, "1")
        .unwrap();
    f.owner
        .store
        .record_self_label(&intent.receipt, "triaged", "effect")
        .unwrap();
    raw["labels"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"triaged"}));
    raw["updated_at"] = json!("2026-11-11T10:00:01Z");
    let again = poll(vec![&raw]);
    assert!(
        again.is_subset(&first),
        "the run's own triaged label started a second triage: {again:?}"
    );
    assert_eq!(
        f.owner.store.neutral_execution(run).unwrap().status,
        NeutralStatus::Running,
        "the run's own label write superseded it"
    );
}
