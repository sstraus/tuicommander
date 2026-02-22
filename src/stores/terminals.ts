import { createStore, produce } from "solid-js/store";
import type { AgentType } from "../agents";

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
}

/** Create the terminals store */
function createTerminalsStore() {
  const [state, setState] = createStore<TerminalsStoreState>({
    terminals: {},
    activeId: null,
    counter: 0,
    layout: { ...DEFAULT_LAYOUT },
    detachedWindows: {},
  });

  const actions = {
    /** Add a new terminal */
    add(data: Omit<TerminalData, "id" | "activity" | "progress" | "shellState" | "nameIsCustom" | "agentType" | "pendingResumeCommand" | "usageLimit" | "lastDataAt">): string {
      const id = `term-${state.counter + 1}`;
      setState("counter", (c) => c + 1);
      setState("terminals", id, { id, activity: false, progress: null, shellState: null, nameIsCustom: false, agentType: null, pendingResumeCommand: null, usageLimit: null, lastDataAt: null, ...data });
      return id;
    },

    /** Register a terminal with a specific ID (used by floating windows to reconnect to existing PTY sessions) */
    register(id: string, data: Omit<TerminalData, "id" | "activity" | "progress" | "shellState" | "nameIsCustom" | "agentType" | "pendingResumeCommand" | "usageLimit" | "lastDataAt">): void {
      setState("terminals", id, { id, activity: false, progress: null, shellState: null, nameIsCustom: false, agentType: null, pendingResumeCommand: null, usageLimit: null, lastDataAt: null, ...data });
    },

    /** Remove a terminal */
    remove(id: string): void {
      setState(
        produce((s) => {
          delete s.terminals[id];
          // If we removed the active terminal, select another
          if (s.activeId === id) {
            const remaining = Object.keys(s.terminals);
            s.activeId = remaining.length > 0 ? remaining[remaining.length - 1] : null;
          }
        })
      );
    },

    /** Set the active terminal (clears its activity and shell state indicators) */
    setActive(id: string | null): void {
      if (id) {
        setState("terminals", id, "activity", false);
        setState("terminals", id, "shellState", null);
      }
      setState("activeId", id);
    },

    /** Update terminal data */
    update(id: string, data: Partial<TerminalState>): void {
      setState("terminals", id, (prev) => ({ ...prev, ...data }));
    },

    /** Update session ID */
    setSessionId(id: string, sessionId: string | null): void {
      setState("terminals", id, "sessionId", sessionId);
    },

    /** Update font size (zoom) */
    setFontSize(id: string, fontSize: number): void {
      setState("terminals", id, "fontSize", fontSize);
    },

    /** Set terminal awaiting input state */
    setAwaitingInput(id: string, type: AwaitingInputType): void {
      setState("terminals", id, "awaitingInput", type);
    },

    /** Clear terminal awaiting input state */
    clearAwaitingInput(id: string): void {
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
