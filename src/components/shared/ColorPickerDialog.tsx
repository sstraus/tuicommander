import { Component, Show, createEffect, onCleanup } from "solid-js";
import { ColorSwatchPicker } from "./ColorSwatchPicker";
import { t } from "../../i18n";
import d from "./dialog.module.css";

export interface ColorPickerDialogProps {
  visible: boolean;
  title: string;
  currentColor: string;
  onClose: () => void;
  onConfirm: (color: string) => void;
}

export const ColorPickerDialog: Component<ColorPickerDialogProps> = (props) => {
  createEffect(() => {
    if (!props.visible) return;

    const handleKeydown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        props.onClose();
      }
    };

    document.addEventListener("keydown", handleKeydown);
    onCleanup(() => document.removeEventListener("keydown", handleKeydown));
  });

  const handleChange = (color: string) => {
    props.onConfirm(color);
    props.onClose();
  };

  return (
    <Show when={props.visible}>
      <div class={d.overlay} onClick={props.onClose}>
        <div class={d.popover} onClick={(e) => e.stopPropagation()}>
          <div class={d.header}>
            <h4>{props.title}</h4>
          </div>
          <div class={d.body}>
            <ColorSwatchPicker color={props.currentColor} onChange={handleChange} />
          </div>
          <div class={d.actions}>
            <button class={d.cancelBtn} onClick={props.onClose}>
              {t("colorPickerDialog.cancel", "Cancel")}
            </button>
          </div>
        </div>
      </div>
    </Show>
  );
};
