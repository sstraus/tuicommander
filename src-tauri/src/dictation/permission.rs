//! Microphone permission checks for macOS TCC.
//!
//! On macOS, microphone access requires explicit user consent via the TCC
//! (Transparency, Consent, and Control) framework. After the user denies
//! access, the system will NOT re-prompt — the user must manually enable it
//! in System Settings. This module detects that state and provides a way
//! to open the correct settings pane.

/// Microphone permission status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicPermission {
    /// Never asked — system will prompt on first microphone access.
    #[cfg_attr(not(target_os = "macos"), expect(dead_code))]
    NotDetermined,
    /// User granted access.
    Authorized,
    /// User denied access — must re-enable via System Settings.
    #[cfg_attr(not(target_os = "macos"), expect(dead_code))]
    Denied,
    /// Restricted by MDM or parental controls.
    #[cfg_attr(not(target_os = "macos"), expect(dead_code))]
    Restricted,
}

impl MicPermission {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotDetermined => "not_determined",
            Self::Authorized => "authorized",
            Self::Denied => "denied",
            Self::Restricted => "restricted",
        }
    }
}

/// Check the current microphone authorization status.
#[cfg(target_os = "macos")]
pub fn check() -> MicPermission {
    use objc2_foundation::NSString;

    // AVCaptureDevice is in AVFoundation. The class is available at runtime
    // because cpal links AVFoundation for audio capture.
    let cls: Option<&objc2::runtime::AnyClass> =
        objc2::runtime::AnyClass::get(c"AVCaptureDevice");
    let Some(cls) = cls else {
        // AVCaptureDevice class not found — shouldn't happen on macOS 10.14+
        return MicPermission::NotDetermined;
    };

    // AVMediaTypeAudio = @"soun" (FourCC constant in AVFoundation)
    let media_type = NSString::from_str("soun");

    // +[AVCaptureDevice authorizationStatusForMediaType:] -> NSInteger
    // Returns: 0=notDetermined, 1=restricted, 2=denied, 3=authorized
    let status: isize =
        unsafe { objc2::msg_send![cls, authorizationStatusForMediaType: &*media_type] };

    match status {
        0 => MicPermission::NotDetermined,
        1 => MicPermission::Restricted,
        2 => MicPermission::Denied,
        3 => MicPermission::Authorized,
        _ => MicPermission::NotDetermined,
    }
}

/// Non-macOS fallback: always authorized (no TCC on Linux/Windows).
#[cfg(not(target_os = "macos"))]
pub fn check() -> MicPermission {
    MicPermission::Authorized
}

/// Explicitly request microphone access, triggering the macOS TCC prompt.
///
/// CoreAudio (used by cpal) does NOT trigger the TCC prompt on its own —
/// only AVCaptureDevice.requestAccessForMediaType: does. This must be called
/// when the status is `NotDetermined` before starting audio capture.
///
/// Blocks the current thread until the user responds to the prompt.
/// Returns `true` if access was granted.
#[cfg(target_os = "macos")]
pub fn request() -> bool {
    use objc2_foundation::NSString;

    let cls: Option<&objc2::runtime::AnyClass> =
        objc2::runtime::AnyClass::get(c"AVCaptureDevice");
    let Some(cls) = cls else {
        return false;
    };

    let media_type = NSString::from_str("soun");

    let (tx, rx) = std::sync::mpsc::channel();
    let handler = block2::RcBlock::new(move |granted: objc2::runtime::Bool| {
        let _ = tx.send(granted.as_bool());
    });

    // +[AVCaptureDevice requestAccessForMediaType:completionHandler:]
    let () = unsafe {
        objc2::msg_send![
            cls,
            requestAccessForMediaType: &*media_type,
            completionHandler: &*handler
        ]
    };

    // Block until the user responds (or timeout after 60s)
    rx.recv_timeout(std::time::Duration::from_secs(60)).unwrap_or(false)
}

/// Non-macOS fallback: always granted.
#[cfg(not(target_os = "macos"))]
pub fn request() -> bool {
    true
}

/// Open the macOS System Settings pane for microphone privacy.
#[cfg(target_os = "macos")]
pub fn open_settings() {
    let _ = std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
        .spawn();
}

/// Non-macOS fallback: no-op.
#[cfg(not(target_os = "macos"))]
pub fn open_settings() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_as_str_values() {
        assert_eq!(MicPermission::NotDetermined.as_str(), "not_determined");
        assert_eq!(MicPermission::Authorized.as_str(), "authorized");
        assert_eq!(MicPermission::Denied.as_str(), "denied");
        assert_eq!(MicPermission::Restricted.as_str(), "restricted");
    }

    #[test]
    fn check_returns_valid_status() {
        let status = check();
        // On any platform, check() should return a valid variant
        let valid = matches!(
            status,
            MicPermission::NotDetermined
                | MicPermission::Authorized
                | MicPermission::Denied
                | MicPermission::Restricted
        );
        assert!(valid, "check() returned unexpected status: {:?}", status);
    }
}
