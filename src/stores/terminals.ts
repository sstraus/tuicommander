import { batch } from "solid-js";
import { createStore, produce } from "solid-js/store";
import type { AgentType } from "../agents";
import type { TerminalMatch } from "../types";
import { appLogger } from "./appLogger";
import { rpc } from "../transport";

/** Type of input being awaited */
export type AwaitingInputType = "question" | "error" | null;

/** A completed or in-progress command block detected via OSC 133 shell integration. */
export interface CommandBlock {
  /** Prompt start marker line (OSC 133;A) */
  promptLine: number;
  /** Command text start marker line (OSC 133;B) — set when user finishes typing */
  commandLine: number | null;
  /** Execution start marker line (OSC 133;C) — set when command is submitted */
  executionLine: number | null;
  /** Command end marker line (OSC 133;D) — set when command exits */
  endLine: number | null;
  /** Exit code from OSC 133;D;exitcode */
  exitCode: number | null;
  /** Timestamp when the block started (prompt appeared) */
  startedAt: number;
  /** Timestamp when the command finished */
  endedAt: number | null;
}

/** Shell activity state: null=never had output, busy=producing output, idle=waiting for input, exited=process terminated */
export type ShellState = "busy" | "idle" | "exited" | null;

const VALID_SHELL_STATES = new Set<string>(["busy", "idle", "exited"]);

/** Type guard for ShellState values received from backend */
export function isShellState(value: unknown): value is ShellState {
  return value === null || (typeof value === "string" && VALID_SHELL_STATES.has(value));
}

/** Terminal pane data (without DOM references for serialization) */
export interface TerminalData {
  id: string;
  sessionId: string | null;
  fontSize: number;
  name: string;
  nameIsCustom: boolean; // When true, OSC/status-line title changes are ignored
  cwd: string | null;
  awaitingInput: AwaitingInputType;
  awaitingInputConfident: boolean; // High-confidence detection — don't clear on idle→busy
  activity: boolean;
  unseen: boolean; // Terminal completed work while user wasn't viewing it
  progress: number | null; // OSC 9;4 progress (0-100), null when inactive
  shellState: ShellState;
  agentType: AgentType | null; // Detected foreground agent process (e.g. "claude")
  pendingResumeCommand: string | null; // Set at restore time, consumed on first shell idle
  pendingInitCommand: string | null; // Setup/run script to auto-execute on first shell idle
  usageLimit: { percentage: number; limitType: string } | null; // Claude Code usage limit
  lastDataAt: number | null; // Timestamp of last PTY output
  lastPrompt: string | null; // Last relevant user prompt (>= 10 words), set by Rust
  agentIntent: string | null; // LLM-declared intent via intent: token
  currentTask: string | null; // Current agent task from status-line parsing (e.g. "Reading files")
  activeSubTasks: number; // Count of running sub-agents/background tasks from ›› status line
  isRemote: boolean; // Created via HTTP/MCP (not locally by the UI)
  agentSessionId: string | null; // Agent session ID for session-specific resume (claude, gemini, codex)
  tuicSession: string | null; // Stable tab UUID — injected as TUIC_SESSION env var, persists across restarts
  suggestedActions: string[] | null; // Follow-up suggestions from suggest: token
  suggestDismissed: boolean; // true after user dismissed/selected/typed — resets on shell-state:idle
  commandBlocks: CommandBlock[]; // Completed command blocks from OSC 133
  activeBlock: CommandBlock | null; // Current in-progress block (A received, D not yet)
}

/** Fields auto-populated with defaults when creating a terminal — callers only provide the remaining fields. */
type TerminalCreateData = Omit<TerminalData, "id" | "activity" | "unseen" | "progress" | "shellState" | "nameIsCustom" | "agentType" | "pendingResumeCommand" | "pendingInitCommand" | "usageLimit" | "lastDataAt" | "lastPrompt" | "agentIntent" | "currentTask" | "activeSubTasks" | "isRemote" | "agentSessionId" | "tuicSession" | "suggestedActions" | "suggestDismissed" | "awaitingInputConfident" | "commandBlocks" | "activeBlock"> & { tuicSession?: string | null; isRemote?: boolean; agentType?: AgentType | null; agentSessionId?: string | null };

/** Terminal component ref interface */
export interface TerminalRef {
  fit: () => void;
  write: (data: string) => void;
  writeln: (data: string) => void;
  input: (data: string) => void;
  clear: () => void;
  focus: () => void;
  getSessionId: () => string | null;
  openSearch: () => void;
  closeSearch: () => void;
  /** Search the terminal buffer for a query string (case-insensitive) */
  searchBuffer: (query: string) => TerminalMatch[];
  /** Scroll to an absolute buffer line index (centered in viewport) */
  scrollToLine: (lineIndex: number) => void;
  getSelection: () => string;
  scrollToTop: () => void;
  scrollToBottom: () => void;
  scrollPages: (pages: number) => void;
  /** Read buffer lines between two absolute line indices (exclusive end) */
  getBufferLines: (startLine: number, endLine: number) => string[];
}

/** Combined terminal state */
export interface TerminalState extends TerminalData {
  ref?: TerminalRef;
}

/** Terminals store state */
interface TerminalsStoreState {
  terminals: Record<string, TerminalState>;
  activeId: string | null;
  counter: number;
  /** Tabs currently detached to floating windows: tabId → window label */
  detachedWindows: Record<string, string>;
  /** Debounced busy state per terminal — stays true for BUSY_HOLD_MS after idle */
  debouncedBusy: Record<string, boolean>;
}

/** Debounce hold time: how long isBusy() stays true after shellState goes idle */
const BUSY_HOLD_MS = 2000;

/** Create the terminals store */
function createTerminalsStore() {
  const [state, setState] = createStore<TerminalsStoreState>({
    terminals: {},
    activeId: null,
    counter: 0,
    detachedWindows: {},
    debouncedBusy: {},
  });

  // Reverse map: session_id → terminal_id for O(1) hot-path lookups.
  // Plain JS (not in SolidJS store) — maintained in sync with sessionId field.
  const sessionToTerminal = new Map<string, string>();

  // Debounced busy tracking: timers + timestamps are plain JS, the boolean state
  // lives in the SolidJS store (state.debouncedBusy) for reactivity.
  const busySinceMap = new Map<string, number>();
  const busyDurationMap = new Map<string, number>();
  const cooldownTimers = new Map<string, ReturnType<typeof setTimeout>>();
  const busyToIdleCallbacks: Array<(id: string, durationMs: number) => void> = [];
  const idleToBusyCallbacks: Array<(id: string) => void> = [];
  const onRemoveCallbacks: Array<(id: string) => void> = [];
  // Tracks which terminals have completed their initial shell startup (reached idle at least once).
  // Used to distinguish "busy from .zshrc startup" from "busy from a user-launched process".
  const reachedIdleSet = new Set<string>();

  /** Guard: check terminal exists before mutating. SolidJS setState creates keys
   *  implicitly — calling setState("terminals", id, ...) on a removed terminal
   *  resurrects it as a ghost entry with partial data. */
  function has(id: string): boolean {
    return id in state.terminals;
  }

  /** Handle shellState transition for debounced busy tracking */
  function handleShellStateChange(id: string, prev: ShellState, next: ShellState): void {
    const now = Date.now();
    const dbBusy = state.debouncedBusy[id] ?? false;
    const hasCooldown = cooldownTimers.has(id);
    appLogger.debug("terminal", `[ShellDebounce] ${id} ${prev}→${next} debouncedBusy=${dbBusy} cooldown=${hasCooldown} t=${now % 100000}`);

    if (next === "busy" && prev !== "busy") {
      // Entering busy: clear any cooldown, mark busy, record start time.
      // If a cooldown was active, this is a continuation of the same busy period
      // (e.g. shell prompt redraw after agent exit) — keep the original start time.
      const existingCooldown = cooldownTimers.get(id);
      const hadCooldown = existingCooldown != null;
      if (hadCooldown) {
        clearTimeout(existingCooldown);
        cooldownTimers.delete(id);
        appLogger.debug("terminal", `[ShellDebounce] ${id} cooldown CANCELLED (re-entered busy)`);
      }
      setState("debouncedBusy", id, true);
      if (!hadCooldown) {
        busySinceMap.set(id, Date.now());
        for (const cb of idleToBusyCallbacks) cb(id);
      }
      busyDurationMap.delete(id);
      // Error state is NOT cleared here: API errors are persistent and should only
      // be cleared by explicit agent activity (status-line, user-input) or process exit.
      // Question state IS cleared: idle→busy means the agent resumed work, so any
      // pending question notification is stale. False-positive idle→busy oscillations
      // from spinner ticks are suppressed upstream in Rust (spinner suppression).
      if (prev === "idle" && state.terminals[id]?.awaitingInput) {
        setState("terminals", id, "awaitingInput", null);
        setState("terminals", id, "awaitingInputConfident", false);
      }
    } else if (next === "idle" && prev !== "busy") {
      // Direct null→idle (e.g. Rust sync on tab switch) — mark startup complete
      reachedIdleSet.add(id);
    } else if (next === null && state.terminals[id]?.awaitingInput) {
      // Process exit (shellState reset to null) — clear any stuck error/question
      // badge so the next session doesn't inherit stale state from the last child.
      setState("terminals", id, "awaitingInput", null);
      setState("terminals", id, "awaitingInputConfident", false);
    } else if (next !== "busy" && prev === "busy") {
      // First idle marks shell startup as complete
      if (next === "idle") reachedIdleSet.add(id);
      // Leaving busy: freeze duration, start cooldown
      const since = busySinceMap.get(id);
      const duration = since != null ? Date.now() - since : 0;
      busyDurationMap.set(id, duration);
      appLogger.debug("terminal", `[ShellDebounce] ${id} busy→idle, starting ${BUSY_HOLD_MS}ms cooldown (busyDuration=${duration}ms)`);

      const timer = setTimeout(() => {
        cooldownTimers.delete(id);
        setState("debouncedBusy", id, false);
        appLogger.debug("terminal", `[ShellDebounce] ${id} cooldown EXPIRED → debouncedBusy=false`);
        for (const cb of busyToIdleCallbacks) cb(id, duration);
      }, BUSY_HOLD_MS);
      cooldownTimers.set(id, timer);
    }
  }

  // Non-reactive map for lastDataAt timestamps — avoids triggering the reactive
  // graph on every PTY output (was 1 store write per second per active terminal).
  // ActivityDashboard reads via getLastDataAt(); flushLastDataAt() syncs to store.
  const lastDataAtMap = new Map<string, number>();
  let lastDataAtFlushTimer: ReturnType<typeof setInterval> | null = null;

  function startLastDataAtFlush(): void {
    if (lastDataAtFlushTimer) return;
    lastDataAtFlushTimer = setInterval(() => {
      // Stop the interval once no terminal is pushing lastDataAt updates.
      // A fresh push via setLastDataAt() restarts it. Prevents a 5s timer from
      // ticking forever after the user closes every terminal.
      if (lastDataAtMap.size === 0) {
        if (lastDataAtFlushTimer) {
          clearInterval(lastDataAtFlushTimer);
          lastDataAtFlushTimer = null;
        }
        return;
      }
      batch(() => {
        for (const [id, ts] of lastDataAtMap) {
          if (state.terminals[id]) setState("terminals", id, "lastDataAt", ts);
        }
      });
    }, 5000);
  }

  /** Clean up debounced busy state for a removed terminal */
  function cleanupBusyState(id: string): void {
    const timer = cooldownTimers.get(id);
    if (timer != null) clearTimeout(timer);
    cooldownTimers.delete(id);
    reachedIdleSet.delete(id);
    setState(produce((s) => { delete s.debouncedBusy[id]; }));
    busySinceMap.delete(id);
    busyDurationMap.delete(id);
  }

  const actions = {
    /** Add a new terminal */
    add(data: TerminalCreateData): string {
      const id = `term-${state.counter + 1}`;
      setState("counter", (c) => c + 1);
      setState("terminals", id, { id, activity: false, unseen: false, progress: null, shellState: null, nameIsCustom: false, agentType: null, pendingResumeCommand: null, pendingInitCommand: null, usageLimit: null, lastDataAt: null, lastPrompt: null, agentIntent: null, currentTask: null, activeSubTasks: 0, isRemote: false, agentSessionId: null, tuicSession: null, suggestedActions: null, suggestDismissed: false, awaitingInputConfident: false, commandBlocks: [], activeBlock: null, ...data });
      if (data.sessionId) sessionToTerminal.set(data.sessionId, id);
      return id;
    },

    /** Register a terminal with a specific ID (used by floating windows to reconnect to existing PTY sessions) */
    register(id: string, data: TerminalCreateData): void {
      setState("terminals", id, { id, activity: false, unseen: false, progress: null, shellState: null, nameIsCustom: false, agentType: null, pendingResumeCommand: null, pendingInitCommand: null, usageLimit: null, lastDataAt: null, lastPrompt: null, agentIntent: null, currentTask: null, activeSubTasks: 0, isRemote: false, agentSessionId: null, tuicSession: null, suggestedActions: null, suggestDismissed: false, awaitingInputConfident: false, commandBlocks: [], activeBlock: null, ...data });
      if (data.sessionId) sessionToTerminal.set(data.sessionId, id);
    },

    /** Remove a terminal. Sets activeId to null when removing the active terminal —
     *  the caller is responsible for selecting a same-branch replacement beforehand. */
    remove(id: string): void {
      appLogger.info("terminal", `TermStore.remove(${id})`, { remaining: Object.keys(state.terminals).filter(k => k !== id) });
      const sessionId = state.terminals[id]?.sessionId;
      if (sessionId) sessionToTerminal.delete(sessionId);
      cleanupBusyState(id);
      lastDataAtMap.delete(id);
      for (const cb of onRemoveCallbacks) cb(id);
      setState(
        produce((s) => {
          delete s.terminals[id];
          if (s.activeId === id) {
            s.activeId = null;
          }
        })
      );
    },

    /** Set the active terminal (clears unread activity indicator, preserves shell state) */
    setActive(id: string | null): void {
      if (id) {
        if (!state.terminals[id]) {
          appLogger.warn("terminal", `setActive(${id}) — terminal not in store, ignoring`);
          return;
        }
        appLogger.debug("terminal", `setActive(${id})`, { shellState: state.terminals[id]?.shellState });
      }
      batch(() => {
        if (id) {
          setState("terminals", id, "activity", false);
          setState("terminals", id, "unseen", false);
        }
        setState("activeId", id);
      });
    },

    /** Update terminal data */
    update(id: string, data: Partial<TerminalState>): void {
      if (!state.terminals[id]) {
        appLogger.warn("terminal", `update(${id}) — terminal not in store, ignoring`);
        return;
      }
      batch(() => {
        if ("shellState" in data) {
          const prev = state.terminals[id]?.shellState ?? null;
          const next = data.shellState ?? null;
          if (prev !== next) handleShellStateChange(id, prev, next);
        }
        // Keep sessionToTerminal reverse map in sync: callers that pass sessionId
        // via update() would otherwise desync the map and break plugin filtering
        // (pluginMatchesSession → getAgentTypeForSession → null → plugin starved).
        if ("sessionId" in data) {
          const prev = state.terminals[id]?.sessionId;
          if (prev) sessionToTerminal.delete(prev);
          const next = data.sessionId ?? null;
          if (next) sessionToTerminal.set(next, id);
        }
        setState("terminals", id, data);
      });
      // Sync display name to backend so PWA session list can show it
      if ("name" in data) {
        const sessionId = state.terminals[id]?.sessionId;
        if (sessionId) {
          rpc("set_session_name", { sessionId, name: data.name ?? null }).catch(() => {});
        }
      }
    },

    /** Update session ID */
    setSessionId(id: string, sessionId: string | null): void {
      if (!has(id)) return;
      const prev = state.terminals[id]?.sessionId;
      if (prev) sessionToTerminal.delete(prev);
      if (sessionId) sessionToTerminal.set(sessionId, id);
      setState("terminals", id, "sessionId", sessionId);
    },

    /** Update last relevant user prompt */
    setLastPrompt(id: string, prompt: string | null): void {
      if (!has(id)) return;
      setState("terminals", id, "lastPrompt", prompt);
    },

    /** Set suggested follow-up actions (timer-free — overlay handles visibility timeout) */
    setSuggestedActions(id: string, items: string[]): void {
      if (!has(id)) return;
      setState("terminals", id, "suggestedActions", items);
    },

    /** Dismiss suggested actions for a specific terminal */
    dismissSuggestedActions(id: string): void {
      if (!has(id)) return;
      setState("terminals", id, "suggestedActions", null);
      setState("terminals", id, "suggestDismissed", true);
    },

    /** OSC 133: Handle shell integration marker.
     *  A=prompt start, B=command start, C=pre-execution, D=command finished.
     *  `line` is the absolute buffer line (baseY + cursorY) when the marker was processed. */
    handleOsc133(id: string, type: string, line: number, exitCode?: number): void {
      const term = state.terminals[id];
      if (!term) return;
      const now = Date.now();

      switch (type) {
        case "A": {
          // Prompt start — begin a new block. If there's already an active block
          // without a D marker (e.g. Ctrl+C), finalize it first.
          if (term.activeBlock) {
            const completed: CommandBlock = { ...term.activeBlock, endedAt: now };
            setState("terminals", id, "commandBlocks", (prev) => [...prev, completed]);
          }
          setState("terminals", id, "activeBlock", {
            promptLine: line,
            commandLine: null,
            executionLine: null,
            endLine: null,
            exitCode: null,
            startedAt: now,
            endedAt: null,
          });
          break;
        }
        case "B": {
          if (term.activeBlock) {
            setState("terminals", id, "activeBlock", { ...term.activeBlock, commandLine: line });
          }
          break;
        }
        case "C": {
          if (term.activeBlock) {
            setState("terminals", id, "activeBlock", { ...term.activeBlock, executionLine: line });
          }
          break;
        }
        case "D": {
          if (term.activeBlock) {
            const completed: CommandBlock = {
              ...term.activeBlock,
              endLine: line,
              exitCode: exitCode ?? null,
              endedAt: now,
            };
            batch(() => {
              setState("terminals", id, "commandBlocks", (prev) => [...prev, completed]);
              setState("terminals", id, "activeBlock", null);
            });
            appLogger.debug("terminal", `[OSC133] ${id} block completed, exit=${exitCode ?? "?"}, blocks=${term.commandBlocks.length + 1}`);
          }
          break;
        }
      }
    },

    /** Update agent-declared intent (via intent: token) */
    setAgentIntent(id: string, intent: string | null): void {
      if (!has(id)) return;
      setState("terminals", id, "agentIntent", intent);
    },

    /** Update font size (zoom) */
    setFontSize(id: string, fontSize: number): void {
      if (!has(id)) return;
      setState("terminals", id, "fontSize", fontSize);
    },

    /** Set terminal awaiting input state */
    setAwaitingInput(id: string, type: AwaitingInputType, confident = false): void {
      if (!has(id)) return;
      const prev = state.terminals[id]?.awaitingInput;
      appLogger.debug("terminal", `setAwaitingInput(${id}) "${prev}" → "${type}" confident=${confident}`);
      batch(() => {
        setState("terminals", id, "awaitingInput", type);
        setState("terminals", id, "awaitingInputConfident", confident);
      });
    },

    /** Clear terminal awaiting input state */
    clearAwaitingInput(id: string): void {
      if (!has(id)) return;
      const prev = state.terminals[id]?.awaitingInput;
      if (prev) appLogger.debug("terminal", `clearAwaitingInput(${id}) was "${prev}" → null`);
      batch(() => {
        setState("terminals", id, "awaitingInput", null);
        setState("terminals", id, "awaitingInputConfident", false);
      });
    },

    /** Check if any terminal is awaiting input */
    hasAwaitingInput(): boolean {
      return Object.values(state.terminals).some((t) => t.awaitingInput !== null);
    },

    /** Get all terminals awaiting input */
    getAwaitingInputIds(): string[] {
      return Object.entries(state.terminals)
        .filter(([_, t]) => t.awaitingInput !== null)
        .map(([id]) => id);
    },

    /** Get terminal by ID */
    get(id: string): TerminalState | undefined {
      return state.terminals[id];
    },

    /** Get the terminal ID for a PTY session, or null if not found */
    getTerminalForSession(sessionId: string): string | null {
      return sessionToTerminal.get(sessionId) ?? null;
    },

    /** Get the agentType for a PTY session, or null if not found */
    getAgentTypeForSession(sessionId: string): string | null {
      const termId = sessionToTerminal.get(sessionId);
      if (!termId) return null;
      return state.terminals[termId]?.agentType ?? null;
    },

    /** Get active terminal */
    getActive(): TerminalState | undefined {
      return state.activeId ? state.terminals[state.activeId] : undefined;
    },

    /** Get all terminal IDs */
    getIds(): string[] {
      return Object.keys(state.terminals);
    },

    /** Get terminal count */
    getCount(): number {
      return Object.keys(state.terminals).length;
    },

    /** Debounced busy: true while shellState is "busy" and for 2s after it transitions to idle */
    isBusy(id: string): boolean {
      return state.debouncedBusy[id] ?? false;
    },

    /** True if the shell has completed its initial startup (reached idle at least once).
     *  Used to distinguish "busy from .zshrc startup" from "busy from a user-launched process". */
    hasReachedIdle(id: string): boolean {
      return reachedIdleSet.has(id);
    },

    /** True if any terminal has a debounced busy state */
    isAnyBusy(): boolean {
      return Object.values(state.debouncedBusy).some(Boolean);
    },

    /** Duration in ms of the current (or last) busy cycle. 0 if never busy. */
    getBusyDuration(id: string): number {
      // If currently busy (no frozen duration yet), compute live
      const since = busySinceMap.get(id);
      const frozen = busyDurationMap.get(id);
      if (frozen != null) return frozen;
      if (since != null) return Date.now() - since;
      return 0;
    },

    /** Register a callback fired when a terminal transitions from debounced-busy to idle.
     *  Callback receives (terminalId, busyDurationMs). */
    onBusyToIdle(callback: (id: string, durationMs: number) => void): () => void {
      busyToIdleCallbacks.push(callback);
      return () => {
        const idx = busyToIdleCallbacks.indexOf(callback);
        if (idx >= 0) busyToIdleCallbacks.splice(idx, 1);
      };
    },

    /** Register a callback fired when a terminal transitions from idle to busy (debounced).
     *  Only fires on genuine new busy cycles — not cooldown re-entries. */
    onIdleToBusy(callback: (id: string) => void): () => void {
      idleToBusyCallbacks.push(callback);
      return () => {
        const idx = idleToBusyCallbacks.indexOf(callback);
        if (idx >= 0) idleToBusyCallbacks.splice(idx, 1);
      };
    },

    /** Register a callback fired when a terminal is removed.
     *  Used by globalWorkspaceStore to auto-unpromote without direct coupling. */
    onRemove(callback: (id: string) => void): () => void {
      onRemoveCallbacks.push(callback);
      return () => {
        const idx = onRemoveCallbacks.indexOf(callback);
        if (idx >= 0) onRemoveCallbacks.splice(idx, 1);
      };
    },

    /** Mark a tab as detached to a floating window */
    detach(tabId: string, windowLabel: string): void {
      setState("detachedWindows", tabId, windowLabel);
    },

    /** Un-mark a tab as detached (called when floating window closes) */
    reattach(tabId: string): void {
      setState(
        produce((s) => {
          delete s.detachedWindows[tabId];
        }),
      );
    },

    /** Check if a tab is currently detached */
    isDetached(tabId: string): boolean {
      return tabId in state.detachedWindows;
    },

    /** Get all non-detached terminal IDs */
    getAttachedIds(): string[] {
      return Object.keys(state.terminals).filter((id) => !(id in state.detachedWindows));
    },

    /** Record last PTY output timestamp without triggering reactive graph */
    touchLastDataAt(id: string, ts: number): void {
      lastDataAtMap.set(id, ts);
      startLastDataAtFlush();
    },

    /** Read last PTY output timestamp (non-reactive, for ActivityDashboard) */
    getLastDataAt(id: string): number | null {
      return lastDataAtMap.get(id) ?? state.terminals[id]?.lastDataAt ?? null;
    },

    /** Flush all pending lastDataAt values to the reactive store */
    flushLastDataAt(): void {
      if (lastDataAtMap.size === 0) return;
      batch(() => {
        for (const [id, ts] of lastDataAtMap) {
          if (state.terminals[id]) setState("terminals", id, "lastDataAt", ts);
        }
      });
    },
  };

  return { state, ...actions };
}

export const terminalsStore = createTerminalsStore();
