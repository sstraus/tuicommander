import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { testInScope, testInScopeAsync } from "../helpers/store";

const mockInvoke = vi.fn().mockResolvedValue(undefined);

vi.mock("@tauri-apps/api/core", () => ({
	invoke: mockInvoke,
}));

describe("settingsStore", () => {
	let store: typeof import("../../stores/settings").settingsStore;

	beforeEach(async () => {
		vi.useFakeTimers();
		vi.resetModules();
		localStorage.clear();
		mockInvoke.mockReset().mockResolvedValue(undefined);

		vi.doMock("@tauri-apps/api/core", () => ({
			invoke: mockInvoke,
		}));

		store = (await import("../../stores/settings")).settingsStore;
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	// Catches: mobile config failures emit desktop-only restart advice instead of remaining retryable.
	it("can retry a quiet failed hydration without emitting restart advice", async () => {
		const { toastsStore } = await import("../../stores/toasts");
		const { appLogger } = await import("../../stores/appLogger");
		const log = vi.spyOn(appLogger, "error").mockImplementation(() => {});
		const toast = vi.spyOn(toastsStore, "add");
		mockInvoke.mockRejectedValueOnce(new TypeError("Load failed"));
		expect(await store.hydrate({ notifyOnError: false })).toBe(false);
		expect(toast).not.toHaveBeenCalled();
		mockInvoke.mockResolvedValueOnce({ ego_executable: "ego" });
		expect(await store.hydrate({ notifyOnError: false })).toBe(true);
		expect(store.state.egoExecutable).toBe("ego");
		log.mockRestore();
		toast.mockRestore();
	});

	// Catches: backend field recovery remains invisible or omits the recoverable original.
	it("shows recovered config fields and backup path through the existing warning toast", async () => {
		const { toastsStore } = await import("../../stores/toasts");
		const toast = vi.spyOn(toastsStore, "add");
		mockInvoke.mockResolvedValueOnce({
			ide: "cursor",
			config_recovery: { fields: ["font_size", "services.server.port"], backup_path: "/config/config.corrupt-example" },
		});
		expect(await store.hydrate()).toBe(true);
		expect(store.state.ide).toBe("cursor");
		expect(toast).toHaveBeenCalledWith(
			"Settings recovered",
			"Reset invalid or missing settings: font_size, services.server.port. Original file: /config/config.corrupt-example",
			"warn",
		);
		toast.mockRestore();
	});

	// Catches: detached-panel focus repeats a cached recovery warning after toast dedup expires.
	it("shows each recovery only once across repeated hydration and focus", async () => {
		const { toastsStore } = await import("../../stores/toasts");
		const toast = vi.spyOn(toastsStore, "add");
		const config = { config_recovery: { fields: ["font_size"], backup_path: "/config/config.corrupt-first" } };
		mockInvoke.mockResolvedValue(config);
		await store.hydrate();
		await vi.advanceTimersByTimeAsync(6000);
		await store.hydrate();
		mockInvoke.mockResolvedValueOnce({});
		await store.hydrate();
		await store.hydrate();
		expect(toast).toHaveBeenCalledTimes(1);
		mockInvoke.mockResolvedValueOnce({
			config_recovery: { fields: ["theme"], backup_path: "/config/config.corrupt-second" },
		});
		await store.hydrate();
		expect(toast).toHaveBeenCalledTimes(2);
		toast.mockRestore();
	});

	/** Hydrate the store so save() is unlocked — pre-hydrate saves are refused
	 *  to avoid clobbering config.json with defaults. */
	async function hydrateStore(): Promise<void> {
		mockInvoke.mockResolvedValueOnce({
			shell: null,
			font_family: "JetBrains Mono",
			font_size: 14,
			theme: "vscode-dark",
			mcp_server_enabled: false,
			ide: "vscode",
			default_font_size: 13,
		});
		await store.hydrate();
	}

	function saveConfigCalls(): unknown[][] {
		return mockInvoke.mock.calls.filter((c: unknown[]) => c[0] === "save_config");
	}

	// Catches: saving the tray preference overwrites another background setting.
	it("persists the tray preference while retaining background fields", async () => {
		mockInvoke.mockResolvedValueOnce({ background: { tray_icon: "always", hide_dock_icon: false } });
		await store.hydrate();
		expect(store.state.trayIcon).toBe("always");
		mockInvoke.mockImplementation(async (command) =>
			command === "load_config"
				? { background: { tray_icon: "always", hide_dock_icon: false, webview_unload_minutes: 25 } }
				: undefined,
		);
		store.setTrayIcon("never");
		await vi.runAllTimersAsync();
		expect(mockInvoke).toHaveBeenCalledWith(
			"save_config",
			expect.objectContaining({
				config: expect.objectContaining({
					background: { tray_icon: "never", hide_dock_icon: false, webview_unload_minutes: 25 },
				}),
			}),
		);
	});

	// Catches: new expert resource fields disappearing at the store/save boundary.
	it("hydrates_and_saves_resource_limits_without_overwriting_a_remote_background_leaf", async () => {
		mockInvoke.mockResolvedValueOnce({
			index_memory_budget_mb: 128,
			attachment_max_bytes: 2048,
			background: { webview_unload_minutes: 15, tray_icon: "always" },
		});
		await store.hydrate();
		expect(store.state.indexMemoryBudgetMb).toBe(128);
		expect(store.state.attachmentMaxBytes).toBe(2048);
		expect(store.state.webviewUnloadMinutes).toBe(15);
		mockInvoke.mockImplementation(async (command) =>
			command === "load_config"
				? { background: { webview_unload_minutes: 15, tray_icon: "never", hide_dock_icon: false } }
				: undefined,
		);
		store.setIndexMemoryBudgetMb(0);
		store.setAttachmentMaxBytes(4096);
		store.setWebviewUnloadMinutes(0);
		await vi.runAllTimersAsync();
		expect(mockInvoke).toHaveBeenCalledWith(
			"save_config",
			expect.objectContaining({
				config: expect.objectContaining({
					index_memory_budget_mb: 0,
					attachment_max_bytes: 4096,
					background: { webview_unload_minutes: 0, tray_icon: "never", hide_dock_icon: false },
				}),
			}),
		);
	});

	// Catches: NaN/fractional/negative inputs entering a whole-number resource setting.
	it.each([-1, 0.5, Number.NaN, Number.POSITIVE_INFINITY, Number.MAX_SAFE_INTEGER + 1])(
		"rejects invalid resource limit %s",
		(value) => {
			const before = [
				store.state.indexMemoryBudgetMb,
				store.state.attachmentMaxBytes,
				store.state.webviewUnloadMinutes,
			];
			store.setIndexMemoryBudgetMb(value);
			store.setAttachmentMaxBytes(value);
			store.setWebviewUnloadMinutes(value);
			expect([
				store.state.indexMemoryBudgetMb,
				store.state.attachmentMaxBytes,
				store.state.webviewUnloadMinutes,
			]).toEqual(before);
		},
	);

	// Catches: hydration silently turns a persisted beta channel back into stable.
	it.each(["beta", "stable", "nightly"])("keeps the saved %s update channel", async (channel) => {
		mockInvoke.mockResolvedValueOnce({ update_channel: channel });
		await testInScopeAsync(async () => {
			await store.hydrate();
			expect(store.state.updateChannel).toBe(channel);
		});
	});

	// Catches: hydration/save omits the master prompt, or over-limit text replaces the saved value.
	it("persists AI Chat master prompt and clears it without touching ACP", async () => {
		mockInvoke.mockResolvedValueOnce({ ai_chat_master_prompt: "Saved instructions" });
		await store.hydrate();
		expect(store.state.aiChatMasterPrompt).toBe("Saved instructions");
		expect(store.setAiChatMasterPrompt("Delegate investigations.\nReturn to chat.")).toBe(true);
		await vi.advanceTimersByTimeAsync(600);
		expect(
			((saveConfigCalls().at(-1) as unknown[])[1] as { config: { ai_chat_master_prompt: string } }).config
				.ai_chat_master_prompt,
		).toBe("Delegate investigations.\nReturn to chat.");
		expect(store.setAiChatMasterPrompt("é".repeat(4097))).toBe(false);
		expect(store.state.aiChatMasterPrompt).toBe("Delegate investigations.\nReturn to chat.");
		expect(store.setAiChatMasterPrompt("")).toBe(true);
		await vi.advanceTimersByTimeAsync(600);
		expect(
			((saveConfigCalls().at(-1) as unknown[])[1] as { config: { ai_chat_master_prompt: string } }).config
				.ai_chat_master_prompt,
		).toBe("");
		expect(mockInvoke.mock.calls.some(([command]) => String(command).startsWith("acp_"))).toBe(false);
	});

	describe("ego ACP profile", () => {
		it("persists a selected profile and allows clearing it", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				expect(store.setEgoProfile("coordinator")).toBe(true);
				expect(store.state.egoProfile).toBe("coordinator");
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect((saveConfigCalls()[0][1] as { config: { ego_profile: string } }).config.ego_profile).toBe("coordinator");
				expect(store.setEgoProfile("")).toBe(true);
				expect(store.state.egoProfile).toBe("");
			});
		});

		it.each(["my profile", "-other", " profile", "profile\nother", "name\u00a0part", "name\u0085part", "a".repeat(65)])(
			"rejects ambiguous profile name %j without changing the saved choice",
			async (name) => {
				await testInScopeAsync(async () => {
					await hydrateStore();
					store.setEgoProfile("coordinator");
					expect(store.setEgoProfile(name)).toBe(false);
					expect(store.state.egoProfile).toBe("coordinator");
				});
			},
		);

		it("accepts a 64-byte profile name", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				expect(store.setEgoProfile("a".repeat(64))).toBe(true);
				expect(store.state.egoProfile).toBe("a".repeat(64));
			});
		});
	});

	describe("AI Chat workspace", () => {
		it("persists an absolute path and allows clearing it", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				expect(store.setAiChatWorkspace("/srv/chat")).toBe(true);
				expect(store.state.aiChatWorkspace).toBe("/srv/chat");
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect((saveConfigCalls()[0][1] as { config: { ai_chat_workspace: string } }).config.ai_chat_workspace).toBe(
					"/srv/chat",
				);
				expect(store.setAiChatWorkspace("")).toBe(true);
				expect(store.state.aiChatWorkspace).toBe("");
			});
		});

		it.each(["relative/dir", "~/chat", "chat", "./chat"])("refuses %j and keeps the saved path", async (path) => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setAiChatWorkspace("/srv/chat");
				expect(store.setAiChatWorkspace(path)).toBe(false);
				expect(store.state.aiChatWorkspace).toBe("/srv/chat");
			});
		});

		it.each(["C:\\chat", "C:/chat", "\\\\host\\share"])("accepts the Windows absolute path %j", async (path) => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				expect(store.setAiChatWorkspace(path)).toBe(true);
			});
		});
	});

	describe("pre-hydrate write protection", () => {
		it("does not persist before hydrate", async () => {
			await testInScopeAsync(async () => {
				store.setIde("cursor");
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(saveConfigCalls()).toHaveLength(0);
			});
		});

		it("does not persist when hydrate failed", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("no backend"));
			await testInScopeAsync(async () => {
				await store.hydrate();
				store.setIde("cursor");
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(saveConfigCalls()).toHaveLength(0);
			});
		});

		it("persists after successful hydrate", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setIde("cursor");
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(saveConfigCalls()).toHaveLength(1);
			});
		});
	});

	describe("load-modify-save preserves foreign-owned fields", () => {
		// Regression: a general-settings save must NOT clobber fields owned by
		// other surfaces (services.server.enabled → RemoteAccessPanel, global_hotkey →
		// set_global_hotkey command). The old buildConfig() rebuilt the whole
		// config from a stale hydrate snapshot, wiping the web-server toggle and
		// global hotkey on the next restart.
		it("keeps services.server.enabled and global_hotkey set by other writers", async () => {
			await testInScopeAsync(async () => {
				// Hydrate with a STALE snapshot: server OFF, no hotkey.
				mockInvoke.mockResolvedValueOnce({
					shell: null,
					font_family: "JetBrains Mono",
					font_size: 14,
					theme: "vscode-dark",
					mcp_server_enabled: false,
					ide: "vscode",
					default_font_size: 13,
					global_hotkey: null,
					services: { server: { enabled: false, port: 9876 } },
				});
				await store.hydrate();

				// Another surface has since enabled the server + set a hotkey; the
				// fresh load_config the save path performs reflects that on disk.
				mockInvoke.mockResolvedValueOnce({
					shell: null,
					font_family: "JetBrains Mono",
					font_size: 14,
					theme: "vscode-dark",
					mcp_server_enabled: true,
					ide: "vscode",
					default_font_size: 13,
					global_hotkey: "CommandOrControl+1",
					services: { server: { enabled: true, port: 9876 } },
				});

				store.setIde("cursor");
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();

				const calls = saveConfigCalls();
				expect(calls).toHaveLength(1);
				const saved = (
					calls[0][1] as {
						config: {
							ide: string;
							services: { server: { enabled: boolean } };
							global_hotkey: string | null;
							mcp_server_enabled: boolean;
						};
					}
				).config;
				expect(saved.ide).toBe("cursor"); // owned field applied
				expect(saved.services.server.enabled).toBe(true); // preserved
				expect(saved.global_hotkey).toBe("CommandOrControl+1"); // preserved
				expect(saved.mcp_server_enabled).toBe(true); // preserved
			});
		});

		it("skips the save (no clobber) when the fresh load_config fails", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				mockInvoke.mockRejectedValueOnce(new Error("backend down"));
				store.setIde("cursor");
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(saveConfigCalls()).toHaveLength(0);
			});
		});
	});

	// Catches: the UI dropping the close choice when saving or hydrating config.
	it("persists and hydrates the main-window close preference", async () => {
		await testInScopeAsync(async () => {
			await hydrateStore();
			expect(store.state.closeBehavior).toBe("ask");
			store.setCloseBehavior("background");
			vi.advanceTimersByTime(600);
			await vi.runAllTimersAsync();
			expect(mockInvoke).toHaveBeenCalledWith("save_config", {
				base: expect.anything(),
				config: expect.objectContaining({ close_behavior: "background" }),
			});
			mockInvoke.mockResolvedValueOnce({ close_behavior: "quit" });
			await store.hydrate();
			expect(store.state.closeBehavior).toBe("quit");
		});
	});

	describe("defaults", () => {
		it("has correct default values", () => {
			testInScope(() => {
				expect(store.state.ide).toBe("vscode");
				expect(store.state.font).toBe("JetBrains Mono");
				expect(store.state.defaultFontSize).toBe(13);
				expect(store.state.confirmBeforeQuit).toBe(true);
				expect(store.state.confirmBeforeClosingTab).toBe(true);
				expect(store.state.splitTabMode).toBe("separate");
			});
		});

		it("hydrates showLastPrompt to true when config.json predates the field", async () => {
			// Rust `show_last_prompt` defaults to true; an old config.json omits it,
			// and the store must agree instead of hiding the last prompt.
			await hydrateStore();
			testInScope(() => {
				expect(store.state.showLastPrompt).toBe(true);
			});
		});
	});

	describe("setIde()", () => {
		it("updates IDE preference in state", () => {
			testInScope(() => {
				store.setIde("cursor");
				expect(store.state.ide).toBe("cursor");
			});
		});

		it("persists IDE via debounced save_config", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setIde("cursor");
				// Not yet saved (debounced)
				expect(mockInvoke).not.toHaveBeenCalledWith("save_config", expect.anything());
				// Advance past debounce
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					base: expect.anything(),
					config: expect.objectContaining({ ide: "cursor" }),
				});
			});
		});

		it("coalesces rapid changes into single save", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setIde("cursor");
				store.setIde("zed");
				store.setIde("windsurf");
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				// Only one save_config call with the final value
				const saveCalls = mockInvoke.mock.calls.filter((c: unknown[]) => c[0] === "save_config");
				expect(saveCalls).toHaveLength(1);
				expect(saveCalls[0][1].config.ide).toBe("windsurf");
			});
		});
	});

	describe("setFont()", () => {
		it("updates font in store state", () => {
			testInScope(() => {
				store.setFont("Fira Code");
				expect(store.state.font).toBe("Fira Code");
			});
		});

		it("persists font via debounced save_config", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setFont("Fira Code");
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					base: expect.anything(),
					config: expect.objectContaining({ font_family: "Fira Code" }),
				});
			});
		});
	});

	describe("getFontFamily()", () => {
		it("returns CSS font family string", () => {
			testInScope(() => {
				const family = store.getFontFamily();
				expect(family).toContain("JetBrains");
				expect(family).toContain("monospace");
			});
		});
	});

	describe("getIdeName()", () => {
		it("returns display name for IDE", () => {
			testInScope(() => {
				expect(store.getIdeName()).toBe("VS Code");
				store.setIde("zed");
				expect(store.getIdeName()).toBe("Zed");
			});
		});
	});

	describe("loadFontFromConfig()", () => {
		it("applies font from hydrated config cache", async () => {
			mockInvoke.mockResolvedValueOnce({
				shell: null,
				font_family: "Hack",
				font_size: 14,
				theme: "tokyo-night",
				mcp_server_enabled: false,
				ide: "vscode",
				default_font_size: 12,
			});

			await testInScopeAsync(async () => {
				await store.hydrate();
				// Change font locally
				store.setFont("Fira Code");
				expect(store.state.font).toBe("Fira Code");
				// Re-apply from cache (no IPC)
				store.loadFontFromConfig();
				expect(store.state.font).toBe("Hack");
				// No extra load_config call — uses hydrate cache
				const loadCalls = mockInvoke.mock.calls.filter((c: unknown[]) => c[0] === "load_config");
				expect(loadCalls).toHaveLength(1); // only from hydrate
			});
		});

		it("no-op before hydrate", () => {
			testInScope(() => {
				store.loadFontFromConfig();
				expect(store.state.font).toBe("JetBrains Mono");
			});
		});
	});

	describe("hydrate()", () => {
		it("loads settings from Rust config", async () => {
			mockInvoke.mockResolvedValueOnce({
				shell: null,
				font_family: "Hack",
				font_size: 14,
				theme: "tokyo-night",

				mcp_server_enabled: false,
				ide: "zed",
				default_font_size: 16,
			});

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.state.font).toBe("Hack");
				expect(store.state.ide).toBe("zed");
				expect(store.state.defaultFontSize).toBe(16);
			});
		});

		it("falls back to the commander theme when the config names none", async () => {
			mockInvoke.mockResolvedValueOnce({
				shell: null,
				font_family: "Hack",
				font_size: 14,
				theme: "",
				mcp_server_enabled: false,
				ide: "zed",
				default_font_size: 16,
			});

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.state.theme).toBe("commander");
			});
		});

		it("migrates legacy IDE from localStorage", async () => {
			localStorage.setItem("tui-commander-default-ide", "cursor");
			mockInvoke.mockResolvedValueOnce({
				shell: null,
				font_family: "JetBrains Mono",
				font_size: 14,
				theme: "tokyo-night",
				mcp_server_enabled: false,
				ide: "vscode",
				default_font_size: 12,
			}); // load_config for migration
			mockInvoke.mockResolvedValueOnce(undefined); // save_config for migration
			mockInvoke.mockResolvedValueOnce({
				shell: null,
				font_family: "JetBrains Mono",
				font_size: 14,
				theme: "tokyo-night",
				mcp_server_enabled: false,
				ide: "cursor",
				default_font_size: 12,
			}); // load_config after migration

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(localStorage.getItem("tui-commander-default-ide")).toBeNull();
			});
		});

		it("falls back to defaults for invalid values from config", async () => {
			mockInvoke.mockResolvedValueOnce({
				shell: null,
				font_family: "Comic Sans",
				font_size: 14,
				theme: "tokyo-night",
				mcp_server_enabled: false,
				ide: "invalid-ide",
				default_font_size: 12,
			});

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.state.font).toBe("JetBrains Mono");
				expect(store.state.ide).toBe("vscode");
			});
		});

		it("keeps defaults on invoke failure", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("no backend"));

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.state.font).toBe("JetBrains Mono");
				expect(store.state.ide).toBe("vscode");
			});
		});
	});

	describe("setDefaultFontSize()", () => {
		it("clamps font size to valid range", () => {
			testInScope(() => {
				store.setDefaultFontSize(5);
				expect(store.state.defaultFontSize).toBe(8);
				store.setDefaultFontSize(50);
				expect(store.state.defaultFontSize).toBe(32);
				store.setDefaultFontSize(16);
				expect(store.state.defaultFontSize).toBe(16);
			});
		});
	});

	describe("setShell()", () => {
		it("sets custom shell", () => {
			testInScope(() => {
				store.setShell("/bin/zsh");
				expect(store.state.shell).toBe("/bin/zsh");
			});
		});

		it("trims whitespace and sets null for empty string", () => {
			testInScope(() => {
				store.setShell("  ");
				expect(store.state.shell).toBeNull();
			});
		});

		it("persists shell via debounced save", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setShell("/bin/zsh");
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					base: expect.anything(),
					config: expect.objectContaining({ shell: "/bin/zsh" }),
				});
			});
		});
	});

	describe("setProgressTracking()", () => {
		it("defaults to on, and persists the flag through load-modify-save", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				expect(store.state.progressTracking).toBe(true);

				store.setProgressTracking(false);
				expect(store.state.progressTracking).toBe(false);
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();

				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					base: expect.anything(),
					config: expect.objectContaining({ progress_tracking: false }),
				});
			});
		});
	});

	describe("setTheme()", () => {
		it("sets theme and persists via debounced save", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setTheme("dracula");
				expect(store.state.theme).toBe("dracula");
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					base: expect.anything(),
					config: expect.objectContaining({ theme: "dracula" }),
				});
			});
		});
	});

	describe("setSplitTabMode()", () => {
		it("sets split tab mode and persists via debounced save", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setSplitTabMode("unified");
				expect(store.state.splitTabMode).toBe("unified");
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					base: expect.anything(),
					config: expect.objectContaining({ split_tab_mode: "unified" }),
				});
			});
		});
	});

	describe("setConfirmBeforeQuit()", () => {
		it("updates state", () => {
			testInScope(() => {
				store.setConfirmBeforeQuit(false);
				expect(store.state.confirmBeforeQuit).toBe(false);
			});
		});
	});

	describe("setConfirmBeforeClosingTab()", () => {
		it("updates state", () => {
			testInScope(() => {
				store.setConfirmBeforeClosingTab(false);
				expect(store.state.confirmBeforeClosingTab).toBe(false);
			});
		});
	});

	describe("autoShowPrPopover", () => {
		it("defaults to true", () => {
			testInScope(() => {
				expect(store.state.autoShowPrPopover).toBe(true);
			});
		});

		it("sets autoShowPrPopover and persists via debounced save", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setAutoShowPrPopover(false);
				expect(store.state.autoShowPrPopover).toBe(false);
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					base: expect.anything(),
					config: expect.objectContaining({ auto_show_pr_popover: false }),
				});
			});
		});

		it("hydrates autoShowPrPopover from config", async () => {
			mockInvoke.mockResolvedValueOnce({
				shell: null,
				font_family: "JetBrains Mono",
				font_size: 14,
				theme: "tokyo-night",
				mcp_server_enabled: false,
				ide: "vscode",
				default_font_size: 12,
				auto_show_pr_popover: false,
			});
			mockInvoke.mockResolvedValueOnce({ primary_agent: "claude" });

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.state.autoShowPrPopover).toBe(false);
			});
		});
	});

	describe("setIssueFilter()", () => {
		it("updates issueFilter in state", () => {
			testInScope(() => {
				store.setIssueFilter("created");
				expect(store.state.issueFilter).toBe("created");
			});
		});

		it("persists issueFilter via debounced save_config", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setIssueFilter("mentioned");
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					base: expect.anything(),
					config: expect.objectContaining({ issue_filter: "mentioned" }),
				});
			});
		});

		it("defaults to 'assigned' on hydrate with missing issue_filter", async () => {
			mockInvoke.mockResolvedValueOnce({
				shell: null,
				font_family: "JetBrains Mono",
				font_size: 14,
				theme: "dark",
				mcp_server_enabled: false,
				ide: "vscode",
			});
			mockInvoke.mockResolvedValueOnce({ primary_agent: "claude" });

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.state.issueFilter).toBe("assigned");
			});
		});

		it("defaults to 'assigned' on hydrate with invalid issue_filter", async () => {
			mockInvoke.mockResolvedValueOnce({
				shell: null,
				font_family: "JetBrains Mono",
				font_size: 14,
				theme: "dark",
				mcp_server_enabled: false,
				ide: "vscode",
				issue_filter: "bogus_value",
			});
			mockInvoke.mockResolvedValueOnce({ primary_agent: "claude" });

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.state.issueFilter).toBe("assigned");
			});
		});

		it("preserves valid issue_filter on hydrate", async () => {
			mockInvoke.mockResolvedValueOnce({
				shell: null,
				font_family: "JetBrains Mono",
				font_size: 14,
				theme: "dark",
				mcp_server_enabled: false,
				ide: "vscode",
				issue_filter: "all",
			});
			mockInvoke.mockResolvedValueOnce({ primary_agent: "claude" });

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.state.issueFilter).toBe("all");
			});
		});
	});

	describe("PR visibility filters", () => {
		it("defaults prHideDrafts, prHideConflicting, prHideCiFailing to false", () => {
			testInScope(() => {
				expect(store.state.prHideDrafts).toBe(false);
				expect(store.state.prHideConflicting).toBe(false);
				expect(store.state.prHideCiFailing).toBe(false);
			});
		});

		it("setPrHideDrafts updates state", async () => {
			await hydrateStore();
			testInScope(() => {
				store.setPrHideDrafts(true);
				expect(store.state.prHideDrafts).toBe(true);
				store.setPrHideDrafts(false);
				expect(store.state.prHideDrafts).toBe(false);
			});
		});

		it("setPrHideDrafts persists via debounced save_config", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setPrHideDrafts(true);
				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					base: expect.anything(),
					config: expect.objectContaining({ pr_hide_drafts: true }),
				});
			});
		});

		it("setPrHideConflicting updates state", async () => {
			await hydrateStore();
			testInScope(() => {
				store.setPrHideConflicting(true);
				expect(store.state.prHideConflicting).toBe(true);
			});
		});

		it("setPrHideCiFailing updates state", async () => {
			await hydrateStore();
			testInScope(() => {
				store.setPrHideCiFailing(true);
				expect(store.state.prHideCiFailing).toBe(true);
			});
		});

		it("hydrate restores pr filter flags from config", async () => {
			mockInvoke.mockResolvedValueOnce({
				shell: null,
				font_family: "JetBrains Mono",
				font_size: 14,
				theme: "dark",
				mcp_server_enabled: false,
				ide: "vscode",
				pr_hide_drafts: true,
				pr_hide_conflicting: true,
				pr_hide_ci_failing: false,
			});
			mockInvoke.mockResolvedValueOnce({ primary_agent: "claude" });

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.state.prHideDrafts).toBe(true);
				expect(store.state.prHideConflicting).toBe(true);
				expect(store.state.prHideCiFailing).toBe(false);
			});
		});
	});

	describe("custom launchers (#71)", () => {
		const launcher = {
			id: "abc",
			name: "My Editor",
			executable: "code",
			args: ["--goto", "{file}:{line}:{column}"],
			enabled: true,
		};

		it("defaults to an empty list", () => {
			testInScope(() => {
				expect(store.state.customLaunchers).toEqual([]);
			});
		});

		it("stores launchers in state and persists them via save_config", async () => {
			await testInScopeAsync(async () => {
				await hydrateStore();
				store.setCustomLaunchers([launcher]);
				expect(store.state.customLaunchers).toEqual([launcher]);

				vi.advanceTimersByTime(600);
				await vi.runAllTimersAsync();
				expect(mockInvoke).toHaveBeenCalledWith("save_config", {
					base: expect.anything(),
					config: expect.objectContaining({ custom_launchers: [launcher] }),
				});
			});
		});

		it("hydrates custom_launchers from config", async () => {
			mockInvoke.mockResolvedValueOnce({
				font_family: "JetBrains Mono",
				font_size: 14,
				theme: "dark",
				mcp_server_enabled: false,
				ide: "vscode",
				custom_launchers: [launcher],
			});
			mockInvoke.mockResolvedValueOnce({ primary_agent: "claude" });

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.state.customLaunchers).toEqual([launcher]);
			});
		});
	});
	// Catches: saving General settings drops a future background key or ignores the Dock preference.
	it("hydrates and persists the Dock preference while preserving other background settings", async () => {
		mockInvoke.mockResolvedValueOnce({ background: { hide_dock_icon: false } });
		await store.hydrate();
		expect(store.state.hideDockIcon).toBe(false);
		mockInvoke.mockImplementation((command: string) =>
			Promise.resolve(
				command === "load_config" ? { background: { hide_dock_icon: false, webview_unload_minutes: 25 } } : undefined,
			),
		);
		store.setHideDockIcon(true);
		await vi.runAllTimersAsync();
		expect(mockInvoke).toHaveBeenCalledWith("save_config", {
			base: { background: { hide_dock_icon: false, webview_unload_minutes: 25 } },
			config: expect.objectContaining({
				background: { hide_dock_icon: true, webview_unload_minutes: 25 },
			}),
		});
	});
});
