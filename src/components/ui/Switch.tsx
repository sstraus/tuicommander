import { type JSX, splitProps } from "solid-js";
import { cx } from "../../utils";
import s from "./FormControls.module.css";

export type SwitchProps = Omit<JSX.InputHTMLAttributes<HTMLInputElement>, "type">;

/** Native checkbox semantics with one shared switch appearance and full row-height target. */
export function Switch(props: SwitchProps) {
	const [local, rest] = splitProps(props, ["class", "classList"]);
	return (
		<span data-form-control="switch" class={cx(s.switchControl, local.class)} classList={local.classList}>
			<input {...rest} type="checkbox" role={props.role ?? "switch"} class={s.switchInput} />
			<span class={s.switchTrack} aria-hidden="true" />
		</span>
	);
}
