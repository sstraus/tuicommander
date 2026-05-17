use crate::AppState;
use serde::Serialize;
use std::path::PathBuf;
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
    pub binary_path: Option<String>,
    pub version: Option<String>,
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
    match client
        .symbol_at_position(&repo_path, &file_path, line, col)
        .await
    {
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
pub async fn mdkb_code_find(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
    name: String,
    kind: Option<String>,
) -> Result<Vec<OutlineSymbol>, String> {
    let mut daemon = state.mdkb_daemon.lock().await;
    let client = match daemon.ensure_running().await {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!("mdkb unavailable: {e}");
            return Ok(vec![]);
        }
    };
    match client.code_find(&repo_path, &name, kind.as_deref()).await {
        Ok(symbols) => Ok(symbols.into_iter().map(OutlineSymbol::from).collect()),
        Err(e) => {
            tracing::warn!("mdkb_code_find failed: {e}");
            Ok(vec![])
        }
    }
}

#[tauri::command]
pub async fn mdkb_status(state: State<'_, Arc<AppState>>) -> Result<MdkbStatus, String> {
    let daemon = state.mdkb_daemon.lock().await;
    let available = daemon.is_available();
    let connected = daemon.is_connected();
    let binary_path = daemon.binary_path().map(|p| p.display().to_string());
    let version = daemon.version();
    Ok(MdkbStatus {
        available,
        connected,
        binary_path,
        version,
    })
}

fn mdkb_install_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        PathBuf::from("/usr/local/bin/mdkb")
    }
    #[cfg(target_os = "linux")]
    {
        PathBuf::from("/usr/local/bin/mdkb")
    }
    #[cfg(target_os = "windows")]
    {
        let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
        PathBuf::from(format!("{local}\\Microsoft\\WindowsApps\\mdkb.exe"))
    }
}

fn mdkb_asset_name() -> &'static str {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "mdkb-macos-arm64"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "mdkb-macos-x64"
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "mdkb-linux-x64"
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        "mdkb-linux-arm64"
    }
    #[cfg(target_os = "windows")]
    {
        "mdkb-windows-x64.exe"
    }
}

#[tauri::command]
pub async fn install_mdkb(state: State<'_, Arc<AppState>>) -> Result<String, String> {
    let asset = mdkb_asset_name();
    let url = format!("https://github.com/sstraus/mdkb/releases/latest/download/{asset}");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Download failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Download failed: HTTP {}", resp.status()));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("Failed to read response: {e}"))?;

    let install_path = mdkb_install_path();
    let tmp_path = install_path.with_extension("tmp");

    if let Some(parent) = install_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    // Try direct write first
    let direct_ok = std::fs::write(&tmp_path, &bytes).is_ok();

    if direct_ok {
        std::fs::rename(&tmp_path, &install_path)
            .map_err(|e| format!("Failed to move binary: {e}"))?;
    } else {
        // Need elevation
        let _ = std::fs::remove_file(&tmp_path);
        let tmp_dir = std::env::temp_dir().join("mdkb-install");
        let _ = std::fs::create_dir_all(&tmp_dir);
        let staged = tmp_dir.join(asset);
        std::fs::write(&staged, &bytes).map_err(|e| format!("Failed to stage binary: {e}"))?;

        crate::tuic_cli::copy_with_elevation(
            &staged.to_string_lossy(),
            &install_path.to_string_lossy(),
        )?;

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&install_path, std::fs::Permissions::from_mode(0o755)).map_err(
            |e| {
                format!(
                    "Installed but failed to set executable bit: {e}. Try: chmod +x {}",
                    install_path.display()
                )
            },
        )?;
    }

    tracing::info!(source = "mdkb", path = %install_path.display(), "mdkb installed");

    // Re-initialize daemon so it picks up the new binary
    let mut daemon = state.mdkb_daemon.lock().await;
    *daemon = crate::mdkb_daemon::MdkbDaemon::new();

    Ok(install_path.display().to_string())
}

#[tauri::command]
pub async fn uninstall_mdkb(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let daemon = state.mdkb_daemon.lock().await;
    let actual_path = daemon
        .binary_path()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| "mdkb is not installed".to_string())?;
    drop(daemon);

    // Refuse to uninstall if managed by a package manager
    let path_str = actual_path.to_string_lossy();
    if path_str.contains("/homebrew/")
        || path_str.contains("/Cellar/")
        || path_str.contains("/linuxbrew/")
    {
        return Err(
            "mdkb appears to be installed via Homebrew. Use `brew uninstall mdkb` instead."
                .to_string(),
        );
    }
    if path_str.contains("/.cargo/") {
        return Err(
            "mdkb appears to be installed via cargo. Use `cargo uninstall mdkb` instead."
                .to_string(),
        );
    }

    if std::fs::remove_file(&actual_path).is_err() {
        crate::tuic_cli::remove_with_elevation(&path_str)?;
    }

    tracing::info!(source = "mdkb", path = %actual_path.display(), "mdkb uninstalled");

    let mut daemon = state.mdkb_daemon.lock().await;
    *daemon = crate::mdkb_daemon::MdkbDaemon::new();

    Ok(())
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
            binary_path: Some("/usr/local/bin/mdkb".into()),
            version: Some("3.1.0".into()),
        };
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["available"], true);
        assert_eq!(json["connected"], false);
        assert_eq!(json["binaryPath"], "/usr/local/bin/mdkb");
        assert_eq!(json["version"], "3.1.0");
    }
}
