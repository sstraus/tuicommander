import { createStore, reconcile } from "solid-js/store";
import type { DetectedPrompt, AgentStats } from "../types";

/** Prompt store state */
interface PromptStoreState {
  // Active prompt
  activePrompt: DetectedPrompt | null;
  selectedOptionIndex: number;

  // Detection buffers
  outputBuffer: string;
  statsBuffer: string;

  // Agent stats per session
  sessionStats: Record<string, AgentStats>;
}

const OUTPUT_BUFFER_MAX = 5000;
const STATS_BUFFER_MAX = 3000;

/** Create the prompt store */
function createPromptStore() {
  const [state, setState] = createStore<PromptStoreState>({
    activePrompt: null,
    selectedOptionIndex: 0,
    outputBuffer: "",
    statsBuffer: "",
    sessionStats: {},
  });

  const actions = {
    /** Show a detected prompt */
    showPrompt(prompt: DetectedPrompt): void {
      setState("activePrompt", prompt);
      setState("selectedOptionIndex", 0);
      // Clear buffer after detection
      setState("outputBuffer", "");
    },

    /** Hide the prompt overlay */
    hidePrompt(): void {
      setState("activePrompt", null);
      setState("selectedOptionIndex", 0);
    },

    /** Select an option by index */
    selectOption(index: number): void {
      const prompt = state.activePrompt;
      if (!prompt) return;
      if (index < 0 || index >= prompt.options.length) return;
      setState("selectedOptionIndex", index);
    },

    /** Move selection up */
    selectPrevious(): void {
      const newIndex = Math.max(0, state.selectedOptionIndex - 1);
      setState("selectedOptionIndex", newIndex);
    },

    /** Move selection down */
    selectNext(): void {
      const prompt = state.activePrompt;
      if (!prompt) return;
      const newIndex = Math.min(prompt.options.length - 1, state.selectedOptionIndex + 1);
      setState("selectedOptionIndex", newIndex);
    },

    /** Get selected option number (1-indexed) */
    getSelectedOptionNumber(): number {
      return state.selectedOptionIndex + 1;
    },

    /** Append to output buffer (for prompt detection) */
    appendOutput(data: string): void {
      const newBuffer = state.outputBuffer + data;
      // Keep buffer size manageable
      if (newBuffer.length > OUTPUT_BUFFER_MAX) {
        setState("outputBuffer", newBuffer.slice(-OUTPUT_BUFFER_MAX / 2));
      } else {
        setState("outputBuffer", newBuffer);
      }
    },

    /** Clear output buffer */
    clearOutputBuffer(): void {
      setState("outputBuffer", "");
    },

    /** Append to stats buffer */
    appendStats(data: string): void {
      const newBuffer = state.statsBuffer + data;
      if (newBuffer.length > STATS_BUFFER_MAX) {
        setState("statsBuffer", newBuffer.slice(-STATS_BUFFER_MAX / 2));
      } else {
        setState("statsBuffer", newBuffer);
      }
    },

    /** Update stats for a session */
    updateSessionStats(sessionId: string, stats: Partial<AgentStats>): void {
      const current = state.sessionStats[sessionId] || { toolUses: 0, tokens: 0, duration: 0 };
      setState("sessionStats", sessionId, { ...current, ...stats });
    },

    /** Get stats for a session */
    getSessionStats(sessionId: string): AgentStats | undefined {
      return state.sessionStats[sessionId];
    },

    /** Clear stats for a session */
    clearSessionStats(sessionId: string): void {
      const { [sessionId]: _, ...rest } = state.sessionStats;
      setState("sessionStats", reconcile(rest));
    },

    /** Check if prompt is active */
    isActive(): boolean {
      return state.activePrompt !== null;
    },
  };

  return { state, ...actions };
}

export const promptStore = createPromptStore();
