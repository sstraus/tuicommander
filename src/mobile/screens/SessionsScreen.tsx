import { For, Index, Show, createSignal } from "solid-js";
import { rpc } from "../../transport";
import { appLogger } from "../../stores/appLogger";
import { SessionCard } from "../components/SessionCard";
import { HeroMetrics } from "../components/HeroMetrics";
import { NewSessionSheet } from "../components/NewSessionSheet";
import type { SessionInfo } from "../useSessions";
import styles from "./SessionsScreen.module.css";

interface SessionsScreenProps {
  sessions: SessionInfo[];
  loading: boolean;
  refreshing: boolean;
  error: string | null;
  onRefresh: () => void;
  onSelectSession: (sessionId: string) => void;
}

const PULL_THRESHOLD = 60;

export function SessionsScreen(props: SessionsScreenProps) {
  const [pulling, setPulling] = createSignal(false);
  const [pullY, setPullY] = createSignal(0);
  const [showNewSession, setShowNewSession] = createSignal(false);
  const [repos, setRepos] = createSignal<string[]>([]);
  let startY = 0;
  let listEl: HTMLDivElement | undefined;

  async function handleKill(sessionId: string) {
    if (!window.confirm("Kill this session?")) return;
    try {
      await rpc("close_pty", { sessionId });
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      appLogger.warn("network", `Failed to kill session: ${msg}`);
    }
  }

  async function openNewSessionSheet() {
    try {
      const config = await rpc<{ repos?: Record<string, unknown> }>("load_repositories");
      setRepos(config.repos ? Object.keys(config.repos) : []);
    } catch {
      setRepos([]);
    }
    setShowNewSession(true);
  }

  function onTouchStart(e: TouchEvent) {
    if (listEl && listEl.scrollTop === 0) {
      startY = e.touches[0].clientY;
      setPulling(true);
    }
  }

  function onTouchMove(e: TouchEvent) {
    if (!pulling()) return;
    const dy = Math.max(0, e.touches[0].clientY - startY);
    setPullY(Math.min(dy, PULL_THRESHOLD * 2));
  }

  function onTouchEnd() {
    if (pullY() >= PULL_THRESHOLD) {
      props.onRefresh();
    }
    setPullY(0);
    setPulling(false);
  }

  return (
    <div
      ref={listEl}
      class={styles.list}
      onTouchStart={onTouchStart}
      onTouchMove={onTouchMove}
      onTouchEnd={onTouchEnd}
    >
      <Show when={pullY() > 0}>
        <div
          class={styles.pullIndicator}
          style={{ height: `${pullY()}px` }}
        >
          <span
            classList={{ [styles.pullReady]: pullY() >= PULL_THRESHOLD }}
          >
            {pullY() >= PULL_THRESHOLD ? "Release to refresh" : "Pull to refresh"}
          </span>
        </div>
      </Show>

      <Show when={props.refreshing && props.sessions.length > 0}>
        <div class={styles.refreshSpinner}>
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M21 12a9 9 0 1 1-6.22-8.56" />
          </svg>
        </div>
      </Show>

      <Show when={props.error}>
        <div class={styles.errorBanner}>
          {props.error}
        </div>
      </Show>

      <Show when={props.sessions.length > 0}>
        <HeroMetrics
          activeCount={props.sessions.length}
          awaitingCount={props.sessions.filter((s) => s.state?.awaiting_input).length}
        />
      </Show>

      <Show when={props.loading && props.sessions.length === 0}>
        <Index each={[0, 1, 2]}>
          {() => (
            <div class={styles.skeleton}>
              <div class={styles.skeletonIcon} />
              <div class={styles.skeletonBody}>
                <div class={styles.skeletonLine} style={{ width: "40%" }} />
                <div class={styles.skeletonLine} style={{ width: "60%" }} />
              </div>
            </div>
          )}
        </Index>
      </Show>

      <Show when={!props.loading && props.sessions.length === 0 && !props.error}>
        <div class={styles.empty}>
          <div class={styles.emptyIcon}>
            <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
              <rect x="2" y="3" width="20" height="14" rx="2" />
              <path d="M8 21h8M12 17v4" />
            </svg>
          </div>
          <span class={styles.emptyTitle}>No active sessions</span>
          <span class={styles.emptyHint}>Start an agent from TUICommander to see it here</span>
        </div>
      </Show>

      <For each={props.sessions}>
        {(session) => (
          <SessionCard
            session={session}
            onSelect={props.onSelectSession}
            onKill={handleKill}
          />
        )}
      </For>

      <button class={styles.fab} onClick={openNewSessionSheet} data-testid="new-session-fab">
        <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <line x1="12" y1="5" x2="12" y2="19" />
          <line x1="5" y1="12" x2="19" y2="12" />
        </svg>
      </button>

      <Show when={showNewSession()}>
        <NewSessionSheet repos={repos()} onDismiss={() => setShowNewSession(false)} />
      </Show>
    </div>
  );
}
