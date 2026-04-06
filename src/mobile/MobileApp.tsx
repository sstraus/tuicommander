import { createEffect, createMemo, createSignal, Match, Show, Switch } from "solid-js";
import { TopBar } from "./components/TopBar";
import { BottomTabs, type TabId } from "./components/BottomTabs";
import { SessionsScreen } from "./screens/SessionsScreen";
import { SessionDetailScreen } from "./screens/SessionDetailScreen";
import { ActivityScreen } from "./screens/ActivityScreen";
import { SettingsScreen } from "./screens/SettingsScreen";
import { QuestionBanner } from "./components/QuestionBanner";
import { useSessions } from "./useSessions";
import { useMobileNotifications } from "./useMobileNotifications";
import { appLogger } from "../stores/appLogger";
import { notesStore } from "../stores/notes";
import styles from "./MobileApp.module.css";

// Register service worker for push notifications (only on HTTPS or localhost)
if ("serviceWorker" in navigator &&
    (location.protocol === "https:" || location.hostname === "localhost" || location.hostname === "127.0.0.1")) {
  navigator.serviceWorker.register("/sw.js").catch((err: unknown) => {
    // Log but don't block — push will be unavailable
    appLogger.warn("sw", "Service worker registration failed", err);
  });
}

/** Extract session ID from deep link path like /mobile/session/<id> */
function sessionIdFromUrl(): string | null {
  const match = location.pathname.match(/^\/mobile\/session\/(.+)/);
  return match ? decodeURIComponent(match[1]) : null;
}

export default function MobileApp() {
  const [activeTab, setActiveTab] = createSignal<TabId>("sessions");
  const [selectedSessionId, setSelectedSessionId] = createSignal<string | null>(sessionIdFromUrl());
  const { sessions, loading, refreshing, error, refresh, questionCount } = useSessions();
  useMobileNotifications(sessions);
  notesStore.hydrate();

  // Keep the last known session data so the detail screen stays mounted
  // and can show the "Session ended" overlay after a session closes.
  const [lastKnownSession, setLastKnownSession] = createSignal<ReturnType<typeof sessions>[number] | null>(null);

  const liveSession = createMemo(() => {
    const id = selectedSessionId();
    if (!id) return null;
    return sessions().find((s) => s.session_id === id) ?? null;
  });

  // Update last known session whenever live data arrives; keep stale value when gone
  createEffect(() => {
    const live = liveSession();
    if (live) setLastKnownSession(live);
  });

  const sessionExists = createMemo(() => {
    const id = selectedSessionId();
    if (!id) return false;
    return sessions().some((s) => s.session_id === id);
  });

  function navigateToSession(id: string) {
    setSelectedSessionId(id);
  }

  function handleBack() {
    setSelectedSessionId(null);
    setLastKnownSession(null);
  }

  const showDetail = () => selectedSessionId() !== null && lastKnownSession() !== null;

  return (
    <div class={styles.shell}>
      <Show
        when={showDetail()}
        fallback={
          <>
            <TopBar notificationCount={questionCount()} isConnected={error() === null} />
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
                    refreshing={refreshing()}
                    error={error()}
                    onRefresh={refresh}
                    onSelectSession={navigateToSession}
                  />
                </Match>
                <Match when={activeTab() === "activity"}>
                  <ActivityScreen onNavigateSession={navigateToSession} />
                </Match>
                <Match when={activeTab() === "settings"}>
                  <SettingsScreen isConnected={error() === null} />
                </Match>
              </Switch>
            </main>
            <BottomTabs active={activeTab()} onSelect={setActiveTab} />
          </>
        }
      >
        <SessionDetailScreen
          session={lastKnownSession()!}
          sessionExists={sessionExists()}
          onBack={handleBack}
        />
      </Show>
    </div>
  );
}
