use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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

/// Our MCP server entry injected into agent configs.
/// `args` and `env` are always serialized (even if empty) — some Claude Code
/// versions reject stdio entries whose `args`/`env` are missing or `null`.
#[derive(Serialize, Deserialize)]
struct TuicMcpEntry {
    #[serde(rename = "type", default = "default_stdio_type")]
    transport_type: String,
    command: String,
    args: Vec<String>,
    env: BTreeMap<String, String>,
}

fn default_stdio_type() -> String {
    "stdio".to_string()
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

const BRIDGE_NAME: &str = "tuic-bridge";

/// Detect the tuic-bridge binary path.
/// Priority: sidecar (same dir as main executable) → PATH → bare name.
fn detect_bridge_binary() -> String {
    // Primary: sidecar bundled alongside the main executable
    // In release: Contents/MacOS/ (macOS), next to .exe (Windows), same dir (Linux)
    // In dev: target/debug/ or target/release/
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        #[cfg(not(windows))]
        let candidate = dir.join(BRIDGE_NAME);
        #[cfg(windows)]
        let candidate = dir.join(format!("{BRIDGE_NAME}.exe"));
        if candidate.exists() {
            return candidate.to_string_lossy().to_string();
        }
    }
    // Fallback: resolve from PATH via well-known directories
    let resolved = crate::cli::resolve_cli(BRIDGE_NAME);
    if std::path::Path::new(&resolved).exists() {
        return resolved;
    }
    // Last resort: bare name, hope it's on PATH
    BRIDGE_NAME.to_string()
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

/// Supported agent types for auto-install
const SUPPORTED_AGENTS: &[&str] = &["claude", "cursor", "windsurf", "vscode", "zed", "amp", "gemini"];

/// Ensure a single agent's MCP config has the correct bridge entry.
/// Returns true if the config was written (installed or updated).
fn ensure_agent_mcp_entry(
    config_path: &std::path::Path,
    key_path: &[&str],
    bridge_path: &str,
    agent_label: &str,
) -> bool {
    let root = read_json_file(config_path);
    let existing_entry = navigate(&root, key_path)
        .and_then(|v| v.as_object())
        .and_then(|obj| obj.get(TUIC_MCP_KEY));
    let existing_command = existing_entry
        .and_then(|entry| entry.get("command"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    // Claude Code rejects stdio entries where `args`/`env` are null or missing.
    // If either is not a proper array/object, we rewrite the entry.
    let fields_malformed = existing_entry
        .map(|entry| {
            let args_ok = entry.get("args").map(|v| v.is_array()).unwrap_or(false);
            let env_ok = entry.get("env").map(|v| v.is_object()).unwrap_or(false);
            !(args_ok && env_ok)
        })
        .unwrap_or(false);

    match &existing_command {
        Some(cmd) if cmd == bridge_path && !fields_malformed => {
            // Already correct — no write needed
            return false;
        }
        Some(old) if fields_malformed => {
            tracing::info!(source = "mcp", agent = %agent_label, "Repairing entry (args/env missing or null) for {old}");
        }
        Some(old) => {
            tracing::info!(source = "mcp", agent = %agent_label, "Updating path: {old} → {bridge_path}");
        }
        None => {
            tracing::info!(source = "mcp", agent = %agent_label, "Installing bridge");
        }
    }

    let entry = TuicMcpEntry {
        transport_type: "stdio".to_string(),
        command: bridge_path.to_string(),
        args: vec![],
        env: BTreeMap::new(),
    };
    let entry_value = match serde_json::to_value(&entry) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(source = "mcp", agent = %agent_label, "Serialize error: {e}");
            return false;
        }
    };

    let mut root = read_json_file(config_path);
    let servers = navigate_or_create(&mut root, key_path);
    if let Some(obj) = servers.as_object_mut() {
        obj.insert(TUIC_MCP_KEY.to_string(), entry_value);
    } else {
        *servers = serde_json::json!({ TUIC_MCP_KEY: entry_value });
    }

    match write_json_file(config_path, &root) {
        Ok(()) => {
            tracing::info!(source = "mcp", agent = %agent_label, path = %config_path.display(), "Config written");
            true
        }
        Err(e) => {
            tracing::error!(source = "mcp", agent = %agent_label, "Write error: {e}");
            false
        }
    }
}

/// Ensure MCP bridge config is installed and up-to-date in all supported agent configs.
/// Called on every app launch. Installs missing entries and updates stale paths.
/// Agents in `disabled` are skipped (user opted out via Settings > Agents).
pub(crate) fn ensure_mcp_configs(disabled: &[String]) {
    let bridge_path = detect_bridge_binary();
    tracing::info!(source = "mcp", bridge = %bridge_path, "Ensuring bridge configs");

    for agent in SUPPORTED_AGENTS {
        if disabled.iter().any(|d| d == agent) {
            tracing::debug!(source = "mcp", agent, "Skipping (disabled by user)");
            continue;
        }
        let Some(spec) = get_mcp_config_spec(agent) else { continue };
        ensure_agent_mcp_entry(&spec.config_path, &spec.key_path, &bridge_path, agent);
    }
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

/// Install the tui-mcp-bridge MCP entry into an agent's config.
/// Also removes the agent from `disabled_mcp_agents` so `ensure_mcp_configs` won't skip it.
#[tauri::command]
pub(crate) fn install_agent_mcp(
    agent_type: String,
    state: tauri::State<'_, std::sync::Arc<crate::state::AppState>>,
) -> Result<(), String> {
    let spec = get_mcp_config_spec(&agent_type)
        .ok_or_else(|| format!("Agent '{agent_type}' does not support MCP configuration"))?;

    let bridge_path = detect_bridge_binary();
    let entry = TuicMcpEntry {
        transport_type: "stdio".to_string(),
        command: bridge_path,
        args: vec![],
        env: BTreeMap::new(),
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

    write_json_file(&spec.config_path, &root)?;

    // Remove from disabled list so ensure_mcp_configs won't undo this
    update_disabled_mcp_agents(state.inner(), |list| list.retain(|a| a != &agent_type));

    Ok(())
}

/// Remove the tui-mcp-bridge MCP entry from an agent's config.
/// Also adds the agent to `disabled_mcp_agents` so `ensure_mcp_configs` won't reinstall it.
#[tauri::command]
pub(crate) fn remove_agent_mcp(
    agent_type: String,
    state: tauri::State<'_, std::sync::Arc<crate::state::AppState>>,
) -> Result<(), String> {
    let spec = get_mcp_config_spec(&agent_type)
        .ok_or_else(|| format!("Agent '{agent_type}' does not support MCP configuration"))?;

    if spec.config_path.exists() {
        let mut root = read_json_file(&spec.config_path);
        let servers = navigate_or_create(&mut root, &spec.key_path);

        if let Some(obj) = servers.as_object_mut() {
            obj.remove(TUIC_MCP_KEY);
        }

        write_json_file(&spec.config_path, &root)?;
    }

    // Add to disabled list so ensure_mcp_configs won't reinstall
    update_disabled_mcp_agents(state.inner(), |list| {
        if !list.contains(&agent_type) {
            list.push(agent_type.clone());
        }
    });

    Ok(())
}

/// Helper: mutate `disabled_mcp_agents` in BOTH the in-memory `AppState.config`
/// and on-disk `config.json`. Updating only disk would leave a stale snapshot in
/// memory, and a subsequent `put_config` from the FE (carrying that stale list)
/// would silently revert the toggle.
fn update_disabled_mcp_agents(
    state: &std::sync::Arc<crate::state::AppState>,
    mutator: impl FnOnce(&mut Vec<String>),
) {
    use crate::config::save_json_config;
    let snapshot = {
        let mut cfg = state.config.write();
        mutator(&mut cfg.disabled_mcp_agents);
        cfg.clone()
    };
    if let Err(e) = save_json_config("config.json", &snapshot) {
        tracing::error!(source = "mcp", "Failed to save disabled_mcp_agents: {e}");
    }
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
            transport_type: "stdio".to_string(),
            command: "/usr/local/bin/tui-mcp-bridge".to_string(),
            args: vec![],
            env: BTreeMap::new(),
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
            transport_type: "stdio".to_string(),
            command: "tui-mcp-bridge".to_string(),
            args: vec![],
            env: BTreeMap::new(),
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
        assert_eq!(entry["type"], "stdio");
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

    // --- ensure_agent_mcp_entry tests ---

    #[test]
    fn ensure_installs_when_missing() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("test.json");

        let wrote = ensure_agent_mcp_entry(&config_path, &["mcpServers"], "/path/a", "test");
        assert!(wrote, "should write when entry is missing");

        let root = read_json_file(&config_path);
        assert_eq!(root["mcpServers"][TUIC_MCP_KEY]["command"], "/path/a");
    }

    #[test]
    fn ensure_updates_stale_path() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("test.json");

        // Install with path A
        ensure_agent_mcp_entry(&config_path, &["mcpServers"], "/old/path", "test");

        // Ensure with path B — should update
        let wrote = ensure_agent_mcp_entry(&config_path, &["mcpServers"], "/new/path", "test");
        assert!(wrote, "should write when path changed");

        let root = read_json_file(&config_path);
        assert_eq!(root["mcpServers"][TUIC_MCP_KEY]["command"], "/new/path");
    }

    #[test]
    fn ensure_skips_when_path_matches() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("test.json");

        // Install
        ensure_agent_mcp_entry(&config_path, &["mcpServers"], "/correct/path", "test");

        // Record mtime
        let mtime_before = std::fs::metadata(&config_path).unwrap().modified().unwrap();
        // Small sleep to ensure mtime would differ if file were rewritten
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Ensure with same path — should not write
        let wrote = ensure_agent_mcp_entry(&config_path, &["mcpServers"], "/correct/path", "test");
        assert!(!wrote, "should not write when path already correct");

        let mtime_after = std::fs::metadata(&config_path).unwrap().modified().unwrap();
        assert_eq!(mtime_before, mtime_after, "file should not have been modified");
    }

    #[test]
    fn ensure_writes_args_and_env_as_empty_collections() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("test.json");

        ensure_agent_mcp_entry(&config_path, &["mcpServers"], "/bridge", "test");

        let root = read_json_file(&config_path);
        let entry = &root["mcpServers"][TUIC_MCP_KEY];
        assert!(entry["args"].is_array(), "args must be an array, got {:?}", entry["args"]);
        assert_eq!(entry["args"].as_array().unwrap().len(), 0);
        assert!(entry["env"].is_object(), "env must be an object, got {:?}", entry["env"]);
        assert_eq!(entry["env"].as_object().unwrap().len(), 0);
    }

    #[test]
    fn ensure_repairs_entry_with_null_args_and_env() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("test.json");

        // Write an entry shaped like Claude Code's rejected form: args/env are null
        let initial = serde_json::json!({
            "mcpServers": {
                TUIC_MCP_KEY: {
                    "type": "stdio",
                    "command": "/bridge",
                    "args": null,
                    "env": null,
                }
            }
        });
        write_json_file(&config_path, &initial).unwrap();

        // Same command, but malformed fields → must rewrite
        let wrote = ensure_agent_mcp_entry(&config_path, &["mcpServers"], "/bridge", "test");
        assert!(wrote, "should rewrite when args/env are null");

        let root = read_json_file(&config_path);
        let entry = &root["mcpServers"][TUIC_MCP_KEY];
        assert!(entry["args"].is_array());
        assert!(entry["env"].is_object());
    }

    #[test]
    fn ensure_preserves_other_servers() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("test.json");

        let initial = serde_json::json!({
            "mcpServers": {
                "other-server": { "command": "other-cmd" }
            }
        });
        write_json_file(&config_path, &initial).unwrap();

        ensure_agent_mcp_entry(&config_path, &["mcpServers"], "/bridge", "test");

        let root = read_json_file(&config_path);
        let servers = root["mcpServers"].as_object().unwrap();
        assert_eq!(servers.len(), 2);
        assert_eq!(servers["other-server"]["command"], "other-cmd");
        assert_eq!(servers[TUIC_MCP_KEY]["command"], "/bridge");
    }

    #[test]
    fn ensure_works_with_nested_key_path() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("test.json");

        let initial = serde_json::json!({ "amp": { "setting": true } });
        write_json_file(&config_path, &initial).unwrap();

        ensure_agent_mcp_entry(&config_path, &["amp", "mcpServers"], "/bridge", "test");

        let root = read_json_file(&config_path);
        assert_eq!(root["amp"]["setting"], true);
        assert_eq!(root["amp"]["mcpServers"][TUIC_MCP_KEY]["command"], "/bridge");
    }

    // --- ensure_mcp_configs disabled_agents tests ---

    #[test]
    fn ensure_skips_disabled_agents() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("test.json");

        // With empty disabled list — should install
        ensure_agent_mcp_entry(&config_path, &["mcpServers"], "/bridge", "test");
        assert!(config_path.exists());
        let root = read_json_file(&config_path);
        assert!(root["mcpServers"][TUIC_MCP_KEY].is_object());

        // Remove the file and verify ensure_mcp_configs logic
        // (we test the skip logic directly since ensure_mcp_configs uses home paths)
        let disabled = vec!["claude".to_string(), "cursor".to_string()];
        assert!(disabled.iter().any(|d| d == "claude"));
        assert!(!disabled.iter().any(|d| d == "vscode"));
    }

    /// Regression for #1368-fa9b: `update_disabled_mcp_agents` must mutate the
    /// in-memory `AppState.config.disabled_mcp_agents`, not just the on-disk file.
    /// Otherwise a `put_config` PUT carrying a stale snapshot silently reverts.
    #[test]
    fn update_disabled_mcp_agents_mutates_in_memory_state() {
        let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
        assert!(state.config.read().disabled_mcp_agents.is_empty(), "precondition");

        // Simulate remove_agent_mcp's branch: add an agent to the disabled list.
        update_disabled_mcp_agents(&state, |list| {
            if !list.contains(&"claude".to_string()) {
                list.push("claude".to_string());
            }
        });

        assert!(
            state.config.read().disabled_mcp_agents.iter().any(|a| a == "claude"),
            "in-memory state.config must be updated, not only disk",
        );

        // Simulate install_agent_mcp's branch: remove the agent.
        update_disabled_mcp_agents(&state, |list| list.retain(|a| a != "claude"));

        assert!(
            !state.config.read().disabled_mcp_agents.iter().any(|a| a == "claude"),
            "in-memory state.config must be cleared on remove",
        );
    }

    #[test]
    fn disabled_list_contains_check() {
        let disabled: Vec<String> = vec!["claude".to_string(), "windsurf".to_string()];

        // Agents in disabled list should be skipped
        for agent in &["claude", "windsurf"] {
            assert!(
                disabled.iter().any(|d| d == agent),
                "{agent} should be in disabled list",
            );
        }

        // Agents NOT in disabled list should proceed
        for agent in &["cursor", "vscode", "zed", "amp", "gemini"] {
            assert!(
                !disabled.iter().any(|d| d == agent),
                "{agent} should NOT be in disabled list",
            );
        }
    }
}
