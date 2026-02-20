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
        { id: "note-1", text: "from backend", createdAt: 1000 },
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
});
