use crate::{
    automations::api,
    mcp_http::{build_router, tests::test_state},
};
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::ServiceExt;

// Catches: MCP keeping a separate parser/result contract from HTTP and CLI's MCP wire.
#[tokio::test]
async fn automation_mcp_http_cli_wire_cannot_drift() {
    use crate::mcp_http::mcp_transport::handle_mcp_tool_call;
    use axum::extract::ConnectInfo;
    let state = test_state();
    state.config.write().mcp_capabilities.denied.clear();
    state
        .mcp
        .to_session
        .insert("parity-cli".into(), "creator".into());
    let definitions = crate::automations::definitions::DefinitionStore::new();
    let definition = json!({"id":"parity","name":"Review","prompt":"Review repository",
        "run_config":"codex","repository":"/project","workspace":{"mode":"existing"},
        "cron":"0 9 * * *","timezone":"UTC","enabled":false,"grace_secs":43200,
        "overlap":"skip","max_duration_secs":3600});
    definitions
        .create(serde_json::from_value(definition.clone()).unwrap())
        .unwrap();
    let mut invalid = definition.clone();
    invalid["id"] = json!("invalid");
    invalid["prompt"] = json!(" ");
    for input in [
        json!({"action":"update","id":"absent","enabled":false}),
        json!({"action":"create","definition":invalid}),
        json!({"action":"update","definition":definition}),
        json!({"action":"update","id":"parity","enabled":false}),
        json!({"action":"get","id":"parity"}),
        json!({"action":"list","unexpected":true}),
        json!({"action":"invented"}),
        json!({"action":"list_runs","limit":"1"}),
        json!({"action":"preview","cron":"0 9 * * *","count":0}),
        json!({"action":"preview","cron":"invalid","timezone":"UTC"}),
        json!({"action":"preset","preset":{"kind":"daily","hour":9,"minute":30}}),
        json!({"action":"list"}),
        json!({"action":"get","id":"absent"}),
        json!({"action":"list_runs","limit":0}),
        json!({"action":"list_runs","limit":1}),
        json!({"action":"summary","window":"7d"}),
        json!({"action":"run","id":"absent"}),
        json!({"action":"run_now","id":"absent"}),
        json!({"action":"preview_definition","definition":{
            "id":"once","name":"Review","prompt":"Review repository","run_config":"codex",
            "repository":"/project","workspace":{"mode":"existing"},"cron":"",
            "once_local":"2099-10-10T09:00:00","timezone":"UTC","enabled":true,
            "grace_secs":43200,"overlap":"skip","max_duration_secs":3600}}),
    ] {
        let response = build_router(state.clone(), false, true)
            .oneshot(
                Request::post("/automations/action")
                    .header("content-type", "application/json")
                    .body(Body::from(json!({"input":input}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let http: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        let mcp = handle_mcp_tool_call(
            &state,
            "127.0.0.1:1234".parse().unwrap(),
            "automations",
            &input,
            Some("parity-cli"),
        )
        .await;
        // CLI's generic MCP command sends this exact tools/call envelope.
        let mut request = Request::post("/mcp")
            .header("content-type", "application/json")
            .header("mcp-session-id", "parity-cli")
            .body(Body::from(
                json!({"jsonrpc":"2.0","id":2,"method":"tools/call",
                "params":{"name":"automations","arguments":input}})
                .to_string(),
            ))
            .unwrap();
        request.extensions_mut().insert(ConnectInfo(
            "127.0.0.1:1234".parse::<std::net::SocketAddr>().unwrap(),
        ));
        let response = build_router(state.clone(), false, true)
            .oneshot(request)
            .await
            .unwrap();
        let envelope: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        let cli: Value = serde_json::from_str(
            envelope["result"]["content"][0]["text"]
                .as_str()
                .expect("CLI tool payload"),
        )
        .unwrap();
        assert_eq!(mcp, http, "MCP parity: {input}");
        assert_eq!(cli, http, "CLI wire parity: {input}");
    }
    definitions.remove("parity").unwrap();
}

// Catches: HTTP envelope drift, validation bypass, or dispatch on a non-owning backend.
#[tokio::test]
async fn automation_http_matches_shared_ipc_results_and_errors() {
    let state = test_state();
    for input in [
        json!({"action":"preset","preset":{"kind":"daily","hour":9,"minute":30}}),
        json!({"action":"preview","cron":"0 9 * * *","timezone":"UTC","count":0}),
        json!({"action":"list","unknown":true}),
        json!({"action":"run_now","id":"a"}),
    ] {
        let expected = api::execute(input.clone(), state.clone()).await;
        let response = build_router(state.clone(), false, true)
            .oneshot(
                Request::post("/automations/action")
                    .header("content-type", "application/json")
                    .body(Body::from(json!({"input": input}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        // Static repair errors are shared by HTTP and MCP; IPC retains its string
        // error signature. Semantic failures below still come from the shared API.
        if input.get("unknown").is_some() || input.get("count") == Some(&json!(0)) {
            assert!(expected.is_err());
            assert_eq!(status, StatusCode::BAD_REQUEST);
            assert_eq!(body["code"], "invalid_arguments");
            assert_eq!(body["effect"], "not_started");
            assert_eq!(body["repair"]["retry_after_correction"], true);
            assert_ne!(
                body["violations"].as_array().unwrap().as_slice(),
                [] as [serde_json::Value; 0]
            );
            continue;
        }
        match expected {
            Ok(value) => {
                assert_eq!(status, StatusCode::OK);
                assert_eq!(body, value);
                assert_eq!(body, json!({"cron":"30 9 * * *"}));
            }
            Err(error) => {
                assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
                assert_eq!(body, json!({"error": error}));
            }
        }
    }
}

async fn unknown_automation_json_404(router: axum::Router) {
    let response = router
        .oneshot(
            Request::post("/automations/missing?token=fallback-404-secret")
                .header("host", "127.0.0.1:9876")
                .extension(axum::extract::ConnectInfo(std::net::SocketAddr::from((
                    [127, 0, 0, 1],
                    5555,
                ))))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
            .expect("404 body is JSON");
    assert_eq!(
        body,
        json!({"error":"no such endpoint: /automations/missing"})
    );
}

fn state_with_token() -> std::sync::Arc<crate::state::AppState> {
    let state = test_state();
    *state.session_token.write() = "fallback-404-secret".to_string();
    state
}

// Catches: the headless TCP router (no SPA fallback) answering an unknown API path
// with an empty 404 instead of the documented JSON error.
#[tokio::test]
async fn headless_tcp_router_unknown_automation_endpoint_returns_json_404() {
    unknown_automation_json_404(crate::mcp_http::build_remote_router(state_with_token())).await;
}

// Catches: the local HTTP-over-IPC router losing the JSON 404 in a build without
// the desktop-only SPA fallback.
#[tokio::test]
async fn local_ipc_router_unknown_automation_endpoint_returns_json_404() {
    unknown_automation_json_404(build_router(state_with_token(), false, true)).await;
}
