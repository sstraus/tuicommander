import { fireEvent, render, screen } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";

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
	resolveRepoPathFor: (path: string) => path,
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
	registerBlockFoldingGate: vi.fn(),
	terminalsStore: {
		getIds: vi.fn(() => []),
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
import { progressStore } from "../../stores/progress";
import { repositoriesStore } from "../../stores/repositories";
import { settingsStore } from "../../stores/settings";

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

const term = (name: string, sessionId: string, parentSession?: string) => ({
	id: name,
	name,
	sessionId,
	tuicSession: `tuic-${sessionId}`,
	parentSession,
	agentType: "claude",
	awaitingInput: null,
	shellState: "idle",
	unseen: false,
	// Cycle visibility is independent of the two-hour idle fold.
	lastActivityAt: Date.now(),
	currentTask: null,
	agentIntent: null,
	lastPrompt: null,
});

describe("rich agent rows: nesting and clicks (critic r5)", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mockGetActive.mockReturnValue(null);
		mockGetSubAgentTag.mockReturnValue(null);
		mockGetCheckSummary.mockReturnValue(null);
		mockGetPrStatus.mockReturnValue(null);
		mockLastActivityAt.mockReturnValue(0);
		settingsStore.setTabTreeEnabled(true);
		_resetMergedActivityAccum();
	});
	afterEach(() => {
		settingsStore.setTabTreeEnabled(false);
		progressStore.resetForTests();
	});

	const withTerms = (terms: ReturnType<typeof term>[]) => {
		mockTerminalsGet.mockImplementation((id: string) => terms.find((x) => x.id === id) ?? null);
		setRepos({
			"/r": makeRepo({
				path: "/r",
				workspaces: {
					main: {
						workspaceId: "main",
						branchName: "main",
						isMain: true,
						worktreePath: null,
						terminals: terms.map((x) => x.id),
						tabsCollapsed: false,
						additions: 0,
						deletions: 0,
					},
				},
			}),
		});
	};
	const tabs = (container: HTMLElement, name: string) => container.querySelectorAll(`button[aria-label="${name}"]`);

	// Catches: two sessions naming each other as parent both filtered out of the top level and
	// never rendered as children of a root, so both agents vanish from the sidebar.
	it("still lists two terminals whose parentSession point at each other", () => {
		withTerms([term("Alpha", "sA", "tuic-sB"), term("Beta", "sB", "tuic-sA")]);
		const { container } = render(() => <Sidebar {...defaultProps()} />);
		expect(tabs(container, "Alpha")).toHaveLength(1);
		expect(tabs(container, "Beta")).toHaveLength(1);
	});

	// Catches: a three-session parent loop (A->B->C->A) leaving every member without a top-level row.
	it("still lists a three-terminal parent loop", () => {
		withTerms([term("Alpha", "sA", "sC"), term("Beta", "sB", "sA"), term("Gamma", "sC", "sB")]);
		const { container } = render(() => <Sidebar {...defaultProps()} />);
		for (const n of ["Alpha", "Beta", "Gamma"]) expect(tabs(container, n)).toHaveLength(1);
	});

	// Catches: a session that names itself as parent being treated as its own child (rendered zero times).
	it("lists a terminal that is its own parent once", () => {
		withTerms([term("Alpha", "sA", "sA")]);
		const { container } = render(() => <Sidebar {...defaultProps()} />);
		expect(tabs(container, "Alpha")).toHaveLength(1);
	});

	// Catches: a child matched by both the sessionId and the tuicSession of different parents rendered under each.
	it("renders each child exactly once under one parent", () => {
		withTerms([term("Parent", "sP"), term("Kid", "sK", "sP"), term("Kid2", "sK2", "tuic-sP")]);
		const { container } = render(() => <Sidebar {...defaultProps()} />);
		expect(tabs(container, "Kid")).toHaveLength(1);
		expect(tabs(container, "Kid2")).toHaveLength(1);
		expect(tabs(container, "Parent")).toHaveLength(1);
	});

	// Catches: the touch "more" button bubbling to the row, so opening the menu also opens the branch.
	it("opening the branch options button does not open the branch", () => {
		withTerms([term("Alpha", "sA")]);
		const onBranchSelect = vi.fn();
		const { container } = render(() => <Sidebar {...defaultProps({ onBranchSelect })} />);
		const more = container.querySelector('button[aria-label="Branch options"]') as HTMLElement;
		fireEvent.click(more);
		expect(onBranchSelect).not.toHaveBeenCalled();
	});

	// Catches: a click on the branch row also toggling the agents list (AGENTS.md "Sidebar clicks").
	it("a click on the branch name opens the branch and never toggles the agents", () => {
		withTerms([term("Alpha", "sA")]);
		const onBranchSelect = vi.fn();
		const { container } = render(() => <Sidebar {...defaultProps({ onBranchSelect })} />);
		fireEvent.click(screen.getByText("main"));
		expect(onBranchSelect).toHaveBeenCalledTimes(1);
		expect(mockToggleBranchTabsCollapsed).not.toHaveBeenCalled();
		expect(tabs(container, "Alpha")).toHaveLength(1);
	});
});
