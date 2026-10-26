import { Show } from "solid-js";
import type { ActivityItem as ActivityItemData } from "../../plugins/types";
import styles from "./ActivityItem.module.css";

interface ActivityItemProps {
	item: ActivityItemData;
	onTap: (item: ActivityItemData) => void;
}

function formatTime(ts: number): string {
	const d = new Date(ts);
	return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false });
}

function readableSubtitle(subtitle: string): string {
	return subtitle
		.replace(/ran for (\d+)s\b/g, (_, seconds: string) => {
			const totalSeconds = Number(seconds);
			return totalSeconds >= 60 ? `ran for ${Math.floor(totalSeconds / 60)} min` : `ran for ${seconds}s`;
		})
		.replace(/\b1 blocks\b/g, "1 block");
}

export function ActivityItem(props: ActivityItemProps) {
	return (
		<button type="button" data-control-layout="row" class={styles.item} onClick={() => props.onTap(props.item)}>
			<div
				class={styles.icon}
				style={{ color: props.item.iconColor ?? "var(--fg-muted)" }}
				innerHTML={props.item.icon}
			/>
			<div class={styles.body}>
				<span class={styles.title}>{props.item.title}</span>
				<Show when={props.item.subtitle}>
					<span class={styles.subtitle}>{readableSubtitle(props.item.subtitle!)}</span>
				</Show>
			</div>
			<span class={styles.time}>{formatTime(props.item.createdAt)}</span>
		</button>
	);
}
