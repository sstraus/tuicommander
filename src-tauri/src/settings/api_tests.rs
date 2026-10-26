//! Transport cutover contracts; every file and credential slot belongs to this fixture.

// Catches: startup requiring ambient credential provenance that settings never publishes.
#[tokio::test]
async fn startup_uses_only_published_schema() {
    let (_dir, _guard, state) = super::accounts_tests::isolated();
    let described = api::execute(
        &state,
        json!({"action":"describe", "store":"github_accounts"}),
        Caller::mcp(
            "127.0.0.1:1234".parse().unwrap(),
            "startup-fixture".into(),
            true,
        ),
    )
    .await;
    assert_eq!(described["inventoryComplete"], true, "{described}");
    let provenance = described["fields"].as_array().unwrap().iter()
        .find(|field| field["path"] == "/auth/source")
        .expect("published settings cannot discover the GitHub owner's auth.source provenance required by startup");
    let read = api::execute(
        &state,
        json!({"action":"get", "store":provenance["store"], "target":{"kind":"machine"}, "paths":[provenance["path"]]}),
        Caller::mcp("127.0.0.1:1234".parse().unwrap(), "startup-fixture".into(), true),
    ).await;
    assert!(
        read["values"]["/auth/source"]["effective"].is_string(),
        "{read}"
    );
}
// Catches: the catalogue leaks auth.secret while publishing credential provenance.
#[tokio::test]
async fn published_github_provenance_cannot_leak_auth_secret() {
    let (_dir, _guard, state) = super::accounts_tests::isolated();
    *state.github.token.write() = Some("startup-token-canary".into());
    *state.github.token_source.write() = crate::github_auth::TokenSource::Env;
    let described = api::execute(
        &state,
        json!({"action":"describe","store":"github_accounts"}),
        Caller::native(),
    )
    .await;
    let fields = described["fields"].as_array().unwrap();
    assert!(!fields.iter().any(|field| {
        field["path"]
            .as_str()
            .is_some_and(|path| path.starts_with("/auth/secret"))
    }));
    let source = fields
        .iter()
        .find(|field| field["path"] == "/auth/source")
        .unwrap();
    assert_eq!(source["operations"], json!(["get"]));
    assert_eq!(source["sensitivity"], "public");
    let read = api::execute(&state, json!({"action":"get","target":{"kind":"machine"},"store":"github_accounts","paths":["/auth/source"]}), Caller::native()).await;
    assert_eq!(
        read["values"],
        json!({"/auth/source":{"stored":"env","effective":"env","source":"stored"}})
    );
    let denied = api::execute(&state, json!({"action":"get","target":{"kind":"machine"},"store":"github_accounts","paths":["/auth/secret"]}), Caller::native()).await;
    assert_eq!(denied["error"]["code"], "unknown_path");
    for reply in [described, read, denied] {
        assert!(!reply.to_string().contains("startup-token-canary"));
        assert!(!reply.to_string().contains("credential_ref"));
    }
}

use super::{api, policy::Caller};
use crate::remote_connection::{RemoteConnection, RemoteConnectionStore};
use serde_json::{Value, json};

fn action(id: &str, operation: &str) -> Value {
    json!({"action":"perform","target":{"kind":"machine"},"store":"connections",
        "operation":{"action":operation,"id":id,"password":"cutover-secret-canary"}})
}

// Catches: flattened typed workflow schemas accepting fields that execution silently ignores.
#[test]
fn typed_owner_envelopes_decode_and_reject_unknown_fields() {
    for request in [
        action("one", "set_password"),
        json!({"action":"perform","target":{"kind":"machine"},"store":"telegram",
            "operation":{"action":"add_chat","chat_id":"123"}}),
        json!({"action":"perform","target":{"kind":"machine"},"store":"upstreams",
            "operation":{"action":"clear","id":"one","header":null}}),
    ] {
        assert!(
            serde_json::from_value::<api::Action>(request.clone()).is_ok(),
            "{request}"
        );
        let mut extra = request.clone();
        extra["confirmed"] = json!(true);
        assert!(serde_json::from_value::<api::Action>(extra).is_err());
        let mut wrong_owner = request;
        wrong_owner["store"] = json!("prompts");
        assert!(serde_json::from_value::<api::Action>(wrong_owner).is_err());
    }
}

// Catches: local credential writes being reached through a foreign target or remote MCP origin.
#[tokio::test]
async fn connection_credential_dispatch_rejects_foreign_target_before_write() {
    let (_dir, _guard, state) = super::accounts_tests::isolated();
    let connection = RemoteConnection::new_ssh("fixture", "not-contacted.example", "fixture");
    RemoteConnectionStore::save(&state.data_dir, std::slice::from_ref(&connection)).unwrap();
    let mut request = action(&connection.id, "set_password");
    request["target"] = json!({"kind":"repository","id":"remote-target"});
    let result = api::execute(&state, request, Caller::native()).await;
    assert_eq!(result["error"]["code"], "not_found", "{result}");
    let remote = Caller::mcp("192.0.2.1:1234".parse().unwrap(), "fixture".into(), true);
    let result = api::execute(&state, action(&connection.id, "set_password"), remote).await;
    assert_eq!(result["error"]["code"], "forbidden_origin", "{result}");
    assert!(!crate::remote_connection::connection_password_exists(&connection.id).unwrap());
    assert_eq!(
        state.remote.snapshot(),
        [] as [crate::remote_runtime::RemoteConnectionStatus; 0]
    );
}

// Catches: credential rotation retaining an old runtime token or reconnecting a sibling target.
#[tokio::test]
async fn connection_credential_dispatch_reconciles_only_addressed_runtime() {
    let (_dir, _guard, state) = super::accounts_tests::isolated();
    let requested = std::sync::Arc::new(tokio::sync::Notify::new());
    let notify = requested.clone();
    // A controlled refusal proves the real reconnect path ran without an external service.
    let router = axum::Router::new().route(
        "/health",
        axum::routing::get(move || {
            let notify = notify.clone();
            async move {
                notify.notify_one();
                axum::http::StatusCode::SERVICE_UNAVAILABLE
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let a = RemoteConnection::new_direct("changed", &url, "fixture");
    let b = RemoteConnection::new_direct("sibling", &url, "fixture");
    RemoteConnectionStore::save(&state.data_dir, &[a.clone(), b.clone()]).unwrap();
    state
        .remote
        .force_connected_for_test(&a.id, &url, Some("retired-token"));
    state
        .remote
        .force_connected_for_test(&b.id, &url, Some("sibling-token"));
    let before = std::fs::read(state.data_dir.join("connections.json")).unwrap();
    let result = api::execute(&state, action(&a.id, "set_password"), Caller::native()).await;
    assert_eq!(result["password"]["status"], "set", "{result}");
    tokio::time::timeout(std::time::Duration::from_secs(30), requested.notified())
        .await
        .expect("credential reconciliation never reached the fixture daemon");
    let statuses = state.remote.snapshot();
    assert!(
        statuses
            .iter()
            .find(|entry| entry.id == a.id)
            .unwrap()
            .token
            .is_none()
    );
    assert_eq!(
        statuses
            .iter()
            .find(|entry| entry.id == b.id)
            .unwrap()
            .token
            .as_deref(),
        Some("sibling-token")
    );
    assert_eq!(
        std::fs::read(state.data_dir.join("connections.json")).unwrap(),
        before
    );
    let read = json!({"action":"get","target":{"kind":"machine"},"store":"connections",
        "paths":[format!("/{}/credential", a.id)]});
    let result = api::execute(&state, read, Caller::native()).await;
    assert_eq!(
        result["values"][format!("/{}/credential", a.id)]["stored"]["password"]["status"],
        "set",
        "{result}"
    );
    assert!(!result.to_string().contains("cutover-secret-canary"));
    crate::remote_runtime::teardown(&state, &a.id);
    crate::remote_runtime::teardown(&state, &b.id);
    server.abort();
}

// Catches: clearing a disconnected connection's credential implicitly connecting it.
#[tokio::test]
async fn connection_credential_dispatch_clear_keeps_disconnected_owner_down() {
    let (_dir, _guard, state) = super::accounts_tests::isolated();
    let connection = RemoteConnection::new_ssh("disconnected", "not-contacted.example", "fixture");
    RemoteConnectionStore::save(&state.data_dir, std::slice::from_ref(&connection)).unwrap();
    let result = api::execute(
        &state,
        action(&connection.id, "set_password"),
        Caller::native(),
    )
    .await;
    assert_eq!(result["password"]["status"], "set", "{result}");
    let request = json!({"action":"perform","target":{"kind":"machine"},"store":"connections",
        "operation":{"action":"clear_password","id":connection.id}});
    let result = api::execute(&state, request, Caller::native()).await;
    assert_eq!(result["password"]["status"], "unset", "{result}");
    assert_eq!(
        state.remote.snapshot(),
        [] as [crate::remote_runtime::RemoteConnectionStatus; 0]
    );
}

// Catches: new typed stores being omitted from the persistent shared registry.
#[tokio::test]
async fn shared_registry_reaches_registration_and_ui_owners() {
    let (dir, _guard, state) = super::accounts_tests::isolated();
    std::fs::write(
        dir.path().join("keybindings.json"),
        r#"[{"action":"toggle-sidebar","key":"Cmd+B"}]"#,
    )
    .unwrap();
    for (store, path) in [
        ("repositories_registration", "/repoOrder"),
        ("keybindings", "/bindings/toggle-sidebar/key"),
        ("pane_layout", "/root"),
    ] {
        let result = api::execute(
            &state,
            json!({"action":"get","target":{"kind":"instance"},"store":store,"paths":[path]}),
            Caller::native(),
        )
        .await;
        assert!(result["revision"].is_string(), "{store}: {result}");
        assert!(result["values"][path].is_object(), "{store}: {result}");
    }
}

// Catches: presence-only enum strings being treated as nonempty credentials (unset becomes set).
#[test]
fn owner_presence_projection_preserves_unset_and_unavailable() {
    let catalogue = super::native::native_catalogue().unwrap();
    let path = "/accounts/fixture/credential";
    for status in ["set", "unset", "unavailable"] {
        let result = super::service::snapshot(
            &catalogue,
            "github_accounts",
            vec![path.into()],
            json!({"accounts":[{"id":"fixture","credential":status}]}),
        );
        if status == "unavailable" {
            assert_eq!(result.unwrap_err().code, "secret_unreadable");
        } else {
            assert_eq!(
                result.unwrap().values[path].stored,
                json!({"status":status})
            );
        }
    }
}

// Catches: the shared facade reading core account wire fields against the GitHub-specific schema.
#[tokio::test]
async fn github_catalogue_paths_use_owner_projection() {
    let (_dir, _guard, state) = super::accounts_tests::isolated();
    let account = crate::github_account::GitHubAccount::ghe_pat(
        crate::github_account::GitHubHost::new("fixture.example").unwrap(),
        None,
    );
    crate::github_account::add_account_record(account.clone(), None).unwrap();
    let host = format!("/accounts/{}/host/host", account.id);
    let credential = format!("/accounts/{}/credential", account.id);
    let result = api::execute(
        &state,
        json!({"action":"get","target":{"kind":"machine"},
        "store":"github_accounts","paths":[host,credential]}),
        Caller::native(),
    )
    .await;
    assert_eq!(
        result["values"][&host]["stored"], "fixture.example",
        "{result}"
    );
    assert_eq!(
        result["values"][&credential]["stored"],
        json!({"status":"unset"}),
        "{result}"
    );
}

// Catches: removing the config tool also removing stable-id prompt editing/readback.
#[tokio::test]
async fn settings_prompt_crud_survives_config_removal() {
    let (_dir, _guard, state) = super::accounts_tests::isolated();
    let get =
        json!({"action":"get","target":{"kind":"instance"},"store":"prompts","paths":["/prompts"]});
    let initial = api::execute(&state, get.clone(), Caller::native()).await;
    assert_eq!(
        initial["values"]["/prompts"]["stored"],
        json!([]),
        "{initial}"
    );
    let create = json!({"action":"patch","target":{"kind":"instance"},"store":"prompts",
        "expectedRevision":initial["revision"],"operations":[{"op":"create","path":"/prompts","id":"fixture",
        "value":{"id":"fixture","label":"One","text":"First","pinned":false}}]});
    let remote = Caller::mcp("192.0.2.1:1234".parse().unwrap(), "remote".into(), true);
    let refused = api::execute(&state, create.clone(), remote).await;
    assert_eq!(refused["error"]["code"], "forbidden_origin");
    let created = api::execute(&state, create, Caller::native()).await;
    assert_eq!(created["persisted"], true, "{created}");
    let edit = json!({"action":"patch","target":{"kind":"instance"},"store":"prompts",
        "expectedRevision":created["revision"],"operations":[{"op":"set","path":"/prompts/fixture/text","value":"Second"},
        {"op":"set","path":"/prompts/fixture/pinned","value":true}]});
    let edited = api::execute(&state, edit, Caller::native()).await;
    assert_eq!(edited["persisted"], true, "{edited}");
    let read = api::execute(&state, get.clone(), Caller::native()).await;
    assert_eq!(
        read["values"]["/prompts"]["stored"],
        json!([{"id":"fixture","label":"One","text":"Second","pinned":true}])
    );
    let remove = json!({"action":"patch","target":{"kind":"instance"},"store":"prompts",
        "expectedRevision":read["revision"],"operations":[{"op":"remove","path":"/prompts","id":"fixture"}]});
    let removed = api::execute(&state, remove, Caller::native()).await;
    assert_eq!(removed["persisted"], true, "{removed}");
    let empty = api::execute(&state, get, Caller::native()).await;
    assert_eq!(empty["values"]["/prompts"]["stored"], json!([]));
    let missing = api::execute(&state, json!({"action":"get","target":{"kind":"instance"},"store":"prompts","paths":["/prompts/fixture/text"]}), Caller::native()).await;
    assert_eq!(missing["error"]["code"], "not_found");
}

// Catches: a durable MCP settings commit never waking desktop/browser readers.
#[tokio::test]
async fn mcp_write_publishes_value_free_settings_change() {
    let (_dir, _guard, state) = super::accounts_tests::isolated();
    let get =
        json!({"action":"get","target":{"kind":"instance"},"store":"prompts","paths":["/prompts"]});
    let initial = api::execute(&state, get, Caller::native()).await;
    let mut events = state.event_bus.subscribe();
    let result = api::execute(&state, json!({"action":"patch","target":{"kind":"instance"},"store":"prompts",
        "expectedRevision":initial["revision"],"operations":[{"op":"create","path":"/prompts","id":"refresh-fixture",
        "value":{"id":"refresh-fixture","label":"Private label canary","text":"Private text canary","pinned":false}}]}),
        Caller::mcp("127.0.0.1:1234".parse().unwrap(), "refresh-fixture".into(), true)).await;
    assert_eq!(result["persisted"], true, "{result}");
    let event = events
        .try_recv()
        .expect("durable MCP settings commit did not publish a change");
    let event = serde_json::to_value(event).unwrap();
    assert_eq!(event["event"], "settings-changed");
    assert_eq!(event["payload"]["section"], "prompts");
    assert_eq!(event["payload"]["generation"], result["generation"]);
    assert!(event["payload"]["generation"].as_u64().unwrap() > 0);
    assert_eq!(event["payload"]["persisted"], true);
    assert_eq!(event["payload"]["applied"], true);
    assert!(!event.to_string().contains("Private label canary"));
    assert!(!event.to_string().contains("Private text canary"));
    assert!(
        events.try_recv().is_err(),
        "one commit must publish one hint"
    );
}

// Catches: vault-only writes reusing a metadata revision and never invalidating credential badges.
#[tokio::test]
async fn vault_only_writes_advance_settings_generation_without_metadata_changes() {
    let (_dir, _guard, state) = super::accounts_tests::isolated();
    let connection = RemoteConnection::new_ssh("fixture", "not-contacted.example", "fixture");
    RemoteConnectionStore::save(&state.data_dir, std::slice::from_ref(&connection)).unwrap();
    let metadata = std::fs::read(state.data_dir.join("connections.json")).unwrap();
    let mut events = state.event_bus.subscribe();
    let first = api::execute(
        &state,
        action(&connection.id, "set_password"),
        Caller::native(),
    )
    .await;
    let second = api::execute(
        &state,
        json!({"action":"perform","target":{"kind":"machine"},"store":"connections",
        "operation":{"action":"clear_password","id":connection.id}}),
        Caller::native(),
    )
    .await;
    assert!(first["generation"].as_u64().unwrap() < second["generation"].as_u64().unwrap());
    assert_eq!(first["persisted"], true);
    assert_eq!(second["persisted"], true);
    assert_eq!(
        std::fs::read(state.data_dir.join("connections.json")).unwrap(),
        metadata
    );
    let changes: Vec<_> = std::iter::from_fn(|| events.try_recv().ok())
        .filter_map(|event| match event {
            crate::state::AppEvent::SettingsChanged(notice) => Some(notice),
            _ => None,
        })
        .collect();
    assert_eq!(changes.len(), 2);
    assert!(
        changes
            .iter()
            .all(|notice| notice.section == "connections" && notice.persisted)
    );
    assert!(
        !serde_json::to_string(&changes)
            .unwrap()
            .contains("cutover-secret-canary")
    );
}

// Catches: partial persistence being reported as a failed write with no refresh hint.
#[cfg(not(feature = "desktop"))]
#[tokio::test]
async fn persisted_apply_failure_publishes_unapplied_generation() {
    let (dir, _guard, state) = super::accounts_tests::isolated();
    let get = json!({"action":"get","target":{"kind":"machine"},"store":"app","paths":["/services/server/port"]});
    let initial = api::execute(&state, get.clone(), Caller::native()).await;
    let mut edit = json!({"action":"set","target":{"kind":"machine"},"store":"app","path":"/services/server/port",
        "value":9981,"expectedRevision":initial["revision"]});
    let mut events = state.event_bus.subscribe();
    let refused = api::execute(&state, edit.clone(), Caller::native()).await;
    assert_eq!(
        refused["error"]["code"], "confirmation_required",
        "{refused}"
    );
    assert!(events.try_recv().is_err());
    let id = refused["error"]["proposalId"].as_str().unwrap();
    state
        .settings_policy
        .begin_confirmation(&Caller::native(), id)
        .unwrap();
    state
        .settings_policy
        .finish_confirmation(&Caller::native(), id, true)
        .unwrap();
    edit["approval"] = json!(id);
    let failed = api::execute(&state, edit, Caller::native()).await;
    assert_eq!(failed["error"]["code"], "apply_failed", "{failed}");
    assert_eq!(failed["error"]["persisted"], true);
    let event = events.try_recv().expect("partial commit must wake readers");
    let event = serde_json::to_value(event).unwrap();
    assert_eq!(
        event["payload"]["generation"],
        failed["error"]["generation"]
    );
    assert_eq!(event["payload"]["applied"], false);
    assert_eq!(event["payload"]["persisted"], true);
    assert_eq!(failed["error"]["nextAction"], "get");
    assert_eq!(failed["error"]["retryable"], false);
    // The durable write already happened: recover with get, not a second set.
    let readback = api::execute(&state, get, Caller::native()).await;
    assert_eq!(readback["values"]["/services/server/port"]["stored"], 9981);
    assert_ne!(readback["revision"], initial["revision"]);
    let disk: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("config.json")).unwrap()).unwrap();
    assert_eq!(disk["services"]["server"]["port"], 9981);
    assert!(
        events.try_recv().is_err(),
        "readback must not replay the mutation"
    );
}

// Catches: refused or unchanged requests waking readers as if they durably changed settings.
#[tokio::test]
async fn rejected_and_noop_settings_edits_publish_nothing() {
    let (dir, _guard, state) = super::accounts_tests::isolated();
    std::fs::write(
        dir.path().join("prompt-library.json"),
        json!({"prompts":[{"id":"fixture","label":"Fixture","text":"unchanged","pinned":false}]})
            .to_string(),
    )
    .unwrap();
    let initial = api::execute(
        &state,
        json!({"action":"get","target":{"kind":"instance"},
        "store":"prompts","paths":["/prompts"]}),
        Caller::native(),
    )
    .await;
    let mut events = state.event_bus.subscribe();
    let rejected = api::execute(
        &state,
        json!({"action":"set","target":{"kind":"instance"},
        "store":"prompts","expectedRevision":initial["revision"],"path":"/unknown","value":true}),
        Caller::native(),
    )
    .await;
    assert!(rejected.get("error").is_some(), "{rejected}");
    assert!(events.try_recv().is_err());
    let unchanged = api::execute(
        &state,
        json!({"action":"set","target":{"kind":"instance"},
        "store":"prompts","expectedRevision":initial["revision"],"path":"/prompts/fixture/text","value":"unchanged"}),
        Caller::native(),
    )
    .await;
    assert_eq!(unchanged["persisted"], false, "{unchanged}");
    assert!(events.try_recv().is_err());
}

// Catches: registered plugin shortcuts staying unavailable, or unloaded shortcuts remaining selectable.
#[tokio::test]
async fn frontend_plugin_actions_reach_keybinding_owner_choices() {
    let (_dir, _guard, state) = super::accounts_tests::isolated();
    let refresh = json!({"action":"refresh","target":{"kind":"instance"},"store":"keybindings",
        "paths":["/bindings/terminal-copy/action"]});
    let mut events = state.event_bus.subscribe();
    let publish = api::execute(
        &state,
        json!({"action":"publish_plugin_keybinding_actions",
        "actions":["plugin:fixture:run"]}),
        Caller::native(),
    )
    .await;
    assert_eq!(publish["persisted"], false, "{publish}");
    let choices = api::execute(&state, refresh.clone(), Caller::native()).await;
    let values = choices["choices"]["/bindings/terminal-copy/action"]["values"]
        .as_array()
        .expect("available owner actions");
    assert!(values.contains(&json!("plugin:fixture:run")), "{choices}");
    assert!(
        values.contains(&json!(super::keybindings::actions()[0])),
        "{choices}"
    );
    let clear = api::execute(
        &state,
        json!({"action":"publish_plugin_keybinding_actions","actions":[]}),
        Caller::native(),
    )
    .await;
    assert_eq!(clear["applied"], true);
    let removed = api::execute(&state, refresh, Caller::native()).await;
    assert!(
        !removed["choices"]["/bindings/terminal-copy/action"]["values"]
            .as_array()
            .unwrap()
            .contains(&json!("plugin:fixture:run"))
    );
    assert!(
        events.try_recv().is_err(),
        "runtime registrations must not look like durable settings edits"
    );
}
