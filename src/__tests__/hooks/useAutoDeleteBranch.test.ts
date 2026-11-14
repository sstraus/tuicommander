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

import { AUTO_DELETE_RETRY_DELAYS_MS, useAutoDeleteBranch } from "../../hooks/useAutoDeleteBranch";
import { keptCheckoutsStore } from "../../stores/keptCheckouts";

/** Backend get_workspace_lifecycle payload for a merged, clean, bound checkout. */
function lifecycle(overrides: Record<string, unknown> = {}) {
	return {
		dirty_files: 0,
		untracked_files: 0,
		live_sessions: [],
		warnings: [],
		missing_checkout: false,
		dirty_fingerprint: "fp-1",
		commit_status: "merged",
		proof_source: "github_pr",
		tip: "tip-1",
		base: "base-1",
		removal_safety: "safe",
		...overrides,
	};
}

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
			Promise.resolve(cmd === "get_workspace_lifecycle" ? lifecycle() : undefined),
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

	let controls: ReturnType<typeof useAutoDeleteBranch> | undefined;
	function setup(): (() => void) | undefined {
		let dispose: (() => void) | undefined;
		createRoot((d) => {
			dispose = d;
			controls = useAutoDeleteBranch({ confirm: mockConfirm, setStatusInfo: mockSetStatusInfo });
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
			if (cmd === "get_workspace_lifecycle") return Promise.resolve(lifecycle());
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
				return Promise.resolve(
					lifecycle({
						live_sessions: [{ session_id: "pty-1", name: "Codex: gate work" }],
						warnings: ["Live session: Codex: gate work"],
					}),
				);
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
				return Promise.resolve(lifecycle({ dirty_files: 1, warnings: ["1 uncommitted file"] }));
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
		await vi.waitFor(() =>
			expect(mockSetStatusInfo).toHaveBeenCalledWith(expect.stringContaining("1 uncommitted file")),
		);
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
			if (cmd === "get_workspace_lifecycle") return Promise.resolve(lifecycle());
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

	const deleteCalls = () => mockInvoke.mock.calls.filter((c) => c[0] === "delete_local_branch");
	const lifecycleCalls = () => mockInvoke.mock.calls.filter((c) => c[0] === "get_workspace_lifecycle");

	// catches: a closed PR (not merge proof) authorising unattended deletion of unintegrated work.
	it("keeps the checkout of a closed-unmerged PR in auto mode", async () => {
		mockGetEffective.mockReturnValue({ autoDeleteOnPrClose: "auto" });
		mockInvoke.mockImplementation((cmd: string) =>
			Promise.resolve(cmd === "get_workspace_lifecycle" ? lifecycle({ commit_status: "unmerged" }) : undefined),
		);
		const dispose = setup();
		getCapturedCallback()("/repo1", "feature/x", 42, "closed");
		await vi.waitFor(() => expect(mockSetStatusInfo).toHaveBeenCalledWith(expect.stringContaining("Kept")));
		expect(deleteCalls()).toHaveLength(0);
		dispose?.();
	});

	// catches: a deletion issued from a proof that names no tip/base/source.
	it("keeps work whose proof is incomplete", async () => {
		mockGetEffective.mockReturnValue({ autoDeleteOnPrClose: "auto" });
		mockInvoke.mockImplementation((cmd: string) =>
			Promise.resolve(cmd === "get_workspace_lifecycle" ? lifecycle({ tip: undefined }) : undefined),
		);
		vi.useFakeTimers();
		const dispose = setup();
		getCapturedCallback()("/repo1", "feature/x", 42, "merged");
		await vi.advanceTimersByTimeAsync(0);
		expect(deleteCalls()).toHaveLength(0);
		dispose?.();
		vi.useRealTimers();
	});

	// catches: marking processed before the async cleanup, so one failed delete is never retried.
	it("retries a failed deletion behind a fresh inspection, deleting once", async () => {
		mockGetEffective.mockReturnValue({ autoDeleteOnPrClose: "auto" });
		let deletes = 0;
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "get_workspace_lifecycle") return Promise.resolve(lifecycle());
			if (cmd === "delete_local_branch" && deletes++ === 0) return Promise.reject(new Error("index.lock busy"));
			return Promise.resolve(undefined);
		});
		vi.useFakeTimers();
		const dispose = setup();
		getCapturedCallback()("/repo1", "feature/x", 42, "merged");
		await vi.advanceTimersByTimeAsync(0);
		expect(deleteCalls()).toHaveLength(1);
		await vi.advanceTimersByTimeAsync(AUTO_DELETE_RETRY_DELAYS_MS[0]);
		expect(deleteCalls()).toHaveLength(2);
		expect(lifecycleCalls()).toHaveLength(2);
		await vi.advanceTimersByTimeAsync(AUTO_DELETE_RETRY_DELAYS_MS[1]);
		expect(deleteCalls()).toHaveLength(2); // success is final
		dispose?.();
		vi.useRealTimers();
	});

	// catches: unbounded retries hammering a permanently failing delete.
	it("stops retrying after the retry budget", async () => {
		mockGetEffective.mockReturnValue({ autoDeleteOnPrClose: "auto" });
		mockInvoke.mockImplementation((cmd: string) =>
			cmd === "delete_local_branch"
				? Promise.reject(new Error("boom"))
				: Promise.resolve(cmd === "get_workspace_lifecycle" ? lifecycle() : undefined),
		);
		vi.useFakeTimers();
		const dispose = setup();
		getCapturedCallback()("/repo1", "feature/x", 42, "merged");
		await vi.advanceTimersByTimeAsync(AUTO_DELETE_RETRY_DELAYS_MS.reduce((a, b) => a + b, 0) * 2);
		expect(deleteCalls()).toHaveLength(AUTO_DELETE_RETRY_DELAYS_MS.length + 1);
		dispose?.();
		vi.useRealTimers();
	});

	// catches: concurrent notifications (in-flight or pending retry) starting a duplicate deletion.
	it("ignores notifications while the first is in flight or awaiting retry", async () => {
		mockGetEffective.mockReturnValue({ autoDeleteOnPrClose: "auto" });
		let release: () => void = () => {};
		mockInvoke.mockImplementation((cmd: string) => {
			if (cmd === "get_workspace_lifecycle")
				return new Promise((resolve) => {
					release = () => resolve(lifecycle());
				});
			return Promise.resolve(undefined);
		});
		const dispose = setup();
		const cb = getCapturedCallback();
		cb("/repo1", "feature/x", 42, "merged");
		cb("/repo1", "feature/x", 42, "merged");
		await vi.waitFor(() => expect(lifecycleCalls()).toHaveLength(1));
		release();
		await vi.waitFor(() => expect(deleteCalls()).toHaveLength(1));
		cb("/repo1", "feature/x", 42, "merged");
		await Promise.resolve();
		expect(deleteCalls()).toHaveLength(1);
		dispose?.();
	});

	// catches: a disabled setting consuming the transition, so enabling it later never cleans up.
	it("does not consume the transition while the setting is off", async () => {
		mockGetEffective.mockReturnValue({ autoDeleteOnPrClose: "off" });
		const dispose = setup();
		const cb = getCapturedCallback();
		cb("/repo1", "feature/x", 42, "merged");
		await Promise.resolve();
		expect(deleteCalls()).toHaveLength(0);
		mockGetEffective.mockReturnValue({ autoDeleteOnPrClose: "auto" });
		cb("/repo1", "feature/x", 42, "merged");
		await vi.waitFor(() => expect(deleteCalls()).toHaveLength(1));
		dispose?.();
	});

	// catches: a pending retry still deleting after the user switched cleanup off.
	it("drops a pending retry when the setting turns off", async () => {
		mockGetEffective.mockReturnValue({ autoDeleteOnPrClose: "auto" });
		mockInvoke.mockImplementation((cmd: string) =>
			cmd === "delete_local_branch"
				? Promise.reject(new Error("busy"))
				: Promise.resolve(cmd === "get_workspace_lifecycle" ? lifecycle() : undefined),
		);
		vi.useFakeTimers();
		const dispose = setup();
		getCapturedCallback()("/repo1", "feature/x", 42, "merged");
		await vi.advanceTimersByTimeAsync(0);
		mockGetEffective.mockReturnValue({ autoDeleteOnPrClose: "off" });
		await vi.advanceTimersByTimeAsync(AUTO_DELETE_RETRY_DELAYS_MS[0]);
		expect(deleteCalls()).toHaveLength(1);
		dispose?.();
		vi.useRealTimers();
	});

	// catches: deleting a tip that moved while the confirm dialog was open.
	it("refuses to delete when the head changed during confirmation", async () => {
		mockGetEffective.mockReturnValue({ autoDeleteOnPrClose: "ask" });
		let inspections = 0;
		mockInvoke.mockImplementation((cmd: string) =>
			Promise.resolve(
				cmd === "get_workspace_lifecycle" ? lifecycle({ tip: inspections++ === 0 ? "tip-1" : "tip-2" }) : undefined,
			),
		);
		const dispose = setup();
		getCapturedCallback()("/repo1", "feature/x", 42, "merged");
		await vi.waitFor(() => expect(mockSetStatusInfo).toHaveBeenCalledWith(expect.stringContaining("changed")));
		expect(deleteCalls()).toHaveLength(0);
		dispose?.();
	});

	// catches: new local changes or a new session appearing during confirmation being deleted.
	it("refuses to delete when a session started during confirmation", async () => {
		mockGetEffective.mockReturnValue({ autoDeleteOnPrClose: "ask" });
		let inspections = 0;
		mockInvoke.mockImplementation((cmd: string) =>
			Promise.resolve(
				cmd === "get_workspace_lifecycle"
					? lifecycle(inspections++ === 0 ? {} : { live_sessions: [{ session_id: "s", name: "agent" }] })
					: undefined,
			),
		);
		const dispose = setup();
		getCapturedCallback()("/repo1", "feature/x", 42, "merged");
		await vi.waitFor(() => expect(mockSetStatusInfo).toHaveBeenCalled());
		expect(deleteCalls()).toHaveLength(0);
		dispose?.();
	});

	// catches: a repeated notification re-asking after the user chose Keep, or Keep that can never be reset.
	it("keeps explicitly until the user resets it", async () => {
		mockGetEffective.mockReturnValue({ autoDeleteOnPrClose: "ask" });
		mockConfirm.mockResolvedValue(false);
		const dispose = setup();
		const cb = getCapturedCallback();
		cb("/repo1", "feature/x", 42, "merged");
		await vi.waitFor(() => expect(mockConfirm).toHaveBeenCalledTimes(1));
		await Promise.resolve();
		cb("/repo1", "feature/x", 42, "merged");
		await Promise.resolve();
		expect(mockConfirm).toHaveBeenCalledTimes(1);

		controls?.resetKeep("/repo1", 42);
		cb("/repo1", "feature/x", 42, "merged");
		await vi.waitFor(() => expect(mockConfirm).toHaveBeenCalledTimes(2));
		dispose?.();
	});

	// catches: a Keep decision being overridden by switching to auto mode.
	it("Keep also beats a later switch to auto", async () => {
		mockGetEffective.mockReturnValue({ autoDeleteOnPrClose: "ask" });
		mockConfirm.mockResolvedValue(false);
		const dispose = setup();
		const cb = getCapturedCallback();
		cb("/repo1", "feature/x", 42, "merged");
		await vi.waitFor(() => expect(mockConfirm).toHaveBeenCalled());
		await Promise.resolve();
		mockGetEffective.mockReturnValue({ autoDeleteOnPrClose: "auto" });
		cb("/repo1", "feature/x", 42, "merged");
		await Promise.resolve();
		expect(deleteCalls()).toHaveLength(0);
		dispose?.();
	});

	// catches: Keep that cannot be reset from the sidebar control without restarting TUIC.
	it("shows Keep for the checkout and the sidebar reset clears it and re-processes the PR", async () => {
		mockGetEffective.mockReturnValue({ autoDeleteOnPrClose: "ask" });
		mockConfirm.mockResolvedValue(false);
		const dispose = setup();
		const cb = getCapturedCallback();
		cb("/repo1", "feature/x", 42, "merged");
		await vi.waitFor(() => expect(keptCheckoutsStore.isKept("/repo1", "feature/x")).toBe(true));

		keptCheckoutsStore.reset("/repo1", "feature/x");
		expect(keptCheckoutsStore.isKept("/repo1", "feature/x")).toBe(false);
		cb("/repo1", "feature/x", 42, "merged");
		await vi.waitFor(() => expect(mockConfirm).toHaveBeenCalledTimes(2));
		dispose?.();
	});
});
