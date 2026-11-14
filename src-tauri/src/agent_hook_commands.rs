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
    #[cfg(test)]
    if let Some(root) = crate::agent_test::test_root() {
        return Some(root.join(agent_type).join("settings.json"));
    }
    let home = dirs::home_dir()?;
    match agent_type {
        "claude" => Some(home.join(".claude/settings.json")),
        "gemini" => Some(home.join(".gemini/settings.json")),
        "grok" => Some(home.join(".grok/hooks/tuic.json")),
        "codex" => Some(
            std::env::var_os("CODEX_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".codex"))
                .join("hooks.json"),
        ),
        "opencode" => Some(home.join(".config/opencode/plugin/tuic.ts")),
        "pi" => Some(
            std::env::var("PI_CODING_AGENT_DIR")
                .ok()
                .filter(|value| !value.is_empty())
                .map(|value| PathBuf::from(crate::cli::expand_tilde(&value)))
                .unwrap_or_else(|| home.join(".pi/agent"))
                .join("extensions/tuic.ts"),
        ),
        _ => None,
    }
}

/// Codex stores its enable flag in a `config.toml` sibling of the hooks file.
fn codex_config_toml(hooks_path: &Path) -> Option<PathBuf> {
    hooks_path.parent().map(|d| d.join("config.toml"))
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
                agent_hook_installer::install(settings_path, &map, None)
            } else {
                agent_hook_installer::uninstall(settings_path)
            }
        }
        "grok" => {
            if enabled {
                agent_hook_installer::install_own_file(
                    settings_path,
                    &crate::agent_hook::grok_hook_map(),
                )
            } else {
                agent_hook_installer::uninstall_own_file(settings_path)
            }
        }
        "codex" => {
            // Codex merges hooks into hooks.json AND needs `[features] hooks = true`
            // in config.toml. Toggle both symmetrically (enable flag first on
            // install; clear it after uninstall) so install_state never strands.
            let map = crate::agent_hook::codex_hook_map();
            let cfg = codex_config_toml(settings_path);
            if enabled {
                if let Some(cfg) = &cfg {
                    crate::agent_hook_codex::set_features_hooks_flag(cfg, true)?;
                }
                agent_hook_installer::install(
                    settings_path,
                    &map,
                    Some(crate::agent_hook::CODEX_HOOK_LABEL),
                )?;
            } else {
                agent_hook_installer::uninstall(settings_path)?;
                if let Some(cfg) = &cfg {
                    crate::agent_hook_codex::set_features_hooks_flag(cfg, false)?;
                }
            }
            Ok(())
        }
        "opencode" => {
            if enabled {
                crate::agent_hook_opencode::install_opencode_plugin(settings_path)
            } else {
                crate::agent_hook_opencode::uninstall_opencode_plugin(settings_path)
            }
        }
        "pi" => {
            if enabled {
                crate::agent_hook_pi::install_pi_extension(settings_path)
            } else {
                crate::agent_hook_pi::uninstall_pi_extension(settings_path)
            }
        }
        _ => Ok(()), // unsupported agent: flag persists, nothing to install
    }
}

/// Install state for an agent at an explicit path. Path-injected for testing.
/// `install_state` works for both merge and own-file files — it scans hook
/// commands for the sentinel regardless of how they were written.
pub(crate) fn state_at(agent_type: &str, settings_path: &Path) -> InstallState {
    // Codex state combines the hooks file AND the config.toml enable flag — both
    // present = reviewRequired until native verification, both absent = notInstalled.
    if agent_type == "codex" {
        let hooks = agent_hook_installer::install_state(
            settings_path,
            &crate::agent_hook::codex_hook_map(),
            Some(crate::agent_hook::CODEX_HOOK_LABEL),
        );
        let flag = codex_config_toml(settings_path)
            .map(|c| crate::agent_hook_codex::features_hooks_present(&c))
            .unwrap_or(false);
        return match (hooks, flag) {
            (InstallState::Installed, true) => InstallState::ReviewRequired,
            (InstallState::NotInstalled, false) => InstallState::NotInstalled,
            _ => InstallState::Outdated,
        };
    }
    if agent_type == "opencode" {
        return crate::agent_hook_opencode::opencode_plugin_state(settings_path);
    }
    if agent_type == "pi" {
        return crate::agent_hook_pi::pi_extension_state(settings_path);
    }
    let map = match agent_type {
        "claude" | "gemini" => hook_map_for(agent_type),
        "grok" => Some(crate::agent_hook::grok_hook_map()),
        _ => None,
    };
    match map {
        Some(map) => agent_hook_installer::install_state(settings_path, &map, None),
        None => InstallState::NotInstalled,
    }
}

/// Persist `hook_instrumentation` for an agent into `AgentsConfig`. Off is the
/// default every reader falls back to, so it is stored as absent.
fn persist_flag(agent_type: &str, enabled: bool) -> Result<(), String> {
    let mut cfg = crate::config::load_agents_config();
    let base = cfg.clone();
    cfg.agents
        .entry(agent_type.to_string())
        .or_default()
        .hook_instrumentation = enabled.then_some(true);
    crate::config::save_agents_config(base, cfg)
}

/// Toggle hook instrumentation for an agent: persist the flag, then install (or
/// uninstall) the hooks in the agent's settings file. Effect applies on the next
/// agent launch.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn set_agent_hook_instrumentation(
    agent_type: String,
    enabled: bool,
) -> Result<(), String> {
    set_agent_hook_instrumentation_on(
        agent_type,
        enabled,
        !crate::agent_hook::hooks_supported_on_host(),
    )
    .await
}

async fn set_agent_hook_instrumentation_on(
    agent_type: String,
    enabled: bool,
    unsupported_host: bool,
) -> Result<(), String> {
    if unsupported_host {
        return Err("Agent hook installation is unsupported on this host (native Windows without the bundled tuic CLI)".into());
    }
    if crate::agent_test::active() {
        return Err("Global agent hook changes are disabled in agent-test".into());
    }
    let _codex_guard = if agent_type == "codex" {
        Some(crate::agent_hook_codex_trust::INSTALL_LOCK.lock().await)
    } else {
        None
    };
    persist_flag(&agent_type, enabled)?;
    if let Some(path) = hook_settings_path(&agent_type) {
        apply_at(&agent_type, &path, enabled)?;
        if agent_type == "codex" && enabled {
            // Installation succeeded even if this CLI needs normal /hooks review.
            // The following status read publishes reviewRequired, never installed.
            let _ = crate::agent_hook_codex_trust::verify(&path, true).await;
        }
    }
    Ok(())
}

/// Report install state for the agent's hooks: `installed` / `outdated` /
/// `notInstalled`, or `unsupported` for agents without a (wired) hook system.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn get_agent_hook_state(agent_type: String) -> String {
    get_agent_hook_state_on(agent_type, !crate::agent_hook::hooks_supported_on_host()).await
}

async fn get_agent_hook_state_on(agent_type: String, unsupported_host: bool) -> String {
    if unsupported_host {
        return "unsupported".into();
    }
    let _codex_guard = if agent_type == "codex" {
        Some(crate::agent_hook_codex_trust::INSTALL_LOCK.lock().await)
    } else {
        None
    };
    match hook_settings_path(&agent_type) {
        Some(path) => {
            let state = state_at(&agent_type, &path);
            if state == InstallState::ReviewRequired
                && crate::agent_hook_codex_trust::verify(&path, false)
                    .await
                    .is_ok()
            {
                InstallState::Installed.as_str().to_string()
            } else {
                state.as_str().to_string()
            }
        }
        None => "unsupported".to_string(),
    }
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn get_agent_native_status_signals(agent_type: String) -> bool {
    crate::agent_hook_launch::enabled(&agent_type)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn set_agent_native_status_signals(
    agent_type: String,
    enabled: bool,
) -> Result<(), String> {
    set_agent_native_status_signals_on(
        agent_type,
        enabled,
        !crate::agent_hook::hooks_supported_on_host(),
    )
}

fn set_agent_native_status_signals_on(
    agent_type: String,
    enabled: bool,
    unsupported_host: bool,
) -> Result<(), String> {
    if unsupported_host || !matches!(agent_type.as_str(), "claude" | "codex") {
        return Err(format!(
            "native status signals are unsupported for '{agent_type}'"
        ));
    }
    // On is the default every reader falls back to, so it is stored as absent.
    let mut config = crate::config::load_agents_config();
    let base = config.clone();
    config
        .agents
        .entry(agent_type)
        .or_default()
        .native_status_signals = (!enabled).then_some(false);
    crate::config::save_agents_config(base, config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // Catches: unsupported Windows installation persists a false preference or removes user hooks.
    #[test]
    fn windows_hook_toggle_preserves_preferences_and_files() {
        let dir = TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
        let _config = crate::config::set_config_dir_override(dir.path().to_path_buf());
        persist_flag("claude", true).unwrap();
        let before = std::fs::read(dir.path().join("agents.json")).unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        crate::agent_test::with_test_profile(dir.path(), || {
            for agent in ["claude", "codex", "gemini", "grok", "opencode", "pi"] {
                let settings = hook_settings_path(agent).unwrap();
                std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
                std::fs::write(&settings, b"user-owned").unwrap();
                for enabled in [true, false] {
                    let result = runtime.block_on(set_agent_hook_instrumentation_on(
                        agent.into(),
                        enabled,
                        true,
                    ));
                    assert!(result.unwrap_err().contains("unsupported"));
                    assert_eq!(
                        runtime.block_on(get_agent_hook_state_on(agent.into(), true)),
                        "unsupported"
                    );
                    assert_eq!(
                        std::fs::read(dir.path().join("agents.json")).unwrap(),
                        before
                    );
                    assert_eq!(std::fs::read(&settings).unwrap(), b"user-owned");
                    if matches!(agent, "claude" | "codex") {
                        assert!(
                            set_agent_native_status_signals_on(agent.into(), enabled, true)
                                .is_err()
                        );
                        assert_eq!(
                            std::fs::read(dir.path().join("agents.json")).unwrap(),
                            before
                        );
                    }
                }
            }
        });
    }

    #[test]
    fn agent_test_hook_toggle_refuses_before_persistence_and_shared_writes() {
        // Bug caught: a named instance still rewrites shared hooks and persists a lying flag.
        for agent in ["claude", "gemini", "grok", "codex", "opencode", "pi"] {
            for enabled in [true, false] {
                let dir = TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
                let config = dir.path().join("instance");
                std::fs::create_dir(&config).unwrap();
                let _config = crate::config::set_config_dir_override(config.clone());
                let settings = dir.path().join(agent).join("settings.json");
                std::fs::create_dir(settings.parent().unwrap()).unwrap();
                // Use the real installer to seed valid agent-specific content for removal.
                apply_at(agent, &settings, true).unwrap();
                let before = std::fs::read(&settings).unwrap();
                let codex_config = settings.with_file_name("config.toml");
                let codex_before = std::fs::read(&codex_config).ok();
                crate::agent_test::with_test_profile(dir.path(), || {
                    let result = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .unwrap()
                        .block_on(set_agent_hook_instrumentation(agent.into(), enabled));
                    assert_eq!(
                        std::fs::read_dir(&config).unwrap().count(),
                        0,
                        "{agent}/{enabled}: refusal must precede instance flag persistence"
                    );
                    assert_eq!(std::fs::read(&settings).unwrap(), before);
                    assert_eq!(std::fs::read(&codex_config).ok(), codex_before);
                    assert!(result.unwrap_err().contains(if cfg!(windows) {
                        "unsupported"
                    } else {
                        "agent-test"
                    }));
                });
            }
        }
    }

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
    fn pi_hook_toggle_dispatches_to_the_marked_extension_installer() {
        // Catches: Settings saves a Pi flag but never installs its extension.
        let dir = TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
        let path = dir.path().join("extensions/tuic.ts");
        assert_eq!(state_at("pi", &path), InstallState::NotInstalled);
        apply_at("pi", &path, true).unwrap();
        assert_eq!(state_at("pi", &path), InstallState::Installed);
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .starts_with("/* tuic-managed-pi */\n")
        );
        apply_at("pi", &path, false).unwrap();
        assert!(!path.exists());
        assert_eq!(state_at("pi", &path), InstallState::NotInstalled);
    }

    #[test]
    fn config_agent_hook_unsupported_agent_writes_nothing() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        apply_at("unknown-agent", &path, true).unwrap();
        assert!(
            !path.exists(),
            "unsupported agent must not write a settings file"
        );
        assert_eq!(state_at("unknown-agent", &path), InstallState::NotInstalled);
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

    /// Every reader resolves an absent flag to the same value as the default
    /// (`hook_instrumented_for`, `agent_hook_launch::enabled`), so writing the
    /// default back as `None` keeps one spelling per meaning. An explicit
    /// default would read as a user override in Settings forever.
    #[test]
    #[serial_test::serial]
    fn hook_flag_stores_the_default_as_absent() {
        let dir = TempDir::new().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let flag = || {
            crate::config::load_agents_config()
                .agents
                .get("gemini")
                .and_then(|s| s.hook_instrumentation)
        };

        persist_flag("gemini", true).unwrap();
        assert_eq!(flag(), Some(true));
        persist_flag("gemini", false).unwrap();
        assert_eq!(
            flag(),
            None,
            "off is the default, so it must be stored as absent"
        );
    }

    #[test]
    #[serial_test::serial]
    fn native_status_signals_store_the_default_as_absent() {
        let dir = TempDir::new().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let flag = || {
            crate::config::load_agents_config()
                .agents
                .get("claude")
                .and_then(|s| s.native_status_signals)
        };

        set_agent_native_status_signals_on("claude".to_string(), false, false).unwrap();
        assert_eq!(flag(), Some(false));
        assert!(!get_agent_native_status_signals("claude".to_string()));
        set_agent_native_status_signals_on("claude".to_string(), true, false).unwrap();
        assert_eq!(
            flag(),
            None,
            "on is the default, so it must be stored as absent"
        );
        assert_eq!(
            get_agent_native_status_signals("claude".to_string()),
            !cfg!(windows)
        );
    }

    #[test]
    fn config_agent_hook_flag_roundtrips() {
        let s = crate::config::AgentSettings {
            hook_instrumentation: Some(true),
            ..Default::default()
        };
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
        assert!(content.contains(r"7770;grok-hook="));
        assert!(content.contains("TUIC_SESSION"));
        assert!(content.contains("tuic-managed-hook"));
        let v: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert!(
            v["hooks"]["Stop"][0].get("matcher").is_none(),
            "lifecycle events must omit the matcher field (Grok rejects it)"
        );
        assert_eq!(
            v["hooks"]["Notification"][0]["matcher"],
            "permission_prompt"
        );
        for event in [
            "StopCancelled",
            "StopFailure",
            "PermissionDenied",
            "PostToolUse",
            "PostToolUseFailure",
        ] {
            assert!(v["hooks"][event].is_array(), "missing retraction: {event}");
        }
    }

    #[test]
    fn agent_hook_codex_install_merges_hooks_and_sets_flag_preserving_user_hook() {
        let dir = TempDir::new().unwrap();
        let hooks = dir.path().join("hooks.json");
        let cfg = dir.path().join("config.toml");
        std::fs::write(
            &hooks,
            br#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"user-codex.sh"}]}]}}"#,
        )
        .unwrap();
        std::fs::write(&cfg, b"[features]\nweb_search = true\n").unwrap();

        apply_at("codex", &hooks, true).unwrap();

        assert_eq!(state_at("codex", &hooks), InstallState::ReviewRequired);
        let h = std::fs::read_to_string(&hooks).unwrap();
        assert!(h.contains("user-codex.sh"), "user Codex hook preserved");
        assert!(h.contains(r"7770;state=busy"));
        assert!(crate::agent_hook_codex::features_hooks_present(&cfg));
        assert!(
            std::fs::read_to_string(&cfg)
                .unwrap()
                .contains("web_search = true"),
            "sibling feature flag preserved"
        );
    }

    #[test]
    fn agent_hook_codex_uninstall_clears_both_hooks_and_flag() {
        let dir = TempDir::new().unwrap();
        let hooks = dir.path().join("hooks.json");
        let cfg = dir.path().join("config.toml");
        std::fs::write(&cfg, b"[features]\n").unwrap();

        apply_at("codex", &hooks, true).unwrap();
        assert_eq!(state_at("codex", &hooks), InstallState::ReviewRequired);

        apply_at("codex", &hooks, false).unwrap();
        assert_eq!(
            state_at("codex", &hooks),
            InstallState::NotInstalled,
            "flag must clear too, else state stays Outdated forever"
        );
        assert!(!crate::agent_hook_codex::features_hooks_present(&cfg));
    }

    /// Every `(event, command, statusMessage)` of the TUIC-owned entries in a hooks file.
    fn codex_tuic_entries(hooks: &Path) -> Vec<(String, Option<String>)> {
        let root: serde_json::Value =
            serde_json::from_slice(&std::fs::read(hooks).unwrap()).unwrap();
        let mut out = Vec::new();
        for (event, groups) in root["hooks"].as_object().unwrap() {
            for group in groups.as_array().unwrap() {
                for hook in group["hooks"].as_array().unwrap() {
                    if hook["command"]
                        .as_str()
                        .unwrap()
                        .contains("tuic-managed-hook")
                    {
                        out.push((
                            event.clone(),
                            hook["statusMessage"].as_str().map(str::to_string),
                        ));
                    }
                }
            }
        }
        out
    }

    #[test]
    fn agent_hook_codex_entries_carry_the_label_codex_lists_hooks_by() {
        // Bug caught: Codex /hooks lists every TUIC entry as "Unnamed hook" (#2622-6809).
        let dir = TempDir::new().unwrap();
        let hooks = dir.path().join("hooks.json");
        std::fs::write(
            &hooks,
            br#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"user-stop.sh"}]}]}}"#,
        )
        .unwrap();
        apply_at("codex", &hooks, true).unwrap();
        let entries = codex_tuic_entries(&hooks);
        assert_eq!(entries.len(), crate::agent_hook::codex_hook_map().len());
        for (event, label) in &entries {
            assert_eq!(label.as_deref(), Some("TUICommander session"), "{event}");
        }
        let text = std::fs::read_to_string(&hooks).unwrap();
        assert_eq!(
            text.matches("statusMessage").count(),
            entries.len(),
            "user hook untouched"
        );
    }

    #[test]
    fn agent_hook_codex_legacy_install_is_outdated_and_upgrades_without_duplicates() {
        // Bug caught: an existing label-less install reads Installed forever, or
        // reinstalling appends a second set next to the old one.
        let dir = TempDir::new().unwrap();
        let hooks = dir.path().join("hooks.json");
        apply_at("codex", &hooks, true).unwrap();
        let mut root: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&hooks).unwrap()).unwrap();
        for groups in root["hooks"].as_object_mut().unwrap().values_mut() {
            for group in groups.as_array_mut().unwrap() {
                for hook in group["hooks"].as_array_mut().unwrap() {
                    hook.as_object_mut().unwrap().remove("statusMessage");
                }
            }
        }
        std::fs::write(&hooks, serde_json::to_vec(&root).unwrap()).unwrap();
        assert_eq!(state_at("codex", &hooks), InstallState::Outdated);

        apply_at("codex", &hooks, true).unwrap();
        let entries = codex_tuic_entries(&hooks);
        assert_eq!(entries.len(), crate::agent_hook::codex_hook_map().len());
        assert!(
            entries
                .iter()
                .all(|(_, l)| l.as_deref() == Some("TUICommander session"))
        );
        assert_eq!(state_at("codex", &hooks), InstallState::ReviewRequired);
    }

    #[test]
    fn agent_hook_claude_entries_get_no_label() {
        // Bug caught: the Codex label leaks into other agents' settings files.
        let dir = TempDir::new().unwrap();
        let settings = dir.path().join("settings.json");
        apply_at("claude", &settings, true).unwrap();
        assert!(
            !std::fs::read_to_string(&settings)
                .unwrap()
                .contains("statusMessage")
        );
    }
}
