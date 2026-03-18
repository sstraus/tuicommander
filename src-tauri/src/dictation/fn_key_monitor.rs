//! Monitors the Fn/Globe key on macOS via NSEvent local event monitor.
//!
//! Uses `addLocalMonitorForEventsMatchingMask:handler:` to observe `flagsChanged`
//! events within the app process. Only fires when the window is focused.
//! No accessibility permissions required.

/// Install the Fn key local event monitor. Emits "fn-key-down" and "fn-key-up"
/// Tauri events (scoped to the main window) when the Fn/Globe modifier flag toggles.
///
/// Must be called from the main thread (Tauri setup runs on main thread).
#[cfg(target_os = "macos")]
pub fn install(app_handle: tauri::AppHandle) {
    use block2::RcBlock;
    use objc2_app_kit::{NSEvent, NSEventMask, NSEventModifierFlags};
    use std::ptr::NonNull;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tauri::Emitter;

    let fn_was_down = AtomicBool::new(false);

    let block = RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
        // SAFETY: event is a valid NSEvent pointer provided by AppKit for the
        // duration of the block invocation.
        let event_ref = unsafe { event.as_ref() };
        let flags = event_ref.modifierFlags();
        let fn_down = flags.contains(NSEventModifierFlags::Function);
        let was_down = fn_was_down.swap(fn_down, Ordering::Relaxed);

        // Scope events to main window only — prevents plugin iframes from
        // observing dictation timing.
        if fn_down && !was_down {
            let _ = app_handle.emit_to(tauri::EventTarget::labeled("main"), "fn-key-down", ());
        } else if !fn_down && was_down {
            let _ = app_handle.emit_to(tauri::EventTarget::labeled("main"), "fn-key-up", ());
        }

        // Return the event unchanged to let it propagate normally.
        event.as_ptr()
    });

    // SAFETY: we pass NSEventMask::FlagsChanged and a valid block.
    // The returned monitor is retained by AppKit for the lifetime of the app;
    // we don't store it since the monitor should live until app exit.
    let monitor = unsafe {
        NSEvent::addLocalMonitorForEventsMatchingMask_handler(
            NSEventMask::FlagsChanged,
            &block,
        )
    };

    if monitor.is_some() {
        // Monitor is retained by AppKit — it will live until removeMonitor or app exit.
        // We intentionally leak it since we want it active for the entire app lifetime.
        tracing::info!(source = "dictation", "Fn key monitor installed");
    } else {
        tracing::warn!(source = "dictation", "Failed to install Fn key monitor");
    }
}

/// No-op on non-macOS platforms.
#[cfg(not(target_os = "macos"))]
pub fn install(_app_handle: tauri::AppHandle) {}
