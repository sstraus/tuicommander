use super::native_cohort::{Cohort, StoryPin};
use super::native_test_support::Fixture;
use super::neutral::{NeutralLimits, NeutralStartRequest};
use super::neutral_execution::NeutralExecution;
use super::neutral_runtime::NeutralHost;
use super::{NativeCaller, NativeContext};
use crate::stories::{NewPlan, NewStory, StoryCommand, StoryOrigin};
use crate::workflows::WorkflowStore;
use crate::workflows::catalog::*;
use crate::workflows::data_binding::DataSource;
use crate::workflows::neutral_api::{
    model::{NeutralAction, NeutralReply},
    neutral_action,
};
use crate::workflows::neutral_host::NeutralCatalogHost;
use crate::workflows::neutral_publication::{PublicationPin, PublicationRef};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn cohort(f: &Fixture, completed: usize) -> Cohort {
    let store = crate::stories::story_backend().unwrap();
    let plan = store
        .create_plan(NewPlan {
            project: f.context.workspace_path().to_string_lossy().into_owned(),
            title: "Cohort".into(),
            source: "source-generation-1".into(),
        })
        .unwrap();
    let stories = (0..2)
        .map(|n| {
            let mut story = store
                .create_story_with_problem(
                    NewStory {
                        plan_id: plan.id.clone(),
                        title: format!("Member {n}"),
                        criteria: vec!["Observed completion".into()],
                        priority: 2,
                        story_type: None,
                        origin: StoryOrigin::Native,
                        file_scope: vec![],
                    },
                    None,
                )
                .unwrap();
            if n < completed {
                for command in [
                    StoryCommand::StartManual,
                    StoryCommand::CheckCriterion(0),
                    StoryCommand::SubmitReview,
                    StoryCommand::Approve,
                ] {
                    story = store
                        .transition_for_actor(&story.id, story.revision, command, None)
                        .unwrap();
                }
            }
            StoryPin {
                id: story.id,
                revision: story.revision,
            }
        })
        .collect();
    let plan = store.get_plan(&plan.id).unwrap();
    Cohort {
        plan: plan.id,
        plan_revision: plan.revision,
        source: plan.source,
        stories,
    }
}

pub(super) fn child(
    f: &Fixture,
    cohort: &Cohort,
) -> (WorkflowStore, WorkflowCatalog, PublicationPin) {
    let workflows = WorkflowStore::open().unwrap();
    let catalog = f
        .host
        .catalog(f.context.workspace_path().to_str().unwrap())
        .unwrap();
    let handlers = f.host.registry().available_handlers();
    let native = catalog
        .get(&DescriptorId {
            namespace: "native".into(),
            name: "story.cohort.observe".into(),
            version: 1,
        })
        .unwrap();
    let template = f.start("workspace.pin", json!({}));
    let mut definition = template.publication.definition;
    definition.conditions.clear();
    definition.collections.clear();
    definition.forks.clear();
    definition.loops.clear();
    definition.inputs.fields = BTreeMap::from([(
        "cohort".into(),
        FieldSchema {
            value_type: ValueType::Object,
            required: true,
            default: None,
        },
    )]);
    definition.outputs = native.outputs.clone();
    definition.output_bindings = BTreeMap::from([(
        "receipt".into(),
        DataSource::ActivityOutput {
            activity: 0,
            field: "receipt".into(),
        },
    )]);
    definition.outcomes = BTreeSet::from(["completed".into(), "blocked".into()]);
    definition.instances = vec![ActivityInstance {
        descriptor: native.id.clone(),
        config: BTreeMap::new(),
        execution_policy: "native".into(),
    }];
    definition.input_bindings = BTreeMap::from([(
        0,
        BTreeMap::from([(
            "request".into(),
            DataSource::WorkflowInput {
                field: "cohort".into(),
            },
        )]),
    )]);
    definition.control.nodes.remove("done");
    for outcome in ["completed", "blocked"] {
        definition.control.nodes.insert(
            outcome.into(),
            crate::workflows::neutral_control::ControlNode::End {
                outcome: outcome.into(),
            },
        );
    }
    definition.control.edges.insert(
        "work".into(),
        BTreeMap::from([
            ("completed".into(), "completed".into()),
            ("blocked".into(), "blocked".into()),
        ]),
    );
    let publication = workflows
        .save_neutral_publication(
            f.context.workspace_path().to_str().unwrap(),
            &format!("cohort-{}", cohort.plan),
            0,
            definition,
            &catalog,
            &handlers,
        )
        .unwrap();
    let pin = PublicationPin {
        reference: PublicationRef {
            project: publication.project.clone(),
            id: publication.id.clone(),
            revision: publication.revision,
        },
        sha256: content_digest(&publication).unwrap(),
    };
    (workflows, catalog, pin)
}

pub(super) fn parent(
    f: &Fixture,
    cohort: &Cohort,
    workflows: &WorkflowStore,
    pin: PublicationPin,
) -> NeutralExecution {
    let project = f.context.workspace_path().to_str().unwrap();
    let reply = neutral_action(
        &f.state,
        project,
        NeutralAction::SaveChildDispatch {
            cohort: Some(cohort.clone()),
            expected_version: 0,
            id: DescriptorId {
                namespace: "user".into(),
                name: "cohort-child".into(),
                version: 1,
            },
            publication: pin,
        },
        Some(NativeCaller::Owner),
    )
    .unwrap();
    let NeutralReply::Operation(descriptor) = reply else {
        panic!("operation reply")
    };
    let installed = f.host.catalog(project).unwrap();
    let registry = f.host.registry();
    let catalog = workflows
        .build_project_catalog(project, &installed, &registry)
        .unwrap();
    let handlers = registry.available_handlers();
    let (_, _, template, _) = super::neutral_execution_tests::fixture(20, false);
    let mut definition = template.publication.definition;
    definition.conditions.clear();
    definition.collections.clear();
    definition.forks.clear();
    definition.loops.clear();
    definition.inputs = descriptor.inputs.clone();
    definition.subject = descriptor.subject.clone();
    definition.outputs = descriptor.outputs.clone();
    definition.outcomes = descriptor.outcomes.clone();
    definition.instances = vec![ActivityInstance {
        descriptor: descriptor.id.clone(),
        config: BTreeMap::new(),
        execution_policy: "local".into(),
    }];
    definition.input_bindings = BTreeMap::from([(
        0,
        BTreeMap::from([(
            "cohort".into(),
            DataSource::WorkflowInput {
                field: "cohort".into(),
            },
        )]),
    )]);
    definition.output_bindings = BTreeMap::from([(
        "receipt".into(),
        DataSource::ActivityOutput {
            activity: 0,
            field: "receipt".into(),
        },
    )]);
    definition.control = serde_json::from_value(json!({"nodes":{"start":{"type":"start"},"work":{"type":"activity","activity":0},"completed":{"type":"end","outcome":"completed"},"blocked":{"type":"end","outcome":"blocked"}},"edges":{"start":{"next":"work"},"work":{"completed":"completed","blocked":"blocked"}}})).unwrap();
    let publication = workflows
        .save_neutral_publication(project, "cohort-parent", 0, definition, &catalog, &handlers)
        .unwrap();
    let request = NeutralStartRequest {
        publication: PublicationRef {
            project: project.into(),
            id: publication.id,
            revision: publication.revision,
        },
        request_id: "original-parent".into(),
        inputs: BTreeMap::from([("cohort".into(), json!(cohort))]),
        subject: Some(BTreeMap::from([(
            "workspace".into(),
            json!({"reference":"workspace","id":project}),
        )])),
        limits: NeutralLimits {
            max_activations: 30,
            max_concurrency: 1,
            max_duration_ms: 300_000,
        },
    };
    let run = f
        .owner
        .start_neutral(workflows, &request, &catalog, &handlers)
        .unwrap();
    let duplicate = f
        .owner
        .start_neutral(workflows, &request, &catalog, &handlers)
        .unwrap();
    assert_eq!(run.id, duplicate.id);
    let context = NativeContext::capture(
        &f.state,
        NativeCaller::Owner,
        f.context.workspace_path(),
        vec![],
    )
    .unwrap();
    f.host.bind_run(&f.owner.store, &run.id, context).unwrap();
    f.owner.store.neutral_execution(&run.id).unwrap()
}
