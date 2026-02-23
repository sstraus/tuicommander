import { Component, Show, createEffect, onCleanup } from "solid-js";
import d from "../shared/dialog.module.css";

export interface ConfirmDialogProps {
  visible: boolean;
  title: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  kind?: "warning" | "info" | "error";
  onClose: () => void;
  onConfirm: () => void;
}

/**
 * In-app confirmation dialog — replaces native Tauri ask() dialogs
 * which render as ugly light-mode macOS system sheets.
 * Uses shared dialog CSS module for consistent dark-theme styling.
 */
export const ConfirmDialog: Component<ConfirmDialogProps> = (props) => {
  createEffect(() => {
    if (!props.visible) return;

    const handleKeydown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        props.onClose();
      } else if (e.key === "Enter") {
        e.preventDefault();
        props.onConfirm();
      }
    };

    document.addEventListener("keydown", handleKeydown);
    onCleanup(() => document.removeEventListener("keydown", handleKeydown));
  });

  return (
    <Show when={props.visible}>
      <div class={d.overlay} onClick={props.onClose}>
        <div class={d.popover} onClick={(e) => e.stopPropagation()}>
          <div class={d.header}>
            <h4>{props.title}</h4>
          </div>
          <div class={d.body}>
            <p style={{ margin: 0, "white-space": "pre-line", color: "var(--fg-secondary)", "font-size": "var(--font-md)" }}>
              {props.message}
            </p>
          </div>
          <div class={d.actions}>
            <button class={d.cancelBtn} onClick={props.onClose}>
              {props.cancelLabel ?? "Cancel"}
            </button>
            <button class={d.primaryBtn} onClick={props.onConfirm}>
              {props.confirmLabel ?? "OK"}
            </button>
          </div>
        </div>
      </div>
    </Show>
  );
};

export default ConfirmDialog;
