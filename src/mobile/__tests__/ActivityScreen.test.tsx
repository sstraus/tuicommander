import { cleanup, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { activityStore } from "../../stores/activityStore";
import { ActivityScreen } from "../screens/ActivityScreen";

const { invoke } = vi.hoisted(() => ({
	invoke: vi.fn(async (command: string) => {
		if (command === "load_activity") {
			return [
				{
					id: "saved-mobile",
					pluginId: "core",
					sectionId: "messages",
					title: "Agent completed work",
					icon: "<svg/>",
					dismissible: true,
					createdAt: 1000,
					subtitle: "ran for 2343s · 1 blocks",
				},
			];
		}
		return {};
	}),
}));
vi.mock("../../invoke", () => ({ invoke }));

beforeEach(async () => {
	await activityStore.hydrate();
	activityStore.clearAll();
	invoke.mockClear();
});
afterEach(() => {
	cleanup();
	activityStore._testCancelPendingSave();
});

describe("mobile Activity", () => {
	it("shows an active persisted event after opening the Activity tab", async () => {
		render(() => <ActivityScreen onNavigateSession={() => {}} />);
		await waitFor(() => expect(screen.getByText("Agent completed work")).toBeTruthy());
		await waitFor(() => expect(screen.queryByText("No recent activity")).toBeNull());
		expect(screen.getByText("ran for 39 min · 1 block")).toBeTruthy();
		expect(screen.queryByText("ran for 2343s · 1 blocks")).toBeNull();
	});
});
