import { Component, createEffect, onCleanup } from "solid-js";
import d from "../shared/dialog.module.css";
import s from "./MergePostActionDialog.module.css";

export interface MergePostActionDialogProps {
  /** Branch name that was merged — shown in the dialog message. */
  branchName: string;
  onArchive: () => void;
  onDelete: () => void;
  onCancel: () => void;
}

/**
 * Shown after a successful merge when afterMerge=ask.
 * Lets the user choose what to do with the worktree directory:
 * Archive (move to __archived/), Delete (remove entirely), or Cancel (keep as-is).
 */
export const MergePostActionDialog: Component<MergePostActionDialogProps> = (props) => {
  createEffect(() => {
    const handleKeydown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        props.onCancel();
      }
    };
    document.addEventListener("keydown", handleKeydown);
    onCleanup(() => document.removeEventListener("keydown", handleKeydown));
  });

  return (
    <div class={d.overlay} onClick={props.onCancel}>
      <div class={d.popover} onClick={(e) => e.stopPropagation()}>
        <div class={d.header}>
          <h4>Merge complete</h4>
        </div>
        <div class={d.body}>
          <p style={{ margin: 0, "white-space": "pre-line", color: "var(--fg-secondary)", "font-size": "var(--font-md)" }}>
            {`${props.branchName} was merged.\nWhat should happen to its worktree directory?`}
          </p>
        </div>
        <div class={s.actions}>
          <button class={d.cancelBtn} onClick={props.onCancel}>
            Keep
          </button>
          <button class={s.deleteBtn} onClick={props.onDelete}>
            Delete
          </button>
          <button class={d.primaryBtn} onClick={props.onArchive}>
            Archive
          </button>
        </div>
      </div>
    </div>
  );
};

export default MergePostActionDialog;
