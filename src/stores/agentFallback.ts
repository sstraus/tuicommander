import { createStore } from "solid-js/store";
import { invoke } from "../invoke";
import type { AgentType } from "../agents";
import { AGENTS } from "../agents";

function isAgentType(value: string): value is AgentType {
  return value in AGENTS;
}
import { rateLimitStore } from "./ratelimit";

/** Fallback chain configuration */
export interface FallbackChainConfig {
  primary: AgentType;
  fallbacks: AgentType[];
  recoveryIntervalMs: number;
  autoRecovery: boolean;
}

/** Agent fallback store state */
interface AgentFallbackState {
  /** Current active agent */
  activeAgent: AgentType;
  /** Primary (preferred) agent */
  primaryAgent: AgentType;
  /** Ordered fallback chain */
  fallbackChain: AgentType[];
  /** Whether we're using a fallback */
  usingFallback: boolean;
  /** Recovery check interval ID */
  recoveryIntervalId: number | null;
  /** Recovery interval in milliseconds */
  recoveryIntervalMs: number;
  /** Auto-recovery enabled */
  autoRecovery: boolean;
  /** Last recovery check timestamp */
  lastRecoveryCheck: number | null;
  /** Agents that are currently unavailable (rate limited or other issues) */
  unavailableAgents: Set<AgentType>;
}

const LEGACY_STORAGE_KEY = "tui-commander-agent-fallback";
const DEFAULT_RECOVERY_INTERVAL = 60000; // 1 minute

/** Persist config to Rust backend (fire-and-forget) */
function saveConfig(config: FallbackChainConfig): void {
  invoke("save_agent_config", {
    config: {
      primary_agent: config.primary,
      auto_recovery: config.autoRecovery,
      fallback_chain: config.fallbacks,
      recovery_interval_ms: config.recoveryIntervalMs,
    },
  }).catch((err) => console.debug("Failed to save agent fallback config:", err));
}

/** Default fallback chain */
const DEFAULT_FALLBACK_CHAIN: AgentType[] = ["claude", "gemini", "opencode", "aider", "codex"];

/** Create the agent fallback store */
function createAgentFallbackStore() {
  const [state, setState] = createStore<AgentFallbackState>({
    activeAgent: "claude",
    primaryAgent: "claude",
    fallbackChain: DEFAULT_FALLBACK_CHAIN.filter((a) => a !== "claude"),
    usingFallback: false,
    recoveryIntervalId: null,
    recoveryIntervalMs: DEFAULT_RECOVERY_INTERVAL,
    autoRecovery: true,
    lastRecoveryCheck: null,
    unavailableAgents: new Set(),
  });

  const actions = {
    /** Load config from Rust backend; migrate from localStorage on first run */
    async hydrate(): Promise<void> {
      try {
        // One-time migration from localStorage
        const legacy = localStorage.getItem(LEGACY_STORAGE_KEY);
        if (legacy) {
          try {
            const parsed = JSON.parse(legacy);
            await invoke("save_agent_config", {
              config: {
                primary_agent: parsed.primary || "claude",
                auto_recovery: parsed.autoRecovery ?? true,
                fallback_chain: parsed.fallbacks || [],
                recovery_interval_ms: parsed.recoveryIntervalMs || DEFAULT_RECOVERY_INTERVAL,
              },
            });
          } catch { /* ignore corrupt legacy data */ }
          localStorage.removeItem(LEGACY_STORAGE_KEY);
        }

        const loaded = await invoke<{
          primary_agent?: string;
          auto_recovery?: boolean;
          fallback_chain?: string[];
          recovery_interval_ms?: number;
        }>("load_agent_config");

        if (loaded) {
          const rawPrimary = loaded.primary_agent || "claude";
          const primary: AgentType = isAgentType(rawPrimary) ? rawPrimary : "claude";
          const rawFallbacks = loaded.fallback_chain || DEFAULT_FALLBACK_CHAIN.filter((a) => a !== primary);
          const fallbacks: AgentType[] = rawFallbacks.filter(isAgentType);
          const recoveryMs = loaded.recovery_interval_ms || DEFAULT_RECOVERY_INTERVAL;
          const autoRecovery = loaded.auto_recovery ?? true;

          setState({
            activeAgent: primary,
            primaryAgent: primary,
            fallbackChain: fallbacks,
            recoveryIntervalMs: recoveryMs,
            autoRecovery,
          });
        }
      } catch (err) {
        console.debug("Failed to hydrate agent fallback config:", err);
      }
    },

    /** Configure the fallback chain */
    configure(config: FallbackChainConfig): void {
      setState({
        primaryAgent: config.primary,
        fallbackChain: config.fallbacks,
        recoveryIntervalMs: config.recoveryIntervalMs,
        autoRecovery: config.autoRecovery,
      });

      // If not using fallback, set active to primary
      if (!state.usingFallback) {
        setState("activeAgent", config.primary);
      }

      saveConfig(config);

      // Restart recovery checks if needed
      if (config.autoRecovery && state.usingFallback) {
        actions.startRecoveryChecks();
      }
    },

    /** Mark an agent as unavailable (rate limited) */
    markUnavailable(agent: AgentType): void {
      const newUnavailable = new Set(state.unavailableAgents);
      newUnavailable.add(agent);
      setState("unavailableAgents", newUnavailable);

      // If this is the active agent, switch to fallback
      if (agent === state.activeAgent) {
        actions.switchToFallback();
      }
    },

    /** Mark an agent as available again */
    markAvailable(agent: AgentType): void {
      const newUnavailable = new Set(state.unavailableAgents);
      newUnavailable.delete(agent);
      setState("unavailableAgents", newUnavailable);

      // If this is the primary and we're using fallback, try to recover
      if (agent === state.primaryAgent && state.usingFallback) {
        actions.tryRecoverToPrimary();
      }
    },

    /** Switch to the next available fallback agent */
    switchToFallback(): void {
      // Build full chain: primary + fallbacks
      const fullChain = [state.primaryAgent, ...state.fallbackChain];

      // Find first available agent
      for (const agent of fullChain) {
        if (!state.unavailableAgents.has(agent)) {
          setState("activeAgent", agent);
          setState("usingFallback", agent !== state.primaryAgent);

          // Start recovery checks if now using fallback
          if (state.usingFallback && state.autoRecovery) {
            actions.startRecoveryChecks();
          }
          return;
        }
      }

      // All agents unavailable - stay on current or first in chain
      console.warn("All agents unavailable, no fallback available");
    },

    /** Try to recover to primary agent */
    tryRecoverToPrimary(): void {
      if (!state.usingFallback) return;

      // Check if primary is available
      if (!state.unavailableAgents.has(state.primaryAgent)) {
        setState("activeAgent", state.primaryAgent);
        setState("usingFallback", false);
        actions.stopRecoveryChecks();
        console.log(`Recovered to primary agent: ${state.primaryAgent}`);
      }
    },

    /** Start periodic recovery checks */
    startRecoveryChecks(): void {
      // Clear existing interval
      actions.stopRecoveryChecks();

      if (!state.autoRecovery) return;

      const intervalId = window.setInterval(() => {
        setState("lastRecoveryCheck", Date.now());

        // Check if primary agent's rate limit has expired
        const primaryRateLimit = Object.values(rateLimitStore.state.rateLimits).find(
          (rl) => rl.agentType === state.primaryAgent
        );

        if (!primaryRateLimit || !rateLimitStore.isRateLimited(primaryRateLimit.sessionId)) {
          // Primary might be available - mark as available
          actions.markAvailable(state.primaryAgent);
        }
      }, state.recoveryIntervalMs);

      setState("recoveryIntervalId", intervalId);
    },

    /** Stop recovery checks */
    stopRecoveryChecks(): void {
      if (state.recoveryIntervalId !== null) {
        clearInterval(state.recoveryIntervalId);
        setState("recoveryIntervalId", null);
      }
    },

    /** Get the current active agent */
    getActiveAgent(): AgentType {
      return state.activeAgent;
    },

    /** Check if using fallback */
    isUsingFallback(): boolean {
      return state.usingFallback;
    },

    /** Get fallback status message */
    getStatusMessage(): string | null {
      if (!state.usingFallback) return null;
      return `Using fallback: ${state.activeAgent} (primary: ${state.primaryAgent})`;
    },

    /** Set primary agent */
    setPrimary(agent: AgentType): void {
      const config: FallbackChainConfig = {
        primary: agent,
        fallbacks: state.fallbackChain.filter((a) => a !== agent),
        recoveryIntervalMs: state.recoveryIntervalMs,
        autoRecovery: state.autoRecovery,
      };
      actions.configure(config);
    },

    /** Dev-only: override active agent without persistence */
    _devOverrideActive(agent: AgentType): void {
      setState({ activeAgent: agent, usingFallback: agent !== state.primaryAgent });
    },

    /** Reset to primary (force) */
    forceResetToPrimary(): void {
      setState("unavailableAgents", new Set());
      setState("activeAgent", state.primaryAgent);
      setState("usingFallback", false);
      actions.stopRecoveryChecks();
    },

    /** Cleanup on unmount */
    cleanup(): void {
      actions.stopRecoveryChecks();
    },
  };

  return { state, ...actions };
}

export const agentFallbackStore = createAgentFallbackStore();
