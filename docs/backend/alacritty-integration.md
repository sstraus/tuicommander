# Alacritty Terminal Integration

TUICommander uses `alacritty_terminal` 0.26.0 as its terminal emulation backend. We maintain a local patch at `src-tauri/patches/alacritty_terminal/` referenced via `[patch.crates-io]` in `Cargo.toml`.

## Why a local patch

`alacritty_terminal` is designed for the Alacritty GUI app. Several methods and fields needed by an embedded terminal backend are private. Rather than forking the entire repo, we patch the crate locally — minimal changes, easy to audit, easy to rebase on upstream updates.

## Our patches

The patch includes semantic changes listed below as well as rustfmt-only
changes in other files. `term/search.rs` now has a semantic change: it searches
retained zero-width scalars in addition to the base scalar. Do not classify it
as formatting-only when rebasing. The crate is a workspace member with no
local `rustfmt.toml`, so workspace formatting also rewrites upstream style;
review semantic changes separately from that reflow.

| File | Change | Why |
|------|--------|-----|
| `src/term/mod.rs` | Zero-width input damages the actual base cell | Separately received combining marks reach both parse-side changed rows and render frames, including marks attached to a wide base cell. |
| `src/term/search.rs` | Feed base and zero-width scalars to the regex search automaton in both directions | Match exact stored Unicode sequences while returning cell coordinates; TUIC converts buffer-search ranges to UTF-16 for JavaScript consumers. |
| `src/term/mod.rs` | `pub fn resize_reflow(size, reflow: crate::grid::ReflowMode)` | Choose the reflow policy on resize. Ink/Claude Code uses CUU cursor positioning that breaks when reflow merges/splits screen lines. |
| `src/term/mod.rs` | `pub fn mark_fully_damaged()` (was `fn`) | Lets us force full-frame damage directly instead of maintaining a parallel flag. |
| `src/term/mod.rs` | Parse-side damage: `TermParseDamage` enum, `TermDamageState.parse_lines`/`parse_full`, `pub fn parse_damage()`/`reset_parse_damage()`, damage recorded in `write_at_cursor` | A SECOND, independent damage view for TUIC's PTY parse path (`TerminalGrid::process` → `ChangedRow`), read+reset separately from the render damage so the two consumers never steal each other's damage. Lets `process()` diff only changed rows instead of rebuilding+diffing the whole screen per PTY chunk. `write_at_cursor` now damages the written cell (upstream reconstructs input damage lazily at `damage()` time from cursor deltas, which left the parse consumer blind to typed text); this is at worst a safe over-damage for the render consumer. Correctness pinned by the `process_damage_matches_full_diff` differential test. |
| `src/term/mod.rs` | `fn osc7770(&mut self, verb, payload)` | OSC 7770 TUIC protocol handler. Fires `Event::Tuic { verb, payload }` for in-band state/suggest/intent signalling. |
| `src/term/color.rs` | `pub fn named_color_to_index(NamedColor) -> Option<u8>` | Maps named colors to xterm-256 indices. Eliminates 30-line match duplication in our serializer. |
| `src/event.rs` | `Event::Osc133 { command, params, line }`, `Event::Osc7(String)`, `Event::Tuic { verb, payload, line }` variants | Carry parsed OSC 133 / OSC 7 / OSC 7770 events from VTE to the application layer. All three carry the grid `line` where the marker landed. |
| `src/term/mod.rs` | `Config.alt_scrolling_history` + alt-grid history in `Term::new`/`set_options`, era reset in `swap_alt` | Optional alternate-screen scrollback, implemented with Alacritty's separate grids rather than a shared persistent line buffer. Upstream gives the alternate grid capacity 0 (XTerm semantics), so an app printing more than a screenful (`gh run watch`, `less`, `man`) loses whatever scrolls off. The field defaults to `0`, preserving upstream behavior for consumers that do not opt in; TUICommander uses the primary cap. Each enter/exit starts a fresh alternate era, so sessions never inherit one another and no alternate lines remain logically retained after exit. Oversized repeated redraws remain repeated because the emulator is byte-faithful, not a semantic snapshot deduplicator. |
| `src/term/mod.rs` | `pub fn primary_total_scrolled()` | Returns the primary grid origin including evicted rows, selecting the inactive primary grid during alternate-screen use. Durable-log extraction and resize synchronization use this origin to keep capturing after the history cap. |
| `src/term/mod.rs` | `pub fn primary_history_size()` | Returns primary-grid history even while the alternate grid is active. Durable-log resize synchronization must stay in this coordinate space; using active alternate history can suppress the first normal-shell lines after exit. |
| `src/grid/row.rs`, `src/grid/mod.rs`, `src/grid/resize.rs`, `src/term/mod.rs` | `Row::copy_origin_unknown` | Per-row copy provenance: predecessor eviction/purge makes the oldest retained origin unknown; full row reset/erase restores it. Selection cleanup consults this flag without changing absolute row counters. |
| `src/grid/mod.rs` | `pub fn reset_history_era()` | `clear_history()` keeps `lines_scrolled` monotonic because absolute row ids must be stable for the life of a physical line. The alternate screen is a separate content universe wiped on every enter/exit, so it gets a fresh era instead: history *and* counter reset. Frame-protocol `keyboard_flags` bit 5 marks the transition; the frontend then atomically invalidates row, scroll, selection, search, and link state. |
| `src/grid/mod.rs` | `lines_scrolled` field + `pub fn total_scrolled()` | Monotonic count of lines ever scrolled into history (incremented in `scroll_up`). `total_scrolled() - history_size()` gives lines evicted from the top, the base for an eviction-stable absolute row coordinate. Excluded from `PartialEq`; `serde(default)` so old ref fixtures still load. |
| `src/grid/mod.rs` | `pub enum ScrollSource` + `pub fn scroll_up_with(region, positions, source)` | Says *why* lines move up, because the region cannot: a linefeed past the bottom margin and a DL at the top row both present a region that starts at line 0. `Overflow` feeds history, advances `lines_scrolled` and shifts a scrolled-back viewport; `Control` swaps within the region and moves no counter. `scroll_up` still delegates as `Overflow`, so `resize.rs` and `clear_viewport` are untouched (#834-1878). |
| `src/grid/resize.rs` | `pub enum ReflowMode { None, All, HistoryOnly }`; `Grid::resize` and `grow_columns`/`shrink_columns` take it instead of `bool` | `HistoryOnly` is the mode TUIC runs: scrollback stays readable across resize cycles while visible screen rows are truncated/padded, so a cursor-addressed TUI is not re-wrapped under itself. The screen/history boundary is derived from the post-`rezero()` storage layout (`i < self.lines` is screen). |
| `src/grid/row.rs` | `Row.reflow_wrap: bool` (`serde(default)`) | Marks a wrap produced by *this* shrink. History rows only merge when it is set, so a stale natural wrap from an earlier width is never joined. Reset by `Row::reset`, propagated when a row is absorbed. |
| `src/grid/storage.rs` | `debug_assert_eq!(size_of::<Row<T>>(), size_of::<usize>() * 5)` (was `* 4`) and the matching cache loop bound | Consequence of the extra `Row` field — the assertion is upstream's guard against an accidentally fat `Row`. |
| `src/term/cell.rs` | `pub enum Osc133CellType { None, Prompt, Input, Output }` + `Cell.cell_type` (`serde(default)`) | Semantic cell tagging from OSC 133 A/B/C/D, written by `Term::osc133` and read by `TerminalGrid` to emit prompt/input/output zones. Replaces the regex pre-parser TUIC used to run over raw output. |
| `src/grid/tests.rs` | Upstream resize tests migrated to `ReflowMode`; new `shrink_reflow_history_only` case | Keeps upstream's reflow coverage green after the signature change and pins the new mode. |
| `src/term/cell.rs` | `CellExtra.zerowidth: ArrayVec<char, MAX_ZEROWIDTH_CHARS>` (was `Vec<char>`), `push_zerowidth` uses `try_push`, `clear_wide` assigns `ArrayVec::new()`; direct `arrayvec` dep | **Backport of upstream `ede2ac14`** (2026-08-26, master only — 0.26.0 predates it, so this is not yet available from crates.io). The unbounded `Vec` let a single cell absorb combining marks forever (`echo -en a; while true; do echo -en '\xcc\x81'; done`), a memory-exhaustion vector any PTY child can reach. Overflow now drops the character instead of allocating. Bound is 9, upstream's value — no glyph cluster we render needs more, and `zerowidth()` still hands out a `&[char]` so no caller changed. **This row exists to stop the next rebase silently reverting the fix:** delete it only once the version we pin actually contains `ede2ac14`. `arrayvec` was already in the lock via `vte`, so the dep costs no new crate. |
| `src/grid/row.rs` | Safe `Row::new` using `iter::repeat_with(T::default).take(columns).collect()` | **Backport of upstream `d692748d`** (2026-08-31, master only; 0.26.0 predates it). Removes the unsafe initializer and its hidden nonzero-column requirement. A zero-column row is now empty. Keep this backport until the pinned upstream release includes it. |
| `src/grid/row.rs` | `grow` reserves exactly the requested additional columns; `shrink` returns allocation slack in both the retained row and its nonempty tail | Width changes no longer double row capacity or retain the previous wide allocation after shrinking. The cell values and reflow contract are unchanged. |
| `src/grid/storage.rs` | `MAX_CACHE_SIZE = 32` (upstream: 1,000) | Bounds spare allocated rows while retaining batched allocation during scrollback growth. At 220 columns and 24 bytes per cell, the spare-cell budget falls from 5,280,000 to 168,960 bytes per grid, before allocator rounding. See the scrollback memory measurement below. |
| `src/tty/unix.rs` | `ShellUser::from_env` calls `getpwuid_r` only when `USER`/`HOME`/`SHELL` is missing | Upstream resolves the passwd entry unconditionally on every PTY spawn. TUIC spawns many PTYs; the lookup is skipped when the environment already answers. |
| `src/term/mod.rs` | IL/DL reset the cursor column; ICH/DCH/ECH clear the pending wrap; ED0 spares the cell behind a pending wrap | Three inherited divergences from the DEC contract, found by the ANSI differential harness (`tests/terminal-stress/INTEGRITY_FINDINGS.md`). They are described one row below; each is pinned by a `term::tests` case that names the operation. |
| `src/term/mod.rs` | `fn scroll_up_overflow()`, separate from `Handler::scroll_up`; DL and SU scroll as `ScrollSource::Control` | DL and SU remove lines that never reached the bottom, so TUIC gained scrollback rows the agent never printed — an agent TUI that repaints with DL manufactured history. `wrapline` and `advance_line` called the same `Handler::scroll_up` the SU control dispatches to, so routing that one method would have stopped a linefeed feeding history as well; the overflow path gets its own entry point instead. Replaying the retained ANSI captures moves the row-count-grew residual bucket from 205 to 0 (#834-1878). |

### The pending wrap and the edit operations

A character written into the last column leaves the cursor there and arms
`input_needs_wrap` instead of moving past the margin. Which operations resolve
that pending state is not a matter of taste — it decides where the next
character lands and whether the last visible cell survives. Upstream Alacritty
answers three of them differently from DEC and from xterm:

| Operation | Contract | What upstream did |
|---|---|---|
| IL (`CSI Ps L`), DL (`CSI Ps M`) | return the cursor to the left margin, but only when the line is inside the scroll region; clear the pending wrap either way | left the column stranded mid-row, so the rest of the line painted at the wrong offset |
| ICH (`CSI Ps @`), DCH (`CSI Ps P`), ECH (`CSI Ps X`) | edit the cell under the cursor, therefore resolve the pending wrap | carried the wrap past the edit, so the next character jumped to the following row |
| ED0 (`CSI 0 J`) | the erase origin is past the right margin, so the current line keeps its last cell; lines below still clear | erased that cell, dropping a character the user had already seen |

ED1 pending-wrap behavior, EL0, EL1 and EL2 already agreed with the reference
and are deliberately unchanged — `clear_line` has carried the `LineClearMode::Right` guard all along.
Do not "fix" them alongside the three rows above; `term::tests` pins their
current behaviour for exactly that reason.

## VTE patch (`src-tauri/patches/vte/`)

We also patch the `vte` crate (0.15.0) to extend the `Handler` trait:

| Method | Purpose |
|--------|---------|
| `fn osc133(&mut self, command: char, params: &str)` | Shell integration markers (A/B/C/D). Routes OSC `133;X` from `osc_dispatch`. |
| `fn osc7(&mut self, url: &str)` | Current working directory. Routes OSC `7;url` from `osc_dispatch`. |
| `fn osc7770(&mut self, verb: &str, payload: &str)` | TUIC protocol. Routes OSC `7770;verb=payload` from `osc_dispatch`. |

## OSC 7770 — TUIC Protocol

In-band signalling via the PTY stream. Never written to the grid (consumed by VTE before rendering).

**Format:** `ESC ] 7770 ; verb=payload BEL` or `ESC ] 7770 ; verb=payload ST`

**Verbs:**

| Verb | Payload | Effect |
|------|---------|--------|
| `state` | `idle`, `busy`, or `awaiting` | `idle`/`busy`: immediate shell state transition (bypasses silence timer). `awaiting`: emits a confident `Question` (sets `awaiting_input`); `busy` also clears a prior `awaiting`. Driven by native agent hooks (see AI Agents → Native Hook Instrumentation). Unknown payloads are ignored. |
| `suggest` | `A\|B\|C` (pipe-separated) | Emits `ParsedEvent::Suggest` — never hits the grid, no conceal needed. |
| `intent` | `text` or `text (Title)` | Emits `ParsedEvent::Intent` with optional tab title. |

**Advantages over text-based detection:**
- Zero cross-chunk issues (OSC has delimiter-based framing in VTE)
- Zero conceal (never written to grid cells)
- Zero regex (structured parse in VTE dispatcher)
- Zero stale rescan (not in visible buffer)

## Upstream API we use directly (no patch needed)

| API | Usage |
|-----|-------|
| `Term::new(config, dimensions, event_proxy)` | Create terminal grid |
| `Processor::advance(&mut term, data)` | Feed PTY bytes |
| `term.grid()` / `term.grid_mut()` | Read cell grid, cursor, history |
| `term.damage()` / `term.reset_damage()` | Dirty-row tracking for incremental serialization |
| `term.scroll_display(Scroll::Delta)` | Viewport scrolling |
| `term.mode()` | Check TermMode flags (ALT_SCREEN, SHOW_CURSOR, kitty keyboard) |
| `term.cursor_style()` | Cursor shape (block/beam/underline) |
| `term.colors()` | Dynamic color palette (OSC 4/10/11/12 overrides) |
| `term.selection` / `term.selection_to_string()` | Native selection API |
| `RegexSearch::new(query)` + `term.regex_search_right()` | Native DFA regex search across grid + scrollback |
| `EventListener` trait | Capture bell, title, clipboard, PTY write-back events |

Canonical HTTP text snapshots read a single absolute range from this grid.
They must not rebuild a snapshot by appending a separately retained log to the
screen: increasing terminal rows can move history back into the viewport and
make the two representations overlap.

Clipboard selections use the same absolute grid coordinates. TUICommander joins
rows marked with `WRAPLINE`, preserves real spaces at wrapped margins, omits
synthetic `WIDE_CHAR_SPACER` and `LEADING_WIDE_CHAR_SPACER` cells from text and
text-length accounting, trims unwrapped terminal padding, and then removes only
coherent multi-line Claude space/NBSP `▎` visual gutters. Claude composer
selections remove one leading `❯ ` and two continuation-margin columns only
when the ordered selection starts at grid column zero with the marker in the
first two cells and the preceding row has no `WRAPLINE`. Partial body selections
and VT soft-wrap continuation origins remain literal. At every retained
row, cleanup requires known row provenance. A single `copy_origin_unknown`
flag prevents cleanup even when RI/IL moves the physical row. Eviction or purge
of nonblank predecessor content sets it on the surviving boundary row, including
a blank survivor; removing only blank rows does not set it. Cap trimming,
reprint-tail removal and reflow truncation use the same content-loss rule.
There is no separate latent flag, blank-history exemption or resize promotion.
Full row replacements (ECH, DCH, ICH, EL or ED covering every column from
column zero, and DECALN alignment-screen replacement) share `Row::reset`, which
clears the flag without changing absolute row counters. Partial edits preserve it.
A composer redrawn on a blank loss boundary can retain its `❯ ` in copied text
until the row is fully erased. This cosmetic limitation is intentional: literal
content takes priority over speculative cleanup of an ambiguous origin.
ED1 resets every row above the cursor, including row zero when the cursor is on
row one; upstream skips that row with its `cursor.line > 1` guard. The cursor row
is erased only through its current column, and rows below remain unchanged.
ED2 resets all visible rows after scrolling occupied content into history. This
clears loss flags that overflow can attach to a fresh blank row with zero scrollback,
while retained history keeps its provenance. ED3 does not erase live content.
DECALN fills live rows with default-background E cells after resetting provenance;
it does not inherit the active erase background. Ordinary printing, including
insert-mode shifts and a sequence that overwrites every cell, preserves unknown
provenance because it has no explicit whole-row replacement boundary.
RI/IL retains the moved row's flag and literal copy behavior at its new position.
Unknown content stays literal; the same width-evidence rule rejoins wraps
while preserving short typed lines and deeper content indentation. Pasted prompt
glyphs remain content. This normalization is
outside the Alacritty fork and is shared by desktop IPC and HTTP clients.

Atomic MCP agent-submission receipts require no Alacritty patch. The receipt
boundary is the raw child-output ring offset captured before Enter; after that
offset moves, the current agent screen adapter may classify the already-built
grid as working, ready, or interrupted. Neither grid mutation nor local input
bookkeeping is itself an acknowledgement.

## Notable forks and patches (external)

### Zed Editor (zed-industries/alacritty)

Zed maintains branches on their fork with patches not yet upstream:

| Branch | What | Relevance |
|--------|------|-----------|
| `osc-133` | Semantic cell tagging — cells get `Osc133CellType` (Prompt/Input/Output) from OSC 133 sequences. Fires `Event::Osc133`. Requires Zed's VTE fork (`osc-133-2` branch). | **None — already implemented in our own patch** (`term/cell.rs` + `Term::osc133` + our VTE `osc133` hook). The regex pre-parser this was meant to replace no longer exists. |
| `v0.16-child-exit-patch` | ~~Uses `exit_status.into_raw()` for `ChildExit`.~~ **Removed from fork (confirmed 2026-05-04).** | Story 1553 needs re-evaluation — implement independently if needed. |
| `use-zed-vte` | ~~Pins to Zed's VTE fork with `Serialize`/`Deserialize` on parser state.~~ **Removed from fork (confirmed 2026-05-04).** | Was prerequisite for OSC 133; check if osc-133 branch still depends on it. |
| `grid-mut` | Makes `grid_mut()` public (removes `#[cfg(test)]`). | **Low** — we already expose grid access via our own patches. |
| `click-links` | URL detection + click-to-open in grid. Ancient branch (pre-0.26 API). | **None** — we handle link detection in our Canvas renderer. |
| `cursor-blink` | Cursor blink timer via `mio::Timer`. WIP with debug prints. | **None** — we handle blink in Canvas/JS. |
| `cursor-config` | Restructures cursor config into `cursor.style`/`hide_when_typing`/`custom_colors`. | **None** — we don't use alacritty's config system. |
| `scrollback` | Added scrollback buffer — already merged into upstream alacritty. | None (already upstream). |
| `scroll/fix-alt-grid-size` | Alt screen gets zero scrollback — already merged upstream. | None (already upstream). |

## Update procedure

### Checking for upstream updates

```bash
# Check latest version on crates.io
cargo search alacritty_terminal

# Compare with our pinned version
grep "alacritty_terminal" src-tauri/Cargo.toml
```

### Rebasing our patch on a new upstream version

1. Download the new version:
   ```bash
   cargo download alacritty_terminal@<new_version> -o /tmp/alacritty_new
   ```
   Or copy from `~/.cargo/registry/src/` after adding the new version to Cargo.toml.

2. Diff our patches against the old upstream:
   ```bash
   diff -ru ~/.cargo/registry/src/*/alacritty_terminal-0.26.0/src/term/mod.rs \
            src-tauri/patches/alacritty_terminal/src/term/mod.rs
   ```

3. Apply patches to the new version. Our changes are small and isolated:
   - `resize_reflow` in `term/mod.rs` — add method, modify `resize()` to call it
   - `mark_fully_damaged` visibility in `term/mod.rs` — `fn` → `pub fn`
   - `named_color_to_index` in `term/color.rs` — new function, no existing code modified

4. Update `Cargo.toml` version and the `patches/` directory.

5. Run tests: `cargo test terminal_grid && cargo test vt_log`

### Checking Zed's fork for new patches

```bash
# List branches on Zed's fork
gh api repos/zed-industries/alacritty/branches --jq '.[].name'

# Compare a specific branch
# https://github.com/zed-industries/alacritty/compare/master...<branch>
```

### Periodic review cadence

Driven by the `alacritty-upstream` entry in `.claude/scheduled-checks.json` (every 20 days). Each run:

- Check crates.io for new alacritty_terminal releases (`cargo search alacritty_terminal`).
- Review Zed fork branches for new patches relevant to our embedded backend.
- **On major issues:** If we hit terminal emulation bugs, check if upstream or Zed has a fix before writing our own.

**Deliberately not ported** (re-evaluate only if the reason changes):

| Upstream | What | Why we skipped it |
|----------|------|-------------------|
| `d692748d` (2026-08-31) | Replaces the `unsafe` hand-rolled `Vec` fill in `grid/row.rs` `Row::new` with `iter::repeat_with`, dropping the undocumented `columns >= 1` limit. | Pure cleanup with no user-visible defect, and it collides with our `reflow_wrap` patch: the hunk's closing context is the `Row { inner, occ: 0 }` literal, which in our fork carries a third field. TUIC never builds a zero-column row, so the unsoundness it fixes is unreachable here. Take it for free at the next version bump instead of hand-merging it now. |

## Planned patches (stories)

| Story | Priority | Description | Status |
|-------|----------|-------------|--------|
| 1552-02ff | P2 | Port Zed OSC 133 semantic cell tagging (requires VTE fork) | **Done** — cell_type tagging + VTE osc133/osc7 handlers implemented |
| 1550-64b1 | P3 | Move OSC 133 extraction into VTE handler (blocked by 1552) | **Done** — VTE routes OSC 133 directly to `Handler::osc133()` |
| — | P2 | OSC 7770 TUIC protocol (state/suggest/intent) | **Done** — full pipeline from VTE→Event→PTY→ParsedEvent |
| — | P3 | Use cell_type for idle detection (OSC 133 shells) | Pending — next step after TUIC protocol |
| 1553-5e8c | P3 | Port Zed child-exit raw waitpid status | Pending |

### Stored terminal marker coordinates

OSC 133 and OSC 7770 event rows use `grid.total_scrolled() + cursor row`. Capture this origin inside the OSC handler, before later bytes in the same chunk can evict history. Retained history size is not an absolute origin.

## Scrollback memory and the single-grid contract

`VtLogBuffer` owns the session's only `TerminalGrid`. That grid backs desktop
frames, HTTP scrollback, copy and search, as well as log extraction. Its
10,000-row history is user-visible history; it is not a duplicate scratch grid
that can be capped independently. The separate 10,000-line `LogLine` deque serves
mobile pagination. Reducing its grid history would delete terminal scrollback.

The 2026-10-07 six-session measurement uses 220 columns and full history,
including a resize cycle to expose retained row capacity. The workload and native
RSS, `vmmap`, `footprint`, `heap`, and diagnostics measurements are recorded in
story `1575-2c7b`. Row cache sizing retains a 32-row allocation batch instead of
allocating one row at a time. This patch changes allocation retention, not the
history limit or transport behavior.

| Metric (six sessions after the width cycle) | Before | After |
|---|---:|---:|
| RSS (KiB) | 977,584 | 966,176 |
| Physical footprint (bytes, diagnostics) | 1,118,438,912 | 596,067,624 |
| Malloc bytes in use (diagnostics) | 822,958,480 | 454,159,264 |
| Heap bytes (`heap -s`) | 822,623,440 | 453,913,968 |
| `LogLine` bytes (diagnostics estimate) | 14,638,752 | 14,638,752 |

The requested 50% RSS reduction was not reached: RSS fell by 1.17%, while
malloc bytes in use fell by 44.81% and physical footprint by 46.71%. The after
`footprint` report also classified 355 MB of Malloc Small as reclaimable; RSS
alone does not show the live-allocation reduction. Heap row allocations changed
from approximately 60,294 12 KiB blocks to 60,295 6 KiB blocks. At that measured
6 KiB per 220-column row, the spare-row bound is 6,144,000 bytes per grid before
and 196,608 bytes after. This is a bound calculation, not a separate measurement
of the cache's contribution to the total reduction.

The largest remaining payload is the retained terminal grid: `(10,000 + 24) ×
220 × 24 = 52,926,720` nominal cell bytes per session, before inactive-screen
rows, spare rows and allocator rounding. Compact cells or file-backed history
need a separate decision; neither is included in this patch.

### Retained wrap edges without reflow

Non-reflow width growth preserves the original cells, including a `WRAPLINE`
flag at the old margin. Blank columns added to the right do not cancel that
edge. The styled-range and live-frame serializers inspect the full retained
row for the wire wrap bit, even when a dirty frame ships only a partial span
(#2560-b448). This keeps answer gutters on continuation rows without changing
the configured reflow policy, cell positions, or copy semantics.

### Resize repaint reconciliation

Every primary-screen resize records the source/target column widths, grid
sequence, original viewport-top row index and the viewport's own row snapshot
(#1407-1ab2). Live VtLogBuffer width changes use full reflow so visible
logical lines retain their complete content and soft-wrap continuity.
History-only reflow truncates visible cells on shrink and leaves natural wrap
flags at the old column on growth, breaking session-output redaction. The
cursor-addressed Ink redraw remains a separate replacement domain for the
reconciliation below.

The first explicit full viewport erase can replace an old displaced row only
when that complete physical row occurs in the new replacement. A two-row
viewport anchor does not license removing other old rows.

Full reflow can move the original viewport prefix into history even when the
height is unchanged. A unique complete two-row source prefix locates that
owned range in the reflowed coordinates; incomplete or ambiguous matches
retain the rows. Each old row still needs complete replacement proof.

Blank screen-prefix rows created by proven suppression retain explicit
provenance. A write, erase, scroll or alternate-screen switch touching them
invalidates that provenance, even if the visible text remains blank. Before
the next full reflow, only still-owned synthetic blanks are removed without
adding history. Program-authored separators are never included.

A fresh redraw prefix is suppressed only when its entire text equals a contiguous
suffix of an immutable history snapshot ending immediately above the owned
viewport. The snapshot includes at most four original viewport heights, capped
at 256 rows. Only its first row may start inside a source logical line, with a fragment
of at least 16 visible characters; all following content must match exactly
through the anchor. Short fragments, non-suffix matches, gaps or missing
anchors preserve the entire prefix. Trailing historical blank separators remain
in the comparison: a redraw omitting that gap provides no replay proof.
Leading blank rows owned by the viewport remain in its replacement domain,
so the prefix boundary excludes their replacement. This path never deletes old history. A genuinely new prefix
identical to that complete retained suffix is indistinguishable from a replay:
its older copy remains, but event multiplicity is lost. This bounded residual
is explicit and regression-tested.

Height growth records the exact history coordinates pulled into the new viewport,
so their new redraw is retained. Matching preserves intra-line whitespace and
uses the shared greedy-width rule for Ink's hard-line word wrapping. The bounded
snapshot retains recorded source widths for unchanged hard rows across subsequent
width changes; native VT soft wraps are joined before comparison, and blank
paragraph separators remain explicit. Fresh
visible duplicates are blanked without shifting child row coordinates.
Alternate screens and ordinary edits provide no replacement authority.
The recorded streaming and idle PTY captures and resize byte timelines live in
`crates/tuic-terminal/src/fixtures/claude-resize-1407/`.

Resize-owned physical fragments retain their complete logical-record identity from
the post-resize WRAPLINE boundaries, before a redraw can erase a continuation still
on screen. Replacement proof must match that complete record; a substring in a
different replaced record never licenses deleting an omitted history record.

### History-anchored viewport scrolling

History-anchored scrolling uses `TerminalGrid::screen_origin()` (`total_scrolled`) and `scroll_to_anchor`; it resolves the all-time row to a display offset under the VT lock. The existing fork origin and history cap provide the coordinate; no new fork patch is required.

### Soft-wrap extraction contract

`TerminalGrid` uses the existing alacritty WRAPLINE flag for text and styled-log extraction. Wrapped rows retain their trailing cells, including a real space in the final column. Non-wrapped rows still trim unused terminal padding. No fork patch is needed: the grid already carries the required boundary. The log wire format exposes this flag as `wrapped` for mobile output.
