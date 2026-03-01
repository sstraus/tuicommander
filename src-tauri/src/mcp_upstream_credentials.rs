//! Credential management for upstream MCP servers via OS keyring.
//!
//! Credentials are stored in the platform-specific secure store:
//! - macOS: Keychain
//! - Windows: Credential Manager
//! - Linux: keyutils / Secret Service
//!
//! The config file (`mcp-upstreams.json`) never contains secrets — only the
//! upstream name is needed to look up the credential at runtime.

const SERVICE_NAME: &str = "tuicommander-mcp";

/// Validate that an upstream name is safe for use as a keyring key.
/// Must match `[a-z0-9_-]+` (same rule as upstream config validation).
fn validate_keyring_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Upstream name must not be empty".to_string());
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        return Err(format!(
            "Upstream name '{name}' is invalid: must contain only lowercase letters, digits, hyphens, and underscores"
        ));
    }
    Ok(())
}

/// Read a credential for an upstream MCP server.
/// Returns `None` if no credential is stored (not an error).
pub(crate) fn read_upstream_credential(upstream_name: &str) -> Result<Option<String>, String> {
    let entry = keyring::Entry::new(SERVICE_NAME, upstream_name)
        .map_err(|e| format!("Failed to create keyring entry: {e}"))?;

    match entry.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!(
            "Failed to read credential for upstream '{upstream_name}': {e}"
        )),
    }
}

/// Store a credential for an upstream MCP server.
pub(crate) fn save_upstream_credential(
    upstream_name: &str,
    token: &str,
) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE_NAME, upstream_name)
        .map_err(|e| format!("Failed to create keyring entry: {e}"))?;

    entry
        .set_password(token)
        .map_err(|e| format!("Failed to save credential for upstream '{upstream_name}': {e}"))
}

/// Delete a credential for an upstream MCP server.
/// Returns Ok(()) even if no credential existed (idempotent).
pub(crate) fn delete_upstream_credential(upstream_name: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE_NAME, upstream_name)
        .map_err(|e| format!("Failed to create keyring entry: {e}"))?;

    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()), // Already gone — idempotent
        Err(e) => Err(format!(
            "Failed to delete credential for upstream '{upstream_name}': {e}"
        )),
    }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub(crate) fn save_mcp_upstream_credential(
    name: String,
    token: String,
) -> Result<(), String> {
    validate_keyring_name(&name)?;
    save_upstream_credential(&name, &token)
}

#[tauri::command]
pub(crate) fn delete_mcp_upstream_credential(name: String) -> Result<(), String> {
    validate_keyring_name(&name)?;
    delete_upstream_credential(&name)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // These tests interact with the real OS keyring. Write tests are #[ignore]
    // because macOS prompts for Keychain access interactively, blocking CI/automated runs.
    // Run manually with: cargo test mcp_upstream_credentials -- --ignored

    const TEST_UPSTREAM: &str = "tuicommander-test-upstream-credential";

    #[test]
    fn read_nonexistent_returns_none() {
        let result = read_upstream_credential("nonexistent-upstream-that-does-not-exist-xyz");
        match result {
            Ok(None) => {} // Expected: no credential stored
            Ok(Some(_)) => panic!("Expected None for nonexistent credential"),
            Err(e) => {
                // On systems without a keyring backend, this may error — skip gracefully
                eprintln!("Skipping test (no keyring backend): {e}");
            }
        }
    }

    #[test]
    #[ignore] // Requires interactive Keychain access on macOS
    fn save_read_delete_round_trip() {
        let token = "test-token-abc123";

        // Save
        match save_upstream_credential(TEST_UPSTREAM, token) {
            Ok(()) => {}
            Err(e) => {
                eprintln!("Skipping test (no keyring backend): {e}");
                return;
            }
        }

        // Read back
        let read = read_upstream_credential(TEST_UPSTREAM).unwrap();
        assert_eq!(read, Some(token.to_string()));

        // Delete
        delete_upstream_credential(TEST_UPSTREAM).unwrap();

        // Verify deleted
        let after_delete = read_upstream_credential(TEST_UPSTREAM).unwrap();
        assert_eq!(after_delete, None);
    }

    // -- validate_keyring_name --

    #[test]
    fn valid_keyring_names() {
        assert!(validate_keyring_name("my-server").is_ok());
        assert!(validate_keyring_name("server_1").is_ok());
        assert!(validate_keyring_name("a").is_ok());
        assert!(validate_keyring_name("abc-def_123").is_ok());
    }

    #[test]
    fn empty_keyring_name_rejected() {
        assert!(validate_keyring_name("").is_err());
    }

    #[test]
    fn uppercase_keyring_name_rejected() {
        assert!(validate_keyring_name("MyServer").is_err());
    }

    #[test]
    fn keyring_name_with_spaces_rejected() {
        assert!(validate_keyring_name("my server").is_err());
    }

    #[test]
    fn keyring_name_with_path_separators_rejected() {
        assert!(validate_keyring_name("../etc/passwd").is_err());
    }

    #[test]
    fn keyring_name_with_special_chars_rejected() {
        assert!(validate_keyring_name("server;drop").is_err());
    }

    #[test]
    fn delete_nonexistent_is_idempotent() {
        let result = delete_upstream_credential("nonexistent-upstream-that-does-not-exist-xyz");
        match result {
            Ok(()) => {} // Expected: idempotent
            Err(e) => {
                eprintln!("Skipping test (no keyring backend): {e}");
            }
        }
    }

    #[test]
    #[ignore] // Requires interactive Keychain access on macOS
    fn overwrite_credential() {
        let token_v1 = "token-v1";
        let token_v2 = "token-v2";

        match save_upstream_credential(TEST_UPSTREAM, token_v1) {
            Ok(()) => {}
            Err(e) => {
                eprintln!("Skipping test (no keyring backend): {e}");
                return;
            }
        }

        // Overwrite
        save_upstream_credential(TEST_UPSTREAM, token_v2).unwrap();

        // Read latest
        let read = read_upstream_credential(TEST_UPSTREAM).unwrap();
        assert_eq!(read, Some(token_v2.to_string()));

        // Cleanup
        delete_upstream_credential(TEST_UPSTREAM).unwrap();
    }
}
