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
    // Check • (U+2022) separately — Codex uses it for both spinner and output.
    // Only classify as chrome when it matches known chrome patterns.
    if is_codex_chrome_bullet(text) {
        return true;
    }
    for c in text.chars() {
        match c {
            '\u{23F5}'          // ⏵ — Claude Code mode-line prefix
            | '\u{23F8}'        // ⏸ — Claude Code plan mode prefix
            | '\u{203A}'        // › — Claude Code / Codex mode-line prefix
            | '\u{00B7}'        // · — Claude Code middle-dot spinner prefix
            | '\u{2580}'        // ▀ — Gemini prompt box top border
            | '\u{2584}'        // ▄ — Gemini prompt box bottom border
            | '\u{2591}'        // ░ — Aider Knight Rider spinner (light shade)
            | '\u{2588}'        // █ — Aider Knight Rider spinner (full block)
            | '\u{25A0}'        // ■ — Codex interrupt marker
            => return true,
            // Claude Code spinner dingbats (U+2720–U+273F): ✢✣✤...✻✼✽✾✿
            c if ('\u{2720}'..='\u{273F}').contains(&c) => return true,
            // Braille spinner chars (U+2800–U+28FF): ⠋⠙⠹⠸⠴⠦⠧⠇ — Gemini CLI
            c if ('\u{2800}'..='\u{28FF}').contains(&c) => return true,
            _ => {}
        }
    }
    false
}

/// Codex uses `•` (U+2022) for both chrome (spinner) and real output (action results).
/// Returns true only for known chrome patterns.
fn is_codex_chrome_bullet(text: &str) -> bool {
    let t = text.trim_start();
    let after = match t.strip_prefix('\u{2022}') {
        Some(rest) => rest.trim_start(),
        None => return false,
    };
    // Short suffixes (e.g. "Boot", "…") are always chrome
    if after.len() <= 5 {
        return true;
    }
    // Known chrome patterns: Working, Boot, esc/interrupt hints
    after.starts_with("Working") || after.starts_with("Boot")
        || after.contains("esc to") || after.contains("interrupt")
}

/// Find the row index where agent chrome starts (from the bottom).
///
/// Returns `Some(cutoff)` where `rows[0..cutoff]` are content and
/// `rows[cutoff..]` are chrome. Scans the last [`CHROME_SCAN_ROWS`] rows
/// for separator or prompt anchors, then extends the cutoff upward past
/// consecutive separators and empty lines.
///
/// Used by both the REST screen trim (session.rs) and mobile log trim (state.rs).
pub fn find_chrome_cutoff(rows: &[&str]) -> Option<usize> {
    if rows.is_empty() {
        return None;
    }

    // Trim trailing empty rows (terminal padding below content).
    let content_end = rows.iter().rposition(|r| !r.is_empty()).map_or(0, |i| i + 1);
    if content_end == 0 {
        return None;
    }

    let scan_start = content_end.saturating_sub(CHROME_SCAN_ROWS);

    // Strategy 1: find the lowest separator line.
    let separator_idx = (scan_start..content_end)
        .rev()
        .find(|&i| is_separator_line(rows[i].trim()));

    // Strategy 2: find the lowest prompt line.
    let prompt_idx = (scan_start..content_end)
        .rev()
        .find(|&i| is_prompt_line(rows[i]));

    // Use whichever anchor is higher (closer to content).
    let anchor = match (separator_idx, prompt_idx) {
        (Some(s), Some(p)) => Some(s.min(p)),
        (Some(s), None) => Some(s),
        (None, Some(p)) => Some(p),
        (None, None) => None,
    };

    let cutoff = match anchor {
        Some(mut idx) => {
            // Extend cutoff up past separators and empty lines above the anchor.
            while idx > 0 {
                let above = rows[idx - 1].trim();
                if above.is_empty() || is_separator_line(above) {
                    idx -= 1;
                } else {
                    break;
                }
            }
            idx
        }
        None => return None,
    };

    Some(cutoff)
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
    fn whitespace_padded_separator() {
        assert!(is_separator_line("  ──────────────  "));
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
        // · (U+00B7) middle dot CC spinner prefix
        assert!(is_chrome_row("· Proofing… (1m 14s · ↓ 1.6k tokens)"));
    }

    // Gemini prompt box borders (captured 2026-03-22)
    #[test]
    fn gemini_prompt_box_top() {
        assert!(is_chrome_row("▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀"));
    }

    #[test]
    fn gemini_prompt_box_bottom() {
        assert!(is_chrome_row("▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄"));
    }

    // Gemini braille spinner (captured 2026-03-22)
    #[test]
    fn gemini_braille_spinner() {
        assert!(is_chrome_row("⠴ Check tool-specific usage stats with /stats tools… (esc to cancel, 14s)"));
    }

    #[test]
    fn gemini_braille_spinner_short() {
        assert!(is_chrome_row("⠋ Connecting to MCP servers..."));
    }

    // Aider Knight Rider spinner (captured 2026-03-22)
    #[test]
    fn aider_knight_rider_1() {
        assert!(is_chrome_row("░█  Updating repo map: examples/plugins/repo-dashboard/main.js"));
    }

    #[test]
    fn aider_knight_rider_2() {
        assert!(is_chrome_row("█░  Waiting for openrouter/anthropic/claude-sonnet-4.5"));
    }

    // Codex interrupt marker (captured 2026-03-21)
    #[test]
    fn codex_interrupt() {
        assert!(is_chrome_row("■ Conversation interrupted - tell the model what to do differently."));
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
    fn codex_bullet_output_not_chrome() {
        // Codex uses • for regular output — should NOT be classified as chrome
        assert!(!is_chrome_row("• Created /tmp/codex-test.txt with hello."));
    }

    #[test]
    fn codex_bullet_added_not_chrome() {
        assert!(!is_chrome_row("• Added /tmp/file.txt (+1 -0)"));
    }

    #[test]
    fn codex_boot_spinner() {
        assert!(is_chrome_row("• Boot"));
    }

    // Claude Code status lines — now detected via █/░ block chars
    #[test]
    fn cc_status_context_bar() {
        assert!(is_chrome_row("  Context █░░░░░░░░░ 8% $0 (~$2.97) │ Usage ⚠ (429)"));
    }

    #[test]
    fn cc_wiz_hud_line() {
        assert!(!is_chrome_row("  5h: 42% (3h) | 7d: 27% (2d)"));
    }

    // Interactive menu footers — now detected as chrome via · (U+00B7)
    #[test]
    fn cc_menu_footer_cancel() {
        assert!(is_chrome_row("Esc to cancel · Tab to amend"));
    }

    #[test]
    fn cc_menu_footer_select() {
        assert!(is_chrome_row("Enter to select · Tab/Arrow keys to navigate · Esc to cancel"));
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

    // --- find_chrome_cutoff ---

    #[test]
    fn cutoff_cc_bypass_mode() {
        // CC bypass mode: content + empty + separator + prompt + separator + 2 status + mode
        // The empty line above separator is chrome padding, included in trim
        let rows: Vec<&str> = vec![
            "Here is the answer to your question.",
            "",
            "────────────────────────────────────────────────────────────────────────",
            "❯",
            "────────────────────────────────────────────────────────────────────────",
            "  [Opus 4.6 (1M context) | Max] │ tuicommander git:(main*)",
            "  ⏵⏵ bypass permissions on (shift+tab to cycle)",
        ];
        assert_eq!(find_chrome_cutoff(&rows), Some(1));
    }

    #[test]
    fn cutoff_cc_default_mode() {
        // CC default mode: content + empty + separator + prompt + separator + 1 status
        let rows: Vec<&str> = vec![
            "The project version is 0.9.5.",
            "",
            "────────────────────────────────────────────────────────────────────────",
            "❯",
            "────────────────────────────────────────────────────────────────────────",
            "  [Opus 4.6 (1M context) | Max] 3% | tuicommander git:(main*)",
        ];
        assert_eq!(find_chrome_cutoff(&rows), Some(1));
    }

    #[test]
    fn cutoff_codex_idle() {
        // Codex: content + prompt + status
        let rows: Vec<&str> = vec![
            "• Created /tmp/codex-test.txt with hello.",
            "› Summarize recent commits",
            "  gpt-5.4 high · 100% left · ~/Gits/personal/tuicommander",
        ];
        assert_eq!(find_chrome_cutoff(&rows), Some(1));
    }

    #[test]
    fn cutoff_aider_idle() {
        // Aider: content + token report + separator + file list + prompt
        let rows: Vec<&str> = vec![
            "The version is 0.9.5.",
            "Tokens: 8.0k sent, 106 received. Cost: $0.03 message, $0.06 session.",
            "─────────────────────────────────────────────────────────────────────",
            "package.json src-tauri/Cargo.toml",
            ">",
        ];
        assert_eq!(find_chrome_cutoff(&rows), Some(2));
    }

    #[test]
    fn cutoff_no_chrome() {
        let rows: Vec<&str> = vec![
            "Just plain text output.",
            "No chrome markers here.",
        ];
        assert_eq!(find_chrome_cutoff(&rows), None);
    }

    #[test]
    fn cutoff_empty() {
        let rows: Vec<&str> = vec![];
        assert_eq!(find_chrome_cutoff(&rows), None);
    }

    #[test]
    fn cutoff_all_empty_rows() {
        let rows: Vec<&str> = vec!["", "", ""];
        assert_eq!(find_chrome_cutoff(&rows), None);
    }

    #[test]
    fn cutoff_prompt_with_separator_above() {
        // Separator above prompt should be included in chrome
        let rows: Vec<&str> = vec![
            "Line of real output.",
            "",
            "────────────────────────────────────────────",
            "❯ hello",
        ];
        assert_eq!(find_chrome_cutoff(&rows), Some(1));
    }

    #[test]
    fn cutoff_prompt_outside_scan_window_ignored() {
        // 20 lines: prompt at index 2, scan window = last 15 = indices 5-19
        let mut items: Vec<&str> = vec!["content"; 20];
        items[2] = "> git diff output";
        assert_eq!(find_chrome_cutoff(&items), None);
    }

    #[test]
    fn cutoff_uses_bottom_most_prompt() {
        // Two prompts in scan window → bottom-most one wins
        let mut items: Vec<&str> = vec!["content"; 20];
        items[6] = "❯ earlier";
        items[15] = "❯ later";
        assert_eq!(find_chrome_cutoff(&items), Some(15));
    }

    // Gemini CLI bottom-zone tests (captured 2026-03-22, v0.34.0)

    #[test]
    fn cutoff_gemini_idle() {
        // Gemini idle: content + suggest + hint + separator + info + prompt box (3) + status (2)
        let rows: Vec<&str> = vec![
            "✦ The version of this project is 0.9.5.",
            "  [[suggest: View CHANGELOG.md | Check README.md]]",
            "                                                                  ? for shortcuts",
            "─────────────────────────────────────────────────────────────────────────────────",
            " Shift+Tab to accept edits                              1 MCP server | 3 skills",
            "▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀",
            " >   Type your message or @path/to/file",
            "▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▀▀",
            " workspace (/directory)          branch          sandbox              /model",
            " ~/Gits/personal/tuicommander   main            no sandbox           Auto (Gemini 3)",
        ];
        // Separator at index 3 is the anchor, walks up: index 2 is not empty/separator → cutoff=3
        assert_eq!(find_chrome_cutoff(&rows), Some(3));
    }

    #[test]
    fn cutoff_gemini_with_spinner() {
        // Gemini working: content + spinner + separator + info + prompt box (3) + status (2)
        let rows: Vec<&str> = vec![
            "✦ I will read the package.json file.",
            " ⠴ Check tool-specific usage stats… (esc to cancel, 14s)",
            "─────────────────────────────────────────────────────────────────────────────────",
            " Shift+Tab to accept edits",
            "▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀",
            " >   Type your message or @path/to/file",
            "▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▀▀",
            " workspace (/directory)          branch          sandbox              /model",
            " ~/Gits/personal/tuicommander   main            no sandbox           Auto (Gemini 3)",
        ];
        // Separator at index 2, spinner at index 1 is not separator/empty → cutoff=2
        assert_eq!(find_chrome_cutoff(&rows), Some(2));
    }

    #[test]
    fn cutoff_gemini_with_tool_box() {
        // Gemini with tool call box above chrome
        let rows: Vec<&str> = vec![
            "╭───────────────────────────────────────────────╮",
            "│ ✓  ReadFile package.json                       │",
            "│                                                │",
            "╰───────────────────────────────────────────────╯",
            "✦ The version is 0.9.5.",
            "  [[suggest: View CHANGELOG.md | Check README.md]]",
            "─────────────────────────────────────────────────────────────────────────────────",
            " Shift+Tab to accept edits                              1 MCP server | 3 skills",
            "▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀",
            " >   Type your message or @path/to/file",
            "▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▀▀",
            " workspace (/directory)          branch          sandbox              /model",
            " ~/Gits/personal/tuicommander   main            no sandbox           Auto (Gemini 3)",
        ];
        // Separator at index 6 → cutoff=6
        assert_eq!(find_chrome_cutoff(&rows), Some(6));
    }
}
