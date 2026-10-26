import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { emitTo } from "@tauri-apps/api/event";
import { afterEach, expect, it, vi } from "vitest";
import { storiesPanelAdapter } from "../panelAdapters/stories";
import { storiesUi } from "../stores/storiesUi";

vi.mock("../invoke", () => ({
	invoke: vi
		.fn()
		.mockImplementation(async (command: string) =>
			command === "story_capabilities" ? true : { type: "plans", value: [] },
		),
	listen: vi.fn().mockResolvedValue(() => {}),
}));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: vi.fn().mockResolvedValue(undefined) }));
vi.mock("../hooks/initPanelWindow", () => ({ initPanelWindow: vi.fn().mockResolvedValue(undefined) }));
vi.mock("../stores/appLogger", () => ({ appLogger: { warn: vi.fn(), error: vi.fn() } }));

// Catches: cold lazy-module setup consuming the control query's behavior deadline.
// Keep the real lazy boundary, but load its dependency tree before the test starts.
await import("../components/StoriesDialog/StoriesDialog");

afterEach(() => {
	cleanup();
	storiesUi.setVisibility("all");
	storiesUi.setStoryType("all");
	storiesUi.setView("list");
	storiesUi.close();
	vi.clearAllMocks();
});

it("catches losing filters and Kanban when crossing the detached WebView parameter and return-action boundaries", async () => {
	storiesUi.open("/selected-project");
	storiesUi.setVisibility("open");
	storiesUi.setStoryType("feature");
	storiesUi.setView("kanban");
	const params = storiesPanelAdapter.detachParams!();
	expect(params).toEqual({ project: "/selected-project", visibility: "open", storyType: "feature", view: "kanban" });
	// A new WebView starts with a fresh module, not the originating window's signals.
	storiesUi.setVisibility("all");
	storiesUi.setStoryType("all");
	storiesUi.setView("list");
	const Panel = storiesPanelAdapter.Component;
	const ui = render(() => <Panel params={new URLSearchParams(params)} />);
	const status = await ui.findByLabelText("Status");
	expect((status as HTMLSelectElement).value).toBe("open");
	expect((ui.getByLabelText("Story type") as HTMLSelectElement).value).toBe("feature");
	expect(ui.getByRole("tab", { name: /^Kanban$/ }).getAttribute("aria-selected")).toBe("true");
	fireEvent.change(status, { target: { value: "closed" } });
	fireEvent.change(ui.getByLabelText("Story type"), { target: { value: "untyped" } });
	fireEvent.click(ui.getByRole("tab", { name: /^List$/ }));
	const returned = { visibility: "closed", storyType: "untyped", view: "list" };
	await waitFor(() =>
		expect(emitTo).toHaveBeenCalledWith("main", "panel-action", {
			panelId: "stories",
			action: "controls",
			data: returned,
		}),
	);
	ui.unmount();
	// The main WebView still has its own old signals until it consumes the action.
	storiesUi.setVisibility("open");
	storiesUi.setStoryType("feature");
	storiesUi.setView("kanban");
	storiesPanelAdapter.handleAction!("controls", returned);
	expect(storiesUi.visibility()).toBe("closed");
	expect(storiesUi.storyType()).toBe("untyped");
	expect(storiesUi.view()).toBe("list");
	storiesPanelAdapter.handleAction!("controls", { visibility: "done", storyType: "native", view: "grid" });
	expect(storiesUi.controls()).toEqual(returned);
});
