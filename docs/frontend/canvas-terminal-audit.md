# CanvasTerminal Feature Audit

**Last updated:** 2026-05-03
**Branch:** feat/alacritty-terminal-migration

CanvasTerminal is the sole terminal renderer. xterm.js has been fully removed. The renderer is powered by `alacritty_terminal` (Rust) sending binary grid frames over a Tauri Channel.

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
        +-- Touch input (tap/swipe/pinch for mobile/tablet via offscreen textarea)
        +-- IntersectionObserver flow control (skip paint when hidden)
        +-- Plugin raw output forwarding (pluginRegistry.processRawOutput)
        +-- OSC 7 CWD + OSC 133 shell integration
```

Key insight: Terminal.tsx handles parsed events, session lifecycle, banners, and compose panel. CanvasTerminal is purely a renderer + input handler with no session logic.

## Binary Frame Format

Each frame: 26-byte header + variable row data. The header ends with a `historyBase: u32` (lines evicted from the history top so far); `historyBase + (historySize - displayOffset + screenRow)` is the eviction-stable absolute index the smooth-scroll row cache keys by, so a cached row never aliases onto a different line after the scrollback cap rotates. Per cell: 4 bytes codepoint + 3 bytes fg RGB + 3 bytes bg RGB + 1 byte attrs bitmask = 11 bytes. Decoded in `decodeBinaryFrame` using struct-of-arrays (SoA) typed arrays — zero per-cell object allocation.

## Performance Notes

- **RAF coalescing:** All paint triggers (frame arrival, keydown selection clear, mousedown) go through `scheduleRepaint()` which schedules a single `requestAnimationFrame`. No synchronous paint calls — prevents double-paint in a single event loop turn.
- **`send_grid_frame` clone guard:** Frame is only cloned for the `grid_watch` channel when `receiver_count() > 0` (i.e. WS clients connected). Desktop-only path (Tauri Channel) is zero-copy.
- **`screen_text_rows_ref()`:** `TerminalGrid` exposes a borrowed `&[String]` view of cached screen rows. Used in `process_chunk` for chrome cutoff detection to avoid cloning 50 Strings per PTY chunk. Downstream parsers (slash-menu, choice-prompt) share a single owned snapshot computed once per chunk.
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
| ResizeObserver | OK | |
| Debounce (100ms) | OK | `clearTimeout` + `setTimeout(remeasure, 100)` |
| Minimum size guard | OK | Guards both `resize_pty` IPC and `remeasure()` |
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
| Image paste detection | OK | Checks `items[i].type.startsWith("image/")` |
| Resume banner keyboard | OK | Space/Enter/Escape/printable |
| Touch tap/swipe/pinch (mobile) | OK | `installTouchHandlers` via offscreen textarea |

### Selection & Clipboard

| Feature | Status | Notes |
|---------|--------|-------|
| Mouse drag selection | OK | |
| Double-click word select | OK | `terminal_select_start` with `word:true` |
| Triple-click line select | OK | |
| Cmd+C copy with selection | OK | `terminal_select_text` IPC |
| Trailing-space trim on copy | OK | `line.replace(/\s+$/, "")` |
| Copy-on-select | OK | `copySelection()` called from `onMouseUp` |
| getSelection() ref method | OK | `getLocalSelectionText()` reads from rowMap codepoints |

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
| OSC 0/2 title change | OK | Handled in Terminal.tsx wrapper |
| OSC 7 cwd tracking | OK | `pty-cwd-{sessionId}` event |
| OSC 133 command blocks | OK | `pty-osc133-{sessionId}` event |
| OSC 133 gutter decoration | OK | `paintGutterMarkers` on overlay canvas |
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
| Plugin raw output forwarding | OK | `pty-output-{sessionId}` → `pluginRegistry.processRawOutput` |

## Remaining Gaps

None. All tracked gaps have been resolved or marked wontfix.
