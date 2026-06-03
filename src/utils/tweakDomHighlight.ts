/**
 * DOM-side rendering of tweak-comment highlights.
 *
 * The markdown source is rendered by `marked` with begin/end *sentinel* delimiters
 * left in place (see `injectTweakSentinels`). Once that HTML is in the DOM, this
 * module locates each sentinel pair and wraps the text between them in
 * `<span class="tweak-highlight">` elements carrying the comment metadata.
 *
 * Wrapping in the DOM — rather than injecting spans into the markdown source before
 * parsing — keeps inline formatting (`**bold**`, `*italic*`, `` `code` ``) intact
 * even when a selection straddles a formatting boundary, and yields a contiguous
 * highlight across inline elements (each crossed text node gets its own span sharing
 * the same `data-tweak-id`).
 */
import { type TweakComment, tweakBeginSentinel, tweakEndSentinel } from "./tweakComments";

/**
 * Find the first text node containing `token`, split it so the token becomes its own
 * range, delete the token characters, and insert an empty marker element in its place.
 * Returns the inserted marker, or null when the token is not present.
 */
function replaceTokenWithMarker(root: HTMLElement, token: string, dataAttr: string): HTMLElement | null {
	const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
	let node = walker.nextNode() as Text | null;
	while (node) {
		const idx = node.data.indexOf(token);
		if (idx !== -1) {
			const after = node.splitText(idx); // `after` now starts with the token
			after.deleteData(0, token.length); // strip the token characters
			const marker = document.createElement("span");
			marker.setAttribute(dataAttr, "");
			after.parentNode?.insertBefore(marker, after);
			return marker;
		}
		node = walker.nextNode() as Text | null;
	}
	return null;
}

/** Wrap every non-empty text node strictly between two marker elements in a highlight span. */
function wrapBetweenMarkers(root: HTMLElement, begin: HTMLElement, end: HTMLElement, comment: TweakComment): void {
	const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
	const between: Text[] = [];
	let node = walker.nextNode() as Text | null;
	while (node) {
		const afterBegin = (begin.compareDocumentPosition(node) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0;
		const beforeEnd = (end.compareDocumentPosition(node) & Node.DOCUMENT_POSITION_PRECEDING) !== 0;
		if (afterBegin && beforeEnd && node.data.length > 0) between.push(node);
		node = walker.nextNode() as Text | null;
	}

	for (const text of between) {
		const span = document.createElement("span");
		span.className = "tweak-highlight";
		span.dataset["tweakId"] = comment.id;
		span.dataset["tweakAt"] = comment.createdAt;
		span.dataset["tweakComment"] = comment.comment;
		text.parentNode?.replaceChild(span, text);
		span.appendChild(text);
	}
}

/**
 * Replace tweak sentinels in the rendered DOM with highlight spans for each comment.
 * Safe to call repeatedly on freshly rendered content (sentinels are removed as they
 * are consumed). Comments whose sentinels are absent (e.g. filtered out by rendering)
 * are skipped.
 */
export function applyTweakDomHighlights(container: HTMLElement, comments: TweakComment[]): void {
	for (const comment of comments) {
		const begin = replaceTokenWithMarker(container, tweakBeginSentinel(comment.id), "data-tweak-begin");
		const end = replaceTokenWithMarker(container, tweakEndSentinel(comment.id), "data-tweak-end");
		if (begin && end) {
			wrapBetweenMarkers(container, begin, end, comment);
		}
		begin?.remove();
		end?.remove();
	}
	// Merge the text nodes left split by token removal so selection offsets stay clean.
	container.normalize();
}
