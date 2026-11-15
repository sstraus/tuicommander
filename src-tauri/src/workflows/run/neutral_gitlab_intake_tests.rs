use super::neutral_execution_tests::fixture_with;
use super::runtime::RuntimeOwner;
use crate::workflows::{WorkflowStore, catalog::WorkflowCatalog, gitlab::*, trigger::*};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

// Catches: a GitLab snapshot mapping the wrong subject, bypassing the shared
// installed schema, or a different delivery ID starting another run after restart.
#[test]
fn documented_mr_event_uses_shared_mapping_and_durable_generation_dedup() {
    let (dir, owner, run, _) = fixture_with(20, false, |_, _, _| {});
    let workflows = WorkflowStore::open_at(&dir.path().join("definitions.sqlite3")).unwrap();
    let mut catalog = WorkflowCatalog::new();
    let mut handlers = BTreeSet::new();
    for descriptor in &run.run.closure.descriptors {
        handlers.insert(descriptor.handler.clone());
        catalog.register(descriptor.clone()).unwrap();
    }
    let docs: Value = serde_json::from_str(include_str!(
        "../../../fixtures/gitlab-workflow/documented.json"
    ))
    .unwrap();
    let raw = &docs["merge_request_event"];
    let install = GitLabInstall::new(
        "https://gitlab.example.invalid/api/v4/".into(),
        raw["project"]["id"].as_u64().unwrap(),
        "account".into(),
    )
    .unwrap();
    let first = event::normalize(&install, "original-delivery", raw)
        .unwrap()
        .trigger()
        .unwrap();
    let draft = TriggerBindingDraft {
        id: "gitlab-mr-review".into(),
        publication: run.run.request.publication.clone(),
        source: TriggerSource::Gitlab,
        schedule: None,
        kind: "merge_request".into(),
        resource_prefix: format!("{}:mr:", install.connector.id),
        events: BTreeSet::from([first.event.clone()]),
        filters: BTreeMap::new(),
        ignored_actors: BTreeSet::new(),
        mapping: TriggerMapping {
            inputs: BTreeMap::from([("initial".into(), "mr".into())]),
            subject: BTreeMap::new(),
        },
        limits: run.run.request.limits,
    };
    let saved = workflows
        .save_neutral_trigger_binding("/project", 0, draft, &catalog, &handlers)
        .unwrap();
    let preview = workflows
        .preview_neutral_trigger_mapping("/project", &saved.draft, &first, &catalog, &handlers)
        .unwrap();
    assert_eq!(preview.inputs["initial"], raw["object_attributes"]["iid"]);
    workflows
        .enable_neutral_trigger_binding(
            "/project",
            &saved.draft.id,
            saved.revision,
            true,
            &preview.overlaps,
            &catalog,
            &handlers,
        )
        .unwrap();
    let admitted = owner
        .admit_trigger(&workflows, &catalog, &handlers, "/project", first.clone())
        .unwrap();
    assert_eq!(admitted.len(), 1);
    let original = admitted[0].run_id.clone().unwrap();
    assert_eq!(
        owner.store.neutral_run(&original).unwrap().inputs["initial"],
        json!(first.values["mr"])
    );
    drop(owner);
    let reopened = RuntimeOwner::acquire(&dir.path().join("runs.sqlite3")).unwrap();
    let redelivery = event::normalize(&install, "different-delivery", raw)
        .unwrap()
        .trigger()
        .unwrap();
    let duplicate = reopened
        .admit_trigger(&workflows, &catalog, &handlers, "/project", redelivery)
        .unwrap();
    assert_eq!(duplicate.len(), 1);
    assert_eq!(duplicate[0].run_id.as_ref(), Some(&original));
}
