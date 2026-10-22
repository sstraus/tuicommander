import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/plugin-notification", () => ({
	isPermissionGranted: vi.fn().mockResolvedValue(true),
	requestPermission: vi.fn().mockResolvedValue("granted"),
	sendNotification: vi.fn(),
}));
vi.mock("../../invoke", () => ({ invoke: invokeMock }));
vi.mock("../../stores/toasts", () => ({ toastsStore: { add: vi.fn() } }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { error: vi.fn() } }));
vi.mock("../../stores/repositories", () => ({
	repositoriesStore: { state: { activeRepoPath: "/repo" }, getPaths: () => ["/repo"], get: () => undefined },
}));
vi.mock("../../stores/terminals", () => ({ terminalsStore: { state: { activeId: null, terminals: {} } } }));

const flow = (tag: string) => ({ project: "/repo", tag, columns: [], steps: [] });
const flowCalls = () => invokeMock.mock.calls.filter(([cmd]) => cmd === "progress_flow").length;

async function makeStore() {
	const { createProgressStore } = await import("../../stores/progress");
	return createProgressStore();
}

describe("sidebar flow lifecycle (critic r7)", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		vi.resetModules();
		vi.useFakeTimers();
	});

	it("a release function called twice does not cancel the trailing read of the other mounted row", async () => {
		// catches: release that decrements on every call, so a double release steals another row's hold
		invokeMock.mockResolvedValue(flow("x"));
		const store = await makeStore();
		const releaseA = store.holdSidebarFlow("/repo");
		store.holdSidebarFlow("/repo");
		await store.refreshSidebarFlow("/repo");
		await store.refreshSidebarFlow("/repo"); // schedules trailing
		releaseA();
		releaseA();
		await vi.advanceTimersByTimeAsync(6000);
		expect(flowCalls()).toBe(2);
		vi.useRealTimers();
	});

	it("remounting a row after the last release schedules and runs a fresh trailing read", async () => {
		// catches: a cancelled trailing entry left in the map so the remounted row's ask is swallowed
		invokeMock.mockResolvedValue(flow("x"));
		const store = await makeStore();
		const release = store.holdSidebarFlow("/repo");
		await store.refreshSidebarFlow("/repo");
		await store.refreshSidebarFlow("/repo");
		release();
		store.holdSidebarFlow("/repo");
		await store.refreshSidebarFlow("/repo"); // still inside the gap
		await vi.advanceTimersByTimeAsync(6000);
		expect(flowCalls()).toBe(2);
		vi.useRealTimers();
	});

	it("a clock that went back reads at once instead of spinning a trailing timer", async () => {
		// catches: waiting out the skew, whose >2^31-1 ms delay fires after 1 ms and re-arms forever
		invokeMock.mockResolvedValue(flow("x"));
		const store = await makeStore();
		vi.setSystemTime(new Date("2026-10-19T12:00:00Z"));
		await store.refreshSidebarFlow("/repo");
		vi.setSystemTime(new Date("2026-09-19T12:00:00Z"));
		await store.refreshSidebarFlow("/repo");
		expect(flowCalls()).toBe(2);
		expect(vi.getTimerCount()).toBe(0);
		vi.useRealTimers();
	});

	it("a skew beyond setTimeout's 2^31-1 ms limit reads at once and arms no timer", async () => {
		// catches: a >2^31 ms wait that fires after 1 ms and re-arms forever
		invokeMock.mockResolvedValue(flow("x"));
		const store = await makeStore();
		vi.setSystemTime(new Date("2030-01-01T00:00:00Z"));
		await store.refreshSidebarFlow("/repo");
		vi.setSystemTime(new Date("2026-01-01T00:00:00Z"));
		await store.refreshSidebarFlow("/repo");
		expect(flowCalls()).toBe(2);
		expect(vi.getTimerCount()).toBe(0);
		await vi.advanceTimersByTimeAsync(60_000);
		expect(flowCalls()).toBe(2);
		vi.useRealTimers();
	});

	it("an ask exactly one gap after the last read reads at once; one ms earlier waits", async () => {
		// catches: an off-by-one at the gap boundary (wait > 0 vs >= 0)
		invokeMock.mockResolvedValue(flow("x"));
		const store = await makeStore();
		vi.setSystemTime(new Date("2026-10-19T12:00:00Z"));
		await store.refreshSidebarFlow("/repo");
		vi.setSystemTime(new Date("2026-10-19T12:00:04.999Z"));
		await store.refreshSidebarFlow("/repo");
		expect(flowCalls()).toBe(1);
		expect(vi.getTimerCount()).toBe(1);
		await vi.advanceTimersByTimeAsync(1);
		expect(flowCalls()).toBe(2);
		vi.useRealTimers();
	});

	it("an ask at the same instant as the last read collapses into one trailing read", async () => {
		// catches: elapsed 0 being mistaken for a skewed clock and read immediately
		invokeMock.mockResolvedValue(flow("x"));
		const store = await makeStore();
		await store.refreshSidebarFlow("/repo");
		await store.refreshSidebarFlow("/repo");
		await store.refreshSidebarFlow("/repo");
		expect(flowCalls()).toBe(1);
		expect(vi.getTimerCount()).toBe(1);
		vi.useRealTimers();
	});

	it("a late older response is dropped once a newer one was applied", async () => {
		// catches: last-arrival-wins, so a slow stale read overwrites the fresh flow
		let resolveOld: (value: unknown) => void = () => {};
		invokeMock.mockImplementationOnce(() => new Promise((resolve) => (resolveOld = resolve)));
		invokeMock.mockResolvedValueOnce(flow("new"));
		const store = await makeStore();
		const old = store.refreshSidebarFlow("/repo");
		await vi.advanceTimersByTimeAsync(5001);
		await store.refreshSidebarFlow("/repo");
		resolveOld(flow("old"));
		await old;
		expect((store.sidebarFlow("/repo") as unknown as { tag: string }).tag).toBe("new");
		vi.useRealTimers();
	});

	it("an older response is applied when the newer read failed", async () => {
		// catches: the guard comparing with the latest requested seq instead of the latest applied one
		let resolveOld: (value: unknown) => void = () => {};
		invokeMock.mockImplementationOnce(() => new Promise((resolve) => (resolveOld = resolve)));
		invokeMock.mockRejectedValueOnce(new Error("boom"));
		const store = await makeStore();
		const old = store.refreshSidebarFlow("/repo");
		await vi.advanceTimersByTimeAsync(5001);
		await store.refreshSidebarFlow("/repo");
		resolveOld(flow("old"));
		await old;
		expect((store.sidebarFlow("/repo") as unknown as { tag: string }).tag).toBe("old");
		vi.useRealTimers();
	});

	it("a response in flight across resetForTests is dropped", async () => {
		// catches: epoch not bumped on reset, so a pre-reset read repopulates the store
		let resolveOld: (value: unknown) => void = () => {};
		invokeMock.mockImplementationOnce(() => new Promise((resolve) => (resolveOld = resolve)));
		const store = await makeStore();
		const pending = store.refreshSidebarFlow("/repo");
		store.resetForTests();
		resolveOld(flow("old"));
		await pending;
		expect(store.sidebarFlow("/repo")).toBeUndefined();
		vi.useRealTimers();
	});
});
