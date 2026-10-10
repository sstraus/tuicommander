import { readFileSync } from "node:fs";
import "@testing-library/jest-dom/vitest";
import { fireEvent, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mockInvoke } from "../mocks/tauri";

const {
	mockToggleExpanded,
	mockToggleCollapsed,
	mockGetActive,
	mockGetOrderedRepos,
	mockReorderRepo,
	mockTerminalsGet,
	mockGetSubAgentTag,
	mockGetCheckSummary,
	mockGetPrStatus,
	mockGetGroupedLayout,
	mockGetGroupForRepo,
	mockToggleGroupCollapsed,
	mockDeleteGroup,
	mockAddRepoToGroup,
	mockRemoveRepoFromGroup,
	mockCreateGroup,
	mockReorderRepoInGroup,
	mockMoveRepoBetweenGroups,
	mockReorderGroups,
	mockToggleBranchTabsCollapsed,
	mockNavigateToTerminal,
	mockOpenUrl,
} = vi.hoisted(() => ({
	mockToggleExpanded: vi.fn(),
	mockToggleCollapsed: vi.fn(),
	mockGetActive: vi.fn<() => unknown>(() => null),
	mockGetOrderedRepos: vi.fn<() => unknown[]>(() => []),
	mockReorderRepo: vi.fn(),
	mockTerminalsGet: vi.fn<(id: string) => unknown>(() => null),
	mockGetSubAgentTag: vi.fn<(id: string) => string | null>(() => null),
	mockGetCheckSummary: vi.fn<() => unknown>(() => null),
	mockGetPrStatus: vi.fn<(...args: unknown[]) => unknown>(() => null),
	mockGetGroupedLayout: vi.fn<() => unknown>(() => ({ groups: [], ungrouped: [] })),
	mockGetGroupForRepo: vi.fn<(path: string) => unknown>(() => undefined),
	mockToggleGroupCollapsed: vi.fn(),
	mockDeleteGroup: vi.fn(),
	mockAddRepoToGroup: vi.fn(),
	mockRemoveRepoFromGroup: vi.fn(),
	mockCreateGroup: vi.fn(() => "new-group-id"),
	mockReorderRepoInGroup: vi.fn(),
	mockMoveRepoBetweenGroups: vi.fn(),
	mockReorderGroups: vi.fn(),
	mockToggleBranchTabsCollapsed: vi.fn(),
	mockNavigateToTerminal: vi.fn(),
	mockOpenUrl: vi.fn(),
}));

vi.mock("../../utils/openUrl", () => ({ handleOpenUrl: mockOpenUrl }));

// Mock stores before importing the component
vi.mock("../../stores/repositories", () => ({
	repositoriesStore: {
		state: {
			repositories: {} as Record<string, unknown>,
			repoOrder: [] as string[],
			activeRepoPath: null as string | null,
			groups: {} as Record<string, unknown>,
			groupOrder: [] as string[],
			staleTempCandidates: [] as Array<{ path: string; displayName: string }>,
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
		setParkGroup: vi.fn(),
		isGroupFullyParked: vi.fn(() => false),
		get: vi.fn(() => undefined),
		setActive: vi.fn(),
		toggleWorkspaceTabsCollapsed: mockToggleBranchTabsCollapsed,
	},
}));

vi.mock("../../stores/repoSettings", () => ({
	repoSettingsStore: {
		get: vi.fn(() => undefined),
		getEffective: vi.fn(() => undefined),
		getEffectiveField: vi.fn(() => undefined),
		setLabel: vi.fn(),
	},
}));

vi.mock("../../stores/terminals", () => ({
	terminalsStore: {
		get: mockTerminalsGet,
		getSubAgentTag: mockGetSubAgentTag,
		isBusy: vi.fn(() => false),
		onRemove: vi.fn(() => () => {}),
		state: { activeId: null as string | null },
	},
}));

vi.mock("../../utils/navigateToTerminal", () => ({
	navigateToTerminal: mockNavigateToTerminal,
}));

vi.mock("../../stores/github", () => ({
	githubStore: {
		getCheckSummary: mockGetCheckSummary,
		getPrStatus: mockGetPrStatus,
		getRemoteOnlyPrs: vi.fn(() => []),
		getRepoIssues: vi.fn(() => []),
		getAllOpenPrs: vi.fn(() => []),
		getRemoteStatus: vi.fn(() => null),
		getLastPolled: vi.fn(() => 0),
		state: { viewerLogin: null, issuesLoading: false, circuitBreakerOpen: false },
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

// Mock PrDetailPopover to avoid cascading store dependencies
vi.mock("../../components/PrDetailPopover/PrDetailPopover", () => ({
	PrDetailPopover: (props: { repoPath: string; branch: string; onClose: () => void }) => (
		<div data-testid="pr-detail-popover" data-branch={props.branch} data-repo={props.repoPath} />
	),
}));

import { _resetMergedActivityAccum } from "../../components/Sidebar/RepoSection";
import { Sidebar } from "../../components/Sidebar/Sidebar";
import { appLogger } from "../../stores/appLogger";
import { githubStore } from "../../stores/github";
import { progressStore } from "../../stores/progress";
import { repositoriesStore } from "../../stores/repositories";
import { settingsStore } from "../../stores/settings";
import { sidebarPluginStore } from "../../stores/sidebarPluginStore";
import { terminalsStore } from "../../stores/terminals";
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
function makeRepo(overrides: Record<string, unknown> = {}) {
	return {
		path: "/repo1",
		displayName: "Repo One",
		initials: "RO",
		expanded: true,
		collapsed: false,
		activeWorkspaceId: "main",
		workspaces: {
			main: {
				workspaceId: "main",
				branchName: "main",
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
function setRepos(repos: Record<string, unknown>, activeRepoPath?: string) {
	(repositoriesStore.state as { repositories: Record<string, unknown> }).repositories = repos;
	(repositoriesStore.state as { activeRepoPath: string | null }).activeRepoPath =
		activeRepoPath ?? Object.keys(repos)[0] ?? null;
	const repoValues = Object.values(repos);
	mockGetOrderedRepos.mockReturnValue(repoValues);
	// Default grouped layout: all repos ungrouped (backward compatible)
	mockGetGroupedLayout.mockReturnValue({ groups: [], ungrouped: repoValues });
}

// Legacy row interaction cases exercise compact mode; rich layout cases opt into auto.
function densityMode(mode: "auto" | "compact") {
	while (uiStore.state.sidebarDensityMode !== mode) uiStore.cycleSidebarDensityMode();
}

describe("Sidebar", () => {
	beforeEach(() => {
		vi.useFakeTimers();
		densityMode("compact");
		vi.clearAllMocks();
		setRepos({});
		mockGetActive.mockReturnValue(null);
		mockGetGroupForRepo.mockReturnValue(undefined);
		mockTerminalsGet.mockReturnValue(null);
		mockGetSubAgentTag.mockReturnValue(null);
		mockGetCheckSummary.mockReturnValue(null);
		mockGetPrStatus.mockReturnValue(null);
		mockLastActivityAt.mockReturnValue(0);
		// Nested terminal tabs are opt-in and off by default — reset per test so the
		// feature-behavior block can enable it and the gating block can rely on off.
		settingsStore.setTabTreeEnabled(false);
		_resetMergedActivityAccum();
	});

	afterEach(() => {
		densityMode("auto");
		vi.useRealTimers();
	});

	describe("touch swipe actions", () => {
		const setup = (overrides: Partial<Parameters<typeof Sidebar>[0]> = {}) => {
			const branch = (id: string, path: string | null, isMain = false) => ({
				workspaceId: id,
				branchName: id,
				worktreePath: path,
				isMain,
				terminals: [],
				additions: 0,
				deletions: 0,
			});
			setRepos({
				"/repo1": makeRepo({
					workspaces: {
						main: branch("main", "/repo1", true),
						feat: branch("feat", "/repo1-feat"),
						other: branch("other", "/repo1-other"),
						root: branch("root", "/repo1"),
						plain: branch("plain", null),
					},
				}),
			});
			return render(() => <Sidebar {...defaultProps(overrides)} />);
		};
		const row = (container: HTMLElement, id: string) =>
			Array.from(container.querySelectorAll<HTMLElement>("[data-swipe-row]")).find(
				(el) => el.dataset.swipeRow === JSON.stringify(["/repo1", id]),
			)!;
		const pointer = (el: Element, type: string, x: number, y: number, pointerType = "touch") => {
			const event = new Event(type, { bubbles: true, cancelable: true });
			Object.assign(event, { pointerType, pointerId: 1, clientX: x, clientY: y, button: 0 });
			fireEvent(el, event);
		};
		const swipe = (el: Element, dx = -60, dy = 0, kind = "touch") => {
			pointer(el, "pointerdown", 100, 50, kind);
			pointer(el, "pointermove", 100 + dx, 50 + dy, kind);
			pointer(el, "pointerup", 100 + dx, 50 + dy, kind);
		};

		it("touch_swipe_reveals_actions", () => {
			const onBranchSelect = vi.fn();
			const { container } = setup({ onBranchSelect });
			const target = row(container, "feat");
			expect(target.querySelector(".branchSwipeActions")).toBeNull();
			swipe(target);
			expect(target.querySelector(".branchSwipeActions button[aria-label='Branch options']")).not.toBeNull();
			expect(target.querySelector(".branchSwipeActions button[aria-label='Add terminal']")).not.toBeNull();
			fireEvent.click(target.querySelector(".branchItem")!);
			expect(target.querySelector(".branchSwipeActions")).not.toBeNull();
			expect(onBranchSelect).not.toHaveBeenCalled();
		});

		it("short_swipes_and_cancelled_vertical_gestures_do_not_open_actions", () => {
			// Catches: jitter or a browser-cancelled scroll revealing destructive actions.
			const { container } = setup();
			const target = row(container, "feat");
			swipe(target, -29);
			expect(target.querySelector(".branchSwipeActions")).toBeNull();
			pointer(target, "pointerdown", 100, 50);
			pointer(target, "pointercancel", 100, 50);
			pointer(target, "pointermove", 30, 50);
			expect(target.querySelector(".branchSwipeActions")).toBeNull();
		});

		it("vertical_drag_keeps_actions_closed", () => {
			const { container } = setup();
			const target = row(container, "feat");
			swipe(target, -15, 60);
			expect(target.querySelector(".branchSwipeActions")).toBeNull();
			pointer(target, "pointerdown", 100, 50);
			pointer(target, "pointermove", 98, 75);
			pointer(target, "pointermove", 30, 80);
			expect(target.querySelector(".branchSwipeActions")).toBeNull();
		});

		it("delete_only_removable_worktrees", () => {
			const onRemoveBranch = vi.fn();
			const { container } = setup({ onRemoveBranch });
			for (const id of ["main", "root", "plain"]) {
				swipe(row(container, id));
				expect(row(container, id).querySelector(".branchSwipeActions .branchRemoveBtn")).toBeNull();
			}
			const target = row(container, "feat");
			swipe(target);
			fireEvent.click(target.querySelector(".branchSwipeActions .branchRemoveBtn")!);
			expect(onRemoveBranch).toHaveBeenCalledExactlyOnceWith("/repo1", "feat");
		});

		it("second_row_closes_first", () => {
			const { container } = setup();
			swipe(row(container, "feat"));
			swipe(row(container, "other"));
			expect(row(container, "feat").querySelector(".branchSwipeActions")).toBeNull();
			expect(row(container, "other").querySelector(".branchSwipeActions")).not.toBeNull();
		});

		it("mouse_pointer_never_swipes", () => {
			const { container } = setup();
			swipe(row(container, "feat"), -60, 0, "mouse");
			expect(container.querySelector(".branchSwipeActions")).toBeNull();
		});

		it("right_swipe_row_tap_and_outside_tap_dismiss_actions", () => {
			// Catches: a revealed tray trapping the row open after dismissal gestures.
			const { container } = setup();
			const target = row(container, "feat");
			swipe(target);
			swipe(target, 60);
			expect(target.querySelector(".branchSwipeActions")).toBeNull();
			swipe(target);
			pointer(target, "pointerdown", 100, 50);
			pointer(target, "pointerup", 100, 50);
			fireEvent.click(target.querySelector(".branchItem")!);
			expect(target.querySelector(".branchSwipeActions")).toBeNull();
			swipe(target);
			pointer(document.body, "pointerdown", 0, 0);
			expect(target.querySelector(".branchSwipeActions")).toBeNull();
		});

		it("tray_add_and_long_press_keep_existing_handlers", () => {
			// Catches: the swipe tray losing add-terminal or long-press agent launch.
			const onAddTerminal = vi.fn();
			const launch = vi.fn();
			const { container } = setup({
				onAddTerminal,
				buildAgentMenuItems: () => [{ label: "Launch test agent", action: launch }],
			});
			const target = row(container, "feat");
			swipe(target);
			fireEvent.click(target.querySelector(".branchSwipeActions .branchAddBtn")!);
			expect(onAddTerminal).toHaveBeenCalledExactlyOnceWith("/repo1", "feat");
			swipe(target);
			const add = target.querySelector(".branchSwipeActions .branchAddBtn")!;
			pointer(add, "pointerdown", 100, 50);
			vi.advanceTimersByTime(500);
			pointer(add, "pointerup", 100, 50);
			fireEvent.mouseDown(add);
			fireEvent.click(add);
			expect(onAddTerminal).toHaveBeenCalledTimes(1);
			const menuItem = Array.from(document.querySelectorAll(".menu .item")).find((el) =>
				el.textContent?.includes("Launch test agent"),
			);
			expect(menuItem).toBeDefined();
			fireEvent.click(menuItem!);
			expect(launch).toHaveBeenCalledOnce();
		});

		it("removing_tray_delete_stays_disabled", () => {
			// Catches: the touch tray enabling deletion during an ongoing removal.
			const onRemoveBranch = vi.fn();
			const { container } = setup({ onRemoveBranch, removingBranches: new Set(["/repo1::feat"]) });
			swipe(row(container, "feat"));
			const remove = row(container, "feat").querySelector<HTMLButtonElement>(".branchSwipeActions .branchRemoveBtn")!;
			expect(remove.disabled).toBe(true);
			fireEvent.click(remove);
			expect(onRemoveBranch).not.toHaveBeenCalled();
		});
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

	describe("density attribute", () => {
		beforeEach(() => densityMode("auto"));
		const manyBranches = (n: number) =>
			Object.fromEntries(
				Array.from({ length: n }, (_, i) => [
					`w${i}`,
					{
						workspaceId: `w${i}`,
						branchName: `b${i}`,
						isMain: i === 0,
						worktreePath: null,
						terminals: [],
						additions: 0,
						deletions: 0,
					},
				]),
			);
		const aside = (container: HTMLElement) => container.querySelector("aside#sidebar") as HTMLElement;
		const originalMatchMedia = window.matchMedia;
		afterEach(() => {
			window.matchMedia = originalMatchMedia;
		});

		// Catches: the density never reaching the <aside> the CSS keys on (derived but not wired).
		it("puts a short list on the aside as rich", () => {
			setRepos({ "/repo1": makeRepo({ workspaces: manyBranches(3) }) });
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			expect(aside(container).dataset.density).toBe("rich");
		});

		// Catches: a long list still getting the roomy rows (row budget not read from the layout).
		it("puts a long list on the aside as compact", () => {
			setRepos({ "/repo1": makeRepo({ workspaces: manyBranches(20) }) });
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			expect(aside(container).dataset.density).toBe("compact");
		});

		// Catches: plugin panel rows (rendered under every open repo) left out of the row budget.
		it("counts an expanded plugin panel with many items toward the budget", () => {
			const handle = sidebarPluginStore.registerPanel("density-test", { id: "p", label: "Panel", collapsed: false });
			handle.setItems(Array.from({ length: 20 }, (_, i) => ({ id: `i${i}`, label: `item ${i}` })));
			try {
				setRepos({ "/repo1": makeRepo({ workspaces: manyBranches(3) }) });
				const { container } = render(() => <Sidebar {...defaultProps()} />);
				expect(aside(container).dataset.density).toBe("compact");
			} finally {
				sidebarPluginStore.clearPlugin("density-test");
			}
		});

		// Catches: a coarse pointer ignored by the component (only the pure function handles it).
		it("puts rich on the aside for a coarse pointer", () => {
			window.matchMedia = vi.fn().mockReturnValue({
				matches: true,
				addEventListener: vi.fn(),
				removeEventListener: vi.fn(),
			});
			setRepos({ "/repo1": makeRepo({ workspaces: manyBranches(20) }) });
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			expect(aside(container).dataset.density).toBe("rich");
		});
	});

	describe("touch controls (1334-b659)", () => {
		const originalMatchMedia = window.matchMedia;
		const touch = (on: boolean) => {
			window.matchMedia = vi.fn().mockImplementation((query: string) => ({
				matches: on && query === "(hover: none)",
				addEventListener: vi.fn(),
				removeEventListener: vi.fn(),
			}));
		};
		afterEach(() => {
			window.matchMedia = originalMatchMedia;
		});
		const remoteRepo = () => setRepos({ "/repo1": makeRepo({ connectionId: "conn-1" }) });

		// Catches: an offline remote badge that does nothing on touch, where no tooltip explains it.
		it("remote badge tap opens Remote Machines on touch and does not toggle the repo", () => {
			touch(true);
			remoteRepo();
			const onOpenRemoteMachines = vi.fn();
			const { container } = render(() => <Sidebar {...defaultProps({ onOpenRemoteMachines })} />);
			fireEvent.click(container.querySelector(".remoteBadge")!);
			expect(onOpenRemoteMachines).toHaveBeenCalledTimes(1);
			expect(mockToggleExpanded).not.toHaveBeenCalled();
		});

		// Catches: the touch-only behaviour leaking to a hover device, where the badge click keeps toggling the repo.
		it("remote badge click keeps toggling the repo on a hover device", () => {
			touch(false);
			remoteRepo();
			const onOpenRemoteMachines = vi.fn();
			const { container } = render(() => <Sidebar {...defaultProps({ onOpenRemoteMachines })} />);
			fireEvent.click(container.querySelector(".remoteBadge")!);
			expect(onOpenRemoteMachines).not.toHaveBeenCalled();
			expect(mockToggleExpanded).toHaveBeenCalledWith("/repo1");
		});

		// Catches: no way to reach the branch menu on touch (long press fires no contextmenu on iPadOS),
		// or the ⋯ click also selecting the branch.
		it("branch ⋯ button opens the branch menu without selecting the branch", () => {
			setRepos({
				"/repo1": makeRepo({
					workspaces: {
						b: {
							workspaceId: "b",
							branchName: "feature/x",
							isMain: false,
							worktreePath: "/wt/feature-x",
							terminals: [],
							additions: 0,
							deletions: 0,
						},
					},
				}),
			});
			const onBranchSelect = vi.fn();
			const { container } = render(() => <Sidebar {...defaultProps({ onBranchSelect })} />);
			const row = Array.from(container.querySelectorAll(".branchItem")).find((el) =>
				el.textContent?.includes("feature/x"),
			)!;
			fireEvent.click(row.querySelector(".branchMoreBtn")!);
			expect(
				Array.from(container.querySelectorAll(".menu .item")).some((el) => el.textContent?.includes("Copy Path")),
			).toBe(true);
			expect(onBranchSelect).not.toHaveBeenCalled();
		});
	});

	describe("rich repo sections", () => {
		beforeEach(() => densityMode("auto"));
		// Catches: treating idle open terminals as inactive, sorting by work, or persisting the partition.
		it("rich_repo_sections_use_open_terminals", () => {
			const [repos, setReposLive] = createSignal([
				makeRepo({ path: "/idle", displayName: "Idle repo" }),
				makeRepo({
					path: "/open",
					displayName: "Open repo",
					workspaces: {
						main: {
							workspaceId: "main",
							branchName: "main",
							isMain: true,
							worktreePath: null,
							terminals: ["t1"],
							additions: 0,
							deletions: 0,
						},
					},
				}),
				makeRepo({ path: "/last", displayName: "Last idle" }),
			]);
			mockGetOrderedRepos.mockImplementation(repos);
			mockGetGroupedLayout.mockImplementation(() => ({ groups: [], ungrouped: repos() }));
			const [state, setState] = createSignal("idle");
			mockTerminalsGet.mockImplementation(() => ({
				id: "t1",
				agentType: "codex",
				agentState: state(),
				shellState: "idle",
			}));
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const paths = () =>
				[...container.querySelectorAll("[data-sidebar-repo]")].map((e) => e.getAttribute("data-sidebar-repo"));
			expect(paths()).toEqual(["/open", "/idle", "/last"]);
			expect(container.querySelectorAll("[data-testid='idle-repos-heading']")).toHaveLength(1);
			expect(container.querySelector("[data-sidebar-repo='/idle'] .branchItem")).toBeNull();
			fireEvent.click(container.querySelector("[data-sidebar-repo='/idle'] .repoHeader")!);
			expect(container.querySelector("[data-sidebar-repo='/idle'] .branchItem")).not.toBeNull();
			expect(mockToggleExpanded).not.toHaveBeenCalled();
			expect(paths()).toEqual(["/open", "/idle", "/last"]);
			expect(container.querySelector("[data-testid='working-agent-count']")).toBeNull();
			setState("working");
			expect(container.querySelector("[data-testid='working-agent-count']")?.textContent).toBe("1 working");
			setState("awaiting_input");
			expect(container.querySelector("[data-testid='working-agent-count']")).toBeNull();
			setReposLive(
				repos().map((repo) => (repo.path === "/open" ? makeRepo({ path: "/open", displayName: "Open repo" }) : repo)),
			);
			expect(paths()).toEqual(["/idle", "/open", "/last"]);
			expect(mockReorderRepo).not.toHaveBeenCalled();
			uiStore.setRepoFilterActiveOnly(true);
			try {
				expect(container.querySelector("[data-testid='idle-repos-heading']")).toBeNull();
			} finally {
				uiStore.setRepoFilterActiveOnly(false);
			}
		});
	});

	describe("rich layout", () => {
		beforeEach(() => densityMode("auto"));
		const richBranch = (over: Record<string, unknown> = {}) => ({
			workspaceId: "feat",
			branchName: "feat",
			isMain: false,
			worktreePath: "/wt/feat",
			terminals: ["fixture-open"],
			additions: 12,
			deletions: 3,
			isMerged: false,
			lastCommitTs: Date.now() - 3 * 3600_000,
			lifecycleStatus: { dirtyFiles: 4, commitStatus: "unmerged" as const, removalSafety: "requires_force" as const },
			...over,
		});
		const withBranch = (b: Record<string, unknown>) => setRepos({ "/repo1": makeRepo({ workspaces: { feat: b } }) });

		// Catches: the rich row printing nothing beyond the name (the bare list Boss rejected).
		it("prints the PR title, commit age, diff stats and dirty count under the branch name", () => {
			mockGetPrStatus.mockReturnValue({ state: "OPEN", number: 77, title: "Add the thing", url: "u" });
			withBranch(richBranch());
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const row = container.querySelector(".branchItem") as HTMLElement;
			expect(row.querySelector(".branchRichLine")?.textContent).toBe("Add the thing");
			const meta = row.querySelector(".branchRichMeta")?.textContent ?? "";
			expect(meta).toContain("3h");
			expect(meta).toContain("+12");
			expect(meta).toContain("4 dirty");
		});

		// Catches: closed-unmerged or live worktrees offered as clean, and bypassing the removal workflow.
		it("merged_summary_does_not_hide_unsafe_worktrees", () => {
			const safe = richBranch({
				terminals: [],
				isMerged: true,
				lifecycleStatus: { dirtyFiles: 35, commitStatus: "merged", removalSafety: "destructive" },
			});
			const live = richBranch({
				workspaceId: "live",
				branchName: "live",
				isMerged: true,
				lifecycleStatus: { dirtyFiles: 0, commitStatus: "merged", removalSafety: "safe" },
			});
			const unmerged = richBranch({ workspaceId: "closed", branchName: "closed", terminals: [], isMerged: true });
			setRepos({ "/repo1": makeRepo({ workspaces: { feat: safe, live, closed: unmerged } }) });
			mockGetPrStatus.mockReturnValue({ state: "CLOSED", number: 1, title: "Closed", url: "" });
			const onRemoveBranch = vi.fn();
			const { container } = render(() => <Sidebar {...defaultProps({ onRemoveBranch })} />);
			const summaries = container.querySelectorAll("[data-testid='merged-worktree-summary']");
			expect(summaries).toHaveLength(1);
			expect(summaries[0].textContent).toContain("35 uncommitted");
			expect(container.querySelectorAll(".branchRichDetail")).toHaveLength(2);
			fireEvent.click(summaries[0].querySelector("button")!);
			expect(onRemoveBranch).toHaveBeenCalledExactlyOnceWith("/repo1", "feat");
		});
		// Catches: a rich-only detail line leaking into the compact sidebar.
		it("keeps the compact row free of detail lines", () => {
			uiStore.cycleSidebarDensityMode(); // auto -> compact
			try {
				withBranch(richBranch());
				const { container } = render(() => <Sidebar {...defaultProps()} />);
				expect(container.querySelector(".branchRichMeta")).toBeNull();
				expect(container.querySelector("[data-testid='repo-rich-meta']")).toBeNull();
			} finally {
				uiStore.cycleSidebarDensityMode();
				uiStore.cycleSidebarDensityMode();
			}
		});

		// Catches: the stats badge duplicated (in the stack and in the meta line) or dropped in rich mode.
		it("shows the diff stats exactly once", () => {
			withBranch(richBranch());
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			expect(container.querySelectorAll(".branchStats").length).toBe(1);
			expect(container.querySelector(".branchBadgeStack .branchStats")).toBeNull();
		});

		// Catches: a merged branch with old commits flagged stale, or a merged branch not flagged at all.
		it("marks a merged branch as merged", () => {
			withBranch(richBranch({ isMerged: true, lifecycleStatus: undefined }));
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			expect(container.querySelector(".branchRichMeta")?.textContent).toContain("Merged");
		});

		// Catches: the repo header facts missing (branch, open PRs, worktrees).
		it("prints current branch, open PR count and worktree count under the repo header", () => {
			withBranch(richBranch());
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const meta = container.querySelector("[data-testid='repo-rich-meta']")?.textContent ?? "";
			expect(meta).not.toContain("open PR");
			expect(meta).toContain("1 worktree");
			expect(meta).not.toContain("1 worktrees");
		});

		// Catches: stale zero labels or positive counts disappearing after a live store update.
		it("sidebar_hides_only_known_zero_counts", () => {
			const [prs, setPrs] = createSignal<ReturnType<typeof githubStore.getAllOpenPrs>>([]);
			vi.spyOn(githubStore, "getAllOpenPrs").mockImplementation(prs);
			const [repo, setRepo] = createSignal(makeRepo());
			mockGetOrderedRepos.mockImplementation(() => [repo()]);
			mockGetGroupedLayout.mockImplementation(() => ({ groups: [], ungrouped: [repo()] }));
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const meta = () => container.querySelector("[data-testid='repo-rich-meta']")!;
			expect(meta()).toBeNull();
			setPrs([{ number: 1 }, { number: 2 }] as ReturnType<typeof githubStore.getAllOpenPrs>);
			setRepo(makeRepo({ workspaces: { feat: richBranch() } }));
			expect(meta()!.textContent).toContain("2 open PRs");
			expect(meta()!.textContent).toContain("1 worktree");
			setPrs([]);
			setRepo(makeRepo());
			expect(meta()).toBeNull();
		});

		// Catches: idle agents keeping expanded intent rows, or blank intent hiding tooltip fallback.
		it("rich_agent_intent_extends_only_while_working", () => {
			const intent = "Review the full sidebar intent without losing any words ".repeat(5);
			const [term, setTerm] = createSignal({
				id: "t1",
				name: "Agent",
				agentType: "codex",
				shellState: "busy",
				awaitingInput: null,
				unseen: false,
				agentIntent: intent,
				currentTask: null,
				lastPrompt: "Fallback prompt" as string | null,
			});
			mockTerminalsGet.mockImplementation(() => term());
			vi.mocked(terminalsStore.isBusy).mockImplementation(() => term().shellState === "busy");
			settingsStore.setTabTreeEnabled(true);
			withBranch(richBranch({ terminals: ["t1"] }));
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const line = () => container.querySelector(".branchTabIntent");
			expect(line()?.textContent).toBe(intent);
			expect(container.querySelector(".branchTabItem")?.getAttribute("data-tooltip")).toBe(`Agent: ${intent}`);
			setTerm({ ...term(), shellState: "idle" });
			expect(line()).toBeNull();
			setTerm({ ...term(), agentIntent: "", lastPrompt: "Fallback prompt" });
			expect(line()).toBeNull();
			expect(container.querySelector(".branchTabItem")?.getAttribute("data-tooltip")).toBe("Agent: Fallback prompt");
			setTerm({ ...term(), shellState: "busy", lastPrompt: null });
			expect(line()).not.toBeNull();
			uiStore.cycleSidebarDensityMode();
			try {
				expect(line()).toBeNull();
			} finally {
				uiStore.cycleSidebarDensityMode();
				uiStore.cycleSidebarDensityMode();
				vi.mocked(terminalsStore.isBusy).mockImplementation(() => false);
			}
		});

		// Catches: the agent row showing only a dot and a title, hiding what the agent is doing or asking.
		it("keeps awaiting input intent in the one-line row tooltip", () => {
			mockTerminalsGet.mockImplementation(() => ({
				id: "t1",
				name: "claude",
				shellState: "busy",
				unseen: false,
				awaitingInput: "question",
				agentIntent: "Refactor the sidebar",
				currentTask: null,
				lastPrompt: null,
				agentType: "claude",
			}));
			settingsStore.setTabTreeEnabled(true);
			withBranch(richBranch({ terminals: ["t1"] }));
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const detail = container.querySelector(".branchTabDetail")?.textContent ?? "";
			expect(detail).toBe("");
			expect(container.querySelector(".branchTabDotQuestion")).not.toBeNull();
			expect(container.querySelector(".branchTabItem")?.getAttribute("data-tooltip")).toContain("Refactor the sidebar");
		});

		const OLD_TS = () => Date.now() - 90 * 86_400_000;
		const chips = (c: HTMLElement) => c.querySelector(".branchRichMeta")?.textContent ?? "";

		// Catches: main flagged Stale in rich because the facts never learn it is a main checkout.
		it("never marks a main branch stale, merged, dirty or unknown", () => {
			withBranch(
				richBranch({
					isMain: true,
					isMerged: true,
					lastCommitTs: OLD_TS(),
					lifecycleStatus: { dirtyFiles: 4, commitStatus: "unknown", removalSafety: "destructive" },
				}),
			);
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const meta = chips(container);
			expect(meta).not.toMatch(/Stale|Merged|Unknown|dirty/);
		});

		// Catches: the lifecycle "unknown" verdict (removal blocked) vanishing in rich.
		it("shows the Unknown chip with the removal-blocked explanation", () => {
			withBranch(
				richBranch({
					lifecycleStatus: { dirtyFiles: 0, commitStatus: "unknown", removalSafety: "destructive", error: "boom" },
				}),
			);
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const chip = [...container.querySelectorAll(".richChip")].find((c) => c.textContent === "Unknown") as HTMLElement;
			expect(chip).toBeTruthy();
			expect(chip.getAttribute("data-tooltip")).toContain("removal is blocked");
			expect(chip.getAttribute("data-tooltip")).toContain("boom");
		});

		// Catches: rich chips explaining themselves through title=, which touch and WKWebView never show.
		it("explains the Stale and dirty chips through data-tooltip", () => {
			withBranch(richBranch({ lastCommitTs: OLD_TS() }));
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const all = [...container.querySelectorAll(".branchRichMeta .richChip")];
			const stale = all.find((c) => c.textContent === "Stale");
			expect(stale?.getAttribute("data-tooltip")).toContain("30 days");
			const dirty = all.find((c) => c.textContent?.includes("dirty"));
			expect(dirty?.getAttribute("data-tooltip")).toContain("4 uncommitted files");
			expect(container.querySelector(".branchRichMeta [title]")).toBeNull();
		});

		// Catches: the rich dirty count being a dead span while the compact chip opens Changes.
		it("opens the Changes tab from the dirty chip", () => {
			const open = vi.spyOn(uiStore, "openGitPanelOnTab").mockImplementation(() => {});
			withBranch(richBranch());
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const dirty = [...container.querySelectorAll(".richChip")].find((c) =>
				c.textContent?.includes("dirty"),
			) as HTMLElement;
			fireEvent.click(dirty);
			expect(open).toHaveBeenCalledWith("changes");
			open.mockRestore();
		});

		// Catches: touch users unable to read the PR state, which compact keeps in a tooltip.
		it("prints the PR state word before the PR title", () => {
			mockGetPrStatus.mockReturnValue({ state: "OPEN", number: 77, title: "Add the thing", url: "u", is_draft: true });
			withBranch(richBranch());
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			expect(container.querySelector(".branchRichLine")?.textContent).toBe("Draft Add the thing");
		});

		// Catches: the unmerged marker and the ahead/behind chip sharing one up-arrow glyph in a rich row.
		it("spells unmerged out and drops the arrow marker in rich", () => {
			withBranch(
				richBranch({ lifecycleStatus: { dirtyFiles: 0, commitStatus: "unmerged", removalSafety: "destructive" } }),
			);
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			expect(chips(container)).toContain("unmerged");
			expect(container.querySelector(".branchUnmergedMarker")).toBeNull();
		});

		// Catches: compact losing what rich shows (age, stale rule) with no other way to read it on desktop.
		it("carries the rich-only branch facts on the compact name tooltip", () => {
			uiStore.cycleSidebarDensityMode();
			try {
				withBranch(richBranch({ lastCommitTs: OLD_TS() }));
				const { container } = render(() => <Sidebar {...defaultProps()} />);
				const name = container.querySelector(".branchName");
				const tip = name?.getAttribute("data-tooltip") ?? "";
				expect(tip).toContain("Last commit: 90d");
				expect(tip).toContain("Stale");
				// Catches: the name dropped from the overlay once the native title that carried it is gone.
				expect(tip.startsWith("feat")).toBe(true);
				// Catches: the native tooltip stacking on the overlay (the 2026-10-02 double tooltip).
				expect(name?.hasAttribute("title")).toBe(false);
			} finally {
				uiStore.cycleSidebarDensityMode();
				uiStore.cycleSidebarDensityMode();
			}
		});

		// Catches: rich keeping a native title on the row, or losing the full name when the facts moved inline.
		it("gives the rich branch name one overlay with the full name and no native title", () => {
			withBranch(richBranch({ branchName: "feature/a-very-long-branch-name" }));
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const name = container.querySelector(".branchName");
			expect(name?.getAttribute("data-tooltip")).toContain("feature/a-very-long-branch-name");
			expect(name?.getAttribute("data-tooltip")).toContain("Last commit: 3h");
			expect(container.querySelector(".branchItem [title]")).toBeNull();
		});

		// Catches: ellipsis making the current branch unreadable in the repo metadata chip.
		it.each(["main", "salvage/" + "long-branch-name-".repeat(10)])(
			"keeps the full current branch %s in the rich repo chip tooltip",
			(branchName) => {
				setRepos({
					"/repo1": makeRepo({ workspaces: { main: richBranch({ branchName }), feat: richBranch() } }),
				});
				const { container } = render(() => <Sidebar {...defaultProps()} />);
				const chip = container.querySelector("[data-testid='repo-rich-meta'] .richChip");
				expect(chip?.textContent).toBe(`⎇ ${branchName}`);
				expect(chip?.getAttribute("data-tooltip")).toBe(branchName);
			},
		);

		// Catches: "1 open PRs" and "1 worktrees".
		it("uses the singular for one worktree and one open PR", () => {
			vi.spyOn(githubStore, "getAllOpenPrs").mockReturnValue([{ number: 1 }] as never);
			withBranch(richBranch());
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const meta = container.querySelector("[data-testid='repo-rich-meta']")?.textContent ?? "";
			expect(meta).toContain("1 open PR");
			expect(meta).not.toContain("open PRs");
		});

		// Catches: the main checkout counted as a worktree when its path differs by a trailing slash.
		it("does not count the main checkout when its path has a trailing slash", () => {
			setRepos({
				"/repo1": makeRepo({
					workspaces: {
						main: richBranch({ workspaceId: "main", branchName: "main", isMain: true, worktreePath: "/repo1/" }),
					},
				}),
			});
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			expect(container.querySelector("[data-testid='repo-rich-meta']")?.textContent).not.toContain("worktree");
		});

		describe("subagents and child sessions", () => {
			const term = (id: string, over: Record<string, unknown> = {}) => ({
				id,
				name: id,
				sessionId: `s-${id}`,
				fontSize: 14,
				cwd: null,
				tuicSession: `tuic-${id}`,
				shellState: "busy",
				unseen: false,
				awaitingInput: null,
				agentIntent: null,
				currentTask: null,
				lastPrompt: null,
				agentType: "claude",
				parentSession: null,
				...over,
			});
			const sub = (n: number, state = "running") => ({
				id: `s-t1/a${n}`,
				kind: "subagent",
				title: `Sub task ${n}`,
				state,
				parent: "s-t1",
				toolCalls: n * 3,
				ptyId: "s-t1",
				agentId: `a${n}`,
			});
			const flowOf = (subs: ReturnType<typeof sub>[]) => ({
				project: "/repo1",
				participants: subs,
				events: subs.map((p) => ({
					id: `${p.id}:${p.state === "running" ? "spawn" : "return"}`,
					kind: p.state === "running" ? "subagent_spawn" : "subagent_return",
					from: "s-t1",
					summary: "",
					atMs: Date.now() - 5 * 60_000,
				})),
				truncated: false,
			});
			const setup = (
				terms: Record<string, unknown> | (() => Record<string, unknown>),
				subs: ReturnType<typeof sub>[],
				ids = Object.keys(typeof terms === "function" ? terms() : terms),
			) => {
				mockTerminalsGet.mockImplementation(
					(id: string) => (typeof terms === "function" ? terms() : terms)[id] ?? null,
				);
				vi.spyOn(progressStore, "sidebarFlow").mockReturnValue(flowOf(subs) as never);
				vi.spyOn(progressStore, "refreshSidebarFlow").mockResolvedValue();
				settingsStore.setTabTreeEnabled(true);
				withBranch(richBranch({ terminals: ids }));
				return render(() => <Sidebar {...defaultProps()} />);
			};
			afterEach(() => vi.restoreAllMocks());

			// Catches: opening a wrong child repository, cross-repo nesting, or stale links after children close.
			it("cross_repo_child_link_navigates_without_reparenting", async () => {
				const realRepos = (
					await vi.importActual<typeof import("../../stores/repositories")>("../../stores/repositories")
				).repositoriesStore;
				const realTerms = (await vi.importActual<typeof import("../../stores/terminals")>("../../stores/terminals"))
					.terminalsStore;
				const { navigateToTerminal } = await vi.importActual<typeof import("../../utils/navigateToTerminal")>(
					"../../utils/navigateToTerminal",
				);
				const savedRepos = { ...repositoriesStore };
				const savedTerms = { ...terminalsStore };
				Object.assign(repositoriesStore, realRepos);
				Object.assign(terminalsStore, realTerms);
				mockNavigateToTerminal.mockImplementation(navigateToTerminal);
				let dispose: (() => void) | undefined;
				try {
					vi.spyOn(progressStore, "sidebarFlow").mockReturnValue(flowOf([]) as never);
					vi.spyOn(progressStore, "refreshSidebarFlow").mockResolvedValue();
					vi.spyOn(appLogger, "info").mockImplementation(() => {});
					const persistenceErrors = vi.spyOn(appLogger, "error").mockImplementation(() => {});
					settingsStore.setTabTreeEnabled(true);
					expect(persistenceErrors.mock.calls).toEqual([
						["config", "Refusing to persist settings: store not hydrated — would clobber config.json with defaults"],
					]);
					for (const [id, parentSession] of [
						["t1", null],
						["child", "tuic-t1"],
						["other", "s-t1"],
					] as const) {
						realTerms.register(id, term(id, { parentSession }) as Parameters<typeof realTerms.register>[1]);
					}
					realRepos.add({ path: "/repo1", displayName: "Repo One" });
					realRepos.setWorkspace("/repo1", "feat", richBranch({ terminals: [] }));
					realRepos.addTerminalToWorkspace("/repo1", "feat", "t1");
					realRepos.setActive("/repo1");
					realRepos.setActiveWorkspace("/repo1", "feat");
					realTerms.setActive("t1");
					realRepos.add({ path: "/child", displayName: "Child repo" });
					realRepos.setWorkspace(
						"/child",
						"work",
						richBranch({ workspaceId: "work", terminals: [], tabsCollapsed: true }),
					);
					realRepos.addTerminalToWorkspace("/child", "work", "child");
					realRepos.addTerminalToWorkspace("/child", "work", "other");
					realRepos.toggleCollapsed("/child");
					realRepos.toggleExpanded("/child");
					const groupId = realRepos.createGroup("Children");
					if (!groupId) throw new Error("Missing child repository group");
					realRepos.addRepoToGroup("/child", groupId);
					realRepos.toggleGroupCollapsed(groupId);
					const { container, unmount } = render(() => <Sidebar {...defaultProps()} />);
					dispose = unmount;
					const link = () => container.querySelector<HTMLButtonElement>("[data-testid='cross-repo-child-link']");
					const childRow = () => container.querySelector("[data-sidebar-repo='/child'] .branchTabItem.active");
					expect(link()?.textContent).toContain("2 agents in Child repo");
					expect(container.querySelector("[data-sidebar-repo='/repo1'] .branchTabNested")).toBeNull();
					expect(childRow()).toBeNull();
					const childLink = link();
					if (!childLink) throw new Error("Missing child navigation link");
					fireEvent.click(childLink);
					expect(realRepos.state.activeRepoPath).toBe("/child");
					expect(realRepos.state.repositories["/child"].activeWorkspaceId).toBe("work");
					expect(realTerms.state.activeId).toBe("child");
					expect(childRow()).toBeVisible();
					expect(childRow()?.textContent).toContain("child");
					expect(realRepos.state.groups[groupId].collapsed).toBe(false);
					expect(realRepos.state.repositories["/repo1"].workspaces.feat.terminals).toEqual(["t1"]);
					expect(realRepos.state.repositories["/child"].workspaces.work.terminals).toEqual(["child", "other"]);
					expect(realTerms.get("child")?.parentSession).toBe("tuic-t1");
					realTerms.remove("child");
					expect(link()?.textContent).toContain("1 agent in Child repo");
					realTerms.remove("other");
					expect(link()).toBeNull();
				} finally {
					dispose?.();
					for (const id of ["t1", "child", "other"]) realTerms.remove(id);
					for (const path of ["/repo1", "/child"]) realRepos.remove(path);
					for (const id of Object.keys(realRepos.state.groups)) realRepos.deleteGroup(id);
					mockNavigateToTerminal.mockReset();
					for (const key of Object.keys(repositoriesStore)) Reflect.deleteProperty(repositoriesStore, key);
					for (const key of Object.keys(terminalsStore)) Reflect.deleteProperty(terminalsStore, key);
					Object.assign(repositoriesStore, savedRepos);
					Object.assign(terminalsStore, savedTerms);
				}
			});
			// Catches: folding at the threshold, hiding attention/selection, or reordering restored rows.
			it("idle_fold_preserves_attention_and_order", () => {
				const old = Date.now() - 2 * 3600_000 - 1;
				const [terms, setTerms] = createSignal({
					old: term("old", { shellState: "idle", lastActivityAt: old }),
					exact: term("exact", { shellState: "idle", lastActivityAt: old + 1 }),
					question: term("question", { shellState: "idle", lastActivityAt: old, awaitingInput: "question" }),
					unread: term("unread", { shellState: "idle", lastActivityAt: old, unseen: true }),
					working: term("working", { shellState: "idle", lastActivityAt: old, agentState: "working" }),
					unknown: term("unknown", { shellState: "idle", lastActivityAt: null }),
				});
				const { container } = setup(terms, []);
				const labels = () =>
					[...container.querySelectorAll(".branchTabItem .branchAgentActivity")].map((e) => e.textContent);
				expect(labels()).toEqual(["exact", "question", "unread", "working", "unknown"]);
				const fold = () => container.querySelector<HTMLButtonElement>("[data-testid='idle-session-fold']")!;
				expect(fold().textContent).toBe("1 idle sessions");
				fireEvent.click(fold());
				expect(labels()).toEqual(Object.keys(terms()));
				fireEvent.click(fold());
				setTerms({
					...terms(),
					old: term("old", { shellState: "idle", lastActivityAt: old, awaitingInput: "question" }),
				});
				expect(labels()).toEqual(Object.keys(terms()));
				expect(fold()).toBeNull();
			});

			// Catches: hiding a live child by folding its old parent or changing compact rows.
			it("idle_fold_keeps_parents_of_live_children_and_compact_rows", () => {
				const { container } = setup(
					{
						parent: term("parent", { shellState: "idle", lastActivityAt: Date.now() - 3 * 3600_000 }),
						child: term("child", { parentSession: "tuic-parent", shellState: "busy" }),
						old: term("old", { shellState: "idle", lastActivityAt: Date.now() - 3 * 3600_000 }),
					},
					[],
				);
				expect(container.querySelectorAll(".branchTabItem")).toHaveLength(2);
				uiStore.cycleSidebarDensityMode();
				try {
					expect(container.querySelectorAll(".branchTabItem")).toHaveLength(3);
					expect(container.querySelector("[data-testid='idle-session-fold']")).toBeNull();
				} finally {
					uiStore.cycleSidebarDensityMode();
					uiStore.cycleSidebarDensityMode();
				}
			});

			// Catches: returned history filling the sidebar or being folded together with active work.
			it("returned_subagents_fold_without_hiding_running_children", () => {
				const returned = Array.from({ length: 30 }, (_, i) => sub(i + 2, "done"));
				const { container } = setup({ t1: term("t1") }, [sub(1), ...returned]);
				const rows = () => [...container.querySelectorAll(".subagentRow")].map((e) => e.textContent);
				expect(rows()).toHaveLength(1);
				expect(rows()[0]).toContain("Sub task 1");
				const fold = container.querySelector<HTMLButtonElement>("[data-testid='returned-subagent-fold']")!;
				expect(fold.textContent).toBe("30 returned");
				fireEvent.click(fold);
				expect(rows()).toHaveLength(31);
				returned.forEach((row, i) => expect(rows()[i + 1]).toContain(row.title));
				fireEvent.click(fold);
				expect(rows()).toHaveLength(1);
			});

			// Catches: subagents missing from the rich agent row (Boss: "non ci sono i subagents").
			it("lists each subagent with state, title, tool calls and age", () => {
				const { container } = setup({ t1: term("t1") }, [sub(1), sub(2, "done")]);
				const rows = [...container.querySelectorAll(".subagentRow")].map((r) => r.textContent ?? "");
				expect(rows).toHaveLength(1);
				expect(rows[0]).toContain("Running");
				expect(rows[0]).toContain("Sub task 1");
				expect(rows[0]).toContain("3 calls");
				expect(rows[0]).toContain("5m");
				const returned = container.querySelector<HTMLButtonElement>("[data-testid='returned-subagent-fold']")!;
				fireEvent.click(returned);
				expect(container.querySelectorAll(".subagentRow")[1].textContent).toContain("Returned");
			});

			// Catches: a long subagent list pushing every other row off screen.
			it("folds more than three subagents into a count and expands on click", () => {
				const { container } = setup({ t1: term("t1") }, [sub(1), sub(2), sub(3), sub(4)]);
				expect(container.querySelectorAll(".subagentRow")).toHaveLength(0);
				const fold = container.querySelector(".subagentFold") as HTMLElement;
				expect(fold.textContent).toBe("4 subagents");
				fireEvent.click(fold);
				expect(container.querySelectorAll(".subagentRow")).toHaveLength(4);
			});

			// Catches: three subagents folding, one past the stated threshold.
			it("shows exactly three subagents unfolded", () => {
				const { container } = setup({ t1: term("t1") }, [sub(1), sub(2), sub(3)]);
				expect(container.querySelectorAll(".subagentRow")).toHaveLength(3);
			});

			// Catches: another agent's subagents printed under this one.
			it("prints only the subagents of its own session", () => {
				const { container } = setup({ t1: term("t1"), t2: term("t2") }, [sub(1)]);
				expect(container.querySelectorAll(".subagentRow")).toHaveLength(1);
			});

			// Catches: a TUIC child session shown only as a tag icon, or twice (top level and nested).
			it("nests a child session under its parent agent row, once", () => {
				const { container } = setup({ t1: term("t1"), t2: term("t2", { parentSession: "tuic-t1" }) }, []);
				const items = [...container.querySelectorAll(".branchTabItem")];
				expect(items).toHaveLength(2);
				expect(items[0].classList.contains("branchTabNested")).toBe(false);
				expect(items[1].classList.contains("branchTabNested")).toBe(true);
			});

			// Catches: a child whose parent is on another branch vanishing from the list.
			it("keeps a child top level when its parent is not on the branch", () => {
				const { container } = setup({ t2: term("t2", { parentSession: "tuic-gone" }) }, []);
				const item = container.querySelector(".branchTabItem") as HTMLElement;
				expect(item.classList.contains("branchTabNested")).toBe(false);
			});

			// Catches: the polling effect tracking only the first agent terminal, so a later agent
			// finishing (its subagents returning) never triggers a refresh.
			it("refreshes the flow when a second agent terminal flips busy", () => {
				const [busy, setBusy] = createSignal(false);
				vi.mocked(terminalsStore.isBusy).mockImplementation((id: string) => (id === "t2" ? busy() : false));
				setup({ t1: term("t1"), t2: term("t2") }, []);
				const calls = vi.mocked(progressStore.refreshSidebarFlow).mock.calls.length;
				setBusy(true);
				expect(vi.mocked(progressStore.refreshSidebarFlow).mock.calls.length).toBeGreaterThan(calls);
				vi.mocked(terminalsStore.isBusy).mockImplementation(() => false);
			});

			// Catches: compact mode changed by round 6.
			it("leaves compact mode without subagent lines or nesting", () => {
				uiStore.cycleSidebarDensityMode();
				try {
					const { container } = setup({ t1: term("t1"), t2: term("t2", { parentSession: "tuic-t1" }) }, [sub(1)]);
					expect(container.querySelectorAll(".subagentRow")).toHaveLength(0);
					expect(container.querySelectorAll(".branchTabNested")).toHaveLength(0);
				} finally {
					uiStore.cycleSidebarDensityMode();
					uiStore.cycleSidebarDensityMode();
				}
			});
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

			const settingsBtn = container.querySelector('.footerAction[aria-label="Settings"]');
			expect(settingsBtn).not.toBeNull();
			fireEvent.click(settingsBtn!);
			expect(onOpenSettings).toHaveBeenCalledOnce();
		});

		it("calls onOpenHelp when Help footer button is clicked", () => {
			const onOpenHelp = vi.fn();
			const { container } = render(() => <Sidebar {...defaultProps({ onOpenHelp })} />);

			const helpBtn = container.querySelector('.footerAction[aria-label="Help"]');
			expect(helpBtn).not.toBeNull();
			fireEvent.click(helpBtn!);
			expect(onOpenHelp).toHaveBeenCalledOnce();
		});

		it("does not render unimplemented Notifications and Tasks buttons", () => {
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			expect(container.querySelector('.footerAction[aria-label="Notifications"]')).toBeNull();
			expect(container.querySelector('.footerAction[aria-label="Tasks"]')).toBeNull();
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
					workspaces: {
						main: {
							workspaceId: "main",
							branchName: "main",
							isMain: true,
							worktreePath: null,
							terminals: ["t1"],
							additions: 0,
							deletions: 0,
						},
						"feature/x": {
							workspaceId: "feature/x",
							branchName: "feature/x",
							isMain: false,
							worktreePath: "/wt/x",
							terminals: ["t2"],
							additions: 0,
							deletions: 0,
						},
					},
				}),
			});
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const icons = container.querySelectorAll(".branchIcon");
			expect(icons.length).toBe(2);
			// Main branch first (sorted), then feature
			const mainIcon = Array.from(icons).find((i) => i.classList.contains("branchIconMain"));
			const featureIcon = Array.from(icons).find((i) => i.classList.contains("branchIconWorktree"));
			expect(mainIcon).toBeDefined();
			expect(mainIcon!.querySelector("svg")).not.toBeNull();
			expect(featureIcon).toBeDefined();
			expect(featureIcon!.querySelector("svg")).not.toBeNull();
		});

		it("sorts branches with main first", () => {
			setRepos({
				"/repo1": makeRepo({
					workspaces: {
						"feature/z": {
							workspaceId: "feature/z",
							branchName: "feature/z",
							isMain: false,
							worktreePath: null,
							terminals: [],
							additions: 0,
							deletions: 0,
						},
						main: { branchName: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
						"feature/a": {
							workspaceId: "feature/a",
							branchName: "feature/a",
							isMain: false,
							worktreePath: null,
							terminals: [],
							additions: 0,
							deletions: 0,
						},
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
					workspaces: {
						main: { branchName: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
						"feature/active": {
							workspaceId: "feature/active",
							branchName: "feature/active",
							isMain: false,
							worktreePath: null,
							terminals: [],
							additions: 0,
							deletions: 0,
						},
						"feature/merged": {
							workspaceId: "feature/merged",
							branchName: "feature/merged",
							isMain: false,
							worktreePath: null,
							terminals: [],
							additions: 0,
							deletions: 0,
						},
						"feature/closed": {
							workspaceId: "feature/closed",
							branchName: "feature/closed",
							isMain: false,
							worktreePath: null,
							terminals: [],
							additions: 0,
							deletions: 0,
						},
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

		describe("add terminal button agent list", () => {
			const agentItems = (launch: () => void) => () => [
				{ label: "Add Agent", action: () => {}, children: [{ label: "Claude Code", action: launch }] },
			];
			const menuLabels = (container: HTMLElement) =>
				Array.from(container.querySelectorAll(".menu .label")).map((l) => l.textContent);
			const clickLabel = (container: HTMLElement, label: string) => {
				const el = Array.from(container.querySelectorAll(".menu .label")).find((l) => l.textContent === label)!;
				fireEvent.click(el.closest(".item") ?? el);
			};

			it("a long press lists the agents of that branch instead of opening a terminal", () => {
				const onAddTerminal = vi.fn();
				const launch = vi.fn();
				const buildAgentMenuItems = vi.fn(agentItems(launch));
				const { container } = render(() => <Sidebar {...defaultProps({ onAddTerminal, buildAgentMenuItems })} />);
				const addBtn = container.querySelector(".branchAddBtn")!;
				fireEvent.pointerDown(addBtn, { button: 0 });
				vi.advanceTimersByTime(500);
				fireEvent.pointerUp(addBtn);
				fireEvent.click(addBtn);

				expect(onAddTerminal).not.toHaveBeenCalled();
				expect(buildAgentMenuItems).toHaveBeenCalledWith("/repo1", "main");
				clickLabel(container, "Claude Code");
				expect(launch).toHaveBeenCalledTimes(1);
			});

			it("a right click still opens the branch row menu", () => {
				// The agent list on right click is withheld until Boss approves a change
				// to a sidebar click (AGENTS.md "Sidebar clicks"). Only the long press has it.
				const onAddTerminal = vi.fn();
				const buildAgentMenuItems = vi.fn(agentItems(() => {}));
				const { container } = render(() => <Sidebar {...defaultProps({ onAddTerminal, buildAgentMenuItems })} />);
				fireEvent.contextMenu(container.querySelector(".branchAddBtn")!);

				expect(menuLabels(container)).toContain("Add Terminal");
				expect(menuLabels(container)).not.toContain("Claude Code");
				expect(onAddTerminal).not.toHaveBeenCalled();
			});

			it("a right click during a pending left press still opens the branch row menu", () => {
				// A mouse right click is button 2; a touch long press's contextmenu is not (#882-e5a7).
				const onAddTerminal = vi.fn();
				const { container } = render(() => (
					<Sidebar {...defaultProps({ onAddTerminal, buildAgentMenuItems: agentItems(() => {}) })} />
				));
				const addBtn = container.querySelector(".branchAddBtn")!;
				fireEvent.pointerDown(addBtn, { button: 0 });
				vi.advanceTimersByTime(200);
				fireEvent.contextMenu(addBtn, { button: 2 });
				vi.advanceTimersByTime(1000);

				expect(menuLabels(container)).toContain("Add Terminal");
				expect(menuLabels(container)).not.toContain("Claude Code");
			});

			it("a macOS Ctrl+click opens the branch row menu, not the agent list", () => {
				// WebKit reports Ctrl+click's contextmenu as button 0, on mousedown,
				// while the press timer is pending: ctrlKey is what marks it (#882-e5a7).
				const onAddTerminal = vi.fn();
				const { container } = render(() => (
					<Sidebar {...defaultProps({ onAddTerminal, buildAgentMenuItems: agentItems(() => {}) })} />
				));
				const addBtn = container.querySelector(".branchAddBtn")!;
				fireEvent.pointerDown(addBtn, { button: 0, ctrlKey: true });
				fireEvent.contextMenu(addBtn, { button: 0, ctrlKey: true });
				vi.advanceTimersByTime(1000);

				expect(menuLabels(container)).toContain("Add Terminal");
				expect(menuLabels(container)).not.toContain("Claude Code");
				expect(onAddTerminal).not.toHaveBeenCalled();
			});

			it("a touch long press whose native contextmenu beats the timer opens the agent list", () => {
				const onAddTerminal = vi.fn();
				const { container } = render(() => (
					<Sidebar {...defaultProps({ onAddTerminal, buildAgentMenuItems: agentItems(() => {}) })} />
				));
				const addBtn = container.querySelector(".branchAddBtn")!;
				fireEvent.pointerDown(addBtn, { button: 0 });
				vi.advanceTimersByTime(450);
				fireEvent.contextMenu(addBtn);
				fireEvent.pointerUp(addBtn);
				fireEvent.click(addBtn);
				vi.advanceTimersByTime(1000);

				expect(menuLabels(container)).toContain("Claude Code");
				expect(menuLabels(container)).not.toContain("Add Terminal");
				expect(onAddTerminal).not.toHaveBeenCalled();
			});

			it("a short press still opens a plain terminal", () => {
				const onAddTerminal = vi.fn();
				const { container } = render(() => (
					<Sidebar {...defaultProps({ onAddTerminal, buildAgentMenuItems: agentItems(() => {}) })} />
				));
				const addBtn = container.querySelector(".branchAddBtn")!;
				fireEvent.pointerDown(addBtn, { button: 0 });
				vi.advanceTimersByTime(200);
				fireEvent.pointerUp(addBtn);
				fireEvent.click(addBtn);
				vi.advanceTimersByTime(1000);

				expect(onAddTerminal).toHaveBeenCalledWith("/repo1", "main");
				expect(menuLabels(container)).not.toContain("Claude Code");
			});

			it("a shell row has no agents, so a right click falls through to the row menu", () => {
				setRepos({
					"/repo1": makeRepo({
						workspaces: {
							shell: {
								branchName: "shell",
								isMain: true,
								isShell: true,
								worktreePath: "/repo1",
								terminals: [],
								additions: 0,
								deletions: 0,
							},
						},
						activeWorkspaceId: "shell",
					}),
				});
				const buildAgentMenuItems = vi.fn(agentItems(() => {}));
				const { container } = render(() => <Sidebar {...defaultProps({ buildAgentMenuItems })} />);
				fireEvent.contextMenu(container.querySelector(".branchAddBtn")!);

				expect(menuLabels(container)).toContain("Add Terminal");
				expect(menuLabels(container)).not.toContain("Claude Code");
			});
		});

		it("main branch has no remove button", () => {
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const removeBtn = container.querySelector(".branchRemoveBtn");
			expect(removeBtn).toBeNull();
		});

		it("feature branch has remove button that calls onRemoveBranch", () => {
			setRepos({
				"/repo1": makeRepo({
					workspaces: {
						main: { branchName: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
						"feature/x": {
							workspaceId: "feature/x",
							branchName: "feature/x",
							isMain: false,
							worktreePath: "/wt/x",
							terminals: [],
							additions: 0,
							deletions: 0,
						},
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
					workspaces: {
						main: { branchName: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
						"feature/y": {
							workspaceId: "feature/y",
							branchName: "feature/y",
							isMain: false,
							worktreePath: null,
							terminals: [],
							additions: 0,
							deletions: 0,
						},
					},
				}),
			});
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const removeBtns = container.querySelectorAll(".branchRemoveBtn");
			expect(removeBtns.length).toBe(0);
		});

		it("main checkout on a non-main-named branch has no remove button (worktreePath === repoPath)", () => {
			// Regression: the main working tree cannot be `git worktree remove`d. `isMain`
			// is name-based (main/master/develop), so a main checkout sitting on a
			// differently-named branch (e.g. POC-00001-merge-blades) has isMain=false. The
			// button guard must fall back to worktreePath !== repoPath — not isMain — or the
			// × wrongly appears on the main checkout when worktrees exist.
			setRepos({
				"/repo1": makeRepo({
					workspaces: {
						"POC-00001-merge-blades": {
							workspaceId: "POC-00001-merge-blades",
							branchName: "POC-00001-merge-blades",
							isMain: false,
							worktreePath: "/repo1",
							terminals: [],
							additions: 0,
							deletions: 0,
						},
						"feature/x": {
							workspaceId: "feature/x",
							branchName: "feature/x",
							isMain: false,
							worktreePath: "/wt/x",
							terminals: [],
							additions: 0,
							deletions: 0,
						},
					},
				}),
			});
			const { container } = render(() => <Sidebar {...defaultProps({ onRemoveBranch: vi.fn() })} />);
			const removeBtns = container.querySelectorAll(".branchRemoveBtn");
			// Only the linked worktree (feature/x) gets a × — the main checkout does not.
			expect(removeBtns.length).toBe(1);
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
			setRepos({
				"/repo1": makeRepo({
					workspaces: {
						main: { branchName: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
						"feature/x": {
							workspaceId: "feature/x",
							branchName: "feature/x",
							isMain: false,
							worktreePath: "/wt/x",
							terminals: [],
							additions: 0,
							deletions: 0,
						},
					},
				}),
			});
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
			expect(menuItems[1].textContent).toContain("Create Worktree");
			expect(menuItems[2].textContent).toContain("Move to Group");
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

			// Click remove — index 4 (after "Repo Settings", "Create Worktree", "Move to Group", and "Park Repository")
			const menuItems = container.querySelectorAll(".item");
			fireEvent.click(menuItems[4]);
			expect(onRemoveRepo).toHaveBeenCalledWith("/repo1");
		});

		it("repo menu Create Worktree click calls onAddWorktree", () => {
			const onAddWorktree = vi.fn();
			const { container } = render(() => <Sidebar {...defaultProps({ onAddWorktree })} />);

			// Open menu
			const menuBtn = container.querySelector(".repoActionBtn")!;
			fireEvent.click(menuBtn);

			// Click Create Worktree — index 1
			const menuItems = container.querySelectorAll(".item");
			fireEvent.click(menuItems[1]);
			expect(onAddWorktree).toHaveBeenCalledWith("/repo1");
		});

		it("repo menu does not show Create Worktree for non-git repos", () => {
			setRepos({ "/repo1": makeRepo({ isGitRepo: false }) });
			const { container } = render(() => <Sidebar {...defaultProps()} />);

			// Open menu
			const menuBtn = container.querySelector(".repoActionBtn")!;
			fireEvent.click(menuBtn);

			const menu = container.querySelector(".menu");
			expect(menu).not.toBeNull();

			const menuItems = menu!.querySelectorAll(".item");
			expect(menuItems.length).toBe(4);
			const labels = Array.from(menuItems).map((el) => el.textContent);
			expect(labels.some((l) => l?.includes("Create Worktree"))).toBe(false);
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
			expect(menuItems[1].textContent).toContain("Create Worktree");
			expect(menuItems[2].textContent).toContain("Move to Group");
			expect(menuItems[3].textContent).toContain("Park Repository");
			expect(menuItems[4].textContent).toContain("Remove Repository");
		});

		it("branch Copy Path item exposes the full path as a native tooltip", () => {
			setRepos({
				"/repo1": makeRepo({
					workspaces: {
						main: { branchName: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
						"feature/x": {
							workspaceId: "feature/x",
							branchName: "feature/x",
							isMain: false,
							worktreePath: "/wt/feature-x",
							terminals: [],
							additions: 0,
							deletions: 0,
						},
					},
				}),
			});
			const { container } = render(() => <Sidebar {...defaultProps()} />);

			const branchRow = Array.from(container.querySelectorAll(".branchItem")).find((el) =>
				el.textContent?.includes("feature/x"),
			)!;
			fireEvent.contextMenu(branchRow, { clientX: 50, clientY: 50 });

			const copyPath = Array.from(container.querySelectorAll(".menu .item")).find((el) =>
				el.textContent?.includes("Copy Path"),
			)!;
			expect(copyPath).toBeTruthy();
			expect(copyPath.getAttribute("title")).toBe("/wt/feature-x");
		});

		it("repo header click calls toggleExpanded", () => {
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const header = container.querySelector(".repoHeader")!;
			fireEvent.click(header);
			expect(mockToggleExpanded).toHaveBeenCalledWith("/repo1");
		});

		it("shows 'No branches loaded' when repo has no branches", () => {
			setRepos({
				"/repo1": makeRepo({ workspaces: {} }),
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
		it("shows shortcut keys and hides action buttons in quick switcher mode", () => {
			setRepos({ "/repo1": makeRepo() });
			const { container } = render(() => <Sidebar {...defaultProps({ quickSwitcherActive: true })} />);

			const shortcut = container.querySelector(".branchShortcut") as HTMLElement;
			expect(shortcut).not.toBeNull();
			expect(shortcut.textContent).toContain("1");
			expect(shortcut.style.display).not.toBe("none");

			// Action buttons should be hidden (display:none), not removed
			const actions = container.querySelector(".branchActions") as HTMLElement;
			expect(actions).not.toBeNull();
			expect(actions.style.display).toBe("none");
		});

		it("keeps collapsed repos hidden in quick switcher mode", () => {
			setRepos({ "/repo1": makeRepo({ expanded: false }) });
			const { container } = render(() => <Sidebar {...defaultProps({ quickSwitcherActive: true })} />);

			const branchItems = container.querySelectorAll(".branchItem");
			expect(branchItems.length).toBe(0);
		});
	});

	describe("branch badges", () => {
		// The compact row carries these badges; the rich layout moves them into detail lines.
		beforeEach(() => densityMode("compact"));
		afterEach(() => {
			uiStore.cycleSidebarDensityMode();
			uiStore.cycleSidebarDensityMode();
		});

		it("shows StatsBadge with additions and deletions", () => {
			setRepos({
				"/repo1": makeRepo({
					workspaces: {
						main: { branchName: "main", isMain: true, worktreePath: null, terminals: [], additions: 10, deletions: 5 },
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

		it("compacts large stats while retaining exact counts in the tooltip", () => {
			setRepos({
				"/repo1": makeRepo({
					workspaces: {
						main: {
							branchName: "main",
							isMain: true,
							worktreePath: null,
							terminals: [],
							additions: 9876,
							deletions: 2913,
						},
					},
				}),
			});
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			expect(container.querySelector(".statAdd")?.textContent).toBe("+9.9k");
			expect(container.querySelector(".statDel")?.textContent).toBe("-2.9k");
			expect(container.querySelector(".branchStats")?.getAttribute("data-tooltip")).toBe(
				"Tracked line changes: +9876 -2913",
			);
		});

		it("does not show StatsBadge when both additions and deletions are 0", () => {
			setRepos({
				"/repo1": makeRepo({
					workspaces: {
						main: { branchName: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
					},
				}),
			});
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const stats = container.querySelector(".branchStats");
			expect(stats).toBeNull();
		});

		/** One workspace row carrying a lifecycle verdict. */
		function renderLifecycle(workspace: Record<string, unknown>) {
			setRepos({
				"/repo1": makeRepo({
					activeWorkspaceId: "feature",
					workspaces: {
						feature: {
							workspaceId: "feature",
							branchName: "feature",
							isMain: false,
							worktreePath: "/repo1__wt/feature",
							terminals: [],
							additions: 0,
							deletions: 0,
							...workspace,
						},
					},
				}),
			});
			return render(() => <Sidebar {...defaultProps()} />).container;
		}

		/** The badge answers one question — what would removing this workspace
		 *  lose? Every combination is enumerated because the two inputs are
		 *  independent: the commit verdict never looked at uncommitted files, and
		 *  a workspace sitting on the default tip never merged anything. Reading
		 *  either as "nothing to lose" is how a worktree holding 24 uncommitted
		 *  files came to be labelled Merged. */
		const lifecycleMatrix: Array<{
			commitStatus: string;
			dirtyFiles: number | null;
			lines: number;
			removalSafety: string;
			expected: string | null;
		}> = [
			{ commitStatus: "unknown", dirtyFiles: null, lines: 0, removalSafety: "unknown", expected: "Unknown" },
			{ commitStatus: "in_sync", dirtyFiles: 24, lines: 0, removalSafety: "requires_force", expected: "24 dirty" },
			{ commitStatus: "merged", dirtyFiles: 24, lines: 0, removalSafety: "requires_force", expected: "24 dirty" },
			{ commitStatus: "unmerged", dirtyFiles: 1, lines: 0, removalSafety: "requires_force", expected: "1 dirty" },
			// Tracked lines are already on the row: a second chip repeated the
			// same warning, which is what made the sidebar unreadable.
			{ commitStatus: "in_sync", dirtyFiles: 24, lines: 429, removalSafety: "requires_force", expected: null },
			{ commitStatus: "merged", dirtyFiles: 24, lines: 429, removalSafety: "requires_force", expected: null },
			{ commitStatus: "merged", dirtyFiles: 0, lines: 0, removalSafety: "safe", expected: "Merged" },
			{ commitStatus: "in_sync", dirtyFiles: 0, lines: 0, removalSafety: "safe", expected: null },
			{ commitStatus: "unmerged", dirtyFiles: 0, lines: 0, removalSafety: "safe", expected: null },
		];

		for (const row of lifecycleMatrix) {
			it(`labels ${row.commitStatus} with ${row.dirtyFiles} uncommitted files and ${row.lines} tracked lines as ${row.expected ?? "no badge"}`, () => {
				const container = renderLifecycle({
					additions: row.lines,
					deletions: 0,
					lifecycleStatus: {
						dirtyFiles: row.dirtyFiles,
						commitStatus: row.commitStatus,
						removalSafety: row.removalSafety,
					},
				});
				expect(container.querySelector(".lifecycleBadge")?.textContent ?? null).toBe(row.expected);
			});
		}

		it("shows unmerged commits as a compact marker beside the PR and diff area", () => {
			const container = renderLifecycle({
				additions: 42,
				lifecycleStatus: { dirtyFiles: 0, commitStatus: "unmerged", removalSafety: "safe" },
			});
			const marker = container.querySelector('[aria-label="Unmerged commits"]');
			expect(marker).not.toBeNull();
			expect(marker?.getAttribute("data-tooltip")).toContain("not merged into the default branch");
			// An outlined square read as a glyph that failed to load (Boss, 2026-09-29).
			expect(marker?.textContent).toBe("↑");
			expect(container.querySelector(".lifecycleBadge")).toBeNull();
			expect(container.querySelector(".branchStats")).not.toBeNull();
		});

		it("counts the files a removal would discard instead of calling the tree dirty", () => {
			const container = renderLifecycle({
				lifecycleStatus: { dirtyFiles: 24, commitStatus: "in_sync", removalSafety: "requires_force" },
			});
			const badge = container.querySelector(".lifecycleBadge");
			expect(badge?.textContent).toBe("24 dirty");
			expect(badge?.getAttribute("data-tooltip")).toContain("24 uncommitted files");
			expect(badge?.getAttribute("data-tooltip")).toContain("discarded by removing this workspace");
			expect(badge?.getAttribute("data-tooltip-pos")).toBe("bottom");
			expect(badge?.getAttribute("title")).toBeNull();
		});

		/** Both chips point at uncommitted work, so either one opens the diff of
		 *  that workspace — never the diff of whichever branch was active. */
		it("opens the changes panel for the row when the dirty chip is clicked", () => {
			uiStore.setGitPanelVisible(false);
			const onBranchSelect = vi.fn();
			setRepos({
				"/repo1": makeRepo({
					workspaces: {
						feature: {
							workspaceId: "feature",
							branchName: "feature",
							isMain: false,
							worktreePath: "/repo1__wt/feature",
							terminals: [],
							additions: 0,
							deletions: 0,
							lifecycleStatus: { dirtyFiles: 113, commitStatus: "in_sync", removalSafety: "requires_force" },
						},
					},
				}),
			});
			const { container } = render(() => <Sidebar {...defaultProps({ onBranchSelect })} />);
			const badge = container.querySelector(".lifecycleBadge") as HTMLElement;
			expect(badge.textContent).toBe("113 dirty");
			fireEvent.click(badge);
			expect(onBranchSelect).toHaveBeenCalledTimes(1);
			expect(onBranchSelect).toHaveBeenCalledWith("/repo1", "feature");
			expect(uiStore.state.gitPanelVisible).toBe(true);
			expect(uiStore.state.gitPanelRequestedTab).toBe("changes");
		});

		it("keeps the changes panel open when a chip is clicked while it already shows", () => {
			setRepos({
				"/repo1": makeRepo({
					workspaces: {
						main: { branchName: "main", isMain: true, worktreePath: null, terminals: [], additions: 3, deletions: 1 },
					},
				}),
			});
			uiStore.setGitPanelVisible(false);
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const stats = container.querySelector(".branchStats") as HTMLElement;
			fireEvent.click(stats);
			fireEvent.click(stats);
			expect(uiStore.state.gitPanelVisible).toBe(true);
			expect(uiStore.state.gitPanelRequestedTab).toBe("changes");
		});

		it("gives the Merged chip no click action", () => {
			uiStore.setGitPanelVisible(false);
			const onBranchSelect = vi.fn();
			renderLifecycle({ lifecycleStatus: { dirtyFiles: 0, commitStatus: "merged", removalSafety: "safe" } });
			const badge = document.querySelector(".lifecycleBadge") as HTMLElement;
			expect(badge.textContent).toBe("Merged");
			fireEvent.click(badge);
			expect(uiStore.state.gitPanelVisible).toBe(false);
			expect(badge.getAttribute("role")).toBeNull();
			expect(onBranchSelect).not.toHaveBeenCalled();
		});

		/** When the stats chip takes the row, the count is not dropped — it moves
		 *  into that chip's tooltip, so the answer is still one hover away. */
		it("carries the file count in the stats tooltip when the stats chip is shown", () => {
			const container = renderLifecycle({
				additions: 429,
				deletions: 90,
				lifecycleStatus: { dirtyFiles: 25, commitStatus: "in_sync", removalSafety: "requires_force" },
			});
			expect(container.querySelector(".lifecycleBadge")).toBeNull();
			expect(container.querySelector(".branchStats")?.getAttribute("data-tooltip")).toBe(
				"Tracked line changes: +429 -90 — 25 uncommitted files",
			);
		});

		/** The PR badge takes the row's one chip slot the same way the stats chip
		 *  does — a row carrying `#256 Review` plus `1 dirty` is the two-chip
		 *  crowding this rule exists to stop. */
		it("carries the file count in the PR badge tooltip when a PR badge is shown", () => {
			mockGetPrStatus.mockReturnValue({ state: "OPEN", number: 256, title: "Test", url: "https://example.com" });
			const container = renderLifecycle({
				lifecycleStatus: { dirtyFiles: 1, commitStatus: "in_sync", removalSafety: "requires_force" },
			});
			expect(container.querySelector(".lifecycleBadge")).toBeNull();
			expect(container.querySelector(".prBadge")?.getAttribute("data-tooltip")).toBe("PR #256 — 1 uncommitted file");
		});

		it("explains the Unknown lifecycle badge and includes the inspection error", () => {
			const container = renderLifecycle({
				lifecycleStatus: {
					dirtyFiles: null,
					commitStatus: "unknown",
					removalSafety: "unknown",
					error: "git status failed",
				},
			});
			expect(container.querySelector(".lifecycleBadge")?.getAttribute("data-tooltip")).toBe(
				"Status unavailable: TUICommander could not verify local changes or merge state, so removal is blocked. git status failed",
			);
		});

		/** A main checkout is not removable from this list, so the whole question
		 *  the badge answers is moot — and answering it anyway put a badge on
		 *  nearly every repository in the sidebar. */
		it("never badges a main checkout, however dirty or merged it is", () => {
			for (const lifecycleStatus of [
				{ dirtyFiles: 12, commitStatus: "in_sync", removalSafety: "requires_force" },
				{ dirtyFiles: 0, commitStatus: "merged", removalSafety: "safe" },
				{ dirtyFiles: null, commitStatus: "unknown", removalSafety: "unknown" },
			]) {
				setRepos({
					"/repo1": makeRepo({
						workspaces: {
							main: {
								workspaceId: "main",
								branchName: "main",
								isMain: true,
								worktreePath: null,
								terminals: [],
								additions: 0,
								deletions: 0,
								lifecycleStatus,
							},
						},
					}),
				});
				const { container } = render(() => <Sidebar {...defaultProps()} />);
				expect(container.querySelector(".lifecycleBadge")).toBeNull();
			}
		});

		it("shows StatsBadge when only additions > 0", () => {
			setRepos({
				"/repo1": makeRepo({
					workspaces: {
						main: { branchName: "main", isMain: true, worktreePath: null, terminals: [], additions: 5, deletions: 0 },
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
					workspaces: {
						main: { branchName: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 3 },
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
					workspaces: {
						main: { branchName: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
					},
				}),
			});
			mockGetPrStatus.mockReturnValue({ state: "OPEN", number: 123, title: "Test PR", url: "https://example.com" });
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const prBadge = container.querySelector(".prBadge");
			expect(prBadge).not.toBeNull();
			expect(prBadge!.getAttribute("data-tooltip")).toBe("PR #123");
		});

		it("shows the PR number beside the diff, never alternating the two", () => {
			setRepos({
				"/repo1": makeRepo({
					workspaces: {
						main: {
							branchName: "main",
							isMain: true,
							worktreePath: null,
							terminals: [],
							additions: 12,
							deletions: 3,
						},
					},
				}),
			});
			mockGetPrStatus.mockReturnValue({
				state: "OPEN",
				number: 256,
				title: "Review me",
				url: "https://example.com",
				review_decision: "REVIEW_REQUIRED",
			});

			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const stack = container.querySelector(".branchBadgeStack");
			expect(stack).not.toBeNull();
			// Both stay rendered and static: a value that disappears half the time
			// cannot be read at a glance, and the diff is a click target.
			expect(stack!.classList.contains("branchBadgeStackAlternating")).toBe(false);
			expect(stack!.querySelector(".prBadge")?.textContent).toBe("#256");
			expect(stack!.querySelector(".prBadge")?.getAttribute("data-tooltip")).toBe("PR #256 · Review");
			expect(stack!.querySelector(".branchStats")?.textContent).toBe("+12-3");
		});

		it.each([
			{
				name: "mergeability checking",
				pr: { conflict_state: "checking" },
				label: "PR #256 · Checking",
			},
			{
				name: "conflicts",
				pr: { conflict_state: "conflicting" },
				label: "PR #256 · Conflicts",
			},
			{
				name: "CI running",
				pr: {},
				checks: { passed: 0, failed: 0, pending: 1 },
				label: "PR #256 · CI Running",
			},
		])("keeps the diff visible beside a PR in the $name state", ({ pr, checks, label }) => {
			setRepos({
				"/repo1": makeRepo({
					workspaces: {
						main: {
							branchName: "main",
							isMain: true,
							worktreePath: null,
							terminals: [],
							additions: 12,
							deletions: 3,
						},
					},
				}),
			});
			mockGetPrStatus.mockReturnValue({
				state: "OPEN",
				number: 256,
				title: "Review me",
				url: "https://example.com",
				...pr,
			});
			mockGetCheckSummary.mockReturnValue(checks ?? null);

			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const stack = container.querySelector(".branchBadgeStack");
			expect(stack).not.toBeNull();
			// The urgent states used to pin the PR and hide the diff outright.
			expect(stack!.querySelector(".branchStats")?.textContent).toBe("+12-3");
			expect(stack!.querySelector(".prBadge")?.getAttribute("data-tooltip")).toBe(label);
		});

		it("does not show PrStateBadge when branch has no PR data", () => {
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const prBadge = container.querySelector(".prBadge");
			expect(prBadge).toBeNull();
		});

		it("shows the merged marker and moves the Merged label into the tooltip", () => {
			setRepos({
				"/repo1": makeRepo({
					workspaces: {
						main: { branchName: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
					},
				}),
			});
			mockGetPrStatus.mockReturnValue({ state: "MERGED", number: 42, title: "Test", url: "https://example.com" });
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const prBadge = container.querySelector(".prBadge");
			expect(prBadge).not.toBeNull();
			expect(prBadge!.classList.contains("prMarkMerged")).toBe(true);
			expect(prBadge!.textContent).toBe("#42");
			expect(prBadge!.getAttribute("data-tooltip")).toBe("PR #42 · Merged");
		});

		it("hides PR badge immediately for CLOSED PR", () => {
			setRepos({
				"/repo1": makeRepo({
					workspaces: {
						main: { branchName: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
					},
				}),
			});
			mockGetPrStatus.mockReturnValue({ state: "CLOSED", number: 43, title: "Test", url: "https://example.com" });
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const prBadge = container.querySelector(".prBadge");
			expect(prBadge).toBeNull();
		});

		it("shows the draft marker and moves the Draft label into the tooltip", () => {
			setRepos({
				"/repo1": makeRepo({
					workspaces: {
						main: { branchName: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
					},
				}),
			});
			mockGetPrStatus.mockReturnValue({
				state: "OPEN",
				number: 45,
				title: "Draft PR",
				url: "https://example.com",
				is_draft: true,
			});
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const prBadge = container.querySelector(".prBadge");
			expect(prBadge).not.toBeNull();
			expect(prBadge!.classList.contains("prMarkDraft")).toBe(true);
			expect(prBadge!.textContent).toBe("#45");
			expect(prBadge!.getAttribute("data-tooltip")).toBe("PR #45 · Draft");
		});

		it("shows the open marker with the PR number when state is OPEN with no special conditions", () => {
			setRepos({
				"/repo1": makeRepo({
					workspaces: {
						main: { branchName: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
					},
				}),
			});
			mockGetPrStatus.mockReturnValue({ state: "OPEN", number: 44, title: "Test", url: "https://example.com" });
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const prBadge = container.querySelector(".prBadge");
			expect(prBadge).not.toBeNull();
			expect(prBadge!.classList.contains("prMarkOpen")).toBe(true);
			expect(prBadge!.textContent).toBe("#44");
		});
	});

	describe("branch activity indicator (via isBusy)", () => {
		it("does not add hasActivity class (activity flag no longer drives sidebar)", () => {
			setRepos({
				"/repo1": makeRepo({
					workspaces: {
						main: {
							workspaceId: "main",
							branchName: "main",
							isMain: true,
							worktreePath: null,
							terminals: ["t1"],
							additions: 0,
							deletions: 0,
						},
					},
				}),
			});
			mockTerminalsGet.mockReturnValue({ activity: true });
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const branchItem = container.querySelector(".branchItem");
			expect(branchItem!.classList.contains("hasActivity")).toBe(false);
		});
	});

	describe("branch tab list", () => {
		// The nested terminal-tab list is opt-in; this block verifies behavior with it enabled.
		beforeEach(() => {
			settingsStore.setTabTreeEnabled(true);
		});

		/** Find the .branchItem row whose name matches. */
		function branchRow(container: HTMLElement, name: string): HTMLElement {
			const label = Array.from(container.querySelectorAll(".branchName")).find((el) => el.textContent === name);
			const row = label?.closest(".branchItem");
			if (!row) throw new Error(`branch row "${name}" not found`);
			return row as HTMLElement;
		}

		it("marks every branch with at least one terminal as expandable", () => {
			setRepos({
				"/repo1": makeRepo({
					workspaces: {
						main: {
							workspaceId: "main",
							branchName: "main",
							isMain: true,
							worktreePath: null,
							terminals: ["t1"],
							additions: 0,
							deletions: 0,
						},
						"feature/x": {
							workspaceId: "feature/x",
							branchName: "feature/x",
							isMain: false,
							worktreePath: "/wt/x",
							terminals: ["t2", "t3"],
							additions: 0,
							deletions: 0,
						},
						"feature/empty": {
							workspaceId: "feature/empty",
							branchName: "feature/empty",
							isMain: false,
							worktreePath: "/wt/empty",
							terminals: [],
							additions: 0,
							deletions: 0,
						},
					},
				}),
			});
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			// Both occupied branches can reveal activity; the empty branch cannot.
			expect(container.querySelectorAll(".branchItem .branchIconToggle[aria-expanded]").length).toBe(2);
		});

		it("renders one subitem per terminal when expanded and >1 terminal", () => {
			mockTerminalsGet.mockImplementation(() => ({
				name: "term",
				shellState: "idle",
				unseen: false,
				awaitingInput: null,
			}));
			setRepos({
				"/repo1": makeRepo({
					workspaces: {
						main: {
							workspaceId: "main",
							branchName: "main",
							isMain: true,
							worktreePath: null,
							terminals: ["t1", "t2"],
							additions: 0,
							deletions: 0,
						},
					},
				}),
			});
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			expect(container.querySelectorAll(".branchTabItem").length).toBe(2);
		});

		it("shows every branch's agents by default and hides a collapsed one", () => {
			mockTerminalsGet.mockImplementation(() => ({
				name: "term",
				shellState: "idle",
				unseen: false,
				awaitingInput: null,
			}));
			setRepos({
				"/repo1": makeRepo({
					workspaces: {
						"feature/open": {
							workspaceId: "feature/open",
							branchName: "feature/open",
							isMain: false,
							worktreePath: "/wt/open",
							terminals: ["t1"],
							additions: 0,
							deletions: 0,
						},
						"feature/closed": {
							workspaceId: "feature/closed",
							branchName: "feature/closed",
							isMain: false,
							worktreePath: "/wt/closed",
							terminals: ["t2", "t3"],
							additions: 0,
							deletions: 0,
							tabsCollapsed: true,
						},
					},
				}),
			});
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			// Only the untouched branch renders its one terminal; the collapsed one renders none.
			expect(container.querySelectorAll(".branchTabItem").length).toBe(1);
			expect(
				branchRow(container, "feature/open").querySelector(".branchIconToggle")?.getAttribute("aria-expanded"),
			).toBe("true");
			expect(
				branchRow(container, "feature/closed").querySelector(".branchIconToggle")?.getAttribute("aria-expanded"),
			).toBe("false");
		});

		it("renders expanded terminal activity as one branch card", () => {
			densityMode("auto");
			vi.setSystemTime(new Date("2026-09-22T12:10:00Z"));
			mockTerminalsGet.mockImplementation((id: string) => {
				const terminals = {
					t1: {
						name: "Claude checkout",
						agentType: "claude",
						agentIntent: "coordinating checkout validation",
						currentTask: null,
						lastPrompt: null,
						lastDataAt: Date.now() - 30_000,
						lastActivityAt: Date.now() - 70 * 60_000,
						shellState: "busy",
						agentState: "working",
						backgroundWork: false,
						sessionId: "s1",
						unseen: false,
						awaitingInput: null,
					},
					t2: {
						name: "Codex taxes",
						agentType: "codex",
						agentIntent: null,
						currentTask: "running EU tax validation",
						lastPrompt: null,
						lastDataAt: Date.now() - 14 * 60_000,
						lastActivityAt: Date.now() - 14 * 60_000,
						shellState: "busy",
						agentState: "working",
						backgroundWork: false,
						sessionId: "s2",
						unseen: false,
						awaitingInput: null,
					},
					t3: {
						name: "Gemini handoff",
						agentType: "gemini",
						agentIntent: null,
						currentTask: null,
						lastPrompt: "preparing the checkout handoff",
						lastDataAt: Date.now() - 31 * 60_000,
						lastActivityAt: Date.now() - 31 * 60_000,
						shellState: "idle",
						agentState: "idle",
						backgroundWork: false,
						sessionId: "s3",
						unseen: false,
						awaitingInput: null,
					},
				};
				return terminals[id as keyof typeof terminals];
			});
			setRepos({
				"/repo1": makeRepo({
					workspaces: {
						main: {
							workspaceId: "main",
							branchName: "main",
							isMain: true,
							worktreePath: null,
							terminals: ["t1", "t2", "t3"],
							additions: 0,
							deletions: 0,
						},
					},
				}),
			});

			const { container } = render(() => <Sidebar {...defaultProps()} />);

			// The row reads like its tab: the tab title. What the agent is doing
			// stays in the tooltip and the accessible label below.
			expect(Array.from(container.querySelectorAll(".branchAgentActivity"), (el) => el.textContent)).toEqual([
				"Claude checkout",
				"Codex taxes",
				"Gemini handoff",
			]);
			expect(container.querySelectorAll(".branchAgentName")).toHaveLength(0);
			expect(Array.from(container.querySelectorAll(".branchTabItem"), (el) => el.getAttribute("aria-label"))).toEqual([
				"Claude checkout: coordinating checkout validation",
				"Codex taxes: running EU tax validation",
				"Gemini handoff: preparing the checkout handoff",
			]);
			expect(Array.from(container.querySelectorAll(".branchAgentTime"), (el) => el.textContent)).toEqual([
				"1h",
				"14m",
				"31m",
			]);
			vi.setSystemTime(Date.now() + 60 * 60_000);
			vi.advanceTimersByTime(60_000);
			expect(Array.from(container.querySelectorAll(".branchAgentTime"), (el) => el.textContent)).toEqual([
				"2h",
				"1h",
				"1h",
			]);
			expect(container.querySelectorAll(".branchAgentIcon svg")).toHaveLength(3);

			fireEvent.click(container.querySelectorAll(".branchTabItem")[1]);
			expect(mockNavigateToTerminal).toHaveBeenCalledWith("t2");
		});

		it("shows an idle session row when the tab is finished despite stale working agent state", () => {
			mockTerminalsGet.mockReturnValue({
				name: "tuic-backlog",
				agentType: "codex",
				sessionId: "089ffa34",
				shellState: "idle",
				agentState: "working",
				backgroundWork: false,
				awaitingInput: null,
				unseen: false,
				lastActivityAt: Date.now() - 70 * 60_000,
			});
			setRepos({
				"/repo1": makeRepo({
					workspaces: {
						main: {
							workspaceId: "main",
							branchName: "main",
							isMain: true,
							worktreePath: null,
							terminals: ["t1"],
							additions: 0,
							deletions: 0,
						},
					},
				}),
			});

			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const dot = container.querySelector(".branchTabItem .branchTabDot");
			expect(dot?.classList.contains("branchTabDotIdle")).toBe(true);
			expect(dot?.classList.contains("branchTabDotBusy")).toBe(false);
		});

		it("tags a sub-agent row with its parent and leaves other rows untagged", () => {
			mockTerminalsGet.mockImplementation((id: string) => ({
				name: id === "t1" ? "Orchestrator" : "Worker",
				agentType: "claude",
				agentIntent: null,
				currentTask: null,
				lastPrompt: null,
				lastDataAt: null,
				shellState: "idle",
				agentState: "idle",
				backgroundWork: false,
				sessionId: id,
				unseen: false,
				awaitingInput: null,
			}));
			mockGetSubAgentTag.mockImplementation((id: string) => (id === "t2" ? "Orchestrator" : null));
			setRepos({
				"/repo1": makeRepo({
					workspaces: {
						main: {
							workspaceId: "main",
							branchName: "main",
							isMain: true,
							worktreePath: null,
							terminals: ["t1", "t2"],
							additions: 0,
							deletions: 0,
						},
					},
				}),
			});

			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const rows = container.querySelectorAll(".branchTabItem");

			// Only the spawned worker says who launched it; the orchestrator row stays plain.
			expect(rows[0].querySelector(".branchSubAgentTag")).toBeNull();
			expect(rows[1].querySelector(".branchSubAgentTag")?.getAttribute("title")).toBe("Spawned by Orchestrator");
			expect(rows[1].querySelector(".branchSubAgentTag svg")).not.toBeNull();
			// The name is on the tag itself, so a screen reader announces it.
			expect(rows[1].querySelector(".branchSubAgentTag")?.getAttribute("aria-label")).toBe("Spawned by Orchestrator");
		});

		// Catches: the passive robot and actionable GitHub badge sharing the accent color.
		it("uses muted metadata color for the sub-agent tag while GitHub keeps the accent", () => {
			const css = readFileSync("src/components/Sidebar/Sidebar.module.css", "utf8");
			const colorToken = (className: string) => {
				const body = css.split(`\n.${className} {`)[1]?.split("}")[0];
				expect(body, `${className} rule exists`).toBeDefined();
				return body?.match(/\bcolor:\s*(var\(--[\w-]+\))/)?.[1];
			};
			expect(colorToken("branchSubAgentTag")).toBe(colorToken("branchAgentTime"));
			expect(colorToken("ghBadgeBtn")).toBe("var(--accent)");
			expect(colorToken("branchSubAgentTag")).not.toBe(colorToken("ghBadgeBtn"));
		});

		it("renders the activity card for a single terminal", () => {
			mockTerminalsGet.mockImplementation(() => ({
				name: "term",
				shellState: "idle",
				unseen: false,
				awaitingInput: null,
			}));
			setRepos({
				"/repo1": makeRepo({
					workspaces: {
						main: {
							workspaceId: "main",
							branchName: "main",
							isMain: true,
							worktreePath: null,
							terminals: ["t1"],
							additions: 0,
							deletions: 0,
						},
					},
				}),
			});
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			expect(container.querySelectorAll(".branchTabItem").length).toBe(1);
			expect(container.querySelector(".branchAgentActivity")?.textContent).toBe("term");
		});

		// Rule (AGENTS.md "Sidebar clicks"): a row click opens the branch and
		// nothing else. Only the separate chevron expands or collapses the agents.
		it("row click on the already-active branch opens it and never toggles the agents", () => {
			setRepos(
				{
					"/repo1": makeRepo({
						activeWorkspaceId: "main",
						workspaces: {
							main: {
								workspaceId: "main",
								branchName: "main",
								isMain: true,
								worktreePath: null,
								terminals: ["t1", "t2"],
								additions: 0,
								deletions: 0,
							},
						},
					}),
				},
				"/repo1",
			);
			const onBranchSelect = vi.fn();
			const { container } = render(() => <Sidebar {...defaultProps({ onBranchSelect })} />);

			fireEvent.click(branchRow(container, "main"));

			expect(onBranchSelect).toHaveBeenCalledWith("/repo1", "main");
			expect(mockToggleBranchTabsCollapsed).not.toHaveBeenCalled();
		});

		it("row click on an inactive branch opens it and never expands the agents", () => {
			setRepos(
				{
					"/repo1": makeRepo({
						activeWorkspaceId: "main",
						workspaces: {
							main: {
								workspaceId: "main",
								branchName: "main",
								isMain: true,
								worktreePath: null,
								terminals: ["t1"],
								additions: 0,
								deletions: 0,
							},
							"feature/x": {
								workspaceId: "feature/x",
								branchName: "feature/x",
								isMain: false,
								worktreePath: "/wt/x",
								terminals: ["t2", "t3"],
								additions: 0,
								deletions: 0,
							},
						},
					}),
				},
				"/repo1",
			);
			const onBranchSelect = vi.fn();
			const { container } = render(() => <Sidebar {...defaultProps({ onBranchSelect })} />);

			fireEvent.click(branchRow(container, "feature/x"));

			expect(onBranchSelect).toHaveBeenCalledWith("/repo1", "feature/x");
			expect(mockToggleBranchTabsCollapsed).not.toHaveBeenCalled();
		});

		it("chevron click prevents unintended branch selection when toggling agents", () => {
			setRepos(
				{
					"/repo1": makeRepo({
						activeWorkspaceId: "main",
						workspaces: {
							main: {
								workspaceId: "main",
								branchName: "main",
								isMain: true,
								worktreePath: null,
								terminals: ["t1"],
								additions: 0,
								deletions: 0,
							},
							"feature/x": {
								workspaceId: "feature/x",
								branchName: "feature/x",
								isMain: false,
								worktreePath: "/wt/x",
								terminals: ["t2"],
								additions: 0,
								deletions: 0,
							},
						},
					}),
				},
				"/repo1",
			);
			const onBranchSelect = vi.fn();
			const { container } = render(() => <Sidebar {...defaultProps({ onBranchSelect })} />);

			const row = branchRow(container, "feature/x");
			const icon = row.querySelector(".branchIcon")!;
			expect(icon.closest("button")).toBeNull();
			fireEvent.click(icon);
			expect(onBranchSelect).toHaveBeenCalledWith("/repo1", "feature/x");
			expect(mockToggleBranchTabsCollapsed).not.toHaveBeenCalled();
			onBranchSelect.mockClear();
			fireEvent.click(row.querySelector(".branchIconToggle")!);

			expect(mockToggleBranchTabsCollapsed).toHaveBeenCalledWith("/repo1", "feature/x");
			expect(onBranchSelect).not.toHaveBeenCalled();
		});

		it("chevron handles Enter and Space without selecting the branch", () => {
			setRepos({
				"/repo1": makeRepo({
					workspaces: {
						main: {
							workspaceId: "main",
							branchName: "main",
							isMain: true,
							worktreePath: null,
							terminals: ["t1"],
							additions: 0,
							deletions: 0,
						},
					},
				}),
			});
			const { container } = render(() => <Sidebar {...defaultProps()} />);

			fireEvent.keyDown(branchRow(container, "main").querySelector(".branchIconToggle")!, { key: "Enter" });
			fireEvent.keyDown(branchRow(container, "main").querySelector(".branchIconToggle")!, { key: " " });
			expect(mockToggleBranchTabsCollapsed).toHaveBeenCalledTimes(2);

			expect(mockToggleBranchTabsCollapsed).toHaveBeenCalledWith("/repo1", "main");
		});

		it("collapsed chevron retains the session count beside the disclosure", () => {
			setRepos({
				"/repo1": makeRepo({
					workspaces: {
						"feature/closed": {
							workspaceId: "feature/closed",
							branchName: "feature/closed",
							isMain: false,
							worktreePath: "/wt/closed",
							terminals: ["t1", "t2", "t3"],
							additions: 0,
							deletions: 0,
							tabsCollapsed: true,
						},
					},
				}),
			});
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const toggle = branchRow(container, "feature/closed").querySelector(".branchIconToggle")!;

			// A collapsed list is exactly when the count is the only trace of the sessions.
			expect(toggle.querySelector(".branchAgentCount")?.textContent).toBe("3");
			expect(toggle.getAttribute("aria-label")).toContain("3");
		});

		it("expanded chevron hides the redundant session count", () => {
			setRepos({
				"/repo1": makeRepo({
					workspaces: {
						"feature/open": {
							workspaceId: "feature/open",
							branchName: "feature/open",
							isMain: false,
							worktreePath: "/wt/open",
							terminals: ["t1", "t2", "t3"],
							additions: 0,
							deletions: 0,
							tabsCollapsed: false,
						},
					},
				}),
			});
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const toggle = branchRow(container, "feature/open").querySelector(".branchIconToggle")!;

			// The expanded rows already show every session; a badge would repeat them.
			expect(toggle.querySelector(".branchAgentCount")).toBeNull();
			// Screen readers keep the count: the toggle label still carries it.
			expect(toggle.getAttribute("aria-label")).toContain("3");
		});
	});

	describe("branch tab list (gating: setting off)", () => {
		// Global beforeEach resets tabTreeEnabled to false, so the feature is inert here.
		/** Find the .branchItem row whose name matches. */
		function branchRow(container: HTMLElement, name: string): HTMLElement {
			const label = Array.from(container.querySelectorAll(".branchName")).find((el) => el.textContent === name);
			const row = label?.closest(".branchItem");
			if (!row) throw new Error(`branch row "${name}" not found`);
			return row as HTMLElement;
		}

		it("marks no branch expandable even when it has more than one terminal", () => {
			setRepos({
				"/repo1": makeRepo({
					workspaces: {
						"feature/x": {
							workspaceId: "feature/x",
							branchName: "feature/x",
							isMain: false,
							worktreePath: "/wt/x",
							terminals: ["t2", "t3"],
							additions: 0,
							deletions: 0,
						},
					},
				}),
			});
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			expect(container.querySelectorAll(".branchIconToggle").length).toBe(0);
		});

		it("renders no subitems even when expanded with more than one terminal", () => {
			mockTerminalsGet.mockImplementation(() => ({
				name: "term",
				shellState: "idle",
				unseen: false,
				awaitingInput: null,
			}));
			setRepos({
				"/repo1": makeRepo({
					workspaces: {
						main: {
							workspaceId: "main",
							branchName: "main",
							isMain: true,
							worktreePath: null,
							terminals: ["t1", "t2"],
							additions: 0,
							deletions: 0,
						},
					},
				}),
			});
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			expect(container.querySelectorAll(".branchTabItem").length).toBe(0);
		});

		it("row click selects the branch but never toggles the tab list", () => {
			setRepos(
				{
					"/repo1": makeRepo({
						activeWorkspaceId: "main",
						workspaces: {
							main: {
								workspaceId: "main",
								branchName: "main",
								isMain: true,
								worktreePath: null,
								terminals: ["t1", "t2"],
								additions: 0,
								deletions: 0,
							},
						},
					}),
				},
				"/repo1",
			);
			const onBranchSelect = vi.fn();
			const { container } = render(() => <Sidebar {...defaultProps({ onBranchSelect })} />);

			fireEvent.click(branchRow(container, "main"));

			expect(onBranchSelect).toHaveBeenCalledWith("/repo1", "main");
			expect(mockToggleBranchTabsCollapsed).not.toHaveBeenCalled();
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
			// Copy Path, Add Terminal, Set Label…, Rename Branch (no Delete Worktree for main)
			expect(items.length).toBe(4);
		});

		it("context menu Copy Path action copies worktreePath to clipboard", async () => {
			// In Tauri mode writeClipboard() routes through the native clipboard
			// command (WKWebView rejects navigator.clipboard.writeText), so assert on the invoke.
			mockInvoke.mockClear();

			setRepos({
				"/repo1": makeRepo({
					workspaces: {
						main: {
							workspaceId: "main",
							branchName: "main",
							isMain: true,
							worktreePath: "/path/to/repo",
							terminals: [],
							additions: 0,
							deletions: 0,
						},
					},
				}),
			});
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const branchItem = container.querySelector(".branchItem")!;
			fireEvent.contextMenu(branchItem, { clientX: 100, clientY: 200 });

			const contextMenu = container.querySelector(".menu");
			const items = contextMenu!.querySelectorAll(".item");
			// Find "Copy Path" item
			const copyPathItem = Array.from(items).find((i) => i.querySelector(".label")?.textContent === "Copy Path")!;
			fireEvent.click(copyPathItem);

			// The action is async (writeClipboard → native clipboard command)
			await vi.waitFor(() => {
				expect(mockInvoke).toHaveBeenCalledWith("write_clipboard_text", {
					text: "/path/to/repo",
				});
			});
		});

		describe("context menu Open in GitHub (#1323-71b1)", () => {
			const openInGitHub = async (branchName: string) => {
				mockInvoke.mockImplementation(async (cmd: string) =>
					cmd === "get_remote_url" ? "git@github.com:acme/agent2.git" : undefined,
				);
				setRepos({
					"/repo1": makeRepo({
						workspaces: {
							b: {
								workspaceId: "b",
								branchName,
								isMain: false,
								worktreePath: "/wt/b",
								terminals: [],
								additions: 0,
								deletions: 0,
							},
						},
					}),
				});
				const { container } = render(() => <Sidebar {...defaultProps()} />);
				fireEvent.contextMenu(container.querySelector(".branchItem")!, { clientX: 1, clientY: 1 });
				const labelEls = () => Array.from(container.querySelectorAll(".menu .label"));
				// The remote URL resolves asynchronously; the item appears once it has.
				await vi.waitFor(() => expect(labelEls().map((l) => l.textContent)).toContain("Open in GitHub"));
				const item = labelEls().find((l) => l.textContent === "Open in GitHub")!;
				const labels = labelEls().map((l) => l.textContent);
				fireEvent.click(item.closest(".item") ?? item);
				return labels;
			};

			afterEach(() => mockInvoke.mockReset().mockResolvedValue(undefined));

			it("opens the PR URL, not the tree or repo root, for a branch with a PR", async () => {
				mockGetPrStatus.mockReturnValue({
					state: "OPEN",
					number: 1143,
					title: "t",
					url: "https://github.com/acme/agent2/pull/1143",
				});
				const labels = await openInGitHub("POC-00001/installed-app");
				expect(mockOpenUrl).toHaveBeenCalledExactlyOnceWith("https://github.com/acme/agent2/pull/1143");
				expect(labels).not.toContain("Open PR");
			});

			it("opens the branch tree URL for a branch without a PR", async () => {
				await openInGitHub("POC-00001/installed-app");
				expect(mockOpenUrl).toHaveBeenCalledExactlyOnceWith(
					"https://github.com/acme/agent2/tree/POC-00001%2Finstalled-app",
				);
			});
		});

		it("context menu Copy Path is disabled when no worktreePath", () => {
			setRepos({
				"/repo1": makeRepo({
					workspaces: {
						main: { branchName: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
					},
				}),
			});
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const branchItem = container.querySelector(".branchItem")!;
			fireEvent.contextMenu(branchItem, { clientX: 100, clientY: 200 });

			const contextMenu = container.querySelector(".menu");
			const items = contextMenu!.querySelectorAll(".item");
			const copyPathItem = Array.from(items).find((i) => i.querySelector(".label")?.textContent === "Copy Path")!;
			expect(copyPathItem.hasAttribute("disabled")).toBe(true);
		});

		it("does not show Delete Worktree for non-main branch without worktreePath", () => {
			setRepos({
				"/repo1": makeRepo({
					workspaces: {
						main: { branchName: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
						"feature/y": {
							workspaceId: "feature/y",
							branchName: "feature/y",
							isMain: false,
							worktreePath: null,
							terminals: [],
							additions: 0,
							deletions: 0,
						},
					},
				}),
			});
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const branchItems = container.querySelectorAll(".branchItem");
			fireEvent.contextMenu(branchItems[1], { clientX: 100, clientY: 200 });

			const contextMenu = container.querySelector(".menu");
			const items = contextMenu!.querySelectorAll(".item");
			// Copy Path, Add Terminal, Set Label…, Rename Branch (NO Delete Worktree)
			expect(items.length).toBe(4);
			const labels = Array.from(items).map((i) => i.querySelector(".label")!.textContent);
			expect(labels).not.toContain("Delete Worktree");
		});

		it("shows Delete Worktree option for non-main branch context menu", () => {
			setRepos({
				"/repo1": makeRepo({
					workspaces: {
						main: { branchName: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
						"feature/x": {
							workspaceId: "feature/x",
							branchName: "feature/x",
							isMain: false,
							worktreePath: "/wt/x",
							terminals: [],
							additions: 0,
							deletions: 0,
						},
					},
				}),
			});
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const branchItems = container.querySelectorAll(".branchItem");
			// feature/x is second (sorted after main)
			fireEvent.contextMenu(branchItems[1], { clientX: 100, clientY: 200 });

			const contextMenu = container.querySelector(".menu")!;
			// Branch git ops were condensed into a "Branch ›" submenu (commit 37d95eaa),
			// so top-level is Copy Path, Add Terminal, Set Label, Branch — and
			// Delete Worktree lives inside the Branch submenu, not at top level.
			const topLabels = Array.from(contextMenu.querySelectorAll(".item")).map(
				(i) => i.querySelector(".label")!.textContent,
			);
			expect(topLabels).toContain("Branch");
			expect(topLabels).not.toContain("Delete Worktree");

			// Open the "Branch" submenu (hover) and assert Delete Worktree is inside.
			const branchWrap = Array.from(contextMenu.querySelectorAll(".itemWrap")).find(
				(w) => w.querySelector(".label")!.textContent === "Branch",
			);
			expect(branchWrap).toBeTruthy();
			fireEvent.mouseEnter(branchWrap!);
			const submenu = contextMenu.querySelector(".submenu")!;
			const subLabels = Array.from(submenu.querySelectorAll(".item")).map(
				(i) => i.querySelector(".label")!.textContent,
			);
			expect(subLabels).toContain("Delete Worktree");
		});
	});

	describe("group sections", () => {
		// Catches: flattening groups, moving repos between groups, or orphan Idle headings after filtering.
		it("grouped_repo_activity_split_preserves_membership", () => {
			densityMode("auto");
			const idle = makeRepo({ path: "/idle" });
			const active = makeRepo({
				path: "/active",
				workspaces: { main: { ...idle.workspaces.main, terminals: ["t1"] } },
			});
			const secondIdle = makeRepo({ path: "/second" });
			const loose = makeRepo({ path: "/loose" });
			setRepos({ "/idle": idle, "/active": active, "/second": secondIdle, "/loose": loose });
			mockGetGroupedLayout.mockReturnValue({
				groups: [
					{
						group: { id: "g1", name: "Mixed", collapsed: false, color: "", repoOrder: ["/idle", "/active"] },
						repos: [idle, active],
					},
					{
						group: { id: "g2", name: "Idle only", collapsed: false, color: "", repoOrder: ["/second"] },
						repos: [secondIdle],
					},
				],
				ungrouped: [loose],
			});
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const paths = (id: string) =>
				[...container.querySelectorAll(`[data-sidebar-group='${id}'] [data-sidebar-repo]`)].map((e) =>
					e.getAttribute("data-sidebar-repo"),
				);
			expect(paths("g1")).toEqual(["/active", "/idle"]);
			expect(paths("g2")).toEqual(["/second"]);
			expect(container.querySelectorAll("[data-testid='idle-repos-heading']")).toHaveLength(3);
			uiStore.setRepoFilterActiveOnly(true);
			try {
				expect(paths("g1")).toEqual(["/active"]);
				expect(container.querySelector("[data-sidebar-group='g2']")).toBeNull();
				expect(container.querySelector("[data-testid='idle-repos-heading']")).toBeNull();
			} finally {
				uiStore.setRepoFilterActiveOnly(false);
			}
			expect(paths("g1")).toEqual(["/active", "/idle"]);
			expect(paths("g2")).toEqual(["/second"]);
			expect(mockMoveRepoBetweenGroups).not.toHaveBeenCalled();
			expect(mockReorderRepoInGroup).not.toHaveBeenCalled();
		});

		it("renders group headers with name and chevron", () => {
			const repo = makeRepo();
			setRepos({ "/repo1": repo });
			mockGetGroupedLayout.mockReturnValue({
				groups: [
					{
						group: { id: "g1", name: "Work", color: "", collapsed: false, repoOrder: ["/repo1"] },
						repos: [repo],
					},
				],
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
				groups: [
					{
						group: { id: "g1", name: "Work", color: "", collapsed: false, repoOrder: ["/repo1"] },
						repos: [repo],
					},
				],
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
				groups: [
					{
						group: { id: "g1", name: "Work", color: "", collapsed: false, repoOrder: ["/repo1"] },
						repos: [repo],
					},
				],
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
				groups: [
					{
						group: { id: "g1", name: "Work", color: "", collapsed: true, repoOrder: ["/repo1"] },
						repos: [repo],
					},
				],
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
				groups: [
					{
						group: { id: "g1", name: "Work", color: "#4A9EFF", collapsed: false, repoOrder: ["/repo1"] },
						repos: [repo],
					},
				],
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
				groups: [
					{
						group: { id: "g1", name: "Work", color: "", collapsed: false, repoOrder: ["/repo1"] },
						repos: [repo],
					},
				],
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
				groups: [
					{
						group: { id: "g1", name: "Work", color: "", collapsed: false, repoOrder: ["/repo1", "/repo2"] },
						repos: [repo1, repo2],
					},
				],
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
				groups: [
					{
						group: { id: "g1", name: "Work", color: "", collapsed: false, repoOrder: ["/repo1"] },
						repos: [repo1],
					},
				],
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
				groups: [
					{
						group: { id: "g1", name: "Empty Group", color: "", collapsed: false, repoOrder: [] },
						repos: [],
					},
				],
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
				groups: [
					{
						group: { id: "g1", name: "Work", color: "", collapsed: false, repoOrder: ["/repo1"] },
						repos: [repo],
					},
				],
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
				groups: [
					{
						group: { id: "g1", name: "Work", color: "", collapsed: false, repoOrder: ["/repo1"] },
						repos: [repo],
					},
				],
				ungrouped: [],
			});
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const header = container.querySelector(".groupHeader")!;
			fireEvent.contextMenu(header, { clientX: 100, clientY: 200 });
			const menuItems = container.querySelectorAll(".menu .item");
			const deleteItem = Array.from(menuItems).find(
				(el) => el.querySelector(".label")?.textContent === "Delete Group",
			)!;
			fireEvent.click(deleteItem);
			expect(mockDeleteGroup).toHaveBeenCalledWith("g1");
		});

		it("repo header right-click includes Move to Group with submenu", () => {
			const repo = makeRepo();
			setRepos({ "/repo1": repo });
			mockGetGroupedLayout.mockReturnValue({
				groups: [
					{
						group: { id: "g1", name: "Work", color: "", collapsed: false, repoOrder: [] },
						repos: [],
					},
				],
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

	describe("drag-and-drop (pointer-based)", () => {
		it("pointerDown on repo section initiates drag after movement threshold", () => {
			const repo = makeRepo();
			setRepos({ "/repo1": repo });
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const repoSection = container.querySelector(".repoSection")!;

			// pointerDown alone should not add dragging class
			fireEvent.pointerDown(repoSection, { button: 0, pointerId: 1, clientX: 10, clientY: 10 });
			expect(repoSection.classList.contains("dragging")).toBe(false);

			// Move past threshold — dragging class appears
			fireEvent.pointerMove(document, { pointerId: 1, clientX: 20, clientY: 10 });
			expect(repoSection.classList.contains("dragging")).toBe(true);

			// Cleanup
			fireEvent.pointerUp(document, { pointerId: 1, clientX: 20, clientY: 10 });
			expect(repoSection.classList.contains("dragging")).toBe(false);
		});

		it("pointerDown on group header initiates group drag after threshold", () => {
			const repo = makeRepo();
			setRepos({ "/repo1": repo });
			mockGetGroupedLayout.mockReturnValue({
				groups: [
					{
						group: { id: "g1", name: "Work", color: "", collapsed: false, repoOrder: ["/repo1"] },
						repos: [repo],
					},
				],
				ungrouped: [],
			});

			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const groupHeader = container.querySelector(".groupHeader")!;

			fireEvent.pointerDown(groupHeader, { button: 0, pointerId: 1, clientX: 10, clientY: 10 });
			fireEvent.pointerMove(document, { pointerId: 1, clientX: 20, clientY: 10 });

			// Ghost element should exist in the document
			const ghosts = document.querySelectorAll("[style*='position: fixed']");
			expect(ghosts.length).toBeGreaterThan(0);

			// Cleanup
			fireEvent.pointerUp(document, { pointerId: 1, clientX: 20, clientY: 10 });
		});

		it("escape cancels drag without performing any action", () => {
			const repo = makeRepo();
			setRepos({ "/repo1": repo });
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const repoSection = container.querySelector(".repoSection")!;

			fireEvent.pointerDown(repoSection, { button: 0, pointerId: 1, clientX: 10, clientY: 10 });
			fireEvent.pointerMove(document, { pointerId: 1, clientX: 20, clientY: 10 });
			fireEvent.keyDown(document, { key: "Escape" });

			expect(mockReorderRepo).not.toHaveBeenCalled();
			expect(mockReorderRepoInGroup).not.toHaveBeenCalled();
			expect(mockMoveRepoBetweenGroups).not.toHaveBeenCalled();
			expect(mockAddRepoToGroup).not.toHaveBeenCalled();
			expect(mockRemoveRepoFromGroup).not.toHaveBeenCalled();
		});

		it("right-click does not initiate drag", () => {
			const repo = makeRepo();
			setRepos({ "/repo1": repo });
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const repoSection = container.querySelector(".repoSection")!;

			// button=2 is right-click
			fireEvent.pointerDown(repoSection, { button: 2, pointerId: 1, clientX: 10, clientY: 10 });
			fireEvent.pointerMove(document, { pointerId: 1, clientX: 20, clientY: 10 });

			expect(repoSection.classList.contains("dragging")).toBe(false);
			fireEvent.pointerUp(document, { pointerId: 1, clientX: 20, clientY: 10 });
		});

		it("pointerUp without crossing threshold is a click, not a drag", () => {
			const repo = makeRepo();
			setRepos({ "/repo1": repo });
			const { container } = render(() => <Sidebar {...defaultProps()} />);
			const repoSection = container.querySelector(".repoSection")!;

			fireEvent.pointerDown(repoSection, { button: 0, pointerId: 1, clientX: 10, clientY: 10 });
			// Move less than threshold (5px)
			fireEvent.pointerMove(document, { pointerId: 1, clientX: 12, clientY: 10 });
			fireEvent.pointerUp(document, { pointerId: 1, clientX: 12, clientY: 10 });

			// No drag actions
			expect(mockReorderRepo).not.toHaveBeenCalled();
			expect(repoSection.classList.contains("dragging")).toBe(false);
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

	describe("PrDetailPopover auto-show vs manual click", () => {
		it("PR badge click on non-active repo opens PrDetailPopover without auto-show closing it", async () => {
			// Two repos: /repo1 is active (no PR), /repo2 has a branch with a conflicting PR
			setRepos(
				{
					"/repo1": makeRepo({ path: "/repo1" }),
					"/repo2": makeRepo({
						path: "/repo2",
						displayName: "Repo Two",
						initials: "RT",
						workspaces: {
							main: { branchName: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
							feature: {
								workspaceId: "feature",
								branchName: "feature",
								isMain: false,
								worktreePath: "/repo2-wt",
								terminals: [],
								additions: 4,
								deletions: 0,
							},
						},
					}),
				},
				"/repo1",
			);

			mockGetActive.mockReturnValue({ path: "/repo1", activeWorkspaceId: "main" });

			// PR data only for /repo2/feature
			mockGetPrStatus.mockImplementation((...args: unknown[]) => {
				if (args[0] === "/repo2" && args[1] === "feature") {
					return {
						state: "OPEN",
						number: 99,
						title: "Feature PR",
						url: "https://example.com",
						mergeable: "CONFLICTING",
					};
				}
				return null;
			});

			const { container } = render(() => <Sidebar {...defaultProps()} />);

			// The Conflicts badge should render for /repo2/feature
			const badge = container.querySelector("[data-tooltip='PR #99']");
			expect(badge).not.toBeNull();

			// Click the badge's parent span (which has the onClick handler)
			await fireEvent.click(badge!.parentElement!);

			// PrDetailPopover should be rendered and not immediately closed by auto-show effect
			const popover = container.querySelector("[data-testid='pr-detail-popover']");
			expect(popover).not.toBeNull();
			expect(popover!.getAttribute("data-branch")).toBe("feature");
			expect(popover!.getAttribute("data-repo")).toBe("/repo2");
		});

		it("auto-shows PrDetailPopover when active branch has an open PR", async () => {
			mockGetActive.mockReturnValue({ path: "/repo1", activeWorkspaceId: "main" });
			mockGetPrStatus.mockImplementation((...args: unknown[]) => {
				if (args[0] === "/repo1" && args[1] === "main") {
					return { state: "OPEN", number: 7, title: "Auto PR", url: "https://example.com" };
				}
				return null;
			});
			setRepos({ "/repo1": makeRepo() });

			const { container } = render(() => <Sidebar {...defaultProps()} />);
			// setPrDetailTarget is deferred via queueMicrotask to avoid blocking branch-switch flush
			await new Promise<void>((r) => queueMicrotask(r));
			const popover = container.querySelector("[data-testid='pr-detail-popover']");
			expect(popover).not.toBeNull();
			expect(popover!.getAttribute("data-branch")).toBe("main");
			expect(popover!.getAttribute("data-repo")).toBe("/repo1");
		});

		it("manual popover stays open during branch switch (closed only via explicit close)", async () => {
			// Two repos: /repo1 active with a PR, /repo2 has PR on feature
			setRepos(
				{
					"/repo1": makeRepo({ path: "/repo1" }),
					"/repo2": makeRepo({
						path: "/repo2",
						displayName: "Repo Two",
						initials: "RT",
						workspaces: {
							main: { branchName: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
							feature: {
								workspaceId: "feature",
								branchName: "feature",
								isMain: false,
								worktreePath: "/repo2-wt",
								terminals: [],
								additions: 4,
								deletions: 0,
							},
						},
					}),
				},
				"/repo1",
			);

			mockGetActive.mockReturnValue({ path: "/repo1", activeWorkspaceId: "main" });
			mockGetPrStatus.mockImplementation((...args: unknown[]) => {
				if (args[0] === "/repo2" && args[1] === "feature") {
					return {
						state: "OPEN",
						number: 99,
						title: "Feature PR",
						url: "https://example.com",
						mergeable: "CONFLICTING",
					};
				}
				return null;
			});

			const { container } = render(() => <Sidebar {...defaultProps()} />);

			// Click badge on /repo2/feature (manual open)
			const badge = container.querySelector("[data-tooltip='PR #99']");
			expect(badge).not.toBeNull();
			await fireEvent.click(badge!.parentElement!);

			// Manual popover should be open
			let popover = container.querySelector("[data-testid='pr-detail-popover']");
			expect(popover).not.toBeNull();
			expect(popover!.getAttribute("data-repo")).toBe("/repo2");

			// Simulate branch switch on the active repo — manual popover must survive
			mockGetActive.mockReturnValue({ path: "/repo1", activeWorkspaceId: "develop" });
			setRepos(
				{
					"/repo1": makeRepo({ path: "/repo1", activeWorkspaceId: "develop" }),
					"/repo2": makeRepo({
						path: "/repo2",
						displayName: "Repo Two",
						initials: "RT",
						workspaces: {
							main: { branchName: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
							feature: {
								workspaceId: "feature",
								branchName: "feature",
								isMain: false,
								worktreePath: "/repo2-wt",
								terminals: [],
								additions: 4,
								deletions: 0,
							},
						},
					}),
				},
				"/repo1",
			);

			// Manual popover should STILL be open after branch switch
			popover = container.querySelector("[data-testid='pr-detail-popover']");
			expect(popover).not.toBeNull();
			expect(popover!.getAttribute("data-repo")).toBe("/repo2");
		});
	});
});
