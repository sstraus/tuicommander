import { pathBasename } from "../utils/pathUtils";
import { currentBranchKey } from "./repositories";
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
	initialLine?: number; // Line to scroll to on first mount
	externalEditable?: boolean; // Allow editing external (absolute-path) files
	cursorLine?: number; // 1-based cursor line, surfaced for custom-launcher {line}
	cursorCol?: number; // 1-based cursor column, surfaced for custom-launcher {column}
}

function createEditorTabsStore() {
	const base = createTabManager<EditorTabData>("editor");

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
		setPinned: base.setPinned,
		reorderByIds: base.reorderByIds,

		/** Add a new editor tab (or activate existing if same file already open).
		 *  Pass `fsRoot` via opts when the file lives in a worktree that differs from the canonical repo path. */
		add(
			repoPath: string,
			filePath: string,
			initialLine?: number,
			opts?: { fsRoot?: string; externalEditable?: boolean },
		): string {
			const fsRoot = opts?.fsRoot ?? repoPath;
			const existing = Object.values(base.state.tabs).find(
				(tab) => tab.repoPath === repoPath && tab.fsRoot === fsRoot && tab.filePath === filePath,
			);
			if (existing) {
				base.setActive(existing.id);
				return existing.id;
			}

			const id = base._nextId("edit");
			const fileName = pathBasename(filePath) || filePath;
			return base._addTab({
				id,
				repoPath,
				fsRoot,
				filePath,
				fileName,
				isDirty: false,
				branchKey: currentBranchKey(),
				initialLine,
				externalEditable: opts?.externalEditable,
			});
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
	};
}

export const editorTabsStore = createEditorTabsStore();
