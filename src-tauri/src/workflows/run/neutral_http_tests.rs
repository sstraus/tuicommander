use super::neutral_execution::NeutralStatus;
use super::neutral_http_fixture::*;
use crate::workflows::catalog::ValueType;
use crate::workflows::collection::{CollectionIdentity, EmptyPolicy, FailurePolicy, ForEach};
use crate::workflows::connector::BoundedValue;
use crate::workflows::data_binding::{DataSource, DataState};
use serde_json::json;
use std::collections::BTreeMap;

// Catches: adapter dropping method/query/body/header data or advancing with an
// unmapped response; the assertion observes actual bytes and retained output.
#[tokio::test]
async fn typed_request_reaches_installed_client_and_completes_owned_run() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(dir.path().into());
    let (state, base, server) = server(dir.path(), vec![(200, json!({"value": 42}), None)]).await;
    let mut op = operation(&base, false);
    op.method = "POST".into();
    op.path = vec![BoundedValue::Input {
        field: "initial".into(),
        max_bytes: 32,
    }];
    op.query.insert(
        "q".into(),
        BoundedValue::Literal {
            value: json!("a & b"),
        },
    );
    op.headers.insert(
        "X-Context".into(),
        BoundedValue::Input {
            field: "initial".into(),
            max_bytes: 32,
        },
    );
    op.body = Some(BoundedValue::Input {
        field: "initial".into(),
        max_bytes: 32,
    });
    let f = fixture(state, &base, op, contract(false));
    let result = run(&f).await;
    assert_eq!(
        result.status,
        NeutralStatus::Completed {
            outcome: "done".into()
        }
    );
    assert_eq!(result.outputs.unwrap().values["value"], json!(42));
    let requests = server.await.unwrap();
    assert_eq!(requests.len(), 1);
    assert!(
        requests[0].starts_with("POST /api/records/6?q=a+%26+b HTTP/1.1\r\n"),
        "{}",
        requests[0]
    );
    assert!(
        requests[0]
            .to_ascii_lowercase()
            .contains("x-context: 6\r\n")
    );
    assert!(requests[0].ends_with("\r\n\r\n6"));
    assert!(result.effects.values().all(|effect| effect.settled()));
}

// Catches: retaining only the last page, accepting service-selected URLs, or
// producing a collection snapshot disconnected from the owned activity output.
#[tokio::test]
async fn paginated_activity_output_drives_existing_collection_in_source_order() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(dir.path().into());
    let (state, base, server) = server(
        dir.path(),
        vec![
            (
                200,
                json!({"items": ["first", "second"], "next": "next /?&"}),
                None,
            ),
            (200, json!({"items": ["third"], "next": null}), None),
        ],
    )
    .await;
    let f = fixture(state, &base, operation(&base, true), contract(true));
    let result = run(&f).await;
    assert_eq!(
        result.status,
        NeutralStatus::Completed {
            outcome: "done".into()
        }
    );
    assert_eq!(
        result.outputs.as_ref().unwrap().values["items"],
        json!(["first", "second", "third"])
    );
    let collection = ForEach {
        collection: DataSource::ActivityOutput {
            activity: 0,
            field: "items".into(),
        },
        item_type: ValueType::String,
        body: 1,
        item_input: "item".into(),
        index_input: "index".into(),
        max_items: 4,
        max_concurrency: 2,
        empty: EmptyPolicy::Complete,
        failure: FailurePolicy::Collect,
    };
    let expanded = collection
        .expand(
            CollectionIdentity {
                run: f.invocation.fence.run.clone(),
                scope: "records".into(),
                generation: 1,
            },
            &BTreeMap::new(),
            &result.activity_outputs,
            &BTreeMap::new(),
        )
        .unwrap();
    assert_eq!(expanded.items().len(), 3);
    assert_eq!(
        expanded.items()[0].inputs["item"].state,
        DataState::Present(json!("first"))
    );
    assert_eq!(
        expanded.items()[2].inputs["item"].state,
        DataState::Present(json!("third"))
    );
    assert_eq!(
        expanded.items()[2].inputs["index"].state,
        DataState::Present(json!(2))
    );
    assert!(!expanded.items()[2].inputs["item"].provenance.is_empty());
    let requests = server.await.unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests[1].starts_with("GET /api/records?cursor=next+%2F%3F%26 HTTP/1.1\r\n"));
}

// Catches: a read that the service refuses (404) leaves the client as Err,
// which the owner records as uncertainty that reconcile can never settle, so
// the run holds until expiry instead of failing the activity. A GET has no
// effect to reconcile; its refusal is a definite failure.
#[tokio::test]
async fn refused_read_fails_the_activity_instead_of_holding_uncertainty() {
    use super::neutral_effects::EffectOutcome;
    use super::neutral_execution::NeutralResult;
    use super::neutral_runtime::NeutralHost;
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(dir.path().into());
    let (state, base, server) =
        server(dir.path(), vec![(404, json!({"error": "missing"}), None)]).await;
    let f = fixture(state, &base, operation(&base, false), contract(false));
    let observed = f.host.invoke_effect(claimed(&f)).await;
    server.await.unwrap();
    assert!(observed.as_ref().unwrap().stopped);
    assert!(
        matches!(
            &observed,
            Ok(observation) if matches!(
                observation.outcome,
                EffectOutcome::Completed { result: NeutralResult::Failed { .. } }
            )
        ),
        "refused read must be a failed activity, got {observed:?}"
    );
}

// Catches: confirmed empty 204 writes and HEAD responses becoming permanent
// uncertainty because the response parser requires a JSON body.
#[tokio::test]
async fn confirmed_empty_write_and_head_complete_instead_of_becoming_uncertain() {
    use super::neutral_effects::EffectOutcome;
    use super::neutral_execution::NeutralResult;
    use super::neutral_runtime::NeutralHost;
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(dir.path().into());
    for (method, status) in [("DELETE", 204), ("HEAD", 200)] {
        let (state, base, server) = server(dir.path(), vec![(status, json!({}), None)]).await;
        let mut op = operation(&base, false);
        op.method = method.into();
        op.response.clear();
        let f = fixture(state, &base, op, contract(false));
        let effect = claimed(&f);
        assert_eq!(effect.preparation.mutation, method == "DELETE");
        let observation = f.host.invoke_effect(effect).await.unwrap();
        assert!(observation.stopped);
        assert!(matches!(observation.outcome, EffectOutcome::Completed {
            result: NeutralResult::Completed { outputs, .. }
        } if outputs.is_empty()));
        assert!(server.await.unwrap()[0].starts_with(method));
    }
}
