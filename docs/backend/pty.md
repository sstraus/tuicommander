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

## Tauri Commands

### Session Creation

| Command | Description |
|---------|-------------|
| `create_pty(config: PtyConfig)` | Spawn a new PTY session. Returns session ID. |
| `create_pty_with_worktree(pty_config, worktree_config)` | Create worktree + spawn PTY in it. Returns `WorktreeResult`. |

### Session Control

| Command | Description |
|---------|-------------|
| `write_pty(session_id, data)` | Write data (user input) to the PTY. |
| `resize_pty(session_id, rows, cols)` | Resize the PTY terminal dimensions. |
| `pause_pty(session_id)` | Pause the reader thread (stops output emission). |
| `resume_pty(session_id)` | Resume the reader thread. |
| `close_pty(session_id, cleanup_worktree)` | Close PTY and optionally remove worktree. |
| `update_session_cwd(session_id, cwd)` | Update session's working directory (called from frontend on OSC 7). |

### Monitoring

| Command | Description |
|---------|-------------|
| `get_orchestrator_stats()` | Active/max/available session counts. |
| `get_session_metrics()` | Total spawned, failed, bytes emitted, pauses. |
| `can_spawn_session()` | Check if under MAX_CONCURRENT_SESSIONS (50). |
| `list_active_sessions()` | List all sessions with cwd and worktree info. |
| `list_worktrees()` | List all managed worktrees. |

## Reader Thread

Each session spawns a dedicated reader thread that reads from the PTY master fd:

```rust
spawn_reader_thread(reader, paused, session_id, app, state)
```

**Processing pipeline per read:**

1. Read raw bytes from PTY master (up to 8KB buffer)
2. Push through `Utf8ReadBuffer` — accumulates bytes until valid UTF-8 boundary, returns safe string
3. Push through `EscapeAwareBuffer` — holds incomplete ANSI escape sequences (CSI, OSC, etc.)
4. Write to `OutputRingBuffer` (64KB circular buffer for MCP access)
5. Broadcast to WebSocket clients (if any connected)
6. Emit Tauri event `pty-output` with `{session_id, data}`

**Pause behavior:** When `paused` flag is set (`AtomicBool`), the reader thread sleeps for 50ms instead of reading. This prevents output flooding during background operations.

**Exit detection:** When the read returns 0 bytes or an error, the thread:
1. Flushes remaining buffered data
2. Emits `pty-exit` event with exit code
3. Removes session from `AppState.sessions`
4. Updates metrics (decrement `active_sessions`)

### Headless Reader Thread

`spawn_headless_reader_thread()` — used for HTTP-created sessions (no Tauri app handle). Same pipeline but skips Tauri event emission; only writes to ring buffer and WebSocket.

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

Fixed-capacity circular buffer (64KB) that stores recent output for MCP access:

```rust
impl OutputRingBuffer {
    fn write(&mut self, data: &[u8])                    // Append data
    fn read_last(&self, limit: usize) -> (Vec<u8>, u64) // Read last N bytes
}
```

## OSC 7 CWD Tracking

Shells that emit OSC 7 (`\x1b]7;file://hostname/path\x07`) report the current working directory after each command. TUICommander uses this to keep the Rust-side `PtySession.cwd` in sync:

1. **Frontend handler:** `terminal.parser.registerOscHandler(7, ...)` in `Terminal.tsx` parses the `file://` URL via `parseOsc7Url()`.
2. **Store update:** The parsed path is written to `terminalsStore` so the UI reflects the current directory.
3. **IPC persist:** The frontend calls `update_session_cwd(sessionId, cwd)` to update `PtySession.cwd` on the Rust side.
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

Additionally, `CLAUDECODE` is removed from the environment (`env_remove`) to prevent nested-session detection when TUICommander itself runs inside a Claude Code session.

## Silence-Based Question Detection

The reader thread tracks output silence to detect unanswered agent prompts. When the terminal stops producing output for a configured duration (10 seconds) after the last line ends with `?`, the session is treated as waiting for input. This complements the instant pattern-based detection in the output parser and catches generic questions that would cause too many false positives if detected immediately (e.g., streaming fragments like "ad?", "swap?").

## Amber Tab Styling

Sessions created via HTTP/MCP (remote sessions) are flagged with `isRemote`. The tab bar applies an amber gradient background and amber bottom border (`rgba(251, 191, 36, ...)`) to visually distinguish remote-created sessions from locally spawned ones.

## Concurrency

- Sessions stored in `DashMap<String, Mutex<PtySession>>` for lock-free concurrent access
- Each session's writer is behind `Mutex` for exclusive write access
- Reader thread holds `Arc<AtomicBool>` for pause signaling
- Metrics use `AtomicUsize` for zero-overhead counting
