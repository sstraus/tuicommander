import { invoke } from "../invoke";
import { appLogger } from "../stores/appLogger";
import { uiStore } from "../stores/ui";
import type { TaskViewPreference } from "../types/taskPreferences";
import type { WorkItemDetail, WorkItemRef } from "../types/work-items.gen";
import { notifyTasksItem } from "./prNativeNotifications";

const POLICY_KEY = "tasks:notification-policy";
const itemKey = (ref: WorkItemRef) => `tasks:follow:${JSON.stringify([ref.providerId, ref.accountId, ref.id])}`;
const revisions = new Map<string, number>();

/** These entries use the existing Tasks UI preference persistence, not a new store. */
export const tasksNotifications = {
	enabled: () => uiStore.state.tasksViewPreferences[POLICY_KEY]?.notificationsEnabled === true,
	setEnabled: (enabled: boolean) => uiStore.setTaskViewPreference(POLICY_KEY, { notificationsEnabled: enabled }),
	followed: (ref: WorkItemRef) => uiStore.state.tasksViewPreferences[itemKey(ref)]?.followed === true,
	setFollowed(ref: WorkItemRef, followed: boolean) {
		const key = itemKey(ref);
		revisions.set(key, (revisions.get(key) ?? 0) + 1);
		uiStore.setTaskViewPreference(key, { followed, commentSnapshot: undefined });
	},
	async observe(detail: WorkItemDetail, silent = false): Promise<void> {
		const ref = detail.item.reference;
		if (!tasksNotifications.followed(ref)) return;
		const key = itemKey(ref);
		const revision = (revisions.get(key) ?? 0) + 1;
		revisions.set(key, revision);
		try {
			const result = await invoke<{ snapshot: NonNullable<TaskViewPreference["commentSnapshot"]>; notify: boolean }>(
				"compare_work_item_comments",
				{
					input: {
						detail,
						previous: uiStore.state.tasksViewPreferences[key]?.commentSnapshot ?? null,
						enabled: tasksNotifications.enabled() && !silent,
						followed: true,
					},
				},
			);
			if (revisions.get(key) !== revision || !tasksNotifications.followed(ref)) return;
			const previous = uiStore.state.tasksViewPreferences[key]?.commentSnapshot;
			// An unchanged snapshot must not write preferences: the write reloads the Tasks list.
			if (JSON.stringify(previous) !== JSON.stringify(result.snapshot)) {
				uiStore.setTaskViewPreference(key, { commentSnapshot: result.snapshot });
			}
			if (result.notify && tasksNotifications.enabled()) {
				notifyTasksItem({
					isCurrent: () => tasksNotifications.enabled() && tasksNotifications.followed(ref),
					title: "New comment on followed task",
					body: `${detail.item.key}: ${detail.item.title}`,
					url: detail.item.url,
					key: `task-comment:${key}:${JSON.stringify(result.snapshot)}`,
				});
			}
		} catch (error) {
			appLogger.debug("store", "Failed to compare Tasks comments", error);
		}
	},
};
