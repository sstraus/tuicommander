use serde::{Deserialize, Serialize};

/// Describes the current terminal mode — plain shell vs fullscreen TUI app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode")]
#[derive(Default)]
pub enum TerminalMode {
    /// Normal shell prompt — no alternate screen active.
    #[default]
    Shell,
    /// A fullscreen TUI application is running in the alternate screen buffer.
    FullscreenTui {
        /// Best-effort app identification (e.g. "vim", "htop"). None if unknown.
        #[serde(skip_serializing_if = "Option::is_none")]
        app_hint: Option<String>,
        /// Nesting depth — how many alt-screen pushes deep we are.
        depth: u8,
    },
}


impl TerminalMode {
    /// Process an alternate screen buffer enter (ESC[?1049h).
    /// Returns the new mode after the transition.
    pub fn on_alt_enter(&self) -> TerminalMode {
        match self {
            TerminalMode::Shell => TerminalMode::FullscreenTui {
                app_hint: None,
                depth: 1,
            },
            TerminalMode::FullscreenTui { depth, .. } => TerminalMode::FullscreenTui {
                app_hint: None,
                depth: depth.saturating_add(1),
            },
        }
    }

    /// Process an alternate screen buffer exit (ESC[?1049l).
    /// Returns the new mode after the transition.
    pub fn on_alt_exit(&self) -> TerminalMode {
        match self {
            TerminalMode::Shell => TerminalMode::Shell, // no-op
            TerminalMode::FullscreenTui { depth, .. } => {
                if *depth <= 1 {
                    TerminalMode::Shell
                } else {
                    TerminalMode::FullscreenTui {
                        app_hint: None,
                        depth: depth - 1,
                    }
                }
            }
        }
    }

    /// Set the app hint (detected from screen content).
    pub fn with_app_hint(&self, hint: String) -> TerminalMode {
        match self {
            TerminalMode::FullscreenTui { depth, .. } => TerminalMode::FullscreenTui {
                app_hint: Some(hint),
                depth: *depth,
            },
            other => other.clone(),
        }
    }

    /// Returns true if we're in a fullscreen TUI app.
    pub fn is_fullscreen(&self) -> bool {
        matches!(self, TerminalMode::FullscreenTui { .. })
    }

}

/// Known TUI application signatures matched against visible screen rows.
/// Each entry is (search_pattern, app_name).
const APP_SIGNATURES: &[(&str, &str)] = &[
    // Vim / Neovim — mode line or empty-line tildes
    ("-- INSERT --", "vim"),
    ("-- VISUAL --", "vim"),
    ("-- REPLACE --", "vim"),
    ("~                ", "vim"),
    // htop / btop
    ("CPU[", "htop"),
    ("Mem[", "htop"),
    ("Tasks:", "htop"),
    // lazygit
    ("Branches ─", "lazygit"),
    ("Commits ─", "lazygit"),
    ("Files ─", "lazygit"),
    // less / man
    ("Manual page ", "man"),
    (" line ", "less"), // less footer: "filename line N/M"
    ("(END)", "less"),
    // top
    ("load average:", "top"),
    ("PID ", "top"),
    // nano
    ("GNU nano", "nano"),
    // tmux — status bar
    ("[0] ", "tmux"),
];

/// Attempt to identify which TUI app is running based on visible screen rows.
/// Returns the app name if a signature matches, or None.
pub fn detect_app_from_rows(rows: &[impl AsRef<str>]) -> Option<&'static str> {
    for row in rows {
        let text = row.as_ref();
        for &(pattern, app) in APP_SIGNATURES {
            if text.contains(pattern) {
                return Some(app);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── TerminalMode transitions ───────────────────────────────

    #[test]
    fn shell_to_fullscreen_on_alt_enter() {
        let mode = TerminalMode::Shell.on_alt_enter();
        assert_eq!(
            mode,
            TerminalMode::FullscreenTui {
                app_hint: None,
                depth: 1,
            }
        );
    }

    #[test]
    fn fullscreen_to_shell_on_alt_exit() {
        let mode = TerminalMode::FullscreenTui {
            app_hint: Some("vim".into()),
            depth: 1,
        }
        .on_alt_exit();
        assert_eq!(mode, TerminalMode::Shell);
    }

    #[test]
    fn nested_alt_increments_depth() {
        let mode = TerminalMode::Shell.on_alt_enter().on_alt_enter();
        assert!(matches!(mode, TerminalMode::FullscreenTui { depth: 2, .. }));
    }

    #[test]
    fn nested_alt_decrements_depth() {
        let mode = TerminalMode::Shell
            .on_alt_enter()
            .on_alt_enter()
            .on_alt_exit();
        assert!(matches!(mode, TerminalMode::FullscreenTui { depth: 1, .. }));
    }

    #[test]
    fn nested_alt_returns_to_shell() {
        let mode = TerminalMode::Shell
            .on_alt_enter()
            .on_alt_enter()
            .on_alt_exit()
            .on_alt_exit();
        assert_eq!(mode, TerminalMode::Shell);
    }

    #[test]
    fn exit_from_shell_is_noop() {
        assert_eq!(TerminalMode::Shell.on_alt_exit(), TerminalMode::Shell);
    }

    #[test]
    fn depth_saturates_at_255() {
        let mut mode = TerminalMode::Shell;
        for _ in 0..300 {
            mode = mode.on_alt_enter();
        }
        assert!(matches!(mode, TerminalMode::FullscreenTui { depth: 255, .. }));
    }

    #[test]
    fn with_app_hint_sets_hint() {
        let mode = TerminalMode::Shell
            .on_alt_enter()
            .with_app_hint("vim".into());
        assert_eq!(
            mode,
            TerminalMode::FullscreenTui {
                app_hint: Some("vim".into()),
                depth: 1,
            }
        );
    }

    #[test]
    fn with_app_hint_on_shell_is_noop() {
        let mode = TerminalMode::Shell.with_app_hint("vim".into());
        assert_eq!(mode, TerminalMode::Shell);
    }

    // ── App detection from rows ────────────────────────────────

    #[test]
    fn detect_vim_insert_mode() {
        let rows = vec!["some code here", "-- INSERT --"];
        assert_eq!(detect_app_from_rows(&rows), Some("vim"));
    }

    #[test]
    fn detect_vim_tildes() {
        let rows = vec!["~                ", "~                "];
        assert_eq!(detect_app_from_rows(&rows), Some("vim"));
    }

    #[test]
    fn detect_htop() {
        let rows = vec!["  CPU[|||||||     50%]", "  Mem[||||       30%]"];
        assert_eq!(detect_app_from_rows(&rows), Some("htop"));
    }

    #[test]
    fn detect_lazygit() {
        let rows = vec!["┌ Branches ─────────┐", "│ main              │"];
        assert_eq!(detect_app_from_rows(&rows), Some("lazygit"));
    }

    #[test]
    fn detect_man() {
        let rows = vec!["Manual page git(1) line 1"];
        assert_eq!(detect_app_from_rows(&rows), Some("man"));
    }

    #[test]
    fn detect_less_end() {
        let rows = vec!["(END)"];
        assert_eq!(detect_app_from_rows(&rows), Some("less"));
    }

    #[test]
    fn detect_nano() {
        let rows = vec!["  GNU nano 7.2   file.txt"];
        assert_eq!(detect_app_from_rows(&rows), Some("nano"));
    }

    #[test]
    fn no_match_on_normal_shell() {
        let rows = vec![
            "$ ls -la",
            "total 42",
            "drwxr-xr-x  5 user staff  160 Apr 15 10:00 .",
            "-rw-r--r--  1 user staff 1024 Apr 15 10:00 file.txt",
        ];
        assert_eq!(detect_app_from_rows(&rows), None);
    }

    #[test]
    fn no_false_positive_on_git_output() {
        let rows = vec![
            "On branch main",
            "nothing to commit, working tree clean",
        ];
        assert_eq!(detect_app_from_rows(&rows), None);
    }

    #[test]
    fn no_false_positive_on_cargo_test() {
        let rows = vec![
            "running 32 tests",
            "test result: ok. 32 passed; 0 failed",
        ];
        assert_eq!(detect_app_from_rows(&rows), None);
    }

    // ── Serialization ──────────────────────────────────────────

    #[test]
    fn shell_serializes_correctly() {
        let json = serde_json::to_string(&TerminalMode::Shell).unwrap();
        assert_eq!(json, r#"{"mode":"Shell"}"#);
    }

    #[test]
    fn fullscreen_serializes_with_hint() {
        let mode = TerminalMode::FullscreenTui {
            app_hint: Some("vim".into()),
            depth: 1,
        };
        let json = serde_json::to_string(&mode).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["mode"], "FullscreenTui");
        assert_eq!(parsed["app_hint"], "vim");
        assert_eq!(parsed["depth"], 1);
    }

    #[test]
    fn fullscreen_without_hint_skips_field() {
        let mode = TerminalMode::FullscreenTui {
            app_hint: None,
            depth: 2,
        };
        let json = serde_json::to_string(&mode).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.get("app_hint").is_none());
    }

    #[test]
    fn roundtrip_serde() {
        let mode = TerminalMode::FullscreenTui {
            app_hint: Some("htop".into()),
            depth: 3,
        };
        let json = serde_json::to_string(&mode).unwrap();
        let back: TerminalMode = serde_json::from_str(&json).unwrap();
        assert_eq!(mode, back);
    }
}
