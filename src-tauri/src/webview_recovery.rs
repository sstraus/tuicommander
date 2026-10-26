//! Putting the main WebView back on the app when its document disappears.
//!
//! On 2026-09-08 the Mac went to standby with the display off, twice, and
//! TUICommander came back showing a white window. The backend, the HTTP server
//! and all 20 PTY sessions were untouched. What had changed was the document in
//! the main frame:
//!
//! ```text
//! TOP {"href":"about:srcdoc","htmlLen":26,"bodyLen":0,"canvases":0,"appPresent":false}
//! ```
//!
//! 26 characters is `<html><body></body></html>`, the parse of an *empty*
//! srcdoc. The WebContent process had not crashed — the same pid served the
//! window before and after — so this is not the `about:blank` case the
//! `on_page_load` hook in `lib.rs` was written for. It happened during the
//! memory-pressure sweep macOS runs while asleep
//! (`JetsamEvent-2026-09-08-214808`: 285 MB free, a systemwide
//! `JETSAM_REASON_MEMORY_IDLE_EXIT` pass at 21:41), which suspends and resumes
//! the WebKit process. The app renders plugin panels and HTML previews in
//! `srcdoc` iframes, and one of those documents is what the main frame came back
//! holding.
//!
//! The recovery does not depend on knowing what WebKit did internally: put the
//! main frame back on the URL it was last healthy at.
//!
//! **`reload()` cannot do that.** There is no URL behind `about:srcdoc` to
//! reload — which is why `POST /debug/reload_webview` answered `{"ok":true}` and
//! left the window white for an hour. Only `navigate(boot_url)` works, and the
//! boot URL has to have been recorded *before* the frame was lost.
//!
//! The frontend heartbeat (`frontend_liveness`) does not cover this either. The
//! app is gone from the DOM rather than blocked, so it never beat once from the
//! blank document, and a frontend that never beat is deliberately not reported
//! as frozen — `tuic-remote` has no WebView at all. Absence of a heartbeat and
//! absence of the app are different faults; this module watches the second.

use std::time::Duration;

/// How often the main frame's URL is checked. The probe is a round trip through
/// the event loop, so it is kept well below the cost of the diagnostics tick;
/// fifteen seconds of white screen before self-healing is not worth polling
/// harder for.
const POLL_INTERVAL: Duration = Duration::from_secs(15);

/// Let the window boot before judging it. A webview that has not committed its
/// first document yet can legitimately read as `about:blank`.
const STARTUP_DELAY: Duration = Duration::from_secs(30);

/// True when the main frame is no longer showing the app.
///
/// Every `about:` URL qualifies: `about:blank` after a WebContent crash,
/// `about:srcdoc` after the standby incident above. Restricting this test to
/// the `about:` scheme prevents a healthy URL from being re-navigated in a loop.
/// The separate boot-URL guard refuses non-app URLs as recovery targets.
pub(crate) fn is_lost(url: &str) -> bool {
    url.starts_with("about:")
}

#[cfg(any(feature = "desktop", test))]
fn record_boot_url(
    slot: &parking_lot::RwLock<Option<url::Url>>,
    observed: &url::Url,
    dev_url: Option<&url::Url>,
) {
    let bundled = (observed.scheme() == "tauri" && observed.host_str() == Some("localhost"))
        || (cfg!(windows)
            && observed.scheme() == "http"
            && observed.host_str() == Some("tauri.localhost")
            && observed.port().is_none());
    let development =
        cfg!(debug_assertions) && dev_url.is_some_and(|dev| observed.origin() == dev.origin());
    if observed.username().is_empty() && observed.password().is_none() && (bundled || development) {
        *slot.write() = Some(observed.clone());
    }
}

#[cfg(any(feature = "desktop", test))]
fn navigate_home_with(
    target: Option<url::Url>,
    trigger: &'static str,
    reload: impl FnOnce() -> Result<(), String>,
    navigate: impl FnOnce(url::Url) -> Result<(), String>,
) -> serde_json::Value {
    let action = if target.is_some() {
        "navigate"
    } else {
        "reload"
    };
    let url = target.as_ref().map_or("<none>", url::Url::as_str);
    let result = match target.clone() {
        Some(target) => navigate(target),
        None => reload(),
    };
    tracing::info!(
        source = "webview",
        trigger,
        action,
        url,
        ok = result.is_ok(),
        "Native WebView navigation"
    );
    match (action, result) {
        ("navigate", Ok(())) => serde_json::json!({"ok": true, "action": action, "url": url}),
        ("reload", Ok(())) => serde_json::json!({
            "ok": true,
            "action": action,
            "warning": "no boot URL recorded yet — reload does not recover a blank document",
        }),
        (action, Err(e)) => serde_json::json!({"error": format!("{action} failed: {e}")}),
        _ => unreachable!(),
    }
}

#[cfg(feature = "desktop")]
mod desktop {
    use super::{POLL_INTERVAL, STARTUP_DELAY, is_lost, record_boot_url};
    use crate::state::AppState;
    use std::sync::Arc;
    use tauri::Manager;

    fn main_window(state: &Arc<AppState>) -> Option<tauri::WebviewWindow> {
        state.app_handle.read().as_ref()?.get_webview_window("main")
    }

    /// Send the main frame back to the last URL it was healthy at.
    ///
    /// Shared by the automatic poller, the `on_page_load` crash hook and
    /// `POST /debug/reload_webview`, because all three want the same thing and
    /// only one of them can be tested by hand.
    pub(crate) fn navigate_home(state: &Arc<AppState>, trigger: &'static str) -> serde_json::Value {
        let Some(window) = main_window(state) else {
            return serde_json::json!({"error": "main window not found"});
        };
        let target = state.webview_boot_url.read().clone();
        super::navigate_home_with(
            target,
            trigger,
            || window.reload().map_err(|e| e.to_string()),
            |url| window.navigate(url).map_err(|e| e.to_string()),
        )
    }

    /// One poll. Returns the lost document's URL, or `None` while the frame
    /// holds the app.
    ///
    /// Probes and nothing else: [`spawn`] does the logging and the recovery,
    /// because it is the only place that knows whether this is a new loss or
    /// the same one fifteen seconds later.
    ///
    /// The nil-safe URL reader dispatches to the event loop and waits for its
    /// reply from this worker. Keep the poll off the diagnostics thread: a wedged
    /// event loop must not take the CPU watchdog down with it. Hence the
    /// dedicated thread in [`spawn`].
    fn lost_document(state: &Arc<AppState>) -> Option<String> {
        let window = main_window(state)?; // No window yet — nothing to recover.
        let url = crate::webview_url::read(&window).ok()??; // An unavailable probe is not a loss.
        let lost = is_lost(url.as_str());
        record_boot_url(
            &state.webview_boot_url,
            &url,
            window.app_handle().config().build.dev_url.as_ref(),
        );
        if !lost {
            return None;
        }
        Some(url.to_string())
    }

    /// Ceiling on the retry schedule, in polls — four minutes at `POLL_INTERVAL`.
    /// Bounded rather than latched: a navigate can fail transiently, and a latch
    /// that stopped retrying would leave the window white for good.
    const MAX_RECOVERY_BACKOFF_POLLS: u32 = 16;

    pub(crate) fn spawn(state: Arc<AppState>) {
        std::thread::Builder::new()
            .name("webview-recovery".into())
            .spawn(move || {
                std::thread::sleep(STARTUP_DELAY);
                // The log lines are latched separately from the retries, and
                // they are the thing that must not repeat: an error and an info
                // every fifteen seconds alternate, so they defeat the ring
                // buffer's adjacent-entry coalescing and take two of its 1000
                // slots per poll. A window left white overnight evicted every
                // other diagnostic in the buffer — including whatever explained
                // the loss.
                let mut announced_loss = false;
                let mut polls_until_retry = 0u32;
                let mut backoff_polls = 1u32;
                loop {
                    std::thread::sleep(POLL_INTERVAL);
                    let Some(url) = lost_document(&state) else {
                        if announced_loss {
                            tracing::info!(
                                source = "webview",
                                "Main WebView is back on the app after a lost document"
                            );
                        }
                        announced_loss = false;
                        polls_until_retry = 0;
                        backoff_polls = 1;
                        continue;
                    };
                    if !announced_loss {
                        announced_loss = true;
                        tracing::error!(
                            source = "webview",
                            url = %url,
                            "Main WebView lost its document — the app is not in the DOM and the \
                             window is white. Navigating back to the app; PTY sessions are \
                             unaffected.",
                        );
                    }
                    if polls_until_retry > 0 {
                        polls_until_retry -= 1;
                        continue;
                    }
                    let outcome = navigate_home(&state, "recovery_thread");
                    tracing::debug!(
                        source = "webview",
                        outcome = %outcome,
                        "WebView recovery attempted"
                    );
                    polls_until_retry = backoff_polls;
                    backoff_polls = (backoff_polls * 2).min(MAX_RECOVERY_BACKOFF_POLLS);
                }
            })
            .expect("failed to spawn webview-recovery thread");
    }
}

#[cfg(feature = "desktop")]
pub(crate) use desktop::{navigate_home, spawn};

#[cfg(not(feature = "desktop"))]
pub(crate) fn navigate_home(
    _state: &std::sync::Arc<crate::state::AppState>,
    _trigger: &'static str,
) -> serde_json::Value {
    serde_json::json!({"error": "webview recovery requires the desktop feature"})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct LogSink(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

    impl std::io::Write for LogSink {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogSink {
        type Writer = LogSink;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    #[test]
    fn native_navigation_logs_action_target_and_trigger_for_both_branches() {
        let output = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let subscriber = tracing_subscriber::fmt()
            .with_writer(LogSink(output.clone()))
            .with_ansi(false)
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            let reload = navigate_home_with(
                None,
                "http_route",
                || Ok(()),
                |_| panic!("unexpected navigate"),
            );
            assert_eq!(reload["action"], "reload");
            let target = url::Url::parse("http://127.0.0.1:1421/").unwrap();
            let navigate = navigate_home_with(
                Some(target),
                "recovery_thread",
                || panic!("unexpected reload"),
                |_| Ok(()),
            );
            assert_eq!(navigate["url"], "http://127.0.0.1:1421/");
            let failed = navigate_home_with(
                None,
                "page_load_hook",
                || Err("window closed".into()),
                |_| panic!("unexpected navigate"),
            );
            assert_eq!(failed["error"], "reload failed: window closed");
        });
        let log = String::from_utf8(output.lock().unwrap().clone()).unwrap();
        assert!(
            log.contains("trigger=\"http_route\" action=\"reload\" url=\"<none>\""),
            "{log}"
        );
        assert!(
            log.contains(
                "trigger=\"recovery_thread\" action=\"navigate\" url=\"http://127.0.0.1:1421/\""
            ),
            "{log}"
        );
        assert!(
            log.contains("trigger=\"page_load_hook\" action=\"reload\" url=\"<none>\" ok=false"),
            "{log}"
        );
    }

    #[test]
    fn the_observed_blank_documents_are_lost() {
        // What the main frame actually held on 2026-09-08.
        assert!(is_lost("about:srcdoc"));
        // What a WebContent crash leaves behind, handled since before that.
        assert!(is_lost("about:blank"));
    }

    #[test]
    fn a_serving_app_is_not_lost() {
        // Dev, production, and an in-app route: none of these may ever be
        // re-navigated, or the poller would reload the app every 15 seconds.
        assert!(!is_lost("http://127.0.0.1:1421/"));
        assert!(!is_lost("tauri://localhost/"));
        assert!(!is_lost("http://127.0.0.1:1421/#/settings"));
        assert!(!is_lost("http://tauri.localhost/index.html"));
    }

    #[test]
    fn a_url_that_merely_mentions_about_is_not_lost() {
        // The check is on the scheme, not on the text: a route called "about"
        // is a page of the app, not a lost frame.
        assert!(!is_lost("http://127.0.0.1:1421/about"));
        assert!(!is_lost("tauri://localhost/#/about:srcdoc"));
    }

    #[test]
    fn recovery_keeps_the_last_app_url_when_other_documents_are_observed() {
        let dev = url::Url::parse("http://localhost:1421/").unwrap();
        let saved = url::Url::parse("http://localhost:1421/#/workspace").unwrap();
        let slot = parking_lot::RwLock::new(Some(saved.clone()));
        for candidate in [
            "about:config",
            "http://localhost.evil.com/",
            "http://127.0.0.2:1421/",
            "HTTP://LOCALHOST.EVIL.COM:1421/",
            "http://localhost:14319/",
            "asset://localhost/file.pdf",
            "tauri://localhost.evil.com/",
            "http://user@localhost:1421/",
        ] {
            record_boot_url(&slot, &url::Url::parse(candidate).unwrap(), Some(&dev));
            assert_eq!(*slot.read(), Some(saved.clone()), "{candidate}");
        }
    }

    #[test]
    fn recovery_remembers_routes_on_the_configured_app_origin() {
        let dev = url::Url::parse("http://localhost:1421/").unwrap();
        let slot = parking_lot::RwLock::new(None);
        let route = url::Url::parse("HTTP://LOCALHOST:1421/#/settings").unwrap();
        record_boot_url(&slot, &route, Some(&dev));
        assert_eq!(*slot.read(), Some(route));

        let bundled = url::Url::parse("tauri://localhost/index.html#/workspace").unwrap();
        record_boot_url(&slot, &bundled, None);
        assert_eq!(*slot.read(), Some(bundled));
    }
}
