//! `httpFetch({account})`: credential injection, host rule, consent and redaction.
//! Local servers speak our generic descriptor protocol, not a provider fixture.
use super::plugin_http_fetch_impl;
use crate::accounts::{AuthKind, NewAccount};
use std::sync::{Arc, Mutex};

pub(super) type Seen = Arc<Mutex<Vec<(String, Option<String>)>>>;

pub(super) fn isolated_config() -> (tempfile::TempDir, impl Drop) {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
    (dir, guard)
}

/// Records every request with its Authorization header. `/identity` answers the
/// account verification, `/echo` reflects the header back in the body and in a
/// response header, `/redirect` answers 302 to `redirect_to`.
pub(super) async fn server(prefix: &str, redirect_to: Option<String>) -> (String, Seen) {
    let seen: Seen = Arc::default();
    let record = seen.clone();
    let app = axum::Router::new().fallback(
        move |headers: axum::http::HeaderMap, uri: axum::http::Uri| {
            let record = record.clone();
            let redirect_to = redirect_to.clone();
            async move {
                use axum::response::IntoResponse;
                let auth = headers
                    .get("authorization")
                    .map(|v| v.to_str().unwrap().to_string());
                record
                    .lock()
                    .unwrap()
                    .push((uri.path().to_string(), auth.clone()));
                let auth = auth.unwrap_or_default();
                match uri.path().rsplit('/').next().unwrap() {
                    "identity" => {
                        axum::Json(serde_json::json!({"name":"a3c-user"})).into_response()
                    }
                    "echo" => ([("x-echo", auth.clone())], format!("seen: {auth}")).into_response(),
                    "redirect" => (
                        axum::http::StatusCode::FOUND,
                        [("location", redirect_to.unwrap())],
                    )
                        .into_response(),
                    _ => "ok".into_response(),
                }
            }
        },
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}{prefix}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (base, seen)
}

pub(super) fn write_manifest(
    dir: &std::path::Path,
    id: &str,
    providers: serde_json::Value,
    urls: &[String],
) {
    let plugin_dir = dir.join("plugins").join(id);
    std::fs::create_dir_all(&plugin_dir).unwrap();
    let manifest = serde_json::json!({
        "id": id, "name": id, "version": "1.0.0", "minAppVersion": "0.0.0",
        "main": "main.js", "capabilities": ["net:http"], "allowedUrls": urls,
        "providers": providers,
    });
    std::fs::write(
        plugin_dir.join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
}

fn descriptor(id: &str) -> serde_json::Value {
    serde_json::json!([{
        "id": id, "label": "A3c protocol test provider", "kinds": ["bearer", "basic"],
        "verify": {"path": "/identity", "loginPointer": "/name"}
    }])
}

/// Load the plugin through the production registration path, which registers
/// its declared descriptors.
pub(super) fn load(state: &crate::AppState, id: &str) {
    crate::plugins::register_loaded_plugin_impl(state, id.into(), vec!["net:http".into()]).unwrap();
}

async fn account(provider: &str, host: &str, kind: AuthKind, token: &str) -> String {
    let username = (kind == AuthKind::Basic).then(|| "user@example.test".to_string());
    let input = NewAccount {
        provider: provider.into(),
        host: host.into(),
        kind,
        label: "A3c account".into(),
        username,
    };
    crate::accounts::add_account(input, token.into())
        .await
        .unwrap()
        .account
        .id
}

async fn fetch(
    state: &Arc<crate::AppState>,
    plugin: &str,
    url: String,
    headers: Option<std::collections::HashMap<String, String>>,
    account: &str,
) -> Result<super::HttpResponse, String> {
    plugin_http_fetch_impl(
        state,
        url,
        None,
        headers,
        None,
        Some(account.into()),
        plugin.into(),
    )
    .await
}

// Catches: token leak to a redirect target on another host.
#[tokio::test]
async fn plugin_account_secret_never_follows_foreign_host() {
    let (dir, _guard) = isolated_config();
    let (target, target_seen) = server("", None).await;
    // Same port family, different host string: `localhost` is not `127.0.0.1`.
    let target = target.replace("127.0.0.1", "localhost");
    let (base, seen) = server("/site", Some(format!("{target}/capture"))).await;
    write_manifest(
        dir.path(),
        "a3c-redirect",
        descriptor("a3c-redirect"),
        &[format!("{target}/*")],
    );
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    load(&state, "a3c-redirect");
    let id = account(
        "a3c-redirect",
        &base,
        AuthKind::Bearer,
        "a3c-redirect-token",
    )
    .await;

    let response = fetch(
        &state,
        "a3c-redirect",
        format!("{base}/redirect"),
        None,
        &id,
    )
    .await
    .unwrap();

    assert_eq!(response.status, 200);
    assert_eq!(
        seen.lock().unwrap().last().unwrap(),
        &(
            "/site/redirect".to_string(),
            Some("Bearer a3c-redirect-token".into())
        ),
        "the account host must receive the injected credential"
    );
    assert_eq!(
        *target_seen.lock().unwrap(),
        vec![("/capture".to_string(), None)],
        "the redirect target on another host must not receive Authorization"
    );
}

// Catches: a plugin sends a Jira token to another host, or to a sibling path on
// the same origin, because the URL was checked only against allowedUrls.
#[tokio::test]
async fn url_outside_account_host_rule_rejected() {
    let (dir, _guard) = isolated_config();
    let (other, other_seen) = server("", None).await;
    let (base, seen) = server("/jira", None).await;
    let origin = base.trim_end_matches("/jira").to_string();
    // The allowlist admits both, so only the account host rule can refuse.
    write_manifest(
        dir.path(),
        "a3c-hostrule",
        descriptor("a3c-hostrule"),
        &[format!("{other}/*"), format!("{origin}/*")],
    );
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    load(&state, "a3c-hostrule");
    let id = account(
        "a3c-hostrule",
        &base,
        AuthKind::Bearer,
        "a3c-hostrule-token",
    )
    .await;
    let verified = seen.lock().unwrap().len();

    for url in [
        format!("{other}/steal"),
        format!("{origin}/jira-evil/steal"),
        format!("{origin}/steal"),
        format!("{base}/../steal"),
    ] {
        let err = fetch(&state, "a3c-hostrule", url.clone(), None, &id)
            .await
            .unwrap_err();
        assert!(err.contains("outside the account host"), "{url}: {err}");
    }
    assert!(
        other_seen.lock().unwrap().is_empty(),
        "other host was contacted"
    );
    assert_eq!(
        seen.lock().unwrap().len(),
        verified,
        "a refused URL reached the site"
    );
}

// Catches: manifest consent bypass — a plugin that never declared the provider
// borrows another plugin's account by id.
#[tokio::test]
async fn plugin_cannot_use_undeclared_provider() {
    let (dir, _guard) = isolated_config();
    let (base, seen) = server("", None).await;
    write_manifest(
        dir.path(),
        "a3c-owner",
        descriptor("a3c-consent"),
        &[format!("{base}/*")],
    );
    write_manifest(
        dir.path(),
        "a3c-borrower",
        serde_json::json!([]),
        &[format!("{base}/*")],
    );
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    load(&state, "a3c-owner");
    load(&state, "a3c-borrower");
    let id = account("a3c-consent", &base, AuthKind::Bearer, "a3c-consent-token").await;
    let verified = seen.lock().unwrap().len();

    let err = fetch(&state, "a3c-borrower", format!("{base}/echo"), None, &id)
        .await
        .unwrap_err();

    assert!(err.contains("does not declare account provider"), "{err}");
    assert_eq!(seen.lock().unwrap().len(), verified, "the request was sent");
}

// Catches: the token echoed back to JS in the body or a response header, for
// both the raw token and the Basic credential encoding.
#[tokio::test]
async fn response_never_contains_authorization() {
    use base64::Engine as _;
    let (dir, _guard) = isolated_config();
    let (base, seen) = server("", None).await;
    write_manifest(
        dir.path(),
        "a3c-echo",
        descriptor("a3c-echo"),
        &[format!("{base}/*")],
    );
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    load(&state, "a3c-echo");

    for (kind, token) in [
        (AuthKind::Bearer, "a3c-echo-bearer-token"),
        (AuthKind::Basic, "a3c-echo-basic-token"),
    ] {
        let id = account("a3c-echo", &base, kind, token).await;
        let response = fetch(&state, "a3c-echo", format!("{base}/echo"), None, &id)
            .await
            .unwrap();
        let sent = seen.lock().unwrap().last().unwrap().1.clone().unwrap();
        let encoded =
            base64::engine::general_purpose::STANDARD.encode(format!("user@example.test:{token}"));
        let expected = match kind {
            AuthKind::Basic => format!("Basic {encoded}"),
            _ => format!("Bearer {token}"),
        };
        assert_eq!(sent, expected, "credential was not injected");
        let reply = format!("{} {:?}", response.body, response.headers);
        assert!(response.body.starts_with("seen: "), "{reply}");
        assert!(!reply.contains(token), "token reached JS: {reply}");
        assert!(
            !reply.contains(&encoded),
            "Basic credential reached JS: {reply}"
        );
    }
}

// Catches: a plugin header overriding or duplicating the injected credential.
#[tokio::test]
async fn caller_authorization_header_rejected_for_account_request() {
    let (dir, _guard) = isolated_config();
    let (base, seen) = server("", None).await;
    write_manifest(
        dir.path(),
        "a3c-header",
        descriptor("a3c-header"),
        &[format!("{base}/*")],
    );
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    load(&state, "a3c-header");
    let id = account("a3c-header", &base, AuthKind::Bearer, "a3c-header-token").await;
    let verified = seen.lock().unwrap().len();
    let headers = std::collections::HashMap::from([("authorization".into(), "Bearer x".into())]);

    let err = fetch(
        &state,
        "a3c-header",
        format!("{base}/echo"),
        Some(headers),
        &id,
    )
    .await
    .unwrap_err();

    assert!(err.contains("must not set Authorization"), "{err}");
    assert_eq!(seen.lock().unwrap().len(), verified);
}

// Catches: a descriptor outliving its plugin, so a new account can still be
// created for a provider nobody provides.
#[tokio::test]
async fn unloading_a_plugin_unregisters_its_providers() {
    let (dir, _guard) = isolated_config();
    write_manifest(
        dir.path(),
        "a3c-unload",
        descriptor("a3c-unload"),
        &["https://a3c.example.test/*".into()],
    );
    let state = crate::state::tests_support::make_test_app_state();
    let input = NewAccount {
        provider: "a3c-unload".into(),
        host: "https://a3c.example.test".into(),
        kind: AuthKind::Bearer,
        label: "unload".into(),
        username: None,
    };
    load(&state, "a3c-unload");
    crate::accounts::precheck_new(&input).unwrap();

    crate::plugins::unregister_loaded_plugin_impl(&state, "a3c-unload");

    let err = crate::accounts::precheck_new(&input).unwrap_err();
    assert!(err.contains("unavailable"), "{err}");
}

// Catches: a plugin shadowing the core GitHub descriptor, referencing a core
// provider that does not exist, or using accounts without net:http consent.
#[test]
fn manifest_provider_entries_are_validated() {
    let (dir, _guard) = isolated_config();
    let github = serde_json::json!([{
        "id": "github", "label": "Fake", "kinds": ["bearer"],
        "verify": {"path": "/user", "loginPointer": "/login"}
    }]);
    let urls = ["https://a3c.example.test/*".to_string()];
    for (id, providers, expected) in [
        ("a3c-shadow", github, "must be a core id"),
        (
            "a3c-ghost",
            serde_json::json!(["jira"]),
            "must be a core id",
        ),
        (
            "a3c-twice",
            serde_json::json!(["github", "github"]),
            "declared twice",
        ),
    ] {
        write_manifest(dir.path(), id, providers, &urls);
        let err = crate::plugins::read_single_manifest(id).unwrap_err();
        assert!(err.contains(expected), "{id}: {err}");
    }
    write_manifest(dir.path(), "a3c-core", serde_json::json!(["github"]), &urls);
    assert!(
        crate::plugins::read_single_manifest("a3c-core")
            .unwrap()
            .declares_provider("github")
    );

    let plugin_dir = dir.path().join("plugins").join("a3c-nohttp");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::write(
        plugin_dir.join("manifest.json"),
        r#"{"id":"a3c-nohttp","name":"n","version":"1","minAppVersion":"0","main":"m.js","providers":["github"]}"#,
    )
    .unwrap();
    let err = crate::plugins::read_single_manifest("a3c-nohttp").unwrap_err();
    assert!(err.contains("lacks net:http"), "{err}");
}

// Catches: an untrusted redirect to an owner-configured destination on another
// host receives the injected service credential, or an owner destination
// widening where an account request may start.
#[tokio::test]
async fn plugin_service_secret_cannot_follow_foreign_host() {
    let (dir, _guard) = isolated_config();
    let (target, target_seen) = server("", None).await;
    // `localhost` is not `127.0.0.1`: the hop changes host and port.
    let target = target.replace("127.0.0.1", "localhost");
    let (base, seen) = server("/svc", Some(format!("{target}/capture"))).await;
    // The manifest admits an unrelated URL: only the owner configuration admits the target.
    write_manifest(
        dir.path(),
        "a3c-owner",
        descriptor("a3c-owner"),
        &[super::destination_tests::UNRELATED.into()],
    );
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    load(&state, "a3c-owner");
    super::destination_tests::set_owner_destinations(&state, &[format!("{target}/*")]);
    let id = account("a3c-owner", &base, AuthKind::Bearer, "a3c-owner-token").await;

    let response = fetch(&state, "a3c-owner", format!("{base}/redirect"), None, &id)
        .await
        .unwrap();
    assert_eq!(response.status, 200);
    assert_eq!(
        seen.lock().unwrap().last().unwrap(),
        &(
            "/svc/redirect".to_string(),
            Some("Bearer a3c-owner-token".into())
        ),
        "the service host must receive the injected credential"
    );
    assert_eq!(
        *target_seen.lock().unwrap(),
        vec![("/capture".to_string(), None)],
        "the owner destination on another host must not receive Authorization"
    );

    let err = fetch(&state, "a3c-owner", format!("{target}/direct"), None, &id)
        .await
        .unwrap_err();
    assert!(err.contains("outside the account host"), "{err}");
    assert_eq!(
        target_seen.lock().unwrap().len(),
        1,
        "a refused URL reached the target"
    );
}
