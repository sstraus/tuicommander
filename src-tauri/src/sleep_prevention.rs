use std::sync::Mutex;

/// Holds an optional KeepAwake guard. While the guard exists, the system
/// will not go to sleep due to idle timeout. Dropping the guard releases
/// the lock and allows normal power management.
pub struct SleepBlocker(pub(crate) Mutex<Option<keepawake::KeepAwake>>);

impl SleepBlocker {
    pub(crate) fn new() -> Self {
        Self(Mutex::new(None))
    }
}

/// Acquire a system sleep lock. No-op if already held.
#[tauri::command]
pub(crate) fn block_sleep(blocker: tauri::State<'_, SleepBlocker>) -> Result<(), String> {
    let mut guard = blocker.0.lock().map_err(|e| e.to_string())?;
    if guard.is_none() {
        let ka = keepawake::Builder::default()
            .display(false)
            .idle(true)
            .sleep(true)
            .reason("Agent session active")
            .app_name("TUICommander")
            .app_reverse_domain("com.tuicommander.app")
            .create()
            .map_err(|e| format!("Failed to block sleep: {e}"))?;
        *guard = Some(ka);
    }
    Ok(())
}

/// Release the system sleep lock. No-op if not held.
#[tauri::command]
pub(crate) fn unblock_sleep(blocker: tauri::State<'_, SleepBlocker>) -> Result<(), String> {
    let mut guard = blocker.0.lock().map_err(|e| e.to_string())?;
    *guard = None;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocker_starts_empty() {
        let blocker = SleepBlocker::new();
        let guard = blocker.0.lock().unwrap();
        assert!(guard.is_none());
    }
}
