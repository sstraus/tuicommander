import { createEffect, createRoot } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { statusBarTicker } from "../../stores/statusBarTicker";

beforeEach(() => {
	statusBarTicker.clear();
	vi.useFakeTimers();
});

afterEach(() => {
	statusBarTicker.clear();
	vi.useRealTimers();
});

describe("statusBarTicker", () => {
	it("returns null when empty", () => {
		expect(statusBarTicker.getRotationState().message).toBeNull();
	});

	it("returns the only message when one is added", () => {
		statusBarTicker.addMessage({
			id: "usage",
			pluginId: "claude-usage",
			text: "Claude: 29% (5h)",
			priority: 50,
			ttlMs: 60_000,
		});
		const state = statusBarTicker.getRotationState();
		expect(state.message).not.toBeNull();
		expect(state.message!.text).toBe("Claude: 29% (5h)");
		expect(state.message!.pluginId).toBe("claude-usage");
		expect(state.current).toBe(1);
		expect(state.total).toBe(1);
	});

	it("shows highest priority message first", () => {
		statusBarTicker.addMessage({
			id: "low",
			pluginId: "p1",
			text: "Low priority",
			priority: 10,
			ttlMs: 0,
		});
		statusBarTicker.addMessage({
			id: "high",
			pluginId: "p1",
			text: "High priority",
			priority: 50,
			ttlMs: 0,
		});
		expect(statusBarTicker.getRotationState().message!.text).toBe("High priority");
	});

	it("rotates among same-priority messages", () => {
		statusBarTicker.addMessage({ id: "a", pluginId: "p1", text: "A", priority: 50, ttlMs: 0 });
		statusBarTicker.addMessage({ id: "b", pluginId: "p1", text: "B", priority: 50, ttlMs: 0 });

		const first = statusBarTicker.getRotationState().message!.text;
		statusBarTicker._rotate();
		const second = statusBarTicker.getRotationState().message!.text;

		// Should show different messages after rotation
		expect([first, second].sort()).toEqual(["A", "B"]);
	});

	it("deduplicates by id + pluginId on addMessage", () => {
		statusBarTicker.addMessage({ id: "usage", pluginId: "p1", text: "v1", priority: 50, ttlMs: 0 });
		statusBarTicker.addMessage({ id: "usage", pluginId: "p1", text: "v2", priority: 50, ttlMs: 0 });

		expect(statusBarTicker.getAll().length).toBe(1);
		expect(statusBarTicker.getAll()[0].text).toBe("v2");
	});

	it("removes message by id and pluginId", () => {
		statusBarTicker.addMessage({ id: "usage", pluginId: "p1", text: "test", priority: 50, ttlMs: 0 });
		statusBarTicker.removeMessage("usage", "p1");
		expect(statusBarTicker.getRotationState().message).toBeNull();
	});

	it("removes all messages for a plugin", () => {
		statusBarTicker.addMessage({ id: "a", pluginId: "p1", text: "A", priority: 50, ttlMs: 0 });
		statusBarTicker.addMessage({ id: "b", pluginId: "p1", text: "B", priority: 50, ttlMs: 0 });
		statusBarTicker.addMessage({ id: "c", pluginId: "p2", text: "C", priority: 50, ttlMs: 0 });

		statusBarTicker.removeAllForPlugin("p1");
		expect(statusBarTicker.getAll().length).toBe(1);
		expect(statusBarTicker.getAll()[0].pluginId).toBe("p2");
	});

	it("scavenges expired messages based on TTL", () => {
		statusBarTicker.addMessage({ id: "short", pluginId: "p1", text: "Short-lived", priority: 50, ttlMs: 500 });
		statusBarTicker.addMessage({ id: "long", pluginId: "p1", text: "Long-lived", priority: 50, ttlMs: 0 });

		expect(statusBarTicker.getAll().length).toBe(2);

		// Advance time past the TTL
		vi.advanceTimersByTime(600);
		statusBarTicker._scavenge();

		expect(statusBarTicker.getAll().length).toBe(1);
		expect(statusBarTicker.getAll()[0].text).toBe("Long-lived");
	});

	it("persistent messages (ttlMs=0) never expire", () => {
		statusBarTicker.addMessage({ id: "persistent", pluginId: "p1", text: "Always here", priority: 50, ttlMs: 0 });

		vi.advanceTimersByTime(1_000_000);
		statusBarTicker._scavenge();

		expect(statusBarTicker.getAll().length).toBe(1);
	});

	it("clear removes all messages", () => {
		statusBarTicker.addMessage({ id: "a", pluginId: "p1", text: "A", priority: 50, ttlMs: 0 });
		statusBarTicker.addMessage({ id: "b", pluginId: "p2", text: "B", priority: 50, ttlMs: 0 });

		statusBarTicker.clear();
		expect(statusBarTicker.getAll().length).toBe(0);
		expect(statusBarTicker.getRotationState().message).toBeNull();
	});

	it("low-priority messages are excluded from rotation", () => {
		statusBarTicker.addMessage({ id: "low", pluginId: "p1", text: "Low", priority: 5, ttlMs: 0 });
		expect(statusBarTicker.getRotationState().message).toBeNull();
		expect(statusBarTicker.getAll().length).toBe(1);
	});
});

/**
 * The scavenge timer fires once a second for as long as any message exists.
 * `Array.prototype.filter` always allocates, so republishing the signal on a
 * tick that expired nothing woke every ticker subscriber 60 times a minute for
 * no change at all. The tick must be silent unless it actually removed a
 * message.
 */
describe("statusBarTicker scavenge quiet ticks", () => {
	/** Runs of an effect reading the message list — i.e. subscriber wakeups. */
	const countWakeups = (drive: () => void): number => {
		let runs = 0;
		const dispose = createRoot((d) => {
			createEffect(() => {
				statusBarTicker.getAll();
				runs++;
			});
			return d;
		});
		// The effect's first run is the subscription itself, not a wakeup.
		const baseline = runs;
		drive();
		dispose();
		return runs - baseline;
	};

	it("does not republish when nothing expired", () => {
		statusBarTicker.addMessage({ id: "persistent", pluginId: "p1", text: "Always here", priority: 50, ttlMs: 0 });
		const wakeups = countWakeups(() => {
			for (let i = 0; i < 5; i++) statusBarTicker._scavenge();
		});
		expect(wakeups).toBe(0);
	});

	it("still republishes on the tick that removes a message", () => {
		statusBarTicker.addMessage({ id: "short", pluginId: "p1", text: "Short", priority: 50, ttlMs: 500 });
		statusBarTicker.addMessage({ id: "long", pluginId: "p1", text: "Long", priority: 50, ttlMs: 0 });
		vi.advanceTimersByTime(600);
		const wakeups = countWakeups(() => {
			statusBarTicker._scavenge();
			statusBarTicker._scavenge();
		});
		// One wakeup for the removal, silence for the tick that found nothing.
		expect(wakeups).toBe(1);
		expect(statusBarTicker.getAll().map((m) => m.text)).toEqual(["Long"]);
	});
});

describe("statusBarTicker side-by-side pool", () => {
	const add = (id: string, priority: number) =>
		statusBarTicker.addMessage({ id, pluginId: "p", text: id, priority, ttlMs: 0 });

	it("getPool returns the normal tier sorted, without low-priority messages", () => {
		add("b", 20);
		add("a", 40);
		add("low", 5);
		expect(statusBarTicker.getPool().map((m) => m.id)).toEqual(["a", "b"]);
	});

	it("getPool keeps urgent messages only", () => {
		add("normal", 40);
		add("urgent", 100);
		expect(statusBarTicker.getPool().map((m) => m.id)).toEqual(["urgent"]);
	});

	it("rotation skips the messages shown side by side — catches a pinned message also rotating", () => {
		add("a", 40);
		add("b", 30);
		add("c", 20);
		for (let i = 0; i < 4; i++) {
			const state = statusBarTicker.getRotationState(undefined, 1);
			expect(state.message!.id).not.toBe("a");
			expect(state.total).toBe(2);
			statusBarTicker._rotate();
		}
	});

	it("rotation is empty when every message is shown side by side", () => {
		add("a", 40);
		expect(statusBarTicker.getRotationState(undefined, 1).message).toBeNull();
	});
});
