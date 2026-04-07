import { createEffect, onCleanup } from "solid-js";
import { invoke } from "../invoke";
import { terminalsStore } from "../stores/terminals";
import { appLogger } from "../stores/appLogger";
import { type AgentType, AGENT_TYPES, AGENTS } from "../agents";
import { pluginRegistry } from "../plugins/pluginRegistry";

/** Fallback polling interval — only catches cold starts and edge cases (ms) */
const POLL_INTERVAL_MS = 30_000;

/**
 * Number of consecutive null detections on shell-idle required before clearing a detected agent.
 * Prevents flickering when the shell briefly reports idle during agent subprocess transitions.
 */
const NULL_THRESHOLD = 3;

/**
 * Detection trigger source — determines whether the call can clear an existing agentType.
 * - "idle": Shell-state transitioned to idle (prompt returned). This is the ONLY source
 *   that can clear a previously detected agent, because it means the foreground process
 *   ended and the shell reclaimed the terminal.
 * - "busy": Shell-state transitioned to busy. Can only discover (set) agents, never clear.
 * - "poll": Periodic 30s fallback. Can only discover (set) agents, never clear.
 */
export type DetectionSource = "idle" | "busy" | "poll";

/** Validate a string from the backend is a known AgentType */
function toAgentType(value: string | null): AgentType | null {
  if (value === null) return null;
  return (AGENT_TYPES as readonly string[]).includes(value) ? (value as AgentType) : null;
}

// Module-level state shared between pollAll and event-driven detection
const discoveryAttempted = new Set<string>();
const nullStreak = new Map<string, number>();

/**
 * Detect the foreground agent for a single terminal and update the store.
 * Called on shell-state transitions (event-driven) and by the fallback poll.
 *
 * @param source - What triggered this detection. Only "idle" can clear an existing agent.
 *   "busy" and "poll" can only discover new agents — they never clear, because
 *   foreground-process sampling is inherently flaky during subprocess transitions.
 */
export async function detectAgentForTerminal(termId: string, source: DetectionSource = "poll"): Promise<void> {
  const current = terminalsStore.get(termId);
  if (!current) {
    // Terminal removed — clean up module-level tracking state
    discoveryAttempted.delete(termId);
    nullStreak.delete(termId);
    return;
  }
  if (!current.sessionId) return;

  let agentType: AgentType | null;
  try {
    const result = await invoke<string | null>("get_session_foreground_process", {
      sessionId: current.sessionId,
    });
    agentType = toAgentType(result);
  } catch (err) {
    appLogger.debug("app", `[AgentDetect] ${termId} invoke failed`, err);
    return;
  }

  const prevAgentType = current.agentType;

  // Agent→null transition: only allowed from "idle" source (shell prompt returned).
  // Poll and busy sources can only discover agents, never clear them — foreground
  // process sampling is too flaky during subprocess transitions (git, node, etc.).
  if (prevAgentType !== null && agentType === null) {
    if (source !== "idle") return; // Not a reliable clearing signal — skip
    const streak = (nullStreak.get(termId) ?? 0) + 1;
    nullStreak.set(termId, streak);
    if (streak < NULL_THRESHOLD) return;
  }

  if (agentType !== null) {
    nullStreak.delete(termId);
  }

  if (prevAgentType !== agentType) {
    appLogger.debug("app", `[AgentDetect] ${termId} agentType "${prevAgentType}" → "${agentType}"`);
    terminalsStore.update(termId, { agentType });

    const sessId = current.sessionId;
    if (prevAgentType === null && agentType !== null && sessId) {
      pluginRegistry.notifyStateChange({ type: "agent-started", sessionId: sessId, terminalId: termId });
      // Replay current shell state to plugins filtered by agentType — they missed
      // events dispatched before detection completed (agentType was still null).
      const freshShellState = terminalsStore.get(termId)?.shellState;
      if (freshShellState) {
        pluginRegistry.dispatchStructuredEvent("shell-state", { state: freshShellState }, sessId);
      }
    }
    if (prevAgentType !== null && agentType === null) {
      if (sessId) {
        pluginRegistry.notifyStateChange({ type: "agent-stopped", sessionId: sessId, terminalId: termId });
      }
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
      const cwd = current.cwd ?? null;

      // Collect UUIDs already claimed by other terminals
      const claimedIds: string[] = [];
      for (const id of terminalsStore.getIds()) {
        const sid = terminalsStore.get(id)?.agentSessionId;
        if (sid) claimedIds.push(sid);
      }

      try {
        const found = await invoke<string | null>("discover_agent_session", {
          agentType,
          cwd,
          claimedIds,
        });
        if (found) {
          appLogger.debug("app", `[AgentDetect] ${termId} discovered agentSessionId "${found}"`);
          terminalsStore.update(termId, { agentSessionId: found });
        }
      } catch (err) {
        appLogger.debug("app", `[AgentDetect] ${termId} discover_agent_session failed`, err);
      }
    }
  }
}

/**
 * Fallback polling loop for agent detection.
 * Primary detection happens event-driven via shell-state transitions in Terminal.tsx.
 * This 30s fallback catches cold starts and edge cases.
 */
export function useAgentPolling(): void {
  createEffect(() => {
    const allIds = terminalsStore.getIds();
    if (allIds.length === 0) return;

    const pollAll = async () => {
      for (const id of allIds) {
        await detectAgentForTerminal(id);
      }
    };

    const timer = setInterval(() => {
      pollAll().catch((err) => appLogger.debug("app", "[AgentPoll] poll failed", err));
    }, POLL_INTERVAL_MS);

    onCleanup(() => clearInterval(timer));
  });
}
