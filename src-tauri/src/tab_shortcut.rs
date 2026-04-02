//! Intercepts Ctrl+Tab / Ctrl+Shift+Tab at the Cocoa event level on macOS.
//!
//! WKWebView (and AppKit) swallow Ctrl+Tab before it reaches the JS keydown
//! handler. This module installs an `NSEvent` local monitor that catches the
//! keypress first and emits a Tauri event so the frontend can switch tabs.

/// macOS virtual key code for the Tab key.
#[cfg(target_os = "macos")]
const KVK_TAB: u16 = 0x30;

/// Install the Ctrl+Tab local event monitor.
///
/// Emits `"ctrl-tab"` with payload `"next"` or `"prev"` (when Shift is held).
/// Must be called from the main thread (Tauri setup runs on main thread).
#[cfg(target_os = "macos")]
pub(crate) fn install(app_handle: tauri::AppHandle) {
    use block2::RcBlock;
    use objc2_app_kit::{NSEvent, NSEventMask, NSEventModifierFlags};
    use std::ptr::{self, NonNull};
    use tauri::Emitter;

    let block = RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
        // SAFETY: event is a valid NSEvent pointer provided by AppKit for the
        // duration of the block invocation.
        let event_ref = unsafe { event.as_ref() };
        let key_code = event_ref.keyCode();

        if key_code == KVK_TAB {
            let flags = event_ref.modifierFlags();
            let ctrl = flags.contains(NSEventModifierFlags::Control);

            if ctrl {
                let direction = if flags.contains(NSEventModifierFlags::Shift) {
                    "prev"
                } else {
                    "next"
                };

                let _ = app_handle.emit("ctrl-tab", direction);

                // Swallow the event so it doesn't reach WKWebView / AppKit tab cycling.
                return ptr::null_mut();
            }
        }

        // Not our event — pass it through unchanged.
        event.as_ptr()
    });

    // SAFETY: NSEventMask::KeyDown and a valid block. The returned monitor is
    // retained by AppKit for the app lifetime; we intentionally leak it.
    let monitor = unsafe {
        NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &block)
    };

    if monitor.is_some() {
        tracing::info!(source = "tab-shortcut", "Ctrl+Tab monitor installed");
    } else {
        tracing::warn!(source = "tab-shortcut", "Failed to install Ctrl+Tab monitor");
    }
}

/// No-op on non-macOS platforms (Ctrl+Tab reaches JS fine on Win/Linux).
#[cfg(not(target_os = "macos"))]
pub fn install(_app_handle: tauri::AppHandle) {}
