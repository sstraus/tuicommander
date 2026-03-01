import { describe, it, expect, beforeEach } from "vitest";
import "../mocks/tauri";
import { worktreeManagerStore } from "../../stores/worktreeManager";

describe("worktreeManagerStore", () => {
  beforeEach(() => {
    worktreeManagerStore.close();
    worktreeManagerStore.clearSelection();
    worktreeManagerStore.setRepoFilter(null);
    worktreeManagerStore.setTextFilter("");
  });

  // --- open/close/toggle ---

  it("starts closed", () => {
    expect(worktreeManagerStore.state.isOpen).toBe(false);
  });

  it("open() sets isOpen to true", () => {
    worktreeManagerStore.open();
    expect(worktreeManagerStore.state.isOpen).toBe(true);
  });

  it("close() sets isOpen to false", () => {
    worktreeManagerStore.open();
    worktreeManagerStore.close();
    expect(worktreeManagerStore.state.isOpen).toBe(false);
  });

  it("toggle() opens when closed and closes when open", () => {
    worktreeManagerStore.toggle();
    expect(worktreeManagerStore.state.isOpen).toBe(true);
    worktreeManagerStore.toggle();
    expect(worktreeManagerStore.state.isOpen).toBe(false);
  });

  it("close() resets filters and selection", () => {
    worktreeManagerStore.open();
    worktreeManagerStore.toggleSelect("/repo::feature-a");
    worktreeManagerStore.setRepoFilter("/repo");
    worktreeManagerStore.setTextFilter("feat");
    worktreeManagerStore.close();
    expect(worktreeManagerStore.state.selectedIds.size).toBe(0);
    expect(worktreeManagerStore.state.repoFilter).toBeNull();
    expect(worktreeManagerStore.state.textFilter).toBe("");
  });

  // --- selection ---

  it("toggleSelect adds and removes ids", () => {
    worktreeManagerStore.toggleSelect("/repo::branch-a");
    expect(worktreeManagerStore.state.selectedIds.has("/repo::branch-a")).toBe(true);
    worktreeManagerStore.toggleSelect("/repo::branch-a");
    expect(worktreeManagerStore.state.selectedIds.has("/repo::branch-a")).toBe(false);
  });

  it("selectAll sets multiple ids", () => {
    worktreeManagerStore.selectAll(["/repo::a", "/repo::b", "/repo::c"]);
    expect(worktreeManagerStore.state.selectedIds.size).toBe(3);
    expect(worktreeManagerStore.state.selectedIds.has("/repo::b")).toBe(true);
  });

  it("clearSelection empties the set", () => {
    worktreeManagerStore.selectAll(["/repo::a", "/repo::b"]);
    worktreeManagerStore.clearSelection();
    expect(worktreeManagerStore.state.selectedIds.size).toBe(0);
  });

  // --- filters ---

  it("setRepoFilter updates repoFilter", () => {
    worktreeManagerStore.setRepoFilter("/my/repo");
    expect(worktreeManagerStore.state.repoFilter).toBe("/my/repo");
  });

  it("setRepoFilter(null) clears filter", () => {
    worktreeManagerStore.setRepoFilter("/my/repo");
    worktreeManagerStore.setRepoFilter(null);
    expect(worktreeManagerStore.state.repoFilter).toBeNull();
  });

  it("setTextFilter updates textFilter", () => {
    worktreeManagerStore.setTextFilter("feature");
    expect(worktreeManagerStore.state.textFilter).toBe("feature");
  });
});
