//! Pre-built BM25 content index for sub-millisecond file content search.
//!
//! Unlike the grep-then-rerank approach in `fs::search_content_impl`, this
//! module builds a persistent BM25 index over all text files in a repo at
//! load time. Queries hit the in-memory index (~1ms) and then grep only the
//! top-ranked files for exact line matches — avoiding a full repo walk.
//!
//! The index is stored per-repo in `AppState::content_indices` and rebuilt
//! incrementally on `RepoChanged` events via file mtime tracking.

use bm25::{Language, SearchEngineBuilder, SearchResult};
use dashmap::DashSet;
use ignore::WalkBuilder;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime};

/// Maximum file size to index (1 MB).
const MAX_FILE_SIZE: u64 = 1_048_576;

/// Minimum interval between consecutive index rebuilds for the same repo.
const REBUILD_COOLDOWN: Duration = Duration::from_secs(60);

/// Files processed between throttle checkpoints during index build.
const THROTTLE_CHECKPOINT_INTERVAL: usize = 50;
/// Poll interval while an indexer is paused waiting for searches to finish.
const THROTTLE_SEARCH_POLL: Duration = Duration::from_millis(100);
/// Unconditional sleep injected at every checkpoint so index builds don't
/// saturate CPU cores — important in debug builds where corpus construction
/// is significantly slower and runs unoptimised.
const THROTTLE_BUILD_YIELD: Duration = Duration::from_millis(10);

/// Cooperative throttle that yields CPU during index builds and pauses
/// indexing while user-initiated searches are in flight.
#[derive(Default)]
pub struct IndexerThrottle {
    search_active: AtomicUsize,
}

/// RAII guard: increments the active-search counter on creation, decrements on drop.
/// Acquire at the top of every search handler so indexers step aside. Owns an
/// `Arc` so the guard is `'static` and can cross `spawn_blocking` boundaries.
#[must_use = "throttle guard must be held for the duration of the search"]
pub struct SearchGuard {
    throttle: Arc<IndexerThrottle>,
}

impl Drop for SearchGuard {
    fn drop(&mut self) {
        self.throttle.search_active.fetch_sub(1, Ordering::Release);
    }
}

impl IndexerThrottle {
    /// Mark a search as active for the lifetime of the returned guard.
    pub fn begin_search(self: &Arc<Self>) -> SearchGuard {
        // AcqRel pairs with the `load(Acquire)` in `checkpoint` so the
        // increment is published to indexer threads before they resume.
        self.search_active.fetch_add(1, Ordering::AcqRel);
        SearchGuard {
            throttle: Arc::clone(self),
        }
    }

    /// Called from the indexer loop every `THROTTLE_CHECKPOINT_INTERVAL` files.
    /// Blocks (via `thread::sleep`) while any search is active, then yields
    /// unconditionally so index builds don't saturate CPU cores.
    pub fn checkpoint(&self) {
        while self.search_active.load(Ordering::Acquire) > 0 {
            std::thread::sleep(THROTTLE_SEARCH_POLL);
        }
        std::thread::sleep(THROTTLE_BUILD_YIELD);
    }
}

/// A single indexed file entry.
#[derive(Debug, Clone)]
#[allow(dead_code)] // fields used during build, read path uses engine results
struct FileEntry {
    /// Path relative to repo root (forward-slash separated).
    rel_path: String,
    /// File mtime at indexing time, for incremental rebuilds.
    mtime: u64,
}

/// Pre-built BM25 index over file contents in a single repository.
#[allow(dead_code)] // path_to_idx populated for future incremental rebuilds
pub struct ContentIndex {
    engine: bm25::SearchEngine<u32>,
    entries: Vec<FileEntry>,
    /// rel_path → index into `entries` (for incremental mtime checks).
    path_to_idx: HashMap<String, usize>,
    /// Absolute repo root used to resolve relative paths.
    repo_root: PathBuf,
    /// Whether the index has been built at least once.
    ready: bool,
    /// When the last successful build completed.
    built_at: std::time::Instant,
    /// Files confirmed binary (rel_path → mtime). Carried across rebuilds
    /// so we skip the 8KB read probe for files whose mtime hasn't changed.
    known_binaries: HashMap<String, u64>,
}

/// Result of a BM25 file-level query: ranked file paths.
#[derive(Debug, Clone)]
#[allow(dead_code)] // score exposed for future ranking/filtering by callers
pub struct RankedFile {
    pub rel_path: String,
    pub score: f32,
}

impl ContentIndex {
    /// Create an empty, not-yet-built index for a repo.
    pub fn empty(repo_root: PathBuf) -> Self {
        Self {
            engine: SearchEngineBuilder::<u32>::with_corpus(
                Language::English,
                Vec::<String>::new(),
            )
            .build(),
            entries: Vec::new(),
            path_to_idx: HashMap::new(),
            repo_root,
            ready: false,
            built_at: std::time::Instant::now(),
            known_binaries: HashMap::new(),
        }
    }

    /// Build (or rebuild) the full index by walking the repo.
    ///
    /// This is I/O-heavy and should be called from `spawn_blocking`. Respects
    /// .gitignore, skips binary files and files > 1 MB. When `throttle` is
    /// provided, the walker yields cooperatively every `THROTTLE_CHECKPOINT_INTERVAL`
    /// files and pauses entirely while a search is active. Pass `None` for
    /// tests or one-shot builds where throttling is irrelevant.
    pub fn build(
        repo_root: PathBuf,
        throttle: Option<&IndexerThrottle>,
        prior_binaries: HashMap<String, u64>,
    ) -> Self {
        let canonical = repo_root
            .canonicalize()
            .unwrap_or_else(|_| repo_root.clone());

        let mut entries = Vec::new();
        let mut corpus = Vec::new();
        let mut path_to_idx = HashMap::new();
        let mut known_binaries = HashMap::new();

        let walker = WalkBuilder::new(&canonical)
            .hidden(false)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .filter_entry(|e| !crate::fs::is_always_excluded_dir(e))
            .build();

        let mut processed: usize = 0;
        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                continue;
            }

            processed += 1;
            if let Some(t) = throttle
                && processed.is_multiple_of(THROTTLE_CHECKPOINT_INTERVAL)
            {
                t.checkpoint();
            }

            let metadata = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };

            if metadata.len() > MAX_FILE_SIZE {
                continue;
            }

            let rel_path = match entry.path().strip_prefix(&canonical) {
                Ok(p) => p.to_string_lossy().replace('\\', "/"),
                Err(_) => continue,
            };

            let mtime = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs());

            // Skip binary files — use cached result if mtime unchanged
            if let Some(&cached_mtime) = prior_binaries.get(&rel_path)
                && cached_mtime == mtime
            {
                known_binaries.insert(rel_path, mtime);
                continue;
            }
            if is_binary(entry.path()) {
                known_binaries.insert(rel_path, mtime);
                continue;
            }

            let content = match std::fs::read_to_string(entry.path()) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let idx = entries.len();
            path_to_idx.insert(rel_path.clone(), idx);

            // BM25 document: filename + content for searchability
            corpus.push(format!("{}\n{}", rel_path, content));

            entries.push(FileEntry { rel_path, mtime });
        }

        let engine = SearchEngineBuilder::<u32>::with_corpus(Language::English, corpus).build();

        Self {
            engine,
            entries,
            path_to_idx,
            repo_root: canonical,
            ready: true,
            built_at: std::time::Instant::now(),
            known_binaries,
        }
    }

    /// Whether the index has been built at least once.
    pub fn is_ready(&self) -> bool {
        self.ready
    }

    /// Query the index, returning up to `limit` ranked file paths.
    ///
    /// Returns empty if the index is not yet built or the query is empty.
    pub fn search(&self, query: &str, limit: usize) -> Vec<RankedFile> {
        if !self.ready || query.trim().is_empty() {
            return Vec::new();
        }

        let results: Vec<SearchResult<u32>> = self.engine.search(query, limit);
        results
            .into_iter()
            .filter_map(|r| {
                self.entries
                    .get(r.document.id as usize)
                    .map(|e| RankedFile {
                        rel_path: e.rel_path.clone(),
                        score: r.score,
                    })
            })
            .collect()
    }

    /// Absolute path for a relative path in this repo.
    pub fn absolute_path(&self, rel_path: &str) -> PathBuf {
        self.repo_root.join(rel_path)
    }

    /// Number of indexed files.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

// ---------------------------------------------------------------------------
// Background index builder — subscribes to RepoChanged events
// ---------------------------------------------------------------------------

/// Spawn a blocking index build and log any panic in the Tokio blocking pool.
/// Without this supervisor the `JoinHandle` would be dropped, silently
/// swallowing panics (e.g. allocation failure, poisoned locks) and leaving
/// the index in a stale/empty state with no diagnostic.
///
/// `rt` must be a handle to the Tokio runtime — callers that may run on the
/// Tauri main thread (which has no implicit runtime context) MUST pass this
/// explicitly rather than relying on `Handle::current()`.
///
/// When `in_flight` is provided, the repo key is removed on completion
/// (success or panic) so future rebuilds are not permanently blocked.
fn spawn_build<F>(
    rt: &tokio::runtime::Handle,
    repo: String,
    build_fn: F,
    in_flight: Option<Arc<DashSet<String>>>,
    sem: Arc<tokio::sync::Semaphore>,
) where
    F: FnOnce() + Send + 'static,
{
    let rt = rt.clone();
    rt.clone().spawn(async move {
        // Acquire global build semaphore (permits=1) to serialize concurrent builds.
        // Callers do not need to coordinate — whichever acquires first runs, others queue.
        let _permit = sem.acquire_owned().await.ok();
        let handle = rt.spawn_blocking(build_fn);
        if let Err(e) = handle.await {
            tracing::error!(repo = %repo, error = ?e, "content index build task panicked");
        }
        if let Some(set) = in_flight {
            set.remove(&repo);
        }
        // _permit drops here, releasing the semaphore for the next queued build
    });
}

/// Ensure a content index exists for the given repo, building it in background
/// if needed. Returns immediately — callers should check `is_ready()`.
///
/// Uses `state.index_in_flight` to prevent duplicate concurrent builds: if a
/// build is already running (started by this function or by `rebuild_index`),
/// the call returns the existing placeholder without spawning a second task.
pub fn ensure_index(
    state: &Arc<crate::state::AppState>,
    repo_path: &str,
) -> Arc<parking_lot::RwLock<ContentIndex>> {
    use dashmap::mapref::entry::Entry;

    // Atomically check-and-insert: if the entry already exists return it,
    // otherwise insert a placeholder and proceed to spawn the build.
    let index = match state.content_indices.entry(repo_path.to_string()) {
        Entry::Occupied(e) => return Arc::clone(e.get()),
        Entry::Vacant(e) => {
            let idx = Arc::new(parking_lot::RwLock::new(ContentIndex::empty(
                PathBuf::from(repo_path),
            )));
            e.insert(Arc::clone(&idx));
            idx
            // Entry (and its shard lock) is dropped here before we spawn.
        }
    };

    // Guard against a concurrent rebuild_index for the same repo.
    // If the key is already in in_flight (e.g. RepoChanged fired first),
    // the placeholder is in the map but no second build is needed.
    if !state.index_in_flight.insert(repo_path.to_string()) {
        return index;
    }

    let index_ref = Arc::clone(&index);
    let repo = repo_path.to_string();
    let throttle = Arc::clone(&state.indexer_throttle);
    let in_flight = Arc::clone(&state.index_in_flight);
    let sem = Arc::clone(&state.index_build_sem);
    let repo_for_log = repo.clone();
    // Use tauri::async_runtime::handle() so this is safe to call from the
    // Tauri main thread (synchronous IPC handlers), which has no implicit
    // tokio runtime context.
    let rt = tauri::async_runtime::handle();
    spawn_build(
        rt.inner(),
        repo_for_log,
        move || {
            let built = ContentIndex::build(PathBuf::from(&repo), Some(&throttle), HashMap::new());
            *index_ref.write() = built;
            tracing::info!(repo = %repo, "content index built");
        },
        Some(in_flight),
        sem,
    );

    index
}

/// Rebuild the content index for a repo (called on RepoChanged events).
/// Runs in background, does not block. Skips if a build is already in-flight
/// for this repo (via `state.index_in_flight`) — the next `RepoChanged` will
/// pick up any missed changes.
pub fn rebuild_index(
    state: &Arc<crate::state::AppState>,
    repo_path: &str,
) {
    let in_flight = &state.index_in_flight;
    let index = if let Some(existing) = state.content_indices.get(repo_path) {
        Arc::clone(existing.value())
    } else {
        return;
    };

    {
        let idx = index.read();
        if idx.ready && idx.built_at.elapsed() < REBUILD_COOLDOWN {
            tracing::trace!(repo = %repo_path, "content index rebuild skipped (cooldown)");
            return;
        }
    }

    if !in_flight.insert(repo_path.to_string()) {
        tracing::debug!(repo = %repo_path, "content index rebuild skipped (already in-flight)");
        return;
    }

    let repo = repo_path.to_string();
    let throttle = Arc::clone(&state.indexer_throttle);
    let sem = Arc::clone(&state.index_build_sem);
    let repo_for_log = repo.clone();
    let prior_binaries = index.read().known_binaries.clone();
    let rt = tauri::async_runtime::handle();
    spawn_build(
        rt.inner(),
        repo_for_log,
        move || {
            let built = ContentIndex::build(PathBuf::from(&repo), Some(&throttle), prior_binaries);
            *index.write() = built;
            tracing::debug!(repo = %repo, "content index rebuilt");
        },
        Some(Arc::clone(in_flight)),
        sem,
    );
}

/// Spawn a background task that listens to the event bus and rebuilds
/// content indices when repos change. Should be called once at startup.
pub fn spawn_content_index_updater(state: Arc<crate::state::AppState>) {
    let mut rx = state.event_bus.subscribe();
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(crate::state::AppEvent::RepoChanged { repo_path }) => {
                    if crate::config::load_app_config().index_strategy != "disabled" {
                        rebuild_index(&state, &repo_path);
                    }
                }
                Ok(other) => {
                    // Other AppEvent variants intentionally ignored by the
                    // content_index updater — trace so new variants are
                    // visible in debug builds.
                    tracing::trace!(source = "content_index", ?other, "ignored event");
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(source = "content_index", lagged = n, "event bus lagged");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

/// Check if a file is binary by reading the first 8 KB for null bytes.
fn is_binary(path: &Path) -> bool {
    use std::io::Read;
    let mut buf = [0u8; 8192];
    match std::fs::File::open(path).and_then(|mut f| f.read(&mut buf)) {
        Ok(n) => buf[..n].contains(&0u8),
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Create a temp repo directory with some text files for testing.
    fn make_test_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Create a few text files with known content
        fs::write(
            root.join("main.rs"),
            "fn main() {\n    println!(\"hello world\");\n}\n",
        )
        .unwrap();
        fs::write(
            root.join("lib.rs"),
            "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n",
        )
        .unwrap();
        fs::write(
            root.join("search.rs"),
            "use bm25::SearchEngine;\nfn search_content(query: &str) {\n    // BM25 search implementation\n}\n",
        ).unwrap();
        fs::write(
            root.join("README.md"),
            "# My Project\n\nA project about search and indexing.\n",
        )
        .unwrap();

        // Create a subdirectory with a file
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("src/utils.rs"),
            "pub fn format_result(s: &str) -> String {\n    s.to_uppercase()\n}\n",
        )
        .unwrap();

        dir
    }

    #[test]
    fn build_indexes_text_files() {
        let repo = make_test_repo();
        let index = ContentIndex::build(repo.path().to_path_buf(), None, HashMap::new());

        assert!(index.is_ready());
        assert_eq!(index.len(), 5); // main.rs, lib.rs, search.rs, README.md, src/utils.rs
    }

    #[test]
    fn search_finds_relevant_file() {
        let repo = make_test_repo();
        let index = ContentIndex::build(repo.path().to_path_buf(), None, HashMap::new());

        let results = index.search("BM25 search implementation", 5);
        assert!(!results.is_empty());
        assert_eq!(results[0].rel_path, "search.rs");
    }

    #[test]
    fn search_ranks_by_relevance() {
        let repo = make_test_repo();
        let index = ContentIndex::build(repo.path().to_path_buf(), None, HashMap::new());

        // "println hello" should rank main.rs first
        let results = index.search("println hello", 5);
        assert!(!results.is_empty());
        assert_eq!(results[0].rel_path, "main.rs");
    }

    #[test]
    fn search_empty_query_returns_nothing() {
        let repo = make_test_repo();
        let index = ContentIndex::build(repo.path().to_path_buf(), None, HashMap::new());

        assert!(index.search("", 5).is_empty());
        assert!(index.search("   ", 5).is_empty());
    }

    #[test]
    fn empty_index_returns_nothing() {
        let index = ContentIndex::empty(PathBuf::from("/nonexistent"));
        assert!(!index.is_ready());
        assert!(index.search("anything", 5).is_empty());
    }

    #[test]
    fn skips_binary_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        fs::write(root.join("text.rs"), "fn hello() {}").unwrap();
        // Binary file: contains null bytes
        fs::write(root.join("binary.bin"), b"\x00\x01\x02\x03").unwrap();

        let index = ContentIndex::build(root.to_path_buf(), None, HashMap::new());
        assert_eq!(index.len(), 1); // only text.rs
    }

    #[test]
    fn skips_large_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        fs::write(root.join("small.rs"), "fn small() {}").unwrap();
        // File > 1 MB
        let large = "x".repeat(MAX_FILE_SIZE as usize + 1);
        fs::write(root.join("large.txt"), large).unwrap();

        let index = ContentIndex::build(root.to_path_buf(), None, HashMap::new());
        assert_eq!(index.len(), 1); // only small.rs
    }

    #[test]
    fn absolute_path_resolves_correctly() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("test.rs"), "fn test() {}").unwrap();

        let index = ContentIndex::build(root.to_path_buf(), None, HashMap::new());
        let abs = index.absolute_path("test.rs");
        assert!(abs.ends_with("test.rs"));
        assert!(abs.is_absolute());
    }

    #[test]
    fn search_finds_file_in_subdirectory() {
        let repo = make_test_repo();
        let index = ContentIndex::build(repo.path().to_path_buf(), None, HashMap::new());

        let results = index.search("format_result to_uppercase", 5);
        assert!(!results.is_empty());
        assert_eq!(results[0].rel_path, "src/utils.rs");
    }

    #[test]
    fn skips_dot_git_directory() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        fs::write(root.join("real.rs"), "fn real() {}").unwrap();
        // Simulate .git internals (normally not in .gitignore)
        fs::create_dir_all(root.join(".git/objects")).unwrap();
        fs::write(root.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        fs::write(root.join(".git/objects/pack.txt"), "pack data here").unwrap();

        let index = ContentIndex::build(root.to_path_buf(), None, HashMap::new());
        assert_eq!(index.len(), 1); // only real.rs
        assert!(index.search("pack data", 5).is_empty());
    }

    #[test]
    fn query_is_sub_millisecond() {
        // Create a repo with 200 files to simulate realistic conditions
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        for i in 0..200 {
            let content = format!(
                "// File {i}\nfn function_{i}() {{\n    let value = {};\n    println!(\"result: {{}}\", value);\n}}\n",
                i * 42
            );
            fs::write(root.join(format!("file_{i}.rs")), content).unwrap();
        }

        let index = ContentIndex::build(root.to_path_buf(), None, HashMap::new());
        assert_eq!(index.len(), 200);

        // Warm up
        let _ = index.search("function value", 50);

        // Measure query time (10 iterations)
        let start = std::time::Instant::now();
        let iterations = 10;
        for _ in 0..iterations {
            let _ = index.search("function value println result", 50);
        }
        let elapsed = start.elapsed();
        let avg_us = elapsed.as_micros() / iterations;

        // Each query should be under 5ms (generous for CI, typically ~0.1ms)
        assert!(
            avg_us < 5000,
            "Average query time {avg_us}µs exceeds 5ms threshold"
        );
    }
}
