//! The chat view of a Claude terminal: its transcript, tailed and projected.
//!
//! The grid is never read. The bound transcript file is tailed by byte offset
//! (`transcript_tail`), each row goes through the Claude adapter, and the
//! resulting ACP updates land in a bounded per-terminal log. A client asks for
//! the log from the sequence it last saw; if the ring no longer holds that
//! sequence, or the conversation changed, it gets the whole retained log marked
//! `reset` and starts over. Nothing is ever lost silently.
//!
//! While a client reads, a ticker re-reads the file once a second and wakes
//! clients with `chat-view-changed`; when nobody has read for `VIEWER_IDLE` the
//! ticker stops and the log is dropped.

pub(crate) mod claude;

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::Serialize;
use serde_json::Value;

use crate::state::AppState;
use crate::transcript_tail::{read_appended, read_last_window};
use claude::ClaudeAdapter;

/// How much of the end of a transcript is read on first attach. The opt-in
/// `measurement` test measures this and a full parse against a real disk file.
const TAIL_WINDOW_BYTES: u64 = 2 * 1024 * 1024;
const MAX_LOG_ENTRIES: usize = 2_000;
const MAX_LOG_BYTES: usize = 4 * 1024 * 1024;
const POLL: Duration = Duration::from_secs(1);
/// A view nobody has read for this long is dropped.
const VIEWER_IDLE: Duration = Duration::from_secs(20);
/// How often the transcript binding is re-checked. `/clear` and `/resume` start
/// a new session file, and Claude exits.
const REBIND_EVERY: Duration = Duration::from_secs(10);

/// Source of epochs. A view dropped by the idle ticker and created again for
/// the same terminal must not share an epoch with the dropped one, or a client
/// holding the old cursor would read a slice of a different window.
static NEXT_EPOCH: AtomicU64 = AtomicU64::new(0);

/// Bounded log of ACP updates with a monotonic sequence.
struct ViewLog {
    /// Bumped when the conversation changes (new file, shrunk file), so a
    /// client never mixes two conversations.
    epoch: u64,
    /// Sequence of the oldest retained entry.
    first_seq: u64,
    next_seq: u64,
    entries: VecDeque<(u64, Value, usize)>,
    bytes: usize,
    max_entries: usize,
    max_bytes: usize,
}

impl ViewLog {
    fn new(max_entries: usize, max_bytes: usize) -> Self {
        Self {
            epoch: NEXT_EPOCH.fetch_add(1, Ordering::Relaxed),
            first_seq: 0,
            next_seq: 0,
            entries: VecDeque::new(),
            bytes: 0,
            max_entries,
            max_bytes,
        }
    }

    fn push(&mut self, update: Value) {
        let size = update.to_string().len();
        self.entries.push_back((self.next_seq, update, size));
        self.next_seq += 1;
        self.bytes += size;
        while self.entries.len() > self.max_entries
            || (self.bytes > self.max_bytes && self.entries.len() > 1)
        {
            if let Some((_, _, size)) = self.entries.pop_front() {
                self.bytes -= size;
            }
        }
        self.first_seq = self.entries.front().map_or(self.next_seq, |e| e.0);
    }

    /// A different conversation. Sequences keep counting: they are only
    /// comparable within an epoch, and never reused.
    fn reset(&mut self) {
        self.epoch = NEXT_EPOCH.fetch_add(1, Ordering::Relaxed);
        self.entries.clear();
        self.bytes = 0;
        self.first_seq = self.next_seq;
    }

    /// Entries from `from_seq` for a client that last saw `epoch`. A client on
    /// another epoch, or behind the ring, gets everything retained and `reset`.
    fn since(&self, epoch: Option<u64>, from_seq: u64) -> (bool, Vec<Value>) {
        let behind = from_seq < self.first_seq;
        let reset = epoch != Some(self.epoch) || behind;
        let from = if reset { self.first_seq } else { from_seq };
        let updates = self
            .entries
            .iter()
            .filter(|(seq, _, _)| *seq >= from)
            .map(|(_, update, _)| update.clone())
            .collect();
        (reset, updates)
    }
}

/// One terminal's tail.
struct View {
    path: PathBuf,
    offset: u64,
    /// The first window has been read; later reads are plain appends.
    attached: bool,
    /// Keep the old file open so its identity cannot be reused after replacement.
    file_identity: Option<same_file::Handle>,
    adapter: ClaudeAdapter,
    log: ViewLog,
    last_read: Instant,
    bound_at: Instant,
    ticking: bool,
}

impl View {
    fn new(path: PathBuf, max_entries: usize, max_bytes: usize) -> Self {
        Self {
            path,
            offset: 0,
            attached: false,
            file_identity: None,
            adapter: ClaudeAdapter::default(),
            log: ViewLog::new(max_entries, max_bytes),
            last_read: Instant::now(),
            bound_at: Instant::now(),
            ticking: false,
        }
    }

    /// Point at another transcript (a new session id). Never mixes two files.
    fn rebind(&mut self, path: PathBuf) {
        self.bound_at = Instant::now();
        if path == self.path {
            return;
        }
        self.path = path;
        self.restart();
    }

    fn restart(&mut self) {
        self.offset = 0;
        self.attached = false;
        self.file_identity = None;
        self.adapter = ClaudeAdapter::default();
        self.log.reset();
    }

    /// Read what the agent wrote since the last call. Returns true when the log
    /// moved.
    fn advance(&mut self, window: u64) -> std::io::Result<bool> {
        let before = (self.log.epoch, self.log.next_seq);
        let identity = same_file::Handle::from_path(&self.path)?;
        if self
            .file_identity
            .as_ref()
            .is_some_and(|old| *old != identity)
        {
            self.restart();
        }
        self.file_identity = Some(identity);
        let appended = if self.attached {
            let appended = read_appended(&self.path, &mut self.offset)?;
            if appended.restarted {
                self.restart();
                // The file shrank: read its end like a first attach.
                return self.advance(window).map(|_| true);
            }
            appended
        } else {
            let window = read_last_window(&self.path, &mut self.offset, window)?;
            self.attached = true;
            window
        };
        for line in appended.text.lines() {
            for update in self.adapter.absorb(line) {
                self.log.push(update);
            }
        }
        Ok((self.log.epoch, self.log.next_seq) != before)
    }
}

/// What a client gets back.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatViewSnapshot {
    pub epoch: u64,
    /// Ask for this next time.
    pub next_seq: u64,
    /// The client must drop what it holds and apply `updates` from scratch.
    pub reset: bool,
    pub updates: Vec<Value>,
    pub unknown_rows: u64,
    pub malformed_rows: u64,
}

/// Per-terminal views, keyed by TUIC session id.
#[derive(Default)]
pub(crate) struct ChatViews {
    views: Mutex<HashMap<String, Arc<Mutex<View>>>>,
}

fn not_bound(why: &str) -> String {
    format!("not_bound: {why}")
}

/// Resolve the transcript the same way Progress Flow does. `None` for a shell
/// tab, another agent, or a Claude without a published session.
fn bound_transcript(state: &AppState, session_id: &str) -> Result<PathBuf, String> {
    let source = crate::subagent_map::transcript_source(state, session_id)
        .ok_or_else(|| not_bound("no Claude agent is bound to this terminal"))?;
    if !Path::new(&source.parent_transcript).is_file() {
        return Err(not_bound("the agent has not written a transcript yet"));
    }
    Ok(source.parent_transcript)
}

/// The chat view of one terminal from `from_seq`. Blocking: stat, read, parse.
pub(crate) fn chat_view_snapshot(
    state: &Arc<AppState>,
    session_id: &str,
    epoch: Option<u64>,
    from_seq: u64,
) -> Result<ChatViewSnapshot, String> {
    let existing = state.chat_views.views.lock().get(session_id).cloned();
    let view = match existing {
        Some(view) => {
            let bound_for = view.lock().bound_at.elapsed();
            if bound_for >= REBIND_EVERY {
                rebind(state, session_id, &view)?;
            }
            view
        }
        None => {
            let path = bound_transcript(state, session_id)?;
            let mut views = state.chat_views.views.lock();
            views
                .entry(session_id.to_owned())
                .or_insert_with(|| {
                    Arc::new(Mutex::new(View::new(path, MAX_LOG_ENTRIES, MAX_LOG_BYTES)))
                })
                .clone()
        }
    };

    let (snapshot, start_ticker) = {
        let mut view = view.lock();
        view.last_read = Instant::now();
        view.advance(TAIL_WINDOW_BYTES)
            .map_err(|e| not_bound(&format!("transcript unreadable: {e}")))?;
        let (reset, updates) = view.log.since(epoch, from_seq);
        let snapshot = ChatViewSnapshot {
            epoch: view.log.epoch,
            next_seq: view.log.next_seq,
            reset,
            updates,
            unknown_rows: view.adapter.stats.unknown_rows,
            malformed_rows: view.adapter.stats.malformed_rows,
        };
        let start = !view.ticking;
        view.ticking = true;
        (snapshot, start)
    };
    if start_ticker {
        spawn_ticker(state.clone(), session_id.to_owned(), view);
    }
    Ok(snapshot)
}

/// Re-check the binding. A lost binding drops the view and answers not bound.
fn rebind(state: &AppState, session_id: &str, view: &Arc<Mutex<View>>) -> Result<(), String> {
    match bound_transcript(state, session_id) {
        Ok(path) => {
            view.lock().rebind(path);
            Ok(())
        }
        Err(e) => {
            state.chat_views.views.lock().remove(session_id);
            Err(e)
        }
    }
}

/// `blocking` pool wrapper: the snapshot reads files.
pub(crate) async fn chat_view_snapshot_blocking(
    state: Arc<AppState>,
    session_id: String,
    epoch: Option<u64>,
    from_seq: u64,
) -> Result<ChatViewSnapshot, String> {
    tokio::task::spawn_blocking(move || chat_view_snapshot(&state, &session_id, epoch, from_seq))
        .await
        .map_err(|e| format!("chat view task failed: {e}"))?
}

fn spawn_ticker(state: Arc<AppState>, session_id: String, view: Arc<Mutex<View>>) {
    // No runtime (a unit test calling the snapshot directly): no wake events.
    let Ok(runtime) = tokio::runtime::Handle::try_current() else {
        view.lock().ticking = false;
        tracing::warn!(target: "chat_view", session_id, "Chat view ticker stopped: no Tokio runtime");
        return;
    };
    runtime.spawn(async move {
        loop {
            tokio::time::sleep(POLL).await;
            let (st, sid, v) = (state.clone(), session_id.clone(), view.clone());
            let tick = tokio::task::spawn_blocking(move || tick(&st, &sid, &v)).await;
            match tick {
                Ok(Tick::Idle) => {}
                Ok(Tick::Moved(seq)) => state.notify_chat_view_changed(&session_id, seq),
                Ok(Tick::Stop(reason)) => {
                    tracing::warn!(target: "chat_view", session_id, %reason, "Chat view ticker stopped");
                    break;
                }
                Err(error) => {
                    view.lock().ticking = false;
                    tracing::warn!(target: "chat_view", session_id, %error, "Chat view ticker stopped: tick task failed");
                    break;
                }
            }
        }
    });
}

enum Tick {
    Idle,
    Moved(u64),
    Stop(String),
}

fn tick(state: &AppState, session_id: &str, view: &Arc<Mutex<View>>) -> Tick {
    let stop = |view: &Arc<Mutex<View>>, reason: String| {
        {
            let mut views = state.chat_views.views.lock();
            // Only the registration of this very view: a newer one may have replaced it.
            if views.get(session_id).is_some_and(|v| Arc::ptr_eq(v, view)) {
                views.remove(session_id);
            }
        }
        view.lock().ticking = false;
        Tick::Stop(reason)
    };
    let (unread_for, bound_for) = {
        let v = view.lock();
        (v.last_read.elapsed(), v.bound_at.elapsed())
    };
    if unread_for > VIEWER_IDLE {
        return stop(view, "viewer idle".into());
    }
    if bound_for >= REBIND_EVERY
        && let Err(error) = rebind(state, session_id, view)
    {
        return stop(view, error);
    }
    let mut guard = view.lock();
    match guard.advance(TAIL_WINDOW_BYTES) {
        Ok(true) => Tick::Moved(guard.log.next_seq),
        Ok(false) => Tick::Idle,
        Err(error) => {
            drop(guard);
            stop(view, format!("transcript unreadable: {error}"))
        }
    }
}

#[cfg(test)]
mod critic_tests;
#[cfg(test)]
mod measurement;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod follow_http_fixture;
#[cfg(test)]
mod follow_tests;
