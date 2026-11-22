import { render } from "@solidjs/testing-library";
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
import { terminalsStore } from "../../stores/terminals";

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

describe("rich agent rows: cycles and flow polling (critic r6)", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mockGetActive.mockReturnValue(null);
		mockGetSubAgentTag.mockReturnValue(null);
		mockGetCheckSummary.mockReturnValue(null);
		mockGetPrStatus.mockReturnValue(null);
		mockLastActivityAt.mockReturnValue(0);
		settingsStore.setTabTreeEnabled(true);
		_resetMergedActivityAccum();
		progressStore.resetForTests();
		mockInvoke.mockReset();
		mockInvoke.mockResolvedValue({ project: "/r", participants: [], events: [], truncated: false });
		(terminalsStore.isBusy as unknown as ReturnType<typeof vi.fn>).mockImplementation(() => false);
	});
	afterEach(() => {
		settingsStore.setTabTreeEnabled(false);
		vi.useRealTimers();
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
	const flowCalls = () => mockInvoke.mock.calls.filter((c) => c[0] === "progress_flow");

	// Catches: the cycle check only looking at direct 2-cycles, so a child hanging off a cycle member
	// (or a tail leading into the loop) disappears with it.
	it("lists a child whose parent sits inside a two-terminal cycle, and both cycle members", () => {
		withTerms([term("Alpha", "sA", "sB"), term("Beta", "sB", "sA"), term("Kid", "sK", "sA")]);
		const { container } = render(() => <Sidebar {...defaultProps()} />);
		for (const n of ["Alpha", "Beta", "Kid"]) expect(tabs(container, n)).toHaveLength(1);
	});

	// Catches: a tail two hops away from a three-member loop vanishing (walk stops at the first revisit
	// but the tail's own parent chain is mistaken for membership).
	it("lists a chain that ends in a three-terminal loop", () => {
		withTerms([
			term("Alpha", "sA", "sC"),
			term("Beta", "sB", "sA"),
			term("Gamma", "sC", "sB"),
			term("Mid", "sM", "sA"),
			term("Leaf", "sL", "sM"),
		]);
		const { container } = render(() => <Sidebar {...defaultProps()} />);
		for (const n of ["Alpha", "Beta", "Gamma", "Mid", "Leaf"]) expect(tabs(container, n)).toHaveLength(1);
	});

	// Catches: two independent loops on one branch interfering (shared visited set across members).
	it("lists two separate parent loops", () => {
		withTerms([term("A1", "a1", "a2"), term("A2", "a2", "a1"), term("B1", "b1", "b2"), term("B2", "b2", "b1")]);
		const { container } = render(() => <Sidebar {...defaultProps()} />);
		for (const n of ["A1", "A2", "B1", "B2"]) expect(tabs(container, n)).toHaveLength(1);
	});

	// Catches: the polling effect tracking only the first agent's busy flag, so a busy flip on a
	// later agent never refreshes the subagent lines.
	it("asks for the flow again when the busy flag of a second agent flips", async () => {
		vi.useFakeTimers();
		const [busyB, setBusyB] = createSignal(false);
		(terminalsStore.isBusy as unknown as ReturnType<typeof vi.fn>).mockImplementation((id: string) =>
			id === "Beta" ? busyB() : false,
		);
		withTerms([term("Alpha", "sA"), term("Beta", "sB")]);
		render(() => <Sidebar {...defaultProps()} />);
		await vi.advanceTimersByTimeAsync(0);
		const before = flowCalls().length;
		expect(before).toBeGreaterThanOrEqual(1);
		await vi.advanceTimersByTimeAsync(6000);
		setBusyB(true);
		await vi.advanceTimersByTimeAsync(0);
		expect(flowCalls().length).toBe(before + 1);
	});

	// Catches: polling the backend for a branch that shows no agent at all.
	it("does not ask for the flow when no terminal is an agent", async () => {
		vi.useFakeTimers();
		withTerms([{ ...term("Shell", "sS"), agentType: null } as unknown as ReturnType<typeof term>]);
		render(() => <Sidebar {...defaultProps()} />);
		await vi.advanceTimersByTimeAsync(70_000);
		expect(flowCalls()).toHaveLength(0);
	});
});
