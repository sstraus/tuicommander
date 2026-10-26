//! Desktop verification policy, enabled only by the dedicated binary entry point.
//! Saved configuration must never re-enable machine-wide integrations here.
use std::sync::OnceLock;

static PORT: OnceLock<u16> = OnceLock::new();

pub(crate) fn active() -> bool {
    #[cfg(test)]
    if test_root().is_some() {
        return true;
    }
    PORT.get().is_some()
}

pub(crate) fn port() -> Option<u16> {
    PORT.get().copied()
}

#[cfg(any(feature = "desktop", test))]
fn validate(instance: &str, port: &str) -> Result<u16, String> {
    crate::app_instance::AppInstance::named(instance)?;
    let port = port.parse::<u16>().map_err(|_| "Invalid agent-test port")?;
    if port < 1024 || port == 9876 {
        return Err("agent-test requires a non-production port in 1024..=65535".into());
    }
    Ok(port)
}

#[cfg(feature = "desktop")]
pub(crate) fn enable() -> Result<(), String> {
    if !cfg!(target_os = "macos") || !cfg!(debug_assertions) {
        return Err("agent-test requires a macOS debug build".into());
    }
    let instance = std::env::var("TUIC_APP_INSTANCE").unwrap_or_default();
    let port = std::env::var("TUIC_PORT").unwrap_or_default();
    if cfg!(feature = "dictation")
        && (std::env::var("TUIC_AGENT_TEST_SPEECH_OUTPUT").as_deref() != Ok("1")
            || !instance.starts_with("agent-test-"))
    {
        return Err("dictation-enabled agent-test requires explicit speech-output opt-in and an agent-test-<name> instance; microphone stays disabled".into());
    }
    PORT.set(validate(&instance, &port)?)
        .map_err(|_| "agent-test policy already selected".into())
}

#[cfg(any(feature = "desktop", test))]
pub(crate) fn apply_config(config: &mut crate::config::AppConfig, port: u16) {
    config.services.server.enabled = true;
    config.services.server.port = port;
    config.services.server.ipv6_enabled = false;
    config.services.relay.enabled = false;
    config.global_hotkey = None;
    config.auto_update_enabled = false;
    config.auto_update_plugins_enabled = false;
    config.prevent_sleep_when_busy = false;
}

/// Tao's launch calls `activateIgnoringOtherApps(false)`, which macOS still grants
/// when the launching app (the daily TUIC) is frontmost (story 2095). Tauri runs
/// `setup` after that call, so the policy is set between build and run.
#[cfg(feature = "desktop")]
pub(crate) fn without_launch_activation(app: tauri::App) -> tauri::App {
    #[cfg(target_os = "macos")]
    let app = {
        let mut app = app;
        if active() {
            app.set_activation_policy(tauri::ActivationPolicy::Prohibited);
        }
        app
    };
    app
}

/// Ready runs on the main thread, after Tao's initial application activation.
/// The test window starts hidden so Tao's visible-window activation sweep skips it.
#[cfg(all(feature = "desktop", target_os = "macos"))]
pub(crate) fn present_without_focus(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    if let Some(monitor) = window.primary_monitor()? {
        let area = monitor.work_area();
        let size = window.outer_size()?;
        let margin = (16.0 * monitor.scale_factor()) as i32;
        window.set_position(tauri::PhysicalPosition::new(
            area.position.x + (area.size.width as i32 - size.width as i32 - margin).max(0),
            area.position.y + (area.size.height as i32 - size.height as i32 - margin).max(0),
        ))?;
    }
    // Always-on-top does not stop a higher or another floating window (a second
    // agent-test, a system prompt) from fully covering the corner. WebKit then
    // reports the page hidden and stops rAF (story 2095). Visibility here must
    // follow window visibility only.
    window.with_webview(|view| {
        use objc2::{msg_send, runtime::AnyObject, sel};
        // SAFETY: with_webview runs on AppKit's main thread with this live
        // window's WKWebView; the selector is checked before it is sent.
        unsafe {
            let webview = &*view.inner().cast::<AnyObject>();
            let supported: bool =
                msg_send![webview, respondsToSelector: sel!(_setWindowOcclusionDetectionEnabled:)];
            if supported {
                let _: () = msg_send![webview, _setWindowOcclusionDetectionEnabled: false];
            } else {
                tracing::warn!(
                    "WebKit cannot ignore occlusion; a covered agent-test page turns hidden"
                );
            }
        }
    })?;
    let pointer = window.ns_window()?;
    // SAFETY: Tauri returns this live window's NSWindow; Ready is on AppKit's
    // main thread and the borrowed native window cannot outlive the Tauri window.
    unsafe {
        let native = &*pointer.cast::<objc2_app_kit::NSWindow>();
        native.orderFront(None);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_test_rejects_default_config_and_production_port() {
        // Bug caught: a direct binary launch silently uses production state.
        for (instance, port) in [
            ("", "19876"),
            ("default", "19876"),
            ("../escape", "19876"),
            ("agent-test", "9876"),
            ("agent-test", "0"),
            ("agent-test", "65536"),
        ] {
            assert!(validate(instance, port).is_err(), "{instance:?}:{port}");
        }
        assert_eq!(validate("agent-test", "19876"), Ok(19876));
    }

    #[test]
    fn agent_test_overrides_saved_machine_wide_services() {
        // Bug caught: a previously edited instance enables relay/update/hotkeys on boot.
        let mut config = crate::config::AppConfig::default();
        config.services.relay.enabled = true;
        config.global_hotkey = Some("CommandOrControl+Shift+T".into());
        config.auto_update_enabled = true;
        config.auto_update_plugins_enabled = true;
        config.prevent_sleep_when_busy = true;
        apply_config(&mut config, 19876);
        assert!(config.services.server.enabled);
        assert_eq!(config.services.server.port, 19876);
        assert!(!config.services.server.ipv6_enabled);
        assert!(!config.services.relay.enabled);
        assert_eq!(config.global_hotkey, None);
        assert!(!config.auto_update_enabled);
        assert!(!config.auto_update_plugins_enabled);
        assert!(!config.prevent_sleep_when_busy);
    }
}

// Test policy and path routing are thread-local: parallel tests never inherit them.
#[cfg(test)]
thread_local! {
    static TEST_ROOT: std::cell::RefCell<Option<std::path::PathBuf>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub(crate) fn test_root() -> Option<std::path::PathBuf> {
    TEST_ROOT.with(|root| root.borrow().clone())
}

#[cfg(test)]
pub(crate) fn with_test_profile(root: &std::path::Path, test: impl FnOnce()) {
    struct Restore(Option<std::path::PathBuf>);
    impl Drop for Restore {
        fn drop(&mut self) {
            TEST_ROOT.with(|root| *root.borrow_mut() = self.0.take());
        }
    }
    let _restore = Restore(TEST_ROOT.with(|slot| slot.replace(Some(root.to_path_buf()))));
    test();
}
