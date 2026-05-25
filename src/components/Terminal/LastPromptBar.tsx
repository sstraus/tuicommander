import { type Accessor, type Component, createSignal } from "solid-js";
import { cx } from "../../utils";
import s from "./LastPromptBar.module.css";

export interface LastPromptBarProps {
	prompt: Accessor<string | null>;
}

const Chevron = () => (
	<svg width="10" height="10" viewBox="0 0 10 10" fill="currentColor">
		<path
			d="M2 3.5l3 3 3-3"
			fill="none"
			stroke="currentColor"
			stroke-width="1.3"
			stroke-linecap="round"
			stroke-linejoin="round"
		/>
	</svg>
);

export const LastPromptBar: Component<LastPromptBarProps> = (props) => {
	const [expanded, setExpanded] = createSignal(false);

	const toggle = (e: MouseEvent) => {
		e.stopPropagation();
		setExpanded((v) => !v);
	};

	return (
		<div
			class={cx(s.bar, expanded() ? s.expanded : s.collapsed)}
			onClick={toggle}
			title={expanded() ? "Click to collapse" : "Click to expand"}
		>
			<div class={s.header}>
				<span class={s.label}>Prompt</span>
				<span class={s.preview}>{expanded() ? "" : props.prompt()}</span>
				<span class={cx(s.chevron, expanded() && s.chevronUp)}>
					<Chevron />
				</span>
			</div>
			{expanded() && <div class={s.body}>{props.prompt()}</div>}
		</div>
	);
};
