import { Component, createSignal, createEffect, createMemo, onCleanup, Show, For } from "solid-js";
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
  /** Base directory where worktrees are created */
  worktreesDir: string;
  onClose: () => void;
  onCreate: (options: WorktreeCreateOptions) => void;
}

/** Sanitize a branch name for use as a directory name (replace slashes with dashes) */
function sanitizeForPath(name: string): string {
  return name.replace(/\//g, "-");
}

export const CreateWorktreeDialog: Component<CreateWorktreeDialogProps> = (props) => {
  const [branchName, setBranchName] = createSignal("");
  const [error, setError] = createSignal<string | null>(null);
  let inputRef: HTMLInputElement | undefined;

  const trimmedName = () => branchName().trim();

  // Whether the typed name matches an existing local branch
  const isExistingBranch = createMemo(() =>
    props.existingBranches.includes(trimmedName()),
  );

  // Whether the typed name matches a branch that already has a worktree
  const hasWorktree = createMemo(() =>
    props.worktreeBranches.includes(trimmedName()),
  );

  // Filter branches by what the user has typed
  const filteredBranches = createMemo(() => {
    const query = trimmedName().toLowerCase();
    if (!query) return props.existingBranches;
    return props.existingBranches.filter((b) =>
      b.toLowerCase().includes(query),
    );
  });

  // Path preview
  const pathPreview = createMemo(() => {
    const name = trimmedName();
    if (!name || !props.worktreesDir) return "";
    const dir = props.worktreesDir.replace(/\/$/, "");
    return `${dir}/${sanitizeForPath(name)}/`;
  });

  // Reset state when dialog opens
  createEffect(() => {
    if (props.visible) {
      setBranchName("");
      setError(null);
      setTimeout(() => {
        if (inputRef) {
          inputRef.focus();
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
    const name = trimmedName();
    if (!name) return;

    // Existing branch that already has a worktree — reject
    if (hasWorktree()) {
      setError(t("createWorktree.alreadyHasWorktree", "Branch already has a worktree"));
      return;
    }

    if (isExistingBranch()) {
      // Check out existing branch into new worktree
      props.onCreate({ branchName: name, createBranch: false });
    } else {
      // New branch — validate name
      const validationError = validateBranchName(name);
      if (validationError) {
        setError(validationError);
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

  const handleBranchClick = (branch: string) => {
    // Don't allow selecting branches that already have worktrees
    if (props.worktreeBranches.includes(branch)) return;
    setBranchName(branch);
    if (error()) setError(null);
  };

  return (
    <Show when={props.visible}>
      <div class={d.overlay} onClick={props.onClose}>
        <div class={d.popover} onClick={(e) => e.stopPropagation()}>
          <div class={d.header}>
            <span class={d.headerIcon}>+</span>
            <h4>{t("createWorktree.title", "New Worktree")}</h4>
          </div>
          <div class={d.body}>
            <input
              ref={inputRef}
              type="text"
              value={branchName()}
              onInput={handleInputChange}
              placeholder={t("createWorktree.comboPlaceholder", "Type branch name or select existing...")}
            />

            <div class={s.branchList}>
              <For each={filteredBranches()}>
                {(branch) => {
                  const isDisabled = () => props.worktreeBranches.includes(branch);
                  return (
                    <div
                      class={`${s.branchItem} ${isDisabled() ? s.disabled : ""} ${trimmedName() === branch ? s.selected : ""}`}
                      onClick={() => handleBranchClick(branch)}
                    >
                      <span>{branch}</span>
                      <Show when={isDisabled()}>
                        <span class={s.worktreeTag}>{t("createWorktree.hasWorktree", "(has worktree)")}</span>
                      </Show>
                    </div>
                  );
                }}
              </For>
            </div>

            <Show when={trimmedName()}>
              <div class={s.statusLine}>
                {isExistingBranch()
                  ? t("createWorktree.statusExisting", "Will check out existing branch into new worktree")
                  : t("createWorktree.statusNew", "Will create new branch and worktree")}
              </div>
            </Show>

            <Show when={pathPreview()}>
              <div class={s.pathPreview}>{pathPreview()}</div>
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
              disabled={!trimmedName()}
            >
              {t("createWorktree.create", "Create")}
            </button>
          </div>
        </div>
      </div>
    </Show>
  );
};

export default CreateWorktreeDialog;
