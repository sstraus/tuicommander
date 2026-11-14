use super::super::NativeCaller;
use super::super::native_test_support::Fixture;
use super::super::neutral::NeutralStartRequest;
use super::super::neutral_execution::NeutralExecution;
use crate::workflows::WorkflowStore;
use crate::workflows::neutral_api::{model::NeutralAction, neutral_action};
use serde_json::{Value, json};

pub(super) fn attach(f: &Fixture) {
    *f.state.workflow_runtime.owner.lock() = Some(f.owner.clone());
}
pub(super) fn request(f: &Fixture, operation: &str, value: Value) -> NeutralStartRequest {
    let template = f.start(operation, value);
    let store = WorkflowStore::open().unwrap();
    let host = f.state.workflow_runtime.neutral_catalog_host().unwrap();
    let catalog = host.catalog(&template.publication.project).unwrap();
    let handlers = host.registry().available_handlers();
    let publication = store
        .save_neutral_publication(
            &template.publication.project,
            &format!("public-{operation}"),
            0,
            template.publication.definition,
            &catalog,
            &handlers,
        )
        .unwrap();
    let mut request = template.run.request;
    request.publication.id = publication.id;
    request.request_id = format!("public-{operation}");
    request
}
pub(super) fn call(
    f: &Fixture,
    action: NeutralAction,
    caller: Option<NativeCaller>,
) -> Result<NeutralExecution, String> {
    let reply = neutral_action(
        &f.state,
        f.context.workspace_path().to_str().unwrap(),
        action,
        caller,
    )?;
    let value = serde_json::to_value(reply).unwrap();
    let id = value["value"]["snapshot"]["run"]["id"]
        .as_str()
        .ok_or("public reply has no retained run identity")?;
    f.owner.store.neutral_execution(id)
}
pub(super) fn ready(f: &Fixture, mut run: NeutralExecution) -> NeutralExecution {
    for id in ["public-enter", "public-prepare"] {
        run = super::super::neutral_execution_tests::step(
            &f.owner,
            &run,
            id,
            super::super::store::now_ms(),
        );
    }
    run
}
pub(super) fn answer(run: &NeutralExecution, revision: u64) -> NeutralAction {
    let effect = run.effects.values().next().unwrap();
    NeutralAction::Answer {
        run_id: run.run.id.clone(),
        command_id: format!("answer-{revision}"),
        expected_sequence: run.sequence,
        key: effect.key.clone(),
        revision,
        approved: true,
    }
}
pub(super) fn commit_request() -> Value {
    json!({"parent":"base","tree":"tree","message":"candidate","review":{"fence":{"run":"review","execution":"review","activation":1,"attempt":1,"scope":null},"digest":"review"}})
}
