//! Documented local HTTP/JSON contract responder.
use serde_json::{Value, json};
use std::sync::Arc;

pub(super) async fn server(
    dir: &std::path::Path,
    replies: Vec<(u16, Value, Option<&str>)>,
) -> (
    Arc<crate::AppState>,
    String,
    tokio::task::JoinHandle<Vec<String>>,
) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/api/", listener.local_addr().unwrap());
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    state
        .loaded_plugins
        .insert("workflow-http-test".into(), vec!["net:http".into()]);
    let plugin_dir = dir.join("plugins/workflow-http-test");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::write(plugin_dir.join("manifest.json"), serde_json::to_vec(&json!({
        "id": "workflow-http-test", "name": "Local recorded contract", "version": "1.0.0", "minAppVersion": "0.0.0", "main": "main.js", "capabilities": ["net:http"], "allowedUrls": [format!("{base}*")]
    })).unwrap()).unwrap();
    // This is the existing owner's credential-free local destination setting.
    state.config.write().plugin_http_destinations = vec![format!("{base}*")];
    let replies: Vec<_> = replies
        .into_iter()
        .map(|(status, body, retry)| {
            (
                status,
                if status == 204 {
                    String::new()
                } else {
                    body.to_string()
                },
                retry.map(str::to_owned),
            )
        })
        .collect();
    let task = tokio::spawn(async move {
        let mut requests = Vec::new();
        for (status, body, retry) in replies {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let header_end = loop {
                let mut buf = [0; 4096];
                let n = socket.read(&mut buf).await.unwrap();
                assert!(n > 0, "client ended before headers");
                request.extend_from_slice(&buf[..n]);
                if let Some(end) = request.windows(4).position(|b| b == b"\r\n\r\n") {
                    break end + 4;
                }
            };
            let headers = String::from_utf8_lossy(&request[..header_end]);
            let length = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .and_then(|s| s.trim().parse::<usize>().ok())
                })
                .unwrap_or(0);
            while request.len() < header_end + length {
                let mut buf = [0; 4096];
                let n = socket.read(&mut buf).await.unwrap();
                assert!(n > 0, "client ended before body");
                request.extend_from_slice(&buf[..n]);
            }
            requests.push(String::from_utf8(request).unwrap());
            let retry = retry
                .map(|s| format!("Retry-After: {s}\r\n"))
                .unwrap_or_default();
            let reply = format!(
                "HTTP/1.1 {status} Contract\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{retry}\r\n{body}",
                body.len()
            );
            socket.write_all(reply.as_bytes()).await.unwrap();
        }
        requests
    });
    (state, base, task)
}
