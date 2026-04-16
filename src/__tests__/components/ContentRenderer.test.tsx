import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@solidjs/testing-library";
import { ContentRenderer, stripEventHandlers } from "../../components/ui/ContentRenderer";
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

describe("ContentRenderer", () => {
  it("renders markdown content as HTML", () => {
    const { container } = render(() => (
      <ContentRenderer content="# Hello" />
    ));
    const content = container.querySelector("#markdown-content");
    expect(content).not.toBeNull();
    expect(content!.innerHTML).toContain("<h1");
    expect(content!.innerHTML).toContain("Hello");
  });

  it("shows empty message when content is empty", () => {
    const { container } = render(() => (
      <ContentRenderer content="" />
    ));
    const p = container.querySelector("#markdown-content p");
    expect(p).not.toBeNull();
    expect(p!.textContent).toBe("No content");
  });

  it("shows custom empty message", () => {
    const { container } = render(() => (
      <ContentRenderer content="  " emptyMessage="Nothing here" />
    ));
    const p = container.querySelector("#markdown-content p");
    expect(p!.textContent).toBe("Nothing here");
  });

  it("sanitizes HTML to prevent XSS", () => {
    const malicious = '# Title\n\n<script>alert("xss")</script>\n\n<img src=x onerror="alert(1)">';
    const { container } = render(() => (
      <ContentRenderer content={malicious} />
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
      <ContentRenderer content={raw} />
    ));
    const content = container.querySelector("#markdown-content");
    expect(content!.textContent).toContain("Red Title");
    // Verify no ESC character in textContent
    expect(content!.textContent).not.toContain("\x1B");
  });

  it("calls onLinkClick with href when .md link is clicked", () => {
    const onLinkClick = vi.fn();
    const { container } = render(() => (
      <ContentRenderer
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
      <ContentRenderer
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
      <ContentRenderer content="See [readme](docs/README.md) for details" />
    ));
    const link = container.querySelector('a[href="docs/README.md"]') as HTMLAnchorElement;
    expect(link).not.toBeNull();
    // Should not throw when clicked without handler
    fireEvent.click(link);
  });

  describe("GFM task-list checkboxes", () => {
    it("renders checkboxes as enabled input elements with data-source-line", () => {
      const md = "- [ ] First\n- [x] Second\n- [ ] Third";
      const { container } = render(() => <ContentRenderer content={md} />);
      const checkboxes = container.querySelectorAll<HTMLInputElement>('input[type="checkbox"]');
      expect(checkboxes.length).toBe(3);
      // All should be enabled (disabled removed)
      checkboxes.forEach((cb) => expect(cb.disabled).toBe(false));
      // data-source-line should map to correct lines
      expect(checkboxes[0].dataset.sourceLine).toBe("0");
      expect(checkboxes[1].dataset.sourceLine).toBe("1");
      expect(checkboxes[2].dataset.sourceLine).toBe("2");
    });

    it("marks checked boxes correctly", () => {
      const md = "- [ ] Unchecked\n- [x] Checked";
      const { container } = render(() => <ContentRenderer content={md} />);
      const checkboxes = container.querySelectorAll<HTMLInputElement>('input[type="checkbox"]');
      expect(checkboxes[0].checked).toBe(false);
      expect(checkboxes[1].checked).toBe(true);
    });

    it("skips checkboxes inside fenced code blocks for line mapping", () => {
      const md = [
        "- [ ] Real task",      // line 0 → sourceLine 0
        "```",                   // line 1
        "- [ ] Code example",   // line 2 — inside fence, not rendered as checkbox
        "```",                   // line 3
        "- [ ] Another task",   // line 4 → sourceLine 4
      ].join("\n");
      const { container } = render(() => <ContentRenderer content={md} />);
      const checkboxes = container.querySelectorAll<HTMLInputElement>('input[type="checkbox"]');
      expect(checkboxes.length).toBe(2);
      expect(checkboxes[0].dataset.sourceLine).toBe("0");
      expect(checkboxes[1].dataset.sourceLine).toBe("4");
    });

    it("calls onCheckboxToggle with source line and next mark on click", () => {
      const onToggle = vi.fn();
      const md = "- [ ] First\n- [x] Second";
      const { container } = render(() => (
        <ContentRenderer content={md} onCheckboxToggle={onToggle} />
      ));
      const checkboxes = container.querySelectorAll<HTMLInputElement>('input[type="checkbox"]');
      // Click unchecked → should request "x"
      fireEvent.click(checkboxes[0]);
      expect(onToggle).toHaveBeenCalledWith(0, "x");
    });

    it("renders [~] as indeterminate checkbox with sentinel attribute", () => {
      const md = "- [ ] Normal\n- [~] In progress";
      const { container } = render(() => <ContentRenderer content={md} />);
      const checkboxes = container.querySelectorAll<HTMLInputElement>('input[type="checkbox"]');
      expect(checkboxes.length).toBe(2);
      // First should not have the sentinel
      expect(checkboxes[0].hasAttribute("data-checkbox-indeterminate")).toBe(false);
      // Second (tilde) should have the sentinel
      expect(checkboxes[1].hasAttribute("data-checkbox-indeterminate")).toBe(true);
    });

    it("handles mixed content: headings, text, and checkboxes", () => {
      const md = "# Plan\n\nSome text.\n\n- [ ] Task A\n- [x] Task B\n\nMore text.";
      const { container } = render(() => <ContentRenderer content={md} />);
      const checkboxes = container.querySelectorAll<HTMLInputElement>('input[type="checkbox"]');
      expect(checkboxes.length).toBe(2);
      expect(checkboxes[0].dataset.sourceLine).toBe("4");
      expect(checkboxes[1].dataset.sourceLine).toBe("5");
    });

    it("handles nested checkboxes", () => {
      const md = "- [ ] Parent\n  - [ ] Child\n  - [x] Done child";
      const { container } = render(() => <ContentRenderer content={md} />);
      const checkboxes = container.querySelectorAll<HTMLInputElement>('input[type="checkbox"]');
      expect(checkboxes.length).toBe(3);
      expect(checkboxes[0].dataset.sourceLine).toBe("0");
      expect(checkboxes[1].dataset.sourceLine).toBe("1");
      expect(checkboxes[2].dataset.sourceLine).toBe("2");
    });
  });
});
