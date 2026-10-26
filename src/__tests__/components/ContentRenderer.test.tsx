// @vitest-environment jsdom

import { fireEvent, render } from "@solidjs/testing-library";
import { marked } from "marked";
import { createSignal } from "solid-js";
import { describe, expect, it, vi } from "vitest";
import { applyCommentBlockMetadata, ContentRenderer, stripEventHandlers } from "../../components/ui/ContentRenderer";
import { appLogger } from "../../stores/appLogger";
import { handleOpenUrl } from "../../utils/openUrl";
import { stripAnsi } from "../../utils/stripAnsi";
import { findTweakCommentBlocks, insertTweakBlockComment } from "../../utils/tweakComments";

vi.mock("../../utils/openUrl", () => ({ handleOpenUrl: vi.fn() }));

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
		expect(stripEventHandlers('<a href="url" class="link">text</a>')).toBe('<a href="url" class="link">text</a>');
	});
});

describe("ContentRenderer", () => {
	// Catches: shared copy actions duplicate on streaming updates, copy their label, or outlive their callback.
	it.each([false, true])("keeps one live code-copy action across updates (incremental=%s)", async (incremental) => {
		const [content, setContent] = createSignal("```sh\nprintf '%s' geometry\n```\n");
		const [enabled, setEnabled] = createSignal(true);
		const copy = vi.fn();
		const { container } = render(() => (
			<ContentRenderer content={content()} incremental={incremental} onCodeCopy={enabled() ? copy : undefined} />
		));
		await Promise.resolve();
		setContent((text) => `${text}\nA later streaming paragraph.\n`);
		await Promise.resolve();
		const buttons = container.querySelectorAll<HTMLButtonElement>("button[data-copy-code]");
		expect(buttons).toHaveLength(1);
		fireEvent.click(buttons[0]);
		expect(copy).toHaveBeenCalledExactlyOnceWith("printf '%s' geometry");
		setEnabled(false);
		await Promise.resolve();
		expect(container.querySelector("button[data-copy-code]")).toBeNull();
	});

	it("annotates rendered markdown blocks with exact source ranges when comments are enabled", async () => {
		const source = "# Heading\n\nA **formatted** [paragraph](SPEC.md).";
		const { container } = render(() => <ContentRenderer content={source} commentableBlocks />);
		await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));

		const heading = container.querySelector("h1") as HTMLElement;
		const paragraph = container.querySelector("p") as HTMLElement;
		expect(source.slice(Number(heading.dataset.commentSourceStart), Number(heading.dataset.commentSourceEnd))).toBe(
			"# Heading",
		);
		expect(source.slice(Number(paragraph.dataset.commentSourceStart), Number(paragraph.dataset.commentSourceEnd))).toBe(
			"A **formatted** [paragraph](SPEC.md).",
		);
	});

	// Catches: assigning the list's range to every item or omitting nested/task/ordered items.
	it("gives each plain, task, nested, and ordered item its own source target", async () => {
		const source = "- parent\n  - nested\n- [ ] task\n- sibling\n\n1. ordered\n";
		const { container } = render(() => <ContentRenderer content={source} commentableBlocks />);
		await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
		const items = Array.from(container.querySelectorAll<HTMLElement>("li[data-comment-source-start]"));
		expect(items).toHaveLength(5);
		expect(
			items.map((item) => source.slice(Number(item.dataset.commentSourceStart), Number(item.dataset.commentSourceEnd))),
		).toEqual(["- parent", "  - nested", "- [ ] task", "- sibling", "1. ordered"]);
		expect(container.querySelector("ul[data-comment-source-start]")).toBeNull();
		expect(container.querySelector("ol[data-comment-source-start]")).toBeNull();
	});

	it("refreshes comment source ranges and highlights when a saved file replaces the rendered source", async () => {
		const initial = "1. First question\n2. Second question\n";
		const [source, setSource] = createSignal(initial);
		const first = findTweakCommentBlocks(initial).find((block) => block.tag === "LI")!;
		const { container } = render(() => <ContentRenderer content={source()} commentableBlocks />);
		await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
		setSource(
			insertTweakBlockComment(
				initial,
				{
					id: "c_first",
					highlighted: initial.slice(first.start, first.end),
					comment: "Yes",
					createdAt: "2026-09-27T12:00:00.000Z",
				},
				first,
			),
		);
		await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
		const items = Array.from(container.querySelectorAll<HTMLElement>("li"));
		expect(Number(items[0].dataset.commentSourceStart)).toBeGreaterThan(initial.length);
		expect(items[0].classList.contains("tweak-block-highlight")).toBe(true);
	});

	it("keeps structural markdown intact and highlights an existing block comment", async () => {
		const source = "# Heading\n\nParagraph.";
		const heading = findTweakCommentBlocks(source)[0];
		const commented = insertTweakBlockComment(
			source,
			{
				id: "c_heading",
				highlighted: source.slice(heading.start, heading.end),
				comment: "Clarify this heading",
				createdAt: "2026-09-22T12:00:00.000Z",
			},
			heading,
		);

		const { container } = render(() => <ContentRenderer content={commented} commentableBlocks />);
		await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));

		const renderedHeading = container.querySelector("h1");
		expect(renderedHeading?.textContent).toBe("Heading");
		expect(renderedHeading?.classList.contains("tweak-block-highlight")).toBe(true);
		expect(renderedHeading?.getAttribute("data-tweak-id")).toBe("c_heading");
	});

	it("annotates a CRLF document with ranges that slice exactly the rendered block", async () => {
		const source = "# Heading\r\n\r\nFirst line\r\nsecond line\r\n\r\nLast.\r\n";
		const { container } = render(() => <ContentRenderer content={source} commentableBlocks />);
		await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));

		const ranges = Array.from(container.querySelectorAll<HTMLElement>("[data-comment-source-start]")).map((el) =>
			source.slice(Number(el.dataset.commentSourceStart), Number(el.dataset.commentSourceEnd)),
		);
		expect(ranges).toEqual(["# Heading", "First line\r\nsecond line", "Last."]);
	});

	// Metadata runs after every render of a file tab; it must lex the document once.
	it("lexes the source once when attaching block metadata", () => {
		const source = "<!--tweak:block:c_h @2026-09-23T08:00:00.000Z\nnote-->\n# Heading\n\nParagraph.";
		const container = document.createElement("div");
		container.innerHTML = "<div><h1>Heading</h1><p>Paragraph.</p></div>";
		const lexer = vi.spyOn(marked, "lexer");
		try {
			applyCommentBlockMetadata(container, source);
			expect(lexer).toHaveBeenCalledTimes(1);
		} finally {
			lexer.mockRestore();
		}
		expect(container.querySelector("h1")?.dataset.tweakId).toBe("c_h");
	});

	it("renders markdown content as HTML", () => {
		const { container } = render(() => <ContentRenderer content="# Hello" />);
		const content = container.querySelector("#markdown-content");
		expect(content).not.toBeNull();
		expect(content!.innerHTML).toContain("<h1");
		expect(content!.innerHTML).toContain("Hello");
	});

	it("shows empty message when content is empty", () => {
		const { container } = render(() => <ContentRenderer content="" />);
		const p = container.querySelector("#markdown-content p");
		expect(p).not.toBeNull();
		expect(p!.textContent).toBe("No content");
	});

	it("shows custom empty message", () => {
		const { container } = render(() => <ContentRenderer content="  " emptyMessage="Nothing here" />);
		const p = container.querySelector("#markdown-content p");
		expect(p!.textContent).toBe("Nothing here");
	});

	it("sanitizes HTML to prevent XSS", () => {
		const malicious = '# Title\n\n<script>alert("xss")</script>\n\n<img src=x onerror="alert(1)">';
		const { container } = render(() => <ContentRenderer content={malicious} />);
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
		const { container } = render(() => <ContentRenderer content={raw} />);
		const content = container.querySelector("#markdown-content");
		expect(content!.textContent).toContain("Red Title");
		// Verify no ESC character in textContent
		expect(content!.textContent).not.toContain("\x1B");
	});

	it("calls onLinkClick with href when .md link is clicked", () => {
		const onLinkClick = vi.fn();
		const { container } = render(() => (
			<ContentRenderer content="See [readme](docs/README.md) for details" onLinkClick={onLinkClick} />
		));
		const link = container.querySelector('a[href="docs/README.md"]') as HTMLAnchorElement;
		expect(link).not.toBeNull();
		fireEvent.click(link);
		expect(onLinkClick).toHaveBeenCalledWith("docs/README.md");
	});

	it("does not call onLinkClick for non-.md links", () => {
		const onLinkClick = vi.fn();
		const { container } = render(() => (
			<ContentRenderer content="See [site](https://example.com) for details" onLinkClick={onLinkClick} />
		));
		const link = container.querySelector('a[href="https://example.com"]') as HTMLAnchorElement;
		expect(link).not.toBeNull();
		fireEvent.click(link);
		expect(onLinkClick).not.toHaveBeenCalled();
	});

	it("prevents a local link when no file handler is provided", () => {
		const debug = vi.spyOn(appLogger, "debug").mockImplementation(() => {});
		try {
			const { container } = render(() => <ContentRenderer content="See [readme](docs/README.md) for details" />);
			const link = container.querySelector('a[href="docs/README.md"]') as HTMLAnchorElement;
			expect(link).not.toBeNull();
			const event = new MouseEvent("click", { bubbles: true, cancelable: true });
			link.dispatchEvent(event);
			expect(event.defaultPrevented).toBe(true);
			expect(debug).toHaveBeenCalledExactlyOnceWith("app", "Markdown link has no file handler", {
				href: "docs/README.md",
			});
		} finally {
			debug.mockRestore();
		}
	});

	it.each([
		"doc.md#section",
		"src/main.rs",
		"src/App.tsx",
		"script.sh",
		"Makefile",
		"LICENSE",
		"docs/",
		"#heading",
		"/tmp/file.py",
		"file.rs:42",
		"file.rs#L42",
		"My%20File.md",
		"./doc.md",
		"//attacker.example/share/readme.md",
		"file://etc/passwd",
		"javascript:alert(1)",
		"javascript:42",
		"data:text/plain,hi",
	])("never lets rendered link %s navigate the webview", (href) => {
		const debug = vi.spyOn(appLogger, "debug").mockImplementation(() => {});
		try {
			const onLinkClick = vi.fn();
			const { container } = render(() => <ContentRenderer content={`[link](${href})`} onLinkClick={onLinkClick} />);
			const link = container.querySelector("a") as HTMLAnchorElement | null;
			if (!link) return; // DOMPurify may remove unsafe links entirely.
			const event = new MouseEvent("click", { bubbles: true, cancelable: true });
			link.dispatchEvent(event);
			expect(event.defaultPrevented).toBe(true);
			if (href.startsWith("file:") || href.startsWith("javascript:") || href.startsWith("data:")) {
				expect(debug).toHaveBeenCalledExactlyOnceWith("app", "Blocked Markdown link scheme", { href });
			} else expect(debug).not.toHaveBeenCalled();
		} finally {
			debug.mockRestore();
		}
	});

	it.each(["https://example.com", "http://example.com", "mailto:boss@example.com"])(
		"opens %s explicitly without navigating the webview",
		(href) => {
			vi.mocked(handleOpenUrl).mockClear();
			const onLinkClick = vi.fn();
			const { container } = render(() => <ContentRenderer content={`[link](${href})`} onLinkClick={onLinkClick} />);
			const event = new MouseEvent("click", { bubbles: true, cancelable: true });
			container.querySelector("a")!.dispatchEvent(event);
			expect(event.defaultPrevented).toBe(true);
			expect(handleOpenUrl).toHaveBeenCalledWith(href);
			expect(onLinkClick).not.toHaveBeenCalled();
		},
	);

	it.each(["src/main.rs", "file.rs:42", "Makefile:42", "My%20File.md", "docs/", "#heading"])(
		"passes local link %s to its file handler",
		(href) => {
			const onLinkClick = vi.fn();
			const { container } = render(() => <ContentRenderer content={`[link](${href})`} onLinkClick={onLinkClick} />);
			const event = new MouseEvent("click", { bubbles: true, cancelable: true });
			container.querySelector("a")!.dispatchEvent(event);
			expect(event.defaultPrevented).toBe(true);
			expect(onLinkClick).toHaveBeenCalledWith(href);
		},
	);

	it("blocks a numeric-suffixed URI scheme instead of treating it as a file line", () => {
		const debug = vi.spyOn(appLogger, "debug").mockImplementation(() => {});
		try {
			const onLinkClick = vi.fn();
			const { container } = render(() => <ContentRenderer content="[bad](javascript:42)" onLinkClick={onLinkClick} />);
			const event = new MouseEvent("click", { bubbles: true, cancelable: true });
			container.querySelector("a")!.dispatchEvent(event);
			expect(event.defaultPrevented).toBe(true);
			expect(onLinkClick).not.toHaveBeenCalled();
			expect(debug).toHaveBeenCalledExactlyOnceWith("app", "Blocked Markdown link scheme", { href: "javascript:42" });
		} finally {
			debug.mockRestore();
		}
	});

	it.each([
		'<a data-tuic-href="https://evil.example" href="docs/guide.md">guide</a>',
		"<a data-tuic-href='https://evil.example' href='docs/guide.md'>guide</a>",
		'<a data-tuic-href=https://evil.example href="docs/guide.md">guide</a>',
		'<a data-tuic-href="https://evil.example/?q=>" href="docs/guide.md">guide</a>',
		'<a data-tuic-href href="docs/guide.md">guide</a>',
		'<a href="docs/guide.md" href="https://evil.example" data-tuic-href="https://evil.example">guide</a>',
		"<a href='docs/guide.md' href=\"https://evil.example\" data-tuic-href='https://evil.example'>guide</a>",
		'<A DATA-TUIC-HREF="https://evil.example" HREF="docs/guide.md">guide</A>',
	])("does not let raw HTML pre-seed the dispatched link", (content) => {
		vi.mocked(handleOpenUrl).mockClear();
		const onLinkClick = vi.fn();
		const { container } = render(() => <ContentRenderer content={content} onLinkClick={onLinkClick} />);
		fireEvent.click(container.querySelector("a")!);
		expect(onLinkClick).toHaveBeenCalledWith("docs/guide.md");
		expect(handleOpenUrl).not.toHaveBeenCalledWith("https://evil.example");
	});

	it("removes forms and image-map links from raw Markdown HTML", () => {
		const { container } = render(() => (
			<ContentRenderer
				content={'<form action="?mode=panel"><button>go</button></form><map><area href="?mode=panel"></map>'}
			/>
		));
		expect(container.querySelector("form, map, area")).toBeNull();
	});

	describe("image src sanitization", () => {
		// convertFileSrc yields asset://localhost/… (macOS/Linux) for rewritten
		// relative image paths. DOMPurify must keep that scheme, otherwise it
		// strips the src and the image renders as a broken box (see ContentRenderer
		// ALLOWED_URI_REGEXP).
		it("preserves asset:// image src through sanitization", () => {
			const md = "![diagram](asset://localhost/Users/me/repo/assets/diagram.png)";
			const { container } = render(() => <ContentRenderer content={md} />);
			const img = container.querySelector("img") as HTMLImageElement;
			expect(img).not.toBeNull();
			expect(img.getAttribute("src")).toBe("asset://localhost/Users/me/repo/assets/diagram.png");
		});

		it("resolves a relative image against baseDir for a file outside any repository", () => {
			const { container } = render(() => (
				<ContentRenderer content="![x](img/x.png)" baseDir="/Users/me/Gits/.tmp/boss" />
			));
			expect(container.querySelector("img")?.getAttribute("src")).toBe(
				"asset://localhost/Users/me/Gits/.tmp/boss/img/x.png",
			);
		});

		// Catches: absolute path concatenated onto baseDir (`<baseDir>//Users/...`).
		it("maps an absolute image path to the asset protocol without prefixing baseDir", () => {
			const { container } = render(() => (
				<ContentRenderer content="![x](/Users/me/Gits/.tmp/boss/img/x.png)" baseDir="/Users/me/other" />
			));
			expect(container.querySelector("img")?.getAttribute("src")).toBe(
				"asset://localhost/Users/me/Gits/.tmp/boss/img/x.png",
			);
		});

		it("preserves tauri:// image src through sanitization", () => {
			const md = "![diagram](tauri://localhost/foo.png)";
			const { container } = render(() => <ContentRenderer content={md} />);
			const img = container.querySelector("img") as HTMLImageElement;
			expect(img.getAttribute("src")).toBe("tauri://localhost/foo.png");
		});

		it("still strips javascript: image src", () => {
			const md = "![x](javascript:alert(1))";
			const { container } = render(() => <ContentRenderer content={md} />);
			const img = container.querySelector("img");
			// DOMPurify must drop the dangerous scheme (empty or removed src)
			expect(img?.getAttribute("src") ?? "").toBe("");
		});
	});

	describe("GFM task-list checkboxes", () => {
		it("ignores checkbox-shaped lines inside block comment bodies when mapping source lines", () => {
			const onToggle = vi.fn();
			const md =
				"<!--tweak:block:c_tasks @2026-09-23T08:00:00.000Z\n- [ ] Mentioned only in the comment-->\n- [ ] Real task";
			const { container } = render(() => <ContentRenderer content={md} onCheckboxToggle={onToggle} />);
			const checkbox = container.querySelector<HTMLInputElement>('input[type="checkbox"]');

			expect(checkbox?.dataset.sourceLine).toBe("2");
			fireEvent.click(checkbox!);
			expect(onToggle).toHaveBeenCalledWith(2, "x");
		});

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
				"- [ ] Real task", // line 0 → sourceLine 0
				"```", // line 1
				"- [ ] Code example", // line 2 — inside fence, not rendered as checkbox
				"```", // line 3
				"- [ ] Another task", // line 4 → sourceLine 4
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
			const { container } = render(() => <ContentRenderer content={md} onCheckboxToggle={onToggle} />);
			const checkboxes = container.querySelectorAll<HTMLInputElement>('input[type="checkbox"]');
			// Click unchecked → should request "x"
			fireEvent.click(checkboxes[0]);
			expect(onToggle).toHaveBeenCalledWith(0, "x");
		});

		it("turns a whole-cell [x] in a table into a checkbox", () => {
			const md = ["| Sel | Cmd |", "| --- | --- |", "| [x] | `/help` |", "| [ ] | `/stop` |"].join("\n");
			const { container } = render(() => <ContentRenderer content={md} />);
			const checkboxes = container.querySelectorAll<HTMLInputElement>('input[type="checkbox"]');
			expect(checkboxes.length).toBe(2);
			expect(checkboxes[0].checked).toBe(true);
			expect(checkboxes[1].checked).toBe(false);
			// Line 2 / line 3, each at the column of its `[`.
			expect(checkboxes[0].dataset.sourceLine).toBe("2");
			expect(checkboxes[0].dataset.sourceCol).toBe("2");
			expect(checkboxes[1].dataset.sourceLine).toBe("3");
		});

		it("reports the clicked table cell's column so a multi-checkbox row stays addressable", () => {
			const onToggle = vi.fn();
			const md = ["| A | B |", "| --- | --- |", "| [ ] | [x] |"].join("\n");
			const { container } = render(() => <ContentRenderer content={md} onCheckboxToggle={onToggle} />);
			const checkboxes = container.querySelectorAll<HTMLInputElement>('input[type="checkbox"]');
			expect(checkboxes.length).toBe(2);
			fireEvent.click(checkboxes[1]);
			// Second cell on line 2 → its own column, not the first cell's.
			expect(onToggle).toHaveBeenCalledWith(2, "~", 8);
		});

		it("leaves [x] inside prose alone — only a whole cell becomes a checkbox", () => {
			const md = ["| Sel | Note |", "| --- | --- |", "| [x] | change [ ] to [x] here |"].join("\n");
			const { container } = render(() => <ContentRenderer content={md} />);
			const checkboxes = container.querySelectorAll<HTMLInputElement>('input[type="checkbox"]');
			expect(checkboxes.length).toBe(1);
			expect(checkboxes[0].dataset.sourceCol).toBe("2");
		});

		it("renders a [~] table cell as indeterminate", () => {
			const md = ["| Sel |", "| --- |", "| [~] |"].join("\n");
			const { container } = render(() => <ContentRenderer content={md} />);
			const cb = container.querySelector<HTMLInputElement>('input[type="checkbox"]');
			expect(cb?.hasAttribute("data-checkbox-indeterminate")).toBe(true);
		});

		it("ignores a table-shaped row inside a fenced code block", () => {
			const md = ["```", "| [x] | fake |", "```", "", "| Sel |", "| --- |", "| [ ] |"].join("\n");
			const { container } = render(() => <ContentRenderer content={md} />);
			const checkboxes = container.querySelectorAll<HTMLInputElement>('input[type="checkbox"]');
			expect(checkboxes.length).toBe(1);
			expect(checkboxes[0].dataset.sourceLine).toBe("6");
		});

		it("keeps list and table checkboxes on independent indexes", () => {
			const md = ["- [ ] a list item", "", "| Sel |", "| --- |", "| [x] |"].join("\n");
			const { container } = render(() => <ContentRenderer content={md} />);
			const checkboxes = container.querySelectorAll<HTMLInputElement>('input[type="checkbox"]');
			expect(checkboxes.length).toBe(2);
			expect(checkboxes[0].dataset.sourceLine).toBe("0");
			expect(checkboxes[0].dataset.sourceCol).toBeUndefined();
			expect(checkboxes[1].dataset.sourceLine).toBe("4");
			expect(checkboxes[1].dataset.sourceCol).toBe("2");
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

/**
 * Incremental mode is the streaming answer's path: the part of the document
 * that can no longer change is parsed once and its DOM is kept, so a tick only
 * costs the block still being written. These tests are about the two things
 * that can go wrong with that — the kept DOM not actually being kept, and a
 * segment rendering differently alone than it does in place.
 */
describe("ContentRenderer incremental mode", () => {
	/** Drives the component the way a stream does: one growing string. */
	const streamed = (initial: string) => {
		const [content, setContent] = createSignal(initial);
		const { container } = render(() => <ContentRenderer content={content()} incremental />);
		return { container, setContent };
	};

	const segments = (container: HTMLElement) => Array.from(container.querySelector("#markdown-content")?.children ?? []);

	it("keeps the committed prefix's DOM node as the answer grows", () => {
		const { container, setContent } = streamed("First para.\n\nSecond para.\n");
		const wrapper = segments(container)[0];
		const rendered = wrapper.querySelector("p");
		expect(rendered?.textContent).toContain("First para.");

		setContent("First para.\n\nSecond para.\n\nThird para.\n");
		// The same element object, not merely equal markup: the committed
		// segment was never re-parsed and never re-inserted.
		expect(segments(container)[0]).toBe(wrapper);
		expect(segments(container)[0].querySelector("p")).toBe(rendered);
	});

	/**
	 * The contrast that proves the test above is not vacuous. Solid keeps the
	 * wrapper <div> in both modes and only re-sets its innerHTML — so the node
	 * that matters is the RENDERED paragraph inside it. Whole-document mode
	 * destroys and recreates it on every tick; that is the cost this replaces.
	 */
	it("rebuilds the rendered element in whole-document mode, which is what this replaces", () => {
		const [content, setContent] = createSignal("First para.\n\nSecond para.\n");
		const { container } = render(() => <ContentRenderer content={content()} />);
		const before = container.querySelector("p");
		expect(before?.textContent).toContain("First para.");
		setContent("First para.\n\nSecond para.\n\nThird para.\n");
		expect(container.querySelector("p")).not.toBe(before);
	});

	it("renders the same text as the whole-document path", () => {
		const md = "# Title\n\nSome prose.\n\n```js\nlet x = 1;\n```\n\n- a\n\n- b\n\nClosing words.\n";
		const { container: inc } = render(() => <ContentRenderer content={md} incremental />);
		const { container: whole } = render(() => <ContentRenderer content={md} />);
		expect(inc.textContent).toBe(whole.textContent);
	});

	it("keeps checkbox source lines absolute across a split", () => {
		// The list sits after a committed paragraph, so its lines are only
		// correct if the segment's offset is carried into the DOM.
		const md = "Intro paragraph.\n\nAnother paragraph.\n\n- [ ] first\n- [x] second\n\nEnd.\n";
		const { container } = render(() => <ContentRenderer content={md} incremental />);
		const boxes = container.querySelectorAll<HTMLInputElement>('input[type="checkbox"]');
		expect(boxes.length).toBe(2);
		expect(boxes[0].dataset.sourceLine).toBe("4");
		expect(boxes[1].dataset.sourceLine).toBe("5");

		const { container: whole } = render(() => <ContentRenderer content={md} />);
		const wholeBoxes = whole.querySelectorAll<HTMLInputElement>('input[type="checkbox"]');
		expect(Array.from(boxes).map((b) => b.dataset.sourceLine)).toEqual(
			Array.from(wholeBoxes).map((b) => b.dataset.sourceLine),
		);
	});

	/**
	 * Tweak highlights are a matched pair of private-use codepoints, consumed
	 * after render. A split between them orphans one, and the survivor shows up
	 * as a stray glyph. The invariant is that incremental mode leaves exactly
	 * the sentinels the whole-document path leaves — no more, no fewer.
	 */
	it("orphans no tweak sentinel across the split", () => {
		const md =
			"Intro.\n\n<!--tweak:begin:t1-->\nHighlighted para.\n\nStill highlighted.\n<!--tweak:end:t1 @0 note-->\n\nAfter.\n";
		const count = (el: HTMLElement, ch: string) => (el.textContent?.split(ch).length ?? 1) - 1;
		const { container } = render(() => <ContentRenderer content={md} incremental />);
		const { container: whole } = render(() => <ContentRenderer content={md} />);
		expect(count(container, "\uE000")).toBe(count(whole, "\uE000"));
		expect(count(container, "\uE001")).toBe(count(whole, "\uE001"));
		// Both halves of a pair land in one segment, so the count stays even.
		expect(count(container, "\uE000") % 2).toBe(0);
	});

	it("never splits a fence, so no half code block reaches the DOM", () => {
		const md = "Intro.\n\n```js\nconst a = 1;\n\nconst b = 2;\n```\n\nAfter.\n";
		const { container } = render(() => <ContentRenderer content={md} incremental />);
		const codes = container.querySelectorAll("pre code");
		expect(codes.length).toBe(1);
		expect(codes[0].textContent).toContain("const a = 1;");
		expect(codes[0].textContent).toContain("const b = 2;");
	});

	// A streaming tick must cost the tail alone; block comments are a file-tab
	// feature, so the answer is never lexed for block ranges.
	it("does not lex the whole answer for block comments on a streaming tick", async () => {
		const lexer = vi.spyOn(marked, "lexer");
		try {
			const { setContent } = streamed("<!--tweak:block:c_q @0\nquoted-->\nFirst.\n\n");
			setContent("<!--tweak:block:c_q @0\nquoted-->\nFirst.\n\nSecond.\n");
			await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
			expect(lexer).not.toHaveBeenCalled();
		} finally {
			lexer.mockRestore();
		}
	});

	it("re-renders from scratch when the content is replaced rather than appended", () => {
		const { container, setContent } = streamed("First answer.\n\nSecond para.\n");
		setContent("A totally different answer.\n\nWith its own text.\n");
		expect(container.textContent).toContain("A totally different answer.");
		expect(container.textContent).not.toContain("First answer.");
	});
});

/** Adversarial-review findings against the first cut of incremental mode. */
describe("ContentRenderer incremental mode, review findings", () => {
	const segments = (container: HTMLElement) => Array.from(container.querySelector("#markdown-content")?.children ?? []);

	/**
	 * Committed HTML is cached by segment identity alone, but the HTML also
	 * depends on `baseDir` — image `src` values are rewritten through it. A
	 * caller that switches repo would keep serving images from the old one.
	 */
	it("re-renders committed segments when baseDir changes", () => {
		const [baseDir, setBaseDir] = createSignal("/repo-a");
		const { container } = render(() => (
			<ContentRenderer content={"![i](img.png)\n\nSecond.\n\nTail.\n"} baseDir={baseDir()} incremental />
		));
		expect(container.querySelector("img")?.getAttribute("src")).toContain("repo-a");

		setBaseDir("/repo-b");

		expect(container.querySelector("img")?.getAttribute("src")).toContain("repo-b");
	});

	/**
	 * Each committed segment lives in its own wrapper, so its last paragraph is
	 * `p:last-child` of that wrapper — and AIChatPanel.module.css zeroes the
	 * bottom margin of exactly that selector, which would collapse the gap
	 * between segments. The wrappers carry a class so the stylesheet can tell a
	 * segment's last paragraph from the answer's last paragraph.
	 */
	it("marks each segment wrapper so the last-paragraph rule can be scoped", () => {
		const { container } = render(() => <ContentRenderer content={"First.\n\nSecond.\n\nThird.\n"} incremental />);

		const wrappers = segments(container);
		expect(wrappers.length).toBeGreaterThan(1);
		for (const w of wrappers) expect(w.classList.contains("md-segment")).toBe(true);
	});
});

/**
 * Mermaid renders asynchronously and DESTRUCTIVELY: it replaces the `<pre>` of
 * a diagram block with an `<svg>` wrapper. That only survives if the committed
 * segment holding it is never re-rendered — otherwise every tick would wipe the
 * diagram and start rendering it again.
 */
describe("ContentRenderer incremental mode, mermaid blocks", () => {
	const mermaidSource = "```mermaid\ngraph TD;\n  A-->B;\n```\n\nAfter the diagram.\n";

	it("exposes a committed mermaid block to the container query", () => {
		const { container } = render(() => <ContentRenderer content={`${mermaidSource}\nTail.\n`} incremental />);
		const root = container.querySelector("#markdown-content") as HTMLElement;

		// The wrapper divs sit between the container and the block; the selector
		// the render pass uses is descendant-based, so it still finds it.
		expect(root.querySelectorAll("code.language-mermaid").length).toBe(1);
	});

	it("does not destroy an already-rendered diagram when the answer grows", () => {
		const [content, setContent] = createSignal(`${mermaidSource}\nTail.\n`);
		const { container } = render(() => <ContentRenderer content={content()} incremental />);
		const root = container.querySelector("#markdown-content") as HTMLElement;

		// Stand in for what mermaid does once its async render resolves.
		const pre = root.querySelector("code.language-mermaid")?.parentElement as HTMLElement;
		const diagram = document.createElement("div");
		diagram.className = "mermaid-diagram";
		pre.replaceWith(diagram);

		setContent(`${mermaidSource}\nTail.\n\nMore answer.\n`);

		expect(root.querySelector(".mermaid-diagram")).toBe(diagram);
		expect(root.querySelectorAll("code.language-mermaid").length).toBe(0);
	});

	/** The contrast that proves the test above is not vacuous: whole-document
	 *  mode re-sets innerHTML, so the same diagram is thrown away. */
	it("loses the rendered diagram in whole-document mode", () => {
		const [content, setContent] = createSignal(`${mermaidSource}\nTail.\n`);
		const { container } = render(() => <ContentRenderer content={content()} />);
		const root = container.querySelector("#markdown-content") as HTMLElement;

		const pre = root.querySelector("code.language-mermaid")?.parentElement as HTMLElement;
		const diagram = document.createElement("div");
		diagram.className = "mermaid-diagram";
		pre.replaceWith(diagram);

		setContent(`${mermaidSource}\nTail.\n\nMore answer.\n`);

		expect(root.querySelector(".mermaid-diagram")).toBeNull();
		expect(root.querySelectorAll("code.language-mermaid").length).toBe(1);
	});
});

/**
 * Non-streaming callers (MarkdownTab is the only one) must be untouched by
 * incremental mode: same single render, same DOM shape, same absolute line
 * numbers. `incremental` is opt-in, and without it the component takes the
 * whole-document path with `lineOffset` 0 — which is the identity.
 */
describe("ContentRenderer non-streaming callers", () => {
	const rich = [
		"# Title",
		"",
		"- [ ] first",
		"- [x] second",
		"",
		"| a | b |",
		"| --- | --- |",
		"| 1 | 2 |",
		"",
		"```js",
		"const x = 1;",
		"```",
		"",
		"Final paragraph.",
	].join("\n");

	/** The whole document stays in the single wrapper it has always had — the
	 *  per-segment wrappers exist only on the incremental path. */
	it("adds no segment wrapper to the document's one container", () => {
		const { container } = render(() => <ContentRenderer content={rich} />);
		const root = container.querySelector("#markdown-content") as HTMLElement;

		expect(root.querySelectorAll(".md-segment").length).toBe(0);
		expect(root.children.length).toBe(1);
		expect(root.querySelector("h1")?.parentElement).toBe(root.children[0]);
	});

	it("keeps checkbox source lines equal to their raw line numbers", () => {
		const { container } = render(() => <ContentRenderer content={rich} />);
		const boxes = container.querySelectorAll<HTMLInputElement>('input[type="checkbox"]');

		expect(Array.from(boxes).map((b) => b.dataset.sourceLine)).toEqual(["2", "3"]);
	});
});
