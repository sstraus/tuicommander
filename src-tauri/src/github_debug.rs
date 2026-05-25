use std::sync::atomic::{AtomicBool, Ordering};

static GITHUB_API_DEBUG: AtomicBool = AtomicBool::new(false);

pub(crate) fn enabled() -> bool {
    GITHUB_API_DEBUG.load(Ordering::Relaxed)
}

pub(crate) fn set(enabled: bool) {
    GITHUB_API_DEBUG.store(enabled, Ordering::Relaxed);
    tracing::info!(source = "github", enabled, "API debug logging toggled");
}

pub(crate) fn log_api(method: &str, url: &str, caller: &str) {
    if GITHUB_API_DEBUG.load(Ordering::Relaxed) {
        tracing::info!(
            source = "github_api",
            method,
            url,
            caller,
            "GitHub API call"
        );
    }
}
