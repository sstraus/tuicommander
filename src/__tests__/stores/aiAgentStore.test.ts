import { describe, it, expect, beforeEach } from "vitest";
import { aiAgentStore } from "../../stores/aiAgentStore";

describe("aiAgentStore", () => {
  beforeEach(() => {
    aiAgentStore.reset();
  });

  describe("state transitions", () => {
    it("starts in idle state", () => {
      expect(aiAgentStore.agentState()).toBe("idle");
    });

    it("transitions to running on started event", () => {
      aiAgentStore.processEvent({ type: "started", session_id: "s1" });
      expect(aiAgentStore.agentState()).toBe("running");
    });

    it("transitions to paused on paused event", () => {
      aiAgentStore.processEvent({ type: "started", session_id: "s1" });
      aiAgentStore.processEvent({ type: "paused", session_id: "s1" });
      expect(aiAgentStore.agentState()).toBe("paused");
    });

    it("transitions back to running on resumed event", () => {
      aiAgentStore.processEvent({ type: "paused", session_id: "s1" });
      aiAgentStore.processEvent({ type: "resumed", session_id: "s1" });
      expect(aiAgentStore.agentState()).toBe("running");
    });

    it("transitions to completed on completed event", () => {
      aiAgentStore.processEvent({ type: "completed", session_id: "s1", iterations: 3, reason: "end_turn" });
      expect(aiAgentStore.agentState()).toBe("completed");
      expect(aiAgentStore.completionReason()).toBe("end_turn");
    });

    it("transitions to error on error event", () => {
      aiAgentStore.processEvent({ type: "error", session_id: "s1", message: "LLM timeout" });
      expect(aiAgentStore.agentState()).toBe("error");
      expect(aiAgentStore.agentError()).toBe("LLM timeout");
    });
  });

  describe("text_chunk accumulation", () => {
    it("accumulates text chunks", () => {
      aiAgentStore.processEvent({ type: "text_chunk", session_id: "s1", text: "Hello " });
      aiAgentStore.processEvent({ type: "text_chunk", session_id: "s1", text: "world" });
      expect(aiAgentStore.textChunks()).toBe("Hello world");
    });

    it("resets text on reset", () => {
      aiAgentStore.processEvent({ type: "text_chunk", session_id: "s1", text: "data" });
      aiAgentStore.reset();
      expect(aiAgentStore.textChunks()).toBe("");
    });
  });

  describe("tool_call / tool_result matching", () => {
    it("adds pending tool call", () => {
      aiAgentStore.processEvent({
        type: "tool_call", session_id: "s1",
        tool_name: "read_screen", args: { session_id: "s1" },
      });
      const calls = aiAgentStore.toolCalls();
      expect(calls).toHaveLength(1);
      expect(calls[0].status).toBe("pending");
      expect(calls[0].toolName).toBe("read_screen");
    });

    it("matches tool_result to pending call", () => {
      aiAgentStore.processEvent({
        type: "tool_call", session_id: "s1",
        tool_name: "read_screen", args: { session_id: "s1" },
      });
      aiAgentStore.processEvent({
        type: "tool_result", session_id: "s1",
        tool_name: "read_screen", success: true, output: "$ ",
      });
      const calls = aiAgentStore.toolCalls();
      expect(calls).toHaveLength(1);
      expect(calls[0].status).toBe("done");
      if (calls[0].status === "done") {
        expect(calls[0].result.success).toBe(true);
        expect(calls[0].result.output).toBe("$ ");
        expect(calls[0].duration).toBeGreaterThanOrEqual(0);
      }
    });

    it("matches result to last pending with same name", () => {
      aiAgentStore.processEvent({ type: "tool_call", session_id: "s1", tool_name: "send_input", args: {} });
      aiAgentStore.processEvent({ type: "tool_call", session_id: "s1", tool_name: "send_input", args: {} });
      aiAgentStore.processEvent({
        type: "tool_result", session_id: "s1",
        tool_name: "send_input", success: true, output: "ok",
      });
      const calls = aiAgentStore.toolCalls();
      expect(calls[0].status).toBe("pending");
      expect(calls[1].status).toBe("done");
    });
  });

  describe("thinking iteration", () => {
    it("tracks current iteration", () => {
      aiAgentStore.processEvent({ type: "thinking", session_id: "s1", iteration: 3 });
      expect(aiAgentStore.currentIteration()).toBe(3);
    });
  });

  describe("needs_approval", () => {
    it("sets pending approval", () => {
      aiAgentStore.processEvent({
        type: "needs_approval", session_id: "s1",
        tool_name: "send_input", command: "rm -rf /tmp", reason: "destructive",
      });
      const approval = aiAgentStore.pendingApproval();
      expect(approval).not.toBeNull();
      expect(approval!.command).toBe("rm -rf /tmp");
      expect(approval!.reason).toBe("destructive");
    });
  });

  describe("reset", () => {
    it("resets all state to idle", () => {
      aiAgentStore.processEvent({ type: "started", session_id: "s1" });
      aiAgentStore.processEvent({ type: "tool_call", session_id: "s1", tool_name: "x", args: {} });
      aiAgentStore.processEvent({ type: "text_chunk", session_id: "s1", text: "data" });
      aiAgentStore.reset();
      expect(aiAgentStore.agentState()).toBe("idle");
      expect(aiAgentStore.toolCalls()).toHaveLength(0);
      expect(aiAgentStore.textChunks()).toBe("");
      expect(aiAgentStore.currentIteration()).toBe(0);
      expect(aiAgentStore.pendingApproval()).toBeNull();
      expect(aiAgentStore.agentError()).toBeNull();
      expect(aiAgentStore.completionReason()).toBeNull();
    });
  });

  describe("ignores unknown events", () => {
    it("does not crash on unknown event type", () => {
      aiAgentStore.processEvent({ type: "unknown_future_event" });
      expect(aiAgentStore.agentState()).toBe("idle");
    });

    it("ignores non-object input", () => {
      aiAgentStore.processEvent(null);
      aiAgentStore.processEvent(42);
      aiAgentStore.processEvent("string");
      expect(aiAgentStore.agentState()).toBe("idle");
    });
  });
});
