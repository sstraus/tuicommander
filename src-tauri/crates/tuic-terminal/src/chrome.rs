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

/// Returns true if `text` is an agent's *empty* interactive prompt row.
///
/// Stricter than [`is_prompt_line`] on both counts, because what it anchors is
/// deleted from history rather than from a screen that repaints next frame:
///
/// - The prompt glyph must carry **no text**. Agents echo the user's submitted
///   message back into the transcript on a prompt row (`❯ rename is broken`,
///   `› riprendiamo`) — that is the conversation, not chrome. The prompt box
///   that scrolls off is empty: the input cleared on submit.
/// - `> text` is a markdown blockquote, so only a bare `>` qualifies.
pub fn is_agent_prompt_row(text: &str) -> bool {
    let t = text.trim();
    let rest = match t.strip_prefix(['❯', '›', '>']) {
        Some(rest) => rest,
        None => return false,
    };
    rest.trim().is_empty()
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
            | '\u{25D0}'        // ◐ — Claude Code tool progress spinner
            | '\u{25D1}'        // ◑ — Claude Code tool progress spinner
            | '\u{25D2}'        // ◒ — Claude Code tool progress spinner
            | '\u{25D3}'        // ◓ — Claude Code tool progress spinner
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

/// Returns true when the row is an agent's animated working spinner, proving the
/// agent is alive. This is a subset of `is_chrome_row`: mode-line prefixes
/// (⏵ ⏸ ›), box borders (▀ ▄), and interrupt markers (■) are static chrome
/// and return false here.
///
/// A spinner is matched STRUCTURALLY: the LEADING glyph of the (trimmed) line
/// must be a spinner character. Every supported agent renders its spinner at the
/// very start of the status row — `✻ Cogitating…`, `· Proofing…`, `⠴ Reading…`,
/// Aider's `█░ Waiting…`. Requiring the glyph to LEAD the line (not merely appear
/// somewhere) is what keeps a status-line HUD (`[Opus] ██░░ 17% · …`), a welcome
/// banner (`│ ▐▛███▜▌ │ … · …`), or prose that happens to contain a `·`/block
/// from being read as a live spinner — the regression that pinned Claude BUSY
/// forever under a wiz status bar whose progress bar ticks every second
/// (#446-596f). Chasing individual glyphs is whack-a-mole; the position is the
/// invariant.
pub fn is_spinner_row(text: &str) -> bool {
    // junie's idle status-bar effort icon (◐◑◒◓) is static chrome, not an animated
    // spinner — suppress it here so junie's idle state is detected (otherwise the
    // silence/idle path treats junie as perpetually busy). See is_junie_status_bar.
    if is_junie_status_bar(text) {
        return false;
    }
    if is_codex_chrome_bullet(text) {
        return true;
    }
    // The spinner must LEAD the line. `•`/`◦` (Codex) are handled above.
    let lead = match text.trim_start().chars().next() {
        Some(c) => c,
        None => return false,
    };
    // A lone block glyph is a scrollbar thumb, not a spinner: grok paints one per row down
    // the right edge as soon as its output outgrows the viewport, so every otherwise-blank
    // row trimmed down to a single `█` and the session never left BUSY. Aider's Knight Rider
    // bar always carries more cells plus its status text, so it is unaffected.
    if matches!(lead, '\u{2588}' | '\u{2591}') && text.trim().chars().count() == 1 {
        return false;
    }
    matches!(lead,
        '\u{00B7}'        // · — Claude Code middle-dot spinner prefix
        | '\u{2591}'      // ░ — Aider Knight Rider spinner (light shade)
        | '\u{2588}'      // █ — Aider Knight Rider spinner (full block)
        | '\u{25D0}'      // ◐ — Claude Code tool progress spinner
        | '\u{25D1}'      // ◑ — Claude Code tool progress spinner
        | '\u{25D2}'      // ◒ — Claude Code tool progress spinner
        | '\u{25D3}'      // ◓ — Claude Code tool progress spinner
    )
        // Claude Code spinner dingbats (U+2720–U+273F): ✢✣✤...✻✼✽✾✿
        || ('\u{2720}'..='\u{273F}').contains(&lead)
        // Braille spinner chars (U+2800–U+28FF): ⠋⠙⠹⠸⠴⠦⠧⠇ — Gemini CLI
        || ('\u{2800}'..='\u{28FF}').contains(&lead)
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
    after.starts_with("Working")
        || after.starts_with("Boot")
        || after.contains("esc to")
        || after.contains("interrupt")
}

/// True if the row is an agent "working" status line that stays on screen for
/// the WHOLE time the agent is busy — even while a spawned subprocess runs and
/// the TUI is frozen (no grid changes at all).
///
/// Codex renders `• Working (12s • esc to interrupt)` (the blink spinner
/// alternates `•` U+2022 / `◦` U+25E6) and then FREEZES its UI while a child
/// process (cargo, git) runs — a long `cargo build` is minutes with zero grid
/// changes. The change-driven spinner keepalive (`is_spinner_row` over
/// `changed_rows`) cannot see a frozen line, so the idle timer needs a
/// PRESENCE-driven signal: while this line sits in the content zone the agent
/// is alive, regardless of whether it changed this tick.
///
/// The verb is NOT part of the signature: Codex renders
/// `<Verb…> (<elapsed> • esc to interrupt)` and swaps the verb per phase
/// (`Working`, `Waiting for background terminal`, …). Keying on the verb made
/// every background-terminal phase read as idle. The invariant is the
/// interrupt hint — Codex only offers `esc` while a turn is running — anchored
/// on its closing paren so prose that merely mentions pressing esc (or a plain
/// `• …` output bullet / the `• Boot` startup line) never matches.
pub fn is_working_status_row(text: &str) -> bool {
    text.contains("esc to interrupt)")
}

/// junie (JetBrains) renders a persistent idle status bar whose "effort"
/// indicator uses a partial-circle glyph (◐◑◒◓, U+25D0–U+25D3) — the SAME glyphs
/// Claude Code uses for its animated tool-progress spinner. Without
/// disambiguation, junie's *static* bar reads as a live spinner, so junie is
/// treated as perpetually busy and the idle notification never fires.
///
/// The junie status bar has a distinctive signature, e.g.:
///   `~ tuicommander   ⚑ Brave off ctrl + b   ⌘ Grok 4.3 OpenRouter   ◐ Medium effort`
/// Detect it by the command/flag markers (⌘ U+2318 / ⚑ U+2691) plus the effort
/// label so the circle glyph in this row is not classified as an animated spinner.
fn is_junie_status_bar(text: &str) -> bool {
    text.contains("effort") && (text.contains('\u{2318}') || text.contains('\u{2691}'))
}

/// Returns true if a row is part of Claude Code's Ink-rendered task list.
///
/// Detected markers:
/// - `⎿` (U+23BF) — sub-tree bracket (task container, e.g. "⎿  4 tasks (1 done, 3 open)")
/// - `◻` (U+25FB) — pending task checkbox
/// - `✔` (U+2714) — completed task checkbox
///
/// Used only by `find_chrome_cutoff` to extend the trim zone upward past task
/// rows — does NOT affect `is_chrome_row` or shell-state detection.
fn is_task_list_row(text: &str) -> bool {
    for c in text.chars() {
        match c {
            '\u{23BF}' | '\u{25FB}' | '\u{2714}' => return true,
            _ => {}
        }
    }
    false
}

/// Locate ego's native composer, never a historical submitted `> prompt`.
/// During output insertion the old composer can be overwritten before its next
/// repaint. In that gap there is no chrome anchor, rather than an earlier one.
/// Returns the prompt row and whether its status reports active work.
pub fn find_ego_composer(rows: &[impl AsRef<str>]) -> Option<(usize, bool)> {
    let prompt = rows.iter().rposition(|row| {
        let row = row.as_ref();
        row.starts_with("> ") || row.trim_end() == ">"
    })?;
    if prompt == 0
        || rows[prompt + 1..]
            .iter()
            .any(|row| !row.as_ref().trim().is_empty())
    {
        return None;
    }
    let status = rows[prompt - 1].as_ref().trim_end();
    let working = ["◐", "◓", "◑", "◒"].iter().any(|frame| {
        status
            .strip_prefix(frame)
            .and_then(|rest| rest.strip_prefix(" Working "))
            .and_then(|rest| rest.strip_suffix("s · Esc to interrupt"))
            .is_some_and(|seconds| seconds.parse::<u64>().is_ok())
    });
    let legacy_idle = status.contains(" · ctx ") && status.contains(" · trust=");
    // The compact status uses a ten-cell context bar instead of the old ctx label.
    let compact_idle = status.split_whitespace().any(|field| {
        field.chars().count() == 10 && field.chars().all(|cell| matches!(cell, '▰' | '▱'))
    }) && status
        .split_whitespace()
        .any(|field| field.starts_with("trust="));
    (working || legacy_idle || compact_idle).then_some((prompt, working))
}

/// Agent-specific chrome uses the same native composer shape as readiness.
/// Other agents retain the existing shared separator/prompt grammar.
pub fn find_agent_chrome_cutoff(rows: &[&str], agent_type: Option<&str>) -> Option<usize> {
    if agent_type == Some("ego") {
        return find_ego_composer(rows).map(|(prompt, _)| prompt - 1);
    }
    find_chrome_cutoff(rows)
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

    // An open Claude Ink dialog owns the bottom of the screen. Its highlighted
    // option begins with the same ❯ glyph as the composer, and its rule looks
    // like a composer separator. Keep the whole dialog for mobile clients.
    if rows.iter().any(|row| row.starts_with("Enter to select")) {
        return None;
    }

    // Trim trailing empty rows (terminal padding below content).
    let content_end = rows
        .iter()
        .rposition(|r| !r.is_empty())
        .map_or(0, |i| i + 1);
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
    }
    // Nothing in the scan window: the bottom zone is taller than
    // [`CHROME_SCAN_ROWS`], which happens as soon as the user's status line is.
    // Its height is not ours to predict — it is a `statusLine` command, a HUD
    // plugin, a shell theme; it may be twenty rows or absent. Falling through
    // here returns None, and None means NO trim, so every status-line row would
    // then reach every parser. Failing open is the one outcome this guard must
    // never have, so search the whole screen for the input box itself.
    .or_else(|| lowest_input_box_row(rows, content_end))
    .or_else(|| lowest_structured_input_box(rows, content_end).map(|(separator, _)| separator));
    // DEFERRED (2026-08-22) — the fallbacks above still leave this returning None
    // often. Measured with `detection_over_capture_corpus` (see docs/backend/pty.md)
    // over a live corpus: 81% and 71% of content ticks found no anchor on two
    // agent captures that do emit status-line events, against 8% on a third.
    // A shell session legitimately has no chrome, so a high rate is not proof of
    // harm — it is unknown whether the untrimmed rows on those ticks carry
    // anything a parser then misreads. Needs the changed rows sampled at the
    // no-anchor ticks before anything here is widened; widening blind is how the
    // loose `is_prompt_line` search starts matching markdown blockquotes.

    Some(extend_cutoff_upward(rows, anchor?))
}

/// Lowest row that can only be an agent's *empty* input box, searched without a
/// window.
///
/// Anchors on [`is_agent_prompt_row`] rather than [`is_prompt_line`]: unwindowed,
/// the loose form would match a markdown blockquote or an echoed user message
/// somewhere up in the conversation and cut the screen in half. An empty prompt
/// glyph carrying no text exists only in the input box.
fn lowest_input_box_row(rows: &[&str], content_end: usize) -> Option<usize> {
    (0..content_end)
        .rev()
        .find(|&i| is_agent_prompt_row(rows[i]))
}

/// Lowest separator that is followed immediately by an agent prompt.
///
/// This is the unwindowed fallback for a non-empty input box hidden above a
/// tall user-configured HUD. A separator alone is too common in transcript
/// content, and an unwindowed loose prompt would match markdown quotes; their
/// bounded structural pairing is the input-box invariant.
fn lowest_structured_input_box(rows: &[&str], content_end: usize) -> Option<(usize, usize)> {
    (0..content_end).rev().find_map(|separator| {
        if !is_separator_line(rows[separator].trim()) {
            return None;
        }
        rows.iter()
            .take(content_end)
            .enumerate()
            .skip(separator + 1)
            .take(4)
            .find(|(_, row)| is_prompt_line(row))
            .map(|(prompt, _)| (separator, prompt))
    })
}

/// Locate the live prompt using only unwindowed input-box invariants.
///
/// Unlike [`find_chrome_cutoff`], this never accepts a loose prompt merely
/// because it is near the viewport bottom: that shape can be transcript prose.
/// It is safe for readiness classifiers that must not treat historical input as
/// the current composer.
pub fn find_input_box_prompt_row(rows: &[&str]) -> Option<usize> {
    let content_end = rows
        .iter()
        .rposition(|row| !row.trim().is_empty())
        .map_or(0, |index| index + 1);
    let empty_prompt = lowest_input_box_row(rows, content_end);
    let structured_prompt =
        lowest_structured_input_box(rows, content_end).map(|(_, prompt)| prompt);
    match (empty_prompt, structured_prompt) {
        (Some(empty), Some(structured)) => Some(empty.max(structured)),
        (Some(prompt), None) | (None, Some(prompt)) => Some(prompt),
        (None, None) => None,
    }
}

/// Cutoff at an agent's *empty* input box, for trimming a screen handed to a
/// reader rather than a parser.
///
/// Anchors only on [`is_agent_prompt_row`], never on a loose prompt: a dialog's
/// `❯ 1. Yes` carries text, so an open permission or choice dialog — the reason
/// an orchestrator reads a stuck child's screen — is never cut, and neither is
/// a draft in the input box. Everything from the input box down is the user's
/// status line and agent chrome.
pub fn find_empty_input_box_cutoff(rows: &[&str]) -> Option<usize> {
    let content_end = rows
        .iter()
        .rposition(|r| !r.trim().is_empty())
        .map_or(0, |i| i + 1);
    // An empty prompt counts only inside the input box frame: the nearest
    // non-blank row above it is a separator. A blockquote's blank line is a
    // bare `>` too, and anchoring on it cut a draft or dialog below (#867-c877).
    let anchor = (0..content_end).rev().find(|&i| {
        is_agent_prompt_row(rows[i])
            && rows[..i]
                .iter()
                .rev()
                .find(|r| !r.trim().is_empty())
                .is_some_and(|r| is_separator_line(r.trim()))
    })?;
    Some(extend_cutoff_upward(rows, anchor))
}

/// Find the row index where agent chrome starts in a batch of lines that
/// scrolled off into **history**.
///
/// Same contract as [`find_chrome_cutoff`] but deliberately conservative,
/// because what it cuts is gone from the user's scrollback for good:
///
/// - A separator alone is **not** an anchor. Tool output, tables, progress bars
///   and Codex's `└ ────` dividers all carry box-drawing runs; anchoring on them
///   truncated real prose mid-sentence (issue: mobile log showed paragraphs
///   starting on their second line).
/// - The prompt must be a real agent prompt row ([`is_agent_prompt_row`]), not
///   any line opening with `> `.
///
/// Separators, blank rows and task-list rows still extend the cut upward once a
/// genuine prompt anchors it — that is the prompt box, not content.
pub fn find_scrollback_chrome_cutoff(rows: &[&str]) -> Option<usize> {
    if rows.is_empty() {
        return None;
    }

    let content_end = rows
        .iter()
        .rposition(|r| !r.is_empty())
        .map_or(0, |i| i + 1);
    if content_end == 0 {
        return None;
    }

    let scan_start = content_end.saturating_sub(CHROME_SCAN_ROWS);
    let anchor = (scan_start..content_end)
        .rev()
        .find(|&i| is_agent_prompt_row(rows[i]))?;

    Some(extend_cutoff_upward(rows, anchor))
}

/// Extend a chrome cutoff up past separators, empty lines, and task list rows
/// (⎿ ◻ ✔) sitting above the anchor.
///
/// `is_chrome_row` is deliberately NOT included: spinners above the separator
/// are agent output indicators (e.g. Gemini braille), not footer chrome.
fn extend_cutoff_upward(rows: &[&str], anchor: usize) -> usize {
    let mut cutoff = anchor;
    while cutoff > 0 {
        let above = rows[cutoff - 1].trim();
        if above.is_empty() || is_separator_line(above) || is_task_list_row(above) {
            cutoff -= 1;
        } else {
            break;
        }
    }
    cutoff
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ego_transient_output_does_not_trim_at_a_submitted_prompt() {
        let rows = [
            "> old submitted task",
            "",
            "intent: new wrapped task",
            "(Latest task)",
        ];
        assert_eq!(find_agent_chrome_cutoff(&rows, Some("ego")), None);
    }

    #[test]
    fn ego_draft_composer_keeps_protocol_looking_input_out_of_content() {
        let rows = [
            "intent: real output (Real)",
            "model · ctx 1% · trust=trusted",
            "> intent: retained draft (Draft)",
        ];
        assert_eq!(find_agent_chrome_cutoff(&rows, Some("ego")), Some(1));
    }

    #[test]
    fn ego_working_composer_keeps_status_and_prompt_out_of_content() {
        let rows = [
            "intent: real output (Real)",
            "◐ Working 12s · Esc to interrupt",
            "> ",
        ];
        assert_eq!(find_agent_chrome_cutoff(&rows, Some("ego")), Some(1));
    }

    // --- Bottom zone: the input area and everything under it ---
    //
    // Below the agent's input box sits a status line the USER configures — a
    // Claude Code `statusLine` command, a wiz HUD, a shell prompt theme. Its
    // height, glyphs and text are arbitrary and differ per install; it may not
    // be there at all. It is never agent output, and nothing in it may be
    // parsed: a path in it would read as a plan file, a `?` as a question, a
    // number as a token count. The cutoff is what enforces that, so it has to
    // hold for a status line of ANY height.

    /// Boss's real screen: input box, then a four-line custom HUD.
    fn screen_with_status_line(hud_lines: usize) -> Vec<String> {
        let mut rows = vec![
            "  Here is some real agent output.".to_string(),
            "  A second line of it.".to_string(),
            String::new(),
            "✻ Simmering… (5m 48s · ↓ 20.7k tokens)".to_string(),
            String::new(),
            "─".repeat(120),
            "❯ ".to_string(),
            "─".repeat(120),
        ];
        for i in 0..hud_lines {
            rows.push(format!(
                "  [Opus 5 (1M) | Team] ██░░ 22% | 📚 8 | line {i} | plans/notes.md | ready?"
            ));
        }
        rows.push("  ⏵⏵ bypass permissions on (shift+tab to cycle)".to_string());
        rows
    }

    fn cutoff_of(rows: &[String]) -> Option<usize> {
        let refs: Vec<&str> = rows.iter().map(|s| s.as_str()).collect();
        find_chrome_cutoff(&refs)
    }

    fn empty_box_cutoff_of(rows: &[String]) -> Option<usize> {
        let refs: Vec<&str> = rows.iter().map(|s| s.as_str()).collect();
        find_empty_input_box_cutoff(&refs)
    }

    /// A reader of the screen gets the agent's output and its spinner, not the
    /// empty input box or the status line under it, whatever that line's height.
    #[test]
    fn empty_input_box_cutoff_drops_the_box_and_everything_below() {
        for hud_lines in [0, 4, 30] {
            let rows = screen_with_status_line(hud_lines);
            let cutoff = empty_box_cutoff_of(&rows).expect("empty input box must anchor");
            assert_eq!(
                rows[..cutoff].last().map(String::as_str),
                Some("✻ Simmering… (5m 48s · ↓ 20.7k tokens)"),
                "hud_lines={hud_lines}"
            );
        }
    }

    /// An open dialog is what an orchestrator reads a stuck child's screen for.
    /// `❯ 1. Yes` carries text, so it is not an input box and nothing is cut.
    #[test]
    fn empty_input_box_cutoff_never_cuts_an_open_dialog() {
        let mut rows = vec![
            "  Some output.".to_string(),
            "─".repeat(120),
            " Bash command".to_string(),
            "   rm -rf target".to_string(),
            " Do you want to proceed?".to_string(),
            " ❯ 1. Yes".to_string(),
            "   2. No".to_string(),
            " Esc to cancel · Tab to amend".to_string(),
        ];
        rows.push("  [Opus 5 (1M) | Team] ██░░ 22%".to_string());
        assert_eq!(empty_box_cutoff_of(&rows), None);
    }

    /// A draft in the input box is state the reader may need; with no empty
    /// prompt there is no anchor and the screen is returned whole.
    #[test]
    fn empty_input_box_cutoff_keeps_a_draft() {
        let mut rows = screen_with_status_line(4);
        let prompt = rows.iter().position(|r| r == "❯ ").unwrap();
        rows[prompt] = "❯ fix the rename test".to_string();
        assert_eq!(empty_box_cutoff_of(&rows), None);
    }

    /// A blank line inside a markdown blockquote renders as a bare `>`. Above a
    /// draft it must not become the anchor, or the draft is cut (#867-c877).
    #[test]
    fn empty_input_box_cutoff_ignores_a_blockquote_blank_line_above_a_draft() {
        let mut rows = screen_with_status_line(4);
        let prompt = rows.iter().position(|r| r == "❯ ").unwrap();
        rows[prompt] = "❯ fix the rename test".to_string();
        rows.splice(
            0..0,
            [
                "  > quoted line".to_string(),
                "  >".to_string(),
                "  > more".to_string(),
            ],
        );
        assert_eq!(empty_box_cutoff_of(&rows), None);
    }

    /// The same bare `>` above an open dialog must not cut the dialog (#867-c877).
    #[test]
    fn empty_input_box_cutoff_ignores_a_blockquote_blank_line_above_a_dialog() {
        let rows: Vec<String> = [
            "  > quoted line",
            "  >",
            "  > more",
            "─".repeat(120).as_str(),
            " Bash command",
            " Do you want to proceed?",
            " ❯ 1. Yes",
            "   2. No",
            " Esc to cancel · Tab to amend",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(empty_box_cutoff_of(&rows), None);
    }

    /// A status line taller than the scan window used to leave the cutoff
    /// unfound, and an unfound cutoff means NO trimming at all — every HUD row
    /// then reaches every parser. The failure is silent and total, which is the
    /// worst shape a guard can fail in.
    #[test]
    fn cutoff_survives_a_status_line_of_any_height() {
        for hud_lines in [0, 3, 4, 12, 30] {
            let rows = screen_with_status_line(hud_lines);
            let cutoff = cutoff_of(&rows)
                .unwrap_or_else(|| panic!("no cutoff found with {hud_lines} HUD lines"));

            // 4, not 5: the blank row above the separator is padding of the
            // input box, eaten by `extend_cutoff_upward`.
            assert_eq!(
                cutoff, 4,
                "cutoff must land on the input box with {hud_lines} HUD lines"
            );
            assert!(
                rows[..cutoff]
                    .iter()
                    .any(|r| r.contains("real agent output")),
                "content above the input box must survive the trim"
            );
            assert!(
                rows[..cutoff].iter().any(|r| r.contains("Simmering")),
                "the agent's own spinner sits ABOVE the input box and must survive — \
                 it is the busy signal"
            );
            assert!(
                !rows[..cutoff].iter().any(|r| r.contains("plans/notes.md")),
                "no HUD row may reach a parser"
            );
        }
    }

    #[test]
    fn cutoff_survives_tall_hud_while_prompt_contains_draft_text() {
        let mut rows = vec![
            "  Agent output that must remain visible.".to_string(),
            String::new(),
            "─".repeat(100),
            "› Run /review on my current changes".to_string(),
            "─".repeat(100),
        ];
        rows.extend((0..30).map(|i| format!("custom HUD row {i}: ready?")));

        assert_eq!(cutoff_of(&rows), Some(1));
    }

    /// The extended search anchors on an EMPTY prompt row only. A markdown
    /// blockquote or an echoed prompt carrying text is conversation, and
    /// anchoring on it would cut real output out of the screen.
    #[test]
    fn extended_search_does_not_anchor_on_quoted_prose() {
        let mut rows: Vec<String> = vec!["> a quoted line from the user".to_string()];
        rows.extend((0..40).map(|i| format!("  output line {i}")));
        assert_eq!(
            cutoff_of(&rows),
            None,
            "prose alone must not produce a cutoff"
        );
        let refs: Vec<&str> = rows.iter().map(String::as_str).collect();
        assert_eq!(find_input_box_prompt_row(&refs), None);
    }

    #[test]
    fn structured_input_box_returns_its_prompt_row_above_tall_hud() {
        let mut rows = vec![
            "agent output".to_string(),
            "─".repeat(80),
            "› draft request".to_string(),
            "─".repeat(80),
        ];
        rows.extend((0..30).map(|i| format!("HUD row {i}")));
        let refs: Vec<&str> = rows.iter().map(String::as_str).collect();

        assert_eq!(find_input_box_prompt_row(&refs), Some(2));
    }

    // --- is_working_status_row (presence-driven busy keepalive) ---

    #[test]
    fn codex_working_line_is_working_status() {
        assert!(is_working_status_row(
            "• Working (9m 24s • esc to interrupt)"
        ));
    }

    #[test]
    fn codex_working_blink_frame_is_working_status() {
        // The blink spinner alternates • (U+2022) and ◦ (U+25E6); both frames
        // must register or the keepalive drops on every other tick.
        assert!(is_working_status_row("◦ Working (12s • esc to interrupt)"));
    }

    #[test]
    fn codex_working_indented_is_working_status() {
        assert!(is_working_status_row(
            "   • Working (1m 3s • esc to interrupt)"
        ));
    }

    #[test]
    fn codex_background_terminal_wait_is_working_status() {
        // Codex swaps the verb while a background terminal runs; the turn is
        // still interruptible, so the session must stay busy. Keying on
        // "Working" made every `rtk cargo …` background phase read as idle.
        assert!(is_working_status_row(
            "• Waiting for background terminal (41s • esc to interrupt) · 1 background terminal running · /ps to view · …"
        ));
    }

    #[test]
    fn codex_working_line_with_background_suffix_is_working_status() {
        assert!(is_working_status_row(
            "• Working (1m 39s • esc to interrupt) · 1 background terminal running · /ps to view · /stop to close"
        ));
    }

    #[test]
    fn codex_completed_background_wait_is_not_working_status() {
        // Past tense, no interrupt hint — the wait is over, this is transcript.
        assert!(!is_working_status_row(
            "• Waited for background terminal · rtk cargo fmt --all"
        ));
    }

    #[test]
    fn codex_output_bullet_is_not_working_status() {
        // Plain action-result bullets must NOT hold the agent busy forever.
        assert!(!is_working_status_row("• Added deny.toml (+95 -0)"));
        assert!(!is_working_status_row("• Ran cargo test -p proxy-min"));
    }

    #[test]
    fn codex_boot_line_is_not_working_status() {
        // Startup line has no "esc to interrupt" hint.
        assert!(!is_working_status_row("• Boot"));
    }

    #[test]
    fn prose_mentioning_interrupt_is_not_working_status() {
        // Real agent text about interrupts must not be mistaken for the status.
        assert!(!is_working_status_row(
            "You can press esc to interrupt a running task."
        ));
    }

    // --- is_separator_line ---

    #[test]
    fn plain_separator() {
        assert!(is_separator_line("────────────────────────"));
    }

    #[test]
    fn decorated_separator_with_label() {
        assert!(is_separator_line(
            "──────────────────────────────── extractor ──"
        ));
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
        assert!(!is_chrome_row(
            "[Opus 4.6 (1M context) | Max] │ tuicommander git:(main*)"
        ));
    }

    // --- Real-world examples from live sessions (CC v2.1.81, Codex v0.116.0) ---

    // Claude Code mode lines (captured 2026-03-21)
    #[test]
    fn cc_mode_line_with_hint() {
        assert!(is_chrome_row(
            "  ⏵⏵ bypass permissions on (shift+tab to cycle)"
        ));
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
        assert!(is_chrome_row(
            "✻ Sautéed for 2m 9s · 1 local agent still running"
        ));
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
        assert!(is_chrome_row(
            "⠴ Check tool-specific usage stats with /stats tools… (esc to cancel, 14s)"
        ));
    }

    #[test]
    fn gemini_braille_spinner_short() {
        assert!(is_chrome_row("⠋ Connecting to MCP servers..."));
    }

    // Aider Knight Rider spinner (captured 2026-03-22)
    #[test]
    fn aider_knight_rider_1() {
        assert!(is_chrome_row(
            "░█  Updating repo map: examples/plugins/repo-dashboard/main.js"
        ));
    }

    #[test]
    fn aider_knight_rider_2() {
        assert!(is_chrome_row(
            "█░  Waiting for openrouter/anthropic/claude-sonnet-4.5"
        ));
    }

    // Codex interrupt marker (captured 2026-03-21)
    #[test]
    fn codex_interrupt() {
        assert!(is_chrome_row(
            "■ Conversation interrupted - tell the model what to do differently."
        ));
    }

    // Claude Code tool progress spinner (◐ ◑ ◒ ◓)
    #[test]
    fn cc_tool_progress_spinner() {
        assert!(is_chrome_row("◐ Bash: .../b... | ✓ Bash ×9 | ✓ Read ×5"));
        assert!(is_chrome_row("◑ Read: src/main.rs"));
        assert!(is_chrome_row("◒ Edit: src/lib.rs"));
        assert!(is_chrome_row("◓ Write: output.txt"));
    }

    // Claude Code separators (captured 2026-03-21)
    #[test]
    fn cc_separator_with_extractor_label() {
        assert!(is_separator_line(
            "───────────────────────────────────────────────────────── extractor ──"
        ));
    }

    #[test]
    fn cc_separator_with_model_badge() {
        assert!(is_separator_line("──────── ■■■ Medium /model ────────"));
    }

    #[test]
    fn cc_permission_prompt_dotted_separator() {
        assert!(is_separator_line(
            "╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌"
        ));
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
        assert!(is_chrome_row(
            "  Context █░░░░░░░░░ 8% $0 (~$2.97) │ Usage ⚠ (429)"
        ));
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
        assert!(is_chrome_row(
            "Enter to select · Tab/Arrow keys to navigate · Esc to cancel"
        ));
    }

    // Codex tool call patterns (captured 2026-04-19) — must NOT be chrome
    #[test]
    fn codex_ran_command_not_chrome() {
        assert!(!is_chrome_row("• Ran git status --short"));
    }

    #[test]
    fn codex_ran_long_command_not_chrome() {
        assert!(!is_chrome_row(
            "• Ran xcodebuild -project StepsWidgetDemo.xcodeproj -scheme StepsWidgetDemo -destination 'generic/platform=iOS'"
        ));
    }

    #[test]
    fn codex_called_not_chrome() {
        assert!(!is_chrome_row("• Called"));
    }

    #[test]
    fn codex_waited_not_chrome() {
        assert!(!is_chrome_row("• Waited for background terminal"));
    }

    #[test]
    fn codex_hook_failed_not_chrome() {
        assert!(!is_chrome_row("• PreToolUse hook (failed)"));
    }

    #[test]
    fn codex_post_hook_failed_not_chrome() {
        assert!(!is_chrome_row("• PostToolUse hook (failed)"));
    }

    #[test]
    fn codex_response_text_not_chrome() {
        assert!(!is_chrome_row(
            "• Il progetto compila. Prima di chiudere salvo anche la memoria di onboarding."
        ));
    }

    #[test]
    fn codex_tree_connector_not_chrome() {
        assert!(!is_chrome_row("  └ ?? .gitignore"));
    }

    #[test]
    fn codex_truncation_indicator_not_chrome() {
        assert!(!is_chrome_row(
            "    … +109 lines (ctrl + t to view transcript)"
        ));
    }

    // Codex separator between tool output and summary
    #[test]
    fn codex_tool_separator() {
        assert!(is_separator_line(
            "───────────────────────────────────────────────────────────────────────────────────────────"
        ));
    }

    // Codex status line — NOT a separator
    #[test]
    fn codex_status_not_separator() {
        assert!(!is_separator_line(
            "  gpt-5.4 high · 100% left · ~/Gits/personal/tuicommander"
        ));
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
    fn cutoff_codex_no_quota() {
        // Codex without quota segment in status line (observed 2026-04-19)
        let rows: Vec<&str> = vec![
            "• Called",
            "  \u{2514} serena.write_memory({\"memory_name\":\"done_checklist\"})",
            "    Memory done_checklist written.",
            "• Working (4m 55s \u{2022} esc to interrupt)",
            "\u{203A} Improve documentation in @filename",
            "  gpt-5.4 high \u{00B7} ~/Gits/personal/steps",
        ];
        assert_eq!(find_chrome_cutoff(&rows), Some(4));
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
        let rows: Vec<&str> = vec!["Just plain text output.", "No chrome markers here."];
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

    // --- is_spinner_row tests ---

    #[test]
    fn spinner_row_dingbat() {
        assert!(is_spinner_row("✻ Cogitated for 3m 47s"));
        assert!(is_spinner_row("✳ Ideating… (1m 32s · ↓ 2.2k tokens)"));
    }

    #[test]
    fn spinner_row_braille() {
        assert!(is_spinner_row("⠋ Generating..."));
    }

    #[test]
    fn spinner_row_aider() {
        // Aider's Knight Rider block spinner (░█) LEADS its row → still matched.
        assert!(is_spinner_row("░██░░░░░░░"));
        assert!(is_spinner_row("█░  Waiting for model"));
    }

    #[test]
    fn hud_progress_bar_not_generic_spinner() {
        // A wiz/status-line HUD progress bar ticks every second but is not a
        // working spinner — the regression that pinned Claude BUSY (#446-596f).
        // The bar glyphs are mid-line (behind `[Opus] `), never leading.
        assert!(!is_spinner_row("[Opus | Team] ██░░░░░░░░ 17% | repo"));
        assert!(!is_spinner_row("  [Opus | Team] ░░░░░░░░░░ 0% | repo"));
    }

    #[test]
    fn claude_welcome_banner_not_spinner() {
        // Claude's ▐▛███▜▌ welcome banner art (boxed) does not lead with a
        // spinner glyph, and the `·` in `context) · Claude Team` is mid-line.
        assert!(!is_spinner_row("│   ▐▛███▜▌   │ What's new"));
        assert!(!is_spinner_row("│  ▝▜█████▛▘  │ Forked subagents"));
        assert!(!is_spinner_row(
            "│ Opus 4.8 (1M context) · Claude Team · LS │"
        ));
    }

    #[test]
    fn footer_with_middle_dot_not_spinner() {
        // The bypass footer carries a `·` separator mid-line — not a spinner.
        assert!(!is_spinner_row(
            "⏵⏵ bypass permissions on (shift+tab to cycle) · ← for agents"
        ));
    }

    #[test]
    fn spinner_row_middledot() {
        assert!(is_spinner_row("· Thinking…"));
    }

    #[test]
    fn spinner_row_codex_bullet() {
        assert!(is_spinner_row("• Working…"));
    }

    #[test]
    fn spinner_row_tool_progress() {
        assert!(is_spinner_row("◐ Bash: .../b... | ✓ Bash ×9 | ✓ Read ×5"));
        assert!(is_spinner_row("◑ Read: src/main.rs"));
        assert!(is_spinner_row("◒ Edit: src/lib.rs"));
        assert!(is_spinner_row("◓ Write: output.txt"));
    }

    #[test]
    fn not_spinner_mode_line() {
        assert!(!is_spinner_row("⏵ auto mode"));
        assert!(!is_spinner_row("⏸ plan mode"));
        assert!(!is_spinner_row("› auto"));
    }

    #[test]
    fn not_spinner_border() {
        assert!(!is_spinner_row("▀▀▀▀▀▀▀▀"));
        assert!(!is_spinner_row("▄▄▄▄▄▄▄▄"));
    }

    #[test]
    fn not_spinner_interrupt() {
        assert!(!is_spinner_row("■ Conversation interrupted"));
    }

    #[test]
    fn not_spinner_plain_text() {
        assert!(!is_spinner_row("Hello world"));
        assert!(!is_spinner_row(""));
    }

    #[test]
    fn not_spinner_lone_scrollbar_thumb() {
        // grok paints a scrollbar column down the right edge once its output outgrows the
        // viewport; the otherwise-blank rows trim to a single block glyph. Reading those as
        // Aider's spinner pinned the session BUSY for the rest of the process.
        assert!(!is_spinner_row(
            "                                                            █"
        ));
        assert!(!is_spinner_row("█"));
        assert!(!is_spinner_row("░"));
        // Aider's Knight Rider bar carries more cells and its status text — still a spinner.
        assert!(is_spinner_row("█░  Waiting for model"));
        assert!(is_spinner_row("░░█░"));
    }

    #[test]
    fn not_spinner_junie_status_bar() {
        // junie's idle effort icon (◐◑◒◓) is static chrome, not a live spinner.
        assert!(!is_spinner_row(
            "~ tuicommander   ⚑ Brave off ctrl + b   ⌘ Grok 4.3 OpenRouter   ◐ Medium effort"
        ));
        assert!(!is_spinner_row("⌘ Grok 4.3   ◑ High effort"));
        assert!(!is_spinner_row("⚑ Brave on   ◓ Low effort"));
    }

    #[test]
    fn junie_disambiguation_preserves_claude_spinner() {
        // A Claude tool-progress row uses the same ◐ glyph but lacks junie's
        // status-bar signature (⌘/⚑ + "effort"), so it stays a spinner.
        assert!(is_spinner_row("◐ Bash: running tests"));
        assert!(!is_junie_status_bar("◐ Bash: running tests"));
        assert!(is_junie_status_bar(
            "~ tuicommander   ⚑ Brave off   ⌘ Grok 4.3   ◐ Medium effort"
        ));
    }

    // --- is_task_list_row ---

    #[test]
    fn task_subtree_bracket() {
        assert!(is_task_list_row(
            "  ⎿  ✔ Hide globe icon in global workspace (done)"
        ));
    }

    #[test]
    fn task_subtree_bracket_with_count() {
        assert!(is_task_list_row("  ⎿  4 tasks (1 done, 3 open)"));
    }

    #[test]
    fn task_pending_checkbox() {
        assert!(is_task_list_row(
            "     ◻ Screenshot and verify overlay rendering"
        ));
    }

    #[test]
    fn task_completed_checkbox() {
        assert!(is_task_list_row(
            "     ✔ Hide globe icon in global workspace (done)"
        ));
    }

    #[test]
    fn task_plain_text_not_task() {
        assert!(!is_task_list_row("This is agent output"));
    }

    // --- find_chrome_cutoff with task list rows ---

    #[test]
    fn cutoff_cc_with_task_list() {
        // CC with task list above separator: task rows trimmed, spinner kept.
        // Walk-up: empty → task ◻ → task ◻ → task ⎿ → spinner (not task/empty/sep) → STOP
        let rows: Vec<&str> = vec![
            "Here is the answer to your question.",
            "✻ Propagating… (1m 57s · ↓ 1.7k tokens)",
            "  ⎿  ✔ Hide globe icon in global workspace (done)",
            "     ◻ Screenshot and verify overlay rendering",
            "     ◻ Add repo overlay on tab hover",
            "",
            "────────────────────────────────────────────────────────────────────────",
            "❯",
            "────────────────────────────────────────────────────────────────────────",
            "  [Opus 4.6 | Team] tuicommander git:(main*)",
            "  ⏵⏵ bypass permissions on (shift+tab to cycle)",
        ];
        // Cutoff at row 2: content + spinner kept, task rows + chrome trimmed
        assert_eq!(find_chrome_cutoff(&rows), Some(2));
    }

    #[test]
    fn cutoff_cc_without_task_list() {
        // CC without task list — should behave exactly as before
        let rows: Vec<&str> = vec![
            "Here is the answer to your question.",
            "",
            "────────────────────────────────────────────────────────────────────────",
            "❯",
            "────────────────────────────────────────────────────────────────────────",
            "  [Opus 4.6 | Team] tuicommander git:(main*)",
            "  ⏵⏵ bypass permissions on (shift+tab to cycle)",
        ];
        assert_eq!(find_chrome_cutoff(&rows), Some(1));
    }

    // --- is_agent_prompt_row ---

    #[test]
    fn agent_prompt_row_accepts_bare_prompt_glyphs() {
        assert!(is_agent_prompt_row("❯"));
        assert!(is_agent_prompt_row("› "));
        assert!(is_agent_prompt_row("  ❯  "));
        assert!(is_agent_prompt_row(">"));
        assert!(is_agent_prompt_row("> "), "bare prompt, nothing typed");
    }

    /// The transcript echo of a submitted user message sits on a prompt row.
    /// Anchoring there deleted the conversation from the mobile log.
    #[test]
    fn agent_prompt_row_rejects_echoed_user_message() {
        assert!(!is_agent_prompt_row(
            "❯ rename non funziona nel filebrowser"
        ));
        assert!(!is_agent_prompt_row("› riprendiamo"));
    }

    #[test]
    fn agent_prompt_row_rejects_markdown_quote() {
        assert!(!is_agent_prompt_row("> quoted guidance from the manual"));
        assert!(!is_agent_prompt_row("  > indented blockquote"));
    }

    // --- find_scrollback_chrome_cutoff ---

    #[test]
    fn scrollback_cutoff_marks_prompt_box() {
        let rows: Vec<&str> = vec![
            "Here is the answer to your question.",
            "",
            "────────────────────────────────────────────",
            "❯",
            "────────────────────────────────────────────",
            "  [Opus 4.6 | Team] tuicommander git:(main*)",
        ];
        assert_eq!(find_scrollback_chrome_cutoff(&rows), Some(1));
    }

    #[test]
    fn scrollback_cutoff_ignores_markdown_quote() {
        let rows: Vec<&str> = vec![
            "Here is what the docs say:",
            "> quoted guidance from the manual",
            "verificabile; non è un force-push e non sovrascrive alcun branch",
        ];
        assert_eq!(
            find_scrollback_chrome_cutoff(&rows),
            None,
            "prose after a blockquote must survive in history"
        );
    }

    #[test]
    fn scrollback_cutoff_ignores_standalone_separator() {
        let rows: Vec<&str> = vec![
            "• Ran cargo nextest run",
            "  └ ──────────────────────",
            "    Summary [ 12.4s ] 91 tests run",
        ];
        assert_eq!(
            find_scrollback_chrome_cutoff(&rows),
            None,
            "a divider is not a prompt — screen trim may cut here, history may not"
        );
    }

    #[test]
    fn scrollback_cutoff_still_extends_past_separator_above_prompt() {
        let rows: Vec<&str> = vec![
            "real output",
            "────────────────────",
            "❯ ",
            "  Context ███░░░",
        ];
        assert_eq!(find_scrollback_chrome_cutoff(&rows), Some(1));
    }

    #[test]
    fn scrollback_cutoff_empty_and_blank_batches() {
        assert_eq!(find_scrollback_chrome_cutoff(&[]), None);
        assert_eq!(find_scrollback_chrome_cutoff(&["", "", ""]), None);
    }

    #[test]
    fn scrollback_cutoff_prompt_outside_scan_window_is_ignored() {
        let mut rows: Vec<&str> = vec!["❯"];
        rows.extend(std::iter::repeat_n("content", CHROME_SCAN_ROWS + 2));
        assert_eq!(find_scrollback_chrome_cutoff(&rows), None);
    }
}
