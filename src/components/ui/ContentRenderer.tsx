import AnsiToHtml from "ansi-to-html";
// DOMPurify's template scrubbing relies on a complete NodeIterator implementation.
// The component tests therefore use jsdom instead of the suite-wide happy-dom
// environment so security behavior matches real browser engines.
import DOMPurify from "dompurify";
import { marked, type Tokens } from "marked";
import "./markdown-content.css";
import { type Component, createEffect, createMemo, Index, onCleanup, Show } from "solid-js";
import { appLogger } from "../../stores/appLogger";
import { type MarkdownSegment, type StreamSplit, splitStream } from "../../utils/incrementalMarkdown";
import { handleOpenUrl } from "../../utils/openUrl";
import { rewriteLocalImages } from "../../utils/repoImageUrl";
import { stripAnsi } from "../../utils/stripAnsi";
import {
	findTweakCommentBlocks,
	injectTweakSentinels,
	maskTweakCommentSyntax,
	parseInlineTweakComments,
	parseTweakComments,
} from "../../utils/tweakComments";
import { applyTweakDomHighlights } from "../../utils/tweakDomHighlight";
import { filePathRegex } from "../Terminal/linkProvider";

/** DOMPurify's default allowed-URI schemes plus Tauri's local asset protocols
 *  (`asset:`, `tauri:`). Without these, DOMPurify strips the rewritten image
 *  `src` (convertFileSrc yields `asset://localhost/…` on macOS/Linux), leaving
 *  `src=""` and a broken-image box. `http(s):` is already covered, so Windows'
 *  `http://asset.localhost` form keeps working. */
const ALLOWED_URI_REGEXP =
	/^(?:(?:(?:f|ht)tps?|mailto|tel|callto|sms|cid|xmpp|asset|tauri):|[^a-z]|[a-z+.-]+(?:[^a-z+.\-:]|$))/i;

export interface ContentRendererProps {
	content: string;
	/** Opt-in copy action for fenced code blocks. Receives the original source text. */
	onCodeCopy?: (text: string) => void;
	/** Turn plain file paths into links; the caller still resolves and validates them. */
	autoLinkFiles?: boolean;
	emptyMessage?: string;
	/** Called for local file, directory, and heading links (raw href passed as argument). */
	onLinkClick?: (href: string) => void;
	/**
	 * Called when a checkbox is clicked (source line, new mark, and — for a
	 * whole-cell checkbox in a table — the column of its `[`, since one row can
	 * carry several).
	 */
	onCheckboxToggle?: (sourceLine: number, mark: " " | "x" | "~", sourceCol?: number) => void;
	/** Absolute directory path of the source file, used to resolve relative image src attributes */
	baseDir?: string;
	/** Browser URL for a relative image, when local asset URLs are unavailable. */
	imageSrc?: (relativePath: string) => string;
	/** Ref callback to expose the rendered content container for search */
	contentRef?: (el: HTMLDivElement) => void;
	/** Override the root font size in pixels (children use em, so everything scales). */
	fontSize?: number;
	/** Add exact raw-source ranges to Markdown blocks and individual list items for gutter comments. */
	commentableBlocks?: boolean;
	/**
	 * Render a growing answer as a committed prefix plus a live tail, so a tick
	 * only re-parses the block still being written instead of the whole
	 * document. Opt-in, and only sound for append-only content: a streaming
	 * answer. Leave it off everywhere else — a static document gains nothing
	 * and would pay for the split.
	 */
	incremental?: boolean;
}

/** Strip event handler attributes (on*) as defense-in-depth before DOMPurify */
export function stripEventHandlers(html: string): string {
	return html.replace(/\s+on\w+\s*=\s*("[^"]*"|'[^']*'|[^\s>]*)/gi, "");
}

/** Attach raw-source ranges to the top-level elements produced by marked. */
export function applyCommentBlockMetadata(container: HTMLElement, source: string): void {
	const blocks = findTweakCommentBlocks(source);
	const comments = parseTweakComments(source, blocks).filter((comment) => comment.anchor === "block");
	const renderRoot = container.firstElementChild ?? container;
	const elements = Array.from(renderRoot.children) as HTMLElement[];
	const listItems = Array.from(renderRoot.querySelectorAll<HTMLElement>("li"));
	let elementIndex = 0;
	let itemIndex = 0;
	// DEFERRED (2026-09-23) — pairing is by tag only. A raw HTML token (e.g.
	// `<p align="center">`) renders an element with no entry in `blocks`, so the
	// next same-tag block binds to it and every later one shifts by one. The fix
	// needs per-token element counts for `html` tokens (after DOMPurify), which
	// is more than a local change; the save path already refuses a stale range.
	for (const block of blocks) {
		let element: HTMLElement | undefined;
		if (block.tag === "LI") element = listItems[itemIndex++];
		else {
			while (elementIndex < elements.length && elements[elementIndex].tagName !== block.tag) elementIndex++;
			element = elements[elementIndex++];
		}
		if (!element) break;
		if (block.tag !== "UL" && block.tag !== "OL") {
			element.dataset.commentSourceStart = String(block.start);
			element.dataset.commentSourceEnd = String(block.end);
		}
		const comment = comments.find(
			(candidate) => candidate.sourceStart === block.start && candidate.sourceEnd === block.end,
		);
		if (!comment) continue;
		element.classList.add("tweak-block-highlight");
		element.dataset.tweakId = comment.id;
		element.dataset.tweakAt = comment.createdAt;
		element.dataset.tweakComment = comment.comment;
	}
}

// Configure marked for safe rendering
marked.setOptions({
	gfm: true, // GitHub Flavored Markdown
	breaks: true, // Convert \n to <br>
});

const ansiConverter = new AnsiToHtml({ escapeXML: true });
const ANSI_CSI_RE = /\x1b\[[\x30-\x3f]*[\x20-\x2f]*[\x40-\x7e]/;

// Custom renderer: code blocks with ANSI sequences render with colors.
marked.use({
	renderer: {
		code(token: Tokens.Code) {
			const lang = token.lang ?? "";
			const baseCls = lang ? `language-${lang}` : "";
			const raw = encodeURIComponent(token.text);
			if (ANSI_CSI_RE.test(token.text)) {
				const cls = [baseCls, "ansi-block"].filter(Boolean).join(" ");
				return `<pre data-raw-code="${raw}"><code class="${cls}">${ansiConverter.toHtml(token.text)}</code></pre>\n`;
			}
			const escaped = token.text.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
			return `<pre data-raw-code="${raw}"><code${baseCls ? ` class="${baseCls}"` : ""}>${escaped}</code></pre>\n`;
		},
	},
});

/**
 * Strips ANSI escape sequences from prose sections only, leaving code fence
 * content intact so the custom marked renderer can colorize it.
 */
function stripAnsiOutsideCodeBlocks(source: string): string {
	const lines = source.split("\n");
	const out: string[] = [];
	let inFence = false;
	for (const line of lines) {
		if (/^\s*(`{3,}|~{3,})/.test(line)) {
			inFence = !inFence;
			out.push(line);
		} else {
			out.push(inFence ? line : stripAnsi(line));
		}
	}
	return out.join("\n");
}

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

/** One checkbox in the source: its line, the column of its `[`, and its mark. */
interface CheckboxSite {
	line: number;
	col: number;
	mark: " " | "x" | "~";
}

function normalizeMark(raw: string): " " | "x" | "~" {
	if (raw === "~") return "~";
	return raw === " " ? " " : "x";
}

/**
 * Build a mapping from sequential checkbox index (as rendered by marked)
 * to its source site. Scans the raw source and returns an array where
 * entry[domIndex] describes the checkbox. Skips lines inside fenced code blocks.
 */
function buildCheckboxLineMap(source: string): CheckboxSite[] {
	const lines = source.split("\n");
	const map: CheckboxSite[] = [];
	let inFence = false;
	for (let i = 0; i < lines.length; i++) {
		if (/^\s*(`{3,}|~{3,})/.test(lines[i])) {
			inFence = !inFence;
			continue;
		}
		if (inFence) continue;
		const m = /^(\s*[-*+]\s+)\[([ xX~])\]/.exec(lines[i]);
		if (m) map.push({ line: i, col: m[1].length, mark: normalizeMark(m[2]) });
	}
	return map;
}

/**
 * Find whole-cell checkboxes in GFM tables.
 *
 * A task list is a *list item* extension, so marked never emits an `<input>` for
 * a `| [x] |` cell — the text stays literal. We locate those cells in the source
 * and inject the input ourselves (step 6 in `renderMarkdownSegment`).
 *
 * Only a cell whose ENTIRE content is a checkbox counts. That is what keeps a
 * sentence like "change `[ ]` to `[x]`" — or any `[x]` inside a prose cell —
 * from sprouting checkboxes nobody asked for.
 *
 * The column matters: one row can hold several checkbox cells, so the line alone
 * cannot address the one that was clicked.
 */
function buildTableCheckboxMap(source: string): CheckboxSite[] {
	const lines = source.split("\n");
	const sites: CheckboxSite[] = [];
	let inFence = false;
	for (let i = 0; i < lines.length; i++) {
		const line = lines[i];
		if (/^\s*(`{3,}|~{3,})/.test(line)) {
			inFence = !inFence;
			continue;
		}
		if (inFence) continue;
		// Allow blockquote markers: marked renders a table inside `>` too, and
		// missing those rows here would desync the sequential index below.
		if (!/^[\s>]*\|/.test(line)) continue;
		// Walk the row between unescaped pipes, keeping each cell's start offset.
		let start = -1;
		for (let c = 0; c < line.length; c++) {
			if (line[c] !== "|" || line[c - 1] === "\\") continue;
			if (start >= 0) {
				const cell = line.slice(start, c);
				const m = /^\s*\[([ xX~])\]\s*$/.exec(cell);
				if (m) sites.push({ line: i, col: start + cell.indexOf("["), mark: normalizeMark(m[1]) });
			}
			start = c + 1;
		}
	}
	return sites;
}

let mermaidInitialized = false;
let mermaidIdCounter = 0;

async function renderMermaidBlocks(container: HTMLElement): Promise<void> {
	const codeEls = container.querySelectorAll<HTMLElement>("code.language-mermaid");
	if (codeEls.length === 0) return;
	try {
		const { default: mermaid } = await import("mermaid");
		if (!mermaidInitialized) {
			mermaid.initialize({
				startOnLoad: false,
				theme: "dark",
				fontFamily: "var(--font-ui)",
				securityLevel: "strict",
			});
			mermaidInitialized = true;
		}
		for (const codeEl of codeEls) {
			const pre = codeEl.parentElement;
			if (pre?.tagName !== "PRE" || pre.dataset.mermaidRendered) continue;
			const source = codeEl.textContent?.trim();
			if (!source) continue;
			const id = `mermaid-${++mermaidIdCounter}`;
			try {
				const { svg } = await mermaid.render(id, source);
				const wrapper = document.createElement("div");
				wrapper.className = "mermaid-diagram";
				wrapper.innerHTML = svg;
				// The diagram stands where the block was: it keeps the block's source range and comment metadata.
				for (const [key, value] of Object.entries(pre.dataset)) {
					if (key.startsWith("commentSource") || key.startsWith("tweak")) wrapper.dataset[key] = value ?? "";
				}
				if (pre.classList.contains("tweak-block-highlight")) wrapper.classList.add("tweak-block-highlight");
				pre.replaceWith(wrapper);
			} catch {
				pre.dataset.mermaidRendered = "error";
			}
		}
	} catch (err) {
		appLogger.warn("app", "Mermaid load failed", err);
	}
}

/**
 * Source → sanitized HTML for ONE parse unit.
 *
 * `lineOffset` is the absolute source line the unit starts on. It is added to
 * every line number this function derives, which is what lets a document be
 * rendered in pieces without the checkbox `data-source-line` values drifting:
 * a segment maps its own lines, then shifts them into document coordinates.
 * At offset 0 — the whole-document path — every step below is exactly what it
 * has always been.
 */
function renderMarkdownSegment(
	source: string,
	opts: { baseDir?: string; imageSrc?: (path: string) => string; lineOffset: number },
): string {
	const raw = stripAnsiOutsideCodeBlocks(source);
	try {
		// 1. Convert [~] to [ ] so marked renders them as standard GFM task-list items.
		//    Track which source lines had tilde for indeterminate styling later.
		const { cleaned, tildeLines } = preprocessTildeCheckboxes(raw);

		// 2. Build source-line map BEFORE any transforms: domIndex → sourceLine.
		//    This must use the tilde-cleaned source (same checkbox count as marked sees).
		const mappingSource = maskTweakCommentSyntax(cleaned);
		const lineMap = buildCheckboxLineMap(mappingSource);
		//    Table cells need their own map: marked emits no <input> for them, so
		//    they cannot share the sequential index of the marked-rendered ones.
		const tableMap = buildTableCheckboxMap(mappingSource);

		// 3. Replace tweak markers with sentinel delimiters (highlight spans are
		//    applied to the rendered DOM afterwards), then parse markdown.
		const withSentinels = injectTweakSentinels(cleaned);
		let html = marked.parse(withSentinels, { async: false }) as string;

		// 4. Resolve local images through the active transport and repository boundary.
		html = rewriteLocalImages(html, opts.baseDir, opts.imageSrc);

		// 5. Make GFM task-list checkboxes interactive and inject source-line metadata.
		//    Sequential checkbox index in the HTML maps to lineMap[domIndex]. Both
		//    are segment-relative; only what reaches the DOM is shifted into
		//    document coordinates, so `tildeLines` is still keyed the way it was
		//    built.
		let cbIndex = 0;
		html = html.replace(/<input\b[^>]*type="checkbox"[^>]*>/gi, (match) => {
			const site = lineMap[cbIndex++];
			// Remove disabled attribute
			let out = match.includes("disabled") ? match.replace(/\s*disabled(?:="")?/i, "") : match;
			// Inject data-source-line for the click handler
			if (site !== undefined) {
				out = out.replace(/>$/, ` data-source-line="${opts.lineOffset + site.line}">`);
				// Mark tilde checkboxes for indeterminate styling
				if (tildeLines.has(site.line)) {
					out = out.replace(/>$/, ` ${TILDE_SENTINEL}>`);
				}
			}
			return out;
		});

		// 6. Same treatment for whole-cell checkboxes in tables, which marked left
		//    as literal text. Both scans run over the source in reading order and
		//    marked emits blocks in that same order, so the sequential index lines
		//    up — exactly as it does for the list-item pass above. `data-source-col`
		//    is what disambiguates a row carrying more than one checkbox.
		let tblIndex = 0;
		html = html.replace(/<(td|th)\b([^>]*)>\s*\[[ xX~]\]\s*<\/\1>/gi, (match, tag, attrs) => {
			const site = tableMap[tblIndex++];
			if (site === undefined) return match;
			const checked = site.mark === "x" ? " checked" : "";
			const tilde = site.mark === "~" ? ` ${TILDE_SENTINEL}` : "";
			return `<${tag}${attrs}><input type="checkbox"${checked}${tilde} data-source-line="${opts.lineOffset + site.line}" data-source-col="${site.col}"></${tag}>`;
		});

		// Preserve the source href in an inert attribute. DOMPurify strips unsafe
		// href schemes, including useful `file.rs:42` line links; the click
		// dispatcher still needs their text to block or route them explicitly.
		// Remove any source-supplied copy first so raw HTML cannot dispatch a
		// different destination from the link shown to the user.
		if (/<a\b/i.test(html)) {
			const template = document.createElement("template");
			template.innerHTML = html;
			for (const anchor of template.content.querySelectorAll("a")) {
				const href = anchor.getAttribute("href");
				anchor.removeAttribute("data-tuic-href");
				if (href !== null) anchor.setAttribute("data-tuic-href", href);
			}
			html = template.innerHTML;
		}
		return DOMPurify.sanitize(stripEventHandlers(html), {
			FORBID_TAGS: ["form", "map", "area"],
			ADD_ATTR: [
				"data-tuic-href",
				"data-tweak-id",
				"data-tweak-at",
				"data-tweak-comment",
				"data-source-line",
				"data-source-col",
				TILDE_SENTINEL,
				"style",
			],
			ALLOWED_URI_REGEXP,
		});
	} catch (err) {
		appLogger.error("app", "Markdown parsing error", err);
		const escaped = raw.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
		return `<pre>${escaped}</pre>`;
	}
}

export const ContentRenderer: Component<ContentRendererProps> = (props) => {
	// Whole-document path: one parse of everything, exactly as before.
	const processedContent = createMemo(() =>
		props.incremental
			? ""
			: renderMarkdownSegment(props.content ?? "", { baseDir: props.baseDir, imageSrc: props.imageSrc, lineOffset: 0 }),
	);

	/**
	 * Incremental path. `renderedHtml` holds the HTML of segments already
	 * committed; each is parsed once and then never again, so a tick costs the
	 * tail alone. `renderedFor` is the segment list those strings were produced
	 * from — comparing its last entry by identity proves the prefix did not move
	 * under us, which is how a conversation switch (a replacement, not an
	 * append) is caught and forces a clean re-render.
	 */
	let split: StreamSplit | undefined;
	let renderedHtml: string[] = [];
	let renderedFor: MarkdownSegment[] = [];
	let renderedBaseDir: string | undefined;
	let renderedImageSrc: ContentRendererProps["imageSrc"];
	const incrementalContent = createMemo(() => {
		if (!props.incremental) return { committed: [] as string[], tail: "" };
		split = splitStream(props.content ?? "", split);
		const committed = split.committed;
		const n = renderedFor.length;
		// Segment identity is not the whole cache key: `baseDir` rewrites image
		// src values, so cached HTML from another directory points at the wrong
		// files.
		if (props.baseDir !== renderedBaseDir || props.imageSrc !== renderedImageSrc) {
			renderedBaseDir = props.baseDir;
			renderedImageSrc = props.imageSrc;
			renderedHtml = [];
			renderedFor = [];
		} else if (n > committed.length || (n > 0 && committed[n - 1] !== renderedFor[n - 1])) {
			renderedHtml = [];
			renderedFor = [];
		}
		for (let i = renderedFor.length; i < committed.length; i++) {
			renderedHtml.push(
				renderMarkdownSegment(committed[i].text, {
					baseDir: props.baseDir,
					imageSrc: props.imageSrc,
					lineOffset: committed[i].lineOffset,
				}),
			);
			renderedFor.push(committed[i]);
		}
		return {
			committed: renderedHtml.slice(),
			tail: renderMarkdownSegment(split.tail.text, {
				baseDir: props.baseDir,
				imageSrc: props.imageSrc,
				lineOffset: split.tail.lineOffset,
			}),
		};
	});

	const isEmpty = createMemo(() => (props.content ?? "").trim() === "");

	// Inline tweak comments parsed from the raw source — applied to the rendered DOM
	// below. Block comments are attached by `applyCommentBlockMetadata`; parsing them
	// here would lex the whole document again, on every streaming tick too.
	const inlineTweakComments = createMemo(() => parseInlineTweakComments(props.content ?? ""));

	const handleClick = (e: MouseEvent) => {
		const target = e.target as HTMLElement;
		if (target.closest("button[data-copy-code]") && props.onCodeCopy) {
			const raw = target.closest("pre")?.getAttribute("data-raw-code");
			if (raw !== null && raw !== undefined) props.onCodeCopy(decodeURIComponent(raw));
			return;
		}

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

			// Only a table cell carries a column; a list item is addressed by line
			// alone, so it keeps calling with the original two-argument shape.
			const rawCol = target.dataset.sourceCol;
			if (rawCol != null) props.onCheckboxToggle?.(line, nextMark, parseInt(rawCol, 10));
			else props.onCheckboxToggle?.(line, nextMark);
			return;
		}

		const anchor = target.closest("a");
		if (!anchor) return;
		const href = anchor.getAttribute("data-tuic-href") ?? anchor.getAttribute("href");
		// All rendered links are untrusted document content. Nothing may fall
		// through to WebView navigation, including links without a host handler.
		e.preventDefault();
		e.stopPropagation();
		if (!href) {
			appLogger.debug("app", "Blocked Markdown link without a safe href");
			return;
		}
		if (/^(https?:|mailto:)/i.test(href)) {
			handleOpenUrl(href);
		} else if (
			// A bare name:42 can be an extensionless file; Rust resolves the target.
			// Keep known URL schemes out of that ambiguous file-line form.
			/^[a-z][a-z\d+.-]*:/i.test(href) &&
			!/^[a-z]:[\\/]/i.test(href) &&
			!(
				/^[^:/\\]+:[1-9]\d*$/.test(href) &&
				!/^(?:javascript|vbscript|data|file|blob|about|ftp|https?|mailto|tel|sms|callto|cid|xmpp|asset|tauri|wss?):/i.test(
					href,
				)
			)
		) {
			appLogger.debug("app", "Blocked Markdown link scheme", { href });
		} else if (props.onLinkClick) {
			props.onLinkClick(href);
		} else if (href.startsWith("#")) {
			const heading = Array.from(containerRef?.querySelectorAll("h1,h2,h3,h4,h5,h6") ?? []).find(
				(el) =>
					el.id === href.slice(1) ||
					el.textContent
						?.trim()
						.toLowerCase()
						.replace(/[^\p{L}\p{N}\s-]/gu, "")
						.replace(/\s+/g, "-") === href.slice(1).toLowerCase(),
			);
			heading?.scrollIntoView({ block: "start" });
		} else {
			appLogger.debug("app", "Markdown link has no file handler", { href });
		}
	};

	let containerRef: HTMLDivElement | undefined;

	// After render, set indeterminate property on [~] checkboxes (not settable via HTML attribute)
	// and render Mermaid diagrams from ```mermaid code blocks.
	createEffect(() => {
		// Subscribe to whichever path is live. Both passes below are idempotent
		// over already-processed DOM — the sentinels are consumed, and a rendered
		// mermaid block no longer matches its selector — so committed segments
		// that survive a tick are simply skipped.
		processedContent();
		incrementalContent();
		// Comment markers are invisible in the rendered HTML. A source-only edit
		// still moves block ranges and must refresh their DOM metadata.
		if (props.commentableBlocks && !props.incremental) props.content;
		if (!containerRef) return;
		if (props.autoLinkFiles) {
			queueMicrotask(() => {
				if (!containerRef) return;
				const walker = document.createTreeWalker(containerRef, NodeFilter.SHOW_TEXT);
				const nodes: Text[] = [];
				while (walker.nextNode()) {
					const node = walker.currentNode as Text;
					if (!node.parentElement?.closest("a, code, pre, button, textarea, input")) nodes.push(node);
				}
				for (const node of nodes) {
					const source = node.textContent ?? "";
					const regex = filePathRegex();
					const fragment = document.createDocumentFragment();
					let end = 0;
					for (const match of source.matchAll(regex)) {
						const path = match[1];
						const start = match.index + match[0].indexOf(path);
						fragment.append(document.createTextNode(source.slice(end, start)));
						const anchor = document.createElement("a");
						anchor.href = path;
						anchor.dataset.tuicHref = path;
						anchor.textContent = path;
						fragment.append(anchor);
						end = start + path.length;
					}
					if (!end) continue;
					fragment.append(document.createTextNode(source.slice(end)));
					node.replaceWith(fragment);
				}
			});
		}
		if (props.onCodeCopy) {
			queueMicrotask(() => {
				containerRef?.querySelectorAll<HTMLPreElement>("pre[data-raw-code]").forEach((pre) => {
					if (pre.querySelector("button[data-copy-code]")) return;
					const button = document.createElement("button");
					button.type = "button";
					button.dataset.copyCode = "";
					button.setAttribute("aria-label", "Copy code");
					button.textContent = "Copy";
					pre.append(button);
				});
			});
		}
		const raf = requestAnimationFrame(() => {
			if (!containerRef) return;
			containerRef.querySelectorAll<HTMLInputElement>(`input[${TILDE_SENTINEL}]`).forEach((cb) => {
				cb.indeterminate = true;
			});
			// Turn highlight sentinels into <span class="tweak-highlight"> wrappers.
			const inlineComments = inlineTweakComments();
			if (inlineComments.length > 0) applyTweakDomHighlights(containerRef, inlineComments);
			if (props.commentableBlocks && !props.incremental) applyCommentBlockMetadata(containerRef, props.content ?? "");
			renderMermaidBlocks(containerRef);
		});
		onCleanup(() => cancelAnimationFrame(raf));
	});

	return (
		<div
			id="markdown-content"
			ref={(el) => {
				containerRef = el;
				props.contentRef?.(el);
			}}
			onClick={handleClick}
			style={props.fontSize !== undefined ? { "font-size": `${props.fontSize}px` } : undefined}
		>
			<Show when={!isEmpty()} fallback={<p>{props.emptyMessage || "No content"}</p>}>
				<Show
					when={props.incremental}
					fallback={
						/* eslint-disable-next-line solid/no-innerhtml */
						<div innerHTML={processedContent()} />
					}
				>
					{/* Index, not For: it keys by position, so a committed segment's
					    string never moves and its DOM is never rebuilt. */}
					{/* `md-segment` is not decoration: each segment's last paragraph is
					    `p:last-child` of its own wrapper, and a stylesheet that zeroes
					    that margin would collapse the gap between segments. The class
					    lets it exclude every wrapper but the last. */}
					<Index each={incrementalContent().committed}>
						{/* eslint-disable-next-line solid/no-innerhtml */}
						{(html) => <div class="md-segment" innerHTML={html()} />}
					</Index>
					{/* eslint-disable-next-line solid/no-innerhtml */}
					<div class="md-segment" innerHTML={incrementalContent().tail} />
				</Show>
			</Show>
		</div>
	);
};
