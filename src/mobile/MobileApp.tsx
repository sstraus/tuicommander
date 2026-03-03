import { createMemo, createSignal, Match, Show, Switch } from "solid-js";
import { TopBar } from "./components/TopBar";
import { BottomTabs, type TabId } from "./components/BottomTabs";
import { SessionsScreen } from "./screens/SessionsScreen";
import { SessionDetailScreen } from "./screens/SessionDetailScreen";
import { ActivityScreen } from "./screens/ActivityScreen";
import { SettingsScreen } from "./screens/SettingsScreen";
import { QuestionBanner } from "./components/QuestionBanner";
import { useSessions } from "./useSessions";
import { useMobileNotifications } from "./useMobileNotifications";
import styles from "./MobileApp.module.css";

export default function MobileApp() {
  const [activeTab, setActiveTab] = createSignal<TabId>("sessions");
  const [selectedSessionId, setSelectedSessionId] = createSignal<string | null>(null);
  const { sessions, loading, error, refresh, questionCount } = useSessions();
  useMobileNotifications(sessions);

  const selectedSession = createMemo(() => {
    const id = selectedSessionId();
    if (!id) return null;
    return sessions().find((s) => s.session_id === id) ?? null;
  });

  function navigateToSession(id: string) {
    setSelectedSessionId(id);
  }

  return (
    <div class={styles.shell}>
      <Show
        when={selectedSession()}
        fallback={
          <>
            <TopBar notificationCount={questionCount()} />
            <QuestionBanner
              sessions={sessions()}
              onNavigate={navigateToSession}
            />
            <main class={styles.content}>
              <Switch>
                <Match when={activeTab() === "sessions"}>
                  <SessionsScreen
                    sessions={sessions()}
                    loading={loading()}
                    error={error()}
                    onRefresh={refresh}
                    onSelectSession={navigateToSession}
                  />
                </Match>
                <Match when={activeTab() === "activity"}>
                  <ActivityScreen onNavigateSession={navigateToSession} />
                </Match>
                <Match when={activeTab() === "settings"}>
                  <SettingsScreen />
                </Match>
              </Switch>
            </main>
            <BottomTabs active={activeTab()} onSelect={setActiveTab} />
          </>
        }
      >
        {(session) => (
          <SessionDetailScreen
            session={session()}
            onBack={() => setSelectedSessionId(null)}
          />
        )}
      </Show>
    </div>
  );
}
