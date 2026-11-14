//! Public commands select the existing neutral owner; adapter callbacks stay private.
use super::neutral_effects::EffectCommand;
use super::neutral_execution::NeutralExecutionCommand;
use super::runtime::RuntimeOwner;
use crate::state::AppState;
use crate::workflows::WorkflowStore;
use crate::workflows::neutral_api::model::{NeutralAction, NeutralReply};
use serde_json::{Value, json};
use std::sync::Arc;
impl RuntimeOwner {
    pub(super) fn publish_neutral_cursor(&self, project: &str, run_id: &str, sequence: i64) {
        if let Some(state) = self.state.lock().upgrade() {
            super::api::emit_run_cursor(&state, project, run_id, sequence);
        }
        self.neutral_effect_updates.notify_waiters();
    }
}

fn inspect(
    store: &super::RunStore,
    project: &str,
    id: &str,
    after: i64,
    limit: usize,
) -> Result<Value, String> {
    let (snapshot, events) = store.neutral_inspection(id, after, limit)?;
    if snapshot.run.request.publication.project != project {
        return Err("neutral run does not belong to project".into());
    }
    Ok(json!({"snapshot":snapshot,"events":events}))
}

pub(crate) fn run_action(
    state: &Arc<AppState>,
    project: &str,
    action: NeutralAction,
    caller: Option<super::NativeCaller>,
) -> Result<NeutralReply, String> {
    match action {
        NeutralAction::Inspect {
            run_id,
            after_sequence,
            limit,
        } => inspect(
            &super::RunStore::open()?,
            project,
            &run_id,
            after_sequence,
            limit,
        )
        .map(NeutralReply::Inspection),
        NeutralAction::Events {
            run_id,
            after_sequence,
            limit,
        } => {
            let store = super::RunStore::open()?;
            let view = inspect(&store, project, &run_id, after_sequence, limit)?;
            Ok(NeutralReply::Events(view["events"].clone()))
        }
        NeutralAction::Start { request } => start_request(state, project, request, caller),
        NeutralAction::StartDelivery { story, key } => {
            start_delivery(state, project, &story, &key, caller)
        }
        NeutralAction::DeliveryRuns { story } => delivery_runs(state, project, &story),
        NeutralAction::Cancel {
            run_id,
            command_id,
            expected_sequence,
        } => command(
            state,
            project,
            &run_id,
            &command_id,
            expected_sequence,
            NeutralExecutionCommand::Cancel,
        ),
        NeutralAction::OperatorAnswer {
            run_id,
            command_id,
            expected_sequence,
            key,
            revision,
            answer,
        } => {
            let owner = state.workflow_runtime.neutral_owner()?;
            let effect = state
                .workflow_runtime
                .neutral_catalog_host()?
                .operator_answer(
                    &owner.store,
                    &run_id,
                    &key,
                    revision,
                    expected_sequence,
                    &answer,
                    caller.as_ref(),
                )?;
            command(
                state,
                project,
                &run_id,
                &command_id,
                expected_sequence,
                NeutralExecutionCommand::Effect { command: effect },
            )
        }
        NeutralAction::Answer {
            run_id,
            command_id,
            expected_sequence,
            key,
            revision,
            approved,
        } => {
            let owner = state.workflow_runtime.neutral_owner()?;
            state
                .workflow_runtime
                .neutral_catalog_host()?
                .authorize_effect_answer(&owner.store, &run_id, &key, caller.as_ref())?;
            command(
                state,
                project,
                &run_id,
                &command_id,
                expected_sequence,
                NeutralExecutionCommand::Effect {
                    command: EffectCommand::Answer {
                        key,
                        revision,
                        approved,
                    },
                },
            )
        }
        _ => Err("not a neutral run action".into()),
    }
}

/// Capture backend identity before persistence and bind before any dispatch wake.
/// Serialize exact retries with creation so another caller cannot claim the run.
fn start_request(
    state: &Arc<AppState>,
    project: &str,
    request: super::neutral::NeutralStartRequest,
    caller: Option<super::NativeCaller>,
) -> Result<NeutralReply, String> {
    if request.publication.project != project {
        return Err("neutral publication does not belong to project".into());
    }
    let owner = state.workflow_runtime.neutral_owner()?;
    let _start = owner.neutral_start_lock.lock();
    let host = state.workflow_runtime.neutral_catalog_host()?;
    if let Some(run) = owner.store.neutral_start_receipt(
        &request,
        &crate::workflows::catalog::content_digest(&request)?,
    )? {
        if !host.resume_run(&owner.store, &run.id, caller.as_ref(), project)? {
            let context = super::neutral_native::capture(
                state,
                host.as_ref(),
                caller,
                project,
                std::iter::once(&run.closure.root).chain(&run.closure.children),
                &run.closure.descriptors,
            )?;
            if let Some(context) = context {
                host.bind_run(&owner.store, &run.id, context)?;
            }
        }
        state.workflow_runtime.wake(&run.id);
        return inspect(&owner.store, project, &run.id, 0, 500).map(NeutralReply::Inspection);
    }
    let store = WorkflowStore::open()?;
    let installed = host.catalog(project)?;
    let registry = host.registry();
    let catalog = store.build_project_catalog(project, &installed, &registry)?;
    let handlers = registry.available_handlers();
    let (root, children) = store.resolve_neutral_publication(
        project,
        &request.publication.id,
        request.publication.revision,
        &catalog,
        &handlers,
    )?;
    let context = super::neutral_native::capture(
        state,
        host.as_ref(),
        caller,
        project,
        std::iter::once(&root).chain(&children),
        &catalog.descriptors().cloned().collect::<Vec<_>>(),
    )?;
    let run = owner.start_neutral(&store, &request, &catalog, &handlers)?;
    if let Some(context) = context {
        host.bind_run(&owner.store, &run.id, context)?;
    }
    state.workflow_runtime.wake(&run.id);
    inspect(&owner.store, project, &run.id, 0, 500).map(NeutralReply::Inspection)
}

fn delivery_prefix(story: &str) -> Result<String, String> {
    if story.is_empty() || story.contains(':') {
        return Err("story id must be nonempty and contain no ':'".into());
    }
    Ok(format!("{}{story}:", super::plan_delivery::RUN_PREFIX))
}

/// Start the plan and story delivery workflow for one story, installing it on
/// first use. A repeated key resumes that run: its retained request rebinds
/// the caller after a restart and wakes the run at its recorded position.
fn start_delivery(
    state: &Arc<AppState>,
    project: &str,
    story: &str,
    key: &str,
    caller: Option<super::NativeCaller>,
) -> Result<NeutralReply, String> {
    if key.is_empty() || key.len() > 200 {
        return Err("delivery key must be nonempty and at most 200 bytes".into());
    }
    let request_id = format!("{}{key}", delivery_prefix(story)?);
    let owner = state.workflow_runtime.neutral_owner()?;
    if let Some((id, _)) = owner
        .store
        .neutral_runs_with_request_prefix(project, &request_id)?
        .into_iter()
        .find(|(_, existing)| *existing == request_id)
    {
        let request = owner.store.neutral_run(&id)?.request;
        return start_request(state, project, request, caller);
    }
    let host = state.workflow_runtime.neutral_catalog_host()?;
    let installed = host.catalog(project)?;
    let registry = host.registry();
    let context = host.publication_context(project)?;
    let draft = super::plan_delivery::install(
        &WorkflowStore::open()?,
        project,
        &installed,
        &registry,
        context.as_ref(),
        &super::plan_delivery::Commands::default(),
    )?;
    let request = super::neutral::NeutralStartRequest {
        publication: crate::workflows::neutral_publication::PublicationRef {
            project: project.into(),
            id: draft.id,
            revision: draft.latest_published_revision,
        },
        request_id,
        inputs: [(
            "story".to_owned(),
            super::plan_delivery::story_input(project, story)?,
        )]
        .into(),
        subject: Some(
            [(
                "workspace".to_owned(),
                json!({"reference": "workspace", "id": project}),
            )]
            .into(),
        ),
        limits: super::neutral::NeutralLimits {
            max_activations: 200,
            max_concurrency: 1,
            max_duration_ms: 86_400_000,
        },
    };
    start_request(state, project, request, caller)
}

/// The delivery runs of one story, newest first. `bound` is false for a
/// running run that waits for a resume after a restart.
fn delivery_runs(
    state: &Arc<AppState>,
    project: &str,
    story: &str,
) -> Result<NeutralReply, String> {
    let prefix = delivery_prefix(story)?;
    let owner = state.workflow_runtime.neutral_owner()?;
    let host = state.workflow_runtime.neutral_catalog_host().ok();
    let runs = owner
        .store
        .neutral_runs_with_request_prefix(project, &prefix)?
        .into_iter()
        .map(|(id, request_id)| {
            let snapshot = owner.store.neutral_execution(&id)?;
            Ok(json!({
                "run_id": id,
                "key": request_id.strip_prefix(&prefix).unwrap_or_default(),
                "status": snapshot.status,
                "bound": host.as_ref().is_some_and(|host| host.bound(&id)),
            }))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(NeutralReply::DeliveryRuns(runs))
}

fn command(
    state: &Arc<AppState>,
    project: &str,
    id: &str,
    command_id: &str,
    sequence: i64,
    command: NeutralExecutionCommand,
) -> Result<NeutralReply, String> {
    let owner = state.workflow_runtime.neutral_owner()?;
    if owner.store.neutral_run(id)?.request.publication.project != project {
        return Err("neutral run does not belong to project".into());
    }
    let receipt = owner.neutral_command(id, command_id, sequence, command)?;
    state.workflow_runtime.wake(id);
    let events = owner
        .store
        .neutral_events_after(id, 0, 500)?
        .into_iter()
        .filter(|e| e.sequence <= receipt.sequence)
        .collect::<Vec<_>>();
    Ok(NeutralReply::Inspection(
        json!({"snapshot":receipt,"events":events}),
    ))
}

/// Calls only deterministic preparation after real host admission. The synthetic
/// fence is never committed, dispatched or returned as a resumable effect key.
pub(crate) fn preview_effect(
    host: &dyn crate::workflows::neutral_host::NeutralCatalogHost,
    publication: &crate::workflows::neutral_publication::NeutralPublication,
    descriptor: &crate::workflows::catalog::ActivityDescriptor,
    instance: &crate::workflows::catalog::ActivityInstance,
    inputs: &std::collections::BTreeMap<String, Value>,
    subject: Option<&std::collections::BTreeMap<String, Value>>,
) -> Result<Value, String> {
    let invocation = super::neutral_runtime::NeutralInvocation {
        fence: super::neutral_execution::NeutralAttemptFence {
            run: "preview".into(),
            execution: "preview".into(),
            activation: 1,
            attempt: 1,
            scope: None,
        },
        publication: publication.clone(),
        descriptor: descriptor.clone(),
        instance: instance.clone(),
        inputs: inputs.clone(),
        subject: subject.cloned(),
    };
    serde_json::to_value(host.preview(&invocation)?).map_err(|e| e.to_string())
}
