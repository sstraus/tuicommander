use super::neutral_effects::{EffectCommand, EffectOutcome};
use super::neutral_execution::NeutralResult;
use super::neutral_http::HttpContext;
use super::neutral_http_fixture::*;
use super::neutral_runtime::NeutralHost;
use crate::workflows::connector::{BoundedValue, CallbackCorrelation, Reconciliation};
use crate::workflows::neutral_admission::HostAdmission;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::Ordering;

// Catches: publication granting dispatch forever, treating network as a write
// grant, accepting mutable connector metadata, or letting data choose authority.
#[tokio::test]
async fn publication_and_dispatch_enforce_installed_boundaries_and_current_grants() {
    let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
    let base = "http://127.0.0.1:1/api/";
    let f = fixture(state, base, operation(base, false), contract(false));
    let i = &f.invocation;
    let context = HttpContext {
        network: BTreeSet::from(["local-recorded".into()]),
        writes: BTreeSet::new(),
    };
    f.host
        .publication(&context, &i.publication, &i.descriptor, &i.instance)
        .unwrap();
    let mut instance = i.instance.clone();
    let mut op = operation(base, false);
    op.method = "POST".into();
    instance
        .config
        .insert("request".into(), serde_json::to_value(op).unwrap());
    assert!(
        f.host
            .publication(&context, &i.publication, &i.descriptor, &instance)
            .unwrap_err()
            .contains("grant")
    );
    for request in [
        json!({"endpoint": "http://elsewhere.invalid/api/records"}),
        json!({"headers": {"Authorization": {"source":"literal","value":"secret"}}}),
        json!({"path": [{"source":"literal","value":".."}]}),
        json!({"headers": {"X-Data": {"source":"literal","value":"bad\r\ninjected: yes"}}}),
        json!({"credential": "graph-secret"}),
        json!({"endpoint": {"source":"input","field":"initial","max_bytes":100}}),
    ] {
        let mut config = serde_json::to_value(operation(base, false)).unwrap();
        for (key, value) in request.as_object().unwrap() {
            config[key] = value.clone();
        }
        instance = i.instance.clone();
        instance.config.insert("request".into(), config);
        assert!(
            f.host
                .publication(&context, &i.publication, &i.descriptor, &instance)
                .is_err(),
            "accepted {request}"
        );
    }
    instance = i.instance.clone();
    instance
        .config
        .insert("connector".into(), json!({"plugin":"other"}));
    assert!(
        f.host
            .publication(&context, &i.publication, &i.descriptor, &instance)
            .unwrap_err()
            .contains("override")
    );
    let mut dynamic = operation(base, false);
    dynamic.headers.insert(
        "X-Token-Data".into(),
        BoundedValue::Input {
            field: "initial".into(),
            max_bytes: 1,
        },
    );
    instance = i.instance.clone();
    instance
        .config
        .insert("request".into(), serde_json::to_value(dynamic).unwrap());
    f.host
        .publication(&context, &i.publication, &i.descriptor, &instance)
        .unwrap();
    assert!(
        f.host
            .dispatch(
                &context,
                &i.publication,
                &i.descriptor,
                &instance,
                &BTreeMap::from([("initial".into(), json!(123))]),
                None
            )
            .unwrap_err()
            .contains("bound")
    );
    let effect = claimed(&f);
    f.granted.store(false, Ordering::Release);
    assert!(
        f.host
            .invoke_effect(effect)
            .await
            .unwrap_err()
            .contains("grant")
    );
    let mut c = contract(false);
    c.authentication = Some(crate::workflows::data_reference::DataReference::Workspace {
        id: "not-a-credential".into(),
    });
    assert!(c.validate().unwrap_err().contains("credential reference"));
}

// Catches: truncating pagination as successful output, repeating a cursor,
// or advancing a graph with a missing/wrong typed response.
#[tokio::test]
async fn pagination_and_response_failures_never_become_success() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(dir.path().into());
    for (bodies, max_pages, max_items, max_bytes, expected) in [
        (
            vec![
                json!({"items": [1], "next": "same"}),
                json!({"items": [2], "next": "same"}),
            ],
            3,
            4,
            4096,
            "repeated",
        ),
        (
            vec![json!({"items": [1], "next": "more"})],
            1,
            4,
            4096,
            "max-pages",
        ),
        (
            vec![json!({"items": [1, 2], "next": null})],
            3,
            1,
            4096,
            "max-items",
        ),
        (
            vec![json!({"items": [1], "next": 123})],
            3,
            4,
            4096,
            "cursor",
        ),
        (
            vec![json!({"items": [1], "next": null})],
            3,
            4,
            1,
            "byte bound",
        ),
    ] {
        let (state, base, server) = server(
            dir.path(),
            bodies.into_iter().map(|body| (200, body, None)).collect(),
        )
        .await;
        let mut c = contract(true);
        c.pagination.as_mut().unwrap().max_pages = max_pages;
        c.pagination.as_mut().unwrap().max_items = max_items;
        let mut op = operation(&base, true);
        op.max_response_bytes = max_bytes;
        let f = fixture(state, &base, op, c);
        let observation = f.host.invoke_effect(claimed(&f)).await.unwrap();
        assert!(observation.stopped);
        assert!(matches!(observation.outcome,
            EffectOutcome::Completed { result: NeutralResult::Failed { diagnostic } }
                if diagnostic.contains(expected)));
        server.await.unwrap();
    }
    for body in [json!({}), json!({"value":"wrong-type"})] {
        let (state, base, server) = server(dir.path(), vec![(200, body, None)]).await;
        let f = fixture(state, &base, operation(&base, false), contract(false));
        let observation = f.host.invoke_effect(claimed(&f)).await.unwrap();
        assert!(observation.stopped);
        assert!(matches!(
            observation.outcome,
            EffectOutcome::Completed {
                result: NeutralResult::Failed { .. }
            }
        ));
        server.await.unwrap();
    }
}

// Catches: replaying an accepted write during restart recovery, losing original
// callback correlation, or automatically retrying a rate-limited write.
#[tokio::test]
async fn asynchronous_and_rate_limited_writes_retain_original_effect_without_retry() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(dir.path().into());
    for status in [202, 429] {
        let (state, base, server) = server(dir.path(), vec![(status, json!({}), Some("12"))]).await;
        let mut c = contract(false);
        c.callback = Some(CallbackCorrelation {
            header: "X-Effect-Id".into(),
        });
        c.reconciliation = Reconciliation::OriginalEffectCallback;
        let mut op = operation(&base, false);
        op.method = "POST".into();
        let f = fixture(state, &base, op, c);
        let effect = claimed(&f);
        let observation = f.host.invoke_effect(effect.clone()).await.unwrap();
        assert!(!observation.stopped);
        if status == 202 {
            assert_eq!(
                observation.outcome,
                EffectOutcome::Pending {
                    handle: effect.key.digest.clone()
                }
            );
        } else {
            assert!(
                matches!(&observation.outcome, EffectOutcome::Uncertain { diagnostic } if diagnostic.contains("12") && diagnostic.contains("not repeated"))
            );
        }
        let retained = f
            .owner
            .neutral_effect_command(
                &effect.key.fence.run,
                EffectCommand::Observe {
                    key: effect.key.clone(),
                    observation,
                },
            )
            .unwrap();
        let retained = retained.effect(&effect.key).unwrap();
        assert!(!retained.settled());
        let recovered = f.host.reconcile(retained.clone()).await.unwrap();
        assert!(!recovered.stopped);
        assert!(matches!(recovered.outcome, EffectOutcome::Uncertain { .. }));
        let requests = server.await.unwrap();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].contains(&effect.key.digest));
        if status == 202 {
            // Existing callback transaction fences the original effect and
            // releases ownership only with actual completion/stop evidence.
            let observed = f
                .owner
                .neutral_effect_command(
                    &effect.key.fence.run,
                    EffectCommand::Observe {
                        key: effect.key.clone(),
                        observation: super::neutral_http::completed(BTreeMap::from([(
                            "value".into(),
                            json!(9),
                        )])),
                    },
                )
                .unwrap();
            assert!(observed.effect(&effect.key).unwrap().settled());
            assert!(
                matches!(&observed.effect(&effect.key).unwrap().observation.as_ref().unwrap().outcome, EffectOutcome::Completed { result: NeutralResult::Completed { outputs, .. } } if outputs["value"] == json!(9))
            );
        }
    }
}
