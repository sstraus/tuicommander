import { createMemo, createSignal, For, onMount, Show } from "solid-js";
import type { ActivityItem as ActivityItemData } from "../../plugins/types";
import { activityStore } from "../../stores/activityStore";
import { ActivityItem } from "../components/ActivityItem";
import { createVisibilityInterval } from "../utils/visibilityInterval";
import styles from "./ActivityScreen.module.css";

interface ActivityScreenProps {
	onNavigateSession: (sessionId: string) => void;
}

interface TimeGroup {
	label: string;
	items: ActivityItemData[];
}

function groupByTime(items: ActivityItemData[]): TimeGroup[] {
	const now = Date.now();
	const fiveMinAgo = now - 5 * 60_000;
	const oneHourAgo = now - 60 * 60_000;
	const startOfDay = new Date();
	startOfDay.setHours(0, 0, 0, 0);
	const dayStart = startOfDay.getTime();

	const buckets = Object.groupBy(items, (item) => {
		if (item.createdAt >= fiveMinAgo) return "NOW";
		if (item.createdAt >= oneHourAgo) return "EARLIER";
		if (item.createdAt >= dayStart) return "TODAY";
		return "OLDER";
	});
	const groups: TimeGroup[] = [];
	for (const label of ["NOW", "EARLIER", "TODAY", "OLDER"] as const) {
		const bucket = buckets[label];
		if (bucket) groups.push({ label, items: bucket });
	}

	return groups;
}

export function ActivityScreen(props: ActivityScreenProps) {
	onMount(() => {
		void activityStore.hydrate();
	});
	const activeItems = createMemo(() => {
		const items = activityStore.getActive();
		return items.toSorted((a, b) => b.createdAt - a.createdAt);
	});

	// Snapshot items every 10s so time-group buckets don't reshuffle
	// on every store mutation (which is constant with multiple sessions).
	// New items/removals trigger an immediate snapshot via count change.
	const [snapshot, setSnapshot] = createSignal(activeItems(), { equals: false });
	createVisibilityInterval(() => setSnapshot(activeItems()), 10_000);

	let prevCount = activeItems().length;
	const groups = createMemo(() => {
		const current = activeItems();
		if (current.length !== prevCount) {
			prevCount = current.length;
			setSnapshot(current);
		}
		return groupByTime(snapshot());
	});

	function handleTap(item: ActivityItemData) {
		if (item.contentUri) {
			// contentUri may contain a session reference
			const sessionMatch = item.contentUri.match(/session\/([^/]+)/);
			if (sessionMatch) {
				props.onNavigateSession(sessionMatch[1]);
				return;
			}
		}
		if (item.onClick) {
			item.onClick();
		}
	}

	return (
		<div class={styles.screen}>
			<Show
				when={activeItems().length > 0}
				fallback={
					<div class={styles.empty}>
						<div class={styles.emptyIcon}>
							<svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
								<path d="M12 8v4l3 3" />
								<circle cx="12" cy="12" r="10" />
							</svg>
						</div>
						<span class={styles.emptyTitle}>No recent activity</span>
						<span class={styles.emptyHint}>Events from your agents will appear here</span>
					</div>
				}
			>
				<For each={groups()}>
					{(group) => (
						<>
							<div class={styles.groupHeader}>{group.label}</div>
							<For each={group.items}>{(item) => <ActivityItem item={item} onTap={handleTap} />}</For>
						</>
					)}
				</For>
			</Show>
		</div>
	);
}
