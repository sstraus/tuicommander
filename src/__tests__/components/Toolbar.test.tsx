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

describe("Toolbar", () => {
  beforeEach(() => {
    localStorage.clear();
    // Clean up repos
    for (const path of repositoriesStore.getPaths()) {
      repositoriesStore.remove(path);
    }
    repositoriesStore.setActive(null);
    // Reset sidebar to visible
    uiStore.setSidebarVisible(true);
    // Clear plan file and notifications
    uiStore.clearPlanFile();
    prNotificationsStore.clearAll();
  });

  /** Helper to add a test PR notification */
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

  it("renders toolbar element", () => {
    const { container } = render(() => <Toolbar />);
    expect(container.querySelector("#toolbar")).not.toBeNull();
  });

  it("renders sidebar toggle button", () => {
    const { container } = render(() => <Toolbar />);
    const toggle = container.querySelector(".toolbar-sidebar-toggle");
    expect(toggle).not.toBeNull();
  });

  it("renders toolbar-left, toolbar-center, and toolbar-right sections", () => {
    const { container } = render(() => <Toolbar />);
    expect(container.querySelector(".toolbar-left")).not.toBeNull();
    expect(container.querySelector(".toolbar-center")).not.toBeNull();
    expect(container.querySelector(".toolbar-right")).not.toBeNull();
  });

  it("has data-tauri-drag-region attribute", () => {
    const { container } = render(() => <Toolbar />);
    const toolbar = container.querySelector("#toolbar");
    expect(toolbar!.getAttribute("data-tauri-drag-region")).not.toBeNull();
  });

  it("sidebar toggle button click calls uiStore.toggleSidebar", () => {
    const spy = vi.spyOn(uiStore, "toggleSidebar");
    const { container } = render(() => <Toolbar />);
    const toggle = container.querySelector(".toolbar-sidebar-toggle")!;
    fireEvent.click(toggle);
    expect(spy).toHaveBeenCalledOnce();
    spy.mockRestore();
  });

  it("sidebar toggle title reflects sidebar visible state", () => {
    uiStore.setSidebarVisible(true);
    const { container } = render(() => <Toolbar />);
    const toggle = container.querySelector(".toolbar-sidebar-toggle")!;
    expect(toggle.getAttribute("title")).toBe(`Hide Sidebar (${getModifierSymbol()}[)`);
  });

  it("sidebar toggle title when sidebar hidden", () => {
    uiStore.setSidebarVisible(false);
    const { container } = render(() => <Toolbar />);
    const toggle = container.querySelector(".toolbar-sidebar-toggle")!;
    expect(toggle.getAttribute("title")).toBe(`Show Sidebar (${getModifierSymbol()}[)`);
  });

  it("shows branch name when active repo has active branch", () => {
    const repoPath = "/test/repo";
    repositoriesStore.add({ path: repoPath, displayName: "Test Repo" });
    repositoriesStore.setActive(repoPath);
    repositoriesStore.setBranch(repoPath, "feature-x", { name: "feature-x" });
    repositoriesStore.setActiveBranch(repoPath, "feature-x");

    const { container } = render(() => <Toolbar />);
    const branchBtn = container.querySelector(".toolbar-branch");
    expect(branchBtn).not.toBeNull();
    const branchName = container.querySelector(".toolbar-branch-name");
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
    const branchBtn = container.querySelector(".toolbar-branch")!;
    fireEvent.click(branchBtn);
    expect(handleBranchClick).toHaveBeenCalledOnce();
  });

  it("no branch button when no active branch", () => {
    const { container } = render(() => <Toolbar />);
    expect(container.querySelector(".toolbar-branch")).toBeNull();
  });

  it("no branch button when activeRepoPath but no activeBranch", () => {
    const repoPath = "/test/repo";
    repositoriesStore.add({ path: repoPath, displayName: "Test Repo" });
    repositoriesStore.setActive(repoPath);
    // No active branch

    const { container } = render(() => <Toolbar />);
    expect(container.querySelector(".toolbar-branch")).toBeNull();
  });

  it("no branch button when activeRepoPath and activeBranch not in branches record", () => {
    const repoPath = "/test/repo";
    repositoriesStore.add({ path: repoPath, displayName: "Test Repo" });
    repositoriesStore.setActive(repoPath);
    // Set activeBranch to a name that doesn't exist in branches
    repositoriesStore.setActiveBranch(repoPath, "nonexistent");

    const { container } = render(() => <Toolbar />);
    expect(container.querySelector(".toolbar-branch")).toBeNull();
  });

  it("branch button has title 'Rename branch'", () => {
    const repoPath = "/test/repo";
    repositoriesStore.add({ path: repoPath, displayName: "Test Repo" });
    repositoriesStore.setActive(repoPath);
    repositoriesStore.setBranch(repoPath, "main", { name: "main" });
    repositoriesStore.setActiveBranch(repoPath, "main");

    const { container } = render(() => <Toolbar />);
    const branchBtn = container.querySelector(".toolbar-branch")!;
    expect(branchBtn.getAttribute("title")).toBe("Rename branch");
  });

  it("branch button contains branch icon", () => {
    const repoPath = "/test/repo";
    repositoriesStore.add({ path: repoPath, displayName: "Test Repo" });
    repositoriesStore.setActive(repoPath);
    repositoriesStore.setBranch(repoPath, "dev", { name: "dev" });
    repositoriesStore.setActiveBranch(repoPath, "dev");

    const { container } = render(() => <Toolbar />);
    const icon = container.querySelector(".toolbar-branch-icon");
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

  describe("PR notification bell", () => {
    it("hides bell when no active notifications", () => {
      const { container } = render(() => <Toolbar />);
      expect(container.querySelector(".pr-notif-bell")).toBeNull();
    });

    it("shows bell when there are active notifications", () => {
      addTestNotif();
      const { container } = render(() => <Toolbar />);
      expect(container.querySelector(".pr-notif-bell")).not.toBeNull();
    });

    it("bell shows notification count", () => {
      addTestNotif();
      addTestNotif({ branch: "other", prNumber: 43 });
      const { container } = render(() => <Toolbar />);
      const count = container.querySelector(".pr-notif-count");
      expect(count?.textContent).toBe("2");
    });

    it("clicking bell toggles popover open", () => {
      addTestNotif();
      const { container } = render(() => <Toolbar />);
      const bell = container.querySelector(".pr-notif-bell")!;
      fireEvent.click(bell);
      expect(container.querySelector(".pr-notif-popover")).not.toBeNull();
    });

    it("clicking bell again closes popover", () => {
      addTestNotif();
      const { container } = render(() => <Toolbar />);
      const bell = container.querySelector(".pr-notif-bell")!;
      fireEvent.click(bell);
      fireEvent.click(bell);
      expect(container.querySelector(".pr-notif-popover")).toBeNull();
    });

    it("popover shows notification items", () => {
      addTestNotif({ type: "ci_failed", prNumber: 99 });
      const { container } = render(() => <Toolbar />);
      fireEvent.click(container.querySelector(".pr-notif-bell")!);
      const items = container.querySelectorAll(".pr-notif-item");
      expect(items.length).toBe(1);
      expect(items[0].textContent).toContain("#99");
    });

    it("dismiss all button dismisses all notifications and closes popover", () => {
      addTestNotif({ branch: "a" });
      addTestNotif({ branch: "b", prNumber: 43 });
      const { container } = render(() => <Toolbar />);
      fireEvent.click(container.querySelector(".pr-notif-bell")!);
      fireEvent.click(container.querySelector(".pr-notif-dismiss-all")!);
      expect(container.querySelector(".pr-notif-bell")).toBeNull();
      expect(container.querySelector(".pr-notif-popover")).toBeNull();
    });

    it("individual dismiss button removes notification", () => {
      addTestNotif();
      const { container } = render(() => <Toolbar />);
      fireEvent.click(container.querySelector(".pr-notif-bell")!);
      const closeBtn = container.querySelector(".pr-notif-close")!;
      fireEvent.click(closeBtn);
      expect(prNotificationsStore.getActive().length).toBe(0);
    });

    it("clicking notification item opens PrDetailPopover", async () => {
      addTestNotif({ repoPath: "/my/repo", branch: "feature", prNumber: 42 });
      const { container } = render(() => <Toolbar />);
      fireEvent.click(container.querySelector(".pr-notif-bell")!);
      const item = container.querySelector(".pr-notif-item")!;
      fireEvent.click(item);

      // requestAnimationFrame fires in jsdom after 0ms
      await waitFor(() => {
        expect(container.querySelector(".pr-detail-popover")).not.toBeNull();
      });
    });
  });

  describe("plan file button", () => {
    it("hides plan button when planFilePath is null", () => {
      const { container } = render(() => <Toolbar />);
      expect(container.querySelector(".plan-button")).toBeNull();
    });

    it("shows plan button when planFilePath is set", () => {
      uiStore.setPlanFilePath("/repo/plans/my-plan.md");
      const { container } = render(() => <Toolbar />);
      expect(container.querySelector(".plan-button")).not.toBeNull();
    });

    it("plan button shows display name without path and extension", () => {
      uiStore.setPlanFilePath("/repo/plans/my-plan.md");
      const { container } = render(() => <Toolbar />);
      const name = container.querySelector(".plan-button-name");
      expect(name?.textContent).toBe("my-plan");
    });

    it("plan button strips .mdx extension", () => {
      uiStore.setPlanFilePath("/repo/plans/feature.mdx");
      const { container } = render(() => <Toolbar />);
      const name = container.querySelector(".plan-button-name");
      expect(name?.textContent).toBe("feature");
    });

    it("clicking plan button calls onOpenPlan with the path", () => {
      const onOpenPlan = vi.fn();
      uiStore.setPlanFilePath("/repo/plans/my-plan.md");
      const { container } = render(() => <Toolbar onOpenPlan={onOpenPlan} />);
      fireEvent.click(container.querySelector(".plan-button")!);
      expect(onOpenPlan).toHaveBeenCalledWith("/repo/plans/my-plan.md");
    });

    it("clicking plan close button calls uiStore.clearPlanFile", () => {
      const spy = vi.spyOn(uiStore, "clearPlanFile");
      uiStore.setPlanFilePath("/repo/plan.md");
      const { container } = render(() => <Toolbar />);
      fireEvent.click(container.querySelector(".plan-button-close")!);
      expect(spy).toHaveBeenCalledOnce();
      spy.mockRestore();
    });
  });
});
