import { type JSX, splitProps } from "solid-js";
import { cx } from "../../utils";
import s from "./FormControls.module.css";

export type ButtonGroupProps = JSX.HTMLAttributes<HTMLDivElement>;

/** Connected native actions; direct Button children retain their own size and behavior. */
export function ButtonGroup(props: ButtonGroupProps) {
	const [local, rest] = splitProps(props, ["class", "classList"]);
	return <div role="group" {...rest} class={cx(s.buttonGroup, local.class)} classList={local.classList} />;
}
