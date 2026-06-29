import type { Component } from "solid-js";
import { cx } from "../../utils";
import { onClickKeyDown } from "../../utils/a11y";
import s from "../PromptOverlay/PromptOverlay.module.css";

export interface PromptOptionProps {
	index: number;
	label: string;
	selected: boolean;
	onClick: () => void;
}

export const PromptOption: Component<PromptOptionProps> = (props) => {
	return (
		<div
			class={cx(s.option, props.selected && s.selected)}
			role="button"
			tabIndex={0}
			onClick={props.onClick}
			onKeyDown={onClickKeyDown(props.onClick)}
		>
			<span class={s.optionKey}>{props.index + 1}</span>
			<span class={s.optionText}>{props.label}</span>
		</div>
	);
};
