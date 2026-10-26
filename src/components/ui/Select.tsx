import { createRenderEffect, type JSX, splitProps } from "solid-js";
import { cx } from "../../utils/cx";
import s from "./FormControls.module.css";

export type SelectProps = JSX.SelectHTMLAttributes<HTMLSelectElement> & { compact?: boolean };

/** Keep native keyboard, option, form and mobile picker behavior. */
export function Select(props: SelectProps) {
	const [local, rest] = splitProps(props, ["class", "classList", "children", "value", "compact"]);
	const element = (
		<select
			{...rest}
			data-form-control
			class={cx(s.control, s.select, local.compact && s.compact, local.class)}
			classList={local.classList}
		>
			{local.children}
		</select>
	) as HTMLSelectElement;
	// Solid spreads children before attributes. Apply multiple before inserting
	// options, then value afterwards, or the browser drops the initial selection.
	createRenderEffect(() => {
		const value = local.value;
		if (value !== undefined) element.value = String(value);
	});
	return element;
}
