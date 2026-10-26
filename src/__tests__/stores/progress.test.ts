import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();
const toastAdd = vi.fn();
const nativeSend = vi.fn();

vi.mock("@tauri-apps/plugin-notification", () => ({
	isPermissionGranted: vi.fn().mockResolvedValue(true),
	requestPermission: vi.fn().mockResolvedValue("granted"),
	sendNotification: nativeSend,
}));

vi.mock("../../invoke", () => ({ invoke: invokeMock }));
vi.mock("../../stores/toasts", () => ({
	toastsStore: { add: toastAdd },
	setToastBellMirrorResolver: vi.fn(),
}));
vi.mock("../../stores/appLogger", () => ({ appLogger: { error: vi.fn() } }));
vi.mock("../../stores/repositories", () => ({
	repositoriesStore: {
		state: { activeRepoPath: "/repo" },
		getPaths: () => ["/repo", "/other", "/third"],
		get: (path: string) => (path === "/repo" ? { displayName: "Repo" } : undefined),
	},
}));
vi.mock("../../stores/terminals", () => ({
	terminalsStore: {
		state: { activeId: "tab-a", terminals: { "tab-a": { sessionId: "pty-a", repoPath: "/repo", name: "Agent A" } } },
		get: () => undefined,
	},
}));

type Kind = "done" | "blocked" | "intent" | "delegated" | "message";

function entry(
	id: number,
	createdAtMs: number,
	type: Kind = "done",
): {
	id: number;
	project: string;
	createdAtMs: number;
	type: Kind;
	text: string;
	step: string;
	ptyId?: string;
} {
	return { id, project: "/repo", createdAtMs, type, text: `entry ${id}`, step: "Delivery" };
}

function list(entries: ReturnType<typeof entry>[], lastViewedMs?: number) {
	return { project: "/repo", entries, lastViewedMs };
}

describe("progressStore", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		vi.resetModules();
		nativeSend.mockReset();
	});

	it("reads older journal pages only on demand", async () => {
		invokeMock
			.mockResolvedValueOnce({ ...list([entry(3, 300), entry(2, 200)]), ptyIds: ["pty-a"], total: 3, nextCursor: 2 })
			.mockResolvedValueOnce({ ...list([entry(1, 100)]), ptyIds: ["pty-a"], total: 3, nextCursor: null });
		const { createProgressStore } = await import("../../stores/progress");
		const store = createProgressStore();
		store.open("/repo", null);
		await vi.waitFor(() => expect(store.state.projects["/repo"]?.loading).toBe(false));
		expect(store.state.projects["/repo"].entries.map((item) => item.id)).toEqual([3, 2]);
		expect(invokeMock).toHaveBeenCalledTimes(1);
		await store.loadOlder();
		expect(store.state.projects["/repo"].entries.map((item) => item.id)).toEqual([3, 2, 1]);
		expect(invokeMock).toHaveBeenCalledWith("progress_list", {
			project: "/repo",
			input: { blockedOnly: false, limit: 100, cursor: 2 },
		});
	});

	// Catches: a deletion finishing in A advancing the epoch and stranding B's pending page.
	it("applies the selected project page after a previous project deletion completes", async () => {
		invokeMock.mockResolvedValue({ ...list([entry(1, 100)]), nextCursor: null });
		const { createProgressStore } = await import("../../stores/progress");
		const store = createProgressStore();
		store.open("/repo", null);
		await vi.waitFor(() => expect(store.state.projects["/repo"]?.loading).toBe(false));
		let finishDelete!: (value: unknown) => void;
		let finishPage!: (value: unknown) => void;
		invokeMock.mockImplementationOnce(
			() =>
				new Promise((resolve) => {
					finishDelete = resolve;
				}),
		);
		const deletion = store.deleteEntries("/repo", [1]);
		invokeMock.mockImplementationOnce(
			() =>
				new Promise((resolve) => {
					finishPage = resolve;
				}),
		);
		store.open("/other", null);
		finishDelete({ deleted: 1 });
		expect(await deletion).toBe(true);
		finishPage({ ...list([{ ...entry(2, 200), project: "/other" }]), project: "/other", nextCursor: null });
		await vi.waitFor(() => expect(store.state.projects["/other"].loading).toBe(false));
		expect(store.requestedProject()).toBe("/other");
		expect(store.state.projects["/other"].entries.map((item) => item.id)).toEqual([2]);
	});

	// Catches: a pending older page leaking into a changed filter or a reopened scope.
	it.each(["blocked", "pty", "project", "reopen"])(
		"discards an older page after a %s selection reset",
		async (change) => {
			invokeMock.mockResolvedValue({ ...list([entry(3, 300)]), nextCursor: 3 });
			const { createProgressStore } = await import("../../stores/progress");
			const store = createProgressStore();
			store.open("/repo", null);
			await vi.waitFor(() => expect(store.state.projects["/repo"]?.loading).toBe(false));
			let finish!: (value: unknown) => void;
			invokeMock.mockImplementationOnce(
				() =>
					new Promise((resolve) => {
						finish = resolve;
					}),
			);
			const older = store.loadOlder();
			invokeMock.mockResolvedValue({ ...list([entry(9, 900)]), nextCursor: null });
			if (change === "blocked") store.setBlockedOnly(true);
			if (change === "pty") store.selectPty("pty-b");
			if (change === "project") {
				store.open("/other", null);
				store.open("/repo", null);
			}
			if (change === "reopen") store.open("/repo", null);
			await vi.waitFor(() => expect(store.state.projects["/repo"].entries.map((item) => item.id)).toEqual([9]));
			finish({ ...list([entry(2, 200)]), nextCursor: null });
			await older;
			expect(store.state.projects["/repo"].entries.map((item) => item.id)).toEqual([9]);
		},
	);

	// Catches: a failed filter reset retaining the old filter's cursor and skipping its newest page.
	it("does not load an old cursor when the new filter's first page failed", async () => {
		invokeMock.mockResolvedValue({ ...list([entry(3, 300)]), nextCursor: 3 });
		const { createProgressStore } = await import("../../stores/progress");
		const store = createProgressStore();
		store.open("/repo", null);
		await vi.waitFor(() => expect(store.state.projects["/repo"]?.loading).toBe(false));
		invokeMock.mockRejectedValueOnce(new Error("Offline"));
		store.setBlockedOnly(true);
		await vi.waitFor(() => expect(store.state.projects["/repo"].error).toBe("Offline"));
		expect(store.state.projects["/repo"].nextCursor).toBeNull();
		expect(store.state.projects["/repo"].entries).toEqual([]);
		const calls = invokeMock.mock.calls.length;
		await store.loadOlder();
		expect(invokeMock).toHaveBeenCalledTimes(calls);
	});

	// Catches: failed older reads consuming the cursor or erasing already rendered entries.
	it("retries the same filtered cursor after an older-page failure", async () => {
		invokeMock.mockResolvedValue({ ...list([entry(3, 300, "blocked")]), nextCursor: 3 });
		const { createProgressStore } = await import("../../stores/progress");
		const store = createProgressStore();
		store.open("/repo", "pty-b");
		store.setBlockedOnly(true);
		await vi.waitFor(() => expect(store.state.projects["/repo"]?.loading).toBe(false));
		invokeMock.mockRejectedValueOnce(new Error("Offline"));
		await store.loadOlder();
		expect(store.state.projects["/repo"].entries.map((item) => item.id)).toEqual([3]);
		expect(store.state.projects["/repo"].error).toBe("Offline");
		invokeMock.mockResolvedValue({ ...list([entry(2, 200, "blocked")]), nextCursor: null });
		await store.loadOlder();
		expect(invokeMock).toHaveBeenLastCalledWith("progress_list", {
			project: "/repo",
			input: { blockedOnly: true, ptyId: "pty-b", limit: 100, cursor: 3 },
		});
		expect(store.state.projects["/repo"].entries.map((item) => item.id)).toEqual([3, 2]);
		expect(store.state.projects["/repo"].error).toBeNull();
		const calls = invokeMock.mock.calls.length;
		await store.loadOlder();
		expect(invokeMock).toHaveBeenCalledTimes(calls);
	});

	it("opens the active PTY and can switch to another PTY in the same repository", async () => {
		invokeMock.mockResolvedValue({ ...list([]), ptyIds: ["pty-a", "pty-b"] });
		const { createProgressStore } = await import("../../stores/progress");
		const store = createProgressStore();
		store.open();
		await vi.waitFor(() =>
			expect(invokeMock).toHaveBeenCalledWith("progress_list", {
				project: "/repo",
				input: { blockedOnly: false, ptyId: "pty-a", limit: 100 },
			}),
		);
		store.selectPty("pty-b");
		await vi.waitFor(() =>
			expect(invokeMock).toHaveBeenCalledWith("progress_list", {
				project: "/repo",
				input: { blockedOnly: false, ptyId: "pty-b", limit: 100 },
			}),
		);
		expect(store.selectedPtyId()).toBe("pty-b");
	});

	it("does not show the previous PTY's entries while another PTY loads", async () => {
		invokeMock.mockResolvedValueOnce({ ...list([{ ...entry(1, 100), ptyId: "pty-a" }]), ptyIds: ["pty-a", "pty-b"] });
		let finishLoad!: (value: ReturnType<typeof list>) => void;
		invokeMock.mockImplementationOnce(
			() =>
				new Promise((resolve) => {
					finishLoad = resolve;
				}),
		);
		const { createProgressStore } = await import("../../stores/progress");
		const store = createProgressStore();
		store.open("/repo");
		await vi.waitFor(() => expect(store.state.projects["/repo"].entries).toHaveLength(1));
		store.selectPty("pty-b");
		expect(store.state.projects["/repo"].entries).toEqual([]);
		finishLoad(list([]));
		await vi.waitFor(() => expect(store.state.projects["/repo"].loading).toBe(false));
	});

	it("marks every viewed scope when closing after switching PTYs", async () => {
		invokeMock.mockResolvedValue(list([]));
		const { createProgressStore } = await import("../../stores/progress");
		const store = createProgressStore();
		store.open("/repo");
		store.selectPty("pty-b");
		store.selectPty(null);
		await store.close();
		expect(invokeMock).toHaveBeenCalledWith("progress_mark_viewed", { project: "/repo", ptyId: "pty-a" });
		expect(invokeMock).toHaveBeenCalledWith("progress_mark_viewed", { project: "/repo", ptyId: "pty-b" });
		expect(invokeMock).toHaveBeenCalledWith("progress_mark_viewed", { project: "/repo" });
	});

	it("uses each PTY's own divider when switching scopes", async () => {
		invokeMock.mockImplementation((_command: string, args: { input?: { ptyId?: string } }) =>
			Promise.resolve(list([], args.input?.ptyId === "pty-b" ? 50 : 150)),
		);
		const { createProgressStore } = await import("../../stores/progress");
		const store = createProgressStore();
		store.open("/repo");
		await vi.waitFor(() => expect(store.state.projects["/repo"].dividerMs).toBe(150));
		store.selectPty("pty-b");
		await vi.waitFor(() => expect(store.state.projects["/repo"].dividerMs).toBe(50));
	});

	/// The list is redrawn whenever an entry arrives. If the divider followed the
	/// stored timestamp it would jump to the top under the reader's cursor and
	/// the "what is new" question would become unanswerable mid-read.
	it("freezes the divider while the dialog is open and moves it on close", async () => {
		invokeMock.mockImplementation((command: string) =>
			command === "progress_list" ? Promise.resolve(list([entry(2, 200), entry(1, 100)], 150)) : Promise.resolve({}),
		);
		const { createProgressStore } = await import("../../stores/progress");
		const store = createProgressStore();

		store.open("/repo");
		await vi.waitFor(() => expect(store.state.projects["/repo"].entries).toHaveLength(2));
		expect(store.state.projects["/repo"].dividerMs).toBe(150);

		// A later entry arrives and the backend has already moved its own mark —
		// the frozen one must not follow.
		invokeMock.mockImplementation((command: string) =>
			command === "progress_list"
				? Promise.resolve(list([entry(3, 300), entry(2, 200), entry(1, 100)], 300))
				: Promise.resolve({}),
		);
		await store.refreshProject("/repo");
		expect(store.state.projects["/repo"].dividerMs).toBe(150);

		await store.close();
		expect(invokeMock).toHaveBeenCalledWith("progress_mark_viewed", { project: "/repo", ptyId: "pty-a" });
		expect(store.state.projects["/repo"].dividerMs).toBeUndefined();
	});

	/// The old panel fanned out over every registered repository on open. The
	/// dialog shows one project, so it asks one question.
	it("queries once for the project it shows, never once per registered repository", async () => {
		invokeMock.mockResolvedValue(list([entry(1, 100)]));
		const { createProgressStore } = await import("../../stores/progress");
		const store = createProgressStore();

		store.open();
		await vi.waitFor(() => expect(store.state.projects["/repo"].entries).toHaveLength(1));
		expect(invokeMock).toHaveBeenCalledTimes(1);
		expect(invokeMock).toHaveBeenCalledWith("progress_list", {
			project: "/repo",
			input: { blockedOnly: false, ptyId: "pty-a", limit: 100 },
		});
	});

	it("carries the blocked-only filter into the query", async () => {
		invokeMock.mockResolvedValue(list([]));
		const { createProgressStore } = await import("../../stores/progress");
		const store = createProgressStore();

		store.open("/repo");
		await vi.waitFor(() => expect(invokeMock).toHaveBeenCalled());
		invokeMock.mockClear();
		store.setBlockedOnly(true);
		await vi.waitFor(() =>
			expect(invokeMock).toHaveBeenCalledWith("progress_list", {
				project: "/repo",
				input: { blockedOnly: true, ptyId: "pty-a", limit: 100 },
			}),
		);
	});

	it("toasts a reported entry silently and stays quiet for a host-written intent", async () => {
		invokeMock.mockResolvedValue(list([]));
		const { createProgressStore } = await import("../../stores/progress");
		const store = createProgressStore();

		store.presentLive({ repo_path: "/repo", payload: { entry: entry(1, 100, "blocked") } });
		expect(toastAdd).toHaveBeenCalledTimes(1);
		// `sound` is the 4th argument and `mirrorToMessages` the 9th: a progress
		// entry is never an interruption and never leaves the app.
		expect(toastAdd.mock.calls[0][3]).toBe(false);
		expect(toastAdd.mock.calls[0][8]).toBe(false);
		// The title is the step alone. The repo goes in as `repoPath` (7th
		// argument) and the toast draws it as its own badge, so a title of
		// "<project> · <step>" would print the project twice and leave the step
		// squashed into what the badge did not take.
		expect(toastAdd.mock.calls[0][0]).toBe("Delivery");
		expect(toastAdd.mock.calls[0][6]).toBe("/repo");

		store.presentLive({ repo_path: "/repo", payload: { entry: entry(2, 200, "intent") } });
		expect(toastAdd).toHaveBeenCalledTimes(1);
		// A hand-off is one agent talking to another, not an outcome.
		store.presentLive({ repo_path: "/repo", payload: { entry: entry(3, 300, "delegated") } });
		store.presentLive({ repo_path: "/repo", payload: { entry: entry(4, 400, "message") } });
		expect(toastAdd).toHaveBeenCalledTimes(1);
	});

	it("reads the Flow for the scope on screen and follows a PTY switch", async () => {
		const flow = { project: "/repo", participants: [], events: [], truncated: false };
		invokeMock.mockImplementation((command: string) => Promise.resolve(command === "progress_flow" ? flow : list([])));
		const { createProgressStore } = await import("../../stores/progress");
		const store = createProgressStore();
		store.open();
		store.setView("flow");
		await vi.waitFor(() =>
			expect(invokeMock).toHaveBeenCalledWith("progress_flow", { project: "/repo", input: { ptyId: "pty-a" } }),
		);
		await vi.waitFor(() => expect(store.state.flows["/repo"]?.data).toEqual(flow));

		store.selectPty(null);
		await vi.waitFor(() => expect(invokeMock).toHaveBeenCalledWith("progress_flow", { project: "/repo", input: {} }));
	});

	// A Flow scoped to a terminal also draws its parent and children. A child's
	// `done` or the parent's message is on screen, so it must redraw the Flow
	// and must not be counted as unread.
	it("refreshes a scoped Flow for an entry from one of its participants", async () => {
		const flow = {
			project: "/repo",
			participants: [
				{ id: "pty-a", kind: "terminal", title: "A", state: "busy", toolCalls: 0, ptyId: "pty-a" },
				{ id: "pty-child", kind: "terminal", title: "C", state: "idle", toolCalls: 0, ptyId: "pty-child" },
			],
			events: [],
			truncated: false,
		};
		invokeMock.mockImplementation((command: string) => Promise.resolve(command === "progress_flow" ? flow : list([])));
		const { createProgressStore } = await import("../../stores/progress");
		const store = createProgressStore();
		store.open();
		store.setView("flow");
		await vi.waitFor(() => expect(store.state.flows["/repo"]?.data).toEqual(flow));
		const flowReads = () => invokeMock.mock.calls.filter(([command]) => command === "progress_flow").length;
		const before = flowReads();

		store.presentLive({ repo_path: "/repo", payload: { entry: { ...entry(5, 500), ptyId: "pty-child" } } });
		expect(flowReads()).toBe(before + 1);
		expect(store.unreadCount).toBe(0);

		// A message from outside the Flow, addressed to a participant.
		store.presentLive({
			repo_path: "/repo",
			payload: { entry: { ...entry(6, 600, "message"), ptyId: "pty-far", targetPtyId: "pty-a" } },
		});
		expect(flowReads()).toBe(before + 2);

		// A terminal the Flow does not draw is still unread.
		store.presentLive({ repo_path: "/repo", payload: { entry: { ...entry(7, 700), ptyId: "pty-far" } } });
		expect(flowReads()).toBe(before + 2);
		expect(store.unreadCount).toBe(1);
	});

	it("does not read the Flow while the List is showing", async () => {
		invokeMock.mockResolvedValue(list([]));
		const { createProgressStore } = await import("../../stores/progress");
		const store = createProgressStore();
		store.open();
		store.selectPty("pty-b");
		await vi.waitFor(() => expect(invokeMock).toHaveBeenCalledTimes(2));
		expect(invokeMock.mock.calls.some(([command]) => command === "progress_flow")).toBe(false);
	});

	it("counts outcomes that arrived while the dialog was closed and clears it on open", async () => {
		invokeMock.mockResolvedValue(list([]));
		const { createProgressStore } = await import("../../stores/progress");
		const store = createProgressStore();

		store.presentLive({ repo_path: "/repo", payload: { entry: entry(1, 100) } });
		store.presentLive({ repo_path: "/repo", payload: { entry: entry(2, 150, "blocked") } });
		// Intents and agent-to-agent messages are journal lines, not outcomes: ten busy
		// agents emit hundreds a day, which made the bell read 156.
		store.presentLive({ repo_path: "/repo", payload: { entry: entry(3, 200, "intent") } });
		store.presentLive({ repo_path: "/repo", payload: { entry: entry(4, 250, "message") } });
		expect(store.unreadCount).toBe(2);

		store.open("/repo");
		expect(store.unreadCount).toBe(0);

		// Open on the same project, the list refreshes instead of counting.
		store.presentLive({ repo_path: "/repo", payload: { entry: { ...entry(3, 300), ptyId: "pty-a" } } });
		expect(store.unreadCount).toBe(0);
	});

	it("notifies natively for unfocused done and blocked outcomes but not intent or hand-off", async () => {
		vi.stubGlobal("__TAURI_INTERNALS__", {});
		const focus = vi.spyOn(document, "hasFocus").mockReturnValue(false);
		try {
			const { createProgressStore } = await import("../../stores/progress");
			const store = createProgressStore();
			store.presentLive({ repo_path: "/repo", payload: { entry: entry(31, 100, "done") } });
			store.presentLive({ repo_path: "/repo", payload: { entry: entry(32, 200, "blocked") } });
			store.presentLive({ repo_path: "/repo", payload: { entry: entry(33, 300, "intent") } });
			store.presentLive({ repo_path: "/repo", payload: { entry: entry(34, 400, "delegated") } });
			await vi.waitFor(() => expect(nativeSend).toHaveBeenCalledTimes(2));
			expect(nativeSend).toHaveBeenCalledWith(
				expect.objectContaining({
					title: expect.stringContaining("Repo"),
					body: "entry 31",
				}),
			);
			expect(nativeSend).toHaveBeenCalledWith(
				expect.objectContaining({
					title: expect.stringContaining("Repo"),
					body: "entry 32",
				}),
			);
		} finally {
			focus.mockRestore();
			vi.unstubAllGlobals();
		}
	});

	it("keeps a failed command visible as one line", async () => {
		invokeMock.mockRejectedValue(new Error("database is locked"));
		const { createProgressStore } = await import("../../stores/progress");
		const store = createProgressStore();

		expect(await store.deleteEntries("/repo", [1])).toBe(false);
		expect(store.state.projects["/repo"].error).toBe("database is locked");
	});

	describe("sidebar flow", () => {
		const flow = { project: "/repo", participants: [], events: [], truncated: false };

		// Catches: every branch row of one repo issuing its own progress_flow call.
		it("collapses asks closer than the minimum gap into one backend read", async () => {
			invokeMock.mockResolvedValue(flow);
			const { createProgressStore } = await import("../../stores/progress");
			const store = createProgressStore();
			await Promise.all([store.refreshSidebarFlow("/repo"), store.refreshSidebarFlow("/repo")]);
			expect(invokeMock).toHaveBeenCalledTimes(1);
			expect(invokeMock).toHaveBeenCalledWith("progress_flow", { project: "/repo", input: {} });
			expect(store.sidebarFlow("/repo")).toEqual(flow);
			store.resetForTests();
		});

		// Catches: a trailing refresh firing after the last row showing the flow has left.
		it("cancels the trailing refresh when the last holder releases, not before", async () => {
			vi.useFakeTimers();
			try {
				invokeMock.mockResolvedValue(flow);
				const { createProgressStore } = await import("../../stores/progress");
				const store = createProgressStore();
				const releaseA = store.holdSidebarFlow("/repo");
				const releaseB = store.holdSidebarFlow("/repo");
				await store.refreshSidebarFlow("/repo");
				await store.refreshSidebarFlow("/repo"); // inside the gap: schedules the trailing read
				releaseA();
				await vi.advanceTimersByTimeAsync(6000);
				expect(invokeMock).toHaveBeenCalledTimes(2); // still held: trailing read ran
				await vi.advanceTimersByTimeAsync(6000);
				await store.refreshSidebarFlow("/repo"); // read 3
				await store.refreshSidebarFlow("/repo"); // inside the gap: schedules the trailing read
				releaseB();
				await vi.advanceTimersByTimeAsync(6000);
				expect(invokeMock).toHaveBeenCalledTimes(3); // released: trailing read cancelled
			} finally {
				vi.useRealTimers();
			}
		});

		// Catches: a failed newer read discarding an older response that is still valid.
		it("applies an older response when the newer request failed", async () => {
			vi.useFakeTimers();
			try {
				let resolveOld: (f: typeof flow) => void = () => {};
				invokeMock
					.mockImplementationOnce(
						() =>
							new Promise((r) => {
								resolveOld = r;
							}),
					)
					.mockRejectedValueOnce(new Error("boom"));
				const { createProgressStore } = await import("../../stores/progress");
				const store = createProgressStore();
				const first = store.refreshSidebarFlow("/repo");
				vi.advanceTimersByTime(6000);
				await store.refreshSidebarFlow("/repo");
				resolveOld(flow);
				await first;
				expect(store.sidebarFlow("/repo")).toEqual(flow);
			} finally {
				vi.useRealTimers();
			}
		});

		// Catches: a failed read blanking the subagent lines that were already on screen.
		it("keeps the last flow when a read fails", async () => {
			vi.useFakeTimers();
			try {
				invokeMock.mockResolvedValueOnce(flow).mockRejectedValueOnce(new Error("boom"));
				const { createProgressStore } = await import("../../stores/progress");
				const store = createProgressStore();
				await store.refreshSidebarFlow("/repo");
				vi.advanceTimersByTime(6000);
				await store.refreshSidebarFlow("/repo");
				expect(store.sidebarFlow("/repo")).toEqual(flow);
			} finally {
				vi.useRealTimers();
			}
		});
	});
});
