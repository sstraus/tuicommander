import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@solidjs/testing-library";
import { MarkdownRenderer, stripEventHandlers } from "../../components/ui/MarkdownRenderer";
import { stripAnsi } from "../../utils/stripAnsi";

describe("stripAnsi", () => {
  it("strips ANSI escape codes", () => {
    expect(stripAnsi("\x1B[31mred\x1B[0m")).toBe("red");
  });

  it("returns clean text unchanged", () => {
    expect(stripAnsi("no ansi here")).toBe("no ansi here");
  });

  it("strips multiple ANSI codes", () => {
    expect(stripAnsi("\x1B[1m\x1B[32mbold green\x1B[0m")).toBe("bold green");
  });
});

describe("stripEventHandlers", () => {
  it("strips onerror attributes", () => {
    expect(stripEventHandlers('<img src="x" onerror="alert(1)">')).toBe('<img src="x">');
  });

  it("strips onclick attributes", () => {
    expect(stripEventHandlers('<div onclick="alert(1)">hi</div>')).toBe("<div>hi</div>");
  });

  it("strips single-quoted event handlers", () => {
    expect(stripEventHandlers("<img onload='fetch(\"evil\")'>")).toBe("<img>");
  });

  it("preserves non-event attributes", () => {
    expect(stripEventHandlers('<a href="url" class="link">text</a>')).toBe(
      '<a href="url" class="link">text</a>'
    );
  });
});

describe("MarkdownRenderer", () => {
  it("renders markdown content as HTML", () => {
    const { container } = render(() => (
      <MarkdownRenderer content="# Hello" />
    ));
    const content = container.querySelector("#markdown-content");
    expect(content).not.toBeNull();
    expect(content!.innerHTML).toContain("<h1");
    expect(content!.innerHTML).toContain("Hello");
  });

  it("shows empty message when content is empty", () => {
    const { container } = render(() => (
      <MarkdownRenderer content="" />
    ));
    const p = container.querySelector("#markdown-content p");
    expect(p).not.toBeNull();
    expect(p!.textContent).toBe("No content");
  });

  it("shows custom empty message", () => {
    const { container } = render(() => (
      <MarkdownRenderer content="  " emptyMessage="Nothing here" />
    ));
    const p = container.querySelector("#markdown-content p");
    expect(p!.textContent).toBe("Nothing here");
  });

  it("sanitizes HTML to prevent XSS", () => {
    const malicious = '# Title\n\n<script>alert("xss")</script>\n\n<img src=x onerror="alert(1)">';
    const { container } = render(() => (
      <MarkdownRenderer content={malicious} />
    ));
    const content = container.querySelector("#markdown-content");
    // Script tags must be stripped
    expect(content!.innerHTML).not.toContain("<script");
    // Event handlers must be stripped
    expect(content!.innerHTML).not.toContain("onerror");
    // Safe content should still render
    expect(content!.textContent).toContain("Title");
  });

  it("strips ANSI codes before rendering markdown", () => {
    const raw = "\x1B[31m# Red Title\x1B[0m";
    const { container } = render(() => (
      <MarkdownRenderer content={raw} />
    ));
    const content = container.querySelector("#markdown-content");
    expect(content!.textContent).toContain("Red Title");
    // Verify no ESC character in textContent
    expect(content!.textContent).not.toContain("\x1B");
  });

  it("calls onLinkClick with href when .md link is clicked", () => {
    const onLinkClick = vi.fn();
    const { container } = render(() => (
      <MarkdownRenderer
        content="See [readme](docs/README.md) for details"
        onLinkClick={onLinkClick}
      />
    ));
    const link = container.querySelector('a[href="docs/README.md"]') as HTMLAnchorElement;
    expect(link).not.toBeNull();
    fireEvent.click(link);
    expect(onLinkClick).toHaveBeenCalledWith("docs/README.md");
  });

  it("does not call onLinkClick for non-.md links", () => {
    const onLinkClick = vi.fn();
    const { container } = render(() => (
      <MarkdownRenderer
        content="See [site](https://example.com) for details"
        onLinkClick={onLinkClick}
      />
    ));
    const link = container.querySelector('a[href="https://example.com"]') as HTMLAnchorElement;
    expect(link).not.toBeNull();
    fireEvent.click(link);
    expect(onLinkClick).not.toHaveBeenCalled();
  });

  it("does not intercept .md links when onLinkClick is not provided", () => {
    const { container } = render(() => (
      <MarkdownRenderer content="See [readme](docs/README.md) for details" />
    ));
    const link = container.querySelector('a[href="docs/README.md"]') as HTMLAnchorElement;
    expect(link).not.toBeNull();
    // Should not throw when clicked without handler
    fireEvent.click(link);
  });
});
