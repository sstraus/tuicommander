import { Component, Show, createSignal, createEffect, onCleanup } from "solid-js";
import { t } from "../../i18n";
import { cx } from "../../utils";
import d from "../shared/dialog.module.css";
import s from "./RunCommandDialog.module.css";

export interface RunCommandDialogProps {
  visible: boolean;
  savedCommand: string;
  onClose: () => void;
  onSaveAndRun: (command: string) => void;
}

export const RunCommandDialog: Component<RunCommandDialogProps> = (props) => {
  const [command, setCommand] = createSignal("");
  let inputRef: HTMLInputElement | undefined;

  // Reset state and focus input when dialog opens
  createEffect(() => {
    if (props.visible) {
      setCommand(props.savedCommand);
      setTimeout(() => {
        if (inputRef) {
          inputRef.focus();
          inputRef.select();
        }
      }, 0);
    }
  });

  // Handle keyboard events
  createEffect(() => {
    if (!props.visible) return;

    const handleKeydown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        props.onClose();
      } else if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        handleSaveAndRun();
      }
    };

    document.addEventListener("keydown", handleKeydown, true);
    onCleanup(() => document.removeEventListener("keydown", handleKeydown, true));
  });

  const handleSaveAndRun = () => {
    const trimmed = command().trim();
    if (!trimmed) return;
    props.onSaveAndRun(trimmed);
  };

  return (
    <Show when={props.visible}>
      <div class={d.overlay} onClick={props.onClose}>
        <div class={cx(d.popover, s.wider)} onClick={(e) => e.stopPropagation()}>
          <div class={d.header}>
            <span class={d.headerIcon}>▶</span>
            <h4>{t("runCommand.title", "Run Command")}</h4>
          </div>
          <div class={d.body}>
            <p class={s.description}>
              {t("runCommand.description", "Enter a command to run in this worktree. It will be saved to repository settings.")}
            </p>
            <input
              ref={inputRef}
              type="text"
              class={s.monoInput}
              value={command()}
              onInput={(e) => setCommand((e.target as HTMLInputElement).value)}
              placeholder={t("runCommand.placeholder", "npm run dev, cargo watch, make dev...")}
            />
          </div>
          <div class={d.actions}>
            <button
              class={d.cancelBtn}
              onClick={props.onClose}
            >
              {t("runCommand.cancel", "Cancel")}
            </button>
            <button
              class={d.primaryBtn}
              onClick={handleSaveAndRun}
              disabled={!command().trim()}
            >
              {t("runCommand.saveAndRun", "Save & Run")}
            </button>
          </div>
        </div>
      </div>
    </Show>
  );
};

export default RunCommandDialog;
