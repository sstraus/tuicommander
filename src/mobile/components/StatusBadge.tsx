import styles from "./StatusBadge.module.css";

export type SessionStatus = "idle" | "busy" | "question" | "error" | "rate-limited";

interface StatusBadgeProps {
  status: SessionStatus;
}

const STATUS_LABELS: Record<SessionStatus, string> = {
  idle: "Idle",
  busy: "Activity",
  question: "Input",
  error: "Error",
  "rate-limited": "Rate Limited",
};

export function StatusBadge(props: StatusBadgeProps) {
  return (
    <span
      class={styles.badge}
      classList={{
        [styles.idle]: props.status === "idle",
        [styles.busy]: props.status === "busy",
        [styles.question]: props.status === "question",
        [styles.error]: props.status === "error" || props.status === "rate-limited",
      }}
    >
      {STATUS_LABELS[props.status]}
    </span>
  );
}
