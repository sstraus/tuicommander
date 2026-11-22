import { type Component, createMemo, createSignal, For, Show } from "solid-js";
import { appLogger } from "../../stores/appLogger";
import type { ProgressFlow as Flow, FlowDetailRef, FlowEvent, FlowParticipant, FlowState } from "../../stores/progress";
import s from "./ProgressFlow.module.css";

const STATE_LABEL: Record<FlowState, string> = {
	busy: "working",
	idle: "idle",
	awaiting: "awaiting input",
	closed: "closed",
	running: "running",
	done: "done",
	interrupted: "interrupted",
};

/// The word printed before an arrow's label. The arrow's direction already
/// says who handed what to whom; the word says which kind of hand-off it is.
const KIND_LABEL: Record<FlowEvent["kind"], string> = {
	intent: "set out to",
	done: "done",
	blocked: "blocked",
	delegated: "delegated",
	message: "message",
	subagent_spawn: "asked",
	subagent_return: "reported",
};

export interface ProgressFlowProps {
	flow: Flow;
	/** Fetch the full prompt or report behind a subagent arrow. */
	fetchDetail: (detail: FlowDetailRef) => Promise<string>;
}

const TerminalIcon = () => (
	<svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
		<path d="M3 4h18v16H3zm2 2v12h14V6zm1.5 2 3.5 3-3.5 3-1-1 2.3-2-2.3-2zM11 14h5v1.5h-5z" />
	</svg>
);

const SubagentIcon = () => (
	<svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
		<path d="M12 3a4 4 0 1 1 0 8 4 4 0 0 1 0-8zM4 21c0-4.4 3.6-8 8-8s8 3.6 8 8z" />
	</svg>
);

const Head = (props: { dir: "left" | "right" }) => (
	<svg
		class={s.head}
		data-dir={props.dir}
		width="8"
		height="8"
		viewBox="0 0 8 8"
		fill="currentColor"
		aria-hidden="true"
	>
		<path d={props.dir === "right" ? "M0 0l8 4-8 4z" : "M8 0L0 4l8 4z"} />
	</svg>
);

/**
 * The journal as a sequence diagram: one column per participant, one row per
 * hand-off, oldest at the top. Order is the only axis — there is no time
 * ruler. Every value comes from the backend; this renders and nothing else.
 */
export const ProgressFlow: Component<ProgressFlowProps> = (props) => {
	const column = createMemo(() => new Map(props.flow.participants.map((p, i) => [p.id, i])));
	// Keyed by the event's stable id, never its position: a refresh re-sorts
	// the events and the oldest fall out of the window, so an index names a
	// different event after every live entry.
	const [expanded, setExpanded] = createSignal<ReadonlySet<string>>(new Set());
	const [fetched, setFetched] = createSignal<ReadonlyMap<string, string>>(new Map());

	const canExpand = (event: FlowEvent) => Boolean(event.text || event.detail);

	async function toggle(event: FlowEvent): Promise<void> {
		const open = new Set(expanded());
		if (open.has(event.id)) {
			open.delete(event.id);
			setExpanded(open);
			return;
		}
		open.add(event.id);
		setExpanded(open);
		if (!event.detail || fetched().has(event.id)) return;
		try {
			const text = await props.fetchDetail(event.detail);
			setFetched(new Map(fetched()).set(event.id, text));
		} catch (error) {
			appLogger.warn("store", "Progress Flow: full text unavailable", { error: String(error) });
			setFetched(new Map(fetched()).set(event.id, "Full text unavailable."));
		}
	}

	const labelText = (event: FlowEvent) => {
		if (!expanded().has(event.id)) return event.summary;
		if (event.text) return event.text;
		return fetched().get(event.id) ?? "Loading…";
	};

	const Label = (p: { event: FlowEvent }) => (
		<Show
			when={canExpand(p.event)}
			fallback={
				<span class={s.label}>
					<span class={s.kind}>{KIND_LABEL[p.event.kind]}</span>
					{p.event.summary}
				</span>
			}
		>
			<button
				type="button"
				class={`${s.label} ${s.expandable}`}
				aria-expanded={expanded().has(p.event.id)}
				onClick={() => void toggle(p.event)}
			>
				<span class={s.kind}>{KIND_LABEL[p.event.kind]}</span>
				<span class={expanded().has(p.event.id) ? s.full : undefined}>{labelText(p.event)}</span>
			</button>
		</Show>
	);

	const Header = (p: { participant: FlowParticipant }) => (
		<div class={s.header} data-kind={p.participant.kind}>
			<div class={s.name}>
				{p.participant.kind === "terminal" ? <TerminalIcon /> : <SubagentIcon />}
				<span title={p.participant.title}>{p.participant.title}</span>
			</div>
			<div class={s.meta}>
				<span class={s.badge} data-state={p.participant.state}>
					{STATE_LABEL[p.participant.state]}
				</span>
				<Show when={p.participant.agentType}>
					<span>{p.participant.agentType}</span>
				</Show>
				<Show when={p.participant.toolCalls > 0}>
					<span>{p.participant.toolCalls} tool calls</span>
				</Show>
			</div>
			<Show when={p.participant.intent}>
				<div class={s.intent}>{p.participant.intent}</div>
			</Show>
		</div>
	);

	return (
		<div class={s.flow} style={{ "--flow-cols": String(props.flow.participants.length) }}>
			<Show when={props.flow.truncated}>
				<p class={s.truncated}>Only the newest entries are drawn; the earliest hand-offs may be missing.</p>
			</Show>
			<div class={s.grid}>
				<div class={`${s.row} ${s.headRow}`}>
					<For each={props.flow.participants}>{(participant) => <Header participant={participant} />}</For>
				</div>
				<For each={props.flow.events}>
					{(event) => {
						const from = () => column().get(event.from) ?? 0;
						const to = () => (event.to === undefined ? undefined : column().get(event.to));
						const isArrow = () => to() !== undefined && to() !== from();
						return (
							<div class={s.row} data-kind={event.kind}>
								{/* Explicit columns: an auto-placed lane would skip the cells the
								    arrow occupies and spill into an implicit column. */}
								<For each={props.flow.participants}>
									{(_, lane) => <div class={s.lane} style={{ "grid-column": `${lane() + 1}` }} />}
								</For>
								<Show
									when={isArrow()}
									fallback={
										<div class={s.note} style={{ "grid-column": `${from() + 1}` }}>
											<Label event={event} />
										</div>
									}
								>
									<div
										class={s.arrow}
										data-dir={(to() as number) > from() ? "right" : "left"}
										style={{
											"grid-column": `${Math.min(from(), to() as number) + 1} / ${Math.max(from(), to() as number) + 2}`,
										}}
									>
										<Label event={event} />
										<div class={s.line}>
											<Head dir={(to() as number) > from() ? "right" : "left"} />
										</div>
									</div>
								</Show>
							</div>
						);
					}}
				</For>
			</div>
		</div>
	);
};

export default ProgressFlow;
