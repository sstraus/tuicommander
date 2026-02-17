import { Component, createMemo, Show } from "solid-js";
import { marked } from "marked";
import DOMPurify from "dompurify";

export interface MarkdownRendererProps {
  content: string;
  emptyMessage?: string;
  /** Called when a relative .md link is clicked (href passed as argument) */
  onLinkClick?: (href: string) => void;
}

/** Strip ANSI escape codes from text */
export function stripAnsi(text: string): string {
  return text.replace(/\x1B\[[0-9;]*[a-zA-Z]/g, "");
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

export const MarkdownRenderer: Component<MarkdownRendererProps> = (props) => {
  // Memoize processed markdown to avoid re-parsing on every render
  const processedContent = createMemo(() => {
    const cleaned = stripAnsi(props.content);
    try {
      const html = marked.parse(cleaned, { async: false }) as string;
      return DOMPurify.sanitize(stripEventHandlers(html));
    } catch (err) {
      console.error("Markdown parsing error:", err);
      return `<pre>${cleaned}</pre>`;
    }
  });

  const isEmpty = createMemo(() => props.content.trim() === "");

  const handleClick = (e: MouseEvent) => {
    if (!props.onLinkClick) return;
    const target = e.target as HTMLElement;
    const anchor = target.closest("a");
    if (!anchor) return;
    const href = anchor.getAttribute("href");
    if (href && href.endsWith(".md") && !href.startsWith("http")) {
      e.preventDefault();
      props.onLinkClick(href);
    }
  };

  return (
    <div id="markdown-content" onClick={handleClick}>
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
