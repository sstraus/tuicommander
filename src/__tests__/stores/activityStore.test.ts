import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { activityStore } from "../../stores/activityStore";
import type { ActivitySection, ActivityItem } from "../../plugins/types";

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

  describe("hydrate()", () => {
    it("loads items from backend", async () => {
      const savedItems = [
        { id: "a1", pluginId: "p1", sectionId: "plan", title: "Saved", icon: "<svg/>", dismissible: true, dismissed: false, createdAt: 1000 },
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
      expect(consoleSpy).toHaveBeenCalledWith("Failed to hydrate activity:", expect.any(Error));
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
        { id: "saved", pluginId: "p1", sectionId: "s", title: "Saved", icon: "<svg/>", dismissible: true, createdAt: 1000 },
      ];
      mockInvoke.mockResolvedValueOnce({ items: saved });
      await store.hydrate();
      // Live item takes precedence (already in store), saved item is added
      expect(store.getActive().map((i) => i.id).sort()).toEqual(["live", "saved"]);
    });

    it("live items override saved items with same id", async () => {
      store.addItem({ id: "dup", pluginId: "p1", sectionId: "s", title: "Fresh", icon: "<svg/>", dismissible: true });
      const saved = [
        { id: "dup", pluginId: "p1", sectionId: "s", title: "Stale", icon: "<svg/>", dismissible: true, createdAt: 1000 },
      ];
      mockInvoke.mockResolvedValueOnce({ items: saved });
      await store.hydrate();
      expect(store.getActive()).toHaveLength(1);
      expect(store.getActive()[0].title).toBe("Fresh");
    });
  });

  describe("save on mutations", () => {
    it("persists after addItem", () => {
      store.addItem({ id: "x", pluginId: "p1", sectionId: "s", title: "T", icon: "<svg/>", dismissible: true });
      expect(mockInvoke).toHaveBeenCalledWith("save_activity", { items: expect.any(Array) });
    });

    it("persists after removeItem", () => {
      store.addItem({ id: "x", pluginId: "p1", sectionId: "s", title: "T", icon: "<svg/>", dismissible: true });
      mockInvoke.mockClear();
      store.removeItem("x");
      expect(mockInvoke).toHaveBeenCalledWith("save_activity", { items: expect.any(Array) });
    });

    it("persists after dismissItem", () => {
      store.addItem({ id: "x", pluginId: "p1", sectionId: "s", title: "T", icon: "<svg/>", dismissible: true });
      mockInvoke.mockClear();
      store.dismissItem("x");
      expect(mockInvoke).toHaveBeenCalledWith("save_activity", { items: expect.any(Array) });
    });

    it("persists after dismissSection", () => {
      store.addItem({ id: "x", pluginId: "p1", sectionId: "s", title: "T", icon: "<svg/>", dismissible: true });
      mockInvoke.mockClear();
      store.dismissSection("s");
      expect(mockInvoke).toHaveBeenCalledWith("save_activity", { items: expect.any(Array) });
    });

    it("persists after updateItem", () => {
      store.addItem({ id: "x", pluginId: "p1", sectionId: "s", title: "T", icon: "<svg/>", dismissible: true });
      mockInvoke.mockClear();
      store.updateItem("x", { title: "Updated" });
      expect(mockInvoke).toHaveBeenCalledWith("save_activity", { items: expect.any(Array) });
    });

    it("strips onClick from persisted items", () => {
      store.addItem({ id: "x", pluginId: "p1", sectionId: "s", title: "T", icon: "<svg/>", dismissible: true, onClick: () => {} });
      const saveCall = mockInvoke.mock.calls.find((c) => c[0] === "save_activity");
      expect(saveCall).toBeDefined();
      const saved = saveCall![1].items[0];
      expect(saved.onClick).toBeUndefined();
    });
  });
});
