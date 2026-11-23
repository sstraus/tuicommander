import { createEffect, createSignal, type JSX, on, Show } from "solid-js";
import { ChevronIcon } from "./ChevronIcon";
import s from "./PanelSection.module.css";

export interface PanelSectionProps {
	title: string;
	/** Short state shown in the header while the section is collapsed or open ("5 ✗ 9 ✓"). */
	summary?: JSX.Element;
	/** Opens the section while the reader has not toggled it (ui-v3 "dynamic content"). */
	autoOpen?: boolean;
	/** Controls placed at the right of the header; clicks on them do not toggle the section. */
	actions?: JSX.Element;
	/** Called each time the section becomes open, for lazy loading. */
	onOpen?: () => void;
	children: JSX.Element;
}

/** UI v3 accordion section (docs/frontend/ui-v3.md). The reader's choice wins over `autoOpen`. */
export function PanelSection(props: PanelSectionProps) {
	const [chosen, setChosen] = createSignal<boolean | null>(null);
	const open = () => chosen() ?? props.autoOpen ?? false;
	const toggle = () => setChosen(!open());
	createEffect(on(open, (isOpen) => isOpen && props.onOpen?.()));
	return (
		<section class={s.section} data-open={open() ? "" : undefined}>
			<div class={s.header}>
				<button type="button" class={s.toggle} data-control-layout="row" aria-expanded={open()} onClick={toggle}>
					<span class={s.chevron} data-open={open() ? "" : undefined}>
						<ChevronIcon />
					</span>
					<span class={s.title}>{props.title}</span>
					<Show when={props.summary}>
						<span class={s.summary}>{props.summary}</span>
					</Show>
				</button>
				<Show when={props.actions}>
					<span class={s.actions}>{props.actions}</span>
				</Show>
			</div>
			<Show when={open()}>
				<div class={s.body}>{props.children}</div>
			</Show>
		</section>
	);
}
