// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeAll, beforeEach, expect, it, vi } from "vitest";

const { snapshotMock } = vi.hoisted(() => ({ snapshotMock: vi.fn() }));
vi.mock("../../../invoke", () => ({
	invoke: (command: string, ...args: unknown[]) =>
		command === "list_queued_agent_commands" ? Promise.resolve([]) : snapshotMock(command, ...args),
	listen: async () => () => {},
}));

import { feedbackMessage, quoteMessage } from "../../../components/AIChatPanel/MessageMenu";
import { TerminalChatTranscript } from "../../../components/Terminal/TerminalChatTranscript";
import { acpTranscript } from "../../../stores/acpTranscript";
import { chatViewStore } from "../../../stores/chatView";

beforeEach(() => {
	chatViewStore.reset();
	acpTranscript.reset();
	snapshotMock.mockReset();
	snapshotMock.mockResolvedValue({
		epoch: 0,
		nextSeq: 2,
		reset: true,
		unknownRows: 0,
		malformedRows: 0,
		updates: [
			{ sessionUpdate: "user_message_chunk", messageId: "q", content: { type: "text", text: "Why?" } },
			{ sessionUpdate: "agent_message_chunk", messageId: "r", content: { type: "text", text: "Because." } },
		],
	});
});
afterEach(cleanup);

// Catches: terminal Chat (desktop and mobile share this view) lacking quote reply and feedback.
it.each([false, true])(
	"terminal chat drafts quotes and feedback through the host composer (mobile=%s)",
	async (mobile) => {
		const onQuote = vi.fn();
		const ui = render(() => (
			<TerminalChatTranscript
				sessionId="s"
				mobile={mobile}
				terminalAgent="claude"
				onQuote={onQuote}
				onUnavailable={() => {}}
			/>
		));
		await waitFor(() => expect(ui.getByLabelText("Chat transcript").textContent).toContain("Because."));
		fireEvent.click(ui.getByRole("button", { name: "Assistant message actions" }));
		fireEvent.click(screen.getByRole("menuitem", { name: "Quote reply" }));
		expect(onQuote).toHaveBeenLastCalledWith(quoteMessage("Because."));
		fireEvent.click(ui.getByRole("button", { name: "Assistant message actions" }));
		fireEvent.click(screen.getByRole("menuitem", { name: "Approve response" }));
		expect(onQuote).toHaveBeenLastCalledWith(feedbackMessage("Because.", "approve"));
	},
);

// Settle the shared UI persistence owner imported by the control utilities.
import { activityStore } from "../../../stores/activityStore";

beforeAll(() => activityStore.hydrate());
