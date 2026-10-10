# CanvasTerminal Feature Audit

**Last updated:** 2026-09-21
**Branch:** refactor/solid-architecture

CanvasTerminal is the sole terminal renderer. xterm.js has been fully removed. The renderer is powered by `alacritty_terminal` (Rust) sending binary grid frames over a Tauri Channel (desktop) or WebSocket (browser/PWA).

Remote grid streams report handshake, initial-replay timeout (15 seconds), decode and unexpected-close failures through the WS-only `onStreamError` callback (never a Tauri PTY listener). CanvasTerminal displays a persistent error toast. Reconnect health resets only after the renderer receives a frame or the server explicitly finishes an empty replay, not at socket open; intentional teardown cancels replay and reconnect timers. No silence deadline applies after initial replay. Replay timeouts detach the socket and schedule recovery directly, even when its close handshake never completes. The recorded idle remote replay regression asserts visible viewport text after reconnect without new PTY output. The backend WS attach regression separately verifies existing scrollback text at both the live viewport and a history offset without publishing a new frame (story 1421-733e); these prove recovery and replay, not attribution of the historical blank pane.

Answers-only history includes a retained prompt-less prefix before the first tracked prompt. Finished prefix and prompt turns share the history-base cache, with prompt association included in its validity check. The panel shows a status notice when no turn has a marked answer.

Submitted prompts are not tinted in the terminal grid. Recorded `userPromptLines` still drive scrollbar ticks and answers-only turn grouping; Chat user messages retain the theme prompt tint and gutter.

Scrollbar history and user-prompt ticks depend only on `showScrollbarMarks`; Ctrl+Cmd controls timestamp text independently. Input/window blur clears the timestamp peek even if keyup is missed. Empty marks overlays are omitted, and zero grid history hides the scrollbar. Search ticks remain independent of the history setting.

## Architecture

```
Terminal.tsx (outer shell)
  +-- Session lifecycle (create/resume/reconnect PTY)
  +-- Parsed event handling (status-line, question, progress, etc.)
  +-- Activity tracking, notifications, auto-retry
  +-- OSC 0/2 title change handling
  +-- Resume/reconnect banners (JSX)
  +-- ComposePanel
  +-- TerminalSearch
  +-- TerminalRef registration with terminalsStore
  |
  +-- CanvasTerminal (sole renderer)
        +-- subscribe_terminal_grid (binary frame push from Rust via Channel/WS)
        +-- Base canvas: text cells, backgrounds, block/box-drawing chars
        +-- Overlay canvas (pointer-events:none): cursor, selection, search highlights, gutter markers
        +-- Custom scrollbar (drag + track click)
        +-- Suggest/intent overlay (DOM divs over canvas)
        +-- Link detection (hover: file paths, web URLs, OSC 8)
        +-- Keyboard input (VT100 + Kitty protocol)
        +-- Touch input (tap/swipe/pinch for mobile/tablet via shared input)
        +-- IntersectionObserver flow control (skip paint when hidden)
        +-- Plugin watcher lines from Rust (pluginRegistry.handleWatcherLines)
        +-- OSC 7 CWD + OSC 133 shell integration
        +-- Imperative controllers (no reactive frame-path state)
              +-- selection + search
              +-- cancellable link verification + caches
              +-- smooth-scroll position + styled-row cache
              +-- keyboard/IME/mouse listener lifecycle
```

Frame decode, row reconciliation, scheduling, and paint remain colocated in
`CanvasTerminal`. The extracted controllers own independent state and cleanup;
they do not add Solid signals, effects, or store writes to the render hot path.

Key insight: Terminal.tsx handles parsed events, session lifecycle, banners, and compose panel. CanvasTerminal is purely a renderer + input handler with no session logic.

## Binary Frame Format

Each frame: 26-byte header + variable row data. The header ends with a `historyBase: u32` (lines evicted from the history top so far); `historyBase + (historySize - displayOffset + screenRow)` is the eviction-stable absolute index the smooth-scroll row cache keys by, so a cached row never aliases onto a different line after the scrollback cap rotates. `keyboard_flags` bits 0–4 remain the public keyboard-mode mask; bit 5 carries the active primary/alternate-screen identity and is removed before exposing `keyboardFlags` to input code. Per cell: 4 bytes codepoint + 3 bytes fg RGB + 3 bytes bg RGB + 1 byte attrs bitmask = 11 bytes. Decoded in `decodeBinaryFrame` using struct-of-arrays (SoA) typed arrays — zero per-cell object allocation.

Primary and alternate grids can reuse identical numeric row coordinates while representing unrelated content. A bit-5 transition therefore starts a new renderer generation: smooth-scroll animation, delayed row fetches, selection, search, link verification, reconciliation, and absolute-row caches are invalidated as one transaction. Partial transition frames wait for a full replacement instead of merging into the previous grid.

### Cell text extension (`TCX1`)

Live frames (including full resynchronization) and styled-row payloads retain
their existing headers, row records and 11-byte cell core. When at least one
transmitted cell has zero-width scalars, an optional trailer follows **all** row
records. Every integer is little-endian:

| Field | Type | Meaning |
|---|---|---|
| Magic | 4 bytes | ASCII `TCX1` |
| Entry count | `u32` | Number of sparse cell extensions |
| Row ordinal | `u16` | Zero-based ordinal in this payload's row records |
| Column | `u16` | Absolute grid column, including for a partial row |
| Scalar count | `u8` | 1–9 retained zero-width scalars |
| Scalars | `u32[count]` | Unicode scalar values in stored order |

The last four fields repeat per entry. No trailer means no extensions. New
clients accept legacy payloads; legacy clients stop after the declared rows and
ignore the trailer. The same serializer serves IPC and WebSocket delivery.
Entries must name distinct cells inside their transmitted row spans and contain
valid Unicode scalars (no surrogates or values above U+10FFFF). Truncated records,
empty trailers, invalid bounds/counts, duplicate entries and trailing bytes are
rejected and trigger full-frame recovery.

Decoded rows retain sparse cell strings and cached UTF-16 cell boundaries.
Partial updates clear prior extensions inside the transmitted span before
installing new ones, so overwriting an accented cell cannot leave a stale mark.
Complete cell strings feed painting, block-cursor repaint, selection/copy and
link offsets. For offsets returned with backend row/logical text, the renderer
omits wire-zero wide spacers, validates the reconstructed text against the
backend string and declines a mapping on mismatch. Native blank cells contain
a space; PTY NUL controls do not become printed cells. Marked box/block glyphs use font shaping rather than geometric
painting, preserving their marks and variation selectors. Terminal search
geometry remains in grid cells; buffer-search string ranges use UTF-16 offsets. Ranges cover whole
matching cells: a regex matching only a combining mark still reports its base
cell's full text span, since native search points have no subcell index.

## Performance Notes

- **Selection row identity:** Endpoints use `historyBase + historySize -
  displayOffset + viewportRow`, the same eviction-stable identity as the styled
  row cache. Search and command-block coordinates keep their separate
  grid-relative contract. Copy converts endpoints back to grid-relative rows
  and includes the displayed `historyBase`; Rust rebases them under the grid
  lock, rejecting evicted endpoints. Reflow and primary/alternate transitions
  invalidate selection.
- **Selection revalidation:** Every new gesture discards the previous copied
  text snapshot. Full-frame content comparison applies only to released,
  fully visible selections; a range still being dragged is expected to change.
- **Coherent frame replacement:** A changed viewport origin cannot borrow
  untouched columns from the previous viewport. A screenful of partial spans
  is still a delta, not an authoritative replacement. While requesting a full
  replacement, text and selection painting retain the same accepted viewport
  instead of clearing the row map and exposing missing rows to mouse-driven
  repaints. History growth that leaves the stable viewport origin unchanged
  does not itself invalidate that row map.

- **RAF coalescing:** All paint triggers (frame arrival, keydown selection clear, mousedown) go through `scheduleRepaint()` which schedules a single `requestAnimationFrame`. No synchronous paint calls — prevents double-paint in a single event loop turn.
- **`send_grid_frame` clone guard:** Frame is only cloned for the `grid_watch` channel when `receiver_count() > 0` (i.e. WS clients connected). Desktop-only path (Tauri Channel) is zero-copy.
- **Raw bytes over the IPC:** the channel carries `tauri::ipc::Response`, not `Vec<u8>`. A bare `Vec<u8>` matches Tauri's blanket `IpcResponse` impl and is serialised as a JSON array of decimal numbers — a 110 KB frame becomes a ~250 KB string plus an extra IPC round trip, and JS receives a `number[]` to walk instead of an `ArrayBuffer`. Same for `terminal_styled_rows`. `toBinaryPayload` still accepts `number[]`, because Tauri's postMessage fallback (custom-protocol IPC blocked) delivers that shape.
- **Frame acks are counters, not a flag:** the frontend echoes its total receipt count, so an ack for a frame the ticker already abandoned is a number in the past and cannot release a burst. See `tuic-terminal/src/grid_gate.rs` for pure ordering and `src-tauri/src/grid_watch.rs` for the Tokio adapter.
- **Visibility batches:** `IntersectionObserver` can deliver multiple states for
  a terminal in one callback after resize/layout changes. `latestIntersectionVisibility`
  selects the greatest `entry.time` (last entry wins ties), so a transient hidden
  sample cannot suppress later frames while the canvas is visible. Existing
  hidden-tab ACK throttling and the full-frame request on show remain unchanged.
- **Hidden terminals:** a background tab is `display:none` and never unmounted, so its producer keeps running. It decodes each frame (the bell rides in the header) and skips paint, links and cache fill, then acks on a 400 ms trailing timer — enough to keep the gate moving at ~2 frames/s without making the backend log a stuck frontend. Reconciliation (`shouldFireReconcile`) is off for it entirely: each fire would build and drop a full frame.
- **Row cache bounds:** `cacheRows()` evicts FIFO at `ROW_CACHE_MAX`, so the *fill* path is bounded too — trimming on scroll alone left an unbounded map for a session that only ever fetched forward.
- **`screen_text_rows_ref()`:** `TerminalGrid` exposes a borrowed `&[String]` view of cached screen rows. Used in `process_chunk` for chrome cutoff detection to avoid cloning 50 Strings per PTY chunk. Downstream parsers (slash-menu, choice-prompt) share a single owned snapshot computed once per chunk.
- **Plugin OutputWatcher matching moved to Rust:** the reader thread assembles the lines, cleans them, and tests the compiled patterns (`tuic-terminal/src/output_watchers.rs`). Rust is now the only line assembler — the per-session `LineBuffer`s and the raw-output listener are gone, so the thread that paints the terminal no longer ANSI-strips and regex-tests every line. It is woken once per 100 ms window with the lines that matched. A pattern the `regex` crate cannot express (lookaround, backreferences) is reported back as rejected and keeps matching in the WebView, which then receives every assembled line.
- **No per-chunk parser logs:** Slash-menu detection can remain active during a large output burst, so its hot path emits events only when a menu is found and never writes a debug record for every parse.
- **Trim in-place:** `read_screen_text()` and `row_to_text()` use `String::truncate()` instead of `.trim_end().to_string()`, eliminating one allocation per row.

## Feature Table

### Rendering

| Feature | Status | Notes |
|---------|--------|-------|
| Cell rendering (text + colors) | OK | `fillText` per cell, SoA typed arrays |
| Bold / italic / dim / underline / strikeout | OK | |
| Inverse video | OK | `resolveFg/resolveBg` swap |
| Block elements (U+2580-259F) | OK | `drawBlockChar()` draws as geometry |
| Box-drawing (U+2500-257F) | OK | `drawBoxDrawingChar()` draws as geometry |
| Ligatures | OK | Adjacent cells with matching attrs grouped into text runs |
| Cursor shapes (block/beam/underline) | OK | `computeCursorRect()` |
| Cursor blink | OK | 700ms interval, reset on keypress |
| Unfocused cursor (outline) | OK | `strokeRect` |
| Overlay canvas (cursor+selection+search) | OK | Separate canvas cleared+redrawn every frame; base canvas only repaints dirty rows |
| DPI/Retina scaling | OK | `dpr * logical` sizing + `ctx.scale()` |
| DPR change listener | OK | `matchMedia(resolution)` re-register on change |
| Theme colors (ANSI 16) | OK | Colors come from Rust/Alacritty in frame data |
| Default fg/bg from terminal theme | OK | `getTerminalTheme(settingsStore.state.theme)` |
| Scrollbar themed | OK | Uses `var(--fg-primary)` CSS custom property with configurable opacity |

### Zoom / Font

| Feature | Status | Notes |
|---------|--------|-------|
| Per-terminal fontSize | OK | Reads `terminalsStore[terminalId].fontSize` |
| Global defaultFontSize | OK | Fallback when per-terminal not set |
| Font family reactive | OK | `createEffect` watches `settingsStore.state.font` |
| Font weight reactive | OK | |
| Line height (snapped) | OK | `snapLineHeight()` |
| Zoom Cmd+/- | OK | Via `terminalsStore.setFontSize` |
| Font preload | OK | `document.fonts.load()` targeting configured terminal font |

### Scroll

| Feature | Status | Notes |
|---------|--------|-------|
| Mouse wheel scroll | OK | `terminal_scroll` IPC |
| Scrollbar visibility | OK | Shows when `historySize > 0` |
| Scrollbar thumb drag | OK | Custom implementation |
| Scrollbar track click-to-position | OK | |
| Arrow Down snap-to-bottom | OK | When `displayOffset > 0` |
| Page Up/Down | OK | Via `Terminal.tsx` refMethods using `terminal_scroll_info` IPC |
| scrollToTop | OK | Via `Terminal.tsx` refMethods |
| scrollToBottom | OK | Via `Terminal.tsx` refMethods |
| scrollToLine (absolute) | OK | `terminal_scroll_to` IPC |
| Viewport lock (ESC[3J suppression) | N/A | Wontfix — no equivalent needed in canvas path |

### Resize

| Feature | Status | Notes |
|---------|--------|-------|
| ResizeObserver | OK | Installed only after `onMount` awaits the session subscriptions and the font load, so it cannot be the sole re-measure path |
| Debounce (100ms) | OK | `clearTimeout` + `setTimeout(remeasure, 100)` |
| Minimum size guard | OK | Guards both `resize_pty` IPC and `measureNow()` |
| Unsized pane | OK | `measureNow()` reports it measured nothing; `remeasure()` then waits for the first real box (`retryUntilMeasured`, bounded at `SIZE_RETRY_MAX_FRAMES`). A page reload mounts every terminal before layout, and dropping that measurement left the canvas in a corner of the pane |
| resize_pty IPC | OK | |

### Input / Keyboard

| Feature | Status | Notes |
|---------|--------|-------|
| VT100 escape sequences | OK | `keyToSequence()` in terminalInput.ts |
| Kitty keyboard protocol (flag 1) | OK | `kittySequenceForKey()` |
| Shift+Enter (ESC CR) | OK | |
| Shift+Tab (CSI Z) | OK | |
| macOS Ctrl+letter (emacs) | OK | Uses `e.code` for reliability |
| macOS Left Option as Meta | OK | `altSequenceFromCode()` |
| Windows Ctrl+V paste | OK | |
| Cmd+Enter passthrough | OK | |
| IME composition | OK | `compositionstart/compositionend`; hidden input positioned at cursor coords via `syncImePosition()` for East Asian IME candidate windows |
| Bracketed paste | OK | `\x1b[200~...\x1b[201~` |
| MCP atomic agent submission | OK | Backend-only `session action=submit`; the shared PTY writer lock spans payload, raw-mode gap, and Enter, so CanvasTerminal input cannot splice the submitted command. No renderer state or new frontend transport exists. |
| Image paste detection | OK | Shared `isImagePaste()` precedence: substantive `text/plain` wins over incidental images; screenshot-only and Finder image filename pastes send Ctrl+V |
| Resume banner keyboard | OK | Space/Enter/Escape/printable |
| Touch tap/swipe/pinch (mobile) | OK | `installTouchHandlers` focuses the shared keyboard input; primary compatibility mouse presses cancel the browser default canvas focus action; native keyboard animation still needs iPad verification |

### Selection & Clipboard

| Feature | Status | Notes |
|---------|--------|-------|
| Mouse drag selection | OK | |
| Double-click word select | OK | `terminal_select_start` with `word:true` |
| Triple-click line select | OK | |
| Cmd+C copy with selection | OK | `terminal_get_selection_text` IPC/HTTP parity path |
| Selection normalization | OK | Rust unwraps soft-wrapped rows, trims row padding, removes coherent Claude space/NBSP `▎` gutters, and strips one composer `❯ ` plus its continuation margin with conservative width reflow only when selected from grid column zero outside a VT soft-wrap continuation |
| Copy-on-select | OK | `copySelection()` called from `onMouseUp` |
| getSelection() ref method | OK | Returns the cached backend selection; `getLocalSelectionText()` is the transient fallback |

### Focus

| Feature | Status | Notes |
|---------|--------|-------|
| focus() ref method | OK | `canvasTerminalRef?.focus()` |
| Auto-focus on tab activation | OK | Visibility effect in Terminal.tsx |
| onFocus callback prop | OK | Wired in CanvasTerminalProps |
| Focus/blur cursor visual | OK | |
| focus() ref race | OK | Resolved via deferred ref registration |

### Links

| Feature | Status | Notes |
|---------|--------|-------|
| File path detection (hover) | OK | Async row text fetch + regex |
| File path Cmd+click open | OK | |
| Pointer cursor on link | OK | |
| Web URL links (http/https) | OK | `webUrlRe` regex in `checkLinksAtRow` |
| OSC 8 hyperlinks | OK | `terminal_hyperlink_at` IPC, priority over other link types |

### Search

| Feature | Status | Notes |
|---------|--------|-------|
| Cmd+F opens search bar | OK | |
| Escape closes search | OK | |
| Search results highlighting | OK | `paintSearchHighlights` on overlay canvas |
| Next/prev match navigation | OK | `searchNext`/`searchPrev` with wrap-around |
| searchBuffer() ref method | OK | `terminal_search_buffer` IPC |
| openSearch/closeSearch ref | OK | |

### Terminal Bell

| Feature | Status | Notes |
|---------|--------|-------|
| Visual flash | OK | `frame.bell` → `bell-flash` CSS class (150ms) |
| Audio bell | OK | `notificationsStore.play("info")` via Terminal.tsx |

### OSC Handlers

| Feature | Status | Notes |
|---------|--------|-------|
| OSC 0/2 and structured intent title change | OK | Handled in Terminal.tsx wrapper; spawn labels remain replaceable, explicit user renames are protected |
| OSC 7 cwd tracking | OK | `pty-cwd-{sessionId}` on desktop, `{"type":"cwd","cwd":…}` on the grid WS. Both carry the same `{ cwd }` object, so the handler needs no per-transport branch. |
| OSC 133 command blocks | OK | `pty-osc133-{sessionId}` on desktop, `{"type":"osc133",…}` on the grid WS — both serialised from `Osc133Event`, so the payload is `{ marker, line, exit_code }` on either transport. Subscribed **only** here, and subscribed before the canvas waits for its fonts — that await used to sit between mount and subscription, which is the window the first prompt marker of a session falls in. Nothing replays either event, so a marker emitted before the terminal mounts at all is still lost; the fix closes the font window, not the whole race. `handleOsc133` is not idempotent for the `A` marker, so a second listener invents one empty command block per prompt. |
| OSC 133 gutter decoration | OK | `paintGutterMarkers` on overlay canvas |
| User-prompt scrollbar markers | OK | Green ticks at `userPromptLines` — distinct from command-block marks, drawn in `paintGutterMarkers` |
| Cmd+Up/Down block navigation | OK | Reads `commandBlocks` + `activeBlock` |
| OSC 9 progress bar | OK | `terminal()?.progress` → 2px green bottom-edge fill on tab |

### TerminalRef Methods

| Method | Status | Notes |
|--------|--------|-------|
| `fit()` | OK | Delegates to `refresh()` (full redraw via ResizeObserver) |
| `write(data)` | OK | `pty.write(sessionId, data)` |
| `writeln(data)` | OK | `pty.write(sessionId, data + "\n")` |
| `input(data)` | OK | `pty.write(sessionId, data)` |
| `clear()` | OK | Sends `\x1b[2J\x1b[H\x1b[3J` via `pty.write` |
| `refresh()` | OK | Clears buffer + requests fresh frame |
| `focus()` | OK | |
| `getSessionId()` | OK | |
| `openSearch()` | OK | |
| `closeSearch()` | OK | |
| `toggleCompose()` | OK | `extractCurrentInput` reads canvas row text |
| `openComposeWithText(text)` | OK | |
| `searchBuffer(query)` | OK | `terminal_search_buffer` IPC |
| `scrollToLine(lineIndex)` | OK | `terminal_scroll_to` IPC |
| `getSelection()` | OK | `getLocalSelectionText()` |
| `scrollToTop()` | OK | |
| `scrollToBottom()` | OK | |
| `scrollPages(pages)` | OK | |
| `getBufferLines(start, end)` | OK | `terminal_get_lines` IPC |

### Other

| Feature | Status | Notes |
|---------|--------|-------|
| File drag-and-drop (internal) | OK | `application/x-tuic-path` MIME from file tree |
| OS file drag-and-drop | OK | Finder/Explorer drag via `tauri://drag` event |
| Parsed events | OK | Handled by Terminal.tsx wrapper |
| Suggest overlay | OK | DOM divs over canvas |
| Intent row highlight | OK | |
| Notifications (sounds) | OK | Handled by Terminal.tsx wrapper |
| Flow control / backpressure | OK | IntersectionObserver: skip paint+ack when hidden |
| Plugin watcher lines | OK | `pty-watcher-lines-{sessionId}` (browser: `watcher-lines` WS frame) → `pluginRegistry.handleWatcherLines`. Rust assembled, cleaned and matched the lines on the reader thread; no raw stream is reassembled or scanned here. The listener is installed BEFORE the grid subscription — an event that lands while it is still being attached is gone, and nothing replays a watcher line |

## Remaining Gaps

None. All tracked gaps have been resolved or marked wontfix.

### Stored terminal marker coordinates

Stored command blocks and user prompt markers use all-time rows. Answers history consumes them directly. Gutter, scrollbar, search scope, navigation and buffer reads subtract the current frame history base at grid-relative boundaries. Evicted markers are omitted.

Terminal stream reconnect notices use the current terminal name when available and show the transport’s current attempt (up to 10). A received grid frame or confirmed empty replay removes only that stream notice; socket opening alone does not. Exhausted retries leave a persistent failure notice. Closing the terminal removes its notice, and late subscription failures cannot publish a notice after closure.
