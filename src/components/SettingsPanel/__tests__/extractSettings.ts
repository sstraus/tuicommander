/** Static extraction of the settings a tab renders, straight from its source.
 *
 * The search index in `settingsSearchIndex.ts` is a committed file, so it could
 * drift from the tabs it describes. This module re-derives the same data from
 * the JSX and the drift test compares the two — that is the whole anti-drift
 * mechanism (see the design note in `settingsSearchIndex.ts`).
 *
 * Extraction rule, deliberately mechanical:
 *   - every `<h3>` opens a section,
 *   - every `label=` prop and every `<label>` element is a setting inside the
 *     section opened by the nearest preceding `<h3>`,
 *   - text is read from `t("key", "Default")`, from a `{"literal"}` or from a
 *     leading plain-text run; anything else is dynamic and is only counted,
 *   - a setting between `<ExpertSetting configKey="…">` and its closing tag
 *     carries that configKey (the index marks it `expert`),
 *   - a section or setting inside `<Show when={isTauri()…}>` is `desktop`-only,
 *     one inside that Show's `fallback={…}` is `browser`-only.
 */

/** The client a gated occurrence renders in; absent means every client. */
export type ClientPlatform = "desktop" | "browser";

export interface ExtractedText {
	/** i18n key, when the text came from a `t()` call */
	key?: string;
	/** Default (English) text */
	text: string;
	platform?: ClientPlatform;
}

export interface ExtractedTab {
	sections: ExtractedText[];
	/** Settings, each tagged with the section heading text it sits under, and
	 * with the configKey of the `ExpertSetting` wrapping it, if any */
	settings: (ExtractedText & { section: string; configKey?: string })[];
	/** Occurrences whose text is computed at runtime and cannot be indexed */
	dynamic: number;
}

/** Index of the `>` that closes the open tag starting at `from`, brace-aware.
 *
 * `<label onClick={(e) => e.stopPropagation()}>` has a `>` inside an attribute
 * expression; a naive scan stops there and reads the arrow body as the label. */
function endOfOpenTag(src: string, from: number): number {
	let depth = 0;
	for (let i = from; i < src.length; i++) {
		const c = src[i];
		if (c === "{") depth++;
		else if (c === "}") depth--;
		else if (c === ">" && depth === 0) return i;
	}
	return -1;
}

const T_CALL = /^\{\s*t\(\s*"([^"]*)"\s*,\s*"((?:[^"\\]|\\.)*)"/;
const BRACED_LITERAL = /^\{\s*"((?:[^"\\]|\\.)*)"\s*\}/;
const BARE_LITERAL = /^"((?:[^"\\]|\\.)*)"/;
// A plain run followed by `{` continues into a runtime value — the rendered
// text is not the run, so it is dynamic rather than a truncated label.
const PLAIN = /^([^<{}]+)(?=<|$)/;

/** Read the statically-known text out of `t("k","V")`, `{"V"}` or `"V"`. */
function staticExpression(trimmed: string): ExtractedText | null {
	const call = trimmed.match(T_CALL);
	if (call) return { key: call[1], text: unescapeJsx(call[2]) };
	const braced = trimmed.match(BRACED_LITERAL);
	if (braced) return { text: unescapeJsx(braced[1]) };
	const bare = trimmed.match(BARE_LITERAL);
	if (bare) return { text: unescapeJsx(bare[1]) };
	return null;
}

/** Read the first statically-known text out of a JSX child run, or null.
 *
 * Children may be plain text (`<h3>Parameters</h3>`); a prop value may not —
 * accepting a plain run there would read arbitrary code as a label. */
export function staticText(inner: string, allowPlain: boolean): ExtractedText | null {
	let trimmed = inner.trim();
	// Toggle labels put the static caption after a self-closing input.
	if (/^<(?:input|Switch)\b/.test(trimmed)) {
		const end = endOfOpenTag(trimmed, 0);
		if (end >= 0 && trimmed[end - 1] === "/") {
			const caption = trimmed.slice(end + 1).trim();
			if (caption.startsWith("<span>")) trimmed = caption.slice("<span>".length).trim();
		}
	}
	const expr = staticExpression(trimmed);
	if (expr) return expr;
	if (!allowPlain) return null;
	const plain = trimmed.match(PLAIN);
	if (plain) {
		const text = plain[1].replace(/\s+/g, " ").trim();
		if (text) return { text };
	}
	return null;
}

function unescapeJsx(s: string): string {
	return s.replace(/\\"/g, '"').replace(/\\n/g, " ").replace(/\\\\/g, "\\");
}

/** Element occurrences (`<h3>`, `<label>`) and `label=` props, in source order. */
function* occurrences(src: string): Generator<{ kind: "h3" | "label"; inner: string; isProp: boolean; at: number }> {
	const re = /<(h3|label)\b|<(SectionCard|DangerZone|SettingPageHeader|StatusBadge|StatusSummary)\b|(?<![\w-])label=/g;
	let m: RegExpExecArray | null;
	while ((m = re.exec(src)) !== null) {
		if (m[1]) {
			const open = endOfOpenTag(src, m.index);
			if (open < 0) continue;
			const close = src.indexOf(`</${m[1]}>`, open);
			if (close < 0) continue;
			yield { kind: m[1] as "h3" | "label", inner: src.slice(open + 1, close), isProp: false, at: m.index };
			re.lastIndex = open + 1;
		} else if (m[2]) {
			const open = endOfOpenTag(src, m.index);
			if (open < 0) continue;
			// Badges and summaries display runtime status, not searchable controls.
			if (m[2] === "StatusBadge" || m[2] === "StatusSummary") {
				re.lastIndex = open + 1;
				continue;
			}
			const tag = src.slice(m.index, open);
			const title = tag.match(/\btitle=/);
			if (title?.index !== undefined)
				yield { kind: "h3", inner: tag.slice(title.index + "title=".length), isProp: true, at: m.index };
		} else {
			yield { kind: "label", inner: src.slice(m.index + "label=".length), isProp: true, at: m.index };
		}
	}
}

/** Source spans of `<ExpertSetting …>…</ExpertSetting>`, with their configKey.
 *
 * The key must be a string literal — the index is static, so a computed key
 * could not be copied into it. Throwing makes that mistake loud. */
function expertSpans(src: string): { start: number; end: number; configKey: string }[] {
	const spans: { start: number; end: number; configKey: string }[] = [];
	for (const m of src.matchAll(/<ExpertSetting\b/g)) {
		const open = endOfOpenTag(src, m.index);
		const key = open < 0 ? null : src.slice(m.index, open).match(/\bconfigKey="([^"]+)"/);
		if (!key) throw new Error(`ExpertSetting at offset ${m.index} needs a string-literal configKey`);
		const close = src.indexOf("</ExpertSetting>", open);
		spans.push({ start: open, end: close < 0 ? src.length : close, configKey: key[1] });
	}
	return spans;
}

/** Index just past the `}` closing the brace that opens at `from`. */
function endOfBraces(src: string, from: number): number {
	let depth = 0;
	for (let i = from; i < src.length; i++) {
		if (src[i] === "{") depth++;
		else if (src[i] === "}" && --depth === 0) return i + 1;
	}
	return src.length;
}

/** Index of the `</Show>` closing the Show whose open tag ends at `open`,
 * skipping nested Shows (a self-closing `<Show … />` opens nothing). */
function closeOfShow(src: string, open: number): number {
	const re = /<Show\b|<\/Show>/g;
	re.lastIndex = open + 1;
	let depth = 0;
	let m: RegExpExecArray | null;
	while ((m = re.exec(src)) !== null) {
		if (m[0] === "</Show>") {
			if (depth === 0) return m.index;
			depth--;
		} else {
			const end = endOfOpenTag(src, m.index);
			if (end < 0) break;
			if (src[end - 1] !== "/") depth++;
			re.lastIndex = end + 1;
		}
	}
	return src.length;
}

/** Source spans that render in one client only: the body of a
 * `<Show when={isTauri()…}>` renders on the desktop, its `fallback` in a
 * browser. A compound `isTauri() && x` is still desktop-only. */
function platformSpans(src: string): { start: number; end: number; platform: ClientPlatform }[] {
	const spans: { start: number; end: number; platform: ClientPlatform }[] = [];
	for (const m of src.matchAll(/<Show\b/g)) {
		const open = endOfOpenTag(src, m.index);
		if (open < 0) continue;
		const tag = src.slice(m.index, open);
		if (!/\bwhen=\{\s*isTauri\(\)/.test(tag)) continue;
		const fallback = tag.match(/\bfallback=\{/);
		if (fallback?.index !== undefined) {
			const start = m.index + fallback.index + fallback[0].length - 1;
			spans.push({ start, end: endOfBraces(src, start), platform: "browser" });
		}
		if (src[open - 1] !== "/") spans.push({ start: open, end: closeOfShow(src, open), platform: "desktop" });
	}
	return spans;
}

export function extractTab(src: string): ExtractedTab {
	// Contextual explanations contain no controls and repeat the row identity only for lookup.
	src = src.replace(/<ContextualHelp\b[^>]*\/>/g, "");
	const out: ExtractedTab = { sections: [], settings: [], dynamic: 0 };
	const spans = expertSpans(src);
	const expertKeyAt = (at: number) => spans.find((span) => at > span.start && at < span.end)?.configKey;
	const gates = platformSpans(src);
	// Innermost gate wins: the latest-starting span that still contains `at`
	const platformAt = (at: number) =>
		gates.filter((gate) => at > gate.start && at < gate.end).sort((a, b) => b.start - a.start)[0]?.platform;
	let section = "";
	for (const occ of occurrences(src)) {
		const text = staticText(occ.inner, !occ.isProp);
		if (!text) {
			out.dynamic++;
			continue;
		}
		const platform = platformAt(occ.at);
		if (occ.kind === "h3") {
			section = text.text;
			out.sections.push({ ...text, ...(platform ? { platform } : {}) });
		} else {
			const configKey = expertKeyAt(occ.at);
			out.settings.push({ ...text, section, ...(configKey ? { configKey } : {}), ...(platform ? { platform } : {}) });
		}
	}
	return out;
}

/** Nav keys `SettingsPanel` actually renders a tab body for.
 *
 * Read from the `<Show when={activeTab() === "…"}>` guards rather than from the
 * nav list: the guards are what decides whether a search result can open
 * anything, so a tab added there but missing from the index is the exact drift
 * the index test must catch. Repo tabs are keyed on `activeRepoPath()` and do
 * not appear here — they are intentionally unindexed. */
export function extractRenderedTabKeys(src: string): string[] {
	const keys = new Set<string>();
	for (const m of src.matchAll(/activeTab\(\)\s*===\s*"([^"]+)"/g)) keys.add(m[1]);
	return [...keys];
}

/** Components `SettingsPanel` renders directly for each nav key, in order.
 *
 * Read from the body of each `<Show when={activeTab() === "…"…}>`: a page can
 * be composed of several components, and every one of them has to be a source
 * of that page's index entries. */
export function extractRenderedTabComponents(src: string): Record<string, string[]> {
	const out: Record<string, string[]> = {};
	for (const m of src.matchAll(/<Show\s+when=\{activeTab\(\)\s*===\s*"([^"]+)"/g)) {
		const open = endOfOpenTag(src, m.index);
		if (open < 0) continue;
		const body = src.slice(open + 1, closeOfShow(src, open));
		out[m[1]] = [...body.matchAll(/<([A-Z]\w*)\b/g)].map((c) => c[1]).filter((name) => name !== "SettingsHelpScope");
	}
	return out;
}
