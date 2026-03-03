import { Show } from "solid-js";
import styles from "./TopBar.module.css";

interface TopBarProps {
  notificationCount?: number;
}

export function TopBar(props: TopBarProps) {
  return (
    <header class={styles.topBar}>
      <div class={styles.titleGroup}>
        <span class={styles.appName}>TUICommander</span>
        <span class={styles.subtitle}>Manage your sessions</span>
      </div>
      <Show when={(props.notificationCount ?? 0) > 0}>
        <span class={styles.badge}>{props.notificationCount}</span>
      </Show>
    </header>
  );
}
