import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

const backend = vi.hoisted(() => ({
	invoke: vi.fn(),
	queue: [{ id: 17, text: "Review the tests", kind: "user_command" }],
}));
vi.mock("../../../invoke", () => ({ invoke: backend.invoke, listen: async () => () => {} }));

import { TerminalChatView } from "../../../components/Terminal/TerminalChatView";
import { appLogger } from "../../../stores/appLogger";
import { chatViewStore } from "../../../stores/chatView";
import { terminalsStore } from "../../../stores/terminals";

beforeEach(() => {
	vi.spyOn(appLogger, "debug").mockImplementation(() => {});
	chatViewStore.reset();
	for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
	backend.queue = [{ id: 17, text: "Review the tests", kind: "user_command" }];
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
function view(agentType: "claude" | "grok" = "claude") {
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
// Catches: exposing destructive keyboard injection for an unsupported CLI.
it("unsupported_agent_has_cancel_but_no_steer", async () => {
	const screen = view("grok");
	const cancel = await screen.findByRole("button", { name: "Cancel pending prompt" });
	expect(screen.queryByRole("button", { name: "Send now: stop the current request" })).toBeNull();
	fireEvent.click(cancel);
	await waitFor(() => expect(screen.queryByLabelText("Pending prompt")).toBeNull());
	expect(backend.invoke).toHaveBeenCalledWith("remove_queued_agent_command", { sessionId: "pending", commandId: 17 });
});
