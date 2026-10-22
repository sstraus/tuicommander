import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../invoke", () => ({
	invoke: vi.fn(() => Promise.resolve(null)),
}));

import { makeTerminal, testInScope, testInScopeAsync } from "../helpers/store";

/**
 * Per-repo worktree consolidation (#e767) — the selection rule and the reactive
 * glue. What lands in a repo's workspace is "every terminal of every branch that
 * has a worktree", recomputed from scratch, so create/remove/archive all fall
 * out of the same code path.
 */
describe("worktree consolidation", () => {
	let hook: typeof import("../../hooks/useWorktreeConsolidation");
	let repositoriesStore: typeof import("../../stores/repositories").repositoriesStore;
	let repoSettingsStore: typeof import("../../stores/repoSettings").repoSettingsStore;
	let terminalsStore: typeof import("../../stores/terminals").terminalsStore;

	beforeEach(async () => {
		vi.resetModules();
		hook = await import("../../hooks/useWorktreeConsolidation");
		repositoriesStore = (await import("../../stores/repositories")).repositoriesStore;
		repoSettingsStore = (await import("../../stores/repoSettings")).repoSettingsStore;
		terminalsStore = (await import("../../stores/terminals")).terminalsStore;
		(await import("../../stores/paneLayout")).resetGroupCounter();
	});

	// `saveRepos`'s debounce schedules a real 500ms setTimeout unconditionally
	// (the `hydrated` check only gates the eventual callback), so every
	// `repositoriesStore.add`/`addTerminalToWorkspace` call above leaves one
	// pending — cancel it or it leaks past the test into vitest's detector.
	afterEach(() => {
		repositoriesStore._testCancelPendingSave();
	});

	const REPO = "/repo/a";

	/** Register a repo with a main branch and `worktrees` worktree branches. */
	function seedRepo(worktrees: string[]): Record<string, string> {
		repositoriesStore.add({ path: REPO, displayName: "a" });
		repositoriesStore.setWorkspace(REPO, "main", { isMain: true, worktreePath: null });
		const ids: Record<string, string> = {};
		for (const name of worktrees) {
			repositoriesStore.setWorkspace(REPO, name, { worktreePath: `/wt/${name}` });
			const termId = terminalsStore.add(makeTerminal({ name }));
			repositoriesStore.addTerminalToWorkspace(REPO, name, termId);
			ids[name] = termId;
		}
		return ids;
	}

	it("selects the terminals of worktree branches only", () => {
		testInScope(() => {
			const ids = seedRepo(["feat-1", "feat-2"]);
			// The main branch has no worktreePath, so its terminals stay out.
			const mainTerm = terminalsStore.add(makeTerminal({ name: "main" }));
			repositoriesStore.addTerminalToWorkspace(REPO, "main", mainTerm);

			const selected = hook.worktreeTerminalsOf(REPO);

			expect(selected.sort()).toEqual([ids["feat-1"], ids["feat-2"]].sort());
			expect(selected).not.toContain(mainTerm);
		});
	});

	it("returns nothing for a repo it has never seen", () => {
		testInScope(() => {
			expect(hook.worktreeTerminalsOf("/repo/never")).toEqual([]);
		});
	});

	it("lists only the repos whose toggle is on", () => {
		testInScope(() => {
			repoSettingsStore.getOrCreate(REPO, "a");
			repoSettingsStore.getOrCreate("/repo/b", "b");
			expect(hook.consolidatedRepos()).toEqual([]);

			repoSettingsStore.update(REPO, { autoConsolidateWorktrees: true });

			expect(hook.consolidatedRepos()).toEqual([REPO]);
		});
	});

	it("defaults the toggle to off so no repo consolidates unasked", () => {
		testInScope(() => {
			const created = repoSettingsStore.getOrCreate(REPO, "a");
			expect(created.autoConsolidateWorktrees).toBe(false);
		});
	});

	/**
	 * The reactive glue itself: `useWorktreeConsolidation()`'s two effects drive
	 * `globalWorkspaceStore` into the state PanelOrchestrator's suppression check
	 * (`isActive() && getScope() === MANUAL_SCOPE`) depends on. Only the pure
	 * selectors above were covered before; these exercise the actual wiring.
	 */
	describe("reactive activation", () => {
		let globalWorkspaceStore: typeof import("../../stores/globalWorkspace").globalWorkspaceStore;
		let MANUAL_SCOPE: typeof import("../../stores/globalWorkspace").MANUAL_SCOPE;
		let paneLayoutStore: typeof import("../../stores/paneLayout").paneLayoutStore;

		beforeEach(async () => {
			const gw = await import("../../stores/globalWorkspace");
			globalWorkspaceStore = gw.globalWorkspaceStore;
			MANUAL_SCOPE = gw.MANUAL_SCOPE;
			paneLayoutStore = (await import("../../stores/paneLayout")).paneLayoutStore;
		});

		afterEach(() => {
			repositoriesStore._testCancelPendingSave();
			paneLayoutStore._testCancelPendingSave();
		});

		/** Let SolidJS flush its effect queue (createEffect runs on a microtask). */
		function flushEffects(): Promise<void> {
			return new Promise((resolve) => queueMicrotask(resolve));
		}

		it("switches scope to the repo and activates once it has worktrees to show", async () => {
			await testInScopeAsync(async () => {
				seedRepo(["feat-1"]);
				repoSettingsStore.getOrCreate(REPO, "a");
				repoSettingsStore.update(REPO, { autoConsolidateWorktrees: true });
				repositoriesStore.setActive(REPO);

				hook.useWorktreeConsolidation();
				await flushEffects();
				await flushEffects();

				expect(globalWorkspaceStore.getScope()).toBe(REPO);
				expect(globalWorkspaceStore.isActive()).toBe(true);
			});
		});

		it("switches scope but stays inactive for a consolidated repo with no worktrees yet", async () => {
			await testInScopeAsync(async () => {
				seedRepo([]);
				repoSettingsStore.getOrCreate(REPO, "a");
				repoSettingsStore.update(REPO, { autoConsolidateWorktrees: true });
				repositoriesStore.setActive(REPO);

				hook.useWorktreeConsolidation();
				await flushEffects();
				await flushEffects();

				expect(globalWorkspaceStore.getScope()).toBe(REPO);
				expect(globalWorkspaceStore.isActive()).toBe(false);
			});
		});

		it("leaves a hand-promoted manual workspace open when the active repo isn't consolidated", async () => {
			await testInScopeAsync(async () => {
				const manualTerm = terminalsStore.add(makeTerminal({ name: "manual" }));
				globalWorkspaceStore.promote(manualTerm);
				globalWorkspaceStore.activate();

				repositoriesStore.add({ path: "/repo/other", displayName: "other" });
				repositoriesStore.setActive("/repo/other");

				hook.useWorktreeConsolidation();
				await flushEffects();
				await flushEffects();

				expect(globalWorkspaceStore.getScope()).toBe(MANUAL_SCOPE);
				expect(globalWorkspaceStore.isActive()).toBe(true);
			});
		});

		it("deactivates and reverts to manual scope when the repo's toggle is turned back off", async () => {
			await testInScopeAsync(async () => {
				seedRepo(["feat-1"]);
				repoSettingsStore.getOrCreate(REPO, "a");
				repoSettingsStore.update(REPO, { autoConsolidateWorktrees: true });
				repositoriesStore.setActive(REPO);

				hook.useWorktreeConsolidation();
				await flushEffects();
				await flushEffects();
				expect(globalWorkspaceStore.isActive()).toBe(true);

				repoSettingsStore.update(REPO, { autoConsolidateWorktrees: false });
				await flushEffects();
				await flushEffects();

				expect(globalWorkspaceStore.isActive()).toBe(false);
				expect(globalWorkspaceStore.getScope()).toBe(MANUAL_SCOPE);
			});
		});
	});
});
