import { beforeEach, describe, expect, it, vi } from "vitest";

const open = vi.fn();
vi.mock("../../stores/progress", () => ({ progressStore: { open: (...args: unknown[]) => open(...args) } }));
vi.mock("../../stores/terminals", () => ({
	terminalsStore: { getActive: () => ({ repoPath: "/repo", sessionId: "pty-1" }) },
}));
vi.mock("../../invoke", () => ({ invoke: vi.fn() }));

import { discussionStore, WARM_MS } from "../../stores/discussion";

const thread = (forkSessionId: string | null, finishedMs: number | null) =>
	({
		threadId: 1,
		project: "/repo",
		ptyId: "pty-1",
		source: "side",
		forkSessionId,
		running: finishedMs === null,
		turns: [{ finishedMs }],
	}) as never;

describe("discussionStore", () => {
	beforeEach(() => open.mockClear());

	// Catches: the shortcut opening Progress on the whole project instead of
	// the active terminal, where the ask box lives, or not moving focus.
	it("requestAsk opens Progress on the active terminal and asks for focus", () => {
		const before = discussionStore.askFocus();
		discussionStore.requestAsk();
		expect(open).toHaveBeenCalledWith("/repo", "pty-1");
		expect(discussionStore.askFocus()).toBe(before + 1);
	});

	// Catches: a thread shown warm without a fork, while still answering, or
	// past the cache lifetime.
	it("is warm only with a fork and a recent finished turn", () => {
		expect(discussionStore.isWarm(thread("fork", 1), 1 + WARM_MS - 1)).toBe(true);
		expect(discussionStore.isWarm(thread("fork", 1), 1 + WARM_MS)).toBe(false);
		expect(discussionStore.isWarm(thread(null, 1), 2)).toBe(false);
		expect(discussionStore.isWarm(thread("fork", null), 2)).toBe(false);
	});
});
