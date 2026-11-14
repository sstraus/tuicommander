import "../fixtures/dialogActivityHydration";
import { readFileSync } from "node:fs";
import { cleanup, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { StoriesDialog } from "../../components/StoriesDialog/StoriesDialog";
import { storiesUi } from "../../stores/storiesUi";
import { nativeBackendStatusReply } from "../fixtures/nativeBackendStatus";

const plans = ["Alpha", "Beta"].map((title) => ({
	id: title,
	title,
	project: "/phase-colour-fixture",
	source: `${title}.md`,
}));
const statuses = ["ready", "in_progress", "done", "wont_fix"];
const stories = statuses.map((status, index) => ({
	id: `story-${index}`,
	planId: "Alpha",
	title: `Story ${index}`,
	criteria: ["Criterion"],
	checked: [false],
	dependencies: [],
	priority: 2,
	origin: { type: "native" },
	fileScope: [],
	status,
	revision: 1,
	claimSession: null,
	abandoned: false,
}));
vi.mock("../../invoke", () => ({
	invoke: vi.fn().mockImplementation(async (command, args) => {
		if (command === "story_capabilities") return { backend: "native", operations: {} };
		if (
			command === "story_action_command" &&
			(args as { action?: { action?: string } })?.action?.action === "backend_status"
		)
			return nativeBackendStatusReply;
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

beforeEach(() => storiesUi.setVisibility("all"));
afterEach(cleanup);

it("tints each phase selector by its story status", async () => {
	const { findByLabelText } = render(() => <StoriesDialog project="/phase-colour-fixture" onClose={() => {}} />);
	const tones: string[] = [];
	for (const [index, status] of statuses.entries()) {
		const phase = (await findByLabelText(new RegExp(`Phase for Story ${index}`))).closest("label") as HTMLElement;
		expect(phase.dataset.status).toBe(status);
		tones.push(phase.dataset.status!);
	}
	// Catches: phase colour not tied to the status (every row the same grey).
	expect(new Set(tones).size).toBe(statuses.length);
});

it("styles every listed status in the stylesheet", () => {
	const css = readFileSync("src/components/StoriesDialog/StoriesDialog.module.css", "utf8");
	// Catches: a status that renders the attribute but has no colour rule.
	for (const status of statuses) expect(css).toContain(`.phaseSelector[data-status="${status}"]`);
});
