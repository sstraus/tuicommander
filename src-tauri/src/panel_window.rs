use std::collections::HashMap;

use tauri::{Emitter, Manager};

fn validate_panel_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 64
        || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err(format!("Invalid panel_id: {id}"));
    }
    Ok(())
}

#[tauri::command]
pub async fn open_panel_window(
    app: tauri::AppHandle,
    panel_id: String,
    title: String,
    params: HashMap<String, String>,
    width: Option<f64>,
    height: Option<f64>,
) -> Result<(), String> {
    validate_panel_id(&panel_id)?;
    let label = format!("panel-{panel_id}");
    if let Some(existing) = app.get_webview_window(&label) {
        existing.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }
    let mut query = format!("mode=panel&panel={panel_id}");
    if !params.is_empty() {
        let mut enc = url::form_urlencoded::Serializer::new(String::new());
        for (k, v) in &params {
            enc.append_pair(k, v);
        }
        let encoded = enc.finish();
        if !encoded.is_empty() {
            query.push('&');
            query.push_str(&encoded);
        }
    }
    let url = tauri::WebviewUrl::App(format!("/?{query}").into());
    let window = tauri::WebviewWindowBuilder::new(&app, &label, url)
        .title(&title)
        .inner_size(width.unwrap_or(500.0), height.unwrap_or(600.0))
        .min_inner_size(300.0, 300.0)
        .build()
        .map_err(|e| format!("Failed to create panel window: {e}"))?;

    let app_handle = app.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Destroyed = event
            && let Err(e) = app_handle.emit("panel-window-closed", &panel_id)
        {
            tracing::warn!("Failed to emit panel-window-closed for {panel_id}: {e}");
        }
    });
    Ok(())
}

#[tauri::command]
pub async fn focus_panel_window(app: tauri::AppHandle, panel_id: String) -> Result<(), String> {
    validate_panel_id(&panel_id)?;
    let label = format!("panel-{panel_id}");
    if let Some(w) = app.get_webview_window(&label) {
        w.set_focus().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn close_panel_window(app: tauri::AppHandle, panel_id: String) -> Result<(), String> {
    validate_panel_id(&panel_id)?;
    let label = format!("panel-{panel_id}");
    if let Some(w) = app.get_webview_window(&label) {
        w.destroy().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn focus_main_window(app: tauri::AppHandle) -> Result<(), String> {
    let w = app
        .get_webview_window("main")
        .ok_or_else(|| "Main window not found".to_string())?;
    super::ensure_window_visible(&w);
    w.unminimize().map_err(|e| e.to_string())?;
    w.set_focus().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_panel_id_rejects_invalid() {
        assert!(validate_panel_id("").is_err(), "empty string");
        assert!(
            validate_panel_id(&"a".repeat(65)).is_err(),
            ">64 chars"
        );
        assert!(validate_panel_id("has spaces").is_err(), "spaces");
        assert!(validate_panel_id("has/slash").is_err(), "slash");
        assert!(validate_panel_id("has.dot").is_err(), "dot");
        assert!(validate_panel_id("under_score").is_err(), "underscore");
        assert!(validate_panel_id("semi;colon").is_err(), "semicolon");
        assert!(
            validate_panel_id("<script>alert(1)</script>").is_err(),
            "html injection"
        );
    }

    #[test]
    fn test_validate_panel_id_accepts_valid() {
        assert!(validate_panel_id("ai-chat").is_ok());
        assert!(validate_panel_id("activity").is_ok());
        assert!(validate_panel_id("my-panel-123").is_ok());
        assert!(validate_panel_id("a").is_ok(), "single char");
        assert!(
            validate_panel_id(&"a".repeat(64)).is_ok(),
            "exactly 64 chars"
        );
        assert!(validate_panel_id("ABC-123-def").is_ok(), "mixed case");
    }
}
