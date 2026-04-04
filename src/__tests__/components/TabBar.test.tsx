import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@solidjs/testing-library";
import { getModifierSymbol } from "../../platform";

// Mock Tauri APIs
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(vi.fn()),
  emit: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({
    listen: vi.fn().mockResolvedValue(vi.fn()),
    setTitle: vi.fn().mockResolvedValue(undefined),
  })),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn().mockResolvedValue(null),
}));
vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: vi.fn().mockResolvedValue(undefined),
}));

import { TabBar } from "../../components/TabBar/TabBar";
import { terminalsStore } from "../../stores/terminals";
import { repositoriesStore } from "../../stores/repositories";
import { diffTabsStore } from "../../stores/diffTabs";
import { mdTabsStore } from "../../stores/mdTabs";

describe("TabBar", () => {
  beforeEach(() => {
    localStorage.clear();
    // Clean up any terminals from previous tests
    for (const id of terminalsStore.getIds()) {
      terminalsStore.remove(id);
    }
    // Clean up repos
    for (const path of repositoriesStore.getPaths()) {
      repositoriesStore.remove(path);
    }
    repositoriesStore.setActive(null);
    // Clean up diff/md tabs
    for (const id of diffTabsStore.getIds()) {
      diffTabsStore.remove(id);
    }
    for (const id of mdTabsStore.getIds()) {
      mdTabsStore.remove(id);
    }
  });

  function addTerminal(overrides: Partial<{ name: string; sessionId: string | null; fontSize: number; cwd: string | null; awaitingInput: any }> = {}) {
    return terminalsStore.add({
      name: overrides.name ?? "Terminal",
      sessionId: overrides.sessionId ?? null,
      fontSize: overrides.fontSize ?? 14,
      cwd: overrides.cwd ?? null,
      awaitingInput: overrides.awaitingInput ?? null,
    });
  }

  it("renders the tab container", () => {
    const { container } = render(() => (
      <TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
    ));
    expect(container.querySelector(".tabs")).not.toBeNull();
  });

  it("renders the new tab button", () => {
    const { container } = render(() => (
      <TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
    ));
    const btn = container.querySelector(".newBtn");
    expect(btn).not.toBeNull();
    expect(btn!.textContent).toContain("+");
  });

  it("clicking new tab button calls onNewTab directly", () => {
    const onNewTab = vi.fn();
    const { container } = render(() => (
      <TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={onNewTab} />
    ));
    fireEvent.click(container.querySelector(".newBtn")!);
    expect(onNewTab).toHaveBeenCalledTimes(1);
  });

  it("right-clicking new tab button opens split context menu", () => {
    const { container } = render(() => (
      <TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
    ));
    const btn = container.querySelector(".newBtn")!;
    vi.spyOn(btn, "getBoundingClientRect").mockReturnValue({
      left: 100, bottom: 50, top: 20, right: 150, width: 50, height: 30, x: 100, y: 20, toJSON: () => {},
    } as DOMRect);
    fireEvent.contextMenu(btn);
    const menus = container.querySelectorAll(".menu");
    expect(menus.length).toBeGreaterThan(0);
    const labels = Array.from(menus[menus.length - 1].querySelectorAll(".label"));
    const labelTexts = labels.map(l => l.textContent);
    expect(labelTexts).toContain("New Tab");
    expect(labelTexts).toContain("Split Vertically");
    expect(labelTexts).toContain("Split Horizontally");
  });

  it("new tab button has correct title", () => {
    const { container } = render(() => (
      <TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
    ));
    expect(container.querySelector(".newBtn")!.getAttribute("title")).toBe(`New Tab (${getModifierSymbol()}T)`);
  });

  it("renders terminal tabs from the store", () => {
    addTerminal({ name: "Tab A" });
    addTerminal({ name: "Tab B" });

    const { container } = render(() => (
      <TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
    ));
    const tabs = container.querySelectorAll(".tab");
    expect(tabs.length).toBe(2);
    expect(tabs[0].querySelector(".tabName")!.textContent).toContain("Tab A");
    expect(tabs[1].querySelector(".tabName")!.textContent).toContain("Tab B");
  });

  it("active tab has 'active' class", () => {
    const id1 = addTerminal({ name: "Tab 1" });
    addTerminal({ name: "Tab 2" });
    terminalsStore.setActive(id1);

    const { container } = render(() => (
      <TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
    ));
    const tabs = container.querySelectorAll(".tab");
    expect(tabs[0].classList.contains("active")).toBe(true);
    expect(tabs[1].classList.contains("active")).toBe(false);
  });

  it("activity flag does NOT produce a visible dot class (busy covers it)", () => {
    const id1 = addTerminal({ name: "Active" });
    const id2 = addTerminal({ name: "Background" });
    terminalsStore.setActive(id1);
    terminalsStore.update(id2, { activity: true });

    const { container } = render(() => (
      <TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
    ));
    const tabs = container.querySelectorAll(".tab");
    // activity flag no longer drives dot color — busy state does
    expect(tabs[0].classList.contains("hasActivity")).toBe(false);
    expect(tabs[1].classList.contains("hasActivity")).toBe(false);
  });

  it("tab awaiting input has 'awaitingInput awaitingQuestion' class", () => {
    const id = addTerminal({ name: "Waiting", awaitingInput: "question" });
    terminalsStore.setActive(id);

    const { container } = render(() => (
      <TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
    ));
    const tab = container.querySelector(".tab")!;
    expect(tab.classList.contains("awaitingInput")).toBe(true);
    expect(tab.classList.contains("awaitingQuestion")).toBe(true);
  });

  it("tab awaiting error input has correct class", () => {
    const id = addTerminal({ name: "Error", awaitingInput: "error" });
    terminalsStore.setActive(id);

    const { container } = render(() => (
      <TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
    ));
    const tab = container.querySelector(".tab")!;
    expect(tab.classList.contains("awaitingInput")).toBe(true);
    expect(tab.classList.contains("awaitingError")).toBe(true);
  });

  it("tab with no awaitingInput has no awaiting classes", () => {
    const id = addTerminal({ name: "Normal" });
    terminalsStore.setActive(id);

    const { container } = render(() => (
      <TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
    ));
    const tab = container.querySelector(".tab")!;
    expect(tab.classList.contains("awaitingInput")).toBe(false);
  });

  it("non-active tab with shellState 'idle' has 'shellIdle' class", () => {
    const id1 = addTerminal({ name: "Active" });
    const id2 = addTerminal({ name: "Idle" });
    terminalsStore.setActive(id1);
    terminalsStore.update(id2, { shellState: "idle" });

    const { container } = render(() => (
      <TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
    ));
    const tabs = container.querySelectorAll(".tab");
    expect(tabs[0].classList.contains("shellIdle")).toBe(false);
    expect(tabs[1].classList.contains("shellIdle")).toBe(true);
  });

  it("active tab with shellState 'idle' has 'shellIdle' class", () => {
    const id1 = addTerminal({ name: "Active" });
    terminalsStore.setActive(id1);
    terminalsStore.update(id1, { shellState: "idle" });

    const { container } = render(() => (
      <TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
    ));
    const tab = container.querySelector(".tab")!;
    expect(tab.classList.contains("shellIdle")).toBe(true);
  });

  it("non-active tab with shellState 'busy' does NOT have 'shellIdle' class", () => {
    const id1 = addTerminal({ name: "Active" });
    const id2 = addTerminal({ name: "Busy" });
    terminalsStore.setActive(id1);
    terminalsStore.update(id2, { shellState: "busy" });

    const { container } = render(() => (
      <TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
    ));
    const tabs = container.querySelectorAll(".tab");
    expect(tabs[1].classList.contains("shellIdle")).toBe(false);
  });

  it("tab with progress shows progress label and bar", () => {
    const id = addTerminal({ name: "Progress" });
    terminalsStore.update(id, { progress: 50 });

    const { container } = render(() => (
      <TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
    ));
    const tab = container.querySelector(".tab")!;
    const label = tab.querySelector(".progressLabel");
    expect(label).not.toBeNull();
    expect(label!.textContent).toBe("50%");
    const bar = tab.querySelector(".progress");
    expect(bar).not.toBeNull();
    expect((bar as HTMLElement).style.width).toBe("50%");
  });

  it("tab with progress=0 shows progress label and bar", () => {
    const id = addTerminal({ name: "Zero" });
    terminalsStore.update(id, { progress: 0 });

    const { container } = render(() => (
      <TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
    ));
    const tab = container.querySelector(".tab")!;
    const label = tab.querySelector(".progressLabel");
    expect(label).not.toBeNull();
    expect(label!.textContent).toBe("0%");
  });

  it("tab with progress=null does not show progress elements", () => {
    addTerminal({ name: "No Progress" });
    // progress defaults to null

    const { container } = render(() => (
      <TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
    ));
    const tab = container.querySelector(".tab")!;
    expect(tab.querySelector(".progressLabel")).toBeNull();
    expect(tab.querySelector(".progress")).toBeNull();
  });

  it("clicking tab calls onTabSelect with correct id", () => {
    const handleSelect = vi.fn();
    addTerminal({ name: "Tab 1" });
    const id2 = addTerminal({ name: "Tab 2" });

    const { container } = render(() => (
      <TabBar onTabSelect={handleSelect} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
    ));
    const tabs = container.querySelectorAll(".tab");
    fireEvent.click(tabs[1]);
    expect(handleSelect).toHaveBeenCalledWith(id2);
  });

  it("clicking close button calls onTabClose and stops propagation", () => {
    const handleSelect = vi.fn();
    const handleClose = vi.fn();
    const id1 = addTerminal({ name: "Tab 1" });

    const { container } = render(() => (
      <TabBar onTabSelect={handleSelect} onTabClose={handleClose} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
    ));
    const closeBtn = container.querySelector(".tabClose")!;
    fireEvent.click(closeBtn);
    expect(handleClose).toHaveBeenCalledWith(id1);
    // stopPropagation means onTabSelect should NOT be called
    expect(handleSelect).not.toHaveBeenCalled();
  });

  it("when activeRepoPath and activeBranch set, uses branch terminals", () => {
    // Set up a repo with a branch that has specific terminals
    const repoPath = "/test/repo";
    repositoriesStore.add({ path: repoPath, displayName: "Test Repo" });
    repositoriesStore.setActive(repoPath);

    const t1 = addTerminal({ name: "Branch Term 1" });
    const t2 = addTerminal({ name: "Branch Term 2" });
    addTerminal({ name: "Other Term" }); // Not in the branch

    repositoriesStore.setBranch(repoPath, "main", {
      name: "main",
      terminals: [t1, t2],
    });
    repositoriesStore.setActiveBranch(repoPath, "main");

    const { container } = render(() => (
      <TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
    ));
    const tabs = container.querySelectorAll(".tab");
    // Should only show t1 and t2, not t3
    expect(tabs.length).toBe(2);
    expect(tabs[0].querySelector(".tabName")!.textContent).toContain("Branch Term 1");
    expect(tabs[1].querySelector(".tabName")!.textContent).toContain("Branch Term 2");
  });

  it("with no activeRepoPath, shows all terminals", () => {
    addTerminal({ name: "T1" });
    addTerminal({ name: "T2" });
    addTerminal({ name: "T3" });

    const { container } = render(() => (
      <TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
    ));
    expect(container.querySelectorAll(".tab").length).toBe(3);
  });

  it("with activeRepoPath but no activeBranch, falls back to all terminals", () => {
    const repoPath = "/test/repo";
    repositoriesStore.add({ path: repoPath, displayName: "Test Repo" });
    repositoriesStore.setActive(repoPath);
    // No activeBranch set

    addTerminal({ name: "T1" });
    addTerminal({ name: "T2" });

    const { container } = render(() => (
      <TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
    ));
    expect(container.querySelectorAll(".tab").length).toBe(2);
  });

  it("tab has correct title with 1-based index", () => {
    addTerminal({ name: "First" });
    addTerminal({ name: "Second" });

    const { container } = render(() => (
      <TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
    ));
    const tabs = container.querySelectorAll(".tab");
    expect(tabs[0].getAttribute("title")).toBe(`Terminal 1 (${getModifierSymbol()}1)`);
    expect(tabs[1].getAttribute("title")).toBe(`Terminal 2 (${getModifierSymbol()}2)`);
  });

  it("tabs have draggable attribute", () => {
    addTerminal({ name: "Tab" });

    const { container } = render(() => (
      <TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
    ));
    const tab = container.querySelector(".tab")!;
    expect(tab.getAttribute("draggable")).toBe("true");
  });

  it("dragStart sets dragging class", () => {
    addTerminal({ name: "Tab 1" });
    addTerminal({ name: "Tab 2" });

    const { container } = render(() => (
      <TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
    ));
    const tabs = container.querySelectorAll(".tab");

    // Simulate drag start on first tab
    const dataTransfer = {
      effectAllowed: "",
      setData: vi.fn(),
      getData: vi.fn().mockReturnValue(""),
      dropEffect: "",
    };
    fireEvent.dragStart(tabs[0], { dataTransfer });
    expect(tabs[0].classList.contains("dragging")).toBe(true);
  });

  it("dragOver on different tab sets drag-over classes", () => {
    const id1 = addTerminal({ name: "Tab 1" });
    addTerminal({ name: "Tab 2" });

    const { container } = render(() => (
      <TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
    ));
    const tabs = container.querySelectorAll(".tab");

    // Start dragging tab 1
    const dataTransfer = {
      effectAllowed: "",
      setData: vi.fn(),
      getData: vi.fn().mockReturnValue(id1),
      dropEffect: "",
    };
    fireEvent.dragStart(tabs[0], { dataTransfer });

    // Drag over tab 2. In happy-dom, getBoundingClientRect returns all zeros,
    // so midpoint=0 and clientX=0 means side="right"
    fireEvent.dragOver(tabs[1], {
      dataTransfer,
      clientX: 0,
    });

    // Tab 2 should have dragOverRight class (midpoint=0, clientX=0 => "right")
    expect(tabs[1].classList.contains("dragOverRight")).toBe(true);
  });

  it("dragLeave resets drag-over state", () => {
    const id1 = addTerminal({ name: "Tab 1" });
    addTerminal({ name: "Tab 2" });

    const { container } = render(() => (
      <TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
    ));
    const tabs = container.querySelectorAll(".tab");

    const dataTransfer = {
      effectAllowed: "",
      setData: vi.fn(),
      getData: vi.fn().mockReturnValue(id1),
      dropEffect: "",
    };
    fireEvent.dragStart(tabs[0], { dataTransfer });
    fireEvent.dragOver(tabs[1], { dataTransfer, clientX: 0 });
    fireEvent.dragLeave(tabs[1]);

    expect(tabs[1].classList.contains("dragOverLeft")).toBe(false);
    expect(tabs[1].classList.contains("dragOverRight")).toBe(false);
  });

  it("drop on same target resets state without reorder", () => {
    const handleReorder = vi.fn();
    const id1 = addTerminal({ name: "Tab 1" });

    const { container } = render(() => (
      <TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} onReorder={handleReorder} />
    ));
    const tabs = container.querySelectorAll(".tab");

    const dataTransfer = {
      effectAllowed: "",
      setData: vi.fn(),
      getData: vi.fn().mockReturnValue(id1),
      dropEffect: "",
    };
    fireEvent.dragStart(tabs[0], { dataTransfer });
    fireEvent.drop(tabs[0], { dataTransfer });

    expect(handleReorder).not.toHaveBeenCalled();
    expect(tabs[0].classList.contains("dragging")).toBe(false);
  });

  it("drop reorders correctly when dropping on different target", () => {
    const handleReorder = vi.fn();
    const id1 = addTerminal({ name: "Tab 1" });
    addTerminal({ name: "Tab 2" });

    const { container } = render(() => (
      <TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} onReorder={handleReorder} />
    ));
    const tabs = container.querySelectorAll(".tab");

    const dataTransfer = {
      effectAllowed: "",
      setData: vi.fn(),
      getData: vi.fn().mockReturnValue(id1),
      dropEffect: "",
    };
    fireEvent.dragStart(tabs[0], { dataTransfer });
    fireEvent.drop(tabs[1], { dataTransfer });

    expect(handleReorder).toHaveBeenCalledWith(0, 1);
  });

  it("dragEnd resets all drag state", () => {
    const id1 = addTerminal({ name: "Tab 1" });
    addTerminal({ name: "Tab 2" });

    const { container } = render(() => (
      <TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
    ));
    const tabs = container.querySelectorAll(".tab");

    const dataTransfer = {
      effectAllowed: "",
      setData: vi.fn(),
      getData: vi.fn().mockReturnValue(id1),
      dropEffect: "",
    };
    fireEvent.dragStart(tabs[0], { dataTransfer });
    fireEvent.dragEnd(tabs[0]);

    expect(tabs[0].classList.contains("dragging")).toBe(false);
  });

  it("drop with no dataTransfer source resets state", () => {
    const handleReorder = vi.fn();
    addTerminal({ name: "Tab 1" });

    const { container } = render(() => (
      <TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} onReorder={handleReorder} />
    ));
    const tabs = container.querySelectorAll(".tab");

    // Drop with empty getData (no source)
    const dataTransfer = {
      effectAllowed: "",
      setData: vi.fn(),
      getData: vi.fn().mockReturnValue(""),
      dropEffect: "",
    };
    fireEvent.drop(tabs[0], { dataTransfer });

    expect(handleReorder).not.toHaveBeenCalled();
  });

  it("drop on right side with adjusted index calls onReorder", () => {
    const handleReorder = vi.fn();
    const id1 = addTerminal({ name: "Tab 1" });
    addTerminal({ name: "Tab 2" });
    addTerminal({ name: "Tab 3" });

    const { container } = render(() => (
      <TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} onReorder={handleReorder} />
    ));
    const tabs = container.querySelectorAll(".tab");

    const dataTransfer = {
      effectAllowed: "",
      setData: vi.fn(),
      getData: vi.fn().mockReturnValue(id1),
      dropEffect: "",
    };

    // Drag tab 1 and drop on tab 3
    fireEvent.dragStart(tabs[0], { dataTransfer });
    // dragOver sets the side (in happy-dom getBoundingClientRect is all zeros, side="right")
    fireEvent.dragOver(tabs[2], { dataTransfer, clientX: 0 });
    fireEvent.drop(tabs[2], { dataTransfer });

    // fromIndex=0, toIndex=2, side="right" and toIndex > fromIndex => adjustedTo=2
    expect(handleReorder).toHaveBeenCalledWith(0, 2);
  });

  describe("diff tabs", () => {
    /** Set up an active repo+branch so diff/md tab visibility filtering works */
    function setupActiveRepo() {
      repositoriesStore.add({ path: "/repo", displayName: "repo" });
      repositoriesStore.setActive("/repo");
      repositoriesStore.setActiveBranch("/repo", "main");
    }

    it("renders diff tabs", () => {
      setupActiveRepo();
      diffTabsStore.add("/repo", "/repo/file.ts", "M");

      const { container } = render(() => (
        <TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
      ));
      const diffTabs = container.querySelectorAll(".diffTab");
      expect(diffTabs.length).toBe(1);
      expect(diffTabs[0].querySelector(".tabName")!.textContent).toBe("file.ts");
    });

    it("clicking diff tab selects it", () => {
      setupActiveRepo();
      const id = diffTabsStore.add("/repo", "/repo/file.ts", "M");
      const handleSelect = vi.fn();

      const { container } = render(() => (
        <TabBar onTabSelect={handleSelect} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
      ));
      fireEvent.click(container.querySelector(".diffTab")!);
      expect(handleSelect).toHaveBeenCalledWith(id);
      expect(diffTabsStore.state.activeId).toBe(id);
    });

    it("closing diff tab via close button", () => {
      setupActiveRepo();
      const id = diffTabsStore.add("/repo", "/repo/file.ts", "M");
      const handleClose = vi.fn();

      const { container } = render(() => (
        <TabBar onTabSelect={() => {}} onTabClose={handleClose} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
      ));
      const closeBtn = container.querySelector(".diffTab .tabClose")!;
      fireEvent.click(closeBtn);
      expect(handleClose).toHaveBeenCalledWith(id);
    });

    it("middle-click closes diff tab", () => {
      setupActiveRepo();
      const id = diffTabsStore.add("/repo", "/repo/file.ts", "M");
      const handleClose = vi.fn();

      const { container } = render(() => (
        <TabBar onTabSelect={() => {}} onTabClose={handleClose} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
      ));
      fireEvent(container.querySelector(".diffTab")!, new MouseEvent("auxclick", { button: 1, bubbles: true }));
      expect(handleClose).toHaveBeenCalledWith(id);
    });
  });

  describe("markdown tabs", () => {
    function setupActiveRepo() {
      repositoriesStore.add({ path: "/repo", displayName: "repo" });
      repositoriesStore.setActive("/repo");
      repositoriesStore.setActiveBranch("/repo", "main");
    }

    it("renders markdown tabs", () => {
      setupActiveRepo();
      mdTabsStore.add("/repo", "/repo/readme.md");

      const { container } = render(() => (
        <TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
      ));
      const mdTabs = container.querySelectorAll(".mdTab");
      expect(mdTabs.length).toBe(1);
      expect(mdTabs[0].querySelector(".tabName")!.textContent).toBe("readme.md");
    });

    it("clicking md tab selects it", () => {
      setupActiveRepo();
      const id = mdTabsStore.add("/repo", "/repo/readme.md");
      const handleSelect = vi.fn();

      const { container } = render(() => (
        <TabBar onTabSelect={handleSelect} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
      ));
      fireEvent.click(container.querySelector(".mdTab")!);
      expect(handleSelect).toHaveBeenCalledWith(id);
      expect(mdTabsStore.state.activeId).toBe(id);
    });

    it("closing md tab via close button", () => {
      setupActiveRepo();
      const id = mdTabsStore.add("/repo", "/repo/readme.md");
      const handleClose = vi.fn();

      const { container } = render(() => (
        <TabBar onTabSelect={() => {}} onTabClose={handleClose} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
      ));
      const closeBtn = container.querySelector(".mdTab .tabClose")!;
      fireEvent.click(closeBtn);
      expect(handleClose).toHaveBeenCalledWith(id);
    });
  });

  describe("tab rename", () => {
    it("double-click enters edit mode", () => {
      const id = addTerminal({ name: "My Tab" });
      terminalsStore.setActive(id);

      const { container } = render(() => (
        <TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
      ));
      const tab = container.querySelector(".tab")!;
      fireEvent.dblClick(tab);
      const input = tab.querySelector(".tabNameInput");
      expect(input).not.toBeNull();
    });

    it("Enter key commits rename", () => {
      const id = addTerminal({ name: "Old Name" });
      terminalsStore.setActive(id);

      const { container } = render(() => (
        <TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
      ));
      const tab = container.querySelector(".tab")!;
      fireEvent.dblClick(tab);
      const input = tab.querySelector(".tabNameInput") as HTMLInputElement;
      fireEvent.input(input, { target: { value: "New Name" } });
      fireEvent.keyDown(input, { key: "Enter" });
      expect(terminalsStore.get(id)?.name).toBe("New Name");
    });

    it("Escape key cancels rename", () => {
      const id = addTerminal({ name: "Original" });
      terminalsStore.setActive(id);

      const { container } = render(() => (
        <TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
      ));
      const tab = container.querySelector(".tab")!;
      fireEvent.dblClick(tab);
      const input = tab.querySelector(".tabNameInput") as HTMLInputElement;
      fireEvent.input(input, { target: { value: "Changed" } });
      fireEvent.keyDown(input, { key: "Escape" });
      // Name should remain original (Escape cancels)
      expect(terminalsStore.get(id)?.name).toBe("Original");
    });
  });

  describe("context menu", () => {
    it("right-click opens context menu", () => {
      addTerminal({ name: "Tab 1" });

      const { container } = render(() => (
        <TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
      ));
      const tab = container.querySelector(".tab")!;
      fireEvent.contextMenu(tab);
      const menu = container.querySelector(".menu");
      expect(menu).not.toBeNull();
    });
  });

  describe("new tab menu", () => {
    it("disables split when no active terminal", () => {
      // Clear all terminals so there's no activeId
      for (const id of terminalsStore.getIds()) {
        terminalsStore.remove(id);
      }

      const { container } = render(() => (
        <TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
      ));
      const btn = container.querySelector(".newBtn")!;
      vi.spyOn(btn, "getBoundingClientRect").mockReturnValue({
        left: 100, bottom: 50, top: 20, right: 150, width: 50, height: 30, x: 100, y: 20, toJSON: () => {},
      } as DOMRect);
      fireEvent.contextMenu(btn);
      const menus = container.querySelectorAll(".menu");
      const menu = menus[menus.length - 1];
      const items = Array.from(menu.querySelectorAll(".item"));
      const splitV = items.find(i => i.textContent?.includes("Split Vertically"));
      const splitH = items.find(i => i.textContent?.includes("Split Horizontally"));
      expect(splitV).toBeDefined();
      expect(splitH).toBeDefined();
      expect(splitV!.classList.contains("disabled")).toBe(true);
      expect(splitH!.classList.contains("disabled")).toBe(true);
    });

    it("calls onSplitVertical when Split Vertically is clicked", () => {
      const handleSplit = vi.fn();
      const id = addTerminal({ name: "T1" });
      terminalsStore.setActive(id);

      const { container } = render(() => (
        <TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} onSplitVertical={handleSplit} />
      ));
      const btn = container.querySelector(".newBtn")!;
      vi.spyOn(btn, "getBoundingClientRect").mockReturnValue({
        left: 100, bottom: 50, top: 20, right: 150, width: 50, height: 30, x: 100, y: 20, toJSON: () => {},
      } as DOMRect);
      fireEvent.contextMenu(btn);
      const menus = container.querySelectorAll(".menu");
      const menu = menus[menus.length - 1];
      const splitBtn = Array.from(menu.querySelectorAll(".item")).find(
        i => i.textContent?.includes("Split Vertically")
      );
      fireEvent.click(splitBtn!);
      expect(handleSplit).toHaveBeenCalledOnce();
    });
  });

  describe("tab close", () => {
    it("close button removes the terminal", () => {
      const id1 = addTerminal({ name: "T1" });
      addTerminal({ name: "T2" });
      terminalsStore.setActive(id1);

      const handleClose = vi.fn();

      const { container } = render(() => (
        <TabBar onTabSelect={() => {}} onTabClose={handleClose} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
      ));

      const closeBtn = container.querySelector(".tabClose")!;
      fireEvent.click(closeBtn);

      expect(handleClose).toHaveBeenCalledWith(id1);
    });
  });

  describe("quick switcher badges", () => {
    it("shows shortcut badges when quickSwitcherActive", () => {
      addTerminal({ name: "Tab 1" });
      addTerminal({ name: "Tab 2" });

      const { container } = render(() => (
        <TabBar quickSwitcherActive={true} onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
      ));
      const badges = container.querySelectorAll(".shortcutBadge");
      expect(badges.length).toBe(2);
    });
  });

  describe("middle-click to close", () => {
    it("middle-click on terminal tab calls onTabClose", () => {
      const handleClose = vi.fn();
      const handleSelect = vi.fn();
      const id1 = addTerminal({ name: "Tab 1" });

      const { container } = render(() => (
        <TabBar onTabSelect={handleSelect} onTabClose={handleClose} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
      ));
      const tab = container.querySelector(".tab")!;
      fireEvent(tab, new MouseEvent("auxclick", { button: 1, bubbles: true }));
      expect(handleClose).toHaveBeenCalledWith(id1);
      expect(handleSelect).not.toHaveBeenCalled();
    });

    it("right-click auxclick does not close tab", () => {
      const handleClose = vi.fn();
      addTerminal({ name: "Tab 1" });

      const { container } = render(() => (
        <TabBar onTabSelect={() => {}} onTabClose={handleClose} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} />
      ));
      const tab = container.querySelector(".tab")!;
      fireEvent(tab, new MouseEvent("auxclick", { button: 2, bubbles: true }));
      expect(handleClose).not.toHaveBeenCalled();
    });
  });

  describe("move to worktree context menu", () => {
    function setupRepoWithWorktrees() {
      // Create a repo with main branch and a worktree branch
      repositoriesStore.add({ path: "/repo", displayName: "repo" });
      repositoriesStore.setBranch("/repo", "main", { isMain: true, worktreePath: null });
      repositoriesStore.setBranch("/repo", "feature-a", { isMain: false, worktreePath: "/repo-wt/feature-a" });
      repositoriesStore.setActive("/repo");
      repositoriesStore.setActiveBranch("/repo", "main");
    }

    it("shows Move to Worktree submenu when repo has multiple worktrees", () => {
      setupRepoWithWorktrees();
      const termId = addTerminal({ name: "T1", sessionId: "sess-1" });
      repositoriesStore.addTerminalToBranch("/repo", "main", termId);
      terminalsStore.setActive(termId);

      const getTargets = () => [{ branchName: "feature-a", path: "/repo-wt/feature-a" }];
      const { container } = render(() => (
        <TabBar
          onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}}
          getWorktreeTargets={getTargets}
        />
      ));
      const tab = container.querySelector(".tab")!;
      fireEvent.contextMenu(tab);

      const menuItems = container.querySelectorAll(".menu .item");
      const moveItem = Array.from(menuItems).find(i => i.textContent?.includes("Move to Worktree"));
      expect(moveItem).not.toBeNull();
    });

    it("hides Move to Worktree when no worktree targets available", () => {
      repositoriesStore.add({ path: "/repo", displayName: "repo" });
      repositoriesStore.setBranch("/repo", "main", { isMain: true, worktreePath: null });
      repositoriesStore.setActive("/repo");
      repositoriesStore.setActiveBranch("/repo", "main");
      const termId = addTerminal({ name: "T1", sessionId: "sess-1" });
      repositoriesStore.addTerminalToBranch("/repo", "main", termId);
      terminalsStore.setActive(termId);

      const getTargets = () => [] as Array<{ branchName: string; path: string }>;
      const { container } = render(() => (
        <TabBar
          onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}}
          getWorktreeTargets={getTargets}
        />
      ));
      const tab = container.querySelector(".tab")!;
      fireEvent.contextMenu(tab);

      const menuItems = container.querySelectorAll(".menu .item");
      const moveItem = Array.from(menuItems).find(i => i.textContent?.includes("Move to Worktree"));
      expect(moveItem).toBeUndefined();
    });

    it("calls onMoveToWorktree with correct args when worktree is selected", () => {
      setupRepoWithWorktrees();
      const handleMove = vi.fn();
      const termId = addTerminal({ name: "T1", sessionId: "sess-1" });
      repositoriesStore.addTerminalToBranch("/repo", "main", termId);
      terminalsStore.setActive(termId);

      const getTargets = () => [{ branchName: "feature-a", path: "/repo-wt/feature-a" }];
      const { container } = render(() => (
        <TabBar
          onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}}
          getWorktreeTargets={getTargets}
          onMoveToWorktree={handleMove}
        />
      ));
      const tab = container.querySelector(".tab")!;
      fireEvent.contextMenu(tab);

      // Find the "Move to Worktree" parent item and hover to open submenu
      const menuItems = container.querySelectorAll(".menu .itemWrap");
      const moveWrap = Array.from(menuItems).find(i => i.textContent?.includes("Move to Worktree"));
      expect(moveWrap).not.toBeNull();
      fireEvent.mouseEnter(moveWrap!);

      // Find the submenu item "feature-a"
      const submenuItems = moveWrap!.querySelectorAll(".submenu .item");
      const featureItem = Array.from(submenuItems).find(i => i.textContent?.includes("feature-a"));
      expect(featureItem).not.toBeNull();
      fireEvent.click(featureItem!);

      expect(handleMove).toHaveBeenCalledWith(termId, "/repo-wt/feature-a");
    });
  });
});
