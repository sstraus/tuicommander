import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { drawSelection, EditorView, keymap } from "@codemirror/view";
import { createCodeMirror } from "solid-codemirror";
import { type Accessor, type Component, createEffect, createSignal, For, on, onCleanup, Show } from "solid-js";
import type { QueuedCommand } from "../../hooks/usePty";
import { cx } from "../../utils";
import { IMAGE_FILE_ACCEPT } from "../../utils/pastedImage";
import { ComposePinIcon, ComposeSendIcon } from "../shared/ComposeActionIcons";
import { Button } from "../ui/Button";
import s from "./ComposePanel.module.css";
import { useComposeImages } from "./useComposeImages";

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
	/** Hand the text to the backend's idle gate instead of typing it now. */
	onEnqueue: (text: string) => void | Promise<void>;
	/** False for a plain shell: only an agent has a composer worth waiting for. */
	canEnqueue: Accessor<boolean>;
	/** Commands already waiting for this session's next idle window. */
	queuedCount: Accessor<number>;
	onClearQueue: () => void | Promise<void>;
	/** Read the queued commands themselves — only called while the list is open. */
	onLoadQueue: () => Promise<QueuedCommand[]>;
	/** Drop a single queued command by id. */
	onRemoveQueued: (id: number) => void | Promise<void>;
	onTextChange?: (text: string) => void;
	/** Host applies successful-submit cleanup to its shared draft and current editor. */
	onSubmitted?: (remainingDraft: string) => void;
	/** Pinned: the panel stays open after send and Esc, and takes its own slot
	 *  under the terminal instead of overlaying it. */
	pinned: Accessor<boolean>;
	/** The host requires this input: hide controls that would close or undock it. */
	persistent?: Accessor<boolean>;
	onTogglePin: () => void;
	/** The close button: closes the panel even when pinned, unlike Esc. */
	onDismiss: () => void;
	/** Bumped to move the caret into the editor while the panel stays open. */
	focusRequest: Accessor<number>;
	/** Explicit host replacement (for example a smart prompt), independent of typing. */
	textRequest?: Accessor<number>;
}

/** Who parked a queue entry, for the entries that are not the operator's own. */
const QUEUE_KIND_LABEL: Record<QueuedCommand["kind"], string> = {
	user_command: "",
	notice: "TUIC notice",
	initial_prompt: "child's initial prompt",
};

const queueItemLabel = (item: QueuedCommand) =>
	item.kind === "user_command" ? item.text : `${QUEUE_KIND_LABEL[item.kind]} — ${item.text}`;

const queueItemTitle = (item: QueuedCommand) =>
	item.kind === "user_command"
		? item.text
		: `${QUEUE_KIND_LABEL[item.kind]} (parked by TUICommander; removing it only skips the notice)\n\n${item.text}`;

export const ComposePanel: Component<ComposePanelProps> = (props) => {
	const { ref, editorView, createExtension } = createCodeMirror({
		onValueChange: (value) => props.onTextChange?.(value),
	});

	/** An unpinned panel is closed by the parent after a successful submit; a
	 *  pinned one stays. Successful cleanup also reaches the host draft when
	 *  this editor has unmounted. A
	 *  rejected submit keeps the text — the parent has already logged the error.
	 *  A pinned panel stays editable while the send runs, so only the submitted
	 *  document is removed: whatever the user typed after it is the next message.
	 *  A submit while another is in flight is dropped, so no text goes out twice. */
	let disposed = false;
	onCleanup(() => {
		disposed = true;
	});
	let inFlight = false;
	const submit = (handler: (text: string) => void | Promise<void>, text: string) => {
		if (inFlight) return;
		inFlight = true;
		const submitted = editorView()?.state.doc.toString() ?? "";
		// The executor calls the handler synchronously and turns a throw into a
		// rejection, so the flag is always released.
		new Promise<void>((resolve) => resolve(handler(text)))
			.then(
				() => {
					const view = editorView();
					// The host's shared draft outlives this editor, including a switch
					// away and back while the request is pending.
					const doc = !disposed && view ? view.state.doc.toString() : props.initialText();
					// Editing the submitted text itself leaves no safe prefix to remove.
					const remaining = doc.startsWith(submitted) ? doc.slice(submitted.length).trimStart() : doc;
					if (props.onSubmitted) {
						props.onSubmitted(remaining);
						return;
					}
					if (disposed || !props.pinned() || !view) return;
					if (doc !== remaining) {
						view.dispatch({ changes: { from: 0, to: doc.length - remaining.length, insert: "" } });
					}
					view.focus();
				},
				() => {},
			)
			.finally(() => {
				inFlight = false;
			});
	};

	createExtension(composeTheme);
	createExtension(drawSelection());
	createExtension(history());
	createExtension(EditorView.lineWrapping);
	const images = useComposeImages(editorView, props.isOpen);
	createExtension(images.extension);
	let fileInput: HTMLInputElement | undefined;
	createExtension(
		keymap.of([
			// Shift first: CodeMirror matches the more specific binding, so plain
			// Ctrl-Enter keeps sending immediately.
			{
				key: "Shift-Ctrl-Enter",
				run: (view) => {
					const text = view.state.doc.toString().trim();
					if (text && props.canEnqueue()) submit(props.onEnqueue, text);
					return true;
				},
			},
			{
				key: "Ctrl-Enter",
				run: (view) => {
					const text = view.state.doc.toString().trim();
					if (text) submit(props.onSend, text);
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
			let inner = 0;
			const outer = requestAnimationFrame(() => {
				inner = requestAnimationFrame(() => {
					const view = editorView();
					if (!view) return;
					// Read the shared draft when initialization runs: a pending submit
					// may have cleaned it since opening. RAF reads are not tracked.
					const initial = props.initialText();
					const current = view.state.doc.toString();
					if (current !== initial) {
						view.dispatch({
							changes: { from: 0, to: view.state.doc.length, insert: initial },
							selection: { anchor: initial.length },
						});
					}
					view.focus();
				});
			});
			// Closing (or unmounting) before the second frame lands would otherwise
			// dispatch into a destroyed view.
			onCleanup(() => {
				cancelAnimationFrame(outer);
				if (inner) cancelAnimationFrame(inner);
			});
		}),
	);

	createEffect(
		on(
			props.focusRequest,
			() => {
				editorView()?.focus();
			},
			{ defer: true },
		),
	);

	createEffect(
		on(
			() => props.textRequest?.(),
			() => {
				const view = editorView();
				if (!view) return;
				const text = props.initialText();
				view.dispatch({
					changes: { from: 0, to: view.state.doc.length, insert: text },
					selection: { anchor: text.length },
				});
				view.focus();
			},
			{ defer: true },
		),
	);

	// Pull the caret back when it wanders off — but not while pinned: a pinned
	// panel sits beside a terminal the user must be able to click into.
	createEffect(() => {
		if (!props.isOpen() || props.pinned()) return;
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

	// The badge count comes from the cheap lifecycle poll; the texts cost an IPC
	// round-trip, so they are fetched only while the list is expanded — and
	// re-fetched whenever the count moves under it (the queue drains on idle).
	const [queueOpen, setQueueOpen] = createSignal(false);
	const [queueItems, setQueueItems] = createSignal<QueuedCommand[]>([]);

	createEffect(() => {
		if (!queueOpen()) return;
		const count = props.queuedCount();
		if (count === 0) {
			setQueueOpen(false);
			setQueueItems([]);
			return;
		}
		void props.onLoadQueue().then(setQueueItems);
	});

	createEffect(() => {
		if (!props.isOpen()) setQueueOpen(false);
	});

	const currentText = (): string => editorView()?.state.doc.toString().trim() ?? "";

	const handleSend = () => {
		const text = currentText();
		if (text) submit(props.onSend, text);
	};

	const handleEnqueue = () => {
		const text = currentText();
		if (text && props.canEnqueue()) submit(props.onEnqueue, text);
	};

	return (
		<div
			class={cx(s.panel, props.isOpen() && s.panelOpen, props.pinned() && s.panelPinned)}
			onMouseDown={(e) => e.stopPropagation()}
		>
			<div class={s.editor} data-compose-editor ref={ref} />
			<Show when={queueOpen() && props.queuedCount() > 0}>
				<div class={s.queueList}>
					<div class={s.queueListHeader}>
						<span>Waiting for the agent to go idle</span>
						<Button variant="ghost" class={s.queueClearAll} onClick={() => props.onClearQueue()}>
							Clear all
						</Button>
					</div>
					<For each={queueItems()}>
						{(item) => (
							<div class={s.queueItem}>
								<span class={s.queueItemText} title={queueItemTitle(item)}>
									{queueItemLabel(item)}
								</span>
								<Button
									variant="ghost"
									shape="icon"
									class={s.queueItemRemove}
									onClick={() => props.onRemoveQueued(item.id)}
									title="Remove from queue"
									aria-label="Remove from queue"
								>
									<svg width="10" height="10" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
										<path d="M4.3 3.3l8.4 8.4-1 1-8.4-8.4zM12.7 4.3l-8.4 8.4-1-1 8.4-8.4z" />
									</svg>
								</Button>
							</div>
						)}
					</For>
				</div>
			</Show>
			<Show when={images.error()}>
				<div class={s.imageError} role="alert">
					{images.error()}
				</div>
			</Show>
			<div class={s.statusBar}>
				{/* Same legend shape as the command palette footer. The bindings are
				    literal Control on every platform: CodeMirror's "Ctrl-" is not "Mod-". */}
				<div class={s.hints}>
					<span class={s.hint}>
						<kbd>ctrl ↵</kbd> send
					</span>
					<Show when={props.canEnqueue()}>
						<span class={s.hint}>
							<kbd>⇧ ctrl ↵</kbd> queue
						</span>
					</Show>
					<span class={s.hint}>
						<kbd>esc</kbd> {props.pinned() ? "terminal" : "close"}
					</span>
				</div>
				<div class={s.actions}>
					<Show when={props.queuedCount() > 0}>
						<Button
							variant="ghost"
							class={cx(s.queueBadge, queueOpen() && s.queueBadgeOpen)}
							onClick={() => setQueueOpen(!queueOpen())}
							title="Waiting for the agent to go idle — click to review"
							aria-expanded={queueOpen()}
						>
							{props.queuedCount()} queued
							<svg
								width="10"
								height="10"
								viewBox="0 0 16 16"
								fill="currentColor"
								aria-hidden="true"
								class={cx(s.queueChevron, queueOpen() && s.queueChevronOpen)}
							>
								<path d="M8 11L3 5h10z" />
							</svg>
						</Button>
					</Show>
					<Show when={!props.persistent?.()}>
						<Button
							variant="ghost"
							onClick={() => props.onTogglePin()}
							shape="icon"
							aria-label={props.pinned() ? "Unpin from the terminal bottom" : "Pin to the terminal bottom"}
							title={props.pinned() ? "Unpin from the terminal bottom" : "Pin to the terminal bottom"}
							aria-pressed={props.pinned()}
						>
							<ComposePinIcon />
						</Button>
						<span class={s.divider} aria-hidden="true" />
					</Show>
					<Show when={props.canEnqueue()}>
						<Button
							variant="ghost"
							shape="icon"
							aria-label="Queue for the next idle moment (Shift+Ctrl+Enter)"
							class={s.queueButton}
							onClick={handleEnqueue}
							title="Queue for the next idle moment (Shift+Ctrl+Enter)"
						>
							<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
								<path d="M2 3h12v2H2zM2 7h12v2H2zM2 11h8v2H2z" />
							</svg>
						</Button>
					</Show>
					<input
						ref={fileInput}
						type="file"
						accept={IMAGE_FILE_ACCEPT}
						multiple
						hidden
						aria-label="Choose images"
						onChange={(event) => {
							images.pick(Array.from(event.currentTarget.files ?? []));
							event.currentTarget.value = "";
						}}
					/>
					<Button
						variant="ghost"
						type="button"
						onClick={() => fileInput?.click()}
						shape="icon"
						title="Attach images"
						aria-label="Attach images"
					>
						<svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
							<path d="M16.5 3a4.5 4.5 0 0 1 3.18 7.68l-8.49 8.49a6 6 0 0 1-8.48-8.48l8.49-8.49 1.41 1.41-8.49 8.49a4 4 0 0 0 5.66 5.66l8.49-8.49a2.5 2.5 0 0 0-3.54-3.54l-8.49 8.49a1 1 0 0 0 1.42 1.42l7.78-7.78 1.41 1.41-7.78 7.78a3 3 0 0 1-4.24-4.24l8.49-8.49A4.49 4.49 0 0 1 16.5 3z" />
						</svg>
					</Button>
					<Button
						shape="icon"
						variant="primary"
						onClick={handleSend}
						title="Send (Ctrl+Enter)"
						aria-label="Send (Ctrl+Enter)"
					>
						<ComposeSendIcon />
					</Button>
					<Show when={!props.persistent?.()}>
						<span class={s.divider} aria-hidden="true" />
						<Button
							variant="ghost"
							shape="icon"
							class={s.closeButton}
							onClick={() => props.onDismiss()}
							title="Close compose panel"
							aria-label="Close compose panel"
						>
							<svg width="10" height="10" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
								<path d="M4.3 3.3l8.4 8.4-1 1-8.4-8.4zM12.7 4.3l-8.4 8.4-1-1 8.4-8.4z" />
							</svg>
						</Button>
					</Show>
				</div>
			</div>
		</div>
	);
};
