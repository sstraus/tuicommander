import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { createRoot } from "solid-js";
import { afterEach, expect, it, vi } from "vitest";

const backend = vi.hoisted(() => ({ config: { ego_executable: "", experimental_features_enabled: true } }));
let disposePanel: (() => void) | undefined;

vi.mock("../../invoke", () => ({
	invoke: vi.fn(async (command: string, args?: Record<string, unknown>) => {
		if (command === "load_config") return structuredClone(backend.config);
		if (command === "save_config") {
			backend.config = structuredClone(args?.config) as typeof backend.config;
			return;
		}
		if (command === "focus_main_window") return;
		throw new Error(`Unexpected command: ${command}`);
	}),
	listen: vi.fn(async () => () => {}),
	guardTauriUnlisten: (unlisten: () => void) => unlisten,
}));
vi.mock("@tauri-apps/api/event", () => ({
	emitTo: vi.fn(async () => {}),
	listen: vi.fn(async () => () => {}),
}));
vi.mock("../../themes", () => ({
	loadThemes: vi.fn(async () => {}),
	listenForThemeChanges: vi.fn(async () => {}),
	applyAppTheme: vi.fn(),
	applyFontFamily: vi.fn(),
}));

// Import setup is not the behavior deadline. Prepare both WebView module graphs
// before the test starts, while keeping their settings stores independent.
const { initPanelWindow } = await import("../../hooks/initPanelWindow");
const { AIChatPanel } = await import("../../components/AIChatPanel/AIChatPanel");
vi.resetModules();
const { settingsStore: mainSettings } = await import("../../stores/settings");

afterEach(() => {
	vi.useRealTimers();
	cleanup();
	disposePanel?.();
	window.history.replaceState({}, "", "/");
});

// Catches: detached settings stay stale after main-window setup, leaving chat inactive until the window is reopened.
it("activates the already-open detached composer after ego is configured in the main window", async () => {
	localStorage.clear();
	window.history.replaceState({}, "", "/?mode=panel&panel=ai-chat");
	await createRoot((dispose) => {
		disposePanel = dispose;
		return initPanelWindow();
	});
	const panel = render(() => <AIChatPanel visible={true} repoPath={null} onClose={() => {}} />);
	expect(panel.queryByRole("textbox")).toBeNull();
	await fireEvent.click(panel.getByRole("button", { name: "Configure ego" }));

	// A second WebView has its own module instance and saves to the same backend.
	await mainSettings.hydrate();
	// The save debounce is setup for focus refresh, not a wall-clock assertion.
	vi.useFakeTimers();
	mainSettings.setEgoExecutable("/usr/local/bin/ego");
	await vi.runOnlyPendingTimersAsync();
	vi.useRealTimers();
	expect(backend.config.ego_executable).toBe("/usr/local/bin/ego");
	window.dispatchEvent(new Event("focus"));
	document.dispatchEvent(new Event("visibilitychange"));
	await waitFor(() => expect(panel.queryByRole("textbox")).not.toBeNull());
});
