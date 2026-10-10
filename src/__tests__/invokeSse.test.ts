import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

// Browser mode — this is the path that serves the web UI / PWA / remote client.
vi.mock("../transport", () => ({
	isTauri: () => false,
	rpc: vi.fn(),
}));

/** Records every stream the module opens, and whether it was closed. */
class MockEventSource {
	static readonly instances: MockEventSource[] = [];
	static readonly CONNECTING = 0;
	static readonly OPEN = 1;
	static readonly CLOSED = 2;
	readyState = MockEventSource.OPEN;
	url: string;
	closed = false;
	onerror: ((ev: unknown) => void) | null = null;
	onopen: ((ev: unknown) => void) | null = null;
	readonly attached: string[] = [];

	constructor(url: string | URL) {
		this.url = String(url);
		MockEventSource.instances.push(this);
	}

	addEventListener(type: string): void {
		this.attached.push(type);
	}
	removeEventListener(): void {}
	close(): void {
		this.closed = true;
		this.readyState = MockEventSource.CLOSED;
	}
}

/** The most recently opened stream. */
function last(): MockEventSource | undefined {
	return MockEventSource.instances.at(-1)!;
}

/** Opens are coalesced onto a macrotask, so drain one. */
function flush(): Promise<void> {
	return new Promise((resolve) => setTimeout(resolve, 0));
}

/** Every filter update the module posted, in order. */
const filterUpdates: Array<{ stream_id: string; types: string[] }> = [];

/** Accept filter updates, or refuse them with `status` to force the fallback. */
function mockFetch(status = 204): void {
	vi.stubGlobal(
		"fetch",
		vi.fn((_url: string, init: { body: string }) => {
			filterUpdates.push(JSON.parse(init.body));
			return Promise.resolve({ ok: status < 400, status } as Response);
		}),
	);
}

/** The `types` the given stream URL subscribes to. */
function typesOf(url: string): string[] {
	const query = url.split("?")[1] ?? "";
	const raw = new URLSearchParams(query).get("types");
	expect(raw, `stream URL carries no type filter: ${url}`).toBeTruthy();
	return raw!.split(",").sort();
}

/** A fresh module instance — the SSE connection is module-level state. */
async function freshInvoke() {
	vi.resetModules();
	return await import("../invoke");
}

describe("browser-mode shared SSE subscription", () => {
	beforeEach(() => {
		MockEventSource.instances.length = 0;
		filterUpdates.length = 0;
		vi.stubGlobal("EventSource", MockEventSource);
		mockFetch();
	});

	it("subscribes only to the event types that were registered", async () => {
		const { listen } = await freshInvoke();
		await listen("repo-changed", () => {});
		await flush();

		const stream = last();
		expect(stream, "no stream was opened").toBeTruthy();
		expect(typesOf(stream!.url)).toEqual(["repo-changed"]);
	});

	it("widens the live stream in place when a new type is registered after connect", async () => {
		const { listen } = await freshInvoke();
		await listen("repo-changed", () => {});
		await flush();
		const first = last()!;
		expect(typesOf(first.url)).toEqual(["repo-changed"]);

		// Reconnecting to widen the filter loses every event published between
		// the close and the new subscription — the server replays nothing.
		await listen("session-created", () => {});
		await flush();

		expect(MockEventSource.instances, "the connection must survive a widening").toHaveLength(1);
		expect(first.closed).toBe(false);
		expect(filterUpdates).toHaveLength(1);
		expect(filterUpdates[0].types.sort()).toEqual(["repo-changed", "session-created"]);
		expect(filterUpdates[0].stream_id, "the update names the stream it applies to").toBeTruthy();
		expect(filterUpdates[0].stream_id).toBe(new URLSearchParams(first.url.split("?")[1]).get("stream_id"));
	});

	it("falls back to reconnecting when the server does not know the stream", async () => {
		mockFetch(404);
		const { listen } = await freshInvoke();
		await listen("repo-changed", () => {});
		await flush();
		const first = last()!;

		await listen("session-created", () => {});
		await flush();
		await flush();

		const second = last()!;
		expect(second, "a new type must not be silently undeliverable").not.toBe(first);
		expect(first.closed, "the superseded stream must be closed, not leaked").toBe(true);
		expect(typesOf(second.url)).toEqual(["repo-changed", "session-created"]);
	});

	it("re-applies the widened filter after an auto-reconnect", async () => {
		const { listen } = await freshInvoke();
		await listen("repo-changed", () => {});
		await flush();
		await listen("session-created", () => {});
		await flush();
		expect(filterUpdates).toHaveLength(1);

		// EventSource reconnects on its own, replaying the URL — which asks for
		// the original narrow filter. Without re-applying, the widened types stop
		// arriving and nothing reports an error.
		last()!.onopen?.({});
		await flush();

		expect(filterUpdates).toHaveLength(2);
		expect(filterUpdates[1].types.sort()).toEqual(["repo-changed", "session-created"]);
	});

	it("opens a single stream for a burst of registrations", async () => {
		const { listen } = await freshInvoke();
		// Startup registers many types back to back. One connection, not one each.
		await listen("repo-changed", () => {});
		await listen("session-created", () => {});
		await listen("pty-exit", () => {});
		await flush();

		expect(MockEventSource.instances).toHaveLength(1);
		expect(typesOf(MockEventSource.instances[0].url)).toEqual(["pty-exit", "repo-changed", "session-created"]);
	});

	it("does not reopen for a second handler on an already-covered type", async () => {
		const { listen } = await freshInvoke();
		await listen("repo-changed", () => {});
		await flush();
		await listen("repo-changed", () => {});
		await flush();

		expect(MockEventSource.instances).toHaveLength(1);
	});

	it("wires the newly covered type on the live stream, exactly once", async () => {
		const { listen } = await freshInvoke();
		await listen("repo-changed", () => {});
		await flush();
		await listen("session-created", () => {});
		await flush();

		const stream = last()!;
		expect(stream.attached).toContain("repo-changed");
		expect(stream.attached).toContain("session-created");
		// Re-attaching a type already wired dispatches every event to it twice.
		expect(stream.attached.filter((t) => t === "repo-changed")).toHaveLength(1);
	});
});
