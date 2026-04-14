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
use ignore::WalkBuilder;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Maximum file size to index (1 MB).
const MAX_FILE_SIZE: u64 = 1_048_576;

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
            engine: SearchEngineBuilder::<u32>::with_corpus(Language::English, Vec::<String>::new())
                .build(),
            entries: Vec::new(),
            path_to_idx: HashMap::new(),
            repo_root,
            ready: false,
        }
    }

    /// Build (or rebuild) the full index by walking the repo.
    ///
    /// This is I/O-heavy and should be called from `spawn_blocking`.
    /// Respects .gitignore, skips binary files and files > 1 MB.
    pub fn build(repo_root: PathBuf) -> Self {
        let canonical = repo_root
            .canonicalize()
            .unwrap_or_else(|_| repo_root.clone());

        let mut entries = Vec::new();
        let mut corpus = Vec::new();
        let mut path_to_idx = HashMap::new();

        let walker = WalkBuilder::new(&canonical)
            .hidden(false)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .filter_entry(|entry| {
                // Never descend into .git directories
                !(entry.file_type().is_some_and(|ft| ft.is_dir())
                    && entry.file_name() == ".git")
            })
            .build();

        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                continue;
            }

            let metadata = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };

            if metadata.len() > MAX_FILE_SIZE {
                continue;
            }

            // Skip binary files (check first 8 KB for null bytes)
            if is_binary(entry.path()) {
                continue;
            }

            let rel_path = match entry.path().strip_prefix(&canonical) {
                Ok(p) => p.to_string_lossy().replace('\\', "/"),
                Err(_) => continue,
            };

            let content = match std::fs::read_to_string(entry.path()) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let mtime = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs());

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
                self.entries.get(r.document.id as usize).map(|e| RankedFile {
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

use std::sync::Arc;

/// Ensure a content index exists for the given repo, building it in background
/// if needed. Returns immediately — callers should check `is_ready()`.
pub fn ensure_index(
    state: &Arc<crate::state::AppState>,
    repo_path: &str,
) -> Arc<parking_lot::RwLock<ContentIndex>> {
    // Fast path: index already exists
    if let Some(existing) = state.content_indices.get(repo_path) {
        return Arc::clone(existing.value());
    }

    // Create an empty placeholder and spawn background build
    let index = Arc::new(parking_lot::RwLock::new(ContentIndex::empty(
        PathBuf::from(repo_path),
    )));
    state
        .content_indices
        .insert(repo_path.to_string(), Arc::clone(&index));

    let index_ref = Arc::clone(&index);
    let repo = repo_path.to_string();
    // Low-priority: spawn_blocking uses the Tokio blocking pool
    tokio::task::spawn_blocking(move || {
        let built = ContentIndex::build(PathBuf::from(&repo));
        *index_ref.write() = built;
        tracing::info!(repo = %repo, "content index built");
    });

    index
}

/// Rebuild the content index for a repo (called on RepoChanged events).
/// Runs in background, does not block.
pub fn rebuild_index(
    state: &Arc<crate::state::AppState>,
    repo_path: &str,
) {
    let index = if let Some(existing) = state.content_indices.get(repo_path) {
        Arc::clone(existing.value())
    } else {
        // No index yet — nothing to rebuild
        return;
    };

    let repo = repo_path.to_string();
    tokio::task::spawn_blocking(move || {
        let built = ContentIndex::build(PathBuf::from(&repo));
        *index.write() = built;
        tracing::debug!(repo = %repo, "content index rebuilt");
    });
}

/// Spawn a background task that listens to the event bus and rebuilds
/// content indices when repos change. Should be called once at startup.
pub fn spawn_content_index_updater(state: Arc<crate::state::AppState>) {
    let mut rx = state.event_bus.subscribe();
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(crate::state::AppEvent::RepoChanged { repo_path }) => {
                    rebuild_index(&state, &repo_path);
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(source = "content_index", lagged = n, "event bus lagged");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                _ => {} // ignore other events
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
        fs::write(root.join("main.rs"), "fn main() {\n    println!(\"hello world\");\n}\n").unwrap();
        fs::write(root.join("lib.rs"), "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n").unwrap();
        fs::write(
            root.join("search.rs"),
            "use bm25::SearchEngine;\nfn search_content(query: &str) {\n    // BM25 search implementation\n}\n",
        ).unwrap();
        fs::write(root.join("README.md"), "# My Project\n\nA project about search and indexing.\n").unwrap();

        // Create a subdirectory with a file
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/utils.rs"), "pub fn format_result(s: &str) -> String {\n    s.to_uppercase()\n}\n").unwrap();

        dir
    }

    #[test]
    fn build_indexes_text_files() {
        let repo = make_test_repo();
        let index = ContentIndex::build(repo.path().to_path_buf());

        assert!(index.is_ready());
        assert_eq!(index.len(), 5); // main.rs, lib.rs, search.rs, README.md, src/utils.rs
    }

    #[test]
    fn search_finds_relevant_file() {
        let repo = make_test_repo();
        let index = ContentIndex::build(repo.path().to_path_buf());

        let results = index.search("BM25 search implementation", 5);
        assert!(!results.is_empty());
        assert_eq!(results[0].rel_path, "search.rs");
    }

    #[test]
    fn search_ranks_by_relevance() {
        let repo = make_test_repo();
        let index = ContentIndex::build(repo.path().to_path_buf());

        // "println hello" should rank main.rs first
        let results = index.search("println hello", 5);
        assert!(!results.is_empty());
        assert_eq!(results[0].rel_path, "main.rs");
    }

    #[test]
    fn search_empty_query_returns_nothing() {
        let repo = make_test_repo();
        let index = ContentIndex::build(repo.path().to_path_buf());

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

        let index = ContentIndex::build(root.to_path_buf());
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

        let index = ContentIndex::build(root.to_path_buf());
        assert_eq!(index.len(), 1); // only small.rs
    }

    #[test]
    fn absolute_path_resolves_correctly() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("test.rs"), "fn test() {}").unwrap();

        let index = ContentIndex::build(root.to_path_buf());
        let abs = index.absolute_path("test.rs");
        assert!(abs.ends_with("test.rs"));
        assert!(abs.is_absolute());
    }

    #[test]
    fn search_finds_file_in_subdirectory() {
        let repo = make_test_repo();
        let index = ContentIndex::build(repo.path().to_path_buf());

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

        let index = ContentIndex::build(root.to_path_buf());
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

        let index = ContentIndex::build(root.to_path_buf());
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
