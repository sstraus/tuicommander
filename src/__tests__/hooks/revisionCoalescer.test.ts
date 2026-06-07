import { describe, expect, it, vi } from "vitest";

import { createRevisionCoalescer } from "../../hooks/revisionCoalescer";

/** Manual frame scheduler: capture the callback and fire it on demand. */
function manualScheduler() {
	let queued: (() => void) | null = null;
	let handles = 0;
	const schedule = (cb: () => void) => {
		queued = cb;
		return ++handles;
	};
	const cancel = vi.fn();
	const flush = () => {
		const cb = queued;
		queued = null;
		cb?.();
	};
	return { schedule, cancel, flush, hasPending: () => queued !== null };
}

describe("createRevisionCoalescer", () => {
	it("collapses multiple same-frame bumps for one repo into a single delivery", () => {
		const bump = vi.fn();
		const sched = manualScheduler();
		const c = createRevisionCoalescer(bump, sched.schedule, sched.cancel);

		c.bump("/repo");
		c.bump("/repo");
		c.bump("/repo");
		// Nothing delivered until the frame fires.
		expect(bump).not.toHaveBeenCalled();

		sched.flush();
		expect(bump).toHaveBeenCalledTimes(1);
		expect(bump).toHaveBeenCalledWith("/repo");
	});

	it("delivers one bump per distinct repo in the same frame", () => {
		const bump = vi.fn();
		const sched = manualScheduler();
		const c = createRevisionCoalescer(bump, sched.schedule, sched.cancel);

		c.bump("/a");
		c.bump("/b");
		c.bump("/a");
		sched.flush();

		expect(bump).toHaveBeenCalledTimes(2);
		expect(bump.mock.calls.map((args) => args[0]).sort()).toEqual(["/a", "/b"]);
	});

	it("does NOT lose bumps across separate frames (re-arms after flush)", () => {
		const bump = vi.fn();
		const sched = manualScheduler();
		const c = createRevisionCoalescer(bump, sched.schedule, sched.cancel);

		c.bump("/repo");
		sched.flush();
		c.bump("/repo");
		sched.flush();

		expect(bump).toHaveBeenCalledTimes(2);
	});

	it("schedules only one frame for a same-frame burst", () => {
		const bump = vi.fn();
		const schedule = vi.fn((_cb: () => void) => 1);
		const c = createRevisionCoalescer(bump, schedule, vi.fn());

		c.bump("/repo");
		c.bump("/other");
		c.bump("/repo");

		expect(schedule).toHaveBeenCalledTimes(1);
	});

	it("dispose cancels a pending frame and drops queued bumps", () => {
		const bump = vi.fn();
		const sched = manualScheduler();
		const c = createRevisionCoalescer(bump, sched.schedule, sched.cancel);

		c.bump("/repo");
		c.dispose();
		expect(sched.cancel).toHaveBeenCalledWith(1);

		// Flushing the captured callback after dispose must not deliver the dropped bump.
		sched.flush();
		expect(bump).not.toHaveBeenCalled();
	});

	it("a bump fired during flush is delivered on the NEXT frame, not the current one", () => {
		const sched = manualScheduler();
		const delivered: string[] = [];
		let reentered = false;
		const c = createRevisionCoalescer(
			(repoPath) => {
				delivered.push(repoPath);
				if (!reentered) {
					reentered = true;
					c.bump("/during-flush"); // re-arm during flush
				}
			},
			sched.schedule,
			sched.cancel,
		);

		c.bump("/first");
		sched.flush();
		expect(delivered).toEqual(["/first"]); // re-armed bump not yet delivered

		sched.flush();
		expect(delivered).toEqual(["/first", "/during-flush"]);
	});
});
