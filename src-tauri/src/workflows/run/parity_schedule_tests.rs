use super::super as run;
use super::parity_support::{field, graph, schema};
use crate::workflows::{
    WorkflowStore, catalog::*, neutral_admission::NeutralAdmissionService,
    neutral_host::NeutralCatalogHost, neutral_publication::PublicationRef, scheduled_trigger,
    trigger::*,
};
use run::{neutral_execution::NeutralStatus, neutral_runtime::NeutralHost};
use serde_json::{Value, json};
use std::sync::Arc;

// Catches: scheduled starts remain unbound, text never reaches the mediated
// adapter, condition bypasses exact approval, or a repeated wake/reopen starts twice.
#[tokio::test]
async fn scheduled_text_reaches_model_condition_and_exact_local_approval_without_git() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let (project_dir, runtime) = run::neutral_model_adapter_tests::fixture("{\"next\":7}", false);
    let project = project_dir
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let mut request = run::neutral_model_adapter_tests::request();
    let text = "scheduled literal $(never execute)";
    request.context.push(text.into());
    // The existing ACP protocol double must receive the exact literal prompt;
    // it provides model output only, never trigger, condition or inbox behavior.
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
    let host = run::native_host::NativeHost::install(&state).unwrap();
    let path = config.path().join("runs.sqlite3");
    let owner = super::RuntimeOwner::acquire(&path).unwrap();
    owner.neutral_host(host.clone());
    *owner.state.lock() = Arc::downgrade(&state);
    *state.workflow_runtime.owner.lock() = Some(owner.clone());
    let store = WorkflowStore::open().unwrap();
    let catalog = host.catalog(&project).unwrap();
    let model = catalog
        .get(&DescriptorId {
            namespace: "model".into(),
            name: "request".into(),
            version: 1,
        })
        .unwrap()
        .clone();
    let approval = catalog
        .get(&DescriptorId {
            namespace: "inbox".into(),
            name: "approval".into(),
            version: 1,
        })
        .unwrap()
        .clone();
    let mut definition = graph(&[model, approval]);
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
            text: text.into(),
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
    let admission = scheduled_trigger::tick(&state, now).unwrap().remove(0);
    let id = admission.run_id.unwrap();
    let waiting = loop {
        let changed = owner.neutral_effect_updates.notified();
        tokio::pin!(changed);
        changed.as_mut().enable();
        let snapshot = owner.store.neutral_execution(&id).unwrap();
        assert_eq!(snapshot.status, NeutralStatus::Running, "{snapshot:?}");
        if snapshot.effects.values().any(|e| e.question.is_some()) {
            break snapshot;
        }
        changed.await;
    };
    let effect = waiting
        .effects
        .values()
        .find(|e| e.question.is_some())
        .unwrap();
    assert_eq!(effect.question.as_ref().unwrap().title, text);
    assert!(!effect.dispatched && effect.answer.is_none());
    let model = waiting
        .effects
        .values()
        .find(|e| e.invocation.descriptor.handler.name == "host.model")
        .unwrap();
    assert!(model.settled() && model.model_session.is_some());
    assert!(
        host.resume_run(&owner.store, &id, Some(&run::NativeCaller::Owner), &project)
            .unwrap()
    );
    assert!(!project_dir.path().join(".git").exists());
    let action = crate::workflows::neutral_api::model::NeutralAction::Answer {
        run_id: id.clone(),
        command_id: "approve-scheduled-text".into(),
        expected_sequence: waiting.sequence,
        key: effect.key.clone(),
        revision: effect.question.as_ref().unwrap().revision,
        approved: true,
    };
    assert!(
        crate::workflows::neutral_api::neutral_action(&state, &project, action.clone(), None)
            .is_err()
    );
    crate::workflows::neutral_api::neutral_action(
        &state,
        &project,
        action,
        Some(run::NativeCaller::Owner),
    )
    .unwrap();
    loop {
        let changed = owner.neutral_effect_updates.notified();
        tokio::pin!(changed);
        changed.as_mut().enable();
        let snapshot = owner.store.neutral_execution(&id).unwrap();
        if snapshot.status != NeutralStatus::Running {
            assert_eq!(
                snapshot.status,
                NeutralStatus::Completed {
                    outcome: "done".into()
                },
                "{snapshot:?}"
            );
            assert_eq!(snapshot.outputs.unwrap().values["approved"], json!(true));
            break;
        }
        changed.await;
    }
    // Reopening the durable store cannot forget the occurrence/start receipt.
    let reopened = run::RunStore::open_at(&path).unwrap();
    assert_eq!(
        reopened.neutral_run(&id).unwrap().inputs["text"],
        json!(text)
    );
    assert_eq!(
        scheduled_trigger::tick(&state, now).unwrap()[0]
            .run_id
            .as_deref(),
        Some(id.as_str())
    );
    let counts:(i64,i64) = reopened.trigger_connection().unwrap().query_row("SELECT (SELECT count(*) FROM neutral_workflow_runs),(SELECT count(*) FROM neutral_trigger_intents)",[],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
    assert_eq!(counts, (1, 1));
}
