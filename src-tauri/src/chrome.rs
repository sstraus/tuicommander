//! Shared agent UI chrome detection utilities.
//!
//! Canonical implementations of separator, prompt, and chrome-row detection
//! used by all three parsing pipelines (pty reader, REST screen trim, mobile
//! log trim). See `docs/architecture/agent-ui-analysis.md` for background.

/// Number of rows from the bottom to scan for agent chrome (prompt, separator,
/// status bar). Must accommodate the tallest observed bottom zone — Claude Code
/// with Wiz HUD uses ~12 rows, so 15 provides a safe margin.
pub const CHROME_SCAN_ROWS: usize = 15;

/// Returns true if `text` contains a run of 4+ box-drawing characters,
/// indicating a separator line.
///
/// Handles both plain separators (`────────`) and decorated ones with embedded
/// labels (`──── extractor ──`, `──── ■■■ Medium /model ────`).
///
/// Recognized box-drawing characters: `─ ━ ═ — ╌ ╍`.
pub fn is_separator_line(text: &str) -> bool {
    let mut run = 0u32;
    for c in text.chars() {
        if matches!(c, '─' | '━' | '═' | '—' | '╌' | '╍') {
            run += 1;
            if run >= 4 {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

/// Returns true if `text` looks like an agent prompt line.
///
/// Supports all known agent prompt characters:
/// - `❯` (U+276F) — Claude Code / Ink
/// - `›` (U+203A) — Codex CLI
/// - `> ` or bare `>` — Gemini CLI, generic
pub fn is_prompt_line(text: &str) -> bool {
    let t = text.trim_start();
    t.starts_with('❯') || t.starts_with('›') || t == ">" || t.starts_with("> ")
}

/// Returns true if a terminal row contains agent UI chrome (mode-line,
/// status-line, spinner) rather than real agent output.
///
/// Used to classify chunks as "chrome-only" when ALL changed rows are chrome,
/// which prevents chrome-only ticks from resetting the silence timer or
/// stamping `last_output_ms`.
///
/// Detected markers:
/// - `⏵` (U+23F5) — Claude Code mode-line prefix
/// - `⏸` (U+23F8) — Claude Code plan mode prefix
/// - `›` (U+203A) — Claude Code / Codex mode-line prefix
/// - `✻` (U+273B) — Claude Code timer marker (also covers ✶✳✢ via font rendering)
/// - `•` (U+2022) — Codex spinner / status indicator
pub fn is_chrome_row(text: &str) -> bool {
    for c in text.chars() {
        match c {
            '\u{23F5}'          // ⏵ — Claude Code mode-line prefix
            | '\u{23F8}'        // ⏸ — Claude Code plan mode prefix
            | '\u{203A}'        // › — Claude Code / Codex mode-line prefix
            | '\u{2022}'        // • — Codex spinner / status indicator
            => return true,
            // Claude Code spinner dingbats (U+2720–U+273F): ✢✣✤...✻✼✽✾✿
            c if ('\u{2720}'..='\u{273F}').contains(&c) => return true,
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- is_separator_line ---

    #[test]
    fn plain_separator() {
        assert!(is_separator_line("────────────────────────"));
    }

    #[test]
    fn decorated_separator_with_label() {
        assert!(is_separator_line("──────────────────────────────── extractor ──"));
    }

    #[test]
    fn decorated_separator_with_badge() {
        assert!(is_separator_line("──────── ■■■ Medium /model ────────"));
    }

    #[test]
    fn dotted_separator() {
        assert!(is_separator_line("╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌"));
    }

    #[test]
    fn short_run_not_separator() {
        assert!(!is_separator_line("───"));
    }

    #[test]
    fn no_box_chars() {
        assert!(!is_separator_line("just some text"));
    }

    #[test]
    fn empty_not_separator() {
        assert!(!is_separator_line(""));
    }

    // --- is_prompt_line ---

    #[test]
    fn claude_code_prompt() {
        assert!(is_prompt_line("❯ hello"));
    }

    #[test]
    fn claude_code_prompt_bare() {
        assert!(is_prompt_line("❯"));
    }

    #[test]
    fn codex_prompt() {
        assert!(is_prompt_line("› list files"));
    }

    #[test]
    fn gemini_prompt() {
        assert!(is_prompt_line("> yes"));
    }

    #[test]
    fn bare_gt() {
        assert!(is_prompt_line(">"));
    }

    #[test]
    fn indented_prompt() {
        assert!(is_prompt_line("  ❯ hello"));
    }

    #[test]
    fn plain_text_not_prompt() {
        assert!(!is_prompt_line("hello world"));
    }

    // --- is_chrome_row ---

    #[test]
    fn mode_line_bypass() {
        assert!(is_chrome_row("⏵⏵ bypass permissions on"));
    }

    #[test]
    fn mode_line_plan() {
        assert!(is_chrome_row("⏸ plan mode on (shift+tab to cycle)"));
    }

    #[test]
    fn timer_marker() {
        assert!(is_chrome_row("✻ Sautéed for 1m 19s"));
    }

    #[test]
    fn codex_spinner() {
        assert!(is_chrome_row("• Working (10s • esc to interrupt)"));
    }

    #[test]
    fn codex_mode_line() {
        assert!(is_chrome_row("›› bypass permissions on · 1 local agent"));
    }

    #[test]
    fn plain_text_not_chrome() {
        assert!(!is_chrome_row("This is agent output"));
    }

    #[test]
    fn status_line_not_chrome() {
        // CC status lines have no chrome markers — this is a known gap
        assert!(!is_chrome_row("[Opus 4.6 (1M context) | Max] │ tuicommander git:(main*)"));
    }

    // --- Real-world examples from live sessions (CC v2.1.81, Codex v0.116.0) ---

    // Claude Code mode lines (captured 2026-03-21)
    #[test]
    fn cc_mode_line_with_hint() {
        assert!(is_chrome_row("  ⏵⏵ bypass permissions on (shift+tab to cycle)"));
    }

    #[test]
    fn cc_mode_line_subprocess_new_format() {
        assert!(is_chrome_row("  1 shell · ⏵⏵ bypass permissions on"));
    }

    #[test]
    fn cc_mode_line_subprocess_only() {
        // "1 shell" without ⏵⏵ — known gap, not detected as chrome
        assert!(!is_chrome_row("  1 shell"));
    }

    #[test]
    fn cc_spinner_undulating() {
        assert!(is_chrome_row("✶ Undulating…"));
    }

    #[test]
    fn cc_spinner_with_tokens() {
        assert!(is_chrome_row("✳ Ideating… (1m 32s · ↓ 2.2k tokens)"));
    }

    #[test]
    fn cc_spinner_with_agent_count() {
        assert!(is_chrome_row("✻ Sautéed for 2m 9s · 1 local agent still running"));
    }

    #[test]
    fn cc_spinner_proofing() {
        // · (U+00B7) is NOT in the chrome markers — spinner with middle dot prefix
        // is detected via › or ✻ in the same chunk, not this specific char
        assert!(!is_chrome_row("· Proofing… (1m 14s · ↓ 1.6k tokens)"));
    }

    // Claude Code separators (captured 2026-03-21)
    #[test]
    fn cc_separator_with_extractor_label() {
        assert!(is_separator_line("───────────────────────────────────────────────────────── extractor ──"));
    }

    #[test]
    fn cc_separator_with_model_badge() {
        assert!(is_separator_line("──────── ■■■ Medium /model ────────"));
    }

    #[test]
    fn cc_permission_prompt_dotted_separator() {
        assert!(is_separator_line("╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌"));
    }

    // Claude Code prompts
    #[test]
    fn cc_prompt_empty() {
        assert!(is_prompt_line("❯"));
    }

    #[test]
    fn cc_permission_selection() {
        // ❯ used as selection indicator in permission prompt — matches as prompt (known)
        assert!(is_prompt_line(" ❯ 1. Yes"));
    }

    // Codex examples (captured 2026-03-21)
    #[test]
    fn codex_prompt_real() {
        assert!(is_prompt_line("› list files in the current directory"));
    }

    #[test]
    fn codex_spinner_working() {
        assert!(is_chrome_row("• Working (10s • esc to interrupt)"));
    }

    #[test]
    fn codex_bullet_output() {
        // Codex uses • for regular output too — this is a known false positive
        assert!(is_chrome_row("• Created /tmp/codex-test.txt with hello."));
    }

    // Claude Code status lines — NOT chrome (known gap)
    #[test]
    fn cc_status_context_bar() {
        assert!(!is_chrome_row("  Context █░░░░░░░░░ 8% $0 (~$2.97) │ Usage ⚠ (429)"));
    }

    #[test]
    fn cc_wiz_hud_line() {
        assert!(!is_chrome_row("  5h: 42% (3h) | 7d: 27% (2d)"));
    }

    // Interactive menu footers — NOT chrome markers
    #[test]
    fn cc_menu_footer_cancel() {
        assert!(!is_chrome_row("Esc to cancel · Tab to amend"));
    }

    #[test]
    fn cc_menu_footer_select() {
        assert!(!is_chrome_row("Enter to select · Tab/Arrow keys to navigate · Esc to cancel"));
    }

    // Codex separator between tool output and summary
    #[test]
    fn codex_tool_separator() {
        assert!(is_separator_line("───────────────────────────────────────────────────────────────────────────────────────────"));
    }

    // Codex status line — NOT a separator
    #[test]
    fn codex_status_not_separator() {
        assert!(!is_separator_line("  gpt-5.4 high · 100% left · ~/Gits/personal/tuicommander"));
    }
}
