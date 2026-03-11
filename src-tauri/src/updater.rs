//! Update channel checker for beta/nightly releases.
//!
//! Owns URL mapping, HTTP fetch (with timeout + size cap), manifest parsing,
//! and error classification. The TypeScript frontend is a pure state consumer.

use serde::Serialize;

/// Result of checking a specific update channel.
#[derive(Debug, Clone, Serialize)]
pub struct UpdateCheckResult {
    pub available: bool,
    pub version: Option<String>,
    pub notes: Option<String>,
    pub release_page: Option<String>,
    /// True when the channel has no published release (404). Not an error.
    pub not_found: bool,
}

#[derive(Debug)]
struct ChannelConfig {
    manifest_url: &'static str,
    release_page: &'static str,
}

const CHANNELS: &[(&str, ChannelConfig)] = &[
    (
        "beta",
        ChannelConfig {
            manifest_url: "https://github.com/sstraus/tuicommander/releases/download/beta/latest.json",
            release_page: "https://github.com/sstraus/tuicommander/releases/tag/beta",
        },
    ),
    (
        "nightly",
        ChannelConfig {
            manifest_url: "https://github.com/sstraus/tuicommander/releases/download/nightly/latest.json",
            release_page: "https://github.com/sstraus/tuicommander/releases/tag/nightly",
        },
    ),
];

/// Maximum manifest size (64 KB). Anything larger is rejected.
const MAX_MANIFEST_BYTES: usize = 64 * 1024;

/// HTTP request timeout.
const TIMEOUT_SECS: u64 = 15;

/// Look up channel configuration by name.
fn get_channel_config(channel: &str) -> Result<&'static ChannelConfig, String> {
    CHANNELS
        .iter()
        .find(|(name, _)| *name == channel)
        .map(|(_, cfg)| cfg)
        .ok_or_else(|| format!("Unknown update channel: \"{channel}\". Valid channels: beta, nightly"))
}

/// Internal implementation that accepts a URL — enables testing with mock server.
async fn fetch_channel_manifest(
    manifest_url: &str,
    release_page: &str,
) -> Result<UpdateCheckResult, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

    let resp = client
        .get(manifest_url)
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    let status = resp.status();

    // 404 = no release published for this channel (informational, not an error)
    if status == reqwest::StatusCode::NOT_FOUND {
        return Ok(UpdateCheckResult {
            available: false,
            version: None,
            notes: None,
            release_page: Some(release_page.to_string()),
            not_found: true,
        });
    }

    if !status.is_success() {
        return Err(format!("HTTP {status}"));
    }

    // Read body with size cap
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("Failed to read response body: {e}"))?;

    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err(format!(
            "Response too large: {} bytes (max {})",
            bytes.len(),
            MAX_MANIFEST_BYTES
        ));
    }

    // Parse JSON
    let json: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("Invalid JSON: {e}"))?;

    // Extract version — if missing, treat as "no update available"
    let version = json
        .get("version")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    if version.is_none() {
        return Ok(UpdateCheckResult {
            available: false,
            version: None,
            notes: None,
            release_page: Some(release_page.to_string()),
            not_found: false,
        });
    }

    let notes = json
        .get("notes")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Ok(UpdateCheckResult {
        available: true,
        version,
        notes,
        release_page: Some(release_page.to_string()),
        not_found: false,
    })
}

/// Check a beta/nightly update channel for available updates.
///
/// URLs are hardcoded — no user-supplied URLs accepted (SSRF prevention).
#[tauri::command]
pub async fn check_update_channel(channel: String) -> Result<UpdateCheckResult, String> {
    let config = get_channel_config(&channel)?;
    fetch_channel_manifest(config.manifest_url, config.release_page).await
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_urls_are_https_github() {
        for (name, cfg) in CHANNELS {
            assert!(
                cfg.manifest_url.starts_with("https://github.com/"),
                "Channel {name}: manifest URL must be HTTPS on github.com, got: {}",
                cfg.manifest_url
            );
            assert!(
                cfg.release_page.starts_with("https://github.com/"),
                "Channel {name}: release page must be HTTPS on github.com, got: {}",
                cfg.release_page
            );
            assert!(
                cfg.manifest_url.ends_with("/latest.json"),
                "Channel {name}: manifest URL must end with /latest.json"
            );
        }
    }

    #[test]
    fn test_get_channel_config_valid() {
        assert!(get_channel_config("beta").is_ok());
        assert!(get_channel_config("nightly").is_ok());
    }

    #[test]
    fn test_get_channel_config_invalid() {
        let err = get_channel_config("alpha").unwrap_err();
        assert!(err.contains("Unknown update channel"));
        assert!(err.contains("alpha"));
    }

    #[tokio::test]
    async fn test_check_update_channel_invalid_channel() {
        let result = check_update_channel("foobar".to_string()).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown update channel"));
    }

    #[tokio::test]
    async fn test_check_update_channel_not_found() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/beta/latest.json")
            .with_status(404)
            .create_async()
            .await;

        let result = fetch_channel_manifest(
            &format!("{}/beta/latest.json", server.url()),
            "https://github.com/sstraus/tuicommander/releases/tag/beta",
        )
        .await
        .expect("404 should return Ok, not Err");

        assert!(!result.available);
        assert!(result.not_found);
        assert!(result.version.is_none());
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_check_update_channel_success() {
        let mut server = mockito::Server::new_async().await;
        let body = serde_json::json!({
            "version": "2.0.0-beta.1",
            "notes": "Beta release notes",
            "pub_date": "2026-03-11"
        });
        let mock = server
            .mock("GET", "/beta/latest.json")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body.to_string())
            .create_async()
            .await;

        let result = fetch_channel_manifest(
            &format!("{}/beta/latest.json", server.url()),
            "https://github.com/sstraus/tuicommander/releases/tag/beta",
        )
        .await
        .expect("200 with valid manifest should succeed");

        assert!(result.available);
        assert_eq!(result.version.as_deref(), Some("2.0.0-beta.1"));
        assert_eq!(result.notes.as_deref(), Some("Beta release notes"));
        assert_eq!(
            result.release_page.as_deref(),
            Some("https://github.com/sstraus/tuicommander/releases/tag/beta")
        );
        assert!(!result.not_found);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_check_update_channel_network_error() {
        // Use a URL that will fail to connect (port 1 is almost certainly closed)
        let result = fetch_channel_manifest(
            "http://127.0.0.1:1/nonexistent",
            "https://example.com",
        )
        .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("Network error") || err.contains("error"),
            "Expected network error, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_check_update_channel_oversized_response() {
        let mut server = mockito::Server::new_async().await;
        // Create a body larger than 64 KB
        let big_body = "x".repeat(MAX_MANIFEST_BYTES + 1);
        let mock = server
            .mock("GET", "/big")
            .with_status(200)
            .with_body(big_body)
            .create_async()
            .await;

        let result = fetch_channel_manifest(
            &format!("{}/big", server.url()),
            "https://example.com",
        )
        .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("too large") || err.contains("Invalid JSON"),
            "Expected size/parse error, got: {err}"
        );
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_check_update_channel_invalid_json() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/garbage")
            .with_status(200)
            .with_body("this is not json {{{")
            .create_async()
            .await;

        let result = fetch_channel_manifest(
            &format!("{}/garbage", server.url()),
            "https://example.com",
        )
        .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid JSON"));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_check_update_channel_missing_version() {
        let mut server = mockito::Server::new_async().await;
        let body = serde_json::json!({ "notes": "Some notes but no version" });
        let mock = server
            .mock("GET", "/noversion")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body.to_string())
            .create_async()
            .await;

        let result = fetch_channel_manifest(
            &format!("{}/noversion", server.url()),
            "https://example.com",
        )
        .await
        .expect("Missing version should return Ok, not Err");

        assert!(!result.available);
        assert!(result.version.is_none());
        assert!(!result.not_found);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_check_update_channel_http_500() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/error")
            .with_status(500)
            .create_async()
            .await;

        let result = fetch_channel_manifest(
            &format!("{}/error", server.url()),
            "https://example.com",
        )
        .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("HTTP 500"));
        mock.assert_async().await;
    }
}
