import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
vi.mock("../../../invoke", () => ({
	invoke: (...args: unknown[]) => invokeMock(...args),
	listen: async () => () => {},
}));

import { TerminalChatTranscript } from "../../../components/Terminal/TerminalChatTranscript";
import { acpTranscript } from "../../../stores/acpTranscript";
import { chatViewStore } from "../../../stores/chatView";
import { HttpRpcError } from "../../../transport/http";

const queued = [{ id: 7, text: "owner correction", kind: "user_command", send_now: "interrupt" }];

beforeEach(() => {
	chatViewStore.reset();
	acpTranscript.reset();
	invokeMock.mockReset();
	invokeMock.mockImplementation((command: string) => {
		if (command === "list_queued_agent_commands") return Promise.resolve(queued);
		if (command === "send_queued_agent_command_now") {
			return Promise.reject(
				new HttpRpcError(
					command,
					500,
					JSON.stringify({ error: "Steer refused (idle_timeout). The prompt stays queued." }),
				),
			);
		}
		return Promise.resolve({ epoch: 0, nextSeq: 0, reset: true, updates: [], unknownRows: 0, malformedRows: 0 });
	});
});
afterEach(cleanup);

// Catches (#2913-8d0a): a refused steer shows the raw "RPC … failed: 500 {json}" transport
// text (mobile) or nothing useful, and the queued item stays disabled after the refusal.
it("a refused steer shows the backend reason and leaves the item steerable", async () => {
	const view = render(() => (
		<TerminalChatTranscript sessionId="steer" mobile terminalAgent="claude" onUnavailable={() => {}} />
	));
	const steer = await view.findByRole("button", { name: /interrupt|send now|steer/i });
	fireEvent.click(steer);
	const alert = await view.findByRole("alert");
	expect(alert.textContent).toBe("Steer refused (idle_timeout). The prompt stays queued.");
	await waitFor(() =>
		expect((view.getByRole("button", { name: /interrupt|send now|steer/i }) as HTMLButtonElement).disabled).toBe(false),
	);
});
