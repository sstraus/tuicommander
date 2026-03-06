use std::net::SocketAddr;
use std::sync::Arc;

use tokio::net::TcpListener;

/// Start a relay server with SQLite on a random port.
async fn start_relay_with_db() -> SocketAddr {
    let conn = tuic_relay::db::init(":memory:").await.unwrap();
    let state = tuic_relay::relay::AppState::with_db(conn);
    let router = tuic_relay::routes::build_router(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });

    addr
}

#[tokio::test]
async fn register_returns_token() {
    let addr = start_relay_with_db().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("http://{addr}/register"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    let body: serde_json::Value = resp.json().await.unwrap();
    let token = body["token"].as_str().unwrap();
    assert!(token.starts_with("tuic_"), "token: {token}");
}

#[tokio::test]
async fn stats_requires_auth() {
    let addr = start_relay_with_db().await;
    let client = reqwest::Client::new();

    // No auth → 401
    let resp = client
        .get(format!("http://{addr}/stats"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);

    // Wrong token → 401
    let resp = client
        .get(format!("http://{addr}/stats"))
        .header("Authorization", "Bearer wrong_token")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn stats_returns_data_for_registered_token() {
    let addr = start_relay_with_db().await;
    let client = reqwest::Client::new();

    // Register
    let resp = client
        .post(format!("http://{addr}/register"))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    let token = body["token"].as_str().unwrap().to_string();

    // Stats with valid token
    let resp = client
        .get(format!("http://{addr}/stats"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let stats: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(stats["total_sessions"], 0);
    assert_eq!(stats["total_bytes"], 0);
    assert!(stats["created_at"].as_i64().unwrap() > 0);
}

// --- Push subscription DB tests ---

#[tokio::test]
async fn push_sub_insert_and_list() {
    let conn = tuic_relay::db::init(":memory:").await.unwrap();
    let token_hash = "test_hash_push";

    // No subscriptions initially
    let subs = tuic_relay::db::list_push_subs(&conn, token_hash).await.unwrap();
    assert!(subs.is_empty());

    // Insert a subscription
    tuic_relay::db::insert_push_sub(
        &conn,
        token_hash,
        "https://fcm.googleapis.com/fcm/send/abc123",
        "BPk1cGB...",
        "authsecret1",
    )
    .await
    .unwrap();

    let subs = tuic_relay::db::list_push_subs(&conn, token_hash).await.unwrap();
    assert_eq!(subs.len(), 1);
    assert_eq!(subs[0].endpoint, "https://fcm.googleapis.com/fcm/send/abc123");
    assert_eq!(subs[0].p256dh, "BPk1cGB...");
    assert_eq!(subs[0].auth, "authsecret1");
}

#[tokio::test]
async fn push_sub_upsert_replaces_keys() {
    let conn = tuic_relay::db::init(":memory:").await.unwrap();
    let token_hash = "test_hash_upsert";
    let endpoint = "https://fcm.googleapis.com/fcm/send/same";

    // Insert initial
    tuic_relay::db::insert_push_sub(&conn, token_hash, endpoint, "old_p256dh", "old_auth")
        .await
        .unwrap();

    // Upsert with new keys
    tuic_relay::db::insert_push_sub(&conn, token_hash, endpoint, "new_p256dh", "new_auth")
        .await
        .unwrap();

    let subs = tuic_relay::db::list_push_subs(&conn, token_hash).await.unwrap();
    assert_eq!(subs.len(), 1);
    assert_eq!(subs[0].p256dh, "new_p256dh");
    assert_eq!(subs[0].auth, "new_auth");
}

#[tokio::test]
async fn push_sub_delete() {
    let conn = tuic_relay::db::init(":memory:").await.unwrap();
    let token_hash = "test_hash_delete";
    let endpoint = "https://fcm.googleapis.com/fcm/send/to_delete";

    tuic_relay::db::insert_push_sub(&conn, token_hash, endpoint, "key", "auth")
        .await
        .unwrap();

    // Delete it
    let deleted = tuic_relay::db::delete_push_sub(&conn, token_hash, endpoint)
        .await
        .unwrap();
    assert!(deleted);

    // Gone
    let subs = tuic_relay::db::list_push_subs(&conn, token_hash).await.unwrap();
    assert!(subs.is_empty());

    // Delete again — returns false
    let deleted = tuic_relay::db::delete_push_sub(&conn, token_hash, endpoint)
        .await
        .unwrap();
    assert!(!deleted);
}

#[tokio::test]
async fn push_sub_isolation_between_tokens() {
    let conn = tuic_relay::db::init(":memory:").await.unwrap();

    tuic_relay::db::insert_push_sub(&conn, "hash_a", "https://push/a", "key_a", "auth_a")
        .await
        .unwrap();
    tuic_relay::db::insert_push_sub(&conn, "hash_b", "https://push/b", "key_b", "auth_b")
        .await
        .unwrap();

    let subs_a = tuic_relay::db::list_push_subs(&conn, "hash_a").await.unwrap();
    assert_eq!(subs_a.len(), 1);
    assert_eq!(subs_a[0].endpoint, "https://push/a");

    let subs_b = tuic_relay::db::list_push_subs(&conn, "hash_b").await.unwrap();
    assert_eq!(subs_b.len(), 1);
    assert_eq!(subs_b[0].endpoint, "https://push/b");
}

// --- Push subscribe/unsubscribe HTTP endpoint tests ---

/// Helper to register a token and return it.
async fn register_token(client: &reqwest::Client, addr: std::net::SocketAddr) -> String {
    let resp = client
        .post(format!("http://{addr}/register"))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    body["token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn push_subscribe_requires_auth() {
    let addr = start_relay_with_db().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("http://{addr}/push/subscribe"))
        .json(&serde_json::json!({
            "endpoint": "https://push.example.com/sub",
            "p256dh": "key",
            "auth": "secret"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn push_subscribe_and_unsubscribe() {
    let addr = start_relay_with_db().await;
    let client = reqwest::Client::new();
    let token = register_token(&client, addr).await;

    // Subscribe
    let resp = client
        .post(format!("http://{addr}/push/subscribe"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "endpoint": "https://fcm.googleapis.com/fcm/send/test123",
            "p256dh": "BPk1cGBIaSg",
            "auth": "authsecret"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    // Unsubscribe
    let resp = client
        .delete(format!("http://{addr}/push/subscribe"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "endpoint": "https://fcm.googleapis.com/fcm/send/test123"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Unsubscribe again — 404
    let resp = client
        .delete(format!("http://{addr}/push/subscribe"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "endpoint": "https://fcm.googleapis.com/fcm/send/test123"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

/// Start a relay with a custom rate limit.
async fn start_relay_with_rate_limit(limit: u32) -> SocketAddr {
    let conn = tuic_relay::db::init(":memory:").await.unwrap();
    let state = Arc::new(tuic_relay::relay::AppState {
        sessions: dashmap::DashMap::new(),
        db: Some(conn),
        token_cache: dashmap::DashMap::new(),
        vapid: None,
        http_client: reqwest::Client::new(),
        rate_limits: dashmap::DashMap::new(),
        rate_limit_per_hour: limit,
        max_sessions_per_token: 0,
    });
    let router = tuic_relay::routes::build_router(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });

    addr
}

#[tokio::test]
async fn register_rate_limited_after_threshold() {
    let limit = 3;
    let addr = start_relay_with_rate_limit(limit).await;
    let client = reqwest::Client::new();

    // First N registrations succeed
    for i in 0..limit {
        let resp = client
            .post(format!("http://{addr}/register"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 201, "registration {i} should succeed");
    }

    // Next registration should be rate limited
    let resp = client
        .post(format!("http://{addr}/register"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 429, "registration after limit should be 429");
}
