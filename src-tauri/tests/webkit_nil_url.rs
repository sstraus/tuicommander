//! Reality probe for WebKit's nullable URL. No Tauri app or window is launched.
//! A standalone harness is required because libtest runs tests off the main thread.

#[cfg(target_os = "macos")]
const TEST_NAME: &str = "unloaded_webkit_url_unwrap_panics";

#[cfg(target_os = "macos")]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Nextest discovers custom harnesses through libtest's terse listing protocol.
    if args.iter().any(|arg| arg == "--list") {
        if !args.iter().any(|arg| arg == "--ignored") {
            println!("{TEST_NAME}: test");
        }
        return;
    }
    unloaded_webkit_url_unwrap_panics();
    println!("test {TEST_NAME} ... ok");
}

#[cfg(not(target_os = "macos"))]
fn main() {}

// Catches: an unwrap on the nil URL that WebKit returns for an unloaded webview.
#[cfg(target_os = "macos")]
fn unloaded_webkit_url_unwrap_panics() {
    use objc2::{
        MainThreadMarker, msg_send,
        rc::Retained,
        runtime::{AnyClass, AnyObject},
    };
    use objc2_foundation::{NSRect, NSURL};

    // Link WebKit even though this probe does not use the Tauri runtime.
    #[link(name = "WebKit", kind = "framework")]
    unsafe extern "C" {}

    let _main = MainThreadMarker::new().expect("WebKit probe must run on the process main thread");
    let class = AnyClass::get(c"WKWebView").expect("WebKit must provide WKWebView");
    let configuration_class = AnyClass::get(c"WKWebViewConfiguration")
        .expect("WebKit must provide WKWebViewConfiguration");
    let store_class =
        AnyClass::get(c"WKWebsiteDataStore").expect("WebKit must provide WKWebsiteDataStore");
    // SAFETY: this process is on its main thread; all selectors and native types
    // below are public WebKit APIs. The nonpersistent store avoids a user profile.
    let native: Option<Retained<NSURL>> = unsafe {
        let configuration: Retained<AnyObject> = msg_send![configuration_class, new];
        let store: Retained<AnyObject> = msg_send![store_class, nonPersistentDataStore];
        let _: () = msg_send![&*configuration, setWebsiteDataStore: &*store];
        let allocated: *mut AnyObject = msg_send![class, alloc];
        let initialized: *mut AnyObject =
            msg_send![allocated, initWithFrame: NSRect::ZERO, configuration: &*configuration];
        let view = Retained::from_raw(initialized).expect("WKWebView initialization failed");
        msg_send![&*view, URL]
    };
    assert!(
        native.is_none(),
        "fresh unloaded WKWebView unexpectedly has a URL"
    );
    // This standalone process owns its panic hook; capture and validate the
    // expected failure instead of printing a misleading raw panic to stderr.
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // The pre-fix access pattern from Wry 0.57.0 wkwebview/mod.rs:1369.
        native.as_ref().unwrap();
    }));
    std::panic::set_hook(hook);
    let panic = result.expect_err("Wry's pre-fix URL unwrap must panic on WebKit's nil");
    assert!(panic.downcast_ref::<&str>().is_some_and(|message| {
        message.contains("called `Option::unwrap()` on a `None` value")
    }));
    println!("WebKit returned nil; the pre-fix URL unwrap panicked as expected");
}
