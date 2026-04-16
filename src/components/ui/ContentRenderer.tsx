import { Component, createEffect, createMemo, Show } from "solid-js";
import { convertFileSrc } from "@tauri-apps/api/core";
import { marked } from "marked";
import DOMPurify from "dompurify";
import { stripAnsi } from "../../utils/stripAnsi";
import { appLogger } from "../../stores/appLogger";
import { applyTweakHighlights } from "../../utils/tweakComments";

/** File extensions that can be previewed inline when clicked as relative links.
 *  .md files open in a markdown tab; all others open in the file preview tab. */
const PREVIEWABLE_RE = /\.(md|pdf|html?|png|jpe?g|gif|webp|svg|avif|ico|bmp|mp4|webm|mov|ogg|mp3|wav|flac|aac|m4a|txt|json|csv|log|xml|ya?ml|toml|ini|cfg|conf)$/i;

export interface ContentRendererProps {
  content: string;
  emptyMessage?: string;
  /** Called when a relative file link is clicked (href passed as argument) */
  onLinkClick?: (href: string) => void;
  /** Called when a GFM task-list checkbox is clicked (source line number, new mark: " ", "x", or "~") */
  onCheckboxToggle?: (sourceLine: number, mark: " " | "x" | "~") => void;
  /** Absolute directory path of the source file, used to resolve relative image src attributes */
  baseDir?: string;
  /** Ref callback to expose the rendered content container for search */
  contentRef?: (el: HTMLDivElement) => void;
  /** Override the root font size in pixels (children use em, so everything scales). */
  fontSize?: number;
}

/** Strip event handler attributes (on*) as defense-in-depth before DOMPurify */
export function stripEventHandlers(html: string): string {
  return html.replace(/\s+on\w+\s*=\s*("[^"]*"|'[^']*'|[^\s>]*)/gi, "");
}

// Configure marked for safe rendering
marked.setOptions({
  gfm: true, // GitHub Flavored Markdown
  breaks: true, // Convert \n to <br>
});

const TILDE_SENTINEL = "data-checkbox-indeterminate";

/**
 * Pre-process `- [~]` (non-standard "in-progress" checkbox) into `- [ ]`
 * so marked renders it as a task-list item. We track which source lines
 * had tilde in a separate set returned alongside the cleaned source.
 */
function preprocessTildeCheckboxes(source: string): { cleaned: string; tildeLines: Set<number> } {
  const lines = source.split("\n");
  const tildeLines = new Set<number>();
  for (let i = 0; i < lines.length; i++) {
    const m = /^(\s*[-*+]\s+)\[~\](.*)$/.exec(lines[i]);
    if (!m) continue;
    tildeLines.add(i);
    lines[i] = `${m[1]}[ ]${m[2]}`;
  }
  return { cleaned: lines.join("\n"), tildeLines };
}

/**
 * Build a mapping from sequential checkbox index (as rendered by marked)
 * to source line number. Scans the raw source and returns an array where
 * entry[domIndex] = sourceLine. Skips lines inside fenced code blocks.
 */
function buildCheckboxLineMap(source: string): number[] {
  const lines = source.split("\n");
  const map: number[] = [];
  let inFence = false;
  for (let i = 0; i < lines.length; i++) {
    if (/^\s*(`{3,}|~{3,})/.test(lines[i])) {
      inFence = !inFence;
      continue;
    }
    if (inFence) continue;
    if (/^\s*[-*+]\s+\[([ xX~])\]/.test(lines[i])) {
      map.push(i);
    }
  }
  return map;
}

export const ContentRenderer: Component<ContentRendererProps> = (props) => {
  // Memoize processed markdown to avoid re-parsing on every render
  const processedContent = createMemo(() => {
    const raw = stripAnsi(props.content ?? "");
    try {
      // 1. Convert [~] to [ ] so marked renders them as standard GFM task-list items.
      //    Track which source lines had tilde for indeterminate styling later.
      const { cleaned, tildeLines } = preprocessTildeCheckboxes(raw);

      // 2. Build source-line map BEFORE any transforms: domIndex → sourceLine.
      //    This must use the tilde-cleaned source (same checkbox count as marked sees).
      const lineMap = buildCheckboxLineMap(cleaned);

      // 3. Convert tweak markers to highlight spans, then parse markdown.
      const withHighlights = applyTweakHighlights(cleaned);
      let html = marked.parse(withHighlights, { async: false }) as string;

      // 4. Rewrite relative image src attributes to loadable asset:// URLs.
      const baseDir = props.baseDir;
      if (baseDir) {
        html = html.replace(
          /(<img\b[^>]*\ssrc=")(?!https?:\/\/|data:|asset:\/\/)([^"]+)"/gi,
          (_, prefix, relativePath) =>
            `${prefix}${convertFileSrc(`${baseDir}/${relativePath}`)}"`,
        );
      }

      // 5. Make GFM task-list checkboxes interactive and inject source-line metadata.
      //    Sequential checkbox index in the HTML maps to lineMap[domIndex].
      let cbIndex = 0;
      html = html.replace(/<input\b[^>]*type="checkbox"[^>]*>/gi, (match) => {
        const idx = cbIndex++;
        const sourceLine = lineMap[idx];
        // Remove disabled attribute
        let out = match.includes("disabled")
          ? match.replace(/\s*disabled(?:="")?/i, "")
          : match;
        // Inject data-source-line for the click handler
        if (sourceLine !== undefined) {
          out = out.replace(/>$/, ` data-source-line="${sourceLine}">`);
          // Mark tilde checkboxes for indeterminate styling
          if (tildeLines.has(sourceLine)) {
            out = out.replace(/>$/, ` ${TILDE_SENTINEL}>`);
          }
        }
        return out;
      });

      return DOMPurify.sanitize(stripEventHandlers(html), {
        ADD_ATTR: ["data-tweak-id", "data-tweak-at", "data-tweak-comment", "data-source-line", TILDE_SENTINEL],
      });
    } catch (err) {
      appLogger.error("app", "Markdown parsing error", err);
      return `<pre>${raw}</pre>`;
    }
  });

  const isEmpty = createMemo(() => (props.content ?? "").trim() === "");

  const handleClick = (e: MouseEvent) => {
    const target = e.target as HTMLElement;

    // GFM task-list checkbox toggle (tri-state: [ ] → [x] → [~] → [ ])
    if (target instanceof HTMLInputElement && target.type === "checkbox" && target.dataset.sourceLine != null) {
      e.preventDefault();
      const line = parseInt(target.dataset.sourceLine, 10);
      const isIndeterminate = target.hasAttribute(TILDE_SENTINEL);
      // NOTE: by the time the click handler fires, the browser has already
      // toggled `checked`. So `target.checked` reflects the POST-click value.
      // The original state was `!target.checked` (for non-indeterminate boxes).
      const wasChecked = !target.checked;

      // Determine next state in the cycle
      let nextMark: " " | "x" | "~";
      if (isIndeterminate) {
        nextMark = " "; // [~] → [ ]
      } else if (wasChecked) {
        nextMark = "~"; // [x] → [~]
      } else {
        nextMark = "x"; // [ ] → [x]
      }

      props.onCheckboxToggle?.(line, nextMark);
      return;
    }

    // Relative file link navigation
    if (!props.onLinkClick) return;
    const anchor = target.closest("a");
    if (!anchor) return;
    const href = anchor.getAttribute("href");
    if (href && !href.startsWith("http") && PREVIEWABLE_RE.test(href)) {
      e.preventDefault();
      props.onLinkClick(href);
    }
  };

  let containerRef: HTMLDivElement | undefined;

  // After render, set indeterminate property on [~] checkboxes (not settable via HTML attribute)
  createEffect(() => {
    processedContent(); // subscribe to re-renders
    if (!containerRef) return;
    requestAnimationFrame(() => {
      containerRef!.querySelectorAll<HTMLInputElement>(`input[${TILDE_SENTINEL}]`).forEach((cb) => {
        cb.indeterminate = true;
      });
    });
  });

  return (
    <div
      id="markdown-content"
      ref={(el) => { containerRef = el; props.contentRef?.(el); }}
      onClick={handleClick}
      style={props.fontSize !== undefined ? { "font-size": `${props.fontSize}px` } : undefined}
    >
      <Show
        when={!isEmpty()}
        fallback={<p>{props.emptyMessage || "No content"}</p>}
      >
        {/* eslint-disable-next-line solid/no-innerhtml */}
        <div innerHTML={processedContent()} />
      </Show>
    </div>
  );
};
