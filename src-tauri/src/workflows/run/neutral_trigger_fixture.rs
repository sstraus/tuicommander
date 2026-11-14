use super::neutral_execution_tests::fixture_with;
use super::runtime::RuntimeOwner;
use crate::workflows::{WorkflowStore, catalog::*, trigger::*};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub(super) struct Fixture {
    pub _dir: tempfile::TempDir,
    pub owner: Arc<RuntimeOwner>,
    pub workflows: WorkflowStore,
    pub catalog: WorkflowCatalog,
    pub handlers: BTreeSet<HandlerRef>,
    pub draft: TriggerBindingDraft,
}
pub(super) fn fixture() -> Fixture {
    let (dir, owner, run, _) = fixture_with(20, false, |definition, _, _| {
        definition.inputs.fields.insert(
            "head".into(),
            FieldSchema {
                value_type: ValueType::String,
                required: false,
                default: None,
            },
        );
    });
    let workflows = WorkflowStore::open_at(&dir.path().join("definitions.sqlite3")).unwrap();
    let mut catalog = WorkflowCatalog::new();
    let mut handlers = BTreeSet::new();
    for descriptor in &run.run.closure.descriptors {
        handlers.insert(descriptor.handler.clone());
        catalog.register(descriptor.clone()).unwrap();
    }
    let draft = TriggerBindingDraft {
        id: "binding-a".into(),
        publication: run.run.request.publication.clone(),
        source: TriggerSource::Github,
        schedule: None,
        kind: "issue".into(),
        resource_prefix: "github:octo-org/example/".into(),
        events: [
            "opened".into(),
            "reopened".into(),
            "updated".into(),
            "labeled".into(),
        ]
        .into(),
        filters: BTreeMap::new(),
        ignored_actors: BTreeSet::new(),
        mapping: TriggerMapping {
            inputs: BTreeMap::from([("initial".into(), "number".into())]),
            subject: BTreeMap::new(),
        },
        limits: run.run.request.limits,
    };
    Fixture {
        _dir: dir,
        owner,
        workflows,
        catalog,
        handlers,
        draft,
    }
}
pub(super) fn event() -> TriggerEvent {
    serde_json::from_str(include_str!("../fixtures/github-issue-opened.json")).unwrap()
}
impl Fixture {
    pub fn enable(&self, draft: TriggerBindingDraft, expected: i64) -> NeutralTriggerBinding {
        let binding = self.enable_without_baseline(draft, expected);
        self.complete_empty_poll(&binding);
        binding
    }
    pub fn complete_empty_poll(&self, binding: &NeutralTriggerBinding) {
        // Domain admission tests begin after a complete empty source cycle.
        self.owner
            .admit_github_batch(
                &self.workflows,
                &self.catalog,
                &self.handlers,
                "/project",
                "octo-org/example",
                if binding.draft.kind == "issue" {
                    crate::workflows::github_trigger::GithubResource::Issue
                } else {
                    crate::workflows::github_trigger::GithubResource::PullRequest
                },
                Vec::new(),
            )
            .unwrap();
    }
    pub fn enable_without_baseline(
        &self,
        draft: TriggerBindingDraft,
        expected: i64,
    ) -> NeutralTriggerBinding {
        let saved = self
            .workflows
            .save_neutral_trigger_binding(
                "/project",
                expected,
                draft,
                &self.catalog,
                &self.handlers,
            )
            .unwrap();
        let overlaps = self
            .workflows
            .preview_neutral_trigger_mapping(
                "/project",
                &saved.draft,
                &self.compatible_event(&saved.draft),
                &self.catalog,
                &self.handlers,
            )
            .unwrap()
            .overlaps;
        self.workflows
            .enable_neutral_trigger_binding(
                "/project",
                &saved.draft.id,
                saved.revision,
                true,
                &overlaps,
                &self.catalog,
                &self.handlers,
            )
            .unwrap()
    }
    fn compatible_event(&self, draft: &TriggerBindingDraft) -> TriggerEvent {
        let mut event = event();
        if draft.kind == "pull_request" {
            event = pr_event("1", 'a');
        }
        event
    }
    pub fn admit(&self, event: TriggerEvent) -> Vec<TriggerAdmission> {
        self.owner
            .admit_trigger(
                &self.workflows,
                &self.catalog,
                &self.handlers,
                "/project",
                event,
            )
            .unwrap()
    }
}
pub(super) fn pr_event(generation: &str, head: char) -> TriggerEvent {
    let mut event = event();
    event.kind = "pull_request".into();
    event.resource = "github:octo-org/example/pull_request/1347".into();
    event.schema = trigger_schema(TriggerSource::Github, "pull_request").unwrap();
    event.values.insert(
        "head".into(),
        serde_json::json!(head.to_string().repeat(40)),
    );
    event
        .values
        .insert("base".into(), serde_json::json!("f".repeat(40)));
    event.generation = generation.into();
    event
}

/// An enabled GitHub issue binding for `repository` in the active config
/// folder's workflow store. Hold the returned folder for the whole test.
pub(crate) fn install_github_issue_binding(repository: &str) -> tempfile::TempDir {
    let f = fixture();
    let mut draft = f.draft.clone();
    draft.resource_prefix = format!("github:{repository}/");
    f.enable_without_baseline(draft, 0);
    let target =
        rusqlite::Connection::open(crate::config::config_dir().join("workflows.sqlite3")).unwrap();
    target
        .execute(
            "ATTACH DATABASE ?1 AS fixture",
            [f._dir.path().join("definitions.sqlite3").to_str().unwrap()],
        )
        .unwrap();
    target
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS neutral_trigger_bindings AS
                 SELECT * FROM fixture.neutral_trigger_bindings WHERE 0;
             INSERT INTO neutral_trigger_bindings SELECT * FROM fixture.neutral_trigger_bindings;
             DETACH DATABASE fixture;",
        )
        .unwrap();
    f._dir
}
