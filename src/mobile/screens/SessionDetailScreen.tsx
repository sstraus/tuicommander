import { Show } from "solid-js";
import { StatusBadge, type SessionStatus } from "../components/StatusBadge";
import { OutputView } from "../components/OutputView";
import { QuickActions } from "../components/QuickActions";
import { CommandInput } from "../components/CommandInput";
import type { SessionInfo } from "../useSessions";
import styles from "./SessionDetailScreen.module.css";

interface SessionDetailScreenProps {
  session: SessionInfo;
  onBack: () => void;
}

function deriveStatus(session: SessionInfo): SessionStatus {
  const s = session.state;
  if (!s) return "idle";
  if (s.rate_limited) return "rate-limited";
  if (s.last_error) return "error";
  if (s.awaiting_input) return "question";
  if (s.is_busy) return "busy";
  return "idle";
}

function projectName(cwd: string | null): string {
  if (!cwd) return "unknown";
  const parts = cwd.split("/");
  return parts[parts.length - 1] || "unknown";
}

export function SessionDetailScreen(props: SessionDetailScreenProps) {
  const status = () => deriveStatus(props.session);

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

      <OutputView sessionId={props.session.session_id} />
      <QuickActions sessionId={props.session.session_id} />
      <CommandInput sessionId={props.session.session_id} />
    </div>
  );
}
