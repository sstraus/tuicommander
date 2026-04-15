//! Session knowledge store: per-session command outcomes, error
//! classification, error→fix correlation, and compact context summary
//! for injection into the agent loop.
//!
//! Pure data layer — no PTY hooks here. The ChunkProcessor wiring that
//! emits `CommandOutcome` records on OSC 133 `D` markers (or shell-state
//! transitions for shells without integration) is added separately.

#![allow(dead_code)]

use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};

use super::tui_detect::TerminalMode;

/// On-disk format version. Bumped when the JSON shape changes.
pub const KNOWLEDGE_SCHEMA_VERSION: u32 = 1;

/// Cap on stored commands per session. FIFO eviction beyond this.
pub const MAX_COMMANDS: usize = 2000;

/// How many subsequent successes count as a "fix" for a recent failure.
const FIX_CORRELATION_WINDOW: usize = 3;

/// Max entries kept in `cwd_history` (most-recent-first).
const MAX_CWD_HISTORY: usize = 50;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OutcomeClass {
    Success,
    Error { error_type: String },
    TuiLaunched { app_name: String },
    Timeout,
    UserCancelled,
    /// Outcome derived from heuristics (no OSC 133) — exit code may be missing.
    Inferred,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandOutcome {
    pub timestamp: u64,
    pub command: String,
    pub cwd: String,
    pub exit_code: Option<i32>,
    pub output_snippet: String,
    pub classification: OutcomeClass,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionKnowledge {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub commands: VecDeque<CommandOutcome>,
    /// error_type → list of commands that fixed it
    pub error_fix_pairs: HashMap<String, Vec<String>>,
    pub tui_apps_seen: HashSet<String>,
    /// (path, timestamp), most recent first.
    pub cwd_history: VecDeque<(String, u64)>,
    pub terminal_mode: TerminalMode,
}

fn default_schema_version() -> u32 {
    KNOWLEDGE_SCHEMA_VERSION
}

impl SessionKnowledge {
    pub fn new() -> Self {
        Self {
            schema_version: KNOWLEDGE_SCHEMA_VERSION,
            commands: VecDeque::new(),
            error_fix_pairs: HashMap::new(),
            tui_apps_seen: HashSet::new(),
            cwd_history: VecDeque::new(),
            terminal_mode: TerminalMode::Shell,
        }
    }

    /// Record a command outcome. Updates cwd history, TUI app set, and
    /// auto-correlates an error→fix pair when this success follows a
    /// recent failure within `FIX_CORRELATION_WINDOW` commands.
    pub fn record(&mut self, outcome: CommandOutcome) {
        // CWD history: dedup adjacent entries.
        if self
            .cwd_history
            .front()
            .map(|(p, _)| p != &outcome.cwd)
            .unwrap_or(true)
        {
            self.cwd_history
                .push_front((outcome.cwd.clone(), outcome.timestamp));
            while self.cwd_history.len() > MAX_CWD_HISTORY {
                self.cwd_history.pop_back();
            }
        }

        // TUI app launches.
        if let OutcomeClass::TuiLaunched { app_name } = &outcome.classification {
            self.tui_apps_seen.insert(app_name.clone());
        }

        // Error→fix correlation: a Success right after one or more recent
        // Errors marks those error_types as "fixed by" this command.
        if matches!(outcome.classification, OutcomeClass::Success) {
            let recent_errors: Vec<String> = self
                .commands
                .iter()
                .rev()
                .take(FIX_CORRELATION_WINDOW)
                .filter_map(|c| match &c.classification {
                    OutcomeClass::Error { error_type } => Some(error_type.clone()),
                    _ => None,
                })
                .collect();
            for err_type in recent_errors {
                self.error_fix_pairs
                    .entry(err_type)
                    .or_default()
                    .push(outcome.command.clone());
            }
        }

        self.commands.push_back(outcome);
        while self.commands.len() > MAX_COMMANDS {
            self.commands.pop_front();
        }
    }

    /// Compact text for LLM context (commands run, recent errors, cwd
    /// trail, TUI apps seen, current mode).
    pub fn build_context_summary(&self) -> String {
        let mut out = String::new();

        out.push_str(&format!(
            "## Session Knowledge\n\nMode: {}\n",
            mode_label(&self.terminal_mode)
        ));

        if !self.cwd_history.is_empty() {
            out.push_str("\n### Recent CWDs\n");
            for (path, _) in self.cwd_history.iter().take(5) {
                out.push_str(&format!("- {path}\n"));
            }
        }

        let recent_errors: Vec<&CommandOutcome> = self
            .commands
            .iter()
            .rev()
            .filter(|c| matches!(c.classification, OutcomeClass::Error { .. }))
            .take(5)
            .collect();
        if !recent_errors.is_empty() {
            out.push_str("\n### Recent Errors\n");
            for c in recent_errors {
                let etype = match &c.classification {
                    OutcomeClass::Error { error_type } => error_type.as_str(),
                    _ => "unknown",
                };
                out.push_str(&format!("- [{etype}] {}\n", c.command));
            }
        }

        if !self.error_fix_pairs.is_empty() {
            out.push_str("\n### Known Fixes\n");
            for (err, fixes) in &self.error_fix_pairs {
                if let Some(last) = fixes.last() {
                    out.push_str(&format!("- {err} → {last}\n"));
                }
            }
        }

        if !self.tui_apps_seen.is_empty() {
            let mut apps: Vec<&String> = self.tui_apps_seen.iter().collect();
            apps.sort();
            out.push_str(&format!(
                "\n### TUI Apps Seen\n{}\n",
                apps.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
            ));
        }

        out
    }
}

fn mode_label(m: &TerminalMode) -> String {
    match m {
        TerminalMode::Shell => "shell".to_string(),
        TerminalMode::FullscreenTui { app_hint, depth } => match app_hint {
            Some(app) => format!("fullscreen TUI ({app}, depth {depth})"),
            None => format!("fullscreen TUI (depth {depth})"),
        },
    }
}

// ---------------------------------------------------------------------------
// Error classification
// ---------------------------------------------------------------------------

/// Classify a command's stderr/stdout into a stable error_type string.
/// Returns `None` if no known pattern matches.
pub fn classify_error(output: &str) -> Option<String> {
    let lower = output.to_lowercase();

    // Order matters: check most specific first.
    if RUST_COMPILATION.iter().any(|p| output.contains(p)) {
        return Some("rust_compilation".into());
    }
    if NPM_ERROR.iter().any(|p| output.contains(p)) {
        return Some("npm_error".into());
    }
    if PYTHON_ERROR.iter().any(|p| output.contains(p)) {
        return Some("python_error".into());
    }
    if GO_ERROR.iter().any(|p| output.contains(p)) {
        return Some("go_error".into());
    }
    if MISSING_TOOL.iter().any(|p| lower.contains(p)) {
        return Some("missing_tool".into());
    }
    if MISSING_FILE.iter().any(|p| lower.contains(p)) {
        return Some("missing_file".into());
    }
    if PERMISSION.iter().any(|p| lower.contains(p)) {
        return Some("permission".into());
    }
    if NETWORK.iter().any(|p| lower.contains(p)) {
        return Some("network".into());
    }
    None
}

const RUST_COMPILATION: &[&str] =
    &["error[E", "error: could not compile", "cannot find type", "cannot find function"];
const NPM_ERROR: &[&str] = &["npm ERR!", "ERR_MODULE_NOT_FOUND", "Cannot find module"];
const PYTHON_ERROR: &[&str] = &[
    "Traceback (most recent call last)",
    "ModuleNotFoundError",
    "SyntaxError:",
    "NameError:",
];
const GO_ERROR: &[&str] = &["go: cannot find module", "undefined:", "syntax error:"];
const MISSING_TOOL: &[&str] = &["command not found", "is not recognized as", "not found in $path"];
const MISSING_FILE: &[&str] =
    &["no such file or directory", "cannot stat", "cannot find the file"];
const PERMISSION: &[&str] = &["permission denied", "operation not permitted", "eacces"];
const NETWORK: &[&str] = &[
    "could not resolve host",
    "connection refused",
    "network is unreachable",
    "timed out",
];

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

const SESSIONS_DIR: &str = "ai-sessions";

fn sessions_dir() -> Result<std::path::PathBuf, String> {
    let dir = crate::config::config_dir().join(SESSIONS_DIR);
    if !dir.exists() {
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create ai-sessions dir: {e}"))?;
    }
    Ok(dir)
}

pub fn persist(session_id: &str, knowledge: &SessionKnowledge) -> Result<(), String> {
    let dir = sessions_dir()?;
    let path = dir.join(format!("{session_id}.json"));
    let data = serde_json::to_string_pretty(knowledge)
        .map_err(|e| format!("Failed to serialize knowledge: {e}"))?;
    std::fs::write(&path, data).map_err(|e| format!("Failed to write knowledge: {e}"))
}

pub fn load(session_id: &str) -> Option<SessionKnowledge> {
    let dir = sessions_dir().ok()?;
    let path = dir.join(format!("{session_id}.json"));
    let data = std::fs::read_to_string(&path).ok()?;
    let mut k: SessionKnowledge = serde_json::from_str(&data).ok()?;
    if k.schema_version < KNOWLEDGE_SCHEMA_VERSION {
        k.schema_version = KNOWLEDGE_SCHEMA_VERSION;
    }
    Some(k)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(cmd: &str, ts: u64, class: OutcomeClass) -> CommandOutcome {
        CommandOutcome {
            timestamp: ts,
            command: cmd.into(),
            cwd: "/tmp".into(),
            exit_code: match class {
                OutcomeClass::Success => Some(0),
                OutcomeClass::Error { .. } => Some(1),
                _ => None,
            },
            output_snippet: String::new(),
            classification: class,
            duration_ms: 100,
        }
    }

    #[test]
    fn classify_rust_compilation_error() {
        let out = "error[E0425]: cannot find function `foo` in this scope";
        assert_eq!(classify_error(out).as_deref(), Some("rust_compilation"));
    }

    #[test]
    fn classify_npm_error() {
        assert_eq!(
            classify_error("npm ERR! code ENOENT").as_deref(),
            Some("npm_error")
        );
        assert_eq!(
            classify_error("Cannot find module 'foo'").as_deref(),
            Some("npm_error")
        );
    }

    #[test]
    fn classify_python_error() {
        let out = "Traceback (most recent call last):\n  File \"x.py\"\nModuleNotFoundError: No module named 'x'";
        assert_eq!(classify_error(out).as_deref(), Some("python_error"));
    }

    #[test]
    fn classify_missing_file() {
        assert_eq!(
            classify_error("ls: cannot access 'foo': No such file or directory").as_deref(),
            Some("missing_file")
        );
    }

    #[test]
    fn classify_permission() {
        assert_eq!(
            classify_error("bash: ./run.sh: Permission denied").as_deref(),
            Some("permission")
        );
    }

    #[test]
    fn classify_network() {
        assert_eq!(
            classify_error("curl: (6) Could not resolve host: example.com").as_deref(),
            Some("network")
        );
    }

    #[test]
    fn classify_missing_tool() {
        assert_eq!(
            classify_error("bash: foo: command not found").as_deref(),
            Some("missing_tool")
        );
    }

    #[test]
    fn classify_unknown_returns_none() {
        assert_eq!(classify_error("everything is fine"), None);
    }

    #[test]
    fn record_caps_at_max_commands() {
        let mut k = SessionKnowledge::new();
        for i in 0..MAX_COMMANDS + 50 {
            k.record(outcome(&format!("cmd{i}"), i as u64, OutcomeClass::Success));
        }
        assert_eq!(k.commands.len(), MAX_COMMANDS);
        // FIFO: oldest evicted, newest preserved
        assert_eq!(k.commands.front().unwrap().command, "cmd50");
        assert_eq!(
            k.commands.back().unwrap().command,
            format!("cmd{}", MAX_COMMANDS + 49)
        );
    }

    #[test]
    fn record_correlates_error_then_fix() {
        let mut k = SessionKnowledge::new();
        k.record(outcome(
            "cargo build",
            1,
            OutcomeClass::Error {
                error_type: "rust_compilation".into(),
            },
        ));
        k.record(outcome("vim src/lib.rs", 2, OutcomeClass::Success));
        k.record(outcome("cargo build", 3, OutcomeClass::Success));

        let fixes = k.error_fix_pairs.get("rust_compilation").unwrap();
        assert!(fixes.contains(&"vim src/lib.rs".to_string()));
        assert!(fixes.contains(&"cargo build".to_string()));
    }

    #[test]
    fn record_drops_correlation_outside_window() {
        let mut k = SessionKnowledge::new();
        k.record(outcome(
            "cargo build",
            1,
            OutcomeClass::Error {
                error_type: "rust_compilation".into(),
            },
        ));
        // 4 unrelated successes — pushes the error outside the window of 3
        for i in 0..4 {
            k.record(outcome(&format!("ls{i}"), 2 + i, OutcomeClass::Success));
        }
        let last_success_fixes = k
            .error_fix_pairs
            .get("rust_compilation")
            .map(|v| v.iter().any(|c| c == "ls3"))
            .unwrap_or(false);
        assert!(
            !last_success_fixes,
            "successes outside the correlation window must not register as fixes"
        );
    }

    #[test]
    fn record_dedups_adjacent_cwds() {
        let mut k = SessionKnowledge::new();
        let mut o = outcome("ls", 1, OutcomeClass::Success);
        o.cwd = "/a".into();
        k.record(o.clone());
        o.timestamp = 2;
        k.record(o.clone());
        o.cwd = "/b".into();
        o.timestamp = 3;
        k.record(o);

        assert_eq!(k.cwd_history.len(), 2);
        assert_eq!(k.cwd_history[0].0, "/b");
        assert_eq!(k.cwd_history[1].0, "/a");
    }

    #[test]
    fn record_collects_tui_apps() {
        let mut k = SessionKnowledge::new();
        k.record(outcome(
            "vim",
            1,
            OutcomeClass::TuiLaunched {
                app_name: "vim".into(),
            },
        ));
        k.record(outcome(
            "htop",
            2,
            OutcomeClass::TuiLaunched {
                app_name: "htop".into(),
            },
        ));
        assert!(k.tui_apps_seen.contains("vim"));
        assert!(k.tui_apps_seen.contains("htop"));
    }

    #[test]
    fn json_roundtrip_preserves_data() {
        let mut k = SessionKnowledge::new();
        k.record(outcome(
            "cargo build",
            1,
            OutcomeClass::Error {
                error_type: "rust_compilation".into(),
            },
        ));
        k.record(outcome("cargo build", 2, OutcomeClass::Success));
        k.terminal_mode = TerminalMode::FullscreenTui {
            app_hint: Some("vim".into()),
            depth: 1,
        };

        let json = serde_json::to_string(&k).unwrap();
        let loaded: SessionKnowledge = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.commands.len(), 2);
        assert_eq!(loaded.error_fix_pairs.len(), 1);
        assert_eq!(loaded.terminal_mode, k.terminal_mode);
        assert_eq!(loaded.schema_version, KNOWLEDGE_SCHEMA_VERSION);
    }

    #[test]
    fn missing_schema_version_loads_as_v1() {
        let json = r#"{
            "commands": [],
            "error_fix_pairs": {},
            "tui_apps_seen": [],
            "cwd_history": [],
            "terminal_mode": {"mode": "Shell"}
        }"#;
        let k: SessionKnowledge = serde_json::from_str(json).unwrap();
        assert_eq!(k.schema_version, 1);
    }

    #[test]
    fn build_context_summary_includes_recent_errors_and_fixes() {
        let mut k = SessionKnowledge::new();
        k.record(outcome(
            "cargo build",
            1,
            OutcomeClass::Error {
                error_type: "rust_compilation".into(),
            },
        ));
        k.record(outcome("cargo build", 2, OutcomeClass::Success));
        let mut o = outcome("ls", 3, OutcomeClass::Success);
        o.cwd = "/projects/foo".into();
        k.record(o);

        let s = k.build_context_summary();
        assert!(s.contains("Mode: shell"));
        assert!(s.contains("/projects/foo"));
        assert!(s.contains("rust_compilation"));
        assert!(s.contains("Known Fixes"));
    }

    #[test]
    fn build_context_summary_labels_fullscreen_mode() {
        let mut k = SessionKnowledge::new();
        k.terminal_mode = TerminalMode::FullscreenTui {
            app_hint: Some("vim".into()),
            depth: 1,
        };
        let s = k.build_context_summary();
        assert!(s.contains("fullscreen TUI (vim, depth 1)"));
    }
}
