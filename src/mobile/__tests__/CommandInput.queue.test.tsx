import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";
import { CommandInput } from "../components/CommandInput";

vi.mock("../../transport", () => ({
	rpc: vi.fn(() => Promise.resolve({ accepted: true, typed: false })),
	HttpRpcError: class extends Error {},
}));
vi.mock("../../stores/toasts", () => ({ toastsStore: { add: vi.fn() } }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn() } }));

import { toastsStore } from "../../stores/toasts";
import { rpc } from "../../transport";

afterEach(() => {
	cleanup();
	vi.useRealTimers();
	vi.clearAllMocks();
});
it.each(["touch", "mouse"])(
	"catches accidental Enter after a %s long press and transfers the owned draft",
	async (pointerType) => {
		vi.useFakeTimers();
		const view = render(() => <CommandInput sessionId="busy" agentType="claude" />);
		const input = view.container.querySelector("textarea")!;
		fireEvent.input(input, { target: { value: "next task" } });
		const send = view.getByLabelText("Send");
		fireEvent.pointerDown(send, { button: 0, pointerType });
		await vi.advanceTimersByTimeAsync(500);
		fireEvent.pointerUp(send);
		fireEvent.click(send);
		expect(view.getByRole("menuitem").textContent).toBe("Queue");
		expect(vi.mocked(rpc).mock.calls.some(([, args]) => args?.data === "\r")).toBe(false);
		fireEvent.click(view.getByRole("menuitem"));
		await vi.advanceTimersByTimeAsync(0);
		expect(rpc).toHaveBeenCalledWith("enqueue_agent_command", {
			sessionId: "busy",
			text: "next task",
			expectedDraft: "next task",
		});
		expect(input.value).toBe("");
		expect(toastsStore.add).toHaveBeenCalledWith("Prompt queued", "Waiting for the next ready window", "info");
	},
);
it("catches a short tap opening Queue instead of submitting", async () => {
	vi.useFakeTimers();
	const view = render(() => <CommandInput sessionId="busy" />);
	fireEvent.input(view.container.querySelector("textarea")!, { target: { value: "now" } });
	fireEvent.pointerDown(view.getByLabelText("Send"), { button: 0 });
	fireEvent.pointerUp(view.getByLabelText("Send"));
	fireEvent.click(view.getByLabelText("Send"));
	await vi.advanceTimersByTimeAsync(600);
	expect(view.queryByRole("menu")).toBeNull();
	expect(rpc).toHaveBeenCalledWith("write_pty", { sessionId: "busy", data: "\r" });
});
it("catches draft loss or editable input during a failed queue request", async () => {
	vi.useFakeTimers();
	let reject!: (error: Error) => void;
	vi.mocked(rpc).mockImplementation((command) =>
		command === "enqueue_agent_command"
			? new Promise((_, fail) => {
					reject = fail;
				})
			: (Promise.resolve(undefined) as never),
	);
	const view = render(() => <CommandInput sessionId="busy" />);
	const input = view.container.querySelector("textarea")!;
	fireEvent.input(input, { target: { value: "keep me" } });
	fireEvent.pointerDown(view.getByLabelText("Send"), { button: 0 });
	await vi.advanceTimersByTimeAsync(500);
	fireEvent.pointerUp(view.getByLabelText("Send"));
	fireEvent.click(view.getByRole("menuitem"));
	await vi.advanceTimersByTimeAsync(0);
	expect(input.disabled).toBe(true);
	expect((view.getByLabelText("Send") as HTMLButtonElement).disabled).toBe(true);
	reject(new Error("Composer draft changed"));
	await vi.advanceTimersByTimeAsync(0);
	expect(input.value).toBe("keep me");
	expect(input.disabled).toBe(false);
	expect(vi.mocked(rpc).mock.calls.filter(([command]) => command === "enqueue_agent_command")).toHaveLength(1);
	expect(toastsStore.add).toHaveBeenCalledWith(
		"Queue failed",
		expect.stringContaining("check the session"),
		"error",
		true,
	);
});

// Catches: Save reaching the PTY as XOFF or submitting rather than parking.
it("parks and swaps mobile drafts without sending Enter or XOFF", async () => {
	const view = render(() => <CommandInput sessionId="park-mobile" agentType="claude" chatMode />);
	const input = view.container.querySelector("textarea")!;
	fireEvent.input(input, { target: { value: "first draft" } });
	fireEvent.keyDown(input, { key: "s", ctrlKey: true });
	expect(input.value).toBe("");
	fireEvent.input(input, { target: { value: "second draft" } });
	fireEvent.keyDown(input, { key: "s", metaKey: true });
	expect(input.value).toBe("first draft");
	fireEvent.keyDown(input, { key: "s", ctrlKey: true, repeat: true });
	expect(input.value).toBe("first draft");
	await Promise.resolve();
	expect(vi.mocked(rpc).mock.calls.some(([, args]) => args?.data === "\r" || args?.data === "\x13")).toBe(false);
});
