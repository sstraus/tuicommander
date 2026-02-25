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
      for (const { termId, sessionId } of sessions) {
        try {
          const result = await invoke<string | null>("get_session_foreground_process", {
            sessionId,
          });
          const agentType = result as AgentType | null;
          const current = terminalsStore.get(termId);
          if (current && current.agentType !== agentType) {
            appLogger.debug("app", `[AgentPoll] ${termId} agentType "${current.agentType}" → "${agentType}"`);
            terminalsStore.update(termId, { agentType });
          }
        } catch {
          // Session may have been closed; ignore errors silently
        }
      }
    };

    // Poll immediately, then every POLL_INTERVAL_MS
    pollAll();
    const timer = setInterval(pollAll, POLL_INTERVAL_MS);

    onCleanup(() => clearInterval(timer));
  });
}
