import { createSignal, Match, Switch } from "solid-js";
import { TopBar } from "./components/TopBar";
import { BottomTabs, type TabId } from "./components/BottomTabs";
import { SessionsScreen } from "./screens/SessionsScreen";
import { ActivityScreen } from "./screens/ActivityScreen";
import { SettingsScreen } from "./screens/SettingsScreen";
import { QuestionBanner } from "./components/QuestionBanner";
import { useSessions } from "./useSessions";
import styles from "./MobileApp.module.css";

export default function MobileApp() {
  const [activeTab, setActiveTab] = createSignal<TabId>("sessions");
  const { sessions, loading, error, refresh, questionCount } = useSessions();

  return (
    <div class={styles.shell}>
      <TopBar notificationCount={questionCount()} />
      <QuestionBanner
        sessions={sessions()}
        onNavigate={(id) => {
          setActiveTab("sessions");
          console.log("Navigate to session:", id);
        }}
      />
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
            <ActivityScreen
              onNavigateSession={(id) => {
                setActiveTab("sessions");
                console.log("Navigate to session:", id);
              }}
            />
          </Match>
          <Match when={activeTab() === "settings"}>
            <SettingsScreen />
          </Match>
        </Switch>
      </main>
      <BottomTabs active={activeTab()} onSelect={setActiveTab} />
    </div>
  );
}
