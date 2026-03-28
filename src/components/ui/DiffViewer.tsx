import { Component, createEffect, createMemo, createSignal, on, Show } from "solid-js";
import { DiffView, DiffModeEnum } from "@git-diff-view/solid";
import { DiffFile } from "@git-diff-view/core";
import "@git-diff-view/solid/styles/diff-view.css";
import type { DiffViewMode } from "../../stores/ui";

// ---------------------------------------------------------------------------
// Legacy types & parsers (kept for backward compatibility with PrDiffTab, tests)
// ---------------------------------------------------------------------------

type LineType = "header" | "hunk" | "addition" | "deletion" | "context";

interface DiffLine {
  content: string;
  type: LineType;
}

/** Classify a single unified-diff line by its prefix. */
export function classifyLine(line: string): LineType {
  if (line.startsWith("diff --git")) return "header";
  if (line.startsWith("@@")) return "hunk";
  if (line.startsWith("+") && !line.startsWith("+++")) return "addition";
  if (line.startsWith("-") && !line.startsWith("---")) return "deletion";
  return "context";
}

export function parseDiff(diff: string): DiffLine[] {
  const lines = diff.split("\n");
  return lines.map((line) => ({ content: line, type: classifyLine(line) }));
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
    const parsed = sectionLines.map((line) => ({
      content: line,
      type: classifyLine(line),
    }));
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

// ---------------------------------------------------------------------------
// Extract file name from unified diff header
// ---------------------------------------------------------------------------

function extractFileName(diff: string): string {
  const match = diff.match(/^diff --git a\/.+ b\/(.+)$/m);
  return match ? match[1] : "";
}

// ---------------------------------------------------------------------------
// DiffViewer component — powered by @git-diff-view/solid
// ---------------------------------------------------------------------------

export interface DiffViewerProps {
  diff: string;
  emptyMessage?: string;
  /** Display mode: "split" for side-by-side, "unified" for inline */
  mode?: DiffViewMode;
  /** Callback to expose the content DOM element for search */
  contentRef?: (el: HTMLElement) => void;
}

/** Convert our mode string to the library's enum */
function toModeEnum(mode: DiffViewMode | undefined): DiffModeEnum {
  return mode === "unified" ? DiffModeEnum.Unified : DiffModeEnum.Split;
}

export const DiffViewer: Component<DiffViewerProps> = (props) => {
  const isEmpty = createMemo(() => props.diff.trim() === "");

  // Build a DiffFile instance from the raw unified diff string.
  // DiffFile.createInstance expects hunks as an array of diff strings.
  const [diffFile, setDiffFile] = createSignal<DiffFile | undefined>(undefined);

  createEffect(on(
    () => props.diff,
    (diff) => {
      if (!diff.trim()) {
        setDiffFile(undefined);
        return;
      }
      const fileName = extractFileName(diff);
      const df = DiffFile.createInstance({
        oldFile: { fileName },
        newFile: { fileName },
        hunks: [diff],
      });
      df.init();
      df.buildSplitDiffLines();
      df.buildUnifiedDiffLines();
      setDiffFile(df);
    },
  ));

  return (
    <div id="diff-content" ref={(el) => props.contentRef?.(el)}>
      <Show
        when={!isEmpty() && diffFile()}
        fallback={
          <div class="diff-empty">{props.emptyMessage || "No changes"}</div>
        }
      >
        <DiffView
          diffFile={diffFile()}
          diffViewMode={toModeEnum(props.mode)}
          diffViewTheme="dark"
          diffViewWrap={false}
          diffViewFontSize={13}
        />
      </Show>
    </div>
  );
};
