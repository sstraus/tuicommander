import { createEffect, onCleanup } from "solid-js";
import { invoke } from "../invoke";
import { terminalsStore } from "../stores/terminals";
import type { AgentType } from "../agents";

/** Polling interval for foreground process detection (ms) */
const POLL_INTERVAL_MS = 3000;

/**
 * Polls the active terminal's PTY session to detect which agent (if any)
 * is running as the foreground process. Updates the terminals store with
 * the detected agentType.
 */
export function useAgentPolling(): void {
  createEffect(() => {
    const activeId = terminalsStore.state.activeId;
    const sessionId = activeId ? terminalsStore.state.terminals[activeId]?.sessionId : null;

    if (!activeId || !sessionId) return;

    // Capture for use in interval callback (avoids stale closure over reactive values)
    const termId = activeId;
    const sessId = sessionId;

    const poll = async () => {
      try {
        const result = await invoke<string | null>("get_session_foreground_process", {
          sessionId: sessId,
        });
        const agentType = result as AgentType | null;
        const current = terminalsStore.get(termId);
        // Only update store if the value actually changed
        if (current && current.agentType !== agentType) {
          terminalsStore.update(termId, { agentType });
        }
      } catch {
        // Session may have been closed; ignore errors silently
      }
    };

    // Poll immediately on activation, then every POLL_INTERVAL_MS
    poll();
    const timer = setInterval(poll, POLL_INTERVAL_MS);

    onCleanup(() => clearInterval(timer));
  });
}
