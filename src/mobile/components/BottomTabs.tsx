import { For } from "solid-js";
import { Button } from "../../components/ui/Button";
import styles from "./BottomTabs.module.css";

export type TabId = "chat" | "sessions" | "files" | "progress" | "activity" | "settings";

interface BottomTabsProps {
	active: TabId;
	aiChatAvailable: boolean;
	onSelect: (tab: TabId) => void;
}

const tabs: Array<{ id: TabId; label: string; icon: string }> = [
	{
		id: "sessions",
		label: "Sessions",
		// Terminal/list icon
		icon: `<svg width="20" height="20" viewBox="0 0 20 20" fill="none"><rect x="2" y="3" width="16" height="14" rx="2" stroke="currentColor" stroke-width="1.5"/><path d="M5 8l3 2-3 2" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/><line x1="10" y1="12" x2="15" y2="12" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>`,
	},
	{
		id: "chat",
		label: "Chat",
		icon: `<svg width="20" height="20" viewBox="0 0 20 20" fill="currentColor"><path d="M3 2h14a1 1 0 0 1 1 1v11a1 1 0 0 1-1 1H7.4l-4.8 3.6A1 1 0 0 1 1 17.8V3a1 1 0 0 1 1-1zm1 2v11.8L6.7 13H16V4H4z"/></svg>`,
	},
	{
		id: "files",
		label: "Files",
		icon: `<svg width="20" height="20" viewBox="0 0 20 20" fill="none"><path d="M2.5 5.5h5l1.5 2h8.5v8.5h-15z" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/></svg>`,
	},
	{
		id: "progress",
		label: "Progress",
		icon: `<svg width="20" height="20" viewBox="0 0 20 20" fill="none"><path d="M3 16h14M4 13l3-4 3 2 6-7" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>`,
	},
	{
		id: "activity",
		label: "Activity",
		// Bell icon
		icon: `<svg width="20" height="20" viewBox="0 0 20 20" fill="none"><path d="M10 2.5a5 5 0 00-5 5v3l-1.5 2h13L15 10.5v-3a5 5 0 00-5-5z" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/><path d="M8 14.5a2 2 0 004 0" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>`,
	},
];

export function BottomTabs(props: BottomTabsProps) {
	return (
		<nav class={styles.tabs}>
			<For each={tabs.filter((tab) => tab.id !== "chat" || props.aiChatAvailable)}>
				{(tab) => (
					<Button
						variant="ghost"
						geometry="stretch"
						class={styles.tab}
						classList={{ [styles.active]: props.active === tab.id }}
						onClick={() => props.onSelect(tab.id)}
						aria-label={tab.label}
						aria-current={props.active === tab.id ? "page" : undefined}
					>
						<span class={styles.icon} innerHTML={tab.icon} />
						<span class={styles.label}>{tab.label}</span>
					</Button>
				)}
			</For>
		</nav>
	);
}
