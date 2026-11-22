import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { invoke } from "../../invoke";
import { pluginRegistry } from "../../plugins/pluginRegistry";
import type { PluginHost } from "../../plugins/types";
import { repositoriesStore } from "../../stores/repositories";

const transport = vi.hoisted(() => ({ subscribe: vi.fn(), owner: vi.fn(), baseUrl: vi.fn() }));
vi.mock("../../invoke", () => ({
	invoke: vi.fn(() => Promise.resolve()),
	listen: vi.fn(() => Promise.resolve(() => {})),
}));
vi.mock("../../transport", () => ({ subscribeEvents: transport.subscribe, owningConnectionFor: transport.owner }));
vi.mock("../../transportRuntime", async (original) => ({
	...(await original<object>()),
	getRemoteBaseUrl: transport.baseUrl,
}));

const repo = "/story-count-host-test";
async function host(capabilities: string[] = ["stories:read"]): Promise<PluginHost> {
	let result!: PluginHost;
	await pluginRegistry.register(
		{
			id: "story-count-host-test",
			onload(value) {
				result = value;
			},
			onunload() {},
		},
		capabilities,
	);
	return result;
}
beforeEach(() => {
	vi.clearAllMocks();
	transport.owner.mockReturnValue(undefined);
	transport.subscribe.mockResolvedValue(vi.fn());
	repositoriesStore.add({ path: repo, displayName: "Count test" });
});
afterEach(async () => {
	await pluginRegistry.clear();
	repositoriesStore.remove(repo);
	repositoriesStore._testCancelPendingSave();
});

// Catches: aggregate access bypassing its dedicated capability or registered-project boundary.
it("rejects missing capability and unregistered repositories before invoking", async () => {
	const denied = await host([]);
	await expect(denied.getStoryCount(repo)).rejects.toThrow("stories:read");
	await expect(denied.watchStoryCount(repo, vi.fn())).rejects.toThrow("stories:read");
	const allowed = await host();
	await expect(allowed.getStoryCount("/unregistered")).rejects.toThrow("registered repository");
	await expect(allowed.watchStoryCount("/unregistered", vi.fn())).rejects.toThrow("registered repository");
	expect(invoke).not.toHaveBeenCalledWith("story_action_command", expect.anything());
});

// Catches: exposing an arbitrary action or reading a different backend than the core projection.
it("requests only the fixed aggregate action and returns its owner snapshot", async () => {
	const api = await host();
	const snapshot = { project: repo, backend: "external:wiz-stories", generation: "g1", openCount: 7 };
	vi.mocked(invoke).mockResolvedValueOnce({ type: "open_story_count", value: snapshot });
	expect(await api.getStoryCount(repo)).toEqual(snapshot);
	expect(invoke).toHaveBeenLastCalledWith("story_action_command", {
		project: repo,
		action: { action: "open_story_count" },
	});
	vi.mocked(invoke).mockResolvedValueOnce({ type: "project_story_view", value: {} });
	await expect(api.getStoryCount(repo)).rejects.toThrow("Unexpected story count reply");
});

// Catches: remote subscriptions silently attaching to localhost or losing reconnect invalidation.
it("subscribes to the owning host and stops invalidations on unload", async () => {
	transport.owner.mockReturnValue("remote-count");
	transport.baseUrl.mockReturnValue("http://remote-count.invalid");
	const stop = vi.fn();
	transport.subscribe.mockResolvedValue(stop);
	const api = await host();
	const changed = vi.fn();
	await api.watchStoryCount(repo, changed);
	const [handlers, options] = transport.subscribe.mock.calls[0];
	expect(options.baseUrl).toBe("http://remote-count.invalid");
	for (const name of ["stories-changed", "boss-attention-changed", "workflow-run-changed"]) handlers[name]({});
	options.onResync("reconnect");
	expect(changed).toHaveBeenCalledTimes(4);
	await pluginRegistry.unregister("story-count-host-test");
	expect(stop).toHaveBeenCalledOnce();
	handlers["stories-changed"]({});
	expect(changed).toHaveBeenCalledTimes(4);
});

// Catches: a slow event subscription leaking after its plugin has been unloaded.
it("disposes a subscription that resolves after unload", async () => {
	let resolve!: (stop: () => void) => void;
	transport.subscribe.mockReturnValue(
		new Promise((done) => {
			resolve = done;
		}),
	);
	const api = await host();
	const pending = api.watchStoryCount(repo, vi.fn());
	await pluginRegistry.unregister("story-count-host-test");
	const stop = vi.fn();
	resolve(stop);
	await pending;
	expect(stop).toHaveBeenCalledOnce();
});

// Catches: a disconnected remote count being replaced with a local subscription.
it("rejects a disconnected owner without subscribing locally", async () => {
	transport.owner.mockReturnValue("offline-count");
	transport.baseUrl.mockReturnValue(undefined);
	const api = await host();
	await expect(api.watchStoryCount(repo, vi.fn())).rejects.toThrow("disconnected");
	expect(transport.subscribe).not.toHaveBeenCalled();
});

// Catches: filesystem connector changes missing the repository revision invalidation path.
it("invalidates on repository revisions and disposes that observer", async () => {
	const api = await host();
	const changed = vi.fn();
	const subscription = await api.watchStoryCount(repo, changed);
	await Promise.resolve();
	expect(changed).not.toHaveBeenCalled();
	repositoriesStore.bumpRevision(repo);
	await Promise.resolve();
	expect(changed).toHaveBeenCalledOnce();
	subscription.dispose();
	repositoriesStore.bumpRevision(repo);
	await Promise.resolve();
	expect(changed).toHaveBeenCalledOnce();
});
