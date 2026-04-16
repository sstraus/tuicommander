import { createStore } from "solid-js/store";
import { invoke, listen } from "../invoke";
import { appLogger } from "./appLogger";
import type { UpstreamMcpServer, UpstreamMcpConfig } from "../transport";

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
  /** True while a toggle save is in flight */
  saving: boolean;
}

function createMcpPopupStore() {
  const [state, setState] = createStore<McpPopupState>({
    isOpen: false,
    servers: [],
    status: [],
    saving: false,
  });

  /** Load full upstream config + live status snapshot */
  async function loadConfig(): Promise<void> {
    try {
      const [cfg, snap] = await Promise.all([
        invoke<UpstreamMcpConfig>("load_mcp_upstreams"),
        invoke<{ upstreams: UpstreamStatusEntry[] }>("get_mcp_upstream_status"),
      ]);
      setState("servers", cfg.servers ?? []);
      setState("status", snap?.upstreams ?? []);
    } catch (err) {
      appLogger.debug("mcp", "McpPopup loadConfig failed", err);
    }
  }

  /** Refresh only the live status snapshot */
  async function refreshStatus(): Promise<void> {
    try {
      const snap = await invoke<{ upstreams: UpstreamStatusEntry[] }>("get_mcp_upstream_status");
      setState("status", snap?.upstreams ?? []);
    } catch {
      // Transient failure — ignore
    }
  }

  /** Toggle a server's enabled state and persist via save_mcp_upstreams */
  async function toggleServer(name: string): Promise<void> {
    if (state.saving) return;

    const idx = state.servers.findIndex((s) => s.name === name);
    if (idx === -1) return;

    const updated = state.servers.map((s) =>
      s.name === name ? { ...s, enabled: !s.enabled } : s,
    );

    // Optimistic UI update
    setState("servers", updated);
    setState("saving", true);

    try {
      await invoke("save_mcp_upstreams", { config: { servers: updated } });
      // Config saved — status will update via event
    } catch (err) {
      appLogger.error("mcp", `Toggle failed for ${name}`, err);
      // Rollback optimistic update
      setState("servers", state.servers.map((s) =>
        s.name === name ? { ...s, enabled: !s.enabled } : s,
      ));
    } finally {
      setState("saving", false);
    }
  }

  return {
    state,

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

    /** Subscribe to upstream-status-changed events. Returns cleanup fn. */
    listenForStatusChanges(): Promise<() => void> {
      return listen<{ name: string; status: string }>(
        "upstream-status-changed",
        () => {
          // Event carries only {name, status} — trigger full refresh
          refreshStatus();
        },
      );
    },
  };
}

export const mcpPopupStore = createMcpPopupStore();
