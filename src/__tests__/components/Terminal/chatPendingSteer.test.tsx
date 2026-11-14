import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeAll, beforeEach, expect, it, vi } from "vitest";

type Queued = { id: number; text: string; kind: string; send_now?: "interrupt" };
const backend = vi.hoisted(() => ({
	invoke: vi.fn(),
	queue: [] as Queued[],
}));
vi.mock("../../../invoke", () => ({ invoke: backend.invoke, listen: async () => () => {} }));

import { TerminalChatView } from "../../../components/Terminal/TerminalChatView";
import { appLogger } from "../../../stores/appLogger";
import { chatViewStore } from "../../../stores/chatView";
import { terminalsStore } from "../../../stores/terminals";

// Complete the real store startup promise; otherwise this isolated UI test leaks it.
beforeAll(async () => {
	const { activityStore } = await import("../../../stores/activityStore");
	await activityStore.hydrate();
});

beforeEach(() => {
	vi.spyOn(appLogger, "debug").mockImplementation(() => {});
	vi.spyOn(appLogger, "info").mockImplementation(() => {});
	chatViewStore.reset();
	for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
	backend.queue = [{ id: 17, text: "Review the tests", kind: "user_command", send_now: "interrupt" }];
	backend.invoke.mockReset().mockImplementation(async (cmd: string) => {
		if (cmd === "list_queued_agent_commands") return [...backend.queue];
		if (cmd === "send_queued_agent_command_now" || cmd === "remove_queued_agent_command") {
			const claimed = backend.queue.length > 0;
			backend.queue = [];
			return claimed;
		}
		return { epoch: 1, nextSeq: 0, reset: true, updates: [], unknownRows: 0, malformedRows: 0 };
	});
});
afterEach(cleanup);
function view(agentType: "claude" | "codex" | "grok" = "codex") {
	const id = terminalsStore.add({
		sessionId: "pending",
		name: "test",
		fontSize: 14,
		cwd: "/repo",
		awaitingInput: null,
		agentType,
		agentSessionId: "bound",
	});
	terminalsStore.update(id, { shellState: "busy" });
	return render(() => <TerminalChatView terminalId={id} sessionId="pending" />);
}
// Catches: replaying a local receipt instead of claiming its backend queue id, or double clicking twice.
it("pending_steer_claims_one_item_and_clears_the_bubble", async () => {
	const screen = view();
	const action = await screen.findByRole("button", { name: "Send now: stop the current request" });
	fireEvent.click(action);
	fireEvent.click(action);
	await waitFor(() => expect(screen.queryByLabelText("Pending prompt")).toBeNull());
	expect(backend.invoke.mock.calls.filter(([cmd]) => cmd === "send_queued_agent_command_now")).toEqual([
		["send_queued_agent_command_now", { sessionId: "pending", commandId: 17 }],
	]);
	expect(backend.invoke.mock.calls.some(([cmd]) => cmd === "write_pty")).toBe(false);
});
// Catches: resubmitting text after the idle drain wins the queue claim.
it("idle_drain_winning_the_click_never_falls_back_to_text_submission", async () => {
	const screen = view();
	const action = await screen.findByRole("button", { name: "Send now: stop the current request" });
	backend.queue = [];
	fireEvent.click(action);
	await waitFor(() => expect(screen.queryByLabelText("Pending prompt")).toBeNull());
	expect(backend.invoke.mock.calls.some(([cmd]) => cmd === "write_pty" || cmd === "enqueue_agent_command")).toBe(false);
});
// Catches: the tooltip promising the request continues while the harness interrupts it, or the reverse.
it("send_now_tooltip_follows_the_backend_declared_mode", async () => {
	backend.queue = [{ id: 17, text: "Review the tests", kind: "user_command", send_now: "interrupt" }];
	const screen = view("claude");
	const action = await screen.findByRole("button", {
		name: "Send now: stop the current request",
	});
	expect(action.getAttribute("title")).toBe("Send now: stop the current request");
	fireEvent.click(action);
	await waitFor(() => expect(screen.queryByLabelText("Pending prompt")).toBeNull());
	expect(backend.invoke).toHaveBeenCalledWith("send_queued_agent_command_now", { sessionId: "pending", commandId: 17 });
});
// Catches: exposing destructive keyboard injection for an unsupported CLI.
it("unsupported_agent_has_cancel_but_no_steer", async () => {
	backend.queue = [{ id: 17, text: "Review the tests", kind: "user_command" }];
	const screen = view("grok");
	const cancel = await screen.findByRole("button", { name: "Cancel pending prompt" });
	expect(screen.queryByRole("button", { name: "Send now: stop the current request" })).toBeNull();
	fireEvent.click(cancel);
	await waitFor(() => expect(screen.queryByLabelText("Pending prompt")).toBeNull());
	expect(backend.invoke).toHaveBeenCalledWith("remove_queued_agent_command", { sessionId: "pending", commandId: 17 });
});
// Catches: queued attachment markers leaking into linked text instead of preview frames.
it.each([1, 2, 3])("pending prompt renders %i image previews above clean prose", async (count) => {
	backend.queue = [
		{
			id: 17,
			text: `${Array.from({ length: count }, (_, i) => `[image: /repo/image ${i}.png]`).join("\n")}\nReview these images`,
			kind: "user_command",
		},
	];
	const screen = view();
	const pending = await screen.findByLabelText("Pending prompt");
	expect(pending.querySelectorAll("img")).toHaveLength(count);
	expect(pending.textContent).not.toContain("[image:");
	expect(pending.textContent).toContain("Review these images");
	const gallery = pending.querySelector("[data-prompt-images]");
	expect(gallery?.getAttribute("data-prompt-images")).toBe(count === 1 ? "single" : count === 2 ? "pair" : "grid");
	const first = pending.querySelector("img")!;
	fireEvent.error(first);
	expect(pending.querySelectorAll("img")).toHaveLength(count - 1);
	expect(pending.textContent).toContain("Image unavailable");
});
