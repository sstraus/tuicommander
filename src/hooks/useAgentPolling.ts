import { createEffect, onCleanup } from "solid-js";
import { invoke } from "../invoke";
import { terminalsStore } from "../stores/terminals";
import { appLogger } from "../stores/appLogger";
import { type AgentType, AGENT_TYPES, AGENTS } from "../agents";

/** Polling interval for foreground process detection (ms) */
const POLL_INTERVAL_MS = 3000;

/** Validate a string from the backend is a known AgentType */
function toAgentType(value: string | null): AgentType | null {
  if (value === null) return null;
  return (AGENT_TYPES as readonly string[]).includes(value) ? (value as AgentType) : null;
}

/**
 * Polls ALL terminals with active PTY sessions to detect which agent (if any)
 * is running as the foreground process. Updates the terminals store with
 * the detected agentType for each terminal.
 *
 * On null→agent transition: attempts to discover the agent session ID from
 * disk (via discover_agent_session Rust command) if the agent supports it.
 * On agent→null transition: clears agentSessionId so re-discovery can occur
 * on the next launch.
 */
export function useAgentPolling(): void {
  // Track which terminals we've already attempted discovery for (cleared on agent exit)
  const discoveryAttempted = new Set<string>();

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
          return { termId, agentType: toAgentType(result) };
        }),
      );

      // Collect UUIDs already claimed by terminals that have a session ID
      const claimedIds: string[] = [];
      for (const id of allIds) {
        const sid = terminalsStore.get(id)?.agentSessionId;
        if (sid) claimedIds.push(sid);
      }

      for (const r of results) {
        if (r.status === "rejected") {
          appLogger.debug("app", "[AgentPoll] session poll rejected", r.reason);
          continue;
        }
        const { termId, agentType } = r.value;
        const current = terminalsStore.get(termId);
        if (!current) continue;

        const prevAgentType = current.agentType;

        if (prevAgentType !== agentType) {
          appLogger.debug("app", `[AgentPoll] ${termId} agentType "${prevAgentType}" → "${agentType}"`);
          terminalsStore.update(termId, { agentType });

          // agent→null: clear session ID so re-discovery fires on next launch
          if (prevAgentType !== null && agentType === null) {
            terminalsStore.update(termId, { agentSessionId: null });
            discoveryAttempted.delete(termId);
          }
        }

        // null→agent: attempt session discovery if supported and not yet tried
        if (agentType !== null && current.agentSessionId === null && !discoveryAttempted.has(termId)) {
          const disc = AGENTS[agentType].sessionDiscovery;
          if (disc) {
            discoveryAttempted.add(termId);
            // Use cwd from terminal (path to project directory for discovery)
            const cwd = current.cwd ?? null;
            try {
              const found = await invoke<string | null>("discover_agent_session", {
                agentType,
                cwd,
                claimedIds,
              });
              if (found) {
                appLogger.debug("app", `[AgentPoll] ${termId} discovered agentSessionId "${found}"`);
                terminalsStore.update(termId, { agentSessionId: found });
                claimedIds.push(found);
              }
            } catch (err) {
              appLogger.debug("app", `[AgentPoll] ${termId} discover_agent_session failed`, err);
            }
          }
        }
      }
    };

    // Don't poll immediately on effect re-run — the 3s interval is acceptable latency
    // and avoids burst polling when many terminals are added during session restore.
    const timer = setInterval(() => {
      pollAll().catch((err) => appLogger.debug("app", "[AgentPoll] poll failed", err));
    }, POLL_INTERVAL_MS);

    onCleanup(() => clearInterval(timer));
  });
}
