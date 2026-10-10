/**
 * CommentOverlay — manages the floating "Comment" button and inline popover
 * for adding, viewing, editing, and deleting tweak comments in a rendered
 * markdown document.
 *
 * Usage:
 *   Mount once inside MarkdownTab. Pass `contentRef` (the rendered markdown
 *   container) and callbacks for save/delete that operate on the raw source.
 */
import { type Component, createEffect, createSignal, onCleanup, onMount, Show } from "solid-js";
import { Portal } from "solid-js/web";
import { generateTweakCommentId, normalizeForMatch, type TweakComment } from "../../utils/tweakComments";
import s from "./MarkdownTab.module.css";

export interface CommentOverlayProps {
	/** The rendered markdown container — used to scope selection and click events. */
	contentRef: HTMLDivElement;
	/** Called with the new or updated comment when the user saves. `occurrenceIndex`
	 *  is the 0-based ordinal of the selected text among identical rendered
	 *  occurrences, used to anchor the correct instance in the source. */
	onSave: (comment: TweakComment, occurrenceIndex: number) => Promise<boolean> | boolean | undefined;
	/** Save a whole rendered Markdown block using its exact raw-source range.
	 *  `comment.highlighted` is the raw source `blockSource` returned when the
	 *  popover opened, so the caller can refuse a range the file moved under. */
	onSaveBlock?: (
		comment: TweakComment,
		range: { start: number; end: number },
	) => Promise<boolean> | boolean | undefined;
	/** Raw source of a block range in the document currently rendered. */
	blockSource?: (range: { start: number; end: number }) => string;
	/** Called with the comment id when the user deletes a comment. */
	onDelete: (id: string) => Promise<boolean> | boolean | undefined;
}

interface PopoverState {
	x: number;
	y: number;
	mode: "new" | "view";
	existingId?: string;
	existingHighlighted?: string;
	existingComment?: string;
	existingCreatedAt?: string;
	selectionText?: string;
	sourceStart?: number;
	sourceEnd?: number;
	/** Raw block source captured when the popover opened. */
	sourceSnapshot?: string;
}

interface TooltipState {
	x: number;
	y: number;
	text: string;
}

/**
 * The top-level block whose comment gutter (36px left of it to 8px inside it)
 * contains the point. Top-level blocks stack in document order, so a binary
 * search on their vertical extent reads O(log n) layouts instead of all of them.
 */
function blockInGutter(blocks: ArrayLike<HTMLElement>, clientX: number, clientY: number): HTMLElement | null {
	let low = 0;
	let high = blocks.length - 1;
	while (low <= high) {
		const mid = (low + high) >> 1;
		const rect = blocks[mid].getBoundingClientRect();
		if (clientY < rect.top) high = mid - 1;
		else if (clientY > rect.bottom) low = mid + 1;
		else return clientX >= rect.left - 36 && clientX <= rect.left + 8 ? blocks[mid] : null;
	}
	return null;
}

/** Find the deepest list item under the pointer; ancestor items may overlap it. */
function listItemInGutter(items: HTMLElement[], clientX: number, clientY: number): HTMLElement | null {
	let low = 0;
	let high = items.length;
	while (low < high) {
		const mid = (low + high) >> 1;
		if (items[mid].getBoundingClientRect().top <= clientY) low = mid + 1;
		else high = mid;
	}
	for (let item: HTMLElement | null = items[low - 1] ?? null; item; item = item.parentElement?.closest("li") ?? null) {
		const rect = item.getBoundingClientRect();
		if (clientY <= rect.bottom && clientX >= rect.left - 36 && clientX <= rect.left + 8) return item;
	}
	return null;
}

export const CommentOverlay: Component<CommentOverlayProps> = (props) => {
	const [btnPos, setBtnPos] = createSignal<{ x: number; y: number } | null>(null);
	const [blockBtn, setBlockBtn] = createSignal<{
		x: number;
		y: number;
		start: number;
		end: number;
		text: string;
	} | null>(null);
	const [popover, setPopover] = createSignal<PopoverState | null>(null);
	const [tooltip, setTooltip] = createSignal<TooltipState | null>(null);
	const [draft, setDraft] = createSignal("");

	// The selected text captured at "add comment" button time.
	// We must snapshot it immediately because the selection may clear on click.
	let pendingSelection = "";
	// 0-based ordinal of the selection among identical rendered occurrences,
	// captured alongside the text so the correct source instance is anchored.
	let pendingOccurrence = 0;
	let hoveredBlock: HTMLElement | null = null;

	const clearBlockTarget = () => {
		hoveredBlock?.classList.remove("tweak-block-target");
		hoveredBlock = null;
		setBlockBtn(null);
	};

	// ── Selection detection ──
	//
	// We deliberately do NOT listen to `selectionchange` on document: that event
	// fires on every cursor movement and DOM selection update anywhere in the
	// app (including xterm buffers repainting), which caused noticeable UI lag.
	//
	// Instead we listen to `mouseup` and `keyup` scoped to the markdown content
	// element. These fire only when the user actively interacts with the
	// markdown, which is the only time a new selection can be created.

	const checkSelectionAndUpdateButton = () => {
		if (popover()) return;

		const sel = window.getSelection();
		if (!sel || sel.isCollapsed || !sel.toString().trim()) {
			setBtnPos(null);
			return;
		}

		// Only show for selections inside our content container.
		const range = sel.getRangeAt(0);
		if (!props.contentRef.contains(range.commonAncestorContainer)) {
			setBtnPos(null);
			return;
		}

		// Single-block only: skip selections that cross block boundaries.
		if (crossesBlockBoundary(range, props.contentRef)) {
			setBtnPos(null);
			return;
		}

		// Overlapping an existing highlight would nest one comment inside another,
		// which the source format cannot represent (the inner one gets swallowed on
		// reparse and silently vanishes). Suppress the button — the user should
		// click the highlight to edit it, or select plain text instead.
		if (rangeIntersectsHighlight(range, props.contentRef)) {
			setBtnPos(null);
			return;
		}

		// Use the last client rect so multi-line selections anchor the icon
		// at the true end of the selection (not the bounding box corner).
		const rects = range.getClientRects();
		// biome-ignore lint/style/useAtIndex: DOMRectList has indexed access but no at method.
		const rect = rects.length > 0 ? rects[rects.length - 1] : range.getBoundingClientRect();
		const BTN_SIZE = 28;
		clearBlockTarget();
		setBtnPos({
			x: rect.right + 4,
			y: rect.top + rect.height / 2 - BTN_SIZE / 2,
		});
	};

	const handleMouseUp = () => checkSelectionAndUpdateButton();
	const handleKeyUp = (e: KeyboardEvent) => {
		// Only handle shift+arrow-style keyboard selection.
		if (e.shiftKey || e.key.startsWith("Arrow") || e.key === "Home" || e.key === "End") {
			checkSelectionAndUpdateButton();
		}
	};

	// ── Click listener — open view/edit popover on existing highlights ──

	const handleClick = (e: MouseEvent) => {
		// Ignore if click is inside an open popover (handled by popover itself).
		const target = e.target as HTMLElement;
		if (target.closest("a, button, input, select, textarea, label")) return;
		const span = target.closest(".tweak-highlight, .tweak-block-highlight") as HTMLElement | null;
		if (!span) return;
		if (span.tagName === "LI" && target.closest("li") !== span) return;
		// The click that ends a drag-select must leave the selection to the inline comment button.
		if (window.getSelection()?.isCollapsed === false) return;

		const id = span.dataset["tweakId"];
		const comment = span.dataset["tweakComment"];
		const createdAt = span.dataset["tweakAt"];
		if (!id || comment === undefined || !createdAt) return;

		e.preventDefault();
		e.stopPropagation();

		const rect = span.getBoundingClientRect();
		setDraft(comment);
		setPopover({
			x: rect.left,
			y: rect.bottom + 6,
			mode: "view",
			existingId: id,
			existingHighlighted: span.textContent ?? "",
			existingComment: comment,
			existingCreatedAt: createdAt,
		});
	};

	// Hover tooltip: uses mouseover/mouseout (delegated) so no per-span listener.
	const handleMouseOver = (e: MouseEvent) => {
		const target = e.target as HTMLElement;
		const span = target.closest(".tweak-highlight, .tweak-block-highlight") as HTMLElement | null;
		if (!span) return;
		if (span.tagName === "LI" && target.closest("li") !== span) {
			setTooltip(null);
			return;
		}
		const comment = span.dataset["tweakComment"];
		if (!comment) return;
		const rect = span.getBoundingClientRect();
		setTooltip({
			x: rect.left,
			y: rect.bottom + 6,
			text: comment,
		});
	};
	const handleMouseOut = (e: MouseEvent) => {
		const target = e.target as HTMLElement;
		const related = e.relatedTarget as HTMLElement | null;
		const leavingSpan = target.closest(".tweak-highlight, .tweak-block-highlight");
		if (!leavingSpan) return;
		// Still inside the same highlight span — don't clear.
		if (related && leavingSpan.contains(related)) return;
		setTooltip(null);
	};

	const handleBlockGutterMove = (e: MouseEvent) => {
		if (popover() || btnPos()) return;
		const targets = props.contentRef.querySelectorAll<HTMLElement>(
			"[data-comment-source-start][data-comment-source-end]",
		);
		const listItems = Array.from(targets).filter((target) => target.tagName === "LI");
		let block = blockInGutter(
			listItems.length ? Array.from(targets).filter((target) => target.tagName !== "LI") : targets,
			e.clientX,
			e.clientY,
		);
		block = listItemInGutter(listItems, e.clientX, e.clientY) ?? block;
		const inlineCommentInBlock =
			block &&
			Array.from(block.querySelectorAll(".tweak-highlight")).some(
				(span) => block!.tagName !== "LI" || span.closest("li") === block,
			);
		if (!block || block.classList.contains("tweak-block-highlight") || inlineCommentInBlock) {
			clearBlockTarget();
			return;
		}
		const start = Number(block.dataset.commentSourceStart);
		const end = Number(block.dataset.commentSourceEnd);
		if (!Number.isFinite(start) || !Number.isFinite(end)) {
			clearBlockTarget();
			return;
		}
		if (hoveredBlock !== block) {
			hoveredBlock?.classList.remove("tweak-block-target");
			hoveredBlock = block;
			block.classList.add("tweak-block-target");
		}
		const rect = block.getBoundingClientRect();
		const x = Math.max(4, rect.left - 30);
		const y = rect.top + 2;
		const current = blockBtn();
		if (current && current.x === x && current.y === y && current.start === start && current.end === end) return;
		setBlockBtn({ x, y, start, end, text: (block.textContent ?? "").trim() });
	};

	onMount(() => {
		const scrollHost = props.contentRef.parentElement ?? props.contentRef;
		props.contentRef.addEventListener("mouseup", handleMouseUp);
		props.contentRef.addEventListener("keyup", handleKeyUp);
		props.contentRef.addEventListener("click", handleClick);
		props.contentRef.addEventListener("mouseover", handleMouseOver);
		props.contentRef.addEventListener("mouseout", handleMouseOut);
		scrollHost.addEventListener("mousemove", handleBlockGutterMove);
		scrollHost.addEventListener("scroll", clearBlockTarget, { passive: true });
		onCleanup(() => {
			props.contentRef.removeEventListener("mouseup", handleMouseUp);
			props.contentRef.removeEventListener("keyup", handleKeyUp);
			props.contentRef.removeEventListener("click", handleClick);
			props.contentRef.removeEventListener("mouseover", handleMouseOver);
			props.contentRef.removeEventListener("mouseout", handleMouseOut);
			scrollHost.removeEventListener("mousemove", handleBlockGutterMove);
			scrollHost.removeEventListener("scroll", clearBlockTarget);
			clearBlockTarget();
		});
	});

	// ── Handlers ──

	const openNewCommentPopover = () => {
		const sel = window.getSelection();
		const text = sel?.toString().trim() ?? "";
		if (!text || !sel || sel.rangeCount === 0) return;

		// Anchor the popover directly below the start of the selection.
		// Reading the range BEFORE clearing the selection so we get real coordinates.
		const range = sel.getRangeAt(0);
		const rects = range.getClientRects();
		const firstRect = rects.length > 0 ? rects[0] : range.getBoundingClientRect();
		// biome-ignore lint/style/useAtIndex: DOMRectList has indexed access but no at method.
		const lastRect = rects.length > 0 ? rects[rects.length - 1] : firstRect;

		// Snapshot the selection text + its occurrence ordinal before clearing.
		// The ordinal = how many identical (normalized) occurrences of the selected
		// text appear in the rendered content BEFORE this selection's start. This
		// disambiguates repeated text so the correct source instance is anchored.
		pendingSelection = text;
		pendingOccurrence = occurrenceOrdinal(props.contentRef, range, text);
		setBtnPos(null);
		setDraft("");
		setPopover({
			x: firstRect.left,
			y: lastRect.bottom + 8,
			mode: "new",
			selectionText: text,
		});
	};

	const openBlockCommentPopover = () => {
		const block = blockBtn();
		if (!block) return;
		clearBlockTarget();
		setDraft("");
		setPopover({
			x: block.x + 30,
			y: block.y + 28,
			mode: "new",
			selectionText: block.text,
			sourceStart: block.start,
			sourceEnd: block.end,
			sourceSnapshot: props.blockSource?.({ start: block.start, end: block.end }),
		});
	};

	const handleSave = async () => {
		const state = popover();
		if (!state) return;

		if (state.mode === "new") {
			if (state.sourceStart !== undefined && state.sourceEnd !== undefined) {
				if (!draft().trim()) return;
				const saved = await props.onSaveBlock?.(
					{
						id: generateTweakCommentId(),
						highlighted: state.sourceSnapshot ?? "",
						comment: draft().trim(),
						createdAt: new Date().toISOString(),
						anchor: "block",
					},
					{ start: state.sourceStart, end: state.sourceEnd },
				);
				if (saved !== false) closePopover();
				return;
			}
			const highlighted = pendingSelection;
			if (!highlighted || !draft().trim()) return;
			const saved = await props.onSave(
				{
					id: generateTweakCommentId(),
					highlighted,
					comment: draft().trim(),
					createdAt: new Date().toISOString(),
				},
				pendingOccurrence,
			);
			if (saved !== false) closePopover();
		} else {
			// Edit existing — preserve original createdAt, update only the comment text.
			// Occurrence ordinal is irrelevant for edits (matched by id).
			if (!state.existingId || !state.existingHighlighted) return;
			const saved = await props.onSave(
				{
					id: state.existingId,
					highlighted: state.existingHighlighted,
					comment: draft().trim(),
					createdAt: state.existingCreatedAt ?? new Date().toISOString(),
				},
				0,
			);
			if (saved !== false) closePopover();
		}
	};

	const handleDelete = async () => {
		const state = popover();
		if (!state?.existingId) return;
		if ((await props.onDelete(state.existingId)) !== false) closePopover();
	};

	const closePopover = () => {
		setPopover(null);
		setDraft("");
		pendingSelection = "";
		pendingOccurrence = 0;
	};

	const handlePopoverKeyDown = (e: KeyboardEvent) => {
		if (e.key === "Escape") closePopover();
		if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) handleSave();
	};

	// Outside-click listener is attached ONLY while the popover is open,
	// so we don't tax every click in the app with an extra handler.
	createEffect(() => {
		if (!popover()) return;
		const handleOutsideClick = (e: MouseEvent) => {
			const target = e.target as HTMLElement;
			if (!target.closest("[data-tweak-popover]")) closePopover();
		};
		document.addEventListener("mousedown", handleOutsideClick);
		onCleanup(() => document.removeEventListener("mousedown", handleOutsideClick));
	});

	return (
		<Portal>
			{/* Hover tooltip showing the comment text on existing highlights */}
			<Show when={!popover() && tooltip()} keyed>
				{(t) => (
					<div class={s.tooltip} style={{ left: `${t.x}px`, top: `${t.y}px` }}>
						{t.text}
					</div>
				)}
			</Show>

			{/* Floating "Comment" button near selection */}
			<Show when={btnPos() && !popover()}>
				<button
					class={s.commentBtn}
					style={{ left: `${btnPos()!.x}px`, top: `${btnPos()!.y}px` }}
					onMouseDown={(e) => {
						e.preventDefault();
						openNewCommentPopover();
					}}
					title="Add inline comment"
					aria-label="Add inline comment"
				>
					<svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
						<path d="M2 2h12v9H9.5l-1.5 2-1.5-2H2V2zm1 1v7h4.17l.83 1.11L8.83 10H13V3H3z" />
						<path d="M5 6h6v1H5zm0 2h4v1H5z" opacity="0.6" />
					</svg>
				</button>
			</Show>

			<Show when={blockBtn() && !btnPos() && !popover()}>
				<button
					class={s.commentBtn}
					style={{ left: `${blockBtn()!.x}px`, top: `${blockBtn()!.y}px` }}
					onMouseDown={(e) => {
						e.preventDefault();
						openBlockCommentPopover();
					}}
					title="Comment on this block"
					aria-label="Comment on this block"
				>
					<svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
						<path d="M2 2h12v9H9.5l-1.5 2-1.5-2H2V2zm1 1v7h4.17l.83 1.11L8.83 10H13V3H3z" />
						<path d="M5 6h6v1H5zm0 2h4v1H5z" opacity="0.6" />
					</svg>
				</button>
			</Show>

			{/* Popover (new comment or view/edit) */}
			<Show when={popover()}>
				{(state) => {
					// Clamp to viewport so the popover never overflows the right/bottom edge.
					// Width/height match the CSS in MarkdownTab.module.css (440 × ~240 expected).
					const clamped = () => {
						const margin = 12;
						const w = 440;
						const h = 240;
						const x = Math.min(state().x, window.innerWidth - w - margin);
						const y = Math.min(state().y, window.innerHeight - h - margin);
						return { x: Math.max(margin, x), y: Math.max(margin, y) };
					};
					return (
						<div
							class={s.popover}
							data-tweak-popover="1"
							style={{ left: `${clamped().x}px`, top: `${clamped().y}px` }}
							onKeyDown={handlePopoverKeyDown}
						>
							{/* Preview of highlighted text */}
							<div class={s.popoverHighlightedText} title={state().existingHighlighted ?? state().selectionText}>
								"{(state().existingHighlighted ?? state().selectionText ?? "").slice(0, 60)}"
							</div>

							<textarea
								class={s.popoverTextarea}
								placeholder="Add your comment… (Ctrl+Enter to save)"
								value={draft()}
								onInput={(e) => setDraft(e.currentTarget.value)}
								ref={(el) => queueMicrotask(() => el.focus())}
								rows={3}
							/>

							<div class={s.popoverActions}>
								<Show when={state().mode === "view"}>
									<button class={`${s.popoverBtn} ${s.popoverBtnDanger}`} onClick={handleDelete}>
										Delete
									</button>
								</Show>
								<button class={s.popoverBtn} onClick={closePopover}>
									Cancel
								</button>
								<button
									class={`${s.popoverBtn} ${s.popoverBtnPrimary}`}
									onClick={handleSave}
									disabled={!draft().trim()}
								>
									Save
								</button>
							</div>
						</div>
					);
				}}
			</Show>
		</Portal>
	);
};

// ── Helpers ──

const BLOCK_TAGS = new Set(["P", "H1", "H2", "H3", "H4", "H5", "H6", "LI", "BLOCKQUOTE", "PRE", "TD", "TH"]);

/**
 * 0-based ordinal of `selectionText` among identical rendered occurrences: the
 * count of normalized occurrences appearing in `root`'s text BEFORE the start of
 * `range`. Mirrors `findSourceMatch`'s normalization so the ordinal lines up with
 * the source-side occurrence the comment must anchor to.
 */
function occurrenceOrdinal(root: HTMLElement, range: Range, selectionText: string): number {
	const needle = normalizeForMatch(selectionText);
	if (!needle) return 0;
	const pre = document.createRange();
	pre.setStart(root, 0);
	pre.setEnd(range.startContainer, range.startOffset);
	const before = normalizeForMatch(pre.toString());
	let count = 0;
	let idx = before.indexOf(needle);
	while (idx !== -1) {
		count++;
		idx = before.indexOf(needle, idx + 1);
	}
	return count;
}

/** Returns true if the selection range intersects any existing highlight span. */
function rangeIntersectsHighlight(range: Range, root: HTMLElement): boolean {
	const highlights = root.querySelectorAll(".tweak-highlight");
	for (const el of highlights) {
		if (range.intersectsNode(el)) return true;
	}
	return false;
}

/** Returns true if the range starts and ends in different block-level elements. */
function crossesBlockBoundary(range: Range, root: HTMLElement): boolean {
	const startBlock = nearestBlock(range.startContainer, root);
	const endBlock = nearestBlock(range.endContainer, root);
	return startBlock !== endBlock;
}

function nearestBlock(node: Node, root: HTMLElement): Element | null {
	let current: Node | null = node;
	while (current && current !== root) {
		if (current.nodeType === Node.ELEMENT_NODE) {
			if (BLOCK_TAGS.has((current as Element).tagName)) return current as Element;
		}
		current = current.parentNode;
	}
	return root;
}
