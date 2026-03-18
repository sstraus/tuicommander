import { batch } from "solid-js";
import { createStore, produce } from "solid-js/store";
import type { AgentType } from "../agents";
import { appLogger } from "./appLogger";
import { rpc } from "../transport";

/** Type of input being awaited */
export type AwaitingInputType = "question" | "error" | null;

/** Shell activity state: null=never had output, busy=producing output, idle=waiting for input */
export type ShellState = "busy" | "idle" | null;

const VALID_SHELL_STATES = new Set<string>(["busy", "idle"]);

/** Type guard for ShellState values received from backend */
export function isShellState(value: unknown): value is ShellState {
  return value === null || (typeof value === "string" && VALID_SHELL_STATES.has(value));
}

/** Split direction for terminal panes */
export type SplitDirection = "none" | "vertical" | "horizontal";

/** Maximum number of panes in a split layout */
export const MAX_SPLIT_PANES = 6;

/** Minimum fraction a pane can occupy (prevents invisible panes during drag) */
export const MIN_PANE_FRACTION = 0.05;

/** Layout state for a tab — supports single pane or N-way split */
export interface TabLayout {
  direction: SplitDirection;
  panes: string[]; // 0..N terminal IDs
  ratios: number[]; // N fractions summing to 1.0 (length === panes.length when split, [] when none)
  activePaneIndex: number; // 0..N-1
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
  agentIntent: string | null; // LLM-declared intent via [[intent: ...]] token
  currentTask: string | null; // Current agent task from status-line parsing (e.g. "Reading files")
  activeSubTasks: number; // Count of running sub-agents/background tasks from ›› status line
  isRemote: boolean; // Created via HTTP/MCP (not locally by the UI)
  agentSessionId: string | null; // Agent session ID for session-specific resume (claude, gemini, codex)
  tuicSession: string | null; // Stable tab UUID — injected as TUIC_SESSION env var, persists across restarts
  suggestedActions: string[] | null; // Follow-up suggestions from [[suggest: ...]] tokens
  suggestDismissed: boolean; // true after user dismissed/selected — prevents re-show until next user-input
}

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
}

/** Combined terminal state */
export interface TerminalState extends TerminalData {
  ref?: TerminalRef;
}

/** Default layout — single pane, no split */
export const DEFAULT_LAYOUT: TabLayout = {
  direction: "none",
  panes: [],
  ratios: [],
  activePaneIndex: 0,
};

/** Terminals store state */
interface TerminalsStoreState {
  terminals: Record<string, TerminalState>;
  activeId: string | null;
  counter: number;
  layout: TabLayout;
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
    layout: { ...DEFAULT_LAYOUT },
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

  /** Handle shellState transition for debounced busy tracking */
  function handleShellStateChange(id: string, prev: ShellState, next: ShellState): void {
    if (next === "busy" && prev !== "busy") {
      // Entering busy: clear any cooldown, mark busy, record start time.
      // If a cooldown was active, this is a continuation of the same busy period
      // (e.g. shell prompt redraw after agent exit) — keep the original start time.
      const existingCooldown = cooldownTimers.get(id);
      const hadCooldown = existingCooldown != null;
      if (hadCooldown) {
        clearTimeout(existingCooldown);
        cooldownTimers.delete(id);
      }
      setState("debouncedBusy", id, true);
      if (!hadCooldown) {
        busySinceMap.set(id, Date.now());
      }
      busyDurationMap.delete(id);
      // Agent resumed output after being idle — clear question state immediately.
      // Error state is NOT cleared here: API errors are persistent and should only
      // be cleared by explicit agent activity (status-line, user-input) or process exit.
      if (prev === "idle" && state.terminals[id]?.awaitingInput === "question") {
        terminalsStore.clearAwaitingInput(id);
      }
    } else if (next !== "busy" && prev === "busy") {
      // Leaving busy: freeze duration, start cooldown
      const since = busySinceMap.get(id);
      const duration = since != null ? Date.now() - since : 0;
      busyDurationMap.set(id, duration);

      const timer = setTimeout(() => {
        cooldownTimers.delete(id);
        setState("debouncedBusy", id, false);
        for (const cb of busyToIdleCallbacks) cb(id, duration);
      }, BUSY_HOLD_MS);
      cooldownTimers.set(id, timer);
    }
  }

  /** Clean up debounced busy state for a removed terminal */
  function cleanupBusyState(id: string): void {
    const timer = cooldownTimers.get(id);
    if (timer != null) clearTimeout(timer);
    cooldownTimers.delete(id);
    setState(produce((s) => { delete s.debouncedBusy[id]; }));
    busySinceMap.delete(id);
    busyDurationMap.delete(id);
  }

  const actions = {
    /** Add a new terminal */
    add(data: Omit<TerminalData, "id" | "activity" | "unseen" | "progress" | "shellState" | "nameIsCustom" | "agentType" | "pendingResumeCommand" | "pendingInitCommand" | "usageLimit" | "lastDataAt" | "lastPrompt" | "agentIntent" | "currentTask" | "activeSubTasks" | "isRemote" | "agentSessionId" | "tuicSession" | "suggestedActions" | "suggestDismissed" | "awaitingInputConfident"> & { tuicSession?: string | null } & { isRemote?: boolean }): string {
      const id = `term-${state.counter + 1}`;
      setState("counter", (c) => c + 1);
      setState("terminals", id, { id, activity: false, unseen: false, progress: null, shellState: null, nameIsCustom: false, agentType: null, pendingResumeCommand: null, pendingInitCommand: null, usageLimit: null, lastDataAt: null, lastPrompt: null, agentIntent: null, currentTask: null, activeSubTasks: 0, isRemote: false, agentSessionId: null, tuicSession: null, suggestedActions: null, suggestDismissed: false, awaitingInputConfident: false, ...data });
      if (data.sessionId) sessionToTerminal.set(data.sessionId, id);
      return id;
    },

    /** Register a terminal with a specific ID (used by floating windows to reconnect to existing PTY sessions) */
    register(id: string, data: Omit<TerminalData, "id" | "activity" | "unseen" | "progress" | "shellState" | "nameIsCustom" | "agentType" | "pendingResumeCommand" | "pendingInitCommand" | "usageLimit" | "lastDataAt" | "lastPrompt" | "agentIntent" | "currentTask" | "activeSubTasks" | "isRemote" | "agentSessionId" | "tuicSession" | "suggestedActions" | "suggestDismissed" | "awaitingInputConfident"> & { tuicSession?: string | null } & { isRemote?: boolean }): void {
      setState("terminals", id, { id, activity: false, unseen: false, progress: null, shellState: null, nameIsCustom: false, agentType: null, pendingResumeCommand: null, pendingInitCommand: null, usageLimit: null, lastDataAt: null, lastPrompt: null, agentIntent: null, currentTask: null, activeSubTasks: 0, isRemote: false, agentSessionId: null, tuicSession: null, suggestedActions: null, suggestDismissed: false, awaitingInputConfident: false, ...data });
      if (data.sessionId) sessionToTerminal.set(data.sessionId, id);
    },

    /** Remove a terminal. Sets activeId to null when removing the active terminal —
     *  the caller is responsible for selecting a same-branch replacement beforehand. */
    remove(id: string): void {
      appLogger.info("terminal", `TermStore.remove(${id})`, { remaining: Object.keys(state.terminals).filter(k => k !== id) });
      const sessionId = state.terminals[id]?.sessionId;
      if (sessionId) sessionToTerminal.delete(sessionId);
      cleanupBusyState(id);
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
      batch(() => {
        if ("shellState" in data) {
          const prev = state.terminals[id]?.shellState ?? null;
          const next = data.shellState ?? null;
          if (prev !== next) handleShellStateChange(id, prev, next);
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
      const prev = state.terminals[id]?.sessionId;
      if (prev) sessionToTerminal.delete(prev);
      if (sessionId) sessionToTerminal.set(sessionId, id);
      setState("terminals", id, "sessionId", sessionId);
    },

    /** Update last relevant user prompt */
    setLastPrompt(id: string, prompt: string | null): void {
      setState("terminals", id, "lastPrompt", prompt);
    },

    /** Set suggested follow-up actions (timer-free — overlay handles visibility timeout) */
    setSuggestedActions(id: string, items: string[]): void {
      setState("terminals", id, "suggestedActions", items);
    },

    /** Dismiss suggested actions for a specific terminal */
    dismissSuggestedActions(id: string): void {
      setState("terminals", id, "suggestedActions", null);
      setState("terminals", id, "suggestDismissed", true);
    },

    /** Update agent-declared intent (via [[intent: ...]] token) */
    setAgentIntent(id: string, intent: string | null): void {
      setState("terminals", id, "agentIntent", intent);
    },

    /** Update font size (zoom) */
    setFontSize(id: string, fontSize: number): void {
      setState("terminals", id, "fontSize", fontSize);
    },

    /** Set terminal awaiting input state */
    setAwaitingInput(id: string, type: AwaitingInputType, confident = false): void {
      const prev = state.terminals[id]?.awaitingInput;
      appLogger.debug("terminal", `setAwaitingInput(${id}) "${prev}" → "${type}" confident=${confident}`);
      batch(() => {
        setState("terminals", id, "awaitingInput", type);
        setState("terminals", id, "awaitingInputConfident", confident);
      });
    },

    /** Clear terminal awaiting input state */
    clearAwaitingInput(id: string): void {
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

    /** Set the complete layout state */
    setLayout(layout: TabLayout): void {
      setState("layout", layout);
    },

    /** Split the current pane in a given direction, returns new terminal ID or null */
    splitPane(direction: "vertical" | "horizontal"): string | null {
      const { panes } = state.layout;
      if (panes.length === 0) return null;

      // Reject opposite direction when already split
      if (state.layout.direction !== "none" && state.layout.direction !== direction) return null;

      // Reject if at max panes
      if (panes.length >= MAX_SPLIT_PANES) return null;

      const sourceId = panes[state.layout.activePaneIndex] ?? panes[0];
      const source = state.terminals[sourceId];
      const cwd = source?.cwd ?? null;

      const newId = actions.add({
        sessionId: null,
        fontSize: source?.fontSize ?? 14,
        name: `Split ${state.counter + 1}`,
        cwd,
        awaitingInput: null,
      });

      const newPanes = [...panes, newId];
      const n = newPanes.length;
      const equalRatio = 1 / n;
      const ratios = newPanes.map((_, i) =>
        i === n - 1 ? 1 - equalRatio * (n - 1) : equalRatio
      );

      setState("layout", {
        direction,
        panes: newPanes,
        ratios,
        activePaneIndex: n - 1,
      });

      return newId;
    },

    /** Close a split pane by index. Collapses to "none" when only 1 pane remains. */
    closeSplitPane(index: number): void {
      if (state.layout.direction === "none") return;
      const { panes, ratios } = state.layout;
      if (index < 0 || index >= panes.length) return;

      const newPanes = panes.filter((_, i) => i !== index);

      if (newPanes.length <= 1) {
        // Collapse to single pane
        setState("layout", {
          direction: "none",
          panes: newPanes,
          ratios: [],
          activePaneIndex: 0,
        });
        return;
      }

      // Redistribute ratios proportionally among remaining panes
      const remainingRatios = ratios.filter((_, i) => i !== index);
      const remainingSum = remainingRatios.reduce((a, b) => a + b, 0);
      const newRatios = remainingSum > 0
        ? remainingRatios.map(r => r / remainingSum)
        : remainingRatios.map(() => 1 / newPanes.length);
      // Normalize last element to guarantee sum === 1.0
      const sum = newRatios.reduce((a, b) => a + b, 0);
      if (sum > 0 && Math.abs(sum - 1) > Number.EPSILON) {
        newRatios[newRatios.length - 1] += 1 - sum;
      }

      // Adjust activePaneIndex based on which pane was removed
      let newActive = state.layout.activePaneIndex;
      if (index < newActive) {
        newActive -= 1;  // pane before active was removed, shift left
      } else if (index === newActive) {
        newActive = Math.min(newActive, newPanes.length - 1);  // active pane removed, clamp
      }
      // If index > newActive, no adjustment needed

      setState("layout", {
        direction: state.layout.direction,
        panes: newPanes,
        ratios: newRatios,
        activePaneIndex: newActive,
      });
    },

    /** Adjust the boundary between pane[handleIndex] and pane[handleIndex+1].
     *  newBoundary is the cumulative fraction at the handle position (0..1). */
    setHandleRatio(handleIndex: number, newBoundary: number): void {
      const { ratios } = state.layout;
      if (handleIndex < 0 || handleIndex >= ratios.length - 1) return;

      // Compute cumulative sum up to handleIndex (the left edge of pane[handleIndex])
      let leftEdge = 0;
      for (let i = 0; i < handleIndex; i++) leftEdge += ratios[i];
      const rightEdge = leftEdge + ratios[handleIndex] + ratios[handleIndex + 1];

      // Clamp boundary so each pane gets at least MIN_PANE_FRACTION
      const clampedBoundary = Math.max(leftEdge + MIN_PANE_FRACTION, Math.min(rightEdge - MIN_PANE_FRACTION, newBoundary));

      const newRatios = [...ratios];
      newRatios[handleIndex] = clampedBoundary - leftEdge;
      newRatios[handleIndex + 1] = rightEdge - clampedBoundary;
      setState("layout", "ratios", newRatios);
    },

    /** Set the active pane index (clamped to valid range) */
    setActivePaneIndex(index: number): void {
      const maxIndex = Math.max(0, state.layout.panes.length - 1);
      setState("layout", "activePaneIndex", Math.max(0, Math.min(index, maxIndex)));
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
  };

  return { state, ...actions };
}

export const terminalsStore = createTerminalsStore();
