import { expect, it, vi } from "vitest";
import { mockInvoke } from "../__tests__/mocks/tauri";
import { invoke } from "../invoke";
import type { subscribeEvents } from "../transport";
import { createSettingsRefreshCoordinator } from "./settingsRefresh";

const flush = async () => {
	for (let i = 0; i < 10; i++) await Promise.resolve();
};

function fixture() {
	let handlers!: Record<string, (payload: unknown) => void>;
	const unlisten = vi.fn();
	const subscribe = vi.fn(async (callbacks: typeof handlers) => {
		handlers = callbacks;
		return unlisten;
	});
	const coordinator = createSettingsRefreshCoordinator(subscribe as typeof subscribeEvents);
	return {
		coordinator,
		unlisten,
		subscribe,
		status(id: string, status: string) {
			handlers["remote-connection-status"]({ id, status });
		},
		emit(section: string, generation: number, connection?: string) {
			handlers["settings-changed"]({
				section,
				generation,
				persisted: true,
				applied: true,
				...(connection ? { __tuic_origin: { connection } } : {}),
			});
		},
	};
}

// Catches: the invoke layer coalescing an invalidated read with its replacement,
// so a post-write refresh adopts the pre-write snapshot despite its newer sequence.
it("refetches_after_an_invalidated_deduplicated_ipc_read_instead_of_adopting_its_old_snapshot", async () => {
	const f = fixture();
	let finishOld!: (value: { enabled: boolean }) => void;
	mockInvoke.mockReset();
	mockInvoke
		.mockImplementationOnce(
			() =>
				new Promise((resolve) => {
					finishOld = resolve;
				}),
		)
		.mockResolvedValue({ enabled: true });
	const apply = vi.fn();
	const stop = f.coordinator.register({
		section: "notifications",
		read: () => invoke<{ enabled: boolean }>("load_notification_config"),
		apply,
	});
	await flush();
	f.emit("notifications", 1);
	await flush();
	finishOld({ enabled: false });
	await flush();
	await vi.waitFor(() => expect(apply.mock.calls.at(-1)).toEqual([{ enabled: true }]));
	expect(apply.mock.calls).not.toContainEqual([{ enabled: false }]);
	stop();
});

// Catches: a delayed older owner read overwriting a later accepted read.
it("out_of_order_revision_ignored", async () => {
	const f = fixture();
	const replies: ((value: string) => void)[] = [];
	const apply = vi.fn();
	const stop = f.coordinator.register({
		section: "app",
		read: () => new Promise<string>((resolve) => replies.push(resolve)),
		apply,
	});
	await flush();
	f.emit("app", 1);
	await flush();
	replies[0]("stale state");
	await flush();
	expect(apply).not.toHaveBeenCalled();
	replies[1]("newer state");
	await flush();
	expect(apply.mock.calls).toEqual([["newer state"]]);
	f.emit("app", 1);
	await flush();
	expect(replies).toHaveLength(2);
	stop();
});

// Catches: a remote notice reloading local settings, or one section suppressing another's generation.
it("routes_by_owner_and_section_with_one_window_subscription", async () => {
	const f = fixture();
	const local = vi.fn().mockResolvedValue("local");
	const remote = vi.fn().mockResolvedValue("remote");
	const notifications = vi.fn().mockResolvedValue("notifications");
	const stops = [
		f.coordinator.register({ section: "agents", read: local, apply: vi.fn() }),
		f.coordinator.register({ section: "agents", connectionId: "daemon", read: remote, apply: vi.fn() }),
		f.coordinator.register({ section: "notifications", read: notifications, apply: vi.fn() }),
	];
	await flush();
	local.mockClear();
	remote.mockClear();
	notifications.mockClear();
	f.emit("agents", 9, "daemon");
	f.emit("notifications", 8);
	await flush();
	expect(local).not.toHaveBeenCalled();
	expect(remote).toHaveBeenCalledTimes(1);
	expect(notifications).toHaveBeenCalledTimes(1);
	expect(f.subscribe).toHaveBeenCalledTimes(1);
	for (const stop of stops) stop();
	expect(f.unlisten).toHaveBeenCalledTimes(1);
});

// Catches: a disposed panel receiving an in-flight settings result and mutating its old form.
it("disposed_reader_never_applies_an_inflight_snapshot", async () => {
	const f = fixture();
	let finish!: (value: string) => void;
	const apply = vi.fn();
	const stop = f.coordinator.register({
		section: "ego",
		read: () =>
			new Promise<string>((resolve) => {
				finish = resolve;
			}),
		apply,
	});
	await flush();
	stop();
	finish("late reply");
	await flush();
	expect(apply).not.toHaveBeenCalled();
});

// Catches: disconnect applying a late daemon read or issuing reads against a known disconnected owner.
it("disconnect_cancels_late_read_until_reconnect", async () => {
	const f = fixture();
	const replies: ((value: string) => void)[] = [];
	const apply = vi.fn();
	const read = vi.fn(() => new Promise<string>((resolve) => replies.push(resolve)));
	const stop = f.coordinator.register({ section: "agents", connectionId: "daemon", read, apply });
	await flush();
	f.status("daemon", "disconnected");
	replies[0]("old daemon");
	f.emit("agents", 90, "daemon");
	await flush();
	expect(apply).not.toHaveBeenCalled();
	expect(read).toHaveBeenCalledTimes(1);
	f.status("daemon", "connected");
	await flush();
	replies[1]("reconnected daemon");
	await flush();
	expect(apply).toHaveBeenCalledExactlyOnceWith("reconnected daemon");
	stop();
});

// Catches: teardown during subscription startup stranding a newly mounted reader without events.
it("new_lifetime_replaces_pending_subscription", async () => {
	const subscriptions: Array<(stop: () => void) => void> = [];
	const subscribe = vi.fn(() => new Promise<() => void>((resolve) => subscriptions.push(resolve)));
	const coordinator = createSettingsRefreshCoordinator(subscribe as typeof subscribeEvents);
	const oldRead = vi.fn().mockResolvedValue("old");
	const stopOld = coordinator.register({ section: "app", read: oldRead, apply: vi.fn() });
	stopOld();
	const read = vi.fn().mockResolvedValue("new");
	const apply = vi.fn();
	const stop = coordinator.register({ section: "app", read, apply });
	const oldUnlisten = vi.fn();
	subscriptions[0](oldUnlisten);
	await flush();
	expect(oldUnlisten).toHaveBeenCalledTimes(1);
	expect(subscribe).toHaveBeenCalledTimes(2);
	const unlisten = vi.fn();
	subscriptions[1](unlisten);
	await flush();
	expect(oldRead).not.toHaveBeenCalled();
	expect(apply).toHaveBeenCalledExactlyOnceWith("new");
	stop();
	expect(unlisten).toHaveBeenCalledTimes(1);
});

// Catches: one transient owner read failure leaving a connected IPC window stale forever.
it("retries a failed read without another generation and cancels retry on disposal", async () => {
	vi.useFakeTimers();
	const { appLogger } = await import("../stores/appLogger");
	const warn = vi.spyOn(appLogger, "warn").mockImplementation(() => {});
	const f = fixture();
	const read = vi.fn().mockRejectedValueOnce(new Error("offline")).mockResolvedValue("peer");
	const apply = vi.fn();
	const stop = f.coordinator.register({ section: "app", read, apply });
	try {
		await flush();
		await vi.advanceTimersByTimeAsync(1000);
		expect(apply).toHaveBeenCalledWith("peer");
		read.mockRejectedValue(new Error("offline"));
		f.emit("app", 1);
		await flush();
		stop();
		const count = read.mock.calls.length;
		await vi.advanceTimersByTimeAsync(60000);
		expect(read).toHaveBeenCalledTimes(count);
	} finally {
		stop();
		warn.mockRestore();
		vi.useRealTimers();
	}
});

// Catches: a failed first subscription preventing reads and all later refreshes until remount.
it("retries failed subscription while initial owner reads remain independent", async () => {
	vi.useFakeTimers();
	const { appLogger } = await import("../stores/appLogger");
	const warn = vi.spyOn(appLogger, "warn").mockImplementation(() => {});
	const unlisten = vi.fn();
	const subscribe = vi.fn().mockRejectedValueOnce(new Error("offline")).mockResolvedValue(unlisten);
	const coordinator = createSettingsRefreshCoordinator(subscribe);
	const read = vi.fn().mockResolvedValue("initial");
	const stop = coordinator.register({ section: "app", read, apply: vi.fn() });
	try {
		await flush();
		expect(read).toHaveBeenCalled();
		await vi.advanceTimersByTimeAsync(1000);
		expect(subscribe).toHaveBeenCalledTimes(2);
		stop();
		expect(unlisten).toHaveBeenCalledOnce();
	} finally {
		stop();
		warn.mockRestore();
		vi.useRealTimers();
	}
});

// Catches: a disposed coordinator retaining a failed-subscription retry and reopening the event stream.
it("disposal cancels a pending subscription retry", async () => {
	vi.useFakeTimers();
	const { appLogger } = await import("../stores/appLogger");
	const warn = vi.spyOn(appLogger, "warn").mockImplementation(() => {});
	const subscribe = vi.fn().mockRejectedValue(new Error("offline"));
	const coordinator = createSettingsRefreshCoordinator(subscribe);
	const read = vi.fn().mockResolvedValue("initial");
	const stop = coordinator.register({ section: "app", read, apply: vi.fn() });
	try {
		await flush();
		expect(subscribe).toHaveBeenCalledOnce();
		stop();
		await vi.advanceTimersByTimeAsync(60000);
		expect(subscribe).toHaveBeenCalledOnce();
	} finally {
		stop();
		warn.mockRestore();
		vi.useRealTimers();
	}
});

// Catches: a legacy invalidation refreshing a different owner or section, or duplicating a settings read.
it("coalesces explicit invalidation with settings events within the selected owner and section", async () => {
	const f = fixture();
	const local = vi.fn().mockResolvedValue("local");
	const remote = vi.fn().mockResolvedValue("remote");
	const other = vi.fn().mockResolvedValue("other");
	const stops = [
		f.coordinator.register({ section: "repositories_registration", read: local, apply: vi.fn() }),
		f.coordinator.register({
			section: "repositories_registration",
			connectionId: "daemon",
			read: remote,
			apply: vi.fn(),
		}),
		f.coordinator.register({ section: "app", read: other, apply: vi.fn() }),
	];
	try {
		await flush();
		local.mockClear();
		remote.mockClear();
		other.mockClear();
		f.coordinator.invalidate("repositories_registration");
		f.emit("repositories_registration", 1);
		await flush();
		expect(local).toHaveBeenCalledTimes(1);
		expect(remote).not.toHaveBeenCalled();
		expect(other).not.toHaveBeenCalled();
		f.coordinator.invalidate("repositories_registration", "daemon");
		await flush();
		expect(remote).toHaveBeenCalledTimes(1);
		expect(local).toHaveBeenCalledTimes(1);
	} finally {
		for (const stop of stops) stop();
	}
});
