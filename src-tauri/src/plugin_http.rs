//! HTTP fetch API for plugins.
//!
//! Plugins declaring the `net:http` capability can make outbound HTTP requests
//! to URLs matching their declared `allowedUrls` patterns. Provides SSRF
//! protection by blocking unsafe schemes and validating URLs against patterns.

use std::collections::HashMap;

/// Maximum response body size (10 MB).
const MAX_RESPONSE_BYTES: usize = 10 * 1024 * 1024;

/// Default request timeout in seconds.
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Response returned to the plugin.
#[derive(Debug, Clone, serde::Serialize)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: String,
}

// ---------------------------------------------------------------------------
// URL validation
// ---------------------------------------------------------------------------

/// Validate that a URL is safe to fetch.
/// - Must be http:// or https://
/// - Must match at least one allowed URL pattern (if any are specified)
/// - If `allowed_urls` is empty, allow any http/https URL (built-in plugins)
fn validate_url(url: &str, allowed_urls: &[String]) -> Result<(), String> {
    let parsed = url::Url::parse(url).map_err(|e| format!("Invalid URL: {e}"))?;

    // Block unsafe schemes
    match parsed.scheme() {
        "http" | "https" => {}
        scheme => return Err(format!("Scheme \"{scheme}\" is not allowed; use http or https")),
    }

    // Block localhost unless explicitly allowed
    if let Some(host) = parsed.host_str() {
        let is_localhost = host == "localhost"
            || host == "127.0.0.1"
            || host == "::1"
            || host == "[::1]"
            || host == "0.0.0.0";
        if is_localhost && !allowed_urls.is_empty() {
            let localhost_allowed = allowed_urls.iter().any(|pattern| {
                pattern.contains("localhost")
                    || pattern.contains("127.0.0.1")
                    || pattern.contains("::1")
                    || pattern.contains("0.0.0.0")
            });
            if !localhost_allowed {
                return Err("Localhost URLs require explicit allowedUrls declaration".into());
            }
        }
    }

    // If no allowed URLs specified (built-in plugin), allow anything http/https
    if allowed_urls.is_empty() {
        return Ok(());
    }

    // Match against allowed URL patterns
    // Patterns use simple prefix matching with optional trailing `*`
    for pattern in allowed_urls {
        if url_matches_pattern(url, pattern) {
            return Ok(());
        }
    }

    Err(format!(
        "URL \"{url}\" does not match any allowed URL pattern"
    ))
}

/// Check if a URL matches a pattern.
/// Pattern format: a URL prefix, optionally ending with `*` for wildcard suffix.
/// Examples:
///   "https://api.anthropic.com/*" matches "https://api.anthropic.com/api/oauth/usage"
///   "https://example.com/api/v1" matches exactly "https://example.com/api/v1"
fn url_matches_pattern(url: &str, pattern: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix('*') {
        url.starts_with(prefix)
    } else {
        url == pattern
    }
}

// ---------------------------------------------------------------------------
// Tauri command
// ---------------------------------------------------------------------------

/// Make an HTTP request on behalf of a plugin.
///
/// Parameters:
/// - `url` — The URL to fetch
/// - `method` — HTTP method (GET, POST, PUT, DELETE, etc.)
/// - `headers` — Request headers
/// - `body` — Optional request body
/// - `allowed_urls` — URL patterns from the plugin's manifest (empty = unrestricted)
/// - `plugin_id` — The requesting plugin's ID (for logging)
#[tauri::command]
pub async fn plugin_http_fetch(
    url: String,
    method: Option<String>,
    headers: Option<HashMap<String, String>>,
    body: Option<String>,
    allowed_urls: Vec<String>,
    _plugin_id: String,
) -> Result<HttpResponse, String> {
    validate_url(&url, &allowed_urls)?;

    let method_str = method.as_deref().unwrap_or("GET");
    let http_method: reqwest::Method = method_str
        .parse()
        .map_err(|_| format!("Invalid HTTP method: {method_str}"))?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(DEFAULT_TIMEOUT_SECS))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

    let mut request = client.request(http_method, &url);

    if let Some(ref hdrs) = headers {
        for (key, value) in hdrs {
            request = request.header(key.as_str(), value.as_str());
        }
    }

    if let Some(ref b) = body {
        request = request.body(b.clone());
    }

    let response = request
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    let status = response.status().as_u16();

    let resp_headers: HashMap<String, String> = response
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();

    // Read body with size cap
    let body_bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read response body: {e}"))?;

    if body_bytes.len() > MAX_RESPONSE_BYTES {
        return Err(format!(
            "Response body exceeds maximum size ({} bytes > {} bytes)",
            body_bytes.len(),
            MAX_RESPONSE_BYTES
        ));
    }

    let body_str = String::from_utf8_lossy(&body_bytes).to_string();

    Ok(HttpResponse {
        status,
        headers: resp_headers,
        body: body_str,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- URL validation --

    #[test]
    fn validate_allows_https() {
        let result = validate_url("https://api.example.com/data", &[]);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_allows_http() {
        let result = validate_url("http://api.example.com/data", &[]);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_blocks_file_scheme() {
        let result = validate_url("file:///etc/passwd", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not allowed"));
    }

    #[test]
    fn validate_blocks_data_scheme() {
        let result = validate_url("data:text/plain,hello", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not allowed"));
    }

    #[test]
    fn validate_blocks_ftp_scheme() {
        let result = validate_url("ftp://example.com/file", &[]);
        assert!(result.is_err());
    }

    #[test]
    fn validate_rejects_invalid_url() {
        let result = validate_url("not a url", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid URL"));
    }

    // -- URL pattern matching --

    #[test]
    fn pattern_wildcard_suffix() {
        assert!(url_matches_pattern(
            "https://api.anthropic.com/api/oauth/usage",
            "https://api.anthropic.com/*"
        ));
    }

    #[test]
    fn pattern_wildcard_no_match() {
        assert!(!url_matches_pattern(
            "https://evil.com/api",
            "https://api.anthropic.com/*"
        ));
    }

    #[test]
    fn pattern_exact_match() {
        assert!(url_matches_pattern(
            "https://example.com/api/v1",
            "https://example.com/api/v1"
        ));
    }

    #[test]
    fn pattern_exact_no_match() {
        assert!(!url_matches_pattern(
            "https://example.com/api/v2",
            "https://example.com/api/v1"
        ));
    }

    // -- Allowed URLs enforcement --

    #[test]
    fn validate_allows_matching_pattern() {
        let allowed = vec!["https://api.anthropic.com/*".to_string()];
        let result = validate_url("https://api.anthropic.com/api/oauth/usage", &allowed);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_rejects_non_matching_pattern() {
        let allowed = vec!["https://api.anthropic.com/*".to_string()];
        let result = validate_url("https://evil.com/steal-tokens", &allowed);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not match"));
    }

    #[test]
    fn validate_allows_any_of_multiple_patterns() {
        let allowed = vec![
            "https://api.anthropic.com/*".to_string(),
            "https://api.github.com/*".to_string(),
        ];
        assert!(validate_url("https://api.github.com/repos", &allowed).is_ok());
        assert!(validate_url("https://api.anthropic.com/usage", &allowed).is_ok());
        assert!(validate_url("https://evil.com/x", &allowed).is_err());
    }

    // -- Localhost blocking --

    #[test]
    fn validate_blocks_localhost_without_declaration() {
        let allowed = vec!["https://api.example.com/*".to_string()];
        let result = validate_url("http://localhost:8080/api", &allowed);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Localhost"));
    }

    #[test]
    fn validate_blocks_127_without_declaration() {
        let allowed = vec!["https://api.example.com/*".to_string()];
        let result = validate_url("http://127.0.0.1:8080/api", &allowed);
        assert!(result.is_err());
    }

    #[test]
    fn validate_allows_localhost_with_declaration() {
        let allowed = vec!["http://localhost:8080/*".to_string()];
        let result = validate_url("http://localhost:8080/api", &allowed);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_allows_localhost_for_builtin() {
        // Empty allowed_urls = built-in plugin, no restrictions
        let result = validate_url("http://localhost:8080/api", &[]);
        assert!(result.is_ok());
    }
}
