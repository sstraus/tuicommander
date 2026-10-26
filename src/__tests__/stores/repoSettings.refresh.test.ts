import { expect, it, vi } from "vitest";
import "../mocks/tauri";
import { createSettingsRefreshCoordinator, settingsRefresh } from "../../services/settingsRefresh";
import { appLogger } from "../../stores/appLogger";
import { repoSettingsStore } from "../../stores/repoSettings";
import { toastsStore } from "../../stores/toasts";
import { mockInvoke } from "../mocks/tauri";

it("omitted defaults adopt peer changes — catches null and empty-map defaults masquerading as unsaved intent", async () => {
	mockInvoke.mockResolvedValue({ repos: { "/repo": { path: "/repo", display_name: "Repo" } } });
	await repoSettingsStore.hydrate();
	let apply: ((value: unknown) => void) | undefined;
	vi.spyOn(settingsRefresh, "register").mockImplementation((reader) => {
		if (reader.section === "repositories") apply = reader.apply;
		return () => {};
	});
	const notice = vi.spyOn(toastsStore, "add").mockReturnValue(0);
	const stop = repoSettingsStore.startRemoteSync();
	try {
		apply!({
			repos: {
				"/repo": { path: "/repo", display_name: "Repo", mcp_upstreams: ["peer"], branch_labels: { main: "Peer" } },
			},
		});
		expect(repoSettingsStore.get("/repo")?.mcpUpstreams).toEqual(["peer"]);
		expect(repoSettingsStore.get("/repo")?.branchLabels).toEqual({ main: "Peer" });
		expect(notice).not.toHaveBeenCalled();
		apply!({ repos: { "/repo": { path: "/repo", display_name: "Repo" } } });
		expect(repoSettingsStore.get("/repo")?.mcpUpstreams).toBeNull();
		expect(repoSettingsStore.get("/repo")?.branchLabels).toEqual({});
		expect(mockInvoke.mock.calls.some(([command]) => command === "save_repo_settings")).toBe(false);
	} finally {
		stop();
		vi.restoreAllMocks();
	}
});

// Catches: one unreachable repository rejecting the entire local-config refresh.
it("local config refresh isolates failed repositories and later recovers", async () => {
	const log = vi.spyOn(appLogger, "debug").mockImplementation(() => {});
	const paths = ["/healthy", "/unreachable", "/deleted"];
	mockInvoke.mockReset().mockResolvedValue({
		repos: Object.fromEntries(paths.map((path) => [path, { path, display_name: path }])),
	});
	await repoSettingsStore.hydrate();
	// IPC deserializes each read independently; do not share one mutable object across repositories.
	mockInvoke.mockImplementation(async () => ({ base_branch: "before" }));
	for (const path of paths) await repoSettingsStore.loadLocalConfig(path);
	let read: (() => Promise<unknown>) | undefined;
	let apply: ((value: unknown) => void) | undefined;
	vi.spyOn(settingsRefresh, "register").mockImplementation((reader) => {
		if (reader.section === "repo_local") {
			read = reader.read;
			apply = reader.apply;
		}
		return () => {};
	});
	const stop = repoSettingsStore.startRemoteSync();
	try {
		mockInvoke.mockImplementation(async (command, args) => {
			if (command !== "load_repo_local_config") throw new Error(`Unexpected command: ${command}`);
			if (args?.repoPath === "/unreachable") throw new Error("remote disconnected");
			return args?.repoPath === "/deleted" ? null : { base_branch: "after" };
		});
		apply!(await read!());
		expect(repoSettingsStore.getEffective("/healthy")?.baseBranch).toBe("after");
		expect(repoSettingsStore.getEffective("/unreachable")?.baseBranch).toBe("before");
		expect(repoSettingsStore.state.localConfigs["/deleted"]).toBeNull();
		expect(log).toHaveBeenCalledWith("config", "Failed to load .tuic.json", {
			path: "/unreachable",
			err: expect.any(Error),
		});
		mockInvoke.mockImplementation(async () => ({ base_branch: "recovered" }));
		apply!(await read!());
		expect(repoSettingsStore.getEffective("/unreachable")?.baseBranch).toBe("recovered");
	} finally {
		stop();
		vi.restoreAllMocks();
	}
});

// Catches: isolating a partial read also cancelling automatic recovery of its failed repository.
it("partial local config refresh retries without another settings event", async () => {
	vi.useFakeTimers();
	const log = vi.spyOn(appLogger, "debug").mockImplementation(() => {});
	const coordinator = createSettingsRefreshCoordinator(async () => () => {});
	vi.spyOn(settingsRefresh, "register").mockImplementation((reader) =>
		reader.section === "repo_local" ? coordinator.register(reader) : () => {},
	);
	mockInvoke.mockReset().mockImplementation(async () => ({ base_branch: "before-retry" }));
	await repoSettingsStore.loadLocalConfig("/retry-healthy");
	await repoSettingsStore.loadLocalConfig("/retry-unreachable");
	mockInvoke.mockImplementation(async (_command, args) => {
		if (args?.repoPath === "/retry-unreachable") throw new Error("remote disconnected");
		return { base_branch: "healthy-now" };
	});
	const stop = repoSettingsStore.startRemoteSync();
	try {
		await vi.advanceTimersByTimeAsync(0);
		expect(repoSettingsStore.state.localConfigs["/retry-healthy"]?.base_branch).toBe("healthy-now");
		expect(repoSettingsStore.state.localConfigs["/retry-unreachable"]?.base_branch).toBe("before-retry");
		expect(log).toHaveBeenCalledWith("config", "Failed to load .tuic.json", {
			path: "/retry-unreachable",
			err: expect.any(Error),
		});
		mockInvoke.mockImplementation(async () => ({ base_branch: "recovered-automatically" }));
		await vi.advanceTimersByTimeAsync(1000);
		expect(repoSettingsStore.state.localConfigs["/retry-unreachable"]?.base_branch).toBe("recovered-automatically");
	} finally {
		stop();
		vi.clearAllTimers();
		vi.useRealTimers();
		vi.restoreAllMocks();
	}
});

// Catches: a transient read error erasing a repository's last-known local settings.
it("direct local config reload preserves the last successful value on failure", async () => {
	const log = vi.spyOn(appLogger, "debug").mockImplementation(() => {});
	try {
		mockInvoke.mockReset().mockResolvedValue({ base_branch: "known" });
		await repoSettingsStore.loadLocalConfig("/cached");
		mockInvoke.mockRejectedValue(new Error("remote disconnected"));
		await repoSettingsStore.loadLocalConfig("/cached");
		expect(repoSettingsStore.state.localConfigs["/cached"]?.base_branch).toBe("known");
		expect(log).toHaveBeenCalledWith("config", "Failed to load .tuic.json", {
			path: "/cached",
			err: expect.any(Error),
		});
		mockInvoke.mockResolvedValue(null);
		await repoSettingsStore.loadLocalConfig("/cached");
		expect(repoSettingsStore.state.localConfigs["/cached"]).toBeNull();
	} finally {
		vi.restoreAllMocks();
	}
});

// Catches: normalizing clean omitted defaults also discarding genuine unsaved repository intent.
it("dirty repository fields survive while untouched peer fields advance", async () => {
	mockInvoke.mockReset().mockResolvedValue({ repos: { "/repo": { path: "/repo", display_name: "Repo" } } });
	await repoSettingsStore.hydrate();
	let apply: ((value: unknown) => void) | undefined;
	vi.spyOn(settingsRefresh, "register").mockImplementation((reader) => {
		if (reader.section === "repositories") apply = reader.apply;
		return () => {};
	});
	const notice = vi.spyOn(toastsStore, "add").mockReturnValue(0);
	const stop = repoSettingsStore.startRemoteSync();
	try {
		repoSettingsStore.update("/repo", { mcpUpstreams: ["draft"] });
		apply!({ repos: { "/repo": { path: "/repo", display_name: "Peer name", mcp_upstreams: ["peer"] } } });
		expect(repoSettingsStore.get("/repo")?.mcpUpstreams).toEqual(["draft"]);
		expect(repoSettingsStore.get("/repo")?.displayName).toBe("Peer name");
		expect(notice).toHaveBeenCalledOnce();
		await Promise.resolve();
	} finally {
		stop();
		vi.restoreAllMocks();
	}
});
