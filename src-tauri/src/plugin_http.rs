//! HTTP fetch API for plugins.
//!
//! Plugins declaring the `net:http` capability can make outbound HTTP requests
//! to URLs matching their declared `allowedUrls` patterns or an owner-configured
//! destination (`plugin_http_destinations`, LAN included). Only http and https
//! pass; account credentials stay bound to the account host.

use std::collections::HashMap;

use crate::provider_gateway::plugin_egress::{Declared, EgressRequest, FetchOptions};

#[path = "plugin_http_account.rs"]
pub(crate) mod account;
#[cfg(test)]
#[path = "plugin_http_account_tests.rs"]
pub(crate) mod account_tests;
#[cfg(test)]
#[path = "plugin_http_destination_tests.rs"]
pub(crate) mod destination_tests;

/// Plugin-scoped requests and bounded, short-lived cancellation-before-start markers.
#[derive(Default)]
pub(crate) struct HttpRequests(std::sync::Mutex<HashMap<(String, String), HttpRequest>>);

struct HttpRequest {
    cancel: tokio::sync::watch::Sender<bool>,
    started: bool,
    created: std::time::Instant,
}

impl HttpRequests {
    fn access(&self) -> std::sync::MutexGuard<'_, HashMap<(String, String), HttpRequest>> {
        let mut requests = self.0.lock().unwrap_or_else(|e| e.into_inner());
        requests.retain(|_, request| request.started || request.created.elapsed().as_secs() < 60);
        requests
    }

    fn start(&self, key: &(String, String)) -> Result<tokio::sync::watch::Receiver<bool>, String> {
        let mut requests = self.access();
        if let Some(request) = requests.get_mut(key) {
            if request.started {
                return Err("Duplicate HTTP request ID".into());
            }
            request.started = true;
            return Ok(request.cancel.subscribe());
        }
        if requests.len() >= 4096 {
            return Err("Too many plugin HTTP requests".into());
        }
        let (cancel, receiver) = tokio::sync::watch::channel(false);
        requests.insert(
            key.clone(),
            HttpRequest {
                cancel,
                started: true,
                created: std::time::Instant::now(),
            },
        );
        Ok(receiver)
    }

    pub(crate) fn cancel_plugin(&self, plugin_id: &str) {
        let mut requests = self.access();
        requests.retain(|(plugin, _), request| {
            if plugin != plugin_id {
                return true;
            }
            request.cancel.send_replace(true);
            request.started
        });
    }

    fn cancel(&self, key: (String, String)) -> Result<(), String> {
        let mut requests = self.access();
        if let Some(request) = requests.get(&key) {
            request.cancel.send_replace(true);
        } else {
            if requests.len() >= 4096 {
                return Err("Too many plugin HTTP requests".into());
            }
            let (cancel, _) = tokio::sync::watch::channel(true);
            requests.insert(
                key,
                HttpRequest {
                    cancel,
                    started: false,
                    created: std::time::Instant::now(),
                },
            );
        }
        Ok(())
    }
}

struct RequestGuard<'a>(&'a HttpRequests, (String, String));
impl Drop for RequestGuard<'_> {
    fn drop(&mut self) {
        self.0.access().remove(&self.1);
    }
}

fn request_key(plugin_id: String, request_id: String) -> Result<(String, String), String> {
    uuid::Uuid::parse_str(&request_id).map_err(|_| "Invalid HTTP request ID")?;
    Ok((plugin_id, request_id))
}

pub(crate) fn plugin_http_cancel_impl(
    state: &crate::AppState,
    plugin_id: String,
    request_id: String,
) -> Result<(), String> {
    crate::plugins::check_plugin_capability(state, &plugin_id, "net:http")?;
    state
        .plugin_http_requests
        .cancel(request_key(plugin_id, request_id)?)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn plugin_http_cancel(
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
    plugin_id: String,
    request_id: String,
) -> Result<(), String> {
    plugin_http_cancel_impl(&state, plugin_id, request_id)
}

#[allow(clippy::too_many_arguments)] // Mirrors the IPC request fields plus its optional cancellation identity.
pub(crate) async fn plugin_http_fetch_cancellable_impl(
    state: &std::sync::Arc<crate::AppState>,
    url: String,
    method: Option<String>,
    headers: Option<HashMap<String, String>>,
    body: Option<String>,
    account: Option<String>,
    plugin_id: String,
    request_id: Option<String>,
    options: FetchOptions,
) -> Result<HttpResponse, String> {
    let Some(request_id) = request_id else {
        return plugin_http_fetch_with_options_impl(
            state, url, method, headers, body, account, plugin_id, options,
        )
        .await;
    };
    crate::plugins::check_plugin_capability(state, &plugin_id, "net:http")?;
    let key = request_key(plugin_id.clone(), request_id)?;
    let mut cancelled = state.plugin_http_requests.start(&key)?;
    let _guard = RequestGuard(&state.plugin_http_requests, key);
    if *cancelled.borrow() {
        return Err("HTTP request cancelled".into());
    }
    tokio::select! {
        biased;
        _ = cancelled.changed() => Err("HTTP request cancelled".into()),
        response = plugin_http_fetch_with_options_impl(state, url, method, headers, body, account, plugin_id, options) => response,
    }
}

/// Response returned to the plugin.
#[derive(Debug, Clone, serde::Serialize)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: String,
}

// ---------------------------------------------------------------------------
// Tauri command
// ---------------------------------------------------------------------------

/// Make an HTTP request on behalf of a plugin.
///
/// Parameters:
/// - `url` — The URL to fetch
/// - `method` — HTTP method (GET, POST, PUT, DELETE, etc.)
/// - `headers` — Request headers
/// - `body` — Optional request body
/// - `account` — Optional account id; Rust injects its credential
/// - `plugin_id` — The requesting plugin's ID (also the manifest lookup key)
#[cfg(feature = "desktop")]
#[tauri::command]
#[allow(clippy::too_many_arguments)] // IPC preserves the existing fields and adds optional request identity.
pub async fn plugin_http_fetch(
    url: String,
    method: Option<String>,
    headers: Option<HashMap<String, String>>,
    body: Option<String>,
    account: Option<String>,
    plugin_id: String,
    request_id: Option<String>,
    timeout_ms: Option<u64>,
    max_response_bytes: Option<u64>,
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
) -> Result<HttpResponse, String> {
    let options = FetchOptions {
        timeout_ms,
        max_response_bytes,
    };
    plugin_http_fetch_cancellable_impl(
        &state, url, method, headers, body, account, plugin_id, request_id, options,
    )
    .await
}

pub(crate) async fn plugin_http_fetch_impl(
    state: &std::sync::Arc<crate::AppState>,
    url: String,
    method: Option<String>,
    headers: Option<HashMap<String, String>>,
    body: Option<String>,
    account: Option<String>,
    plugin_id: String,
) -> Result<HttpResponse, String> {
    plugin_http_fetch_declared_impl(
        state,
        url,
        method,
        headers,
        body,
        account,
        plugin_id,
        Declared::default(),
    )
    .await
}

/// A host call site (a workflow connector) that states its own effect and caller.
#[allow(clippy::too_many_arguments)] // Mirrors the IPC request fields plus the declaration.
pub(crate) async fn plugin_http_fetch_declared_impl(
    state: &std::sync::Arc<crate::AppState>,
    url: String,
    method: Option<String>,
    headers: Option<HashMap<String, String>>,
    body: Option<String>,
    account: Option<String>,
    plugin_id: String,
    declared: Declared,
) -> Result<HttpResponse, String> {
    crate::provider_gateway::plugin_egress::fetch(
        state,
        EgressRequest {
            url,
            method,
            headers,
            body,
            account,
            plugin_id,
            options: FetchOptions::default(),
            declared,
        },
    )
    .await
}

#[allow(clippy::too_many_arguments)] // Mirrors the IPC request fields plus the plugin's limits.
pub(crate) async fn plugin_http_fetch_with_options_impl(
    state: &std::sync::Arc<crate::AppState>,
    url: String,
    method: Option<String>,
    headers: Option<HashMap<String, String>>,
    body: Option<String>,
    account: Option<String>,
    plugin_id: String,
    options: FetchOptions,
) -> Result<HttpResponse, String> {
    crate::provider_gateway::plugin_egress::fetch(
        state,
        EgressRequest {
            url,
            method,
            headers,
            body,
            account,
            plugin_id,
            options,
            declared: Declared::default(),
        },
    )
    .await
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Catches: cancelling only the JS promise while Rust keeps waiting for headers/body.
    #[tokio::test]
    async fn cancellation_drops_headers_and_body_requests() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _override = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
        state
            .loaded_plugins
            .insert("cancel-test".into(), vec!["net:http".into()]);
        for body_phase in [false, true] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let plugin_dir = dir.path().join("plugins/cancel-test");
            std::fs::create_dir_all(&plugin_dir).unwrap();
            std::fs::write(plugin_dir.join("manifest.json"), serde_json::to_vec(&serde_json::json!({
                "id": "cancel-test", "name": "Cancellation", "version": "1.0.0", "minAppVersion": "0.0.0", "main": "main.js", "capabilities": ["net:http"], "allowedUrls": [format!("http://{address}/*")]
            })).unwrap()).unwrap();
            let (ready, started) = tokio::sync::oneshot::channel();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buf = [0; 4096];
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n") {
                    let received = socket.read(&mut buf).await.unwrap();
                    assert!(
                        received > 0,
                        "client disconnected before sending HTTP headers"
                    );
                    request.extend_from_slice(&buf[..received]);
                    assert!(
                        request.len() <= 16 * 1024,
                        "unexpectedly large request headers"
                    );
                }
                if body_phase {
                    socket
                        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nx")
                        .await
                        .unwrap();
                }
                ready.send(()).unwrap();
                // A dropped request closes the socket; the OS may report EOF or a reset.
                socket.read(&mut buf).await
            });
            let id = uuid::Uuid::new_v4().to_string();
            let fetch_state = state.clone();
            let fetch_id = id.clone();
            let fetch = tokio::spawn(async move {
                plugin_http_fetch_cancellable_impl(
                    &fetch_state,
                    format!("http://{address}/wait"),
                    None,
                    None,
                    None,
                    None,
                    "cancel-test".into(),
                    Some(fetch_id),
                    FetchOptions::default(),
                )
                .await
            });
            started.await.unwrap();
            plugin_http_cancel_impl(&state, "cancel-test".into(), id).unwrap();
            let result = tokio::time::timeout(std::time::Duration::from_secs(5), fetch)
                .await
                .expect("cancel did not settle an established HTTP request")
                .unwrap();
            assert_eq!(result.unwrap_err(), "HTTP request cancelled");
            let disconnect = tokio::time::timeout(std::time::Duration::from_secs(5), server)
                .await
                .expect("cancel did not close the HTTP connection")
                .unwrap();
            assert!(
                matches!(&disconnect, Ok(0))
                    || matches!(&disconnect, Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset),
                "expected EOF or connection reset after cancellation, got {disconnect:?}"
            );
            assert!(state.plugin_http_requests.access().is_empty());
        }
    }

    // Catches: cancel arriving before fetch registration is lost, or crosses plugin scope.
    #[tokio::test]
    async fn cancellation_before_registration_is_scoped_and_cleaned() {
        let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
        state
            .loaded_plugins
            .insert("one".into(), vec!["net:http".into()]);
        let id = uuid::Uuid::new_v4().to_string();
        plugin_http_cancel_impl(&state, "one".into(), id.clone()).unwrap();
        let other = state
            .plugin_http_requests
            .start(&("two".into(), id.clone()))
            .unwrap();
        assert!(!*other.borrow());
        let error = plugin_http_fetch_cancellable_impl(
            &state,
            "https://example.invalid".into(),
            None,
            None,
            None,
            None,
            "one".into(),
            Some(id.clone()),
            FetchOptions::default(),
        )
        .await
        .unwrap_err();
        assert_eq!(error, "HTTP request cancelled");
        assert!(
            !state
                .plugin_http_requests
                .access()
                .contains_key(&("one".into(), id))
        );
    }

    // Catches: backend plugin unload leaving active requests running after host teardown.
    #[test]
    fn unloading_plugin_cancels_only_its_requests() {
        let state = crate::state::tests_support::make_test_app_state();
        let one = ("one".into(), uuid::Uuid::new_v4().to_string());
        let two = ("two".into(), uuid::Uuid::new_v4().to_string());
        let own = state.plugin_http_requests.start(&one).unwrap();
        let other = state.plugin_http_requests.start(&two).unwrap();
        crate::plugins::unregister_loaded_plugin_impl(&state, "one");
        assert!(*own.borrow());
        assert!(!*other.borrow());
    }

    // -- Manifest is the source of truth (SSRF / scope-bypass fix) --

    /// A plugin whose manifest allows only host A must not be able to fetch
    /// host B — the allowlist is re-read from the on-disk manifest, so a caller
    /// can no longer widen it (the request parameter is gone entirely).
    #[tokio::test]
    async fn fetch_uses_manifest_allowlist_not_caller_supplied() {
        use std::sync::Arc;

        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        let plugin_id = "scoped-plugin";
        let plugin_dir = dir.path().join("plugins").join(plugin_id);
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(
            plugin_dir.join("manifest.json"),
            r#"{
                "id": "scoped-plugin",
                "name": "Scoped Plugin",
                "version": "1.0.0",
                "minAppVersion": "0.0.0",
                "main": "main.js",
                "capabilities": ["net:http"],
                "allowedUrls": ["https://host-a.example.com/*"]
            }"#,
        )
        .unwrap();

        // The manifest — not any caller-supplied list — is the allowlist source.
        let manifest = crate::plugins::read_single_manifest(plugin_id).unwrap();
        assert_eq!(manifest.allowed_urls, vec!["https://host-a.example.com/*"]);

        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        state
            .loaded_plugins
            .insert(plugin_id.to_string(), vec!["net:http".to_string()]);

        // Host B is not in the manifest — rejected before any network call.
        let err = plugin_http_fetch_impl(
            &state,
            "https://host-b.example.com/steal".to_string(),
            None,
            None,
            None,
            None,
            plugin_id.to_string(),
        )
        .await
        .unwrap_err();
        assert!(err.contains("does not match"), "unexpected error: {err}");

        // Cloud metadata / SSRF target is likewise rejected.
        let ssrf = plugin_http_fetch_impl(
            &state,
            "http://169.254.169.254/latest/meta-data/".to_string(),
            None,
            None,
            None,
            None,
            plugin_id.to_string(),
        )
        .await
        .unwrap_err();
        assert!(ssrf.contains("does not match"), "unexpected error: {ssrf}");
    }

    // -- Redirect re-validation (SEC-2: SSRF via redirect) --

    /// A plugin whose manifest explicitly declares its own localhost test
    /// server must still be blocked from following a redirect the server
    /// issues to an RFC1918 address the plugin never declared. Before the
    /// fix, `Policy::limited(5)` followed any redirect target unchecked.
    #[tokio::test]
    async fn fetch_blocks_redirect_to_undeclared_private_ip() {
        use std::sync::Arc;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        // Server answers the initial (allowed) request with a redirect to a
        // private IP that is not covered by the plugin's allowedUrls pattern.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 1024];
            let _ = sock.read(&mut buf).await;
            let _ = sock
                .write_all(
                    b"HTTP/1.1 302 Found\r\nLocation: http://192.168.1.1/steal\r\nContent-Length: 0\r\n\r\n",
                )
                .await;
        });

        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        let plugin_id = "redirect-plugin";
        let plugin_dir = dir.path().join("plugins").join(plugin_id);
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(
            plugin_dir.join("manifest.json"),
            format!(
                r#"{{
                    "id": "redirect-plugin",
                    "name": "Redirect Plugin",
                    "version": "1.0.0",
                    "minAppVersion": "0.0.0",
                    "main": "main.js",
                    "capabilities": ["net:http"],
                    "allowedUrls": ["http://127.0.0.1:{port}/*"]
                }}"#
            ),
        )
        .unwrap();

        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        state
            .loaded_plugins
            .insert(plugin_id.to_string(), vec!["net:http".to_string()]);

        let err = plugin_http_fetch_impl(
            &state,
            format!("http://127.0.0.1:{port}/start"),
            None,
            None,
            None,
            None,
            plugin_id.to_string(),
        )
        .await
        .unwrap_err();

        assert!(
            err.contains("Private network"),
            "redirect to an undeclared private IP must be blocked: {err}"
        );
    }

    // -- Streamed body size cap (Content-Length absent) --

    /// A response with no Content-Length that streams a body larger than
    /// MAX_RESPONSE_BYTES must be aborted mid-stream, not fully buffered.
    /// The server here uses chunked transfer-encoding (no Content-Length), so
    /// the early content_length() check can't catch it — only the running-total
    /// cap during collection can.
    #[tokio::test]
    async fn fetch_aborts_when_streamed_body_exceeds_cap_without_content_length() {
        use std::sync::Arc;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        // Raw HTTP/1.1 chunked server: emits chunks totaling more than
        // MAX_RESPONSE_BYTES with NO Content-Length header.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            // Drain the request headers (read until we've seen the blank line).
            let mut buf = [0u8; 1024];
            let _ = sock.read(&mut buf).await;
            let _ = sock
                .write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n")
                .await;
            // 64 KiB chunk, repeated past the cap. Stop once the client aborts
            // (write fails with broken pipe) or we've clearly exceeded the cap.
            let payload = vec![b'x'; 64 * 1024];
            let hdr = format!("{:x}\r\n", payload.len());
            let mut sent = 0usize;
            while sent <= crate::provider_gateway::plugin_egress::MAX_RESPONSE_BYTES + 128 * 1024 {
                if sock.write_all(hdr.as_bytes()).await.is_err() {
                    break;
                }
                if sock.write_all(&payload).await.is_err() {
                    break;
                }
                if sock.write_all(b"\r\n").await.is_err() {
                    break;
                }
                sent += payload.len();
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        let plugin_id = "streamer-plugin";
        let plugin_dir = dir.path().join("plugins").join(plugin_id);
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(
            plugin_dir.join("manifest.json"),
            serde_json::to_vec(&serde_json::json!({
                "id": "streamer-plugin",
                "name": "Streamer Plugin",
                "version": "1.0.0",
                "minAppVersion": "0.0.0",
                "main": "main.js",
                "capabilities": ["net:http"],
                "allowedUrls": [format!("http://127.0.0.1:{port}/*")]
            }))
            .unwrap(),
        )
        .unwrap();

        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        state
            .loaded_plugins
            .insert(plugin_id.to_string(), vec!["net:http".to_string()]);

        let err = plugin_http_fetch_impl(
            &state,
            format!("http://127.0.0.1:{port}/big"),
            None,
            None,
            None,
            None,
            plugin_id.to_string(),
        )
        .await
        .unwrap_err();
        assert!(
            err.contains("exceeds maximum size"),
            "expected size-cap error, got: {err}"
        );
    }
}
