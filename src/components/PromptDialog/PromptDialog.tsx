import { Component, createSignal, createEffect, onCleanup } from "solid-js";

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
 * in Tauri's webview. Reuses branch-popover CSS classes for consistent styling.
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

  if (!props.visible) return null;

  return (
    <div class="branch-popover-overlay" onClick={props.onClose}>
      <div class="branch-popover" onClick={(e) => e.stopPropagation()}>
        <div class="branch-popover-header">
          <h4>{props.title}</h4>
        </div>
        <div class="branch-popover-content">
          <input
            ref={inputRef}
            type="text"
            value={value()}
            onInput={(e) => setValue((e.target as HTMLInputElement).value)}
            placeholder={props.placeholder ?? ""}
          />
        </div>
        <div class="branch-popover-actions">
          <button class="branch-popover-cancel" onClick={props.onClose}>
            Cancel
          </button>
          <button
            class="branch-popover-rename"
            onClick={() => {
              if (value().trim()) {
                props.onConfirm(value().trim());
                props.onClose();
              }
            }}
            disabled={!value().trim()}
          >
            {props.confirmLabel ?? "OK"}
          </button>
        </div>
      </div>
    </div>
  );
};

export default PromptDialog;
