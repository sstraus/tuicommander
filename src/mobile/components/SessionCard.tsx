import { Show } from "solid-js";
import { AGENT_DISPLAY, type AgentType } from "../../agents";
import { AgentIcon } from "../../components/ui/AgentIcon";
import { SubAgentIcon } from "../../components/ui/SubAgentIcon";
import { displayTask } from "../../utils/activitySnapshot";
import type { SessionInfo } from "../useSessions";
import { isKnownAgentType } from "../utils/sessionKind";
import { useDebouncedStatus } from "../utils/useDebouncedStatus";
import styles from "./SessionCard.module.css";
import { StatusBadge } from "./StatusBadge";

interface SessionCardProps {
	session: SessionInfo;
	/** Nesting level under the spawning agent; 0 or absent for a top-level row. */
	depth?: number;
	/** Last row of its sibling group: the left guide line ends here. */
	lastInGroup?: boolean;
	/** Parent label for the sub-agent marker; set when the session was spawned by another agent. */
	spawnedBy?: string;
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
	return parts.at(-1)! || "unknown";
}

/** A plain shell. Marks the card so a PTY is not read as an unidentified agent. */
function TerminalIcon() {
	return (
		<svg
			width="22"
			height="22"
			viewBox="0 0 24 24"
			fill="none"
			stroke="currentColor"
			stroke-width="2"
			stroke-linecap="round"
			stroke-linejoin="round"
			aria-hidden="true"
			data-testid="pty-icon"
		>
			<rect x="2" y="4" width="20" height="16" rx="2" />
			<path d="M7 9l3 3-3 3" />
			<path d="M13 15h4" />
		</svg>
	);
}

export function SessionCard(props: SessionCardProps) {
	const status = useDebouncedStatus(() => props.session);
	const agentType = () => props.session.state?.agent_type;
	const task = () => displayTask(props.session.state?.current_task, agentType());
	const agentColor = () => {
		const t = agentType();
		if (isKnownAgentType(t)) return AGENT_DISPLAY[t].color;
		return "var(--fg-muted)";
	};

	return (
		<div
			class={styles.card}
			classList={{
				[styles.question]: status() === "question",
				[styles.child]: (props.depth ?? 0) > 0,
				[styles.childLast]: (props.depth ?? 0) > 0 && !!props.lastInGroup,
				[styles.childDeep]: (props.depth ?? 0) > 1,
			}}
			data-testid={(props.depth ?? 0) > 0 ? "subagent-row" : undefined}
		>
			<button
				class={styles.cardMain}
				aria-label={`Open session ${props.session.display_name || agentType() || "Terminal"}`}
				onClick={() => props.onSelect(props.session.session_id)}
			>
				<div class={styles.iconCol} style={{ color: agentColor() }}>
					<Show when={isKnownAgentType(agentType())} fallback={<TerminalIcon />}>
						<AgentIcon agent={agentType() as AgentType} size={22} />
					</Show>
				</div>

				<div class={styles.body}>
					<div class={styles.topRow}>
						<Show when={props.spawnedBy}>
							<SubAgentIcon parent={props.spawnedBy!} class={styles.subAgentTag} iconClass={styles.subAgentIcon} />
						</Show>
						<span class={styles.name}>{props.session.display_name || agentType() || "Terminal"}</span>
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
					<Show
						when={props.session.state?.agent_intent}
						fallback={
							<Show when={props.session.state?.last_prompt}>
								<div class={styles.subRow} data-testid="prompt-row">
									<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
										<path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
									</svg>
									<span class={styles.subRowText}>{props.session.state!.last_prompt}</span>
								</div>
							</Show>
						}
					>
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

					{/* Current task sub-row. `displayTask` drops a bare spinner verb
				    rather than repeating the Working badge. */}
					<Show when={task()}>
						<div class={styles.subRow} data-testid="task-row">
							<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
								<circle cx="12" cy="12" r="3" />
								<path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
							</svg>
							<span class={styles.subRowText}>{task()}</span>
						</div>
					</Show>

					{/* Progress is its own signal (OSC 9;4), independent of the task text. */}
					<Show when={props.session.state?.progress != null}>
						<div class={styles.subRow} data-testid="progress-row">
							<div class={styles.progressBar} data-testid="progress-bar">
								<div
									class={styles.progressFill}
									style={{ transform: `scaleX(${props.session.state!.progress! / 100})` }}
								/>
							</div>
						</div>
					</Show>

					{/* Usage limit */}
					<Show when={props.session.state?.usage_limit_pct != null}>
						<span class={styles.usageLabel} data-testid="usage-label">
							{props.session.state!.usage_limit_pct}% used
						</span>
					</Show>
				</div>
			</button>

			<div class={styles.actions}>
				<span class={styles.time}>
					{props.session.state?.last_activity_ms ? formatTime(props.session.state.last_activity_ms) : ""}
				</span>
				<Show when={props.onKill}>
					<button
						class={styles.killBtn}
						aria-label={`Kill session ${props.session.display_name || agentType() || "Terminal"}`}
						data-testid="kill-btn"
						onClick={() => props.onKill!(props.session.session_id)}
					>
						<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
							<line x1="18" y1="6" x2="6" y2="18" />
							<line x1="6" y1="6" x2="18" y2="18" />
						</svg>
					</button>
				</Show>
			</div>
		</div>
	);
}
