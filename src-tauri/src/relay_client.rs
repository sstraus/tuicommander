//! Relay client: connects to the cloud relay server via WSS, bridging
//! E2E-encrypted messages between TUICommander's event bus and the mobile PWA.

use std::sync::Arc;
use std::time::Duration;

use aes_gcm::aead::{Aead, OsRng};
use aes_gcm::{Aes256Gcm, AeadCore, KeyInit};
use futures_util::{SinkExt, StreamExt};
use sha2::{Digest, Sha256};
use tokio::sync::oneshot;
use tokio_tungstenite::tungstenite::Message;

use crate::state::{AppEvent, AppState};

// ---------------------------------------------------------------------------
// E2E Encryption (AES-256-GCM)
// ---------------------------------------------------------------------------

/// Derive a 256-bit AES key from the relay token using SHA-256.
fn derive_key(relay_token: &str) -> aes_gcm::Key<Aes256Gcm> {
    let hash = Sha256::digest(relay_token.as_bytes());
    *aes_gcm::Key::<Aes256Gcm>::from_slice(&hash)
}

/// Encrypt plaintext with AES-256-GCM. Returns nonce (12 bytes) || ciphertext.
fn encrypt(key: &aes_gcm::Key<Aes256Gcm>, plaintext: &[u8]) -> anyhow::Result<Vec<u8>> {
    let cipher = Aes256Gcm::new(key);
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
fn decrypt(key: &aes_gcm::Key<Aes256Gcm>, data: &[u8]) -> anyhow::Result<Vec<u8>> {
    if data.len() < 12 {
        anyhow::bail!("ciphertext too short (missing nonce)");
    }
    let (nonce_bytes, ciphertext) = data.split_at(12);
    let nonce = aes_gcm::Nonce::from_slice(nonce_bytes);
    let cipher = Aes256Gcm::new(key);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| anyhow::anyhow!("decryption failed: {e}"))?;
    Ok(plaintext)
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

    let key = derive_key(&config.relay_token);
    let ws_url = format!("{}/ws/{}", config.relay_url, config.relay_session_id);
    let mut backoff = INITIAL_BACKOFF;

    loop {
        eprintln!("[relay] connecting to {ws_url}");

        match connect_and_run(&state, &ws_url, &config.relay_token, &key, &mut shutdown_rx).await {
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
    key: &aes_gcm::Key<Aes256Gcm>,
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
                        // Check for awaiting_input transition → send push hint
                        if let AppEvent::PtyParsed { parsed, session_id, .. } = evt {
                            let new_awaiting = parsed.get("awaiting_input")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);
                            if new_awaiting && !awaiting_input {
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
                        let encrypted = encrypt(key, &json)?;
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
                        match decrypt(key, &data) {
                            Ok(_plaintext) => {
                                // Future: dispatch decrypted message as HTTP request
                                // to local server or handle as mobile command
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

    #[test]
    fn encrypt_decrypt_round_trip() {
        let key = derive_key("tuic_test_token_abc123");
        let plaintext = b"hello from tuicommander";
        let encrypted = encrypt(&key, plaintext).unwrap();
        let decrypted = decrypt(&key, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn decrypt_fails_with_wrong_key() {
        let key1 = derive_key("token_one");
        let key2 = derive_key("token_two");
        let encrypted = encrypt(&key1, b"secret data").unwrap();
        let result = decrypt(&key2, &encrypted);
        assert!(result.is_err());
    }

    #[test]
    fn decrypt_fails_with_short_data() {
        let key = derive_key("token");
        let result = decrypt(&key, &[1, 2, 3]);
        assert!(result.is_err());
    }

    #[test]
    fn different_encryptions_produce_different_ciphertexts() {
        let key = derive_key("token");
        let plaintext = b"same message";
        let e1 = encrypt(&key, plaintext).unwrap();
        let e2 = encrypt(&key, plaintext).unwrap();
        // Random nonce means different ciphertext each time
        assert_ne!(e1, e2);
        // But both decrypt to the same plaintext
        assert_eq!(decrypt(&key, &e1).unwrap(), plaintext);
        assert_eq!(decrypt(&key, &e2).unwrap(), plaintext);
    }

    #[test]
    fn derive_key_is_deterministic() {
        let k1 = derive_key("same_token");
        let k2 = derive_key("same_token");
        assert_eq!(k1, k2);
    }

    #[test]
    fn different_tokens_produce_different_keys() {
        let k1 = derive_key("token_a");
        let k2 = derive_key("token_b");
        assert_ne!(k1, k2);
    }
}
