import { Component, createSignal, createEffect, onCleanup, Show } from "solid-js";
import { t } from "../../i18n";
import d from "../shared/dialog.module.css";

export interface RenameBranchDialogProps {
  visible: boolean;
  currentName: string;
  onClose: () => void;
  onRename: (oldName: string, newName: string) => Promise<void>;
}

/** Validate git branch name */
export function validateBranchName(name: string): string | null {
  if (!name || name.trim() === "") {
    return "Branch name cannot be empty";
  }
  if (name.includes(" ")) {
    return "Branch name cannot contain spaces";
  }
  if (name.startsWith("-")) {
    return "Branch name cannot start with a hyphen";
  }
  if (name.includes("..")) {
    return "Branch name cannot contain '..'";
  }
  if (name.endsWith(".lock")) {
    return "Branch name cannot end with '.lock'";
  }
  if (name.includes("~") || name.includes("^") || name.includes(":") || name.includes("?") || name.includes("*") || name.includes("[") || name.includes("\\")) {
    return "Branch name contains invalid characters";
  }
  if (name.startsWith("/") || name.endsWith("/") || name.includes("//")) {
    return "Invalid slash usage in branch name";
  }
  if (name.endsWith(".")) {
    return "Branch name cannot end with a period";
  }
  if (name.includes("@{")) {
    return "Branch name cannot contain '@{'";
  }
  return null;
}

export const RenameBranchDialog: Component<RenameBranchDialogProps> = (props) => {
  const [newName, setNewName] = createSignal("");
  const [error, setError] = createSignal<string | null>(null);
  const [isRenaming, setIsRenaming] = createSignal(false);
  let inputRef: HTMLInputElement | undefined;

  // Reset state and focus input when dialog opens
  createEffect(() => {
    if (props.visible) {
      setNewName(props.currentName);
      setError(null);
      setIsRenaming(false);
      // Focus and select input after render
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
        props.onClose();
      } else if (e.key === "Enter" && !isRenaming()) {
        e.preventDefault();
        handleRename();
      }
    };

    document.addEventListener("keydown", handleKeydown);
    onCleanup(() => document.removeEventListener("keydown", handleKeydown));
  });

  const handleRename = async () => {
    const trimmedName = newName().trim();

    // Validate
    const validationError = validateBranchName(trimmedName);
    if (validationError) {
      setError(validationError);
      return;
    }

    // Check if name is the same
    if (trimmedName === props.currentName) {
      props.onClose();
      return;
    }

    setIsRenaming(true);
    setError(null);

    try {
      await props.onRename(props.currentName, trimmedName);
      props.onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setIsRenaming(false);
    }
  };

  const handleInputChange = (e: Event) => {
    const value = (e.target as HTMLInputElement).value;
    setNewName(value);
    // Clear error when user starts typing
    if (error()) {
      setError(null);
    }
  };

  return (
    <Show when={props.visible}>
      <div class={d.overlay} onClick={props.onClose}>
        <div class={d.popover} onClick={(e) => e.stopPropagation()}>
          <div class={d.header}>
            <span class={d.headerIcon}>Y</span>
            <h4>{t("renameBranch.title", "Rename Branch")}</h4>
          </div>
          <div class={d.body}>
            <input
              ref={inputRef}
              type="text"
              value={newName()}
              onInput={handleInputChange}
              placeholder={t("renameBranch.placeholder", "New branch name")}
              disabled={isRenaming()}
            />
            {error() && <p class={d.error}>{error()}</p>}
          </div>
          <div class={d.actions}>
            <button
              class={d.cancelBtn}
              onClick={props.onClose}
              disabled={isRenaming()}
            >
              {t("renameBranch.cancel", "Cancel")}
            </button>
            <button
              class={d.primaryBtn}
              onClick={handleRename}
              disabled={isRenaming() || !newName().trim()}
            >
              {isRenaming() ? t("renameBranch.renaming", "Renaming...") : t("renameBranch.rename", "Rename")}
            </button>
          </div>
        </div>
      </div>
    </Show>
  );
};

export default RenameBranchDialog;
