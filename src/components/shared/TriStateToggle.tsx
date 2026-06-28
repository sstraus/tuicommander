import type { Component } from "solid-js";
import { onClickKeyDown } from "../../utils/a11y";
import s from "./TriStateToggle.module.css";

export type TriState = "show" | "default" | "hide";

interface TriStateToggleProps {
	value: TriState;
	label: string;
	defaultLabel?: string;
	onChange: (value: TriState) => void;
}

const CYCLE: TriState[] = ["hide", "default", "show"];

export const TriStateToggle: Component<TriStateToggleProps> = (props) => {
	const handleClick = () => {
		const idx = CYCLE.indexOf(props.value);
		props.onChange(CYCLE[(idx + 1) % CYCLE.length]);
	};

	return (
		<div class={s.triToggle}>
			<div
				class={s.track}
				data-value={props.value}
				onClick={handleClick}
				onKeyDown={onClickKeyDown(handleClick)}
				role="button"
				tabIndex={0}
			>
				<div class={s.knob} />
			</div>
			<span class={s.label}>
				{props.label}
				{props.value === "default" && props.defaultLabel && <span class={s.stateHint}> ({props.defaultLabel})</span>}
			</span>
		</div>
	);
};
