import { createEffect, onCleanup } from "solid-js";
import { invoke } from "../invoke";
import { terminalsStore } from "../stores/terminals";
import { appLogger } from "../stores/appLogger";
import type { AgentType } from "../agents";

/** Polling interval for foreground process detection (ms) */
const POLL_INTERVAL_MS = 3000;

/**
 * Polls ALL terminals with active PTY sessions to detect which agent (if any)
 * is running as the foreground process. Updates the terminals store with
 * the detected agentType for each terminal.
 */
export function useAgentPolling(): void {
  createEffect(() => {
    // Read all terminal IDs reactively so the effect re-runs when terminals are added/removed
    const allIds = terminalsStore.getIds();
    // Build a snapshot of id→sessionId for terminals that have active sessions
    const sessions: Array<{ termId: string; sessionId: string }> = [];
    for (const id of allIds) {
      const sess = terminalsStore.state.terminals[id]?.sessionId;
      if (sess) sessions.push({ termId: id, sessionId: sess });
    }

    if (sessions.length === 0) return;

    const pollAll = async () => {
      const results = await Promise.allSettled(
        sessions.map(async ({ termId, sessionId }) => {
          const result = await invoke<string | null>("get_session_foreground_process", {
            sessionId,
          });
          return { termId, agentType: result as AgentType | null };
        }),
      );
      for (const r of results) {
        if (r.status === "rejected") {
          appLogger.debug("app", "[AgentPoll] session poll rejected", r.reason);
          continue;
        }
        if (r.status !== "fulfilled") continue;
        const { termId, agentType } = r.value;
        const current = terminalsStore.get(termId);
        if (current && current.agentType !== agentType) {
          appLogger.debug("app", `[AgentPoll] ${termId} agentType "${current.agentType}" → "${agentType}"`);
          terminalsStore.update(termId, { agentType });
        }
      }
    };

    // Poll immediately, then every POLL_INTERVAL_MS
    pollAll().catch((err) => appLogger.debug("app", "[AgentPoll] initial poll failed", err));
    const timer = setInterval(() => {
      pollAll().catch((err) => appLogger.debug("app", "[AgentPoll] poll failed", err));
    }, POLL_INTERVAL_MS);

    onCleanup(() => clearInterval(timer));
  });
}
