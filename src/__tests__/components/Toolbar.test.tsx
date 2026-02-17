import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@solidjs/testing-library";
import { getModifierSymbol } from "../../platform";

// Mock IdeLauncher to avoid Tauri invoke calls from that component
vi.mock("../../components/IdeLauncher", () => ({
  IdeLauncher: (props: any) => <div data-testid="ide-launcher" data-repo-path={props.repoPath || ""} />,
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
  });

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
    expect(icon!.textContent).toBe("Y");
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
});
