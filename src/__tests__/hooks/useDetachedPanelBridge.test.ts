import { createRoot } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { mockInvoke, mockListen, mockProviderStart, mockProviderStop, mockCreateProvider } = vi.hoisted(() => ({
	mockInvoke: vi.fn().mockResolvedValue(undefined),
	mockListen: vi.fn(),
	mockProviderStart: vi.fn(),
	mockProviderStop: vi.fn(),
	mockCreateProvider: vi.fn(),
}));

vi.mock("../../invoke", () => ({ invoke: mockInvoke, listen: mockListen }));
vi.mock("../../transport", () => ({ isTauri: () => true }));
vi.mock("../../utils/panelSync", () => ({
	createPanelSyncProvider: mockCreateProvider,
}));

vi.mock("../../services/chatVoice", () => ({ stopChatVoice: vi.fn().mockResolvedValue(undefined) }));

import { useDetachedPanelBridge } from "../../hooks/useDetachedPanelBridge";
import { createMarkdownDocumentPanelAdapter } from "../../panelAdapters/markdownDocument";
import type { PanelAdapter } from "../../panelRouter";
import { panelRegistry, registerPanel } from "../../panelRouter";
import { stopChatVoice } from "../../services/chatVoice";
import { mdTabsStore } from "../../stores/mdTabs";
import { uiStore } from "../../stores/ui";

type EventHandler = (event: { payload: unknown }) => void;

const makeAdapter = (overrides: Partial<PanelAdapter> = {}): PanelAdapter => ({
	id: "test-panel",
	title: "Test Panel",
	defaultSize: { width: 600, height: 400 },
	Component: (() => null) as unknown as PanelAdapter["Component"],
	...overrides,
});

describe("useDetachedPanelBridge", () => {
	let dispose: (() => void) | undefined;
	let handlers: Map<string, EventHandler>;
	let restoreDetachedPanels: (() => void) | undefined;
	const unlisteners: Array<ReturnType<typeof vi.fn>> = [];

	const startBridge = async (onConfigureEgo?: () => void) => {
		createRoot((rootDispose) => {
			dispose = rootDispose;
			({ restoreDetachedPanels } = useDetachedPanelBridge(onConfigureEgo));
		});
		await Promise.resolve();
	};

	beforeEach(() => {
		mdTabsStore.clearAll();
		for (const key of Object.keys(panelRegistry)) delete panelRegistry[key];
		for (const key of Object.keys(uiStore.state.detachedPanels)) uiStore.clearDetached(key);
		uiStore._testCancelPendingSave();
		handlers = new Map();
		unlisteners.length = 0;
		mockInvoke.mockReset().mockResolvedValue(undefined);
		mockListen.mockReset().mockImplementation((event: string, handler: EventHandler) => {
			handlers.set(event, handler);
			const unlisten = vi.fn();
			unlisteners.push(unlisten);
			return Promise.resolve(unlisten);
		});
		vi.mocked(stopChatVoice).mockClear();
		mockProviderStart.mockClear();
		mockProviderStop.mockClear();
		mockCreateProvider.mockReset().mockReturnValue({ start: mockProviderStart, stop: mockProviderStop });
	});

	// Catches: detached AI Chat setup mail is discarded instead of opening the main Settings.
	it("opens ego settings only for the AI Chat configure action", async () => {
		let opened = 0;
		await startBridge(() => {
			opened += 1;
		});
		handlers.get("panel-action")?.({ payload: { panelId: "notes", action: "configure-ego", data: {} } });
		expect(opened).toBe(0);
		handlers.get("panel-action")?.({ payload: { panelId: "ai-chat", action: "configure-ego", data: {} } });
		expect(opened).toBe(1);
	});

	it("returns a detached Markdown document to its existing tab when its window closes", async () => {
		const tabId = mdTabsStore.add("/repo", "notes.md");
		const onSelect = vi.fn();
		const adapter = createMarkdownDocumentPanelAdapter(tabId, onSelect);
		registerPanel(adapter);
		uiStore.setDetached(adapter.id, `panel-${adapter.id}`);
		await startBridge();
		handlers.get("panel-window-closed")?.({ payload: adapter.id });
		expect(uiStore.isDetached(adapter.id)).toBe(false);
		expect(mdTabsStore.get(tabId)).toMatchObject({ type: "file", filePath: "notes.md" });
		expect(onSelect).toHaveBeenCalledExactlyOnceWith(tabId);
	});

	afterEach(() => {
		dispose?.();
		dispose = undefined;
		uiStore._testCancelPendingSave();
	});

	it("opens a linked Markdown file from a detached document in the main tab store", async () => {
		const tabId = mdTabsStore.add("/repo", "notes.md");
		const adapter = createMarkdownDocumentPanelAdapter(tabId);
		registerPanel(adapter);
		await startBridge();
		handlers.get("panel-action")?.({
			payload: {
				panelId: adapter.id,
				action: "open-link",
				data: {
					kind: "file",
					absolute_path: "/repo/next.md",
					open_path: "next.md",
					is_directory: false,
					same_document: false,
				},
			},
		});
		expect(mdTabsStore.getIds().map((id) => mdTabsStore.get(id))).toEqual(
			expect.arrayContaining([expect.objectContaining({ type: "file", filePath: "next.md" })]),
		);
	});

	// Both paths mean the same thing to a panel: the detached copy is gone and
	// this window owns it again. A panel that caches state the detached copy
	// was mutating has to be told on BOTH, not just on the explicit reattach.
	it("routes close and reattach events through the registered adapter", async () => {
		const toggle = vi.fn();
		const onReattach = vi.fn();
		registerPanel(makeAdapter({ toggle, onReattach }));
		uiStore.setDetached("test-panel", "panel-test-panel");
		await startBridge();

		handlers.get("panel-window-closed")?.({ payload: "test-panel" });

		expect(uiStore.isDetached("test-panel")).toBe(false);
		expect(toggle).toHaveBeenCalledOnce();
		expect(onReattach).toHaveBeenCalledOnce();

		uiStore.setDetached("test-panel", "panel-test-panel");
		handlers.get("panel-action")?.({ payload: { panelId: "test-panel", action: "reattach", data: {} } });
		expect(uiStore.isDetached("test-panel")).toBe(false);
		expect(toggle).toHaveBeenCalledTimes(2);
		expect(onReattach).toHaveBeenCalledTimes(2);
	});

	// `reattachPanel` emits panel-action AND then closes the window, and closing
	// it emits panel-window-closed — so the real reattach click delivers BOTH
	// events for one homecoming. Handling each on its own toggled the panel open
	// and straight back closed, and ran the adapter's disk re-read twice.
	it("comes home once when reattach is followed by the window closing", async () => {
		const toggle = vi.fn();
		const onReattach = vi.fn();
		registerPanel(makeAdapter({ toggle, onReattach }));
		uiStore.setDetached("test-panel", "panel-test-panel");
		await startBridge();

		handlers.get("panel-action")?.({ payload: { panelId: "test-panel", action: "reattach", data: {} } });
		handlers.get("panel-window-closed")?.({ payload: "test-panel" });

		expect(uiStore.isDetached("test-panel")).toBe(false);
		expect(toggle).toHaveBeenCalledOnce();
		expect(onReattach).toHaveBeenCalledOnce();
	});

	// A window closed while the panel is not marked detached is not a homecoming
	// — nothing to clear, nothing to toggle back on.
	it("ignores a close for a panel that is not detached", async () => {
		const toggle = vi.fn();
		const onReattach = vi.fn();
		registerPanel(makeAdapter({ toggle, onReattach }));
		await startBridge();

		handlers.get("panel-window-closed")?.({ payload: "test-panel" });

		expect(toggle).not.toHaveBeenCalled();
		expect(onReattach).not.toHaveBeenCalled();
	});

	// Catches: destroying the detached view leaves its microphone binding armed.
	it("revokes AI Chat voice on native close but not reattach", async () => {
		registerPanel(makeAdapter({ id: "ai-chat" }));
		uiStore.setDetached("ai-chat", "panel-ai-chat");
		await startBridge();
		handlers.get("panel-action")?.({ payload: { panelId: "ai-chat", action: "reattach", data: {} } });
		handlers.get("panel-window-closed")?.({ payload: "ai-chat" });
		expect(stopChatVoice).not.toHaveBeenCalled();

		uiStore.setDetached("ai-chat", "panel-ai-chat");
		handlers.get("panel-window-closed")?.({ payload: "ai-chat" });
		expect(stopChatVoice).toHaveBeenCalledOnce();
	});

	it("routes non-lifecycle actions to the adapter", async () => {
		const handleAction = vi.fn();
		registerPanel(makeAdapter({ handleAction }));
		await startBridge();

		handlers.get("panel-action")?.({ payload: { panelId: "test-panel", action: "refresh", data: { force: true } } });

		expect(handleAction).toHaveBeenCalledWith("refresh", { force: true });
	});

	it("starts and stops a projection provider with detached state", async () => {
		const serialize = vi.fn(() => ({ value: 1 }));
		registerPanel(makeAdapter({ serialize, syncIntervalMs: 500 }));
		await startBridge();

		uiStore.setDetached("test-panel", "panel-test-panel");
		expect(mockCreateProvider).toHaveBeenCalledWith("test-panel", serialize, 500);
		expect(mockProviderStart).toHaveBeenCalledOnce();

		uiStore.clearDetached("test-panel");
		expect(mockProviderStop).toHaveBeenCalledOnce();
	});

	it("restores known detached panels and clears unknown ones", async () => {
		registerPanel(makeAdapter());
		uiStore.setDetached("test-panel", "panel-test-panel");
		uiStore.setDetached("missing-panel", "panel-missing-panel");
		await startBridge();

		restoreDetachedPanels?.();

		expect(mockInvoke).toHaveBeenCalledWith("open_panel_window", {
			panelId: "test-panel",
			title: "Test Panel",
			params: {},
			width: 600,
			height: 400,
		});
		expect(uiStore.isDetached("missing-panel")).toBe(false);
	});

	// Restoring with a hardcoded `{}` reopened the AI Chat window with no chat
	// id, so it came back blank after every restart — the exact symptom the
	// param exists to prevent.
	it("restores a panel with the params it would be detached with", async () => {
		registerPanel(makeAdapter({ detachParams: () => ({ chatId: "conv-7" }) }));
		uiStore.setDetached("test-panel", "panel-test-panel");
		await startBridge();

		restoreDetachedPanels?.();

		expect(mockInvoke).toHaveBeenCalledWith(
			"open_panel_window",
			expect.objectContaining({ panelId: "test-panel", params: { chatId: "conv-7" } }),
		);
	});

	it("cleans up both native listeners on disposal", async () => {
		registerPanel(makeAdapter());
		await startBridge();

		dispose?.();
		dispose = undefined;

		expect(unlisteners).toHaveLength(2);
		expect(unlisteners.every((unlisten) => unlisten.mock.calls.length === 1)).toBe(true);
	});
});
