import { createSignal } from "solid-js";
import { invoke } from "../invoke";
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

/** Agent detection hook */
export function useAgentDetection() {
  const [detections, setDetections] = createSignal<Map<AgentType, AgentAvailability>>(new Map());
  const [loading, setLoading] = createSignal(false);

  /** Detect a single agent binary */
  async function detectAgent(type: AgentType, binary: string): Promise<AgentAvailability> {
    try {
      const result = await invoke<AgentDetection>("detect_agent_binary", { binary });
      return {
        type,
        available: result.path !== null,
        path: result.path,
        version: result.version,
      };
    } catch (err) {
      console.error(`Failed to detect ${type}:`, err);
      return {
        type,
        available: false,
        path: null,
        version: null,
      };
    }
  }

  /** Detect all agents */
  async function detectAll(): Promise<void> {
    setLoading(true);

    const agents: Array<{ type: AgentType; binary: string }> = [
      { type: "claude", binary: "claude" },
      { type: "gemini", binary: "gemini" },
      { type: "opencode", binary: "opencode" },
      { type: "aider", binary: "aider" },
      { type: "codex", binary: "codex" },
      { type: "amp", binary: "amp" },
      { type: "jules", binary: "jules" },
      { type: "cursor", binary: "cursor-agent" },
      { type: "warp", binary: "oz" },
      { type: "ona", binary: "gitpod" },
    ];

    const results = await Promise.all(
      agents.map(({ type, binary }) => detectAgent(type, binary))
    );

    const newMap = new Map<AgentType, AgentAvailability>();
    for (const result of results) {
      newMap.set(result.type, result);
    }
    setDetections(newMap);

    setLoading(false);
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
    detectAgent,
    getDetection,
    isAvailable,
    getAvailable,
  };
}
