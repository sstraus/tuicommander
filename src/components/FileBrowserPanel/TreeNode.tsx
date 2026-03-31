import { Component, createSignal, For, Show } from "solid-js";
import { useFileBrowser } from "../../hooks/useFileBrowser";
import { appLogger } from "../../stores/appLogger";
import { cx } from "../../utils";
import type { DirEntry } from "../../types/fs";
import { getStatusClass, formatSize } from "./fileUtils";
import { FileIcon } from "./FileIcon";
import g from "../shared/git-status.module.css";
import s from "./FileBrowserPanel.module.css";

export interface TreeNodeProps {
  entry: DirEntry;
  depth: number;
  repoPath: string;
  fsRoot: string;
  expandedDirs: Set<string>;
  onToggleExpand: (path: string) => void;
  onFileOpen: (repoPath: string, filePath: string) => void;
  onContextMenu: (e: MouseEvent, entry: DirEntry) => void;
  /** Cache of loaded children, keyed by dir path */
  childrenCache: Map<string, DirEntry[]>;
  onChildrenLoaded: (path: string, children: DirEntry[]) => void;
}

export const TreeNode: Component<TreeNodeProps> = (props) => {
  const fb = useFileBrowser();
  const [loading, setLoading] = createSignal(false);

  const isExpanded = () => props.expandedDirs.has(props.entry.path);
  const children = () => props.childrenCache.get(props.entry.path) ?? [];

  const handleClick = () => {
    if (props.entry.is_dir) {
      const wasExpanded = isExpanded();
      props.onToggleExpand(props.entry.path);
      // Lazy-load children on first expand (check state BEFORE toggle)
      if (!wasExpanded && !props.childrenCache.has(props.entry.path)) {
        setLoading(true);
        fb.listDirectory(props.fsRoot, props.entry.path).then((entries) => {
          props.onChildrenLoaded(props.entry.path, entries);
          setLoading(false);
        }).catch((err) => {
          appLogger.error("app", "Failed to list directory", { path: props.entry.path, error: err });
          setLoading(false);
        });
      }
    } else {
      props.onFileOpen(props.repoPath, props.entry.path);
    }
  };

  return (
    <>
      <div
        class={cx(s.entry, props.entry.is_dir && s.entryDir, props.entry.is_ignored && s.entryIgnored)}
        style={{ "padding-left": `${8 + props.depth * 16}px` }}
        onClick={handleClick}
        onContextMenu={(e) => props.onContextMenu(e, props.entry)}
      >
        <Show when={props.entry.is_dir}>
          <svg
            class={cx(s.treeChevron, isExpanded() && s.treeChevronExpanded)}
            width="10" height="10" viewBox="0 0 16 16" fill="currentColor"
          >
            <path d="M6 4l4 4-4 4" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
          </svg>
        </Show>
        <Show when={!props.entry.is_dir}>
          <span class={s.treeLeafSpacer} />
        </Show>
        <FileIcon name={props.entry.name} isDir={props.entry.is_dir} class={s.entryIcon} />
        <span class={s.entryName}>{props.entry.name}</span>
        <Show when={props.entry.git_status}>
          <span class={cx(g.dot, getStatusClass(props.entry.git_status))} title={props.entry.git_status} />
        </Show>
        <Show when={!props.entry.is_dir && props.entry.size > 0}>
          <span class={s.entrySize}>{formatSize(props.entry.size)}</span>
        </Show>
      </div>
      {/* Recursive children */}
      <Show when={props.entry.is_dir && isExpanded()}>
        <Show when={loading()}>
          <div class={s.treeLoading} style={{ "padding-left": `${8 + (props.depth + 1) * 16}px` }}>
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
              expandedDirs={props.expandedDirs}
              onToggleExpand={props.onToggleExpand}
              onFileOpen={props.onFileOpen}
              onContextMenu={props.onContextMenu}
              childrenCache={props.childrenCache}
              onChildrenLoaded={props.onChildrenLoaded}
            />
          )}
        </For>
      </Show>
    </>
  );
};
