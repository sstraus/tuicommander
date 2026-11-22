import styles from "./StatusBadge.module.css";

export type SessionStatus =
	| "idle"
	| "busy"
	| "background"
	| "sub-tasks"
	| "question"
	| "stalled"
	| "error"
	| "rate-limited"
	| "unseen";

interface StatusBadgeProps {
	status: SessionStatus;
}

const STATUS_LABELS: Record<SessionStatus, string> = {
	idle: "Idle",
	busy: "Working",
	background: "Background",
	"sub-tasks": "Sub-tasks",
	question: "Input",
	stalled: "Stalled",
	error: "Error",
	"rate-limited": "Rate Limited",
	unseen: "Finished",
};

export function StatusBadge(props: StatusBadgeProps) {
	return (
		<span
			class={styles.badge}
			classList={{
				[styles.idle]: props.status === "idle",
				[styles.busy]: props.status === "busy" || props.status === "sub-tasks",
				[styles.background]: props.status === "background",
				[styles.question]: props.status === "question",
				[styles.stalled]: props.status === "stalled",
				[styles.error]: props.status === "error" || props.status === "rate-limited",
				[styles.unseen]: props.status === "unseen",
			}}
		>
			{STATUS_LABELS[props.status]}
		</span>
	);
}
