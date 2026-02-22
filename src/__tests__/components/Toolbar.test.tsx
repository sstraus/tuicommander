import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent, waitFor } from "@solidjs/testing-library";
import { getModifierSymbol } from "../../platform";

// Mock IdeLauncher to avoid Tauri invoke calls from that component
vi.mock("../../components/IdeLauncher", () => ({
  IdeLauncher: (props: any) => <div data-testid="ide-launcher" data-repo-path={props.repoPath || ""} />,
}));

// Mock PrDetailPopover to avoid needing GitHub store setup
vi.mock("../../components/PrDetailPopover/PrDetailPopover", () => ({
  PrDetailPopover: (props: any) => <div class="pr-detail-popover" data-repo={props.repoPath} data-branch={props.branch} />,
}));

// Mock Tauri core (for any transitive imports)
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

import { Toolbar } from "../../components/Toolbar/Toolbar";
import { repositoriesStore } from "../../stores/repositories";
import { uiStore } from "../../stores/ui";
import { prNotificationsStore } from "../../stores/prNotifications";
import { activityStore } from "../../stores/activityStore";

describe("Toolbar", () => {
  beforeEach(() => {
    localStorage.clear();
    for (const path of repositoriesStore.getPaths()) {
      repositoriesStore.remove(path);
    }
    repositoriesStore.setActive(null);
    uiStore.setSidebarVisible(true);
    prNotificationsStore.clearAll();
    activityStore.clearAll();
  });

  function addTestNotif(overrides: Partial<Parameters<typeof prNotificationsStore.add>[0]> = {}) {
    prNotificationsStore.add({
      repoPath: "/repo",
      branch: "feature",
      prNumber: 42,
      title: "Test PR",
      type: "ci_failed",
      ...overrides,
    });
  }

  function addTestActivityItem(overrides: Partial<{ id: string; title: string; subtitle: string; sectionId: string; contentUri: string }> = {}) {
    activityStore.addItem({
      id: overrides.id ?? "act-1",
      pluginId: "test",
      sectionId: overrides.sectionId ?? "plan",
      title: overrides.title ?? "Test Item",
      icon: "<svg/>",
      dismissible: true,
      subtitle: overrides.subtitle,
      contentUri: overrides.contentUri,
    });
  }

  it("renders toolbar element", () => {
    const { container } = render(() => <Toolbar />);
    expect(container.querySelector(".toolbar")).not.toBeNull();
  });

  it("renders sidebar toggle button", () => {
    const { container } = render(() => <Toolbar />);
    const toggle = container.querySelector(".sidebarToggle");
    expect(toggle).not.toBeNull();
  });

  it("renders left, center, and right sections", () => {
    const { container } = render(() => <Toolbar />);
    expect(container.querySelector(".left")).not.toBeNull();
    expect(container.querySelector(".center")).not.toBeNull();
    expect(container.querySelector(".right")).not.toBeNull();
  });

  it("has data-tauri-drag-region attribute", () => {
    const { container } = render(() => <Toolbar />);
    const toolbar = container.querySelector(".toolbar");
    expect(toolbar!.getAttribute("data-tauri-drag-region")).not.toBeNull();
  });

  it("sidebar toggle button click calls uiStore.toggleSidebar", () => {
    const spy = vi.spyOn(uiStore, "toggleSidebar");
    const { container } = render(() => <Toolbar />);
    const toggle = container.querySelector(".sidebarToggle")!;
    fireEvent.click(toggle);
    expect(spy).toHaveBeenCalledOnce();
    spy.mockRestore();
  });

  it("sidebar toggle title reflects sidebar visible state", () => {
    uiStore.setSidebarVisible(true);
    const { container } = render(() => <Toolbar />);
    const toggle = container.querySelector(".sidebarToggle")!;
    expect(toggle.getAttribute("title")).toBe(`Hide Sidebar (${getModifierSymbol()}[)`);
  });

  it("sidebar toggle title when sidebar hidden", () => {
    uiStore.setSidebarVisible(false);
    const { container } = render(() => <Toolbar />);
    const toggle = container.querySelector(".sidebarToggle")!;
    expect(toggle.getAttribute("title")).toBe(`Show Sidebar (${getModifierSymbol()}[)`);
  });

  it("shows branch name when active repo has active branch", () => {
    const repoPath = "/test/repo";
    repositoriesStore.add({ path: repoPath, displayName: "Test Repo" });
    repositoriesStore.setActive(repoPath);
    repositoriesStore.setBranch(repoPath, "feature-x", { name: "feature-x" });
    repositoriesStore.setActiveBranch(repoPath, "feature-x");

    const { container } = render(() => <Toolbar />);
    const branchBtn = container.querySelector(".branch");
    expect(branchBtn).not.toBeNull();
    const branchName = container.querySelector(".branchName");
    expect(branchName!.textContent).toBe("feature-x");
  });

  it("branch button calls onBranchClick when clicked", () => {
    const handleBranchClick = vi.fn();
    const repoPath = "/test/repo";
    repositoriesStore.add({ path: repoPath, displayName: "Test Repo" });
    repositoriesStore.setActive(repoPath);
    repositoriesStore.setBranch(repoPath, "main", { name: "main" });
    repositoriesStore.setActiveBranch(repoPath, "main");

    const { container } = render(() => <Toolbar onBranchClick={handleBranchClick} />);
    const branchBtn = container.querySelector(".branch")!;
    fireEvent.click(branchBtn);
    expect(handleBranchClick).toHaveBeenCalledOnce();
  });

  it("no branch button when no active branch", () => {
    const { container } = render(() => <Toolbar />);
    expect(container.querySelector(".branch")).toBeNull();
  });

  it("no branch button when activeRepoPath but no activeBranch", () => {
    const repoPath = "/test/repo";
    repositoriesStore.add({ path: repoPath, displayName: "Test Repo" });
    repositoriesStore.setActive(repoPath);

    const { container } = render(() => <Toolbar />);
    expect(container.querySelector(".branch")).toBeNull();
  });

  it("no branch button when activeRepoPath and activeBranch not in branches record", () => {
    const repoPath = "/test/repo";
    repositoriesStore.add({ path: repoPath, displayName: "Test Repo" });
    repositoriesStore.setActive(repoPath);
    repositoriesStore.setActiveBranch(repoPath, "nonexistent");

    const { container } = render(() => <Toolbar />);
    expect(container.querySelector(".branch")).toBeNull();
  });

  it("branch button has title 'Rename branch'", () => {
    const repoPath = "/test/repo";
    repositoriesStore.add({ path: repoPath, displayName: "Test Repo" });
    repositoriesStore.setActive(repoPath);
    repositoriesStore.setBranch(repoPath, "main", { name: "main" });
    repositoriesStore.setActiveBranch(repoPath, "main");

    const { container } = render(() => <Toolbar />);
    const branchBtn = container.querySelector(".branch")!;
    expect(branchBtn.getAttribute("title")).toBe("Rename branch");
  });

  it("branch button contains branch icon", () => {
    const repoPath = "/test/repo";
    repositoriesStore.add({ path: repoPath, displayName: "Test Repo" });
    repositoriesStore.setActive(repoPath);
    repositoriesStore.setBranch(repoPath, "dev", { name: "dev" });
    repositoriesStore.setActiveBranch(repoPath, "dev");

    const { container } = render(() => <Toolbar />);
    const icon = container.querySelector(".branchIcon");
    expect(icon).not.toBeNull();
    expect(icon!.tagName.toLowerCase()).toBe("svg");
  });

  it("IdeLauncher receives worktreePath from active branch", () => {
    const repoPath = "/test/repo";
    repositoriesStore.add({ path: repoPath, displayName: "Test Repo" });
    repositoriesStore.setActive(repoPath);
    repositoriesStore.setBranch(repoPath, "feature", {
      name: "feature",
      worktreePath: "/test/repo-worktrees/feature",
    });
    repositoriesStore.setActiveBranch(repoPath, "feature");

    const { container } = render(() => <Toolbar repoPath={repoPath} />);
    const launcher = container.querySelector("[data-testid='ide-launcher']")!;
    expect(launcher.getAttribute("data-repo-path")).toBe("/test/repo-worktrees/feature");
  });

  it("IdeLauncher falls back to repoPath when no worktreePath", () => {
    const repoPath = "/test/repo";
    repositoriesStore.add({ path: repoPath, displayName: "Test Repo" });
    repositoriesStore.setActive(repoPath);
    repositoriesStore.setBranch(repoPath, "main", {
      name: "main",
      worktreePath: null,
    });
    repositoriesStore.setActiveBranch(repoPath, "main");

    const { container } = render(() => <Toolbar repoPath={repoPath} />);
    const launcher = container.querySelector("[data-testid='ide-launcher']")!;
    expect(launcher.getAttribute("data-repo-path")).toBe(repoPath);
  });

  it("IdeLauncher receives repoPath when no active branch", () => {
    const repoPath = "/test/repo";

    const { container } = render(() => <Toolbar repoPath={repoPath} />);
    const launcher = container.querySelector("[data-testid='ide-launcher']")!;
    expect(launcher.getAttribute("data-repo-path")).toBe(repoPath);
  });

  // -------------------------------------------------------------------------
  // Plan button is REMOVED (now plan items go through Activity Center)
  // -------------------------------------------------------------------------
  it("plan button is not rendered (plan moved to Activity Center)", () => {
    const { container } = render(() => <Toolbar />);
    expect(container.querySelector(".plan-button")).toBeNull();
    expect(container.querySelector(".plan-button-group")).toBeNull();
  });

  // -------------------------------------------------------------------------
  // Bell (Activity Center) — PR notifications
  // -------------------------------------------------------------------------
  describe("PR notification bell", () => {
    it("hides bell when no active notifications or activity items", () => {
      const { container } = render(() => <Toolbar />);
      expect(container.querySelector(".bell")).toBeNull();
    });

    it("shows bell when there are active PR notifications", () => {
      addTestNotif();
      const { container } = render(() => <Toolbar />);
      expect(container.querySelector(".bell")).not.toBeNull();
    });

    it("shows bell when there are active activity items", () => {
      activityStore.registerSection({ id: "plan", label: "PLAN", priority: 10, canDismissAll: false });
      addTestActivityItem();
      const { container } = render(() => <Toolbar />);
      expect(container.querySelector(".bell")).not.toBeNull();
    });

    it("bell badge count is sum of PR + activity items", () => {
      activityStore.registerSection({ id: "plan", label: "PLAN", priority: 10, canDismissAll: false });
      addTestNotif();
      addTestActivityItem({ id: "a1" });
      addTestActivityItem({ id: "a2" });
      const { container } = render(() => <Toolbar />);
      const count = container.querySelector(".notifCount");
      expect(count?.textContent).toBe("3"); // 1 PR + 2 activity
    });

    it("clicking bell toggles popover open", () => {
      addTestNotif();
      const { container } = render(() => <Toolbar />);
      const bell = container.querySelector(".bell")!;
      fireEvent.click(bell);
      expect(container.querySelector(".popover")).not.toBeNull();
    });

    it("clicking bell again closes popover", () => {
      addTestNotif();
      const { container } = render(() => <Toolbar />);
      const bell = container.querySelector(".bell")!;
      fireEvent.click(bell);
      fireEvent.click(bell);
      expect(container.querySelector(".popover")).toBeNull();
    });

    it("popover shows PR notification items", () => {
      addTestNotif({ type: "ci_failed", prNumber: 99 });
      const { container } = render(() => <Toolbar />);
      fireEvent.click(container.querySelector(".bell")!);
      const items = container.querySelectorAll(".notifItem");
      expect(items.length).toBe(1);
      expect(items[0].textContent).toContain("#99");
    });

    it("dismiss all button dismisses all PR notifications and closes popover", () => {
      addTestNotif({ branch: "a" });
      addTestNotif({ branch: "b", prNumber: 43 });
      const { container } = render(() => <Toolbar />);
      fireEvent.click(container.querySelector(".bell")!);
      fireEvent.click(container.querySelector(".dismissAll")!);
      expect(prNotificationsStore.getActive().length).toBe(0);
      expect(container.querySelector(".popover")).toBeNull();
    });

    it("individual PR dismiss button removes notification", () => {
      addTestNotif();
      const { container } = render(() => <Toolbar />);
      fireEvent.click(container.querySelector(".bell")!);
      const closeBtn = container.querySelector(".notifClose")!;
      fireEvent.click(closeBtn);
      expect(prNotificationsStore.getActive().length).toBe(0);
    });

    it("clicking PR notification item opens PrDetailPopover", async () => {
      addTestNotif({ repoPath: "/my/repo", branch: "feature", prNumber: 42 });
      const { container } = render(() => <Toolbar />);
      fireEvent.click(container.querySelector(".bell")!);
      const item = container.querySelector(".notifItem")!;
      fireEvent.click(item);

      await waitFor(() => {
        expect(container.querySelector(".pr-detail-popover")).not.toBeNull();
      });
    });
  });

  // -------------------------------------------------------------------------
  // Bell — Activity Center plugin sections
  // -------------------------------------------------------------------------
  describe("activity sections in bell popover", () => {
    beforeEach(() => {
      activityStore.registerSection({ id: "plan", label: "ACTIVE PLAN", priority: 10, canDismissAll: false });
    });

    it("shows activity section header with label", () => {
      addTestActivityItem({ title: "My Plan" });
      const { container } = render(() => <Toolbar />);
      fireEvent.click(container.querySelector(".bell")!);
      const header = container.querySelector(".sectionLabel");
      expect(header?.textContent).toBe("ACTIVE PLAN");
    });

    it("shows activity item title", () => {
      addTestActivityItem({ title: "My Plan" });
      const { container } = render(() => <Toolbar />);
      fireEvent.click(container.querySelector(".bell")!);
      const title = container.querySelector(".activityItemTitle");
      expect(title?.textContent).toBe("My Plan");
    });

    it("shows activity item subtitle when present", () => {
      addTestActivityItem({ title: "My Plan", subtitle: "/repo/plans/foo.md" });
      const { container } = render(() => <Toolbar />);
      fireEvent.click(container.querySelector(".bell")!);
      const subtitle = container.querySelector(".activityItemSubtitle");
      expect(subtitle?.textContent).toBe("/repo/plans/foo.md");
    });

    it("clicking dismissible activity item shows dismiss button", () => {
      addTestActivityItem();
      const { container } = render(() => <Toolbar />);
      fireEvent.click(container.querySelector(".bell")!);
      expect(container.querySelector(".activityItemDismiss")).not.toBeNull();
    });

    it("clicking activity item dismiss removes it from store", () => {
      addTestActivityItem({ id: "act-x" });
      const { container } = render(() => <Toolbar />);
      fireEvent.click(container.querySelector(".bell")!);
      fireEvent.click(container.querySelector(".activityItemDismiss")!);
      expect(activityStore.getActive().find((i) => i.id === "act-x")).toBeUndefined();
    });
  });

  // -------------------------------------------------------------------------
  // Last-item shortcut button
  // -------------------------------------------------------------------------
  describe("last-item shortcut button", () => {
    it("hidden when no items exist", () => {
      const { container } = render(() => <Toolbar />);
      expect(container.querySelector(".lastItemBtn")).toBeNull();
    });

    it("visible when an activity item exists", () => {
      activityStore.registerSection({ id: "plan", label: "PLAN", priority: 10, canDismissAll: false });
      addTestActivityItem({ title: "My Plan" });
      const { container } = render(() => <Toolbar />);
      expect(container.querySelector(".lastItemBtn")).not.toBeNull();
    });

    it("visible when a PR notification exists", () => {
      addTestNotif({ branch: "my-feature" });
      const { container } = render(() => <Toolbar />);
      expect(container.querySelector(".lastItemBtn")).not.toBeNull();
    });

    it("shows activity item title in last-item button", () => {
      activityStore.registerSection({ id: "plan", label: "PLAN", priority: 10, canDismissAll: false });
      addTestActivityItem({ title: "Active Plan" });
      const { container } = render(() => <Toolbar />);
      const btn = container.querySelector(".lastItemBtn");
      expect(btn?.textContent).toContain("Active Plan");
    });

    it("shows PR branch name when PR is the last item", () => {
      addTestNotif({ branch: "hotfix/critical" });
      const { container } = render(() => <Toolbar />);
      const btn = container.querySelector(".lastItemBtn");
      expect(btn?.textContent).toContain("hotfix/critical");
    });
  });
});
