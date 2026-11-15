//! Generated native command dispatch without the desktop setup closure or OS WebView.
use super::*;
use serde_json::{Value, json};

fn invoke(
    webview: &tauri::WebviewWindow<tauri::test::MockRuntime>,
    body: Value,
) -> Result<Value, Value> {
    tauri::test::get_ipc_response(
        webview,
        tauri::webview::InvokeRequest {
            cmd: "settings_action".into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            // Tauri's native local origin differs by platform (test module example).
            // The Windows spelling is remote content on macOS, correctly denied by ACL.
            url: if cfg!(any(windows, target_os = "android")) {
                "http://tauri.localhost"
            } else {
                "tauri://localhost"
            }
            .parse()
            .unwrap(),
            body: tauri::ipc::InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.to_owned(),
        },
    )
    .map(|body| body.deserialize().unwrap())
}

// Catches: generated IPC argument extraction dropping the request envelope or
// selecting different durable owner state from MCP/HTTP. No real window is opened.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn settings_generated_mock_ipc_agrees_with_mcp_and_http() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
    let state = test_state();
    state.config.write().settings_mcp_enabled = true;
    state.config.write().mcp_capabilities.denied.clear();
    let app = tauri::test::mock_builder()
        .manage(state.clone())
        .invoke_handler(tauri::generate_handler![
            crate::settings::api::settings_action
        ])
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .unwrap();
    let webview = tauri::WebviewWindowBuilder::new(&app, "settings-test", Default::default())
        .build()
        .unwrap();
    let read = json!({"action":"get","target":{"kind":"machine"},"store":"notifications","paths":["/volume"]});
    let initial = invoke(&webview, json!({"request":read})).unwrap();
    assert!(initial["revision"].is_string(), "{initial}");
    let patch = json!({"action":"patch","target":{"kind":"machine"},"store":"notifications",
        "expectedRevision":initial["revision"],"operations":[{"op":"set","path":"/volume","value":0.25}]});
    let mcp = handle_mcp_tool_call(
        &state,
        "127.0.0.1:12345".parse().unwrap(),
        "settings",
        &patch,
        None,
    )
    .await;
    assert_eq!(mcp["persisted"], true, "{mcp}");
    let ipc = invoke(&webview, json!({"request":read})).unwrap();
    assert_eq!(ipc["values"]["/volume"]["stored"], 0.25, "{ipc}");
    assert_eq!(
        settings_transport_request(&state, read.clone(), Some(12345)).await,
        ipc
    );
    assert_eq!(
        handle_mcp_tool_call(
            &state,
            "127.0.0.1:12345".parse().unwrap(),
            "settings",
            &read,
            None
        )
        .await,
        ipc
    );

    let patch = json!({"action":"patch","target":{"kind":"machine"},"store":"notifications",
        "expectedRevision":ipc["revision"],"operations":[{"op":"set","path":"/volume","value":0.5}]});
    let saved = invoke(&webview, json!({"request":patch})).unwrap();
    assert_eq!(saved["persisted"], true, "{saved}");
    let ipc = invoke(&webview, json!({"request":read})).unwrap();
    assert_eq!(ipc["values"]["/volume"]["stored"], 0.5, "{ipc}");
    assert_eq!(
        settings_transport_request(&state, read.clone(), Some(12345)).await,
        ipc
    );
    assert_eq!(
        handle_mcp_tool_call(
            &state,
            "127.0.0.1:12345".parse().unwrap(),
            "settings",
            &read,
            None
        )
        .await,
        ipc
    );
    assert_eq!(crate::config::load_notification_config().volume, 0.5);

    // A frontend sending the bare facade action instead of {request: action}
    // must fail argument extraction, not silently return or mutate another owner.
    let bytes = std::fs::read(dir.path().join("notifications.json")).unwrap();
    let malformed = invoke(&webview, patch).unwrap_err();
    assert!(
        malformed.as_str().unwrap().contains("request"),
        "{malformed}"
    );
    assert_eq!(
        std::fs::read(dir.path().join("notifications.json")).unwrap(),
        bytes
    );
    let invalid = invoke(&webview, json!({"request":{"action":"not-an-action"}})).unwrap();
    assert!(invalid["error"].is_object(), "{invalid}");
    assert_eq!(
        std::fs::read(dir.path().join("notifications.json")).unwrap(),
        bytes
    );

    // MCP opt-out is not native settings authority. Native UI still needs to
    // configure the opt-in while the MCP surface refuses access.
    state.config.write().settings_mcp_enabled = false;
    let denied = handle_mcp_tool_call(
        &state,
        "127.0.0.1:12345".parse().unwrap(),
        "settings",
        &read,
        None,
    )
    .await;
    assert!(denied.to_string().contains("disabled"), "{denied}");
    assert_eq!(invoke(&webview, json!({"request":read})).unwrap(), ipc);
}
