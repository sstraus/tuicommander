//! Tauri commands + browser-route cores for hook-based agent state instrumentation.
//!
//! The toggle is the source of truth: enabling persists `hook_instrumentation` in
//! `AgentsConfig` and installs the agent's hooks; disabling persists `false` and
//! removes only TUIC's entries. Install state (for the UI badge) is read back from
//! the agent's settings file via the installer.
//!
//! Only agents whose hook shape is a JSON settings-hook are wired here (A1: Claude,
//! Gemini). Codex/Grok/OpenCode arrive in later stories with their own adapters.

use crate::agent_hook::{HookEntry, claude_hook_map, gemini_hook_map};
use crate::agent_hook_installer::{self, InstallState};
use std::path::{Path, PathBuf};

/// Hook map for agents that support settings-hook instrumentation, or `None` for
/// agents without a (wired) hook system.
fn hook_map_for(agent_type: &str) -> Option<Vec<HookEntry>> {
    match agent_type {
        "claude" => Some(claude_hook_map()),
        "gemini" => Some(gemini_hook_map()),
        _ => None,
    }
}

/// The settings file an agent reads its hooks from. Claude/Gemini merge into a
/// shared `settings.json`; Grok owns its own `~/.grok/hooks/tuic.json`.
fn hook_settings_path(agent_type: &str) -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    match agent_type {
        "claude" => Some(home.join(".claude/settings.json")),
        "gemini" => Some(home.join(".gemini/settings.json")),
        "grok" => Some(home.join(".grok/hooks/tuic.json")),
        _ => None,
    }
}

/// Install/uninstall the hooks at an explicit path. Path-injected for testing.
/// Dispatches by the agent's strategy: Claude/Gemini merge into a shared file;
/// Grok owns its whole file.
pub(crate) fn apply_at(
    agent_type: &str,
    settings_path: &Path,
    enabled: bool,
) -> Result<(), String> {
    match agent_type {
        "claude" | "gemini" => {
            let map = hook_map_for(agent_type).expect("merge agent has a hook map");
            if enabled {
                agent_hook_installer::install(settings_path, &map)
            } else {
                agent_hook_installer::uninstall(settings_path)
            }
        }
        "grok" => {
            if enabled {
                agent_hook_installer::install_own_file(settings_path, &crate::agent_hook::grok_hook_map())
            } else {
                agent_hook_installer::uninstall_own_file(settings_path)
            }
        }
        _ => Ok(()), // unsupported agent: flag persists, nothing to install
    }
}

/// Install state for an agent at an explicit path. Path-injected for testing.
/// `install_state` works for both merge and own-file files — it scans hook
/// commands for the sentinel regardless of how they were written.
pub(crate) fn state_at(agent_type: &str, settings_path: &Path) -> InstallState {
    let map = match agent_type {
        "claude" | "gemini" => hook_map_for(agent_type),
        "grok" => Some(crate::agent_hook::grok_hook_map()),
        _ => None,
    };
    match map {
        Some(map) => agent_hook_installer::install_state(settings_path, &map),
        None => InstallState::NotInstalled,
    }
}

/// Persist `hook_instrumentation` for an agent into `AgentsConfig`.
fn persist_flag(agent_type: &str, enabled: bool) -> Result<(), String> {
    let mut cfg = crate::config::load_agents_config();
    cfg.agents
        .entry(agent_type.to_string())
        .or_default()
        .hook_instrumentation = Some(enabled);
    crate::config::save_agents_config(cfg)
}

/// Toggle hook instrumentation for an agent: persist the flag, then install (or
/// uninstall) the hooks in the agent's settings file. Effect applies on the next
/// agent launch.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn set_agent_hook_instrumentation(
    agent_type: String,
    enabled: bool,
) -> Result<(), String> {
    persist_flag(&agent_type, enabled)?;
    if let Some(path) = hook_settings_path(&agent_type) {
        apply_at(&agent_type, &path, enabled)?;
    }
    Ok(())
}

/// Report install state for the agent's hooks: `installed` / `outdated` /
/// `notInstalled`, or `unsupported` for agents without a (wired) hook system.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn get_agent_hook_state(agent_type: String) -> String {
    match hook_settings_path(&agent_type) {
        Some(path) => state_at(&agent_type, &path).as_str().to_string(),
        None => "unsupported".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn config_agent_hook_apply_installs_then_uninstalls() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        apply_at("claude", &path, true).unwrap();
        assert_eq!(state_at("claude", &path), InstallState::Installed);
        apply_at("claude", &path, false).unwrap();
        assert_eq!(state_at("claude", &path), InstallState::NotInstalled);
    }

    #[test]
    fn config_agent_hook_unsupported_agent_writes_nothing() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        apply_at("aider", &path, true).unwrap();
        assert!(
            !path.exists(),
            "unsupported agent must not write a settings file"
        );
        assert_eq!(state_at("aider", &path), InstallState::NotInstalled);
    }

    #[test]
    fn config_agent_hook_gemini_uses_its_own_map() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        apply_at("gemini", &path, true).unwrap();
        assert_eq!(state_at("gemini", &path), InstallState::Installed);
        // Same file judged against Claude's map: different command multiset → outdated.
        assert_eq!(state_at("claude", &path), InstallState::Outdated);
    }

    #[test]
    fn config_agent_hook_flag_roundtrips() {
        let mut s = crate::config::AgentSettings::default();
        s.hook_instrumentation = Some(true);
        let json = serde_json::to_string(&s).unwrap();
        let back: crate::config::AgentSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.hook_instrumentation, Some(true));
    }

    #[test]
    fn agent_hook_grok_installs_own_file_leaving_siblings() {
        let dir = TempDir::new().unwrap();
        let sibling = dir.path().join("user-hook.json");
        std::fs::write(&sibling, b"{\"keep\":true}").unwrap();
        let ours = dir.path().join("tuic.json");
        apply_at("grok", &ours, true).unwrap();
        assert!(ours.exists());
        assert_eq!(state_at("grok", &ours), InstallState::Installed);
        assert_eq!(
            std::fs::read(&sibling).unwrap(),
            b"{\"keep\":true}",
            "sibling hook files in the grok hooks dir are never touched"
        );
    }

    #[test]
    fn agent_hook_grok_uninstall_deletes_only_our_file() {
        let dir = TempDir::new().unwrap();
        let sibling = dir.path().join("user-hook.json");
        std::fs::write(&sibling, b"x").unwrap();
        let ours = dir.path().join("tuic.json");
        apply_at("grok", &ours, true).unwrap();
        apply_at("grok", &ours, false).unwrap();
        assert!(!ours.exists(), "our file is deleted on uninstall");
        assert!(sibling.exists(), "sibling preserved");
        assert_eq!(state_at("grok", &ours), InstallState::NotInstalled);
    }

    #[test]
    fn agent_hook_grok_commands_carry_osc_guard_and_omit_lifecycle_matcher() {
        let dir = TempDir::new().unwrap();
        let ours = dir.path().join("tuic.json");
        apply_at("grok", &ours, true).unwrap();
        let content = std::fs::read_to_string(&ours).unwrap();
        assert!(content.contains(r"7770;state=busy"));
        assert!(content.contains(r"7770;state=idle"));
        assert!(content.contains("TUIC_SESSION"));
        assert!(content.contains("tuic-managed-hook"));
        let v: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert!(
            v["hooks"]["Stop"][0].get("matcher").is_none(),
            "lifecycle events must omit the matcher field (Grok rejects it)"
        );
    }
}
