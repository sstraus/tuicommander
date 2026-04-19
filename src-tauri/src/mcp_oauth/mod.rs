pub(crate) mod commands;
pub(crate) mod dcr;
pub(crate) mod discovery;
pub(crate) mod flow;
pub(crate) mod token;

/// Redirect URI handed to upstream authorization servers for the OAuth 2.1
/// flow. Must match the hostname matched by the frontend deep-link handler in
/// `src/deep-link-handler.ts` — any divergence causes AS-side
/// `redirect_uri_mismatch` errors AND a silent drop in the deep-link handler.
pub(crate) const OAUTH_REDIRECT_URI: &str = "tuic://oauth-callback";

#[cfg(test)]
mod redirect_uri_tests {
    use super::OAUTH_REDIRECT_URI;

    #[test]
    fn redirect_uri_matches_deep_link_hostname() {
        // Hostname (between "tuic://" and the first "/" or end) must be
        // "oauth-callback" — the exact string matched in
        // src/deep-link-handler.ts case "oauth-callback".
        assert_eq!(OAUTH_REDIRECT_URI, "tuic://oauth-callback");
        let hostname = OAUTH_REDIRECT_URI
            .strip_prefix("tuic://")
            .and_then(|rest| rest.split('/').next())
            .expect("redirect URI must be tuic://<hostname>");
        assert_eq!(hostname, "oauth-callback");
    }

    #[test]
    fn redirect_uri_const_used_in_both_call_sites() {
        // Sanity check: the constant is the single source of truth. This test
        // is a compile-time-ish guard — if someone inlines a literal again,
        // add it to the grep assertion below.
        let commands_src = include_str!("commands.rs");
        let registry_src = include_str!("../mcp_proxy/registry.rs");
        // Neither file may contain a raw "tuic://oauth" literal — they must
        // import OAUTH_REDIRECT_URI instead.
        assert!(
            !commands_src.contains("\"tuic://oauth"),
            "commands.rs must use OAUTH_REDIRECT_URI const, not a string literal"
        );
        assert!(
            !registry_src.contains("\"tuic://oauth"),
            "registry.rs must use OAUTH_REDIRECT_URI const, not a string literal"
        );
    }
}
