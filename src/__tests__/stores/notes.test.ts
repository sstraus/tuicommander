import { describe, it, expect, vi, beforeEach } from "vitest";
import { testInScope, testInScopeAsync } from "../helpers/store";

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
      testInScope(() => {
        store.addNote("  hello world  ");
        expect(store.state.notes[0].text).toBe("hello world");
      });
    });

    it("ignores empty string (after trim)", () => {
      testInScope(() => {
        store.addNote("   ");
        expect(store.state.notes.length).toBe(0);
      });
    });

    it("ignores empty string", () => {
      testInScope(() => {
        store.addNote("");
        expect(store.state.notes.length).toBe(0);
      });
    });

    it("prepends: most recent note is first", () => {
      testInScope(() => {
        store.addNote("first");
        store.addNote("second");
        expect(store.state.notes[0].text).toBe("second");
        expect(store.state.notes[1].text).toBe("first");
      });
    });

    it("assigns a unique id to each note", () => {
      testInScope(() => {
        store.addNote("a");
        store.addNote("b");
        expect(store.state.notes[0].id).not.toBe(store.state.notes[1].id);
      });
    });

    it("persists via invoke save_notes", () => {
      testInScope(() => {
        store.addNote("saved note");
        expect(mockInvoke).toHaveBeenCalledWith("save_notes", {
          config: { notes: expect.arrayContaining([expect.objectContaining({ text: "saved note" })]) },
        });
      });
    });
  });

  describe("removeNote()", () => {
    it("removes the note by id", () => {
      testInScope(() => {
        store.addNote("to remove");
        const id = store.state.notes[0].id;
        store.removeNote(id);
        expect(store.state.notes.length).toBe(0);
      });
    });

    it("only removes the matching note", () => {
      testInScope(() => {
        store.addNote("keep me");
        store.addNote("remove me");
        const idToRemove = store.state.notes[0].id; // most recent
        store.removeNote(idToRemove);
        expect(store.state.notes.length).toBe(1);
        expect(store.state.notes[0].text).toBe("keep me");
      });
    });

    it("ignores unknown id without error", () => {
      testInScope(() => {
        store.addNote("note");
        expect(() => store.removeNote("nonexistent")).not.toThrow();
        expect(store.state.notes.length).toBe(1);
      });
    });

    it("persists via invoke save_notes", () => {
      testInScope(() => {
        store.addNote("note");
        mockInvoke.mockClear();
        const id = store.state.notes[0].id;
        store.removeNote(id);
        expect(mockInvoke).toHaveBeenCalledWith("save_notes", {
          config: { notes: [] },
        });
      });
    });
  });

  describe("hydrate()", () => {
    it("loads notes from backend", async () => {
      const savedNotes = [
        { id: "note-1", text: "from backend", createdAt: 1000, repoPath: null, repoDisplayName: null, usedAt: null, images: [] },
      ];
      mockInvoke.mockResolvedValueOnce({ notes: savedNotes });

      await testInScopeAsync(async () => {
        await store.hydrate();
        expect(store.state.notes).toEqual(savedNotes);
        expect(mockInvoke).toHaveBeenCalledWith("load_notes");
      });
    });

    it("keeps empty state when backend returns null", async () => {
      mockInvoke.mockResolvedValueOnce(null);

      await testInScopeAsync(async () => {
        await store.hydrate();
        expect(store.state.notes).toEqual([]);
      });
    });

    it("keeps empty state on invoke failure", async () => {
      const consoleSpy = vi.spyOn(console, "debug").mockImplementation(() => {});
      mockInvoke.mockRejectedValueOnce(new Error("backend error"));

      await testInScopeAsync(async () => {
        await store.hydrate();
        expect(store.state.notes).toEqual([]);
        expect(consoleSpy).toHaveBeenCalledWith("[store]", "Failed to hydrate notes", expect.any(Error));
      });
      consoleSpy.mockRestore();
    });
  });

  describe("count()", () => {
    it("returns 0 initially", () => {
      testInScope(() => {
        expect(store.count()).toBe(0);
      });
    });

    it("increments on add", () => {
      testInScope(() => {
        store.addNote("a");
        store.addNote("b");
        expect(store.count()).toBe(2);
      });
    });
  });

  describe("addNote() with repo context", () => {
    it("saves repoPath and repoDisplayName when provided", () => {
      testInScope(() => {
        store.addNote("idea", "/Users/foo/project-x", "project-x");
        const note = store.state.notes[0];
        expect(note.repoPath).toBe("/Users/foo/project-x");
        expect(note.repoDisplayName).toBe("project-x");
      });
    });

    it("defaults repoPath and repoDisplayName to null when not provided", () => {
      testInScope(() => {
        store.addNote("global idea");
        const note = store.state.notes[0];
        expect(note.repoPath).toBeNull();
        expect(note.repoDisplayName).toBeNull();
      });
    });

    it("persists repo fields via save_notes", () => {
      testInScope(() => {
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
      });
    });
  });

  describe("reassignNote()", () => {
    it("updates repoPath and repoDisplayName", () => {
      testInScope(() => {
        store.addNote("idea", "/old/repo", "old-repo");
        const id = store.state.notes[0].id;
        store.reassignNote(id, "/new/repo", "new-repo");
        expect(store.state.notes[0].repoPath).toBe("/new/repo");
        expect(store.state.notes[0].repoDisplayName).toBe("new-repo");
      });
    });

    it("can reassign to global (null)", () => {
      testInScope(() => {
        store.addNote("idea", "/some/repo", "repo");
        const id = store.state.notes[0].id;
        store.reassignNote(id, null, null);
        expect(store.state.notes[0].repoPath).toBeNull();
        expect(store.state.notes[0].repoDisplayName).toBeNull();
      });
    });

    it("persists after reassign", () => {
      testInScope(() => {
        store.addNote("idea", "/old", "old");
        mockInvoke.mockClear();
        const id = store.state.notes[0].id;
        store.reassignNote(id, "/new", "new");
        expect(mockInvoke).toHaveBeenCalledWith("save_notes", expect.anything());
      });
    });

    it("ignores unknown id", () => {
      testInScope(() => {
        store.addNote("idea");
        expect(() => store.reassignNote("nonexistent", "/x", "x")).not.toThrow();
      });
    });
  });

  describe("getFilteredNotes()", () => {
    it("returns all notes when activeRepo is null", () => {
      testInScope(() => {
        store.addNote("global");
        store.addNote("tagged", "/repo/a", "a");
        store.addNote("tagged2", "/repo/b", "b");
        expect(store.getFilteredNotes(null)).toHaveLength(3);
      });
    });

    it("returns matching + global notes when activeRepo is set", () => {
      testInScope(() => {
        store.addNote("global");
        store.addNote("repo-a", "/repo/a", "a");
        store.addNote("repo-b", "/repo/b", "b");
        const filtered = store.getFilteredNotes("/repo/a");
        expect(filtered).toHaveLength(2);
        expect(filtered.map((n) => n.text).sort()).toEqual(["global", "repo-a"]);
      });
    });

    it("includes notes with null repoPath (global) in any filter", () => {
      testInScope(() => {
        store.addNote("always visible");
        const filtered = store.getFilteredNotes("/any/repo");
        expect(filtered).toHaveLength(1);
        expect(filtered[0].text).toBe("always visible");
      });
    });
  });

  describe("filteredCount()", () => {
    it("returns total count when activeRepo is null", () => {
      testInScope(() => {
        store.addNote("a");
        store.addNote("b", "/repo", "repo");
        expect(store.filteredCount(null)).toBe(2);
      });
    });

    it("returns filtered count when activeRepo is set", () => {
      testInScope(() => {
        store.addNote("global");
        store.addNote("match", "/repo/a", "a");
        store.addNote("other", "/repo/b", "b");
        expect(store.filteredCount("/repo/a")).toBe(2); // match + global
      });
    });
  });

  describe("hydrate() migration", () => {
    it("fills missing repoPath, repoDisplayName, and usedAt with null", async () => {
      const legacyNotes = [
        { id: "note-1", text: "legacy", createdAt: 1000 },
      ];
      mockInvoke.mockResolvedValueOnce({ notes: legacyNotes });

      await testInScopeAsync(async () => {
        await store.hydrate();
        expect(store.state.notes[0].repoPath).toBeNull();
        expect(store.state.notes[0].repoDisplayName).toBeNull();
        expect(store.state.notes[0].usedAt).toBeNull();
      });
    });
  });

  describe("addNote() with images", () => {
    it("stores images array when provided", () => {
      testInScope(() => {
        store.addNote("idea with image", null, null, ["/path/img.png"]);
        expect(store.state.notes[0].images).toEqual(["/path/img.png"]);
      });
    });

    it("defaults images to empty array when not provided", () => {
      testInScope(() => {
        store.addNote("plain idea");
        expect(store.state.notes[0].images).toEqual([]);
      });
    });

    it("allows image-only notes (no text)", () => {
      testInScope(() => {
        store.addNote("", null, null, ["/path/img.png"]);
        expect(store.state.notes.length).toBe(1);
        expect(store.state.notes[0].text).toBe("");
        expect(store.state.notes[0].images).toEqual(["/path/img.png"]);
      });
    });

    it("rejects notes with no text AND no images", () => {
      testInScope(() => {
        store.addNote("", null, null, []);
        expect(store.state.notes.length).toBe(0);
      });
    });

    it("accepts optional noteId parameter", () => {
      testInScope(() => {
        store.addNote("with id", null, null, [], "custom-id-123");
        expect(store.state.notes[0].id).toBe("custom-id-123");
      });
    });
  });

  describe("updateNote()", () => {
    it("updates text in-place preserving id and createdAt", () => {
      testInScope(() => {
        store.addNote("original");
        const note = store.state.notes[0];
        const { id, createdAt } = note;
        store.updateNote(id, "updated text", []);
        expect(store.state.notes[0].id).toBe(id);
        expect(store.state.notes[0].createdAt).toBe(createdAt);
        expect(store.state.notes[0].text).toBe("updated text");
      });
    });

    it("updates images in-place", () => {
      testInScope(() => {
        store.addNote("idea", null, null, ["/old.png"]);
        const id = store.state.notes[0].id;
        store.updateNote(id, "idea", ["/old.png", "/new.png"]);
        expect(store.state.notes[0].images).toEqual(["/old.png", "/new.png"]);
      });
    });

    it("preserves repoPath and repoDisplayName", () => {
      testInScope(() => {
        store.addNote("idea", "/repo", "my-repo");
        const id = store.state.notes[0].id;
        store.updateNote(id, "updated", []);
        expect(store.state.notes[0].repoPath).toBe("/repo");
        expect(store.state.notes[0].repoDisplayName).toBe("my-repo");
      });
    });

    it("persists via save_notes", () => {
      testInScope(() => {
        store.addNote("idea");
        mockInvoke.mockClear();
        store.updateNote(store.state.notes[0].id, "updated", []);
        expect(mockInvoke).toHaveBeenCalledWith("save_notes", expect.anything());
      });
    });

    it("ignores unknown id", () => {
      testInScope(() => {
        store.addNote("idea");
        expect(() => store.updateNote("nonexistent", "x", [])).not.toThrow();
      });
    });
  });

  describe("removeNote() with image cleanup", () => {
    it("calls delete_note_assets on removal", () => {
      testInScope(() => {
        store.addNote("to remove", null, null, ["/img.png"]);
        const id = store.state.notes[0].id;
        mockInvoke.mockClear();
        store.removeNote(id);
        expect(mockInvoke).toHaveBeenCalledWith("delete_note_assets", { noteId: id });
      });
    });
  });

  describe("hydrate() images migration", () => {
    it("defaults missing images field to empty array", async () => {
      const legacyNotes = [
        { id: "note-1", text: "old note", createdAt: 1000, repoPath: null, repoDisplayName: null, usedAt: null },
      ];
      mockInvoke.mockResolvedValueOnce({ notes: legacyNotes });

      await testInScopeAsync(async () => {
        await store.hydrate();
        expect(store.state.notes[0].images).toEqual([]);
      });
    });

    it("preserves existing images field", async () => {
      const notes = [
        { id: "note-1", text: "with image", createdAt: 1000, repoPath: null, repoDisplayName: null, usedAt: null, images: ["/path/img.png"] },
      ];
      mockInvoke.mockResolvedValueOnce({ notes });

      await testInScopeAsync(async () => {
        await store.hydrate();
        expect(store.state.notes[0].images).toEqual(["/path/img.png"]);
      });
    });
  });

  describe("markUsed()", () => {
    it("sets usedAt timestamp on the note", () => {
      testInScope(() => {
        store.addNote("idea");
        const id = store.state.notes[0].id;
        expect(store.state.notes[0].usedAt).toBeNull();
        const before = Date.now();
        store.markUsed(id);
        const after = Date.now();
        expect(store.state.notes[0].usedAt).toBeGreaterThanOrEqual(before);
        expect(store.state.notes[0].usedAt).toBeLessThanOrEqual(after);
      });
    });

    it("persists after marking used", () => {
      testInScope(() => {
        store.addNote("idea");
        mockInvoke.mockClear();
        store.markUsed(store.state.notes[0].id);
        expect(mockInvoke).toHaveBeenCalledWith("save_notes", expect.anything());
      });
    });

    it("ignores unknown id", () => {
      testInScope(() => {
        store.addNote("idea");
        expect(() => store.markUsed("nonexistent")).not.toThrow();
      });
    });
  });
});
