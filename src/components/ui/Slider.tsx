import { type JSX, splitProps } from "solid-js";
import { cx } from "../../utils";
import s from "./FormControls.module.css";

export type SliderProps = Omit<JSX.InputHTMLAttributes<HTMLInputElement>, "type">;

/** Native range input; the consumer owns input and commit events. */
export function Slider(props: SliderProps) {
	const [local, rest] = splitProps(props, ["class", "classList"]);
	return (
		<input
			{...rest}
			type="range"
			data-form-control="slider"
			class={cx(s.slider, local.class)}
			classList={local.classList}
		/>
	);
}
