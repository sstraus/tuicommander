//! Codex-specific glue: the `[features] hooks = true` enable flag in
//! `~/.codex/config.toml`.
//!
//! Codex won't load `hooks.json` unless this flag is set, so install/uninstall
//! must toggle it symmetrically — a stranded flag would make the install-state
//! report `Outdated` forever with no path back to `NotInstalled`. The edits are
//! line-based so existing formatting, comments, and other `[features]` keys are
//! preserved (a full TOML parse round-trip would drop them).

use std::path::Path;

/// Legacy flag name from an older Codex; we strip it on uninstall too.
const LEGACY_KEY: &str = "codex_hooks";

/// The section name when `line` is a single-bracket TOML header (`[features]`,
/// `[ features ]`, `[features] # note`), else `None`. `[[array]]` is not a header.
fn toml_section(line: &str) -> Option<String> {
    let line = line.split('#').next().unwrap_or(line).trim();
    if line.starts_with('[') && !line.starts_with("[[") && line.ends_with(']') {
        Some(line[1..line.len() - 1].trim().to_string())
    } else {
        None
    }
}

/// True if the (comment-stripped) line assigns `key` (`key = …`), exact-match so
/// `hooks` never matches `hooks_extra`.
fn assigns_key(line: &str, key: &str) -> bool {
    let line = line.split('#').next().unwrap_or(line).trim();
    line.strip_prefix(key)
        .map(|rest| rest.trim_start().starts_with('='))
        .unwrap_or(false)
}

/// True if the line is exactly `hooks = true` (comment/whitespace tolerant).
fn assigns_true(line: &str, key: &str) -> bool {
    let line = line.split('#').next().unwrap_or(line).trim();
    line.strip_prefix(key)
        .and_then(|rest| rest.trim_start().strip_prefix('='))
        .map(|val| val.trim() == "true")
        .unwrap_or(false)
}

/// Whether `[features] hooks = true` is currently set.
pub(crate) fn features_hooks_present(config: &Path) -> bool {
    let Ok(contents) = std::fs::read_to_string(config) else {
        return false;
    };
    let mut in_features = false;
    for line in contents.lines() {
        if let Some(section) = toml_section(line) {
            in_features = section == "features";
            continue;
        }
        if in_features && assigns_true(line, "hooks") {
            return true;
        }
    }
    false
}

/// Set (`enabled`) or clear `[features] hooks = true`, preserving the rest of the
/// file. Idempotent. Creates the file/section as needed when enabling.
pub(crate) fn set_features_hooks_flag(config: &Path, enabled: bool) -> Result<(), String> {
    let contents = std::fs::read_to_string(config).unwrap_or_default();
    let out = if enabled {
        enable(&contents)
    } else {
        disable(&contents)
    };
    // Nothing to write when disabling a file that never existed.
    if !enabled && contents.is_empty() {
        return Ok(());
    }
    crate::config::persist_atomic(config, out.as_bytes())
}

fn enable(contents: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut in_features = false;
    let mut wrote = false;
    let mut features_seen = false;
    for line in contents.lines() {
        if let Some(section) = toml_section(line) {
            if in_features && !wrote {
                out.push("hooks = true".to_string());
                wrote = true;
            }
            in_features = section == "features";
            features_seen |= in_features;
            out.push(line.to_string());
            continue;
        }
        if in_features && assigns_key(line, "hooks") {
            out.push("hooks = true".to_string()); // replace any prior value
            wrote = true;
            continue;
        }
        out.push(line.to_string());
    }
    if in_features && !wrote {
        out.push("hooks = true".to_string());
    }
    if !features_seen {
        if out.last().is_some_and(|l| !l.trim().is_empty()) {
            out.push(String::new());
        }
        out.push("[features]".to_string());
        out.push("hooks = true".to_string());
    }
    finalize(out)
}

fn disable(contents: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut in_features = false;
    for line in contents.lines() {
        if let Some(section) = toml_section(line) {
            in_features = section == "features";
            out.push(line.to_string());
            continue;
        }
        if in_features && (assigns_key(line, "hooks") || assigns_key(line, LEGACY_KEY)) {
            continue; // drop only our flag(s); other [features] keys stay
        }
        out.push(line.to_string());
    }
    finalize(out)
}

fn finalize(lines: Vec<String>) -> String {
    let mut s = lines.join("\n");
    if !s.is_empty() && !s.ends_with('\n') {
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write(dir: &TempDir, body: &str) -> std::path::PathBuf {
        let p = dir.path().join("config.toml");
        std::fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn agent_hook_codex_flag_created_when_missing() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("config.toml");
        assert!(!features_hooks_present(&p));
        set_features_hooks_flag(&p, true).unwrap();
        assert!(features_hooks_present(&p));
        assert!(std::fs::read_to_string(&p).unwrap().contains("[features]"));
    }

    #[test]
    fn agent_hook_codex_flag_added_to_existing_features_preserving_keys() {
        let dir = TempDir::new().unwrap();
        let p = write(&dir, "[features]\nweb_search = true\n\n[other]\nx = 1\n");
        set_features_hooks_flag(&p, true).unwrap();
        let c = std::fs::read_to_string(&p).unwrap();
        assert!(features_hooks_present(&p));
        assert!(c.contains("web_search = true"), "sibling feature key kept");
        assert!(c.contains("x = 1"), "other section kept");
    }

    #[test]
    fn agent_hook_codex_flag_idempotent() {
        let dir = TempDir::new().unwrap();
        let p = write(&dir, "[features]\nhooks = true\n");
        set_features_hooks_flag(&p, true).unwrap();
        let count = std::fs::read_to_string(&p)
            .unwrap()
            .matches("hooks = true")
            .count();
        assert_eq!(count, 1, "no duplicate flag");
    }

    #[test]
    fn agent_hook_codex_flag_cleared_preserving_other_config() {
        let dir = TempDir::new().unwrap();
        let p = write(
            &dir,
            "[features]\nhooks = true\nweb_search = true\n\n[mcp.server]\ncmd = \"x\"\n",
        );
        set_features_hooks_flag(&p, false).unwrap();
        let c = std::fs::read_to_string(&p).unwrap();
        assert!(!features_hooks_present(&p), "our flag removed");
        assert!(c.contains("web_search = true"), "sibling feature kept");
        assert!(c.contains("[mcp.server]"), "other section kept");
    }

    #[test]
    fn agent_hook_codex_flag_disable_missing_file_is_noop() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("config.toml");
        set_features_hooks_flag(&p, false).unwrap();
        assert!(!p.exists(), "must not create the file when disabling");
    }

    #[test]
    fn agent_hook_codex_commented_flag_not_detected() {
        let dir = TempDir::new().unwrap();
        let p = write(&dir, "[features]\n# hooks = true\n");
        assert!(
            !features_hooks_present(&p),
            "a commented flag is not active"
        );
    }
}
