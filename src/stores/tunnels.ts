import { batch } from "solid-js";
import { createStore, produce } from "solid-js/store";
import { invoke } from "../invoke";
import { appLogger } from "./appLogger";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface TunnelProfile {
	id: string;
	name: string;
	host: string;
	port: number;
	user: string;
	identity_file: string | null;
	forwards: ForwardSpec[];
	options: ProfileOptions;
	auto_connect: boolean;
}

export interface ForwardSpec {
	type: "Local" | "Remote";
	bind_port: number;
	remote_host?: string;
	remote_port?: number;
	local_host?: string;
	local_port?: number;
}

export interface ProfileOptions {
	server_alive_interval: number;
	server_alive_count_max: number;
	strict_host_key_checking: "Yes" | "AcceptNew";
}

export type TunnelStatus =
	| { type: "starting" }
	| { type: "connected" }
	| { type: "reconnecting"; attempt: number; reason: string }
	| { type: "stopped"; reason: string }
	| { type: "error"; message: string };

export interface ActiveTunnel {
	id: string;
	status: TunnelStatus;
	started_at: string;
}

interface TunnelsState {
	profiles: TunnelProfile[];
	activeTunnels: Record<string, ActiveTunnel>;
	hydrated: boolean;
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

/** Guard: prevent hydrate from running twice */
let hydrated = false;

/** Interval handle for status polling during tunnel start */
const POLL_INTERVAL_MS = 2000;
/** Terminal states — polling stops when reached */
const TERMINAL_STATES = new Set(["connected", "stopped", "error"]);

function createTunnelsStore() {
	const [state, setState] = createStore<TunnelsState>({
		profiles: [],
		activeTunnels: {},
		hydrated: false,
	});

	const actions = {
		/** Load profiles and active tunnels from backend */
		async hydrate(): Promise<void> {
			if (hydrated) return;
			try {
				const [profiles, activeTunnels] = await Promise.all([
					invoke<TunnelProfile[]>("list_tunnel_profiles"),
					invoke<ActiveTunnel[]>("list_active_tunnels"),
				]);

				const activeTunnelsMap: Record<string, ActiveTunnel> = {};
				for (const tunnel of activeTunnels ?? []) {
					activeTunnelsMap[tunnel.id] = tunnel;
				}

				batch(() => {
					setState("profiles", profiles ?? []);
					setState("activeTunnels", activeTunnelsMap);
					setState("hydrated", true);
				});

				hydrated = true;

				// Auto-connect profiles that aren't already active
				for (const profile of profiles ?? []) {
					if (profile.auto_connect && !activeTunnelsMap[profile.id]) {
						actions.startTunnel(profile.id).catch((err) => {
							appLogger.error("store", `Auto-connect failed for ${profile.name}`, err);
						});
					}
				}
			} catch (err) {
				appLogger.error("store", "Failed to hydrate tunnels", err);
				// hydrated stays false — safe to retry
			}
		},

		/** Reload profiles from backend */
		async refreshProfiles(): Promise<void> {
			try {
				const profiles = await invoke<TunnelProfile[]>("list_tunnel_profiles");
				setState("profiles", profiles ?? []);
			} catch (err) {
				appLogger.error("store", "Failed to refresh tunnel profiles", err);
			}
		},

		/** Reload active tunnel statuses from backend */
		async refreshActiveTunnels(): Promise<void> {
			try {
				const activeTunnels = await invoke<ActiveTunnel[]>("list_active_tunnels");
				const activeTunnelsMap: Record<string, ActiveTunnel> = {};
				for (const tunnel of activeTunnels ?? []) {
					activeTunnelsMap[tunnel.id] = tunnel;
				}
				setState("activeTunnels", activeTunnelsMap);
			} catch (err) {
				appLogger.error("store", "Failed to refresh active tunnels", err);
			}
		},

		/** Save a new profile to backend, then refresh */
		async createProfile(profile: Omit<TunnelProfile, "id">): Promise<void> {
			try {
				await invoke("save_tunnel_profile", { profile });
				await actions.refreshProfiles();
			} catch (err) {
				appLogger.error("store", "Failed to create tunnel profile", err);
				throw err;
			}
		},

		/** Update an existing profile in backend, then refresh */
		async updateProfile(profile: TunnelProfile): Promise<void> {
			try {
				await invoke("save_tunnel_profile", { profile });
				await actions.refreshProfiles();
			} catch (err) {
				appLogger.error("store", "Failed to update tunnel profile", err);
				throw err;
			}
		},

		/** Delete a profile from backend, then refresh */
		async deleteProfile(id: string): Promise<void> {
			try {
				await invoke("delete_tunnel_profile", { id });
				await actions.refreshProfiles();
			} catch (err) {
				appLogger.error("store", "Failed to delete tunnel profile", err);
				throw err;
			}
		},

		/** Start a tunnel, then poll status until terminal state */
		async startTunnel(id: string): Promise<void> {
			try {
				await invoke("start_tunnel", { id });

				// Optimistically mark as starting
				setState(
					produce((s) => {
						s.activeTunnels[id] = {
							id,
							status: { type: "starting" },
							started_at: new Date().toISOString(),
						};
					}),
				);

				// Poll until terminal state
				await new Promise<void>((resolve) => {
					const interval = setInterval(async () => {
						try {
							const tunnel = await invoke<ActiveTunnel>("get_tunnel_status", { id });
							if (tunnel) {
								setState("activeTunnels", id, tunnel);
								if (TERMINAL_STATES.has(tunnel.status.type)) {
									clearInterval(interval);
									resolve();
								}
							} else {
								// Tunnel gone — stopped externally
								setState(
									produce((s) => {
										delete s.activeTunnels[id];
									}),
								);
								clearInterval(interval);
								resolve();
							}
						} catch (err) {
							appLogger.error("store", `Failed to poll tunnel status for ${id}`, err);
							clearInterval(interval);
							setState(
								produce((s) => {
									s.activeTunnels[id] = {
										id,
										status: { type: "error", message: String(err) },
										started_at: s.activeTunnels[id]?.started_at ?? new Date().toISOString(),
									};
								}),
							);
							resolve();
						}
					}, POLL_INTERVAL_MS);
				});
			} catch (err) {
				appLogger.error("store", `Failed to start tunnel ${id}`, err);
				throw err;
			}
		},

		/** Stop a tunnel and remove from active tunnels */
		async stopTunnel(id: string): Promise<void> {
			try {
				await invoke("stop_tunnel", { id });
				setState(
					produce((s) => {
						delete s.activeTunnels[id];
					}),
				);
			} catch (err) {
				appLogger.error("store", `Failed to stop tunnel ${id}`, err);
				throw err;
			}
		},

		/** Return all profiles (reactive) */
		getProfiles(): TunnelProfile[] {
			return state.profiles;
		},

		/** Return all active tunnels map (reactive) */
		getActiveTunnels(): Record<string, ActiveTunnel> {
			return state.activeTunnels;
		},

		/** Return status for a single tunnel (reactive) */
		getTunnelStatus(id: string): TunnelStatus | undefined {
			return state.activeTunnels[id]?.status;
		},
	};

	return {
		state,
		...actions,
	};
}

export const tunnelsStore = createTunnelsStore();
