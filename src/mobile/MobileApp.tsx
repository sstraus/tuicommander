import { createSignal, For, Match, Show, Switch } from "solid-js";
import { TopBar } from "./components/TopBar";
import { BottomTabs, type TabId } from "./components/BottomTabs";
import { useSessions, type SessionInfo } from "./useSessions";
import styles from "./MobileApp.module.css";

/** Placeholder: shows real session data to validate the data layer */
function SessionsPlaceholder(props: { sessions: SessionInfo[]; loading: boolean; error: string | null }) {
  return (
    <div class={styles.placeholder}>
      <Show when={props.loading}>Loading sessions...</Show>
      <Show when={props.error}>
        <span style={{ color: "var(--error)" }}>Error: {props.error}</span>
      </Show>
      <Show when={!props.loading && !props.error}>
        <Show when={props.sessions.length === 0} fallback={
          <div style={{ padding: "16px", width: "100%" }}>
            <For each={props.sessions}>
              {(s) => (
                <div style={{
                  padding: "12px",
                  "border-bottom": "1px solid var(--border)",
                  "font-size": "13px",
                }}>
                  <div style={{ color: "var(--fg-primary)" }}>
                    {s.state?.agent_type ?? "Terminal"} — {s.session_id.slice(0, 8)}
                  </div>
                  <div style={{ color: "var(--fg-muted)", "margin-top": "4px" }}>
                    {s.cwd ?? "no cwd"}
                    {s.state?.awaiting_input ? " — awaiting input" : ""}
                    {s.state?.is_busy ? " — busy" : ""}
                  </div>
                </div>
              )}
            </For>
          </div>
        }>
          <span>No active sessions</span>
        </Show>
      </Show>
    </div>
  );
}

function ActivityPlaceholder() {
  return <div class={styles.placeholder}>Activity screen</div>;
}
function SettingsPlaceholder() {
  return <div class={styles.placeholder}>Settings screen</div>;
}

export default function MobileApp() {
  const [activeTab, setActiveTab] = createSignal<TabId>("sessions");
  const { sessions, loading, error, questionCount } = useSessions();

  return (
    <div class={styles.shell}>
      <TopBar notificationCount={questionCount()} />
      <main class={styles.content}>
        <Switch>
          <Match when={activeTab() === "sessions"}>
            <SessionsPlaceholder sessions={sessions()} loading={loading()} error={error()} />
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
