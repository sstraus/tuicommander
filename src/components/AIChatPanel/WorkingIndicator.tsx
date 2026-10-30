import type { Component } from "solid-js";
import s from "./WorkingIndicator.module.css";

/** The host owns busy, retry and settlement; the orb only renders that state. */
export const WorkingIndicator: Component<{ label?: string }> = (props) => (
	<div class={s.indicator} role="status" aria-label={props.label ?? "Working…"}>
		<span class={s.orb} aria-hidden="true">
			<i />
		</span>
		<span aria-hidden="true">{props.label ?? "Working…"}</span>
	</div>
);
