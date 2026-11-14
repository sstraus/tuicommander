import { createStore, produce } from "solid-js/store";
import { invoke, listen } from "../invoke";
import { reportSettingsConflicts, settingsRefresh } from "../services/settingsRefresh";
import { rpc, type UpstreamMcpConfig, type UpstreamMcpServer } from "../transport";
import { getRemoteBaseUrl } from "../transportRuntime";
import { mergeSettingsSnapshot } from "../utils/mergeSettingsSnapshot";
import { reconcileById } from "../utils/reconcileById";
import { type RemoteEventPayload, remoteEventOrigin } from "../utils/remoteEventOrigin";
import { appLogger } from "./appLogger";
import { repoSettingsStore } from "./repoSettings";
import { toastsStore } from "./toasts";

/** Mirrors the status snapshot returned by get_mcp_upstream_status */
export interface UpstreamStatusEntry {
	name: string;
	status: "connecting" | "ready" | "circuit_open" | "disabled" | "failed" | "authenticating" | "needs_auth";
	transport: { type: string };
	tool_count: number;
}

interface McpPopupState {
	isOpen: boolean;
	/** Full upstream config (with id, enabled, auth) — loaded on open */
	servers: UpstreamMcpServer[];
	/** Live status snapshot — refreshed via events + fallback poll */
	status: UpstreamStatusEntry[];
	/** Daemon health is isolated from this machine's editable configuration. */
	remoteStatus: Record<string, { name: string; upstreams: UpstreamStatusEntry[] }>;
	/** True while a toggle save is in flight */
	saving: boolean;
	/** Per-project MCP upstream allowlist (null = no restriction / no active repo) */
	projectAllowlist: string[] | null;
}

function createMcpPopupStore() {
	const [state, setState] = createStore<McpPopupState>({
		isOpen: false,
		servers: [],
		status: [],
		remoteStatus: {},
		saving: false,
		projectAllowlist: null,
	});
	const remoteRefreshes = new Map<string, number>();

	// App-lifetime listener: remote failures must be visible with the popup closed.
	listen<RemoteEventPayload & { name: string; status: string }>("upstream-status-changed", (event) => {
		const payload = event.payload;
		const origin = remoteEventOrigin(payload);
		if (!origin || !getRemoteBaseUrl(origin.connection)) return;
		if (typeof payload.name !== "string" || typeof payload.status !== "string") return;
		toastsStore.add(`[${origin.name}] MCP upstream`, `${payload.name}: ${payload.status}`, "info");
		const revision = (remoteRefreshes.get(origin.connection) ?? 0) + 1;
		remoteRefreshes.set(origin.connection, revision);
		rpc<{ upstreams: UpstreamStatusEntry[] }>("get_mcp_upstream_status", undefined, origin.connection)
			.then((snapshot) => {
				if (!getRemoteBaseUrl(origin.connection) || remoteRefreshes.get(origin.connection) !== revision) return;
				setState("remoteStatus", origin.connection, { name: origin.name, upstreams: snapshot.upstreams });
			})
			.catch((error) => appLogger.debug("mcp", "Remote upstream status refresh failed", error));
	}).catch((error) => appLogger.debug("mcp", "Remote upstream status listener failed", error));
	listen<RemoteEventPayload & { id: string; status: string }>("remote-connection-status", (event) => {
		const payload = event.payload;
		if (payload.__tuic_origin !== undefined || payload.status === "connected") return;
		remoteRefreshes.set(payload.id, (remoteRefreshes.get(payload.id) ?? 0) + 1);
		setState(
			"remoteStatus",
			produce((snapshots) => {
				delete snapshots[payload.id];
			}),
		);
	}).catch((error) => appLogger.debug("mcp", "Remote upstream disconnect listener failed", error));

	/** Resolve the project allowlist from repoSettingsStore */
	function resolveProjectAllowlist(): string[] | null {
		const repoPath = repoSettingsStore.state.activeRepoPath;
		if (!repoPath) return null;
		const effective = repoSettingsStore.getEffective(repoPath);
		return effective?.mcpUpstreams ?? null;
	}

	let acknowledgedServers: UpstreamMcpServer[] = [];

	/** Load full upstream config + live status snapshot + project allowlist */
	async function loadConfig(): Promise<void> {
		try {
			const [cfg, snap] = await Promise.all([
				invoke<UpstreamMcpConfig>("load_mcp_upstreams"),
				invoke<{ upstreams: UpstreamStatusEntry[] }>("get_mcp_upstream_status"),
			]);
			acknowledgedServers = cfg.servers ?? [];
			setState("servers", cfg.servers ?? []);
			setState(
				"status",
				reconcileById(snap?.upstreams ?? [], state.status, (u) => u.name),
			);
			setState("projectAllowlist", resolveProjectAllowlist());
		} catch (err) {
			appLogger.debug("mcp", "McpPopup loadConfig failed", err);
		}
	}

	/** Refresh only the live status snapshot */
	async function refreshStatus(): Promise<void> {
		try {
			const snap = await invoke<{ upstreams: UpstreamStatusEntry[] }>("get_mcp_upstream_status");
			setState(
				"status",
				reconcileById(snap?.upstreams ?? [], state.status, (u) => u.name),
			);
		} catch {
			// Transient failure — ignore
		}
	}

	/** Toggle a server's enabled state and persist via save_mcp_upstreams */
	async function toggleServer(name: string): Promise<void> {
		if (state.saving) return;

		const idx = state.servers.findIndex((s) => s.name === name);
		if (idx === -1) return;

		const base = { servers: [...state.servers] };
		const updated = state.servers.map((s) => (s.name === name ? { ...s, enabled: !s.enabled } : s));

		// Optimistic UI update
		setState("servers", updated);
		setState("saving", true);

		try {
			await invoke("save_mcp_upstreams", { base, config: { servers: updated } });
			acknowledgedServers = updated;
			// TUIC advertises tools.listChanged and the bridge forwards the standard
			// notification. MCP does not expose whether a particular client version
			// actually applies it, so report what TUIC knows without falsely requiring
			// every Claude/Codex session to restart.
			toastsStore.add(
				"MCP servers changed",
				"Connected AI sessions were notified. Compatible clients refresh automatically; others may need to reconnect.",
				"info",
			);
		} catch (err) {
			appLogger.error("mcp", `Toggle failed for ${name}`, err);
			// Rollback optimistic update
			setState(
				"servers",
				state.servers.map((s) => (s.name === name ? { ...s, enabled: !s.enabled } : s)),
			);
		} finally {
			setState("saving", false);
		}
	}

	return {
		state,
		startRemoteSync(): () => void {
			return settingsRefresh.register({
				section: "upstreams",
				read: () => invoke<UpstreamMcpConfig>("load_mcp_upstreams"),
				apply: (config) => {
					const incoming = config.servers ?? [];
					const merged = mergeSettingsSnapshot(acknowledgedServers, state.servers, incoming);
					acknowledgedServers = incoming;
					setState("servers", merged.value);
					reportSettingsConflicts(merged.conflicts);
				},
			});
		},

		open(): void {
			setState("isOpen", true);
			loadConfig();
		},

		close(): void {
			setState("isOpen", false);
		},

		toggle(): void {
			const opening = !state.isOpen;
			setState("isOpen", opening);
			if (opening) loadConfig();
		},

		loadConfig,
		refreshStatus,
		toggleServer,

		/**
		 * Effective enabled state for a server in the context of the active repo.
		 * A server is effective-enabled when: globally enabled AND (no project allowlist OR in allowlist).
		 *
		 * Reads from `state.projectAllowlist` (the popup's reactive snapshot) instead
		 * of repoSettingsStore so the checkbox updates immediately after toggleServerForProject —
		 * repoSettingsStore.mcpUpstreams isn't refreshed by set_project_mcp_upstreams (#1367-7fb0/H2).
		 */
		effectiveEnabledForRepo(name: string): boolean {
			const server = state.servers.find((s) => s.name === name);
			if (!server?.enabled) return false;

			const allowlist = state.projectAllowlist;
			if (allowlist === null) return true;
			return allowlist.includes(name);
		},

		/**
		 * Toggle a server's per-project enabled state via the mcp_upstreams allowlist.
		 * Does nothing if no active repo.
		 */
		async toggleServerForProject(name: string): Promise<void> {
			const repoPath = repoSettingsStore.state.activeRepoPath;
			if (!repoPath) return;

			const currentAllowlist = resolveProjectAllowlist();
			const allNames = state.servers.map((s) => s.name);
			let newAllowlist: string[] | null;

			if (currentAllowlist === null) {
				// No restriction → create allowlist excluding this server
				newAllowlist = allNames.filter((n) => n !== name);
			} else if (currentAllowlist.includes(name)) {
				// Remove from allowlist
				newAllowlist = currentAllowlist.filter((n) => n !== name);
			} else {
				// Add to allowlist
				newAllowlist = [...currentAllowlist, name];
			}

			// If new allowlist contains all servers, clear restriction (set null)
			if (newAllowlist !== null && allNames.length > 0 && allNames.every((n) => newAllowlist!.includes(n))) {
				newAllowlist = null;
			}

			try {
				await invoke("set_project_mcp_upstreams", {
					repoPath,
					upstreamNames: newAllowlist,
				});
				setState("projectAllowlist", newAllowlist);
				// Keep repoSettingsStore in sync so other consumers (settings panel,
				// getEffective fallback path) observe the same value without a reload.
				if (repoSettingsStore.state.settings[repoPath]) {
					repoSettingsStore.update(repoPath, { mcpUpstreams: newAllowlist });
				}
			} catch (err) {
				appLogger.error("mcp", `toggleServerForProject failed for ${name}`, err);
			}
		},

		/** Subscribe to upstream-status-changed events. Returns cleanup fn. */
		listenForStatusChanges(): Promise<() => void> {
			return listen<RemoteEventPayload & { name: string; status: string }>("upstream-status-changed", (event) => {
				if (event.payload.__tuic_origin !== undefined) return;
				// Event carries only {name, status} — trigger full refresh
				refreshStatus();
			});
		},
	};
}

export const mcpPopupStore = createMcpPopupStore();
