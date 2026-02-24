//! CLI binary resolution with caching.
//!
//! Desktop-launched apps (Finder, Explorer, desktop launchers) don't inherit
//! the user's shell PATH, so CLI tools like `git` and `gh` aren't found.
//! This module probes well-known directories and caches the results for
//! the lifetime of the app.

use std::collections::HashMap;
use std::sync::OnceLock;

/// Well-known directories where CLI tools live but that desktop-launched apps
/// don't have on PATH. Computed once and cached via OnceLock.
fn extra_bin_dirs() -> &'static [String] {
    static DIRS: OnceLock<Vec<String>> = OnceLock::new();
    DIRS.get_or_init(|| {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_default();

        let mut dirs = Vec::new();

        #[cfg(target_os = "macos")]
        {
            dirs.extend([
                "/usr/local/bin".to_string(),
                "/opt/homebrew/bin".to_string(),
                "/opt/homebrew/sbin".to_string(),
            ]);
        }

        #[cfg(target_os = "linux")]
        {
            dirs.extend([
                "/usr/bin".to_string(),
                "/usr/local/bin".to_string(),
                format!("{home}/.local/bin"),
                "/snap/bin".to_string(),
                "/var/lib/flatpak/exports/bin".to_string(),
            ]);
        }

        // Common across Unix-like systems
        #[cfg(not(target_os = "windows"))]
        {
            dirs.push(format!("{home}/.cargo/bin"));
        }

        #[cfg(target_os = "windows")]
        {
            let program_files =
                std::env::var("ProgramFiles").unwrap_or_else(|_| "C:\\Program Files".to_string());
            let local_app_data = std::env::var("LOCALAPPDATA")
                .unwrap_or_else(|_| format!("{home}\\AppData\\Local"));
            dirs.extend([
                format!("{home}\\.cargo\\bin"),
                format!("{local_app_data}\\Programs\\Microsoft VS Code\\bin"),
                format!("{local_app_data}\\Programs\\cursor\\resources\\app\\bin"),
                format!("{program_files}\\Microsoft VS Code\\bin"),
                format!("{local_app_data}\\Programs\\windsurf\\resources\\app\\bin"),
                format!("{home}\\scoop\\shims"),
                format!("{home}\\AppData\\Roaming\\npm"),
            ]);
        }

        dirs
    })
}

/// Resolve a CLI binary to its full path, probing well-known directories that
/// desktop-launched apps don't have on PATH.
///
/// Results are cached per binary name for the lifetime of the app —
/// CLI tool locations don't change at runtime.
pub(crate) fn resolve_cli(name: &str) -> String {
    static CACHE: OnceLock<parking_lot::Mutex<HashMap<String, String>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| parking_lot::Mutex::new(HashMap::new()));

    {
        let guard = cache.lock();
        if let Some(cached) = guard.get(name) {
            return cached.clone();
        }
    }

    // Not cached — probe filesystem
    let resolved = resolve_cli_uncached(name);

    {
        let mut guard = cache.lock();
        guard.insert(name.to_string(), resolved.clone());
    }

    resolved
}

/// Uncached version for testing and first-call resolution
fn resolve_cli_uncached(name: &str) -> String {
    for dir in extra_bin_dirs() {
        let candidate = std::path::Path::new(dir).join(name);
        if candidate.exists() {
            return candidate.to_string_lossy().to_string();
        }
    }
    name.to_string()
}

/// Check if a CLI tool exists on PATH or in well-known directories.
pub(crate) fn has_cli(name: &str) -> bool {
    let checker = if cfg!(target_os = "windows") {
        "where"
    } else {
        "which"
    };
    if std::process::Command::new(checker)
        .arg(name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return true;
    }
    // Also check extra_bin_dirs
    resolve_cli(name) != name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extra_bin_dirs_returns_non_empty() {
        let dirs = extra_bin_dirs();
        assert!(!dirs.is_empty(), "extra_bin_dirs should return at least one directory");
    }

    #[test]
    fn test_extra_bin_dirs_no_duplicates() {
        let dirs = extra_bin_dirs();
        let mut seen = std::collections::HashSet::new();
        for dir in dirs {
            assert!(seen.insert(dir), "Duplicate directory in extra_bin_dirs: {dir}");
        }
    }

    #[test]
    fn test_extra_bin_dirs_no_empty_strings() {
        let dirs = extra_bin_dirs();
        for dir in dirs {
            assert!(!dir.is_empty(), "extra_bin_dirs should not contain empty strings");
        }
    }

    #[test]
    fn test_resolve_cli_returns_name_when_not_found() {
        let result = resolve_cli("nonexistent_binary_xyz_12345");
        assert_eq!(result, "nonexistent_binary_xyz_12345");
    }

    #[test]
    fn test_resolve_cli_finds_known_binary() {
        #[cfg(not(target_os = "windows"))]
        {
            let result = resolve_cli("git");
            if std::path::Path::new("/usr/bin/git").exists()
                || std::path::Path::new("/usr/local/bin/git").exists()
                || std::path::Path::new("/opt/homebrew/bin/git").exists()
            {
                assert!(
                    result.contains('/'),
                    "resolve_cli('git') should return an absolute path, got: {result}"
                );
            }
        }
    }

    #[test]
    fn test_resolve_cli_caches_result() {
        // Call twice — should return the same value (cached)
        let first = resolve_cli("nonexistent_cached_test_abc");
        let second = resolve_cli("nonexistent_cached_test_abc");
        assert_eq!(first, second);
    }
}
