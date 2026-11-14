import { pathBasename } from "../utils/pathUtils";
import { clampFontSize, FONT_STEP } from "../utils/terminalZoom";
import { branchKeyFor } from "./repositories";
import { type BaseTab, createTabManager } from "./tabManager";

/** Editor tab data */
export interface EditorTabData extends BaseTab {
	/** Canonical repo path — drives branch-scope filtering and repo-store ops. */
	repoPath: string;
	/** On-disk root for file I/O. Equals the worktree path when active, otherwise repoPath. */
	fsRoot: string;
	filePath: string;
	fileName: string; // Display name (basename of filePath)
	isDirty: boolean;
	initialLine?: number; // 1-based line to select on first mount
	initialCol?: number; // 1-based column to select on first mount
	externalEditable?: boolean; // Allow editing external (absolute-path) files
	cursorLine?: number; // 1-based cursor line, surfaced for custom-launcher {line}
	cursorCol?: number; // 1-based cursor column, surfaced for custom-launcher {column}
}

function createEditorTabsStore() {
	const base = createTabManager<EditorTabData>("editor");
	// Imperative per-tab handles (e.g. openSearch) so the global Cmd+F router can
	// drive the active editor tab even when focus left the CodeMirror content
	// (e.g. while dragging the scrollbar). Mirrors diffTabsStore / mdTabsStore.
	const handles = new Map<string, unknown>();

	return {
		state: base.state,
		remove: base.remove,
		setActive: base.setActive,
		clearAll: base.clearAll,
		get: base.get,
		getIds: base.getIds,
		getVisibleIds: base.getVisibleIds,
		getActive: base.getActive,
		getCount: base.getCount,

		/** Zoom the active editor, or a specified tab for collective zoom. */
		zoomIn(defaultFontSize: number, id = base.state.activeId): void {
			if (!id) return;
			base._setState(
				"tabs",
				id,
				"fontSize",
				clampFontSize((base.state.tabs[id]?.fontSize ?? defaultFontSize) + FONT_STEP),
			);
		},
		zoomOut(defaultFontSize: number, id = base.state.activeId): void {
			if (!id) return;
			base._setState(
				"tabs",
				id,
				"fontSize",
				clampFontSize((base.state.tabs[id]?.fontSize ?? defaultFontSize) - FONT_STEP),
			);
		},
		zoomReset(defaultFontSize: number, id = base.state.activeId): void {
			if (!id) return;
			base._setState("tabs", id, "fontSize", clampFontSize(defaultFontSize));
		},
		setPinned: base.setPinned,
		reorderByIds: base.reorderByIds,

		/** Register an imperative handle for a tab (e.g. openSearch) */
		setHandle(tabId: string, handle: unknown): void {
			handles.set(tabId, handle);
		},

		/** Remove the imperative handle when a tab component unmounts */
		clearHandle(tabId: string): void {
			handles.delete(tabId);
		},

		/** Retrieve the imperative handle for a tab */
		getHandle<T = unknown>(tabId: string): T | undefined {
			return handles.get(tabId) as T | undefined;
		},

		/** Add a new editor tab (or activate existing if same file already open).
		 *  Deactivates the terminal/diff/markdown panes so the editor is the only one showing.
		 *  Pass `fsRoot` via opts when the file lives in a worktree that differs from the canonical repo path.
		 *  Pass `background` when the caller must not steal focus — an opener that
		 *  deliberately leaves the user in another repo has to leave the tab inactive
		 *  too, or its content shows with its tab button filtered out of the bar. */
		add(
			repoPath: string,
			filePath: string,
			initialLine?: number,
			opts?: { fsRoot?: string; externalEditable?: boolean; background?: boolean; initialCol?: number },
		): string {
			const fsRoot = opts?.fsRoot ?? repoPath;
			const existing = Object.values(base.state.tabs).find(
				(tab) => !tab.mcpUiId && tab.repoPath === repoPath && tab.fsRoot === fsRoot && tab.filePath === filePath,
			);
			if (existing) {
				if (initialLine !== undefined) {
					base._setState("tabs", existing.id, "initialLine", initialLine);
					base._setState("tabs", existing.id, "initialCol", opts?.initialCol);
					const handle = handles.get(existing.id) as
						| { goToPosition?: (line: number, col?: number) => void }
						| undefined;
					handle?.goToPosition?.(initialLine, opts?.initialCol);
				}
				if (!opts?.background) base.setActive(existing.id);
				return existing.id;
			}

			const id = base._nextId("edit");
			const fileName = pathBasename(filePath) || filePath;
			const add = opts?.background ? base._addTabBackground : base._addTab;
			return add({
				id,
				repoPath,
				fsRoot,
				filePath,
				fileName,
				isDirty: false,
				branchKey: branchKeyFor(repoPath),
				initialLine,
				initialCol: opts?.initialCol,
				externalEditable: opts?.externalEditable,
			});
		},

		/** Open a native editor tab using the MCP id, independent of its file path. */
		addMcpFile(
			mcpUiId: string,
			repoPath: string,
			filePath: string,
			initialLine: number | undefined,
			pinned: boolean,
			opts: { background: boolean; externalEditable: boolean; initialCol?: number },
		): string {
			const existing = Object.values(base.state.tabs).find((tab) => tab.mcpUiId === mcpUiId);
			const id = existing?.id ?? base._nextId("edit");
			const tab: EditorTabData = {
				id,
				mcpUiId,
				repoPath,
				fsRoot: repoPath,
				filePath,
				fileName: pathBasename(filePath) || filePath,
				isDirty: false,
				branchKey: branchKeyFor(repoPath),
				initialLine,
				initialCol: opts.initialCol,
				externalEditable: opts.externalEditable,
				pinned,
				pinAcrossRepos: true,
			};
			if (existing) {
				base._setState("tabs", id, tab);
				if (!opts.background) base.setActive(id);
				return id;
			}
			return opts.background ? base._addTabBackground(tab) : base._addTab(tab);
		},

		closeMcpFile(mcpUiId: string): void {
			const existing = Object.values(base.state.tabs).find((tab) => tab.mcpUiId === mcpUiId);
			if (existing) base.remove(existing.id);
		},

		/** Mark a tab as dirty or clean */
		setDirty(id: string, isDirty: boolean): void {
			if (base.state.tabs[id]) {
				base._setState("tabs", id, "isDirty", isDirty);
			}
		},

		/** Record the editor cursor position (1-based) for custom-launcher placeholders.
		 *  This fires on every CodeMirror selection change (i.e. every keystroke); skip
		 *  the reactive writes when the position is unchanged so idle re-selections and
		 *  no-op transactions don't churn the store and its Toolbar subscribers. */
		setCursor(id: string, line: number, col: number): void {
			const tab = base.state.tabs[id];
			if (!tab || (tab.cursorLine === line && tab.cursorCol === col)) return;
			base._setState("tabs", id, "cursorLine", line);
			base._setState("tabs", id, "cursorCol", col);
		},

		/** Clear all editor tabs for a repository */
		clearForRepo(repoPath: string): void {
			base._clearWhere((tab) => tab.repoPath === repoPath);
		},

		/** Get tabs for a specific repository */
		getForRepo(repoPath: string): EditorTabData[] {
			return Object.values(base.state.tabs).filter((tab) => tab.repoPath === repoPath);
		},
	};
}

export const editorTabsStore = createEditorTabsStore();
