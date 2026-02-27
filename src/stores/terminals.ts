import { createStore, produce } from "solid-js/store";
import type { AgentType } from "../agents";
import { appLogger } from "./appLogger";

/** Type of input being awaited */
export type AwaitingInputType = "question" | "error" | "confirmation" | null;

/** Shell activity state: null=never had output, busy=producing output, idle=waiting for input */
export type ShellState = "busy" | "idle" | null;

/** Split direction for terminal panes */
export type SplitDirection = "none" | "vertical" | "horizontal";

/** Layout state for a tab — supports single pane or a 2-pane split */
export interface TabLayout {
  direction: SplitDirection;
  panes: string[]; // 0, 1, or 2 terminal IDs
  ratio: number; // 0.0–1.0, first pane gets this fraction (default 0.5)
  activePaneIndex: 0 | 1;
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
  activity: boolean;
  progress: number | null; // OSC 9;4 progress (0-100), null when inactive
  shellState: ShellState;
  agentType: AgentType | null; // Detected foreground agent process (e.g. "claude")
  pendingResumeCommand: string | null; // Set at restore time, consumed on first shell idle
  usageLimit: { percentage: number; limitType: string } | null; // Claude Code usage limit
  lastDataAt: number | null; // Timestamp of last PTY output
  lastPrompt: string | null; // Last relevant user prompt (>= 10 words), set by Rust
  agentIntent: string | null; // LLM-declared intent via [[intent: ...]] token
}

/** Terminal component ref interface */
export interface TerminalRef {
  fit: () => void;
  write: (data: string) => void;
  writeln: (data: string) => void;
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
  ratio: 0.5,
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

  // Debounced busy tracking: timers + timestamps are plain JS, the boolean state
  // lives in the SolidJS store (state.debouncedBusy) for reactivity.
  const busySinceMap = new Map<string, number>();
  const busyDurationMap = new Map<string, number>();
  const cooldownTimers = new Map<string, ReturnType<typeof setTimeout>>();
  const busyToIdleCallbacks: Array<(id: string, durationMs: number) => void> = [];

  /** Handle shellState transition for debounced busy tracking */
  function handleShellStateChange(id: string, prev: ShellState, next: ShellState): void {
    if (next === "busy" && prev !== "busy") {
      // Entering busy: clear any cooldown, mark busy, record start time
      const timer = cooldownTimers.get(id);
      if (timer != null) {
        clearTimeout(timer);
        cooldownTimers.delete(id);
      }
      setState("debouncedBusy", id, true);
      busySinceMap.set(id, Date.now());
      busyDurationMap.delete(id);
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
    add(data: Omit<TerminalData, "id" | "activity" | "progress" | "shellState" | "nameIsCustom" | "agentType" | "pendingResumeCommand" | "usageLimit" | "lastDataAt" | "lastPrompt" | "agentIntent">): string {
      const id = `term-${state.counter + 1}`;
      setState("counter", (c) => c + 1);
      setState("terminals", id, { id, activity: false, progress: null, shellState: null, nameIsCustom: false, agentType: null, pendingResumeCommand: null, usageLimit: null, lastDataAt: null, lastPrompt: null, agentIntent: null, ...data });
      return id;
    },

    /** Register a terminal with a specific ID (used by floating windows to reconnect to existing PTY sessions) */
    register(id: string, data: Omit<TerminalData, "id" | "activity" | "progress" | "shellState" | "nameIsCustom" | "agentType" | "pendingResumeCommand" | "usageLimit" | "lastDataAt" | "lastPrompt" | "agentIntent">): void {
      setState("terminals", id, { id, activity: false, progress: null, shellState: null, nameIsCustom: false, agentType: null, pendingResumeCommand: null, usageLimit: null, lastDataAt: null, lastPrompt: null, agentIntent: null, ...data });
    },

    /** Remove a terminal. Sets activeId to null when removing the active terminal —
     *  the caller is responsible for selecting a same-branch replacement beforehand. */
    remove(id: string): void {
      appLogger.info("terminal", `TermStore.remove(${id})`, { remaining: Object.keys(state.terminals).filter(k => k !== id) });
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
        setState("terminals", id, "activity", false);
      }
      setState("activeId", id);
    },

    /** Update terminal data */
    update(id: string, data: Partial<TerminalState>): void {
      if ("shellState" in data) {
        const prev = state.terminals[id]?.shellState ?? null;
        const next = data.shellState ?? null;
        if (prev !== next) handleShellStateChange(id, prev, next);
      }
      setState("terminals", id, (prev) => ({ ...prev, ...data }));
    },

    /** Update session ID */
    setSessionId(id: string, sessionId: string | null): void {
      setState("terminals", id, "sessionId", sessionId);
    },

    /** Update last relevant user prompt */
    setLastPrompt(id: string, prompt: string | null): void {
      setState("terminals", id, "lastPrompt", prompt);
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
    setAwaitingInput(id: string, type: AwaitingInputType): void {
      const prev = state.terminals[id]?.awaitingInput;
      appLogger.debug("terminal", `setAwaitingInput(${id}) "${prev}" → "${type}"`);
      setState("terminals", id, "awaitingInput", type);
    },

    /** Clear terminal awaiting input state */
    clearAwaitingInput(id: string): void {
      const prev = state.terminals[id]?.awaitingInput;
      if (prev) appLogger.debug("terminal", `clearAwaitingInput(${id}) was "${prev}" → null`);
      setState("terminals", id, "awaitingInput", null);
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

    /** Get the agentType for a PTY session, or null if not found */
    getAgentTypeForSession(sessionId: string): string | null {
      for (const t of Object.values(state.terminals)) {
        if (t.sessionId === sessionId) return t.agentType ?? null;
      }
      return null;
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
    onBusyToIdle(callback: (id: string, durationMs: number) => void): void {
      busyToIdleCallbacks.push(callback);
    },

    /** Set the complete layout state */
    setLayout(layout: TabLayout): void {
      setState("layout", layout);
    },

    /** Split the current pane in a given direction, returns new terminal ID or null */
    splitPane(direction: "vertical" | "horizontal"): string | null {
      const { panes } = state.layout;
      // Can't split if already split or no panes
      if (state.layout.direction !== "none" || panes.length === 0) return null;

      const sourceId = panes[0];
      const source = state.terminals[sourceId];
      const cwd = source?.cwd || null;

      const newId = actions.add({
        sessionId: null,
        fontSize: source?.fontSize ?? 14,
        name: `Split ${state.counter + 1}`,
        cwd,
        awaitingInput: null,
      });

      setState("layout", {
        direction,
        panes: [sourceId, newId],
        ratio: 0.5,
        activePaneIndex: 1,
      });

      return newId;
    },

    /** Close a split pane by index, collapsing to single pane */
    closeSplitPane(index: 0 | 1): void {
      if (state.layout.direction === "none") return;
      const { panes } = state.layout;
      if (panes.length < 2) return;

      const survivorIndex = index === 0 ? 1 : 0;
      const survivor = panes[survivorIndex];

      setState("layout", {
        direction: "none",
        panes: [survivor],
        ratio: 0.5,
        activePaneIndex: 0,
      });
    },

    /** Set the split ratio (clamped to 0.2–0.8) */
    setSplitRatio(ratio: number): void {
      const clamped = Math.min(0.8, Math.max(0.2, ratio));
      setState("layout", "ratio", clamped);
    },

    /** Set the active pane index (0 or 1) */
    setActivePaneIndex(index: 0 | 1): void {
      setState("layout", "activePaneIndex", index);
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
