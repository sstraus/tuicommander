//! Tauri commands for managing the `tuic` CLI binary installation.
//!
//! The CLI binary is embedded as a sidecar. These commands handle:
//! - Checking if the CLI is installed in PATH
//! - Installing the CLI (copy sidecar to /usr/local/bin/tuic)
//! - Auto-updating the installed CLI on app startup
//! - Tracking whether the first-run prompt has been dismissed

use serde::Serialize;
#[cfg(feature = "desktop")]
use tauri::Manager;

#[derive(Serialize)]
pub(crate) struct CliStatus {
    installed: bool,
    path: Option<String>,
    version_match: bool,
    prompt_dismissed: bool,
}

/// Check CLI installation status.
#[tauri::command]
pub(crate) fn get_cli_status(app: tauri::AppHandle) -> CliStatus {
    let prompt_dismissed = crate::config::config_dir()
        .join(".cli-prompt-dismissed")
        .exists();

    let install_path = resolve_install_path();

    if !std::path::Path::new(&install_path).exists() {
        return CliStatus {
            installed: false,
            path: None,
            version_match: false,
            prompt_dismissed,
        };
    }

    // Check if installed version matches current sidecar
    let version_match = check_version_match(&app, &install_path);

    CliStatus {
        installed: true,
        path: Some(install_path),
        version_match,
        prompt_dismissed,
    }
}

/// Install the CLI binary to the system PATH.
/// On macOS, uses osascript for admin privileges if needed.
#[tauri::command]
pub(crate) fn install_cli(app: tauri::AppHandle) -> Result<String, String> {
    let sidecar_path = resolve_sidecar_path(&app)?;
    let install_path = resolve_install_path();

    copy_with_elevation(&sidecar_path, &install_path)?;

    // Mark executable on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&install_path, std::fs::Permissions::from_mode(0o755));
    }

    tracing::info!(source = "tuic_cli", path = %install_path, "CLI installed");

    Ok(install_path)
}

/// Uninstall the CLI binary from system PATH.
#[tauri::command]
pub(crate) fn uninstall_cli() -> Result<(), String> {
    let install_path = resolve_install_path();
    if !std::path::Path::new(&install_path).exists() {
        return Ok(());
    }

    remove_with_elevation(&install_path)?;
    tracing::info!(source = "tuic_cli", path = %install_path, "CLI uninstalled");
    Ok(())
}

/// Dismiss the first-run CLI install prompt (persisted to disk).
#[tauri::command]
pub(crate) fn dismiss_cli_prompt() {
    let marker = crate::config::config_dir().join(".cli-prompt-dismissed");
    let _ = std::fs::write(&marker, "");
}

#[tauri::command]
pub(crate) fn get_last_seen_version() -> Option<String> {
    let path = crate::config::config_dir().join(".whats-new-seen");
    std::fs::read_to_string(path).ok().filter(|s| !s.is_empty())
}

#[tauri::command]
pub(crate) fn set_last_seen_version(version: String) {
    let path = crate::config::config_dir().join(".whats-new-seen");
    let _ = std::fs::write(path, version);
}

/// Auto-update: if the CLI is installed, overwrite it with the current sidecar.
/// Called at app startup — silent, no elevation prompt (relies on existing permissions).
pub(crate) fn auto_update_cli(app: &tauri::AppHandle) {
    let install_path = resolve_install_path();
    if !std::path::Path::new(&install_path).exists() {
        return;
    }

    if check_version_match(app, &install_path) {
        return;
    }

    let Ok(sidecar_path) = resolve_sidecar_path(app) else {
        return;
    };

    // Try direct copy (no elevation) — will succeed if user owns the file
    if std::fs::copy(&sidecar_path, &install_path).is_ok() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&install_path, std::fs::Permissions::from_mode(0o755));
        }
        tracing::info!(source = "tuic_cli", "CLI auto-updated at {install_path}");
    } else {
        tracing::debug!(
            source = "tuic_cli",
            "CLI auto-update skipped (permission denied)"
        );
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn resolve_install_path() -> String {
    // macOS: /usr/local/bin (in default PATH, standard for user-installed CLIs)
    #[cfg(target_os = "macos")]
    {
        "/usr/local/bin/tuic".to_string()
    }

    // Linux: /usr/local/bin (FHS standard for locally installed software)
    #[cfg(target_os = "linux")]
    {
        "/usr/local/bin/tuic".to_string()
    }

    // Windows: add to user-scoped PATH via %LOCALAPPDATA%\Microsoft\WindowsApps
    // (writable without admin, automatically in PATH on modern Windows)
    #[cfg(target_os = "windows")]
    {
        let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_default();
        format!("{local_app_data}\\Microsoft\\WindowsApps\\tuic.exe")
    }
}

fn resolve_sidecar_path(app: &tauri::AppHandle) -> Result<String, String> {
    let target = current_target_triple();
    let ext = if cfg!(target_os = "windows") {
        ".exe"
    } else {
        ""
    };

    // Release mode: sidecar embedded in app bundle resources
    if let Ok(resource_dir) = app.path().resource_dir() {
        let sidecar = resource_dir.join(format!("binaries/tuic-{target}{ext}"));
        if sidecar.exists() {
            return Ok(sidecar.to_string_lossy().to_string());
        }
    }

    // Dev mode: try the workspace target directory
    let dev_binary =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("target/debug/tuic{ext}"));
    if dev_binary.exists() {
        return Ok(dev_binary.to_string_lossy().to_string());
    }

    // Also try release build in dev
    let release_binary =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("target/release/tuic{ext}"));
    if release_binary.exists() {
        return Ok(release_binary.to_string_lossy().to_string());
    }

    Err("tuic CLI binary not found. Run 'cargo build -p tuic-cli' first.".to_string())
}

fn current_target_triple() -> String {
    // Build-time target triple, same format as `rustc --print host-tuple`
    env!("TUIC_TARGET_TRIPLE").to_string()
}

fn check_version_match(app: &tauri::AppHandle, installed_path: &str) -> bool {
    let Ok(sidecar_path) = resolve_sidecar_path(app) else {
        return false;
    };

    // Compare file sizes as a quick check — different sizes = different versions
    let installed_size = std::fs::metadata(installed_path)
        .map(|m| m.len())
        .unwrap_or(0);
    let sidecar_size = std::fs::metadata(&sidecar_path)
        .map(|m| m.len())
        .unwrap_or(0);

    installed_size == sidecar_size
}

pub(crate) fn copy_with_elevation(src: &str, dst: &str) -> Result<(), String> {
    // Ensure parent directory exists
    if let Some(parent) = std::path::Path::new(dst).parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    // Try direct copy first
    if std::fs::copy(src, dst).is_ok() {
        return Ok(());
    }

    // Need elevation
    #[cfg(target_os = "macos")]
    {
        let parent = std::path::Path::new(dst)
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "/usr/local/bin".to_string());
        let script = format!(
            "do shell script \"mkdir -p '{parent}' && cp -f '{src}' '{dst}' && chmod 755 '{dst}'\" with administrator privileges"
        );
        let status = std::process::Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .status()
            .map_err(|e| format!("Failed to run osascript: {e}"))?;
        if !status.success() {
            return Err("Installation cancelled by user".to_string());
        }
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        let status = std::process::Command::new("pkexec")
            .args(["cp", "-f", src, dst])
            .status()
            .or_else(|_| {
                std::process::Command::new("sudo")
                    .args(["cp", "-f", src, dst])
                    .status()
            })
            .map_err(|e| format!("Failed to elevate: {e}"))?;
        if !status.success() {
            return Err("Installation cancelled by user".to_string());
        }
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        // On Windows the install path is user-writable (LOCALAPPDATA)
        std::fs::copy(src, dst).map_err(|e| format!("Failed to copy: {e}"))?;
        return Ok(());
    }

    #[allow(unreachable_code)]
    Err("Unsupported platform".to_string())
}

pub(crate) fn remove_with_elevation(path: &str) -> Result<(), String> {
    if std::fs::remove_file(path).is_ok() {
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        let script = format!("do shell script \"rm -f '{path}'\" with administrator privileges");
        let status = std::process::Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .status()
            .map_err(|e| format!("Failed to run osascript: {e}"))?;
        if !status.success() {
            return Err("Removal cancelled by user".to_string());
        }
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        let status = std::process::Command::new("pkexec")
            .args(["rm", "-f", path])
            .status()
            .or_else(|_| {
                std::process::Command::new("sudo")
                    .args(["rm", "-f", path])
                    .status()
            })
            .map_err(|e| format!("Failed to elevate: {e}"))?;
        if !status.success() {
            return Err("Removal cancelled by user".to_string());
        }
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        std::fs::remove_file(path).map_err(|e| format!("Failed to remove: {e}"))?;
        return Ok(());
    }

    #[allow(unreachable_code)]
    Err("Unsupported platform".to_string())
}
