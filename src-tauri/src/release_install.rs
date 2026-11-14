//! Download one binary from a GitHub release and install it on this host.
//!
//! Shared by the mdkb and ego installers. Both projects publish a bare
//! executable per platform, named `<tool>-<os>-<arch>` (`.exe` on Windows),
//! for example `ego-macos-arm64`. The ego release also carries target-triple
//! archives (`ego-aarch64-apple-darwin.tar.gz`); this installer uses only the
//! bare binaries, so no archive is extracted and both tools share one name rule.

use std::path::{Path, PathBuf};

/// The `latest/download` URL prefix of a GitHub repository (`owner/name`).
pub(crate) fn latest_download_base(repo: &str) -> String {
    format!("https://github.com/{repo}/releases/latest/download")
}

/// The release asset of `tool` for this platform.
pub(crate) fn asset_name(tool: &str) -> String {
    format!("{tool}-{}", platform_suffix())
}

fn platform_suffix() -> &'static str {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "macos-arm64"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "macos-x64"
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "linux-x64"
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        "linux-arm64"
    }
    #[cfg(target_os = "windows")]
    {
        "windows-x64.exe"
    }
}

/// Where the installer puts `tool`.
pub(crate) fn install_path(tool: &str) -> PathBuf {
    // agent-test must never touch the machine-wide CLI directory: it installs
    // beside its own isolated config instead.
    if crate::agent_test::active() {
        return crate::config::config_dir().join("bin").join(tool);
    }
    #[cfg(unix)]
    {
        PathBuf::from(format!("/usr/local/bin/{tool}"))
    }
    #[cfg(target_os = "windows")]
    {
        let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
        PathBuf::from(format!("{local}\\Microsoft\\WindowsApps\\{tool}.exe"))
    }
}

fn stage_for_elevation(
    base: &Path,
    asset: &str,
    bytes: &[u8],
) -> std::io::Result<crate::private_scratch::StagedFile> {
    crate::private_scratch::stage_for_elevated_copy(base, "release-install-", asset, bytes)
}

/// Download `<base_url>/<asset>` and install it executable at `install_path`.
pub(crate) async fn download_and_install(
    base_url: &str,
    asset: &str,
    install_path: &Path,
) -> Result<(), String> {
    let url = format!("{base_url}/{asset}");

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

    let tmp_path = install_path.with_extension("tmp");

    if let Some(parent) = install_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    // Try direct write first
    let direct_ok = std::fs::write(&tmp_path, &bytes).is_ok();

    if direct_ok {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o755)).map_err(
                |e| {
                    format!(
                        "Failed to set executable bit: {e}. Try: chmod +x {}",
                        install_path.display()
                    )
                },
            )?;
        }
        std::fs::rename(&tmp_path, install_path)
            .map_err(|e| format!("Failed to move binary: {e}"))?;
    } else {
        // Need elevation. The staged file is copied as root, so it must sit
        // in a fresh private dir (random name, 0700), never a predictable
        // `temp_dir()/<tool>-install` another local user could pre-create and
        // swap the binary in. Dropping `staged` removes the dir on every path.
        let _ = std::fs::remove_file(&tmp_path);
        let staged = stage_for_elevation(&std::env::temp_dir(), asset, &bytes)
            .map_err(|e| format!("Failed to stage binary: {e}"))?;

        crate::tuic_cli::copy_with_elevation(
            &staged.path().to_string_lossy(),
            &install_path.to_string_lossy(),
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_test_installs_under_its_config_dir_not_the_global_bin() {
        // Catches agent-test installing into /usr/local/bin (a global CLI mutation).
        let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        crate::agent_test::with_test_profile(root.path(), || {
            let path = install_path("ego");
            assert!(
                path.starts_with(crate::config::config_dir()),
                "{} escapes the agent-test config dir",
                path.display()
            );
            assert_eq!(path.file_name().unwrap(), "ego");
        });
    }

    #[test]
    fn staging_does_not_reuse_a_planted_install_path() {
        // Catches the elevated installer adopting a directory controlled by another user.
        let base = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let planted = base.path().join("release-install");
        std::fs::create_dir(&planted).unwrap();
        let planted_binary = planted.join("mdkb-linux-x64");
        std::fs::write(&planted_binary, b"attacker-controlled").unwrap();
        let staged = stage_for_elevation(base.path(), "mdkb-linux-x64", b"downloaded").unwrap();
        assert_ne!(
            staged.path().parent().unwrap(),
            planted,
            "must not reuse a planted staging directory"
        );
        assert_eq!(std::fs::read(staged.path()).unwrap(), b"downloaded");
        assert_eq!(
            std::fs::read(&planted_binary).unwrap(),
            b"attacker-controlled"
        );
    }
}
