import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../invoke", () => ({ invoke: vi.fn() }));

import { StoryDelivery } from "../../components/StoriesDialog/StoryDelivery";
import { invoke } from "../../invoke";

const runs = [
	{ run_id: "aaaaaaaa-1", key: "waiting-key", status: { type: "running" }, bound: false },
	{ run_id: "bbbbbbbb-2", key: "done-key", status: { type: "completed", outcome: "delivered" }, bound: false },
];

describe("StoryDelivery", () => {
	beforeEach(() => {
		vi.mocked(invoke).mockReset();
		vi.mocked(invoke).mockImplementation(async (_command, args) => {
			const action = (args as { action: { action: string } }).action;
			if (action.action === "delivery_runs") return { type: "delivery_runs", value: runs };
			if (action.action === "start_delivery") return { type: "inspection", value: {} };
			throw new Error(`unexpected action ${action.action}`);
		});
	});

	it("resumes a run waiting after a restart with its own key", async () => {
		// Catches: Resume sending a new key, which starts a second run of the story (double delivery).
		render(() => <StoryDelivery project="/repo" storyId="s1" />);
		await screen.findByText(/Waiting for resume/);
		expect(screen.getAllByRole("button", { name: "Resume" })).toHaveLength(1);
		fireEvent.click(screen.getByRole("button", { name: "Resume" }));
		await waitFor(() =>
			expect(invoke).toHaveBeenCalledWith("workflow_neutral_action", {
				project: "/repo",
				action: { action: "start_delivery", story: "s1", key: "waiting-key" },
			}),
		);
	});
});
