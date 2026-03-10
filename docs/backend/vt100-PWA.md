# VT100-to-PWA Protocol

How TUICommander transforms raw PTY output into structured, styled terminal content for the mobile Progressive Web App.

## Architecture Overview

```
PTY (raw bytes)
  │
  ▼
VtLogBuffer (vt100 parser)
  ├── scrollback → LogLine[] (styled spans)
  ├── screen_rows → String[] (plain text)
  └── prompt_input_text → String (user-typed, no ghost text)
        │
        ▼
  HTTP / WebSocket handlers
  ├── trim_screen_chrome() → remove agent UI footer
  ├── is_separator_line() → detect decorated separators
  └── SessionState → accumulated parsed events
        │
        ▼
  Frontend (SolidJS)
  ├── OutputView → log + screen rendering, auto-scroll
  ├── CommandInput → bidirectional PTY ↔ textarea sync
  └── SlashMenuOverlay → arrow navigation, prefill
```

---

## 1. VtLogBuffer — VT100 Parsing Engine

**File:** `src-tauri/src/state.rs`

### Configuration

| Parameter | Value | Notes |
|-----------|-------|-------|
| Scrollback | 10,000 lines (`VT100_SCROLLBACK`) | Internal vt100 parser buffer |
| Log capacity | 10,000 lines (`VT_LOG_BUFFER_CAPACITY`) | Ring buffer of finalized LogLines |
| Default size | 24 rows × 80 cols | Resizable via `resize()` |

```rust
VtLogBuffer::new(rows, cols, capacity)
// Parser created with: vt100::Parser::new(rows, cols, VT100_SCROLLBACK)
```

### `process(data: &[u8])` — Core Pipeline

Called for every PTY read chunk. Steps:

1. **Feed bytes** to vt100 parser
2. **Detect changed rows** by diffing current screen against `prev_rows` cache
3. **Extract scrollback delta:**
   ```
   total_sb = scrollback_count()        // query vt100 internal counter
   delta = total_sb - self.scrollback_read
   new_lines = read_scrollback_lines(delta)
   ```
4. **Trim agent chrome** from new lines (remove prompt/separator lines)
5. **Push to log ring buffer**
6. **Update `prev_rows`** snapshot for next diff
7. **Return changed row indices** (for output parser)

### Scrollback Extraction

The vt100 parser maintains an internal scrollback buffer. Lines are "scrolled off" when a line feed occurs at the bottom of the screen (real scroll), but NOT when cursor-based TUI redraws happen.

```rust
fn scrollback_count(&mut self) -> usize {
    // Temporarily set max scrollback window to query total count
    self.parser.screen_mut().set_scrollback(usize::MAX);
    let count = self.parser.screen().scrollback();
    self.parser.screen_mut().set_scrollback(0);
    count
}

fn read_scrollback_lines(&mut self, count, screen_height) -> Vec<LogLine> {
    // Page through scrollback in screen_height-sized chunks
    // using set_scrollback(offset) to position the view
    // Extract each row via extract_log_line()
}
```

**Key invariant:** `scrollback_read` is monotonically increasing. Each `process()` call reads only the delta since the last call.

### `extract_log_line(screen, row)` — Styled Cell Extraction

Converts a vt100 screen row into a `LogLine` with colored spans:

```rust
struct LogLine { spans: Vec<LogSpan> }
struct LogSpan {
    text: String,
    fg: Option<LogColor>,   // Idx(u8) or Rgb(r,g,b)
    bg: Option<LogColor>,
    bold: bool,
    italic: bool,
    underline: bool,
}
```

Algorithm:
- Iterate columns left-to-right
- Skip wide-char continuation cells
- Group consecutive cells with identical attributes into one span
- Flush span when attributes change
- Trim trailing empty/whitespace-only spans with default styling

### `screen_rows()` — Current Screen Content

Returns the visible terminal content as plain text strings.

- **Fast path:** Returns cached `prev_rows` from last `process()` call
- **Fallback:** Reads directly from parser (before first process or after resize)

### `prompt_input_text()` — User Input Extraction

Extracts what the user has typed at the prompt, **excluding ghost/suggestion text**.

Algorithm:
1. Scan rows bottom-to-top for prompt character (`❯`, `>`, `> `)
2. Walk cells left-to-right after the prompt char
3. Collect cell contents while `!cell.dim()`
4. **Stop at first dim cell** — that's ghost/autocomplete text
5. Return trimmed result

This is critical: Claude Code shows inline suggestions in dim text (e.g., `❯ /wiz:status` where `/wiz:status` is grey). Without the dim check, the suggestion text would appear in the PWA textarea.

### `trim_agent_chrome()` — Log Line Filtering

Removes agent UI chrome (prompt, status bar, separator) from captured log lines.

- Scans last 8 rows for prompt pattern
- Extends cutoff upward past separator and empty lines
- Uses `is_separator_line()` — checks for 4+ consecutive box-drawing chars

---

## 2. HTTP/WS Session Handlers

**File:** `src-tauri/src/mcp_http/session.rs`

### `GET /sessions/:id/output?format=log`

Initial data fetch before WebSocket connects.

**Response:**
```json
{
  "lines": [LogLine, ...],
  "total_lines": 1234,
  "screen": ["row1", "row2", ...],
  "input_line": "user typed text"
}
```

- `total_lines` serves as the offset cursor for WS catch-up
- `screen` has chrome trimmed via `trim_screen_chrome()`
- `input_line` from `prompt_input_text()` (dim text excluded)

### `WS /sessions/:id/stream?format=log&offset=N`

Real-time bidirectional stream. Two concurrent tasks:

#### Server → Client (polling task)

Runs in a `tokio::select!` loop with two branches:

**Branch 1: 200ms timer** — polls VtLogBuffer for changes:
```rust
let (lines, new_offset) = buf.lines_since_owned(offset);
let trim = trim_screen_chrome(buf.screen_rows());
let input_line = buf.prompt_input_text();
```

Change detection via hash:
```rust
let mut hasher = DefaultHasher::new();
screen.hash(&mut hasher);
input_line.hash(&mut hasher);
let screen_hash = hasher.finish();
let screen_changed = screen_hash != prev_screen_hash;
```

Frame sent only when `!lines.is_empty() || screen_changed`:
```json
{
  "type": "log",
  "offset": 100,
  "lines": [...],
  "screen": [...],
  "input_line": "text"
}
```

**Branch 2: event bus** — forwards SessionState on parsed events:
```json
{
  "type": "state",
  "state": { "awaiting_input": true, "agent_type": "claude-code", ... }
}
```

#### Client → Server (input passthrough)

WebSocket Text/Binary messages are written directly to the PTY.

### `trim_screen_chrome()` — Screen Footer Removal

Removes Claude Code's TUI footer (separator, status bar, permissions line, prompt).

**Scan window:** Last 15 rows from content end (handles Claude Code's ~12-row footer).

**Two anchor strategies:**

1. **Separator line** — `is_separator_line()`:
   ```rust
   fn is_separator_line(s: &str) -> bool {
       // 4+ consecutive: ─ ━ ═ — ╌ ╍
       // Tolerates decorated separators:
       // "──────── ■■■ Medium /model ─"
   }
   ```

2. **Prompt line** — starts with `❯`, `>`, `> `

Takes the **higher anchor** (closer to content), extends upward past empty/separator lines, truncates.

### `write_to_session()` — Slash Mode Tracking

When users type `/` from the mobile PWA, the backend tracks this to enable slash menu detection in the output parser:

```rust
if data == "/" || data.starts_with('/') {
    slash_mode.store(true);
} else if data.contains('\r') || data.contains('\n') {
    slash_mode.store(false);
}
```

The output parser checks `slash_mode` before scanning screen rows for slash command menus.

---

## 3. Frontend Transport

**File:** `src/transport.ts`

### `subscribePty(sessionId, onData, onExit, options)`

Auto-detects Tauri (native) vs browser (HTTP/WS) mode.

**Browser mode** connects a WebSocket:
```
ws(s)://host/sessions/{sessionId}/stream?format=log&offset={logOffset}
```

**Options:**
```typescript
interface SubscribePtyOptions {
  format?: "log";
  logOffset?: number;
  onLogLines?: (lines: unknown[]) => void;
  onScreenRows?: (rows: string[]) => void;
  onInputLine?: (text: string | null) => void;
  onStateChange?: (state: Record<string, unknown>) => void;
}
```

**Frame dispatch:**

| Frame type | Callback | Data |
|------------|----------|------|
| `"log"` | `onLogLines`, `onScreenRows`, `onInputLine` | Styled lines, screen rows, prompt input |
| `"state"` | `onStateChange` | SessionState snapshot |
| `"exit"` / `"closed"` | `onExit` | Session ended |

### `rpc("write_pty", { sessionId, data })`

Maps to `POST /sessions/{sessionId}/write` with body `{ data: "..." }`.

Used for all PTY input: typed characters, escape sequences (arrow keys), control characters (Ctrl-U, Ctrl-C).

---

## 4. Mobile Components

### OutputView

**File:** `src/mobile/components/OutputView.tsx`

Renders combined log history + current screen with auto-scroll management.

**Text wrapping strategy:** Normal text uses `pre-wrap` so long lines wrap on narrow screens. Consecutive lines containing box-drawing characters (U+2500–U+257F) are grouped into scrollable `tableBlock` containers with `white-space: pre` and `overflow-x: auto`, preserving alignment for tables, tree views, and bordered output. Grouping is done by `groupLineBlocks()` and detection by `hasBoxDrawing()` in `src/mobile/utils/logLine.ts`.

**Initialization:**
1. HTTP fetch: `GET /sessions/{id}/output?format=log`
2. WebSocket connect with `logOffset` from step 1 (avoids duplicate lines)

**Data model:**
```
displayedLines = [...logLines, ...screenRows]  // combined
                 .filter(searchQuery)            // optional filter
```

- `logLines`: Accumulated scrollback (max 500, ring buffer)
- `screenRows`: Current terminal screen (replaced each update)

**Auto-scroll suppression:**
- Track `userScrolledUp` via scroll event listener
- "At bottom" = within 80px of scroll end
- `scrollToBottom(force?)`: skips if user scrolled up, unless `force=true`
- Initial load and session exit force-scroll

### CommandInput — Last-Writer-Wins Sync

**File:** `src/mobile/components/CommandInput.tsx`

The textarea syncs bidirectionally with the PTY input line using a **last-writer-wins** strategy. Only one side writes at a time — there is no attempt at incremental character-by-character synchronization.

#### Direction 1: PTY → Textarea

Source: `ptyInputLine` prop (from WebSocket `input_line` field).

When `userEditing` is false, every incoming `input_line` replaces the textarea content entirely:

```typescript
createEffect(() => {
  const il = props.ptyInputLine;
  if (userEditing) return;
  setValue(il ?? "");
});
```

#### Direction 2: Textarea → PTY

On every input event, the **full textarea content** is sent to the PTY after a 300ms debounce:

```typescript
function debouncedSync(text: string) {
  if (syncTimer) clearTimeout(syncTimer);
  syncTimer = setTimeout(() => {
    rpc("write_pty", { data: "\x15" + text }); // Ctrl-U + full text
  }, SYNC_DEBOUNCE_MS);
}
```

This handles all input methods uniformly: typing, paste, cut, autocomplete, composition, cursor-based edits. No `InputEvent.inputType` parsing needed.

On blur, any pending debounce flushes immediately.

#### Send (Enter)

On Enter: `Ctrl-U` → text → `\r` as three separate writes.

Ink-based TUIs (Claude Code) treat `\r` combined with text as a newline in their multiline input, not as submit. Splitting into separate writes works around this.

#### State Transitions

```
idle (no focus)     → PTY input_line updates textarea
  │
  ▼ (tap/focus)
editing             → debounced Ctrl-U + full text to PTY
  │
  ├── Enter         → flush + send + clear + back to idle
  ├── blur (empty)  → flush + back to idle
  └── blur (draft)  → flush + stays in editing (preserves draft)
```

### SlashMenuOverlay

**File:** `src/mobile/components/SlashMenuOverlay.tsx`

Displays slash command menu items detected from the terminal.

**Item source:** `sessionState.slash_menu_items` — populated by the output parser when it detects a slash command menu on screen (gated by `slash_mode`).

**Arrow navigation:**
```
┌─────────────────────────┐
│ /help     Get help...   │
│ /compact  Compact...    │
│ /review   Review code   │
│ ⏫  ▲  ▼  ⏬           │  ← arrow bar
└─────────────────────────┘
```

- Single arrow (▲/▼): sends one `\x1b[A` or `\x1b[B`
- Page arrow (⏫/⏬): sends `items.length` arrows (scrolls one page)
- Anti-zoom: `touch-action: manipulation` on all buttons

**Selection flow:**
1. User taps item → `onSelect(command)` callback
2. Parent sets `inputPrefill` signal with `{ text, seq }` counter
3. CommandInput receives prefill, sets textarea value, focuses
4. Also sends `Ctrl-U + text` to PTY so terminal shows it
5. User reviews, optionally edits, presses Enter to submit

### SessionDetailScreen — Orchestration

**File:** `src/mobile/screens/SessionDetailScreen.tsx`

Wires everything together:

```
OutputView ──onStateChange──→ wsState signal
           ──onInputLine───→ ptyInputLine signal
                                    │
SlashMenuOverlay ──onSelect──→ inputPrefill signal
                                    │
CommandInput ←── prefillValue ──────┘
             ←── ptyInputLine ──────┘
```

**State merging:** WebSocket state is authoritative when present; 3s poll state fills gaps.

---

## 5. SessionState Accumulation

**File:** `src-tauri/src/state.rs` — `apply_event_to_session_state()`

The output parser emits `PtyParsed` events. These accumulate into `SessionState`:

| Event type | Fields updated |
|------------|---------------|
| `question` | `awaiting_input = true`, `question_text` |
| `user-input` | `awaiting_input = false`, clear slash menu, capture `last_prompt` |
| `rate-limit` | `rate_limited = true`, `retry_after_ms` |
| `usage-limit` | `usage_limit_pct` |
| `api-error` | `last_error` |
| `status-line` | Clear rate-limit/error/suggest/slash, set `current_task` |
| `intent` | `agent_intent` |
| `suggest` | `suggested_actions` |
| `slash-menu` | `slash_menu_items` |
| `progress` | `progress` (0-100, None on state=0) |

**Lifecycle:**
- Created on `SessionCreated`
- Removed on `SessionClosed`
- `is_busy` cleared on `PtyExit`

---

## 6. Key Escape Sequences

| Sequence | Meaning | Usage |
|----------|---------|-------|
| `\x15` | Ctrl-U | Clear input line (readline) |
| `\x7f` | DEL/Backspace | Delete char before cursor |
| `\r` | Carriage Return | Submit input |
| `\x1b[A` | Arrow Up | Navigate menu / history |
| `\x1b[B` | Arrow Down | Navigate menu / history |
| `\x1b[3~` | Delete | Delete char after cursor |
| `\x03` | Ctrl-C | Interrupt |
| `\x04` | Ctrl-D | EOF / exit |

---

## 7. Debugging

### Check backend logs

```
GET /logs?source=terminal&limit=20
```

### Inspect raw WS frames

In browser DevTools → Network → WS → filter by `/stream`:
- `"type":"log"` frames show line count, screen rows, input_line
- `"type":"state"` frames show accumulated session state

### Verify input_line extraction

```
GET /sessions/{id}/output?format=log
```

Response includes `"input_line"` — should show only user-typed text (no ghost/dim suggestions).

### Verify separator detection

If agent chrome leaks through, check `trim_screen_chrome()` scan window (15 rows) and `is_separator_line()` threshold (4+ consecutive box-drawing chars).
