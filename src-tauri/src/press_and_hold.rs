//! Disables the macOS "press-and-hold" accent popup for our app.
//!
//! When a key is held over a focused editable element in WKWebView, macOS
//! shows the diacritic picker (é, ë, ê…) instead of repeating the key. TUIC
//! routes terminal keystrokes through a hidden `<input>`, so holding `j`/`l`
//! in vim triggers the picker instead of cursor movement (issue #79).
//!
//! Native terminals (iTerm2, Terminal.app) suppress this by registering
//! `ApplePressAndHoldEnabled = NO` in their own user-defaults registration
//! domain. We do the same: it is scoped to our app domain only and does NOT
//! touch `NSGlobalDomain`, so a user who explicitly set `-g true` still wins.

/// Register `ApplePressAndHoldEnabled = NO` in our registration domain.
///
/// Must run on the main thread (Tauri setup does) and before the WKWebView
/// text input context first reads the flag.
#[cfg(target_os = "macos")]
pub fn disable() {
    use objc2::runtime::AnyObject;
    use objc2_foundation::{NSDictionary, NSNumber, NSString, NSUserDefaults};

    let key = NSString::from_str("ApplePressAndHoldEnabled");
    let value = NSNumber::new_bool(false);
    // registerDefaults wants NSDictionary<NSString, AnyObject>; upcast the NSNumber
    // value to &AnyObject so from_slices infers the AnyObject value type.
    let value_obj: &AnyObject = &value;
    let dict = NSDictionary::from_slices(&[&*key], &[value_obj]);

    let defaults = NSUserDefaults::standardUserDefaults();
    // SAFETY: registerDefaults is a thread-safe AppKit call; `dict` is a valid
    // retained NSDictionary for the call duration.
    unsafe { defaults.registerDefaults(&dict) };

    tracing::info!(
        source = "press-and-hold",
        "ApplePressAndHoldEnabled=NO registered for app domain"
    );
}

/// No-op on non-macOS platforms (the accent popup is macOS-only).
#[cfg(not(target_os = "macos"))]
pub fn disable() {}
