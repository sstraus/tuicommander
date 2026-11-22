import { render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useQuickSwitcherVisibility } from "../../hooks/useQuickSwitcherVisibility";
import "../mocks/tauri";
import * as handsFree from "../../actions/handsFreeConversation";
import { type ShortcutHandlers, useKeyboardShortcuts } from "../../hooks/useKeyboardShortcuts";
import { resetPlatformCache } from "../../platform";
import { dictationStore } from "../../stores/dictation";
import { globalWorkspaceStore } from "../../stores/globalWorkspace";
import { keybindingsStore } from "../../stores/keybindings";
import { paneLayoutStore, resetGroupCounter } from "../../stores/paneLayout";
import { progressStore } from "../../stores/progress";
import { repositoriesStore } from "../../stores/repositories";
import { settingsStore } from "../../stores/settings";
import { terminalsStore } from "../../stores/terminals";
import { makeTerminal } from "../helpers/store";
import { mockInvoke } from "../mocks/tauri";

function resetStores() {
	for (const id of terminalsStore.getIds()) {
		terminalsStore.remove(id);
	}
	paneLayoutStore.reset();
	resetGroupCounter();
}

function createMockHandlers(): ShortcutHandlers {
	return {
		zoomIn: vi.fn(),
		zoomOut: vi.fn(),
		zoomReset: vi.fn(),
		zoomInAll: vi.fn(),
		zoomOutAll: vi.fn(),
		zoomResetAll: vi.fn(),
		createNewTerminal: vi.fn(),
		closeTerminal: vi.fn(),
		reopenClosedTab: vi.fn(),
		navigateTab: vi.fn(),
		focusLastTerminal: vi.fn(),
		historyBack: vi.fn(),
		historyForward: vi.fn(),
		jumpWaitingTerminal: vi.fn(),
		clearTerminal: vi.fn(),
		terminalIds: vi.fn().mockReturnValue([]),
		handleTerminalSelect: vi.fn(),
		handleSplit: vi.fn(),
		handleRunCommand: vi.fn(),
		reloadActiveTab: vi.fn(() => true),
		switchToBranchByIndex: vi.fn(),
		isQuickSwitcherOpen: vi.fn().mockReturnValue(false),
		toggleMarkdownPanel: vi.fn(),
		toggleSidebar: vi.fn(),
		toggleSettings: vi.fn(),
		toggleTaskQueue: vi.fn(),
		toggleGitOpsPanel: vi.fn(),
		toggleHelpPanel: vi.fn(),
		toggleIdeasPanel: vi.fn(),
		toggleFileBrowserPanel: vi.fn(),
		requestFileBrowserContentSearch: vi.fn(),
		toggleOutlinePanel: vi.fn(),
		askSideQuestion: vi.fn(),
		findInTerminal: vi.fn(),
		toggleCommandPalette: vi.fn(),
		toggleActivityDashboard: vi.fn(),
		toggleWorktreeManager: vi.fn(),
		toggleBranchSwitcher: vi.fn(),
		toggleErrorLog: vi.fn(),
		toggleBranchesTab: vi.fn(),
		toggleAiChatPanel: vi.fn(),
		toggleMcpPopup: vi.fn(),
		clearScrollback: vi.fn(),
		scrollToTop: vi.fn(),
		scrollToBottom: vi.fn(),
		scrollPageUp: vi.fn(),
		scrollPageDown: vi.fn(),
		toggleZoomPane: vi.fn(),
		toggleFocusMode: vi.fn(),
		closeActivePane: vi.fn(),
		closeActiveTabOrPane: vi.fn(),
		togglePromptLibrary: vi.fn(),
		toggleDiffScroll: vi.fn(),
		toggleGlobalWorkspace: vi.fn(),
		openFile: vi.fn(),
		newFile: vi.fn(),
		openFolder: vi.fn(),
		openPath: vi.fn(),
		openSecondaryWindow: vi.fn(),
		toggleCommandOverview: vi.fn(),
		openTasks: vi.fn(),
		toggleComposePanel: vi.fn(),
		detachActivityDashboard: vi.fn(),
		toggleProcessManager: vi.fn(),
		toggleGenerators: vi.fn(),
		showRemoteQr: vi.fn(),
		refreshTerminal: vi.fn(),
		blockPrev: vi.fn(),
		blockNext: vi.fn(),
		blockFoldToggle: vi.fn(),
		blockSearchToggle: vi.fn(),
		runSmartPromptByCombo: vi.fn(() => false),
	};
}

function fireKeydown(key: string, opts: Partial<KeyboardEvent> = {}) {
	const event = new KeyboardEvent("keydown", {
		key,
		bubbles: true,
		cancelable: true,
		...opts,
	});
	document.dispatchEvent(event);
	return event;
}

describe("useKeyboardShortcuts", () => {
	let handlers: ShortcutHandlers;
	let cleanup: (() => void) | undefined;
	const originalPlatform = navigator.platform;

	beforeEach(() => {
		vi.useFakeTimers();
		Object.defineProperty(navigator, "platform", { value: "MacIntel", configurable: true });
		resetPlatformCache();
		resetStores();
		handlers = createMockHandlers();
		cleanup = useKeyboardShortcuts(handlers);
	});

	afterEach(() => {
		vi.useRealTimers();
		cleanup?.();
		Object.defineProperty(navigator, "platform", { value: originalPlatform, configurable: true });
		resetPlatformCache();
		settingsStore._testCancelPendingSave();
		paneLayoutStore._testCancelPendingSave();
	});

	// Catches: Ctrl+Alt+V being swallowed by the real quick-switcher hold on Win/Linux.
	it.each(["Win32", "Linux x86_64"])(
		"hands-free chord survives the quick-switcher modifier hold on %s",
		async (platform) => {
			Object.defineProperty(navigator, "platform", { configurable: true, value: platform });
			resetPlatformCache();
			const toggle = vi.spyOn(handsFree, "toggleHandsFreeConversation").mockResolvedValue(undefined);
			mockInvoke.mockResolvedValue({ enabled: true, hotkey: "F5", language: "auto" });
			await dictationStore.refreshConfig();
			mockInvoke.mockReset().mockResolvedValue(undefined);
			const view = render(() => {
				const [visible, setVisible] = createSignal(false);
				useQuickSwitcherVisibility(setVisible);
				handlers.isQuickSwitcherOpen = visible;
				return null;
			});
			const input = document.createElement("input");
			const terminalKeydown = vi.fn();
			input.addEventListener("keydown", terminalKeydown);
			document.body.append(input);
			try {
				fireKeydown("Control", { ctrlKey: true });
				fireKeydown("Alt", { ctrlKey: true, altKey: true });
				const event = new KeyboardEvent("keydown", {
					key: "v",
					ctrlKey: true,
					altKey: true,
					code: "KeyV",
					bubbles: true,
					cancelable: true,
				});
				input.dispatchEvent(event);
				expect(toggle).toHaveBeenCalledOnce();
				expect(event.defaultPrevented).toBe(true);
				expect(terminalKeydown).not.toHaveBeenCalled();
				fireKeydown("2", { ctrlKey: true, altKey: true });
				expect(handlers.switchToBranchByIndex).toHaveBeenCalledWith(2);
			} finally {
				input.remove();
				view.unmount();
				toggle.mockRestore();
			}
		},
	);

	describe("hands-free default hotkey", () => {
		async function configure(enabled: boolean) {
			mockInvoke.mockImplementation(async (cmd: string) =>
				cmd === "get_dictation_config" ? { enabled, hotkey: "F5", language: "auto" } : undefined,
			);
			await dictationStore.refreshConfig();
			mockInvoke.mockReset().mockResolvedValue(undefined);
		}

		it.each(["MacIntel", "Win32", "Linux x86_64"])(
			"primary+Alt+V dispatches the shared toggle on %s",
			async (platform) => {
				Object.defineProperty(navigator, "platform", { configurable: true, value: platform });
				resetPlatformCache();
				await configure(true);
				const toggle = vi.spyOn(handsFree, "toggleHandsFreeConversation").mockResolvedValue(undefined);
				try {
					const event = fireKeydown("v", {
						metaKey: platform === "MacIntel",
						ctrlKey: platform !== "MacIntel",
						altKey: true,
					});
					expect(event.defaultPrevented).toBe(true);
					expect(toggle).toHaveBeenCalledOnce();
				} finally {
					toggle.mockRestore();
					resetPlatformCache();
				}
			},
		);

		it("disabled dictation consumes the chord without invoking the toggle or showing an error", async () => {
			await configure(false);
			const toggle = vi.spyOn(handsFree, "toggleHandsFreeConversation").mockResolvedValue(undefined);
			try {
				const before = dictationStore.state.handsFreeError;
				const event = fireKeydown("v", { metaKey: true, altKey: true });
				expect(event.defaultPrevented).toBe(true);
				expect(toggle).not.toHaveBeenCalled();
				expect(mockInvoke).not.toHaveBeenCalled();
				expect(dictationStore.state.handsFreeError).toBe(before);
			} finally {
				toggle.mockRestore();
			}
		});

		it("the default chord starts the active terminal then stops the same conversation", async () => {
			await configure(true);
			terminalsStore.add(makeTerminal({ sessionId: "other-session" }));
			const active = terminalsStore.add(makeTerminal({ sessionId: "active-session" }));
			terminalsStore.setActive(active);
			const status = {
				armed: true,
				phase: "waiting",
				sessionId: "active-session",
				owner: "desktop",
				generation: 1,
				pendingText: null,
				holdBackMs: 0,
				error: null,
				deliveredTurns: 0,
				droppedTurns: 0,
			};
			mockInvoke.mockImplementation(async (cmd: string) => {
				if (cmd === "arm_hands_free_dictation") return status;
				if (cmd === "disarm_hands_free_dictation") return { status: { ...status, armed: false, sessionId: null } };
				return undefined;
			});
			fireKeydown("v", { metaKey: true, altKey: true });
			await vi.waitFor(() => expect(dictationStore.state.handsFree?.armed).toBe(true));
			expect(mockInvoke.mock.calls.filter(([cmd]) => cmd === "arm_hands_free_dictation")).toEqual([
				["arm_hands_free_dictation", { sessionId: "active-session", owner: "desktop" }],
			]);
			mockInvoke.mockClear();
			fireKeydown("v", { metaKey: true, altKey: true });
			await vi.waitFor(() => expect(dictationStore.state.handsFree?.armed).toBe(false));
			expect(mockInvoke.mock.calls.map(([cmd]) => cmd)).toContain("disarm_hands_free_dictation");
			expect(mockInvoke.mock.calls.map(([cmd]) => cmd)).not.toContain("arm_hands_free_dictation");
		});

		it("an explicit unbind overrides the new default", async () => {
			await configure(true);
			mockInvoke.mockResolvedValue([]);
			await keybindingsStore.hydrate();
			const toggle = vi.spyOn(handsFree, "toggleHandsFreeConversation").mockResolvedValue(undefined);
			try {
				await keybindingsStore.unbind("toggle-hands-free");
				fireKeydown("v", { metaKey: true, altKey: true });
				expect(toggle).not.toHaveBeenCalled();
			} finally {
				await keybindingsStore.resetAction("toggle-hands-free");
				toggle.mockRestore();
			}
		});
	});
	describe("zoom shortcuts", () => {
		it("Cmd+= zooms in", () => {
			fireKeydown("=", { metaKey: true });
			expect(handlers.zoomIn).toHaveBeenCalled();
		});

		it("Cmd++ zooms in", () => {
			fireKeydown("+", { metaKey: true });
			expect(handlers.zoomIn).toHaveBeenCalled();
		});

		it("Cmd+- zooms out", () => {
			fireKeydown("-", { metaKey: true });
			expect(handlers.zoomOut).toHaveBeenCalled();
		});

		it("Cmd+0 resets zoom", () => {
			fireKeydown("0", { metaKey: true });
			expect(handlers.zoomReset).toHaveBeenCalled();
		});
	});

	it("Cmd+Shift+P opens Progress for the active repository", () => {
		progressStore.resetForTests();
		repositoriesStore.add({ path: "/repo/progress", displayName: "Progress Repo" });
		repositoriesStore.setActive("/repo/progress");
		const event = fireKeydown("P", { metaKey: true, shiftKey: true });
		expect(event.defaultPrevented).toBe(true);
		expect(progressStore.dialogVisible()).toBe(true);
		expect(progressStore.requestedProject()).toBe("/repo/progress");
		progressStore.resetForTests();
		repositoriesStore.remove("/repo/progress");
		repositoriesStore._testCancelPendingSave();
	});

	describe("smart prompt shortcuts", () => {
		it("fires runSmartPromptByCombo for a combo with no built-in action", () => {
			(handlers.runSmartPromptByCombo as ReturnType<typeof vi.fn>).mockReturnValue(true);
			const event = fireKeydown("y", { metaKey: true, shiftKey: true });
			expect(handlers.runSmartPromptByCombo).toHaveBeenCalledWith("cmd+shift+y");
			expect(event.defaultPrevented).toBe(true);
		});

		it("does not run a smart prompt when a built-in action matches (precedence)", () => {
			fireKeydown("t", { metaKey: true });
			expect(handlers.createNewTerminal).toHaveBeenCalled();
			expect(handlers.runSmartPromptByCombo).not.toHaveBeenCalled();
		});
	});

	describe("terminal shortcuts", () => {
		it("Cmd+T creates new terminal", () => {
			fireKeydown("t", { metaKey: true });
			expect(handlers.createNewTerminal).toHaveBeenCalled();
		});

		it("Cmd+W delegates to the shared close-tab-or-pane handler", () => {
			fireKeydown("w", { metaKey: true });
			expect(handlers.closeActiveTabOrPane).toHaveBeenCalledOnce();
		});

		it("Cmd+Shift+T reopens closed tab", () => {
			fireKeydown("T", { metaKey: true, shiftKey: true });
			expect(handlers.reopenClosedTab).toHaveBeenCalled();
		});

		it("Cmd+L clears terminal", () => {
			fireKeydown("l", { metaKey: true });
			expect(handlers.clearTerminal).toHaveBeenCalled();
		});

		it("Cmd+Shift+L refreshes terminal (atlas rebuild)", () => {
			fireKeydown("l", { metaKey: true, shiftKey: true });
			expect(handlers.refreshTerminal).toHaveBeenCalled();
		});
	});

	describe("tab navigation", () => {
		it("Ctrl+Tab navigates to next tab", () => {
			fireKeydown("Tab", { ctrlKey: true });
			expect(handlers.navigateTab).toHaveBeenCalledWith("next");
		});

		it("Ctrl+Shift+Tab navigates to previous tab", () => {
			fireKeydown("Tab", { ctrlKey: true, shiftKey: true });
			expect(handlers.navigateTab).toHaveBeenCalledWith("prev");
		});

		it("Cmd+1 selects first terminal", () => {
			(handlers.terminalIds as ReturnType<typeof vi.fn>).mockReturnValue(["t1", "t2"]);
			fireKeydown("1", { metaKey: true });
			expect(handlers.handleTerminalSelect).toHaveBeenCalledWith("t1");
		});

		it("Cmd+2 selects second terminal", () => {
			(handlers.terminalIds as ReturnType<typeof vi.fn>).mockReturnValue(["t1", "t2"]);
			fireKeydown("2", { metaKey: true });
			expect(handlers.handleTerminalSelect).toHaveBeenCalledWith("t2");
		});
	});

	describe("split pane shortcuts", () => {
		it("Cmd+\\ splits vertically", () => {
			fireKeydown("\\", { metaKey: true });
			expect(handlers.handleSplit).toHaveBeenCalledWith("vertical");
		});

		it("Cmd+Alt+\\ splits horizontally", () => {
			fireKeydown("\\", { metaKey: true, altKey: true });
			expect(handlers.handleSplit).toHaveBeenCalledWith("horizontal");
		});
	});

	describe("panel toggles", () => {
		it("Cmd+Shift+D toggles git panel", () => {
			fireKeydown("D", { metaKey: true, shiftKey: true });
			expect(handlers.toggleGitOpsPanel).toHaveBeenCalled();
		});

		it("Cmd+Shift+M toggles markdown panel", () => {
			fireKeydown("m", { metaKey: true, shiftKey: true });
			expect(handlers.toggleMarkdownPanel).toHaveBeenCalled();
		});

		it("Cmd+K clears scrollback", () => {
			fireKeydown("k", { metaKey: true });
			expect(handlers.clearScrollback).toHaveBeenCalled();
		});

		it("Cmd+Shift+K toggles prompt library", () => {
			fireKeydown("k", { metaKey: true, shiftKey: true });
			expect(handlers.togglePromptLibrary).toHaveBeenCalled();
		});

		it("Cmd+, toggles settings", () => {
			fireKeydown(",", { metaKey: true });
			expect(handlers.toggleSettings).toHaveBeenCalled();
		});

		it("Cmd+J toggles task queue", () => {
			fireKeydown("j", { metaKey: true });
			expect(handlers.toggleTaskQueue).toHaveBeenCalled();
		});

		it("Cmd+[ toggles sidebar", () => {
			fireKeydown("[", { metaKey: true });
			expect(handlers.toggleSidebar).toHaveBeenCalled();
		});

		it("Cmd+? toggles help", () => {
			// "?" is a shifted key on every layout, so a real keypress carries
			// shiftKey:true and e.key:"?" — eventToCombo un-shifts it to "cmd+shift+/".
			// (Firing without shift was unrealistic and hid the dead-binding bug.)
			fireKeydown("?", { metaKey: true, shiftKey: true });
			expect(handlers.toggleHelpPanel).toHaveBeenCalled();
		});

		it("Cmd+U jumps to the next waiting terminal", () => {
			fireKeydown("u", { metaKey: true });
			expect(handlers.jumpWaitingTerminal).toHaveBeenCalled();
		});

		it("Cmd+Alt+N toggles notes panel", () => {
			fireKeydown("n", { metaKey: true, altKey: true });
			expect(handlers.toggleIdeasPanel).toHaveBeenCalled();
		});

		it("Cmd+Alt+A toggles AI chat panel (option-modified key → å)", () => {
			// On macOS, Cmd+Alt+A produces e.key="å" — combo must still resolve via e.code
			settingsStore.setExperimentalFeaturesEnabled(true);
			fireKeydown("å", { metaKey: true, altKey: true, code: "KeyA" });
			expect(handlers.toggleAiChatPanel).toHaveBeenCalled();
		});

		it("Cmd+Alt+R toggles answers-only on the active terminal (option-modified key → ®)", () => {
			// catches: the action bound but unreachable on macOS, where Option+R yields e.key="®"
			const id = terminalsStore.add(makeTerminal());
			terminalsStore.setActive(id);
			fireKeydown("®", { metaKey: true, altKey: true, code: "KeyR" });
			expect(terminalsStore.get(id)?.answersOnly).toBe(true);
			fireKeydown("®", { metaKey: true, altKey: true, code: "KeyR" });
			expect(terminalsStore.get(id)?.answersOnly).toBe(false);
		});

		it("Cmd+Alt+A does nothing when AI chat is disabled", () => {
			settingsStore.setExperimentalFeaturesEnabled(false);
			fireKeydown("å", { metaKey: true, altKey: true, code: "KeyA" });
			expect(handlers.toggleAiChatPanel).not.toHaveBeenCalled();
		});

		it("Cmd+E toggles file browser panel", () => {
			fireKeydown("e", { metaKey: true });
			expect(handlers.toggleFileBrowserPanel).toHaveBeenCalled();
		});

		it("Cmd+Shift+F opens file browser in content-search mode", () => {
			fireKeydown("F", { metaKey: true, shiftKey: true });
			expect(handlers.requestFileBrowserContentSearch).toHaveBeenCalled();
		});

		it("Cmd+Shift+D toggles git ops panel", () => {
			fireKeydown("D", { metaKey: true, shiftKey: true });
			expect(handlers.toggleGitOpsPanel).toHaveBeenCalled();
		});

		it("Cmd+Alt+Enter toggles focus mode", () => {
			fireKeydown("Enter", { metaKey: true, altKey: true });
			expect(handlers.toggleFocusMode).toHaveBeenCalled();
		});

		it("Cmd+Shift+Enter does NOT trigger focus mode (reserved for zoom-pane)", () => {
			fireKeydown("Enter", { metaKey: true, shiftKey: true });
			expect(handlers.toggleFocusMode).not.toHaveBeenCalled();
			expect(handlers.toggleZoomPane).toHaveBeenCalled();
		});
	});

	describe("run command", () => {
		it("Cmd+Shift+R runs command", () => {
			fireKeydown("R", { metaKey: true, shiftKey: true });
			expect(handlers.handleRunCommand).toHaveBeenCalledWith(false);
		});

		it("Cmd+R reloads the active tab and never runs the command", () => {
			fireKeydown("r", { metaKey: true });
			expect(handlers.reloadActiveTab).toHaveBeenCalledOnce();
			expect(handlers.handleRunCommand).not.toHaveBeenCalled();
		});

		it("Cmd+R is left to the platform when no tab can reload", () => {
			vi.mocked(handlers.reloadActiveTab).mockReturnValue(false);
			const event = fireKeydown("r", { metaKey: true });
			expect(event.defaultPrevented).toBe(false);
		});
	});

	describe("Cmd+Ctrl+N chord ownership", () => {
		const promote = (ids: string[]) => {
			for (const id of ids) {
				globalWorkspaceStore.promote(id);
			}
			globalWorkspaceStore.activate();
		};

		afterEach(() => {
			globalWorkspaceStore.deactivate();
			for (const id of globalWorkspaceStore.getPromotedIds()) globalWorkspaceStore.unpromote(id);
		});

		it("in the global workspace Cmd+Ctrl+2 selects tab 2 and keeps the workspace active", () => {
			promote(["t1", "t2", "t3"]);
			const event = fireKeydown("2", { metaKey: true, ctrlKey: true });
			expect(handlers.handleTerminalSelect).toHaveBeenCalledWith("t2");
			expect(handlers.switchToBranchByIndex).not.toHaveBeenCalled();
			expect(globalWorkspaceStore.isActive()).toBe(true);
			expect(event.defaultPrevented).toBe(true);
		});

		it("in the global workspace Cmd+Ctrl+N past the last tab does nothing", () => {
			promote(["t1", "t2", "t3"]);
			fireKeydown("5", { metaKey: true, ctrlKey: true });
			expect(handlers.handleTerminalSelect).not.toHaveBeenCalled();
			expect(handlers.switchToBranchByIndex).not.toHaveBeenCalled();
		});

		it("in the global workspace Cmd+2 selects the second workspace tab, not a branch tab", () => {
			(handlers.terminalIds as ReturnType<typeof vi.fn>).mockReturnValue(["b1", "b2"]);
			promote(["t1", "t2", "t3"]);
			fireKeydown("2", { metaKey: true });
			expect(handlers.handleTerminalSelect).toHaveBeenCalledWith("t2");
		});

		it("outside the global workspace Cmd+Ctrl+2 still switches to branch 2", () => {
			fireKeydown("2", { metaKey: true, ctrlKey: true });
			expect(handlers.switchToBranchByIndex).toHaveBeenCalledWith(2);
			expect(handlers.handleTerminalSelect).not.toHaveBeenCalled();
		});
	});

	describe("quick switcher suppression", () => {
		it("skips shortcuts when quick switcher is open", () => {
			(handlers.isQuickSwitcherOpen as ReturnType<typeof vi.fn>).mockReturnValue(true);
			fireKeydown("t", { metaKey: true });
			expect(handlers.createNewTerminal).not.toHaveBeenCalled();
		});
	});

	describe("Cmd+W delegation", () => {
		// The keyboard case only delegates; the split-vs-terminal decision (and the
		// empty-pane cancellation) is owned by closeActiveTabOrPane / closeActivePane
		// and covered in useSplitPanes.test.ts.
		it("routes through closeActiveTabOrPane regardless of split state", () => {
			fireKeydown("w", { metaKey: true });
			expect(handlers.closeActiveTabOrPane).toHaveBeenCalledOnce();
			expect(handlers.closeTerminal).not.toHaveBeenCalled();
			expect(handlers.closeActivePane).not.toHaveBeenCalled();
		});
	});

	describe("split pane navigation", () => {
		/** Helper: create a vertical split with two terminal groups */
		function setupVerticalSplit() {
			const id1 = terminalsStore.add(makeTerminal({ name: "T1" }));
			const id2 = terminalsStore.add(makeTerminal({ name: "T2" }));
			const g1 = paneLayoutStore.createGroup();
			paneLayoutStore.addTab(g1, { id: id1, type: "terminal" });
			const g2 = paneLayoutStore.createGroup();
			paneLayoutStore.addTab(g2, { id: id2, type: "terminal" });
			paneLayoutStore.setRoot({
				type: "branch",
				direction: "vertical",
				children: [
					{ type: "leaf", id: g1 },
					{ type: "leaf", id: g2 },
				],
				ratios: [0.5, 0.5],
			});
			paneLayoutStore.setActiveGroup(g1);
			terminalsStore.setActive(id1);
			return { id1, id2, g1, g2 };
		}

		it("Alt+ArrowRight navigates panes in vertical split", () => {
			const { g2 } = setupVerticalSplit();
			fireKeydown("ArrowRight", { altKey: true });
			expect(paneLayoutStore.state.activeGroupId).toBe(g2);
		});

		it("Alt+ArrowLeft navigates panes in vertical split", () => {
			const { g1, g2, id2 } = setupVerticalSplit();
			paneLayoutStore.setActiveGroup(g2);
			terminalsStore.setActive(id2);
			fireKeydown("ArrowLeft", { altKey: true });
			expect(paneLayoutStore.state.activeGroupId).toBe(g1);
		});

		it("Alt+ArrowDown navigates panes in horizontal split", () => {
			const id1 = terminalsStore.add(makeTerminal({ name: "T1" }));
			const id2 = terminalsStore.add(makeTerminal({ name: "T2" }));
			const g1 = paneLayoutStore.createGroup();
			paneLayoutStore.addTab(g1, { id: id1, type: "terminal" });
			const g2 = paneLayoutStore.createGroup();
			paneLayoutStore.addTab(g2, { id: id2, type: "terminal" });
			paneLayoutStore.setRoot({
				type: "branch",
				direction: "horizontal",
				children: [
					{ type: "leaf", id: g1 },
					{ type: "leaf", id: g2 },
				],
				ratios: [0.5, 0.5],
			});
			paneLayoutStore.setActiveGroup(g1);
			terminalsStore.setActive(id1);

			fireKeydown("ArrowDown", { altKey: true });
			expect(paneLayoutStore.state.activeGroupId).toBe(g2);
		});

		it("Alt+ArrowRight at rightmost pane stays put", () => {
			const { g2, id2 } = setupVerticalSplit();
			paneLayoutStore.setActiveGroup(g2);
			terminalsStore.setActive(id2);
			fireKeydown("ArrowRight", { altKey: true });
			expect(paneLayoutStore.state.activeGroupId).toBe(g2);
		});

		it("ignores Alt+Arrow when not in split mode", () => {
			const id = terminalsStore.add(makeTerminal({ name: "T1" }));
			terminalsStore.setActive(id);

			fireKeydown("ArrowRight", { altKey: true });

			// Should not error and terminal should stay active
			expect(terminalsStore.state.activeId).toBe(id);
		});
	});

	describe("find in terminal", () => {
		it("Cmd+F triggers findInTerminal", () => {
			fireKeydown("f", { metaKey: true });
			expect(handlers.findInTerminal).toHaveBeenCalled();
		});
	});

	describe("number key out of range", () => {
		it("does not select terminal for index beyond available", () => {
			(handlers.terminalIds as ReturnType<typeof vi.fn>).mockReturnValue(["t1"]);
			fireKeydown("5", { metaKey: true });
			expect(handlers.handleTerminalSelect).not.toHaveBeenCalled();
		});
	});

	describe("cleanup", () => {
		it("removes keydown listener on cleanup", () => {
			cleanup?.();
			cleanup = undefined;

			fireKeydown("t", { metaKey: true });
			expect(handlers.createNewTerminal).not.toHaveBeenCalled();
		});
	});
});
