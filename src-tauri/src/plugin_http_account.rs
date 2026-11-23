//! Account-authenticated plugin requests: Rust attaches the credential, and no
//! reply carries it back to the plugin.

use crate::accounts::{Account, AuthKind};
use crate::plugins::PluginManifest;
use reqwest::header::HeaderValue;

/// The credential of one account, bound to the API base it may be sent to.
pub(crate) struct AccountAuth {
    account_id: String,
    provider: String,
    base: url::Url,
    header: HeaderValue,
    /// Every form of the secret that may appear on the wire, for redaction.
    secrets: Vec<String>,
}

/// Resolve `account_id` for a request from the plugin that owns `manifest`.
/// The provider must be declared in the on-disk manifest (consent); the token
/// is re-read on every request, so a re-entered credential applies at once.
pub(crate) async fn resolve(
    state: &crate::AppState,
    manifest: &PluginManifest,
    account_id: &str,
) -> Result<AccountAuth, String> {
    let (account, token) = crate::accounts::load_credential(account_id.into(), state).await?;
    if !manifest.declares_provider(&account.provider) {
        return Err(format!(
            "Plugin \"{}\" does not declare account provider \"{}\" in its manifest",
            manifest.id, account.provider
        ));
    }
    let base = api_base(&account)?;
    let (value, mut secrets) = match account.kind {
        AuthKind::Basic => {
            use base64::Engine as _;
            let user = account
                .username
                .as_deref()
                .ok_or("Basic authentication requires a username")?;
            let encoded =
                base64::engine::general_purpose::STANDARD.encode(format!("{user}:{token}"));
            (format!("Basic {encoded}"), vec![encoded])
        }
        _ => (format!("Bearer {token}"), Vec::new()),
    };
    let mut header =
        HeaderValue::from_str(&value).map_err(|_| "Account credential is not a valid header")?;
    header.set_sensitive(true);
    secrets.push(token);
    Ok(AccountAuth {
        account_id: account_id.to_owned(),
        provider: account.provider,
        base,
        header,
        secrets,
    })
}

/// The URL prefix an account's credential may reach. GitHub accounts store the
/// web host; their API lives at `api.github.com` or `https://<ghe>/api/`.
fn api_base(account: &Account) -> Result<url::Url, String> {
    let base = if account.provider == "github" {
        let host = account
            .host
            .strip_prefix("https://")
            .and_then(crate::github_account::GitHubHost::new)
            .ok_or("GitHub account requires a canonical HTTPS hostname")?;
        if host.is_cloud() {
            "https://api.github.com/".to_string()
        } else {
            format!("https://{}/api/", host.as_str())
        }
    } else {
        format!("{}/", account.host.trim_end_matches('/'))
    };
    url::Url::parse(&base).map_err(|_| "Account host is not a valid URL".into())
}

/// True when `url` is under `base`: same scheme, host and port, path prefix.
/// `Url::parse` has already normalised `..` and `%2e` segments. A redirect hop
/// under the base is followed; reqwest drops `Authorization` on any hop that
/// changes scheme, host or port (`redirect::remove_sensitive_headers`).
pub(crate) fn is_under(url: &url::Url, base: &url::Url) -> bool {
    url.scheme() == base.scheme()
        && url.host_str() == base.host_str()
        && url.port_or_known_default() == base.port_or_known_default()
        && url.username().is_empty()
        && url.password().is_none()
        && url.path().starts_with(base.path())
}

impl AccountAuth {
    /// The account host rule: the credential goes only under the account API base.
    pub(crate) fn check_url(&self, url: &str) -> Result<(), String> {
        let parsed = url::Url::parse(url).map_err(|e| format!("Invalid URL: {e}"))?;
        if !is_under(&parsed, &self.base) {
            return Err(format!(
                "URL is outside the account host \"{}\"",
                self.base.as_str()
            ));
        }
        Ok(())
    }

    pub(crate) fn account_id(&self) -> &str {
        &self.account_id
    }

    pub(crate) fn provider(&self) -> &str {
        &self.provider
    }

    pub(crate) fn base(&self) -> url::Url {
        self.base.clone()
    }

    pub(crate) fn header(&self) -> HeaderValue {
        self.header.clone()
    }

    /// Remove every form of the secret from text a provider sent back.
    pub(crate) fn redact(&self, text: String) -> String {
        self.secrets
            .iter()
            .filter(|secret| !secret.is_empty())
            .fold(text, |text, secret| {
                text.replace(secret.as_str(), "[redacted]")
            })
    }
}
