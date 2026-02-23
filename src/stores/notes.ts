import { createStore, produce } from "solid-js/store";
import { invoke } from "../invoke";

/** A single note */
export interface Note {
  id: string;
  text: string;
  createdAt: number;
  repoPath: string | null;
  repoDisplayName: string | null;
}

/** Notes store state */
interface NotesStoreState {
  notes: Note[];
}

/** Generate a unique note ID */
function generateId(): string {
  return `note-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
}

/** Persist notes to Rust backend (fire-and-forget) */
function saveNotes(notes: Note[]): void {
  invoke("save_notes", { config: { notes } }).catch((err) =>
    console.error("Failed to save notes:", err),
  );
}

/** Create the notes store */
function createNotesStore() {
  const [state, setState] = createStore<NotesStoreState>({
    notes: [],
  });

  const actions = {
    /** Load notes from Rust backend */
    async hydrate(): Promise<void> {
      try {
        const loaded = await invoke<{ notes?: Note[] }>("load_notes");
        if (loaded?.notes && Array.isArray(loaded.notes)) {
          const migrated = loaded.notes.map((n) => ({
            ...n,
            repoPath: n.repoPath ?? null,
            repoDisplayName: n.repoDisplayName ?? null,
          }));
          setState("notes", migrated);
        }
      } catch (err) {
        console.debug("Failed to hydrate notes:", err);
      }
    },

    /** Add a new note, optionally tagged with a repo */
    addNote(text: string, repoPath?: string | null, repoDisplayName?: string | null): void {
      const trimmed = text.trim();
      if (!trimmed) return;

      const note: Note = {
        id: generateId(),
        text: trimmed,
        createdAt: Date.now(),
        repoPath: repoPath ?? null,
        repoDisplayName: repoDisplayName ?? null,
      };

      setState(
        produce((s) => {
          s.notes.unshift(note);
        }),
      );
      saveNotes(state.notes);
    },

    /** Remove a note by ID */
    removeNote(id: string): void {
      setState("notes", (notes) => notes.filter((n) => n.id !== id));
      saveNotes(state.notes);
    },

    /** Reassign a note to a different project */
    reassignNote(id: string, repoPath: string | null, repoDisplayName: string | null): void {
      setState(
        produce((s) => {
          const note = s.notes.find((n) => n.id === id);
          if (note) {
            note.repoPath = repoPath;
            note.repoDisplayName = repoDisplayName;
          }
        }),
      );
      saveNotes(state.notes);
    },

    /** Get notes filtered by active repo. null = all notes. */
    getFilteredNotes(activeRepo: string | null): Note[] {
      if (!activeRepo) return state.notes;
      return state.notes.filter((n) => n.repoPath === null || n.repoPath === activeRepo);
    },

    /** Count of notes visible for the given repo filter */
    filteredCount(activeRepo: string | null): number {
      if (!activeRepo) return state.notes.length;
      return state.notes.filter((n) => n.repoPath === null || n.repoPath === activeRepo).length;
    },

    /** Get total note count */
    count(): number {
      return state.notes.length;
    },
  };

  return { state, ...actions };
}

export const notesStore = createNotesStore();
