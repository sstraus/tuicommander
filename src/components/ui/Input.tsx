import { type JSX, splitProps } from "solid-js";
import { cx } from "../../utils/cx";
import s from "./FormControls.module.css";

export type InputProps = JSX.InputHTMLAttributes<HTMLInputElement> & { compact?: boolean };

/** Text, numeric and date fields; checkboxes and sliders retain their own controls. */
export function Input(props: InputProps) {
	const [local, rest] = splitProps(props, ["class", "classList", "compact"]);
	return (
		<input
			{...rest}
			data-form-control
			class={cx(s.control, local.compact && s.compact, local.class)}
			classList={local.classList}
		/>
	);
}
