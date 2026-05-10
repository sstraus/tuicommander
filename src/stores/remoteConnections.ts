import { batch } from "solid-js";
import { createStore, produce } from "solid-js/store";
import { invoke } from "../invoke";
import { startRemoteEventBridge } from "../utils/remoteEventBridge";
import { appLogger } from "./appLogger";
import { tunnelsStore } from "./tunnels";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface RemoteConnection {
	id: string;
	name: string;
	transport: RemoteTransport;
	auth_username: string;
	enabled: boolean;
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

export type ConnectionStatus = "disconnected" | "connecting" | "connected" | "error";

export interface ConnectionState {
	connection: RemoteConnection;
	status: ConnectionStatus;
	baseUrl?: string;
	protocolVersion?: number;
	error?: string;
	tunnelProfileId?: string;
}

interface RemoteConnectionsState {
	connections: Record<string, ConnectionState>;
	hydrated: boolean;
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

/** Guard: prevent hydrate from running twice */
let hydrated = false;

/** Active health poll intervals keyed by connection ID */
const healthIntervals = new Map<string, ReturnType<typeof setInterval>>();

/** Active SSE event bridge cleanup functions keyed by connection ID */
const eventBridges = new Map<string, () => void>();

const HEALTH_POLL_MS = 5_000;
const TUNNEL_CONNECT_TIMEOUT_MS = 30_000;
const TUNNEL_POLL_MS = 500;

/** Pick a random local port in [10000, 60000) */
function randomLocalPort(): number {
	return 10_000 + Math.floor(Math.random() * 50_000);
}

function createRemoteConnectionsStore() {
	const [state, setState] = createStore<RemoteConnectionsState>({
		connections: {},
		hydrated: false,
	});

	// ---------------------------------------------------------------------------
	// Internal helpers
	// ---------------------------------------------------------------------------

	async function pollHealth(id: string): Promise<void> {
		const connState = state.connections[id];
		if (!connState?.baseUrl) return;
		const baseUrl = connState.baseUrl;
		try {
			const resp = await fetch(`${baseUrl}/health`);
			if (resp.ok) {
				const data = (await resp.json()) as { protocol_version?: number };
				setState("connections", id, {
					status: "connected",
					protocolVersion: data.protocol_version,
				});
			} else {
				setState("connections", id, {
					status: "error",
					error: `Health check failed: ${resp.status}`,
				});
			}
		} catch (e) {
			setState("connections", id, {
				status: "error",
				error: `Unreachable: ${e}`,
			});
		}
	}

	function startHealthPolling(id: string): void {
		stopHealthPolling(id);
		const interval = setInterval(() => void pollHealth(id), HEALTH_POLL_MS);
		healthIntervals.set(id, interval);
	}

	function stopHealthPolling(id: string): void {
		const existing = healthIntervals.get(id);
		if (existing !== undefined) {
			clearInterval(existing);
			healthIntervals.delete(id);
		}
	}

	/** Wait for an SSH tunnel to reach connected state (or error/stopped). */
	async function waitForTunnel(profileId: string): Promise<boolean> {
		const deadline = Date.now() + TUNNEL_CONNECT_TIMEOUT_MS;
		while (Date.now() < deadline) {
			const status = tunnelsStore.getTunnelStatus(profileId);
			if (status?.type === "connected") return true;
			if (status?.type === "stopped" || status?.type === "error") {
				appLogger.warn("store", `Tunnel ${profileId} failed: ${status.type}`);
				return false;
			}
			await new Promise<void>((resolve) => setTimeout(resolve, TUNNEL_POLL_MS));
		}
		appLogger.warn("store", `Tunnel ${profileId} connect timeout`);
		return false;
	}

	// ---------------------------------------------------------------------------
	// Actions
	// ---------------------------------------------------------------------------

	const actions = {
		/** Load connections from backend, set hydrated */
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
			} catch (err) {
				appLogger.error("store", "Failed to hydrate remote connections", err);
			}
		},

		/**
		 * Connect to a remote connection.
		 * - SSH: creates a tunnel profile, starts it, sets baseUrl to the local port.
		 * - Direct: sets baseUrl to the configured URL directly.
		 * In both cases, health polling begins once the baseUrl is set.
		 */
		async connect(id: string): Promise<void> {
			const connState = state.connections[id];
			if (!connState) {
				appLogger.warn("store", `connect: unknown connection ${id}`);
				return;
			}
			if (connState.status === "connecting" || connState.status === "connected") return;

			setState("connections", id, { status: "connecting", error: undefined });
			appLogger.info("store", `Connecting remote connection ${id} (${connState.connection.name})`);

			const { transport } = connState.connection;

			try {
				if (transport.type === "Ssh") {
					const localPort = randomLocalPort();
					const profileName = `__remote_${id}`;

					// Create (or re-use) a tunnel profile for this connection
					await tunnelsStore.createProfile({
						name: profileName,
						host: transport.ssh_host,
						port: transport.ssh_port,
						user: transport.ssh_user,
						identity_file: transport.identity_file,
						forwards: [
							{
								type: "Local",
								bind_port: localPort,
								remote_host: "127.0.0.1",
								remote_port: transport.remote_daemon_port,
							},
						],
						options: {
							server_alive_interval: 15,
							server_alive_count_max: 3,
							strict_host_key_checking: "AcceptNew",
						},
						auto_connect: false,
					});

					// Find the profile ID we just created (by name)
					await tunnelsStore.refreshProfiles();
					const profiles = tunnelsStore.getProfiles();
					const profile = profiles.find((p) => p.name === profileName);
					if (!profile) {
						throw new Error(`Could not find tunnel profile "${profileName}" after creation`);
					}

					// Start the tunnel and wait for it to connect
					await tunnelsStore.startTunnel(profile.id);
					const connected = await waitForTunnel(profile.id);
					if (!connected) {
						setState("connections", id, {
							status: "error",
							error: "SSH tunnel failed to connect",
						});
						return;
					}

					const baseUrl = `http://127.0.0.1:${localPort}`;
					setState("connections", id, {
						baseUrl,
						tunnelProfileId: profile.id,
					});

					// Initial health check sets status to "connected" or "error"
					await pollHealth(id);
					startHealthPolling(id);
					eventBridges.get(id)?.();
					eventBridges.set(id, startRemoteEventBridge(id, baseUrl));
				} else {
					// Direct transport — baseUrl is already known
					setState("connections", id, { baseUrl: transport.url });
					await pollHealth(id);
					startHealthPolling(id);
					eventBridges.get(id)?.();
					eventBridges.set(id, startRemoteEventBridge(id, transport.url));
				}
			} catch (err) {
				appLogger.error("store", `Failed to connect remote connection ${id}`, err);
				setState("connections", id, {
					status: "error",
					error: String(err),
				});
			}
		},

		/** Disconnect from a remote connection. Stops the SSH tunnel if applicable. */
		async disconnect(id: string): Promise<void> {
			const connState = state.connections[id];
			if (!connState) return;

			stopHealthPolling(id);
			const bridgeCleanup = eventBridges.get(id);
			if (bridgeCleanup) {
				bridgeCleanup();
				eventBridges.delete(id);
			}

			const { tunnelProfileId } = connState;
			if (tunnelProfileId) {
				try {
					await tunnelsStore.stopTunnel(tunnelProfileId);
					// Clean up the auto-created profile
					await tunnelsStore.deleteProfile(tunnelProfileId);
				} catch (err) {
					appLogger.warn("store", `Failed to stop/delete tunnel for connection ${id}`, err);
				}
			}

			setState("connections", id, {
				status: "disconnected",
				baseUrl: undefined,
				protocolVersion: undefined,
				error: undefined,
				tunnelProfileId: undefined,
			});
			appLogger.info("store", `Disconnected remote connection ${id}`);
		},

		/** Save a new connection to the backend and add it to state */
		async addConnection(conn: RemoteConnection): Promise<void> {
			try {
				await invoke("save_remote_connection", { connection: conn });
				setState("connections", conn.id, { connection: conn, status: "disconnected" });
			} catch (err) {
				appLogger.error("store", "Failed to save remote connection", err);
				throw err;
			}
		},

		/** Disconnect (if connected), delete from backend, and remove from state */
		async removeConnection(id: string): Promise<void> {
			const connState = state.connections[id];
			if (!connState) return;

			if (connState.status === "connecting" || connState.status === "connected") {
				await actions.disconnect(id);
			}

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
		 * Returns the baseUrl for a connected connection, or undefined if not connected.
		 * This is the primary API used by transport routing (Step 17).
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
