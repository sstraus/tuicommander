use crate::AppState;
use serde::Serialize;
use std::sync::Arc;
use tauri::State;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutlineSymbol {
    pub name: String,
    pub kind: String,
    pub file_path: String,
    pub line_start: u32,
    pub line_end: Option<u32>,
    pub signature: Option<String>,
    pub scope_context: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DefinitionLocation {
    pub file_path: String,
    pub line: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceLocation {
    pub file_path: String,
    pub line: u32,
    pub name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MdkbStatus {
    pub available: bool,
    pub connected: bool,
}

impl From<crate::mdkb_client::MdkbSymbol> for OutlineSymbol {
    fn from(s: crate::mdkb_client::MdkbSymbol) -> Self {
        Self {
            name: s.name,
            kind: s.kind,
            file_path: s.file_path,
            line_start: s.line_start,
            line_end: s.line_end,
            signature: s.signature,
            scope_context: s.scope_context,
        }
    }
}

#[tauri::command]
pub async fn mdkb_outline(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
    file_path: String,
) -> Result<Vec<OutlineSymbol>, String> {
    let mut daemon = state.mdkb_daemon.lock().await;
    let client = match daemon.ensure_running().await {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!("mdkb unavailable: {e}");
            return Ok(vec![]);
        }
    };
    match client.symbols_in_file(&repo_path, &file_path).await {
        Ok(symbols) => Ok(symbols.into_iter().map(OutlineSymbol::from).collect()),
        Err(e) => {
            tracing::warn!("mdkb_outline failed: {e}");
            Ok(vec![])
        }
    }
}

#[tauri::command]
pub async fn mdkb_goto_definition(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
    file_path: String,
    line: u32,
    col: Option<u32>,
) -> Result<Option<DefinitionLocation>, String> {
    let mut daemon = state.mdkb_daemon.lock().await;
    let client = match daemon.ensure_running().await {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!("mdkb unavailable: {e}");
            return Ok(None);
        }
    };
    match client.symbol_at_position(&repo_path, &file_path, line, col).await {
        Ok(Some(sym)) => Ok(Some(DefinitionLocation {
            file_path: sym.file_path,
            line: sym.line_start,
        })),
        Ok(None) => Ok(None),
        Err(e) => {
            tracing::warn!("mdkb_goto_definition failed: {e}");
            Ok(None)
        }
    }
}

#[tauri::command]
pub async fn mdkb_references(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
    symbol_name: String,
) -> Result<Vec<ReferenceLocation>, String> {
    let mut daemon = state.mdkb_daemon.lock().await;
    let client = match daemon.ensure_running().await {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!("mdkb unavailable: {e}");
            return Ok(vec![]);
        }
    };
    match client.code_graph(&repo_path, &symbol_name, "callers").await {
        Ok(value) => {
            let Some(arr) = value.as_array() else {
                tracing::warn!("mdkb_references: expected array, got {}", value);
                return Ok(vec![]);
            };
            let refs = arr
                .iter()
                .filter_map(|v| {
                    Some(ReferenceLocation {
                        file_path: v.get("file_path")?.as_str()?.to_string(),
                        line: v.get("line_start")?.as_u64()? as u32,
                        name: v.get("name")?.as_str()?.to_string(),
                    })
                })
                .collect();
            Ok(refs)
        }
        Err(e) => {
            tracing::warn!("mdkb_references failed: {e}");
            Ok(vec![])
        }
    }
}

#[tauri::command]
pub async fn mdkb_status(
    state: State<'_, Arc<AppState>>,
) -> Result<MdkbStatus, String> {
    let daemon = state.mdkb_daemon.lock().await;
    let available = daemon.is_available();
    let connected = daemon.is_connected();
    Ok(MdkbStatus { available, connected })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outline_symbol_from_mdkb_symbol() {
        let mdkb = crate::mdkb_client::MdkbSymbol {
            name: "foo".into(),
            kind: "Function".into(),
            file_path: "src/main.rs".into(),
            line_start: 1,
            line_end: Some(10),
            signature: Some("fn foo()".into()),
            scope_context: None,
        };
        let outline: OutlineSymbol = mdkb.into();
        assert_eq!(outline.name, "foo");
        assert_eq!(outline.line_start, 1);
        assert_eq!(outline.line_end, Some(10));
    }

    #[test]
    fn reference_location_parse_from_json() {
        let json = serde_json::json!([
            {"file_path": "src/lib.rs", "line_start": 42, "name": "bar"},
            {"file_path": "src/main.rs", "line_start": 7, "name": "baz"},
        ]);
        let refs: Vec<ReferenceLocation> = json
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| {
                Some(ReferenceLocation {
                    file_path: v.get("file_path")?.as_str()?.to_string(),
                    line: v.get("line_start")?.as_u64()? as u32,
                    name: v.get("name")?.as_str()?.to_string(),
                })
            })
            .collect();
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].file_path, "src/lib.rs");
        assert_eq!(refs[0].line, 42);
        assert_eq!(refs[1].name, "baz");
    }

    #[test]
    fn mdkb_status_serializes_camel_case() {
        let status = MdkbStatus {
            available: true,
            connected: false,
        };
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["available"], true);
        assert_eq!(json["connected"], false);
    }
}
