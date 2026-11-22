import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ToastList } from "../../components/ToastContainer/ToastList";
import { type ProgressEntry, type ProgressRecordedPayload, progressStore } from "../../stores/progress";
import { progressRepliesStore } from "../../stores/progressReplies";
import { toastsStore } from "../../stores/toasts";

const { reply } = vi.hoisted(() => ({
	reply: vi.fn(async (_args: Record<string, unknown>): Promise<unknown> => ({ delivery: "delivered" })),
}));
vi.mock("../../invoke", () => ({
	invoke: vi.fn(async (command: string, args: Record<string, unknown>, connectionId?: string | null) => {
		if (command === "progress_reply") return reply({ ...args, connectionId });
		if (command === "progress_list") return { project: "/orig", entries: [], ptyIds: [], nextCursor: null };
		return {};
	}),
}));
vi.mock("../../stores/modalStack", () => ({ registerModal: vi.fn() }));
vi.mock("../../services/nativeNotifications", () => ({ showNativeNotice: vi.fn() }));
// The user is looking at a different repository than the one that reported.
vi.mock("../../stores/repositories", () => ({
	repositoriesStore: { state: { activeRepoPath: "/selected" }, get: () => ({ displayName: "Selected" }) },
}));
vi.mock("../../stores/terminals", () => ({
	terminalsStore: { state: { activeId: null, terminals: {} }, get: () => undefined },
}));

function entry(patch: Partial<ProgressEntry> = {}): ProgressEntry {
	return {
		id: 41,
		project: "/orig",
		ptyId: "pty-author",
		replyTargetAvailable: true,
		createdAtMs: Date.now(),
		type: "blocked",
		text: "Need input",
		...patch,
	};
}

function present(e: ProgressEntry, extra: Record<string, unknown> = {}) {
	progressStore.presentLive({ repo_path: e.project, payload: { entry: e }, ...extra } as ProgressRecordedPayload);
}

const onDismiss = vi.fn((toast: { id: number }) => toastsStore.remove(toast.id));
function popups() {
	return render(() => <ToastList onDismiss={onDismiss} repoName={() => "Orig"} />);
}

function replyCalls() {
	return reply.mock.calls.map(([args]) => args as { project: string; input: Record<string, unknown> });
}

beforeEach(() => {
	progressStore.resetForTests();
	progressRepliesStore.resetForTests();
	reply.mockReset();
	reply.mockImplementation(async () => ({ entryId: 41, requestId: "r", delivery: "delivered" }));
	onDismiss.mockClear();
	while (toastsStore.toasts.length) toastsStore.remove(toastsStore.toasts[0].id);
});
afterEach(() => {
	cleanup();
	vi.useRealTimers();
	while (toastsStore.toasts.length) toastsStore.remove(toastsStore.toasts[0].id);
});

describe("Progress reply composer keys", () => {
	it("opens only on click and sends on plain Enter, never on Shift+Enter or IME composition", async () => {
		// Catches: a composer rendered eagerly, or Enter handling that submits a newline or an IME commit.
		popups();
		present(entry());
		expect(screen.queryByRole("textbox")).toBeNull();
		fireEvent.click(screen.getByRole("button", { name: "Reply" }));
		const box = await screen.findByRole("textbox");
		fireEvent.input(box, { target: { value: "use the staging host" } });
		fireEvent.keyDown(box, { key: "Enter", shiftKey: true });
		fireEvent.keyDown(box, { key: "Enter", isComposing: true });
		fireEvent.keyDown(box, { key: "Enter", keyCode: 229 });
		expect(reply).not.toHaveBeenCalled();
		fireEvent.keyDown(box, { key: "Enter" });
		await waitFor(() => expect(reply).toHaveBeenCalledTimes(1));
		// Original report, not the repository the user has selected.
		expect(replyCalls()[0].project).toBe("/orig");
		expect(replyCalls()[0].input).toMatchObject({ entryId: 41, text: "use the staging host" });
		expect(toastsStore.toasts).toHaveLength(1);
	});
});

describe("Progress reply retries", () => {
	it("keeps the draft after a failure, retries with the same id, and uses a new id for an edited comment", async () => {
		// Catches: a draft cleared on failure, a retry that mints a new id (duplicate mail), or an edit reusing the old id.
		reply.mockRejectedValueOnce(new Error("network down")).mockRejectedValueOnce(new Error("still down"));
		popups();
		present(entry());
		fireEvent.click(screen.getByRole("button", { name: "Reply" }));
		const box = (await screen.findByRole("textbox")) as HTMLTextAreaElement;
		fireEvent.input(box, { target: { value: "first" } });
		fireEvent.keyDown(box, { key: "Enter" });
		await screen.findByText("network down");
		expect((screen.getByRole("textbox") as HTMLTextAreaElement).value).toBe("first");
		fireEvent.keyDown(screen.getByRole("textbox"), { key: "Enter" });
		await screen.findByText("still down");
		const [a, b] = replyCalls();
		expect(b.input.requestId).toBe(a.input.requestId);
		fireEvent.input(screen.getByRole("textbox"), { target: { value: "first, edited" } });
		fireEvent.keyDown(screen.getByRole("textbox"), { key: "Enter" });
		await waitFor(() => expect(reply).toHaveBeenCalledTimes(3));
		expect(replyCalls()[2].input.requestId).not.toBe(a.input.requestId);
		await waitFor(() => expect(screen.queryByRole("textbox")).toBeNull());
	});
});

describe("Explicit yes/no questions", () => {
	it("offers Yes/No directly, sends the boolean, and reports a queued receipt honestly", async () => {
		// Catches: a text composer for a structured question, a stringly answer, or "sent" for an inbox-only delivery.
		reply.mockImplementation(async () => ({ entryId: 41, requestId: "r", delivery: "queued" }));
		popups();
		present(entry({ question: { kind: "yes_no" }, answered: false }));
		expect(screen.queryByRole("button", { name: "Reply" })).toBeNull();
		fireEvent.click(screen.getByRole("button", { name: "No" }));
		await screen.findByText("Reply queued for the agent");
		expect(replyCalls()[0].input).toMatchObject({ entryId: 41, answer: false });
		expect(replyCalls()[0].input).not.toHaveProperty("text");
		expect(screen.queryByRole("button", { name: "Yes" })).toBeNull();
		expect(screen.queryByRole("button", { name: "No" })).toBeNull();
	});

	it("does not offer a reply when the report has no reply target", () => {
		// Catches: a Reply button for an author TUIC cannot address.
		popups();
		present(entry({ id: 42, ptyId: undefined, replyTargetAvailable: false }));
		expect(screen.queryByRole("button", { name: "Reply" })).toBeNull();
	});
});

describe("Toast lifetime while replying", () => {
	it("keeps a done card on screen while its composer is open", () => {
		// Catches: the auto-dismiss timer removing the card the user is typing into.
		vi.useFakeTimers();
		popups();
		present(entry({ id: 43, type: "done" }));
		fireEvent.click(screen.getByRole("button", { name: "Reply" }));
		vi.advanceTimersByTime(120_000);
		expect(toastsStore.toasts).toHaveLength(1);
	});

	it("does not dismiss the card when the user clicks inside the open composer", async () => {
		// Catches: clicks on the composer's hint or gaps bubbling to the card's dismiss handler.
		popups();
		present(entry({ id: 44 }));
		fireEvent.click(screen.getByRole("button", { name: "Reply" }));
		await screen.findByRole("textbox");
		fireEvent.click(screen.getByText("Enter to send · Shift+Enter for a new line"));
		expect(onDismiss).not.toHaveBeenCalled();
		expect(toastsStore.toasts).toHaveLength(1);
	});
});

describe("Reply host", () => {
	it("never sends a reply to a mirrored remote report without the report's origin connection", async () => {
		// Catches: a report mirrored from another host answered on the local backend, where the same
		// project path and entry id can name a different local author.
		popups();
		present(entry({ id: 45 }), { __tuic_origin: { connection: "remote-host-1" } });
		const open = screen.getByRole("button", { name: "Reply" });
		fireEvent.click(open);
		const box = await screen.findByRole("textbox");
		fireEvent.input(box, { target: { value: "for the remote agent" } });
		fireEvent.keyDown(box, { key: "Enter" });
		await new Promise((resolve) => setTimeout(resolve, 0));
		expect(replyCalls()).toHaveLength(1);
		for (const call of replyCalls()) {
			expect(JSON.stringify(call)).toContain("remote-host-1");
		}
	});
});

// Catches: an invalid mirrored origin being reinterpreted as a local report/action.
it("does not turn malformed mirrored origins into local reply cards", () => {
	popups();
	for (const stamp of [null, {}, { connection: 7 }, { connection: "" }]) {
		present(entry({ id: 76 }), { __tuic_origin: stamp });
		expect(toastsStore.toasts).toHaveLength(0);
	}
});

// Catches: a legacy PTY id alone offering a reply despite unknown author availability.
it("does not offer Reply for an unproven legacy PTY-only report", () => {
	popups();
	present(entry({ replyTargetAvailable: undefined }));
	expect(screen.queryByRole("button", { name: "Reply" })).toBeNull();
});
