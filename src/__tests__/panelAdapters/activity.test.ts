import { describe, expect, it } from "vitest";
import { snapshotToRows } from "../../panelAdapters/activity";
import type { ActivitySnapshot, ActivityTerminalRow } from "../../utils/activitySnapshot";

/**
 * The detached Activity panel re-serializes its whole snapshot once per second
 * (`syncIntervalMs: 1000`) and the payload crosses a postMessage boundary, so
 * every tick arrives as brand-new objects even when nothing about a terminal
 * changed. `snapshotToRows` then allocated a brand-new `TerminalRow` per
 * terminal, and `ActivityDashboard` renders them through a reference-keyed
 * `<For>` — so every row's DOM subtree was torn down and rebuilt every second,
 * forever, for a panel showing identical text.
 *
 * The fix is the same one `src/mobile/useSessions.ts:66-74` already uses:
 * return the previous object when the new one is field-equal.
 */
function terminal(over: Partial<ActivityTerminalRow> = {}): ActivityTerminalRow {
	return {
		id: "t1",
		name: "shell",
		shellState: "idle",
		awaitingInput: null,
		sessionId: "s1",
		agentType: "claude",
		agentIntent: null,
		currentTask: null,
		lastPrompt: null,
		activeSubTasks: 0,
		cwd: "/repo/tuicommander",
		project: "tuicommander",
		lastDataAt: 1000,
		idleSince: 900,
		isActive: false,
		isRateLimited: false,
		agentState: null,
		backgroundWork: false,
		isBusy: false,
		isPromoted: false,
		subAgentTag: null,
		...over,
	};
}

/** A fresh snapshot object graph — what the sync channel actually delivers. */
function snapshot(...terminals: ActivityTerminalRow[]): ActivitySnapshot {
	return JSON.parse(JSON.stringify({ terminals })) as ActivitySnapshot;
}

describe("snapshotToRows row identity", () => {
	it("returns the previous row object when a terminal is unchanged", () => {
		const first = snapshotToRows(snapshot(terminal()));
		const second = snapshotToRows(snapshot(terminal()), first);

		expect(second[0]).toBe(first[0]);
	});

	it("returns the previous array itself when no terminal changed", () => {
		const first = snapshotToRows(snapshot(terminal({ id: "a" }), terminal({ id: "b" })));
		const second = snapshotToRows(snapshot(terminal({ id: "a" }), terminal({ id: "b" })), first);

		expect(second).toBe(first);
	});

	it("replaces only the row whose fields changed", () => {
		const first = snapshotToRows(snapshot(terminal({ id: "a" }), terminal({ id: "b" })));
		const second = snapshotToRows(snapshot(terminal({ id: "a", name: "renamed" }), terminal({ id: "b" })), first);

		expect(second[0]).not.toBe(first[0]);
		expect(second[0].name).toBe("renamed");
		expect(second[1]).toBe(first[1]);
	});

	/** `status` is a derived object, so a naive `===` on it would never match and
	 *  every row would look changed. It has to be compared by value. */
	it("keeps identity across a re-derived status object", () => {
		const first = snapshotToRows(snapshot(terminal({ shellState: "busy" })));
		const second = snapshotToRows(snapshot(terminal({ shellState: "busy" })), first);

		expect(first[0].status.label).toBe("Working");
		expect(second[0]).toBe(first[0]);
	});

	it("replaces the row when the derived status changes", () => {
		const first = snapshotToRows(snapshot(terminal({ shellState: "idle" })));
		const second = snapshotToRows(snapshot(terminal({ shellState: "busy" })), first);

		expect(second[0]).not.toBe(first[0]);
		expect(second[0].status.label).toBe("Working");
	});

	it("does not reuse a row for a different terminal at the same index", () => {
		const first = snapshotToRows(snapshot(terminal({ id: "a" })));
		const second = snapshotToRows(snapshot(terminal({ id: "b" })), first);

		expect(second[0]).not.toBe(first[0]);
		expect(second[0].id).toBe("b");
	});

	it("returns a new array when the terminal set shrinks", () => {
		const first = snapshotToRows(snapshot(terminal({ id: "a" }), terminal({ id: "b" })));
		const second = snapshotToRows(snapshot(terminal({ id: "a" })), first);

		expect(second).not.toBe(first);
		expect(second).toHaveLength(1);
		expect(second[0]).toBe(first[0]);
	});
});

describe("snapshotToRows sub-agent tag", () => {
	it("carries the parent tag into the detached panel row", () => {
		// The detached window never reads the store, so a tag missing from the
		// snapshot would silently disappear from the popped-out dashboard only.
		const [row] = snapshotToRows(snapshot(terminal({ subAgentTag: "Progress Flow" })));
		expect(row.subAgentTag).toBe("Progress Flow");
	});

	it("rebuilds the row when only the tag changes (parent tab renamed)", () => {
		const first = snapshotToRows(snapshot(terminal({ subAgentTag: "old" })));
		const second = snapshotToRows(snapshot(terminal({ subAgentTag: "new" })), first);
		expect(second[0]).not.toBe(first[0]);
		expect(second[0].subAgentTag).toBe("new");
	});
});
