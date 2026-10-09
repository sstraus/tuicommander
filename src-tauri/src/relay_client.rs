//! Relay client: connects to the cloud relay server via WSS, bridging
//! encrypted messages between TUICommander's event bus and the mobile PWA.
//!
//! TRUST MODEL — this is NOT end-to-end encryption. The AES-256-GCM key is
//! derived (HKDF) from the relay token, and that SAME token is sent to the
//! relay for authentication (see `connect_and_run`, the `Bearer {token}`
//! handshake). The relay operator therefore holds the token and CAN re-derive
//! the key and decrypt all forwarded messages. The guarantee is transport
//! encryption with a trusted relay operator — treat the operator as able to
//! read message contents. True E2E would require a key the relay never sees
//! (e.g. X25519 ECDH per `docs/research/e2e-relay-server.md`).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use ring::aead::{AES_256_GCM, Aad, LessSafeKey, Nonce, UnboundKey};
use ring::rand::{SecureRandom, SystemRandom};
use tokio::sync::oneshot;
use tokio_tungstenite::tungstenite::Message;

use crate::app_logger::log_via_state;
use crate::state::{AppEvent, AppState};

// ---------------------------------------------------------------------------
// Relay message types (mirrored from tools/relay/src/types.rs)
// ---------------------------------------------------------------------------

/// Relay-generated status messages sent as plaintext JSON to connected peers.
#[derive(Debug, Clone, serde::Deserialize, PartialEq)]
#[serde(tag = "type")]
enum RelayMessage {
    /// Connection status update from relay to peers.
    #[serde(rename = "relay:status")]
    Status { peer: PeerStatus },

    /// Push notification hint from TUICommander to relay (echoed back).
    #[serde(rename = "relay:push")]
    Push {
        reason: String,
        session_name: String,
    },
}

/// Peer connection state as seen by the relay.
#[derive(Debug, Clone, serde::Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum PeerStatus {
    Waiting,
    Connected,
    Disconnected,
    Timeout,
}

// ---------------------------------------------------------------------------
// Message Encryption (AES-256-GCM with HKDF key derivation)
// NOTE: not end-to-end — the key is derived from the relay token, which the
// relay itself receives for auth, so the relay operator CAN decrypt. See the
// TRUST MODEL note in the module docs above.
// ---------------------------------------------------------------------------

struct Aes256Gcm(LessSafeKey);

impl Aes256Gcm {
    fn new(key: &[u8; 32]) -> Self {
        Self(LessSafeKey::new(
            UnboundKey::new(&AES_256_GCM, key).expect("AES-256 key is exactly 32 bytes"),
        ))
    }
}

/// Derive a 256-bit AES key from the relay token using HKDF-SHA-256.
///
/// NOT E2E: the relay receives this same token for authentication and can
/// therefore re-derive this key (see module-level TRUST MODEL note).
///
/// BREAKING CHANGE: mobile clients must update their key derivation to use the
/// same HKDF parameters (salt + info) or they will fail to decrypt messages.
fn derive_cipher(relay_token: &str) -> Aes256Gcm {
    Aes256Gcm::new(&derive_key(relay_token))
}

fn derive_key(relay_token: &str) -> [u8; 32] {
    crate::push::crypto::hkdf_sha256(
        b"tuicommander-relay-v1",
        relay_token.as_bytes(),
        b"aes-256-gcm-key",
    )
}

/// Encrypt plaintext with AES-256-GCM. Returns nonce (12 bytes) || ciphertext.
fn encrypt(cipher: &Aes256Gcm, plaintext: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut nonce = [0; 12];
    SystemRandom::new()
        .fill(&mut nonce)
        .map_err(|_| anyhow::anyhow!("nonce generation failed"))?;
    encrypt_with_nonce(cipher, nonce, plaintext)
}

fn encrypt_with_nonce(
    cipher: &Aes256Gcm,
    nonce: [u8; 12],
    plaintext: &[u8],
) -> anyhow::Result<Vec<u8>> {
    let mut ciphertext = plaintext.to_vec();
    cipher
        .0
        .seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce),
            Aad::empty(),
            &mut ciphertext,
        )
        .map_err(|_| anyhow::anyhow!("encryption failed"))?;
    let mut out = Vec::with_capacity(12 + ciphertext.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Decrypt nonce (12 bytes) || ciphertext with AES-256-GCM.
fn decrypt(cipher: &Aes256Gcm, data: &[u8]) -> anyhow::Result<Vec<u8>> {
    if data.len() < 12 {
        anyhow::bail!("ciphertext too short (missing nonce)");
    }
    let (nonce_bytes, ciphertext) = data.split_at(12);
    let nonce = Nonce::try_assume_unique_for_key(nonce_bytes)
        .map_err(|_| anyhow::anyhow!("invalid nonce length"))?;
    let mut plaintext = ciphertext.to_vec();
    let decrypted = cipher
        .0
        .open_in_place(nonce, Aad::empty(), &mut plaintext)
        .map_err(|_| anyhow::anyhow!("decryption failed"))?;
    Ok(decrypted.to_vec())
}

// ---------------------------------------------------------------------------
// Push-hint decision logic
// ---------------------------------------------------------------------------

/// Evaluate a parsed PTY event and determine the new `awaiting_input` state.
/// Returns `(new_awaiting, should_push)` where `should_push` is true when the
/// agent transitions from working to awaiting input (i.e. a "question" event
/// while not already awaiting).
fn evaluate_push_hint(
    parsed: &serde_json::Value,
    prior_confidence: Option<bool>,
) -> (Option<bool>, bool) {
    let event_type = parsed.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let new_confidence = match event_type {
        "question" => Some(
            parsed
                .get("confident")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
        ),
        "choice-prompt" => Some(true),
        "user-input" | "question-cleared" | "choice-cleared" => None,
        "status-line" if prior_confidence == Some(false) => None,
        _ => prior_confidence,
    };
    let should_push = new_confidence.is_some() && prior_confidence.is_none();
    (new_confidence, should_push)
}

// ---------------------------------------------------------------------------
// What reaches the phone
// ---------------------------------------------------------------------------

/// Whether a mobile peer is on the other end of the relay.
///
/// `Waiting` is the relay holding the slot open for a peer that has not arrived,
/// so it is not an audience: encrypting and shipping the whole event bus to it
/// burns CPU and uplink on frames nobody reads.
fn peer_is_attached(status: &PeerStatus) -> bool {
    match status {
        PeerStatus::Connected => true,
        PeerStatus::Waiting | PeerStatus::Disconnected | PeerStatus::Timeout => false,
    }
}

/// Whether an event means anything to a remote client.
///
/// The dropped variants are instructions aimed at the desktop process itself —
/// a phone cannot open a tab in the app's webview, drive the desktop browser
/// through an OAuth flow, or reload a locally-installed plugin — plus raw
/// filesystem-watcher churn, which fires per write during a build.
fn is_relayable(event: &AppEvent) -> bool {
    !matches!(
        event,
        AppEvent::UiTab { .. }
            | AppEvent::CloseHtmlTabs { .. }
            | AppEvent::McpOAuthStart { .. }
            | AppEvent::PluginChanged { .. }
            | AppEvent::DirChanged { .. }
    )
}

// ---------------------------------------------------------------------------
// Relay client lifecycle
// ---------------------------------------------------------------------------

/// Maximum backoff between reconnection attempts.
const MAX_BACKOFF: Duration = Duration::from_secs(60);

/// Initial backoff between reconnection attempts.
const INITIAL_BACKOFF: Duration = Duration::from_secs(1);

/// The settings that decide whether the relay connects, and where to.
///
/// A snapshot, not a live read: the supervisor compares the set it started a
/// client with against the current one to decide whether the running client is
/// still the right client.
#[derive(Clone, Debug, PartialEq)]
struct RelaySettings {
    url: String,
    token: String,
    session_id: String,
}

impl RelaySettings {
    /// What a live client would need, or `None` when the relay must not run.
    /// `enabled` alone is not enough — an empty URL or token cannot connect.
    fn active(config: &crate::config::AppConfig) -> Option<Self> {
        let relay = &config.services.relay;
        (relay.enabled && !relay.url.is_empty() && !relay.token.is_empty()).then(|| Self {
            url: relay.url.clone(),
            token: relay.token.clone(),
            session_id: relay.session_id.clone(),
        })
    }
}

/// Tell the relay supervisor that the config moved.
///
/// Called from `config::commit_config_change`, the single choke point every
/// config write goes through (IPC `save_config`, `PUT /config`, the MCP
/// `config/save`). Hanging it there rather than on each writer is what keeps
/// the desktop and HTTP transports from drifting: no writer can forget it.
pub(crate) fn notify_config_changed(state: &AppState) {
    state
        .relay
        .config_revision
        .send_modify(|revision| *revision += 1);
}

/// Own the relay client's lifecycle for the life of the process.
///
/// The client only knows how to stay connected; deciding *whether* it should be
/// connected is this task's job. It is spawned unconditionally at boot — the
/// relay being off is a state it supervises, not a reason not to run — because
/// that is what turns the Settings toggle into a start/stop rather than a
/// "restart the app for this to take effect".
pub(crate) async fn supervise(state: Arc<AppState>, mut shutdown_rx: oneshot::Receiver<()>) {
    let mut config_rx = state.relay.config_revision.subscribe();
    let mut client: Option<Client> = None;

    loop {
        let wanted = RelaySettings::active(&state.config.read());
        if wanted.as_ref() != client.as_ref().map(|running| &running.settings) {
            if let Some(running) = client.take() {
                running.stop().await;
                log_via_state(&state, "info", "relay", "stopped");
            }
            client = wanted.map(|settings| Client::start(&state, settings));
        }

        tokio::select! {
            _ = &mut shutdown_rx => break,
            // The sender lives in AppState: an error means the app is gone.
            changed = config_rx.changed() => if changed.is_err() { break },
        }
    }

    if let Some(running) = client.take() {
        running.stop().await;
    }
    log_via_state(&state, "info", "relay", "supervisor shutting down");
}

/// A running relay client and the handle that stops it.
struct Client {
    settings: RelaySettings,
    stop: oneshot::Sender<()>,
    task: tokio::task::JoinHandle<()>,
}

impl Client {
    fn start(state: &Arc<AppState>, settings: RelaySettings) -> Self {
        let (stop, stop_rx) = oneshot::channel();
        let task = tokio::spawn(run(state.clone(), settings.clone(), stop_rx));
        Self {
            settings,
            stop,
            task,
        }
    }

    /// Close the socket and wait for the client to be gone before the next one
    /// starts. Bounded without a timeout on purpose: `run` reaches an await that
    /// watches the stop signal on every path, including the connect attempt, so
    /// a deadline here could only fire on a machine that was merely slow.
    async fn stop(self) {
        let _ = self.stop.send(());
        let _ = self.task.await;
    }
}

/// Keep one relay connection alive: connect, bridge, reconnect on loss.
/// Returns when the stop signal is received.
async fn run(
    state: Arc<AppState>,
    settings: RelaySettings,
    mut shutdown_rx: oneshot::Receiver<()>,
) {
    let cipher = derive_cipher(&settings.token);
    let ws_url = format!("{}/ws/{}", settings.url, settings.session_id);
    let mut backoff = INITIAL_BACKOFF;

    loop {
        log_via_state(&state, "info", "relay", &format!("connecting to {ws_url}"));

        let outcome =
            connect_and_run(&state, &ws_url, &settings.token, &cipher, &mut shutdown_rx).await;

        // `connected` is raised once the handshake is through, so swapping it
        // here both clears the reported status and answers "did we get in?".
        // A connection that came up ends the failure streak: whatever dropped
        // it is a new failure, and it starts from the bottom of the ladder.
        // Without this the backoff only ever grows, and a client that has been
        // up for a week waits a full minute to recover from one blip.
        if state
            .relay
            .connected
            .swap(false, std::sync::atomic::Ordering::Relaxed)
        {
            backoff = INITIAL_BACKOFF;
        }

        match outcome {
            Ok(ShutdownReason::Signal) => {
                log_via_state(&state, "info", "relay", "shutting down");
                return;
            }
            Ok(ShutdownReason::Disconnected) => {
                log_via_state(
                    &state,
                    "warn",
                    "relay",
                    &format!("disconnected, reconnecting in {}s", backoff.as_secs()),
                );
            }
            Err(e) => {
                log_via_state(
                    &state,
                    "error",
                    "relay",
                    &format!(
                        "connection error: {e}, reconnecting in {}s",
                        backoff.as_secs()
                    ),
                );
            }
        }

        // Wait for backoff or shutdown
        tokio::select! {
            _ = tokio::time::sleep(backoff) => {}
            _ = &mut shutdown_rx => {
                log_via_state(&state, "info", "relay", "shutting down during backoff");
                return;
            }
        }

        // Exponential backoff with cap
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

enum ShutdownReason {
    Signal,
    Disconnected,
}

/// Connect to relay, authenticate, and run the event bridge loop.
async fn connect_and_run(
    state: &Arc<AppState>,
    ws_url: &str,
    relay_token: &str,
    cipher: &Aes256Gcm,
    shutdown_rx: &mut oneshot::Receiver<()>,
) -> anyhow::Result<ShutdownReason> {
    // Racing the stop signal here is what lets the supervisor swap settings
    // promptly: a connect to a black-holed address can otherwise sit in the
    // TCP handshake for a minute with the signal already sent.
    let (ws_stream, _) = tokio::select! {
        connected = tokio_tungstenite::connect_async(ws_url) => connected?,
        _ = &mut *shutdown_rx => return Ok(ShutdownReason::Signal),
    };
    let (mut ws_sink, mut ws_source) = ws_stream.split();

    // Authenticate: send bearer token as first text message.
    // NOTE: this is the exact reason the scheme is not E2E — the relay now
    // holds `relay_token`, the same secret `derive_cipher` uses for the AES
    // key, so it can decrypt everything below. Trusted-relay, not zero-knowledge.
    ws_sink
        .send(Message::Text(format!("Bearer {relay_token}").into()))
        .await?;

    // Writing the bearer proves nothing: the relay reads it, verifies it, and
    // closes with 4003 when it does not resolve. Raising `connected` here
    // therefore called a rejected token a successful connection — and since
    // `run` resets the backoff ladder off that same flag, every rejected
    // attempt dropped the wait back to one second and hammered the relay for as
    // long as the token stayed wrong. The relay answers a peer that gets past
    // auth with an unconditional `relay:status`, so that frame — and nothing
    // earlier — is the handshake completing.
    let mut event_rx = state.event_bus.subscribe();
    let mut authenticated = false;
    let mut awaiting_by_session: HashMap<String, bool> = HashMap::new();
    // The relay tells us when a phone is actually listening. Until it does, the
    // only thing worth sending is a push hint — that is what wakes the phone up.
    let mut peer_attached = false;

    loop {
        tokio::select! {
            // Shutdown signal
            _ = &mut *shutdown_rx => {
                let _ = ws_sink.close().await;
                return Ok(ShutdownReason::Signal);
            }

            // Incoming event from event bus → encrypt and send to relay
            event = event_rx.recv() => {
                match event {
                    Ok(ref evt) => {
                        // Check for question/user-input transition → send push hint
                        if let AppEvent::PtyParsed { parsed, session_id, .. } = evt {
                            let prior_confidence = awaiting_by_session.get(session_id).copied();
                            let (new_confidence, should_push) =
                                evaluate_push_hint(parsed, prior_confidence);
                            if should_push {
                                let push_hint = serde_json::json!({
                                    "type": "relay:push",
                                    "reason": "awaiting_input",
                                    "session_name": session_id,
                                });
                                let _ = ws_sink.send(Message::Text(
                                    push_hint.to_string().into()
                                )).await;
                            }
                            if let Some(confidence) = new_confidence {
                                awaiting_by_session.insert(session_id.clone(), confidence);
                            } else {
                                awaiting_by_session.remove(session_id);
                            }
                        } else if let AppEvent::PtyExit { session_id } = evt {
                            awaiting_by_session.remove(session_id);
                        }

                        // Encrypt and forward the event — but only when a peer is
                        // there to read it, and only for events it can use. The
                        // awaiting bookkeeping above runs either way, so the state
                        // is already correct when a phone attaches.
                        if peer_attached && is_relayable(evt) {
                            let json = serde_json::to_vec(evt)?;
                            let encrypted = encrypt(cipher, &json)?;
                            if ws_sink.send(Message::Binary(encrypted.into())).await.is_err() {
                                return Ok(ShutdownReason::Disconnected);
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        log_via_state(state, "warn", "relay", &format!("event bus lagged, skipped {n} events"));
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        return Ok(ShutdownReason::Signal);
                    }
                }
            }

            // Incoming message from relay → decrypt and dispatch
            msg = ws_source.next() => {
                match msg {
                    Some(Ok(Message::Binary(data))) => {
                        match decrypt(cipher, &data) {
                            Ok(_plaintext) => {
                                // TODO: dispatch decrypted message as mobile command
                                log_via_state(state, "info", "relay", "received mobile command (dispatch not yet implemented)");
                            }
                            Err(e) => {
                                log_via_state(state, "error", "relay", &format!("failed to decrypt message: {e}"));
                            }
                        }
                    }
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<RelayMessage>(&text) {
                            Ok(RelayMessage::Status { peer }) => {
                                if !authenticated {
                                    authenticated = true;
                                    log_via_state(state, "info", "relay", "authenticated, starting event bridge");
                                    // Also what tells `run` this attempt got in,
                                    // so the backoff ladder resets — the reset
                                    // lives there, not here.
                                    state
                                        .relay
                                        .connected
                                        .store(true, std::sync::atomic::Ordering::Relaxed);
                                }
                                peer_attached = peer_is_attached(&peer);
                                let label = match peer {
                                    PeerStatus::Connected => "mobile peer connected",
                                    PeerStatus::Disconnected => "mobile peer disconnected",
                                    PeerStatus::Waiting => "mobile peer waiting",
                                    PeerStatus::Timeout => "mobile peer timed out",
                                };
                                log_via_state(state, "info", "relay", label);
                            }
                            Ok(RelayMessage::Push { .. }) => {
                                // Push hints echoed back — ignore
                            }
                            Err(_) => {
                                log_via_state(state, "warn", "relay", &format!("unrecognized text frame: {text}"));
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        return Ok(ShutdownReason::Disconnected);
                    }
                    Some(Ok(_)) => {} // ping/pong handled by tungstenite
                    Some(Err(e)) => {
                        return Err(e.into());
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crypto_guard_relay_fixed_nonce_prevents_ciphertext_or_framing_drift() {
        let cipher = Aes256Gcm::new(&[0u8; 32].into());
        let expected = hex::decode("000000000000000000000000cea7403d4d606b6e074ec5d3baf39d18d0d1c8a799996bf0265b98b5d48ab919").unwrap();
        assert_eq!(
            encrypt_with_nonce(&cipher, [0; 12], &[0; 16]).unwrap(),
            expected
        );
        assert_eq!(decrypt(&cipher, &expected).unwrap(), [0; 16]);
    }

    #[test]
    fn crypto_guard_relay_hkdf_prevents_salt_info_or_digest_drift() {
        assert_eq!(
            hex::encode(derive_key("test_token")),
            "1b4d1ef34de6d5b63ed99d9934cddb5ca72136151442609da11b4e631a2016c7"
        );
    }

    #[test]
    fn encrypt_decrypt_round_trip() {
        let cipher = derive_cipher("tuic_test_token_abc123");
        let plaintext = b"hello from tuicommander";
        let encrypted = encrypt(&cipher, plaintext).unwrap();
        let decrypted = decrypt(&cipher, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn decrypt_fails_with_wrong_key() {
        let c1 = derive_cipher("token_one");
        let c2 = derive_cipher("token_two");
        let encrypted = encrypt(&c1, b"secret data").unwrap();
        let result = decrypt(&c2, &encrypted);
        assert!(result.is_err());
    }

    #[test]
    fn decrypt_fails_with_short_data() {
        let cipher = derive_cipher("token");
        let result = decrypt(&cipher, &[1, 2, 3]);
        assert!(result.is_err());
    }

    #[test]
    fn different_encryptions_produce_different_ciphertexts() {
        let cipher = derive_cipher("token");
        let plaintext = b"same message";
        let e1 = encrypt(&cipher, plaintext).unwrap();
        let e2 = encrypt(&cipher, plaintext).unwrap();
        // Random nonce means different ciphertext each time
        assert_ne!(e1, e2);
        // But both decrypt to the same plaintext
        assert_eq!(decrypt(&cipher, &e1).unwrap(), plaintext);
        assert_eq!(decrypt(&cipher, &e2).unwrap(), plaintext);
    }

    #[test]
    fn derive_cipher_is_deterministic() {
        // Same token must produce same key material
        let c1 = derive_cipher("same_token");
        let c2 = derive_cipher("same_token");
        let plaintext = b"test data";
        let encrypted = encrypt(&c1, plaintext).unwrap();
        let decrypted = decrypt(&c2, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn different_tokens_produce_different_keys() {
        let c1 = derive_cipher("token_a");
        let c2 = derive_cipher("token_b");
        let encrypted = encrypt(&c1, b"test").unwrap();
        assert!(decrypt(&c2, &encrypted).is_err());
    }

    // -----------------------------------------------------------------------
    // Push-hint decision tests
    // -----------------------------------------------------------------------

    #[test]
    fn push_hint_triggers_on_question_event() {
        let parsed = serde_json::json!({ "type": "question", "prompt_text": "Allow?" });
        let (new_awaiting, should_push) = evaluate_push_hint(&parsed, None);
        assert_eq!(new_awaiting, Some(false));
        assert!(should_push);
    }

    #[test]
    fn push_hint_does_not_retrigger_while_already_awaiting() {
        let parsed = serde_json::json!({ "type": "question", "prompt_text": "Again?" });
        let (new_awaiting, should_push) = evaluate_push_hint(&parsed, Some(true));
        assert_eq!(new_awaiting, Some(false));
        assert!(!should_push, "should not push when already awaiting");
    }

    #[test]
    fn push_hint_clears_on_user_input() {
        let parsed = serde_json::json!({ "type": "user-input", "content": "yes" });
        let (new_awaiting, should_push) = evaluate_push_hint(&parsed, Some(true));
        assert_eq!(new_awaiting, None);
        assert!(!should_push);
    }

    #[test]
    fn push_hint_clears_on_question_or_choice_retraction() {
        for event_type in ["question-cleared", "choice-cleared"] {
            let parsed = serde_json::json!({ "type": event_type });
            assert_eq!(evaluate_push_hint(&parsed, Some(false)), (None, false));
        }
    }

    #[test]
    fn low_confidence_wait_clears_on_working_status_but_confident_wait_does_not() {
        let status = serde_json::json!({ "type": "status-line" });
        assert_eq!(evaluate_push_hint(&status, Some(false)), (None, false));
        assert_eq!(evaluate_push_hint(&status, Some(true)), (Some(true), false));
    }

    #[test]
    fn push_hint_preserves_state_on_unrelated_events() {
        let status = serde_json::json!({ "type": "status-line" });
        // was false → stays false
        let (aw, push) = evaluate_push_hint(&status, None);
        assert_eq!(aw, None);
        assert!(!push);
        // was true → stays true (no re-push)
        let (aw2, push2) = evaluate_push_hint(&status, Some(true));
        assert_eq!(aw2, Some(true));
        assert!(!push2);
    }

    #[test]
    fn push_hint_handles_missing_type_field() {
        let parsed = serde_json::json!({ "content": "no type" });
        let (aw, push) = evaluate_push_hint(&parsed, None);
        assert_eq!(aw, None);
        assert!(!push);
    }

    // -----------------------------------------------------------------------
    // What reaches the phone (F65)
    // -----------------------------------------------------------------------

    #[test]
    fn only_a_connected_peer_counts_as_an_audience() {
        assert!(peer_is_attached(&PeerStatus::Connected));
        // `Waiting` is a reserved slot, not a listener. Treating it as one is how
        // the whole event bus got encrypted and shipped to nobody.
        assert!(!peer_is_attached(&PeerStatus::Waiting));
        assert!(!peer_is_attached(&PeerStatus::Disconnected));
        assert!(!peer_is_attached(&PeerStatus::Timeout));
    }

    #[test]
    fn desktop_only_instructions_are_not_relayed() {
        for event in [
            AppEvent::UiTab {
                id: "panel".to_string(),
                title: "Panel".to_string(),
                html: String::new(),
                url: None,
                pinned: false,
                focus: false,
                origin_repo_path: None,
            },
            AppEvent::CloseHtmlTabs {
                tab_ids: vec!["panel".to_string()],
            },
            AppEvent::McpOAuthStart {
                name: "upstream".to_string(),
                authorization_url: "https://example.test/auth".to_string(),
            },
            AppEvent::PluginChanged {
                plugin_ids: vec!["p".to_string()],
            },
            AppEvent::DirChanged {
                dir_path: "/tmp".to_string(),
            },
        ] {
            assert!(
                !is_relayable(&event),
                "a phone cannot act on {event:?} — it must not cost an encrypt"
            );
        }
    }

    #[test]
    fn session_and_repo_events_are_relayed() {
        for event in [
            AppEvent::PtyExit {
                session_id: "s1".to_string(),
            },
            AppEvent::SessionClosed {
                session_id: "s1".to_string(),
                reason: "exit".to_string(),
            },
            AppEvent::RepoChanged {
                repo_path: "/repo".to_string(),
                kind: crate::repo_watcher::RepoChangeKind::WorkingTree,
            },
            AppEvent::PtyParsed {
                session_id: "s1".to_string(),
                parsed: serde_json::json!({ "type": "question" }).into(),
            },
        ] {
            assert!(
                is_relayable(&event),
                "{event:?} is what a remote client is watching for"
            );
        }
    }

    // -----------------------------------------------------------------------
    // Relay message deserialization tests
    // -----------------------------------------------------------------------

    #[test]
    fn deserialize_status_connected() {
        let json = r#"{"type":"relay:status","peer":"connected"}"#;
        let msg: RelayMessage = serde_json::from_str(json).unwrap();
        assert_eq!(
            msg,
            RelayMessage::Status {
                peer: PeerStatus::Connected
            }
        );
    }

    #[test]
    fn deserialize_status_disconnected() {
        let json = r#"{"type":"relay:status","peer":"disconnected"}"#;
        let msg: RelayMessage = serde_json::from_str(json).unwrap();
        assert_eq!(
            msg,
            RelayMessage::Status {
                peer: PeerStatus::Disconnected
            }
        );
    }

    #[test]
    fn deserialize_status_waiting() {
        let json = r#"{"type":"relay:status","peer":"waiting"}"#;
        let msg: RelayMessage = serde_json::from_str(json).unwrap();
        assert_eq!(
            msg,
            RelayMessage::Status {
                peer: PeerStatus::Waiting
            }
        );
    }

    #[test]
    fn deserialize_status_timeout() {
        let json = r#"{"type":"relay:status","peer":"timeout"}"#;
        let msg: RelayMessage = serde_json::from_str(json).unwrap();
        assert_eq!(
            msg,
            RelayMessage::Status {
                peer: PeerStatus::Timeout
            }
        );
    }

    #[test]
    fn deserialize_push_message() {
        let json = r#"{"type":"relay:push","reason":"awaiting_input","session_name":"s1"}"#;
        let msg: RelayMessage = serde_json::from_str(json).unwrap();
        assert_eq!(
            msg,
            RelayMessage::Push {
                reason: "awaiting_input".to_string(),
                session_name: "s1".to_string(),
            }
        );
    }

    #[test]
    fn deserialize_unknown_type_fails() {
        let json = r#"{"type":"relay:unknown","data":"foo"}"#;
        let result = serde_json::from_str::<RelayMessage>(json);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Crypto tests
    // -----------------------------------------------------------------------

    #[test]
    fn derive_key_uses_hkdf_with_salt_and_info() {
        // Verify derive_key uses HKDF-SHA256 with:
        //   salt = b"tuicommander-relay-v1"
        //   info = b"aes-256-gcm-key"
        // by comparing against a manually-computed reference.
        let expected: [u8; 32] =
            hex::decode("1b4d1ef34de6d5b63ed99d9934cddb5ca72136151442609da11b4e631a2016c7")
                .unwrap()
                .try_into()
                .unwrap();

        let reference_cipher = Aes256Gcm::new(&expected.into());
        let cipher = derive_cipher("test_token");

        // If derive_cipher uses the same HKDF params, cross-decryption works
        let plaintext = b"hkdf salt and info verification";
        let encrypted = encrypt(&reference_cipher, plaintext).unwrap();
        let decrypted = decrypt(&cipher, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    // -----------------------------------------------------------------------
    // Reconnect lifecycle
    // -----------------------------------------------------------------------

    /// A relay server that completes the handshake, reads the bearer frame and
    /// then closes — the "connects fine, drops immediately" shape that drives
    /// the reconnect path. Every accepted connection is announced on `accepted`.
    ///
    /// `authenticate` is the whole difference between the two shapes the client
    /// must tell apart. The real relay answers a peer that gets past auth with
    /// an unconditional `relay:status` and answers a bad token with a bare
    /// close, so that frame is the only evidence the handshake succeeded.
    async fn spawn_dropping_relay(
        accepted: tokio::sync::mpsc::UnboundedSender<()>,
        authenticate: bool,
    ) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test relay");
        let addr = listener.local_addr().expect("test relay address");
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                if accepted.send(()).is_err() {
                    return;
                }
                tokio::spawn(async move {
                    if let Ok(mut ws) = tokio_tungstenite::accept_async(stream).await {
                        let _ = ws.next().await; // the `Bearer <token>` frame
                        if authenticate {
                            let _ = ws
                                .send(Message::Text(
                                    r#"{"type":"relay:status","peer":"waiting"}"#.into(),
                                ))
                                .await;
                        }
                        let _ = ws.close(None).await;
                    }
                });
            }
        });
        format!("ws://{addr}")
    }

    /// Every "reconnecting in Ns" wait the relay logged, in order.
    fn logged_reconnect_waits(state: &Arc<AppState>) -> Vec<String> {
        state
            .log_buffer
            .lock()
            .get_entries(0)
            .into_iter()
            .filter(|entry| entry.source == "relay")
            .filter_map(|entry| {
                entry
                    .message
                    .split("reconnecting in ")
                    .nth(1)
                    .map(str::to_string)
            })
            .collect()
    }

    /// The clock is paused: the backoff waits are the subject, so they are read
    /// out of the log rather than measured. Nothing here bounds wall time —
    /// a genuine hang is nextest's `slow-timeout` to report, and a deadline of
    /// our own would only be able to fail for the wrong reason.
    #[tokio::test(start_paused = true)]
    async fn backoff_resets_after_a_connection_that_came_up() {
        let (accepted_tx, mut accepted_rx) = tokio::sync::mpsc::unbounded_channel();
        let url = spawn_dropping_relay(accepted_tx, true).await;

        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        {
            let mut cfg = state.config.write();
            cfg.services.relay.enabled = true;
            cfg.services.relay.url = url;
            cfg.services.relay.token = "test_relay_token".to_string();
            cfg.services.relay.session_id = "test-session".to_string();
        }

        let settings =
            RelaySettings::active(&state.config.read()).expect("relay settings are complete");
        let (stop_tx, stop_rx) = oneshot::channel();
        let client = tokio::spawn(run(state.clone(), settings, stop_rx));

        // Three attempts means two reconnects, each preceded by a backoff wait.
        for attempt in 1..=3 {
            accepted_rx
                .recv()
                .await
                .unwrap_or_else(|| panic!("relay client never made attempt {attempt}"));
        }
        let _ = stop_tx.send(());
        let _ = client.await;

        let waits = logged_reconnect_waits(&state);
        assert!(
            waits.len() >= 2,
            "expected two reconnect waits, logged {waits:?}"
        );
        assert!(
            waits.iter().all(|wait| wait == "1s"),
            "a connection that came up ends the failure streak, so every wait \
             must stay at the initial backoff; logged {waits:?}"
        );
    }

    /// The counterpart: a token the relay refuses. It reads the bearer frame and
    /// closes without ever sending a status, which is what 4003 looks like from
    /// here. Writing the frame used to count as connecting, so every rejection
    /// reset the ladder to one second and the client hammered the relay for as
    /// long as the token stayed wrong — the one failure that never self-heals.
    #[tokio::test(start_paused = true)]
    async fn a_refused_token_lets_the_backoff_grow() {
        let (accepted_tx, mut accepted_rx) = tokio::sync::mpsc::unbounded_channel();
        let url = spawn_dropping_relay(accepted_tx, false).await;

        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        {
            let mut cfg = state.config.write();
            cfg.services.relay.enabled = true;
            cfg.services.relay.url = url;
            cfg.services.relay.token = "rejected_relay_token".to_string();
            cfg.services.relay.session_id = "test-session".to_string();
        }

        let settings =
            RelaySettings::active(&state.config.read()).expect("relay settings are complete");
        let (stop_tx, stop_rx) = oneshot::channel();
        let client = tokio::spawn(run(state.clone(), settings, stop_rx));

        for attempt in 1..=3 {
            accepted_rx
                .recv()
                .await
                .unwrap_or_else(|| panic!("relay client never made attempt {attempt}"));
        }
        let _ = stop_tx.send(());
        let _ = client.await;

        let waits = logged_reconnect_waits(&state);
        assert!(
            waits.len() >= 2,
            "expected two reconnect waits, logged {waits:?}"
        );
        assert_eq!(
            &waits[..2],
            ["1s", "2s"],
            "a handshake that never completed must keep climbing the ladder; \
             logged {waits:?}"
        );
    }

    /// What the relay server saw a client do.
    #[derive(Debug, PartialEq)]
    enum PeerEvent {
        Attached,
        Detached,
    }

    /// A relay server that holds every connection open until the client goes
    /// away, reporting both edges.
    async fn spawn_holding_relay(events: tokio::sync::mpsc::UnboundedSender<PeerEvent>) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test relay");
        let addr = listener.local_addr().expect("test relay address");
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let events = events.clone();
                tokio::spawn(async move {
                    let Ok(mut ws) = tokio_tungstenite::accept_async(stream).await else {
                        return;
                    };
                    if events.send(PeerEvent::Attached).is_err() {
                        return;
                    }
                    while let Some(Ok(_)) = ws.next().await {}
                    let _ = events.send(PeerEvent::Detached);
                });
            }
        });
        format!("ws://{addr}")
    }

    fn set_relay_enabled(state: &Arc<AppState>, enabled: bool, url: &str) {
        {
            let mut cfg = state.config.write();
            cfg.services.relay.enabled = enabled;
            cfg.services.relay.url = url.to_string();
            cfg.services.relay.token = "test_relay_token".to_string();
            cfg.services.relay.session_id = "test-session".to_string();
        }
        notify_config_changed(state);
    }

    /// The Settings toggle is a config write and nothing else — the supervisor
    /// has to notice it. Every wait here is on an event the relay server
    /// actually observed, so a regression hangs and nextest reports it rather
    /// than a deadline of ours failing for an unrelated reason.
    #[tokio::test(start_paused = true)]
    async fn settings_toggle_starts_and_stops_the_client_without_a_restart() {
        let (events_tx, mut events_rx) = tokio::sync::mpsc::unbounded_channel();
        let url = spawn_holding_relay(events_tx).await;

        // Relay off at boot — the supervisor still runs, which is the whole point.
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let (stop_tx, stop_rx) = oneshot::channel();
        let supervisor = tokio::spawn(supervise(state.clone(), stop_rx));

        set_relay_enabled(&state, true, &url);
        assert_eq!(
            events_rx.recv().await,
            Some(PeerEvent::Attached),
            "enabling the relay must connect without an app restart"
        );

        set_relay_enabled(&state, false, &url);
        assert_eq!(
            events_rx.recv().await,
            Some(PeerEvent::Detached),
            "disabling the relay must drop the connection without an app restart"
        );

        // Back on again: a supervisor that stopped supervising after one toggle
        // is the same bug wearing a different hat.
        set_relay_enabled(&state, true, &url);
        assert_eq!(
            events_rx.recv().await,
            Some(PeerEvent::Attached),
            "the supervisor must keep watching after a stop"
        );

        let _ = stop_tx.send(());
        let _ = supervisor.await;
        assert_eq!(
            events_rx.recv().await,
            Some(PeerEvent::Detached),
            "app shutdown must close the relay connection"
        );
    }
}
