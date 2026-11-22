import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

/**
 * A repository registered against a remote machine must run its work on that
 * machine. These tests wire the real stores to the real transport and assert on
 * the URL that leaves the process — threading a connection id through arguments
 * proves nothing about which backend answered.
 */

const mockInvoke = vi.fn().mockResolvedValue(undefined);
vi.mock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));

const CONNECTION = "conn-mac-mint";
const DEAD_CONNECTION = "conn-unplugged";
const REMOTE_REPO = "/home/boss/work/api";
const DEAD_REPO = "/home/boss/work/offline";
const LOCAL_REPO = "/Users/boss/Gits/app";
const BASE_URL = "http://mac-mint.test:9877";
const TOKEN = "tok-live";

function jsonResponse(body: string) {
	return {
		ok: true,
		status: 200,
		statusText: "OK",
		headers: new Headers({ "content-type": "application/json" }),
		text: vi.fn().mockResolvedValue(body),
	};
}

describe("a repo registered on a remote machine runs its work there", () => {
	let rpc: typeof import("../transport").rpc;
	let repositoriesStore: typeof import("../stores/repositories").repositoriesStore;
	let terminalsStore: typeof import("../stores/terminals").terminalsStore;
	let getSessionConnection: typeof import("../transportRuntime").getSessionConnection;
	const originalFetch = globalThis.fetch;

	function fetchedUrls(): string[] {
		return (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls.map((call) => String(call[0]));
	}

	beforeEach(async () => {
		vi.resetModules();
		// Registering the repos below schedules a debounced persist. Frozen time
		// keeps that write out of the fetch log every test reads.
		vi.useFakeTimers();
		localStorage.clear();
		delete (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;
		mockInvoke.mockReset().mockResolvedValue(undefined);
		globalThis.fetch = vi.fn().mockResolvedValue(jsonResponse("{}"));

		({ rpc } = await import("../transport"));
		const runtime = await import("../transportRuntime");
		getSessionConnection = runtime.getSessionConnection;
		({ repositoriesStore } = await import("../stores/repositories"));
		({ terminalsStore } = await import("../stores/terminals"));
		repositoriesStore._testSetHydrated(true);

		// Only CONNECTION is connected. DEAD_CONNECTION is registered but has no
		// base URL, which is exactly what a machine that went away looks like.
		runtime.setRemoteBaseUrlLookup((id) => (id === CONNECTION ? BASE_URL : undefined));
		runtime.setRemoteTokenLookup((id) => (id === CONNECTION ? TOKEN : undefined));

		repositoriesStore.add({ path: REMOTE_REPO, displayName: "api", connectionId: CONNECTION });
		repositoriesStore.add({ path: DEAD_REPO, displayName: "offline", connectionId: DEAD_CONNECTION });
		repositoriesStore.add({ path: LOCAL_REPO, displayName: "app" });
		// Registration itself talks to the backend (the GitHub poller's path list),
		// fire-and-forget through a promise chain. Let those land, then forget them:
		// every assertion below is about the call the test makes, not the setup.
		for (let tick = 0; tick < 10; tick += 1) await Promise.resolve();
		(globalThis.fetch as ReturnType<typeof vi.fn>).mockClear();
	});

	afterEach(() => {
		globalThis.fetch = originalFetch;
		vi.useRealTimers();
	});

	// Catches: launch preparation injects the desktop's --settings path into a remote command.
	it("prepares remote agent arguments on the owning daemon instead of local IPC", async () => {
		(globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = {};
		globalThis.fetch = vi.fn().mockResolvedValue(jsonResponse("[]"));
		const { prepareAgentLaunchCommand } = await import("../utils/agentSession");
		expect(await prepareAgentLaunchCommand("claude", null, "claude", REMOTE_REPO)).toBe("claude");
		expect(fetchedUrls()).toEqual([`${BASE_URL}/agents/launch-args?token=${TOKEN}`]);
		expect(mockInvoke).not.toHaveBeenCalled();
	});

	// Catches: nesting the resume cwd in request silently routes a remote agent's resume to local IPC.
	it("resolves a remote agent resume on its owning daemon", async () => {
		(globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = {};
		globalThis.fetch = vi.fn().mockResolvedValue(jsonResponse('{"cmd":null,"verified":false}'));
		const { resolveAgentResume } = await import("../utils/agentSession");
		await resolveAgentResume("claude", REMOTE_REPO, "remote-peer", "remote-agent");
		expect(fetchedUrls()).toEqual([`${BASE_URL}/sessions/suspended?token=${TOKEN}`]);
		expect(mockInvoke).not.toHaveBeenCalled();
	});

	it("sends a git write on a remote repo to that machine", async () => {
		await rpc("git_commit", { path: REMOTE_REPO, message: "wip", amend: false });

		expect(fetchedUrls()).toEqual([`${BASE_URL}/repo/commit?token=${TOKEN}`]);
	});

	it("sends a git read on a remote repo to that machine", async () => {
		await rpc("get_commit_log", { path: REMOTE_REPO, count: 10 });

		expect(fetchedUrls()[0]).toBe(
			`${BASE_URL}/repo/commit-log?path=${encodeURIComponent(REMOTE_REPO)}&count=10&token=${TOKEN}`,
		);
	});

	it("routes a file inside a remote repo, not only the repo root", async () => {
		await rpc("read_file", { path: `${REMOTE_REPO}/crates/core`, file: "lib.rs" });

		expect(fetchedUrls()[0]).toContain(BASE_URL);
	});

	it("subscribes a remote repo's watcher on that machine", async () => {
		await rpc("start_repo_watcher", { repoPath: REMOTE_REPO });

		expect(fetchedUrls()[0]).toBe(`${BASE_URL}/watchers/repo?path=${encodeURIComponent(REMOTE_REPO)}&token=${TOKEN}`);
	});

	it("leaves a local repo on the local backend", async () => {
		await rpc("git_commit", { path: LOCAL_REPO, message: "wip", amend: false });

		const url = fetchedUrls()[0];
		expect(url).not.toContain(BASE_URL);
		expect(url).not.toContain("token=");
	});

	it("says the machine is gone instead of silently running locally", async () => {
		await expect(rpc("get_commit_log", { path: DEAD_REPO })).rejects.toThrow(
			`Remote connection ${DEAD_CONNECTION} not connected`,
		);
		expect(globalThis.fetch).not.toHaveBeenCalled();
	});

	describe("session lifecycle", () => {
		const SESSION = "sess-remote-1";

		/** A tab opened on the remote repo, as the app has it once the PTY exists. */
		function openRemoteTab(repoPath: string | null, cwd: string | null): string {
			return terminalsStore.add({
				sessionId: SESSION,
				fontSize: 13,
				name: "api",
				cwd,
				repoPath,
				awaitingInput: null,
			});
		}

		it("creates the PTY on the machine that holds the repo", async () => {
			await rpc("create_pty", { config: { cwd: REMOTE_REPO } });

			expect(fetchedUrls()).toEqual([`${BASE_URL}/sessions?token=${TOKEN}`]);
		});

		it("creates a worktree PTY on that machine too", async () => {
			await rpc("create_pty_with_worktree", {
				pty_config: { cwd: REMOTE_REPO },
				worktree_config: { base_repo: REMOTE_REPO, branch: "feat" },
			});

			expect(fetchedUrls()).toEqual([`${BASE_URL}/sessions/worktree?token=${TOKEN}`]);
		});

		it("writes to the daemon that spawned the session", async () => {
			openRemoteTab(REMOTE_REPO, REMOTE_REPO);

			await rpc("write_pty", { sessionId: SESSION, data: "ls\r" });

			expect(fetchedUrls()[0]).toBe(`${BASE_URL}/sessions/${SESSION}/write?token=${TOKEN}`);
		});

		// Catches: nested Suspend/Resume routing loses the owning daemon after sessionId is cleared.
		it("suspends and resumes a remote tab on its daemon without a live session id", async () => {
			(globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = {};
			const id = openRemoteTab(REMOTE_REPO, REMOTE_REPO);
			(globalThis.fetch as ReturnType<typeof vi.fn>).mockResolvedValue(
				jsonResponse('{"ok":true,"session_id":"remote-replacement"}'),
			);
			const { suspendTerminal, resumeTerminal } = await import("../utils/suspendTerminal");
			expect(await suspendTerminal(id)).toEqual({ ok: true });
			expect(terminalsStore.get(id)?.sessionId).toBeNull();
			expect(await resumeTerminal(id)).toEqual({ ok: true });
			expect(terminalsStore.get(id)?.sessionId).toBe("remote-replacement");
			expect(fetchedUrls()).toEqual([
				`${BASE_URL}/sessions/suspended?token=${TOKEN}`,
				`${BASE_URL}/sessions/wake?token=${TOKEN}`,
			]);
			expect(mockInvoke).not.toHaveBeenCalled();
		});

		it("resizes and closes on the same daemon", async () => {
			openRemoteTab(REMOTE_REPO, REMOTE_REPO);

			await rpc("resize_pty", { sessionId: SESSION, rows: 40, cols: 120 });
			await rpc("close_pty", { sessionId: SESSION });

			for (const url of fetchedUrls()) expect(url).toContain(BASE_URL);
		});

		/**
		 * Ownership reconciliation assigns `repoPath` after the tab exists, so a
		 * keystroke can arrive while it is still null. The cwd the tab was opened
		 * in answers meanwhile — without it the first writes would land locally.
		 */
		it("routes a session whose repo ownership has not been reconciled yet", async () => {
			openRemoteTab(null, REMOTE_REPO);

			await rpc("write_pty", { sessionId: SESSION, data: "x" });

			expect(fetchedUrls()[0]).toContain(BASE_URL);
		});

		it("hands the owning connection to the terminal's stream transport", () => {
			openRemoteTab(REMOTE_REPO, REMOTE_REPO);

			expect(getSessionConnection(SESSION)).toBe(CONNECTION);
		});

		it("keeps a local session on the local backend", async () => {
			openRemoteTab(LOCAL_REPO, LOCAL_REPO);

			await rpc("write_pty", { sessionId: SESSION, data: "ls\r" });

			expect(getSessionConnection(SESSION)).toBeUndefined();
			expect(fetchedUrls()[0]).not.toContain(BASE_URL);
		});
	});

	/**
	 * `config` is both a PTY spawn request and the app's whole settings object.
	 * Settings describe this app, not a repository, and must never be saved to
	 * another machine because a path-shaped field happens to live in them.
	 */
	it("does not route a settings save that carries an unrelated path", async () => {
		await rpc("save_config", { config: { theme: "dark", path: REMOTE_REPO } });

		expect(fetchedUrls()[0]).not.toContain(BASE_URL);
	});

	describe("commands with no remote route", () => {
		it("runs a host-only command locally and reports that it did", async () => {
			const runtime = await import("../transportRuntime");
			const warn = vi.fn();
			runtime.setTransportLogger({ debug: vi.fn(), warn });

			// A plugin filesystem watch needs the desktop AppHandle to deliver events
			// and has no HTTP route, so routing it would replace a working local call
			// with a throw. It stays local and the warning says so.
			await rpc("plugin_watch_path", { pluginId: "p1", path: `${REMOTE_REPO}/docs` }).catch(() => {});

			expect(warn).toHaveBeenCalledWith(
				"network",
				expect.stringContaining("ran on the local machine"),
				expect.objectContaining({ connectionId: CONNECTION }),
			);
		});
	});

	/**
	 * The satellite case, and the one that was silently broken: a full desktop
	 * TUICommander attached to a remote daemon. `invoke()` short-circuits to Tauri
	 * IPC on the desktop, so a remote repo has to divert before that happens —
	 * IPC reaches the local backend, which knows nothing about the other machine's
	 * disk and would answer about a path it does not have.
	 */
	describe("from the desktop app, where invoke() bypasses rpc()", () => {
		let invoke: typeof import("../invoke").invoke;

		beforeEach(async () => {
			({ invoke } = await import("../invoke"));
			(globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = {};
		});

		afterEach(() => {
			delete (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;
		});

		it("sends a remote repo's call over HTTP instead of local IPC", async () => {
			await invoke("get_commit_log", { path: REMOTE_REPO });

			expect(fetchedUrls()[0]).toContain(BASE_URL);
			expect(mockInvoke).not.toHaveBeenCalled();
		});

		it("still starts a remote repo's watcher on the remote machine", async () => {
			await invoke("start_repo_watcher", { repoPath: REMOTE_REPO });

			expect(fetchedUrls()[0]).toContain(`${BASE_URL}/watchers/repo`);
			expect(mockInvoke).not.toHaveBeenCalled();
		});

		it("leaves a local repo on local IPC", async () => {
			await invoke("get_commit_log", { path: LOCAL_REPO });

			expect(mockInvoke).toHaveBeenCalledWith("get_commit_log", { path: LOCAL_REPO });
			expect(globalThis.fetch).not.toHaveBeenCalled();
		});
	});

	describe("an explicit connection still wins", () => {
		// Catches: browsing the serving TUIC jumps to a registered daemon with the same path.
		it("keeps explicit current-server directory browsing local despite remote path ownership", async () => {
			await rpc("list_directory", { repoPath: REMOTE_REPO, subdir: "" }, "");
			expect(fetchedUrls()).toHaveLength(1);
			expect(fetchedUrls()[0]).not.toContain(BASE_URL);
			expect(fetchedUrls()[0]).not.toContain("token=");
			expect(new URL(fetchedUrls()[0]).origin).toBe(window.location.origin);
		});

		it("probes a path before its repo is registered", async () => {
			await rpc("get_repo_info", { path: "/home/boss/work/not-yet-added" }, CONNECTION);

			expect(fetchedUrls()[0]).toContain(BASE_URL);
		});
	});

	/**
	 * A run config names a binary, a path and an environment on one machine.
	 * `claude`, `grok` and `codex` live on the box that runs them, so a tab opened
	 * on a remote repository has to launch with that machine's `agents.json` — the
	 * local copy can only ever describe this desktop.
	 *
	 * `load_agents_config` carries neither a path nor a session, so the argument
	 * routing every other command relies on cannot place it. The registry names
	 * the connection instead, and that is what these assert.
	 */
	describe("run configs follow the machine that owns the repo", () => {
		const REMOTE_AGENTS = {
			agents: {
				claude: {
					run_configs: [
						{ name: "vps", command: "/opt/claude/bin/claude", args: ["--model", "opus"], is_default: true },
					],
					env_flags: { CLAUDE_CONFIG_DIR: "/srv/.claude" },
				},
			},
		};

		let registry: typeof import("../stores/agentConfigs");

		beforeEach(async () => {
			registry = await import("../stores/agentConfigs");
			(globalThis.fetch as ReturnType<typeof vi.fn>).mockImplementation((url: string) =>
				Promise.resolve(jsonResponse(String(url).includes("/config/agents") ? JSON.stringify(REMOTE_AGENTS) : "{}")),
			);
		});

		it("reads a remote repo's run configs from that machine", async () => {
			const configs = await registry.ensureAgentConfigsForRepo(REMOTE_REPO);

			expect(fetchedUrls()).toEqual([`${BASE_URL}/config/agents?token=${TOKEN}`]);
			expect(configs.getDefaultConfig("claude")?.command).toBe("/opt/claude/bin/claude");
		});

		it("identifies the remote machine and endpoint when its config route is missing", async () => {
			(globalThis.fetch as ReturnType<typeof vi.fn>).mockResolvedValue({
				ok: false,
				status: 404,
				statusText: "Not Found",
				text: async () => "Not Found",
			});

			await expect(registry.ensureAgentConfigsForRepo(REMOTE_REPO)).rejects.toThrow(
				`${CONNECTION} (${BASE_URL}/config/agents): RPC load_agents_config failed: 404`,
			);
			expect(fetchedUrls().filter((url) => url.startsWith(BASE_URL))).toEqual([
				`${BASE_URL}/config/agents?token=${TOKEN}`,
			]);
			expect(registry.agentConfigsForRepo(REMOTE_REPO).state.loaded).toBe(false);
		});

		it("hands the tab the env of the machine it will run on", async () => {
			const configs = await registry.ensureAgentConfigsForRepo(REMOTE_REPO);

			expect(configs.getEnvFlags("claude")).toEqual({ CLAUDE_CONFIG_DIR: "/srv/.claude" });
		});

		it("keeps one config per machine, so the remote one never leaks into the local one", async () => {
			await registry.ensureAgentConfigsForRepo(REMOTE_REPO);

			expect(registry.agentConfigsForRepo(LOCAL_REPO).getRunConfigs("claude")).toEqual([]);
			expect(registry.agentConfigsForRepo(LOCAL_REPO)).not.toBe(registry.agentConfigsForRepo(REMOTE_REPO));
		});

		it("costs a local repo no round trip once the local config is loaded", async () => {
			// What boot does: the local machine's config is read once, up front.
			await registry.ensureAgentConfigs();
			(globalThis.fetch as ReturnType<typeof vi.fn>).mockClear();
			mockInvoke.mockClear();

			await registry.ensureAgentConfigsForRepo(LOCAL_REPO);

			expect(globalThis.fetch).not.toHaveBeenCalled();
			expect(mockInvoke).not.toHaveBeenCalled();
		});

		it("reads a machine again after its config was invalidated", async () => {
			await registry.ensureAgentConfigsForRepo(REMOTE_REPO);
			registry.invalidateAgentConfigs(CONNECTION);
			(globalThis.fetch as ReturnType<typeof vi.fn>).mockClear();

			await registry.ensureAgentConfigsForRepo(REMOTE_REPO);

			expect(fetchedUrls()).toEqual([`${BASE_URL}/config/agents?token=${TOKEN}`]);
		});

		it("does not drop the local config when a machine is invalidated", async () => {
			await registry.ensureAgentConfigs();
			const before = registry.agentConfigsFor();
			registry.invalidateAgentConfigs(CONNECTION);

			expect(registry.agentConfigsFor()).toBe(before);
		});
	});

	/**
	 * The rest of the family a tab depends on. Hooks are installed into an agent's
	 * config directory and upstream MCP servers are dialled by the backend that
	 * holds them, so both belong to the machine, and both have to be reachable on
	 * one that is not this desktop.
	 */
	describe("the rest of the machine family reaches the machine", () => {
		const FAMILY: [string, Record<string, unknown>][] = [
			["load_agents_config", {}],
			["save_agents_config", { config: { agents: {} } }],
			["get_agent_hook_state", { agentType: "claude" }],
			["set_agent_hook_instrumentation", { agentType: "agy", enabled: true }],
			["set_agent_native_status_signals", { agentType: "claude", enabled: false }],
			["load_mcp_upstreams", {}],
			["save_mcp_upstreams", { base: { servers: [] }, config: { servers: [] } }],
			["get_mcp_upstream_status", {}],
		];

		for (const [command, args] of FAMILY) {
			it(`sends ${command} to the machine it is asked for`, async () => {
				await rpc(command, args, CONNECTION);

				expect(fetchedUrls()[0]).toContain(BASE_URL);
			});
		}
	});

	/**
	 * The other half of the ownership rule, and the one a refactor can break
	 * silently: config that describes THIS APP has to stay on this machine even
	 * while every repository in the registry is remote. A settings save sent to a
	 * daemon would move the user's theme onto a server and leave the desktop
	 * reading a file nobody writes.
	 */
	describe("app-local config never follows a repo", () => {
		const LOCAL_ONLY: [string, Record<string, unknown>][] = [
			["load_config", {}],
			["save_config", { config: { theme: "dark" } }],
			["load_keybindings", {}],
			["save_keybindings", { config: { "split-vertical": "Cmd+D" } }],
			["load_pane_layout", {}],
			["save_pane_layout", { layout: { groups: [] } }],
			["load_notification_config", {}],
			["save_notification_config", { config: { enabled: true } }],
			["load_repositories", {}],
			["save_repositories", { config: { repositories: [{ path: REMOTE_REPO }] } }],
		];

		for (const [command, args] of LOCAL_ONLY) {
			it(`keeps ${command} on this machine`, async () => {
				await rpc(command, args);

				const url = fetchedUrls()[0];
				expect(url).not.toContain(BASE_URL);
				expect(url).not.toContain("token=");
			});
		}
	});
});
