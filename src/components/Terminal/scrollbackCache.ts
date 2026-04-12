/**
 * LRU-bounded, chunk-paginated cache for VtLogBuffer lines used by the
 * scrollback overlay.
 *
 * The cache is fed by an injected `fetcher` (so the component can plug in
 * either the Tauri `read_vt_log` command or the browser HTTP endpoint)
 * and keeps a bounded number of chunks in memory. Chunks are sized so
 * that typical wheel-scrolls hit a single chunk (200 lines default) and
 * virtualization can render only the DOM nodes currently visible.
 *
 * Memory model:
 * - `chunkSize` lines per chunk, default 200.
 * - `maxChunks` resident chunks, default 10 → ~2000 lines in memory.
 * - Least-Recently-Accessed chunk is evicted on insertion overflow.
 * - `setOldest()` invalidates chunks that fall entirely below the new
 *   server-side `oldest_offset` (i.e. PTY ring rotation evicted them).
 *
 * Live growth:
 * - `setTotal()` bumps the known `total` but never invalidates cached
 *   chunks — the scrollback layer uses it only to size the virtual
 *   scrollbar. New lines are fetched on demand when the user scrolls
 *   them into view.
 *
 * Dedup:
 * - Concurrent `ensureLoaded` calls that target the same chunk share a
 *   single in-flight Promise so the fetcher is never called twice for
 *   the same chunk.
 */

import type { LogLine } from "../../mobile/utils/logLine";

/** Response shape returned by the fetcher (matches Rust `VtLogChunk`). */
export interface VtLogChunk {
  lines: LogLine[];
  /** Monotonically increasing total of lines ever pushed to VtLogBuffer. */
  total: number;
  /** Absolute offset of the oldest retained line in the buffer. */
  oldest: number;
}

export type CacheEvent =
  | { type: "total"; total: number }
  | { type: "oldest"; oldest: number }
  | { type: "chunkLoaded"; chunkIdx: number };

export interface ScrollbackCacheOptions {
  /** Lines per chunk. Default: 200. */
  chunkSize: number;
  /** Max resident chunks (LRU window). Default: 10. */
  maxChunks: number;
  /** Data source — receives `(offset, limit)` and returns `VtLogChunk`. */
  fetcher: (offset: number, limit: number) => Promise<VtLogChunk>;
}

interface Chunk {
  /** Absolute offset of the first line in `lines`. */
  startOffset: number;
  lines: LogLine[];
}

export class ScrollbackCache {
  private readonly chunkSize: number;
  private readonly maxChunks: number;
  private readonly fetcher: ScrollbackCacheOptions["fetcher"];

  /** Resident chunks keyed by chunk index. Map preserves insertion order,
   *  which we (re)use as the LRU order: the first key is LRU, last is MRU. */
  private readonly chunks = new Map<number, Chunk>();

  /** In-flight fetches keyed by chunk index — used for dedup. */
  private readonly inFlight = new Map<number, Promise<void>>();

  /** Tracks consecutive fetch failures per chunk for retry logic.
   *  Key = chunk index, value = consecutive failure count.
   *  Cleared on successful fetch. After 2 failures, chunk is permanently failed. */
  private readonly failedChunks = new Map<number, number>();

  private readonly listeners = new Set<(event: CacheEvent) => void>();

  private _total = 0;
  private _oldest = 0;

  constructor(opts: ScrollbackCacheOptions) {
    this.chunkSize = opts.chunkSize;
    this.maxChunks = opts.maxChunks;
    this.fetcher = opts.fetcher;
  }

  // --- metadata ---

  get total(): number {
    return this._total;
  }

  get oldest(): number {
    return this._oldest;
  }

  /** Update the known `total` (e.g. from the `pty-vt-log-total-*` event).
   *  Notifies subscribers when the value actually changes. */
  setTotal(total: number): void {
    if (total === this._total) return;
    this._total = total;
    this.emit({ type: "total", total });
  }

  /** Update the known `oldest` offset. Invalidates any resident chunk
   *  that is entirely below the new oldest (the server already dropped
   *  those lines, so we can't refetch them anyway). */
  setOldest(oldest: number): void {
    if (oldest === this._oldest) return;
    this._oldest = oldest;
    // A chunk is entirely invalidated if its end (exclusive) <= oldest
    // OR if oldest falls inside the chunk — in the latter case the
    // partial slice cached locally is misaligned, so we drop it and let
    // the overlay refetch it on demand. Simpler invariant, no partial
    // patching.
    for (const [idx] of this.chunks) {
      const chunkEnd = (idx + 1) * this.chunkSize;
      if (chunkEnd <= oldest || idx * this.chunkSize < oldest) {
        this.chunks.delete(idx);
      }
    }
    this.emit({ type: "oldest", oldest });
  }

  // --- lookups ---

  /** Returns the cached line at absolute offset `offset`, or `undefined`
   *  if that line is not currently loaded.
   *
   *  Does NOT touch LRU order — the render loop calls getLine() ~56 times
   *  per frame for visible rows, and the delete+re-insert churn is wasteful.
   *  LRU promotion is handled by `ensureLoaded()` which is always called
   *  before rendering via the progressive-load effect. */
  getLine(offset: number): LogLine | undefined {
    const idx = Math.floor(offset / this.chunkSize);
    const chunk = this.chunks.get(idx);
    if (!chunk) return undefined;
    const relative = offset - chunk.startOffset;
    return chunk.lines[relative];
  }

  /** Chunk indices currently resident in memory. Test-only convenience. */
  get residentChunks(): number[] {
    return Array.from(this.chunks.keys());
  }

  // --- fetching ---

  /** Ensures all lines in the half-open range [startOffset, endOffset)
   *  are loaded. Missing chunks are fetched in parallel; concurrent
   *  callers for the same chunk share a single Promise. Resolves when
   *  every needed chunk is resident (or the fetcher rejected). */
  async ensureLoaded(startOffset: number, endOffset: number): Promise<void> {
    if (endOffset <= startOffset) return;
    const firstChunk = Math.floor(startOffset / this.chunkSize);
    // endOffset is exclusive, so the last chunk is floor((endOffset-1)/chunkSize).
    const lastChunk = Math.floor((endOffset - 1) / this.chunkSize);

    const pending: Promise<void>[] = [];
    for (let idx = firstChunk; idx <= lastChunk; idx++) {
      if (this.chunks.has(idx)) {
        // Touch LRU position on cache hit.
        const chunk = this.chunks.get(idx)!;
        this.chunks.delete(idx);
        this.chunks.set(idx, chunk);
        continue;
      }
      // Skip permanently failed chunks (2+ consecutive failures)
      const failures = this.failedChunks.get(idx) ?? 0;
      if (failures >= 2) continue;
      const inflight = this.inFlight.get(idx);
      if (inflight) {
        pending.push(inflight);
        continue;
      }
      pending.push(this.fetchChunk(idx));
    }
    await Promise.all(pending);
  }

  private fetchChunk(idx: number): Promise<void> {
    const startOffset = idx * this.chunkSize;
    const promise = (async () => {
      try {
        const result = await this.fetcher(startOffset, this.chunkSize);
        // Update metadata first so listeners see consistent state when
        // they receive the `chunkLoaded` notification.
        if (result.total !== this._total) {
          this._total = result.total;
          this.emit({ type: "total", total: result.total });
        }
        if (result.oldest !== this._oldest) {
          this._oldest = result.oldest;
          this.emit({ type: "oldest", oldest: result.oldest });
        }
        // Store the chunk (touches MRU).
        this.chunks.set(idx, { startOffset, lines: result.lines });
        this.evictIfOverCapacity();
        // Clear failure counter on success
        this.failedChunks.delete(idx);
        this.emit({ type: "chunkLoaded", chunkIdx: idx });
      } catch {
        // Track consecutive failures — after 2, chunk is permanently skipped
        const prev = this.failedChunks.get(idx) ?? 0;
        this.failedChunks.set(idx, prev + 1);
      } finally {
        this.inFlight.delete(idx);
      }
    })();
    this.inFlight.set(idx, promise);
    return promise;
  }

  /** Evict least-recently-used chunks until we are at/under `maxChunks`. */
  private evictIfOverCapacity(): void {
    while (this.chunks.size > this.maxChunks) {
      // First key in insertion order is the LRU entry.
      const lru = this.chunks.keys().next().value;
      if (lru === undefined) return;
      this.chunks.delete(lru);
    }
  }

  // --- subscriptions ---

  /** Subscribe to cache events. Returns an unsubscribe function. */
  subscribe(listener: (event: CacheEvent) => void): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  private emit(event: CacheEvent): void {
    for (const listener of this.listeners) {
      listener(event);
    }
  }
}
