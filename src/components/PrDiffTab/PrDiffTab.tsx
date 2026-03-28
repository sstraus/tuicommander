import { Component, createSignal, For, Show } from "solid-js";
import { DiffViewer, parseDiffFiles, type DiffFileSection } from "../ui/DiffViewer";
import { uiStore, type DiffViewMode } from "../../stores/ui";
import { t } from "../../i18n";
import { cx } from "../../utils";
import s from "./PrDiffTab.module.css";

export interface PrDiffTabProps {
  prNumber: number;
  prTitle: string;
  diff: string;
}

/** Reconstruct the raw diff string for a single file section */
function sectionToRawDiff(section: DiffFileSection): string {
  return section.lines.map((l) => l.content).join("\n");
}

const FileSection: Component<{ file: DiffFileSection; mode: DiffViewMode }> = (props) => {
  const [collapsed, setCollapsed] = createSignal(false);

  return (
    <div class={s.fileSection}>
      <div class={s.fileHeader} onClick={() => setCollapsed(!collapsed())}>
        <svg
          class={`${s.chevron} ${collapsed() ? s.chevronCollapsed : ""}`}
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
          <DiffViewer
            diff={sectionToRawDiff(props.file)}
            mode={props.mode}
          />
        </div>
      </Show>
    </div>
  );
};

export const PrDiffTab: Component<PrDiffTabProps> = (props) => {
  const files = () => parseDiffFiles(props.diff);
  const totalAdd = () => files().reduce((sum, f) => sum + f.additions, 0);
  const totalDel = () => files().reduce((sum, f) => sum + f.deletions, 0);

  const mode = (): DiffViewMode => uiStore.state.diffViewMode;

  return (
    <div class={s.container}>
      <div class={s.header}>
        <span class={s.headerTitle}>
          #{props.prNumber} {props.prTitle}
        </span>
        <span class={s.headerStats}>
          {files().length} {t("prDiff.files", "files")}
          {" "}
          <span class={s.statAdd}>+{totalAdd()}</span>
          {" "}
          <span class={s.statDel}>-{totalDel()}</span>
        </span>
        <div class={s.modeToggle}>
          <button
            class={cx(s.modeBtn, mode() === "split" && s.modeBtnActive)}
            onClick={() => uiStore.setDiffViewMode("split")}
            title={t("diffTab.splitView", "Side-by-side")}
          >
            <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
              <path d="M1 2h6v12H1V2zm8 0h6v12H9V2zM2 3v10h4V3H2zm8 0v10h4V3h-4z" />
            </svg>
          </button>
          <button
            class={cx(s.modeBtn, mode() === "unified" && s.modeBtnActive)}
            onClick={() => uiStore.setDiffViewMode("unified")}
            title={t("diffTab.unifiedView", "Inline")}
          >
            <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
              <path d="M1 2h14v12H1V2zm1 1v10h12V3H2z" />
            </svg>
          </button>
        </div>
      </div>
      <Show when={files().length > 0} fallback={<div class={s.emptyState}>{t("prDiff.empty", "No changes")}</div>}>
        <For each={files()}>
          {(file) => <FileSection file={file} mode={mode()} />}
        </For>
      </Show>
    </div>
  );
};
