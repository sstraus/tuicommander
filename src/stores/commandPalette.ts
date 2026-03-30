import { createStore } from "solid-js/store";
import type { ContentMatch, ContentSearchBatch } from "../types/fs";
import { invoke, listen } from "../invoke";
import { repositoriesStore } from "./repositories";
import { appLogger } from "./appLogger";

const RECENT_ACTIONS_KEY = "tui-commander-recent-actions";
const MAX_RECENT = 10;
const CONTENT_SEARCH_DEBOUNCE_MS = 500;
const CONTENT_SEARCH_MIN_CHARS = 3;

export type PaletteMode = "command" | "content";

interface CommandPaletteState {
  isOpen: boolean;
  query: string;
  recentActions: string[];
  contentResults: ContentMatch[];
  contentSearching: boolean;
  contentError: string | null;
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
  });

  // Content search lifecycle state
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;
  let cancelled = false;
  let unlistenBatch: (() => void) | null = null;
  let unlistenError: (() => void) | null = null;

  function cleanupContentSearch(): void {
    cancelled = true;
    if (debounceTimer) { clearTimeout(debounceTimer); debounceTimer = null; }
    unlistenBatch?.(); unlistenBatch = null;
    unlistenError?.(); unlistenError = null;
    setState({ contentResults: [], contentSearching: false, contentError: null });
  }

  /** Fire a content search with streaming results */
  function triggerContentSearch(searchQuery: string): void {
    const repoPath = repositoriesStore.state.activeRepoPath;
    if (!repoPath || searchQuery.length < CONTENT_SEARCH_MIN_CHARS) return;

    cancelled = false;
    setState({ contentResults: [], contentSearching: true, contentError: null });

    // Subscribe to streaming results BEFORE invoking search
    listen<ContentSearchBatch>("content-search-batch", (event) => {
      if (cancelled) return;
      const batch = event.payload;
      setState("contentResults", (prev) => [...prev, ...batch.matches]);
      if (batch.is_final) {
        setState("contentSearching", false);
      }
    }).then((fn) => { unlistenBatch = fn; });

    listen<string>("content-search-error", (event) => {
      if (cancelled) return;
      setState({ contentError: event.payload, contentSearching: false });
    }).then((fn) => { unlistenError = fn; });

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

  return {
    state,

    /** Derived mode based on query prefix */
    mode(): PaletteMode {
      return state.query.startsWith("!") ? "content" : "command";
    },

    /** The effective search query (strips `!` prefix in content mode) */
    contentQuery(): string {
      return state.query.startsWith("!") ? state.query.slice(1) : "";
    },

    open(): void {
      cleanupContentSearch();
      setState("isOpen", true);
      setState("query", "");
    },

    close(): void {
      cleanupContentSearch();
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
      const wasContent = state.query.startsWith("!");
      const isContent = query.startsWith("!");
      setState("query", query);

      // Mode changed or query changed in content mode → manage search lifecycle
      if (isContent) {
        const searchQuery = query.slice(1);
        // Cancel previous debounce/search
        if (debounceTimer) { clearTimeout(debounceTimer); debounceTimer = null; }
        cancelled = true;
        unlistenBatch?.(); unlistenBatch = null;
        unlistenError?.(); unlistenError = null;

        if (searchQuery.length >= CONTENT_SEARCH_MIN_CHARS) {
          setState("contentSearching", false);
          debounceTimer = setTimeout(() => triggerContentSearch(searchQuery), CONTENT_SEARCH_DEBOUNCE_MS);
        } else {
          setState({ contentResults: [], contentSearching: false, contentError: null });
        }
      } else if (wasContent && !isContent) {
        // Switched back to command mode — cleanup
        cleanupContentSearch();
      }
    },

    recordUsage(actionId: string): void {
      const updated = [actionId, ...state.recentActions.filter((id) => id !== actionId)].slice(0, MAX_RECENT);
      setState("recentActions", updated);
      try {
        localStorage.setItem(RECENT_ACTIONS_KEY, JSON.stringify(updated));
      } catch {
        // localStorage full — ignore
      }
    },
  };
}

export const commandPaletteStore = createCommandPaletteStore();
