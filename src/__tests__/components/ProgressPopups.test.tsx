import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ProgressDialog } from "../../components/ProgressDialog";
import { ToastList } from "../../components/ToastContainer/ToastList";
import { createProgressStore, type ProgressEntry, progressStore } from "../../stores/progress";
import { toastsStore } from "../../stores/toasts";

const { journal } = vi.hoisted(() => ({ journal: [] as ProgressEntry[] }));
vi.mock("../../invoke", () => ({
	invoke: vi.fn(async (command: string) =>
		command === "progress_list" ? { project: "/repo", entries: [...journal], ptyIds: [], nextCursor: null } : {},
	),
}));
vi.mock("../../stores/modalStack", () => ({ registerModal: vi.fn() }));
vi.mock("../../services/nativeNotifications", () => ({ showNativeNotice: vi.fn() }));
vi.mock("../../stores/repositories", () => ({
	repositoriesStore: { state: { activeRepoPath: "/repo" }, get: () => ({ displayName: "Example" }) },
}));
vi.mock("../../stores/terminals", () => ({
	terminalsStore: { state: { activeId: null, terminals: {} } },
}));

function record(id: number, type: ProgressEntry["type"] = "done") {
	const entry: ProgressEntry = { id, project: "/repo", createdAtMs: Date.now(), type, text: `Report ${id}` };
	journal.unshift(entry);
	progressStore.presentLive({ repo_path: "/repo", payload: { entry } });
}

function popups() {
	return render(() => <ToastList onDismiss={(toast) => toastsStore.remove(toast.id)} repoName={() => "Example"} />);
}

beforeEach(() => {
	progressStore.resetForTests();
	journal.length = 0;
	while (toastsStore.toasts.length) toastsStore.remove(toastsStore.toasts[0].id);
});
afterEach(() => {
	cleanup();
	while (toastsStore.toasts.length) toastsStore.remove(toastsStore.toasts[0].id);
});

describe("session-only progress popup mute", () => {
	it("keeps muted reports in Progress and restores the next popup through Unmute", async () => {
		// Catches: suppressing the journal refresh/count with the popup, or an Unmute that does not restore delivery.
		popups();
		record(1);
		fireEvent.click(screen.getByRole("button", { name: "Mute" }));
		expect(toastsStore.toasts).toHaveLength(0);
		record(2);
		expect(toastsStore.toasts).toHaveLength(0);
		expect(progressStore.unreadCount).toBe(2);
		progressStore.open("/repo", null);
		render(() => <ProgressDialog />);
		await screen.findByText("Report 2");
		expect(screen.getByText("Report 1")).toBeTruthy();
		expect(screen.getByText("Progress popups muted for this session. Blocked reports still appear.")).toBeTruthy();
		record(3);
		await screen.findByText("Report 3");
		expect(toastsStore.toasts).toHaveLength(0);
		fireEvent.click(screen.getByRole("button", { name: "Unmute" }));
		expect(screen.queryByText(/Progress popups muted/)).toBeNull();
		record(4);
		expect(toastsStore.toasts.map((toast) => toast.message)).toEqual(["Report 4"]);
	});

	it("removes visible and queued completions but retains blocked and unrelated popups", () => {
		// Catches: mute leaving queued progress interruptions behind or deleting requests that need the user.
		popups();
		record(1, "blocked");
		record(2);
		record(3); // Same title groups with 2.
		journal[0].step = "Another step";
		progressStore.presentLive({ repo_path: "/repo", payload: { entry: journal[0] } });
		fireEvent.click(screen.getAllByRole("button", { name: "Mute" })[0]);
		expect(toastsStore.toasts.map((toast) => toast.message)).toEqual(["Report 1"]);
		toastsStore.add("Unrelated", "Keep this error", "error", false, undefined, undefined, undefined, undefined, false);
		record(4);
		expect(toastsStore.toasts.map((toast) => toast.message)).toEqual(["Report 1", "Keep this error"]);
		record(5, "blocked");
		expect(toastsStore.toasts.some((toast) => toast.message === "Report 5")).toBe(true);
		while (toastsStore.toasts.length) toastsStore.remove(toastsStore.toasts[0].id);
		expect(toastsStore.toasts).toHaveLength(0);
	});

	it("starts a fresh client store unmuted without writing settings", async () => {
		// Catches: restoring mute from persisted client settings on restart.
		const save = vi.spyOn(Storage.prototype, "setItem");
		popups();
		record(1);
		fireEvent.click(screen.getByRole("button", { name: "Mute" }));
		expect(save).not.toHaveBeenCalled();
		save.mockRestore();
		const fresh = createProgressStore();
		fresh.presentLive({ repo_path: "/repo", payload: { entry: { ...journal[0], id: 2, text: "Fresh startup" } } });
		await waitFor(() => expect(toastsStore.toasts.map((toast) => toast.message)).toEqual(["Fresh startup"]));
	});
});
