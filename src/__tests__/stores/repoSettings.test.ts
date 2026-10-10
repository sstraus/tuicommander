import { createEffect, createRoot } from "solid-js";
import { createStore } from "solid-js/store";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { testInScope, testInScopeAsync } from "../helpers/store";

const mockInvoke = vi.fn().mockResolvedValue(undefined);

vi.mock("@tauri-apps/api/core", () => ({
	invoke: mockInvoke,
}));

// Mock repoDefaultsStore so getEffective tests are deterministic. It is a real
// Solid store, not a plain object, so tests can observe which reads actually
// subscribe to it.
const [mockDefaults, setMockDefaults] = createStore({
	baseBranch: "automatic",
	copyIgnoredFiles: false,
	copyUntrackedFiles: false,
	setupScript: "",
	runScript: "",
	archiveScript: "",
});

vi.mock("../../stores/repoDefaults", () => ({
	repoDefaultsStore: { state: mockDefaults },
}));

/** The entry for `path` in the most recent `save_repo_settings` call. */
function lastSavedEntry(path: string): Record<string, unknown> {
	const calls = mockInvoke.mock.calls.filter(([name]) => name === "save_repo_settings");
	const call = calls.at(-1)!;
	if (!call) throw new Error("save_repo_settings was never called");
	const repos = (call[1] as { config: { repos: Record<string, Record<string, unknown>> } }).config.repos;
	return repos[path];
}

describe("repoSettingsStore", () => {
	let store: typeof import("../../stores/repoSettings").repoSettingsStore;

	beforeEach(async () => {
		vi.resetModules();
		mockInvoke.mockReset().mockResolvedValue(undefined);
		localStorage.clear();

		// Reset mock defaults to known state
		setMockDefaults({
			baseBranch: "automatic",
			copyIgnoredFiles: false,
			copyUntrackedFiles: false,
			setupScript: "",
			runScript: "",
			archiveScript: "",
		});

		vi.doMock("@tauri-apps/api/core", () => ({
			invoke: mockInvoke,
		}));

		vi.doMock("../../stores/repoDefaults", () => ({
			repoDefaultsStore: { state: mockDefaults },
		}));

		store = (await import("../../stores/repoSettings")).repoSettingsStore;
		await store.hydrate();
		mockInvoke.mockClear();
	});

	describe("get()", () => {
		it("returns undefined for unknown repo", () => {
			testInScope(() => {
				expect(store.get("/unknown")).toBeUndefined();
			});
		});
	});

	describe("getOrCreate()", () => {
		it("creates settings for new repo with null overridable fields (inheriting from global)", () => {
			testInScope(() => {
				const settings = store.getOrCreate("/repo", "my-repo");
				expect(settings.path).toBe("/repo");
				expect(settings.displayName).toBe("my-repo");
				// Overridable fields default to null (inherit from global defaults)
				expect(settings.baseBranch).toBeNull();
				expect(settings.copyIgnoredFiles).toBeNull();
				expect(settings.copyUntrackedFiles).toBeNull();
				expect(settings.setupScript).toBeNull();
				expect(settings.runScript).toBeNull();
				// Non-overridable fields remain non-nullable
				expect(settings.color).toBe("");
			});
		});

		it("returns existing settings", () => {
			testInScope(() => {
				store.getOrCreate("/repo", "my-repo");
				store.update("/repo", { baseBranch: "main" });
				const settings = store.getOrCreate("/repo", "my-repo");
				expect(settings.baseBranch).toBe("main");
			});
		});

		it("persists via invoke", () => {
			testInScope(() => {
				store.getOrCreate("/repo", "my-repo");
				expect(mockInvoke).toHaveBeenCalledWith("save_repo_settings", {
					base: expect.anything(),
					config: expect.objectContaining({
						repos: expect.objectContaining({
							"/repo": expect.objectContaining({ path: "/repo" }),
						}),
					}),
				});
			});
		});
	});

	describe("update()", () => {
		it("updates existing settings", () => {
			testInScope(() => {
				store.getOrCreate("/repo", "my-repo");
				store.update("/repo", { baseBranch: "main", setupScript: "npm install" });
				expect(store.get("/repo")?.baseBranch).toBe("main");
				expect(store.get("/repo")?.setupScript).toBe("npm install");
			});
		});

		it("can set overridable fields back to null (inherit)", () => {
			testInScope(() => {
				store.getOrCreate("/repo", "my-repo");
				store.update("/repo", { baseBranch: "main" });
				store.update("/repo", { baseBranch: null });
				expect(store.get("/repo")?.baseBranch).toBeNull();
			});
		});

		it("ignores updates for unknown repos", () => {
			testInScope(() => {
				store.update("/unknown", { baseBranch: "main" }); // Should not throw
			});
		});
	});

	describe("getEffective()", () => {
		it("returns global defaults for a new repo with null overrides", () => {
			testInScope(() => {
				store.getOrCreate("/repo", "my-repo");
				const effective = store.getEffective("/repo");
				expect(effective).toBeDefined();
				expect(effective!.baseBranch).toBe("automatic"); // from global default
				expect(effective!.copyIgnoredFiles).toBe(false);
				expect(effective!.copyUntrackedFiles).toBe(false);
				expect(effective!.setupScript).toBe("");
				expect(effective!.runScript).toBe("");
			});
		});

		it("uses per-repo override when set", () => {
			testInScope(() => {
				store.getOrCreate("/repo", "my-repo");
				store.update("/repo", { baseBranch: "main", copyIgnoredFiles: true });

				setMockDefaults("baseBranch", "develop"); // global default is different
				const effective = store.getEffective("/repo");
				expect(effective).toBeDefined();
				expect(effective!.baseBranch).toBe("main"); // repo override wins
				expect(effective!.copyIgnoredFiles).toBe(true);
			});
		});

		it("falls back to global default when field is null", () => {
			testInScope(() => {
				store.getOrCreate("/repo", "my-repo");
				// baseBranch is null (inherit) but global says "develop"
				setMockDefaults("baseBranch", "develop");
				const effective = store.getEffective("/repo");
				expect(effective).toBeDefined();
				expect(effective!.baseBranch).toBe("develop");
			});
		});

		it("returns undefined for unknown repo", () => {
			testInScope(() => {
				expect(store.getEffective("/unknown")).toBeUndefined();
			});
		});

		it("returns archiveScript from global default when not overridden", () => {
			testInScope(() => {
				store.getOrCreate("/repo", "my-repo");
				setMockDefaults("archiveScript", "cleanup.sh");
				const effective = store.getEffective("/repo");
				expect(effective!.archiveScript).toBe("cleanup.sh");
			});
		});

		it("uses per-repo archiveScript override when set", () => {
			testInScope(() => {
				store.getOrCreate("/repo", "my-repo");
				store.update("/repo", { archiveScript: "my-cleanup.sh" });
				setMockDefaults("archiveScript", "global-cleanup.sh");
				const effective = store.getEffective("/repo");
				expect(effective!.archiveScript).toBe("my-cleanup.sh");
			});
		});

		it("preserves non-overridable fields (path, displayName, color)", () => {
			testInScope(() => {
				store.getOrCreate("/repo", "my-repo");
				store.update("/repo", { color: "#ff0000" });
				const effective = store.getEffective("/repo");
				expect(effective!.path).toBe("/repo");
				expect(effective!.displayName).toBe("my-repo");
				expect(effective!.color).toBe("#ff0000");
			});
		});
	});

	describe("three-tier getEffective() with local config", () => {
		it("uses .tuic.json values when per-repo setting is null", async () => {
			await testInScopeAsync(async () => {
				// Simulate Tauri returning a local config from .tuic.json
				mockInvoke.mockImplementation(async (cmd: string) => {
					if (cmd === "load_repo_local_config") {
						return { base_branch: "develop", setup_script: "make setup" };
					}
					return undefined;
				});

				store.getOrCreate("/repo", "my-repo");
				await store.loadLocalConfig("/repo");

				const effective = store.getEffective("/repo");
				expect(effective).toBeDefined();
				// .tuic.json overrides global default for non-script fields
				expect(effective!.baseBranch).toBe("develop");
				// SECURITY: script fields from .tuic.json are intentionally NOT merged
				// (a malicious repo could inject shell commands via committed .tuic.json)
				expect(effective!.setupScript).toBe(""); // from global default, NOT .tuic.json
				// Global default still applies for fields not in .tuic.json
				expect(effective!.runScript).toBe(""); // from global default
			});
		});

		it("per-repo setting overrides .tuic.json", async () => {
			await testInScopeAsync(async () => {
				mockInvoke.mockImplementation(async (cmd: string) => {
					if (cmd === "load_repo_local_config") {
						return { base_branch: "develop", setup_script: "make setup" };
					}
					return undefined;
				});

				store.getOrCreate("/repo", "my-repo");
				store.update("/repo", { baseBranch: "main" }); // per-repo override
				await store.loadLocalConfig("/repo");

				const effective = store.getEffective("/repo");
				expect(effective).toBeDefined();
				// per-repo overrides .tuic.json
				expect(effective!.baseBranch).toBe("main");
				// SECURITY: script fields from .tuic.json are NOT merged — falls back to global default
				expect(effective!.setupScript).toBe(""); // from global default, NOT .tuic.json
			});
		});

		it("returns undefined for missing .tuic.json (no local config cached)", () => {
			testInScope(() => {
				store.getOrCreate("/repo", "my-repo");
				// No loadLocalConfig called — should fall back to two-tier
				const effective = store.getEffective("/repo");
				expect(effective).toBeDefined();
				expect(effective!.baseBranch).toBe("automatic"); // global default
			});
		});

		it("handles null from Tauri (no .tuic.json file)", async () => {
			await testInScopeAsync(async () => {
				mockInvoke.mockImplementation(async (cmd: string) => {
					if (cmd === "load_repo_local_config") return null;
					return undefined;
				});

				store.getOrCreate("/repo", "my-repo");
				await store.loadLocalConfig("/repo");

				const effective = store.getEffective("/repo");
				expect(effective).toBeDefined();
				expect(effective!.baseBranch).toBe("automatic"); // global default
			});
		});
	});

	describe("remove()", () => {
		it("removes settings", () => {
			testInScope(() => {
				store.getOrCreate("/repo", "my-repo");
				store.remove("/repo");
				expect(store.get("/repo")).toBeUndefined();
			});
		});

		it("clears activeRepoPath if removed", () => {
			testInScope(() => {
				store.getOrCreate("/repo", "my-repo");
				store.setActiveRepo("/repo");
				store.remove("/repo");
				expect(store.state.activeRepoPath).toBeNull();
			});
		});
	});

	describe("hasCustomSettings()", () => {
		it("returns false for defaults", async () => {
			await testInScopeAsync(async () => {
				store.getOrCreate("/repo", "my-repo");
				mockInvoke.mockResolvedValueOnce(false);
				expect(await store.hasCustomSettings("/repo")).toBe(false);
				expect(mockInvoke).toHaveBeenCalledWith("check_has_custom_settings", { path: "/repo" });
			});
		});

		it("returns false for unknown repos", async () => {
			await testInScopeAsync(async () => {
				expect(await store.hasCustomSettings("/unknown")).toBe(false);
			});
		});
	});

	describe("reset()", () => {
		it("resets overridable fields to null (inherit from global)", () => {
			testInScope(() => {
				store.getOrCreate("/repo", "my-repo");
				store.update("/repo", { baseBranch: "main", setupScript: "npm install" });
				store.reset("/repo");
				expect(store.get("/repo")?.baseBranch).toBeNull();
				expect(store.get("/repo")?.setupScript).toBeNull();
				expect(store.get("/repo")?.copyIgnoredFiles).toBeNull();
			});
		});

		it("preserves non-overridable fields (displayName, color)", () => {
			testInScope(() => {
				store.getOrCreate("/repo", "my-repo");
				store.update("/repo", { color: "#ff0000" });
				store.reset("/repo");
				expect(store.get("/repo")?.displayName).toBe("my-repo");
				expect(store.get("/repo")?.color).toBe("#ff0000");
			});
		});
	});

	describe("getAll()", () => {
		it("returns all settings", () => {
			testInScope(() => {
				store.getOrCreate("/repo1", "repo1");
				store.getOrCreate("/repo2", "repo2");
				expect(store.getAll()).toHaveLength(2);
			});
		});
	});

	describe("setActiveRepo()", () => {
		it("sets active repo path", () => {
			testInScope(() => {
				store.setActiveRepo("/repo");
				expect(store.state.activeRepoPath).toBe("/repo");
			});
		});
	});

	// `RepoSettingsEntry` in config.rs is snake_case with `#[serde(default)]` on
	// every field, so a camelCase key is not rejected — it is dropped, and the
	// override reads as unset. These tests pin the wire shape on both directions;
	// getting it wrong loses every per-repo override on restart, silently.
	describe("wire format (snake_case)", () => {
		it("loads settings from Rust backend", async () => {
			mockInvoke.mockResolvedValueOnce({
				repos: {
					"/repo": {
						path: "/repo",
						display_name: "my-repo",
						base_branch: "main",
						dev_server_url: "http://localhost:5173",
						prompt_on_create: false,
						auto_fetch_interval_minutes: 15,
						copy_ignored_files: null,
						branch_labels: { my_feature: "My Feature" },
					},
				},
			});

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.get("/repo")?.displayName).toBe("my-repo");
				expect(store.get("/repo")?.baseBranch).toBe("main");
				expect(store.get("/repo")?.devServerUrl).toBe("http://localhost:5173");
				expect(store.get("/repo")?.promptOnCreate).toBe(false);
				expect(store.get("/repo")?.autoFetchIntervalMinutes).toBe(15);
				expect(store.get("/repo")?.copyIgnoredFiles).toBeNull();
				// The map is keyed by branch name — those keys are user data.
				expect(store.get("/repo")?.branchLabels).toEqual({ my_feature: "My Feature" });
				expect(mockInvoke).toHaveBeenCalledWith("load_repo_settings");
			});
		});

		it("fills a key the backend omitted", async () => {
			// `branch_labels` and `mcp_upstreams` are skipped when empty.
			mockInvoke.mockResolvedValueOnce({ repos: { "/repo": { path: "/repo", color: "" } } });

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.get("/repo")?.branchLabels).toEqual({});
				expect(store.get("/repo")?.mcpUpstreams).toBeNull();
				expect(store.get("/repo")?.devServerUrl).toBeNull();
			});
		});

		it("saves every override under the name the backend reads", async () => {
			await testInScopeAsync(async () => {
				store.getOrCreate("/repo", "my-repo");
				mockInvoke.mockClear();
				store.update("/repo", {
					promptOnCreate: false,
					deleteBranchOnRemove: true,
					autoFetchIntervalMinutes: 5,
					devServerUrl: "http://localhost:5173",
				});

				await vi.waitFor(() => expect(lastSavedEntry("/repo")).toMatchObject({ prompt_on_create: false }));
				const entry = lastSavedEntry("/repo");
				expect(entry).toMatchObject({
					path: "/repo",
					display_name: "my-repo",
					prompt_on_create: false,
					delete_branch_on_remove: true,
					auto_fetch_interval_minutes: 5,
					dev_server_url: "http://localhost:5173",
				});
				// No camelCase key may survive: serde would drop it without a word.
				expect(Object.keys(entry).filter((k) => /[A-Z]/.test(k))).toEqual([]);
			});
		});

		it("round-trips an override through the wire shape", async () => {
			testInScope(() => {
				store.getOrCreate("/repo", "my-repo");
				mockInvoke.mockClear();
				store.update("/repo", { promptOnCreate: false });
			});
			await vi.waitFor(() => expect(lastSavedEntry("/repo")).toMatchObject({ prompt_on_create: false }));
			const saved = lastSavedEntry("/repo");

			mockInvoke.mockResolvedValueOnce({ repos: { "/repo": saved } });
			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.get("/repo")?.promptOnCreate).toBe(false);
			});
		});

		it("migrates from localStorage on first run", async () => {
			localStorage.setItem(
				"tui-commander-repo-settings",
				JSON.stringify({
					"/repo": { path: "/repo", displayName: "my-repo", baseBranch: "main" },
				}),
			);
			mockInvoke.mockResolvedValueOnce(undefined); // save_repo_settings migration
			mockInvoke.mockResolvedValueOnce({ repos: {} }); // load_repo_settings

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(localStorage.getItem("tui-commander-repo-settings")).toBeNull();
				// The migration writes the wire shape too, or it hands the backend an
				// entry it reads as empty and the old settings are gone for good.
				expect(mockInvoke).toHaveBeenCalledWith("save_repo_settings", {
					base: expect.anything(),
					config: { repos: { "/repo": { path: "/repo", display_name: "my-repo", base_branch: "main" } } },
				});
			});
		});
	});
	// ---- Per-field access (F73) ----
	//
	// getEffective() reads ~50 signals across four stores and allocates a
	// 24-field object. Call sites that want one field (a branch label, a
	// terminalMetaHotkeys flag) must not wake on every one of those.

	describe("per-field effective access", () => {
		let dispose: (() => void) | undefined;

		afterEach(() => {
			dispose?.();
			dispose = undefined;
		});

		const flush = () => new Promise<void>((resolve) => queueMicrotask(resolve));

		it("reading one field does not subscribe to unrelated global defaults", async () => {
			store.getOrCreate("/repo", "my-repo");
			let runs = 0;

			createRoot((d) => {
				dispose = d;
				createEffect(() => {
					// terminalMetaHotkeys resolves from the repo field alone.
					void store.getEffectiveField("/repo", "terminalMetaHotkeys");
					runs++;
				});
			});
			await flush();
			runs = 0;

			// A default this field never consults.
			setMockDefaults("baseBranch", "develop");
			await flush();

			expect(runs).toBe(0);
		});

		// The contrast that motivates getEffectiveField: the same one-field read
		// through getEffective wakes on a default the field never consults,
		// because getEffective touches all 53 properties on every call.
		it("getEffective wakes a one-field reader on an unrelated default", async () => {
			store.getOrCreate("/repo", "my-repo");
			let runs = 0;

			createRoot((d) => {
				dispose = d;
				createEffect(() => {
					void store.getEffective("/repo")?.terminalMetaHotkeys;
					runs++;
				});
			});
			await flush();
			runs = 0;

			setMockDefaults("baseBranch", "develop");
			await flush();

			expect(runs).toBe(1);
		});

		it("still wakes when the field's own inheritance chain changes", async () => {
			store.getOrCreate("/repo", "my-repo");
			let seen: string | undefined;
			let runs = 0;

			createRoot((d) => {
				dispose = d;
				createEffect(() => {
					seen = store.getEffectiveField("/repo", "baseBranch");
					runs++;
				});
			});
			await flush();
			expect(seen).toBe("automatic");
			runs = 0;

			setMockDefaults("baseBranch", "develop");
			await flush();

			expect(runs).toBe(1);
			expect(seen).toBe("develop");
		});

		it("agrees with getEffective on every field", () => {
			testInScope(() => {
				store.getOrCreate("/repo", "my-repo");
				store.update("/repo", { baseBranch: "main", color: "#fff", autoFetchIntervalMinutes: 7 });
				setMockDefaults("archiveScript", "cleanup.sh");

				const effective = store.getEffective("/repo")!;
				for (const key of Object.keys(effective) as (keyof typeof effective)[]) {
					expect(store.getEffectiveField("/repo", key)).toEqual(effective[key]);
				}
			});
		});

		it("returns undefined for an unknown repo", () => {
			testInScope(() => {
				expect(store.getEffectiveField("/unknown", "baseBranch")).toBeUndefined();
			});
		});
	});
});
