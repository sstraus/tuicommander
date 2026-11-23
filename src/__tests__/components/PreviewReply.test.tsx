import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";

vi.mock("../../invoke", () => ({ invoke: vi.fn(), listen: vi.fn(async () => () => {}) }));

import { ThreadActions } from "../../components/shared/PreviewReply";
import type { WorkItemPreview } from "../../components/shared/WorkItemPreview";
import { invoke } from "../../invoke";

const preview = () => ({ reload: vi.fn() }) as unknown as WorkItemPreview & { reload: ReturnType<typeof vi.fn> };

describe("review thread actions", () => {
	it("resolves the thread it sits under and reloads the item", async () => {
		// catches: resolving with the wrong thread id, or leaving the resolved thread on screen
		vi.mocked(invoke).mockResolvedValue(undefined);
		const p = preview();
		render(() => <ThreadActions repoPath="/repo" threadId="T_1" preview={p} />);
		fireEvent.click(screen.getByText("Resolve"));
		await waitFor(() => expect(p.reload).toHaveBeenCalled());
		expect(invoke).toHaveBeenCalledWith("set_pr_thread_resolved", {
			repoPath: "/repo",
			threadId: "T_1",
			resolved: true,
		});
	});

	it("posts the typed reply to the thread and closes the composer", async () => {
		// catches: the reply body lost or sent to another thread
		vi.mocked(invoke).mockResolvedValue(undefined);
		const p = preview();
		render(() => <ThreadActions repoPath="/repo" threadId="T_1" preview={p} />);
		fireEvent.click(screen.getByText("Reply"));
		fireEvent.input(screen.getByPlaceholderText("Write a reply"), { target: { value: "Done" } });
		fireEvent.click(screen.getByText("Send"));
		await waitFor(() => expect(p.reload).toHaveBeenCalled());
		expect(invoke).toHaveBeenCalledWith("reply_pr_thread", { repoPath: "/repo", threadId: "T_1", body: "Done" });
		expect(screen.queryByPlaceholderText("Write a reply")).toBeNull();
	});

	it("keeps the composer and shows the error when the reply fails", async () => {
		// catches: a failed reply that silently discards what the user wrote
		vi.mocked(invoke).mockImplementation(async () => {
			throw new Error("thread is locked");
		});
		const p = preview();
		render(() => <ThreadActions repoPath="/repo" threadId="T_1" preview={p} />);
		fireEvent.click(screen.getByText("Reply"));
		fireEvent.input(screen.getByPlaceholderText("Write a reply"), { target: { value: "Done" } });
		fireEvent.click(screen.getByText("Send"));
		await screen.findByText("thread is locked");
		expect((screen.getByPlaceholderText("Write a reply") as HTMLTextAreaElement).value).toBe("Done");
		expect(p.reload).not.toHaveBeenCalled();
	});
});
