import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { makeTerminal, testInScope } from "../helpers/store";

describe("terminalsStore", () => {
	let store: typeof import("../../stores/terminals").terminalsStore;

	beforeEach(async () => {
		vi.resetModules();
		localStorage.clear();
		store = (await import("../../stores/terminals")).terminalsStore;
	});

	afterEach(() => {
		store._testCancelPendingTimers();
	});

	describe("add()", () => {
		it("creates a terminal with generated ID", () => {
			testInScope(() => {
				const id = store.add(makeTerminal());
				expect(id).toBe("term-1");
				expect(store.get(id)).toBeDefined();
				expect(store.get(id)!.name).toBe("Test");
				expect(store.get(id)!.activity).toBe(false);
				expect(store.get(id)!.progress).toBeNull();
			});
		});

		it("increments counter for each terminal", () => {
			testInScope(() => {
				const id1 = store.add(makeTerminal({ name: "T1" }));
				const id2 = store.add(makeTerminal({ name: "T2" }));
				expect(id1).toBe("term-1");
				expect(id2).toBe("term-2");
			});
		});
	});

	describe("toggleAnswersOnly()", () => {
		it("is off for a new terminal and flips only the targeted one", () => {
			testInScope(() => {
				const id1 = store.add(makeTerminal({ name: "T1" }));
				const id2 = store.add(makeTerminal({ name: "T2" }));
				expect(store.get(id1)!.answersOnly).toBe(false);
				store.toggleAnswersOnly(id1);
				expect(store.get(id1)!.answersOnly).toBe(true);
				expect(store.get(id2)!.answersOnly).toBe(false);
				store.toggleAnswersOnly(id1);
				expect(store.get(id1)!.answersOnly).toBe(false);
			});
		});

		it("defaults to the active terminal and ignores a missing one", () => {
			testInScope(() => {
				store.toggleAnswersOnly(); // no active terminal: no-op, no throw
				const id = store.add(makeTerminal());
				store.setActive(id);
				store.toggleAnswersOnly();
				expect(store.get(id)!.answersOnly).toBe(true);
				store.toggleAnswersOnly("term-missing");
				expect(store.state.terminals["term-missing"]).toBeUndefined();
			});
		});
	});

	describe("remove()", () => {
		it("removes a terminal", () => {
			testInScope(() => {
				const id = store.add(makeTerminal());
				store.remove(id);
				expect(store.get(id)).toBeUndefined();
			});
		});

		it("sets activeId to null when active terminal is removed (caller handles replacement)", () => {
			testInScope(() => {
				const id1 = store.add(makeTerminal({ name: "T1" }));
				store.add(makeTerminal({ name: "T2" }));
				store.setActive(id1);
				store.remove(id1);
				expect(store.state.activeId).toBeNull();
			});
		});

		it("sets activeId to null when last terminal is removed", () => {
			testInScope(() => {
				const id = store.add(makeTerminal());
				store.setActive(id);
				store.remove(id);
				expect(store.state.activeId).toBeNull();
			});
		});

		it("cancels a pending OSC 133 rAF flush so it can't resurrect a ghost entry", () => {
			const rafCallbacks = new Map<number, FrameRequestCallback>();
			let nextHandle = 0;
			const raf = vi.fn((cb: FrameRequestCallback) => {
				nextHandle += 1;
				rafCallbacks.set(nextHandle, cb);
				return nextHandle;
			});
			const caf = vi.fn((handle: number) => {
				rafCallbacks.delete(handle);
			});
			vi.stubGlobal("requestAnimationFrame", raf);
			vi.stubGlobal("cancelAnimationFrame", caf);

			try {
				testInScope(() => {
					const id = store.add(makeTerminal());
					// Enqueue a completed command block → schedules an OSC 133 flush rAF.
					store.handleOsc133(id, "A", 1);
					store.handleOsc133(id, "D", 5, 0);
					expect(raf).toHaveBeenCalledTimes(1);
					const pendingFlush = rafCallbacks.get(1);
					expect(pendingFlush).toBeDefined();

					// Remove the terminal while the flush is still pending.
					store.remove(id);
					expect(store.get(id)).toBeUndefined();
					expect(caf).toHaveBeenCalledWith(1);

					// Even if a stale callback fires, the has(id) guard must keep the
					// terminal dead — a setState on a missing key would create a ghost.
					pendingFlush?.(0);
					expect(store.get(id)).toBeUndefined();
				});
			} finally {
				vi.unstubAllGlobals();
			}
		});
	});

	describe("addUserPromptLine()", () => {
		it("appends user-prompt lines in order", () => {
			testInScope(() => {
				const id = store.add(makeTerminal());
				store.addUserPromptLine(id, 10);
				store.addUserPromptLine(id, 42);
				expect(store.get(id)!.userPromptLines).toEqual([10, 42]);
			});
		});

		it("ignores negative lines (keystroke-reconstructed UserInput has no grid row)", () => {
			testInScope(() => {
				const id = store.add(makeTerminal());
				store.addUserPromptLine(id, -1);
				expect(store.get(id)!.userPromptLines).toEqual([]);
			});
		});

		it("dedups a consecutive repeat of the same line (prompt redraw re-fires busy)", () => {
			testInScope(() => {
				const id = store.add(makeTerminal());
				store.addUserPromptLine(id, 7);
				store.addUserPromptLine(id, 7);
				expect(store.get(id)!.userPromptLines).toEqual([7]);
			});
		});

		it("evicts the oldest lines past the cap (mirrors commandBlocks)", () => {
			testInScope(() => {
				const id = store.add(makeTerminal());
				// MAX_BLOCKS is 500; push 501 distinct lines and assert the oldest dropped.
				for (let i = 0; i < 501; i++) store.addUserPromptLine(id, i);
				const lines = store.get(id)!.userPromptLines;
				expect(lines.length).toBe(500);
				expect(lines[0]).toBe(1);
				expect(lines.at(-1)!).toBe(500);
			});
		});
	});

	describe("setActive()", () => {
		it("sets the active terminal", () => {
			testInScope(() => {
				const id = store.add(makeTerminal());
				store.setActive(id);
				expect(store.state.activeId).toBe(id);
			});
		});

		it("clears activity indicator when setting active", () => {
			testInScope(() => {
				const id = store.add(makeTerminal());
				store.update(id, { activity: true });
				store.setActive(id);
				expect(store.get(id)!.activity).toBe(false);
			});
		});

		it("accepts null", () => {
			testInScope(() => {
				store.setActive(null);
				expect(store.state.activeId).toBeNull();
			});
		});
	});

	describe("getActive()", () => {
		it("returns the active terminal", () => {
			testInScope(() => {
				const id = store.add(makeTerminal());
				store.setActive(id);
				expect(store.getActive()?.id).toBe(id);
			});
		});

		it("returns undefined when no active", () => {
			testInScope(() => {
				expect(store.getActive()).toBeUndefined();
			});
		});
	});

	describe("update()", () => {
		it("updates terminal properties", () => {
			testInScope(() => {
				const id = store.add(makeTerminal());
				store.update(id, { name: "Updated", fontSize: 16 });
				expect(store.get(id)!.name).toBe("Updated");
				expect(store.get(id)!.fontSize).toBe(16);
			});
		});
	});

	describe("setSessionId()", () => {
		it("updates session ID", () => {
			testInScope(() => {
				const id = store.add(makeTerminal());
				store.setSessionId(id, "sess-1");
				expect(store.get(id)!.sessionId).toBe("sess-1");
			});
		});
	});

	describe("applyAlias()", () => {
		// Story 761-c847: the backend assigns the alias the instant a PTY spawns,
		// while the frontend still awaits session creation before calling
		// setSessionId — so `term-alias-assigned` can arrive before the session
		// is bound to any terminal. Before this fix, that event was silently
		// dropped because getTerminalForSession(sessionId) returned null.
		it("retains an alias that arrives before setSessionId binds the session, and applies it on bind", () => {
			testInScope(() => {
				const id = store.add(makeTerminal());
				// Alias event races ahead of the bind — no terminal maps to "sess-1" yet.
				store.applyAlias("sess-1", "tc-1");
				expect(store.get(id)!.alias).toBeNull();

				store.setSessionId(id, "sess-1");
				expect(store.get(id)!.alias).toBe("tc-1");
			});
		});

		it("applies immediately when the session is already bound", () => {
			testInScope(() => {
				const id = store.add(makeTerminal());
				store.setSessionId(id, "sess-2");
				store.applyAlias("sess-2", "tc-2");
				expect(store.get(id)!.alias).toBe("tc-2");
			});
		});

		it("applies immediately when bound via add() with a sessionId", () => {
			testInScope(() => {
				store.applyAlias("sess-3", "tc-3");
				const id = store.add(makeTerminal({ sessionId: "sess-3" }));
				expect(store.get(id)!.alias).toBe("tc-3");
			});
		});

		it("applies immediately when bound via register() with a sessionId", () => {
			testInScope(() => {
				store.applyAlias("sess-4", "tc-4");
				store.register("term-fixed", { ...makeTerminal(), sessionId: "sess-4" });
				expect(store.get("term-fixed")!.alias).toBe("tc-4");
			});
		});

		it("does not disturb a persisted alias when no alias event ever arrives for the new session", () => {
			testInScope(() => {
				const id = store.add({ ...makeTerminal(), alias: "persisted-alias" });
				store.setSessionId(id, "sess-5");
				expect(store.get(id)!.alias).toBe("persisted-alias");
			});
		});

		it("a retained alias for one session is not applied to a different terminal's bind", () => {
			testInScope(() => {
				const idA = store.add(makeTerminal({ name: "A" }));
				const idB = store.add(makeTerminal({ name: "B" }));
				store.applyAlias("sess-a", "tc-a");
				store.setSessionId(idB, "sess-b");
				expect(store.get(idB)!.alias).toBeNull();
				store.setSessionId(idA, "sess-a");
				expect(store.get(idA)!.alias).toBe("tc-a");
			});
		});
	});

	describe("setFontSize()", () => {
		it("updates font size", () => {
			testInScope(() => {
				const id = store.add(makeTerminal());
				store.setFontSize(id, 18);
				expect(store.get(id)!.fontSize).toBe(18);
			});
		});

		it("fontSize is accessible via direct store path", () => {
			testInScope(() => {
				const id = store.add(makeTerminal());

				// Verify direct store path access works (used in Terminal.tsx for SolidJS reactivity)
				expect(store.state.terminals[id]?.fontSize).toBe(14);

				store.setFontSize(id, 18);
				expect(store.state.terminals[id]?.fontSize).toBe(18);

				store.setFontSize(id, 22);
				expect(store.state.terminals[id]?.fontSize).toBe(22);

				// Verify it returns undefined for non-existent terminal
				expect(store.state.terminals["nonexistent"]?.fontSize).toBeUndefined();
			});
		});
	});

	describe("awaiting input", () => {
		it("sets awaiting input type", () => {
			testInScope(() => {
				const id = store.add(makeTerminal());
				store.setAwaitingInput(id, "question");
				expect(store.get(id)!.awaitingInput).toBe("question");
			});
		});

		it("clears awaiting input", () => {
			testInScope(() => {
				const id = store.add(makeTerminal({ awaitingInput: "question" }));
				store.clearAwaitingInput(id);
				expect(store.get(id)!.awaitingInput).toBeNull();
			});
		});

		it("hasAwaitingInput returns true when any terminal is awaiting", () => {
			testInScope(() => {
				store.add(makeTerminal({ name: "T1" }));
				const id2 = store.add(makeTerminal({ name: "T2" }));
				store.setAwaitingInput(id2, "error");
				expect(store.hasAwaitingInput()).toBe(true);
			});
		});

		it("hasAwaitingInput returns false when none awaiting", () => {
			testInScope(() => {
				store.add(makeTerminal({ name: "T1" }));
				expect(store.hasAwaitingInput()).toBe(false);
			});
		});

		it("shellState update does not clear awaitingInput (PTY output must not erase question state)", () => {
			testInScope(() => {
				const id = store.add(makeTerminal());
				store.setAwaitingInput(id, "question");
				// Simulate PTY output: shellState goes busy
				store.update(id, { shellState: "busy" });
				// awaitingInput must survive — the question is still pending
				expect(store.get(id)!.awaitingInput).toBe("question");
				// And going back to idle also must not clear it
				store.update(id, { shellState: "idle" });
				expect(store.get(id)!.awaitingInput).toBe("question");
			});
		});

		it("agentType set→null clears awaitingInput (agent exited to a plain shell)", () => {
			testInScope(() => {
				const id = store.add(makeTerminal({ agentType: "claude" }));
				// Confident question: deliberately survives idle↔busy, so only an
				// explicit signal (user-input, process exit, or agent→shell) may clear it.
				store.setAwaitingInput(id, "question", true);
				// Agent process exits; detection flips agentType to null (shell prompt returned).
				store.update(id, { agentType: null });
				expect(store.get(id)!.awaitingInput).toBeNull();
				expect(store.get(id)!.awaitingInputConfident).toBe(false);
			});
		});

		it("agentType null→null does not clear awaitingInput (no agent transition)", () => {
			testInScope(() => {
				// A plain shell that legitimately detected a question (e.g. a script's
				// "Continue? " prompt). No agent→shell transition occurs, so it must survive.
				const id = store.add(makeTerminal({ agentType: null }));
				store.setAwaitingInput(id, "question", true);
				store.update(id, { agentType: null });
				expect(store.get(id)!.awaitingInput).toBe("question");
			});
		});

		it("agentType claude→codex does not clear awaitingInput (still an agent)", () => {
			testInScope(() => {
				const id = store.add(makeTerminal({ agentType: "claude" }));
				store.setAwaitingInput(id, "question", true);
				store.update(id, { agentType: "codex" });
				expect(store.get(id)!.awaitingInput).toBe("question");
			});
		});

		it("getAwaitingInputIds returns correct IDs", () => {
			testInScope(() => {
				const id1 = store.add(makeTerminal({ name: "T1" }));
				const id2 = store.add(makeTerminal({ name: "T2" }));
				store.setAwaitingInput(id2, "error");
				const ids = store.getAwaitingInputIds();
				expect(ids).toContain(id2);
				expect(ids).not.toContain(id1);
			});
		});
	});

	describe("getIds()", () => {
		it("returns all terminal IDs", () => {
			testInScope(() => {
				store.add(makeTerminal({ name: "T1" }));
				store.add(makeTerminal({ name: "T2" }));
				expect(store.getIds()).toHaveLength(2);
			});
		});
	});

	describe("getSubAgentTag()", () => {
		it("returns null for a terminal no agent spawned", () => {
			testInScope(() => {
				const id = store.add(makeTerminal({ name: "Plain", sessionId: "s-plain" }));
				expect(store.getSubAgentTag(id)).toBeNull();
			});
		});

		it("names the parent tab matched by its TUIC_SESSION", () => {
			testInScope(() => {
				store.add(makeTerminal({ name: "Orchestrator", sessionId: "pty-parent", tuicSession: "tuic-parent" }));
				const child = store.add(makeTerminal({ name: "Worker", sessionId: "pty-child", parentSession: "tuic-parent" }));
				expect(store.getSubAgentTag(child)).toBe("Orchestrator");
			});
		});

		it("names the parent tab matched by its PTY session id", () => {
			testInScope(() => {
				// A spawned parent has no frontend tuicSession: its PTY id IS its TUIC_SESSION.
				store.add(makeTerminal({ name: "Spawned lead", sessionId: "pty-lead" }));
				const child = store.add(makeTerminal({ name: "Worker", sessionId: "pty-child", parentSession: "pty-lead" }));
				expect(store.getSubAgentTag(child)).toBe("Spawned lead");
			});
		});

		it("follows a rename of the parent tab", () => {
			testInScope(() => {
				const parent = store.add(makeTerminal({ name: "Before", sessionId: "pty-p" }));
				const child = store.add(makeTerminal({ name: "Worker", sessionId: "pty-c", parentSession: "pty-p" }));
				store.update(parent, { name: "After" });
				expect(store.getSubAgentTag(child)).toBe("After");
			});
		});

		it("still marks the terminal as a sub-agent when the parent has no tab", () => {
			testInScope(() => {
				const child = store.add(makeTerminal({ name: "Worker", sessionId: "pty-c", parentSession: "external-caller" }));
				expect(store.getSubAgentTag(child)).toBe("external agent");
			});
		});
	});

	describe("sessionToTerminal reverse map", () => {
		it("getTerminalForSession returns terminal ID when session is assigned", () => {
			testInScope(() => {
				const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-abc" }));
				expect(store.getTerminalForSession("sess-abc")).toBe(id);
			});
		});

		it("getTerminalForSession returns null for unknown session", () => {
			testInScope(() => {
				store.add(makeTerminal({ name: "T1", sessionId: "sess-abc" }));
				expect(store.getTerminalForSession("not-a-session")).toBeNull();
			});
		});

		it("getTerminalForSession returns null for terminal with null sessionId", () => {
			testInScope(() => {
				store.add(makeTerminal({ name: "T1" }));
				expect(store.getTerminalForSession("sess-xyz")).toBeNull();
			});
		});

		it("map is updated when setSessionId is called", () => {
			testInScope(() => {
				const id = store.add(makeTerminal({ name: "T1" }));
				expect(store.getTerminalForSession("sess-new")).toBeNull();
				store.setSessionId(id, "sess-new");
				expect(store.getTerminalForSession("sess-new")).toBe(id);
			});
		});

		it("map is updated when setSessionId reassigns to a new session", () => {
			testInScope(() => {
				const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-old" }));
				store.setSessionId(id, "sess-new");
				expect(store.getTerminalForSession("sess-old")).toBeNull();
				expect(store.getTerminalForSession("sess-new")).toBe(id);
			});
		});

		it("map entry is removed when terminal is removed", () => {
			testInScope(() => {
				const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-abc" }));
				expect(store.getTerminalForSession("sess-abc")).toBe(id);
				store.remove(id);
				expect(store.getTerminalForSession("sess-abc")).toBeNull();
			});
		});

		it("getAgentTypeForSession uses reverse map for O(1) lookup", () => {
			testInScope(() => {
				const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-agent" }));
				store.update(id, { agentType: "claude" });
				expect(store.getAgentTypeForSession("sess-agent")).toBe("claude");
			});
		});

		it("getAgentTypeForSession returns null for unknown session", () => {
			testInScope(() => {
				store.add(makeTerminal({ name: "T1", sessionId: "sess-agent" }));
				expect(store.getAgentTypeForSession("unknown-sess")).toBeNull();
			});
		});

		// Regression: Terminal.tsx used to call update({ sessionId }) which bypassed
		// the reverse map, breaking plugin filtering (cache-keepalive starved of
		// shell-state events because getAgentTypeForSession returned null).
		it("update({ sessionId }) keeps reverse map in sync", () => {
			testInScope(() => {
				const id = store.add(makeTerminal({ name: "T1", agentType: "claude" }));
				expect(store.getTerminalForSession("sess-late")).toBeNull();
				store.update(id, { sessionId: "sess-late" });
				expect(store.getTerminalForSession("sess-late")).toBe(id);
				expect(store.getAgentTypeForSession("sess-late")).toBe("claude");
			});
		});

		it("update({ sessionId: null }) removes reverse map entry", () => {
			testInScope(() => {
				const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-bye" }));
				store.update(id, { agentState: "working", backgroundWork: true });
				expect(store.getTerminalForSession("sess-bye")).toBe(id);
				store.update(id, { sessionId: null });
				expect(store.getTerminalForSession("sess-bye")).toBeNull();
				expect(store.get(id)?.agentState).toBeNull();
				expect(store.get(id)?.backgroundWork).toBe(false);
			});
		});

		it("update({ sessionId: newId }) replaces prior reverse map entry", () => {
			testInScope(() => {
				const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-old" }));
				store.update(id, { sessionId: "sess-new" });
				expect(store.getTerminalForSession("sess-old")).toBeNull();
				expect(store.getTerminalForSession("sess-new")).toBe(id);
			});
		});

		it("add({ agentType }) exposes agentType via reverse map lookup", () => {
			testInScope(() => {
				const id = store.add(makeTerminal({ name: "T1", sessionId: "sess-restored", agentType: "claude" }));
				expect(store.getAgentTypeForSession("sess-restored")).toBe("claude");
				expect(store.getTerminalForSession("sess-restored")).toBe(id);
			});
		});
	});

	describe("agentType", () => {
		it("initializes agentType as null", () => {
			testInScope(() => {
				const id = store.add(makeTerminal());
				expect(store.get(id)!.agentType).toBeNull();
			});
		});

		it("can be updated to a known agent", () => {
			testInScope(() => {
				const id = store.add(makeTerminal());
				store.update(id, { agentType: "claude" });
				expect(store.get(id)!.agentType).toBe("claude");
			});
		});

		it("can be cleared back to null", () => {
			testInScope(() => {
				const id = store.add(makeTerminal());
				store.update(id, { agentType: "gemini" });
				store.update(id, { agentType: null });
				expect(store.get(id)!.agentType).toBeNull();
			});
		});
	});

	describe("unseen", () => {
		it("initializes unseen as false", () => {
			testInScope(() => {
				const id = store.add(makeTerminal());
				expect(store.get(id)!.unseen).toBe(false);
			});
		});

		it("clears unseen when setting active", () => {
			testInScope(() => {
				const id = store.add(makeTerminal());
				store.update(id, { unseen: true });
				store.setActive(id);
				expect(store.get(id)!.unseen).toBe(false);
			});
		});

		it("setAwaitingInput does NOT set unseen (question dot is sufficient)", () => {
			testInScope(() => {
				const id1 = store.add(makeTerminal({ name: "T1" }));
				const id2 = store.add(makeTerminal({ name: "T2" }));
				store.setActive(id1);
				// Terminal id2 is not active — awaitingInput should NOT set unseen
				// (the orange/red dot already communicates "needs attention")
				store.setAwaitingInput(id2, "question");
				expect(store.get(id2)!.unseen).toBe(false);
			});
		});
	});

	describe("shellState", () => {
		it("initializes shellState as null", () => {
			testInScope(() => {
				const id = store.add(makeTerminal());
				expect(store.get(id)!.shellState).toBeNull();
			});
		});

		it("can be updated to busy", () => {
			testInScope(() => {
				const id = store.add(makeTerminal());
				store.update(id, { shellState: "busy" });
				expect(store.get(id)!.shellState).toBe("busy");
			});
		});

		it("can be updated to idle", () => {
			testInScope(() => {
				const id = store.add(makeTerminal());
				store.update(id, { shellState: "idle" });
				expect(store.get(id)!.shellState).toBe("idle");
			});
		});

		it("clears awaitingInput on idle→busy transition", () => {
			testInScope(() => {
				const id = store.add(makeTerminal());
				store.update(id, { shellState: "idle" });
				store.setAwaitingInput(id, "question");
				expect(store.get(id)!.awaitingInput).toBe("question");
				// Agent resumes output → idle→busy transition should clear stale question
				store.update(id, { shellState: "busy" });
				expect(store.get(id)!.awaitingInput).toBeNull();
			});
		});

		it("preserves a CONFIDENT awaitingInput across idle→busy oscillation", () => {
			testInScope(() => {
				const id = store.add(makeTerminal());
				store.update(id, { shellState: "idle" });
				// Confident prompt (e.g. Ink "Enter to select" menu)
				store.setAwaitingInput(id, "question", true);
				expect(store.get(id)!.awaitingInput).toBe("question");
				// A fullscreen TUI repaint oscillates idle→busy; the confident prompt
				// must survive (it clears only on real user-input or process exit).
				store.update(id, { shellState: "busy" });
				expect(store.get(id)!.awaitingInput).toBe("question");
				expect(store.get(id)!.awaitingInputConfident).toBe(true);
				// A confident question arms the sustained-busy clear timer; cancel it so
				// the real setTimeout doesn't outlive the test (vitest leak warning).
				store.clearAwaitingInput(id);
			});
		});

		it("does not clear awaitingInput on null→busy transition", () => {
			testInScope(() => {
				const id = store.add(makeTerminal());
				store.setAwaitingInput(id, "question");
				// Initial busy (from null) should NOT clear — agent hasn't been idle yet
				store.update(id, { shellState: "busy" });
				expect(store.get(id)!.awaitingInput).toBe("question");
			});
		});

		it("preserves shellState on setActive", () => {
			testInScope(() => {
				const id = store.add(makeTerminal());
				store.update(id, { shellState: "idle" });
				store.setActive(id);
				expect(store.get(id)!.shellState).toBe("idle");
			});
		});
	});

	describe("lastActiveId", () => {
		it("is set when setActive is called with a valid ID", () => {
			testInScope(() => {
				const id = store.add(makeTerminal());
				store.setActive(id);
				expect(store.state.lastActiveId).toBe(id);
			});
		});

		it("persists after setActive(null)", () => {
			testInScope(() => {
				const id = store.add(makeTerminal());
				store.setActive(id);
				store.setActive(null);
				expect(store.state.activeId).toBeNull();
				expect(store.state.lastActiveId).toBe(id);
			});
		});

		it("updates to the latest active terminal", () => {
			testInScope(() => {
				const id1 = store.add(makeTerminal({ name: "T1" }));
				const id2 = store.add(makeTerminal({ name: "T2" }));
				store.setActive(id1);
				store.setActive(id2);
				expect(store.state.lastActiveId).toBe(id2);
			});
		});

		it("is cleared when the lastActive terminal is removed", () => {
			testInScope(() => {
				const id = store.add(makeTerminal());
				store.setActive(id);
				store.setActive(null);
				expect(store.state.lastActiveId).toBe(id);
				store.remove(id);
				expect(store.state.lastActiveId).toBeNull();
			});
		});

		it("is not cleared when a different terminal is removed", () => {
			testInScope(() => {
				const id1 = store.add(makeTerminal({ name: "T1" }));
				const id2 = store.add(makeTerminal({ name: "T2" }));
				store.setActive(id1);
				store.remove(id2);
				expect(store.state.lastActiveId).toBe(id1);
			});
		});

		it("is not set when setActive is called with null and no previous active", () => {
			testInScope(() => {
				store.setActive(null);
				expect(store.state.lastActiveId).toBeNull();
			});
		});
	});

	describe("previousActiveId (return to last terminal)", () => {
		it("is null with no prior active terminal", () => {
			testInScope(() => {
				const id = store.add(makeTerminal());
				store.setActive(id);
				expect(store.getPreviousActiveId()).toBeNull();
			});
		});

		it("remembers the terminal left behind when switching", () => {
			testInScope(() => {
				const id1 = store.add(makeTerminal({ name: "T1" }));
				const id2 = store.add(makeTerminal({ name: "T2" }));
				store.setActive(id1);
				store.setActive(id2);
				expect(store.getPreviousActiveId()).toBe(id1);
			});
		});

		it("toggles between two terminals on repeated switches", () => {
			testInScope(() => {
				const id1 = store.add(makeTerminal({ name: "T1" }));
				const id2 = store.add(makeTerminal({ name: "T2" }));
				store.setActive(id1);
				store.setActive(id2);
				// Return to T1: previous becomes T2.
				store.setActive(id1);
				expect(store.getPreviousActiveId()).toBe(id2);
				// Return to T2: previous becomes T1 again.
				store.setActive(id2);
				expect(store.getPreviousActiveId()).toBe(id1);
			});
		});

		it("does not update when re-activating the already-active terminal", () => {
			testInScope(() => {
				const id1 = store.add(makeTerminal({ name: "T1" }));
				const id2 = store.add(makeTerminal({ name: "T2" }));
				store.setActive(id1);
				store.setActive(id2);
				store.setActive(id2);
				expect(store.getPreviousActiveId()).toBe(id1);
			});
		});

		it("getPreviousActiveId returns null once the previous terminal is removed", () => {
			testInScope(() => {
				const id1 = store.add(makeTerminal({ name: "T1" }));
				const id2 = store.add(makeTerminal({ name: "T2" }));
				store.setActive(id1);
				store.setActive(id2);
				store.remove(id1);
				expect(store.getPreviousActiveId()).toBeNull();
			});
		});
	});

	describe("activation history (back/forward)", () => {
		function addActivated(n: number): string[] {
			const ids: string[] = [];
			for (let i = 0; i < n; i++) {
				const id = store.add(makeTerminal({ name: `T${i}` }));
				store.setActive(id);
				ids.push(id);
			}
			return ids;
		}

		// catches: back navigation reorders the history so back twice returns to the start
		it("history_back_forward_walks_mru_without_reordering", () => {
			testInScope(() => {
				const [a, b, c] = addActivated(3);
				const walk = (dir: "back" | "forward") => {
					const id = store.stepHistory(dir);
					if (id) store.setActive(id);
					return id;
				};
				expect(walk("back")).toBe(b);
				expect(walk("back")).toBe(a);
				expect(walk("back")).toBeNull();
				expect(walk("forward")).toBe(b);
				expect(walk("forward")).toBe(c);
				expect(walk("forward")).toBeNull();
				expect(store.state.history).toEqual([c, b, a]);
			});
		});

		// catches: activating a terminal after stepping back leaves a stale forward part
		it("activating_another_terminal_truncates_the_forward_part", () => {
			testInScope(() => {
				const [a, b, c] = addActivated(3);
				store.setActive(store.stepHistory("back") as string); // b
				const d = store.add(makeTerminal({ name: "D" }));
				store.setActive(d);
				expect(store.getHistoryTarget("forward")).toBeNull();
				expect(store.state.history).toEqual([d, b, a]);
				expect(store.state.history).not.toContain(c);
			});
		});

		// catches: back jumps to a closed terminal
		it("closed_terminal_leaves_history", () => {
			testInScope(() => {
				const [a, b, c] = addActivated(3);
				store.remove(b);
				expect(store.state.history).toEqual([c, a]);
				expect(store.getHistoryTarget("back")).toBe(a);
			});
		});

		// catches: removing an entry before the cursor shifts back/forward onto the wrong terminal
		it("removing_an_entry_ahead_of_the_cursor_keeps_the_current_position", () => {
			testInScope(() => {
				const [a, b, c] = addActivated(3);
				store.setActive(store.stepHistory("back") as string); // b, cursor 1
				store.setActive(store.stepHistory("back") as string); // a, cursor 2
				store.remove(c);
				expect(store.state.history).toEqual([b, a]);
				expect(store.getHistoryTarget("forward")).toBe(b);
				expect(store.getHistoryTarget("back")).toBeNull();
			});
		});

		// catches: unbounded history growth
		it("keeps_only_the_ten_most_recent_distinct_terminals", () => {
			testInScope(() => {
				const ids = addActivated(12);
				expect(store.state.history).toEqual(ids.slice(2).reverse());
				store.setActive(ids[5]);
				expect(store.state.history[0]).toBe(ids[5]);
				expect(new Set(store.state.history).size).toBe(store.state.history.length);
			});
		});
	});

	describe("findTerminalWithSession()", () => {
		it("returns active terminal when it has a session", () => {
			testInScope(() => {
				const id = store.add(makeTerminal({ sessionId: "s1" }));
				store.setActive(id);
				const result = store.findTerminalWithSession();
				expect(result).toEqual({ sessionId: "s1", agentType: null });
			});
		});

		it("falls back to lastActiveId when active has no session", () => {
			testInScope(() => {
				const id1 = store.add(makeTerminal({ name: "T1", sessionId: "s1" }));
				const id2 = store.add(makeTerminal({ name: "T2" }));
				store.setActive(id1);
				store.setActive(id2);
				// id2 is active but has no session, id1 is lastActiveId (from previous setActive)
				// Actually both calls update lastActiveId, so lastActiveId = id2 too.
				// Let's set active to null to use lastActiveId
				store.setActive(id1);
				store.setActive(null);
				// Now activeId=null, lastActiveId=id1 (which has session)
				const result = store.findTerminalWithSession();
				expect(result).toEqual({ sessionId: "s1", agentType: null });
			});
		});

		it("falls back to any terminal with a session", () => {
			testInScope(() => {
				store.add(makeTerminal({ name: "T1" }));
				store.add(makeTerminal({ name: "T2", sessionId: "s2" }));
				// No active terminal set
				const result = store.findTerminalWithSession();
				expect(result?.sessionId).toBe("s2");
			});
		});

		it("returns null when no terminal has a session", () => {
			testInScope(() => {
				store.add(makeTerminal({ name: "T1" }));
				store.add(makeTerminal({ name: "T2" }));
				expect(store.findTerminalWithSession()).toBeNull();
			});
		});

		it("returns null when no terminals exist", () => {
			testInScope(() => {
				expect(store.findTerminalWithSession()).toBeNull();
			});
		});

		it("includes agentType when terminal has one", () => {
			testInScope(() => {
				const id = store.add(makeTerminal({ sessionId: "s1", agentType: "claude" }));
				store.setActive(id);
				const result = store.findTerminalWithSession();
				expect(result).toEqual({ sessionId: "s1", agentType: "claude" });
			});
		});
	});

	describe("getCount()", () => {
		it("returns terminal count", () => {
			testInScope(() => {
				expect(store.getCount()).toBe(0);
				store.add(makeTerminal({ name: "T1" }));
				expect(store.getCount()).toBe(1);
			});
		});
	});

	describe("nextDefaultName()", () => {
		it("returns Terminal 1 when store is empty", () => {
			testInScope(() => {
				expect(store.nextDefaultName()).toBe("Terminal 1");
			});
		});

		it("increments with each added terminal", () => {
			testInScope(() => {
				store.add(makeTerminal({ name: "T1" }));
				expect(store.nextDefaultName()).toBe("Terminal 2");
				store.add(makeTerminal({ name: "T2" }));
				expect(store.nextDefaultName()).toBe("Terminal 3");
			});
		});
	});

	describe("onShellExit / notifyShellExit", () => {
		it("notifyShellExit fires registered callbacks with the terminal id", () => {
			testInScope(() => {
				const callback = vi.fn();
				store.onShellExit(callback);

				store.notifyShellExit("term-1");

				expect(callback).toHaveBeenCalledWith("term-1");
			});
		});

		it("fires every registered callback", () => {
			testInScope(() => {
				const a = vi.fn();
				const b = vi.fn();
				store.onShellExit(a);
				store.onShellExit(b);

				store.notifyShellExit("term-1");

				expect(a).toHaveBeenCalledWith("term-1");
				expect(b).toHaveBeenCalledWith("term-1");
			});
		});

		it("stops firing after unsubscribe", () => {
			testInScope(() => {
				const callback = vi.fn();
				const unsubscribe = store.onShellExit(callback);

				unsubscribe();
				store.notifyShellExit("term-1");

				expect(callback).not.toHaveBeenCalled();
			});
		});
	});
});
