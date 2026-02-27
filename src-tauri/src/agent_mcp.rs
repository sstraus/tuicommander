use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// MCP config lookup result
#[derive(Clone, Serialize)]
pub(crate) struct AgentMcpStatus {
    pub(crate) supported: bool,
    pub(crate) installed: bool,
    pub(crate) config_path: Option<String>,
}

/// Per-agent MCP config spec
struct McpConfigSpec {
    /// Path to the MCP configuration file
    config_path: PathBuf,
    /// JSON pointer segments to the mcpServers object (e.g. ["mcpServers"])
    key_path: Vec<&'static str>,
}

/// Our MCP server entry injected into agent configs
#[derive(Serialize, Deserialize)]
struct TuicMcpEntry {
    command: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    args: Vec<String>,
}

const TUIC_MCP_KEY: &str = "tuicommander";

/// Get the home directory, panicking on failure (should never happen in practice)
fn home() -> PathBuf {
    dirs::home_dir().expect("HOME directory not found")
}

/// Get the VS Code user directory (platform-specific)
fn vscode_user_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        home().join("Library/Application Support/Code/User")
    }
    #[cfg(target_os = "linux")]
    {
        home().join(".config/Code/User")
    }
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var("APPDATA").unwrap_or_default();
        PathBuf::from(appdata).join("Code/User")
    }
}

/// Look up the MCP config spec for a given agent type.
/// Returns None for agents that don't support MCP.
fn get_mcp_config_spec(agent_type: &str) -> Option<McpConfigSpec> {
    let h = home();
    match agent_type {
        "claude" => Some(McpConfigSpec {
            config_path: h.join(".claude.json"),
            key_path: vec!["mcpServers"],
        }),
        "cursor" => Some(McpConfigSpec {
            config_path: h.join(".cursor/mcp.json"),
            key_path: vec!["mcpServers"],
        }),
        "windsurf" => Some(McpConfigSpec {
            config_path: h.join(".codeium/windsurf/mcp_config.json"),
            key_path: vec!["mcpServers"],
        }),
        "vscode" => Some(McpConfigSpec {
            config_path: vscode_user_dir().join("mcp.json"),
            key_path: vec!["servers"],
        }),
        "zed" => Some(McpConfigSpec {
            config_path: h.join(".config/zed/settings.json"),
            key_path: vec!["context_servers"],
        }),
        "amp" => Some(McpConfigSpec {
            config_path: h.join(".config/amp/settings.json"),
            key_path: vec!["amp", "mcpServers"],
        }),
        "gemini" => Some(McpConfigSpec {
            config_path: h.join(".gemini/settings.json"),
            key_path: vec!["mcpServers"],
        }),
        // Agents that don't support MCP config files
        "aider" | "warp" | "opencode" | "codex" | "droid" => None,
        _ => None,
    }
}

/// Get the path to an agent's own settings file (for "Edit Config" button)
fn get_agent_settings_path(agent_type: &str) -> Option<PathBuf> {
    let h = home();
    match agent_type {
        "claude" => Some(h.join(".claude/settings.json")),
        "cursor" => Some(h.join(".cursor")),
        "aider" => Some(h.join(".aider.conf.yml")),
        "gemini" => Some(h.join(".gemini/settings.json")),
        "codex" => Some(h.join(".codex/config.toml")),
        "amp" => Some(h.join(".config/amp/settings.json")),
        "zed" => Some(h.join(".config/zed/settings.json")),
        "vscode" => Some(vscode_user_dir().join("settings.json")),
        "windsurf" => Some(h.join(".codeium/windsurf/settings.json")),
        _ => None,
    }
}

/// Navigate a JSON object by key path, creating intermediate objects as needed.
/// Returns a mutable reference to the target object.
fn navigate_or_create<'a>(
    root: &'a mut serde_json::Value,
    key_path: &[&str],
) -> &'a mut serde_json::Value {
    let mut current = root;
    for key in key_path {
        if !current.is_object() {
            *current = serde_json::json!({});
        }
        current = current
            .as_object_mut()
            .unwrap()
            .entry(*key)
            .or_insert_with(|| serde_json::json!({}));
    }
    current
}

/// Navigate a JSON object by key path (read-only).
fn navigate<'a>(root: &'a serde_json::Value, key_path: &[&str]) -> Option<&'a serde_json::Value> {
    let mut current = root;
    for key in key_path {
        current = current.get(*key)?;
    }
    Some(current)
}

/// Detect the tui-mcp-bridge binary path.
/// Tries resolve_cli first, then falls back to the app's resource directory.
fn detect_bridge_binary() -> String {
    let resolved = crate::cli::resolve_cli("tui-mcp-bridge");
    if std::path::Path::new(&resolved).exists() {
        return resolved;
    }
    // Fallback: same directory as the current executable
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let candidate = dir.join("tui-mcp-bridge");
        if candidate.exists() {
            return candidate.to_string_lossy().to_string();
        }
        #[cfg(windows)]
        {
            let candidate = dir.join("tui-mcp-bridge.exe");
            if candidate.exists() {
                return candidate.to_string_lossy().to_string();
            }
        }
    }
    // Last resort: bare name, hope it's on PATH
    "tui-mcp-bridge".to_string()
}

/// Read a JSON file, returning an empty object if it doesn't exist or is invalid.
fn read_json_file(path: &std::path::Path) -> serde_json::Value {
    if !path.exists() {
        return serde_json::json!({});
    }
    match std::fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({})),
        Err(_) => serde_json::json!({}),
    }
}

/// Write a JSON file atomically (temp + rename), preserving formatting.
fn write_json_file(path: &std::path::Path, value: &serde_json::Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory {}: {e}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(value)
        .map_err(|e| format!("Failed to serialize JSON: {e}"))?;
    let temp = path.with_extension("tmp");
    std::fs::write(&temp, &json)
        .map_err(|e| format!("Failed to write temp file: {e}"))?;
    std::fs::rename(&temp, path)
        .map_err(|e| {
            let _ = std::fs::remove_file(&temp);
            format!("Failed to rename temp file: {e}")
        })?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Check MCP installation status for an agent
#[tauri::command]
pub(crate) fn get_agent_mcp_status(agent_type: String) -> AgentMcpStatus {
    let Some(spec) = get_mcp_config_spec(&agent_type) else {
        return AgentMcpStatus {
            supported: false,
            installed: false,
            config_path: None,
        };
    };

    let config_path_str = spec.config_path.to_string_lossy().to_string();

    if !spec.config_path.exists() {
        return AgentMcpStatus {
            supported: true,
            installed: false,
            config_path: Some(config_path_str),
        };
    }

    let root = read_json_file(&spec.config_path);
    let installed = navigate(&root, &spec.key_path)
        .and_then(|v| v.as_object())
        .is_some_and(|obj| obj.contains_key(TUIC_MCP_KEY));

    AgentMcpStatus {
        supported: true,
        installed,
        config_path: Some(config_path_str),
    }
}

/// Install the tui-mcp-bridge MCP entry into an agent's config
#[tauri::command]
pub(crate) fn install_agent_mcp(agent_type: String) -> Result<(), String> {
    let spec = get_mcp_config_spec(&agent_type)
        .ok_or_else(|| format!("Agent '{agent_type}' does not support MCP configuration"))?;

    let bridge_path = detect_bridge_binary();
    let entry = TuicMcpEntry {
        command: bridge_path,
        args: vec![],
    };
    let entry_value = serde_json::to_value(&entry)
        .map_err(|e| format!("Failed to serialize MCP entry: {e}"))?;

    let mut root = read_json_file(&spec.config_path);
    let servers = navigate_or_create(&mut root, &spec.key_path);

    if let Some(obj) = servers.as_object_mut() {
        obj.insert(TUIC_MCP_KEY.to_string(), entry_value);
    } else {
        *servers = serde_json::json!({ TUIC_MCP_KEY: entry_value });
    }

    write_json_file(&spec.config_path, &root)
}

/// Remove the tui-mcp-bridge MCP entry from an agent's config
#[tauri::command]
pub(crate) fn remove_agent_mcp(agent_type: String) -> Result<(), String> {
    let spec = get_mcp_config_spec(&agent_type)
        .ok_or_else(|| format!("Agent '{agent_type}' does not support MCP configuration"))?;

    if !spec.config_path.exists() {
        return Ok(()); // Nothing to remove
    }

    let mut root = read_json_file(&spec.config_path);
    let servers = navigate_or_create(&mut root, &spec.key_path);

    if let Some(obj) = servers.as_object_mut() {
        obj.remove(TUIC_MCP_KEY);
    }

    write_json_file(&spec.config_path, &root)
}

/// Get the path to an agent's own configuration file
#[tauri::command]
pub(crate) fn get_agent_config_path(agent_type: String) -> Option<String> {
    get_agent_settings_path(&agent_type).map(|p| p.to_string_lossy().to_string())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_json(dir: &std::path::Path, name: &str, value: &serde_json::Value) {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, serde_json::to_string_pretty(value).unwrap()).unwrap();
    }

    #[test]
    fn unsupported_agent_returns_not_supported() {
        let status = get_agent_mcp_status("aider".to_string());
        assert!(!status.supported);
        assert!(!status.installed);
        assert!(status.config_path.is_none());
    }

    #[test]
    fn install_remove_round_trip_json() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("test-mcp.json");

        // Start with existing config that has another server
        let initial = serde_json::json!({
            "mcpServers": {
                "other-server": { "command": "other-cmd" }
            }
        });
        write_json_file(&config_path, &initial).unwrap();

        // Simulate install
        let mut root = read_json_file(&config_path);
        let entry = TuicMcpEntry {
            command: "/usr/local/bin/tui-mcp-bridge".to_string(),
            args: vec![],
        };
        let entry_value = serde_json::to_value(&entry).unwrap();
        let servers = navigate_or_create(&mut root, &["mcpServers"]);
        servers.as_object_mut().unwrap().insert(TUIC_MCP_KEY.to_string(), entry_value);
        write_json_file(&config_path, &root).unwrap();

        // Verify both entries exist
        let root = read_json_file(&config_path);
        let servers = navigate(&root, &["mcpServers"]).unwrap().as_object().unwrap();
        assert!(servers.contains_key(TUIC_MCP_KEY));
        assert!(servers.contains_key("other-server"));
        assert_eq!(servers.len(), 2);

        // Simulate remove
        let mut root = read_json_file(&config_path);
        let servers = navigate_or_create(&mut root, &["mcpServers"]);
        servers.as_object_mut().unwrap().remove(TUIC_MCP_KEY);
        write_json_file(&config_path, &root).unwrap();

        // Verify only other-server remains
        let root = read_json_file(&config_path);
        let servers = navigate(&root, &["mcpServers"]).unwrap().as_object().unwrap();
        assert!(!servers.contains_key(TUIC_MCP_KEY));
        assert!(servers.contains_key("other-server"));
        assert_eq!(servers.len(), 1);
    }

    #[test]
    fn install_creates_file_if_missing() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("nonexistent.json");

        // File doesn't exist yet
        assert!(!config_path.exists());

        let mut root = read_json_file(&config_path);
        let entry = TuicMcpEntry {
            command: "tui-mcp-bridge".to_string(),
            args: vec![],
        };
        let entry_value = serde_json::to_value(&entry).unwrap();
        let servers = navigate_or_create(&mut root, &["mcpServers"]);
        servers.as_object_mut().unwrap().insert(TUIC_MCP_KEY.to_string(), entry_value);
        write_json_file(&config_path, &root).unwrap();

        // Verify file was created with correct content
        let root = read_json_file(&config_path);
        let servers = navigate(&root, &["mcpServers"]).unwrap().as_object().unwrap();
        assert!(servers.contains_key(TUIC_MCP_KEY));
        let entry = servers.get(TUIC_MCP_KEY).unwrap();
        assert_eq!(entry["command"], "tui-mcp-bridge");
    }

    #[test]
    fn nested_key_path_works() {
        // Test with amp-style nested path: ["amp", "mcpServers"]
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("amp-settings.json");

        let initial = serde_json::json!({
            "amp": {
                "someOtherSetting": true
            }
        });
        write_json_file(&config_path, &initial).unwrap();

        let mut root = read_json_file(&config_path);
        let entry = serde_json::json!({ "command": "tui-mcp-bridge" });
        let servers = navigate_or_create(&mut root, &["amp", "mcpServers"]);
        servers.as_object_mut().unwrap().insert(TUIC_MCP_KEY.to_string(), entry);
        write_json_file(&config_path, &root).unwrap();

        let root = read_json_file(&config_path);
        // Verify the nested structure
        assert_eq!(root["amp"]["someOtherSetting"], true);
        assert!(root["amp"]["mcpServers"][TUIC_MCP_KEY].is_object());
    }

    #[test]
    fn remove_from_nonexistent_file_is_ok() {
        // Removing from a file that doesn't exist should succeed (no-op)
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("does-not-exist.json");

        // This should not create the file
        if config_path.exists() {
            let mut root = read_json_file(&config_path);
            let servers = navigate_or_create(&mut root, &["mcpServers"]);
            if let Some(obj) = servers.as_object_mut() {
                obj.remove(TUIC_MCP_KEY);
            }
            write_json_file(&config_path, &root).unwrap();
        }
        // File should still not exist
        assert!(!config_path.exists());
    }

    #[test]
    fn agent_config_path_returns_expected_paths() {
        // Claude should return ~/.claude/settings.json
        let claude_path = get_agent_settings_path("claude");
        assert!(claude_path.is_some());
        let path_str = claude_path.unwrap().to_string_lossy().to_string();
        assert!(path_str.contains(".claude"));
        assert!(path_str.ends_with("settings.json"));

        // Unknown agent returns None
        assert!(get_agent_settings_path("unknown-agent").is_none());
    }

    #[test]
    fn mcp_config_spec_known_agents() {
        // Verify supported agents have specs
        for agent in &["claude", "cursor", "windsurf", "vscode", "zed", "amp", "gemini"] {
            assert!(get_mcp_config_spec(agent).is_some(), "{agent} should be supported");
        }
        // Verify unsupported agents don't
        for agent in &["aider", "warp", "opencode", "codex", "droid"] {
            assert!(get_mcp_config_spec(agent).is_none(), "{agent} should not be supported");
        }
    }
}
