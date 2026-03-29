//! Web Push notification support — VAPID keys, subscription management, push sending.

use base64ct::{Base64UrlUnpadded, Encoding};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Known push service host suffixes. Endpoints not matching these are rejected (SSRF prevention).
const ALLOWED_PUSH_HOSTS: &[&str] = &[
    ".googleapis.com",   // Google FCM
    ".google.com",       // Google FCM alternate
    ".mozilla.com",      // Mozilla autopush
    ".windows.com",      // Windows Push Notification Service
    ".notify.windows.com",
    ".apple.com",        // Apple Push Notification Service
    ".push.apple.com",
    ".web.push.apple.com",
];

/// Browser PushSubscription keys (nested under "keys" in the JSON).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct PushSubscriptionKeys {
    /// Base64url-encoded P-256 public key from the browser
    pub p256dh: String,
    /// Base64url-encoded authentication secret from the browser
    pub auth: String,
}

/// A push subscription from a browser's PushManager.subscribe().
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct PushSubscription {
    pub endpoint: String,
    pub keys: PushSubscriptionKeys,
    #[serde(default = "chrono::Utc::now")]
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Validate that a push endpoint URL is safe to send to.
/// Rejects non-HTTPS endpoints and hosts not in the known push service allowlist.
pub(crate) fn validate_push_endpoint(endpoint: &str) -> Result<(), String> {
    let uri: axum::http::Uri = endpoint
        .parse()
        .map_err(|_| "Invalid endpoint URL".to_string())?;

    if uri.scheme_str() != Some("https") {
        return Err("Push endpoint must use HTTPS".to_string());
    }

    let host = uri.host().ok_or("Push endpoint has no host")?;
    let host_lower = host.to_lowercase();

    if !ALLOWED_PUSH_HOSTS.iter().any(|suffix| host_lower.ends_with(suffix)) {
        return Err(format!("Push endpoint host '{host}' is not a known push service"));
    }

    Ok(())
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
                Ok(content) => serde_json::from_str(&content).unwrap_or_else(|e| {
                    tracing::warn!(source = "push", path = %path.display(), "Corrupt subscriptions file, starting empty: {e}");
                    Vec::new()
                }),
                Err(e) => {
                    tracing::warn!(source = "push", path = %path.display(), "Cannot read subscriptions file: {e}");
                    Vec::new()
                }
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
        let snapshot = {
            let mut subs = self.subs.write();
            if let Some(existing) = subs.iter_mut().find(|s| s.endpoint == sub.endpoint) {
                existing.keys.p256dh = sub.keys.p256dh;
                existing.keys.auth = sub.keys.auth;
            } else {
                subs.push(sub);
            }
            subs.clone()
        };
        self.persist(&snapshot);
    }

    /// Remove a subscription by endpoint.
    pub fn remove(&self, endpoint: &str) -> bool {
        let (removed, snapshot) = {
            let mut subs = self.subs.write();
            let before = subs.len();
            subs.retain(|s| s.endpoint != endpoint);
            let removed = subs.len() < before;
            (removed, if removed { Some(subs.clone()) } else { None })
        };
        if let Some(snapshot) = snapshot {
            self.persist(&snapshot);
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
        match serde_json::to_string_pretty(subs) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&self.path, &json) {
                    tracing::error!(source = "push", path = %self.path.display(), "Failed to persist subscriptions: {e}");
                }
            }
            Err(e) => {
                tracing::error!(source = "push", "Failed to serialize subscriptions: {e}");
            }
        }
    }
}

/// Generate a new ES256 VAPID key pair for Web Push.
/// Returns (private_key_base64url, public_key_base64url).
pub(crate) fn generate_vapid_keys() -> Result<(String, String), String> {
    use web_push_native::jwt_simple::algorithms::ES256KeyPair;
    use web_push_native::p256::elliptic_curve::sec1::ToEncodedPoint;

    let kp = ES256KeyPair::generate();
    let private_bytes = kp.to_bytes();
    let private_b64 = Base64UrlUnpadded::encode_string(&private_bytes);

    // ES256PublicKey::to_bytes() returns compressed (33 bytes); VAPID needs
    // uncompressed (65 bytes). Re-parse via p256 crate and decompress.
    let compressed = kp.public_key().to_bytes();
    let p256_key = web_push_native::p256::PublicKey::from_sec1_bytes(&compressed)
        .map_err(|e| format!("Failed to parse public key: {e}"))?;
    let uncompressed = p256_key.to_encoded_point(false);
    let public_b64 = Base64UrlUnpadded::encode_string(uncompressed.as_bytes());

    Ok((private_b64, public_b64))
}

/// Send a push notification to a list of subscriptions.
/// Returns endpoints that should be removed (410 Gone = revoked).
pub(crate) async fn send_push_batch(
    subs: Vec<PushSubscription>,
    config: &crate::config::AppConfig,
    http_client: &reqwest::Client,
    title: &str,
    body: &str,
    url: &str,
) -> Vec<String> {
    use web_push_native::jwt_simple::algorithms::ES256KeyPair;

    let mut stale_endpoints: Vec<String> = Vec::new();

    if !config.push_enabled || config.vapid_private_key.is_empty() || subs.is_empty() {
        return stale_endpoints;
    }

    let payload = serde_json::json!({ "title": title, "body": body, "url": url });
    let payload_bytes = payload.to_string().into_bytes();

    let kp_bytes = match Base64UrlUnpadded::decode_vec(&config.vapid_private_key) {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(source = "push", "Invalid VAPID private key encoding: {e}");
            return stale_endpoints;
        }
    };
    let vapid_kp = match ES256KeyPair::from_bytes(&kp_bytes) {
        Ok(kp) => kp,
        Err(e) => {
            tracing::error!(source = "push", "Failed to load VAPID key pair: {e}");
            return stale_endpoints;
        }
    };

    // Send to all subscriptions concurrently
    let futures: Vec<_> = subs
        .iter()
        .filter_map(|sub| {
            build_push_request(sub, &vapid_kp, &config.vapid_subject, &payload_bytes)
        })
        .map(|(endpoint, request)| {
            let client = http_client.clone();
            async move {
                let (parts, body) = request.into_parts();
                let uri = parts.uri.to_string();

                let mut req_builder = client.post(&uri);
                for (name, value) in &parts.headers {
                    if let Ok(v) = value.to_str() {
                        req_builder = req_builder.header(name.as_str(), v);
                    }
                }

                match req_builder.body(body).send().await {
                    Ok(resp) if resp.status().as_u16() == 410 => {
                        tracing::info!(source = "push", "Subscription revoked (410 Gone), removing");
                        Some(endpoint)
                    }
                    Ok(resp) if resp.status().is_success() || resp.status().as_u16() == 201 => {
                        tracing::debug!(source = "push", "Push sent successfully");
                        None
                    }
                    Ok(resp) => {
                        tracing::warn!(source = "push", status = resp.status().as_u16(), "Push delivery failed");
                        None
                    }
                    Err(e) => {
                        tracing::warn!(source = "push", "Push request error: {e}");
                        None
                    }
                }
            }
        })
        .collect();

    let results = futures_util::future::join_all(futures).await;
    for result in results.into_iter().flatten() {
        stale_endpoints.push(result);
    }

    stale_endpoints
}

/// Build a single push request for a subscription. Returns None if the subscription is invalid.
fn build_push_request(
    sub: &PushSubscription,
    vapid_kp: &web_push_native::jwt_simple::algorithms::ES256KeyPair,
    vapid_subject: &str,
    payload_bytes: &[u8],
) -> Option<(String, axum::http::Request<Vec<u8>>)> {
    use web_push_native::p256;

    let p256dh_bytes = match Base64UrlUnpadded::decode_vec(&sub.keys.p256dh) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(source = "push", endpoint = %sub.endpoint, "Invalid p256dh encoding: {e}");
            return None;
        }
    };
    let auth_bytes = match Base64UrlUnpadded::decode_vec(&sub.keys.auth) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(source = "push", endpoint = %sub.endpoint, "Invalid auth encoding: {e}");
            return None;
        }
    };
    let endpoint: axum::http::Uri = match sub.endpoint.parse() {
        Ok(u) => u,
        Err(e) => {
            tracing::warn!(source = "push", "Invalid endpoint URI: {e}");
            return None;
        }
    };
    let ua_public = match p256::PublicKey::from_sec1_bytes(&p256dh_bytes) {
        Ok(pk) => pk,
        Err(e) => {
            tracing::warn!(source = "push", endpoint = %sub.endpoint, "Invalid p256dh key: {e}");
            return None;
        }
    };
    if auth_bytes.len() != 16 {
        tracing::warn!(source = "push", endpoint = %sub.endpoint, "Invalid auth secret length ({})", auth_bytes.len());
        return None;
    }
    let ua_auth: web_push_native::Auth = {
        let mut arr = [0u8; 16];
        arr.copy_from_slice(&auth_bytes);
        arr.into()
    };

    let builder = web_push_native::WebPushBuilder::new(endpoint, ua_public, ua_auth)
        .with_vapid(vapid_kp, vapid_subject);

    match builder.build(payload_bytes.to_vec()) {
        Ok(request) => Some((sub.endpoint.clone(), request)),
        Err(e) => {
            tracing::warn!(source = "push", "Failed to build push request: {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_store_crud() {
        let dir = tempfile::tempdir().unwrap();
        let store = PushStore::load(dir.path());
        assert!(store.is_empty());

        let sub = PushSubscription {
            endpoint: "https://fcm.googleapis.com/fcm/send/test".to_string(),
            keys: PushSubscriptionKeys { p256dh: "test-key".to_string(), auth: "test-auth".to_string() },
            created_at: chrono::Utc::now(),
        };
        store.upsert(sub.clone());
        assert_eq!(store.list().len(), 1);

        // Upsert same endpoint updates instead of duplicating
        let mut sub2 = sub.clone();
        sub2.keys.p256dh = "updated-key".to_string();
        store.upsert(sub2);
        assert_eq!(store.list().len(), 1);
        assert_eq!(store.list()[0].keys.p256dh, "updated-key");

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
                keys: PushSubscriptionKeys { p256dh: "key".to_string(), auth: "auth".to_string() },
                created_at: chrono::Utc::now(),
            });
        }
        // Re-load from disk
        let store = PushStore::load(dir.path());
        assert_eq!(store.list().len(), 1);
        assert_eq!(store.list()[0].endpoint, "https://example.com/push");
    }

    #[test]
    fn validate_push_endpoint_accepts_known_services() {
        assert!(validate_push_endpoint("https://fcm.googleapis.com/fcm/send/abc").is_ok());
        assert!(validate_push_endpoint("https://updates.push.services.mozilla.com/wpush/v2/abc").is_ok());
        assert!(validate_push_endpoint("https://wns2-par02p.notify.windows.com/w/?token=abc").is_ok());
        assert!(validate_push_endpoint("https://web.push.apple.com/abc").is_ok());
    }

    #[test]
    fn validate_push_endpoint_rejects_unknown_hosts() {
        assert!(validate_push_endpoint("https://evil.com/steal").is_err());
        assert!(validate_push_endpoint("https://169.254.169.254/metadata").is_err());
        assert!(validate_push_endpoint("http://fcm.googleapis.com/fcm/send/abc").is_err()); // http not https
        assert!(validate_push_endpoint("https://10.0.0.1/internal").is_err());
    }
}
