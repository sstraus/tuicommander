//! Native URL reads shared by desktop recovery and detached-window snapshots.

/// Read a URL without Wry's macOS `URL().unwrap()` / `absoluteString().unwrap()`.
/// A WebView without a committed document has no URL, not a Rust panic.
///
/// `with_webview` runs inline on the event-loop thread or dispatches there from
/// a worker. Only the owned Rust result leaves that callback. Like Tauri's URL
/// getter, this waits for the event loop when called from a worker.
pub(crate) fn read(window: &tauri::WebviewWindow) -> Result<Option<url::Url>, String> {
    #[cfg(target_os = "macos")]
    {
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        window
            .with_webview(move |view| {
                use objc2::{msg_send, rc::Retained, runtime::AnyObject};
                use objc2_foundation::NSURL;
                // SAFETY: Tauri supplies the live WKWebView and invokes this
                // callback on the main thread. URL is a nullable NSURL property.
                let native: Option<Retained<NSURL>> = unsafe {
                    let webview = &*view.inner().cast::<AnyObject>();
                    msg_send![webview, URL]
                };
                let _ = send.send(from_native(native.as_deref()));
            })
            .map_err(|error| error.to_string())?;
        receive.recv().map_err(|error| error.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    window.url().map(Some).map_err(|error| error.to_string())
}

#[cfg(target_os = "macos")]
fn from_native(native: Option<&objc2_foundation::NSURL>) -> Option<url::Url> {
    let text = native?.absoluteString()?;
    url::Url::parse(&text.to_string()).ok()
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use objc2_foundation::{NSString, NSURL};

    // Catches: an absent WKWebView URL reaches an unwrap instead of a skipped probe.
    #[test]
    fn nil_native_url_is_unavailable_without_panicking() {
        assert_eq!(from_native(None), None);
    }

    // Catches: the safe native adapter drops app routes or hides about: recovery URLs.
    #[test]
    fn native_urls_preserve_app_routes_and_lost_documents() {
        for text in [
            "tauri://localhost/index.html#/workspace",
            "http://localhost:1421/#/settings",
            "about:blank",
            "about:srcdoc",
        ] {
            let native = NSURL::URLWithString(&NSString::from_str(text)).unwrap();
            assert_eq!(from_native(Some(&native)).unwrap().as_str(), text);
        }
    }

    // Catches: an empty native absolute string is treated as a usable recovery target.
    #[test]
    fn empty_native_url_is_unavailable() {
        let native = NSURL::URLWithString(&NSString::from_str("")).unwrap();
        assert_eq!(from_native(Some(&native)), None);
    }
}
