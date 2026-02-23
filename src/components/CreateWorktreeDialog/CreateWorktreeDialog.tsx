import { Component, createSignal, createEffect, onCleanup, Show, For } from "solid-js";
import { t } from "../../i18n";
import { validateBranchName } from "../RenameBranchDialog/RenameBranchDialog";
import d from "../shared/dialog.module.css";
import s from "./CreateWorktreeDialog.module.css";

/** Options returned when the user confirms worktree creation */
export interface WorktreeCreateOptions {
  branchName: string;
  createBranch: boolean;
}

export interface CreateWorktreeDialogProps {
  visible: boolean;
  suggestedName: string;
  existingBranches: string[];
  /** Branches that already have worktrees (cannot be checked out again) */
  worktreeBranches: string[];
  onClose: () => void;
  onCreate: (options: WorktreeCreateOptions) => void;
}

export const CreateWorktreeDialog: Component<CreateWorktreeDialogProps> = (props) => {
  const [branchName, setBranchName] = createSignal("");
  const [useExisting, setUseExisting] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  let inputRef: HTMLInputElement | undefined;

  // Available existing branches: local branches that don't already have a worktree
  const availableExistingBranches = () =>
    props.existingBranches.filter((b) => !props.worktreeBranches.includes(b));

  // Reset state when dialog opens
  createEffect(() => {
    if (props.visible) {
      setBranchName(props.suggestedName);
      setUseExisting(false);
      setError(null);
      setTimeout(() => {
        if (inputRef) {
          inputRef.focus();
          inputRef.select();
        }
      }, 0);
    }
  });

  // Keyboard handling
  createEffect(() => {
    if (!props.visible) return;

    const handleKeydown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        props.onClose();
      } else if (e.key === "Enter") {
        e.preventDefault();
        handleCreate();
      }
    };

    document.addEventListener("keydown", handleKeydown);
    onCleanup(() => document.removeEventListener("keydown", handleKeydown));
  });

  const handleCreate = () => {
    const name = branchName().trim();

    if (useExisting()) {
      if (!name) {
        setError("Select a branch");
        return;
      }
      if (props.worktreeBranches.includes(name)) {
        setError("Branch already has a worktree");
        return;
      }
      props.onCreate({ branchName: name, createBranch: false });
    } else {
      const validationError = validateBranchName(name);
      if (validationError) {
        setError(validationError);
        return;
      }
      if (props.existingBranches.includes(name)) {
        setError("Branch already exists — use 'Existing branch' mode");
        return;
      }
      props.onCreate({ branchName: name, createBranch: true });
    }
  };

  const handleInputChange = (e: Event) => {
    const value = (e.target as HTMLInputElement).value;
    setBranchName(value);
    if (error()) setError(null);
  };

  const handleSelectChange = (e: Event) => {
    const value = (e.target as HTMLSelectElement).value;
    setBranchName(value);
    if (error()) setError(null);
  };

  const handleModeToggle = (existing: boolean) => {
    setUseExisting(existing);
    setError(null);
    if (existing) {
      const available = availableExistingBranches();
      setBranchName(available[0] ?? "");
    } else {
      setBranchName(props.suggestedName);
      setTimeout(() => {
        if (inputRef) {
          inputRef.focus();
          inputRef.select();
        }
      }, 0);
    }
  };

  if (!props.visible) return null;

  return (
    <div class={d.overlay} onClick={props.onClose}>
      <div class={d.popover} onClick={(e) => e.stopPropagation()}>
        <div class={d.header}>
          <span class={d.headerIcon}>+</span>
          <h4>{t("createWorktree.title", "New Worktree")}</h4>
        </div>
        <div class={d.body}>
          <div class={s.modeToggle}>
            <button
              class={`${s.modeBtn} ${!useExisting() ? s.active : ""}`}
              onClick={() => handleModeToggle(false)}
            >
              {t("createWorktree.newBranch", "New branch")}
            </button>
            <button
              class={`${s.modeBtn} ${useExisting() ? s.active : ""}`}
              onClick={() => handleModeToggle(true)}
              disabled={availableExistingBranches().length === 0}
              title={availableExistingBranches().length === 0 ? t("createWorktree.noAvailableBranches", "No branches available without worktrees") : undefined}
            >
              {t("createWorktree.existingBranch", "Existing branch")}
            </button>
          </div>

          <Show when={!useExisting()}>
            <input
              ref={inputRef}
              type="text"
              value={branchName()}
              onInput={handleInputChange}
              placeholder={t("createWorktree.branchPlaceholder", "Branch name")}
            />
          </Show>

          <Show when={useExisting()}>
            <select
              class={s.branchSelect}
              value={branchName()}
              onChange={handleSelectChange}
            >
              <For each={availableExistingBranches()}>
                {(branch) => <option value={branch}>{branch}</option>}
              </For>
            </select>
          </Show>

          {error() && <p class={d.error}>{error()}</p>}
        </div>
        <div class={d.actions}>
          <button class={d.cancelBtn} onClick={props.onClose}>
            {t("createWorktree.cancel", "Cancel")}
          </button>
          <button
            class={d.primaryBtn}
            onClick={handleCreate}
            disabled={!branchName().trim()}
          >
            {t("createWorktree.create", "Create")}
          </button>
        </div>
      </div>
    </div>
  );
};

export default CreateWorktreeDialog;
