//! OpenCode adapter: a code-extension plugin (Bun/TS) rather than a settings
//! hook. TUIC owns the whole plugin file `~/.config/opencode/plugin/tuic.ts`,
//! identified by a `/* tuic-managed */` marker on the first line.
//!
//! Install writes the file only if it is absent or already ours (never clobbers a
//! user plugin sharing that path); uninstall deletes it only if marked. The
//! generated plugin emits OSC 7770 to the controlling tty on lifecycle events,
//! inert outside a TUIC session.

use crate::agent_hook_installer::InstallState;
use std::path::Path;

/// First-line ownership marker — both the install guard and the uninstall gate.
pub(crate) const OPENCODE_MARKER: &str = "/* tuic-managed */";

/// The generated OpenCode plugin source. Mirrors the shell hook: guard on
/// `TUIC_SESSION`, write `OSC 7770;state=…` to `/dev/tty`, never throw.
pub(crate) fn opencode_plugin_source() -> String {
    format!(
        r#"{OPENCODE_MARKER}
// TUICommander native-hook instrumentation for OpenCode.
// Drives busy/idle/awaiting by emitting OSC 7770 to the controlling tty.
// Inert outside a TUIC session (guarded on TUIC_SESSION). Do not edit — this
// file is managed by TUICommander and is overwritten/removed by the Settings
// toggle.
import {{ openSync, writeSync, closeSync }} from "node:fs";

function emit(state) {{
  if (!process.env.TUIC_SESSION) return;
  try {{
    const fd = openSync("/dev/tty", "w");
    writeSync(fd, `\x1b]7770;state=${{state}}\x1b\\`);
    closeSync(fd);
  }} catch {{
    /* no controlling tty — no-op */
  }}
}}

export const TuicState = async () => ({{
  "tool.execute.before": async () => emit("busy"),
  "tool.execute.after": async () => emit("busy"),
  "permission.asked": async () => emit("awaiting"),
  event: async ({{ event }}) => {{
    if (event?.type === "session.idle") emit("idle");
  }},
}});
"#
    )
}

/// Install: write the plugin, but refuse to overwrite a user plugin that doesn't
/// carry our marker.
pub(crate) fn install_opencode_plugin(path: &Path) -> Result<(), String> {
    if path.exists() {
        let existing =
            std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        if !existing.contains(OPENCODE_MARKER) {
            return Err(format!(
                "{} exists and is not TUIC-managed — refusing to overwrite",
                path.display()
            ));
        }
    }
    crate::config::persist_atomic(path, opencode_plugin_source().as_bytes())
}

/// Uninstall: delete the plugin only if it carries our marker. Missing/unmarked
/// file is a no-op (we never delete a user plugin).
pub(crate) fn uninstall_opencode_plugin(path: &Path) -> Result<(), String> {
    if path.exists() {
        let existing =
            std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        if existing.contains(OPENCODE_MARKER) {
            std::fs::remove_file(path).map_err(|e| format!("remove {}: {e}", path.display()))?;
        }
    }
    Ok(())
}

/// State: present + marked + current source → Installed; present + marked + stale
/// → Outdated; absent or present-but-unmarked → NotInstalled.
pub(crate) fn opencode_plugin_state(path: &Path) -> InstallState {
    let Ok(content) = std::fs::read_to_string(path) else {
        return InstallState::NotInstalled;
    };
    if !content.contains(OPENCODE_MARKER) {
        return InstallState::NotInstalled;
    }
    if content == opencode_plugin_source() {
        InstallState::Installed
    } else {
        InstallState::Outdated
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn agent_hook_opencode_source_has_osc_and_guard() {
        let src = opencode_plugin_source();
        assert!(src.starts_with(OPENCODE_MARKER), "marker on first line");
        assert!(src.contains(r"7770;state="), "emits OSC 7770 state");
        assert!(src.contains("TUIC_SESSION"), "guarded on TUIC_SESSION");
        assert!(src.contains("session.idle"));
        assert!(src.contains("permission.asked"));
    }

    #[test]
    fn agent_hook_opencode_install_then_state_installed() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("plugin/tuic.ts");
        assert_eq!(opencode_plugin_state(&path), InstallState::NotInstalled);
        install_opencode_plugin(&path).unwrap();
        assert!(path.exists());
        assert_eq!(opencode_plugin_state(&path), InstallState::Installed);
    }

    #[test]
    fn agent_hook_opencode_never_overwrites_unmarked_user_plugin() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("tuic.ts");
        std::fs::write(&path, b"// my own plugin\nexport const mine = 1;\n").unwrap();
        let err = install_opencode_plugin(&path);
        assert!(err.is_err(), "must refuse to overwrite an unmarked file");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "// my own plugin\nexport const mine = 1;\n",
            "user plugin left untouched"
        );
        // And state never claims a user plugin as ours.
        assert_eq!(opencode_plugin_state(&path), InstallState::NotInstalled);
    }

    #[test]
    fn agent_hook_opencode_uninstall_deletes_only_marked_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("tuic.ts");
        // Unmarked user file: uninstall must NOT delete it.
        std::fs::write(&path, b"// user plugin\n").unwrap();
        uninstall_opencode_plugin(&path).unwrap();
        assert!(path.exists(), "unmarked file preserved");
        // Our file: uninstall deletes it.
        install_opencode_plugin(&dir.path().join("ours.ts")).unwrap();
        let ours = dir.path().join("ours.ts");
        uninstall_opencode_plugin(&ours).unwrap();
        assert!(!ours.exists(), "marked file deleted");
    }

    #[test]
    fn agent_hook_opencode_reinstall_idempotent() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("tuic.ts");
        install_opencode_plugin(&path).unwrap();
        install_opencode_plugin(&path).unwrap(); // ours → allowed to rewrite
        assert_eq!(opencode_plugin_state(&path), InstallState::Installed);
    }
}
