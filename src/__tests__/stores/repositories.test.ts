import { batch } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { testInScope, testInScopeAsync } from "../helpers/store";

const mockInvoke = vi.fn().mockResolvedValue(undefined);

vi.mock("@tauri-apps/api/core", () => ({
	invoke: mockInvoke,
}));

describe("repositoriesStore", () => {
	let store: typeof import("../../stores/repositories").repositoriesStore;
	let logger: typeof import("../../stores/appLogger").appLogger;

	function lastRepositoryMutation() {
		const calls = mockInvoke.mock.calls.filter((call: unknown[]) => call[0] === "save_repositories");
		const last = calls.at(-1)!;
		return (
			last[1] as {
				config: {
					// `before` is half of the compare-and-swap, not optional detail: the
					// conflict-rebase tests assert the backend is handed the baseline it
					// actually holds on disk.
					repos: Array<{ id: string; before: unknown; after: unknown }>;
					groups: Array<{ id: string; before: unknown; after: unknown }>;
					groupOrder?: { after: string[] };
				};
			}
		).config;
	}

	beforeEach(async () => {
		vi.resetModules();
		vi.useFakeTimers();
		mockInvoke.mockReset().mockResolvedValue(undefined);
		localStorage.clear();

		vi.doMock("@tauri-apps/api/core", () => ({
			invoke: mockInvoke,
		}));

		store = (await import("../../stores/repositories")).repositoriesStore;
		logger = (await import("../../stores/appLogger")).appLogger;
		store._testSetHydrated(true);
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	describe("add()", () => {
		it("adds a repository", () => {
			testInScope(() => {
				store.add({ path: "/path/to/repo", displayName: "my-project" });
				const repo = store.get("/path/to/repo");
				expect(repo).toBeDefined();
				expect(repo!.displayName).toBe("my-project");
				expect(repo!.expanded).toBe(true);
				expect(repo!.collapsed).toBe(false);
			});
		});

		it("stores initials from Rust backend", () => {
			testInScope(() => {
				store.add({ path: "/path/1", displayName: "my-project", initials: "MP" });
				expect(store.get("/path/1")!.initials).toBe("MP");

				store.add({ path: "/path/2", displayName: "app", initials: "AP" });
				expect(store.get("/path/2")!.initials).toBe("AP");
			});
		});

		it("persists via invoke (debounced)", () => {
			testInScope(() => {
				store.add({ path: "/path/to/repo", displayName: "test" });
				vi.advanceTimersByTime(500);
				expect(mockInvoke).toHaveBeenCalledWith("save_repositories", {
					config: expect.objectContaining({
						mutationVersion: 1,
						repos: expect.arrayContaining([
							expect.objectContaining({
								id: "/path/to/repo",
								after: expect.objectContaining({ displayName: "test" }),
							}),
						]),
					}),
				});
			});
		});

		it("does not persist terminals via invoke", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "test" });
				store.setWorkspace("/repo", "main");
				store.addTerminalToWorkspace("/repo", "main", "term-1");
				vi.advanceTimersByTime(500);
				// Find the last save_repositories call
				const repo = lastRepositoryMutation().repos.find((mutation) => mutation.id === "/repo")?.after as {
					workspaces: Record<string, { terminals: string[] }>;
				};
				expect(repo.workspaces.main.terminals).toEqual([]);
			});
		});
	});

	describe("setCiAutoHeal()", () => {
		it("persists ciAutoHeal via invoke (debounced)", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "test" });
				store.setWorkspace("/repo", "main");
				store.setCiAutoHeal("/repo", "main", { enabled: true, attempts: 1 });
				vi.advanceTimersByTime(500);
				const repo = lastRepositoryMutation().repos.find((mutation) => mutation.id === "/repo")?.after as {
					workspaces: Record<string, { ciAutoHeal: unknown }>;
				};
				expect(repo.workspaces.main.ciAutoHeal).toEqual({ enabled: true, attempts: 1 });
			});
		});

		it("never persists the transient healing flag", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "test" });
				store.setWorkspace("/repo", "main");
				store.setCiAutoHeal("/repo", "main", { enabled: true, attempts: 2, healing: true });
				vi.advanceTimersByTime(500);
				const repo = lastRepositoryMutation().repos.find((mutation) => mutation.id === "/repo")?.after as {
					workspaces: Record<string, { ciAutoHeal: { healing: boolean } }>;
				};
				expect(repo.workspaces.main.ciAutoHeal.healing).toBe(false);
			});
		});
	});

	describe("remove()", () => {
		it("removes a repository", () => {
			testInScope(() => {
				store.add({ path: "/path/to/repo", displayName: "test" });
				store.remove("/path/to/repo");
				expect(store.get("/path/to/repo")).toBeUndefined();
			});
		});

		it("clears activeRepoPath if removed repo was active", () => {
			testInScope(() => {
				store.add({ path: "/path/to/repo", displayName: "test" });
				store.setActive("/path/to/repo");
				store.remove("/path/to/repo");
				expect(store.state.activeRepoPath).toBeNull();
			});
		});
	});

	describe("setActive()", () => {
		it("sets active repository", () => {
			testInScope(() => {
				store.add({ path: "/path/to/repo", displayName: "test" });
				store.setActive("/path/to/repo");
				expect(store.state.activeRepoPath).toBe("/path/to/repo");
			});
		});

		it("persists active selection as an expected field mutation", () => {
			testInScope(() => {
				store.add({ path: "/path/to/repo", displayName: "test" });
				store.setActive("/path/to/repo");
				vi.advanceTimersByTime(500);

				const calls = mockInvoke.mock.calls.filter((call: unknown[]) => call[0] === "save_repositories");
				const mutation = (
					calls[0][1] as {
						config: { activeRepoPath?: { before: string | null; after: string | null } };
					}
				).config.activeRepoPath;
				expect(mutation).toEqual({ before: null, after: "/path/to/repo" });
			});
		});

		// The "and switch" half of the default `active_and_switch` index strategy.
		// Without this call the strategy only ever pre-warms the boot repo, and a
		// cross-repo content search reports every other repo as pending forever.
		// Rust owns whether the warm actually runs (`content_index::warm_index`
		// honours `index_strategy`); the store's only job is to say a switch
		// happened.
		it("asks the backend to warm the content index of the repo switched to", () => {
			testInScope(() => {
				store.add({ path: "/path/to/repo", displayName: "test" });
				mockInvoke.mockClear();
				store.setActive("/path/to/repo");

				expect(mockInvoke.mock.calls.filter((call: unknown[]) => call[0] === "warm_content_index")).toEqual([
					["warm_content_index", { repoPath: "/path/to/repo" }],
				]);
			});
		});

		it("does not warm anything when the active repo is cleared", () => {
			testInScope(() => {
				store.add({ path: "/path/to/repo", displayName: "test" });
				store.setActive("/path/to/repo");
				mockInvoke.mockClear();
				store.setActive(null);

				expect(mockInvoke.mock.calls.some((call: unknown[]) => call[0] === "warm_content_index")).toBe(false);
			});
		});
	});

	describe("toggleExpanded()", () => {
		it("toggles expanded state", () => {
			testInScope(() => {
				store.add({ path: "/path/to/repo", displayName: "test" });
				expect(store.get("/path/to/repo")!.expanded).toBe(true);
				store.toggleExpanded("/path/to/repo");
				expect(store.get("/path/to/repo")!.expanded).toBe(false);
			});
		});
	});

	describe("branches", () => {
		it("setWorkspace creates a new branch", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "test" });
				store.setWorkspace("/repo", "feature/test");
				const repo = store.get("/repo")!;
				expect(repo.workspaces["feature/test"]).toBeDefined();
				expect(repo.workspaces["feature/test"].branchName).toBe("feature/test");
				expect(repo.workspaces["feature/test"].isMain).toBe(false);
			});
		});

		it("setWorkspace detects main branches", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "test" });
				store.setWorkspace("/repo", "main");
				expect(store.get("/repo")!.workspaces["main"].isMain).toBe(true);

				store.setWorkspace("/repo", "master");
				expect(store.get("/repo")!.workspaces["master"].isMain).toBe(true);

				store.setWorkspace("/repo", "develop");
				expect(store.get("/repo")!.workspaces["develop"].isMain).toBe(true);
			});
		});

		it("setWorkspace updates existing branch", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "test" });
				store.setWorkspace("/repo", "main");
				store.setWorkspace("/repo", "main", { additions: 5, deletions: 3 });
				expect(store.get("/repo")!.workspaces["main"].additions).toBe(5);
			});
		});

		it("setActiveBranch sets the active branch", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "test" });
				store.setWorkspace("/repo", "main");
				store.setActiveWorkspace("/repo", "main");
				expect(store.get("/repo")!.activeWorkspaceId).toBe("main");
			});
		});

		/**
		 * A pointer at a row that does not exist breaks two things far from here: the
		 * sidebar renders no tab for the active workspace, and the next terminal is
		 * created with a null cwd — which the backend spawns in the user's HOME.
		 * Observed live on a repo whose active id named a workspace that had been
		 * pruned. The previous pointer at least names a row that exists.
		 */
		it("setActiveWorkspace refuses an id that names no workspace", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "test" });
				store.setWorkspace("/repo", "main");
				store.setActiveWorkspace("/repo", "main");

				store.setActiveWorkspace("/repo", "branch-that-was-pruned");

				expect(store.get("/repo")!.activeWorkspaceId).toBe("main");
			});
		});

		it("setActiveWorkspace still accepts null — no repo has a workspace on screen", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "test" });
				store.setWorkspace("/repo", "main");
				store.setActiveWorkspace("/repo", "main");

				store.setActiveWorkspace("/repo", null);

				expect(store.get("/repo")!.activeWorkspaceId).toBeNull();
			});
		});

		it("setActiveWorkspace accepts a row created in the same batch", () => {
			// Several callers create the row and activate it back to back; a store read
			// inside a batch sees the write, so the guard must not reject that order.
			testInScope(() => {
				store.add({ path: "/repo", displayName: "test" });
				batch(() => {
					store.setWorkspace("/repo", "feature", { branchName: "feature" });
					store.setActiveWorkspace("/repo", "feature");
				});
				expect(store.get("/repo")!.activeWorkspaceId).toBe("feature");
			});
		});

		it("removing the active workspace repoints rather than dangling", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "test" });
				store.setWorkspace("/repo", "main");
				store.setWorkspace("/repo", "feature");
				store.setActiveWorkspace("/repo", "feature");

				store.removeWorkspace("/repo", "feature");

				const repo = store.get("/repo")!;
				expect(repo.activeWorkspaceId).toBe("main");
				expect(repo.workspaces[repo.activeWorkspaceId!]).toBeDefined();
			});
		});
	});

	describe("terminal-branch association", () => {
		it("addTerminalToWorkspace adds terminal", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "test" });
				store.setWorkspace("/repo", "main");
				store.addTerminalToWorkspace("/repo", "main", "term-1");
				expect(store.get("/repo")!.workspaces["main"].terminals).toContain("term-1");
			});
		});

		it("addTerminalToWorkspace prevents duplicates", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "test" });
				store.setWorkspace("/repo", "main");
				store.addTerminalToWorkspace("/repo", "main", "term-1");
				store.addTerminalToWorkspace("/repo", "main", "term-1");
				expect(store.get("/repo")!.workspaces["main"].terminals).toHaveLength(1);
			});
		});

		it("removeTerminalFromWorkspace removes terminal", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "test" });
				store.setWorkspace("/repo", "main");
				store.addTerminalToWorkspace("/repo", "main", "term-1");
				store.removeTerminalFromWorkspace("/repo", "main", "term-1");
				expect(store.get("/repo")!.workspaces["main"].terminals).toHaveLength(0);
			});
		});

		it("removeTerminalFromWorkspace clears savedTerminals when last terminal removed", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "test" });
				store.setWorkspace("/repo", "main", {
					savedTerminals: [
						{ name: "T1", cwd: "/repo", fontSize: 14, agentType: null },
						{ name: "T2", cwd: "/repo", fontSize: 14, agentType: null },
					],
				});
				store.addTerminalToWorkspace("/repo", "main", "term-1");
				store.addTerminalToWorkspace("/repo", "main", "term-2");

				// Remove first — savedTerminals should persist (still have one terminal)
				store.removeTerminalFromWorkspace("/repo", "main", "term-1");
				expect(store.get("/repo")!.workspaces["main"].savedTerminals).toHaveLength(2);

				// Remove last — savedTerminals should be cleared
				store.removeTerminalFromWorkspace("/repo", "main", "term-2");
				expect(store.get("/repo")!.workspaces["main"].savedTerminals).toHaveLength(0);
			});
		});
	});

	describe("getRepoPathForTerminal()", () => {
		it("returns repo path for a known terminal", () => {
			testInScope(() => {
				store.add({ path: "/repo-a", displayName: "A" });
				store.setWorkspace("/repo-a", "main");
				store.addTerminalToWorkspace("/repo-a", "main", "term-1");
				expect(store.getRepoPathForTerminal("term-1")).toBe("/repo-a");
			});
		});

		it("returns null for an unknown terminal", () => {
			testInScope(() => {
				store.add({ path: "/repo-a", displayName: "A" });
				store.setWorkspace("/repo-a", "main");
				expect(store.getRepoPathForTerminal("nonexistent")).toBeNull();
			});
		});

		it("finds terminal across multiple repos and branches", () => {
			testInScope(() => {
				store.add({ path: "/repo-a", displayName: "A" });
				store.add({ path: "/repo-b", displayName: "B" });
				store.setWorkspace("/repo-a", "main");
				store.setWorkspace("/repo-b", "feature");
				store.addTerminalToWorkspace("/repo-a", "main", "term-1");
				store.addTerminalToWorkspace("/repo-b", "feature", "term-2");
				expect(store.getRepoPathForTerminal("term-1")).toBe("/repo-a");
				expect(store.getRepoPathForTerminal("term-2")).toBe("/repo-b");
			});
		});

		it("returns null after terminal is removed from branch", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "test" });
				store.setWorkspace("/repo", "main");
				store.addTerminalToWorkspace("/repo", "main", "term-1");
				expect(store.getRepoPathForTerminal("term-1")).toBe("/repo");
				store.removeTerminalFromWorkspace("/repo", "main", "term-1");
				expect(store.getRepoPathForTerminal("term-1")).toBeNull();
			});
		});
	});

	describe("removeWorkspace()", () => {
		it("removes a branch", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "test" });
				store.setWorkspace("/repo", "feature");
				store.removeWorkspace("/repo", "feature");
				expect(store.get("/repo")!.workspaces["feature"]).toBeUndefined();
			});
		});

		it("updates activeBranch when removed branch was active", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "test" });
				store.setWorkspace("/repo", "main");
				store.setWorkspace("/repo", "feature");
				store.setActiveWorkspace("/repo", "feature");
				store.removeWorkspace("/repo", "feature");
				expect(store.get("/repo")!.activeWorkspaceId).toBe("main");
			});
		});
	});

	describe("renameBranch()", () => {
		it("renames a branch", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "test" });
				store.setWorkspace("/repo", "old-name");
				store.addTerminalToWorkspace("/repo", "old-name", "term-1");
				store.renameBranch("/repo", "old-name", "new-name");
				expect(store.get("/repo")!.workspaces["old-name"]).toBeUndefined();
				expect(store.get("/repo")!.workspaces["new-name"]).toBeDefined();
				expect(store.get("/repo")!.workspaces["new-name"].terminals).toContain("term-1");
			});
		});

		it("updates activeBranch when renamed branch was active", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "test" });
				store.setWorkspace("/repo", "old-name");
				store.setActiveWorkspace("/repo", "old-name");
				store.renameBranch("/repo", "old-name", "new-name");
				expect(store.get("/repo")!.activeWorkspaceId).toBe("new-name");
			});
		});
	});

	describe("mergeWorkspaceState()", () => {
		it("moves terminals and flags from source to target", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "test" });
				store.setWorkspace("/repo", "old-branch", { hadTerminals: true });
				store.addTerminalToWorkspace("/repo", "old-branch", "term-1");
				store.addTerminalToWorkspace("/repo", "old-branch", "term-2");
				store.setWorkspace("/repo", "new-branch", { worktreePath: "/repo" });

				store.mergeWorkspaceState("/repo", "old-branch", "new-branch");

				const oldB = store.get("/repo")!.workspaces["old-branch"];
				const newB = store.get("/repo")!.workspaces["new-branch"];
				expect(oldB.terminals).toEqual([]);
				expect(newB.terminals).toContain("term-1");
				expect(newB.terminals).toContain("term-2");
				expect(newB.hadTerminals).toBe(true);
				// Target keeps its worktreePath
				expect(newB.worktreePath).toBe("/repo");
			});
		});

		it("transfers savedTerminals when target has none", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "test" });
				store.setWorkspace("/repo", "old", {
					savedTerminals: [{ name: "T", cwd: "/repo", fontSize: 14, agentType: null }],
				});
				store.setWorkspace("/repo", "new", {});

				store.mergeWorkspaceState("/repo", "old", "new");

				expect(store.get("/repo")!.workspaces["old"].savedTerminals).toEqual([]);
				expect(store.get("/repo")!.workspaces["new"].savedTerminals?.length).toBe(1);
			});
		});

		it("does not overwrite target savedTerminals", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "test" });
				store.setWorkspace("/repo", "old", {
					savedTerminals: [{ name: "Old", cwd: "/repo", fontSize: 14, agentType: null }],
				});
				store.setWorkspace("/repo", "new", {
					savedTerminals: [{ name: "Existing", cwd: "/repo", fontSize: 14, agentType: null }],
				});

				store.mergeWorkspaceState("/repo", "old", "new");

				// Target keeps its own savedTerminals
				expect(store.get("/repo")!.workspaces["new"].savedTerminals?.[0]?.name).toBe("Existing");
			});
		});
	});

	describe("getActive()", () => {
		it("returns active repo", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "test" });
				store.setActive("/repo");
				expect(store.getActive()?.path).toBe("/repo");
			});
		});

		it("returns undefined when no active", () => {
			testInScope(() => {
				expect(store.getActive()).toBeUndefined();
			});
		});
	});

	describe("isGitRepo()", () => {
		it("returns false for a registered plain directory", () => {
			testInScope(() => {
				store.add({ path: "/plain", displayName: "plain", isGitRepo: false });
				expect(store.isGitRepo("/plain")).toBe(false);
			});
		});

		it("returns true for a registered git repo", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "repo" });
				expect(store.isGitRepo("/repo")).toBe(true);
			});
		});

		// Worktree paths are not registered under their own key — callers pass the
		// worktree path and must not be gated out of git calls.
		it("returns true for an unknown path", () => {
			testInScope(() => {
				expect(store.isGitRepo("/never-registered")).toBe(true);
			});
		});
	});

	describe("getPaths()", () => {
		it("returns all repo paths", () => {
			testInScope(() => {
				store.add({ path: "/repo1", displayName: "test1" });
				store.add({ path: "/repo2", displayName: "test2" });
				expect(store.getPaths()).toEqual(["/repo1", "/repo2"]);
			});
		});
	});

	describe("hydrate()", () => {
		it("loads repos from Rust backend and clears stale terminals", async () => {
			mockInvoke.mockResolvedValueOnce({
				repos: {
					"/repo": {
						path: "/repo",
						displayName: "test",
						initials: "TE",
						expanded: true,
						collapsed: false,
						workspaces: {
							main: {
								name: "main",
								isMain: true,
								worktreePath: "/repo",
								terminals: ["stale-term-1", "stale-term-2"],
								additions: 0,
								deletions: 0,
							},
						},
						activeBranch: "main",
					},
				},
			});

			await testInScopeAsync(async () => {
				await store.hydrate();
				const repo = store.get("/repo");
				expect(repo).toBeDefined();
				expect(repo!.workspaces["main"].terminals).toEqual([]);
				expect(mockInvoke).toHaveBeenCalledWith("load_repositories");
			});
		});

		it("migrates expanded/collapsed fields when missing", async () => {
			mockInvoke.mockResolvedValueOnce({
				repos: {
					"/repo": {
						path: "/repo",
						displayName: "test",
						initials: "TE",
						workspaces: {
							main: {
								name: "main",
								isMain: true,
								worktreePath: "/repo",
								terminals: [],
								additions: 0,
								deletions: 0,
							},
						},
						activeBranch: "main",
						// No collapsed or expanded fields — migration should add them
					},
				},
			});

			await testInScopeAsync(async () => {
				await store.hydrate();
				const repo = store.get("/repo");
				expect(repo).toBeDefined();
				expect(repo!.collapsed).toBe(false);
				expect(repo!.expanded).toBe(true);
			});
		});

		// #763-d219: a repo record with a missing/blank displayName reached the
		// Command Palette as `label: undefined`, and `baseSort` crashed the whole
		// app on `undefined.localeCompare`. `normalizeLoadedRepo` must sanitize it
		// on every path a record enters the store — hydrate here, adoption below.
		describe("displayName sanitization (#763-d219)", () => {
			it.each([
				["missing", undefined],
				["null", null],
				["a number", 42],
				["blank", ""],
				["whitespace-only", "   "],
			])("falls back to a path-derived name when displayName is %s", async (_label, badDisplayName) => {
				mockInvoke.mockResolvedValueOnce({
					repos: {
						"/tmp/shell-repos/xyz-workspace": {
							path: "/tmp/shell-repos/xyz-workspace",
							displayName: badDisplayName,
							initials: "",
							isGitRepo: false,
							workspaces: {},
							activeWorkspaceId: null,
						},
					},
				});
				const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});

				await testInScopeAsync(async () => {
					await store.hydrate();
					const repo = store.get("/tmp/shell-repos/xyz-workspace");
					expect(repo).toBeDefined();
					expect(repo!.displayName).toBe("xyz-workspace");
					expect(typeof repo!.displayName).toBe("string");
					expect(warnSpy).toHaveBeenCalledWith(
						"[store]",
						"Repository record had an invalid displayName; using path-derived fallback",
					);
					expect(logger.getEntries()).toContainEqual(
						expect.objectContaining({
							level: "warn",
							source: "store",
							data: expect.objectContaining({ path: "/tmp/shell-repos/xyz-workspace" }),
						}),
					);
				});

				warnSpy.mockRestore();
			});

			it("normalizes a Windows-style path the same way", async () => {
				mockInvoke.mockResolvedValueOnce({
					repos: {
						"C:\\Users\\boss\\AppData\\Local\\Temp\\shell-repo": {
							path: "C:\\Users\\boss\\AppData\\Local\\Temp\\shell-repo",
							displayName: undefined,
							initials: "",
							isGitRepo: false,
							workspaces: {},
							activeWorkspaceId: null,
						},
					},
				});
				const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});

				await testInScopeAsync(async () => {
					await store.hydrate();
					const repo = store.get("C:\\Users\\boss\\AppData\\Local\\Temp\\shell-repo");
					expect(repo!.displayName).toBe("shell-repo");
				});

				warnSpy.mockRestore();
			});

			it("leaves a valid displayName untouched", async () => {
				mockInvoke.mockResolvedValueOnce({
					repos: {
						"/repo": {
							path: "/repo",
							displayName: "My Repo",
							initials: "MR",
							workspaces: {},
							activeWorkspaceId: null,
						},
					},
				});

				await testInScopeAsync(async () => {
					await store.hydrate();
					expect(store.get("/repo")!.displayName).toBe("My Repo");
				});
			});

			// The adopt-path regression (a remote client's write coming back through
			// the `repositories-changed` broadcast) lives in
			// repositoriesRemoteSync.test.ts, which already mocks the Tauri
			// `listen` used by that path — see "sanitizes a displayName the
			// remote client wrote" there.
		});

		it("handles hydration failure gracefully", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("load failed"));
			const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				await store.hydrate(); // Should not throw
				expect(store.getPaths()).toEqual([]);
				expect(errorSpy).toHaveBeenCalledWith("[store]", "Failed to hydrate repositories");
				expect(logger.getEntries()).toContainEqual(
					expect.objectContaining({
						level: "error",
						source: "store",
						message: "Failed to hydrate repositories",
						data: expect.any(Error),
					}),
				);
				errorSpy.mockRestore();
			});
		});

		it("blocks save after hydration failure", async () => {
			// Reset hydrated flag to false — beforeEach sets it to true for other tests,
			// but this test specifically checks that a failed hydration blocks saves.
			store._testSetHydrated(false);
			mockInvoke.mockRejectedValueOnce(new Error("load failed"));
			const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.getPaths()).toEqual([]);

				// Clear mock calls from the hydrate attempt
				mockInvoke.mockClear();

				// Attempt a mutation after failed hydration
				store.add({ path: "/test-repo", displayName: "Test" });

				// Advance timers to trigger any debounced saves
				vi.advanceTimersByTime(1000);

				// Save should NOT have been called (hydrated flag is false)
				const saveCalls = mockInvoke.mock.calls.filter((c: unknown[]) => c[0] === "save_repositories");
				expect(saveCalls).toHaveLength(0);
			});

			errorSpy.mockRestore();
		});

		it("refuses to hydrate a mutation delta left by a stale backend", async () => {
			// Exactly what was on disk after the 2026-08-21 incident: a backend too old
			// to apply the keyed delta stored the delta itself as the whole document.
			store._testSetHydrated(false);
			mockInvoke.mockResolvedValueOnce({ mutationVersion: 1, repos: [], groups: [] });
			const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.getPaths()).toEqual([]);
				expect(errorSpy).toHaveBeenCalledWith("[store]", expect.stringContaining("mutation delta"));

				// The file is still the user's only copy. Saving the empty in-memory state
				// over it is what made the incident unrecoverable, so saves stay blocked.
				mockInvoke.mockClear();
				store.add({ path: "/test-repo", displayName: "Test" });
				vi.advanceTimersByTime(1000);
				const saveCalls = mockInvoke.mock.calls.filter((c: unknown[]) => c[0] === "save_repositories");
				expect(saveCalls).toHaveLength(0);
			});

			errorSpy.mockRestore();
		});

		it("drops a persisted agentType the current build no longer knows", async () => {
			// `fx` was a first-class agent for five days before being reverted, long
			// enough to reach savedTerminals on disk. AGENT_DISPLAY and AGENTS are
			// exhaustive `Record<AgentType, …>` indexed without an existence check, so
			// restoring the value verbatim throws inside StatusBar's render — and no
			// ErrorBoundary wraps it. Sanitise once, at the disk boundary.
			store._testSetHydrated(false);
			mockInvoke.mockResolvedValueOnce({
				repos: {
					"/repo": {
						path: "/repo",
						displayName: "Repo",
						workspaces: {
							main: {
								savedTerminals: [
									{ name: "t1", cwd: null, fontSize: 12, agentType: "fx" },
									{ name: "t2", cwd: null, fontSize: 12, agentType: "claude" },
								],
							},
						},
					},
				},
				repoOrder: ["/repo"],
			});

			await testInScopeAsync(async () => {
				await store.hydrate();
				const saved = store.get("/repo")?.workspaces.main?.savedTerminals ?? [];
				expect(saved.map((t) => t.agentType)).toEqual([null, "claude"]);
			});
		});

		it("refuses a delta recognised by its array `repos` alone", async () => {
			// The envelope carries no `mutationVersion` once a future protocol drops it,
			// so the shape of `repos` has to be enough on its own.
			store._testSetHydrated(false);
			mockInvoke.mockResolvedValueOnce({ repos: [], groups: [] });
			const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(errorSpy).toHaveBeenCalledWith("[store]", expect.stringContaining("mutation delta"));
			});

			errorSpy.mockRestore();
		});

		it("hydrates a first-run empty document instead of reading it as a delta", async () => {
			// Failing closed here would brick every fresh install: no file yet means an
			// empty document, which must hydrate to an empty store with saves ENABLED.
			store._testSetHydrated(false);
			mockInvoke.mockResolvedValueOnce({});

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.getPaths()).toEqual([]);

				mockInvoke.mockClear();
				store.add({ path: "/first-repo", displayName: "First" });
				vi.advanceTimersByTime(1000);
				const saveCalls = mockInvoke.mock.calls.filter((c: unknown[]) => c[0] === "save_repositories");
				expect(saveCalls).toHaveLength(1);
			});
		});

		it("migrates from localStorage on first run", async () => {
			const staleData = {
				"/repo": {
					path: "/repo",
					displayName: "test",
					initials: "TE",
					expanded: true,
					collapsed: false,
					workspaces: {},
					activeBranch: null,
				},
			};
			localStorage.setItem("tui-commander-repos", JSON.stringify(staleData));
			mockInvoke.mockResolvedValueOnce(undefined); // save_repositories migration
			mockInvoke.mockResolvedValueOnce({ repos: {} }); // load_repositories

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(localStorage.getItem("tui-commander-repos")).toBeNull();
			});
		});

		it("keeps legacy repositories when the migration conflicts", async () => {
			const staleData = {
				"/repo": {
					path: "/repo",
					displayName: "test",
					initials: "TE",
					expanded: true,
					collapsed: false,
					parked: false,
					workspaces: {},
					activeBranch: null,
				},
			};
			localStorage.setItem("tui-commander-repos", JSON.stringify(staleData));
			mockInvoke.mockRejectedValueOnce(new Error("repository configuration conflict"));
			mockInvoke.mockResolvedValueOnce({ repos: {} });
			const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(localStorage.getItem("tui-commander-repos")).not.toBeNull();
				expect(errorSpy).toHaveBeenCalledWith("[store]", "Failed to migrate legacy repositories");
				expect(logger.getEntries()).toContainEqual(
					expect.objectContaining({
						message: "Failed to migrate legacy repositories",
						data: expect.any(Error),
					}),
				);
			});

			errorSpy.mockRestore();
		});
	});

	describe("toggleCollapsed()", () => {
		it("toggles collapsed state", () => {
			testInScope(() => {
				store.add({ path: "/path/to/repo", displayName: "test" });
				expect(store.get("/path/to/repo")!.collapsed).toBe(false);
				store.toggleCollapsed("/path/to/repo");
				expect(store.get("/path/to/repo")!.collapsed).toBe(true);
				store.toggleCollapsed("/path/to/repo");
				expect(store.get("/path/to/repo")!.collapsed).toBe(false);
			});
		});
	});

	describe("toggleWorkspaceTabsCollapsed()", () => {
		it("starts expanded: a new workspace carries no collapsed flag", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "My Repo" });
				store.setWorkspace("/repo", "feat/foo");
				expect(store.state.repositories["/repo"].workspaces["feat/foo"].tabsCollapsed).toBeFalsy();
			});
		});

		it("collapses on the first toggle and expands on the second", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "My Repo" });
				store.setWorkspace("/repo", "feat/foo");
				store.toggleWorkspaceTabsCollapsed("/repo", "feat/foo");
				expect(store.state.repositories["/repo"].workspaces["feat/foo"].tabsCollapsed).toBe(true);
				store.toggleWorkspaceTabsCollapsed("/repo", "feat/foo");
				expect(store.state.repositories["/repo"].workspaces["feat/foo"].tabsCollapsed).toBe(false);
			});
		});

		it("no-ops on unknown repo/branch", () => {
			testInScope(() => {
				expect(() => store.toggleWorkspaceTabsCollapsed("/nonexistent", "main")).not.toThrow();
			});
		});

		it("persists via save_repositories (debounced)", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "My Repo" });
				store.setWorkspace("/repo", "feat/foo");
				store.toggleWorkspaceTabsCollapsed("/repo", "feat/foo");
				vi.advanceTimersByTime(500);
				const calls = mockInvoke.mock.calls.filter((c: unknown[]) => c[0] === "save_repositories");
				expect(calls.length).toBeGreaterThan(0);
			});
		});
	});

	describe("isEmpty()", () => {
		it("returns true when empty", () => {
			testInScope(() => {
				expect(store.isEmpty()).toBe(true);
			});
		});

		it("returns false when repos exist", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "test" });
				expect(store.isEmpty()).toBe(false);
			});
		});
	});

	describe("revision tracking", () => {
		it("returns 0 for unknown repo", () => {
			testInScope(() => {
				expect(store.getRevision("/unknown-repo")).toBe(0);
			});
		});

		it("increments revision on bumpRevision", () => {
			testInScope(() => {
				store.add({ path: "/test", displayName: "test" });
				expect(store.getRevision("/test")).toBe(0);
				store.bumpRevision("/test");
				expect(store.getRevision("/test")).toBe(1);
				store.bumpRevision("/test");
				expect(store.getRevision("/test")).toBe(2);
			});
		});

		// The git revision is a strict subset of the general one: panels that only
		// read committed history (log, reflog, stashes) subscribe to it so a mere
		// file save stops re-running their git processes. The subset direction
		// matters — a git-state change also changes what those panels show, so it
		// MUST bump both. Bumping only the narrow counter would leave every
		// `getRevision` subscriber stale on commits.
		describe("git revision tracking", () => {
			it("returns 0 for unknown repo", () => {
				testInScope(() => {
					expect(store.getGitRevision("/unknown-repo")).toBe(0);
				});
			});

			it("leaves the git revision alone on a working-tree bump", () => {
				testInScope(() => {
					store.add({ path: "/test", displayName: "test" });
					store.bumpRevision("/test");
					expect(store.getRevision("/test")).toBe(1);
					expect(store.getGitRevision("/test")).toBe(0);
				});
			});

			it("bumps both counters on a git-state bump", () => {
				testInScope(() => {
					store.add({ path: "/test", displayName: "test" });
					store.bumpGitRevision("/test");
					expect(store.getGitRevision("/test")).toBe(1);
					expect(store.getRevision("/test")).toBe(1);
				});
			});

			it("drops the git revision when the repo is removed", () => {
				testInScope(() => {
					store.add({ path: "/test", displayName: "test" });
					store.bumpGitRevision("/test");
					store.remove("/test");
					expect(store.getGitRevision("/test")).toBe(0);
				});
			});
		});
	});

	describe("reorderTerminals()", () => {
		it("reorders terminals in a branch", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "test" });
				store.setWorkspace("/repo", "main");
				store.addTerminalToWorkspace("/repo", "main", "term-1");
				store.addTerminalToWorkspace("/repo", "main", "term-2");
				store.addTerminalToWorkspace("/repo", "main", "term-3");
				store.reorderTerminals("/repo", "main", 0, 2);
				expect(store.get("/repo")!.workspaces["main"].terminals).toEqual(["term-2", "term-3", "term-1"]);
			});
		});
	});

	describe("getActiveTerminals()", () => {
		it("returns terminals for active branch", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "test" });
				store.setActive("/repo");
				store.setWorkspace("/repo", "main");
				store.setActiveWorkspace("/repo", "main");
				store.addTerminalToWorkspace("/repo", "main", "term-1");
				expect(store.getActiveTerminals()).toEqual(["term-1"]);
			});
		});

		it("returns empty when no active repo", () => {
			testInScope(() => {
				expect(store.getActiveTerminals()).toEqual([]);
			});
		});
	});

	describe("groups — CRUD", () => {
		it("initializes with empty groups and groupOrder", () => {
			testInScope(() => {
				expect(store.state.groups).toEqual({});
				expect(store.state.groupOrder).toEqual([]);
			});
		});

		it("createGroup() adds a group and returns its ID", () => {
			testInScope(() => {
				const id = store.createGroup("Work")!;
				expect(id).toBeTruthy();
				expect(store.state.groups[id]).toBeDefined();
				expect(store.state.groups[id].name).toBe("Work");
				expect(store.state.groups[id].color).toBe("");
				expect(store.state.groups[id].collapsed).toBe(false);
				expect(store.state.groups[id].repoOrder).toEqual([]);
				expect(store.state.groupOrder).toContain(id);
			});
		});

		it("createGroup() enforces unique names (case-insensitive)", () => {
			testInScope(() => {
				store.createGroup("Work");
				expect(store.createGroup("work")).toBeNull();
				expect(store.createGroup("WORK")).toBeNull();
			});
		});

		it("deleteGroup() removes group and moves repos to ungrouped", () => {
			testInScope(() => {
				store.add({ path: "/repo-a", displayName: "A" });
				const id = store.createGroup("Work")!;
				store.addRepoToGroup("/repo-a", id);
				expect(store.state.repoOrder).not.toContain("/repo-a");
				store.deleteGroup(id);
				expect(store.state.groups[id]).toBeUndefined();
				expect(store.state.groupOrder).not.toContain(id);
				expect(store.state.repoOrder).toContain("/repo-a");
			});
		});

		it("renameGroup() updates name", () => {
			testInScope(() => {
				const id = store.createGroup("Work")!;
				expect(store.renameGroup(id, "Personal")).toBe(true);
				expect(store.state.groups[id].name).toBe("Personal");
			});
		});

		it("renameGroup() rejects duplicate names", () => {
			testInScope(() => {
				const id1 = store.createGroup("Work")!;
				store.createGroup("Personal");
				expect(store.renameGroup(id1, "personal")).toBe(false);
				expect(store.state.groups[id1].name).toBe("Work");
			});
		});

		it("setGroupColor() updates color", () => {
			testInScope(() => {
				const id = store.createGroup("Work")!;
				store.setGroupColor(id, "#4A9EFF");
				expect(store.state.groups[id].color).toBe("#4A9EFF");
			});
		});

		it("toggleGroupCollapsed() toggles collapsed flag", () => {
			testInScope(() => {
				const id = store.createGroup("Work")!;
				expect(store.state.groups[id].collapsed).toBe(false);
				store.toggleGroupCollapsed(id);
				expect(store.state.groups[id].collapsed).toBe(true);
				store.toggleGroupCollapsed(id);
				expect(store.state.groups[id].collapsed).toBe(false);
			});
		});

		it("hydrate() loads groups from backend", async () => {
			mockInvoke.mockResolvedValueOnce({
				repos: {},
				repoOrder: [],
				groups: {
					g1: { id: "g1", name: "Work", color: "#4A9EFF", collapsed: false, repoOrder: [] },
				},
				groupOrder: ["g1"],
			});
			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.state.groups["g1"]).toBeDefined();
				expect(store.state.groups["g1"].name).toBe("Work");
				expect(store.state.groupOrder).toEqual(["g1"]);
			});
		});

		it("hydrate() migration: missing groups field initializes empty", async () => {
			mockInvoke.mockResolvedValueOnce({
				repos: {
					"/repo": {
						path: "/repo",
						displayName: "test",
						initials: "TE",
						expanded: true,
						collapsed: false,
						workspaces: {
							main: { name: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
						},
						activeBranch: "main",
					},
				},
				repoOrder: ["/repo"],
				// No groups or groupOrder
			});
			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.state.groups).toEqual({});
				expect(store.state.groupOrder).toEqual([]);
				expect(store.state.repoOrder).toEqual(["/repo"]);
			});
		});

		it("persists groups via save", () => {
			testInScope(() => {
				const id = store.createGroup("Work")!;
				vi.advanceTimersByTime(500);
				const mutation = lastRepositoryMutation();
				const group = mutation.groups.find((entry) => entry.id === id)?.after as { name: string };
				expect(group.name).toBe("Work");
				expect(mutation.groupOrder?.after).toContain(id);
			});
		});
	});

	describe("groups — repo assignment", () => {
		it("addRepoToGroup() moves repo from ungrouped to group", () => {
			testInScope(() => {
				store.add({ path: "/repo-a", displayName: "A" });
				const gid = store.createGroup("Work")!;
				store.addRepoToGroup("/repo-a", gid);
				expect(store.state.groups[gid].repoOrder).toContain("/repo-a");
				expect(store.state.repoOrder).not.toContain("/repo-a");
			});
		});

		it("addRepoToGroup() moves repo from one group to another", () => {
			testInScope(() => {
				store.add({ path: "/repo-a", displayName: "A" });
				const g1 = store.createGroup("Work")!;
				const g2 = store.createGroup("Personal")!;
				store.addRepoToGroup("/repo-a", g1);
				store.addRepoToGroup("/repo-a", g2);
				expect(store.state.groups[g1].repoOrder).not.toContain("/repo-a");
				expect(store.state.groups[g2].repoOrder).toContain("/repo-a");
			});
		});

		it("removeRepoFromGroup() moves repo to ungrouped", () => {
			testInScope(() => {
				store.add({ path: "/repo-a", displayName: "A" });
				const gid = store.createGroup("Work")!;
				store.addRepoToGroup("/repo-a", gid);
				store.removeRepoFromGroup("/repo-a");
				expect(store.state.groups[gid].repoOrder).not.toContain("/repo-a");
				expect(store.state.repoOrder).toContain("/repo-a");
			});
		});

		it("remove() also cleans up group membership", () => {
			testInScope(() => {
				store.add({ path: "/repo-a", displayName: "A" });
				const gid = store.createGroup("Work")!;
				store.addRepoToGroup("/repo-a", gid);
				store.remove("/repo-a");
				expect(store.state.groups[gid].repoOrder).not.toContain("/repo-a");
			});
		});

		it("getGroupForRepo() returns correct group or undefined", () => {
			testInScope(() => {
				store.add({ path: "/repo-a", displayName: "A" });
				store.add({ path: "/repo-b", displayName: "B" });
				const gid = store.createGroup("Work")!;
				store.addRepoToGroup("/repo-a", gid);
				expect(store.getGroupForRepo("/repo-a")?.id).toBe(gid);
				expect(store.getGroupForRepo("/repo-b")).toBeUndefined();
			});
		});
	});

	describe("groups — reordering and layout", () => {
		it("reorderRepoInGroup() reorders within group", () => {
			testInScope(() => {
				store.add({ path: "/a", displayName: "A" });
				store.add({ path: "/b", displayName: "B" });
				store.add({ path: "/c", displayName: "C" });
				const gid = store.createGroup("Work")!;
				store.addRepoToGroup("/a", gid);
				store.addRepoToGroup("/b", gid);
				store.addRepoToGroup("/c", gid);
				store.reorderRepoInGroup(gid, 0, 2);
				expect(store.state.groups[gid].repoOrder).toEqual(["/b", "/c", "/a"]);
			});
		});

		it("moveRepoBetweenGroups() moves with correct index", () => {
			testInScope(() => {
				store.add({ path: "/a", displayName: "A" });
				store.add({ path: "/b", displayName: "B" });
				const g1 = store.createGroup("Work")!;
				const g2 = store.createGroup("Personal")!;
				store.addRepoToGroup("/a", g1);
				store.addRepoToGroup("/b", g2);
				store.moveRepoBetweenGroups("/a", g1, g2, 0);
				expect(store.state.groups[g1].repoOrder).toEqual([]);
				expect(store.state.groups[g2].repoOrder).toEqual(["/a", "/b"]);
			});
		});

		it("reorderGroups() reorders group display order", () => {
			testInScope(() => {
				const g1 = store.createGroup("A")!;
				const g2 = store.createGroup("B")!;
				const g3 = store.createGroup("C")!;
				expect(store.state.groupOrder).toEqual([g1, g2, g3]);
				store.reorderGroups(0, 2);
				expect(store.state.groupOrder).toEqual([g2, g3, g1]);
			});
		});

		it("getGroupedLayout() returns groups + ungrouped split", () => {
			testInScope(() => {
				store.add({ path: "/a", displayName: "A" });
				store.add({ path: "/b", displayName: "B" });
				store.add({ path: "/c", displayName: "C" });
				const gid = store.createGroup("Work")!;
				store.addRepoToGroup("/a", gid);
				store.addRepoToGroup("/b", gid);
				const layout = store.getGroupedLayout();
				expect(layout.groups).toHaveLength(1);
				expect(layout.groups[0].group.id).toBe(gid);
				expect(layout.groups[0].repos).toHaveLength(2);
				expect(layout.groups[0].repos[0].path).toBe("/a");
				expect(layout.ungrouped).toHaveLength(1);
				expect(layout.ungrouped[0].path).toBe("/c");
			});
		});

		/**
		 * Every consumer of this layout renders it through a reference-keyed
		 * `<For>`. Allocating a fresh wrapper and a fresh repos array on every
		 * call means every group header and every repo row is torn down and
		 * rebuilt whenever anything in the repo store moves — a branch poll, a
		 * file save, an unrelated repo's revision bump.
		 */
		it("getGroupedLayout() reuses the wrapper when nothing changed", () => {
			testInScope(() => {
				store.add({ path: "/a", displayName: "A" });
				store.add({ path: "/b", displayName: "B" });
				const gid = store.createGroup("Work")!;
				store.addRepoToGroup("/a", gid);

				const first = store.getGroupedLayout();
				const second = store.getGroupedLayout();
				expect(second.groups[0]).toBe(first.groups[0]);
				expect(second.groups[0].repos).toBe(first.groups[0].repos);
				expect(second.ungrouped).toBe(first.ungrouped);
			});
		});

		it("getGroupedLayout() builds a new wrapper when its repos change", () => {
			testInScope(() => {
				store.add({ path: "/a", displayName: "A" });
				store.add({ path: "/b", displayName: "B" });
				const gid = store.createGroup("Work")!;
				store.addRepoToGroup("/a", gid);
				const first = store.getGroupedLayout();

				store.addRepoToGroup("/b", gid);
				const second = store.getGroupedLayout();
				expect(second.groups[0]).not.toBe(first.groups[0]);
				expect(second.groups[0].repos.map((r) => r.path)).toEqual(["/a", "/b"]);
			});
		});

		it("getGroupedLayout() leaves an untouched group alone when another one changes", () => {
			testInScope(() => {
				store.add({ path: "/a", displayName: "A" });
				store.add({ path: "/b", displayName: "B" });
				const stable = store.createGroup("Stable")!;
				const moving = store.createGroup("Moving")!;
				store.addRepoToGroup("/a", stable);
				const first = store.getGroupedLayout();

				store.addRepoToGroup("/b", moving);
				const second = store.getGroupedLayout();
				expect(second.groups[0]).toBe(first.groups[0]);
				expect(second.groups[1]).not.toBe(first.groups[1]);
			});
		});

		it("getGroupedLayout() respects groupOrder and per-group repoOrder", () => {
			testInScope(() => {
				store.add({ path: "/a", displayName: "A" });
				store.add({ path: "/b", displayName: "B" });
				const g1 = store.createGroup("First")!;
				const g2 = store.createGroup("Second")!;
				store.addRepoToGroup("/a", g2);
				store.addRepoToGroup("/b", g1);
				const layout = store.getGroupedLayout();
				expect(layout.groups[0].group.name).toBe("First");
				expect(layout.groups[0].repos[0].path).toBe("/b");
				expect(layout.groups[1].group.name).toBe("Second");
				expect(layout.groups[1].repos[0].path).toBe("/a");
			});
		});
	});

	describe("park repos", () => {
		it("setPark() marks a repo as parked", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "test" });
				expect(store.get("/repo")!.parked).toBe(false);
				store.setPark("/repo", true);
				expect(store.get("/repo")!.parked).toBe(true);
			});
		});

		it("setPark(false) unparks a repo", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "test" });
				store.setPark("/repo", true);
				store.setPark("/repo", false);
				expect(store.get("/repo")!.parked).toBe(false);
			});
		});

		it("getParkedRepos() returns only parked repos", () => {
			testInScope(() => {
				store.add({ path: "/a", displayName: "A" });
				store.add({ path: "/b", displayName: "B" });
				store.add({ path: "/c", displayName: "C" });
				store.setPark("/b", true);
				const parked = store.getParkedRepos();
				expect(parked).toHaveLength(1);
				expect(parked[0].path).toBe("/b");
			});
		});

		it("getOrderedRepos() excludes parked repos", () => {
			testInScope(() => {
				store.add({ path: "/a", displayName: "A" });
				store.add({ path: "/b", displayName: "B" });
				store.setPark("/b", true);
				const ordered = store.getOrderedRepos();
				expect(ordered).toHaveLength(1);
				expect(ordered[0].path).toBe("/a");
			});
		});

		// #1358-caf5: scan/poll loops must use getActivePaths so parked repos
		// stay fully dormant (no git refresh, no PR polling, no plugin scans).
		it("getActivePaths() returns all paths when none are parked", () => {
			testInScope(() => {
				store.add({ path: "/a", displayName: "A" });
				store.add({ path: "/b", displayName: "B" });
				expect(store.getActivePaths().sort()).toEqual(["/a", "/b"]);
			});
		});

		it("getActivePaths() excludes parked repos (vs getPaths which keeps them)", () => {
			testInScope(() => {
				store.add({ path: "/a", displayName: "A" });
				store.add({ path: "/b", displayName: "B" });
				store.add({ path: "/c", displayName: "C" });
				store.setPark("/b", true);
				expect(store.getActivePaths().sort()).toEqual(["/a", "/c"]);
				// getPaths still returns parked entries (persistence/path-resolution
				// callers must keep seeing them).
				expect(store.getPaths().sort()).toEqual(["/a", "/b", "/c"]);
			});
		});

		it("setPark(true) calls stop_repo_watcher; setPark(false) calls start_repo_watcher", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "test" });
				mockInvoke.mockClear();
				store.setPark("/repo", true);
				expect(
					mockInvoke.mock.calls.some(
						([cmd, args]) => cmd === "stop_repo_watcher" && (args as { repoPath: string }).repoPath === "/repo",
					),
				).toBe(true);

				mockInvoke.mockClear();
				store.setPark("/repo", false);
				expect(
					mockInvoke.mock.calls.some(
						([cmd, args]) => cmd === "start_repo_watcher" && (args as { repoPath: string }).repoPath === "/repo",
					),
				).toBe(true);
			});
		});

		it("getGroupedLayout() excludes parked repos from groups and ungrouped", () => {
			testInScope(() => {
				store.add({ path: "/a", displayName: "A" });
				store.add({ path: "/b", displayName: "B" });
				store.add({ path: "/c", displayName: "C" });
				const gid = store.createGroup("Work")!;
				store.addRepoToGroup("/a", gid);
				store.addRepoToGroup("/b", gid);
				store.setPark("/b", true);
				store.setPark("/c", true);
				const layout = store.getGroupedLayout();
				expect(layout.groups[0].repos).toHaveLength(1);
				expect(layout.groups[0].repos[0].path).toBe("/a");
				expect(layout.ungrouped).toHaveLength(0);
			});
		});

		it("parked repos persist via save", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "test" });
				store.setPark("/repo", true);
				vi.advanceTimersByTime(500);
				const repo = lastRepositoryMutation().repos.find((mutation) => mutation.id === "/repo")?.after as {
					parked: boolean;
				};
				expect(repo.parked).toBe(true);
			});
		});

		it("hydrate() defaults parked to false when missing", async () => {
			mockInvoke.mockResolvedValueOnce({
				repos: {
					"/repo": {
						path: "/repo",
						displayName: "test",
						initials: "TE",
						expanded: true,
						collapsed: false,
						workspaces: {
							main: { name: "main", isMain: true, worktreePath: null, terminals: [], additions: 0, deletions: 0 },
						},
						activeBranch: "main",
					},
				},
				repoOrder: ["/repo"],
			});
			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.get("/repo")!.parked).toBe(false);
			});
		});
	});

	describe("hydrate guard", () => {
		it("blocks saves before hydrate completes", () => {
			testInScope(() => {
				store._testSetHydrated(false);
				store.add({ path: "/repo", displayName: "test" });
				vi.advanceTimersByTime(500);

				const saveCalls = mockInvoke.mock.calls.filter((c: unknown[]) => c[0] === "save_repositories").length;
				expect(saveCalls).toBe(0);

				store._testSetHydrated(true);
			});
		});

		it("allows saves after hydrate completes", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "test" });
				vi.advanceTimersByTime(500);

				const saveCalls = mockInvoke.mock.calls.filter((c: unknown[]) => c[0] === "save_repositories").length;
				expect(saveCalls).toBe(1);
			});
		});
	});

	describe("save debouncing", () => {
		it("coalesces rapid mutations into a single save call", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "test" });
				store.setWorkspace("/repo", "main");
				store.setWorkspace("/repo", "feature");
				store.toggleExpanded("/repo");

				// Before debounce fires, no save_repositories should have been called
				const saveCallsBefore = mockInvoke.mock.calls.filter((c: unknown[]) => c[0] === "save_repositories").length;
				expect(saveCallsBefore).toBe(0);

				// After debounce period, exactly one save call
				vi.advanceTimersByTime(500);
				const saveCallsAfter = mockInvoke.mock.calls.filter((c: unknown[]) => c[0] === "save_repositories").length;
				expect(saveCallsAfter).toBe(1);
			});
		});

		it("does not save for updateWorkspaceStats (ephemeral data)", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "test" });
				store.setWorkspace("/repo", "main");
				vi.advanceTimersByTime(500);
				mockInvoke.mockClear();

				store.updateWorkspaceStats("/repo", "main", 10, 5);
				vi.advanceTimersByTime(1000);

				expect(mockInvoke).not.toHaveBeenCalled();
			});
		});

		it("uses the last successful snapshot as the same-record expectation", async () => {
			await testInScopeAsync(async () => {
				store.add({ path: "/repo", displayName: "Original" });
				await vi.advanceTimersByTimeAsync(500);

				store.setDisplayName("/repo", "Renamed");
				await vi.advanceTimersByTimeAsync(500);

				const calls = mockInvoke.mock.calls.filter((call: unknown[]) => call[0] === "save_repositories");
				const second = (
					calls[1][1] as { config: { repos: Array<{ id: string; before: unknown; after: unknown }> } }
				).config.repos.find((mutation) => mutation.id === "/repo");
				expect(second?.before).toEqual(expect.objectContaining({ displayName: "Original" }));
				expect(second?.after).toEqual(expect.objectContaining({ displayName: "Renamed" }));
			});
		});

		it("surfaces a rejected repository mutation at error level", async () => {
			mockInvoke.mockImplementation((command: string) => {
				if (command === "save_repositories") {
					return Promise.reject(new Error("repository configuration conflict: repository '/repo' changed"));
				}
				return Promise.resolve(undefined);
			});
			const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				store.add({ path: "/repo", displayName: "Repo" });
				await vi.advanceTimersByTimeAsync(500);
				expect(errorSpy).toHaveBeenCalledWith("[store]", "Repository changes were not saved");
				expect(logger.getEntries()).toContainEqual(
					expect.objectContaining({
						message: "Repository changes were not saved",
						data: expect.objectContaining({ message: expect.stringContaining("repository configuration conflict") }),
					}),
				);
			});

			errorSpy.mockRestore();
		});

		it("does not discard a newer mutation queued behind a rejected save", async () => {
			let rejectFirst!: (reason?: unknown) => void;
			let saveCount = 0;
			mockInvoke.mockImplementation((command: string) => {
				if (command !== "save_repositories") return Promise.resolve(undefined);
				saveCount += 1;
				if (saveCount === 1) {
					return new Promise((_, reject) => {
						rejectFirst = reject;
					});
				}
				return Promise.resolve(undefined);
			});
			const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				store.add({ path: "/repo", displayName: "Original" });
				await vi.advanceTimersByTimeAsync(500);
				store.setDisplayName("/repo", "Queued");
				await vi.advanceTimersByTimeAsync(500);
				expect(saveCount).toBe(1);

				rejectFirst(new Error("repository configuration conflict"));
				await vi.advanceTimersByTimeAsync(0);

				// Position-independent on purpose: a conflict now also emits a rebased
				// retry, so the queued mutation is no longer call #2. What must hold is
				// that it reaches the backend at all.
				const queuedAfters = mockInvoke.mock.calls
					.filter((call: unknown[]) => call[0] === "save_repositories")
					.flatMap(
						(call: unknown[]) => (call[1] as { config: { repos: Array<{ id: string; after: unknown }> } }).config.repos,
					)
					.filter((mutation) => mutation.id === "/repo")
					.map((mutation) => mutation.after);
				expect(queuedAfters).toContainEqual(expect.objectContaining({ displayName: "Queued" }));
			});

			errorSpy.mockRestore();
		});

		it("rebases a conflicting mutation on the current disk state and retries it once", async () => {
			// Another client renamed /repo behind our back, so our `before` no longer
			// matches disk. The retry must carry disk's value as `before` and our value
			// as `after` — last-writer-wins for the record we meant to change.
			const onDisk = {
				repos: { "/repo": { path: "/repo", displayName: "RenamedElsewhere", workspaces: {} } },
				repoOrder: ["/repo"],
				activeRepoPath: null,
				groups: {},
				groupOrder: [],
			};
			let saveCount = 0;
			mockInvoke.mockImplementation((command: string) => {
				if (command === "load_repositories") return Promise.resolve(onDisk);
				if (command !== "save_repositories") return Promise.resolve(undefined);
				saveCount += 1;
				if (saveCount === 1) {
					return Promise.reject(
						new Error("repository configuration conflict: repository '/repo' changed in another window"),
					);
				}
				return Promise.resolve(undefined);
			});

			await testInScopeAsync(async () => {
				store.add({ path: "/repo", displayName: "Mine" });
				await vi.advanceTimersByTimeAsync(500);
				await vi.advanceTimersByTimeAsync(0);

				expect(saveCount).toBe(2);
				const retry = lastRepositoryMutation().repos.find((m) => m.id === "/repo");
				expect(retry?.before).toEqual(expect.objectContaining({ displayName: "RenamedElsewhere" }));
				expect(retry?.after).toEqual(expect.objectContaining({ displayName: "Mine" }));
			});
		});

		it("does not wedge future saves after a conflict", async () => {
			// The regression: a conflict left `persistedSnapshot` stale forever, so every
			// later save diffed against a baseline disk had already moved past and was
			// rejected too — permanently, for every repo, not just the conflicting one.
			const onDisk = {
				repos: { "/repo": { path: "/repo", displayName: "RenamedElsewhere", workspaces: {} } },
				repoOrder: ["/repo"],
				activeRepoPath: null,
				groups: {},
				groupOrder: [],
			};
			const conflicts: string[] = [];
			let saveCount = 0;
			mockInvoke.mockImplementation((command: string, args: unknown) => {
				if (command === "load_repositories") return Promise.resolve(onDisk);
				if (command !== "save_repositories") return Promise.resolve(undefined);
				saveCount += 1;
				const batch = (args as { config: { repos: Array<{ id: string; before: unknown }> } }).config;
				const mutation = batch.repos.find((m) => m.id === "/repo");
				// Emulate the backend CAS: reject unless `before` matches disk.
				if (
					mutation &&
					(mutation.before as { displayName?: string } | null)?.displayName !== onDisk.repos["/repo"].displayName
				) {
					conflicts.push(`save#${saveCount}`);
					return Promise.reject(new Error("repository configuration conflict: repository '/repo' changed"));
				}
				return Promise.resolve(undefined);
			});
			const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});

			await testInScopeAsync(async () => {
				store.add({ path: "/repo", displayName: "Mine" });
				await vi.advanceTimersByTimeAsync(500);
				await vi.advanceTimersByTimeAsync(0);
				const afterFirst = saveCount;

				// A completely unrelated, later user edit must still reach disk.
				store.setDisplayName("/repo", "MineAgain");
				await vi.advanceTimersByTimeAsync(500);
				await vi.advanceTimersByTimeAsync(0);

				expect(saveCount).toBeGreaterThan(afterFirst);
				// The last save attempted must have been accepted, not rejected.
				expect(conflicts).not.toContain(`save#${saveCount}`);
			});

			errorSpy.mockRestore();
		});
	});

	describe("findOwnerForTerminal()", () => {
		it("returns repo and branch for a registered terminal", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "Repo" });
				store.setWorkspace("/repo", "main", { worktreePath: "/repo" });
				store.addTerminalToWorkspace("/repo", "main", "term-1");

				const owner = store.findOwnerForTerminal("term-1");
				expect(owner).toEqual({ repoPath: "/repo", workspaceId: "main" });
			});
		});

		it("returns null for an unknown terminal", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "Repo" });
				store.setWorkspace("/repo", "main", { worktreePath: "/repo" });

				expect(store.findOwnerForTerminal("unknown")).toBeNull();
			});
		});

		it("finds the correct branch when multiple branches exist", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "Repo" });
				store.setWorkspace("/repo", "main", { worktreePath: "/repo" });
				store.setWorkspace("/repo", "feature", { worktreePath: "/repo-feat" });
				store.addTerminalToWorkspace("/repo", "main", "term-main");
				store.addTerminalToWorkspace("/repo", "feature", "term-feat");

				expect(store.findOwnerForTerminal("term-main")).toEqual({ repoPath: "/repo", workspaceId: "main" });
				expect(store.findOwnerForTerminal("term-feat")).toEqual({ repoPath: "/repo", workspaceId: "feature" });
			});
		});

		it("returns null when terminal was removed from branch", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "Repo" });
				store.setWorkspace("/repo", "main", { worktreePath: "/repo" });
				store.addTerminalToWorkspace("/repo", "main", "term-1");
				store.removeTerminalFromWorkspace("/repo", "main", "term-1");

				expect(store.findOwnerForTerminal("term-1")).toBeNull();
			});
		});

		it("works across multiple repos", () => {
			testInScope(() => {
				store.add({ path: "/repo-a", displayName: "A" });
				store.setWorkspace("/repo-a", "main", { worktreePath: "/repo-a" });
				store.addTerminalToWorkspace("/repo-a", "main", "term-a");

				store.add({ path: "/repo-b", displayName: "B" });
				store.setWorkspace("/repo-b", "dev", { worktreePath: "/repo-b" });
				store.addTerminalToWorkspace("/repo-b", "dev", "term-b");

				expect(store.findOwnerForTerminal("term-a")).toEqual({ repoPath: "/repo-a", workspaceId: "main" });
				expect(store.findOwnerForTerminal("term-b")).toEqual({ repoPath: "/repo-b", workspaceId: "dev" });
			});
		});
	});

	describe("getAllReposOrdered()", () => {
		it("includes repos nested in groups, not just ungrouped (#64)", () => {
			testInScope(() => {
				store.add({ path: "/repo-a", displayName: "A" });
				store.add({ path: "/repo-b", displayName: "B" });
				store.add({ path: "/repo-c", displayName: "C" });

				const gid = store.createGroup("Group 1") as string;
				store.addRepoToGroup("/repo-b", gid);
				store.addRepoToGroup("/repo-c", gid);

				// ungrouped first (a), then the group's repos in order (b, c)
				expect(store.getAllReposOrdered().map((r) => r.path)).toEqual(["/repo-a", "/repo-b", "/repo-c"]);
			});
		});

		it("returns every repo even when all are grouped (#64)", () => {
			testInScope(() => {
				store.add({ path: "/repo-a", displayName: "A" });
				store.add({ path: "/repo-b", displayName: "B" });

				const gid = store.createGroup("Group 1") as string;
				store.addRepoToGroup("/repo-a", gid);
				store.addRepoToGroup("/repo-b", gid);

				// repoOrder is empty, but the repos still surface via the group
				expect(store.state.repoOrder).toEqual([]);
				expect(store.getAllReposOrdered().map((r) => r.path)).toEqual(["/repo-a", "/repo-b"]);
			});
		});
	});

	// #763-d219 — the classifier itself lives in Rust (`config.rs`); this store
	// only fetches its verdict and quarantines the named paths, so these tests
	// drive `refreshStaleTempCandidates`/`repairStaleTemp` through a mocked
	// `invoke` rather than asserting on any frontend-side classification logic.
	describe("stale-temp repositories (#763-d219)", () => {
		it("refreshStaleTempCandidates() populates the candidate list from the backend", async () => {
			await testInScopeAsync(async () => {
				mockInvoke.mockImplementationOnce((cmd: string) => {
					if (cmd === "list_stale_temp_repository_candidates") {
						return Promise.resolve([{ path: "/tmp/ghost", displayName: "ghost" }]);
					}
					return Promise.resolve(undefined);
				});
				await store.refreshStaleTempCandidates();
				expect(store.getStaleTempCandidates()).toEqual([{ path: "/tmp/ghost", displayName: "ghost" }]);
			});
		});

		it("logs via appLogger and leaves the list untouched when the backend call fails", async () => {
			const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
			await testInScopeAsync(async () => {
				mockInvoke.mockImplementationOnce((cmd: string) => {
					if (cmd === "list_stale_temp_repository_candidates") return Promise.reject(new Error("boom"));
					return Promise.resolve(undefined);
				});
				await store.refreshStaleTempCandidates();
				expect(store.getStaleTempCandidates()).toEqual([]);
				expect(errorSpy).toHaveBeenCalledWith("[store]", "Failed to list stale-temp repository candidates");
				expect(logger.getEntries()).toContainEqual(
					expect.objectContaining({
						message: "Failed to list stale-temp repository candidates",
						data: expect.any(Error),
					}),
				);
			});
			errorSpy.mockRestore();
		});

		it("getGroupedLayout() and getOrderedRepos() quarantine classified candidates out of the sidebar", async () => {
			await testInScopeAsync(async () => {
				store.add({ path: "/legit", displayName: "Legit" });
				store.add({ path: "/tmp/ghost", displayName: "ghost-fallback" });
				mockInvoke.mockImplementationOnce((cmd: string) => {
					if (cmd === "list_stale_temp_repository_candidates") {
						return Promise.resolve([{ path: "/tmp/ghost", displayName: "ghost-fallback" }]);
					}
					return Promise.resolve(undefined);
				});
				await store.refreshStaleTempCandidates();

				expect(store.getOrderedRepos().map((r) => r.path)).toEqual(["/legit"]);
				const layout = store.getGroupedLayout();
				expect(layout.ungrouped.map((r) => r.path)).toEqual(["/legit"]);

				// The repo record itself is untouched — quarantine hides it from
				// these two listings only, it never deletes anything.
				expect(store.get("/tmp/ghost")).toBeDefined();
			});
		});

		it("repairStaleTemp() sends exactly the given paths and drops only the removed ones from the candidate list", async () => {
			await testInScopeAsync(async () => {
				mockInvoke.mockImplementationOnce((cmd: string) => {
					if (cmd === "list_stale_temp_repository_candidates") {
						return Promise.resolve([
							{ path: "/tmp/ghost-1", displayName: "ghost-1" },
							{ path: "/tmp/ghost-2", displayName: "ghost-2" },
						]);
					}
					return Promise.resolve(undefined);
				});
				await store.refreshStaleTempCandidates();

				mockInvoke.mockImplementationOnce((cmd: string, args: unknown) => {
					if (cmd === "repair_stale_temp_repositories") {
						expect(args).toEqual({ paths: ["/tmp/ghost-1"] });
						return Promise.resolve({
							removed: ["/tmp/ghost-1"],
							backupPath: "/config/repositories.repair-backup-x.json",
						});
					}
					return Promise.resolve(undefined);
				});
				const summary = await store.repairStaleTemp(["/tmp/ghost-1"]);

				expect(summary.removed).toEqual(["/tmp/ghost-1"]);
				expect(store.getStaleTempCandidates().map((c) => c.path)).toEqual(["/tmp/ghost-2"]);
			});
		});

		it("repairStaleTemp() propagates a backend refusal without touching the candidate list", async () => {
			await testInScopeAsync(async () => {
				mockInvoke.mockImplementationOnce((cmd: string) => {
					if (cmd === "list_stale_temp_repository_candidates") {
						return Promise.resolve([{ path: "/tmp/ghost-1", displayName: "ghost-1" }]);
					}
					return Promise.resolve(undefined);
				});
				await store.refreshStaleTempCandidates();

				mockInvoke.mockImplementationOnce((cmd: string) => {
					if (cmd === "repair_stale_temp_repositories") {
						return Promise.reject(new Error("no longer a stale-temp candidate on disk"));
					}
					return Promise.resolve(undefined);
				});

				await expect(store.repairStaleTemp(["/tmp/ghost-1"])).rejects.toThrow(
					"no longer a stale-temp candidate on disk",
				);
				expect(store.getStaleTempCandidates().map((c) => c.path)).toEqual(["/tmp/ghost-1"]);
			});
		});
	});

	describe("placementWorkspaceFor()", () => {
		const rootOwner = { repoPath: "/repo", workspaceId: null };

		it("places a root session in the root workspace even while a linked worktree is active", async () => {
			const { placementWorkspaceFor } = await import("../../stores/repositories");
			testInScope(() => {
				store.add({ path: "/repo", displayName: "test" });
				store.setWorkspace("/repo", "main", { worktreePath: "/repo" });
				store.setWorkspace("/repo", "feature", { worktreePath: "/repo__wt/feature" });
				store.setActiveWorkspace("/repo", "feature");
				expect(placementWorkspaceFor(rootOwner)).toBe("main");
			});
		});

		it("matches the root workspace across a trailing slash and Windows separators", async () => {
			const { placementWorkspaceFor } = await import("../../stores/repositories");
			testInScope(() => {
				store.add({ path: "C:\\repo", displayName: "test" });
				store.setWorkspace("C:\\repo", "main", { worktreePath: "C:/repo/" });
				store.setWorkspace("C:\\repo", "feature", { worktreePath: "C:/repo__wt/feature" });
				store.setActiveWorkspace("C:\\repo", "feature");
				expect(placementWorkspaceFor({ repoPath: "C:\\repo", workspaceId: null })).toBe("main");
			});
		});

		it("falls back to the active workspace when no workspace records the root", async () => {
			const { placementWorkspaceFor } = await import("../../stores/repositories");
			testInScope(() => {
				store.add({ path: "/repo", displayName: "test" });
				store.setWorkspace("/repo", "feature", { worktreePath: "/repo__wt/feature" });
				store.setActiveWorkspace("/repo", "feature");
				expect(placementWorkspaceFor(rootOwner)).toBe("feature");
			});
		});

		it("returns null when neither a root workspace nor an active one exists", async () => {
			const { placementWorkspaceFor } = await import("../../stores/repositories");
			testInScope(() => {
				store.add({ path: "/repo", displayName: "test" });
				store.setWorkspace("/repo", "feature", { worktreePath: "/repo__wt/feature" });
				store.setActiveWorkspace("/repo", null);
				expect(placementWorkspaceFor(rootOwner)).toBeNull();
			});
		});

		it("keeps an owner that already names its workspace", async () => {
			const { placementWorkspaceFor } = await import("../../stores/repositories");
			testInScope(() => {
				store.add({ path: "/repo", displayName: "test" });
				store.setWorkspace("/repo", "main", { worktreePath: "/repo" });
				expect(placementWorkspaceFor({ repoPath: "/repo", workspaceId: "feature" })).toBe("feature");
			});
		});
	});
});
