//! The pairing bundle: everything the desktop needs to reach one daemon
//! through the relay, produced on the daemon host and carried to the desktop by
//! a trusted channel that is NOT the relay (file over SSH, paste from the host
//! console). Whoever holds it can impersonate either endpoint of this pairing.
//!
//! Text form: `tuic-pair-v1:` + base64url(JSON). The version lives in both the
//! prefix and the JSON so an unknown version fails before any field is read.
//!
//! DEFERRED (2026-10-22) — a single-use P (a fresh long-term secret delivered
//! in the second handshake message) was proposed by the advisor and rejected
//! for 2085-721d: it needs an atomic two-sided persist and a crash between the
//! two stores locks the pair out. The version field leaves room for it.

use std::fmt;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use super::secure::PairingSecret;

pub(crate) const BUNDLE_VERSION: u8 = 1;
const PREFIX: &str = "tuic-pair-v1:";
const MAX_BUNDLE_BYTES: usize = 4096;
/// The relay's own session-id limit (`tools/relay` `ws_upgrade`).
const MAX_ID_LEN: usize = 64;
const MAX_TOKEN_LEN: usize = 512;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum BundleError {
    Malformed,
    UnsupportedVersion,
    InvalidField(&'static str),
}

impl fmt::Display for BundleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed => write!(f, "not a TUIC relay pairing bundle"),
            Self::UnsupportedVersion => write!(f, "unsupported pairing bundle version"),
            Self::InvalidField(name) => write!(f, "invalid pairing bundle field: {name}"),
        }
    }
}

impl std::error::Error for BundleError {}

/// The relay bearer R. Zeroed on drop, never printed.
#[derive(Clone)]
pub(crate) struct RelayBearer(Zeroizing<String>);

impl RelayBearer {
    pub(crate) fn new(token: String) -> Result<Self, BundleError> {
        if token.is_empty() || token.len() > MAX_TOKEN_LEN || token.chars().any(char::is_whitespace)
        {
            return Err(BundleError::InvalidField("relay_token"));
        }
        Ok(Self(Zeroizing::new(token)))
    }

    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for RelayBearer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RelayBearer(<redacted>)")
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PairingBundle {
    relay_url: url::Url,
    pub room: String,
    pub pairing_id: String,
    pub relay_token: RelayBearer,
    pub secret: PairingSecret,
}

#[derive(Serialize, Deserialize)]
struct Wire {
    v: u8,
    relay_url: String,
    room: String,
    pairing_id: String,
    relay_token: String,
    secret: String,
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_LEN
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Plain `ws://` is accepted only to a loopback host, for local tests.
fn valid_relay_url(url: &url::Url) -> bool {
    match url.scheme() {
        "wss" => url.host_str().is_some(),
        "ws" => {
            matches!(
                url.host(),
                Some(url::Host::Ipv4(a)) if a.is_loopback()
            ) || matches!(url.host(), Some(url::Host::Domain("localhost")))
        }
        _ => false,
    }
}

fn random_id() -> Result<String, BundleError> {
    let mut bytes = [0u8; 16];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| BundleError::InvalidField("random"))?;
    Ok(hex::encode(bytes))
}

impl PairingBundle {
    /// A fresh pairing: new room, pairing id and secret, independent of R.
    pub(crate) fn generate(relay_url: &str, relay_token: RelayBearer) -> Result<Self, BundleError> {
        let relay_url =
            url::Url::parse(relay_url).map_err(|_| BundleError::InvalidField("relay_url"))?;
        if !valid_relay_url(&relay_url) {
            return Err(BundleError::InvalidField("relay_url"));
        }
        Ok(Self {
            relay_url,
            room: random_id()?,
            pairing_id: random_id()?,
            relay_token,
            secret: PairingSecret::generate().map_err(|_| BundleError::InvalidField("secret"))?,
        })
    }

    pub(crate) fn relay_url(&self) -> &url::Url {
        &self.relay_url
    }

    pub(crate) fn to_text(&self) -> Zeroizing<String> {
        let wire = Zeroizing::new(
            serde_json::to_string(&Wire {
                v: BUNDLE_VERSION,
                relay_url: self.relay_url.to_string(),
                room: self.room.clone(),
                pairing_id: self.pairing_id.clone(),
                relay_token: self.relay_token.expose().to_string(),
                secret: URL_SAFE_NO_PAD.encode(self.secret.as_bytes()),
            })
            .expect("bundle serialization is infallible"),
        );
        Zeroizing::new(format!(
            "{PREFIX}{}",
            URL_SAFE_NO_PAD.encode(wire.as_bytes())
        ))
    }

    pub(crate) fn parse(text: &str) -> Result<Self, BundleError> {
        let text = text.trim();
        if text.len() > MAX_BUNDLE_BYTES {
            return Err(BundleError::Malformed);
        }
        let encoded = text.strip_prefix(PREFIX).ok_or_else(|| {
            if text.starts_with("tuic-pair-v") {
                BundleError::UnsupportedVersion
            } else {
                BundleError::Malformed
            }
        })?;
        let json = Zeroizing::new(
            URL_SAFE_NO_PAD
                .decode(encoded)
                .map_err(|_| BundleError::Malformed)?,
        );
        let wire: Wire = serde_json::from_slice(&json).map_err(|_| BundleError::Malformed)?;
        if wire.v != BUNDLE_VERSION {
            return Err(BundleError::UnsupportedVersion);
        }
        let relay_url =
            url::Url::parse(&wire.relay_url).map_err(|_| BundleError::InvalidField("relay_url"))?;
        if !valid_relay_url(&relay_url) {
            return Err(BundleError::InvalidField("relay_url"));
        }
        if !valid_id(&wire.room) {
            return Err(BundleError::InvalidField("room"));
        }
        if !valid_id(&wire.pairing_id) {
            return Err(BundleError::InvalidField("pairing_id"));
        }
        let secret_bytes = Zeroizing::new(
            URL_SAFE_NO_PAD
                .decode(&wire.secret)
                .map_err(|_| BundleError::InvalidField("secret"))?,
        );
        let secret: [u8; 32] = secret_bytes
            .as_slice()
            .try_into()
            .map_err(|_| BundleError::InvalidField("secret"))?;
        Ok(Self {
            relay_url,
            room: wire.room,
            pairing_id: wire.pairing_id,
            relay_token: RelayBearer::new(wire.relay_token)?,
            secret: PairingSecret::from_bytes(secret),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundle() -> PairingBundle {
        PairingBundle::generate(
            "wss://relay.example",
            RelayBearer::new("relay-bearer-token".into()).unwrap(),
        )
        .unwrap()
    }

    fn encode(wire: serde_json::Value) -> String {
        format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(wire.to_string()))
    }

    fn wire_with(field: &str, value: serde_json::Value) -> String {
        let mut wire = serde_json::json!({
            "v": 1, "relay_url": "wss://relay.example", "room": "room",
            "pairing_id": "pid", "relay_token": "tok",
            "secret": URL_SAFE_NO_PAD.encode([7u8; 32]),
        });
        wire[field] = value;
        encode(wire)
    }

    #[test]
    fn a_bundle_round_trips_through_its_text_form() {
        let original = bundle();
        let parsed = PairingBundle::parse(&original.to_text()).unwrap();
        assert_eq!(parsed.room, original.room);
        assert_eq!(parsed.pairing_id, original.pairing_id);
        assert_eq!(parsed.relay_token.expose(), "relay-bearer-token");
        assert_eq!(parsed.secret.as_bytes(), original.secret.as_bytes());
    }

    // Catches pasted text with a trailing newline being refused.
    #[test]
    fn surrounding_whitespace_from_a_paste_is_ignored() {
        let text = format!("  {}\n", *bundle().to_text());
        assert!(PairingBundle::parse(&text).is_ok());
    }

    // Catches P derived from, or equal to, the other identifiers or R.
    #[test]
    fn the_secret_is_independent_of_the_relay_credentials_and_ids() {
        let a = bundle();
        let b = bundle();
        assert_ne!(a.secret.as_bytes(), b.secret.as_bytes());
        assert_ne!(a.room, b.room);
        assert_ne!(a.pairing_id, b.pairing_id);
        let secret_hex = hex::encode(a.secret.as_bytes());
        assert!(!a.room.contains(&secret_hex) && !a.pairing_id.contains(&secret_hex));
    }

    // Catches a Debug dump (log line, panic message) leaking P or R.
    #[test]
    fn debug_output_redacts_the_secret_and_the_bearer() {
        let shown = format!("{:?}", bundle());
        assert!(!shown.contains("relay-bearer-token"));
        assert!(shown.matches("<redacted>").count() >= 2);
    }

    #[test]
    fn an_unknown_version_is_refused_not_guessed() {
        let future = wire_with("v", serde_json::json!(2));
        assert_eq!(
            PairingBundle::parse(&future).unwrap_err(),
            BundleError::UnsupportedVersion
        );
        assert_eq!(
            PairingBundle::parse("tuic-pair-v2:AAAA").unwrap_err(),
            BundleError::UnsupportedVersion
        );
    }

    // Catches a plaintext relay to a remote host, which would expose R.
    #[test]
    fn only_wss_or_loopback_ws_relays_are_accepted() {
        for bad in [
            "ws://relay.example",
            "http://relay.example",
            "wss://",
            "ftp://a",
        ] {
            let text = wire_with("relay_url", serde_json::json!(bad));
            assert_eq!(
                PairingBundle::parse(&text).unwrap_err(),
                BundleError::InvalidField("relay_url"),
                "{bad}"
            );
        }
        for ok in [
            "ws://127.0.0.1:9000",
            "ws://localhost:9000",
            "wss://relay.example:8443",
        ] {
            assert!(
                PairingBundle::parse(&wire_with("relay_url", serde_json::json!(ok))).is_ok(),
                "{ok}"
            );
        }
    }

    // Catches a room id the relay would reject, or one that could inject path.
    #[test]
    fn ids_must_be_short_and_plain() {
        for bad in ["", "a/b", "a b", &"x".repeat(65)] {
            assert_eq!(
                PairingBundle::parse(&wire_with("room", serde_json::json!(bad))).unwrap_err(),
                BundleError::InvalidField("room")
            );
        }
    }

    // Catches a short or malformed secret being padded or accepted.
    #[test]
    fn the_secret_must_be_exactly_32_bytes() {
        for bad in [
            URL_SAFE_NO_PAD.encode([1u8; 31]),
            URL_SAFE_NO_PAD.encode([1u8; 33]),
            "!!".into(),
        ] {
            assert_eq!(
                PairingBundle::parse(&wire_with("secret", serde_json::json!(bad))).unwrap_err(),
                BundleError::InvalidField("secret")
            );
        }
    }

    #[test]
    fn garbage_and_oversized_input_are_malformed() {
        assert_eq!(
            PairingBundle::parse("hello").unwrap_err(),
            BundleError::Malformed
        );
        assert_eq!(
            PairingBundle::parse(&format!("{PREFIX}{}", "A".repeat(5000))).unwrap_err(),
            BundleError::Malformed
        );
        assert_eq!(
            PairingBundle::parse(&format!("{PREFIX}!!!")).unwrap_err(),
            BundleError::Malformed
        );
    }

    #[test]
    fn the_bearer_rejects_empty_or_spaced_tokens() {
        assert!(RelayBearer::new(String::new()).is_err());
        assert!(RelayBearer::new("a b".into()).is_err());
    }
}
