import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";
import { useAppShortcutHandlers } from "../../hooks/useAppShortcutHandlers";
import { dispatchNativeMenuAction } from "../../hooks/useNativeMenuBridge";
import { useTerminalLifecycle } from "../../hooks/useTerminalLifecycle";
import { appLogger } from "../../stores/appLogger";
import { editorTabsStore } from "../../stores/editorTabs";
import { mdTabsStore } from "../../stores/mdTabs";
import { settingsStore } from "../../stores/settings";
import { terminalsStore } from "../../stores/terminals";
import { makeTerminal, testInScope } from "../helpers/store";

function createLifecycle() {
	return useTerminalLifecycle({
		pty: { canSpawn: vi.fn(), close: vi.fn() },
		dialogs: { confirmCloseTerminal: vi.fn(), confirmSaveChanges: vi.fn(), confirm: vi.fn() },
		setStatusInfo: vi.fn(),
		getDefaultFontSize: () => settingsStore.state.defaultFontSize,
	});
}

function createHandlers(terminalLifecycle = createLifecycle()) {
	return useAppShortcutHandlers({
		terminalLifecycle,
		gitOps: {},
		splitPanes: {},
		quickSwitcher: {},
	} as Parameters<typeof useAppShortcutHandlers>[0]);
}

function openTabs() {
	const markdown = [mdTabsStore.add("/repo", "a.md"), mdTabsStore.add("/other-repo", "b.md")];
	const editors = [editorTabsStore.add("/repo", "a.ts"), editorTabsStore.add("/other-repo", "b.ts")];
	const terminals = [
		terminalsStore.add(makeTerminal({ fontSize: 13 })),
		terminalsStore.add(makeTerminal({ fontSize: 19 })),
	];
	return { markdown, editors, terminals };
}

function sizes(tabs: ReturnType<typeof openTabs>) {
	return {
		markdown: tabs.markdown.map((id) => mdTabsStore.getFontSize(id)),
		editors: tabs.editors.map((id) => editorTabsStore.get(id)?.fontSize ?? settingsStore.state.defaultFontSize),
		terminals: tabs.terminals.map((id) => terminalsStore.get(id)?.fontSize),
	};
}

function clearTabs() {
	mdTabsStore.clearAll();
	editorTabsStore.clearAll();
	for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
}

describe("collective shortcut zoom with real tab stores", () => {
	beforeEach(() => {
		vi.spyOn(appLogger, "info").mockImplementation(() => {});
		clearTabs();
	});
	afterEach(() => {
		clearTabs();
		vi.restoreAllMocks();
	});

	it("native collective zoom does not bypass Markdown and editor stores", () => {
		testInScope(() => {
			const tabs = openTabs();
			const terminalLifecycle = createLifecycle();
			const options = {
				terminalLifecycle,
				shortcutHandlers: createHandlers(terminalLifecycle),
			} as Parameters<typeof dispatchNativeMenuAction>[1];
			const defaultSize = settingsStore.state.defaultFontSize;
			dispatchNativeMenuAction("zoom-in-all", options);
			expect(sizes(tabs)).toEqual({
				markdown: [17, 17],
				editors: [defaultSize + 2, defaultSize + 2],
				terminals: [15, 21],
			});
			dispatchNativeMenuAction("zoom-out-all", options);
			expect(sizes(tabs)).toEqual({ markdown: [15, 15], editors: [defaultSize, defaultSize], terminals: [13, 19] });
			options.shortcutHandlers.zoomInAll();
			dispatchNativeMenuAction("zoom-reset-all", options);
			expect(sizes(tabs)).toEqual({
				markdown: [15, 15],
				editors: [defaultSize, defaultSize],
				terminals: [defaultSize, defaultSize],
			});
		});
	});

	it("collective zoom updates inactive Markdown and editor tabs instead of only terminals", () => {
		testInScope(() => {
			const tabs = openTabs();
			const handlers = createHandlers();
			const defaultSize = settingsStore.state.defaultFontSize;
			handlers.zoomInAll();
			expect(sizes(tabs)).toEqual({
				markdown: [17, 17],
				editors: [defaultSize + 2, defaultSize + 2],
				terminals: [15, 21],
			});
			handlers.zoomOutAll();
			expect(sizes(tabs)).toEqual({ markdown: [15, 15], editors: [defaultSize, defaultSize], terminals: [13, 19] });
			handlers.zoomOutAll();
			expect(sizes(tabs)).toEqual({
				markdown: [13, 13],
				editors: [defaultSize - 2, defaultSize - 2],
				terminals: [11, 17],
			});
		});
	});

	it("collective reset restores each tab kind's default without requiring an active tab", () => {
		testInScope(() => {
			const tabs = openTabs();
			for (const id of tabs.markdown) mdTabsStore.setFontSize(id, 27);
			for (const id of tabs.editors) {
				editorTabsStore.setActive(id);
				editorTabsStore.zoomIn(settingsStore.state.defaultFontSize);
			}
			mdTabsStore.setActive(null);
			editorTabsStore.setActive(null);
			createHandlers().zoomResetAll();
			const defaultSize = settingsStore.state.defaultFontSize;
			expect(sizes(tabs)).toEqual({
				markdown: [15, 15],
				editors: [defaultSize, defaultSize],
				terminals: [defaultSize, defaultSize],
			});
		});
	});

	it("collective zoom clamps every tab at both existing font bounds", () => {
		testInScope(() => {
			const tabs = openTabs();
			const handlers = createHandlers();
			for (let step = 0; step < 20; step++) handlers.zoomInAll();
			expect(sizes(tabs)).toEqual({ markdown: [32, 32], editors: [32, 32], terminals: [32, 32] });
			for (let step = 0; step < 20; step++) handlers.zoomOutAll();
			expect(sizes(tabs)).toEqual({ markdown: [8, 8], editors: [8, 8], terminals: [8, 8] });
		});
	});

	it("per-tab zoom leaves inactive tabs and other tab kinds unchanged", () => {
		testInScope(() => {
			const tabs = openTabs();
			const handlers = createHandlers();
			const defaultSize = settingsStore.state.defaultFontSize;
			editorTabsStore.setActive(tabs.editors[0]);
			handlers.zoomIn();
			expect(sizes(tabs)).toEqual({ markdown: [15, 15], editors: [defaultSize + 2, defaultSize], terminals: [13, 19] });
			handlers.zoomReset();
			editorTabsStore.setActive(null);
			mdTabsStore.setActive(tabs.markdown[0]);
			handlers.zoomOut();
			expect(sizes(tabs)).toEqual({ markdown: [13, 15], editors: [defaultSize, defaultSize], terminals: [13, 19] });
			handlers.zoomReset();
			mdTabsStore.setActive(null);
			terminalsStore.setActive(tabs.terminals[0]);
			handlers.zoomIn();
			expect(sizes(tabs)).toEqual({ markdown: [15, 15], editors: [defaultSize, defaultSize], terminals: [15, 19] });
		});
	});

	it("collective zoom tolerates empty tab stores without creating tabs", () => {
		testInScope(() => {
			const handlers = createHandlers();
			handlers.zoomInAll();
			handlers.zoomOutAll();
			handlers.zoomResetAll();
			expect(mdTabsStore.getIds()).toEqual([]);
			expect(editorTabsStore.getIds()).toEqual([]);
			expect(terminalsStore.getIds()).toEqual([]);
		});
	});
});
