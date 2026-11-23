import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DiscussionThread } from "../../components/ProgressDialog/DiscussionThread";
import { ProgressRecent } from "../../components/ProgressDialog/ProgressRecent";
import { invoke } from "../../invoke";
import { progressStore } from "../../stores/progress";

vi.mock("../../invoke", () => ({ invoke: vi.fn(), listen: vi.fn(async () => () => {}) }));
vi.mock("../../stores/repositories", () => ({ repositoriesStore: { get: () => undefined } }));
vi.mock("../../stores/terminals", () => ({ terminalsStore: { findBySessionId: () => null } }));
vi.mock("../../stores/progress", () => ({ progressStore: { open: vi.fn() } }));

beforeEach(() => {
	vi.mocked(invoke).mockReset();
	vi.mocked(progressStore.open).mockReset();
});

describe("ProgressRecent", () => {
	it("renders the inline error when the recent list cannot be read", async () => {
		vi.mocked(invoke).mockRejectedValue("journal unavailable");
		render(() => <ProgressRecent revision={0} />);
		expect(await screen.findByText(/journal unavailable/)).toBeInTheDocument();
	});

	it("opens the project and tells the parent so it can leave the cross-project view", async () => {
		vi.mocked(invoke).mockResolvedValue([
			{ id: 1, project: "/repo", createdAtMs: 1, type: "done", text: "Shipped it", unread: false },
		]);
		const onOpen = vi.fn();
		render(() => <ProgressRecent revision={0} onOpen={onOpen} />);
		fireEvent.click(await screen.findByText("Shipped it"));
		expect(progressStore.open).toHaveBeenCalledWith("/repo", null);
		expect(onOpen).toHaveBeenCalledTimes(1);
	});
});

describe("DiscussionThread", () => {
	it("renders the inline error when the thread cannot be read", async () => {
		vi.mocked(invoke).mockRejectedValue("thread unavailable");
		render(() => <DiscussionThread entryId={7} revision={0} />);
		await waitFor(() => expect(screen.getByText(/thread unavailable/)).toBeInTheDocument());
	});
});
