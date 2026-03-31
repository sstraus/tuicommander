import { createStore } from "solid-js/store";
import type { ContentMatch, ContentSearchBatch, DirEntry } from "../types/fs";
import type { TerminalMatch } from "../types";
import { invoke, listen } from "../invoke";
import { repositoriesStore } from "./repositories";
import { appLogger } from "./appLogger";

const RECENT_ACTIONS_KEY = "tui-commander-recent-actions";
const MAX_RECENT = 10;
const SEARCH_DEBOUNCE_MS = 300;
const CONTENT_SEARCH_MIN_CHARS = 3;
const FILENAME_SEARCH_MIN_CHARS = 1;
const TERMINAL_SEARCH_MIN_CHARS = 3;
const MAX_CONTENT_RESULTS = 200;
const MAX_TERMINAL_RESULTS = 200;

export type PaletteMode = "command" | "filename" | "content" | "terminal";

interface CommandPaletteState {
  isOpen: boolean;
  query: string;
  recentActions: string[];
  /** Content search results (? prefix) */
  contentResults: ContentMatch[];
  contentSearching: boolean;
  contentError: string | null;
  /** Filename search results (! prefix) */
  filenameResults: DirEntry[];
  filenameSearching: boolean;
  /** Terminal buffer search results (~ prefix) */
  terminalResults: TerminalMatch[];
  terminalSearching: boolean;
}

function loadRecentActions(): string[] {
  try {
    const raw = localStorage.getItem(RECENT_ACTIONS_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed.slice(0, MAX_RECENT) : [];
  } catch {
    return [];
  }
}

function createCommandPaletteStore() {
  const [state, setState] = createStore<CommandPaletteState>({
    isOpen: false,
    query: "",
    recentActions: loadRecentActions(),
    contentResults: [],
    contentSearching: false,
    contentError: null,
    filenameResults: [],
    filenameSearching: false,
    terminalResults: [],
    terminalSearching: false,
  });

  // Search lifecycle state
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;
  let cancelled = false;
  let unlistenBatch: (() => void) | null = null;
  let unlistenError: (() => void) | null = null;

  function cleanupSearch(): void {
    cancelled = true;
    if (debounceTimer) { clearTimeout(debounceTimer); debounceTimer = null; }
    unlistenBatch?.(); unlistenBatch = null;
    unlistenError?.(); unlistenError = null;
    setState({ contentResults: [], contentSearching: false, contentError: null, filenameResults: [], filenameSearching: false, terminalResults: [], terminalSearching: false });
  }

  /** Fire a filename search (non-streaming, single invoke) */
  function triggerFilenameSearch(searchQuery: string): void {
    const repoPath = repositoriesStore.state.activeRepoPath;
    if (!repoPath || searchQuery.length < FILENAME_SEARCH_MIN_CHARS) return;

    cancelled = false;
    setState({ filenameResults: [], filenameSearching: true });

    invoke<DirEntry[]>("search_files", { repoPath, query: searchQuery, limit: 50 })
      .then((results) => {
        if (!cancelled) {
          setState({ filenameResults: results, filenameSearching: false });
        }
      })
      .catch((err) => {
        if (!cancelled) {
          appLogger.error("app", "Filename search failed", err);
          setState({ filenameSearching: false });
        }
      });
  }

  /** Fire a content search with streaming results */
  async function triggerContentSearch(searchQuery: string): Promise<void> {
    const repoPath = repositoriesStore.state.activeRepoPath;
    if (!repoPath || searchQuery.length < CONTENT_SEARCH_MIN_CHARS) return;

    cancelled = false;
    setState({ contentResults: [], contentSearching: true, contentError: null });

    // Subscribe to streaming results BEFORE invoking search
    try {
      unlistenBatch = await listen<ContentSearchBatch>("content-search-batch", (event) => {
        if (cancelled) return;
        const batch = event.payload;
        setState("contentResults", (prev) => {
          if (prev.length >= MAX_CONTENT_RESULTS) return prev;
          const combined = [...prev, ...batch.matches];
          return combined.slice(0, MAX_CONTENT_RESULTS);
        });
        if (batch.is_final) {
          setState("contentSearching", false);
        }
      });

      unlistenError = await listen<string>("content-search-error", (event) => {
        if (cancelled) return;
        setState({ contentError: event.payload, contentSearching: false });
      });
    } catch (err) {
      appLogger.error("app", "Failed to subscribe to content search events", err);
      setState({ contentError: "Search setup failed", contentSearching: false });
      return;
    }

    if (cancelled) return;

    invoke("search_content", {
      repoPath,
      query: searchQuery,
      caseSensitive: false,
      useRegex: false,
      wholeWord: false,
    }).catch((err) => {
      if (!cancelled) {
        appLogger.error("app", "Content search failed", err);
        setState({ contentError: String(err), contentSearching: false });
      }
    });
  }

  /** Search across all attached terminal buffers */
  function triggerTerminalSearch(searchQuery: string): void {
    // Lazy import to avoid circular dependency
    const { terminalsStore } = require("./terminals");

    cancelled = false;
    setState({ terminalResults: [], terminalSearching: true });

    const allResults: TerminalMatch[] = [];
    const terminals = terminalsStore.state.terminals as Record<string, { id: string; ref?: { searchBuffer?: (q: string) => TerminalMatch[] } }>;
    const detached = terminalsStore.state.detachedWindows as Record<string, string>;

    for (const id of Object.keys(terminals)) {
      if (cancelled) break;
      // Skip detached terminals
      if (detached[id]) continue;
      const ref = terminals[id]?.ref;
      if (!ref?.searchBuffer) continue;
      const matches = ref.searchBuffer(searchQuery);
      allResults.push(...matches);
      if (allResults.length >= MAX_TERMINAL_RESULTS) break;
    }

    if (!cancelled) {
      setState({
        terminalResults: allResults.slice(0, MAX_TERMINAL_RESULTS),
        terminalSearching: false,
      });
    }
  }

  return {
    state,

    /** Derived mode based on query prefix: ! = filename, ? = content, ~ = terminal */
    mode(): PaletteMode {
      if (state.query.startsWith("!")) return "filename";
      if (state.query.startsWith("?")) return "content";
      if (state.query.startsWith("~")) return "terminal";
      return "command";
    },

    /** The effective search query (strips prefix character and leading space) */
    searchQuery(): string {
      if (state.query.startsWith("!") || state.query.startsWith("?") || state.query.startsWith("~")) return state.query.slice(1).trimStart();
      return "";
    },

    open(): void {
      cleanupSearch();
      setState("isOpen", true);
      setState("query", "");
    },

    close(): void {
      cleanupSearch();
      setState("isOpen", false);
      setState("query", "");
    },

    toggle(): void {
      if (state.isOpen) {
        this.close();
      } else {
        this.open();
      }
    },

    setQuery(query: string): void {
      const prevMode = this.mode();
      setState("query", query);
      const newMode = this.mode();

      // Mode changed → cleanup previous search
      if (prevMode !== newMode && prevMode !== "command") {
        cleanupSearch();
      }

      if (newMode === "filename") {
        const searchQuery = query.slice(1).trimStart();
        if (debounceTimer) { clearTimeout(debounceTimer); debounceTimer = null; }
        cancelled = true;

        if (searchQuery.length >= FILENAME_SEARCH_MIN_CHARS) {
          debounceTimer = setTimeout(() => triggerFilenameSearch(searchQuery), SEARCH_DEBOUNCE_MS);
        } else {
          setState({ filenameResults: [], filenameSearching: false });
        }
      } else if (newMode === "content") {
        const searchQuery = query.slice(1).trimStart();
        if (debounceTimer) { clearTimeout(debounceTimer); debounceTimer = null; }
        cancelled = true;
        unlistenBatch?.(); unlistenBatch = null;
        unlistenError?.(); unlistenError = null;

        if (searchQuery.length >= CONTENT_SEARCH_MIN_CHARS) {
          setState("contentSearching", false);
          debounceTimer = setTimeout(() => triggerContentSearch(searchQuery), SEARCH_DEBOUNCE_MS);
        } else {
          setState({ contentResults: [], contentSearching: false, contentError: null });
        }
      } else if (newMode === "terminal") {
        const searchQuery = query.slice(1).trimStart();
        if (debounceTimer) { clearTimeout(debounceTimer); debounceTimer = null; }
        cancelled = true;

        if (searchQuery.length >= TERMINAL_SEARCH_MIN_CHARS) {
          debounceTimer = setTimeout(() => triggerTerminalSearch(searchQuery), SEARCH_DEBOUNCE_MS);
        } else {
          setState({ terminalResults: [], terminalSearching: false });
        }
      }
    },

    /** Open palette with a pre-filled query (e.g. "~ " for terminal search mode) */
    openWithQuery(query: string): void {
      cleanupSearch();
      setState({ isOpen: true, query });
      // Re-run setQuery to trigger mode-specific search logic
      this.setQuery(query);
    },

    recordUsage(actionId: string): void {
      const updated = [actionId, ...state.recentActions.filter((id) => id !== actionId)].slice(0, MAX_RECENT);
      setState("recentActions", updated);
      try {
        localStorage.setItem(RECENT_ACTIONS_KEY, JSON.stringify(updated));
      } catch (err) {
        appLogger.warn("app", "Failed to persist recent actions to localStorage", err);
      }
    },
  };
}

export const commandPaletteStore = createCommandPaletteStore();
