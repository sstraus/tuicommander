import { readFileSync } from "node:fs";
import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";
import { StoriesDialog } from "../../components/StoriesDialog/StoriesDialog";

const plans = ["Alpha", "Beta"].map((title) => ({
	id: title,
	title,
	project: "/sizing-fixture",
	source: `${title}.md`,
}));
const stories = [1, 40].map((count, index) => ({
	id: `story-${index}`,
	planId: "Alpha",
	title: `Story ${index}`,
	criteria: Array.from({ length: count }, (_, i) => `Criterion ${i}`),
	checked: Array(count).fill(false),
	dependencies: [],
	priority: 2,
	origin: { type: "native" },
	fileScope: [],
	status: "ready",
	revision: 1,
	claimSession: null,
	abandoned: false,
}));
vi.mock("../../invoke", () => ({
	invoke: vi.fn().mockImplementation(async (command, args) => {
		if (command === "story_capabilities") return true;
		if (args.action.action === "transition_history") return { type: "transitions", value: [] };
		if (args.action.action === "list_notes") return { type: "notes", value: [] };
		if (args.action.action === "list_plans") return { type: "plans", value: plans };
		if (args.action.action === "plan_view")
			return { type: "plan_view", value: { stories, state: "active", wontFixCount: 0, allCancelled: false } };
		return { type: "plan_sources", value: [] };
	}),
}));
vi.mock("../../stores/modalStack", () => ({ registerModal: vi.fn() }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn(), error: vi.fn() } }));
vi.mock("../../components/WorkflowDesigner/WorkflowDesigner", () => ({
	WorkflowDesigner: () => <div>Designer content</div>,
}));

afterEach(cleanup);

it("catches content-derived dialog height when switching between empty stories and Designer", async () => {
	const sheet = document.createElement("style");
	sheet.textContent = readFileSync("src/components/StoriesDialog/StoriesDialog.module.css", "utf8");
	document.head.append(sheet);
	try {
		const { getByRole, findByRole } = render(() => <StoriesDialog project="/sizing-fixture" onClose={() => {}} />);
		const dialog = getByRole("dialog");
		const size = () => ({ width: getComputedStyle(dialog).width, height: getComputedStyle(dialog).height });
		const initial = size();
		// Happy DOM has no layout engine: assert the definite sizing contract here;
		// the isolated desktop check measures actual bounding boxes across selections.
		expect(initial.height).not.toBe("");
		expect(initial.height).not.toBe("auto");
		fireEvent.click(await findByRole("button", { name: /Story 1/ }));
		expect(size()).toEqual(initial);
		fireEvent.click(getByRole("button", { name: "Beta" }));
		expect(size()).toEqual(initial);
		fireEvent.click(getByRole("button", { name: "Designer" }));
		expect(size()).toEqual(initial);
		fireEvent.click(getByRole("button", { name: /^Stories$/ }));
		expect(size()).toEqual(initial);
	} finally {
		sheet.remove();
	}
});
