import { createStore, reconcile } from "solid-js/store";
import { createMemo } from "solid-js";
import { invoke } from "../invoke";
import { appLogger } from "./appLogger";
import { SMART_PROMPTS_BUILTIN } from "../data/smartPromptsBuiltIn";

/** Prompt category */
export type PromptCategory = "custom" | "recent" | "favorite";

/** Where a smart prompt can appear in the UI */
export type SmartPlacement = "toolbar" | "git-changes" | "git-branches" | "pr-popover" | "terminal-context" | "command-palette";

/** Prompt variable for template substitution */
export interface PromptVariable {
  name: string;
  description?: string;
  defaultValue?: string;
}

/** A saved prompt */
export interface SavedPrompt {
  id: string;
  name: string;
  description?: string;
  content: string;
  category: PromptCategory;
  shortcut?: string;
  variables?: PromptVariable[];
  isFavorite: boolean;
  lastUsed?: number;
  createdAt: number;
  updatedAt: number;
  tags?: string[];
  autoExecute?: boolean;
  requiresIdle?: boolean;
  placement?: SmartPlacement[];
  builtIn?: boolean;
  builtInVersion?: number;
  icon?: string;
  executionMode?: "inject" | "headless" | "api";
  outputTarget?: "clipboard" | "commit-message" | "toast" | "panel";
  systemPrompt?: string;
  enabled?: boolean;
}

/** Prompt library store state */
interface PromptLibraryState {
  prompts: Record<string, SavedPrompt>;
  recentIds: string[];
  drawerOpen: boolean;
  searchQuery: string;
  selectedCategory: PromptCategory | "all";
}

const LEGACY_STORAGE_KEY = "tui-commander-prompt-library";
const MAX_RECENT = 10;

/** Generate a unique ID */
function generateId(): string {
  return `prompt-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
}

/** Debounced persist to Rust backend — coalesces rapid updates (e.g. markAsUsed) */
let saveTimer: ReturnType<typeof setTimeout> | null = null;
function savePrompts(prompts: Record<string, SavedPrompt>): void {
  if (saveTimer) clearTimeout(saveTimer);
  saveTimer = setTimeout(() => {
    const promptArray = Object.values(prompts).map((p) => ({
      id: p.id,
      label: p.name,
      text: JSON.stringify(p),
      pinned: p.isFavorite,
    }));
    invoke("save_prompt_library", { config: { prompts: promptArray } }).catch((err) =>
      appLogger.error("store", "Failed to save prompt library", err),
    );
  }, 500);
}

/** Create the prompt library store */
function createPromptLibraryStore() {
  const [state, setState] = createStore<PromptLibraryState>({
    prompts: {},
    recentIds: [],
    drawerOpen: false,
    searchQuery: "",
    selectedCategory: "all",
  });

  const actions = {
    /** Load prompts from Rust backend; migrate from localStorage on first run */
    async hydrate(): Promise<void> {
      try {
        // One-time migration from localStorage
        const legacy = localStorage.getItem(LEGACY_STORAGE_KEY);
        if (legacy) {
          try {
            const parsed = JSON.parse(legacy) as Record<string, SavedPrompt>;
            const promptArray = Object.values(parsed).map((p) => ({
              id: p.id,
              label: p.name,
              text: JSON.stringify(p),
              pinned: p.isFavorite,
            }));
            await invoke("save_prompt_library", { config: { prompts: promptArray } });
            localStorage.removeItem(LEGACY_STORAGE_KEY);
          } catch (err) {
            appLogger.warn("store", "Legacy prompt migration failed, will retry next launch", err);
          }
        }

        const loaded = await invoke<{ prompts?: Array<{ id: string; label: string; text: string; pinned: boolean }> }>("load_prompt_library");
        let migrated = false;
        if (loaded?.prompts && loaded.prompts.length > 0) {
          const restored: Record<string, SavedPrompt> = {};
          for (const entry of loaded.prompts) {
            // Try to parse full SavedPrompt from text field (migration format)
            try {
              const full = JSON.parse(entry.text) as SavedPrompt;
              // Validate security-relevant fields before trusting deserialized data
              if (full.executionMode && full.executionMode !== "inject" && full.executionMode !== "headless" && full.executionMode !== "api") {
                appLogger.warn("store", `Prompt "${entry.id}" has invalid executionMode "${full.executionMode}", resetting to inject`);
                full.executionMode = "inject";
              }
              if (full.placement && !Array.isArray(full.placement)) {
                full.placement = undefined;
              }
              // Migrate legacy placement name: tab-context → terminal-context
              if (Array.isArray(full.placement) && full.placement.some((p) => (p as string) === "tab-context")) {
                full.placement = full.placement.map((p) =>
                  (p as string) === "tab-context" ? "terminal-context" : p,
                ) as SmartPlacement[];
                migrated = true;
              }
              restored[full.id] = full;
            } catch (err) {
              appLogger.warn("store", `Prompt "${entry.id}" has non-JSON text field, using simple format`, err);
              restored[entry.id] = {
                id: entry.id,
                name: entry.label,
                content: entry.text,
                category: "custom",
                isFavorite: entry.pinned,
                createdAt: Date.now(),
                updatedAt: Date.now(),
              };
            }
          }
          setState("prompts", restored);
          if (migrated) savePrompts(restored);
        }

        // Merge built-in smart prompts: add new ones, update unmodified metadata, preserve user overrides
        const merged = { ...state.prompts };
        let changed = false;
        for (const builtin of SMART_PROMPTS_BUILTIN) {
          const existing = merged[builtin.id];
          if (!existing) {
            merged[builtin.id] = builtin;
            changed = true;
          } else if (existing.builtIn && existing.content === builtin.content) {
            // Unmodified built-in: update metadata silently (version, placement, etc.)
            merged[builtin.id] = { ...existing, ...builtin, content: existing.content };
            changed = true;
          }
          // If user has overridden content, keep their version
        }
        if (changed) {
          setState("prompts", merged);
          savePrompts(merged);
        }
      } catch (err) {
        appLogger.error("store", "Failed to hydrate prompt library", err);
      }
    },


    /** Open the drawer */
    openDrawer(): void {
      setState("drawerOpen", true);
      setState("searchQuery", "");
    },

    /** Close the drawer */
    closeDrawer(): void {
      setState("drawerOpen", false);
      setState("searchQuery", "");
    },

    /** Toggle drawer */
    toggleDrawer(): void {
      if (state.drawerOpen) {
        actions.closeDrawer();
      } else {
        actions.openDrawer();
      }
    },

    /** Set search query */
    setSearchQuery(query: string): void {
      setState("searchQuery", query);
    },

    /** Set selected category filter */
    setSelectedCategory(category: PromptCategory | "all"): void {
      setState("selectedCategory", category);
    },

    /** Create a new prompt */
    createPrompt(data: Omit<SavedPrompt, "id" | "createdAt" | "updatedAt">): SavedPrompt {
      const now = Date.now();
      const prompt: SavedPrompt = {
        ...data,
        id: generateId(),
        createdAt: now,
        updatedAt: now,
      };
      setState("prompts", prompt.id, prompt);
      savePrompts(state.prompts);
      return prompt;
    },

    /** Update an existing prompt */
    updatePrompt(id: string, data: Partial<SavedPrompt>): void {
      const existing = state.prompts[id];
      if (!existing) return;

      setState("prompts", id, {
        ...existing,
        ...data,
        updatedAt: Date.now(),
      });
      savePrompts(state.prompts);
    },

    /** Delete a prompt */
    deletePrompt(id: string): void {
      const { [id]: _, ...rest } = state.prompts;
      setState("prompts", reconcile(rest));
      savePrompts(state.prompts);
    },

    /** Toggle favorite status */
    toggleFavorite(id: string): void {
      const prompt = state.prompts[id];
      if (prompt) {
        actions.updatePrompt(id, { isFavorite: !prompt.isFavorite });
      }
    },

    /** Mark prompt as recently used */
    markAsUsed(id: string): void {
      const prompt = state.prompts[id];
      if (prompt) {
        actions.updatePrompt(id, { lastUsed: Date.now() });

        // Update recent list
        const newRecent = [id, ...state.recentIds.filter((r) => r !== id)].slice(0, MAX_RECENT);
        setState("recentIds", newRecent);
      }
    },

    /** Get prompt by ID */
    getPrompt(id: string): SavedPrompt | undefined {
      return state.prompts[id];
    },

    /** Get all prompts */
    getAllPrompts(): SavedPrompt[] {
      return Object.values(state.prompts);
    },


    /** Get enabled smart prompts (built-in tagged "smart" or any prompt with a placement), sorted by category then name (memoized) */
    getSmartPrompts: (() => {
      const memo = createMemo(() =>
        Object.values(state.prompts)
          .filter((p) => p.enabled !== false && (p.tags?.includes("smart") || (p.placement && p.placement.length > 0)))
          .sort((a, b) => {
            const catCmp = a.category.localeCompare(b.category);
            return catCmp !== 0 ? catCmp : a.name.localeCompare(b.name);
          }),
      );
      return () => memo();
    })(),

    /** Get enabled smart prompts for a specific UI placement */
    getSmartByPlacement(placement: SmartPlacement): SavedPrompt[] {
      return actions.getSmartPrompts().filter((p) => p.placement?.includes(placement));
    },

    /** Get a smart prompt by ID (must have the smart tag) */
    getSmartById(id: string): SavedPrompt | undefined {
      const p = state.prompts[id];
      return p?.tags?.includes("smart") ? p : undefined;
    },

    /** Replace prompt content with the default, keeping user preferences */
    resetToDefault(id: string, defaultPrompt: SavedPrompt): void {
      const existing = state.prompts[id];
      if (!existing) return;
      setState("prompts", id, {
        ...defaultPrompt,
        enabled: existing.enabled,
        shortcut: existing.shortcut,
        updatedAt: Date.now(),
      });
      savePrompts(state.prompts);
    },

    /** Check if the prompt's content differs from the default */
    isOverridden(id: string, defaultContent: string): boolean {
      const p = state.prompts[id];
      return p !== undefined && p.content !== defaultContent;
    },

    /** Check if the prompt's builtInVersion is less than latestVersion */
    hasUpdate(id: string, latestVersion: number): boolean {
      const p = state.prompts[id];
      return p !== undefined && (p.builtInVersion ?? 0) < latestVersion;
    },

    /** Get filtered prompts based on search and category */
    getFilteredPrompts(): SavedPrompt[] {
      let prompts = Object.values(state.prompts);

      // Filter by category
      if (state.selectedCategory !== "all") {
        if (state.selectedCategory === "recent") {
          prompts = state.recentIds
            .map((id) => state.prompts[id])
            .filter((p): p is SavedPrompt => p !== undefined);
        } else if (state.selectedCategory === "favorite") {
          prompts = prompts.filter((p) => p.isFavorite);
        } else {
          prompts = prompts.filter((p) => p.category === state.selectedCategory);
        }
      }

      // Filter by search query
      if (state.searchQuery.trim()) {
        const query = state.searchQuery.toLowerCase();
        prompts = prompts.filter(
          (p) =>
            p.name.toLowerCase().includes(query) ||
            p.description?.toLowerCase().includes(query) ||
            p.content.toLowerCase().includes(query),
        );
      }

      // Sort by most recently used/updated
      return [...prompts].sort((a, b) => {
        const aTime = a.lastUsed || a.updatedAt;
        const bTime = b.lastUsed || b.updatedAt;
        return bTime - aTime;
      });
    },
    /** Process prompt content with variable substitution (Rust backend) */
    async processContent(prompt: SavedPrompt, variables: Record<string, string>): Promise<string> {
      return invoke<string>("process_prompt_content", { content: prompt.content, variables });
    },

    /** Get variables from prompt content (Rust backend) */
    async extractVariables(content: string): Promise<string[]> {
      return invoke<string[]>("extract_prompt_variables", { content });
    },

    /** Resolve all auto-resolvable git context variables for smart prompt execution.
     *
     * Returns git/repo variables from Rust. Frontend store variables (GitHub, agent, etc.)
     * are merged by the execution engine (useSmartPrompts hook) to avoid circular deps. */
    async resolveVariables(repoPath: string): Promise<Record<string, string>> {
      try {
        return await invoke<Record<string, string>>("resolve_context_variables", { repoPath });
      } catch (err) {
        appLogger.warn("store", "Failed to resolve context variables", err);
        return {};
      }
    },
  };

  return { state, ...actions };
}

export const promptLibraryStore = createPromptLibraryStore();
