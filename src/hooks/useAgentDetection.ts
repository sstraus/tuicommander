import { createSignal } from "solid-js";
import { invoke } from "../invoke";
import { appLogger } from "../stores/appLogger";
import type { AgentType } from "../agents";

/** Agent binary detection result */
interface AgentDetection {
  path: string | null;
  version: string | null;
}

/** Agent availability status */
export interface AgentAvailability {
  type: AgentType;
  available: boolean;
  path: string | null;
  version: string | null;
}

/** Binary name for each agent type */
const AGENT_BINARIES: Record<AgentType, string> = {
  claude: "claude",
  gemini: "gemini",
  opencode: "opencode",
  aider: "aider",
  codex: "codex",
  amp: "amp",
  cursor: "cursor-agent",
  warp: "oz",
  droid: "droid",
  git: "git",
};

/** Agent detection hook */
export function useAgentDetection() {
  const [detections, setDetections] = createSignal<Map<AgentType, AgentAvailability>>(new Map());
  const [loading, setLoading] = createSignal(false);

  /** Detect all agents in a single batch call (fast, no version detection) */
  async function detectAll(): Promise<void> {
    setLoading(true);

    try {
      const binaries = Object.values(AGENT_BINARIES);
      const results = await invoke<Record<string, AgentDetection>>(
        "detect_all_agent_binaries",
        { binaries },
      );

      const newMap = new Map<AgentType, AgentAvailability>();
      for (const [agentType, binary] of Object.entries(AGENT_BINARIES)) {
        const det = results[binary];
        newMap.set(agentType as AgentType, {
          type: agentType as AgentType,
          available: det?.path !== null && det?.path !== undefined,
          path: det?.path ?? null,
          version: det?.version ?? null,
        });
      }
      setDetections(newMap);
    } catch (err) {
      appLogger.error("app", "Failed to detect agents", err);
    } finally {
      setLoading(false);
    }
  }

  /** Detect version for a single agent (lazy, called on expand) */
  async function detectVersion(type: AgentType): Promise<void> {
    const current = detections().get(type);
    if (!current?.available || current.version) return;

    try {
      const result = await invoke<AgentDetection>("detect_agent_binary", {
        binary: AGENT_BINARIES[type],
      });
      if (result.version) {
        const newMap = new Map(detections());
        newMap.set(type, { ...current, version: result.version });
        setDetections(newMap);
      }
    } catch {
      // Version detection is best-effort
    }
  }

  /** Get detection result for an agent */
  function getDetection(type: AgentType): AgentAvailability | undefined {
    return detections().get(type);
  }

  /** Check if an agent is available */
  function isAvailable(type: AgentType): boolean {
    const detection = detections().get(type);
    return detection?.available ?? false;
  }

  /** Get all available agents */
  function getAvailable(): AgentAvailability[] {
    return Array.from(detections().values()).filter((d) => d.available);
  }

  return {
    detections,
    loading,
    detectAll,
    detectVersion,
    getDetection,
    isAvailable,
    getAvailable,
  };
}
