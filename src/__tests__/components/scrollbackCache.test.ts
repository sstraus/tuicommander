import { describe, expect, it, vi } from "vitest";
import {
  ScrollbackCache,
  type VtLogChunk,
} from "../../components/Terminal/scrollbackCache";
import type { LogLine } from "../../mobile/utils/logLine";

/**
 * Build N fake LogLine entries with incrementing text so we can assert
 * ordering. Each line has a single span with `"line-${globalOffset}"`.
 */
function fakeLines(startOffset: number, count: number): LogLine[] {
  const out: LogLine[] = [];
  for (let i = 0; i < count; i++) {
    out.push({ spans: [{ text: `line-${startOffset + i}` }] });
  }
  return out;
}

/**
 * Build a deterministic fetcher over a backing array of `totalServerLines`.
 * Captures every (offset, limit) call for assertion.
 */
function makeFetcher(totalServerLines: number) {
  const calls: Array<{ offset: number; limit: number }> = [];
  const fetcher = vi.fn(async (offset: number, limit: number): Promise<VtLogChunk> => {
    calls.push({ offset, limit });
    const end = Math.min(offset + limit, totalServerLines);
    const lines = offset >= totalServerLines ? [] : fakeLines(offset, end - offset);
    return { lines, total: totalServerLines, oldest: 0 };
  });
  return { fetcher, calls };
}

describe("ScrollbackCache — construction & metadata", () => {
  it("starts empty with total=0, oldest=0, no residents", () => {
    const { fetcher } = makeFetcher(0);
    const cache = new ScrollbackCache({ chunkSize: 200, maxChunks: 4, fetcher });
    expect(cache.total).toBe(0);
    expect(cache.oldest).toBe(0);
    expect(cache.residentChunks).toEqual([]);
  });

  it("setTotal updates total and notifies subscribers", () => {
    const { fetcher } = makeFetcher(0);
    const cache = new ScrollbackCache({ chunkSize: 200, maxChunks: 4, fetcher });
    const events: Array<{ type: string }> = [];
    cache.subscribe((e) => events.push(e));
    cache.setTotal(500);
    expect(cache.total).toBe(500);
    expect(events).toEqual([{ type: "total", total: 500 }]);
  });

  it("setTotal with same value does not re-emit", () => {
    const { fetcher } = makeFetcher(0);
    const cache = new ScrollbackCache({ chunkSize: 200, maxChunks: 4, fetcher });
    cache.setTotal(500);
    const events: Array<{ type: string }> = [];
    cache.subscribe((e) => events.push(e));
    cache.setTotal(500);
    expect(events).toEqual([]);
  });

  it("subscribe unsubscribe removes listener", () => {
    const { fetcher } = makeFetcher(0);
    const cache = new ScrollbackCache({ chunkSize: 200, maxChunks: 4, fetcher });
    const events: Array<{ type: string }> = [];
    const off = cache.subscribe((e) => events.push(e));
    off();
    cache.setTotal(500);
    expect(events).toEqual([]);
  });
});

describe("ScrollbackCache — ensureLoaded & getLine", () => {
  it("fetches the chunk containing the requested range", async () => {
    const { fetcher, calls } = makeFetcher(1000);
    const cache = new ScrollbackCache({ chunkSize: 200, maxChunks: 4, fetcher });
    await cache.ensureLoaded(450, 500);
    // Only chunk 2 (offsets 400..600) is needed.
    expect(calls).toEqual([{ offset: 400, limit: 200 }]);
    expect(cache.residentChunks).toEqual([2]);
    expect(cache.getLine(450)?.spans[0]?.text).toBe("line-450");
    expect(cache.getLine(499)?.spans[0]?.text).toBe("line-499");
  });

  it("fetches multiple chunks when range spans a boundary", async () => {
    const { fetcher, calls } = makeFetcher(1000);
    const cache = new ScrollbackCache({ chunkSize: 200, maxChunks: 4, fetcher });
    await cache.ensureLoaded(350, 650);
    // chunks 1 (200..400), 2 (400..600), 3 (600..800)
    expect(calls).toEqual([
      { offset: 200, limit: 200 },
      { offset: 400, limit: 200 },
      { offset: 600, limit: 200 },
    ]);
    expect(cache.residentChunks.sort((a, b) => a - b)).toEqual([1, 2, 3]);
  });

  it("deduplicates concurrent fetches for the same chunk", async () => {
    const { fetcher, calls } = makeFetcher(1000);
    const cache = new ScrollbackCache({ chunkSize: 200, maxChunks: 4, fetcher });
    await Promise.all([
      cache.ensureLoaded(450, 500),
      cache.ensureLoaded(460, 510),
      cache.ensureLoaded(420, 480),
    ]);
    expect(calls).toEqual([{ offset: 400, limit: 200 }]);
  });

  it("does not refetch already-loaded chunks", async () => {
    const { fetcher, calls } = makeFetcher(1000);
    const cache = new ScrollbackCache({ chunkSize: 200, maxChunks: 4, fetcher });
    await cache.ensureLoaded(450, 500);
    expect(calls.length).toBe(1);
    await cache.ensureLoaded(400, 550);
    expect(calls.length).toBe(1);
  });

  it("getLine returns undefined outside loaded range", () => {
    const { fetcher } = makeFetcher(1000);
    const cache = new ScrollbackCache({ chunkSize: 200, maxChunks: 4, fetcher });
    expect(cache.getLine(42)).toBeUndefined();
  });

  it("updates total and oldest from fetcher response", async () => {
    const fetcher = vi.fn(async (offset: number, limit: number): Promise<VtLogChunk> => ({
      lines: fakeLines(offset, Math.min(limit, 100)),
      total: 1234,
      oldest: 50,
    }));
    const cache = new ScrollbackCache({ chunkSize: 200, maxChunks: 4, fetcher });
    await cache.ensureLoaded(0, 100);
    expect(cache.total).toBe(1234);
    expect(cache.oldest).toBe(50);
  });
});

describe("ScrollbackCache — LRU eviction", () => {
  it("evicts least-recently-accessed chunk when over maxChunks", async () => {
    const { fetcher } = makeFetcher(2000);
    const cache = new ScrollbackCache({ chunkSize: 200, maxChunks: 3, fetcher });

    await cache.ensureLoaded(0, 50); // chunk 0 (LRU head)
    await cache.ensureLoaded(200, 250); // chunk 1
    await cache.ensureLoaded(400, 450); // chunk 2 → now full
    expect(cache.residentChunks.sort((a, b) => a - b)).toEqual([0, 1, 2]);

    await cache.ensureLoaded(600, 650); // chunk 3 → evicts chunk 0 (oldest)
    expect(cache.residentChunks.sort((a, b) => a - b)).toEqual([1, 2, 3]);
    expect(cache.getLine(0)).toBeUndefined();
  });

  it("re-accessing a chunk moves it to MRU position", async () => {
    const { fetcher } = makeFetcher(2000);
    const cache = new ScrollbackCache({ chunkSize: 200, maxChunks: 3, fetcher });

    await cache.ensureLoaded(0, 50); // chunk 0
    await cache.ensureLoaded(200, 250); // chunk 1
    await cache.ensureLoaded(400, 450); // chunk 2

    // Touch chunk 0 → it becomes MRU, chunk 1 is now LRU.
    await cache.ensureLoaded(0, 50);
    await cache.ensureLoaded(600, 650); // chunk 3 → evicts chunk 1
    expect(cache.residentChunks.sort((a, b) => a - b)).toEqual([0, 2, 3]);
    expect(cache.getLine(200)).toBeUndefined();
    expect(cache.getLine(0)?.spans[0]?.text).toBe("line-0");
  });
});

describe("ScrollbackCache — live growth & oldest shifting", () => {
  it("setTotal does not invalidate already-loaded chunks", async () => {
    const { fetcher } = makeFetcher(1000);
    const cache = new ScrollbackCache({ chunkSize: 200, maxChunks: 4, fetcher });
    await cache.ensureLoaded(0, 100);
    cache.setTotal(1500);
    expect(cache.residentChunks).toEqual([0]);
    expect(cache.getLine(50)?.spans[0]?.text).toBe("line-50");
  });

  it("setOldest invalidates chunks that are entirely below the new oldest", async () => {
    const { fetcher } = makeFetcher(2000);
    const cache = new ScrollbackCache({ chunkSize: 200, maxChunks: 6, fetcher });
    await cache.ensureLoaded(0, 50);    // chunk 0 (0..200)
    await cache.ensureLoaded(200, 250); // chunk 1 (200..400)
    await cache.ensureLoaded(400, 450); // chunk 2 (400..600)

    // oldest jumps to 450 — chunk 0 fully evicted, chunk 1 fully evicted,
    // chunk 2 (covers 400..600) is still partially valid but we drop it too
    // to keep invariants simple (the overlay will refetch it on demand).
    cache.setOldest(450);
    expect(cache.residentChunks).toEqual([]);
    expect(cache.oldest).toBe(450);
  });

  it("setOldest leaves chunks entirely above the new oldest intact", async () => {
    const { fetcher } = makeFetcher(2000);
    const cache = new ScrollbackCache({ chunkSize: 200, maxChunks: 6, fetcher });
    await cache.ensureLoaded(0, 50);    // chunk 0
    await cache.ensureLoaded(800, 850); // chunk 4 (800..1000)

    cache.setOldest(600); // chunk 0 below → drop; chunk 4 above → keep
    expect(cache.residentChunks).toEqual([4]);
  });

  it("fetching near total returns partial chunk without breaking getLine", async () => {
    const { fetcher } = makeFetcher(1050); // last chunk has only 50 lines
    const cache = new ScrollbackCache({ chunkSize: 200, maxChunks: 4, fetcher });
    await cache.ensureLoaded(1000, 1050);
    expect(cache.getLine(1000)?.spans[0]?.text).toBe("line-1000");
    expect(cache.getLine(1049)?.spans[0]?.text).toBe("line-1049");
    // Nothing beyond total.
    expect(cache.getLine(1050)).toBeUndefined();
  });
});

describe("ScrollbackCache — chunkLoaded notifications", () => {
  it("emits chunkLoaded for each newly fetched chunk", async () => {
    const { fetcher } = makeFetcher(1000);
    const cache = new ScrollbackCache({ chunkSize: 200, maxChunks: 4, fetcher });
    const events: Array<{ type: string; chunkIdx?: number }> = [];
    cache.subscribe((e) => events.push(e));
    await cache.ensureLoaded(350, 650);
    const chunkLoaded = events.filter((e) => e.type === "chunkLoaded");
    expect(chunkLoaded.map((e) => e.chunkIdx).sort()).toEqual([1, 2, 3]);
  });

  it("does not re-emit chunkLoaded for cache hits", async () => {
    const { fetcher } = makeFetcher(1000);
    const cache = new ScrollbackCache({ chunkSize: 200, maxChunks: 4, fetcher });
    await cache.ensureLoaded(400, 500);
    const events: Array<{ type: string }> = [];
    cache.subscribe((e) => events.push(e));
    await cache.ensureLoaded(400, 500);
    expect(events).toEqual([]);
  });
});

describe("ScrollbackCache — fetch failure retry", () => {
  it("retries a failed chunk on the next ensureLoaded call", async () => {
    let callCount = 0;
    const fetcher = vi.fn(async (offset: number, limit: number): Promise<VtLogChunk> => {
      callCount++;
      if (callCount === 1) throw new Error("network error");
      // Second call succeeds
      const lines = fakeLines(offset, limit);
      return { lines, total: 1000, oldest: 0 };
    });
    const cache = new ScrollbackCache({ chunkSize: 200, maxChunks: 4, fetcher });

    // First call fails — chunk not loaded
    await cache.ensureLoaded(0, 50);
    expect(cache.getLine(0)).toBeUndefined();
    expect(callCount).toBe(1);

    // Second call retries and succeeds
    await cache.ensureLoaded(0, 50);
    expect(cache.getLine(0)?.spans[0]?.text).toBe("line-0");
    expect(callCount).toBe(2);
  });

  it("stops retrying after 2 consecutive failures for the same chunk", async () => {
    let callCount = 0;
    const fetcher = vi.fn(async (_offset: number, _limit: number): Promise<VtLogChunk> => {
      callCount++;
      throw new Error("persistent failure");
    });
    const cache = new ScrollbackCache({ chunkSize: 200, maxChunks: 4, fetcher });

    // First failure
    await cache.ensureLoaded(0, 50);
    expect(callCount).toBe(1);

    // Second failure — chunk now permanently failed
    await cache.ensureLoaded(0, 50);
    expect(callCount).toBe(2);

    // Third call should NOT retry (permanently failed)
    await cache.ensureLoaded(0, 50);
    expect(callCount).toBe(2);
  });
});
