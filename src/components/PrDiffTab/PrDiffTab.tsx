import { Component, createSignal, For, Show } from "solid-js";
import { parseDiffFiles, type DiffFileSection } from "../ui/DiffViewer";
import { t } from "../../i18n";
import s from "./PrDiffTab.module.css";

export interface PrDiffTabProps {
  prNumber: number;
  prTitle: string;
  diff: string;
}

/** Parse hunk header "@@ -a,b +c,d @@" to extract the starting line numbers */
function parseHunkStart(hunk: string): { oldStart: number; newStart: number } {
  const match = hunk.match(/@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/);
  return match ? { oldStart: parseInt(match[1]), newStart: parseInt(match[2]) } : { oldStart: 0, newStart: 0 };
}

const FileSection: Component<{ file: DiffFileSection }> = (props) => {
  const [collapsed, setCollapsed] = createSignal(false);

  /** Build line numbers by walking the diff lines */
  const numberedLines = () => {
    const result: Array<{ oldNum: string; newNum: string; content: string; type: string }> = [];
    let oldLine = 0;
    let newLine = 0;

    for (const line of props.file.lines) {
      if (line.type === "header" || line.content.startsWith("---") || line.content.startsWith("+++") ||
          line.content.startsWith("index ") || line.content.startsWith("similarity") ||
          line.content.startsWith("rename ") || line.content.startsWith("new file") ||
          line.content.startsWith("deleted file")) {
        // Skip meta lines from numbered display
        continue;
      }
      if (line.type === "hunk") {
        const { oldStart, newStart } = parseHunkStart(line.content);
        oldLine = oldStart;
        newLine = newStart;
        result.push({ oldNum: "", newNum: "", content: line.content, type: "hunk" });
        continue;
      }
      if (line.type === "addition") {
        result.push({ oldNum: "", newNum: String(newLine), content: line.content, type: "addition" });
        newLine++;
      } else if (line.type === "deletion") {
        result.push({ oldNum: String(oldLine), newNum: "", content: line.content, type: "deletion" });
        oldLine++;
      } else {
        result.push({ oldNum: String(oldLine), newNum: String(newLine), content: line.content, type: "context" });
        oldLine++;
        newLine++;
      }
    }
    return result;
  };

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
        <table class={s.diffTable}>
          <tbody>
            <For each={numberedLines()}>
              {(line) => (
                <tr class={
                  line.type === "addition" ? s.lineAddition :
                  line.type === "deletion" ? s.lineDeletion :
                  line.type === "hunk" ? s.lineHunk :
                  s.lineContext
                }>
                  <td class={s.lineNum}>{line.oldNum}</td>
                  <td class={s.lineNum}>{line.newNum}</td>
                  <td class={s.lineContent}>{line.content}</td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </Show>
    </div>
  );
};

export const PrDiffTab: Component<PrDiffTabProps> = (props) => {
  const files = () => parseDiffFiles(props.diff);
  const totalAdd = () => files().reduce((sum, f) => sum + f.additions, 0);
  const totalDel = () => files().reduce((sum, f) => sum + f.deletions, 0);

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
      </div>
      <Show when={files().length > 0} fallback={<div class={s.emptyState}>{t("prDiff.empty", "No changes")}</div>}>
        <For each={files()}>
          {(file) => <FileSection file={file} />}
        </For>
      </Show>
    </div>
  );
};
