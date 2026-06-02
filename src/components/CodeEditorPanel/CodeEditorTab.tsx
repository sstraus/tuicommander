import { closeBrackets, closeBracketsKeymap } from "@codemirror/autocomplete";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import type { LanguageSupport } from "@codemirror/language";
import { bracketMatching, foldGutter, foldKeymap, indentOnInput } from "@codemirror/language";
import { highlightSelectionMatches, search, searchKeymap } from "@codemirror/search";
import { type Extension, StateEffect, StateField } from "@codemirror/state";
import {
	crosshairCursor,
	Decoration,
	type DecorationSet,
	drawSelection,
	dropCursor,
	EditorView,
	highlightActiveLine,
	highlightActiveLineGutter,
	highlightSpecialChars,
	keymap,
	lineNumbers,
	rectangularSelection,
	scrollPastEnd,
} from "@codemirror/view";
import { colorPicker } from "@replit/codemirror-css-color-picker";
import { createCodeMirror, createEditorControlledValue, createEditorReadonly } from "solid-codemirror";
import { type Component, createEffect, createSignal, on, onCleanup, Show } from "solid-js";
import { useFileBrowser } from "../../hooks/useFileBrowser";
import { t } from "../../i18n";
import { invoke } from "../../invoke";
import { isMacOS, shortenHomePath } from "../../platform";
import { appLogger } from "../../stores/appLogger";
import { diffTabsStore } from "../../stores/diffTabs";
import { editorTabsStore } from "../../stores/editorTabs";
import { referencesStore } from "../../stores/references";
import { repositoriesStore } from "../../stores/repositories";
import { uiStore } from "../../stores/ui";
import { openFileAction } from "../../utils/filePreview";
import { isAbsolutePath } from "../../utils/pathUtils";
import { ContextMenu, createContextMenu } from "../ContextMenu";
import e from "../shared/editor-header.module.css";
import s from "./CodeEditorTab.module.css";
import { detectLanguage } from "./languageDetection";
import { codeEditorTheme } from "./theme";

export interface CodeEditorTabProps {
	id: string;
	repoPath: string;
	/** On-disk root for file I/O (worktree path when active, otherwise repoPath). */
	fsRoot?: string;
	filePath: string;
	initialLine?: number; // Line to scroll to on first mount (1-based)
	externalEditable?: boolean; // External files start unlocked when true, locked (but unlockable) when false
	onClose?: () => void;
}

function wordAtCursor(view: EditorView): string | null {
	const range = wordRangeAt(view, view.state.selection.main.head);
	if (!range) return null;
	return view.state.doc.sliceString(range.from, range.to) || null;
}

/** Large file threshold — skip syntax highlighting above this size */
const LARGE_FILE_BYTES = 500 * 1024;

// --- Cmd+Hover underline (VS Code-style go-to-definition hint) ---

const setHoverLink = StateEffect.define<{ from: number; to: number } | null>();

const hoverLinkMark = Decoration.mark({ class: "cm-hover-link" });

const hoverLinkField = StateField.define<DecorationSet>({
	create: () => Decoration.none,
	update(decos, tr) {
		for (const e of tr.effects) {
			if (e.is(setHoverLink)) {
				return e.value ? Decoration.set([hoverLinkMark.range(e.value.from, e.value.to)]) : Decoration.none;
			}
		}
		return decos;
	},
	provide: (f) => EditorView.decorations.from(f),
});

const hoverLinkTheme = EditorView.baseTheme({
	".cm-hover-link": {
		textDecoration: "underline",
		cursor: "pointer",
		color: "var(--fg-link, #4fc1ff)",
	},
});

function wordRangeAt(view: EditorView, pos: number): { from: number; to: number } | null {
	const line = view.state.doc.lineAt(pos);
	const text = line.text;
	const col = pos - line.from;
	const wordChars = /[\w$]/;
	let start = col;
	let end = col;
	while (start > 0 && wordChars.test(text[start - 1])) start--;
	while (end < text.length && wordChars.test(text[end])) end++;
	if (start === end) return null;
	return { from: line.from + start, to: line.from + end };
}

function hoverLinkHandlers(): Extension {
	return EditorView.domEventHandlers({
		mousemove(event: MouseEvent, view: EditorView) {
			const modKey = isMacOS() ? event.metaKey : event.ctrlKey;
			if (!modKey) {
				if (view.state.field(hoverLinkField) !== Decoration.none) {
					view.dispatch({ effects: setHoverLink.of(null) });
				}
				return false;
			}
			const pos = view.posAtCoords({ x: event.clientX, y: event.clientY });
			if (pos === null) {
				view.dispatch({ effects: setHoverLink.of(null) });
				return false;
			}
			const range = wordRangeAt(view, pos);
			view.dispatch({ effects: setHoverLink.of(range) });
			return false;
		},
		mouseleave(_event: MouseEvent, view: EditorView) {
			view.dispatch({ effects: setHoverLink.of(null) });
			return false;
		},
		keyup(event: KeyboardEvent, view: EditorView) {
			const modKey = isMacOS() ? "Meta" : "Control";
			if (event.key === modKey) {
				view.dispatch({ effects: setHoverLink.of(null) });
			}
			return false;
		},
	});
}

export const CodeEditorTab: Component<CodeEditorTabProps> = (props) => {
	const [langSupport, setLangSupport] = createSignal<LanguageSupport | null>(null);
	/** Signal for external content pushes (disk load/reload) — drives createEditorControlledValue */
	const [code, setCode] = createSignal("");
	/** Mutable ref tracking live editor value without triggering reactivity on every keystroke */
	let currentCode = "";
	const [savedContent, setSavedContent] = createSignal("");
	const [loading, setLoading] = createSignal(true);
	const [error, setError] = createSignal<string | null>(null);
	const [isReadOnly, setIsReadOnly] = createSignal(false);
	/** True when the file changed on disk while editor has unsaved changes */
	const [diskConflict, setDiskConflict] = createSignal(false);
	/** Reactive dirty flag — only transitions on save/load, not every keystroke */
	const [dirty, setDirty] = createSignal(false);
	/** Current symbol under cursor (for breadcrumb) */
	const [currentSymbol, setCurrentSymbol] = createSignal<string | null>(null);
	let outlineSymbols: { name: string; lineStart: number; lineEnd: number | null }[] = [];
	let outlineGeneration = 0;
	const contextMenu = createContextMenu();
	const fb = useFileBrowser();

	/** True when the file path is absolute (outside the repository) */
	const isExternal = () => isAbsolutePath(props.filePath);

	/** Filesystem root for disk I/O — worktree when active, otherwise canonical repoPath. */
	const fsRoot = () => props.fsRoot ?? props.repoPath;

	/** Guard: scroll to initialLine only once on first file load */
	let didScrollToInitialLine = false;

	/** Read file content — uses the right command depending on internal vs external */
	const readContent = async (): Promise<string> => {
		if (isExternal()) {
			return invoke<string>("read_external_file", { path: props.filePath });
		}
		return fb.readFile(fsRoot(), props.filePath);
	};

	// Sync dirty state to tab store for the tab bar indicator
	createEffect(() => {
		editorTabsStore.setDirty(props.id, dirty());
	});

	// Load file content
	createEffect(
		on(
			() => [fsRoot(), props.filePath] as const,
			async ([_fsRoot, filePath]) => {
				if (!filePath) return;

				setLoading(true);
				setError(null);
				if (isExternal() && !props.externalEditable) setIsReadOnly(true);

				try {
					const content = await readContent();
					currentCode = content;
					setCode(content);
					setSavedContent(content);
					setDirty(false);

					// Scroll to initialLine on the very first load only
					if (props.initialLine !== undefined && !didScrollToInitialLine) {
						didScrollToInitialLine = true;
						const targetLine = props.initialLine;
						requestAnimationFrame(() => {
							const view = editorView();
							if (!view) return;
							const line = view.state.doc.line(Math.max(1, Math.min(targetLine, view.state.doc.lines)));
							view.dispatch({ effects: EditorView.scrollIntoView(line.from, { y: "center" }) });
						});
					}
				} catch (err) {
					setError(String(err));
					currentCode = "";
					setCode("");
					setSavedContent("");
					setDirty(false);
				} finally {
					setLoading(false);
				}
			},
		),
	);

	// Fetch outline symbols for breadcrumb (non-blocking)
	createEffect(
		on(
			() => [props.repoPath, props.filePath] as const,
			async ([repoPath, filePath]) => {
				if (!repoPath || !filePath) {
					outlineSymbols = [];
					setCurrentSymbol(null);
					return;
				}
				const gen = ++outlineGeneration;
				try {
					const symbols = await invoke<{ name: string; lineStart: number; lineEnd: number | null }[]>("mdkb_outline", {
						repoPath,
						filePath,
					});
					if (gen === outlineGeneration) outlineSymbols = symbols;
				} catch (err) {
					if (gen === outlineGeneration) outlineSymbols = [];
					appLogger.debug("editor", "mdkb_outline failed", err);
				}
			},
		),
	);

	/** Check disk content and reload or show conflict banner */
	const checkDiskContent = async () => {
		if (!savedContent()) return;
		try {
			const diskContent = await readContent();
			if (diskContent === savedContent()) return;

			if (currentCode !== savedContent()) {
				setDiskConflict(true);
			} else {
				currentCode = diskContent;
				setCode(diskContent);
				setSavedContent(diskContent);
				setDirty(false);
			}
		} catch (err) {
			appLogger.debug("app", `checkDiskContent failed (file may be deleted): ${props.filePath}`, err);
		}
	};

	// Re-check file content on git changes (revision signal)
	createEffect(() => {
		const repoPath = props.repoPath;
		if (!repoPath || isExternal()) return;
		const rev = repositoriesStore.getRevision(repoPath);
		if (rev === 0 || !savedContent()) return;
		void checkDiskContent();
	});

	// Poll for file changes (agent edits, external tools).
	// 5s interval, skip while tab is hidden to avoid competing with terminal I/O.
	createEffect(() => {
		if (!props.filePath) return;
		const timer = setInterval(() => {
			if (document.visibilityState === "hidden") return;
			if (editorTabsStore.state.activeId !== props.id) return;
			void checkDiskContent();
		}, 5000);
		onCleanup(() => clearInterval(timer));
	});

	/** Reload content from disk, discarding local changes */
	const handleReloadFromDisk = async () => {
		try {
			const diskContent = await readContent();
			currentCode = diskContent;
			setCode(diskContent);
			setSavedContent(diskContent);
			setDirty(false);
			setDiskConflict(false);
		} catch (err) {
			appLogger.error("app", "Failed to reload file", err);
		}
	};

	/** Keep local changes, dismiss the conflict banner (next save will overwrite disk) */
	const handleKeepLocal = () => {
		setDiskConflict(false);
	};

	const { ref, editorView, createExtension } = createCodeMirror({
		onValueChange: (value) => {
			currentCode = value;
			const nowDirty = value !== savedContent();
			if (nowDirty !== dirty()) setDirty(nowDirty);
		},
	});

	// Controlled value — sync external changes into editor
	createEditorControlledValue(editorView, code);

	// Read-only mode
	createEditorReadonly(editorView, isReadOnly);

	// Base extensions
	createExtension(codeEditorTheme);
	createExtension(lineNumbers());
	createExtension(history());
	createExtension(foldGutter());
	createExtension(drawSelection());
	createExtension(highlightActiveLine());
	createExtension(highlightActiveLineGutter());
	createExtension(highlightSpecialChars());
	createExtension(dropCursor());
	createExtension(rectangularSelection());
	createExtension(crosshairCursor());
	createExtension(bracketMatching());
	createExtension(closeBrackets());
	createExtension(indentOnInput());
	createExtension(scrollPastEnd());
	createExtension(colorPicker);
	createExtension(
		keymap.of([
			...defaultKeymap,
			...historyKeymap,
			...foldKeymap,
			...closeBracketsKeymap,
			...searchKeymap,
			indentWithTab,
		]),
	);
	createExtension(search());
	createExtension(highlightSelectionMatches());

	// Cmd+Hover underline (VS Code-style link hint)
	createExtension(hoverLinkField);
	createExtension(hoverLinkTheme);
	createExtension(hoverLinkHandlers());

	// Cmd+Click (Mac) / Ctrl+Click (other) → go to definition via mdkb
	createExtension(
		EditorView.domEventHandlers({
			click(event: MouseEvent, view: EditorView) {
				const modKey = isMacOS() ? event.metaKey : event.ctrlKey;
				if (!modKey) return false;
				const pos = view.posAtCoords({ x: event.clientX, y: event.clientY });
				if (pos === null) return false;
				const line = view.state.doc.lineAt(pos);
				const col = pos - line.from;
				invoke<{ filePath: string; line: number } | null>("mdkb_goto_definition", {
					repoPath: props.repoPath,
					filePath: props.filePath,
					line: line.number,
					col,
				})
					.then((result) => {
						if (!result) return;
						openFileAction(result.filePath, props.repoPath, fsRoot(), result.line);
					})
					.catch((e) => appLogger.debug("editor", "go-to-definition failed", { error: String(e) }));
				return true;
			},
		}),
	);

	// Shift+F12 → find references for word under cursor
	createExtension(
		keymap.of([
			{
				key: "Shift-F12",
				run(view: EditorView) {
					const word = wordAtCursor(view);
					if (!word) return false;
					void referencesStore.findReferences(props.repoPath, fsRoot(), word);
					uiStore.setReferencesPanelVisible(true);
					return true;
				},
			},
		]),
	);

	// Reactive language extension
	createExtension((): Extension => langSupport() ?? []);

	// Update breadcrumb on cursor movement
	createExtension(
		EditorView.updateListener.of((update) => {
			if (!update.selectionSet || outlineSymbols.length === 0) return;
			const line = update.state.doc.lineAt(update.state.selection.main.head).number;
			let best: string | null = null;
			for (const sym of outlineSymbols) {
				if (line >= sym.lineStart && (sym.lineEnd === null || line <= sym.lineEnd)) {
					best = sym.name;
				}
			}
			setCurrentSymbol(best);
		}),
	);

	// Force CodeMirror to recalculate layout when the editor container resizes.
	// The container starts as display:none (.terminal-pane without .active),
	// so CodeMirror computes zero dimensions during initial mount. When the
	// container becomes visible (0→real size), ResizeObserver fires and we
	// tell CodeMirror to re-measure. We also re-measure when loading completes
	// (display:none → visible transition on the editor div itself).
	let editorDiv: HTMLDivElement | undefined;
	createEffect(() => {
		const view = editorView();
		if (!view || !editorDiv) return;
		const ro = new ResizeObserver(() => {
			// Use rAF to ensure the browser has completed the layout pass before
			// CodeMirror measures. Plain requestMeasure() can run too early after
			// a display:none → block transition.
			requestAnimationFrame(() => view.requestMeasure());
		});
		ro.observe(editorDiv);
		onCleanup(() => ro.disconnect());
	});

	// Load language support
	createEffect(
		on(
			() => props.filePath,
			async (filePath) => {
				if (!filePath) {
					setLangSupport(null);
					return;
				}
				// Skip syntax highlighting for large files
				if (currentCode.length > LARGE_FILE_BYTES) {
					setLangSupport(null);
					return;
				}
				const lang = await detectLanguage(filePath);
				setLangSupport(lang);
			},
		),
	);

	// Save handler
	const handleSave = async () => {
		if (!dirty() || isReadOnly()) return;
		try {
			if (isExternal()) {
				await invoke("write_external_file", { path: props.filePath, content: currentCode });
			} else {
				await fb.writeFile(fsRoot(), props.filePath, currentCode);
			}
			setSavedContent(currentCode);
			setDirty(false);
			// Notify revision-subscribed panels (e.g. MarkdownTab) that a file changed on disk
			if (props.repoPath) {
				repositoriesStore.bumpRevision(props.repoPath);
			}
		} catch (err) {
			appLogger.error("app", "Failed to save file", err);
			setError(String(err));
		}
	};

	// Cmd+S save shortcut
	createEffect(() => {
		const handleKeydown = (e: KeyboardEvent) => {
			const isMeta = e.metaKey || e.ctrlKey;
			if (isMeta && e.key === "s") {
				// Only handle if this tab's container has focus
				const container = document.querySelector(`[data-editor-tab-id="${props.id}"]`);
				if (!container?.contains(document.activeElement)) return;

				e.preventDefault();
				void handleSave();
			}
		};

		document.addEventListener("keydown", handleKeydown);
		onCleanup(() => document.removeEventListener("keydown", handleKeydown));
	});

	return (
		<div class={s.tabContent} data-editor-tab-id={props.id}>
			<div
				class={e.header}
				onContextMenu={(ev) => {
					ev.preventDefault();
					contextMenu.open(ev);
				}}
			>
				<span class={e.filename} title={props.filePath}>
					{props.filePath}
				</span>
				<Show when={currentSymbol()}>
					<span class={e.breadcrumb} title={currentSymbol()!}>
						<span class={e.breadcrumbSep}>{"›"}</span>
						{currentSymbol()}
					</span>
				</Show>
				<Show when={dirty()}>
					<span class={e.dirtyDot} title={t("codeEditor.unsaved", "Unsaved changes")} />
				</Show>
				<button
					class={e.btn}
					onClick={() => setIsReadOnly((v) => !v)}
					title={isReadOnly() ? t("codeEditor.unlock", "Unlock editing") : t("codeEditor.lock", "Lock (read-only)")}
				>
					{isReadOnly() ? (
						<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
							<path d="M8 1a3 3 0 0 0-3 3v3H4a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1V8a1 1 0 0 0-1-1h-1V4a3 3 0 0 0-3-3zm1.5 6H6.5V4a1.5 1.5 0 0 1 3 0v3z" />
						</svg>
					) : (
						<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
							<path d="M8 1a3 3 0 0 1 3 3v1h.5a1.5 1.5 0 0 1 1.5 1.5V14a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V6.5A1.5 1.5 0 0 1 4.5 5H5V4a3 3 0 0 1 3-3zm1.5 4V4a1.5 1.5 0 0 0-3 0v1h3z" />
						</svg>
					)}
				</button>
				<Show when={!isExternal() && props.repoPath}>
					<button
						class={e.btn}
						// Diff against fsRoot (the worktree) where the file actually lives and is
						// modified — props.repoPath is the canonical repo, so on a worktree git diff
						// would run in the wrong tree and report "No changes". (#67)
						onClick={() => diffTabsStore.add(fsRoot(), props.filePath, "M")}
						title={t("codeEditor.viewDiff", "View diff")}
					>
						<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
							<path
								d="M2 3h5v1H2zm0 3h5v1H2zm0 3h4v1H2zm7-6h5v1H9zm0 3h5v1H9zm0 3h4v1H9zM7.5 1v14M.5 0v16"
								fill="none"
								stroke="currentColor"
								stroke-width="1"
								opacity="0.5"
							/>
							<path d="M4 12l-2 2 2 2" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
							<path d="M12 12l2 2-2 2" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
						</svg>
					</button>
				</Show>
				<Show when={dirty() && !isReadOnly()}>
					<button class={e.btn} onClick={handleSave} title={`${t("codeEditor.save", "Save")} (${"\u2318"}S)`}>
						<svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
							<path d="M13.354 1.146a.5.5 0 0 1 .146.354v12a1.5 1.5 0 0 1-1.5 1.5h-8A1.5 1.5 0 0 1 2.5 13.5v-11A1.5 1.5 0 0 1 4 1h8.5a.5.5 0 0 1 .354.146L13.354 1.146zM4 2.5a.5.5 0 0 0-.5.5v10.5a.5.5 0 0 0 .5.5h1V10a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v4h1a.5.5 0 0 0 .5-.5V2.207L11.793 2H11v2.5A1.5 1.5 0 0 1 9.5 6h-3A1.5 1.5 0 0 1 5 4.5V2H4zm2 0v2.5a.5.5 0 0 0 .5.5h3a.5.5 0 0 0 .5-.5V2H6zm0 8v4h4v-4H6z" />
						</svg>
					</button>
				</Show>
			</div>

			<Show when={diskConflict()}>
				<div class={s.conflictBanner}>
					<span>{t("codeEditor.fileChanged", "File changed on disk.")}</span>
					<button class={e.btn} onClick={handleReloadFromDisk}>
						{t("codeEditor.reload", "Reload")}
					</button>
					<button class={e.btn} onClick={handleKeepLocal}>
						{t("codeEditor.keepMine", "Keep mine")}
					</button>
				</div>
			</Show>

			<Show when={loading()}>
				<div class={s.empty}>{t("codeEditor.loading", "Loading...")}</div>
			</Show>

			<Show when={error()}>
				<div class={s.empty} style={{ color: "var(--error)" }}>
					{t("codeEditor.error", "Error:")} {error()}
				</div>
			</Show>

			{/* Always mount the editor div so solid-codemirror's ref callback fires during
          initial component mount. Wrapping in <Show> defers the ref, causing onMount
          inside createCodeMirror to never fire in production builds — the editorView
          signal stays undefined and content/extensions are never applied. */}
			<div
				class={s.editorContent}
				ref={(el) => {
					editorDiv = el;
					ref(el);
				}}
				style={{ display: loading() || error() ? "none" : undefined }}
			/>

			<ContextMenu
				items={[
					{
						label: t("codeEditor.copyPath", "Copy Path"),
						action: () => {
							const fullPath = isExternal() ? props.filePath : `${fsRoot()}/${props.filePath}`;
							navigator.clipboard
								.writeText(shortenHomePath(fullPath))
								.catch((err) => appLogger.error("app", "Failed to copy path", err));
						},
					},
					{
						label: "Find References (Shift+F12)",
						action: () => {
							const view = editorView();
							if (!view) return;
							const word = wordAtCursor(view);
							if (!word) return;
							void referencesStore.findReferences(props.repoPath, fsRoot(), word);
							uiStore.setReferencesPanelVisible(true);
						},
					},
				]}
				x={contextMenu.position().x}
				y={contextMenu.position().y}
				visible={contextMenu.visible()}
				onClose={contextMenu.close}
			/>
		</div>
	);
};

export default CodeEditorTab;
