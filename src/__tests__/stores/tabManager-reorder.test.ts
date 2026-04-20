import { describe, it, expect, beforeEach } from "vitest";
import { createTabManager, type BaseTab } from "../../stores/tabManager";
import { testInScope } from "../helpers/store";

interface TestTab extends BaseTab {
  id: string;
  label: string;
}

function makeTab(id: string, overrides: Partial<TestTab> = {}): TestTab {
  return { id, label: `Tab ${id}`, ...overrides };
}

describe("createTabManager — reorderByIds", () => {
  let mgr: ReturnType<typeof createTabManager<TestTab>>;

  beforeEach(() => {
    mgr = createTabManager<TestTab>();
  });

  it("moves a tab before another", () => {
    testInScope(() => {
      mgr._addTab(makeTab("a"));
      mgr._addTab(makeTab("b"));
      mgr._addTab(makeTab("c"));
      expect(mgr.getVisibleIds(null)).toEqual(["a", "b", "c"]);

      mgr.reorderByIds("c", "a", "before");
      expect(mgr.getVisibleIds(null)).toEqual(["c", "a", "b"]);
    });
  });

  it("moves a tab after another", () => {
    testInScope(() => {
      mgr._addTab(makeTab("a"));
      mgr._addTab(makeTab("b"));
      mgr._addTab(makeTab("c"));

      mgr.reorderByIds("a", "c", "after");
      expect(mgr.getVisibleIds(null)).toEqual(["b", "c", "a"]);
    });
  });

  it("no-ops when source equals target", () => {
    testInScope(() => {
      mgr._addTab(makeTab("a"));
      mgr._addTab(makeTab("b"));

      mgr.reorderByIds("a", "a", "before");
      expect(mgr.getVisibleIds(null)).toEqual(["a", "b"]);
    });
  });

  it("no-ops when source id is unknown", () => {
    testInScope(() => {
      mgr._addTab(makeTab("a"));
      mgr._addTab(makeTab("b"));

      mgr.reorderByIds("x", "a", "before");
      expect(mgr.getVisibleIds(null)).toEqual(["a", "b"]);
    });
  });

  it("no-ops when target id is unknown", () => {
    testInScope(() => {
      mgr._addTab(makeTab("a"));
      mgr._addTab(makeTab("b"));

      mgr.reorderByIds("a", "x", "before");
      expect(mgr.getVisibleIds(null)).toEqual(["a", "b"]);
    });
  });

  it("order survives remove()", () => {
    testInScope(() => {
      mgr._addTab(makeTab("a"));
      mgr._addTab(makeTab("b"));
      mgr._addTab(makeTab("c"));
      mgr.reorderByIds("c", "a", "before"); // c, a, b

      mgr.remove("a");
      expect(mgr.getVisibleIds(null)).toEqual(["c", "b"]);
    });
  });

  it("order resets on clearAll()", () => {
    testInScope(() => {
      mgr._addTab(makeTab("a"));
      mgr._addTab(makeTab("b"));
      mgr.reorderByIds("b", "a", "before");

      mgr.clearAll();
      mgr._addTab(makeTab("x"));
      mgr._addTab(makeTab("y"));
      expect(mgr.getVisibleIds(null)).toEqual(["x", "y"]);
    });
  });
});
