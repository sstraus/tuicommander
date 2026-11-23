//! The credentialless destination rules of plugin egress: scheme, private-host
//! declaration and `allowed_urls` / owner patterns. Admission and every redirect
//! hop apply them.

/// Validate that a URL is safe to fetch.
/// - Must be http:// or https://
/// - Must match at least one declared allowed URL pattern; an empty list allows nothing.
pub(super) fn validate_url(url: &str, allowed_urls: &[String]) -> Result<(), String> {
    let parsed = url::Url::parse(url).map_err(|e| format!("Invalid URL: {e}"))?;

    // Block unsafe schemes
    match parsed.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(format!(
                "Scheme \"{scheme}\" is not allowed; use http or https"
            ));
        }
    }

    // Block localhost and private/RFC1918 IPs unless explicitly allowed.
    // This prevents plugins from reaching LAN hosts via SSRF.
    if let Some(host) = parsed.host_str() {
        let is_localhost = host == "localhost"
            || host == "127.0.0.1"
            || host == "::1"
            || host == "[::1]"
            || host == "0.0.0.0";

        // Check if host is a private IP (RFC1918, CGNAT/Tailscale, IPv6 ULA/link-local).
        // DEFERRED (2026-07-09) — only catches a literal IP host; a hostname
        // that *resolves* to a private IP (DNS rebinding) is not caught here.
        // See the DEFERRED note on `plugin_egress::fetch`.
        let is_private = host
            .parse::<std::net::IpAddr>()
            .map(|ip| crate::mcp_http::auth::is_private_ip(&ip))
            .unwrap_or(false);

        if is_localhost || is_private {
            // Only allow if the host is explicitly declared in allowedUrls
            let host_allowed = allowed_urls.iter().any(|pattern| pattern.contains(host));
            if !host_allowed {
                let kind = if is_localhost {
                    "Localhost"
                } else {
                    "Private network"
                };
                return Err(format!(
                    "{kind} URLs require explicit allowedUrls declaration"
                ));
            }
        }
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

/// The credentialless destination rule for the first URL and every redirect
/// hop: an owner-configured destination admits the URL without the manifest's
/// private-host declaration (the owner chose it, LAN included); otherwise the
/// manifest rule applies. Only http and https pass either way.
pub(crate) fn validate_destination(
    url: &str,
    allowed_urls: &[String],
    owner_urls: &[String],
) -> Result<(), String> {
    let parsed = url::Url::parse(url).map_err(|e| format!("Invalid URL: {e}"))?;
    if matches!(parsed.scheme(), "http" | "https")
        && owner_urls
            .iter()
            .any(|pattern| url_matches_pattern(parsed.as_str(), pattern))
    {
        return Ok(());
    }
    validate_url(url, allowed_urls)
}

/// Check if a URL matches a pattern.
/// Pattern format: a URL prefix, optionally ending with `*` for wildcard suffix.
/// Examples:
///   "https://api.anthropic.com/*" matches "https://api.anthropic.com/api/oauth/usage"
///   "https://example.com/api/v1" matches exactly "https://example.com/api/v1"
pub(super) fn url_matches_pattern(url: &str, pattern: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix('*') {
        url.starts_with(prefix)
    } else {
        url == pattern
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- URL validation --

    #[test]
    fn empty_allowlist_rejects_every_http_destination() {
        for url in [
            "https://api.example.com/data",
            "http://localhost:8080/api",
            "http://10.0.0.1/api",
        ] {
            assert!(
                validate_url(url, &[]).is_err(),
                "empty allowlist permitted {url}"
            );
        }
    }

    #[test]
    fn validate_allows_https() {
        let result = validate_url(
            "https://api.example.com/data",
            &["https://api.example.com/*".into()],
        );
        assert!(result.is_ok());
    }

    #[test]
    fn validate_allows_http() {
        let result = validate_url(
            "http://api.example.com/data",
            &["http://api.example.com/*".into()],
        );
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
    fn validate_rejects_localhost_with_empty_allowlist() {
        // No plugin may treat an empty list as unrestricted network access.
        let result = validate_url("http://localhost:8080/api", &[]);
        assert!(result.is_err());
    }

    // -- Private IP (RFC1918) blocking --

    #[test]
    fn validate_blocks_rfc1918_10_network() {
        let allowed = vec!["http://*".to_string()];
        assert!(validate_url("http://10.0.0.1:8080/api", &allowed).is_err());
        assert!(validate_url("http://10.255.255.255/", &allowed).is_err());
    }

    #[test]
    fn validate_blocks_rfc1918_172_network() {
        let allowed = vec!["http://*".to_string()];
        assert!(validate_url("http://172.16.0.1/api", &allowed).is_err());
        assert!(validate_url("http://172.31.255.255/api", &allowed).is_err());
    }

    #[test]
    fn validate_blocks_rfc1918_192_168() {
        let allowed = vec!["http://*".to_string()];
        assert!(validate_url("http://192.168.1.1/api", &allowed).is_err());
        assert!(validate_url("http://192.168.68.100:3000/", &allowed).is_err());
    }

    #[test]
    fn validate_blocks_cgnat_tailscale() {
        let allowed = vec!["http://*".to_string()];
        assert!(validate_url("http://100.64.0.1/api", &allowed).is_err());
    }

    #[test]
    fn validate_allows_private_ip_with_explicit_declaration() {
        let allowed = vec!["http://192.168.1.100:8080/*".to_string()];
        assert!(validate_url("http://192.168.1.100:8080/api", &allowed).is_ok());
    }

    #[test]
    fn validate_rejects_private_ip_with_empty_allowlist() {
        assert!(validate_url("http://10.0.0.1/api", &[]).is_err());
    }
}
