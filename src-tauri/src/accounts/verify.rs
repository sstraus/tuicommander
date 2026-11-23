//! Verification returns identity only, never a provider response body or token.
use super::model::{Account, AuthKind};
use super::providers::{ProviderDescriptor, descriptor};
use crate::github::gateway::Origin;
use crate::provider_gateway::direct::{self, Direct};
use tuic_gateway::model::Provider;

/// A verification request has one fixed deadline, no redirects, and a small body.
const VERIFY_TIMEOUT_SECS: u64 = 30;
/// Viewer identities fit comfortably below 64 KiB; reject unbounded responses.
const MAX_IDENTITY_BYTES: usize = 64 * 1024;

pub(crate) async fn verify(account: &Account, token: &str) -> Result<String, String> {
    verify_with_descriptor(account, token, &descriptor(&account.provider)?).await
}

async fn verify_with_descriptor(
    account: &Account,
    token: &str,
    provider: &ProviderDescriptor,
) -> Result<String, String> {
    // A credential must not follow a redirect: the verifier never does.
    let call = Direct::new(
        Origin::interactive("accounts.verify"),
        if account.provider == "github" {
            Provider::GitHub
        } else {
            Provider::Generic
        },
        &account.id,
    )
    .no_redirect()
    .timeout(std::time::Duration::from_secs(VERIFY_TIMEOUT_SECS));
    if account.provider == "github" {
        let host = account
            .host
            .strip_prefix("https://")
            .and_then(crate::github_account::GitHubHost::new)
            .ok_or("GitHub requires a canonical HTTPS hostname")?;
        return crate::github_account::fetch_account_login(call, &host, token)
            .await?
            .ok_or_else(|| "Account verification did not return a viewer identity".into());
    }
    let url = format!(
        "{}/{}",
        account.host,
        provider.verify.path.trim_start_matches('/')
    );
    let request = direct::builder(reqwest::Method::GET, url)
        .header("User-Agent", "TUICommander")
        .header("Accept", "application/json");
    let request = match account.kind {
        AuthKind::Basic => request.basic_auth(
            account
                .username
                .as_deref()
                .ok_or("Basic authentication requires a username")?,
            Some(token),
        ),
        _ => request.bearer_auth(token),
    };
    let response = call
        .send(request)
        .await
        .map_err(|_| "Account verification could not reach the site")?;
    read_verified_identity(response, &provider.verify.login_pointer, token).await
}

/// Shared with the existing core GitHub verifier; bounded and content-free errors.
pub(crate) async fn read_verified_identity(
    mut response: reqwest::Response,
    login_pointer: &str,
    token: &str,
) -> Result<String, String> {
    if !response.status().is_success() {
        // Status is safe; provider error bodies and reqwest URLs may contain secrets.
        return Err(format!(
            "Account verification failed (HTTP {})",
            response.status().as_u16()
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_IDENTITY_BYTES as u64)
    {
        return Err("Account verification response is too large".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Account verification response failed")?
    {
        if bytes.len().saturating_add(chunk.len()) > MAX_IDENTITY_BYTES {
            return Err("Account verification response is too large".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let body: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|_| "Account verification returned invalid identity data")?;
    let login = body
        .pointer(login_pointer)
        .and_then(|v| v.as_str())
        .filter(|login| !login.trim().is_empty() && login.len() <= 512 && !login.contains(token))
        .ok_or("Account verification did not return a safe viewer identity")?;
    Ok(login.to_string())
}
