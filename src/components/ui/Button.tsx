import { type JSX, splitProps } from "solid-js";
import { cx } from "../../utils/cx";
import s from "./FormControls.module.css";

type ButtonBaseProps = JSX.ButtonHTMLAttributes<HTMLButtonElement> & {
	variant?: "primary" | "secondary" | "ghost" | "danger" | "removalToggle";
	size?: "default" | "compact";
};

export type ButtonProps = ButtonBaseProps &
	(
		| { shape?: "default"; geometry?: "default" | "stretch"; circular?: never }
		| { shape: "icon"; circular?: boolean; geometry?: never; "aria-label": string }
	);

/** Native action button; list rows and cards retain their layout semantics. */
export function Button(props: ButtonProps) {
	const [local, rest] = splitProps(props, [
		"class",
		"classList",
		"variant",
		"size",
		"geometry",
		"shape",
		"circular",
		"type",
		"children",
	]);
	return (
		<button
			{...rest}
			type={local.type ?? "button"}
			data-form-control
			class={cx(
				s.control,
				s.button,
				!(local.shape === "icon" && local.circular) && s[local.variant ?? "secondary"],
				local.shape !== "icon" && local.size === "compact" && s.compact,
				local.shape === "icon" && s.iconButton,
				local.shape === "icon" && local.circular && s.circleButton,
				local.geometry === "stretch" && s.stretchButton,
				local.class,
			)}
			classList={local.classList}
		>
			{local.shape === "icon" && local.circular ? (
				<span aria-hidden="true" class={cx(s.circleSurface, s[local.variant ?? "secondary"])}>
					{local.children}
				</span>
			) : (
				local.children
			)}
		</button>
	);
}
