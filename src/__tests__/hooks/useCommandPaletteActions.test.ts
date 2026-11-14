import { render } from "@solidjs/testing-library";
import { createComponent, createRoot } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ToastList } from "../../components/ToastContainer/ToastList";

const {
	mockReveal,
	mockContextActions,
	mockGetActionEntries,
	mockPluginStore,
	mockPrompt,
	mockRepositories,
	mockTerminals,
} = vi.hoisted(() => ({
	mockReveal: vi.fn(),
	mockContextActions: { getActions: vi.fn() },
	mockGetActionEntries: vi.fn(),
	mockPluginStore: {
		state: { plugins: [] as Array<Record<string, unknown>> },
		getPlugin: vi.fn(),
		setEnabled: vi.fn().mockResolvedValue(undefined),
	},
	mockPrompt: { id: "prompt-1", name: "Review", shortcut: "Cmd+R" },
	mockRepositories: {
		state: {
			repositories: {
				"/active": {
					path: "/active",
					displayName: "Active",
					parked: false,
					activeWorkspaceId: "main",
					workspaces: { main: {} },
				},
				"/parked": {
					path: "/parked",
					displayName: "Parked",
					parked: true,
					activeWorkspaceId: "dev",
					workspaces: { dev: {} },
				},
			},
			groups: { group: { id: "group", name: "Team", repoOrder: ["/active"] } },
		},
		setActive: vi.fn(),
		setPark: vi.fn(),
		isGroupFullyParked: vi.fn(() => false),
		setParkGroup: vi.fn(),
	},
	mockTerminals: { state: { activeId: "term-1" as string | null }, get: vi.fn(), findTerminalWithSession: vi.fn() },
}));

vi.mock("../../actions/actionRegistry", () => ({ getActionEntries: mockGetActionEntries }));
vi.mock("../../utils/revealRepoInSidebar", () => ({ revealRepoInSidebar: mockReveal }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { error: vi.fn(), warn: vi.fn() } }));
vi.mock("../../stores/commandPalette", () => ({ commandPaletteStore: { openWithQuery: vi.fn() } }));
vi.mock("../../stores/contextMenuActionsStore", () => ({ contextMenuActionsStore: mockContextActions }));
vi.mock("../../stores/pluginStore", () => ({ pluginStore: mockPluginStore }));
vi.mock("../../stores/promptLibrary", () => ({
	promptLibraryStore: { getSmartByPlacement: vi.fn(() => [mockPrompt]) },
}));
vi.mock("../../stores/repositories", () => ({ repositoriesStore: mockRepositories }));
vi.mock("../../stores/terminals", () => ({ terminalsStore: mockTerminals }));
vi.mock("../../invoke", () => ({
	invoke: vi.fn().mockResolvedValue(undefined),
	listen: vi.fn().mockResolvedValue(vi.fn()),
}));
vi.mock("../../stores/updater", () => ({ updaterStore: { checkForUpdate: vi.fn().mockResolvedValue(undefined) } }));

import { useCommandPaletteActions } from "../../hooks/useCommandPaletteActions";
import { invoke, listen } from "../../invoke";
import { toastsStore } from "../../stores/toasts";
import * as transport from "../../transport";

describe("useCommandPaletteActions", () => {
	let dispose: (() => void) | undefined;

	beforeEach(() => {
		vi.useFakeTimers();
		for (const toast of [...toastsStore.toasts]) toastsStore.remove(toast.id);
		// A fresh array per call, as in production: the hook appends to it on every recompute.
		mockGetActionEntries.mockImplementation(() => [
			{ id: "static", label: "Static", category: "Test", keybinding: "", execute: vi.fn() },
		]);
		mockPluginStore.state.plugins = [
			{ id: "plugin-1", enabled: true, builtIn: false, manifest: { name: "Plugin One" } },
		];
		mockPluginStore.getPlugin.mockReturnValue({ id: "plugin-1", enabled: true });
		mockContextActions.getActions.mockReturnValue([
			{ id: "inspect", label: "Inspect", action: vi.fn(), disabled: vi.fn() },
		]);
		mockTerminals.get.mockReturnValue({ sessionId: "session-1", agentType: "claude" });
		mockTerminals.findTerminalWithSession.mockReturnValue({ sessionId: "session-1", agentType: "claude" });
		vi.clearAllMocks();
		vi.spyOn(toastsStore, "add");
	});

	const renderActions = () => {
		dispose?.();
		let actions: ReturnType<typeof useCommandPaletteActions> | undefined;
		createRoot((rootDispose) => {
			dispose = rootDispose;
			actions = useCommandPaletteActions({
				shortcutHandlers: {} as never,
				gitOps: { getWorktreeTargets: vi.fn(() => []) } as never,
				splitPanes: { resetLayout: vi.fn() } as never,
				executeSmartPrompt: vi.fn().mockResolvedValue(undefined),
			});
		});
		return (id: string) => actions?.().find((entry) => entry.id === id);
	};

	it("starts design mode for the active terminal, not for another session with a live agent", async () => {
		// findTerminalWithSession falls back to the last active or the first
		// terminal with a session — an agent the user may not be looking at.
		mockTerminals.get.mockImplementation((id: string) =>
			id === "term-1" ? { sessionId: "active-session", agentType: "claude" } : undefined,
		);
		mockTerminals.findTerminalWithSession.mockReturnValue({ sessionId: "other-session", agentType: "claude" });
		const byId = renderActions();
		// Catches: a duplicate generic start hiding browser choice.
		expect(byId("start-design-mode")).toBeUndefined();
		const action = byId("start-design-mode-edge");
		expect(action).toBeDefined();
		action?.execute();
		await Promise.resolve();
		expect(invoke).toHaveBeenCalledWith("start_design_mode", { sessionId: "active-session", browser: "edge" });
	});

	it("hides design mode when the active tab is not an agent terminal", () => {
		mockTerminals.findTerminalWithSession.mockReturnValue({ sessionId: "other-session", agentType: "claude" });
		mockTerminals.get.mockReturnValue(undefined);
		expect(renderActions()("start-design-mode-edge")).toBeUndefined();
		mockTerminals.get.mockReturnValue({ sessionId: "shell-session", agentType: null });
		expect(renderActions()("start-design-mode-edge")).toBeUndefined();
	});

	it("offers Stop instead of Start while the active agent's repository is armed", async () => {
		mockTerminals.get.mockReturnValue({ sessionId: "session-1", agentType: "claude" });
		const byId = renderActions();
		await Promise.resolve();
		const push = vi.mocked(listen).mock.calls.find(([name]) => name === "design-mode-changed")?.[1];
		expect(push).toBeDefined();
		push?.({ payload: { repo_path: "/repo", session_id: "session-1", status: "armed" } } as never);

		// Start again would silently rebind the armed repository to this session.
		expect(byId("start-design-mode-edge")).toBeUndefined();
		const stop = byId("stop-design-mode");
		expect(stop?.label).toBe("Stop Design Mode");
		stop?.execute();
		expect(invoke).toHaveBeenCalledWith("stop_design_mode", { repoPath: "/repo" });

		push?.({ payload: { repo_path: "/repo", session_id: "session-1", status: "stopped" } } as never);
		expect(byId("stop-design-mode")).toBeUndefined();
		expect(byId("start-design-mode-edge")).toBeDefined();
	});

	it("reports a design mode start failure with the backend message", async () => {
		vi.mocked(invoke).mockRejectedValueOnce(new Error("No dev server"));
		let actions: ReturnType<typeof useCommandPaletteActions> | undefined;
		createRoot((rootDispose) => {
			dispose = rootDispose;
			actions = useCommandPaletteActions({
				shortcutHandlers: {} as never,
				gitOps: { getWorktreeTargets: vi.fn(() => []) } as never,
				splitPanes: { resetLayout: vi.fn() } as never,
				executeSmartPrompt: vi.fn().mockResolvedValue(undefined),
			});
		});
		actions?.()
			.find((entry) => entry.id === "start-design-mode-edge")
			?.execute();
		await Promise.resolve();
		expect(toastsStore.add).toHaveBeenCalledWith(expect.any(String), "No dev server", "error");
	});

	it("explains the browser host after starting design mode from the palette", async () => {
		const notice = render(() => createComponent(ToastList, { onDismiss: () => {}, repoName: () => null }));
		const tauri = vi.spyOn(transport, "isTauri").mockReturnValue(false);
		let actions: ReturnType<typeof useCommandPaletteActions> | undefined;
		createRoot((rootDispose) => {
			dispose = rootDispose;
			actions = useCommandPaletteActions({
				shortcutHandlers: {} as never,
				gitOps: { getWorktreeTargets: vi.fn(() => []) } as never,
				splitPanes: { resetLayout: vi.fn() } as never,
				executeSmartPrompt: vi.fn().mockResolvedValue(undefined),
			});
		});
		actions?.()
			.find((entry) => entry.id === "start-design-mode-edge")
			?.execute();
		await Promise.resolve();
		expect(notice.getByText("Browser opened on the host machine.")).toBeVisible();
		expect(notice.getByText("Design Mode")).toBeVisible();
		tauri.mockRestore();
	});

	afterEach(async () => {
		for (const toast of [...toastsStore.toasts]) toastsStore.remove(toast.id);
		dispose?.();
		dispose = undefined;
		await vi.runOnlyPendingTimersAsync();
		vi.useRealTimers();
	});

	it("combines static, repository, plugin, worktree, prompt, and registered actions", () => {
		const gitOps = {
			handleBranchSelect: vi.fn(),
			handleAddRepo: vi.fn(),
			getWorktreeTargets: vi.fn(() => [{ path: "/wt", branchName: "feature" }]),
			moveTerminalToWorktree: vi.fn(),
		};
		let actions: ReturnType<typeof useCommandPaletteActions> | undefined;
		createRoot((rootDispose) => {
			dispose = rootDispose;
			actions = useCommandPaletteActions({
				shortcutHandlers: {} as never,
				gitOps: gitOps as never,
				splitPanes: { resetLayout: vi.fn() } as never,
				executeSmartPrompt: vi.fn().mockResolvedValue(undefined),
			});
		});

		const ids = actions?.().map((action) => action.id);
		expect(ids).toEqual(
			expect.arrayContaining([
				"static",
				"switch-repo:/active",
				"unpark-repo:/parked",
				"park-group:group",
				"toggle-plugin:plugin-1",
				"move-to-worktree:/wt",
				"smart:prompt-1",
				"plugin-action:inspect",
			]),
		);
	});

	it("keeps dynamic action closures bound to the represented entity", async () => {
		const pluginAction = vi.fn();
		mockContextActions.getActions.mockReturnValue([{ id: "inspect", label: "Inspect", action: pluginAction }]);
		const executeSmartPrompt = vi.fn().mockResolvedValue(undefined);
		const gitOps = {
			handleBranchSelect: vi.fn(),
			handleAddRepo: vi.fn(),
			getWorktreeTargets: vi.fn(() => [{ path: "/wt", branchName: "feature" }]),
			moveTerminalToWorktree: vi.fn(),
		};
		let actions: ReturnType<typeof useCommandPaletteActions> | undefined;
		createRoot((rootDispose) => {
			dispose = rootDispose;
			actions = useCommandPaletteActions({
				shortcutHandlers: {} as never,
				gitOps: gitOps as never,
				splitPanes: { resetLayout: vi.fn() } as never,
				executeSmartPrompt,
			});
		});

		const byId = (id: string) =>
			actions?.()
				.find((action) => action.id === id)
				?.execute();
		byId("switch-repo:/active");
		byId("unpark-repo:/parked");
		byId("toggle-plugin:plugin-1");
		byId("move-to-worktree:/wt");
		byId("smart:prompt-1");
		byId("plugin-action:inspect");
		await Promise.resolve();

		// catches: Ctrl+P picks a repo (tab opens) but the sidebar is not revealed
		expect(mockReveal).toHaveBeenNthCalledWith(1, "/active");
		expect(mockReveal).toHaveBeenNthCalledWith(2, "/parked");
		expect(gitOps.handleBranchSelect).toHaveBeenNthCalledWith(1, "/active", "main");
		expect(gitOps.handleBranchSelect).toHaveBeenNthCalledWith(2, "/parked", "dev");
		expect(mockPluginStore.setEnabled).toHaveBeenCalledWith("plugin-1", false);
		expect(gitOps.moveTerminalToWorktree).toHaveBeenCalledWith("term-1", "/wt");
		expect(executeSmartPrompt).toHaveBeenCalledWith(mockPrompt);
		expect(pluginAction).toHaveBeenCalledWith({ sessionId: "session-1", repoPath: null });
	});
});
