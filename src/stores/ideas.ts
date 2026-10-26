import { createStore, produce } from "solid-js/store";
import { invoke } from "../invoke";
import { subscribeEvents } from "../transport";
import { createConfigDeltaWriter } from "../utils/configDeltaWriter";
import { appLogger } from "./appLogger";

/**
 * A single idea.
 *
 * The panel, the store and this type speak "idea"; the backend command names,
 * the `notes.json` payload field and the `note-` id prefix below still say
 * "note" on purpose. Those are on-disk and on-wire identifiers — renaming them
 * would need a migration, which this rename deliberately does not do. Every
 * translation between the two vocabularies happens at the IO boundary in
 * `hydrate` and `saveIdeas`, nowhere else.
 */
export interface Idea {
	id: string;
	text: string;
	createdAt: number;
	repoPath: string | null;
	repoDisplayName: string | null;
	usedAt: number | null;
	images: string[];
	source?: string;
}

/** Ideas store state */
interface IdeasStoreState {
	ideas: Idea[];
}

/** Generate a unique idea ID. The `note-` prefix names image asset directories
 *  on disk (`note-images/<id>/`), so it stays as it is. */
export function generateId(): string {
	return `note-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
}

/**
 * Set once `load_notes` has answered. Before that the store holds an empty array, and
 * persisting it would atomically overwrite notes.json with `[]` — a permanent, silent loss
 * (GH #107). Every mutation path funnels through saveIdeas, so gating here covers them all.
 */
let hydrated = false;
let pendingWrite: Promise<void> = Promise.resolve();
let mutationRevision = 0;
const notesWriter = createConfigDeltaWriter<{ notes: Idea[] }>("save_notes");

/** Persist ideas to Rust backend (fire-and-forget) */
function saveIdeas(ideas: Idea[]): void {
	if (!hydrated) {
		appLogger.error("store", "Refusing to persist ideas before a successful hydrate — notes.json left untouched");
		return;
	}
	mutationRevision++;
	pendingWrite = notesWriter
		.save({ notes: ideas })
		.catch((err) => appLogger.error("store", "Failed to save ideas", err));
}

/** Create the ideas store */
function createIdeasStore() {
	const [state, setState] = createStore<IdeasStoreState>({
		ideas: [],
	});

	const visibleIdeas = (activeRepo: string | null): Idea[] =>
		activeRepo ? state.ideas.filter((n) => n.repoPath === null || n.repoPath === activeRepo) : state.ideas;

	let syncStarted = false;
	let readSequence = 0;
	const actions = {
		/** Load ideas from Rust backend */
		async hydrate(): Promise<void> {
			if (!syncStarted) {
				syncStarted = true;
				await subscribeEvents(
					{
						"ideas-changed": () => {
							void actions.hydrate();
						},
					},
					{
						onResync: () => {
							void actions.hydrate();
						},
					},
				).catch((err) => {
					syncStarted = false;
					appLogger.error("store", "Failed to subscribe to idea changes", err);
				});
			}
			const sequence = ++readSequence;
			try {
				// A backend notice can arrive before the local write acknowledgement.
				// Finish all queued intent first, and discard reads raced by newer edits.
				const revision = mutationRevision;
				await pendingWrite;
				if (sequence !== readSequence) return;
				if (revision !== mutationRevision) {
					void actions.hydrate();
					return;
				}
				const loaded = await invoke<{ notes?: Idea[] }>("load_notes");
				if (sequence !== readSequence) return;
				if (revision !== mutationRevision) {
					void actions.hydrate();
					return;
				}
				notesWriter.loaded({ notes: loaded?.notes ?? [] });
				if (loaded?.notes && Array.isArray(loaded.notes)) {
					const migrated = loaded.notes.map((n) => ({
						...n,
						repoPath: n.repoPath ?? null,
						repoDisplayName: n.repoDisplayName ?? null,
						usedAt: n.usedAt ?? null,
						images: (n as Idea & { images?: string[] }).images ?? [],
					}));
					setState("ideas", migrated);
				} else {
					setState("ideas", []);
				}
				hydrated = true;
			} catch (err) {
				// Backend could not read notes.json (unreadable or corrupt). Stay un-hydrated so
				// the next mutation cannot overwrite the file we failed to read.
				appLogger.error("store", "Failed to hydrate ideas — ideas will not be saved this session", err);
			}
		},

		/** Add a new idea, optionally tagged with a repo and images */
		addIdea(
			text: string,
			repoPath?: string | null,
			repoDisplayName?: string | null,
			images?: string[],
			ideaId?: string,
		): void {
			const trimmed = text.trim();
			const imgs = images ?? [];
			if (!trimmed && imgs.length === 0) return;

			const idea: Idea = {
				id: ideaId ?? generateId(),
				text: trimmed,
				createdAt: Date.now(),
				repoPath: repoPath ?? null,
				repoDisplayName: repoDisplayName ?? null,
				usedAt: null,
				images: imgs,
			};

			setState(
				produce((s) => {
					s.ideas.unshift(idea);
				}),
			);
			saveIdeas(state.ideas);
		},

		/** Remove an idea by ID, cleaning up image assets on disk */
		removeIdea(id: string): void {
			setState("ideas", (ideas) => ideas.filter((n) => n.id !== id));
			saveIdeas(state.ideas);
			invoke("delete_note_assets", { noteId: id }).catch((err) =>
				appLogger.debug("store", "Failed to delete idea assets", err),
			);
		},

		/** Update an idea in-place, preserving id, createdAt, and repo assignment */
		updateIdea(id: string, text: string, images: string[]): void {
			setState(
				produce((s) => {
					const idea = s.ideas.find((n) => n.id === id);
					if (idea) {
						idea.text = text.trim();
						idea.images = images;
					}
				}),
			);
			saveIdeas(state.ideas);
		},

		/** Reassign an idea to a different project */
		reassignIdea(id: string, repoPath: string | null, repoDisplayName: string | null): void {
			setState(
				produce((s) => {
					const idea = s.ideas.find((n) => n.id === id);
					if (idea) {
						idea.repoPath = repoPath;
						idea.repoDisplayName = repoDisplayName;
					}
				}),
			);
			saveIdeas(state.ideas);
		},

		/** Mark an idea as used (sent to or queued for a terminal) */
		markUsed(id: string): void {
			setState(
				produce((s) => {
					const idea = s.ideas.find((n) => n.id === id);
					if (idea) {
						idea.usedAt = Date.now();
					}
				}),
			);
			saveIdeas(state.ideas);
		},

		/** Get ideas filtered by active repo. null = all ideas. */
		getFilteredIdeas(activeRepo: string | null): Idea[] {
			return visibleIdeas(activeRepo);
		},

		/** Count of ideas visible for the given repo filter */
		filteredCount(activeRepo: string | null): number {
			if (!activeRepo) return state.ideas.length;
			return state.ideas.filter((n) => n.repoPath === null || n.repoPath === activeRepo).length;
		},

		/** Count of pending (not yet used) ideas for the given repo filter */
		pendingCount(activeRepo: string | null): number {
			return state.ideas.filter((n) => !n.usedAt && (!activeRepo || n.repoPath === null || n.repoPath === activeRepo))
				.length;
		},

		/** Remove the used ideas (usedAt !== null) visible for the repo filter. null = all. */
		clearCompleted(activeRepo: string | null = null): void {
			const completed = visibleIdeas(activeRepo).filter((n) => n.usedAt !== null);
			if (completed.length === 0) return;
			const completedIds = completed.map((n) => n.id);
			setState("ideas", (ideas) => ideas.filter((n) => !completedIds.includes(n.id)));
			saveIdeas(state.ideas);
			invoke("delete_note_assets_batch", { noteIds: completedIds }).catch((err) =>
				appLogger.debug("store", "Failed to delete idea assets", err),
			);
		},

		/** Get total idea count */
		count(): number {
			return state.ideas.length;
		},
	};

	return { state, ...actions };
}

export const ideasStore = createIdeasStore();

// Debug registry — expose ideas for MCP introspection. The snapshot key is a
// debug-surface identifier, kept as it is so existing queries keep working.
import { registerDebugSnapshot } from "./debugRegistry";

registerDebugSnapshot("notes", () => {
	return ideasStore.state.ideas.map((n) => ({
		id: n.id,
		text: n.text.substring(0, 100),
		repoPath: n.repoPath,
		createdAt: n.createdAt,
	}));
});
