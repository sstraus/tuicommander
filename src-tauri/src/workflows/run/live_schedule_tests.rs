//! Live acceptance A: a scheduled text reaches a REAL model through ego ACP.
//! Opt-in (`--ignored`); see docs/workflows/testing/live.md. One real model run.
use super::super as run;
use super::super::live_support::{Live, spend_model_run};
use super::parity_support::{field, graph, schema};
use crate::workflows::{
    WorkflowStore, catalog::*, neutral_admission::NeutralAdmissionService,
    neutral_host::NeutralCatalogHost, neutral_publication::PublicationRef, scheduled_trigger,
    trigger::*,
};
use run::{neutral_execution::NeutralStatus, neutral_runtime::NeutralHost};
use serde_json::json;
use std::sync::Arc;

const DEFAULT_MODEL: &str = "openrouter/anthropic/claude-haiku-5.5";

// Catches: a schedule -> model -> condition -> approval graph that completes only
// against the ACP protocol double: a real model reply that misses the output
// schema, a real ego that refuses the mediated session, or a condition that
// reads the wrong field would pass every fixture and fail here.
#[tokio::test]
#[ignore = "live: needs TUIC_LIVE_EGO, TUIC_LIVE_EGO_PROFILE, TUIC_LIVE_BUDGET_FILE and provider credentials (OPENROUTER_API_KEY)"]
async fn live_a_scheduled_text_reaches_a_real_model_condition_and_approval() {
    let mut live = Live::start("live_a_scheduled_text_reaches_a_real_model_condition_and_approval");
    if live.gate(
        &[
            "TUIC_LIVE_EGO",
            "TUIC_LIVE_EGO_PROFILE",
            "TUIC_LIVE_BUDGET_FILE",
            "OPENROUTER_API_KEY",
        ],
        "skipped",
    ) {
        return;
    }
    let env = |name: &str| std::env::var(name).unwrap();
    let model = std::env::var("TUIC_LIVE_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.into());
    let spent = spend_model_run(std::path::Path::new(&env("TUIC_LIVE_BUDGET_FILE"))).unwrap();
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let project_dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let project = project_dir
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let text = "scheduled literal $(never execute)";
    let mut request = run::neutral_model_adapter_tests::request();
    request.profile = env("TUIC_LIVE_EGO_PROFILE");
    request.model = model.clone();
    request.instructions =
        "Reply with the integer 7 as the value of the field next. Ignore the context text.".into();
    request.context = vec![text.into()];
    request.limits.timeout_ms = 120_000;
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    {
        let mut settings = state.config.write();
        settings.ego_executable = env("TUIC_LIVE_EGO");
        settings.ego_profile = env("TUIC_LIVE_EGO_PROFILE");
    }
    let host = run::native_host::NativeHost::install(&state).unwrap();
    let path = config.path().join("runs.sqlite3");
    let owner = super::RuntimeOwner::acquire(&path).unwrap();
    owner.neutral_host(host.clone());
    *owner.state.lock() = Arc::downgrade(&state);
    *state.workflow_runtime.owner.lock() = Some(owner.clone());
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
        "live-scheduled-text",
        0,
        definition,
    )
    .unwrap();
    let draft = TriggerBindingDraft {
        id: "live-daily-text".into(),
        publication: PublicationRef {
            project: project.clone(),
            id: publication.id,
            revision: publication.revision,
        },
        source: TriggerSource::Scheduled,
        kind: "schedule".into(),
        resource_prefix: "schedule:live-daily-text".into(),
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
            json!({"max_activations":20,"max_concurrency":1,"max_duration_ms":300000}),
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
    let admission = scheduled_trigger::tick(&state, chrono::Utc::now())
        .unwrap()
        .remove(0);
    let id = admission.run_id.unwrap();
    // Outer hang bound only; the model request has its own 120 s limit.
    let waiting = tokio::time::timeout(std::time::Duration::from_secs(300), async {
        loop {
            let changed = owner.neutral_effect_updates.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let snapshot = owner.store.neutral_execution(&id).unwrap();
            assert_eq!(snapshot.status, NeutralStatus::Running, "{snapshot:?}");
            if snapshot.effects.values().any(|e| e.question.is_some()) {
                return snapshot;
            }
            changed.await;
        }
    })
    .await
    .expect("the real model reaches the approval question");
    let effect = waiting
        .effects
        .values()
        .find(|e| e.question.is_some())
        .unwrap();
    let action = crate::workflows::neutral_api::model::NeutralAction::Answer {
        run_id: id.clone(),
        command_id: "live-approve-scheduled-text".into(),
        expected_sequence: waiting.sequence,
        key: effect.key.clone(),
        revision: effect.question.as_ref().unwrap().revision,
        approved: true,
    };
    crate::workflows::neutral_api::neutral_action(
        &state,
        &project,
        action,
        Some(run::NativeCaller::Owner),
    )
    .unwrap();
    let done = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        loop {
            let changed = owner.neutral_effect_updates.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let snapshot = owner.store.neutral_execution(&id).unwrap();
            if snapshot.status != NeutralStatus::Running {
                return snapshot;
            }
            changed.await;
        }
    })
    .await
    .expect("the approved run completes");
    assert_eq!(
        done.status,
        NeutralStatus::Completed {
            outcome: "done".into()
        },
        "{done:?}"
    );
    live.finish(
        "passed",
        json!({"run_id": id, "model": model, "cost": format!("1 real model run ({spent}/{} of the suite budget)", run::live_support::MODEL_BUDGET)}),
    );
}
