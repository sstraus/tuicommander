//! Global OS-level hotkey for toggling TUICommander window visibility.
//!
//! Uses `tauri-plugin-global-shortcut` (Carbon RegisterEventHotKey on macOS,
//! Win32 RegisterHotKey on Windows, X11 XGrabKey on Linux).
//! No Accessibility permission required on macOS.

use anyhow::Result;
use std::str::FromStr;
use std::sync::Arc;
use tauri::Manager;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::state::AppState;

/// Initialize the global-shortcut plugin with the toggle handler.
///
/// Must be called inside the Tauri `setup()` closure (not at builder level).
pub fn init(app: &tauri::AppHandle) -> Result<()> {
    let plugin = tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app_handle, shortcut, event| {
            if event.state == ShortcutState::Pressed {
                let _ = handle_toggle(app_handle, shortcut);
            }
        })
        .build();
    app.plugin(plugin)?;
    Ok(())
}

/// Register the saved hotkey from config (if any) on app startup.
pub fn restore_from_config(app: &tauri::AppHandle) {
    let state = app.state::<Arc<AppState>>();
    let combo = state.config.read().global_hotkey.clone();
    if let Some(ref combo) = combo {
        if let Err(e) = register(app, combo) {
            tracing::warn!(
                source = "global-hotkey",
                "Failed to restore global hotkey '{}': {}",
                combo,
                e
            );
        } else {
            tracing::info!(source = "global-hotkey", combo = %combo, "Restored global hotkey");
        }
    }
}

/// Register a global shortcut combo string (e.g. "CommandOrControl+Shift+T").
pub fn register(app: &tauri::AppHandle, combo: &str) -> Result<()> {
    let shortcut = Shortcut::from_str(combo)?;
    app.global_shortcut().register(shortcut)?;
    Ok(())
}

/// Unregister a global shortcut combo string.
pub fn unregister(app: &tauri::AppHandle, combo: &str) -> Result<()> {
    let shortcut = Shortcut::from_str(combo)?;
    app.global_shortcut().unregister(shortcut)?;
    Ok(())
}

/// Toggle the main window:
/// - Not visible / minimized → unminimize + show + focus
/// - Visible but not focused → focus
/// - Visible and focused → hide (instant, no dock animation)
fn handle_toggle(app: &tauri::AppHandle, _shortcut: &Shortcut) -> Result<()> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| anyhow::anyhow!("main window not found"))?;

    let visible = window.is_visible().unwrap_or(false);
    let focused = window.is_focused().unwrap_or(false);
    let minimized = window.is_minimized().unwrap_or(false);

    if !visible || minimized {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    } else if !focused {
        let _ = window.set_focus();
    } else {
        let _ = window.hide();
    }

    Ok(())
}

// -- Tauri commands ----------------------------------------------------------

/// Set (or clear) the global hotkey. Unregisters old, registers new, persists.
#[tauri::command]
pub async fn set_global_hotkey(
    app: tauri::AppHandle,
    combo: Option<String>,
) -> std::result::Result<(), String> {
    let state = app.state::<Arc<AppState>>();

    // Unregister current hotkey (if any)
    let current = state.config.read().global_hotkey.clone();
    if let Some(ref old) = current {
        let _ = unregister(&app, old);
    }

    // Register new hotkey (if provided)
    if let Some(ref new_combo) = combo {
        register(&app, new_combo).map_err(|e| e.to_string())?;
    }

    // Persist to config
    {
        let mut config = state.config.read().clone();
        config.global_hotkey = combo;
        crate::config::save_app_config(config.clone()).map_err(|e| e.to_string())?;
        *state.config.write() = config;
    }

    Ok(())
}

/// Get the currently configured global hotkey combo.
#[tauri::command]
pub fn get_global_hotkey(app: tauri::AppHandle) -> Option<String> {
    let state = app.state::<Arc<AppState>>();
    state.config.read().global_hotkey.clone()
}
