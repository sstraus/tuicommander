//! Monitors the Fn/Globe key on macOS via NSEvent local event monitor.
//!
//! Uses `addLocalMonitorForEventsMatchingMask:handler:` to observe `flagsChanged`
//! events within the app process. Only fires when the window is focused.
//! No accessibility permissions required.
//!
//! Keys that arrive as ordinary `keyDown` events but never reach the WebView
//! (Ctrl+Tab, F13-F20) are handled by `crate::native_keys` instead.

/// WebViews that mount a push-to-talk lifecycle and can own a native Fn take.
#[cfg(target_os = "macos")]
pub(crate) const DICTATION_WINDOWS: [&str; 3] = ["main", "secondary", "panel-ai-chat"];

/// Install the Fn key local event monitor. Emits "fn-key-down" and "fn-key-up"
/// Tauri events when the Fn/Globe modifier flag toggles. Key-down belongs to the
/// focused dictation-capable window; key-up also reaches a recording owner
/// that lost focus while Fn was held.
///
/// Must be called from the main thread (Tauri setup runs on main thread).
// DEFERRED (2026-09-06) — story #672-c1a3 asked for this monitor to start
// only when dictation is enabled and stop when disabled, instead of running
// for the whole app lifetime. `addLocalMonitorForEventsMatchingMask:` is
// AppKit main-thread-only; today `install()` runs once, at setup, on the main
// thread by construction. Toggling it dynamically from a settings change
// (which fires off the main thread) needs a real main-thread dispatch and
// hardware testing to confirm the monitor can be safely removed/re-added
// without racing an in-flight event callback — not something to guess at
// blind. Left as an always-on monitor.
#[cfg(target_os = "macos")]
pub fn install(app_handle: tauri::AppHandle) {
    use block2::RcBlock;
    use objc2_app_kit::{NSEvent, NSEventMask, NSEventModifierFlags};
    use std::ptr::NonNull;
    use std::sync::atomic::Ordering;
    #[cfg(feature = "desktop")]
    use tauri::{Emitter, Manager};

    let block = RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
        // SAFETY: event is a valid NSEvent pointer provided by AppKit for the
        // duration of the block invocation.
        let event_ref = unsafe { event.as_ref() };
        let flags = event_ref.modifierFlags();
        let fn_down = flags.contains(NSEventModifierFlags::Function);
        let dictation = app_handle.state::<super::DictationState>();
        let was_down = dictation.fn_down.swap(fn_down, Ordering::AcqRel);

        // Each WebView captures its own DOM target before recording starts.
        if fn_down && !was_down {
            tracing::info!(
                source = "dictation",
                key = "Fn",
                edge = "down",
                origin = "native-monitor",
                unix_ms = unix_ms(),
                "Push-to-talk key edge"
            );
            let target = DICTATION_WINDOWS
                .into_iter()
                .find(|label| {
                    app_handle
                        .get_webview_window(label)
                        .is_some_and(|window| window.is_focused().unwrap_or(false))
                })
                .unwrap_or("main");
            let _ = app_handle.emit_to(tauri::EventTarget::labeled(target), "fn-key-down", ());
        } else if !fn_down && was_down {
            tracing::info!(
                source = "dictation",
                key = "Fn",
                edge = "up",
                origin = "native-monitor",
                unix_ms = unix_ms(),
                "Push-to-talk key edge"
            );
            dictation.request_native_stop("fn-key-up");
            emit_fn_release(&app_handle);
        }

        // Return the event unchanged to let it propagate normally.
        event.as_ptr()
    });

    // SAFETY: we pass NSEventMask::FlagsChanged and a valid block.
    // The returned monitor is retained by AppKit for the lifetime of the app;
    // we don't store it since the monitor should live until app exit.
    let monitor = unsafe {
        NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::FlagsChanged, &block)
    };

    if monitor.is_some() {
        // Monitor is retained by AppKit — it will live until removeMonitor or app exit.
        // We intentionally leak it since we want it active for the entire app lifetime.
        tracing::info!(source = "dictation", "Fn key monitor installed");
    } else {
        tracing::warn!(source = "dictation", "Failed to install Fn key monitor");
    }
}

#[cfg(target_os = "macos")]
pub fn release_on_focus_loss(app_handle: &tauri::AppHandle) {
    use objc2_app_kit::{NSEvent, NSEventModifierFlags};
    use objc2_foundation::{NSRunLoop, NSRunLoopCommonModes};
    use std::sync::atomic::Ordering;
    use tauri::Manager;

    let dictation = app_handle.state::<super::DictationState>();
    if !dictation.fn_down.load(Ordering::Acquire) {
        return;
    }
    let physically_down = NSEvent::modifierFlags_class().contains(NSEventModifierFlags::Function);
    tracing::info!(
        source = "dictation",
        physically_down,
        recording = dictation.recording.load(Ordering::Acquire),
        unix_ms = unix_ms(),
        "Fn focus loss observed"
    );
    release_fn_capture(&dictation, physically_down, "focus-loss", || {
        emit_fn_release(app_handle);
    });
    if !dictation.fn_down.load(Ordering::Acquire)
        || dictation
            .fn_release_watch_active
            .swap(true, Ordering::AcqRel)
    {
        return;
    }

    // The local monitor cannot see key-up in another app. Read physical state
    // on the AppKit thread until release, without global-monitor permissions.
    let app = app_handle.clone();
    let timer = release_watch_timer(move || {
        let dictation = app.state::<super::DictationState>();
        if dictation.fn_down.load(Ordering::Acquire) {
            let physically_down =
                NSEvent::modifierFlags_class().contains(NSEventModifierFlags::Function);
            release_fn_capture(&dictation, physically_down, "fn-state-poll", || {
                emit_fn_release(&app);
            });
        }
        let held = dictation.fn_down.load(Ordering::Acquire);
        if !held {
            dictation
                .fn_release_watch_active
                .store(false, Ordering::Release);
        }
        held
    });
    // SAFETY: Tauri window events run on the AppKit thread. The run loop retains
    // the timer, including during menus/tracking, until it invalidates itself.
    unsafe { NSRunLoop::mainRunLoop().addTimer_forMode(&timer, NSRunLoopCommonModes) };
}

/// Only the window whose handler saw key-down can stop a recording. Deliver
/// release to all supported windows so a focus change cannot strand it.
#[cfg(target_os = "macos")]
fn emit_fn_release(app: &tauri::AppHandle) {
    use tauri::Emitter;
    for target in DICTATION_WINDOWS {
        let _ = app.emit_to(tauri::EventTarget::labeled(target), "fn-key-up", ());
    }
}

/// A temporary watch: no wakeups remain once the held key has been released.
#[cfg(target_os = "macos")]
fn release_watch_timer(
    observe_held: impl Fn() -> bool + Send + 'static,
) -> objc2::rc::Retained<objc2_foundation::NSTimer> {
    use block2::RcBlock;
    use objc2_foundation::NSTimer;
    use std::ptr::NonNull;

    let block = RcBlock::new(move |timer: NonNull<NSTimer>| {
        if !observe_held() {
            // SAFETY: Foundation supplies the live timer for this invocation.
            unsafe { timer.as_ref().invalidate() };
        }
    });
    // SAFETY: the block captures a Send closure and retains no borrowed state.
    unsafe { NSTimer::timerWithTimeInterval_repeats_block(0.05, true, &block) }
}

/// Reconcile the held Fn key with a native release observation.
#[cfg(target_os = "macos")]
pub(super) fn release_fn_capture(
    dictation: &super::DictationState,
    physically_down: bool,
    origin: &'static str,
    emit_release: impl FnOnce(),
) {
    use std::sync::atomic::Ordering;

    if physically_down {
        return;
    }
    if dictation.fn_down.swap(false, Ordering::AcqRel) {
        tracing::info!(
            source = "dictation",
            key = "Fn",
            edge = "up",
            origin,
            unix_ms = unix_ms(),
            "Push-to-talk key edge"
        );
        dictation.request_native_stop(origin);
        emit_release();
    }
}

#[cfg(target_os = "macos")]
fn unix_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

/// No-op on non-macOS platforms.
#[cfg(not(target_os = "macos"))]
pub fn install(_app_handle: tauri::AppHandle) {}

#[cfg(all(test, target_os = "macos"))]
#[path = "fn_key_monitor_tests.rs"]
mod tests;
