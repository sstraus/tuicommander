import { afterEach, beforeEach, describe, expect, it, type Mock, vi } from "vitest";
import type { PluginManifest } from "../../plugins/pluginLoader";
import {
	loadPluginsConcurrently,
	loadUserPlugins,
	pluginModuleBaseUrl,
	pluginModuleUrl,
	replayActiveAgents,
	setPluginEnabled,
	validateManifest,
	validateModule,
} from "../../plugins/pluginLoader";

import { importPluginModule } from "../../plugins/pluginModuleImport";

vi.mock("../../plugins/pluginModuleImport", () => ({
	importPluginModule: vi.fn((url: string) => import(/* @vite-ignore */ url)),
}));

// Mock invoke
vi.mock("../../invoke", () => ({
	invoke: vi.fn(() => Promise.resolve([])),
	listen: vi.fn(() => Promise.resolve(() => {})),
}));

// Mock pluginRegistry
vi.mock("../../plugins/pluginRegistry", () => ({
	pluginRegistry: {
		register: vi.fn(),
		unregister: vi.fn(),
		notifyStateChange: vi.fn(),
		dispatchStructuredEvent: vi.fn(),
	},
}));

// Mock pluginStore
vi.mock("../../stores/pluginStore", () => ({
	pluginStore: {
		registerPlugin: vi.fn(),
		updatePlugin: vi.fn(),
		getLogger: vi.fn(() => ({ info: vi.fn(), error: vi.fn(), warn: vi.fn() })),
		removePlugin: vi.fn(),
	},
}));

vi.mock("../../stores/appLogger", () => ({
	appLogger: {
		debug: vi.fn(),
		info: vi.fn(),
		warn: vi.fn(),
		error: vi.fn(),
	},
}));

import { invoke, listen } from "../../invoke";
import { pluginRegistry } from "../../plugins/pluginRegistry";
import { appLogger } from "../../stores/appLogger";
import { pluginStore } from "../../stores/pluginStore";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function validManifest(overrides: Partial<PluginManifest> = {}): PluginManifest {
	return {
		id: "test-plugin",
		name: "Test Plugin",
		version: "1.0.0",
		minAppVersion: "0.1.0",
		main: "main.js",
		capabilities: [],
		...overrides,
	};
}

// ---------------------------------------------------------------------------
// validateManifest
// ---------------------------------------------------------------------------

describe("validateManifest", () => {
	it("accepts a valid manifest", () => {
		expect(validateManifest(validManifest())).toBeNull();
	});

	it("rejects missing id", () => {
		expect(validateManifest(validManifest({ id: "" }))).toContain("id");
	});

	it("rejects missing name", () => {
		expect(validateManifest(validManifest({ name: "" }))).toContain("name");
	});

	it("rejects missing version", () => {
		expect(validateManifest(validManifest({ version: "" }))).toContain("version");
	});

	it("rejects missing main", () => {
		expect(validateManifest(validManifest({ main: "" }))).toContain("main");
	});

	it("rejects minAppVersion higher than current app version", () => {
		const error = validateManifest(validManifest({ minAppVersion: "99.0.0" }));
		expect(error).toContain("requires app version");
	});

	it("accepts minAppVersion equal to current", () => {
		// App is 0.3.0
		expect(validateManifest(validManifest({ minAppVersion: "0.3.0" }))).toBeNull();
	});

	it("accepts minAppVersion lower than current", () => {
		expect(validateManifest(validManifest({ minAppVersion: "0.1.0" }))).toBeNull();
	});
});

// ---------------------------------------------------------------------------
// validateModule
// ---------------------------------------------------------------------------

describe("validateModule", () => {
	it("accepts a valid module with default export", () => {
		const mod = {
			default: { id: "test", onload: () => {}, onunload: () => {} },
		};
		expect(validateModule(mod, "test")).toBeNull();
	});

	it("rejects module without default export", () => {
		expect(validateModule({}, "test")).toContain("default export");
	});

	it("rejects module where default is not an object", () => {
		expect(validateModule({ default: "string" }, "test")).toContain("default export");
	});

	it("rejects module missing onload", () => {
		const mod = { default: { id: "test", onunload: () => {} } };
		expect(validateModule(mod, "test")).toContain("onload");
	});

	it("rejects module missing onunload", () => {
		const mod = { default: { id: "test", onload: () => {} } };
		expect(validateModule(mod, "test")).toContain("onunload");
	});

	it("rejects module where id doesn't match manifest", () => {
		const mod = {
			default: { id: "wrong-id", onload: () => {}, onunload: () => {} },
		};
		expect(validateModule(mod, "expected-id")).toContain("id mismatch");
	});
});

// ---------------------------------------------------------------------------
// pluginModuleBaseUrl
// ---------------------------------------------------------------------------

describe("pluginModuleBaseUrl", () => {
	it("uses the raw plugin:// scheme off Windows (macOS/Linux)", () => {
		// wry serves custom schemes under their raw scheme on macOS/Linux.
		expect(pluginModuleBaseUrl("mdkb-dashboard", "main.js", false)).toBe("plugin://mdkb-dashboard/main.js");
	});

	it("uses http://plugin.localhost on Windows (WebView2)", () => {
		// WebView2 only serves register_uri_scheme_protocol schemes via
		// http://{scheme}.localhost — see issue #86. plugins.rs parses this form.
		expect(pluginModuleBaseUrl("mdkb-dashboard", "main.js", true)).toBe(
			"http://plugin.localhost/mdkb-dashboard/main.js",
		);
	});
});

describe("pluginModuleUrl", () => {
	// The browser resolves `import "./provider.js"` against the importing module's URL
	// and keys its module map on the result, so what a reload must change is the URL
	// that sub-modules resolve to, not only the entry URL.
	const resolveSibling = (entry: string) => new URL("./provider.js", entry).href;

	it("gives a sub-module a different URL on every reload", () => {
		const first = pluginModuleUrl("github-work", "main.js", false, 1000);
		const second = pluginModuleUrl("github-work", "main.js", false, 2000);
		expect(resolveSibling(first)).not.toBe(resolveSibling(second));
	});

	it("keeps sub-modules of one load on one URL, so the graph shares instances", () => {
		const entry = pluginModuleUrl("github-work", "main.js", false, 1000);
		expect(resolveSibling(entry)).toBe(resolveSibling(pluginModuleUrl("github-work", "main.js", false, 1000)));
	});

	it("resolves a parent-directory import under the same load version", () => {
		const entry = pluginModuleUrl("github-work", "src/main.js", false, 1000);
		expect(new URL("../lib/model.js", entry).href).toBe("plugin://github-work/~v1000/lib/model.js");
	});

	it("uses the http://plugin.localhost form on Windows", () => {
		expect(pluginModuleUrl("github-work", "main.js", true, 1000)).toBe(
			"http://plugin.localhost/github-work/~v1000/main.js",
		);
	});
});

// ---------------------------------------------------------------------------
// loadUserPlugins
// ---------------------------------------------------------------------------

describe("loadUserPlugins", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		(invoke as Mock).mockResolvedValue([]);
	});

	it("logs import URL and cause per load — catches import failures without actionable provenance", async () => {
		const manifest = validManifest({ id: "missing-import-diagnostics" });
		(invoke as Mock).mockResolvedValue([manifest]);
		// Keep the native Node rejection as the diagnostic oracle, not an invented WebView error.
		let nativeError: unknown;
		try {
			await importPluginModule("plugin://missing-import-diagnostics/main.js");
		} catch (error) {
			nativeError = error;
		}
		expect(nativeError).toBeDefined();
		vi.mocked(importPluginModule).mockRejectedValue(nativeError);
		vi.useFakeTimers();
		const clock = vi.spyOn(Date, "now").mockReturnValue(1793461000000);
		try {
			for (let cycle = 0; cycle < 2; cycle++) {
				const loading = loadUserPlugins(false);
				await vi.runAllTimersAsync();
				await loading;
			}
			const failures = (appLogger.error as Mock).mock.calls.filter(([, message]) =>
				message.includes('Failed to import plugin "missing-import-diagnostics"'),
			);
			expect(failures).toHaveLength(6);
			for (const [index, [source, message, data]] of failures.entries()) {
				expect(source).toBe("plugin");
				// Catches: appLogger coalesces a later attempt and drops its diagnostic data.
				expect(message).toContain(`(load ${data.loadCycleId}, attempt ${data.attempt})`);
				expect(data).toEqual(
					expect.objectContaining({
						url: expect.stringMatching(/^plugin:\/\/missing-import-diagnostics\/~v\d+\/main\.js$/),
						errorName: "Error",
						errorMessage: expect.stringContaining("Received protocol 'plugin:'"),
						httpStatus: null,
						attempt: (index % 3) + 1,
						documentId: performance.timeOrigin,
						loadCycleId: expect.any(String),
					}),
				);
			}
			expect(new Set(failures.map(([, , data]) => data.url)).size).toBe(6);
			expect(new Set(failures.slice(0, 3).map(([, , data]) => data.loadCycleId)).size).toBe(1);
			expect(failures[0][2].loadCycleId).not.toBe(failures[3][2].loadCycleId);
			expect(pluginRegistry.register).not.toHaveBeenCalled();
		} finally {
			clock.mockRestore();
			vi.useRealTimers();
			vi.mocked(importPluginModule).mockImplementation((url) => import(/* @vite-ignore */ url));
		}
	});

	it("calls list_user_plugins to discover plugins", async () => {
		await loadUserPlugins();
		expect(invoke).toHaveBeenCalledWith("list_user_plugins");
	});

	it("discovers only once for a multi-plugin reload batch — catches repeated whole-store scans", async () => {
		await loadUserPlugins(false);
		const handler = (listen as Mock).mock.calls.find(([event]) => event === "plugin-changed")?.[1];
		expect(handler).toBeTypeOf("function");
		vi.clearAllMocks();
		await handler({ payload: ["plugin-a", "plugin-b", "plugin-a"] });
		expect(invoke).toHaveBeenCalledTimes(1);
		expect(invoke).toHaveBeenCalledWith("list_user_plugins", { pluginIds: ["plugin-a", "plugin-b"] });
	});

	it("coalesces overlapping reload notifications — catches concurrent duplicate discovery", async () => {
		await loadUserPlugins(false);
		const handler = (listen as Mock).mock.calls.find(([event]) => event === "plugin-changed")?.[1];
		vi.clearAllMocks();
		await Promise.all([handler({ payload: ["plugin-a"] }), handler({ payload: ["plugin-b"] })]);
		expect(invoke).toHaveBeenCalledTimes(1);
		expect(invoke).toHaveBeenCalledWith("list_user_plugins", { pluginIds: ["plugin-a", "plugin-b"] });
	});

	it("registers listen handler for plugin-changed events", async () => {
		await loadUserPlugins();
		expect(listen).toHaveBeenCalledWith("plugin-changed", expect.any(Function));
	});

	it("can reuse an already-synced disabled list", async () => {
		await loadUserPlugins(false);

		expect(invoke).not.toHaveBeenCalledWith("load_config");
	});

	it("loads valid user plugins concurrently", async () => {
		const manifests = [validManifest({ id: "plugin-a" }), validManifest({ id: "plugin-b" })];
		const releases: Array<() => void> = [];
		const loader = vi.fn(() => new Promise<void>((resolve) => releases.push(resolve)));

		const loading = loadPluginsConcurrently(manifests, loader);
		await vi.waitFor(() => expect(loader).toHaveBeenCalledTimes(2));
		for (const release of releases) release();
		await loading;
	});

	it("skips plugins that fail manifest validation", async () => {
		(invoke as Mock).mockResolvedValue([validManifest({ minAppVersion: "99.0.0" })]);
		await loadUserPlugins();
		expect(pluginRegistry.register).not.toHaveBeenCalled();
	});

	it("does not crash when list_user_plugins rejects", async () => {
		(invoke as Mock).mockRejectedValue(new Error("Tauri not available"));
		// Should not throw — just log and return
		await expect(loadUserPlugins()).resolves.toBeUndefined();
	});

	it("registers disabled plugins in store without loading them", async () => {
		const manifest = validManifest({ id: "disabled-plugin" });
		(invoke as Mock).mockImplementation((cmd: string) => {
			if (cmd === "load_config") return Promise.resolve({ disabled_plugin_ids: ["disabled-plugin"] });
			if (cmd === "list_user_plugins") return Promise.resolve([manifest]);
			return Promise.resolve(undefined);
		});

		await loadUserPlugins();

		// Plugin is disabled — should NOT be loaded into registry
		expect(pluginRegistry.register).not.toHaveBeenCalled();

		// Plugin IS registered in pluginStore as disabled
		expect(pluginStore.registerPlugin).toHaveBeenCalledWith(
			"disabled-plugin",
			expect.objectContaining({
				manifest,
				enabled: false,
				loaded: false,
			}),
		);
	});
});

// Catches: hot reload loses an already open question and agent identity.
it("replays awaiting and agent type when reloading into an open question", async () => {
	const { terminalsStore } = await import("../../stores/terminals");
	const id = terminalsStore.add({
		sessionId: "reload-question",
		agentType: "claude",
		fontSize: 14,
		name: "Reload",
		cwd: "/repo",
		awaitingInput: null,
	});
	terminalsStore.update(id, { shellState: "idle", awaitingInput: "question", awaitingInputConfident: true });
	replayActiveAgents(terminalsStore);
	expect(pluginRegistry.dispatchStructuredEvent).toHaveBeenCalledWith(
		"shell-state",
		{ state: "idle", agent_type: "claude" },
		"reload-question",
	);
	expect(pluginRegistry.dispatchStructuredEvent).toHaveBeenCalledWith(
		"awaiting",
		{ awaiting: true, confident: true },
		"reload-question",
	);
	terminalsStore.remove(id);
});

describe("import recovery", () => {
	const module = { default: { id: "recover", onload: () => {}, onunload: () => {} } };
	beforeEach(async () => {
		vi.clearAllMocks();
		vi.useFakeTimers();
		(invoke as Mock).mockImplementation(async (cmd: string) =>
			cmd === "load_config"
				? { disabled_plugin_ids: [] }
				: cmd === "list_user_plugins"
					? [validManifest({ id: "recover" })]
					: undefined,
		);
		(pluginRegistry.register as Mock).mockResolvedValue(true);
		await import("../../plugins/pluginLoader").then((loader) => loader.syncDisabledList());
	});
	afterEach(() => {
		vi.useRealTimers();
		vi.mocked(importPluginModule).mockReset();
	});

	it("recovers a rejected import without duplicate activation — catches enabled plugins stranded unloaded", async () => {
		vi.mocked(importPluginModule).mockRejectedValueOnce(new Error("temporary")).mockResolvedValue(module);
		const loading = loadUserPlugins(false);
		await vi.advanceTimersByTimeAsync(999);
		expect(importPluginModule).toHaveBeenCalledTimes(1);
		expect(pluginRegistry.register).not.toHaveBeenCalled();
		await vi.advanceTimersByTimeAsync(1);
		await loading;
		expect(pluginRegistry.register).toHaveBeenCalledExactlyOnceWith(module.default, [], undefined);
		expect(importPluginModule).toHaveBeenCalledTimes(2);
		expect(new Set(vi.mocked(importPluginModule).mock.calls.map(([url]) => url)).size).toBe(2);
	});

	it("bounds permanent import failure and preserves its error — catches endless or silent recovery", async () => {
		vi.mocked(importPluginModule).mockRejectedValue(new Error("permanent"));
		const loading = loadUserPlugins(false);
		await vi.runAllTimersAsync();
		await loading;
		expect(importPluginModule).toHaveBeenCalledTimes(3);
		expect(pluginRegistry.register).not.toHaveBeenCalled();
		expect(pluginStore.updatePlugin).toHaveBeenLastCalledWith("recover", { error: "permanent" });
		expect(vi.getTimerCount()).toBe(0);
	});

	it("does not retry invalid modules or failed activation — catches repeated plugin side effects", async () => {
		vi.mocked(importPluginModule).mockResolvedValueOnce({ default: {} });
		await loadUserPlugins(false);
		expect(importPluginModule).toHaveBeenCalledTimes(1);
		expect(pluginRegistry.register).not.toHaveBeenCalled();
		vi.mocked(importPluginModule).mockResolvedValue(module);
		(pluginRegistry.register as Mock).mockResolvedValue(false);
		await loadUserPlugins(false);
		expect(importPluginModule).toHaveBeenCalledTimes(2);
		expect(pluginRegistry.register).toHaveBeenCalledTimes(1);
	});

	it("cancels recovery when disabled — catches delayed activation after an explicit disable", async () => {
		vi.mocked(importPluginModule).mockRejectedValueOnce(new Error("temporary")).mockResolvedValue(module);
		const loading = loadUserPlugins(false);
		await vi.advanceTimersByTimeAsync(0);
		await setPluginEnabled("recover", false);
		await vi.runAllTimersAsync();
		await loading;
		expect(importPluginModule).toHaveBeenCalledTimes(1);
		expect(pluginRegistry.register).not.toHaveBeenCalled();
	});

	it("ignores a successful pending import after disable — catches activation from a stale module result", async () => {
		let finish!: (value: unknown) => void;
		vi.mocked(importPluginModule).mockImplementation(
			() =>
				new Promise((resolve) => {
					finish = resolve;
				}),
		);
		const loading = loadUserPlugins(false);
		await vi.advanceTimersByTimeAsync(0);
		await setPluginEnabled("recover", false);
		finish(module);
		await loading;
		expect(pluginRegistry.register).not.toHaveBeenCalled();
		expect(vi.getTimerCount()).toBe(0);
	});

	it("retires the old recovery on reload — catches duplicate activation from stale imports", async () => {
		vi.mocked(importPluginModule).mockRejectedValueOnce(new Error("temporary")).mockResolvedValue(module);
		const loading = loadUserPlugins(false);
		await vi.advanceTimersByTimeAsync(0);
		const handler = (listen as Mock).mock.calls.find(([event]) => event === "plugin-changed")?.[1];
		await handler({ payload: ["recover"] });
		await vi.runAllTimersAsync();
		await loading;
		expect(importPluginModule).toHaveBeenCalledTimes(2);
		expect(pluginRegistry.register).toHaveBeenCalledTimes(1);
	});
});
