//! Rust-side matching for plugin `OutputWatcher`s.
//!
//! Watchers used to be matched entirely in the WebView: every PTY line was
//! ANSI-stripped and run through every registered `RegExp` on the same thread
//! that paints the terminal (audit finding F3). The matching is pure text work
//! with no DOM dependency, so it moves here.
//!
//! Four rules keep that split honest:
//!
//! * **Rust is the only line assembler.** The WebView no longer reassembles
//!   lines from raw chunks. Two independent assemblers meant a line that began
//!   before a watcher registered and ended after it was split between them and
//!   seen by neither. [`StreamLines`] runs even with no watcher registered, so
//!   its trailing partial is always warm when one appears.
//! * **The cleaner is a port, not an approximation.** [`clean_line`] mirrors
//!   `src/utils/stripAnsi.ts` pattern-for-pattern and applies the same backtick
//!   strip the frontend used to apply, so both sides see the same text.
//! * **Rust may over-match, never under-match.** The frontend re-runs the real
//!   `RegExp` on every flagged line to build the `RegExpExecArray` the plugin
//!   API promises, so a Rust false positive is filtered there. A false
//!   *negative* is invisible and permanent — hence [`to_portable_pattern`],
//!   which rewrites the ECMAScript character-class escapes whose Rust meaning
//!   is narrower, and rejects what it cannot express.
//! * **A pattern we cannot express is not our problem to guess at.** Rejected
//!   watchers keep matching in the WebView, which is told to expect every line
//!   for as long as one is registered.
//!
//! Known accepted divergence: a pattern that *counts* UTF-16 code units of
//! astral characters (`/^.{2}$/` against an emoji) matches in JavaScript and
//! not in Rust. Rewriting quantifier semantics is not worth it; the case has
//! no plausible watcher.

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// A watcher as the frontend registered it: the pattern source and flags of a
/// JS `RegExp`, plus the id the frontend uses to find it again.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WatcherSpec {
    pub id: String,
    pub pattern: String,
    /// JS `RegExp` flag string, e.g. `"i"` or `"gim"`.
    #[serde(default)]
    pub flags: String,
}

/// One assembled line on its way to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WatcherLine {
    /// The cleaned line. The frontend re-runs its own `RegExp` on this exact
    /// text, so it must be the text we matched.
    pub text: String,
    /// Qualified ids (`client/watcher`) of the watchers Rust already matched.
    /// The frontend fires those and skips them when it scans the line for its
    /// own, un-compilable watchers — one line never fires a watcher twice.
    pub matched_ids: Vec<String>,
}

/// The batch of lines pushed to one session's frontend.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WatcherLines {
    pub session_id: String,
    pub lines: Vec<WatcherLine>,
}

/// Reply to a watcher sync. `applied` is false when the sync was stale — the
/// frontend must then ignore `rejected` entirely, because the set it describes
/// is not the set the backend holds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SyncOutcome {
    pub applied: bool,
    pub rejected: Vec<String>,
}

struct CompiledWatcher {
    /// `client_id/watcher_id` — watcher ids are per-frontend counters, so they
    /// collide across clients unless qualified.
    qualified_id: String,
    regex: Regex,
    /// The `m` flag was set. ECMAScript then anchors `^`/`$` at every
    /// *LineTerminator* — CR and U+2028/U+2029 included — while the crate knows
    /// only LF, so such a pattern also has to be tried on each segment.
    multi_line: bool,
}

struct ClientWatchers {
    client_id: String,
    webview_label: Option<String>,
    /// Registry-wide tick of this client's last sync. Eviction takes the
    /// smallest — the client that has been silent longest, which the frontend
    /// heartbeat makes a good proxy for "gone".
    touched: u64,
    /// Monotonic per-client sync counter. A lower one is a reply that lost a
    /// race and must not install its (older) set.
    seq: u64,
    watchers: Vec<CompiledWatcher>,
    /// This client has at least one watcher Rust could not compile, so it needs
    /// every line, not only the matched ones.
    has_rejected: bool,
}

/// Watcher sets, one per connected frontend. A desktop window and a browser tab
/// each register their own; matching is the union, and every id is qualified so
/// a client only fires what it registered.
#[derive(Default)]
pub struct OutputWatcherRegistry {
    clients: Vec<ClientWatchers>,
    /// Incremented on every sync; stamped into `ClientWatchers::touched`.
    tick: u64,
}

/// Frontends that can hold a watcher set at once. Browser tabs do not have a
/// reliable disconnect signal, so the list is bounded and evicts the oldest.
const MAX_CLIENTS: usize = 8;

impl OutputWatcherRegistry {
    pub fn sync_for_webview(
        &mut self,
        webview_label: &str,
        client_id: &str,
        seq: u64,
        specs: &[WatcherSpec],
    ) -> SyncOutcome {
        self.sync_owned(Some(webview_label), client_id, seq, specs)
    }

    pub fn remove_webview(&mut self, webview_label: &str) {
        self.clients
            .retain(|client| client.webview_label.as_deref() != Some(webview_label));
    }

    /// Install `specs` for `client_id`. Returns the ids that could not be
    /// compiled — those stay matched in the WebView.
    ///
    /// `seq` orders the *mutations*, not the replies: two syncs from one client
    /// can be in flight at once, and the older one must not install its set
    /// after the newer one.
    pub fn sync(&mut self, client_id: &str, seq: u64, specs: &[WatcherSpec]) -> SyncOutcome {
        self.sync_owned(None, client_id, seq, specs)
    }

    fn sync_owned(
        &mut self,
        webview_label: Option<&str>,
        client_id: &str,
        seq: u64,
        specs: &[WatcherSpec],
    ) -> SyncOutcome {
        if let Some(existing) = self.clients.iter().find(|c| c.client_id == client_id)
            && seq <= existing.seq
        {
            return SyncOutcome {
                applied: false,
                rejected: Vec::new(),
            };
        }

        let mut watchers = Vec::with_capacity(specs.len());
        let mut rejected = Vec::new();
        for spec in specs {
            match build_regex(&spec.pattern, &spec.flags) {
                Some(regex) => watchers.push(CompiledWatcher {
                    qualified_id: qualify(client_id, &spec.id),
                    regex,
                    multi_line: spec.flags.contains('m'),
                }),
                None => rejected.push(spec.id.clone()),
            }
        }

        self.tick += 1;
        self.clients.retain(|c| c.client_id != client_id);
        // An empty set still parks an entry. Removing it would drop the `seq`
        // with it, and a *delayed* older sync would then pass the staleness
        // check and resurrect the set the frontend just disposed of.
        if self.clients.len() >= MAX_CLIENTS
            && let Some(oldest) = self
                .clients
                .iter()
                .enumerate()
                .min_by_key(|(_, c)| c.touched)
                .map(|(i, _)| i)
        {
            let evicted = self.clients.remove(oldest);
            tracing::warn!(
                source = "plugin",
                "Output watcher clients exceeded {MAX_CLIENTS}; dropped the set of \"{}\". \
                 If that client is alive it goes blind until its next heartbeat sync.",
                evicted.client_id
            );
        }
        self.clients.push(ClientWatchers {
            client_id: client_id.to_string(),
            webview_label: webview_label.map(str::to_string),
            touched: self.tick,
            seq,
            watchers,
            has_rejected: !rejected.is_empty(),
        });

        SyncOutcome {
            applied: true,
            rejected,
        }
    }

    /// No watcher anywhere: the reader can skip matching entirely.
    pub fn is_idle(&self) -> bool {
        self.clients
            .iter()
            .all(|c| c.watchers.is_empty() && !c.has_rejected)
    }

    /// Some frontend still matches patterns Rust could not compile, so it needs
    /// every line rather than only the ones Rust flagged.
    pub fn needs_all_lines(&self) -> bool {
        self.clients.iter().any(|c| c.has_rejected)
    }

    /// Qualified ids of the watchers matching `line`, which must already be
    /// cleaned.
    pub fn matching_ids(&self, line: &str) -> Vec<String> {
        let mut ids = Vec::new();
        // Computed once for the whole set, and only when the line carries a
        // terminator the crate ignores — the common line allocates nothing.
        let segments = ecma_segments(line);
        for client in &self.clients {
            for watcher in &client.watchers {
                if watcher.regex.is_match(line) {
                    ids.push(watcher.qualified_id.clone());
                    continue;
                }
                if watcher.multi_line
                    && segments
                        .iter()
                        .any(|segment| watcher.regex.is_match(segment))
                {
                    ids.push(watcher.qualified_id.clone());
                }
            }
        }
        ids
    }
}

fn qualify(client_id: &str, watcher_id: &str) -> String {
    format!("{client_id}/{watcher_id}")
}

/// ECMAScript LineTerminators other than LF. A cleaned line can hold them: a
/// progress bar redraws with CR, and the assembler only splits on LF.
const ECMA_EXTRA_TERMINATORS: [char; 3] = ['\r', '\u{2028}', '\u{2029}'];

/// Split a line the way an `m`-flagged `RegExp` sees it. Empty when the line has
/// no terminator beyond LF, which is the overwhelmingly common case.
fn ecma_segments(line: &str) -> Vec<&str> {
    if !line.contains(ECMA_EXTRA_TERMINATORS) {
        return Vec::new();
    }
    line.split(ECMA_EXTRA_TERMINATORS).collect()
}

/// ECMAScript `\d` — the ten ASCII digits, unlike the Unicode-aware `\d` of the
/// `regex` crate.
const ECMA_DIGIT: &str = "0-9";
/// ECMAScript `\w`.
const ECMA_WORD: &str = "0-9A-Za-z_";
/// ECMAScript `\s` — WhiteSpace ∪ LineTerminator ∪ U+FEFF. Notably it holds
/// U+FEFF, which is not Unicode `White_Space`, and omits U+0085, which is.
const ECMA_SPACE: &str = concat!(
    r"\t\n\x0B\x0C\r ",
    r"\u{00a0}\u{1680}\u{2000}-\u{200a}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}\u{feff}"
);

/// Rewrite a JS `RegExp` source into one whose `regex`-crate meaning is at
/// least as wide as the ECMAScript meaning. Returns `None` for patterns that
/// cannot be expressed that way — the caller rejects them.
///
/// Only the escapes whose Rust meaning is *narrower* need rewriting: `\D`, `\W`
/// and `\S` are Unicode-aware negations in Rust and would drop lines JS keeps.
/// The positive forms are rewritten too, so both stay exact.
fn to_portable_pattern(pattern: &str) -> Option<String> {
    let mut out = String::with_capacity(pattern.len());
    let mut chars = pattern.chars();
    let mut in_class = false;
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                // A trailing backslash is a syntax error in both engines; let the
                // compiler reject it.
                let escaped = chars.next()?;
                match escaped {
                    'd' if in_class => out.push_str(ECMA_DIGIT),
                    'w' if in_class => out.push_str(ECMA_WORD),
                    's' if in_class => out.push_str(ECMA_SPACE),
                    // `(?-i:…)` because an `i` flag would case-fold these classes
                    // and a *negated* one would then exclude what the fold pulled
                    // in: `[^A-Za-z]` folded also excludes U+212A KELVIN SIGN,
                    // which ECMAScript `\W` matches. That is an under-match, the
                    // one direction this translation may not take.
                    'd' => out.push_str(&format!("(?-i:[{ECMA_DIGIT}])")),
                    'w' => out.push_str(&format!("(?-i:[{ECMA_WORD}])")),
                    's' => out.push_str(&format!("(?-i:[{ECMA_SPACE}])")),
                    // A negated class inside a class ("everything except digits,
                    // or an 'a'") has no character-class spelling. Reject.
                    'D' | 'W' | 'S' if in_class => return None,
                    'D' => out.push_str(&format!("(?-i:[^{ECMA_DIGIT}])")),
                    'W' => out.push_str(&format!("(?-i:[^{ECMA_WORD}])")),
                    'S' => out.push_str(&format!("(?-i:[^{ECMA_SPACE}])")),
                    // Word boundaries: Rust's are Unicode-aware, so "caf|é" is a
                    // boundary in JS and not in Rust. `(?-u:…)` asks for the ASCII
                    // ones; if the crate refuses, the pattern is rejected on build.
                    'b' if !in_class => out.push_str("(?-u:\\b)"),
                    'B' if !in_class => out.push_str("(?-u:\\B)"),
                    other => {
                        out.push('\\');
                        out.push(other);
                    }
                }
            }
            '[' if !in_class => {
                in_class = true;
                out.push('[');
            }
            ']' if in_class => {
                in_class = false;
                out.push(']');
            }
            other => out.push(other),
        }
    }
    if in_class { None } else { Some(out) }
}

/// Translate a JS `RegExp` source + flags into a compiled `regex`.
/// Returns `None` when the pattern uses syntax the crate does not support.
fn build_regex(pattern: &str, flags: &str) -> Option<Regex> {
    let portable = to_portable_pattern(pattern)?;
    RegexBuilder::new(&portable)
        // Rust folds case over Unicode, JS (without `u`) over ASCII: Rust matches
        // a superset here, which the frontend's re-exec filters.
        .case_insensitive(flags.contains('i'))
        .multi_line(flags.contains('m'))
        .dot_matches_new_line(flags.contains('s'))
        .build()
        .ok()
}

/// The ANSI escape pattern, ported from `src/utils/stripAnsi.ts`:
/// CSI (`ESC [ params intermediates final`) and OSC (`ESC ] … BEL|ST`).
/// `[\s\S]*?` becomes `(?s:.)*?` — the crate has no `\s\S` idiom but the same
/// meaning is "any char including newline, lazily".
const ANSI_PATTERN: &str =
    r"\x1b\[[\x30-\x3f]*[\x20-\x2f]*[\x40-\x7e]|\x1b\](?s:.)*?(?:\x07|\x1b\\)";

fn ansi_regex() -> &'static Regex {
    static ANSI: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    ANSI.get_or_init(|| Regex::new(ANSI_PATTERN).expect("ANSI_PATTERN is a valid regex"))
}

/// Clean a raw PTY line the way the frontend used to: strip ANSI escapes, then
/// backticks (agents render tokens as markdown inline code, which leaves
/// literal backticks in the clean text).
pub fn clean_line(line: &str) -> String {
    let stripped = ansi_regex().replace_all(line, "");
    if stripped.contains('`') {
        stripped.replace('`', "")
    } else {
        stripped.into_owned()
    }
}

/// A line this long has no plausible watcher and must not grow the reader's
/// buffer without bound: PTY producers that never emit `\n` (progress bars
/// redrawing with `\r`, TUI repaints) would otherwise accumulate for the whole
/// session.
const MAX_PENDING_LINE: usize = 256 * 1024;

/// Reassembles complete lines from arbitrary PTY chunks — the Rust counterpart
/// of the `LineBuffer` the frontend used to run, including its trailing-`\r`
/// trim.
#[derive(Default)]
pub struct StreamLines {
    buffer: String,
    /// The line being assembled passed `MAX_PENDING_LINE` and was dropped. The
    /// bytes still arriving belong to it, so they are dropped too, up to the
    /// newline that ends it.
    overflowed: bool,
}

impl StreamLines {
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a chunk, returning the complete lines it closed. The partial
    /// trailing line is held until a newline arrives.
    pub fn push(&mut self, chunk: &str) -> Vec<String> {
        self.buffer.push_str(chunk);
        if !self.buffer.contains('\n') {
            self.cap_pending();
            return Vec::new();
        }
        let mut lines: Vec<String> = self
            .buffer
            .split('\n')
            .map(|l| l.strip_suffix('\r').unwrap_or(l).to_string())
            .collect();
        // The last element is the still-incomplete line (empty when the chunk
        // ended on a newline). Keep it untrimmed — its `\r` may belong to a
        // `\r\n` the next chunk completes.
        let tail = lines.pop().unwrap_or_default();
        self.buffer = tail;
        // The first complete line is the end of the one that overflowed. It is
        // not a line that was on the wire, so it never reaches a watcher.
        if self.overflowed {
            self.overflowed = false;
            if !lines.is_empty() {
                lines.remove(0);
            }
        }
        self.cap_pending();
        lines
    }

    /// Track the trailing partial line without materialising the complete ones.
    /// Used while no watcher is registered: the buffer must stay correct so a
    /// watcher registering mid-line still sees that line completed, but nothing
    /// downstream wants the lines themselves.
    pub fn push_discarding(&mut self, chunk: &str) {
        match chunk.rfind('\n') {
            Some(idx) => {
                self.buffer.clear();
                self.buffer.push_str(&chunk[idx + 1..]);
                self.overflowed = false;
            }
            None => self.buffer.push_str(chunk),
        }
        self.cap_pending();
    }

    /// The unterminated tail, at end of stream. A command that exits without a
    /// final newline (`printf DONE`) still put that line on the wire, so it is a
    /// line — there is simply nothing left to complete it.
    pub fn flush(&mut self) -> Option<String> {
        let overflowed = std::mem::take(&mut self.overflowed);
        let tail = std::mem::take(&mut self.buffer);
        if overflowed || tail.is_empty() {
            return None;
        }
        Some(tail.strip_suffix('\r').unwrap_or(&tail).to_string())
    }

    fn cap_pending(&mut self) {
        if self.buffer.len() <= MAX_PENDING_LINE {
            return;
        }
        // Drop the whole line, not just its head. Publishing the retained tail
        // would hand watchers a line that was never on the wire — `/^x+$/` fires
        // on `yxxx…` once the `y` is cut — and that is the one direction this
        // path may never take. Loud, because a silent drop reads as "no match".
        tracing::warn!(
            source = "plugin",
            "An output line passed {MAX_PENDING_LINE} bytes without a newline; \
             it is dropped rather than truncated, so no watcher can match it"
        );
        self.buffer.clear();
        self.overflowed = true;
    }
}

/// Throttles the watcher-line stream to one event per window, concatenating
/// instead of dropping.
///
/// Dropping was the original bug (audit F1): the frontend reassembled lines
/// across chunks, so a dropped chunk spliced the tail of one onto the head of a
/// later one and reported a line that was never on the wire. Rust assembles the
/// lines now, but the same rule holds for the batch — every matched line must
/// arrive, and arrive in order.
pub struct WatcherLineBatcher {
    pending: Vec<WatcherLine>,
    pending_bytes: usize,
    last_emit: Option<Instant>,
    window: Duration,
    /// Emit early rather than let a flood grow the batch without bound. This is
    /// the one place the window ceiling gives way: a producer fast enough to
    /// fill the cap inside a window gets more than one event. The alternative is
    /// dropping lines or holding a window's worth of a multi-MB/s flood, and
    /// only a watcher that matches nearly every line can get there.
    max_bytes: usize,
}

impl WatcherLineBatcher {
    pub fn new(window: Duration, max_bytes: usize) -> Self {
        Self {
            pending: Vec::new(),
            pending_bytes: 0,
            last_emit: None,
            window,
            max_bytes,
        }
    }

    /// Buffer a line. Returns the batch to emit now — when the throttle window
    /// has elapsed, on the very first line, or when the batch reached its cap.
    pub fn push(&mut self, line: WatcherLine, now: Instant) -> Option<Vec<WatcherLine>> {
        self.pending_bytes += line.text.len();
        self.pending.push(line);
        let due = self
            .last_emit
            .is_none_or(|last| now.duration_since(last) >= self.window);
        if due || self.pending_bytes >= self.max_bytes {
            self.emit(now)
        } else {
            None
        }
    }

    /// Drain a buffered tail once the window elapsed. Called from the frame
    /// ticker: reads block, so a trailing line would otherwise sit in the batch
    /// until more output arrives — which, for a rare-line watcher, may be never.
    pub fn flush_due(&mut self, now: Instant) -> Option<Vec<WatcherLine>> {
        if self.pending.is_empty() {
            return None;
        }
        let due = self
            .last_emit
            .is_none_or(|last| now.duration_since(last) >= self.window);
        if due { self.emit(now) } else { None }
    }

    /// Unconditional drain at session teardown.
    pub fn take(&mut self) -> Option<Vec<WatcherLine>> {
        if self.pending.is_empty() {
            return None;
        }
        self.pending_bytes = 0;
        Some(std::mem::take(&mut self.pending))
    }

    fn emit(&mut self, now: Instant) -> Option<Vec<WatcherLine>> {
        self.last_emit = Some(now);
        if self.pending.is_empty() {
            return None;
        }
        self.pending_bytes = 0;
        Some(std::mem::take(&mut self.pending))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(id: &str, pattern: &str, flags: &str) -> WatcherSpec {
        WatcherSpec {
            id: id.into(),
            pattern: pattern.into(),
            flags: flags.into(),
        }
    }

    // ── clean_line: must agree with the frontend, character for character ──

    #[test]
    fn clean_line_strips_sgr_cursor_and_erase_sequences() {
        assert_eq!(clean_line("\x1b[32mhello\x1b[0m"), "hello");
        assert_eq!(clean_line("\x1b[2Kline\x1b[1A"), "line");
        assert_eq!(clean_line("\x1b[?25lhidden\x1b[?25h"), "hidden");
    }

    #[test]
    fn clean_line_strips_osc_with_both_terminators() {
        assert_eq!(clean_line("\x1b]0;title\x07text"), "text");
        assert_eq!(clean_line("\x1b]777;notify;a;b\x1b\\text"), "text");
    }

    #[test]
    fn clean_line_strips_backticks_like_the_frontend() {
        assert_eq!(clean_line("edit `src/main.rs` now"), "edit src/main.rs now");
    }

    #[test]
    fn clean_line_leaves_plain_text_untouched() {
        assert_eq!(clean_line("model is at capacity"), "model is at capacity");
    }

    // ── compile: reject rather than mis-compile ──

    #[test]
    fn compile_accepts_portable_patterns() {
        let mut registry = OutputWatcherRegistry::default();
        let outcome = registry.sync(
            "c1",
            1,
            &[
                spec("w1", "done", "i"),
                spec("w2", "model is at capacity|rate limit", "i"),
            ],
        );

        assert!(outcome.applied);
        assert_eq!(outcome.rejected, [] as [std::string::String; 0]);
        assert_eq!(registry.matching_ids("DONE"), vec!["c1/w1".to_string()]);
        assert_eq!(
            registry.matching_ids("model is at capacity"),
            vec!["c1/w2".to_string()]
        );
    }

    #[test]
    fn compile_rejects_patterns_the_crate_cannot_express() {
        // Lookahead and backreferences are JS-only; mis-compiling them would
        // silently change which lines a plugin sees.
        let mut registry = OutputWatcherRegistry::default();
        let outcome = registry.sync(
            "c1",
            1,
            &[
                spec("look", "foo(?=bar)", ""),
                spec("backref", r"(a)\1", ""),
                spec("ok", "plain", ""),
            ],
        );

        assert_eq!(
            outcome.rejected,
            vec!["look".to_string(), "backref".to_string()]
        );
        assert_eq!(registry.matching_ids("plain"), vec!["c1/ok".to_string()]);
        assert!(registry.needs_all_lines());
    }

    #[test]
    fn case_sensitivity_follows_the_js_flag() {
        let mut sensitive = OutputWatcherRegistry::default();
        sensitive.sync("c1", 1, &[spec("w", "Done", "")]);
        assert_eq!(
            sensitive.matching_ids("done"),
            [] as [std::string::String; 0]
        );

        let mut insensitive = OutputWatcherRegistry::default();
        insensitive.sync("c1", 1, &[spec("w", "Done", "i")]);
        assert_eq!(insensitive.matching_ids("done"), vec!["c1/w".to_string()]);
    }

    #[test]
    fn an_empty_registry_matches_nothing() {
        let registry = OutputWatcherRegistry::default();
        assert!(registry.is_idle());
        assert!(!registry.needs_all_lines());
        assert_eq!(
            registry.matching_ids("anything"),
            [] as [std::string::String; 0]
        );
    }

    // ── ECMAScript class semantics: Rust must never match less than JS ──

    #[test]
    fn negated_digit_keeps_the_ascii_meaning_of_javascript() {
        // U+0661 ARABIC-INDIC DIGIT ONE is not a `\d` in ECMAScript, so `\D`
        // matches it. The crate's Unicode-aware `\D` would not.
        let mut registry = OutputWatcherRegistry::default();
        registry.sync("c1", 1, &[spec("w", r"^\D$", "")]);
        assert_eq!(registry.matching_ids("\u{661}"), vec!["c1/w".to_string()]);
        assert_eq!(registry.matching_ids("7"), [] as [std::string::String; 0]);
    }

    #[test]
    fn negated_word_and_space_keep_the_ascii_meaning_of_javascript() {
        let mut registry = OutputWatcherRegistry::default();
        registry.sync("c1", 1, &[spec("w", r"^\W$", ""), spec("s", r"^\S$", "")]);
        // 'é' is a word character for the crate, not for ECMAScript.
        assert!(registry.matching_ids("é").contains(&"c1/w".to_string()));
        // U+0085 NEXT LINE is Unicode whitespace but not ECMAScript whitespace.
        assert!(
            registry
                .matching_ids("\u{85}")
                .contains(&"c1/s".to_string())
        );
    }

    #[test]
    fn positive_classes_stay_exact_inside_and_outside_a_class() {
        let mut registry = OutputWatcherRegistry::default();
        registry.sync(
            "c1",
            1,
            &[spec("d", r"^\d+$", ""), spec("mix", r"^[\w.]+$", "")],
        );
        assert_eq!(
            registry.matching_ids("42"),
            vec!["c1/d".to_string(), "c1/mix".to_string()]
        );
        // Arabic-Indic digits are not ECMAScript `\d`.
        assert_eq!(
            registry.matching_ids("\u{661}"),
            [] as [std::string::String; 0]
        );
        assert_eq!(
            registry.matching_ids("src.main_2"),
            vec!["c1/mix".to_string()]
        );
    }

    #[test]
    fn a_negated_class_escape_inside_a_class_is_rejected() {
        let mut registry = OutputWatcherRegistry::default();
        let outcome = registry.sync("c1", 1, &[spec("w", r"[\D.]", "")]);
        assert_eq!(outcome.rejected, vec!["w".to_string()]);
    }

    #[test]
    fn a_case_insensitive_negated_class_still_matches_what_javascript_matches() {
        // U+212A KELVIN SIGN and U+017F LATIN SMALL LETTER LONG S case-fold to
        // ASCII letters in Rust but not in ECMAScript, so `\W` matches them in
        // JavaScript. A folded `[^A-Za-z]` would not.
        let mut registry = OutputWatcherRegistry::default();
        registry.sync("c1", 1, &[spec("w", r"^\W$", "i")]);
        assert_eq!(registry.matching_ids("\u{212a}"), vec!["c1/w".to_string()]);
        assert_eq!(registry.matching_ids("\u{17f}"), vec!["c1/w".to_string()]);
        assert_eq!(registry.matching_ids("a"), [] as [std::string::String; 0]);
    }

    #[test]
    fn word_boundaries_use_the_ascii_definition() {
        let mut registry = OutputWatcherRegistry::default();
        let outcome = registry.sync("c1", 1, &[spec("w", r"\berror\b", "")]);
        assert!(outcome.rejected.is_empty(), "\\b must stay compilable");
        assert_eq!(
            registry.matching_ids("fatal error here"),
            vec!["c1/w".to_string()]
        );
        assert_eq!(
            registry.matching_ids("errors"),
            [] as [std::string::String; 0]
        );
        // ECMAScript sees a boundary between 'f' and 'é'; a Unicode-aware `\b`
        // would not, and the line would be lost.
        let mut accented = OutputWatcherRegistry::default();
        accented.sync("c1", 1, &[spec("w", r"caf\b", "")]);
        assert_eq!(accented.matching_ids("café"), vec!["c1/w".to_string()]);
    }

    /// ECMAScript ends a line at CR and U+2028/U+2029 too, so `/^DONE/m` matches
    /// inside `progress\rDONE`. The crate anchors at LF only and returned no
    /// match — a false negative, which is invisible: the frontend trusts the
    /// pattern to Rust and never re-scans that line.
    #[test]
    fn multiline_anchors_see_every_ecmascript_line_terminator() {
        let mut registry = OutputWatcherRegistry::default();
        registry.sync("c1", 1, &[spec("w", "^DONE", "m")]);

        assert_eq!(
            registry.matching_ids("progress\rDONE"),
            vec!["c1/w".to_string()],
            "a bare CR ends a line for an m-flagged RegExp"
        );
        assert_eq!(
            registry.matching_ids("progress\u{2028}DONE"),
            vec!["c1/w".to_string()]
        );
        assert_eq!(
            registry.matching_ids("progress\nDONE"),
            vec!["c1/w".to_string()]
        );
        assert_eq!(
            registry.matching_ids("progress DONE"),
            [] as [std::string::String; 0]
        );
    }

    /// Segment matching is only for `m`. Without it, `^` is start-of-input in
    /// both engines, and an over-match here would be filtered by the frontend
    /// anyway — but there is no reason to send the line.
    #[test]
    fn a_pattern_without_m_is_not_matched_per_segment() {
        let mut registry = OutputWatcherRegistry::default();
        registry.sync("c1", 1, &[spec("w", "^DONE", "")]);
        assert_eq!(
            registry.matching_ids("progress\rDONE"),
            [] as [std::string::String; 0]
        );
    }

    // ── per-client sets: one frontend cannot blind another ──

    #[test]
    fn two_clients_keep_independent_sets_and_ids() {
        let mut registry = OutputWatcherRegistry::default();
        registry.sync("a", 1, &[spec("w1", "alpha", "")]);
        registry.sync("b", 1, &[spec("w1", "beta", "")]);

        assert_eq!(registry.matching_ids("alpha"), vec!["a/w1".to_string()]);
        assert_eq!(registry.matching_ids("beta"), vec!["b/w1".to_string()]);
    }

    #[test]
    fn one_client_rejecting_a_pattern_makes_every_line_flow() {
        let mut registry = OutputWatcherRegistry::default();
        registry.sync("a", 1, &[spec("w1", "foo(?=bar)", "")]);
        registry.sync("b", 1, &[spec("w1", "plain", "")]);

        assert!(registry.needs_all_lines());
        assert!(!registry.is_idle());
    }

    #[test]
    fn a_stale_sync_cannot_install_its_older_set() {
        let mut registry = OutputWatcherRegistry::default();
        // Sync 2 lands first (the invokes raced), then sync 1 arrives late.
        registry.sync("a", 2, &[spec("w1", "alpha", ""), spec("w2", "beta", "")]);
        let outcome = registry.sync("a", 1, &[spec("w1", "alpha", "")]);

        assert!(!outcome.applied);
        assert_eq!(registry.matching_ids("beta"), vec!["a/w2".to_string()]);
    }

    #[test]
    fn an_empty_sync_leaves_no_watcher_matching() {
        let mut registry = OutputWatcherRegistry::default();
        registry.sync("a", 1, &[spec("w1", "foo(?=bar)", "")]);
        assert!(registry.needs_all_lines());

        registry.sync("a", 2, &[]);
        assert!(registry.is_idle());
        assert!(!registry.needs_all_lines());
    }

    /// Disposing a set must not forget the sequence that disposed it. Removing
    /// the record took its `seq` along, so a *delayed* older sync then passed
    /// the staleness check and reinstalled the set the frontend had dropped —
    /// resurrecting, in the reported case, a rejected pattern that forces every
    /// PTY line across the boundary for a frontend that holds no watcher.
    #[test]
    fn an_empty_sync_still_blocks_a_delayed_older_sync() {
        let mut registry = OutputWatcherRegistry::default();
        // Disposal (seq 2) overtakes the registration (seq 1) on the wire.
        registry.sync("a", 2, &[]);
        let outcome = registry.sync("a", 1, &[spec("w1", "foo(?=bar)", "")]);

        assert!(!outcome.applied);
        assert!(registry.is_idle());
        assert!(!registry.needs_all_lines());
    }

    /// Eviction takes the client that has been silent longest, not the one that
    /// registered first: the frontend heartbeat keeps a live client recent, so a
    /// set that has gone quiet is the one that is really gone.
    #[test]
    fn eviction_takes_the_least_recently_synced_client() {
        let mut registry = OutputWatcherRegistry::default();
        for i in 0..MAX_CLIENTS {
            registry.sync(&format!("c{i}"), 1, &[spec("w", "plain", "")]);
        }
        // c0 is the oldest registration but heartbeats; c1 stays silent.
        registry.sync("c0", 2, &[spec("w", "plain", "")]);
        registry.sync("newcomer", 1, &[spec("w", "plain", "")]);

        assert_eq!(registry.clients.len(), MAX_CLIENTS);
        assert!(registry.clients.iter().any(|c| c.client_id == "c0"));
        assert!(registry.clients.iter().all(|c| c.client_id != "c1"));
    }

    #[test]
    fn the_client_list_is_bounded() {
        let mut registry = OutputWatcherRegistry::default();
        for i in 0..(MAX_CLIENTS + 2) {
            registry.sync(&format!("c{i}"), 1, &[spec("w", "plain", "")]);
        }
        assert_eq!(registry.clients.len(), MAX_CLIENTS);
        // The oldest two were evicted, the newest survive.
        assert!(registry.clients.iter().all(|c| c.client_id != "c0"));
        assert!(registry.clients.iter().any(|c| c.client_id == "c9"));
    }

    // ── StreamLines: the only line assembler, and a bounded one ──

    #[test]
    fn stream_lines_holds_a_partial_line_until_the_newline_arrives() {
        let mut lines = StreamLines::new();
        assert_eq!(lines.push("model is at "), [] as [std::string::String; 0]);
        assert_eq!(lines.push("capacity\n"), vec!["model is at capacity"]);
    }

    #[test]
    fn stream_lines_trims_the_carriage_return_of_crlf_output() {
        let mut lines = StreamLines::new();
        assert_eq!(lines.push("a\r\nb\r\n"), vec!["a", "b"]);
    }

    #[test]
    fn stream_lines_keeps_a_split_crlf_intact() {
        let mut lines = StreamLines::new();
        assert_eq!(lines.push("value\r"), [] as [std::string::String; 0]);
        assert_eq!(lines.push("\nnext"), vec!["value"]);
    }

    #[test]
    fn stream_lines_reassembles_the_same_lines_at_every_chunk_boundary() {
        let stream = "model is at capacity\nretrying now\ndone\n";
        for cut in 0..=stream.len() {
            let mut lines = StreamLines::new();
            let mut seen = lines.push(&stream[..cut]);
            seen.extend(lines.push(&stream[cut..]));
            assert_eq!(
                seen,
                vec!["model is at capacity", "retrying now", "done"],
                "split at {cut}"
            );
        }
    }

    #[test]
    fn discarding_keeps_the_partial_line_so_a_late_watcher_still_sees_it() {
        // The handoff case: no watcher is registered while "tar" arrives, one
        // registers, and "get\n" closes the line. With one assembler the line is
        // whole; with two it was split and seen by neither (Codex finding 3).
        let mut lines = StreamLines::new();
        lines.push_discarding("noise\ntar");
        assert_eq!(lines.push("get\n"), vec!["target"]);
    }

    #[test]
    fn discarding_drops_everything_before_the_last_newline() {
        let mut lines = StreamLines::new();
        lines.push_discarding("a\nb\nc");
        assert_eq!(lines.push("\n"), vec!["c"]);
    }

    #[test]
    fn a_line_that_never_ends_cannot_grow_without_bound() {
        let mut lines = StreamLines::new();
        let chunk = "x".repeat(64 * 1024);
        for _ in 0..16 {
            assert_eq!(lines.push(&chunk), [] as [std::string::String; 0]);
        }
        assert!(lines.buffer.len() <= MAX_PENDING_LINE);

        let mut discarding = StreamLines::new();
        for _ in 0..16 {
            discarding.push_discarding(&chunk);
        }
        assert!(discarding.buffer.len() <= MAX_PENDING_LINE);
    }

    /// A line past the cap is dropped whole. Keeping its tail instead published
    /// text that was never a line on the wire, and both engines then matched it:
    /// `/^x+$/` fired for `yxxx…` as soon as the `y` fell off the front.
    #[test]
    fn an_overflowing_line_is_dropped_rather_than_truncated() {
        let mut lines = StreamLines::new();
        assert_eq!(lines.push("y"), [] as [std::string::String; 0]);
        let chunk = "x".repeat(64 * 1024);
        for _ in 0..5 {
            assert_eq!(lines.push(&chunk), [] as [std::string::String; 0]);
        }
        // The newline that ends the oversized line yields nothing, and the
        // stream resumes cleanly on the next one.
        assert_eq!(lines.push("\nafter\n"), vec!["after"]);
    }

    #[test]
    fn an_overflow_while_discarding_also_swallows_only_its_own_line() {
        let mut lines = StreamLines::new();
        let chunk = "x".repeat(64 * 1024);
        for _ in 0..5 {
            lines.push_discarding(&chunk);
        }
        assert_eq!(lines.push("\nafter\n"), vec!["after"]);
    }

    /// End of stream: a command that exits without a final newline still put
    /// that line on the wire. `printf DONE` used to leave `DONE` in the buffer,
    /// so the watcher never saw it.
    #[test]
    fn flush_returns_the_unterminated_tail_at_end_of_stream() {
        let mut lines = StreamLines::new();
        assert_eq!(lines.push("DONE"), [] as [std::string::String; 0]);
        assert_eq!(lines.flush().as_deref(), Some("DONE"));
        // Drained: a second flush has nothing left.
        assert_eq!(lines.flush(), None);

        let mut crlf = StreamLines::new();
        crlf.push("DONE\r");
        assert_eq!(crlf.flush().as_deref(), Some("DONE"));

        let mut clean = StreamLines::new();
        clean.push("DONE\n");
        assert_eq!(clean.flush(), None, "nothing is pending after a full line");
    }

    #[test]
    fn flush_never_returns_an_overflowed_line() {
        let mut lines = StreamLines::new();
        let chunk = "x".repeat(64 * 1024);
        for _ in 0..5 {
            lines.push(&chunk);
        }
        assert_eq!(lines.flush(), None);
    }

    // ── WatcherLineBatcher: throttle without losing a line ──

    const TEST_WINDOW: Duration = Duration::from_millis(100);
    const TEST_CAP: usize = 32;

    fn batcher() -> WatcherLineBatcher {
        WatcherLineBatcher::new(TEST_WINDOW, TEST_CAP)
    }

    fn line(text: &str) -> WatcherLine {
        WatcherLine {
            text: text.into(),
            matched_ids: Vec::new(),
        }
    }

    fn texts(batch: Option<Vec<WatcherLine>>) -> Vec<String> {
        batch
            .unwrap_or_default()
            .into_iter()
            .map(|l| l.text)
            .collect()
    }

    #[test]
    fn batcher_emits_the_first_line_immediately() {
        let mut b = batcher();
        assert_eq!(texts(b.push(line("first"), Instant::now())), vec!["first"]);
    }

    #[test]
    fn batcher_keeps_every_line_inside_the_window() {
        let mut b = batcher();
        let start = Instant::now();
        b.push(line("opening"), start);

        assert!(
            b.push(line("a"), start + Duration::from_millis(10))
                .is_none()
        );
        assert!(
            b.push(line("b"), start + Duration::from_millis(20))
                .is_none()
        );
        assert_eq!(
            texts(b.push(line("c"), start + Duration::from_millis(120))),
            vec!["a", "b", "c"]
        );
    }

    #[test]
    fn batcher_emits_early_rather_than_grow_past_the_cap() {
        let mut b = batcher();
        let start = Instant::now();
        b.push(line("opening"), start);

        let long = "y".repeat(TEST_CAP);
        assert_eq!(
            texts(b.push(line(&long), start + Duration::from_millis(1))),
            vec![long]
        );
    }

    #[test]
    fn batcher_flush_due_drains_a_tail_the_reader_left() {
        let mut b = batcher();
        let start = Instant::now();
        b.push(line("opening"), start);
        b.push(line("tail"), start + Duration::from_millis(5));

        assert!(b.flush_due(start + Duration::from_millis(20)).is_none());
        assert_eq!(
            texts(b.flush_due(start + Duration::from_millis(150))),
            vec!["tail"]
        );
        assert!(b.flush_due(start + Duration::from_millis(300)).is_none());
    }

    #[test]
    fn batcher_take_drains_unconditionally_for_teardown() {
        let mut b = batcher();
        let start = Instant::now();
        b.push(line("opening"), start);
        b.push(line("last"), start + Duration::from_millis(1));

        assert_eq!(texts(b.take()), vec!["last"]);
        assert!(b.take().is_none());
    }

    #[test]
    fn batcher_preserves_order_across_a_window_boundary() {
        let mut b = batcher();
        let start = Instant::now();
        let mut seen = texts(b.push(line("1"), start));
        seen.extend(texts(b.push(line("2"), start + Duration::from_millis(10))));
        seen.extend(texts(b.push(line("3"), start + Duration::from_millis(20))));
        seen.extend(texts(b.flush_due(start + Duration::from_millis(150))));
        assert_eq!(seen, vec!["1", "2", "3"]);
    }
}
