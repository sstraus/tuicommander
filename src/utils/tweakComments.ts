import { marked, type Tokens } from "marked";

/**
 * Inline review comments for markdown files.
 *
 * Comments are stored directly inside the .md source as HTML comments,
 * which are invisible to any markdown renderer but readable by humans and LLMs:
 *
 *     prefix <!--tweak:begin:ID-->highlighted text<!--tweak:end:ID @<ISO-TIMESTAMP>
 *     comment body, free text, may span multiple lines
 *     --> suffix
 *
 * The body is plain text. The only forbidden sequence is `-->` (which would
 * close the enclosing HTML comment prematurely); it is escaped to `--&gt;`
 * on write and restored on read. No other escaping is performed — quotes,
 * newlines, unicode, `<`, `&` are all kept verbatim.
 *
 * The first time a comment is added to a file, a convention header is prepended
 * so that any LLM reading the file understands the format without external context.
 */

export interface TweakComment {
	id: string;
	highlighted: string;
	comment: string;
	createdAt: string;
	anchor?: "block";
	sourceStart?: number;
	sourceEnd?: number;
}

export interface TweakCommentBlock {
	start: number;
	end: number;
	tag: "P" | "H1" | "H2" | "H3" | "H4" | "H5" | "H6" | "UL" | "OL" | "LI" | "BLOCKQUOTE" | "PRE" | "TABLE";
}

export const CONVENTION_HEADER =
	"<!-- tweak-comments v1: inline review comments.\n" +
	"     Inline: [tweak:begin:ID]selected text[tweak:end:ID @ISO-TIMESTAMP\n" +
	"     comment body]. Block: [tweak:block:ID @ISO-TIMESTAMP\n" +
	"     comment body] immediately before the selected block.\n" +
	"     Item: [tweak:item:ID @ISO-TIMESTAMP\n" +
	"     comment body] after the selected list item, within the list.\n" +
	"     Square brackets stand for HTML comment delimiters, not literal brackets.\n" +
	"     Escape HTML comment closing sequences as '--&gt;' in comment bodies.\n" +
	"     Read each comment, apply the feedback to the selected text or block,\n" +
	"     then remove the tweak markers. -->\n\n";

const LEGACY_CONVENTION_HEADER =
	"<!-- tweak-comments v1: inline review comments.\n" +
	"     Format: [tweak:begin:ID]highlighted text[tweak:end:ID @ISO-TIMESTAMP\n" +
	"     comment body (free text, may span multiple lines)\n" +
	"     ] — where [ ] are the HTML comment delimiters <!-- -->.\n" +
	"     The only escape is '-->' → '--&gt;' inside the comment body.\n" +
	"     Read each comment, apply the feedback to the highlighted text,\n" +
	"     then remove the tweak markers. -->\n\n";

const CONVENTION_HEADERS = [CONVENTION_HEADER, LEGACY_CONVENTION_HEADER];

function conventionHeaderPrefixLength(source: string): number {
	for (const header of CONVENTION_HEADERS) {
		if (source.startsWith(header)) return header.length;
		if (source.startsWith(header.trimEnd())) return header.trimEnd().length;
	}
	return 0;
}

// Matches a full tweak comment span: begin marker, highlighted content,
// end marker with timestamp + body. Lazy matching is safe because the body
// cannot contain `-->` (escaped at write time).
const FULL_RE = /<!--tweak:begin:([A-Za-z0-9_-]+)-->([\s\S]*?)<!--tweak:end:\1 @(\S+)\s([\s\S]*?)-->/g;
const BLOCK_RE = /<!--tweak:block:([A-Za-z0-9_-]+) @(\S+)\s([\s\S]*?)-->\r?\n?/g;
const ITEM_RE = /^([ \t]*)<!--tweak:item:([A-Za-z0-9_-]+) @(\S+)( eof)?\r?\n([\s\S]*?)-->\r?\n?/gm;

/** Escape the only sequence that would break the enclosing HTML comment. */
function escapeBody(body: string): string {
	return body.replaceAll("-->", "--&gt;");
}

/** Reverse escapeBody. */
function unescapeBody(body: string): string {
	return body.replaceAll("--&gt;", "-->");
}

/** Serialize a comment into its inline marker form (does not insert into source). */
export function serializeTweakComment(c: TweakComment): string {
	return `<!--tweak:begin:${c.id}-->${c.highlighted}<!--tweak:end:${c.id} @${c.createdAt}\n${escapeBody(c.comment)}-->`;
}

/** Parse all tweak comments from a markdown source, in document order.
 *  `blocks` may carry a `findTweakCommentBlocks(source)` result the caller
 *  already holds, so the document is not lexed twice. */
export function parseTweakComments(source: string, blocks?: TweakCommentBlock[]): TweakComment[] {
	const indexed = [
		...indexInlineComments(source),
		...indexBlockComments(source, blocks),
		...indexItemComments(source, blocks),
	];
	return indexed.sort((a, b) => a.index - b.index).map(({ comment }) => comment);
}

/** Inline (begin/end) comments only. Never lexes the document, so it is cheap
 *  enough for a render path that re-runs on every streaming tick. */
export function parseInlineTweakComments(source: string): TweakComment[] {
	return indexInlineComments(source).map(({ comment }) => comment);
}

type IndexedComment = { index: number; comment: TweakComment };

function indexInlineComments(source: string): IndexedComment[] {
	const indexed: IndexedComment[] = [];
	FULL_RE.lastIndex = 0;
	let match: RegExpExecArray | null;
	while ((match = FULL_RE.exec(source)) !== null) {
		const [, id, highlighted, createdAt, body] = match;
		indexed.push({
			index: match.index,
			comment: { id, highlighted, comment: unescapeBody(body), createdAt },
		});
	}
	return indexed;
}

function indexBlockComments(source: string, knownBlocks?: TweakCommentBlock[]): IndexedComment[] {
	// Without a marker there is nothing to anchor, and lexing is the costly part.
	if (!source.includes("<!--tweak:block:")) return [];
	const blocks = knownBlocks ?? findTweakCommentBlocks(source);
	const indexed: IndexedComment[] = [];
	BLOCK_RE.lastIndex = 0;
	let match: RegExpExecArray | null;
	while ((match = BLOCK_RE.exec(source)) !== null) {
		const [, id, createdAt, body] = match;
		const block = blocks.find((candidate) => candidate.start >= BLOCK_RE.lastIndex);
		indexed.push({
			index: match.index,
			comment: {
				id,
				highlighted: block ? source.slice(block.start, block.end) : "",
				comment: unescapeBody(body),
				createdAt,
				anchor: "block",
				sourceStart: block?.start,
				sourceEnd: block?.end,
			},
		});
	}
	return indexed;
}

function indexItemComments(source: string, knownBlocks?: TweakCommentBlock[]): IndexedComment[] {
	if (!source.includes("<!--tweak:item:")) return [];
	const items = (knownBlocks ?? findTweakCommentBlocks(source)).filter((block) => block.tag === "LI");
	const indexed: IndexedComment[] = [];
	ITEM_RE.lastIndex = 0;
	let match: RegExpExecArray | null;
	while ((match = ITEM_RE.exec(source)) !== null) {
		const [, indent, id, createdAt, , rawBody] = match;
		const markerStart = match.index;
		const item = items.filter((candidate) => candidate.end <= markerStart).at(-1);
		indexed.push({
			index: match.index,
			comment: {
				id,
				highlighted: item ? source.slice(item.start, item.end) : "",
				comment: unescapeBody(
					rawBody
						.split(/\r?\n/)
						.map((line) => (line.startsWith(indent) ? line.slice(indent.length) : line))
						.join("\n"),
				),
				createdAt,
				anchor: "block",
				sourceStart: item?.start,
				sourceEnd: item?.end,
			},
		});
	}
	return indexed;
}

function tokenTag(token: ReturnType<typeof marked.lexer>[number]): TweakCommentBlock["tag"] | null {
	switch (token.type) {
		case "paragraph":
			return "P";
		case "heading":
			return `H${token.depth}` as TweakCommentBlock["tag"];
		case "list":
			return token.ordered ? "OL" : "UL";
		case "blockquote":
			return "BLOCKQUOTE";
		case "code":
			return "PRE";
		case "table":
			return "TABLE";
		default:
			return null;
	}
}

/**
 * Build the visible markdown plus an index back to the raw source. Line endings
 * are normalized the way `marked.lexer` does it (`\r\n` and a lone `\r` become
 * `\n`), so the lexer's `raw` lengths add up to offsets into `visible`.
 */
function visibleSourceIndex(source: string): { visible: string; map: number[] } {
	const shown = buildVisibilityMask(source);
	const chars: string[] = [];
	const map: number[] = [];
	for (let i = 0; i < source.length; i++) {
		if (!shown[i]) continue;
		if (source[i] === "\r") {
			if (source[i + 1] === "\n" && shown[i + 1]) continue; // the `\n` stands for the pair
			chars.push("\n");
			map.push(i);
			continue;
		}
		chars.push(source[i]);
		map.push(i);
	}
	return { visible: chars.join(""), map };
}

/** Exact raw-source ranges for the top-level Markdown blocks rendered by marked. */
export function findTweakCommentBlocks(source: string): TweakCommentBlock[] {
	const { visible, map } = visibleSourceIndex(source);
	const tokens = marked.lexer(visible);
	const blocks: TweakCommentBlock[] = [];
	let cursor = 0;
	for (const token of tokens) {
		const tokenStart = cursor;
		cursor += token.raw.length;
		const tag = tokenTag(token);
		if (!tag || token.raw.length === 0) continue;
		let visibleEnd = cursor;
		while (visibleEnd > tokenStart && /\s/.test(visible[visibleEnd - 1])) visibleEnd--;
		if (visibleEnd <= tokenStart || map[tokenStart] === undefined || map[visibleEnd - 1] === undefined) continue;
		blocks.push({ start: map[tokenStart], end: map[visibleEnd - 1] + 1, tag });
		if (token.type === "list" && "items" in token) {
			const lines = visible.slice(tokenStart, cursor).split("\n");
			const lineOffsets: number[] = [];
			let offset = tokenStart;
			for (const line of lines) {
				lineOffsets.push(offset);
				offset += line.length + 1;
			}
			const locate = (firstLine: string, from: number, to: number, indentMin = 0): number => {
				for (let i = from; i < to; i++) {
					const line = lines[i];
					if (line.trimStart() === firstLine.trimStart() && line.length - line.trimStart().length >= indentMin)
						return i;
				}
				return -1;
			};
			const visit = (list: Tokens.List, first: number, limit: number, indentMin: number) => {
				let scan = first;
				for (let index = 0; index < list.items.length; index++) {
					const item = list.items[index];
					const startLine = locate(item.raw.split("\n", 1)[0], scan, limit, indentMin);
					if (startLine < 0) continue;
					const indent = lines[startLine].length - lines[startLine].trimStart().length;
					let siblingLine = limit;
					for (let i = startLine + 1; i < limit; i++) {
						const line = lines[i];
						if (
							line.trimStart() === list.items[index + 1]?.raw.split("\n", 1)[0]?.trimStart() &&
							line.length - line.trimStart().length === indent
						) {
							siblingLine = i;
							break;
						}
					}
					const nested = item.tokens.filter((child): child is Tokens.List => child.type === "list");
					const childLine = nested.length
						? locate(nested[0].items[0]?.raw.split("\n", 1)[0] ?? "", startLine + 1, siblingLine, indent + 1)
						: -1;
					const ownLimit = childLine >= 0 ? childLine : siblingLine;
					let ownEnd = lineOffsets[ownLimit] ?? cursor;
					while (ownEnd > lineOffsets[startLine] && /\s/.test(visible[ownEnd - 1])) ownEnd--;
					if (map[lineOffsets[startLine]] !== undefined && map[ownEnd - 1] !== undefined) {
						blocks.push({ start: map[lineOffsets[startLine]], end: map[ownEnd - 1] + 1, tag: "LI" });
					}
					let nextChildLine = childLine;
					for (let childIndex = 0; childIndex < nested.length; childIndex++) {
						const childList = nested[childIndex];
						if (nextChildLine < 0) break;
						const following = nested[childIndex + 1];
						const childLimit = following
							? locate(following.items[0]?.raw.split("\n", 1)[0] ?? "", nextChildLine + 1, siblingLine, indent + 1)
							: siblingLine;
						visit(childList, nextChildLine, childLimit < 0 ? siblingLine : childLimit, indent + 1);
						nextChildLine = childLimit;
					}
					scan = siblingLine;
				}
			};
			visit(token as Tokens.List, 0, lines.length, 0);
		}
	}
	return blocks;
}

export function serializeTweakBlockComment(comment: TweakComment): string {
	return `<!--tweak:block:${comment.id} @${comment.createdAt}\n${escapeBody(comment.comment)}-->`;
}

/** Insert a comment marker immediately before an exact Markdown block range. */
export function insertTweakBlockComment(
	source: string,
	comment: TweakComment,
	block: Pick<TweakCommentBlock, "start" | "end">,
): string {
	if (block.start < 0 || block.end <= block.start || block.end > source.length) {
		throw new Error("insertTweakBlockComment: invalid source range");
	}
	if (source.slice(block.start, block.end) !== comment.highlighted) {
		throw new Error("insertTweakBlockComment: source range no longer matches the rendered block");
	}
	// Match the file's line endings so a CRLF file does not gain a lone LF line.
	const eol = source.includes("\r\n") ? "\r\n" : "\n";
	if (
		findTweakCommentBlocks(source).some(
			(candidate) => candidate.tag === "LI" && candidate.start === block.start && candidate.end === block.end,
		)
	) {
		const line = source.slice(
			source.lastIndexOf("\n", block.start - 1) + 1,
			source.indexOf("\n", block.start) < 0 ? source.length : source.indexOf("\n", block.start),
		);
		const prefix = /^(\s*(?:[-*+]|\d+[.)])\s+)/.exec(line)?.[1] ?? "  ";
		const indent = " ".repeat(prefix.length);
		const hadLineEnding = source.slice(block.end).startsWith(eol);
		const marker = `${indent}<!--tweak:item:${comment.id} @${comment.createdAt}${hadLineEnding ? "" : " eof"}${eol}${comment.comment
			.split("\n")
			.map((part) => indent + escapeBody(part))
			.join(eol)}-->${eol}`;
		const afterLine = hadLineEnding ? block.end + eol.length : block.end;
		return ensureConventionHeader(
			source.slice(0, afterLine) + (afterLine === block.end ? eol : "") + marker + source.slice(afterLine),
		);
	}
	const marker = `${serializeTweakBlockComment(comment)}${eol}`;
	return ensureConventionHeader(source.slice(0, block.start) + marker + source.slice(block.start));
}

/** Prepend the convention header if not already present. */
export function ensureConventionHeader(source: string): string {
	// Also treat a stripped (whitespace-trimmed) match as present to be resilient
	// to trailing-newline normalization by editors.
	if (CONVENTION_HEADERS.some((header) => source.includes(header.trimEnd()))) return source;
	return CONVENTION_HEADER + source;
}

/** Remove the convention header if present (used when last comment is removed). */
function removeConventionHeader(source: string): string {
	const prefixLength = conventionHeaderPrefixLength(source);
	if (prefixLength > 0) {
		let end = prefixLength;
		while (end < source.length && (source[end] === "\n" || source[end] === "\r")) end++;
		return source.slice(end);
	}
	return source;
}

// Inline emphasis/code/strikethrough markers that markdown renders away, so they
// are absent from a DOM text selection. We strip them (and collapse whitespace)
// when matching a rendered selection back to the raw source.
// DEFERRED (2026-06-01) — link syntax `[text](url)` is not normalized: the
// rendered selection shows "text" while the source keeps "(url)", so a selection
// spanning a link still fails to match. Needs a tokenizer-aware pass; rare enough
// to defer until a real case shows up.
const INLINE_MARKERS_RE = /[*_`~]/g;

/** Normalize a rendered selection for matching: drop inline markers, collapse whitespace. */
export function normalizeForMatch(text: string): string {
	return text.replace(INLINE_MARKERS_RE, "").replace(/\s+/g, " ").trim();
}

/**
 * Build a boolean mask over `source`, `false` for every character that is NOT
 * visible in the rendered DOM: the convention header and every tweak marker's
 * syntax (begin marker, and the end marker including its comment body). The
 * highlighted text BETWEEN a begin/end pair stays visible. Handles nested
 * markers because each marker is masked independently.
 *
 * This makes source-side occurrence counting align with the rendered DOM, which
 * is where the caller's `occurrenceIndex` is measured — the header/markers/bodies
 * are absent from the DOM, so they must not shift ordinals.
 */
function buildVisibilityMask(source: string): boolean[] {
	const visible = new Array<boolean>(source.length).fill(true);
	const hide = (start: number, end: number) => {
		for (let i = start; i < end && i < source.length; i++) visible[i] = false;
	};
	// Convention header (exact or trailing-trimmed).
	hide(0, conventionHeaderPrefixLength(source));
	// Marker syntax — begin markers and end markers (bodies included).
	const beginRe = /<!--tweak:begin:[A-Za-z0-9_-]+-->/g;
	const endRe = /<!--tweak:end:[A-Za-z0-9_-]+ @\S+\s[\s\S]*?-->/g;
	for (const re of [beginRe, endRe]) {
		re.lastIndex = 0;
		let m: RegExpExecArray | null;
		while ((m = re.exec(source)) !== null) hide(m.index, m.index + m[0].length);
	}
	BLOCK_RE.lastIndex = 0;
	let blockMatch: RegExpExecArray | null;
	while ((blockMatch = BLOCK_RE.exec(source)) !== null) hide(blockMatch.index, blockMatch.index + blockMatch[0].length);
	ITEM_RE.lastIndex = 0;
	while ((blockMatch = ITEM_RE.exec(source)) !== null) hide(blockMatch.index, blockMatch.index + blockMatch[0].length);
	return visible;
}

/** Hide tweak syntax without changing source line/column coordinates. */
export function maskTweakCommentSyntax(source: string): string {
	const visible = buildVisibilityMask(source);
	let masked = "";
	for (let i = 0; i < source.length; i++) {
		const char = source[i];
		masked += visible[i] || char === "\n" || char === "\r" ? char : " ";
	}
	return masked;
}

/**
 * Build a marker-stripped, whitespace-collapsed view of `source` alongside a map
 * from each normalized-string index to its originating source offset. This lets a
 * match found in the normalized view be translated back to a slice of the raw source.
 * Characters hidden by `buildVisibilityMask` (header, marker syntax, comment bodies)
 * are skipped so the normalized view matches the rendered DOM text.
 */
function buildNormalizedIndex(source: string): { normalized: string; map: number[] } {
	const chars: string[] = [];
	const map: number[] = [];
	const visible = buildVisibilityMask(source);
	let prevWasSpace = false;
	for (let i = 0; i < source.length; i++) {
		if (!visible[i]) continue; // header / marker syntax / body — not in the DOM
		const ch = source[i];
		if (ch === "*" || ch === "_" || ch === "`" || ch === "~") continue; // invisible inline marker
		if (/\s/.test(ch)) {
			if (prevWasSpace) continue; // collapse runs of whitespace to one space
			chars.push(" ");
			map.push(i);
			prevWasSpace = true;
			continue;
		}
		chars.push(ch);
		map.push(i);
		prevWasSpace = false;
	}
	return { normalized: chars.join(""), map };
}

/** Index of the `n`-th (0-based) occurrence of `needle` in `hay`, or -1. */
function nthIndexOf(hay: string, needle: string, n: number): number {
	let idx = -1;
	for (let k = 0; k <= n; k++) {
		idx = hay.indexOf(needle, idx + 1);
		if (idx === -1) return -1;
	}
	return idx;
}

/**
 * Locate `selection` (text taken from the rendered DOM) within the raw markdown
 * `source`, returning the source offsets to wrap. Matching is done against a
 * normalized view that ignores inline markdown formatting and whitespace
 * differences (line wraps, `**bold**`, `*italic*`, `` `code` ``, `~~strike~~`)
 * and skips invisible regions (convention header, marker syntax, comment bodies).
 *
 * `occurrenceIndex` selects WHICH occurrence to anchor when the selected text
 * appears multiple times in the document — it is the 0-based ordinal of the
 * user's actual selection among identical rendered occurrences (measured in the
 * DOM by the caller). Without it, a repeated word ("reason" ×18) would always
 * anchor to the first occurrence, leaving the user's real selection unhighlighted.
 * Returns null when the requested occurrence cannot be located.
 */
export function findSourceMatch(
	source: string,
	selection: string,
	occurrenceIndex = 0,
): { start: number; end: number } | null {
	const normSel = normalizeForMatch(selection);
	if (!normSel) return null;
	const { normalized, map } = buildNormalizedIndex(source);
	const nIdx = nthIndexOf(normalized, normSel, occurrenceIndex);
	if (nIdx === -1) return null;
	// Map the normalized [start, last] back to raw-source offsets. The selection is
	// trimmed, so its last normalized char is non-whitespace and maps cleanly.
	const start = map[nIdx];
	const end = map[nIdx + normSel.length - 1] + 1;
	return { start, end };
}

/** Source offsets [start, end) of every existing tweak-comment marker span. */
function existingCommentSpans(source: string): Array<{ start: number; end: number }> {
	const spans: Array<{ start: number; end: number }> = [];
	FULL_RE.lastIndex = 0;
	let match: RegExpExecArray | null;
	while ((match = FULL_RE.exec(source)) !== null) {
		spans.push({ start: match.index, end: match.index + match[0].length });
	}
	return spans;
}

/** Source range of tweak syntax that is not part of the reader's text. */
export interface TweakSyntaxRegion {
	from: number;
	to: number;
}

/** One inline comment: its hidden begin/end syntax and the visible highlighted text between them. */
export interface TweakInlineSpan {
	id: string;
	comment: string;
	begin: TweakSyntaxRegion;
	highlight: TweakSyntaxRegion;
	end: TweakSyntaxRegion;
}

/**
 * Locate every piece of tweak syntax in `source` so an editor can hide it:
 * inline spans, and standalone regions (convention header, block and item
 * comments). Uses the same patterns as the parser, so what the editor hides is
 * exactly what the viewer strips.
 */
export function findTweakSyntax(source: string): { spans: TweakInlineSpan[]; standalone: TweakSyntaxRegion[] } {
	const spans: TweakInlineSpan[] = [];
	const standalone: TweakSyntaxRegion[] = [];
	const headerLength = conventionHeaderPrefixLength(source);
	if (headerLength > 0) standalone.push({ from: 0, to: headerLength });
	FULL_RE.lastIndex = 0;
	let match: RegExpExecArray | null;
	while ((match = FULL_RE.exec(source)) !== null) {
		const [whole, id, highlighted, , body] = match;
		const beginEnd = match.index + `<!--tweak:begin:${id}-->`.length;
		const highlightEnd = beginEnd + highlighted.length;
		spans.push({
			id,
			comment: unescapeBody(body),
			begin: { from: match.index, to: beginEnd },
			highlight: { from: beginEnd, to: highlightEnd },
			end: { from: highlightEnd, to: match.index + whole.length },
		});
	}
	for (const re of [BLOCK_RE, ITEM_RE]) {
		re.lastIndex = 0;
		while ((match = re.exec(source)) !== null)
			standalone.push({ from: match.index, to: match.index + match[0].length });
	}
	standalone.sort((a, b) => a.from - b.from);
	return { spans, standalone };
}

/**
 * Thrown by `insertTweakComment` when the selection overlaps an existing
 * comment. Nesting one `<!--tweak-->` span inside another is unrepresentable:
 * the lazy parser would swallow the inner comment into the outer's highlighted
 * text, so it saves to disk but never parses or renders again (looks like it
 * "didn't persist"). Callers surface this as a distinct, actionable message.
 */
export class OverlappingCommentError extends Error {}

/**
 * Insert a tweak comment into the source by wrapping the source span that the
 * `highlighted` selection corresponds to with begin/end markers. The wrapped
 * span uses the RAW source text (markers included) so the rendered output and
 * round-trip removal stay correct. Throws if the text cannot be located, or an
 * `OverlappingCommentError` if it overlaps an existing comment (nesting is not
 * supported — comment on plain text, or edit the existing comment instead).
 */
export function insertTweakComment(source: string, comment: TweakComment, occurrenceIndex = 0): string {
	const match = findSourceMatch(source, comment.highlighted, occurrenceIndex);
	if (!match) {
		throw new Error(
			`insertTweakComment: highlighted text not found in source: "${comment.highlighted.slice(0, 40)}..."`,
		);
	}
	// Reject a span that intersects an existing comment — nesting corrupts both.
	const overlaps = existingCommentSpans(source).some((sp) => match.start < sp.end && sp.start < match.end);
	if (overlaps) {
		throw new OverlappingCommentError(
			`insertTweakComment: selection overlaps an existing comment: "${comment.highlighted.slice(0, 40)}..."`,
		);
	}
	const sourceSlice = source.slice(match.start, match.end);
	const wrapped = serializeTweakComment({ ...comment, highlighted: sourceSlice });
	const replaced = source.slice(0, match.start) + wrapped + source.slice(match.end);
	return ensureConventionHeader(replaced);
}

/** Remove a comment by id, keeping the highlighted text in place. */
export function removeTweakComment(source: string, id: string): string {
	const escapedId = id.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
	const re = new RegExp(
		`<!--tweak:begin:${escapedId}-->([\\s\\S]*?)<!--tweak:end:${escapedId} @\\S+\\s[\\s\\S]*?-->`,
		"g",
	);
	let changed = false;
	let out = source.replace(re, (_, highlighted) => {
		changed = true;
		return highlighted;
	});
	const blockRe = new RegExp(`<!--tweak:block:${escapedId} @\\S+\\s[\\s\\S]*?-->\\r?\\n?`, "g");
	out = out.replace(blockRe, () => {
		changed = true;
		return "";
	});
	const itemRe = new RegExp(
		`(\\r?\\n)([ \\t]*)<!--tweak:item:${escapedId} @\\S+( eof)?\\r?\\n[\\s\\S]*?-->\\r?\\n?`,
		"g",
	);
	out = out.replace(itemRe, (_, precedingEol, _indent, eof) => {
		changed = true;
		return eof ? "" : precedingEol;
	});
	if (!changed) return source;
	if (parseTweakComments(out).length === 0) {
		return removeConventionHeader(out);
	}
	return out;
}

/** Update the comment text of an existing tweak comment, preserving its id and highlighted text. */
export function updateTweakComment(source: string, id: string, newComment: string): string {
	const escapedId = id.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
	const re = new RegExp(
		`<!--tweak:begin:${escapedId}-->([\\s\\S]*?)<!--tweak:end:${escapedId} @(\\S+)\\s[\\s\\S]*?-->`,
		"g",
	);
	let out = source.replace(re, (_, highlighted, createdAt) =>
		serializeTweakComment({ id, highlighted, comment: newComment, createdAt }),
	);
	const blockRe = new RegExp(`<!--tweak:block:${escapedId} @(\\S+)\\s[\\s\\S]*?-->`, "g");
	out = out.replace(blockRe, (_, createdAt) =>
		serializeTweakBlockComment({ id, highlighted: "", comment: newComment, createdAt, anchor: "block" }),
	);
	const itemRe = new RegExp(`^([ \\t]*)<!--tweak:item:${escapedId} @(\\S+)( eof)?\\r?\\n[\\s\\S]*?-->`, "gm");
	out = out.replace(itemRe, (_, indent, createdAt, eof) => {
		const eol = source.includes("\r\n") ? "\r\n" : "\n";
		return `${indent}<!--tweak:item:${id} @${createdAt}${eof ?? ""}${eol}${newComment
			.split("\n")
			.map((part) => indent + escapeBody(part))
			.join(eol)}-->`;
	});
	return out;
}

// Private-use Unicode delimiters used to mark a highlight's begin/end boundaries
// in the source BEFORE markdown parsing. They are plain text to `marked` (they do
// not affect emphasis flanking) and survive DOMPurify, so after rendering they can
// be located in the DOM and replaced with `<span class="tweak-highlight">` wrappers.
// Rendering the highlight via the DOM (instead of injecting spans into the source)
// keeps inline formatting intact even when a selection straddles a `**bold**` edge.
const SENTINEL_BEGIN = "\uE000";
const SENTINEL_END = "\uE001";

/** Begin delimiter for a highlight, e.g. `<id>`. */
export function tweakBeginSentinel(id: string): string {
	return `${SENTINEL_BEGIN}${id}${SENTINEL_BEGIN}`;
}

/** End delimiter for a highlight, e.g. `<id>`. */
export function tweakEndSentinel(id: string): string {
	return `${SENTINEL_END}${id}${SENTINEL_END}`;
}

/**
 * Pre-process markdown source before passing to `marked`: strips the convention
 * header and replaces each tweak marker pair with begin/end sentinel delimiters,
 * leaving the highlighted text (with its markdown formatting) inline so `marked`
 * renders it normally. The sentinels are turned into highlight spans afterwards by
 * `applyTweakDomHighlights` operating on the rendered DOM.
 */
export function injectTweakSentinels(source: string): string {
	let out = source.slice(conventionHeaderPrefixLength(source));
	FULL_RE.lastIndex = 0;
	out = out.replace(FULL_RE, (_, id, highlighted) => `${tweakBeginSentinel(id)}${highlighted}${tweakEndSentinel(id)}`);
	BLOCK_RE.lastIndex = 0;
	out = out.replace(BLOCK_RE, "");
	ITEM_RE.lastIndex = 0;
	out = out.replace(ITEM_RE, "");
	return out;
}

/** Generate a unique id for a new comment (short, URL-safe). */
export function generateTweakCommentId(): string {
	const rand = Math.random().toString(36).slice(2, 8);
	const ts = Date.now().toString(36);
	return `c_${ts}${rand}`;
}

// ---- GFM Task-List Checkbox Toggle ----

/**
 * Set the checkbox on the given source line to the specified mark.
 * `sourceLine` is the 0-based line number in the raw markdown source,
 * injected as `data-source-line` by the ContentRenderer preprocessor.
 * `mark` is one of: `" "` (unchecked), `"x"` (checked), `"~"` (in-progress).
 *
 * `sourceCol` addresses one exact `[` on the line and is supplied for whole-cell
 * checkboxes in tables, where a single row can carry several. Without it the
 * leading list-item checkbox is rewritten, which is the only one a list line has.
 */
export function toggleCheckbox(source: string, sourceLine: number, mark: " " | "x" | "~", sourceCol?: number): string {
	const lines = source.split("\n");
	if (sourceLine < 0 || sourceLine >= lines.length) return source;
	const line = lines[sourceLine];
	if (sourceCol != null) {
		// Refuse unless the column really holds a `[x]`: a stale coordinate from a
		// source edited under the rendered DOM must not corrupt an unrelated span.
		if (line[sourceCol] !== "[" || line[sourceCol + 2] !== "]") return source;
		if (!/^[ xX~]$/.test(line[sourceCol + 1] ?? "")) return source;
		lines[sourceLine] = `${line.slice(0, sourceCol)}[${mark}]${line.slice(sourceCol + 3)}`;
		return lines.join("\n");
	}
	const m = /^(\s*[-*+]\s+)\[([ xX~])\]/.exec(line);
	if (!m) return source;
	lines[sourceLine] = `${m[1]}[${mark}]${line.slice(m[0].length)}`;
	return lines.join("\n");
}
