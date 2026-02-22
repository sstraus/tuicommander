//! Plugin registry: fetch the remote plugin index and compare installed versions.

use serde::{Deserialize, Serialize};

/// Default registry URL — points to the raw `registry.json` in the public GitHub repo.
const DEFAULT_REGISTRY_URL: &str =
    "https://raw.githubusercontent.com/sstraus/tui-commander-plugins/main/registry.json";

/// A single entry in the remote plugin registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub author: String,
    /// GitHub `owner/repo` slug (optional, for linking)
    #[serde(default)]
    pub repo: String,
    pub latest_version: String,
    /// Minimum TUICommander version required to run this plugin.
    #[serde(default)]
    pub min_app_version: String,
    /// Capabilities the plugin requires.
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Direct HTTPS download URL for the `.zip` archive.
    pub download_url: String,
}

/// Fetch the remote plugin registry.
///
/// Returns the parsed list of registry entries or an error string.
/// The caller (TypeScript store) is responsible for caching / TTL.
#[tauri::command]
pub async fn fetch_plugin_registry() -> Result<Vec<RegistryEntry>, String> {
    fetch_registry_from(DEFAULT_REGISTRY_URL).await
}

/// Inner fetch logic (testable, accepts arbitrary URL).
pub(crate) async fn fetch_registry_from(url: &str) -> Result<Vec<RegistryEntry>, String> {
    let response = reqwest::get(url)
        .await
        .map_err(|e| format!("Failed to fetch plugin registry: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "Plugin registry returned HTTP {}",
            response.status()
        ));
    }

    let entries: Vec<RegistryEntry> = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse plugin registry JSON: {e}"))?;

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialise_registry_entry() {
        let json = r#"[{
            "id": "hello-world",
            "name": "Hello World",
            "description": "Example plugin",
            "author": "TUICommander",
            "repo": "sstraus/hello-world-tuic-plugin",
            "latestVersion": "1.0.0",
            "minAppVersion": "0.4.0",
            "capabilities": [],
            "downloadUrl": "https://github.com/sstraus/hello-world-tuic-plugin/releases/download/v1.0.0/hello-world.zip"
        }]"#;

        let entries: Vec<RegistryEntry> = serde_json::from_str(json).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "hello-world");
        assert_eq!(entries[0].latest_version, "1.0.0");
        assert_eq!(entries[0].min_app_version, "0.4.0");
        assert!(entries[0].download_url.starts_with("https://"));
    }

    #[test]
    fn deserialise_minimal_entry() {
        let json = r#"[{
            "id": "minimal",
            "name": "Minimal",
            "description": "No optional fields",
            "author": "Test",
            "latestVersion": "0.1.0",
            "downloadUrl": "https://example.com/minimal.zip"
        }]"#;

        let entries: Vec<RegistryEntry> = serde_json::from_str(json).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].repo, "");
        assert_eq!(entries[0].min_app_version, "");
        assert!(entries[0].capabilities.is_empty());
    }
}
