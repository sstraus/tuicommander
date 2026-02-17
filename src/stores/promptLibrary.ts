import { createStore, reconcile } from "solid-js/store";
import { invoke } from "../invoke";

/** Prompt category */
export type PromptCategory = "custom" | "recent" | "favorite";

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
}

/** Prompt library store state */
interface PromptLibraryState {
  prompts: Record<string, SavedPrompt>;
  drawerOpen: boolean;
  searchQuery: string;
  selectedCategory: PromptCategory | "all";
  recentIds: string[];
}

const LEGACY_STORAGE_KEY = "tui-commander-prompt-library";
const MAX_RECENT = 10;

/** Generate a unique ID */
function generateId(): string {
  return `prompt-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
}

/** Persist prompts to Rust backend (fire-and-forget) */
function savePrompts(prompts: Record<string, SavedPrompt>): void {
  // Convert to array format for the Rust config struct
  const promptArray = Object.values(prompts).map((p) => ({
    id: p.id,
    label: p.name,
    text: p.content,
    pinned: p.isFavorite,
  }));
  invoke("save_prompt_library", { config: { prompts: promptArray } }).catch((err) =>
    console.error("Failed to save prompt library:", err),
  );
}

/** Create the prompt library store */
function createPromptLibraryStore() {
  const [state, setState] = createStore<PromptLibraryState>({
    prompts: {},
    drawerOpen: false,
    searchQuery: "",
    selectedCategory: "all",
    recentIds: [],
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
            // Save full prompts as-is via a generic save (the Rust struct is simplified;
            // we store the full data as JSON for lossless round-trip)
            const promptArray = Object.values(parsed).map((p) => ({
              id: p.id,
              label: p.name,
              text: JSON.stringify(p),
              pinned: p.isFavorite,
            }));
            await invoke("save_prompt_library", { config: { prompts: promptArray } });
          } catch { /* ignore corrupt legacy data */ }
          localStorage.removeItem(LEGACY_STORAGE_KEY);
        }

        const loaded = await invoke<{ prompts?: Array<{ id: string; label: string; text: string; pinned: boolean }> }>("load_prompt_library");
        if (loaded?.prompts && loaded.prompts.length > 0) {
          const restored: Record<string, SavedPrompt> = {};
          for (const entry of loaded.prompts) {
            // Try to parse full SavedPrompt from text field (migration format)
            try {
              const full = JSON.parse(entry.text) as SavedPrompt;
              restored[full.id] = full;
            } catch {
              // Simple prompt entry
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
        }
      } catch (err) {
        console.debug("Failed to hydrate prompt library:", err);
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
            p.content.toLowerCase().includes(query)
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
  };

  return { state, ...actions };
}

export const promptLibraryStore = createPromptLibraryStore();
