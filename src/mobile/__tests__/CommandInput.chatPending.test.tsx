import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, expect, it, vi } from "vitest";

const backend = vi.hoisted(() => ({
	rpc: vi.fn(async (_command: string, _args?: Record<string, unknown>) => ({
		accepted: true,
		typed: false,
		queued: 1,
	})),
}));
vi.mock("../../transport", () => ({
	rpc: backend.rpc,
	buildHttpUrl: (p: string) => p,
	HttpRpcError: class extends Error {},
}));
vi.mock("../../invoke", () => ({
	invoke: async (cmd: string) => (cmd === "list_queued_agent_commands" ? [] : undefined),
	listen: async () => () => {},
}));

import { CommandInput } from "../components/CommandInput";

afterEach(() => {
	cleanup();
	backend.rpc.mockClear();
});
// Catches: the phone streaming a busy Chat draft to the native composer before queue ownership.
it("mobile_busy_chat_send_queues_without_typing_the_draft", async () => {
	const props = { sessionId: "mobile-pending", agentType: "claude", chatMode: true, busy: true };
	const screen = render(() => <CommandInput {...props} />);
	fireEvent.input(screen.container.querySelector("textarea")!, { target: { value: "Review now" } });
	fireEvent.click(screen.getByRole("button", { name: "Send" }));
	await waitFor(() =>
		expect(backend.rpc).toHaveBeenCalledWith(
			"enqueue_agent_command",
			expect.objectContaining({ sessionId: "mobile-pending", text: "Review now" }),
		),
	);
	expect(backend.rpc.mock.calls.some(([cmd]) => cmd === "write_pty")).toBe(false);
	expect(screen.container.querySelector("textarea")?.value).toBe("");
});

function writes() {
	return backend.rpc.mock.calls.filter(([command]) => command === "write_pty").map(([, args]) => args?.data);
}

// Catches: a non-null empty native baseline makes CLI Send emit only Enter for a local Chat draft.
it("chat_to_cli_transfers_the_visible_draft_before_enter_after_a_previous_send", async () => {
	const [chat, setChat] = createSignal(true);
	const screen = render(() => <CommandInput sessionId="mode-send" agentType="claude" chatMode={chat()} busy />);
	const input = screen.container.querySelector("textarea")!;
	fireEvent.input(input, { target: { value: "first" } });
	fireEvent.click(screen.getByRole("button", { name: "Send" }));
	await waitFor(() => expect(input.value).toBe(""));
	fireEvent.input(input, { target: { value: "next" } });
	setChat(false);
	fireEvent.click(screen.getByRole("button", { name: "Send" }));
	await waitFor(() => expect(writes()).toEqual(["next", "\r"]));
});

// Catches: entering Chat leaves the formerly live CLI draft blocking its queued prompt.
it("cli_to_chat_withdraws_only_the_owned_live_draft_and_queues_the_local_edit", async () => {
	const [chat, setChat] = createSignal(false);
	const screen = render(() => <CommandInput sessionId="mode-queue" agentType="claude" chatMode={chat()} busy />);
	const input = screen.container.querySelector("textarea")!;
	fireEvent.input(input, { target: { value: "old" } });
	setChat(true);
	expect(input.value).toBe("old");
	await waitFor(() => expect(writes()).toEqual(["old", "\x7f\x7f\x7f"]));
	fireEvent.input(input, { target: { value: "new" } });
	fireEvent.click(screen.getByRole("button", { name: "Send" }));
	await waitFor(() => expect(input.value).toBe(""));
	expect(backend.rpc).toHaveBeenCalledWith("enqueue_agent_command", expect.objectContaining({ text: "new" }));
	expect(writes()).toEqual(["old", "\x7f\x7f\x7f"]);
});

// Catches: Tab reaches an empty native composer and its returned text remains native-owned behind a queued send.
it("chat_tab_lends_the_visible_draft_then_parks_the_completed_input_before_queueing", async () => {
	const [line, setLine] = createSignal("");
	const screen = render(() => (
		<CommandInput sessionId="chat-tab" agentType="claude" chatMode busy ptyInputLine={line()} />
	));
	const input = screen.container.querySelector("textarea")!;
	fireEvent.input(input, { target: { value: "/he" } });
	fireEvent.keyDown(input, { key: "Tab" });
	await waitFor(() => expect(writes()).toEqual(["/he", "\t"]));
	// input_line is TUIC's own transport contract, not a fabricated native screen fixture.
	setLine("/help");
	await waitFor(() => expect(input.value).toBe("/help"));
	fireEvent.click(screen.getByRole("button", { name: "Send" }));
	await waitFor(() => expect(input.value).toBe(""));
	expect(backend.rpc).toHaveBeenCalledWith("enqueue_agent_command", expect.objectContaining({ text: "/help" }));
	expect(writes()).toEqual(["/he", "\t", "\x7f\x7f\x7f\x7f\x7f"]);
});

// Catches: a view switch while enqueue awaits its receipt retypes the queued text into the native composer.
it("a_mode_switch_during_enqueue_does_not_copy_the_submitted_draft_into_cli", async () => {
	let accept!: () => void;
	const accepted = new Promise<void>((resolve) => {
		accept = resolve;
	});
	backend.rpc.mockImplementationOnce(async () => {
		await accepted;
		return { accepted: true, typed: false, queued: 1 };
	});
	const [chat, setChat] = createSignal(true);
	const screen = render(() => <CommandInput sessionId="queue-mode-race" agentType="claude" chatMode={chat()} busy />);
	const input = screen.container.querySelector("textarea")!;
	fireEvent.input(input, { target: { value: "once" } });
	fireEvent.click(screen.getByRole("button", { name: "Send" }));
	await waitFor(() =>
		expect(backend.rpc).toHaveBeenCalledWith("enqueue_agent_command", expect.objectContaining({ text: "once" })),
	);
	setChat(false);
	expect(writes()).toEqual([]);
	accept();
	await waitFor(() => expect(input.value).toBe(""));
	expect(writes()).toEqual([]);
});

// Catches: a delayed native draft transfer lets Tab overwrite edits made after the request.
it("editing_chat_while_tab_transfers_the_old_draft_cancels_native_navigation", async () => {
	let release!: () => void;
	const transfer = new Promise<void>((resolve) => {
		release = resolve;
	});
	backend.rpc.mockImplementationOnce(async () => {
		await transfer;
		return { accepted: true, typed: false, queued: 1 };
	});
	const screen = render(() => <CommandInput sessionId="chat-tab-edit" agentType="claude" chatMode busy />);
	const input = screen.container.querySelector("textarea")!;
	fireEvent.input(input, { target: { value: "/he" } });
	fireEvent.keyDown(input, { key: "Tab" });
	fireEvent.input(input, { target: { value: "different" } });
	release();
	await waitFor(() => expect(writes()).toEqual(["/he", "\x7f\x7f\x7f"]));
	expect(input.value).toBe("different");
	expect(writes()).not.toContain("\t");
});

// Catches: editing after Tab dispatch abandons a borrowed native draft and blocks the host FIFO.
it("editing_chat_after_tab_dispatch_withdraws_the_borrowed_native_draft", async () => {
	const [line, setLine] = createSignal("");
	const screen = render(() => (
		<CommandInput sessionId="chat-tab-dispatched-edit" agentType="claude" chatMode busy ptyInputLine={line()} />
	));
	const input = screen.container.querySelector("textarea")!;
	fireEvent.input(input, { target: { value: "/he" } });
	fireEvent.keyDown(input, { key: "Tab" });
	await waitFor(() => expect(writes()).toEqual(["/he", "\t"]));
	fireEvent.input(input, { target: { value: "different" } });
	await waitFor(() => expect(writes()).toEqual(["/he", "\t", "\x7f\x7f\x7f"]));
	setLine("/help");
	expect(input.value).toBe("different");
	fireEvent.click(screen.getByRole("button", { name: "Send" }));
	await waitFor(() => expect(input.value).toBe(""));
	expect(backend.rpc).toHaveBeenCalledWith("enqueue_agent_command", expect.objectContaining({ text: "different" }));
	expect(writes()).toEqual(["/he", "\t", "\x7f\x7f\x7f"]);
});
