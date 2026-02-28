import { Show } from "solid-js";
import type { ActivityItem as ActivityItemData } from "../../plugins/types";
import styles from "./ActivityItem.module.css";

interface ActivityItemProps {
  item: ActivityItemData;
  onTap: (item: ActivityItemData) => void;
}

function formatTime(ts: number): string {
  const d = new Date(ts);
  return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

export function ActivityItem(props: ActivityItemProps) {
  return (
    <button class={styles.item} onClick={() => props.onTap(props.item)}>
      <div
        class={styles.icon}
        style={{ color: props.item.iconColor ?? "var(--fg-muted)" }}
        innerHTML={props.item.icon}
      />
      <div class={styles.body}>
        <span class={styles.title}>{props.item.title}</span>
        <Show when={props.item.subtitle}>
          <span class={styles.subtitle}>{props.item.subtitle}</span>
        </Show>
      </div>
      <span class={styles.time}>{formatTime(props.item.createdAt)}</span>
    </button>
  );
}
