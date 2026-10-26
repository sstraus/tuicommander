import { fireEvent, render, screen, within } from "@solidjs/testing-library";
import { beforeEach, expect, it, vi } from "vitest";
import { StoriesDialog } from "../../components/StoriesDialog/StoriesDialog";
import { invoke } from "../../invoke";
import recorded from "../fixtures/story-api/rb-cancellation.json";

vi.mock("../../invoke", () => ({ invoke: vi.fn() }));
vi.mock("../../stores/modalStack", () => ({ registerModal: vi.fn() }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn(), error: vi.fn() } }));

beforeEach(() => {
	vi.mocked(invoke).mockReset();
	vi.mocked(invoke).mockImplementation(async (command, args) => {
		if (command === "story_capabilities") return true;
		const action = (args as { action: { action: string } }).action;
		if (action.action === "transition_history") return { type: "transitions", value: [] };
		if (action.action === "list_notes") return { type: "notes", value: [] };
		if (action.action === "list_plans") return { type: "plans", value: [recorded.plan.value] };
		if (action.action === "plan_view") return recorded.after_cancel;
		throw new Error(`unexpected action ${action.action}`);
	});
});

// Catches: treating the recorded wont_fix status as unknown and offering repeat cancellation.
it("shows recorded cancelled status without offering Won't fix again", async () => {
	render(() => <StoriesDialog project={recorded.plan.value.project} onClose={() => {}} />);
	await screen.findByRole("heading", { name: "A prerequisite" });
	expect(screen.getByText("Won't fix", { selector: "span" })).toBeTruthy();
	expect(screen.queryByRole("button", { name: "Won't fix" })).toBeNull();
});

// Catches: checking wontfix instead of the wire status when offering dependency removal.
it("offers Remove only for the recorded cancelled prerequisite on backlog C", async () => {
	render(() => <StoriesDialog project={recorded.plan.value.project} onClose={() => {}} />);
	await screen.findByRole("heading", { name: "A prerequisite" });
	fireEvent.click(screen.getByRole("button", { name: /C indirect/ }));
	await screen.findByRole("heading", { name: "C indirect" });
	expect(screen.getByRole("button", { name: "Remove A prerequisite" })).toBeTruthy();
	expect(screen.queryByRole("button", { name: "Remove B direct" })).toBeNull();
	expect(screen.getAllByText(/abandoned/)).toHaveLength(2);
	expect(screen.getByRole("button", { name: "Won't fix" })).toBeTruthy();
});

// Catches: hiding the plan state whenever the plan contains stories and cancellation counts.
it("shows Active beside cancellation counts while a recorded story is selected", async () => {
	render(() => <StoriesDialog project={recorded.plan.value.project} onClose={() => {}} />);
	await screen.findByRole("heading", { name: "A prerequisite" });
	const plans = within(screen.getByRole("complementary", { name: "Plans" }));
	expect(plans.getByText(/State: Active/)).toBeTruthy();
	expect(plans.getByText("1 won't fix")).toBeTruthy();
});
