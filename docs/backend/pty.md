# PTY Management

**Module:** `src-tauri/src/pty.rs`

Manages pseudo-terminal sessions for all terminal tabs in the application.

## Session Lifecycle

```
create_pty() / create_pty_with_worktree()
    │
    ├── Resolve shell (platform default or user override)
    ├── Build shell command via portable-pty CommandBuilder
    ├── Spawn PTY pair (master + child process)
    ├── Store PtySession in AppState.sessions (DashMap)
    ├── Create OutputRingBuffer for MCP access
    ├── Spawn reader thread (background, non-blocking)
    │
    ▼
Session Active: write_pty() / resize_pty() / pause_pty() / resume_pty()
    │
    ▼
close_pty(cleanup_worktree)
    ├── Remove session from DashMap
    ├── Kill child process
    ├── Remove output buffer
    └── Optionally remove associated git worktree
```

Each session may also carry an orchestrator-owned `pty_description`, separate
from the last user prompt captured by input-line bookkeeping. MCP spawn and
input actions update it through `pty-description-changed`; desktop and browser
clients render it together with the last prompt above the terminal.

The idle-close task considers only managed child PTYs with an idle or completed
agent state. After the configured per-agent delay it sends the parent an
`idle_timeout` lifecycle notice and closes through the normal session-close
path without removing the worktree. Activity restarts the window; unread mail,
background work, a live detached runner, a failed background wake, and keep-open
disable closure while they remain present.
The five-second idle sweep reads in-memory state first and requests a process
inventory only when an eligible child's delay has expired. A failed inventory
defers closure; a panicking sweep logs the error and restarts its timer state on
the next tick.

## Tauri Commands

### Session Creation

| Command | Description |
|---------|-------------|
| `create_pty(config: PtyConfig)` | Spawn a new PTY session. Returns session ID. |
| `create_pty_with_worktree(pty_config, worktree_config)` | Create worktree + spawn PTY in it. Returns `WorktreeResult`. |

Production spawn sites share `pty::spawn_pty_pair_with_retry` and its async
wrapper. Only explicitly classified PTY allocation failures (for example OS
resource exhaustion or an interrupted/would-block allocation) receive the
bounded three-attempt, 100/200 ms backoff. Once a PTY pair exists, command spawn
runs exactly once: invalid binaries, cwd, and permission failures return
immediately. Async Tauri and HTTP entry points run allocation and backoff on
Tokio's blocking pool; synchronous internal callers retain the same bounded
policy. Each site still owns its justified command/env/dimension assembly.
The HTTP session spawn checks a supplied `cwd` before PTY allocation: on some
platforms the PTY library returns a session even when the child later fails to
enter the directory, leaving a blank terminal. A missing or non-directory cwd
returns `400` and does not enter the session map.

### Session Control

Session removal polls the child once and releases the PTY handles immediately.
If the child has not terminated yet, a separate waiter owns only its child handle
until `wait()` reaps it. This covers reader EOF arriving before process exit and
explicit close/kill racing termination, without holding session or map locks.
The existing post-mortem buffers still expire through the five-minute tombstone
sweeper; explicit close removes them immediately.

On Linux with glibc, releasing terminal payloads also calls `malloc_trim(0)`
after the owners and map guards are dropped. The expiry sweeper trims once per
batch that actually releases payloads. This returns unused scrollback pages
that glibc thread arenas otherwise keep resident, without changing live grids
or the process-wide arena limit. Other platforms and libcs keep their allocator
behavior.

| Command | Description |
|---------|-------------|
| `write_pty(session_id, data)` | Write data (user input) to the PTY. Raises the calling thread to `QOS_CLASS_USER_INTERACTIVE` on macOS for the duration of the write and restores the previous class on the way out, so a keystroke is not scheduled behind background work on a pool thread TUIC only borrowed. |
| `resize_pty(session_id, rows, cols)` | Resize the PTY terminal dimensions. `async`: the reflow it triggers is proportional to scrollback and runs on the blocking pool (`resize_session_off_thread`), never on the IPC thread. The HTTP route takes the same path. |
| `pause_pty(session_id)` | Pause the reader thread (stops output emission). |
| `resume_pty(session_id)` | Resume the reader thread. |
| `close_pty(session_id, cleanup_worktree)` | Close PTY and optionally remove worktree. |

### Monitoring

Foreground-agent exit or an observed agent-type change also retires the terminal's volatile Telegram registration
lifetime synchronously. A restarted agent using the same terminal and MCP identity
must opt in again, even if the Telegram worker was busy during the transition. Unrecognized
non-shell probes retain the previous identity and do not revoke registration.

| Command | Description |
|---------|-------------|
| `get_orchestrator_stats()` | Active/max/available session counts. |
| `get_session_metrics()` | Total spawned, failed, bytes emitted, pauses. |
| `can_spawn_session()` | Check if under MAX_CONCURRENT_SESSIONS (50). |
| `list_active_sessions()` | List all sessions with cwd and worktree info. |
| `list_worktrees()` | List all managed worktrees. |
| `get_process_stats()` | CPU% and RSS for TUIC + all child process trees (desktop Tauri command). |
| `collect_process_stats(state)` | Same logic, callable from HTTP routes and MCP tools. |

## Reader Thread

Each session spawns a dedicated reader thread that reads from the PTY master fd:

```rust
spawn_reader_thread(reader, paused, session_id, app, state)
```

**Processing pipeline per read:**

1. Read raw bytes from PTY master (up to 64KB buffer for natural burst batching)
2. Strip Kitty keyboard protocol sequences (non-printable noise for consumers)
3. Push through `Utf8ReadBuffer` — accumulates bytes until valid UTF-8 boundary, returns safe string
4. Push through `EscapeAwareBuffer` — holds incomplete ANSI escape sequences (CSI, OSC, etc.)
5. Feed into `VtLogBuffer` for VT100-aware changed-row parsing and primary-screen log extraction (mobile/MCP consumers)
6. Write to `OutputRingBuffer` (2 MB circular buffer for MCP access — `OUTPUT_RING_BUFFER_CAPACITY`)
7. Serialize parsed events once with `serde_json::to_value` — reused for both Tauri IPC and event bus (avoids double serialization)
8. Broadcast to WebSocket clients (if any connected)
9. Assemble the lines of the chunk and match them against the compiled plugin OutputWatchers (`crates/tuic-terminal/src/output_watchers.rs`), then emit `pty-watcher-lines-{session_id}` with the batch — see [Plugin OutputWatcher matching](#plugin-outputwatcher-matching) below. No raw-output Tauri event is emitted any more: the desktop canvas renders from grid frames, and the assembled lines are the only text the WebView needs. (The raw `output` frame of step 8 is unaffected — it is fed from the output ring buffer to raw-mode WebSocket clients.)

The intent path reconstructs soft-wrapped grid rows up to the parser's
character budget, then reads hard-wrap continuations only above the input-box
chrome cutoff. A capped history scroll keeps the open intent's anchor stable;
repainting its unchanged anchor does not close it as replaced prose.

For a managed Claude child with workspace trust acceptance enabled, the rendered screen is also checked for its exact startup safety question, both choices, and **No, exit** selected. Only then does the reader send Up followed by Enter under the PTY writer lock, once for that session. Other dialogs and user-opened terminals are never answered by this path; session cleanup removes an unused allowance.

**ANSI anomaly detection** — The `detect_anomalous_sequences()` function scans PTY output for unusual escape sequences (screen clears, cursor home, alt-screen toggles, scrollback clears) and logs them at warn level. This is a diagnostic tool for investigating scroll-jump issues.

**Pause behavior:** When `paused` flag is set (`AtomicBool`), the reader thread sleeps in 10 ms slices instead of reading. This prevents output flooding during background operations.

**Exit detection:** When the read returns 0 bytes or an error, the thread:
1. Flushes remaining buffered data
2. Emits `pty-exit` event with exit code
3. Removes session from `AppState.sessions`
4. Updates metrics (decrement `active_sessions`)

### Plugin OutputWatcher matching

Plugin `OutputWatcher` patterns are matched on the reader thread instead of the WebView
main thread. Each frontend pushes its whole watcher set through
`set_plugin_output_watchers` (HTTP: `POST /api/plugins/output-watchers`) with its
`client_id` and a monotonic `seq`; `plugins.rs` compiles it into
`AppState.plugin_output_watchers` (`OutputWatcherRegistry`, one set per client, at most
8, least recently synced evicted). A stale `seq` answers `applied: false` and changes
nothing — including for a client whose current set is empty, whose record is *parked*
rather than removed so that a delayed older sync cannot resurrect a disposed set.
Tauri IPC registrations also carry their WebView label. A document navigation or
window destruction removes that WebView's sets before a replacement document
registers; browser clients remain independent and use the bounded eviction path.

**The frontend re-syncs every 30 s while it holds any watcher.** Rust is the only source
of lines, so a client the backend does not know about is blind with no local symptom, and
two paths lead there without raising an event: the sync failed, or the client bound
evicted a live set (reloading a browser tab leaves its old id behind — no disconnect
reaches the PTY reader). The heartbeat is both the recovery and the liveness signal
eviction ranks by. It stops when the last watcher is disposed.

**Rust is the only line assembler.** The WebView reassembles nothing. Per chunk,
`assemble_watcher_lines()`:

1. Takes one read lock for the whole chunk, so the compiled set and its "I still need every line" answer come from the same snapshot.
2. When no watcher is registered at all (`is_idle`), feeds the chunk to `StreamLines::push_discarding` and returns. The pending partial line still has to be tracked: a watcher that registers mid-line must see that line whole.
3. Otherwise reassembles complete lines with `StreamLines` (trailing `\r` trimmed, the partial line held until the newline arrives). A pending line that passes `MAX_PENDING_LINE` (256 KB) without a newline is **dropped whole**, with a warning, and so is the rest of it up to its closing newline: a producer that never emits `\n` (a redrawing progress bar) must not grow the buffer without bound, and keeping the tail instead was worse than dropping — `/^x+$/` fires on a truncated `yxxx…`, a line that was never on the wire. Both engines would agree on that fabricated text, so the frontend filter cannot catch it.
4. Cleans each line with `clean_line()`, a port of `src/utils/stripAnsi.ts` plus a backtick strip. Both sides must match on exactly the same text.
5. Keeps a line when a compiled pattern matched it. While some registered pattern could not be compiled (`needs_all_lines`), every assembled line is kept instead, because the WebView has to scan them itself.
6. Batches the kept lines in `WatcherLineBatcher` and emits `pty-watcher-lines-{session_id}` (desktop) plus the `PluginWatcherLines` bus event (browser: `watcher-lines` WS frame on `/sessions/:id/stream` in both `?format=grid` and raw mode — `?format=log|text` delegates to a separate handler that does not carry it — plus `plugin-watcher-lines` SSE) with `{session_id, lines}`. Each line is `{text, matched_ids}`, where `matched_ids` are qualified `client_id/watcher_id`. The frontend runs the real JS `RegExp` on `text` to build the `RegExpExecArray` the plugin API promises.

**Batching is throttled but lossless.** One event per `WATCHER_LINE_WINDOW` (100 ms),
because each event is deserialized and dispatched on the WebView main thread and
per-chunk emission starved keydown under an output flood. Lines inside a window are
concatenated in order, never dropped — dropping was the original bug: with an assembler
on each side, a dropped chunk spliced the tail of one chunk onto the head of a later one
and reported a line that was never on the wire. A batch that reaches
`WATCHER_BATCH_CAP` (256 KB of text) is emitted early rather than growing. The frame
ticker drains a due tail, because `read()` blocks and the last line of a burst would
otherwise wait for output that may never arrive. Teardown has exactly one drain, the
reader's EOF path, which assembles the `flush_eof` remainder and then takes the batch
unconditionally; both the ticker and the reader emit while holding the batcher lock, so
an older tail can never overtake a newer batch. The EOF drain also flushes the
assembler's unterminated tail (`StreamLines::flush`) — a command that exits without a
final newline (`printf DONE`) still put that line on the wire.

Losslessness is a property of the assembler and the batcher, **not of delivery**. The
Tauri event and the per-session WS frame are live-only and never replayed: output a
session produces before its terminal installs the listener (or during a WS reconnect
gap) reaches no watcher, and a browser client whose socket stalls past the broadcast
capacity gets `Lagged` and skips those batches. Every live event on this bus behaves the
same way; the 100 ms window keeps the rate at ~10 events/s, which is what makes the lag
case remote rather than routine.

`OutputWatcherRegistry::sync` returns the ids of the patterns the `regex` crate cannot
express — lookaround, backreferences, and a negated class escape inside a character
class (`[\D.]`). Those are **not** compiled to something different: they are reported
back and stay on the frontend path. `to_portable_pattern` rewrites the escapes whose
Rust meaning is narrower than the ECMAScript one (`\d \D \w \W \s \S` to explicit ASCII
sets wrapped in `(?-i:…)` so an `i` flag cannot case-fold a negated set and exclude what
the fold pulled in, `\b`/`\B` to `(?-u:\b)`/`(?-u:\B)`), because the invariant is **Rust
may over-match, never under-match**: the frontend re-runs the real `RegExp` and filters a
false positive, while a false negative is invisible and permanent. The same invariant
covers the `m` flag from the other side: ECMAScript anchors `^`/`$` at every
LineTerminator, CR and U+2028/U+2029 included, so an `m`-flagged pattern is also tried
against each of those segments of the line. `RegexBuilder::multi_line` knows LF only, and
a cleaned line can carry a bare CR.

### Frame Emission Pipeline

Frame emission is decoupled from PTY reading via a per-session **frame ticker** thread (same approach as iTerm2's Metal display-link renderer):

1. **Reader thread**: processes PTY data into the alacritty VT grid, sets a `grid_frame_dirty` AtomicBool flag
2. **Ticker thread**: every 16ms, checks the dirty flag → if set, serializes dirty rows via `serialize_dirty_rows()` → sends frame via `send_grid_frame()` (respects the `GridGate` backpressure)
3. **Frontend**: coalesces paint triggers via `requestAnimationFrame` (~60fps)
4. **Ack handler**: credits the gate; the ticker sends any dirty rows accumulated while the prior frame was in flight

**Cell text fidelity.** Text extraction and binary serialization retain each
cell's base scalar plus its zero-width scalars. A separately received combining
mark damages the cell it modifies, for both parse-side row snapshots and render
frames. Live/full frames and styled-row fetches share the optional sparse
[`TCX1` trailer](../frontend/canvas-terminal-audit.md#cell-text-extension-tcx1);
the fixed cell core stays 11 bytes. The fork's existing nine-mark bound remains.
Search matches exact stored codepoints; buffer-search results use UTF-16 string
offsets, while terminal highlight coordinates remain grid columns.

**Delivery gate (`grid_gate.rs` and `grid_watch.rs`).** A frame is a *delta*, so a dropped one strands
rows that exist nowhere else. Both transports are guarded, and both count rather
than flag:

- **Desktop.** `GridGate` holds `sent` and `acked` counters; the frontend echoes
  its total receipt count in `ack_terminal_frame { received }`. The gate is open
  only when the echo has caught up. The counters exist because a bare boolean
  could not tell a fresh ack from a late one: after the ticker gave a frame up
  for lost (`MAX_IN_FLIGHT_MS` = 500 ms, `abandon()`) and sent the next, the late
  ack reopened the gate and a third frame went out at once — a burst at exactly
  the moment the frontend was proven to be behind.
  Each gate also carries an `epoch`, returned by `subscribe_terminal_grid` and
  echoed on `ack_terminal_frame` / `unsubscribe_terminal_grid`. A remount
  subscribes before the outgoing instance tears down, so the backend receives the
  dead instance's calls against the new gate: without the epoch its late ack
  credits frames the new terminal never received, and its late unsubscribe
  deletes the live channel and leaves a mounted terminal blank.
  Each channel records the subscribing WebView label and epoch. On navigation or
  window destruction, only that WebView's channels and matching gates are
  removed. A main-window reload therefore leaves a floating terminal's channel
  live. The frame ticker sees no desktop subscriber for an unmounted terminal;
  a newly mounted terminal installs a fresh channel and gate.
- **Browser/WS.** The `watch` channel keeps only the newest value, so a slow
  client silently skips frames. Frames carry a Rust-internal `seq`
  (`GridWatchFrame`); when the reader sees a gap it re-serializes the whole grid
  instead of forwarding a delta onto a row map with holes in it. The sequence
  never reaches the wire — the frame format is unchanged.
- **Hidden terminals** decode each frame (the bell rides in the header) but ack
  on a 400 ms trailing timer, below the ticker's deadline: a background tab drops
  to ~2 frames/s without making the ticker's stuck-frontend warning fire.

**Coalesced scroll (`pending_scroll`).** The wheel and the scrollbar drag do not
take the vt lock: `terminal_scroll_to_offset` — the Tauri command and the
`POST /sessions/{id}/terminal/scroll-to-offset` route alike — stores the absolute
display offset in a per-session `AtomicI64` and marks the grid dirty. The ticker
consumes it (`take_pending_scroll`) under the lock it already holds, so a burst of
wheel events costs one grid move, latest wins, and scrolling never contends with
the PTY output processor.

The entry is created by `spawn_reader_thread` — the one funnel every session
creation path shares — and dropped by `remove_live_session_state`. It belongs to
the **session**, not to a subscriber: when `subscribe_terminal_grid` owned it, a
session only a browser had ever rendered had nowhere to record a scroll, and
closing the desktop terminal disabled scrolling for an attached browser. For the
same reason the ticker applies a pending target even when nobody is subscribed —
`/terminal/scroll-info` and the row reads answer from the display offset, so a
target dropped there would disagree with every later read of the session.

Frames and styled-row chunks cross the desktop IPC as **raw bytes**
(`tauri::ipc::Channel<tauri::ipc::Response>`, `tauri::ipc::Response`). A bare
`Vec<u8>` takes Tauri's blanket `IpcResponse` impl and is serialized as a JSON
array of decimal numbers — ~250 KB of string per 110 KB frame, plus an extra IPC
round trip. The HTTP twin (`/sessions/{id}/terminal/styled-rows`) answers
`application/octet-stream` for the same reason.

This coalesces rapid writes (e.g. spinner CR+erase+rewrite within 16ms) into a single frame, eliminating flicker from intermediate erase states. The ticker exits when the reader's `running` flag clears, with a final flush to avoid losing the last frame.

**Synchronized output (DEC mode 2026).** TUIC advertises `Sy` in the spoofed `TERM_FEATURES`, so agents — Codex in particular — wrap each repaint in `ESC[?2026h` … `ESC[?2026l`. The vendored VTE buffers those bytes and applies them atomically on ESU, but its 150ms `SYNC_UPDATE_TIMEOUT` is **passive**: it records a deadline and never fires. The embedder must enforce it, and the ticker is the only wakeup that can — by definition no further PTY bytes are coming.

The ticker therefore checks the deadline **before** the non-dirty early return, via `flush_sync_timeout_if_needed()` on `VtLogBuffer`. Three details are load-bearing:

- A per-session `sync_update_active` AtomicBool, published by the reader after each `process()`, keeps idle sessions from taking the vt lock every 16ms. It mirrors real parser state, so a nested BSU (which re-arms the deadline rather than closing the update) keeps it set.
- A timeout flush **bypasses** `grid_send_min_interval_ms()`. A protocol deadline is not animation; throttling it back a tick defeats the purpose.
- Teardown calls `force_stop_sync_if_buffered()` before the final serialize — session exit is the other "no more bytes arrive" case, and without it buffered output dies with the session.

Without this enforcement a single BSU whose ESU is delayed or lost freezes the tab **indefinitely**: content buffers invisibly and only a later ESU releases it. That was the cause of Codex streaming appearing to eat text and then dump it all at once, and it made any binary containing the BSU bytes a permanent tab wedge.

### Headless Reader Thread

Foreground agent detection is shared by the desktop command, HTTP foreground endpoint and backend silence timer. Spawn records the root role explicitly: shell PTYs use `Shell`; direct IPC, HTTP and MCP agent spawns use `DirectProgram`. On macOS and Linux, the retained child PID and PTY foreground process group identify the owning root without a shell-name list. In a shell PTY, returning to that root revokes an observed identity only when its current image is not a supported agent. A shell can exec into an agent without changing its PID: detection classifies the current root image rather than assuming its spawn role still describes the executable. An agent exec at the root ends the session when it exits; it does not return to an owning shell. A script wrapper named bash is a different foreground group and counts as observed. Unknown wrappers can retain a run-config preset; a startup helper such as direnv/nvm may therefore disarm the preset early, an intentional fail-closed trade-off. Before any child is observed, the startup preset remains armed.

A direct agent has no shell to return to. Its identity remains valid while its root lives; a foreground child (including a nested shell, git or an editor) holds unattended submit and mail wake until the root regains the foreground. Missing root role, PID or foreground evidence also holds input. Mirrors without the owning daemon's answer never infer a role. A different discovered agent drops preset provenance. Creation seeds provenance synchronously; delayed lifecycle events cannot re-arm an observed identity.

OS samples receive a generation under the session lock. Older samples cannot overwrite a newer identity or foreground-input hold; screen reclassification also verifies ownership of its generation. Identity changes reclassify the existing terminal screen so a quiet composer does not retain a shell-era cache. The refresh return value is the effective foreground detection, which can differ from the retained startup preset; lifecycle consumers read stored state.

The output reader checks the grid's alternate-screen state for agent sessions. On the first entry per session, it logs a warning with the agent type and detected CLI version. This catches agent versions or launch paths that bypass the native-scrollback launch defaults without spamming on repaints.

PTY identity applies Claude's primary-screen environment control only while that agent's `prevent_alt_screen` setting is enabled. `apply_agent_screen_env` covers IPC, HTTP and MCP spawns before explicit caller environment values are installed. Rust also probes Codex, Grok and OpenCode when creating a shell PTY and exports supported flags and subcommand exclusions into its environment. The zsh, bash and fish wrappers apply those values to manually typed agent commands; `command <agent>` bypasses them. The same Rust policy builds structured launch arguments, with a bounded CLI help probe that retries inconclusive checks.

Grid WebSocket attachment sends the existing viewport as a full frame, including history metadata, without waiting for new PTY output. If a live grid watch has no available frame, the server sends `{"type":"grid-replay-empty"}` so an idle empty session completes replay explicitly. The client bounds initial replay to 15 seconds and reconnects with a persistent error toast when replay stalls or decoding fails. Healthy idle sessions have no subsequent-output deadline.

`spawn_headless_reader_thread()` — used for HTTP-created sessions (no Tauri app handle). Same pipeline but skips Tauri event emission; only writes to ring buffer and WebSocket. Includes `extract_question_line()` for silence-based question detection, session lifecycle events (`session-created`, `session-closed`), and full output parser integration.

Named agent sessions propagate their stable `display_name` through the
`session-created` event. That launch label is a replaceable base title: OSC and
structured intent titles may update it. An independent
`display_name_is_custom` flag protects only an explicit user rename and survives
frontend reconnection. `display_name_from_spawn` records that `agent action=spawn`
named the session, so a reloaded tab still refuses the agent's OSC 0/2 title; a
non-custom `set_session_name` (an OSC or intent title synced back) keeps it, and
only a user rename clears it. Session snapshots also carry `is_remote`, so reconnecting
an HTTP/MCP-created PTY does not lose orchestration-only notification muting.

## Shell Resolution

```rust
pub(crate) fn resolve_shell(override_shell: Option<String>) -> String
```

Priority:
1. User override from settings (`override_shell`)
2. Platform default via `default_shell()`

Platform defaults:
- macOS: `/bin/zsh`
- Linux: `$SHELL` environment variable, fallback `/bin/bash`
- Windows: `powershell.exe`

## Buffer Types

### Utf8ReadBuffer

Handles the case where a multi-byte UTF-8 character (e.g., emoji, CJK) is split across two reads:

```rust
impl Utf8ReadBuffer {
    fn push(&mut self, new_bytes: &[u8]) -> String  // Returns valid UTF-8, keeps remainder
    fn flush(&mut self) -> String                     // Force-flush (lossy conversion)
}
```

### EscapeAwareBuffer

Prevents ANSI escape sequences from being split between two emissions. Detects incomplete CSI (`\x1b[...`), OSC (`\x1b]...`), and other escape sequences:

```rust
impl EscapeAwareBuffer {
    fn push(&mut self, input: &str) -> String  // Returns safe-to-emit portion
    fn flush(&mut self) -> String              // Force-flush buffered escapes
}
```

### OutputRingBuffer

Fixed-capacity circular buffer (2 MB, `OUTPUT_RING_BUFFER_CAPACITY`) that stores recent output for MCP access:

```rust
impl OutputRingBuffer {
    fn write(&mut self, data: &[u8])                    // Append data
    fn read_last(&self, limit: usize) -> (Vec<u8>, u64) // Read last N bytes
}
```

### VtLogBuffer

**Module:** `src-tauri/src/state.rs`

VT100-aware extractor that captures clean log lines from PTY output. Designed for mobile/browser clients that need readable text without ANSI noise or TUI screen garbage.

An open Claude Ink choice dialog is retained in the mobile screen payload;
its highlighted `❯ N. option` row must not act as a composer cutoff. The
agent-specific choice parser publishes its navigation contract through
`SessionState.choice_prompt.selection_mode`.

```rust
impl VtLogBuffer {
    fn new(rows: u16, cols: u16, capacity: usize) -> Self  // Create with terminal size
    fn process(&mut self, data: &[u8]) -> Vec<ChangedRow>   // Feed raw PTY bytes, return changed rows
    fn resize(&mut self, rows: u16, cols: u16)              // Update terminal dimensions
    fn screen_rows(&self) -> Vec<String>                    // Current VT100 screen content (for slash menu detection)
    fn screen_log_lines(&self) -> Vec<LogLine>              // Styled screen rows for mobile/REST (structural tokens stripped)
    fn lines_since_owned(&self, offset: usize, limit: usize) -> (Vec<LogLine>, usize) // Paginated reads (absolute offset, structural tokens stripped, chrome lines skipped)
    fn total_lines(&self) -> usize                          // Monotonic counter (never decreases on eviction)
    fn oldest_offset(&self) -> usize                        // Absolute offset of oldest retained line
}
```

**`ChangedRow`** — describes a row that changed between two `process()` calls:

```rust
struct ChangedRow {
    row_index: usize,   // 0-based row in the VT100 screen
    text: String,        // Clean text content (no ANSI)
}
```

**How it works:**

1. Owns the session's `TerminalGrid`, backed by Alacritty (24 rows × 220 cols default); the same grid supplies rendered frames, scrollback, copy and search
2. On each `process()` call, compares current screen rows against previous snapshot
3. Newly scrolled primary-history rows are extracted into the separate `LogLine` log
4. **Separate alternate-screen contracts:** changed rows are still returned while a TUI app owns the alternate screen, so status/intent/question parsers keep working. Durable log extraction reads only primary-screen history, so fullscreen repaint noise never reaches mobile/MCP logs
5. The grid retains 10,000 history rows per screen; the separate log is bounded by `VT_LOG_BUFFER_CAPACITY` (10,000 lines). Oldest entries are dropped when each buffer is full. The grid history cannot be reduced as an independent log scratch buffer
6. **Monotonic cursor:** `total_lines()` returns a monotonically increasing count of all lines ever pushed (not the current buffer length). Clients use this as a stable cursor for paginated reads via `lines_since_owned(offset, limit)`. If a client's saved offset falls in the evicted range, it is clamped to `oldest_offset()`

**Resize:** When the PTY is resized, `VtLogBuffer.resize()` keeps the parser in sync and clears the previous-row snapshot (avoids false scroll detection after resize). If an alternate-screen app is active, the durable-log cursor is synchronized against the inactive primary grid, not the unrelated alternate history; normal shell capture therefore resumes on the first line after exit.

Each session gets its own `VtLogBuffer` stored in `AppState.vt_log_buffers: DashMap<String, Mutex<VtLogBuffer>>`.

### Selection extraction

`TerminalGrid::get_selection_text()` reads grid-relative scrollback coordinates, skips wide-character spacer cells, removes row padding, and joins rows carrying Alacritty's `WRAPLINE` flag so a visual wrap does not become a newline. Before returning clipboard text, it removes only contiguous multi-line Claude visual gutters with the exact `NBSP NBSP ▎` prefix. A lone marker, ASCII-indented block character, and non-breaking spaces inside the selected content remain unchanged.

Desktop and HTTP selection requests may carry the displayed frame's
`historyBase` alongside those row coordinates. The shared backend read rebases
them under the grid lock, preventing output-driven history eviction between
frame delivery and copying from silently selecting another row. Requests whose
endpoints have been evicted are rejected. The frontend retains selection in
eviction-stable coordinates (`historyBase + grid-relative row`); legacy callers
without a history snapshot continue to address the current retained grid.

## OSC 7 CWD Tracking

Shells that emit OSC 7 (`\x1b]7;file://hostname/path\x07`) report the current working directory after each command. TUICommander uses this to keep the Rust-side `PtySession.cwd` in sync:

1. **Backend handler:** the VT parser surfaces `TermEvent::Osc7(url)`, and `parse_osc7_cwd()` decodes the `file://` URL (`pty.rs`).
2. **Session update:** `PtySession.cwd` is written on the Rust side directly — the frontend is not in this path and has no IPC command to call.
3. **Push to clients:** the new cwd is dual-emitted as `pty-cwd-{session_id}` (desktop) and `AppEvent::PtyCwd` (bus → SSE/WS), both carrying `{ cwd }` so `CanvasTerminal` needs no per-transport branch. The frontend updates `terminalsStore` from that event and re-runs `reconcileTerminalOwnership`.
4. **Restart recovery:** The persisted cwd is used during session restore so reopened terminals start in the correct directory.
5. **Worktree reassignment:** When the cwd changes to a path inside a different worktree, the terminal tab is reassigned to the corresponding branch in the sidebar.

## Shell Environment Variables

`build_shell_command()` sets these environment variables for spawned PTY sessions:

| Variable | Value | Purpose |
|----------|-------|---------|
| `COLORTERM` | `truecolor` | Advertise 24-bit color support |
| `KITTY_WINDOW_ID` | `1` | Signal kitty keyboard protocol support for heuristic detection by Ink-based agents |
| `TERM_PROGRAM` | `ghostty` | Satisfy Claude Code's terminal allow-list for kitty protocol; also prevents macOS `/etc/zshrc` from sourcing `zshrc_Apple_Terminal` |
| `TERM_PROGRAM_VERSION` | `3.0.0` | Passes Claude Code's version gate (rejects `^[0-2]\.`) |

Additionally, `CLAUDECODE` is removed from the environment (`env_remove`) to prevent nested-session detection when TUICommander itself runs inside a Claude Code session. `NO_COLOR` is also removed from every PTY command immediately after construction because it may belong to a Codex parent that launched TUICommander, not to the independent child session. This does not force application color or override explicit command flags; a deliberate per-agent environment may restore `NO_COLOR` after sanitization.

The same PTY command sanitizer removes Cargo package and executable metadata (`CARGO`, `CARGO_MANIFEST_*`, `CARGO_PKG_*`, `CARGO_BIN_NAME`, `CARGO_BIN_EXE_*`, `CARGO_CRATE_NAME`, `CARGO_PRIMARY_PACKAGE`, `CARGO_FEATURE_*`, `CARGO_CFG_*`), build paths and jobserver state (`CARGO_TARGET_DIR`, `CARGO_TARGET_TMPDIR`, `OUT_DIR`, `CARGO_MAKEFLAGS`), build settings (`CARGO_INCREMENTAL`, `CARGO_ENCODED_RUSTFLAGS`, `RUSTFLAGS`, `RUSTC`, `RUSTC_LINKER`, `RUSTC_WRAPPER`, `RUSTC_WORKSPACE_WRAPPER`, `RUSTDOC`, `HOST_CC`, `HOST_CXX`, `HOST`, `TARGET`, `PROFILE`, `NUM_JOBS`, `OPT_LEVEL`, `DEBUG`), build-script dependency metadata (`DEP_*`), and all `MBX_*` keys inherited from TUICommander's Cargo/mbx development launch. Those values describe the TUIC build, not the repository opened in the PTY. User preferences such as `CARGO_HOME` and `CARGO_TERM_COLOR` remain inherited; explicit per-agent or run-config environment values applied after sanitization can restore any removed key when requested.

## Child Process Priority

Each spawned shell is given a lower scheduling priority right after spawn
(`lower_pty_child_priority()`), so heavy workloads run inside a pane (`cargo
build`, bundlers, test runners) yield CPU to TUIC's own render loop and the rest
of the system. A child inherits the parent's priority **at fork time**, so every
process the shell later spawns is deprioritized too. The effect only bites under
contention — an idle machine still runs the build at full speed.

| Platform | Mechanism | Default |
|----------|-----------|---------|
| macOS / Linux | `setpriority(PRIO_PROCESS, …)` | nice **+10**, override via `TUIC_PTY_NICE` |
| Windows | `SetPriorityClass(BELOW_NORMAL_PRIORITY_CLASS)` | fixed |

Validated on an M4 Max under 14-core saturation: TUIC's UI goes from frozen
(nice 0) to responsive (nice +10). `BELOW_NORMAL` (not `IDLE_PRIORITY_CLASS`) is
the Windows analog — `IDLE` only runs when the whole system is idle, the
equivalent of macOS QoS-background, which would make builds crawl.

### macOS Thread QoS Elevation

On macOS, the PTY **reader thread**, the **frame ticker**, and the **keystroke-write thread** are all raised to `QOS_CLASS_USER_INTERACTIVE` via `pthread_set_qos_class_self_np` (`raise_thread_for_interactive_io()` in `src-tauri/src/pty.rs`, `thread_qos` module). This is complementary to the child-process renice: on Apple Silicon the scheduler is QoS-band driven — `nice` only reorders threads within a band. Without this elevation, TUIC's interactive-path threads ran in the default QoS band alongside compiler worker threads, causing input latency under heavy builds. Raising to `USER_INTERACTIVE` puts the interactive path in a higher scheduler band. macOS-only; a no-op on Linux/Windows.

## Session Conflict Flag File

When an agent reports a session conflict (session already in use or not found), TUICommander handles it via a flag-file mechanism instead of writing directly to the PTY.

**Flow:**

1. The output parser detects a session conflict message (`ParsedEvent::AgentSessionConflict`)
2. `ChunkProcessor` calls `mark_session_conflict()`, which creates a flag file named `no-session-inject.<TUIC_SESSION>` in the app config directory
3. Shell wrapper functions (zsh, bash, fish) check for this flag file before injecting `--session-id $TUIC_SESSION`
4. If the flag file exists, the wrapper skips session-id injection, allowing the agent to start a fresh session

This replaced the previous `maybe_reset_tuic_session` approach, which wrote `export TUIC_SESSION=...` directly to the PTY. Direct PTY writes could corrupt TUI output (e.g., Ink-based agents in raw mode). The flag-file approach is safe because it uses the filesystem as a side-channel — no bytes are injected into the terminal stream.

A debounce (`last_session_conflict_mark`) prevents creating multiple flag files within a short window for the same session.

## Ctrl-U Prefix Handling

Single-key PTY writes that should clear the current input line prepend `\x15` (Ctrl-U) on POSIX shells. The selection is **shell-family aware**, not host-platform aware: the detected shell (`bash`/`zsh`/`fish` → POSIX, `powershell`/`cmd` → Windows) drives the choice. Mixing PowerShell on macOS or a POSIX shell via WSL/MSYS now behaves correctly. Native Windows shells skip the prefix entirely to avoid inserting a literal `^U`.

Frontend input helpers route through `src/utils/sendCommand.ts`:
- `sendCommand(fn, text)` — full command: `Ctrl-U` (family-gated) + text + `\r`. With an agent attached, Ctrl-U precedes text by 50 ms. Enter follows text by 200 ms for Codex and 50 ms for other known agents. If the type is unknown but a non-shell process owns the foreground, the frontend uses agent framing and the 200 ms gap; a failed foreground probe keeps shell framing and delays Enter. Claude Code strips a Ctrl-U inside a long pasted text; Codex suppresses Enter for 120 ms after a paste burst.
- `sendPtyKey(fn, key)` — pass-through single key/escape sequence. No prefix, no trailing CR. Use for `ChoicePrompt` option keys, TUI app navigation, and any raw-stdin interaction.

Never write `text + "\r"` directly to a PTY — see `AGENTS.md`.

## OSC 133 Semantic Prompts

When the shell emits OSC 133 markers (modern bash/zsh/fish with the integration enabled), the reader records clean command lifecycles into the per-session knowledge store:

| Marker | Meaning |
|--------|---------|
| `OSC 133;A` | Prompt start — delimits a new prompt line |
| `OSC 133;B` | Command start — the user has pressed Enter, command is about to run |
| `OSC 133;C` | Command output start |
| `OSC 133;D[;exit_code]` | Command completed with the given exit code |

`ChunkProcessor.record_osc133_outcomes` consumes the markers and writes a `CommandOutcome { command, cwd, exit_code, classification, duration_ms, output_snippet }` into the session knowledge store. Classification is one of `Success`, `Error { error_type }`, `TuiLaunched { app_name }`, `Timeout`, `UserCancelled`, `Inferred`. `error_type` is inferred from the output snippet (e.g. `rust-error-borrow`, `npm-missing-module`, `python-traceback`).

**Fallback:** when OSC 133 is absent (plain shells, remote sessions), the silence timer still records an `Inferred` outcome so the AI agent loop has *something* to learn from. The `has_osc133_integration` flag on `AppState` tracks per-session whether real markers have been seen.

Persistence lives at `<config_dir>/agent-knowledge/<session_id>.json`. A 2 s debounced background task (`spawn_persist_task`) flushes `knowledge_dirty` sessions to disk. `load_all` rehydrates stores on app start.

## TUI Application Detection

`src-tauri/src/ai_agent/tui_detect.rs` tracks alternate-screen enter (`ESC[?1049h`) and leave (`ESC[?1049l`) to classify the terminal as:

```rust
enum TerminalMode {
    Shell,
    FullscreenTui { app_hint: Option<String>, depth: u8 },
}
```

`depth` is a counter for nested alt-screen pushes (e.g. `less` invoked from inside `vim`). Known app hints — matched heuristically from nearby screen rows — include `vim`, `nvim`, `htop`, `btop`, `lazygit`, `less`, `tmux`, `claude`, and others. The mode is surfaced on `SessionState.terminal_mode` and used by:
- The session knowledge store — `tui_apps_seen` accumulates from it. Its two
  former readers, the ReAct loop's `get_context` tool and the
  `SessionKnowledgeBar` footer, went with the embedded AI engine (#784-0aec).
- The agent safety layer — blocks Ctrl-U prefix injection while a TUI app is in the foreground.

## Silence-Based Question Detection

The reader thread tracks output silence to detect unanswered agent prompts. When the terminal stops producing output for 10 seconds after a line ending with `?` is detected, the session is treated as waiting for input. This complements the instant pattern-based detection in the output parser and catches generic questions that would cause too many false positives if detected immediately (e.g., streaming fragments like "ad?", "swap?").

**Question extraction:** `extract_question_line()` scans changed rows for a candidate, excluding `suggest: [ … ]` protocol rows and their wrapped continuations even when an item ends in `?`. A visible input-box anchor makes chat order authoritative: only the latest chat content above the current prompt may become a question. The changed-row fallback is used only when no prompt anchor is available. This prevents scroll/repaint from resurrecting a question retained above a later answer or completion. Question events carry the input `turn_epoch`, and the state accumulator rejects an event produced by an older turn.

**Codex approval cancellation:** An `Action Required` title raises a confident question. If Codex then paints `You canceled the request` after that approval, shows its ready or interrupted composer, and drops the title, the PTY reader clears the matching question. The clear carries the originating question text and turn epoch; an older cancellation in scrollback or a newer, different question cannot clear the current wait.

**Echo suppression:** When the user submits a line — including bare Enter — the shared desktop/HTTP bookkeeping advances the turn, clears the current wait, and activates a 500ms suppression window (`suppress_user_input`). During this window, matching PTY echo is ignored for question detection.

**Single threshold:** All silence-based questions use a uniform 10-second timeout regardless of whether new output has arrived since the question was detected.

## Shell State (Busy/Idle) Detection

The backend combines explicit lifecycle markers, agent-specific screen evidence,
real output, and silence to emit `ShellState` events (`busy`/`idle`). Rust is the
single source of truth — the frontend does not derive activity from raw PTY data.
Before the first lifecycle observation, shell state is absent and detected-agent
state is `starting`; the internal null sentinel is never serialized as `idle`.

PTY lifecycle events update the authoritative `SessionState` through a lossless
single-consumer lane. The global broadcast bus remains the live SSE/WS transport,
where slow consumers may reconnect after lag, but a dropped broadcast copy cannot
strand the sticky awaiting/idle state.

**Transitions:**
- **Completed turns:** A protocol idle marker survives ordinary output, decorative animation, and subsequent ready-screen timer ticks. Activity metadata must not clear idle evidence before `record_busy` compares ranks. New submitted input or accepted semantic working evidence can reopen the turn.
- **Explicit markers:** OSC 133 shell markers and OSC 7770 agent hooks transition immediately. Output silence cannot override an observed hook `busy`; it ends on hook `idle`, a confirmed interruption, process exit, or a stable ready composer after the submitted turn produced real activity. The last path recovers safely when an idle hook is missed without letting the previous turn's composer cancel a fresh submission. A Claude submission following a hook idle also returns to idle after five minutes with no output or subsequent busy hook when a transcript view hides the composer.
- **→ busy:** A submitted agent prompt, real output, an animated spinner, or an agent-specific `Working` screen transitions via atomic CAS (`try_shell_transition`). Every successful CAS writes a `Shell state →` trace, including injection claims and rollbacks. Positive screen evidence is evaluated even while the stored state is idle, so false-idle is self-healing.
- **→ idle:** The 1s silence timer is the sole heuristic idle path. Plain shells use 500ms; agents use 2.5s and must have no active sub-tasks. Agents with ready-screen adapters require the ready prompt to remain stable for 1.5s.
- **Interrupts:** Ctrl-C and bare Escape record `interrupt pending` but never force idle. Idle follows only after an interrupted/ready screen, explicit Stop, or process exit.
- **Nested prompts (plain shells only):** an interactive subshell — `sh`, `bash -l`, `su`, `sudo su` — is one OSC 133 command that never ends, because the inner shell has no integration of its own and so never emits the closing marker. The outer shell stayed latched `busy` for the subshell's whole life while the user looked at an idle prompt. A plain shell (no `agent_type`) that has been silent for 3s therefore has its foreground process group inspected: if that group and everything under it are shells and privilege wrappers, the explicit busy marker is overruled and the silence path may idle it. Any non-shell descendant (`dd` under `sudo`, a `-c` script) keeps it busy, and the reader still relatches busy on the next byte of output, so this only ever moves an idle prompt. Agents are excluded — their ready-screen adapters already own this. The probe reads the app-wide process snapshot, whose 1s refresher now counts these sessions as demand.

**Movement is the default busy signal (#446-596f):** "if the text above the input area moves, the agent is active." Post-cutoff `changed_rows` are text-equality diffed (`TerminalGrid::process`), so a byte-identical repaint produces no ChangedRow. Static completed summaries, hints, HUD bars, and banner art are inert. Spinner rows among the changed rows additionally refresh `last_output_ms` while they animate. Claude and Codex have narrowly scoped semantic presence exceptions described below because current versions can freeze a valid active status while a child or blocking hook runs.

**Agent screen adapters:** Gemini and Aider remain prompt-based (`Ready` or `Unknown`). Gemini and Codex accept composers only in the current bottom chrome zone (or the final three rows when no input box can be identified), so a historical submitted prompt or markdown quote cannot report `Ready`. Codex detects `Working`/`Ready`/`Interrupted` from its semantic status near that current composer; both `›` and the newer `»` composer glyph are accepted. Claude treats only a spinner-prefixed phase containing an ellipsis and parenthesized progress as `Working`; this outranks the empty `❯` composer that current Claude versions leave visible during long tools. Completed summaries such as `✻ Sautéed for 1m 25s` remain `Ready`. If Claude emits a premature Stop/suggest before a blocking Stop hook, a live phase marker reopens that turn and clears the stale completion suggestions. Grok similarly keeps its `❯` composer visible during a turn: a leading Braille spinner in its bottom status row is `Working`, and the stable composer becomes `Ready` only after that row disappears. These adapters repair the shell's long-lived OSC 133 busy marker even when native hooks are unavailable or disabled.

**Signal precedence and confirmation:** Explicit hook busy > current Claude/Codex/Grok semantic Working marker > movement (real output / animated spinner) > silence. A ready prompt visible from the previous turn cannot cancel a newly submitted prompt until real activity has been observed; after activity, a stable ready composer can repair a missed hook idle. A current-turn completion marker prevents a stale static Codex Working row from relatching BUSY; movement of that exact semantic row can reopen a Codex internal continuation that starts without PTY input. Claude's current live phase marker can supersede a premature completion from a blocking Stop hook. A pending process probe or confirmed meaningful descendant still owns the task lifecycle. Hook-based question suppression activates only after an OSC 7770 state marker is actually received.

**OSC 777 notification classification:** OSC 777 `notify` is a desktop-notification transport, not an awaiting-state protocol. Raw-stream parsing promotes only unambiguous permission or approval wording (`needs your permission`, `approval required`) to a confident question. Claude's `is waiting for your input` body also arrives after a normal reply at the ready composer; promoting it flashed Waiting input until the silence timer retracted it. Ink pickers use their visible footer and full-screen presence recovery, including hook-instrumented sessions. A debug trace at the session-state setter records the confidence, turn epoch, hook configuration and whether a generic Claude notification reached it, without logging the question text.

**State-regression capture:** Enable `POST /diagnostics/capture` before reproducing (`{"enabled":true,"session_id":"<id>"}`), stop it afterward, and copy the exact `<config dir>/captures/<id>.tcap` file into `src-tauri/src/fixtures/agent_prompts/`. Set `TUIC_CAPTURE_DIR` to an absolute path before launch when captures must go elsewhere; a relative value rejects activation. `GET /diagnostics/capture` reports the selected directory and bytes written. The desktop equivalents are the `get_pty_capture` / `set_pty_capture` commands, surfaced as **Capture Session** in the tab context menu whenever `isPerfDebug()` is on (dev by default). TUICCAP2 records the initial terminal rows/columns and preserves input/output ordering, original chunk boundaries, and monotonic timestamps. The decoder still accepts geometry-less TUICCAP1 and legacy output-only `.raw` fixtures; replay code must provide independently observed dimensions for those files. Do not build fixtures from `/sessions/:id/output`: the bounded ring can overwrite the signal and its JSON string is lossy UTF-8.

**Reading a corpus back:** `detection_over_capture_corpus` (ignored test in `pty.rs`) replays every capture in a directory through the production composition and reports, per file, the event kinds produced, whether an awaiting badge was left set at the end, and how often `find_chrome_cutoff` found no anchor — the fail-open branch, where no trim happens and every status-line row reaches every parser.

```text
TUIC_CAPTURE_CORPUS="$HOME/Library/Application Support/com.tuic.commander/captures" \
  cargo test -p tuicommander detection_over_capture_corpus -- --ignored --nocapture
```

Captures hold real session content and are not committed; the harness measures them in place. Replay fidelity is bounded by the fixed 41×128 grid it reconstructs — a session recorded at another width wraps differently, which moves chrome detection.

**Safety consumers:** For agents with a verified screen adapter, peer-message injection and Unix auto-standby require confirmed idle (explicit Stop/OSC or stable ready screen). A silence-only idle can update the cosmetic state but cannot type into or `SIGSTOP` a potentially working agent. Agents without an adapter retain the legacy heuristic behavior until their UI is characterized.

**Task lifecycle is separate from shell activity:** `shell_state=idle` means the
PTY is quiet; it does not prove that the assigned task finished. An agent's
`suggest: [ ... ]` marker explicitly closes the current task epoch and produces
`agent_state=completed` plus a `state_change: completed` parent notification.
Likewise, a visible ready composer may coexist with an autonomous background
command. While a meaningful descendant of the agent is alive, session state
reports `background_work=true` and keeps `agent_state=working`; `shell_state`
remains `idle` because terminal input readiness is a separate fact.

A descendant is excluded from that judgement two ways. By name: the persistent
integration helpers (`mdkb`, `tuic-bridge`, and `node_repl`) and Claude's
standalone timed `caffeinate -i -t <seconds>` assertion. Unix classification
checks both `comm` and the authoritative argv path from unlimited-width `ps`
output; a `caffeinate` invocation that wraps a command remains meaningful
background work. And by age: a descendant that started within
`AGENT_STARTUP_WINDOW_SECS` (60s) of the agent itself is session plumbing
whatever it is called.

The startup window exists because the name list cannot be completed.
`codex-code-mode-host` arrived with Codex 0.149.0, and an MCP server started
through `npm exec` reports as `npm` — a name that must keep meaning work,
because a turn also runs npm. Measured on a live 14-session instance on
2026-08-23, every agent owned at least one such daemon, so `background_work` was
a constant `true` and no session could ever leave `working`. In the same
snapshot every daemon appeared within 18s of its agent while work spawned by a
turn was hundreds of seconds younger, which is the gap the window sits in.
Ages come from the `etime` column of the shared `ps` snapshot. Where the
platform cannot supply one — Windows `PROCESSENTRY32` carries no creation
time — the window is skipped and the name list is the only rule, which errs
toward reporting work rather than hiding it. Parent `idle` lifecycle mail is
deferred until the real descendant exits, while confirmed-ready message
delivery keeps using the terminal-readiness gate. The first confirmed-ready
observation and every explicit agent IDLE marker arm a generation boundary:
idle/completed lifecycle output waits until a process snapshot newer than that
observation or marker has been reconciled. Fresh working evidence starts a new
readiness episode even within the same task epoch: it invalidates only the
satisfied or pending snapshot boundary, so the next ready observation must
reconcile a newer snapshot while preserving tracked background work and the
snapshot generation. That reconciliation is also an authoritative idle edge for
the session itself: if a registered orchestrator accumulated inbox mail while
the probe still reported it as working, settling the probe reevaluates the
pending payload-free inbox wake immediately. It does not wait for a later child
lifecycle event, and the child payload remains in the inbox. One app-wide process snapshot is collected at most once
per second on Tokio's
blocking pool and shared by every session. The refresher runs only while a
ready probe or tracked background process needs it, skips missed interval ticks,
and stops scanning stable idle sessions. Enumeration or parse failures preserve
the prior `background_work` value. On Windows, where Toolhelp does not provide
command lines, generic `node.exe` processes are kept as meaningful work rather
than guessed to be `node_repl` helpers.
Submitting new user or PTY-injected peer input starts a new task epoch immediately,
clearing the prior completion marker and its stale suggested actions before new output arrives.
Claude channel and inbox delivery do not claim a submitted turn; the channel is used only
inside an already working turn. Idle or completed managed composers take the PTY submission
path, and lifecycle changes only after that input or normal activity evidence. Idle
CAS and parent lifecycle notification share the same per-session lifecycle lock;
submitted epoch mutation and its IDLE-to-BUSY transition hold that lock as one
critical section, so a new turn cannot inherit a stale idle notification. The
authoritative parent inbox enqueue occurs under the child lock; parent terminal
wake/dispatch runs only after release, avoiding cross-session lock ordering. A
queued BUSY-to-IDLE transition also carries the task epoch observed before it
waited for the lock and is discarded if a new submitted turn won first.
Without a fresh marker the new task epoch returns to `idle`, not `completed`.

**Transactional peer injection:** Reserving an idle composer creates an ownership token before the PTY write. A failure proven to occur before any byte was written rolls the synthetic BUSY state back to the prior confirmed IDLE state and keeps the message queued. Once any byte may have escaped, failure is `delivery_uncertain`: the session remains conservatively BUSY, the authoritative inbox remains readable, and TUIC does not automatically retry into the terminal. A queued injection waits up to six seconds for Codex and Claude and one second for other agents after Enter for child output plus an agent hook, a new Working screen, or (for Gemini and Aider) the ready prompt disappearing. Codex's stop hook delayed its accepted turn's Working repaint by four seconds; a live Claude notice took 3.8 seconds to leave Claude's internal queue after Enter. A Working screen already present before Enter cannot confirm the new turn: the rb-tool capture showed cursor updates while the queued wake remained in a Codex composer until a manual Enter. A Ready-to-Working transition in output *after* Enter does confirm it, even if the pre-write screen classification was stale. A silent or unrecognised composer reports `delivery_uncertain` and emits an error toast to desktop and browser clients. The toast tells the user to inspect the transcript and composer, and to press Enter once only if the text remains. Further queued entries wait until a human submission or later activity resolves the uncertainty. Enter alone never proves submission. Real output, a Working screen, or an explicit state marker invalidates rollback ownership so a late error cannot erase genuine activity. Amp, Cursor, and Droid retain the previous PTY-write result because they have neither verified screen adapters nor hook support; they need live captures before semantic confirmation can be enabled. `session status` exposes the additive `delivery_uncertain` flag.

Queued confirmation waits run per session so a silent agent does not block other sessions or ordered lifecycle notices on the global injection worker. Each idle edge with a nonempty queue can start a worker; there is no global thread cap. A blocked claim returns promptly, and an active acknowledgement wait lasts at most six seconds for Codex or Claude or one second for other agents. The HTTP queue handler runs the blocking delivery on Tokio's blocking pool. If an agent remains silent with no human input after uncertain delivery, the uncertainty flag and later queued entries remain held until activity or a human submission supplies new evidence.

**Atomic MCP submission reuses the same claim:** `session action=submit` first
requires a confirmed-idle managed agent, empty `InputLineBuffer`, no confident
dialog, and an empty shared injection FIFO. It never adds itself to that FIFO.
The claim marks the session BUSY before any bytes; one PTY writer guard then
spans Ctrl-U, a 50 ms gap, the text in optional bracketed paste, a second
gap (200 ms for Codex or an undetected agent type, 50 ms for known other agents), and CR, so
neither raw input nor a peer can splice the command. A peer arriving after the
claim queues; a peer that claims first makes submission reject.

Foreground detection warns once per session when a non-shell process has no
recognized agent type or run-config fallback. Such a session retains the safe
200 ms Enter gap during managed injection.

After a complete write, the existing input bookkeeping records the original
text and CR, clears slash mode, and advances `turn_epoch` once. The MCP handler
waits up to a bounded deadline for `OutputRingBuffer.total_written` to move past
the pre-Enter offset and labels the existing agent screen state when available.
Local composer clearing, epoch mutation, and the PTY write itself cannot satisfy
that acknowledgement. Movement proves the child terminal reacted, not that the
application understood the command. A complete or uncertain write is never
automatically retryable; only rejection/no-byte failure is.

**User-composed commands share that gate:** the Compose panel's enqueue action
(`enqueue_agent_command` / `POST /sessions/:id/queue`) appends to the same
typed `pending_injections` FIFO as peer delivery rather than writing to the PTY,
so a command typed by the user cannot steer a turn in progress. It is appended
and then flushed, never handed straight to `deliver_message_to_pty`: injecting
ahead of any accepted peer message or user command would reorder delivery. Each
flush pops one entry and leaves the session BUSY, so the queue drains one item per
idle window in global acceptance order. `state.queued_commands`,
`list_queued_agent_commands`, `remove_queued_agent_command` and
`clear_queued_agent_commands` select only user-command entries; clear retains all
peer/orchestrator entries in their original relative order. Each user command
carries a process-unique id so the Compose panel can delete a single entry —
a queue position would shift under the caller as the FIFO drains.

Queue requests may include `idempotencyKey`. A per-PTY bounded recent-key set
reserves acceptance atomically with FIFO append, before the blocking flush. The
last 128 keys survive drain and cancellation and are removed with the PTY;
backend restart also clears them. Duplicate requests do not flush or append.
Both HTTP and desktop IPC return `accepted`, `typed` and current `queued`;
a recognized retry can be accepted even with an empty queue and no new typing.

Each nonempty flush attempt emits one `queue delivery attempt` tracing record with
the session id, agent and shell states at the attempt, queued counts before and
after, whether text reached the composer, whether submission was confirmed, and
whether Enter was sent separately. An uncertain delivery reports uncertain fields
rather than claiming a completed submission; command text is never logged.
Deferred attempts include `defer_reason` (unavailable agent, shell busy,
unconfirmed idle, confident question, partial composer, or a lost claim). If a
Ready screen becomes confirmed after silence already marked the shell idle, the
timer retries this same guarded flush without waiting for another shell
transition; a debug record names that ordering and the pending queue depth.

**Status line ticks:** Animated spinner repaint evidence refreshes both shell activity and `SilenceState`, preventing low-confidence question/tool-error events from contradicting a busy tab. Static mode/footer rows remain chrome only and do not prove activity.

**Status line dedup is per turn:** `ChunkProcessor.last_status_task` keys its dedup on `(turn_epoch, task_name)`, so a spinner rotation inside one turn stays suppressed while the first status line of a *new* turn always re-emits. The epoch must stay in the key because an agent may name every turn identically — Codex always reports `Working`. A session-lifetime dedup swallowed every turn after the first, and since the `status-line` event is the only thing that clears the previous turn's `suggested_actions` (which `session_state_with_shell` reads as a completion marker), the session reported a busy agent as `completed`/`idle` permanently.

**Agent detection:** `detectAgentForTerminal()` fires on shell-state transitions (immediate on idle, 500ms debounce on busy). A 30s fallback poll catches cold starts. This replaces the previous 3s polling interval, reducing syscalls ~30x.

## Amber Tab Styling

Sessions created via HTTP/MCP (remote sessions) are flagged with `isRemote`. The tab bar applies an amber gradient background and amber bottom border (`rgba(251, 191, 36, ...)`) to visually distinguish remote-created sessions from locally spawned ones.

## Concurrency

- Sessions stored in `DashMap<String, Mutex<PtySession>>` for lock-free concurrent access
- Each session's writer has its own shared `Mutex`, independent from the
  `PtySession` metadata lock. User input, HTTP/WebSocket input, agent injection,
  and terminal-generated protocol replies all serialize through that writer.
  A reader may therefore wait for an in-flight write without blocking PTY
  draining, and mandatory device/kitty query replies are never dropped merely
  because session metadata is contended.
- Reader thread holds `Arc<AtomicBool>` for pause signaling
- Metrics use `AtomicUsize` for zero-overhead counting

Claude transcript discovery, session verification, project-directory lookup and subagent paths share the Claude project-path encoder. It replaces every non-ASCII-alphanumeric character with `-`, including Windows drive colons, spaces, Unix dots and underscores. Claude's hashed suffix for paths whose encoded slug exceeds 200 characters remains unsupported.

### Stored terminal marker coordinates

OSC 133 command boundaries and OSC 7770 prompt rows are eviction-stable all-time rows. The PTY reader forwards these coordinates unchanged on IPC and HTTP/WS; it must not add the end-of-chunk history base.

Claude launch settings apply to prompt and option-first launches. Shell wrappers use backend-captured installed CLI help to recognise subcommands and aliases, without probing again at launch. Help is unavailable unless its `Commands:` section has parseable command rows; empty, whitespace-only or truncated help therefore uses the complete recorded Claude help, including `auth` and advertised aliases. Rust publishes this fallback to the shell environment; generated wrappers also embed it for an unusable cached value. No separate fallback verb list is maintained. The exact hidden `remote-control` command also bypasses settings because its reported CLI refusal confirms that requirement. Hyphenated prompts retain settings. Explicit settings and bare mode remain authoritative.

Headless PTY registration uses the requested terminal geometry without a minimum VT width. A same-size resize preserves that width.

MCP retained-output pages use `VtLogBuffer::lines_since_logical` source-row
start/end positions, including omitted chrome slots. The end is a page boundary,
not the total scrollback size. Logical wrap lines stay whole; raw pages use the
existing output ring and original-byte cursors. See [MCP output paging](mcp-http.md#mcp-tool-session-output).

### Claude transcript Chat view

`chat_view::View::advance` reads complete JSONL rows through `transcript_tail`,
projects them with `ClaudeAdapter`, and retains a bounded ACP update log. Each
view retains an open file-identity handle: replacing a JSONL at the same path
resets the conversation even when the replacement is as large as the old file.
The existing one-second ticker emits wake events; snapshot reads keep it alive.
Binding discovery and its ten-second recheck require the parent JSONL, without
requiring a subagents directory. Each frontend watch owns its cursor; delayed
listener cleanup and snapshot replies from an earlier mount cannot clear or
replace the current Chat conversation.
Ticker stops log a WARN with the terminal ID and reason, and failed frontend
refreshes log a WARN before retrying on the next wake or existing keepalive. It
recognizes older prompt-ID rows without `origin`, excludes harness command echoes
and sidechains, and preserves image/PDF result markers and model fallback cards.
Recorded sanitized cases and the last-30-days schema counts are in
[`fixtures/chat_view/recorded`](../../src-tauri/src/fixtures/chat_view/recorded/README.md).
The opt-in `disk_ticker_http_store_browser_follow_proof` test runs the real
snapshot/SSE routes against two fixture-bound disk transcripts, and renders the
production store and Chat component through worktree Vite. It covers appends,
same-path replacement, a new JSONL binding and a CLI/Chat remount. Set
`TUIC_CHAT_FOLLOW_PROOF_DIR` to a directory under the worktree and run it with
`--run-ignored all`; the installed stealth browser wrapper is required. Binding
discovery itself is outside this fixture. Screenshots and `proof.json` stay in
the specified evidence directory.

The opt-in `view_real_transcript_throughput` measurement reads an authorized local
file at runtime through the same path, reporting both the 2 MiB attach and full
parse times plus process peak RSS. Raw transcripts are never committed.

The recorded chunk-path parity gate covers all `.tcap` captures under `src-tauri/`,
including captures in terminal crates. See [Recorded replay oracle](output-parser.md#recorded-replay-oracle)
for scope, first-difference diagnostics and the explicit regeneration command.

### Bound Telegram interruption

The Telegram daemon arms a draft ownership token in `SessionState` under the
session writer mutex. Raw desktop/HTTP input, atomic input pairs, managed agent
submissions, trust answers and retained-composer Enter retries retire that token
before writing bytes. Retirement records the current epoch, so a delayed draft
cannot rearm it before native submission bookkeeping establishes a new turn.
Terminal protocol replies leave it intact. Stop consumes
the current draft, then checks the token, peer mapping and turn epoch under the
same writer mutex. It writes one Escape without feeding the input line editor.
`SilenceState` is released before the native write; synchronous captured output
can therefore traverse the normal chunk processor. Input bookkeeping remains
post-write and keeps its existing per-request boundaries.

## Remote terminal metadata

Desktop terminals owned by a remote connection subscribe to that daemon's
`/sessions/{id}/stream` WebSocket for parsed events, activity, title and exit.
Local desktop terminals retain IPC subscriptions. OSC 0/2 titles and resets
are dual-emitted: `pty-title-{id}` carries the text on IPC, while `PtyTitle`
reaches the per-session socket and `/events` (`pty-title`, `{session_id,title}`).
A reset carries an empty string. Titles do not change semantic session activity
and bypass the lossless state queue. Authoritative lifecycle snapshots and pushes
apply `agent_type` immediately, including removal when the shell returns; the
30-second foreground probe remains a recovery/discovery path.
