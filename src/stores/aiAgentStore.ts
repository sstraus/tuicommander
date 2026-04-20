/**
 * AI Agent store — manages agent loop state, events, and tool call history.
 *
 * Per-terminal architecture: each terminal gets its own PerTerminalAgentState
 * keyed by terminal key (tuicSession or id). Existing exports proxy through
 * activeAgent() so all 21 callers remain unchanged.
 *
 * Thin projection of AgentLoopEvent from the Rust backend.
 * Drives agent mode UI: toggle, tool call cards, confirmation, pause/resume.
 */

import { createSignal, batch } from "solid-js";
import type { Accessor, Setter } from "solid-js";
import { isTauri } from "../transport";
import { appLogger } from "./appLogger";

// ---------------------------------------------------------------------------
// Types (mirror Rust AgentLoopEvent variants)
// ---------------------------------------------------------------------------

export type AgentState = "running" | "paused" | "completed" | "cancelled" | "error" | "idle";

export type ToolCallEntry =
  | { status: "pending"; toolName: string; args: Record<string, unknown>; startedAt: number }
  | { status: "done"; toolName: string; args: Record<string, unknown>; startedAt: number; result: { success: boolean; output: string }; duration: number };

export interface PendingApproval {
  sessionId: string;
  command: string;
  reason: string;
}

// Discriminated union for backend events
type AgentEvent =
  | { type: "started"; session_id: string }
  | { type: "thinking"; session_id: string; iteration: number }
  | { type: "text_chunk"; session_id: string; text: string }
  | { type: "tool_call"; session_id: string; tool_name: string; args: Record<string, unknown> }
  | { type: "tool_result"; session_id: string; tool_name: string; success: boolean; output: string }
  | { type: "needs_approval"; session_id: string; tool_name: string; command: string; reason: string }
  | { type: "paused"; session_id: string }
  | { type: "resumed"; session_id: string }
  | { type: "rate_limited"; session_id: string; wait_ms: number }
  | { type: "error"; session_id: string; message: string }
  | { type: "completed"; session_id: string; iterations: number; reason: string };

function isAgentEvent(v: unknown): v is AgentEvent {
  return typeof v === "object" && v !== null && "type" in v && typeof (v as { type: unknown }).type === "string";
}

/** Reactive state for a single terminal's agent session. */
export interface PerTerminalAgentState {
  agentState: Accessor<AgentState>;
  setAgentState: Setter<AgentState>;
  currentIteration: Accessor<number>;
  setCurrentIteration: Setter<number>;
  toolCalls: Accessor<ToolCallEntry[]>;
  setToolCalls: Setter<ToolCallEntry[]>;
  textChunks: Accessor<string>;
  setTextChunks: Setter<string>;
  pendingApproval: Accessor<PendingApproval | null>;
  setPendingApproval: Setter<PendingApproval | null>;
  agentError: Accessor<string | null>;
  setAgentError: Setter<string | null>;
  completionReason: Accessor<string | null>;
  setCompletionReason: Setter<string | null>;
}

const DEFAULT_KEY = "__default__";

const agentStateMap = new Map<string, PerTerminalAgentState>();
const [activeAgentKey, setActiveAgentKey] = createSignal<string>(DEFAULT_KEY);

function createAgentState(): PerTerminalAgentState {
  const [agentState, setAgentState] = createSignal<AgentState>("idle");
  const [currentIteration, setCurrentIteration] = createSignal(0);
  const [toolCalls, setToolCalls] = createSignal<ToolCallEntry[]>([]);
  const [textChunks, setTextChunks] = createSignal("");
  const [pendingApproval, setPendingApproval] = createSignal<PendingApproval | null>(null);
  const [agentError, setAgentError] = createSignal<string | null>(null);
  const [completionReason, setCompletionReason] = createSignal<string | null>(null);
  return {
    agentState, setAgentState,
    currentIteration, setCurrentIteration,
    toolCalls, setToolCalls,
    textChunks, setTextChunks,
    pendingApproval, setPendingApproval,
    agentError, setAgentError,
    completionReason, setCompletionReason,
  };
}

function getOrCreate(key: string): PerTerminalAgentState {
  let state = agentStateMap.get(key);
  if (!state) {
    state = createAgentState();
    agentStateMap.set(key, state);
  }
  return state;
}

function activeAgent(): PerTerminalAgentState {
  return getOrCreate(activeAgentKey());
}

function setActiveTerminal(key: string): void {
  setActiveAgentKey(key);
}

// ---------------------------------------------------------------------------
// Convenience accessors — proxy through activeAgent()
// ---------------------------------------------------------------------------

function agentState(): AgentState { return activeAgent().agentState(); }
function currentIteration(): number { return activeAgent().currentIteration(); }
function toolCalls(): ToolCallEntry[] { return activeAgent().toolCalls(); }
function textChunks(): string { return activeAgent().textChunks(); }
function pendingApproval(): PendingApproval | null { return activeAgent().pendingApproval(); }
function agentError(): string | null { return activeAgent().agentError(); }
function completionReason(): string | null { return activeAgent().completionReason(); }

// ---------------------------------------------------------------------------
// Agent control
// ---------------------------------------------------------------------------

async function startAgent(sessionId: string, goal: string): Promise<void> {
  if (!isTauri()) return;
  const s = activeAgent();
  if (s.agentState() === "running" || s.agentState() === "paused") return;

  batch(() => {
    s.setAgentState("running");
    s.setToolCalls([]);
    s.setTextChunks("");
    s.setAgentError(null);
    s.setCompletionReason(null);
    s.setCurrentIteration(0);
    s.setPendingApproval(null);
  });

  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("start_agent_loop", { sessionId, goal });
  } catch (e) {
    batch(() => {
      s.setAgentState("error");
      s.setAgentError(String(e));
    });
    appLogger.warn("ai-agent", "start_agent_loop failed", { error: String(e) });
  }
}

async function cancelAgent(sessionId: string): Promise<void> {
  if (!isTauri()) return;
  const s = activeAgent();
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("cancel_agent_loop", { sessionId });
    s.setAgentState("cancelled");
  } catch (e) {
    s.setAgentState("error");
    s.setAgentError(String(e));
    appLogger.warn("ai-agent", "cancel_agent_loop failed", { error: String(e) });
  }
}

async function pauseAgent(sessionId: string): Promise<void> {
  if (!isTauri()) return;
  const s = activeAgent();
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("pause_agent_loop", { sessionId });
    s.setAgentState("paused");
  } catch (e) {
    s.setAgentState("error");
    s.setAgentError(String(e));
    appLogger.warn("ai-agent", "pause_agent_loop failed", { error: String(e) });
  }
}

async function resumeAgent(sessionId: string): Promise<void> {
  if (!isTauri()) return;
  const s = activeAgent();
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("resume_agent_loop", { sessionId });
    s.setAgentState("running");
  } catch (e) {
    s.setAgentState("error");
    s.setAgentError(String(e));
    appLogger.warn("ai-agent", "resume_agent_loop failed", { error: String(e) });
  }
}

async function approveAction(sessionId: string, approved: boolean): Promise<void> {
  if (!isTauri()) return;
  const s = activeAgent();
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("approve_agent_action", { sessionId, approved });
    s.setPendingApproval(null);
  } catch (e) {
    s.setAgentState("error");
    s.setAgentError(String(e));
    appLogger.warn("ai-agent", "approve_agent_action failed", { error: String(e) });
  }
}

// ---------------------------------------------------------------------------
// Event processing — routes to active terminal's state
// ---------------------------------------------------------------------------

/** Process an AgentLoopEvent from the backend. */
function processEvent(raw: unknown): void {
  if (!isAgentEvent(raw)) return;
  const s = activeAgent();
  const event = raw;
  switch (event.type) {
    case "started":
      s.setAgentState("running");
      break;

    case "thinking":
      s.setCurrentIteration(event.iteration);
      break;

    case "text_chunk":
      s.setTextChunks((prev) => prev + event.text);
      break;

    case "tool_call": {
      const entry: ToolCallEntry = {
        status: "pending",
        toolName: event.tool_name,
        args: event.args,
        startedAt: Date.now(),
      };
      s.setToolCalls((prev) => [...prev, entry]);
      break;
    }

    case "tool_result": {
      s.setToolCalls((prev) => {
        const updated = [...prev];
        for (let i = updated.length - 1; i >= 0; i--) {
          if (updated[i].toolName === event.tool_name && updated[i].status === "pending") {
            updated[i] = {
              ...updated[i],
              status: "done",
              result: { success: event.success, output: event.output },
              duration: Date.now() - updated[i].startedAt,
            };
            break;
          }
        }
        return updated;
      });
      break;
    }

    case "needs_approval":
      s.setPendingApproval({
        sessionId: event.session_id,
        command: event.command,
        reason: event.reason,
      });
      break;

    case "paused":
      s.setAgentState("paused");
      break;

    case "resumed":
      s.setAgentState("running");
      break;

    case "rate_limited":
      appLogger.info("ai-agent", `Rate limited, waiting ${event.wait_ms}ms`);
      break;

    case "error":
      batch(() => {
        s.setAgentState("error");
        s.setAgentError(event.message);
      });
      break;

    case "completed":
      batch(() => {
        s.setAgentState("completed");
        s.setCompletionReason(event.reason);
      });
      break;
  }
}

/** Reset active terminal's agent state to idle. */
function reset(): void {
  const s = activeAgent();
  batch(() => {
    s.setAgentState("idle");
    s.setToolCalls([]);
    s.setTextChunks("");
    s.setAgentError(null);
    s.setCompletionReason(null);
    s.setCurrentIteration(0);
    s.setPendingApproval(null);
  });
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

export const aiAgentStore = {
  // Per-terminal API (new in 1409-e641)
  activeAgent,
  getOrCreate,
  setActiveTerminal,

  // Reactive getters (proxy through activeAgent)
  agentState,
  currentIteration,
  toolCalls,
  textChunks,
  pendingApproval,
  agentError,
  completionReason,

  // Actions
  startAgent,
  cancelAgent,
  pauseAgent,
  resumeAgent,
  approveAction,
  processEvent,
  reset,
};
