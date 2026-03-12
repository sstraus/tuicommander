//! Centralized application log ring buffer.
//!
//! Stores structured log entries (level, source, message) in a fixed-capacity
//! circular buffer. The frontend pushes entries via `push_log` and retrieves
//! them via `get_logs`. This is the Rust source-of-truth for the log store;
//! the TypeScript `appLogger` store delegates to these commands.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{Manager, State};

use crate::AppState;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A single log entry stored in the ring buffer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct LogEntry {
    pub id: u64,
    pub timestamp_ms: i64,
    pub level: String,
    pub source: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_json: Option<String>,
    /// How many consecutive duplicate messages were coalesced into this entry.
    /// 0 = first occurrence (no repeats), 1 = seen twice, etc.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub repeat_count: u32,
}

fn is_zero(v: &u32) -> bool {
    *v == 0
}

// ---------------------------------------------------------------------------
// Ring buffer
// ---------------------------------------------------------------------------

pub(crate) const LOG_RING_CAPACITY: usize = 1000;

/// Fixed-capacity circular buffer for structured log entries.
pub(crate) struct LogRingBuffer {
    entries: Vec<Option<LogEntry>>,
    capacity: usize,
    /// Write position (wraps around)
    write_pos: usize,
    /// Number of entries currently stored (≤ capacity)
    count: usize,
    /// Monotonically increasing ID for the next entry
    next_id: u64,
}

impl LogRingBuffer {
    pub(crate) fn new(capacity: usize) -> Self {
        let mut entries = Vec::with_capacity(capacity);
        entries.resize_with(capacity, || None);
        Self {
            entries,
            capacity,
            write_pos: 0,
            count: 0,
            next_id: 1,
        }
    }

    /// Push a new entry into the ring buffer. Returns the assigned entry ID.
    ///
    /// If the most recent entry has the same level+source+message, the new push
    /// is coalesced: the existing entry's timestamp and repeat_count are updated
    /// instead of allocating a new slot. This prevents identical recurring
    /// warnings (e.g. polling failures) from flooding the ring buffer.
    pub(crate) fn push(
        &mut self,
        level: String,
        source: String,
        message: String,
        data_json: Option<String>,
    ) -> u64 {
        // Dedup: coalesce with the most recent entry if level+source+message match
        if self.count > 0 {
            let last_idx = if self.write_pos == 0 {
                self.capacity - 1
            } else {
                self.write_pos - 1
            };
            if let Some(last) = &mut self.entries[last_idx]
                && last.level == level
                && last.source == source
                && last.message == message
            {
                last.repeat_count += 1;
                last.timestamp_ms = chrono::Utc::now().timestamp_millis();
                return last.id;
            }
        }

        let id = self.next_id;
        self.next_id += 1;

        let timestamp_ms = chrono::Utc::now().timestamp_millis();

        let entry = LogEntry {
            id,
            timestamp_ms,
            level,
            source,
            message,
            data_json,
            repeat_count: 0,
        };

        self.entries[self.write_pos] = Some(entry);
        self.write_pos = (self.write_pos + 1) % self.capacity;
        if self.count < self.capacity {
            self.count += 1;
        }

        id
    }

    /// Return entries in chronological order (oldest first), up to `limit`.
    /// If `limit` is 0, returns all entries.
    pub(crate) fn get_entries(&self, limit: usize) -> Vec<LogEntry> {
        if self.count == 0 {
            return Vec::new();
        }

        let effective_limit = if limit == 0 { self.count } else { limit.min(self.count) };

        // Start index: oldest entry in the buffer
        let start = if self.count < self.capacity {
            0
        } else {
            self.write_pos // write_pos points to the oldest when buffer is full
        };

        // We want the *last* `effective_limit` entries (most recent)
        let skip = self.count - effective_limit;
        let mut result = Vec::with_capacity(effective_limit);
        for i in skip..self.count {
            let idx = (start + i) % self.capacity;
            if let Some(entry) = &self.entries[idx] {
                result.push(entry.clone());
            }
        }

        result
    }

    /// Remove all entries.
    pub(crate) fn clear(&mut self) {
        for slot in self.entries.iter_mut() {
            *slot = None;
        }
        self.write_pos = 0;
        self.count = 0;
        // Keep next_id monotonic — don't reset
    }

    /// Current number of entries in the buffer.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.count
    }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Push a log entry from the frontend into the Rust ring buffer.
#[tauri::command]
pub(crate) fn push_log(
    state: State<'_, Arc<AppState>>,
    level: String,
    source: String,
    message: String,
    data_json: Option<String>,
) {
    let mut buf = state.log_buffer.lock();
    buf.push(level, source, message, data_json);
}

/// Retrieve log entries. Returns up to `limit` most recent entries (0 = all).
#[tauri::command]
pub(crate) fn get_logs(
    state: State<'_, Arc<AppState>>,
    limit: Option<usize>,
) -> Vec<LogEntry> {
    let buf = state.log_buffer.lock();
    buf.get_entries(limit.unwrap_or(0))
}

/// Clear all log entries.
#[tauri::command]
pub(crate) fn clear_logs(state: State<'_, Arc<AppState>>) {
    let mut buf = state.log_buffer.lock();
    buf.clear();
}

/// Push a log entry from internal Rust code using an AppHandle.
///
/// Use this in contexts where you have an `AppHandle` but not a `State<>` extractor
/// (e.g. watcher callbacks, plugin lifecycle hooks). Falls back silently if state
/// is not yet initialised.
pub(crate) fn log_via_handle(
    handle: &tauri::AppHandle,
    level: &str,
    source: &str,
    message: &str,
) {
    let state = handle.state::<Arc<AppState>>();
    let mut buf = state.log_buffer.lock();
    buf.push(level.to_string(), source.to_string(), message.to_string(), None);
}

/// Push a log entry from internal Rust code using an AppState reference directly.
pub(crate) fn log_via_state(
    state: &Arc<AppState>,
    level: &str,
    source: &str,
    message: &str,
) {
    let mut buf = state.log_buffer.lock();
    buf.push(level.to_string(), source.to_string(), message.to_string(), None);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_assigns_monotonic_ids() {
        let mut buf = LogRingBuffer::new(10);
        let id1 = buf.push("info".into(), "app".into(), "first".into(), None);
        let id2 = buf.push("warn".into(), "git".into(), "second".into(), None);
        let id3 = buf.push("error".into(), "app".into(), "third".into(), None);

        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
        assert_eq!(id3, 3);
    }

    #[test]
    fn get_entries_returns_chronological_order() {
        let mut buf = LogRingBuffer::new(10);
        buf.push("info".into(), "app".into(), "first".into(), None);
        buf.push("warn".into(), "app".into(), "second".into(), None);
        buf.push("error".into(), "app".into(), "third".into(), None);

        let entries = buf.get_entries(0);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].message, "first");
        assert_eq!(entries[1].message, "second");
        assert_eq!(entries[2].message, "third");
    }

    #[test]
    fn get_entries_with_limit() {
        let mut buf = LogRingBuffer::new(10);
        buf.push("info".into(), "app".into(), "a".into(), None);
        buf.push("info".into(), "app".into(), "b".into(), None);
        buf.push("info".into(), "app".into(), "c".into(), None);

        let entries = buf.get_entries(2);
        assert_eq!(entries.len(), 2);
        // Should return the 2 most recent
        assert_eq!(entries[0].message, "b");
        assert_eq!(entries[1].message, "c");
    }

    #[test]
    fn ring_buffer_wraps_and_drops_oldest() {
        let mut buf = LogRingBuffer::new(3);
        buf.push("info".into(), "app".into(), "a".into(), None);
        buf.push("info".into(), "app".into(), "b".into(), None);
        buf.push("info".into(), "app".into(), "c".into(), None);
        // Buffer is full, next push drops "a"
        buf.push("info".into(), "app".into(), "d".into(), None);

        assert_eq!(buf.len(), 3);
        let entries = buf.get_entries(0);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].message, "b");
        assert_eq!(entries[1].message, "c");
        assert_eq!(entries[2].message, "d");
    }

    #[test]
    fn ring_buffer_wraps_multiple_times() {
        let mut buf = LogRingBuffer::new(3);
        for i in 0..10 {
            buf.push("info".into(), "app".into(), format!("msg-{}", i), None);
        }

        assert_eq!(buf.len(), 3);
        let entries = buf.get_entries(0);
        assert_eq!(entries[0].message, "msg-7");
        assert_eq!(entries[1].message, "msg-8");
        assert_eq!(entries[2].message, "msg-9");
    }

    #[test]
    fn clear_removes_all_entries_but_keeps_next_id() {
        let mut buf = LogRingBuffer::new(10);
        buf.push("info".into(), "app".into(), "a".into(), None);
        buf.push("info".into(), "app".into(), "b".into(), None);
        assert_eq!(buf.len(), 2);

        buf.clear();
        assert_eq!(buf.len(), 0);
        assert_eq!(buf.get_entries(0).len(), 0);

        // Next push should get id=3 (not 1)
        let id = buf.push("info".into(), "app".into(), "after-clear".into(), None);
        assert_eq!(id, 3);
    }

    #[test]
    fn empty_buffer_returns_empty() {
        let buf = LogRingBuffer::new(10);
        assert_eq!(buf.len(), 0);
        assert_eq!(buf.get_entries(0).len(), 0);
        assert_eq!(buf.get_entries(5).len(), 0);
    }

    #[test]
    fn data_json_is_preserved() {
        let mut buf = LogRingBuffer::new(10);
        buf.push(
            "error".into(),
            "network".into(),
            "request failed".into(),
            Some(r#"{"status":500}"#.into()),
        );

        let entries = buf.get_entries(0);
        assert_eq!(entries[0].data_json.as_deref(), Some(r#"{"status":500}"#));
    }

    #[test]
    fn limit_larger_than_count() {
        let mut buf = LogRingBuffer::new(10);
        buf.push("info".into(), "app".into(), "only-one".into(), None);

        let entries = buf.get_entries(100);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].message, "only-one");
    }

    #[test]
    fn entry_fields_are_correct() {
        let mut buf = LogRingBuffer::new(10);
        buf.push("warn".into(), "github".into(), "rate limited".into(), None);

        let entry = &buf.get_entries(0)[0];
        assert_eq!(entry.id, 1);
        assert_eq!(entry.level, "warn");
        assert_eq!(entry.source, "github");
        assert_eq!(entry.message, "rate limited");
        assert!(entry.timestamp_ms > 0);
        assert!(entry.data_json.is_none());
    }

    // ---- Deduplication tests ----

    #[test]
    fn dedup_identical_consecutive_messages() {
        let mut buf = LogRingBuffer::new(10);
        buf.push("warn".into(), "github".into(), "poll failed".into(), None);
        buf.push("warn".into(), "github".into(), "poll failed".into(), None);
        buf.push("warn".into(), "github".into(), "poll failed".into(), None);

        // Should coalesce into a single entry with repeat_count=2
        assert_eq!(buf.len(), 1);
        let entries = buf.get_entries(0);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].message, "poll failed");
        assert_eq!(entries[0].repeat_count, 2);
    }

    #[test]
    fn dedup_resets_on_different_message() {
        let mut buf = LogRingBuffer::new(10);
        buf.push("warn".into(), "github".into(), "poll failed".into(), None);
        buf.push("warn".into(), "github".into(), "poll failed".into(), None);
        buf.push("info".into(), "app".into(), "something else".into(), None);
        buf.push("warn".into(), "github".into(), "poll failed".into(), None);

        // 3 entries: first (repeat_count=1), different, second occurrence
        assert_eq!(buf.len(), 3);
        let entries = buf.get_entries(0);
        assert_eq!(entries[0].repeat_count, 1);
        assert_eq!(entries[1].repeat_count, 0);
        assert_eq!(entries[2].repeat_count, 0);
    }

    #[test]
    fn dedup_requires_matching_level_source_message() {
        let mut buf = LogRingBuffer::new(10);
        buf.push("warn".into(), "github".into(), "poll failed".into(), None);
        // Same message but different source — no dedup
        buf.push("warn".into(), "git".into(), "poll failed".into(), None);
        // Same source+message but different level — no dedup
        buf.push("error".into(), "github".into(), "poll failed".into(), None);

        assert_eq!(buf.len(), 3);
    }

    #[test]
    fn dedup_updates_timestamp() {
        let mut buf = LogRingBuffer::new(10);
        buf.push("warn".into(), "github".into(), "poll failed".into(), None);
        let ts1 = buf.get_entries(0)[0].timestamp_ms;

        // Small delay to ensure different timestamp
        std::thread::sleep(std::time::Duration::from_millis(2));
        buf.push("warn".into(), "github".into(), "poll failed".into(), None);

        let entries = buf.get_entries(0);
        assert_eq!(entries.len(), 1);
        assert!(entries[0].timestamp_ms >= ts1);
    }

    #[test]
    fn dedup_preserves_original_id() {
        let mut buf = LogRingBuffer::new(10);
        let id1 = buf.push("warn".into(), "github".into(), "poll failed".into(), None);
        let id2 = buf.push("warn".into(), "github".into(), "poll failed".into(), None);

        // Both return same id since it's coalesced
        assert_eq!(id1, id2);
        let entries = buf.get_entries(0);
        assert_eq!(entries[0].id, id1);
    }

    #[test]
    fn dedup_works_at_buffer_boundary() {
        let mut buf = LogRingBuffer::new(3);
        buf.push("info".into(), "app".into(), "a".into(), None);
        buf.push("info".into(), "app".into(), "b".into(), None);
        buf.push("info".into(), "app".into(), "c".into(), None);
        // Buffer full. Now push duplicate of last — should dedup, not wrap
        buf.push("info".into(), "app".into(), "c".into(), None);

        assert_eq!(buf.len(), 3);
        let entries = buf.get_entries(0);
        assert_eq!(entries[2].message, "c");
        assert_eq!(entries[2].repeat_count, 1);
    }
}
