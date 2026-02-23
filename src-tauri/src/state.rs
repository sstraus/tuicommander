use dashmap::DashMap;
use notify_debouncer_mini::Debouncer;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::AppHandle;

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
    /// Active MCP Streamable HTTP sessions (session_id -> unit)
    pub mcp_sessions: DashMap<String, ()>,
    /// WebSocket clients per PTY session for streaming output
    pub ws_clients: DashMap<String, Vec<tokio::sync::mpsc::UnboundedSender<String>>>,
    /// Cached AppConfig to avoid re-reading from disk on every request
    pub(crate) config: parking_lot::RwLock<crate::config::AppConfig>,
    /// TTL cache for get_repo_info results, keyed by repo path
    pub(crate) repo_info_cache: DashMap<String, (crate::git::RepoInfo, Instant)>,
    /// TTL cache for get_repo_pr_statuses results, keyed by repo path
    pub(crate) github_status_cache: DashMap<String, (Vec<crate::github::BranchPrStatus>, Instant)>,
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
    /// Per-session kitty keyboard protocol state (session_id → state).
    /// Separate DashMap (not inside PtySession) to avoid writer contention.
    pub(crate) kitty_states: DashMap<String, Mutex<KittyKeyboardState>>,
    /// Incremental cache for Claude session transcript parsing.
    /// Loaded from disk on startup, persisted after each scan.
    pub(crate) claude_usage_cache: Mutex<crate::claude_usage::SessionStatsCache>,
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
        self.github_status_cache.clear();
    }

    /// Invalidate caches for a specific repo path.
    pub(crate) fn invalidate_repo_caches(&self, path: &str) {
        self.repo_info_cache.remove(path);
        self.github_status_cache.remove(path);
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
            github_status_cache: dashmap::DashMap::new(),
            head_watchers: dashmap::DashMap::new(),
            repo_watchers: dashmap::DashMap::new(),
            http_client: std::mem::ManuallyDrop::new(reqwest::blocking::Client::new()),
            github_token: parking_lot::RwLock::new(None),
            github_circuit_breaker: crate::github::GitHubCircuitBreaker::new(),
            server_shutdown: parking_lot::Mutex::new(None),
            session_token: parking_lot::RwLock::new(String::from("test-token")),
            app_handle: parking_lot::RwLock::new(None),
            plugin_watchers: dashmap::DashMap::new(),
            kitty_states: dashmap::DashMap::new(),
            claude_usage_cache: parking_lot::Mutex::new(std::collections::HashMap::new()),
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
}
