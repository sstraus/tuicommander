import { createStore, produce } from "solid-js/store";
import { invoke } from "../invoke";

/** A single note */
export interface Note {
  id: string;
  text: string;
  createdAt: number;
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
          setState("notes", loaded.notes);
        }
      } catch (err) {
        console.debug("Failed to hydrate notes:", err);
      }
    },

    /** Add a new note */
    addNote(text: string): void {
      const trimmed = text.trim();
      if (!trimmed) return;

      const note: Note = {
        id: generateId(),
        text: trimmed,
        createdAt: Date.now(),
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

    /** Get note count */
    count(): number {
      return state.notes.length;
    },
  };

  return { state, ...actions };
}

export const notesStore = createNotesStore();
