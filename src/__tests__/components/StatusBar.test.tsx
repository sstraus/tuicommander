import { fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mockInvoke } from "../mocks/tauri";

// Use vi.hoisted so these are available to vi.mock factories (which are hoisted)
const {
	mockGitHubStatus,
	mockGitHubRefresh,
	mockGetActive,
	mockGetBranchPrData,
	mockIsGitRepo,
	mockTerminalGetActive,
	mockGetAllReposOrdered,
	mockGetConnections,
} = vi.hoisted(() => ({
	mockGitHubStatus: vi.fn<() => unknown>(() => null),
	mockGitHubRefresh: vi.fn(),
	mockGetActive: vi.fn<() => unknown>(() => null),
	mockGetBranchPrData: vi.fn<() => unknown>(() => null),
	mockIsGitRepo: vi.fn<() => boolean>(() => true),
	mockTerminalGetActive: vi.fn<() => unknown>(() => null),
	mockGetAllReposOrdered: vi.fn<() => unknown[]>(() => []),
	mockGetConnections: vi.fn<() => Record<string, unknown>>(() => ({})),
}));

vi.mock("../../hooks/useGitHub", () => ({
	useGitHub: () => ({
		status: mockGitHubStatus,
		loading: () => false,
		error: () => null,
		refresh: mockGitHubRefresh,
		startPolling: vi.fn(),
		stopPolling: vi.fn(),
	}),
}));

vi.mock("../../stores/repositories", () => ({
	resolveRepoPathFor: (path: string) => path,
	repositoriesStore: {
		getActive: mockGetActive,
		get: vi.fn(() => undefined),
		getGroupForRepo: vi.fn(() => undefined),
		getRevision: vi.fn(() => 0),
		getAllReposOrdered: mockGetAllReposOrdered,
		isGitRepo: mockIsGitRepo,
	},
}));

vi.mock("../../stores/terminals", async (importOriginal) => {
	const actual = await importOriginal<typeof import("../../stores/terminals")>();
	return {
		...actual,
		terminalsStore: {
			...actual.terminalsStore,
			getActive: mockTerminalGetActive,
		},
	};
});

vi.mock("../../stores/remoteConnections", () => ({
	remoteConnectionsStore: {
		getConnections: mockGetConnections,
	},
}));

vi.mock("../../stores/repoSettings", () => ({
	repoSettingsStore: {
		get: vi.fn(() => undefined),
	},
}));

vi.mock("../../hooks/useRepository", () => ({
	useRepository: () => ({
		renameBranch: vi.fn().mockResolvedValue(undefined),
	}),
}));

vi.mock("../../stores/github", () => ({
	githubStore: {
		getBranchPrData: mockGetBranchPrData,
		// getPrStatus is used by mergedPrGrace utility — share the same mock so tests
		// that set mockGetBranchPrData automatically drive both the StatusBar PR badge
		// (via activePrStatus → getPrStatus) and PrDetailPopover (via getBranchPrData)
		getPrStatus: mockGetBranchPrData,
		getCheckSummary: vi.fn(() => null),
		getCheckDetails: vi.fn(() => []),
		loadCheckDetails: vi.fn().mockResolvedValue(undefined),
	},
}));

const { mockDictationState } = vi.hoisted(() => ({
	mockDictationState: {
		enabled: false,
		recording: false,
		processing: false,
		loading: false,
		hotkey: "F8",
	},
}));

vi.mock("../../stores/dictation", () => ({
	dictationStore: {
		state: mockDictationState,
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

import { StatusBar } from "../../components/StatusBar/StatusBar";
import { getTickerPriority } from "../../features/claudeUsage";
import { getCodexTickerPriority } from "../../features/codexUsage";
import { acpStore } from "../../stores/acp";
import { settingsStore } from "../../stores/settings";
import { statusBarTicker } from "../../stores/statusBarTicker";
import { uiStore } from "../../stores/ui";

/** Build a mock BranchPrStatus for testing */
function makePrData(overrides: Record<string, unknown> = {}) {
	return {
		branch: "main",
		number: 42,
		title: "Fix bug",
		state: "OPEN",
		url: "https://github.com/test/pr/42",
		additions: 10,
		deletions: 5,
		checks: { passed: 3, failed: 0, pending: 0, total: 3 },
		check_details: [],
		author: "alice",
		commits: 1,
		mergeable: "MERGEABLE",
		merge_state_status: "CLEAN",
		review_decision: "APPROVED",
		labels: [],
		is_draft: false,
		base_ref_name: "main",
		head_ref_oid: "abc1234",
		created_at: "2025-01-01T00:00:00Z",
		updated_at: "2025-01-02T00:00:00Z",
		merge_state_label: null,
		review_state_label: null,
		...overrides,
	};
}

/** Find the CI badge wrapper span (the one with the onClick handler) */
function findCiBadgeWrapper(container: HTMLElement): HTMLElement {
	const badges = container.querySelectorAll(".githubStatus [data-testid='status-badge']");
	const ciBadgeEl = Array.from(badges).find((b) => b.textContent?.includes("CI"));
	if (!ciBadgeEl?.parentElement) throw new Error("CI badge wrapper not found");
	return ciBadgeEl.parentElement;
}

/** Find a toggle button by its title text */
function findToggleByTitle(container: HTMLElement, titlePart: string): HTMLElement | null {
	const buttons = container.querySelectorAll("button[title]");
	return (Array.from(buttons).find((b) => b.getAttribute("title")?.includes(titlePart)) as HTMLElement) ?? null;
}

describe("StatusBar", () => {
	const defaultProps = {
		fontSize: 14,
		defaultFontSize: 14,
		statusInfo: "Ready",
		onToggleDiff: vi.fn(),
		onToggleMarkdown: vi.fn(),
		onDictationStart: vi.fn(),
		onDictationStop: vi.fn(),
	};

	beforeEach(async () => {
		vi.useFakeTimers();
		vi.clearAllMocks();
		mockInvoke.mockImplementation(async (command: string) =>
			command === "get_working_tree_status" ? { staged: [], unstaged: [], untracked: [] } : undefined,
		);
		await uiStore.hydrate();
		mockGitHubStatus.mockReturnValue(null);
		mockGetActive.mockReturnValue(null);
		mockTerminalGetActive.mockReturnValue(null);
		mockGetBranchPrData.mockReturnValue(null);
		statusBarTicker.clear();
		mockDictationState.enabled = false;
		mockDictationState.recording = false;
		mockDictationState.processing = false;
		mockDictationState.loading = false;
		mockLastActivityAt.mockReturnValue(0);
		mockIsGitRepo.mockReturnValue(true);
		mockGetAllReposOrdered.mockReturnValue([]);
		mockGetConnections.mockReturnValue({});
		acpStore.reset();
		uiStore.setAiChatPanelVisible(false);
	});

	afterEach(() => {
		statusBarTicker.clear();
		uiStore._testCancelPendingSave();
		vi.useRealTimers();
	});

	it("shows pending ACP permission and elicitation counts on the hidden AI Chat toggle, then clears on settlement", () => {
		vi.spyOn(settingsStore, "isAiChatEnabled").mockReturnValue(true);
		acpStore.applySnapshot({
			connectionId: "connection-1",
			generation: 1,
			state: "ready",
			agentInfo: null,
			capabilities: null,
			attachments: [],
			earliestSequence: 1,
			latestSequence: 1,
			settlement: null,
		});
		const { container } = render(() => <StatusBar {...defaultProps} />);
		const toggle = findToggleByTitle(container, "AI Chat");
		expect(toggle?.textContent).not.toContain("1");

		acpStore.applyInteractions("connection-1", [
			{
				kind: "permission",
				requestId: "permission-1",
				sessionId: "session-1",
				request: { sessionId: "session-1", toolCall: {}, options: [] },
			},
			{
				kind: "elicitation",
				requestId: "elicitation-1",
				sessionId: "session-1",
				request: { sessionId: "session-1", message: "Need a value", mode: "form", requestedSchema: {} },
			},
		]);
		expect(toggle?.textContent).toContain("2");
		uiStore.setAiChatPanelVisible(true);
		expect(toggle?.textContent).not.toContain("2");
		uiStore.setAiChatPanelVisible(false);
		expect(toggle?.textContent).toContain("2");

		acpStore.applyInteractions("connection-1", []);
		expect(toggle?.textContent).not.toContain("2");
	});

	it("renders status info text", () => {
		const { container } = render(() => <StatusBar {...defaultProps} />);
		const statusInfo = container.querySelector(".info");
		expect(statusInfo).not.toBeNull();
		expect(statusInfo!.textContent).toBe("Ready");
	});

	describe("non-modal popups dismiss on an outside press without a backdrop", () => {
		const outsidePress = (onSidebarPress: () => void) => {
			const sidebar = document.createElement("aside");
			sidebar.addEventListener("pointerdown", onSidebarPress);
			document.body.appendChild(sidebar);
			const event = new PointerEvent("pointerdown", { bubbles: true, cancelable: true });
			sidebar.dispatchEvent(event);
			sidebar.remove();
			return event;
		};

		// Catches: an invisible fixed inset:0 backdrop swallows the press (and wheel) meant for the sidebar.
		it("ticker popover closes on an outside pointer press and leaves the event alone", () => {
			statusBarTicker.addMessage({ id: "t:1", pluginId: "t", label: "T", text: "hello", priority: 10, ttlMs: 0 });
			const { container } = render(() => <StatusBar {...defaultProps} />);
			fireEvent.contextMenu(container.querySelector(".tickerMessage")!);
			expect(container.querySelector(".tickerPopover")).not.toBeNull();
			expect(container.querySelector(".tickerOverlay")).toBeNull();

			const onSidebarPress = vi.fn();
			const event = outsidePress(onSidebarPress);

			expect(container.querySelector(".tickerPopover")).toBeNull();
			expect(onSidebarPress).toHaveBeenCalledOnce();
			expect(event.defaultPrevented).toBe(false);
		});

		// Catches: pressing inside the open ticker popover dismisses it before its row handler runs.
		it("ticker popover stays open for a press inside it", () => {
			statusBarTicker.addMessage({ id: "t:1", pluginId: "t", label: "T", text: "hello", priority: 10, ttlMs: 0 });
			const { container } = render(() => <StatusBar {...defaultProps} />);
			fireEvent.contextMenu(container.querySelector(".tickerMessage")!);
			fireEvent.pointerDown(container.querySelector(".tickerPopover")!);
			expect(container.querySelector(".tickerPopover")).not.toBeNull();
		});

		// Catches: an invisible fixed inset:0 backdrop swallows the press (and wheel) meant for the sidebar.
		it("info balloon closes on an outside pointer press and leaves the event alone", () => {
			vi.spyOn(HTMLElement.prototype, "scrollWidth", "get").mockReturnValue(500);
			vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(100);
			const { container } = render(() => <StatusBar {...defaultProps} statusInfo="A very long status message" />);
			vi.advanceTimersByTime(50);
			fireEvent.click(container.querySelector(".info")!);
			expect(container.querySelector(".infoBalloon")).not.toBeNull();
			expect(container.querySelector(".infoBalloonOverlay")).toBeNull();

			const onSidebarPress = vi.fn();
			const event = outsidePress(onSidebarPress);

			expect(container.querySelector(".infoBalloon")).toBeNull();
			expect(onSidebarPress).toHaveBeenCalledOnce();
			expect(event.defaultPrevented).toBe(false);
			vi.restoreAllMocks();
		});

		// Catches: the toggle trigger counts as an outside press, so closing it by click re-opens it.
		it("info balloon still toggles closed from its own trigger", () => {
			vi.spyOn(HTMLElement.prototype, "scrollWidth", "get").mockReturnValue(500);
			vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(100);
			const { container } = render(() => <StatusBar {...defaultProps} statusInfo="A very long status message" />);
			vi.advanceTimersByTime(50);
			const trigger = container.querySelector(".info")!;
			fireEvent.click(trigger);
			expect(container.querySelector(".infoBalloon")).not.toBeNull();
			fireEvent.pointerDown(trigger);
			fireEvent.click(trigger);
			expect(container.querySelector(".infoBalloon")).toBeNull();
			vi.restoreAllMocks();
		});
	});

	describe("usage blink boundary", () => {
		// Catches: the badge blinks early, or a priority producer never reaches its critical class.
		it.each(["claude", "codex", "pty"] as const)("%s blinks only from 96 percent", (source) => {
			for (const percentage of [69, 70, 90, 95, 95.9, 96, 100]) {
				mockTerminalGetActive.mockReturnValue({
					agentType: source === "pty" ? "grok" : source,
					usageLimit: source === "pty" ? { percentage, limitType: "weekly" } : null,
				});
				if (source !== "pty") {
					const priority =
						source === "claude"
							? getTickerPriority({
									five_hour: { utilization: percentage, resets_at: null },
									seven_day: null,
									seven_day_opus: null,
									seven_day_sonnet: null,
									seven_day_oauth_apps: null,
									seven_day_cowork: null,
									extra_usage: null,
									plan: null,
									meta: null,
								})
							: getCodexTickerPriority({
									plan_type: null,
									rate_limit: {
										allowed: true,
										limit_reached: false,
										primary_window: {
											used_percent: percentage,
											limit_window_seconds: 18000,
											reset_after_seconds: null,
											reset_at: null,
										},
										secondary_window: null,
									},
									additional_rate_limits: [],
									credits: null,
									model_usage: {},
								});
					statusBarTicker.addMessage({
						id: "claude-usage:rate",
						pluginId: "claude-usage",
						label: source === "claude" ? "Claude" : "Codex",
						text: `5h: ${percentage}%`,
						priority,
						ttlMs: 0,
					});
				}
				const { container, unmount } = render(() => <StatusBar {...defaultProps} />);
				const usage = container.querySelector(".agentUsage");
				expect(usage).not.toBeNull();
				expect(usage!.classList.contains("agentUsageCritical"), `${source} at ${percentage}%`).toBe(percentage >= 96);
				expect(usage!.classList.contains("agentUsageWarning"), `${source} at ${percentage}%`).toBe(
					percentage >= 70 && percentage < 96,
				);
				unmount();
				statusBarTicker.clear();
			}
		});
	});

	it("absorbs matching Codex usage into the active agent badge", () => {
		const openDashboard = vi.fn();
		mockTerminalGetActive.mockReturnValue({ agentType: "codex", usageLimit: null });
		statusBarTicker.addMessage({
			id: "claude-usage:rate",
			pluginId: "claude-usage",
			label: "Codex",
			text: "5h: 42% · week: 18%",
			priority: 42,
			ttlMs: 0,
			onClick: openDashboard,
		});

		const { container } = render(() => <StatusBar {...defaultProps} />);
		const badge = container.querySelector(".agentBadge");

		expect(badge?.textContent).toContain("5h: 42% · week: 18%");
		expect(container.querySelector(".tickerMessage")).toBeNull();
		fireEvent.click(badge!);
		expect(openDashboard).toHaveBeenCalledOnce();
	});

	it("does not show stale Claude usage in an active Codex badge", () => {
		mockTerminalGetActive.mockReturnValue({ agentType: "codex", usageLimit: null });
		statusBarTicker.addMessage({
			id: "claude-usage:rate",
			pluginId: "claude-usage",
			label: "Claude",
			text: "5h: 99% · 7d: 88%",
			priority: 99,
			ttlMs: 0,
		});

		const { container } = render(() => <StatusBar {...defaultProps} />);
		const badge = container.querySelector(".agentBadge");

		expect(badge?.textContent).toContain("codex");
		expect(badge?.textContent).not.toContain("5h: 99%");
	});

	it("calls onToggleMarkdown when MD button clicked", () => {
		const onToggleMarkdown = vi.fn();
		const { container } = render(() => <StatusBar {...defaultProps} onToggleMarkdown={onToggleMarkdown} />);
		const mdBtn = findToggleByTitle(container, "Markdown")!;
		fireEvent.click(mdBtn);
		expect(onToggleMarkdown).toHaveBeenCalledOnce();
	});

	it("calls onToggleDiff when Git button clicked", () => {
		const onToggleDiff = vi.fn();
		const { container } = render(() => <StatusBar {...defaultProps} onToggleDiff={onToggleDiff} />);
		const gitBtn = findToggleByTitle(container, "Git")!;
		fireEvent.click(gitBtn);
		expect(onToggleDiff).toHaveBeenCalledOnce();
	});

	// A plain directory registered as a repo has no git index. Querying it made
	// `get_working_tree_status` fail on every repo-revision bump, flooding the log.
	it("does not query the working tree for a plain (non-git) directory", () => {
		mockIsGitRepo.mockReturnValue(false);
		render(() => <StatusBar {...defaultProps} currentRepoPath="/plain-dir" />);
		const calls = mockInvoke.mock.calls.filter((c) => c[0] === "get_working_tree_status");
		expect(calls).toHaveLength(0);
	});

	it("queries the working tree for a git repo", () => {
		render(() => <StatusBar {...defaultProps} currentRepoPath="/git-repo" />);
		const calls = mockInvoke.mock.calls.filter((c) => c[0] === "get_working_tree_status");
		expect(calls).toHaveLength(1);
	});

	it("does not render githubStatus when github status is null", () => {
		const { container } = render(() => <StatusBar {...defaultProps} />);
		const githubStatus = container.querySelector(".githubStatus");
		expect(githubStatus).toBeNull();
	});

	it("shows PrBadge when githubStore has PR data for active branch", () => {
		mockGitHubStatus.mockReturnValue({
			current_branch: "main",
			ahead: 0,
			behind: 0,
		});
		mockGetBranchPrData.mockReturnValue(makePrData());
		const { container } = render(() => <StatusBar {...defaultProps} currentRepoPath="/repo" />);
		const githubStatus = container.querySelector(".githubStatus");
		expect(githubStatus).not.toBeNull();
		const badges = githubStatus!.querySelectorAll("[data-testid='status-badge']");
		const prBadge = Array.from(badges).find((b) => b.textContent?.includes("PR #42"));
		expect(prBadge).toBeDefined();
	});

	it("shows CiBadge when githubStore has checks with total > 0", () => {
		mockGitHubStatus.mockReturnValue({
			current_branch: "main",
			ahead: 0,
			behind: 0,
		});
		mockGetBranchPrData.mockReturnValue(
			makePrData({
				checks: { passed: 3, failed: 0, pending: 0, total: 3 },
			}),
		);
		const { container } = render(() => <StatusBar {...defaultProps} currentRepoPath="/repo" />);
		const githubStatus = container.querySelector(".githubStatus");
		expect(githubStatus).not.toBeNull();
		const badges = githubStatus!.querySelectorAll("[data-testid='status-badge']");
		const ciBadge = Array.from(badges).find((b) => b.textContent?.includes("CI passed"));
		expect(ciBadge).toBeDefined();
	});

	it("CI badge click opens PrDetailPopover", async () => {
		mockGitHubStatus.mockReturnValue({
			current_branch: "main",
			ahead: 0,
			behind: 0,
		});
		mockGetBranchPrData.mockReturnValue(
			makePrData({
				checks: { passed: 3, failed: 0, pending: 0, total: 3 },
			}),
		);
		const { container } = render(() => <StatusBar {...defaultProps} currentRepoPath="/repo" />);

		const ciBadge = findCiBadgeWrapper(container);
		fireEvent.click(ciBadge);

		await waitFor(() => {
			const popover = container.querySelector(".popover");
			expect(popover).not.toBeNull();
		});
	});

	it("does not render CI badge when no currentRepoPath", () => {
		mockGitHubStatus.mockReturnValue({
			current_branch: "main",
			ahead: 0,
			behind: 0,
		});
		// Even if githubStore has PR data, without currentRepoPath the activePrData memo returns null
		mockGetBranchPrData.mockReturnValue(
			makePrData({
				checks: { passed: 1, failed: 0, pending: 0, total: 1 },
			}),
		);
		const { container } = render(() => <StatusBar {...defaultProps} />);

		const badges = container.querySelectorAll(".githubStatus [data-testid='status-badge']");
		const ciBadge = Array.from(badges).find((b) => b.textContent?.includes("CI"));
		expect(ciBadge).toBeUndefined();
	});

	it("shows CiBadge with failure state", () => {
		mockGitHubStatus.mockReturnValue({
			current_branch: "main",
			ahead: 0,
			behind: 0,
		});
		mockGetBranchPrData.mockReturnValue(
			makePrData({
				checks: { passed: 2, failed: 1, pending: 0, total: 3 },
			}),
		);
		const { container } = render(() => <StatusBar {...defaultProps} currentRepoPath="/repo" />);
		const badges = container.querySelectorAll("[data-testid='status-badge']");
		const ciBadge = Array.from(badges).find((b) => b.textContent?.includes("CI failed"));
		expect(ciBadge).toBeDefined();
	});

	it("opens PR detail popover when PrBadge is clicked", () => {
		mockGitHubStatus.mockReturnValue({
			current_branch: "main",
			ahead: 0,
			behind: 0,
		});
		mockGetBranchPrData.mockReturnValue(makePrData());
		const { container } = render(() => <StatusBar {...defaultProps} currentRepoPath="/repo" />);
		const badges = container.querySelectorAll("[data-testid='status-badge']");
		const prBadge = Array.from(badges).find((b) => b.textContent?.includes("PR #42"))!;
		fireEvent.click(prBadge);
		const popover = container.querySelector(".popover");
		expect(popover).not.toBeNull();
	});

	it("shows mic button when dictation is enabled", () => {
		mockDictationState.enabled = true;
		const { container } = render(() => <StatusBar {...defaultProps} />);
		const micBtn = findToggleByTitle(container, "Voice Dictation");
		expect(micBtn).not.toBeNull();
	});

	it("does not show mic button when dictation is disabled", () => {
		mockDictationState.enabled = false;
		const { container } = render(() => <StatusBar {...defaultProps} />);
		const micBtn = findToggleByTitle(container, "Voice Dictation");
		expect(micBtn).toBeNull();
	});

	it("calls onDictationStart on mouseDown", () => {
		mockDictationState.enabled = true;
		const onDictationStart = vi.fn();
		const { container } = render(() => <StatusBar {...defaultProps} onDictationStart={onDictationStart} />);
		const micBtn = findToggleByTitle(container, "Voice Dictation")!;
		fireEvent.mouseDown(micBtn, { button: 0 });
		expect(onDictationStart).toHaveBeenCalled();
	});

	it("calls onDictationStop on mouseUp when recording", () => {
		mockDictationState.enabled = true;
		mockDictationState.recording = true;
		const onDictationStop = vi.fn();
		const { container } = render(() => <StatusBar {...defaultProps} onDictationStop={onDictationStop} />);
		const micBtn = findToggleByTitle(container, "Voice Dictation")!;
		fireEvent.mouseUp(micBtn, { button: 0 });
		expect(onDictationStop).toHaveBeenCalled();
	});

	it("calls onDictationStop on mouseUp even when recording is false (start in-flight)", () => {
		mockDictationState.enabled = true;
		mockDictationState.recording = false; // start hasn't resolved yet
		const onDictationStop = vi.fn();
		const { container } = render(() => <StatusBar {...defaultProps} onDictationStop={onDictationStop} />);
		const micBtn = findToggleByTitle(container, "Voice Dictation")!;
		fireEvent.mouseUp(micBtn, { button: 0 });
		expect(onDictationStop).toHaveBeenCalled();
	});

	it("calls onDictationStop on mouseLeave when recording", () => {
		mockDictationState.enabled = true;
		mockDictationState.recording = true;
		const onDictationStop = vi.fn();
		const { container } = render(() => <StatusBar {...defaultProps} onDictationStop={onDictationStop} />);
		const micBtn = findToggleByTitle(container, "Voice Dictation")!;
		fireEvent.mouseLeave(micBtn);
		expect(onDictationStop).toHaveBeenCalled();
	});

	it("calls onDictationStop on mouseLeave when loading (start in-flight)", () => {
		mockDictationState.enabled = true;
		mockDictationState.recording = false;
		mockDictationState.loading = true;
		const onDictationStop = vi.fn();
		const { container } = render(() => <StatusBar {...defaultProps} onDictationStop={onDictationStop} />);
		const micBtn = findToggleByTitle(container, "Voice Dictation")!;
		fireEvent.mouseLeave(micBtn);
		expect(onDictationStop).toHaveBeenCalled();
	});

	it("offers no hands-free control: it starts from the Command Palette", () => {
		mockDictationState.enabled = true;
		const { container } = render(() => <StatusBar {...defaultProps} />);
		expect(findToggleByTitle(container, "hands-free")).toBeNull();
	});

	it("does not show PrBadge for CLOSED PR", () => {
		mockGitHubStatus.mockReturnValue({
			current_branch: "main",
			ahead: 0,
			behind: 0,
		});
		mockGetBranchPrData.mockReturnValue(makePrData({ state: "CLOSED" }));
		const { container } = render(() => <StatusBar {...defaultProps} currentRepoPath="/repo" />);
		// CLOSED PR hides the entire github section (no activePrData)
		const githubStatus = container.querySelector(".githubStatus");
		expect(githubStatus).toBeNull();
	});

	it("shows PrBadge for MERGED PR with recent user activity", () => {
		vi.setSystemTime(10000);
		mockLastActivityAt.mockReturnValue(9000); // active 1s ago
		mockGitHubStatus.mockReturnValue({
			current_branch: "main",
			ahead: 0,
			behind: 0,
		});
		mockGetBranchPrData.mockReturnValue(makePrData({ state: "MERGED" }));
		const { container } = render(() => <StatusBar {...defaultProps} currentRepoPath="/repo" />);
		const githubStatus = container.querySelector(".githubStatus");
		const badges = githubStatus!.querySelectorAll("[data-testid='status-badge']");
		const prBadge = Array.from(badges).find((b) => b.textContent?.includes("PR #42"));
		expect(prBadge).toBeDefined();
	});

	it("hides PrBadge for MERGED PR after 5 min of accumulated activity", () => {
		vi.setSystemTime(400_000); // 400s in
		// Simulate: last activity was long ago, and accumulated activity exceeds 5 min
		// This tests the mergedActivityMs accumulation logic
		mockLastActivityAt.mockReturnValue(0); // no recent activity
		mockGitHubStatus.mockReturnValue({
			current_branch: "main",
			ahead: 0,
			behind: 0,
		});
		mockGetBranchPrData.mockReturnValue(makePrData({ state: "MERGED" }));
		const { container } = render(() => <StatusBar {...defaultProps} currentRepoPath="/repo" />);

		// Simulate 301 seconds of activity ticking (each tick adds 1s when user is active)
		for (let i = 0; i < 301; i++) {
			vi.setSystemTime(400_000 + (i + 1) * 1000);
			mockLastActivityAt.mockReturnValue(400_000 + (i + 1) * 1000 - 500); // active within 2s
			vi.advanceTimersByTime(1000);
		}

		// After 5+ min of accumulated activity, the entire github section is hidden
		const githubStatus = container.querySelector(".githubStatus");
		expect(githubStatus).toBeNull();
	});

	it("OPEN PR still renders normally", () => {
		mockGitHubStatus.mockReturnValue({
			current_branch: "main",
			ahead: 0,
			behind: 0,
		});
		mockGetBranchPrData.mockReturnValue(makePrData({ state: "OPEN" }));
		const { container } = render(() => <StatusBar {...defaultProps} currentRepoPath="/repo" />);
		const githubStatus = container.querySelector(".githubStatus");
		const badges = githubStatus!.querySelectorAll("[data-testid='status-badge']");
		const prBadge = Array.from(badges).find((b) => b.textContent?.includes("PR #42"));
		expect(prBadge).toBeDefined();
	});

	// The offline notice exists so a disconnected machine is named once, instead
	// of surfacing as every operation on its repos failing separately. A machine
	// holding no registered repo is silent: not being connected to it is the
	// normal resting state.
	it("names a disconnected machine that holds a registered repo", () => {
		mockGetAllReposOrdered.mockReturnValue([{ path: "/r", connectionId: "c1" }]);
		mockGetConnections.mockReturnValue({
			c1: { connection: { id: "c1", name: "lab-linux" }, status: "disconnected" },
		});
		const { container } = render(() => <StatusBar {...defaultProps} />);
		expect(container.textContent).toContain("lab-linux");
	});

	it("stays silent for a disconnected machine with no registered repo", () => {
		mockGetAllReposOrdered.mockReturnValue([]);
		mockGetConnections.mockReturnValue({
			c1: { connection: { id: "c1", name: "lab-linux" }, status: "disconnected" },
		});
		const { container } = render(() => <StatusBar {...defaultProps} />);
		expect(container.textContent).not.toContain("lab-linux");
	});

	it("stays silent for a connected machine that holds a registered repo", () => {
		mockGetAllReposOrdered.mockReturnValue([{ path: "/r", connectionId: "c1" }]);
		mockGetConnections.mockReturnValue({
			c1: { connection: { id: "c1", name: "lab-linux" }, status: "connected" },
		});
		const { container } = render(() => <StatusBar {...defaultProps} />);
		expect(container.textContent).not.toContain("lab-linux");
	});
});
