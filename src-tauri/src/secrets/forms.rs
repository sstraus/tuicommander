//! Private form bootstrap: only the native window receives the capability.
use super::*;
use crate::AppState;
use axum::{
    Json,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use std::sync::Arc;

pub(crate) async fn form_http(
    State(state): State<Arc<AppState>>,
    Path(nonce): Path<String>,
) -> Response {
    let reply = match state.secrets.public_form(&nonce) {
        Ok(form) => Json(form).into_response(),
        Err(_) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Unknown or expired secret form"})),
        )
            .into_response(),
    };
    private_response(reply)
}

pub(crate) async fn submit_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<Submission>,
) -> Response {
    let nonce = Zeroizing::new(body.nonce.clone());
    let id = state.secrets.form(&nonce).map(|f| f.id).unwrap_or_default();
    let reply = match state.secrets.submit(&nonce, body) {
        Ok(result) => {
            close_window(&state, &id);
            Json(result).into_response()
        }
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": error})),
        )
            .into_response(),
    };
    private_response(reply)
}

pub(crate) fn private_response(mut response: Response) -> Response {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn secret_form_bootstrap(
    state: tauri::State<'_, Arc<AppState>>,
    window: tauri::WebviewWindow,
) -> Result<Form, String> {
    let inner = state.secrets.inner.lock();
    let pending = inner
        .pending
        .as_ref()
        .ok_or("Unknown or expired secret form")?;
    if window.label() != format!("secret-{}", pending.form.id)
        || !state.secrets.has_window(window.label())
    {
        return Err("Secret bootstrap requires the private native window".into());
    }
    Ok(pending.form.clone())
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn secret_form_submit(
    state: tauri::State<'_, Arc<AppState>>,
    window: tauri::WebviewWindow,
    submission: Submission,
) -> Result<Status, String> {
    // Tauri supplies the window; invoke arguments cannot spoof its identity.
    let id = state.secrets.form(&submission.nonce)?.id;
    let result = account_sink::submit_native(&state, window.label(), submission).await?;
    close_window(&state, &id);
    Ok(result)
}

pub(super) async fn ask(state: &Arc<AppState>, form: Form) -> Result<Status, String> {
    ask_with_sink(state, form, None).await
}

pub(super) async fn ask_with_sink(
    state: &Arc<AppState>,
    form: Form,
    sink: Option<account_sink::Request>,
) -> Result<Status, String> {
    #[cfg(feature = "desktop")]
    if state.app_handle.read().is_some() {
        return ask_native(state, form, sink).await;
    }
    // A headless host has no private window: only the phone link can collect a
    // generic form. Account vault entry stays native-only.
    if sink.is_some() {
        return Err("Account entry requires the private native window of a desktop host".into());
    }
    let address = phone_entry_address(state);
    ask_headless(state, form, address, tuic_ipc::SECRET_FORM_TIMEOUT).await
}

/// Collects a generic form through the one-time phone link alone. The value
/// travels phone → `submit_http` → store; the caller receives names and status.
async fn ask_headless(
    state: &Arc<AppState>,
    form: Form,
    address: Result<String, &'static str>,
    timeout: std::time::Duration,
) -> Result<Status, String> {
    let base = address.map_err(String::from)?;
    // Web Push is the only owner channel encrypted to the phone alone; the
    // `/events` stream is readable by local agents, which must not get the nonce.
    if !AppState::mobile_push_ready(state) {
        return Err("Push notifications are off, so the phone link cannot reach you. Turn on Push notifications in the mobile Settings, then request a new form.".into());
    }
    tracing::info!(source = "secrets", "Opening headless secret form");
    let form = state.secrets.open_with_sink(form, None)?;
    let url = phone_entry_link(&base, &form.nonce);
    let (tx, rx) = tokio::sync::oneshot::channel();
    if let Some(pending) = state.secrets.inner.lock().pending.as_mut() {
        pending.form.mobile_url = Some(url.clone());
        pending.response = Some(tx);
    }
    AppState::send_mobile_push_url(state, url, "A private secret form is waiting for you");
    struct CancelOnDrop {
        state: Arc<AppState>,
        id: String,
    }
    impl Drop for CancelOnDrop {
        fn drop(&mut self) {
            cancel(&self.state, &self.id);
        }
    }
    let _guard = CancelOnDrop {
        state: state.clone(),
        id: form.id,
    };
    match tokio::time::timeout(timeout, rx).await {
        Ok(Ok(status)) => Ok(status),
        _ => Err("Secret form expired or was cancelled".into()),
    }
}

#[cfg(feature = "desktop")]
async fn ask_native(
    state: &Arc<AppState>,
    form: Form,
    sink: Option<account_sink::Request>,
) -> Result<Status, String> {
    {
        tracing::info!(source = "secrets", "Opening private secret form");
        let handle = state
            .app_handle
            .read()
            .clone()
            .ok_or("Secret entry requires a desktop host")?;
        let form = state.secrets.open_with_sink(form, sink)?;
        // Account vault entry remains native-only. Generic forms use the same
        // address/TLS policy as RemoteQrDialog, without its session token.
        let mobile = phone_entry_address(state);
        if let Some(pending) = state.secrets.inner.lock().pending.as_mut()
            && pending.sink.is_none()
        {
            match mobile {
                Ok(base) => pending.form.mobile_url = Some(phone_entry_link(&base, &form.nonce)),
                Err(reason) => pending.form.mobile_unavailable = Some(reason.into()),
            }
        }
        let (tx, rx) = tokio::sync::oneshot::channel();
        if let Some(pending) = state.secrets.inner.lock().pending.as_mut() {
            pending.response = Some(tx);
        }
        let label = format!("secret-{}", form.id);
        *state.secrets.window.lock() = Some(label.clone());
        tracing::info!(source = "secrets", "Creating private secret window");
        let builder = tauri::WebviewWindowBuilder::new(
            &handle,
            &label,
            tauri::WebviewUrl::App("index.html#/secret-form".into()),
        )
        .title("TUICommander — Private secret form")
        .inner_size(540.0, 620.0)
        .resizable(true)
        .devtools(false)
        .on_navigation(|url| {
            url.path() == "/index.html"
                && url.fragment() == Some("/secret-form")
                && ((url.scheme() == "tauri" && url.host_str() == Some("localhost"))
                    || (url.scheme() == "http" && url.host_str() == Some("tauri.localhost"))
                    || (cfg!(debug_assertions)
                        && url.scheme() == "http"
                        && url.host_str() == Some("127.0.0.1")
                        && url.port() == Some(1421)))
        });
        let built = crate::themes::window_background::configure(
            builder,
            crate::themes::window_background::current(&handle),
        )
        .build();
        let window = match built {
            Ok(window) => window,
            Err(error) => {
                tracing::error!(source = "secrets", %error, "Private secret window creation failed");
                state.secrets.release_window(&label);
                cancel(state, &form.id);
                return Err("Could not open private secret form".into());
            }
        };
        tracing::info!(source = "secrets", "Private secret window created");
        let callback_state = Arc::downgrade(state);
        let callback_label = label.clone();
        let id = form.id.clone();
        window.on_window_event(move |event| {
            if matches!(event, tauri::WindowEvent::Destroyed)
                && let Some(state) = callback_state.upgrade()
            {
                state.secrets.release_window(&callback_label);
                cancel(&state, &id);
            }
        });
        // Cancel even if the MCP caller disconnects and drops its future.
        struct CancelOnDrop {
            state: Arc<AppState>,
            id: String,
        }
        impl Drop for CancelOnDrop {
            fn drop(&mut self) {
                cancel(&self.state, &self.id);
                close_window(&self.state, &self.id);
            }
        }
        let _guard = CancelOnDrop {
            state: state.clone(),
            id: form.id,
        };
        let _ = window.set_focus();
        match tokio::time::timeout(tuic_ipc::SECRET_FORM_TIMEOUT, rx).await {
            Ok(Ok(status)) => Ok(status),
            _ => Err("Secret form expired or was closed".into()),
        }
    }
}

pub(super) fn cancel(state: &Arc<AppState>, id: &str) {
    let mut inner = state.secrets.inner.lock();
    if inner.pending.as_ref().is_some_and(|p| p.form.id == id)
        && let Some(mut pending) = inner.pending.take()
    {
        let result = Status {
            names: pending.form.fields.iter().map(|f| f.name.clone()).collect(),
            status: "declined".into(),
        };
        if let Some(tx) = pending.response.take() {
            let _ = tx.send(result);
        }
    }
}

fn close_window(state: &Arc<AppState>, id: &str) {
    #[cfg(feature = "desktop")]
    {
        use tauri::Manager;
        if let Some(handle) = state.app_handle.read().clone()
            && let Some(window) = handle.get_webview_window(&format!("secret-{id}"))
        {
            let _ = window.destroy();
        }
    }
    #[cfg(not(feature = "desktop"))]
    let _ = (state, id);
}

fn phone_entry_address(state: &AppState) -> Result<String, &'static str> {
    if !state.config.read().services.server.enabled {
        return Err(
            "Remote Access is off. Enable it in Settings → Remote Access, then request a new form.",
        );
    }
    let ip = crate::get_local_ips_impl(state).into_iter().next()
        .ok_or("No remote address is available. Connect to a network and check Settings → Remote Access, then request a new form.")?;
    Ok(crate::remote_access_base_url(state, &ip.ip))
}

/// The one-time phone link for a pending form; the nonce is its only capability.
fn phone_entry_link(base: &str, nonce: &str) -> String {
    format!("{base}index.html#/secret-form?nonce={nonce}")
}

#[cfg(test)]
mod tests {
    // Catches: offering an unusable phone address while Remote Access is off.
    #[test]
    fn phone_entry_disabled_has_setup_guidance() {
        let state = crate::state::tests_support::make_test_app_state();
        state.config.write().services.server.enabled = false;
        let reason = super::phone_entry_address(&state).unwrap_err();
        assert!(reason.contains("Remote Access is off"));
        assert!(reason.contains("Settings → Remote Access"));
    }

    use super::*;
    use std::collections::BTreeMap;
    use std::time::Duration;

    fn test_form() -> Form {
        Form::request(
            vec![Field {
                name: "TOKEN".into(),
                kind: FieldKind::Password,
                display: None,
            }],
            "headless".into(),
        )
        .unwrap()
    }

    fn body(nonce: &str, value: &str) -> Submission {
        Submission {
            nonce: nonce.into(),
            status: "stored".into(),
            values: BTreeMap::from([("TOKEN".into(), value.into())]),
            template: None,
        }
    }

    /// Waits until the headless request published its phone link; returns the nonce.
    async fn published_nonce(state: &Arc<AppState>) -> (String, String) {
        for _ in 0..200 {
            if let Some(p) = state.secrets.inner.lock().pending.as_ref()
                && let Some(url) = p.form.mobile_url.clone()
            {
                return (url, p.form.nonce.clone());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("headless form never published a phone link");
    }

    // Catches: a phone-only owner cannot initiate entry without a desktop window.
    #[tokio::test]
    async fn headless_secret_request_returns_secure_phone_entry() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        subscribe_phone(&state, UNREACHABLE_PHONE.into());
        let ask = tokio::spawn({
            let state = state.clone();
            async move {
                ask_headless(
                    &state,
                    test_form(),
                    Ok("https://phone.test/".into()),
                    Duration::from_secs(30),
                )
                .await
            }
        });
        let (url, nonce) = published_nonce(&state).await;
        assert_eq!(
            url,
            format!("https://phone.test/index.html#/secret-form?nonce={nonce}")
        );
        let page = form_http(State(state.clone()), Path(nonce.clone())).await;
        assert_eq!(page.status(), StatusCode::OK);
        assert_eq!(page.headers()[header::CACHE_CONTROL], "no-store");
        let reply = submit_http(State(state.clone()), Json(body(&nonce, "phone-secret"))).await;
        assert_eq!(reply.status(), StatusCode::OK);
        let status = ask.await.unwrap().unwrap();
        assert_eq!(status.status, "stored");
        assert_eq!(status.names, vec!["TOKEN".to_string()]);
    }

    // Catches: an untrusted caller retrieving the submitted value, or reusing a
    // consumed/expired nonce.
    #[tokio::test]
    async fn headless_secret_value_never_reaches_agent_or_reused_nonce() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        subscribe_phone(&state, UNREACHABLE_PHONE.into());
        let ask = tokio::spawn({
            let state = state.clone();
            async move {
                ask_headless(
                    &state,
                    test_form(),
                    Ok("https://phone.test/".into()),
                    Duration::from_secs(30),
                )
                .await
            }
        });
        let (_, nonce) = published_nonce(&state).await;
        submit_http(State(state.clone()), Json(body(&nonce, "phone-secret"))).await;
        let status = ask.await.unwrap().unwrap();
        // The agent-visible reply carries names and status only.
        let agent_view = serde_json::to_string(&status).unwrap();
        assert!(!agent_view.contains("phone-secret"), "{agent_view}");
        // Consumed nonce: no replay, no re-read.
        let replay = submit_http(State(state.clone()), Json(body(&nonce, "replacement"))).await;
        assert_eq!(replay.status(), StatusCode::BAD_REQUEST);
        let page = form_http(State(state.clone()), Path(nonce.clone())).await;
        assert_eq!(page.status(), StatusCode::NOT_FOUND);
        // The value is stored for later injection but masked in output.
        assert_eq!(state.secrets.mask("x phone-secret x"), "x [REDACTED] x");
        assert_eq!(state.secrets.mask("replacement"), "replacement");

        // Expiry: the nonce dies with the request.
        let expired = {
            let state = state.clone();
            tokio::spawn(async move {
                ask_headless(
                    &state,
                    test_form(),
                    Ok("https://phone.test/".into()),
                    Duration::from_millis(100),
                )
                .await
            })
        };
        let (_, late) = published_nonce(&state).await;
        let error = expired.await.unwrap().err().unwrap();
        assert!(error.contains("expired"));
        let late_reply = submit_http(State(state.clone()), Json(body(&late, "late"))).await;
        assert_eq!(late_reply.status(), StatusCode::BAD_REQUEST);
        assert_eq!(state.secrets.mask("late"), "late");
    }

    // Catches: a headless request left pending (blocking inspection) when the
    // agent call is dropped, or Remote Access off, no address or no push device
    // opening a form whose link can never reach the owner.
    #[tokio::test]
    async fn headless_secret_request_cancels_and_reports_setup_guidance() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let unavailable = ask_headless(
            &state,
            test_form(),
            Err("Remote Access is off. Enable it."),
            Duration::from_secs(30),
        )
        .await
        .err()
        .unwrap();
        assert!(unavailable.contains("Remote Access is off"));
        assert!(state.secrets.inner.lock().pending.is_none());
        let no_push = ask_headless(
            &state,
            test_form(),
            Ok("https://phone.test/".into()),
            Duration::from_secs(30),
        )
        .await
        .err()
        .unwrap();
        assert!(no_push.contains("Push notifications are off"), "{no_push}");
        assert!(state.secrets.inner.lock().pending.is_none());
        subscribe_phone(&state, UNREACHABLE_PHONE.into());

        let ask = tokio::spawn({
            let state = state.clone();
            async move {
                ask_headless(
                    &state,
                    test_form(),
                    Ok("https://phone.test/".into()),
                    Duration::from_secs(30),
                )
                .await
            }
        });
        published_nonce(&state).await;
        ask.abort();
        let _ = ask.await;
        assert!(state.secrets.inner.lock().pending.is_none());
        assert!(!state.secrets.tools_blocked());
    }

    // Catches: account vault entry becoming reachable through the headless phone path.
    #[tokio::test]
    async fn headless_account_entry_stays_native_only() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let error = ask_with_sink(
            &state,
            test_form(),
            Some(account_sink::Request::Rotate { id: "a".into() }),
        )
        .await
        .err()
        .unwrap();
        assert!(error.contains("native window"));
        assert!(state.secrets.inner.lock().pending.is_none());
    }

    // Catches: a headless host without Remote Access still failing with the
    // desktop-host error instead of setup guidance.
    #[tokio::test]
    async fn headless_ask_without_remote_access_gives_setup_guidance() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        state.config.write().services.server.enabled = false;
        let error = ask_with_sink(&state, test_form(), None)
            .await
            .err()
            .unwrap();
        assert!(error.contains("Remote Access is off"), "{error}");
    }

    /// Loopback discard port: the push is refused at once.
    const UNREACHABLE_PHONE: &str = "http://127.0.0.1:9/push";

    /// Enables Web Push with one subscription whose endpoint is `endpoint`;
    /// returns the subscriber's private key and auth secret to decrypt with.
    fn subscribe_phone(state: &AppState, endpoint: String) -> (p256::SecretKey, [u8; 16]) {
        use base64ct::{Base64UrlUnpadded, Encoding};
        use p256::elliptic_curve::sec1::ToEncodedPoint;
        let receiver = p256::SecretKey::random(&mut rand_core::OsRng);
        let auth = [7u8; 16];
        let (private, public) = crate::push::generate_vapid_keys().unwrap();
        {
            let mut config = state.config.write();
            config.services.push.enabled = true;
            config.services.push.vapid_private_key = private;
            config.services.push.vapid_public_key = public;
            // The loopback receiver must be an enrolled provider (#2668-aa16).
            config.services.push.added_providers = vec!["127.0.0.1".into()];
        }
        state.push_store.upsert(crate::push::PushSubscription {
            endpoint,
            keys: crate::push::PushSubscriptionKeys {
                p256dh: Base64UrlUnpadded::encode_string(
                    receiver.public_key().to_encoded_point(false).as_bytes(),
                ),
                auth: Base64UrlUnpadded::encode_string(&auth),
            },
            created_at: chrono::Utc::now(),
        });
        (receiver, auth)
    }

    /// RFC 8291 receiver side: decrypts one aes128gcm push body to its JSON payload.
    fn open_push(receiver: &p256::SecretKey, auth: &[u8], body: &[u8]) -> serde_json::Value {
        use crate::push::crypto::hkdf_sha256;
        use p256::elliptic_curve::sec1::ToEncodedPoint;
        let (salt, sender) = (&body[..16], &body[21..86]);
        let sender_key = p256::PublicKey::from_sec1_bytes(sender).unwrap();
        let shared =
            p256::ecdh::diffie_hellman(receiver.to_nonzero_scalar(), sender_key.as_affine());
        let mut info = b"WebPush: info\0".to_vec();
        info.extend_from_slice(receiver.public_key().to_encoded_point(false).as_bytes());
        info.extend_from_slice(sender);
        let ikm: [u8; 32] = hkdf_sha256(auth, shared.raw_secret_bytes(), &info);
        let key: [u8; 16] = hkdf_sha256(salt, &ikm, b"Content-Encoding: aes128gcm\0");
        let nonce: [u8; 12] = hkdf_sha256(salt, &ikm, b"Content-Encoding: nonce\0");
        let key = ring::aead::LessSafeKey::new(
            ring::aead::UnboundKey::new(&ring::aead::AES_128_GCM, &key).unwrap(),
        );
        let mut record = body[86..].to_vec();
        let plain = key
            .open_in_place(
                ring::aead::Nonce::assume_unique_for_key(nonce),
                ring::aead::Aad::empty(),
                &mut record,
            )
            .unwrap();
        // Strip the RFC 8188 last-record delimiter.
        serde_json::from_slice(&plain[..plain.len() - 1]).unwrap()
    }

    // Catches: the headless request writes its phone link only into the pending
    // form, which nothing but the native window reads, so the phone-only owner
    // never receives it and the agent call waits out the whole timeout.
    #[tokio::test]
    async fn headless_secret_request_delivers_phone_link_to_owner() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/push", listener.local_addr().unwrap());
        let (pushed, mut received) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
        let server = tokio::spawn(async move {
            let route = axum::routing::post(move |body: axum::body::Bytes| {
                let _ = pushed.send(body.to_vec());
                async { StatusCode::CREATED }
            });
            axum::serve(listener, axum::Router::new().route("/push", route))
                .await
                .unwrap();
        });
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let (receiver, auth) = subscribe_phone(&state, endpoint);
        let ask = tokio::spawn({
            let state = state.clone();
            async move {
                ask_headless(
                    &state,
                    test_form(),
                    Ok("https://phone.test/".into()),
                    Duration::from_secs(30),
                )
                .await
            }
        });
        let (_, nonce) = published_nonce(&state).await;
        let body = tokio::time::timeout(Duration::from_secs(10), received.recv())
            .await
            .expect("no push reached the owner's phone")
            .unwrap();
        let payload = open_push(&receiver, &auth, &body);
        assert_eq!(
            payload["url"],
            format!("https://phone.test/index.html#/secret-form?nonce={nonce}")
        );
        ask.abort();
        server.abort();
    }
}
