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

const flowOf = (project: string, tag: string) => ({ project, participants: [], events: [], truncated: false, tag });
const deferred = <T>() => {
	let resolve!: (v: T) => void;
	let reject!: (e: unknown) => void;
	const promise = new Promise<T>((res, rej) => {
		resolve = res;
		reject = rej;
	});
	return { promise, resolve, reject };
};
const flowCalls = () => invokeMock.mock.calls.filter((c) => c[0] === "progress_flow");

describe("sidebar flow (critic r6)", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		vi.resetModules();
		vi.useFakeTimers();
	});
	afterEach(() => vi.useRealTimers());

	const fresh = async () => (await import("../../stores/progress")).createProgressStore();

	// Catches: an ask inside the gap being dropped, so a busy flip right after a read never refreshes.
	it("runs exactly one trailing read when the gap ends, however many asks came inside it", async () => {
		invokeMock.mockResolvedValue(flowOf("/repo", "x"));
		const store = await fresh();
		await store.refreshSidebarFlow("/repo");
		vi.advanceTimersByTime(1000);
		await store.refreshSidebarFlow("/repo");
		await store.refreshSidebarFlow("/repo");
		await store.refreshSidebarFlow("/repo");
		expect(flowCalls()).toHaveLength(1);
		await vi.advanceTimersByTimeAsync(4000);
		expect(flowCalls()).toHaveLength(2);
		await vi.advanceTimersByTimeAsync(20000);
		expect(flowCalls()).toHaveLength(2);
	});

	// Catches: an off-by-one at the gap boundary (< vs <=) delaying a read that is already due.
	it("reads immediately when exactly the gap has elapsed", async () => {
		invokeMock.mockResolvedValue(flowOf("/repo", "x"));
		const store = await fresh();
		await store.refreshSidebarFlow("/repo");
		vi.advanceTimersByTime(5000);
		await store.refreshSidebarFlow("/repo");
		expect(flowCalls()).toHaveLength(2);
	});

	// Catches: one project's pending trailing read swallowing another project's ask.
	it("keeps a trailing read per project", async () => {
		invokeMock.mockImplementation((_c: string, a: { project: string }) => Promise.resolve(flowOf(a.project, "x")));
		const store = await fresh();
		await store.refreshSidebarFlow("/a");
		await store.refreshSidebarFlow("/b");
		await store.refreshSidebarFlow("/a");
		await store.refreshSidebarFlow("/b");
		await vi.advanceTimersByTimeAsync(5000);
		const byProject = flowCalls().map((c) => c[1].project);
		expect(byProject.filter((p) => p === "/a")).toHaveLength(2);
		expect(byProject.filter((p) => p === "/b")).toHaveLength(2);
	});

	// Catches: a pending trailing timer surviving a reset and fetching for a project that was wiped.
	it("cancels the trailing read on reset", async () => {
		invokeMock.mockResolvedValue(flowOf("/repo", "x"));
		const store = await fresh();
		await store.refreshSidebarFlow("/repo");
		await store.refreshSidebarFlow("/repo");
		store.resetForTests();
		await vi.advanceTimersByTimeAsync(20000);
		expect(flowCalls()).toHaveLength(1);
	});

	// Catches: a slow older response landing after a newer one and putting stale subagents back.
	it("ignores an older response that arrives after a newer one", async () => {
		const slow = deferred<unknown>();
		invokeMock.mockReturnValueOnce(slow.promise).mockResolvedValueOnce(flowOf("/repo", "new"));
		const store = await fresh();
		const first = store.refreshSidebarFlow("/repo");
		vi.advanceTimersByTime(5000);
		await store.refreshSidebarFlow("/repo");
		slow.resolve(flowOf("/repo", "old"));
		await first;
		expect((store.sidebarFlow("/repo") as { tag?: string }).tag).toBe("new");
	});

	// Catches: the newer request failing and the older, valid response being dropped with it,
	// leaving the sidebar on a flow older than one the backend already delivered.
	it("applies an older response when the newer request failed", async () => {
		const slow = deferred<unknown>();
		invokeMock.mockReturnValueOnce(slow.promise).mockRejectedValueOnce(new Error("boom"));
		const store = await fresh();
		const first = store.refreshSidebarFlow("/repo");
		vi.advanceTimersByTime(5000);
		await store.refreshSidebarFlow("/repo");
		slow.resolve(flowOf("/repo", "old"));
		await first;
		expect((store.sidebarFlow("/repo") as { tag?: string } | undefined)?.tag).toBe("old");
	});

	// Catches: a response sent before a reset repopulating the wiped store.
	it("drops a response that was in flight when the store was reset", async () => {
		const slow = deferred<unknown>();
		invokeMock.mockReturnValueOnce(slow.promise);
		const store = await fresh();
		const first = store.refreshSidebarFlow("/repo");
		store.resetForTests();
		slow.resolve(flowOf("/repo", "old"));
		await first;
		expect(store.sidebarFlow("/repo")).toBeUndefined();
	});

	// Catches: the epoch guard also blocking requests made after the reset.
	it("accepts a response for a request sent after a reset", async () => {
		invokeMock.mockResolvedValue(flowOf("/repo", "after"));
		const store = await fresh();
		await store.refreshSidebarFlow("/repo");
		store.resetForTests();
		await store.refreshSidebarFlow("/repo");
		expect((store.sidebarFlow("/repo") as { tag?: string }).tag).toBe("after");
	});
});
