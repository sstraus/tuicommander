//! Headless Settings must use the same authenticated facade and SSE contract.
use super::*;
use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode, header},
};
use futures_util::StreamExt;
use serde_json::{Value, json};
use tower::ServiceExt;

fn request(method: &str, path: &str, body: Value) -> Request<Body> {
    let mut request = Request::builder()
        .method(method)
        .uri(format!(
            "{path}{}token=settings-fixture",
            if path.contains('?') { "&" } else { "?" }
        ))
        .header(header::HOST, "127.0.0.1:9876")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    request.extensions_mut().insert(ConnectInfo(
        "203.0.113.5:1234".parse::<std::net::SocketAddr>().unwrap(),
    ));
    request
}

async fn action(router: Router, body: Value) -> Value {
    let response = router
        .oneshot(request("POST", "/settings/action", body))
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "authenticated headless Settings action must not return 404"
    );
    serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap()
}

fn isolated() -> (tempfile::TempDir, impl Drop, Arc<AppState>) {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
    let state = tests::test_state();
    *state.session_token.write() = "settings-fixture".into();
    state.config.write().services.auth.lan_auth_bypass = false;
    (dir, guard, state)
}

// Catches: the headless router omitting the Settings facade even for authenticated clients.
#[tokio::test]
async fn remote_router_serves_settings_action() {
    let (_dir, _guard, state) = isolated();
    let router = build_remote_router(state);
    let described = action(router, json!({"action":"describe","store":"prompts"})).await;
    assert_eq!(described["inventoryComplete"], true, "{described}");
    assert!(
        described["fields"]
            .as_array()
            .is_some_and(|fields| !fields.is_empty())
    );
}

// Catches: a headless durable write failing to reach the value-free SSE publisher.
#[tokio::test]
async fn remote_settings_write_publishes_settings_changed() {
    let (_dir, _guard, state) = isolated();
    let router = build_remote_router(state.clone());
    // Read the revision through the owner so a missing HTTP route fails at the write itself.
    let initial = crate::settings::api::execute(
        &state,
        json!({"action":"get","target":{"kind":"instance"},"store":"prompts","paths":["/prompts"]}),
        crate::settings::policy::Caller::native(),
    )
    .await;
    let response = router
        .clone()
        .oneshot(request(
            "GET",
            "/events?types=settings-changed",
            Value::Null,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut stream = response.into_body().into_data_stream();
    let retry = stream.next().await.unwrap().unwrap();
    assert!(std::str::from_utf8(&retry).unwrap().contains("retry:"));
    let result = action(router.clone(), json!({"action":"patch","target":{"kind":"instance"},"store":"prompts",
        "expectedRevision":initial["revision"],"operations":[{"op":"create","path":"/prompts","id":"remote-refresh",
        "value":{"id":"remote-refresh","label":"Private label canary","text":"Private text canary","pinned":false}}]})).await;
    assert_eq!(result["persisted"], true, "{result}");
    let frame = tokio::time::timeout(std::time::Duration::from_secs(10), stream.next())
        .await
        .expect("persisted write never reached SSE")
        .unwrap()
        .unwrap();
    let text = std::str::from_utf8(&frame).unwrap();
    assert!(text.contains("event: settings-changed"), "{text}");
    let payload: Value = serde_json::from_str(
        text.lines()
            .find_map(|line| line.strip_prefix("data: "))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        payload,
        json!({"section":"prompts","generation":result["generation"],"persisted":true,"applied":true})
    );
    let stored = action(
        router,
        json!({"action":"get","target":{"kind":"instance"},"store":"prompts","paths":["/prompts"]}),
    )
    .await;
    assert_eq!(
        stored["values"]["/prompts"]["stored"][0]["text"],
        "Private text canary"
    );
}

// Catches: exposing desktop-only bridge/editor/OAuth workflows on a headless Settings surface.
#[tokio::test]
async fn remote_settings_refuses_local_only_workflows() {
    let (_dir, _guard, state) = isolated();
    let router = build_remote_router(state.clone());
    for operation in ["install_agent_mcp", "open_agent_config"] {
        let reply = action(router.clone(), json!({"action":"perform","target":{"kind":"machine"},"store":"agents","operation":{"action":operation}})).await;
        assert!(reply.get("error").is_some(), "{operation}: {reply}");
    }
    let reply = action(router, json!({"action":"perform","target":{"kind":"machine"},"store":"upstreams","operation":{"action":"begin_login","id":"not-contacted"}})).await;
    assert_eq!(
        reply["error"]["code"], "unavailable",
        "headless OAuth must be refused before resolving or contacting an upstream: {reply}"
    );
}

// Catches: moving the route into shared_routes accidentally bypassing daemon authentication.
#[tokio::test]
async fn remote_settings_requires_authentication() {
    let (_dir, _guard, state) = isolated();
    let router = build_remote_router(state);
    for (method, path) in [
        ("POST", "/settings/action"),
        ("GET", "/config"),
        ("PUT", "/config"),
    ] {
        let mut request = request(method, path, json!({"action":"describe"}));
        *request.uri_mut() = path.parse().unwrap();
        assert_eq!(
            router.clone().oneshot(request).await.unwrap().status(),
            StatusCode::UNAUTHORIZED,
            "{method} {path} must retain daemon authentication"
        );
    }
}

// Catches: browser Settings disabling persistence because headless GET /config is missing.
#[tokio::test]
async fn remote_router_serves_settings_bootstrap() {
    let (_dir, _guard, state) = isolated();
    {
        let mut config = state.config.write();
        config.default_font_size = 19;
        config.services.auth.password_hash = "bootstrap-password-canary".into();
        config.services.auth.session_token = "bootstrap-token-canary".into();
        config.services.push.vapid_private_key = "bootstrap-vapid-canary".into();
        config.services.relay.token = "bootstrap-relay-canary".into();
    }
    let response = build_remote_router(state)
        .oneshot(request("GET", "/config", Value::Null))
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "browser Settings bootstrap must not fail on a headless daemon"
    );
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let config: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(config["default_font_size"], 19);
    assert!(
        !std::str::from_utf8(&bytes).unwrap().contains("canary"),
        "shared bootstrap must retain secret redaction"
    );
}

async fn put_config(router: Router, body: Value) -> (StatusCode, Value) {
    let response = router
        .oneshot(request("PUT", "/config", body))
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn save_body(change: impl FnOnce(&mut crate::config::AppConfig)) -> Value {
    let base = crate::config::AppConfig::default();
    let mut config = base.clone();
    change(&mut config);
    json!({"base": base, "config": config})
}

// Catches: the raw config route writing owner-confirmed settings without the facade's approval.
#[tokio::test]
async fn put_config_refuses_owner_confirmed_settings() {
    type Change = fn(&mut crate::config::AppConfig);
    let changes: [(&str, Change); 4] = [
        ("ego_executable", |c| c.ego_executable = "/tmp/evil".into()),
        ("services.server.port", |c| c.services.server.port = 9981),
        ("services.auth.username", |c| {
            c.services.auth.username = "intruder".into()
        }),
        ("services.relay", |c| c.services.relay.enabled = true),
    ];
    for (name, change) in changes {
        let (_dir, _guard, state) = isolated();
        let before = serde_json::to_value(&*state.config.read()).unwrap();
        let (status, body) =
            put_config(build_remote_router(state.clone()), save_body(change)).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{name}: {body}");
        assert_eq!(body["error"], "confirmation_required", "{name}: {body}");
        assert_eq!(
            serde_json::to_value(&*state.config.read()).unwrap(),
            before,
            "{name}: refused write must leave the config unchanged"
        );
    }
}

// Catches: the guard refusing ordinary settings that never needed owner confirmation.
#[tokio::test]
async fn put_config_still_writes_ordinary_settings() {
    let (_dir, _guard, state) = isolated();
    let (status, body) = put_config(
        build_remote_router(state.clone()),
        save_body(|c| c.default_font_size = 23),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(state.config.read().default_font_size, 23);
}

// Catches: launcher executable/args edits slipping through because the delta replaces the whole array.
#[tokio::test]
async fn put_config_refuses_custom_launcher_execution_edits() {
    let (_dir, _guard, state) = isolated();
    let body = save_body(|c| {
        c.custom_launchers.push(crate::config::CustomLauncher {
            id: "x".into(),
            executable: "/tmp/evil".into(),
            ..Default::default()
        })
    });
    let (status, body) = put_config(build_remote_router(state.clone()), body).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert!(state.config.read().custom_launchers.is_empty());
}

// Catches: the raw route refusing a change that only removes authority, which the facade writes unasked.
#[tokio::test]
async fn put_config_writes_owner_confirmed_settings_that_only_reduce_authority() {
    let (_dir, _guard, state) = isolated();
    state.config.write().services.relay.enabled = true;
    let base = state.config.read().clone();
    let mut config = base.clone();
    config.services.relay.enabled = false;
    let (status, body) = put_config(
        build_remote_router(state.clone()),
        json!({"base": base, "config": config}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(!state.config.read().services.relay.enabled);
}

async fn send(router: Router, method: &str, path: &str, body: Value) -> (StatusCode, Value) {
    let response = router.oneshot(request(method, path, body)).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn stored_profile(state: &AppState) -> crate::tunnels::profile::TunnelProfile {
    let profile = crate::tunnels::profile::TunnelProfile::new("p", "example.com", "alice");
    crate::tunnels::storage::ProfileStore::save(&state.data_dir, &profile).unwrap();
    profile
}

fn stored_profiles(state: &AppState) -> Vec<Value> {
    let profiles = crate::tunnels::storage::ProfileStore::load_all(&state.data_dir, None).unwrap();
    profiles
        .iter()
        .map(|p| serde_json::to_value(p).unwrap())
        .collect()
}

// Catches: POST /tunnels/profiles writing a port, auto-connect or forward change that the
// facade gates on owner confirmation.
#[tokio::test]
async fn tunnel_profile_route_refuses_owner_confirmed_changes() {
    type Change = fn(&mut Value);
    let changes: [(&str, Change); 3] = [
        ("port", |p| p["port"] = json!(2222)),
        ("auto_connect", |p| p["auto_connect"] = json!(true)),
        (
            "forwards",
            |p| {
                p["forwards"] =
                    json!([{"type":"Local","bind_port":8080,"remote_host":"h","remote_port":80}])
            },
        ),
    ];
    for (name, change) in changes {
        let (_dir, _guard, state) = isolated();
        let stored = stored_profile(&state);
        let before = stored_profiles(&state);
        let mut next = serde_json::to_value(&stored).unwrap();
        change(&mut next);
        let (status, body) = send(
            build_remote_router(state.clone()),
            "POST",
            "/tunnels/profiles",
            next,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{name}: {body}");
        assert_eq!(body["error"], "confirmation_required", "{name}: {body}");
        assert_eq!(
            stored_profiles(&state),
            before,
            "{name}: refused write must not persist"
        );
    }
}

// Catches: a brand-new profile with forwards being created over HTTP without confirmation.
#[tokio::test]
async fn tunnel_profile_route_refuses_new_profile_with_forwards() {
    let (_dir, _guard, state) = isolated();
    let mut profile = serde_json::to_value(crate::tunnels::profile::TunnelProfile::new(
        "n",
        "example.com",
        "alice",
    ))
    .unwrap();
    profile["forwards"] =
        json!([{"type":"Local","bind_port":8080,"remote_host":"h","remote_port":80}]);
    let (status, body) = send(
        build_remote_router(state.clone()),
        "POST",
        "/tunnels/profiles",
        profile,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(stored_profiles(&state), [] as [Value; 0]);
}

// Catches: the tunnel guard refusing ordinary profile edits (rename) that never needed confirmation.
#[tokio::test]
async fn tunnel_profile_route_still_saves_ordinary_edits() {
    let (_dir, _guard, state) = isolated();
    let stored = stored_profile(&state);
    let mut next = serde_json::to_value(&stored).unwrap();
    next["name"] = json!("renamed");
    let (status, body) = send(
        build_remote_router(state.clone()),
        "POST",
        "/tunnels/profiles",
        next,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(stored_profiles(&state)[0]["name"], "renamed");
}

fn upstream(id: &str, transport: Value) -> Value {
    json!({"id": id, "name": id, "transport": transport, "enabled": true})
}

// Catches: PUT /mcp/upstreams adding or editing a stdio (process-spawning) upstream without
// the owner confirmation the facade requires.
#[tokio::test]
async fn upstream_route_refuses_stdio_changes() {
    let (_dir, _guard, state) = isolated();
    let body = json!({
        "base": {"servers": []},
        "config": {"servers": [upstream("evil", json!({"type":"stdio","command":"/bin/sh","args":["-c","id"]}))]},
    });
    let (status, resp) = send(
        build_router(state.clone(), true, true),
        "PUT",
        "/mcp/upstreams",
        body,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{resp}");
    assert!(
        crate::mcp_upstream_config::load_mcp_upstreams()
            .servers
            .is_empty(),
        "refused write must not persist"
    );
}

// Catches: the upstream guard refusing HTTP upstreams, which the facade does not gate.
#[tokio::test]
async fn upstream_route_still_saves_http_upstreams() {
    let (_dir, _guard, state) = isolated();
    let body = json!({
        "base": {"servers": []},
        "config": {"servers": [upstream("remote", json!({"type":"http","url":"https://example.com/mcp"}))]},
    });
    let (status, resp) = send(
        build_router(state.clone(), true, true),
        "PUT",
        "/mcp/upstreams",
        body,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{resp}");
    assert_eq!(
        crate::mcp_upstream_config::load_mcp_upstreams()
            .servers
            .len(),
        1
    );
}
