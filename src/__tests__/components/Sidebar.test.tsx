import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@solidjs/testing-library";
import "../mocks/tauri";

const { mockToggleExpanded, mockToggleCollapsed, mockGetActive, mockGetOrderedRepos, mockReorderRepo, mockTerminalsGet, mockGetCheckSummary, mockGetPrStatus, mockGetGroupedLayout, mockGetGroupForRepo, mockToggleGroupCollapsed, mockDeleteGroup, mockAddRepoToGroup, mockRemoveRepoFromGroup, mockCreateGroup, mockReorderRepoInGroup, mockMoveRepoBetweenGroups, mockReorderGroups } = vi.hoisted(() => ({
  mockToggleExpanded: vi.fn(),
  mockToggleCollapsed: vi.fn(),
  mockGetActive: vi.fn<() => any>(() => null),
  mockGetOrderedRepos: vi.fn<() => any[]>(() => []),
  mockReorderRepo: vi.fn(),
  mockTerminalsGet: vi.fn<() => any>(() => null),
  mockGetCheckSummary: vi.fn<() => any>(() => null),
  mockGetPrStatus: vi.fn<(...args: unknown[]) => unknown>(() => null),
  mockGetGroupedLayout: vi.fn<() => any>(() => ({ groups: [], ungrouped: [] })),
  mockGetGroupForRepo: vi.fn<(path: string) => any>(() => undefined),
  mockToggleGroupCollapsed: vi.fn(),
  mockDeleteGroup: vi.fn(),
  mockAddRepoToGroup: vi.fn(),
  mockRemoveRepoFromGroup: vi.fn(),
  mockCreateGroup: vi.fn(() => "new-group-id"),
  mockReorderRepoInGroup: vi.fn(),
  mockMoveRepoBetweenGroups: vi.fn(),
  mockReorderGroups: vi.fn(),
}));

// Mock stores before importing the component
vi.mock("../../stores/repositories", () => ({
  repositoriesStore: {
    state: {
      repositories: {} as Record<string, unknown>,
      repoOrder: [] as string[],
      activeRepoPath: null as string | null,
      groups: {} as Record<string, unknown>,
      groupOrder: [] as string[],
    },
    getActive: mockGetActive,
    getOrderedRepos: mockGetOrderedRepos,
    reorderRepo: mockReorderRepo,
    toggleExpanded: mockToggleExpanded,
    toggleCollapsed: mockToggleCollapsed,
    getGroupedLayout: mockGetGroupedLayout,
    getGroupForRepo: mockGetGroupForRepo,
    toggleGroupCollapsed: mockToggleGroupCollapsed,
    deleteGroup: mockDeleteGroup,
    addRepoToGroup: mockAddRepoToGroup,
    removeRepoFromGroup: mockRemoveRepoFromGroup,
    createGroup: mockCreateGroup,
    renameGroup: vi.fn(() => true),
    setGroupColor: vi.fn(),
    reorderRepoInGroup: mockReorderRepoInGroup,
    moveRepoBetweenGroups: mockMoveRepoBetweenGroups,
    reorderGroups: mockReorderGroups,
    getParkedRepos: vi.fn(() => []),
    setPark: vi.fn(),
    get: vi.fn(() => undefined),
    setActive: vi.fn(),
  },
}));

vi.mock("../../stores/repoSettings", () => ({
  repoSettingsStore: {
    get: vi.fn(() => undefined),
  },
}));

vi.mock("../../stores/terminals", () => ({
  terminalsStore: {
    get: mockTerminalsGet,
  },
}));

vi.mock("../../stores/github", () => ({
  githubStore: {
    getCheckSummary: mockGetCheckSummary,
    getPrStatus: mockGetPrStatus,
  },
}));

const { mockLastActivityAt } = vi.hoisted(() => ({
  mockLastActivityAt: vi.fn<() => number>(() => 0),
}));

vi.mock("../../stores/userActivity", () => ({
  userActivityStore: {
    lastActivityAt: mockLastActivityAt,
  },
}));

import { Sidebar } from "../../components/Sidebar/Sidebar";
import { _resetMergedActivityAccum } from "../../components/Sidebar/RepoSection";
import { repositoriesStore } from "../../stores/repositories";
import { uiStore } from "../../stores/ui";

/** Helper to create default no-op props for Sidebar */
function defaultProps(overrides: Partial<Parameters<typeof Sidebar>[0]> = {}) {
  return {
    onBranchSelect: vi.fn(),
    onAddTerminal: vi.fn(),
    onRemoveBranch: vi.fn(),
    onRenameBranch: vi.fn(),
    onAddWorktree: vi.fn(),
    onAddRepo: vi.fn(),
    onRepoSettings: vi.fn(),
    onRemoveRepo: vi.fn(),
    onOpenSettings: vi.fn(),
    onOpenHelp: vi.fn(),
    onBackgroundGit: vi.fn(),
    ...overrides,
  };
}

/** Helper to create a repository with branches */
function makeRepo(overrides: Record<string, any> = {}) {
  return {
    path: "/repo1",
    displayName: "Repo One",
    initials: "RO",
    expanded: true,
    collapsed: false,
    activeBranch: "main",
    branches: {
      main: {
        name: "main",
        isMain: true,
        worktreePath: null,
        terminals: [],
        additions: 0,
        deletions: 0,
      },
    },
    order: 0,
    ...overrides,
  };
}

/** Helper to set repo store state */
function setRepos(repos: Record<string, any>, activeRepoPath?: string) {
  (repositoriesStore.state as { repositories: Record<string, unknown> }).repositories = repos;
  (repositoriesStore.state as { activeRepoPath: string | null }).activeRepoPath = activeRepoPath ?? Object.keys(repos)[0] ?? null;
  const repoValues = Object.values(repos);
  mockGetOrderedRepos.mockReturnValue(repoValues);
  // Default grouped layout: all repos ungrouped (backward compatible)
  mockGetGroupedLayout.mockReturnValue({ groups: [], ungrouped: repoValues });
}

describe("Sidebar", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setRepos({});
    mockGetActive.mockReturnValue(null);
    mockTerminalsGet.mockReturnValue(null);
    mockLastActivityAt.mockReturnValue(0);
    _resetMergedActivityAccum();
  });

  describe("empty state", () => {
    it("renders 'No repositories' and 'Add Repository' button when no repos exist", () => {
      const { container } = render(() => <Sidebar {...defaultProps()} />);

      const emptyDiv = container.querySelector(".empty");
      expect(emptyDiv).not.toBeNull();
      expect(emptyDiv!.textContent).toContain("No repositories");

      const addButton = emptyDiv!.querySelector("button");
      expect(addButton).not.toBeNull();
      expect(addButton!.textContent).toBe("Add Repository");
    });

    it("calls onAddRepo when empty-state Add Repository button is clicked", () => {
      const onAddRepo = vi.fn();
      const { container } = render(() => <Sidebar {...defaultProps({ onAddRepo })} />);

      const addButton = container.querySelector(".empty button");
      expect(addButton).not.toBeNull();
      fireEvent.click(addButton!);
      expect(onAddRepo).toHaveBeenCalledOnce();
    });
  });

  describe("footer buttons", () => {
    it("calls onAddRepo when footer Add Repository button is clicked", () => {
      const onAddRepo = vi.fn();
      const { container } = render(() => <Sidebar {...defaultProps({ onAddRepo })} />);

      const footerAddBtn = container.querySelector(".addRepo");
      expect(footerAddBtn).not.toBeNull();
      fireEvent.click(footerAddBtn!);
      expect(onAddRepo).toHaveBeenCalled();
    });

    it("calls onOpenSettings when Settings footer button is clicked", () => {
      const onOpenSettings = vi.fn();
      const { container } = render(() => <Sidebar {...defaultProps({ onOpenSettings })} />);

      const settingsBtn = container.querySelector('.footerAction[title="Settings"]');
      expect(settingsBtn).not.toBeNull();
      fireEvent.click(settingsBtn!);
      expect(onOpenSettings).toHaveBeenCalledOnce();
    });

    it("calls onOpenHelp when Help footer button is clicked", () => {
      const onOpenHelp = vi.fn();
      const { container } = render(() => <Sidebar {...defaultProps({ onOpenHelp })} />);

      const helpBtn = container.querySelector('.footerAction[title="Help"]');
      expect(helpBtn).not.toBeNull();
      fireEvent.click(helpBtn!);
      expect(onOpenHelp).toHaveBeenCalledOnce();
    });

    it("does not render unimplemented Notifications and Tasks buttons", () => {
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      expect(container.querySelector('.footerAction[title="Notifications"]')).toBeNull();
      expect(container.querySelector('.footerAction[title="Tasks"]')).toBeNull();
    });
  });

  describe("with repositories", () => {
    beforeEach(() => {
      setRepos({ "/repo1": makeRepo() });
    });

    it("renders repo sections when repos exist", () => {
      const { container } = render(() => <Sidebar {...defaultProps()} />);

      const repoSections = container.querySelectorAll(".repoSection");
      expect(repoSections.length).toBe(1);

      // Should NOT show empty state
      const emptyDiv = container.querySelector(".empty");
      expect(emptyDiv).toBeNull();
    });

    it("shows repo display name", () => {
      const { container } = render(() => <Sidebar {...defaultProps()} />);

      const repoName = container.querySelector(".repoName");
      expect(repoName).not.toBeNull();
      expect(repoName!.textContent).toBe("Repo One");
    });

    it("renders branch items for expanded repo", () => {
      const { container } = render(() => <Sidebar {...defaultProps()} />);

      const branchItems = container.querySelectorAll(".branchItem");
      expect(branchItems.length).toBe(1);

      const branchName = container.querySelector(".branchName");
      expect(branchName).not.toBeNull();
      expect(branchName!.textContent).toBe("main");
    });

    it("shows SVG icons for main and feature branches", () => {
      setRepos({
        "/repo1": makeRepo({
          branches: {
            main: { name: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
            "feature/x": { name: "feature/x", isMain: false, worktreePath: "/wt/x", terminals: [], additions: 0, deletions: 0 },
          },
        }),
      });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const icons = container.querySelectorAll(".branchIcon");
      expect(icons.length).toBe(2);
      // Main branch first (sorted), then feature
      const mainIcon = Array.from(icons).find((i) => i.classList.contains("branchIconMain"));
      const featureIcon = Array.from(icons).find((i) => i.classList.contains("branchIconFeature"));
      expect(mainIcon).toBeDefined();
      expect(mainIcon!.querySelector("svg")).not.toBeNull();
      expect(featureIcon).toBeDefined();
      expect(featureIcon!.querySelector("svg")).not.toBeNull();
    });

    it("sorts branches with main first", () => {
      setRepos({
        "/repo1": makeRepo({
          branches: {
            "feature/z": { name: "feature/z", isMain: false, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
            main: { name: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
            "feature/a": { name: "feature/a", isMain: false, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
          },
        }),
      });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const names = container.querySelectorAll(".branchName");
      expect(names.length).toBe(3);
      expect(names[0].textContent).toBe("main");
      expect(names[1].textContent).toBe("feature/a");
      expect(names[2].textContent).toBe("feature/z");
    });

    it("sorts merged PR branches to bottom", () => {
      setRepos({
        "/repo1": makeRepo({
          branches: {
            main: { name: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
            "feature/active": { name: "feature/active", isMain: false, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
            "feature/merged": { name: "feature/merged", isMain: false, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
            "feature/closed": { name: "feature/closed", isMain: false, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
          },
        }),
      });
      // Mock merged and closed PR states
      mockGetPrStatus.mockImplementation((_repoPath: unknown, branch: unknown) => {
        if (branch === "feature/merged") return { state: "MERGED", number: 1, title: "", url: "" };
        if (branch === "feature/closed") return { state: "CLOSED", number: 2, title: "", url: "" };
        if (branch === "feature/active") return { state: "OPEN", number: 3, title: "", url: "" };
        return null;
      });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const names = container.querySelectorAll(".branchName");
      expect(names.length).toBe(4);
      expect(names[0].textContent).toBe("main");
      expect(names[1].textContent).toBe("feature/active");
      // Merged/closed at bottom, alphabetically
      expect(names[2].textContent).toBe("feature/closed");
      expect(names[3].textContent).toBe("feature/merged");
    });

    it("marks active branch with active class", () => {
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const branchItem = container.querySelector(".branchItem");
      expect(branchItem).not.toBeNull();
      expect(branchItem!.classList.contains("active")).toBe(true);
    });

    it("branch item click calls onBranchSelect", () => {
      const onBranchSelect = vi.fn();
      const { container } = render(() => <Sidebar {...defaultProps({ onBranchSelect })} />);
      const branchItem = container.querySelector(".branchItem")!;
      fireEvent.click(branchItem);
      expect(onBranchSelect).toHaveBeenCalledWith("/repo1", "main");
    });

    it("add terminal button click calls onAddTerminal", () => {
      const onAddTerminal = vi.fn();
      const { container } = render(() => <Sidebar {...defaultProps({ onAddTerminal })} />);
      const addBtn = container.querySelector(".branchAddBtn")!;
      fireEvent.click(addBtn);
      expect(onAddTerminal).toHaveBeenCalledWith("/repo1", "main");
    });

    it("main branch has no remove button", () => {
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const removeBtn = container.querySelector(".branchRemoveBtn");
      expect(removeBtn).toBeNull();
    });

    it("feature branch has remove button that calls onRemoveBranch", () => {
      setRepos({
        "/repo1": makeRepo({
          branches: {
            main: { name: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
            "feature/x": { name: "feature/x", isMain: false, worktreePath: "/wt/x", terminals: [], additions: 0, deletions: 0 },
          },
        }),
      });
      const onRemoveBranch = vi.fn();
      const { container } = render(() => <Sidebar {...defaultProps({ onRemoveBranch })} />);
      const removeBtns = container.querySelectorAll(".branchRemoveBtn");
      expect(removeBtns.length).toBe(1);
      fireEvent.click(removeBtns[0]);
      expect(onRemoveBranch).toHaveBeenCalledWith("/repo1", "feature/x");
    });

    it("non-main branch without worktreePath has no remove button", () => {
      setRepos({
        "/repo1": makeRepo({
          branches: {
            main: { name: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
            "feature/y": { name: "feature/y", isMain: false, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
          },
        }),
      });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const removeBtns = container.querySelectorAll(".branchRemoveBtn");
      expect(removeBtns.length).toBe(0);
    });

    it("double-click main branch name calls onAddTerminal instead of rename", () => {
      const onAddTerminal = vi.fn();
      const onRenameBranch = vi.fn();
      const { container } = render(() => <Sidebar {...defaultProps({ onAddTerminal, onRenameBranch })} />);
      const branchName = container.querySelector(".branchName")!;
      fireEvent.dblClick(branchName);
      expect(onRenameBranch).not.toHaveBeenCalled();
      expect(onAddTerminal).toHaveBeenCalledWith("/repo1", "main");
    });

    it("double-click feature branch name calls onRenameBranch", () => {
      const onRenameBranch = vi.fn();
      setRepos({ "/repo1": makeRepo({ branches: {
        main: { name: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
        "feature/x": { name: "feature/x", isMain: false, worktreePath: "/wt/x", terminals: [], additions: 0, deletions: 0 },
      }}) });
      const { container } = render(() => <Sidebar {...defaultProps({ onRenameBranch })} />);
      const branchNames = container.querySelectorAll(".branchName");
      // feature/x is the second branch
      const featureBranch = branchNames[1]!;
      fireEvent.dblClick(featureBranch);
      expect(onRenameBranch).toHaveBeenCalledWith("/repo1", "feature/x");
    });

    it("add worktree button click calls onAddWorktree", () => {
      const onAddWorktree = vi.fn();
      const { container } = render(() => <Sidebar {...defaultProps({ onAddWorktree })} />);
      const addBtn = container.querySelector(".addBtn")!;
      fireEvent.click(addBtn);
      expect(onAddWorktree).toHaveBeenCalledWith("/repo1");
    });

    it("repo menu opens on click and shows Settings and Remove options", () => {
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      // Click the ⋯ button
      const menuBtn = container.querySelector(".repoActionBtn")!;
      fireEvent.click(menuBtn);

      const menu = container.querySelector(".menu");
      expect(menu).not.toBeNull();

      const menuItems = menu!.querySelectorAll(".item");
      expect(menuItems.length).toBe(5);
      expect(menuItems[0].textContent).toContain("Repo Settings");
      expect(menuItems[1].textContent).toContain("Move to Group");
      expect(menuItems[2].textContent).toContain("Show All Branches");
      expect(menuItems[3].textContent).toContain("Park Repository");
      expect(menuItems[4].textContent).toContain("Remove Repository");
    });

    it("repo menu Settings click calls onRepoSettings", () => {
      const onRepoSettings = vi.fn();
      const { container } = render(() => <Sidebar {...defaultProps({ onRepoSettings })} />);

      // Open menu
      const menuBtn = container.querySelector(".repoActionBtn")!;
      fireEvent.click(menuBtn);

      // Click settings
      const menuItems = container.querySelectorAll(".item");
      fireEvent.click(menuItems[0]);
      expect(onRepoSettings).toHaveBeenCalledWith("/repo1");
    });

    it("repo menu Remove click calls onRemoveRepo", () => {
      const onRemoveRepo = vi.fn();
      const { container } = render(() => <Sidebar {...defaultProps({ onRemoveRepo })} />);

      // Open menu
      const menuBtn = container.querySelector(".repoActionBtn")!;
      fireEvent.click(menuBtn);

      // Click remove — index 4 (after "Repo Settings", "Move to Group", "Show All Branches", and "Park Repository")
      const menuItems = container.querySelectorAll(".item");
      fireEvent.click(menuItems[4]);
      expect(onRemoveRepo).toHaveBeenCalledWith("/repo1");
    });

    it("repo menu closes after action", () => {
      const { container } = render(() => <Sidebar {...defaultProps()} />);

      // Open menu
      const menuBtn = container.querySelector(".repoActionBtn")!;
      fireEvent.click(menuBtn);
      expect(container.querySelector(".menu")).not.toBeNull();

      // Click settings to close menu
      const menuItems = container.querySelectorAll(".item");
      fireEvent.click(menuItems[0]);

      // Menu should be closed
      expect(container.querySelector(".menu")).toBeNull();
    });

    it("repo menu closes on Escape", () => {
      const { container } = render(() => <Sidebar {...defaultProps()} />);

      // Open menu
      const menuBtn = container.querySelector(".repoActionBtn")!;
      fireEvent.click(menuBtn);
      expect(container.querySelector(".menu")).not.toBeNull();

      // Press Escape
      fireEvent.keyDown(document, { key: "Escape" });
      expect(container.querySelector(".menu")).toBeNull();
    });

    it("repo menu toggles on repeated clicks", () => {
      const { container } = render(() => <Sidebar {...defaultProps()} />);

      const menuBtn = container.querySelector(".repoActionBtn")!;

      // Open
      fireEvent.click(menuBtn);
      expect(container.querySelector(".menu")).not.toBeNull();

      // Close
      fireEvent.click(menuBtn);
      expect(container.querySelector(".menu")).toBeNull();
    });

    it("repo header right-click opens context menu with Settings and Remove options", () => {
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const header = container.querySelector(".repoHeader")!;
      fireEvent.contextMenu(header, { clientX: 100, clientY: 200 });

      const menu = container.querySelector(".menu");
      expect(menu).not.toBeNull();

      const menuItems = menu!.querySelectorAll(".item");
      expect(menuItems.length).toBe(5);
      expect(menuItems[0].textContent).toContain("Repo Settings");
      expect(menuItems[1].textContent).toContain("Move to Group");
      expect(menuItems[2].textContent).toContain("Show All Branches");
      expect(menuItems[3].textContent).toContain("Park Repository");
      expect(menuItems[4].textContent).toContain("Remove Repository");
    });

    it("repo header click calls toggleExpanded", () => {
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const header = container.querySelector(".repoHeader")!;
      fireEvent.click(header);
      expect(mockToggleExpanded).toHaveBeenCalledWith("/repo1");
    });

    it("shows 'No branches loaded' when repo has no branches", () => {
      setRepos({
        "/repo1": makeRepo({ branches: {} }),
      });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const empty = container.querySelector(".repoEmpty");
      expect(empty).not.toBeNull();
      expect(empty!.textContent).toBe("No branches loaded");
    });

    it("renders a chevron toggle in the repo header", () => {
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const chevron = container.querySelector(".repoChevron");
      expect(chevron).not.toBeNull();
    });

    it("chevron has expanded class when repo is expanded", () => {
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const chevron = container.querySelector(".repoChevron");
      expect(chevron!.classList.contains("expanded")).toBe(true);
    });

    it("chevron does not have expanded class when repo is not expanded", () => {
      setRepos({
        "/repo1": makeRepo({ expanded: false }),
      });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const chevron = container.querySelector(".repoChevron");
      expect(chevron).not.toBeNull();
      expect(chevron!.classList.contains("expanded")).toBe(false);
    });
  });

  describe("collapsed repo", () => {
    it("shows initials and not branches when collapsed", () => {
      setRepos({
        "/repo1": makeRepo({ collapsed: true }),
      });
      const { container } = render(() => <Sidebar {...defaultProps()} />);

      const initials = container.querySelector(".repoInitials");
      expect(initials).not.toBeNull();
      expect(initials!.textContent).toBe("RO");

      // Should not show repo name or branches
      const repoName = container.querySelector(".repoName");
      expect(repoName).toBeNull();

      const branchItems = container.querySelectorAll(".branchItem");
      expect(branchItems.length).toBe(0);
    });

    it("shows collapsed class on repo section", () => {
      setRepos({
        "/repo1": makeRepo({ collapsed: true }),
      });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const section = container.querySelector(".repoSection");
      expect(section).not.toBeNull();
      expect(section!.classList.contains("collapsed")).toBe(true);
    });

    it("initials click calls toggleCollapsed", () => {
      setRepos({
        "/repo1": makeRepo({ collapsed: true }),
      });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const initials = container.querySelector(".repoInitials")!;
      fireEvent.click(initials);
      expect(mockToggleCollapsed).toHaveBeenCalledWith("/repo1");
    });
  });

  describe("unexpanded repo", () => {
    it("does not show branches when expanded is false and not collapsed", () => {
      setRepos({
        "/repo1": makeRepo({ expanded: false, collapsed: false }),
      });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const branchItems = container.querySelectorAll(".branchItem");
      expect(branchItems.length).toBe(0);
      // But should still show repo name
      expect(container.querySelector(".repoName")!.textContent).toBe("Repo One");
    });
  });

  describe("Git Quick Actions", () => {
    it("shows git quick actions when getActive returns a repo", () => {
      mockGetActive.mockReturnValue({ path: "/repo1" });
      setRepos({ "/repo1": makeRepo() });
      const { container } = render(() => <Sidebar {...defaultProps()} />);

      const quickActions = container.querySelector(".gitQuickActions");
      expect(quickActions).not.toBeNull();

      const buttons = container.querySelectorAll(".gitQuickBtn");
      expect(buttons.length).toBe(4);
    });

    it("does not show git quick actions when getActive returns null", () => {
      mockGetActive.mockReturnValue(null);
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const quickActions = container.querySelector(".gitQuickActions");
      expect(quickActions).toBeNull();
    });

    it("Pull button calls onBackgroundGit with pull args", () => {
      mockGetActive.mockReturnValue({ path: "/repo1" });
      setRepos({ "/repo1": makeRepo() });
      const onBackgroundGit = vi.fn();
      const { container } = render(() => <Sidebar {...defaultProps({ onBackgroundGit })} />);

      const buttons = container.querySelectorAll(".gitQuickBtn");
      const pullBtn = Array.from(buttons).find((b) => b.textContent?.includes("Pull"))!;
      fireEvent.click(pullBtn);
      expect(onBackgroundGit).toHaveBeenCalledWith("/repo1", "pull", ["pull"]);
    });

    it("Push button calls onBackgroundGit with push args", () => {
      mockGetActive.mockReturnValue({ path: "/repo1" });
      setRepos({ "/repo1": makeRepo() });
      const onBackgroundGit = vi.fn();
      const { container } = render(() => <Sidebar {...defaultProps({ onBackgroundGit })} />);

      const buttons = container.querySelectorAll(".gitQuickBtn");
      const pushBtn = Array.from(buttons).find((b) => b.textContent?.includes("Push"))!;
      fireEvent.click(pushBtn);
      expect(onBackgroundGit).toHaveBeenCalledWith("/repo1", "push", ["push"]);
    });

    it("Fetch button calls onBackgroundGit with fetch args", () => {
      mockGetActive.mockReturnValue({ path: "/repo1" });
      setRepos({ "/repo1": makeRepo() });
      const onBackgroundGit = vi.fn();
      const { container } = render(() => <Sidebar {...defaultProps({ onBackgroundGit })} />);

      const buttons = container.querySelectorAll(".gitQuickBtn");
      const fetchBtn = Array.from(buttons).find((b) => b.textContent?.includes("Fetch"))!;
      fireEvent.click(fetchBtn);
      expect(onBackgroundGit).toHaveBeenCalledWith("/repo1", "fetch", ["fetch", "--all"]);
    });

    it("Stash button calls onBackgroundGit with stash args", () => {
      mockGetActive.mockReturnValue({ path: "/repo1" });
      setRepos({ "/repo1": makeRepo() });
      const onBackgroundGit = vi.fn();
      const { container } = render(() => <Sidebar {...defaultProps({ onBackgroundGit })} />);

      const buttons = container.querySelectorAll(".gitQuickBtn");
      const stashBtn = Array.from(buttons).find((b) => b.textContent?.includes("Stash"))!;
      fireEvent.click(stashBtn);
      expect(onBackgroundGit).toHaveBeenCalledWith("/repo1", "stash", ["stash"]);
    });

    it("disables button when operation is in runningGitOps", () => {
      mockGetActive.mockReturnValue({ path: "/repo1" });
      setRepos({ "/repo1": makeRepo() });
      const runningOps = new Set(["pull"]);
      const { container } = render(() => <Sidebar {...defaultProps({ runningGitOps: runningOps })} />);

      const buttons = container.querySelectorAll(".gitQuickBtn");
      const pullBtn = Array.from(buttons).find((b) => b.textContent?.includes("Pull"))!;
      expect(pullBtn.hasAttribute("disabled")).toBe(true);

      // Other buttons should not be disabled
      const pushBtn = Array.from(buttons).find((b) => b.textContent?.includes("Push"))!;
      expect(pushBtn.hasAttribute("disabled")).toBe(false);
    });
  });

  describe("quick switcher mode", () => {
    it("shows shortcut keys instead of action buttons in quick switcher mode", () => {
      setRepos({ "/repo1": makeRepo() });
      const { container } = render(() => <Sidebar {...defaultProps({ quickSwitcherActive: true })} />);

      const shortcut = container.querySelector(".branchShortcut");
      expect(shortcut).not.toBeNull();
      expect(shortcut!.textContent).toContain("1");

      // Should not show add/remove buttons
      const addBtn = container.querySelector(".branchAddBtn");
      expect(addBtn).toBeNull();
    });

    it("forces branches visible in quick switcher even when not expanded", () => {
      setRepos({ "/repo1": makeRepo({ expanded: false }) });
      const { container } = render(() => <Sidebar {...defaultProps({ quickSwitcherActive: true })} />);

      const branchItems = container.querySelectorAll(".branchItem");
      expect(branchItems.length).toBe(1);
    });
  });

  describe("branch badges", () => {
    it("shows StatsBadge with additions and deletions", () => {
      setRepos({
        "/repo1": makeRepo({
          branches: {
            main: { name: "main", isMain: true, worktreePath: null, terminals: [], additions: 10, deletions: 5 },
          },
        }),
      });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const stats = container.querySelector(".branchStats");
      expect(stats).not.toBeNull();
      const addStat = container.querySelector(".statAdd");
      const delStat = container.querySelector(".statDel");
      expect(addStat!.textContent).toBe("+10");
      expect(delStat!.textContent).toBe("-5");
    });

    it("does not show StatsBadge when both additions and deletions are 0", () => {
      setRepos({
        "/repo1": makeRepo({
          branches: {
            main: { name: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
          },
        }),
      });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const stats = container.querySelector(".branchStats");
      expect(stats).toBeNull();
    });

    it("shows StatsBadge when only additions > 0", () => {
      setRepos({
        "/repo1": makeRepo({
          branches: {
            main: { name: "main", isMain: true, worktreePath: null, terminals: [], additions: 5, deletions: 0 },
          },
        }),
      });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const stats = container.querySelector(".branchStats");
      expect(stats).not.toBeNull();
    });

    it("shows StatsBadge when only deletions > 0", () => {
      setRepos({
        "/repo1": makeRepo({
          branches: {
            main: { name: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 3 },
          },
        }),
      });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const stats = container.querySelector(".branchStats");
      expect(stats).not.toBeNull();
    });

    it("shows PrStateBadge when GitHub store has PR data for branch", () => {
      setRepos({
        "/repo1": makeRepo({
          branches: {
            main: { name: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
          },
        }),
      });
      mockGetPrStatus.mockReturnValue({ state: "OPEN", number: 123, title: "Test PR", url: "https://example.com" });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const prBadge = container.querySelector(".prBadge");
      expect(prBadge).not.toBeNull();
      expect(prBadge!.getAttribute("title")).toBe("PR #123");
    });

    it("does not show PrStateBadge when branch has no PR data", () => {
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const prBadge = container.querySelector(".prBadge");
      expect(prBadge).toBeNull();
    });

    it("shows Merged label and class when PR state is MERGED", () => {
      setRepos({
        "/repo1": makeRepo({
          branches: {
            main: { name: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
          },
        }),
      });
      mockGetPrStatus.mockReturnValue({ state: "MERGED", number: 42, title: "Test", url: "https://example.com" });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const prBadge = container.querySelector(".prBadge");
      expect(prBadge).not.toBeNull();
      expect(prBadge!.classList.contains("prMerged")).toBe(true);
      expect(prBadge!.textContent).toBe("Merged");
    });

    it("hides PR badge immediately for CLOSED PR", () => {
      setRepos({
        "/repo1": makeRepo({
          branches: {
            main: { name: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
          },
        }),
      });
      mockGetPrStatus.mockReturnValue({ state: "CLOSED", number: 43, title: "Test", url: "https://example.com" });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const prBadge = container.querySelector(".prBadge");
      expect(prBadge).toBeNull();
    });

    it("shows Draft label and class when PR is a draft", () => {
      setRepos({
        "/repo1": makeRepo({
          branches: {
            main: { name: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
          },
        }),
      });
      mockGetPrStatus.mockReturnValue({ state: "OPEN", number: 45, title: "Draft PR", url: "https://example.com", is_draft: true });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const prBadge = container.querySelector(".prBadge");
      expect(prBadge).not.toBeNull();
      expect(prBadge!.classList.contains("prDraft")).toBe(true);
      expect(prBadge!.textContent).toBe("Draft");
    });

    it("shows open class with PR number when state is OPEN with no special conditions", () => {
      setRepos({
        "/repo1": makeRepo({
          branches: {
            main: { name: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
          },
        }),
      });
      mockGetPrStatus.mockReturnValue({ state: "OPEN", number: 44, title: "Test", url: "https://example.com" });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const prBadge = container.querySelector(".prBadge");
      expect(prBadge).not.toBeNull();
      expect(prBadge!.classList.contains("prOpen")).toBe(true);
      expect(prBadge!.textContent).toBe("#44");
    });
  });

  describe("branch activity indicator", () => {
    it("adds has-activity class when terminal has activity", () => {
      setRepos({
        "/repo1": makeRepo({
          branches: {
            main: { name: "main", isMain: true, worktreePath: null, terminals: ["t1"], additions: 0, deletions: 0 },
          },
        }),
      });
      mockTerminalsGet.mockReturnValue({ activity: true });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const branchItem = container.querySelector(".branchItem");
      expect(branchItem!.classList.contains("hasActivity")).toBe(true);
    });

    it("does not add has-activity class when no terminal activity", () => {
      mockTerminalsGet.mockReturnValue({ activity: false });
      setRepos({
        "/repo1": makeRepo({
          branches: {
            main: { name: "main", isMain: true, worktreePath: null, terminals: ["t1"], additions: 0, deletions: 0 },
          },
        }),
      });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const branchItem = container.querySelector(".branchItem");
      expect(branchItem!.classList.contains("hasActivity")).toBe(false);
    });
  });

  describe("multiple repos", () => {
    it("renders multiple repo sections", () => {
      setRepos({
        "/repo1": makeRepo(),
        "/repo2": makeRepo({ path: "/repo2", displayName: "Repo Two", initials: "RT" }),
      });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const repoSections = container.querySelectorAll(".repoSection");
      expect(repoSections.length).toBe(2);
    });
  });

  describe("click outside repo menu", () => {
    it("closes repo menu when clicking outside", () => {
      setRepos({ "/repo1": makeRepo() });
      const { container } = render(() => <Sidebar {...defaultProps()} />);

      // Open menu
      const menuBtn = container.querySelector(".repoActionBtn")!;
      fireEvent.click(menuBtn);
      expect(container.querySelector(".menu")).not.toBeNull();

      // Click somewhere outside the menu (on the sidebar itself)
      fireEvent.mouseDown(container.querySelector("[data-testid='sidebar']")!);
      expect(container.querySelector(".menu")).toBeNull();
    });
  });

  describe("context menu", () => {
    it("opens context menu on right-click of branch item", () => {
      setRepos({ "/repo1": makeRepo() });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const branchItem = container.querySelector(".branchItem")!;
      fireEvent.contextMenu(branchItem, { clientX: 100, clientY: 200 });

      const contextMenu = container.querySelector(".menu");
      expect(contextMenu).not.toBeNull();

      const items = contextMenu!.querySelectorAll(".item");
      // Copy Path, Add Terminal, Rename Branch (no Delete Worktree for main)
      expect(items.length).toBe(3);
    });

    it("context menu Copy Path action copies worktreePath to clipboard", async () => {
      const writeTextMock = vi.fn().mockResolvedValue(undefined);
      Object.defineProperty(navigator, "clipboard", {
        value: { writeText: writeTextMock },
        writable: true,
        configurable: true,
      });

      setRepos({
        "/repo1": makeRepo({
          branches: {
            main: { name: "main", isMain: true, worktreePath: "/path/to/repo", terminals: [], additions: 0, deletions: 0 },
          },
        }),
      });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const branchItem = container.querySelector(".branchItem")!;
      fireEvent.contextMenu(branchItem, { clientX: 100, clientY: 200 });

      const contextMenu = container.querySelector(".menu");
      const items = contextMenu!.querySelectorAll(".item");
      // Find "Copy Path" item
      const copyPathItem = Array.from(items).find((i) =>
        i.querySelector(".label")?.textContent === "Copy Path"
      )!;
      fireEvent.click(copyPathItem);

      // The action is async (uses navigator.clipboard.writeText)
      await vi.waitFor(() => {
        expect(writeTextMock).toHaveBeenCalledWith("/path/to/repo");
      });
    });

    it("context menu Copy Path is disabled when no worktreePath", () => {
      setRepos({
        "/repo1": makeRepo({
          branches: {
            main: { name: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
          },
        }),
      });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const branchItem = container.querySelector(".branchItem")!;
      fireEvent.contextMenu(branchItem, { clientX: 100, clientY: 200 });

      const contextMenu = container.querySelector(".menu");
      const items = contextMenu!.querySelectorAll(".item");
      const copyPathItem = Array.from(items).find((i) =>
        i.querySelector(".label")?.textContent === "Copy Path"
      )!;
      expect(copyPathItem.hasAttribute("disabled")).toBe(true);
    });

    it("does not show Delete Worktree for non-main branch without worktreePath", () => {
      setRepos({
        "/repo1": makeRepo({
          branches: {
            main: { name: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
            "feature/y": { name: "feature/y", isMain: false, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
          },
        }),
      });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const branchItems = container.querySelectorAll(".branchItem");
      fireEvent.contextMenu(branchItems[1], { clientX: 100, clientY: 200 });

      const contextMenu = container.querySelector(".menu");
      const items = contextMenu!.querySelectorAll(".item");
      // Copy Path, Add Terminal, Rename Branch (NO Delete Worktree)
      expect(items.length).toBe(3);
      const labels = Array.from(items).map((i) => i.querySelector(".label")!.textContent);
      expect(labels).not.toContain("Delete Worktree");
    });

    it("shows Delete Worktree option for non-main branch context menu", () => {
      setRepos({
        "/repo1": makeRepo({
          branches: {
            main: { name: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
            "feature/x": { name: "feature/x", isMain: false, worktreePath: "/wt/x", terminals: [], additions: 0, deletions: 0 },
          },
        }),
      });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const branchItems = container.querySelectorAll(".branchItem");
      // feature/x is second (sorted after main)
      fireEvent.contextMenu(branchItems[1], { clientX: 100, clientY: 200 });

      const contextMenu = container.querySelector(".menu");
      const items = contextMenu!.querySelectorAll(".item");
      // Copy Path, Add Terminal, Rename Branch, Delete Worktree
      expect(items.length).toBe(4);
      const labels = Array.from(items).map((i) => i.querySelector(".label")!.textContent);
      expect(labels).toContain("Delete Worktree");
    });
  });

  describe("group sections", () => {
    it("renders group headers with name and chevron", () => {
      const repo = makeRepo();
      setRepos({ "/repo1": repo });
      mockGetGroupedLayout.mockReturnValue({
        groups: [{
          group: { id: "g1", name: "Work", color: "", collapsed: false, repoOrder: ["/repo1"] },
          repos: [repo],
        }],
        ungrouped: [],
      });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const header = container.querySelector(".groupHeader");
      expect(header).not.toBeNull();
      expect(container.querySelector(".groupName")!.textContent).toBe("Work");
      expect(container.querySelector(".groupChevron")).not.toBeNull();
    });

    it("renders repos inside group", () => {
      const repo = makeRepo();
      setRepos({ "/repo1": repo });
      mockGetGroupedLayout.mockReturnValue({
        groups: [{
          group: { id: "g1", name: "Work", color: "", collapsed: false, repoOrder: ["/repo1"] },
          repos: [repo],
        }],
        ungrouped: [],
      });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const groupSection = container.querySelector(".groupSection");
      expect(groupSection).not.toBeNull();
      const repoSections = groupSection!.querySelectorAll(".repoSection");
      expect(repoSections.length).toBe(1);
    });

    it("clicking group header toggles collapsed", () => {
      const repo = makeRepo();
      setRepos({ "/repo1": repo });
      mockGetGroupedLayout.mockReturnValue({
        groups: [{
          group: { id: "g1", name: "Work", color: "", collapsed: false, repoOrder: ["/repo1"] },
          repos: [repo],
        }],
        ungrouped: [],
      });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const header = container.querySelector(".groupHeader")!;
      fireEvent.click(header);
      expect(mockToggleGroupCollapsed).toHaveBeenCalledWith("g1");
    });

    it("collapsed group hides repos", () => {
      const repo = makeRepo();
      setRepos({ "/repo1": repo });
      mockGetGroupedLayout.mockReturnValue({
        groups: [{
          group: { id: "g1", name: "Work", color: "", collapsed: true, repoOrder: ["/repo1"] },
          repos: [repo],
        }],
        ungrouped: [],
      });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const groupRepos = container.querySelector(".groupRepos");
      expect(groupRepos).toBeNull();
    });

    it("group color dot renders when color is set", () => {
      const repo = makeRepo();
      setRepos({ "/repo1": repo });
      mockGetGroupedLayout.mockReturnValue({
        groups: [{
          group: { id: "g1", name: "Work", color: "#4A9EFF", collapsed: false, repoOrder: ["/repo1"] },
          repos: [repo],
        }],
        ungrouped: [],
      });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const dot = container.querySelector(".groupColorDot");
      expect(dot).not.toBeNull();
    });

    it("group color dot does not render when no color", () => {
      const repo = makeRepo();
      setRepos({ "/repo1": repo });
      mockGetGroupedLayout.mockReturnValue({
        groups: [{
          group: { id: "g1", name: "Work", color: "", collapsed: false, repoOrder: ["/repo1"] },
          repos: [repo],
        }],
        ungrouped: [],
      });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const dot = container.querySelector(".groupColorDot");
      expect(dot).toBeNull();
    });

    it("repo count badge shows correct number", () => {
      const repo1 = makeRepo();
      const repo2 = makeRepo({ path: "/repo2", displayName: "Repo Two", initials: "RT" });
      setRepos({ "/repo1": repo1, "/repo2": repo2 });
      mockGetGroupedLayout.mockReturnValue({
        groups: [{
          group: { id: "g1", name: "Work", color: "", collapsed: false, repoOrder: ["/repo1", "/repo2"] },
          repos: [repo1, repo2],
        }],
        ungrouped: [],
      });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const count = container.querySelector(".groupCount");
      expect(count).not.toBeNull();
      expect(count!.textContent).toBe("2");
    });

    it("groups render before ungrouped repos", () => {
      const repo1 = makeRepo();
      const repo2 = makeRepo({ path: "/repo2", displayName: "Repo Two", initials: "RT" });
      setRepos({ "/repo1": repo1, "/repo2": repo2 });
      mockGetGroupedLayout.mockReturnValue({
        groups: [{
          group: { id: "g1", name: "Work", color: "", collapsed: false, repoOrder: ["/repo1"] },
          repos: [repo1],
        }],
        ungrouped: [repo2],
      });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const repoList = container.querySelector(".repoList")!;
      const groupSection = repoList.querySelector(".groupSection");
      const repoSections = repoList.querySelectorAll(":scope > .repoSection");
      // Group section should exist
      expect(groupSection).not.toBeNull();
      // Ungrouped repo should render as direct child repo-section
      expect(repoSections.length).toBe(1);
    });

    it("empty group shows placeholder text", () => {
      setRepos({});
      mockGetGroupedLayout.mockReturnValue({
        groups: [{
          group: { id: "g1", name: "Empty Group", color: "", collapsed: false, repoOrder: [] },
          repos: [],
        }],
        ungrouped: [],
      });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const hint = container.querySelector(".groupEmptyHint");
      expect(hint).not.toBeNull();
    });

    it("group header right-click shows Rename, Change Color, Delete", () => {
      const repo = makeRepo();
      setRepos({ "/repo1": repo });
      mockGetGroupedLayout.mockReturnValue({
        groups: [{
          group: { id: "g1", name: "Work", color: "", collapsed: false, repoOrder: ["/repo1"] },
          repos: [repo],
        }],
        ungrouped: [],
      });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const header = container.querySelector(".groupHeader")!;
      fireEvent.contextMenu(header, { clientX: 100, clientY: 200 });
      const menu = container.querySelector(".menu");
      expect(menu).not.toBeNull();
      const labels = Array.from(menu!.querySelectorAll(".label")).map((el) => el.textContent);
      expect(labels).toContain("Rename Group");
      expect(labels).toContain("Change Color");
      expect(labels).toContain("Delete Group");
    });

    it("delete group calls deleteGroup()", () => {
      const repo = makeRepo();
      setRepos({ "/repo1": repo });
      mockGetGroupedLayout.mockReturnValue({
        groups: [{
          group: { id: "g1", name: "Work", color: "", collapsed: false, repoOrder: ["/repo1"] },
          repos: [repo],
        }],
        ungrouped: [],
      });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const header = container.querySelector(".groupHeader")!;
      fireEvent.contextMenu(header, { clientX: 100, clientY: 200 });
      const menuItems = container.querySelectorAll(".menu .item");
      const deleteItem = Array.from(menuItems).find(
        (el) => el.querySelector(".label")?.textContent === "Delete Group"
      )!;
      fireEvent.click(deleteItem);
      expect(mockDeleteGroup).toHaveBeenCalledWith("g1");
    });

    it("repo header right-click includes Move to Group with submenu", () => {
      const repo = makeRepo();
      setRepos({ "/repo1": repo });
      mockGetGroupedLayout.mockReturnValue({
        groups: [{
          group: { id: "g1", name: "Work", color: "", collapsed: false, repoOrder: [] },
          repos: [],
        }],
        ungrouped: [repo],
      });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const header = container.querySelector(".repoHeader")!;
      fireEvent.contextMenu(header, { clientX: 100, clientY: 200 });
      const labels = Array.from(container.querySelectorAll(".label")).map((el) => el.textContent);
      expect(labels).toContain("Move to Group");
    });

    it("no groups = flat list behavior (backward compatible)", () => {
      const repo = makeRepo();
      setRepos({ "/repo1": repo });
      // setRepos defaults to all ungrouped, which is correct here
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      // No group sections
      expect(container.querySelector(".groupSection")).toBeNull();
      // But repos still render
      expect(container.querySelectorAll(".repoSection").length).toBe(1);
    });
  });

  describe("drag-and-drop", () => {
    /** Helper: create a DataTransfer mock for drag events */
    function makeDataTransfer(): DataTransfer {
      const store: Record<string, string> = {};
      return {
        effectAllowed: "move",
        dropEffect: "move",
        setData: vi.fn((type: string, data: string) => { store[type] = data; }),
        getData: vi.fn((type: string) => store[type] ?? ""),
        setDragImage: vi.fn(),
        clearData: vi.fn(),
        items: [] as unknown,
        types: [] as unknown,
        files: [] as unknown,
      } as unknown as DataTransfer;
    }

    it("drag repo within same group reorders", () => {
      const group = { id: "g1", name: "Work", color: "", collapsed: false, repoOrder: ["/repo1", "/repo2"] };
      const repo1 = makeRepo();
      const repo2 = makeRepo({ path: "/repo2", displayName: "Repo Two", initials: "RT" });
      setRepos({ "/repo1": repo1, "/repo2": repo2 });
      mockGetGroupedLayout.mockReturnValue({
        groups: [{ group, repos: [repo1, repo2] }],
        ungrouped: [],
      });
      mockGetGroupForRepo.mockImplementation((path: string) =>
        path === "/repo1" || path === "/repo2" ? group : undefined
      );
      // Store mock needs repoOrder for the handler
      (repositoriesStore.state as any).groups = { g1: group };

      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const repoSections = container.querySelectorAll(".repoSection");
      expect(repoSections.length).toBe(2);

      const dt = makeDataTransfer();

      // Simulate drag start on repo1
      fireEvent.dragStart(repoSections[0], { dataTransfer: dt });

      // Simulate drag over repo2 (bottom half)
      const rect = { top: 0, height: 40, bottom: 40, left: 0, right: 100, width: 100, x: 0, y: 0, toJSON: () => {} };
      repoSections[1].getBoundingClientRect = () => rect as DOMRect;
      fireEvent.dragOver(repoSections[1], { dataTransfer: dt, clientY: 30 });

      // Simulate drop on repo2
      fireEvent.drop(repoSections[1], { dataTransfer: dt });

      // Should call reorderRepoInGroup for same-group reorder
      expect(mockReorderRepoInGroup).toHaveBeenCalledWith("g1", 0, 1);
    });

    it("drag repo onto different group header assigns to group", () => {
      const repo = makeRepo();
      setRepos({ "/repo1": repo });
      mockGetGroupedLayout.mockReturnValue({
        groups: [
          {
            group: { id: "g1", name: "Work", color: "", collapsed: false, repoOrder: [] },
            repos: [],
          },
          {
            group: { id: "g2", name: "Personal", color: "", collapsed: false, repoOrder: [] },
            repos: [],
          },
        ],
        ungrouped: [repo],
      });
      mockGetGroupForRepo.mockReturnValue(undefined);

      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const repoSection = container.querySelector(".repoSection")!;
      const groupHeaders = container.querySelectorAll(".groupHeader");
      expect(groupHeaders.length).toBe(2);

      const dt = makeDataTransfer();

      // Simulate drag start on repo
      fireEvent.dragStart(repoSection, { dataTransfer: dt });

      // Simulate drop on first group header
      fireEvent.dragOver(groupHeaders[0], { dataTransfer: dt });
      fireEvent.drop(groupHeaders[0], { dataTransfer: dt });

      // Should call addRepoToGroup
      expect(mockAddRepoToGroup).toHaveBeenCalledWith("/repo1", "g1");
    });

    it("drag repo from group to ungrouped area removes from group", () => {
      const repo1 = makeRepo();
      const repo2 = makeRepo({ path: "/repo2", displayName: "Repo Two", initials: "RT" });
      setRepos({ "/repo1": repo1, "/repo2": repo2 });
      mockGetGroupedLayout.mockReturnValue({
        groups: [{
          group: { id: "g1", name: "Work", color: "", collapsed: false, repoOrder: ["/repo1"] },
          repos: [repo1],
        }],
        ungrouped: [repo2],
      });
      mockGetGroupForRepo.mockImplementation((path: string) =>
        path === "/repo1"
          ? { id: "g1", name: "Work", color: "", collapsed: false, repoOrder: ["/repo1"] }
          : undefined
      );

      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const groupRepoSections = container.querySelector(".groupRepos")!.querySelectorAll(".repoSection");
      const ungroupedRepoSections = container.querySelectorAll(".repoList > .repoSection");

      expect(groupRepoSections.length).toBe(1);
      expect(ungroupedRepoSections.length).toBe(1);

      const dt = makeDataTransfer();

      // Simulate drag start on grouped repo1
      fireEvent.dragStart(groupRepoSections[0], { dataTransfer: dt });

      // Simulate drop on ungrouped repo2
      const rect = { top: 0, height: 40, bottom: 40, left: 0, right: 100, width: 100, x: 0, y: 0, toJSON: () => {} };
      ungroupedRepoSections[0].getBoundingClientRect = () => rect as DOMRect;
      fireEvent.dragOver(ungroupedRepoSections[0], { dataTransfer: dt, clientY: 30 });
      fireEvent.drop(ungroupedRepoSections[0], { dataTransfer: dt });

      // Should call removeRepoFromGroup since target is ungrouped
      expect(mockRemoveRepoFromGroup).toHaveBeenCalledWith("/repo1");
    });

    it("drag repo from group A to position in group B moves between groups", () => {
      const g1 = { id: "g1", name: "Work", color: "", collapsed: false, repoOrder: ["/repo1"] };
      const g2 = { id: "g2", name: "Personal", color: "", collapsed: false, repoOrder: ["/repo2", "/repo3"] };
      const repo1 = makeRepo();
      const repo2 = makeRepo({ path: "/repo2", displayName: "Repo Two", initials: "RT" });
      const repo3 = makeRepo({ path: "/repo3", displayName: "Repo Three", initials: "R3" });
      setRepos({ "/repo1": repo1, "/repo2": repo2, "/repo3": repo3 });
      mockGetGroupedLayout.mockReturnValue({
        groups: [
          { group: g1, repos: [repo1] },
          { group: g2, repos: [repo2, repo3] },
        ],
        ungrouped: [],
      });
      mockGetGroupForRepo.mockImplementation((path: string) => {
        if (path === "/repo1") return g1;
        if (path === "/repo2" || path === "/repo3") return g2;
        return undefined;
      });
      (repositoriesStore.state as any).groups = { g1, g2 };

      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const groupSections = container.querySelectorAll(".groupSection");
      expect(groupSections.length).toBe(2);

      const g1Repos = groupSections[0].querySelectorAll(".repoSection");
      const g2Repos = groupSections[1].querySelectorAll(".repoSection");
      expect(g1Repos.length).toBe(1);
      expect(g2Repos.length).toBe(2);

      const dt = makeDataTransfer();

      // Drag repo1 from g1
      fireEvent.dragStart(g1Repos[0], { dataTransfer: dt });

      // Drop on repo2 in g2
      fireEvent.dragOver(g2Repos[0], { dataTransfer: dt, clientY: 30 });
      fireEvent.drop(g2Repos[0], { dataTransfer: dt });

      // Should call moveRepoBetweenGroups (index 1 = after repo2, since JSDOM getBoundingClientRect returns zeros → "bottom" side)
      expect(mockMoveRepoBetweenGroups).toHaveBeenCalledWith("/repo1", "g1", "g2", 1);
    });

    it("drag group header reorders groups", () => {
      const g1 = { id: "g1", name: "Work", color: "", collapsed: false, repoOrder: ["/repo1"] };
      const g2 = { id: "g2", name: "Personal", color: "", collapsed: false, repoOrder: ["/repo2"] };
      const repo1 = makeRepo();
      const repo2 = makeRepo({ path: "/repo2", displayName: "Repo Two", initials: "RT" });
      setRepos({ "/repo1": repo1, "/repo2": repo2 });
      mockGetGroupedLayout.mockReturnValue({
        groups: [
          { group: g1, repos: [repo1] },
          { group: g2, repos: [repo2] },
        ],
        ungrouped: [],
      });
      (repositoriesStore.state as any).groupOrder = ["g1", "g2"];

      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const groupSections = container.querySelectorAll(".groupSection");
      expect(groupSections.length).toBe(2);

      const dt = makeDataTransfer();

      // Drag first group section
      fireEvent.dragStart(groupSections[0], { dataTransfer: dt });

      // Drop on second group section (bottom half)
      const rect = { top: 0, height: 60, bottom: 60, left: 0, right: 200, width: 200, x: 0, y: 0, toJSON: () => {} };
      groupSections[1].getBoundingClientRect = () => rect as DOMRect;
      fireEvent.dragOver(groupSections[1], { dataTransfer: dt, clientY: 40 });
      fireEvent.drop(groupSections[1], { dataTransfer: dt });

      expect(mockReorderGroups).toHaveBeenCalledWith(0, 1);
    });

    it("drag same repo onto itself is a no-op", () => {
      const repo = makeRepo();
      setRepos({ "/repo1": repo });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const repoSection = container.querySelector(".repoSection")!;

      const dt = makeDataTransfer();

      fireEvent.dragStart(repoSection, { dataTransfer: dt });
      const rect = { top: 0, height: 40, bottom: 40, left: 0, right: 100, width: 100, x: 0, y: 0, toJSON: () => {} };
      repoSection.getBoundingClientRect = () => rect as DOMRect;
      fireEvent.dragOver(repoSection, { dataTransfer: dt, clientY: 20 });
      fireEvent.drop(repoSection, { dataTransfer: dt });

      // No reorder actions should be called
      expect(mockReorderRepo).not.toHaveBeenCalled();
      expect(mockReorderRepoInGroup).not.toHaveBeenCalled();
      expect(mockMoveRepoBetweenGroups).not.toHaveBeenCalled();
      expect(mockAddRepoToGroup).not.toHaveBeenCalled();
      expect(mockRemoveRepoFromGroup).not.toHaveBeenCalled();
    });
  });

  describe("resize handle", () => {
    it("renders a resize handle element", () => {
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const handle = container.querySelector(".resizeHandle");
      expect(handle).not.toBeNull();
    });

    it("applies width from ui store", () => {
      uiStore.setSidebarWidth(350);
      render(() => <Sidebar {...defaultProps()} />);
      expect(document.documentElement.style.getPropertyValue("--sidebar-width")).toBe("350px");
    });

    it("clamps width to min/max via ui store", () => {
      uiStore.setSidebarWidth(100);
      render(() => <Sidebar {...defaultProps()} />);
      expect(document.documentElement.style.getPropertyValue("--sidebar-width")).toBe("200px");
    });
  });
});
