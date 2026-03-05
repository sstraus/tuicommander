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

/** A single file section within a multi-file diff */
export interface DiffFileSection {
  path: string;
  additions: number;
  deletions: number;
  lines: DiffLine[];
}

/** Split a multi-file unified diff into per-file sections with stats */
export function parseDiffFiles(diff: string): DiffFileSection[] {
  if (!diff.trim()) return [];

  const rawLines = diff.split("\n");
  const sections: DiffFileSection[] = [];
  let current: { path: string; startIdx: number } | null = null;

  const flush = (endIdx: number) => {
    if (!current) return;
    const sectionLines = rawLines.slice(current.startIdx, endIdx);
    const parsed = sectionLines.map((line) => {
      let type: LineType = "context";
      if (line.startsWith("diff --git")) type = "header";
      else if (line.startsWith("@@")) type = "hunk";
      else if (line.startsWith("+") && !line.startsWith("+++")) type = "addition";
      else if (line.startsWith("-") && !line.startsWith("---")) type = "deletion";
      return { content: line, type };
    });
    let additions = 0;
    let deletions = 0;
    for (const l of parsed) {
      if (l.type === "addition") additions++;
      else if (l.type === "deletion") deletions++;
    }
    sections.push({ path: current.path, additions, deletions, lines: parsed });
  };

  for (let i = 0; i < rawLines.length; i++) {
    const line = rawLines[i];
    if (line.startsWith("diff --git ")) {
      flush(i);
      // Extract path from "diff --git a/path b/path" — use the b/ side
      const match = line.match(/^diff --git a\/.+ b\/(.+)$/);
      current = { path: match ? match[1] : line, startIdx: i };
    }
  }
  flush(rawLines.length);

  return sections;
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
