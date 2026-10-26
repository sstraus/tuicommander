import { type Component, createEffect, createSignal, For, Show } from "solid-js";
import { useFileBrowser } from "../../hooks/useFileBrowser";
import { appLogger } from "../../stores/appLogger";
import type { DirEntry } from "../../types/fs";
import { cx } from "../../utils";
import { isAbsolutePath, joinPath } from "../../utils/pathUtils";
import g from "../shared/git-status.module.css";
import s from "./FileBrowserPanel.module.css";
import { FileIcon } from "./FileIcon";
import { fileTooltip, formatSize, getStatusClass } from "./fileUtils";

export interface TreeNodeProps {
	entry: DirEntry;
	depth: number;
	repoPath: string;
	fsRoot: string;
	/** Relative path of the file open in the active editor, for highlighting. */
	activePath: string | null;
	expandedDirs: Set<string>;
	onToggleExpand: (path: string) => void;
	onFileOpen: (repoPath: string, filePath: string) => void;
	onContextMenu: (e: MouseEvent, entry: DirEntry) => void;
	onPointerDragStart?: (absPath: string, e: PointerEvent) => void;
	/** Cache of loaded children, keyed by dir path */
	childrenCache: Map<string, DirEntry[]>;
	onChildrenLoaded: (path: string, children: DirEntry[]) => void;
}

export const TreeNode: Component<TreeNodeProps> = (props) => {
	const fb = useFileBrowser();
	const [loading, setLoading] = createSignal(false);

	const isExpanded = () => props.expandedDirs.has(props.entry.path);
	const children = () => props.childrenCache.get(props.entry.path) ?? [];

	/**
	 * Load children whenever this node is expanded and the cache has no entry for
	 * it — NOT only on the click that expanded it.
	 *
	 * Every invalidation depends on this. Dropping a cache key is how a mutation
	 * (new file, delete, rename) and a `dir-changed` watcher event both ask the
	 * subtree to reload; with the fetch living in the click handler instead, an
	 * already-expanded folder had nothing to re-read it and either kept showing
	 * stale rows or, once its key was deleted, rendered empty forever.
	 */
	let fetching = false;
	createEffect(() => {
		if (!props.entry.is_dir || !isExpanded()) return;
		if (props.childrenCache.has(props.entry.path)) return;
		if (fetching) return;
		fetching = true;
		setLoading(true);
		fb.listDirectory(props.fsRoot, props.entry.path)
			.then((entries) => props.onChildrenLoaded(props.entry.path, entries))
			.catch((err) => {
				appLogger.error("app", "Failed to list directory", { path: props.entry.path, error: err });
			})
			.finally(() => {
				fetching = false;
				setLoading(false);
			});
	});

	const handleClick = () => {
		if (props.entry.is_dir) {
			props.onToggleExpand(props.entry.path);
		} else {
			props.onFileOpen(props.repoPath, props.entry.path);
		}
	};

	const absPath = () =>
		isAbsolutePath(props.entry.path) ? props.entry.path : joinPath(props.fsRoot, props.entry.path);

	return (
		<>
			<div
				class={cx(
					s.entry,
					props.entry.is_dir && s.entryDir,
					!props.entry.is_dir && props.entry.path === props.activePath && s.entryActive,
					props.entry.is_ignored && s.entryIgnored,
				)}
				style={{ "padding-left": `calc(var(--row-pad-x) + ${props.depth} * var(--tree-indent))` }}
				onClick={handleClick}
				onContextMenu={(e) => props.onContextMenu(e, props.entry)}
				onPointerDown={(e) => props.onPointerDragStart?.(absPath(), e)}
				data-drop-target={props.entry.is_dir ? "folder" : undefined}
				data-abs-path={props.entry.is_dir ? absPath() : undefined}
			>
				<Show when={props.entry.is_dir}>
					<svg
						class={cx(s.treeChevron, isExpanded() && s.treeChevronExpanded)}
						width="10"
						height="10"
						viewBox="0 0 16 16"
						fill="currentColor"
					>
						<path d="M6 4l4 4-4 4" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
					</svg>
				</Show>
				<Show when={!props.entry.is_dir}>
					<span class={s.treeLeafSpacer} />
				</Show>
				<FileIcon
					name={props.entry.name}
					isDir={props.entry.is_dir}
					isLink={props.entry.symlink_target_kind !== undefined}
					class={s.entryIcon}
				/>
				<span class={s.entryName} title={fileTooltip(props.entry)}>
					{props.entry.name}
					<Show when={props.entry.symlink_target_kind}>
						<span class={s.entryLink}>
							{props.entry.symlink_target_kind} link → {props.entry.symlink_target ?? "Target unavailable"}
						</span>
					</Show>
				</span>
				<Show when={props.entry.git_status}>
					<span class={cx(g.dot, getStatusClass(props.entry.git_status))} title={props.entry.git_status} />
				</Show>
				<Show when={!props.entry.is_dir && !props.entry.symlink_target_kind && props.entry.size > 0}>
					<span class={s.entrySize}>{formatSize(props.entry.size)}</span>
				</Show>
			</div>
			{/* Recursive children */}
			<Show when={props.entry.is_dir && isExpanded()}>
				<Show when={loading()}>
					<div
						class={s.treeLoading}
						style={{ "padding-left": `calc(var(--row-pad-x) + ${props.depth + 1} * var(--tree-indent))` }}
					>
						Loading...
					</div>
				</Show>
				<For each={children()}>
					{(child) => (
						<TreeNode
							entry={child}
							depth={props.depth + 1}
							repoPath={props.repoPath}
							fsRoot={props.fsRoot}
							activePath={props.activePath}
							expandedDirs={props.expandedDirs}
							onToggleExpand={props.onToggleExpand}
							onFileOpen={props.onFileOpen}
							onContextMenu={props.onContextMenu}
							onPointerDragStart={props.onPointerDragStart}
							childrenCache={props.childrenCache}
							onChildrenLoaded={props.onChildrenLoaded}
						/>
					)}
				</For>
			</Show>
		</>
	);
};
