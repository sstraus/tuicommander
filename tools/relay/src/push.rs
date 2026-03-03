use anyhow::{Context, Result};
use base64ct::{Base64UrlUnpadded, Encoding};
use tracing::{info, warn};
use web_push_native::jwt_simple::algorithms::ES256KeyPair;
use web_push_native::{p256, Auth, WebPushBuilder};

use crate::types::PushSubscription;

/// VAPID configuration for sending Web Push notifications.
pub struct VapidConfig {
    /// ES256 key pair for VAPID signing.
    key_pair: ES256KeyPair,
    /// Contact URI (mailto: or https:) for the VAPID subject claim.
    subject: String,
}

impl VapidConfig {
    /// Create from a base64url-encoded ES256 private key and a subject URI.
    pub fn new(private_key_base64: &str, subject: &str) -> Result<Self> {
        let key_bytes = Base64UrlUnpadded::decode_vec(private_key_base64)
            .map_err(|e| anyhow::anyhow!("invalid base64url VAPID key: {e}"))?;
        let key_pair = ES256KeyPair::from_bytes(&key_bytes)
            .map_err(|e| anyhow::anyhow!("invalid ES256 private key: {e}"))?;
        Ok(Self {
            key_pair,
            subject: subject.to_owned(),
        })
    }

    /// Get the VAPID public key as base64url-encoded uncompressed bytes
    /// (the value clients need for `applicationServerKey` in `pushManager.subscribe`).
    pub fn public_key_base64(&self) -> String {
        use web_push_native::jwt_simple::algorithms::ECDSAP256PublicKeyLike;
        let pk = self.key_pair.public_key();
        Base64UrlUnpadded::encode_string(&pk.public_key().to_bytes_uncompressed())
    }
}

/// Parse browser push subscription fields into typed values for WebPushBuilder.
fn parse_subscription(
    sub: &PushSubscription,
) -> Result<(http::Uri, p256::PublicKey, Auth)> {
    let endpoint: http::Uri = sub
        .endpoint
        .parse()
        .context("invalid push endpoint URL")?;
    let p256dh_bytes = Base64UrlUnpadded::decode_vec(&sub.p256dh)
        .context("invalid p256dh base64url")?;
    let ua_public = p256::PublicKey::from_sec1_bytes(&p256dh_bytes)
        .map_err(|e| anyhow::anyhow!("invalid p256dh public key: {e}"))?;
    let auth_bytes = Base64UrlUnpadded::decode_vec(&sub.auth)
        .context("invalid auth base64url")?;
    let ua_auth = Auth::clone_from_slice(&auth_bytes);
    Ok((endpoint, ua_public, ua_auth))
}

/// Build an HTTP request for a Web Push notification.
pub fn build_push_request(
    vapid: &VapidConfig,
    sub: &PushSubscription,
    payload: &[u8],
) -> Result<http::Request<Vec<u8>>> {
    let (endpoint, ua_public, ua_auth) = parse_subscription(sub)?;
    let builder = WebPushBuilder::new(endpoint, ua_public, ua_auth)
        .with_vapid(&vapid.key_pair, &vapid.subject);
    let request = builder
        .build(payload.to_vec())
        .map_err(|e| anyhow::anyhow!("web push build error: {e}"))?;
    Ok(request)
}

/// Send a Web Push notification to a single subscription.
/// Returns `Ok(true)` if sent, `Ok(false)` if the endpoint is gone (caller should delete sub).
pub async fn send_push(
    client: &reqwest::Client,
    vapid: &VapidConfig,
    sub: &PushSubscription,
    payload: &[u8],
) -> Result<bool> {
    let request = match build_push_request(vapid, sub, payload) {
        Ok(r) => r,
        Err(e) => {
            warn!(endpoint = %sub.endpoint, error = %e, "failed to build push request");
            return Ok(false);
        }
    };

    let (parts, body) = request.into_parts();
    let mut reqwest_request = client.request(parts.method, parts.uri.to_string());
    for (name, value) in &parts.headers {
        reqwest_request = reqwest_request.header(name, value);
    }

    let response = reqwest_request.body(body).send().await?;
    let status = response.status();

    if status.is_success() {
        info!(endpoint = %sub.endpoint, "push notification sent");
        Ok(true)
    } else if status == reqwest::StatusCode::GONE || status == reqwest::StatusCode::NOT_FOUND {
        info!(endpoint = %sub.endpoint, %status, "push endpoint gone");
        Ok(false)
    } else {
        let body = response.text().await.unwrap_or_default();
        warn!(endpoint = %sub.endpoint, %status, %body, "push notification failed");
        Ok(true) // keep subscription, might be transient
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::elliptic_curve::sec1::ToEncodedPoint;
    /// Generate a throwaway VAPID key pair, return base64url-encoded private key.
    fn generate_vapid_key_base64() -> String {
        let kp = ES256KeyPair::generate();
        Base64UrlUnpadded::encode_string(&kp.to_bytes())
    }

    /// Generate a fake browser subscription with valid crypto keys.
    fn fake_subscription() -> PushSubscription {
        let secret = p256::SecretKey::random(&mut p256::elliptic_curve::rand_core::OsRng);
        let public = secret.public_key();
        let p256dh = Base64UrlUnpadded::encode_string(
            public.as_affine().to_encoded_point(false).as_bytes(),
        );
        let auth = Base64UrlUnpadded::encode_string(&[0u8; 16]);

        PushSubscription {
            endpoint: "https://fcm.googleapis.com/fcm/send/fake-id".to_string(),
            p256dh,
            auth,
        }
    }

    #[test]
    fn vapid_config_from_base64() {
        let b64 = generate_vapid_key_base64();
        let config = VapidConfig::new(&b64, "mailto:test@example.com").unwrap();
        let pub_key = config.public_key_base64();
        assert!(!pub_key.is_empty());
    }

    #[test]
    fn vapid_config_rejects_invalid_key() {
        let result = VapidConfig::new("not-a-valid-key", "mailto:test@example.com");
        assert!(result.is_err());
    }

    #[test]
    fn build_push_request_produces_valid_http() {
        let b64 = generate_vapid_key_base64();
        let config = VapidConfig::new(&b64, "mailto:test@example.com").unwrap();
        let sub = fake_subscription();

        let request = build_push_request(&config, &sub, b"test payload").unwrap();
        assert_eq!(request.method(), http::Method::POST);
        assert!(request
            .uri()
            .to_string()
            .starts_with("https://fcm.googleapis.com/"));
        assert!(request.headers().contains_key("authorization"));
        assert!(!request.body().is_empty());
    }

    #[test]
    fn build_push_request_rejects_bad_subscription() {
        let b64 = generate_vapid_key_base64();
        let config = VapidConfig::new(&b64, "mailto:test@example.com").unwrap();
        let sub = PushSubscription {
            endpoint: "https://push.example.com/sub".to_string(),
            p256dh: "invalid!!!".to_string(),
            auth: "also-invalid".to_string(),
        };

        let result = build_push_request(&config, &sub, b"test");
        assert!(result.is_err());
    }
}
