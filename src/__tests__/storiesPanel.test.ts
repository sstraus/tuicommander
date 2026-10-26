import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { invoke } from "../invoke";
import { storiesPanelAdapter } from "../panelAdapters/stories";
import { detachPanel, registerPanel } from "../panelRouter";
import { storiesUi } from "../stores/storiesUi";
import { uiStore } from "../stores/ui";

vi.mock("../invoke", () => ({
	invoke: vi.fn().mockResolvedValue(undefined),
	listen: vi.fn().mockResolvedValue(() => {}),
}));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: vi.fn().mockResolvedValue(undefined) }));

beforeEach(async () => {
	vi.useFakeTimers();
	await uiStore.hydrate();
});

afterEach(async () => {
	uiStore.clearDetached("stories");
	storiesUi.close();
	await vi.runAllTimersAsync();
	vi.useRealTimers();
	vi.clearAllMocks();
});

it("catches losing the selected project or opening a second dialog while Stories is detached", async () => {
	registerPanel(storiesPanelAdapter);
	storiesUi.open("/selected-project");
	await detachPanel("stories");
	expect(invoke).toHaveBeenCalledWith("open_panel_window", {
		panelId: "stories",
		title: "Plans and Stories",
		params: { project: "/selected-project", visibility: "all", storyType: "all", view: "list" },
		width: 1120,
		height: 800,
	});
	expect(storiesUi.visible()).toBe(false);
	expect(uiStore.isDetached("stories")).toBe(true);
	storiesUi.open("/another-project");
	expect(invoke).toHaveBeenCalledWith("focus_panel_window", { panelId: "stories" });
	expect(storiesUi.project()).toBe("/selected-project");
	expect(storiesUi.visible()).toBe(false);
	uiStore.clearDetached("stories");
	storiesPanelAdapter.toggle?.();
	expect(storiesUi.visible()).toBe(true);
	expect(storiesUi.project()).toBe("/selected-project");
});
