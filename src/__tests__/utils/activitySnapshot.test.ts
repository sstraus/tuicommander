import { beforeEach, describe, expect, it, vi } from "vitest";
import type {
	terminalStatusLabel as LabelFn,
	reconcileActivityOrder as ReconcileFn,
} from "../../utils/activitySnapshot";

const mockInvoke = vi.fn().mockResolvedValue(undefined);

vi.mock("@tauri-apps/api/core", () => ({
	invoke: mockInvoke,
}));

describe("activitySnapshot", () => {
	let buildActivitySnapshot: typeof import("../../utils/activitySnapshot").buildActivitySnapshot;
	let terminalsStore: typeof import("../../stores/terminals").terminalsStore;
	let globalWorkspaceStore: typeof import("../../stores/globalWorkspace").globalWorkspaceStore;

	beforeEach(async () => {
		vi.resetModules();
		mockInvoke.mockReset().mockResolvedValue(undefined);
		vi.doMock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));

		const termMod = await import("../../stores/terminals");
		terminalsStore = termMod.terminalsStore;
		const gwMod = await import("../../stores/globalWorkspace");
		globalWorkspaceStore = gwMod.globalWorkspaceStore;
		const snapMod = await import("../../utils/activitySnapshot");
		buildActivitySnapshot = snapMod.buildActivitySnapshot;
	});

	it("returns empty terminals array when none exist", () => {
		const snap = buildActivitySnapshot();
		expect(snap.terminals).toEqual([]);
	});

	it("includes all terminal fields in snapshot", () => {
		const id = terminalsStore.add({
			name: "Terminal 1",
			sessionId: "sess1",
			cwd: "/Users/test/project",
			fontSize: 14,
			awaitingInput: null,
			agentType: "claude",
		});
		terminalsStore.update(id, {
			shellState: "busy",
			agentIntent: "Writing tests",
			lastPrompt: "Write tests for panelSync",
		});

		const snap = buildActivitySnapshot();
		expect(snap.terminals).toHaveLength(1);

		const t = snap.terminals[0];
		expect(t.id).toBe(id);
		expect(t.name).toBe("Terminal 1");
		expect(t.shellState).toBe("busy");
		expect(t.awaitingInput).toBeNull();
		expect(t.sessionId).toBe("sess1");
		expect(t.agentType).toBe("claude");
		expect(t.agentIntent).toBe("Writing tests");
		expect(t.currentTask).toBeNull(); // claude agentType suppresses currentTask
		expect(t.lastPrompt).toBe("Write tests for panelSync");
		expect(t.cwd).toBe("/Users/test/project");
		expect(typeof t.isActive).toBe("boolean");
		expect(typeof t.isRateLimited).toBe("boolean");
		expect(typeof t.isPromoted).toBe("boolean");
	});

	it("shows currentTask for non-claude agents", () => {
		const id = terminalsStore.add({
			name: "Terminal 2",
			sessionId: "sess2",
			cwd: null,
			fontSize: 14,
			awaitingInput: null,
			agentType: "aider",
		});
		terminalsStore.update(id, { currentTask: "Running migration" });

		const snap = buildActivitySnapshot();
		expect(snap.terminals[0].currentTask).toBe("Running migration");
	});

	it("reflects isPromoted from globalWorkspaceStore", () => {
		const id = terminalsStore.add({
			name: "Terminal 3",
			sessionId: null,
			cwd: null,
			fontSize: 14,
			awaitingInput: null,
		});
		globalWorkspaceStore.togglePromote(id);

		const snap = buildActivitySnapshot();
		expect(snap.terminals[0].isPromoted).toBe(true);
	});
});

describe("terminalStatusLabel", () => {
	let terminalStatusLabel: typeof LabelFn;
	beforeEach(async () => {
		vi.resetModules();
		vi.doMock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));
		terminalStatusLabel = (await import("../../utils/activitySnapshot")).terminalStatusLabel;
	});

	const cls = { rateLimited: "RL", error: "ERR", waiting: "WAIT", working: "WORK", idle: "IDLE" };

	it("rate-limited wins over everything", () => {
		expect(terminalStatusLabel("busy", "error", true, cls)).toEqual({ label: "Rate limited", className: "RL" });
	});

	it("labels an API error as Error, NOT Waiting for input", () => {
		// Regression: an errored agent must not be collapsed into "Waiting for input".
		expect(terminalStatusLabel("idle", "error", false, cls)).toEqual({ label: "Error", className: "ERR" });
	});

	it("labels a question as Waiting for input", () => {
		expect(terminalStatusLabel("idle", "question", false, cls)).toEqual({
			label: "Waiting for input",
			className: "WAIT",
		});
	});

	it("maps shellState busy/idle when no awaiting input", () => {
		expect(terminalStatusLabel("busy", null, false, cls)).toEqual({ label: "Working", className: "WORK" });
		expect(terminalStatusLabel("idle", null, false, cls)).toEqual({ label: "Idle", className: "IDLE" });
		expect(terminalStatusLabel(null, null, false, cls)).toEqual({ label: "—", className: "IDLE" });
	});
});

describe("reconcileActivityOrder", () => {
	let reconcileActivityOrder: typeof ReconcileFn;
	beforeEach(async () => {
		vi.resetModules();
		vi.doMock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));
		reconcileActivityOrder = (await import("../../utils/activitySnapshot")).reconcileActivityOrder;
	});

	const working = (set: Set<string>) => (id: string) => set.has(id);

	it("partitions working-first, idle-second, each in first-seen order", () => {
		const spine: string[] = [];
		const order = reconcileActivityOrder(spine, ["a", "b", "c", "d"], working(new Set(["b", "d"])));
		expect(order).toEqual(["b", "d", "a", "c"]);
	});

	it("keeps a terminal in place while its working state is unchanged", () => {
		const spine: string[] = [];
		const w = new Set(["a", "b"]);
		const first = reconcileActivityOrder(spine, ["a", "b", "c"], working(w));
		// Recompute with the SAME states — order must be identical (no avanti-e-indietro).
		const second = reconcileActivityOrder(spine, ["a", "b", "c"], working(w));
		expect(second).toEqual(first);
	});

	it("moves a terminal only when it crosses the working/idle boundary", () => {
		const spine: string[] = [];
		reconcileActivityOrder(spine, ["a", "b", "c"], working(new Set(["a"])));
		// b flips to working — it joins the working group at its spine position.
		const after = reconcileActivityOrder(spine, ["a", "b", "c"], working(new Set(["a", "b"])));
		expect(after).toEqual(["a", "b", "c"]);
	});

	it("appends newly-seen terminals at the end of their group", () => {
		const spine: string[] = [];
		reconcileActivityOrder(spine, ["a", "b"], working(new Set(["a"])));
		const after = reconcileActivityOrder(spine, ["a", "b", "c"], working(new Set(["a", "c"])));
		// c is new + working → after existing working 'a'; idle 'b' stays last.
		expect(after).toEqual(["a", "c", "b"]);
	});

	it("drops removed terminals while preserving relative order", () => {
		const spine: string[] = [];
		reconcileActivityOrder(spine, ["a", "b", "c"], working(new Set()));
		const after = reconcileActivityOrder(spine, ["a", "c"], working(new Set()));
		expect(after).toEqual(["a", "c"]);
		expect(spine).toEqual(["a", "c"]);
	});
});
