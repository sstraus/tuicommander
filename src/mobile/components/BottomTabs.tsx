import styles from "./BottomTabs.module.css";

export type TabId = "sessions" | "activity" | "settings";

interface BottomTabsProps {
  active: TabId;
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
    id: "activity",
    label: "Activity",
    // Bell icon
    icon: `<svg width="20" height="20" viewBox="0 0 20 20" fill="none"><path d="M10 2.5a5 5 0 00-5 5v3l-1.5 2h13L15 10.5v-3a5 5 0 00-5-5z" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/><path d="M8 14.5a2 2 0 004 0" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>`,
  },
  {
    id: "settings",
    label: "Settings",
    // Gear icon
    icon: `<svg width="20" height="20" viewBox="0 0 20 20" fill="none"><circle cx="10" cy="10" r="2.5" stroke="currentColor" stroke-width="1.5"/><path d="M10 1.5l1.2 2.1a6.5 6.5 0 012.2 1.3l2.1-1.2 1.5 2.6-2.1 1.2a6.5 6.5 0 010 2.5l2.1 1.2-1.5 2.6-2.1-1.2a6.5 6.5 0 01-2.2 1.3L10 18.5l-1.2-2.1a6.5 6.5 0 01-2.2-1.3l-2.1 1.2-1.5-2.6 2.1-1.2a6.5 6.5 0 010-2.5L3 8.8l1.5-2.6 2.1 1.2a6.5 6.5 0 012.2-1.3L10 1.5z" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/></svg>`,
  },
];

export function BottomTabs(props: BottomTabsProps) {
  return (
    <nav class={styles.tabs}>
      {tabs.map((tab) => (
        <button
          class={styles.tab}
          classList={{ [styles.active]: props.active === tab.id }}
          onClick={() => props.onSelect(tab.id)}
          aria-label={tab.label}
          aria-current={props.active === tab.id ? "page" : undefined}
        >
          <span class={styles.icon} innerHTML={tab.icon} />
          <span class={styles.label}>{tab.label}</span>
        </button>
      ))}
    </nav>
  );
}
