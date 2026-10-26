import { type Accessor, createEffect, onCleanup, Show } from "solid-js";
import { t } from "../../i18n";
import { closeDialogStore } from "../../stores/closeDialog";
import { Button } from "../ui/Button";
import qd from "./QuitDialog.module.css";

/** Capture before terminal shortcuts; Enter keeps the safe Cancel default. */
export function useCloseDialogKeyCapture(visible: Accessor<boolean>, cancel: () => void): void {
	createEffect(() => {
		if (!visible()) return;
		const handler = (event: KeyboardEvent) => {
			if (event.key !== "Escape" && event.key !== "Enter") return;
			event.preventDefault();
			event.stopPropagation();
			event.stopImmediatePropagation();
			cancel();
		};
		document.addEventListener("keydown", handler, true);
		onCleanup(() => document.removeEventListener("keydown", handler, true));
	});
}

export function CloseDialog() {
	const state = closeDialogStore.state;
	const cancel = () => void closeDialogStore.answer("cancel");
	useCloseDialogKeyCapture(() => state.request !== null, cancel);
	const ask = () => state.request?.kind === "ask";
	return (
		<Show when={state.request}>
			<div class={qd.overlay} onClick={cancel}>
				<div
					class={qd.dialog}
					role="dialog"
					aria-modal="true"
					aria-labelledby="close-dialog-title"
					onClick={(event) => event.stopPropagation()}
				>
					<h3 id="close-dialog-title">
						{ask()
							? t("closeDialog.title.ask", "Close TUICommander?")
							: t("closeDialog.title.quit", "Quit TUICommander?")}
					</h3>
					<p>{t("closeDialog.message", "Background keeps sessions running. Quit closes all sessions.")}</p>
					<p>
						{t("closeDialog.sessions", "Active terminal sessions: {count}", {
							count: String(state.request?.sessions ?? 0),
						})}
					</p>
					<Show when={ask()}>
						<label class={qd.remember}>
							<input
								type="checkbox"
								checked={state.remember}
								disabled={state.saving}
								onChange={(event) => closeDialogStore.setRemember(event.currentTarget.checked)}
							/>
							{t("closeDialog.remember", "Remember my choice")}
						</label>
					</Show>
					<Show when={state.error}>
						<p role="alert">{state.error}</p>
					</Show>
					<div class={qd.actions}>
						<Button disabled={state.saving} onClick={cancel}>
							{t("closeDialog.cancel", "Cancel")}
						</Button>
						<Show when={ask() && state.request?.background_available !== false}>
							<Button disabled={state.saving} onClick={() => void closeDialogStore.answer("background")}>
								{t("closeDialog.background", "Background")}
							</Button>
						</Show>
						<Button variant="danger" disabled={state.saving} onClick={() => void closeDialogStore.answer("quit")}>
							{t("closeDialog.quit", "Quit")}
						</Button>
					</div>
				</div>
			</div>
		</Show>
	);
}
