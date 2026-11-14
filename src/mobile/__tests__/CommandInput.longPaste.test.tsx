import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";
import { CommandInput } from "../components/CommandInput";

const writes: string[] = [];
vi.mock("../../invoke", () => ({
	invoke: vi.fn(async () => ({ epoch: 1, nextSeq: 0, reset: true, updates: [], unknownRows: 0, malformedRows: 0 })),
	listen: vi.fn(async () => () => {}),
}));
vi.mock("../../transport", () => ({
	buildHttpUrl: (path: string) => `http://localhost${path}`,
	rpc: vi.fn((_command: string, args: { data?: string }) => {
		if (args.data !== undefined) writes.push(args.data);
		return Promise.resolve(undefined);
	}),
	HttpRpcError: class extends Error {},
}));

afterEach(() => {
	cleanup();
	writes.length = 0;
});

// Catches: a long mobile paste is live-synced to Claude as one plain write; Claude drops all but the
// tail of the first read (#2915), so the agent receives truncated text.
it.each([5_000, 25_000, 80_000])("frames a %i-character pasted draft as one bracketed paste for Claude", (length) => {
	const text = "x".repeat(length);
	const { container } = render(() => <CommandInput sessionId="long-paste" agentType="claude" ptyInputLine="" />);
	fireEvent.input(container.querySelector("textarea")!, { target: { value: text } });
	expect(writes).toEqual([`\x1b[200~${text}\x1b[201~`]);
});

// Catches: framing every live keystroke breaks typing; short text stays plain.
it("keeps a short typed draft unframed", () => {
	const { container } = render(() => <CommandInput sessionId="short" agentType="claude" ptyInputLine="" />);
	fireEvent.input(container.querySelector("textarea")!, { target: { value: "hello" } });
	expect(writes).toEqual(["hello"]);
});
