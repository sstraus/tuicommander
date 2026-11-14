use super::super as run;
use crate::workflows::WorkflowStore;
use crate::workflows::catalog::*;
use crate::workflows::neutral_admission::NeutralAdmissionService;
use crate::workflows::neutral_host::NeutralCatalogHost;
use crate::workflows::neutral_publication::{
    NeutralDefinition, NeutralPublication, PublicationPin, PublicationRef,
};
use run::native_test_support::Fixture;
use run::neutral_execution::NeutralExecution;
use run::neutral_runtime::NeutralHost;
use serde_json::{Value, json};

pub(super) fn schema(fields: Value) -> ObjectSchema {
    serde_json::from_value(json!({"fields":fields})).unwrap()
}

pub(super) fn field(kind: &str) -> Value {
    json!({"value_type":kind,"required":true,"default":null})
}

pub(super) fn graph(descriptors: &[ActivityDescriptor]) -> NeutralDefinition {
    serde_json::from_value(json!({
        "control":{"nodes":{"start":{"type":"start"},"work":{"type":"activity","activity":0},
            "end":{"type":"end","outcome":"done"}},"edges":{"start":{"next":"work"},"work":{(descriptors[0].outcomes.first().unwrap()):"end"}}},
        "inputs":descriptors[0].inputs,"subject":descriptors[0].subject,"outputs":descriptors.last().unwrap().outputs,"outcomes":["done"],
        "instances":descriptors.iter().map(|d| json!({"descriptor":d.id,"config":{},"execution_policy":d.execution_policies.first().unwrap()})).collect::<Vec<_>>(),
        "input_bindings":{},"output_bindings":{},"conditions":{},"collections":{},"loops":{},"forks":{},"subflows":[]
    })).unwrap()
}

pub(super) fn script(
    name: &str,
    inputs: ObjectSchema,
    outputs: ObjectSchema,
    body: &str,
) -> ActivityDescriptor {
    let descriptor: ActivityDescriptor = serde_json::from_value(json!({
        "id":{"namespace":"acceptance","name":name,"version":1},"purpose":"Execute the parity acceptance transform",
        "handler":{"name":"host.script","version":1},"target":null,"config":{"fields":{}},
        "inputs":inputs,"outputs":outputs,"subject":null,"outcomes":["ok"],"required_capabilities":[],"execution_policies":["local"]
    })).unwrap();
    let policy = serde_json::from_value(json!({"version":1,"timeout_secs":10,"input_bytes":65536,
        "stdout_bytes":16384,"stderr_bytes":16384,"artifact_limit":4,"resources":[],"environment_refs":{},
        "secret_refs":{},"mutation":false,"retry":"never","reconciliation":{"type":"read_only"},"output":{"type":"structured"}})).unwrap();
    run::neutral_script::script_descriptor(descriptor, crate::workflows::command::Invocation::Shell {
        program: if cfg!(windows) {"python"} else {"python3"}.into(),
        script: format!("import json,sys\nq=json.load(sys.stdin)\n{body}\nprint(json.dumps(dict(version=1,identity=q['identity'],outcome='ok',data=data,artifacts={{}})))"),
        version:1,
    }, &policy).unwrap()
}

pub(super) fn save(
    f: &Fixture,
    store: &WorkflowStore,
    descriptor: ActivityDescriptor,
) -> WorkflowCatalog {
    let project = f.context.workspace_path().to_str().unwrap();
    let installed = f.host.catalog(project).unwrap();
    let registry = f.host.registry();
    let current = store
        .build_project_catalog(project, &installed, &registry)
        .unwrap();
    store
        .save_reusable_operation(project, 0, descriptor, &current, &registry)
        .unwrap();
    store
        .build_project_catalog(project, &installed, &registry)
        .unwrap()
}

pub(super) fn publish(
    f: &Fixture,
    store: &WorkflowStore,
    catalog: &WorkflowCatalog,
    name: &str,
    definition: NeutralDefinition,
) -> NeutralPublication {
    let project = f.context.workspace_path().to_str().unwrap();
    let registry = f.host.registry();
    let context = f.host.publication_context(project).unwrap();
    NeutralAdmissionService {
        store,
        catalog,
        registry: &registry,
    }
    .publish(Some(context.as_ref()), project, name, 0, definition)
    .unwrap()
}

pub(super) fn subflow(child: &NeutralPublication, name: &str) -> ActivityDescriptor {
    let mut descriptor = script(
        name,
        child.definition.inputs.clone(),
        child.definition.outputs.clone(),
        "data={}",
    );
    descriptor.subject = child.definition.subject.clone();
    descriptor.config = ObjectSchema::default();
    descriptor.handler = HandlerRef {
        name: "host.subflow".into(),
        version: 1,
    };
    descriptor.outcomes = child.definition.outcomes.clone();
    descriptor.target = Some(ReusableTarget::Subflow {
        publication: PublicationPin {
            reference: PublicationRef {
                project: child.project.clone(),
                id: child.id.clone(),
                revision: child.revision,
            },
            sha256: content_digest(child).unwrap(),
        },
    });
    descriptor
}

pub(super) async fn run(
    f: &Fixture,
    publication: &NeutralPublication,
    inputs: Value,
    subject: Option<Value>,
) -> NeutralExecution {
    *f.state.workflow_runtime.owner.lock() = Some(f.owner.clone());
    let request = serde_json::from_value(json!({"publication":{"project":publication.project,"id":publication.id,"revision":publication.revision},
        "request_id":publication.id,"inputs":inputs,"subject":subject,
        "limits":{"max_activations":100,"max_concurrency":2,"max_duration_ms":60000}})).unwrap();
    let reply = crate::workflows::neutral_api::neutral_action(
        &f.state,
        f.context.workspace_path().to_str().unwrap(),
        crate::workflows::neutral_api::model::NeutralAction::Start { request },
        Some(run::NativeCaller::Owner),
    )
    .unwrap();
    let reply = serde_json::to_value(reply).unwrap();
    let run_id = reply["value"]["snapshot"]["run"]["id"].as_str().unwrap();
    // Public Start wakes the single production actor. Subscribe before each
    // read so completion cannot be lost between the snapshot and the wait.
    loop {
        let changed = f.owner.neutral_effect_updates.notified();
        tokio::pin!(changed);
        changed.as_mut().enable();
        let snapshot = f.owner.store.neutral_execution(run_id).unwrap();
        if snapshot.status != run::neutral_execution::NeutralStatus::Running {
            return snapshot;
        }
        changed.await;
    }
}
