import { describe, it, expect, vi, beforeEach } from "vitest";
import { createRoot } from "solid-js";

const mockInvoke = vi.fn().mockResolvedValue(undefined);

vi.mock("@tauri-apps/api/core", () => ({
  invoke: mockInvoke,
}));

describe("promptLibraryStore", () => {
  let store: typeof import("../../stores/promptLibrary").promptLibraryStore;

  beforeEach(async () => {
    vi.resetModules();
    mockInvoke.mockReset().mockResolvedValue(undefined);
    localStorage.clear();

    vi.doMock("@tauri-apps/api/core", () => ({
      invoke: mockInvoke,
    }));

    store = (await import("../../stores/promptLibrary")).promptLibraryStore;
  });

  describe("createPrompt()", () => {
    it("creates a prompt with generated ID", () => {
      createRoot((dispose) => {
        const prompt = store.createPrompt({
          name: "Test Prompt",
          content: "Hello {name}!",
          category: "custom",
          isFavorite: false,
        });
        expect(prompt.id).toBeTruthy();
        expect(prompt.name).toBe("Test Prompt");
        expect(prompt.createdAt).toBeGreaterThan(0);
        dispose();
      });
    });

    it("persists via invoke", () => {
      createRoot((dispose) => {
        store.createPrompt({
          name: "Test",
          content: "content",
          category: "custom",
          isFavorite: false,
        });
        expect(mockInvoke).toHaveBeenCalledWith("save_prompt_library", {
          config: expect.objectContaining({
            prompts: expect.arrayContaining([
              expect.objectContaining({ label: "Test", text: "content" }),
            ]),
          }),
        });
        dispose();
      });
    });
  });

  describe("updatePrompt()", () => {
    it("updates existing prompt", () => {
      createRoot((dispose) => {
        const prompt = store.createPrompt({
          name: "Old Name",
          content: "content",
          category: "custom",
          isFavorite: false,
        });
        store.updatePrompt(prompt.id, { name: "New Name" });
        expect(store.getPrompt(prompt.id)?.name).toBe("New Name");
        dispose();
      });
    });

    it("ignores updates for non-existent prompts", () => {
      createRoot((dispose) => {
        store.updatePrompt("nonexistent", { name: "Updated" }); // Should not throw
        dispose();
      });
    });
  });

  describe("deletePrompt()", () => {
    it("deletes a prompt", () => {
      createRoot((dispose) => {
        const prompt = store.createPrompt({
          name: "Test",
          content: "content",
          category: "custom",
          isFavorite: false,
        });
        store.deletePrompt(prompt.id);
        expect(store.getPrompt(prompt.id)).toBeUndefined();
        dispose();
      });
    });
  });

  describe("toggleFavorite()", () => {
    it("toggles favorite status", () => {
      createRoot((dispose) => {
        const prompt = store.createPrompt({
          name: "Test",
          content: "content",
          category: "custom",
          isFavorite: false,
        });
        store.toggleFavorite(prompt.id);
        expect(store.getPrompt(prompt.id)?.isFavorite).toBe(true);
        store.toggleFavorite(prompt.id);
        expect(store.getPrompt(prompt.id)?.isFavorite).toBe(false);
        dispose();
      });
    });
  });

  describe("markAsUsed()", () => {
    it("updates lastUsed timestamp", () => {
      createRoot((dispose) => {
        const prompt = store.createPrompt({
          name: "Test",
          content: "content",
          category: "custom",
          isFavorite: false,
        });
        store.markAsUsed(prompt.id);
        expect(store.getPrompt(prompt.id)?.lastUsed).toBeGreaterThan(0);
        dispose();
      });
    });

    it("adds to recent list", () => {
      createRoot((dispose) => {
        const prompt = store.createPrompt({
          name: "Test",
          content: "content",
          category: "custom",
          isFavorite: false,
        });
        store.markAsUsed(prompt.id);
        expect(store.state.recentIds).toContain(prompt.id);
        dispose();
      });
    });
  });

  describe("drawer management", () => {
    it("openDrawer opens and clears search", () => {
      createRoot((dispose) => {
        store.setSearchQuery("old query");
        store.openDrawer();
        expect(store.state.drawerOpen).toBe(true);
        expect(store.state.searchQuery).toBe("");
        dispose();
      });
    });

    it("closeDrawer closes and clears search", () => {
      createRoot((dispose) => {
        store.openDrawer();
        store.setSearchQuery("test");
        store.closeDrawer();
        expect(store.state.drawerOpen).toBe(false);
        expect(store.state.searchQuery).toBe("");
        dispose();
      });
    });

    it("toggleDrawer toggles", () => {
      createRoot((dispose) => {
        store.toggleDrawer();
        expect(store.state.drawerOpen).toBe(true);
        store.toggleDrawer();
        expect(store.state.drawerOpen).toBe(false);
        dispose();
      });
    });
  });

  describe("getFilteredPrompts()", () => {
    it("returns all prompts when no filter", () => {
      createRoot((dispose) => {
        store.createPrompt({ name: "P1", content: "c1", category: "custom", isFavorite: false });
        store.createPrompt({ name: "P2", content: "c2", category: "custom", isFavorite: false });
        expect(store.getFilteredPrompts()).toHaveLength(2);
        dispose();
      });
    });

    it("filters by search query", () => {
      createRoot((dispose) => {
        store.createPrompt({ name: "Fix Bug", content: "fix", category: "custom", isFavorite: false });
        store.createPrompt({ name: "Add Feature", content: "add", category: "custom", isFavorite: false });
        store.setSearchQuery("bug");
        expect(store.getFilteredPrompts()).toHaveLength(1);
        expect(store.getFilteredPrompts()[0].name).toBe("Fix Bug");
        dispose();
      });
    });

    it("filters by favorite category", () => {
      createRoot((dispose) => {
        store.createPrompt({ name: "P1", content: "c1", category: "custom", isFavorite: true });
        store.createPrompt({ name: "P2", content: "c2", category: "custom", isFavorite: false });
        store.setSelectedCategory("favorite");
        expect(store.getFilteredPrompts()).toHaveLength(1);
        dispose();
      });
    });

    it("filters by custom category", () => {
      createRoot((dispose) => {
        store.createPrompt({ name: "P1", content: "c1", category: "custom", isFavorite: false });
        store.setSelectedCategory("custom");
        expect(store.getFilteredPrompts()).toHaveLength(1);
        dispose();
      });
    });

    it("does not mutate store array when sorting", () => {
      createRoot((dispose) => {
        // Create prompts with different timestamps so sort would change order
        store.createPrompt({ name: "Older", content: "c1", category: "custom", isFavorite: false });
        store.createPrompt({ name: "Newer", content: "c2", category: "custom", isFavorite: false });
        const before = store.getAllPrompts().map((p) => p.id);
        store.getFilteredPrompts();
        const after = store.getAllPrompts().map((p) => p.id);
        expect(after).toEqual(before);
        dispose();
      });
    });
  });

  describe("getAllPrompts()", () => {
    it("returns all prompts as array", () => {
      createRoot((dispose) => {
        store.createPrompt({ name: "P1", content: "c1", category: "custom", isFavorite: false });
        store.createPrompt({ name: "P2", content: "c2", category: "custom", isFavorite: false });
        expect(store.getAllPrompts()).toHaveLength(2);
        dispose();
      });
    });
  });

  describe("hydrate()", () => {
    it("loads prompts from Rust backend", async () => {
      mockInvoke.mockResolvedValueOnce({
        prompts: [
          { id: "p1", label: "Test", text: "content", pinned: false },
        ],
      });

      await createRoot(async (dispose) => {
        await store.hydrate();
        expect(store.getPrompt("p1")?.name).toBe("Test");
        expect(store.getPrompt("p1")?.content).toBe("content");
        expect(mockInvoke).toHaveBeenCalledWith("load_prompt_library");
        dispose();
      });
    });

    it("migrates from localStorage on first run", async () => {
      const legacyPrompts = {
        "p1": { id: "p1", name: "Legacy", content: "old", category: "custom", isFavorite: true, createdAt: 1, updatedAt: 1 },
      };
      localStorage.setItem("tui-commander-prompt-library", JSON.stringify(legacyPrompts));
      mockInvoke.mockResolvedValueOnce(undefined); // save_prompt_library migration
      mockInvoke.mockResolvedValueOnce({ prompts: [] }); // load_prompt_library

      await createRoot(async (dispose) => {
        await store.hydrate();
        expect(localStorage.getItem("tui-commander-prompt-library")).toBeNull();
        dispose();
      });
    });
  });
});
