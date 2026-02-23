import { describe, it, expect, vi, beforeEach } from "vitest";
import { createRoot } from "solid-js";

const mockInvoke = vi.fn().mockResolvedValue(undefined);

vi.mock("@tauri-apps/api/core", () => ({
  invoke: mockInvoke,
}));

describe("notesStore", () => {
  let store: typeof import("../../stores/notes").notesStore;

  beforeEach(async () => {
    vi.resetModules();
    mockInvoke.mockReset().mockResolvedValue(undefined);

    vi.doMock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));

    store = (await import("../../stores/notes")).notesStore;
  });

  describe("addNote()", () => {
    it("adds a note with trimmed text", () => {
      createRoot((dispose) => {
        store.addNote("  hello world  ");
        expect(store.state.notes[0].text).toBe("hello world");
        dispose();
      });
    });

    it("ignores empty string (after trim)", () => {
      createRoot((dispose) => {
        store.addNote("   ");
        expect(store.state.notes.length).toBe(0);
        dispose();
      });
    });

    it("ignores empty string", () => {
      createRoot((dispose) => {
        store.addNote("");
        expect(store.state.notes.length).toBe(0);
        dispose();
      });
    });

    it("prepends: most recent note is first", () => {
      createRoot((dispose) => {
        store.addNote("first");
        store.addNote("second");
        expect(store.state.notes[0].text).toBe("second");
        expect(store.state.notes[1].text).toBe("first");
        dispose();
      });
    });

    it("assigns a unique id to each note", () => {
      createRoot((dispose) => {
        store.addNote("a");
        store.addNote("b");
        expect(store.state.notes[0].id).not.toBe(store.state.notes[1].id);
        dispose();
      });
    });

    it("persists via invoke save_notes", () => {
      createRoot((dispose) => {
        store.addNote("saved note");
        expect(mockInvoke).toHaveBeenCalledWith("save_notes", {
          config: { notes: expect.arrayContaining([expect.objectContaining({ text: "saved note" })]) },
        });
        dispose();
      });
    });
  });

  describe("removeNote()", () => {
    it("removes the note by id", () => {
      createRoot((dispose) => {
        store.addNote("to remove");
        const id = store.state.notes[0].id;
        store.removeNote(id);
        expect(store.state.notes.length).toBe(0);
        dispose();
      });
    });

    it("only removes the matching note", () => {
      createRoot((dispose) => {
        store.addNote("keep me");
        store.addNote("remove me");
        const idToRemove = store.state.notes[0].id; // most recent
        store.removeNote(idToRemove);
        expect(store.state.notes.length).toBe(1);
        expect(store.state.notes[0].text).toBe("keep me");
        dispose();
      });
    });

    it("ignores unknown id without error", () => {
      createRoot((dispose) => {
        store.addNote("note");
        expect(() => store.removeNote("nonexistent")).not.toThrow();
        expect(store.state.notes.length).toBe(1);
        dispose();
      });
    });

    it("persists via invoke save_notes", () => {
      createRoot((dispose) => {
        store.addNote("note");
        mockInvoke.mockClear();
        const id = store.state.notes[0].id;
        store.removeNote(id);
        expect(mockInvoke).toHaveBeenCalledWith("save_notes", {
          config: { notes: [] },
        });
        dispose();
      });
    });
  });

  describe("hydrate()", () => {
    it("loads notes from backend", async () => {
      const savedNotes = [
        { id: "note-1", text: "from backend", createdAt: 1000, repoPath: null, repoDisplayName: null },
      ];
      mockInvoke.mockResolvedValueOnce({ notes: savedNotes });

      await createRoot(async (dispose) => {
        await store.hydrate();
        expect(store.state.notes).toEqual(savedNotes);
        expect(mockInvoke).toHaveBeenCalledWith("load_notes");
        dispose();
      });
    });

    it("keeps empty state when backend returns null", async () => {
      mockInvoke.mockResolvedValueOnce(null);

      await createRoot(async (dispose) => {
        await store.hydrate();
        expect(store.state.notes).toEqual([]);
        dispose();
      });
    });

    it("keeps empty state on invoke failure", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("backend error"));

      await createRoot(async (dispose) => {
        await store.hydrate();
        expect(store.state.notes).toEqual([]);
        dispose();
      });
    });
  });

  describe("count()", () => {
    it("returns 0 initially", () => {
      createRoot((dispose) => {
        expect(store.count()).toBe(0);
        dispose();
      });
    });

    it("increments on add", () => {
      createRoot((dispose) => {
        store.addNote("a");
        store.addNote("b");
        expect(store.count()).toBe(2);
        dispose();
      });
    });
  });

  describe("addNote() with repo context", () => {
    it("saves repoPath and repoDisplayName when provided", () => {
      createRoot((dispose) => {
        store.addNote("idea", "/Users/foo/project-x", "project-x");
        const note = store.state.notes[0];
        expect(note.repoPath).toBe("/Users/foo/project-x");
        expect(note.repoDisplayName).toBe("project-x");
        dispose();
      });
    });

    it("defaults repoPath and repoDisplayName to null when not provided", () => {
      createRoot((dispose) => {
        store.addNote("global idea");
        const note = store.state.notes[0];
        expect(note.repoPath).toBeNull();
        expect(note.repoDisplayName).toBeNull();
        dispose();
      });
    });

    it("persists repo fields via save_notes", () => {
      createRoot((dispose) => {
        store.addNote("tagged", "/path/repo", "repo");
        expect(mockInvoke).toHaveBeenCalledWith("save_notes", {
          config: {
            notes: expect.arrayContaining([
              expect.objectContaining({
                text: "tagged",
                repoPath: "/path/repo",
                repoDisplayName: "repo",
              }),
            ]),
          },
        });
        dispose();
      });
    });
  });

  describe("reassignNote()", () => {
    it("updates repoPath and repoDisplayName", () => {
      createRoot((dispose) => {
        store.addNote("idea", "/old/repo", "old-repo");
        const id = store.state.notes[0].id;
        store.reassignNote(id, "/new/repo", "new-repo");
        expect(store.state.notes[0].repoPath).toBe("/new/repo");
        expect(store.state.notes[0].repoDisplayName).toBe("new-repo");
        dispose();
      });
    });

    it("can reassign to global (null)", () => {
      createRoot((dispose) => {
        store.addNote("idea", "/some/repo", "repo");
        const id = store.state.notes[0].id;
        store.reassignNote(id, null, null);
        expect(store.state.notes[0].repoPath).toBeNull();
        expect(store.state.notes[0].repoDisplayName).toBeNull();
        dispose();
      });
    });

    it("persists after reassign", () => {
      createRoot((dispose) => {
        store.addNote("idea", "/old", "old");
        mockInvoke.mockClear();
        const id = store.state.notes[0].id;
        store.reassignNote(id, "/new", "new");
        expect(mockInvoke).toHaveBeenCalledWith("save_notes", expect.anything());
        dispose();
      });
    });

    it("ignores unknown id", () => {
      createRoot((dispose) => {
        store.addNote("idea");
        expect(() => store.reassignNote("nonexistent", "/x", "x")).not.toThrow();
        dispose();
      });
    });
  });

  describe("getFilteredNotes()", () => {
    it("returns all notes when activeRepo is null", () => {
      createRoot((dispose) => {
        store.addNote("global");
        store.addNote("tagged", "/repo/a", "a");
        store.addNote("tagged2", "/repo/b", "b");
        expect(store.getFilteredNotes(null)).toHaveLength(3);
        dispose();
      });
    });

    it("returns matching + global notes when activeRepo is set", () => {
      createRoot((dispose) => {
        store.addNote("global");
        store.addNote("repo-a", "/repo/a", "a");
        store.addNote("repo-b", "/repo/b", "b");
        const filtered = store.getFilteredNotes("/repo/a");
        expect(filtered).toHaveLength(2);
        expect(filtered.map((n) => n.text).sort()).toEqual(["global", "repo-a"]);
        dispose();
      });
    });

    it("includes notes with null repoPath (global) in any filter", () => {
      createRoot((dispose) => {
        store.addNote("always visible");
        const filtered = store.getFilteredNotes("/any/repo");
        expect(filtered).toHaveLength(1);
        expect(filtered[0].text).toBe("always visible");
        dispose();
      });
    });
  });

  describe("filteredCount()", () => {
    it("returns total count when activeRepo is null", () => {
      createRoot((dispose) => {
        store.addNote("a");
        store.addNote("b", "/repo", "repo");
        expect(store.filteredCount(null)).toBe(2);
        dispose();
      });
    });

    it("returns filtered count when activeRepo is set", () => {
      createRoot((dispose) => {
        store.addNote("global");
        store.addNote("match", "/repo/a", "a");
        store.addNote("other", "/repo/b", "b");
        expect(store.filteredCount("/repo/a")).toBe(2); // match + global
        dispose();
      });
    });
  });

  describe("hydrate() migration", () => {
    it("fills missing repoPath/repoDisplayName with null", async () => {
      const legacyNotes = [
        { id: "note-1", text: "legacy", createdAt: 1000 },
      ];
      mockInvoke.mockResolvedValueOnce({ notes: legacyNotes });

      await createRoot(async (dispose) => {
        await store.hydrate();
        expect(store.state.notes[0].repoPath).toBeNull();
        expect(store.state.notes[0].repoDisplayName).toBeNull();
        dispose();
      });
    });
  });
});
