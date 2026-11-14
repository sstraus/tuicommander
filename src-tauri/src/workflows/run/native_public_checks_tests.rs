use super::super::NativeCaller;
use super::super::native_test_support::Fixture;
use super::native_public_support::{attach, call, ready, request};
use crate::workflows::WorkflowStore;
use crate::workflows::neutral_api::model::NeutralAction;
use crate::workflows::neutral_host::NeutralCatalogHost;
use serde_json::json;

// Catches: start JSON selecting arbitrary check names, dynamic publication
// requests slipping through, or retries replacing captured verification config.
#[tokio::test]
async fn native_public_checks_use_published_constant_and_retry_keeps_capture() {
    use crate::workflows::data_binding::DataSource;
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let f = Fixture::new();
    attach(&f);
    let pin = f.context.pin_artifact(&["artifact.txt".into()]).unwrap();
    let mut request = request(
        &f,
        "verification.run",
        json!({"check":"injected-check","commit":pin.commit,"tree":pin.tree}),
    );
    assert!(
        call(
            &f,
            NeutralAction::Start {
                request: request.clone()
            },
            Some(NativeCaller::Owner)
        )
        .unwrap_err()
        .contains("published constant")
    );
    assert!(
        f.owner
            .store
            .neutral_start_receipt(
                &request,
                &crate::workflows::catalog::content_digest(&request).unwrap()
            )
            .unwrap()
            .is_none()
    );
    let store = WorkflowStore::open().unwrap();
    let catalog = f.host.catalog(&request.publication.project).unwrap();
    let handlers =
        crate::workflows::run::NeutralHost::registry(f.host.as_ref()).available_handlers();
    let previous = store
        .get_neutral_publication(
            &request.publication.project,
            &request.publication.id,
            1,
            &catalog,
            &handlers,
        )
        .unwrap();
    let mut definition = previous.definition;
    definition.input_bindings.get_mut(&0).unwrap().insert(
        "request".into(),
        DataSource::Constant {
            value: json!({"check":"exact-tree","commit":pin.commit,"tree":pin.tree}),
        },
    );
    store
        .save_neutral_publication(
            &request.publication.project,
            &request.publication.id,
            1,
            definition,
            &catalog,
            &handlers,
        )
        .unwrap();
    request.publication.revision = 2;
    let run = call(
        &f,
        NeutralAction::Start {
            request: request.clone(),
        },
        Some(NativeCaller::Owner),
    )
    .unwrap();
    f.host
        .install_project_checks(f.context.workspace_path(), vec![])
        .unwrap();
    call(
        &f,
        NeutralAction::Start { request },
        Some(NativeCaller::Owner),
    )
    .unwrap();
    let run = ready(&f, run);
    let checked = f.dispatch(&run).await;
    let receipt = serde_json::to_value(checked.effects.values().next().unwrap()).unwrap();
    assert_eq!(
        receipt["observation"]["outcome"]["result"]["outputs"]["receipt"]["checkId"], "exact-tree",
        "{receipt}"
    );
}
