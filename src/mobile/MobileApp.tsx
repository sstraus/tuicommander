import { createSignal, Match, Switch } from "solid-js";
import { TopBar } from "./components/TopBar";
import { BottomTabs, type TabId } from "./components/BottomTabs";
import styles from "./MobileApp.module.css";

/** Placeholder screens — replaced by real implementations in later stories */
function SessionsPlaceholder() {
  return <div class={styles.placeholder}>Sessions screen</div>;
}
function ActivityPlaceholder() {
  return <div class={styles.placeholder}>Activity screen</div>;
}
function SettingsPlaceholder() {
  return <div class={styles.placeholder}>Settings screen</div>;
}

export default function MobileApp() {
  const [activeTab, setActiveTab] = createSignal<TabId>("sessions");

  return (
    <div class={styles.shell}>
      <TopBar />
      <main class={styles.content}>
        <Switch>
          <Match when={activeTab() === "sessions"}>
            <SessionsPlaceholder />
          </Match>
          <Match when={activeTab() === "activity"}>
            <ActivityPlaceholder />
          </Match>
          <Match when={activeTab() === "settings"}>
            <SettingsPlaceholder />
          </Match>
        </Switch>
      </main>
      <BottomTabs active={activeTab()} onSelect={setActiveTab} />
    </div>
  );
}
