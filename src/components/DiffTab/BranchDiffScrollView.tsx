import { Component, createEffect, createSignal, For, Show } from "solid-js";
import { DiffViewer, parseDiffFiles, type DiffFileSection } from "../ui/DiffViewer";
import { uiStore, type DiffViewMode } from "../../stores/ui";
import { repositoriesStore } from "../../stores/repositories";
import { useRepository } from "../../hooks/useRepository";
import { t } from "../../i18n";
import { cx } from "../../utils";
import s from "../PrDiffTab/PrDiffTab.module.css";

/** Reconstruct the raw diff string for a single file section */
function sectionToRawDiff(section: DiffFileSection): string {
  return section.lines.map((l) => l.content).join("\n");
}

const FileSection: Component<{ file: DiffFileSection; baseMode: DiffViewMode }> = (props) => {
  const [collapsed, setCollapsed] = createSignal(false);

  return (
    <div class={s.fileSection}>
      <div class={s.fileHeader} onClick={() => setCollapsed(!collapsed())}>
        <svg
          class={cx(s.chevron, collapsed() && s.chevronCollapsed)}
          width="12" height="12" viewBox="0 0 16 16" fill="currentColor"
        >
          <path d="M4 6l4 4 4-4" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
        </svg>
        <span class={s.filePath}>{props.file.path}</span>
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

    setLoading(true);
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

  const files = () => parseDiffFiles(diff()).filter((f) => f.additions > 0 || f.deletions > 0);
  const totalAdd = () => files().reduce((sum, f) => sum + f.additions, 0);
  const totalDel = () => files().reduce((sum, f) => sum + f.deletions, 0);

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
        <div class={s.modeToggle}>
          <button
            class={cx(s.modeBtn, uiStore.state.diffViewMode === "split" && s.modeBtnActive)}
            onClick={() => uiStore.setDiffViewMode("split")}
            title={t("diffTab.splitView", "Side-by-side")}
          >
            <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
              <path d="M1 2h6v12H1V2zm8 0h6v12H9V2zM2 3v10h4V3H2zm8 0v10h4V3h-4z" />
            </svg>
          </button>
          <button
            class={cx(s.modeBtn, uiStore.state.diffViewMode === "unified" && s.modeBtnActive)}
            onClick={() => uiStore.setDiffViewMode("unified")}
            title={t("diffTab.unifiedView", "Inline")}
          >
            <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
              <path d="M1 2h14v12H1V2zm1 1v10h12V3H2z" />
            </svg>
          </button>
          <button
            class={cx(s.modeBtn, uiStore.state.diffViewMode === "scroll" && s.modeBtnActive)}
            onClick={() => uiStore.setDiffViewMode("scroll")}
            title={t("diffScroll.scrollView", "All files")}
          >
            <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
              <path d="M2 2h12v1H2zm0 3h12v1H2zm0 3h10v1H2zm0 3h8v1H2z" />
            </svg>
          </button>
        </div>
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
          {(file) => <FileSection file={file} baseMode={baseMode()} />}
        </For>
      </Show>
    </div>
  );
};
