//! Tauri commands for managing the `tuic` CLI binary installation.
//!
//! The CLI binary is embedded as a sidecar. These commands handle:
//! - Checking if the CLI is installed in PATH
//! - Installing the CLI (copy sidecar to /usr/local/bin/tuic)
//! - Auto-updating the installed CLI on app startup
//! - Tracking whether the first-run prompt has been dismissed

use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct CliStatus {
    installed: bool,
    path: Option<String>,
    version_match: bool,
    /// True when the installed binary can be overwritten without elevation, i.e.
    /// the silent startup auto-update can actually apply a pending update.
    /// False (e.g. a root-owned file) means "restart to apply" would never work —
    /// the user must click Update to trigger the elevation prompt.
    auto_updatable: bool,
    prompt_dismissed: bool,
}

/// Check CLI installation status.
#[tauri::command]
pub(crate) fn get_cli_status() -> CliStatus {
    let prompt_dismissed = crate::config::config_dir()
        .join(".cli-prompt-dismissed")
        .exists();

    // Prefer the canonical install location (keeps install/update semantics),
    // then fall back to wherever `tuic` lives on the user's PATH — they may have
    // symlinked it into their own bin dir without using our installer (#98).
    let canonical = resolve_install_path();
    let install_path = if std::path::Path::new(&canonical).exists() {
        Some(canonical)
    } else {
        crate::cli::which_cli(sidecar_name())
    };

    let Some(install_path) = install_path else {
        return CliStatus {
            installed: false,
            path: None,
            version_match: false,
            auto_updatable: false,
            prompt_dismissed,
        };
    };

    // Check if installed version matches current sidecar
    let version_match = check_version_match(&install_path);
    let auto_updatable = install_path_writable(&install_path);

    CliStatus {
        installed: true,
        path: Some(install_path),
        version_match,
        auto_updatable,
        prompt_dismissed,
    }
}

/// Install the CLI binary to the system PATH.
/// On macOS, uses osascript for admin privileges if needed.
#[tauri::command]
pub(crate) fn install_cli() -> Result<String, String> {
    require_global_cli_mutation()?;
    let sidecar_path = resolve_sidecar_path()?;
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
    require_global_cli_mutation()?;
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

/// Auto-update: if the CLI is installed, atomically replace it with the current sidecar.
/// Called off the setup path — silent, no elevation prompt (relies on directory permissions).
pub(crate) fn auto_update_cli() {
    if crate::agent_test::active() {
        return;
    }
    let install_path = resolve_install_path();
    if !std::path::Path::new(&install_path).exists() {
        return;
    }

    if check_version_match(&install_path) {
        return;
    }

    let Ok(sidecar_path) = resolve_sidecar_path() else {
        return;
    };

    if replace_cli_atomically(
        std::path::Path::new(&sidecar_path),
        std::path::Path::new(&install_path),
    )
    .is_ok()
    {
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

fn require_global_cli_mutation() -> Result<(), String> {
    if crate::agent_test::active() {
        return Err(
            "Global CLI installation, update and removal are disabled in agent-test".into(),
        );
    }
    Ok(())
}

fn replace_cli_atomically(
    sidecar: &std::path::Path,
    install_path: &std::path::Path,
) -> Result<(), String> {
    require_global_cli_mutation()?;
    let staged = install_path.with_extension(format!("update.{}", uuid::Uuid::new_v4()));
    let result = (|| {
        std::fs::copy(sidecar, &staged).map_err(|e| format!("Failed to stage CLI update: {e}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| format!("Failed to make staged CLI executable: {e}"))?;
        }
        // Opened for writing, not reading: Windows `FlushFileBuffers` needs a
        // handle with write access and answers a read-only one with
        // ERROR_ACCESS_DENIED, so the update never reached the install path.
        std::fs::OpenOptions::new()
            .write(true)
            .open(&staged)
            .and_then(|file| file.sync_all())
            .map_err(|e| format!("Failed to flush staged CLI update: {e}"))?;
        std::fs::rename(&staged, install_path)
            .map_err(|e| format!("Failed to install staged CLI update: {e}"))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&staged);
    }
    result
}

fn resolve_install_path() -> String {
    #[cfg(test)]
    if let Some(root) = crate::agent_test::test_root() {
        return root.join("installed-tuic").to_string_lossy().into_owned();
    }

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

/// Bundled sidecar filename (no target-triple suffix — Tauri strips it at
/// bundle time, so the installed file is plain `tuic`/`tuic.exe`).
fn sidecar_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "tuic.exe"
    } else {
        "tuic"
    }
}

/// Locate the bundled `tuic` sidecar to copy into PATH.
///
/// `externalBin` sidecars are installed *next to the main executable* —
/// `Contents/MacOS/` on macOS, the install dir on Windows, the same dir on
/// Linux — NOT under the resource dir. This mirrors `detect_bridge_binary`
/// in `agent_mcp.rs`; the previous resource-dir lookup never resolved in a
/// packaged build and fell through to the dev path, which only exists on the
/// build machine (the root cause of issue #52).
fn resolve_sidecar_path() -> Result<String, String> {
    #[cfg(test)]
    if let Some(root) = crate::agent_test::test_root() {
        return Ok(root.join("sidecar-tuic").to_string_lossy().into_owned());
    }

    let name = sidecar_name();

    // Release: sidecar bundled alongside the main executable.
    // Dev (`cargo tauri dev`/`build`): also next to the built app under target/.
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let candidate = dir.join(name);
        if candidate.exists() {
            return Ok(candidate.to_string_lossy().to_string());
        }
    }

    // Dev fallback: workspace target directory (build:sidecar output).
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for profile in ["debug", "release"] {
        let dev_binary = manifest.join(format!("target/{profile}")).join(name);
        if dev_binary.exists() {
            return Ok(dev_binary.to_string_lossy().to_string());
        }
    }

    Err("tuic CLI binary not found. Run 'cargo build -p tuic-cli' first.".to_string())
}

/// Run `<path> --version` and return its trimmed stdout (e.g. "tuic 1.1.0").
/// Returns None if the binary can't be executed or exits non-zero.
fn cli_version(path: &str) -> Option<String> {
    let output = std::process::Command::new(path)
        .arg("--version")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!version.is_empty()).then_some(version)
}

/// Compare the installed CLI against the bundled sidecar by their reported
/// version strings — NOT file size, which flaps on every rebuild of the same
/// version (different rustc/deps produce different-sized but identical-version
/// binaries). If either version can't be determined, treat as a mismatch.
fn check_version_match(installed_path: &str) -> bool {
    let Ok(sidecar_path) = resolve_sidecar_path() else {
        return false;
    };
    match (cli_version(installed_path), cli_version(&sidecar_path)) {
        (Some(installed), Some(sidecar)) => installed == sidecar,
        _ => false,
    }
}

/// Atomic updates stage a sibling and replace the directory entry, so probe the
/// directory, not the executable. On macOS even a write-open without any writes
/// can invalidate the signed executable's cached signature (#2129-1dfd).
fn install_path_writable(path: &str) -> bool {
    let path = std::path::Path::new(path);
    #[cfg(windows)]
    if path
        .metadata()
        .is_ok_and(|metadata| metadata.permissions().readonly())
    {
        // Windows refuses replacement of a read-only destination.
        return false;
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    tempfile::Builder::new()
        .prefix(".tuic-update-probe-")
        .tempfile_in(parent)
        .is_ok()
}

pub(crate) fn copy_with_elevation(src: &str, dst: &str) -> Result<(), String> {
    require_global_cli_mutation()?;
    // Ensure parent directory exists
    if let Some(parent) = std::path::Path::new(dst).parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    // Stage beside the destination so a running CLI keeps its old inode.
    let direct_result =
        replace_cli_atomically(std::path::Path::new(src), std::path::Path::new(dst));
    if direct_result.is_ok() {
        return Ok(());
    }

    // Need elevation
    #[cfg(target_os = "macos")]
    {
        let staged = format!("{dst}.update.{}", uuid::Uuid::new_v4());
        let parent = std::path::Path::new(dst)
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "/usr/local/bin".to_string());
        let script = format!(
            "do shell script \"mkdir -p '{parent}' && cp -f '{src}' '{staged}' && chmod 755 '{staged}' && mv -f '{staged}' '{dst}'\" with administrator privileges"
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
        let staged = format!("{dst}.update.{}", uuid::Uuid::new_v4());
        let script = "cp -- \"$1\" \"$2\" && chmod 755 \"$2\" && mv -f -- \"$2\" \"$3\"";
        let status = std::process::Command::new("pkexec")
            .args(["sh", "-c", script, "sh", src, &staged, dst])
            .status()
            .or_else(|_| {
                std::process::Command::new("sudo")
                    .args(["sh", "-c", script, "sh", src, &staged, dst])
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
        // On Windows the install path is user-writable (LOCALAPPDATA).
        return direct_result;
    }

    #[allow(unreachable_code)]
    Err("Unsupported platform".to_string())
}

pub(crate) fn remove_with_elevation(path: &str) -> Result<(), String> {
    require_global_cli_mutation()?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn isolated_cli(test: impl FnOnce(&std::path::Path, &std::path::Path)) {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let installed = dir.path().join("installed-tuic");
        let sidecar = dir.path().join("sidecar-tuic");
        std::fs::write(&installed, b"daily-cli").unwrap();
        std::fs::write(&sidecar, b"agent-test-cli").unwrap();
        crate::agent_test::with_test_profile(dir.path(), || test(&installed, &sidecar));
    }

    // Catches: checking the old executable's write bit reports that atomic
    // replacement needs elevation even when its directory permits replacement.
    #[cfg(unix)]
    #[test]
    fn cli_update_probe_uses_install_directory_not_executable_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let installed = dir.path().join("tuic");
        std::fs::write(&installed, b"old-cli").unwrap();
        std::fs::set_permissions(&installed, std::fs::Permissions::from_mode(0o555)).unwrap();

        assert!(
            install_path_writable(installed.to_str().unwrap()),
            "atomic replacement needs a writable directory, not a writable executable"
        );
        assert_eq!(std::fs::read(&installed).unwrap(), b"old-cli");
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            1,
            "probe must clean up"
        );
    }

    // Catches: the probe answers true unconditionally (or from the executable's
    // own mode), so a read-only install directory is reported as auto-updatable.
    #[cfg(unix)]
    #[test]
    fn cli_update_probe_rejects_read_only_install_directory() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let installed = dir.path().join("tuic");
        std::fs::write(&installed, b"old-cli").unwrap();
        std::fs::set_permissions(&installed, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
        let root = std::fs::File::create(dir.path().join("root-check")).is_ok();
        let writable = install_path_writable(installed.to_str().unwrap());
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        if root {
            return; // permission bits do not bind root
        }
        assert!(
            !writable,
            "a read-only directory cannot take an atomic replacement"
        );
    }

    #[test]
    fn agent_test_cli_auto_update_preserves_global_binary() {
        // Bug caught: ordinary desktop setup replaces the global CLI from agent-test.
        isolated_cli(|installed, _| {
            auto_update_cli();
            assert_eq!(std::fs::read(installed).unwrap(), b"daily-cli");
        });
    }

    #[test]
    fn agent_test_cli_install_refuses_global_replacement() {
        // Bug caught: the explicit installer bypasses startup isolation.
        isolated_cli(|installed, _| {
            let result = install_cli();
            assert_eq!(std::fs::read(installed).unwrap(), b"daily-cli");
            assert!(result.unwrap_err().contains("agent-test"));
        });
    }

    #[test]
    fn agent_test_cli_uninstall_refuses_global_removal() {
        // Bug caught: explicit uninstall removes the daily CLI from agent-test.
        isolated_cli(|installed, _| {
            let result = uninstall_cli();
            assert_eq!(std::fs::read(installed).unwrap(), b"daily-cli");
            assert!(result.unwrap_err().contains("agent-test"));
        });
    }

    #[test]
    fn agent_test_cli_helpers_refuse_global_mutations() {
        // Bug caught: another CLI caller bypasses the guarded command via elevation helpers.
        isolated_cli(|installed, sidecar| {
            let copy = copy_with_elevation(sidecar.to_str().unwrap(), installed.to_str().unwrap());
            assert_eq!(std::fs::read(installed).unwrap(), b"daily-cli");
            assert!(copy.unwrap_err().contains("agent-test"));
            let remove = remove_with_elevation(installed.to_str().unwrap());
            assert_eq!(std::fs::read(installed).unwrap(), b"daily-cli");
            assert!(remove.unwrap_err().contains("agent-test"));
        });
    }

    #[test]
    fn cli_update_replaces_the_installed_binary_atomically() {
        let dir = tempfile::tempdir().expect("temp CLI dir");
        let sidecar = dir.path().join("sidecar-tuic");
        let installed = dir.path().join("tuic");
        std::fs::write(&sidecar, b"complete-new-binary").expect("write sidecar");
        std::fs::write(&installed, b"complete-old-binary").expect("write installed CLI");

        replace_cli_atomically(&sidecar, &installed).expect("replace installed CLI");

        assert_eq!(
            std::fs::read(&installed).expect("read installed CLI"),
            b"complete-new-binary"
        );
        assert_eq!(
            std::fs::read_dir(dir.path())
                .expect("read CLI dir")
                .filter_map(Result::ok)
                .count(),
            2,
            "the staging file must not survive the replacement"
        );
    }

    #[test]
    fn manual_cli_install_keeps_existing_reader_on_old_binary() {
        let dir =
            tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("temp CLI dir");
        let sidecar = dir.path().join("sidecar-tuic");
        let installed = dir.path().join("tuic");
        let reader = dir.path().join("running-reader");
        std::fs::write(&sidecar, b"complete-new-binary").expect("write sidecar");
        std::fs::write(&installed, b"complete-old-binary").expect("write installed CLI");
        std::fs::hard_link(&installed, &reader).expect("preserve old inode for reader");

        copy_with_elevation(sidecar.to_str().unwrap(), installed.to_str().unwrap())
            .expect("install CLI");

        assert_eq!(std::fs::read(&reader).unwrap(), b"complete-old-binary");
        assert_eq!(std::fs::read(&installed).unwrap(), b"complete-new-binary");
    }

    #[cfg(unix)]
    #[test]
    fn manual_cli_install_replaces_a_link_without_rewriting_its_target() {
        let dir =
            tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("temp CLI dir");
        let sidecar = dir.path().join("new-sidecar");
        let old_target = dir.path().join("old-sidecar");
        let installed = dir.path().join("tuic");
        std::fs::write(&sidecar, b"complete-new-binary").unwrap();
        std::fs::write(&old_target, b"complete-old-binary").unwrap();
        std::os::unix::fs::symlink(&old_target, &installed).unwrap();

        copy_with_elevation(sidecar.to_str().unwrap(), installed.to_str().unwrap())
            .expect("install CLI");

        assert_eq!(std::fs::read(&old_target).unwrap(), b"complete-old-binary");
        assert_eq!(std::fs::read(&installed).unwrap(), b"complete-new-binary");
        assert!(!std::fs::symlink_metadata(&installed).unwrap().is_symlink());
    }

    /// Regression for issue #52: the bundled sidecar must be looked up by its
    /// plain name (no `-{target-triple}` suffix), since Tauri strips the triple
    /// when bundling `externalBin`. A reintroduced triple would make the lookup
    /// miss the packaged binary and fall through to the dev-only path.
    #[test]
    fn sidecar_name_has_no_target_triple() {
        let name = sidecar_name();
        assert!(
            !name.contains("aarch64")
                && !name.contains("x86_64")
                && !name.contains("apple")
                && !name.contains("pc-windows")
                && !name.contains("unknown-linux"),
            "sidecar name must not embed a target triple, got {name:?}"
        );
        #[cfg(target_os = "windows")]
        assert_eq!(name, "tuic.exe");
        #[cfg(not(target_os = "windows"))]
        assert_eq!(name, "tuic");
    }
}
