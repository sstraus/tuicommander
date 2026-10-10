import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { testInScope, testInScopeAsync } from "../helpers/store";

/**
 * The `repositories-changed` broadcast: one backend serves the desktop WebView,
 * the browser and the PWA, so a save by any of them must reach the others.
 *
 * The property under test is not "the store updates" but "the store and the
 * compare-and-swap baseline move together". Refreshing only one of the two makes
 * the *next* local save diff against a document the client never held, and that
 * diff reverts whatever the other client just wrote — which is why every test
 * here also inspects the mutation the following save emits.
 */

const mockInvoke = vi.fn().mockResolvedValue(undefined);
const listeners = new Map<string, (event: { payload: unknown }) => void>();
const mockListen = vi.fn((event: string, handler: (e: { payload: unknown }) => void) => {
	listeners.set(event, handler);
	return Promise.resolve(() => {
		listeners.delete(event);
	});
});

vi.mock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: mockListen, emit: vi.fn() }));

type DiskRepo = Record<string, unknown>;

function repoRecord(path: string, displayName: string, branches: Record<string, unknown> = {}): DiskRepo {
	return {
		path,
		displayName,
		initials: "",
		isGitRepo: true,
		expanded: true,
		collapsed: false,
		parked: false,
		branches,
		activeBranch: null,
	};
}

function groupRecord(id: string, name: string, repoOrder: string[] = []): Record<string, unknown> {
	return { id, name, color: "", collapsed: false, repoOrder };
}

function branchRecord(name: string, overrides: Record<string, unknown> = {}): Record<string, unknown> {
	return {
		name,
		isMain: true,
		worktreePath: null,
		terminals: [],
		hadTerminals: false,
		lastActiveTerminal: null,
		additions: 0,
		deletions: 0,
		isMerged: false,
		lastCommitTs: null,
		...overrides,
	};
}

describe("repositoriesStore remote sync", () => {
	let store: typeof import("../../stores/repositories").repositoriesStore;

	/** The document `load_repositories` returns. Reassigned to stand in for a write
	 *  another client made between the last save and the broadcast. */
	let disk: {
		repos: Record<string, DiskRepo>;
		repoOrder: string[];
		activeRepoPath: string | null;
		groups: Record<string, unknown>;
		groupOrder: string[];
	};

	function setDisk(next: Partial<typeof disk>): void {
		disk = { repos: {}, repoOrder: [], activeRepoPath: null, groups: {}, groupOrder: [], ...next };
	}

	/** Deliver the broadcast and let the re-read settle. */
	async function broadcast(): Promise<void> {
		const handler = listeners.get("repositories-changed");
		expect(handler, "hydrate must subscribe to repositories-changed").toBeDefined();
		handler!({ payload: {} });
		await vi.advanceTimersByTimeAsync(0);
	}

	/** The keyed delta of the most recent save, or null when nothing was saved. */
	function lastSavedMutation(): {
		repos: Array<{ id: string; before: unknown; after: unknown }>;
		groups: Array<{ id: string; before: unknown; after: unknown }>;
		repoOrder?: { before: string[]; after: string[] };
		activeRepoPath?: { before: string | null; after: string | null };
	} | null {
		const calls = mockInvoke.mock.calls.filter((call: unknown[]) => call[0] === "save_repositories");
		if (calls.length === 0) return null;
		return (calls.at(-1)![1] as { config: ReturnType<typeof lastSavedMutation> }).config;
	}

	beforeEach(async () => {
		vi.resetModules();
		vi.useFakeTimers();
		listeners.clear();
		mockListen.mockClear();
		localStorage.clear();
		setDisk({});
		mockInvoke.mockReset().mockImplementation((command: string) => {
			if (command === "load_repositories") return Promise.resolve(structuredClone(disk));
			return Promise.resolve(undefined);
		});

		vi.doMock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));
		vi.doMock("@tauri-apps/api/event", () => ({ listen: mockListen, emit: vi.fn() }));

		store = (await import("../../stores/repositories")).repositoriesStore;
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	it("adopts a repository another client added", async () => {
		await testInScopeAsync(async () => {
			await store.hydrate();

			setDisk({ repos: { "/other": repoRecord("/other", "Other") }, repoOrder: ["/other"] });
			await broadcast();

			expect(store.get("/other")?.displayName).toBe("Other");
			expect(store.state.repoOrder).toEqual(["/other"]);
		});
	});

	it("does not revert the adopted repository on the next local save", async () => {
		await testInScopeAsync(async () => {
			await store.hydrate();

			setDisk({ repos: { "/other": repoRecord("/other", "Other") }, repoOrder: ["/other"] });
			await broadcast();

			store.add({ path: "/mine", displayName: "Mine" });
			await vi.advanceTimersByTimeAsync(500);

			const mutation = lastSavedMutation();
			expect(mutation).not.toBeNull();
			// The baseline moved with the store, so `/other` produces no delta at all.
			// Had only one of the two moved, this save would carry an `/other` mutation
			// whose `after` is stale — silently undoing the other client's write.
			expect(mutation!.repos.map((entry) => entry.id)).toEqual(["/mine"]);
		});
	});

	it("keeps a local edit that has not been saved yet", async () => {
		setDisk({ repos: { "/repo": repoRecord("/repo", "Original") }, repoOrder: ["/repo"] });

		await testInScopeAsync(async () => {
			await store.hydrate();

			store.setDisplayName("/repo", "Mine");
			store._testCancelPendingSave();

			setDisk({ repos: { "/repo": repoRecord("/repo", "Theirs") }, repoOrder: ["/repo"] });
			await broadcast();

			expect(store.get("/repo")?.displayName).toBe("Mine");

			// Untouched baseline: this client still offers the document it last saw, so
			// the backend's compare-and-swap rejects it and the rebase decides the winner.
			store.setDisplayName("/repo", "Mine");
			await vi.advanceTimersByTimeAsync(500);
			const repoMutation = lastSavedMutation()?.repos.find((entry) => entry.id === "/repo");
			expect(repoMutation?.before).toEqual(expect.objectContaining({ displayName: "Original" }));
			expect(repoMutation?.after).toEqual(expect.objectContaining({ displayName: "Mine" }));
		});
	});

	it("never moves the repo this window is looking at", async () => {
		setDisk({
			repos: { "/a": repoRecord("/a", "A"), "/b": repoRecord("/b", "B") },
			repoOrder: ["/a", "/b"],
			activeRepoPath: "/a",
		});

		await testInScopeAsync(async () => {
			await store.hydrate();
			expect(store.state.activeRepoPath).toBe("/a");

			// Another client (a phone, say) switched repo. Focus is per-window.
			setDisk({
				repos: { "/a": repoRecord("/a", "A"), "/b": repoRecord("/b", "B") },
				repoOrder: ["/a", "/b"],
				activeRepoPath: "/b",
			});
			await broadcast();

			expect(store.state.activeRepoPath).toBe("/a");
		});
	});

	it("keeps live terminals when it adopts the record holding them", async () => {
		setDisk({
			repos: { "/repo": repoRecord("/repo", "Original", { main: branchRecord("main") }) },
			repoOrder: ["/repo"],
		});

		await testInScopeAsync(async () => {
			await store.hydrate();
			store.addTerminalToWorkspace("/repo", "main", "term-1");
			await vi.advanceTimersByTimeAsync(500);

			// Tab placement is memory-only: disk never carries it, so an adoption that
			// copies the record verbatim would close the pane the user is looking at.
			setDisk({
				repos: {
					"/repo": repoRecord("/repo", "Renamed", { main: branchRecord("main", { hadTerminals: true }) }),
				},
				repoOrder: ["/repo"],
			});
			await broadcast();

			expect(store.get("/repo")?.displayName).toBe("Renamed");
			expect(store.get("/repo")?.workspaces["main"].terminals).toEqual(["term-1"]);
		});
	});

	it("refuses a removal that would orphan live terminals", async () => {
		setDisk({
			repos: { "/repo": repoRecord("/repo", "Original", { main: branchRecord("main") }) },
			repoOrder: ["/repo"],
		});

		await testInScopeAsync(async () => {
			await store.hydrate();
			store.addTerminalToWorkspace("/repo", "main", "term-1");
			await vi.advanceTimersByTimeAsync(500);

			setDisk({ repos: {}, repoOrder: [] });
			await broadcast();

			expect(store.get("/repo")).toBeDefined();
			expect(store.get("/repo")?.workspaces["main"].terminals).toEqual(["term-1"]);
			// The record survives, and so must its place in the order: the sidebar renders
			// from `repoOrder`, so keeping the record alone leaves the pane running behind
			// a row that no longer exists.
			expect(store.state.repoOrder).toEqual(["/repo"]);
		});
	});

	it("keeps a branch another client deleted while a terminal is open in it", async () => {
		setDisk({
			repos: {
				"/repo": repoRecord("/repo", "Repo", {
					main: branchRecord("main"),
					feature: branchRecord("feature", { isMain: false }),
				}),
			},
			repoOrder: ["/repo"],
		});

		await testInScopeAsync(async () => {
			await store.hydrate();
			store.addTerminalToWorkspace("/repo", "feature", "term-1");
			await vi.advanceTimersByTimeAsync(500);

			// Another client archived the worktree. The repo record stays, so the
			// repo-level guard never runs — only the branch-level one can save the pane.
			setDisk({
				repos: { "/repo": repoRecord("/repo", "Repo", { main: branchRecord("main") }) },
				repoOrder: ["/repo"],
			});
			await broadcast();

			expect(store.get("/repo")?.workspaces["feature"]?.terminals).toEqual(["term-1"]);
			expect(store.getRepoPathForTerminal("term-1")).toBe("/repo");
		});
	});

	it("adopts a rename while this window's diffstat is ahead of its baseline", async () => {
		setDisk({
			repos: { "/repo": repoRecord("/repo", "Original", { main: branchRecord("main") }) },
			repoOrder: ["/repo"],
		});

		await testInScopeAsync(async () => {
			await store.hydrate();

			// `updateWorkspaceStats` never saves, so a repo under active work drifts from its
			// own baseline every few seconds. Reading that as an edit would refuse every
			// remote change for exactly the repos the user is working in.
			store.updateWorkspaceStats("/repo", "main", 42, 7);

			setDisk({
				repos: { "/repo": repoRecord("/repo", "Renamed", { main: branchRecord("main") }) },
				repoOrder: ["/repo"],
			});
			await broadcast();

			expect(store.get("/repo")?.displayName).toBe("Renamed");
			// This window's own count is the fresher one; disk's zero must not win.
			expect(store.get("/repo")?.workspaces["main"].additions).toBe(42);
		});
	});

	it("normalizes a record an older client wrote", async () => {
		await testInScopeAsync(async () => {
			await store.hydrate();

			// A build that dropped an agent leaves its name on disk; every index into
			// `AGENT_DISPLAY` is exhaustive, so a stale name throws inside a render.
			// Legacy records also predate collapsed/parked/isMerged.
			setDisk({
				repos: {
					"/legacy": {
						path: "/legacy",
						displayName: "Legacy",
						initials: "",
						isGitRepo: true,
						branches: {
							main: {
								name: "main",
								isMain: true,
								worktreePath: null,
								terminals: [],
								hadTerminals: false,
								lastActiveTerminal: null,
								additions: 0,
								deletions: 0,
								lastCommitTs: null,
								savedTerminals: [{ id: "t1", agentType: "fx" }],
							},
						},
					},
				},
				repoOrder: ["/legacy"],
			});
			await broadcast();

			const adopted = store.get("/legacy");
			expect(adopted?.collapsed).toBe(false);
			expect(adopted?.expanded).toBe(true);
			expect(adopted?.parked).toBe(false);
			expect(adopted?.workspaces["main"].isMerged).toBe(false);
			expect(adopted?.workspaces["main"].savedTerminals?.[0]?.agentType).toBeNull();
		});
	});

	it("adopts a group another client created, and the order that places it", async () => {
		setDisk({ repos: { "/repo": repoRecord("/repo", "Repo") }, repoOrder: ["/repo"] });

		await testInScopeAsync(async () => {
			await store.hydrate();

			setDisk({
				repos: { "/repo": repoRecord("/repo", "Repo") },
				repoOrder: ["/repo"],
				groups: { g1: groupRecord("g1", "Work", ["/repo"]) },
				groupOrder: ["g1"],
			});
			await broadcast();

			expect(store.state.groups["g1"]?.name).toBe("Work");
			expect(store.state.groupOrder).toEqual(["g1"]);
			// Grouping is an overlay on `repoOrder`, not a move out of it —
			// `getGroupedLayout` filters the grouped paths out at render time.
			expect(store.getGroupedLayout().ungrouped).toEqual([]);
		});
	});

	it("drops a group deleted elsewhere from the order too", async () => {
		setDisk({
			repos: { "/repo": repoRecord("/repo", "Repo") },
			repoOrder: ["/repo"],
			groups: { g1: groupRecord("g1", "Work", ["/repo"]) },
			groupOrder: ["g1"],
		});

		await testInScopeAsync(async () => {
			await store.hydrate();

			setDisk({ repos: { "/repo": repoRecord("/repo", "Repo") }, repoOrder: ["/repo"] });
			await broadcast();

			expect(store.state.groups["g1"]).toBeUndefined();
			// A `groupOrder` entry with no group behind it renders an empty accordion.
			expect(store.state.groupOrder).toEqual([]);
			expect(store.getGroupedLayout().ungrouped.map((repo) => repo.path)).toEqual(["/repo"]);
		});
	});

	it("leaves a group this window edited but has not saved", async () => {
		setDisk({
			repos: {},
			repoOrder: [],
			groups: { g1: groupRecord("g1", "Work") },
			groupOrder: ["g1"],
		});

		await testInScopeAsync(async () => {
			await store.hydrate();

			store.renameGroup("g1", "Mine");
			store._testCancelPendingSave();

			setDisk({
				repos: {},
				repoOrder: [],
				groups: { g1: groupRecord("g1", "Theirs") },
				groupOrder: ["g1"],
			});
			await broadcast();

			expect(store.state.groups["g1"]?.name).toBe("Mine");
		});
	});

	it("waits for an in-flight save before adopting", async () => {
		let releaseSave: (() => void) | null = null;
		mockInvoke.mockImplementation((command: string) => {
			if (command === "load_repositories") return Promise.resolve(structuredClone(disk));
			// Only the first save is held open; later ones resolve normally.
			if (command === "save_repositories" && !releaseSave) {
				return new Promise<void>((resolve) => {
					releaseSave = () => resolve();
				});
			}
			return Promise.resolve(undefined);
		});

		await testInScopeAsync(async () => {
			await store.hydrate();

			store.add({ path: "/mine", displayName: "Mine" });
			await vi.advanceTimersByTimeAsync(500);
			expect(releaseSave, "the save must still be in flight").not.toBeNull();

			// A save ends by assigning the baseline it computed before it was sent.
			// Adopting now would have that assignment overwrite the adopted baseline
			// while the adopted store changes stay — the two out of step, which is the
			// one thing this path exists to prevent.
			setDisk({
				repos: { "/mine": repoRecord("/mine", "Mine"), "/other": repoRecord("/other", "Other") },
				repoOrder: ["/mine", "/other"],
			});
			await broadcast();
			expect(store.get("/other"), "adoption must wait for the save").toBeUndefined();

			releaseSave!();
			await vi.advanceTimersByTimeAsync(0);

			expect(store.get("/other")?.displayName).toBe("Other");

			// And the baseline moved with it: the next save carries no `/other` delta.
			store.setDisplayName("/mine", "Renamed");
			await vi.advanceTimersByTimeAsync(500);
			expect(lastSavedMutation()?.repos.map((entry) => entry.id)).toEqual(["/mine"]);
		});
	});

	it("adopts a removal of a repository with nothing open in it", async () => {
		setDisk({
			repos: { "/repo": repoRecord("/repo", "Original"), "/keep": repoRecord("/keep", "Keep") },
			repoOrder: ["/repo", "/keep"],
		});

		await testInScopeAsync(async () => {
			await store.hydrate();

			setDisk({ repos: { "/keep": repoRecord("/keep", "Keep") }, repoOrder: ["/keep"] });
			await broadcast();

			expect(store.get("/repo")).toBeUndefined();
			expect(store.state.repoOrder).toEqual(["/keep"]);
		});
	});

	it("ignores the echo of this client's own save", async () => {
		await testInScopeAsync(async () => {
			await store.hydrate();

			store.add({ path: "/mine", displayName: "Mine" });
			await vi.advanceTimersByTimeAsync(500);

			// The backend announces every write, including this client's. Disk now holds
			// exactly what was just sent, so the echo must change nothing.
			setDisk({ repos: { "/mine": repoRecord("/mine", "Mine") }, repoOrder: ["/mine"] });
			await broadcast();

			expect(store.get("/mine")?.displayName).toBe("Mine");

			const savesBefore = mockInvoke.mock.calls.filter((call: unknown[]) => call[0] === "save_repositories").length;
			await vi.advanceTimersByTimeAsync(500);
			const savesAfter = mockInvoke.mock.calls.filter((call: unknown[]) => call[0] === "save_repositories").length;
			expect(savesAfter).toBe(savesBefore);
		});
	});

	it("subscribes once even if hydrate runs again", async () => {
		await testInScopeAsync(async () => {
			await store.hydrate();
			await store.hydrate();
		});

		const subscriptions = mockListen.mock.calls.filter((call) => call[0] === "repositories-changed");
		expect(subscriptions).toHaveLength(1);
	});

	it("does not adopt anything before hydrate has a baseline", () => {
		testInScope(() => {
			expect(listeners.has("repositories-changed")).toBe(false);
		});
	});
});
