import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@solidjs/testing-library";
import "../mocks/tauri";

const { mockToggleExpanded, mockToggleCollapsed, mockGetActive, mockTerminalsGet, mockGetCheckSummary, mockGetPrStatus } = vi.hoisted(() => ({
  mockToggleExpanded: vi.fn(),
  mockToggleCollapsed: vi.fn(),
  mockGetActive: vi.fn<() => any>(() => null),
  mockTerminalsGet: vi.fn<() => any>(() => null),
  mockGetCheckSummary: vi.fn<() => any>(() => null),
  mockGetPrStatus: vi.fn<(...args: unknown[]) => unknown>(() => null),
}));

// Mock stores before importing the component
vi.mock("../../stores/repositories", () => ({
  repositoriesStore: {
    state: {
      repositories: {} as Record<string, unknown>,
    },
    getActive: mockGetActive,
    toggleExpanded: mockToggleExpanded,
    toggleCollapsed: mockToggleCollapsed,
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

import { Sidebar } from "../../components/Sidebar/Sidebar";
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
    onGitCommand: vi.fn(),
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
}

describe("Sidebar", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setRepos({});
    mockGetActive.mockReturnValue(null);
    mockTerminalsGet.mockReturnValue(null);
  });

  describe("empty state", () => {
    it("renders 'No repositories' and 'Add Repository' button when no repos exist", () => {
      const { container } = render(() => <Sidebar {...defaultProps()} />);

      const emptyDiv = container.querySelector(".sidebar-empty");
      expect(emptyDiv).not.toBeNull();
      expect(emptyDiv!.textContent).toContain("No repositories");

      const addButton = emptyDiv!.querySelector("button");
      expect(addButton).not.toBeNull();
      expect(addButton!.textContent).toBe("Add Repository");
    });

    it("calls onAddRepo when empty-state Add Repository button is clicked", () => {
      const onAddRepo = vi.fn();
      const { container } = render(() => <Sidebar {...defaultProps({ onAddRepo })} />);

      const addButton = container.querySelector(".sidebar-empty button");
      expect(addButton).not.toBeNull();
      fireEvent.click(addButton!);
      expect(onAddRepo).toHaveBeenCalledOnce();
    });
  });

  describe("footer buttons", () => {
    it("calls onAddRepo when footer Add Repository button is clicked", () => {
      const onAddRepo = vi.fn();
      const { container } = render(() => <Sidebar {...defaultProps({ onAddRepo })} />);

      const footerAddBtn = container.querySelector(".sidebar-add-repo");
      expect(footerAddBtn).not.toBeNull();
      fireEvent.click(footerAddBtn!);
      expect(onAddRepo).toHaveBeenCalled();
    });

    it("calls onOpenSettings when Settings footer button is clicked", () => {
      const onOpenSettings = vi.fn();
      const { container } = render(() => <Sidebar {...defaultProps({ onOpenSettings })} />);

      const settingsBtn = container.querySelector('.sidebar-footer-action[title="Settings"]');
      expect(settingsBtn).not.toBeNull();
      fireEvent.click(settingsBtn!);
      expect(onOpenSettings).toHaveBeenCalledOnce();
    });

    it("calls onOpenHelp when Help footer button is clicked", () => {
      const onOpenHelp = vi.fn();
      const { container } = render(() => <Sidebar {...defaultProps({ onOpenHelp })} />);

      const helpBtn = container.querySelector('.sidebar-footer-action[title="Help"]');
      expect(helpBtn).not.toBeNull();
      fireEvent.click(helpBtn!);
      expect(onOpenHelp).toHaveBeenCalledOnce();
    });

    it("renders Notifications and Tasks buttons in footer", () => {
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const notifBtn = container.querySelector('.sidebar-footer-action[title="Notifications"]');
      const tasksBtn = container.querySelector('.sidebar-footer-action[title="Tasks"]');
      expect(notifBtn).not.toBeNull();
      expect(tasksBtn).not.toBeNull();
    });
  });

  describe("with repositories", () => {
    beforeEach(() => {
      setRepos({ "/repo1": makeRepo() });
    });

    it("renders repo sections when repos exist", () => {
      const { container } = render(() => <Sidebar {...defaultProps()} />);

      const repoSections = container.querySelectorAll(".repo-section");
      expect(repoSections.length).toBe(1);

      // Should NOT show empty state
      const emptyDiv = container.querySelector(".sidebar-empty");
      expect(emptyDiv).toBeNull();
    });

    it("shows repo display name", () => {
      const { container } = render(() => <Sidebar {...defaultProps()} />);

      const repoName = container.querySelector(".repo-name");
      expect(repoName).not.toBeNull();
      expect(repoName!.textContent).toBe("Repo One");
    });

    it("renders branch items for expanded repo", () => {
      const { container } = render(() => <Sidebar {...defaultProps()} />);

      const branchItems = container.querySelectorAll(".branch-item");
      expect(branchItems.length).toBe(1);

      const branchName = container.querySelector(".branch-name");
      expect(branchName).not.toBeNull();
      expect(branchName!.textContent).toBe("main");
    });

    it("shows star icon for main branch and Y for feature branch", () => {
      setRepos({
        "/repo1": makeRepo({
          branches: {
            main: { name: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
            "feature/x": { name: "feature/x", isMain: false, worktreePath: "/wt/x", terminals: [], additions: 0, deletions: 0 },
          },
        }),
      });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const icons = container.querySelectorAll(".branch-icon");
      expect(icons.length).toBe(2);
      // Main branch first (sorted), then feature
      const mainIcon = Array.from(icons).find((i) => i.classList.contains("main"));
      const featureIcon = Array.from(icons).find((i) => i.classList.contains("feature"));
      expect(mainIcon).toBeDefined();
      expect(mainIcon!.textContent).toContain("\u2605");
      expect(featureIcon).toBeDefined();
      expect(featureIcon!.textContent).toContain("Y");
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
      const names = container.querySelectorAll(".branch-name");
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
      const names = container.querySelectorAll(".branch-name");
      expect(names.length).toBe(4);
      expect(names[0].textContent).toBe("main");
      expect(names[1].textContent).toBe("feature/active");
      // Merged/closed at bottom, alphabetically
      expect(names[2].textContent).toBe("feature/closed");
      expect(names[3].textContent).toBe("feature/merged");
    });

    it("marks active branch with active class", () => {
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const branchItem = container.querySelector(".branch-item");
      expect(branchItem).not.toBeNull();
      expect(branchItem!.classList.contains("active")).toBe(true);
    });

    it("branch item click calls onBranchSelect", () => {
      const onBranchSelect = vi.fn();
      const { container } = render(() => <Sidebar {...defaultProps({ onBranchSelect })} />);
      const branchItem = container.querySelector(".branch-item")!;
      fireEvent.click(branchItem);
      expect(onBranchSelect).toHaveBeenCalledWith("/repo1", "main");
    });

    it("add terminal button click calls onAddTerminal", () => {
      const onAddTerminal = vi.fn();
      const { container } = render(() => <Sidebar {...defaultProps({ onAddTerminal })} />);
      const addBtn = container.querySelector(".branch-add-btn")!;
      fireEvent.click(addBtn);
      expect(onAddTerminal).toHaveBeenCalledWith("/repo1", "main");
    });

    it("main branch has no remove button", () => {
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const removeBtn = container.querySelector(".branch-remove-btn");
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
      const removeBtns = container.querySelectorAll(".branch-remove-btn");
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
      const removeBtns = container.querySelectorAll(".branch-remove-btn");
      expect(removeBtns.length).toBe(0);
    });

    it("double-click branch name calls onRenameBranch", () => {
      const onRenameBranch = vi.fn();
      const { container } = render(() => <Sidebar {...defaultProps({ onRenameBranch })} />);
      const branchName = container.querySelector(".branch-name")!;
      fireEvent.dblClick(branchName);
      expect(onRenameBranch).toHaveBeenCalledWith("/repo1", "main");
    });

    it("add worktree button click calls onAddWorktree", () => {
      const onAddWorktree = vi.fn();
      const { container } = render(() => <Sidebar {...defaultProps({ onAddWorktree })} />);
      const addBtn = container.querySelector(".add-btn")!;
      fireEvent.click(addBtn);
      expect(onAddWorktree).toHaveBeenCalledWith("/repo1");
    });

    it("repo menu opens on click and shows Settings and Remove options", () => {
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      // Click the ⋯ button
      const menuBtn = container.querySelector(".repo-action-btn")!;
      fireEvent.click(menuBtn);

      const menu = container.querySelector(".context-menu");
      expect(menu).not.toBeNull();

      const menuItems = menu!.querySelectorAll(".context-menu-item");
      expect(menuItems.length).toBe(2);
      expect(menuItems[0].textContent).toContain("Repo Settings");
      expect(menuItems[1].textContent).toContain("Remove Repository");
    });

    it("repo menu Settings click calls onRepoSettings", () => {
      const onRepoSettings = vi.fn();
      const { container } = render(() => <Sidebar {...defaultProps({ onRepoSettings })} />);

      // Open menu
      const menuBtn = container.querySelector(".repo-action-btn")!;
      fireEvent.click(menuBtn);

      // Click settings
      const menuItems = container.querySelectorAll(".context-menu-item");
      fireEvent.click(menuItems[0]);
      expect(onRepoSettings).toHaveBeenCalledWith("/repo1");
    });

    it("repo menu Remove click calls onRemoveRepo", () => {
      const onRemoveRepo = vi.fn();
      const { container } = render(() => <Sidebar {...defaultProps({ onRemoveRepo })} />);

      // Open menu
      const menuBtn = container.querySelector(".repo-action-btn")!;
      fireEvent.click(menuBtn);

      // Click remove
      const menuItems = container.querySelectorAll(".context-menu-item");
      fireEvent.click(menuItems[1]);
      expect(onRemoveRepo).toHaveBeenCalledWith("/repo1");
    });

    it("repo menu closes after action", () => {
      const { container } = render(() => <Sidebar {...defaultProps()} />);

      // Open menu
      const menuBtn = container.querySelector(".repo-action-btn")!;
      fireEvent.click(menuBtn);
      expect(container.querySelector(".context-menu")).not.toBeNull();

      // Click settings to close menu
      const menuItems = container.querySelectorAll(".context-menu-item");
      fireEvent.click(menuItems[0]);

      // Menu should be closed
      expect(container.querySelector(".context-menu")).toBeNull();
    });

    it("repo menu closes on Escape", () => {
      const { container } = render(() => <Sidebar {...defaultProps()} />);

      // Open menu
      const menuBtn = container.querySelector(".repo-action-btn")!;
      fireEvent.click(menuBtn);
      expect(container.querySelector(".context-menu")).not.toBeNull();

      // Press Escape
      fireEvent.keyDown(document, { key: "Escape" });
      expect(container.querySelector(".context-menu")).toBeNull();
    });

    it("repo menu toggles on repeated clicks", () => {
      const { container } = render(() => <Sidebar {...defaultProps()} />);

      const menuBtn = container.querySelector(".repo-action-btn")!;

      // Open
      fireEvent.click(menuBtn);
      expect(container.querySelector(".context-menu")).not.toBeNull();

      // Close
      fireEvent.click(menuBtn);
      expect(container.querySelector(".context-menu")).toBeNull();
    });

    it("repo header click calls toggleExpanded", () => {
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const header = container.querySelector(".repo-header")!;
      fireEvent.click(header);
      expect(mockToggleExpanded).toHaveBeenCalledWith("/repo1");
    });

    it("shows 'No branches loaded' when repo has no branches", () => {
      setRepos({
        "/repo1": makeRepo({ branches: {} }),
      });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const empty = container.querySelector(".repo-empty");
      expect(empty).not.toBeNull();
      expect(empty!.textContent).toBe("No branches loaded");
    });

    it("renders a chevron toggle in the repo header", () => {
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const chevron = container.querySelector(".repo-chevron");
      expect(chevron).not.toBeNull();
    });

    it("chevron has expanded class when repo is expanded", () => {
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const chevron = container.querySelector(".repo-chevron");
      expect(chevron!.classList.contains("expanded")).toBe(true);
    });

    it("chevron does not have expanded class when repo is not expanded", () => {
      setRepos({
        "/repo1": makeRepo({ expanded: false }),
      });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const chevron = container.querySelector(".repo-chevron");
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

      const initials = container.querySelector(".repo-initials");
      expect(initials).not.toBeNull();
      expect(initials!.textContent).toBe("RO");

      // Should not show repo name or branches
      const repoName = container.querySelector(".repo-name");
      expect(repoName).toBeNull();

      const branchItems = container.querySelectorAll(".branch-item");
      expect(branchItems.length).toBe(0);
    });

    it("shows collapsed class on repo section", () => {
      setRepos({
        "/repo1": makeRepo({ collapsed: true }),
      });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const section = container.querySelector(".repo-section");
      expect(section).not.toBeNull();
      expect(section!.classList.contains("collapsed")).toBe(true);
    });

    it("initials click calls toggleCollapsed", () => {
      setRepos({
        "/repo1": makeRepo({ collapsed: true }),
      });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const initials = container.querySelector(".repo-initials")!;
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
      const branchItems = container.querySelectorAll(".branch-item");
      expect(branchItems.length).toBe(0);
      // But should still show repo name
      expect(container.querySelector(".repo-name")!.textContent).toBe("Repo One");
    });
  });

  describe("Git Quick Actions", () => {
    it("shows git quick actions when getActive returns a repo", () => {
      mockGetActive.mockReturnValue({ path: "/repo1" });
      setRepos({ "/repo1": makeRepo() });
      const { container } = render(() => <Sidebar {...defaultProps()} />);

      const quickActions = container.querySelector(".git-quick-actions");
      expect(quickActions).not.toBeNull();

      const title = container.querySelector(".git-quick-actions-title");
      expect(title!.textContent).toBe("Git");

      const buttons = container.querySelectorAll(".git-quick-btn");
      expect(buttons.length).toBe(4);
    });

    it("does not show git quick actions when getActive returns null", () => {
      mockGetActive.mockReturnValue(null);
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const quickActions = container.querySelector(".git-quick-actions");
      expect(quickActions).toBeNull();
    });

    it("Pull button calls onGitCommand with pull command", () => {
      mockGetActive.mockReturnValue({ path: "/repo1" });
      setRepos({ "/repo1": makeRepo() });
      const onGitCommand = vi.fn();
      const { container } = render(() => <Sidebar {...defaultProps({ onGitCommand })} />);

      const buttons = container.querySelectorAll(".git-quick-btn");
      const pullBtn = Array.from(buttons).find((b) => b.textContent?.includes("Pull"))!;
      fireEvent.click(pullBtn);
      expect(onGitCommand).toHaveBeenCalledWith("cd /repo1 && git pull");
    });

    it("Push button calls onGitCommand with push command", () => {
      mockGetActive.mockReturnValue({ path: "/repo1" });
      setRepos({ "/repo1": makeRepo() });
      const onGitCommand = vi.fn();
      const { container } = render(() => <Sidebar {...defaultProps({ onGitCommand })} />);

      const buttons = container.querySelectorAll(".git-quick-btn");
      const pushBtn = Array.from(buttons).find((b) => b.textContent?.includes("Push"))!;
      fireEvent.click(pushBtn);
      expect(onGitCommand).toHaveBeenCalledWith("cd /repo1 && git push");
    });

    it("Fetch button calls onGitCommand with fetch command", () => {
      mockGetActive.mockReturnValue({ path: "/repo1" });
      setRepos({ "/repo1": makeRepo() });
      const onGitCommand = vi.fn();
      const { container } = render(() => <Sidebar {...defaultProps({ onGitCommand })} />);

      const buttons = container.querySelectorAll(".git-quick-btn");
      const fetchBtn = Array.from(buttons).find((b) => b.textContent?.includes("Fetch"))!;
      fireEvent.click(fetchBtn);
      expect(onGitCommand).toHaveBeenCalledWith("cd /repo1 && git fetch --all");
    });

    it("Stash button calls onGitCommand with stash command", () => {
      mockGetActive.mockReturnValue({ path: "/repo1" });
      setRepos({ "/repo1": makeRepo() });
      const onGitCommand = vi.fn();
      const { container } = render(() => <Sidebar {...defaultProps({ onGitCommand })} />);

      const buttons = container.querySelectorAll(".git-quick-btn");
      const stashBtn = Array.from(buttons).find((b) => b.textContent?.includes("Stash"))!;
      fireEvent.click(stashBtn);
      expect(onGitCommand).toHaveBeenCalledWith("cd /repo1 && git stash");
    });
  });

  describe("quick switcher mode", () => {
    it("shows shortcut keys instead of action buttons in quick switcher mode", () => {
      setRepos({ "/repo1": makeRepo() });
      const { container } = render(() => <Sidebar {...defaultProps({ quickSwitcherActive: true })} />);

      const shortcut = container.querySelector(".branch-shortcut");
      expect(shortcut).not.toBeNull();
      expect(shortcut!.textContent).toContain("1");

      // Should not show add/remove buttons
      const addBtn = container.querySelector(".branch-add-btn");
      expect(addBtn).toBeNull();
    });

    it("forces branches visible in quick switcher even when not expanded", () => {
      setRepos({ "/repo1": makeRepo({ expanded: false }) });
      const { container } = render(() => <Sidebar {...defaultProps({ quickSwitcherActive: true })} />);

      const branchItems = container.querySelectorAll(".branch-item");
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
      const stats = container.querySelector(".branch-stats");
      expect(stats).not.toBeNull();
      const addStat = container.querySelector(".stat-add");
      const delStat = container.querySelector(".stat-del");
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
      const stats = container.querySelector(".branch-stats");
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
      const stats = container.querySelector(".branch-stats");
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
      const stats = container.querySelector(".branch-stats");
      expect(stats).not.toBeNull();
    });

    it("shows PrBadgeSidebar when GitHub store has PR data for branch", () => {
      setRepos({
        "/repo1": makeRepo({
          branches: {
            main: { name: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
          },
        }),
      });
      mockGetPrStatus.mockReturnValue({ state: "OPEN", number: 123, title: "Test PR", url: "https://example.com" });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const prBadge = container.querySelector(".branch-pr-badge");
      expect(prBadge).not.toBeNull();
      expect(prBadge!.textContent).toBe("#123");
      expect(prBadge!.getAttribute("title")).toBe("PR #123");
    });

    it("does not show PrBadgeSidebar when branch has no prNumber", () => {
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const prBadge = container.querySelector(".branch-pr-badge");
      expect(prBadge).toBeNull();
    });

    it("applies merged class to PR badge when PR state is MERGED", () => {
      setRepos({
        "/repo1": makeRepo({
          branches: {
            main: { name: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
          },
        }),
      });
      mockGetPrStatus.mockReturnValue({ state: "MERGED", number: 42, title: "Test", url: "https://example.com" });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const prBadge = container.querySelector(".branch-pr-badge");
      expect(prBadge).not.toBeNull();
      expect(prBadge!.classList.contains("merged")).toBe(true);
    });

    it("applies closed class to PR badge when PR state is CLOSED", () => {
      setRepos({
        "/repo1": makeRepo({
          branches: {
            main: { name: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
          },
        }),
      });
      mockGetPrStatus.mockReturnValue({ state: "CLOSED", number: 43, title: "Test", url: "https://example.com" });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const prBadge = container.querySelector(".branch-pr-badge");
      expect(prBadge).not.toBeNull();
      expect(prBadge!.classList.contains("closed")).toBe(true);
    });

    it("applies draft class to PR badge when PR is a draft", () => {
      setRepos({
        "/repo1": makeRepo({
          branches: {
            main: { name: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
          },
        }),
      });
      mockGetPrStatus.mockReturnValue({ state: "OPEN", number: 45, title: "Draft PR", url: "https://example.com", is_draft: true });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const prBadge = container.querySelector(".branch-pr-badge");
      expect(prBadge).not.toBeNull();
      expect(prBadge!.classList.contains("draft")).toBe(true);
    });

    it("does not apply state class when PR state is OPEN", () => {
      setRepos({
        "/repo1": makeRepo({
          branches: {
            main: { name: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
          },
        }),
      });
      mockGetPrStatus.mockReturnValue({ state: "OPEN", number: 44, title: "Test", url: "https://example.com" });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const prBadge = container.querySelector(".branch-pr-badge");
      expect(prBadge).not.toBeNull();
      expect(prBadge!.classList.contains("merged")).toBe(false);
      expect(prBadge!.classList.contains("closed")).toBe(false);
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
      const branchItem = container.querySelector(".branch-item");
      expect(branchItem!.classList.contains("has-activity")).toBe(true);
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
      const branchItem = container.querySelector(".branch-item");
      expect(branchItem!.classList.contains("has-activity")).toBe(false);
    });
  });

  describe("multiple repos", () => {
    it("renders multiple repo sections", () => {
      setRepos({
        "/repo1": makeRepo(),
        "/repo2": makeRepo({ path: "/repo2", displayName: "Repo Two", initials: "RT" }),
      });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const repoSections = container.querySelectorAll(".repo-section");
      expect(repoSections.length).toBe(2);
    });
  });

  describe("click outside repo menu", () => {
    it("closes repo menu when clicking outside", () => {
      setRepos({ "/repo1": makeRepo() });
      const { container } = render(() => <Sidebar {...defaultProps()} />);

      // Open menu
      const menuBtn = container.querySelector(".repo-action-btn")!;
      fireEvent.click(menuBtn);
      expect(container.querySelector(".context-menu")).not.toBeNull();

      // Click somewhere outside the menu (on the sidebar itself)
      fireEvent.mouseDown(container.querySelector("#sidebar")!);
      expect(container.querySelector(".context-menu")).toBeNull();
    });
  });

  describe("context menu", () => {
    it("opens context menu on right-click of branch item", () => {
      setRepos({ "/repo1": makeRepo() });
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const branchItem = container.querySelector(".branch-item")!;
      fireEvent.contextMenu(branchItem, { clientX: 100, clientY: 200 });

      const contextMenu = container.querySelector(".context-menu");
      expect(contextMenu).not.toBeNull();

      const items = contextMenu!.querySelectorAll(".context-menu-item");
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
      const branchItem = container.querySelector(".branch-item")!;
      fireEvent.contextMenu(branchItem, { clientX: 100, clientY: 200 });

      const contextMenu = container.querySelector(".context-menu");
      const items = contextMenu!.querySelectorAll(".context-menu-item");
      // Find "Copy Path" item
      const copyPathItem = Array.from(items).find((i) =>
        i.querySelector(".context-menu-label")?.textContent === "Copy Path"
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
      const branchItem = container.querySelector(".branch-item")!;
      fireEvent.contextMenu(branchItem, { clientX: 100, clientY: 200 });

      const contextMenu = container.querySelector(".context-menu");
      const items = contextMenu!.querySelectorAll(".context-menu-item");
      const copyPathItem = Array.from(items).find((i) =>
        i.querySelector(".context-menu-label")?.textContent === "Copy Path"
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
      const branchItems = container.querySelectorAll(".branch-item");
      fireEvent.contextMenu(branchItems[1], { clientX: 100, clientY: 200 });

      const contextMenu = container.querySelector(".context-menu");
      const items = contextMenu!.querySelectorAll(".context-menu-item");
      // Copy Path, Add Terminal, Rename Branch (NO Delete Worktree)
      expect(items.length).toBe(3);
      const labels = Array.from(items).map((i) => i.querySelector(".context-menu-label")!.textContent);
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
      const branchItems = container.querySelectorAll(".branch-item");
      // feature/x is second (sorted after main)
      fireEvent.contextMenu(branchItems[1], { clientX: 100, clientY: 200 });

      const contextMenu = container.querySelector(".context-menu");
      const items = contextMenu!.querySelectorAll(".context-menu-item");
      // Copy Path, Add Terminal, Rename Branch, Delete Worktree
      expect(items.length).toBe(4);
      const labels = Array.from(items).map((i) => i.querySelector(".context-menu-label")!.textContent);
      expect(labels).toContain("Delete Worktree");
    });
  });

  describe("resize handle", () => {
    it("renders a resize handle element", () => {
      const { container } = render(() => <Sidebar {...defaultProps()} />);
      const handle = container.querySelector(".sidebar-resize-handle");
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
