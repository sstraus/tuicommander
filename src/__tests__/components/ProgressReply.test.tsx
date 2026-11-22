import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ProgressReply } from "../../components/ProgressDialog/ProgressReply";
import { invoke } from "../../invoke";
import type { ProgressEntry } from "../../stores/progress";
import { progressRepliesStore } from "../../stores/progressReplies";

vi.mock("../../invoke", () => ({ invoke: vi.fn() }));
vi.mock("../../i18n", () => ({ t: (_key: string, fallback: string) => fallback }));
const entry: ProgressEntry = {
	id: 31,
	project: "/origin",
	ptyId: "author",
	replyTargetAvailable: true,
	createdAtMs: 1,
	type: "blocked",
	text: "Continue the current step?",
};
const receipt = { entryId: 31, requestId: "server-receipt", delivery: "queued" };
function deferred() {
	let resolve!: (value: typeof receipt) => void;
	const promise = new Promise<typeof receipt>((done) => {
		resolve = done;
	});
	return { promise, resolve };
}
beforeEach(() => {
	vi.clearAllMocks();
	progressRepliesStore.resetForTests();
});
afterEach(cleanup);

// Catches: Enter submitting during IME/newline entry, or sending to a global selection instead of the report origin.
it("opens an inline draft and sends Enter to the immutable report scope without bubbling", async () => {
	const outside = vi.fn();
	vi.mocked(invoke).mockResolvedValue(receipt);
	render(() => (
		<div onClick={outside} onKeyDown={outside}>
			<ProgressReply entry={entry} />
		</div>
	));
	fireEvent.click(screen.getByRole("button", { name: "Reply" }));
	const input = screen.getByRole("textbox", { name: "Reply to this progress" });
	fireEvent.input(input, { target: { value: "Please continue\nwith the scoped checks" } });
	fireEvent.keyDown(input, { key: "Enter", shiftKey: true });
	fireEvent.keyDown(input, { key: "Enter", isComposing: true });
	expect(invoke).not.toHaveBeenCalled();
	fireEvent.keyDown(input, { key: "Enter" });
	await screen.findByText("Reply queued for the agent");
	expect(invoke).toHaveBeenCalledOnce();
	expect(invoke).toHaveBeenCalledWith("progress_reply", {
		project: "/origin",
		input: { entryId: 31, requestId: expect.any(String), text: "Please continue\nwith the scoped checks" },
	});
	expect(outside).not.toHaveBeenCalled();
	expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
});

// Catches: guessing yes/no from prose, and duplicate answers from the toast and journal surfaces.
it("shows binary buttons only for structured questions and shares their pending/answered state", async () => {
	const binary: ProgressEntry = { ...entry, question: { kind: "yes_no" } };
	const pending = deferred();
	vi.mocked(invoke).mockReturnValue(pending.promise);
	render(() => (
		<>
			<ProgressReply entry={binary} />
			<ProgressReply entry={binary} />
		</>
	));
	expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
	const yes = screen.getAllByRole("button", { name: "Yes" });
	fireEvent.click(yes[0]);
	expect(yes[1]).toBeDisabled();
	fireEvent.click(yes[1]);
	expect(invoke).toHaveBeenCalledOnce();
	expect(invoke).toHaveBeenLastCalledWith("progress_reply", {
		project: "/origin",
		input: { entryId: 31, requestId: expect.any(String), answer: true },
	});
	pending.resolve({ ...receipt, delivery: "delivered" });
	await waitFor(() => expect(screen.getAllByText("Reply sent")).toHaveLength(2));
	expect(screen.queryByRole("button", { name: "No" })).not.toBeInTheDocument();
});

// Catches: failed delivery destroying the draft or a retry acquiring a new delivery identity.
it("retains failed drafts and retries the same request identity", async () => {
	vi.mocked(invoke).mockRejectedValueOnce(new Error("Author is unavailable")).mockResolvedValueOnce(receipt);
	render(() => <ProgressReply entry={entry} />);
	expect(screen.queryByRole("button", { name: "Yes" })).not.toBeInTheDocument();
	fireEvent.click(screen.getByRole("button", { name: "Reply" }));
	const input = screen.getByRole("textbox");
	fireEvent.input(input, { target: { value: "Keep this comment" } });
	fireEvent.click(screen.getByRole("button", { name: "Send" }));
	await screen.findByRole("alert");
	expect(input).toHaveValue("Keep this comment");
	const first = vi.mocked(invoke).mock.calls[0][1];
	fireEvent.click(screen.getByRole("button", { name: "Send" }));
	await screen.findByText("Reply queued for the agent");
	expect(vi.mocked(invoke).mock.calls[1][1]).toEqual(first);
});

// Catches: a response for a hidden report clearing another report's draft after switching views/projects.
it("keeps drafts and late receipts separated by project and entry", async () => {
	const pending = deferred();
	vi.mocked(invoke).mockReturnValue(pending.promise);
	const other = { ...entry, project: "/other", id: 32 };
	const [selected, select] = createSignal(entry);
	render(() => <ProgressReply entry={selected()} />);
	fireEvent.click(screen.getByRole("button", { name: "Reply" }));
	fireEvent.input(screen.getByRole("textbox"), { target: { value: "Origin reply" } });
	fireEvent.click(screen.getByRole("button", { name: "Send" }));
	select(other);
	fireEvent.click(screen.getByRole("button", { name: "Reply" }));
	fireEvent.input(screen.getByRole("textbox"), { target: { value: "Other draft" } });
	pending.resolve(receipt);
	await waitFor(() => expect(progressRepliesStore.get(entry).pending).toBe(false));
	expect(screen.getByRole("textbox")).toHaveValue("Other draft");
	expect(screen.queryByText("Reply queued for the agent")).not.toBeInTheDocument();
});
