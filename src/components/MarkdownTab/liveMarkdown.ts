import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import type { LanguageSupport } from "@codemirror/language";
import { languages } from "@codemirror/language-data";
import {
	Annotation,
	type Range as CmRange,
	EditorState,
	type Extension,
	StateField,
	Transaction,
	type TransactionSpec,
} from "@codemirror/state";
import { Decoration, type DecorationSet, EditorView } from "@codemirror/view";
import {
	findSourceMatch,
	findTweakSyntax,
	insertTweakComment,
	type TweakComment,
	type TweakInlineSpan,
	type TweakSyntaxRegion,
} from "../../utils/tweakComments";

export interface Range {
	from: number;
	to: number;
}

export interface DocEdit {
	from: number;
	to: number;
	insert: string;
}

/** Above this size the editor is plain source: every decoration pass walks the whole tree. */
const LIVE_MAX_CHARS = 500 * 1024;

/** GFM markdown with nested code-block languages, so fenced code keeps its highlighting. */
export function loadMarkdownLanguage(): LanguageSupport {
	return markdown({ base: markdownLanguage, codeLanguages: languages });
}

/** The text outside tweak syntax. The viewer's writer emits LF inside comments whatever the
 *  file uses, so those bytes say nothing about the file's own line endings. */
function withoutTweakSyntax(text: string): string {
	if (!text.includes("<!--tweak") && !text.includes("<!-- tweak-comments")) return text;
	const { spans, standalone } = findTweakSyntax(text);
	const regions = [...spans.flatMap((s) => [s.begin, s.end]), ...standalone].sort((a, b) => b.from - a.from);
	let rest = text;
	for (const r of regions) rest = rest.slice(0, r.from) + rest.slice(r.to);
	return rest;
}

/**
 * True when Live mode can save the file byte for byte. CodeMirror keeps one line
 * separator per document, so mixed endings (or a lone CR) would be rewritten.
 */
export function liveModeSupported(text: string): boolean {
	if (text.length > LIVE_MAX_CHARS) return false;
	const rest = withoutTweakSyntax(text);
	if (/\r(?!\n)/.test(rest)) return false;
	return !(rest.includes("\r\n") && /(?<!\r)\n/.test(rest));
}

/** Pin the separator to the file's own so `state.sliceDoc()` reproduces it. Also stops CM
 *  from splitting on U+2028/U+2029, which it would otherwise turn into newlines. */
export function liveLineSeparator(text: string): Extension {
	return EditorState.lineSeparator.of(withoutTweakSyntax(text).includes("\r\n") ? "\r\n" : "\n");
}

interface LiveValue {
	decorations: DecorationSet;
	/** Tweak syntax, always hidden and atomic. */
	tweakRegions: Range[];
	tweakAtomic: DecorationSet;
	spans: TweakInlineSpan[];
	standalone: TweakSyntaxRegion[];
}

const hide = Decoration.replace({});

function buildLive(state: EditorState): LiveValue {
	const source = state.doc.toString();
	const { spans, standalone } = findTweakSyntax(source);
	const tweakRegions: Range[] = [...spans.flatMap((s) => [s.begin, s.end]), ...standalone].sort(
		(a, b) => a.from - b.from,
	);
	const tweakAtomic = Decoration.set(tweakRegions.map((r) => hide.range(r.from, r.to)));

	const decos: CmRange<Decoration>[] = tweakRegions.map((r) => hide.range(r.from, r.to));
	for (const s of spans) {
		if (s.highlight.to > s.highlight.from) {
			decos.push(
				Decoration.mark({ class: "tweak-highlight", attributes: { title: s.comment } }).range(
					s.highlight.from,
					s.highlight.to,
				),
			);
		}
	}
	return { decorations: Decoration.set(decos, true), tweakRegions, tweakAtomic, spans, standalone };
}

const liveField = StateField.define<LiveValue>({
	create: buildLive,
	update(value, tr) {
		return tr.docChanged ? buildLive(tr.state) : value;
	},
	provide: (f) => EditorView.decorations.from(f, (v) => v.decorations),
});

export function liveDecorations(state: EditorState): DecorationSet {
	return state.field(liveField).decorations;
}

/** Tweak comment syntax: hidden regardless of the cursor, and atomic. */
export function tweakHiddenRanges(state: EditorState): Range[] {
	return state.field(liveField).tweakRegions;
}

/**
 * Rewrite edits (positions in the old document) so no edit leaves half a tweak
 * comment behind.
 *  - An edit that covers all the highlighted text of a comment grows to remove
 *    the whole comment, markers and body included.
 *  - A bare delete of a hidden marker (Backspace/Delete beside a comment) deletes the
 *    adjacent highlighted character instead.
 *  - Any other edit is trimmed so it never cuts into marker syntax, which is
 *    hidden and cannot be edited by hand. Standalone comments (block, item,
 *    convention header) are removed only when an edit covers them entirely.
 */
export function protectTweakEdits(
	edits: readonly DocEdit[],
	spans: readonly TweakInlineSpan[],
	standalone: readonly TweakSyntaxRegion[],
): DocEdit[] {
	const out: DocEdit[] = [];
	for (const edit of edits) {
		let { from, to } = edit;
		if (from === to) {
			const inside = [...spans.flatMap((s) => [s.begin, s.end]), ...standalone].find(
				(r) => from > r.from && from < r.to,
			);
			out.push({ from: inside ? inside.from : from, to: inside ? inside.from : to, insert: edit.insert });
			continue;
		}
		// Backspace/Delete beside a comment targets its hidden marker; act on the nearest highlighted char instead.
		if (!edit.insert) {
			for (const s of spans) {
				if (s.highlight.to === s.highlight.from) continue;
				if (from === s.end.from && to === s.end.to) [from, to] = [s.highlight.to - 1, s.highlight.to];
				else if (from === s.begin.from && to === s.begin.to) [from, to] = [s.highlight.from, s.highlight.from + 1];
			}
		}
		for (let grown = true; grown; ) {
			grown = false;
			for (const s of spans) {
				const covers = s.highlight.to > s.highlight.from && from <= s.highlight.from && to >= s.highlight.to;
				if (covers && (from > s.begin.from || to < s.end.to)) {
					from = Math.min(from, s.begin.from);
					to = Math.max(to, s.end.to);
					grown = true;
				}
			}
		}
		const protectedRegions: TweakSyntaxRegion[] = [];
		for (const s of spans) {
			if (from <= s.begin.from && to >= s.end.to) continue;
			protectedRegions.push(s.begin, s.end);
		}
		for (const r of standalone) {
			if (!(from <= r.from && to >= r.to)) protectedRegions.push(r);
		}
		const pieces: Range[] = [];
		let cursor = from;
		for (const r of protectedRegions.filter((p) => p.from < to && from < p.to).sort((a, b) => a.from - b.from)) {
			if (r.from > cursor) pieces.push({ from: cursor, to: r.from });
			cursor = Math.max(cursor, r.to);
		}
		if (cursor < to) pieces.push({ from: cursor, to });
		if (pieces.length === 0) {
			if (edit.insert) out.push({ from: cursor, to: cursor, insert: edit.insert });
			continue;
		}
		pieces.forEach((p, i) => out.push({ from: p.from, to: p.to, insert: i === 0 ? edit.insert : "" }));
	}
	return out;
}

/** Marks a transaction the editor built itself (add comment); the guard leaves it alone. */
const trusted = Annotation.define<boolean>();

const tweakGuard = EditorState.transactionFilter.of((tr): TransactionSpec | readonly TransactionSpec[] => {
	if (!tr.docChanged || tr.annotation(trusted)) return tr;
	const event = tr.annotation(Transaction.userEvent);
	if (!event || event.startsWith("undo") || event.startsWith("redo")) return tr;
	const { spans, standalone } = tr.startState.field(liveField);
	if (spans.length === 0 && standalone.length === 0) return tr;
	const edits: DocEdit[] = [];
	tr.changes.iterChanges((from, to, _fb, _tb, inserted) => edits.push({ from, to, insert: inserted.toString() }));
	const safe = protectTweakEdits(edits, spans, standalone);
	const same = safe.length === edits.length && safe.every((e, i) => e.from === edits[i].from && e.to === edits[i].to);
	if (same) return tr;
	return { changes: safe, annotations: Transaction.userEvent.of(event), scrollIntoView: true };
});

/**
 * The preview's look, read from the same tokens as `markdown-content.css`
 * (a test compares the two). Headings are line decorations, so the rule under h1/h2 spans the line.
 */
const liveTheme = EditorView.baseTheme({
	".tweak-highlight": {
		background: "color-mix(in srgb, var(--tweak-highlight) 25%, transparent)",
		borderBottom: "1.5px solid color-mix(in srgb, var(--tweak-highlight) 70%, transparent)",
	},
	".tweak-highlight:hover": { background: "color-mix(in srgb, var(--tweak-highlight) 40%, transparent)" },
});

/** Tweak-comment presentation for the block editor: hidden atomic markers, highlight, edit guard. */
export function liveMarkdown(): Extension {
	return [
		liveField,
		EditorView.atomicRanges.of((view) => view.state.field(liveField).tweakAtomic),
		tweakGuard,
		liveTheme,
	];
}

/**
 * Wrap the main selection in a tweak comment with the viewer's own writer
 * (`insertTweakComment`), so the file format has a single producer. Throws
 * `OverlappingCommentError` when the selection touches an existing comment.
 */
export function addTweakCommentAtSelection(view: EditorView, comment: Omit<TweakComment, "highlighted">): void {
	const { from, to } = view.state.selection.main;
	if (from === to) return;
	const source = view.state.sliceDoc();
	const highlighted = view.state.sliceDoc(from, to);
	// `source` spells a line break with two characters in a CRLF file, the document with one.
	const sourceFrom = view.state.lineBreak === "\r\n" ? from + view.state.doc.lineAt(from).number - 1 : from;
	// `insertTweakComment` counts occurrences the way the viewer's DOM does; find the one at the selection.
	let occurrence = 0;
	for (let n = 0; ; n++) {
		const match = findSourceMatch(source, highlighted, n);
		if (!match) break;
		occurrence = n;
		if (match.start >= sourceFrom) break;
	}
	const updated = insertTweakComment(source, { ...comment, highlighted }, occurrence);
	// Diff in CodeMirror's coordinates (one character per line break, whatever the file uses).
	const crlf = view.state.lineBreak === "\r\n";
	const before = view.state.doc.toString();
	const after = updated.replaceAll("\r\n", "\n");
	let start = 0;
	while (start < before.length && before[start] === after[start]) start++;
	let end = 0;
	while (end < before.length - start && before.at(-1 - end)! === after.at(-1 - end)!) end++;
	const insert = after.slice(start, after.length - end);
	view.dispatch({
		changes: { from: start, to: before.length - end, insert: crlf ? insert.replaceAll("\n", "\r\n") : insert },
		annotations: [trusted.of(true), Transaction.userEvent.of("input.tweak")],
	});
}
