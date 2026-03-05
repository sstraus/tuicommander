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
    #[serde(rename = "type", default = "default_stdio_type")]
    transport_type: String,
    command: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    args: Vec<String>,
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
    let existing_command = navigate(&root, key_path)
        .and_then(|v| v.as_object())
        .and_then(|obj| obj.get(TUIC_MCP_KEY))
        .and_then(|entry| entry.get("command"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    match existing_command {
        Some(ref cmd) if cmd == bridge_path => {
            // Already correct — no write needed
            return false;
        }
        Some(ref old) => {
            eprintln!("MCP: {agent_label} — updating path: {old} → {bridge_path}");
        }
        None => {
            eprintln!("MCP: {agent_label} — installing bridge");
        }
    }

    let entry = TuicMcpEntry {
        transport_type: "stdio".to_string(),
        command: bridge_path.to_string(),
        args: vec![],
    };
    let entry_value = match serde_json::to_value(&entry) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("MCP: {agent_label} — serialize error: {e}");
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
            eprintln!("MCP: {agent_label} — written to {}", config_path.display());
            true
        }
        Err(e) => {
            eprintln!("MCP: {agent_label} — write error: {e}");
            false
        }
    }
}

/// Ensure MCP bridge config is installed and up-to-date in all supported agent configs.
/// Called on every app launch. Installs missing entries and updates stale paths.
pub(crate) fn ensure_mcp_configs() {
    // On Windows, skip until named pipe bridge is functional
    if cfg!(windows) {
        eprintln!("MCP: skipping auto-install on Windows (named pipe transport pending)");
        return;
    }

    let bridge_path = detect_bridge_binary();
    eprintln!("MCP: ensuring bridge configs (bridge: {bridge_path})");

    for agent in SUPPORTED_AGENTS {
        let Some(spec) = get_mcp_config_spec(agent) else { continue };
        ensure_agent_mcp_entry(&spec.config_path, &spec.key_path, &bridge_path, agent);
    }
}

// ---------------------------------------------------------------------------
// Agent Teams it2 shim
// ---------------------------------------------------------------------------

/// Shell script content for the `it2` shim.
/// Translates iTerm2 CLI commands into TUIC HTTP API calls over Unix socket.
const IT2_SHIM_SCRIPT: &str = r#"#!/bin/bash
# it2 shim — translates iTerm2 CLI commands to TUICommander HTTP API.
# Auto-installed by TUICommander when Agent Teams shim is enabled.
set -euo pipefail

SOCKET="${TUIC_SOCKET_PATH:-}"
if [ -z "$SOCKET" ]; then
  echo "Error: TUIC_SOCKET_PATH not set" >&2
  exit 1
fi

curl_sock() {
  curl -sS --unix-socket "$SOCKET" "http://localhost$1" "${@:2}"
}

case "${1:-}" in
  --version)
    echo "it2 (TUICommander shim) 1.0.0"
    ;;
  session)
    case "${2:-}" in
      split)
        # Parse flags: -v (vertical), -s <session_id>
        shift 2
        while [ $# -gt 0 ]; do
          case "$1" in
            -v) shift ;;
            -s) shift; shift ;;  # parent session id — ignored, TUIC manages layout
            *) shift ;;
          esac
        done
        RESP=$(curl_sock "/sessions" -X POST -H "Content-Type: application/json" -d '{"rows":24,"cols":80}')
        SID=$(echo "$RESP" | grep -o '"session_id":"[^"]*"' | head -1 | cut -d'"' -f4)
        if [ -n "$SID" ]; then
          echo "Created new pane: $SID"
        else
          echo "Error: failed to create session" >&2
          exit 1
        fi
        ;;
      run)
        # it2 session run -s <session_id> <command...>
        shift 2
        SID=""
        while [ $# -gt 0 ]; do
          case "$1" in
            -s) SID="$2"; shift 2 ;;
            *) break ;;
          esac
        done
        CMD="$*"
        if [ -z "$SID" ] || [ -z "$CMD" ]; then
          echo "Usage: it2 session run -s <session_id> <command>" >&2
          exit 1
        fi
        # Append newline so the command executes
        PAYLOAD=$(printf '{"data":"%s\\n"}' "$CMD")
        curl_sock "/sessions/$SID/write" -X POST -H "Content-Type: application/json" -d "$PAYLOAD" >/dev/null
        ;;
      close)
        # it2 session close -s <session_id>
        shift 2
        SID=""
        while [ $# -gt 0 ]; do
          case "$1" in
            -s) SID="$2"; shift 2 ;;
            *) shift ;;
          esac
        done
        if [ -z "$SID" ]; then
          echo "Usage: it2 session close -s <session_id>" >&2
          exit 1
        fi
        curl_sock "/sessions/$SID" -X DELETE >/dev/null
        ;;
      list)
        curl_sock "/sessions"
        ;;
      *)
        echo "Unknown session command: ${2:-}" >&2
        exit 1
        ;;
    esac
    ;;
  *)
    echo "it2 (TUICommander shim) — supported: --version, session split|run|close|list" >&2
    exit 0
    ;;
esac
"#;

/// Install the `it2` shim script to `~/.tuicommander/bin/it2`.
/// Called on app startup when `agent_teams_shim` is enabled in config.
pub(crate) fn install_it2_shim() {
    let Some(home) = dirs::home_dir() else {
        eprintln!("Agent Teams: cannot determine home directory, skipping it2 shim install");
        return;
    };
    let bin_dir = home.join(".tuicommander").join("bin");
    if let Err(e) = std::fs::create_dir_all(&bin_dir) {
        eprintln!("Agent Teams: failed to create {}: {e}", bin_dir.display());
        return;
    }
    let shim_path = bin_dir.join("it2");
    if let Err(e) = std::fs::write(&shim_path, IT2_SHIM_SCRIPT) {
        eprintln!("Agent Teams: failed to write {}: {e}", shim_path.display());
        return;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = std::fs::set_permissions(&shim_path, std::fs::Permissions::from_mode(0o755)) {
            eprintln!("Agent Teams: failed to chmod {}: {e}", shim_path.display());
            return;
        }
    }
    eprintln!("Agent Teams: installed it2 shim at {}", shim_path.display());
}

/// Tauri command to install the it2 shim on demand (e.g. from Settings UI toggle).
#[tauri::command]
pub(crate) fn install_it2_shim_cmd() {
    install_it2_shim();
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
        transport_type: "stdio".to_string(),
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

    // --- it2 shim tests ---

    #[test]
    fn it2_shim_script_contains_created_new_pane() {
        // Claude Code parses output with regex: /Created new pane:\s*(.+)/
        assert!(IT2_SHIM_SCRIPT.contains("Created new pane:"));
    }

    #[test]
    fn it2_shim_script_starts_with_shebang() {
        assert!(IT2_SHIM_SCRIPT.starts_with("#!/bin/bash"));
    }

    #[test]
    fn it2_shim_script_handles_all_required_commands() {
        for cmd in &["--version", "split", "run", "close", "list"] {
            assert!(IT2_SHIM_SCRIPT.contains(cmd), "script must handle {cmd}");
        }
    }

    #[test]
    fn it2_shim_installs_to_disk() {
        let dir = TempDir::new().unwrap();
        let bin_dir = dir.path().join(".tuicommander").join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let shim_path = bin_dir.join("it2");
        std::fs::write(&shim_path, IT2_SHIM_SCRIPT).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&shim_path, std::fs::Permissions::from_mode(0o755)).unwrap();
            let perms = std::fs::metadata(&shim_path).unwrap().permissions().mode();
            assert_eq!(perms & 0o111, 0o111, "script should be executable");
        }

        let content = std::fs::read_to_string(&shim_path).unwrap();
        assert!(content.starts_with("#!/bin/bash"));
    }

    #[test]
    fn it2_shim_uses_unix_socket() {
        assert!(IT2_SHIM_SCRIPT.contains("--unix-socket"));
        assert!(IT2_SHIM_SCRIPT.contains("TUIC_SOCKET_PATH"));
    }
}
