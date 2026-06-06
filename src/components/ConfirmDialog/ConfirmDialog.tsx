import { type Component, createEffect, createSignal, onCleanup, Show } from "solid-js";
import d from "../shared/dialog.module.css";

export interface ConfirmDialogProps {
	visible: boolean;
	title: string;
	message: string;
	confirmLabel?: string;
	cancelLabel?: string;
	kind?: "warning" | "info" | "error";
	/** When set, auto-clicks the cancel button after this many ms, showing a countdown on its label. */
	autoCancelMs?: number;
	onClose: () => void;
	onConfirm: () => void;
}

/**
 * In-app confirmation dialog — replaces native Tauri ask() dialogs
 * which render as ugly light-mode macOS system sheets.
 * Uses shared dialog CSS module for consistent dark-theme styling.
 */
export const ConfirmDialog: Component<ConfirmDialogProps> = (props) => {
	// Countdown until auto-cancel — null when no auto-cancel is configured.
	const [remaining, setRemaining] = createSignal<number | null>(null);

	createEffect(() => {
		if (!props.visible || !props.autoCancelMs) {
			setRemaining(null);
			return;
		}
		let left = Math.ceil(props.autoCancelMs / 1000);
		setRemaining(left);
		const interval = setInterval(() => {
			left -= 1;
			setRemaining(left);
			if (left <= 0) {
				clearInterval(interval);
				props.onClose();
			}
		}, 1000);
		onCleanup(() => clearInterval(interval));
	});

	createEffect(() => {
		if (!props.visible) return;

		const handleKeydown = (e: KeyboardEvent) => {
			if (e.key === "Escape") {
				e.preventDefault();
				props.onClose();
			} else if (e.key === "Enter") {
				e.preventDefault();
				props.onConfirm();
			}
		};

		document.addEventListener("keydown", handleKeydown);
		onCleanup(() => document.removeEventListener("keydown", handleKeydown));
	});

	return (
		<Show when={props.visible}>
			<div class={d.overlay} onClick={props.onClose}>
				<div class={d.popover} onClick={(e) => e.stopPropagation()}>
					<div class={d.header}>
						<h4>{props.title}</h4>
					</div>
					<div class={d.body}>
						<p
							style={{
								margin: 0,
								"white-space": "pre-line",
								color: "var(--fg-secondary)",
								"font-size": "var(--font-md)",
							}}
						>
							{props.message}
						</p>
					</div>
					<div class={d.actions}>
						<button class={d.cancelBtn} onClick={props.onClose}>
							{props.cancelLabel ?? "Cancel"}
							{remaining() !== null ? ` (${remaining()})` : ""}
						</button>
						<button class={d.primaryBtn} onClick={props.onConfirm}>
							{props.confirmLabel ?? "OK"}
						</button>
					</div>
				</div>
			</div>
		</Show>
	);
};

export default ConfirmDialog;
