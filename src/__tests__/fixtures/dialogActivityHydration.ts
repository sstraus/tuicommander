import { afterAll, beforeAll, vi } from "vitest";
import { invoke } from "../../invoke";
import { activityStore } from "../../stores/activityStore";

// Isolated dialogs do not mount the application shell that hydrates this store.
// Settle its real initialization instead of hiding the suite's promise-leak check.
beforeAll(async () => {
	vi.mocked(invoke).mockResolvedValueOnce(null);
	await activityStore.hydrate();
});

// The store's app-lifetime debounced writer is not owned by a standalone dialog.
afterAll(() => activityStore._testCancelPendingSave());
