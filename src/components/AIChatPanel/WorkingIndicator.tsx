import { type Component, For, Match, Switch } from "solid-js";
import s from "./WorkingIndicator.module.css";

export type WorkingEffect = "shimmer" | "dots" | "orb" | "typewriter";

/** Opt-in candidates only: the host still owns busy, retry and settlement. */
export const WorkingIndicator: Component<{ effect: WorkingEffect; label?: string }> = (props) => (
	<div class={s.indicator} data-working-effect={props.effect} role="status" aria-label={props.label ?? "Working…"}>
		<div class={s.visual} aria-hidden="true">
			<Switch>
				<Match when={props.effect === "shimmer"}>
					<span class={s.shimmer}>{props.label ?? "Working…"}</span>
				</Match>
				<Match when={props.effect === "dots"}>
					<span class={s.dots}>
						<i />
						<i />
						<i />
					</span>
					<span>{props.label ?? "Working…"}</span>
				</Match>
				<Match when={props.effect === "orb"}>
					<span class={s.orb}>
						<i />
					</span>
					<span>{props.label ?? "Working…"}</span>
				</Match>
				<Match when={props.effect === "typewriter"}>
					<span class={s.typewriter}>
						<For each={["Working…", "Thinking…", "Continuing…"]}>{(word) => <span>{word}</span>}</For>
					</span>
				</Match>
			</Switch>
		</div>
	</div>
);
