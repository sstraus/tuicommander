//! Web Push notification support — VAPID keys, subscription management, push sending.

use base64ct::{Base64UrlUnpadded, Encoding};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A push subscription from a browser's PushManager.subscribe().
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct PushSubscription {
    pub endpoint: String,
    /// Base64url-encoded P-256 public key from the browser
    pub p256dh: String,
    /// Base64url-encoded authentication secret from the browser
    pub auth: String,
    #[serde(default = "chrono::Utc::now")]
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// In-memory subscription store backed by a JSON file.
pub(crate) struct PushStore {
    path: PathBuf,
    subs: parking_lot::RwLock<Vec<PushSubscription>>,
}

impl PushStore {
    /// Load subscriptions from disk (or create empty store).
    pub fn load(config_dir: &std::path::Path) -> Self {
        let path = config_dir.join("push_subscriptions.json");
        let subs = if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
                Err(_) => Vec::new(),
            }
        } else {
            Vec::new()
        };
        Self {
            path,
            subs: parking_lot::RwLock::new(subs),
        }
    }

    /// Add or update a subscription (idempotent by endpoint).
    pub fn upsert(&self, sub: PushSubscription) {
        let mut subs = self.subs.write();
        if let Some(existing) = subs.iter_mut().find(|s| s.endpoint == sub.endpoint) {
            existing.p256dh = sub.p256dh;
            existing.auth = sub.auth;
        } else {
            subs.push(sub);
        }
        self.persist(&subs);
    }

    /// Remove a subscription by endpoint.
    pub fn remove(&self, endpoint: &str) -> bool {
        let mut subs = self.subs.write();
        let before = subs.len();
        subs.retain(|s| s.endpoint != endpoint);
        let removed = subs.len() < before;
        if removed {
            self.persist(&subs);
        }
        removed
    }

    /// Get all active subscriptions.
    pub fn list(&self) -> Vec<PushSubscription> {
        self.subs.read().clone()
    }

    /// Check if any subscriptions exist.
    pub fn is_empty(&self) -> bool {
        self.subs.read().is_empty()
    }

    fn persist(&self, subs: &[PushSubscription]) {
        if let Ok(json) = serde_json::to_string_pretty(subs) {
            let _ = std::fs::write(&self.path, json);
        }
    }
}

/// Generate a VAPID ES256 key pair using jwt_simple (same lib web-push-native uses).
/// Returns (private_key_base64url, public_key_base64url).
pub(crate) fn generate_vapid_keys() -> anyhow::Result<(String, String)> {
    use web_push_native::jwt_simple::algorithms::{
        ECDSAP256KeyPairLike, ECDSAP256PublicKeyLike, ES256KeyPair,
    };

    let kp = ES256KeyPair::generate();
    let private_b64 = Base64UrlUnpadded::encode_string(&kp.to_bytes());
    // Uncompressed P-256 public key (65 bytes)
    let public_b64 = Base64UrlUnpadded::encode_string(
        &kp.public_key().public_key().to_bytes_uncompressed(),
    );

    Ok((private_b64, public_b64))
}

/// Send a push notification to all active subscriptions.
/// Removes subscriptions that return HTTP 410 Gone (revoked).
pub(crate) async fn send_push_to_all(
    store: &PushStore,
    config: &crate::config::AppConfig,
    title: &str,
    body: &str,
    url: &str,
) {
    use web_push_native::jwt_simple::algorithms::ES256KeyPair;
    use web_push_native::p256;

    if !config.push_enabled || config.vapid_private_key.is_empty() {
        return;
    }

    let subs = store.list();
    if subs.is_empty() {
        return;
    }

    let payload = serde_json::json!({
        "title": title,
        "body": body,
        "url": url,
    });
    let payload_bytes = payload.to_string().into_bytes();

    // Reconstruct VAPID ES256 key pair
    let kp_bytes = match Base64UrlUnpadded::decode_vec(&config.vapid_private_key) {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(source = "push", "Invalid VAPID private key encoding: {e}");
            return;
        }
    };
    let vapid_kp = match ES256KeyPair::from_bytes(&kp_bytes) {
        Ok(kp) => kp,
        Err(e) => {
            tracing::error!(source = "push", "Failed to load VAPID key pair: {e}");
            return;
        }
    };

    let http_client = reqwest::Client::new();
    let mut stale_endpoints: Vec<String> = Vec::new();

    for sub in &subs {
        // Decode browser subscription keys
        let p256dh_bytes = match Base64UrlUnpadded::decode_vec(&sub.p256dh) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let auth_bytes = match Base64UrlUnpadded::decode_vec(&sub.auth) {
            Ok(b) => b,
            Err(_) => continue,
        };

        // Parse endpoint URI
        let endpoint: axum::http::Uri = match sub.endpoint.parse() {
            Ok(u) => u,
            Err(_) => continue,
        };

        // Parse p256dh as a P-256 public key
        let ua_public = match p256::PublicKey::from_sec1_bytes(&p256dh_bytes) {
            Ok(pk) => pk,
            Err(_) => {
                tracing::warn!(source = "push", "Invalid p256dh key, skipping");
                continue;
            }
        };

        // Auth secret must be exactly 16 bytes
        if auth_bytes.len() != 16 {
            tracing::warn!(source = "push", "Invalid auth secret length ({}), skipping", auth_bytes.len());
            continue;
        }
        let ua_auth: web_push_native::Auth = {
            let mut arr = [0u8; 16];
            arr.copy_from_slice(&auth_bytes);
            arr.into()
        };

        let builder = web_push_native::WebPushBuilder::new(endpoint, ua_public, ua_auth)
            .with_vapid(&vapid_kp, &config.vapid_subject);

        match builder.build(payload_bytes.clone()) {
            Ok(request) => {
                let (parts, body) = request.into_parts();
                let uri = parts.uri.to_string();

                let mut req_builder = http_client.post(&uri);
                for (name, value) in &parts.headers {
                    if let Ok(v) = value.to_str() {
                        req_builder = req_builder.header(name.as_str(), v);
                    }
                }

                match req_builder.body(body).send().await {
                    Ok(resp) if resp.status().as_u16() == 410 => {
                        tracing::info!(source = "push", "Subscription revoked (410 Gone), removing");
                        stale_endpoints.push(sub.endpoint.clone());
                    }
                    Ok(resp) if resp.status().is_success() || resp.status().as_u16() == 201 => {
                        tracing::debug!(source = "push", "Push sent successfully");
                    }
                    Ok(resp) => {
                        tracing::warn!(source = "push", status = resp.status().as_u16(), "Push delivery failed");
                    }
                    Err(e) => {
                        tracing::warn!(source = "push", "Push request error: {e}");
                    }
                }
            }
            Err(e) => {
                tracing::warn!(source = "push", "Failed to build push request: {e}");
            }
        }
    }

    // Clean up stale subscriptions
    for endpoint in stale_endpoints {
        store.remove(&endpoint);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vapid_key_generation_roundtrip() {
        use web_push_native::jwt_simple::algorithms::ES256KeyPair;

        let (private, public) = generate_vapid_keys().unwrap();
        assert!(!private.is_empty());
        assert!(!public.is_empty());

        // Public key should be 65 bytes (uncompressed P-256 point)
        let pub_bytes = Base64UrlUnpadded::decode_vec(&public).unwrap();
        assert_eq!(pub_bytes.len(), 65);

        // Private key can be loaded back into an ES256KeyPair
        let priv_bytes = Base64UrlUnpadded::decode_vec(&private).unwrap();
        let kp = ES256KeyPair::from_bytes(&priv_bytes);
        assert!(kp.is_ok(), "Should roundtrip key pair");
    }

    #[test]
    fn push_store_crud() {
        let dir = tempfile::tempdir().unwrap();
        let store = PushStore::load(dir.path());
        assert!(store.is_empty());

        let sub = PushSubscription {
            endpoint: "https://fcm.googleapis.com/fcm/send/test".to_string(),
            p256dh: "test-key".to_string(),
            auth: "test-auth".to_string(),
            created_at: chrono::Utc::now(),
        };
        store.upsert(sub.clone());
        assert_eq!(store.list().len(), 1);

        // Upsert same endpoint updates instead of duplicating
        let mut sub2 = sub.clone();
        sub2.p256dh = "updated-key".to_string();
        store.upsert(sub2);
        assert_eq!(store.list().len(), 1);
        assert_eq!(store.list()[0].p256dh, "updated-key");

        // Remove
        assert!(store.remove(&sub.endpoint));
        assert!(store.is_empty());
        assert!(!store.remove("nonexistent"));
    }

    #[test]
    fn push_store_persistence() {
        let dir = tempfile::tempdir().unwrap();
        {
            let store = PushStore::load(dir.path());
            store.upsert(PushSubscription {
                endpoint: "https://example.com/push".to_string(),
                p256dh: "key".to_string(),
                auth: "auth".to_string(),
                created_at: chrono::Utc::now(),
            });
        }
        // Re-load from disk
        let store = PushStore::load(dir.path());
        assert_eq!(store.list().len(), 1);
        assert_eq!(store.list()[0].endpoint, "https://example.com/push");
    }
}
