/**
 * AI Agent store — manages agent loop state, events, and tool call history.
 *
 * Thin projection of AgentLoopEvent from the Rust backend.
 * Drives agent mode UI: toggle, tool call cards, confirmation, pause/resume.
 */

import { createSignal, batch } from "solid-js";
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

// ---------------------------------------------------------------------------
// Signals
// ---------------------------------------------------------------------------

const [agentState, setAgentState] = createSignal<AgentState>("idle");
const [currentIteration, setCurrentIteration] = createSignal(0);
const [toolCalls, setToolCalls] = createSignal<ToolCallEntry[]>([]);
const [textChunks, setTextChunks] = createSignal("");
const [pendingApproval, setPendingApproval] = createSignal<PendingApproval | null>(null);
const [queuedInputCount, setQueuedInputCount] = createSignal(0);
const [agentError, setAgentError] = createSignal<string | null>(null);
const [completionReason, setCompletionReason] = createSignal<string | null>(null);

// ---------------------------------------------------------------------------
// Agent control
// ---------------------------------------------------------------------------

async function startAgent(sessionId: string, goal: string): Promise<void> {
  if (!isTauri()) return;
  if (agentState() === "running" || agentState() === "paused") return;

  batch(() => {
    setAgentState("running");
    setToolCalls([]);
    setTextChunks("");
    setAgentError(null);
    setCompletionReason(null);
    setCurrentIteration(0);
    setQueuedInputCount(0);
    setPendingApproval(null);
  });

  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("start_agent_loop", { sessionId, goal });
  } catch (e) {
    batch(() => {
      setAgentState("error");
      setAgentError(String(e));
    });
    appLogger.warn("ai-agent", "start_agent_loop failed", { error: String(e) });
  }
}

async function cancelAgent(sessionId: string): Promise<void> {
  if (!isTauri()) return;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("cancel_agent_loop", { sessionId });
    setAgentState("cancelled");
  } catch (e) {
    setAgentState("error");
    setAgentError(String(e));
    appLogger.warn("ai-agent", "cancel_agent_loop failed", { error: String(e) });
  }
}

async function pauseAgent(sessionId: string): Promise<void> {
  if (!isTauri()) return;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("pause_agent_loop", { sessionId });
    setAgentState("paused");
  } catch (e) {
    setAgentState("error");
    setAgentError(String(e));
    appLogger.warn("ai-agent", "pause_agent_loop failed", { error: String(e) });
  }
}

async function resumeAgent(sessionId: string): Promise<void> {
  if (!isTauri()) return;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("resume_agent_loop", { sessionId });
    setAgentState("running");
  } catch (e) {
    setAgentState("error");
    setAgentError(String(e));
    appLogger.warn("ai-agent", "resume_agent_loop failed", { error: String(e) });
  }
}

async function approveAction(sessionId: string, approved: boolean): Promise<void> {
  if (!isTauri()) return;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("approve_agent_action", { sessionId, approved });
    setPendingApproval(null);
  } catch (e) {
    setAgentState("error");
    setAgentError(String(e));
    appLogger.warn("ai-agent", "approve_agent_action failed", { error: String(e) });
  }
}

// ---------------------------------------------------------------------------
// Event processing (called from Tauri event listener)
// ---------------------------------------------------------------------------

/** Process an AgentLoopEvent from the backend. */
function processEvent(raw: unknown): void {
  if (!isAgentEvent(raw)) return;
  const event = raw;
  switch (event.type) {
    case "started":
      setAgentState("running");
      break;

    case "thinking":
      setCurrentIteration(event.iteration);
      break;

    case "text_chunk":
      setTextChunks((prev) => prev + event.text);
      break;

    case "tool_call": {
      const entry: ToolCallEntry = {
        status: "pending",
        toolName: event.tool_name,
        args: event.args,
        startedAt: Date.now(),
      };
      setToolCalls((prev) => [...prev, entry]);
      break;
    }

    case "tool_result": {
      setToolCalls((prev) => {
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
      setPendingApproval({
        sessionId: event.session_id,
        command: event.command,
        reason: event.reason,
      });
      break;

    case "paused":
      setAgentState("paused");
      break;

    case "resumed":
      setAgentState("running");
      break;

    case "rate_limited":
      appLogger.info("ai-agent", `Rate limited, waiting ${event.wait_ms}ms`);
      break;

    case "error":
      batch(() => {
        setAgentState("error");
        setAgentError(event.message);
      });
      break;

    case "completed":
      batch(() => {
        setAgentState("completed");
        setCompletionReason(event.reason);
      });
      break;
  }
}

/** Reset store to idle state. */
function reset(): void {
  batch(() => {
    setAgentState("idle");
    setToolCalls([]);
    setTextChunks("");
    setAgentError(null);
    setCompletionReason(null);
    setCurrentIteration(0);
    setQueuedInputCount(0);
    setPendingApproval(null);
  });
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

export const aiAgentStore = {
  // Reactive getters
  agentState,
  currentIteration,
  toolCalls,
  textChunks,
  pendingApproval,
  queuedInputCount,
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

  // Direct setters for external integration
  setQueuedInputCount,
};
