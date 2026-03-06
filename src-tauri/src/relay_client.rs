//! Relay client: connects to the cloud relay server via WSS, bridging
//! E2E-encrypted messages between TUICommander's event bus and the mobile PWA.

use std::sync::Arc;
use std::time::Duration;

use aes_gcm::aead::{Aead, OsRng};
use aes_gcm::{Aes256Gcm, AeadCore, KeyInit};
use futures_util::{SinkExt, StreamExt};
use hkdf::Hkdf;
use sha2::Sha256;
use tokio::sync::oneshot;
use tokio_tungstenite::tungstenite::Message;

use crate::state::{AppEvent, AppState};

// ---------------------------------------------------------------------------
// E2E Encryption (AES-256-GCM with HKDF key derivation)
// ---------------------------------------------------------------------------

/// Derive a 256-bit AES key from the relay token using HKDF-SHA-256.
///
/// BREAKING CHANGE: mobile clients must update their key derivation to use the
/// same HKDF parameters (salt + info) or they will fail to decrypt messages.
fn derive_cipher(relay_token: &str) -> Aes256Gcm {
    let hk = Hkdf::<Sha256>::new(Some(b"tuicommander-relay-v1"), relay_token.as_bytes());
    let mut okm = [0u8; 32];
    hk.expand(b"aes-256-gcm-key", &mut okm)
        .expect("HKDF-SHA256 expand for 32 bytes always succeeds");
    Aes256Gcm::new(aes_gcm::Key::<Aes256Gcm>::from_slice(&okm))
}

/// Encrypt plaintext with AES-256-GCM. Returns nonce (12 bytes) || ciphertext.
fn encrypt(cipher: &Aes256Gcm, plaintext: &[u8]) -> anyhow::Result<Vec<u8>> {
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|e| anyhow::anyhow!("encryption failed: {e}"))?;
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
    let nonce = aes_gcm::Nonce::from_slice(nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| anyhow::anyhow!("decryption failed: {e}"))?;
    Ok(plaintext)
}

// ---------------------------------------------------------------------------
// Push-hint decision logic
// ---------------------------------------------------------------------------

/// Evaluate a parsed PTY event and determine the new `awaiting_input` state.
/// Returns `(new_awaiting, should_push)` where `should_push` is true when the
/// agent transitions from working to awaiting input (i.e. a "question" event
/// while not already awaiting).
fn evaluate_push_hint(parsed: &serde_json::Value, was_awaiting: bool) -> (bool, bool) {
    let event_type = parsed
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let new_awaiting = match event_type {
        "question" => true,
        "user-input" => false,
        _ => was_awaiting,
    };
    let should_push = new_awaiting && !was_awaiting;
    (new_awaiting, should_push)
}

// ---------------------------------------------------------------------------
// Relay client lifecycle
// ---------------------------------------------------------------------------

/// Maximum backoff between reconnection attempts.
const MAX_BACKOFF: Duration = Duration::from_secs(60);

/// Initial backoff between reconnection attempts.
const INITIAL_BACKOFF: Duration = Duration::from_secs(1);

/// Start the relay client. Connects to the relay server and bridges events.
/// Returns when the shutdown signal is received.
pub(crate) async fn run(state: Arc<AppState>, mut shutdown_rx: oneshot::Receiver<()>) {
    let config = state.config.read().clone();
    if !config.relay_enabled || config.relay_url.is_empty() || config.relay_token.is_empty() {
        eprintln!("[relay] disabled or not configured");
        return;
    }

    let cipher = derive_cipher(&config.relay_token);
    let ws_url = format!("{}/ws/{}", config.relay_url, config.relay_session_id);
    let mut backoff = INITIAL_BACKOFF;

    loop {
        eprintln!("[relay] connecting to {ws_url}");

        match connect_and_run(&state, &ws_url, &config.relay_token, &cipher, &mut shutdown_rx).await {
            Ok(ShutdownReason::Signal) => {
                eprintln!("[relay] shutting down");
                return;
            }
            Ok(ShutdownReason::Disconnected) => {
                eprintln!("[relay] disconnected, reconnecting in {}s", backoff.as_secs());
            }
            Err(e) => {
                eprintln!("[relay] connection error: {e}, reconnecting in {}s", backoff.as_secs());
            }
        }

        // Wait for backoff or shutdown
        tokio::select! {
            _ = tokio::time::sleep(backoff) => {}
            _ = &mut shutdown_rx => {
                eprintln!("[relay] shutting down during backoff");
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
    let (ws_stream, _) = tokio_tungstenite::connect_async(ws_url).await?;
    let (mut ws_sink, mut ws_source) = ws_stream.split();

    // Authenticate: send bearer token as first text message
    ws_sink
        .send(Message::Text(format!("Bearer {relay_token}").into()))
        .await?;

    eprintln!("[relay] authenticated, starting event bridge");

    // Subscribe to event bus — reset backoff on successful connection
    let mut event_rx = state.event_bus.subscribe();
    let mut awaiting_input = false;

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
                            let (new_awaiting, should_push) =
                                evaluate_push_hint(parsed, awaiting_input);
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
                            awaiting_input = new_awaiting;
                        }

                        // Encrypt and forward event
                        let json = serde_json::to_vec(evt)?;
                        let encrypted = encrypt(cipher, &json)?;
                        if ws_sink.send(Message::Binary(encrypted.into())).await.is_err() {
                            return Ok(ShutdownReason::Disconnected);
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        eprintln!("[relay] event bus lagged, skipped {n} events");
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
                                eprintln!("[relay] received mobile command (dispatch not yet implemented)");
                            }
                            Err(e) => {
                                eprintln!("[relay] failed to decrypt message: {e}");
                            }
                        }
                    }
                    Some(Ok(Message::Text(text))) => {
                        // Status messages from relay (relay:status)
                        if text.contains("connected") {
                            eprintln!("[relay] mobile peer connected");
                        } else if text.contains("disconnected") {
                            eprintln!("[relay] mobile peer disconnected");
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
    use aes_gcm::KeyInit;

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
        let (new_awaiting, should_push) = evaluate_push_hint(&parsed, false);
        assert!(new_awaiting);
        assert!(should_push);
    }

    #[test]
    fn push_hint_does_not_retrigger_while_already_awaiting() {
        let parsed = serde_json::json!({ "type": "question", "prompt_text": "Again?" });
        let (new_awaiting, should_push) = evaluate_push_hint(&parsed, true);
        assert!(new_awaiting);
        assert!(!should_push, "should not push when already awaiting");
    }

    #[test]
    fn push_hint_clears_on_user_input() {
        let parsed = serde_json::json!({ "type": "user-input", "content": "yes" });
        let (new_awaiting, should_push) = evaluate_push_hint(&parsed, true);
        assert!(!new_awaiting);
        assert!(!should_push);
    }

    #[test]
    fn push_hint_preserves_state_on_unrelated_events() {
        let status = serde_json::json!({ "type": "status-line" });
        // was false → stays false
        let (aw, push) = evaluate_push_hint(&status, false);
        assert!(!aw);
        assert!(!push);
        // was true → stays true (no re-push)
        let (aw2, push2) = evaluate_push_hint(&status, true);
        assert!(aw2);
        assert!(!push2);
    }

    #[test]
    fn push_hint_handles_missing_type_field() {
        let parsed = serde_json::json!({ "content": "no type" });
        let (aw, push) = evaluate_push_hint(&parsed, false);
        assert!(!aw);
        assert!(!push);
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
        let hk = Hkdf::<Sha256>::new(
            Some(b"tuicommander-relay-v1"),
            b"test_token",
        );
        let mut expected = [0u8; 32];
        hk.expand(b"aes-256-gcm-key", &mut expected).unwrap();

        let reference_cipher =
            Aes256Gcm::new(aes_gcm::Key::<Aes256Gcm>::from_slice(&expected));
        let cipher = derive_cipher("test_token");

        // If derive_cipher uses the same HKDF params, cross-decryption works
        let plaintext = b"hkdf salt and info verification";
        let encrypted = encrypt(&reference_cipher, plaintext).unwrap();
        let decrypted = decrypt(&cipher, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }
}
