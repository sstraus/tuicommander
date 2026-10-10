import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// The connection state machine lives in `remote_runtime.rs` since #790-ef85:
// health probe, password-for-token exchange, status poll, SSH tunnel and the
// re-authentication after a daemon restart are all proven there, against a mock
// daemon (`remote_runtime::tests`). What is left here is a renderer, and these
// tests assert exactly that — it applies what the backend pushes, asks the
// backend to act, and does no networking of its own.
//
// It opens no stream to the daemon either. Since #791-055e `remote_mirror.rs`
// consumes the daemon's `/events` and repeats every frame on the local bus, so
// a remote event reaches this renderer through the ordinary local handlers. The
// `EventSource` the store used to own was a second, narrower copy of that pipe.
const eventSourceCtor = vi.fn();

const { invokeMock, subscribeEventsMock } = vi.hoisted(() => ({
	invokeMock: vi.fn(),
	subscribeEventsMock: vi.fn(),
}));
vi.mock("../../invoke", () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }));
// Partial: `appLogger` pulls `rpc` out of the same module, and a store that
// warns must not take the transport down with it.
vi.mock("../../transport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../transport")>()),
	subscribeEvents: (...args: unknown[]) => subscribeEventsMock(...args),
}));

// A machine's `agents.json` is cached per connection. Whether the cache is
// dropped and refilled on a status edge is this store's decision, so it is
// asserted here; what the registry then does with it belongs to its own tests.
const { ensureAgentConfigsMock, invalidateAgentConfigsMock } = vi.hoisted(() => ({
	ensureAgentConfigsMock: vi.fn(() => Promise.resolve({})),
	invalidateAgentConfigsMock: vi.fn(),
}));
vi.mock("../../stores/agentConfigs", () => ({
	ensureAgentConfigs: (...args: unknown[]) => ensureAgentConfigsMock(...(args as [])),
	invalidateAgentConfigs: (...args: unknown[]) => invalidateAgentConfigsMock(...(args as [])),
}));

import type { RemoteConnection } from "../../stores/remoteConnections";

type Store = typeof import("../../stores/remoteConnections").remoteConnectionsStore;
type StatusPayload = {
	id: string;
	status: string;
	base_url?: string;
	token?: string;
	protocol_version?: number;
	out_of_date?: boolean;
	error?: string;
	retry_after_secs?: number;
	step?: string;
};

function directConn(id: string): RemoteConnection {
	return {
		id,
		name: `conn-${id}`,
		transport: { type: "Direct", url: "http://remote.test:9876" },
		auth_username: "user",
		enabled: true,
		deploy: "never",
		survive_secs: 1800,
	};
}

const connected: StatusPayload = {
	id: "c1",
	status: "connected",
	base_url: "http://remote.test:9876",
	token: "tok-abc",
	protocol_version: 2,
};

describe("remoteConnectionsStore renders what the backend reports", () => {
	let store: Store;
	/** The `remote-connection-status` handler the store subscribed with. */
	let push: (payload: StatusPayload) => void;
	/** What `remote_connection_statuses` answers during the next hydrate. */
	let initialStatuses: StatusPayload[];
	/** Every backend interaction, in order — hydrate's ordering is load bearing. */
	let calls: string[];
	const fetchMock = vi.fn();

	beforeEach(async () => {
		vi.resetModules();
		vi.useFakeTimers();
		eventSourceCtor.mockReset();
		vi.stubGlobal("EventSource", eventSourceCtor);
		invokeMock.mockReset();
		subscribeEventsMock.mockReset();
		fetchMock.mockReset();
		vi.stubGlobal("fetch", fetchMock);
		calls = [];
		initialStatuses = [];

		subscribeEventsMock.mockImplementation(async (handlers: Record<string, (p: unknown) => void>) => {
			calls.push("subscribe");
			push = handlers["remote-connection-status"] as (payload: StatusPayload) => void;
			return () => {};
		});
		invokeMock.mockImplementation((command: string) => {
			calls.push(command);
			if (command === "list_remote_connections") return Promise.resolve([directConn("c1")]);
			if (command === "remote_connection_statuses") return Promise.resolve(initialStatuses);
			return Promise.resolve(undefined);
		});

		store = (await import("../../stores/remoteConnections")).remoteConnectionsStore;
	});

	afterEach(() => {
		vi.useRealTimers();
		vi.unstubAllGlobals();
	});

	it("subscribes before reading the statuses, so a change in between is not lost", async () => {
		await store.hydrate();
		expect(calls).toEqual(["list_remote_connections", "subscribe", "remote_connection_statuses"]);
	});

	it("starts with no remote routes before hydration", () => {
		expect(store.getConnections()).toEqual({});
		expect(store.getBaseUrl("c1")).toBeUndefined();
	});

	it("hydrates once when two consumers request the loaded connection list", async () => {
		await store.hydrate();
		await store.hydrate();

		expect(calls).toEqual(["list_remote_connections", "subscribe", "remote_connection_statuses"]);
		expect(Object.keys(store.getConnections())).toEqual(["c1"]);
	});

	it("an empty backend connection list gives consumers an empty collection", async () => {
		invokeMock.mockImplementation((command: string) => {
			if (command === "list_remote_connections") return Promise.resolve([]);
			if (command === "remote_connection_statuses") return Promise.resolve([]);
			return Promise.resolve(undefined);
		});
		await store.hydrate();

		expect(store.getConnections()).toEqual({});
	});

	it("a missing backend connection list gives consumers an empty collection", async () => {
		invokeMock.mockImplementation((command: string) => {
			if (command === "list_remote_connections") return Promise.resolve(null);
			if (command === "remote_connection_statuses") return Promise.resolve([]);
			return Promise.resolve(undefined);
		});
		await store.hydrate();

		expect(store.getConnections()).toEqual({});
	});

	it("hydrate adopts the live status the backend already holds", async () => {
		initialStatuses = [connected];
		await store.hydrate();

		expect(store.getConnectionState("c1")?.status).toBe("connected");
		expect(store.getBaseUrl("c1")).toBe("http://remote.test:9876");
		expect(store.getToken("c1")).toBe("tok-abc");
	});

	describe("once hydrated", () => {
		beforeEach(async () => {
			await store.hydrate();
		});

		it("sends the loaded base for edits and null for a new connection", async () => {
			const loaded = store.getConnectionState("c1")!.connection;
			const edited = { ...loaded, auto_update: true };
			await store.addConnection(edited);
			const added = directConn("c2");
			await store.addConnection(added);

			expect(invokeMock).toHaveBeenCalledWith("save_remote_connection", { base: loaded, connection: edited });
			expect(invokeMock).toHaveBeenCalledWith("save_remote_connection", { base: null, connection: added });
			expect(store.getConnectionState("c2")).toMatchObject({ connection: added, status: "disconnected" });
		});

		it("a rejected save leaves the loaded connection unchanged", async () => {
			const original = store.getConnectionState("c1")!.connection;
			invokeMock.mockRejectedValueOnce(new Error("save failed"));

			await expect(store.addConnection({ ...original, name: "edited" })).rejects.toThrow("save failed");
			expect(store.getConnectionState("c1")?.connection.name).toBe(original.name);
		});

		it("connect asks the backend and waits: the status arrives as a push", async () => {
			await store.connect("c1");

			expect(invokeMock).toHaveBeenCalledWith("connect_remote_connection", { id: "c1" });
			// Nothing moved yet — the renderer does not guess at a status.
			expect(store.getConnectionState("c1")?.status).toBe("disconnected");

			push({ id: "c1", status: "connecting" });
			expect(store.getConnectionState("c1")?.status).toBe("connecting");

			push(connected);
			expect(store.getConnectionState("c1")?.status).toBe("connected");
			expect(store.getConnectionState("c1")?.protocolVersion).toBe(2);
		});

		it("clears failed-attempt details when the next backend attempt begins", () => {
			push({ id: "c1", status: "error", error: "Cannot resolve mac-mint", retry_after_secs: 3 });
			expect(store.getConnectionState("c1")).toMatchObject({ error: "Cannot resolve mac-mint", retryAfterSecs: 3 });
			push({ id: "c1", status: "connecting" });
			expect(store.getConnectionState("c1")?.error).toBeUndefined();
			expect(store.getConnectionState("c1")?.retryAfterSecs).toBeUndefined();
		});

		it("keeps the deployment step from the backend status", () => {
			push({ id: "c1", status: "deploying", step: "starting daemon" });

			expect(store.getConnectionState("c1")).toMatchObject({
				status: "deploying",
				deployStep: "starting daemon",
			});
		});

		it("shows the backend's out-of-date result and clears it when the connection drops", () => {
			push({ ...connected, out_of_date: true });
			expect(store.getConnectionState("c1")?.outOfDate).toBe(true);
			push({ id: "c1", status: "disconnected" });
			expect(store.getConnectionState("c1")?.outOfDate).toBeUndefined();
		});

		it("sends the confirmed count and selected hash to the backend", async () => {
			const preview = {
				session_count: 3,
				desktop_build: { version: "1.2.3", target: "aarch64-apple-darwin", sha256: "fixture-hash" },
			};
			invokeMock.mockResolvedValueOnce(preview);
			await expect(store.prepareUpdate("c1")).resolves.toEqual(preview);
			expect(invokeMock).toHaveBeenCalledWith("prepare_remote_update", { id: "c1" });
			await store.updateAndRestart("c1", 3, "fixture-hash");
			expect(invokeMock).toHaveBeenCalledWith("update_and_restart_remote", {
				id: "c1",
				confirmedSessions: 3,
				expectedSha256: "fixture-hash",
			});
		});

		it("unknown install, uninstall and removal leave the backend untouched", async () => {
			await store.install("ghost");
			await store.uninstall("ghost");
			await store.removeConnection("ghost");

			expect(invokeMock.mock.calls.map((c) => c[0])).toEqual(["list_remote_connections", "remote_connection_statuses"]);
		});

		it("install and uninstall update the persisted deploy mode after the backend succeeds", async () => {
			await store.install("c1");
			expect(invokeMock).toHaveBeenCalledWith("install_remote_daemon", { id: "c1" });
			expect(store.getConnectionState("c1")?.connection.deploy).toBe("installed");

			await store.uninstall("c1");
			expect(invokeMock).toHaveBeenCalledWith("uninstall_remote_daemon", { id: "c1" });
			expect(store.getConnectionState("c1")?.connection.deploy).toBe("on_connect");
		});

		it("returns the probed SSH host states from the backend", async () => {
			invokeMock.mockResolvedValueOnce([{ host: "builder", auth: "shell" }]);

			await expect(store.probeSshHosts()).resolves.toEqual([{ host: "builder", auth: "shell" }]);
			expect(invokeMock).toHaveBeenCalledWith("probe_ssh_config_hosts");
		});

		it("returns an empty host list when the backend has no probe result", async () => {
			invokeMock.mockResolvedValueOnce(null);

			await expect(store.probeSshHosts()).resolves.toEqual([]);
		});

		it("reports the password vault result for this connection", async () => {
			invokeMock.mockResolvedValueOnce(true);
			await expect(store.hasPassword("c1")).resolves.toBe(true);
			expect(invokeMock).toHaveBeenCalledWith("remote_connection_password_exists", { id: "c1" });

			invokeMock.mockResolvedValueOnce(false);
			await expect(store.hasPassword("c1")).resolves.toBe(false);

			invokeMock.mockResolvedValueOnce(null);
			await expect(store.hasPassword("c1")).resolves.toBe(false);
		});

		it("does no networking and runs no poll of its own", async () => {
			await store.connect("c1");
			push(connected);
			await vi.advanceTimersByTimeAsync(60_000);

			// The poll is a Rust task now. A second one here would double every
			// probe and could contradict the backend's own status. The stream is
			// a Rust task too, so a connected connection opens no `EventSource`.
			expect(fetchMock).not.toHaveBeenCalled();
			expect(eventSourceCtor).not.toHaveBeenCalled();
			const commands = invokeMock.mock.calls.map((c) => c[0]);
			expect(commands).toEqual(["list_remote_connections", "remote_connection_statuses", "connect_remote_connection"]);
		});

		it("disconnect forgets the token and tells the backend", async () => {
			push(connected);
			expect(store.getToken("c1")).toBe("tok-abc");

			await store.disconnect("c1");

			expect(store.getToken("c1")).toBeUndefined();
			expect(invokeMock).toHaveBeenCalledWith("disconnect_remote_connection", { id: "c1" });

			// …and the push that follows agrees, without starting anything again.
			push({ id: "c1", status: "disconnected" });
			expect(store.getConnectionState("c1")?.status).toBe("disconnected");
			expect(store.getConnectionState("c1")?.baseUrl).toBeUndefined();
			expect(store.getBaseUrl("c1")).toBeUndefined();
		});

		it("reconnecting restores the route the disconnect retracted", () => {
			push(connected);
			push({ id: "c1", status: "disconnected" });
			expect(store.getBaseUrl("c1")).toBeUndefined();

			push(connected);
			expect(store.getBaseUrl("c1")).toBe("http://remote.test:9876");
			expect(store.getToken("c1")).toBe("tok-abc");
		});

		/**
		 * A run config describes the machine, so the cached copy is only valid
		 * while this exact daemon is up: it may have been reinstalled, edited, or
		 * be a different box behind the same name by the time it answers again.
		 */
		describe("the machine's agent config follows the connection", () => {
			beforeEach(() => {
				ensureAgentConfigsMock.mockClear();
				invalidateAgentConfigsMock.mockClear();
			});

			it("reads it once the machine is up, not on every status repeat", () => {
				push(connected);
				push(connected);

				expect(ensureAgentConfigsMock).toHaveBeenCalledTimes(1);
				expect(ensureAgentConfigsMock).toHaveBeenCalledWith("c1");
			});

			it("drops it when the machine goes away", () => {
				push(connected);
				invalidateAgentConfigsMock.mockClear();

				push({ id: "c1", status: "disconnected" });

				expect(invalidateAgentConfigsMock).toHaveBeenCalledWith("c1");
			});

			it("does not evict a machine cache for repeated disconnected status", () => {
				push({ id: "c1", status: "disconnected" });
				push({ id: "c1", status: "disconnected" });

				expect(invalidateAgentConfigsMock).not.toHaveBeenCalled();
			});

			it("re-reads it on reconnect rather than trusting the old copy", () => {
				push(connected);
				push({ id: "c1", status: "disconnected" });
				ensureAgentConfigsMock.mockClear();
				invalidateAgentConfigsMock.mockClear();

				push(connected);

				expect(invalidateAgentConfigsMock).toHaveBeenCalledWith("c1");
				expect(ensureAgentConfigsMock).toHaveBeenCalledWith("c1");
			});

			it("drops it when the daemon stops taking the credential", () => {
				push(connected);
				invalidateAgentConfigsMock.mockClear();

				push({ id: "c1", status: "unauthenticated" });

				expect(invalidateAgentConfigsMock).toHaveBeenCalledWith("c1");
			});
		});

		it("connect on an unknown connection reaches no backend", async () => {
			await store.connect("ghost");
			expect(invokeMock).not.toHaveBeenCalledWith("connect_remote_connection", { id: "ghost" });
		});

		it("disconnect on an unknown connection is a safe no-op", async () => {
			await expect(store.disconnect("ghost")).resolves.toBeUndefined();
			expect(invokeMock).not.toHaveBeenCalledWith("disconnect_remote_connection", { id: "ghost" });
		});

		// The bug the whole feature exists for: `/health` is the only route
		// tuic-remote serves without a credential, so a client reading "connected"
		// off it then 401s on every real call. The backend now distinguishes the
		// two, and the store must render the distinction rather than flatten it.
		it("an unauthenticated connection routes nothing", () => {
			push({
				id: "c1",
				status: "unauthenticated",
				error: "The remote daemon rejected these credentials — check the username and password.",
			});

			const state = store.getConnectionState("c1");
			expect(state?.status).toBe("unauthenticated");
			expect(state?.error).toContain("rejected these credentials");
			expect(store.getBaseUrl("c1")).toBeUndefined();
			expect(store.getToken("c1")).toBeUndefined();
		});

		it("a status without a token forgets the one it held", () => {
			push(connected);
			expect(store.getToken("c1")).toBe("tok-abc");

			// The daemon restarted and rejected the old token: the backend says so
			// in one push, and a renderer holding the stale copy would sign calls
			// with a credential that is already dead.
			push({ id: "c1", status: "error", error: "Unreachable" });
			expect(store.getToken("c1")).toBeUndefined();
			expect(store.getBaseUrl("c1")).toBeUndefined();
		});

		it("a re-authenticated connection is signed with the new token", () => {
			push(connected);
			push({ ...connected, token: "tok-2" });

			expect(store.getToken("c1")).toBe("tok-2");
			expect(store.getConnectionState("c1")?.status).toBe("connected");
			expect(store.getBaseUrl("c1")).toBe("http://remote.test:9876");
		});

		it("a status for a connection this store does not know is dropped", () => {
			push({ id: "ghost", status: "connected", base_url: "http://ghost:9876", token: "t" });
			expect(store.getConnectionState("ghost")).toBeUndefined();
			expect(store.getToken("ghost")).toBeUndefined();
			expect(store.getBaseUrl("ghost")).toBeUndefined();
		});

		it("does not route a disconnected connection even if a stale URL arrives", () => {
			push({ id: "c1", status: "disconnected", base_url: "http://stale.test:9876" });

			expect(store.getBaseUrl("c1")).toBeUndefined();
		});

		it("registers the connected URL and token with transport routing", async () => {
			const { getRemoteBaseUrl, getRemoteToken } = await import("../../transportRuntime");
			push(connected);

			expect(getRemoteBaseUrl("c1")).toBe("http://remote.test:9876");
			expect(getRemoteToken("c1")).toBe("tok-abc");
		});

		it("the password goes to the vault through the backend and is never held here", async () => {
			await store.setPassword("c1", "s3cret");
			expect(invokeMock).toHaveBeenCalledWith("set_remote_connection_password", {
				id: "c1",
				password: "s3cret",
			});
			expect(JSON.stringify(store.getConnections())).not.toContain("s3cret");
		});

		it("removing a live connection deletes it without a separate disconnect", async () => {
			// The backend tears the connection down inside delete — poll, mirror,
			// mirrored rows, tunnel and token — so a disconnect from here bought
			// nothing, and it hid the fact that the HTTP delete route did none of
			// it. The one thing that IS this process's to drop is the token cache.
			push(connected);
			expect(store.getToken("c1")).toBe("tok-abc");

			await store.removeConnection("c1");

			const commands = invokeMock.mock.calls.map((c) => c[0]);
			expect(commands).toContain("delete_remote_connection");
			expect(invokeMock).toHaveBeenCalledWith("delete_remote_connection", { id: "c1" });
			expect(commands).not.toContain("disconnect_remote_connection");
			expect(store.getConnectionState("c1")).toBeUndefined();
			expect(store.getToken("c1")).toBeUndefined();
		});

		it("a rejected delete keeps the connection available", async () => {
			invokeMock.mockRejectedValueOnce(new Error("delete failed"));

			await expect(store.removeConnection("c1")).rejects.toThrow("delete failed");
			expect(store.getConnectionState("c1")?.connection.id).toBe("c1");
		});
	});
});
