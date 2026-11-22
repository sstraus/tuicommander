import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Mock @tauri-apps/api/core before importing invoke
const mockTauriInvoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
	invoke: (...args: unknown[]) => mockTauriInvoke(...args),
}));

const mockTauriListen = vi.fn();
vi.mock("@tauri-apps/api/event", () => ({
	listen: (...args: unknown[]) => mockTauriListen(...args),
}));

const routing = vi.hoisted(() => ({ owner: undefined as string | undefined, rpc: vi.fn() }));

// Mock transport to simulate Tauri mode
vi.mock("../transport", () => ({
	isTauri: () => true,
	// These suites are desktop-only and register no remote repo, so nothing
	// diverts off local IPC.
	owningConnectionFor: () => routing.owner,
	rpc: (...args: unknown[]) => routing.rpc(...args),
}));

// Import after mocks are set up
const { invoke, listen, _inflight_TEST_ONLY } = await import("../invoke");

describe("invoke in-flight dedup", () => {
	beforeEach(() => {
		mockTauriInvoke.mockReset();
		_inflight_TEST_ONLY.clear();
	});

	it("concurrent identical read-only calls return the same promise", async () => {
		let resolveFirst!: (val: unknown) => void;
		mockTauriInvoke.mockImplementationOnce(
			() =>
				new Promise((r) => {
					resolveFirst = r;
				}),
		);

		const args = { repoPath: "/repo" };
		const p1 = invoke("get_repo_summary", args);
		const p2 = invoke("get_repo_summary", args);

		// Same promise returned — only one Tauri call
		expect(mockTauriInvoke).toHaveBeenCalledTimes(1);

		resolveFirst({ status: "ok" });
		const [r1, r2] = await Promise.all([p1, p2]);
		expect(r1).toEqual({ status: "ok" });
		expect(r2).toEqual({ status: "ok" });
	});

	it("different args get separate promises", async () => {
		mockTauriInvoke.mockResolvedValueOnce("a").mockResolvedValueOnce("b");

		const p1 = invoke("get_repo_summary", { repoPath: "/repo1" });
		const p2 = invoke("get_repo_summary", { repoPath: "/repo2" });

		expect(mockTauriInvoke).toHaveBeenCalledTimes(2);
		expect(await p1).toBe("a");
		expect(await p2).toBe("b");
	});

	it("non-dedup commands bypass dedup (write commands)", async () => {
		mockTauriInvoke.mockResolvedValueOnce("first").mockResolvedValueOnce("second");

		const p1 = invoke("git_stage_files", { path: "/repo", files: ["a.ts"] });
		const p2 = invoke("git_stage_files", { path: "/repo", files: ["a.ts"] });

		// Both should result in separate calls — no dedup for mutations
		expect(mockTauriInvoke).toHaveBeenCalledTimes(2);
		expect(await p1).toBe("first");
		expect(await p2).toBe("second");
	});

	it("promise cleared from cache after settle (resolved)", async () => {
		mockTauriInvoke.mockResolvedValueOnce("ok");

		await invoke("get_repo_summary", { repoPath: "/repo" });
		expect(_inflight_TEST_ONLY.size).toBe(0);
	});

	it("promise cleared from cache after settle (rejected)", async () => {
		mockTauriInvoke.mockRejectedValueOnce(new Error("fail"));

		await expect(invoke("get_repo_summary", { repoPath: "/repo" })).rejects.toThrow("fail");
		expect(_inflight_TEST_ONLY.size).toBe(0);
	});

	it("rejected promises propagate to all subscribers", async () => {
		let rejectFirst!: (err: unknown) => void;
		mockTauriInvoke.mockImplementationOnce(
			() =>
				new Promise((_r, rej) => {
					rejectFirst = rej;
				}),
		);

		const args = { repoPath: "/repo" };
		const p1 = invoke("get_repo_info", args);
		const p2 = invoke("get_repo_info", args);

		expect(mockTauriInvoke).toHaveBeenCalledTimes(1);

		rejectFirst(new Error("network"));
		await expect(p1).rejects.toThrow("network");
		await expect(p2).rejects.toThrow("network");
	});

	it("commands with no args are deduped correctly", async () => {
		let resolveFirst!: (val: unknown) => void;
		mockTauriInvoke.mockImplementationOnce(
			() =>
				new Promise((r) => {
					resolveFirst = r;
				}),
		);

		const p1 = invoke("fetch_plugin_registry");
		const p2 = invoke("fetch_plugin_registry");

		expect(mockTauriInvoke).toHaveBeenCalledTimes(1);

		resolveFirst([]);
		expect(await p1).toEqual([]);
		expect(await p2).toEqual([]);
	});

	// Regression for #1377-9e24: ensure each git read command is in DEDUP_COMMANDS
	// so that repo-changed fan-out doesn't spawn N parallel git processes.
	it.each([
		"get_diff_stats",
		"get_changed_files",
		"get_file_diff",
		"get_git_branches",
		"get_merged_branches",
		"get_recent_commits",
		"get_remote_url",
	])("%s is dedupable", async (cmd) => {
		let resolveFirst!: (val: unknown) => void;
		mockTauriInvoke.mockImplementationOnce(
			() =>
				new Promise((r) => {
					resolveFirst = r;
				}),
		);

		const args = { path: "/repo" };
		const p1 = invoke(cmd, args);
		const p2 = invoke(cmd, args);

		expect(mockTauriInvoke).toHaveBeenCalledTimes(1);

		resolveFirst("ok");
		expect(await p1).toBe("ok");
		expect(await p2).toBe("ok");
	});

	it("get_file_diff dedups by file+scope+untracked tuple", async () => {
		mockTauriInvoke
			.mockResolvedValueOnce("diff-a-staged")
			.mockResolvedValueOnce("diff-a-unstaged")
			.mockResolvedValueOnce("diff-b-staged");

		const a = invoke("get_file_diff", { path: "/r", file: "a.ts", scope: "staged", untracked: false });
		const b = invoke("get_file_diff", { path: "/r", file: "a.ts", scope: "staged", untracked: false });
		const c = invoke("get_file_diff", { path: "/r", file: "a.ts", scope: "unstaged", untracked: false });
		const d = invoke("get_file_diff", { path: "/r", file: "b.ts", scope: "staged", untracked: false });

		// a/b dedupe; c, d are distinct keys → 3 underlying calls
		expect(mockTauriInvoke).toHaveBeenCalledTimes(3);
		expect(await a).toBe("diff-a-staged");
		expect(await b).toBe("diff-a-staged");
		expect(await c).toBe("diff-a-unstaged");
		expect(await d).toBe("diff-b-staged");
	});

	it("after settle, a new call creates a fresh promise", async () => {
		mockTauriInvoke.mockResolvedValueOnce("first").mockResolvedValueOnce("second");

		const r1 = await invoke("get_repo_summary", { repoPath: "/repo" });
		const r2 = await invoke("get_repo_summary", { repoPath: "/repo" });

		expect(mockTauriInvoke).toHaveBeenCalledTimes(2);
		expect(r1).toBe("first");
		expect(r2).toBe("second");
	});
});

describe("listen() unlisten error handling", () => {
	it("swallows the async rejection when Tauri's unlisten double-unregisters", async () => {
		// Tauri's UnlistenFn is `async () => _unlisten(...)`, so a double-unregister
		// (listeners[eventId] is undefined) throws *inside* the async fn and surfaces
		// as a REJECTED PROMISE, not a sync throw. The dispose wrapper must catch that
		// async rejection, otherwise it bubbles up as the "[boot rejection] undefined
		// is not an object (listeners[eventId].handlerId)" unhandled rejection.
		mockTauriListen.mockResolvedValueOnce(() =>
			Promise.reject(new Error("undefined is not an object (evaluating 'listeners[eventId].handlerId')")),
		);
		const dispose = await listen("evt", () => {});

		const seen: unknown[] = [];
		const onUnhandled = (reason: unknown) => seen.push(reason);
		process.on("unhandledRejection", onUnhandled);

		expect(() => dispose()).not.toThrow();
		// Second call hits the disposed guard — must remain a no-op.
		expect(() => dispose()).not.toThrow();

		// Let the rejected promise settle; with the fix it is caught, so nothing
		// reaches the unhandledRejection handler.
		await new Promise((r) => setTimeout(r, 20));
		process.off("unhandledRejection", onUnhandled);
		expect(seen).toEqual([]);
	});
});

describe("development backend mismatch boundary", () => {
	beforeEach(() => {
		mockTauriInvoke.mockReset();
		routing.owner = undefined;
		routing.rpc.mockReset();
		_inflight_TEST_ONLY.clear();
	});
	afterEach(() => {
		routing.owner = undefined;
		vi.unstubAllEnvs();
	});

	it("preserves ordinary command errors instead of misdiagnosing backend skew", async () => {
		for (const failure of [new Error("connection failed"), "Command nested_helper not found", { message: "missing" }]) {
			mockTauriInvoke.mockRejectedValueOnce(failure);
			await expect(invoke("select_ai_chat_conversation")).rejects.toBe(failure);
		}
	});

	it("leaves release unknown-command errors unchanged", async () => {
		vi.stubEnv("DEV", false);
		const failure = "Command select_ai_chat_conversation not found";
		mockTauriInvoke.mockRejectedValueOnce(failure);
		await expect(invoke("select_ai_chat_conversation")).rejects.toBe(failure);
	});

	it("does not tell a satellite to restart its local backend for a remote failure", async () => {
		routing.owner = "remote-host";
		const failure = "Command select_ai_chat_conversation not found";
		routing.rpc.mockRejectedValueOnce(failure);
		await expect(invoke("select_ai_chat_conversation")).rejects.toBe(failure);
		expect(mockTauriInvoke).not.toHaveBeenCalled();
	});

	it("explains deduplicated missing reads to every caller and releases the failed entry", async () => {
		mockTauriInvoke.mockRejectedValueOnce("Command load_config not found");
		const first = invoke("load_config");
		const second = invoke("load_config");
		const failures = await Promise.allSettled([first, second]);
		for (const result of failures) {
			expect(result.status).toBe("rejected");
			if (result.status === "rejected") {
				expect(result.reason.message).toContain("Backend/frontend version mismatch");
				expect(result.reason.message).toContain("Restart make dev");
			}
		}
		expect(mockTauriInvoke).toHaveBeenCalledOnce();
		expect(_inflight_TEST_ONLY.size).toBe(0);
	});
});

describe("recorded backend ownership", () => {
	afterEach(() => {
		routing.owner = undefined;
		routing.rpc.mockReset();
		mockTauriInvoke.mockReset();
	});
	// Catches: a local report following changed repository ownership onto another host.
	it("pins local IPC even when the repository now resolves to a remote host", async () => {
		routing.owner = "new-owner";
		mockTauriInvoke.mockResolvedValueOnce({ delivery: "queued" });
		await invoke("progress_reply", { project: "/same", input: { entryId: 1 } }, null);
		expect(mockTauriInvoke).toHaveBeenCalledWith("progress_reply", { project: "/same", input: { entryId: 1 } });
		expect(routing.rpc).not.toHaveBeenCalled();
	});
	// Catches: a mirrored report rerouting by the currently registered repository instead of its original backend.
	it("pins the recorded remote host instead of the repository's current owner", async () => {
		routing.owner = "new-owner";
		routing.rpc.mockResolvedValueOnce({ delivery: "queued" });
		await invoke("progress_reply", { project: "/same", input: { entryId: 1 } }, "original-host");
		expect(routing.rpc).toHaveBeenCalledWith(
			"progress_reply",
			{ project: "/same", input: { entryId: 1 } },
			"original-host",
		);
		expect(mockTauriInvoke).not.toHaveBeenCalled();
	});
});
