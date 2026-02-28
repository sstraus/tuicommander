import { createSignal, Match, Switch } from "solid-js";
import { TopBar } from "./components/TopBar";
import { BottomTabs, type TabId } from "./components/BottomTabs";
import { SessionsScreen } from "./screens/SessionsScreen";
import { useSessions } from "./useSessions";
import styles from "./MobileApp.module.css";

function ActivityPlaceholder() {
  return <div class={styles.placeholder}>Activity screen</div>;
}
function SettingsPlaceholder() {
  return <div class={styles.placeholder}>Settings screen</div>;
}

export default function MobileApp() {
  const [activeTab, setActiveTab] = createSignal<TabId>("sessions");
  const { sessions, loading, error, refresh, questionCount } = useSessions();

  return (
    <div class={styles.shell}>
      <TopBar notificationCount={questionCount()} />
      <main class={styles.content}>
        <Switch>
          <Match when={activeTab() === "sessions"}>
            <SessionsScreen
              sessions={sessions()}
              loading={loading()}
              error={error()}
              onRefresh={refresh}
              onSelectSession={(id) => {
                // Session detail screen (story 440) will handle this
                // For now just log the selection
                console.log("Navigate to session:", id);
              }}
            />
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
