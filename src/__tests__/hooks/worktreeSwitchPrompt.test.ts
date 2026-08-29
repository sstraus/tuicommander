import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { makeTerminal, testInScopeAsync } from "../helpers/store";

const mockInvoke = vi.fn().mockResolvedValue(undefined);
const mockListen = vi.fn().mockResolvedValue(vi.fn());

vi.mock("../../invoke", () => ({
	invoke: mockInvoke,
	listen: mockListen,
}));

const REPO = "/repo";
const WORKTREE = "/repo__wt/feature";
const BRANCH = "feature";

describe("promptForCreatedWorktree", () => {
	let repositoriesStore: typeof import("../../stores/repositories").repositoriesStore;
	let terminalsStore: typeof import("../../stores/terminals").terminalsStore;
	let promptForCreatedWorktree: typeof import("../../hooks/useWorktreeSwitchPrompt").promptForCreatedWorktree;

	beforeEach(async () => {
		vi.resetModules();
		vi.useFakeTimers();
		mockInvoke.mockReset().mockResolvedValue(undefined);
		mockListen.mockReset().mockResolvedValue(vi.fn());
		localStorage.clear();
		vi.doMock("../../invoke", () => ({ invoke: mockInvoke, listen: mockListen }));

		repositoriesStore = (await import("../../stores/repositories")).repositoriesStore;
		terminalsStore = (await import("../../stores/terminals")).terminalsStore;
		repositoriesStore._testSetHydrated(true);
		promptForCreatedWorktree = (await import("../../hooks/useWorktreeSwitchPrompt")).promptForCreatedWorktree;
	});

	afterEach(() => {
		repositoriesStore._testCancelPendingSave();
		terminalsStore._testCancelPendingTimers();
		vi.useRealTimers();
	});

	function seedActiveTerminal(agentType: "codex" | null): string {
		repositoriesStore.add({ path: REPO, displayName: "repo" });
		repositoriesStore.setBranch(REPO, "main", { worktreePath: REPO, isMain: true });
		repositoriesStore.setBranch(REPO, BRANCH, { worktreePath: WORKTREE });
		const terminalId = terminalsStore.add(makeTerminal({ sessionId: "session-main", cwd: REPO, agentType }));
		repositoriesStore.addTerminalToBranch(REPO, "main", terminalId);
		terminalsStore.setActive(terminalId);
		mockInvoke.mockClear();
		return terminalId;
	}

	it("opens the worktree without moving or interrupting a running agent", async () => {
		await testInScopeAsync(async () => {
			const terminalId = seedActiveTerminal("codex");
			const confirm = vi.fn().mockResolvedValue(true);
			const handleBranchSelect = vi.fn().mockResolvedValue(undefined);

			await promptForCreatedWorktree(
				{ confirm, handleBranchSelect, closeTerminalsForBranch: vi.fn() },
				REPO,
				BRANCH,
				WORKTREE,
			);

			expect(confirm).toHaveBeenCalledWith({
				title: "Switch to new worktree?",
				message:
					'Worktree "feature" was created.\nThe running agent cannot switch directories mid-session.\nOpen the worktree in its own terminal and keep the agent in its current branch?',
				okLabel: "Open Worktree",
				cancelLabel: "Stay",
				kind: "info",
				autoCancelMs: 10_000,
			});
			expect(handleBranchSelect).toHaveBeenCalledWith(REPO, BRANCH);
			expect(repositoriesStore.get(REPO)!.branches.main.terminals).toContain(terminalId);
			expect(repositoriesStore.get(REPO)!.branches[BRANCH].terminals).not.toContain(terminalId);
			expect(mockInvoke).not.toHaveBeenCalled();
		});
	});

	it("leaves the running agent untouched when the prompt is declined", async () => {
		await testInScopeAsync(async () => {
			const terminalId = seedActiveTerminal("codex");
			const handleBranchSelect = vi.fn().mockResolvedValue(undefined);

			await promptForCreatedWorktree(
				{ confirm: vi.fn().mockResolvedValue(false), handleBranchSelect, closeTerminalsForBranch: vi.fn() },
				REPO,
				BRANCH,
				WORKTREE,
			);

			expect(handleBranchSelect).not.toHaveBeenCalled();
			expect(repositoriesStore.get(REPO)!.branches.main.terminals).toContain(terminalId);
			expect(repositoriesStore.get(REPO)!.branches[BRANCH].terminals).not.toContain(terminalId);
			expect(mockInvoke).not.toHaveBeenCalled();
		});
	});

	it("skips the prompt entirely when promptOnWorktreeSwitch is off for the repo", async () => {
		await testInScopeAsync(async () => {
			const terminalId = seedActiveTerminal(null);
			const confirm = vi.fn().mockResolvedValue(true);
			const handleBranchSelect = vi.fn().mockResolvedValue(undefined);

			await promptForCreatedWorktree(
				{
					confirm,
					handleBranchSelect,
					closeTerminalsForBranch: vi.fn(),
					getPromptOnWorktreeSwitch: () => false,
				},
				REPO,
				BRANCH,
				WORKTREE,
			);

			expect(confirm).not.toHaveBeenCalled();
			expect(handleBranchSelect).not.toHaveBeenCalled();
			expect(repositoriesStore.get(REPO)!.branches.main.terminals).toContain(terminalId);
			expect(mockInvoke).not.toHaveBeenCalled();
		});
	});

	it("still asks when promptOnWorktreeSwitch is on for the repo", async () => {
		await testInScopeAsync(async () => {
			seedActiveTerminal(null);
			const confirm = vi.fn().mockResolvedValue(true);
			const handleBranchSelect = vi.fn().mockResolvedValue(undefined);

			await promptForCreatedWorktree(
				{
					confirm,
					handleBranchSelect,
					closeTerminalsForBranch: vi.fn(),
					getPromptOnWorktreeSwitch: () => true,
				},
				REPO,
				BRANCH,
				WORKTREE,
			);

			expect(confirm).toHaveBeenCalledWith(expect.objectContaining({ title: "Switch to new worktree?" }));
			expect(handleBranchSelect).toHaveBeenCalledWith(REPO, BRANCH);
		});
	});

	it("keeps moving a plain shell into the worktree", async () => {
		await testInScopeAsync(async () => {
			const terminalId = seedActiveTerminal(null);
			const confirm = vi.fn().mockResolvedValue(true);
			const handleBranchSelect = vi.fn().mockResolvedValue(undefined);

			await promptForCreatedWorktree(
				{ confirm, handleBranchSelect, closeTerminalsForBranch: vi.fn() },
				REPO,
				BRANCH,
				WORKTREE,
			);

			expect(confirm).toHaveBeenCalledWith(expect.objectContaining({ okLabel: "Switch" }));
			expect(handleBranchSelect).toHaveBeenCalledWith(REPO, BRANCH);
			expect(repositoriesStore.get(REPO)!.branches.main.terminals).not.toContain(terminalId);
			expect(repositoriesStore.get(REPO)!.branches[BRANCH].terminals).toContain(terminalId);
			expect(mockInvoke).toHaveBeenCalledWith("write_pty", {
				sessionId: "session-main",
				data: `cd ${WORKTREE}\n`,
			});
		});
	});
});
