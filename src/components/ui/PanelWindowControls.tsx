import { type Component, Show } from "solid-js";
import { closePanel, detachPanel, reattachPanel } from "../../panelRouter";
import { isTauri } from "../../transport";
import s from "./PanelWindowControls.module.css";

export const IconDetach = () => (
	<svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.3">
		<path
			d="M8 2h4v4M8 6l4-4M6 3H3a1 1 0 00-1 1v7a1 1 0 001 1h7a1 1 0 001-1V8"
			stroke-linecap="round"
			stroke-linejoin="round"
		/>
	</svg>
);

export const IconReattach = () => (
	<svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.3">
		<path
			d="M6 12H3a1 1 0 01-1-1V4a1 1 0 011-1h7a1 1 0 011 1v3M10 8l-4 4M10 12V8H6"
			stroke-linecap="round"
			stroke-linejoin="round"
		/>
	</svg>
);

// SVG close (not the `&times;` glyph) so it shares the exact 14×14 geometry and
// optical center of the detach/reattach icons — a text glyph sits a hair higher
// and looks misaligned next to them.
export const IconClose = () => (
	<svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.3">
		<path d="M3.5 3.5l7 7M10.5 3.5l-7 7" stroke-linecap="round" />
	</svg>
);

interface PanelWindowControlsProps {
	panelId: string;
	mode: "inline" | "detached";
	onInlineClose?: () => void;
}

export const PanelWindowControls: Component<PanelWindowControlsProps> = (props) => {
	return (
		<div class={s.controls}>
			<Show when={props.mode === "inline" && isTauri()}>
				<button class={s.btn} onClick={() => detachPanel(props.panelId)} title="Open in separate window">
					<IconDetach />
				</button>
			</Show>
			<Show when={props.mode === "detached"}>
				<button class={s.btn} onClick={() => reattachPanel(props.panelId)} title="Bring back to main window">
					<IconReattach />
				</button>
			</Show>
			<button
				class={s.btn}
				onClick={() => (props.mode === "detached" ? closePanel(props.panelId) : props.onInlineClose?.())}
				title="Close"
			>
				<IconClose />
			</button>
		</div>
	);
};
