/**
 * CommentOverlay — manages the floating "Comment" button and inline popover
 * for adding, viewing, editing, and deleting tweak comments in a rendered
 * markdown document.
 *
 * Usage:
 *   Mount once inside MarkdownTab. Pass `contentRef` (the rendered markdown
 *   container) and callbacks for save/delete that operate on the raw source.
 */
import { Component, createSignal, onCleanup, onMount, Show } from "solid-js";
import { Portal } from "solid-js/web";
import s from "./MarkdownTab.module.css";
import {
  generateTweakCommentId,
  type TweakComment,
} from "../../utils/tweakComments";

export interface CommentOverlayProps {
  /** The rendered markdown container — used to scope selection and click events. */
  contentRef: HTMLDivElement;
  /** Called with the new or updated comment when the user saves. */
  onSave: (comment: TweakComment) => void;
  /** Called with the comment id when the user deletes a comment. */
  onDelete: (id: string) => void;
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
}

export const CommentOverlay: Component<CommentOverlayProps> = (props) => {
  const [btnPos, setBtnPos] = createSignal<{ x: number; y: number } | null>(null);
  const [popover, setPopover] = createSignal<PopoverState | null>(null);
  const [draft, setDraft] = createSignal("");

  // The selected text captured at "add comment" button time.
  // We must snapshot it immediately because the selection may clear on click.
  let pendingSelection = "";

  // ── Selection listener — show floating "Comment" button on non-empty selection ──

  const handleSelectionChange = () => {
    // Ignore if popover is open — don't interrupt editing.
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

    // Disallow selections that cross block boundaries (single-block only).
    // A cross-block selection has the start and end in different block-level
    // ancestors (p, h1-h6, li, blockquote, etc.).
    if (crossesBlockBoundary(range, props.contentRef)) {
      setBtnPos(null);
      return;
    }

    const rect = range.getBoundingClientRect();
    // Position button just below the selection, centered.
    setBtnPos({
      x: rect.left + rect.width / 2 - 50,
      y: rect.bottom + 6,
    });
  };

  // ── Click listener — open view/edit popover on existing highlights ──

  const handleClick = (e: MouseEvent) => {
    // Ignore if click is inside an open popover (handled by popover itself).
    const target = e.target as HTMLElement;
    const span = target.closest(".tweak-highlight") as HTMLElement | null;
    if (!span) return;

    const id = span.dataset["tweakId"];
    const b64 = span.dataset["tweakCommentB64"];
    if (!id || !b64) return;

    let comment = "";
    let createdAt = new Date().toISOString();
    try {
      const payload = JSON.parse(atob(b64)) as { comment: string; created_at: string };
      comment = payload.comment;
      createdAt = payload.created_at;
    } catch {
      return;
    }

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

  onMount(() => {
    document.addEventListener("selectionchange", handleSelectionChange);
    props.contentRef.addEventListener("click", handleClick);
    onCleanup(() => {
      document.removeEventListener("selectionchange", handleSelectionChange);
      props.contentRef.removeEventListener("click", handleClick);
    });
  });

  // ── Handlers ──

  const openNewCommentPopover = () => {
    const sel = window.getSelection();
    const text = sel?.toString().trim() ?? "";
    if (!text) return;

    // Snapshot the selection text before clearing it.
    pendingSelection = text;
    // Clear the floating button and open popover.
    setBtnPos(null);
    setDraft("");
    const pos = btnPos();
    setPopover({
      x: pos?.x ?? 200,
      y: pos?.y ?? 200,
      mode: "new",
      selectionText: text,
    });
  };

  const handleSave = () => {
    const state = popover();
    if (!state) return;

    if (state.mode === "new") {
      const highlighted = pendingSelection;
      if (!highlighted || !draft().trim()) return;
      props.onSave({
        id: generateTweakCommentId(),
        highlighted,
        comment: draft().trim(),
        createdAt: new Date().toISOString(),
      });
    } else {
      // Edit existing — preserve original createdAt, update only the comment text.
      if (!state.existingId || !state.existingHighlighted) return;
      props.onSave({
        id: state.existingId,
        highlighted: state.existingHighlighted,
        comment: draft().trim(),
        createdAt: state.existingCreatedAt ?? new Date().toISOString(),
      });
    }
    closePopover();
  };

  const handleDelete = () => {
    const state = popover();
    if (!state?.existingId) return;
    props.onDelete(state.existingId);
    closePopover();
  };

  const closePopover = () => {
    setPopover(null);
    setDraft("");
    pendingSelection = "";
  };

  const handlePopoverKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") closePopover();
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) handleSave();
  };

  // Close popover when clicking outside
  const handleOutsideClick = (e: MouseEvent) => {
    if (!popover()) return;
    const target = e.target as HTMLElement;
    if (!target.closest("[data-tweak-popover]")) closePopover();
  };

  onMount(() => {
    document.addEventListener("mousedown", handleOutsideClick);
    onCleanup(() => document.removeEventListener("mousedown", handleOutsideClick));
  });

  return (
    <Portal>
      {/* Floating "Comment" button near selection */}
      <Show when={btnPos() && !popover()}>
        <button
          class={s.commentBtn}
          style={{ left: `${btnPos()!.x}px`, top: `${btnPos()!.y}px` }}
          onMouseDown={(e) => { e.preventDefault(); openNewCommentPopover(); }}
          title="Add inline comment"
          aria-label="Add inline comment"
        >
          <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
            <path d="M2 2h12v9H9.5l-1.5 2-1.5-2H2V2zm1 1v7h4.17l.83 1.11L8.83 10H13V3H3z"/>
            <path d="M5 6h6v1H5zm0 2h4v1H5z" opacity="0.6"/>
          </svg>
        </button>
      </Show>

      {/* Popover (new comment or view/edit) */}
      <Show when={popover()}>
        {(state) => (
          <div
            class={s.popover}
            data-tweak-popover="1"
            style={{ left: `${state().x}px`, top: `${state().y}px` }}
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
              autofocus
              rows={3}
            />

            <div class={s.popoverActions}>
              <Show when={state().mode === "view"}>
                <button class={`${s.popoverBtn} ${s.popoverBtnDanger}`} onClick={handleDelete}>
                  Delete
                </button>
              </Show>
              <button class={s.popoverBtn} onClick={closePopover}>Cancel</button>
              <button
                class={`${s.popoverBtn} ${s.popoverBtnPrimary}`}
                onClick={handleSave}
                disabled={!draft().trim()}
              >
                Save
              </button>
            </div>
          </div>
        )}
      </Show>
    </Portal>
  );
};

// ── Helpers ──

const BLOCK_TAGS = new Set([
  "P", "H1", "H2", "H3", "H4", "H5", "H6",
  "LI", "BLOCKQUOTE", "PRE", "TD", "TH",
]);

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
