import { createMemo, For, Show } from "solid-js";
import { activityStore } from "../../stores/activityStore";
import type { ActivityItem as ActivityItemData } from "../../plugins/types";
import { ActivityItem } from "../components/ActivityItem";
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

  const groups: TimeGroup[] = [];
  const recent: ActivityItemData[] = [];
  const earlier: ActivityItemData[] = [];
  const today: ActivityItemData[] = [];
  const older: ActivityItemData[] = [];

  for (const item of items) {
    if (item.createdAt >= fiveMinAgo) {
      recent.push(item);
    } else if (item.createdAt >= oneHourAgo) {
      earlier.push(item);
    } else if (item.createdAt >= dayStart) {
      today.push(item);
    } else {
      older.push(item);
    }
  }

  if (recent.length > 0) groups.push({ label: "NOW", items: recent });
  if (earlier.length > 0) groups.push({ label: "EARLIER", items: earlier });
  if (today.length > 0) groups.push({ label: "TODAY", items: today });
  if (older.length > 0) groups.push({ label: "OLDER", items: older });

  return groups;
}

export function ActivityScreen(props: ActivityScreenProps) {
  const activeItems = createMemo(() => {
    const items = activityStore.getActive();
    return [...items].sort((a, b) => b.createdAt - a.createdAt);
  });

  const groups = createMemo(() => groupByTime(activeItems()));

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
              <For each={group.items}>
                {(item) => (
                  <ActivityItem item={item} onTap={handleTap} />
                )}
              </For>
            </>
          )}
        </For>
      </Show>
    </div>
  );
}
