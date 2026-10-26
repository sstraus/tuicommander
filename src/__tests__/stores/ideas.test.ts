import { beforeEach, describe, expect, it, vi } from "vitest";
import { testInScope, testInScopeAsync } from "../helpers/store";

const mockInvoke = vi.fn().mockResolvedValue(undefined);
const ideaEvents = vi.hoisted(() => ({ handlers: {} as Record<string, () => void>, resync: () => {} }));
vi.mock("../../transport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../transport")>()),
	subscribeEvents: vi.fn(async (handlers: Record<string, () => void>, options: { onResync: () => void }) => {
		ideaEvents.handlers = handlers;
		ideaEvents.resync = options.onResync;
		return () => {};
	}),
}));

vi.mock("@tauri-apps/api/core", () => ({
	invoke: mockInvoke,
}));

describe("ideasStore", () => {
	let store: typeof import("../../stores/ideas").ideasStore;

	beforeEach(async () => {
		vi.resetModules();
		mockInvoke.mockReset().mockResolvedValue(undefined);

		vi.doMock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));

		store = (await import("../../stores/ideas")).ideasStore;
		// Mirror the real boot order (useAppBootstrap hydrates before any UI can mutate).
		// Without a successful hydrate the store refuses to persist — see GH #107 below.
		await store.hydrate();
		mockInvoke.mockClear();
	});

	// Catches: a notice reload being persisted back, or overwriting an unacknowledged UI edit.
	it("reloads backend additions after pending writes without saving the reload", async () => {
		let acknowledge!: () => void;
		mockInvoke.mockImplementation((command: string) => {
			if (command === "save_notes")
				return new Promise<void>((resolve) => {
					acknowledge = resolve;
				});
			return Promise.resolve({
				notes: [
					{ id: "agent", text: "Agent idea", createdAt: 2, repoPath: "/repo", source: "ego", images: [] },
					{ id: "local", text: "Local edit", createdAt: 1, images: [] },
				],
			});
		});
		store.addIdea("Local edit", null, null, [], "local");
		ideaEvents.handlers["ideas-changed"]();
		await Promise.resolve();
		expect(store.state.ideas.map((idea) => idea.id)).toEqual(["local"]);
		expect(mockInvoke).not.toHaveBeenCalledWith("load_notes");
		acknowledge();
		await vi.waitFor(() => expect(store.state.ideas.map((idea) => idea.id)).toEqual(["agent", "local"]));
		expect(store.state.ideas[0].source).toBe("ego");
		expect(mockInvoke.mock.calls.filter(([command]) => command === "save_notes")).toHaveLength(1);
		mockInvoke.mockClear();
		ideaEvents.resync();
		await vi.waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("load_notes"));
		expect(mockInvoke.mock.calls.filter(([command]) => command === "save_notes")).toHaveLength(0);
	});

	describe("addIdea()", () => {
		it("adds a note with trimmed text", () => {
			testInScope(() => {
				store.addIdea("  hello world  ");
				expect(store.state.ideas[0].text).toBe("hello world");
			});
		});

		it("ignores empty string (after trim)", () => {
			testInScope(() => {
				store.addIdea("   ");
				expect(store.state.ideas.length).toBe(0);
			});
		});

		it("ignores empty string", () => {
			testInScope(() => {
				store.addIdea("");
				expect(store.state.ideas.length).toBe(0);
			});
		});

		it("prepends: most recent note is first", () => {
			testInScope(() => {
				store.addIdea("first");
				store.addIdea("second");
				expect(store.state.ideas[0].text).toBe("second");
				expect(store.state.ideas[1].text).toBe("first");
			});
		});

		it("assigns a unique id to each note", () => {
			testInScope(() => {
				store.addIdea("a");
				store.addIdea("b");
				expect(store.state.ideas[0].id).not.toBe(store.state.ideas[1].id);
			});
		});

		it("persists via invoke save_notes", async () => {
			await testInScopeAsync(async () => {
				store.addIdea("saved note");
				await vi.waitFor(() =>
					expect(mockInvoke).toHaveBeenCalledWith("save_notes", {
						base: expect.anything(),
						config: { notes: expect.arrayContaining([expect.objectContaining({ text: "saved note" })]) },
					}),
				);
			});
		});
	});

	describe("removeIdea()", () => {
		it("removes the note by id", () => {
			testInScope(() => {
				store.addIdea("to remove");
				const id = store.state.ideas[0].id;
				store.removeIdea(id);
				expect(store.state.ideas.length).toBe(0);
			});
		});

		it("only removes the matching note", () => {
			testInScope(() => {
				store.addIdea("keep me");
				store.addIdea("remove me");
				const idToRemove = store.state.ideas[0].id; // most recent
				store.removeIdea(idToRemove);
				expect(store.state.ideas.length).toBe(1);
				expect(store.state.ideas[0].text).toBe("keep me");
			});
		});

		it("ignores unknown id without error", () => {
			testInScope(() => {
				store.addIdea("note");
				expect(() => store.removeIdea("nonexistent")).not.toThrow();
				expect(store.state.ideas.length).toBe(1);
			});
		});

		it("persists via invoke save_notes", async () => {
			await testInScopeAsync(async () => {
				store.addIdea("note");
				mockInvoke.mockClear();
				const id = store.state.ideas[0].id;
				store.removeIdea(id);
				await vi.waitFor(() =>
					expect(mockInvoke).toHaveBeenCalledWith("save_notes", { base: expect.anything(), config: { notes: [] } }),
				);
			});
		});
	});

	describe("hydrate()", () => {
		it("loads notes from backend", async () => {
			const savedNotes = [
				{
					id: "note-1",
					text: "from backend",
					createdAt: 1000,
					repoPath: null,
					repoDisplayName: null,
					usedAt: null,
					images: [],
				},
			];
			mockInvoke.mockResolvedValueOnce({ notes: savedNotes });

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.state.ideas).toEqual(savedNotes);
				expect(mockInvoke).toHaveBeenCalledWith("load_notes");
			});
		});

		it("keeps empty state when backend returns null", async () => {
			mockInvoke.mockResolvedValueOnce(null);

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.state.ideas).toEqual([]);
			});
		});

		it("keeps empty state on invoke failure and reports it as an error", async () => {
			const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});
			mockInvoke.mockRejectedValueOnce(new Error("backend error"));

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.state.ideas).toEqual([]);
				expect(consoleSpy).toHaveBeenCalledWith("[store]", expect.stringContaining("Failed to hydrate ideas"));
			});
			consoleSpy.mockRestore();
		});
	});

	// GH #107: after a failed hydrate the store holds [], and persisting that would
	// atomically overwrite notes.json with an empty array — a permanent, silent loss.
	describe("persistence guard after a failed hydrate", () => {
		async function freshStore(invokeImpl: () => Promise<unknown>) {
			vi.resetModules();
			mockInvoke.mockReset().mockImplementation(invokeImpl);
			vi.doMock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));
			return (await import("../../stores/ideas")).ideasStore;
		}

		it("does not call save_notes for a mutation issued after a failed hydrate", async () => {
			const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});
			const fresh = await freshStore(() => Promise.reject(new Error("notes.json unreadable")));

			await testInScopeAsync(async () => {
				await fresh.hydrate();
				fresh.addIdea("must not wipe notes.json");
				expect(fresh.state.ideas.length).toBe(1);
				expect(mockInvoke).not.toHaveBeenCalledWith("save_notes", expect.anything());
			});
			consoleSpy.mockRestore();
		});

		it("does persist after a successful hydrate of a missing/empty file", async () => {
			const fresh = await freshStore(() => Promise.resolve(null));

			await testInScopeAsync(async () => {
				await fresh.hydrate();
				fresh.addIdea("kept");
				expect(mockInvoke).toHaveBeenCalledWith("save_notes", expect.anything());
			});
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
				store.addIdea("a");
				store.addIdea("b");
				expect(store.count()).toBe(2);
			});
		});
	});

	describe("addIdea() with repo context", () => {
		it("saves repoPath and repoDisplayName when provided", () => {
			testInScope(() => {
				store.addIdea("idea", "/Users/foo/project-x", "project-x");
				const note = store.state.ideas[0];
				expect(note.repoPath).toBe("/Users/foo/project-x");
				expect(note.repoDisplayName).toBe("project-x");
			});
		});

		it("defaults repoPath and repoDisplayName to null when not provided", () => {
			testInScope(() => {
				store.addIdea("global idea");
				const note = store.state.ideas[0];
				expect(note.repoPath).toBeNull();
				expect(note.repoDisplayName).toBeNull();
			});
		});

		it("persists repo fields via save_notes", () => {
			testInScope(() => {
				store.addIdea("tagged", "/path/repo", "repo");
				expect(mockInvoke).toHaveBeenCalledWith("save_notes", {
					base: expect.anything(),
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

	describe("reassignIdea()", () => {
		it("updates repoPath and repoDisplayName", () => {
			testInScope(() => {
				store.addIdea("idea", "/old/repo", "old-repo");
				const id = store.state.ideas[0].id;
				store.reassignIdea(id, "/new/repo", "new-repo");
				expect(store.state.ideas[0].repoPath).toBe("/new/repo");
				expect(store.state.ideas[0].repoDisplayName).toBe("new-repo");
			});
		});

		it("can reassign to global (null)", () => {
			testInScope(() => {
				store.addIdea("idea", "/some/repo", "repo");
				const id = store.state.ideas[0].id;
				store.reassignIdea(id, null, null);
				expect(store.state.ideas[0].repoPath).toBeNull();
				expect(store.state.ideas[0].repoDisplayName).toBeNull();
			});
		});

		it("persists after reassign", async () => {
			await testInScopeAsync(async () => {
				store.addIdea("idea", "/old", "old");
				mockInvoke.mockClear();
				const id = store.state.ideas[0].id;
				store.reassignIdea(id, "/new", "new");
				await vi.waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("save_notes", expect.anything()));
			});
		});

		it("ignores unknown id", () => {
			testInScope(() => {
				store.addIdea("idea");
				expect(() => store.reassignIdea("nonexistent", "/x", "x")).not.toThrow();
			});
		});
	});

	describe("getFilteredIdeas()", () => {
		it("returns all notes when activeRepo is null", () => {
			testInScope(() => {
				store.addIdea("global");
				store.addIdea("tagged", "/repo/a", "a");
				store.addIdea("tagged2", "/repo/b", "b");
				expect(store.getFilteredIdeas(null)).toHaveLength(3);
			});
		});

		it("returns matching + global notes when activeRepo is set", () => {
			testInScope(() => {
				store.addIdea("global");
				store.addIdea("repo-a", "/repo/a", "a");
				store.addIdea("repo-b", "/repo/b", "b");
				const filtered = store.getFilteredIdeas("/repo/a");
				expect(filtered).toHaveLength(2);
				expect(filtered.map((n) => n.text).sort()).toEqual(["global", "repo-a"]);
			});
		});

		it("includes notes with null repoPath (global) in any filter", () => {
			testInScope(() => {
				store.addIdea("always visible");
				const filtered = store.getFilteredIdeas("/any/repo");
				expect(filtered).toHaveLength(1);
				expect(filtered[0].text).toBe("always visible");
			});
		});
	});

	describe("filteredCount()", () => {
		it("returns total count when activeRepo is null", () => {
			testInScope(() => {
				store.addIdea("a");
				store.addIdea("b", "/repo", "repo");
				expect(store.filteredCount(null)).toBe(2);
			});
		});

		it("returns filtered count when activeRepo is set", () => {
			testInScope(() => {
				store.addIdea("global");
				store.addIdea("match", "/repo/a", "a");
				store.addIdea("other", "/repo/b", "b");
				expect(store.filteredCount("/repo/a")).toBe(2); // match + global
			});
		});
	});

	describe("hydrate() migration", () => {
		it("fills missing repoPath, repoDisplayName, and usedAt with null", async () => {
			const legacyNotes = [{ id: "note-1", text: "legacy", createdAt: 1000 }];
			mockInvoke.mockResolvedValueOnce({ notes: legacyNotes });

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.state.ideas[0].repoPath).toBeNull();
				expect(store.state.ideas[0].repoDisplayName).toBeNull();
				expect(store.state.ideas[0].usedAt).toBeNull();
			});
		});
	});

	describe("addIdea() with images", () => {
		it("stores images array when provided", () => {
			testInScope(() => {
				store.addIdea("idea with image", null, null, ["/path/img.png"]);
				expect(store.state.ideas[0].images).toEqual(["/path/img.png"]);
			});
		});

		it("defaults images to empty array when not provided", () => {
			testInScope(() => {
				store.addIdea("plain idea");
				expect(store.state.ideas[0].images).toEqual([]);
			});
		});

		it("allows image-only notes (no text)", () => {
			testInScope(() => {
				store.addIdea("", null, null, ["/path/img.png"]);
				expect(store.state.ideas.length).toBe(1);
				expect(store.state.ideas[0].text).toBe("");
				expect(store.state.ideas[0].images).toEqual(["/path/img.png"]);
			});
		});

		it("rejects notes with no text AND no images", () => {
			testInScope(() => {
				store.addIdea("", null, null, []);
				expect(store.state.ideas.length).toBe(0);
			});
		});

		it("accepts optional noteId parameter", () => {
			testInScope(() => {
				store.addIdea("with id", null, null, [], "custom-id-123");
				expect(store.state.ideas[0].id).toBe("custom-id-123");
			});
		});
	});

	describe("updateIdea()", () => {
		it("updates text in-place preserving id and createdAt", () => {
			testInScope(() => {
				store.addIdea("original");
				const note = store.state.ideas[0];
				const { id, createdAt } = note;
				store.updateIdea(id, "updated text", []);
				expect(store.state.ideas[0].id).toBe(id);
				expect(store.state.ideas[0].createdAt).toBe(createdAt);
				expect(store.state.ideas[0].text).toBe("updated text");
			});
		});

		it("updates images in-place", () => {
			testInScope(() => {
				store.addIdea("idea", null, null, ["/old.png"]);
				const id = store.state.ideas[0].id;
				store.updateIdea(id, "idea", ["/old.png", "/new.png"]);
				expect(store.state.ideas[0].images).toEqual(["/old.png", "/new.png"]);
			});
		});

		it("preserves repoPath and repoDisplayName", () => {
			testInScope(() => {
				store.addIdea("idea", "/repo", "my-repo");
				const id = store.state.ideas[0].id;
				store.updateIdea(id, "updated", []);
				expect(store.state.ideas[0].repoPath).toBe("/repo");
				expect(store.state.ideas[0].repoDisplayName).toBe("my-repo");
			});
		});

		it("persists via save_notes", async () => {
			await testInScopeAsync(async () => {
				store.addIdea("idea");
				mockInvoke.mockClear();
				store.updateIdea(store.state.ideas[0].id, "updated", []);
				await vi.waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("save_notes", expect.anything()));
			});
		});

		it("ignores unknown id", () => {
			testInScope(() => {
				store.addIdea("idea");
				expect(() => store.updateIdea("nonexistent", "x", [])).not.toThrow();
			});
		});
	});

	describe("removeIdea() with image cleanup", () => {
		it("calls delete_note_assets on removal", () => {
			testInScope(() => {
				store.addIdea("to remove", null, null, ["/img.png"]);
				const id = store.state.ideas[0].id;
				mockInvoke.mockClear();
				store.removeIdea(id);
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
				expect(store.state.ideas[0].images).toEqual([]);
			});
		});

		it("preserves existing images field", async () => {
			const notes = [
				{
					id: "note-1",
					text: "with image",
					createdAt: 1000,
					repoPath: null,
					repoDisplayName: null,
					usedAt: null,
					images: ["/path/img.png"],
				},
			];
			mockInvoke.mockResolvedValueOnce({ notes });

			await testInScopeAsync(async () => {
				await store.hydrate();
				expect(store.state.ideas[0].images).toEqual(["/path/img.png"]);
			});
		});
	});

	describe("pendingCount()", () => {
		it("returns total count when all notes are pending (no repo filter)", () => {
			testInScope(() => {
				store.addIdea("a");
				store.addIdea("b");
				expect(store.pendingCount(null)).toBe(2);
			});
		});

		it("excludes used notes", () => {
			testInScope(() => {
				store.addIdea("pending");
				store.addIdea("used");
				store.markUsed(store.state.ideas[0].id);
				expect(store.pendingCount(null)).toBe(1);
			});
		});

		it("returns 0 when all notes are used", () => {
			testInScope(() => {
				store.addIdea("a");
				store.addIdea("b");
				store.markUsed(store.state.ideas[0].id);
				store.markUsed(store.state.ideas[1].id);
				expect(store.pendingCount(null)).toBe(0);
			});
		});

		it("filters by repo when activeRepo is set", () => {
			testInScope(() => {
				store.addIdea("global pending");
				store.addIdea("repo-a pending", "/repo/a", "a");
				store.addIdea("repo-b pending", "/repo/b", "b");
				// global + repo-a match, repo-b excluded
				expect(store.pendingCount("/repo/a")).toBe(2);
			});
		});

		it("excludes used notes from repo filter", () => {
			testInScope(() => {
				store.addIdea("global pending");
				store.addIdea("repo-a used", "/repo/a", "a");
				store.markUsed(store.state.ideas[0].id); // most recent = repo-a used
				expect(store.pendingCount("/repo/a")).toBe(1);
			});
		});
	});

	describe("clearCompleted()", () => {
		it("removes all used notes", () => {
			testInScope(() => {
				store.addIdea("pending");
				store.addIdea("used");
				store.markUsed(store.state.ideas[0].id); // most recent = "used"
				mockInvoke.mockClear();

				store.clearCompleted();

				expect(store.state.ideas).toHaveLength(1);
				expect(store.state.ideas[0].text).toBe("pending");
			});
		});

		it("does nothing when no notes are used", () => {
			testInScope(() => {
				store.addIdea("a");
				store.addIdea("b");
				mockInvoke.mockClear();

				store.clearCompleted();

				expect(store.state.ideas).toHaveLength(2);
				expect(mockInvoke).not.toHaveBeenCalled();
			});
		});

		it("persists via save_notes after clearing", async () => {
			await testInScopeAsync(async () => {
				store.addIdea("used");
				store.markUsed(store.state.ideas[0].id);
				mockInvoke.mockClear();

				store.clearCompleted();

				await vi.waitFor(() =>
					expect(mockInvoke).toHaveBeenCalledWith("save_notes", { base: expect.anything(), config: { notes: [] } }),
				);
			});
		});

		it("calls delete_note_assets_batch for all cleared notes", () => {
			testInScope(() => {
				store.addIdea("used-1");
				store.addIdea("used-2");
				const id1 = store.state.ideas[0].id;
				const id2 = store.state.ideas[1].id;
				store.markUsed(id1);
				store.markUsed(id2);
				mockInvoke.mockClear();

				store.clearCompleted();

				expect(mockInvoke).toHaveBeenCalledWith("delete_note_assets_batch", {
					noteIds: expect.arrayContaining([id1, id2]),
				});
			});
		});

		it("clears all used notes while preserving pending ones", () => {
			testInScope(() => {
				store.addIdea("keep-1");
				store.addIdea("remove");
				store.addIdea("keep-2");
				store.markUsed(store.state.ideas[1].id); // "remove" is at index 1

				store.clearCompleted();

				expect(store.state.ideas).toHaveLength(2);
				expect(store.state.ideas.map((n) => n.text).sort()).toEqual(["keep-1", "keep-2"]);
			});
		});
	});

	describe("markUsed()", () => {
		it("sets usedAt timestamp on the note", () => {
			testInScope(() => {
				store.addIdea("idea");
				const id = store.state.ideas[0].id;
				expect(store.state.ideas[0].usedAt).toBeNull();
				const before = Date.now();
				store.markUsed(id);
				const after = Date.now();
				expect(store.state.ideas[0].usedAt).toBeGreaterThanOrEqual(before);
				expect(store.state.ideas[0].usedAt).toBeLessThanOrEqual(after);
			});
		});

		it("persists after marking used", async () => {
			await testInScopeAsync(async () => {
				store.addIdea("idea");
				mockInvoke.mockClear();
				store.markUsed(store.state.ideas[0].id);
				await vi.waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("save_notes", expect.anything()));
			});
		});

		it("ignores unknown id", () => {
			testInScope(() => {
				store.addIdea("idea");
				expect(() => store.markUsed("nonexistent")).not.toThrow();
			});
		});
	});
});
