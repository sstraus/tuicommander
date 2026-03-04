import { Show, createSignal, createEffect, onCleanup } from "solid-js";
import { StatusBadge } from "../components/StatusBadge";
import { OutputView } from "../components/OutputView";
import { QuickActions } from "../components/QuickActions";
import { SuggestChips } from "../components/SuggestChips";
import { SlashMenuOverlay } from "../components/SlashMenuOverlay";
import { CommandInput } from "../components/CommandInput";
import type { SessionInfo } from "../useSessions";
import { deriveStatus } from "../utils/deriveStatus";
import { formatRetryCountdown } from "../utils/formatRetryCountdown";
import styles from "./SessionDetailScreen.module.css";

interface SessionDetailScreenProps {
  session: SessionInfo;
  sessionExists: boolean;
  onBack: () => void;
}

function projectName(cwd: string | null): string {
  if (!cwd) return "unknown";
  const parts = cwd.split("/");
  return parts[parts.length - 1] || "unknown";
}

export function SessionDetailScreen(props: SessionDetailScreenProps) {
  const status = () => deriveStatus(props.session);

  // Local dismiss flag for the slash menu overlay (resets when new items arrive)
  const [slashMenuDismissed, setSlashMenuDismissed] = createSignal(false);
  let lastSlashMenuItems: unknown = null;
  const showSlashMenu = () => {
    const items = props.session.state?.slash_menu_items;
    // Reset dismiss flag when items change
    if (items !== lastSlashMenuItems) {
      lastSlashMenuItems = items;
      setSlashMenuDismissed(false);
    }
    return !slashMenuDismissed() && items != null && items.length > 0;
  };

  // Live countdown for rate limit retry_after_ms
  const [retryRemaining, setRetryRemaining] = createSignal(0);

  createEffect(() => {
    const ms = props.session.state?.retry_after_ms;
    if (!ms || !props.session.state?.rate_limited) {
      setRetryRemaining(0);
      return;
    }
    setRetryRemaining(ms);
    const interval = setInterval(() => {
      setRetryRemaining((prev) => {
        const next = prev - 1000;
        if (next <= 0) {
          clearInterval(interval);
          return 0;
        }
        return next;
      });
    }, 1000);
    onCleanup(() => clearInterval(interval));
  });

  return (
    <div class={styles.screen}>
      <header class={styles.header}>
        <button class={styles.backBtn} onClick={props.onBack}>
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M15 18l-6-6 6-6" />
          </svg>
        </button>
        <div class={styles.headerInfo}>
          <span class={styles.agentName}>
            {props.session.state?.agent_type ?? "Terminal"}
          </span>
          <span class={styles.project}>{projectName(props.session.cwd)}</span>
        </div>
        <Show when={props.session.state?.usage_limit_pct != null}>
          <span
            class={styles.usageLabel}
            classList={{ [styles.danger]: (props.session.state!.usage_limit_pct ?? 0) > 80 }}
          >
            {props.session.state!.usage_limit_pct}%
          </span>
        </Show>
        <StatusBadge status={status()} />
      </header>

      <Show when={props.session.state?.agent_intent}>
        <div class={styles.intentLine}>
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <circle cx="12" cy="12" r="10" />
            <line x1="22" y1="12" x2="18" y2="12" />
            <line x1="6" y1="12" x2="2" y2="12" />
            <line x1="12" y1="6" x2="12" y2="2" />
            <line x1="12" y1="22" x2="12" y2="18" />
          </svg>
          <span class={styles.subText}>{props.session.state!.agent_intent}</span>
        </div>
      </Show>

      <Show when={props.session.state?.current_task}>
        <div class={styles.taskLine}>
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <circle cx="12" cy="12" r="3" />
            <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
          </svg>
          <span class={styles.subText}>{props.session.state!.current_task}</span>
        </div>
      </Show>

      <Show when={props.session.state?.progress != null}>
        <div class={styles.headerProgressBar}>
          <div class={styles.headerProgressFill} style={{ width: `${props.session.state!.progress}%` }} />
        </div>
      </Show>

      <Show when={props.session.state?.question_text}>
        <div class={styles.questionBar}>
          {props.session.state!.question_text}
        </div>
      </Show>

      <Show when={props.session.state?.last_error}>
        <div class={styles.errorBar}>
          {props.session.state!.last_error}
        </div>
      </Show>

      <Show when={props.session.state?.rate_limited}>
        <div class={styles.rateLimitBar}>
          <span>Rate limited</span>
          <Show when={retryRemaining() > 0}>
            <span class={styles.rateLimitCountdown}>
              {formatRetryCountdown(retryRemaining())}
            </span>
          </Show>
        </div>
      </Show>

      <div class={styles.outputArea}>
        <OutputView sessionId={props.session.session_id} />
        <Show when={!props.sessionExists}>
          <div class={styles.endedOverlay}>
            <span class={styles.endedText}>Session ended</span>
            <button class={styles.endedBackBtn} onClick={props.onBack}>Back</button>
          </div>
        </Show>
      </div>
      <Show when={props.session.state?.awaiting_input}>
        <QuickActions sessionId={props.session.session_id} />
      </Show>
      <Show when={props.session.state?.suggested_actions?.length}>
        <SuggestChips sessionId={props.session.session_id} items={props.session.state!.suggested_actions!} />
      </Show>
      <CommandInput sessionId={props.session.session_id} />
      <Show when={showSlashMenu()}>
        <SlashMenuOverlay
          sessionId={props.session.session_id}
          items={props.session.state!.slash_menu_items!}
          onDismiss={() => setSlashMenuDismissed(true)}
        />
      </Show>
    </div>
  );
}
