import { Component, Show, createEffect, createSignal, onCleanup } from "solid-js";
import { useRepository } from "../../hooks/useRepository";

export interface BranchPopoverProps {
  branch: string;
  repoPath: string | null;
  onClose: () => void;
  onBranchRenamed?: (oldName: string, newName: string) => void;
}

export const BranchPopover: Component<BranchPopoverProps> = (props) => {
  const [newBranchName, setNewBranchName] = createSignal(props.branch);
  const [error, setError] = createSignal<string | null>(null);
  const [isRenaming, setIsRenaming] = createSignal(false);

  const repo = useRepository();

  // Focus input on mount
  let inputRef: HTMLInputElement | undefined;

  createEffect(() => {
    if (inputRef) {
      inputRef.focus();
      inputRef.select();
    }
  });

  // Handle Escape key
  createEffect(() => {
    const handleKeydown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        props.onClose();
      } else if (e.key === "Enter") {
        handleRename();
      }
    };

    document.addEventListener("keydown", handleKeydown);
    onCleanup(() => document.removeEventListener("keydown", handleKeydown));
  });

  const handleRename = async () => {
    const newName = newBranchName().trim();
    const oldName = props.branch;

    // Validation
    if (!newName) {
      setError("Branch name cannot be empty");
      return;
    }

    if (newName === oldName) {
      props.onClose();
      return;
    }

    // Check for invalid characters
    if (!/^[a-zA-Z0-9_\-/.]+$/.test(newName)) {
      setError("Invalid characters in branch name");
      return;
    }

    // Protected branch check
    if (oldName === "main" || oldName === "master") {
      setError("Cannot rename protected branch");
      return;
    }

    if (!props.repoPath) {
      setError("No repository selected");
      return;
    }

    setIsRenaming(true);
    setError(null);

    try {
      await repo.renameBranch(props.repoPath, oldName, newName);
      props.onBranchRenamed?.(oldName, newName);
      props.onClose();
    } catch (err) {
      setError(`Failed to rename: ${err}`);
    } finally {
      setIsRenaming(false);
    }
  };

  return (
    <div class="branch-popover-overlay" onClick={props.onClose}>
      <div class="branch-popover" onClick={(e) => e.stopPropagation()}>
        <div class="branch-popover-header">
          <span class="branch-icon">&#xe0a0;</span>
          <h4>Rename Branch</h4>
        </div>

        <div class="branch-popover-content">
          <input
            ref={inputRef}
            type="text"
            value={newBranchName()}
            onInput={(e) => {
              setNewBranchName(e.currentTarget.value);
              setError(null);
            }}
            placeholder="New branch name"
            disabled={isRenaming()}
          />

          <Show when={error()}>
            <p class="branch-popover-error">{error()}</p>
          </Show>
        </div>

        <div class="branch-popover-actions">
          <button
            class="branch-popover-cancel"
            onClick={props.onClose}
            disabled={isRenaming()}
          >
            Cancel
          </button>
          <button
            class="branch-popover-rename"
            onClick={handleRename}
            disabled={isRenaming() || !newBranchName().trim()}
          >
            {isRenaming() ? "Renaming..." : "Rename"}
          </button>
        </div>
      </div>
    </div>
  );
};

export default BranchPopover;
