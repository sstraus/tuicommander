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
            {agentType() ?? "Terminal"}
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
      </div>

      <div class={styles.time}>
        {props.session.state?.last_activity_ms
          ? formatTime(props.session.state.last_activity_ms)
          : ""}
      </div>
    </button>
  );
}
