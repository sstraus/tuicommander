import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { testInScope } from "../helpers/store";

const mockInvoke = vi.fn().mockResolvedValue(undefined);
vi.mock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));

describe("mdTabsStore", () => {
	let store: typeof import("../../stores/mdTabs").mdTabsStore;
	let uiStore: typeof import("../../stores/ui").uiStore;
	let repositoriesStore: typeof import("../../stores/repositories").repositoriesStore;
	let resolveRepoForCwd: typeof import("../../stores/mdTabs").resolveRepoForCwd;

	beforeEach(async () => {
		vi.resetModules();
		mockInvoke.mockReset().mockResolvedValue(undefined);
		vi.doMock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));
		const mdTabsMod = await import("../../stores/mdTabs");
		store = mdTabsMod.mdTabsStore;
		resolveRepoForCwd = mdTabsMod.resolveRepoForCwd;
		uiStore = (await import("../../stores/ui")).uiStore;
		repositoriesStore = (await import("../../stores/repositories")).repositoriesStore;
		repositoriesStore._testSetHydrated(true);
	});

	afterEach(() => {
		repositoriesStore._testCancelPendingSave();
	});

	describe("addTasks()", () => {
		// Catches: a second Tasks tab per action call.
		it("reuses the one Tasks tab and activates it", () => {
			testInScope(() => {
				const first = store.addTasks();
				const other = store.add("/repo", "docs/README.md");
				expect(store.state.activeId).toBe(other);
				expect(store.addTasks()).toBe(first);
				expect(store.state.activeId).toBe(first);
				expect(Object.values(store.state.tabs).filter((tab) => tab.type === "tasks")).toHaveLength(1);
			});
		});
	});

	describe("add()", () => {
		it("adds a markdown tab and sets it active", () => {
			testInScope(() => {
				const id = store.add("/repo", "docs/README.md");
				expect(id).toBe("md-1");
				expect(store.state.activeId).toBe(id);
				const tab = store.get(id);
				expect(tab?.type).toBe("file");
				if (tab?.type === "file") {
					expect(tab.filePath).toBe("docs/README.md");
					expect(tab.fileName).toBe("README.md");
				}
			});
		});

		it("deduplicates by repoPath + filePath", () => {
			testInScope(() => {
				const id1 = store.add("/repo", "README.md");
				const id2 = store.add("/repo", "README.md");
				expect(id2).toBe(id1);
				expect(store.getCount()).toBe(1);
			});
		});

		it("allows same file from different repos", () => {
			testInScope(() => {
				const id1 = store.add("/repo1", "README.md");
				const id2 = store.add("/repo2", "README.md");
				expect(id1).not.toBe(id2);
				expect(store.getCount()).toBe(2);
			});
		});

		it("stores fsRoot when provided (worktree path)", () => {
			testInScope(() => {
				const id = store.add("/repo", "README.md", "/repo/.worktrees/feature");
				const tab = store.get(id);
				expect(tab?.type).toBe("file");
				if (tab?.type === "file") {
					expect(tab.repoPath).toBe("/repo");
					expect(tab.fsRoot).toBe("/repo/.worktrees/feature");
				}
			});
		});

		it("defaults fsRoot to repoPath when not provided", () => {
			testInScope(() => {
				const id = store.add("/repo", "README.md");
				const tab = store.get(id);
				if (tab?.type === "file") {
					expect(tab.fsRoot).toBe("/repo");
				}
			});
		});

		it("deduplicates by fsRoot + filePath (same repo, different worktrees)", () => {
			testInScope(() => {
				const id1 = store.add("/repo", "README.md", "/repo/.worktrees/feat-a");
				const id2 = store.add("/repo", "README.md", "/repo/.worktrees/feat-b");
				expect(id1).not.toBe(id2);
				expect(store.getCount()).toBe(2);
			});
		});

		it("deduplicates same worktree + filePath", () => {
			testInScope(() => {
				const id1 = store.add("/repo", "README.md", "/repo/.worktrees/feat-a");
				const id2 = store.add("/repo", "README.md", "/repo/.worktrees/feat-a");
				expect(id2).toBe(id1);
				expect(store.getCount()).toBe(1);
			});
		});
	});

	// -------------------------------------------------------------------------
	// Virtual tabs
	// -------------------------------------------------------------------------
	describe("addVirtual()", () => {
		it("adds a virtual tab and sets it active", () => {
			testInScope(() => {
				const id = store.addVirtual("Active Plan", "plan:file?path=/foo.md");
				expect(store.state.activeId).toBe(id);
				const tab = store.get(id);
				expect(tab?.type).toBe("virtual");
				if (tab?.type === "virtual") {
					expect(tab.title).toBe("Active Plan");
					expect(tab.contentUri).toBe("plan:file?path=/foo.md");
				}
			});
		});

		it("deduplicates by contentUri", () => {
			testInScope(() => {
				const id1 = store.addVirtual("Plan", "plan:file?path=/foo.md");
				const id2 = store.addVirtual("Plan", "plan:file?path=/foo.md");
				expect(id2).toBe(id1);
				expect(store.getCount()).toBe(1);
			});
		});

		it("allows different contentUris as separate tabs", () => {
			testInScope(() => {
				const id1 = store.addVirtual("Plan A", "plan:file?path=/a.md");
				const id2 = store.addVirtual("Plan B", "plan:file?path=/b.md");
				expect(id1).not.toBe(id2);
				expect(store.getCount()).toBe(2);
			});
		});

		it("can coexist with file tabs", () => {
			testInScope(() => {
				store.add("/repo", "a.md");
				store.addVirtual("Stories", "stories:detail?id=1");
				expect(store.getCount()).toBe(2);
			});
		});

		it("sets repoPath on virtual tab when provided", () => {
			testInScope(() => {
				const id = store.addVirtual("Plan", "plan:file?path=/repo1/plan.md", "/repo1");
				const tab = store.get(id);
				expect(tab?.repoPath).toBe("/repo1");
			});
		});

		it("virtual tab without repoPath has no repoPath field", () => {
			testInScope(() => {
				const id = store.addVirtual("Global", "stories:detail?id=1");
				const tab = store.get(id);
				expect(tab?.repoPath).toBeUndefined();
			});
		});
	});

	// -------------------------------------------------------------------------
	// remove()
	// -------------------------------------------------------------------------
	describe("remove()", () => {
		it("removes the active tab without promoting another (caller selects the replacement)", () => {
			testInScope(() => {
				const id1 = store.add("/repo", "a.md");
				const id2 = store.add("/repo", "b.md");
				store.remove(id2);
				expect(store.get(id2)).toBeUndefined();
				expect(store.get(id1)).toBeDefined();
				// No auto-promotion: id1 could be scoped to another branch/repo —
				// visibility-aware selection happens in useTerminalLifecycle.
				expect(store.state.activeId).toBeNull();
			});
		});

		it("clears activeId when last tab removed", () => {
			testInScope(() => {
				const id = store.add("/repo", "a.md");
				store.remove(id);
				expect(store.state.activeId).toBeNull();
			});
		});

		it("removes virtual tabs correctly", () => {
			testInScope(() => {
				const id = store.addVirtual("Plan", "plan:x");
				store.remove(id);
				expect(store.get(id)).toBeUndefined();
			});
		});
	});

	// -------------------------------------------------------------------------
	// clearForRepo()
	// -------------------------------------------------------------------------
	describe("clearForRepo()", () => {
		it("removes only tabs for specified repo", () => {
			testInScope(() => {
				store.add("/repo1", "a.md");
				const id2 = store.add("/repo2", "b.md");
				store.clearForRepo("/repo1");
				expect(store.getCount()).toBe(1);
				expect(store.get(id2)).toBeDefined();
			});
		});

		it("clears activeId when active tab is removed", () => {
			testInScope(() => {
				store.add("/repo1", "a.md");
				store.clearForRepo("/repo1");
				expect(store.state.activeId).toBeNull();
			});
		});

		it("keeps activeId when active tab is NOT removed", () => {
			testInScope(() => {
				store.add("/repo1", "a.md");
				const id2 = store.add("/repo2", "b.md");
				expect(store.state.activeId).toBe(id2);
				store.clearForRepo("/repo1");
				expect(store.state.activeId).toBe(id2);
			});
		});

		it("does not remove virtual tabs when clearing a repo", () => {
			testInScope(() => {
				store.add("/repo1", "a.md");
				const vId = store.addVirtual("Stories", "stories:x");
				store.clearForRepo("/repo1");
				expect(store.get(vId)).toBeDefined();
			});
		});
	});

	// -------------------------------------------------------------------------
	// clearAll()
	// -------------------------------------------------------------------------
	describe("clearAll()", () => {
		it("removes all tabs including virtual", () => {
			testInScope(() => {
				store.add("/repo", "a.md");
				store.add("/repo", "b.md");
				store.addVirtual("Plan", "plan:x");
				store.clearAll();
				expect(store.getCount()).toBe(0);
				expect(store.state.activeId).toBeNull();
			});
		});
	});

	// -------------------------------------------------------------------------
	// getForRepo()
	// -------------------------------------------------------------------------
	describe("getForRepo()", () => {
		it("filters tabs by repo, excludes virtual tabs", () => {
			testInScope(() => {
				store.add("/repo1", "a.md");
				store.add("/repo1", "b.md");
				store.add("/repo2", "c.md");
				store.addVirtual("Plan", "plan:x");
				expect(store.getForRepo("/repo1")).toHaveLength(2);
				expect(store.getForRepo("/repo2")).toHaveLength(1);
			});
		});
	});

	// -------------------------------------------------------------------------
	// Auto-show markdown panel
	// -------------------------------------------------------------------------
	// -------------------------------------------------------------------------
	// PR Diff tabs
	// -------------------------------------------------------------------------
	describe("addPrDiff()", () => {
		it("adds a pr-diff tab and sets it active", () => {
			testInScope(() => {
				const id = store.addPrDiff("/repo", 42, "Fix bug", "diff --git a/f.ts b/f.ts\n-old\n+new");
				expect(store.state.activeId).toBe(id);
				const tab = store.get(id);
				expect(tab?.type).toBe("pr-diff");
				if (tab?.type === "pr-diff") {
					expect(tab.prNumber).toBe(42);
					expect(tab.prTitle).toBe("Fix bug");
					expect(tab.diff).toContain("diff --git");
				}
			});
		});

		it("deduplicates by repoPath + prNumber", () => {
			testInScope(() => {
				const id1 = store.addPrDiff("/repo", 42, "Fix bug", "diff1");
				const id2 = store.addPrDiff("/repo", 42, "Fix bug", "diff2");
				expect(id2).toBe(id1);
				expect(store.getCount()).toBe(1);
			});
		});

		it("allows different PR numbers as separate tabs", () => {
			testInScope(() => {
				const id1 = store.addPrDiff("/repo", 42, "Fix A", "diff1");
				const id2 = store.addPrDiff("/repo", 43, "Fix B", "diff2");
				expect(id1).not.toBe(id2);
				expect(store.getCount()).toBe(2);
			});
		});

		it("updates diff content when reopening same PR", () => {
			testInScope(() => {
				store.addPrDiff("/repo", 42, "Fix bug", "old diff");
				const id = store.addPrDiff("/repo", 42, "Fix bug", "new diff");
				const tab = store.get(id);
				if (tab?.type === "pr-diff") {
					expect(tab.diff).toBe("new diff");
				}
			});
		});
	});

	describe("does not auto-show markdown file browser", () => {
		it("does not open file browser when adding a file tab", () => {
			testInScope(() => {
				expect(uiStore.state.markdownPanelVisible).toBe(false);
				store.add("/repo", "README.md");
				expect(uiStore.state.markdownPanelVisible).toBe(false);
			});
		});

		it("does not open file browser when adding a virtual tab", () => {
			testInScope(() => {
				expect(uiStore.state.markdownPanelVisible).toBe(false);
				store.addVirtual("Plan", "plan:file?path=/foo.md");
				expect(uiStore.state.markdownPanelVisible).toBe(false);
			});
		});

		it("does not open file browser when adding claude usage tab", () => {
			testInScope(() => {
				expect(uiStore.state.markdownPanelVisible).toBe(false);
				store.addClaudeUsage();
				expect(uiStore.state.markdownPanelVisible).toBe(false);
			});
		});

		it("updates the singleton Claude dashboard when opened from another session", () => {
			testInScope(() => {
				const id = store.addClaudeUsage("private-session");
				expect(store.get(id)).toMatchObject({ type: "claude-usage", sessionId: "private-session" });
				expect(store.addClaudeUsage("default-session")).toBe(id);
				expect(store.get(id)).toMatchObject({ type: "claude-usage", sessionId: "default-session" });
			});
		});
	});

	describe("openUiTab()", () => {
		it("replaces URL content with HTML and HTML content with a URL", () => {
			testInScope(() => {
				const id = store.openUiTab("dashboard", "Dashboard", "", true, "https://example.test/one");
				store.openUiTab("dashboard", "Notice", "<p>Paused</p>", true, undefined, false);
				const htmlTab = store.get(id);
				expect(htmlTab?.type).toBe("plugin-panel");
				if (htmlTab?.type !== "plugin-panel") return;
				expect(htmlTab.url).toBeUndefined();
				expect(htmlTab.html).toBe("<p>Paused</p>");
				store.openUiTab("dashboard", "Dashboard", "<p>stale</p>", true, "https://example.test/two", false);
				const urlTab = store.get(id);
				if (urlTab?.type !== "plugin-panel") return;
				expect(urlTab.url).toBe("https://example.test/two");
				expect(urlTab.html).toBe("");
			});
		});
		// catches: a focused model tab in another repo leaves Back pointing one step
		// past the terminal Boss was working in; a same-repo open must add nothing
		it("records a back step only when a focused open changes the repository", async () => {
			const terminals = (await import("../../stores/terminals")).terminalsStore;
			const { makeTerminal } = await import("../helpers/store");
			testInScope(() => {
				repositoriesStore.add({ path: "/Gits/alpha", displayName: "alpha" });
				repositoriesStore.add({ path: "/Gits/beta", displayName: "beta" });
				repositoriesStore.setActive("/Gits/alpha");
				const older = terminals.add(makeTerminal({ name: "older" }));
				terminals.setActive(older);
				const working = terminals.add(makeTerminal({ name: "working" }));
				terminals.setActive(working);
				store.openUiTab("same", "Same", "<p>a</p>", false, undefined, true, "/Gits/alpha");
				expect(terminals.getHistoryTarget("back")).toBe(older);
				store.openUiTab("report", "Report", "<p>b</p>", false, undefined, true, "/Gits/beta");
				expect(repositoriesStore.state.activeRepoPath).toBe("/Gits/beta");
				expect(terminals.getHistoryTarget("back")).toBe(working);
			});
		});
		it("creates a new plugin-panel tab with the given id", () => {
			testInScope(() => {
				const tabId = store.openUiTab("wiz-coverage", "Coverage Report", "<h1>Report</h1>", true);
				expect(tabId).toBeTruthy();
				expect(store.state.activeId).toBe(tabId);
				const tab = store.get(tabId);
				expect(tab?.type).toBe("plugin-panel");
				if (tab?.type === "plugin-panel") {
					expect(tab.pluginId).toBe("wiz-coverage");
					expect(tab.title).toBe("Coverage Report");
					expect(tab.html).toBe("<h1>Report</h1>");
					expect(tab.pinned).toBe(true);
				}
			});
		});

		it("deduplicates by pluginId — same id reuses existing tab", () => {
			testInScope(() => {
				const id1 = store.openUiTab("wiz-report", "Report v1", "<p>v1</p>", true);
				const id2 = store.openUiTab("wiz-report", "Report v2", "<p>v2</p>", true);
				expect(id1).toBe(id2);
				expect(store.getCount()).toBe(1);
				const tab = store.get(id1);
				if (tab?.type === "plugin-panel") {
					expect(tab.title).toBe("Report v2");
					expect(tab.html).toBe("<p>v2</p>");
				}
			});
		});

		it("respects pinned=false", () => {
			testInScope(() => {
				const tabId = store.openUiTab("wiz-temp", "Temp", "<p>temp</p>", false);
				const tab = store.get(tabId);
				expect(tab?.pinned).toBe(false);
			});
		});

		it("different ids create separate tabs", () => {
			testInScope(() => {
				store.openUiTab("wiz-a", "Tab A", "<p>A</p>", true);
				store.openUiTab("wiz-b", "Tab B", "<p>B</p>", true);
				expect(store.getCount()).toBe(2);
			});
		});

		it("focus=true (default) sets the new tab as active", () => {
			testInScope(() => {
				const id = store.openUiTab("wiz-x", "X", "<p/>", true, undefined, true);
				expect(store.state.activeId).toBe(id);
			});
		});

		it("focus=false does not change activeId when creating a new tab", () => {
			testInScope(() => {
				const existing = store.add("/repo", "README.md");
				expect(store.state.activeId).toBe(existing);
				store.openUiTab("wiz-bg", "BG", "<p/>", true, undefined, false);
				expect(store.state.activeId).toBe(existing);
			});
		});

		it("focus=false does not change activeId when updating an existing tab", () => {
			testInScope(() => {
				store.openUiTab("wiz-mc", "MC v1", "<p>v1</p>", true);
				const other = store.add("/repo", "README.md");
				expect(store.state.activeId).toBe(other);
				store.openUiTab("wiz-mc", "MC v2", "<p>v2</p>", true, undefined, false);
				expect(store.state.activeId).toBe(other);
				// content still updated
				const tab = store.get(store.getIds().find((id) => store.get(id)?.type === "plugin-panel")!);
				if (tab?.type === "plugin-panel") expect(tab.html).toBe("<p>v2</p>");
			});
		});
	});

	describe("resolveRepoForCwd()", () => {
		it("returns null for null/undefined/empty input", () => {
			testInScope(() => {
				expect(resolveRepoForCwd(null)).toBeNull();
				expect(resolveRepoForCwd(undefined)).toBeNull();
				expect(resolveRepoForCwd("")).toBeNull();
			});
		});

		it("returns null when no repos registered", () => {
			testInScope(() => {
				expect(resolveRepoForCwd("/some/path")).toBeNull();
			});
		});

		it("returns exact match when cwd equals a registered repo path", () => {
			testInScope(() => {
				repositoriesStore.add({ path: "/Gits/alpha", displayName: "alpha" });
				repositoriesStore.add({ path: "/Gits/beta", displayName: "beta" });
				expect(resolveRepoForCwd("/Gits/alpha")).toBe("/Gits/alpha");
			});
		});

		it("returns the prefix-matching repo when cwd is nested inside it", () => {
			testInScope(() => {
				repositoriesStore.add({ path: "/Gits/alpha", displayName: "alpha" });
				expect(resolveRepoForCwd("/Gits/alpha/src/lib/foo.rs")).toBe("/Gits/alpha");
			});
		});

		it("picks the longest-prefix match for nested repos", () => {
			testInScope(() => {
				repositoriesStore.add({ path: "/Gits", displayName: "root" });
				repositoriesStore.add({ path: "/Gits/alpha", displayName: "alpha" });
				expect(resolveRepoForCwd("/Gits/alpha/src/main.rs")).toBe("/Gits/alpha");
			});
		});

		it("does not match a path that merely shares a name prefix with a repo", () => {
			testInScope(() => {
				repositoriesStore.add({ path: "/Gits/alpha", displayName: "alpha" });
				// /Gits/alpha-sibling must NOT match /Gits/alpha
				expect(resolveRepoForCwd("/Gits/alpha-sibling/src")).toBeNull();
			});
		});

		it("resolves Windows drive-letter CWD to registered Windows repo", () => {
			testInScope(() => {
				repositoriesStore.add({ path: "C:\\DATA\\repos\\arcane", displayName: "arcane" });
				expect(resolveRepoForCwd("C:\\DATA\\repos\\arcane\\src")).toBe("C:\\DATA\\repos\\arcane");
			});
		});

		it("does not match Windows path that merely shares a name prefix", () => {
			testInScope(() => {
				repositoriesStore.add({ path: "C:\\DATA\\repos\\arcane", displayName: "arcane" });
				expect(resolveRepoForCwd("C:\\DATA\\repos\\arcane-fork\\src")).toBeNull();
			});
		});

		it("matches mixed separators (forward slash CWD against backslash repo)", () => {
			testInScope(() => {
				repositoriesStore.add({ path: "C:\\DATA\\repos\\arcane", displayName: "arcane" });
				expect(resolveRepoForCwd("C:/DATA/repos/arcane/src/lib")).toBe("C:\\DATA\\repos\\arcane");
			});
		});

		it("returns exact match for Windows drive-letter repo", () => {
			testInScope(() => {
				repositoriesStore.add({ path: "C:\\DATA\\repos\\arcane", displayName: "arcane" });
				expect(resolveRepoForCwd("C:\\DATA\\repos\\arcane")).toBe("C:\\DATA\\repos\\arcane");
			});
		});
	});

	describe("file tabs are scoped to their OWN repo, never the focused one", () => {
		/** Register a repo with one branch checked out, so branchKeyFor() can answer. */
		const addRepoWithBranch = (path: string, branch: string) => {
			repositoriesStore.add({ path, displayName: path });
			repositoriesStore.setWorkspace(path, branch, { worktreePath: path });
			repositoriesStore.setActiveWorkspace(path, branch);
		};

		it("stamps branchKey from the tab's repo, not from the active repo", () => {
			testInScope(() => {
				addRepoWithBranch("/Gits/alpha", "main");
				addRepoWithBranch("/Gits/beta", "develop");
				repositoriesStore.setActive("/Gits/beta");

				const id = store.add("/Gits/alpha", "README.md");
				expect(store.get(id)?.branchKey).toBe("/Gits/alpha|main");
			});
		});

		it("does not re-stamp an existing tab when it is reopened from another repo", () => {
			testInScope(() => {
				addRepoWithBranch("/Gits/alpha", "main");
				addRepoWithBranch("/Gits/beta", "develop");

				repositoriesStore.setActive("/Gits/alpha");
				const id = store.add("/Gits/alpha", "README.md");

				// User walks over to beta and reopens the same alpha file.
				repositoriesStore.setActive("/Gits/beta");
				expect(store.add("/Gits/alpha", "README.md")).toBe(id);

				const tab = store.get(id);
				expect(tab?.repoPath).toBe("/Gits/alpha");
				expect(tab?.branchKey).toBe("/Gits/alpha|main");
			});
		});

		it("keeps two same-named files from two repos as two tabs, each on its own branch", () => {
			testInScope(() => {
				addRepoWithBranch("/Gits/alpha", "main");
				addRepoWithBranch("/Gits/beta", "develop");

				const alphaId = store.add("/Gits/alpha", "README.md");
				const betaId = store.add("/Gits/beta", "README.md");

				expect(alphaId).not.toBe(betaId);
				expect(store.get(alphaId)?.branchKey).toBe("/Gits/alpha|main");
				expect(store.get(betaId)?.branchKey).toBe("/Gits/beta|develop");
			});
		});

		it("leaves a tab unscoped when its repo is unknown instead of borrowing the active one", () => {
			testInScope(() => {
				addRepoWithBranch("/Gits/beta", "develop");
				repositoriesStore.setActive("/Gits/beta");

				const id = store.add("", "/tmp/scratch.md");
				expect(store.get(id)?.branchKey).toBeUndefined();
			});
		});

		it("addFileBackground scopes to its own repo too", () => {
			testInScope(() => {
				addRepoWithBranch("/Gits/alpha", "main");
				addRepoWithBranch("/Gits/beta", "develop");
				repositoriesStore.setActive("/Gits/beta");

				const id = store.addFileBackground("/Gits/alpha", "plans/foo.md");
				expect(id).not.toBeNull();
				expect(store.get(id as string)?.branchKey).toBe("/Gits/alpha|main");
			});
		});
	});

	describe("openUiTab() repo routing", () => {
		it("keeps SDK plugin dashboards global", () => {
			testInScope(() => {
				for (const path of ["/Gits/alpha", "/Gits/beta"]) {
					repositoriesStore.add({ path, displayName: path });
					repositoriesStore.setWorkspace(path, "main", { worktreePath: path });
					repositoriesStore.setActiveWorkspace(path, "main");
				}
				repositoriesStore.setActive("/Gits/alpha");
				const id = store.addPluginPanel("sdk-plugin", "dashboard", "Dashboard", "<p>stats</p>");
				store.setActive(null);
				repositoriesStore.setActive("/Gits/beta");
				expect(store.getVisibleIds("/Gits/beta|main")).toContain(id);
			});
		});
		it.each([
			{ pinned: false, url: undefined },
			{ pinned: true, url: undefined },
			{ pinned: false, url: "https://example.test/report" },
			{ pinned: true, url: "https://example.test/report" },
		])(
			"shows pinned MCP tabs across repos and restores unpinned tabs on return (pinned=$pinned, url=$url)",
			({ pinned, url }) => {
				testInScope(() => {
					for (const path of ["/Gits/alpha", "/Gits/beta"]) {
						repositoriesStore.add({ path, displayName: path.split("/").pop()! });
						repositoriesStore.setWorkspace(path, "main", { worktreePath: path });
						repositoriesStore.setActiveWorkspace(path, "main");
					}
					repositoriesStore.setActive("/Gits/alpha");
					const id = store.openUiTab(`mcp-${pinned}`, "Preview", "<p>test</p>", pinned, url, true);
					store.setActive(null);
					repositoriesStore.setActive("/Gits/beta");
					if (pinned) expect(store.getVisibleIds("/Gits/beta|main")).toContain(id);
					else expect(store.getVisibleIds("/Gits/beta|main")).not.toContain(id);
					repositoriesStore.setActive("/Gits/alpha");
					expect(store.getVisibleIds("/Gits/alpha|main")).toContain(id);
				});
			},
		);
		it("returns a pinned MCP tab to repo scope when unpinned", () => {
			testInScope(() => {
				for (const path of ["/Gits/alpha", "/Gits/beta"]) {
					repositoriesStore.add({ path, displayName: path });
					repositoriesStore.setWorkspace(path, "main", { worktreePath: path });
					repositoriesStore.setActiveWorkspace(path, "main");
				}
				repositoriesStore.setActive("/Gits/alpha");
				const id = store.openUiTab("reversible-pin", "Preview", "<p>test</p>", true);
				store.setActive(null);
				repositoriesStore.setActive("/Gits/beta");
				expect(store.getVisibleIds("/Gits/beta|main")).toContain(id);
				store.setPinned(id, false);
				expect(store.getVisibleIds("/Gits/beta|main")).not.toContain(id);
				repositoriesStore.setActive("/Gits/alpha");
				expect(store.getVisibleIds("/Gits/alpha|main")).toContain(id);
			});
		});
		it("scopes an MCP tab to the caller repo when another repo is visible", () => {
			testInScope(() => {
				repositoriesStore.add({ path: "/Gits/alpha", displayName: "alpha" });
				repositoriesStore.add({ path: "/Gits/beta", displayName: "beta" });
				repositoriesStore.setActive("/Gits/beta");

				const id = store.openUiTab("wiz-panel", "MCF", "<p/>", false, undefined, true, "/Gits/alpha/src");
				const tab = store.get(id);
				expect(tab?.repoPath).toBe("/Gits/alpha");
				expect(repositoriesStore.state.activeRepoPath).toBe("/Gits/alpha");
			});
		});

		it.each([undefined, "https://example.test/report"])(
			"binds a focused MCP HTML/URL tab to its registered caller (url=%s)",
			(url) => {
				testInScope(() => {
					for (const path of ["/Gits/alpha", "/Gits/beta"]) {
						repositoriesStore.add({ path, displayName: path });
						repositoriesStore.setWorkspace(path, "main", { worktreePath: path });
						repositoriesStore.setActiveWorkspace(path, "main");
					}
					repositoriesStore.setActive("/Gits/beta");
					const id = store.openUiTab("caller-panel", "Report", "<p>report</p>", false, url, true, "/Gits/alpha/src");
					expect(store.get(id)?.repoPath).toBe("/Gits/alpha");
					expect(repositoriesStore.state.activeRepoPath).toBe("/Gits/alpha");
					expect(store.getVisibleIds("/Gits/alpha|main")).toContain(id);
				});
			},
		);

		it("keeps the visible repo unchanged for a background caller-scoped HTML tab", () => {
			testInScope(() => {
				for (const path of ["/Gits/alpha", "/Gits/beta"]) {
					repositoriesStore.add({ path, displayName: path });
					repositoriesStore.setWorkspace(path, "main", { worktreePath: path });
					repositoriesStore.setActiveWorkspace(path, "main");
				}
				repositoriesStore.setActive("/Gits/beta");
				const id = store.openUiTab("background", "Report", "<p>report</p>", false, undefined, false, "/Gits/alpha");
				expect(repositoriesStore.state.activeRepoPath).toBe("/Gits/beta");
				expect(store.get(id)?.repoPath).toBe("/Gits/alpha");
				expect(store.getVisibleIds("/Gits/beta|main")).not.toContain(id);
				expect(store.getVisibleIds("/Gits/alpha|main")).toContain(id);
			});
		});

		it("stores caller ownership for pinned MCP tabs too", () => {
			testInScope(() => {
				repositoriesStore.add({ path: "/Gits/alpha", displayName: "alpha" });
				repositoriesStore.add({ path: "/Gits/beta", displayName: "beta" });
				repositoriesStore.setActive("/Gits/beta");

				const id = store.openUiTab("wiz-pinned", "Pinned", "<p/>", true, undefined, true, "/Gits/alpha");
				const tab = store.get(id);
				expect(tab?.repoPath).toBe("/Gits/alpha");
			});
		});

		it("scopes unpinned MCP tabs to the active repo when origin cannot be resolved", () => {
			testInScope(() => {
				repositoriesStore.add({ path: "/Gits/beta", displayName: "beta" });
				repositoriesStore.setActive("/Gits/beta");

				const id = store.openUiTab("wiz-unknown", "X", "<p/>", false, undefined, true, "/not/a/registered/repo");
				const tab = store.get(id);
				expect(tab?.repoPath).toBe("/Gits/beta");
			});
		});

		it("scopes pinned MCP tabs to the active repo when origin is absent", () => {
			testInScope(() => {
				repositoriesStore.add({ path: "/Gits/beta", displayName: "beta" });
				repositoriesStore.setActive("/Gits/beta");

				const id = store.openUiTab("wiz-global", "G", "<p/>", true);
				const tab = store.get(id);
				expect(tab?.repoPath).toBe("/Gits/beta");
			});
		});
	});

	describe("closeUiTab()", () => {
		it("removes a plugin-panel tab by pluginId", () => {
			testInScope(() => {
				store.openUiTab("wiz-status", "Status", "<p>ok</p>", true);
				expect(store.getCount()).toBe(1);
				store.closeUiTab("wiz-status");
				expect(store.getCount()).toBe(0);
			});
		});

		it("is a no-op when pluginId does not exist", () => {
			testInScope(() => {
				store.openUiTab("wiz-a", "A", "<p>a</p>", true);
				store.closeUiTab("nonexistent");
				expect(store.getCount()).toBe(1);
			});
		});
	});
});
