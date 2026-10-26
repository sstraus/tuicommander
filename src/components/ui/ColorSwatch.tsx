import { type JSX, splitProps } from "solid-js";
import { cx } from "../../utils";
import s from "./FormControls.module.css";

export type ColorSwatchProps = JSX.ButtonHTMLAttributes<HTMLButtonElement> & {
	swatchColor?: string;
	selected?: boolean;
};

/** Immediate preset/clear action; the owner retains color selection state. */
export function ColorSwatch(props: ColorSwatchProps) {
	const [local, rest] = splitProps(props, ["class", "classList", "children", "swatchColor", "selected", "type"]);
	return (
		<button
			{...rest}
			type={local.type ?? "button"}
			data-form-control="swatch"
			data-clear={!local.swatchColor || undefined}
			aria-pressed={local.selected ?? false}
			class={cx(s.colorSwatch, local.class)}
			classList={local.classList}
		>
			<span class={s.colorValue} style={{ background: local.swatchColor }} aria-hidden="true">
				{local.children}
			</span>
		</button>
	);
}
