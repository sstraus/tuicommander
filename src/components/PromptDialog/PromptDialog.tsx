import { Component, Show, createSignal, createEffect, onCleanup } from "solid-js";
import { t } from "../../i18n";
import d from "../shared/dialog.module.css";

export interface PromptDialogProps {
  visible: boolean;
  title: string;
  placeholder?: string;
  defaultValue?: string;
  confirmLabel?: string;
  onClose: () => void;
  onConfirm: (value: string) => void;
}

/**
 * Generic text input dialog — replaces window.prompt() which doesn't work
 * in Tauri's webview. Uses shared dialog CSS module for consistent styling.
 */
export const PromptDialog: Component<PromptDialogProps> = (props) => {
  const [value, setValue] = createSignal("");
  let inputRef: HTMLInputElement | undefined;

  createEffect(() => {
    if (props.visible) {
      setValue(props.defaultValue ?? "");
      setTimeout(() => {
        if (inputRef) {
          inputRef.focus();
          inputRef.select();
        }
      }, 0);
    }
  });

  createEffect(() => {
    if (!props.visible) return;

    const handleKeydown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        props.onClose();
      } else if (e.key === "Enter" && value().trim()) {
        e.preventDefault();
        props.onConfirm(value().trim());
        props.onClose();
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
            <input
              ref={inputRef}
              type="text"
              value={value()}
              onInput={(e) => setValue((e.target as HTMLInputElement).value)}
              placeholder={props.placeholder ?? ""}
            />
          </div>
          <div class={d.actions}>
            <button class={d.cancelBtn} onClick={props.onClose}>
              {t("promptDialog.cancel", "Cancel")}
            </button>
            <button
              class={d.primaryBtn}
              onClick={() => {
                if (value().trim()) {
                  props.onConfirm(value().trim());
                  props.onClose();
                }
              }}
              disabled={!value().trim()}
            >
              {props.confirmLabel ?? t("promptDialog.ok", "OK")}
            </button>
          </div>
        </div>
      </div>
    </Show>
  );
};

export default PromptDialog;
