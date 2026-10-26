import { type Component, createEffect, createSignal, onCleanup, Show } from "solid-js";
import { t } from "../../i18n";
import { registerModal } from "../../stores/modalStack";
import d from "../shared/dialog.module.css";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";

export interface PromptDialogProps {
	visible: boolean;
	title: string;
	subtitle?: string;
	placeholder?: string;
	defaultValue?: string;
	confirmLabel?: string;
	maxLength?: number;
	onClose: () => void;
	onConfirm: (value: string) => void;
}

/**
 * Generic text input dialog — replaces window.prompt() which doesn't work
 * in Tauri's webview. Uses shared dialog CSS module for consistent styling.
 */
export const PromptDialog: Component<PromptDialogProps> = (props) => {
	const [value, setValue] = createSignal("");
	let inputRef: HTMLInputElement | undefined;

	createEffect(() => {
		if (props.visible) {
			setValue(props.defaultValue ?? "");
			setTimeout(() => {
				if (inputRef) {
					inputRef.focus();
					inputRef.select();
				}
			}, 0);
		}
	});

	createEffect(() => {
		if (!props.visible) return;

		// Escape-to-close is handled centrally (stores/modalStack): registering routes
		// Escape to props.onClose AND stops it reaching the terminal underneath.
		registerModal(props.onClose);

		const handleKeydown = (e: KeyboardEvent) => {
			if (e.key === "Enter" && value().trim()) {
				e.preventDefault();
				props.onConfirm(value().trim());
				props.onClose();
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
						<div class={d.headerText}>
							<h4>{props.title}</h4>
							<Show when={props.subtitle}>
								<p class={d.subtitle}>{props.subtitle}</p>
							</Show>
						</div>
					</div>
					<div class={d.body}>
						<Input
							ref={inputRef}
							type="text"
							value={value()}
							onInput={(e) => setValue((e.target as HTMLInputElement).value)}
							placeholder={props.placeholder ?? ""}
							maxLength={props.maxLength}
							autocomplete="off"
							autocorrect="off"
							spellcheck={false}
						/>
					</div>
					<div class={d.actions}>
						<Button class={d.cancelBtn} onClick={props.onClose}>
							{t("promptDialog.cancel", "Cancel")}
						</Button>
						<Button
							variant="primary"
							class={d.primaryBtn}
							onClick={() => {
								if (value().trim()) {
									props.onConfirm(value().trim());
									props.onClose();
								}
							}}
							disabled={!value().trim()}
						>
							{props.confirmLabel ?? t("promptDialog.ok", "OK")}
						</Button>
					</div>
				</div>
			</div>
		</Show>
	);
};

export default PromptDialog;
