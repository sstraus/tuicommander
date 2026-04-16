import { Component, createEffect, createMemo, createSignal, For, Show } from "solid-js";
import { DiffViewer, parseDiffFiles, type DiffFileSection } from "../ui/DiffViewer";
import { editorTabsStore } from "../../stores/editorTabs";
import { mdTabsStore } from "../../stores/mdTabs";
import { uiStore, type DiffViewMode } from "../../stores/ui";
import { repositoriesStore } from "../../stores/repositories";
import { useRepository } from "../../hooks/useRepository";
import { classifyFile } from "../../utils/filePreview";
import { t } from "../../i18n";
import { cx } from "../../utils";
import s from "../PrDiffTab/PrDiffTab.module.css";

/** Reconstruct the raw diff string for a single file section */
function sectionToRawDiff(section: DiffFileSection): string {
  return section.lines.map((l) => l.content).join("\n");
}

const FileSection: Component<{ file: DiffFileSection; baseMode: DiffViewMode; repoPath: string }> = (props) => {
  const [collapsed, setCollapsed] = createSignal(false);

  const openFile = () => {
    const target = classifyFile(props.file.path);
    if (target === "markdown") {
      mdTabsStore.add(props.repoPath, props.file.path);
    } else if (target === "preview") {
      mdTabsStore.addHtmlPreview(props.repoPath, props.file.path);
    } else {
      editorTabsStore.add(props.repoPath, props.file.path);
    }
  };

  return (
    <div class={s.fileSection}>
      <div class={s.fileHeader}>
        <svg
          class={cx(s.chevron, collapsed() && s.chevronCollapsed)}
          width="12" height="12" viewBox="0 0 16 16" fill="currentColor"
          onClick={(e) => { e.stopPropagation(); setCollapsed(!collapsed()); }}
          style={{ cursor: "pointer" }}
        >
          <path d="M4 6l4 4 4-4" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
        </svg>
        <span class={s.filePath} onClick={openFile} style={{ cursor: "pointer" }}>{props.file.path}</span>
        <span class={s.fileStats}>
          <Show when={props.file.additions > 0}>
            <span class={s.statAdd}>+{props.file.additions}</span>
          </Show>
          <Show when={props.file.deletions > 0}>
            <span class={s.statDel}>-{props.file.deletions}</span>
          </Show>
        </span>
      </div>
      <Show when={!collapsed()}>
        <div class={s.fileDiff}>
          <DiffViewer diff={sectionToRawDiff(props.file)} mode={props.baseMode} />
        </div>
      </Show>
    </div>
  );
};

export interface BranchDiffScrollViewProps {
  repoPath: string;
  /** Pass a ref callback to get the container element for Cmd+F search */
  contentRef?: (el: HTMLElement) => void;
}

/**
 * All-files diff scroll view for the current working tree.
 * Shows every changed file as a collapsible section in a continuous scroll.
 * Reactively reloads on git operations via repositoriesStore.getRevision.
 */
export const BranchDiffScrollView: Component<BranchDiffScrollViewProps> = (props) => {
  const repo = useRepository();
  const [diff, setDiff] = createSignal("");
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);

  // Reactively reload when git state changes
  createEffect(() => {
    const repoPath = props.repoPath;
    if (!repoPath) return;
    // Track revision for reactivity
    void repositoriesStore.getRevision(repoPath);

    if (!diff()) setLoading(true);
    setError(null);
    // Fetch both unstaged and staged diffs, concatenate for a full picture
    Promise.all([
      repo.getDiff(repoPath),
      repo.getDiff(repoPath, "staged"),
    ]).then(([unstaged, staged]) => {
      // Concatenate: staged first, then unstaged (avoids duplicate files
      // since git diff and git diff --cached don't overlap)
      setDiff([staged, unstaged].filter(Boolean).join("\n"));
      setLoading(false);
    }).catch((err) => {
      setError(String(err));
      setLoading(false);
    });
  });

  const files = createMemo(() => parseDiffFiles(diff()).filter((f) => f.additions > 0 || f.deletions > 0));
  const totalAdd = createMemo(() => files().reduce((sum, f) => sum + f.additions, 0));
  const totalDel = createMemo(() => files().reduce((sum, f) => sum + f.deletions, 0));

  // In scroll mode, each DiffViewer uses unified or split (not "scroll" which DiffViewer doesn't understand)
  const baseMode = (): DiffViewMode => {
    const m = uiStore.state.diffViewMode;
    return m === "scroll" ? "unified" : m;
  };

  return (
    <div class={s.container} ref={props.contentRef}>
      <div class={s.header}>
        <span class={s.headerTitle}>
          {t("diffScroll.title", "All Changes")}
        </span>
        <span class={s.headerStats}>
          {files().length} {t("diffScroll.files", "files")}
          {" "}
          <span class={s.statAdd}>+{totalAdd()}</span>
          {" "}
          <span class={s.statDel}>-{totalDel()}</span>
        </span>
      </div>
      <Show when={loading()}>
        <div class={s.emptyState}>{t("diffTab.loading", "Loading diff...")}</div>
      </Show>
      <Show when={error()}>
        <div class={s.emptyState}>{t("diffTab.error", "Error:")} {error()}</div>
      </Show>
      <Show when={!loading() && !error() && files().length === 0}>
        <div class={s.emptyState}>{t("diffScroll.noChanges", "No uncommitted changes")}</div>
      </Show>
      <Show when={!loading() && !error() && files().length > 0}>
        <For each={files()}>
          {(file) => <FileSection file={file} baseMode={baseMode()} repoPath={props.repoPath} />}
        </For>
      </Show>
    </div>
  );
};
