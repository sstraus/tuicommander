import { beforeEach, describe, expect, it, type Mock, vi } from "vitest";
import type { PluginManifest } from "../../plugins/pluginLoader";
import {
	loadPluginsConcurrently,
	loadUserPlugins,
	pluginModuleBaseUrl,
	pluginModuleUrl,
	replayActiveAgents,
	validateManifest,
	validateModule,
} from "../../plugins/pluginLoader";

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
		const clock = vi.spyOn(Date, "now").mockReturnValue(1793461000000);
		try {
			// The runner rejects this native custom-scheme import; no plugin is installed or executed.
			await loadUserPlugins(false);
			await loadUserPlugins(false);
			const failures = (appLogger.error as Mock).mock.calls.filter(([, message]) =>
				message.includes('Failed to import plugin "missing-import-diagnostics"'),
			);
			expect(failures).toHaveLength(2);
			for (const [source, message, data] of failures) {
				expect(source).toBe("plugin");
				// Catches: appLogger coalesces a later failure and drops its new diagnostic data.
				expect(message).toContain(`(load ${data.loadCycleId})`);
				expect(data).toEqual(
					expect.objectContaining({
						url: "plugin://missing-import-diagnostics/~v1793461000000/main.js",
						errorName: "Error",
						// Recorded from Node's native import rejection in the pre-fix RED run.
						errorMessage: expect.stringContaining("Received protocol 'plugin:'"),
						httpStatus: null,
						attempt: 1,
						documentId: performance.timeOrigin,
						loadCycleId: expect.any(String),
					}),
				);
			}
			expect(failures[0][2].loadCycleId).not.toBe(failures[1][2].loadCycleId);
			expect(pluginRegistry.register).not.toHaveBeenCalled();
		} finally {
			clock.mockRestore();
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
