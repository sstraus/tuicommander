import { beforeEach, describe, expect, it, vi } from "vitest";

const mockInvoke = vi.fn();
const registered: { apply: (remote: unknown) => void }[] = [];

vi.mock("../../services/settingsRefresh", () => ({
	settingsRefresh: { register: (reader: { apply: (remote: unknown) => void }) => (registered.push(reader), () => {}) },
	reportSettingsConflicts: vi.fn(),
}));

const tasks = (key: string, value: object) => ({ [key]: JSON.stringify(value) });

describe("uiStore Tasks preferences revision", () => {
	let store: typeof import("../../stores/ui").uiStore;

	beforeEach(async () => {
		vi.resetModules();
		registered.length = 0;
		mockInvoke.mockReset().mockResolvedValue(undefined);
		vi.doMock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));
		store = (await import("../../stores/ui")).uiStore;
		mockInvoke.mockResolvedValue({ tasks_view_preferences: tasks("a", { followed: true }) });
		await store.hydrate();
		store.startRemoteSync();
	});

	// Catches: an unrelated preferences save in another window reloads Tasks (reload storm).
	it("keeps the revision when a remote apply carries identical Tasks preferences", () => {
		const before = store.state.tasksPreferencesRevision;
		registered[0].apply({ sidebar_width: 321, tasks_view_preferences: tasks("a", { followed: true }) });
		expect(store.state.tasksPreferencesRevision).toBe(before);
	});

	// Catches: a real Tasks preference change from another window never reloads the list.
	it("bumps the revision once when a remote apply changes a Tasks key", () => {
		const before = store.state.tasksPreferencesRevision;
		registered[0].apply({ tasks_view_preferences: tasks("a", { followed: false }) });
		expect(store.state.tasksPreferencesRevision).toBe(before + 1);
	});
});
