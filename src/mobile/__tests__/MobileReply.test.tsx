import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import codexQuestion from "../../../src-tauri/src/fixtures/choice_prompts/codex-request-user-input.json";
import { HttpRpcError } from "../../transport";
import { waitForAgentEnterGap } from "../../utils/sendCommand";
import { CommandInput } from "../components/CommandInput";

const { rpc } = vi.hoisted(() => ({
	rpc: vi.fn(async (_command: string, _args: Record<string, unknown>) => ({
		status: "acknowledged",
		submitted: true,
		acknowledged: true,
	})),
}));
vi.mock("../../transport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../transport")>()),
	rpc,
}));
const { toastAdd } = vi.hoisted(() => ({ toastAdd: vi.fn() }));
vi.mock("../../stores/toasts", () => ({ toastsStore: { add: toastAdd } }));

afterEach(() => {
	cleanup();
	rpc.mockClear();
	toastAdd.mockClear();
});

describe("mobile managed-agent reply", () => {
	it("sends the complete answer once after the user submits it", async () => {
		const { container } = render(() => (
			<CommandInput sessionId="question-session" agentType="claude" awaitingInput={true} managedSession={true} />
		));
		const input = container.querySelector("textarea")!;
		await fireEvent.input(input, { target: { value: "Please wait for me" } });
		expect(rpc).not.toHaveBeenCalled();
		await fireEvent.click(screen.getByRole("button", { name: "Send" }));
		await waitFor(() => expect(rpc).toHaveBeenCalledTimes(1));
		expect(rpc).toHaveBeenCalledWith("submit_agent_reply", {
			sessionId: "question-session",
			input: "Please wait for me",
		});
	});

	it("does not write any answer to a closed session", async () => {
		const { container } = render(() => (
			<CommandInput
				sessionId="closed-session"
				agentType="claude"
				awaitingInput={true}
				managedSession={true}
				sessionExists={false}
			/>
		));
		await fireEvent.input(container.querySelector("textarea")!, { target: { value: "yes" } });
		await fireEvent.click(screen.getByRole("button", { name: "Send" }));
		expect(rpc).not.toHaveBeenCalled();
	});

	it("keeps an answer editable when the session rejects it", async () => {
		rpc.mockImplementationOnce(async () => ({ status: "rejected", submitted: false, acknowledged: false }));
		const { container } = render(() => (
			<CommandInput sessionId="busy-session" agentType="claude" awaitingInput={true} managedSession={true} />
		));
		const input = container.querySelector("textarea")!;
		await fireEvent.input(input, { target: { value: "Wait for approval" } });
		await fireEvent.click(screen.getByRole("button", { name: "Send" }));
		await waitFor(() => expect(rpc).toHaveBeenCalledTimes(1));
		expect(input.value).toBe("Wait for approval");
	});

	it("explains a closed-session HTTP rejection without losing the answer", async () => {
		rpc.mockRejectedValueOnce(
			new HttpRpcError("submit_agent_reply", 404, '{"submitted":false,"reason":"session_not_found"}'),
		);
		const { container } = render(() => (
			<CommandInput sessionId="closed-session" agentType="claude" awaitingInput={true} managedSession={true} />
		));
		const input = container.querySelector("textarea")!;
		await fireEvent.input(input, { target: { value: "Wait for me" } });
		await fireEvent.click(screen.getByRole("button", { name: "Send" }));
		await waitFor(() =>
			expect(toastAdd).toHaveBeenCalledWith("Reply not sent", "This session has ended", "error", true),
		);
		expect(input.value).toBe("Wait for me");
	});

	it("explains an unclaimable waiting composer without calling it busy", async () => {
		rpc.mockRejectedValueOnce(
			new HttpRpcError("submit_agent_reply", 409, '{"submitted":false,"reason":"agent_not_ready"}'),
		);
		const { container } = render(() => (
			<CommandInput sessionId="busy-session" agentType="claude" awaitingInput={true} managedSession={true} />
		));
		const input = container.querySelector("textarea")!;
		await fireEvent.input(input, { target: { value: "Wait for me" } });
		await fireEvent.click(screen.getByRole("button", { name: "Send" }));
		await waitFor(() =>
			expect(toastAdd).toHaveBeenCalledWith(
				"Reply not sent",
				"The agent is waiting for input. Check its question before retrying.",
				"error",
				true,
			),
		);
		expect(input.value).toBe("Wait for me");
	});

	// Catches: replacing an authoritative refusal detail with a generic busy toast.
	it.each(["awaiting_input", "selection_required", "agent_not_ready"])(
		"shows the actual %s refusal while retaining the human answer",
		async (reason) => {
			const detail = "This question requires an option selection. Open the question and use its answer controls.";
			rpc.mockRejectedValueOnce(
				new HttpRpcError("submit_agent_reply", 409, JSON.stringify({ submitted: false, reason, detail })),
			);
			const { container } = render(() => (
				<CommandInput sessionId="waiting" agentType="ego" awaitingInput managedSession />
			));
			const input = container.querySelector("textarea")!;
			await fireEvent.input(input, { target: { value: "My answer" } });
			await fireEvent.click(screen.getByRole("button", { name: "Send" }));
			await waitFor(() => expect(toastAdd).toHaveBeenCalledWith("Reply not sent", detail, "error", true));
			expect(input.value).toBe("My answer");
		},
	);

	it("does not submit the same answer twice while the first receipt is pending", async () => {
		let finish!: (receipt: { status: string; submitted: boolean; acknowledged: boolean }) => void;
		rpc.mockImplementationOnce(
			() =>
				new Promise((resolve) => {
					finish = resolve;
				}),
		);
		const { container } = render(() => (
			<CommandInput sessionId="question-session" agentType="claude" awaitingInput={true} managedSession={true} />
		));
		await fireEvent.input(container.querySelector("textarea")!, { target: { value: "yes" } });
		const send = screen.getByRole("button", { name: "Send" });
		await fireEvent.click(send);
		await fireEvent.click(send);
		expect(rpc).toHaveBeenCalledTimes(1);
		finish({ status: "acknowledged", submitted: true, acknowledged: true });
		await waitFor(() => expect(container.querySelector("textarea")!.value).toBe(""));
	});

	it("does not leak editing keys to a waiting managed agent", async () => {
		const { container } = render(() => (
			<CommandInput sessionId="question-session" agentType="claude" awaitingInput={true} managedSession={true} />
		));
		const input = container.querySelector("textarea")!;
		await fireEvent.input(input, { target: { value: "draft" } });
		await fireEvent.keyDown(input, { key: "Tab" });
		await fireEvent.keyDown(input, { key: "Escape" });
		expect(rpc).not.toHaveBeenCalled();
	});

	it("keeps numbered choice prompts on the key input path", async () => {
		const { container } = render(() => (
			<CommandInput
				sessionId="choice-session"
				agentType="claude"
				awaitingInput={true}
				managedSession={true}
				choicePrompt={{
					title: "Choose one",
					options: [{ key: "1", label: "Approve", highlighted: false, destructive: false }],
				}}
			/>
		));
		await fireEvent.click(
			Array.from(container.querySelectorAll("button")).find((button) => button.textContent?.includes("Approve"))!,
		);
		expect(rpc.mock.calls.some(([command]) => command === "submit_agent_reply")).toBe(false);
		expect(rpc.mock.calls.some(([command, args]) => command === "write_pty" && args.data === "1")).toBe(true);
	});

	// Catches: a captured Codex option is followed by an extra Enter or sent twice.
	it("submits a Codex question option once without an extra Enter", async () => {
		const { container } = render(() => (
			<CommandInput
				sessionId="codex-question"
				agentType="codex"
				awaitingInput={true}
				managedSession={true}
				choicePrompt={codexQuestion}
			/>
		));
		const option = Array.from(container.querySelectorAll("button")).find((button) =>
			button.textContent?.includes("Blue"),
		)!;
		await fireEvent.click(option);
		await fireEvent.click(option);
		await waitFor(() => expect(rpc).toHaveBeenCalledTimes(1));
		expect(rpc).toHaveBeenCalledWith("write_pty", { sessionId: "codex-question", data: "3" });
	});

	// Catches: a Claude Ink picker receives a numeric key without the Enter its footer requires.
	it("moves to and selects the second captured Claude AskUserQuestion option once", async () => {
		const { container } = render(() => (
			<CommandInput
				sessionId="claude-question"
				agentType="claude"
				awaitingInput={true}
				managedSession={true}
				choicePrompt={{
					title: "Which color do you prefer?",
					options: [
						{ key: "1", label: "Red", highlighted: true, destructive: false },
						{ key: "2", label: "Green", highlighted: false, destructive: false },
						{ key: "3", label: "Blue", highlighted: false, destructive: false },
						{ key: "4", label: "Type something.", highlighted: false, destructive: false },
						{ key: "5", label: "Chat about this", highlighted: false, destructive: false },
					],
					dismiss_key: "cancel",
					selection_mode: "navigate-enter",
				}}
			/>
		));
		const green = Array.from(container.querySelectorAll("button")).find((button) =>
			button.textContent?.includes("Green"),
		)!;
		await fireEvent.click(green);
		await fireEvent.click(green);
		await waitFor(() => expect(rpc).toHaveBeenCalledTimes(2));
		expect(rpc.mock.calls.map(([, args]) => args.data)).toEqual(["\x1b[B", "\r"]);
	});

	it("opens Codex Other notes before typing a free-form answer", async () => {
		const { container } = render(() => (
			<CommandInput
				sessionId="codex-other"
				agentType="codex"
				awaitingInput={true}
				managedSession={true}
				choicePrompt={codexQuestion}
			/>
		));
		await fireEvent.click(
			Array.from(container.querySelectorAll("button")).find((button) => button.textContent?.includes("Other"))!,
		);
		await waitFor(() => expect(rpc).toHaveBeenCalledWith("write_pty", { sessionId: "codex-other", data: "\t" }));
		expect(rpc.mock.calls.map(([, args]) => args.data)).toEqual(["\x1b[B", "\x1b[B", "\x1b[B", "\t"]);
		await fireEvent.input(container.querySelector("textarea")!, { target: { value: "Purple" } });
		await fireEvent.click(screen.getByRole("button", { name: "Send" }));
		await waitFor(() => expect(rpc.mock.calls.some(([, args]) => args.data === "\r")).toBe(true));
		await waitForAgentEnterGap("codex");
		expect(rpc.mock.calls.some(([command]) => command === "submit_agent_reply")).toBe(false);
	});
});
