import { createStore } from "solid-js/store";
import type { ContentMatch, ContentSearchBatch, DirEntry } from "../types/fs";
import { invoke, listen } from "../invoke";
import { repositoriesStore } from "./repositories";
import { appLogger } from "./appLogger";

const RECENT_ACTIONS_KEY = "tui-commander-recent-actions";
const MAX_RECENT = 10;
const SEARCH_DEBOUNCE_MS = 300;
const CONTENT_SEARCH_MIN_CHARS = 3;
const FILENAME_SEARCH_MIN_CHARS = 1;

export type PaletteMode = "command" | "filename" | "content";

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
    setState({ contentResults: [], contentSearching: false, contentError: null, filenameResults: [], filenameSearching: false });
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

    /** Derived mode based on query prefix: ! = filename, ? = content */
    mode(): PaletteMode {
      if (state.query.startsWith("!")) return "filename";
      if (state.query.startsWith("?")) return "content";
      return "command";
    },

    /** The effective search query (strips prefix character and leading space) */
    searchQuery(): string {
      if (state.query.startsWith("!") || state.query.startsWith("?")) return state.query.slice(1).trimStart();
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
