import { fireEvent, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useQuickSwitcherVisibility } from "../../hooks/useQuickSwitcherVisibility";
import { getModifierSymbol, isMacOS, resetPlatformCache } from "../../platform";
import type { AwaitingInputType } from "../../stores/terminals";

const mockCopyPathToClipboard = vi.hoisted(() => vi.fn());
const mockWriteClipboard = vi.hoisted(() => vi.fn().mockResolvedValue(undefined));
vi.mock("../../utils/clipboard", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../utils/clipboard")>()),
	copyPathToClipboard: mockCopyPathToClipboard,
	writeClipboard: mockWriteClipboard,
}));

const { mockOpenLocalPath, mockHandleOpenUrl } = vi.hoisted(() => ({
	mockOpenLocalPath: vi.fn(),
	mockHandleOpenUrl: vi.fn(),
}));
vi.mock("../../utils/openUrl", () => ({
	openLocalPath: mockOpenLocalPath,
	handleOpenUrl: mockHandleOpenUrl,
}));

// Mock Tauri APIs
vi.mock("@tauri-apps/api/core", () => ({
	invoke: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("../../invoke", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../invoke")>()),
	invoke: vi.fn().mockResolvedValue(undefined),
	listen: vi.fn().mockResolvedValue(vi.fn()),
}));
vi.mock("@tauri-apps/api/event", () => ({
	listen: vi.fn().mockResolvedValue(vi.fn()),
	emit: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("@tauri-apps/api/window", () => ({
	getCurrentWindow: vi.fn(() => ({
		listen: vi.fn().mockResolvedValue(vi.fn()),
		setTitle: vi.fn().mockResolvedValue(undefined),
	})),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({
	open: vi.fn().mockResolvedValue(null),
}));
vi.mock("@tauri-apps/plugin-opener", () => ({
	openUrl: vi.fn().mockResolvedValue(undefined),
}));

import type { ContextMenuItem } from "../../components/ContextMenu/ContextMenu";
import { TabBar } from "../../components/TabBar/TabBar";
import { ToastList } from "../../components/ToastContainer/ToastList";
import { invoke, listen } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import { diffTabsStore } from "../../stores/diffTabs";
import { editorTabsStore } from "../../stores/editorTabs";
import { globalWorkspaceStore } from "../../stores/globalWorkspace";
import { mdTabsStore } from "../../stores/mdTabs";
import { paneLayoutStore } from "../../stores/paneLayout";
import { repositoriesStore } from "../../stores/repositories";
import { settingsStore, type TabOrderingMode } from "../../stores/settings";
import { tabOrderingStore } from "../../stores/tabManager";
import { terminalsStore } from "../../stores/terminals";
import { toastsStore } from "../../stores/toasts";
import { uiStore } from "../../stores/ui";
import * as transport from "../../transport";
import { keyFor } from "../../utils/hotkey";
import { adoptSuspendedSession } from "../../utils/suspendedSessions";

describe("TabBar", () => {
	beforeEach(async () => {
		vi.useFakeTimers();
		vi.mocked(invoke).mockReset().mockResolvedValue(undefined);
		await (await import("../../stores/activityStore")).activityStore.hydrate();
		vi.mocked(listen).mockReset().mockResolvedValue(vi.fn());
		localStorage.clear();
		// Clean up any terminals from previous tests
		for (const id of terminalsStore.getIds()) {
			terminalsStore.remove(id);
		}
		// Clean up repos
		for (const path of repositoriesStore.getPaths()) {
			repositoriesStore.remove(path);
		}
		repositoriesStore.setActive(null);
		// Clean up diff/md tabs
		for (const id of diffTabsStore.getIds()) {
			diffTabsStore.remove(id);
		}
		for (const id of mdTabsStore.getIds()) {
			mdTabsStore.remove(id);
		}
		for (const id of editorTabsStore.getIds()) {
			editorTabsStore.remove(id);
		}
		tabOrderingStore.clear();
		for (const panelId of Object.keys(uiStore.state.detachedPanels)) {
			if (panelId.startsWith("markdown-tab-")) uiStore.clearDetached(panelId);
		}
		settingsStore.setTabOrderingMode("grouped-by-type");
		// Deactivate global workspace
		if (globalWorkspaceStore.isActive()) {
			globalWorkspaceStore.deactivate();
		}
		for (const id of globalWorkspaceStore.getPromotedIds()) {
			globalWorkspaceStore.unpromote(id);
		}
	});

	afterEach(() => {
		vi.useRealTimers();
		paneLayoutStore._testCancelPendingSave();
		repositoriesStore._testCancelPendingSave();
	});

	function addTerminal(
		overrides: Partial<{
			name: string;
			sessionId: string | null;
			fontSize: number;
			cwd: string | null;
			awaitingInput: AwaitingInputType;
			lastError: string;
		}> = {},
	) {
		return terminalsStore.add({
			name: overrides.name ?? "Terminal",
			sessionId: overrides.sessionId ?? null,
			fontSize: overrides.fontSize ?? 14,
			cwd: overrides.cwd ?? null,
			awaitingInput: overrides.awaitingInput ?? null,
			...(overrides.lastError ? { lastError: overrides.lastError } : {}),
		});
	}

	// Catches: a persisted backend record being lost when a fresh frontend mounts.
	it("suspended_record_survives_restart_and_frontend_renders_it", () => {
		const id = adoptSuspendedSession(
			{
				tuic_session: "durable-tab",
				session_id: "old-pty",
				agent_type: null,
				cwd: "/Gits/alpha",
				worktree: null,
				agent_session_id: null,
				launch_command: null,
				alias: "parked",
				pane_slot: null,
				reason: "user",
				name: "Durable shell",
			},
			14,
		);
		const view = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		expect(view.getByText("Durable shell")).toBeTruthy();
		expect(terminalsStore.get(id)?.suspended).toBe(true);
		expect(view.container.querySelector(".suspendedBadge")).toBeTruthy();
	});

	// Catches: an agent label or later intent title hiding an explicit session rename.
	it("renders a custom session rename instead of the agent label", async () => {
		const expectedWarning = vi.spyOn(appLogger, "error").mockImplementation((_source, message) => {
			expect(message).toMatch(/Refusing to persist settings: store not hydrated/);
		});
		const id = addTerminal({ name: "ego", sessionId: "renamed-ego" });
		terminalsStore.update(id, { agentType: "ego" });
		const view = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		terminalsStore.update(id, { name: "Ripresa eg42 · immagini", nameIsCustom: true });
		const { handleIntentEvent } = await import("../../components/Terminal/intentTitle");
		handleIntentEvent({
			terminalId: id,
			text: "Working on images",
			title: "ego",
			globalEnabled: true,
			perAgentEnabled: true,
		});
		expect(view.getByText("Ripresa eg42 · immagini")).toBeTruthy();
		expectedWarning.mockRestore();
	});

	// Catches: launch-menu agent labels being pinned as custom and suppressing every later title.
	it("lets a launch-menu tab take a later agent intent title", async () => {
		const agentConfigs = await import("../../stores/agentConfigs");
		const configLoad = vi
			.spyOn(agentConfigs, "ensureAgentConfigsForRepo")
			.mockResolvedValue(agentConfigs.agentConfigsStore);
		const { useTerminalContextMenus } = await import("../../hooks/useTerminalContextMenus");
		const { handleIntentEvent } = await import("../../components/Terminal/intentTitle");
		const id = addTerminal({ name: "Terminal" });
		const menus = useTerminalContextMenus({
			agentDetection: { getAvailable: () => [{ type: "ego" }] },
			gitOps: { handleAddTerminalToWorkspace: async () => id },
		} as never);
		const view = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		await menus.buildSidebarAgentMenuItems("/repo", "main", true)[0].action();
		handleIntentEvent({
			terminalId: id,
			text: "Working on images",
			title: "Image work",
			globalEnabled: true,
			perAgentEnabled: true,
		});
		configLoad.mockRestore();
		expect(view.getByText("Image work")).toBeTruthy();
	});

	it("Alt-clicking new tab button calls onNewTab directly", () => {
		const onNewTab = vi.fn();
		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={onNewTab}
			/>
		));
		fireEvent.click(container.querySelector(".newBtn")!, { altKey: true });
		expect(onNewTab).toHaveBeenCalledTimes(1);
	});

	describe("long press on new tab button", () => {
		const renderWithAgents = (onNewTab: () => void, getNewAgentMenuItems: () => ContextMenuItem[]) =>
			render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={onNewTab}
					getNewAgentMenuItems={getNewAgentMenuItems}
				/>
			));
		const menuLabels = (container: HTMLElement) =>
			Array.from(container.querySelectorAll(".menu .label")).map((l) => l.textContent);

		it("opens the agent list instead of a plain tab, and launching an agent is one click away", () => {
			const onNewTab = vi.fn();
			const launchClaude = vi.fn();
			const { container } = renderWithAgents(onNewTab, () => [
				{ label: "Claude Code", action: launchClaude },
				{ label: "Codex", action: () => {} },
			]);
			const btn = container.querySelector(".newBtn")!;
			fireEvent.pointerDown(btn, { button: 0 });
			vi.advanceTimersByTime(500);
			fireEvent.pointerUp(btn);
			fireEvent.click(btn);

			expect(onNewTab).not.toHaveBeenCalled();
			expect(menuLabels(container)).toEqual(expect.arrayContaining(["Claude Code", "Codex"]));
			const claude = Array.from(container.querySelectorAll(".menu .label")).find(
				(l) => l.textContent === "Claude Code",
			)!;
			fireEvent.click(claude.closest(".item") ?? claude);
			expect(launchClaude).toHaveBeenCalledTimes(1);
		});

		it("a short press opens the launch menu and Terminal opens a plain tab", () => {
			const onNewTab = vi.fn();
			const getItems = vi.fn(() => [{ label: "Claude Code", action: () => {} }]);
			const { container } = renderWithAgents(onNewTab, getItems);
			const btn = container.querySelector(".newBtn")!;
			fireEvent.pointerDown(btn, { button: 0 });
			vi.advanceTimersByTime(200);
			fireEvent.pointerUp(btn);
			fireEvent.click(btn);
			vi.advanceTimersByTime(1000);

			expect(onNewTab).not.toHaveBeenCalled();
			expect(getItems).toHaveBeenCalledTimes(1);
			const terminal = Array.from(container.querySelectorAll(".menu .label")).find(
				(label) => label.textContent === "Terminal",
			)!;
			fireEvent.click(terminal.closest(".item") ?? terminal);
			expect(onNewTab).toHaveBeenCalledTimes(1);
		});

		it("leaving the button cancels the press", () => {
			const getItems = vi.fn(() => [{ label: "Claude Code", action: () => {} }]);
			const { container } = renderWithAgents(() => {}, getItems);
			const btn = container.querySelector(".newBtn")!;
			fireEvent.pointerDown(btn, { button: 0 });
			fireEvent.pointerLeave(btn);
			vi.advanceTimersByTime(1000);

			expect(getItems).not.toHaveBeenCalled();
		});

		it("a long press released off the button does not swallow the next keyboard activation", () => {
			// No click follows a release elsewhere, so only the menu closing can
			// tell the button that the long press is over.
			const onNewTab = vi.fn();
			const { container } = renderWithAgents(onNewTab, () => [{ label: "Claude Code", action: () => {} }]);
			const btn = container.querySelector(".newBtn")!;
			fireEvent.pointerDown(btn, { button: 0 });
			vi.advanceTimersByTime(500);
			fireEvent.pointerLeave(btn);
			fireEvent.keyDown(document, { key: "Escape" });
			expect(menuLabels(container)).not.toContain("Claude Code");

			fireEvent.click(btn);
			expect(onNewTab).not.toHaveBeenCalled();
			expect(menuLabels(container)).toContain("Claude Code");
		});

		it("the native contextmenu of a touch long press does not open the list a second time", () => {
			const getItems = vi.fn(() => [{ label: "Claude Code", action: () => {} }]);
			const { container } = renderWithAgents(() => {}, getItems);
			const btn = container.querySelector(".newBtn")!;
			// The press timer fires first and opens the list; the native event follows.
			fireEvent.pointerDown(btn, { button: 0 });
			vi.advanceTimersByTime(500);
			fireEvent.contextMenu(btn);
			expect(getItems).toHaveBeenCalledTimes(1);
			expect(menuLabels(container)).toContain("Claude Code");
		});

		it("a touch long press whose native contextmenu beats the timer still opens the list", () => {
			// The browser's gesture recognizer and the 500 ms timer race. If the
			// contextmenu wins, the pointerup that follows must not cancel the list
			// and let the click open a plain tab instead.
			const onNewTab = vi.fn();
			const getItems = vi.fn(() => [{ label: "Claude Code", action: () => {} }]);
			const { container } = renderWithAgents(onNewTab, getItems);
			const btn = container.querySelector(".newBtn")!;
			fireEvent.pointerDown(btn, { button: 0 });
			vi.advanceTimersByTime(450);
			const early = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
			btn.dispatchEvent(early);
			fireEvent.pointerUp(btn);
			fireEvent.click(btn);
			vi.advanceTimersByTime(1000);

			expect(early.defaultPrevented).toBe(true);
			expect(menuLabels(container)).toContain("Claude Code");
			expect(getItems).toHaveBeenCalledTimes(1);
			expect(onNewTab).not.toHaveBeenCalled();
		});

		it("a cancelled pointer gesture cancels the press", () => {
			// A touch that turns into a scroll ends in pointercancel, not pointerup.
			const getItems = vi.fn(() => [{ label: "Claude Code", action: () => {} }]);
			const { container } = renderWithAgents(() => {}, getItems);
			const btn = container.querySelector(".newBtn")!;
			fireEvent.pointerDown(btn, { button: 0 });
			fireEvent.pointerCancel(btn);
			vi.advanceTimersByTime(1000);

			expect(getItems).not.toHaveBeenCalled();
		});

		it("a right click opens the agent list without opening a tab", () => {
			const onNewTab = vi.fn();
			const launchCodex = vi.fn();
			const { container } = renderWithAgents(onNewTab, () => [
				{ label: "Claude Code", action: () => {} },
				{ label: "Codex", action: launchCodex },
			]);
			const btn = container.querySelector(".newBtn")!;
			fireEvent.pointerDown(btn, { button: 2 });
			const ev = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
			btn.dispatchEvent(ev);
			expect(ev.defaultPrevented).toBe(true);
			vi.advanceTimersByTime(1000);

			expect(onNewTab).not.toHaveBeenCalled();
			const codex = Array.from(container.querySelectorAll(".menu .label")).find((l) => l.textContent === "Codex")!;
			fireEvent.click(codex.closest(".item") ?? codex);
			expect(launchCodex).toHaveBeenCalledTimes(1);
		});

		it("unwraps the single Add Agent submenu so the agents are the list", () => {
			const { container } = renderWithAgents(
				() => {},
				() => [
					{
						label: "Add Agent",
						action: () => {},
						children: [
							{ label: "Claude Code", action: () => {} },
							{ label: "Codex", action: () => {} },
						],
					},
				],
			);
			fireEvent.contextMenu(container.querySelector(".newBtn")!);
			expect(menuLabels(container)).toEqual(expect.arrayContaining(["Claude Code", "Codex"]));
			expect(menuLabels(container)).not.toContain("Add Agent");
		});

		it("with no agents available a long press offers the Terminal action", () => {
			const onNewTab = vi.fn();
			const { container } = renderWithAgents(onNewTab, () => []);
			const btn = container.querySelector(".newBtn")!;
			fireEvent.pointerDown(btn, { button: 0 });
			vi.advanceTimersByTime(500);
			fireEvent.pointerUp(btn);
			fireEvent.click(btn);

			expect(onNewTab).not.toHaveBeenCalled();
			expect(menuLabels(container)).toEqual(["Terminal"]);
		});
	});

	it("new tab button has correct title", () => {
		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		expect(container.querySelector(".newBtn")!.getAttribute("title")).toBe(
			`Launch agent · Alt/Option-click: Terminal (${getModifierSymbol()}T)`,
		);
	});

	it("renders terminal tabs from the store", () => {
		addTerminal({ name: "Tab A" });
		addTerminal({ name: "Tab B" });

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const tabs = container.querySelectorAll(".tab");
		expect(tabs.length).toBe(2);
		expect(tabs[0].querySelector(".tabName")!.textContent).toContain("Tab A");
		expect(tabs[1].querySelector(".tabName")!.textContent).toContain("Tab B");
	});

	it("active tab has 'active' class", () => {
		const id1 = addTerminal({ name: "Tab 1" });
		addTerminal({ name: "Tab 2" });
		terminalsStore.setActive(id1);

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const tabs = container.querySelectorAll(".tab");
		expect(tabs[0].classList.contains("active")).toBe(true);
		expect(tabs[1].classList.contains("active")).toBe(false);
	});

	it("activity flag does NOT produce a visible dot class (busy covers it)", () => {
		const id1 = addTerminal({ name: "Active" });
		const id2 = addTerminal({ name: "Background" });
		terminalsStore.setActive(id1);
		terminalsStore.update(id2, { activity: true });

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const tabs = container.querySelectorAll(".tab");
		// activity flag no longer drives dot color — busy state does
		expect(tabs[0].classList.contains("hasActivity")).toBe(false);
		expect(tabs[1].classList.contains("hasActivity")).toBe(false);
	});

	it("tab awaiting input has 'awaitingInput awaitingQuestion' class", () => {
		const id = addTerminal({ name: "Waiting", awaitingInput: "question" });
		terminalsStore.setActive(id);

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const tab = container.querySelector(".tab")!;
		expect(tab.classList.contains("awaitingInput")).toBe(true);
		expect(tab.classList.contains("awaitingQuestion")).toBe(true);
	});

	// Catches: the agent's error text never reaches the tab, so a dead tab
	// looks the same as any other failure.
	it("tab awaiting error input has correct class and shows the error text", () => {
		const id = addTerminal({ name: "Error", awaitingInput: "error", lastError: "model refuses it" });
		terminalsStore.setActive(id);

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const tab = container.querySelector(".tab")!;
		expect(tab.classList.contains("awaitingInput")).toBe(true);
		expect(tab.classList.contains("awaitingError")).toBe(true);
		expect(tab.getAttribute("title")).toBe(`Terminal 1 (${getModifierSymbol()}1)\nmodel refuses it`);
	});

	it("tab with no awaitingInput has no awaiting classes", () => {
		const id = addTerminal({ name: "Normal" });
		terminalsStore.setActive(id);

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const tab = container.querySelector(".tab")!;
		expect(tab.classList.contains("awaitingInput")).toBe(false);
	});

	it("non-active tab with shellState 'idle' has 'shellIdle' class", () => {
		const id1 = addTerminal({ name: "Active" });
		const id2 = addTerminal({ name: "Idle" });
		terminalsStore.setActive(id1);
		terminalsStore.update(id2, { shellState: "idle" });

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const tabs = container.querySelectorAll(".tab");
		expect(tabs[0].classList.contains("shellIdle")).toBe(false);
		expect(tabs[1].classList.contains("shellIdle")).toBe(true);
	});

	it("active tab with shellState 'idle' has 'shellIdle' class", () => {
		const id1 = addTerminal({ name: "Active" });
		terminalsStore.setActive(id1);
		terminalsStore.update(id1, { shellState: "idle" });

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const tab = container.querySelector(".tab")!;
		expect(tab.classList.contains("shellIdle")).toBe(true);
	});

	it("non-active tab with shellState 'busy' does NOT have 'shellIdle' class", () => {
		const id1 = addTerminal({ name: "Active" });
		const id2 = addTerminal({ name: "Busy" });
		terminalsStore.setActive(id1);
		terminalsStore.update(id2, { shellState: "busy" });

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const tabs = container.querySelectorAll(".tab");
		expect(tabs[1].classList.contains("shellIdle")).toBe(false);
	});

	it("tab with progress shows bar at correct width (no label)", () => {
		const id = addTerminal({ name: "Progress" });
		terminalsStore.update(id, { progress: 50 });

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const tab = container.querySelector(".tab")!;
		expect(tab.querySelector(".progressLabel")).toBeNull();
		const bar = tab.querySelector(".progress");
		expect(bar).not.toBeNull();
		expect((bar as HTMLElement).style.transform).toBe("scaleX(0.5)");
	});

	it("tab with progress=0 shows bar, no label", () => {
		const id = addTerminal({ name: "Zero" });
		terminalsStore.update(id, { progress: 0 });

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const tab = container.querySelector(".tab")!;
		expect(tab.querySelector(".progressLabel")).toBeNull();
		const bar = tab.querySelector(".progress");
		expect(bar).not.toBeNull();
		expect((bar as HTMLElement).style.transform).toBe("scaleX(0)");
	});

	it("tab with progress=null does not show progress elements", () => {
		addTerminal({ name: "No Progress" });
		// progress defaults to null

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const tab = container.querySelector(".tab")!;
		expect(tab.querySelector(".progressLabel")).toBeNull();
		expect(tab.querySelector(".progress")).toBeNull();
	});

	it("clicking tab calls onTabSelect with correct id", () => {
		const handleSelect = vi.fn();
		addTerminal({ name: "Tab 1" });
		const id2 = addTerminal({ name: "Tab 2" });

		const { container } = render(() => (
			<TabBar
				onTabSelect={handleSelect}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const tabs = container.querySelectorAll(".tab");
		fireEvent.click(tabs[1]);
		expect(handleSelect).toHaveBeenCalledWith(id2);
	});

	it("clicking close button calls onTabClose and stops propagation", () => {
		const handleSelect = vi.fn();
		const handleClose = vi.fn();
		const id1 = addTerminal({ name: "Tab 1" });

		const { container } = render(() => (
			<TabBar
				onTabSelect={handleSelect}
				onTabClose={handleClose}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const closeBtn = container.querySelector(".tabClose")!;
		fireEvent.click(closeBtn);
		expect(handleClose).toHaveBeenCalledWith(id1);
		// stopPropagation means onTabSelect should NOT be called
		expect(handleSelect).not.toHaveBeenCalled();
	});

	it("when activeRepoPath and activeBranch set, uses branch terminals", () => {
		// Set up a repo with a branch that has specific terminals
		const repoPath = "/test/repo";
		repositoriesStore.add({ path: repoPath, displayName: "Test Repo" });
		repositoriesStore.setActive(repoPath);

		const t1 = addTerminal({ name: "Branch Term 1" });
		const t2 = addTerminal({ name: "Branch Term 2" });
		addTerminal({ name: "Other Term" }); // Not in the branch

		repositoriesStore.setWorkspace(repoPath, "main", {
			branchName: "main",
			terminals: [t1, t2],
		});
		repositoriesStore.setActiveWorkspace(repoPath, "main");

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const tabs = container.querySelectorAll(".tab");
		// Should only show t1 and t2, not t3
		expect(tabs.length).toBe(2);
		expect(tabs[0].querySelector(".tabName")!.textContent).toContain("Branch Term 1");
		expect(tabs[1].querySelector(".tabName")!.textContent).toContain("Branch Term 2");
	});

	it("with no activeRepoPath, shows all terminals", () => {
		addTerminal({ name: "T1" });
		addTerminal({ name: "T2" });
		addTerminal({ name: "T3" });

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		expect(container.querySelectorAll(".tab").length).toBe(3);
	});

	it("when global workspace is active, shows only promoted terminals", () => {
		const repoPath = "/test/repo";
		repositoriesStore.add({ path: repoPath, displayName: "Test Repo" });
		repositoriesStore.setActive(repoPath);

		const promoted1 = addTerminal({ name: "Promoted 1" });
		const promoted2 = addTerminal({ name: "Promoted 2" });
		const repoBound = addTerminal({ name: "Repo Term" });

		repositoriesStore.setWorkspace(repoPath, "main", {
			branchName: "main",
			terminals: [repoBound],
		});
		repositoriesStore.setActiveWorkspace(repoPath, "main");

		globalWorkspaceStore.promote(promoted1);
		globalWorkspaceStore.promote(promoted2);
		globalWorkspaceStore.activate();

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const tabs = container.querySelectorAll(".tab");
		expect(tabs.length).toBe(2);
		const names = Array.from(tabs).map((t) => t.querySelector(".tabName")!.textContent);
		expect(names).toContain("Promoted 1");
		expect(names).toContain("Promoted 2");
		expect(names).not.toContain("Repo Term");
	});

	it("with activeRepoPath but no activeBranch, falls back to all terminals", () => {
		const repoPath = "/test/repo";
		repositoriesStore.add({ path: repoPath, displayName: "Test Repo" });
		repositoriesStore.setActive(repoPath);
		// No activeBranch set

		addTerminal({ name: "T1" });
		addTerminal({ name: "T2" });

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		expect(container.querySelectorAll(".tab").length).toBe(2);
	});

	it("tab has correct title with 1-based index", () => {
		addTerminal({ name: "First" });
		addTerminal({ name: "Second" });

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const tabs = container.querySelectorAll(".tab");
		expect(tabs[0].getAttribute("title")).toBe(`Terminal 1 (${getModifierSymbol()}1)`);
		expect(tabs[1].getAttribute("title")).toBe(`Terminal 2 (${getModifierSymbol()}2)`);
	});

	it("pointerDown + move initiates drag (dragging class appears after threshold)", () => {
		addTerminal({ name: "Tab 1" });
		addTerminal({ name: "Tab 2" });

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const tabs = container.querySelectorAll(".tab");

		// pointerDown alone should NOT set dragging
		fireEvent.pointerDown(tabs[0], { button: 0, pointerId: 1, clientX: 10, clientY: 10 });
		expect(tabs[0].classList.contains("dragging")).toBe(false);

		// Move past threshold to start drag
		fireEvent.pointerMove(document, { pointerId: 1, clientX: 20, clientY: 10 });
		expect(tabs[0].classList.contains("dragging")).toBe(true);

		// Cleanup
		fireEvent.pointerUp(document, { pointerId: 1, clientX: 20, clientY: 10 });
	});

	it("pointerUp without movement does not trigger drag (click works normally)", () => {
		const handleReorder = vi.fn();
		addTerminal({ name: "Tab 1" });

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
				onReorder={handleReorder}
			/>
		));
		const tabs = container.querySelectorAll(".tab");

		fireEvent.pointerDown(tabs[0], { button: 0, pointerId: 1, clientX: 10, clientY: 10 });
		fireEvent.pointerUp(document, { pointerId: 1, clientX: 10, clientY: 10 });

		expect(handleReorder).not.toHaveBeenCalled();
		expect(tabs[0].classList.contains("dragging")).toBe(false);
	});

	it("escape cancels drag and resets state", () => {
		addTerminal({ name: "Tab 1" });
		addTerminal({ name: "Tab 2" });

		const { container } = render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
		const tabs = container.querySelectorAll(".tab");

		fireEvent.pointerDown(tabs[0], { button: 0, pointerId: 1, clientX: 10, clientY: 10 });
		fireEvent.pointerMove(document, { pointerId: 1, clientX: 20, clientY: 10 });
		expect(tabs[0].classList.contains("dragging")).toBe(true);

		fireEvent.keyDown(document, { key: "Escape" });
		expect(tabs[0].classList.contains("dragging")).toBe(false);
	});

	describe("diff tabs", () => {
		/** Set up an active repo+branch so diff/md tab visibility filtering works */
		function setupActiveRepo() {
			repositoriesStore.add({ path: "/repo", displayName: "repo" });
			repositoriesStore.setWorkspace("/repo", "main", { isMain: true, worktreePath: null });
			repositoriesStore.setActive("/repo");
			repositoriesStore.setActiveWorkspace("/repo", "main");
		}

		it("renders diff tabs", () => {
			setupActiveRepo();
			diffTabsStore.add("/repo", "/repo/file.ts", "M");

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			const diffTabs = container.querySelectorAll(".diffTab");
			expect(diffTabs.length).toBe(1);
			expect(diffTabs[0].querySelector(".tabName")!.textContent).toBe("file.ts");
		});

		it("clicking diff tab selects it", () => {
			setupActiveRepo();
			const id = diffTabsStore.add("/repo", "/repo/file.ts", "M");
			const handleSelect = vi.fn();

			const { container } = render(() => (
				<TabBar
					onTabSelect={handleSelect}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			fireEvent.click(container.querySelector(".diffTab")!);
			expect(handleSelect).toHaveBeenCalledWith(id);
			expect(diffTabsStore.state.activeId).toBe(id);
		});

		it("closing diff tab via close button", () => {
			setupActiveRepo();
			const id = diffTabsStore.add("/repo", "/repo/file.ts", "M");
			const handleClose = vi.fn();

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={handleClose}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			const closeBtn = container.querySelector(".diffTab .tabClose")!;
			fireEvent.click(closeBtn);
			expect(handleClose).toHaveBeenCalledWith(id);
		});

		it("middle-click closes diff tab", () => {
			setupActiveRepo();
			const id = diffTabsStore.add("/repo", "/repo/file.ts", "M");
			const handleClose = vi.fn();

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={handleClose}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			fireEvent(container.querySelector(".diffTab")!, new MouseEvent("auxclick", { button: 1, bubbles: true }));
			expect(handleClose).toHaveBeenCalledWith(id, true);
		});
	});

	describe("markdown tabs", () => {
		it("retains an MCP file tab after selecting a terminal in a repo without a workspace", () => {
			repositoriesStore.add({ path: "/repo", displayName: "repo" });
			repositoriesStore.setActive("/repo");
			const terminalId = addTerminal();
			const id = mdTabsStore.add("/repo", "/Users/boss/Gits/.tmp/boss/ego-coordinator-proposal.md");
			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			terminalsStore.setActive(terminalId);
			expect(mdTabsStore.get(id)).toBeDefined();
			expect(container.querySelector(`[data-tab-id="${id}"]`)).not.toBeNull();
			repositoriesStore.add({ path: "/other", displayName: "other" });
			repositoriesStore.setActive("/other");
			expect(container.querySelector(`[data-tab-id="${id}"]`)).toBeNull();
			repositoriesStore.setActive("/repo");
			expect(container.querySelector(`[data-tab-id="${id}"]`)).not.toBeNull();
		});

		it("shows two different MCP file tabs together in a repo without a workspace", () => {
			repositoriesStore.add({ path: "/repo", displayName: "repo" });
			repositoriesStore.setActive("/repo");
			const first = mdTabsStore.add("/repo", "/Users/boss/Gits/.tmp/boss/ego-coordinator-proposal.md");
			const second = mdTabsStore.add("/repo", "/Users/boss/Gits/.tmp/boss/tuic-mobile-files.md");
			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			expect(first).not.toBe(second);
			expect(container.querySelector(`[data-tab-id="${first}"]`)).not.toBeNull();
			expect(container.querySelector(`[data-tab-id="${second}"]`)).not.toBeNull();
		});
		it.each(["markdown", "editor"])("keeps an active pinned MCP %s tab visible across a repo switch", (kind) => {
			for (const path of ["/repo-a", "/repo-b"]) {
				repositoriesStore.add({ path, displayName: path });
				repositoriesStore.setWorkspace(path, "main", { worktreePath: path });
				repositoriesStore.setActiveWorkspace(path, "main");
			}
			repositoriesStore.setActive("/repo-a");
			const id =
				kind === "markdown"
					? mdTabsStore.openUiTab("pinned-preview", "Preview", "<p>test</p>", true)
					: editorTabsStore.add("/repo-a", "/outside/notes.txt");
			const tabs = kind === "markdown" ? mdTabsStore : editorTabsStore;
			if (kind === "editor") editorTabsStore.setPinned(id, true, true);
			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			repositoriesStore.setActive("/repo-b");
			expect(tabs.state.activeId).toBe(id);
			expect(container.querySelector(`[data-tab-id="${id}"]`)).not.toBeNull();
			tabs.setPinned(id, false);
			expect(tabs.state.activeId).toBeNull();
			expect(container.querySelector(`[data-tab-id="${id}"]`)).toBeNull();
		});
		it("hides an active MCP tab on repo switch and restores it on return", () => {
			for (const path of ["/repo-a", "/repo-b"]) {
				repositoriesStore.add({ path, displayName: path });
				repositoriesStore.setWorkspace(path, "main", { worktreePath: path });
				repositoriesStore.setActiveWorkspace(path, "main");
			}
			repositoriesStore.setActive("/repo-a");
			const id = mdTabsStore.openUiTab("repo-preview", "Preview", "<p>test</p>", false);
			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			expect(container.querySelector(`[data-tab-id="${id}"]`)).not.toBeNull();
			repositoriesStore.setActive("/repo-b");
			expect(mdTabsStore.state.activeId).toBeNull();
			expect(container.querySelector(`[data-tab-id="${id}"]`)).toBeNull();
			repositoriesStore.setActive("/repo-a");
			expect(mdTabsStore.get(id)).toBeDefined();
			expect(container.querySelector(`[data-tab-id="${id}"]`)).not.toBeNull();
		});
		function setupActiveRepo() {
			repositoriesStore.add({ path: "/repo", displayName: "repo" });
			repositoriesStore.setActive("/repo");
			repositoriesStore.setActiveWorkspace("/repo", "main");
		}

		it("renders markdown tabs", () => {
			setupActiveRepo();
			mdTabsStore.add("/repo", "/repo/readme.md");

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			const mdTabs = container.querySelectorAll(".mdTab");
			expect(mdTabs.length).toBe(1);
			expect(mdTabs[0].querySelector(".tabName")!.textContent).toBe("readme.md");
		});

		it("clicking md tab selects it", () => {
			setupActiveRepo();
			const id = mdTabsStore.add("/repo", "/repo/readme.md");
			const handleSelect = vi.fn();

			const { container } = render(() => (
				<TabBar
					onTabSelect={handleSelect}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			fireEvent.click(container.querySelector(".mdTab")!);
			expect(handleSelect).toHaveBeenCalledWith(id);
			expect(mdTabsStore.state.activeId).toBe(id);
		});

		it("closing md tab via close button", () => {
			setupActiveRepo();
			const id = mdTabsStore.add("/repo", "/repo/readme.md");
			const handleClose = vi.fn();

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={handleClose}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			const closeBtn = container.querySelector(".mdTab .tabClose")!;
			fireEvent.click(closeBtn);
			expect(handleClose).toHaveBeenCalledWith(id);
		});
	});

	describe("cross-kind drag reorder", () => {
		/** Set up a repo with one terminal plus one diff and one markdown tab. */
		function setupMixedTabs() {
			repositoriesStore.add({ path: "/repo", displayName: "repo" });
			repositoriesStore.setWorkspace("/repo", "main", { isMain: true, worktreePath: null });
			repositoriesStore.setActive("/repo");
			repositoriesStore.setActiveWorkspace("/repo", "main");
			const terminalId = addTerminal({ name: "Terminal" });
			repositoriesStore.addTerminalToWorkspace("/repo", "main", terminalId);
			const diffId = diffTabsStore.add("/repo", "/repo/change.ts", "M");
			const markdownId = mdTabsStore.add("/repo", "/repo/readme.md");
			return { terminalId, diffId, markdownId };
		}

		/** Drag `sourceEl` onto `targetEl` and release, dropping on the given half. */
		function dragOnto(sourceEl: Element, targetEl: Element, side: "left" | "right") {
			vi.spyOn(targetEl, "getBoundingClientRect").mockReturnValue({
				left: 100,
				right: 200,
				top: 0,
				bottom: 30,
				width: 100,
				height: 30,
				x: 100,
				y: 0,
				toJSON: () => ({}),
			} as DOMRect);
			vi.spyOn(document, "elementFromPoint").mockReturnValue(targetEl);
			const dropX = side === "left" ? 110 : 190;

			fireEvent.pointerDown(sourceEl, { button: 0, pointerId: 1, clientX: 10, clientY: 10 });
			fireEvent.pointerMove(document, { pointerId: 1, clientX: dropX, clientY: 10 });
			fireEvent.pointerUp(document, { pointerId: 1, clientX: dropX, clientY: 10 });
		}

		function renderedTabIds(container: HTMLElement): string[] {
			return [...container.querySelectorAll("[data-tab-id]")].map((el) => (el as HTMLElement).dataset.tabId!);
		}

		it("free mode: dropping a diff on a terminal moves it before the terminal", () => {
			const { terminalId, diffId, markdownId } = setupMixedTabs();
			settingsStore.setTabOrderingMode("free");

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			expect(renderedTabIds(container)).toEqual([terminalId, diffId, markdownId]);

			const source = container.querySelector(`[data-tab-id="${diffId}"]`)!;
			const target = container.querySelector(`[data-tab-id="${terminalId}"]`)!;
			dragOnto(source, target, "left");

			expect(renderedTabIds(container)).toEqual([diffId, terminalId, markdownId]);
		});

		it("free mode: dropping a terminal on a markdown tab moves it after the markdown tab", () => {
			const { terminalId, diffId, markdownId } = setupMixedTabs();
			settingsStore.setTabOrderingMode("free");

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));

			const source = container.querySelector(`[data-tab-id="${terminalId}"]`)!;
			const target = container.querySelector(`[data-tab-id="${markdownId}"]`)!;
			dragOnto(source, target, "right");

			expect(renderedTabIds(container)).toEqual([diffId, markdownId, terminalId]);
		});

		it("terminals-first mode: dropping a markdown tab on a diff reorders the non-terminal tabs", () => {
			const { terminalId, diffId, markdownId } = setupMixedTabs();
			settingsStore.setTabOrderingMode("terminals-first");

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			expect(renderedTabIds(container)).toEqual([terminalId, diffId, markdownId]);

			const source = container.querySelector(`[data-tab-id="${markdownId}"]`)!;
			const target = container.querySelector(`[data-tab-id="${diffId}"]`)!;
			dragOnto(source, target, "left");

			expect(renderedTabIds(container)).toEqual([terminalId, markdownId, diffId]);
		});

		it("grouped mode: a same-kind drop still reorders within the kind", () => {
			const { terminalId } = setupMixedTabs();
			const secondDiffId = diffTabsStore.add("/repo", "/repo/other.ts", "M");
			settingsStore.setTabOrderingMode("grouped-by-type");

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			const ids = renderedTabIds(container);
			const firstDiffId = ids[1];

			const source = container.querySelector(`[data-tab-id="${secondDiffId}"]`)!;
			const target = container.querySelector(`[data-tab-id="${firstDiffId}"]`)!;
			dragOnto(source, target, "left");

			expect(renderedTabIds(container).slice(0, 3)).toEqual([terminalId, secondDiffId, firstDiffId]);
		});

		it("terminals-first mode: dropping a terminal on a terminal still calls onReorder", () => {
			setupMixedTabs();
			const secondTerminalId = addTerminal({ name: "Terminal 2" });
			repositoriesStore.addTerminalToWorkspace("/repo", "main", secondTerminalId);
			settingsStore.setTabOrderingMode("terminals-first");
			const onReorder = vi.fn();

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
					onReorder={onReorder}
				/>
			));
			const ids = renderedTabIds(container);

			const source = container.querySelector(`[data-tab-id="${secondTerminalId}"]`)!;
			const target = container.querySelector(`[data-tab-id="${ids[0]}"]`)!;
			dragOnto(source, target, "left");

			expect(onReorder).toHaveBeenCalledWith(1, 0);
		});

		it("closing a tab drops it from the cross-kind order", () => {
			const { terminalId, diffId, markdownId } = setupMixedTabs();
			settingsStore.setTabOrderingMode("free");

			mdTabsStore.remove(markdownId);

			expect(tabOrderingStore.getOrdered(new Set([terminalId, diffId]))).toEqual([terminalId, diffId]);
			expect(tabOrderingStore.state.order).not.toContain(markdownId);
		});
	});

	describe("tab-kind parity across ordering modes", () => {
		it.each(["grouped-by-type", "free"] satisfies TabOrderingMode[])(
			"renders and selects every tab kind in %s mode",
			(mode) => {
				repositoriesStore.add({ path: "/repo", displayName: "repo" });
				repositoriesStore.setWorkspace("/repo", "main", { isMain: true, worktreePath: null });
				repositoriesStore.setActive("/repo");
				repositoriesStore.setActiveWorkspace("/repo", "main");

				const terminalId = addTerminal({ name: "Terminal parity" });
				repositoriesStore.addTerminalToWorkspace("/repo", "main", terminalId);
				const diffId = diffTabsStore.add("/repo", "/repo/change.ts", "M");
				const markdownId = mdTabsStore.add("/repo", "/repo/readme.md");
				const editorId = editorTabsStore.add("/repo", "/repo/edit.ts");
				settingsStore.setTabOrderingMode(mode);

				const onTabSelect = vi.fn();
				const { container } = render(() => (
					<TabBar
						onTabSelect={onTabSelect}
						onTabClose={() => {}}
						onCloseOthers={() => {}}
						onCloseToRight={() => {}}
						onNewTab={() => {}}
					/>
				));

				for (const [id, label] of [
					[terminalId, "Terminal parity"],
					[diffId, "change.ts"],
					[markdownId, "readme.md"],
					[editorId, "edit.ts"],
				] as const) {
					const tab = container.querySelector(`[data-tab-id="${id}"]`);
					expect(tab, `${mode}:${id}`).not.toBeNull();
					expect(tab!.querySelector(".tabName")?.textContent).toContain(label);
					fireEvent.click(tab!);
					expect(onTabSelect).toHaveBeenLastCalledWith(id);
				}
			},
		);
	});

	describe("tab rename", () => {
		it("double-click enters edit mode", () => {
			const id = addTerminal({ name: "My Tab" });
			terminalsStore.setActive(id);

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			const tab = container.querySelector(".tab")!;
			fireEvent.dblClick(tab);
			const input = tab.querySelector(".tabNameInput");
			expect(input).not.toBeNull();
		});

		it("Enter key commits rename", () => {
			const id = addTerminal({ name: "Old Name" });
			terminalsStore.setActive(id);

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			const tab = container.querySelector(".tab")!;
			fireEvent.dblClick(tab);
			const input = tab.querySelector(".tabNameInput") as HTMLInputElement;
			fireEvent.input(input, { target: { value: "New Name" } });
			fireEvent.keyDown(input, { key: "Enter" });
			expect(terminalsStore.get(id)?.name).toBe("New Name");
		});

		it("Escape key cancels rename", () => {
			const id = addTerminal({ name: "Original" });
			terminalsStore.setActive(id);

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			const tab = container.querySelector(".tab")!;
			fireEvent.dblClick(tab);
			const input = tab.querySelector(".tabNameInput") as HTMLInputElement;
			fireEvent.input(input, { target: { value: "Changed" } });
			fireEvent.keyDown(input, { key: "Escape" });
			// Name should remain original (Escape cancels)
			expect(terminalsStore.get(id)?.name).toBe("Original");
		});
	});

	// The tab stores keep `filePath` RELATIVE to the tab's filesystem root. Copy Path
	// used to hand the bare `filePath` to the clipboard, so a tab yielded `src/a.ts`
	// while the File Browser yielded the full path for the same file — useless
	// anywhere the consumer's cwd is not the repo root.
	describe("Copy Path copies the absolute path", () => {
		function setupRepo() {
			repositoriesStore.add({ path: "/repo", displayName: "repo" });
			repositoriesStore.setWorkspace("/repo", "main", { isMain: true, worktreePath: null });
			repositoriesStore.setActive("/repo");
			repositoriesStore.setActiveWorkspace("/repo", "main");
		}

		function clickCopyPath(container: HTMLElement, tabSelector: string) {
			fireEvent.contextMenu(container.querySelector(tabSelector)!);
			const item = Array.from(container.querySelectorAll(".menu .item")).find(
				(i) => i.querySelector(".label")?.textContent === "Copy Path",
			)!;
			fireEvent.click(item);
		}

		const renderTabBar = () =>
			render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));

		beforeEach(() => {
			mockCopyPathToClipboard.mockClear();
			mockOpenLocalPath.mockClear();
			mockHandleOpenUrl.mockClear();
			setupRepo();
		});

		it("joins a diff tab's relative path onto its repo root", () => {
			diffTabsStore.add("/repo", "src/file.ts", "M");
			const { container } = renderTabBar();
			clickCopyPath(container, ".diffTab");
			expect(mockCopyPathToClipboard).toHaveBeenCalledWith("/repo/src/file.ts");
		});

		it("joins a markdown tab's relative path onto its worktree root", () => {
			mdTabsStore.add("/repo", "docs/readme.md", "/wt/feature");
			const { container } = renderTabBar();
			clickCopyPath(container, ".mdTab");
			expect(mockCopyPathToClipboard).toHaveBeenCalledWith("/wt/feature/docs/readme.md");
		});

		it("joins an editor tab's relative path onto its fs root", () => {
			editorTabsStore.add("/repo", "src/main.rs", undefined, { fsRoot: "/wt/feature" });
			const { container } = renderTabBar();
			clickCopyPath(container, ".editTab");
			expect(mockCopyPathToClipboard).toHaveBeenCalledWith("/wt/feature/src/main.rs");
		});

		// `ui action=tab url=file://…` renders through srcdoc because the iframe
		// sandbox blocks file://, so the panel looks memory-only. It is not — the
		// url is the only record of where the document came from.
		it("recovers the path of a file:// panel tab", () => {
			mdTabsStore.openUiTab("bench", "Dev benchmark 90d", "", false, "file:///tmp/report%20one.html", false);
			const { container } = renderTabBar();
			clickCopyPath(container, ".panelTab");
			expect(mockCopyPathToClipboard).toHaveBeenCalledWith("/tmp/report one.html");
		});

		// `handleOpenUrl` allows http/https/mailto only — terminal output is
		// untrusted — so a file:// panel must go to the OS opener instead, or the
		// menu item does nothing but log "Blocked URL with disallowed scheme".
		it("sends a file:// panel to the OS opener, not the URL allowlist", () => {
			mdTabsStore.openUiTab("bench2", "Report", "", false, "file:///tmp/report.html", false);
			const { container } = renderTabBar();
			fireEvent.contextMenu(container.querySelector(".panelTab")!);
			const item = Array.from(container.querySelectorAll(".menu .item")).find(
				(i) => i.querySelector(".label")?.textContent === "Open in Browser",
			)!;
			fireEvent.click(item);
			expect(mockOpenLocalPath).toHaveBeenCalledWith("/tmp/report.html");
			expect(mockHandleOpenUrl).not.toHaveBeenCalled();
		});

		it("keeps an http panel on the URL allowlist", () => {
			mdTabsStore.openUiTab("web", "Dashboard", "", false, "http://127.0.0.1:14319", false);
			const { container } = renderTabBar();
			fireEvent.contextMenu(container.querySelector(".panelTab")!);
			const item = Array.from(container.querySelectorAll(".menu .item")).find(
				(i) => i.querySelector(".label")?.textContent === "Open in Browser",
			)!;
			fireEvent.click(item);
			expect(mockHandleOpenUrl).toHaveBeenCalledWith("http://127.0.0.1:14319");
			expect(mockOpenLocalPath).not.toHaveBeenCalled();
		});

		it("offers no Copy Path for a panel tab with inline HTML only", () => {
			mdTabsStore.openUiTab("inline", "Inline panel", "<p>hi</p>", false, undefined, false);
			const { container } = renderTabBar();
			fireEvent.contextMenu(container.querySelector(".panelTab")!);
			const labels = Array.from(container.querySelectorAll(".menu .item .label")).map((l) => l.textContent);
			expect(labels).not.toContain("Copy Path");
		});

		it("leaves an already-absolute path from an external file alone", () => {
			editorTabsStore.add("/repo", "/etc/hosts");
			const { container } = renderTabBar();
			clickCopyPath(container, ".editTab");
			expect(mockCopyPathToClipboard).toHaveBeenCalledWith("/etc/hosts");
		});
	});

	describe("alias context menu item", () => {
		const renderTabBar = () =>
			render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));

		function aliasItem(container: HTMLElement): Element | undefined {
			fireEvent.contextMenu(container.querySelector(".tab")!);
			return Array.from(container.querySelectorAll(".menu .item")).find((item) =>
				item.querySelector(".label")?.textContent?.startsWith("Alias:"),
			);
		}

		beforeEach(() => {
			mockWriteClipboard.mockClear();
		});

		// The alias is the address another agent has to be told, so it has to be
		// readable and copyable from the tab that owns it — otherwise it is only
		// ever visible inside an MCP payload.
		it("shows the alias and copies it to the clipboard", () => {
			const id = addTerminal({ name: "Tab 1" });
			terminalsStore.update(id, { alias: "tu-2" });

			const { container } = renderTabBar();
			const item = aliasItem(container);

			expect(item?.querySelector(".label")?.textContent).toBe("Alias: tu-2");
			fireEvent.click(item!);
			expect(mockWriteClipboard).toHaveBeenCalledWith("tu-2");
		});

		it("is absent while the session has no alias yet", () => {
			addTerminal({ name: "Tab 1" });
			const { container } = renderTabBar();
			expect(aliasItem(container)).toBeUndefined();
		});
	});

	describe("context menu", () => {
		// Catches: copying identifiers for the active tab instead of the clicked tab.
		it("copies all identifiers for the right-clicked terminal", async () => {
			addTerminal({ name: "First", sessionId: "first-session" });
			const second = addTerminal({ name: "Second", sessionId: "second-session" });
			const addToast = vi.spyOn(toastsStore, "add").mockImplementation(() => 0);
			vi.mocked(invoke).mockImplementation(async (command) =>
				command === "list_active_sessions"
					? [
							{ session_id: "first-session", child_pid: 11, alias: "one" },
							{ session_id: "second-session", child_pid: 22, alias: "two" },
						]
					: undefined,
			);
			const view = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			for (const [label, value] of [
				["Copy session ID", "second-session"],
				["Copy process ID", "22"],
				["Copy alias", "two"],
			]) {
				fireEvent.contextMenu(view.container.querySelector(`[data-tab-id="${second}"]`)!);
				for (let n = 0; n < 5; n++) await Promise.resolve();
				fireEvent.click(view.getByText(label));
				for (let n = 0; n < 5; n++) await Promise.resolve();
				expect(mockWriteClipboard).toHaveBeenCalledWith(value);
			}
			addToast.mockRestore();
		});

		it("focuses the detached Markdown window instead of selecting a second copy", () => {
			const id = mdTabsStore.add("/repo", "notes.md");
			uiStore.setDetached(`markdown-tab-${id}`, `panel-markdown-tab-${id}`);
			const onSelect = vi.fn();
			const onFocusDetached = vi.fn();
			const { container } = render(() => (
				<TabBar
					onTabSelect={onSelect}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
					onFocusDetachedTab={onFocusDetached}
				/>
			));
			fireEvent.click(container.querySelector(`[data-tab-id="${id}"]`)!);
			expect(onFocusDetached).toHaveBeenCalledExactlyOnceWith(id);
			expect(onSelect).not.toHaveBeenCalled();
		});

		it("offers Detach to Window for a tuic://open Markdown document outside registered repos", () => {
			const id = mdTabsStore.addMcpFile("digest-1", "", "/Users/boss/Gits/.tmp/report.md", false, false);
			const onDetachTab = vi.fn();
			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
					onDetachTab={onDetachTab}
				/>
			));
			fireEvent.contextMenu(container.querySelector(`[data-tab-id="${id}"]`)!);
			const item = Array.from(container.querySelectorAll(".menu .item")).find(
				(node) => node.querySelector(".label")?.textContent === "Detach to Window",
			);
			expect(item).toBeDefined();
			fireEvent.click(item!);
			expect(onDetachTab).toHaveBeenCalledExactlyOnceWith(id);
		});

		it("right-click opens context menu", () => {
			addTerminal({ name: "Tab 1" });

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			const tab = container.querySelector(".tab")!;
			fireEvent.contextMenu(tab);
			const menu = container.querySelector(".menu");
			expect(menu).not.toBeNull();
		});
	});

	describe("design mode", () => {
		function agentTab(sessionId: string, agentType: "claude" | null) {
			const id = addTerminal({ name: sessionId, sessionId });
			terminalsStore.update(id, { agentType });
			return id;
		}

		const renderBar = () =>
			render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));

		it("offers design mode only for an agent session and starts the clicked tab", async () => {
			const first = agentTab("first-session", "claude");
			const second = agentTab("second-session", "claude");
			terminalsStore.setActive(first);
			const { container } = renderBar();
			fireEvent.contextMenu(container.querySelector(`[data-tab-id="${second}"]`)!);
			const item = Array.from(container.querySelectorAll(".menu .item")).find((node) =>
				node.textContent?.includes("Start Design Mode"),
			);
			expect(item).toBeDefined();
			expect(container.textContent).not.toContain("Choose Design Mode browser");
			fireEvent.click(item!);
			const browser = Array.from(container.querySelectorAll(".item")).find(
				(node) => node.textContent?.trim() === "Edge",
			);
			expect(browser).toBeDefined();
			fireEvent.click(browser!);
			await Promise.resolve();
			expect(invoke).toHaveBeenCalledWith("start_design_mode", { sessionId: "second-session", browser: "edge" });
		});

		it("removes the Design Mode badge when the bound session stops (regression: stopped badge remains)", async () => {
			const shell = agentTab("shell-session", null);
			const agent = agentTab("agent-session", "claude");
			const { container } = renderBar();
			fireEvent.contextMenu(container.querySelector(`[data-tab-id="${shell}"]`)!);
			expect(container.textContent).not.toContain("Start Design Mode");
			await Promise.resolve();
			const eventHandler = vi.mocked(listen).mock.calls.find(([name]) => name === "design-mode-changed")?.[1];
			expect(eventHandler).toBeDefined();
			eventHandler?.({ payload: { repo_path: "/repo", session_id: "agent-session", status: "armed" } });
			expect(container.querySelector(`[data-tab-id="${agent}"] .designModeBadge`)?.textContent).toBe("D");
			expect(container.querySelector(`[data-tab-id="${agent}"] .designModeBadge`)?.getAttribute("title")).toBe(
				"Design Mode armed",
			);
			expect(container.querySelector(`[data-tab-id="${shell}"] .designModeBadge`)).toBeNull();
			fireEvent.contextMenu(container.querySelector(`[data-tab-id="${agent}"]`)!);
			const stopItem = Array.from(container.querySelectorAll(".menu .item")).find((node) =>
				node.textContent?.includes("Stop Design Mode"),
			);
			expect(stopItem).toBeDefined();
			fireEvent.click(stopItem!);
			expect(invoke).toHaveBeenCalledWith("stop_design_mode", { repoPath: "/repo" });
			eventHandler?.({ payload: { repo_path: "/repo", session_id: "agent-session", status: "stopped" } });
			expect(container.querySelector(`[data-tab-id="${agent}"] .designModeBadge`)).toBeNull();
		});

		it("restores design mode status after a webview reload", async () => {
			const id = agentTab("agent-session", "claude");
			vi.mocked(invoke).mockImplementation((command) =>
				Promise.resolve(
					command === "get_design_mode_status"
						? [{ repoPath: "/repo", sessionId: "agent-session", status: "armed" }]
						: undefined,
				),
			);
			const { container } = renderBar();
			await Promise.resolve();
			await Promise.resolve();
			expect(invoke).toHaveBeenCalledWith("get_design_mode_status");
			expect(container.querySelector(`[data-tab-id="${id}"] .designModeBadge`)?.getAttribute("title")).toBe(
				"Design Mode armed",
			);
			fireEvent.contextMenu(container.querySelector(`[data-tab-id="${id}"]`)!);
			expect(container.textContent).toContain("Stop Design Mode");
		});

		it("keeps a newer design mode event when the mount snapshot arrives late", async () => {
			const id = agentTab("agent-session", "claude");
			let resolveSnapshot!: (value: unknown) => void;
			const snapshot = new Promise<unknown>((resolve) => {
				resolveSnapshot = resolve;
			});
			vi.mocked(invoke).mockImplementation((command) =>
				command === "get_design_mode_status" ? snapshot : Promise.resolve(undefined),
			);
			const { container } = renderBar();
			await Promise.resolve();
			expect(invoke).toHaveBeenCalledWith("get_design_mode_status");
			const eventHandler = vi.mocked(listen).mock.calls.find(([name]) => name === "design-mode-changed")?.[1];
			eventHandler?.({ payload: { repo_path: "/repo", session_id: "agent-session", status: "stopped" } });
			resolveSnapshot([{ repoPath: "/repo", sessionId: "agent-session", status: "armed" }]);
			await Promise.resolve();
			expect(container.querySelector(`[data-tab-id="${id}"] .designModeBadge`)).toBeNull();
		});

		it("shows the backend message when starting design mode fails", async () => {
			const id = agentTab("agent-session", "claude");
			vi.mocked(invoke).mockImplementation((command) =>
				command === "start_design_mode"
					? Promise.reject(new Error("Chrome is unavailable"))
					: Promise.resolve(undefined),
			);
			const addToast = vi.spyOn(toastsStore, "add");
			const { container } = renderBar();
			fireEvent.contextMenu(container.querySelector(`[data-tab-id="${id}"]`)!);
			const item = Array.from(container.querySelectorAll(".menu .item")).find((node) =>
				node.textContent?.includes("Start Design Mode"),
			);
			expect(container.textContent).not.toContain("Choose Design Mode browser");
			fireEvent.click(item!);
			const browser = Array.from(container.querySelectorAll(".item")).find(
				(node) => node.textContent?.trim() === "Edge",
			);
			expect(browser).toBeDefined();
			fireEvent.click(browser!);
			await Promise.resolve();
			expect(addToast).toHaveBeenCalledWith(expect.any(String), "Chrome is unavailable", "error");
			addToast.mockRestore();
		});

		it("explains that browser design mode opens Chrome on the host", async () => {
			for (const toast of [...toastsStore.toasts]) toastsStore.remove(toast.id);
			const tauri = vi.spyOn(transport, "isTauri").mockReturnValue(false);
			const id = agentTab("agent-session", "claude");
			const notice = render(() => <ToastList onDismiss={() => {}} repoName={() => null} />);
			const { container } = renderBar();
			fireEvent.contextMenu(container.querySelector(`[data-tab-id="${id}"]`)!);
			const item = Array.from(container.querySelectorAll(".menu .item")).find((node) =>
				node.textContent?.includes("Start Design Mode"),
			);
			expect(container.textContent).not.toContain("Choose Design Mode browser");
			fireEvent.click(item!);
			const browser = Array.from(container.querySelectorAll(".item")).find(
				(node) => node.textContent?.trim() === "Edge",
			);
			expect(browser).toBeDefined();
			fireEvent.click(browser!);
			await Promise.resolve();
			expect(notice.getByText("Browser opened on the host machine.")).toBeVisible();
			expect(notice.getByText("Design Mode")).toBeVisible();
			tauri.mockRestore();
		});
	});

	describe("tab close", () => {
		it("close button removes the terminal", () => {
			const id1 = addTerminal({ name: "T1" });
			addTerminal({ name: "T2" });
			terminalsStore.setActive(id1);

			const handleClose = vi.fn();

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={handleClose}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));

			const closeBtn = container.querySelector(".tabClose")!;
			fireEvent.click(closeBtn);

			expect(handleClose).toHaveBeenCalledWith(id1);
		});
	});

	describe("quick switcher badges", () => {
		function renderLongPressHints() {
			for (let i = 1; i <= 10; i++) addTerminal({ name: `Tab ${i}` });
			return render(() => {
				const [quick, setQuick] = createSignal(false);
				const [hints, setHints] = createSignal(false);
				useQuickSwitcherVisibility(setQuick, setHints);
				return (
					<TabBar
						tabHintsActive={hints()}
						quickSwitcherActive={quick()}
						onTabSelect={() => {}}
						onTabClose={() => {}}
						onCloseOthers={() => {}}
						onCloseToRight={() => {}}
						onNewTab={() => {}}
					/>
				);
			});
		}

		function primaryDown() {
			fireEvent.keyDown(document, { key: isMacOS() ? "Meta" : "Control", metaKey: isMacOS(), ctrlKey: !isMacOS() });
		}

		it.each(["MacIntel", "Win32", "Linux x86_64"])(
			"long press reveals only nine tab shortcuts and release hides them on %s",
			(platform) => {
				const original = navigator.platform;
				Object.defineProperty(navigator, "platform", { configurable: true, value: platform });
				resetPlatformCache();
				const view = renderLongPressHints();
				try {
					primaryDown();
					vi.advanceTimersByTime(449);
					expect(view.container.querySelectorAll(".shortcutBadge")).toHaveLength(0);
					vi.advanceTimersByTime(1);
					const badges = view.container.querySelectorAll(".shortcutBadge");
					expect([...badges].map((b) => b.textContent)).toEqual(
						Array.from({ length: 9 }, (_, i) => keyFor(`switch-tab-${i + 1}`)),
					);
					fireEvent.keyUp(document, { key: isMacOS() ? "Meta" : "Control" });
					expect(view.container.querySelectorAll(".shortcutBadge")).toHaveLength(0);
				} finally {
					view.unmount();
					Object.defineProperty(navigator, "platform", { configurable: true, value: original });
					resetPlatformCache();
				}
			},
		);

		it.each(["c", "t", "1", "mousedown", "wheel"])(
			"%s cancels pending tab hints without consuming the event or rearming on repeat",
			(action) => {
				const { container } = renderLongPressHints();
				primaryDown();
				vi.advanceTimersByTime(200);
				const event =
					action === "mousedown"
						? new MouseEvent(action, { bubbles: true, cancelable: true })
						: action === "wheel"
							? new WheelEvent(action, { bubbles: true, cancelable: true })
							: new KeyboardEvent("keydown", {
									key: action,
									metaKey: isMacOS(),
									ctrlKey: !isMacOS(),
									bubbles: true,
									cancelable: true,
								});
				document.dispatchEvent(event);
				expect(event.defaultPrevented).toBe(false);
				fireEvent.keyDown(document, {
					key: isMacOS() ? "Meta" : "Control",
					metaKey: isMacOS(),
					ctrlKey: !isMacOS(),
					repeat: true,
				});
				vi.advanceTimersByTime(1000);
				expect(container.querySelectorAll(".shortcutBadge")).toHaveLength(0);
			},
		);

		it.each(["mousedown", "wheel", "blur", "visibilitychange", "keydown"])(
			"%s retracts visible long-press badges",
			(action) => {
				const { container } = renderLongPressHints();
				primaryDown();
				vi.advanceTimersByTime(450);
				expect(container.querySelectorAll(".shortcutBadge")).toHaveLength(9);
				(action === "blur" ? window : document).dispatchEvent(
					action === "keydown" ? new KeyboardEvent(action, { key: "c" }) : new Event(action),
				);
				expect(container.querySelectorAll(".shortcutBadge")).toHaveLength(0);
			},
		);

		it("quick-switcher chord stays immediate and cancels the primary hold", () => {
			const { container } = renderLongPressHints();
			primaryDown();
			fireEvent.keyDown(document, {
				key: isMacOS() ? "Control" : "Alt",
				metaKey: isMacOS(),
				ctrlKey: true,
				altKey: !isMacOS(),
			});
			expect(container.querySelector(".shortcutBadge")?.textContent).toBe(keyFor("switch-branch-1"));
			vi.advanceTimersByTime(1000);
			expect(container.querySelector(".shortcutBadge")?.textContent).toBe(keyFor("switch-branch-1"));
			fireEvent.keyUp(document, { key: isMacOS() ? "Control" : "Alt" });
			expect(container.querySelectorAll(".shortcutBadge")).toHaveLength(0);
		});

		it("unmount cancels the pending timer and listeners", () => {
			const { unmount } = renderLongPressHints();
			primaryDown();
			const clear = vi.spyOn(globalThis, "clearTimeout");
			unmount();
			expect(clear).toHaveBeenCalled();
			vi.advanceTimersByTime(1000);
			clear.mockRestore();
		});

		it("shows shortcut badges when quickSwitcherActive", () => {
			addTerminal({ name: "Tab 1" });
			addTerminal({ name: "Tab 2" });

			const { container } = render(() => (
				<TabBar
					quickSwitcherActive={true}
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			const badges = container.querySelectorAll(".shortcutBadge");
			expect(badges.length).toBe(2);
			// The badge names the chord that is held (Cmd+Ctrl+N), not plain Cmd+N.
			expect(badges[0].textContent).toBe(keyFor("switch-branch-1"));
		});
	});

	describe("middle-click to close", () => {
		it("middle-click on terminal tab calls onTabClose", () => {
			const handleClose = vi.fn();
			const handleSelect = vi.fn();
			const id1 = addTerminal({ name: "Tab 1" });

			const { container } = render(() => (
				<TabBar
					onTabSelect={handleSelect}
					onTabClose={handleClose}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			const tab = container.querySelector(".tab")!;
			fireEvent(tab, new MouseEvent("auxclick", { button: 1, bubbles: true }));
			expect(handleClose).toHaveBeenCalledWith(id1, true);
			expect(handleSelect).not.toHaveBeenCalled();
		});

		it("right-click auxclick does not close tab", () => {
			const handleClose = vi.fn();
			addTerminal({ name: "Tab 1" });

			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={handleClose}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
				/>
			));
			const tab = container.querySelector(".tab")!;
			fireEvent(tab, new MouseEvent("auxclick", { button: 2, bubbles: true }));
			expect(handleClose).not.toHaveBeenCalled();
		});
	});

	describe("move to worktree context menu", () => {
		function setupRepoWithWorktrees() {
			// Create a repo with main branch and a worktree branch
			repositoriesStore.add({ path: "/repo", displayName: "repo" });
			repositoriesStore.setWorkspace("/repo", "main", { isMain: true, worktreePath: null });
			repositoriesStore.setWorkspace("/repo", "feature-a", { isMain: false, worktreePath: "/repo-wt/feature-a" });
			repositoriesStore.setActive("/repo");
			repositoriesStore.setActiveWorkspace("/repo", "main");
		}

		it("shows Move to Worktree submenu when repo has multiple worktrees", () => {
			setupRepoWithWorktrees();
			const termId = addTerminal({ name: "T1", sessionId: "sess-1" });
			repositoriesStore.addTerminalToWorkspace("/repo", "main", termId);
			terminalsStore.setActive(termId);

			const getTargets = () => [{ branchName: "feature-a", path: "/repo-wt/feature-a" }];
			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
					getWorktreeTargets={getTargets}
				/>
			));
			const tab = container.querySelector(".tab")!;
			fireEvent.contextMenu(tab);

			const menuItems = container.querySelectorAll(".menu .item");
			const moveItem = Array.from(menuItems).find((i) => i.textContent?.includes("Move to Worktree"));
			expect(moveItem).not.toBeNull();
		});

		it("hides Move to Worktree when no worktree targets available", () => {
			repositoriesStore.add({ path: "/repo", displayName: "repo" });
			repositoriesStore.setWorkspace("/repo", "main", { isMain: true, worktreePath: null });
			repositoriesStore.setActive("/repo");
			repositoriesStore.setActiveWorkspace("/repo", "main");
			const termId = addTerminal({ name: "T1", sessionId: "sess-1" });
			repositoriesStore.addTerminalToWorkspace("/repo", "main", termId);
			terminalsStore.setActive(termId);

			const getTargets = () => [] as Array<{ branchName: string; path: string }>;
			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
					getWorktreeTargets={getTargets}
				/>
			));
			const tab = container.querySelector(".tab")!;
			fireEvent.contextMenu(tab);

			const menuItems = container.querySelectorAll(".menu .item");
			const moveItem = Array.from(menuItems).find((i) => i.textContent?.includes("Move to Worktree"));
			expect(moveItem).toBeUndefined();
		});

		it("calls onMoveToWorktree with correct args when worktree is selected", () => {
			setupRepoWithWorktrees();
			const handleMove = vi.fn();
			const termId = addTerminal({ name: "T1", sessionId: "sess-1" });
			repositoriesStore.addTerminalToWorkspace("/repo", "main", termId);
			terminalsStore.setActive(termId);

			const getTargets = () => [{ branchName: "feature-a", path: "/repo-wt/feature-a" }];
			const { container } = render(() => (
				<TabBar
					onTabSelect={() => {}}
					onTabClose={() => {}}
					onCloseOthers={() => {}}
					onCloseToRight={() => {}}
					onNewTab={() => {}}
					getWorktreeTargets={getTargets}
					onMoveToWorktree={handleMove}
				/>
			));
			const tab = container.querySelector(".tab")!;
			fireEvent.contextMenu(tab);

			// Find the "Move to Worktree" parent item and hover to open submenu
			const menuItems = container.querySelectorAll(".menu .itemWrap");
			const moveWrap = Array.from(menuItems).find((i) => i.textContent?.includes("Move to Worktree"));
			expect(moveWrap).not.toBeNull();
			fireEvent.mouseEnter(moveWrap!);

			// Find the submenu item "feature-a"
			const submenuItems = moveWrap!.querySelectorAll(".submenu .item");
			const featureItem = Array.from(submenuItems).find((i) => i.textContent?.includes("feature-a"));
			expect(featureItem).not.toBeNull();
			fireEvent.click(featureItem!);

			expect(handleMove).toHaveBeenCalledWith(termId, "/repo-wt/feature-a");
		});
	});
});
