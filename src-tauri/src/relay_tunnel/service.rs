//! Daemon-side setup of the relay tunnel: pairing (`tuic-remote --relay-pair`)
//! and startup of the agent from the stored pairing.
//!
//! The pairing bundle lives in the credential vault, never in the config file.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use zeroize::Zeroizing;

use super::bundle::{PairingBundle, RelayBearer};
use super::daemon::DaemonAgent;
use super::mux::MuxConfig;
use crate::credentials::{self, Credential};

/// One pairing per daemon (design choice 4A: one desktop per daemon).
const DAEMON_PAIRING_ID: &str = "daemon";
const REGISTER_TIMEOUT: Duration = Duration::from_secs(15);

/// The vault holds a bundle only after `pair`.
pub(crate) fn load_agent(
    upstream: SocketAddr,
    daemon_token: &str,
) -> Result<Option<Arc<DaemonAgent>>, String> {
    let Some(text) = credentials::get(Credential::RelayPairing(DAEMON_PAIRING_ID))? else {
        return Ok(None);
    };
    let text = Zeroizing::new(text);
    let bundle = PairingBundle::parse(&text).map_err(|e| format!("stored relay pairing: {e}"))?;
    Ok(Some(Arc::new(DaemonAgent {
        bundle,
        upstream,
        daemon_token: Zeroizing::new(daemon_token.to_owned()),
        mux: MuxConfig::default(),
    })))
}

/// Create a fresh pairing (new room, pairing id and secret), replace any stored
/// one and return the bundle text to carry to the desktop. The old desktop's
/// pairing stops working because the secret changes.
///
/// DEFERRED (2026-10-22) — rotating the daemon API token D together with P.
/// Every phone paired through the existing token would be locked out; revisit
/// with a per-client token before the relay tunnel leaves beta.
pub(crate) fn pair(relay_url: &str, relay_token: RelayBearer) -> Result<Zeroizing<String>, String> {
    let bundle =
        PairingBundle::generate(relay_url, relay_token).map_err(|e| format!("pairing: {e}"))?;
    let text = bundle.to_text();
    credentials::set(Credential::RelayPairing(DAEMON_PAIRING_ID), &text)?;
    Ok(text)
}

/// Ask the relay for a new bearer (`POST /register`).
pub(crate) async fn register_bearer(relay_url: &str) -> Result<RelayBearer, String> {
    #[derive(serde::Deserialize)]
    struct Registered {
        token: String,
    }
    let mut url = url::Url::parse(relay_url).map_err(|e| format!("relay url: {e}"))?;
    let scheme = match url.scheme() {
        "wss" => "https",
        "ws" => "http",
        other => return Err(format!("relay url scheme must be ws or wss, not {other}")),
    };
    url.set_scheme(scheme)
        .map_err(|()| "relay url scheme".to_owned())?;
    let endpoint = format!("{}/register", url.as_str().trim_end_matches('/'));
    let response = reqwest::Client::builder()
        .timeout(REGISTER_TIMEOUT)
        .build()
        .map_err(|e| e.to_string())?
        .post(endpoint)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| format!("relay registration failed: {}", e.without_url()))?;
    let registered: Registered = response
        .json()
        .await
        .map_err(|e| format!("relay registration reply: {}", e.without_url()))?;
    RelayBearer::new(registered.token).map_err(|e| format!("relay registration reply: {e}"))
}

/// `tuic-remote --relay-pair <url>`.
#[derive(Debug, Clone, Default)]
pub struct RelayPairOptions {
    pub relay_url: String,
    /// Ask the relay for a bearer instead of reading `TUIC_RELAY_TOKEN`.
    pub register: bool,
    /// Write the bundle here (mode 0600, must not exist) instead of stdout.
    pub out: Option<std::path::PathBuf>,
}

/// Create the pairing, store it, and hand the bundle to the operator. R is never
/// an argument: it comes from the relay's `/register` or from `TUIC_RELAY_TOKEN`.
pub async fn relay_pair(options: RelayPairOptions) -> anyhow::Result<()> {
    let bearer = if options.register {
        register_bearer(&options.relay_url)
            .await
            .map_err(anyhow::Error::msg)?
    } else {
        let token = std::env::var("TUIC_RELAY_TOKEN")
            .map_err(|_| anyhow::anyhow!("set TUIC_RELAY_TOKEN, or pass --relay-register"))?;
        // SAFETY: called before the runtime has other threads reading the env.
        unsafe { std::env::remove_var("TUIC_RELAY_TOKEN") };
        RelayBearer::new(token).map_err(|e| anyhow::anyhow!("TUIC_RELAY_TOKEN: {e}"))?
    };
    let text = pair(&options.relay_url, bearer).map_err(anyhow::Error::msg)?;
    match &options.out {
        Some(path) => {
            write_private_new(path, text.as_bytes())?;
            eprintln!(
                "Pairing bundle written to {}. It holds the end-to-end secret: copy it to the \
                 desktop over a trusted channel, import it there, then delete the file.",
                path.display()
            );
        }
        None => {
            println!("{}", *text);
            eprintln!(
                "Pairing bundle above holds the end-to-end secret: import it on the desktop \
                 over a trusted channel and clear your terminal history."
            );
        }
    }
    eprintln!("Restart tuic-remote so it registers with the relay.");
    Ok(())
}

fn write_private_new(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)?.write_all(bytes)
}

#[cfg(test)]
mod tests;
