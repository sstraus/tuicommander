import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { invoke, listen, warnings } = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), warnings: vi.fn() }));
vi.mock("../../invoke", () => ({ invoke, listen }));
vi.mock("../../stores/appLogger", () => ({
	appLogger: { warn: warnings, error: vi.fn(), info: vi.fn(), debug: vi.fn() },
}));

import { TerminalChatView, ViewModeToggle } from "../../components/Terminal/TerminalChatView";
import { chatViewStore } from "../../stores/chatView";
import { terminalsStore } from "../../stores/terminals";
import { HttpRpcError } from "../../transport/http";
import { useSessionChatView } from "../components/useSessionChatView";

let wake: () => void;
let release: ReturnType<typeof vi.fn<() => void>>;
const snapshot = (text: string, epoch = 1) => ({
	epoch,
	nextSeq: 1,
	reset: true,
	unknownRows: 0,
	malformedRows: 0,
	updates: [{ sessionUpdate: "agent_message_chunk", messageId: `answer-${epoch}`, content: { type: "text", text } }],
});
beforeEach(() => {
	vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
	chatViewStore.reset();
	invoke.mockReset();
	warnings.mockClear();
	release = vi.fn();
	listen.mockImplementation(async (_event, fn) => {
		wake = () => fn({ payload: { session_id: "contract" } });
		return release;
	});
	invoke.mockResolvedValue(snapshot("First answer"));
});
afterEach(() => {
	cleanup();
	vi.useRealTimers();
	vi.unstubAllGlobals();
	for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
});

function mount(host: "desktop" | "mobile") {
	const [agent, setAgent] = createSignal<string | undefined>("claude");
	const [busy, setBusy] = createSignal(false);
	const [awaiting, setAwaiting] = createSignal(false);
	const [revision, setRevision] = createSignal(0);
	if (host === "desktop") {
		const id = terminalsStore.add({
			sessionId: "contract",
			cwd: "/repo",
			name: "Claude",
			fontSize: 14,
			awaitingInput: null,
			agentType: "claude",
			agentSessionId: null,
		});
		const view = render(() => (
			<>
				<ViewModeToggle terminalId={id} />
				<TerminalChatView terminalId={id} sessionId="contract" />
			</>
		));
		return {
			...view,
			exit: () => terminalsStore.update(id, { agentType: null }),
			mode: () => terminalsStore.get(id)?.viewMode,
			busy: (value: boolean) => terminalsStore.update(id, { shellState: value ? "busy" : "idle" }),
			awaiting: (value: boolean) =>
				terminalsStore.update(id, {
					awaitingInput: value ? "question" : null,
					questionText: value ? "Approve command?" : null,
				}),
			output: () => terminalsStore.update(id, { lastDataAt: 42 }),
		};
	}
	const view = render(() => {
		const chat = useSessionChatView({
			sessionId: () => "contract",
			agentType: agent,
			sessionExists: () => true,
			busy,
			awaitingInput: awaiting,
			activityRevision: revision,
			subscribeWake: (_id, fn) => {
				wake = fn;
				return release;
			},
		});
		return (
			<>
				<chat.Toggle />
				<chat.View />
			</>
		);
	});
	return {
		...view,
		exit: () => setAgent(undefined),
		mode: () => (view.queryByText("CLI")?.getAttribute("aria-selected") === "false" ? "chat" : "cli"),
		busy: setBusy,
		awaiting: setAwaiting,
		output: () => setRevision((value) => value + 1),
	};
}

// Desktop-only file navigation/search are covered by TerminalChatView.test and chatViewSearch.test.
describe.each(["desktop", "mobile"] as const)("%s terminal chat contract", (host) => {
	// Catches: sending leaves Chat blank until a transcript message or busy event arrives.
	it("shows working from submission, through busy, and removes it on idle", async () => {
		const view = mount(host);
		view.getByText("Chat").click();
		await waitFor(() => expect(view.container.textContent).toContain("First answer"));
		expect(view.queryByText("Working…")).toBeNull();
		chatViewStore.beginSubmission("contract");
		expect(view.getByRole("status").textContent).toBe("Working…");
		// An unchanged keepalive is not evidence of output or completion.
		wake();
		await waitFor(() => expect(invoke.mock.calls.length).toBeGreaterThan(1));
		expect(view.getByText("Working…")).toBeTruthy();
		view.busy(true);
		expect(view.getByText("Working…")).toBeTruthy();
		view.busy(false);
		expect(view.queryByText("Working…")).toBeNull();
	});
	// Catches: optimistic feedback sticks after output arrives without a busy transition.
	it("ends the submission bridge at the first output", async () => {
		const view = mount(host);
		view.getByText("Chat").click();
		await waitFor(() => expect(view.container.textContent).toContain("First answer"));
		chatViewStore.beginSubmission("contract");
		expect(view.getByText("Working…")).toBeTruthy();
		view.output();
		expect(view.queryByText("Working…")).toBeNull();
	});
	// Catches: the working row competes with an awaiting question or returns after it clears.
	it("gives awaiting input precedence over busy and pending submission", async () => {
		const view = mount(host);
		view.getByText("Chat").click();
		await waitFor(() => expect(view.container.textContent).toContain("First answer"));
		chatViewStore.beginSubmission("contract");
		view.busy(true);
		view.awaiting(true);
		expect(view.queryByText("Working…")).toBeNull();
		if (host === "desktop") expect(view.getByText("Approve command?")).toBeTruthy();
		view.busy(false);
		view.awaiting(false);
		expect(view.queryByText("Working…")).toBeNull();
	});
	// Catches: an earlier failed send cancels a newer send, or failed sends remain working.
	it("rolls back only the failed submission and clears feedback when Chat closes", async () => {
		const view = mount(host);
		view.getByText("Chat").click();
		await waitFor(() => expect(view.container.textContent).toContain("First answer"));
		const oldFailure = chatViewStore.beginSubmission("contract");
		const latestFailure = chatViewStore.beginSubmission("contract");
		oldFailure();
		expect(view.getByText("Working…")).toBeTruthy();
		latestFailure();
		expect(view.queryByText("Working…")).toBeNull();
		chatViewStore.beginSubmission("contract");
		view.unmount();
		expect(chatViewStore.state.submissions.contract).toBeUndefined();
	});

	// Catches: a shared transcript hint prop/default reintroduces invented guidance.
	it("renders Chat without invented guidance", async () => {
		const view = mount(host);
		view.getByText("Chat").click();
		await waitFor(() => expect(view.container.textContent).toContain("First answer"));
		expect(view.container.textContent).not.toMatch(/Reply below|Switch to CLI|Read file edits in Chat/);
	});
	// Catches: frontend missing-id gate prevents backend-owned binding; epoch reset leaves old prose visible.
	it("opens Claude without a discovered id and replaces the old epoch on a wake", async () => {
		const view = mount(host);
		const chat = view.getByText("Chat") as HTMLButtonElement;
		expect(chat.disabled).toBe(false);
		chat.click();
		await waitFor(() => expect(view.container.textContent).toContain("First answer"));
		invoke.mockResolvedValue(snapshot("New conversation", 2));
		wake();
		await waitFor(() => expect(view.container.textContent).toContain("New conversation"));
		expect(view.container.textContent).not.toContain("First answer");
		view.unmount();
		expect(release).toHaveBeenCalled();
		const reads = invoke.mock.calls.length;
		vi.advanceTimersByTime(10_000);
		wake();
		expect(invoke.mock.calls.length).toBe(reads);
	});
	// Catches: HTTP not_bound is treated as an ordinary error and leaves a blank Chat panel.
	it("returns to CLI with the backend reason from an HTTP not_bound response", async () => {
		invoke.mockRejectedValue(
			new HttpRpcError("chat_view_snapshot", 500, '{"error":"not_bound: transcript unavailable"}'),
		);
		const view = mount(host);
		view.getByText("Chat").click();
		await waitFor(() => expect(view.mode()).toBe("cli"));
		expect(chatViewStore.unavailableReason("contract")).toBe("transcript unavailable");
		expect(warnings).toHaveBeenCalledWith(
			"terminal",
			"Chat view refresh failed",
			expect.objectContaining({ sessionId: "contract" }),
		);
	});
	// Catches: terminal death leaves the host stuck in Chat.
	it("returns to CLI when the agent exits", async () => {
		const view = mount(host);
		view.getByText("Chat").click();
		await waitFor(() => expect(view.container.textContent).toContain("First answer"));
		view.exit();
		await waitFor(() => expect(view.mode()).toBe("cli"));
	});
	// Catches: hidden page keeps polling or misses the full reset on resume.
	it("pauses hidden reads and catches up immediately on return", async () => {
		let hidden = false;
		Object.defineProperty(document, "visibilityState", {
			configurable: true,
			get: () => (hidden ? "hidden" : "visible"),
		});
		const view = mount(host);
		view.getByText("Chat").click();
		await waitFor(() => expect(view.container.textContent).toContain("First answer"));
		const reads = invoke.mock.calls.length;
		hidden = true;
		fireEvent(document, new Event("visibilitychange"));
		vi.advanceTimersByTime(15_000);
		wake();
		expect(invoke.mock.calls.length).toBe(reads);
		invoke.mockResolvedValue(snapshot("Resumed answer", 3));
		hidden = false;
		fireEvent(document, new Event("visibilitychange"));
		await waitFor(() => expect(view.container.textContent).toContain("Resumed answer"));
	});
});

// Catches: one host using the raw transcript instead of the shared pending projection.
for (const host of ["desktop", "mobile"] as const) {
	it(`${host} shows queued sends muted, then replaces the pending bubble without duplication`, async () => {
		const view = mount(host);
		fireEvent.click(view.getByText("Chat"));
		await waitFor(() => expect(view.getByText("First answer")).toBeTruthy());
		view.busy(true);
		chatViewStore.beginSubmission("contract", "Review the tests");
		chatViewStore.beginSubmission("contract", "Then check the docs");
		await waitFor(() => expect(view.getAllByLabelText("Pending prompt")).toHaveLength(2));
		expect(view.getAllByLabelText("Pending prompt").map((element) => element.textContent)).toEqual([
			"PendingReview the testsCopy",
			"PendingThen check the docsCopy",
		]);
		expect(view.getAllByLabelText("Pending prompt").every((element) => element.hasAttribute("data-pending"))).toBe(
			true,
		);
		invoke.mockResolvedValue({
			epoch: 1,
			nextSeq: 2,
			reset: false,
			unknownRows: 0,
			malformedRows: 0,
			updates: [
				{
					sessionUpdate: "user_message_chunk",
					messageId: "submitted",
					content: { type: "text", text: "Review the tests" },
				},
			],
		});
		wake();
		await waitFor(() => expect(view.getAllByLabelText("Pending prompt")).toHaveLength(1));
		expect(view.getAllByText("Review the tests")).toHaveLength(1);
	});
}
