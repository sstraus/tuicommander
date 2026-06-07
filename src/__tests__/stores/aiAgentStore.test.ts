import { beforeEach, describe, expect, it } from "vitest";
import { conversationStore } from "../../stores/conversationStore";

describe("aiAgentStore (via conversationStore)", () => {
	beforeEach(() => {
		conversationStore.reset();
	});

	describe("state transitions", () => {
		it("starts in idle state", () => {
			expect(conversationStore.agentState()).toBe("idle");
		});

		it("transitions to running on started event", () => {
			conversationStore.processEvent({ type: "started", session_id: "s1" });
			expect(conversationStore.agentState()).toBe("running");
		});

		it("transitions to paused on paused event", () => {
			conversationStore.processEvent({ type: "started", session_id: "s1" });
			conversationStore.processEvent({ type: "paused", session_id: "s1" });
			expect(conversationStore.agentState()).toBe("paused");
		});

		it("transitions back to running on resumed event", () => {
			conversationStore.processEvent({ type: "paused", session_id: "s1" });
			conversationStore.processEvent({ type: "resumed", session_id: "s1" });
			expect(conversationStore.agentState()).toBe("running");
		});

		it("transitions to completed on completed event", () => {
			conversationStore.processEvent({ type: "completed", session_id: "s1", iterations: 3, reason: "end_turn" });
			expect(conversationStore.agentState()).toBe("completed");
			expect(conversationStore.completionReason()).toBe("end_turn");
		});

		it("transitions to error on error event", () => {
			conversationStore.processEvent({ type: "error", session_id: "s1", message: "LLM timeout" });
			expect(conversationStore.agentState()).toBe("error");
			expect(conversationStore.agentError()).toBe("LLM timeout");
		});
	});

	describe("text_chunk accumulation", () => {
		it("accumulates text chunks", () => {
			conversationStore.processEvent({ type: "text_chunk", session_id: "s1", text: "Hello " });
			conversationStore.processEvent({ type: "text_chunk", session_id: "s1", text: "world" });
			expect(conversationStore.textChunks()).toBe("Hello world");
		});

		it("resets text on reset", () => {
			conversationStore.processEvent({ type: "text_chunk", session_id: "s1", text: "data" });
			conversationStore.reset();
			expect(conversationStore.textChunks()).toBe("");
		});
	});

	describe("reasoning_chunk accumulation", () => {
		it("accumulates reasoning chunks separately from text", () => {
			conversationStore.processEvent({ type: "reasoning_chunk", session_id: "s1", text: "let me " });
			conversationStore.processEvent({ type: "reasoning_chunk", session_id: "s1", text: "think" });
			conversationStore.processEvent({ type: "text_chunk", session_id: "s1", text: "answer" });
			expect(conversationStore.reasoningChunks()).toBe("let me think");
			expect(conversationStore.textChunks()).toBe("answer");
		});

		it("resets reasoning on reset", () => {
			conversationStore.processEvent({ type: "reasoning_chunk", session_id: "s1", text: "data" });
			conversationStore.reset();
			expect(conversationStore.reasoningChunks()).toBe("");
		});
	});

	describe("tool_call / tool_result matching", () => {
		it("adds pending tool call", () => {
			conversationStore.processEvent({
				type: "tool_call",
				session_id: "s1",
				tool_name: "read_screen",
				args: { session_id: "s1" },
			});
			const calls = conversationStore.toolCalls();
			expect(calls).toHaveLength(1);
			expect(calls[0].status).toBe("pending");
			expect(calls[0].toolName).toBe("read_screen");
		});

		it("matches tool_result to pending call", () => {
			conversationStore.processEvent({
				type: "tool_call",
				session_id: "s1",
				tool_name: "read_screen",
				args: { session_id: "s1" },
			});
			conversationStore.processEvent({
				type: "tool_result",
				session_id: "s1",
				tool_name: "read_screen",
				success: true,
				output: "$ ",
			});
			const calls = conversationStore.toolCalls();
			expect(calls).toHaveLength(1);
			expect(calls[0].status).toBe("done");
			if (calls[0].status === "done") {
				expect(calls[0].result.success).toBe(true);
				expect(calls[0].result.output).toBe("$ ");
				expect(calls[0].duration).toBeGreaterThanOrEqual(0);
			}
		});

		it("matches result to last pending with same name", () => {
			conversationStore.processEvent({ type: "tool_call", session_id: "s1", tool_name: "send_input", args: {} });
			conversationStore.processEvent({ type: "tool_call", session_id: "s1", tool_name: "send_input", args: {} });
			conversationStore.processEvent({
				type: "tool_result",
				session_id: "s1",
				tool_name: "send_input",
				success: true,
				output: "ok",
			});
			const calls = conversationStore.toolCalls();
			expect(calls[0].status).toBe("pending");
			expect(calls[1].status).toBe("done");
		});
	});

	describe("thinking iteration", () => {
		it("tracks current iteration", () => {
			conversationStore.processEvent({ type: "thinking", session_id: "s1", iteration: 3 });
			expect(conversationStore.currentIteration()).toBe(3);
		});
	});

	describe("needs_approval", () => {
		it("sets pending approval", () => {
			conversationStore.processEvent({
				type: "needs_approval",
				session_id: "s1",
				tool_name: "send_input",
				command: "rm -rf /tmp",
				reason: "destructive",
			});
			const approval = conversationStore.pendingApproval();
			expect(approval).not.toBeNull();
			expect(approval!.command).toBe("rm -rf /tmp");
			expect(approval!.reason).toBe("destructive");
		});
	});

	describe("reset", () => {
		it("resets all state to idle", () => {
			conversationStore.processEvent({ type: "started", session_id: "s1" });
			conversationStore.processEvent({ type: "tool_call", session_id: "s1", tool_name: "x", args: {} });
			conversationStore.processEvent({ type: "text_chunk", session_id: "s1", text: "data" });
			conversationStore.reset();
			expect(conversationStore.agentState()).toBe("idle");
			expect(conversationStore.toolCalls()).toHaveLength(0);
			expect(conversationStore.textChunks()).toBe("");
			expect(conversationStore.currentIteration()).toBe(0);
			expect(conversationStore.pendingApproval()).toBeNull();
			expect(conversationStore.agentError()).toBeNull();
			expect(conversationStore.completionReason()).toBeNull();
		});
	});

	describe("per-terminal agent state (1409-e641)", () => {
		beforeEach(() => {
			conversationStore.setActiveTerminal("__default__");
			conversationStore.reset();
		});

		it("getOrCreate returns independent state for different keys", () => {
			const stateA = conversationStore.getOrCreate("termA");
			const stateB = conversationStore.getOrCreate("termB");
			expect(stateA).not.toBe(stateB);
		});

		it("agentState() reflects the active terminal only", () => {
			conversationStore.setActiveTerminal("T1");
			conversationStore.processEvent({ type: "started", session_id: "s1" });
			expect(conversationStore.agentState()).toBe("running");

			conversationStore.setActiveTerminal("T2");
			expect(conversationStore.agentState()).toBe("idle");
		});

		it("toolCalls() is independent per terminal", () => {
			conversationStore.setActiveTerminal("T1");
			conversationStore.processEvent({ type: "tool_call", session_id: "s1", tool_name: "bash", args: {} });

			conversationStore.setActiveTerminal("T2");
			expect(conversationStore.toolCalls()).toEqual([]);

			conversationStore.setActiveTerminal("T1");
			expect(conversationStore.toolCalls()).toHaveLength(1);
		});

		it("activeConversation() returns the PerTerminalConversationState for the active terminal", () => {
			conversationStore.setActiveTerminal("termX");
			const state = conversationStore.activeConversation();
			expect(typeof state.agentState).toBe("function");
			expect(typeof state.toolCalls).toBe("function");
		});

		it("reset() clears only the active terminal state", () => {
			conversationStore.setActiveTerminal("T1");
			conversationStore.processEvent({ type: "started", session_id: "s1" });

			conversationStore.setActiveTerminal("T2");
			conversationStore.processEvent({ type: "started", session_id: "s2" });

			conversationStore.setActiveTerminal("T1");
			conversationStore.reset();
			expect(conversationStore.agentState()).toBe("idle");

			conversationStore.setActiveTerminal("T2");
			expect(conversationStore.agentState()).toBe("running");
		});
	});

	describe("ignores unknown events", () => {
		it("does not crash on unknown event type", () => {
			conversationStore.processEvent({ type: "unknown_future_event" });
			expect(conversationStore.agentState()).toBe("idle");
		});

		it("ignores non-object input", () => {
			conversationStore.processEvent(null);
			conversationStore.processEvent(42);
			conversationStore.processEvent("string");
			expect(conversationStore.agentState()).toBe("idle");
		});
	});
});
