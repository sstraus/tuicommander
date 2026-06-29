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
            let local_app_data =
                std::env::var("LOCALAPPDATA").unwrap_or_else(|_| format!("{home}\\AppData\\Local"));
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

/// Apply `CREATE_NO_WINDOW` on Windows to suppress console window flash.
///
/// No-op on other platforms. Use for background `.output()` and piped
/// `.spawn()` calls — never for interactive processes that need a visible window.
pub(crate) fn apply_no_window(_cmd: &mut std::process::Command) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        _cmd.creation_flags(CREATE_NO_WINDOW);
    }
}

/// Expand a leading `~` or `~/` to `$HOME`. `std::process::Command` and
/// `std::fs` APIs do not invoke a shell, so tilde is treated as a literal
/// character and the OS returns ENOENT.
pub(crate) fn expand_tilde(path: &str) -> String {
    if (path == "~" || path.starts_with("~/"))
        && let Ok(home) = std::env::var("HOME")
    {
        return format!("{}{}", home, &path[1..]);
    }
    path.to_string()
}

/// Return a PATH string that prepends extra bin dirs to the current PATH.
///
/// Used to enrich subprocess environments so git hooks and other child
/// processes can find tools (pnpm, node, etc.) that desktop-launched apps
/// don't have on PATH.
pub(crate) fn enriched_path() -> String {
    let current = std::env::var("PATH").unwrap_or_default();
    let extra = extra_bin_dirs();
    if extra.is_empty() {
        return current;
    }
    let mut dirs: Vec<&str> = extra.iter().map(String::as_str).collect();
    if !current.is_empty() {
        dirs.push(&current);
    }
    let sep = if cfg!(windows) { ";" } else { ":" };
    dirs.join(sep)
}

/// Check if a CLI tool exists on PATH or in well-known directories.
pub(crate) fn has_cli(name: &str) -> bool {
    which_cli(name).is_some()
}

/// Locate a CLI tool and return the path where it was found, or None.
///
/// Uses `which`/`where` first (honors the inherited PATH, including dirs the
/// user added like `~/bin`), then falls back to probing well-known dirs that
/// desktop-launched apps miss. The path is returned *as found* — a symlink on
/// PATH is reported as the symlink, not its target (issue #98).
pub(crate) fn which_cli(name: &str) -> Option<String> {
    let checker = if cfg!(target_os = "windows") {
        "where"
    } else {
        "which"
    };
    let mut cmd = std::process::Command::new(checker);
    cmd.arg(name);
    apply_no_window(&mut cmd);
    if let Ok(output) = cmd.output()
        && output.status.success()
        && let Some(line) = String::from_utf8_lossy(&output.stdout).lines().next()
    {
        let path = line.trim();
        if !path.is_empty() {
            return Some(path.to_string());
        }
    }
    // Fallback: probe extra_bin_dirs (resolve_cli returns `name` unchanged when
    // not found).
    let resolved = resolve_cli(name);
    (resolved != name).then_some(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_which_cli_none_for_missing() {
        assert_eq!(which_cli("nonexistent_binary_xyz_12345"), None);
    }

    #[test]
    fn test_which_cli_finds_known_binary() {
        // `sh` is on PATH on every Unix; `cmd` on every Windows.
        #[cfg(not(target_os = "windows"))]
        let name = "sh";
        #[cfg(target_os = "windows")]
        let name = "cmd";
        let found = which_cli(name);
        assert!(
            found.as_deref().is_some_and(|p| !p.is_empty()),
            "which_cli({name:?}) should resolve to a path, got {found:?}"
        );
    }

    #[test]
    fn test_expand_tilde_home_prefix() {
        let home = std::env::var("HOME").unwrap();
        assert_eq!(expand_tilde("~/foo/bar"), format!("{home}/foo/bar"));
        assert_eq!(expand_tilde("~"), home);
    }

    #[test]
    fn test_expand_tilde_no_op() {
        assert_eq!(expand_tilde("/usr/bin/git"), "/usr/bin/git");
        assert_eq!(expand_tilde("relative/path"), "relative/path");
        assert_eq!(expand_tilde("~other_user/dir"), "~other_user/dir");
        assert_eq!(expand_tilde(""), "");
    }
}
