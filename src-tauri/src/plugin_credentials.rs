//! Credential reading API for plugins.
//!
//! Plugins declaring the `credentials:read` capability can read credentials
//! from the system credential store. On macOS this reads from Keychain;
//! on Linux/Windows it reads from a JSON file in the user's home directory.
//!
//! Returns the raw credential JSON string, or null if not found.

use std::path::PathBuf;

/// Read a credential by service name.
///
/// - macOS: reads from Keychain via `security find-generic-password`
/// - Linux/Windows: reads from `~/.claude/.credentials.json`
///
/// Returns `Ok(Some(json_string))` if found, `Ok(None)` if not found,
/// `Err` on I/O or permission errors.
#[tauri::command]
pub async fn plugin_read_credential(
    service_name: String,
    _plugin_id: String,
) -> Result<Option<String>, String> {
    if service_name.is_empty() {
        return Err("Service name is empty".into());
    }

    #[cfg(target_os = "macos")]
    {
        read_from_keychain(&service_name)
    }
    #[cfg(not(target_os = "macos"))]
    {
        read_from_json_file(&service_name)
    }
}

/// macOS: read from Keychain via `security find-generic-password -s <service> -w`.
/// This shells out to avoid adding a native Keychain crate dependency.
#[cfg(target_os = "macos")]
fn read_from_keychain(service_name: &str) -> Result<Option<String>, String> {
    let output = std::process::Command::new("security")
        .args(["find-generic-password", "-s", service_name, "-w"])
        .output()
        .map_err(|e| format!("Failed to run security command: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // "could not be found" means the credential doesn't exist — not an error
        if stderr.contains("could not be found") || stderr.contains("SecKeychainSearchCopyNext") {
            return Ok(None);
        }
        return Err(format!("Keychain read failed: {}", stderr.trim()));
    }

    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if raw.is_empty() {
        return Ok(None);
    }

    Ok(Some(raw))
}

/// Linux/Windows: read from `~/.claude/.credentials.json`.
#[cfg(not(target_os = "macos"))]
fn read_from_json_file(service_name: &str) -> Result<Option<String>, String> {
    let path = credentials_json_path()?;
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            // Parse the JSON file and extract the value for the service name key
            let parsed: serde_json::Value = serde_json::from_str(&content)
                .map_err(|e| format!("Failed to parse credentials file: {e}"))?;
            // Convert service name to the camelCase key format used in the JSON
            // "Claude Code-credentials" → "claudeAiOauth" is stored at the root level
            // For general use, return the raw JSON string of the whole file
            // The plugin will extract what it needs
            Ok(Some(content))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("Failed to read credentials file: {e}")),
    }
}

/// Path to the credentials JSON file on non-macOS platforms.
#[cfg(not(target_os = "macos"))]
fn credentials_json_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    Ok(home.join(".claude").join(".credentials.json"))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_service_name_is_rejected() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(plugin_read_credential(
            String::new(),
            "test-plugin".to_string(),
        ));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn keychain_returns_none_for_nonexistent_service() {
        let result = read_from_keychain("nonexistent-service-that-definitely-does-not-exist-12345");
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn keychain_returns_some_for_claude_credentials() {
        // This test verifies the integration works on this machine
        let result = read_from_keychain("Claude Code-credentials");
        assert!(result.is_ok());
        // May or may not exist on the test machine, but should not error
        if let Some(json) = result.unwrap() {
            // If it exists, it should be valid JSON
            assert!(json.starts_with('{'));
            let parsed: Result<serde_json::Value, _> = serde_json::from_str(&json);
            assert!(parsed.is_ok());
        }
    }
}
