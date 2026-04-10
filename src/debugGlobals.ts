/**
 * Debug globals for MCP eval_js introspection.
 *
 * Exposes key stores and utilities on window.__TUIC__ so that
 * debug(action="invoke_js") can inspect runtime state without
 * needing dynamic module imports (which fail in release builds).
 *
 * Stores self-register snapshots via debugRegistry — this file wires
 * the registry into window.__TUIC__ and keeps the legacy convenience
 * functions for backward compatibility.
 *
 * Initialised once from index.tsx — always available, dev or release.
 */

import { pluginStore } from "./stores/pluginStore";
import { terminalsStore } from "./stores/terminals";
import { activityStore } from "./stores/activityStore";
import { appLogger } from "./stores/appLogger";
import { getDebugSnapshot, listDebugSnapshots } from "./stores/debugRegistry";

export function initDebugGlobals(): void {
  (window as any).__TUIC__ = {
    // ---- Dynamic registry ----

    /** List all registered store snapshot names */
    stores: () => listDebugSnapshots(),

    /** Get a store snapshot by name (returns null if not registered) */
    store: (name: string) => getDebugSnapshot(name),

    // ---- Legacy convenience functions (backward-compatible) ----

    /** All plugin states: id, loaded, enabled, error */
    plugins() {
      return pluginStore.getAll().map((p) => ({
        id: p.id,
        loaded: p.loaded,
        enabled: p.enabled,
        error: p.error,
        builtIn: p.builtIn,
      }));
    },

    /** Single plugin state by ID */
    plugin(id: string) {
      const p = pluginStore.getPlugin(id);
      if (!p) return null;
      return {
        id: p.id,
        loaded: p.loaded,
        enabled: p.enabled,
        error: p.error,
        builtIn: p.builtIn,
        manifest: p.manifest,
      };
    },

    /** Plugin log entries (from PluginLogger ring buffer) */
    pluginLogs(id: string, limit = 20) {
      const p = pluginStore.getPlugin(id);
      if (!p) return null;
      return p.logger.getEntries().slice(-limit);
    },

    /** All terminal states with session/agent info */
    terminals() {
      return terminalsStore.getIds().map((id) => {
        const t = terminalsStore.get(id);
        if (!t) return { id };
        return {
          id: t.id,
          name: t.name,
          sessionId: t.sessionId,
          shellState: t.shellState,
          agentType: t.agentType,
          agentSessionId: t.agentSessionId,
          cwd: t.cwd,
        };
      });
    },

    /** Single terminal state by terminal ID */
    terminal(id: string) {
      const t = terminalsStore.get(id);
      if (!t) return null;
      return {
        id: t.id,
        name: t.name,
        sessionId: t.sessionId,
        shellState: t.shellState,
        agentType: t.agentType,
        agentSessionId: t.agentSessionId,
        cwd: t.cwd,
        awaitingInput: t.awaitingInput,
        usageLimit: t.usageLimit,
      };
    },

    /** Lookup agentType for a PTY session ID */
    agentTypeForSession(sessionId: string) {
      return terminalsStore.getAgentTypeForSession(sessionId);
    },

    /** Activity center sections and items */
    activity() {
      return {
        sections: activityStore.getSections(),
        items: activityStore.getActive(),
      };
    },

    /** App log entries (JS ring buffer, all levels) */
    logs(limit = 50) {
      return appLogger.getEntries().slice(-limit).map((e) => ({
        id: e.id,
        level: e.level,
        source: e.source,
        message: e.message.substring(0, 200),
        repeat: e.repeatCount,
      }));
    },
  };
}
