use std::net::SocketAddr;

use tokio::net::TcpListener;

/// Start a relay server with SQLite on a random port.
async fn start_relay_with_db() -> SocketAddr {
    let conn = tuic_relay::db::init(":memory:").await.unwrap();
    let state = tuic_relay::relay::AppState::with_db(conn);
    let router = tuic_relay::routes::build_router(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
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
