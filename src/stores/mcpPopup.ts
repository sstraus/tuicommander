import { createStore } from "solid-js/store";
import { invoke, listen } from "../invoke";
import { appLogger } from "./appLogger";
import { repoSettingsStore } from "./repoSettings";
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
  /** Per-project MCP upstream allowlist (null = no restriction / no active repo) */
  projectAllowlist: string[] | null;
}

function createMcpPopupStore() {
  const [state, setState] = createStore<McpPopupState>({
    isOpen: false,
    servers: [],
    status: [],
    saving: false,
    projectAllowlist: null,
  });

  /** Resolve the project allowlist from repoSettingsStore */
  function resolveProjectAllowlist(): string[] | null {
    const repoPath = repoSettingsStore.state.activeRepoPath;
    if (!repoPath) return null;
    const effective = repoSettingsStore.getEffective(repoPath);
    return effective?.mcpUpstreams ?? null;
  }

  /** Load full upstream config + live status snapshot + project allowlist */
  async function loadConfig(): Promise<void> {
    try {
      const [cfg, snap] = await Promise.all([
        invoke<UpstreamMcpConfig>("load_mcp_upstreams"),
        invoke<{ upstreams: UpstreamStatusEntry[] }>("get_mcp_upstream_status"),
      ]);
      setState("servers", cfg.servers ?? []);
      setState("status", snap?.upstreams ?? []);
      setState("projectAllowlist", resolveProjectAllowlist());
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

    /**
     * Effective enabled state for a server in the context of the active repo.
     * A server is effective-enabled when: globally enabled AND (no project allowlist OR in allowlist).
     */
    effectiveEnabledForRepo(name: string): boolean {
      const server = state.servers.find((s) => s.name === name);
      if (!server?.enabled) return false;

      const allowlist = resolveProjectAllowlist();
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
      } catch (err) {
        appLogger.error("mcp", `toggleServerForProject failed for ${name}`, err);
      }
    },

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
