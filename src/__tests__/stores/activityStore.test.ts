import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ActivityItem, ActivitySection } from "../../plugins/types";
import { activityStore } from "../../stores/activityStore";

const makeSection = (overrides: Partial<ActivitySection> = {}): ActivitySection => ({
	id: "test",
	label: "TEST",
	priority: 10,
	canDismissAll: false,
	...overrides,
});

const makeItem = (overrides: Partial<Omit<ActivityItem, "createdAt">> = {}): Omit<ActivityItem, "createdAt"> => ({
	id: "item-1",
	pluginId: "test-plugin",
	sectionId: "test",
	title: "Test Item",
	icon: "<svg/>",
	dismissible: true,
	...overrides,
});

describe("activityStore", () => {
	let consoleSpy: ReturnType<typeof vi.spyOn>;

	beforeEach(() => {
		consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});
		activityStore.clearAll();
	});

	afterEach(() => {
		activityStore._testCancelPendingSave();
		consoleSpy.mockRestore();
	});

	// -------------------------------------------------------------------------
	// Section registration
	// -------------------------------------------------------------------------
	describe("registerSection", () => {
		it("adds a section and returns a Disposable", () => {
			const d = activityStore.registerSection(makeSection({ id: "plan" }));
			expect(activityStore.getSections().some((s) => s.id === "plan")).toBe(true);
			d.dispose();
		});

		it("dispose removes the section", () => {
			const d = activityStore.registerSection(makeSection({ id: "plan" }));
			d.dispose();
			expect(activityStore.getSections().some((s) => s.id === "plan")).toBe(false);
		});

		it("sections are returned sorted by priority ascending", () => {
			activityStore.registerSection(makeSection({ id: "b", priority: 20 }));
			activityStore.registerSection(makeSection({ id: "a", priority: 10 }));
			const ids = activityStore.getSections().map((s) => s.id);
			expect(ids.indexOf("a")).toBeLessThan(ids.indexOf("b"));
		});

		it("re-registering same section id replaces the existing one", () => {
			activityStore.registerSection(makeSection({ id: "dup", label: "First" }));
			activityStore.registerSection(makeSection({ id: "dup", label: "Second" }));
			const sections = activityStore.getSections().filter((s) => s.id === "dup");
			expect(sections).toHaveLength(1);
			expect(sections[0].label).toBe("Second");
		});

		it("disposing old registration after re-register removes section (dispose filters by id)", () => {
			const d1 = activityStore.registerSection(makeSection({ id: "dup", label: "First" }));
			activityStore.registerSection(makeSection({ id: "dup", label: "Second" }));
			d1.dispose(); // dispose filters by id — removes the replacement too
			expect(activityStore.getSections().some((s) => s.id === "dup")).toBe(false);
		});
	});

	// -------------------------------------------------------------------------
	// CRUD items
	// -------------------------------------------------------------------------
	describe("addItem", () => {
		it("adds an item with a createdAt timestamp", () => {
			const before = Date.now();
			activityStore.addItem(makeItem());
			const after = Date.now();
			const item = activityStore.getActive()[0];
			expect(item).toBeDefined();
			expect(item.createdAt).toBeGreaterThanOrEqual(before);
			expect(item.createdAt).toBeLessThanOrEqual(after);
		});

		it("adding duplicate id replaces the existing item", () => {
			activityStore.addItem(makeItem({ id: "dup", title: "First" }));
			activityStore.addItem(makeItem({ id: "dup", title: "Second" }));
			const active = activityStore.getActive();
			expect(active).toHaveLength(1);
			expect(active[0].title).toBe("Second");
		});
	});

	describe("removeItem", () => {
		it("removes an item by id", () => {
			activityStore.addItem(makeItem({ id: "r1" }));
			activityStore.removeItem("r1");
			expect(activityStore.getActive().find((i) => i.id === "r1")).toBeUndefined();
		});

		it("is a no-op for unknown id", () => {
			activityStore.addItem(makeItem({ id: "r2" }));
			activityStore.removeItem("unknown");
			expect(activityStore.getActive()).toHaveLength(1);
		});
	});

	describe("updateItem", () => {
		it("updates fields on an existing item", () => {
			activityStore.addItem(makeItem({ id: "u1", title: "Old" }));
			activityStore.updateItem("u1", { title: "New", subtitle: "sub" });
			const item = activityStore.getActive().find((i) => i.id === "u1");
			expect(item?.title).toBe("New");
			expect(item?.subtitle).toBe("sub");
		});

		it("does not change createdAt on update", () => {
			activityStore.addItem(makeItem({ id: "u2" }));
			const original = activityStore.getActive().find((i) => i.id === "u2")!.createdAt;
			activityStore.updateItem("u2", { title: "Changed" });
			expect(activityStore.getActive().find((i) => i.id === "u2")!.createdAt).toBe(original);
		});

		it("is a no-op for unknown id", () => {
			activityStore.updateItem("unknown", { title: "X" });
			// No throw
		});
	});

	// -------------------------------------------------------------------------
	// Queries
	// -------------------------------------------------------------------------
	describe("getActive", () => {
		it("returns only non-dismissed items", () => {
			activityStore.addItem(makeItem({ id: "a1" }));
			activityStore.addItem(makeItem({ id: "a2" }));
			activityStore.dismissItem("a1");
			const active = activityStore.getActive();
			expect(active.map((i) => i.id)).not.toContain("a1");
			expect(active.map((i) => i.id)).toContain("a2");
		});
	});

	describe("getForSection", () => {
		it("returns non-dismissed items for the given section", () => {
			activityStore.addItem(makeItem({ id: "s1", sectionId: "plan" }));
			activityStore.addItem(makeItem({ id: "s2", sectionId: "stories" }));
			const planItems = activityStore.getForSection("plan");
			expect(planItems.map((i) => i.id)).toEqual(["s1"]);
		});

		it("excludes dismissed items", () => {
			activityStore.addItem(makeItem({ id: "s3", sectionId: "plan" }));
			activityStore.dismissItem("s3");
			expect(activityStore.getForSection("plan")).toHaveLength(0);
		});

		it("filters by repoPath when provided (includes items without repoPath)", () => {
			activityStore.addItem(makeItem({ id: "rp1", sectionId: "plan", repoPath: "/repo/a" }));
			activityStore.addItem(makeItem({ id: "rp2", sectionId: "plan", repoPath: "/repo/b" }));
			activityStore.addItem(makeItem({ id: "rp3", sectionId: "plan" })); // no repoPath — always included
			const filtered = activityStore.getForSection("plan", "/repo/a");
			// Membership, not order — items added inside one millisecond tie on
			// createdAt, so the newest-first sort leaves their relative order open.
			expect(filtered.map((i) => i.id).sort()).toEqual(["rp1", "rp3"]);
		});

		it("returns the newest item first — the bell reads top-down", () => {
			const now = vi.spyOn(Date, "now");
			now.mockReturnValueOnce(1000).mockReturnValueOnce(2000).mockReturnValueOnce(3000);
			activityStore.addItem(makeItem({ id: "old", sectionId: "plan" }));
			activityStore.addItem(makeItem({ id: "mid", sectionId: "plan" }));
			activityStore.addItem(makeItem({ id: "new", sectionId: "plan" }));
			now.mockRestore();
			expect(activityStore.getForSection("plan").map((i) => i.id)).toEqual(["new", "mid", "old"]);
		});

		it("returns all section items when repoPath filter is omitted", () => {
			activityStore.addItem(makeItem({ id: "rp4", sectionId: "plan", repoPath: "/repo/a" }));
			activityStore.addItem(makeItem({ id: "rp5", sectionId: "plan", repoPath: "/repo/b" }));
			const all = activityStore.getForSection("plan");
			expect(all).toHaveLength(2);
		});
	});

	describe("getLastItem", () => {
		it("returns null when no items", () => {
			expect(activityStore.getLastItem()).toBeNull();
		});

		it("returns the most recently created non-dismissed item", () => {
			activityStore.addItem(makeItem({ id: "l1" }));
			activityStore.addItem(makeItem({ id: "l2" }));
			expect(activityStore.getLastItem()?.id).toBe("l2");
		});

		it("skips dismissed items", () => {
			activityStore.addItem(makeItem({ id: "l3" }));
			activityStore.addItem(makeItem({ id: "l4" }));
			activityStore.dismissItem("l4");
			expect(activityStore.getLastItem()?.id).toBe("l3");
		});

		it("returns null when all items are dismissed", () => {
			activityStore.addItem(makeItem({ id: "l5" }));
			activityStore.dismissItem("l5");
			expect(activityStore.getLastItem()).toBeNull();
		});

		it("filters by repoPath when provided", () => {
			activityStore.addItem(makeItem({ id: "lr1", repoPath: "/repo/a" }));
			activityStore.addItem(makeItem({ id: "lr2", repoPath: "/repo/b" }));
			expect(activityStore.getLastItem("/repo/a")?.id).toBe("lr1");
		});

		it("returns null when no items match the repoPath (all have different repoPath)", () => {
			activityStore.addItem(makeItem({ id: "lr3", repoPath: "/repo/a" }));
			expect(activityStore.getLastItem("/repo/z")).toBeNull();
		});

		it("includes items without repoPath when filtering", () => {
			activityStore.addItem(makeItem({ id: "lr4" })); // no repoPath
			activityStore.addItem(makeItem({ id: "lr5", repoPath: "/repo/a" }));
			expect(activityStore.getLastItem("/repo/x")?.id).toBe("lr4");
		});
	});

	// -------------------------------------------------------------------------
	// Dismiss
	// -------------------------------------------------------------------------
	describe("dismissItem", () => {
		it("marks a single item as dismissed", () => {
			activityStore.addItem(makeItem({ id: "d1" }));
			activityStore.dismissItem("d1");
			expect(activityStore.getActive().find((i) => i.id === "d1")).toBeUndefined();
		});

		it("is a no-op for unknown id", () => {
			activityStore.addItem(makeItem({ id: "d2" }));
			activityStore.dismissItem("unknown");
			expect(activityStore.getActive()).toHaveLength(1);
		});
	});

	describe("dismissSection", () => {
		it("dismisses all non-dismissed items in the section", () => {
			activityStore.addItem(makeItem({ id: "ds1", sectionId: "plan" }));
			activityStore.addItem(makeItem({ id: "ds2", sectionId: "plan" }));
			activityStore.addItem(makeItem({ id: "ds3", sectionId: "stories" }));
			activityStore.dismissSection("plan");
			expect(activityStore.getForSection("plan")).toHaveLength(0);
			expect(activityStore.getForSection("stories")).toHaveLength(1);
		});
	});

	// -------------------------------------------------------------------------
	// Reset
	// -------------------------------------------------------------------------
	describe("clearAll", () => {
		it("removes all items and sections", () => {
			activityStore.registerSection(makeSection({ id: "x" }));
			activityStore.addItem(makeItem({ id: "c1" }));
			activityStore.clearAll();
			expect(activityStore.getActive()).toHaveLength(0);
			expect(activityStore.getSections()).toHaveLength(0);
		});
	});
});

// ---------------------------------------------------------------------------
// Persistence tests (require mocked invoke)
// ---------------------------------------------------------------------------
const mockInvoke = vi.fn().mockResolvedValue(undefined);

describe("activityStore persistence", () => {
	let store: typeof import("../../stores/activityStore").activityStore;

	beforeEach(async () => {
		vi.resetModules();
		mockInvoke.mockReset().mockResolvedValue(undefined);
		vi.doMock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));
		store = (await import("../../stores/activityStore")).activityStore;
	});

	afterEach(() => {
		store._testCancelPendingSave();
	});

	describe("hydrate()", () => {
		it("loads active items from the array returned by the Activity API", async () => {
			store.addItem({
				id: "live",
				pluginId: "core",
				sectionId: "messages",
				title: "Live notice",
				icon: "<svg/>",
				dismissible: true,
			});
			mockInvoke.mockResolvedValueOnce([
				{
					id: "saved",
					pluginId: "core",
					sectionId: "messages",
					title: "Saved notice",
					icon: "<svg/>",
					dismissible: true,
					createdAt: 1000,
				},
				{
					id: "hidden",
					pluginId: "core",
					sectionId: "messages",
					title: "Dismissed notice",
					icon: "<svg/>",
					dismissible: true,
					dismissed: true,
					createdAt: 900,
				},
				{
					id: "live",
					pluginId: "core",
					sectionId: "messages",
					title: "Stale notice",
					icon: "<svg/>",
					dismissible: true,
					createdAt: 800,
				},
			]);

			await store.hydrate();
			expect(
				store
					.getActive()
					.map((item) => item.title)
					.sort(),
			).toEqual(["Live notice", "Saved notice"]);
			expect(store.state.items.find((item) => item.id === "hidden")?.dismissed).toBe(true);
		});

		it("loads items from backend", async () => {
			const savedItems = [
				{
					id: "a1",
					pluginId: "p1",
					sectionId: "plan",
					title: "Saved",
					icon: "<svg/>",
					dismissible: true,
					dismissed: false,
					createdAt: 1000,
				},
			];
			mockInvoke.mockResolvedValueOnce({ items: savedItems });

			await store.hydrate();
			expect(mockInvoke).toHaveBeenCalledWith("load_activity");
			expect(store.getActive()).toHaveLength(1);
			expect(store.getActive()[0].title).toBe("Saved");
			expect(store.getActive()[0].createdAt).toBe(1000);
		});

		it("keeps empty state when backend returns null", async () => {
			mockInvoke.mockResolvedValueOnce(null);
			await store.hydrate();
			expect(store.getActive()).toEqual([]);
		});

		it("keeps empty state on invoke failure", async () => {
			const consoleSpy = vi.spyOn(console, "debug").mockImplementation(() => {});
			mockInvoke.mockRejectedValueOnce(new Error("backend error"));
			await store.hydrate();
			expect(store.getActive()).toEqual([]);
			expect(consoleSpy).toHaveBeenCalledWith("[store]", "Failed to hydrate activity");
			consoleSpy.mockRestore();
		});

		it("fills missing dismissed field with false", async () => {
			const legacy = [
				{ id: "a1", pluginId: "p1", sectionId: "s", title: "T", icon: "<svg/>", dismissible: true, createdAt: 1000 },
			];
			mockInvoke.mockResolvedValueOnce({ items: legacy });
			await store.hydrate();
			expect(store.state.items[0].dismissed).toBe(false);
		});

		it("merges hydrated items with items added before hydrate", async () => {
			store.addItem({ id: "live", pluginId: "p1", sectionId: "s", title: "Live", icon: "<svg/>", dismissible: true });
			const saved = [
				{
					id: "saved",
					pluginId: "p1",
					sectionId: "s",
					title: "Saved",
					icon: "<svg/>",
					dismissible: true,
					createdAt: 1000,
				},
			];
			mockInvoke.mockResolvedValueOnce({ items: saved });
			await store.hydrate();
			// Live item takes precedence (already in store), saved item is added
			expect(
				store
					.getActive()
					.map((i) => i.id)
					.sort(),
			).toEqual(["live", "saved"]);
		});

		it("live items override saved items with same id", async () => {
			store.addItem({ id: "dup", pluginId: "p1", sectionId: "s", title: "Fresh", icon: "<svg/>", dismissible: true });
			const saved = [
				{
					id: "dup",
					pluginId: "p1",
					sectionId: "s",
					title: "Stale",
					icon: "<svg/>",
					dismissible: true,
					createdAt: 1000,
				},
			];
			mockInvoke.mockResolvedValueOnce({ items: saved });
			await store.hydrate();
			expect(store.getActive()).toHaveLength(1);
			expect(store.getActive()[0].title).toBe("Fresh");
		});

		it("preserves repoPath through persist/hydrate cycle", async () => {
			const saved = [
				{
					id: "rph",
					pluginId: "p1",
					sectionId: "plan",
					title: "Plan",
					icon: "<svg/>",
					dismissible: true,
					dismissed: false,
					createdAt: 1000,
					repoPath: "/repo/project",
				},
			];
			mockInvoke.mockResolvedValueOnce({ items: saved });
			await store.hydrate();
			expect(store.getActive()[0].repoPath).toBe("/repo/project");
		});
	});

	describe("save on mutations", () => {
		// saveActivity is debounced (300ms) — use fake timers to flush
		beforeEach(async () => {
			await store.hydrate();
			mockInvoke.mockClear();
			vi.useFakeTimers();
		});
		afterEach(() => {
			vi.useRealTimers();
		});

		it("persists after addItem", () => {
			store.addItem({ id: "x", pluginId: "p1", sectionId: "s", title: "T", icon: "<svg/>", dismissible: true });
			vi.advanceTimersByTime(300);
			expect(mockInvoke).toHaveBeenCalledWith("save_activity", { base: expect.anything(), items: expect.any(Array) });
		});

		it("persists after removeItem", async () => {
			store.addItem({ id: "x", pluginId: "p1", sectionId: "s", title: "T", icon: "<svg/>", dismissible: true });
			await vi.advanceTimersByTimeAsync(300);
			mockInvoke.mockClear();
			store.removeItem("x");
			await vi.advanceTimersByTimeAsync(300);
			expect(mockInvoke).toHaveBeenCalledWith("save_activity", { base: expect.anything(), items: expect.any(Array) });
		});

		it("persists after dismissItem", async () => {
			store.addItem({ id: "x", pluginId: "p1", sectionId: "s", title: "T", icon: "<svg/>", dismissible: true });
			await vi.advanceTimersByTimeAsync(300);
			mockInvoke.mockClear();
			store.dismissItem("x");
			await vi.advanceTimersByTimeAsync(300);
			expect(mockInvoke).toHaveBeenCalledWith("save_activity", { base: expect.anything(), items: expect.any(Array) });
		});

		it("persists after dismissSection", async () => {
			store.addItem({ id: "x", pluginId: "p1", sectionId: "s", title: "T", icon: "<svg/>", dismissible: true });
			await vi.advanceTimersByTimeAsync(300);
			mockInvoke.mockClear();
			store.dismissSection("s");
			await vi.advanceTimersByTimeAsync(300);
			expect(mockInvoke).toHaveBeenCalledWith("save_activity", { base: expect.anything(), items: expect.any(Array) });
		});

		it("persists after updateItem", async () => {
			store.addItem({ id: "x", pluginId: "p1", sectionId: "s", title: "T", icon: "<svg/>", dismissible: true });
			await vi.advanceTimersByTimeAsync(300);
			mockInvoke.mockClear();
			store.updateItem("x", { title: "Updated" });
			await vi.advanceTimersByTimeAsync(300);
			expect(mockInvoke).toHaveBeenCalledWith("save_activity", { base: expect.anything(), items: expect.any(Array) });
		});

		it("strips onClick from persisted items", () => {
			store.addItem({
				id: "x",
				pluginId: "p1",
				sectionId: "s",
				title: "T",
				icon: "<svg/>",
				dismissible: true,
				onClick: () => {},
			});
			vi.advanceTimersByTime(300);
			const saveCall = mockInvoke.mock.calls.find((c) => c[0] === "save_activity");
			expect(saveCall).toBeDefined();
			const saved = saveCall![1].items[0];
			expect(saved.onClick).toBeUndefined();
		});
	});
});

// The bell badge needs one number: how many items the popover will show. It got
// it by asking each section for its item LIST — a full scan plus a full sort per
// section, both thrown away for a `.length`. With S sections that is S scans and
// S sorts on every recount, and the ordering the sort produces is never read.
describe("activityStore.countActiveInSections (613-00e8 F108)", () => {
	beforeEach(() => {
		activityStore.clearAll();
	});

	afterEach(() => {
		activityStore._testCancelPendingSave();
	});

	it("counts the active items of several sections in one call", () => {
		activityStore.addItem(makeItem({ id: "a1", sectionId: "alpha" }));
		activityStore.addItem(makeItem({ id: "a2", sectionId: "alpha" }));
		activityStore.addItem(makeItem({ id: "b1", sectionId: "beta" }));
		activityStore.addItem(makeItem({ id: "c1", sectionId: "gamma" }));

		expect(activityStore.countActiveInSections(["alpha", "beta"])).toBe(3);
		expect(activityStore.countActiveInSections(["gamma"])).toBe(1);
		expect(activityStore.countActiveInSections([])).toBe(0);
	});

	it("skips dismissed items", () => {
		activityStore.addItem(makeItem({ id: "a1", sectionId: "alpha" }));
		activityStore.addItem(makeItem({ id: "a2", sectionId: "alpha" }));
		activityStore.dismissItem("a1");

		expect(activityStore.countActiveInSections(["alpha"])).toBe(1);
	});

	// Same repo rule as getForSection: an item with no repoPath belongs everywhere.
	it("applies the repo filter exactly as getForSection does", () => {
		activityStore.addItem(makeItem({ id: "mine", sectionId: "alpha", repoPath: "/repo/a" }));
		activityStore.addItem(makeItem({ id: "theirs", sectionId: "alpha", repoPath: "/repo/b" }));
		activityStore.addItem(makeItem({ id: "global", sectionId: "alpha" }));

		expect(activityStore.countActiveInSections(["alpha"], "/repo/a")).toBe(2);
		expect(activityStore.countActiveInSections(["alpha"], "/repo/a")).toBe(
			activityStore.getForSection("alpha", "/repo/a").length,
		);
		expect(activityStore.countActiveInSections(["alpha"])).toBe(3);
	});
});

// `setState("items", prev => prev.filter(...))` hands the store a brand new
// array, so every consumer of the list re-runs even when the item it reads was
// not touched. A targeted splice keeps the untouched rows quiet.
describe("activityStore list mutations (613-00e8 F107)", () => {
	beforeEach(() => {
		activityStore.clearAll();
	});

	afterEach(() => {
		activityStore._testCancelPendingSave();
	});

	it("does not disturb a surviving item when another is removed", async () => {
		const { createEffect, createRoot } = await import("solid-js");
		activityStore.addItem(makeItem({ id: "keep", title: "Keep me" }));
		activityStore.addItem(makeItem({ id: "drop", title: "Drop me" }));

		let reads = 0;
		const dispose = createRoot((disposeFn) => {
			createEffect(() => {
				// Read ONE surviving row by path. Iterating the list with find()
				// would subscribe to the iteration itself and re-run either way,
				// which is why this reads index 0 directly.
				activityStore.state.items[0]?.title;
				reads++;
			});
			return disposeFn;
		});
		await Promise.resolve();
		const before = reads;

		activityStore.removeItem("drop");
		await Promise.resolve();

		expect(activityStore.state.items.map((i) => i.id)).toEqual(["keep"]);
		expect(reads).toBe(before);
		dispose();
	});

	it("still removes the right item", () => {
		activityStore.addItem(makeItem({ id: "a" }));
		activityStore.addItem(makeItem({ id: "b" }));
		activityStore.addItem(makeItem({ id: "c" }));

		activityStore.removeItem("b");

		expect(activityStore.state.items.map((i) => i.id)).toEqual(["a", "c"]);
	});

	it("removing an unknown id changes nothing", () => {
		activityStore.addItem(makeItem({ id: "a" }));

		activityStore.removeItem("nope");

		expect(activityStore.state.items.map((i) => i.id)).toEqual(["a"]);
	});
});

// Catches: importing the store creates a module-level hydration promise that stays
// pending in every suite that never hydrates, which Vitest reports as an async leak.
it("importing the store leaves no pending promise of its own", async () => {
	const { createHook } = await import("node:async_hooks");
	vi.resetModules();
	const pending = new Map<number, string>();
	const hook = createHook({
		init(asyncId, type) {
			if (type !== "PROMISE") return;
			const stack = new Error("activity-store-import").stack ?? "";
			if (stack.includes("stores/activityStore.ts")) pending.set(asyncId, stack);
		},
		promiseResolve(asyncId) {
			pending.delete(asyncId);
		},
	});
	hook.enable();
	try {
		await import("../../stores/activityStore");
		await new Promise((resolve) => setImmediate(resolve));
	} finally {
		hook.disable();
	}
	expect([...pending.values()]).toEqual([]);
});
