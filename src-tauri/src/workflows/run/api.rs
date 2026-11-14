#[cfg(feature = "desktop")]
use tauri::Emitter;

pub(super) fn emit_run_cursor(
    state: &crate::state::AppState,
    repo_path: &str,
    run_id: &str,
    sequence: i64,
) {
    let payload = serde_json::json!({ "runId": run_id, "sequence": sequence });
    #[cfg(feature = "desktop")]
    if let Some(app) = state.app_handle.read().as_ref() {
        let _ = app.emit(
            "workflow-run-changed",
            serde_json::json!({
                "repo_path": repo_path, "payload": &payload,
            }),
        );
    }
    let _ = state
        .event_bus
        .send(crate::state::AppEvent::WorkflowRunChanged {
            repo_path: repo_path.to_owned(),
            payload,
        });
}
