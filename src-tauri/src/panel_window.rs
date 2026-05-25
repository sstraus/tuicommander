use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[cfg(feature = "desktop")]
use tauri::{Emitter, Manager};

const PANEL_GEOMETRY_FILE: &str = "panel-geometry.json";

#[derive(Clone, Default, Serialize, Deserialize)]
struct PanelGeometry {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

type PanelGeometryMap = HashMap<String, PanelGeometry>;

fn load_geometry() -> PanelGeometryMap {
    crate::config::load_json_config(PANEL_GEOMETRY_FILE)
}

fn save_geometry(map: &PanelGeometryMap) {
    let _ = crate::config::save_json_config(PANEL_GEOMETRY_FILE, map);
}

fn validate_panel_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 64 || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
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

    let saved = load_geometry();
    let geo = saved.get(&panel_id);

    let w = geo.map_or_else(|| width.unwrap_or(500.0), |g| f64::from(g.width));
    let h = geo.map_or_else(|| height.unwrap_or(600.0), |g| f64::from(g.height));

    let url = tauri::WebviewUrl::App(format!("/?{query}").into());
    let mut builder = tauri::WebviewWindowBuilder::new(&app, &label, url)
        .title(&title)
        .inner_size(w, h)
        .min_inner_size(300.0, 300.0);

    if let Some(g) = geo {
        builder = builder.position(f64::from(g.x), f64::from(g.y));
    }

    let window = builder
        .build()
        .map_err(|e| format!("Failed to create panel window: {e}"))?;

    let panel_id_close = panel_id.clone();
    let app_handle = app.clone();
    window.on_window_event(move |event| match event {
        tauri::WindowEvent::CloseRequested { .. } | tauri::WindowEvent::Destroyed => {
            if let tauri::WindowEvent::CloseRequested { .. } = event
                && let Some(w) = app_handle.get_webview_window(&format!("panel-{panel_id_close}"))
            {
                save_window_geometry(&panel_id_close, &w);
            }
            if let tauri::WindowEvent::Destroyed = event
                && let Err(e) = app_handle.emit("panel-window-closed", &panel_id_close)
            {
                tracing::warn!(
                    "Failed to emit panel-window-closed for {}: {e}",
                    panel_id_close
                );
            }
        }
        _ => {}
    });
    Ok(())
}

fn save_window_geometry(panel_id: &str, window: &tauri::WebviewWindow) {
    let Ok(pos) = window.outer_position() else {
        return;
    };
    let Ok(size) = window.inner_size() else {
        return;
    };
    if size.width < 100 || size.height < 100 {
        return;
    }
    let mut map = load_geometry();
    map.insert(
        panel_id.to_string(),
        PanelGeometry {
            x: pos.x,
            y: pos.y,
            width: size.width,
            height: size.height,
        },
    );
    save_geometry(&map);
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
        save_window_geometry(&panel_id, &w);
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
        assert!(validate_panel_id(&"a".repeat(65)).is_err(), ">64 chars");
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
