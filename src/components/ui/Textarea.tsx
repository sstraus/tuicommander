import { createEffect, type JSX, onCleanup, splitProps } from "solid-js";
import { cx } from "../../utils";
import s from "./FormControls.module.css";

export type TextareaProps = JSX.TextareaHTMLAttributes<HTMLTextAreaElement> & {
	/** Compact content-sized editor, bounded by three shared control heights. */
	autoGrow?: boolean;
};

export function Textarea(props: TextareaProps) {
	const [local, rest] = splitProps(props, ["class", "classList", "autoGrow", "ref"]);
	let element: HTMLTextAreaElement | undefined;
	const resize = () => {
		if (!element) return;
		// Avoid the native two-row intrinsic height when measuring an empty editor.
		element.style.height = "0px";
		// scrollHeight includes padding but not borders; CSS owns the min/max bounds.
		element.style.height = `${element.scrollHeight + element.offsetHeight - element.clientHeight}px`;
	};
	createEffect(() => {
		if (!local.autoGrow || !element) return;
		const node = element;
		node.addEventListener("input", resize);
		let width = node.getBoundingClientRect().width;
		const observer = new ResizeObserver(() => {
			const next = node.getBoundingClientRect().width;
			// Height changes are ours; only width changes require a wrapping recalculation.
			if (next !== width) {
				width = next;
				resize();
			}
		});
		observer.observe(node);
		onCleanup(() => {
			node.removeEventListener("input", resize);
			observer.disconnect();
			node.style.removeProperty("height");
		});
	});
	createEffect(() => {
		void props.value;
		if (local.autoGrow) resize();
	});
	return (
		<textarea
			{...rest}
			ref={(node) => {
				element = node;
				if (typeof local.ref === "function") local.ref(node);
			}}
			data-form-control
			class={cx(s.control, s.textarea, local.autoGrow && s.textareaAutoGrow, local.class)}
			classList={local.classList}
		/>
	);
}
