//! Centralized application log ring buffer.
//!
//! Stores structured log entries (level, source, message) in a fixed-capacity
//! circular buffer. The frontend pushes entries via `push_log` and retrieves
//! them via `get_logs`. This is the Rust source-of-truth for the log store;
//! the TypeScript `appLogger` store delegates to these commands.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

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
    pub(crate) fn push(
        &mut self,
        level: String,
        source: String,
        message: String,
        data_json: Option<String>,
    ) -> u64 {
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
}
