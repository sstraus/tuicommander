import { batch } from "solid-js";
import { createStore, produce } from "solid-js/store";
import { invoke } from "../invoke";
import { subscribeEvents } from "../transport";
import { setRemoteBaseUrlLookup, setRemoteTokenLookup } from "../transportRuntime";
import { ensureAgentConfigs, invalidateAgentConfigs } from "./agentConfigs";
import { appLogger } from "./appLogger";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface RemoteConnection {
	id: string;
	name: string;
	transport: RemoteTransport;
	auth_username: string;
	enabled: boolean;
	auto_update?: boolean;
	deploy: DeployMode;
	survive_secs: number;
}

export type DeployMode = "never" | "on_connect" | "installed";

export interface DiscoveredSshHost {
	/** Name to show and to connect with: the config alias or the known_hosts name. */
	host: string;
	/** Resolved machine; with `port` it identifies the entry. */
	target: string;
	user: string | null;
	port: number | null;
	source: "config" | "known_hosts";
}

export interface DiscoveredSshHosts {
	hosts: DiscoveredSshHost[];
	/** known_hosts entries with hashed names: present on disk, not listable. */
	hashed_count: number;
}

export interface SshAgentInfo {
	keys: { fingerprint: string; comment: string; key_type: string }[];
	agent_type: string;
}

export interface SshHostStatus {
	host: string;
	target: string;
	port: number | null;
	auth: "shell" | "no_shell" | "auth_failed" | "unreachable";
}

export type RemoteTransport =
	| {
			type: "Ssh";
			ssh_host: string;
			ssh_port: number;
			ssh_user: string;
			identity_file: string | null;
			remote_daemon_port: number;
	  }
	| { type: "Direct"; url: string };

/**
 * `unauthenticated` is deliberately not `error`: the daemon is reachable and
 * `/health` answers, but every other route needs a credential this client does
 * not have. The fix is a password, not a network one, and the panel says so.
 */
export type ConnectionStatus = "disconnected" | "connecting" | "deploying" | "connected" | "unauthenticated" | "error";

export interface ConnectionState {
	connection: RemoteConnection;
	status: ConnectionStatus;
	baseUrl?: string;
	protocolVersion?: number;
	outOfDate?: boolean;
	liveSessions?: number;
	updateNotice?: string;
	updateInProgress?: boolean;
	error?: string;
	retryAfterSecs?: number;
	deployStep?: string;
}

/** One connection as the backend reports it. Snake case: it is a Rust struct. */
interface RemoteConnectionStatusPayload {
	id: string;
	status: ConnectionStatus;
	base_url?: string;
	token?: string;
	protocol_version?: number;
	out_of_date?: boolean;
	live_sessions?: number;
	update_notice?: string;
	update_in_progress?: boolean;
	error?: string;
	retry_after_secs?: number;
	step?: string;
}

interface RemoteConnectionsState {
	connections: Record<string, ConnectionState>;
	hydrated: boolean;
}

export interface RemoteBuildIdentity {
	version: string;
	target: string;
	sha256: string;
}

export interface RemoteUpdatePreview {
	remote_build: RemoteBuildIdentity | null;
	desktop_build: RemoteBuildIdentity;
	source: "release" | "local";
	session_count: number;
	out_of_date: boolean;
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

/**
 * This store renders remote connections; it does not run them.
 *
 * The state machine — health probe, password-for-token exchange, status polling,
 * SSH tunnel, base URL — lives in `remote_runtime.rs` (#790-ef85). It has to:
 * a base URL and a token held in the JS heap can only be used by JS, so no
 * backend task could ever reach a remote daemon, and everything that makes a
 * remote session behave like a local one needs exactly that.
 *
 * What stays here is what a renderer owns: the merged view of configuration plus
 * live status. A remote daemon's events are mirrored onto the local bus by
 * `remote_mirror.rs` and arrive through the ordinary local handlers, so this
 * store subscribes to no remote stream of its own (#791-055e).
 */

/** Guard: prevent hydrate from running twice */
let hydrated = false;

/**
 * Session tokens for connected connections, keyed by connection ID.
 *
 * Deliberately outside the store and never persisted: the daemon mints this in
 * memory and forgets it on every restart, so a stored copy would be a stale
 * credential sitting on disk. It arrives with each status push; the password it
 * was traded for never leaves the Rust side.
 */
const remoteTokens = new Map<string, string>();

/**
 * Whether the status stream has been subscribed.
 *
 * A flag rather than the `Unsubscribe` handle: the subscription lasts as long as
 * the process does, so nothing ever calls it, and an unread handle reads as dead
 * state. What is load bearing is subscribing exactly once.
 */
let statusSubscribed = false;

function createRemoteConnectionsStore() {
	const [state, setState] = createStore<RemoteConnectionsState>({
		connections: {},
		hydrated: false,
	});

	// ---------------------------------------------------------------------------
	// Internal helpers
	// ---------------------------------------------------------------------------

	/**
	 * Apply one backend status snapshot.
	 *
	 * The payload is the whole client view of a connection, never a delta, so a
	 * missed push cannot leave this store holding a base URL the backend has
	 * retracted: the next one overwrites all of it.
	 */
	function applyStatus(payload: RemoteConnectionStatusPayload): void {
		const existing = state.connections[payload.id];
		if (!existing) {
			// A status for a connection this store has not hydrated yet. Dropping
			// it is safe: hydrate reads the same snapshot it came from.
			return;
		}
		if (payload.token) remoteTokens.set(payload.id, payload.token);
		else remoteTokens.delete(payload.id);

		const wasConnected = existing.status === "connected";

		setState("connections", payload.id, {
			status: payload.status,
			baseUrl: payload.base_url,
			protocolVersion: payload.protocol_version,
			outOfDate: payload.out_of_date,
			liveSessions: payload.live_sessions,
			updateNotice: payload.update_notice,
			updateInProgress: payload.update_in_progress,
			error: payload.error,
			retryAfterSecs: payload.retry_after_secs,
			deployStep: payload.step,
		});

		// A machine's run configs belong to the machine, and a daemon that went
		// away and came back may have been reconfigured — or be a different box
		// behind the same name. Drop the cached copy on every edge, then read the
		// new one straight away so a context menu opened later is already warm:
		// a menu is built inside the click that opens it and cannot await.
		if (payload.status === "connected") {
			if (!wasConnected) {
				invalidateAgentConfigs(payload.id);
				void ensureAgentConfigs(payload.id).catch((err) =>
					appLogger.warn("store", `Failed to read agent config of ${payload.id}`, err),
				);
			}
		} else if (wasConnected) {
			invalidateAgentConfigs(payload.id);
		}
	}

	// ---------------------------------------------------------------------------
	// Actions
	// ---------------------------------------------------------------------------

	const actions = {
		/** Load connections and their live status, then follow the status pushes. */
		async hydrate(): Promise<void> {
			if (hydrated) return;
			try {
				const connections = await invoke<RemoteConnection[]>("list_remote_connections");
				const connectionsMap: Record<string, ConnectionState> = {};
				for (const conn of connections ?? []) {
					connectionsMap[conn.id] = { connection: conn, status: "disconnected" };
				}
				batch(() => {
					setState("connections", connectionsMap);
					setState("hydrated", true);
				});
				hydrated = true;

				// Status before the subscription would race a change landing in
				// between; the other order only ever re-applies the same value.
				if (!statusSubscribed) {
					await subscribeEvents({
						"remote-connection-status": (payload) => applyStatus(payload as RemoteConnectionStatusPayload),
					});
					statusSubscribed = true;
				}
				const statuses = await invoke<RemoteConnectionStatusPayload[]>("remote_connection_statuses");
				for (const status of statuses ?? []) applyStatus(status);
			} catch (err) {
				appLogger.error("store", "Failed to hydrate remote connections", err);
			}
		},

		/**
		 * Ask the backend to bring a connection up.
		 *
		 * Every transition it goes through — connecting, then connected or one of
		 * the two failures — arrives as a status push, so this does not set state
		 * itself. A rejected password resolves here as a rejection AND as an
		 * `unauthenticated` status; callers that only render can ignore the throw.
		 */
		async connect(id: string): Promise<void> {
			if (!state.connections[id]) {
				appLogger.warn("store", `connect: unknown connection ${id}`);
				return;
			}
			await invoke("connect_remote_connection", { id });
		},

		/** Ask the backend to take a connection down. */
		async disconnect(id: string): Promise<void> {
			if (!state.connections[id]) return;
			remoteTokens.delete(id);
			await invoke("disconnect_remote_connection", { id });
		},

		async prepareUpdate(id: string): Promise<RemoteUpdatePreview> {
			return await invoke<RemoteUpdatePreview>("prepare_remote_update", { id });
		},

		async updateAndRestart(id: string, confirmedSessions: number, expectedSha256: string): Promise<void> {
			await invoke("update_and_restart_remote", { id, confirmedSessions, expectedSha256 });
		},

		async install(id: string): Promise<void> {
			const current = state.connections[id];
			if (!current) return;
			await invoke("install_remote_daemon", { id });
			setState("connections", id, "connection", "deploy", "installed");
		},

		async uninstall(id: string): Promise<void> {
			const current = state.connections[id];
			if (!current) return;
			await invoke("uninstall_remote_daemon", { id });
			setState("connections", id, "connection", "deploy", "on_connect");
		},

		async discoverSshHosts(): Promise<DiscoveredSshHosts> {
			return (await invoke<DiscoveredSshHosts>("list_discovered_ssh_hosts")) ?? { hosts: [], hashed_count: 0 };
		},

		async sshAgentInfo(): Promise<SshAgentInfo> {
			return (await invoke<SshAgentInfo>("list_ssh_agent_keys")) ?? { keys: [], agent_type: "" };
		},

		/** Probe one discovered host on request; known_hosts entries are only ever probed this way. */
		async probeSshHost(host: DiscoveredSshHost): Promise<SshHostStatus> {
			return invoke<SshHostStatus>("probe_discovered_ssh_host", { target: host.target, port: host.port });
		},

		async probeSshHosts(): Promise<SshHostStatus[]> {
			return (await invoke<SshHostStatus[]>("probe_ssh_config_hosts")) ?? [];
		},

		/** Save a new connection to the backend and add it to state */
		async addConnection(conn: RemoteConnection): Promise<void> {
			try {
				const base = state.connections[conn.id]?.connection ?? null;
				await invoke("save_remote_connection", { base, connection: conn });
				setState("connections", conn.id, { connection: conn, status: "disconnected" });
			} catch (err) {
				appLogger.error("store", "Failed to save remote connection", err);
				throw err;
			}
		},

		/** Delete from the backend and remove from state */
		async removeConnection(id: string): Promise<void> {
			const connState = state.connections[id];
			if (!connState) return;

			// No pre-delete disconnect. `delete_remote_connection` tears the
			// connection down itself — poll, mirror, mirrored rows, tunnel and
			// token — before it rewrites the store, so asking from here bought
			// nothing and hid the fact that the HTTP route never did. Only this
			// process's token cache is ours to clear.
			remoteTokens.delete(id);

			try {
				await invoke("delete_remote_connection", { id });
				setState(
					produce((s) => {
						delete s.connections[id];
					}),
				);
			} catch (err) {
				appLogger.error("store", `Failed to delete remote connection ${id}`, err);
				throw err;
			}
		},

		/**
		 * Store the Basic Auth password for a connection, or forget it when given
		 * an empty string. The secret goes straight to the Rust credential vault;
		 * it is never held here and never read back.
		 */
		async setPassword(id: string, password: string): Promise<void> {
			await invoke("set_remote_connection_password", { id, password });
		},

		/** Whether a password is stored for this connection. */
		async hasPassword(id: string): Promise<boolean> {
			return (await invoke<boolean>("remote_connection_password_exists", { id })) ?? false;
		},

		/**
		 * The daemon's session token for a connection, or undefined when none was
		 * needed or the connection is not authenticated. Read by the transport to
		 * sign HTTP, WebSocket and SSE alike.
		 */
		getToken(connectionId: string): string | undefined {
			return remoteTokens.get(connectionId);
		},

		/**
		 * Returns the baseUrl for a connected connection, or undefined if not
		 * connected. The primary API used by transport routing.
		 *
		 * The backend already withholds the URL for anything but a connected
		 * connection; the status check here is the same answer stated locally, so
		 * a stale store cannot route either.
		 */
		getBaseUrl(connectionId: string): string | undefined {
			const connState = state.connections[connectionId];
			if (connState?.status === "connected" && connState.baseUrl) {
				return connState.baseUrl;
			}
			return undefined;
		},

		/** Reactive getter for all connections */
		getConnections(): Record<string, ConnectionState> {
			return state.connections;
		},

		/** Reactive getter for a single connection's state */
		getConnectionState(id: string): ConnectionState | undefined {
			return state.connections[id];
		},
	};

	return {
		state,
		...actions,
	};
}

export const remoteConnectionsStore = createRemoteConnectionsStore();
setRemoteBaseUrlLookup((connectionId) => remoteConnectionsStore.getBaseUrl(connectionId));
setRemoteTokenLookup((connectionId) => remoteConnectionsStore.getToken(connectionId));
