use crate::AppState;
use crate::mdkb_daemon::with_client;
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

/// mdkb reports symbol ranges 0-based; every TUIC surface that consumes them
/// (outline click, go-to-definition, references) addresses lines 1-based, the
/// way CodeMirror's `doc.line(n)` does. Convert here, at the one boundary where
/// the two conventions meet, so no caller has to remember which side it is on.
///
/// Note the asymmetry inside mdkb itself: `symbol_at_position` takes a *1-based*
/// `line` input, so the request path needs no conversion — only the response.
fn editor_line(mdkb_line: u32) -> u32 {
    mdkb_line + 1
}

impl From<crate::mdkb_client::MdkbSymbol> for OutlineSymbol {
    fn from(s: crate::mdkb_client::MdkbSymbol) -> Self {
        Self {
            name: s.name,
            kind: s.kind,
            file_path: s.file_path,
            line_start: editor_line(s.line_start),
            line_end: s.line_end.map(editor_line),
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
    let symbols = with_client(
        &state.mdkb_daemon,
        "mdkb_outline",
        |mut client| async move {
            let result = client.symbols_in_file(&repo_path, &file_path).await;
            (client, result)
        },
    )
    .await;

    Ok(symbols
        .unwrap_or_default()
        .into_iter()
        .map(OutlineSymbol::from)
        .collect())
}

#[tauri::command]
pub async fn mdkb_goto_definition(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
    file_path: String,
    line: u32,
    col: Option<u32>,
) -> Result<Option<DefinitionLocation>, String> {
    let symbol = with_client(
        &state.mdkb_daemon,
        "mdkb_goto_definition",
        |mut client| async move {
            let result = client
                .symbol_at_position(&repo_path, &file_path, line, col)
                .await;
            (client, result)
        },
    )
    .await
    .flatten();

    Ok(symbol.map(|sym| DefinitionLocation {
        file_path: sym.file_path,
        line: editor_line(sym.line_start),
    }))
}

/// A caller symbol is a jump target: the panel needs where it is and what it is
/// called, nothing else.
fn to_reference_locations(symbols: Vec<crate::mdkb_client::MdkbSymbol>) -> Vec<ReferenceLocation> {
    symbols
        .into_iter()
        .map(|s| ReferenceLocation {
            file_path: s.file_path,
            line: editor_line(s.line_start),
            name: s.name,
        })
        .collect()
}

#[tauri::command]
pub async fn mdkb_references(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
    symbol_name: String,
) -> Result<Vec<ReferenceLocation>, String> {
    let symbols = with_client(
        &state.mdkb_daemon,
        "mdkb_references",
        |mut client| async move {
            let result = client.code_graph(&repo_path, &symbol_name, "callers").await;
            (client, result)
        },
    )
    .await;

    Ok(to_reference_locations(symbols.unwrap_or_default()))
}

/// A `code_find` answer, with the count that says whether it is the whole one.
///
/// `symbols` alone cannot carry that: mdkb caps the rows, so a list of fifty is
/// fifty of fifty or fifty of three hundred and the rows look identical either
/// way. `total` is what mdkb matched before the cap, `None` when mdkb sent no
/// count — which a caller must render as "unknown", not as `symbols.length`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeFindResult {
    pub symbols: Vec<OutlineSymbol>,
    pub total: Option<u32>,
    pub capped: Option<bool>,
}

#[tauri::command]
pub async fn mdkb_code_find(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
    name: String,
    kind: Option<String>,
) -> Result<CodeFindResult, String> {
    let found = with_client(
        &state.mdkb_daemon,
        "mdkb_code_find",
        |mut client| async move {
            let result = client.code_find(&repo_path, &name, kind.as_deref()).await;
            (client, result)
        },
    )
    .await
    .unwrap_or_default();

    let capped = found.capped();
    Ok(CodeFindResult {
        symbols: found.symbols.into_iter().map(OutlineSymbol::from).collect(),
        total: found.total,
        capped,
    })
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
        // Need elevation. The staged file is copied as root, so it must sit
        // in a fresh private dir (random name, 0700), never a predictable
        // `temp_dir()/mdkb-install` another local user could pre-create and
        // swap the binary in. Dropping `staged` removes the dir on every path.
        let _ = std::fs::remove_file(&tmp_path);
        let staged = crate::private_scratch::stage_for_elevated_copy(
            &std::env::temp_dir(),
            "mdkb-install-",
            asset,
            &bytes,
        )
        .map_err(|e| format!("Failed to stage binary: {e}"))?;

        crate::tuic_cli::copy_with_elevation(
            &staged.path().to_string_lossy(),
            &install_path.to_string_lossy(),
        )?;
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

    // Re-initialize and connect now: version-aware startup will stop an older
    // detached daemon before this installation is reported as complete.
    let mut daemon = state.mdkb_daemon.lock().await;
    *daemon = crate::mdkb_daemon::MdkbDaemon::new();
    daemon
        .ensure_running()
        .await
        .map_err(|e| format!("Installed mdkb but failed to start its daemon: {e}"))?;

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

    fn symbol(name: &str, file: &str, line_start: u32) -> crate::mdkb_client::MdkbSymbol {
        crate::mdkb_client::MdkbSymbol {
            name: name.into(),
            kind: "Function".into(),
            file_path: file.into(),
            line_start,
            line_end: Some(line_start + 9),
            signature: Some(format!("fn {name}()")),
            scope_context: None,
        }
    }

    #[test]
    fn outline_symbol_from_mdkb_symbol() {
        let outline: OutlineSymbol = symbol("foo", "src/main.rs", 0).into();
        assert_eq!(outline.name, "foo");
        assert_eq!(outline.signature.as_deref(), Some("fn foo()"));
    }

    #[test]
    fn outline_symbol_shifts_mdkb_lines_into_editor_lines() {
        // mdkb's first line is 0; the editor's first line is 1. Without the
        // shift, every outline click and go-to-definition lands one line short
        // — and a symbol on the very first line clamps to the same place as one
        // on the second, so the error is invisible at the top of a file.
        let outline: OutlineSymbol = symbol("first", "src/main.rs", 0).into();
        assert_eq!(outline.line_start, 1, "mdkb line 0 is editor line 1");
        assert_eq!(outline.line_end, Some(10));

        let outline: OutlineSymbol = symbol("later", "src/main.rs", 41).into();
        assert_eq!(outline.line_start, 42);
    }

    #[test]
    fn editor_line_is_the_only_shift_applied() {
        // Pinned so the conversion cannot quietly become a no-op or a double
        // shift: the request side (`symbol_at_position`) already takes 1-based
        // input, so only responses move.
        assert_eq!(editor_line(0), 1);
        assert_eq!(editor_line(41), 42);
    }

    #[test]
    fn reference_locations_carry_the_caller_name_and_editor_line() {
        let refs = to_reference_locations(vec![
            symbol("bar", "src/lib.rs", 41),
            symbol("baz", "src/main.rs", 6),
        ]);

        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].file_path, "src/lib.rs");
        assert_eq!(refs[0].line, 42);
        assert_eq!(refs[1].name, "baz");
        assert_eq!(refs[1].line, 7);
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
