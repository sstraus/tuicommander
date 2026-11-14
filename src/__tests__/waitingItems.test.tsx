import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { listen } from "@tauri-apps/api/event";
import { createRoot } from "solid-js";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ActivityDashboard } from "../components/ActivityDashboard/ActivityDashboard";
import { WaitingOnYou } from "../components/StoriesDialog/WaitingOnYou";
import { useWaitingItems } from "../hooks/useWaitingItems";
import { activityDashboardStore } from "../stores/activityDashboard";
import { activityStore } from "../stores/activityStore";
import { storiesUi } from "../stores/storiesUi";
import { waitingItemsStore } from "../stores/waitingItems";
import { mockInvoke } from "./mocks/tauri";

const manualCheck = {
	id: "story:s1",
	kind: "manual_check",
	title: "Review the layout",
	detail: "Manual checks are waiting in Plans and Stories.",
	options: [],
	project: "/reviewed-project",
	planId: "p1",
	story: { id: "s1", title: "Review the layout", status: "qa" },
};

let snapshot: { items: unknown[]; health: unknown[]; warnings: string[] };

beforeEach(async () => {
	snapshot = { items: [manualCheck], health: [], warnings: [] };
	// A fresh object per read, as over IPC: identical references would not republish.
	mockInvoke.mockImplementation(async (command: string) =>
		command === "boss_attention" ? structuredClone(snapshot) : undefined,
	);
	// The bell sync starts after persisted notifications load, as in the app.
	await activityStore.hydrate();
});

afterEach(() => {
	activityDashboardStore.close();
	storiesUi.close();
	activityStore.clearAll();
	waitingItemsStore._resetForTest();
	vi.clearAllMocks();
});

const reads = () => mockInvoke.mock.calls.filter(([command]) => command === "boss_attention").length;

function emit(event: string): void {
	for (const [name, handler] of vi.mocked(listen).mock.calls) {
		if (name === event) handler({ event: name, id: 1, payload: {} });
	}
}

/** Deliver the backend invalidation and wait until the reread snapshot is published and synced. */
async function emitChanged(): Promise<void> {
	const before = reads();
	emit("boss-attention-changed");
	await waitFor(() => expect(reads()).toBeGreaterThan(before));
	await waitFor(() => expect(waitingItemsStore.loading()).toBe(false));
}

const waitingBell = () => activityStore.state.items.filter((item) => item.sectionId === "waiting");

function startBellSync(): () => void {
	return createRoot((dispose) => {
		useWaitingItems();
		return dispose;
	});
}

// Catches: the section is still mounted in the dashboard.
it("attention_section_absent_from_activity_dashboard", async () => {
	snapshot.health = [
		{ sessionId: "w", connectionId: null, name: "worker", kind: "Failed", reason: "The agent reported a failure." },
	];
	render(() => <ActivityDashboard embedded terminals={() => []} />);
	expect(await screen.findByText("Failed · worker")).toBeInTheDocument();
	expect(screen.queryByText("Review the layout")).not.toBeInTheDocument();
	expect(screen.queryByRole("region", { name: /needs|waiting/i })).not.toBeInTheDocument();
	expect(screen.queryByRole("button", { name: "Open Plans and Stories" })).not.toBeInTheDocument();
});

// Catches: a refresh or reseed re-notifies the same item.
it("new_item_notifies_bell_once", async () => {
	const dispose = startBellSync();
	try {
		await waitFor(() => expect(waitingBell()).toHaveLength(1));
		const first = waitingBell()[0];
		expect(first.title).toBe("Review the layout");
		expect(first.dismissible).toBe(true);
		const createdAt = first.createdAt;
		activityStore.dismissItem(first.id);
		// A changed snapshot re-runs the sync; the item already notified must not notify again.
		snapshot = {
			items: [manualCheck, { ...manualCheck, id: "story:s2", title: "Check the colors" }],
			health: [],
			warnings: [],
		};
		await emitChanged();
		expect(waitingBell()).toHaveLength(2);
		const again = waitingBell().find((item) => item.title === "Review the layout");
		expect(again?.createdAt).toBe(createdAt);
		expect(again?.dismissed).toBe(true);
	} finally {
		dispose();
	}
});

// Catches: a bell entry stays after the story leaves qa.
it("resolved_item_clears_bell", async () => {
	const dispose = startBellSync();
	try {
		await waitFor(() => expect(waitingBell()).toHaveLength(1));
		snapshot = { items: [], health: [], warnings: [] };
		await emitChanged();
		expect(waitingBell()).toHaveLength(0);
	} finally {
		dispose();
	}
});

// Catches: an unreadable project drops its items from the snapshot, the bell
// clears them, and the next readable snapshot notifies them again.
it("partial_snapshot_does_not_clear_or_renotify_the_bell", async () => {
	const dispose = startBellSync();
	try {
		await waitFor(() => expect(waitingBell()).toHaveLength(1));
		const createdAt = waitingBell()[0].createdAt;
		snapshot = { items: [], health: [], warnings: ["/reviewed-project: locked"] };
		await emitChanged();
		expect(waitingBell()).toHaveLength(1);
		snapshot = { items: [manualCheck], health: [], warnings: [] };
		await emitChanged();
		expect(waitingBell()[0].createdAt).toBe(createdAt);
	} finally {
		dispose();
	}
});

// Catches: the bell entry does not lead to the item's story in Plans and Stories.
it("bell_click_reveals_the_story_in_plans_and_stories", async () => {
	const dispose = startBellSync();
	try {
		await waitFor(() => expect(waitingBell()).toHaveLength(1));
		waitingBell()[0].onClick?.();
		expect(storiesUi.visible()).toBe(true);
		expect(storiesUi.project()).toBe("/reviewed-project");
		expect(storiesUi.takeTarget("/reviewed-project")).toMatchObject({ planId: "p1", storyId: "s1" });
	} finally {
		dispose();
	}
});

// Catches: a failed refresh leaves old health visible as if it were current.
it("refresh_failure_replaces_cached_health_with_an_explicit_error", async () => {
	let failed = false;
	mockInvoke.mockImplementation(async (command: string) => {
		if (command !== "boss_attention") return;
		if (failed) throw new Error("offline");
		return {
			items: [],
			health: [
				{
					sessionId: "failed-worker",
					connectionId: null,
					name: "worker",
					kind: "Failed",
					reason: "The agent reported a failure.",
				},
			],
			warnings: [],
		};
	});
	const open = vi.fn();
	render(() => <ActivityDashboard embedded terminals={() => []} onOpenSession={open} />);
	expect(await screen.findByText("Failed · worker")).toBeInTheDocument();
	await fireEvent.click(screen.getByRole("button", { name: "Inspect terminal" }));
	expect(open).toHaveBeenCalledWith("failed-worker", null);
	failed = true;
	await fireEvent.click(screen.getByRole("button", { name: "Refresh health" }));
	expect(await screen.findByRole("alert")).toHaveTextContent("Agent health is unavailable");
	expect(screen.queryByText("Failed · worker")).not.toBeInTheDocument();
});

// Catches: every session-state-changed remounts all rows and rereads every story store.
it("unchanged_snapshot_keeps_row_identity", async () => {
	render(() => <WaitingOnYou project="/reviewed-project" onSelect={() => {}} />);
	const row = await screen.findByRole("button", { name: /Review the layout/ });
	const item = waitingItemsStore.snapshot()?.items[0];
	await emitChanged();
	expect(screen.getByRole("button", { name: /Review the layout/ })).toBe(row);
	expect(waitingItemsStore.snapshot()?.items[0]).toBe(item);

	const before = reads();
	for (let i = 0; i < 5; i++) emit("session-state-changed");
	await new Promise((resolve) => setTimeout(resolve, 400));
	expect(reads()).toBe(before);

	for (let i = 0; i < 5; i++) emit("boss-attention-changed");
	await waitFor(() => expect(reads()).toBe(before + 1));
	await new Promise((resolve) => setTimeout(resolve, 400));
	expect(reads()).toBe(before + 1);
	expect(screen.getByRole("button", { name: /Review the layout/ })).toBe(row);
});
