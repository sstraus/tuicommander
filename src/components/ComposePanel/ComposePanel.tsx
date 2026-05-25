import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { drawSelection, EditorView, keymap } from "@codemirror/view";
import { createCodeMirror } from "solid-codemirror";
import { type Accessor, type Component, createEffect, on, onCleanup } from "solid-js";
import { cx } from "../../utils";
import s from "./ComposePanel.module.css";

const composeTheme = EditorView.theme(
	{
		"&": {
			width: "100%",
			height: "100%",
			fontSize: "13px",
			background: "var(--bg-primary)",
		},
		".cm-scroller": {
			fontFamily: "var(--font-mono)",
			overflow: "auto",
		},
		".cm-content": {
			caretColor: "var(--accent)",
			padding: "8px 12px",
		},
		".cm-cursor, .cm-dropCursor": {
			borderLeftColor: "var(--accent)",
		},
		"&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection": {
			backgroundColor: "rgba(122, 162, 247, 0.2)",
		},
		".cm-activeLine": {
			backgroundColor: "transparent",
		},
		"&.cm-focused": {
			outline: "none",
		},
	},
	{ dark: true },
);

export interface ComposePanelProps {
	isOpen: Accessor<boolean>;
	initialText: Accessor<string>;
	onClose: () => void;
	onSend: (text: string) => void | Promise<void>;
	onTextChange?: (text: string) => void;
}

export const ComposePanel: Component<ComposePanelProps> = (props) => {
	const { ref, editorView, createExtension } = createCodeMirror({
		onValueChange: (value) => props.onTextChange?.(value),
	});

	createExtension(composeTheme);
	createExtension(drawSelection());
	createExtension(history());
	createExtension(EditorView.lineWrapping);
	createExtension(
		keymap.of([
			{
				key: "Ctrl-Enter",
				run: (view) => {
					const text = view.state.doc.toString().trim();
					if (text) props.onSend(text);
					return true;
				},
			},
			{
				key: "Escape",
				run: () => {
					props.onClose();
					return true;
				},
			},
			...defaultKeymap,
			...historyKeymap,
		]),
	);

	// Re-initialise content only when the panel opens — NOT on every keystroke.
	// Tracking initialText() here would re-run on every user keystroke (since
	// onTextChange feeds back into the same signal), causing a 2-RAF delay loop
	// that overwrites the current content with content from ~32ms ago (ghost text).
	createEffect(
		on(props.isOpen, (open) => {
			if (!open) return;
			// Read initialText outside reactive tracking — we only want the value
			// at open time, not to subscribe to further changes while typing.
			const initial = props.initialText();
			requestAnimationFrame(() =>
				requestAnimationFrame(() => {
					const view = editorView();
					if (!view) return;
					const current = view.state.doc.toString();
					if (current !== initial) {
						view.dispatch({
							changes: { from: 0, to: view.state.doc.length, insert: initial },
							selection: { anchor: initial.length },
						});
					}
					view.focus();
				}),
			);
		}),
	);

	createEffect(() => {
		if (!props.isOpen()) return;
		const handleFocusOut = (e: FocusEvent) => {
			const related = e.relatedTarget as Node | null;
			const panel = editorView()?.dom?.closest(`.${s.panel}`);
			if (related && panel?.contains(related)) return;
			requestAnimationFrame(() => {
				if (props.isOpen()) editorView()?.focus();
			});
		};
		const cmDom = editorView()?.dom;
		cmDom?.addEventListener("focusout", handleFocusOut);
		onCleanup(() => cmDom?.removeEventListener("focusout", handleFocusOut));
	});

	const handleSend = () => {
		const view = editorView();
		if (!view) return;
		const text = view.state.doc.toString().trim();
		if (text) props.onSend(text);
	};

	return (
		<div class={cx(s.panel, props.isOpen() && s.panelOpen)} onMouseDown={(e) => e.stopPropagation()}>
			<div class={s.editor} ref={ref} />
			<div class={s.statusBar}>
				<span>Ctrl+Enter to send &middot; Esc to close</span>
				<button class={s.sendButton} onClick={handleSend} title="Send (Ctrl+Enter)">
					<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
						<path d="M4 2l10 6-10 6V2z" />
					</svg>
				</button>
			</div>
		</div>
	);
};
