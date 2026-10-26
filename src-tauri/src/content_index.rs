//! Pre-built BM25 content index for sub-millisecond file content search.
//!
//! Unlike the grep-then-rerank approach in `fs::search_content_impl`, this
//! module builds a persistent BM25 index over all text files in a repo at
//! load time. Queries hit the in-memory index (~1ms) and then grep only the
//! top-ranked files for exact line matches — avoiding a full repo walk.
//!
//! The index is stored per-repo in `AppState::content_indices` and rebuilt on
//! `RepoChanged` events, but only when a stat-only walk finds an indexable file
//! whose mtime or size moved (`ContentIndex::is_current`) — a git-state change that
//! touches no file content must not pay for a full re-read of the repo. When
//! something did move, only the files that moved are re-read and re-embedded
//! (`plan_disk_changes`/`apply_disk_changes`); the whole-corpus rebuild is the
//! fallback for a change set too large to absorb that way.
//!
//! Total resident size across repos is bounded by `enforce_memory_budget`, which
//! drops the least recently used indices after each build. An evicted index is
//! written to `<data_dir>/content-index/` first and restored on the next
//! `ensure_index`, so the bound costs a stat walk on return rather than a
//! rebuild. A restored snapshot is always validated against disk and brought up
//! to date before it serves anything. `repo_watcher::stop_watching` releases a
//! repo's index when it is no longer in use.
//!
//! Search results need only the BM25 embeddings and their file ids. The source
//! text is deliberately dropped after each build; a future feature that needs
//! snippets or previews must read the current file from disk rather than serve
//! a stale second copy retained by the index.

use bm25::{DefaultTokenizer, Embedder, EmbedderBuilder, Language, Scorer, Tokenizer};
use dashmap::DashSet;
use ignore::WalkBuilder;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, SystemTime};

/// Maximum file size to index (1 MB).
const MAX_FILE_SIZE: u64 = 1_048_576;

/// Ticks handed to indices so the budget can tell which was used least recently.
///
/// A counter rather than a clock because eviction only ever *compares* two uses,
/// and a monotonic counter cannot be moved by the system clock or made ambiguous
/// by two indices touched inside the same millisecond.
static ACCESS_CLOCK: AtomicU64 = AtomicU64::new(0);

/// The next tick. Every touch gets a value strictly greater than every earlier one.
fn next_access_tick() -> u64 {
    ACCESS_CLOCK.fetch_add(1, Ordering::Relaxed) + 1
}

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

/// Cooperative throttle that yields CPU during index builds and gives
/// user-initiated searches a bounded head start.
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
    /// Defers once when a search is active, then yields unconditionally so
    /// index builds don't saturate CPU cores. A fallback grep can span the whole
    /// repository, so waiting for every search guard to drop would starve the
    /// missing index whose absence selected that fallback in the first place.
    pub fn checkpoint(&self) {
        if self.search_active.load(Ordering::Acquire) > 0 {
            std::thread::sleep(THROTTLE_SEARCH_POLL);
        }
        std::thread::sleep(THROTTLE_BUILD_YIELD);
    }
}

/// The stat-only fingerprint `is_current` compares a file against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileStamp {
    /// File mtime at indexing time, in nanoseconds since the epoch.
    ///
    /// Nanoseconds, not seconds: an agent editing the same file twice inside one
    /// second is the normal case here — second granularity would report the index
    /// as still current and the edit would stay unsearchable until something else
    /// invalidated it.
    mtime: u64,
    /// File size in bytes.
    ///
    /// mtime alone misses a content replacement that preserves it, and the tools
    /// that restore files do exactly that: `cp -p`, `rsync -a`, `tar -x`, `unzip`
    /// with timestamps. The index would then serve the old text for as long as
    /// nothing else touched the file. The size is free — the walk already stats
    /// every file — and catches such a replacement whenever the length changes. A
    /// same-size, same-mtime rewrite still slips through; catching that needs the
    /// content read this check exists to avoid.
    len: u64,
}

/// A single indexed file entry.
#[derive(Debug, Clone)]
struct FileEntry {
    /// Path relative to repo root (forward-slash separated).
    rel_path: String,
    /// What the file looked like on disk when it was indexed.
    stamp: FileStamp,
}

/// A file's stat fingerprint; the mtime is `0` when unavailable.
fn file_stamp(metadata: &std::fs::Metadata) -> FileStamp {
    FileStamp {
        mtime: metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_nanos() as u64),
        len: metadata.len(),
    }
}

/// Pre-built BM25 index over file contents in a single repository.
pub struct ContentIndex {
    engine: EmbeddingIndex,
    /// Slots, not a dense list: the position **is** the BM25 document id, so a
    /// deleted file has to leave a hole rather than shift every id after it.
    /// Compacting would silently re-point every posting in the scorer at the
    /// wrong file. `free_ids` hands the holes back out.
    entries: Vec<Option<FileEntry>>,
    /// Slots vacated by deleted files, reused before the vector grows. Without
    /// this, a repo that churns files grows `entries` without bound even though
    /// the file count is steady.
    free_ids: Vec<u32>,
    /// rel_path → index into `entries`, for `is_current`'s stamp comparison.
    path_to_idx: HashMap<String, usize>,
    /// Absolute repo root used to resolve relative paths.
    repo_root: PathBuf,
    /// Whether the index has been built at least once.
    ready: bool,
    /// When the last successful build completed.
    built_at: std::time::Instant,
    /// Last completed no-change validation; independent of corpus rebuilds.
    validated_at: Option<std::time::Instant>,
    /// Files confirmed binary (rel_path → stamp). Carried across rebuilds
    /// so we skip the 8KB read probe for files whose stamp hasn't changed.
    known_binaries: HashMap<String, FileStamp>,
    /// Heap the BM25 engine retained, measured across its build.
    ///
    /// The engine is the heavy half of an index and its internals are opaque —
    /// there is nothing to walk. So it is weighed instead: the allocator's
    /// in-use total before and after `EmbeddingIndex::build`, which retains the
    /// postings and frees its token cache before returning. Builds hold
    /// `index_build_sem` (one permit), so no second build is running; other
    /// threads still allocate during the window, which is why this is an
    /// approximation and not a measurement. It is reported because an index is
    /// the heaviest per-repo thing the app holds and it lives until the repo is
    /// retired — with many repos open, this is the number that explains the
    /// footprint.
    engine_bytes: usize,
    /// Tick of the last search or lookup that reached this index, for the memory
    /// budget's least-recently-used choice.
    ///
    /// Atomic so a search can record the use through the read lock it already
    /// holds. Taking the write lock instead would serialise every query against
    /// every other one, which is the opposite of what this index is for.
    last_used: AtomicU64,
}

/// What a walk found that the index does not already hold.
///
/// Built under the read lock and applied under the write lock, so the expensive
/// half — walking the repo, reading changed files, embedding them — never blocks
/// a search. Keyed by path rather than slot id for the same reason: the plan has
/// to survive the gap between the two locks.
#[derive(Default)]
struct DiskChanges {
    /// Files to index, already read and embedded.
    upserts: Vec<(String, FileStamp, bm25::Embedding<u32>)>,
    /// Files that are binary, unreadable or not UTF-8. Tracked, never indexed.
    unindexable: Vec<(String, FileStamp)>,
    /// Paths the index holds that are no longer on disk.
    removed: Vec<String>,
}

impl DiskChanges {
    /// Files this plan touches, against which the incremental limit is applied.
    fn len(&self) -> usize {
        self.upserts.len() + self.unindexable.len() + self.removed.len()
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Result of a BM25 file-level query: ranked file paths.
#[derive(Debug, Clone)]
#[allow(dead_code)] // score exposed for future ranking/filtering by callers
pub struct RankedFile {
    pub rel_path: String,
    pub score: f32,
}

type BuildTokenCache = HashMap<(usize, usize), Vec<String>>;

/// Shares tokens between the crate's avgdl-fitting and embedding passes. The
/// pointer key is valid only while `EmbeddingIndex::build` owns the corpus; the
/// cache is cleared before it returns, avoiding a second copy of each document.
struct BuildTokenizer {
    inner: DefaultTokenizer,
    cache: Arc<parking_lot::Mutex<Option<BuildTokenCache>>>,
    #[cfg(test)]
    tokenizations: Arc<AtomicUsize>,
}

impl Default for BuildTokenizer {
    /// The post-build state: no cache, so `tokenize` delegates straight to the
    /// inner tokenizer. That is what every path other than a full build wants —
    /// the cache exists only to bridge the crate's two passes over the corpus, and
    /// a restored or incrementally updated index has no corpus to share.
    fn default() -> Self {
        Self {
            inner: DefaultTokenizer::new(Language::English),
            cache: Arc::new(parking_lot::Mutex::new(None)),
            #[cfg(test)]
            tokenizations: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl Tokenizer for BuildTokenizer {
    fn tokenize(&self, input_text: &str) -> Vec<String> {
        let key = (input_text.as_ptr() as usize, input_text.len());
        if let Some(tokens) = self
            .cache
            .lock()
            .as_ref()
            .and_then(|cache| cache.get(&key))
            .cloned()
        {
            return tokens;
        }

        let tokens = self.inner.tokenize(input_text);
        let mut cache = self.cache.lock();
        if let Some(cache) = cache.as_mut() {
            cache.insert(key, tokens.clone());
            #[cfg(test)]
            self.tokenizations.fetch_add(1, Ordering::Relaxed);
        }
        tokens
    }
}

/// The parts of the BM25 crate needed to rank file ids. Its higher-level
/// `SearchEngine` also retains every document string so it can return the text
/// with each result, but callers map ids back to `entries` and never use it.
struct EmbeddingIndex {
    embedder: Embedder<u32, BuildTokenizer>,
    scorer: Scorer<u32>,
    #[cfg(test)]
    build_cache: Arc<parking_lot::Mutex<Option<BuildTokenCache>>>,
    #[cfg(test)]
    build_document_tokenizations: usize,
}

impl EmbeddingIndex {
    fn build(corpus: &[String]) -> Self {
        let documents: Vec<&str> = corpus.iter().map(String::as_str).collect();
        let cache = Arc::new(parking_lot::Mutex::new(Some(HashMap::new())));
        #[cfg(test)]
        let tokenizations = Arc::new(AtomicUsize::new(0));
        let tokenizer = BuildTokenizer {
            inner: DefaultTokenizer::new(Language::English),
            cache: Arc::clone(&cache),
            #[cfg(test)]
            tokenizations: Arc::clone(&tokenizations),
        };
        let embedder = EmbedderBuilder::<u32, BuildTokenizer>::with_tokenizer_and_fit_to_corpus(
            tokenizer, &documents,
        )
        .build();
        let mut scorer = Scorer::new();
        for (id, document) in corpus.iter().enumerate() {
            scorer.upsert(&(id as u32), embedder.embed(document));
        }
        // The cache only bridges the crate's avgdl-fitting and embedding passes.
        // Queries tokenize directly, and no source text survives this point.
        drop(cache.lock().take());
        Self {
            embedder,
            scorer,
            #[cfg(test)]
            build_cache: cache,
            #[cfg(test)]
            build_document_tokenizations: tokenizations.load(Ordering::Relaxed),
        }
    }

    fn search(&self, query: &str, limit: usize) -> Vec<bm25::ScoredDocument<u32>> {
        let query = self.embedder.embed(query);
        self.scorer
            .matches(&query)
            .into_iter()
            .take(limit)
            .collect()
    }
}

impl ContentIndex {
    /// Create an empty, not-yet-built index for a repo.
    pub fn empty(repo_root: PathBuf) -> Self {
        Self {
            engine: EmbeddingIndex::build(&[]),
            entries: Vec::new(),
            free_ids: Vec::new(),
            path_to_idx: HashMap::new(),
            repo_root,
            ready: false,
            built_at: std::time::Instant::now(),
            validated_at: None,
            known_binaries: HashMap::new(),
            engine_bytes: 0,
            last_used: AtomicU64::new(next_access_tick()),
        }
    }

    /// Record that something reached this index, so the budget evicts it last.
    ///
    /// Takes `&self`: callers hold the read lock they were already using to
    /// search. A missed touch costs an index its place in the ordering, never
    /// correctness — the worst case is evicting one that was in use, and it is
    /// rebuilt on the next search.
    pub fn touch(&self) {
        self.last_used.store(next_access_tick(), Ordering::Relaxed);
    }

    /// The tick of the most recent touch. Lower means less recently used.
    fn last_used(&self) -> u64 {
        self.last_used.load(Ordering::Relaxed)
    }

    /// Roughly how much heap this index holds, for `memory_report`: the BM25
    /// engine weighed at build time plus the three maps that scale with the
    /// repo's file count.
    pub fn approx_bytes(&self) -> usize {
        let entries: usize = self
            .entries
            .iter()
            .flatten()
            .map(|e| e.rel_path.len() + std::mem::size_of::<FileEntry>())
            .sum();
        let paths: usize = self
            .path_to_idx
            .keys()
            .map(|k| k.len() + std::mem::size_of::<usize>())
            .sum();
        let binaries: usize = self
            .known_binaries
            .keys()
            .map(|k| k.len() + std::mem::size_of::<FileStamp>())
            .sum();
        self.engine_bytes + entries + paths + binaries
    }

    /// Build (or rebuild) the full index by walking the repo.
    ///
    /// This is I/O-heavy and should be called from `spawn_blocking`. Respects
    /// .gitignore, skips binary files and files > 1 MB. When `throttle` is
    /// provided, the walker yields cooperatively every `THROTTLE_CHECKPOINT_INTERVAL`
    /// files and briefly defers to an active search at each checkpoint. Pass
    /// `None` for tests or one-shot builds where throttling is irrelevant.
    pub fn build(
        repo_root: PathBuf,
        throttle: Option<&IndexerThrottle>,
        prior_binaries: HashMap<String, FileStamp>,
    ) -> Self {
        let canonical = repo_root
            .canonicalize()
            .unwrap_or_else(|_| repo_root.clone());

        let mut entries = Vec::new();
        let mut corpus = Vec::new();
        let mut path_to_idx = HashMap::new();
        let mut known_binaries = HashMap::new();

        let walker = Self::walker(&canonical);

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

            let Some((rel_path, stamp)) = Self::indexable(&entry, &canonical) else {
                continue;
            };

            // Reuse the previous verdict when the stamp has not moved, so a
            // rebuild does not re-probe every binary in the repo.
            let known_binary = prior_binaries.get(&rel_path) == Some(&stamp);
            let Some(content) = Self::indexable_text(entry.path(), known_binary) else {
                known_binaries.insert(rel_path, stamp);
                continue;
            };

            let idx = entries.len();
            path_to_idx.insert(rel_path.clone(), idx);

            // BM25 document: filename + content for searchability
            corpus.push(format!("{}\n{}", rel_path, content));

            entries.push(Some(FileEntry { rel_path, stamp }));
        }

        // Weigh the engine across its own build — see `engine_bytes`. The
        // corpus is already allocated at this point, so it is not counted.
        let heap_before = crate::memory_report::malloc_bytes_in_use();
        let engine = EmbeddingIndex::build(&corpus);
        let engine_bytes = crate::memory_report::malloc_bytes_in_use()
            .zip(heap_before)
            .map_or(0, |(after, before)| after.saturating_sub(before) as usize);

        Self {
            engine,
            entries,
            // A full build assigns ids densely, so there is nothing to reuse.
            free_ids: Vec::new(),
            path_to_idx,
            repo_root: canonical,
            ready: true,
            built_at: std::time::Instant::now(),
            validated_at: None,
            known_binaries,
            engine_bytes,
            last_used: AtomicU64::new(next_access_tick()),
        }
    }

    /// The relative path and stat fingerprint of a walked file, or `None` when it
    /// is not something the index covers: unreadable metadata, larger than
    /// `MAX_FILE_SIZE`, or outside the canonical root.
    ///
    /// Extracted so the full build, the currency check and the incremental update
    /// apply one set of rules. When they drift, `is_current` starts disagreeing
    /// with `build` about which files should be present and reports every index as
    /// stale forever.
    fn indexable(entry: &ignore::DirEntry, canonical_root: &Path) -> Option<(String, FileStamp)> {
        let metadata = entry.metadata().ok()?;
        if metadata.len() > MAX_FILE_SIZE {
            return None;
        }
        let rel = entry.path().strip_prefix(canonical_root).ok()?;
        Some((
            rel.to_string_lossy().replace('\\', "/"),
            file_stamp(&metadata),
        ))
    }

    /// The text to index for a file, or `None` when there is none to index:
    /// binary, unreadable, or not UTF-8 (Latin-1 text with no null byte in the
    /// probed 8 KB lands here).
    ///
    /// A `None` is not a skip — callers must record the file as unindexable.
    /// `is_current` requires every file on disk to be accounted for, so a file in
    /// neither map reports the index stale on every event for the life of the repo.
    ///
    /// `known_binary` skips the 8 KB probe for a file whose stamp has not moved
    /// since it was last classified.
    fn indexable_text(path: &Path, known_binary: bool) -> Option<String> {
        if known_binary || is_binary(path) {
            return None;
        }
        std::fs::read_to_string(path).ok()
    }

    /// The repo walk both `build` and `is_current` must agree on — same ignore
    /// rules, same pruning — so the currency check can never disagree with the
    /// build about which files the index is supposed to cover.
    fn walker(canonical_root: &Path) -> ignore::Walk {
        WalkBuilder::new(canonical_root)
            .hidden(false)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .filter_entry(|e| !crate::fs::is_always_excluded_dir(e))
            .build()
    }

    /// Whether the index still reflects the repo on disk: a stat-only walk, no
    /// file reads and no corpus construction.
    ///
    /// Most `RepoChanged` events do not touch indexable content — `git add`,
    /// `git commit`, a stash, a ref move all emit one while every working-tree
    /// file is byte-for-byte unchanged — and each of those otherwise paid for a
    /// full re-read of the repo plus a complete BM25 rebuild, once a minute for
    /// as long as the events kept coming.
    pub fn is_current(&self) -> bool {
        if !self.ready {
            return false;
        }
        let mut matched = 0usize;
        for entry in Self::walker(&self.repo_root) {
            let Ok(entry) = entry else { continue };
            if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                continue;
            }
            let Some((rel_path, stamp)) = Self::indexable(&entry, &self.repo_root) else {
                continue;
            };
            let known = self
                .path_to_idx
                .get(&rel_path)
                .and_then(|&i| self.entries.get(i))
                .and_then(|slot| slot.as_ref())
                .map(|e| e.stamp)
                .or_else(|| self.known_binaries.get(&rel_path).copied());
            // A file we have never seen, or one whose mtime or size moved: stale.
            if known != Some(stamp) {
                return false;
            }
            matched += 1;
        }
        // Every file we hold must still be on disk, or something was deleted.
        // Live entries only — a hole left by a deleted file is not a file on disk.
        matched == self.live_len() + self.known_binaries.len()
    }

    /// Indexed files, ignoring the holes left by deletions. `path_to_idx` holds
    /// exactly one key per live slot, which makes this O(1) instead of a scan.
    fn live_len(&self) -> usize {
        self.path_to_idx.len()
    }

    /// How many files may move before an incremental update is refused.
    ///
    /// The embedder's average document length is fitted once, by the full build,
    /// and incremental upserts do not move it. Past some fraction of the corpus
    /// that average describes a repo that no longer exists and BM25's length
    /// normalisation degrades, so beyond this the caller rebuilds and refits. The
    /// floor keeps small repos — where a quarter is one or two files — on the
    /// cheap path for ordinary edits.
    fn incremental_change_limit(&self) -> usize {
        (self.live_len() / 4).max(64)
    }

    /// Walk the repo and collect what the index does not already hold, reading and
    /// embedding only the files whose stamp moved.
    ///
    /// Takes `&self` so it can run under the read lock: this walks the whole repo
    /// and reads the changed files, and searches must keep being served while it
    /// does. Only `apply_disk_changes` needs the write lock, and it is O(changes).
    ///
    /// Returns `None` when the caller should do a full rebuild instead — either
    /// too much moved (see `incremental_change_limit`) or the index was never
    /// built, in which case there is no fitted embedder to upsert against.
    fn plan_disk_changes(&self, throttle: Option<&IndexerThrottle>) -> Option<DiskChanges> {
        if !self.ready {
            return None;
        }
        let limit = self.incremental_change_limit();

        let mut seen: std::collections::HashSet<String> =
            std::collections::HashSet::with_capacity(self.live_len());
        let mut changes = DiskChanges::default();
        let mut processed: usize = 0;

        for entry in Self::walker(&self.repo_root) {
            let Ok(entry) = entry else { continue };
            if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                continue;
            }
            processed += 1;
            if let Some(t) = throttle
                && processed.is_multiple_of(THROTTLE_CHECKPOINT_INTERVAL)
            {
                t.checkpoint();
            }
            let Some((rel_path, stamp)) = Self::indexable(&entry, &self.repo_root) else {
                continue;
            };

            let as_text = self
                .path_to_idx
                .get(&rel_path)
                .and_then(|&i| self.entries.get(i))
                .and_then(|slot| slot.as_ref())
                .map(|e| e.stamp);
            let as_unindexable = self.known_binaries.get(&rel_path).copied();

            // Recorded before the unchanged check: this set answers "what is still
            // on disk", and an unchanged file is very much still on disk.
            seen.insert(rel_path.clone());

            if as_text == Some(stamp) || as_unindexable == Some(stamp) {
                continue;
            }

            if changes.len() >= limit {
                return None;
            }

            match Self::indexable_text(entry.path(), false) {
                Some(text) => {
                    // The stored document is filename + content, exactly as the
                    // full build composes it, or the same file would rank
                    // differently depending on which path last touched it.
                    let embedding = self.engine.embedder.embed(&format!("{rel_path}\n{text}"));
                    changes.upserts.push((rel_path, stamp, embedding));
                }
                None => changes.unindexable.push((rel_path, stamp)),
            }
        }

        // Anything the index holds that the walk did not reach is gone from disk.
        changes.removed = self
            .path_to_idx
            .keys()
            .chain(self.known_binaries.keys())
            .filter(|p| !seen.contains(*p))
            .cloned()
            .collect();

        if changes.len() > limit {
            return None;
        }
        Some(changes)
    }

    /// Apply a plan produced by `plan_disk_changes`.
    ///
    /// The plan carries paths, never slot ids, so it stays valid across the gap
    /// between dropping the read lock and taking the write lock. Ids are assigned
    /// here, where the free list can be consulted.
    fn apply_disk_changes(&mut self, changes: DiskChanges) {
        // Weighed the same way as a build, and for the same reason: the engine is
        // opaque, and `approx_bytes` feeds the memory budget. Approximate — other
        // threads allocate during the window — but a budget fed by a number frozen
        // at the last full build would drift further with every update.
        let heap_before = crate::memory_report::malloc_bytes_in_use();

        for path in changes.removed {
            self.retire(&path);
            self.known_binaries.remove(&path);
        }

        for (rel_path, stamp, embedding) in changes.upserts {
            // A file can cross between the two maps — a binary replaced by text.
            self.known_binaries.remove(&rel_path);
            let id = match self.path_to_idx.get(&rel_path) {
                Some(&id) => id,
                None => {
                    let id = self
                        .free_ids
                        .pop()
                        .map_or(self.entries.len(), |slot| slot as usize);
                    if id == self.entries.len() {
                        self.entries.push(None);
                    }
                    self.path_to_idx.insert(rel_path.clone(), id);
                    id
                }
            };
            self.engine.scorer.upsert(&(id as u32), embedding);
            self.entries[id] = Some(FileEntry { rel_path, stamp });
        }

        for (rel_path, stamp) in changes.unindexable {
            // Text that became binary has to leave the scorer, or a search still
            // returns it and `is_current` counts the same path in both maps.
            self.retire(&rel_path);
            self.known_binaries.insert(rel_path, stamp);
        }

        if let Some((after, before)) = crate::memory_report::malloc_bytes_in_use().zip(heap_before)
        {
            let delta = after as i64 - before as i64;
            self.engine_bytes = (self.engine_bytes as i64 + delta).max(0) as usize;
        }
        self.built_at = std::time::Instant::now();
    }

    /// Drop a path from the text index, freeing its slot for reuse. A no-op for a
    /// path the index does not hold as text.
    fn retire(&mut self, rel_path: &str) {
        if let Some(id) = self.path_to_idx.remove(rel_path) {
            self.entries[id] = None;
            self.free_ids.push(id as u32);
            self.engine.scorer.remove(&(id as u32));
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

        self.engine
            .search(query, limit)
            .into_iter()
            .filter_map(|r| {
                self.entries
                    .get(r.id as usize)
                    .and_then(|slot| slot.as_ref())
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
        self.live_len()
    }

    /// The custom engine retains embeddings and ids only; there is no document
    /// store whose byte count could grow with the source corpus.
    #[cfg(test)]
    fn retained_document_text_bytes(&self) -> usize {
        self.engine
            .build_cache
            .lock()
            .as_ref()
            .map_or(0, |cache| cache.values().flatten().map(String::len).sum())
    }

    #[cfg(test)]
    fn build_document_tokenizations(&self) -> usize {
        self.engine.build_document_tokenizations
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
///
/// Every build ends by enforcing the memory budget, which is why `state` is
/// threaded here rather than into each caller's closure: a build is the only
/// event that adds bytes to `content_indices`, so it is the only place the total
/// can newly exceed the budget. Enforcing it here also means the two build paths
/// (`ensure_index` and `rebuild_index`) cannot drift apart on the bound.
fn spawn_build<F>(
    rt: &tokio::runtime::Handle,
    state: Arc<crate::state::AppState>,
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
        // Before the budget runs, or this repo reads as in-flight and its bytes —
        // the ones just added — are left out of the total they should dominate.
        if let Some(set) = in_flight {
            set.remove(&repo);
        }
        enforce_memory_budget(&state, &repo);
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
        Entry::Occupied(e) => {
            // A hit here is a use: the repo was searched, switched to, or asked
            // for by an agent. Without it a warm index that callers keep reaching
            // for still looks idle to the budget and is evicted under one that
            // was merely built later.
            let index = Arc::clone(e.get());
            index.read().touch();
            return index;
        }
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
    let data_dir = state.data_dir.clone();
    let throttle = Arc::clone(&state.indexer_throttle);
    let in_flight = Arc::clone(&state.index_in_flight);
    let sem = Arc::clone(&state.index_build_sem);
    let repo_for_log = repo.clone();
    #[cfg(feature = "desktop")]
    let rt = tauri::async_runtime::handle();
    #[cfg(not(feature = "desktop"))]
    let rt = tokio::runtime::Handle::current();

    spawn_build(
        #[cfg(feature = "desktop")]
        rt.inner(),
        #[cfg(not(feature = "desktop"))]
        &rt,
        Arc::clone(state),
        repo_for_log,
        move || {
            // A snapshot left by an earlier eviction is the cheap way in. It may
            // be behind the repo, so it goes through the same update path a live
            // index uses — which either applies the diff or rebuilds outright.
            // Nothing here can serve content that disagrees with disk.
            match restore_reconciled(&data_dir, &repo, Some(&throttle)) {
                Some(reconciled) => {
                    *index_ref.write() = reconciled;
                    tracing::info!(repo = %repo, "content index restored from snapshot");
                }
                None => {
                    *index_ref.write() =
                        ContentIndex::build(PathBuf::from(&repo), Some(&throttle), HashMap::new());
                    tracing::info!(repo = %repo, "content index built");
                }
            }
        },
        Some(in_flight),
        sem,
    );

    index
}

/// Warm the index for a repo the user just switched to — the "and switch" half
/// of the `active_and_switch` strategy, whose boot half lives in `lib.rs`.
///
/// Gated here rather than at the call site: the decision is a config policy, and
/// both the IPC command and the HTTP route must make it the same way. `disabled`
/// and `active_only` are the two strategies that say "nothing beyond the boot
/// repo"; anything else (including an unrecognised value) gets the documented
/// default behaviour.
///
/// Cheap to call on every switch: `ensure_index` returns the existing entry
/// without spawning when the repo is already indexed or already building, and a
/// genuine build still queues behind the single global build semaphore.
///
/// Reads the in-memory config, never `load_app_config()` — same reason as the
/// `RepoChanged` arm below: that takes a cross-process file lock.
pub fn warm_index(state: &Arc<crate::state::AppState>, repo_path: &str) {
    let strategy = state.config.read().index_strategy.clone();
    if matches!(strategy.as_str(), "disabled" | "active_only") {
        tracing::debug!(repo = %repo_path, %strategy, "content index warm skipped by strategy");
        return;
    }
    tracing::info!(repo = %repo_path, %strategy, "content index warm on repo switch");
    ensure_index(state, repo_path);
}

/// Drop least-recently-used indices until the rest fit the configured budget.
///
/// An index is created by `ensure_index` and released by nothing but
/// `repo_watcher::stop_watching`, so before this existed the only bound on the
/// total was how many repos the user ever touched. That was survivable while
/// `warm_content_index` had no caller and a session held exactly one index; the
/// switch was wired on 2026-09-06 and two days later the backend reached 40.7 GB
/// across seven indices, one of which was 1.9 GB on its own.
///
/// Eviction is cheap to be wrong about and expensive to skip: an evicted repo
/// reports as `repos_pending` to a cross-repo search and is rebuilt the next time
/// the user searches it or switches to it. Nothing serves a stale result.
///
/// Two indices are never evicted. `keep` is the one that just finished building —
/// evicting it would make the build that triggered this pointless. An in-flight
/// one is skipped because its `Arc` is already held by a builder that will write
/// into it, so removing the map entry only orphans the work.
///
/// A single index larger than the whole budget is kept rather than dropped: it
/// leaves the repo searchable and bounds the process at that one index, whereas
/// refusing it would silently remove content search from a real repo.
pub(crate) fn enforce_memory_budget(state: &Arc<crate::state::AppState>, keep: &str) {
    let budget = state
        .config
        .read()
        .index_memory_budget_mb
        .saturating_mul(1024 * 1024);

    // Snapshot first: the map must not be borrowed while entries are removed.
    let mut resident: Vec<(String, usize, u64)> = Vec::new();
    let mut total: usize = 0;
    for entry in state.content_indices.iter() {
        let path = entry.key().clone();
        if state.index_in_flight.contains(&path) {
            continue;
        }
        let index = entry.value().read();
        let bytes = index.approx_bytes();
        total += bytes;
        if path != keep {
            resident.push((path, bytes, index.last_used()));
        }
    }
    if total <= budget {
        return;
    }

    // Least recently used first — the ones the user has moved on from.
    resident.sort_by_key(|(_, _, last_used)| *last_used);
    for (path, bytes, _) in resident {
        if total <= budget {
            break;
        }
        // Snapshot on the way out, so switching back to this repo reloads it
        // instead of walking and re-embedding the whole corpus again. This runs on
        // the build task, never on a request path.
        if let Some((_, index)) = state.content_indices.remove(&path) {
            index.read().save_snapshot(&state.data_dir, &path);
        }
        total = total.saturating_sub(bytes);
        tracing::info!(
            repo = %path,
            freed_bytes = bytes,
            remaining_bytes = total,
            budget_bytes = budget,
            "content index evicted to stay within the memory budget"
        );
    }
}

/// Rebuild the content index for a repo (called on RepoChanged events).
/// Runs in background, does not block. Skips if a build is already in-flight
/// for this repo (via `state.index_in_flight`) — the next `RepoChanged` will
/// pick up any missed changes.
pub fn rebuild_index(state: &Arc<crate::state::AppState>, repo_path: &str) {
    let in_flight = &state.index_in_flight;
    let index = if let Some(existing) = state.content_indices.get(repo_path) {
        Arc::clone(existing.value())
    } else {
        return;
    };

    {
        let idx = index.read();
        if idx.ready
            && (idx.built_at.elapsed() < REBUILD_COOLDOWN
                || idx
                    .validated_at
                    .is_some_and(|at| at.elapsed() < REBUILD_COOLDOWN))
        {
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
    #[cfg(feature = "desktop")]
    let rt = tauri::async_runtime::handle();
    #[cfg(not(feature = "desktop"))]
    let rt = tokio::runtime::Handle::current();

    spawn_build(
        #[cfg(feature = "desktop")]
        rt.inner(),
        #[cfg(not(feature = "desktop"))]
        &rt,
        Arc::clone(state),
        repo_for_log,
        move || rebuild_in_place(&index, &repo, Some(&throttle)),
        Some(Arc::clone(in_flight)),
        sem,
    );
}

/// Bring `index` back in line with `repo` on disk. Blocking — runs on the build
/// pool.
///
/// Three paths, cheapest first: nothing moved, so nothing to do; a handful of
/// files moved, so only those are re-read and re-embedded; too much moved, so the
/// corpus is rebuilt and the embedder refitted.
///
/// This exists as its own function because it is the whole point of the rebuild
/// path and has to be testable without a 60-second cooldown and a background task
/// in the way.
///
/// Both cheap paths hold only the read lock while walking, and every write is
/// O(changes). Only one builder per repo runs at a time (`index_in_flight`), so
/// nothing else can replace the index between planning and applying.
/// Restore `repo`'s snapshot and bring it in line with disk, or `None` when
/// there is no snapshot to restore.
///
/// Reconciles in a private lock and hands back a finished index, so the caller
/// publishes exactly once. Assigning the snapshot to the shared entry first and
/// reconciling afterwards published a `ready: true` index that still described
/// the snapshot, and every consumer checks `is_ready()` and nothing else: for
/// the length of the reconciliation a search answered from a corpus predating
/// the last commits and reported a file it could not find as absent.
fn restore_reconciled(
    data_dir: &Path,
    repo: &str,
    throttle: Option<&IndexerThrottle>,
) -> Option<ContentIndex> {
    let staging = parking_lot::RwLock::new(ContentIndex::restore(data_dir, repo)?);
    rebuild_in_place(&staging, repo, throttle);
    Some(staging.into_inner())
}

fn rebuild_in_place(
    index: &parking_lot::RwLock<ContentIndex>,
    repo: &str,
    throttle: Option<&IndexerThrottle>,
) {
    let (plan, prior_binaries) = {
        let idx = index.read();
        if idx.is_current() {
            drop(idx);
            // A completed no-op walk costs the same metadata I/O as any other
            // validation. Re-arm admission without pretending the corpus changed.
            index.write().validated_at = Some(std::time::Instant::now());
            tracing::debug!(repo = %repo, "content index rebuild skipped (no indexable change)");
            return;
        }
        (idx.plan_disk_changes(throttle), idx.known_binaries.clone())
    };

    if let Some(changes) = plan {
        // `is_current` said something moved, so an empty plan means the two
        // disagree — rebuild rather than silently leave the index stale.
        if !changes.is_empty() {
            let touched = changes.len();
            index.write().apply_disk_changes(changes);
            tracing::debug!(repo = %repo, files = touched, "content index updated incrementally");
            return;
        }
        tracing::debug!(repo = %repo, "content index reported stale with no change to apply");
    }

    let built = ContentIndex::build(PathBuf::from(repo), throttle, prior_binaries);
    *index.write() = built;
    tracing::debug!(repo = %repo, "content index rebuilt");
}

/// Spawn a background task that listens to the event bus and rebuilds
/// content indices when repos change. Should be called once at startup.
pub fn spawn_content_index_updater(state: Arc<crate::state::AppState>) {
    let mut rx = state.event_bus.subscribe();
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                // Both kinds, deliberately: a `git checkout` is git-state and
                // rewrites indexable files wholesale, so narrowing this to
                // working-tree would leave the index describing the old branch.
                Ok(crate::state::AppEvent::RepoChanged { repo_path, .. }) => {
                    crate::fs::invalidate_filename_inventory(&repo_path);
                    // The in-memory cache, NOT `load_app_config()`: that holds the
                    // config mutex and a cross-process *file* lock across the whole
                    // read, and this arm runs on every RepoChanged — hundreds an
                    // hour per repo. `state.config` is kept current by every save.
                    if state.config.read().index_strategy != "disabled" {
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
                    crate::fs::invalidate_all_filename_inventories();
                    tracing::warn!(source = "content_index", lagged = n, "event bus lagged");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

// ---------------------------------------------------------------------------
// On-disk snapshots
// ---------------------------------------------------------------------------

/// Format tag. The trailing digit is the layout version: bump it for any change
/// to the byte layout below, so an old file is rejected instead of misread.
const SNAPSHOT_MAGIC: &[u8; 8] = b"TUICIDX1";

/// Where a repo's snapshot lives.
///
/// Named by digest, not by a sanitised repo path: paths contain separators, run
/// past filename limits, and two repos that sanitise to the same name would
/// silently share one snapshot.
///
// DEFERRED (2026-09-10) — nothing removes the snapshot of a repo the user has
// unregistered. One file per repo ever indexed, tens of MB each, so the ceiling
// is the number of repos and not a leak. Revisit if a real profile shows this
// directory growing past the indices it stands in for.
fn snapshot_path(data_dir: &Path, repo: &str) -> PathBuf {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(repo.as_bytes());
    let mut name = String::with_capacity(36);
    for byte in digest.iter().take(16) {
        name.push_str(&format!("{byte:02x}"));
    }
    name.push_str(".idx");
    data_dir.join("content-index").join(name)
}

/// Sequential reader over a snapshot. Every read is bounds-checked and returns
/// `None` past the end, so a truncated or corrupt file fails the parse instead of
/// producing an index that disagrees with the repo.
struct SnapshotReader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> SnapshotReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.at.checked_add(n)?;
        let slice = self.bytes.get(self.at..end)?;
        self.at = end;
        Some(slice)
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }

    fn f32(&mut self) -> Option<f32> {
        Some(f32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn string(&mut self) -> Option<String> {
        let len = self.u32()? as usize;
        String::from_utf8(self.take(len)?.to_vec()).ok()
    }

    fn stamp(&mut self) -> Option<FileStamp> {
        Some(FileStamp {
            mtime: self.u64()?,
            len: self.u64()?,
        })
    }

    fn finished(&self) -> bool {
        self.at == self.bytes.len()
    }
}

fn put_string(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u32).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

fn put_stamp(out: &mut Vec<u8>, stamp: &FileStamp) {
    out.extend_from_slice(&stamp.mtime.to_le_bytes());
    out.extend_from_slice(&stamp.len.to_le_bytes());
}

impl ContentIndex {
    /// Serialise the index.
    ///
    /// A hand-rolled little-endian layout rather than the `serde_json` used
    /// elsewhere in the app, because the bulk of this is a few million
    /// `(u32, f32)` pairs: JSON spends roughly 20 bytes per pair against 8 here,
    /// and parses each one by decimal conversion instead of a copy. A snapshot
    /// that takes seconds to read is not worth having over a rebuild.
    ///
    /// `k1` and `b` are not stored: both paths that construct an `Embedder` leave
    /// the crate's defaults alone, so a stored copy could only ever disagree with
    /// the code that reads it.
    fn to_snapshot_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(SNAPSHOT_MAGIC);
        out.extend_from_slice(&self.engine.embedder.avgdl().to_le_bytes());
        out.extend_from_slice(&(self.engine_bytes as u64).to_le_bytes());
        out.extend_from_slice(&(self.entries.len() as u32).to_le_bytes());

        let embeddings: HashMap<u32, &bm25::Embedding<u32>> = self
            .engine
            .scorer
            .embeddings()
            .map(|(id, embedding)| (*id, embedding))
            .collect();

        // Documents, keyed by slot so the holes left by deletions survive the
        // round trip — a compacted snapshot would hand every id to a different
        // file on reload.
        let live: Vec<(u32, &FileEntry, &bm25::Embedding<u32>)> = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(id, slot)| {
                let entry = slot.as_ref()?;
                let embedding = embeddings.get(&(id as u32))?;
                Some((id as u32, entry, *embedding))
            })
            .collect();
        out.extend_from_slice(&(live.len() as u32).to_le_bytes());
        out.extend_from_slice(&(self.known_binaries.len() as u32).to_le_bytes());

        for (id, entry, embedding) in live {
            out.extend_from_slice(&id.to_le_bytes());
            put_stamp(&mut out, &entry.stamp);
            put_string(&mut out, &entry.rel_path);
            out.extend_from_slice(&(embedding.0.len() as u32).to_le_bytes());
            for token in &embedding.0 {
                out.extend_from_slice(&token.index.to_le_bytes());
                out.extend_from_slice(&token.value.to_le_bytes());
            }
        }

        for (rel_path, stamp) in &self.known_binaries {
            put_stamp(&mut out, stamp);
            put_string(&mut out, rel_path);
        }
        out
    }

    /// Rebuild an index from `to_snapshot_bytes`, or `None` if the bytes are not
    /// a snapshot this build understands.
    ///
    /// Nothing here trusts the file. A snapshot that fails to parse costs one
    /// rebuild, which is exactly what would have happened without it.
    fn from_snapshot_bytes(bytes: &[u8], repo_root: PathBuf) -> Option<Self> {
        let mut r = SnapshotReader::new(bytes);
        if r.take(SNAPSHOT_MAGIC.len())? != SNAPSHOT_MAGIC {
            return None;
        }
        let avgdl = r.f32()?;
        if !avgdl.is_finite() || avgdl <= 0.0 {
            return None;
        }
        let engine_bytes = r.u64()? as usize;
        let slot_count = r.u32()? as usize;
        let doc_count = r.u32()? as usize;
        let binary_count = r.u32()? as usize;
        if doc_count > slot_count {
            return None;
        }

        let embedder = EmbedderBuilder::<u32, BuildTokenizer>::with_avgdl(avgdl).build();
        let mut scorer = Scorer::new();
        let mut entries: Vec<Option<FileEntry>> = vec![None; slot_count];
        let mut path_to_idx = HashMap::with_capacity(doc_count);

        for _ in 0..doc_count {
            let id = r.u32()? as usize;
            let stamp = r.stamp()?;
            let rel_path = r.string()?;
            let token_count = r.u32()? as usize;
            // Guard before allocating: a corrupt count must not ask for gigabytes.
            let mut tokens = Vec::with_capacity(token_count.min(bytes.len() / 8));
            for _ in 0..token_count {
                tokens.push(bm25::TokenEmbedding {
                    index: r.u32()?,
                    value: r.f32()?,
                });
            }
            if entries.get(id)?.is_some() {
                return None; // two documents claiming one slot
            }
            scorer.upsert(&(id as u32), bm25::Embedding(tokens));
            path_to_idx.insert(rel_path.clone(), id);
            entries[id] = Some(FileEntry { rel_path, stamp });
        }

        let mut known_binaries = HashMap::with_capacity(binary_count);
        for _ in 0..binary_count {
            let stamp = r.stamp()?;
            known_binaries.insert(r.string()?, stamp);
        }
        // Trailing bytes mean the writer and this reader disagree about the layout.
        if !r.finished() {
            return None;
        }

        let free_ids = entries
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.is_none())
            .map(|(id, _)| id as u32)
            .collect();

        Some(Self {
            engine: EmbeddingIndex {
                embedder,
                scorer,
                #[cfg(test)]
                build_cache: Arc::new(parking_lot::Mutex::new(None)),
                #[cfg(test)]
                build_document_tokenizations: 0,
            },
            entries,
            free_ids,
            path_to_idx,
            repo_root,
            ready: true,
            built_at: std::time::Instant::now(),
            validated_at: None,
            known_binaries,
            engine_bytes,
            last_used: AtomicU64::new(next_access_tick()),
        })
    }

    /// Write this index next to the app's data, so a later `restore` can skip the
    /// rebuild. Errors are logged and swallowed: a snapshot is an optimisation,
    /// and failing to write one must never fail the build that produced it.
    ///
    /// Written to a temporary file and renamed, so a crash mid-write leaves the
    /// previous snapshot intact rather than a half-file that parses to a
    /// plausible-looking index.
    fn save_snapshot(&self, data_dir: &Path, repo: &str) {
        if !self.ready {
            return;
        }
        let path = snapshot_path(data_dir, repo);
        let Some(parent) = path.parent() else { return };
        if let Err(e) = std::fs::create_dir_all(parent) {
            tracing::warn!(repo = %repo, error = %e, "content index snapshot directory unavailable");
            return;
        }
        let temporary = path.with_extension("idx.partial");
        let bytes = self.to_snapshot_bytes();
        let written = bytes.len();
        if let Err(e) =
            std::fs::write(&temporary, bytes).and_then(|()| std::fs::rename(&temporary, &path))
        {
            tracing::warn!(repo = %repo, error = %e, "content index snapshot not written");
            let _ = std::fs::remove_file(&temporary);
            return;
        }
        tracing::debug!(repo = %repo, bytes = written, "content index snapshot written");
    }

    /// Load a repo's snapshot, or `None` when there is none this build can use.
    ///
    /// The result may be behind the repo — the caller brings it up to date through
    /// the same `rebuild_in_place` path a live index uses, so a snapshot can never
    /// serve content that disagrees with disk.
    fn restore(data_dir: &Path, repo: &str) -> Option<Self> {
        let path = snapshot_path(data_dir, repo);
        let bytes = std::fs::read(&path).ok()?;
        match Self::from_snapshot_bytes(&bytes, PathBuf::from(repo)) {
            Some(index) => Some(index),
            None => {
                // Unusable and it will stay unusable; leaving it means paying the
                // read on every restore for the life of the repo.
                tracing::warn!(repo = %repo, "content index snapshot unreadable, discarding");
                let _ = std::fs::remove_file(&path);
                None
            }
        }
    }
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
    fn build_does_not_retain_document_text() {
        let repo = make_test_repo();
        let index = ContentIndex::build(repo.path().to_path_buf(), None, HashMap::new());

        assert_eq!(
            index.retained_document_text_bytes(),
            0,
            "search needs embeddings and file ids, not a second in-memory copy of every file"
        );
    }

    #[test]
    fn build_tokenizes_each_document_once() {
        let repo = make_test_repo();
        let index = ContentIndex::build(repo.path().to_path_buf(), None, HashMap::new());

        assert_eq!(
            index.build_document_tokenizations(),
            index.len(),
            "fitting avgdl and creating embeddings must share the first tokenization"
        );
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

    // Catches: checkpoint waits for a fallback search guard to drop, starving the build.
    #[test]
    fn a_fallback_search_guard_cannot_starve_an_index_checkpoint() {
        let throttle = Arc::new(IndexerThrottle::default());
        let guard = throttle.begin_search();
        // Keep the search active until checkpoint returns. A starvation regression
        // hangs here and is caught by nextest's outer timeout, without confusing
        // worker scheduling under CI load with checkpoint's bounded deferral.
        throttle.checkpoint();
        drop(guard);
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

    /// Most `RepoChanged` events do not touch indexable content: `git add`,
    /// `git commit`, a stash, a branch ref move — every git-state change emits one
    /// while the working tree's bytes are unchanged. Each of those used to
    /// schedule a full re-read of every text file in the repo plus a complete BM25
    /// corpus rebuild, once a minute, forever. A stat-only walk answers "did
    /// anything indexable change" for a fraction of the cost.
    #[test]
    fn is_current_detects_edits_additions_and_deletions() {
        let repo = make_test_repo();
        let index = ContentIndex::build(repo.path().to_path_buf(), None, HashMap::new());

        assert!(
            index.is_current(),
            "nothing changed since the build — a rebuild would be pure waste"
        );

        fs::write(repo.path().join("main.rs"), "fn main() { /* edited */ }").unwrap();
        assert!(
            !index.is_current(),
            "an edited indexed file must invalidate"
        );

        let index = ContentIndex::build(repo.path().to_path_buf(), None, HashMap::new());
        fs::write(repo.path().join("brand_new.rs"), "fn brand_new() {}").unwrap();
        assert!(!index.is_current(), "a new file must invalidate");

        let index = ContentIndex::build(repo.path().to_path_buf(), None, HashMap::new());
        fs::remove_file(repo.path().join("brand_new.rs")).unwrap();
        assert!(!index.is_current(), "a deleted file must invalidate");
    }

    /// A file the build could not decode is in neither `entries` nor
    /// `known_binaries`, so `is_current` would count it as never-seen and report
    /// stale forever — silently reverting this repo to a full rebuild a minute,
    /// with nothing to show why. Latin-1 text whose first 8 KB has no null byte
    /// is exactly that: not binary by the probe, not valid UTF-8 either.
    #[test]
    fn is_current_is_stable_for_a_file_that_cannot_be_decoded() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("ok.rs"), "fn ok() {}").unwrap();
        // No null bytes (so `is_binary` says text) but invalid UTF-8 past 8 KB.
        let mut latin1 = vec![b'a'; 9000];
        latin1.extend_from_slice(&[0xE9, 0xE8, 0xFF]);
        fs::write(root.join("latin1.txt"), &latin1).unwrap();

        let index = ContentIndex::build(root.to_path_buf(), None, HashMap::new());
        assert!(
            index.is_current(),
            "an undecodable file must not make the index look permanently stale"
        );
    }

    /// An edit landing in the same wall-clock second as the build must still
    /// invalidate. Second-granularity mtimes silently miss those, and an agent
    /// editing a file twice in a second is the normal case here.
    #[test]
    fn is_current_detects_a_same_second_edit() {
        let repo = make_test_repo();
        let index = ContentIndex::build(repo.path().to_path_buf(), None, HashMap::new());
        fs::write(repo.path().join("lib.rs"), "pub fn add() -> i32 { 0 }").unwrap();
        assert!(
            !index.is_current(),
            "an edit within the same second as the build must invalidate"
        );
    }

    /// A restore that preserves timestamps — `cp -p`, `rsync -a`, `tar -x`,
    /// unpacking a build cache — replaces the content while leaving mtime exactly
    /// as the build recorded it. On mtime alone the index reports itself current
    /// and keeps serving the old text for as long as nothing else touches the
    /// file. The size is stat'd by the same walk, so comparing it costs nothing.
    #[test]
    fn is_current_detects_a_replacement_that_preserved_the_mtime() {
        let repo = make_test_repo();
        let target = repo.path().join("main.rs");
        let original_mtime = fs::metadata(&target).unwrap().modified().unwrap();

        let index = ContentIndex::build(repo.path().to_path_buf(), None, HashMap::new());
        assert!(index.is_current());

        // Different content, different length, mtime restored to the indexed value.
        fs::write(
            &target,
            "fn main() { println!(\"restored from an archive\"); }",
        )
        .unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(&target)
            .unwrap()
            .set_modified(original_mtime)
            .unwrap();
        assert_eq!(
            fs::metadata(&target).unwrap().modified().unwrap(),
            original_mtime,
            "the test must actually restore the mtime, or it proves nothing"
        );

        assert!(
            !index.is_current(),
            "content replaced under a preserved mtime must invalidate"
        );
    }

    /// Publication is a single assignment because reconciliation happens off to
    /// the side. The entry is published `ready: true` and every consumer checks
    /// `is_ready()` and nothing else, so reconciling after publication left a
    /// window in which a search answered from the snapshot's corpus and reported
    /// a file committed since as absent.
    #[test]
    fn a_restored_snapshot_agrees_with_disk_before_it_is_handed_back() {
        let repo = make_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();
        let data_dir = tempfile::tempdir().unwrap();
        ContentIndex::build(repo.path().to_path_buf(), None, HashMap::new())
            .save_snapshot(data_dir.path(), &repo_path);

        // A file lands after the snapshot — exactly what reconciliation exists
        // to pick up, and what a prematurely published index cannot find.
        fs::write(repo.path().join("afterwards.rs"), "fn quokka_marker() {}").unwrap();

        let restored = restore_reconciled(data_dir.path(), &repo_path, None)
            .expect("a snapshot was written just above");

        assert!(restored.is_ready());
        assert!(
            restored.is_current(),
            "an index handed back for publication must already agree with disk"
        );
        assert!(
            restored
                .search("quokka_marker", 5)
                .iter()
                .any(|hit| hit.rel_path == "afterwards.rs"),
            "a file added after the snapshot must be searchable the moment the index is published"
        );
    }

    /// Catches a no-op validation leaving the cooldown expired, so each later
    /// RepoChanged schedules another full stat walk despite unchanged content.
    #[tokio::test]
    async fn unchanged_validation_rearms_repo_changed_cooldown() {
        let repo = make_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mut built = ContentIndex::build(repo.path().to_path_buf(), None, HashMap::new());
        built.built_at = std::time::Instant::now() - REBUILD_COOLDOWN;
        let index = Arc::new(parking_lot::RwLock::new(built));
        state
            .content_indices
            .insert(repo_path.clone(), Arc::clone(&index));

        rebuild_in_place(&index, &repo_path, None);
        rebuild_index(&state, &repo_path);

        assert!(
            !state.index_in_flight.contains(&repo_path),
            "an unchanged validation must suppress the next RepoChanged before scheduling any walk"
        );

        // Expiry must still admit a later event; no-op suppression is not a
        // permanent exemption from checking working-tree content.
        index.write().validated_at = Some(std::time::Instant::now() - REBUILD_COOLDOWN);
        rebuild_index(&state, &repo_path);
        assert!(state.index_in_flight.contains(&repo_path));
    }

    #[test]
    fn rebuild_in_place_leaves_an_unchanged_index_alone() {
        let repo = make_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();
        let index = parking_lot::RwLock::new(ContentIndex::build(
            repo.path().to_path_buf(),
            None,
            HashMap::new(),
        ));
        let built_at = index.read().built_at;

        rebuild_in_place(&index, &repo_path, None);
        assert_eq!(
            index.read().built_at,
            built_at,
            "an unchanged repo must not be re-indexed"
        );

        fs::write(repo.path().join("main.rs"), "fn main() { /* edited */ }").unwrap();
        rebuild_in_place(&index, &repo_path, None);
        assert!(
            index.read().built_at > built_at,
            "a real content change must still be re-indexed"
        );
    }

    /// The updater must read `index_strategy` from the in-memory config, not from
    /// disk. `load_app_config` takes the in-process config mutex AND a
    /// cross-process file lock for the whole read, and this ran once per
    /// `RepoChanged` — hundreds of times an hour during a working-tree storm,
    /// serialising against every other config reader and writer in both the
    /// release app and a `make dev` build sharing the config dir.
    #[tokio::test]
    async fn repo_changed_reads_index_strategy_from_the_in_memory_config() {
        let cfg_dir = tempfile::tempdir().unwrap();
        // On-disk config keeps indexing enabled (the default), so reading the
        // file instead of the cache is observable as a rebuild that should not
        // have happened.
        let _guard = crate::config::set_config_dir_override(cfg_dir.path().to_path_buf());

        let repo = make_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        state.config.write().index_strategy = "disabled".to_string();
        let index = Arc::new(parking_lot::RwLock::new(ContentIndex::empty(
            repo.path().to_path_buf(),
        )));
        state
            .content_indices
            .insert(repo_path.clone(), Arc::clone(&index));

        spawn_content_index_updater(Arc::clone(&state));
        // Let the subscriber attach before the send — a broadcast delivers only
        // to receivers that already exist.
        tokio::time::sleep(Duration::from_millis(50)).await;
        let _ = state.event_bus.send(crate::state::AppEvent::RepoChanged {
            repo_path: repo_path.clone(),
            kind: crate::repo_watcher::RepoChangeKind::WorkingTree,
        });
        tokio::time::sleep(Duration::from_millis(500)).await;

        assert!(
            !index.read().is_ready(),
            "index_strategy=disabled in the live config must suppress the rebuild"
        );
    }

    /// `warm_index` is the "and switch" half of `active_and_switch`. It runs on a
    /// user action (switching repo), so the strategy setting is the only thing
    /// standing between "index this one repo" and "index on every click even
    /// though the user asked for active_only".
    #[tokio::test]
    async fn warm_index_builds_under_active_and_switch() {
        let cfg_dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(cfg_dir.path().to_path_buf());

        let repo = make_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        state.config.write().index_strategy = "active_and_switch".to_string();

        warm_index(&state, &repo_path);

        let index = state
            .content_indices
            .get(&repo_path)
            .map(|e| Arc::clone(e.value()))
            .expect("active_and_switch must schedule a build for the switched-to repo");
        for _ in 0..100 {
            if index.read().is_ready() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(
            index.read().is_ready(),
            "the switched-to repo must actually get indexed, not just registered"
        );
    }

    /// `active_only` and `disabled` both mean "do not index anything but the boot
    /// repo". A switch must stay a no-op for them, or the setting is decorative.
    #[tokio::test]
    async fn warm_index_is_a_no_op_for_strategies_that_exclude_switches() {
        let cfg_dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(cfg_dir.path().to_path_buf());

        for strategy in ["active_only", "disabled"] {
            let repo = make_test_repo();
            let repo_path = repo.path().to_string_lossy().to_string();
            let state = Arc::new(crate::state::tests_support::make_test_app_state());
            state.config.write().index_strategy = strategy.to_string();

            warm_index(&state, &repo_path);

            tokio::time::sleep(Duration::from_millis(100)).await;
            assert!(
                !state.content_indices.contains_key(&repo_path),
                "index_strategy={strategy} must not index a repo on switch"
            );
        }
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

    // --- Incremental update ---

    /// A repo with enough files that the incremental limit (a floor of 64) is not
    /// reached by the handful of edits these tests make.
    fn incremental_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..100 {
            fs::write(
                dir.path().join(format!("file_{i}.rs")),
                format!("fn routine_{i}() {{ let padding = {}; }}\n", i * 7),
            )
            .unwrap();
        }
        dir
    }

    fn built(root: &Path) -> parking_lot::RwLock<ContentIndex> {
        parking_lot::RwLock::new(ContentIndex::build(
            root.to_path_buf(),
            None,
            HashMap::new(),
        ))
    }

    fn hits(index: &parking_lot::RwLock<ContentIndex>, query: &str) -> Vec<String> {
        index
            .read()
            .search(query, 20)
            .into_iter()
            .map(|r| r.rel_path)
            .collect()
    }

    #[test]
    fn an_edited_file_is_re_embedded_without_rebuilding_the_corpus() {
        let dir = incremental_repo();
        let index = built(dir.path());
        let slots_before = index.read().entries.len();
        assert_eq!(hits(&index, "kumquat"), [] as [std::string::String; 0]);

        fs::write(
            dir.path().join("file_7.rs"),
            "fn routine_7() { let kumquat = 1; }\n",
        )
        .unwrap();
        rebuild_in_place(&index, dir.path().to_str().unwrap(), None);

        assert!(hits(&index, "kumquat").contains(&"file_7.rs".to_string()));
        assert_eq!(index.read().len(), 100, "an edit must not change the count");
        assert_eq!(
            index.read().entries.len(),
            slots_before,
            "an edit reuses the file's own slot"
        );
        assert!(index.read().is_current());
    }

    #[test]
    fn a_new_file_becomes_searchable_without_a_full_rebuild() {
        let dir = incremental_repo();
        let index = built(dir.path());

        fs::write(
            dir.path().join("added.rs"),
            "fn added() { let quokka = 2; }\n",
        )
        .unwrap();
        rebuild_in_place(&index, dir.path().to_str().unwrap(), None);

        assert!(hits(&index, "quokka").contains(&"added.rs".to_string()));
        assert_eq!(index.read().len(), 101);
        assert!(index.read().is_current());
    }

    #[test]
    fn a_deleted_file_leaves_the_index_and_frees_its_slot() {
        let dir = incremental_repo();
        let index = built(dir.path());
        let slots_before = index.read().entries.len();
        assert!(hits(&index, "routine_3").contains(&"file_3.rs".to_string()));

        fs::remove_file(dir.path().join("file_3.rs")).unwrap();
        rebuild_in_place(&index, dir.path().to_str().unwrap(), None);

        assert!(!hits(&index, "routine_3").contains(&"file_3.rs".to_string()));
        assert_eq!(index.read().len(), 99);
        assert_eq!(index.read().free_ids.len(), 1);
        assert!(index.read().is_current());

        // The next file added must land in the hole, not past the end — otherwise
        // a repo that churns files grows `entries` without bound.
        fs::write(
            dir.path().join("later.rs"),
            "fn later() { let axolotl = 3; }\n",
        )
        .unwrap();
        rebuild_in_place(&index, dir.path().to_str().unwrap(), None);

        assert!(hits(&index, "axolotl").contains(&"later.rs".to_string()));
        assert_eq!(index.read().entries.len(), slots_before);
        assert_eq!(index.read().free_ids, [] as [u32; 0]);
    }

    #[test]
    fn a_reused_slot_does_not_answer_for_the_file_that_freed_it() {
        let dir = incremental_repo();
        let index = built(dir.path());

        fs::remove_file(dir.path().join("file_3.rs")).unwrap();
        rebuild_in_place(&index, dir.path().to_str().unwrap(), None);
        fs::write(
            dir.path().join("later.rs"),
            "fn later() { let axolotl = 3; }\n",
        )
        .unwrap();
        rebuild_in_place(&index, dir.path().to_str().unwrap(), None);

        // The slot that held file_3 now holds later.rs. If `remove` had left the
        // old postings behind, the deleted file's terms would still match and
        // resolve through the slot to the new file's path.
        let stale = hits(&index, "routine_3");
        assert!(
            !stale.contains(&"later.rs".to_string()),
            "a freed slot must not carry the removed file's postings: {stale:?}"
        );
    }

    #[test]
    fn a_file_that_turns_binary_leaves_the_text_index() {
        let dir = incremental_repo();
        let index = built(dir.path());
        assert!(hits(&index, "routine_5").contains(&"file_5.rs".to_string()));

        fs::write(dir.path().join("file_5.rs"), [0u8, 1, 2, 3, 0, 4, 5]).unwrap();
        rebuild_in_place(&index, dir.path().to_str().unwrap(), None);

        assert!(!hits(&index, "routine_5").contains(&"file_5.rs".to_string()));
        assert_eq!(index.read().len(), 99);
        assert!(
            index.read().known_binaries.contains_key("file_5.rs"),
            "it must still be accounted for, or is_current reports stale forever"
        );
        assert!(index.read().is_current());
    }

    #[test]
    fn a_binary_file_that_turns_into_text_joins_the_index() {
        let dir = incremental_repo();
        fs::write(dir.path().join("blob.bin"), [0u8, 1, 2, 3]).unwrap();
        let index = built(dir.path());
        assert_eq!(index.read().len(), 100);

        fs::write(
            dir.path().join("blob.bin"),
            "fn now_text() { let wombat = 4; }\n",
        )
        .unwrap();
        rebuild_in_place(&index, dir.path().to_str().unwrap(), None);

        assert!(hits(&index, "wombat").contains(&"blob.bin".to_string()));
        assert!(!index.read().known_binaries.contains_key("blob.bin"));
        assert!(index.read().is_current());
    }

    #[test]
    fn a_change_set_over_the_limit_falls_back_to_a_full_rebuild() {
        let dir = incremental_repo();
        let index = built(dir.path());
        // 100 files: the limit is max(100/4, 64) = 64. Rewrite 70 of them.
        for i in 0..70 {
            fs::write(
                dir.path().join(format!("file_{i}.rs")),
                format!("fn routine_{i}() {{ let capybara = {}; }}\n", i * 3),
            )
            .unwrap();
        }

        rebuild_in_place(&index, dir.path().to_str().unwrap(), None);

        // Whichever path ran, the index must describe the repo.
        assert_eq!(index.read().len(), 100);
        assert!(index.read().is_current());
        assert_eq!(hits(&index, "capybara").len(), 20);
        assert!(
            index.read().free_ids.is_empty(),
            "a full rebuild assigns ids densely"
        );
    }

    #[test]
    fn the_incremental_limit_floors_at_64_so_small_repos_stay_cheap() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("only.rs"), "fn only() {}\n").unwrap();
        let index = built(dir.path());

        // A quarter of a one-file corpus is zero; without the floor every edit in
        // a small repo would pay for a full rebuild.
        assert_eq!(index.read().incremental_change_limit(), 64);
    }

    #[test]
    fn an_unbuilt_index_declines_the_incremental_path() {
        let dir = incremental_repo();
        let index = ContentIndex::empty(dir.path().to_path_buf());
        assert!(
            index.plan_disk_changes(None).is_none(),
            "there is no fitted embedder to upsert against before the first build"
        );
    }

    // --- Snapshots ---

    fn round_trip(index: &ContentIndex) -> ContentIndex {
        let bytes = index.to_snapshot_bytes();
        ContentIndex::from_snapshot_bytes(&bytes, index.repo_root.clone())
            .expect("a snapshot this build wrote must be one it can read")
    }

    #[test]
    fn a_snapshot_round_trip_preserves_every_ranking() {
        let dir = make_test_repo();
        let index = ContentIndex::build(dir.path().to_path_buf(), None, HashMap::new());
        let restored = round_trip(&index);

        for query in ["search", "add", "project indexing", "format uppercase"] {
            let before = index.search(query, 10);
            let after = restored.search(query, 10);
            assert_eq!(
                before.iter().map(|r| &r.rel_path).collect::<Vec<_>>(),
                after.iter().map(|r| &r.rel_path).collect::<Vec<_>>(),
                "ranking changed across the round trip for {query:?}"
            );
            for (b, a) in before.iter().zip(&after) {
                assert!(
                    (b.score - a.score).abs() < f32::EPSILON,
                    "score changed for {query:?}: {} vs {}",
                    b.score,
                    a.score
                );
            }
        }
        assert_eq!(restored.len(), index.len());
        assert!(restored.is_ready());
        assert!(restored.is_current());
    }

    #[test]
    fn a_snapshot_preserves_the_holes_left_by_deletions() {
        let dir = incremental_repo();
        let index = built(dir.path());
        fs::remove_file(dir.path().join("file_3.rs")).unwrap();
        fs::remove_file(dir.path().join("file_9.rs")).unwrap();
        rebuild_in_place(&index, dir.path().to_str().unwrap(), None);

        let restored = round_trip(&index.read());

        // Compacting the slots on the way out would hand every id after a hole to
        // a different file, and every search would then answer with the wrong path.
        assert_eq!(restored.entries.len(), index.read().entries.len());
        assert_eq!(restored.free_ids.len(), 2);
        assert_eq!(restored.len(), 98);
        for (path, &id) in &index.read().path_to_idx {
            assert_eq!(
                restored.path_to_idx.get(path),
                Some(&id),
                "slot moved for {path}"
            );
        }
    }

    #[test]
    fn a_snapshot_preserves_the_unindexable_files() {
        let dir = incremental_repo();
        fs::write(dir.path().join("blob.bin"), [0u8, 1, 2, 3]).unwrap();
        let index = ContentIndex::build(dir.path().to_path_buf(), None, HashMap::new());
        let restored = round_trip(&index);

        assert_eq!(restored.known_binaries, index.known_binaries);
        // Dropping them would make every later event report the index stale,
        // because `is_current` requires every file on disk to be accounted for.
        assert!(restored.is_current());
    }

    #[test]
    fn a_snapshot_that_is_not_one_is_refused() {
        let dir = make_test_repo();
        let index = ContentIndex::build(dir.path().to_path_buf(), None, HashMap::new());
        let good = index.to_snapshot_bytes();
        let root = || dir.path().to_path_buf();

        assert!(
            ContentIndex::from_snapshot_bytes(b"", root()).is_none(),
            "empty"
        );
        assert!(
            ContentIndex::from_snapshot_bytes(b"TUICIDX0somethingelse", root()).is_none(),
            "a layout from another version must be rejected, not misread"
        );
        assert!(
            ContentIndex::from_snapshot_bytes(&good[..good.len() / 2], root()).is_none(),
            "truncated"
        );
        let mut trailing = good.clone();
        trailing.push(0);
        assert!(
            ContentIndex::from_snapshot_bytes(&trailing, root()).is_none(),
            "trailing bytes mean the reader and writer disagree"
        );
    }

    /// A snapshot of `make_test_repo()`, captured with `to_snapshot_bytes()`
    /// *before* fxhash was vendored into `crate::fxhash` (story 758-ff0d). Every
    /// `token.index` inside it is a fxhash32 hash computed by the real,
    /// upstream `fxhash` 0.2.1 crate. If the vendored algorithm in
    /// `patches/bm25/src/fxhash.rs` ever drifts from it, this snapshot still
    /// decodes (the byte layout hasn't changed) but the token indices stop
    /// matching the query embedder's, so every search below returns nothing —
    /// the failure this test exists to catch.
    const PRE_FXHASH_VENDORING_SNAPSHOT_B64: &str = "VFVJQ0lEWDGamflA0GQAAAAAAAAFAAAABQAAAAAAAAAAAAAACgzSutzW1BgwAAAAAAAAAAYAAABsaWIucnMJAAAApF3WMJXXcD/XkEj6lddwP8E72BeV13A/tolAEpXXcD8ucLq6MrnCPyBSg11Ss6g/LnC6ujK5wj8ucLq6MrnCPyBSg11Ss6g/AQAAAICQCbvc1tQYMwAAAAAAAAAJAAAAUkVBRE1FLm1kBQAAAJ5wuRVlCJY/0MiDIsTDwz/QyIMixMPDP4rO8aBlCJY/v1Nwx2UIlj8CAAAAduu3utzW1BgrAAAAAAAAAAcAAABtYWluLnJzBgAAAIgOqyf4V40/wTvYF/hXjT/q6uV1+FeNP1uAKjH4V40/UGMDP/hXjT/vREmn+FeNPwMAAAAM0wK73NbUGF0AAAAAAAAACQAAAHNlYXJjaC5ycwsAAABJYOdtpzVbP9ZfG8inNVs/L7tiIQnLnT9KSCilpzVbP8E72BenNVs/15otfac1Wz8g/v5HpzVbP+BdR4unNVs/L7tiIQnLnT+KzvGgpzVbP/kcHuCnNVs/BAAAAKgDILvc1tQYQQAAAAAAAAAMAAAAc3JjL3V0aWxzLnJzCAAAAC9GuHqzV30/Cp4QzrNXfT/XkEj6s1d9P8E72BezV30/8e1iarNXfT/gXUeLs1d9P3MBlzOzV30/M9PJSrNXfT8=";

    #[test]
    fn a_pre_vendoring_snapshot_still_loads_and_searches_the_same() {
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        let bytes = STANDARD
            .decode(PRE_FXHASH_VENDORING_SNAPSHOT_B64)
            .expect("fixture is valid base64");
        assert_eq!(
            &bytes[..SNAPSHOT_MAGIC.len()],
            SNAPSHOT_MAGIC,
            "SNAPSHOT_MAGIC changed — the fxhash vendoring must be bit-identical, not a new layout"
        );

        let index = ContentIndex::from_snapshot_bytes(&bytes, PathBuf::from("/repo"))
            .expect("a pre-vendoring snapshot must still decode after the fxhash swap");

        for (query, expected) in [
            ("search", vec!["README.md", "search.rs"]),
            ("add", vec!["lib.rs"]),
            ("project indexing", vec!["README.md"]),
            ("hello world", vec!["main.rs"]),
            ("format uppercase", vec![]),
        ] {
            let results = index.search(query, 10);
            let got: Vec<&str> = results.iter().map(|r| r.rel_path.as_str()).collect();
            assert_eq!(got, expected, "search results changed for {query:?}");
        }
    }

    #[test]
    fn an_unreadable_snapshot_is_deleted_rather_than_re_read_forever() {
        let data_dir = tempfile::tempdir().unwrap();
        let path = snapshot_path(data_dir.path(), "/repo");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"not a snapshot").unwrap();

        assert!(ContentIndex::restore(data_dir.path(), "/repo").is_none());
        assert!(
            !path.exists(),
            "leaving it costs the read on every restore for the life of the repo"
        );
    }

    #[test]
    fn a_saved_snapshot_reloads_through_the_filesystem() {
        let repo = make_test_repo();
        let data_dir = tempfile::tempdir().unwrap();
        let key = repo.path().to_str().unwrap();
        let index = ContentIndex::build(repo.path().to_path_buf(), None, HashMap::new());
        index.save_snapshot(data_dir.path(), key);

        let restored = ContentIndex::restore(data_dir.path(), key).expect("written, so readable");
        assert_eq!(restored.len(), index.len());
        assert_eq!(
            restored
                .search("search", 5)
                .first()
                .map(|r| r.rel_path.clone()),
            index
                .search("search", 5)
                .first()
                .map(|r| r.rel_path.clone())
        );
    }

    #[test]
    fn two_repos_never_share_one_snapshot_file() {
        let data_dir = tempfile::tempdir().unwrap();
        assert_ne!(
            snapshot_path(data_dir.path(), "/a/project"),
            snapshot_path(data_dir.path(), "/b/project"),
        );
    }

    #[test]
    fn a_restored_snapshot_catches_up_with_a_repo_that_moved() {
        let repo = incremental_repo();
        let data_dir = tempfile::tempdir().unwrap();
        let key = repo.path().to_str().unwrap();
        ContentIndex::build(repo.path().to_path_buf(), None, HashMap::new())
            .save_snapshot(data_dir.path(), key);

        // The repo moves on while the index is only on disk.
        fs::write(
            repo.path().join("file_11.rs"),
            "fn routine_11() { let narwhal = 5; }\n",
        )
        .unwrap();
        fs::remove_file(repo.path().join("file_12.rs")).unwrap();

        let restored = ContentIndex::restore(data_dir.path(), key).expect("readable");
        assert!(!restored.is_current(), "the snapshot is behind on purpose");

        let index = parking_lot::RwLock::new(restored);
        rebuild_in_place(&index, key, None);

        assert!(hits(&index, "narwhal").contains(&"file_11.rs".to_string()));
        assert!(!hits(&index, "routine_12").contains(&"file_12.rs".to_string()));
        assert!(index.read().is_current());
    }

    #[test]
    fn eviction_writes_the_snapshot_that_makes_the_return_cheap() {
        let repo = make_test_repo();
        let key = repo.path().to_str().unwrap().to_string();
        let state = budget_state(1);
        state.content_indices.insert(
            key.clone(),
            Arc::new(parking_lot::RwLock::new(ContentIndex::build(
                repo.path().to_path_buf(),
                None,
                HashMap::new(),
            ))),
        );
        // Something heavier and newer forces the real index out.
        resident_index(&state, "/hog", 2_000_000);

        enforce_memory_budget(&state, "/hog");

        assert!(!state.content_indices.contains_key(&key), "evicted");
        assert!(
            snapshot_path(&state.data_dir, &key).exists(),
            "an eviction with no snapshot makes the user pay a full rebuild to come back"
        );
    }

    // --- Memory budget ---

    fn budget_state(budget_mb: usize) -> Arc<crate::state::AppState> {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        state.config.write().index_memory_budget_mb = budget_mb;
        state
    }

    /// A resident index of a known weight. `approx_bytes` sums `engine_bytes` and
    /// three maps that are empty here, so the weight is exactly what is asked for —
    /// the budget can then be crossed without building a repo big enough to cross it.
    fn resident_index(state: &Arc<crate::state::AppState>, repo: &str, bytes: usize) {
        let mut index = ContentIndex::empty(PathBuf::from(repo));
        index.engine_bytes = bytes;
        index.ready = true;
        state
            .content_indices
            .insert(repo.to_string(), Arc::new(parking_lot::RwLock::new(index)));
    }

    fn resident(state: &Arc<crate::state::AppState>, repo: &str) -> bool {
        state.content_indices.contains_key(repo)
    }

    #[test]
    fn budget_evicts_the_least_recently_used_first() {
        let state = budget_state(1);
        resident_index(&state, "/old", 500_000);
        resident_index(&state, "/recent", 500_000);
        resident_index(&state, "/built", 500_000);
        // Order matters, not the call: /recent was reached after /old.
        state.content_indices.get("/old").unwrap().read().touch();
        state.content_indices.get("/recent").unwrap().read().touch();

        enforce_memory_budget(&state, "/built");

        assert!(!resident(&state, "/old"), "the stalest index must go first");
        assert!(resident(&state, "/recent"));
        assert!(resident(&state, "/built"));
    }

    #[test]
    fn budget_stops_as_soon_as_the_total_fits() {
        let state = budget_state(1);
        resident_index(&state, "/a", 400_000);
        resident_index(&state, "/b", 400_000);
        resident_index(&state, "/built", 400_000);
        state.content_indices.get("/a").unwrap().read().touch();
        state.content_indices.get("/b").unwrap().read().touch();

        enforce_memory_budget(&state, "/built");

        // 1.2 MB against a 1 MB budget: dropping the stalest is enough, and the
        // second one must survive — an over-eager sweep costs a needless rebuild.
        assert!(!resident(&state, "/a"));
        assert!(resident(&state, "/b"));
        assert!(resident(&state, "/built"));
    }

    #[test]
    fn budget_never_evicts_the_index_that_just_built() {
        let state = budget_state(1);
        // The freshly built one is both the stalest and the heaviest: every rule
        // except the `keep` exemption would pick it.
        resident_index(&state, "/built", 2_000_000);
        state.content_indices.get("/built").unwrap().read().touch();
        resident_index(&state, "/other", 500_000);
        state.content_indices.get("/other").unwrap().read().touch();

        enforce_memory_budget(&state, "/built");

        assert!(
            resident(&state, "/built"),
            "evicting the build that triggered the sweep makes the build pointless"
        );
        assert!(!resident(&state, "/other"));
    }

    #[test]
    fn budget_never_evicts_a_build_in_flight() {
        let state = budget_state(1);
        resident_index(&state, "/loading", 500_000);
        resident_index(&state, "/idle", 900_000);
        resident_index(&state, "/built", 900_000);
        state.index_in_flight.insert("/loading".to_string());
        state
            .content_indices
            .get("/loading")
            .unwrap()
            .read()
            .touch();
        state.content_indices.get("/idle").unwrap().read().touch();

        enforce_memory_budget(&state, "/built");

        assert!(
            resident(&state, "/loading"),
            "a builder holds this Arc and will write into it; removing the entry orphans the work"
        );
        assert!(!resident(&state, "/idle"));
        assert!(resident(&state, "/built"));
    }

    #[test]
    fn budget_keeps_one_index_that_is_larger_than_the_whole_budget() {
        let state = budget_state(1);
        resident_index(&state, "/huge", 5_000_000);

        enforce_memory_budget(&state, "/huge");

        assert!(
            resident(&state, "/huge"),
            "dropping it would silently remove content search from a real repo"
        );
    }

    #[test]
    fn budget_leaves_everything_alone_under_the_bound() {
        let state = budget_state(1);
        resident_index(&state, "/a", 100_000);
        resident_index(&state, "/b", 100_000);

        enforce_memory_budget(&state, "/a");

        assert!(resident(&state, "/a"));
        assert!(resident(&state, "/b"));
    }

    #[test]
    fn a_hit_on_ensure_index_counts_as_a_use() {
        let state = budget_state(1);
        resident_index(&state, "/warm", 100_000);
        let before = state
            .content_indices
            .get("/warm")
            .unwrap()
            .read()
            .last_used();

        // The occupied branch returns without spawning, so this needs no runtime.
        ensure_index(&state, "/warm");

        let after = state
            .content_indices
            .get("/warm")
            .unwrap()
            .read()
            .last_used();
        assert!(
            after > before,
            "a warm index callers keep reaching for must not look idle to the budget"
        );
    }
}
