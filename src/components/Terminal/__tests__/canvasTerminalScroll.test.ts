import { describe, expect, it } from "vitest";
import {
	createCanvasScrollController,
	GESTURE_ACCEL_MAX,
	GESTURE_ACCEL_MIN,
	gestureAccelFactor,
	ROW_CACHE_CHUNK,
	ROW_CACHE_MAX,
} from "../canvasTerminalScroll";

describe("canvas terminal scroll controller", () => {
	const row = (n: number) => ({ index: n }) as never;

	it("tracks fractional gestures and clamps their pending backend offset", () => {
		const scroll = createCanvasScrollController();
		expect(scroll.applyDelta(-2.5, 0, 10)).toBe(2.5);
		expect(scroll.pendingOffset).toBe(2);
		expect(scroll.applyDelta(-20, 0, 10)).toBe(10);
		expect(scroll.pendingOffset).toBe(10);
		expect(scroll.scrolling).toBe(true);
	});

	it("snaps and hands off only when the matching backend frame arrives", () => {
		const scroll = createCanvasScrollController();
		scroll.applyDelta(-3.6, 0, 20);
		expect(scroll.snap()).toBe(4);
		expect(scroll.acceptSettledFrame(3)).toBe(false);
		expect(scroll.position).toBe(4);
		expect(scroll.acceptSettledFrame(4)).toBe(true);
		expect(scroll.position).toBeNull();
		expect(scroll.settleTarget).toBeNull();
	});

	// #1264-89c8. Oracle: alacritty's rule for a scrolled-back viewport — output
	// adds the lines it pushed into history to the offset, clamped to the history
	// size, and leaves offset 0 following the tail.
	describe("followHistory", () => {
		it("catches: a gesture at the bottom being pinned above new output", () => {
			const scroll = createCanvasScrollController();
			scroll.applyDelta(0, 0, 50);
			expect(scroll.followHistory(5, 55, 0)).toBe(false);
			expect(scroll.position).toBe(0);
		});

		it("catches: an unsent offset flushed in the pre-output coordinates", () => {
			const scroll = createCanvasScrollController();
			scroll.applyDelta(-3.5, 0, 50);
			expect(scroll.followHistory(5, 55, 0)).toBe(true);
			expect(scroll.position).toBe(8.5);
			expect(scroll.pendingOffset).toBe(8);
		});

		it("catches: a backend that already holds the lines being sent a redundant offset", () => {
			const scroll = createCanvasScrollController();
			scroll.applyDelta(-3, 0, 50);
			scroll.pendingOffset = null;
			expect(scroll.followHistory(5, 55, 8)).toBe(false);
			expect(scroll.position).toBe(8);
		});

		it("catches: a backend that missed the output being left forward of the user", () => {
			const scroll = createCanvasScrollController();
			scroll.applyDelta(-3, 0, 50);
			scroll.pendingOffset = null;
			expect(scroll.followHistory(5, 55, 3)).toBe(true);
			expect(scroll.pendingOffset).toBe(8);
		});

		it("catches: a snapped gesture never handing off because its target went stale", () => {
			const scroll = createCanvasScrollController();
			scroll.applyDelta(-3.6, 0, 50);
			expect(scroll.snap()).toBe(4);
			scroll.followHistory(3, 53, 7);
			expect(scroll.acceptSettledFrame(7)).toBe(true);
			expect(scroll.position).toBeNull();
		});

		it("catches: an offset past the top once the scrollback cap evicts lines", () => {
			const scroll = createCanvasScrollController();
			scroll.applyDelta(-48, 0, 50);
			// Full history: its size stays 50 while 5 lines are evicted from the top.
			scroll.followHistory(5, 50, 50);
			expect(scroll.position).toBe(50);
		});
	});

	it("owns cache state and cancels every transient field", () => {
		const scroll = createCanvasScrollController();
		scroll.requestedChunks.add(2);
		scroll.position = 1.5;
		scroll.pendingOffset = 1;
		scroll.scrolling = true;
		scroll.settleTarget = 2;
		scroll.gestureDistancePx = 42;
		scroll.clearCache();
		expect(scroll.requestedChunks.size).toBe(0);

		scroll.cancel();
		expect(scroll.position).toBeNull();
		expect(scroll.pendingOffset).toBeNull();
		expect(scroll.scrolling).toBe(false);
		expect(scroll.settleTarget).toBeNull();
		expect(scroll.gestureDistancePx).toBe(0);
	});

	it("accumulates same-signed gesture distance and restarts the ramp on reversal", () => {
		const scroll = createCanvasScrollController();
		expect(scroll.accumulateGesture(10)).toBe(10);
		expect(scroll.accumulateGesture(5)).toBe(15);
		// Reversal restarts at |dy| instead of adding to the outbound distance.
		expect(scroll.accumulateGesture(-4)).toBe(4);
		expect(scroll.accumulateGesture(-6)).toBe(10);
	});

	it("snap and cancel both reset the gesture sign, not just the distance", () => {
		const scroll = createCanvasScrollController();
		scroll.accumulateGesture(10);
		scroll.snap();
		// If sign weren't reset, this same-direction call would still read as a
		// reversal relative to stale state and incorrectly restart the ramp.
		expect(scroll.accumulateGesture(5)).toBe(5);

		scroll.accumulateGesture(10);
		scroll.cancel();
		expect(scroll.accumulateGesture(5)).toBe(5);
	});

	it("invalidates delayed cache responses when the screen generation changes", () => {
		const scroll = createCanvasScrollController();
		const requestGeneration = scroll.cacheGeneration;
		expect(requestGeneration).toBeTypeOf("number");

		scroll.rowCache.set(7, {} as never);
		scroll.requestedChunks.add(0);
		scroll.clearCache();

		expect(scroll.cacheGeneration).toBe(requestGeneration + 1);
		expect(scroll.isCacheGenerationCurrent(requestGeneration)).toBe(false);
		expect(scroll.rowCache.size).toBe(0);
		expect(scroll.requestedChunks.size).toBe(0);
	});

	// The cache is keyed by an eviction-stable ALL-TIME row index, so a line that
	// scrolls in always gets a fresh key and never overwrites an old entry. Every
	// writer therefore has to be bounded, not just the scroll-gesture one: a tab
	// that is never resized, wheel-scrolled or alt-screened grows the map for the
	// life of the session, ~2.6 KB per row.
	describe("cacheRows", () => {
		it("preserves sparse multi-codepoint cell content", () => {
			const scroll = createCanvasScrollController();
			const cached = { index: 4, cellExtras: new Map([[2, "\u0301"]]) } as never;
			scroll.cacheRows([{ abs: 40, row: cached }]);
			expect(scroll.rowCache.get(40)).toBe(cached);
			expect(scroll.rowCache.get(40)?.cellExtras?.get(2)).toBe("\u0301");
		});

		it("bounds the cache however many rows are written", () => {
			const scroll = createCanvasScrollController();
			for (let abs = 0; abs < ROW_CACHE_MAX + 500; abs++) {
				scroll.cacheRows([{ abs, row: row(abs) }]);
			}
			expect(scroll.rowCache.size).toBe(ROW_CACHE_MAX);
		});

		it("evicts the oldest rows and keeps the newest", () => {
			const scroll = createCanvasScrollController();
			const total = ROW_CACHE_MAX + 3;
			for (let abs = 0; abs < total; abs++) {
				scroll.cacheRows([{ abs, row: row(abs) }]);
			}
			// The rows just written are the ones a scroll gesture is about to paint.
			expect(scroll.rowCache.has(total - 1)).toBe(true);
			expect(scroll.rowCache.has(total - ROW_CACHE_MAX)).toBe(true);
			expect(scroll.rowCache.has(0)).toBe(false);
			expect(scroll.rowCache.has(2)).toBe(false);
		});

		it("bounds a single oversized write too", () => {
			const scroll = createCanvasScrollController();
			const batch = Array.from({ length: ROW_CACHE_MAX + 64 }, (_, abs) => ({ abs, row: row(abs) }));
			scroll.cacheRows(batch);
			expect(scroll.rowCache.size).toBe(ROW_CACHE_MAX);
		});

		// `requestedChunks` is a "do not ask again" set: ensureCacheBand skips every
		// chunk in it and a successful fetch never removes its id. So a chunk whose
		// rows get evicted has to leave that set too, or scrolling back to it paints
		// blanks forever — the whole-cache clear this replaced dropped both together.
		it("lets an evicted chunk be requested again", () => {
			const scroll = createCanvasScrollController();
			const chunkOf = (abs: number) => Math.floor(abs / ROW_CACHE_CHUNK);
			const total = ROW_CACHE_MAX + 3 * ROW_CACHE_CHUNK;
			for (let abs = 0; abs < total; abs++) {
				scroll.requestedChunks.add(chunkOf(abs));
				scroll.cacheRows([{ abs, row: row(abs) }]);
			}

			expect(scroll.rowCache.has(0)).toBe(false);
			expect(scroll.requestedChunks.has(chunkOf(0))).toBe(false);
			// A chunk still fully cached stays claimed — re-fetching it would be a
			// pointless round trip.
			expect(scroll.requestedChunks.has(chunkOf(total - 1))).toBe(true);
		});

		it("keeps a partially evicted chunk requestable", () => {
			// Half a chunk in cache is not a paintable chunk: the band fetch has to
			// be free to ask for it again.
			const scroll = createCanvasScrollController();
			const total = ROW_CACHE_MAX + 1;
			for (let abs = 0; abs < total; abs++) {
				scroll.requestedChunks.add(Math.floor(abs / ROW_CACHE_CHUNK));
				scroll.cacheRows([{ abs, row: row(abs) }]);
			}
			// Row 0 was evicted; rows 1..63 of chunk 0 are still cached.
			expect(scroll.rowCache.has(1)).toBe(true);
			expect(scroll.requestedChunks.has(0)).toBe(false);
		});

		it("does not invalidate in-flight chunk requests when it evicts", () => {
			// Eviction is not invalidation: dropping the oldest rows says nothing
			// about the rows a pending fetchChunk is on its way to deliver. Bumping
			// the generation here would make every fill during steady output throw
			// away a chunk that is still correct.
			const scroll = createCanvasScrollController();
			const generation = scroll.cacheGeneration;
			for (let abs = 0; abs < ROW_CACHE_MAX + 10; abs++) {
				scroll.cacheRows([{ abs, row: row(abs) }]);
			}
			expect(scroll.cacheGeneration).toBe(generation);
			expect(scroll.isCacheGenerationCurrent(generation)).toBe(true);
		});
	});

	// #1264-89c8. Oracle: a line that left the live screen has its final content
	// only after it left; everything read before is stale.
	describe("commitLiveRows", () => {
		it("catches: a cached live row surviving the redraw that committed it", () => {
			const scroll = createCanvasScrollController();
			scroll.cacheRows([100, 101, 102, 103].map((abs) => ({ abs, row: row(abs) })));
			scroll.requestedChunks.add(1);
			scroll.commitLiveRows(100, 102);
			expect(scroll.rowCache.has(100)).toBe(false);
			expect(scroll.rowCache.has(101)).toBe(false);
			expect(scroll.rowCache.has(102)).toBe(true);
			expect(scroll.requestedChunks.has(1)).toBe(false);
		});

		it("catches: releasing only the first chunk when the committed rows straddle two", () => {
			const scroll = createCanvasScrollController();
			scroll.requestedChunks.add(0);
			scroll.requestedChunks.add(1);
			scroll.requestedChunks.add(2);
			scroll.commitLiveRows(ROW_CACHE_CHUNK - 2, ROW_CACHE_CHUNK + 2);
			expect([0, 1, 2].map((c) => scroll.requestedChunks.has(c))).toEqual([false, false, true]);
		});

		it("catches: an empty commit invalidating a chunk that is still correct", () => {
			const scroll = createCanvasScrollController();
			scroll.requestedChunks.add(1);
			scroll.commitLiveRows(100, 100);
			expect(scroll.requestedChunks.has(1)).toBe(true);
			expect(scroll.liveEpoch).toBe(0);
		});
	});

	describe("cacheFetchedRows", () => {
		it("catches: a fetch in flight while lines were committed re-caching their stale rows", () => {
			const scroll = createCanvasScrollController();
			const epoch = scroll.liveEpoch;
			scroll.commitLiveRows(100, 105);
			const rows = [98, 99, 100, 101].map((abs) => ({ abs, row: row(abs) }));
			expect(scroll.cacheFetchedRows(rows, epoch, 100)).toBe(false);
			expect([98, 99, 100, 101].map((abs) => scroll.rowCache.has(abs))).toEqual([true, true, false, false]);
		});

		it("catches: refetching a chunk that holds only history after a commit above it", () => {
			const scroll = createCanvasScrollController();
			const epoch = scroll.liveEpoch;
			scroll.commitLiveRows(100, 105);
			const rows = [10, 11].map((abs) => ({ abs, row: row(abs) }));
			expect(scroll.cacheFetchedRows(rows, epoch, 100)).toBe(true);
			expect(scroll.rowCache.size).toBe(2);
		});

		it("catches: dropping a fetch that saw no commit", () => {
			const scroll = createCanvasScrollController();
			const rows = [100, 101].map((abs) => ({ abs, row: row(abs) }));
			expect(scroll.cacheFetchedRows(rows, scroll.liveEpoch, 100)).toBe(true);
			expect(scroll.rowCache.size).toBe(2);
		});
	});
});

describe("gestureAccelFactor", () => {
	it("starts at the damped floor and reaches 1:1 tracking after two screens", () => {
		expect(gestureAccelFactor(0, 100)).toBe(GESTURE_ACCEL_MIN);
		expect(gestureAccelFactor(100, 100)).toBe(GESTURE_ACCEL_MIN);
		expect(gestureAccelFactor(200, 100)).toBe(1.0);
	});

	it("is capped at the ceiling no matter how long the gesture runs", () => {
		expect(gestureAccelFactor(2000, 100)).toBe(GESTURE_ACCEL_MAX);
	});

	it("falls back to the floor when there is no screen height to ramp against", () => {
		expect(gestureAccelFactor(500, 0)).toBe(GESTURE_ACCEL_MIN);
		expect(gestureAccelFactor(500, -10)).toBe(GESTURE_ACCEL_MIN);
	});
});
