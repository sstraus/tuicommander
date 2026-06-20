# Alacritty Terminal Integration

TUICommander uses `alacritty_terminal` 0.26.0 as its terminal emulation backend. We maintain a local patch at `src-tauri/patches/alacritty_terminal/` referenced via `[patch.crates-io]` in `Cargo.toml`.

## Why a local patch

`alacritty_terminal` is designed for the Alacritty GUI app. Several methods and fields needed by an embedded terminal backend are private. Rather than forking the entire repo, we patch the crate locally — minimal changes, easy to audit, easy to rebase on upstream updates.

## Our patches

| File | Change | Why |
|------|--------|-----|
| `src/term/mod.rs` | `pub fn resize_reflow(size, reflow: bool)` | Disable reflow on resize. Ink/Claude Code uses CUU cursor positioning that breaks when reflow merges/splits lines. |
| `src/term/mod.rs` | `pub fn mark_fully_damaged()` (was `fn`) | Lets us force full-frame damage directly instead of maintaining a parallel flag. |
| `src/term/mod.rs` | `fn osc7770(&mut self, verb, payload)` | OSC 7770 TUIC protocol handler. Fires `Event::Tuic { verb, payload }` for in-band state/suggest/intent signalling. |
| `src/term/color.rs` | `pub fn named_color_to_index(NamedColor) -> Option<u8>` | Maps named colors to xterm-256 indices. Eliminates 30-line match duplication in our serializer. |
| `src/event.rs` | `Event::Tuic { verb, payload }` variant | Carries parsed OSC 7770 events from VTE to the application layer. |
| `src/grid/mod.rs` | `lines_scrolled` field + `pub fn total_scrolled()` | Monotonic count of lines ever scrolled into history (incremented in `scroll_up`). `total_scrolled() - history_size()` gives lines evicted from the top, the base for an eviction-stable absolute row coordinate. Excluded from `PartialEq`; `serde(default)` so old ref fixtures still load. |

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

## Notable forks and patches (external)

### Zed Editor (zed-industries/alacritty)

Zed maintains branches on their fork with patches not yet upstream:

| Branch | What | Relevance |
|--------|------|-----------|
| `osc-133` | Semantic cell tagging — cells get `Osc133CellType` (Prompt/Input/Output) from OSC 133 sequences. Fires `Event::Osc133`. Requires Zed's VTE fork (`osc-133-2` branch). | **High** — would replace our regex-based `extract_osc133()` pre-parser. Enables prompt zone rendering. See story 1552. |
| `v0.16-child-exit-patch` | ~~Uses `exit_status.into_raw()` for `ChildExit`.~~ **Removed from fork (confirmed 2026-05-04).** | Story 1553 needs re-evaluation — implement independently if needed. |
| `use-zed-vte` | ~~Pins to Zed's VTE fork with `Serialize`/`Deserialize` on parser state.~~ **Removed from fork (confirmed 2026-05-04).** | Was prerequisite for OSC 133; check if osc-133 branch still depends on it. |
| `grid-mut` | Makes `grid_mut()` public (removes `#[cfg(test)]`). | **Low** — we already expose grid access via our own patches. |
| `click-links` | URL detection + click-to-open in grid. Ancient branch (pre-0.26 API). | **None** — we handle link detection in our Canvas renderer. |
| `cursor-blink` | Cursor blink timer via `mio::Timer`. WIP with debug prints. | **None** — we handle blink in Canvas/JS. |
| `cursor-config` | Restructures cursor config into `cursor.style`/`hide_when_typing`/`custom_colors`. | **None** — we don't use alacritty's config system. |
| `scrollback` | Added scrollback buffer — already merged into upstream alacritty. | None (already upstream). |
| `scroll/fix-alt-grid-size` | Alt screen gets zero scrollback — already merged upstream. | None (already upstream). |

### Other projects

- **Rio Terminal** — built on alacritty_terminal but maintains its own fork with rendering changes (not relevant to us since we do our own Canvas2D rendering).
- **Ghostty** — uses its own terminal emulation written in Zig, not alacritty_terminal.
- **Warp** — uses `vte` + forked alacritty grid internally, tightly coupled to their `warpui` framework. Not extractable.

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

- **Monthly:** Check crates.io for new alacritty_terminal releases.
- **Quarterly:** Review Zed fork branches for new patches relevant to our use case.
- **On major issues:** If we hit terminal emulation bugs, check if upstream or Zed has a fix before writing our own.

## Planned patches (stories)

| Story | Priority | Description | Status |
|-------|----------|-------------|--------|
| 1552-02ff | P2 | Port Zed OSC 133 semantic cell tagging (requires VTE fork) | **Done** — cell_type tagging + VTE osc133/osc7 handlers implemented |
| 1550-64b1 | P3 | Move OSC 133 extraction into VTE handler (blocked by 1552) | **Done** — VTE routes OSC 133 directly to `Handler::osc133()` |
| — | P2 | OSC 7770 TUIC protocol (state/suggest/intent) | **Done** — full pipeline from VTE→Event→PTY→ParsedEvent |
| — | P3 | Use cell_type for idle detection (OSC 133 shells) | Pending — next step after TUIC protocol |
| 1553-5e8c | P3 | Port Zed child-exit raw waitpid status | Pending |
