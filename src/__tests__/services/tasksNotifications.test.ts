import { beforeEach, expect, it, vi } from "vitest";
import { detail } from "../../components/Tasks/detail/__tests__/detailFixture";
import type { TaskViewPreference } from "../../types/taskPreferences";

const { preferences, invoke, notifyTasksItem, save } = vi.hoisted(() => {
	const preferences: Record<string, TaskViewPreference> = {};
	return {
		preferences,
		invoke: vi.fn(),
		notifyTasksItem: vi.fn(),
		save: vi.fn((key: string, patch: TaskViewPreference) => {
			preferences[key] = { ...preferences[key], ...patch };
		}),
	};
});
vi.mock("../../invoke", () => ({ invoke }));
vi.mock("../../stores/ui", () => ({
	uiStore: { state: { tasksViewPreferences: preferences }, setTaskViewPreference: save },
}));
vi.mock("../../services/prNativeNotifications", () => ({ notifyTasksItem }));

import { tasksNotifications } from "../../services/tasksNotifications";

beforeEach(() => {
	for (const key of Object.keys(preferences)) delete preferences[key];
	vi.clearAllMocks();
});

// Catches: default settings or unfollowed items emit Tasks notices during refresh.
it("default_policy_is_off_and_unfollowed_refresh_is_silent", async () => {
	expect(tasksNotifications.enabled()).toBe(false);
	await tasksNotifications.observe(detail());
	expect(invoke).not.toHaveBeenCalled();
	expect(notifyTasksItem).not.toHaveBeenCalled();
	tasksNotifications.setFollowed(detail().item.reference, true);
	invoke.mockResolvedValue({ snapshot: { latest: 1, ids: ["comment"], count: null }, notify: false });
	await tasksNotifications.observe(detail());
	expect(invoke.mock.calls[0][1].input.enabled).toBe(false);
	expect(notifyTasksItem).not.toHaveBeenCalled();
});

// Catches: the provider comparison result never reaches native delivery or the next comparison baseline.
// Persistence is covered by tasksNotificationsPersistence.test.ts through the real UI preferences round-trip.
it("routes_new_comment_and_reuses_the_saved_comparison_baseline", async () => {
	const value = detail();
	tasksNotifications.setEnabled(true);
	tasksNotifications.setFollowed(value.item.reference, true);
	expect(save).toHaveBeenCalledWith(expect.stringContaining("tasks:follow:"), {
		followed: true,
		commentSnapshot: undefined,
	});
	invoke.mockResolvedValue({ snapshot: { latest: 2, ids: ["new"], count: null }, notify: true });
	await tasksNotifications.observe(value);
	expect(notifyTasksItem).toHaveBeenCalledWith(
		expect.objectContaining({ body: "DEMO-1: Inspect the work item", url: value.item.url }),
	);
	await tasksNotifications.observe(value);
	expect(invoke.mock.calls[1][1].input.previous).toEqual({ latest: 2, ids: ["new"], count: null });
});

// Catches: delayed provider replies notify after unfollow or after the policy is disabled.
it("inflight_refresh_cannot_notify_after_unfollow_or_disable", async () => {
	const value = detail();
	tasksNotifications.setEnabled(true);
	tasksNotifications.setFollowed(value.item.reference, true);
	let resolve!: (value: unknown) => void;
	invoke.mockImplementation(
		() =>
			new Promise((done) => {
				resolve = done;
			}),
	);
	const pending = tasksNotifications.observe(value);
	tasksNotifications.setFollowed(value.item.reference, false);
	resolve({ snapshot: { latest: 2, ids: [], count: null }, notify: true });
	await pending;
	expect(notifyTasksItem).not.toHaveBeenCalled();
	tasksNotifications.setFollowed(value.item.reference, true);
	const disabled = tasksNotifications.observe(value);
	tasksNotifications.setEnabled(false);
	resolve({ snapshot: { latest: 3, ids: [], count: null }, notify: true });
	await disabled;
	expect(notifyTasksItem).not.toHaveBeenCalled();
});

// Catches: adding our own comment produces a notification for our own completed action.
it("own_comment_advances_baseline_without_requesting_a_notice", async () => {
	tasksNotifications.setEnabled(true);
	tasksNotifications.setFollowed(detail().item.reference, true);
	invoke.mockResolvedValue({ snapshot: { latest: 4, ids: [], count: null }, notify: false });
	await tasksNotifications.observe(detail(), true);
	expect(invoke.mock.calls[0][1].input.enabled).toBe(false);
	expect(notifyTasksItem).not.toHaveBeenCalled();
	await tasksNotifications.observe(detail());
	expect(invoke.mock.calls[1][1].input.previous).toEqual({ latest: 4, ids: [], count: null });
});

// Catches: every 60 s detail refresh rewrites an unchanged snapshot -> prefs write -> list reload.
it("unchanged_comment_snapshot_is_not_written_again", async () => {
	const value = detail();
	tasksNotifications.setFollowed(value.item.reference, true);
	invoke.mockResolvedValue({ snapshot: { latest: 2, ids: ["a"], count: null }, notify: false });
	await tasksNotifications.observe(value);
	await tasksNotifications.observe(value);
	const snapshotWrites = save.mock.calls.filter(([, patch]) => patch.commentSnapshot !== undefined);
	expect(snapshotWrites).toHaveLength(1);
});
