import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";

const { mockRpc } = vi.hoisted(() => ({ mockRpc: vi.fn().mockResolvedValue(undefined) }));

vi.mock("../../transport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../transport")>()),
	rpc: mockRpc,
}));

import { listen } from "@tauri-apps/api/event";
import { handleIntentEvent, shouldApplyIntentTitle } from "../../components/Terminal/intentTitle";
import { type AppInitDeps, browserCreatedSessions, initApp } from "../../hooks/useAppInit";
import { notificationManager } from "../../notifications";
import { activityStore } from "../../stores/activityStore";
import { appLogger } from "../../stores/appLogger";
import { chatViewAvailability } from "../../stores/chatView";
import { closeDialogStore } from "../../stores/closeDialog";
import { editorTabsStore } from "../../stores/editorTabs";
import { globalWorkspaceStore, MANUAL_SCOPE } from "../../stores/globalWorkspace";
import { mdTabsStore } from "../../stores/mdTabs";
import { notificationsStore } from "../../stores/notifications";
import { paneLayoutStore, resetGroupCounter } from "../../stores/paneLayout";
import { repositoriesStore } from "../../stores/repositories";
import { reconcileTerminalOwnership } from "../../stores/terminalOwnership";
import { terminalsStore } from "../../stores/terminals";
import { toastsStore } from "../../stores/toasts";
import * as transport from "../../transport";
import egoAwaiting2103 from "../fixtures/ego-awaiting-2103.json";
import { makeTerminal, testInScopeAsync } from "../helpers/store";
import { mockInvoke } from "../mocks/tauri";

function resetStores() {
	activityStore.clearAll();
	for (const id of terminalsStore.getIds()) {
		terminalsStore.remove(id);
	}
	for (const path of repositoriesStore.getPaths()) {
		repositoriesStore.remove(path);
	}
	for (const id of mdTabsStore.getIds()) {
		mdTabsStore.remove(id);
	}
	for (const id of editorTabsStore.getIds()) {
		editorTabsStore.remove(id);
	}
	// Toasts dedup on title+message+level+repoPath, so one left behind by an
	// earlier test silently suppresses the next test's identical toast.
	for (const toast of [...toastsStore.toasts]) {
		toastsStore.remove(toast.id);
	}
}

function createMockDeps(overrides: Partial<AppInitDeps> = {}): AppInitDeps {
	return {
		pty: {
			listActiveSessions: vi.fn().mockResolvedValue([]),
			close: vi.fn().mockResolvedValue(undefined),
		},
		setStatusInfo: vi.fn(),
		handleBranchSelect: vi.fn().mockResolvedValue(undefined),
		refreshAllBranchStats: vi.fn(),
		getDefaultFontSize: () => 14,
		stores: {
			hydrate: vi.fn().mockResolvedValue(undefined),
			startPolling: vi.fn(),
			stopPolling: vi.fn(),
			startAutoFetch: vi.fn(),
			startPrNotificationTimer: vi.fn(),
			loadFontFromConfig: vi.fn(),
			startDictationSettingsSync: vi.fn(() => vi.fn()),
			refreshDictationConfig: vi.fn().mockResolvedValue(undefined),
			startUserActivityListening: vi.fn(),
		},
		applyPlatformClass: vi.fn().mockReturnValue("macos"),
		registerRepo: vi.fn().mockResolvedValue(undefined),
		...overrides,
	};
}

describe("initApp", () => {
	// Catches: init resolves with a pending progress subscription, leaking its promise and missing early events.
	it("waits for progress event registration before initialization completes", async () => {
		vi.useRealTimers();
		let release!: (unlisten: () => void) => void;
		const pending = new Promise<() => void>((resolve) => {
			release = resolve;
		});
		const subscribe = transport.subscribeEvents;
		const subscription = vi.spyOn(transport, "subscribeEvents").mockImplementation((handlers, options) => {
			if ("progress-recorded" in handlers) return pending;
			return subscribe(handlers, options);
		});
		let finished = false;
		const initializing = initApp(createMockDeps()).then(() => {
			finished = true;
		});
		try {
			await vi.waitFor(() =>
				expect(subscription.mock.calls.some(([handlers]) => "progress-recorded" in handlers)).toBe(true),
			);
			await new Promise((resolve) => setTimeout(resolve, 0));
			expect(finished).toBe(false);
		} finally {
			release(vi.fn());
			await initializing;
		}
	});

	// Catches: a backend mail/submit wake creates a duplicate tab and leaves the original suspended.
	it("reattaches a mail-woken replacement PTY to the suspended tab instead of adding a duplicate", async () => {
		let created: ((event: { payload: unknown }) => void) | undefined;
		vi.mocked(listen).mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
			if (event === "session-created") created = handler;
			return Promise.resolve(vi.fn());
		}) as unknown as typeof listen);
		await initApp(createMockDeps());
		const id = terminalsStore.add(makeTerminal({ sessionId: null, tuicSession: "retained-identity", cwd: "/repo" }));
		terminalsStore.update(id, { suspended: true });
		// The wake producer includes the retained stable identity with the new PTY key.
		created!({
			payload: {
				session_id: "replacement-pty",
				tuic_session: "retained-identity",
				cwd: "/repo",
				agent_type: null,
				display_name: null,
				parent_session: null,
			},
		});
		expect(terminalsStore.getIds()).toEqual([id]);
		expect(terminalsStore.get(id)).toMatchObject({
			suspended: false,
			sessionId: "replacement-pty",
			tuicSession: "retained-identity",
		});
	});

	// Catches: live session adoption drops tuic_session, so declarations cannot move a new worker's sidebar tab.
	it("live_created_caller_declaration_moves_only_its_tab_without_changing_cwd", async () => {
		const handlers = new Map<string, (event: { payload: unknown }) => void>();
		vi.mocked(listen).mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
			handlers.set(event, handler);
			return Promise.resolve(vi.fn());
		}) as unknown as typeof listen);
		repositoriesStore.add({ path: "/repo", displayName: "Repo" });
		repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo", isMain: true });
		await initApp(createMockDeps());
		const { useWorktreeSwitchPrompt } = await import("../../hooks/useWorktreeSwitchPrompt");
		await testInScopeAsync(async () => {
			useWorktreeSwitchPrompt({ handleBranchSelect: vi.fn(), closeTerminalsForBranch: vi.fn() });
			await Promise.resolve();
			const created = handlers.get("session-created")!;
			const caller = {
				payload: { session_id: "caller-pty", tuic_session: "stable-caller", cwd: "/repo", agent_type: "ego" },
			};
			created(caller);
			created({
				payload: { session_id: "sibling-pty", tuic_session: "stable-sibling", cwd: "/repo", agent_type: "ego" },
			});
			const callerId = terminalsStore.findBySessionId("caller-pty")!;
			const siblingId = terminalsStore.findBySessionId("sibling-pty")!;
			const declared = {
				payload: {
					creator_session: "caller-pty",
					spawn_session: false,
					repo_path: "/repo",
					workspace_id: "feature",
					branch: "feature",
					worktree_path: "/repo__wt/feature",
					kind: "worktree",
				},
			};
			handlers.get("session-worktree-declared")!(declared);
			expect(repositoriesStore.findOwnerForTerminal(callerId)?.workspaceId).toBe("feature");
			created(caller);
			handlers.get("session-worktree-declared")!(declared);
			reconcileTerminalOwnership();
			expect(repositoriesStore.get("/repo")?.workspaces.feature.terminals).toEqual([callerId]);
			expect(repositoriesStore.findOwnerForTerminal(siblingId)?.workspaceId).toBe("main");
			expect(terminalsStore.get(callerId)).toMatchObject({ cwd: "/repo", tuicSession: "stable-caller" });
			expect(terminalsStore.getCount()).toBe(2);
		});
	});

	// Catches: HTTP/MCP creation being confused with execution on another machine.
	it("adopts a local MCP-spawned agent without marking its Chat remote", async () => {
		let created: ((event: { payload: unknown }) => void) | undefined;
		vi.mocked(listen).mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
			if (event === "session-created") created = handler;
			return Promise.resolve(vi.fn());
		}) as unknown as typeof listen);
		await initApp(createMockDeps());
		created!({ payload: { session_id: "local-worker", cwd: "/repo", agent_type: "claude", display_name: "worker" } });
		const terminal = terminalsStore.get(terminalsStore.getTerminalForSession("local-worker")!);
		expect(terminal).toMatchObject({ isRemote: false, isExternallyCreated: true });
		expect(chatViewAvailability(terminal)).toEqual({ available: true });
	});

	// Catches: startup reconciliation restoring the creation flag as machine ownership.
	it("keeps local reconnected agents local and configured remote agents remote", async () => {
		await initApp(
			createMockDeps({
				pty: {
					listActiveSessions: vi.fn().mockResolvedValue([
						{ session_id: "local-reconnect", cwd: "/repo", is_remote: true, state: { agent_type: "claude" } },
						{
							session_id: "remote-reconnect",
							cwd: "/remote",
							is_remote: false,
							connection_id: "saved-host",
							state: { agent_type: "claude" },
						},
					]),
					close: vi.fn().mockResolvedValue(undefined),
				},
			}),
		);
		const local = terminalsStore.get(terminalsStore.getTerminalForSession("local-reconnect")!);
		const remote = terminalsStore.get(terminalsStore.getTerminalForSession("remote-reconnect")!);
		expect(local).toMatchObject({ isRemote: false, isExternallyCreated: true });
		expect(chatViewAvailability(local)).toEqual({ available: true });
		expect(remote?.isRemote).toBe(true);
		expect(chatViewAvailability(remote)).toEqual({
			available: false,
			reason: "The chat view is not available for remote terminals",
		});
	});

	it("explains the browser fallback when desktop navigation blocks an external iframe link", async () => {
		let onBlocked: ((event: { payload: string }) => void) | undefined;
		vi.mocked(listen).mockImplementation(((event: string, handler: (event: { payload: string }) => void) => {
			if (event === "navigation-blocked") onBlocked = handler;
			return Promise.resolve(vi.fn());
		}) as unknown as typeof listen);
		await initApp(createMockDeps());
		expect(onBlocked).toBeTypeOf("function");
		onBlocked!({ payload: "https://example.org/help" });
		const toast = toastsStore.toasts.find((item) => item.title === "External link blocked");
		expect(toast?.message).toContain("Open in Browser");
	});
	beforeEach(({ task }) => {
		if (
			/^(live_created_caller_declaration|adopts a local MCP-spawned|keeps local reconnected|reconnect_preserves_declaration|offers Chat for a restored awaiting ego)/.test(
				task.name,
			)
		) {
			vi.spyOn(appLogger, "info").mockImplementation(() => {});
			vi.spyOn(appLogger, "debug").mockImplementation(() => {});
			for (const level of ["warn", "error"] as const) {
				vi.spyOn(appLogger, level).mockImplementation((_source, message) => {
					expect(message).toMatch(
						/Failed to save activity|Failed to load themes|Unknown theme|Settings reload failed|Failed to register progress-recorded|Failed to sync native window background|owned by no registered repo/,
					);
				});
			}
		}
		if (task.name === "settings sync starts every owner and disposes on re-init and unload") {
			// resetStores intentionally saves before hydration; capture its expected rejection too.
			vi.spyOn(appLogger, "error").mockImplementation(() => {});
		}
		vi.useFakeTimers();
		vi.mocked(listen).mockReset().mockResolvedValue(vi.fn());
		resetStores();
		notificationsStore.setAutoToastCards(true);
		sessionStorage.clear();
	});

	afterEach(() => {
		vi.restoreAllMocks();
		vi.useRealTimers();
		repositoriesStore._testCancelPendingSave();
		paneLayoutStore._testCancelPendingSave();
	});

	// Catches: init wiring omitting an owner or re-init retaining its old subscriptions.
	it("settings sync starts every owner and disposes on re-init and unload", async () => {
		vi.spyOn(appLogger, "info").mockImplementation(() => {});
		vi.spyOn(appLogger, "debug").mockImplementation(() => {});
		const errors = vi.spyOn(appLogger, "error").mockImplementation(() => {});
		const warnings = vi.spyOn(appLogger, "warn").mockImplementation(() => {});
		await activityStore.hydrate();
		const owners = [
			(await import("../../stores/mcpPopup")).mcpPopupStore,
			(await import("../../stores/promptLibrary")).promptLibraryStore,
			(await import("../../stores/repoSettings")).repoSettingsStore,
			(await import("../../stores/remoteConnections")).remoteConnectionsStore,
			(await import("../../stores/tunnels")).tunnelsStore,
			(await import("../../stores/ui")).uiStore,
			(await import("../../stores/settings")).settingsStore,
			notificationsStore,
			(await import("../../stores/repoDefaults")).repoDefaultsStore,
			(await import("../../stores/keybindings")).keybindingsStore,
			paneLayoutStore,
		];
		const stops = owners.map(() => vi.fn());
		const starts = owners.map((owner, index) => vi.spyOn(owner, "startRemoteSync").mockReturnValue(stops[index]));
		const agentStop = vi.fn();
		const agents = await import("../../stores/agentConfigs");
		const agentStart = vi.spyOn(agents, "startAgentSettingsSync").mockReturnValue(agentStop);
		const deps = createMockDeps();
		const voiceStop = vi.fn();
		vi.mocked(deps.stores.startDictationSettingsSync).mockReturnValue(voiceStop);
		await initApp(deps);
		for (const start of [...starts, agentStart]) expect(start).toHaveBeenCalledTimes(1);
		for (const stop of [...stops, agentStop, voiceStop]) expect(stop).not.toHaveBeenCalled();
		await initApp(deps);
		for (const start of [...starts, agentStart]) expect(start).toHaveBeenCalledTimes(2);
		for (const stop of [...stops, agentStop, voiceStop]) expect(stop).toHaveBeenCalledTimes(1);
		window.dispatchEvent(new Event("beforeunload"));
		for (const stop of [...stops, agentStop, voiceStop]) expect(stop).toHaveBeenCalledTimes(2);
		activityStore.flushSave();
		await vi.advanceTimersByTimeAsync(0);
		const expectedFixtureErrors = [
			"Failed to save activity",
			"Failed to load themes from backend",
			"Failed to register progress-recorded listener",
			"Failed to sync native window background",
			'Unknown theme "commander", falling back to commander',
		];
		for (const [, message] of [...errors.mock.calls, ...warnings.mock.calls])
			expect(expectedFixtureErrors).toContain(message);
	});

	it("logs the navigation type and document start at each initialization", async () => {
		const navigation = { type: "reload" } as PerformanceNavigationTiming;
		vi.spyOn(performance, "getEntriesByType").mockReturnValue([navigation]);
		const log = vi.spyOn(appLogger, "info");
		await initApp(createMockDeps());
		expect(log).toHaveBeenCalledWith("app", expect.stringContaining("navigation=reload"));
		expect(log).toHaveBeenCalledWith("app", expect.stringContaining(`documentStart=${performance.timeOrigin}`));
	});

	// Catches: ui-tab documents are saved but initApp returns after restoring a branch without restoring them.
	it("restores an MCP Markdown document after unload and repository startup without duplicating its identity", async () => {
		vi.resetModules();
		const { initApp } = await import("../../hooks/useAppInit");
		const { mdTabsStore } = await import("../../stores/mdTabs");
		const { repositoriesStore } = await import("../../stores/repositories");
		let send: ((event: { payload: unknown }) => void) | undefined;
		vi.mocked(listen).mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
			if (event === "ui-tab") send = handler;
			return Promise.resolve(vi.fn());
		}) as unknown as typeof listen);
		const events = vi.spyOn(window, "addEventListener");
		repositoriesStore.add({ path: "/repo", displayName: "repo" });
		repositoriesStore.setWorkspace("/repo", "main", { branchName: "main", worktreePath: "/repo" });
		repositoriesStore.setActiveWorkspace("/repo", "main");
		repositoriesStore.setActive("/repo");
		await initApp(createMockDeps());
		const payload = {
			id: "boss-digest",
			title: "Digest",
			html: "",
			pinned: false,
			url: "tuic://open//Users/boss/Gits/digest.md",
		};
		send!({ payload });
		expect(mdTabsStore.getActive()).toMatchObject({ mcpUiId: "boss-digest" });
		const unload = events.mock.calls.find(([name]) => name === "beforeunload")?.[1];
		expect(unload).toBeTypeOf("function");
		(unload as EventListener)(new Event("beforeunload"));
		// A reload discards the old module graph; a repeated init in one graph is idempotent.
		vi.resetModules();
		const { initApp: reloadApp } = await import("../../hooks/useAppInit");
		const { mdTabsStore: restored } = await import("../../stores/mdTabs");
		const { repositoriesStore: restoredRepos } = await import("../../stores/repositories");
		restoredRepos.add({ path: "/repo", displayName: "repo" });
		restoredRepos.setWorkspace("/repo", "main", { branchName: "main", worktreePath: "/repo" });
		restoredRepos.setActiveWorkspace("/repo", "main");
		restoredRepos.setActive("/repo");
		await reloadApp(createMockDeps());
		expect(restored.getActive()).toMatchObject({
			mcpUiId: "boss-digest",
			filePath: "/Users/boss/Gits/digest.md",
		});
		send!({ payload });
		expect(restored.getCount()).toBe(1);
	});

	// Catches: a rejected branch restore skips Markdown restore and leaves later MCP tabs unsaved.
	it("arms MCP Markdown snapshots even when branch restoration rejects", async () => {
		vi.resetModules();
		const { initApp } = await import("../../hooks/useAppInit");
		const { mdTabsStore } = await import("../../stores/mdTabs");
		const { repositoriesStore } = await import("../../stores/repositories");
		repositoriesStore.add({ path: "/repo", displayName: "repo" });
		repositoriesStore.setWorkspace("/repo", "main", { branchName: "main", worktreePath: "/repo" });
		repositoriesStore.setActiveWorkspace("/repo", "main");
		repositoriesStore.setActive("/repo");
		const failure = new Error("branch restore failed");
		await expect(initApp(createMockDeps({ handleBranchSelect: vi.fn().mockRejectedValue(failure) }))).rejects.toBe(
			failure,
		);
		mdTabsStore.addMcpFile("boss-digest", "", "/Users/boss/Gits/digest.md", false, false);
		vi.resetModules();
		const { mdTabsStore: reloaded } = await import("../../stores/mdTabs");
		reloaded.restoreAfterReload();
		expect(reloaded.getActive()).toMatchObject({
			mcpUiId: "boss-digest",
			filePath: "/Users/boss/Gits/digest.md",
		});
	});

	it("hydrates stores and detects platform", async () => {
		const deps = createMockDeps();
		await initApp(deps);

		expect(deps.applyPlatformClass).toHaveBeenCalled();
		expect(deps.stores.hydrate).toHaveBeenCalled();
		expect(deps.stores.loadFontFromConfig).toHaveBeenCalled();
	});

	it("switches to the owning repo before focusing an MCP native file tab", async () => {
		let uiTabCallback:
			| ((event: {
					payload: {
						id: string;
						title: string;
						html: string;
						pinned: boolean;
						url: string;
						focus: boolean;
						origin_repo_path: string;
						opener_tuic_session: string;
					};
			  }) => void)
			| null = null;
		vi.mocked(listen).mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
			if (event === "ui-tab") uiTabCallback = handler as typeof uiTabCallback;
			return Promise.resolve(vi.fn());
		}) as unknown as typeof listen);

		const sourceRepo = "/repos/investimenti";
		const targetRepo = "/repos/aicheck";
		for (const path of [sourceRepo, targetRepo]) {
			repositoriesStore.add({ path, displayName: path.split("/").pop()! });
			repositoriesStore.setWorkspace(path, "main", { branchName: "main", worktreePath: path });
			repositoriesStore.setActiveWorkspace(path, "main");
		}
		repositoriesStore.setActive(sourceRepo);

		const deps = createMockDeps();
		await initApp(deps);
		uiTabCallback!({
			payload: {
				id: "comparison",
				title: "Comparison",
				html: "",
				pinned: false,
				url: `tuic://open/${targetRepo}/reports/comparison.md`,
				focus: true,
				origin_repo_path: sourceRepo,
				opener_tuic_session: "tuic-b",
			},
		});

		expect(repositoriesStore.state.activeRepoPath).toBe(targetRepo);
		const activeTab = mdTabsStore.getActive();
		expect(activeTab).toMatchObject({
			repoPath: targetRepo,
			filePath: "reports/comparison.md",
			openerTuicSession: "tuic-b",
		});
		expect(mdTabsStore.getVisibleIds(`${targetRepo}|main`)).toContain(activeTab!.id);
	});

	it("does not activate a background MCP file tab that belongs to another repo", async () => {
		let uiTabCallback:
			| ((event: {
					payload: {
						id: string;
						title: string;
						html: string;
						pinned: boolean;
						url: string;
						focus: boolean;
					};
			  }) => void)
			| null = null;
		vi.mocked(listen).mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
			if (event === "ui-tab") uiTabCallback = handler as typeof uiTabCallback;
			return Promise.resolve(vi.fn());
		}) as unknown as typeof listen);

		const sourceRepo = "/repos/investimenti";
		const targetRepo = "/repos/aicheck";
		for (const path of [sourceRepo, targetRepo]) {
			repositoriesStore.add({ path, displayName: path.split("/").pop()! });
			repositoriesStore.setWorkspace(path, "main", { branchName: "main", worktreePath: path });
			repositoriesStore.setActiveWorkspace(path, "main");
		}
		repositoriesStore.setActive(sourceRepo);

		const deps = createMockDeps();
		await initApp(deps);
		uiTabCallback!({
			payload: {
				id: "comparison",
				title: "Comparison",
				html: "",
				pinned: false,
				url: `tuic://open/${targetRepo}/reports/comparison.md`,
				focus: false,
			},
		});

		// `focus: false` deliberately does NOT switch repo, so activating the tab
		// would leave its content on screen with its own tab button filtered out of
		// the tab bar — the exact ghost the focused branch above exists to avoid.
		expect(repositoriesStore.state.activeRepoPath).toBe(sourceRepo);
		const tab = Object.values(mdTabsStore.state.tabs).find(
			(t) => t.type === "file" && t.filePath === "reports/comparison.md",
		);
		expect(tab).toBeDefined();
		expect(tab!.repoPath).toBe(targetRepo);
		expect(mdTabsStore.getActive()?.id).not.toBe(tab!.id);
	});

	it("opens an external Markdown MCP link as a visible Markdown tab", async () => {
		let uiTabCallback:
			| ((event: {
					payload: { id: string; title: string; html: string; pinned: boolean; url: string; focus: boolean };
			  }) => void)
			| null = null;
		vi.mocked(listen).mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
			if (event === "ui-tab") uiTabCallback = handler as typeof uiTabCallback;
			return Promise.resolve(vi.fn());
		}) as unknown as typeof listen);
		await initApp(createMockDeps());
		uiTabCallback!({
			payload: {
				id: "external-report",
				title: "Report",
				html: "",
				pinned: false,
				url: "tuic://open//Users/boss/Gits/.tmp/boss/ego-ux-eval.md",
				focus: true,
			},
		});
		const tab = mdTabsStore.getActive();
		expect(tab).toMatchObject({ type: "file", filePath: "/Users/boss/Gits/.tmp/boss/ego-ux-eval.md" });
		expect(mdTabsStore.getVisibleIds(null)).toContain(tab!.id);
		expect(editorTabsStore.getActive()).toBeUndefined();
	});

	// Catches: tuic://open of an image routed to the editor (UTF-8 read fails, "can't be displayed")
	// or to a Markdown tab, instead of the asset-protocol preview.
	it.each([
		["outside any repo", "tuic://open//Users/boss/Gits/.tmp/boss/shot.png", "/Users/boss/Gits/.tmp/boss/shot.png"],
		["inside a repo", "tuic://open//repos/tuicommander/docs/shot.png", "docs/shot.png"],
	])("opens an image %s in the preview tab, not the editor", async (_label, url, expectedPath) => {
		let uiTabCallback: ((event: { payload: Record<string, unknown> }) => void) | null = null;
		vi.mocked(listen).mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
			if (event === "ui-tab") uiTabCallback = handler as typeof uiTabCallback;
			return Promise.resolve(vi.fn());
		}) as unknown as typeof listen);
		repositoriesStore.add({ path: "/repos/tuicommander", displayName: "tuicommander" });
		await initApp(createMockDeps());
		uiTabCallback!({ payload: { id: "img", title: "Shot", html: "", pinned: false, url, focus: true } });
		expect(mdTabsStore.getActive()).toMatchObject({ type: "html-preview", filePath: expectedPath });
		expect(editorTabsStore.getActive()).toBeUndefined();
	});

	// Catches: html/htm routed to the preview tab by the image routing, bypassing the editor path it used before.
	it("keeps tuic://open of an html file off the preview tab", async () => {
		let uiTabCallback: ((event: { payload: Record<string, unknown> }) => void) | null = null;
		vi.mocked(listen).mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
			if (event === "ui-tab") uiTabCallback = handler as typeof uiTabCallback;
			return Promise.resolve(vi.fn());
		}) as unknown as typeof listen);
		await initApp(createMockDeps());
		uiTabCallback!({
			payload: {
				id: "page",
				title: "Page",
				html: "",
				pinned: true,
				url: "tuic://open//Users/boss/Gits/.tmp/p.html",
				focus: true,
			},
		});
		expect(Object.values(mdTabsStore.state.tabs).some((t) => t.type === "html-preview")).toBe(false);
		expect(editorTabsStore.getActive()).toMatchObject({ filePath: "/Users/boss/Gits/.tmp/p.html" });
	});

	it.each([true, false])("binds an external MCP file to its caller repo with focus=%s", async (focus) => {
		let uiTabCallback:
			| ((event: {
					payload: {
						id: string;
						title: string;
						html: string;
						pinned: boolean;
						url: string;
						focus: boolean;
						origin_repo_path: string;
					};
			  }) => void)
			| null = null;
		vi.mocked(listen).mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
			if (event === "ui-tab") uiTabCallback = handler as typeof uiTabCallback;
			return Promise.resolve(vi.fn());
		}) as unknown as typeof listen);
		for (const repo of ["/repos/orchestrator", "/repos/boss"]) {
			repositoriesStore.add({ path: repo, displayName: repo });
			repositoriesStore.setWorkspace(repo, "main", { worktreePath: repo });
			repositoriesStore.setActiveWorkspace(repo, "main");
		}
		repositoriesStore.setActive("/repos/boss");
		await initApp(createMockDeps());
		uiTabCallback!({
			payload: {
				id: "boss-open-questions",
				title: "Questions",
				html: "",
				pinned: false,
				url: "tuic://open//Users/boss/Gits/.tmp/boss/open-questions.md",
				focus,
				origin_repo_path: "/repos/orchestrator",
			},
		});
		const tab = Object.values(mdTabsStore.state.tabs).find((item) => item.mcpUiId === "boss-open-questions");
		expect(tab?.repoPath).toBe("/repos/orchestrator");
		expect(repositoriesStore.state.activeRepoPath).toBe(focus ? "/repos/orchestrator" : "/repos/boss");
		expect(mdTabsStore.getVisibleIds("/repos/orchestrator|main")).toContain(tab!.id);
	});

	it("uses MCP ids for native Markdown tab identity", async () => {
		let uiTabCallback:
			| ((event: {
					payload: { id: string; title: string; html: string; pinned: boolean; url: string; focus: boolean };
			  }) => void)
			| null = null;
		vi.mocked(listen).mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
			if (event === "ui-tab") uiTabCallback = handler as typeof uiTabCallback;
			return Promise.resolve(vi.fn());
		}) as unknown as typeof listen);
		repositoriesStore.add({ path: "/repo", displayName: "repo" });
		repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
		repositoriesStore.setActiveWorkspace("/repo", "main");
		repositoriesStore.setActive("/repo");
		await initApp(createMockDeps());
		const open = (id: string, path: string) =>
			uiTabCallback!({
				payload: {
					id,
					title: id,
					html: "",
					pinned: false,
					url: `tuic://open/${path}`,
					focus: true,
				},
			});
		const firstPath = "/Users/boss/Gits/.tmp/boss/ego-coordinator-proposal.md";
		const secondPath = "/Users/boss/Gits/.tmp/boss/tuic-mobile-files.md";
		open("proposal", firstPath);
		const firstId = mdTabsStore.state.activeId!;
		open("mobile", firstPath);
		const secondId = mdTabsStore.state.activeId!;
		expect(secondId).not.toBe(firstId);
		expect(mdTabsStore.getVisibleIds("/repo|main")).toEqual(expect.arrayContaining([firstId, secondId]));
		open("proposal", secondPath);
		expect(mdTabsStore.state.activeId).toBe(firstId);
		expect(mdTabsStore.get(firstId)).toMatchObject({ filePath: secondPath });
		expect(mdTabsStore.get(secondId)).toMatchObject({ filePath: firstPath });
		expect(mdTabsStore.add("/repo", firstPath)).not.toBe(secondId);
	});

	it("uses MCP ids for native editor tab identity", async () => {
		let uiTabCallback:
			| ((event: {
					payload: { id: string; title: string; html: string; pinned: boolean; url: string; focus: boolean };
			  }) => void)
			| null = null;
		vi.mocked(listen).mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
			if (event === "ui-tab") uiTabCallback = handler as typeof uiTabCallback;
			return Promise.resolve(vi.fn());
		}) as unknown as typeof listen);
		repositoriesStore.add({ path: "/repo", displayName: "repo" });
		repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
		repositoriesStore.setActiveWorkspace("/repo", "main");
		repositoriesStore.setActive("/repo");
		await initApp(createMockDeps());
		const open = (id: string, path: string) =>
			uiTabCallback!({
				payload: {
					id,
					title: id,
					html: "",
					pinned: false,
					url: `tuic://edit/${path}`,
					focus: true,
				},
			});
		open("first-editor", "/repo/src/main.ts");
		const firstId = editorTabsStore.state.activeId!;
		open("second-editor", "/repo/src/main.ts");
		const secondId = editorTabsStore.state.activeId!;
		expect(secondId).not.toBe(firstId);
		open("first-editor", "/repo/src/other.ts");
		expect(editorTabsStore.state.activeId).toBe(firstId);
		expect(editorTabsStore.get(firstId)).toMatchObject({ filePath: "src/other.ts" });
		expect(editorTabsStore.get(secondId)).toMatchObject({ filePath: "src/main.ts" });
		expect(editorTabsStore.add("/repo", "src/main.ts")).not.toBe(secondId);
	});

	it("replaces an MCP tab when the same id changes between native and HTML routes", async () => {
		let uiTabCallback:
			| ((event: {
					payload: { id: string; title: string; html: string; pinned: boolean; url: string; focus: boolean };
			  }) => void)
			| null = null;
		vi.mocked(listen).mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
			if (event === "ui-tab") uiTabCallback = handler as typeof uiTabCallback;
			return Promise.resolve(vi.fn());
		}) as unknown as typeof listen);
		repositoriesStore.add({ path: "/repo", displayName: "repo" });
		repositoriesStore.setActive("/repo");
		await initApp(createMockDeps());
		const send = (url: string, html = "") =>
			uiTabCallback!({
				payload: {
					id: "same-id",
					title: "Preview",
					html,
					pinned: false,
					url,
					focus: true,
				},
			});
		send("tuic://open//Users/boss/Gits/.tmp/report.md");
		const markdownId = mdTabsStore.state.activeId!;
		send("tuic://edit//Users/boss/Gits/.tmp/report.rs");
		expect(mdTabsStore.get(markdownId)).toBeUndefined();
		const editorId = editorTabsStore.state.activeId!;
		send("", "<p>done</p>");
		expect(editorTabsStore.get(editorId)).toBeUndefined();
		expect(mdTabsStore.getActive()).toMatchObject({ type: "plugin-panel", pluginId: "same-id" });
	});

	it("keeps an external Markdown MCP tab in the background when focus is false", async () => {
		let uiTabCallback:
			| ((event: {
					payload: { id: string; title: string; html: string; pinned: boolean; url: string; focus: boolean };
			  }) => void)
			| null = null;
		vi.mocked(listen).mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
			if (event === "ui-tab") uiTabCallback = handler as typeof uiTabCallback;
			return Promise.resolve(vi.fn());
		}) as unknown as typeof listen);
		await initApp(createMockDeps());
		uiTabCallback!({
			payload: {
				id: "background-report",
				title: "Report",
				html: "",
				pinned: false,
				url: "tuic://open//Users/boss/Gits/.tmp/boss/ego-ux-eval.md",
				focus: false,
			},
		});
		expect(mdTabsStore.getActive()).toBeUndefined();
		expect(mdTabsStore.getVisibleIds(null).length).toBe(1);
	});

	it.each([
		{ command: "open", pinned: false },
		{ command: "edit", pinned: false },
		{ command: "open", pinned: true },
		{ command: "edit", pinned: true },
	])("scopes an external tuic://$command tab with pinned=$pinned", async ({ command, pinned }) => {
		let uiTabCallback:
			| ((event: {
					payload: { id: string; title: string; html: string; pinned: boolean; url: string; focus: boolean };
			  }) => void)
			| null = null;
		vi.mocked(listen).mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
			if (event === "ui-tab") uiTabCallback = handler as typeof uiTabCallback;
			return Promise.resolve(vi.fn());
		}) as unknown as typeof listen);
		for (const path of ["/repos/alpha", "/repos/beta"]) {
			repositoriesStore.add({ path, displayName: path.split("/").pop()! });
			repositoriesStore.setWorkspace(path, "main", { branchName: "main", worktreePath: path });
			repositoriesStore.setActiveWorkspace(path, "main");
		}
		repositoriesStore.setActive("/repos/alpha");
		await initApp(createMockDeps());
		uiTabCallback!({
			payload: {
				id: `external-${command}`,
				title: "External",
				html: "",
				pinned,
				url: `tuic://${command}//Users/boss/Gits/.tmp/boss/ego-ux-eval.${command === "open" ? "md" : "txt"}`,
				focus: true,
			},
		});
		const tabs = command === "open" ? mdTabsStore : editorTabsStore;
		const tabId = tabs.state.activeId!;
		expect(tabs.get(tabId)?.repoPath).toBe("/repos/alpha");
		tabs.setActive(null);
		repositoriesStore.setActive("/repos/beta");
		if (pinned) expect(tabs.getVisibleIds("/repos/beta|main")).toContain(tabId);
		else expect(tabs.getVisibleIds("/repos/beta|main")).not.toContain(tabId);
		repositoriesStore.setActive("/repos/alpha");
		expect(tabs.getVisibleIds("/repos/alpha|main")).toContain(tabId);
	});

	// Catches: reconnect searches declarations only under cwd-derived repo B or null, losing repo A's placement.
	it.each(["/repo-b/src", "/unregistered/src"])(
		"reconnect_preserves_declaration_after_cross_repo_cwd (%s)",
		async (cwd) => {
			repositoriesStore._testSetHydrated(true);
			repositoriesStore.add({ path: "/repo-a", displayName: "repo-a" });
			repositoriesStore.setWorkspace("/repo-a", "main", { worktreePath: "/repo-a" });
			repositoriesStore.setWorkspace("/repo-a", "declared", { worktreePath: "/repo-a__wt/declared" });
			repositoriesStore.setDeclaredWorktree("/repo-a", "stable-declared-peer", {
				workspaceId: "declared",
				branch: "declared",
				worktreePath: "/repo-a__wt/declared",
			});
			repositoriesStore.add({ path: "/repo-b", displayName: "repo-b" });
			repositoriesStore.setWorkspace("/repo-b", "main", { worktreePath: "/repo-b" });
			await initApp(
				createMockDeps({
					pty: {
						listActiveSessions: vi
							.fn()
							.mockResolvedValue([{ session_id: "declared-pty", tuic_session: "stable-declared-peer", cwd }]),
						close: vi.fn().mockResolvedValue(undefined),
					},
				}),
			);
			const id = terminalsStore.getTerminalForSession("declared-pty")!;
			expect(repositoriesStore.findOwnerForTerminal(id)).toEqual({ repoPath: "/repo-a", workspaceId: "declared" });
			expect(terminalsStore.get(id)?.repoPath).toBe("/repo-a");
			expect(terminalsStore.get(id)?.cwd).toBe(cwd);
			expect(globalWorkspaceStore.getScopeMembers(MANUAL_SCOPE)).not.toContain(id);
		},
	);

	it("re-adopts surviving PTY sessions", async () => {
		const deps = createMockDeps({
			pty: {
				listActiveSessions: vi.fn().mockResolvedValue([
					{ session_id: "sess-1", cwd: "/repo" },
					{ session_id: "sess-2", cwd: "/other" },
				]),
				close: vi.fn().mockResolvedValue(undefined),
			},
		});

		await initApp(deps);

		expect(terminalsStore.getCount()).toBe(2);
		const ids = terminalsStore.getIds();
		expect(terminalsStore.get(ids[0])?.sessionId).toBe("sess-1");
		expect(terminalsStore.get(ids[1])?.sessionId).toBe("sess-2");
		expect(terminalsStore.get(ids[0])?.nameIsCustom).toBe(false);
	});

	// Catches: reload disables desktop ego Chat for a pending prompt in the Global Workspace.
	// Recorded list_active_sessions row from the 2021-30cd live audit; see fixture provenance.
	it("offers Chat for a restored awaiting ego session without a saved frontend binding", async () => {
		const deps = createMockDeps({
			pty: {
				listActiveSessions: vi.fn().mockResolvedValue([egoAwaiting2103]),
				close: vi.fn().mockResolvedValue(undefined),
			},
		});
		await initApp(deps);
		const id = terminalsStore.getTerminalForSession(egoAwaiting2103.session_id)!;
		const restored = terminalsStore.get(id)!;
		expect(restored).toMatchObject({ agentType: "ego", awaitingInput: "question", agentSessionId: null });
		expect(chatViewAvailability(restored)).toEqual({ available: true });
	});

	it("preserves an explicitly customized surviving session name", async () => {
		const deps = createMockDeps({
			pty: {
				listActiveSessions: vi.fn().mockResolvedValue([
					{
						session_id: "sess-named",
						cwd: "/repo",
						display_name: "linux-primary",
						display_name_is_custom: true,
					},
				]),
				close: vi.fn().mockResolvedValue(undefined),
			},
		});

		await initApp(deps);

		const terminal = terminalsStore.getIds().map((id) => terminalsStore.get(id))[0];
		expect(terminal?.name).toBe("linux-primary");
		expect(terminal?.nameIsCustom).toBe(true);
	});

	// An explicit spawn name must survive the agent's own OSC 0/2 title. After a
	// reload only the row says it was one, so the backend records the origin.
	// It cannot be inferred from the row's shape: every OSC or intent title is
	// synced back as a non-custom name, and every HTTP-created session is remote,
	// so an inferred flag froze the title of any browser-opened agent tab.
	it("takes the spawn-name flag for re-adopted sessions from the backend's record", async () => {
		const deps = createMockDeps({
			pty: {
				listActiveSessions: vi.fn().mockResolvedValue([
					{
						session_id: "spawned",
						cwd: "/repo",
						display_name: "call-map",
						display_name_is_custom: false,
						display_name_from_spawn: true,
						is_remote: true,
						state: { agent_type: "claude" },
					},
					{
						session_id: "osc-synced",
						cwd: "/repo",
						display_name: "main-wise-beacon",
						display_name_is_custom: false,
						display_name_from_spawn: false,
						is_remote: true,
						state: { agent_type: "claude" },
					},
					{
						session_id: "renamed",
						cwd: "/repo",
						display_name: "mine",
						display_name_is_custom: true,
						is_remote: true,
						state: { agent_type: "claude" },
					},
					{ session_id: "unnamed", cwd: "/repo", is_remote: true, state: { agent_type: "claude" } },
				]),
				close: vi.fn().mockResolvedValue(undefined),
			},
		});

		await initApp(deps);

		const flag = (sid: string) => terminalsStore.get(terminalsStore.getTerminalForSession(sid)!)?.nameFromSpawn;
		expect(flag("spawned")).toBe(true);
		expect(flag("osc-synced")).toBe(false);
		expect(flag("renamed")).toBe(false);
		expect(flag("unnamed")).toBe(false);
	});

	// The spawn parent is published once, on `session-created`. A reload (or a
	// browser that connects later) rebuilds the tab from the row alone, so the
	// sub-agent tag survives only if the row carries the parent.
	it("re-adopts the spawning agent of a surviving sub-agent session", async () => {
		const deps = createMockDeps({
			pty: {
				listActiveSessions: vi.fn().mockResolvedValue([
					{ session_id: "pty-lead", tuic_session: "tuic-lead", cwd: "/repo", display_name: "COORDINATOR" },
					{ session_id: "child", cwd: "/repo", parent_session: "tuic-lead" },
					{ session_id: "plain", cwd: "/repo" },
				]),
				close: vi.fn().mockResolvedValue(undefined),
			},
		});

		await initApp(deps);

		const child = terminalsStore.getTerminalForSession("child")!;
		expect(terminalsStore.get(child)?.parentSession).toBe("tuic-lead");
		expect(terminalsStore.getSubAgentTag(child)).toBe("COORDINATOR");
		expect(terminalsStore.get(terminalsStore.getTerminalForSession("plain")!)?.parentSession).toBeNull();
	});

	it("marks a session-created tab as spawn-named only when the spawn passed a name", async () => {
		let sessionCreated: ((event: { payload: Record<string, unknown> }) => void) | null = null;
		vi.mocked(listen).mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
			if (event === "session-created") sessionCreated = handler as typeof sessionCreated;
			return Promise.resolve(vi.fn());
		}) as unknown as typeof listen);

		await initApp(createMockDeps());

		sessionCreated!({ payload: { session_id: "named", cwd: "/repo", agent_type: "claude", display_name: "call-map" } });
		sessionCreated!({ payload: { session_id: "unnamed", cwd: "/repo", agent_type: "claude", display_name: null } });
		const byId = (sid: string) => terminalsStore.get(terminalsStore.getTerminalForSession(sid)!);
		expect(byId("named")).toMatchObject({ name: "call-map", nameFromSpawn: true, nameIsCustom: true });
		expect(byId("unnamed")?.nameIsCustom).toBe(false);

		// The child's `intent:` title arrives later and must not replace the spawn name.
		const namedId = terminalsStore.getTerminalForSession("named")!;
		handleIntentEvent({
			terminalId: namedId,
			text: "Picking the diagnosis",
			title: "Pick diagnosis",
			globalEnabled: true,
			perAgentEnabled: true,
		});
		expect(byId("named")?.name).toBe("call-map");
		expect(byId("unnamed")?.nameFromSpawn).toBe(false);
	});

	// `term-alias-assigned` fires once, at spawn. A WebView reload rebuilds every
	// tab from this list, so the list is the only place the alias survives — a tab
	// without it loses the address other agents reach it by, and the next restart
	// snapshot saves `alias: null` and asks the backend for a fresh one.
	it("re-adopts a surviving session with the alias the backend holds for it", async () => {
		const deps = createMockDeps({
			pty: {
				listActiveSessions: vi.fn().mockResolvedValue([
					{ session_id: "sess-aliased", cwd: "/repo", alias: "tu-23" },
					{ session_id: "sess-plain", cwd: "/repo" },
				]),
				close: vi.fn().mockResolvedValue(undefined),
			},
		});

		await initApp(deps);

		const bySession = (sid: string) => terminalsStore.get(terminalsStore.getTerminalForSession(sid)!);
		expect(bySession("sess-aliased")?.alias).toBe("tu-23");
		expect(bySession("sess-plain")?.alias).toBeNull();
	});

	// The Context bar above an agent tab renders only once intent or prompt is
	// known. Left to the later lifecycle sync, it appears after the terminal has
	// measured, so the pane shrinks and the PTY sees a transient taller height:
	// Claude repaints for it, and the shrink back pushes those rows into history
	// a second time — duplicated scrollback after every WebView reload.
	it("re-adopts a surviving session with the intent and prompt the backend holds", async () => {
		const deps = createMockDeps({
			pty: {
				listActiveSessions: vi.fn().mockResolvedValue([
					{
						session_id: "sess-context",
						cwd: "/repo",
						state: {
							agent_type: "claude",
							agent_intent: "Answering a question",
							last_prompt: "what is the role",
							last_activity_ms: 1_234_567,
						},
					},
					{ session_id: "sess-bare", cwd: "/repo", state: { agent_type: "claude" } },
				]),
				close: vi.fn().mockResolvedValue(undefined),
			},
		});

		await initApp(deps);

		const bySession = (sid: string) => terminalsStore.get(terminalsStore.getTerminalForSession(sid)!);
		expect(bySession("sess-context")).toMatchObject({
			agentIntent: "Answering a question",
			lastPrompt: "what is the role",
			lastActivityAt: 1_234_567,
		});
		expect(bySession("sess-bare")).toMatchObject({ agentIntent: null, lastPrompt: null, lastActivityAt: null });
	});

	it("re-adopts a remote spawn name as an intent-replaceable base title", async () => {
		let activeSessions = [
			{
				session_id: "remote-agent",
				cwd: "/repo",
				display_name: "repo-audit",
				display_name_is_custom: false,
				is_remote: true,
			},
		];
		const deps = createMockDeps({
			pty: {
				listActiveSessions: vi.fn().mockImplementation(async () => activeSessions),
				close: vi.fn().mockResolvedValue(undefined),
			},
		});

		await initApp(deps);

		const terminal = terminalsStore.getIds().map((id) => terminalsStore.get(id))[0];
		expect(terminal).toMatchObject({ name: "repo-audit", nameIsCustom: false, isExternallyCreated: true });

		const intentTitle = "Fresh audit";
		expect(
			shouldApplyIntentTitle({
				title: intentTitle,
				globalEnabled: true,
				perAgentEnabled: true,
				nameIsCustom: terminal!.nameIsCustom,
			}),
		).toBe(true);
		mockRpc.mockClear();
		handleIntentEvent({
			terminalId: terminal!.id,
			text: "Reviewing reconnect behavior",
			title: intentTitle,
			globalEnabled: true,
			perAgentEnabled: true,
		});
		expect(mockRpc).toHaveBeenCalledWith("set_session_name", {
			sessionId: "remote-agent",
			name: intentTitle,
			isCustom: false,
		});

		activeSessions = [{ ...activeSessions[0], display_name: intentTitle }];
		terminalsStore.remove(terminal!.id);
		await initApp(deps);
		const reconnected = terminalsStore.getIds().map((id) => terminalsStore.get(id))[0];
		expect(reconnected).toMatchObject({ name: intentTitle, nameIsCustom: false, isExternallyCreated: true });
		expect(
			shouldApplyIntentTitle({
				title: "Next audit",
				globalEnabled: true,
				perAgentEnabled: true,
				nameIsCustom: reconnected!.nameIsCustom,
			}),
		).toBe(true);
	});

	it("matches surviving sessions to repos by cwd", async () => {
		repositoriesStore.add({ path: "/repo", displayName: "Repo" });
		repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });

		const deps = createMockDeps({
			pty: {
				listActiveSessions: vi.fn().mockResolvedValue([{ session_id: "sess-1", cwd: "/repo" }]),
				close: vi.fn().mockResolvedValue(undefined),
			},
		});

		await initApp(deps);

		const branch = repositoriesStore.get("/repo")?.workspaces["main"];
		expect(branch?.terminals.length).toBe(1);
	});

	it("matches a surviving session whose cwd is nested below the repo", async () => {
		repositoriesStore.add({ path: "/repo", displayName: "Repo" });
		repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });

		const deps = createMockDeps({
			pty: {
				listActiveSessions: vi.fn().mockResolvedValue([{ session_id: "sess-nested", cwd: "/repo/packages/app" }]),
				close: vi.fn().mockResolvedValue(undefined),
			},
		});

		await initApp(deps);

		const branch = repositoriesStore.get("/repo")?.workspaces["main"];
		expect(branch?.terminals).toHaveLength(1);
		expect(terminalsStore.get(branch!.terminals[0])?.sessionId).toBe("sess-nested");
	});

	it("re-adopts repo-root sessions under main when the active workspace is a linked worktree", async () => {
		repositoriesStore.add({ path: "/repo", displayName: "Repo" });
		repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo", isMain: true });
		repositoriesStore.setWorkspace("/repo", "feature-one", { worktreePath: "/repo__wt/feature-one" });
		repositoriesStore.setWorkspace("/repo", "feature-two", { worktreePath: "/repo__wt/feature-two" });
		repositoriesStore.setWorkspace("/repo", "feature-three", { worktreePath: "/repo__wt/feature-three" });
		repositoriesStore.setActiveWorkspace("/repo", "feature-three");

		const deps = createMockDeps({
			pty: {
				listActiveSessions: vi.fn().mockResolvedValue([
					{ session_id: "root-one", cwd: "/repo" },
					{ session_id: "root-two", cwd: "/repo/src" },
					{ session_id: "worktree-one", cwd: "/repo__wt/feature-one" },
					{ session_id: "worktree-two", cwd: "/repo__wt/feature-two/src" },
				]),
				close: vi.fn().mockResolvedValue(undefined),
			},
		});

		await initApp(deps);

		const repo = repositoriesStore.get("/repo")!;
		expect(repo.workspaces.main.terminals.map((id) => terminalsStore.get(id)?.sessionId)).toEqual([
			"root-one",
			"root-two",
		]);
		expect(repo.workspaces["feature-one"].terminals.map((id) => terminalsStore.get(id)?.sessionId)).toEqual([
			"worktree-one",
		]);
		expect(repo.workspaces["feature-two"].terminals.map((id) => terminalsStore.get(id)?.sessionId)).toEqual([
			"worktree-two",
		]);
		expect(repo.workspaces["feature-three"].terminals).toEqual([]);
	});

	it("keeps an already-adopted repo-root session under main when a new worktree is added", async () => {
		repositoriesStore.add({ path: "/repo", displayName: "Repo" });
		repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo", isMain: true });
		repositoriesStore.setActiveWorkspace("/repo", "main");

		await initApp(
			createMockDeps({
				pty: {
					listActiveSessions: vi.fn().mockResolvedValue([{ session_id: "root", cwd: "/repo/src" }]),
					close: vi.fn().mockResolvedValue(undefined),
				},
			}),
		);

		repositoriesStore.setWorkspace("/repo", "new-worktree", { worktreePath: "/repo__wt/new-worktree" });
		repositoriesStore.setActiveWorkspace("/repo", "new-worktree");
		reconcileTerminalOwnership();

		const repo = repositoriesStore.get("/repo")!;
		expect(repo.workspaces.main.terminals).toHaveLength(1);
		expect(repo.workspaces["new-worktree"].terminals).toEqual([]);
	});

	it("parks a surviving session whose sibling worktree no longer exists", async () => {
		repositoriesStore.add({ path: "/repo", displayName: "Repo" });
		repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo", isMain: true });
		repositoriesStore.setWorkspace("/repo", "current", { worktreePath: "/repo__wt/current" });
		repositoriesStore.setActiveWorkspace("/repo", "current");

		await initApp(
			createMockDeps({
				pty: {
					listActiveSessions: vi.fn().mockResolvedValue([{ session_id: "removed", cwd: "/repo__wt/removed/src" }]),
					close: vi.fn().mockResolvedValue(undefined),
				},
			}),
		);

		const id = terminalsStore.getIds()[0]!;
		expect(terminalsStore.get(id)?.repoPath).toBeNull();
		expect(repositoriesStore.get("/repo")?.workspaces.main.terminals).toEqual([]);
		expect(repositoriesStore.get("/repo")?.workspaces.current.terminals).toEqual([]);
	});

	it("assigns a surviving session to the most-specific nested repo", async () => {
		repositoriesStore.add({ path: "/repo", displayName: "Outer" });
		repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
		repositoriesStore.setWorkspace("/repo", "embedded", { worktreePath: "/repo/packages/app/" });
		repositoriesStore.add({ path: "/repo/packages/app", displayName: "Nested" });
		repositoriesStore.setWorkspace("/repo/packages/app", "main", { worktreePath: "/repo/packages/app" });
		repositoriesStore.setActiveWorkspace("/repo/packages/app", "main");

		const deps = createMockDeps({
			pty: {
				listActiveSessions: vi
					.fn()
					.mockResolvedValue([{ session_id: "sess-nested-repo", cwd: "/repo/packages/app/src" }]),
				close: vi.fn().mockResolvedValue(undefined),
			},
		});

		await initApp(deps);

		expect(repositoriesStore.get("/repo")?.workspaces["main"].terminals).toHaveLength(0);
		expect(repositoriesStore.get("/repo")?.workspaces["embedded"].terminals).toHaveLength(0);
		const nestedBranch = repositoriesStore.get("/repo/packages/app")?.workspaces["main"];
		expect(nestedBranch?.terminals).toHaveLength(1);
		expect(terminalsStore.get(nestedBranch!.terminals[0])?.sessionId).toBe("sess-nested-repo");
	});

	it("prefers a longer external worktree over an enclosing repo root", async () => {
		repositoriesStore.add({ path: "/external", displayName: "External" });
		repositoriesStore.setWorkspace("/external", "main", { worktreePath: "/external" });
		repositoriesStore.add({ path: "/repo", displayName: "Repo" });
		repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
		repositoriesStore.setWorkspace("/repo", "feature", { worktreePath: "/external/feature" });

		const deps = createMockDeps({
			pty: {
				listActiveSessions: vi
					.fn()
					.mockResolvedValue([{ session_id: "sess-external-worktree", cwd: "/external/feature/src" }]),
				close: vi.fn().mockResolvedValue(undefined),
			},
		});

		await initApp(deps);

		expect(repositoriesStore.get("/external")?.workspaces["main"].terminals).toHaveLength(0);
		const feature = repositoriesStore.get("/repo")?.workspaces["feature"];
		expect(feature?.terminals).toHaveLength(1);
		expect(terminalsStore.get(feature!.terminals[0])?.sessionId).toBe("sess-external-worktree");
	});

	it("deduplicates a session-created event while the surviving-session list is pending", async () => {
		repositoriesStore.add({ path: "/repo", displayName: "Repo" });
		repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
		repositoriesStore.setActiveWorkspace("/repo", "main");
		repositoriesStore.setActive("/repo");

		let sessionCreated:
			| ((event: { payload: { session_id: string; cwd: string | null; agent_type?: string | null } }) => void)
			| null = null;
		vi.mocked(listen).mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
			if (event === "session-created") {
				sessionCreated = handler as typeof sessionCreated;
			}
			return Promise.resolve(vi.fn());
		}) as unknown as typeof listen);

		let markListStarted!: () => void;
		const listStarted = new Promise<void>((resolve) => {
			markListStarted = resolve;
		});
		type SurvivingSession = {
			session_id: string;
			cwd: string | null;
			is_remote?: boolean;
			state?: { shell_state?: "busy" | "idle"; agent_state?: "working" | "idle"; background_work?: boolean };
		};
		let resolveSessions!: (sessions: SurvivingSession[]) => void;
		const sessions = new Promise<SurvivingSession[]>((resolve) => {
			resolveSessions = resolve;
		});
		const deps = createMockDeps({
			pty: {
				listActiveSessions: vi.fn(() => {
					markListStarted();
					return sessions;
				}),
				close: vi.fn().mockResolvedValue(undefined),
			},
		});

		const initializing = initApp(deps);
		await listStarted;
		sessionCreated!({ payload: { session_id: "sess-race", cwd: "/repo", agent_type: "codex" } });
		resolveSessions([
			{
				session_id: "sess-race",
				cwd: "/repo",
				is_remote: true,
				state: { shell_state: "idle", agent_state: "working", background_work: true },
			},
		]);
		await initializing;

		const branch = repositoriesStore.get("/repo")?.workspaces["main"];
		expect(terminalsStore.getCount()).toBe(1);
		expect(branch?.terminals).toHaveLength(1);
		expect(new Set(branch?.terminals).size).toBe(1);
		const terminal = terminalsStore.get(branch!.terminals[0]);
		expect(terminal?.sessionId).toBe("sess-race");
		expect(terminal).toMatchObject({ isRemote: false, isExternallyCreated: true });
		expect(terminal?.shellState).toBe("idle");
		expect(terminal?.agentState).toBe("working");
		expect(terminal?.backgroundWork).toBe(true);
	});

	/// A daemon on another machine publishes `session-created` for its own PTYs,
	/// and `remote_mirror.rs` repeats the whole stream. Building a tab for one
	/// attaches this client's transport to a PTY this machine does not run — the
	/// phantom `PTY: Session N` beside the real tab. The mirrored session is
	/// already listed as a row carrying its connection id; the marker is what
	/// tells the two apart.
	it("ignores a session-created event mirrored from another machine", async () => {
		repositoriesStore.add({ path: "/repo", displayName: "Repo" });
		repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
		repositoriesStore.setActiveWorkspace("/repo", "main");
		repositoriesStore.setActive("/repo");

		let sessionCreated: ((event: { payload: Record<string, unknown> }) => void) | null = null;
		vi.mocked(listen).mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
			if (event === "session-created") sessionCreated = handler as typeof sessionCreated;
			return Promise.resolve(vi.fn());
		}) as unknown as typeof listen);

		await initApp(createMockDeps());

		sessionCreated!({
			payload: {
				session_id: "sess-on-mac-mint",
				cwd: "/home/stefano/omi",
				__tuic_origin: { connection: "mac-mint" },
			},
		});
		expect(terminalsStore.getCount()).toBe(0);

		// The control: the same event without the marker is a local session and
		// still opens its tab, so the guard is not "session-created is ignored".
		sessionCreated!({ payload: { session_id: "sess-local", cwd: "/repo" } });
		expect(terminalsStore.getCount()).toBe(1);
	});

	it("does not overwrite a newer shell event while reconciling a deduplicated surviving session", async () => {
		repositoriesStore.add({ path: "/repo", displayName: "Repo" });
		repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
		repositoriesStore.setActiveWorkspace("/repo", "main");
		repositoriesStore.setActive("/repo");

		let sessionCreated:
			| ((event: { payload: { session_id: string; cwd: string | null; agent_type?: string | null } }) => void)
			| null = null;
		vi.mocked(listen).mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
			if (event === "session-created") sessionCreated = handler as typeof sessionCreated;
			return Promise.resolve(vi.fn());
		}) as unknown as typeof listen);

		let markListStarted!: () => void;
		const listStarted = new Promise<void>((resolve) => {
			markListStarted = resolve;
		});
		let resolveSessions!: (
			sessions: Array<{
				session_id: string;
				cwd: string | null;
				state: { shell_state: "busy"; agent_state: "working"; background_work: true };
			}>,
		) => void;
		const sessions = new Promise<
			Array<{
				session_id: string;
				cwd: string | null;
				state: { shell_state: "busy"; agent_state: "working"; background_work: true };
			}>
		>((resolve) => {
			resolveSessions = resolve;
		});
		const deps = createMockDeps({
			pty: {
				listActiveSessions: vi.fn(() => {
					markListStarted();
					return sessions;
				}),
				close: vi.fn().mockResolvedValue(undefined),
			},
		});

		const initializing = initApp(deps);
		await listStarted;
		sessionCreated!({ payload: { session_id: "sess-race-newer", cwd: "/repo", agent_type: "codex" } });
		const terminalId = terminalsStore.getTerminalForSession("sess-race-newer")!;
		terminalsStore.update(terminalId, { shellState: "idle" });
		resolveSessions([
			{
				session_id: "sess-race-newer",
				cwd: "/repo",
				state: { shell_state: "busy", agent_state: "working", background_work: true },
			},
		]);
		await initializing;

		const terminal = terminalsStore.get(terminalId);
		expect(terminal?.shellState).toBe("idle");
		expect(terminal?.agentState).toBe("working");
		expect(terminal?.backgroundWork).toBe(true);
	});

	it("applies a surviving shell snapshot newer than a pre-request shell event", async () => {
		repositoriesStore.add({ path: "/repo", displayName: "Repo" });
		repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
		repositoriesStore.setActiveWorkspace("/repo", "main");
		repositoriesStore.setActive("/repo");

		vi.mocked(listen).mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
			if (event === "session-created") {
				handler({ payload: { session_id: "sess-before-request", cwd: "/repo", agent_type: "codex" } });
				const terminalId = terminalsStore.getTerminalForSession("sess-before-request")!;
				terminalsStore.update(terminalId, { shellState: "idle" });
			}
			return Promise.resolve(vi.fn());
		}) as unknown as typeof listen);

		const deps = createMockDeps({
			pty: {
				listActiveSessions: vi.fn().mockResolvedValue([
					{
						session_id: "sess-before-request",
						cwd: "/repo",
						state: { shell_state: "busy", agent_state: "idle", background_work: false },
					},
				]),
				close: vi.fn().mockResolvedValue(undefined),
			},
		});

		await initApp(deps);

		const terminalId = terminalsStore.getTerminalForSession("sess-before-request")!;
		const terminal = terminalsStore.get(terminalId);
		expect(terminal?.shellState).toBe("busy");
		expect(terminal?.agentState).toBe("idle");
	});

	it("restores active repo/branch and eagerly calls handleBranchSelect", async () => {
		repositoriesStore.add({ path: "/repo", displayName: "Repo" });
		repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
		repositoriesStore.setActiveWorkspace("/repo", "main");

		const deps = createMockDeps();
		await initApp(deps);

		expect(repositoriesStore.state.activeRepoPath).toBe("/repo");
		// Eagerly restore terminals so pane layout IDs match
		expect(deps.handleBranchSelect).toHaveBeenCalledWith("/repo", "main");
	});

	it("does not create terminals when repos exist but no active branch (lazy restore)", async () => {
		repositoriesStore.add({ path: "/repo", displayName: "Repo" });
		// No setWorkspace/setActiveBranch, so activeBranch is undefined

		const deps = createMockDeps();
		await initApp(deps);

		// Lazy restore: no terminals created on startup
		expect(terminalsStore.getCount()).toBe(0);
	});

	it("reports hydration failures in status", async () => {
		const deps = createMockDeps({
			stores: {
				hydrate: vi.fn().mockRejectedValue(new Error("hydration failed")),
				startPolling: vi.fn(),
				stopPolling: vi.fn(),
				startAutoFetch: vi.fn(),
				startPrNotificationTimer: vi.fn(),
				loadFontFromConfig: vi.fn(),
				startDictationSettingsSync: vi.fn(() => vi.fn()),
				refreshDictationConfig: vi.fn().mockResolvedValue(undefined),
				startUserActivityListening: vi.fn(),
			},
		});

		await initApp(deps);

		expect(deps.setStatusInfo).toHaveBeenCalledWith(expect.stringContaining("failed to load"));
	});

	it("starts GitHub polling", async () => {
		const deps = createMockDeps();
		await initApp(deps);

		expect(deps.stores.startPolling).toHaveBeenCalled();
	});

	describe("term-alias-assigned event", () => {
		// Story 761-c847: Terminal.tsx creates the tab locally (sessionId null),
		// awaits PTY creation, then calls terminalsStore.setSessionId — so the
		// backend's alias-assignment event for a freshly spawned session can
		// arrive before that bind exists. The listener must delegate to the
		// store's race-safe applyAlias() rather than resolving the terminal id
		// itself via getTerminalForSession, which would silently drop the event.
		function captureAliasAssigned() {
			const listenMock = vi.mocked(listen);
			let cb: ((event: { payload: { session_id: string; alias: string } }) => void) | null = null;
			listenMock.mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
				if (event === "term-alias-assigned") cb = handler as typeof cb;
				return Promise.resolve(vi.fn());
			}) as unknown as typeof listen);
			return () => cb;
		}

		// An MCP rename starts in the backend, so only this push can tell the tab
		// bar and sidebar about it (#869-e5da).
		it("applies a session-renamed event to the bound terminal", async () => {
			const listenMock = vi.mocked(listen);
			let cb: ((event: { payload: { session_id: string; name: string; is_custom: boolean } }) => void) | null = null;
			listenMock.mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
				if (event === "session-renamed") cb = handler as typeof cb;
				return Promise.resolve(vi.fn());
			}) as unknown as typeof listen);
			await initApp(createMockDeps());
			const id = terminalsStore.add(makeTerminal({ name: "Old name" }));
			terminalsStore.setSessionId(id, "sess-renamed");

			cb!({ payload: { session_id: "sess-renamed", name: "Foo", is_custom: true } });

			expect(terminalsStore.get(id)).toMatchObject({ name: "Foo", nameIsCustom: true });
		});

		it("retains an alias event that arrives before the session is bound to a terminal", async () => {
			const getCb = captureAliasAssigned();
			const deps = createMockDeps();
			await initApp(deps);

			// Mirrors Terminal.tsx's initSession: the tab exists before the PTY
			// session id is known.
			const id = terminalsStore.add(makeTerminal({ name: "Fresh tab" }));

			// Backend assigns the alias and emits the event before setSessionId runs.
			getCb()!({ payload: { session_id: "sess-fresh", alias: "tc-9" } });
			expect(terminalsStore.get(id)?.alias).toBeNull();

			// setSessionId establishes the binding — the retained alias applies now.
			terminalsStore.setSessionId(id, "sess-fresh");
			expect(terminalsStore.get(id)?.alias).toBe("tc-9");
		});

		it("applies an alias event that arrives after the session is already bound", async () => {
			const getCb = captureAliasAssigned();
			const deps = createMockDeps();
			await initApp(deps);

			const id = terminalsStore.add(makeTerminal({ name: "Bound tab" }));
			terminalsStore.setSessionId(id, "sess-bound");

			getCb()!({ payload: { session_id: "sess-bound", alias: "tc-10" } });
			expect(terminalsStore.get(id)?.alias).toBe("tc-10");
		});

		// A mirrored alias names a session on another machine; retaining it would
		// grow the pending map by one entry per remote spawn, never consumed.
		it("ignores an alias event mirrored from a remote daemon", async () => {
			const getCb = captureAliasAssigned();
			await initApp(createMockDeps());

			getCb()!({
				payload: { session_id: "sess-mirrored", alias: "tc-11", __tuic_origin: { connection: "mac-mint" } } as never,
			});
			const id = terminalsStore.add(makeTerminal({ name: "Local tab" }));
			terminalsStore.setSessionId(id, "sess-mirrored");
			expect(terminalsStore.get(id)?.alias).toBeNull();
		});

		// Desktop-created PTYs publish an alias but no bus `session-created`, so a
		// browser client never binds them. The close is the last chance to forget it.
		it("forgets a retained alias once its session closes", async () => {
			const handlers = new Map<string, (event: { payload: unknown }) => void>();
			vi.mocked(listen).mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
				handlers.set(event, handler);
				return Promise.resolve(vi.fn());
			}) as unknown as typeof listen);
			await initApp(createMockDeps());

			handlers.get("term-alias-assigned")!({ payload: { session_id: "sess-gone", alias: "tc-12" } });
			handlers.get("session-closed")!({ payload: { session_id: "sess-gone" } });
			const id = terminalsStore.add(makeTerminal({ name: "Reused id" }));
			terminalsStore.setSessionId(id, "sess-gone");
			expect(terminalsStore.get(id)?.alias).toBeNull();
		});
	});

	it("refreshes all branch stats", async () => {
		const deps = createMockDeps();
		await initApp(deps);

		expect(deps.refreshAllBranchStats).toHaveBeenCalled();
	});

	it("matches surviving session to worktree by cwd", async () => {
		repositoriesStore.add({ path: "/repo", displayName: "Repo" });
		repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
		repositoriesStore.setWorkspace("/repo", "feature", { worktreePath: "/repo/wt-feature" });

		const deps = createMockDeps({
			pty: {
				listActiveSessions: vi.fn().mockResolvedValue([{ session_id: "sess-1", cwd: "/repo/wt-feature" }]),
				close: vi.fn().mockResolvedValue(undefined),
			},
		});

		await initApp(deps);

		const branch = repositoriesStore.get("/repo")?.workspaces["feature"];
		expect(branch?.terminals.length).toBe(1);
	});

	// Catches: reconnect uses a stale declaration to file a root agent under a linked worktree.
	it("restores a repo-root tab by cwd despite a stale declared worktree", async () => {
		repositoriesStore.add({ path: "/repo", displayName: "Repo" });
		repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
		repositoriesStore.setWorkspace("/repo", "feature", { worktreePath: "/repo__wt/feature" });
		const deps = createMockDeps({
			pty: {
				listActiveSessions: vi.fn().mockResolvedValue([
					{
						session_id: "declared-survivor",
						cwd: "/repo",
						worktree_path: "/repo__wt/feature",
						tuic_session: "stable-caller",
					},
					{ session_id: "sibling-survivor", cwd: "/repo" },
				]),
				close: vi.fn().mockResolvedValue(undefined),
			},
		});
		await initApp(deps);
		const declared = terminalsStore.findBySessionId("declared-survivor")!;
		const sibling = terminalsStore.findBySessionId("sibling-survivor")!;
		expect(repositoriesStore.findOwnerForTerminal(declared)?.workspaceId).toBe("main");
		expect(repositoriesStore.findOwnerForTerminal(sibling)?.workspaceId).toBe("main");
		expect(terminalsStore.get(declared)?.cwd).toBe("/repo");
		expect(terminalsStore.get(declared)?.tuicSession).toBe("stable-caller");
	});

	// Catches: re-adoption losing split IDs/suspended tabs or colliding with sessions created while unloaded.
	it("reinit_after_recreate_restores_tabs_layout_and_suspended_tabs", async () => {
		repositoriesStore.add({ path: "/repo", displayName: "Repo" });
		repositoriesStore.setWorkspace("/repo", "main", {
			worktreePath: "/repo",
			lastActiveTerminal: "term-800",
			savedTerminals: [
				{
					frontendId: "term-800",
					sessionId: "live",
					tuicSession: "stable-live",
					name: "Live",
					cwd: "/repo",
					fontSize: 16,
					agentType: "codex",
				},
				{
					frontendId: "term-801",
					tuicSession: "stable-suspended",
					name: "Suspended",
					cwd: "/repo",
					fontSize: 14,
					agentType: "codex",
					suspended: true,
				},
			],
		});
		repositoriesStore.setActiveWorkspace("/repo", "main");
		paneLayoutStore.reset();
		const left = paneLayoutStore.createGroup();
		const right = paneLayoutStore.createGroup();
		paneLayoutStore.addTab(left, { type: "terminal", id: "term-800" });
		paneLayoutStore.addTab(right, { type: "terminal", id: "term-801" });
		paneLayoutStore.setRoot({
			type: "branch",
			direction: "horizontal",
			children: [
				{ type: "leaf", id: left },
				{ type: "leaf", id: right },
			],
			ratios: [0.5, 0.5],
		});
		const layout = paneLayoutStore.serialize();
		const deps = createMockDeps({
			pty: {
				listActiveSessions: vi.fn().mockResolvedValue([
					{ session_id: "created-while-unloaded", cwd: "/repo", tuic_session: "new" },
					{ session_id: "live", cwd: "/repo", tuic_session: "stable-live", state: { agent_type: "codex" } },
				]),
				close: vi.fn().mockResolvedValue(undefined),
			},
		});
		await initApp(deps);
		expect(terminalsStore.get("term-800")?.sessionId).toBe("live");
		expect(terminalsStore.get("term-801")).toMatchObject({
			sessionId: null,
			suspended: true,
			tuicSession: "stable-suspended",
		});
		expect(terminalsStore.getCount()).toBe(3);
		expect(terminalsStore.getTerminalForSession("created-while-unloaded")).not.toBe("term-800");
		expect(terminalsStore.state.activeId).toBe("term-800");
		expect(paneLayoutStore.serialize()).toEqual(layout);
		expect(deps.pty.close).not.toHaveBeenCalled();
	});

	// Catches: an early session-created event replaces a saved pane's terminal identity.
	it("keeps the saved pane binding when session-created races re-adoption", async () => {
		repositoriesStore.add({ path: "/repo", displayName: "Repo" });
		repositoriesStore.setWorkspace("/repo", "main", {
			worktreePath: "/repo",
			lastActiveTerminal: "term-880",
			savedTerminals: [
				{ frontendId: "term-880", agentType: null, sessionId: "race-live", name: "Live", cwd: "/repo", fontSize: 18 },
			],
		});
		repositoriesStore.setActiveWorkspace("/repo", "main");
		paneLayoutStore.reset();
		const group = paneLayoutStore.createGroup();
		paneLayoutStore.addTab(group, { type: "terminal", id: "term-880" });
		paneLayoutStore.setRoot({ type: "leaf", id: group });
		let created: ((event: { payload: unknown }) => void) | undefined;
		vi.mocked(listen).mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
			if (event === "session-created") created = handler;
			return Promise.resolve(vi.fn());
		}) as unknown as typeof listen);
		await initApp(
			createMockDeps({
				pty: {
					listActiveSessions: vi.fn().mockImplementation(async () => {
						created!({ payload: { session_id: "race-live", cwd: "/repo" } });
						terminalsStore.update(terminalsStore.getTerminalForSession("race-live")!, { shellState: "busy" });
						return [{ session_id: "race-live", cwd: "/repo", state: { shell_state: "idle" } }];
					}),
					close: vi.fn().mockResolvedValue(undefined),
				},
			}),
		);
		expect(terminalsStore.get("term-880")).toMatchObject({ sessionId: "race-live", fontSize: 18, shellState: "busy" });
		expect(terminalsStore.state.activeId).toBe("term-880");
		expect(terminalsStore.getCount()).toBe(1);
	});

	// Catches: an inactive split pane loses a suspended tab when its branch has no live sibling.
	it("restores a suspended pane without requiring a live sibling in its workspace", async () => {
		repositoriesStore.add({ path: "/repo", displayName: "Repo" });
		repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
		repositoriesStore.setWorkspace("/repo", "paused", {
			worktreePath: "/repo-paused",
			savedTerminals: [
				{
					frontendId: "term-890",
					agentType: null,
					tuicSession: "paused-peer",
					name: "Paused",
					cwd: "/repo-paused",
					fontSize: 14,
					suspended: true,
				},
			],
		});
		repositoriesStore.setActiveWorkspace("/repo", "main");
		paneLayoutStore.reset();
		const group = paneLayoutStore.createGroup();
		paneLayoutStore.addTab(group, { type: "terminal", id: "term-890" });
		paneLayoutStore.setRoot({ type: "leaf", id: group });
		await initApp(
			createMockDeps({
				pty: {
					listActiveSessions: vi.fn().mockResolvedValue([{ session_id: "other-live", cwd: "/repo" }]),
					close: vi.fn().mockResolvedValue(undefined),
				},
			}),
		);
		expect(terminalsStore.get("term-890")).toMatchObject({
			suspended: true,
			sessionId: null,
			tuicSession: "paused-peer",
		});
	});

	// Catches: restoring an all-suspended workspace twice when lazy branch selection follows init.
	it("consumes suspended snapshots without stable session IDs before lazy branch restore", async () => {
		repositoriesStore.add({ path: "/repo", displayName: "Repo" });
		repositoriesStore.setWorkspace("/repo", "main", {
			worktreePath: "/repo",
			savedTerminals: [
				{ frontendId: "term-891", agentType: null, name: "Paused", cwd: "/repo", fontSize: 14, suspended: true },
			],
		});
		repositoriesStore.setActiveWorkspace("/repo", "main");
		const deps = createMockDeps({
			handleBranchSelect: vi.fn().mockImplementation(async () => {
				expect(repositoriesStore.get("/repo")?.workspaces.main.savedTerminals).toEqual([]);
				expect(repositoriesStore.get("/repo")?.workspaces.main.terminals).toContain("term-891");
			}),
		});
		await initApp(deps);
		expect(deps.handleBranchSelect).toHaveBeenCalledWith("/repo", "main");
		expect(terminalsStore.get("term-891")).toMatchObject({ suspended: true, sessionId: null });
		expect(terminalsStore.getCount()).toBe(1);
	});

	it("restores active branch with surviving sessions", async () => {
		repositoriesStore.add({ path: "/repo", displayName: "Repo" });
		repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
		repositoriesStore.setActiveWorkspace("/repo", "main");

		// Add a terminal that will be cleared and re-adopted
		const deps = createMockDeps({
			pty: {
				listActiveSessions: vi.fn().mockResolvedValue([{ session_id: "sess-1", cwd: "/repo" }]),
				close: vi.fn().mockResolvedValue(undefined),
			},
		});

		await initApp(deps);

		expect(repositoriesStore.state.activeRepoPath).toBe("/repo");
		// Should activate an existing terminal, not call handleBranchSelect
		const ids = terminalsStore.getIds();
		expect(ids.length).toBe(1);
	});

	// The active repo must NOT lend a slot to a session it does not own. Doing that
	// made an unowned tab's home depend on which repo happened to have focus when
	// the session arrived, so two sessions from one unregistered repo landed under
	// two different repos. It goes to the Global Workspace instead — the same place
	// every time, regardless of where the user is standing.
	it("parks an unmatched surviving session in the Global Workspace, not the active branch", async () => {
		repositoriesStore.add({ path: "/repo", displayName: "Repo" });
		repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
		repositoriesStore.setActiveWorkspace("/repo", "main");
		repositoriesStore.setActive("/repo");

		const deps = createMockDeps({
			pty: {
				listActiveSessions: vi.fn().mockResolvedValue([{ session_id: "sess-1", cwd: "/other" }]),
				close: vi.fn().mockResolvedValue(undefined),
			},
		});

		await initApp(deps);

		expect(repositoriesStore.get("/repo")?.workspaces["main"].terminals).toHaveLength(0);
		expect(terminalsStore.getCount()).toBe(1);

		const termId = terminalsStore.getIds()[0];
		expect(terminalsStore.get(termId)?.sessionId).toBe("sess-1");
		// `repoPath: null` is the parked marker reconcile later keys off.
		expect(terminalsStore.get(termId)?.repoPath).toBeNull();
		expect(globalWorkspaceStore.getScopeMembers(MANUAL_SCOPE)).toContain(termId);

		// Consequence worth stating: the active branch is now genuinely empty, so
		// init opens a terminal for it. The borrowed tab used to satisfy "this
		// branch has a terminal" and suppress that — a foreign session standing in
		// for the user's own. One extra terminal is the price of not lying about
		// ownership.
		expect(deps.handleBranchSelect).toHaveBeenCalledWith("/repo", "main");
	});

	// Where the user stands must not change where the tab lands. Same unregistered
	// repo, two sessions, two different active repos at adoption time.
	it("parks two sessions from one unregistered repo together, whatever repo has focus", async () => {
		repositoriesStore.add({ path: "/repo-a", displayName: "A" });
		repositoriesStore.setWorkspace("/repo-a", "main", { worktreePath: "/repo-a" });
		repositoriesStore.setActiveWorkspace("/repo-a", "main");
		repositoriesStore.add({ path: "/repo-b", displayName: "B" });
		repositoriesStore.setWorkspace("/repo-b", "main", { worktreePath: "/repo-b" });
		repositoriesStore.setActiveWorkspace("/repo-b", "main");
		repositoriesStore.setActive("/repo-a");

		await initApp(
			createMockDeps({
				pty: {
					listActiveSessions: vi.fn().mockResolvedValue([{ session_id: "sess-1", cwd: "/unmapped/wt-one" }]),
					close: vi.fn().mockResolvedValue(undefined),
				},
			}),
		);

		repositoriesStore.setActive("/repo-b");

		await initApp(
			createMockDeps({
				pty: {
					listActiveSessions: vi.fn().mockResolvedValue([
						{ session_id: "sess-1", cwd: "/unmapped/wt-one" },
						{ session_id: "sess-2", cwd: "/unmapped/wt-two" },
					]),
					close: vi.fn().mockResolvedValue(undefined),
				},
			}),
		);

		expect(repositoriesStore.get("/repo-a")?.workspaces["main"].terminals).toHaveLength(0);
		expect(repositoriesStore.get("/repo-b")?.workspaces["main"].terminals).toHaveLength(0);
		expect(globalWorkspaceStore.getScopeMembers(MANUAL_SCOPE)).toHaveLength(2);
	});

	describe("parked-tab toast registration", () => {
		// A worktree of an unregistered repo: unregisteredRepoRootFor strips the
		// `__wt/<branch>` suffix, so the root the user must register is /gits/ls/gate-os.
		const PARKED_CWD = "/gits/ls/gate-os__wt/poc-0001-blade";
		const DEDUCED_ROOT = "/gits/ls/gate-os";

		function parkedSessionDeps(): AppInitDeps {
			repositoriesStore.add({ path: "/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.setActiveWorkspace("/repo", "main");
			repositoriesStore.setActive("/repo");
			return createMockDeps({
				pty: {
					listActiveSessions: vi.fn().mockResolvedValue([{ session_id: "sess-parked", cwd: PARKED_CWD }]),
					close: vi.fn().mockResolvedValue(undefined),
				},
			});
		}

		function parkedToast() {
			return toastsStore.toasts.find((toast) => toast.title === "Tab parked outside your repos");
		}

		it("does not offer registration for a newly discovered worktree of a registered repo", async () => {
			const repoPath = "/gits/ls/gate-os";
			repositoriesStore.add({ path: repoPath, displayName: "gate-os" });
			repositoriesStore.setWorkspace(repoPath, "main", { worktreePath: repoPath });
			repositoriesStore.setActiveWorkspace(repoPath, "main");
			const deps = createMockDeps({
				pty: {
					listActiveSessions: vi.fn().mockResolvedValue([{ session_id: "new-worktree", cwd: PARKED_CWD }]),
					close: vi.fn().mockResolvedValue(undefined),
				},
				refreshAllBranchStats: vi.fn().mockImplementation(async () => {
					repositoriesStore.setWorkspace(repoPath, "poc-0001-blade", { worktreePath: PARKED_CWD });
				}),
			});

			await initApp(deps);

			expect(parkedToast()).toBeUndefined();
			expect(deps.registerRepo).not.toHaveBeenCalled();
		});

		it("homes a new-worktree session under its registered repo after refresh", async () => {
			const repoPath = "/gits/ls/gate-os";
			repositoriesStore.add({ path: repoPath, displayName: "gate-os" });
			repositoriesStore.setWorkspace(repoPath, "main", { worktreePath: repoPath });
			repositoriesStore.setActiveWorkspace(repoPath, "main");
			const deps = createMockDeps({
				pty: {
					listActiveSessions: vi.fn().mockResolvedValue([{ session_id: "new-worktree", cwd: PARKED_CWD }]),
					close: vi.fn().mockResolvedValue(undefined),
				},
				refreshAllBranchStats: vi.fn().mockImplementation(async () => {
					repositoriesStore.setWorkspace(repoPath, "poc-0001-blade", { worktreePath: PARKED_CWD });
				}),
			});

			await initApp(deps);
			await vi.advanceTimersByTimeAsync(0);

			const workspace = repositoriesStore.get(repoPath)?.workspaces["poc-0001-blade"];
			const terminalId = terminalsStore.getTerminalForSession("new-worktree");
			expect(workspace?.terminals).toContain(terminalId);
			expect(terminalsStore.get(terminalId!)?.repoPath).toBe(repoPath);
			expect(globalWorkspaceStore.getScopeMembers(MANUAL_SCOPE)).not.toContain(terminalId);
			expect(deps.refreshAllBranchStats).toHaveBeenCalledWith(repoPath);
		});

		it("homes a live session-created tab when its registered repo learns the worktree", async () => {
			const repoPath = "/gits/ls/gate-os";
			repositoriesStore.add({ path: repoPath, displayName: "gate-os" });
			repositoriesStore.setWorkspace(repoPath, "main", { worktreePath: repoPath });
			repositoriesStore.setActiveWorkspace(repoPath, "main");
			let onSessionCreated: ((event: { payload: { session_id: string; cwd: string } }) => void) | undefined;
			vi.mocked(listen).mockImplementation(((event: string, handler: typeof onSessionCreated) => {
				if (event === "session-created") onSessionCreated = handler;
				return Promise.resolve(vi.fn());
			}) as unknown as typeof listen);
			const deps = createMockDeps({
				refreshAllBranchStats: vi.fn().mockImplementation(async (path?: string) => {
					if (path === repoPath) {
						repositoriesStore.setWorkspace(repoPath, "poc-0001-blade", { worktreePath: PARKED_CWD });
					}
				}),
			});
			await initApp(deps);

			onSessionCreated!({ payload: { session_id: "live-worktree", cwd: PARKED_CWD } });
			await vi.advanceTimersByTimeAsync(0);

			const terminalId = terminalsStore.getTerminalForSession("live-worktree");
			expect(repositoriesStore.get(repoPath)?.workspaces["poc-0001-blade"].terminals).toContain(terminalId);
			expect(globalWorkspaceStore.getScopeMembers(MANUAL_SCOPE)).not.toContain(terminalId);
			expect(parkedToast()).toBeUndefined();
		});

		it("does not offer registration when Windows separators differ from the stored repo", async () => {
			const repoPath = "C:\\Gits\\gate-os";
			const cwd = "C:\\Gits\\gate-os__wt\\feature";
			repositoriesStore.add({ path: repoPath, displayName: "gate-os" });
			repositoriesStore.setWorkspace(repoPath, "main", { worktreePath: repoPath });
			repositoriesStore.setActiveWorkspace(repoPath, "main");
			const deps = createMockDeps({
				pty: {
					listActiveSessions: vi.fn().mockResolvedValue([{ session_id: "windows-worktree", cwd }]),
					close: vi.fn().mockResolvedValue(undefined),
				},
				refreshAllBranchStats: vi.fn().mockImplementation(async (path?: string) => {
					if (path === repoPath) repositoriesStore.setWorkspace(repoPath, "feature", { worktreePath: cwd });
				}),
			});

			await initApp(deps);
			await vi.advanceTimersByTimeAsync(0);

			const terminalId = terminalsStore.getTerminalForSession("windows-worktree");
			expect(parkedToast()).toBeUndefined();
			expect(repositoriesStore.get(repoPath)?.workspaces.feature.terminals).toContain(terminalId);
		});

		it("keeps the tab parked without a Register toast if worktree refresh fails", async () => {
			const repoPath = "/gits/ls/gate-os";
			repositoriesStore.add({ path: repoPath, displayName: "gate-os" });
			repositoriesStore.setWorkspace(repoPath, "main", { worktreePath: repoPath });
			repositoriesStore.setActiveWorkspace(repoPath, "main");
			const deps = createMockDeps({
				pty: {
					listActiveSessions: vi.fn().mockResolvedValue([{ session_id: "refresh-failed", cwd: PARKED_CWD }]),
					close: vi.fn().mockResolvedValue(undefined),
				},
				refreshAllBranchStats: vi.fn().mockImplementation(async (path?: string) => {
					if (path === repoPath) throw new Error("repository scan unavailable");
				}),
			});

			await initApp(deps);
			await vi.advanceTimersByTimeAsync(0);

			const terminalId = terminalsStore.getTerminalForSession("refresh-failed");
			expect(globalWorkspaceStore.getScopeMembers(MANUAL_SCOPE)).toContain(terminalId);
			expect(parkedToast()).toBeUndefined();
		});

		it("offers a register action naming the deduced repo root", async () => {
			const deps = parkedSessionDeps();
			await initApp(deps);

			const toast = parkedToast();
			expect(toast).toBeDefined();
			expect(toast!.message).toContain(DEDUCED_ROOT);
			expect(toast!.action?.label).toBe("Register");
		});

		// The whole reason auto-registration was rejected: addRepoByPath calls
		// setActive(), so registering from a background reconnect would yank the
		// focused repo out from under the user. Adoption must stay inert.
		it("registers nothing while the action is not clicked", async () => {
			const deps = parkedSessionDeps();
			await initApp(deps);

			expect(parkedToast()).toBeDefined();
			expect(deps.registerRepo).not.toHaveBeenCalled();
			expect(repositoriesStore.getPaths()).not.toContain(DEDUCED_ROOT);
			// The focused repo did not lend it a slot, and the focus is untouched.
			expect(repositoriesStore.get("/repo")?.workspaces["main"].terminals).toHaveLength(0);
			expect(repositoriesStore.state.activeRepoPath).toBe("/repo");
		});

		it("registers the deduced root when the action is clicked", async () => {
			const deps = parkedSessionDeps();
			await initApp(deps);

			parkedToast()!.action!.onClick();

			expect(deps.registerRepo).toHaveBeenCalledTimes(1);
			expect(deps.registerRepo).toHaveBeenCalledWith(DEDUCED_ROOT);
		});

		it("survives a failing registration without an unhandled rejection", async () => {
			const deps = parkedSessionDeps();
			deps.registerRepo = vi.fn().mockRejectedValue(new Error("not a directory"));
			await initApp(deps);

			expect(() => parkedToast()!.action!.onClick()).not.toThrow();
			await vi.advanceTimersByTimeAsync(0);
			expect(deps.registerRepo).toHaveBeenCalledWith(DEDUCED_ROOT);
		});
	});

	it("snapshots agentSessionId into savedTerminals on beforeunload", async () => {
		repositoriesStore.add({ path: "/repo", displayName: "Repo" });
		repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });

		// Surviving session so initApp re-adopts and assigns to branch
		const deps = createMockDeps({
			pty: {
				listActiveSessions: vi.fn().mockResolvedValue([{ session_id: "sess-1", cwd: "/repo" }]),
				close: vi.fn().mockResolvedValue(undefined),
			},
		});

		await initApp(deps);

		// Set agentSessionId on the re-adopted terminal
		const termId = terminalsStore.getIds()[0];
		terminalsStore.update(termId, { agentSessionId: "abc-123-uuid" });

		// Trigger beforeunload to snapshot
		window.dispatchEvent(new Event("beforeunload"));

		const branch = repositoriesStore.get("/repo")?.workspaces["main"];
		expect(branch?.savedTerminals?.length).toBe(1);
		expect(branch?.savedTerminals?.[0].agentSessionId).toBe("abc-123-uuid");
	});

	// The flag is what keeps a suspended tab suspended across a restart; a snapshot that
	// dropped it would restore the tab as an ordinary one (or not at all for a plain shell).
	it("snapshots the suspended flag into savedTerminals on beforeunload", async () => {
		repositoriesStore.add({ path: "/repo", displayName: "Repo" });
		repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });

		const deps = createMockDeps({
			pty: {
				listActiveSessions: vi.fn().mockResolvedValue([{ session_id: "sess-s", cwd: "/repo" }]),
				close: vi.fn().mockResolvedValue(undefined),
			},
		});

		await initApp(deps);

		terminalsStore.update(terminalsStore.getIds()[0], { suspended: true });
		window.dispatchEvent(new Event("beforeunload"));

		const saved = repositoriesStore.get("/repo")?.workspaces["main"]?.savedTerminals?.[0];
		expect(saved?.suspended).toBe(true);
	});

	it("snapshots null agentSessionId for terminals without it", async () => {
		repositoriesStore.add({ path: "/repo", displayName: "Repo" });
		repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });

		const deps = createMockDeps({
			pty: {
				listActiveSessions: vi.fn().mockResolvedValue([{ session_id: "sess-2", cwd: "/repo" }]),
				close: vi.fn().mockResolvedValue(undefined),
			},
		});

		await initApp(deps);

		window.dispatchEvent(new Event("beforeunload"));

		const branch = repositoriesStore.get("/repo")?.workspaces["main"];
		expect(branch?.savedTerminals?.length).toBe(1);
		expect(branch?.savedTerminals?.[0].agentSessionId).toBeNull();
	});

	it("snapshots the intent and a truncated prompt so a restored tab can say what it was doing", async () => {
		repositoriesStore.add({ path: "/repo", displayName: "Repo" });
		repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });

		const deps = createMockDeps({
			pty: {
				listActiveSessions: vi.fn().mockResolvedValue([{ session_id: "sess-3", cwd: "/repo" }]),
				close: vi.fn().mockResolvedValue(undefined),
			},
		});

		await initApp(deps);

		const termId = terminalsStore.getIds()[0];
		terminalsStore.update(termId, {
			agentIntent: "finishing the resume banner",
			lastPrompt: "x".repeat(500),
		});

		window.dispatchEvent(new Event("beforeunload"));

		const saved = repositoriesStore.get("/repo")?.workspaces["main"]?.savedTerminals?.[0];
		expect(saved?.agentIntent).toBe("finishing the resume banner");
		expect(saved?.lastPrompt).toBe("x".repeat(300));
	});

	it("registers beforeunload handler to close PTY sessions", async () => {
		const addListenerSpy = vi.spyOn(window, "addEventListener");
		const deps = createMockDeps();
		await initApp(deps);

		expect(addListenerSpy).toHaveBeenCalledWith("beforeunload", expect.any(Function));
		addListenerSpy.mockRestore();
	});

	it("refreshes dictation config", async () => {
		const deps = createMockDeps();
		await initApp(deps);

		expect(deps.stores.refreshDictationConfig).toHaveBeenCalled();
	});

	// Catches: the desktop rendering its own policy instead of Rust's close decision.
	it("renders the Rust close request without recomputing terminal counts", async () => {
		let handler: ((event: { payload: { kind: "ask"; sessions: number } }) => void) | undefined;
		vi.mocked(listen).mockImplementation(((event: string, callback: typeof handler) => {
			if (event === "close-dialog") handler = callback;
			return Promise.resolve(vi.fn());
		}) as unknown as typeof listen);
		await initApp(createMockDeps());
		handler!({ payload: { kind: "ask", sessions: 17 } });
		expect(closeDialogStore.state.request).toEqual({ kind: "ask", sessions: 17 });
	});

	it("beforeunload closes browser-created PTY sessions", async () => {
		const closeSpy = vi.fn().mockResolvedValue(undefined);
		const deps = createMockDeps({
			pty: {
				listActiveSessions: vi.fn().mockResolvedValue([{ session_id: "sess-1", cwd: "/tmp" }]),
				close: closeSpy,
			},
		});

		await initApp(deps);

		// Register sess-1 as browser-created (beforeunload only closes these)
		browserCreatedSessions.add("sess-1");

		// Temporarily disable Tauri flag — beforeunload only closes in browser mode
		const saved = (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;
		delete (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;

		window.dispatchEvent(new Event("beforeunload"));
		expect(closeSpy).toHaveBeenCalledWith("sess-1");

		(globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = saved;
		browserCreatedSessions.delete("sess-1");
	});

	it("removes splash screen after hydration", async () => {
		const splash = document.createElement("div");
		splash.id = "splash";
		document.body.appendChild(splash);

		const deps = createMockDeps();
		await initApp(deps);

		expect(document.getElementById("splash")).toBeNull();
	});

	it("removes splash screen even when hydration fails", async () => {
		const splash = document.createElement("div");
		splash.id = "splash";
		document.body.appendChild(splash);

		const deps = createMockDeps({
			stores: {
				hydrate: vi.fn().mockRejectedValue(new Error("hydration failed")),
				startPolling: vi.fn(),
				stopPolling: vi.fn(),
				startAutoFetch: vi.fn(),
				startPrNotificationTimer: vi.fn(),
				loadFontFromConfig: vi.fn(),
				startDictationSettingsSync: vi.fn(() => vi.fn()),
				refreshDictationConfig: vi.fn().mockResolvedValue(undefined),
				startUserActivityListening: vi.fn(),
			},
		});

		await initApp(deps);

		expect(document.getElementById("splash")).toBeNull();
	});

	it("clears stale terminals from previous session", async () => {
		// Pre-populate stale terminals
		terminalsStore.add(makeTerminal({ name: "stale", cwd: "/old" }));
		expect(terminalsStore.getCount()).toBe(1);

		const deps = createMockDeps();
		await initApp(deps);

		// Stale terminal should be removed, and a new fallback terminal created
		const ids = terminalsStore.getIds();
		for (const id of ids) {
			expect(terminalsStore.get(id)?.name).not.toBe("stale");
		}
	});

	it("repo-changed event triggers debounced refreshAllBranchStats", async () => {
		// Capture the "repo-changed" listener callback
		const listenMock = vi.mocked(listen);
		let repoChangedCallback: ((event: { payload: { repo_path: string } }) => void) | null = null;
		listenMock.mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
			if (event === "repo-changed") {
				repoChangedCallback = handler as typeof repoChangedCallback;
			}
			return Promise.resolve(vi.fn());
		}) as unknown as typeof listen);

		const deps = createMockDeps();
		await initApp(deps);

		// refreshAllBranchStats is called once during init
		expect(deps.refreshAllBranchStats).toHaveBeenCalledTimes(1);

		// Simulate repo-changed event
		expect(repoChangedCallback).not.toBeNull();
		repoChangedCallback!({ payload: { repo_path: "/repo" } });

		// Should not fire immediately (debounced)
		expect(deps.refreshAllBranchStats).toHaveBeenCalledTimes(1);

		// After debounce period (500ms), should fire
		await vi.advanceTimersByTimeAsync(500);
		expect(deps.refreshAllBranchStats).toHaveBeenCalledTimes(2);
	});

	it("repo-changed scopes the refresh to the repo that changed (no full fan-out)", async () => {
		// Regression: a change to ONE repo must not re-scan every open repo. The
		// debounced refresh is called with the changed repo's path so the fan-out
		// stays bounded to that repo.
		const listenMock = vi.mocked(listen);
		let repoChangedCallback: ((event: { payload: { repo_path: string } }) => void) | null = null;
		listenMock.mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
			if (event === "repo-changed") {
				repoChangedCallback = handler as typeof repoChangedCallback;
			}
			return Promise.resolve(vi.fn());
		}) as unknown as typeof listen);

		const deps = createMockDeps();
		await initApp(deps);
		vi.mocked(deps.refreshAllBranchStats).mockClear();

		repoChangedCallback!({ payload: { repo_path: "/repo-a" } });
		await vi.advanceTimersByTimeAsync(500);

		expect(deps.refreshAllBranchStats).toHaveBeenCalledTimes(1);
		expect(deps.refreshAllBranchStats).toHaveBeenCalledWith("/repo-a");
	});

	it("repo-changed debounces each repo independently — one change never delays another", async () => {
		// Two different repos changing within the same window each get their own
		// scoped refresh; they are NOT coalesced into a single all-repos scan.
		const listenMock = vi.mocked(listen);
		let repoChangedCallback: ((event: { payload: { repo_path: string } }) => void) | null = null;
		listenMock.mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
			if (event === "repo-changed") {
				repoChangedCallback = handler as typeof repoChangedCallback;
			}
			return Promise.resolve(vi.fn());
		}) as unknown as typeof listen);

		const deps = createMockDeps();
		await initApp(deps);
		vi.mocked(deps.refreshAllBranchStats).mockClear();

		repoChangedCallback!({ payload: { repo_path: "/repo-a" } });
		repoChangedCallback!({ payload: { repo_path: "/repo-b" } });
		await vi.advanceTimersByTimeAsync(500);

		expect(deps.refreshAllBranchStats).toHaveBeenCalledTimes(2);
		expect(deps.refreshAllBranchStats).toHaveBeenCalledWith("/repo-a");
		expect(deps.refreshAllBranchStats).toHaveBeenCalledWith("/repo-b");
	});

	it("repo-changed debounce coalesces rapid events", async () => {
		const listenMock = vi.mocked(listen);
		let repoChangedCallback: ((event: { payload: { repo_path: string } }) => void) | null = null;
		listenMock.mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
			if (event === "repo-changed") {
				repoChangedCallback = handler as typeof repoChangedCallback;
			}
			return Promise.resolve(vi.fn());
		}) as unknown as typeof listen);

		const deps = createMockDeps();
		await initApp(deps);

		// Fire 5 rapid events
		for (let i = 0; i < 5; i++) {
			repoChangedCallback!({ payload: { repo_path: "/repo" } });
		}

		// After debounce (500ms), should only have called refreshAllBranchStats once more (not 5 times)
		await vi.advanceTimersByTimeAsync(500);
		expect(deps.refreshAllBranchStats).toHaveBeenCalledTimes(2); // 1 init + 1 debounced
	});

	it("repo-changed coalesces same-frame bumps to one per repo, but never loses them across frames", async () => {
		// Regression (story 1277-31a0): bumpRevision must NOT be lost when rapid
		// repo-changed events arrive. It used to live inside the branchStatsTimer
		// setTimeout, so a second event's clearTimeout dropped the first bump.
		// It is now per-frame coalesced: a same-frame burst collapses to ONE bump
		// per repo (avoiding redundant ~20-effect flushes) while still always
		// delivering — separate frames each bump, so no update is lost.

		const listenMock = vi.mocked(listen);
		let repoChangedCallback: ((event: { payload: { repo_path: string } }) => void) | null = null;
		listenMock.mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
			if (event === "repo-changed") {
				repoChangedCallback = handler as typeof repoChangedCallback;
			}
			return Promise.resolve(vi.fn());
		}) as unknown as typeof listen);

		repositoriesStore.add({ path: "/repo", displayName: "Repo" });

		const deps = createMockDeps();
		await initApp(deps);

		// Capture baseline AFTER initApp — setActive() may bump revision for non-hot repos.
		const before = repositoriesStore.getRevision("/repo");

		// Two same-frame events collapse to a single bump after the frame flushes.
		repoChangedCallback!({ payload: { repo_path: "/repo" } });
		repoChangedCallback!({ payload: { repo_path: "/repo" } });
		expect(repositoriesStore.getRevision("/repo")).toBe(before); // not yet delivered
		await vi.advanceTimersByTimeAsync(20); // flush the animation frame
		expect(repositoriesStore.getRevision("/repo")).toBe(before + 1); // coalesced to one

		// A later event in a separate frame still bumps — bumps are never lost.
		repoChangedCallback!({ payload: { repo_path: "/repo" } });
		await vi.advanceTimersByTimeAsync(20);
		expect(repositoriesStore.getRevision("/repo")).toBe(before + 2);

		// And the branch-stats refresh stays debounced.
		await vi.advanceTimersByTimeAsync(500);
		expect(deps.refreshAllBranchStats).toHaveBeenCalledTimes(2);
	});

	describe("scoped cache invalidation", () => {
		function captureRepoAndHeadChanged() {
			const listenMock = vi.mocked(listen);
			let repoChangedCb: ((event: { payload: { repo_path: string; kind: string } }) => void) | null = null;
			let headChangedCb: ((event: { payload: { repo_path: string; branch: string } }) => void) | null = null;
			listenMock.mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
				if (event === "repo-changed") repoChangedCb = handler as typeof repoChangedCb;
				if (event === "head-changed") headChangedCb = handler as typeof headChangedCb;
				return Promise.resolve(vi.fn());
			}) as unknown as typeof listen);
			return {
				getRepoChanged: () => repoChangedCb,
				getHeadChanged: () => headChangedCb,
			};
		}

		beforeEach(() => {
			mockInvoke.mockClear();
		});

		// The backend already invalidated. Every producer of `repo-changed`
		// (the watcher's git-state and working-tree emits, and worktree
		// creation) calls `invalidate_repo_caches` before it sends the event,
		// and `clear_repo_caches` does nothing else — so this round trip could
		// only ever re-clear caches that were already empty, once per repo per
		// event, on the IPC thread.
		it("repo-changed does not re-invalidate caches the backend already cleared", async () => {
			const { getRepoChanged } = captureRepoAndHeadChanged();
			const deps = createMockDeps();
			await initApp(deps);

			mockInvoke.mockClear();
			getRepoChanged()!({ payload: { repo_path: "/my/repo", kind: "git-state" } });

			expect(mockInvoke).not.toHaveBeenCalledWith("clear_repo_caches", { path: "/my/repo" });
			expect(mockInvoke).not.toHaveBeenCalledWith("clear_caches");
		});

		it("head-changed calls clear_repo_caches with repo path, not clear_caches", async () => {
			const { getHeadChanged } = captureRepoAndHeadChanged();
			repositoriesStore.add({ path: "/my/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/my/repo", "main", { worktreePath: null });
			repositoriesStore.setActiveWorkspace("/my/repo", "main");

			const deps = createMockDeps();
			await initApp(deps);

			mockInvoke.mockClear();
			getHeadChanged()!({ payload: { repo_path: "/my/repo", branch: "feature" } });

			expect(mockInvoke).toHaveBeenCalledWith("clear_repo_caches", { path: "/my/repo" });
			expect(mockInvoke).not.toHaveBeenCalledWith("clear_caches");
		});

		// head-changed keeps its call: `resolve_head_target` short-circuits the
		// watcher's git-state emit when only HEAD moved, so nothing else
		// invalidated for a plain branch switch.
		it("repo-changed leaves the head-changed invalidation untouched", async () => {
			const { getRepoChanged, getHeadChanged } = captureRepoAndHeadChanged();
			repositoriesStore.add({ path: "/my/repo", displayName: "Repo" });
			repositoriesStore.setWorkspace("/my/repo", "main", { worktreePath: null });
			repositoriesStore.setActiveWorkspace("/my/repo", "main");
			const deps = createMockDeps();
			await initApp(deps);

			mockInvoke.mockClear();
			getRepoChanged()!({ payload: { repo_path: "/my/repo", kind: "git-state" } });
			expect(mockInvoke).not.toHaveBeenCalledWith("clear_repo_caches", { path: "/my/repo" });

			getHeadChanged()!({ payload: { repo_path: "/my/repo", branch: "feature" } });
			expect(mockInvoke).toHaveBeenCalledWith("clear_repo_caches", { path: "/my/repo" });
		});
	});

	// The narrowing is only safe because it is a strict subset: `getRevision`
	// still moves on every event, so a panel that was never migrated cannot go
	// stale. Only `getGitRevision` is held back on a working-tree change.
	describe("repo-changed change kind", () => {
		function captureRepoChanged() {
			const listenMock = vi.mocked(listen);
			let cb: ((event: { payload: { repo_path: string; kind: string } }) => void) | null = null;
			listenMock.mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
				if (event === "repo-changed") cb = handler as typeof cb;
				return Promise.resolve(vi.fn());
			}) as unknown as typeof listen);
			return () => cb;
		}

		it("a working-tree change bumps the general revision but not the git one", async () => {
			const getCb = captureRepoChanged();
			const deps = createMockDeps();
			await initApp(deps);

			const revision = repositoriesStore.getRevision("/repo");
			const gitRevision = repositoriesStore.getGitRevision("/repo");
			getCb()!({ payload: { repo_path: "/repo", kind: "working-tree" } });
			await vi.advanceTimersByTimeAsync(20);

			expect(repositoriesStore.getRevision("/repo")).toBe(revision + 1);
			expect(repositoriesStore.getGitRevision("/repo")).toBe(gitRevision);
		});

		it("a git-state change bumps both", async () => {
			const getCb = captureRepoChanged();
			const deps = createMockDeps();
			await initApp(deps);

			const revision = repositoriesStore.getRevision("/repo");
			const gitRevision = repositoriesStore.getGitRevision("/repo");
			getCb()!({ payload: { repo_path: "/repo", kind: "git-state" } });
			await vi.advanceTimersByTimeAsync(20);

			expect(repositoriesStore.getRevision("/repo")).toBe(revision + 1);
			expect(repositoriesStore.getGitRevision("/repo")).toBe(gitRevision + 1);
		});
	});

	describe("head-changed event", () => {
		function captureHeadChanged() {
			const listenMock = vi.mocked(listen);
			let headChangedCallback: ((event: { payload: { repo_path: string; branch: string } }) => void) | null = null;
			listenMock.mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
				if (event === "head-changed") {
					headChangedCallback = handler as typeof headChangedCallback;
				}
				return Promise.resolve(vi.fn());
			}) as unknown as typeof listen);
			return { listenMock, getCallback: () => headChangedCallback };
		}

		it("renames branch entry when old branch is main checkout (worktreePath null)", async () => {
			const { getCallback } = captureHeadChanged();
			const deps = createMockDeps();
			repositoriesStore.add({ path: "/repo", displayName: "repo" });
			repositoriesStore.setWorkspace("/repo", "develop", { worktreePath: null });
			repositoriesStore.setActiveWorkspace("/repo", "develop");
			repositoriesStore.addTerminalToWorkspace("/repo", "develop", "term-1");

			await initApp(deps);

			getCallback()!({ payload: { repo_path: "/repo", branch: "ACME-00106/feature" } });

			// Old branch gone, new branch has it
			expect(repositoriesStore.get("/repo")?.workspaces["develop"]).toBeUndefined();
			expect(repositoriesStore.get("/repo")?.workspaces["ACME-00106/feature"]).toBeDefined();
			// Terminals carry over
			expect(repositoriesStore.get("/repo")?.workspaces["ACME-00106/feature"]?.terminals).toContain("term-1");
			// Active branch updated
			expect(repositoriesStore.get("/repo")?.activeWorkspaceId).toBe("ACME-00106/feature");
		});

		it("creates new branch entry when old branch is a worktree (worktreePath set)", async () => {
			const { getCallback } = captureHeadChanged();
			const deps = createMockDeps();
			repositoriesStore.add({ path: "/repo", displayName: "repo" });
			repositoriesStore.setWorkspace("/repo", "wt-branch", { worktreePath: "/repo/.worktrees/wt-branch" });
			repositoriesStore.setActiveWorkspace("/repo", "wt-branch");

			await initApp(deps);

			getCallback()!({ payload: { repo_path: "/repo", branch: "new-branch" } });

			// Old worktree branch preserved
			expect(repositoriesStore.get("/repo")?.workspaces["wt-branch"]).toBeDefined();
			// New branch created
			expect(repositoriesStore.get("/repo")?.workspaces["new-branch"]).toBeDefined();
			expect(repositoriesStore.get("/repo")?.activeWorkspaceId).toBe("new-branch");
		});

		it("sets activeBranch when target branch already exists in store", async () => {
			const { getCallback } = captureHeadChanged();
			const deps = createMockDeps();
			repositoriesStore.add({ path: "/repo", displayName: "repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: null });
			repositoriesStore.setWorkspace("/repo", "feature", { worktreePath: null });
			repositoriesStore.setActiveWorkspace("/repo", "feature");

			await initApp(deps);

			getCallback()!({ payload: { repo_path: "/repo", branch: "main" } });

			// Both branches still exist (feature kept, main was pre-existing)
			expect(repositoriesStore.get("/repo")?.workspaces["main"]).toBeDefined();
			expect(repositoriesStore.get("/repo")?.activeWorkspaceId).toBe("main");
		});

		it("does nothing when branch has not changed", async () => {
			const { getCallback } = captureHeadChanged();
			const deps = createMockDeps();
			repositoriesStore.add({ path: "/repo", displayName: "repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: null });
			repositoriesStore.setActiveWorkspace("/repo", "main");

			await initApp(deps);

			getCallback()!({ payload: { repo_path: "/repo", branch: "main" } });

			// Store unchanged
			expect(Object.keys(repositoriesStore.get("/repo")?.workspaces ?? {})).toEqual(["main"]);
			expect(repositoriesStore.get("/repo")?.activeWorkspaceId).toBe("main");
		});

		it("moves terminals when new branch already exists in store (race with refreshAllBranchStats)", async () => {
			const { getCallback } = captureHeadChanged();
			const deps = createMockDeps();
			repositoriesStore.add({ path: "/repo", displayName: "repo" });
			repositoriesStore.setWorkspace("/repo", "wip/global-config", { worktreePath: null });
			repositoriesStore.setActiveWorkspace("/repo", "wip/global-config");
			repositoriesStore.addTerminalToWorkspace("/repo", "wip/global-config", "term-1");
			repositoriesStore.addTerminalToWorkspace("/repo", "wip/global-config", "term-2");

			// Simulate refreshAllBranchStats creating the new branch before head-changed fires
			repositoriesStore.setWorkspace("/repo", "wip/memory-system-improvements", { worktreePath: null });

			await initApp(deps);

			getCallback()!({ payload: { repo_path: "/repo", branch: "wip/memory-system-improvements" } });

			// Terminals moved to new branch
			expect(repositoriesStore.get("/repo")?.workspaces["wip/memory-system-improvements"]?.terminals).toContain(
				"term-1",
			);
			expect(repositoriesStore.get("/repo")?.workspaces["wip/memory-system-improvements"]?.terminals).toContain(
				"term-2",
			);
			// Old branch entry removed after merge
			expect(repositoriesStore.get("/repo")?.workspaces["wip/global-config"]).toBeUndefined();
			// Active branch updated
			expect(repositoriesStore.get("/repo")?.activeWorkspaceId).toBe("wip/memory-system-improvements");
		});

		it("renames branch entry when old branch is main worktree (worktreePath === repoPath)", async () => {
			const { getCallback } = captureHeadChanged();
			const deps = createMockDeps();
			repositoriesStore.add({ path: "/repo", displayName: "repo" });
			repositoriesStore.setWorkspace("/repo", "main", { worktreePath: "/repo" });
			repositoriesStore.setActiveWorkspace("/repo", "main");
			repositoriesStore.addTerminalToWorkspace("/repo", "main", "term-1");

			await initApp(deps);

			getCallback()!({ payload: { repo_path: "/repo", branch: "feat/incremental-reindex" } });

			// Old branch gone — renamed, not duplicated
			expect(repositoriesStore.get("/repo")?.workspaces["main"]).toBeUndefined();
			// New branch exists with terminals carried over
			expect(repositoriesStore.get("/repo")?.workspaces["feat/incremental-reindex"]).toBeDefined();
			expect(repositoriesStore.get("/repo")?.workspaces["feat/incremental-reindex"]?.terminals).toContain("term-1");
			// Active branch updated
			expect(repositoriesStore.get("/repo")?.activeWorkspaceId).toBe("feat/incremental-reindex");
			// Should NOT create a phantom entry — only one branch in sidebar
			expect(Object.keys(repositoriesStore.get("/repo")?.workspaces ?? {})).toEqual(["feat/incremental-reindex"]);
		});

		it("does nothing when repo is not found", async () => {
			const { getCallback } = captureHeadChanged();
			const deps = createMockDeps();

			await initApp(deps);

			// Should not throw
			expect(() => getCallback()!({ payload: { repo_path: "/unknown-repo", branch: "main" } })).not.toThrow();
		});
	});

	describe("session-closed event (shellState exited)", () => {
		type SessionClosedPayload = { session_id: string; reason: string; agent_type?: string | null };

		function captureSessionClosed() {
			const listenMock = vi.mocked(listen);
			let callback: ((event: { payload: SessionClosedPayload }) => void) | null = null;
			listenMock.mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
				if (event === "session-closed") {
					callback = handler as typeof callback;
				}
				return Promise.resolve(vi.fn());
			}) as unknown as typeof listen);
			return { getCallback: () => callback };
		}

		it("sets shellState to exited on the terminal when a remote session closes", async () => {
			const { getCallback } = captureSessionClosed();
			const deps = createMockDeps();
			await initApp(deps);
			const playCompletion = vi.spyOn(notificationsStore, "playCompletion").mockResolvedValue(undefined);
			notificationsStore.setSilenceRemoteCompletions(true);

			const termId = terminalsStore.add({
				sessionId: "remote-sess",
				fontSize: 14,
				name: "Agent",
				cwd: "/tmp",
				awaitingInput: null,
				isExternallyCreated: true,
			});
			terminalsStore.update(termId, { agentState: "working", backgroundWork: true });

			getCallback()!({ payload: { session_id: "remote-sess", reason: "process_exit", agent_type: "claude" } });

			expect(terminalsStore.get(termId)?.shellState).toBe("exited");
			expect(terminalsStore.get(termId)?.sessionId).toBeNull();
			expect(terminalsStore.get(termId)?.agentState).toBeNull();
			expect(terminalsStore.get(termId)?.backgroundWork).toBe(false);
			expect(terminalsStore.get(termId)?.completionNotified).toBe(true);
			expect(playCompletion).not.toHaveBeenCalled();

			notificationsStore.setSilenceRemoteCompletions(false);
			playCompletion.mockRestore();
		});

		it("does not set shellState when session_id has no matching terminal", async () => {
			const { getCallback } = captureSessionClosed();
			const deps = createMockDeps();
			await initApp(deps);

			// No terminal registered for this session — should not throw
			expect(() => getCallback()!({ payload: { session_id: "unknown-sess", reason: "process_exit" } })).not.toThrow();
		});
	});

	describe("session-closed auto-close path", () => {
		type SessionCreatedPayload = {
			session_id: string;
			cwd: string | null;
			agent_type?: string | null;
			display_name?: string | null;
			parent_session?: string | null;
		};
		type SessionClosedPayload = { session_id: string; reason: string; agent_type?: string | null };

		/** Captures both session-created and session-closed callbacks in a single mock pass. */
		function captureCreatedAndClosed() {
			const listenMock = vi.mocked(listen);
			let createdCb: ((event: { payload: SessionCreatedPayload }) => void) | null = null;
			let closedCb: ((event: { payload: SessionClosedPayload }) => void) | null = null;
			listenMock.mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
				if (event === "session-created") createdCb = handler as typeof createdCb;
				if (event === "session-closed") closedCb = handler as typeof closedCb;
				return Promise.resolve(vi.fn());
			}) as unknown as typeof listen);
			return {
				getCreated: () => createdCb,
				getClosed: () => closedCb,
			};
		}

		it("auto-removes an agent tab after AGENT_TAB_AUTOCLOSE_MS when agent_type is set", async () => {
			const { getCreated, getClosed } = captureCreatedAndClosed();
			const deps = createMockDeps();
			await initApp(deps);

			// Register the remote tab via session-created so remoteSessionTabs is populated
			getCreated()!({ payload: { session_id: "agent-sess", cwd: null, agent_type: "claude" } });
			const termId = terminalsStore.getIds().find((id) => terminalsStore.get(id)?.sessionId === "agent-sess")!;
			expect(termId).toBeDefined();

			// Fire session-closed with agent_type — triggers AGENT_TAB_AUTOCLOSE_MS (10 000ms)
			getClosed()!({ payload: { session_id: "agent-sess", reason: "process_exit", agent_type: "claude" } });

			// Tab still present before timeout
			expect(terminalsStore.get(termId)).toBeDefined();

			// Advance past the 10s agent autoclose
			vi.advanceTimersByTime(10_001);

			// Tab must be gone
			expect(terminalsStore.get(termId)).toBeUndefined();
		});

		// Suspend closes the PTY, so the backend reports session-closed; for a tab opened over
		// HTTP/MCP that started the countdown and deleted the tab the user had just parked.
		it("keeps a suspended remote tab when its session closes", async () => {
			const { getCreated, getClosed } = captureCreatedAndClosed();
			const deps = createMockDeps();
			await initApp(deps);

			getCreated()!({ payload: { session_id: "parked-sess", cwd: null, agent_type: "claude" } });
			const termId = terminalsStore.getIds().find((id) => terminalsStore.get(id)?.sessionId === "parked-sess")!;
			terminalsStore.update(termId, { suspended: true });

			getClosed()!({ payload: { session_id: "parked-sess", reason: "closed", agent_type: "claude" } });
			vi.advanceTimersByTime(60_000);

			expect(terminalsStore.get(termId)).toBeDefined();
			expect(terminalsStore.get(termId)?.name).not.toMatch(/\(\d+s\)/);
		});

		it("records the spawning agent on a sub-agent tab and nothing on a plain one", async () => {
			const { getCreated } = captureCreatedAndClosed();
			const deps = createMockDeps();
			await initApp(deps);

			getCreated()!({ payload: { session_id: "child", cwd: null, agent_type: "claude", parent_session: "tuic-lead" } });
			getCreated()!({ payload: { session_id: "plain", cwd: null, agent_type: "claude" } });
			const byPty = (sid: string) =>
				terminalsStore.get(terminalsStore.getIds().find((id) => terminalsStore.get(id)?.sessionId === sid)!);

			// The sidebar and Activity Dashboard tag a tab only from this field.
			expect(byPty("child")?.parentSession).toBe("tuic-lead");
			expect(byPty("plain")?.parentSession).toBeNull();
		});

		it("auto-removes a remote tab after REMOTE_TAB_AUTOCLOSE_MS when agent_type is absent", async () => {
			const { getCreated, getClosed } = captureCreatedAndClosed();
			const deps = createMockDeps();
			await initApp(deps);

			getCreated()!({ payload: { session_id: "remote-sess-2", cwd: null, agent_type: null } });
			const termId = terminalsStore.getIds().find((id) => terminalsStore.get(id)?.sessionId === "remote-sess-2")!;
			expect(termId).toBeDefined();

			getClosed()!({ payload: { session_id: "remote-sess-2", reason: "process_exit", agent_type: null } });

			// Advancing only 10s must NOT remove the tab (REMOTE uses 30s)
			vi.advanceTimersByTime(10_001);
			expect(terminalsStore.get(termId)).toBeDefined();

			// Advance to just past 30s — tab must be gone
			vi.advanceTimersByTime(20_000);
			expect(terminalsStore.get(termId)).toBeUndefined();
		});
	});

	describe("close-html-tabs event", () => {
		function captureCloseHtmlTabs() {
			const listenMock = vi.mocked(listen);
			let callback: ((event: { payload: { tab_ids: string[] } }) => void) | null = null;
			listenMock.mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
				if (event === "close-html-tabs") callback = handler as typeof callback;
				return Promise.resolve(vi.fn());
			}) as unknown as typeof listen);
			return { getCallback: () => callback };
		}

		it("closes mdTab UI tabs matching the emitted tab_ids", async () => {
			const { getCallback } = captureCloseHtmlTabs();
			const deps = createMockDeps();
			await initApp(deps);

			// Open two plugin tabs in mdTabsStore
			mdTabsStore.openUiTab("plugin-a", "Plugin A", "<p>a</p>", false, undefined, false);
			mdTabsStore.openUiTab("plugin-b", "Plugin B", "<p>b</p>", false, undefined, false);

			const tabsBefore = Object.values(mdTabsStore.state.tabs).filter((t) => t.type === "plugin-panel");
			expect(tabsBefore).toHaveLength(2);

			// Fire close-html-tabs for one of them
			getCallback()!({ payload: { tab_ids: ["plugin-a"] } });

			const remaining = Object.values(mdTabsStore.state.tabs).filter((t) => t.type === "plugin-panel");
			expect(remaining).toHaveLength(1);
			expect(remaining[0].title).toBe("Plugin B");
		});

		it("is a no-op for unknown tab_ids", async () => {
			const { getCallback } = captureCloseHtmlTabs();
			const deps = createMockDeps();
			await initApp(deps);

			mdTabsStore.openUiTab("plugin-c", "Plugin C", "<p>c</p>", false, undefined, false);

			// Should not throw for IDs that don't exist
			expect(() => getCallback()!({ payload: { tab_ids: ["nonexistent-id"] } })).not.toThrow();

			// Existing tab untouched
			const remaining = Object.values(mdTabsStore.state.tabs).filter((t) => t.type === "plugin-panel");
			expect(remaining).toHaveLength(1);
		});
	});

	describe("mcp-toast event (agent-raised attention)", () => {
		type ToastPayload = {
			title: string;
			message: string | null;
			level: string;
			sound: string | null;
			urgency?: string;
			origin_repo_path?: string;
			origin_session_id?: string;
			__tuic_origin?: { connection: string; name?: string };
		};

		function captureMcpToast() {
			const listenMock = vi.mocked(listen);
			let callback: ((event: { payload: ToastPayload }) => void) | null = null;
			listenMock.mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
				if (event === "mcp-toast") {
					callback = handler as typeof callback;
				}
				return Promise.resolve(vi.fn());
			}) as unknown as typeof listen);
			return { getCallback: () => callback };
		}

		// Catches: inverted origin routing shows normal cards on unrelated tabs and hides them on the active origin.
		it.each([
			{ condition: "inactive origin", activeOrigin: false, cards: 0 },
			{ condition: "active origin", activeOrigin: true, cards: 1 },
		])("1760 regression normal toast from $condition yields $cards cards", async ({ activeOrigin, cards }) => {
			const { getCallback } = captureMcpToast();
			await initApp(createMockDeps());
			const origin = terminalsStore.add(makeTerminal({ sessionId: "auto-card-origin" }));
			const other = terminalsStore.add(makeTerminal({ sessionId: "auto-card-other" }));
			terminalsStore.setActive(activeOrigin ? origin : other);
			const focus = vi.spyOn(document, "hasFocus").mockReturnValue(true);
			getCallback()!({
				payload: {
					title: "Background notice",
					message: "Done",
					level: "info",
					sound: null,
					origin_session_id: "auto-card-origin",
				},
			});
			expect(toastsStore.toasts).toHaveLength(cards);
			expect(activityStore.getForSection("messages")).toHaveLength(1);
			focus.mockRestore();
		});

		// Catches: suppressing a subagent's attention sound also discards its urgent card or bell entry.
		it("keeps urgent child cards and bell entries while muting attention", async () => {
			const { getCallback } = captureMcpToast();
			await initApp(createMockDeps());
			terminalsStore.add(
				makeTerminal({ sessionId: "child-pty", tuicSession: "child-peer", parentSession: "parent-peer" }),
			);
			notificationsStore.setSilenceRemoteCompletions(true);
			const audio = vi.spyOn(notificationManager, "play").mockResolvedValue(undefined);
			getCallback()!({
				payload: {
					title: "Child needs a decision",
					message: "Choose a branch",
					level: "warn",
					sound: "attention",
					urgency: "urgent",
					origin_session_id: "child-peer",
				},
			});
			expect(toastsStore.toasts).toHaveLength(1);
			expect(activityStore.getForSection("messages")).toHaveLength(1);
			expect(audio).not.toHaveBeenCalled();
			audio.mockRestore();
		});

		// Catches: automatic routing ignores the opt-out, non-terminal tabs, unknown origins, or urgent bypass.
		it.each([
			{ name: "active unfocused origin", enabled: true, origin: "active", focused: false, urgent: false, cards: 1 },
			{ name: "disabled active origin", enabled: false, origin: "active", focused: true, urgent: false, cards: 0 },
			{ name: "inactive unfocused origin", enabled: true, origin: "other", focused: false, urgent: false, cards: 0 },
			{ name: "urgent inactive origin", enabled: false, origin: "other", focused: true, urgent: true, cards: 1 },
			{ name: "urgent with toggle off", enabled: false, origin: "active", focused: true, urgent: true, cards: 1 },
			{ name: "urgent without origin", enabled: false, origin: undefined, focused: true, urgent: true, cards: 1 },
			{ name: "no origin in background", enabled: true, origin: undefined, focused: false, urgent: false, cards: 0 },
			{ name: "unknown origin", enabled: true, origin: "unknown", focused: true, urgent: false, cards: 0 },
			{
				name: "normal on non-terminal tab",
				enabled: true,
				origin: "active",
				focused: true,
				urgent: false,
				cards: 0,
				nonTerminal: true,
			},
			{
				name: "urgent on non-terminal tab",
				enabled: false,
				origin: "active",
				focused: true,
				urgent: true,
				cards: 1,
				nonTerminal: true,
			},
		])(
			"1760 routes $name without duplicating the bell or stealing focus",
			async ({ enabled, origin, focused, urgent, cards, nonTerminal }) => {
				const { getCallback } = captureMcpToast();
				await initApp(createMockDeps());
				const active = terminalsStore.add(makeTerminal({ sessionId: "active" }));
				terminalsStore.add(makeTerminal({ sessionId: "other" }));
				terminalsStore.setActive(active);
				if (nonTerminal) mdTabsStore.openUiTab("toast-other-tab", "Other", "<p>Other</p>", false);
				const activeBefore = terminalsStore.state.activeId;
				notificationsStore.setAutoToastCards(enabled);
				const focus = vi.spyOn(document, "hasFocus").mockReturnValue(focused);
				const event = {
					payload: {
						title: "Notice",
						message: "Done",
						level: "info",
						sound: null,
						origin_session_id: origin,
						urgency: urgent ? "urgent" : "normal",
					},
				};
				getCallback()!(event);
				getCallback()!(event);
				expect(toastsStore.toasts).toHaveLength(cards);
				expect(activityStore.getForSection("messages")).toHaveLength(1);
				expect(terminalsStore.state.activeId).toBe(activeBefore);
				focus.mockRestore();
			},
		);

		// Catches: malformed remote fields create a bell item/sound or leak their content into logs.
		it.each([
			{ title: null },
			{ title: 42 },
			{ message: 42 },
			{ message: { secret: "do-not-log" } },
			{ message: undefined },
		])("1439 discards malformed remote text fields with a content-free debug log: %j", async (malformed) => {
			const { getCallback } = captureMcpToast();
			await initApp(createMockDeps());
			const debug = vi.spyOn(appLogger, "debug").mockImplementation(() => {});
			const play = vi.spyOn(notificationsStore, "play").mockResolvedValue(undefined);
			const payload = Object.assign(
				{
					title: "Private title",
					message: "Private message",
					level: "warn",
					sound: "attention",
					__tuic_origin: { connection: "mint", name: "mac-mint" },
				},
				malformed,
			);
			getCallback()!({ payload });
			expect(activityStore.getForSection("messages")).toHaveLength(0);
			expect(play).not.toHaveBeenCalled();
			expect(debug).toHaveBeenCalledExactlyOnceWith("app", "Discarding malformed mirrored MCP toast");
			debug.mockRestore();
			play.mockRestore();
		});

		// Catches: remote toast has no host label, loses severity/sound, or clicks the wrong tab.
		it("1439 labels a remote notice once and clicks its originating terminal", async () => {
			const { getCallback } = captureMcpToast();
			await initApp(createMockDeps());
			repositoriesStore.add({ path: "/remote/repo", displayName: "Remote", connectionId: "mint" });
			repositoriesStore.setWorkspace("/remote/repo", "main", { worktreePath: "/remote/repo" });
			const remote = terminalsStore.add(makeTerminal({ sessionId: "remote-pty", cwd: "/remote/repo" }));
			repositoriesStore.addTerminalToWorkspace("/remote/repo", "main", remote);
			const local = terminalsStore.add(makeTerminal({ sessionId: "local-pty" }));
			terminalsStore.setActive(local);
			const play = vi.spyOn(notificationsStore, "play").mockResolvedValue(undefined);
			const payload: ToastPayload = {
				title: "Need input",
				message: "Which branch?",
				level: "warn",
				sound: "attention",
				origin_session_id: "remote-pty",
				__tuic_origin: { connection: "mint", name: "mac-mint" },
			};
			getCallback()!({ payload });
			getCallback()!({ payload });
			const notices = activityStore.getForSection("messages");
			expect(notices).toHaveLength(1);
			expect(notices[0]).toMatchObject({
				title: "[mac-mint] Need input",
				subtitle: "Which branch?",
				severity: "warn",
				repoPath: "/remote/repo",
			});
			expect(play).toHaveBeenCalledOnce();
			expect(play).toHaveBeenCalledWith("attention", { originSessionId: "remote-pty" });
			expect(terminalsStore.state.activeId).toBe(local);
			notices[0].onClick!();
			expect(terminalsStore.state.activeId).toBe(remote);
			expect(repositoriesStore.state.activeRepoPath).toBe("/remote/repo");
			play.mockRestore();
		});

		// Catches: an unknown, closed, or local session in a remote notice steals focus.
		it("1439 remote notices cannot navigate to unknown or locally owned sessions", async () => {
			const { getCallback } = captureMcpToast();
			await initApp(createMockDeps());
			repositoriesStore.add({ path: "/same/path", displayName: "Local" });
			const local = terminalsStore.add(makeTerminal({ sessionId: "local-pty", cwd: "/same/path" }));
			terminalsStore.setActive(local);
			const setActive = vi.spyOn(terminalsStore, "setActive");
			for (const session of ["unknown", "local-pty"]) {
				getCallback()!({
					payload: {
						title: session,
						message: null,
						level: "error",
						sound: null,
						origin_repo_path: "/same/path",
						origin_session_id: session,
						__tuic_origin: { connection: "mint" },
					},
				});
				const notice = activityStore.getForSection("messages").find((item) => item.title === `[mint] ${session}`)!;
				expect(notice.repoPath).toBeUndefined();
				expect(notice.severity).toBe("error");
				notice.onClick!();
			}
			expect(setActive).not.toHaveBeenCalled();
			expect(terminalsStore.state.activeId).toBe(local);
			setActive.mockRestore();
		});

		// Catches: a retained remote notice uses a stale tab after it closes or changes owner.
		it("1439 rechecks remote terminal ownership when a retained notice is clicked", async () => {
			const { getCallback } = captureMcpToast();
			await initApp(createMockDeps());
			repositoriesStore.add({ path: "/remote/repo", displayName: "Remote", connectionId: "mint" });
			const remote = terminalsStore.add(makeTerminal({ sessionId: "remote-pty", cwd: "/remote/repo" }));
			const local = terminalsStore.add(makeTerminal({ sessionId: "local-pty" }));
			terminalsStore.setActive(local);
			getCallback()!({
				payload: {
					title: "Retained",
					message: null,
					level: "info",
					sound: null,
					origin_session_id: "remote-pty",
					__tuic_origin: { connection: "mint", name: "mac-mint" },
				},
			});
			const notice = activityStore.getForSection("messages")[0];
			terminalsStore.remove(remote);
			const replacement = terminalsStore.add(makeTerminal({ sessionId: "remote-pty", cwd: "/remote/repo" }));
			repositoriesStore.add({ path: "/remote/repo", displayName: "Other", connectionId: "other-host" });
			const setActive = vi.spyOn(terminalsStore, "setActive");
			notice.onClick!();
			terminalsStore.remove(replacement);
			notice.onClick!();
			expect(setActive).not.toHaveBeenCalled();
			expect(terminalsStore.state.activeId).toBe(local);
			setActive.mockRestore();
		});

		it("plays the named sound through the notification scheme, not the toast's own tone", async () => {
			const { getCallback } = captureMcpToast();
			const deps = createMockDeps();
			await initApp(deps);
			const play = vi.spyOn(notificationsStore, "play").mockResolvedValue(undefined);

			getCallback()!({
				payload: { title: "need you", message: "which branch?", level: "warn", sound: "attention" },
			});

			expect(play).toHaveBeenCalledWith("attention", { originSessionId: undefined });
			expect(toastsStore.toasts).toHaveLength(0);
			expect(activityStore.getForSection("messages")[0]).toMatchObject({
				title: "need you",
				subtitle: "which branch?",
				severity: "warn",
			});
			play.mockRestore();
		});

		it("1397 duplicate backend delivery keeps one bell item and plays its requested sound once", async () => {
			// catches: transport redelivery duplicates both the bell notice and attention sound
			const { getCallback } = captureMcpToast();
			await initApp(createMockDeps());
			const play = vi.spyOn(notificationsStore, "play").mockResolvedValue(undefined);
			const event = {
				payload: {
					title: "Duplicate delivery",
					message: "same",
					level: "warn",
					sound: "attention",
					origin_session_id: "caller",
				},
			};
			getCallback()!(event);
			getCallback()!(event);
			expect(
				activityStore.getForSection("messages").filter((item) => item.title === "Duplicate delivery"),
			).toHaveLength(1);
			expect(play).toHaveBeenCalledTimes(1);
			play.mockRestore();
		});

		it("1397 a repeated backend failure ten minutes later creates a fresh item and sound", async () => {
			// catches: dedup lasts as long as the old bell item, silencing a new failure
			const { getCallback } = captureMcpToast();
			await initApp(createMockDeps());
			const play = vi.spyOn(notificationsStore, "play").mockResolvedValue(undefined);
			const event = {
				payload: {
					title: "Repeated failure",
					message: "same",
					level: "error",
					sound: "error",
					origin_session_id: "caller",
				},
			};
			getCallback()!(event);
			vi.setSystemTime(Date.now() + 10 * 60_000);
			getCallback()!(event);
			expect(activityStore.getForSection("messages").filter((item) => item.title === "Repeated failure")).toHaveLength(
				2,
			);
			expect(play).toHaveBeenCalledTimes(2);
			play.mockRestore();
		});

		it("stays silent when no sound was requested or the name is unknown", async () => {
			const { getCallback } = captureMcpToast();
			const deps = createMockDeps();
			await initApp(deps);
			const play = vi.spyOn(notificationsStore, "play").mockResolvedValue(undefined);

			getCallback()!({ payload: { title: "done", message: null, level: "info", sound: null } });
			getCallback()!({ payload: { title: "done", message: null, level: "info", sound: "buzzer" } });

			expect(play).not.toHaveBeenCalled();
			play.mockRestore();
		});

		it("1397 scopes the notification to the repository resolved from the caller cwd", async () => {
			repositoriesStore.add({ path: "/Gits/personal/tuicommander", displayName: "TUICommander" });
			const { getCallback } = captureMcpToast();
			const deps = createMockDeps();
			await initApp(deps);

			getCallback()!({
				payload: {
					title: "Release published",
					message: "v1.7.4",
					level: "info",
					sound: null,
					origin_repo_path: "/Gits/personal/tuicommander/src-tauri",
					origin_session_id: "sess-abc",
				},
			});

			expect(toastsStore.toasts).toHaveLength(0);
			expect(activityStore.getForSection("messages")[0]).toMatchObject({
				title: "Release published",
				subtitle: "v1.7.4",
				repoPath: "/Gits/personal/tuicommander",
				severity: "info",
			});
			expect(activityStore.getForSection("messages")[0].onClick).toBeTypeOf("function");
		});

		it("1752 an urgent notice shows a card and a bell item; a normal one only the bell item", async () => {
			// catches: urgency ignored by the listener, or normal notices popping up as cards again
			const { getCallback } = captureMcpToast();
			await initApp(createMockDeps());
			for (const toast of [...toastsStore.toasts]) toastsStore.remove(toast.id);
			const base = { message: "body", level: "info", sound: null };

			getCallback()!({ payload: { ...base, title: "Quiet", urgency: "normal" } });
			getCallback()!({ payload: { ...base, title: "Legacy" } });
			expect(toastsStore.toasts).toHaveLength(0);

			getCallback()!({ payload: { ...base, title: "Loud", urgency: "urgent" } });
			expect(toastsStore.toasts.map((t) => t.title)).toEqual(["Loud"]);
			expect(
				activityStore
					.getForSection("messages")
					.map((i) => i.title)
					.sort(),
			).toEqual(["Legacy", "Loud", "Quiet"]);
		});

		it("1752 a mirrored urgent notice keeps its connection prefix on the card and is not duplicated", async () => {
			// catches: remote urgent toast loses the [connection] label, or a redelivery stacks cards
			const { getCallback } = captureMcpToast();
			await initApp(createMockDeps());
			for (const toast of [...toastsStore.toasts]) toastsStore.remove(toast.id);
			const payload: ToastPayload = {
				title: "Answer",
				message: "42",
				level: "info",
				sound: null,
				urgency: "urgent",
				__tuic_origin: { connection: "mint", name: "mac-mint" },
			};
			getCallback()!({ payload });
			getCallback()!({ payload });
			expect(toastsStore.toasts.map((t) => t.title)).toEqual(["[mac-mint] Answer"]);
			expect(activityStore.getForSection("messages")).toHaveLength(1);
		});
	});

	describe("session-created event (agent tab activation)", () => {
		type SessionCreatedPayload = {
			session_id: string;
			cwd: string | null;
			agent_type?: string | null;
			display_name?: string | null;
		};

		function captureSessionCreated() {
			const listenMock = vi.mocked(listen);
			let callback: ((event: { payload: SessionCreatedPayload }) => void) | null = null;
			listenMock.mockImplementation(((event: string, handler: (event: { payload: unknown }) => void) => {
				if (event === "session-created") {
					callback = handler as typeof callback;
				}
				return Promise.resolve(vi.fn());
			}) as unknown as typeof listen);
			return { getCallback: () => callback };
		}

		beforeEach(() => {
			paneLayoutStore.reset();
			resetGroupCounter();
		});

		// Catches: a managed spawn interrupting input or leaving no retained notice.
		it("1397 retains agent spawns in the bell without a toast", async () => {
			const { getCallback } = captureSessionCreated();
			await initApp(createMockDeps());
			getCallback()!({
				payload: { session_id: "1397-agent", cwd: null, agent_type: "claude", display_name: "Worker" },
			});
			expect(toastsStore.toasts).toHaveLength(0);
			expect(activityStore.getForSection("messages")[0]).toMatchObject({ title: "Agent started", subtitle: "Worker" });
		});

		it("setActive not called when active terminal already exists", async () => {
			const { getCallback } = captureSessionCreated();
			const deps = createMockDeps();
			await initApp(deps);

			// Pre-existing active terminal
			const existingId = terminalsStore.add({
				sessionId: "existing",
				fontSize: 14,
				name: "Existing",
				cwd: "/tmp",
				awaitingInput: null,
			});
			terminalsStore.setActive(existingId);

			const setActiveSpy = vi.spyOn(terminalsStore, "setActive");
			getCallback()!({ payload: { session_id: "new-sess", cwd: null, agent_type: "claude" } });

			expect(setActiveSpy).not.toHaveBeenCalled();
			setActiveSpy.mockRestore();
		});

		it("setActive called when no active terminal exists", async () => {
			const { getCallback } = captureSessionCreated();
			const deps = createMockDeps();
			await initApp(deps);

			expect(terminalsStore.state.activeId).toBeNull();

			getCallback()!({ payload: { session_id: "new-sess", cwd: null, agent_type: "claude" } });

			const newId = terminalsStore.getIds().find((id) => terminalsStore.get(id)?.sessionId === "new-sess");
			expect(newId).toBeDefined();
			expect(terminalsStore.state.activeId).toBe(newId);
			expect(terminalsStore.get(newId!)?.nameIsCustom).toBe(false);
		});

		/// Opening a diff/markdown/plugin tab runs the terminals pane deactivator, which
		/// sets `activeId` to null while the terminals stay. A spawn arriving then used to
		/// read that null as "no terminals" and call setActive, and setActive activates the
		/// terminals pane exclusively — so a worker tab replaced the panel the user was
		/// reading. Observed live: a MyWallet worker took over the wiz kanban tab.
		it("setActive not called when terminals exist but a non-terminal tab holds the pane", async () => {
			const { getCallback } = captureSessionCreated();
			const deps = createMockDeps();
			await initApp(deps);

			const existingId = terminalsStore.add({
				sessionId: "existing",
				fontSize: 14,
				name: "Existing",
				cwd: "/tmp",
				awaitingInput: null,
			});
			terminalsStore.setActive(existingId);
			// The user opens a panel: the pane deactivator clears activeId, terminals remain.
			terminalsStore.setActive(null);
			expect(terminalsStore.state.activeId).toBeNull();
			expect(terminalsStore.getCount()).toBe(1);

			const setActiveSpy = vi.spyOn(terminalsStore, "setActive");
			getCallback()!({ payload: { session_id: "new-sess", cwd: null, agent_type: "claude" } });

			expect(setActiveSpy).not.toHaveBeenCalled();
			expect(terminalsStore.state.activeId).toBeNull();
			setActiveSpy.mockRestore();
		});

		it("pins a spawned agent display name against intent and OSC titles", async () => {
			const { getCallback } = captureSessionCreated();
			const deps = createMockDeps();
			await initApp(deps);

			getCallback()!({
				payload: {
					session_id: "named-sess",
					cwd: "/repo",
					agent_type: "codex",
					display_name: "windows-primary",
				},
			});

			const terminalId = terminalsStore.getIds().find((id) => terminalsStore.get(id)?.sessionId === "named-sess");
			expect(terminalsStore.get(terminalId!)?.name).toBe("windows-primary");
			expect(terminalsStore.get(terminalId!)?.nameIsCustom).toBe(true);
		});

		it("setActiveGroup called with first leaf when split but no active group", async () => {
			const { getCallback } = captureSessionCreated();
			const deps = createMockDeps();
			await initApp(deps);

			// Set up split mode with two groups but no activeGroupId
			const g1 = paneLayoutStore.createGroup();
			const g2 = paneLayoutStore.createGroup();
			paneLayoutStore.setRoot({
				type: "branch",
				direction: "horizontal",
				children: [
					{ type: "leaf", id: g1 },
					{ type: "leaf", id: g2 },
				],
				ratios: [0.5, 0.5],
			});
			// activeGroupId should be null since we called setRoot directly (not split())
			expect(paneLayoutStore.state.activeGroupId).toBeNull();

			const setActiveGroupSpy = vi.spyOn(paneLayoutStore, "setActiveGroup");
			getCallback()!({ payload: { session_id: "new-sess", cwd: null, agent_type: "claude" } });

			expect(setActiveGroupSpy).toHaveBeenCalledWith(g1);
			setActiveGroupSpy.mockRestore();
		});
	});
});
