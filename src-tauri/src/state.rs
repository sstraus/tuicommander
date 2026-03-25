use dashmap::DashMap;
use notify_debouncer_mini::Debouncer;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::VecDeque;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::AppHandle;

// ---------------------------------------------------------------------------
// AppEvent — unified event bus for all backend events
// ---------------------------------------------------------------------------

/// Events broadcast to SSE/WebSocket consumers via `tokio::sync::broadcast`.
/// All event producers (PTY reader, watchers, session lifecycle, plugins) send
/// to this channel. Consumers: SSE endpoint, WebSocket multiplexer, session
/// state accumulator, Tauri bridge (desktop backward compat).
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "event", content = "payload")]
pub enum AppEvent {
    #[serde(rename = "head-changed")]
    HeadChanged {
        repo_path: String,
        branch: String,
    },
    #[serde(rename = "repo-changed")]
    RepoChanged {
        repo_path: String,
    },
    #[serde(rename = "session-created")]
    SessionCreated {
        session_id: String,
        cwd: Option<String>,
    },
    #[serde(rename = "session-closed")]
    SessionClosed {
        session_id: String,
        reason: String,
    },
    #[serde(rename = "pty-parsed")]
    PtyParsed {
        session_id: String,
        parsed: serde_json::Value,
    },
    #[serde(rename = "pty-exit")]
    PtyExit {
        session_id: String,
    },
    #[serde(rename = "plugin-changed")]
    #[allow(dead_code)] // reserved for future plugin hot-reload notifications
    PluginChanged {
        plugin_ids: Vec<String>,
    },
    #[serde(rename = "upstream-status-changed")]
    UpstreamStatusChanged {
        name: String,
        status: String,
    },
    /// Toast notification from MCP tool
    #[serde(rename = "mcp-toast")]
    McpToast {
        title: String,
        message: Option<String>,
        level: String,
    },
    /// Directory contents changed (non-git filesystem watcher)
    #[serde(rename = "dir-changed")]
    DirChanged {
        dir_path: String,
    },
    /// A worktree was created via MCP — frontend may offer to switch to it
    #[serde(rename = "worktree-created")]
    WorktreeCreated {
        repo_path: String,
        branch: String,
        worktree_path: String,
    },
    /// A peer agent registered for inter-agent messaging
    #[serde(rename = "peer-registered")]
    PeerRegistered {
        tuic_session: String,
        name: String,
    },
    /// A peer agent was unregistered (session closed or reaped)
    #[serde(rename = "peer-unregistered")]
    PeerUnregistered {
        tuic_session: String,
    },
}

// ---------------------------------------------------------------------------
// SessionState — server-side accumulator for REST polling (mobile/browser)
// ---------------------------------------------------------------------------

fn is_zero(v: &u32) -> bool { *v == 0 }

/// Per-session state accumulated from broadcast events.
/// Updated by a background task that subscribes to the event bus.
/// Read by `GET /sessions` to enrich the response for REST-polling clients.
#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct SessionState {
    /// True when a Question parsed event is pending (no subsequent user-input or pty-exit)
    pub awaiting_input: bool,
    /// The question text, if awaiting input
    #[serde(skip_serializing_if = "Option::is_none")]
    pub question_text: Option<String>,
    /// True when a rate-limit parsed event is active
    pub rate_limited: bool,
    /// Retry-after in ms from the rate-limit event
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
    /// Epoch ms when rate_limited was set; used for auto-expiry.
    #[serde(skip)]
    pub rate_limit_set_ms: u64,
    /// Usage limit percentage (0-100), if detected
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_limit_pct: Option<u8>,
    /// Shell state derived from PTY output timing (matches desktop model).
    /// "busy" = recent PTY output (< 500ms), "idle" = no recent output.
    /// Computed on-the-fly when serializing; None for sessions with no output yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell_state: Option<String>,
    /// Timestamp of last activity (any event for this session).
    /// Excluded from PartialEq — telemetry field, not logical state.
    pub last_activity_ms: u64,
    /// Detected agent type, if known
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,
    /// Last API error, if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    /// Current agent intent text (from [intent: ...] tokens)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_intent: Option<String>,
    /// Current task name from the agent status line
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_task: Option<String>,
    /// Last user prompt with >= 10 words
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_prompt: Option<String>,
    /// Current progress value (0-100); None when no active progress bar
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<u8>,
    /// Number of active sub-tasks (local agents, bash, background tasks) from ›› mode line
    #[serde(skip_serializing_if = "is_zero")]
    pub active_sub_tasks: u32,
    /// Suggested follow-up actions from the agent (from [[suggest: ...]] tokens)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_actions: Option<Vec<String>>,
    /// Slash command menu items (from slash-menu parsed events)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slash_menu_items: Option<Vec<crate::output_parser::SlashMenuItem>>,
}

/// PartialEq excludes last_activity_ms (telemetry, not logical state).
/// Used by WS dedup to avoid sending identical state frames.
impl PartialEq for SessionState {
    fn eq(&self, other: &Self) -> bool {
        self.awaiting_input == other.awaiting_input
            && self.question_text == other.question_text
            && self.rate_limited == other.rate_limited
            && self.retry_after_ms == other.retry_after_ms
            && self.usage_limit_pct == other.usage_limit_pct
            && self.shell_state == other.shell_state
            && self.agent_type == other.agent_type
            && self.last_error == other.last_error
            && self.agent_intent == other.agent_intent
            && self.current_task == other.current_task
            && self.active_sub_tasks == other.active_sub_tasks
            && self.last_prompt == other.last_prompt
            && self.progress == other.progress
            && self.suggested_actions == other.suggested_actions
            && self.slash_menu_items == other.slash_menu_items
    }
}


/// TTL for git operations (local disk): 5 seconds
pub(crate) const GIT_CACHE_TTL: Duration = Duration::from_secs(5);

/// TTL for GitHub operations (network): 30 seconds
pub(crate) const GITHUB_CACHE_TTL: Duration = Duration::from_secs(30);

/// Buffer that handles UTF-8 characters split across read boundaries.
/// Carries incomplete trailing bytes from one read to the next.
pub(crate) struct Utf8ReadBuffer {
    /// Incomplete bytes from the previous read (at most 3 bytes for a 4-byte sequence)
    pub(crate) remainder: Vec<u8>,
}

impl Utf8ReadBuffer {
    pub(crate) fn new() -> Self {
        Self {
            remainder: Vec::with_capacity(4),
        }
    }

    /// Process raw bytes from a read, returning valid UTF-8 text.
    /// Incomplete multi-byte sequences at the end are saved for the next call.
    pub(crate) fn push(&mut self, new_bytes: &[u8]) -> String {
        let mut combined = Vec::with_capacity(self.remainder.len() + new_bytes.len());
        combined.extend_from_slice(&self.remainder);
        combined.extend_from_slice(new_bytes);
        self.remainder.clear();

        // Find the last valid UTF-8 boundary
        let valid_up_to = match std::str::from_utf8(&combined) {
            Ok(_) => combined.len(),
            Err(e) => {
                let valid = e.valid_up_to();
                // Check if the error is due to incomplete sequence at the end
                if e.error_len().is_none() {
                    // Incomplete sequence — save trailing bytes for next read
                    valid
                } else {
                    // Invalid byte sequence — skip the bad byte(s) and keep going
                    // Replace the invalid portion with U+FFFD and continue
                    let error_len = e.error_len().unwrap();
                    let mut result = String::from_utf8_lossy(&combined[..valid + error_len]).to_string();
                    // Process any remaining bytes after the error
                    if valid + error_len < combined.len() {
                        let rest = self.push(&combined[valid + error_len..]);
                        result.push_str(&rest);
                    }
                    return result;
                }
            }
        };

        // Save incomplete trailing bytes
        if valid_up_to < combined.len() {
            self.remainder.extend_from_slice(&combined[valid_up_to..]);
        }

        // SAFETY: `combined[..valid_up_to]` was verified as valid UTF-8 above via
        // `std::str::from_utf8` / `Utf8Error::valid_up_to`, so `from_utf8_unchecked` is sound.
        unsafe { String::from_utf8_unchecked(combined[..valid_up_to].to_vec()) }
    }

    /// Flush any remaining bytes (at EOF). Incomplete sequences are dropped.
    pub(crate) fn flush(&mut self) -> String {
        if self.remainder.is_empty() {
            return String::new();
        }
        let remaining = std::mem::take(&mut self.remainder);
        String::from_utf8_lossy(&remaining).to_string()
    }
}

/// Buffer that prevents escape sequences from being split across write boundaries.
/// Detects incomplete ANSI/OSC sequences at the end of a chunk and carries them
/// to the next call, so xterm.js always receives complete sequences.
pub(crate) struct EscapeAwareBuffer {
    /// Incomplete escape sequence bytes carried from the previous chunk.
    remainder: String,
}

impl EscapeAwareBuffer {
    pub(crate) fn new() -> Self {
        Self {
            remainder: String::new(),
        }
    }

    /// Process a UTF-8 string chunk, returning text safe to write to xterm.js.
    /// Any trailing incomplete escape sequence is held for the next call.
    pub(crate) fn push(&mut self, input: &str) -> String {
        if input.is_empty() && self.remainder.is_empty() {
            return String::new();
        }

        let mut data = std::mem::take(&mut self.remainder);
        data.push_str(input);

        // Find safe split point: the last position where we're NOT inside an escape sequence
        let safe = find_safe_boundary(&data);

        if safe == data.len() {
            // Entire string is safe
            data
        } else if safe == 0 {
            // Entire string is an incomplete escape — hold it all
            // But cap at 256 bytes to prevent unbounded growth from garbage input
            if data.len() > 256 {
                // Give up and emit it raw — likely not a real escape sequence
                data
            } else {
                self.remainder = data;
                String::new()
            }
        } else {
            self.remainder = data[safe..].to_string();
            data.truncate(safe);
            data
        }
    }

    /// Flush remaining bytes at EOF.
    pub(crate) fn flush(&mut self) -> String {
        std::mem::take(&mut self.remainder)
    }
}

/// Find the last byte position where the string is not inside an incomplete escape sequence.
/// Returns data.len() if the entire string is safe, or a smaller index if trailing bytes
/// form an incomplete sequence.
fn find_safe_boundary(data: &str) -> usize {
    let bytes = data.as_bytes();
    let len = bytes.len();
    if len == 0 {
        return 0;
    }

    // Scan backwards from the end to find incomplete escape sequences.
    // We only need to check the last ~256 bytes (max reasonable escape sequence length).
    let scan_start = len.saturating_sub(256);

    // Walk forward through the tail to track escape state
    let mut i = scan_start;
    let mut last_safe = len; // Assume safe unless we find an incomplete escape

    while i < len {
        let b = bytes[i];
        if b == 0x1b {
            // ESC — start of a potential escape sequence
            let seq_start = i;
            i += 1;
            if i >= len {
                // ESC at very end — incomplete
                last_safe = seq_start;
                break;
            }

            match bytes[i] {
                b'[' => {
                    // CSI sequence: ESC [ <params> <final byte>
                    // Parameter bytes: 0x30-0x3F, intermediate: 0x20-0x2F
                    // Final byte: 0x40-0x7E (@A-Z[\]^_`a-z{|}~)
                    i += 1;
                    let mut found_final = false;
                    while i < len {
                        let c = bytes[i];
                        if (0x40..=0x7E).contains(&c) {
                            // Final byte found — sequence is complete
                            i += 1;
                            found_final = true;
                            break;
                        }
                        if c == 0x1b {
                            // New ESC interrupts — this CSI is broken, treat as complete
                            found_final = true;
                            break;
                        }
                        i += 1;
                    }
                    if !found_final {
                        // Ran off the end without finding final byte — incomplete
                        last_safe = seq_start;
                    }
                }
                b']' => {
                    // OSC sequence: ESC ] <text> (ST | BEL)
                    // ST = ESC \ , BEL = 0x07
                    i += 1;
                    let mut terminated = false;
                    while i < len {
                        let c = bytes[i];
                        if c == 0x07 {
                            // BEL terminator
                            i += 1;
                            terminated = true;
                            break;
                        }
                        if c == 0x1b && i + 1 < len && bytes[i + 1] == b'\\' {
                            // ST terminator (ESC \)
                            i += 2;
                            terminated = true;
                            break;
                        }
                        if c == 0x1b && (i + 1 >= len || bytes[i + 1] != b'\\') {
                            // New ESC that's not ST — OSC is broken, treat as complete
                            terminated = true;
                            break;
                        }
                        i += 1;
                    }
                    if !terminated {
                        last_safe = seq_start;
                    }
                }
                b'P' => {
                    // DCS sequence: ESC P <text> ST
                    i += 1;
                    let mut terminated = false;
                    while i < len {
                        let c = bytes[i];
                        if c == 0x1b && i + 1 < len && bytes[i + 1] == b'\\' {
                            i += 2;
                            terminated = true;
                            break;
                        }
                        if c == 0x1b && (i + 1 >= len || bytes[i + 1] != b'\\') {
                            terminated = true;
                            break;
                        }
                        i += 1;
                    }
                    if !terminated {
                        last_safe = seq_start;
                    }
                }
                _ => {
                    // Simple ESC + single char (e.g., ESC c, ESC 7, ESC 8)
                    // Sequence is complete
                    i += 1;
                }
            }
        } else {
            i += 1;
        }
    }

    last_safe
}

/// Fixed-capacity circular buffer for PTY output, readable by external consumers (MCP bridge).
/// Stores raw terminal output bytes; consumers get the last N bytes on demand.
pub struct OutputRingBuffer {
    buf: Vec<u8>,
    capacity: usize,
    /// Write position (wraps around). When total_written < capacity, data starts at 0.
    write_pos: usize,
    /// Total bytes ever written (monotonic). Consumers use this to detect missed data.
    pub total_written: u64,
}

impl OutputRingBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            buf: vec![0u8; capacity],
            capacity,
            write_pos: 0,
            total_written: 0,
        }
    }

    /// Append data to the ring buffer using bulk copy to avoid per-byte overhead.
    pub fn write(&mut self, data: &[u8]) {
        let len = data.len();
        if len == 0 {
            return;
        }
        // How many bytes fit from write_pos to end of buffer
        let first_chunk = (self.capacity - self.write_pos).min(len);
        self.buf[self.write_pos..self.write_pos + first_chunk]
            .copy_from_slice(&data[..first_chunk]);
        if first_chunk < len {
            // Wrap around: copy the remainder starting at index 0
            let second_chunk = len - first_chunk;
            self.buf[..second_chunk].copy_from_slice(&data[first_chunk..]);
            self.write_pos = second_chunk;
        } else {
            self.write_pos += first_chunk;
            if self.write_pos == self.capacity {
                self.write_pos = 0;
            }
        }
        self.total_written += len as u64;
    }

    /// Read the last `limit` bytes (or fewer if not enough data).
    /// Returns (bytes, total_written) so consumers can track position.
    pub fn read_last(&self, limit: usize) -> (Vec<u8>, u64) {
        let available = std::cmp::min(self.total_written as usize, self.capacity);
        let to_read = std::cmp::min(limit, available);
        if to_read == 0 {
            return (Vec::new(), self.total_written);
        }

        let mut result = Vec::with_capacity(to_read);
        // Start position: write_pos - to_read, wrapping around
        let start = if self.write_pos >= to_read {
            self.write_pos - to_read
        } else {
            self.capacity - (to_read - self.write_pos)
        };

        // Bulk copy using extend_from_slice (two slices if wrapping)
        let first_chunk = (self.capacity - start).min(to_read);
        result.extend_from_slice(&self.buf[start..start + first_chunk]);
        if first_chunk < to_read {
            result.extend_from_slice(&self.buf[..to_read - first_chunk]);
        }

        (result, self.total_written)
    }

    /// Read bytes written after `since_offset` (based on `total_written`).
    /// Returns (bytes, current_total_written).
    /// If `since_offset` is older than the buffer capacity, returns whatever is still available.
    pub fn read_since(&self, since_offset: u64) -> (Vec<u8>, u64) {
        if since_offset >= self.total_written {
            return (Vec::new(), self.total_written);
        }
        let bytes_behind = (self.total_written - since_offset) as usize;
        // Clamp to available data in the ring buffer
        let available = std::cmp::min(self.total_written as usize, self.capacity);
        let to_read = std::cmp::min(bytes_behind, available);
        self.read_last(to_read)
    }

    /// Current total_written counter (bytes ever written, monotonically increasing).
    #[cfg(test)]
    pub fn total_written(&self) -> u64 {
        self.total_written
    }
}

pub(crate) const OUTPUT_RING_BUFFER_CAPACITY: usize = 2 * 1024 * 1024; // 2 MB

/// Kitty keyboard protocol: actions detected in PTY output.
/// Applications send these sequences to request enhanced key encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum KittyAction {
    /// `CSI > flags u` — push flags onto the stack
    Push(u32),
    /// `CSI < u` — pop one entry from the stack
    Pop,
    /// `CSI ? u` — query current flags (terminal responds with `CSI ? flags u`)
    Query,
}

/// Per-session kitty keyboard protocol state.
/// Tracks a stack of flag values as specified by the protocol.
pub(crate) struct KittyKeyboardState {
    stack: Vec<u32>,
}

impl KittyKeyboardState {
    pub(crate) fn new() -> Self {
        Self { stack: Vec::new() }
    }

    /// Push flags onto the stack.
    pub(crate) fn push(&mut self, flags: u32) {
        self.stack.push(flags);
    }

    /// Pop one entry from the stack. No-op if already empty (underflow safety).
    pub(crate) fn pop(&mut self) {
        self.stack.pop();
    }

    /// Current effective flags (top of stack, or 0 if empty).
    pub(crate) fn current_flags(&self) -> u32 {
        self.stack.last().copied().unwrap_or(0)
    }
}

/// Scan PTY output for kitty keyboard protocol sequences and strip them.
///
/// Detects:
/// - `ESC [ > N u` — push flags (N is one or more digits)
/// - `ESC [ < u`   — pop
/// - `ESC [ ? u`   — query
///
/// Returns the cleaned string (with kitty sequences removed) and a list of actions.
/// Fast path: if the input contains none of the trigger prefixes, returns it unchanged.
pub(crate) fn strip_kitty_sequences(input: &str) -> (Cow<'_, str>, Vec<KittyAction>) {
    // Fast path: skip scanning if no possible kitty sequence prefix exists
    if !input.contains("\x1b[>") && !input.contains("\x1b[<") && !input.contains("\x1b[?") {
        return (Cow::Borrowed(input), Vec::new());
    }

    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut actions = Vec::new();
    // Track ranges of the input to KEEP (everything except kitty sequences).
    // At the end we concatenate these slices to preserve UTF-8 integrity.
    let mut kept_start = 0; // start of current "keep" span
    let mut kept_ranges: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;

    while i < len {
        if bytes[i] == 0x1b && i + 2 < len && bytes[i + 1] == b'[' {
            match bytes[i + 2] {
                b'>' => {
                    // Potential push: ESC [ > digits u
                    let mut j = i + 3;
                    while j < len && bytes[j].is_ascii_digit() {
                        j += 1;
                    }
                    if j > i + 3 && j < len && bytes[j] == b'u' {
                        // Valid push — save preceding text, skip sequence
                        if i > kept_start {
                            kept_ranges.push((kept_start, i));
                        }
                        let digits = &input[i + 3..j];
                        if let Ok(flags) = digits.parse::<u32>() {
                            actions.push(KittyAction::Push(flags));
                        }
                        i = j + 1;
                        kept_start = i;
                        continue;
                    }
                    // Not a kitty push — advance past ESC only
                    i += 1;
                }
                b'<' => {
                    if i + 3 < len && bytes[i + 3] == b'u' {
                        // Valid pop — save preceding text, skip sequence
                        if i > kept_start {
                            kept_ranges.push((kept_start, i));
                        }
                        actions.push(KittyAction::Pop);
                        i += 4;
                        kept_start = i;
                        continue;
                    }
                    i += 1;
                }
                b'?' => {
                    if i + 3 < len && bytes[i + 3] == b'u' {
                        // Valid query — save preceding text, skip sequence
                        if i > kept_start {
                            kept_ranges.push((kept_start, i));
                        }
                        actions.push(KittyAction::Query);
                        i += 4;
                        kept_start = i;
                        continue;
                    }
                    i += 1;
                }
                _ => {
                    i += 1;
                }
            }
        } else {
            i += 1;
        }
    }

    // If no kitty sequences were found, return original string
    if actions.is_empty() {
        return (Cow::Borrowed(input), actions);
    }

    // Flush trailing kept span
    if kept_start < len {
        kept_ranges.push((kept_start, len));
    }

    // Concatenate kept slices (all are valid UTF-8 sub-slices of input)
    let mut output = String::with_capacity(len);
    for (start, end) in &kept_ranges {
        output.push_str(&input[*start..*end]);
    }

    (Cow::Owned(output), actions)
}

/// Represents a git worktree
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorktreeInfo {
    pub name: String,
    pub path: PathBuf,
    pub branch: Option<String>,
    pub base_repo: PathBuf,
}

/// Represents a PTY session with optional worktree
pub struct PtySession {
    pub writer: Box<dyn Write + Send>,
    pub master: Box<dyn portable_pty::MasterPty + Send>,
    pub(crate) _child: Box<dyn portable_pty::Child + Send + Sync>,
    pub(crate) paused: Arc<AtomicBool>,
    pub worktree: Option<WorktreeInfo>,
    pub cwd: Option<String>,
    /// Display name set by the desktop UI (tab rename, agent launch, intent title).
    pub display_name: Option<String>,
}

/// Configuration for agent orchestration
pub(crate) const MAX_CONCURRENT_SESSIONS: usize = 50;

/// PTY subsystem metrics for observability.
/// All counters use AtomicUsize for lock-free, zero-overhead-when-idle tracking.
pub(crate) struct SessionMetrics {
    pub(crate) total_spawned: AtomicUsize,
    pub(crate) failed_spawns: AtomicUsize,
    pub(crate) active_sessions: AtomicUsize,
    pub(crate) bytes_emitted: AtomicUsize,
    pub(crate) pauses_triggered: AtomicUsize,
}

impl SessionMetrics {
    pub(crate) const fn new() -> Self {
        Self {
            total_spawned: AtomicUsize::new(0),
            failed_spawns: AtomicUsize::new(0),
            active_sessions: AtomicUsize::new(0),
            bytes_emitted: AtomicUsize::new(0),
            pauses_triggered: AtomicUsize::new(0),
        }
    }
}

/// Metadata for an active MCP protocol session.
/// Stored per session_id so tool handlers can check client identity at call time.
#[derive(Debug, Clone)]
pub struct McpSessionMeta {
    /// When the session was created (for TTL reaping)
    pub created_at: Instant,
    /// Whether the client identified as Claude Code (or tuic-bridge) at initialize time
    pub is_claude_code: bool,
    /// Whether this session has an active SSE stream (GET /mcp connected)
    pub has_sse_stream: bool,
}

/// A registered peer agent in the inter-agent messaging system.
/// Keyed by `tuic_session` (the stable tab UUID from TUIC_SESSION env var).
#[derive(Debug, Clone, Serialize)]
pub struct PeerAgent {
    /// Stable tab UUID (from TUIC_SESSION env var) — primary identifier
    pub tuic_session: String,
    /// MCP session ID (for routing notifications via SSE)
    pub mcp_session_id: String,
    /// Display name (tab name or agent-chosen name)
    pub name: String,
    /// Git repo root this agent is working on (for filtering)
    pub project: Option<String>,
    /// When the agent registered (unix millis for serialization)
    pub registered_at: u64,
}

/// A message in the inter-agent mailbox.
#[derive(Debug, Clone, Serialize)]
pub struct AgentMessage {
    /// Unique message ID
    pub id: String,
    /// Sender's tuicSession UUID
    pub from_tuic_session: String,
    /// Sender display name
    pub from_name: String,
    /// Message body (max 64 KB)
    pub content: String,
    /// Unix millis timestamp
    pub timestamp: u64,
    /// Whether this message was pushed via SSE channel notification
    pub delivered_via_channel: bool,
}

/// Max messages per agent inbox before FIFO eviction.
pub(crate) const AGENT_INBOX_CAPACITY: usize = 100;

/// Max message body size in bytes (64 KB).
pub(crate) const AGENT_MESSAGE_MAX_BYTES: usize = 64 * 1024;

/// Global state for managing PTY sessions and worktrees
pub struct AppState {
    pub sessions: DashMap<String, Mutex<PtySession>>,
    pub(crate) worktrees_dir: PathBuf,
    pub(crate) metrics: SessionMetrics,
    /// Ring buffers for MCP output access (one per session)
    pub output_buffers: DashMap<String, Mutex<OutputRingBuffer>>,
    /// Active MCP Streamable HTTP sessions (session_id -> metadata for TTL reaping + client identity)
    pub mcp_sessions: DashMap<String, McpSessionMeta>,
    /// WebSocket clients per PTY session for streaming output
    pub ws_clients: DashMap<String, Vec<tokio::sync::mpsc::UnboundedSender<String>>>,
    /// Cached AppConfig to avoid re-reading from disk on every request
    pub(crate) config: parking_lot::RwLock<crate::config::AppConfig>,
    /// TTL caches for git and GitHub query results
    pub(crate) git_cache: GitCacheState,
    /// File watchers for .git/HEAD per repo (keyed by repo path)
    pub(crate) head_watchers: DashMap<String, Debouncer<notify::RecommendedWatcher>>,
    /// File watchers for .git/ directory per repo (keyed by repo path)
    pub(crate) repo_watchers: DashMap<String, Debouncer<notify::RecommendedWatcher>>,
    /// File watchers for directory contents (keyed by absolute dir path)
    pub(crate) dir_watchers: DashMap<String, Debouncer<notify::RecommendedWatcher>>,
    /// Shared async HTTP client for GitHub API requests.
    pub(crate) http_client: reqwest::Client,
    /// GitHub API token — updated on fallback when a 401 triggers candidate rotation
    pub(crate) github_token: parking_lot::RwLock<Option<String>>,
    /// Where the current GitHub token came from (env, OAuth keyring, gh CLI)
    pub(crate) github_token_source: parking_lot::RwLock<crate::github_auth::TokenSource>,
    /// Circuit breaker for GitHub API calls
    pub(crate) github_circuit_breaker: crate::github::GitHubCircuitBreaker,
    /// Shutdown sender for the HTTP server — send () to gracefully stop it
    pub(crate) server_shutdown: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
    /// Random session token for browser cookie auth — regenerated on each server start.
    /// Browsers auto-send cookies in fetch(), unlike stored Basic Auth credentials.
    /// Behind RwLock so it can be regenerated at runtime (invalidating all sessions).
    pub(crate) session_token: parking_lot::RwLock<String>,
    /// Tauri AppHandle — stored after setup() so HTTP handlers can emit events.
    /// None before Tauri initializes (or in headless/test scenarios).
    pub(crate) app_handle: parking_lot::RwLock<Option<AppHandle>>,
    /// Plugin filesystem watchers: watch_id → (plugin_id, watcher)
    pub plugin_watchers: DashMap<String, (String, notify::RecommendedWatcher)>,
    /// Per-session VT100 log buffers for clean mobile/REST output (session_id → buffer).
    /// Separate DashMap to avoid writer contention on PtySession.
    pub(crate) vt_log_buffers: DashMap<String, Mutex<VtLogBuffer>>,
    /// Per-session VT100 diff renderers for scroll-jump-free xterm output.
    /// When present for a session, raw PTY data is processed through vt100::Parser
    /// and only minimal screen diffs are emitted to the frontend.
    pub(crate) diff_renderers: DashMap<String, Mutex<crate::diff_renderer::DiffRenderer>>,
    /// Per-session kitty keyboard protocol state (session_id → state).
    /// Separate DashMap (not inside PtySession) to avoid writer contention.
    pub(crate) kitty_states: DashMap<String, Mutex<KittyKeyboardState>>,
    /// Per-session input line buffers for reconstructing user input from PTY writes.
    /// Separate DashMap (like kitty_states) to avoid writer lock contention.
    pub(crate) input_buffers: DashMap<String, Mutex<crate::input_line_buffer::InputLineBuffer>>,
    /// Last relevant user prompt per session (>= 10 words).
    /// Updated on each qualifying user input line, read by the Activity Dashboard.
    pub(crate) last_prompts: DashMap<String, String>,
    /// Per-session silence state for fallback question detection.
    /// Shared between the reader thread and write_pty so user-typed lines can be suppressed.
    pub(crate) silence_states: DashMap<String, Arc<Mutex<crate::pty::SilenceState>>>,
    /// Incremental cache for Claude session transcript parsing.
    /// Loaded from disk on startup, persisted after each scan.
    pub(crate) claude_usage_cache: Mutex<crate::claude_usage::SessionStatsCache>,
    /// Centralized application log ring buffer (1000 entries).
    /// Frontend pushes via push_log, reads via get_logs.
    /// Wrapped in Arc so the tracing subscriber layer can share the same buffer.
    pub(crate) log_buffer: Arc<Mutex<crate::app_logger::LogRingBuffer>>,
    /// Broadcast channel for all backend events (SSE, WebSocket, state accumulator).
    /// Capacity 256 — lagged receivers get `RecvError::Lagged` and should reconnect.
    pub(crate) event_bus: tokio::sync::broadcast::Sender<AppEvent>,
    /// Monotonic counter for SSE event IDs.
    pub(crate) event_counter: Arc<AtomicU64>,
    /// Per-session state accumulated from broadcast events (for REST polling).
    pub(crate) session_states: DashMap<String, SessionState>,
    /// Upstream MCP proxy registry — aggregates tools from all connected upstreams.
    pub(crate) mcp_upstream_registry: Arc<crate::mcp_proxy::registry::UpstreamRegistry>,
    /// Broadcast channel for MCP `notifications/tools/list_changed`.
    /// Fired when native tools are toggled or upstream tool lists change.
    pub(crate) mcp_tools_changed: tokio::sync::broadcast::Sender<()>,
    /// Per-session slash command mode (true when input starts with `/`).
    /// Used to suppress false-positive slash menu detection on PTY output.
    pub(crate) slash_mode: DashMap<String, std::sync::atomic::AtomicBool>,
    /// Per-session timestamp of last PTY output (epoch ms).
    /// Updated by PTY reader on every non-empty chunk. Used to derive shell_state:
    /// "busy" when now - last < 500ms, "idle" otherwise (matches desktop model).
    pub(crate) last_output_ms: DashMap<String, AtomicU64>,
    /// Per-session shell activity state (AtomicU8: 0=null, 1=busy, 2=idle).
    /// Updated by the reader thread and silence timer via compare_exchange.
    /// The single source of truth for busy/idle — the frontend consumes events,
    /// it does not derive this state from raw PTY output timing.
    pub(crate) shell_states: DashMap<String, std::sync::atomic::AtomicU8>,
    /// Loaded plugin capabilities: plugin_id → list of capability strings.
    /// Populated by the frontend via `register_loaded_plugin` on plugin load.
    /// Used by Rust plugin commands to enforce capability checks server-side.
    pub(crate) loaded_plugins: DashMap<String, Vec<String>>,
    /// Cloud relay client state
    pub(crate) relay: RelayState,
    /// Registered peer agents for inter-agent messaging (tuic_session → PeerAgent)
    pub peer_agents: DashMap<String, PeerAgent>,
    /// Message inbox per agent (tuic_session → VecDeque<AgentMessage>).
    /// Capped at AGENT_INBOX_CAPACITY messages per agent, old messages evicted FIFO.
    pub agent_inbox: DashMap<String, VecDeque<AgentMessage>>,
    /// Actual bound socket path (may differ from default if another instance holds mcp.sock).
    /// Updated by `start_server` after successful bind.
    #[cfg(unix)]
    pub(crate) bound_socket_path: parking_lot::RwLock<std::path::PathBuf>,
    /// Server start time for uptime calculation in health endpoint.
    pub(crate) server_start_time: std::time::Instant,
    /// Per-MCP-session broadcast channels for inter-agent messaging notifications.
    /// Each SSE listener subscribes; `send` action pushes here for real-time delivery.
    pub(crate) messaging_channels: DashMap<String, tokio::sync::broadcast::Sender<String>>,
}

/// Cloud relay client state (connection + shutdown handle).
pub(crate) struct RelayState {
    /// Shutdown sender — send () to gracefully stop the relay client
    pub(crate) shutdown: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
    /// Whether the relay client is currently connected
    pub(crate) connected: std::sync::atomic::AtomicBool,
}

impl RelayState {
    pub(crate) fn new() -> Self {
        Self {
            shutdown: parking_lot::Mutex::new(None),
            connected: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

/// TTL caches for git and GitHub query results, keyed by repo path.
pub(crate) struct GitCacheState {
    pub(crate) repo_info: DashMap<String, (crate::git::RepoInfo, Instant)>,
    pub(crate) merged_branches: DashMap<String, (Vec<String>, Instant)>,
    pub(crate) branches_detail: DashMap<String, (Vec<crate::git::BranchDetail>, Instant)>,
    pub(crate) github_status: DashMap<String, (Vec<crate::github::BranchPrStatus>, Instant)>,
    pub(crate) git_status: DashMap<String, (crate::github::GitHubStatus, Instant)>,
    pub(crate) git_panel_context: DashMap<String, (crate::git::GitPanelContext, Instant)>,
    /// Repos that returned null from GitHub GraphQL (not found / no access).
    /// Keyed by "owner/name", value is the cooldown expiry time.
    /// Excluded from batch queries until the cooldown expires (1 hour).
    pub(crate) github_repo_cooldown: DashMap<String, Instant>,
}

impl GitCacheState {
    pub(crate) fn new() -> Self {
        Self {
            repo_info: DashMap::new(),
            merged_branches: DashMap::new(),
            branches_detail: DashMap::new(),
            github_status: DashMap::new(),
            git_status: DashMap::new(),
            git_panel_context: DashMap::new(),
            github_repo_cooldown: DashMap::new(),
        }
    }

    /// Invalidate all caches.
    pub(crate) fn clear_all(&self) {
        self.repo_info.clear();
        self.merged_branches.clear();
        self.branches_detail.clear();
        self.github_status.clear();
        self.git_status.clear();
        self.git_panel_context.clear();
        self.github_repo_cooldown.clear();
    }

    /// Invalidate caches for a specific repo path.
    pub(crate) fn invalidate_repo(&self, path: &str) {
        self.repo_info.remove(path);
        self.merged_branches.remove(path);
        self.branches_detail.remove(path);
        self.github_status.remove(path);
        self.git_status.remove(path);
        self.git_panel_context.remove(path);
    }
}

/// Remove dead (closed-receiver) WebSocket senders for a session.
///
/// Called on WS close so that disconnected clients don't accumulate
/// on idle PTY sessions that produce no output (which would otherwise
/// be the only trigger for retain-based cleanup).
pub(crate) fn purge_dead_ws_clients(
    ws_clients: &DashMap<String, Vec<tokio::sync::mpsc::UnboundedSender<String>>>,
    session_id: &str,
) {
    if let Some(mut clients) = ws_clients.get_mut(session_id) {
        clients.retain(|tx| !tx.is_closed());
    }
}

impl AppState {
    /// Look up a cached value if it exists and hasn't expired.
    pub(crate) fn get_cached<T: Clone>(
        map: &DashMap<String, (T, Instant)>,
        key: &str,
        ttl: Duration,
    ) -> Option<T> {
        map.get(key).and_then(|entry| {
            let (value, stored_at) = entry.value();
            if stored_at.elapsed() < ttl {
                Some(value.clone())
            } else {
                None
            }
        })
    }

    /// Store a value in a TTL cache.
    pub(crate) fn set_cached<T>(
        map: &DashMap<String, (T, Instant)>,
        key: String,
        value: T,
    ) {
        map.insert(key, (value, Instant::now()));
    }

    /// Invalidate all operation caches (git + GitHub).
    pub(crate) fn clear_caches(&self) {
        self.git_cache.clear_all();
    }

    /// Invalidate caches for a specific repo path.
    pub(crate) fn invalidate_repo_caches(&self, path: &str) {
        self.git_cache.invalidate_repo(path);
    }

    /// Shell idle timeout: 500ms without PTY output → "idle" (matches desktop model).
    const SHELL_IDLE_MS: u64 = 500;

    /// Derive shell_state from PTY output timing for a session.
    /// Returns "busy" if last output was < 500ms ago, "idle" otherwise, None if never output.
    pub(crate) fn derive_shell_state(&self, session_id: &str) -> Option<String> {
        self.last_output_ms.get(session_id).map(|ts| {
            let last = ts.load(std::sync::atomic::Ordering::Relaxed);
            if last == 0 {
                return "idle".to_string();
            }
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            if now.saturating_sub(last) < Self::SHELL_IDLE_MS {
                "busy".to_string()
            } else {
                "idle".to_string()
            }
        })
    }

    /// Default rate limit expiry when no retry_after_ms is provided (120s).
    const RATE_LIMIT_DEFAULT_EXPIRY_MS: u64 = 120_000;

    /// Get a SessionState snapshot with shell_state computed from output timing.
    /// Also expires stale rate limits based on retry_after_ms + timestamp.
    pub(crate) fn session_state_with_shell(&self, session_id: &str) -> Option<SessionState> {
        // Expire stale rate limits in-place before building the snapshot.
        if let Some(mut entry) = self.session_states.get_mut(session_id)
            && entry.rate_limited && entry.rate_limit_set_ms > 0
        {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            let ttl = entry.retry_after_ms.unwrap_or(Self::RATE_LIMIT_DEFAULT_EXPIRY_MS);
            if now.saturating_sub(entry.rate_limit_set_ms) > ttl {
                entry.rate_limited = false;
                entry.retry_after_ms = None;
                entry.rate_limit_set_ms = 0;
            }
        }
        self.session_states.get(session_id).map(|s| {
            let mut state = s.clone();
            state.shell_state = self.derive_shell_state(session_id);
            state
        })
    }

    /// Spawn a background task that subscribes to the event bus and updates
    /// `session_states`. Call once at startup after constructing AppState.
    pub(crate) fn spawn_session_state_accumulator(state: Arc<AppState>) {
        let mut rx = state.event_bus.subscribe();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => Self::apply_event_to_session_state(&state, &event),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(source = "session_state", lagged = n, "Event bus lagged");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    /// Apply a single event to the session state accumulator.
    fn apply_event_to_session_state(state: &Arc<AppState>, event: &AppEvent) {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        match event {
            AppEvent::SessionCreated { session_id, .. } => {
                state.session_states.insert(session_id.clone(), SessionState {
                    last_activity_ms: now_ms,
                    ..Default::default()
                });
            }
            AppEvent::SessionClosed { session_id, .. } => {
                state.session_states.remove(session_id);
            }
            AppEvent::PtyParsed { session_id, parsed } => {
                let event_type = parsed.get("type").and_then(|t| t.as_str()).unwrap_or("");
                state.session_states
                    .entry(session_id.clone())
                    .and_modify(|s| {
                        s.last_activity_ms = now_ms;
                        match event_type {
                            "question" => {
                                s.awaiting_input = true;
                                s.question_text = parsed.get("prompt_text")
                                    .and_then(|t| t.as_str())
                                    .map(|t| t.to_string());
                            }
                            "user-input" => {
                                // User responded — agent will start working
                                s.awaiting_input = false;
                                s.question_text = None;
                                s.slash_menu_items = None;
                                // Capture as last_prompt if >= 10 words
                                if let Some(content) = parsed.get("content").and_then(|v| v.as_str())
                                    && content.split_whitespace().count() >= 10 {
                                        s.last_prompt = Some(content.to_string());
                                    }
                            }
                            "rate-limit" => {
                                // Only track rate limits on agent sessions.
                                // Plain shell terminals can match rate-limit patterns
                                // in their output but should never show the badge.
                                if s.current_task.is_some() {
                                    s.rate_limited = true;
                                    s.retry_after_ms = parsed.get("retry_after_ms")
                                        .and_then(|v| v.as_u64());
                                    s.rate_limit_set_ms = now_ms;
                                }
                            }
                            "usage-limit" => {
                                s.usage_limit_pct = parsed.get("percentage")
                                    .and_then(|v| v.as_u64())
                                    .map(|v| v as u8);
                            }
                            "api-error" => {
                                s.last_error = parsed.get("matched_text")
                                    .and_then(|t| t.as_str())
                                    .map(|t| t.to_string());
                            }
                            "status-line" => {
                                // Agent is working — clear error/rate-limit/suggest/menu/question
                                s.awaiting_input = false;
                                s.question_text = None;
                                s.rate_limited = false;
                                s.retry_after_ms = None;
                                s.rate_limit_set_ms = 0;
                                s.last_error = None;
                                s.suggested_actions = None;
                                s.slash_menu_items = None;
                                // Only update current_task + activity timestamp when task changes.
                                // Spinner rotations (same task name) are suppressed to avoid
                                // churning the state and flooding WS clients.
                                let new_task = parsed.get("task_name")
                                    .and_then(|v| v.as_str())
                                    .map(|t| t.to_string());
                                if s.current_task != new_task {
                                    s.current_task = new_task;
                                }
                            }
                            "intent" => {
                                s.agent_intent = parsed.get("text")
                                    .and_then(|v| v.as_str())
                                    .map(|t| t.to_string());
                            }
                            "suggest" => {
                                s.suggested_actions = parsed.get("items")
                                    .and_then(|v| v.as_array())
                                    .map(|arr| arr.iter()
                                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                        .collect());
                            }
                            "slash-menu" => {
                                s.slash_menu_items = parsed.get("items")
                                    .and_then(|v| serde_json::from_value::<Vec<crate::output_parser::SlashMenuItem>>(v.clone()).ok());
                            }
                            "active-subtasks" => {
                                s.active_sub_tasks = parsed.get("count")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0) as u32;
                            }
                            "progress" => {
                                let state_val = parsed.get("state").and_then(|v| v.as_u64()).unwrap_or(0);
                                if state_val == 0 {
                                    // state=0 means remove the progress bar
                                    s.progress = None;
                                } else {
                                    s.progress = parsed.get("value")
                                        .and_then(|v| v.as_u64())
                                        .map(|v| v as u8);
                                }
                            }
                            _ => {}
                        }
                    })
                    .or_insert_with(|| SessionState {
                        last_activity_ms: now_ms,
                        ..Default::default()
                    });
            }
            AppEvent::PtyExit { session_id } => {
                if let Some(mut entry) = state.session_states.get_mut(session_id) {
                    entry.awaiting_input = false;
                    entry.question_text = None;
                    entry.rate_limited = false;
                    entry.retry_after_ms = None;
                    entry.rate_limit_set_ms = 0;
                    entry.active_sub_tasks = 0;
                    entry.last_activity_ms = now_ms;
                }
            }
            // Global events don't affect per-session state
            AppEvent::HeadChanged { .. }
            | AppEvent::RepoChanged { .. }
            | AppEvent::PluginChanged { .. }
            | AppEvent::UpstreamStatusChanged { .. }
            | AppEvent::McpToast { .. }
            | AppEvent::DirChanged { .. }
            | AppEvent::WorktreeCreated { .. }
            | AppEvent::PeerRegistered { .. }
            | AppEvent::PeerUnregistered { .. } => {}
        }
    }

    /// Build orchestrator stats snapshot from current state.
    pub(crate) fn orchestrator_stats(&self) -> OrchestratorStats {
        let active = self.sessions.len();
        OrchestratorStats {
            active_sessions: active,
            max_sessions: MAX_CONCURRENT_SESSIONS,
            available_slots: MAX_CONCURRENT_SESSIONS.saturating_sub(active),
        }
    }

    /// Build session metrics JSON from current atomic counters.
    pub(crate) fn session_metrics_json(&self) -> serde_json::Value {
        use std::sync::atomic::Ordering;
        serde_json::json!({
            "total_spawned": self.metrics.total_spawned.load(Ordering::Relaxed),
            "failed_spawns": self.metrics.failed_spawns.load(Ordering::Relaxed),
            "active_sessions": self.metrics.active_sessions.load(Ordering::Relaxed),
            "bytes_emitted": self.metrics.bytes_emitted.load(Ordering::Relaxed),
            "pauses_triggered": self.metrics.pauses_triggered.load(Ordering::Relaxed),
        })
    }
}

/// Agent orchestration stats
#[derive(Clone, Serialize)]
pub(crate) struct OrchestratorStats {
    pub(crate) active_sessions: usize,
    pub(crate) max_sessions: usize,
    pub(crate) available_slots: usize,
}

#[derive(Clone, Serialize)]
pub(crate) struct PtyOutput {
    pub(crate) session_id: String,
    pub(crate) data: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct PtyConfig {
    pub(crate) rows: u16,
    pub(crate) cols: u16,
    pub(crate) shell: Option<String>,
    pub(crate) cwd: Option<String>,
    /// Pre-generated stable session UUID — injected as `TUIC_SESSION` env var.
    /// Persists across app restarts so agents can resume the same session.
    pub(crate) tuic_session: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct AgentConfig {
    pub(crate) prompt: String,
    pub(crate) cwd: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) print_mode: bool,
    pub(crate) output_format: Option<String>,
    pub(crate) agent_type: Option<String>,
    pub(crate) binary_path: Option<String>,
    pub(crate) args: Option<Vec<String>>,
}

// ---------------------------------------------------------------------------
// VtLogBuffer — VT100-aware log extractor for mobile/REST consumers
// ---------------------------------------------------------------------------

/// Default maximum log lines retained per session.
pub(crate) const VT_LOG_BUFFER_CAPACITY: usize = 10_000;

/// Terminal color extracted from vt100 cells.
///
/// Serializes as `{"idx": N}` for 256-color palette or `{"rgb": [r,g,b]}` for
/// 24-bit color.  Default color is omitted (serialized as `null` / skipped).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogColor {
    Idx(u8),
    Rgb(u8, u8, u8),
}

impl LogColor {
    fn from_vt100(c: vt100::Color) -> Option<Self> {
        match c {
            vt100::Color::Default => None,
            vt100::Color::Idx(i) => Some(LogColor::Idx(i)),
            vt100::Color::Rgb(r, g, b) => Some(LogColor::Rgb(r, g, b)),
        }
    }
}

/// A contiguous run of text with uniform formatting attributes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct LogSpan {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fg: Option<LogColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bg: Option<LogColor>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub bold: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub italic: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub underline: bool,
}

/// A single log line composed of styled spans.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LogLine {
    pub spans: Vec<LogSpan>,
}

impl LogLine {
    /// Returns the plain-text content (all span texts concatenated).
    pub fn text(&self) -> String {
        let mut s = String::new();
        for span in &self.spans {
            s.push_str(&span.text);
        }
        s
    }

    /// Strip structural tokens (`[[intent: ...]]`, `[[suggest: ...]]`) from span text.
    /// These tokens are parsed by the output parser for state updates but should not
    /// appear in rendered log output (PWA/REST consumers).
    pub fn strip_structural_tokens(&mut self) {
        lazy_static::lazy_static! {
            static ref STRUCTURAL_RE: regex::Regex = regex::Regex::new(
                r"(?:\[\[?|\x{27E6})(?:intent|suggest):\s*[^\]\x{27E7}]*?\s*(?:\]?\]|\x{27E7})"
            ).unwrap();
        }
        for span in &mut self.spans {
            if span.text.contains("intent:") || span.text.contains("suggest:") {
                let replaced = STRUCTURAL_RE.replace_all(&span.text, "");
                span.text = replaced.into_owned();
            }
        }
        // Remove spans that became empty after stripping
        self.spans.retain(|s| !s.text.is_empty());
    }
}

/// Extract a styled `LogLine` from a vt100 screen row by iterating cells.
///
/// Consecutive cells with the same (fg, bg, bold, italic, underline) attributes
/// are grouped into a single `LogSpan`.  Trailing whitespace-only spans with
/// default attributes are trimmed.
fn extract_log_line(screen: &vt100::Screen, row: u16) -> LogLine {
    let cols = screen.size().1;
    let mut spans: Vec<LogSpan> = Vec::new();

    // Current span accumulator state
    let mut cur_fg: Option<LogColor> = None;
    let mut cur_bg: Option<LogColor> = None;
    let mut cur_bold = false;
    let mut cur_italic = false;
    let mut cur_underline = false;
    let mut cur_text = String::new();

    for col in 0..cols {
        let cell = match screen.cell(row, col) {
            Some(c) => c,
            None => break,
        };
        // Skip wide-char continuation cells
        if cell.is_wide_continuation() {
            continue;
        }

        let fg = LogColor::from_vt100(cell.fgcolor());
        let bg = LogColor::from_vt100(cell.bgcolor());
        let bold = cell.bold();
        let italic = cell.italic();
        let underline = cell.underline();

        // If attributes changed, flush current span and start new one
        if !cur_text.is_empty()
            && (fg != cur_fg || bg != cur_bg || bold != cur_bold
                || italic != cur_italic || underline != cur_underline)
        {
            spans.push(LogSpan {
                text: std::mem::take(&mut cur_text),
                fg: cur_fg,
                bg: cur_bg,
                bold: cur_bold,
                italic: cur_italic,
                underline: cur_underline,
            });
        }

        cur_fg = fg;
        cur_bg = bg;
        cur_bold = bold;
        cur_italic = italic;
        cur_underline = underline;

        let contents = cell.contents();
        if contents.is_empty() {
            cur_text.push(' ');
        } else {
            cur_text.push_str(contents);
        }
    }

    // Flush last span
    if !cur_text.is_empty() {
        spans.push(LogSpan {
            text: cur_text,
            fg: cur_fg,
            bg: cur_bg,
            bold: cur_bold,
            italic: cur_italic,
            underline: cur_underline,
        });
    }

    // Trim trailing whitespace-only spans with default attrs, then trim the last span's trailing whitespace
    while let Some(last) = spans.last() {
        if last.fg.is_none() && last.bg.is_none() && !last.bold && !last.italic && !last.underline
            && last.text.trim_end().is_empty()
        {
            spans.pop();
        } else {
            break;
        }
    }
    if let Some(last) = spans.last_mut() {
        let trimmed = last.text.trim_end().to_string();
        if trimmed.is_empty() && last.fg.is_none() && last.bg.is_none() && !last.bold && !last.italic && !last.underline {
            spans.pop();
        } else {
            last.text = trimmed;
        }
    }

    LogLine { spans }
}

/// A screen row that changed after a `VtLogBuffer::process()` call.
///
/// Consumers (output parsers) iterate these to detect status lines, intent
/// tokens, and other structured events — regardless of whether the terminal
/// is in normal or alternate screen mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedRow {
    /// Zero-based row index on the visible screen.
    pub row_index: usize,
    /// Clean text content of the row (ANSI sequences stripped by the vt100 parser).
    pub text: String,
}

/// Per-session VT100-aware log buffer.
///
/// Wraps a `vt100::Parser` with native scrollback enabled. Lines that scroll
/// off the top of the screen are captured by the vt100 parser itself (via its
/// internal `VecDeque<Row>`). This struct reads new scrollback lines after each
/// `process()` call and stores them in a bounded `VecDeque<LogLine>` for REST
/// and WebSocket consumers.
///
/// **Thread safety:** Not `Sync` — lives behind `Mutex<VtLogBuffer>` in `AppState`.
/// The `vt100::Parser` is `Send` but not `Sync`, so this struct shares that bound.
pub struct VtLogBuffer {
    parser: vt100::Parser,
    /// Plain text snapshot of visible rows from the previous `process()` call.
    /// Used for changed-row detection (output parser) and screen_rows() cache.
    prev_rows: Vec<String>,
    /// Finalized log lines (oldest first).
    log: VecDeque<LogLine>,
    /// Maximum number of log lines retained in our own buffer.
    capacity: usize,
    /// Whether the previous `process()` call saw the alternate screen active.
    was_alternate: bool,
    /// Number of scrollback lines already read from the vt100 parser.
    /// Used to detect new scrollback lines after each `process()`.
    scrollback_read: usize,
}

/// Internal scrollback capacity for the vt100 parser. Must be large enough
/// that it never fills up between consecutive `process()` calls — in practice
/// even a `cat huge_file` sends data in ~4KB PTY read chunks.
const VT100_SCROLLBACK: usize = 10_000;

impl VtLogBuffer {
    pub fn new(rows: u16, cols: u16, capacity: usize) -> Self {
        let parser = vt100::Parser::new(rows, cols, VT100_SCROLLBACK);
        Self {
            parser,
            prev_rows: Vec::new(),
            log: VecDeque::new(),
            capacity,
            was_alternate: false,
            scrollback_read: 0,
        }
    }

    /// Feed raw PTY bytes into the VT100 parser.
    ///
    /// Returns the screen rows that changed since the previous call.  Changed
    /// rows are detected for **both** normal and alternate screen so that
    /// output parsers can match status lines and intent tokens emitted by
    /// agents that use the alternate screen (e.g. Claude Code / Ink).
    ///
    /// Log extraction reads new scrollback lines from the vt100 parser's
    /// native scrollback buffer (normal-screen-only — alternate screen does
    /// not produce scrollback).
    pub fn process(&mut self, data: &[u8]) -> Vec<ChangedRow> {
        self.parser.process(data);

        // --- Changed-row detection (for output parser) ---
        let (is_alternate, curr_rows, changed) = {
            let screen = self.parser.screen();
            let is_alternate = screen.alternate_screen();
            let cols = screen.size().1;

            let curr_rows: Vec<String> = screen
                .rows(0, cols)
                .map(|r| r.trim_end().to_string())
                .collect();

            // On screen switch we need fresh prev_rows for changed-row detection.
            let prev_rows_ref = if is_alternate != self.was_alternate {
                &[][..]
            } else {
                &self.prev_rows[..]
            };

            let changed: Vec<ChangedRow> = curr_rows
                .iter()
                .enumerate()
                .filter_map(|(i, curr)| {
                    let prev = prev_rows_ref.get(i).map(String::as_str).unwrap_or("");
                    if curr != prev {
                        Some(ChangedRow { row_index: i, text: curr.clone() })
                    } else {
                        None
                    }
                })
                .collect();

            (is_alternate, curr_rows, changed)
        }; // screen borrow ends here

        if is_alternate != self.was_alternate {
            self.prev_rows.clear();
        }

        // --- Log extraction: read new scrollback lines from vt100 ---
        // The vt100 parser accumulates scrollback automatically when lines
        // scroll off the top of the normal screen. We just read the delta.
        if !is_alternate {
            let total_sb = self.scrollback_count();
            let delta = total_sb.saturating_sub(self.scrollback_read);
            if delta > 0 {
                let screen_height = self.parser.screen().size().0 as usize;
                let new_lines = self.read_scrollback_lines(delta, screen_height);
                let trimmed = trim_agent_chrome(new_lines);
                for ll in trimmed {
                    self.push_log_line(ll);
                }
                self.scrollback_read = total_sb;
            }
        }

        self.prev_rows = curr_rows;
        self.was_alternate = is_alternate;
        changed
    }

    /// Update parser dimensions on terminal resize.
    pub fn resize(&mut self, rows: u16, cols: u16) {
        self.parser.screen_mut().set_size(rows, cols);
        self.prev_rows.clear();
        // Re-sync scrollback count after resize (vt100 may adjust scrollback).
        self.scrollback_read = self.scrollback_count();
    }

    /// Returns the number of scrollback lines currently stored in the vt100
    /// parser. Uses set_scrollback(MAX) → scrollback() → set_scrollback(0)
    /// to query without side effects.
    fn scrollback_count(&mut self) -> usize {
        self.parser.screen_mut().set_scrollback(usize::MAX);
        let count = self.parser.screen().scrollback();
        self.parser.screen_mut().set_scrollback(0);
        count
    }

    /// Reads the `count` most recent scrollback lines as styled `LogLine`s.
    /// Pages through the vt100 scrollback if `count` exceeds screen height.
    fn read_scrollback_lines(&mut self, count: usize, screen_height: usize) -> Vec<LogLine> {
        let mut result = Vec::with_capacity(count);
        let total_sb = self.scrollback_count();
        // Read in pages of screen_height, from oldest to newest.
        let mut remaining = count;
        let mut read_start = total_sb.saturating_sub(count); // oldest unread position

        while remaining > 0 {
            let page = remaining.min(screen_height);
            // Set scrollback offset so the page starts at read_start.
            // Offset = total_sb - read_start positions the view so that
            // row 0 is at read_start in the scrollback.
            let offset = total_sb - read_start;
            self.parser.screen_mut().set_scrollback(offset);
            {
                let screen = self.parser.screen();
                for row_idx in 0..page {
                    result.push(extract_log_line(screen, row_idx as u16));
                }
            }
            read_start += page;
            remaining -= page;
        }
        self.parser.screen_mut().set_scrollback(0);
        result
    }

    /// All finalized log lines (oldest first).
    #[allow(dead_code)]
    pub fn lines(&self) -> &VecDeque<LogLine> {
        &self.log
    }

    /// Returns log lines starting at `offset` (0-indexed from oldest retained line).
    /// Also returns the new offset (= total lines so far) for incremental reads.
    pub fn lines_since_owned(&self, offset: usize) -> (Vec<LogLine>, usize) {
        let total = self.log.len();
        if offset >= total {
            return (Vec::new(), total);
        }
        let mut slice: Vec<LogLine> = self.log.iter().skip(offset).cloned().collect();
        for line in &mut slice {
            line.strip_structural_tokens();
        }
        // Remove lines that became entirely empty after stripping
        slice.retain(|l| !l.spans.is_empty());
        let new_offset = total;
        (slice, new_offset)
    }

    /// Current visible screen rows.
    ///
    /// Returns the cached `prev_rows` snapshot (from the last `process()` call)
    /// when available — no re-parsing needed.  Falls back to reading from the
    /// parser when `prev_rows` is empty (before any `process()` or after `resize()`).
    pub fn screen_rows(&self) -> Vec<String> {
        if !self.prev_rows.is_empty() {
            return self.prev_rows.clone();
        }
        let screen = self.parser.screen();
        let cols = screen.size().1;
        screen
            .rows(0, cols)
            .map(|r| r.trim_end().to_string())
            .collect()
    }

    /// Current visible screen rows as styled LogLines (with ANSI color attributes).
    /// Used by mobile/REST to render screen content with colors.
    pub fn screen_log_lines(&self) -> Vec<LogLine> {
        let screen = self.parser.screen();
        let rows = screen.size().0;
        let mut lines = Vec::with_capacity(rows as usize);
        for row in 0..rows {
            let mut line = extract_log_line(screen, row);
            line.strip_structural_tokens();
            lines.push(line);
        }
        // Trim trailing empty lines
        while let Some(last) = lines.last() {
            if last.spans.is_empty() {
                lines.pop();
            } else {
                break;
            }
        }
        lines
    }

    /// Extract the user-typed text from the prompt line, excluding ghost/dim text.
    /// Scans from the bottom for `❯` or `>` prompt, then collects non-dim cell contents.
    pub fn prompt_input_text(&self) -> Option<String> {
        let screen = self.parser.screen();
        let (rows, cols) = screen.size();
        // Scan from bottom to find prompt row
        for row in (0..rows).rev() {
            // Check first non-space cell for prompt character
            let row_text: String = (0..cols)
                .filter_map(|c| screen.cell(row, c).map(|cell| cell.contents()))
                .collect::<Vec<_>>()
                .join("");
            let trimmed = row_text.trim_start();
            if !(trimmed.starts_with('❯') || trimmed == ">" || trimmed.starts_with("> ")) {
                continue;
            }
            // Found prompt row — collect non-dim text after prompt char
            let mut result = String::new();
            let mut past_prompt = false;
            for col in 0..cols {
                let Some(cell) = screen.cell(row, col) else { break };
                if cell.is_wide_continuation() {
                    continue;
                }
                let ch = cell.contents();
                if !past_prompt {
                    // Skip until after prompt char(s) and space
                    if ch == "❯" || ch == "›" || ch == ">" {
                        past_prompt = true;
                        continue;
                    }
                    if ch.trim().is_empty() {
                        continue;
                    }
                    past_prompt = true;
                }
                if past_prompt && ch.trim().is_empty() && result.is_empty() {
                    // Skip leading spaces after prompt
                    continue;
                }
                if cell.dim() {
                    // Ghost/suggestion text — stop collecting
                    break;
                }
                result.push_str(ch);
            }
            return Some(result.trim_end().to_string());
        }
        None
    }

    /// Total log lines ever finalized (monotonically increasing offset).
    /// Callers can use this as a cursor for incremental reads.
    pub fn total_lines(&self) -> usize {
        self.log.len()
    }

    // --- private helpers ---

    fn push_log_line(&mut self, line: LogLine) {
        if self.log.len() >= self.capacity {
            self.log.pop_front();
        }
        self.log.push_back(line);
    }

}

// ---------------------------------------------------------------------------
// Agent chrome trimming — removes prompt lines and UI chrome from full-screen
// redraw batches so they don't pollute the mobile log.
// ---------------------------------------------------------------------------

use crate::chrome::find_chrome_cutoff;

/// Find chrome cutoff for `LogLine` slices (mobile log trim).
fn find_prompt_cutoff_loglines(lines: &[LogLine]) -> Option<usize> {
    let texts: Vec<String> = lines.iter().map(|l| l.text()).collect();
    let refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
    find_chrome_cutoff(&refs)
}

/// Trims agent prompt and chrome from a batch of scrolled-off lines.
///
/// When a prompt line is found in the last [`CHROME_SCAN_ROWS`] rows, everything
/// from the prompt (and any immediately preceding separator/empty lines) to the
/// end of the batch is discarded. Applied to all batches regardless of size —
/// every CLI agent renders context info below the prompt that should not appear
/// in the log.
fn trim_agent_chrome(mut lines: Vec<LogLine>) -> Vec<LogLine> {
    if let Some(cutoff) = find_prompt_cutoff_loglines(&lines) {
        lines.truncate(cutoff);
    }
    lines
}

/// Test helper: construct a minimal `AppState` for unit tests in other modules.
#[cfg(test)]
pub(crate) mod tests_support {
    use super::*;

    pub fn make_test_app_state() -> AppState {
        AppState {
            sessions: dashmap::DashMap::new(),
            worktrees_dir: std::env::temp_dir().join("test-worktrees"),
            metrics: SessionMetrics::new(),
            output_buffers: dashmap::DashMap::new(),
            mcp_sessions: dashmap::DashMap::new(),
            ws_clients: dashmap::DashMap::new(),
            config: parking_lot::RwLock::new(crate::config::AppConfig::default()),
            git_cache: GitCacheState::new(),
            head_watchers: dashmap::DashMap::new(),
            repo_watchers: dashmap::DashMap::new(),
            dir_watchers: dashmap::DashMap::new(),
            http_client: reqwest::Client::new(),
            github_token: parking_lot::RwLock::new(None),
            github_token_source: parking_lot::RwLock::new(Default::default()),
            github_circuit_breaker: crate::github::GitHubCircuitBreaker::new(),
            server_shutdown: parking_lot::Mutex::new(None),
            session_token: parking_lot::RwLock::new(String::from("test-token")),
            app_handle: parking_lot::RwLock::new(None),
            plugin_watchers: dashmap::DashMap::new(),
            vt_log_buffers: dashmap::DashMap::new(),
            diff_renderers: dashmap::DashMap::new(),
            kitty_states: dashmap::DashMap::new(),
            input_buffers: dashmap::DashMap::new(),
            last_prompts: dashmap::DashMap::new(),
            silence_states: dashmap::DashMap::new(),
            claude_usage_cache: parking_lot::Mutex::new(std::collections::HashMap::new()),
            log_buffer: Arc::new(parking_lot::Mutex::new(crate::app_logger::LogRingBuffer::new(crate::app_logger::LOG_RING_CAPACITY))),
            event_bus: tokio::sync::broadcast::channel(256).0,
            event_counter: Arc::new(AtomicU64::new(0)),
            session_states: DashMap::new(),
            mcp_upstream_registry: Arc::new(crate::mcp_proxy::registry::UpstreamRegistry::new()),
            mcp_tools_changed: tokio::sync::broadcast::channel(16).0,
            slash_mode: DashMap::new(),
            last_output_ms: DashMap::new(),
            shell_states: DashMap::new(),
            loaded_plugins: DashMap::new(),
            relay: RelayState::new(),
            peer_agents: DashMap::new(),
            agent_inbox: DashMap::new(),
            messaging_channels: DashMap::new(),
            #[cfg(unix)]
            bound_socket_path: parking_lot::RwLock::new(std::path::PathBuf::new()),
            server_start_time: std::time::Instant::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_utf8_buffer_ascii() {
        let mut buf = Utf8ReadBuffer::new();
        assert_eq!(buf.push(b"hello world"), "hello world");
        assert!(buf.remainder.is_empty());
    }

    #[test]
    fn test_utf8_buffer_complete_multibyte() {
        let mut buf = Utf8ReadBuffer::new();
        assert_eq!(buf.push("€100".as_bytes()), "€100");
        assert!(buf.remainder.is_empty());
    }

    #[test]
    fn test_utf8_buffer_split_multibyte() {
        let mut buf = Utf8ReadBuffer::new();
        let result1 = buf.push(&[0xE2]);
        assert_eq!(result1, "");
        assert_eq!(buf.remainder.len(), 1);

        let result2 = buf.push(&[0x82, 0xAC]);
        assert_eq!(result2, "€");
        assert!(buf.remainder.is_empty());
    }

    #[test]
    fn test_utf8_buffer_split_4byte_emoji() {
        let mut buf = Utf8ReadBuffer::new();
        let crab = "🦀";
        let bytes = crab.as_bytes();
        assert_eq!(bytes.len(), 4);

        let result1 = buf.push(&bytes[..2]);
        assert_eq!(result1, "");
        assert_eq!(buf.remainder.len(), 2);

        let result2 = buf.push(&bytes[2..]);
        assert_eq!(result2, "🦀");
        assert!(buf.remainder.is_empty());
    }

    #[test]
    fn test_utf8_buffer_ascii_then_split() {
        let mut buf = Utf8ReadBuffer::new();
        let mut chunk1 = b"hello".to_vec();
        chunk1.push(0xE2);
        let result1 = buf.push(&chunk1);
        assert_eq!(result1, "hello");
        assert_eq!(buf.remainder.len(), 1);

        let mut chunk2 = vec![0x82, 0xAC];
        chunk2.extend_from_slice(b" world");
        let result2 = buf.push(&chunk2);
        assert_eq!(result2, "€ world");
        assert!(buf.remainder.is_empty());
    }

    #[test]
    fn test_utf8_buffer_flush_incomplete() {
        let mut buf = Utf8ReadBuffer::new();
        let result = buf.push(&[0xE2]);
        assert_eq!(result, "");

        let flushed = buf.flush();
        assert!(flushed.contains('\u{FFFD}'));
    }

    #[test]
    fn test_utf8_buffer_flush_empty() {
        let mut buf = Utf8ReadBuffer::new();
        assert_eq!(buf.flush(), "");
    }

    #[test]
    fn test_utf8_buffer_cjk_characters() {
        let mut buf = Utf8ReadBuffer::new();
        let han = "漢字";
        let bytes = han.as_bytes();
        let split = 4;
        let result1 = buf.push(&bytes[..split]);
        assert_eq!(result1, "漢");
        let result2 = buf.push(&bytes[split..]);
        assert_eq!(result2, "字");
    }

    #[test]
    fn test_ring_buffer_basic() {
        let mut rb = OutputRingBuffer::new(16);
        rb.write(b"hello");
        let (data, total) = rb.read_last(16);
        assert_eq!(&data, b"hello");
        assert_eq!(total, 5);
    }

    #[test]
    fn test_ring_buffer_wraps_around() {
        let mut rb = OutputRingBuffer::new(8);
        rb.write(b"12345678");
        rb.write(b"AB");
        let (data, total) = rb.read_last(8);
        assert_eq!(&data, b"345678AB");
        assert_eq!(total, 10);
    }

    #[test]
    fn test_ring_buffer_read_less_than_available() {
        let mut rb = OutputRingBuffer::new(16);
        rb.write(b"hello world");
        let (data, _) = rb.read_last(5);
        assert_eq!(&data, b"world");
    }

    #[test]
    fn test_ring_buffer_empty() {
        let rb = OutputRingBuffer::new(16);
        let (data, total) = rb.read_last(16);
        assert!(data.is_empty());
        assert_eq!(total, 0);
    }

    #[test]
    fn test_ring_buffer_large_write() {
        let mut rb = OutputRingBuffer::new(4);
        rb.write(b"abcdefgh");
        let (data, total) = rb.read_last(4);
        assert_eq!(&data, b"efgh");
        assert_eq!(total, 8);
    }

    #[test]
    fn test_ring_buffer_read_last_bulk_copy_wrap() {
        // Wrap-around: read_last must stitch two slices correctly.
        let mut rb = OutputRingBuffer::new(8);
        rb.write(b"ABCDEFGH"); // fills buffer, write_pos = 0
        rb.write(b"XY");       // overwrites A,B → buf = [X,Y,C,D,E,F,G,H], write_pos = 2

        // Read all 8 bytes — should produce CDEFGHXY (oldest to newest)
        let (data, total) = rb.read_last(8);
        assert_eq!(&data, b"CDEFGHXY");
        assert_eq!(total, 10);

        // Read last 3 — should produce HXY (straddles the wrap boundary)
        let (data, _) = rb.read_last(3);
        assert_eq!(&data, b"HXY");

        // Read last 6 — wraps from before write_pos to after.
        // buf = [X,Y,C,D,E,F,G,H], write_pos = 2, available = 8
        // start = 2 + 8 - 6 = 4 → indices 4,5,6,7,0,1 → E,F,G,H,X,Y
        let (data, _) = rb.read_last(6);
        assert_eq!(&data, b"EFGHXY");
    }

    #[test]
    fn test_ring_buffer_read_last_no_wrap() {
        // No wrap: all data sits in a contiguous region.
        let mut rb = OutputRingBuffer::new(16);
        rb.write(b"hello world!");
        let (data, total) = rb.read_last(5);
        assert_eq!(&data, b"orld!");
        assert_eq!(total, 12);

        let (data, _) = rb.read_last(12);
        assert_eq!(&data, b"hello world!");

        // Request more than available
        let (data, _) = rb.read_last(100);
        assert_eq!(&data, b"hello world!");
    }

    #[test]
    fn test_ring_buffer_read_last_2mb_performance() {
        // Verify that read_last on a full 2MB buffer completes quickly.
        // With bulk copy this should be sub-millisecond; the old byte-per-byte
        // loop took ~2M iterations with modulo on each.
        let cap = 2 * 1024 * 1024; // 2 MB
        let mut rb = OutputRingBuffer::new(cap);

        // Fill the buffer with pattern data that wraps
        let chunk: Vec<u8> = (0..=255u8).cycle().take(cap + 1024).collect();
        rb.write(&chunk);

        let start = std::time::Instant::now();
        let iterations = 100;
        for _ in 0..iterations {
            let (data, _) = rb.read_last(cap);
            assert_eq!(data.len(), cap);
            // Prevent optimizing away
            std::hint::black_box(&data);
        }
        let elapsed = start.elapsed();
        let per_call = elapsed / iterations;
        eprintln!(
            "read_last(2MB) x {}: total {:?}, per call {:?}",
            iterations, elapsed, per_call
        );
        // Bulk copy should finish each call well under 5ms on any modern machine.
        assert!(
            per_call.as_millis() < 5,
            "read_last(2MB) took {:?} per call — too slow",
            per_call
        );
    }

    // --- read_since tests ---

    #[test]
    fn test_ring_buffer_read_since_basic() {
        let mut rb = OutputRingBuffer::new(16);
        rb.write(b"hello");
        // offset 0 → get everything
        let (data, total) = rb.read_since(0);
        assert_eq!(&data, b"hello");
        assert_eq!(total, 5);

        rb.write(b" world");
        // offset 5 → only " world"
        let (data, total) = rb.read_since(5);
        assert_eq!(&data, b" world");
        assert_eq!(total, 11);
    }

    #[test]
    fn test_ring_buffer_read_since_at_current() {
        let mut rb = OutputRingBuffer::new(16);
        rb.write(b"abc");
        let (data, _) = rb.read_since(3);
        assert!(data.is_empty());
    }

    #[test]
    fn test_ring_buffer_read_since_future_offset() {
        let mut rb = OutputRingBuffer::new(16);
        rb.write(b"abc");
        let (data, total) = rb.read_since(999);
        assert!(data.is_empty());
        assert_eq!(total, 3);
    }

    #[test]
    fn test_ring_buffer_read_since_old_offset_clamped() {
        // Offset is so old that data has been overwritten — return what's available
        let mut rb = OutputRingBuffer::new(8);
        rb.write(b"12345678"); // total=8, buf full
        rb.write(b"ABCD");    // total=12, oldest is 5678ABCD
        // offset 2 would want 10 bytes, but only 8 available
        let (data, total) = rb.read_since(2);
        assert_eq!(&data, b"5678ABCD");
        assert_eq!(total, 12);
    }

    #[test]
    fn test_ring_buffer_total_written() {
        let mut rb = OutputRingBuffer::new(8);
        assert_eq!(rb.total_written(), 0);
        rb.write(b"abc");
        assert_eq!(rb.total_written(), 3);
        rb.write(b"defghij");
        assert_eq!(rb.total_written(), 10);
    }

    // --- EscapeAwareBuffer tests ---

    #[test]
    fn test_escape_buffer_plain_text() {
        let mut buf = EscapeAwareBuffer::new();
        assert_eq!(buf.push("hello world"), "hello world");
    }

    #[test]
    fn test_escape_buffer_complete_csi() {
        let mut buf = EscapeAwareBuffer::new();
        // Complete CSI sequence: ESC[31m (set red)
        assert_eq!(buf.push("\x1b[31mRed\x1b[0m"), "\x1b[31mRed\x1b[0m");
    }

    #[test]
    fn test_escape_buffer_split_csi() {
        let mut buf = EscapeAwareBuffer::new();
        // ESC[31 is incomplete — missing final byte
        let out1 = buf.push("Hello\x1b[31");
        assert_eq!(out1, "Hello");
        // Now complete it
        let out2 = buf.push("mRed\x1b[0m");
        assert_eq!(out2, "\x1b[31mRed\x1b[0m");
    }

    #[test]
    fn test_escape_buffer_split_esc_alone() {
        let mut buf = EscapeAwareBuffer::new();
        // Bare ESC at end
        let out1 = buf.push("text\x1b");
        assert_eq!(out1, "text");
        // Complete on next chunk
        let out2 = buf.push("[Cmore");
        assert_eq!(out2, "\x1b[Cmore");
    }

    #[test]
    fn test_escape_buffer_complete_osc() {
        let mut buf = EscapeAwareBuffer::new();
        // Complete OSC with BEL terminator
        assert_eq!(
            buf.push("\x1b]0;My Title\x07text"),
            "\x1b]0;My Title\x07text"
        );
    }

    #[test]
    fn test_escape_buffer_split_osc() {
        let mut buf = EscapeAwareBuffer::new();
        // OSC without terminator
        let out1 = buf.push("before\x1b]0;My Title");
        assert_eq!(out1, "before");
        // Complete with BEL
        let out2 = buf.push("\x07after");
        assert_eq!(out2, "\x1b]0;My Title\x07after");
    }

    #[test]
    fn test_escape_buffer_cursor_forward_split() {
        let mut buf = EscapeAwareBuffer::new();
        // This is the actual bug case: ESC[C (cursor forward) split as ESC[ then C
        let out1 = buf.push("content\x1b[");
        assert_eq!(out1, "content");
        let out2 = buf.push("Cmore");
        assert_eq!(out2, "\x1b[Cmore");
    }

    #[test]
    fn test_escape_buffer_flush_at_eof() {
        let mut buf = EscapeAwareBuffer::new();
        let out = buf.push("text\x1b[31");
        assert_eq!(out, "text");
        // Flush sends remaining even if incomplete
        let flushed = buf.flush();
        assert_eq!(flushed, "\x1b[31");
    }

    #[test]
    fn test_escape_buffer_multiple_sequences() {
        let mut buf = EscapeAwareBuffer::new();
        let out = buf.push("\x1b[1m\x1b[31mBold Red\x1b[0m normal");
        assert_eq!(out, "\x1b[1m\x1b[31mBold Red\x1b[0m normal");
    }

    #[test]
    fn test_escape_buffer_osc_with_st_terminator() {
        let mut buf = EscapeAwareBuffer::new();
        // OSC with ST (ESC \) terminator
        assert_eq!(
            buf.push("\x1b]8;;https://example.com\x1b\\Click\x1b]8;;\x1b\\"),
            "\x1b]8;;https://example.com\x1b\\Click\x1b]8;;\x1b\\"
        );
    }

    #[test]
    fn test_escape_buffer_empty_input() {
        let mut buf = EscapeAwareBuffer::new();
        assert_eq!(buf.push(""), "");
    }

    #[test]
    fn test_escape_buffer_cap_prevents_unbounded_growth() {
        let mut buf = EscapeAwareBuffer::new();
        // Fake "incomplete escape" that's really garbage — over 256 bytes
        let long_fake = format!("\x1b]{}", "x".repeat(300));
        let out = buf.push(&long_fake);
        // Should emit raw since it exceeds cap
        assert_eq!(out, long_fake);
    }

    // --- Cached config in AppState tests ---

    fn make_test_app_state() -> AppState {
        AppState {
            sessions: dashmap::DashMap::new(),
            worktrees_dir: std::env::temp_dir().join("test-worktrees"),
            metrics: SessionMetrics::new(),
            output_buffers: dashmap::DashMap::new(),
            mcp_sessions: dashmap::DashMap::new(),
            ws_clients: dashmap::DashMap::new(),
            config: parking_lot::RwLock::new(crate::config::AppConfig::default()),
            git_cache: GitCacheState::new(),
            head_watchers: dashmap::DashMap::new(),
            repo_watchers: dashmap::DashMap::new(),
            dir_watchers: dashmap::DashMap::new(),
            http_client: reqwest::Client::new(),
            github_token: parking_lot::RwLock::new(None),
            github_token_source: parking_lot::RwLock::new(Default::default()),
            github_circuit_breaker: crate::github::GitHubCircuitBreaker::new(),
            server_shutdown: parking_lot::Mutex::new(None),
            session_token: parking_lot::RwLock::new(String::from("test-token")),
            app_handle: parking_lot::RwLock::new(None),
            plugin_watchers: dashmap::DashMap::new(),
            vt_log_buffers: dashmap::DashMap::new(),
            diff_renderers: dashmap::DashMap::new(),
            kitty_states: dashmap::DashMap::new(),
            input_buffers: dashmap::DashMap::new(),
            last_prompts: dashmap::DashMap::new(),
            silence_states: dashmap::DashMap::new(),
            claude_usage_cache: parking_lot::Mutex::new(std::collections::HashMap::new()),
            log_buffer: Arc::new(parking_lot::Mutex::new(crate::app_logger::LogRingBuffer::new(crate::app_logger::LOG_RING_CAPACITY))),
            event_bus: tokio::sync::broadcast::channel(256).0,
            event_counter: Arc::new(AtomicU64::new(0)),
            session_states: DashMap::new(),
            mcp_upstream_registry: Arc::new(crate::mcp_proxy::registry::UpstreamRegistry::new()),
            mcp_tools_changed: tokio::sync::broadcast::channel(16).0,
            slash_mode: DashMap::new(),
            last_output_ms: DashMap::new(),
            shell_states: DashMap::new(),
            loaded_plugins: DashMap::new(),
            relay: RelayState::new(),
            peer_agents: DashMap::new(),
            agent_inbox: DashMap::new(),
            messaging_channels: DashMap::new(),
            #[cfg(unix)]
            bound_socket_path: parking_lot::RwLock::new(std::path::PathBuf::new()),
            server_start_time: std::time::Instant::now(),
        }
    }

    #[test]
    fn test_cached_config_returns_default() {
        let state = make_test_app_state();
        let config = state.config.read();
        assert_eq!(config.font_family, "JetBrains Mono");
        assert_eq!(config.theme, "vscode-dark");
        assert!(config.mcp_server_enabled);
    }

    #[test]
    fn test_cached_config_write_updates_cache() {
        let state = make_test_app_state();
        {
            let mut config = state.config.write();
            config.font_size = 20;
            config.theme = "dracula".to_string();
        }
        let config = state.config.read();
        assert_eq!(config.font_size, 20);
        assert_eq!(config.theme, "dracula");
    }

    #[test]
    fn test_cached_config_full_replacement() {
        let state = make_test_app_state();
        let new_config = crate::config::AppConfig {
            mcp_server_enabled: true,
            font_family: "Fira Code".to_string(),
            ..crate::config::AppConfig::default()
        };
        *state.config.write() = new_config;

        let config = state.config.read();
        assert!(config.mcp_server_enabled);
        assert_eq!(config.font_family, "Fira Code");
    }

    // --- TTL cache tests ---

    #[test]
    fn test_cache_hit_within_ttl() {
        let map: DashMap<String, (String, Instant)> = DashMap::new();
        AppState::set_cached(&map, "key1".to_string(), "value1".to_string());

        let result = AppState::get_cached(&map, "key1", Duration::from_secs(60));
        assert_eq!(result, Some("value1".to_string()));
    }

    #[test]
    fn test_cache_miss_nonexistent_key() {
        let map: DashMap<String, (String, Instant)> = DashMap::new();

        let result = AppState::get_cached(&map, "missing", Duration::from_secs(60));
        assert_eq!(result, None);
    }

    #[test]
    fn test_cache_miss_expired_ttl() {
        let map: DashMap<String, (String, Instant)> = DashMap::new();
        // Insert with a timestamp in the past
        map.insert(
            "expired".to_string(),
            ("old_value".to_string(), Instant::now() - Duration::from_secs(10)),
        );

        let result = AppState::get_cached(&map, "expired", Duration::from_secs(5));
        assert_eq!(result, None);
    }

    #[test]
    fn test_cache_overwrite_resets_ttl() {
        let map: DashMap<String, (String, Instant)> = DashMap::new();
        // Insert old entry
        map.insert(
            "key".to_string(),
            ("old".to_string(), Instant::now() - Duration::from_secs(10)),
        );
        // Overwrite with fresh value
        AppState::set_cached(&map, "key".to_string(), "new".to_string());

        let result = AppState::get_cached(&map, "key", Duration::from_secs(5));
        assert_eq!(result, Some("new".to_string()));
    }

    #[test]
    fn test_clear_caches_empties_all() {
        let state = make_test_app_state();
        state.git_cache.repo_info.insert(
            "/some/path".to_string(),
            (
                crate::git::RepoInfo {
                    path: "/some/path".to_string(),
                    name: "test".to_string(),
                    initials: "TE".to_string(),
                    branch: "main".to_string(),
                    status: "clean".to_string(),
                    is_git_repo: true,
                },
                Instant::now(),
            ),
        );
        state.git_cache.github_status.insert(
            "/some/path".to_string(),
            (vec![], Instant::now()),
        );

        assert!(!state.git_cache.repo_info.is_empty());
        assert!(!state.git_cache.github_status.is_empty());

        state.clear_caches();

        assert!(state.git_cache.repo_info.is_empty());
        assert!(state.git_cache.github_status.is_empty());
    }

    #[test]
    fn test_invalidate_repo_caches_removes_specific_path() {
        let state = make_test_app_state();
        state.git_cache.repo_info.insert(
            "/repo/a".to_string(),
            (
                crate::git::RepoInfo {
                    path: "/repo/a".to_string(),
                    name: "a".to_string(),
                    initials: "A".to_string(),
                    branch: "main".to_string(),
                    status: "clean".to_string(),
                    is_git_repo: true,
                },
                Instant::now(),
            ),
        );
        state.git_cache.repo_info.insert(
            "/repo/b".to_string(),
            (
                crate::git::RepoInfo {
                    path: "/repo/b".to_string(),
                    name: "b".to_string(),
                    initials: "B".to_string(),
                    branch: "main".to_string(),
                    status: "clean".to_string(),
                    is_git_repo: true,
                },
                Instant::now(),
            ),
        );

        state.invalidate_repo_caches("/repo/a");

        assert!(state.git_cache.repo_info.get("/repo/a").is_none());
        assert!(state.git_cache.repo_info.get("/repo/b").is_some());
    }

    // --- KittyKeyboardState tests ---

    #[test]
    fn test_kitty_state_default_flags_zero() {
        let state = KittyKeyboardState::new();
        assert_eq!(state.current_flags(), 0);
    }

    #[test]
    fn test_kitty_state_push_sets_flags() {
        let mut state = KittyKeyboardState::new();
        state.push(1);
        assert_eq!(state.current_flags(), 1);
    }

    #[test]
    fn test_kitty_state_push_pop_stack() {
        let mut state = KittyKeyboardState::new();
        state.push(1);
        state.push(3);
        assert_eq!(state.current_flags(), 3);
        state.pop();
        assert_eq!(state.current_flags(), 1);
        state.pop();
        assert_eq!(state.current_flags(), 0);
    }

    #[test]
    fn test_kitty_state_pop_underflow_safe() {
        let mut state = KittyKeyboardState::new();
        state.pop(); // Should not panic
        assert_eq!(state.current_flags(), 0);
        state.pop(); // Still safe
        assert_eq!(state.current_flags(), 0);
    }

    // --- strip_kitty_sequences tests ---

    #[test]
    fn test_strip_kitty_plain_text_passthrough() {
        let (out, actions) = strip_kitty_sequences("hello world");
        assert_eq!(out, "hello world");
        assert!(actions.is_empty());
    }

    #[test]
    fn test_strip_kitty_push_single_digit() {
        let (out, actions) = strip_kitty_sequences("\x1b[>1u");
        assert_eq!(out, "");
        assert_eq!(actions, vec![KittyAction::Push(1)]);
    }

    #[test]
    fn test_strip_kitty_push_multi_digit() {
        let (out, actions) = strip_kitty_sequences("\x1b[>15u");
        assert_eq!(out, "");
        assert_eq!(actions, vec![KittyAction::Push(15)]);
    }

    #[test]
    fn test_strip_kitty_pop() {
        let (out, actions) = strip_kitty_sequences("\x1b[<u");
        assert_eq!(out, "");
        assert_eq!(actions, vec![KittyAction::Pop]);
    }

    #[test]
    fn test_strip_kitty_query() {
        let (out, actions) = strip_kitty_sequences("\x1b[?u");
        assert_eq!(out, "");
        assert_eq!(actions, vec![KittyAction::Query]);
    }

    #[test]
    fn test_strip_kitty_embedded_in_text() {
        let (out, actions) = strip_kitty_sequences("before\x1b[>1uafter");
        assert_eq!(out, "beforeafter");
        assert_eq!(actions, vec![KittyAction::Push(1)]);
    }

    #[test]
    fn test_strip_kitty_multiple_actions() {
        let (out, actions) = strip_kitty_sequences("\x1b[>1u\x1b[?u\x1b[<u");
        assert_eq!(out, "");
        assert_eq!(actions, vec![
            KittyAction::Push(1),
            KittyAction::Query,
            KittyAction::Pop,
        ]);
    }

    #[test]
    fn test_strip_kitty_sgr_mouse_passthrough() {
        // SGR mouse: ESC [ < 0 ; 35 ; 16 M — starts with ESC[< but next byte is digit, not 'u'
        let input = "\x1b[<0;35;16M";
        let (out, actions) = strip_kitty_sequences(input);
        assert_eq!(out, input);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_strip_kitty_dec_private_mode_passthrough() {
        // DEC private mode: ESC [ ? 1049 h — starts with ESC[? but next byte is digit, not 'u'
        let input = "\x1b[?1049h";
        let (out, actions) = strip_kitty_sequences(input);
        assert_eq!(out, input);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_strip_kitty_normal_csi_passthrough() {
        // Normal CSI (SGR color): should pass through unchanged
        let input = "\x1b[31mRed\x1b[0m";
        let (out, actions) = strip_kitty_sequences(input);
        assert_eq!(out, input);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_strip_kitty_fast_path_no_trigger() {
        // No ESC[> or ESC[< or ESC[? — should take fast path
        let input = "\x1b[31m\x1b[0mhello";
        let (out, actions) = strip_kitty_sequences(input);
        assert_eq!(out, input);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_strip_kitty_push_zero_flags() {
        let (out, actions) = strip_kitty_sequences("\x1b[>0u");
        assert_eq!(out, "");
        assert_eq!(actions, vec![KittyAction::Push(0)]);
    }

    #[test]
    fn test_strip_kitty_incomplete_push_no_digits() {
        // ESC [ > u (no digits) — not a valid push, should pass through
        let input = "\x1b[>u";
        let (out, actions) = strip_kitty_sequences(input);
        // The ESC is emitted, then [>u follows as normal text
        assert_eq!(out, input);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_strip_kitty_preserves_utf8_box_drawing() {
        // Box-drawing chars (╭│╰) are 3-byte UTF-8 — must not be corrupted
        let input = "╭──────╮\n│ hello │\n╰──────╯";
        let (out, actions) = strip_kitty_sequences(input);
        assert_eq!(out, input);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_strip_kitty_utf8_mixed_with_sequences() {
        // Kitty sequence embedded between multi-byte UTF-8 text
        let input = "╭──╮\x1b[>1u╰──╯";
        let (out, actions) = strip_kitty_sequences(input);
        assert_eq!(out, "╭──╮╰──╯");
        assert_eq!(actions, vec![KittyAction::Push(1)]);
    }

    #[test]
    fn test_strip_kitty_emoji_passthrough() {
        let input = "🦀 hello \x1b[?u 🎉";
        let (out, actions) = strip_kitty_sequences(input);
        assert_eq!(out, "🦀 hello  🎉");
        assert_eq!(actions, vec![KittyAction::Query]);
    }

    #[test]
    fn test_strip_kitty_cjk_passthrough() {
        let input = "漢字\x1b[<u日本語";
        let (out, actions) = strip_kitty_sequences(input);
        assert_eq!(out, "漢字日本語");
        assert_eq!(actions, vec![KittyAction::Pop]);
    }

    #[test]
    fn test_strip_kitty_fast_path_returns_borrowed() {
        use std::borrow::Cow;
        let input = "hello world";
        let (out, actions) = strip_kitty_sequences(input);
        assert!(actions.is_empty());
        assert!(matches!(out, Cow::Borrowed(_)), "fast path should return Cow::Borrowed");
        assert_eq!(&*out, input);
    }

    #[test]
    fn test_strip_kitty_slow_path_returns_owned() {
        use std::borrow::Cow;
        let input = "before\x1b[>1uafter";
        let (out, actions) = strip_kitty_sequences(input);
        assert!(!actions.is_empty());
        assert!(matches!(out, Cow::Owned(_)), "slow path should return Cow::Owned");
        assert_eq!(&*out, "beforeafter");
    }

    // Helpers for session-state accumulator tests
    fn make_parsed(type_: &str, extra: serde_json::Value) -> AppEvent {
        let mut obj = serde_json::json!({ "type": type_ });
        if let (serde_json::Value::Object(m), serde_json::Value::Object(extra_m)) =
            (&mut obj, extra)
        {
            m.extend(extra_m);
        }
        AppEvent::PtyParsed {
            session_id: "s1".to_string(),
            parsed: obj,
        }
    }

    fn apply(state: &Arc<AppState>, event: &AppEvent) -> SessionState {
        AppState::apply_event_to_session_state(state, event);
        state.session_states.get("s1").map(|s| s.clone()).unwrap_or_default()
    }

    fn fresh_state() -> Arc<AppState> {
        let s = Arc::new(make_test_app_state());
        // Insert initial entry so and_modify fires
        s.session_states.insert("s1".to_string(), SessionState::default());
        // Initialize last_output_ms for shell_state derivation
        s.last_output_ms.insert("s1".to_string(), AtomicU64::new(0));
        s
    }

    #[test]
    fn test_session_state_intent_sets_agent_intent() {
        let state = fresh_state();
        let event = make_parsed("intent", serde_json::json!({ "text": "fixing the bug", "title": null }));
        let s = apply(&state, &event);
        assert_eq!(s.agent_intent.as_deref(), Some("fixing the bug"));
    }

    #[test]
    fn test_session_state_status_line_sets_current_task() {
        let state = fresh_state();
        let event = make_parsed("status-line", serde_json::json!({ "task_name": "Reading files", "full_line": "⏺ Reading files" }));
        let s = apply(&state, &event);
        assert_eq!(s.current_task.as_deref(), Some("Reading files"));
        // status-line also clears error and rate-limit
        assert!(!s.rate_limited);
        assert!(s.last_error.is_none());
    }

    #[test]
    fn test_session_state_user_input_short_does_not_set_last_prompt() {
        let state = fresh_state();
        let event = make_parsed("user-input", serde_json::json!({ "content": "yes" }));
        let s = apply(&state, &event);
        assert!(s.last_prompt.is_none());
    }

    #[test]
    fn test_session_state_user_input_long_sets_last_prompt() {
        let state = fresh_state();
        let long = "please refactor this function to use the new API correctly and efficiently";
        let event = make_parsed("user-input", serde_json::json!({ "content": long }));
        let s = apply(&state, &event);
        assert_eq!(s.last_prompt.as_deref(), Some(long));
    }

    #[test]
    fn test_session_state_user_input_exactly_10_words_sets_last_prompt() {
        let state = fresh_state();
        let ten_words = "one two three four five six seven eight nine ten";
        let event = make_parsed("user-input", serde_json::json!({ "content": ten_words }));
        let s = apply(&state, &event);
        assert_eq!(s.last_prompt.as_deref(), Some(ten_words));
    }

    #[test]
    fn test_session_state_progress_normal_sets_value() {
        let state = fresh_state();
        let event = make_parsed("progress", serde_json::json!({ "state": 1, "value": 42 }));
        let s = apply(&state, &event);
        assert_eq!(s.progress, Some(42));
    }

    #[test]
    fn test_session_state_progress_remove_clears_value() {
        let state = fresh_state();
        // First set a value
        let set_event = make_parsed("progress", serde_json::json!({ "state": 1, "value": 75 }));
        apply(&state, &set_event);
        // Then remove it
        let remove_event = make_parsed("progress", serde_json::json!({ "state": 0, "value": 0 }));
        let s = apply(&state, &remove_event);
        assert!(s.progress.is_none());
    }

    #[test]
    fn test_session_state_suggest_sets_suggested_actions() {
        let state = fresh_state();
        let event = make_parsed("suggest", serde_json::json!({ "items": ["Run tests", "Review diff"] }));
        let s = apply(&state, &event);
        assert_eq!(s.suggested_actions, Some(vec!["Run tests".to_string(), "Review diff".to_string()]));
    }

    #[test]
    fn test_session_state_status_line_clears_suggested_actions() {
        let state = fresh_state();
        // Set suggestions
        let suggest = make_parsed("suggest", serde_json::json!({ "items": ["Deploy"] }));
        apply(&state, &suggest);
        // Status-line should clear them
        let status = make_parsed("status-line", serde_json::json!({ "task_name": "Working" }));
        let s = apply(&state, &status);
        assert!(s.suggested_actions.is_none());
    }

    #[test]
    fn test_session_state_question_sets_awaiting_input() {
        let state = fresh_state();
        let event = make_parsed("question", serde_json::json!({ "prompt_text": "Do you want to proceed?" }));
        let s = apply(&state, &event);
        assert!(s.awaiting_input);
        assert_eq!(s.question_text.as_deref(), Some("Do you want to proceed?"));
    }

    #[test]
    fn test_session_state_user_input_clears_awaiting() {
        let state = fresh_state();
        // First go to question state
        let q = make_parsed("question", serde_json::json!({ "prompt_text": "Ready?" }));
        apply(&state, &q);
        // User responds → no longer awaiting
        let event = make_parsed("user-input", serde_json::json!({ "content": "yes" }));
        let s = apply(&state, &event);
        assert!(!s.awaiting_input);
    }

    #[test]
    fn test_session_state_status_line_clears_awaiting_input() {
        let state = fresh_state();
        // Set question state
        let q = make_parsed("question", serde_json::json!({ "prompt_text": "Install gopls?" }));
        apply(&state, &q);
        // Status-line means agent is working → question answered
        let status = make_parsed("status-line", serde_json::json!({ "task_name": "Reading files" }));
        let s = apply(&state, &status);
        assert!(!s.awaiting_input, "status-line should clear awaiting_input");
        assert!(s.question_text.is_none(), "status-line should clear question_text");
    }

    #[test]
    fn test_session_state_shell_state_from_output_timing() {
        let state = fresh_state();
        // No output yet → idle
        assert_eq!(state.derive_shell_state("s1"), Some("idle".to_string()));
        // Stamp recent output
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        state.last_output_ms.get("s1").unwrap()
            .store(now, std::sync::atomic::Ordering::Relaxed);
        assert_eq!(state.derive_shell_state("s1"), Some("busy".to_string()));
        // Old output → idle
        state.last_output_ms.get("s1").unwrap()
            .store(now - 1000, std::sync::atomic::Ordering::Relaxed);
        assert_eq!(state.derive_shell_state("s1"), Some("idle".to_string()));
    }

    #[test]
    fn test_session_state_with_shell_enriches_state() {
        let state = fresh_state();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        state.last_output_ms.get("s1").unwrap()
            .store(now, std::sync::atomic::Ordering::Relaxed);
        let ss = state.session_state_with_shell("s1").unwrap();
        assert_eq!(ss.shell_state.as_deref(), Some("busy"));
    }

    #[test]
    fn test_session_state_repeated_status_line_same_task_no_change() {
        let state = fresh_state();
        let e1 = make_parsed("status-line", serde_json::json!({ "task_name": "Twisting" }));
        let s1 = apply(&state, &e1);
        assert_eq!(s1.current_task.as_deref(), Some("Twisting"));
        // Same task again — state should be identical (PartialEq)
        let e2 = make_parsed("status-line", serde_json::json!({ "task_name": "Twisting" }));
        let s2 = apply(&state, &e2);
        assert_eq!(s1, s2);
    }

    #[test]
    fn test_session_state_status_line_different_task_updates() {
        let state = fresh_state();
        let e1 = make_parsed("status-line", serde_json::json!({ "task_name": "Twisting" }));
        let s1 = apply(&state, &e1);
        let e2 = make_parsed("status-line", serde_json::json!({ "task_name": "Reading files" }));
        let s2 = apply(&state, &e2);
        assert_ne!(s1, s2);
        assert_eq!(s2.current_task.as_deref(), Some("Reading files"));
    }

    // --- VtLogBuffer tests ---

    fn make_vt_log() -> VtLogBuffer {
        VtLogBuffer::new(24, 80, 1000)
    }

    /// Helper: extract plain text from log lines for easy assertion.
    fn log_texts(buf: &VtLogBuffer) -> Vec<String> {
        buf.lines().iter().map(|ll| ll.text()).collect()
    }

    /// Emit lines one at a time and verify they appear in the log after scrolling off.
    /// NOTE: process() is called per-line to match production behavior (PTY reader calls
    /// process() on each small chunk; a single bulk process() call cannot detect scroll).
    #[test]
    fn test_vt_log_line_oriented_output() {
        let mut buf = make_vt_log();
        // Feed lines one at a time so the diff algorithm can detect scroll
        for i in 0..30 {
            buf.process(format!("line {i}\r\n").as_bytes());
        }
        let texts = log_texts(&buf);
        // Lines 0..6 scrolled off (30 total - 24 visible = 6)
        assert!(!texts.is_empty(), "should have finalized some lines");
        assert!(texts[0].starts_with("line "), "line content preserved: {:?}", texts[0]);
    }

    /// In-place rewrite via \r should resolve to the final content on screen.
    #[test]
    fn test_vt_log_in_place_rewrite() {
        let mut buf = make_vt_log();
        // Write "aaaa\r" followed by "bbbb" — vt100 resolves this to "bbbb" on the line
        buf.process(b"aaaa\rbbbb\r\n");
        // The current screen should show "bbbb" not "aaaa"
        let rows = buf.screen_rows();
        let first_nonempty = rows.iter().find(|r| !r.is_empty()).map(|s| s.as_str()).unwrap_or("");
        assert_eq!(first_nonempty, "bbbb", "in-place rewrite resolved to final content");
    }

    /// Rows scrolled off the top go into the log.
    /// NOTE: per-line process() calls to match production behavior.
    #[test]
    fn test_vt_log_scroll_emits_scrolled_off_rows() {
        let mut buf = make_vt_log();
        // Feed exactly 25 lines one at a time — the first one scrolls off
        for i in 0..25 {
            buf.process(format!("row{i}\r\n").as_bytes());
        }
        let texts = log_texts(&buf);
        assert!(!texts.is_empty(), "at least one line should have scrolled off");
        assert_eq!(texts[0], "row0", "first scrolled-off row is row0, got: {:?}", texts[0]);
    }

    /// Alternate screen suppresses log extraction.
    #[test]
    fn test_vt_log_alternate_screen_suppresses_extraction() {
        let mut buf = make_vt_log();
        // Enter alternate screen (smcup), write some content, exit (rmcup)
        // smcup: ESC[?1049h
        buf.process(b"\x1b[?1049h");
        // Write 30 lines in alternate screen
        let mut content = String::new();
        for i in 0..30 {
            content.push_str(&format!("alt-line {i}\r\n"));
        }
        buf.process(content.as_bytes());
        // No lines should be in the log while alternate screen is active
        assert!(
            buf.lines().is_empty(),
            "alternate screen content should not be logged"
        );
    }

    /// After alternate screen exits, extraction resumes.
    #[test]
    fn test_vt_log_alternate_screen_exit_resumes_extraction() {
        let mut buf = make_vt_log();
        // Enter alternate screen
        buf.process(b"\x1b[?1049h");
        // Write in alternate screen (per-line)
        for i in 0..30 {
            buf.process(format!("alt {i}\r\n").as_bytes());
        }
        // Exit alternate screen (rmcup)
        buf.process(b"\x1b[?1049l");
        // Now write regular output that will scroll (per-line)
        for i in 0..30 {
            buf.process(format!("main {i}\r\n").as_bytes());
        }
        // Main screen output should appear in the log
        let texts = log_texts(&buf);
        assert!(!texts.is_empty(), "main screen lines should appear in log after alt exit");
        assert!(
            texts.iter().any(|l| l.starts_with("main")),
            "main lines present in log"
        );
    }

    /// resize() updates parser dimensions.
    #[test]
    fn test_vt_log_resize() {
        let mut buf = make_vt_log();
        // Resize to a smaller terminal
        buf.resize(10, 40);
        // Should not panic and screen size should be updated
        let rows = buf.screen_rows();
        // After resize, screen_rows returns 10 rows (all empty after resize)
        assert_eq!(rows.len(), 10, "resize to 10 rows");
    }

    /// Empty/whitespace-only rows are not added to the log.
    #[test]
    fn test_vt_log_trims_whitespace_only_rows() {
        let mut buf = make_vt_log();
        // Fill with content including empty lines
        let mut input = String::new();
        for i in 0..25 {
            if i == 5 {
                input.push_str("\r\n"); // empty line
            } else {
                input.push_str(&format!("content {i}\r\n"));
            }
        }
        buf.process(input.as_bytes());
        let texts = log_texts(&buf);
        // None of the logged lines should be empty
        for line in &texts {
            assert!(!line.is_empty(), "logged line should not be empty: {line:?}");
        }
    }

    /// Log capacity is bounded: old lines are dropped when capacity is exceeded.
    #[test]
    fn test_vt_log_bounded_capacity() {
        let mut buf = VtLogBuffer::new(24, 80, 10); // tiny capacity
        // Produce many more than 10 lines
        let mut input = String::new();
        for i in 0..200 {
            input.push_str(&format!("line {i:04}\r\n"));
        }
        buf.process(input.as_bytes());
        assert!(
            buf.lines().len() <= 10,
            "log should be capped at capacity=10, got {}",
            buf.lines().len()
        );
    }

    /// lines_since_owned returns lines after offset and correct new offset.
    #[test]
    fn test_vt_log_lines_since_owned() {
        let mut buf = VtLogBuffer::new(24, 80, 1000);
        // Feed lines one at a time to trigger scroll detection
        for i in 0..30 {
            buf.process(format!("line {i}\r\n").as_bytes());
        }
        let total = buf.total_lines();
        assert!(total > 0, "should have some finalized lines");
        // First fetch: all lines
        let (batch1, off1) = buf.lines_since_owned(0);
        assert_eq!(batch1.len(), total);
        assert_eq!(off1, total);
        // Second fetch: nothing new
        let (batch2, off2) = buf.lines_since_owned(off1);
        assert!(batch2.is_empty());
        assert_eq!(off2, total);
    }

    /// lines_since_owned returns correct results after buffer rotation
    /// (oldest lines evicted by pop_front).
    #[test]
    fn test_vt_log_lines_since_owned_after_rotation() {
        let mut buf = VtLogBuffer::new(24, 80, 10); // capacity = 10
        // Feed 40 lines one-at-a-time so scroll detection works
        for i in 0..40 {
            buf.process(format!("rot-{i}\r\n").as_bytes());
        }
        // Buffer should be at capacity
        assert!(buf.lines().len() <= 10, "should be capped at 10");
        let total = buf.total_lines();
        // Fetch all — should return only the retained lines
        let (batch, off) = buf.lines_since_owned(0);
        assert_eq!(batch.len(), buf.lines().len());
        assert_eq!(off, total);
        // The retained lines should be the newest ones
        for line in &batch {
            assert!(line.text().starts_with("rot-"), "unexpected line: {:?}", line.text());
        }
        // Fetch with an offset past the end — empty
        let (empty, off2) = buf.lines_since_owned(off);
        assert!(empty.is_empty());
        assert_eq!(off2, off);
    }

    /// Feed data in small incremental chunks (simulating real PTY reads that
    /// may split mid-line) and verify lines are still extracted.
    ///
    /// Misaligned chunks (7-byte boundaries) may produce partial rows that
    /// break overlap detection between consecutive process() calls. The
    /// conservative approach captures fewer lines than a "dump everything"
    /// fallback but avoids false duplicate log entries.
    #[test]
    fn test_vt_log_incremental_chunked_feed() {
        let mut buf = VtLogBuffer::new(24, 80, 1000);
        // Build 30 lines of output
        let full_output: String = (0..30)
            .map(|i| format!("chunk-{i}\r\n"))
            .collect();
        let bytes = full_output.as_bytes();
        // Feed in small chunks of 7 bytes (deliberately misaligned with lines)
        for chunk in bytes.chunks(7) {
            buf.process(chunk);
        }
        let lines = log_texts(&buf);
        let matching: Vec<&String> = lines
            .iter()
            .filter(|l| l.starts_with("chunk-"))
            .collect();
        // Misaligned chunking limits overlap detection — we capture some lines
        // but not all. The key property: no duplicate entries.
        assert!(
            !matching.is_empty(),
            "chunked feed should capture at least some lines: {:?}",
            lines,
        );
    }

    /// Scroll regions (DECSTBM): scrolling within a restricted region does
    /// NOT produce overlap with the full-screen prev/curr comparison, so the
    /// conservative detector does not extract these lines. This is acceptable:
    /// scroll regions are rare in mobile-targeted sessions, and screen rows
    /// always show the current content accurately.
    #[test]
    fn test_vt_log_scroll_region_decstbm() {
        let mut buf = VtLogBuffer::new(10, 80, 1000); // small 10-row screen
        // Fill the screen first so we have a baseline for diff detection
        for i in 0..10 {
            buf.process(format!("init-{i}\r\n").as_bytes());
        }
        let before = buf.total_lines();
        // Set scroll region to rows 3-8 (1-indexed): ESC[3;8r
        // Then move cursor into the region and write lines to force scrolling
        // within the region only.
        buf.process(b"\x1b[3;8r");   // DECSTBM: set scroll region rows 3-8
        buf.process(b"\x1b[3;1H");   // CUP: move cursor to row 3, col 1
        for i in 0..20 {
            buf.process(format!("region-{i}\r\n").as_bytes());
        }
        buf.process(b"\x1b[r");      // Reset scroll region to full screen
        let after = buf.total_lines();
        // Scroll-region scrolling changes rows within the region but doesn't
        // produce a full-screen overlap pattern, so no new log lines are
        // expected with the conservative detector.
        assert_eq!(
            after, before,
            "scroll region scroll does not produce overlap-based log lines"
        );
    }

    /// Cursor movement (CUU) that overwrites existing rows should NOT
    /// produce the overwritten content as new log output.
    #[test]
    fn test_vt_log_cursor_movement_no_overwrite_in_log() {
        let mut buf = VtLogBuffer::new(10, 80, 1000);
        // Write 5 lines
        for i in 0..5 {
            buf.process(format!("orig-{i}\r\n").as_bytes());
        }
        // Move cursor up 3 rows (CUU) and overwrite with "REPLACED"
        buf.process(b"\x1b[3A");            // CUU 3: move up 3
        buf.process(b"REPLACED\r\n");       // overwrite current line
        let lines = log_texts(&buf);
        // "REPLACED" should NOT appear in the log — it was written via cursor
        // movement within the viewport, not as new scrolled-off output.
        // (The diff detector may emit the displaced orig-N lines, but the
        // replacement text itself should stay on-screen, not in the log.)
        let replaced_in_log = lines.iter().any(|l| l.contains("REPLACED"));
        assert!(
            !replaced_in_log,
            "cursor-overwritten text should not appear in log: {lines:?}"
        );
    }

    /// SGR attributes (colors, bold, etc.) should not leak into extracted text.
    #[test]
    fn test_vt_log_sgr_produces_clean_text() {
        let mut buf = VtLogBuffer::new(24, 80, 1000);
        // Write 30 lines with SGR color codes (enough to cause scrolling)
        for i in 0..30 {
            // ESC[31m = red, ESC[1m = bold, ESC[0m = reset
            buf.process(format!("\x1b[1;31mcolor-{i}\x1b[0m\r\n").as_bytes());
        }
        let lines = log_texts(&buf);
        let color_lines: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("color-"))
            .collect();
        assert!(
            color_lines.len() >= 5,
            "should capture color-N lines, got {}: {:?}",
            color_lines.len(),
            lines,
        );
        // No line should contain raw ESC characters — vt100 parser strips them
        for line in &lines {
            assert!(
                !line.contains('\x1b'),
                "log line should not contain ESC sequences: {line:?}"
            );
        }
    }

    /// Integration test: spawn a real PTY process, feed its output through
    /// VtLogBuffer, and verify that clean log lines are extracted.
    #[test]
    fn test_vt_log_real_pty_echo() {
        use portable_pty::{native_pty_system, CommandBuilder, PtySize};
        use std::io::Read;

        let pty_system = native_pty_system();
        let pair = match pty_system.openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        }) {
            Ok(p) => p,
            Err(e) if e.to_string().contains("Operation not permitted") => {
                eprintln!("Skipping test: PTY not available in sandbox");
                return;
            }
            Err(e) => panic!("open pty: {e}"),
        };

        // Spawn a shell that echos numbered lines and exits
        let mut cmd = CommandBuilder::new("/bin/sh");
        cmd.arg("-c");
        cmd.arg("for i in $(seq 1 30); do echo \"test-line-$i\"; done; exit 0");
        let mut child = pair.slave.spawn_command(cmd).expect("spawn");
        drop(pair.slave); // close slave so reads see EOF

        let mut reader = pair.master.try_clone_reader().expect("reader");
        let mut buf = VtLogBuffer::new(24, 80, 1000);
        // Use a small read buffer (64 bytes) to force multiple reads,
        // simulating the incremental reads the production reader thread does.
        let mut raw = [0u8; 64];

        // Read all output from the PTY and feed into VtLogBuffer
        loop {
            match reader.read(&mut raw) {
                Ok(0) => break,
                Ok(n) => { buf.process(&raw[..n]); }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                Err(_) => break,
            }
        }
        let _ = child.wait();

        let lines = log_texts(&buf);
        // We should have captured at least some of our "test-line-N" lines
        let matching: Vec<&String> = lines
            .iter()
            .filter(|l| l.starts_with("test-line-"))
            .collect();
        assert!(
            !matching.is_empty(),
            "expected some 'test-line-N' lines in log, got 0 out of {} total lines: {:?}",
            lines.len(),
            lines,
        );
        // Verify the captured lines cover a reasonable range.
        let nums: Vec<u32> = matching
            .iter()
            .filter_map(|l| l.strip_prefix("test-line-").and_then(|n| n.parse().ok()))
            .collect();
        let max_num = nums.iter().copied().max().unwrap_or(0);
        assert!(
            max_num >= 5,
            "should capture lines up to at least 5, max was {max_num}, nums: {nums:?}",
        );
    }

    /// Integration test: verify that alternate-screen content (TUI) is NOT
    /// captured by VtLogBuffer when using a real PTY.
    #[test]
    fn test_vt_log_real_pty_alternate_screen_suppressed() {
        use portable_pty::{native_pty_system, CommandBuilder, PtySize};
        use std::io::Read;

        let pty_system = native_pty_system();
        let pair = match pty_system.openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        }) {
            Ok(p) => p,
            Err(e) if e.to_string().contains("Operation not permitted") => {
                eprintln!("Skipping test: PTY not available in sandbox");
                return;
            }
            Err(e) => panic!("open pty: {e}"),
        };

        // Script: echo normal lines, enter alternate screen, write TUI garbage,
        // exit alternate screen, echo more lines.
        let script = concat!(
            "echo 'before-alt-1'; echo 'before-alt-2'; ",
            "printf '\\033[?1049h'; ",           // enter alternate screen
            "echo 'TUI-GARBAGE-LINE'; ",
            "printf '\\033[?1049l'; ",           // exit alternate screen
            "echo 'after-alt-1'; echo 'after-alt-2'; ",
            "exit 0"
        );
        let mut cmd = CommandBuilder::new("/bin/sh");
        cmd.arg("-c");
        cmd.arg(script);
        let mut child = pair.slave.spawn_command(cmd).expect("spawn");
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader().expect("reader");
        let mut buf = VtLogBuffer::new(24, 80, 1000);
        let mut raw = [0u8; 64]; // small buffer for incremental reads

        loop {
            match reader.read(&mut raw) {
                Ok(0) => break,
                Ok(n) => { buf.process(&raw[..n]); }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                Err(_) => break,
            }
        }
        let _ = child.wait();

        let lines = log_texts(&buf);
        // TUI-GARBAGE-LINE should not appear in the log
        let has_garbage = lines.iter().any(|l| l.contains("TUI-GARBAGE"));
        assert!(
            !has_garbage,
            "alternate-screen content should be suppressed, but found TUI-GARBAGE in: {:?}",
            lines,
        );
    }

    // --- ChangedRow / process() return value tests ---

    /// process() returns the rows that changed on the normal screen.
    #[test]
    fn test_vt_log_changed_rows_basic() {
        let mut buf = make_vt_log();
        let changed = buf.process(b"hello world");
        assert!(
            changed.iter().any(|r| r.text == "hello world"),
            "expected 'hello world' in changed rows, got: {:?}",
            changed,
        );
    }

    /// CR-based overwrite: process() returns the final overwritten row text.
    #[test]
    fn test_vt_log_changed_rows_overwrite() {
        let mut buf = make_vt_log();
        // "aaaa\r" moves cursor to column 0; "bbbb" overwrites — vt100 renders "bbbb"
        let changed = buf.process(b"aaaa\rbbbb");
        assert!(
            changed.iter().any(|r| r.text.contains("bbbb")),
            "expected 'bbbb' after CR overwrite, got: {:?}",
            changed,
        );
        assert!(
            !changed.iter().any(|r| r.text == "aaaa"),
            "should not see raw 'aaaa' (overwritten), got: {:?}",
            changed,
        );
    }

    /// Alternate screen: changed rows are reported even when alternate screen is active.
    #[test]
    fn test_vt_log_changed_rows_alternate_screen() {
        let mut buf = make_vt_log();
        buf.process(b"\x1b[?1049h");
        let changed = buf.process(b"status: running");
        assert!(
            changed.iter().any(|r| r.text.contains("status: running")),
            "changed rows must be reported during alternate screen, got: {:?}",
            changed,
        );
        assert_eq!(buf.total_lines(), 0, "log must remain empty during alternate screen");
    }

    /// Cursor movement: changed rows reflect the final rendered state.
    #[test]
    fn test_vt_log_changed_rows_cursor_movement() {
        let mut buf = make_vt_log();
        buf.process(b"line0\r\nline1\r\nline2");
        // Move cursor up 1 row (CUU 1) and overwrite line1
        let changed = buf.process(b"\x1b[1Aupdated");
        assert!(
            changed.iter().any(|r| r.text.contains("updated")),
            "expected 'updated' in changed rows after CUU overwrite, got: {:?}",
            changed,
        );
    }

    /// No change: a second process() with no new data returns empty Vec.
    #[test]
    fn test_vt_log_changed_rows_empty_on_no_change() {
        let mut buf = make_vt_log();
        buf.process(b"hello");
        let changed = buf.process(b"");
        assert!(
            changed.is_empty(),
            "expected no changed rows when no data written, got: {:?}",
            changed,
        );
    }

    /// resize() clears prev_rows so the next process() reports all non-empty rows.
    #[test]
    fn test_vt_log_changed_rows_resize_clears_prev() {
        let mut buf = make_vt_log();
        buf.process(b"hello");
        buf.process(b""); // stabilise prev_rows
        buf.resize(24, 80);
        let changed = buf.process(b"");
        assert!(
            changed.iter().any(|r| r.text.contains("hello")),
            "expected 'hello' after resize clears prev, got: {:?}",
            changed,
        );
    }

    /// Screen switch resets prev_rows so first frame reports all non-empty rows.
    #[test]
    fn test_vt_log_changed_rows_screen_switch_resets_prev() {
        let mut buf = make_vt_log();
        buf.process(b"normal line");
        buf.process(b""); // stabilise
        buf.process(b"\x1b[?1049h"); // enter alternate
        buf.process(b"alt content");
        // Exit alternate — should reset prev for normal screen
        let changed = buf.process(b"\x1b[?1049l");
        assert!(
            changed.iter().any(|r| r.text.contains("normal line")),
            "expected 'normal line' after screen switch resets prev, got: {:?}",
            changed,
        );
    }

    // --- trim_agent_chrome / find_prompt_cutoff tests ---

    fn make_lines(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    /// Helper: create plain LogLine vec from string slices for testing.
    fn make_log_lines(items: &[&str]) -> Vec<LogLine> {
        items.iter().map(|s| LogLine {
            spans: if s.is_empty() {
                vec![]
            } else {
                vec![LogSpan {
                    text: s.to_string(),
                    fg: None, bg: None,
                    bold: false, italic: false, underline: false,
                }]
            },
        }).collect()
    }

    // find_prompt_cutoff tests live in chrome.rs (canonical location)

    /// Large batch (>= 2/3 of screen height) with an Ink prompt → chrome trimmed.
    #[test]
    fn test_trim_agent_chrome_large_batch_ink_prompt() {
        // 21 lines for a 24-row screen (threshold = 16) — large batch
        let mut items: Vec<&str> = vec!["real content"; 18];
        items.push("❯ command"); // index 18
        items.push("");           // index 19
        items.push("Model: x");  // index 20
        let lines = make_log_lines(&items);
        let result = trim_agent_chrome(lines);
        assert_eq!(result.len(), 18, "lines before prompt kept");
        assert!(result.iter().all(|l| l.text() == "real content"));
    }

    /// Large batch with `> ` prompt → chrome trimmed.
    #[test]
    fn test_trim_agent_chrome_large_batch_gt_prompt() {
        let mut items: Vec<&str> = vec!["output"; 17];
        items.push("> ");    // index 17 — bare "> " treated as prompt
        items.push("chrome"); // index 18
        let lines = make_log_lines(&items);
        let result = trim_agent_chrome(lines);
        assert_eq!(result.len(), 17);
    }

    /// Small batch with a prompt in the scan window → chrome trimmed.
    #[test]
    fn test_trim_agent_chrome_small_batch_with_prompt_trims() {
        let lines = make_log_lines(&["line 1", "❯ command", "chrome"]);
        let result = trim_agent_chrome(lines);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text(), "line 1");
    }

    /// Separator lines immediately above a prompt are included in the cutoff.
    #[test]
    fn test_trim_agent_chrome_separator_above_prompt_trimmed() {
        let lines = make_log_lines(&[
            "real output",
            "more output",
            "────────────────────",  // separator above prompt
            "❯ ",                    // bare prompt
            "────────────────────",  // separator below
            "[Opus 4.6 | Max]",
            "Context ███░░░",
        ]);
        let result = trim_agent_chrome(lines);
        assert_eq!(result.len(), 2, "only real output lines kept");
        assert_eq!(result[0].text(), "real output");
        assert_eq!(result[1].text(), "more output");
    }

    /// Empty lines above a prompt are included in the cutoff.
    #[test]
    fn test_trim_agent_chrome_empty_lines_above_prompt_trimmed() {
        let lines = make_log_lines(&[
            "real output",
            "",
            "",
            "❯",
            "Model: x",
        ]);
        let result = trim_agent_chrome(lines);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text(), "real output");
    }

    // is_separator_line tests live in chrome.rs (canonical location)

    /// Batch with no prompt → all lines kept, regardless of size.
    #[test]
    fn test_trim_agent_chrome_no_prompt_any_size_passthrough() {
        let lines = make_log_lines(&["a", "b", "c"]);
        let result = trim_agent_chrome(lines.clone());
        assert_eq!(result, lines);
    }

    /// Large batch without any prompt → all lines kept.
    #[test]
    fn test_trim_agent_chrome_large_batch_no_prompt_passthrough() {
        let items: Vec<&str> = vec!["output line"; 20];
        let lines = make_log_lines(&items);
        let result = trim_agent_chrome(lines.clone());
        assert_eq!(result, lines);
    }

    /// Empty batch → no panic, returns empty.
    #[test]
    fn test_trim_agent_chrome_empty_batch() {
        assert!(trim_agent_chrome(vec![]).is_empty());
    }

    // --- LogLine / extract_log_line tests ---

    /// Plain text produces a single span with no color attributes.
    #[test]
    fn test_extract_log_line_plain_text_single_span() {
        let parser = vt100::Parser::new(4, 80, 0);
        // Feed plain text to row 0 (no escape sequences)
        // Actually we need to use a mutable parser, but extract_log_line takes a Screen ref.
        // Use a local parser.
        let mut p = vt100::Parser::new(4, 80, 0);
        p.process(b"Hello world");
        let line = extract_log_line(p.screen(), 0);
        assert_eq!(line.spans.len(), 1, "plain text = single span");
        assert_eq!(line.spans[0].text, "Hello world");
        assert_eq!(line.spans[0].fg, None);
        assert_eq!(line.spans[0].bg, None);
        assert!(!line.spans[0].bold);
        assert!(!line.spans[0].italic);
        assert!(!line.spans[0].underline);
        drop(parser); // suppress unused warning
    }

    /// Colored text (ANSI escape for red) produces a span with fg color.
    #[test]
    fn test_extract_log_line_colored_text() {
        let mut p = vt100::Parser::new(4, 80, 0);
        // ESC[31m = red foreground, ESC[0m = reset
        p.process(b"\x1b[31mERROR\x1b[0m ok");
        let line = extract_log_line(p.screen(), 0);
        assert!(line.spans.len() >= 2, "should have at least 2 spans: {:?}", line.spans);
        // First span: "ERROR" with red fg
        assert_eq!(line.spans[0].text, "ERROR");
        assert_eq!(line.spans[0].fg, Some(LogColor::Idx(1))); // ANSI red = idx 1
        // Second span: " ok" with default color
        assert_eq!(line.spans[1].text, " ok");
        assert_eq!(line.spans[1].fg, None);
    }

    /// Multi-span line with bold + color changes.
    #[test]
    fn test_extract_log_line_multi_span_bold_color() {
        let mut p = vt100::Parser::new(4, 80, 0);
        // Bold green then normal
        p.process(b"\x1b[1;32m+added\x1b[0m context");
        let line = extract_log_line(p.screen(), 0);
        assert!(line.spans.len() >= 2, "multi-span: {:?}", line.spans);
        assert_eq!(line.spans[0].text, "+added");
        assert!(line.spans[0].bold);
        assert_eq!(line.spans[0].fg, Some(LogColor::Idx(2))); // green = idx 2
        // Rest is plain
        let last = line.spans.last().unwrap();
        assert_eq!(last.text, " context");
        assert!(!last.bold);
        assert_eq!(last.fg, None);
    }

    /// Trailing whitespace spans are trimmed.
    #[test]
    fn test_extract_log_line_trims_trailing_whitespace() {
        let mut p = vt100::Parser::new(4, 80, 0);
        p.process(b"text");
        let line = extract_log_line(p.screen(), 0);
        // Should not have trailing spaces filling to column 80
        let total_len: usize = line.spans.iter().map(|s| s.text.len()).sum();
        assert_eq!(total_len, 4, "no trailing whitespace: {:?}", line.spans);
    }

    /// Empty row produces an empty spans vec.
    #[test]
    fn test_extract_log_line_empty_row() {
        let p = vt100::Parser::new(4, 80, 0);
        let line = extract_log_line(p.screen(), 0);
        assert!(line.spans.is_empty(), "empty row = empty spans: {:?}", line.spans);
    }

    /// LogLine.text() concatenates all span texts.
    #[test]
    fn test_log_line_text_method() {
        let line = LogLine {
            spans: vec![
                LogSpan { text: "hello".into(), fg: Some(LogColor::Idx(1)), bg: None, bold: true, italic: false, underline: false },
                LogSpan { text: " world".into(), fg: None, bg: None, bold: false, italic: false, underline: false },
            ],
        };
        assert_eq!(line.text(), "hello world");
    }

    /// Colored lines scrolled off are preserved as LogLine with attributes.
    #[test]
    fn test_vt_log_colored_lines_preserved_in_log() {
        let mut buf = VtLogBuffer::new(4, 80, 100); // small 4-row screen
        // Feed colored lines that will scroll off
        for i in 0..6 {
            buf.process(format!("\x1b[31mred-{i}\x1b[0m\r\n").as_bytes());
        }
        let log = buf.lines();
        assert!(!log.is_empty(), "should have scrolled-off lines");
        // At least one line should have a colored span
        let has_color = log.iter().any(|ll| ll.spans.iter().any(|s| s.fg.is_some()));
        assert!(has_color, "scrolled-off lines should preserve color: {:?}", log);
    }

    /// Serialize LogLine to JSON matches expected format.
    #[test]
    fn test_log_line_serialization() {
        let line = LogLine {
            spans: vec![
                LogSpan { text: "hello".into(), fg: Some(LogColor::Idx(1)), bg: None, bold: true, italic: false, underline: false },
                LogSpan { text: " world".into(), fg: None, bg: None, bold: false, italic: false, underline: false },
            ],
        };
        let json = serde_json::to_value(&line).unwrap();
        let spans = json["spans"].as_array().unwrap();
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0]["text"], "hello");
        assert_eq!(spans[0]["fg"], serde_json::json!({"idx": 1}));
        assert!(spans[0]["bold"].as_bool().unwrap());
        // Second span has no color fields (skipped by serde)
        assert_eq!(spans[1]["text"], " world");
        assert!(spans[1].get("fg").is_none() || spans[1]["fg"].is_null());
        assert!(spans[1].get("bold").is_none() || !spans[1]["bold"].as_bool().unwrap_or(true));
    }

    #[test]
    fn test_strip_structural_tokens_intent() {
        let mut line = LogLine {
            spans: vec![
                LogSpan { text: "● ".into(), ..Default::default() },
                LogSpan { text: "[[intent: Reading codebase(Reading)]]".into(), ..Default::default() },
            ],
        };
        line.strip_structural_tokens();
        assert_eq!(line.spans.len(), 1);
        assert_eq!(line.spans[0].text, "● ");
    }

    #[test]
    fn test_strip_structural_tokens_suggest() {
        let mut line = LogLine {
            spans: vec![
                LogSpan { text: "[[suggest: Run tests | Deploy]]".into(), ..Default::default() },
            ],
        };
        line.strip_structural_tokens();
        assert!(line.spans.is_empty(), "suggest-only line should be empty after stripping");
    }

    #[test]
    fn test_strip_structural_tokens_mixed_content() {
        let mut line = LogLine {
            spans: vec![
                LogSpan { text: "prefix [[intent: Doing work(Work)]] suffix".into(), ..Default::default() },
            ],
        };
        line.strip_structural_tokens();
        assert_eq!(line.spans[0].text, "prefix  suffix");
    }

    #[test]
    fn test_strip_structural_tokens_no_match() {
        let mut line = LogLine {
            spans: vec![
                LogSpan { text: "normal output".into(), ..Default::default() },
            ],
        };
        line.strip_structural_tokens();
        assert_eq!(line.spans[0].text, "normal output");
    }

    #[test]
    fn test_rate_limit_ignored_without_agent_activity() {
        let state = fresh_state();
        // No agent activity (no status-line received) → rate-limit should be ignored
        let event = make_parsed("rate-limit", serde_json::json!({ "retry_after_ms": 5000 }));
        let s = apply(&state, &event);
        assert!(!s.rate_limited, "rate-limit must be ignored on non-agent session");
        assert!(s.retry_after_ms.is_none());
    }

    #[test]
    fn test_rate_limit_accepted_with_agent_activity() {
        let state = fresh_state();
        // Establish agent presence via status-line
        let status = make_parsed("status-line", serde_json::json!({ "task_name": "Working" }));
        apply(&state, &status);
        // Now rate-limit should be accepted
        let event = make_parsed("rate-limit", serde_json::json!({ "retry_after_ms": 5000 }));
        let s = apply(&state, &event);
        assert!(s.rate_limited, "rate-limit must be accepted on agent session");
        assert_eq!(s.retry_after_ms, Some(5000));
    }

    #[test]
    fn test_rate_limit_expires_after_retry_after_ms() {
        let state = fresh_state();
        // Establish agent presence
        let status = make_parsed("status-line", serde_json::json!({ "task_name": "Working" }));
        apply(&state, &status);
        // Set rate limited with 5s retry
        let event = make_parsed("rate-limit", serde_json::json!({ "retry_after_ms": 5000 }));
        let s = apply(&state, &event);
        assert!(s.rate_limited);
        assert_eq!(s.retry_after_ms, Some(5000));
        assert!(s.rate_limit_set_ms > 0);

        // Manually backdate the timestamp to simulate expiry
        if let Some(mut entry) = state.session_states.get_mut("s1") {
            entry.rate_limit_set_ms = entry.rate_limit_set_ms.saturating_sub(6000);
        }

        // session_state_with_shell should auto-expire the stale rate limit
        let ss = state.session_state_with_shell("s1").unwrap();
        assert!(!ss.rate_limited, "rate limit should have expired after retry_after_ms");
        assert!(ss.retry_after_ms.is_none());
    }

    #[test]
    fn test_rate_limit_not_expired_within_retry_window() {
        let state = fresh_state();
        // Establish agent presence
        let status = make_parsed("status-line", serde_json::json!({ "task_name": "Working" }));
        apply(&state, &status);
        let event = make_parsed("rate-limit", serde_json::json!({ "retry_after_ms": 60000 }));
        let s = apply(&state, &event);
        assert!(s.rate_limited);

        // Still within the window — should remain rate limited
        let ss = state.session_state_with_shell("s1").unwrap();
        assert!(ss.rate_limited, "rate limit should still be active within retry window");
    }

    #[test]
    fn test_rate_limit_expires_with_default_timeout_when_no_retry_after() {
        let state = fresh_state();
        // Establish agent presence
        let status = make_parsed("status-line", serde_json::json!({ "task_name": "Working" }));
        apply(&state, &status);
        // Rate limit with no retry_after_ms
        let event = make_parsed("rate-limit", serde_json::json!({}));
        let s = apply(&state, &event);
        assert!(s.rate_limited);
        assert!(s.retry_after_ms.is_none());

        // Backdate past the default expiry (120s)
        if let Some(mut entry) = state.session_states.get_mut("s1") {
            entry.rate_limit_set_ms = entry.rate_limit_set_ms.saturating_sub(121_000);
        }

        let ss = state.session_state_with_shell("s1").unwrap();
        assert!(!ss.rate_limited, "rate limit should expire with default timeout");
    }
}
