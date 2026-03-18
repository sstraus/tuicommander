import { Show } from "solid-js";
import { AgentIcon } from "../../components/ui/AgentIcon";
import { AGENT_DISPLAY, type AgentType, AGENT_TYPES } from "../../agents";
import { StatusBadge } from "./StatusBadge";
import type { SessionInfo } from "../useSessions";
import { deriveStatus } from "../utils/deriveStatus";
import styles from "./SessionCard.module.css";

interface SessionCardProps {
  session: SessionInfo;
  onSelect: (sessionId: string) => void;
  onKill?: (sessionId: string) => void;
}

function formatTime(ms: number): string {
  const delta = Date.now() - ms;
  if (delta < 60_000) return "now";
  if (delta < 3_600_000) return `${Math.floor(delta / 60_000)}m ago`;
  if (delta < 86_400_000) return `${Math.floor(delta / 3_600_000)}h ago`;
  return `${Math.floor(delta / 86_400_000)}d ago`;
}

function projectName(cwd: string | null): string {
  if (!cwd) return "unknown";
  const parts = cwd.split("/");
  return parts[parts.length - 1] || "unknown";
}

function isAgentType(value: string): value is AgentType {
  return (AGENT_TYPES as readonly string[]).includes(value);
}

export function SessionCard(props: SessionCardProps) {
  const status = () => deriveStatus(props.session);
  const agentType = () => props.session.state?.agent_type;
  const agentColor = () => {
    const t = agentType();
    if (t && isAgentType(t)) return AGENT_DISPLAY[t].color;
    return "var(--fg-muted)";
  };

  return (
    <button
      class={styles.card}
      classList={{ [styles.question]: status() === "question" }}
      onClick={() => props.onSelect(props.session.session_id)}
    >
      <div class={styles.iconCol} style={{ color: agentColor() }}>
        <Show
          when={agentType() && isAgentType(agentType()!)}
          fallback={<span class={styles.termIcon}>{">"}_</span>}
        >
          <AgentIcon agent={agentType()! as AgentType} size={22} />
        </Show>
      </div>

      <div class={styles.body}>
        <div class={styles.topRow}>
          <span class={styles.name}>
            {props.session.display_name || agentType() || "Terminal"}
          </span>
          <StatusBadge status={status()} />
        </div>
        <div class={styles.meta}>
          <span class={styles.project}>{projectName(props.session.cwd)}</span>
          <Show when={props.session.worktree_branch}>
            <span class={styles.branch}>{props.session.worktree_branch}</span>
          </Show>
        </div>
        <Show when={props.session.state?.question_text}>
          <div class={styles.snippet}>{props.session.state!.question_text}</div>
        </Show>

        {/* Intent or last prompt sub-row */}
        <Show when={props.session.state?.agent_intent} fallback={
          <Show when={props.session.state?.last_prompt}>
            <div class={styles.subRow} data-testid="prompt-row">
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
              </svg>
              <span class={styles.subRowText}>{props.session.state!.last_prompt}</span>
            </div>
          </Show>
        }>
          <div class={styles.subRow} data-testid="intent-row">
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <circle cx="12" cy="12" r="10" />
              <line x1="22" y1="12" x2="18" y2="12" />
              <line x1="6" y1="12" x2="2" y2="12" />
              <line x1="12" y1="6" x2="12" y2="2" />
              <line x1="12" y1="22" x2="12" y2="18" />
            </svg>
            <span class={styles.subRowText}>{props.session.state!.agent_intent}</span>
          </div>
        </Show>

        {/* Current task sub-row */}
        <Show when={props.session.state?.current_task}>
          <div class={styles.subRow} data-testid="task-row">
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <circle cx="12" cy="12" r="3" />
              <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
            </svg>
            <span class={styles.subRowText}>{props.session.state!.current_task}</span>
            <Show when={props.session.state?.progress != null}>
              <div class={styles.progressBar} data-testid="progress-bar">
                <div class={styles.progressFill} style={{ width: `${props.session.state!.progress}%` }} />
              </div>
            </Show>
          </div>
        </Show>

        {/* Usage limit */}
        <Show when={props.session.state?.usage_limit_pct != null}>
          <span class={styles.usageLabel} data-testid="usage-label">{props.session.state!.usage_limit_pct}% used</span>
        </Show>
      </div>

      <div class={styles.actions}>
        <span class={styles.time}>
          {props.session.state?.last_activity_ms
            ? formatTime(props.session.state.last_activity_ms)
            : ""}
        </span>
        <Show when={props.onKill}>
          <button
            class={styles.killBtn}
            data-testid="kill-btn"
            onClick={(e) => {
              e.stopPropagation();
              props.onKill!(props.session.session_id);
            }}
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <line x1="18" y1="6" x2="6" y2="18" />
              <line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        </Show>
      </div>
    </button>
  );
}
