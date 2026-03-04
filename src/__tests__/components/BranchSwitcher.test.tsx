import { describe, it, expect, vi, beforeEach } from "vitest";
import "../mocks/tauri";
import { mockInvoke } from "../mocks/tauri";
import { render, fireEvent, waitFor } from "@solidjs/testing-library";
import { BranchSwitcher } from "../../components/BranchSwitcher/BranchSwitcher";
import { branchSwitcherStore } from "../../stores/branchSwitcher";

const MOCK_BRANCHES = [
  { name: "main", is_current: false, is_remote: false, is_main: true },
  { name: "feat/login", is_current: true, is_remote: false, is_main: false },
  { name: "fix/bug-42", is_current: false, is_remote: false, is_main: false },
  { name: "origin/main", is_current: false, is_remote: true, is_main: true },
  { name: "origin/feat/remote-only", is_current: false, is_remote: true, is_main: false },
];

function defaultProps(overrides: Partial<Parameters<typeof BranchSwitcher>[0]> = {}) {
  return {
    activeRepoPath: "/repo",
    onSelect: vi.fn(),
    onCheckoutRemote: vi.fn(),
    ...overrides,
  };
}

describe("BranchSwitcher", () => {
  beforeEach(() => {
    branchSwitcherStore.close();
    mockInvoke.mockReset();
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_git_branches") return Promise.resolve(MOCK_BRANCHES);
      return Promise.resolve(undefined);
    });
  });

  it("does not render when closed", () => {
    const { container } = render(() => <BranchSwitcher {...defaultProps()} />);
    expect(container.querySelector("input")).toBeNull();
  });

  it("renders branch list when opened", async () => {
    branchSwitcherStore.open();
    const { container } = render(() => <BranchSwitcher {...defaultProps()} />);

    await waitFor(() => {
      expect(container.querySelectorAll("[data-testid='branch-item']").length).toBeGreaterThan(0);
    });

    const items = container.querySelectorAll("[data-testid='branch-item']");
    expect(items.length).toBe(5);
  });

  it("shows current branch badge", async () => {
    branchSwitcherStore.open();
    const { container } = render(() => <BranchSwitcher {...defaultProps()} />);

    await waitFor(() => {
      expect(container.querySelector("[data-testid='badge-current']")).toBeTruthy();
    });
  });

  it("shows remote badge for remote branches", async () => {
    branchSwitcherStore.open();
    const { container } = render(() => <BranchSwitcher {...defaultProps()} />);

    await waitFor(() => {
      const remoteBadges = container.querySelectorAll("[data-testid='badge-remote']");
      expect(remoteBadges.length).toBe(2);
    });
  });

  it("filters branches by query", async () => {
    branchSwitcherStore.open();
    const { container } = render(() => <BranchSwitcher {...defaultProps()} />);

    await waitFor(() => {
      expect(container.querySelectorAll("[data-testid='branch-item']").length).toBe(5);
    });

    const input = container.querySelector("input")!;
    fireEvent.input(input, { target: { value: "feat" } });

    await waitFor(() => {
      const items = container.querySelectorAll("[data-testid='branch-item']");
      expect(items.length).toBe(2); // feat/login + origin/feat/remote-only
    });
  });

  it("shows empty state when no branches match", async () => {
    branchSwitcherStore.open();
    const { container } = render(() => <BranchSwitcher {...defaultProps()} />);

    await waitFor(() => {
      expect(container.querySelectorAll("[data-testid='branch-item']").length).toBe(5);
    });

    const input = container.querySelector("input")!;
    fireEvent.input(input, { target: { value: "zzzzz" } });

    await waitFor(() => {
      expect(container.querySelectorAll("[data-testid='branch-item']").length).toBe(0);
      expect(container.textContent).toContain("No branches match");
    });
  });

  it("calls onSelect for local branch on Enter", async () => {
    branchSwitcherStore.open();
    const props = defaultProps();
    const { container } = render(() => <BranchSwitcher {...props} />);

    await waitFor(() => {
      expect(container.querySelectorAll("[data-testid='branch-item']").length).toBeGreaterThan(0);
    });

    // First item is current branch (feat/login) — skip it.
    // Navigate to a non-current local branch and select it.
    fireEvent.keyDown(document, { key: "ArrowDown" }); // main
    fireEvent.keyDown(document, { key: "Enter" });

    expect(props.onSelect).toHaveBeenCalledWith("/repo", "main");
  });

  it("calls onCheckoutRemote for remote branch on Enter", async () => {
    branchSwitcherStore.open();
    const props = defaultProps();
    const { container } = render(() => <BranchSwitcher {...props} />);

    await waitFor(() => {
      expect(container.querySelectorAll("[data-testid='branch-item']").length).toBe(5);
    });

    // Filter to show only the remote-only branch
    const input = container.querySelector("input")!;
    fireEvent.input(input, { target: { value: "remote-only" } });

    await waitFor(() => {
      expect(container.querySelectorAll("[data-testid='branch-item']").length).toBe(1);
    });

    fireEvent.keyDown(document, { key: "Enter" });

    expect(props.onCheckoutRemote).toHaveBeenCalledWith("/repo", "feat/remote-only");
  });

  it("closes on Escape", async () => {
    branchSwitcherStore.open();
    render(() => <BranchSwitcher {...defaultProps()} />);

    await waitFor(() => {
      expect(branchSwitcherStore.state.isOpen).toBe(true);
    });

    fireEvent.keyDown(document, { key: "Escape" });
    expect(branchSwitcherStore.state.isOpen).toBe(false);
  });

  it("does not call any callback when selecting current branch", async () => {
    branchSwitcherStore.open();
    const props = defaultProps();
    const { container } = render(() => <BranchSwitcher {...props} />);

    await waitFor(() => {
      expect(container.querySelectorAll("[data-testid='branch-item']").length).toBe(5);
    });

    // feat/login is current and sorted first — click it
    const items = container.querySelectorAll("[data-testid='branch-item']");
    fireEvent.click(items[0]);

    expect(props.onSelect).not.toHaveBeenCalled();
    expect(props.onCheckoutRemote).not.toHaveBeenCalled();
    expect(branchSwitcherStore.state.isOpen).toBe(false);
  });

  it("shows 'No repository selected' when no active repo", async () => {
    branchSwitcherStore.open();
    const { container } = render(() => <BranchSwitcher {...defaultProps({ activeRepoPath: undefined })} />);

    await waitFor(() => {
      expect(container.textContent).toContain("No repository selected");
    });
  });

  it("navigates with arrow keys", async () => {
    branchSwitcherStore.open();
    const { container } = render(() => <BranchSwitcher {...defaultProps()} />);

    await waitFor(() => {
      expect(container.querySelectorAll("[data-testid='branch-item']").length).toBe(5);
    });

    // Move down once
    fireEvent.keyDown(document, { key: "ArrowDown" });

    // The second item should now be selected
    const items = container.querySelectorAll("[data-testid='branch-item']");
    const selectedItems = Array.from(items).filter((el) =>
      el.className.includes("selected"),
    );
    expect(selectedItems.length).toBe(1);
  });
});
