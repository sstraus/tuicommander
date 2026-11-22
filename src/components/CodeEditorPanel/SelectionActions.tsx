import type { EditorView } from "@codemirror/view";
import { type Component, createEffect, createSignal, onCleanup, Show } from "solid-js";
import { t } from "../../i18n";
import { ideasStore } from "../../stores/ideas";
import { terminalsStore } from "../../stores/terminals";
import { toastsStore } from "../../stores/toasts";
import { sendTextToActiveTerminal } from "../../utils/sendToActiveTerminal";
import { Button } from "../ui";
import s from "./CodeEditorTab.module.css";

/**
 * Floating actions for a selected block: add it to the Ideas panel or send it
 * to the active agent terminal. Shown after the user finishes a selection
 * (mouse or keyboard), positioned below its end inside the editor area.
 */
export const SelectionActions: Component<{ view: EditorView | undefined }> = (props) => {
	const [pos, setPos] = createSignal<{ top: number; left: number } | null>(null);

	const selectedText = (): string => {
		const view = props.view;
		if (!view) return "";
		const { from, to } = view.state.selection.main;
		return view.state.sliceDoc(from, to).trim();
	};

	const refresh = () => {
		const view = props.view;
		if (!view || !selectedText()) {
			setPos(null);
			return;
		}
		const coords = view.coordsAtPos(view.state.selection.main.to);
		const area = view.dom.closest(`.${s.editorArea}`)?.getBoundingClientRect();
		if (!coords || !area) {
			setPos(null);
			return;
		}
		setPos({ top: coords.bottom - area.top + 4, left: Math.max(4, coords.left - area.left) });
	};

	createEffect(() => {
		const view = props.view;
		if (!view) return;
		const dom = view.dom;
		const onUp = () => requestAnimationFrame(refresh);
		const hide = () => setPos(null);
		dom.addEventListener("mouseup", onUp);
		dom.addEventListener("keyup", onUp);
		view.scrollDOM.addEventListener("scroll", hide);
		onCleanup(() => {
			dom.removeEventListener("mouseup", onUp);
			dom.removeEventListener("keyup", onUp);
			view.scrollDOM.removeEventListener("scroll", hide);
		});
	});

	const done = () => {
		setPos(null);
		props.view?.focus();
	};

	const addToIdeas = () => {
		ideasStore.addIdea(selectedText());
		toastsStore.add(t("scratchpad.addedToIdeas", "Added to Ideas"));
		done();
	};

	const sendToAgent = () => {
		void sendTextToActiveTerminal(selectedText());
		setPos(null);
	};

	const target = () => terminalsStore.getActive();

	return (
		<Show when={pos()}>
			{(p) => (
				<div
					class={s.selectionActions}
					style={{ top: `${p().top}px`, left: `${p().left}px` }}
					onMouseDown={(ev) => ev.preventDefault()}
				>
					<Button size="compact" variant="ghost" onClick={addToIdeas}>
						{t("scratchpad.addToIdeas", "Add to Ideas")}
					</Button>
					<Button
						size="compact"
						variant="ghost"
						disabled={!target()?.sessionId}
						title={target()?.name}
						onClick={sendToAgent}
					>
						{t("scratchpad.sendToAgent", "Send to agent")}
					</Button>
				</div>
			)}
		</Show>
	);
};
