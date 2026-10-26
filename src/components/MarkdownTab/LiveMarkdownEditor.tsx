import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { search, searchKeymap } from "@codemirror/search";
import { EditorState } from "@codemirror/state";
import { drawSelection, EditorView, keymap } from "@codemirror/view";
import { type Component, createEffect, createSignal, on, onCleanup, onMount, Show } from "solid-js";
import { t } from "../../i18n";
import { appLogger } from "../../stores/appLogger";
import { toastsStore } from "../../stores/toasts";
import { generateTweakCommentId, OverlappingCommentError, toggleCheckbox } from "../../utils/tweakComments";
import { codeEditorTheme } from "../CodeEditorPanel/theme";
import { Button } from "../ui/Button";
import { ContentRenderer } from "../ui/ContentRenderer";
import { Input } from "../ui/Input";
import s from "./LiveMarkdownEditor.module.css";
import { addTweakCommentAtSelection, liveLineSeparator, liveMarkdown, loadMarkdownLanguage } from "./liveMarkdown";

export interface LiveMarkdownEditorProps {
	/** Document as stored on disk. The editor saves exactly what is typed, nothing normalised. */
	content: string;
	onSave: (text: string) => Promise<boolean>;
	/** Current on-disk text; a save is refused when it differs from what this editor loaded. */
	readDisk: () => Promise<string>;
	onDirtyChange?: (dirty: boolean) => void;
	baseDir?: string;
	onLinkClick?: (href: string) => void;
	fontSize?: number;
}

const BLOCK_SELECTOR = "[data-comment-source-start]";
/** Frames to wait for the renderer to stamp source ranges on a fresh render. */
const RENDER_WAIT_FRAMES = 30;

interface ActiveBlock {
	start: number;
	end: number;
	view: EditorView;
	/** Wrapper standing where the rendered block was; re-attached after every re-render. */
	host: HTMLElement;
	tag: string;
	rendered?: HTMLElement;
}

/**
 * The preview renderer for every block but one. Clicking a block swaps it for a source editor over
 * exactly that block's bytes; the text between blocks is never touched, so a save is lossless.
 */
export const LiveMarkdownEditor: Component<LiveMarkdownEditorProps> = (props) => {
	let container: HTMLElement | undefined;
	let baseline = props.content;
	/** Document as rendered. While a block is open it is the text from before the block was opened. */
	const [text, setText] = createSignal(props.content);
	let active: ActiveBlock | undefined;
	const [hasSelection, setHasSelection] = createSignal(false);
	const [composing, setComposing] = createSignal(false);
	const [draft, setDraft] = createSignal("");
	let commentInput: HTMLInputElement | undefined;
	// The toolbar button never takes focus (mousedown is prevented to keep the selection), so hand it to the input.
	createEffect(on(composing, (open) => open && commentInput?.focus(), { defer: true }));

	const [dirty, setDirty] = createSignal(false);
	const markDirty = (value: boolean) => {
		setDirty(value);
		props.onDirtyChange?.(value);
	};

	/** The document as it stands: the rendered text with the open block's current source spliced in. */
	const full = () => {
		const doc = text();
		return active ? doc.slice(0, active.start) + active.view.state.sliceDoc() + doc.slice(active.end) : doc;
	};
	const refreshDirty = () => markDirty(full() !== baseline);

	/** Disk text that differs from `baseline`: someone else wrote the file while this buffer was open. */
	const [conflict, setConflict] = createSignal<string | null>(null);

	const commit = async () => {
		const doc = full();
		if (!(await props.onSave(doc))) return;
		baseline = doc;
		setConflict(null);
		markDirty(false);
	};

	const save = async () => {
		let disk: string;
		try {
			disk = await props.readDisk();
		} catch (err) {
			appLogger.error("app", "live save: cannot read the file to check for external changes", err);
			toastsStore.add(t("markdownTab.liveReadFailed", "Couldn't save"), String(err), "error");
			return;
		}
		if (disk !== baseline) {
			setConflict(disk);
			return;
		}
		await commit();
	};

	/** Close the open block, keeping its edits in the document. */
	const closeBlock = (): { changed: boolean; delta: number } => {
		if (!active) return { changed: false, delta: 0 };
		const doc = full();
		const changed = doc !== text();
		const delta = doc.length - text().length;
		const { view, host, rendered } = active;
		active = undefined;
		setHasSelection(false);
		view.destroy();
		host.remove();
		if (rendered) rendered.style.display = "";
		setText(doc);
		return { changed, delta };
	};

	/** Replace the whole document (disk reload); the open block's range no longer means anything. */
	const resetTo = (doc: string) => {
		if (active) {
			active.view.destroy();
			active.host.remove();
			active = undefined;
			setHasSelection(false);
		}
		baseline = doc;
		setText(doc);
	};

	const reloadFromDisk = () => {
		const disk = conflict();
		if (disk === null) return;
		resetTo(disk);
		setConflict(null);
		markDirty(false);
	};

	const createState = (doc: string) =>
		EditorState.create({
			doc,
			extensions: [
				loadMarkdownLanguage(),
				liveLineSeparator(text()),
				liveMarkdown(),
				codeEditorTheme,
				history(),
				drawSelection(),
				search({ top: true }),
				EditorView.lineWrapping,
				keymap.of([
					{
						key: "Mod-s",
						run: () => {
							void save();
							return true;
						},
					},
					...searchKeymap,
					{
						key: "Escape",
						run: () => {
							closeBlock();
							return true;
						},
					},
					...historyKeymap,
					...defaultKeymap,
				]),
				EditorView.updateListener.of((update) => {
					if (update.selectionSet) setHasSelection(!update.state.selection.main.empty);
					if (update.docChanged) refreshDirty();
				}),
			],
		});

	const blockAt = (start: number): HTMLElement | null =>
		container?.querySelector<HTMLElement>(`[data-comment-source-start="${start}"]`) ?? null;

	/** Put the open block's editor where its rendered twin is, hiding the twin. */
	const place = () => {
		if (!active) return;
		const twin = blockAt(active.start);
		if (!twin || twin === active.rendered) return;
		active.rendered = twin;
		twin.style.display = "none";
		twin.after(active.host);
	};

	const openBlock = (twin: HTMLElement) => {
		const start = Number(twin.dataset.commentSourceStart);
		const end = Number(twin.dataset.commentSourceEnd);
		const doc = text().slice(start, end);
		const host = document.createElement(twin.tagName === "LI" ? "li" : "div");
		host.style.listStyle = "none";
		const view = new EditorView({ state: createState(doc), parent: host });
		view.dom.classList.add(s.block);
		active = { start, end, view, host, tag: twin.tagName };
		place();
		view.focus();
		view.dispatch({ selection: { anchor: view.state.doc.length } });
	};

	/** After a re-render the stamped ranges arrive a frame later; wait for the block at `start`. */
	const openBlockAt = (start: number, frames = RENDER_WAIT_FRAMES) => {
		const twin = blockAt(start);
		if (twin) openBlock(twin);
		else if (frames > 0) requestAnimationFrame(() => openBlockAt(start, frames - 1));
	};

	const handleClick = (ev: MouseEvent) => {
		const target = ev.target as HTMLElement;
		if (active?.host.contains(target)) return;
		if (target.closest("a, input, button")) return;
		const twin = target.closest<HTMLElement>(BLOCK_SELECTOR);
		if (!twin || !container?.contains(twin)) {
			closeBlock();
			return;
		}
		const start = Number(twin.dataset.commentSourceStart);
		if (!active) {
			openBlock(twin);
			return;
		}
		const closed = active;
		const { changed, delta } = closeBlock();
		// Unchanged text means no re-render: the clicked block is still in the DOM.
		if (!changed) openBlock(twin);
		else openBlockAt(start >= closed.end ? start + delta : start);
	};

	onMount(() => {
		if (!container) return;
		// A re-render rebuilds the rendered DOM and drops the swapped-in editor with it.
		const observer = new MutationObserver(() => {
			// The twin is gone when a re-render rebuilt the DOM or when mermaid swapped its <pre> for the diagram.
			if (active && (!active.host.isConnected || !active.rendered?.isConnected)) {
				active.rendered = undefined;
				const wait = (frames: number) => {
					place();
					if (active && !active.host.isConnected && frames > 0) requestAnimationFrame(() => wait(frames - 1));
				};
				wait(RENDER_WAIT_FRAMES);
			}
		});
		observer.observe(container, { childList: true, subtree: true });
		onCleanup(() => {
			observer.disconnect();
			active?.view.destroy();
			active = undefined;
		});
	});

	// A disk change lands in a clean buffer; a dirty buffer keeps the user's text and the save guard handles it.
	createEffect(
		on(
			() => props.content,
			(incoming) => {
				if (incoming === baseline || full() !== baseline) return;
				resetTo(incoming);
			},
			{ defer: true },
		),
	);

	const toggleMark = (sourceLine: number, mark: " " | "x" | "~", sourceCol?: number) => {
		// A one-character rewrite keeps every block range valid, the open block's included.
		setText(toggleCheckbox(text(), sourceLine, mark, sourceCol));
		refreshDirty();
	};

	const cancelComment = () => {
		setDraft("");
		setComposing(false);
		active?.view.focus();
	};

	const addComment = () => {
		const body = draft().trim();
		const view = active?.view;
		if (!view || !body) return;
		try {
			addTweakCommentAtSelection(view, {
				id: generateTweakCommentId(),
				comment: body,
				createdAt: new Date().toISOString(),
			});
			setDraft("");
			setComposing(false);
			view.focus();
		} catch (err) {
			appLogger.error("app", "live addTweakComment failed", err);
			toastsStore.add(
				t("markdownTab.commentOverlap", "Couldn't add comment"),
				err instanceof OverlappingCommentError
					? t(
							"markdownTab.commentOverlapMsg",
							"That text already has a comment. Click the highlight to edit it, or select different text.",
						)
					: String(err),
				"error",
			);
		}
	};

	return (
		<div class={s.root}>
			<div class={s.toolbar}>
				<Show when={hasSelection() && !composing()}>
					<Button type="button" onMouseDown={(ev) => ev.preventDefault()} onClick={() => setComposing(true)}>
						{t("markdownTab.addComment", "Comment")}
					</Button>
				</Show>
				<Show when={composing()}>
					<Input
						class={s.input}
						ref={commentInput}
						value={draft()}
						placeholder={t("markdownTab.commentPlaceholder", "Comment on the selection")}
						onInput={(ev) => setDraft(ev.currentTarget.value)}
						onKeyDown={(ev) => {
							// Enter/Escape that confirm an IME candidate are not ours (WebKit reports them as keyCode 229 after compositionend).
							if (ev.isComposing || ev.keyCode === 229) return;
							// The key's default action lands where focus is after the handler; keep it out of the editor.
							if (ev.key === "Enter") {
								ev.preventDefault();
								addComment();
							}
							if (ev.key === "Escape") {
								ev.preventDefault();
								cancelComment();
							}
						}}
					/>
					<Button type="button" onClick={addComment}>
						{t("markdownTab.addCommentConfirm", "Add")}
					</Button>
					<Button type="button" onClick={cancelComment}>
						{t("markdownTab.addCommentCancel", "Cancel")}
					</Button>
				</Show>
				<Show when={dirty()}>
					<Button type="button" data-ready="true" style={{ "margin-left": "auto" }} onClick={() => void save()}>
						{t("markdownTab.saveLive", "Save")}
					</Button>
				</Show>
			</div>
			<Show when={conflict() !== null}>
				<div class={s.conflict} role="alert">
					<span>
						{t(
							"markdownTab.liveConflict",
							"This file changed on disk while you were editing. Saving would overwrite those changes.",
						)}
					</span>
					<Button type="button" onClick={reloadFromDisk}>
						{t("markdownTab.liveReload", "Reload")}
					</Button>
					<Button type="button" onClick={() => void commit()}>
						{t("markdownTab.liveOverwrite", "Overwrite")}
					</Button>
				</div>
			</Show>
			<div class={s.content} onClick={handleClick}>
				<ContentRenderer
					content={text()}
					commentableBlocks
					baseDir={props.baseDir}
					onLinkClick={props.onLinkClick}
					onCheckboxToggle={toggleMark}
					fontSize={props.fontSize}
					contentRef={(el) => {
						container = el;
					}}
				/>
			</div>
		</div>
	);
};
