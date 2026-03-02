import { describe, it, expect, vi, beforeEach } from "vitest";
import { render } from "@solidjs/testing-library";

// Mock Tauri APIs before any component imports
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

import { PlanPanel } from "../../components/PlanPanel/PlanPanel";
import { activityStore } from "../../stores/activityStore";
import { mdTabsStore } from "../../stores/mdTabs";

/** Add a plan activity item to the store */
function addPlanItem(opts: { id: string; title: string; repoPath?: string; contentUri?: string; subtitle?: string }) {
  activityStore.addItem({
    id: opts.id,
    pluginId: "plan",
    sectionId: "plan",
    title: opts.title,
    icon: "<svg/>",
    dismissible: true,
    repoPath: opts.repoPath,
    contentUri: opts.contentUri,
    subtitle: opts.subtitle,
  });
}

beforeEach(() => {
  activityStore.clearAll();
  mdTabsStore.clearAll();
});

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

describe("PlanPanel rendering", () => {
  it("renders panel structure with header and close button", () => {
    const onClose = vi.fn();
    const { container } = render(() => (
      <PlanPanel visible={true} repoPath="/repo" onClose={onClose} />
    ));
    const panel = container.querySelector("#plan-panel");
    expect(panel).not.toBeNull();
    // Has header with title
    expect(container.textContent).toContain("Plans");
    // Has close button
    const closeBtn = container.querySelector("[data-testid='plan-panel-close']");
    expect(closeBtn).not.toBeNull();
  });

  it("hides panel when visible is false", () => {
    const { container } = render(() => (
      <PlanPanel visible={false} repoPath="/repo" onClose={() => {}} />
    ));
    const panel = container.querySelector("#plan-panel");
    expect(panel).not.toBeNull();
    // Panel should have the hidden class
    expect(panel!.classList.toString()).toContain("hidden");
  });

  it("shows empty state when no plans exist", () => {
    const { container } = render(() => (
      <PlanPanel visible={true} repoPath="/repo" onClose={() => {}} />
    ));
    expect(container.textContent).toContain("No plans");
  });

  it("shows empty state when no repo is selected", () => {
    const { container } = render(() => (
      <PlanPanel visible={true} repoPath={null} onClose={() => {}} />
    ));
    expect(container.textContent).toContain("No repository");
  });
});

// ---------------------------------------------------------------------------
// Plan listing
// ---------------------------------------------------------------------------

describe("PlanPanel plan items", () => {
  it("lists plans for the active repo", () => {
    addPlanItem({ id: "plan:/repo/plans/a.md", title: "Plan A", repoPath: "/repo", contentUri: "plan:file?path=%2Frepo%2Fplans%2Fa.md" });
    addPlanItem({ id: "plan:/repo/plans/b.md", title: "Plan B", repoPath: "/repo", contentUri: "plan:file?path=%2Frepo%2Fplans%2Fb.md" });
    addPlanItem({ id: "plan:/other/plans/c.md", title: "Plan C", repoPath: "/other", contentUri: "plan:file?path=%2Fother%2Fplans%2Fc.md" });

    const { container } = render(() => (
      <PlanPanel visible={true} repoPath="/repo" onClose={() => {}} />
    ));

    const items = container.querySelectorAll("[data-testid='plan-item']");
    expect(items.length).toBe(2);
    expect(container.textContent).toContain("Plan A");
    expect(container.textContent).toContain("Plan B");
    expect(container.textContent).not.toContain("Plan C");
  });

  it("shows plan count badge", () => {
    addPlanItem({ id: "plan:/repo/a.md", title: "A", repoPath: "/repo" });
    addPlanItem({ id: "plan:/repo/b.md", title: "B", repoPath: "/repo" });

    const { container } = render(() => (
      <PlanPanel visible={true} repoPath="/repo" onClose={() => {}} />
    ));

    const badge = container.querySelector("[data-testid='plan-count-badge']");
    expect(badge).not.toBeNull();
    expect(badge!.textContent).toBe("2");
  });

  it("shows plan subtitle (file path)", () => {
    addPlanItem({ id: "plan:/repo/plans/x.md", title: "X", repoPath: "/repo", subtitle: "/repo/plans/x.md" });

    const { container } = render(() => (
      <PlanPanel visible={true} repoPath="/repo" onClose={() => {}} />
    ));

    expect(container.textContent).toContain("/repo/plans/x.md");
  });

  it("includes plans without repoPath (backward compat)", () => {
    addPlanItem({ id: "plan:/orphan.md", title: "Orphan" });

    const { container } = render(() => (
      <PlanPanel visible={true} repoPath="/repo" onClose={() => {}} />
    ));

    const items = container.querySelectorAll("[data-testid='plan-item']");
    expect(items.length).toBe(1);
    expect(container.textContent).toContain("Orphan");
  });
});

// ---------------------------------------------------------------------------
// Interaction
// ---------------------------------------------------------------------------

describe("PlanPanel interactions", () => {
  it("calls onClose when close button is clicked", () => {
    const onClose = vi.fn();
    const { container } = render(() => (
      <PlanPanel visible={true} repoPath="/repo" onClose={onClose} />
    ));

    const closeBtn = container.querySelector("[data-testid='plan-panel-close']") as HTMLElement;
    closeBtn.click();
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("opens virtual tab when plan item is clicked", () => {
    addPlanItem({
      id: "plan:/repo/plans/my-plan.md",
      title: "my-plan",
      repoPath: "/repo",
      contentUri: "plan:file?path=%2Frepo%2Fplans%2Fmy-plan.md",
    });

    const addVirtualSpy = vi.spyOn(mdTabsStore, "addVirtual");

    const { container } = render(() => (
      <PlanPanel visible={true} repoPath="/repo" onClose={() => {}} />
    ));

    const item = container.querySelector("[data-testid='plan-item']") as HTMLElement;
    item.click();

    expect(addVirtualSpy).toHaveBeenCalledWith("my-plan", "plan:file?path=%2Frepo%2Fplans%2Fmy-plan.md");
    addVirtualSpy.mockRestore();
  });
});
