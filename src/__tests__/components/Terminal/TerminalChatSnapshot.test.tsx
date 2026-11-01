import { cleanup, render, waitFor } from "@solidjs/testing-library";
import { createEffect } from "solid-js";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

const { invokeMock, snapshotMock } = vi.hoisted(() => ({ invokeMock: vi.fn(), snapshotMock: vi.fn() }));
vi.mock("../../../invoke", () => ({
	invoke: (...args: unknown[]) => invokeMock(...args),
	listen: async () => () => {},
}));

import { TerminalChatTranscript } from "../../../components/Terminal/TerminalChatTranscript";
import { acpTranscript } from "../../../stores/acpTranscript";
import { chatViewKey, chatViewStore } from "../../../stores/chatView";
import type { AcpSessionUpdate } from "../../../types/acp";

beforeEach(() => {
	chatViewStore.reset();
	acpTranscript.reset();
	invokeMock.mockReset();
	snapshotMock.mockReset();
	invokeMock.mockImplementation((command: string, ...args: unknown[]) =>
		command === "list_queued_agent_commands" ? Promise.resolve([]) : snapshotMock(command, ...args),
	);
});
afterEach(cleanup);

// Catches: a cleared projection is reported as an empty conversation while remount replay is pending.
it.each([false, true])("reopening Chat twice never reports empty during replay (mobile=%s)", async (mobile) => {
	const snapshot = {
		epoch: 0,
		nextSeq: 2,
		reset: true,
		unknownRows: 0,
		malformedRows: 0,
		updates: [
			{
				sessionUpdate: "user_message_chunk",
				messageId: "question",
				content: { type: "text", text: "Original question" },
			},
			{ sessionUpdate: "agent_message_chunk", messageId: "reply", content: { type: "text", text: "Original answer" } },
		],
	};
	for (let mount = 0; mount < 3; mount++) {
		let resolve!: (snapshot: unknown) => void;
		snapshotMock.mockReturnValueOnce(
			new Promise((done) => {
				resolve = done;
			}),
		);
		const view = render(() => (
			<TerminalChatTranscript sessionId="reopen" mobile={mobile} terminalAgent="claude" onUnavailable={() => {}} />
		));
		await waitFor(() => expect(snapshotMock).toHaveBeenCalledTimes(mount + 1));
		try {
			expect(view.queryByText("No messages in this conversation yet.")).toBeNull();
			expect(view.getByText("Loading conversation…")).toBeTruthy();
		} finally {
			resolve(snapshot);
			await waitFor(() => expect(view.getByLabelText("Chat transcript").textContent).toContain("Original answer"));
		}
		const text = view.getByLabelText("Chat transcript").textContent!;
		expect(text.indexOf("Original question")).toBeLessThan(text.indexOf("Original answer"));
		view.unmount();
	}
});

// Catches: the loading label hides a genuinely empty conversation after a successful snapshot.
it("reports an empty conversation only after its first snapshot arrives", async () => {
	let resolve!: (snapshot: unknown) => void;
	snapshotMock.mockReturnValueOnce(
		new Promise((done) => {
			resolve = done;
		}),
	);
	const view = render(() => <TerminalChatTranscript sessionId="empty" onUnavailable={() => {}} />);
	await waitFor(() => expect(snapshotMock).toHaveBeenCalledTimes(1));
	expect(view.queryByText("No messages in this conversation yet.")).toBeNull();
	resolve({ epoch: 0, nextSeq: 0, reset: true, updates: [], unknownRows: 0, malformedRows: 0 });
	await waitFor(() => expect(view.getByText("No messages in this conversation yet.")).toBeTruthy());
	expect(view.queryByText("Loading conversation…")).toBeNull();
});

// Catches: every replayed entry republishes partial history and rerenders all
// preceding rows, making desktop/mobile first open quadratic at 2000 entries.
it("snapshot replay publishes only the complete transcript", async () => {
	let resolve!: (snapshot: unknown) => void;
	snapshotMock.mockReturnValue(
		new Promise((done) => {
			resolve = done;
		}),
	);
	const observed: number[] = [];
	const view = render(() => {
		createEffect(() => {
			const count = acpTranscript.entries(chatViewKey("snapshot")).length;
			if (count) observed.push(count);
		});
		return <TerminalChatTranscript sessionId="snapshot" terminalAgent="claude" onUnavailable={() => {}} />;
	});
	await Promise.resolve();
	await Promise.resolve();
	const updates: AcpSessionUpdate[] = Array.from({ length: 2000 }, (_, i) => ({
		sessionUpdate: i % 2 === 0 ? "user_message_chunk" : "agent_message_chunk",
		messageId: `message-${i}`,
		content: { type: "text", text: `Entry ${i + 1}. Synthetic measurement paragraph.` },
	}));
	const start = performance.now();
	resolve({ epoch: 1, nextSeq: 2000, reset: true, updates, unknownRows: 0, malformedRows: 0 });
	await waitFor(() => expect(view.getByLabelText("Chat transcript").textContent).toContain("Entry 2000."));
	const elapsed = performance.now() - start;
	expect(observed.length, `2000-entry desktop snapshot replay/render: ${elapsed.toFixed(1)} ms`).toBe(1);
	expect(observed).toEqual([2000]);
}, 30000);
