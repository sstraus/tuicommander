import { afterEach, beforeEach, expect, it, vi } from "vitest";
import "../mocks/tauri";
import { makeTerminal, testInScopeAsync } from "../helpers/store";
import { mockInvoke } from "../mocks/tauri";

beforeEach(() => {
	vi.resetModules();
	vi.useFakeTimers();
	mockInvoke.mockReset();
});

afterEach(() => {
	vi.restoreAllMocks();
	vi.useRealTimers();
});

// The dynamic imports below load the whole hook graph after vi.resetModules(); measured 2-4 s
// cold, which is setup and not the behaviour under test, so it gets its own bound (the
// assertions run in ~35 ms).
// Catches: resync reuses a pre-gap snapshot and leaves a drained queue visible indefinitely.
it("refreshes a drained queue when reconnect races an in-flight pre-gap snapshot", async () => {
	const transport = await import("../../transport");
	let resync: ((reason: import("../../transport").ResyncReason) => void) | undefined;
	vi.spyOn(transport, "subscribeEvents").mockImplementation(async (_handlers, options) => {
		resync = options?.onResync;
		return () => {};
	});
	let releaseSnapshot: (() => void) | undefined;
	const staleSnapshot = new Promise<unknown>((resolve) => {
		releaseSnapshot = () =>
			resolve([{ session_id: "sess-1", state: { shell_state: "idle", agent_state: "idle", queued_commands: 1 } }]);
	});
	let snapshots = 0;
	mockInvoke.mockImplementation(async (command) => {
		if (command !== "list_active_sessions") return null;
		snapshots += 1;
		if (snapshots === 1) return staleSnapshot;
		return [{ session_id: "sess-1", state: { shell_state: "idle", agent_state: "idle" } }];
	});
	const store = (await import("../../stores/terminals")).terminalsStore;
	const { useAgentPolling } = await import("../../hooks/useAgentPolling");
	await testInScopeAsync(async () => {
		const id = store.add(makeTerminal({ name: "Codex", sessionId: "sess-1" }));
		store.update(id, { queuedCommands: 1 });
		useAgentPolling();
		await vi.advanceTimersByTimeAsync(0);
		expect(resync).toBeDefined();
		// The backend drained during the gap; the old response is still in transit.
		resync?.("reconnect");
		releaseSnapshot?.();
		await vi.advanceTimersByTimeAsync(0);
		expect(store.get(id)?.queuedCommands).toBe(0);
	});
}, 30_000);
