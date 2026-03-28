import { createEffect, onCleanup } from "solid-js";
import { invoke } from "../invoke";
import { terminalsStore } from "../stores/terminals";
import { appLogger } from "../stores/appLogger";
import { type AgentType, AGENT_TYPES, AGENTS } from "../agents";

/** Polling interval for foreground process detection (ms) */
const POLL_INTERVAL_MS = 3000;

/**
 * Number of consecutive null polls required before clearing a detected agent.
 * Prevents flickering when the agent spawns short-lived subprocesses (git, node, etc.)
 * that briefly become the foreground process group.
 */
const NULL_THRESHOLD = 3;

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
  // Count consecutive null polls per terminal (for debounce)
  const nullStreak = new Map<string, number>();

  createEffect(() => {
    // Read terminal IDs reactively so the effect re-runs when terminals are added/removed
    const allIds = terminalsStore.getIds();

    // Nothing to poll if no terminals exist at all
    if (allIds.length === 0) return;

    const pollAll = async () => {
      // Read current sessions inside poll — sessionIds may arrive after the
      // terminal is added (terminal add fires getIds() before sessionId is set),
      // so we must re-read here on every tick instead of snapshotting once.
      const currentSessions: Array<{ termId: string; sessionId: string }> = [];
      for (const id of allIds) {
        const sess = terminalsStore.state.terminals[id]?.sessionId;
        if (sess) currentSessions.push({ termId: id, sessionId: sess });
      }
      if (currentSessions.length === 0) return;

      const results = await Promise.allSettled(
        currentSessions.map(async ({ termId, sessionId }) => {
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

        // Debounce agent→null: require N consecutive null polls before clearing.
        // This prevents flickering when the agent spawns subprocesses.
        if (prevAgentType !== null && agentType === null) {
          const streak = (nullStreak.get(termId) ?? 0) + 1;
          nullStreak.set(termId, streak);
          if (streak < NULL_THRESHOLD) continue; // hold previous agentType
        }

        // Any non-null result resets the streak
        if (agentType !== null) {
          nullStreak.delete(termId);
        }

        if (prevAgentType !== agentType) {
          appLogger.debug("app", `[AgentPoll] ${termId} agentType "${prevAgentType}" → "${agentType}"`);
          terminalsStore.update(termId, { agentType });

          // agent→null: clear session ID so re-discovery fires on next launch
          if (prevAgentType !== null && agentType === null) {
            terminalsStore.update(termId, { agentSessionId: null });
            discoveryAttempted.delete(termId);
            nullStreak.delete(termId);
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
