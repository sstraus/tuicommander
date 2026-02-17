import { Component, createMemo, For, Show } from "solid-js";

export interface DiffViewerProps {
  diff: string;
  emptyMessage?: string;
}

type LineType = "header" | "hunk" | "addition" | "deletion" | "context";

interface DiffLine {
  content: string;
  type: LineType;
}

export function parseDiff(diff: string): DiffLine[] {
  const lines = diff.split("\n");
  return lines.map((line) => {
    let type: LineType = "context";

    if (line.startsWith("diff --git")) {
      type = "header";
    } else if (line.startsWith("@@")) {
      type = "hunk";
    } else if (line.startsWith("+") && !line.startsWith("+++")) {
      type = "addition";
    } else if (line.startsWith("-") && !line.startsWith("---")) {
      type = "deletion";
    }

    return { content: line, type };
  });
}

export const DiffViewer: Component<DiffViewerProps> = (props) => {
  // Memoize parsed diff to avoid re-parsing on every render
  const lines = createMemo(() => parseDiff(props.diff));
  const isEmpty = createMemo(() => props.diff.trim() === "");

  return (
    <div id="diff-content">
      <Show
        when={!isEmpty()}
        fallback={
          <div class="diff-empty">{props.emptyMessage || "No changes"}</div>
        }
      >
        <For each={lines()}>
          {(line) => (
            <div class={`diff-line ${line.type}`}>{line.content}</div>
          )}
        </For>
      </Show>
    </div>
  );
};
