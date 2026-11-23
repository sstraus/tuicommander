//! Legacy desktop transport adapters retain owner signatures and publish only after success.
use crate::{AppState, config};
use std::sync::Arc;

#[tauri::command]
pub(crate) fn save_notification_config(
    state: tauri::State<'_, Arc<AppState>>,
    base: config::NotificationConfig,
    config: config::NotificationConfig,
) -> Result<(), String> {
    save_notification_config_inner(&state, base, config)
}

fn save_notification_config_inner(
    state: &AppState,
    base: config::NotificationConfig,
    config: config::NotificationConfig,
) -> Result<(), String> {
    config::save_notification_config(base, config)?;
    super::api::publish_change(state, "notifications", true);
    Ok(())
}

#[tauri::command]
pub(crate) fn save_ui_prefs(
    state: tauri::State<'_, Arc<AppState>>,
    base: config::UIPrefsConfig,
    config: config::UIPrefsConfig,
) -> Result<(), String> {
    save_ui_prefs_inner(&state, base, config)
}

fn save_ui_prefs_inner(
    state: &AppState,
    base: config::UIPrefsConfig,
    config: config::UIPrefsConfig,
) -> Result<(), String> {
    if config::save_ui_prefs(base, config)? {
        super::api::publish_change(state, "preferences", true);
    }
    Ok(())
}

#[tauri::command]
pub(crate) fn save_repo_settings(
    state: tauri::State<'_, Arc<AppState>>,
    base: config::RepoSettingsMap,
    config: config::RepoSettingsMap,
) -> Result<(), String> {
    save_repo_settings_inner(&state, base, config)
}

fn save_repo_settings_inner(
    state: &AppState,
    base: config::RepoSettingsMap,
    config: config::RepoSettingsMap,
) -> Result<(), String> {
    config::save_repo_settings(base, config)?;
    super::api::publish_change(state, "repositories", true);
    Ok(())
}

#[tauri::command]
pub(crate) fn save_repo_defaults(
    state: tauri::State<'_, Arc<AppState>>,
    base: config::RepoDefaultsConfig,
    config: config::RepoDefaultsConfig,
) -> Result<(), String> {
    save_repo_defaults_inner(&state, base, config)
}

fn save_repo_defaults_inner(
    state: &AppState,
    base: config::RepoDefaultsConfig,
    config: config::RepoDefaultsConfig,
) -> Result<(), String> {
    config::save_repo_defaults(base, config)?;
    super::api::publish_change(state, "repo_defaults", true);
    Ok(())
}

#[tauri::command]
pub(crate) fn save_prompt_library(
    state: tauri::State<'_, Arc<AppState>>,
    base: config::PromptLibraryConfig,
    config: config::PromptLibraryConfig,
) -> Result<(), String> {
    save_prompt_library_inner(&state, base, config)
}

fn save_prompt_library_inner(
    state: &AppState,
    base: config::PromptLibraryConfig,
    config: config::PromptLibraryConfig,
) -> Result<(), String> {
    config::save_prompt_library(base, config)?;
    super::api::publish_change(state, "prompts", true);
    Ok(())
}

#[tauri::command]
pub(crate) fn save_agents_config(
    state: tauri::State<'_, Arc<AppState>>,
    base: config::AgentsConfig,
    config: config::AgentsConfig,
) -> Result<(), String> {
    save_agents_config_inner(&state, base, config)
}

fn save_agents_config_inner(
    state: &AppState,
    base: config::AgentsConfig,
    config: config::AgentsConfig,
) -> Result<(), String> {
    config::save_agents_config(base, config)?;
    super::api::publish_change(state, "agents", true);
    Ok(())
}

#[tauri::command]
pub(crate) fn save_keybindings(
    state: tauri::State<'_, Arc<AppState>>,
    base: serde_json::Value,
    config: serde_json::Value,
) -> Result<(), String> {
    save_keybindings_inner(&state, base, config)
}

fn save_keybindings_inner(
    state: &AppState,
    base: serde_json::Value,
    config: serde_json::Value,
) -> Result<(), String> {
    config::save_keybindings(base, config)?;
    super::api::publish_change(state, "keybindings", true);
    Ok(())
}

#[tauri::command]
pub(crate) fn save_pane_layout(
    state: tauri::State<'_, Arc<AppState>>,
    base: serde_json::Value,
    layout: serde_json::Value,
) -> Result<(), String> {
    save_pane_layout_inner(&state, base, layout)
}

fn save_pane_layout_inner(
    state: &AppState,
    base: serde_json::Value,
    layout: serde_json::Value,
) -> Result<(), String> {
    config::save_pane_layout(base, layout)?;
    super::api::publish_change(state, "pane_layout", true);
    Ok(())
}

#[cfg(test)]
mod refresh_review_tests {
    use super::*;

    // Catches: an individual legacy save adapter silently dropping its settings invalidation.
    #[test]
    fn every_legacy_save_wakes_its_settings_section() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = config::set_config_dir_override(dir.path().to_path_buf());
        let state = crate::state::tests_support::make_test_app_state();
        let mut events = state.event_bus.subscribe();
        macro_rules! save {
            ($save:ident, $load:ident, $section:literal) => {{
                let base = config::$load();
                let desired = super::super::refresh_test_support::edited($section, &base);
                let expected = serde_json::to_value(&desired).unwrap();
                $save(&state, base, desired).unwrap();
                assert_eq!(serde_json::to_value(config::$load()).unwrap(), expected);
                let event = events.try_recv().expect(concat!($section, " save did not publish"));
                assert!(matches!(event, crate::state::AppEvent::SettingsChanged(ref change) if change.section == $section && change.persisted && change.applied));
                assert!(events.try_recv().is_err(), "one save must publish once");
            }};
        }
        save!(
            save_notification_config_inner,
            load_notification_config,
            "notifications"
        );
        save!(save_ui_prefs_inner, load_ui_prefs, "preferences");
        save!(save_repo_settings_inner, load_repo_settings, "repositories");
        save!(
            save_repo_defaults_inner,
            load_repo_defaults,
            "repo_defaults"
        );
        save!(save_prompt_library_inner, load_prompt_library, "prompts");
        save!(save_agents_config_inner, load_agents_config, "agents");
        save!(save_keybindings_inner, load_keybindings, "keybindings");
        save!(save_pane_layout_inner, load_pane_layout, "pane_layout");
    }
    // Catches: failed persistence announcing a notification configuration that was not saved.
    #[test]
    fn failed_legacy_save_does_not_publish_settings_change() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = config::set_config_dir_override(dir.path().to_path_buf());
        let state = crate::state::tests_support::make_test_app_state();
        let base = config::load_notification_config();
        std::fs::create_dir(dir.path().join("notifications.json.lock")).unwrap();
        let mut events = state.event_bus.subscribe();
        let mut desired = base.clone();
        desired.enabled = !base.enabled;
        assert!(save_notification_config_inner(&state, base, desired).is_err());
        assert!(events.try_recv().is_err());
    }
}

#[cfg(test)]
mod ui_prefs_save_tests {
    use super::*;

    fn publishes(change: impl FnOnce(&mut config::UIPrefsConfig)) -> usize {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = config::set_config_dir_override(dir.path().to_path_buf());
        let state = crate::state::tests_support::make_test_app_state();
        let base = config::load_ui_prefs();
        let mut desired = base.clone();
        change(&mut desired);
        let mut events = state.event_bus.subscribe();
        save_ui_prefs_inner(&state, base, desired).unwrap();
        std::iter::from_fn(|| events.try_recv().ok()).count()
    }

    // Catches: echo loop where an identical save bumps the settings generation.
    #[test]
    fn unchanged_ui_prefs_save_publishes_no_change() {
        assert_eq!(publishes(|_| {}), 0);
    }

    // Catches: over-correction that hides real changes from other windows.
    #[test]
    fn changed_ui_prefs_save_publishes_once() {
        assert_eq!(publishes(|p| p.sidebar_visible = !p.sidebar_visible), 1);
    }
}
