import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";
import { createRoot } from "solid-js";
import { mockInvoke } from "../mocks/tauri";

const {
	mockSetOnPrTerminal,
	mockGetEffective,
	mockGet,
	mockBumpRevision,
	mockBumpGitRevision,
	mockConfirm,
	mockSetStatusInfo,
} = vi.hoisted(() => ({
	mockSetOnPrTerminal: vi.fn(),
	mockGetEffective: vi.fn(),
	mockGet: vi.fn(),
	mockBumpRevision: vi.fn(),
	mockBumpGitRevision: vi.fn(),
	mockConfirm: vi.fn(),
	mockSetStatusInfo: vi.fn(),
}));

vi.mock("../../stores/github", () => ({
	githubStore: {
		setOnPrTerminal: mockSetOnPrTerminal,
	},
}));

vi.mock("../../stores/repoSettings", () => ({
	repoSettingsStore: {
		getEffective: mockGetEffective,
	},
}));

vi.mock("../../stores/repositories", () => ({
	repositoriesStore: {
		get: mockGet,
		bumpRevision: mockBumpRevision,
		bumpGitRevision: mockBumpGitRevision,
		// The real seam: scan the mocked repo's workspaces for one on that branch.
		// Stubbing it as identity would hide the very lookup under test.
		workspaceIdOnBranch: (repoPath: string, branchName: string) => {
			const workspaces = mockGet(repoPath)?.workspaces ?? {};
			for (const [workspaceId, workspace] of Object.entries(workspaces)) {
				if ((workspace as { branchName?: string }).branchName === branchName) return workspaceId;
			}
			return null;
		},
	},
}));

vi.mock("../../stores/appLogger", () => ({
	appLogger: {
		info: vi.fn(),
		warn: vi.fn(),
		error: vi.fn(),
		debug: vi.fn(),
	},
}));

import { useAutoDeleteBranch } from "../../hooks/useAutoDeleteBranch";

/** Simulate the prTerminal callback by capturing what setOnPrTerminal received */
function getCapturedCallback(): (
	repoPath: string,
	branch: string,
	prNumber: number,
	type: "merged" | "closed",
) => void {
	return mockSetOnPrTerminal.mock.calls[0][0];
}

describe("useAutoDeleteBranch", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mockInvoke.mockImplementation((cmd: string) =>
			Promise.resolve(
				cmd === "get_workspace_lifecycle" ? { dirty_files: 0, live_sessions: [], warnings: [] } : undefined,
			),
		);
		mockConfirm.mockResolvedValue(true);
		mockGetEffective.mockReturnValue({ autoDeleteOnPrClose: "off" });
		mockGet.mockReturnValue({
			workspaces: {
				"feature/x": { branchName: "feature/x", isMain: false },
			},
		});
	});

	afterEach(() => {
		vi.restoreAllMocks();
	});

	function setup(): (() => void) | undefined {
		let dispose: (() => void) | undefined;
		createRoot((d) => {
			dispose = d;
			useAutoDeleteBranch({ confirm: mockConfirm, setStatusInfo: mockSetStatusInfo });
		});
		return dispose;
	}

	it("registers prTerminal callback on mount", () => {
		const dispose = setup();
		expect(mockSetOnPrTerminal).toHaveBeenCalledWith(expect.any(Function));
		dispose?.();
	});

	it("unregisters callback on cleanup", () => {
		const dispose = setup();
		dispose?.();
		// Last call should be null (cleanup)
		const calls = mockSetOnPrTerminal.mock.calls;
		expect(calls.at(-1)![0]).toBeNull();
	});

	it("does nothing when setting is off", async () => {
		mockGetEffective.mockReturnValue({ autoDeleteOnPrClose: "off" });
		const dispose = setup();
		const cb = getCapturedCallback();

		cb("/repo1", "feature/x", 42, "merged");
		await vi.waitFor(() => {});

		expect(mockInvoke).not.toHaveBeenCalledWith("delete_local_branch", expect.anything());
		expect(mockConfirm).not.toHaveBeenCalled();
		dispose?.();
	});

	it("auto-deletes silently when setting is auto and worktree is clean", async () => {
		mockGetEffective.mockReturnValue({ autoDeleteOnPrClose: "auto" });
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "get_workspace_lifecycle")
				return Promise.resolve({ dirty_files: 0, live_sessions: [], warnings: [] });
			return Promise.resolve(undefined);
		});
		const dispose = setup();
		const cb = getCapturedCallback();

		cb("/repo1", "feature/x", 42, "merged");

		await vi.waitFor(() => {
			expect(mockInvoke).toHaveBeenCalledWith("delete_local_branch", {
				repoPath: "/repo1",
				branchName: "feature/x",
				workspaceId: "feature/x",
			});
		});

		expect(mockConfirm).not.toHaveBeenCalled();
		expect(mockBumpGitRevision).toHaveBeenCalledWith("/repo1");
		dispose?.();
	});

	it("uses the checkout id when the PR branch has an opaque workspace id", async () => {
		mockGetEffective.mockReturnValue({ autoDeleteOnPrClose: "auto" });
		mockGet.mockReturnValue({ workspaces: { "workspace-42": { branchName: "feature/x", isMain: false } } });
		const dispose = setup();
		getCapturedCallback()("/repo1", "feature/x", 42, "merged");
		await vi.waitFor(() => {
			expect(mockInvoke).toHaveBeenCalledWith("delete_local_branch", {
				repoPath: "/repo1",
				branchName: "feature/x",
				workspaceId: "workspace-42",
			});
		});
		dispose?.();
	});

	it("unattended PR cleanup keeps a checkout used by a live agent and reports why", async () => {
		mockGetEffective.mockReturnValue({ autoDeleteOnPrClose: "auto" });
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "get_workspace_lifecycle") {
				return Promise.resolve({
					dirty_files: 0,
					live_sessions: [{ session_id: "pty-1", name: "Codex: gate work" }],
					warnings: ["Live session: Codex: gate work"],
				});
			}
			return Promise.resolve(undefined);
		});
		const dispose = setup();
		getCapturedCallback()("/repo1", "feature/x", 42, "merged");
		await vi.waitFor(() => {
			expect(mockInvoke).toHaveBeenCalledWith("get_workspace_lifecycle", {
				repoPath: "/repo1",
				workspaceId: "feature/x",
			});
		});
		expect(mockInvoke).not.toHaveBeenCalledWith("delete_local_branch", expect.anything());
		dispose?.();
	});

	it("skips unattended removal when the worktree is dirty", async () => {
		mockGetEffective.mockReturnValue({ autoDeleteOnPrClose: "auto" });
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "get_workspace_lifecycle")
				return Promise.resolve({ dirty_files: 1, live_sessions: [], warnings: ["1 uncommitted file"] });
			return Promise.resolve(undefined);
		});
		const dispose = setup();
		const cb = getCapturedCallback();

		cb("/repo1", "feature/x", 42, "merged");

		await vi.waitFor(() => {
			expect(mockInvoke).toHaveBeenCalledWith("get_workspace_lifecycle", {
				repoPath: "/repo1",
				workspaceId: "feature/x",
			});
		});
		expect(mockConfirm).not.toHaveBeenCalled();
		expect(mockInvoke).not.toHaveBeenCalledWith("delete_local_branch", expect.anything());
		expect(mockSetStatusInfo).toHaveBeenCalledWith(expect.stringContaining("1 uncommitted file"));
		dispose?.();
	});

	it("shows confirm dialog when setting is ask", async () => {
		mockGetEffective.mockReturnValue({ autoDeleteOnPrClose: "ask" });
		mockConfirm.mockResolvedValue(true);
		const dispose = setup();
		const cb = getCapturedCallback();

		cb("/repo1", "feature/x", 42, "closed");

		await vi.waitFor(() => {
			expect(mockConfirm).toHaveBeenCalledWith(
				expect.objectContaining({
					message: expect.stringContaining("PR #42 was closed"),
				}),
			);
		});

		expect(mockInvoke).toHaveBeenCalledWith("delete_local_branch", {
			repoPath: "/repo1",
			branchName: "feature/x",
			workspaceId: "feature/x",
		});
		dispose?.();
	});

	it("does not delete when user cancels confirm dialog", async () => {
		mockGetEffective.mockReturnValue({ autoDeleteOnPrClose: "ask" });
		mockConfirm.mockResolvedValue(false);
		const dispose = setup();
		const cb = getCapturedCallback();

		cb("/repo1", "feature/x", 42, "merged");

		await vi.waitFor(() => {
			expect(mockConfirm).toHaveBeenCalled();
		});

		expect(mockInvoke).not.toHaveBeenCalledWith("delete_local_branch", expect.anything());
		dispose?.();
	});

	it("never deletes the main branch", async () => {
		mockGetEffective.mockReturnValue({ autoDeleteOnPrClose: "auto" });
		mockGet.mockReturnValue({
			workspaces: {
				main: { branchName: "main", isMain: true },
			},
		});
		const dispose = setup();
		const cb = getCapturedCallback();

		cb("/repo1", "main", 99, "merged");
		await vi.waitFor(() => {});

		expect(mockInvoke).not.toHaveBeenCalledWith("delete_local_branch", expect.anything());
		expect(mockConfirm).not.toHaveBeenCalled();
		dispose?.();
	});

	it("skips branch that does not exist locally", async () => {
		mockGetEffective.mockReturnValue({ autoDeleteOnPrClose: "auto" });
		mockGet.mockReturnValue({
			workspaces: {
				"other-branch": { branchName: "other-branch", isMain: false },
			},
		});
		const dispose = setup();
		const cb = getCapturedCallback();

		cb("/repo1", "nonexistent-branch", 42, "merged");
		await vi.waitFor(() => {});

		expect(mockInvoke).not.toHaveBeenCalledWith("delete_local_branch", expect.anything());
		dispose?.();
	});

	it("deduplicates same PR transition", async () => {
		mockGetEffective.mockReturnValue({ autoDeleteOnPrClose: "auto" });
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "get_workspace_lifecycle")
				return Promise.resolve({ dirty_files: 0, live_sessions: [], warnings: [] });
			return Promise.resolve(undefined);
		});
		const dispose = setup();
		const cb = getCapturedCallback();

		cb("/repo1", "feature/x", 42, "merged");
		cb("/repo1", "feature/x", 42, "merged"); // duplicate

		await vi.waitFor(() => {
			expect(mockInvoke).toHaveBeenCalledWith("delete_local_branch", expect.anything());
		});

		const deleteCalls = mockInvoke.mock.calls.filter((c) => c[0] === "delete_local_branch");
		expect(deleteCalls).toHaveLength(1);
		dispose?.();
	});
});
