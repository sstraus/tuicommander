/** Interpret TUIC markers in ACP answer text after streamed chunks are joined.
 *
 * The terminal's Rust parser reads VT rows and their wrap state. ACP supplies
 * markdown text in the browser as well as Tauri, so this projection handles
 * logical lines and leaves fenced examples alone. See docs/backend/output-parser.md.
 */
import { ANSWER_MARKER_RE } from "../Terminal/suggestOverlay";

export interface ChatProtocolText {
	body: string;
	intent: { text: string; title: string | null } | null;
	suggestions: string[];
}

const ACK = /^TUICommander[\t ]+v[0-9][^\s]*[\t ]+is[\t ]+connected\.[\t ]*/;
const BULLET = "(?:[●⏺•◦][\\t ]+)?";
const INTENT = new RegExp(`^[\\t ]*${BULLET}intent:[\\t ]+(.+)$`);
const SUGGEST = new RegExp(`^[\\t ]*${BULLET}suggest:[\\t ]*\\[([^\\[\\]\\r\\n]*)\\][\\t ]*$`);
const TRAILING_SUGGEST = /[\t ]+suggest:[\t ]*\[([^[\]\r\n]*)\][\t ]*$/;
// Only consecutive parenthesized groups belong to the marker prefix.
// Prose after its title is reply text, even when that prose ends in parentheses.
const TITLE = /^([^()]+(?:\([^()]+\)[\t ]+)*)\(([^()]+)\)(.*)$/;

export function projectChatProtocolText(text: string): ChatProtocolText {
	const body: string[] = [];
	const lines = text.split(/\r?\n/);
	let lastContentIndex = lines.length - 1;
	while (lastContentIndex >= 0 && !lines[lastContentIndex].trim()) lastContentIndex -= 1;
	let intent: ChatProtocolText["intent"] = null;
	let suggestions: string[] = [];
	let fence: "`" | "~" | null = null;
	for (const [index, original] of lines.entries()) {
		const fenceMatch = /^[ \t]{0,3}(`{3,}|~{3,})/.exec(original);
		if (fenceMatch) {
			const marker = fenceMatch[1][0] as "`" | "~";
			if (!fence || fence === marker) fence = fence ? null : marker;
			body.push(original);
			continue;
		}
		if (fence) {
			body.push(original);
			continue;
		}
		if (/^(?: {4}|\t)/.test(original)) {
			body.push(original);
			continue;
		}
		// The acknowledgement can precede the first intent on the same line.
		const line = index === 0 ? original.replace(ACK, "") : original;
		const intentMatch = INTENT.exec(line);
		if (intentMatch) {
			const raw = intentMatch[1].trim();
			const titleMatch = TITLE.exec(raw);
			// Unbalanced or nested marker parentheses have no reliable title boundary.
			if (!titleMatch && /[()]/.test(raw)) {
				body.push(line);
				continue;
			}
			const description = (titleMatch?.[1] ?? raw).trim();
			if (description.length >= 3 && description !== "...") {
				intent = { text: description, title: titleMatch?.[2].trim() || null };
				if (titleMatch?.[3]?.trim()) body.push(titleMatch[3].trimStart());
				continue;
			}
		}
		const standaloneSuggest = SUGGEST.exec(line);
		const suggestMatch = standaloneSuggest ?? (index === lastContentIndex ? TRAILING_SUGGEST.exec(line) : null);
		if (suggestMatch) {
			const items = suggestMatch[1]
				.split("|")
				.map((item) => item.trim())
				.filter(Boolean);
			if (items.length >= 2 && items.length <= 4) {
				suggestions = items;
				if (!standaloneSuggest) body.push(line.slice(0, suggestMatch.index).trimEnd());
				continue;
			}
		}
		// Blank lines stay: they separate a paragraph from the list before it. An answer marker starts
		// its own paragraph, as it starts its own row in the grid, even when no blank line precedes it.
		if (ANSWER_MARKER_RE.test(line) && body.length > 0 && body.at(-1)!.trim()) body.push("");
		body.push(line);
	}
	return { body: body.join("\n").replace(/^\n+|\n+$/g, ""), intent, suggestions };
}
