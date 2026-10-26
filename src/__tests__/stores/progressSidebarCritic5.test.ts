import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();
vi.mock("../../invoke", () => ({ invoke: invokeMock }));
vi.mock("../../stores/notifications", () => ({ notificationsStore: { isSessionSilenced: () => false } }));
vi.mock("../../stores/toasts", () => ({ toastsStore: { add: vi.fn() } }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { error: vi.fn() } }));
vi.mock("../../stores/repositories", () => ({
	repositoriesStore: { state: { activeRepoPath: "/repo" }, getPaths: () => ["/repo"], get: () => undefined },
}));
vi.mock("../../stores/terminals", () => ({ terminalsStore: { state: { activeId: null, terminals: {} } } }));

const flow = (title: string) => ({
	project: "/repo",
	participants: [{ id: "p1", kind: "subagent", title, state: "running", toolCalls: 1, ptyId: "pty" }],
	events: [],
	truncated: false,
});

describe("progressStore sidebar flow (critic r5)", () => {
	beforeEach(() => {
		vi.useFakeTimers();
		vi.clearAllMocks();
		vi.resetModules();
	});
	afterEach(() => vi.useRealTimers());

	// Catches: an ask inside the 5s gap being dropped with no retry, so a subagent that returned right
	// after a refresh keeps reading "Running" until the next minute tick.
	it("honours an ask made inside the gap once the gap has passed", async () => {
		const { progressStore } = await import("../../stores/progress");
		invokeMock.mockResolvedValueOnce(flow("v1")).mockResolvedValueOnce(flow("v2"));
		await progressStore.refreshSidebarFlow("/repo");
		vi.advanceTimersByTime(1000);
		await progressStore.refreshSidebarFlow("/repo");
		await vi.advanceTimersByTimeAsync(10_000);
		expect(progressStore.sidebarFlow("/repo")?.participants[0].title).toBe("v2");
	});

	// Catches: a fetch still in flight when the store is reset landing afterwards and resurrecting a flow.
	it("drops a response that arrives after the store was reset", async () => {
		const { progressStore } = await import("../../stores/progress");
		let resolve!: (v: unknown) => void;
		invokeMock.mockReturnValueOnce(new Promise((r) => (resolve = r)));
		const pending = progressStore.refreshSidebarFlow("/repo");
		progressStore.resetForTests();
		resolve(flow("late"));
		await pending;
		expect(progressStore.sidebarFlow("/repo")).toBeUndefined();
	});

	// Catches: a slow older response overwriting a newer one (out-of-order completion), showing stale subagents.
	it("keeps the newer flow when an older fetch completes last", async () => {
		const { progressStore } = await import("../../stores/progress");
		let resolveOld!: (v: unknown) => void;
		invokeMock.mockReturnValueOnce(new Promise((r) => (resolveOld = r))).mockResolvedValueOnce(flow("new"));
		const old = progressStore.refreshSidebarFlow("/repo");
		vi.advanceTimersByTime(6000);
		await progressStore.refreshSidebarFlow("/repo");
		resolveOld(flow("old"));
		await old;
		expect(progressStore.sidebarFlow("/repo")?.participants[0].title).toBe("new");
	});
});
