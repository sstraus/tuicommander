//! Desktop-only text clipboard. Browser clients use their own pasteboard.

use std::sync::Arc;

use parking_lot::Mutex;

/// Keep the clipboard alive between calls: on Linux the last owner hosts the
/// copied text. Serialize operations as required by the Windows clipboard.
#[derive(Clone, Default)]
pub struct NativeClipboard(Arc<Mutex<Option<arboard::Clipboard>>>);

impl NativeClipboard {
    /// Tauri does not drop managed state on exit; arboard requires cleanup.
    pub fn clear(&self) {
        self.0.lock().take();
    }

    async fn run<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&mut arboard::Clipboard) -> Result<T, arboard::Error> + Send + 'static,
    ) -> Result<T, String> {
        let clipboard = self.0.clone();
        tokio::task::spawn_blocking(move || {
            let mut clipboard = clipboard.lock();
            if clipboard.is_none() {
                *clipboard = Some(arboard::Clipboard::new().map_err(|e| e.to_string())?);
            }
            operation(clipboard.as_mut().expect("clipboard initialized")).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| format!("Clipboard worker failed: {e}"))?
    }
}

#[tauri::command]
pub async fn write_clipboard_text(
    clipboard: tauri::State<'_, NativeClipboard>,
    text: String,
) -> Result<(), String> {
    clipboard
        .run(move |clipboard| clipboard.set_text(text))
        .await
}

#[tauri::command]
pub async fn read_clipboard_text(
    clipboard: tauri::State<'_, NativeClipboard>,
) -> Result<String, String> {
    clipboard.run(|clipboard| clipboard.get_text()).await
}
