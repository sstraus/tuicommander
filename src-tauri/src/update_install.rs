//! Native updater ownership: verified download, preflight, full error reporting.
use serde::Serialize;
use tauri::{Manager, ResourceId, Webview, ipc::Channel};
use tauri_plugin_updater::Update;

use crate::update_install_core::error_chain;

#[derive(Clone, Serialize)]
#[serde(tag = "event", content = "data")]
pub(crate) enum DownloadEvent {
    #[serde(rename_all = "camelCase")]
    Started {
        content_length: Option<u64>,
    },
    #[serde(rename_all = "camelCase")]
    Progress {
        chunk_length: usize,
    },
    Finished,
}

/// Host-only: the resource and progress channel belong to the calling webview.
#[tauri::command]
pub(crate) async fn install_app_update(
    webview: Webview,
    rid: ResourceId,
    on_event: Channel<DownloadEvent>,
) -> Result<(), String> {
    if crate::agent_test::active() {
        return Err("Updater is disabled in agent-test".into());
    }
    let update = webview
        .resources_table()
        .get::<Update>(rid)
        .map_err(|e| error_chain(&e))?;
    let mut first = true;
    let bytes = update
        .download(
            |chunk_length, content_length| {
                if first {
                    first = false;
                    let _ = on_event.send(DownloadEvent::Started { content_length });
                }
                let _ = on_event.send(DownloadEvent::Progress { chunk_length });
            },
            || {
                let _ = on_event.send(DownloadEvent::Finished);
            },
        )
        .await
        .map_err(|e| error_chain(&e))?;

    // Archive scanning and installation are blocking filesystem operations.
    tauri::async_runtime::spawn_blocking(move || {
        #[cfg(target_os = "macos")]
        {
            let executable = tauri::utils::platform::current_exe().map_err(|e| error_chain(&e))?;
            let app = tauri_plugin_updater::extract_path_from_executable(&executable)
                .map_err(|e| error_chain(&e))?;
            let root = crate::update_install_core::extraction_root(&app, &std::env::temp_dir())
                .map_err(|e| error_chain(&e))?;
            crate::update_install_core::archive_space_check(&bytes, &root)?;
        }
        // The plugin retains its signature/version verification and native installer.
        update.install(bytes).map_err(|e| error_chain(&e))
    })
    .await
    .map_err(|e| format!("Update installer task failed: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri_plugin_updater::Error;

    // Catches: the plugin's transparent Io variant formatted with Display only,
    // which keeps the unpack context but drops the nested OS cause.
    #[test]
    fn plugin_io_error_reports_unpack_context_and_os_cause() {
        #[derive(Debug)]
        struct Context(std::io::Error);
        impl std::fmt::Display for Context {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("failed to unpack test executable")
            }
        }
        impl std::error::Error for Context {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&self.0)
            }
        }
        let error = Error::Io(std::io::Error::other(Context(
            std::io::Error::from_raw_os_error(13),
        )));
        let message = error_chain(&error);
        assert!(
            message.contains("failed to unpack test executable"),
            "{message}"
        );
        assert!(message.contains("os error 13"), "{message}");
    }
}
