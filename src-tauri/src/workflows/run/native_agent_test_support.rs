use super::native_test_support::Fixture;
use super::neutral::{NeutralLimits, NeutralStartRequest};
use super::neutral_execution::NeutralExecution;
use super::neutral_execution_tests::step;
use super::neutral_runtime::NeutralHost;
use super::{NativeCaller, NativeContext};
use crate::acp::EgoAcpConfig;
use crate::workflows::WorkflowStore;
use crate::workflows::catalog::*;
use crate::workflows::data_binding::DataSource;
use crate::workflows::neutral_host::NeutralCatalogHost;
use crate::workflows::neutral_publication::PublicationRef;
use serde_json::json;
use std::collections::BTreeMap;

pub(super) fn role_run(f: &Fixture, role: &str, runtime: &EgoAcpConfig) -> NeutralExecution {
    role_run_with(f, role, runtime, NativeCaller::Owner, None)
}

pub(super) fn role_run_with(
    f: &Fixture,
    role: &str,
    runtime: &EgoAcpConfig,
    caller: NativeCaller,
    tools: Option<super::neutral_model_tools::ModelToolPolicy>,
) -> NeutralExecution {
    role_run_custom(f, role, runtime, caller, tools, |_| {})
}
pub(super) fn role_run_custom(
    f: &Fixture,
    role: &str,
    runtime: &EgoAcpConfig,
    caller: NativeCaller,
    tools: Option<super::neutral_model_tools::ModelToolPolicy>,
    customize: impl FnOnce(&mut crate::workflows::neutral_publication::NeutralDefinition),
) -> NeutralExecution {
    {
        let mut config = f.state.config.write();
        config.ego_executable = runtime.executable.to_string_lossy().into_owned();
        config.ego_profile = runtime.profile.clone();
    }
    let project = f.context.workspace_path().to_string_lossy().into_owned();
    let catalog = f.host.catalog(&project).unwrap();
    let handlers = f.host.registry().available_handlers();
    let descriptor = super::native_agent::descriptor(runtime).unwrap();
    let template = f.start("workspace.pin", json!({}));
    let mut definition = template.publication.definition;
    definition.inputs = descriptor.inputs.clone();
    definition.subject = descriptor.subject.clone();
    definition.outputs = descriptor.outputs.clone();
    definition.output_bindings = BTreeMap::from([(
        "report".into(),
        DataSource::ActivityOutput {
            activity: 0,
            field: "report".into(),
        },
    )]);
    definition.instances = vec![ActivityInstance {
        descriptor: descriptor.id.clone(),
        execution_policy: "local".into(),
        config: BTreeMap::from([
            ("role".into(), json!(role)),
            (
                "request".into(),
                json!(super::neutral_model_adapter_tests::request()),
            ),
        ]),
    }];
    if let Some(tools) = tools {
        definition.instances[0]
            .config
            .insert("tools".into(), json!(tools));
    }
    definition.input_bindings = BTreeMap::from([(
        0,
        BTreeMap::from([(
            "source".into(),
            DataSource::WorkflowInput {
                field: "source".into(),
            },
        )]),
    )]);
    definition
        .control
        .edges
        .get_mut("work")
        .unwrap()
        .remove("ok");
    definition
        .control
        .edges
        .get_mut("work")
        .unwrap()
        .insert("complete".into(), "done".into());
    customize(&mut definition);
    let workflows =
        WorkflowStore::open_at(&f.dir.path().join(format!("role-{role}.sqlite3"))).unwrap();
    let publication = workflows
        .save_neutral_publication(&project, "role", 0, definition, &catalog, &handlers)
        .unwrap();
    let (commit, tree) = super::check::clean_artifact(f.context.workspace_path()).unwrap();
    let run = f.owner.start_neutral(&workflows, &NeutralStartRequest {
        publication: PublicationRef { project: project.clone(), id: publication.id, revision: publication.revision },
        request_id: uuid::Uuid::new_v4().to_string(),
        inputs: BTreeMap::from([("source".into(), json!({"origin":"fixture-plan","generation":"generation-7","commit":commit,"tree":tree}))]),
        subject: Some(BTreeMap::from([("workspace".into(), json!({"reference":"workspace","id":project}))])),
        limits: NeutralLimits { max_activations: 20, max_concurrency: 1, max_duration_ms: 300_000 },
    }, &catalog, &handlers).unwrap();
    let context =
        NativeContext::capture(&f.state, caller, f.context.workspace_path(), vec![]).unwrap();
    f.host.bind_run(&f.owner.store, &run.id, context).unwrap();
    let run = f.owner.store.neutral_execution(&run.id).unwrap();
    let run = step(&f.owner, &run, "role-enter", super::store::now_ms());
    step(&f.owner, &run, "role-prepare", super::store::now_ms())
}

pub(super) fn invocation(run: &NeutralExecution) -> super::neutral_runtime::NeutralInvocation {
    let attempt = run.attempt.as_ref().unwrap();
    super::neutral_runtime::NeutralInvocation {
        fence: attempt.fence.clone(),
        publication: run.publication.clone(),
        descriptor: run.descriptor(0).unwrap().clone(),
        instance: {
            let mut instance = run.publication.definition.instances[0].clone();
            instance.config = run
                .descriptor(0)
                .unwrap()
                .config
                .resolve(&instance.config, "role config")
                .unwrap();
            instance
        },
        inputs: attempt.inputs.clone(),
        subject: attempt.subject.clone(),
    }
}
