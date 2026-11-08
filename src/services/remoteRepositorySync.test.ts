import { afterEach, beforeEach, describe, expect, it, type Mock, vi } from "vitest";

const { handlers, invoke, unlisten } = vi.hoisted(() => ({
	handlers: new Map<string, Set<(event: { payload: unknown }) => void>>(),
	invoke: vi.fn().mockResolvedValue([]),
	unlisten: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({
	listen: vi.fn(async (name: string, handler: (event: { payload: unknown }) => void) => {
		const listeners = handlers.get(name) ?? new Set();
		listeners.add(handler);
		handlers.set(name, listeners);
		return () => {
			listeners.delete(handler);
			unlisten();
		};
	}),
}));

const REMOTE = "/work/remote";
const LOCAL = "/work/local";
const OTHER = "/work/other";
const BASE = "http://daemon.test:9876";

describe("remote repository synchronization", () => {
	let repositories: typeof import("../stores/repositories").repositoriesStore;
	let start: typeof import("./remoteRepositorySync").startRemoteRepositorySync;
	let rpc: typeof import("../transport").rpc;
	let stop: () => void;
	let refresh: Mock<(path: string) => Promise<void>>;
	let fetch: ReturnType<typeof vi.fn>;

	const settle = async () => {
		for (let tick = 0; tick < 30; tick++) await Promise.resolve();
	};
	const emit = (name: string, payload: unknown) => handlers.get(name)?.forEach((handler) => handler({ payload }));
	const connect = () =>
		emit("remote-connection-status", { id: "host", status: "connected", base_url: BASE, token: "test-token" });
	const change = (path = REMOTE, connection = "host", kind = "git-state") =>
		emit("remote-repo-changed", { repo_path: path, kind, __tuic_origin: { connection } });
	const urls = () => fetch.mock.calls.map((call) => String(call[0]));

	beforeEach(async () => {
		vi.resetModules();
		vi.useFakeTimers();
		handlers.clear();
		invoke.mockClear();
		unlisten.mockClear();
		invoke.mockImplementation(async (command: string) => {
			if (command === "list_remote_connections")
				return [{ id: "host", name: "Host", transport: { type: "Direct", url: BASE } }];
			if (command === "remote_connection_statuses")
				return [{ id: "host", status: "connected", base_url: BASE, token: "test-token" }];
			return [];
		});
		fetch = vi.fn().mockResolvedValue({
			ok: true,
			status: 200,
			headers: new Headers({ "content-type": "application/json" }),
			text: async () => "{}",
		});
		vi.stubGlobal("fetch", fetch);
		({ repositoriesStore: repositories } = await import("../stores/repositories"));
		({ startRemoteRepositorySync: start } = await import("./remoteRepositorySync"));
		({ rpc } = await import("../transport"));
		repositories._testSetHydrated(true);
		repositories.add({ path: REMOTE, displayName: "Remote", connectionId: "host" });
		repositories.add({ path: OTHER, displayName: "Other", connectionId: "other-host" });
		repositories.add({ path: LOCAL, displayName: "Local" });
		refresh = vi.fn(async (path: string) => {
			await rpc("get_repo_structure", { repoPath: path });
		});
		await settle();
		stop = start(refresh);
		await settle();
		await vi.advanceTimersByTimeAsync(500);
		await settle();
		expect(refresh).toHaveBeenCalledExactlyOnceWith(REMOTE);
		fetch.mockClear();
		invoke.mockClear();
		refresh.mockClear();
	});

	it("refreshes during a continuous event stream", async () => {
		for (let tick = 0; tick < 10; tick++) {
			change();
			await vi.advanceTimersByTimeAsync(100);
		}
		expect(refresh).toHaveBeenCalledTimes(2);
	});

	afterEach(() => {
		stop();
		vi.clearAllTimers();
		vi.useRealTimers();
		vi.unstubAllGlobals();
	});

	it("coalesces hints and reads the owning daemon with its authentication", async () => {
		change();
		change();
		await vi.advanceTimersByTimeAsync(500);
		expect(refresh).toHaveBeenCalledExactlyOnceWith(REMOTE);
		expect(urls()).toHaveLength(1);
		expect(urls()[0]).toBe(`${BASE}/repo/structure?path=${encodeURIComponent(REMOTE)}&token=test-token`);
		expect(invoke).not.toHaveBeenCalledWith("get_repo_structure", expect.anything());
	});

	it("reconciles created and removed remote workspaces through the real refresh coordinator", async () => {
		stop();
		let featureExists = false;
		fetch.mockImplementation(async (input: string) => {
			const endpoint = new URL(input).pathname;
			const response =
				endpoint === "/repo/structure"
					? {
							worktree_paths: {
								main: { branch: "main", path: REMOTE, kind: "worktree" },
								...(featureExists
									? { feature: { branch: "feature", path: `${REMOTE}/feature`, kind: "worktree" } }
									: {}),
							},
							merged_branches: [],
						}
					: endpoint === "/repo/diff-stats/batch"
						? { diff_stats: {}, last_commit_ts: {}, workspace_statuses: {} }
						: endpoint === "/repo/info"
							? { is_git_repo: false }
							: [];
			return new Response(JSON.stringify(response), { headers: { "content-type": "application/json" } });
		});
		const { useRepository } = await import("../hooks/useRepository");
		const { createRepositoryRefreshCoordinator } = await import("../hooks/git/createRepositoryRefreshCoordinator");
		const coordinator = createRepositoryRefreshCoordinator({
			repo: useRepository(),
			dialogs: {},
			closeTerminal: vi.fn(),
			closeTerminalsInWorktree: vi.fn(),
			setStatusInfo: vi.fn(),
		});
		stop = start(coordinator.refreshAllBranchStats);
		await settle();
		await vi.advanceTimersByTimeAsync(500);
		expect(Object.keys(repositories.get(REMOTE)?.workspaces ?? {})).toEqual(["main"]);
		featureExists = true;
		change();
		await vi.advanceTimersByTimeAsync(5_000);
		expect(repositories.get(REMOTE)?.workspaces.feature.worktreePath).toBe(`${REMOTE}/feature`);
		featureExists = false;
		change();
		await vi.advanceTimersByTimeAsync(5_000);
		// The existing creation grace protects the row initially. It must still
		// disappear after the grace without another event from the daemon.
		expect(repositories.get(REMOTE)?.workspaces.feature).toBeDefined();
		await vi.advanceTimersByTimeAsync(62_000);
		expect(Object.keys(repositories.get(REMOTE)?.workspaces ?? {})).toEqual(["main"]);
		expect(Object.keys(repositories.get(LOCAL)?.workspaces ?? {})).toEqual([]);
		expect(invoke.mock.calls.some(([command]) => ["get_repo_structure", "get_repo_info"].includes(command))).toBe(
			false,
		);
	});

	it.each([LOCAL, OTHER, "/work/unregistered"])("rejects a hint about %s from the wrong owner", async (path) => {
		change(path);
		await vi.advanceTimersByTimeAsync(500);
		expect(refresh).not.toHaveBeenCalled();
	});

	it.each([undefined, null, {}, { connection: "" }, { connection: "other-host" }])(
		"rejects a missing, malformed or mismatched origin %j",
		async (origin) => {
			emit("remote-repo-changed", { repo_path: REMOTE, kind: "git-state", __tuic_origin: origin });
			await vi.advanceTimersByTimeAsync(500);
			expect(refresh).not.toHaveBeenCalled();
		},
	);

	it("does not turn a queued remote path into a local operation after ownership changes", async () => {
		change();
		repositories.remove(REMOTE);
		repositories.add({ path: REMOTE, displayName: "Now local" });
		await vi.advanceTimersByTimeAsync(62_000);
		expect(refresh).not.toHaveBeenCalled();
	});

	it("preserves a new hint merged into the reconciliation after creation grace", async () => {
		await vi.advanceTimersByTimeAsync(61_000);
		change();
		await vi.advanceTimersByTimeAsync(500);
		expect(refresh).toHaveBeenCalledTimes(1);
		await vi.advanceTimersByTimeAsync(61_500);
		expect(refresh).toHaveBeenCalledTimes(2);
		await vi.advanceTimersByTimeAsync(62_000);
		expect(refresh).toHaveBeenCalledTimes(2);
	});

	it("drops buffered hints after disconnection", async () => {
		change();
		emit("remote-connection-status", { id: "host", status: "disconnected" });
		change();
		await vi.advanceTimersByTimeAsync(500);
		expect(refresh).not.toHaveBeenCalled();
	});

	it("re-registers watchers and catches up only the connected owner's repositories", async () => {
		emit("remote-connection-status", { id: "host", status: "disconnected" });
		connect();
		await settle();
		await vi.advanceTimersByTimeAsync(500);
		expect(urls().some((url) => url.startsWith(`${BASE}/watchers/repo?`))).toBe(true);
		expect(refresh).toHaveBeenCalledExactlyOnceWith(REMOTE);
	});

	it("ignores a status forwarded from another remote machine", async () => {
		emit("remote-connection-status", {
			id: "host",
			status: "connected",
			base_url: BASE,
			token: "test-token",
			__tuic_origin: { connection: "other-host" },
		});
		await settle();
		await vi.advanceTimersByTimeAsync(500);
		expect(refresh).not.toHaveBeenCalled();
		expect(urls().some((url) => url.includes("/watchers/repo"))).toBe(false);
	});

	it("cancels pending work and subscriptions on teardown", async () => {
		change();
		stop();
		await vi.advanceTimersByTimeAsync(62_000);
		expect(refresh).not.toHaveBeenCalled();
		expect(unlisten).toHaveBeenCalledTimes(2);
	});
});
