import { For, Show, createSignal } from "solid-js";
import { SessionCard } from "../components/SessionCard";
import type { SessionInfo } from "../useSessions";
import styles from "./SessionsScreen.module.css";

interface SessionsScreenProps {
  sessions: SessionInfo[];
  loading: boolean;
  error: string | null;
  onRefresh: () => void;
  onSelectSession: (sessionId: string) => void;
}

const PULL_THRESHOLD = 60;

export function SessionsScreen(props: SessionsScreenProps) {
  const [pulling, setPulling] = createSignal(false);
  const [pullY, setPullY] = createSignal(0);
  let startY = 0;
  let listEl: HTMLDivElement | undefined;

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

      <Show when={props.error}>
        <div class={styles.errorBanner}>
          {props.error}
        </div>
      </Show>

      <Show when={props.loading && props.sessions.length === 0}>
        <div class={styles.empty}>Loading sessions...</div>
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
          />
        )}
      </For>
    </div>
  );
}
