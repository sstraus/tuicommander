//! Pilot scenarios. Each builder returns a fresh `Rig`; the neutral definition is
//! authored with the parity helpers and the external ports are scripted doubles.
use super::host::Calls;
use super::rig::Rig;
use super::run::native_host::NativeHost;
use super::run::native_test_support::Fixture;
use super::run::neutral_execution::{NeutralExecution, NeutralStatus};
use super::run::neutral_runtime::NeutralHost;
use super::runtime::RuntimeOwner;
use super::{parity_support::*, run};
use crate::workflows::{
    WorkflowStore, catalog::*, neutral_admission::NeutralAdmissionService,
    neutral_host::NeutralCatalogHost, neutral_publication::PublicationRef, scheduled_trigger,
    trigger::*,
};
use serde_json::{Value, json};
use std::sync::Arc;

pub(super) struct Scenario {
    pub name: &'static str,
    pub build: fn() -> Rig,
    /// Oracle for the fault-free run; fault runs are compared with that run.
    pub expect: fn(&NeutralExecution),
    /// Run-specific values (commit ids, clocks) removed so fault runs compare with the golden run.
    pub canon: fn(&NeutralExecution) -> Value,
    /// Commit points where a restart is known to stall (each has an ignored RED test).
    pub known_stalls: &'static [usize],
}

pub(super) const SCHEDULED_TEXT: &str = "scheduled literal $(never execute)";

/// Acceptance A: scheduled text -> model (ACP fixture) -> condition -> exact approval.
pub(super) fn a_scheduled_text() -> Scenario {
    Scenario {
        name: "A.scheduled-text-model-condition-approval",
        build: build_a,
        known_stalls: &[],
        canon: |run| json!(run.outputs.as_ref().map(|o| &o.values)),
        expect: |run| {
            assert_eq!(
                run.status,
                NeutralStatus::Completed {
                    outcome: "done".into()
                },
                "{run:?}"
            );
            assert_eq!(
                run.outputs.as_ref().unwrap().values["approved"],
                json!(true)
            );
        },
    }
}

fn build_a() -> Rig {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let guard = crate::config::set_config_dir_override(config.path().into());
    let (project_dir, runtime) = run::neutral_model_adapter_tests::fixture("{\"next\":7}", false);
    let project = project_dir
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let mut request = run::neutral_model_adapter_tests::request();
    request.context.push(SCHEDULED_TEXT.into());
    // The ACP protocol double must see the exact literal prompt; it supplies model output only.
    let scenario_path = project_dir.path().join("scenario.jsonl");
    let scenario = std::fs::read_to_string(&scenario_path)
        .unwrap()
        .lines()
        .map(|line| {
            let mut step: Value = serde_json::from_str(line).unwrap();
            if step["method"] == "session/prompt" && step["step"] == "expect" {
                step["params"]["prompt"] =
                    json!([{"type":"text","text":request.prompt().unwrap()}]);
            }
            step.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(scenario_path, scenario).unwrap();
    request.context.pop();
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    {
        let mut settings = state.config.write();
        settings.ego_executable = runtime.executable.to_string_lossy().into_owned();
        settings.ego_profile = runtime.profile.clone();
    }
    let host = NativeHost::install(&state).unwrap();
    let db = config.path().join("runs.sqlite3");
    let owner = RuntimeOwner::acquire(&db).unwrap();
    let store = WorkflowStore::open().unwrap();
    let catalog = host.catalog(&project).unwrap();
    let get = |namespace: &str, name: &str| {
        catalog
            .get(&DescriptorId {
                namespace: namespace.into(),
                name: name.into(),
                version: 1,
            })
            .unwrap()
            .clone()
    };
    let mut definition = graph(&[get("model", "request"), get("inbox", "approval")]);
    definition.inputs = schema(json!({"text":field("string")}));
    definition.outputs = schema(json!({"approved":field("boolean")}));
    definition.instances[0]
        .config
        .insert("request".into(), serde_json::to_value(request).unwrap());
    definition.outcomes = ["done".into(), "blocked".into()].into();
    definition.control = serde_json::from_value(json!({"nodes":{"start":{"type":"start"},"work":{"type":"activity","activity":0},"check":{"type":"condition","condition":"verdict"},"approve":{"type":"activity","activity":1},"end":{"type":"end","outcome":"done"},"blocked":{"type":"end","outcome":"blocked"}},"edges":{"start":{"next":"work"},"work":{"complete":"check"},"check":{"true":"approve","false":"blocked","unknown":"blocked"},"approve":{"complete":"end"}}})).unwrap();
    definition.conditions = serde_json::from_value(json!({"verdict":{"operator":"compare","comparison":"equal","left":{"source":"activity_output","activity":0,"field":"result"},"right":{"source":"constant","value":{"next":7}}}})).unwrap();
    definition.input_bindings = serde_json::from_value(json!({"0":{"text":{"source":"workflow_input","field":"text"}},"1":{"text":{"source":"workflow_input","field":"text"}}})).unwrap();
    definition.output_bindings = serde_json::from_value(
        json!({"approved":{"source":"activity_output","activity":1,"field":"approved"}}),
    )
    .unwrap();
    let registry = host.registry();
    let publication = NeutralAdmissionService {
        store: &store,
        catalog: &catalog,
        registry: &registry,
    }
    .publish(
        Some(host.publication_context(&project).unwrap().as_ref()),
        &project,
        "scheduled-text",
        0,
        definition,
    )
    .unwrap();
    let draft = TriggerBindingDraft {
        id: "daily-text".into(),
        publication: PublicationRef {
            project: project.clone(),
            id: publication.id,
            revision: publication.revision,
        },
        source: TriggerSource::Scheduled,
        kind: "schedule".into(),
        resource_prefix: "schedule:daily-text".into(),
        schedule: Some(scheduled_trigger::TriggerSchedule {
            cron: "* * * * *".into(),
            timezone: "UTC".into(),
            text: SCHEDULED_TEXT.into(),
        }),
        events: ["due".into()].into(),
        filters: Default::default(),
        ignored_actors: Default::default(),
        mapping: TriggerMapping {
            inputs: [("text".into(), "text".into())].into(),
            subject: Default::default(),
        },
        limits: serde_json::from_value(
            json!({"max_activations":20,"max_concurrency":1,"max_duration_ms":90000}),
        )
        .unwrap(),
    };
    let binding = store
        .save_neutral_trigger_binding(&project, 0, draft, &catalog, &registry.available_handlers())
        .unwrap();
    store
        .enable_neutral_trigger_binding(
            &project,
            &binding.draft.id,
            binding.revision,
            true,
            &Default::default(),
            &catalog,
            &registry.available_handlers(),
        )
        .unwrap();
    let now = chrono::Utc::now();
    let start: super::rig::StartFn = Box::new(move |rig| {
        // The same occurrence ticked again returns the same run: a durable receipt.
        // DEFECT (2026-11-15): the first tick on a freshly installed host reads the
        // registry before the project catalog is loaded and fails with "handler
        // unavailable". `first_tick_after_restart_admits_the_due_occurrence` is the RED;
        // the harness retries once so the sweep can reach the recovery paths.
        let tick = || scheduled_trigger::tick(&rig.state, now);
        // tick() logs a failing binding and returns no admission for it.
        let mut admissions = tick().unwrap();
        if admissions.is_empty() {
            admissions = tick().unwrap();
        }
        admissions.remove(0).run_id.unwrap()
    });
    Rig::attach(Rig {
        owner,
        state,
        db,
        project,
        host,
        calls: Arc::new(Calls::default()),
        start,
        _keep: vec![Box::new(project_dir), Box::new(guard), Box::new(config)],
    })
}

/// Acceptance B: script transform -> mapped native operation -> pinned subflow output.
pub(super) fn b_script_native_subflow() -> Scenario {
    Scenario {
        name: "B.script-native-pinned-subflow",
        build: build_b,
        known_stalls: &[],
        canon: |run| {
            let mut value = json!(run.outputs.as_ref().map(|o| &o.values));
            // The baseline commit id differs per fixture; tree and blobs are the content.
            value["receipt"]["commit"] = Value::Null;
            value
        },
        expect: |run| {
            assert_eq!(
                run.status,
                NeutralStatus::Completed {
                    outcome: "done".into()
                },
                "{run:?}"
            );
            let receipt = &run.outputs.as_ref().unwrap().values["receipt"];
            assert!(receipt["blobs"]["artifact.txt"].is_string(), "{receipt}");
        },
    }
}

fn build_b() -> Rig {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let guard = crate::config::set_config_dir_override(config.path().into());
    let f = Fixture::new();
    let store = WorkflowStore::open().unwrap();
    let mut transform = script(
        "artifact-request",
        schema(json!({"path":field("string")})),
        schema(json!({"request":field("object")})),
        "data=dict(request=dict(paths=[q['inputs']['path']]))",
    );
    transform.subject = Some(SubjectSchema {
        required: true,
        fields: schema(json!({"workspace":field("workspace_reference")})),
    });
    let catalog = save(&f, &store, transform.clone());
    let native = catalog
        .get(&DescriptorId {
            namespace: "native".into(),
            name: "artifact.pin".into(),
            version: 1,
        })
        .unwrap()
        .clone();
    let mut child = graph(&[native]);
    child.input_bindings = serde_json::from_value(
        json!({"0":{"request":{"source":"workflow_input","field":"request"}}}),
    )
    .unwrap();
    child.output_bindings = serde_json::from_value(
        json!({"receipt":{"source":"activity_output","activity":0,"field":"receipt"}}),
    )
    .unwrap();
    let child = publish(&f, &store, &catalog, "pin-child", child);
    let call = subflow(&child, "pin-subflow");
    let catalog = save(&f, &store, call.clone());
    let mut root = graph(&[transform, call]);
    root.subject = child.definition.subject.clone();
    root.control = serde_json::from_value(json!({"nodes":{"start":{"type":"start"},"work":{"type":"activity","activity":0},"child":{"type":"activity","activity":1},"end":{"type":"end","outcome":"done"}},
        "edges":{"start":{"next":"work"},"work":{"ok":"child"},"child":{"done":"end"}}})).unwrap();
    root.input_bindings = serde_json::from_value(json!({"0":{"path":{"source":"workflow_input","field":"path"}},"1":{"request":{"source":"activity_output","activity":0,"field":"request"}}})).unwrap();
    root.output_bindings = serde_json::from_value(
        json!({"receipt":{"source":"activity_output","activity":1,"field":"receipt"}}),
    )
    .unwrap();
    let publication = publish(&f, &store, &catalog, "script-native-parent", root);
    let project = f.context.workspace_path().to_str().unwrap().to_owned();
    let workspace = f.context.workspace_path().to_owned();
    let Fixture {
        dir,
        state,
        host,
        context,
        owner,
    } = f;
    let db = dir.path().join("runs.sqlite3");
    let start_project = project.clone();
    let start: super::rig::StartFn = Box::new(move |rig| {
        let request = serde_json::from_value(json!({"publication":{"project":publication.project,"id":publication.id,"revision":publication.revision},
            "request_id":publication.id,"inputs":{"path":"artifact.txt"},
            "subject":{"workspace":{"reference":"workspace","id":workspace}},
            "limits":{"max_activations":100,"max_concurrency":2,"max_duration_ms":60000}})).unwrap();
        let reply = crate::workflows::neutral_api::neutral_action(
            &rig.state,
            &start_project,
            crate::workflows::neutral_api::model::NeutralAction::Start { request },
            Some(run::NativeCaller::Owner),
        )
        .unwrap();
        let reply = serde_json::to_value(reply).unwrap();
        reply["value"]["snapshot"]["run"]["id"]
            .as_str()
            .unwrap()
            .to_owned()
    });
    Rig::attach(Rig {
        owner,
        state,
        db,
        project,
        host,
        calls: Arc::new(Calls::default()),
        start,
        _keep: vec![
            Box::new(dir),
            Box::new(context),
            Box::new(guard),
            Box::new(config),
        ],
    })
}

/// One script activity that appends a line to `script-runs.log` in the workspace on
/// every execution, so a test counts real executions independently of `CountingHost`.
pub(super) fn build_script_marker(mutation: bool) -> Rig {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let guard = crate::config::set_config_dir_override(config.path().into());
    let f = Fixture::new();
    let store = WorkflowStore::open().unwrap();
    let contract: ActivityDescriptor = serde_json::from_value(json!({
        "id":{"namespace":"acceptance","name":"marker","version":1},"purpose":"Count script executions",
        "handler":{"name":"host.script","version":1},"target":null,"config":{"fields":{}},
        "inputs":{"fields":{}},"outputs":{"fields":{}},"subject":null,"outcomes":["ok"],
        "required_capabilities":[],"execution_policies":["local"]
    }))
    .unwrap();
    let reconciliation = if mutation {
        json!({"type":"manual","procedure":"inspect script-runs.log"})
    } else {
        json!({"type":"read_only"})
    };
    let resources = if mutation {
        json!(["script-runs.log"])
    } else {
        json!([])
    };
    let policy = serde_json::from_value(json!({"version":1,"timeout_secs":10,"input_bytes":65536,
        "stdout_bytes":16384,"stderr_bytes":16384,"artifact_limit":0,"resources":resources,"environment_refs":{},
        "secret_refs":{},"mutation":mutation,"retry":"never","reconciliation":reconciliation,"output":{"type":"structured"}})).unwrap();
    let marker = run::neutral_script::script_descriptor(contract, crate::workflows::command::Invocation::Shell {
        program: if cfg!(windows) {"python"} else {"python3"}.into(),
        script: "import json,sys\nq=json.load(sys.stdin)\nopen('script-runs.log','a').write('run\\n')\nprint(json.dumps(dict(version=1,identity=q['identity'],outcome='ok',data={},artifacts={})))".into(),
        version: 1,
    }, &policy)
    .unwrap();
    let catalog = save(&f, &store, marker.clone());
    let publication = publish(&f, &store, &catalog, "script-marker", graph(&[marker]));
    let project = f.context.workspace_path().to_str().unwrap().to_owned();
    let Fixture {
        dir,
        state,
        host,
        context,
        owner,
    } = f;
    let db = dir.path().join("runs.sqlite3");
    let start_project = project.clone();
    let start: super::rig::StartFn = Box::new(move |rig| {
        let request = serde_json::from_value(json!({"publication":{"project":publication.project,"id":publication.id,"revision":publication.revision},
            "request_id":publication.id,"inputs":{},"subject":null,
            "limits":{"max_activations":10,"max_concurrency":1,"max_duration_ms":60000}})).unwrap();
        let reply = crate::workflows::neutral_api::neutral_action(
            &rig.state,
            &start_project,
            crate::workflows::neutral_api::model::NeutralAction::Start { request },
            Some(run::NativeCaller::Owner),
        )
        .unwrap();
        let reply = serde_json::to_value(reply).unwrap();
        reply["value"]["snapshot"]["run"]["id"]
            .as_str()
            .unwrap()
            .to_owned()
    });
    Rig::attach(Rig {
        owner,
        state,
        db,
        project,
        host,
        calls: Arc::new(Calls::default()),
        start,
        _keep: vec![
            Box::new(dir),
            Box::new(context),
            Box::new(guard),
            Box::new(config),
        ],
    })
}
