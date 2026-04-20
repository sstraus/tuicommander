//! Builds the "Session Knowledge" section that gets appended to the agent
//! system prompt. Reads SessionKnowledge from AppState and (when present)
//! renders it as markdown, prefixed with a TUI warning when the terminal
//! is in fullscreen-app mode.

use crate::ai_agent::knowledge::SessionKnowledge;
use crate::ai_agent::tui_detect::TerminalMode;
use crate::state::AppState;

const TUI_WARNING: &str = "> ⚠️ Terminal is in fullscreen TUI mode — key sends may not reach the shell. Prefer `read_screen` + explicit keys (e.g. `q`, `ctrl+c`) over shell commands.";

/// Returns a cross-session memory section for injection into the agent's
/// system prompt, or `None` when no relevant prior-session data exists.
/// Scans all sessions whose cwd history overlaps the current session's repo root.
pub fn build_cross_session_section(state: &AppState, session_id: &str) -> Option<String> {
    let sandbox = state.file_sandboxes.get(session_id)?;
    let repo_path = sandbox.root().to_string_lossy().to_string();
    super::knowledge::summarize_for_repo(
        &state.session_knowledge,
        &repo_path,
        session_id,
        8_000,
    )
}

/// Returns a markdown-formatted knowledge section for injection into the
/// agent's system prompt, or `None` when no knowledge has been recorded.
pub fn build_knowledge_section(state: &AppState, session_id: &str) -> Option<String> {
    let entry = state.session_knowledge.get(session_id)?;
    let k = entry.lock();
    build_section_from_knowledge(&k)
}

fn build_section_from_knowledge(k: &SessionKnowledge) -> Option<String> {
    if is_empty(k) {
        return None;
    }
    let mut out = String::new();
    if matches!(k.terminal_mode, TerminalMode::FullscreenTui { .. }) {
        out.push_str(TUI_WARNING);
        out.push_str("\n\n");
    }
    out.push_str(&k.build_context_summary());
    Some(out)
}

fn is_empty(k: &SessionKnowledge) -> bool {
    k.commands.is_empty()
        && k.cwd_history.is_empty()
        && k.tui_apps_seen.is_empty()
        && k.error_fix_pairs.is_empty()
        && matches!(k.terminal_mode, TerminalMode::Shell)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_agent::knowledge::{CommandOutcome, OutcomeClass, SessionKnowledge};

    fn shell_outcome(cmd: &str, ts: u64, class: OutcomeClass) -> CommandOutcome {
        CommandOutcome {
            timestamp: ts,
            command: cmd.into(),
            cwd: "/".into(),
            exit_code: Some(0),
            output_snippet: String::new(),
            classification: class,
            duration_ms: 0,
            id: 0,
            semantic_intent: None,
        }
    }

    #[test]
    fn empty_knowledge_returns_none() {
        let k = SessionKnowledge::new();
        assert!(build_section_from_knowledge(&k).is_none());
    }

    #[test]
    fn populated_knowledge_returns_summary() {
        let mut k = SessionKnowledge::new();
        k.record(shell_outcome(
            "cargo build",
            1,
            OutcomeClass::Error {
                error_type: "rust_compilation".into(),
            },
        ));
        let s = build_section_from_knowledge(&k).expect("section");
        assert!(s.contains("## Session Knowledge"));
        assert!(s.contains("rust_compilation"));
        assert!(!s.contains(TUI_WARNING));
    }

    #[test]
    fn fullscreen_tui_prepends_warning() {
        let mut k = SessionKnowledge::new();
        k.terminal_mode = TerminalMode::FullscreenTui {
            app_hint: Some("vim".into()),
            depth: 1,
        };
        let s = build_section_from_knowledge(&k).expect("section");
        assert!(s.starts_with(TUI_WARNING));
        assert!(s.contains("fullscreen TUI (vim, depth 1)"));
    }

    #[test]
    fn only_tui_mode_change_is_enough_to_render() {
        let mut k = SessionKnowledge::new();
        k.terminal_mode = TerminalMode::FullscreenTui {
            app_hint: None,
            depth: 1,
        };
        assert!(build_section_from_knowledge(&k).is_some());
    }
}
