import { type JSX, splitProps } from "solid-js";
import { cx } from "../../utils";
import s from "./FormControls.module.css";

export type ColorInputProps = Omit<JSX.InputHTMLAttributes<HTMLInputElement>, "type" | "children"> & {
	children?: JSX.Element;
	selected?: boolean;
};

/** Existing swatch-overlay picker: the native input retains focus, events and OS picker. */
export function ColorInput(props: ColorInputProps) {
	const [local, rest] = splitProps(props, ["class", "classList", "children", "selected"]);
	return (
		<span
			data-form-control="color"
			data-selected={local.selected || undefined}
			class={cx(s.colorInput, local.class)}
			classList={local.classList}
		>
			<span class={s.colorValue} style={{ background: props.value as string }} aria-hidden="true">
				{local.children}
			</span>
			<input {...rest} type="color" />
		</span>
	);
}
