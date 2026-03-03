import { Show, createSignal, createEffect, onCleanup } from "solid-js";
import { StatusBadge } from "../components/StatusBadge";
import { OutputView } from "../components/OutputView";
import { QuickActions } from "../components/QuickActions";
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
        <StatusBadge status={status()} />
      </header>

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
      <CommandInput sessionId={props.session.session_id} />
    </div>
  );
}
