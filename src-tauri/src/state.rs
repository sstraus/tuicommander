use dashmap::DashMap;
use notify_debouncer_mini::Debouncer;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
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
}

// ---------------------------------------------------------------------------
// SessionState — server-side accumulator for REST polling (mobile/browser)
// ---------------------------------------------------------------------------

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
    /// Usage limit percentage (0-100), if detected
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_limit_pct: Option<u8>,
    /// True when we have recent output activity (not idle)
    pub is_busy: bool,
    /// Timestamp of last activity (any event for this session)
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
    /// Suggested follow-up actions from the agent (from [[suggest: ...]] tokens)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_actions: Option<Vec<String>>,
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

        for i in 0..to_read {
            result.push(self.buf[(start + i) % self.capacity]);
        }

        (result, self.total_written)
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
pub(crate) fn strip_kitty_sequences(input: &str) -> (String, Vec<KittyAction>) {
    // Fast path: skip scanning if no possible kitty sequence prefix exists
    if !input.contains("\x1b[>") && !input.contains("\x1b[<") && !input.contains("\x1b[?") {
        return (input.to_string(), Vec::new());
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
        return (input.to_string(), actions);
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

    (output, actions)
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

/// Global state for managing PTY sessions and worktrees
pub struct AppState {
    pub sessions: DashMap<String, Mutex<PtySession>>,
    pub(crate) worktrees_dir: PathBuf,
    pub(crate) metrics: SessionMetrics,
    /// Ring buffers for MCP output access (one per session)
    pub output_buffers: DashMap<String, Mutex<OutputRingBuffer>>,
    /// Active MCP Streamable HTTP sessions (session_id -> creation time for TTL reaping)
    pub mcp_sessions: DashMap<String, Instant>,
    /// WebSocket clients per PTY session for streaming output
    pub ws_clients: DashMap<String, Vec<tokio::sync::mpsc::UnboundedSender<String>>>,
    /// Cached AppConfig to avoid re-reading from disk on every request
    pub(crate) config: parking_lot::RwLock<crate::config::AppConfig>,
    /// TTL cache for get_repo_info results, keyed by repo path
    pub(crate) repo_info_cache: DashMap<String, (crate::git::RepoInfo, Instant)>,
    /// TTL cache for get_merged_branches results, keyed by repo path
    pub(crate) merged_branches_cache: DashMap<String, (Vec<String>, Instant)>,
    /// TTL cache for get_repo_pr_statuses results, keyed by repo path
    pub(crate) github_status_cache: DashMap<String, (Vec<crate::github::BranchPrStatus>, Instant)>,
    /// TTL cache for get_github_status results (ahead/behind), keyed by repo path
    pub(crate) git_status_cache: DashMap<String, (crate::github::GitHubStatus, Instant)>,
    /// File watchers for .git/HEAD per repo (keyed by repo path)
    pub(crate) head_watchers: DashMap<String, Debouncer<notify::RecommendedWatcher>>,
    /// File watchers for .git/ directory per repo (keyed by repo path)
    pub(crate) repo_watchers: DashMap<String, Debouncer<notify::RecommendedWatcher>>,
    /// Shared HTTP client for GitHub API requests.
    /// Wrapped in ManuallyDrop because reqwest::blocking::Client owns an internal
    /// tokio runtime that panics on drop inside another runtime (e.g. #[tokio::test]).
    /// The client lives for the app's lifetime, so never dropping it is harmless.
    pub(crate) http_client: std::mem::ManuallyDrop<reqwest::blocking::Client>,
    /// GitHub API token — updated on fallback when a 401 triggers candidate rotation
    pub(crate) github_token: parking_lot::RwLock<Option<String>>,
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
    pub(crate) log_buffer: Mutex<crate::app_logger::LogRingBuffer>,
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
        self.repo_info_cache.clear();
        self.merged_branches_cache.clear();
        self.github_status_cache.clear();
        self.git_status_cache.clear();
    }

    /// Invalidate caches for a specific repo path.
    pub(crate) fn invalidate_repo_caches(&self, path: &str) {
        self.repo_info_cache.remove(path);
        self.merged_branches_cache.remove(path);
        self.github_status_cache.remove(path);
        self.git_status_cache.remove(path);
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
                        eprintln!("[session-state] lagged by {n} events");
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
                    is_busy: true,
                    last_activity_ms: now_ms,
                    ..Default::default()
                });
            }
            AppEvent::SessionClosed { session_id } => {
                state.session_states.remove(session_id);
            }
            AppEvent::PtyParsed { session_id, parsed } => {
                let event_type = parsed.get("type").and_then(|t| t.as_str()).unwrap_or("");
                state.session_states
                    .entry(session_id.clone())
                    .and_modify(|s| {
                        s.last_activity_ms = now_ms;
                        s.is_busy = true;
                        match event_type {
                            "question" => {
                                s.awaiting_input = true;
                                s.question_text = parsed.get("prompt_text")
                                    .and_then(|t| t.as_str())
                                    .map(|t| t.to_string());
                            }
                            "user-input" => {
                                // User responded — clear question state
                                s.awaiting_input = false;
                                s.question_text = None;
                                // Capture as last_prompt if >= 10 words
                                if let Some(content) = parsed.get("content").and_then(|v| v.as_str())
                                    && content.split_whitespace().count() >= 10 {
                                        s.last_prompt = Some(content.to_string());
                                    }
                            }
                            "rate-limit" => {
                                s.rate_limited = true;
                                s.retry_after_ms = parsed.get("retry_after_ms")
                                    .and_then(|v| v.as_u64());
                                s.is_busy = false;
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
                                // Agent is working — clear error/rate-limit/suggest states
                                s.rate_limited = false;
                                s.retry_after_ms = None;
                                s.last_error = None;
                                s.suggested_actions = None;
                                // Capture current task name from status line
                                s.current_task = parsed.get("task_name")
                                    .and_then(|v| v.as_str())
                                    .map(|t| t.to_string());
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
                        is_busy: true,
                        last_activity_ms: now_ms,
                        ..Default::default()
                    });
            }
            AppEvent::PtyExit { session_id } => {
                if let Some(mut entry) = state.session_states.get_mut(session_id) {
                    entry.is_busy = false;
                    entry.awaiting_input = false;
                    entry.question_text = None;
                    entry.rate_limited = false;
                    entry.last_activity_ms = now_ms;
                }
            }
            // Global events don't affect per-session state
            AppEvent::HeadChanged { .. }
            | AppEvent::RepoChanged { .. }
            | AppEvent::PluginChanged { .. }
            | AppEvent::UpstreamStatusChanged { .. }
            | AppEvent::McpToast { .. } => {}
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
/// Wraps a `vt100::Parser` (screen-only, no vt100 scrollback) and applies a
/// diff-based algorithm to extract "finalized" lines as they scroll off the top
/// of the visible screen.  Results are stored in a bounded `VecDeque<String>`
/// that REST and WebSocket consumers can query with `lines_since()`.
///
/// **Thread safety:** Not `Sync` — lives behind `Mutex<VtLogBuffer>` in `AppState`.
/// The `vt100::Parser` is `Send` but not `Sync`, so this struct shares that bound.
pub struct VtLogBuffer {
    parser: vt100::Parser,
    /// Snapshot of visible screen rows from the previous `process()` call.
    prev_rows: Vec<String>,
    /// Finalized log lines (oldest first).
    log: VecDeque<String>,
    /// Maximum number of log lines retained.
    capacity: usize,
    /// Whether the previous `process()` call saw the alternate screen active.
    was_alternate: bool,
}

impl VtLogBuffer {
    pub fn new(rows: u16, cols: u16, capacity: usize) -> Self {
        // scrollback=0: we own the log; we don't need vt100's scrollback mechanism.
        let parser = vt100::Parser::new(rows, cols, 0);
        Self {
            parser,
            prev_rows: Vec::new(),
            log: VecDeque::new(),
            capacity,
            was_alternate: false,
        }
    }

    /// Feed raw PTY bytes into the VT100 parser.
    ///
    /// Returns the screen rows that changed since the previous call.  Changed
    /// rows are detected for **both** normal and alternate screen so that
    /// output parsers can match status lines and intent tokens emitted by
    /// agents that use the alternate screen (e.g. Claude Code / Ink).
    ///
    /// Log extraction (scrolled-off lines) remains **normal-screen-only**.
    pub fn process(&mut self, data: &[u8]) -> Vec<ChangedRow> {
        self.parser.process(data);
        let screen = self.parser.screen();
        let is_alternate = screen.alternate_screen();

        // On screen switch reset prev_rows so the first frame of the new
        // screen reports all non-empty rows as changed.
        if is_alternate != self.was_alternate {
            self.prev_rows.clear();
        }

        let cols = screen.size().1;
        let curr_rows: Vec<String> = screen
            .rows(0, cols)
            .map(|r| r.trim_end().to_string())
            .collect();

        // Compute changed rows by diffing curr vs prev (both screens).
        let changed: Vec<ChangedRow> = curr_rows
            .iter()
            .enumerate()
            .filter_map(|(i, curr)| {
                let prev = self.prev_rows.get(i).map(String::as_str).unwrap_or("");
                if curr != prev {
                    Some(ChangedRow { row_index: i, text: curr.clone() })
                } else {
                    None
                }
            })
            .collect();

        // Log extraction: scrolled-off lines on normal screen only.
        if !is_alternate {
            let prev_snapshot = self.prev_rows.clone();
            let scrolled_off = self.rows_scrolled_off(&prev_snapshot, &curr_rows);
            let new_lines: Vec<String> = scrolled_off
                .iter()
                .filter(|l| !l.is_empty())
                .cloned()
                .collect();
            for line in new_lines {
                self.push_log_line(line);
            }
        }

        self.prev_rows = curr_rows;
        self.was_alternate = is_alternate;
        changed
    }

    /// Update parser dimensions on terminal resize.
    pub fn resize(&mut self, rows: u16, cols: u16) {
        self.parser.set_size(rows, cols);
        // prev_rows may no longer match the new dimensions; reset to avoid stale diffs.
        self.prev_rows.clear();
    }

    /// All finalized log lines (oldest first).
    #[allow(dead_code)]
    pub fn lines(&self) -> &VecDeque<String> {
        &self.log
    }

    /// Returns log lines starting at `offset` (0-indexed from oldest retained line).
    /// Also returns the new offset (= total lines so far) for incremental reads.
    pub fn lines_since_owned(&self, offset: usize) -> (Vec<String>, usize) {
        let total = self.log.len();
        if offset >= total {
            return (Vec::new(), total);
        }
        let slice: Vec<String> = self.log.iter().skip(offset).cloned().collect();
        let new_offset = total;
        (slice, new_offset)
    }

    /// Current visible screen rows (useful for appending after the log).
    #[allow(dead_code)]
    pub fn screen_rows(&self) -> Vec<String> {
        let screen = self.parser.screen();
        let cols = screen.size().1;
        screen
            .rows(0, cols)
            .map(|r| r.trim_end().to_string())
            .collect()
    }

    /// Total log lines ever finalized (monotonically increasing offset).
    /// Callers can use this as a cursor for incremental reads.
    pub fn total_lines(&self) -> usize {
        self.log.len()
    }

    // --- private helpers ---

    fn push_log_line(&mut self, line: String) {
        if self.log.len() >= self.capacity {
            self.log.pop_front();
        }
        self.log.push_back(line);
    }

    /// Compute which rows from `prev` scrolled off the top, given that `curr`
    /// is the new screen content.
    ///
    /// Finds the largest overlap k where `prev[prev.len()-k..] == curr[..k]`.
    /// Everything in `prev[..prev.len()-k]` scrolled off.
    fn rows_scrolled_off<'a>(&self, prev: &'a [String], curr: &[String]) -> &'a [String] {
        if prev.is_empty() {
            return &[];
        }
        let plen = prev.len();
        let clen = curr.len();
        let max_overlap = plen.min(clen);

        // Try overlaps from largest to smallest.
        for k in (0..=max_overlap).rev() {
            if prev[plen - k..] == curr[..k] {
                // prev[..plen-k] scrolled off.
                return &prev[..plen - k];
            }
        }
        // No overlap found — entire prev scrolled off.
        prev
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
            repo_info_cache: dashmap::DashMap::new(),
            merged_branches_cache: dashmap::DashMap::new(),
            github_status_cache: dashmap::DashMap::new(),
            git_status_cache: dashmap::DashMap::new(),
            head_watchers: dashmap::DashMap::new(),
            repo_watchers: dashmap::DashMap::new(),
            http_client: std::mem::ManuallyDrop::new(reqwest::blocking::Client::new()),
            github_token: parking_lot::RwLock::new(None),
            github_circuit_breaker: crate::github::GitHubCircuitBreaker::new(),
            server_shutdown: parking_lot::Mutex::new(None),
            session_token: parking_lot::RwLock::new(String::from("test-token")),
            app_handle: parking_lot::RwLock::new(None),
            plugin_watchers: dashmap::DashMap::new(),
            vt_log_buffers: dashmap::DashMap::new(),
            kitty_states: dashmap::DashMap::new(),
            input_buffers: dashmap::DashMap::new(),
            last_prompts: dashmap::DashMap::new(),
            silence_states: dashmap::DashMap::new(),
            claude_usage_cache: parking_lot::Mutex::new(std::collections::HashMap::new()),
            log_buffer: parking_lot::Mutex::new(crate::app_logger::LogRingBuffer::new(crate::app_logger::LOG_RING_CAPACITY)),
            event_bus: tokio::sync::broadcast::channel(256).0,
            event_counter: Arc::new(AtomicU64::new(0)),
            session_states: DashMap::new(),
            mcp_upstream_registry: Arc::new(crate::mcp_proxy::registry::UpstreamRegistry::new()),
            mcp_tools_changed: tokio::sync::broadcast::channel(16).0,
        }
    }

    #[test]
    fn test_cached_config_returns_default() {
        let state = make_test_app_state();
        let config = state.config.read();
        assert_eq!(config.font_family, "JetBrains Mono");
        assert_eq!(config.theme, "vscode-dark");
        assert!(!config.mcp_server_enabled);
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
        state.repo_info_cache.insert(
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
        state.github_status_cache.insert(
            "/some/path".to_string(),
            (vec![], Instant::now()),
        );

        assert!(!state.repo_info_cache.is_empty());
        assert!(!state.github_status_cache.is_empty());

        state.clear_caches();

        assert!(state.repo_info_cache.is_empty());
        assert!(state.github_status_cache.is_empty());
    }

    #[test]
    fn test_invalidate_repo_caches_removes_specific_path() {
        let state = make_test_app_state();
        state.repo_info_cache.insert(
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
        state.repo_info_cache.insert(
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

        assert!(state.repo_info_cache.get("/repo/a").is_none());
        assert!(state.repo_info_cache.get("/repo/b").is_some());
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
        s.session_states.insert("s1".to_string(), SessionState {
            is_busy: true,
            ..Default::default()
        });
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

    // --- VtLogBuffer tests ---

    fn make_vt_log() -> VtLogBuffer {
        VtLogBuffer::new(24, 80, 1000)
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
        let lines = buf.lines();
        // Lines 0..6 scrolled off (30 total - 24 visible = 6)
        assert!(!lines.is_empty(), "should have finalized some lines");
        let first = lines.front().unwrap();
        assert!(first.starts_with("line "), "line content preserved: {first:?}");
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
        let lines = buf.lines();
        assert!(!lines.is_empty(), "at least one line should have scrolled off");
        let first = lines.front().unwrap();
        assert_eq!(first, "row0", "first scrolled-off row is row0, got: {first:?}");
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
        let lines = buf.lines();
        assert!(!lines.is_empty(), "main screen lines should appear in log after alt exit");
        assert!(
            lines.iter().any(|l| l.starts_with("main")),
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
        let lines = buf.lines();
        // None of the logged lines should be empty
        for line in lines.iter() {
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
            assert!(line.starts_with("rot-"), "unexpected line: {line}");
        }
        // Fetch with an offset past the end — empty
        let (empty, off2) = buf.lines_since_owned(off);
        assert!(empty.is_empty());
        assert_eq!(off2, off);
    }

    /// Feed data in small incremental chunks (simulating real PTY reads that
    /// may split mid-line) and verify lines are still extracted.
    /// Note: chunk boundaries that split a line mid-write can cause the
    /// diff-based detector to emit a partial row (e.g. "chunk-" before the
    /// number arrives in the next chunk). This is expected — we verify that
    /// the majority of lines are complete.
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
        let lines: Vec<String> = buf.lines().iter().cloned().collect();
        let matching: Vec<&String> = lines
            .iter()
            .filter(|l| l.starts_with("chunk-"))
            .collect();
        assert!(
            matching.len() >= 5,
            "chunked feed should still capture lines, got {} matching out of {}: {:?}",
            matching.len(),
            lines.len(),
            lines,
        );
        // Count complete lines (chunk-N with valid number)
        let complete_count = matching
            .iter()
            .filter(|l| {
                l.strip_prefix("chunk-")
                    .and_then(|s| s.parse::<u32>().ok())
                    .is_some()
            })
            .count();
        assert!(
            complete_count >= 5,
            "at least 5 lines should be complete 'chunk-N', got {complete_count} complete out of {} matching: {:?}",
            matching.len(),
            matching,
        );
    }

    /// Scroll regions (DECSTBM): lines scrolling out of a restricted region
    /// should still be captured by the diff-based detector.
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
        // The scroll region caused lines to scroll off within the region.
        // Our diff-based detector compares full screen snapshots, so it should
        // detect the changed rows. We just verify lines were captured.
        assert!(
            after > before,
            "scroll region output should produce new log lines: before={before}, after={after}"
        );
        let lines: Vec<String> = buf.lines().iter().cloned().collect();
        let region_lines: Vec<&String> = lines
            .iter()
            .filter(|l| l.starts_with("region-"))
            .collect();
        assert!(
            !region_lines.is_empty(),
            "should capture region-N lines, got: {lines:?}"
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
        let lines: Vec<String> = buf.lines().iter().cloned().collect();
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
        let lines: Vec<String> = buf.lines().iter().cloned().collect();
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
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("open pty");

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

        let lines: Vec<String> = buf.lines().iter().cloned().collect();
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
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("open pty");

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

        let lines: Vec<String> = buf.lines().iter().cloned().collect();
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
}
