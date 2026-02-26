import { Component, createSignal, createEffect, createMemo, onCleanup, Show, For } from "solid-js";
import { t } from "../../i18n";
import { validateBranchName } from "../RenameBranchDialog/RenameBranchDialog";
import d from "../shared/dialog.module.css";
import s from "./CreateWorktreeDialog.module.css";

/** Options returned when the user confirms worktree creation */
export interface WorktreeCreateOptions {
  branchName: string;
  createBranch: boolean;
  /** Base ref to create the worktree from (branch name or "HEAD") */
  baseRef: string;
}

export interface CreateWorktreeDialogProps {
  visible: boolean;
  suggestedName: string;
  existingBranches: string[];
  /** Branches that already have worktrees (cannot be checked out again) */
  worktreeBranches: string[];
  /** Base directory where worktrees are created */
  worktreesDir: string;
  /** Available base refs for the "Start from" dropdown (first is default) */
  baseRefs?: string[];
  /** Generate a random branch name */
  onGenerateName?: () => Promise<string>;
  onClose: () => void;
  onCreate: (options: WorktreeCreateOptions) => void;
}

/** Sanitize a branch name for use as a directory name (replace slashes with dashes) */
function sanitizeForPath(name: string): string {
  return name.replace(/\//g, "-");
}

/** Custom styled dropdown replacing native <select> */
const BaseRefDropdown: Component<{
  value: string;
  options: string[];
  onChange: (value: string) => void;
}> = (props) => {
  const [open, setOpen] = createSignal(false);
  let triggerRef: HTMLButtonElement | undefined;
  let listRef: HTMLDivElement | undefined;

  // Close on outside click
  const handleDocClick = (e: MouseEvent) => {
    if (!triggerRef?.contains(e.target as Node) && !listRef?.contains(e.target as Node)) {
      setOpen(false);
    }
  };

  createEffect(() => {
    if (open()) {
      document.addEventListener("mousedown", handleDocClick);
    } else {
      document.removeEventListener("mousedown", handleDocClick);
    }
    onCleanup(() => document.removeEventListener("mousedown", handleDocClick));
  });

  return (
    <div class={s.baseRefRow}>
      <label>{t("createWorktree.startFrom", "Start from")}</label>
      <div class={s.dropdownWrapper}>
        <button
          ref={triggerRef}
          type="button"
          class={s.dropdownTrigger}
          onClick={() => setOpen(!open())}
        >
          <span class={s.dropdownValue}>{props.value}</span>
          <svg class={s.dropdownChevron} width="10" height="10" viewBox="0 0 16 16" fill="currentColor">
            <path d="M4 6l4 4 4-4"/>
          </svg>
        </button>
        <Show when={open()}>
          <div ref={listRef} class={s.dropdownList}>
            <For each={props.options}>
              {(option) => (
                <div
                  class={`${s.dropdownItem} ${option === props.value ? s.dropdownItemActive : ""}`}
                  onClick={() => { props.onChange(option); setOpen(false); }}
                >
                  {option}
                </div>
              )}
            </For>
          </div>
        </Show>
      </div>
    </div>
  );
};

export const CreateWorktreeDialog: Component<CreateWorktreeDialogProps> = (props) => {
  const [branchName, setBranchName] = createSignal("");
  const [baseRef, setBaseRef] = createSignal("");
  const [error, setError] = createSignal<string | null>(null);
  let inputRef: HTMLInputElement | undefined;

  /** Available base refs — first entry is the default */
  const availableBaseRefs = () => props.baseRefs ?? [];

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
      setBaseRef(availableBaseRefs()[0] ?? "");
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
      props.onCreate({ branchName: name, createBranch: false, baseRef: baseRef() });
    } else {
      // New branch — validate name
      const validationError = validateBranchName(name);
      if (validationError) {
        setError(validationError);
        return;
      }
      props.onCreate({ branchName: name, createBranch: true, baseRef: baseRef() });
    }
  };

  const handleInputChange = (e: Event) => {
    const value = (e.target as HTMLInputElement).value;
    setBranchName(value);
    if (error()) setError(null);
  };

  const handleGenerateName = async () => {
    if (!props.onGenerateName) return;
    const name = await props.onGenerateName();
    setBranchName(name);
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
            <div class={s.inputRow}>
              <input
                ref={inputRef}
                type="text"
                value={branchName()}
                onInput={handleInputChange}
                placeholder={t("createWorktree.comboPlaceholder", "Type branch name or select existing...")}
              />
              <Show when={props.onGenerateName}>
                <button
                  class={s.generateBtn}
                  onClick={handleGenerateName}
                  title={t("createWorktree.generateName", "Generate random name")}
                  type="button"
                >
                  <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
                    <path d="M13 3.5a.5.5 0 0 1 .5.5v1a.5.5 0 0 1-.5.5h-1a.5.5 0 0 1 0-1h.2A4.5 4.5 0 0 0 8 2.05a4.5 4.5 0 0 0-4.5 4.5.5.5 0 0 1-1 0A5.5 5.5 0 0 1 8 1.05a5.5 5.5 0 0 1 5.5 3.37V4a.5.5 0 0 1 .5-.5zM3 12.5a.5.5 0 0 1-.5-.5v-1a.5.5 0 0 1 .5-.5h1a.5.5 0 0 1 0 1h-.2A4.5 4.5 0 0 0 8 13.95a4.5 4.5 0 0 0 4.5-4.5.5.5 0 0 1 1 0A5.5 5.5 0 0 1 8 14.95a5.5 5.5 0 0 1-5.5-3.37V12a.5.5 0 0 1-.5.5z"/>
                  </svg>
                </button>
              </Show>
            </div>

            <Show when={availableBaseRefs().length > 1 && !isExistingBranch()}>
              <BaseRefDropdown
                value={baseRef()}
                options={availableBaseRefs()}
                onChange={setBaseRef}
              />
            </Show>

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
