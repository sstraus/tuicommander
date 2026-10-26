import { createEffect, createSignal, Show } from "solid-js";
import { Button, Textarea } from "../../components/ui";
import { ContentRenderer } from "../../components/ui/ContentRenderer";
import { generateTweakCommentId, type TweakComment } from "../../utils/tweakComments";
import styles from "./ReviewableMarkdown.module.css";

/** A checkbox is 13px wide; this much on every side brings its tap target to 44px. */
const CHECKBOX_TAP_SLOP = 16;

interface SelectedBlock {
	start: number;
	end: number;
	text: string;
	/** Comment already stored on this block (the desktop highlights it), if any. */
	existing: string | null;
}

interface ReviewableMarkdownProps {
	content: string;
	imageSrc: (relativePath: string) => string;
	onCheckboxToggle: (sourceLine: number, mark: " " | "x" | "~", sourceCol?: number) => void;
	/** Resolves true when the comment reached the file. */
	onSaveBlockComment: (comment: TweakComment, range: { start: number; end: number }) => Promise<boolean>;
}

/** The checkbox whose enlarged tap target holds the point, nearest centre first. */
function checkboxNear(root: HTMLElement, x: number, y: number): HTMLInputElement | null {
	let best: HTMLInputElement | null = null;
	let bestDistance = Number.POSITIVE_INFINITY;
	for (const box of root.querySelectorAll<HTMLInputElement>('input[type="checkbox"][data-source-line]')) {
		const r = box.getBoundingClientRect();
		if (
			x < r.left - CHECKBOX_TAP_SLOP ||
			x > r.right + CHECKBOX_TAP_SLOP ||
			y < r.top - CHECKBOX_TAP_SLOP ||
			y > r.bottom + CHECKBOX_TAP_SLOP
		)
			continue;
		const distance = Math.hypot(x - (r.left + r.right) / 2, y - (r.top + r.bottom) / 2);
		if (distance < bestDistance) {
			best = box;
			bestDistance = distance;
		}
	}
	return best;
}

/**
 * Markdown view for the mobile PWA that can answer what it shows: a tap near a task-list
 * checkbox toggles it, and a tap on a block offers a block comment. Writing to the file is
 * the caller's job; the file format comes from the same helpers the desktop viewer uses.
 */
export function ReviewableMarkdown(props: ReviewableMarkdownProps) {
	const [selected, setSelected] = createSignal<SelectedBlock | null>(null);
	const [composing, setComposing] = createSignal(false);
	const [draft, setDraft] = createSignal("");
	const [saving, setSaving] = createSignal(false);
	let root: HTMLDivElement | undefined;
	let selectedEl: HTMLElement | null = null;

	function clearSelection() {
		selectedEl?.classList.remove("tweak-block-target");
		selectedEl = null;
		setSelected(null);
		setComposing(false);
	}

	function discardDraft() {
		clearSelection();
		setDraft("");
	}

	// The rendered DOM is rebuilt whenever the text changes, so a selection would point at stale ranges.
	// The typed comment survives it: after a reload the user taps the block again and finds it still there.
	createEffect(() => {
		props.content;
		clearSelection();
	});

	function onClick(event: MouseEvent) {
		const target = event.target as HTMLElement;
		if (target.closest("a, button, textarea")) return;
		if (target instanceof HTMLInputElement && target.type === "checkbox") return;
		if (root) {
			const box = checkboxNear(root, event.clientX, event.clientY);
			if (box) {
				box.click();
				return;
			}
		}
		const block = target.closest<HTMLElement>("[data-comment-source-start][data-comment-source-end]");
		if (!block) return;
		const start = Number(block.dataset.commentSourceStart);
		const end = Number(block.dataset.commentSourceEnd);
		if (!Number.isFinite(start) || !Number.isFinite(end)) return;
		const wasSelected = block === selectedEl;
		clearSelection();
		if (wasSelected) return;
		block.classList.add("tweak-block-target");
		selectedEl = block;
		setSelected({
			start,
			end,
			text: props.content.slice(start, end),
			existing: block.dataset.tweakComment ?? null,
		});
	}

	async function saveComment() {
		const block = selected();
		const text = draft().trim();
		if (!block || !text || saving()) return;
		setSaving(true);
		try {
			const saved = await props.onSaveBlockComment(
				{
					id: generateTweakCommentId(),
					highlighted: block.text,
					comment: text,
					createdAt: new Date().toISOString(),
					anchor: "block",
				},
				{ start: block.start, end: block.end },
			);
			if (saved) discardDraft();
		} finally {
			setSaving(false);
		}
	}

	return (
		<div class={styles.root}>
			<div ref={root} onClick={onClick}>
				<ContentRenderer
					content={props.content}
					imageSrc={props.imageSrc}
					onCheckboxToggle={props.onCheckboxToggle}
					commentableBlocks
				/>
			</div>
			<Show when={selected()}>
				{(block) => (
					<div class={styles.bar} role="region" aria-label="Block comment">
						<p class={styles.preview}>{block().text}</p>
						<Show when={block().existing !== null}>
							<p class={styles.existing}>{block().existing}</p>
						</Show>
						<Show when={block().existing === null}>
							<Show
								when={composing()}
								fallback={
									<Button type="button" class={styles.button} onClick={() => setComposing(true)}>
										Comment
									</Button>
								}
							>
								<Textarea
									aria-label="Comment text"
									value={draft()}
									onInput={(event) => setDraft(event.currentTarget.value)}
								/>
								<div class={styles.actions}>
									<Button type="button" class={styles.button} onClick={discardDraft}>
										Cancel
									</Button>
									<Button
										type="button"
										class={styles.button}
										disabled={saving() || !draft().trim()}
										onClick={() => void saveComment()}
									>
										Save comment
									</Button>
								</div>
							</Show>
						</Show>
					</div>
				)}
			</Show>
		</div>
	);
}
