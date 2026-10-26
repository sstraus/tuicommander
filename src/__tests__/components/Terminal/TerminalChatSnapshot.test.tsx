import { cleanup, render, waitFor } from "@solidjs/testing-library";
import { createEffect } from "solid-js";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
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
});
afterEach(cleanup);

// Catches: every replayed entry republishes partial history and rerenders all
// preceding rows, making desktop/mobile first open quadratic at 2000 entries.
it("snapshot replay publishes only the complete transcript", async () => {
	let resolve!: (snapshot: unknown) => void;
	invokeMock.mockReturnValue(
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
