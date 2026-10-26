import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";
import { makeTerminal, testInScopeAsync } from "../helpers/store";
import { mockInvoke } from "../mocks/tauri";

/** Helper: advance timers and flush all pending microtasks */
async function tick(ms: number) {
	await vi.advanceTimersByTimeAsync(ms);
	await Promise.resolve();
	await Promise.resolve();
}

describe("useAgentPolling", () => {
	let store: typeof import("../../stores/terminals").terminalsStore;

	beforeEach(async () => {
		vi.resetModules();
		vi.useFakeTimers();
		mockInvoke.mockReset();
		store = (await import("../../stores/terminals")).terminalsStore;
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	// Catches: the authoritative idle snapshot erases a final API error's red dot.
	it.each(["claude", "codex", "ego"])("retains %s final failure until the backend observes recovery", async (agent) => {
		const id = store.add(makeTerminal({ sessionId: "failed-2335" }));
		const { applySessionStateEvent } = await import("../../hooks/useAgentPolling");
		const snapshot = (shell: string, error?: string) =>
			applySessionStateEvent({
				session_id: "failed-2335",
				state: {
					agent_type: agent,
					shell_state: shell,
					agent_state: shell === "busy" ? "working" : "completed",
					last_error: error,
				},
			});
		snapshot("busy");
		snapshot("idle", "provider response failed");
		expect(store.get(id)?.awaitingInput).toBe("error");
		// A repaint or an injected continue is not proof of successful recovery.
		snapshot("busy", "provider response failed");
		expect(store.get(id)?.awaitingInput).toBe("error");
		snapshot("idle", "provider response failed");
		expect(store.get(id)?.awaitingInput).toBe("error");
		snapshot("busy");
		snapshot("idle");
		expect(store.get(id)?.awaitingInput).toBeNull();
		store.remove(id);
	});

	// Catches: remote agent_type is dropped until the 30s process-discovery poll.
	it("applies backend identity immediately and retires it when the shell returns", async () => {
		const id = store.add(makeTerminal({ name: "shell 1", sessionId: "remote-1" }));
		mockInvoke.mockResolvedValueOnce([
			{
				session_id: "remote-1",
				connection_id: "mint",
				state: {
					agent_type: "codex",
					agent_state: "idle",
					shell_state: "idle",
				},
			},
		]);
		const { syncAgentLifecycleStates, applySessionStateEvent } = await import("../../hooks/useAgentPolling");
		const { pluginRegistry } = await import("../../plugins/pluginRegistry");
		const transitions: Array<{ type: string; identity: string | null }> = [];
		const notify = vi.spyOn(pluginRegistry, "notifyStateChange").mockImplementation((event) => {
			transitions.push({ type: event.type, identity: store.get(id)?.agentType ?? null });
		});
		try {
			await syncAgentLifecycleStates();
			expect(store.get(id)?.agentType).toBe("codex");
			expect(store.get(id)?.agentState).toBe("idle");
			expect(store.isBusy(id)).toBe(false);
			store.update(id, { agentSessionId: "old-conversation" });
			applySessionStateEvent({
				session_id: "remote-1",
				state: { agent_type: "claude", agent_state: "idle", shell_state: "idle" },
			});
			expect(store.get(id)?.agentType).toBe("claude");
			expect(store.get(id)?.agentSessionId).toBeNull();
			applySessionStateEvent({ session_id: "remote-1", state: { shell_state: "idle" } });
			expect(store.get(id)?.agentType).toBeNull();
			expect(transitions).toEqual([
				{ type: "agent-started", identity: "codex" },
				{ type: "agent-stopped", identity: "codex" },
				{ type: "agent-started", identity: "claude" },
				{ type: "agent-stopped", identity: "claude" },
			]);
		} finally {
			notify.mockRestore();
		}
	});

	// Catches: hook exactness attached to the wrong tab or surviving an omitted/retracted lifecycle field.
	it("keeps native Codex binding on its tab and retracts it with the snapshot", async () => {
		const a = store.add(makeTerminal({ sessionId: "codex-a", agentType: "codex" }));
		const b = store.add(makeTerminal({ sessionId: "codex-b", agentType: "codex" }));
		const { applySessionStateEvent } = await import("../../hooks/useAgentPolling");
		applySessionStateEvent({
			session_id: "codex-b",
			state: { agent_type: "codex", codex_binding: { session_id: "thread-b" } },
		});
		applySessionStateEvent({
			session_id: "codex-a",
			state: { agent_type: "codex", codex_binding: { session_id: "thread-a" } },
		});
		expect(store.get(a)?.exactAgentSessionId).toBe("thread-a");
		expect(store.get(b)?.exactAgentSessionId).toBe("thread-b");
		expect(store.get(a)?.agentSessionId).toBe("thread-a");
		applySessionStateEvent({ session_id: "codex-a", state: { agent_type: "codex" } });
		expect(store.get(a)?.exactAgentSessionId).toBeNull();
		expect(store.get(b)?.exactAgentSessionId).toBe("thread-b");
		applySessionStateEvent({ session_id: "codex-b", state: { agent_type: null } });
		expect(store.get(b)?.exactAgentSessionId).toBeNull();
	});

	// Catches: an older catch-up undoes an identity push that arrived while the snapshot was in flight.
	it("does not let a stale snapshot replace a newer pushed identity", async () => {
		const id = store.add(makeTerminal({ sessionId: "identity-race", agentType: "claude" }));
		let resolve!: (rows: unknown[]) => void;
		mockInvoke.mockImplementationOnce(
			() =>
				new Promise((done) => {
					resolve = done;
				}),
		);
		const { syncAgentLifecycleStates, applySessionStateEvent } = await import("../../hooks/useAgentPolling");
		const pending = syncAgentLifecycleStates();
		applySessionStateEvent({ session_id: "identity-race", state: { agent_type: "codex" } });
		resolve([{ session_id: "identity-race", state: { agent_type: "claude" } }]);
		await pending;
		expect(store.get(id)?.agentType).toBe("codex");
	});

	it("applies authoritative lifecycle transitions without retaining stale working state", async () => {
		const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));
		mockInvoke.mockResolvedValueOnce([
			{ session_id: "sess-1", state: { shell_state: "idle", agent_state: "working", background_work: true } },
		]);
		const { syncAgentLifecycleStates } = await import("../../hooks/useAgentPolling");

		await syncAgentLifecycleStates();
		expect(store.get(id)?.agentState).toBe("working");
		expect(store.get(id)?.backgroundWork).toBe(true);
		expect(store.get(id)?.shellState).toBe("idle");

		mockInvoke.mockResolvedValueOnce([
			{ session_id: "sess-1", state: { shell_state: "idle", agent_state: "idle", background_work: false } },
		]);
		await syncAgentLifecycleStates();
		expect(store.get(id)?.agentState).toBe("idle");
		expect(store.get(id)?.backgroundWork).toBe(false);
	});

	it("hydrates prompt and intent from the authoritative session snapshot", async () => {
		const id = store.add(makeTerminal({ name: "Custom Grok", sessionId: "sess-1" }));
		store.update(id, { nameIsCustom: true });
		mockInvoke.mockResolvedValueOnce([
			{
				session_id: "sess-1",
				state: {
					shell_state: "idle",
					agent_intent: "locking out hashtags",
					last_prompt: "write a campaign with enough words to remain the relevant prompt",
				},
			},
		]);
		const { syncAgentLifecycleStates } = await import("../../hooks/useAgentPolling");

		await syncAgentLifecycleStates();

		expect(store.get(id)?.agentIntent).toBe("locking out hashtags");
		expect(store.get(id)?.lastPrompt).toBe("write a campaign with enough words to remain the relevant prompt");
		expect(store.get(id)?.name).toBe("Custom Grok");
		expect(store.get(id)?.nameIsCustom).toBe(true);
	});

	it("keeps backend semantic activity time when PTY output is more recent", async () => {
		const id = store.add(makeTerminal({ name: "Codex", sessionId: "sess-1" }));
		const now = Date.now();
		store.touchLastDataAt(id, now - 30_000);
		mockInvoke.mockResolvedValueOnce([
			{
				session_id: "sess-1",
				state: { shell_state: "idle", agent_state: "idle", last_activity_ms: now - 70 * 60_000 },
			},
		]);
		const { syncAgentLifecycleStates } = await import("../../hooks/useAgentPolling");

		await syncAgentLifecycleStates();
		expect(store.get(id)?.lastActivityAt).toBe(now - 70 * 60_000);
		expect(store.getLastDataAt(id)).toBe(now - 30_000);
	});

	it("tracks the queued-command depth, treating an omitted field as an empty queue", async () => {
		const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));
		mockInvoke.mockResolvedValueOnce([
			{ session_id: "sess-1", state: { shell_state: "busy", agent_state: "working", queued_commands: 3 } },
		]);
		const { syncAgentLifecycleStates } = await import("../../hooks/useAgentPolling");

		await syncAgentLifecycleStates();
		expect(store.get(id)?.queuedCommands).toBe(3);

		// serde skips the field at zero, so its absence means the queue drained —
		// keeping the previous count would leave a badge offering to clear nothing.
		mockInvoke.mockResolvedValueOnce([{ session_id: "sess-1", state: { shell_state: "idle", agent_state: "idle" } }]);
		await syncAgentLifecycleStates();
		expect(store.get(id)?.queuedCommands).toBe(0);
	});

	// Catches: a missed queue-drained push leaves a stale queue badge after reconnect or SSE lag.
	it.each(["reconnect", "lagged"] as const)("clears stale queued commands after %s catch-up", async (reason) => {
		const transport = await import("../../transport");
		let resync: ((reason: import("../../transport").ResyncReason) => void) | undefined;
		const subscription = vi.spyOn(transport, "subscribeEvents").mockImplementation(async (_handlers, options) => {
			resync = options?.onResync;
			return () => {};
		});
		try {
			mockInvoke.mockResolvedValue([
				{ session_id: "sess-1", state: { shell_state: "busy", agent_state: "working", queued_commands: 1 } },
			]);
			const { useAgentPolling } = await import("../../hooks/useAgentPolling");
			await testInScopeAsync(async () => {
				const id = store.add(makeTerminal({ name: "Codex", sessionId: "sess-1" }));
				useAgentPolling();
				await tick(0);
				expect(store.get(id)?.queuedCommands).toBe(1);
				// No queue-drained push arrives; only the transport recovery hint does.
				mockInvoke.mockClear();
				mockInvoke.mockResolvedValue([
					{ session_id: "sess-1", state: { shell_state: "busy", agent_state: "working" } },
				]);
				expect(resync).toBeDefined();
				resync?.(reason);
				await tick(0);
				expect(mockInvoke).toHaveBeenCalledWith("list_active_sessions");
				expect(store.get(id)?.queuedCommands).toBe(0);
				expect(store.get(id)?.agentState).toBe("working");
			});
		} finally {
			subscription.mockRestore();
		}
	});

	it("reconciles awaiting state from the authoritative backend snapshot", async () => {
		const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));
		mockInvoke.mockResolvedValueOnce([
			{
				session_id: "sess-1",
				state: { agent_state: "awaiting_input", awaiting_input: true, question_confident: true },
			},
		]);
		const { syncAgentLifecycleStates } = await import("../../hooks/useAgentPolling");

		await syncAgentLifecycleStates();
		expect(store.get(id)?.awaitingInput).toBe("question");
		expect(store.get(id)?.awaitingInputConfident).toBe(true);

		mockInvoke.mockResolvedValueOnce([
			{ session_id: "sess-1", state: { agent_state: "working", awaiting_input: false } },
		]);
		await syncAgentLifecycleStates();
		expect(store.get(id)?.awaitingInput).toBeNull();
		expect(store.get(id)?.awaitingInputConfident).toBe(false);
	});

	it("clears lifecycle state for a terminal omitted from a successful snapshot", async () => {
		const id = store.add(makeTerminal({ name: "T1", sessionId: "lost-session" }));
		store.update(id, { agentState: "working", backgroundWork: true });
		mockInvoke.mockResolvedValueOnce([]);
		const { syncAgentLifecycleStates } = await import("../../hooks/useAgentPolling");

		await syncAgentLifecycleStates();
		expect(store.get(id)?.shellState).toBe("exited");
		expect(store.get(id)?.sessionId).toBeNull();
		expect(store.get(id)?.agentState).toBeNull();
		expect(store.get(id)?.backgroundWork).toBe(false);
	});

	it("does not close an omitted terminal after a newer PTY event", async () => {
		const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));
		let resolveSnapshot!: (value: unknown) => void;
		mockInvoke.mockImplementationOnce(
			() =>
				new Promise((resolve) => {
					resolveSnapshot = resolve;
				}),
		);
		const { syncAgentLifecycleStates } = await import("../../hooks/useAgentPolling");

		const pending = syncAgentLifecycleStates();
		store.update(id, { shellState: "busy" });
		resolveSnapshot([]);
		await pending;

		expect(store.get(id)?.sessionId).toBe("sess-1");
		expect(store.get(id)?.shellState).toBe("busy");
	});

	it("does not close an omitted terminal after its session is replaced", async () => {
		const id = store.add(makeTerminal({ name: "T1", sessionId: "old-session" }));
		let resolveSnapshot!: (value: unknown) => void;
		mockInvoke.mockImplementationOnce(
			() =>
				new Promise((resolve) => {
					resolveSnapshot = resolve;
				}),
		);
		const { syncAgentLifecycleStates } = await import("../../hooks/useAgentPolling");

		const pending = syncAgentLifecycleStates();
		store.update(id, { sessionId: "replacement-session" });
		resolveSnapshot([]);
		await pending;

		expect(store.get(id)?.sessionId).toBe("replacement-session");
	});

	it("does not let a snapshot overwrite a PTY state event that arrived after the request", async () => {
		const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));
		store.update(id, { shellState: "idle", agentState: "working", backgroundWork: true });
		let resolveSnapshot!: (value: unknown) => void;
		mockInvoke.mockImplementationOnce(
			() =>
				new Promise((resolve) => {
					resolveSnapshot = resolve;
				}),
		);
		const { syncAgentLifecycleStates } = await import("../../hooks/useAgentPolling");

		const pending = syncAgentLifecycleStates();
		store.update(id, { shellState: "busy" });
		resolveSnapshot([
			{ session_id: "sess-1", state: { shell_state: "idle", agent_state: "idle", background_work: false } },
		]);
		await pending;

		expect(store.get(id)?.shellState).toBe("busy");
		expect(store.get(id)?.agentState).toBe("working");
		expect(store.get(id)?.backgroundWork).toBe(true);
	});

	it("recovers polling after a native session-list timeout", async () => {
		const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));
		let resolveHung!: (value: unknown) => void;
		mockInvoke.mockImplementationOnce(
			() =>
				new Promise((resolve) => {
					resolveHung = resolve;
				}),
		);
		const { syncAgentLifecycleStates } = await import("../../hooks/useAgentPolling");

		const timedOut = syncAgentLifecycleStates();
		await vi.advanceTimersByTimeAsync(5_001);
		await timedOut;

		mockInvoke.mockResolvedValueOnce([
			{ session_id: "sess-1", state: { shell_state: "idle", agent_state: "completed", background_work: false } },
		]);
		await syncAgentLifecycleStates();
		resolveHung([]);

		expect(store.get(id)?.agentState).toBe("completed");
	});

	it("serializes lifecycle polls so an older response cannot overwrite a newer snapshot", async () => {
		const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));
		let resolveOlder!: (value: unknown) => void;
		const older = new Promise((resolve) => {
			resolveOlder = resolve;
		});
		mockInvoke
			.mockImplementationOnce(() => older)
			.mockResolvedValueOnce([
				{ session_id: "sess-1", state: { shell_state: "idle", agent_state: "completed", background_work: false } },
			]);
		const { syncAgentLifecycleStates } = await import("../../hooks/useAgentPolling");

		const oldRequest = syncAgentLifecycleStates();
		const coalescedRequest = syncAgentLifecycleStates();
		resolveOlder([
			{ session_id: "sess-1", state: { shell_state: "busy", agent_state: "working", background_work: true } },
		]);
		await oldRequest;
		await coalescedRequest;
		await syncAgentLifecycleStates();

		expect(store.get(id)?.agentState).toBe("completed");
		expect(store.get(id)?.backgroundWork).toBe(false);
		expect(store.get(id)?.shellState).toBe("idle");
	});

	it("polls the active terminal's foreground process", async () => {
		mockInvoke.mockResolvedValue("claude");

		await testInScopeAsync(async () => {
			const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));
			store.setActive(id);

			const { useAgentPolling } = await import("../../hooks/useAgentPolling");
			useAgentPolling();

			// Poll fires on first interval tick (30s fallback), not immediately
			await vi.advanceTimersByTimeAsync(30_000);
			await Promise.resolve(); // flush microtasks

			expect(mockInvoke).toHaveBeenCalledWith("get_session_foreground_process", {
				sessionId: "sess-1",
			});
			expect(store.get(id)?.agentType).toBe("claude");
		});
	});

	it("does not poll when no active terminal", async () => {
		await testInScopeAsync(async () => {
			const { useAgentPolling } = await import("../../hooks/useAgentPolling");
			useAgentPolling();

			await vi.advanceTimersByTimeAsync(30_000);

			expect(mockInvoke).not.toHaveBeenCalledWith("get_session_foreground_process", expect.anything());
		});
	});

	it("does not poll when active terminal has no session", async () => {
		await testInScopeAsync(async () => {
			const id = store.add(makeTerminal({ name: "T1" }));
			store.setActive(id);

			const { useAgentPolling } = await import("../../hooks/useAgentPolling");
			useAgentPolling();

			await vi.advanceTimersByTimeAsync(30_000);

			expect(mockInvoke).not.toHaveBeenCalledWith("get_session_foreground_process", expect.anything());
		});
	});

	it("sets agentType to null when result is null", async () => {
		mockInvoke.mockResolvedValue(null);

		await testInScopeAsync(async () => {
			const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));
			store.setActive(id);

			const { useAgentPolling } = await import("../../hooks/useAgentPolling");
			useAgentPolling();

			await vi.advanceTimersByTimeAsync(30_000);
			await Promise.resolve();

			expect(store.get(id)?.agentType).toBeNull();
		});
	});

	it("handles invoke errors gracefully", async () => {
		mockInvoke.mockRejectedValue(new Error("Session not found"));

		await testInScopeAsync(async () => {
			const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));
			store.setActive(id);

			const { useAgentPolling } = await import("../../hooks/useAgentPolling");
			useAgentPolling();

			// Should not throw
			await vi.advanceTimersByTimeAsync(30_000);
			await Promise.resolve();

			// agentType should remain null (default)
			expect(store.get(id)?.agentType).toBeNull();
		});
	});

	describe("session discovery", () => {
		/** A discovery result with no rebuilt launch command (no readable agent process). */
		const discovered = (sessionId: string) => ({ sessionId, launchCommand: null });

		it("calls discover_agent_session when agentType transitions null→agent and agentSessionId is null", async () => {
			let pollCount = 0;
			mockInvoke.mockImplementation((cmd: string) => {
				if (cmd === "get_session_foreground_process") {
					pollCount++;
					return Promise.resolve(pollCount >= 2 ? "claude" : null);
				}
				if (cmd === "get_session_leaf_pid") return Promise.resolve(1234);
				if (cmd === "discover_agent_session") return Promise.resolve(discovered("found-uuid"));
				return Promise.resolve(null);
			});

			await testInScopeAsync(async () => {
				const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));

				const { useAgentPolling } = await import("../../hooks/useAgentPolling");
				useAgentPolling();

				await tick(30_000); // first poll: null
				expect(store.get(id)?.agentType).toBeNull();

				await tick(30_000); // second poll: claude detected + discovery fires in same cycle
				expect(store.get(id)?.agentType).toBe("claude");
				expect(mockInvoke).toHaveBeenCalledWith(
					"discover_agent_session",
					expect.objectContaining({
						agentType: "claude",
					}),
				);
				expect(store.get(id)?.agentSessionId).toBe("found-uuid");
			});
		});

		it("re-discovers claude session on subsequent polls (tracks /clear)", async () => {
			let discoverCount = 0;
			mockInvoke.mockImplementation((cmd: string) => {
				if (cmd === "get_session_foreground_process") return Promise.resolve("claude");
				if (cmd === "get_session_leaf_pid") return Promise.resolve(1234);
				if (cmd === "discover_agent_session") {
					discoverCount++;
					return Promise.resolve(discovered(discoverCount <= 2 ? "uuid-1" : "uuid-2"));
				}
				return Promise.resolve(null);
			});

			await testInScopeAsync(async () => {
				const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));

				const { useAgentPolling } = await import("../../hooks/useAgentPolling");
				useAgentPolling();

				await tick(30_000); // poll 1 + discovery
				expect(store.get(id)?.agentSessionId).toBe("uuid-1");

				await tick(30_000); // poll 2 + re-discover (same uuid, no store update)
				expect(store.get(id)?.agentSessionId).toBe("uuid-1");

				await tick(30_000); // poll 3 + re-discover (new uuid after /clear)
				expect(store.get(id)?.agentSessionId).toBe("uuid-2");

				const discoveryCalls = mockInvoke.mock.calls.filter(([cmd]) => cmd === "discover_agent_session");
				expect(discoveryCalls).toHaveLength(3);
			});
		});

		it("re-discovers non-claude agents on subsequent polls too", async () => {
			let discoverCount = 0;
			mockInvoke.mockImplementation((cmd: string) => {
				if (cmd === "get_session_foreground_process") return Promise.resolve("gemini");
				if (cmd === "get_session_leaf_pid") return Promise.resolve(1234);
				if (cmd === "discover_agent_session") {
					discoverCount++;
					return Promise.resolve(discovered(discoverCount <= 2 ? "found-uuid" : "new-uuid"));
				}
				return Promise.resolve(null);
			});

			await testInScopeAsync(async () => {
				const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));

				const { useAgentPolling } = await import("../../hooks/useAgentPolling");
				useAgentPolling();

				await tick(30_000); // poll 1 + discovery
				expect(store.get(id)?.agentSessionId).toBe("found-uuid");

				await tick(30_000); // poll 2 + re-discover (same uuid)
				expect(store.get(id)?.agentSessionId).toBe("found-uuid");

				await tick(30_000); // poll 3 + re-discover (new uuid after /clear)
				expect(store.get(id)?.agentSessionId).toBe("new-uuid");

				const discoveryCalls = mockInvoke.mock.calls.filter(([cmd]) => cmd === "discover_agent_session");
				expect(discoveryCalls).toHaveLength(3);
			});
		});

		it("discovers claude session even when tuicSession is set", async () => {
			mockInvoke.mockImplementation((cmd: string) => {
				if (cmd === "get_session_foreground_process") return Promise.resolve("claude");
				if (cmd === "get_session_leaf_pid") return Promise.resolve(1234);
				if (cmd === "discover_agent_session") return Promise.resolve(discovered("discovered-uuid"));
				return Promise.resolve(null);
			});

			await testInScopeAsync(async () => {
				const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));
				store.update(id, { tuicSession: "tuic-uuid-123" });

				const { useAgentPolling } = await import("../../hooks/useAgentPolling");
				useAgentPolling();

				await tick(30_000);
				expect(store.get(id)?.agentType).toBe("claude");

				const discoveryCalls = mockInvoke.mock.calls.filter(([cmd]) => cmd === "discover_agent_session");
				expect(discoveryCalls).toHaveLength(1);
				expect(store.get(id)?.agentSessionId).toBe("discovered-uuid");
			});
		});

		it("discovers non-claude agents even when tuicSession is set", async () => {
			mockInvoke.mockImplementation((cmd: string) => {
				if (cmd === "get_session_foreground_process") return Promise.resolve("gemini");
				if (cmd === "get_session_leaf_pid") return Promise.resolve(1234);
				if (cmd === "discover_agent_session") return Promise.resolve(discovered("discovered-uuid"));
				return Promise.resolve(null);
			});

			await testInScopeAsync(async () => {
				const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));
				store.update(id, { tuicSession: "tuic-uuid-123" });

				const { useAgentPolling } = await import("../../hooks/useAgentPolling");
				useAgentPolling();

				await tick(30_000);
				expect(store.get(id)?.agentType).toBe("gemini");

				const discoveryCalls = mockInvoke.mock.calls.filter(([cmd]) => cmd === "discover_agent_session");
				expect(discoveryCalls).toHaveLength(1);
				expect(store.get(id)?.agentSessionId).toBe("discovered-uuid");
			});
		});

		it("skips discovery for agents without sessionDiscovery config (e.g. opencode)", async () => {
			mockInvoke.mockResolvedValue("opencode");

			await testInScopeAsync(async () => {
				const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));

				const { useAgentPolling } = await import("../../hooks/useAgentPolling");
				useAgentPolling();

				await tick(30_000);
				expect(store.get(id)?.agentType).toBe("opencode");

				const discoveryCalls = mockInvoke.mock.calls.filter(([cmd]) => cmd === "discover_agent_session");
				expect(discoveryCalls).toHaveLength(0);
			});
		});

		it("dispatches synthetic shell-state to plugins when agent first detected", async () => {
			// Bug: when agentType transitions null→"claude", structured shell-state events
			// dispatched BEFORE detection completes were filtered out (pluginMatchesSession
			// returned false because agentType was still null). The plugin never learned
			// the current shellState. Fix: after agent-started, replay the current shellState
			// so filtered plugins catch up.
			mockInvoke.mockResolvedValue("claude");

			await testInScopeAsync(async () => {
				const { detectAgentForTerminal } = await import("../../hooks/useAgentPolling");
				const { pluginRegistry } = await import("../../plugins/pluginRegistry");

				const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-synth" }));
				// Simulate: shell is idle, but agentType not yet detected
				store.update(id, { shellState: "idle" });

				// Register a plugin with agentTypes: ["claude"] that listens for shell-state
				const shellStateHandler = vi.fn();
				await pluginRegistry.register(
					{
						id: "test-keepalive",
						onload: (host) => {
							host.registerStructuredEventHandler("shell-state", shellStateHandler);
						},
						onunload: () => {},
					},
					["pty:write"],
					["claude"],
				);

				// Before detection: dispatch shell-state directly → should be filtered (agentType null)
				pluginRegistry.dispatchStructuredEvent("shell-state", { state: "idle" }, "sess-synth");
				await new Promise<void>((r) => queueMicrotask(r));
				expect(shellStateHandler).not.toHaveBeenCalled();

				// Now detect agent (null → claude) — should trigger synthetic replay
				await detectAgentForTerminal(id, "idle");
				await new Promise<void>((r) => queueMicrotask(r));

				// Plugin should have received the synthetic shell-state event
				expect(shellStateHandler).toHaveBeenCalledWith(expect.objectContaining({ state: "idle" }), "sess-synth");

				pluginRegistry.unregister("test-keepalive");
			});
		});

		it("fires agent-stopped for filtered plugins on direct agent→agent transitions", async () => {
			// Bug: when agentType switched from claude to codex without first passing
			// through null (user exits claude and immediately runs codex, before the
			// NULL_THRESHOLD idle-streak clears the agent), neither agent-started nor
			// agent-stopped was dispatched. Plugins filtered on agentTypes=["claude"]
			// (e.g. cache-keepalive) kept their internal per-session state and wrote
			// keepalive messages into the now-codex PTY. Fix: emit agent-stopped
			// before the store update (filter still matches old type) and agent-started
			// after (filter matches new type).
			let foregroundReturn: string | null = "claude";
			mockInvoke.mockImplementation((cmd: string) => {
				if (cmd === "get_session_foreground_process") return Promise.resolve(foregroundReturn);
				if (cmd === "get_session_leaf_pid") return Promise.resolve(1234);
				if (cmd === "discover_agent_session") return Promise.resolve(null);
				return Promise.resolve(null);
			});

			await testInScopeAsync(async () => {
				const { detectAgentForTerminal } = await import("../../hooks/useAgentPolling");
				const { pluginRegistry } = await import("../../plugins/pluginRegistry");

				const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-trans" }));
				store.update(id, { shellState: "idle" });

				const claudeEvents: string[] = [];
				await pluginRegistry.register(
					{
						id: "test-claude-only",
						onload: (host) => {
							host.onStateChange((e) => {
								if (e.sessionId === "sess-trans") claudeEvents.push(e.type);
							});
						},
						onunload: () => {},
					},
					["pty:write"],
					["claude"],
				);

				const codexEvents: string[] = [];
				await pluginRegistry.register(
					{
						id: "test-codex-only",
						onload: (host) => {
							host.onStateChange((e) => {
								if (e.sessionId === "sess-trans") codexEvents.push(e.type);
							});
						},
						onunload: () => {},
					},
					["pty:write"],
					["codex"],
				);

				// null → claude: claude-filtered plugin gets agent-started
				await detectAgentForTerminal(id, "busy");
				expect(store.get(id)?.agentType).toBe("claude");
				expect(claudeEvents).toEqual(["agent-started"]);
				expect(codexEvents).toEqual([]);

				// claude → codex (direct): claude plugin MUST receive agent-stopped,
				// codex plugin MUST receive agent-started
				foregroundReturn = "codex";
				await detectAgentForTerminal(id, "busy");
				expect(store.get(id)?.agentType).toBe("codex");
				expect(claudeEvents).toEqual(["agent-started", "agent-stopped"]);
				expect(codexEvents).toEqual(["agent-started"]);

				pluginRegistry.unregister("test-claude-only");
				pluginRegistry.unregister("test-codex-only");
			});
		});

		it("clears agentSessionId on the first definitive idle transition and allows re-discovery", async () => {
			// Only source="idle" can clear — polls never clear (sticky agentType fix).
			let phase: "active1" | "idle" | "active2" = "active1";
			let discoverCount = 0;
			mockInvoke.mockImplementation((cmd: string) => {
				if (cmd === "get_session_foreground_process") {
					if (phase === "idle") return Promise.resolve(null);
					return Promise.resolve("claude");
				}
				if (cmd === "get_session_leaf_pid") return Promise.resolve(1234);
				if (cmd === "discover_agent_session") {
					discoverCount++;
					return Promise.resolve(discovered(discoverCount <= 2 ? "uuid-1" : "uuid-2"));
				}
				return Promise.resolve(null);
			});

			await testInScopeAsync(async () => {
				const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));

				const { useAgentPolling, detectAgentForTerminal } = await import("../../hooks/useAgentPolling");
				useAgentPolling();

				await tick(30_000); // poll 1: claude + discovery
				await tick(30_000); // poll 2: still claude + re-discover (same uuid)
				expect(store.get(id)?.agentSessionId).toBe("uuid-1");

				// A shell-idle transition means the prompt has returned, so clear immediately.
				phase = "idle";
				await detectAgentForTerminal(id, "idle");
				expect(store.get(id)?.agentType).toBeNull();
				expect(store.get(id)?.agentSessionId).toBeNull();

				phase = "active2";
				await tick(30_000); // poll 3: re-launched → re-discovery
				expect(store.get(id)?.agentType).toBe("claude");
				expect(store.get(id)?.agentSessionId).toBe("uuid-2");
			});
		});

		it("passes claimed_ids from other terminals to avoid duplicate assignment", async () => {
			let discoverCount = 0;
			mockInvoke.mockImplementation((cmd: string) => {
				if (cmd === "get_session_foreground_process") return Promise.resolve("claude");
				if (cmd === "get_session_leaf_pid") return Promise.resolve(1234);
				if (cmd === "discover_agent_session") {
					discoverCount++;
					return Promise.resolve(discovered(discoverCount === 1 ? "uuid-a" : "uuid-b"));
				}
				return Promise.resolve(null);
			});

			await testInScopeAsync(async () => {
				const id1 = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));
				const id2 = store.add({ sessionId: "sess-2", fontSize: 14, name: "T2", cwd: null, awaitingInput: null });

				const { useAgentPolling } = await import("../../hooks/useAgentPolling");
				useAgentPolling();

				await tick(30_000); // both polled sequentially + both discover

				const discoveryCalls = mockInvoke.mock.calls.filter(([cmd]) => cmd === "discover_agent_session");
				expect(discoveryCalls).toHaveLength(2);

				// Second discovery call must include the first terminal's claimed UUID
				const secondArgs = discoveryCalls[1];
				expect(secondArgs[1]).toHaveProperty("claimedIds");
				expect(secondArgs[1].claimedIds).toContain("uuid-a");

				expect(store.get(id1)?.agentSessionId).toBe("uuid-a");
				expect(store.get(id2)?.agentSessionId).toBe("uuid-b");
			});
		});
	});

	describe("timer lifecycle", () => {
		it("catches up after a terminal receives its backend session id", async () => {
			const { listen } = await import("@tauri-apps/api/event");
			vi.mocked(listen).mockClear();
			mockInvoke.mockResolvedValueOnce([]).mockResolvedValueOnce([
				{
					session_id: "sess-late",
					state: {
						agent_intent: "locking out hashtags",
						last_prompt: "write a campaign with enough words to remain the relevant prompt",
					},
				},
			]);

			await testInScopeAsync(async () => {
				const id = store.add(makeTerminal({ name: "Grok" }));
				const { useAgentPolling } = await import("../../hooks/useAgentPolling");
				useAgentPolling();
				await tick(0);
				expect(store.get(id)?.agentIntent).toBeNull();
				const subscriptionCalls = vi.mocked(listen).mock.calls.length;
				const timerCount = vi.getTimerCount();

				store.update(id, { sessionId: "sess-late" });
				await tick(0);

				expect(mockInvoke).toHaveBeenCalledTimes(2);
				expect(store.get(id)?.agentIntent).toBe("locking out hashtags");
				expect(store.get(id)?.lastPrompt).toBe("write a campaign with enough words to remain the relevant prompt");
				expect(vi.mocked(listen).mock.calls).toHaveLength(subscriptionCalls);
				expect(vi.getTimerCount()).toBe(timerCount);
			});
		});

		it("runs one trailing catch-up when more session bindings arrive during the initial request", async () => {
			let resolveInitial!: (value: unknown) => void;
			const snapshot = [
				{ session_id: "sess-1", state: { agent_intent: "first intent" } },
				{ session_id: "sess-2", state: { agent_intent: "second intent" } },
			];
			mockInvoke
				.mockImplementationOnce(
					() =>
						new Promise((resolve) => {
							resolveInitial = resolve;
						}),
				)
				.mockResolvedValueOnce(snapshot);

			await testInScopeAsync(async () => {
				const first = store.add(makeTerminal({ name: "First", sessionId: "sess-1" }));
				const { useAgentPolling } = await import("../../hooks/useAgentPolling");
				useAgentPolling();
				await tick(0);

				const second = store.add(makeTerminal({ name: "Second", sessionId: "sess-2" }));
				resolveInitial(snapshot);
				await tick(0);

				expect(mockInvoke).toHaveBeenCalledTimes(2);
				expect(store.get(first)?.agentIntent).toBe("first intent");
				expect(store.get(second)?.agentIntent).toBe("second intent");
			});
		});

		it("does not restart the 30s fallback poll when a terminal is added mid-cycle", async () => {
			mockInvoke.mockImplementation((cmd: string) => {
				if (cmd === "get_session_foreground_process") return Promise.resolve(null);
				// Listing both sessions up front (sess-2 doesn't exist yet) keeps the
				// mount-time lifecycle catch-up from treating either as exited and
				// clearing its sessionId — which would make detectAgentForTerminal bail
				// out before the fallback poll under test ever gets a chance to run.
				if (cmd === "list_active_sessions")
					return Promise.resolve([{ session_id: "sess-1" }, { session_id: "sess-2" }]);
				return Promise.resolve(null);
			});

			await testInScopeAsync(async () => {
				store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));

				const { useAgentPolling } = await import("../../hooks/useAgentPolling");
				useAgentPolling();
				await tick(0); // flush the initial lifecycle sync
				mockInvoke.mockClear();

				await tick(25_000); // just short of the 30s fallback poll

				// Tab churn mid-cycle must not reset the fallback timer's countdown.
				store.add(makeTerminal({ name: "T2", sessionId: "sess-2" }));

				await tick(5_000); // total 30s since useAgentPolling() started

				expect(mockInvoke).toHaveBeenCalledWith("get_session_foreground_process", { sessionId: "sess-1" });
				expect(mockInvoke).toHaveBeenCalledWith("get_session_foreground_process", { sessionId: "sess-2" });
			});
		});

		/// Supersedes the visibility-gated 1 Hz lifecycle poll (#652-0114). Gating
		/// it on `document.visibilityState` only silenced a hidden window; a
		/// visible idle one kept sampling forever. The backend now pushes
		/// `session-state-changed`, so there is nothing left to gate — and a
		/// visible window is the stricter case to assert.
		it("never re-polls list_active_sessions after the mount-time catch-up", async () => {
			mockInvoke.mockResolvedValue([]);
			expect(document.visibilityState).not.toBe("hidden");

			await testInScopeAsync(async () => {
				store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));

				const { useAgentPolling } = await import("../../hooks/useAgentPolling");
				useAgentPolling();
				await tick(0);
				expect(mockInvoke).toHaveBeenCalledWith("list_active_sessions");

				mockInvoke.mockClear();
				await tick(10_000); // 10 lifecycle-poll ticks would have fired

				expect(mockInvoke).not.toHaveBeenCalledWith("list_active_sessions");
			});
		});
	});

	describe("session-state push", () => {
		/** Capture the handlers `subscribeEvents` registers on the Tauri window. */
		async function captureWindowEvents(): Promise<Map<string, (event: { payload: unknown }) => void>> {
			const listeners = new Map<string, (event: { payload: unknown }) => void>();
			const { listen } = await import("@tauri-apps/api/event");
			vi.mocked(listen).mockImplementation(async (name, handler) => {
				listeners.set(name, handler as (event: { payload: unknown }) => void);
				return () => {};
			});
			return listeners;
		}

		/** The mount-time catch-up must see the session, or it reaps the terminal
		 *  and the push below would have no owner to apply to. */
		const catchUpSnapshot = [{ session_id: "sess-1", state: {} }];

		it("converges the store from a session-state-changed push, with no timer and no snapshot", async () => {
			mockInvoke.mockResolvedValue(catchUpSnapshot);
			const listeners = await captureWindowEvents();

			await testInScopeAsync(async () => {
				const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));

				const { useAgentPolling } = await import("../../hooks/useAgentPolling");
				useAgentPolling();
				await tick(0); // subscribeEvents resolves its dynamic import

				const push = listeners.get("session-state-changed");
				expect(push, "the hook must subscribe to the backend push").toBeDefined();

				// From here on nothing may reach the backend: no timer is advanced
				// and no snapshot is fetched, so the event is the only input.
				mockInvoke.mockClear();
				push?.({
					payload: {
						session_id: "sess-1",
						state: {
							shell_state: "busy",
							agent_state: "awaiting_input",
							awaiting_input: true,
							question_confident: true,
							queued_commands: 2,
						},
					},
				});

				expect(store.get(id)?.agentState).toBe("awaiting_input");
				expect(store.get(id)?.awaitingInput).toBe("question");
				expect(store.get(id)?.awaitingInputConfident).toBe(true);
				expect(store.get(id)?.shellState).toBe("busy");
				expect(store.get(id)?.queuedCommands).toBe(2);
				expect(mockInvoke).not.toHaveBeenCalled();
			});
		});

		it("hydrates and clears prompt and intent from complete session-state pushes", async () => {
			mockInvoke.mockResolvedValue(catchUpSnapshot);
			const listeners = await captureWindowEvents();

			await testInScopeAsync(async () => {
				const id = store.add(makeTerminal({ name: "Custom Grok", sessionId: "sess-1" }));
				store.update(id, { nameIsCustom: true });

				const { useAgentPolling } = await import("../../hooks/useAgentPolling");
				useAgentPolling();
				await tick(0);
				const push = listeners.get("session-state-changed");

				push?.({
					payload: {
						session_id: "sess-1",
						state: {
							agent_intent: "locking out hashtags",
							last_prompt: "write a campaign with enough words to remain the relevant prompt",
						},
					},
				});
				expect(store.get(id)?.agentIntent).toBe("locking out hashtags");
				expect(store.get(id)?.lastPrompt).toBe("write a campaign with enough words to remain the relevant prompt");
				expect(store.get(id)?.name).toBe("Custom Grok");

				// SessionState serializes Option::None by omitting these fields. Each
				// push is a complete snapshot, so omission retracts prior context.
				push?.({ payload: { session_id: "sess-1", state: { shell_state: "idle" } } });
				expect(store.get(id)?.agentIntent).toBeNull();
				expect(store.get(id)?.lastPrompt).toBeNull();

				push?.({
					payload: {
						session_id: "sess-1",
						state: { agent_intent: "temporary intent", last_prompt: "temporary retained prompt" },
					},
				});
				push?.({ payload: { session_id: "sess-1", state: null } });
				expect(store.get(id)?.agentIntent).toBeNull();
				expect(store.get(id)?.lastPrompt).toBeNull();
			});
		});

		it("does not let an older catch-up overwrite context from a later push", async () => {
			let resolveSnapshot!: (value: unknown) => void;
			mockInvoke.mockImplementationOnce(
				() =>
					new Promise((resolve) => {
						resolveSnapshot = resolve;
					}),
			);
			const listeners = await captureWindowEvents();

			await testInScopeAsync(async () => {
				const id = store.add(makeTerminal({ name: "Grok", sessionId: "sess-1" }));
				const { useAgentPolling } = await import("../../hooks/useAgentPolling");
				useAgentPolling();
				await tick(0);

				listeners.get("session-state-changed")?.({
					payload: {
						session_id: "sess-1",
						state: {
							agent_intent: "new intent",
							last_prompt: "new prompt with ten complete words for capture here now",
						},
					},
				});
				resolveSnapshot([
					{
						session_id: "sess-1",
						state: { agent_intent: "old intent", last_prompt: "old prompt from the delayed backend snapshot" },
					},
				]);
				await tick(0);

				expect(store.get(id)?.agentIntent).toBe("new intent");
				expect(store.get(id)?.lastPrompt).toBe("new prompt with ten complete words for capture here now");
			});
		});

		it("applies a later push, so a cleared question does not stay latched", async () => {
			mockInvoke.mockResolvedValue(catchUpSnapshot);
			const listeners = await captureWindowEvents();

			await testInScopeAsync(async () => {
				const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));

				const { useAgentPolling } = await import("../../hooks/useAgentPolling");
				useAgentPolling();
				await tick(0);
				const push = listeners.get("session-state-changed");

				push?.({
					payload: {
						session_id: "sess-1",
						state: { awaiting_input: true, question_confident: true, agent_state: "awaiting_input" },
					},
				});
				expect(store.get(id)?.awaitingInput).toBe("question");

				// serde skips `queued_commands` at zero and omits `awaiting_input`
				// when false, so the retraction arrives as an absence.
				push?.({ payload: { session_id: "sess-1", state: { agent_state: "working", shell_state: "busy" } } });
				expect(store.get(id)?.awaitingInput).toBeNull();
				expect(store.get(id)?.awaitingInputConfident).toBe(false);
				expect(store.get(id)?.agentState).toBe("working");
				expect(store.get(id)?.queuedCommands).toBe(0);
			});
		});

		it("ignores a push for a session no terminal owns", async () => {
			mockInvoke.mockResolvedValue(catchUpSnapshot);
			const listeners = await captureWindowEvents();

			await testInScopeAsync(async () => {
				const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-1" }));

				const { useAgentPolling } = await import("../../hooks/useAgentPolling");
				useAgentPolling();
				await tick(0);
				store.update(id, { agentState: "working" });
				const push = listeners.get("session-state-changed");

				push?.({ payload: { session_id: "someone-elses-session", state: { agent_state: "idle" } } });
				push?.({ payload: null });
				push?.({ payload: { state: { agent_state: "idle" } } });

				expect(store.get(id)?.agentState).toBe("working");
			});
		});

		// #791-055e. `remote_mirror.rs` repeats a remote daemon's events on the
		// local bus under the daemon's own name, so a session running on another
		// machine reaches this handler with nothing here changed. The absence of
		// a second subscription IS the parity: a future event type crosses for
		// free, and there is no remote-only code path to keep in step.
		it("a remote session raises the same question badge through the same handler", async () => {
			mockInvoke.mockResolvedValue([{ session_id: "vps-sess", state: {}, connection_id: "vps" }]);
			const listeners = await captureWindowEvents();

			await testInScopeAsync(async () => {
				const id = store.add(makeTerminal({ name: "VPS", sessionId: "vps-sess" }));

				const { useAgentPolling } = await import("../../hooks/useAgentPolling");
				const { pluginRegistry } = await import("../../plugins/pluginRegistry");
				const notify = vi.spyOn(pluginRegistry, "dispatchStructuredEvent");
				useAgentPolling();
				await tick(0);

				expect(
					[...listeners.keys()].filter((name) => name.startsWith("remote") && name !== "remote-connection-status"),
					"a mirrored event must need no subscription of its own",
				).toEqual([]);

				listeners.get("session-state-changed")?.({
					payload: {
						session_id: "vps-sess",
						state: { awaiting_input: true, question_confident: true, agent_state: "awaiting_input" },
					},
				});

				expect(store.get(id)?.awaitingInput).toBe("question");
				expect(store.get(id)?.awaitingInputConfident).toBe(true);
				expect(store.get(id)?.agentState).toBe("awaiting_input");
				// The notification too, and through the same call a local agent
				// makes — one applier, no branch on where the session runs.
				expect(notify).toHaveBeenCalledWith("awaiting", { awaiting: true, confident: true }, "vps-sess");
				notify.mockRestore();
			});
		});

		it("the catch-up applies a mirrored row rather than skipping it", async () => {
			// `list_active_sessions` answers local plus mirrored rows since
			// #791-055e; `connection_id` names the machine and must not make the
			// row unrecognisable to the applier.
			const id = store.add(makeTerminal({ name: "VPS", sessionId: "vps-sess" }));
			mockInvoke.mockResolvedValueOnce([
				{
					session_id: "vps-sess",
					connection_id: "vps",
					state: { shell_state: "busy", agent_state: "working" },
				},
			]);
			const { syncAgentLifecycleStates } = await import("../../hooks/useAgentPolling");

			await syncAgentLifecycleStates();
			expect(store.get(id)?.agentState).toBe("working");
			expect(store.get(id)?.shellState).toBe("busy");
		});
	});
});

// Catches: recovery snapshots crossing tabs or an omitted clear leaving a stale overlay.
it("routes and retracts provider recovery in complete backend snapshots", async () => {
	const { terminalsStore } = await import("../../stores/terminals");
	const a = terminalsStore.add(makeTerminal({ sessionId: "recover-a" }));
	const b = terminalsStore.add(makeTerminal({ sessionId: "recover-b" }));
	const { applySessionStateEvent } = await import("../../hooks/useAgentPolling");
	applySessionStateEvent({
		session_id: "recover-a",
		state: { agent_type: "ego", provider_recovery: { phase: "waiting", attempt: 2, next_attempt_ms: 10000 } },
	});
	expect(terminalsStore.get(a)?.providerRecovery?.attempt).toBe(2);
	expect(terminalsStore.get(b)?.providerRecovery).toBeUndefined();
	applySessionStateEvent({ session_id: "recover-a", state: { agent_type: "ego" } });
	expect(terminalsStore.get(a)?.providerRecovery).toBeNull();
});
