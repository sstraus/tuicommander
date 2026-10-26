//! Native window adapter. Calls stay on the desktop event loop.
#[cfg(target_os = "macos")]
use crate::AppState;
#[cfg(target_os = "macos")]
use std::sync::Arc;
use tauri::Manager;

pub(super) struct PanelRestore {
    url: url::Url,
    title: String,
    position: tauri::LogicalPosition<f64>,
    size: tauri::LogicalSize<f64>,
}

pub(crate) struct DesktopWindows<'a>(pub &'a tauri::AppHandle);
impl DesktopWindows<'_> {
    fn window(&self, label: &str) -> Result<tauri::WebviewWindow, String> {
        self.0
            .get_webview_window(label)
            .ok_or_else(|| format!("Window missing: {label}"))
    }
}
impl super::super::hidden::Windows for DesktopWindows<'_> {
    fn ensure_main(&self) -> Result<(), String> {
        if self.0.get_webview_window("main").is_some() {
            return Ok(());
        }
        let config = self
            .0
            .config()
            .app
            .windows
            .iter()
            .find(|w| w.label == "main")
            .ok_or("Main window configuration missing")?;
        let builder =
            tauri::WebviewWindowBuilder::from_config(self.0, config).map_err(|e| e.to_string())?;
        crate::themes::window_background::configure(
            builder.visible(false),
            crate::themes::window_background::current(self.0),
        )
        .build()
        .map_err(|e| e.to_string())?;
        Ok(())
    }
    fn all_labels(&self) -> Vec<String> {
        self.0.webview_windows().into_keys().collect()
    }
    fn destroy(&self, label: &str) -> Result<(), String> {
        let window = self.window(label)?;
        if label == "main" {
            use tauri_plugin_window_state::{AppHandleExt, StateFlags};
            self.0
                .save_window_state(
                    StateFlags::POSITION
                        | StateFlags::MAXIMIZED
                        | StateFlags::DECORATIONS
                        | StateFlags::FULLSCREEN,
                )
                .map_err(|e| e.to_string())?;
        }
        if label != "main" {
            let scale = window.scale_factor().map_err(|e| e.to_string())?;
            let saved = PanelRestore {
                url: crate::webview_url::read(&window)?.ok_or("WebView has no committed URL")?,
                title: window.title().map_err(|e| e.to_string())?,
                position: window
                    .outer_position()
                    .map_err(|e| e.to_string())?
                    .to_logical(scale),
                size: window
                    .inner_size()
                    .map_err(|e| e.to_string())?
                    .to_logical(scale),
            };
            self.0
                .state::<super::BackgroundMode>()
                .panels
                .lock()
                .insert(label.into(), saved);
        }
        window.destroy().map_err(|e| e.to_string())
    }
    fn visible_labels(&self) -> Vec<String> {
        self.0
            .webview_windows()
            .into_iter()
            .filter(|(_, window)| window.is_visible().unwrap_or(false))
            .map(|(label, _)| label)
            .collect()
    }
    fn hide(&self, label: &str) -> Result<(), String> {
        self.window(label)?.hide().map_err(|e| e.to_string())
    }
    fn show(&self, label: &str) -> Result<(), String> {
        if self.0.get_webview_window(label).is_none() {
            let mode = self.0.state::<super::BackgroundMode>();
            let panels = mode.panels.lock();
            let saved = panels
                .get(label)
                .ok_or_else(|| format!("Window missing: {label}"))?;
            tauri::WebviewWindowBuilder::new(
                self.0,
                label,
                tauri::WebviewUrl::External(saved.url.clone()),
            )
            .title(&saved.title)
            .position(saved.position.x, saved.position.y)
            .inner_size(saved.size.width, saved.size.height)
            .build()
            .map_err(|e| e.to_string())?;
        }
        self.window(label)?.show().map_err(|e| e.to_string())
    }
    fn unminimize(&self, label: &str) -> Result<(), String> {
        self.window(label)?.unminimize().map_err(|e| e.to_string())
    }
    fn focus(&self, label: &str) -> Result<(), String> {
        self.window(label)?.set_focus().map_err(|e| e.to_string())
    }
    fn tray_available(&self) -> bool {
        super::super::tray::desktop::available(self.0)
    }
    fn minimize(&self, label: &str) -> Result<(), String> {
        self.window(label)?.minimize().map_err(|e| e.to_string())
    }
    fn tray_status(&self, text: &str, attention: bool) {
        super::super::tray::desktop::render(self.0, text, attention);
    }
    fn background_resources(&self, hidden: bool) {
        #[cfg(target_os = "macos")]
        {
            let state = self.0.state::<Arc<AppState>>();
            if !hidden || state.config.read().background.hide_dock_icon {
                let policy = if hidden {
                    tauri::ActivationPolicy::Accessory
                } else {
                    tauri::ActivationPolicy::Regular
                };
                if let Err(error) = self.0.set_activation_policy(policy) {
                    tracing::warn!(%error, "Cannot change Dock visibility");
                }
            }
        }
        #[cfg(windows)]
        for window in self.0.webview_windows().into_values() {
            if let Err(error) = window.with_webview(move |view| {
                use webview2_com::Microsoft::Web::WebView2::Win32::{
                    COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW,
                    COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL, ICoreWebView2_19,
                };
                use windows_core::Interface;
                // Older WebView2 runtimes do not implement this optional interface.
                let result = unsafe { view.controller().CoreWebView2() }
                    .and_then(|webview| webview.cast::<ICoreWebView2_19>())
                    .and_then(|webview| unsafe {
                        webview.SetMemoryUsageTargetLevel(if hidden {
                            COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW
                        } else {
                            COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL
                        })
                    });
                if let Err(error) = result {
                    tracing::debug!(%error, "WebView memory hint unavailable");
                }
            }) {
                tracing::warn!(%error, "Cannot schedule WebView memory hint");
            }
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        let _ = hidden;
    }
}
