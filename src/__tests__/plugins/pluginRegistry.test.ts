import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "../../invoke";
import { dashboardRegistry } from "../../plugins/dashboardRegistry";
import { filePreviewRegistry } from "../../plugins/filePreviewRegistry";
import { markdownProviderRegistry } from "../../plugins/markdownProviderRegistry";
import { pluginRegistry } from "../../plugins/pluginRegistry";
import type { PluginHost, TuiPlugin } from "../../plugins/types";
import { PluginCapabilityError } from "../../plugins/types";
import { activityStore } from "../../stores/activityStore";
import { contextMenuActionsStore } from "../../stores/contextMenuActionsStore";
import { mdTabsStore } from "../../stores/mdTabs";
import { pluginStore } from "../../stores/pluginStore";
import { repositoriesStore } from "../../stores/repositories";
import { statusBarTicker } from "../../stores/statusBarTicker";
import { terminalsStore } from "../../stores/terminals";

// Mock invoke to avoid Tauri internals in test environment.
// `listen` records every subscription so a test can fire an event at exactly
// one event name and see who woke up.
const listeners = new Map<string, Array<(event: { payload: unknown }) => void>>();
vi.mock("../../invoke", () => ({
	invoke: vi.fn(() => Promise.resolve()),
	listen: vi.fn((name: string, handler: (event: { payload: unknown }) => void) => {
		const forName = listeners.get(name) ?? [];
		forName.push(handler);
		listeners.set(name, forName);
		return Promise.resolve(() => {
			listeners.set(
				name,
				(listeners.get(name) ?? []).filter((h) => h !== handler),
			);
		});
	}),
}));

/** Deliver a payload to everything listening on that exact event name. */
function emitTauri(name: string, payload: unknown): void {
	for (const handler of listeners.get(name) ?? []) handler({ payload });
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Flush pending queueMicrotask callbacks so deferred dispatch handlers run */
const flushMicrotasks = () => new Promise<void>((resolve) => queueMicrotask(resolve));

function makePlugin(id: string, onload?: (host: PluginHost) => void, onunload?: () => void): TuiPlugin {
	return {
		id,
		onload: onload ?? (() => {}),
		onunload: onunload ?? (() => {}),
	};
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
	pluginRegistry.clear();
	activityStore.clearAll();
	contextMenuActionsStore.clear();
	filePreviewRegistry.clear();
	markdownProviderRegistry.clear();
	pluginStore.clear();
	statusBarTicker.clear();
	mdTabsStore.clearAll();
});

afterEach(() => {
	repositoriesStore._testCancelPendingSave();
});

// ---------------------------------------------------------------------------
// Plugin lifecycle
// ---------------------------------------------------------------------------

describe("register / unregister", () => {
	it("calls plugin.onload when registered", () => {
		const onload = vi.fn();
		pluginRegistry.register(makePlugin("p1", onload));
		expect(onload).toHaveBeenCalledOnce();
	});

	it("calls plugin.onunload when unregistered", () => {
		const onunload = vi.fn();
		pluginRegistry.register(makePlugin("p1", undefined, onunload));
		pluginRegistry.unregister("p1");
		expect(onunload).toHaveBeenCalledOnce();
	});

	it("unregister is a no-op for unknown plugin id", () => {
		expect(() => pluginRegistry.unregister("unknown")).not.toThrow();
	});

	it("re-registering same id replaces the old plugin", () => {
		const onunload1 = vi.fn();
		const onload2 = vi.fn();
		pluginRegistry.register(makePlugin("p1", undefined, onunload1));
		pluginRegistry.register(makePlugin("p1", onload2));
		expect(onunload1).toHaveBeenCalledOnce();
		expect(onload2).toHaveBeenCalledOnce();
	});
});

// ---------------------------------------------------------------------------
// PluginHost — section delegation
// ---------------------------------------------------------------------------

describe("PluginHost.registerSection", () => {
	it("delegates to activityStore and section appears in getSections", () => {
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				host.registerSection({ id: "test-section", label: "TEST", priority: 10, canDismissAll: false });
			}),
		);
		expect(activityStore.getSections().some((s) => s.id === "test-section")).toBe(true);
	});

	it("disposing host registration removes section from activityStore", () => {
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				const d = host.registerSection({ id: "s1", label: "S1", priority: 10, canDismissAll: false });
				d.dispose();
			}),
		);
		expect(activityStore.getSections().some((s) => s.id === "s1")).toBe(false);
	});

	it("unregistering plugin auto-disposes section", () => {
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				host.registerSection({ id: "s2", label: "S2", priority: 10, canDismissAll: false });
			}),
		);
		pluginRegistry.unregister("p1");
		expect(activityStore.getSections().some((s) => s.id === "s2")).toBe(false);
	});

	it("double dispose is idempotent (no crash)", () => {
		let disposable: { dispose: () => void } | null = null;
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				disposable = host.registerSection({ id: "s3", label: "S3", priority: 10, canDismissAll: false });
			}),
		);
		// Plugin manually disposes, then unregister disposes again — must not throw
		disposable!.dispose();
		expect(() => pluginRegistry.unregister("p1")).not.toThrow();
	});
});

// ---------------------------------------------------------------------------
// PluginHost — item delegation
// ---------------------------------------------------------------------------

describe("PluginHost.addItem / removeItem / updateItem", () => {
	const baseItem = () => ({
		id: "item-1",
		pluginId: "p1",
		sectionId: "s1",
		title: "Test",
		icon: "<svg/>",
		dismissible: true,
	});

	it("addItem adds to activityStore", () => {
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				host.addItem(baseItem());
			}),
		);
		expect(activityStore.getActive().some((i) => i.id === "item-1")).toBe(true);
	});

	it("activity item onClick errors are caught and logged", () => {
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				host.addItem({
					...baseItem(),
					onClick: () => {
						throw new Error("activity boom");
					},
				});
			}),
		);
		const item = activityStore.getActive().find((i) => i.id === "item-1");
		expect(item?.onClick).toBeDefined();

		expect(() => item?.onClick?.()).not.toThrow();
		expect(
			pluginStore
				.getLogger("p1")
				.getEntries()
				.some((e) => e.level === "error" && e.message.includes("activity")),
		).toBe(true);
	});

	it("removeItem removes from activityStore", () => {
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				host.addItem(baseItem());
				host.removeItem("item-1");
			}),
		);
		expect(activityStore.getActive().find((i) => i.id === "item-1")).toBeUndefined();
	});

	it("updateItem updates title in activityStore", () => {
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				host.addItem(baseItem());
				host.updateItem("item-1", { title: "Updated" });
			}),
		);
		expect(activityStore.getActive().find((i) => i.id === "item-1")?.title).toBe("Updated");
	});

	it("updateItem wraps replacement onClick handlers", () => {
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				host.addItem(baseItem());
				host.updateItem("item-1", {
					onClick: () => {
						throw new Error("updated boom");
					},
				});
			}),
		);
		const item = activityStore.getActive().find((i) => i.id === "item-1");

		expect(() => item?.onClick?.()).not.toThrow();
		expect(
			pluginStore
				.getLogger("p1")
				.getEntries()
				.some((e) => e.level === "error" && e.message.includes("activity")),
		).toBe(true);
	});
});

describe("PluginHost callback guards", () => {
	it("file preview onOpen errors are caught and logged", () => {
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				host.registerFilePreview({
					extensions: ["xyz"],
					onOpen: () => {
						throw new Error("preview boom");
					},
				});
			}),
		);
		const handler = filePreviewRegistry.getHandler("demo.xyz");
		expect(handler).toBeDefined();

		expect(() => handler?.onOpen({ filePath: "demo.xyz", repoPath: "/repo", fsRoot: "/repo" })).not.toThrow();
		expect(
			pluginStore
				.getLogger("p1")
				.getEntries()
				.some((e) => e.level === "error" && e.message.includes("file preview")),
		).toBe(true);
	});

	it("context menu action and disabled callback errors are caught and logged", () => {
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				host.registerContextMenuAction({
					id: "explode",
					label: "Explode",
					target: "repo",
					disabled: () => {
						throw new Error("disabled boom");
					},
					action: () => {
						throw new Error("action boom");
					},
				});
			}),
		);
		const action = contextMenuActionsStore.getContextActions("repo")[0];

		expect(action.disabled?.({ target: "repo" })).toBe(true);
		expect(() => action.action({ target: "repo" })).not.toThrow();
		const entries = pluginStore.getLogger("p1").getEntries();
		expect(entries.some((e) => e.level === "error" && e.message.includes("disabled"))).toBe(true);
		expect(entries.some((e) => e.level === "error" && e.message.includes("context menu action"))).toBe(true);
	});

	it("terminal action errors are caught and logged", () => {
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				host.registerTerminalAction({
					id: "explode",
					label: "Explode",
					action: () => {
						throw new Error("terminal boom");
					},
				});
			}),
		);
		const action = contextMenuActionsStore.getActions()[0];

		expect(() => action.action({ sessionId: "s1", repoPath: "/repo" })).not.toThrow();
		expect(
			pluginStore
				.getLogger("p1")
				.getEntries()
				.some((e) => e.level === "error" && e.message.includes("terminal action")),
		).toBe(true);
	});

	it("ticker and dashboard callbacks are caught and logged", async () => {
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				host.setTicker({
					id: "tick",
					text: "Tick",
					onClick: () => {
						throw new Error("ticker boom");
					},
				});
				host.registerDashboard({
					open: () => {
						throw new Error("dashboard boom");
					},
				});
			}),
		);

		expect(() => statusBarTicker.getAll()[0].onClick?.()).not.toThrow();
		await expect(dashboardRegistry.get("p1")?.open()).resolves.toBeUndefined();
		const entries = pluginStore.getLogger("p1").getEntries();
		expect(entries.some((e) => e.level === "error" && e.message.includes("ticker"))).toBe(true);
		expect(entries.some((e) => e.level === "error" && e.message.includes("dashboard"))).toBe(true);
	});
});

describe("PluginHost.openMarkdownFileBackground", () => {
	it("opens an owned file without changing the active markdown tab and deduplicates it", async () => {
		const repoPath = "/plugin-background-repo";
		repositoriesStore.add({ path: repoPath, displayName: "background" });
		const activeId = mdTabsStore.addVirtual("Existing", "test:existing");
		let opened = false;

		try {
			await pluginRegistry.register(
				makePlugin("background-opener", (host) => {
					opened = host.openMarkdownFileBackground(`${repoPath}/plans/feature.md`);
					expect(host.openMarkdownFileBackground(`${repoPath}/plans/feature.md`)).toBe(true);
				}),
				["ui:markdown"],
			);

			expect(opened).toBe(true);
			expect(mdTabsStore.state.activeId).toBe(activeId);
			expect(mdTabsStore.getIds()).toHaveLength(2);
			expect(
				mdTabsStore.getIds().some((id) => {
					const tab = mdTabsStore.get(id);
					return tab?.type === "file" && tab.filePath === "plans/feature.md";
				}),
			).toBe(true);
		} finally {
			repositoriesStore.remove(repoPath);
		}
	});

	it("refuses an absolute path outside every registered repository", async () => {
		let result = true;
		await pluginRegistry.register(
			makePlugin("background-opener", (host) => {
				result = host.openMarkdownFileBackground("/unowned/plans/feature.md");
			}),
			["ui:markdown"],
		);

		expect(result).toBe(false);
		expect(mdTabsStore.getIds()).toHaveLength(0);
	});

	it("requires the ui:markdown capability", async () => {
		await pluginRegistry.register(
			makePlugin("background-opener", (host) => {
				expect(() => host.openMarkdownFileBackground("/repo/plan.md")).toThrow(PluginCapabilityError);
			}),
			[],
		);
	});
});

// ---------------------------------------------------------------------------
// PluginHost — markdown provider delegation
// ---------------------------------------------------------------------------

describe("PluginHost.registerMarkdownProvider", () => {
	it("delegates to markdownProviderRegistry", async () => {
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				host.registerMarkdownProvider("plan", { provideContent: () => "# Plan" });
			}),
		);
		const result = await markdownProviderRegistry.resolve("plan:file?path=/foo.md");
		expect(result).toBe("# Plan");
	});

	it("unregistering plugin auto-disposes markdown provider", async () => {
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				host.registerMarkdownProvider("plan", { provideContent: () => "# Plan" });
			}),
		);
		pluginRegistry.unregister("p1");
		const result = await markdownProviderRegistry.resolve("plan:file");
		expect(result).toBeNull();
	});
});

// ---------------------------------------------------------------------------
// OutputWatcher dispatch
// ---------------------------------------------------------------------------

describe("dispatchLine", () => {
	it("calls matching watcher with match and sessionId", async () => {
		const onMatch = vi.fn();
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				host.registerOutputWatcher({ pattern: /hello (\w+)/, onMatch });
			}),
		);
		pluginRegistry.dispatchLine("hello world", "session-1");
		await flushMicrotasks();
		expect(onMatch).toHaveBeenCalledOnce();
		expect(onMatch.mock.calls[0][0][1]).toBe("world");
		expect(onMatch.mock.calls[0][1]).toBe("session-1");
	});

	it("does not call watcher when pattern does not match", async () => {
		const onMatch = vi.fn();
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				host.registerOutputWatcher({ pattern: /hello/, onMatch });
			}),
		);
		pluginRegistry.dispatchLine("goodbye world", "s1");
		await flushMicrotasks();
		expect(onMatch).not.toHaveBeenCalled();
	});

	it("resets lastIndex on global regex before each test", async () => {
		const onMatch = vi.fn();
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				host.registerOutputWatcher({ pattern: /foo/g, onMatch });
			}),
		);
		pluginRegistry.dispatchLine("foo bar", "s1");
		await flushMicrotasks();
		pluginRegistry.dispatchLine("foo bar", "s1");
		await flushMicrotasks();
		expect(onMatch).toHaveBeenCalledTimes(2);
	});

	it("catches and does not rethrow watcher exceptions", async () => {
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				host.registerOutputWatcher({
					pattern: /anything/,
					onMatch: () => {
						throw new Error("watcher boom");
					},
				});
			}),
		);
		pluginRegistry.dispatchLine("anything", "s1");
		await flushMicrotasks();
		// Exception is caught inside the microtask — no unhandled error
	});

	it("continues dispatching to other watchers after one throws", async () => {
		const onMatch = vi.fn();
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				host.registerOutputWatcher({
					pattern: /anything/,
					onMatch: () => {
						throw new Error("boom");
					},
				});
				host.registerOutputWatcher({ pattern: /anything/, onMatch });
			}),
		);
		pluginRegistry.dispatchLine("anything", "s1");
		await flushMicrotasks();
		expect(onMatch).toHaveBeenCalledOnce();
	});

	it("unregistering plugin removes its watchers", async () => {
		const onMatch = vi.fn();
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				host.registerOutputWatcher({ pattern: /hello/, onMatch });
			}),
		);
		pluginRegistry.unregister("p1");
		pluginRegistry.dispatchLine("hello", "s1");
		await flushMicrotasks();
		expect(onMatch).not.toHaveBeenCalled();
	});
});

// ---------------------------------------------------------------------------
// handleWatcherLines — the Rust reader assembles the lines, this dispatches them
// ---------------------------------------------------------------------------

/** Read the payload of the last set_plugin_output_watchers invoke. */
async function lastSync(): Promise<{
	clientId: string;
	seq: number;
	watchers: Array<{ id: string; pattern: string; flags: string }>;
}> {
	const { invoke } = await import("../../invoke");
	const calls = vi.mocked(invoke).mock.calls.filter((c) => c[0] === "set_plugin_output_watchers");
	return calls.at(-1)?.[1] as never;
}

describe("handleWatcherLines", () => {
	it("pushes every registered watcher to the backend as source + flags, tagged with client and seq", async () => {
		const { invoke } = await import("../../invoke");
		vi.mocked(invoke).mockClear();
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				host.registerOutputWatcher({ pattern: /model is at capacity/i, onMatch: vi.fn() });
			}),
		);

		const { clientId, seq, watchers } = await lastSync();
		expect(watchers).toHaveLength(1);
		expect(watchers[0].pattern).toBe("model is at capacity");
		expect(watchers[0].flags).toBe("i");
		expect(watchers[0].id).toBeTruthy();
		expect(clientId).toBeTruthy();
		// The backend orders mutations by this counter, so it must climb.
		expect(seq).toBeGreaterThan(0);
	});

	it("fires a watcher the backend matched, with its capture groups", async () => {
		const { invoke } = await import("../../invoke");
		vi.mocked(invoke).mockClear();
		vi.mocked(invoke).mockResolvedValueOnce({ applied: true, rejected: [] } as unknown as undefined);
		const onMatch = vi.fn();
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				host.registerOutputWatcher({ pattern: /capacity (\w+)/, onMatch });
			}),
		);
		await flushMicrotasks();

		const { clientId, watchers } = await lastSync();
		pluginRegistry.handleWatcherLines("s1", [
			{ text: "capacity exceeded", matched_ids: [`${clientId}/${watchers[0].id}`] },
		]);
		await flushMicrotasks();

		expect(onMatch).toHaveBeenCalledOnce();
		expect(onMatch.mock.calls[0][0][1]).toBe("exceeded");
		expect(onMatch.mock.calls[0][1]).toBe("s1");
	});

	it("ignores a match qualified with another frontend's client id", async () => {
		const { invoke } = await import("../../invoke");
		vi.mocked(invoke).mockResolvedValueOnce({ applied: true, rejected: [] } as unknown as undefined);
		const onMatch = vi.fn();
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				host.registerOutputWatcher({ pattern: /capacity/, onMatch });
			}),
		);
		await flushMicrotasks();

		const { watchers } = await lastSync();
		// Same watcher id, different client: a browser tab's set travels on the
		// same event and must not fire this window's watchers.
		pluginRegistry.handleWatcherLines("s1", [
			{ text: "capacity exceeded", matched_ids: [`other-client/${watchers[0].id}`] },
		]);
		await flushMicrotasks();
		expect(onMatch).not.toHaveBeenCalled();
	});

	it("fires a watcher exactly once when the backend flagged it and the line is delivered too", async () => {
		const { invoke } = await import("../../invoke");
		vi.mocked(invoke).mockClear();
		// The reply has not landed yet: the watcher is still `inRust: false` here
		// while Rust already matches it — the acknowledgement window.
		let ack: (v: unknown) => void = () => {};
		vi.mocked(invoke).mockImplementationOnce(
			() => new Promise((r) => (ack = r as (v: unknown) => void)) as Promise<void>,
		);
		const onMatch = vi.fn();
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				host.registerOutputWatcher({ pattern: /done/, onMatch });
			}),
		);
		await flushMicrotasks();

		const { clientId, watchers } = await lastSync();
		pluginRegistry.handleWatcherLines("s1", [{ text: "done", matched_ids: [`${clientId}/${watchers[0].id}`] }]);
		await flushMicrotasks();
		expect(onMatch).toHaveBeenCalledOnce();

		ack({ applied: true, rejected: [] });
		await flushMicrotasks();
	});

	it("matches a pattern the backend rejected against the lines it ships", async () => {
		const { invoke } = await import("../../invoke");
		const onMatch = vi.fn();
		vi.mocked(invoke).mockImplementationOnce((_cmd, args) => {
			const { watchers } = args as { watchers: Array<{ id: string }> };
			return Promise.resolve({ applied: true, rejected: [watchers[0].id] }) as unknown as Promise<void>;
		});
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				// Rust cannot compile lookahead.
				host.registerOutputWatcher({ pattern: /done(?= now)/, onMatch });
			}),
		);
		await flushMicrotasks();

		pluginRegistry.handleWatcherLines("s1", [{ text: "done now", matched_ids: [] }]);
		await flushMicrotasks();
		expect(onMatch).toHaveBeenCalledOnce();
	});

	it("ignores a stale sync reply that resolves after a newer one", async () => {
		const { invoke } = await import("../../invoke");
		vi.mocked(invoke).mockClear();
		const onMatch = vi.fn();
		// First sync (one watcher) is slow and answers "nothing rejected"; the
		// second sync (two watchers) answers first and rejects the new pattern.
		let resolveFirst: (v: unknown) => void = () => {};
		vi.mocked(invoke)
			.mockImplementationOnce(() => new Promise((r) => (resolveFirst = r as (v: unknown) => void)) as Promise<void>)
			.mockImplementationOnce((_cmd, args) => {
				const { watchers } = args as { watchers: Array<{ id: string }> };
				return Promise.resolve({ applied: true, rejected: [watchers[1].id] }) as unknown as Promise<void>;
			});

		pluginRegistry.register(
			makePlugin("p1", (host) => {
				host.registerOutputWatcher({ pattern: /first/, onMatch: vi.fn() });
				host.registerOutputWatcher({ pattern: /second(?= now)/, onMatch });
			}),
		);
		await flushMicrotasks();
		resolveFirst({ applied: true, rejected: [] });
		await flushMicrotasks();

		// The stale "nothing rejected" reply must not claim the lookahead watcher.
		pluginRegistry.handleWatcherLines("s1", [{ text: "second now", matched_ids: [] }]);
		await flushMicrotasks();
		expect(onMatch).toHaveBeenCalledOnce();
	});

	it("keeps matching in the WebView when the backend refuses the sync as stale", async () => {
		const { invoke } = await import("../../invoke");
		const onMatch = vi.fn();
		// `applied: false` describes a set the backend does not hold — its
		// rejected list says nothing about who matches what.
		vi.mocked(invoke).mockResolvedValueOnce({ applied: false, rejected: [] } as unknown as undefined);
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				host.registerOutputWatcher({ pattern: /done/, onMatch });
			}),
		);
		await flushMicrotasks();

		pluginRegistry.handleWatcherLines("s1", [{ text: "done", matched_ids: [] }]);
		await flushMicrotasks();
		expect(onMatch).toHaveBeenCalledOnce();
	});

	it("keeps matching in the WebView when the backend call fails", async () => {
		const { invoke } = await import("../../invoke");
		const onMatch = vi.fn();
		vi.mocked(invoke).mockRejectedValueOnce(new Error("no such command"));
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				host.registerOutputWatcher({ pattern: /done/, onMatch });
			}),
		);
		await flushMicrotasks();

		pluginRegistry.handleWatcherLines("s1", [{ text: "done", matched_ids: [] }]);
		await flushMicrotasks();
		expect(onMatch).toHaveBeenCalledOnce();
	});

	it("is a no-op when no watcher is registered", () => {
		expect(() => pluginRegistry.handleWatcherLines("s1", [{ text: "anything", matched_ids: [] }])).not.toThrow();
	});

	// Rust is the only source of lines, so a client the backend does not know
	// about is blind with no local symptom — a failed sync, or an eviction when
	// stale browser reloads fill the client bound. Neither raises an event, so
	// recovery is periodic.
	it("resyncs on a heartbeat while watchers exist, and stops when the last one goes", async () => {
		vi.useFakeTimers();
		try {
			const { invoke } = await import("../../invoke");
			vi.mocked(invoke).mockClear();
			const syncCount = () => vi.mocked(invoke).mock.calls.filter((c) => c[0] === "set_plugin_output_watchers").length;

			pluginRegistry.register(
				makePlugin("p1", (host) => {
					host.registerOutputWatcher({ pattern: /done/, onMatch: vi.fn() });
				}),
			);
			expect(syncCount()).toBe(1);
			const firstSeq = (await lastSync()).seq;

			vi.advanceTimersByTime(30_000);
			expect(syncCount()).toBe(2);
			// A fresh sequence each time, so the backend can order the heartbeat
			// against a concurrent mutation instead of refusing it as stale.
			expect((await lastSync()).seq).toBe(firstSeq + 1);

			vi.advanceTimersByTime(30_000);
			expect(syncCount()).toBe(3);

			// Disposal syncs the now-empty set, then the timer must stop: an idle
			// frontend has nothing to keep alive.
			pluginRegistry.unregister("p1");
			const afterDisposal = syncCount();
			vi.advanceTimersByTime(5 * 60_000);
			expect(syncCount()).toBe(afterDisposal);
		} finally {
			vi.useRealTimers();
		}
	});
});

// ---------------------------------------------------------------------------
// Structured event dispatch
// ---------------------------------------------------------------------------

describe("dispatchStructuredEvent", () => {
	it("calls handler for matching type", async () => {
		const handler = vi.fn();
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				host.registerStructuredEventHandler("plan-file", handler);
			}),
		);
		pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/foo.md" }, "s1");
		await flushMicrotasks();
		expect(handler).toHaveBeenCalledWith({ path: "/foo.md" }, "s1");
	});

	it("does not call handler for non-matching type", async () => {
		const handler = vi.fn();
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				host.registerStructuredEventHandler("plan-file", handler);
			}),
		);
		pluginRegistry.dispatchStructuredEvent("rate-limit", {}, "s1");
		await flushMicrotasks();
		expect(handler).not.toHaveBeenCalled();
	});

	it("calls all handlers registered for the same type", async () => {
		const h1 = vi.fn();
		const h2 = vi.fn();
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				host.registerStructuredEventHandler("plan-file", h1);
			}),
		);
		pluginRegistry.register(
			makePlugin("p2", (host) => {
				host.registerStructuredEventHandler("plan-file", h2);
			}),
		);
		pluginRegistry.dispatchStructuredEvent("plan-file", {}, "s1");
		await flushMicrotasks();
		expect(h1).toHaveBeenCalledOnce();
		expect(h2).toHaveBeenCalledOnce();
	});

	it("unregistering plugin removes its structured event handlers", async () => {
		const handler = vi.fn();
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				host.registerStructuredEventHandler("plan-file", handler);
			}),
		);
		pluginRegistry.unregister("p1");
		pluginRegistry.dispatchStructuredEvent("plan-file", {}, "s1");
		await flushMicrotasks();
		expect(handler).not.toHaveBeenCalled();
	});

	it("catches and does not rethrow handler exceptions", async () => {
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				host.registerStructuredEventHandler("plan-file", () => {
					throw new Error("handler boom");
				});
			}),
		);
		pluginRegistry.dispatchStructuredEvent("plan-file", {}, "s1");
		await flushMicrotasks();
		// Exception is caught inside the microtask — no unhandled error
	});

	it("continues dispatching to other handlers after one throws", async () => {
		const h2 = vi.fn();
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				host.registerStructuredEventHandler("plan-file", () => {
					throw new Error("boom");
				});
			}),
		);
		pluginRegistry.register(
			makePlugin("p2", (host) => {
				host.registerStructuredEventHandler("plan-file", h2);
			}),
		);
		pluginRegistry.dispatchStructuredEvent("plan-file", { path: "/foo.md" }, "s1");
		await flushMicrotasks();
		expect(h2).toHaveBeenCalledOnce();
	});
});

// ---------------------------------------------------------------------------
// onload failure
// ---------------------------------------------------------------------------

describe("onload failure", () => {
	it("does not register a plugin whose onload throws", () => {
		pluginRegistry.register(
			makePlugin("bad", () => {
				throw new Error("onload boom");
			}),
		);
		// Plugin should not be registered — unregister should be a no-op
		const onunload = vi.fn();
		pluginRegistry.unregister("bad");
		expect(onunload).not.toHaveBeenCalled();
	});

	it("cleans up partial registrations when onload throws", () => {
		pluginRegistry.register(
			makePlugin("bad", (host) => {
				host.registerSection({ id: "partial-section", label: "X", priority: 10, canDismissAll: false });
				throw new Error("mid-onload boom");
			}),
		);
		// The section registered before the throw should be cleaned up
		expect(activityStore.getSections().some((s) => s.id === "partial-section")).toBe(false);
	});

	it("does not affect subsequent plugin registrations", () => {
		pluginRegistry.register(
			makePlugin("bad", () => {
				throw new Error("boom");
			}),
		);
		const onload = vi.fn();
		pluginRegistry.register(makePlugin("good", onload));
		expect(onload).toHaveBeenCalledOnce();
	});
});

// ---------------------------------------------------------------------------
// removeSession
// ---------------------------------------------------------------------------

describe("removeSession", () => {
	it("tells plugins the session is gone", async () => {
		const handler = vi.fn();
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				host.registerStructuredEventHandler("session-closed", handler);
			}),
		);
		pluginRegistry.removeSession("s1");
		await flushMicrotasks();
		expect(handler).toHaveBeenCalledWith({}, "s1");
	});

	it("is a no-op for unknown session", () => {
		expect(() => pluginRegistry.removeSession("nonexistent")).not.toThrow();
	});
});

// ---------------------------------------------------------------------------
// Tier 2: Read-only app state
// ---------------------------------------------------------------------------

describe("PluginHost — Tier 2 read-only state", () => {
	it("getActiveRepo returns null when no repo is active", () => {
		let result: ReturnType<PluginHost["getActiveRepo"]> = undefined as never;
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				result = host.getActiveRepo();
			}),
		);
		expect(result).toBeNull();
	});

	it("getRepos returns empty array when no repos registered", () => {
		let result: ReturnType<PluginHost["getRepos"]> = undefined as never;
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				result = host.getRepos();
			}),
		);
		expect(result).toEqual([]);
	});

	it("getActiveTerminalSessionId returns null when no terminal active", () => {
		let result: ReturnType<PluginHost["getActiveTerminalSessionId"]> = undefined as never;
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				result = host.getActiveTerminalSessionId();
			}),
		);
		expect(result).toBeNull();
	});

	it("getPrNotifications returns empty array when no notifications", () => {
		let result: ReturnType<PluginHost["getPrNotifications"]> = undefined as never;
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				result = host.getPrNotifications();
			}),
		);
		expect(result).toEqual([]);
	});

	it("getSettings returns null for unknown repo", () => {
		let result: ReturnType<PluginHost["getSettings"]> = undefined as never;
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				result = host.getSettings("/nonexistent/repo");
			}),
		);
		expect(result).toBeNull();
	});

	it("getTerminalState returns null when no terminal active", () => {
		let result: ReturnType<PluginHost["getTerminalState"]> = undefined as never;
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				result = host.getTerminalState();
			}),
		);
		expect(result).toBeNull();
	});

	it("a disposed onStateChange listener stops receiving events", () => {
		// Catches a dispose() that does not unsubscribe: the old test only checked the shape.
		const listener = vi.fn();
		let disposable: { dispose: () => void } | null = null;
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				disposable = host.onStateChange(listener);
			}),
		);
		const event = { type: "agent-started", sessionId: "s", terminalId: "t" } as const;
		pluginRegistry.notifyStateChange(event);
		expect(listener).toHaveBeenCalledTimes(1);
		disposable!.dispose();
		pluginRegistry.notifyStateChange(event);
		expect(listener).toHaveBeenCalledTimes(1);
	});

	it("breaks synchronous reentrancy in notifyStateChange — caller returns without freezing", async () => {
		// A listener that re-triggers notifyStateChange from inside its own callback
		// must not cause unbounded sync recursion (the bug class that freezes the UI).
		let depth = 0;
		let maxSyncDepth = 0;
		let totalCalls = 0;

		pluginRegistry.register(
			makePlugin("p1", (host) => {
				host.onStateChange((event) => {
					totalCalls++;
					depth++;
					maxSyncDepth = Math.max(maxSyncDepth, depth);
					if (totalCalls < 5 && event.type === "agent-started") {
						// Re-enter — without the guard this would blow the stack synchronously.
						pluginRegistry.notifyStateChange({
							type: "agent-started",
							sessionId: "s",
							terminalId: "t",
						});
					}
					depth--;
				});
			}),
		);

		pluginRegistry.notifyStateChange({ type: "agent-started", sessionId: "s", terminalId: "t" });

		// First call runs synchronously; nested calls are deferred — so max sync
		// depth must be 1 regardless of how many re-entries the listener requested.
		expect(maxSyncDepth).toBe(1);
		expect(totalCalls).toBe(1);

		// Drain the deferred microtask queue until the listener stops re-entering.
		for (let i = 0; i < 10 && totalCalls < 5; i++) await flushMicrotasks();
		expect(totalCalls).toBe(5);
	});
});

// ---------------------------------------------------------------------------
// Tier 2b: git:read capability gating
// ---------------------------------------------------------------------------

describe("PluginHost — git:read capability gating", () => {
	it("external plugin without git:read throws on getGitBranches", async () => {
		let host: PluginHost | null = null;
		await pluginRegistry.register(
			makePlugin("ext", (h) => {
				host = h;
			}),
			[], // no capabilities
		);
		await expect(host!.getGitBranches("/some/repo")).rejects.toThrow(PluginCapabilityError);
	});

	it("external plugin with git:read can call getGitBranches", async () => {
		let host: PluginHost | null = null;
		await pluginRegistry.register(
			makePlugin("ext", (h) => {
				host = h;
			}),
			["git:read"],
		);
		// invoke is mocked to resolve — should not throw
		await expect(host!.getGitBranches("/some/repo")).resolves.toBeDefined();
	});

	it("external plugin without git:read throws on getRecentCommits", async () => {
		let host: PluginHost | null = null;
		await pluginRegistry.register(
			makePlugin("ext", (h) => {
				host = h;
			}),
			[],
		);
		await expect(host!.getRecentCommits("/some/repo")).rejects.toThrow(PluginCapabilityError);
	});

	it("external plugin with git:read can call getRecentCommits", async () => {
		let host: PluginHost | null = null;
		await pluginRegistry.register(
			makePlugin("ext", (h) => {
				host = h;
			}),
			["git:read"],
		);
		await expect(host!.getRecentCommits("/some/repo", 5)).resolves.toBeDefined();
	});

	it("external plugin without git:read throws on getGitDiff", async () => {
		let host: PluginHost | null = null;
		await pluginRegistry.register(
			makePlugin("ext", (h) => {
				host = h;
			}),
			[],
		);
		await expect(host!.getGitDiff("/some/repo")).rejects.toThrow(PluginCapabilityError);
	});

	it("external plugin with git:read can call getGitDiff", async () => {
		let host: PluginHost | null = null;
		await pluginRegistry.register(
			makePlugin("ext", (h) => {
				host = h;
			}),
			["git:read"],
		);
		await expect(host!.getGitDiff("/some/repo", "staged")).resolves.toBeDefined();
	});

	it("built-in plugin can call git:read methods without capability", async () => {
		let host: PluginHost | null = null;
		pluginRegistry.register(
			makePlugin("builtin", (h) => {
				host = h;
			}),
		);
		// Built-in plugins have null capabilities (unrestricted)
		await expect(host!.getGitBranches("/some/repo")).resolves.toBeDefined();
		await expect(host!.getRecentCommits("/some/repo")).resolves.toBeDefined();
		await expect(host!.getGitDiff("/some/repo")).resolves.toBeDefined();
	});
});

// ---------------------------------------------------------------------------
// Tier 3: Capability-gated write actions
// ---------------------------------------------------------------------------

describe("PluginHost — Tier 3 capability gating", () => {
	it("built-in plugins (no capabilities) can call writePty without error", async () => {
		let host: PluginHost | null = null;
		pluginRegistry.register(
			makePlugin("builtin", (h) => {
				host = h;
			}),
		);
		// invoke is mocked to resolve — should not throw PluginCapabilityError
		await expect(host!.writePty("s1", "data")).resolves.toBeUndefined();
	});

	it("external plugin without pty:write throws PluginCapabilityError on writePty", async () => {
		let host: PluginHost | null = null;
		await pluginRegistry.register(
			makePlugin("ext", (h) => {
				host = h;
			}),
			[], // no capabilities
		);
		await expect(host!.writePty("s1", "data")).rejects.toThrow(PluginCapabilityError);
	});

	it("external plugin with pty:write can call writePty", async () => {
		let host: PluginHost | null = null;
		await pluginRegistry.register(
			makePlugin("ext", (h) => {
				host = h;
			}),
			["pty:write"],
		);
		await expect(host!.writePty("s1", "data")).resolves.toBeUndefined();
	});

	it("built-in plugins can call sendAgentInput without error", async () => {
		let host: PluginHost | null = null;
		pluginRegistry.register(
			makePlugin("builtin", (h) => {
				host = h;
			}),
		);
		await expect(host!.sendAgentInput("s1", "hello")).resolves.toBeUndefined();
	});

	it("sendAgentInput is a no-op when session has no active agent", async () => {
		const { invoke: mockInvoke } = await import("../../invoke");
		(mockInvoke as ReturnType<typeof vi.fn>).mockClear();
		let host: PluginHost | null = null;
		pluginRegistry.register(
			makePlugin("builtin", (h) => {
				host = h;
			}),
		);
		await host!.sendAgentInput("no-agent-session", "wake up");
		expect(mockInvoke).not.toHaveBeenCalledWith("write_pty", expect.anything());
	});

	it("external plugin without pty:write throws PluginCapabilityError on sendAgentInput", async () => {
		let host: PluginHost | null = null;
		await pluginRegistry.register(
			makePlugin("ext", (h) => {
				host = h;
			}),
			[], // no capabilities
		);
		await expect(host!.sendAgentInput("s1", "hello")).rejects.toThrow(PluginCapabilityError);
	});

	it("external plugin with pty:write can call sendAgentInput", async () => {
		let host: PluginHost | null = null;
		await pluginRegistry.register(
			makePlugin("ext", (h) => {
				host = h;
			}),
			["pty:write"],
		);
		await expect(host!.sendAgentInput("s1", "hello")).resolves.toBeUndefined();
	});

	it("external plugin without ui:markdown throws on openMarkdownPanel", async () => {
		let host: PluginHost | null = null;
		await pluginRegistry.register(
			makePlugin("ext", (h) => {
				host = h;
			}),
			[],
		);
		expect(() => host!.openMarkdownPanel("Title", "plan:file")).toThrow(PluginCapabilityError);
	});

	it("external plugin with ui:markdown can call openMarkdownPanel", async () => {
		let host: PluginHost | null = null;
		await pluginRegistry.register(
			makePlugin("ext", (h) => {
				host = h;
			}),
			["ui:markdown"],
		);
		expect(() => host!.openMarkdownPanel("Title", "plan:file")).not.toThrow();
	});

	it("external plugin without ui:sound throws on playNotificationSound", async () => {
		let host: PluginHost | null = null;
		await pluginRegistry.register(
			makePlugin("ext", (h) => {
				host = h;
			}),
			[],
		);
		await expect(host!.playNotificationSound()).rejects.toThrow(PluginCapabilityError);
	});

	it("external plugin with ui:sound can call playNotificationSound", async () => {
		let host: PluginHost | null = null;
		await pluginRegistry.register(
			makePlugin("ext", (h) => {
				host = h;
			}),
			["ui:sound"],
		);
		await expect(host!.playNotificationSound()).resolves.toBeUndefined();
	});
});

// ---------------------------------------------------------------------------
// Tier 3c: Panel UI capability gating
// ---------------------------------------------------------------------------

describe("PluginHost — Tier 3c openPanel capability gating", () => {
	it("external plugin without ui:panel throws on openPanel", async () => {
		let host: PluginHost | null = null;
		await pluginRegistry.register(
			makePlugin("ext", (h) => {
				host = h;
			}),
			[], // no capabilities
		);
		expect(() => host!.openPanel({ id: "test", title: "Test", html: "<h1>hi</h1>" })).toThrow(PluginCapabilityError);
	});

	it("external plugin with ui:panel can call openPanel", async () => {
		let host: PluginHost | null = null;
		await pluginRegistry.register(
			makePlugin("ext", (h) => {
				host = h;
			}),
			["ui:panel"],
		);
		const handle = host!.openPanel({ id: "test", title: "Test Panel", html: "<h1>hello</h1>" });
		expect(handle).toBeDefined();
		expect(handle.tabId).toBeTruthy();
		expect(typeof handle.update).toBe("function");
		expect(typeof handle.close).toBe("function");
	});

	it("built-in plugin can open panel without declaring capability", () => {
		let host: PluginHost | null = null;
		pluginRegistry.register(
			makePlugin("builtin", (h) => {
				host = h;
			}),
		);
		const handle = host!.openPanel({ id: "test", title: "Test", html: "<h1>hi</h1>" });
		expect(handle.tabId).toBeTruthy();
	});

	it("openPanel returns handle that can close the tab", () => {
		let host: PluginHost | null = null;
		pluginRegistry.register(
			makePlugin("builtin", (h) => {
				host = h;
			}),
		);
		const handle = host!.openPanel({ id: "test", title: "Test", html: "<h1>hi</h1>" });
		const tabBefore = mdTabsStore.get(handle.tabId);
		expect(tabBefore).toBeDefined();
		handle.close();
		const tabAfter = mdTabsStore.get(handle.tabId);
		expect(tabAfter).toBeUndefined();
	});

	it("opening same panel twice returns existing tab", () => {
		let host: PluginHost | null = null;
		pluginRegistry.register(
			makePlugin("builtin", (h) => {
				host = h;
			}),
		);
		const handle1 = host!.openPanel({ id: "test", title: "Test", html: "<h1>v1</h1>" });
		const handle2 = host!.openPanel({ id: "test", title: "Test", html: "<h1>v2</h1>" });
		expect(handle1.tabId).toBe(handle2.tabId);
	});

	describe("OpenPanelOptions.id (613-00e8 F106)", () => {
		function hostFor(pluginId: string): PluginHost {
			let host: PluginHost | null = null;
			pluginRegistry.register(
				makePlugin(pluginId, (h) => {
					host = h;
				}),
			);
			return host!;
		}

		it("reuses the panel with the same id even when the title changed", () => {
			const host = hostFor("builtin");
			const first = host.openPanel({ id: "dash", title: "mdkb", html: "<h1>v1</h1>" });
			// A dashboard that puts the repo name in its title is still the same panel.
			const second = host.openPanel({ id: "dash", title: "mdkb — repo-b", html: "<h1>v2</h1>" });

			expect(second.tabId).toBe(first.tabId);
			expect(mdTabsStore.getCount()).toBe(1);
			const tab = mdTabsStore.get(first.tabId) as { title: string; html: string } | undefined;
			expect(tab?.title).toBe("mdkb — repo-b");
			expect(tab?.html).toBe("<h1>v2</h1>");
		});

		it("activates the reused panel so the user sees what was asked for", () => {
			const host = hostFor("builtin");
			const first = host.openPanel({ id: "dash", title: "Dash", html: "<h1>v1</h1>" });
			mdTabsStore.setActive(null);

			host.openPanel({ id: "dash", title: "Dash", html: "<h1>v2</h1>" });

			expect(mdTabsStore.state.activeId).toBe(first.tabId);
		});

		it("keeps distinct ids in distinct tabs even when the titles collide", () => {
			const host = hostFor("builtin");
			const a = host.openPanel({ id: "left", title: "Report", html: "<h1>a</h1>" });
			const b = host.openPanel({ id: "right", title: "Report", html: "<h1>b</h1>" });

			expect(b.tabId).not.toBe(a.tabId);
			expect(mdTabsStore.getCount()).toBe(2);
		});

		it("scopes the id to the plugin — two plugins may both use 'dash'", () => {
			const a = hostFor("plugin-a").openPanel({ id: "dash", title: "A", html: "<h1>a</h1>" });
			const b = hostFor("plugin-b").openPanel({ id: "dash", title: "B", html: "<h1>b</h1>" });

			expect(b.tabId).not.toBe(a.tabId);
			expect(mdTabsStore.getCount()).toBe(2);
		});
	});

	describe("panel visibility (613-00e8 F102)", () => {
		function openWith(onVisibilityChange?: (visible: boolean) => void) {
			let handle: ReturnType<PluginHost["openPanel"]> | null = null;
			pluginRegistry.register(
				makePlugin("builtin", (host) => {
					handle = host.openPanel({ id: "dash", title: "Dash", html: "<h1>hi</h1>", onVisibilityChange });
				}),
			);
			return handle!;
		}

		it("a freshly opened panel is visible — openPanel activates its tab", () => {
			expect(openWith().isVisible()).toBe(true);
		});

		it("follows what the panel component reports", () => {
			const handle = openWith();
			pluginRegistry.setPanelVisible(handle.tabId, false);
			expect(handle.isVisible()).toBe(false);
			pluginRegistry.setPanelVisible(handle.tabId, true);
			expect(handle.isVisible()).toBe(true);
		});

		it("notifies the plugin on a change, and only on a change", () => {
			const onVisibilityChange = vi.fn();
			const handle = openWith(onVisibilityChange);

			pluginRegistry.setPanelVisible(handle.tabId, false);
			pluginRegistry.setPanelVisible(handle.tabId, false);

			expect(onVisibilityChange).toHaveBeenCalledTimes(1);
			expect(onVisibilityChange).toHaveBeenCalledWith(false);
		});

		it("a closed panel is not visible and stops notifying", () => {
			const onVisibilityChange = vi.fn();
			const handle = openWith(onVisibilityChange);
			mdTabsStore.remove(handle.tabId);
			onVisibilityChange.mockClear();

			pluginRegistry.setPanelVisible(handle.tabId, false);

			expect(handle.isVisible()).toBe(false);
			expect(onVisibilityChange).not.toHaveBeenCalled();
		});
	});

	describe("panel close notification (613-00e8 F101)", () => {
		function openWith(onClose?: () => void) {
			let handle: ReturnType<PluginHost["openPanel"]> | null = null;
			pluginRegistry.register(
				makePlugin("builtin", (host) => {
					handle = host.openPanel({ id: "dash", title: "Dash", html: "<h1>hi</h1>", onClose });
				}),
			);
			return handle!;
		}

		it("tells the plugin its panel is gone, whoever closed it", () => {
			const onClose = vi.fn();
			const handle = openWith(onClose);

			// The tab bar, not the plugin: without this the plugin holds a dead
			// handle and never releases what the panel owned.
			mdTabsStore.remove(handle.tabId);

			expect(onClose).toHaveBeenCalledTimes(1);
		});

		it("fires once, even when the plugin closes the panel itself", () => {
			const onClose = vi.fn();
			const handle = openWith(onClose);

			handle.close();
			mdTabsStore.remove(handle.tabId);

			expect(onClose).toHaveBeenCalledTimes(1);
		});

		it("is not confused with a panel simply being hidden", () => {
			const onClose = vi.fn();
			const handle = openWith(onClose);

			pluginRegistry.setPanelVisible(handle.tabId, false);

			expect(onClose).not.toHaveBeenCalled();
		});
	});

	describe("update() reports a dead tab (613-00e8 F106)", () => {
		it("returns true while the panel is open and false once it is gone", () => {
			let host: PluginHost | null = null;
			pluginRegistry.register(
				makePlugin("builtin", (h) => {
					host = h;
				}),
			);
			const handle = host!.openPanel({ id: "dash", title: "Dash", html: "<h1>v1</h1>" });

			expect(handle.update("<h1>v2</h1>")).toBe(true);

			// Closed from the tab bar, not through the handle — the plugin has no
			// way to know unless update() tells it.
			mdTabsStore.remove(handle.tabId);

			expect(handle.update("<h1>v3</h1>")).toBe(false);
		});
	});
});

// ---------------------------------------------------------------------------
// Tier 3d: Credential read capability gating
// ---------------------------------------------------------------------------

describe("PluginHost — Tier 3c readCredential capability gating", () => {
	it("external plugin without credentials:read throws on readCredential", async () => {
		let host: PluginHost | null = null;
		await pluginRegistry.register(
			makePlugin("ext", (h) => {
				host = h;
			}),
			[], // no capabilities
		);
		await expect(host!.readCredential("Claude Code-credentials")).rejects.toThrow(PluginCapabilityError);
	});

	it("built-in plugin can call readCredential without capability", async () => {
		let host: PluginHost | null = null;
		pluginRegistry.register(
			makePlugin("builtin", (h) => {
				host = h;
			}),
		);
		// Mocked invoke returns undefined → null, no consent dialog for built-in
		await expect(host!.readCredential("Claude Code-credentials")).resolves.not.toThrow();
	});
});

// ---------------------------------------------------------------------------
// Tier 3g: CLI execution capability gating
// ---------------------------------------------------------------------------

describe("PluginHost — execCli capability gating", () => {
	it("external plugin without exec:cli throws on execCli", async () => {
		let host: PluginHost | null = null;
		await pluginRegistry.register(
			makePlugin("ext", (h) => {
				host = h;
			}),
			[], // no capabilities
		);
		await expect(host!.execCli("mdkb", ["status"])).rejects.toThrow(PluginCapabilityError);
	});

	it("external plugin with exec:cli can call execCli", async () => {
		let host: PluginHost | null = null;
		await pluginRegistry.register(
			makePlugin("ext", (h) => {
				host = h;
			}),
			["exec:cli"],
		);
		// invoke is mocked → resolves, should not throw capability error
		await expect(host!.execCli("mdkb", ["--format", "json", "status"], "/Users/me/project")).resolves.not.toThrow();
	});

	it("built-in plugin can call execCli without declaring capability", async () => {
		let host: PluginHost | null = null;
		pluginRegistry.register(
			makePlugin("builtin", (h) => {
				host = h;
			}),
		);
		await expect(host!.execCli("mdkb", ["status"])).resolves.not.toThrow();
	});

	it("passes correct args to the Rust command", async () => {
		let host: PluginHost | null = null;
		await pluginRegistry.register(
			makePlugin("ext", (h) => {
				host = h;
			}),
			["exec:cli"],
		);
		await host!.execCli("mdkb", ["--format", "json", "status"], "/Users/me/repo");
		const { invoke } = await import("../../invoke");
		expect(invoke).toHaveBeenCalledWith("plugin_exec_cli", {
			binary: "mdkb",
			args: ["--format", "json", "status"],
			cwd: "/Users/me/repo",
			pluginId: "ext",
		});
	});

	it("passes null cwd when not provided", async () => {
		let host: PluginHost | null = null;
		await pluginRegistry.register(
			makePlugin("ext", (h) => {
				host = h;
			}),
			["exec:cli"],
		);
		await host!.execCli("mdkb", ["status"]);
		const { invoke } = await import("../../invoke");
		expect(invoke).toHaveBeenCalledWith(
			"plugin_exec_cli",
			expect.objectContaining({
				cwd: null,
			}),
		);
	});
});

// ---------------------------------------------------------------------------
// Tier 3d: HTTP fetch capability gating
// ---------------------------------------------------------------------------

describe("PluginHost — Tier 3c httpFetch capability gating", () => {
	it("external plugin without net:http throws on httpFetch", async () => {
		let host: PluginHost | null = null;
		await pluginRegistry.register(
			makePlugin("ext", (h) => {
				host = h;
			}),
			[], // no capabilities
		);
		await expect(host!.httpFetch("https://example.com")).rejects.toThrow(PluginCapabilityError);
	});

	it("external plugin with net:http can call httpFetch", async () => {
		let host: PluginHost | null = null;
		await pluginRegistry.register(
			makePlugin("ext", (h) => {
				host = h;
			}),
			["net:http"],
			["https://example.com/*"],
		);
		// invoke is mocked → resolves, should not throw capability error
		await expect(host!.httpFetch("https://example.com/api")).resolves.not.toThrow();
	});

	it("built-in plugin can call httpFetch without declaring capability", async () => {
		let host: PluginHost | null = null;
		pluginRegistry.register(
			makePlugin("builtin", (h) => {
				host = h;
			}),
		);
		await expect(host!.httpFetch("https://example.com")).resolves.not.toThrow();
	});

	it("does NOT pass allowedUrls to the Rust command (manifest is source of truth)", async () => {
		let host: PluginHost | null = null;
		await pluginRegistry.register(
			makePlugin("ext", (h) => {
				host = h;
			}),
			["net:http"],
		);
		await host!.httpFetch("https://api.anthropic.com/usage", {
			method: "GET",
			headers: { Authorization: "Bearer token" },
		});
		// The allowlist is re-read from the on-disk manifest by the backend, so the
		// caller must NOT be able to supply it — otherwise a scoped plugin could
		// widen its own allowlist and bypass the SSRF guard.
		const { invoke } = await import("../../invoke");
		expect(invoke).toHaveBeenCalledWith(
			"plugin_http_fetch",
			expect.objectContaining({
				url: "https://api.anthropic.com/usage",
				method: "GET",
				headers: { Authorization: "Bearer token" },
				pluginId: "ext",
			}),
		);
		const httpCall = (invoke as unknown as { mock: { calls: unknown[][] } }).mock.calls.find(
			(c) => c[0] === "plugin_http_fetch",
		);
		expect(httpCall?.[1]).not.toHaveProperty("allowedUrls");
	});
});

// ---------------------------------------------------------------------------
// Tier 4: Scoped invoke
// ---------------------------------------------------------------------------

describe("PluginHost — Tier 4 scoped invoke", () => {
	it("rejects non-whitelisted commands", async () => {
		let host: PluginHost | null = null;
		pluginRegistry.register(
			makePlugin("p1", (h) => {
				host = h;
			}),
		);
		await expect(host!.invoke("dangerous_command")).rejects.toThrow("not in the invoke whitelist");
	});

	it("allows whitelisted plugin data commands without capability", async () => {
		let host: PluginHost | null = null;
		await pluginRegistry.register(
			makePlugin("ext", (h) => {
				host = h;
			}),
			[], // no capabilities
		);
		// read_plugin_data, write_plugin_data, delete_plugin_data are always allowed
		await expect(host!.invoke("read_plugin_data", { plugin_id: "ext", path: "cache.json" })).resolves.toBeUndefined();
	});

	it("external plugin needs invoke:read_file capability for read_file", async () => {
		let host: PluginHost | null = null;
		await pluginRegistry.register(
			makePlugin("ext", (h) => {
				host = h;
			}),
			[], // no invoke:read_file capability
		);
		await expect(host!.invoke("read_file", { path: "/repo", file: "README.md" })).rejects.toThrow(
			PluginCapabilityError,
		);
	});

	it("external plugin with invoke:read_file can call read_file", async () => {
		let host: PluginHost | null = null;
		await pluginRegistry.register(
			makePlugin("ext", (h) => {
				host = h;
			}),
			["invoke:read_file"],
		);
		await expect(host!.invoke("read_file", { path: "/repo", file: "README.md" })).resolves.toBeUndefined();
	});

	it("get_input_buffer_content requires pty:read (not invoke:*)", async () => {
		let hostNoCap: PluginHost | null = null;
		await pluginRegistry.register(
			makePlugin("no-pty", (h) => {
				hostNoCap = h;
			}),
			[],
		);
		await expect(hostNoCap!.invoke("get_input_buffer_content", { sessionId: "s1" })).rejects.toThrow(
			PluginCapabilityError,
		);

		let hostWithCap: PluginHost | null = null;
		await pluginRegistry.register(
			makePlugin("with-pty", (h) => {
				hostWithCap = h;
			}),
			["pty:read"],
		);
		await expect(hostWithCap!.invoke("get_input_buffer_content", { sessionId: "s1" })).resolves.toBeUndefined();
	});
});

// ---------------------------------------------------------------------------
// Tier 3b: fs:write and fs:rename capability gating
// ---------------------------------------------------------------------------

describe("PluginHost — fs:write capability gating", () => {
	it("external plugin without fs:write throws on writeFile", async () => {
		let host: PluginHost | null = null;
		await pluginRegistry.register(
			makePlugin("ext", (h) => {
				host = h;
			}),
			[],
		);
		await expect(host!.writeFile("/home/user/test.txt", "content")).rejects.toThrow(PluginCapabilityError);
		await expect(host!.writeFileBase64("/home/user/test.db", "U1FMaXRl")).rejects.toThrow(PluginCapabilityError);
	});

	it("external plugin with fs:write can call writeFile", async () => {
		let host: PluginHost | null = null;
		await pluginRegistry.register(
			makePlugin("ext", (h) => {
				host = h;
			}),
			["fs:write"],
		);
		await expect(host!.writeFile("/home/user/test.txt", "content")).resolves.toBeUndefined();
	});

	it("passes correct args to the Rust command", async () => {
		let host: PluginHost | null = null;
		await pluginRegistry.register(
			makePlugin("ext", (h) => {
				host = h;
			}),
			["fs:write"],
		);
		await host!.writeFile("/home/user/test.txt", "hello");
		await host!.writeFileBase64("/home/user/test.db", "U1FMaXRl", { maxBytes: 2048 });
		const { invoke } = await import("../../invoke");
		expect(invoke).toHaveBeenCalledWith("plugin_write_file", {
			path: "/home/user/test.txt",
			content: "hello",
			pluginId: "ext",
		});
		expect(invoke).toHaveBeenCalledWith("plugin_write_file_base64", {
			path: "/home/user/test.db",
			content: "U1FMaXRl",
			maxBytes: 2048,
			pluginId: "ext",
		});
	});

	it("built-in plugin can call writeFile without declaring capability", async () => {
		let host: PluginHost | null = null;
		pluginRegistry.register(
			makePlugin("builtin", (h) => {
				host = h;
			}),
		);
		await expect(host!.writeFile("/home/user/test.txt", "content")).resolves.toBeUndefined();
	});
});

describe("PluginHost — readFiles batch read (613-00e8 F101)", () => {
	async function hostWith(capabilities: string[]): Promise<PluginHost> {
		let host: PluginHost | null = null;
		await pluginRegistry.register(
			makePlugin("ext", (h) => {
				host = h;
			}),
			capabilities,
		);
		return host!;
	}

	it("external plugin without fs:read throws on readFiles", async () => {
		const host = await hostWith([]);
		await expect(host.readFiles(["/home/user/a.md"])).rejects.toThrow(PluginCapabilityError);
	});

	it("reads a whole directory in a single command", async () => {
		const host = await hostWith(["fs:read"]);
		const { invoke } = await import("../../invoke");
		vi.mocked(invoke).mockClear();

		const paths = Array.from({ length: 40 }, (_, i) => `/home/user/${i}.md`);
		await host.readFiles(paths);

		expect(invoke).toHaveBeenCalledTimes(1);
		expect(invoke).toHaveBeenCalledWith("plugin_read_files", { paths, pluginId: "ext" });
	});

	it("hands back the backend's result, nulls and all", async () => {
		const host = await hostWith(["fs:read"]);
		const { invoke } = await import("../../invoke");
		// The null marks a file that could not be read; collapsing or dropping it
		// would silently misalign the result with the paths that were asked for.
		vi.mocked(invoke).mockResolvedValueOnce(["first", null, "third"]);

		await expect(host.readFiles(["/home/user/a", "/home/user/b", "/home/user/c"])).resolves.toEqual([
			"first",
			null,
			"third",
		]);
	});

	it("costs no round trip at all for an empty list", async () => {
		const host = await hostWith(["fs:read"]);
		const { invoke } = await import("../../invoke");
		vi.mocked(invoke).mockClear();

		await expect(host.readFiles([])).resolves.toEqual([]);
		expect(invoke).not.toHaveBeenCalled();
	});
});

describe("PluginHost — fs:rename capability gating", () => {
	it("external plugin without fs:rename throws on renamePath", async () => {
		let host: PluginHost | null = null;
		await pluginRegistry.register(
			makePlugin("ext", (h) => {
				host = h;
			}),
			[],
		);
		await expect(host!.renamePath("/home/user/a.txt", "/home/user/b.txt")).rejects.toThrow(PluginCapabilityError);
	});

	it("external plugin with fs:rename can call renamePath", async () => {
		let host: PluginHost | null = null;
		await pluginRegistry.register(
			makePlugin("ext", (h) => {
				host = h;
			}),
			["fs:rename"],
		);
		await expect(host!.renamePath("/home/user/a.txt", "/home/user/b.txt")).resolves.toBeUndefined();
	});

	it("passes correct args to the Rust command", async () => {
		let host: PluginHost | null = null;
		await pluginRegistry.register(
			makePlugin("ext", (h) => {
				host = h;
			}),
			["fs:rename"],
		);
		await host!.renamePath("/home/user/a.txt", "/home/user/b.txt");
		const { invoke } = await import("../../invoke");
		expect(invoke).toHaveBeenCalledWith("plugin_rename_path", {
			from: "/home/user/a.txt",
			to: "/home/user/b.txt",
			pluginId: "ext",
		});
	});

	it("built-in plugin can call renamePath without declaring capability", async () => {
		let host: PluginHost | null = null;
		pluginRegistry.register(
			makePlugin("builtin", (h) => {
				host = h;
			}),
		);
		await expect(host!.renamePath("/home/user/a.txt", "/home/user/b.txt")).resolves.toBeUndefined();
	});
});

// ---------------------------------------------------------------------------
// Panel message bridge
// ---------------------------------------------------------------------------

describe("PluginHost — watchPath fan-out (613-00e8 F104)", () => {
	const invoked = vi.mocked(invoke);

	it("delivers a watch's events to that watch's callback only", async () => {
		invoked.mockResolvedValueOnce("watch-a").mockResolvedValueOnce("watch-b");
		const onA = vi.fn();
		const onB = vi.fn();
		let host: PluginHost | null = null;
		await pluginRegistry.register(
			makePlugin("p1", (h) => {
				host = h;
			}),
		);
		await host!.watchPath("/a", onA);
		await host!.watchPath("/b", onB);

		emitTauri("plugin-fs-change-watch-a", [{ type: "modify", path: "/a/x" }]);

		// Both watches belong to the same plugin. Keying the event on the plugin
		// would wake every one of its callbacks for a change none of them asked
		// about — K watches, K wake-ups, K-1 of them wrong.
		expect(onA).toHaveBeenCalledOnce();
		expect(onA).toHaveBeenCalledWith([{ type: "modify", path: "/a/x" }]);
		expect(onB).not.toHaveBeenCalled();
	});
});

describe("PluginHost — panel message bridge", () => {
	it("unloading a plugin closes its panels and bridges", () => {
		const onMessage = vi.fn();
		const onClose = vi.fn();
		const sender = vi.fn();
		const handles: ReturnType<PluginHost["openPanel"]>[] = [];
		pluginRegistry.register(
			makePlugin("owner", (host) => {
				for (const id of ["one", "two"])
					handles.push(host.openPanel({ id, title: id, html: "hello", onMessage, onClose }));
			}),
		);
		let other: ReturnType<PluginHost["openPanel"]>;
		pluginRegistry.register(
			makePlugin("other", (host) => {
				other = host.openPanel({ id: "one", title: "Other", html: "hello" });
			}),
		);
		for (const handle of handles) pluginRegistry.registerPanelSendChannel(handle.tabId, sender);

		pluginRegistry.unregister("owner");

		expect(Object.values(mdTabsStore.state.tabs).map((tab) => tab.id)).toEqual([other!.tabId]);
		for (const handle of handles) {
			pluginRegistry.handlePanelMessage(handle.tabId, "late");
			handle.send("late");
			expect(handle.isVisible()).toBe(false);
			handle.close();
		}
		expect(onMessage).not.toHaveBeenCalled();
		expect(sender).not.toHaveBeenCalled();
		expect(onClose).toHaveBeenCalledTimes(2);
	});

	it("unload tolerates a panel already closed and closes a reopened panel once", () => {
		let host!: PluginHost;
		const onClose = vi.fn();
		pluginRegistry.register(
			makePlugin("owner", (api) => {
				host = api;
			}),
		);
		const closed = host.openPanel({ id: "closed", title: "Closed", html: "hello", onClose });
		closed.close();
		host.openPanel({ id: "open", title: "Open", html: "hello", onClose });
		host.openPanel({ id: "open", title: "Open", html: "updated", onClose });
		pluginRegistry.unregister("owner");
		expect(Object.values(mdTabsStore.state.tabs)).toHaveLength(0);
		expect(onClose).toHaveBeenCalledTimes(2);
	});

	it("onMessage callback receives messages via handlePanelMessage", () => {
		const onMessage = vi.fn();
		let handle: ReturnType<PluginHost["openPanel"]> | null = null;
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				handle = host.openPanel({ id: "test", title: "Test", html: "<h1>hi</h1>", onMessage });
			}),
		);
		pluginRegistry.handlePanelMessage(handle!.tabId, { type: "custom", value: 42 });
		expect(onMessage).toHaveBeenCalledOnce();
		expect(onMessage).toHaveBeenCalledWith({ type: "custom", value: 42 });
	});

	it("handlePanelMessage is a no-op when no onMessage registered", () => {
		let handle: ReturnType<PluginHost["openPanel"]> | null = null;
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				handle = host.openPanel({ id: "test", title: "Test", html: "<h1>hi</h1>" });
			}),
		);
		// Should not throw
		expect(() => pluginRegistry.handlePanelMessage(handle!.tabId, { type: "any" })).not.toThrow();
	});

	it("close() cleans up message handlers", () => {
		const onMessage = vi.fn();
		let handle: ReturnType<PluginHost["openPanel"]> | null = null;
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				handle = host.openPanel({ id: "test", title: "Test", html: "<h1>hi</h1>", onMessage });
			}),
		);
		handle!.close();
		pluginRegistry.handlePanelMessage(handle!.tabId, { type: "after-close" });
		expect(onMessage).not.toHaveBeenCalled();
	});

	it("closing the tab from the tab bar cleans up the message handler (613-00e8 F105)", () => {
		const onMessage = vi.fn();
		let handle: ReturnType<PluginHost["openPanel"]> | null = null;
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				handle = host.openPanel({ id: "test", title: "Test", html: "<h1>hi</h1>", onMessage });
			}),
		);

		// The × on the tab, the middle-click, the context menu and closeTerminal all
		// go straight to the store — none of them can reach handle.close().
		mdTabsStore.remove(handle!.tabId);
		pluginRegistry.handlePanelMessage(handle!.tabId, { type: "after-close" });

		expect(onMessage).not.toHaveBeenCalled();
	});

	it("send() calls registered send channel", () => {
		let handle: ReturnType<PluginHost["openPanel"]> | null = null;
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				handle = host.openPanel({ id: "test", title: "Test", html: "<h1>hi</h1>" });
			}),
		);
		const sender = vi.fn();
		pluginRegistry.registerPanelSendChannel(handle!.tabId, sender);
		handle!.send({ type: "response", ok: true });
		expect(sender).toHaveBeenCalledOnce();
		expect(sender).toHaveBeenCalledWith({ type: "response", ok: true }, undefined);
	});

	it("send() forwards transferable ownership to the panel channel", () => {
		let handle: ReturnType<PluginHost["openPanel"]> | null = null;
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				handle = host.openPanel({ id: "test", title: "Test", html: "<h1>hi</h1>" });
			}),
		);
		const sender = vi.fn();
		const buffer = new ArrayBuffer(8);
		pluginRegistry.registerPanelSendChannel(handle!.tabId, sender);

		handle!.send({ type: "database", buffer }, [buffer]);

		expect(sender).toHaveBeenCalledWith({ type: "database", buffer }, [buffer]);
	});

	it("send() is a no-op when no send channel registered", () => {
		let handle: ReturnType<PluginHost["openPanel"]> | null = null;
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				handle = host.openPanel({ id: "test", title: "Test", html: "<h1>hi</h1>" });
			}),
		);
		// No registerPanelSendChannel called — should not throw
		expect(() => handle!.send({ type: "msg" })).not.toThrow();
	});
});

// ---------------------------------------------------------------------------
// register() with capabilities parameter
// ---------------------------------------------------------------------------

describe("register with capabilities", () => {
	it("second arg passes capabilities to buildHost", async () => {
		// Verify the external plugin pattern works end-to-end
		const onload = vi.fn();
		await pluginRegistry.register(makePlugin("ext", onload), ["pty:write", "ui:sound"]);
		expect(onload).toHaveBeenCalledOnce();
	});

	it("built-in plugins (no second arg) have unrestricted access", () => {
		let host: PluginHost | null = null;
		pluginRegistry.register(
			makePlugin("builtin", (h) => {
				host = h;
			}),
		);
		// Should not throw PluginCapabilityError for any Tier 3 method
		expect(() => host!.openMarkdownPanel("Title", "plan:file")).not.toThrow(PluginCapabilityError);
	});
});

// ---------------------------------------------------------------------------
// PluginHost.log — per-plugin logging
// ---------------------------------------------------------------------------

describe("PluginHost.log", () => {
	it("writes to the plugin's logger via host.log()", () => {
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				host.log("info", "hello from plugin");
				host.log("error", "something broke", { code: 42 });
			}),
		);
		const logger = pluginStore.getLogger("p1");
		const entries = logger.getEntries();
		expect(entries).toHaveLength(2);
		expect(entries[0]).toMatchObject({ level: "info", message: "hello from plugin" });
		expect(entries[1]).toMatchObject({ level: "error", message: "something broke", data: { code: 42 } });
	});
});

// ---------------------------------------------------------------------------
// Error capture in plugin logger
// ---------------------------------------------------------------------------

describe("error capture in plugin logger", () => {
	it("captures onload errors in the plugin logger", () => {
		pluginRegistry.register(
			makePlugin("bad", () => {
				throw new Error("onload boom");
			}),
		);
		const logger = pluginStore.getLogger("bad");
		const entries = logger.getEntries();
		expect(entries.length).toBeGreaterThan(0);
		expect(entries.some((e) => e.level === "error" && e.message.includes("onload"))).toBe(true);
	});

	it("captures watcher errors in the plugin logger", async () => {
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				host.registerOutputWatcher({
					pattern: /boom/,
					onMatch: () => {
						throw new Error("watcher error");
					},
				});
			}),
		);
		pluginRegistry.dispatchLine("boom", "s1");
		await flushMicrotasks();
		const logger = pluginStore.getLogger("p1");
		expect(logger.getEntries().some((e) => e.level === "error" && e.message.includes("OutputWatcher"))).toBe(true);
	});

	it("captures structured handler errors in the plugin logger", async () => {
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				host.registerStructuredEventHandler("test-type", () => {
					throw new Error("handler error");
				});
			}),
		);
		pluginRegistry.dispatchStructuredEvent("test-type", {}, "s1");
		await flushMicrotasks();
		const logger = pluginStore.getLogger("p1");
		expect(logger.getEntries().some((e) => e.level === "error" && e.message.includes("handler error"))).toBe(true);
	});

	it("updates pluginStore loaded state on successful registration", () => {
		pluginStore.registerPlugin("p1", { loaded: false });
		pluginRegistry.register(makePlugin("p1"));
		expect(pluginStore.getPlugin("p1")?.loaded).toBe(true);
	});

	it("updates pluginStore with error on failed onload", () => {
		pluginStore.registerPlugin("bad", { loaded: false });
		pluginRegistry.register(
			makePlugin("bad", () => {
				throw new Error("fail!");
			}),
		);
		const state = pluginStore.getPlugin("bad");
		expect(state?.loaded).toBe(false);
		expect(state?.error).toBe("fail!");
	});
});

// ---------------------------------------------------------------------------
// Tier 2: getSessionCwd and getActiveRepoPath
// ---------------------------------------------------------------------------

describe("PluginHost — getSessionCwd", () => {
	beforeEach(() => {
		// Clean up any terminals added in previous tests
		for (const id of terminalsStore.getIds()) {
			terminalsStore.remove(id);
		}
	});

	it("returns null when sessionId is not found", () => {
		let result: string | null = "sentinel";
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				result = host.getSessionCwd("nonexistent-session");
			}),
		);
		expect(result).toBeNull();
	});

	it("returns the cwd for a known sessionId", () => {
		const termId = terminalsStore.add({
			sessionId: "sess-abc",
			fontSize: 14,
			name: "Test",
			cwd: "/home/user/project",
			awaitingInput: null,
		});
		let result: string | null = null;
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				result = host.getSessionCwd("sess-abc");
			}),
		);
		expect(result).toBe("/home/user/project");
		terminalsStore.remove(termId);
	});

	it("returns null when terminal cwd is null", () => {
		const termId = terminalsStore.add({
			sessionId: "sess-xyz",
			fontSize: 14,
			name: "Test",
			cwd: null,
			awaitingInput: null,
		});
		let result: string | null = "sentinel";
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				result = host.getSessionCwd("sess-xyz");
			}),
		);
		expect(result).toBeNull();
		terminalsStore.remove(termId);
	});
});

describe("PluginHost — getActiveRepoPath", () => {
	it("returns null when no active repo", () => {
		let result: string | null = "sentinel";
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				result = host.getActiveRepoPath();
			}),
		);
		expect(result).toBeNull();
	});

	it("returns the active repo path when one is set", () => {
		repositoriesStore.add({ path: "/my/repo", displayName: "my-repo" });
		repositoriesStore.setActive("/my/repo");
		let result: string | null = null;
		pluginRegistry.register(
			makePlugin("p1", (host) => {
				result = host.getActiveRepoPath();
			}),
		);
		expect(result).toBe("/my/repo");
		// Cleanup
		repositoriesStore.setActive(null);
		repositoriesStore.remove("/my/repo");
	});
});
