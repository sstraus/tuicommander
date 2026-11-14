// @vitest-environment jsdom
import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeAll, beforeEach, expect, it, vi } from "vitest";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
vi.mock("../../../invoke", () => ({
	invoke: (command: string, ...args: unknown[]) =>
		command === "list_queued_agent_commands" ? Promise.resolve([]) : invokeMock(command, ...args),
	listen: async () => () => {},
}));

import { TerminalChatTranscript } from "../../../components/Terminal/TerminalChatTranscript";
import { acpTranscript } from "../../../stores/acpTranscript";
import { activityStore } from "../../../stores/activityStore";
import { chatViewStore } from "../../../stores/chatView";

const FULL = "ab".repeat(40_000);
const CAPPED = `${FULL.slice(0, 19_000)}… [truncated, 80000 chars total]`;

beforeAll(() => activityStore.hydrate());
beforeEach(() => {
	chatViewStore.reset();
	acpTranscript.reset();
	invokeMock.mockReset();
	invokeMock.mockImplementation((command: string) =>
		command === "chat_view_full_text"
			? Promise.resolve({ text: FULL })
			: Promise.resolve({
					epoch: 0,
					nextSeq: 2,
					reset: true,
					unknownRows: 0,
					malformedRows: 0,
					updates: [
						{ sessionUpdate: "user_message_chunk", messageId: "u-1", content: { type: "text", text: CAPPED } },
						{ sessionUpdate: "user_message_chunk", messageId: "u-2", content: { type: "text", text: "short" } },
					],
				}),
	);
});
afterEach(cleanup);

// Catches: full text read from the capped copy (the panel must show what the backend returns, not the stream text).
it.each([false, true])("a capped message opens its full text (mobile=%s)", async (mobile) => {
	const ui = render(() => (
		<TerminalChatTranscript sessionId="s" mobile={mobile} terminalAgent="claude" onUnavailable={() => {}} />
	));
	const button = await waitFor(() => ui.getByRole("button", { name: "Show full text" }));
	expect(ui.queryAllByRole("button", { name: "Show full text" })).toHaveLength(1);
	fireEvent.click(button);
	const body = await waitFor(() => ui.getByTestId("chat-full-text"));
	expect(body.textContent).toHaveLength(80_000);
	expect(body.textContent).not.toContain("truncated");
	expect(invokeMock).toHaveBeenCalledWith("chat_view_full_text", { sessionId: "s", messageId: "u-1" });
});
