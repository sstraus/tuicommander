import { describe, it, expect, beforeEach, vi } from "vitest";
import "../mocks/tauri";
import { mockInvoke } from "../mocks/tauri";
import {
  DEFAULT_BINDINGS,
  normalizeCombo,
  ACTION_NAMES,
  type ActionName,
} from "../../keybindingDefaults";
import { createKeybindingsStore } from "../../stores/keybindings";

describe("keybindingDefaults", () => {
  describe("ACTION_NAMES", () => {
    it("contains all expected actions", () => {
      const expected: ActionName[] = [
        "zoom-in",
        "zoom-out",
        "zoom-reset",
        "new-terminal",
        "close-terminal",
        "split-vertical",
        "split-horizontal",
        "run-command",
        "edit-command",
        "toggle-diff",
        "toggle-markdown",
        "toggle-notes",
        "toggle-file-browser",
        "toggle-prompt-library",
        "toggle-settings",
        "toggle-task-queue",
        "reopen-closed-tab",
        "toggle-sidebar",
        "prev-tab",
        "next-tab",
        "clear-terminal",
        "open-lazygit",
        "toggle-git-ops",
        "toggle-help",
        "open-lazygit-pane",
      ];
      for (const name of expected) {
        expect(ACTION_NAMES).toContain(name);
      }
    });

    it("includes switch-tab-N actions (1-9)", () => {
      for (let i = 1; i <= 9; i++) {
        expect(ACTION_NAMES).toContain(`switch-tab-${i}`);
      }
    });

    it("includes switch-branch-N actions (1-9)", () => {
      for (let i = 1; i <= 9; i++) {
        expect(ACTION_NAMES).toContain(`switch-branch-${i}`);
      }
    });
  });

  describe("DEFAULT_BINDINGS", () => {
    it("has a binding for every action", () => {
      for (const name of ACTION_NAMES) {
        expect(DEFAULT_BINDINGS).toHaveProperty(name);
        expect(typeof DEFAULT_BINDINGS[name as ActionName]).toBe("string");
      }
    });

    it("has no duplicate key combos", () => {
      // Find duplicates for a better error message
      const seen = new Map<string, string[]>();
      for (const [action, combo] of Object.entries(DEFAULT_BINDINGS)) {
        const normalized = normalizeCombo(combo);
        if (!seen.has(normalized)) seen.set(normalized, []);
        seen.get(normalized)!.push(action);
      }
      const duplicates = [...seen.entries()].filter(([, actions]) => actions.length > 1);
      expect(duplicates).toEqual([]);
    });
  });

  describe("normalizeCombo", () => {
    it("lowercases the key", () => {
      expect(normalizeCombo("Cmd+D")).toBe("cmd+d");
    });

    it("sorts modifiers alphabetically", () => {
      expect(normalizeCombo("Shift+Cmd+D")).toBe("cmd+shift+d");
      expect(normalizeCombo("Cmd+Shift+D")).toBe("cmd+shift+d");
    });

    it("handles Alt modifier", () => {
      expect(normalizeCombo("Cmd+Alt+\\")).toBe("alt+cmd+\\");
    });

    it("normalizes single key (no modifiers)", () => {
      expect(normalizeCombo("Escape")).toBe("escape");
    });

    it("handles Ctrl as separate from Cmd", () => {
      expect(normalizeCombo("Cmd+Ctrl+1")).toBe("cmd+ctrl+1");
    });

    it("returns empty string for empty input", () => {
      expect(normalizeCombo("")).toBe("");
    });
  });
});

describe("keybindingsStore", () => {
  let store: ReturnType<typeof createKeybindingsStore>;

  beforeEach(() => {
    store = createKeybindingsStore();
    vi.clearAllMocks();
  });

  describe("defaults (no overrides)", () => {
    it("resolves default action for a known combo", () => {
      // Store not hydrated — should still have defaults
      expect(store.getActionForCombo("cmd+shift+d")).toBe("toggle-diff");
    });

    it("returns display key for an action", () => {
      expect(store.getKeyForAction("toggle-diff")).toBe("Cmd+Shift+D");
    });

    it("returns undefined for unknown combo", () => {
      expect(store.getActionForCombo("cmd+shift+z")).toBeUndefined();
    });

    it("returns undefined key for unknown action", () => {
      expect(store.getKeyForAction("nonexistent-action" as ActionName)).toBeUndefined();
    });
  });

  describe("hydrate with overrides", () => {
    it("applies user overrides from loaded config", async () => {
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === "load_keybindings") {
          return Promise.resolve([
            { action: "toggle-diff", key: "Cmd+Y" },
          ]);
        }
        return Promise.resolve(undefined);
      });

      await store.hydrate();

      // Override should work
      expect(store.getActionForCombo("cmd+y")).toBe("toggle-diff");
      // Old combo should no longer map to toggle-diff
      expect(store.getActionForCombo("cmd+shift+d")).toBeUndefined();
      // Display key should reflect override
      expect(store.getKeyForAction("toggle-diff")).toBe("Cmd+Y");
    });

    it("unbinds action when key is empty string", async () => {
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === "load_keybindings") {
          return Promise.resolve([
            { action: "toggle-diff", key: "" },
          ]);
        }
        return Promise.resolve(undefined);
      });

      await store.hydrate();

      expect(store.getActionForCombo("cmd+shift+d")).toBeUndefined();
      expect(store.getKeyForAction("toggle-diff")).toBeUndefined();
    });

    it("unbinds action when key is null", async () => {
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === "load_keybindings") {
          return Promise.resolve([
            { action: "toggle-diff", key: null },
          ]);
        }
        return Promise.resolve(undefined);
      });

      await store.hydrate();

      expect(store.getActionForCombo("cmd+shift+d")).toBeUndefined();
      expect(store.getKeyForAction("toggle-diff")).toBeUndefined();
    });

    it("ignores invalid action names in overrides", async () => {
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === "load_keybindings") {
          return Promise.resolve([
            { action: "bogus-action", key: "Cmd+Y" },
          ]);
        }
        return Promise.resolve(undefined);
      });

      await store.hydrate();

      // bogus-action should not appear in lookup
      expect(store.getActionForCombo("cmd+y")).toBeUndefined();
      // Existing defaults should still work
      expect(store.getActionForCombo("cmd+shift+d")).toBe("toggle-diff");
    });

    it("preserves unmodified defaults after hydrate", async () => {
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === "load_keybindings") {
          return Promise.resolve([
            { action: "toggle-diff", key: "Cmd+Y" },
          ]);
        }
        return Promise.resolve(undefined);
      });

      await store.hydrate();

      // Other defaults should still work
      expect(store.getActionForCombo("cmd+m")).toBe("toggle-markdown");
      expect(store.getActionForCombo("cmd+t")).toBe("new-terminal");
    });

    it("handles missing keybindings file gracefully", async () => {
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === "load_keybindings") {
          // File doesn't exist — returns empty array
          return Promise.resolve([]);
        }
        return Promise.resolve(undefined);
      });

      await store.hydrate();

      // All defaults should work
      expect(store.getActionForCombo("cmd+shift+d")).toBe("toggle-diff");
    });

    it("handles invoke failure gracefully", async () => {
      const debugSpy = vi.spyOn(console, "debug").mockImplementation(() => {});

      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === "load_keybindings") {
          return Promise.reject(new Error("file not found"));
        }
        return Promise.resolve(undefined);
      });

      // Should not throw
      await store.hydrate();

      // All defaults should still work
      expect(store.getActionForCombo("cmd+shift+d")).toBe("toggle-diff");
      // Verify the error was logged
      expect(debugSpy).toHaveBeenCalledWith(
        "Failed to load keybindings overrides:",
        expect.any(Error),
      );
      debugSpy.mockRestore();
    });
  });

  describe("getAllBindings", () => {
    it("returns all action-to-key mappings", () => {
      const all = store.getAllBindings();
      expect(all["toggle-diff"]).toBe("Cmd+Shift+D");
      expect(all["new-terminal"]).toBe("Cmd+T");
    });

    it("reflects overrides after hydrate", async () => {
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === "load_keybindings") {
          return Promise.resolve([
            { action: "toggle-diff", key: "Cmd+Y" },
          ]);
        }
        return Promise.resolve(undefined);
      });

      await store.hydrate();

      const all = store.getAllBindings();
      expect(all["toggle-diff"]).toBe("Cmd+Y");
    });
  });
});
