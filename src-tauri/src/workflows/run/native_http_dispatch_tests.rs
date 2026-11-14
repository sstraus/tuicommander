use super::native_family_tests::http;
use super::native_host::NativeHost;
use super::native_test_support::Fixture;
use super::neutral::{NeutralLimits, NeutralStartRequest};
use super::neutral_execution_tests::step;
use super::neutral_runtime::NeutralHost;
use crate::workflows::WorkflowStore;
use crate::workflows::catalog::{ActivityInstance, ObjectSchema, ReusableTarget};
use crate::workflows::neutral_admission::NeutralAdmissionService;
use crate::workflows::neutral_host::NeutralCatalogHost;
use crate::workflows::neutral_publication::PublicationRef;
use serde_json::json;
use std::collections::BTreeMap;

// Catches composition preparing an approval but never reaching the existing HTTP
// client after approval, or issuing the write before its exact ledger answer.
#[tokio::test]
async fn native_saved_http_write_dispatches_once_after_exact_approval() {
    Box::pin(saved_http_write_after_approval(false)).await;
}

// Catches: a revision published while a run waits at its approval changing
// that run, so the approved write goes to the new endpoint (018 AC7, a draft
// edit changing a running workflow).
#[tokio::test]
async fn publishing_a_new_revision_mid_run_keeps_the_run_on_its_pinned_revision() {
    Box::pin(saved_http_write_after_approval(true)).await;
}

async fn saved_http_write_after_approval(republish_mid_run: bool) {
    let mut f = Fixture::new();
    let _config = crate::config::set_config_dir_override(f.dir.path().into());
    let workspace = f.context.workspace_path().to_owned();
    let (state, base, server) =
        super::neutral_http_fixture_server::server(f.dir.path(), vec![(200, json!({}), None)])
            .await;
    f.state = state;
    f.host = NativeHost::install(&f.state).unwrap();
    f.context =
        super::NativeContext::capture(&f.state, super::NativeCaller::Owner, &workspace, vec![])
            .unwrap();
    f.owner.neutral_host(f.host.clone());
    let template = f.start("workspace.pin", json!({}));
    let project = workspace.to_string_lossy().into_owned();
    let mut descriptor = http();
    if let Some(ReusableTarget::Http {
        base: installed, ..
    }) = &mut descriptor.target
    {
        *installed = base.clone();
    }
    descriptor
        .config
        .fields
        .get_mut("request")
        .unwrap()
        .default
        .as_mut()
        .unwrap()["endpoint"] = json!(format!("{base}items"));
    let workflows = WorkflowStore::open_at(&f.dir.path().join("saved-http.sqlite3")).unwrap();
    let installed = f.host.catalog(&project).unwrap();
    let registry = f.host.registry();
    workflows
        .save_reusable_operation(&project, 0, descriptor.clone(), &installed, &registry)
        .unwrap();
    let catalog = workflows
        .build_project_catalog(&project, &installed, &registry)
        .unwrap();
    let request = descriptor.config.fields["request"].default.clone().unwrap();
    let mut definition = template.publication.definition;
    definition.inputs = ObjectSchema::default();
    definition.subject = None;
    definition.instances = vec![ActivityInstance {
        descriptor: descriptor.id,
        config: BTreeMap::new(),
        execution_policy: "local".into(),
    }];
    definition.input_bindings.clear();
    definition.control.edges.insert(
        "work".into(),
        BTreeMap::from([("complete".into(), "done".into())]),
    );
    let context = f.host.publication_context(&project).unwrap();
    let admission = NeutralAdmissionService {
        store: &workflows,
        catalog: &catalog,
        registry: &registry,
    };
    let publication = admission
        .publish(
            Some(context.as_ref()),
            &project,
            "saved-http",
            0,
            definition.clone(),
        )
        .unwrap();
    let run = f
        .owner
        .start_neutral(
            &workflows,
            &NeutralStartRequest {
                publication: PublicationRef {
                    project: project.clone(),
                    id: publication.id,
                    revision: publication.revision,
                },
                request_id: "exact-http-write".into(),
                inputs: BTreeMap::new(),
                subject: None,
                limits: NeutralLimits {
                    max_activations: 20,
                    max_concurrency: 1,
                    max_duration_ms: 600_000,
                },
            },
            &catalog,
            &registry.available_handlers(),
        )
        .unwrap();
    f.host
        .bind_run(&f.owner.store, &run.id, f.context.clone())
        .unwrap();
    let run = f.owner.store.neutral_execution(&run.id).unwrap();
    let at = super::store::now_ms();
    let run = step(&f.owner, &run, "enter", at);
    let run = step(&f.owner, &run, "prepare", at);
    let run = f.dispatch(&run).await;
    let effect = run.effects.values().next().unwrap();
    assert!(effect.question.is_some() && effect.answer.is_none() && !effect.dispatched);
    if republish_mid_run {
        let mut edited = request.clone();
        edited["endpoint"] = json!(format!("{base}other"));
        definition.instances[0]
            .config
            .insert("request".into(), edited);
        let second = admission
            .publish(
                Some(context.as_ref()),
                &project,
                "saved-http",
                publication.revision,
                definition,
            )
            .unwrap();
        assert_eq!(second.revision, publication.revision + 1);
    }
    let run = f.approve(&run);
    let run = f.dispatch(&run).await;
    assert!(
        run.effects.values().all(|effect| effect.settled()),
        "{run:?}"
    );
    assert_eq!(run.run.closure.root.revision, publication.revision);
    let requests = server.await.unwrap();
    assert_eq!(requests.len(), 1);
    assert!(
        requests[0].starts_with("POST /api/items HTTP/1.1\r\n"),
        "{}",
        requests[0]
    );
}
