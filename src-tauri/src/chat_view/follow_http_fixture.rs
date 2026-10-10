//! Opt-in real-disk → ticker → HTTP/SSE → frontend store/render proof.
//! Transcript discovery is outside this fixture: it registers two known files
//! directly, using the recorded Claude schema, and exercises the real transports.

use super::*;
use axum::{Json, extract::Path as RoutePath, routing::post};
use serde_json::json;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "Requires TUIC_CHAT_FOLLOW_PROOF_DIR, installed JS dependencies and the authorized browser wrapper"]
async fn disk_ticker_http_store_browser_follow_proof() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repo");
    let evidence =
        PathBuf::from(std::env::var_os("TUIC_CHAT_FOLLOW_PROOF_DIR").expect("proof directory"));
    assert!(
        evidence.starts_with(repo),
        "proof files must stay in the worktree"
    );
    std::fs::create_dir_all(&evidence).expect("evidence directory");
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let paths = Arc::new(Mutex::new(HashMap::new()));
    for (sid, text) in [
        ("proof-one", "Initial transcript one"),
        ("proof-two", "Initial transcript two"),
    ] {
        let path = evidence.join(format!("{sid}.jsonl"));
        std::fs::write(&path, row(sid, text)).expect("transcript");
        let mut view = View::new(path.clone(), MAX_LOG_ENTRIES, MAX_LOG_BYTES);
        // The fixture owns the binding; do not ask the real machine's Claude registry.
        view.bound_at = Instant::now() + Duration::from_secs(300);
        state
            .chat_views
            .views
            .lock()
            .insert(sid.into(), Arc::new(Mutex::new(view)));
        paths.lock().insert(sid.to_owned(), path);
    }
    let fixture_state = state.clone();
    let fixture_paths = paths.clone();
    let next_id = Arc::new(AtomicU64::new(0));
    let app = crate::mcp_http::build_router(state, false, false).route(
        "/proof/{sid}/{operation}",
        post(
            move |RoutePath((sid, operation)): RoutePath<(String, String)>,
                  Json(body): Json<Value>| {
                let state = fixture_state.clone();
                let paths = fixture_paths.clone();
                let next_id = next_id.clone();
                async move {
                    let text = body["text"].as_str().expect("fixture text");
                    let id = format!("{sid}-{}", next_id.fetch_add(1, Ordering::Relaxed));
                    let path = paths.lock().get(&sid).expect("fixture session").clone();
                    match operation.as_str() {
                        "append" => {
                            use std::io::Write;
                            std::fs::OpenOptions::new()
                                .append(true)
                                .open(&path)
                                .expect("open")
                                .write_all(row(&id, text).as_bytes())
                                .expect("append");
                        }
                        "replace" => {
                            let replacement = path.with_extension("replacement");
                            std::fs::write(&replacement, row(&id, text)).expect("replacement");
                            std::fs::rename(replacement, &path).expect("replace");
                        }
                        "rebind" => {
                            let replacement = path.with_extension("resumed.jsonl");
                            std::fs::write(&replacement, row(&id, text)).expect("resumed");
                            state
                                .chat_views
                                .views
                                .lock()
                                .get(&sid)
                                .expect("registered view")
                                .lock()
                                .rebind(replacement.clone());
                            // Binding remains fixture-owned after the explicit rebind.
                            state
                                .chat_views
                                .views
                                .lock()
                                .get(&sid)
                                .unwrap()
                                .lock()
                                .bound_at = Instant::now() + Duration::from_secs(300);
                            paths.lock().insert(sid, replacement);
                        }
                        _ => panic!("unknown fixture operation"),
                    }
                    Json(json!({"ok": true}))
                }
            },
        ),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listen");
    let address = listener.local_addr().expect("address");
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .expect("HTTP fixture");
    });
    let result = tokio::time::timeout(
        Duration::from_secs(100),
        tokio::process::Command::new("node")
            .arg(repo.join("scripts/chat-view-follow-proof.mjs"))
            .arg(format!("http://{address}"))
            .arg(&evidence)
            .current_dir(repo)
            .kill_on_drop(true)
            .output(),
    )
    .await;
    server.abort();
    let output = result
        .expect("browser proof exceeded its own 100s budget")
        .expect("launch node");
    assert!(
        output.status.success(),
        "browser proof failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{}", String::from_utf8_lossy(&output.stdout));
}

fn row(id: &str, text: &str) -> String {
    let mut row: Value = serde_json::from_str(include_str!(
        "../fixtures/chat_view/recorded/shape-012.jsonl"
    ))
    .expect("recorded Claude row");
    row["uuid"] = json!(id);
    row["message"]["id"] = json!(id);
    row["message"]["content"][0]["text"] = json!(text);
    format!("{row}\n")
}
