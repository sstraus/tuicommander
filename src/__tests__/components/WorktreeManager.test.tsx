import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@solidjs/testing-library";

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

import { WorktreeManager } from "../../components/WorktreeManager";
import { worktreeManagerStore } from "../../stores/worktreeManager";
import { repositoriesStore } from "../../stores/repositories";

describe("WorktreeManager", () => {
  beforeEach(() => {
    worktreeManagerStore.close();
    // Clear repos — reset to empty
    for (const path of Object.keys(repositoriesStore.state.repositories)) {
      repositoriesStore.remove(path);
    }
  });

  it("renders nothing when store is closed", () => {
    const { container } = render(() => <WorktreeManager />);
    expect(container.innerHTML).toBe("");
  });

  it("renders overlay when store is open", () => {
    worktreeManagerStore.open();
    const { container } = render(() => <WorktreeManager />);
    expect(container.querySelector("[class*='overlay']")).not.toBeNull();
    expect(container.querySelector("[class*='panel']")).not.toBeNull();
  });

  it("shows empty state when no worktrees exist", () => {
    worktreeManagerStore.open();
    const { container } = render(() => <WorktreeManager />);
    expect(container.querySelector("[class*='empty']")?.textContent).toContain("No worktrees found");
  });

  it("lists worktrees from repositories store", () => {
    repositoriesStore.add({ path: "/repo", displayName: "MyRepo" });
    repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
    repositoriesStore.setBranch("/repo", "feature-a", { worktreePath: "/repo/.wt/feature-a" });

    worktreeManagerStore.open();
    const { container } = render(() => <WorktreeManager />);

    const rows = container.querySelectorAll("[class*='row']");
    expect(rows.length).toBe(2);

    // feature-a should be first (non-main sorted before main)
    const branchTexts = Array.from(rows).map((r) =>
      r.querySelector("[class*='branch']")?.textContent,
    );
    expect(branchTexts[0]).toBe("feature-a");
    expect(branchTexts[1]).toBe("main");
  });

  it("shows main badge on main worktree rows", () => {
    repositoriesStore.add({ path: "/repo", displayName: "Repo" });
    repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });

    worktreeManagerStore.open();
    const { container } = render(() => <WorktreeManager />);

    expect(container.querySelector("[class*='mainBadge']")?.textContent).toBe("main");
  });

  it("shows dirty stats for worktrees with changes", () => {
    repositoriesStore.add({ path: "/repo", displayName: "Repo" });
    repositoriesStore.setBranch("/repo", "feature-x", { worktreePath: "/repo/.wt/x", additions: 5, deletions: 3 });

    worktreeManagerStore.open();
    const { container } = render(() => <WorktreeManager />);

    const stats = container.querySelector("[class*='stats']");
    expect(stats?.textContent).toContain("+5");
    expect(stats?.textContent).toContain("-3");
  });

  it("shows 'clean' for worktrees with no changes", () => {
    repositoriesStore.add({ path: "/repo", displayName: "Repo" });
    repositoriesStore.setBranch("/repo", "feature-y", { worktreePath: "/repo/.wt/y", additions: 0, deletions: 0 });

    worktreeManagerStore.open();
    const { container } = render(() => <WorktreeManager />);

    expect(container.querySelector("[class*='statsClean']")?.textContent).toBe("clean");
  });

  it("closes on escape key", () => {
    worktreeManagerStore.open();
    render(() => <WorktreeManager />);

    fireEvent.keyDown(document, { key: "Escape" });
    expect(worktreeManagerStore.state.isOpen).toBe(false);
  });

  it("closes on backdrop click", () => {
    worktreeManagerStore.open();
    const { container } = render(() => <WorktreeManager />);

    const overlay = container.querySelector("[class*='overlay']");
    if (overlay) fireEvent.click(overlay);
    expect(worktreeManagerStore.state.isOpen).toBe(false);
  });

  it("does not close when clicking inside the panel", () => {
    worktreeManagerStore.open();
    const { container } = render(() => <WorktreeManager />);

    const panel = container.querySelector("[class*='panel']");
    if (panel) fireEvent.click(panel);
    expect(worktreeManagerStore.state.isOpen).toBe(true);
  });

  it("shows footer with worktree count", () => {
    repositoriesStore.add({ path: "/repo", displayName: "Repo" });
    repositoriesStore.setBranch("/repo", "main", { worktreePath: "/repo" });
    repositoriesStore.setBranch("/repo", "feat", { worktreePath: "/repo/.wt/feat" });

    worktreeManagerStore.open();
    const { container } = render(() => <WorktreeManager />);

    expect(container.querySelector("[class*='footer']")?.textContent).toContain("2 worktree(s)");
  });

  it("shows repo name badge", () => {
    repositoriesStore.add({ path: "/home/user/my-project", displayName: "my-project" });
    repositoriesStore.setBranch("/home/user/my-project", "feat", { worktreePath: "/home/user/my-project/.wt/feat" });

    worktreeManagerStore.open();
    const { container } = render(() => <WorktreeManager />);

    expect(container.querySelector("[class*='repo']")?.textContent).toBe("my-project");
  });
});
